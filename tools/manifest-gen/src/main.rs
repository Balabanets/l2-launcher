//! Генератор подписанного манифеста клиента L2.
//!
//! Проходит по папке клиента, считает размеры и SHA-256 (параллельно),
//! формирует manifest.json и подписывает его Ed25519 → manifest.json.sig.
//!
//! Использование:
//!   manifest-gen \
//!     --client  /home/creative/l2-client-master \
//!     --out     ./dist \
//!     --base-url https://l2cdn.balabanets.uk/client/ \
//!     --version 2026.06.06 \
//!     --key     ./keys/manifest_ed25519.key   (32 байта приватного ключа, hex или base64)
//!
//! Критичные паттерны и команда запуска заданы значениями по умолчанию для Interlude,
//! переопределяются флагами --critical и --exe.

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use l2_manifest::{sign, FileEntry, LaunchSpec, Manifest, MANIFEST_FILE, SIGNATURE_FILE};
use rayon::prelude::*;
use std::path::{Path, PathBuf};

struct Args {
    client: PathBuf,
    out: PathBuf,
    /// Один или несколько источников (для cas-multi). Первый — основной base_url.
    base_urls: Vec<String>,
    version: String,
    key: PathBuf,
    critical: Vec<String>,
    exe: String,
    cwd: Option<String>,
    layout: String,
}

fn parse_args() -> Result<Args> {
    let mut client = None;
    let mut out = PathBuf::from("./dist");
    let mut base_urls: Vec<String> = vec![];
    let mut version = None;
    let mut key = None;
    let mut critical: Vec<String> = vec![];
    let mut exe = "system/l2.exe".to_string();
    let mut cwd = Some("system".to_string());
    let mut layout = "path".to_string();

    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut val = || it.next().ok_or_else(|| anyhow!("ожидалось значение после {a}"));
        match a.as_str() {
            "--client" => client = Some(PathBuf::from(val()?)),
            "--out" => out = PathBuf::from(val()?),
            "--base-url" => base_urls.push(val()?),
            "--version" => version = Some(val()?),
            "--key" => key = Some(PathBuf::from(val()?)),
            "--critical" => critical.push(val()?),
            "--exe" => exe = val()?,
            "--cwd" => cwd = Some(val()?),
            "--layout" => layout = val()?,
            "-h" | "--help" => {
                println!("{}", include_str!("../README_USAGE.txt"));
                std::process::exit(0);
            }
            other => bail!("неизвестный аргумент: {other}"),
        }
    }

    if critical.is_empty() {
        // Разумные значения по умолчанию для Interlude.
        critical = vec![
            "system/l2.exe".into(),
            "system/*.dll".into(),
            "system/*.exe".into(),
            "system/*.int".into(),
            "system/*.dat".into(),
        ];
    }

    if base_urls.is_empty() {
        bail!("обязателен хотя бы один --base-url");
    }

    Ok(Args {
        client: client.context("обязателен --client")?,
        out,
        base_urls,
        version: version.context("обязателен --version")?,
        key: key.context("обязателен --key")?,
        critical,
        exe,
        cwd,
        layout,
    })
}

/// Загрузить 32 байта приватного ключа из файла (hex или base64).
fn load_key(path: &Path) -> Result<[u8; 32]> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("не читается ключ {}", path.display()))?;
    let raw = raw.trim();
    let bytes = if let Ok(b) = hex::decode(raw) {
        b
    } else {
        B64.decode(raw).context("ключ не hex и не base64")?
    };
    let arr: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("ключ должен быть ровно 32 байта, получено {}", bytes.len()))?;
    Ok(arr)
}

fn main() -> Result<()> {
    let args = parse_args()?;
    let root = args.client.canonicalize().context("папка клиента не найдена")?;
    if !root.is_dir() {
        bail!("--client должен быть директорией");
    }
    let key = load_key(&args.key)?;

    // 1. Собираем список файлов (исключая служебные манифесты).
    let mut paths: Vec<PathBuf> = vec![];
    for entry in walkdir::WalkDir::new(&root).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let name = entry.file_name().to_string_lossy();
            if name == MANIFEST_FILE || name == SIGNATURE_FILE {
                continue;
            }
            paths.push(entry.path().to_path_buf());
        }
    }
    eprintln!("Файлов к обработке: {}", paths.len());

    // 2. Параллельно считаем размер + SHA-256.
    let files: Vec<FileEntry> = paths
        .par_iter()
        .map(|p| -> Result<FileEntry> {
            let rel = p
                .strip_prefix(&root)?
                .to_string_lossy()
                .replace('\\', "/");
            let size = std::fs::metadata(p)?.len();
            let sha256 = l2_manifest::hash_file(p)?;
            Ok(FileEntry { path: rel, size, sha256 })
        })
        .collect::<Result<Vec<_>>>()?;

    let mut files = files;
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let total: u64 = files.iter().map(|f| f.size).sum();
    eprintln!("Суммарный размер: {:.2} ГБ", total as f64 / 1e9);

    // 3. Собираем и подписываем манифест.
    let norm = |u: &str| if u.ends_with('/') { u.to_string() } else { format!("{u}/") };
    let base_urls: Vec<String> = args.base_urls.iter().map(|u| norm(u)).collect();
    let base_url = base_urls[0].clone();
    // base_urls в манифесте имеет смысл только для cas-multi.
    let manifest_base_urls = if args.layout == "cas-multi" { base_urls.clone() } else { vec![] };
    let manifest = Manifest {
        version: args.version,
        base_url,
        base_urls: manifest_base_urls,
        layout: args.layout,
        files,
        critical: args.critical,
        launch: LaunchSpec { exe: args.exe, args: vec![], cwd: args.cwd },
    };

    let bytes = manifest.canonical_bytes()?;
    let sig = sign(&key, &bytes);

    // 4. Пишем dist/manifest.json + .sig
    std::fs::create_dir_all(&args.out)?;
    std::fs::write(args.out.join(MANIFEST_FILE), &bytes)?;
    std::fs::write(args.out.join(SIGNATURE_FILE), sig.as_bytes())?;

    eprintln!(
        "Готово: {} ({} файлов), подпись {}",
        args.out.join(MANIFEST_FILE).display(),
        manifest.files.len(),
        args.out.join(SIGNATURE_FILE).display()
    );
    Ok(())
}
