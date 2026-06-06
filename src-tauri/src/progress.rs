//! Общая модель прогресса для загрузки и проверки + аккумулятор со снимками.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub const PROGRESS_EVENT: &str = "update:progress";

/// Колбэк прогресса — отвязывает задачи от Tauri (тестируемость).
pub type ProgressCb = std::sync::Arc<dyn Fn(Progress) + Send + Sync>;

#[derive(Clone, Serialize)]
pub struct Progress {
    /// "download" | "verify"
    pub phase: String,
    /// загружено / захешировано байт
    pub processed: u64,
    pub total: u64,
    pub files_done: usize,
    pub files_total: usize,
    pub speed_bps: u64,
    pub eta_secs: u64,
    pub current: String,
    pub paused: bool,
    pub done: bool,
}

/// Потокобезопасный аккумулятор прогресса.
pub struct Shared {
    processed: AtomicU64,
    files_done: AtomicUsize,
    total: u64,
    files_total: usize,
    start: Instant,
    current: Mutex<String>,
    phase: &'static str,
}

impl Shared {
    pub fn new(total: u64, files_total: usize, phase: &'static str, start: Instant) -> Self {
        Self {
            processed: AtomicU64::new(0),
            files_done: AtomicUsize::new(0),
            total,
            files_total,
            start,
            current: Mutex::new(String::new()),
            phase,
        }
    }

    pub fn add_processed(&self, n: u64) {
        self.processed.fetch_add(n, Ordering::Relaxed);
    }
    pub fn sub_processed(&self, n: u64) {
        self.processed.fetch_sub(n, Ordering::Relaxed);
    }
    pub fn inc_files(&self) {
        self.files_done.fetch_add(1, Ordering::Relaxed);
    }
    pub fn set_current(&self, s: &str) {
        if let Ok(mut c) = self.current.lock() {
            *c = s.to_string();
        }
    }

    pub fn snapshot(&self, paused: bool, done: bool) -> Progress {
        let processed = self.processed.load(Ordering::Relaxed);
        let elapsed = self.start.elapsed().as_secs_f64().max(0.001);
        let speed = (processed as f64 / elapsed) as u64;
        let remaining = self.total.saturating_sub(processed);
        let eta = if speed > 0 { remaining / speed } else { 0 };
        Progress {
            phase: self.phase.to_string(),
            processed,
            total: self.total,
            files_done: self.files_done.load(Ordering::Relaxed),
            files_total: self.files_total,
            speed_bps: speed,
            eta_secs: eta,
            current: self.current.lock().map(|c| c.clone()).unwrap_or_default(),
            paused,
            done,
        }
    }
}
