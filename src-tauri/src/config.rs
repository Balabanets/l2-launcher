//! Настройки лаунчера: путь установки клиента, URL манифеста, адрес игрового сервера.
//! Персистятся в app config dir в JSON.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherConfig {
    /// Куда установлен/устанавливается клиент.
    pub install_dir: PathBuf,
    /// Прямой URL манифеста (manifest.json) в R2.
    pub manifest_url: String,
    /// Базовый URL backend API (для Слоя 2 / статуса / новостей).
    pub api_base: String,
    /// Адрес игрового сервера (для информации/подстановки).
    pub server_host: String,
    pub server_port: u16,
    /// Сколько файлов качать параллельно.
    pub concurrency: usize,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            install_dir: default_install_dir(),
            manifest_url: "https://l2cdn.balabanets.uk/client/manifest.json".to_string(),
            api_base: "https://l2.balabanets.uk".to_string(),
            server_host: "l2.balabanets.uk".to_string(),
            server_port: 2106,
            concurrency: 6,
        }
    }
}

fn default_install_dir() -> PathBuf {
    // Windows: C:\Games\L2Interlude; на остальных — папка рядом.
    if cfg!(windows) {
        PathBuf::from("C:/Games/L2Interlude")
    } else {
        dirs_like_home().join("L2Interlude")
    }
}

fn dirs_like_home() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

impl LauncherConfig {
    pub fn load(config_path: &Path) -> Self {
        match std::fs::read(config_path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, config_path: &Path) -> std::io::Result<()> {
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(config_path, bytes)
    }

    /// URL подписи манифеста.
    pub fn signature_url(&self) -> String {
        format!("{}.sig", self.manifest_url)
    }
}
