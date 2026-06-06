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
    files: Vec<FileHash>,
    hmac: String,
}

#[derive(Deserialize)]
struct AuthorizeResp {
    authorized: bool,
}

/// Канонический payload отчёта (должен совпадать с сервером, см. reportPayload в TS).
fn report_payload(version: &str, nonce: &str, files: &[(String, String)]) -> String {
    let mut sorted = files.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0)); // сортировка по кодовым точкам — как на сервере
    let body = sorted
        .iter()
        .map(|(p, h)| format!("{p}={h}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{version}\n{nonce}\n{body}")
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
) -> bool {
    let base = api_base.trim_end_matches('/');

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

    // 2. HMAC + authorize
    let hmac = hmac_hex(&report_payload(version, &nonce, &files));
    let req = AuthorizeReq {
        version: version.to_string(),
        nonce,
        files: files.into_iter().map(|(path, sha256)| FileHash { path, sha256 }).collect(),
        hmac,
    };
    match client
        .post(format!("{base}/api/launcher/authorize"))
        .json(&req)
        .timeout(std::time::Duration::from_secs(12))
        .send()
        .await
    {
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
        let payload = report_payload("t", "abc", &files);
        assert_eq!(payload, "t\nabc\nsystem/a.dll=aa\nsystem/b.exe=bb");
        assert_eq!(
            hmac_hex(&payload),
            "0638fdfb3b14ed95622f01f9ffdc991ef7a78017069801f0d148cf6fd867979e"
        );
    }
}
