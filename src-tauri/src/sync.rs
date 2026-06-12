//! Классовая синхронизация клиента (5 классов из манифеста):
//!  managed — хэш-синк; optional(языки) — по требованию; seed-once/launcher-owned —
//!  только если отсутствует; preserve — отсутствует в манифесте (не трогаем).
//! После апдейта — засев WindowsInfo и переприменение perf/языка.

use crate::client_settings;
use l2_manifest::{safe_join, FileEntry, Manifest};
use std::path::Path;

/// Маркер наличия английского набора на диске (раз скачан — держим в активных).
pub fn en_downloaded(install: &Path) -> bool {
    install.join("systextures").join("L2Font-e.utx").exists()
}

/// Активен ли EN-набор: выбран язык EN ИЛИ уже скачан ранее.
pub fn en_active(install: &Path) -> bool {
    client_settings::read_settings(install).language == "en" || en_downloaded(install)
}

/// Файлы для хэш-синка (managed + активные языковые).
pub fn sync_refs<'a>(manifest: &'a Manifest, install: &Path) -> Vec<&'a FileEntry> {
    manifest.sync_files(en_active(install))
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

/// После managed-апдейта: засеять WindowsInfo из defaults и переприменить perf+язык.
pub fn reapply(install: &Path) {
    let wi = install.join("system").join("WindowsInfo.ini");
    let src = install.join("_launcher").join("defaults").join("WindowsInfo.ini");
    if !wi.exists() && src.is_file() {
        if let Some(p) = wi.parent() {
            std::fs::create_dir_all(p).ok();
        }
        std::fs::copy(&src, &wi).ok();
    }
    // Переприменяем выбор игрока (состояние читаем с диска). Best-effort: ошибки
    // (например, отсутствует payload) не должны рушить апдейт.
    let st = client_settings::read_settings(install);
    if let Err(e) = client_settings::set_performance(install, st.performance) {
        eprintln!("reapply perf: {e}");
    }
    if let Err(e) = client_settings::set_language(install, &st.language) {
        eprintln!("reapply lang: {e}");
    }
}
