//! Настройки клиента L2: «Режим производительности» (dgVoodoo) и язык RU/EN.
//!
//! Все операции — только при ЗАКРЫТОЙ игре (проверяем отсутствие процесса L2.exe).
//! Полезная нагрузка лежит в корне клиента: `_launcher/perf/*`.
//! l2.ini зашифрован — мы его НЕ редактируем, а подменяем целиком готовыми файлами.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

const LOCALIZATION_XOR: u8 = 0xAC;
const LOCALIZATION_HEADER_LEN: usize = 28;

#[derive(Serialize)]
pub struct ClientSettings {
    /// Режим производительности включён (существует system/d3d8.dll).
    pub performance: bool,
    /// Язык клиента: "ru" (Language=6) или "en" (Language=1).
    pub language: String,
}

fn system_dir(install: &Path) -> PathBuf {
    install.join("system")
}
fn perf_dir(install: &Path) -> PathBuf {
    install.join("_launcher").join("perf")
}

/// Привести клиент к ЖЕЛАЕМЫМ настройкам (perf/язык). Best-effort и ТИХО —
/// вызывается при запуске игры и после апдейта. Никаких окон/ошибок наружу:
/// если payload ещё не скачан, просто логируем и пропускаем (применится позже).
pub fn apply(install: &Path, performance: bool, language: &str) {
    // Засеять WindowsInfo из _launcher/defaults, если его ещё нет.
    let wi = system_dir(install).join("WindowsInfo.ini");
    let src = install.join("_launcher").join("defaults").join("WindowsInfo.ini");
    if !wi.exists() && src.is_file() {
        if let Some(p) = wi.parent() {
            std::fs::create_dir_all(p).ok();
        }
        std::fs::copy(&src, &wi).ok();
    }
    if let Err(e) = set_performance(install, performance) {
        eprintln!("apply perf: {e}");
    }
    if let Err(e) = set_language(install, language) {
        eprintln!("apply lang: {e}");
    }
}

/// Включить/выключить режим производительности (подмена файлов целиком).
pub fn set_performance(install: &Path, enabled: bool) -> Result<()> {
    let sys = system_dir(install);
    let perf = perf_dir(install);
    let d3d8 = sys.join("d3d8.dll");
    let dgv = sys.join("dgVoodoo.conf");

    if enabled {
        copy_required(&perf.join("d3d8.dll"), &d3d8)?;
        copy_required(&perf.join("dgVoodoo.conf"), &dgv)?;
        copy_required(&perf.join("l2.ini.perf"), &sys.join("l2.ini"))?;
    } else {
        remove_if_exists(&d3d8)?;
        remove_if_exists(&dgv)?;
        copy_required(&perf.join("l2.ini.std"), &sys.join("l2.ini"))?;
    }
    Ok(())
}

/// Сменить язык клиента (RU=6 / EN=1) правкой Localization.ini на уровне байтов.
pub fn set_language(install: &Path, lang: &str) -> Result<()> {
    let target: u8 = match lang {
        "ru" => b'6',
        "en" => b'1',
        _ => bail!("неизвестный язык: {lang}"),
    };
    let path = system_dir(install).join("Localization.ini");
    let raw = std::fs::read(&path).with_context(|| format!("чтение {}", path.display()))?;
    if raw.len() <= LOCALIZATION_HEADER_LEN {
        bail!("Localization.ini короче ожидаемого");
    }
    let header = &raw[..LOCALIZATION_HEADER_LEN];
    // Тело — однобайтовое (Windows-1251), работаем строго по байтам, без UTF-8.
    let mut body: Vec<u8> = raw[LOCALIZATION_HEADER_LEN..].iter().map(|b| b ^ LOCALIZATION_XOR).collect();

    let pos = find_subslice(&body, b"Language=")
        .context("в Localization.ini не найдено Language=")?;
    let val_idx = pos + b"Language=".len();
    if val_idx >= body.len() {
        bail!("повреждённый Localization.ini");
    }
    body[val_idx] = target;

    let mut out = Vec::with_capacity(raw.len());
    out.extend_from_slice(header);
    out.extend(body.iter().map(|b| b ^ LOCALIZATION_XOR));
    std::fs::write(&path, &out).with_context(|| format!("запись {}", path.display()))?;
    Ok(())
}

fn copy_required(src: &Path, dst: &Path) -> Result<()> {
    if !src.is_file() {
        bail!(
            "нет файла полезной нагрузки: {} — папка _launcher/perf отсутствует в клиенте",
            src.display()
        );
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::copy(src, dst).with_context(|| format!("копирование {}", dst.display()))?;
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path).with_context(|| format!("удаление {}", path.display()))?;
    }
    Ok(())
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Прочитать текущий язык из Localization.ini (только для тестов).
#[cfg(test)]
fn read_language(install: &Path) -> Option<String> {
    let path = system_dir(install).join("Localization.ini");
    let raw = std::fs::read(&path).ok()?;
    if raw.len() <= LOCALIZATION_HEADER_LEN {
        return None;
    }
    let body: Vec<u8> = raw[LOCALIZATION_HEADER_LEN..].iter().map(|b| b ^ LOCALIZATION_XOR).collect();
    let pos = find_subslice(&body, b"Language=")?;
    match body.get(pos + b"Language=".len()) {
        Some(b'1') => Some("en".to_string()),
        Some(_) => Some("ru".to_string()),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_roundtrip_byte_level_preserves_other_bytes() {
        // Заголовок 28 байт + тело "[LanguageSet]\nLanguage=6\n" + «кириллица» (0xC0..)
        let plain = b"[LanguageSet]\nLanguage=6\nName=\xc0\xc1\xc2";
        let mut raw = vec![0u8; LOCALIZATION_HEADER_LEN];
        raw.extend(plain.iter().map(|b| b ^ LOCALIZATION_XOR));

        let dir = std::env::temp_dir().join(format!("l2cs_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("system")).unwrap();
        std::fs::write(dir.join("system/Localization.ini"), &raw).unwrap();

        assert_eq!(read_language(&dir).as_deref(), Some("ru"));
        set_language(&dir, "en").unwrap();
        assert_eq!(read_language(&dir).as_deref(), Some("en"));

        // Прочие байты (в т.ч. не-UTF-8 кириллица) не повреждены.
        let after = std::fs::read(dir.join("system/Localization.ini")).unwrap();
        let body: Vec<u8> = after[LOCALIZATION_HEADER_LEN..].iter().map(|b| b ^ LOCALIZATION_XOR).collect();
        assert!(find_subslice(&body, b"Name=\xc0\xc1\xc2").is_some());
        assert_eq!(&after[..LOCALIZATION_HEADER_LEN], &raw[..LOCALIZATION_HEADER_LEN]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
