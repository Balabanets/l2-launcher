//! L2 Interlude Launcher — ядро Tauri-приложения.

pub mod config;
pub mod download;
pub mod launch;
pub mod manifest;
pub mod scan;
pub mod session;
pub mod verify;

// Реэкспорт типов манифеста для интеграционных тестов и потребителей.
pub use l2_manifest;

use config::LauncherConfig;
use download::Progress;
use l2_manifest::Manifest;
use scan::ScanMode;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};
use tokio::sync::Mutex;

/// HTTP-клиент лаунчера (используется и в командах, и в интеграционных тестах).
pub fn default_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("L2Launcher/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("reqwest client")
}

/// Колбэк, который шлёт прогресс во фронтенд через событие Tauri.
fn progress_cb(app: tauri::AppHandle) -> download::ProgressCb {
    Arc::new(move |p: Progress| {
        let _ = app.emit(download::PROGRESS_EVENT, p);
    })
}

/// Глобальное состояние лаунчера.
pub struct AppState {
    config: Mutex<LauncherConfig>,
    manifest: Mutex<Option<Manifest>>,
    client: reqwest::Client,
    config_path: PathBuf,
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
    /// Битые/отсутствующие критичные файлы (если launched=false).
    pub bad: Vec<String>,
}

#[derive(Serialize)]
pub struct ScanSummary {
    pub ok: usize,
    pub missing: usize,
    pub mismatched: usize,
    pub bytes_to_download: u64,
    pub checked: usize,
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

// ---- commands ----

#[tauri::command]
async fn get_config(state: State<'_, AppState>) -> Result<LauncherConfig, String> {
    Ok(state.config.lock().await.clone())
}

#[tauri::command]
async fn save_config(state: State<'_, AppState>, config: LauncherConfig) -> Result<(), String> {
    config.save(&state.config_path).map_err(|e| e.to_string())?;
    *state.config.lock().await = config;
    Ok(())
}

/// Проверить наличие обновлений (быстро: размер+наличие, без полного хеширования).
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

/// Скачать обновление (missing + изменённые по размеру). Прогресс — события `update:progress`.
#[tauri::command]
async fn start_update(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
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
        &manifest.base_url,
        to_fetch,
        cfg.concurrency,
        progress_cb(app),
    )
    .await
    .map_err(|e| e.to_string())
}

/// Полная проверка целостности всех файлов (SHA-256) + докачка/починка.
#[tauri::command]
async fn repair(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<ScanSummary, String> {
    let manifest = cached_or_load(&state).await?;
    let cfg = state.config.lock().await.clone();
    let install = cfg.install_dir.clone();
    let m = manifest.clone();
    let diff = tokio::task::spawn_blocking(move || scan::scan_all(&install, &m, ScanMode::Hash))
        .await
        .map_err(|e| e.to_string())?;
    let summary = ScanSummary {
        ok: diff.ok,
        missing: diff.missing.len(),
        mismatched: diff.mismatched.len(),
        bytes_to_download: diff.bytes_to_download,
        checked: diff.checked,
    };
    let to_fetch = diff.to_fetch();
    if !to_fetch.is_empty() {
        download::download_all(
            &state.client,
            &cfg.install_dir,
            &manifest.base_url,
            to_fetch,
            cfg.concurrency,
            progress_cb(app),
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    Ok(summary)
}

/// Полная проверка целостности без скачивания (отчёт).
#[tauri::command]
async fn verify_files(state: State<'_, AppState>) -> Result<ScanSummary, String> {
    let manifest = cached_or_load(&state).await?;
    let install = state.config.lock().await.install_dir.clone();
    let m = manifest.clone();
    let diff = tokio::task::spawn_blocking(move || scan::scan_all(&install, &m, ScanMode::Hash))
        .await
        .map_err(|e| e.to_string())?;
    Ok(ScanSummary {
        ok: diff.ok,
        missing: diff.missing.len(),
        mismatched: diff.mismatched.len(),
        bytes_to_download: diff.bytes_to_download,
        checked: diff.checked,
    })
}

/// Запустить игру: ВСЕГДА проверяет критичные файлы. При несовпадении — не запускает.
#[tauri::command]
async fn play(state: State<'_, AppState>) -> Result<PlayResult, String> {
    let manifest = cached_or_load(&state).await?;
    let cfg = state.config.lock().await.clone();

    // Слой 1: обязательная проверка критичных файлов.
    let install = cfg.install_dir.clone();
    let m = manifest.clone();
    let report = tokio::task::spawn_blocking(move || verify::verify_critical(&install, &m))
        .await
        .map_err(|e| e.to_string())?;
    if !report.ok {
        return Ok(PlayResult { launched: false, bad: report.bad });
    }

    // Слой 2 (hook): получаем токен сессии (если backend доступен).
    let digest = verify::critical_digest(&manifest);
    let token = session::get_session(&state.client, &cfg.api_base, &manifest.version, &digest).await;

    // Запуск (только из лаунчера).
    let install = cfg.install_dir.clone();
    let m = manifest.clone();
    tokio::task::spawn_blocking(move || launch::launch_game(&install, &m, token.as_deref()))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    Ok(PlayResult { launched: true, bad: vec![] })
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
            let client = default_client();
            app.manage(AppState {
                config: Mutex::new(config),
                manifest: Mutex::new(None),
                client,
                config_path,
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
            play
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
