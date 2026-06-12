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

/// Удалить у игрока файлы/папки из manifest.delete (анти-traversal, best-effort).
/// Путь с '/' на конце или существующая директория — удаляются рекурсивно.
pub fn apply_deletions(install: &Path, manifest: &Manifest) {
    for d in &manifest.delete {
        let is_dir_hint = d.ends_with('/');
        let rel = d.trim_end_matches('/');
        let Some(target) = safe_join(install, rel) else {
            eprintln!("delete: небезопасный путь пропущен: {d}");
            continue;
        };
        let meta = match std::fs::symlink_metadata(&target) {
            Ok(m) => m,
            Err(_) => continue, // уже нет — ок
        };
        let res = if is_dir_hint || meta.is_dir() {
            std::fs::remove_dir_all(&target)
        } else {
            std::fs::remove_file(&target)
        };
        if let Err(e) = res {
            eprintln!("delete {}: {e}", target.display());
        }
    }
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

