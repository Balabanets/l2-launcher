//! Классовая синхронизация клиента (5 классов из манифеста):
//!  managed — хэш-синк; optional(языки) — по требованию; seed-once/launcher-owned —
//!  только если отсутствует; preserve — отсутствует в манифесте (не трогаем).
//! После апдейта — засев WindowsInfo и переприменение perf/языка.

use l2_manifest::{safe_join, FileEntry, Manifest};
use std::collections::HashSet;
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

/// Удаляет managed/optional-файлы, которые были в ранее применённой версии,
/// но отсутствуют в новой. Это делает откат симметричным: файлы из более новой
/// версии не остаются у игрока. seed-once и launcher-owned никогда не трогаем.
pub fn apply_version_transition(install: &Path, current: &Manifest, state_file: &Path) {
    let Ok(raw) = std::fs::read(state_file) else {
        return;
    };
    let Ok(previous) = serde_json::from_slice::<Manifest>(&raw) else {
        eprintln!("applied manifest: повреждённый локальный state пропущен");
        return;
    };
    let current_paths: HashSet<String> = current
        .files
        .iter()
        .map(|entry| entry.path.to_lowercase())
        .collect();
    for old in previous
        .files
        .iter()
        .filter(|entry| entry.is_managed() || entry.is_optional())
    {
        if current_paths.contains(&old.path.to_lowercase()) {
            continue;
        }
        let Some(target) = safe_join(install, &old.path) else {
            continue;
        };
        if let Err(error) = std::fs::remove_file(&target) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("version transition delete {}: {error}", target.display());
            }
        }
    }
}

pub fn save_applied_manifest(manifest: &Manifest, state_file: &Path) -> std::io::Result<()> {
    if let Some(parent) = state_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = state_file.with_extension("json.new");
    let bytes = serde_json::to_vec(manifest)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    std::fs::write(&tmp, bytes)?;
    if state_file.exists() {
        std::fs::remove_file(state_file)?;
    }
    std::fs::rename(tmp, state_file)
}

/// seed-once/launcher-owned, которых нет на диске → докачать как дефолт установки.
pub fn missing_seed(manifest: &Manifest, install: &Path) -> Vec<FileEntry> {
    manifest
        .seed_files()
        .into_iter()
        .filter(|e| {
            safe_join(install, &e.path)
                .map(|p| !p.exists())
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use l2_manifest::LaunchSpec;

    fn manifest(version: &str, paths: &[&str]) -> Manifest {
        Manifest {
            version: version.into(),
            base_url: "https://cdn.example.test/".into(),
            base_urls: vec![],
            layout: "path".into(),
            files: paths
                .iter()
                .map(|path| FileEntry {
                    path: (*path).into(),
                    size: 1,
                    sha256: "0".repeat(64),
                    ..Default::default()
                })
                .collect(),
            critical: vec![],
            delete: vec![],
            launch: LaunchSpec {
                exe: "system/l2.exe".into(),
                args: vec![],
                cwd: None,
            },
        }
    }

    #[test]
    fn rollback_removes_files_absent_from_target_manifest() {
        let root = std::env::temp_dir().join(format!("l2-sync-transition-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("system")).unwrap();
        std::fs::write(root.join("system/keep.dat"), b"x").unwrap();
        std::fs::write(root.join("system/new-only.dat"), b"x").unwrap();
        let state = root.join("applied.json");
        save_applied_manifest(
            &manifest("new", &["system/keep.dat", "system/new-only.dat"]),
            &state,
        )
        .unwrap();

        apply_version_transition(&root, &manifest("old", &["system/keep.dat"]), &state);

        assert!(root.join("system/keep.dat").exists());
        assert!(!root.join("system/new-only.dat").exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
