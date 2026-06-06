//! Проверка целостности перед запуском и построение хеш-отчёта для сервера (Слой 2).

use l2_manifest::Manifest;
use serde::Serialize;
use std::path::Path;

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
