//! Проверка точных байтов manifest.json внешним публичным Ed25519-ключом.
//! Используется publisher-воркером до активации релиза и после загрузки с GitHub.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use std::path::PathBuf;

fn decode_32(path: &PathBuf) -> Result<[u8; 32]> {
    let value = std::fs::read_to_string(path)
        .with_context(|| format!("не читается публичный ключ {}", path.display()))?;
    let value = value.trim();
    let bytes = if let Ok(bytes) = hex::decode(value) {
        bytes
    } else {
        B64.decode(value).context("ключ не hex и не base64")?
    };
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow::anyhow!("публичный ключ должен быть ровно 32 байта"))
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let manifest = PathBuf::from(args.next().context("использование: manifest-verify MANIFEST SIG PUBKEY")?);
    let signature = PathBuf::from(args.next().context("нет файла подписи")?);
    let pubkey = PathBuf::from(args.next().context("нет публичного ключа")?);
    if args.next().is_some() {
        bail!("слишком много аргументов");
    }
    let raw = std::fs::read(&manifest).with_context(|| format!("не читается {}", manifest.display()))?;
    let sig = std::fs::read_to_string(&signature)
        .with_context(|| format!("не читается {}", signature.display()))?;
    l2_manifest::verify(&decode_32(&pubkey)?, &raw, &sig)?;
    let parsed: l2_manifest::Manifest = serde_json::from_slice(&raw).context("неверный JSON манифеста")?;
    let unsafe_paths = parsed.unsafe_paths();
    if !unsafe_paths.is_empty() {
        bail!("манифест содержит опасные пути: {:?}", unsafe_paths);
    }
    println!("OK {} ({} файлов)", parsed.version, parsed.files.len());
    Ok(())
}
