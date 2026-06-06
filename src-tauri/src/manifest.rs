//! Загрузка и проверка подписи манифеста.
//!
//! Публичный ключ ВШИТ в бинарь — манифест без валидной подписи отвергается,
//! поэтому подменить манифест на сервере раздачи нельзя.

use anyhow::{bail, Context, Result};
use l2_manifest::Manifest;

/// Публичный ключ Ed25519 для проверки подписи манифеста.
/// Сгенерирован `keygen`; приватная пара хранится в секрете на сервере.
pub const MANIFEST_PUBKEY: [u8; 32] = [
    121, 207, 207, 60, 223, 89, 240, 77, 158, 105, 27, 215, 33, 57, 188, 44, 165, 249, 184, 204,
    107, 208, 253, 182, 27, 83, 250, 60, 81, 215, 18, 100,
];

/// Скачать манифест и его подпись, проверить подпись, распарсить.
pub async fn fetch_manifest(client: &reqwest::Client, manifest_url: &str, sig_url: &str) -> Result<Manifest> {
    let raw = client
        .get(manifest_url)
        .send()
        .await
        .context("не удалось скачать манифест")?
        .error_for_status()
        .context("манифест: HTTP-ошибка")?
        .bytes()
        .await?;

    let sig = client
        .get(sig_url)
        .send()
        .await
        .context("не удалось скачать подпись манифеста")?
        .error_for_status()
        .context("подпись: HTTP-ошибка")?
        .text()
        .await?;

    // Критично: проверяем подпись по СЫРЫМ байтам ровно так, как их подписал генератор.
    if l2_manifest::verify(&MANIFEST_PUBKEY, &raw, &sig).is_err() {
        bail!("подпись манифеста недействительна — файл повреждён или подменён");
    }

    let manifest: Manifest = serde_json::from_slice(&raw).context("манифест: неверный JSON")?;
    Ok(manifest)
}
