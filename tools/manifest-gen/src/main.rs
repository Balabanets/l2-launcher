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

/// Имена файлов (без учёта регистра), которые НЕ попадают в манифест: это
/// изменяемое пер-игроком состояние, а не часть дистрибутива. Их раздача либо
/// утекает чужие данные (AutoLogin.ini хранит логин/пароль), либо вызывает ложные
/// «требуется обновление», когда игрок сохраняет свой вход.
const IGNORED_FILENAMES: &[&str] = &["autologin.ini", "_prep_for_upload.bat"];

fn is_ignored(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    IGNORED_FILENAMES.contains(&lower.as_str())
}

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

    // Списки групп клиента (_launcher/groups) для классификации файлов.
    let groups = l2_manifest::groups::GroupLists::load(&root.join("_launcher"));

    // 1. Собираем список файлов (исключая служебные манифесты).
    let mut paths: Vec<PathBuf> = vec![];
    for entry in walkdir::WalkDir::new(&root).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let name = entry.file_name().to_string_lossy();
            if name == MANIFEST_FILE || name == SIGNATURE_FILE {
                continue;
            }
            if is_ignored(&name) {
                eprintln!("Пропускаю (пер-игровое состояние): {}", entry.path().display());
                continue;
            }
            paths.push(entry.path().to_path_buf());
        }
    }
    eprintln!("Файлов к обработке: {}", paths.len());

    // 2. Параллельно считаем размер + SHA-256 и классифицируем (Excluded → не в манифест).
    use l2_manifest::groups::Class;
    let files: Vec<FileEntry> = paths
        .par_iter()
        .map(|p| -> Result<Option<FileEntry>> {
            let rel = p.strip_prefix(&root)?.to_string_lossy().replace('\\', "/");
            let (class, group) = match groups.classify(&rel) {
                Class::Excluded => return Ok(None),
                Class::Managed => (None, None),
                Class::Optional(g) => (Some("optional".to_string()), Some(g)),
                Class::SeedOnce => (Some("seed-once".to_string()), None),
                Class::LauncherOwned => (Some("launcher-owned".to_string()), None),
            };
            let size = std::fs::metadata(p)?.len();
            let sha256 = l2_manifest::hash_file(p)?;
            Ok(Some(FileEntry { path: rel, size, sha256, class, group }))
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();

    let mut files = files;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let n_opt = files.iter().filter(|f| f.is_optional()).count();
    let n_seed = files.iter().filter(|f| f.is_seed_once()).count();
    let n_owned = files.iter().filter(|f| f.is_launcher_owned()).count();
    eprintln!("Классы: optional={n_opt}, seed-once={n_seed}, launcher-owned={n_owned}");

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
