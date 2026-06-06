//! Управление выполнением задач: пауза / возобновление / отмена.
//! Используется и загрузкой (async), и проверкой (sync/rayon).

use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Default)]
pub struct Control {
    paused: AtomicBool,
    cancelled: AtomicBool,
}

impl Control {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_paused(&self, v: bool) {
        self.paused.store(v, Ordering::Relaxed);
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    /// Сброс перед новым запуском задачи.
    pub fn reset(&self) {
        self.paused.store(false, Ordering::Relaxed);
        self.cancelled.store(false, Ordering::Relaxed);
    }
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Блокирующее ожидание снятия паузы (для sync-кода в rayon).
    /// Возвращает false, если задача отменена.
    pub fn gate_blocking(&self) -> bool {
        while self.is_paused() && !self.is_cancelled() {
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        !self.is_cancelled()
    }

    /// Асинхронное ожидание снятия паузы (для загрузки).
    /// Возвращает false, если задача отменена.
    pub async fn gate_async(&self) -> bool {
        while self.is_paused() && !self.is_cancelled() {
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
        !self.is_cancelled()
    }
}
