//! L2 Interlude Launcher — ядро Tauri-приложения.

pub mod config;
pub mod control;
pub mod download;
pub mod launch;
pub mod manifest;
pub mod progress;
pub mod scan;
pub mod session;
pub mod verify;

// Реэкспорт типов манифеста для интеграционных тестов и потребителей.
pub use l2_manifest;

use config::LauncherConfig;
use control::Control;
use l2_manifest::{FileEntry, Manifest};
use progress::{Progress, ProgressCb, PROGRESS_EVENT};
use scan::ScanMode;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tokio::sync::Mutex;

/// Глобальное состояние лаунчера.
pub struct AppState {
    config: Mutex<LauncherConfig>,
    manifest: Mutex<Option<Manifest>>,
    client: reqwest::Client,
    config_path: PathBuf,
    control: Arc<Control>,
    /// Запущен ли фоновый heartbeat авторизации (чтобы не плодить задачи).
    heartbeat: std::sync::atomic::AtomicBool,
}

/// Период переавторизации IP, пока лаунчер открыт. Держит окно авторизации живым
/// всю игровую сессию и ловит подмену критичных файлов в рантайме.
const HEARTBEAT_SECS: u64 = 300;

/// Запустить (один раз) фоновую переавторизацию: каждые HEARTBEAT_SECS пере-хешируем
/// критичные файлы и продлеваем авторизацию IP на бэкенде.
fn start_heartbeat(
    client: reqwest::Client,
    api_base: String,
    install: PathBuf,
    manifest: Manifest,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(HEARTBEAT_SECS)).await;
            let install2 = install.clone();
            let m2 = manifest.clone();
            let hashes =
                tokio::task::spawn_blocking(move || verify::critical_real_hashes(&install2, &m2))
                    .await
                    .unwrap_or_default();
            if hashes.is_empty() {
                continue; // файлы пропали/подменены — окно протухнет само
            }
            let _ = session::authorize(&client, &api_base, &manifest.version, hashes).await;
        }
    });
}

#[derive(Serialize)]
pub struct CheckResult {
    pub version: String,
    pub needs_update: bool,
    pub missing: usize,
    pub mismatched: usize,
    pub bytes_to_download: u64,
    pub files_total: usize,
}

#[derive(Serialize)]
pub struct PlayResult {
    pub launched: bool,
    pub bad: Vec<String>,
}

#[derive(Serialize)]
pub struct ScanSummary {
    pub ok: usize,
    pub missing: usize,
    pub mismatched: usize,
    pub bytes_to_download: u64,
    pub checked: usize,
    pub cancelled: bool,
}

/// HTTP-клиент лаунчера (используется и в командах, и в интеграционных тестах).
pub fn default_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("L2Launcher/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("reqwest client")
}

fn progress_cb(app: tauri::AppHandle) -> ProgressCb {
    Arc::new(move |p: Progress| {
        let _ = app.emit(PROGRESS_EVENT, p);
    })
}

/// Источники раздачи: для cas-multi — список base_urls, иначе один base_url.
fn bases_of(m: &Manifest) -> Vec<String> {
    if m.layout == "cas-multi" && !m.base_urls.is_empty() {
        m.base_urls.clone()
    } else {
        vec![m.base_url.clone()]
    }
}

// ---- helpers ----

async fn load_manifest(state: &AppState) -> Result<Manifest, String> {
    let cfg = state.config.lock().await.clone();
    let m = manifest::fetch_manifest(&state.client, &cfg.manifest_url, &cfg.signature_url())
        .await
        .map_err(|e| e.to_string())?;
    *state.manifest.lock().await = Some(m.clone());
    Ok(m)
}

async fn cached_or_load(state: &AppState) -> Result<Manifest, String> {
    if let Some(m) = state.manifest.lock().await.clone() {
        return Ok(m);
    }
    load_manifest(state).await
}

// ---- управление задачами ----

#[tauri::command]
fn pause_tasks(state: State<'_, AppState>) {
    state.control.set_paused(true);
}
#[tauri::command]
fn resume_tasks(state: State<'_, AppState>) {
    state.control.set_paused(false);
}
#[tauri::command]
fn cancel_tasks(state: State<'_, AppState>) {
    state.control.cancel();
}

// ---- команды ----

#[tauri::command]
async fn get_config(state: State<'_, AppState>) -> Result<LauncherConfig, String> {
    Ok(state.config.lock().await.clone())
}

#[tauri::command]
async fn save_config(state: State<'_, AppState>, config: LauncherConfig) -> Result<(), String> {
    config.validate()?; // отклоняем недоверенные источники / некорректные пути
    config.save(&state.config_path).map_err(|e| e.to_string())?;
    *state.config.lock().await = config;
    Ok(())
}

/// Быстрая проверка обновлений (наличие+размер).
#[tauri::command]
async fn check_update(state: State<'_, AppState>) -> Result<CheckResult, String> {
    let manifest = load_manifest(&state).await?;
    let install = state.config.lock().await.install_dir.clone();
    let m = manifest.clone();
    let diff = tokio::task::spawn_blocking(move || scan::scan_all(&install, &m, ScanMode::Quick))
        .await
        .map_err(|e| e.to_string())?;
    Ok(CheckResult {
        version: manifest.version,
        needs_update: diff.needs_update(),
        missing: diff.missing.len(),
        mismatched: diff.mismatched.len(),
        bytes_to_download: diff.bytes_to_download,
        files_total: manifest.files.len(),
    })
}

/// Скачать обновление (missing + изменённые по размеру). Прогресс — `update:progress`.
#[tauri::command]
async fn start_update(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.control.reset();
    let manifest = cached_or_load(&state).await?;
    let cfg = state.config.lock().await.clone();
    let install = cfg.install_dir.clone();
    let m = manifest.clone();
    let diff = tokio::task::spawn_blocking(move || scan::scan_all(&install, &m, ScanMode::Quick))
        .await
        .map_err(|e| e.to_string())?;
    let to_fetch = diff.to_fetch();
    if to_fetch.is_empty() {
        return Ok(());
    }
    download::download_all(
        &state.client,
        &cfg.install_dir,
        bases_of(&manifest),
        to_fetch,
        cfg.concurrency,
        manifest.layout.clone(),
        state.control.clone(),
        progress_cb(app),
    )
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// Полная проверка целостности всех файлов (SHA-256) + докачка/починка.
#[tauri::command]
async fn repair(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<ScanSummary, String> {
    state.control.reset();
    let manifest = cached_or_load(&state).await?;
    let cfg = state.config.lock().await.clone();
    let install = cfg.install_dir.clone();

    let m = manifest.clone();
    let control = state.control.clone();
    let cb = progress_cb(app.clone());
    let (diff, cancelled) = tokio::task::spawn_blocking(move || {
        let refs: Vec<&FileEntry> = m.files.iter().collect();
        scan::scan_with_progress(&install, &refs, ScanMode::Hash, control, cb)
    })
    .await
    .map_err(|e| e.to_string())?;

    let summary = ScanSummary {
        ok: diff.ok,
        missing: diff.missing.len(),
        mismatched: diff.mismatched.len(),
        bytes_to_download: diff.bytes_to_download,
        checked: diff.checked,
        cancelled,
    };
    let to_fetch = diff.to_fetch();
    if !cancelled && !to_fetch.is_empty() {
        download::download_all(
            &state.client,
            &cfg.install_dir,
            bases_of(&manifest),
            to_fetch,
            cfg.concurrency,
            manifest.layout.clone(),
            state.control.clone(),
            progress_cb(app),
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(summary)
}

/// Полная проверка целостности без скачивания.
#[tauri::command]
async fn verify_files(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<ScanSummary, String> {
    state.control.reset();
    let manifest = cached_or_load(&state).await?;
    let install = state.config.lock().await.install_dir.clone();
    let m = manifest.clone();
    let control = state.control.clone();
    let cb = progress_cb(app);
    let (diff, cancelled) = tokio::task::spawn_blocking(move || {
        let refs: Vec<&FileEntry> = m.files.iter().collect();
        scan::scan_with_progress(&install, &refs, ScanMode::Hash, control, cb)
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(ScanSummary {
        ok: diff.ok,
        missing: diff.missing.len(),
        mismatched: diff.mismatched.len(),
        bytes_to_download: diff.bytes_to_download,
        checked: diff.checked,
        cancelled,
    })
}

/// Запустить игру: ВСЕГДА проверяет критичные файлы. При несовпадении — не запускает.
#[tauri::command]
async fn play(state: State<'_, AppState>) -> Result<PlayResult, String> {
    let manifest = cached_or_load(&state).await?;
    let cfg = state.config.lock().await.clone();

    // Слой 1: проверка критичных файлов + сбор реальных хешей для авторизации.
    let install = cfg.install_dir.clone();
    let m = manifest.clone();
    let (report, real_hashes) = tokio::task::spawn_blocking(move || {
        let report = verify::verify_critical(&install, &m);
        let hashes = if report.ok { verify::critical_real_hashes(&install, &m) } else { vec![] };
        (report, hashes)
    })
    .await
    .map_err(|e| e.to_string())?;
    if !report.ok {
        return Ok(PlayResult { launched: false, bad: report.bad });
    }

    // Слой 2: авторизуем IP на бэкенде (сервер aCis проверит это при входе в мир).
    // Результат не блокирует запуск — enforcement живёт на стороне сервера.
    let _authorized =
        session::authorize(&state.client, &cfg.api_base, &manifest.version, real_hashes).await;

    // Heartbeat: пока лаунчер открыт, продлеваем авторизацию IP (и ловим подмену в рантайме).
    if !state.heartbeat.swap(true, std::sync::atomic::Ordering::SeqCst) {
        start_heartbeat(
            state.client.clone(),
            cfg.api_base.clone(),
            cfg.install_dir.clone(),
            manifest.clone(),
        );
    }

    // Финальная проверка целостности НЕПОСРЕДСТВЕННО перед запуском — сужает TOCTOU-окно
    // (между этой проверкой и spawn нет сетевых задержек).
    let install = cfg.install_dir.clone();
    let m = manifest.clone();
    let recheck = tokio::task::spawn_blocking(move || {
        let report = verify::verify_critical(&install, &m);
        if report.ok {
            launch::launch_game(&install, &m, None).map(|_| Vec::new())
        } else {
            Ok(report.bad)
        }
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    if recheck.is_empty() {
        Ok(PlayResult { launched: true, bad: vec![] })
    } else {
        Ok(PlayResult { launched: false, bad: recheck })
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let config_path = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("config.json");
            let config = LauncherConfig::load(&config_path);
            app.manage(AppState {
                config: Mutex::new(config),
                manifest: Mutex::new(None),
                client: default_client(),
                config_path,
                control: Arc::new(Control::new()),
                heartbeat: std::sync::atomic::AtomicBool::new(false),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            check_update,
            start_update,
            repair,
            verify_files,
            play,
            pause_tasks,
            resume_tasks,
            cancel_tasks
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
