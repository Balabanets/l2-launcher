//! Слой 2 (клиентская часть): авторизация IP игрока на бэкенде.
//!
//! Поток: GET /api/launcher/challenge → nonce; считаем HMAC отчёта (реальные хеши
//! критичных файлов) с общим секретом; POST /api/launcher/authorize. Сервер сверяет
//! хеши с эталонным манифестом и авторизует IP. Игровой сервер aCis при логине
//! проверяет авторизацию IP (см. /api/launcher/check).
//!
//! Любая ошибка/недоступность → Ok(false): запуск не блокируем (enforcement на сервере),
//! но пользователь будет не авторизован, и aCis его не пустит (политика fail-closed на aCis).

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

/// Общий секрет с бэкендом. Подставляется при сборке (CI secret); для локальной
/// разработки — небезопасное значение по умолчанию (совпадает с бэкендом).
const HMAC_SECRET: &str = match option_env!("LAUNCHER_HMAC_SECRET") {
    Some(s) => s,
    None => "dev-insecure-secret-change-me",
};

#[derive(Deserialize)]
struct Challenge {
    nonce: String,
}

#[derive(Serialize)]
struct FileHash {
    path: String,
    sha256: String,
}

#[derive(Serialize)]
struct AuthorizeReq {
    version: String,
    nonce: String,
    hwid: String,
    files: Vec<FileHash>,
    hmac: String,
}

#[derive(Deserialize)]
struct AuthorizeResp {
    authorized: bool,
}

/// Лимит одновременных окон с бэкенда (админ может менять). При недоступности —
/// fallback (из конфига лаунчера). Эндпоинт: GET /api/launcher/limits → {max_clients}.
pub async fn fetch_max_clients(client: &reqwest::Client, api_base: &str, fallback: usize) -> usize {
    #[derive(Deserialize)]
    struct Limits {
        max_clients: usize,
    }
    let url = format!("{}/api/launcher/limits", api_base.trim_end_matches('/'));
    match client.get(&url).timeout(std::time::Duration::from_secs(6)).send().await {
        Ok(r) if r.status().is_success() => {
            r.json::<Limits>().await.map(|l| l.max_clients.max(1)).unwrap_or(fallback)
        }
        _ => fallback,
    }
}

/// Стабильный идентификатор железа (для аппаратных банов и счёта окон по ПК).
/// Windows: SHA-256 от MachineGuid. Dev: от machine-id/hostname.
pub fn hwid() -> String {
    #[cfg(windows)]
    let raw = {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        use winreg::RegKey;
        RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SOFTWARE\Microsoft\Cryptography")
            .and_then(|k| k.get_value::<String, _>("MachineGuid"))
            .unwrap_or_default()
    };
    #[cfg(not(windows))]
    let raw = std::fs::read_to_string("/etc/machine-id")
        .ok()
        .map(|s| s.trim().to_string())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_else(|| "dev".into());
    l2_manifest::hash_bytes(format!("l2hwid:{raw}").as_bytes())
}

/// Канонический payload отчёта (должен совпадать с сервером, см. reportPayload в TS).
fn report_payload(version: &str, nonce: &str, hwid: &str, files: &[(String, String)]) -> String {
    let mut sorted = files.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0)); // сортировка по кодовым точкам — как на сервере
    let body = sorted
        .iter()
        .map(|(p, h)| format!("{p}={h}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{version}\n{nonce}\n{hwid}\n{body}")
}

fn hmac_hex(payload: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(HMAC_SECRET.as_bytes()).expect("hmac key");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Авторизовать IP игрока. files — реальные (path, sha256) критичных файлов с диска.
pub async fn authorize(
    client: &reqwest::Client,
    api_base: &str,
    version: &str,
    files: Vec<(String, String)>,
    token: Option<&str>,
) -> bool {
    let base = api_base.trim_end_matches('/');
    let hw = hwid();

    // 1. challenge
    let nonce = match client
        .get(format!("{base}/api/launcher/challenge"))
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .ok()
        .and_then(|r| if r.status().is_success() { Some(r) } else { None })
    {
        Some(r) => match r.json::<Challenge>().await {
            Ok(c) => c.nonce,
            Err(_) => return false,
        },
        None => return false,
    };

    // 2. HMAC (включает HWID — подделать «на лету» нельзя) + authorize
    let hmac = hmac_hex(&report_payload(version, &nonce, &hw, &files));
    let req = AuthorizeReq {
        version: version.to_string(),
        nonce,
        hwid: hw,
        files: files.into_iter().map(|(path, sha256)| FileHash { path, sha256 }).collect(),
        hmac,
    };
    let mut rb = client
        .post(format!("{base}/api/launcher/authorize"))
        .json(&req)
        .timeout(std::time::Duration::from_secs(12));
    // Токен сессии лаунчера: при включённом на сервере LAUNCHER_REQUIRE_LOGIN
    // авторизация IP пройдёт только для вошедшего игрока (зубы гейта входа).
    if let Some(t) = token {
        rb = rb.bearer_auth(t);
    }
    match rb.send().await {
        Ok(r) if r.status().is_success() => {
            r.json::<AuthorizeResp>().await.map(|a| a.authorized).unwrap_or(false)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Кросс-проверка с TS-бэкендом (reportPayload + computeHmac, секрет по умолчанию):
    // эталон вычислен node-скриптом. Если формат/секрет разойдутся — тест упадёт.
    #[test]
    fn payload_and_hmac_match_backend() {
        let files = vec![
            ("system/b.exe".to_string(), "bb".to_string()),
            ("system/a.dll".to_string(), "aa".to_string()),
        ];
        let payload = report_payload("t", "abc", "HW123", &files);
        assert_eq!(payload, "t\nabc\nHW123\nsystem/a.dll=aa\nsystem/b.exe=bb");
        assert_eq!(
            hmac_hex(&payload),
            "d40126d22984e6460c377628675ed9f931798e20e8b62d3b863d5d2e1a241cf4"
        );
    }
}
