//! Генерация пары Ed25519 для подписи манифеста.
//!
//! Печатает приватный ключ (hex, 32 байта — ХРАНИТЬ В СЕКРЕТЕ) и публичный ключ
//! в разных форматах, включая готовый Rust-массив для вшивания в лаунчер.
//!
//! Использование:
//!   keygen                 — вывести в stdout
//!   keygen ./keys/manifest — записать manifest.key (приват) и manifest.pub (публичный hex)

use anyhow::Result;
use ed25519_dalek::SigningKey;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| anyhow::anyhow!("getrandom: {e}"))?;
    let sk = SigningKey::from_bytes(&seed);
    let pk = sk.verifying_key().to_bytes();

    let priv_hex = hex::encode(seed);
    let pub_hex = hex::encode(pk);
    let rust_array = format!(
        "pub const MANIFEST_PUBKEY: [u8; 32] = [{}];",
        pk.iter().map(|b| b.to_string()).collect::<Vec<_>>().join(", ")
    );

    if let Some(prefix) = std::env::args().nth(1) {
        let base = PathBuf::from(&prefix);
        if let Some(parent) = base.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(format!("{prefix}.key"), &priv_hex)?;
        std::fs::write(format!("{prefix}.pub"), &pub_hex)?;
        eprintln!("Приватный ключ → {prefix}.key (СЕКРЕТ, не коммитить!)");
        eprintln!("Публичный ключ → {prefix}.pub");
        eprintln!();
    }

    println!("# Приватный ключ (hex, 32 байта) — ХРАНИТЬ В СЕКРЕТЕ:");
    println!("{priv_hex}");
    println!();
    println!("# Публичный ключ (hex):");
    println!("{pub_hex}");
    println!();
    println!("# Для вшивания в лаунчер (src-tauri/src/manifest.rs):");
    println!("{rust_array}");
    Ok(())
}
