//! Сессия лаунчера: OAuth-вход игрока через сайт по device-code / pairing flow,
//! хранение токена, профиль игрока, игровые аккаунты и отправка баг-репорта.
//!
//! Поток входа: auth_begin → открыть verify_url в браузере → опрашивать auth_poll,
//! пока сайт не подтвердит привязку → получаем и сохраняем токен сессии лаунчера.
//! Токен идёт в Authorization: Bearer на защищённые эндпоинты.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(15);

/// Единый конверт ответа API сайта: { ok, data, error }.
#[derive(Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<String>,
}

/// Распаковать конверт: data при ok=true, иначе текст ошибки.
async fn unwrap_env<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T, String> {
    let status = resp.status();
    let env = resp.json::<Envelope<T>>().await.map_err(|e| format!("ответ сервера: {e}"))?;
    if env.ok {
        env.data.ok_or_else(|| "пустой ответ сервера".to_string())
    } else {
        Err(env.error.unwrap_or_else(|| format!("ошибка сервера (HTTP {status})")))
    }
}

// ---- хранение токена ----

/// Прочитать сохранённый токен (если есть и непустой).
pub fn load_token(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn save_token(path: &Path, token: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, token)?;
    // Ограничиваем доступ к токену на уровне ФС (Unix). На Windows файл лежит в
    // %LOCALAPPDATA% профиля пользователя — закрыт ACL по умолчанию.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn clear_token(path: &Path) {
    let _ = std::fs::remove_file(path);
}

// ---- device-code вход ----

#[derive(Serialize, Clone)]
pub struct BeginAuth {
    pub code: String,
    pub secret: String,
    pub verify_url: String,
    pub expires_in: u64,
}

#[derive(Deserialize)]
struct StartData {
    code: String,
    secret: String,
    verify_url: String,
    expires_in: u64,
}

/// Начать привязку: получить код, секрет и URL подтверждения.
pub async fn begin(client: &reqwest::Client, base: &str) -> Result<BeginAuth, String> {
    let url = format!("{}/api/launcher/auth/start", base.trim_end_matches('/'));
    let resp = client.post(&url).timeout(TIMEOUT).send().await.map_err(|e| e.to_string())?;
    let d: StartData = unwrap_env(resp).await?;
    Ok(BeginAuth { code: d.code, secret: d.secret, verify_url: d.verify_url, expires_in: d.expires_in })
}

/// Статус привязки для фронта: "pending" | "approved" | "expired".
#[derive(Serialize, Clone)]
pub struct PollResult {
    pub status: String,
    /// Токен — только при status == "approved".
    pub token: Option<String>,
}

#[derive(Deserialize)]
struct PollData {
    status: String,
    token: Option<String>,
}

/// Опросить привязку по секрету. При "approved" возвращает токен (вызывающий сохранит).
pub async fn poll(client: &reqwest::Client, base: &str, secret: &str) -> Result<PollResult, String> {
    let url = format!("{}/api/launcher/auth/poll", base.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "secret": secret }))
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let d: PollData = unwrap_env(resp).await?;
    Ok(PollResult { status: d.status, token: d.token })
}

// ---- профиль игрока ----

#[derive(Serialize, Deserialize, Clone)]
pub struct Me {
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub image: Option<String>,
}

/// Профиль текущего игрока по токену. None при невалидном/просроченном токене.
pub async fn me(client: &reqwest::Client, base: &str, token: &str) -> Option<Me> {
    let url = format!("{}/api/launcher/me", base.trim_end_matches('/'));
    let resp = client.get(&url).bearer_auth(token).timeout(TIMEOUT).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    unwrap_env::<Me>(resp).await.ok()
}

// ---- игровые аккаунты ----

#[derive(Serialize, Deserialize, Clone)]
pub struct GameAccount {
    pub login: String,
}

#[derive(Deserialize)]
struct AccountsData {
    accounts: Vec<GameAccount>,
}

pub async fn list_game_accounts(
    client: &reqwest::Client,
    base: &str,
    token: &str,
) -> Result<Vec<GameAccount>, String> {
    let url = format!("{}/api/launcher/game-accounts", base.trim_end_matches('/'));
    let resp =
        client.get(&url).bearer_auth(token).timeout(TIMEOUT).send().await.map_err(|e| e.to_string())?;
    let d: AccountsData = unwrap_env(resp).await?;
    Ok(d.accounts)
}

pub async fn create_game_account(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    login: &str,
    password: &str,
) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Created {
        login: String,
    }
    let url = format!("{}/api/launcher/game-accounts", base.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .bearer_auth(token)
        .json(&serde_json::json!({ "login": login, "password": password }))
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let d: Created = unwrap_env(resp).await?;
    Ok(d.login)
}

/// Сменить пароль игрового аккаунта (владельца — по токену).
pub async fn change_game_account_password(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    login: &str,
    password: &str,
) -> Result<(), String> {
    // login по loginSchema — алфанумерик, безопасен в пути.
    let url = format!("{}/api/launcher/game-accounts/{}/password", base.trim_end_matches('/'), login);
    let resp = client
        .post(&url)
        .bearer_auth(token)
        .json(&serde_json::json!({ "password": password }))
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let _: serde_json::Value = unwrap_env(resp).await?;
    Ok(())
}

// ---- баг-репорт ----

#[derive(Serialize, Clone)]
pub struct ReportResult {
    pub ticket_id: i64,
    pub attached: u32,
    pub rejected: u32,
}

#[derive(Deserialize)]
struct ReportData {
    #[serde(rename = "ticketId")]
    ticket_id: i64,
    attached: u32,
    rejected: u32,
}

/// MIME по расширению. None — расширение не в белом списке (файл не отправляем):
/// защита от чтения произвольных файлов с диска через IPC (.ssh, .env и т.п.).
fn mime_for(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("png") => Some("image/png"),
        Some("jpg") | Some("jpeg") => Some("image/jpeg"),
        Some("webp") => Some("image/webp"),
        Some("gif") => Some("image/gif"),
        Some("json") => Some("application/json"),
        Some("txt") | Some("log") => Some("text/plain"),
        _ => None,
    }
}

/// Отправить баг-репорт (multipart): поля тикета + версия/HWID + файлы.
#[allow(clippy::too_many_arguments)]
pub async fn submit_report(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    version: &str,
    category: &str,
    subcategory: &str,
    title: &str,
    description: &str,
    file_paths: &[String],
) -> Result<ReportResult, String> {
    let mut form = reqwest::multipart::Form::new()
        .text("version", version.to_string())
        .text("hwid", crate::session::hwid())
        .text("category", category.to_string())
        .text("subcategory", subcategory.to_string())
        .text("title", title.to_string())
        .text("description", description.to_string());

    for p in file_paths {
        let path = PathBuf::from(p);
        // Только абсолютные пути с разрешённым расширением (анти-IDOR/произвольное чтение).
        let mime = match (path.is_absolute(), mime_for(&path)) {
            (true, Some(m)) => m,
            _ => return Err(format!("недопустимый файл: {p}")),
        };
        let bytes = std::fs::read(&path).map_err(|e| format!("не читается {p}: {e}"))?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(name)
            .mime_str(mime)
            .map_err(|e| e.to_string())?;
        form = form.part("files", part);
    }

    let url = format!("{}/api/launcher/report", base.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .bearer_auth(token)
        .multipart(form)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let d: ReportData = unwrap_env(resp).await?;
    Ok(ReportResult { ticket_id: d.ticket_id, attached: d.attached, rejected: d.rejected })
}
