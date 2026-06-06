//! Сканирование локального клиента и вычисление дельты относительно манифеста.

use l2_manifest::{FileEntry, Manifest};
use rayon::prelude::*;
use serde::Serialize;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScanMode {
    /// Быстро: только наличие + размер (для частой проверки обновлений).
    Quick,
    /// Полно: SHA-256 каждого файла (целостность).
    Hash,
}

#[derive(Debug, Default, Serialize)]
pub struct Diff {
    /// Файлы, которых нет локально.
    pub missing: Vec<FileEntry>,
    /// Файлы с неверным размером/хешем.
    pub mismatched: Vec<FileEntry>,
    /// Сколько файлов в порядке.
    pub ok: usize,
    /// Сколько байт предстоит скачать (missing + mismatched).
    pub bytes_to_download: u64,
    /// Всего проверено файлов.
    pub checked: usize,
}

impl Diff {
    pub fn needs_update(&self) -> bool {
        !self.missing.is_empty() || !self.mismatched.is_empty()
    }
    /// Файлы, которые нужно (пере)скачать.
    pub fn to_fetch(&self) -> Vec<FileEntry> {
        self.missing.iter().chain(self.mismatched.iter()).cloned().collect()
    }
}

enum Status {
    Ok,
    Missing,
    Mismatch,
}

fn check_one(install: &Path, entry: &FileEntry, mode: ScanMode) -> Status {
    let path = install.join(&entry.path);
    let meta = match std::fs::metadata(&path) {
        Ok(m) => m,
        Err(_) => return Status::Missing,
    };
    if !meta.is_file() {
        return Status::Missing;
    }
    if meta.len() != entry.size {
        return Status::Mismatch;
    }
    if mode == ScanMode::Quick {
        return Status::Ok;
    }
    match l2_manifest::hash_file(&path) {
        Ok(h) if h == entry.sha256 => Status::Ok,
        _ => Status::Mismatch,
    }
}

/// Просканировать заданный набор файлов и собрать дельту.
pub fn scan(install: &Path, entries: &[&FileEntry], mode: ScanMode) -> Diff {
    let results: Vec<(Status, &FileEntry)> = entries
        .par_iter()
        .map(|e| (check_one(install, e, mode), *e))
        .collect();

    let mut diff = Diff::default();
    diff.checked = results.len();
    for (status, entry) in results {
        match status {
            Status::Ok => diff.ok += 1,
            Status::Missing => {
                diff.bytes_to_download += entry.size;
                diff.missing.push(entry.clone());
            }
            Status::Mismatch => {
                diff.bytes_to_download += entry.size;
                diff.mismatched.push(entry.clone());
            }
        }
    }
    diff
}

/// Скан всех файлов манифеста.
pub fn scan_all(install: &Path, manifest: &Manifest, mode: ScanMode) -> Diff {
    let refs: Vec<&FileEntry> = manifest.files.iter().collect();
    scan(install, &refs, mode)
}

/// Скан только критичных файлов (для проверки перед запуском).
pub fn scan_critical(install: &Path, manifest: &Manifest, mode: ScanMode) -> Diff {
    let refs = manifest.critical_files();
    scan(install, &refs, mode)
}
