//! Параллельная докачка файлов: resume (HTTP Range), прогресс, пер-файловая
//! проверка SHA-256, пауза/отмена. Поддерживает два режима адресации:
//!  - path: URL = base_url + относительный путь (раздача из R2/nginx)
//!  - cas:  URL = base_url + sha256  (контентно-адресуемая раздача с GitHub Releases)

use crate::control::Control;
use crate::progress::{ProgressCb, Shared};
use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use l2_manifest::FileEntry;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::Semaphore;

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Cancelled,
}

enum FileStatus {
    Done,
    Skipped, // отменено до завершения
}

/// Список URL-кандидатов файла (пробуются по порядку, пока не 200/206):
///  - "path":        [bases[0] + относительный путь]
///  - "cas":         [bases[0] + sha256]
///  - "cas-sharded": [bases[0] + <1-й символ sha256> + "/" + sha256]
///  - "cas-multi":   [base + sha256 для каждого base из bases]
fn candidate_urls(bases: &[String], layout: &str, entry: &FileEntry) -> Vec<String> {
    // zstd-файлы лежат рядом с оригиналом как "<url>.zst".
    let sfx = if entry.is_zstd() { ".zst" } else { "" };
    match layout {
        "cas" => vec![format!("{}{}{}", bases[0], entry.sha256, sfx)],
        "cas-sharded" => {
            let shard = &entry.sha256[..1];
            vec![format!("{}{}/{}{}", bases[0], shard, entry.sha256, sfx)]
        }
        "cas-multi" => bases.iter().map(|b| format!("{}{}{}", b, entry.sha256, sfx)).collect(),
        _ => vec![format!("{}{}{}", bases[0], entry.path, sfx)],
    }
}

async fn download_one(
    client: &reqwest::Client,
    bases: &[String],
    install: &Path,
    entry: &FileEntry,
    layout: &str,
    control: &Control,
    shared: &Shared,
) -> Result<FileStatus> {
    let urls = candidate_urls(bases, layout, entry);
    let compressed = entry.is_zstd();
    let dl_size = entry.download_size(); // сжатый размер при zstd, иначе обычный
    // Защита от path traversal — пишем строго внутри install.
    let target = l2_manifest::safe_join(install, &entry.path)
        .with_context(|| format!("небезопасный путь в манифесте: {}", entry.path))?;
    // .part — сырой файл; для zstd качаем сжатый в .zst.part, потом распаковываем.
    let part_rel = if compressed {
        format!("{}.zst.part", entry.path)
    } else {
        format!("{}.part", entry.path)
    };
    let tmp = l2_manifest::safe_join(install, &part_rel)
        .with_context(|| format!("небезопасный путь: {}", entry.path))?;

    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }
    shared.set_current(&entry.path);

    // Докачка: сколько уже есть в .part (в единицах того, что качаем — сжатого при zstd)
    let mut existing: u64 = match tokio::fs::metadata(&tmp).await {
        Ok(m) if m.is_file() => m.len(),
        _ => 0,
    };
    if existing > dl_size {
        let _ = tokio::fs::remove_file(&tmp).await;
        existing = 0;
    }
    if existing > 0 {
        shared.add_processed(existing);
    }

    // Пробуем кандидатов по порядку; 404 → следующий источник.
    let mut resp = None;
    let last = urls.len().saturating_sub(1);
    for (i, url) in urls.iter().enumerate() {
        let mut req = client.get(url);
        if existing > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={}-", existing));
        }
        let r = req.send().await.with_context(|| format!("скачивание {}", entry.path))?;
        let status = r.status();
        if status == reqwest::StatusCode::NOT_FOUND && i < last {
            continue;
        }
        if !(status.is_success() || status == reqwest::StatusCode::PARTIAL_CONTENT) {
            bail!("{}: HTTP {}", entry.path, status);
        }
        resp = Some(r);
        break;
    }
    let resp = resp.with_context(|| format!("{}: не найден ни в одном источнике", entry.path))?;
    let status = resp.status();
    let append = status == reqwest::StatusCode::PARTIAL_CONTENT && existing > 0;
    if !append && existing > 0 {
        shared.sub_processed(existing);
        existing = 0;
    }

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .read(true)
        .open(&tmp)
        .await
        .with_context(|| format!("создание {}", tmp.display()))?;
    if append {
        file.seek(std::io::SeekFrom::Start(existing)).await?;
    } else {
        file.set_len(0).await?;
    }

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        // Быстрая реакция на отмену (пауза обрабатывается на границе файлов).
        if control.is_cancelled() {
            file.flush().await.ok();
            return Ok(FileStatus::Skipped); // .part сохраняется для resume
        }
        let chunk = chunk.context("обрыв загрузки")?;
        file.write_all(&chunk).await?;
        shared.add_processed(chunk.len() as u64);
    }
    file.flush().await?;
    drop(file);

    if compressed {
        // Распаковываем .zst.part → target, затем сверяем SHA-256 ОРИГИНАЛА.
        let tmp_c = tmp.clone();
        let target_c = target.clone();
        let path_for_err = entry.path.clone();
        let sha = entry.sha256.clone();
        let ok_hash = tokio::task::spawn_blocking(move || -> Result<bool> {
            let inp = std::fs::File::open(&tmp_c)
                .with_context(|| format!("открытие {}", tmp_c.display()))?;
            let out = std::fs::File::create(&target_c)
                .with_context(|| format!("создание {}", target_c.display()))?;
            zstd::stream::copy_decode(inp, out)
                .with_context(|| format!("распаковка {path_for_err}"))?;
            Ok(l2_manifest::hash_file(&target_c)? == sha)
        })
        .await??;
        if !ok_hash {
            let _ = tokio::fs::remove_file(&tmp).await;
            let _ = tokio::fs::remove_file(&target).await;
            bail!("{}: контрольная сумма не совпала после распаковки", entry.path);
        }
        let _ = tokio::fs::remove_file(&tmp).await;
    } else {
        // Сырой файл: сверяем хеш и переименовываем.
        let tmp_clone = tmp.clone();
        let actual = tokio::task::spawn_blocking(move || l2_manifest::hash_file(&tmp_clone))
            .await?
            .with_context(|| format!("хеш {}", entry.path))?;
        if actual != entry.sha256 {
            let _ = tokio::fs::remove_file(&tmp).await;
            bail!("{}: контрольная сумма не совпала после загрузки", entry.path);
        }
        tokio::fs::rename(&tmp, &target)
            .await
            .with_context(|| format!("переименование {}", entry.path))?;
    }
    shared.inc_files();
    Ok(FileStatus::Done)
}

/// Скачать все файлы. Пауза — на границе файлов; отмена — мгновенно.
pub async fn download_all(
    client: &reqwest::Client,
    install: &Path,
    bases: Vec<String>,
    entries: Vec<FileEntry>,
    concurrency: usize,
    layout: String,
    control: Arc<Control>,
    progress: ProgressCb,
) -> Result<Outcome> {
    let total: u64 = entries.iter().map(|e| e.download_size()).sum();
    let files_total = entries.len();
    let shared = Arc::new(Shared::new(total, files_total, "download", Instant::now()));

    // Тикер прогресса.
    let ticker_shared = shared.clone();
    let ticker_cb = progress.clone();
    let ticker_control = control.clone();
    let stop = Arc::new(tokio::sync::Notify::new());
    let stop_ticker = stop.clone();
    let ticker = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop_ticker.notified() => break,
                _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                    (ticker_cb)(ticker_shared.snapshot(ticker_control.is_paused(), false));
                }
            }
        }
    });

    let sem = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut handles = Vec::with_capacity(files_total);
    let bases = Arc::new(bases);
    for entry in entries {
        let permit_sem = sem.clone();
        let client = client.clone();
        let bases = bases.clone();
        let install = install.to_path_buf();
        let shared = shared.clone();
        let control = control.clone();
        let layout = layout.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit_sem.acquire_owned().await.unwrap();
            // Пауза/отмена на границе файла.
            if !control.gate_async().await {
                return Ok(FileStatus::Skipped);
            }
            download_one(&client, &bases, &install, &entry, &layout, &control, &shared).await
        }));
    }

    let mut first_err: Option<String> = None;
    for h in handles {
        match h.await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                if first_err.is_none() {
                    first_err = Some(e.to_string());
                }
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(e.to_string());
                }
            }
        }
    }

    stop.notify_one();
    let _ = ticker.await;

    if let Some(e) = first_err {
        (progress)(shared.snapshot(false, true));
        bail!(e);
    }

    let outcome = if control.is_cancelled() {
        Outcome::Cancelled
    } else {
        Outcome::Completed
    };
    (progress)(shared.snapshot(false, true));
    Ok(outcome)
}
