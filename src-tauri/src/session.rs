//! Слой 2 (hook): отправка хеш-отчёта на backend и получение токена сессии.
//!
//! На этом этапе backend может быть недоступен/заглушкой — тогда возвращаем None,
//! и запуск продолжается с локальной проверкой (Слой 1). Когда aCis начнёт требовать
//! токен, тот же путь станет обязательным.

use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct SessionRequest<'a> {
    /// Версия клиента из манифеста.
    version: &'a str,
    /// Дайджест критичных файлов (см. verify::critical_digest).
    digest: &'a str,
}

#[derive(Deserialize)]
struct SessionResponse {
    token: String,
}

/// Запросить токен сессии. Любая ошибка/недоступность → Ok(None) (не блокируем Слой 1).
pub async fn get_session(
    client: &reqwest::Client,
    api_base: &str,
    version: &str,
    digest: &str,
) -> Option<String> {
    let url = format!("{}/api/launcher/session", api_base.trim_end_matches('/'));
    let body = SessionRequest { version, digest };
    let resp = client
        .post(&url)
        .json(&body)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let parsed: SessionResponse = resp.json().await.ok()?;
    Some(parsed.token)
}
