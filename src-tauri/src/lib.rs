//! L2 Interlude Launcher — ядро Tauri-приложения.

pub mod auth;
pub mod client_settings;
pub mod config;
pub mod control;
pub mod defender;
pub mod download;
pub mod install;
pub mod launch;
pub mod manifest;
pub mod progress;
pub mod sac;
pub mod scan;
pub mod selfupdate;
pub mod session;
pub mod sync;
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
    /// Токен сессии лаунчера (OAuth-вход игрока). None = не вошёл. Arc — чтобы фоновый
    /// heartbeat читал актуальный токен и прекращал авторизацию IP после выхода.
    session: Arc<Mutex<Option<String>>>,
    /// Файл с токеном сессии (рядом с config.json).
    session_path: PathBuf,
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
    session: Arc<Mutex<Option<String>>>,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(HEARTBEAT_SECS)).await;
            // Читаем актуальный токен: после выхода (logout) он None → авторизацию IP
            // больше не продлеваем, окно протухает.
            let token = session.lock().await.clone();
            let Some(token) = token else {
                continue;
            };
            let install2 = install.clone();
            let m2 = manifest.clone();
            let hashes =
                tokio::task::spawn_blocking(move || verify::critical_real_hashes(&install2, &m2))
                    .await
                    .unwrap_or_default();
            if hashes.is_empty() {
                continue; // файлы пропали/подменены — окно протухнет само
            }
            let _ = session::authorize(
                &client,
                &api_base,
                &manifest.version,
                hashes,
                Some(token.as_str()),
            )
            .await;
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

#[derive(Serialize)]
pub struct ServerInfoOut {
    pub id: String,
    pub name: String,
    pub online: bool,
    pub players: u32,
    pub max: u32,
    pub started_at: u64,
}

/// Живой статус всех серверов (через backend; CSP не даёт фронту ходить наружу напрямую).
#[tauri::command]
async fn server_status(state: State<'_, AppState>) -> Result<Vec<ServerInfoOut>, String> {
    let api = state.config.lock().await.api_base.clone();
    let url = format!("{}/api/status", api.trim_end_matches('/'));
    let resp = state
        .client
        .get(&url)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("status HTTP {}", resp.status()));
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let servers = v.get("servers").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    Ok(servers
        .iter()
        .map(|s| ServerInfoOut {
            id: s.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            name: s.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            online: s.get("online").and_then(|x| x.as_bool()).unwrap_or(false),
            players: s.get("players").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
            max: s.get("max").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
            started_at: s.get("startedAt").and_then(|x| x.as_u64()).unwrap_or(0),
        })
        .collect())
}

#[tauri::command]
async fn save_config(state: State<'_, AppState>, config: LauncherConfig) -> Result<(), String> {
    config.validate()?; // отклоняем недоверенные источники / некорректные пути
    config.save(&state.config_path).map_err(|e| e.to_string())?;
    *state.config.lock().await = config;
    Ok(())
}

/// Быстрая проверка обновлений: наличие + размер крупных ассетов и SHA-256 мелких
/// файлов (конфиги/данные меняются с сохранением размера — ловим их по хешу).
#[tauri::command]
async fn check_update(state: State<'_, AppState>) -> Result<CheckResult, String> {
    let manifest = load_manifest(&state).await?;
    let cfg = state.config.lock().await.clone();
    let install = cfg.install_dir.clone();
    let m = manifest.clone();
    let files_total = manifest.files.len();
    let lang = cfg.language.clone();
    let (missing, mismatched, bytes, seed) = tokio::task::spawn_blocking(move || {
        // Хэш-синк только по managed + активным языковым; seed/launcher-owned — лишь отсутствующие.
        let refs = sync::sync_refs(&m, &install, &lang);
        let diff = scan::scan(&install, &refs, ScanMode::Quick);
        let seed = sync::missing_seed(&m, &install);
        let seed_bytes: u64 = seed.iter().map(|f| f.size).sum();
        (diff.missing.len(), diff.mismatched.len(), diff.bytes_to_download + seed_bytes, seed.len())
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(CheckResult {
        version: manifest.version,
        needs_update: missing > 0 || mismatched > 0 || seed > 0,
        missing: missing + seed,
        mismatched,
        bytes_to_download: bytes,
        files_total,
    })
}

/// Скачать обновление (missing + изменённые по размеру). Прогресс — `update:progress`.
/// Однократно (запоминаем в конфиге) добавить папку установки в исключения Windows
/// Defender — лечит ложное срабатывание эвристики на неподписанном L2.exe. Вызывается
/// ДО скачивания, чтобы Defender не отправил свежие файлы в карантин. Один UAC-промпт
/// за всё время; при отказе UAC флаг не ставится → попытка повторится в след. раз.
/// Тихо и best-effort: ошибка/отказ не должны мешать обновлению.
async fn ensure_defender_exclusion_once(state: &State<'_, AppState>) {
    let (install, done) = {
        let cfg = state.config.lock().await;
        (cfg.install_dir.clone(), cfg.defender_excluded)
    };
    if done {
        return;
    }
    let ok = tokio::task::spawn_blocking(move || defender::ensure_exclusion(&install).is_ok())
        .await
        .unwrap_or(false);
    if ok {
        let mut cfg = state.config.lock().await;
        cfg.defender_excluded = true;
        let _ = cfg.save(&state.config_path);
    }
}

#[tauri::command]
async fn start_update(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.control.reset();
    let manifest = cached_or_load(&state).await?;
    // До скачивания: добавить папку в исключения Defender (один раз), иначе свежий
    // L2.exe может попасть в карантин прямо во время загрузки → цикл «недостающий файл».
    ensure_defender_exclusion_once(&state).await;
    let cfg = state.config.lock().await.clone();
    let install = cfg.install_dir.clone();
    let m = manifest.clone();
    let install_scan = install.clone();
    let lang = cfg.language.clone();
    let to_fetch = tokio::task::spawn_blocking(move || {
        let refs = sync::sync_refs(&m, &install_scan, &lang);
        let mut f = scan::scan(&install_scan, &refs, ScanMode::Quick).to_fetch();
        f.extend(sync::missing_seed(&m, &install_scan)); // дописать недостающие дефолты
        f
    })
    .await
    .map_err(|e| e.to_string())?;

    if !to_fetch.is_empty() {
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
    // Удалить устаревшие файлы (GameGuard и т.п.) согласно манифесту.
    let install_del = install.clone();
    let m_del = manifest.clone();
    tokio::task::spawn_blocking(move || sync::apply_deletions(&install_del, &m_del))
        .await
        .map_err(|e| e.to_string())?;
    // Применить выбор игрока (perf/язык) + засеять WindowsInfo — тихо, best-effort.
    let install_re = install.clone();
    let (perf, lang2) = (cfg.performance, cfg.language.clone());
    tokio::task::spawn_blocking(move || client_settings::apply(&install_re, perf, &lang2))
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Полная проверка целостности всех файлов (SHA-256) + докачка/починка.
#[tauri::command]
async fn repair(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<ScanSummary, String> {
    state.control.reset();
    let manifest = cached_or_load(&state).await?;
    // Если Defender уже отправил L2.exe в карантин — добавляем исключение до починки,
    // чтобы докачанный файл не удалили снова.
    ensure_defender_exclusion_once(&state).await;
    let cfg = state.config.lock().await.clone();
    let install = cfg.install_dir.clone();

    let m = manifest.clone();
    let install_scan = install.clone();
    let control = state.control.clone();
    let cb = progress_cb(app.clone());
    let lang = cfg.language.clone();
    let (diff, cancelled, seed) = tokio::task::spawn_blocking(move || {
        let refs = sync::sync_refs(&m, &install_scan, &lang);
        let (diff, cancelled) = scan::scan_with_progress(&install_scan, &refs, ScanMode::Hash, control, cb);
        let seed = sync::missing_seed(&m, &install_scan);
        (diff, cancelled, seed)
    })
    .await
    .map_err(|e| e.to_string())?;

    let summary = ScanSummary {
        ok: diff.ok,
        missing: diff.missing.len() + seed.len(),
        mismatched: diff.mismatched.len(),
        bytes_to_download: diff.bytes_to_download,
        checked: diff.checked,
        cancelled,
    };
    let mut to_fetch = diff.to_fetch();
    to_fetch.extend(seed);
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
    if !cancelled {
        let install_del = install.clone();
        let m_del = manifest.clone();
        tokio::task::spawn_blocking(move || sync::apply_deletions(&install_del, &m_del))
            .await
            .map_err(|e| e.to_string())?;
        let install_re = install.clone();
        let (perf, lang2) = (cfg.performance, cfg.language.clone());
        tokio::task::spawn_blocking(move || client_settings::apply(&install_re, perf, &lang2))
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
    let cfg = state.config.lock().await.clone();
    let install = cfg.install_dir.clone();
    let m = manifest.clone();
    let control = state.control.clone();
    let cb = progress_cb(app);
    let lang = cfg.language.clone();
    let (diff, cancelled) = tokio::task::spawn_blocking(move || {
        let refs = sync::sync_refs(&m, &install, &lang);
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

/// Запустить игру: проверяет критичные файлы (с прогрессом). При несовпадении — не запускает.
#[tauri::command]
async fn play(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<PlayResult, String> {
    state.control.reset();

    // Обязательное обновление лаунчера: если доступна новая версия — игру не запускаем.
    // Новый клиент/протокол/правила могут требовать свежий лаунчер, поэтому запуск
    // блокируется до самообновления. Проверка живая (на момент нажатия «Играть»), её
    // нельзя обойти из фронта. Fail-open: если проверить не удалось (оффлайн/ошибка) —
    // не блокируем, чтобы сетевой сбой не лишал игры.
    if let Ok(Some(rel)) = selfupdate::check(&state.client).await {
        return Err(format!(
            "Сначала обновите лаунчер до версии {}. Запуск игры заблокирован до обновления лаунчера.",
            rel.version
        ));
    }

    // Smart App Control (Windows 11) принудительно блокирует неподписанный L2.exe и
    // его DLL на этапе загрузки. Исключения/подпись здесь не помогают — даём понятную
    // ошибку (фронт показывает гид по выключению SAC) вместо криптового системного окна.
    if sac::is_blocking(sac::state()) {
        return Err(
            "Smart App Control блокирует запуск игры. Откройте «Безопасность Windows» → \
             «Управление приложениями и браузером» → Smart App Control и выключите его, \
             затем нажмите «Играть» снова."
                .to_string(),
        );
    }

    let cfg = state.config.lock().await.clone();

    // Гейт входа: играть можно только войдя через сайт (OAuth) И имея игровой аккаунт.
    // Токен валидируем на бэкенде (me); при невалидном — чистим и просим войти заново.
    let token = state.session.lock().await.clone();
    let token = match token {
        Some(t) if auth::me(&state.client, &cfg.api_base, &t).await.is_some() => t,
        _ => {
            // Токен отсутствует или протух — сбросим локально.
            *state.session.lock().await = None;
            auth::clear_token(&state.session_path);
            return Err("Войдите в лаунчер (через сайт), чтобы играть.".to_string());
        }
    };
    match auth::list_game_accounts(&state.client, &cfg.api_base, &token).await {
        Ok(accs) if !accs.is_empty() => {}
        Ok(_) => {
            return Err("Создайте игровой аккаунт, чтобы играть (кнопка «Игровой аккаунт»).".to_string());
        }
        Err(e) => return Err(format!("Не удалось проверить игровой аккаунт: {e}")),
    }

    let manifest = cached_or_load(&state).await?;

    // Анти-мультибокс: не запускать больше лимита окон. Лимит — с бэкенда (админ),
    // fallback — из конфига. Подсчёт запущенных l2.exe без окна.
    let max = session::fetch_max_clients(&state.client, &cfg.api_base, cfg.max_clients).await;
    let running = tokio::task::spawn_blocking(|| launch::running_count("l2.exe"))
        .await
        .map_err(|e| e.to_string())?;
    if running >= max {
        return Err(format!(
            "Открыто максимум окон ({max}). Закройте лишние клиенты, чтобы запустить ещё."
        ));
    }

    // Слой 1: проверка критичных файлов + сбор реальных хешей ЗА ОДИН проход, с прогрессом.
    let install = cfg.install_dir.clone();
    let m = manifest.clone();
    let control = state.control.clone();
    let cb = progress_cb(app);
    let (report, real_hashes) = tokio::task::spawn_blocking(move || {
        verify::verify_and_hash_critical(&install, &m, control, cb)
    })
    .await
    .map_err(|e| e.to_string())?;
    if !report.ok {
        return Ok(PlayResult { launched: false, bad: report.bad });
    }

    // Слой 2: авторизуем IP на бэкенде (сервер aCis проверит это при входе в мир).
    let _authorized = session::authorize(
        &state.client,
        &cfg.api_base,
        &manifest.version,
        real_hashes,
        Some(token.as_str()),
    )
    .await;

    // Heartbeat: пока лаунчер открыт, продлеваем авторизацию IP (и ловим подмену в рантайме).
    if !state.heartbeat.swap(true, std::sync::atomic::Ordering::SeqCst) {
        start_heartbeat(
            state.client.clone(),
            cfg.api_base.clone(),
            cfg.install_dir.clone(),
            manifest.clone(),
            state.session.clone(),
        );
    }

    // Применить выбранные настройки клиента (perf/язык) перед запуском — тихо, best-effort.
    let install_ap = cfg.install_dir.clone();
    let (perf, lang_ap) = (cfg.performance, cfg.language.clone());
    tokio::task::spawn_blocking(move || client_settings::apply(&install_ap, perf, &lang_ap))
        .await
        .map_err(|e| e.to_string())?;

    // Запуск (критичные файлы только что проверены — повторно не хешируем).
    let install = cfg.install_dir.clone();
    let m = manifest.clone();
    tokio::task::spawn_blocking(move || launch::launch_game(&install, &m, None))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

    Ok(PlayResult { launched: true, bad: vec![] })
}

/// Состояние Smart App Control: "off" | "on" | "evaluation" | "unknown".
#[tauri::command]
fn sac_status() -> sac::SacState {
    sac::state()
}

/// Сводка состояния для панели «Состояние»: защита, целостность, версии.
#[derive(Serialize)]
struct Diagnostics {
    /// Версия лаунчера (вшита на сборке).
    launcher_version: String,
    /// Версия клиента из манифеста ("—", если манифест недоступен).
    client_version: String,
    /// Подпись манифеста: Some(true) — манифест загружен и подпись валидна;
    /// None — не удалось загрузить/проверить (оффлайн).
    manifest_signature_ok: Option<bool>,
    /// Найден ли исполняемый файл игры по пути установки.
    exe_present: bool,
    /// Состояние Smart App Control.
    sac: sac::SacState,
    /// Добавлена ли папка игры в исключения Defender.
    defender_excluded: bool,
    /// Путь установки клиента.
    install_dir: String,
}

#[tauri::command]
async fn diagnostics(state: State<'_, AppState>) -> Result<Diagnostics, String> {
    let cfg = state.config.lock().await.clone();
    // Манифест грузится только при валидной подписи (verify внутри load), поэтому
    // успешная загрузка == подпись валидна. Ошибка трактуется как «не проверено».
    let manifest = cached_or_load(state.inner()).await.ok();
    let (client_version, exe_present, sig_ok) = match &manifest {
        Some(m) => {
            let exe_present = l2_manifest::safe_join(&cfg.install_dir, &m.launch.exe)
                .map(|p| p.is_file())
                .unwrap_or(false);
            (m.version.clone(), exe_present, Some(true))
        }
        None => ("—".to_string(), false, None),
    };
    Ok(Diagnostics {
        launcher_version: selfupdate::current_version().to_string(),
        client_version,
        manifest_signature_ok: sig_ok,
        exe_present,
        sac: sac::state(),
        defender_excluded: cfg.defender_excluded,
        install_dir: cfg.install_dir.display().to_string(),
    })
}

/// Открыть страницу настроек Windows с переключателем Smart App Control.
#[tauri::command]
fn open_sac_settings() -> Result<(), String> {
    sac::open_settings().map_err(|e| e.to_string())
}

/// Проверить, есть ли новая версия лаунчера (портативное самообновление).
/// Возвращает None, если установлена актуальная версия.
#[tauri::command]
async fn check_self_update(
    state: State<'_, AppState>,
) -> Result<Option<selfupdate::SelfUpdateInfo>, String> {
    match selfupdate::check(&state.client).await {
        Ok(Some(rel)) => Ok(Some(selfupdate::SelfUpdateInfo {
            version: rel.version,
            current: selfupdate::current_version().to_string(),
        })),
        Ok(None) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Скачать, проверить (SHA-256 + подпись) и заменить exe на месте, затем перезапуститься.
#[tauri::command]
async fn apply_self_update(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let rel = selfupdate::check(&state.client)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "обновление недоступно".to_string())?;
    selfupdate::apply(&state.client, &rel, progress_cb(app.clone()))
        .await
        .map_err(|e| e.to_string())?;
    // exe заменён — перезапускаемся на новую версию (не возвращается).
    app.restart();
}

/// Текущие настройки клиента (режим производительности + язык).
#[tauri::command]
async fn get_client_settings(
    state: State<'_, AppState>,
) -> Result<client_settings::ClientSettings, String> {
    let cfg = state.config.lock().await;
    Ok(client_settings::ClientSettings {
        performance: cfg.performance,
        language: cfg.language.clone(),
    })
}

/// Включить/выключить режим производительности. Это НАСТРОЙКА: пишем в конфиг
/// мгновенно (без ошибок/окон) и тихо применяем к клиенту, если файлы уже на месте;
/// иначе применится при следующем запуске/обновлении.
#[tauri::command]
async fn set_performance_mode(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let (install, lang) = {
        let mut cfg = state.config.lock().await;
        cfg.performance = enabled;
        cfg.save(&state.config_path).map_err(|e| e.to_string())?;
        (cfg.install_dir.clone(), cfg.language.clone())
    };
    tokio::task::spawn_blocking(move || client_settings::apply(&install, enabled, &lang))
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Сменить язык клиента RU/EN. НАСТРОЙКА: пишем в конфиг мгновенно. Если выбран EN и
/// пака ещё нет — докачиваем lang-en (с прогрессом). Затем тихо применяем.
#[tauri::command]
async fn set_client_language(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    lang: String,
) -> Result<(), String> {
    let (install, concurrency, perf) = {
        let mut cfg = state.config.lock().await;
        cfg.language = lang.clone();
        cfg.save(&state.config_path).map_err(|e| e.to_string())?;
        (cfg.install_dir.clone(), cfg.concurrency, cfg.performance)
    };

    // EN: докачать языковой набор, если его ещё нет (best-effort, не блокируем выбор).
    if lang == "en" && !sync::en_downloaded(&install) {
        state.control.reset();
        if let Ok(manifest) = cached_or_load(&state).await {
            let files: Vec<FileEntry> =
                manifest.lang_group_files("lang-en").into_iter().cloned().collect();
            if !files.is_empty() {
                let _ = download::download_all(
                    &state.client,
                    &install,
                    bases_of(&manifest),
                    files,
                    concurrency,
                    manifest.layout.clone(),
                    state.control.clone(),
                    progress_cb(app),
                )
                .await;
            }
        }
    }

    let install2 = install.clone();
    tokio::task::spawn_blocking(move || client_settings::apply(&install2, perf, &lang))
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---- OAuth-вход игрока, игровые аккаунты, баг-репорт ----

/// Начать вход: получить код и URL подтверждения (фронт откроет его в браузере).
#[tauri::command]
async fn auth_begin(state: State<'_, AppState>) -> Result<auth::BeginAuth, String> {
    let api_base = state.config.lock().await.api_base.clone();
    auth::begin(&state.client, &api_base).await
}

/// Опросить статус привязки по секрету. При «approved» сохраняет токен.
#[tauri::command]
async fn auth_poll(state: State<'_, AppState>, secret: String) -> Result<auth::PollResult, String> {
    let api_base = state.config.lock().await.api_base.clone();
    let mut res = auth::poll(&state.client, &api_base, &secret).await?;
    if res.status == "approved" {
        if let Some(tok) = res.token.take() {
            auth::save_token(&state.session_path, &tok).map_err(|e| e.to_string())?;
            *state.session.lock().await = Some(tok);
        }
    }
    // Токен во фронт не отдаём — он остаётся только в Rust (res.token уже None).
    Ok(res)
}

/// Выйти: удалить токен локально.
#[tauri::command]
async fn auth_logout(state: State<'_, AppState>) -> Result<(), String> {
    *state.session.lock().await = None;
    auth::clear_token(&state.session_path);
    Ok(())
}

/// Текущий игрок (по сохранённому токену) или None.
#[tauri::command]
async fn auth_me(state: State<'_, AppState>) -> Result<Option<auth::Me>, String> {
    let api_base = state.config.lock().await.api_base.clone();
    let token = state.session.lock().await.clone();
    match token {
        Some(t) => Ok(auth::me(&state.client, &api_base, &t).await),
        None => Ok(None),
    }
}

/// Список игровых аккаунтов игрока (требует входа).
#[tauri::command]
async fn list_game_accounts(state: State<'_, AppState>) -> Result<Vec<auth::GameAccount>, String> {
    let api_base = state.config.lock().await.api_base.clone();
    let token = state.session.lock().await.clone().ok_or("Сначала войдите в лаунчер")?;
    auth::list_game_accounts(&state.client, &api_base, &token).await
}

/// Создать игровой аккаунт (login/password) — требует входа.
#[tauri::command]
async fn create_game_account(
    state: State<'_, AppState>,
    login: String,
    password: String,
) -> Result<String, String> {
    let api_base = state.config.lock().await.api_base.clone();
    let token = state.session.lock().await.clone().ok_or("Сначала войдите в лаунчер")?;
    auth::create_game_account(&state.client, &api_base, &token, &login, &password).await
}

/// Отправить баг-репорт (тикет от имени игрока) с вложениями.
#[tauri::command]
async fn submit_bug_report(
    state: State<'_, AppState>,
    category: String,
    subcategory: String,
    title: String,
    description: String,
    files: Vec<String>,
) -> Result<auth::ReportResult, String> {
    let api_base = state.config.lock().await.api_base.clone();
    let token = state.session.lock().await.clone().ok_or("Сначала войдите в лаунчер")?;
    let version = selfupdate::current_version();
    auth::submit_report(
        &state.client,
        &api_base,
        &token,
        version,
        &category,
        &subcategory,
        &title,
        &description,
        &files,
    )
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Portable: при первом запуске встать в стабильный «дом» (%LOCALAPPDATA%) и
    // создать ярлыки. Если уже дома — освежить ярлыки. На не-Windows — no-op.
    install::ensure_installed();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let config_path = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("config.json");
            let config = LauncherConfig::load(&config_path);
            let session_path = config_path
                .parent()
                .map(|p| p.join("session.json"))
                .unwrap_or_else(|| PathBuf::from("session.json"));
            let token = auth::load_token(&session_path);
            app.manage(AppState {
                config: Mutex::new(config),
                manifest: Mutex::new(None),
                client: default_client(),
                config_path,
                control: Arc::new(Control::new()),
                heartbeat: std::sync::atomic::AtomicBool::new(false),
                session: Arc::new(Mutex::new(token)),
                session_path,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            server_status,
            check_update,
            start_update,
            repair,
            verify_files,
            play,
            pause_tasks,
            resume_tasks,
            cancel_tasks,
            check_self_update,
            apply_self_update,
            sac_status,
            open_sac_settings,
            diagnostics,
            auth_begin,
            auth_poll,
            auth_logout,
            auth_me,
            list_game_accounts,
            create_game_account,
            submit_bug_report,
            get_client_settings,
            set_performance_mode,
            set_client_language
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
