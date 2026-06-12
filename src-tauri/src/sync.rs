//! Классовая синхронизация клиента (5 классов из манифеста):
//!  managed — хэш-синк; optional(языки) — по требованию; seed-once/launcher-owned —
//!  только если отсутствует; preserve — отсутствует в манифесте (не трогаем).
//! После апдейта — засев WindowsInfo и переприменение perf/языка.

use l2_manifest::{safe_join, FileEntry, Manifest};
use std::path::Path;

/// Маркер наличия английского набора на диске (раз скачан — держим в активных).
pub fn en_downloaded(install: &Path) -> bool {
    install.join("systextures").join("L2Font-e.utx").exists()
}

/// Активен ли EN-набор: ЖЕЛАЕМЫЙ язык EN (из конфига) ИЛИ уже скачан ранее.
pub fn en_active(install: &Path, lang: &str) -> bool {
    lang == "en" || en_downloaded(install)
}

/// Файлы для хэш-синка (managed + активные языковые).
pub fn sync_refs<'a>(manifest: &'a Manifest, install: &Path, lang: &str) -> Vec<&'a FileEntry> {
    manifest.sync_files(en_active(install, lang))
}

/// seed-once/launcher-owned, которых нет на диске → докачать как дефолт установки.
pub fn missing_seed(manifest: &Manifest, install: &Path) -> Vec<FileEntry> {
    manifest
        .seed_files()
        .into_iter()
        .filter(|e| safe_join(install, &e.path).map(|p| !p.exists()).unwrap_or(false))
        .cloned()
        .collect()
}

