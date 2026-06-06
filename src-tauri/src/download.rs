//! Параллельная докачка файлов с R2: resume (HTTP Range), прогресс-события,
//! пер-файловая проверка SHA-256 (битый файл не попадёт в установку).

use anyhow::{bail, Context, Result};
use futures_util::StreamExt;
use l2_manifest::FileEntry;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Mutex, Semaphore};

pub const PROGRESS_EVENT: &str = "update:progress";

/// Колбэк прогресса — отвязывает загрузчик от Tauri (тестируемость).
pub type ProgressCb = Arc<dyn Fn(Progress) + Send + Sync>;

#[derive(Clone, Serialize)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
    pub files_done: usize,
    pub files_total: usize,
    pub speed_bps: u64,
    pub eta_secs: u64,
    pub current: String,
    pub done: bool,
}

struct Shared {
    downloaded: AtomicU64,
    files_done: AtomicUsize,
    total: u64,
    files_total: usize,
    start: Instant,
    current: Mutex<String>,
}

impl Shared {
    fn snapshot(&self, done: bool) -> Progress {
        let downloaded = self.downloaded.load(Ordering::Relaxed);
        let elapsed = self.start.elapsed().as_secs_f64().max(0.001);
        let speed = (downloaded as f64 / elapsed) as u64;
        let remaining = self.total.saturating_sub(downloaded);
        let eta = if speed > 0 { remaining / speed } else { 0 };
        Progress {
            downloaded,
            total: self.total,
            files_done: self.files_done.load(Ordering::Relaxed),
            files_total: self.files_total,
            speed_bps: speed,
            eta_secs: eta,
            current: String::new(), // заполняется вызывающим при необходимости
            done,
        }
    }
}

/// Скачать один файл с поддержкой докачки и проверкой хеша.
async fn download_one(
    client: &reqwest::Client,
    base_url: &str,
    install: &Path,
    entry: &FileEntry,
    shared: &Shared,
) -> Result<()> {
    let url = format!("{}{}", base_url, entry.path);
    let target: PathBuf = install.join(&entry.path);
    let tmp: PathBuf = install.join(format!("{}.part", entry.path));

    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }

    {
        let mut cur = shared.current.lock().await;
        *cur = entry.path.clone();
    }

    // Возможная докачка: сколько уже есть в .part
    let mut existing: u64 = match tokio::fs::metadata(&tmp).await {
        Ok(m) if m.is_file() => m.len(),
        _ => 0,
    };
    // Если .part больше ожидаемого — начинаем заново.
    if existing > entry.size {
        let _ = tokio::fs::remove_file(&tmp).await;
        existing = 0;
    }
    if existing > 0 {
        shared.downloaded.fetch_add(existing, Ordering::Relaxed);
    }

    let mut req = client.get(&url);
    if existing > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={}-", existing));
    }
    let resp = req.send().await.with_context(|| format!("скачивание {}", entry.path))?;
    let status = resp.status();
    if !(status.is_success() || status == reqwest::StatusCode::PARTIAL_CONTENT) {
        bail!("{}: HTTP {}", entry.path, status);
    }
    // Если сервер проигнорировал Range и вернул 200 — пишем с нуля.
    let append = status == reqwest::StatusCode::PARTIAL_CONTENT && existing > 0;
    if !append && existing > 0 {
        // откатываем учтённые байты, файл перезапишем целиком
        shared.downloaded.fetch_sub(existing, Ordering::Relaxed);
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
        let chunk = chunk.context("обрыв загрузки")?;
        file.write_all(&chunk).await?;
        shared.downloaded.fetch_add(chunk.len() as u64, Ordering::Relaxed);
    }
    file.flush().await?;
    drop(file);

    // Проверка целостности скачанного файла.
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

    shared.files_done.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// Скачать все переданные файлы. Прогресс отдаётся через колбэк `progress`.
pub async fn download_all(
    client: &reqwest::Client,
    install: &Path,
    base_url: &str,
    entries: Vec<FileEntry>,
    concurrency: usize,
    progress: ProgressCb,
) -> Result<()> {
    let total: u64 = entries.iter().map(|e| e.size).sum();
    let files_total = entries.len();
    let shared = Arc::new(Shared {
        downloaded: AtomicU64::new(0),
        files_done: AtomicUsize::new(0),
        total,
        files_total,
        start: Instant::now(),
        current: Mutex::new(String::new()),
    });

    // Тикер прогресса.
    let ticker_shared = shared.clone();
    let ticker_cb = progress.clone();
    let stop = Arc::new(tokio::sync::Notify::new());
    let stop_ticker = stop.clone();
    let ticker = tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = stop_ticker.notified() => break,
                _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                    let mut p = ticker_shared.snapshot(false);
                    p.current = ticker_shared.current.lock().await.clone();
                    (ticker_cb)(p);
                }
            }
        }
    });

    let sem = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut handles = Vec::with_capacity(files_total);
    for entry in entries {
        let permit_sem = sem.clone();
        let client = client.clone();
        let base_url = base_url.to_string();
        let install = install.to_path_buf();
        let shared = shared.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit_sem.acquire_owned().await.unwrap();
            download_one(&client, &base_url, &install, &entry, &shared).await
        }));
    }

    let mut first_err: Option<String> = None;
    for h in handles {
        match h.await {
            Ok(Ok(())) => {}
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

    // Останавливаем тикер и шлём финальный снапшот.
    stop.notify_one();
    let _ = ticker.await;
    let final_p = shared.snapshot(first_err.is_none());
    (progress)(final_p);

    if let Some(e) = first_err {
        bail!(e);
    }
    Ok(())
}
