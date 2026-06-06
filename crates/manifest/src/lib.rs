//! Общие типы манифеста и подпись/проверка Ed25519.
//! Используется и лаунчером, и генератором манифеста.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const MANIFEST_FILE: &str = "manifest.json";
pub const SIGNATURE_FILE: &str = "manifest.json.sig";

/// Один файл клиента в манифесте.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    /// Путь относительно корня клиента, всегда через '/'.
    pub path: String,
    pub size: u64,
    /// SHA-256 в hex (нижний регистр).
    pub sha256: String,
}

/// Как лаунчер запускает игру.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchSpec {
    /// Относительный путь к исполняемому файлу (например "system/l2.exe").
    pub exe: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Рабочая директория относительно корня (например "system").
    #[serde(default)]
    pub cwd: Option<String>,
}

fn default_layout() -> String {
    "path".to_string()
}

/// Полный манифест клиента.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Версия набора файлов, например "2026.06.06".
    pub version: String,
    /// Базовый URL раздачи, со слешем на конце.
    pub base_url: String,
    /// Способ адресации файлов на раздаче:
    ///  - "path": URL = base_url + относительный путь (R2/nginx)
    ///  - "cas":  URL = base_url + sha256 (контентно-адресуемо, GitHub Releases)
    #[serde(default = "default_layout")]
    pub layout: String,
    pub files: Vec<FileEntry>,
    /// glob-паттерны критичных файлов — проверяются ВСЕГДА перед запуском.
    #[serde(default)]
    pub critical: Vec<String>,
    pub launch: LaunchSpec,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("ошибка сериализации: {0}")]
    Json(#[from] serde_json::Error),
    #[error("неверная подпись манифеста")]
    BadSignature,
    #[error("неверный формат ключа/подписи: {0}")]
    Crypto(String),
}

impl Manifest {
    /// Канонические байты для подписи/проверки (стабильная сериализация).
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ManifestError> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Является ли путь критичным (по glob-паттернам).
    pub fn is_critical(&self, path: &str) -> bool {
        self.critical.iter().any(|pat| {
            glob::Pattern::new(pat)
                .map(|p| p.matches(path))
                .unwrap_or(false)
        })
    }

    /// Список критичных файлов из манифеста.
    pub fn critical_files(&self) -> Vec<&FileEntry> {
        self.files.iter().filter(|f| self.is_critical(&f.path)).collect()
    }

    /// Контентно-адресуемая раздача (файлы по sha256).
    pub fn is_cas(&self) -> bool {
        self.layout == "cas"
    }
}

/// Подписать байты приватным ключом → подпись в base64.
pub fn sign(signing_key_bytes: &[u8; 32], data: &[u8]) -> String {
    let sk = SigningKey::from_bytes(signing_key_bytes);
    let sig: Signature = sk.sign(data);
    B64.encode(sig.to_bytes())
}

/// Проверить подпись (base64) публичным ключом (32 байта).
pub fn verify(pubkey_bytes: &[u8; 32], data: &[u8], sig_b64: &str) -> Result<(), ManifestError> {
    let vk = VerifyingKey::from_bytes(pubkey_bytes)
        .map_err(|e| ManifestError::Crypto(e.to_string()))?;
    let sig_raw = B64
        .decode(sig_b64.trim())
        .map_err(|e| ManifestError::Crypto(e.to_string()))?;
    let sig = Signature::from_slice(&sig_raw)
        .map_err(|e| ManifestError::Crypto(e.to_string()))?;
    vk.verify(data, &sig).map_err(|_| ManifestError::BadSignature)
}

/// SHA-256 файла в hex. Потоковое чтение — подходит для больших файлов.
pub fn hash_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1 << 20]; // 1 МБ
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// SHA-256 байтов в hex.
pub fn hash_bytes(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_roundtrip() {
        // детерминированный ключ для теста
        let sk_bytes = [7u8; 32];
        let sk = SigningKey::from_bytes(&sk_bytes);
        let pk = sk.verifying_key().to_bytes();
        let data = b"hello l2 interlude";
        let sig = sign(&sk_bytes, data);
        assert!(verify(&pk, data, &sig).is_ok());
        assert!(verify(&pk, b"tampered", &sig).is_err());
    }

    #[test]
    fn critical_matching() {
        let m = Manifest {
            version: "t".into(),
            base_url: "https://x/".into(),
            layout: "path".into(),
            files: vec![],
            critical: vec!["system/*.dll".into(), "system/l2.exe".into()],
            launch: LaunchSpec { exe: "system/l2.exe".into(), args: vec![], cwd: None },
        };
        assert!(m.is_critical("system/foo.dll"));
        assert!(m.is_critical("system/l2.exe"));
        assert!(!m.is_critical("textures/a.utx"));
    }
}
