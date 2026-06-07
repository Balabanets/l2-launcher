//! Проверка целостности перед запуском и построение хеш-отчёта для сервера (Слой 2).

use l2_manifest::Manifest;
use rayon::prelude::*;
use serde::Serialize;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::control::Control;
use crate::progress::{ProgressCb, Shared};
use crate::scan::{scan_critical, ScanMode};

#[derive(Debug, Serialize)]
pub struct VerifyReport {
    pub ok: bool,
    /// Пути проблемных критичных файлов (отсутствуют/повреждены).
    pub bad: Vec<String>,
    pub checked: usize,
}

/// Проверить критичные файлы (полное хеширование). Вызывается ВСЕГДА перед запуском.
pub fn verify_critical(install: &Path, manifest: &Manifest) -> VerifyReport {
    let diff = scan_critical(install, manifest, ScanMode::Hash);
    let mut bad: Vec<String> = Vec::new();
    bad.extend(diff.missing.iter().map(|f| f.path.clone()));
    bad.extend(diff.mismatched.iter().map(|f| f.path.clone()));
    VerifyReport {
        ok: bad.is_empty(),
        bad,
        checked: diff.checked,
    }
}

/// Реальные SHA-256 критичных файлов с диска (path, sha256). Отсутствующие пропускаются —
/// тогда сверка на сервере не сойдётся и авторизация будет отклонена.
pub fn critical_real_hashes(install: &Path, manifest: &Manifest) -> Vec<(String, String)> {
    manifest
        .critical_files()
        .iter()
        .filter_map(|f| {
            let p = l2_manifest::safe_join(install, &f.path)?;
            l2_manifest::hash_file(&p).ok().map(|h| (f.path.clone(), h))
        })
        .collect()
}

/// Проверка критичных файлов С ПРОГРЕССОМ за один проход хеширования: возвращает отчёт
/// (ок/битые) и реальные (path, sha256) для авторизации. Эмитит progress (phase "verify"),
/// уважает паузу/отмену. Так фаза проверки перед запуском видна в UI и не «висит молча».
pub fn verify_and_hash_critical(
    install: &Path,
    manifest: &Manifest,
    control: Arc<Control>,
    progress: ProgressCb,
) -> (VerifyReport, Vec<(String, String)>) {
    let crit = manifest.critical_files();
    let total: u64 = crit.iter().map(|f| f.size).sum();
    let shared = Arc::new(Shared::new(total, crit.len(), "verify", Instant::now()));
    let stop = Arc::new(AtomicBool::new(false));

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

    // (path, expected_sha, real_sha|None)
    let results: Vec<(String, String, Option<String>)> = crit
        .par_iter()
        .map(|f| {
            if !control.gate_blocking() {
                return (f.path.clone(), f.sha256.clone(), None);
            }
            shared.set_current(&f.path);
            let real = l2_manifest::safe_join(install, &f.path)
                .and_then(|p| l2_manifest::hash_file(&p).ok());
            shared.add_processed(f.size);
            shared.inc_files();
            (f.path.clone(), f.sha256.clone(), real)
        })
        .collect();

    stop.store(true, Ordering::Relaxed);
    let _ = ticker.join();
    (progress)(shared.snapshot(false, true));

    let mut bad = Vec::new();
    let mut hashes = Vec::new();
    for (path, expected, real) in results {
        match real {
            Some(h) if h == expected => hashes.push((path, h)),
            _ => bad.push(path),
        }
    }
    let checked = bad.len() + hashes.len();
    (VerifyReport { ok: bad.is_empty(), bad, checked }, hashes)
}

/// Детерминированный дайджест состояния критичных файлов — для отправки на сервер.
/// Сервер (aCis, Слой 2) сверяет его с эталоном и пускает только совпадающих.
pub fn critical_digest(manifest: &Manifest) -> String {
    let mut critical: Vec<_> = manifest.critical_files();
    critical.sort_by(|a, b| a.path.cmp(&b.path));
    let mut buf = String::new();
    for f in critical {
        buf.push_str(&f.path);
        buf.push(':');
        buf.push_str(&f.sha256);
        buf.push('\n');
    }
    l2_manifest::hash_bytes(buf.as_bytes())
}
