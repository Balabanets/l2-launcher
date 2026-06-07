//! Самообновление портативного лаунчера: «скачал .exe → запустил → если есть
//! обновление, кнопка → скачалось, проверилось, заменилось на месте → перезапуск».
//!
//! Безопасность (тот же корень доверия, что у манифеста):
//!  1. `launcher.json` подписан Ed25519 нашим ключом и проверяется ВШИТЫМ
//!     публичным ключом (`MANIFEST_PUBKEY`) — подменить метаданные обновления
//!     на сервере раздачи нельзя.
//!  2. Скачанный exe сверяется по SHA-256 из подписанного `launcher.json` ДО
//!     любой замены. Не сошлось — временный файл удаляется, текущий exe не трогаем.
//!  3. Замена «на месте» через обкатанный `self-replace` (корректно решает
//!     «нельзя удалить запущенный exe» на Windows). Установщик/UAC не нужны.

use crate::config::is_allowed_url;
use crate::manifest::MANIFEST_PUBKEY;
use crate::progress::{ProgressCb, Shared};
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Подписанные метаданные релиза лаунчера. Лежат рядом с портативным exe в
/// GitHub-релизе и доступны по алиасу `latest`.
const RELEASE_JSON_URL: &str =
    "https://github.com/Balabanets/l2-launcher/releases/latest/download/launcher.json";
const RELEASE_SIG_URL: &str =
    "https://github.com/Balabanets/l2-launcher/releases/latest/download/launcher.json.sig";

#[derive(Debug, Clone, Deserialize)]
pub struct LauncherRelease {
    /// Версия в формате "major.minor.patch".
    pub version: String,
    /// Прямая ссылка на портативный exe этого релиза (host из ALLOWED_HOSTS).
    pub exe_url: String,
    /// SHA-256 портативного exe в hex (нижний регистр).
    pub sha256: String,
    /// Размер exe в байтах (для прогресса).
    #[serde(default)]
    pub size: u64,
}

/// То, что отдаём фронту: доступна ли новая версия.
#[derive(Debug, Clone, Serialize)]
pub struct SelfUpdateInfo {
    pub version: String,
    pub current: String,
}

fn parse_semver(v: &str) -> (u64, u64, u64) {
    let mut it = v.trim().split('.').map(|p| p.trim().parse::<u64>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

/// remote строго новее current?
fn is_newer(remote: &str, current: &str) -> bool {
    parse_semver(remote) > parse_semver(current)
}

/// Текущая версия лаунчера (вшита на сборке).
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Скачать и проверить подписанные метаданные релиза. Возвращает release, только
/// если подпись валидна и версия строго новее текущей.
pub async fn check(client: &reqwest::Client) -> Result<Option<LauncherRelease>> {
    let raw = client
        .get(RELEASE_JSON_URL)
        .send()
        .await
        .context("запрос launcher.json")?
        .error_for_status()
        .context("launcher.json: HTTP")?
        .bytes()
        .await
        .context("чтение launcher.json")?;
    let sig = client
        .get(RELEASE_SIG_URL)
        .send()
        .await
        .context("запрос подписи launcher.json")?
        .error_for_status()
        .context("launcher.json.sig: HTTP")?
        .text()
        .await
        .context("чтение подписи")?;

    // Проверяем подпись ВШИТЫМ ключом — иначе метаданные обновления не доверяем.
    l2_manifest::verify(&MANIFEST_PUBKEY, &raw, sig.trim())
        .map_err(|e| anyhow::anyhow!("подпись launcher.json неверна: {e}"))?;

    let rel: LauncherRelease = serde_json::from_slice(&raw).context("launcher.json: неверный JSON")?;

    // exe качаем только с доверенного хоста.
    if !is_allowed_url(&rel.exe_url) {
        bail!("недоверенный источник обновления: {}", rel.exe_url);
    }
    if rel.sha256.len() != 64 || !rel.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("launcher.json: некорректный sha256");
    }

    if is_newer(&rel.version, current_version()) {
        Ok(Some(rel))
    } else {
        Ok(None)
    }
}

/// Скачать новый exe рядом с текущим, проверить SHA-256 и заменить себя на месте.
/// После успеха процесс нужно перезапустить (вызывающий код делает app.restart()).
pub async fn apply(
    client: &reqwest::Client,
    rel: &LauncherRelease,
    progress: ProgressCb,
) -> Result<()> {
    let current = std::env::current_exe().context("определение пути текущего exe")?;
    let dir = current
        .parent()
        .context("у текущего exe нет родительской папки")?
        .to_path_buf();

    // Временный файл — в той же папке, чтобы замена была переименованием в пределах
    // тома (мгновенно), а не копированием между дисками.
    let tmp = dir.join(format!("{}.update.tmp", file_stem(&current)));

    // Проверяем, что в папку вообще можно писать (портативный exe в Program Files
    // без прав — частая причина «ошибок»). Лучше сказать заранее, чем сломаться на замене.
    if let Err(e) = std::fs::File::create(&tmp) {
        bail!(
            "нет прав на запись в папку лаунчера ({}). Переместите лаунчер в обычную папку \
             (Загрузки, Рабочий стол, папка игры) и обновите снова. Причина: {e}",
            dir.display()
        );
    }

    let total = rel.size;
    let shared = std::sync::Arc::new(Shared::new(total, 1, "download", Instant::now()));
    shared.set_current("Обновление лаунчера");

    // Тикер прогресса.
    let stop = std::sync::Arc::new(tokio::sync::Notify::new());
    let ticker = {
        let s = shared.clone();
        let cb = progress.clone();
        let stop = stop.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop.notified() => break,
                    _ = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                        (cb)(s.snapshot(false, false));
                    }
                }
            }
        })
    };

    let res = download_to(client, &rel.exe_url, &tmp, &shared).await;
    stop.notify_one();
    let _ = ticker.await;

    if let Err(e) = res {
        let _ = tokio::fs::remove_file(&tmp).await;
        (progress)(shared.snapshot(false, true));
        return Err(e);
    }

    // Проверка целостности ДО замены. Не сошлось — выходим, текущий exe цел.
    let tmp_clone = tmp.clone();
    let actual = tokio::task::spawn_blocking(move || l2_manifest::hash_file(&tmp_clone))
        .await?
        .context("хеширование скачанного exe")?;
    if actual != rel.sha256 {
        let _ = tokio::fs::remove_file(&tmp).await;
        (progress)(shared.snapshot(false, true));
        bail!("контрольная сумма обновления не совпала — обновление отменено, текущая версия не тронута");
    }

    // Атомарная замена себя + удаление временного файла (self-replace убирает tmp).
    let tmp_for_swap = tmp.clone();
    tokio::task::spawn_blocking(move || self_replace::self_replace(&tmp_for_swap))
        .await?
        .context("замена исполняемого файла")?;
    let _ = tokio::fs::remove_file(&tmp).await;

    (progress)(shared.snapshot(false, true));
    Ok(())
}

fn file_stem(p: &std::path::Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "l2-launcher".into())
}

/// Потоковая загрузка в файл с обновлением прогресса.
async fn download_to(
    client: &reqwest::Client,
    url: &str,
    dst: &std::path::Path,
    shared: &Shared,
) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let resp = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("скачивание {url}"))?
        .error_for_status()
        .context("обновление: HTTP")?;
    let mut file = tokio::fs::File::create(dst)
        .await
        .with_context(|| format!("создание {}", dst.display()))?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("обрыв загрузки обновления")?;
        file.write_all(&chunk).await?;
        shared.add_processed(chunk.len() as u64);
    }
    file.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_ordering() {
        assert!(is_newer("0.4.0", "0.3.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.3.10", "0.3.9"));
        assert!(!is_newer("0.3.9", "0.3.9"));
        assert!(!is_newer("0.3.8", "0.3.9"));
    }
}
