//! Сканирование локального клиента и вычисление дельты относительно манифеста.

use crate::control::Control;
use crate::progress::{ProgressCb, Shared};
use l2_manifest::{FileEntry, Manifest};
use rayon::prelude::*;
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScanMode {
    /// Быстро: наличие + размер для крупных ассетов, SHA-256 для мелких файлов.
    Quick,
    /// Полно: SHA-256 каждого файла.
    Hash,
}

/// В Quick-режиме файлы не крупнее этого порога хешируются всегда. Именно мелкие
/// файлы (ini/dat/int/htm/txt — конфиги, локализация, адрес логин-сервера) меняются
/// с сохранением размера, поэтому проверка только по размеру их подмену пропускает.
/// Крупные ассеты (текстуры/меши) при идентичном размере байт-в-байт не меняются —
/// для них достаточно размера, а полную сверку даёт кнопка «Проверить файлы».
const QUICK_HASH_MAX_BYTES: u64 = 1 << 20; // 1 МиБ

#[derive(Debug, Default, Serialize)]
pub struct Diff {
    pub missing: Vec<FileEntry>,
    pub mismatched: Vec<FileEntry>,
    pub ok: usize,
    pub bytes_to_download: u64,
    pub checked: usize,
}

impl Diff {
    pub fn needs_update(&self) -> bool {
        !self.missing.is_empty() || !self.mismatched.is_empty()
    }
    pub fn to_fetch(&self) -> Vec<FileEntry> {
        self.missing.iter().chain(self.mismatched.iter()).cloned().collect()
    }
}

enum Status {
    Ok,
    Missing,
    Mismatch,
    Skipped,
}

fn check_one(install: &Path, entry: &FileEntry, mode: ScanMode) -> Status {
    // Небезопасный путь (path traversal) → считаем расхождением, не трогаем ФС за пределами install.
    let path = match l2_manifest::safe_join(install, &entry.path) {
        Some(p) => p,
        None => return Status::Mismatch,
    };
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
    // Крупные ассеты в быстром режиме сверяем только по размеру; мелкие файлы —
    // всегда по SHA-256, чтобы ловить подмену с сохранением размера (см. порог выше).
    if mode == ScanMode::Quick && entry.size > QUICK_HASH_MAX_BYTES {
        return Status::Ok;
    }
    match l2_manifest::hash_file(&path) {
        Ok(h) if h == entry.sha256 => Status::Ok,
        _ => Status::Mismatch,
    }
}

fn collect(results: Vec<(Status, &FileEntry)>) -> Diff {
    let mut diff = Diff::default();
    for (status, entry) in results {
        diff.checked += 1;
        match status {
            Status::Ok => diff.ok += 1,
            Status::Skipped => diff.checked -= 1, // не считаем пропущенные при отмене
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

/// Быстрый скан без прогресса/паузы (для проверки обновлений).
pub fn scan(install: &Path, entries: &[&FileEntry], mode: ScanMode) -> Diff {
    let results: Vec<(Status, &FileEntry)> =
        entries.par_iter().map(|e| (check_one(install, e, mode), *e)).collect();
    collect(results)
}

/// Полный скан с прогрессом, паузой и отменой. Возвращает (дельта, отменено).
pub fn scan_with_progress(
    install: &Path,
    entries: &[&FileEntry],
    mode: ScanMode,
    control: Arc<Control>,
    progress: ProgressCb,
) -> (Diff, bool) {
    let total: u64 = entries.iter().map(|e| e.size).sum();
    let shared = Arc::new(Shared::new(total, entries.len(), "verify", Instant::now()));
    let stop = Arc::new(AtomicBool::new(false));

    // Тикер прогресса в отдельном потоке.
    let t_shared = shared.clone();
    let t_control = control.clone();
    let t_stop = stop.clone();
    let t_cb = progress.clone();
    let ticker = std::thread::spawn(move || {
        while !t_stop.load(Ordering::Relaxed) {
            (t_cb)(t_shared.snapshot(t_control.is_paused(), false));
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
    });

    let results: Vec<(Status, &FileEntry)> = entries
        .par_iter()
        .map(|e| {
            if !control.gate_blocking() {
                return (Status::Skipped, *e);
            }
            shared.set_current(&e.path);
            let st = check_one(install, e, mode);
            shared.add_processed(e.size);
            shared.inc_files();
            (st, *e)
        })
        .collect();

    stop.store(true, Ordering::Relaxed);
    let _ = ticker.join();
    (progress)(shared.snapshot(false, true));

    let cancelled = control.is_cancelled();
    (collect(results), cancelled)
}

pub fn scan_all(install: &Path, manifest: &Manifest, mode: ScanMode) -> Diff {
    let refs: Vec<&FileEntry> = manifest.files.iter().collect();
    scan(install, &refs, mode)
}

pub fn scan_critical(install: &Path, manifest: &Manifest, mode: ScanMode) -> Diff {
    let refs = manifest.critical_files();
    scan(install, &refs, mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    fn unique_tmp(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("l2scan_{}_{}", tag, SEQ.fetch_add(1, Ordering::SeqCst)));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn entry(path: &str, bytes: &[u8]) -> FileEntry {
        FileEntry {
            path: path.to_string(),
            size: bytes.len() as u64,
            sha256: l2_manifest::hash_bytes(bytes),
            ..Default::default()
        }
    }

    fn write(dir: &std::path::Path, rel: &str, bytes: &[u8]) {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    /// Мелкий файл с тем же размером, но другим содержимым в Quick-режиме
    /// должен распознаваться как изменённый (а не считаться целым по размеру).
    #[test]
    fn quick_detects_same_size_content_change_for_small_files() {
        let dir = unique_tmp("changed");
        // Манифест ожидает "aaaa", на диске лежит "bbbb" — размер совпадает, хеш нет.
        let e = entry("system/l2.ini", b"aaaa");
        write(&dir, "system/l2.ini", b"bbbb");

        let diff = scan(&dir, &[&e], ScanMode::Quick);
        assert_eq!(diff.mismatched.len(), 1, "подмена мелкого файла должна ловиться в Quick");
        assert_eq!(diff.ok, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Идентичный мелкий файл в Quick-режиме считается целым.
    #[test]
    fn quick_accepts_matching_small_file() {
        let dir = unique_tmp("match");
        let e = entry("system/l2.ini", b"hello world");
        write(&dir, "system/l2.ini", b"hello world");

        let diff = scan(&dir, &[&e], ScanMode::Quick);
        assert_eq!(diff.ok, 1);
        assert!(diff.mismatched.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
