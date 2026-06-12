//! Интеграционный тест ядра лаунчера: загрузка с локального HTTP-сервера,
//! проверка целостности, обнаружение подмены и починка.
//!
//! GUI не требуется — проверяется именно логика апдейтера.

use l2_launcher_lib::control::Control;
use l2_launcher_lib::l2_manifest::{hash_file, FileEntry, LaunchSpec, Manifest};
use l2_launcher_lib::progress::ProgressCb;
use l2_launcher_lib::{default_client, download, scan, verify};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Минимальный HTTP-сервер, отдающий файлы из каталога с поддержкой Range.
fn serve_dir(root: PathBuf) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let root = root.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    return;
                }
                let req = String::from_utf8_lossy(&buf[..n]);
                let mut lines = req.lines();
                let first = lines.next().unwrap_or("");
                let mut parts = first.split_whitespace();
                let _method = parts.next().unwrap_or("");
                let path = parts.next().unwrap_or("/").trim_start_matches('/');
                // Диапазон?
                let mut range_start: Option<u64> = None;
                for l in lines {
                    if let Some(v) = l.strip_prefix("Range:").or_else(|| l.strip_prefix("range:")) {
                        if let Some(r) = v.trim().strip_prefix("bytes=") {
                            if let Some(s) = r.split('-').next() {
                                range_start = s.trim().parse().ok();
                            }
                        }
                    }
                }

                let file_path = root.join(path);
                match std::fs::read(&file_path) {
                    Ok(data) => {
                        if let Some(start) = range_start {
                            let start = start.min(data.len() as u64) as usize;
                            let body = &data[start..];
                            let header = format!(
                                "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                                body.len()
                            );
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.write_all(body);
                        } else {
                            let header = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\n\r\n",
                                data.len()
                            );
                            let _ = stream.write_all(header.as_bytes());
                            let _ = stream.write_all(&data);
                        }
                    }
                    Err(_) => {
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
                    }
                }
            });
        }
    });
    port
}

fn write_file(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

fn entry(root: &Path, rel: &str) -> FileEntry {
    let p = root.join(rel);
    FileEntry {
        path: rel.to_string(),
        size: std::fs::metadata(&p).unwrap().len(),
        sha256: hash_file(&p).unwrap(),
        ..Default::default()
    }
}

fn unique_tmp(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("l2test_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_update_verify_tamper_repair() {
    // 1. Сервер раздачи (как R2) с файлами клиента.
    let srv = unique_tmp("srv");
    write_file(&srv, "system/l2.exe", b"the real l2 executable bytes");
    write_file(&srv, "system/core.dll", b"critical core dll payload");
    let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    write_file(&srv, "textures/a.utx", &big);

    let port = serve_dir(srv.clone());
    let base_url = format!("http://127.0.0.1:{}/", port);

    // 2. Манифест по этим файлам.
    let manifest = Manifest {
        version: "test".into(),
        base_url: base_url.clone(),
        base_urls: vec![],
        layout: "path".into(),
        files: vec![
            entry(&srv, "system/l2.exe"),
            entry(&srv, "system/core.dll"),
            entry(&srv, "textures/a.utx"),
        ],
        critical: vec!["system/*.dll".into(), "system/*.exe".into()],
        delete: vec![],
        launch: LaunchSpec { exe: "system/l2.exe".into(), args: vec![], cwd: Some("system".into()) },
    };

    // 3. Пустой каталог установки → всё отсутствует.
    let install = unique_tmp("install");
    let diff = scan::scan_all(&install, &manifest, scan::ScanMode::Quick);
    assert!(diff.needs_update(), "на пустой установке должно требоваться обновление");
    assert_eq!(diff.missing.len(), 3);

    // 4. Скачиваем всё, считаем вызовы прогресса.
    let calls = Arc::new(AtomicUsize::new(0));
    let calls2 = calls.clone();
    let cb: ProgressCb = Arc::new(move |_p| {
        calls2.fetch_add(1, Ordering::Relaxed);
    });
    let client = default_client();
    download::download_all(
        &client, &install, vec![base_url.clone()], diff.to_fetch(), 4, "path".into(),
        Arc::new(Control::new()), cb,
    )
    .await
    .expect("загрузка должна пройти");
    assert!(calls.load(Ordering::Relaxed) >= 1, "прогресс должен эмититься");

    // 5. После загрузки всё на месте и хеши совпадают.
    let diff = scan::scan_all(&install, &manifest, scan::ScanMode::Hash);
    assert!(!diff.needs_update(), "после загрузки расхождений быть не должно");
    assert_eq!(diff.ok, 3);

    // 6. Проверка критичных файлов перед запуском — ок.
    let report = verify::verify_critical(&install, &manifest);
    assert!(report.ok, "критичные файлы должны быть валидны");
    assert!(report.bad.is_empty());

    // 7. ПОДМЕНА критичного файла → проверка обязана упасть.
    std::fs::write(install.join("system/core.dll"), b"HACKED CONTENT").unwrap();
    let report = verify::verify_critical(&install, &manifest);
    assert!(!report.ok, "подмена критичного файла должна обнаруживаться");
    assert!(report.bad.iter().any(|b| b == "system/core.dll"));

    // полный скан тоже видит расхождение
    let diff = scan::scan_all(&install, &manifest, scan::ScanMode::Hash);
    assert!(diff.needs_update());
    assert!(diff.mismatched.iter().any(|f| f.path == "system/core.dll"));

    // 8. Починка: докачиваем расхождения → снова валидно.
    let cb2: ProgressCb = Arc::new(|_p| {});
    download::download_all(
        &client, &install, vec![base_url.clone()], diff.to_fetch(), 4, "path".into(),
        Arc::new(Control::new()), cb2,
    )
    .await
    .expect("починка должна пройти");
    let report = verify::verify_critical(&install, &manifest);
    assert!(report.ok, "после починки целостность восстановлена");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cas_layout_downloads_by_hash() {
    // Раздача контентно-адресуема: файлы лежат под именами = sha256 (как на GitHub Releases).
    let srv = unique_tmp("srv_cas");
    let files: Vec<(&str, &[u8])> = vec![
        ("system/L2.exe", b"interlude client exe"),
        ("system/Core.dll", b"core dll bytes here"),
        ("textures/x.utx", b"texture payload data"),
    ];
    let mut entries = vec![];
    for (path, bytes) in &files {
        let h = l2_launcher_lib::l2_manifest::hash_bytes(bytes);
        // файл на сервере назван своим sha256
        std::fs::write(srv.join(&h), bytes).unwrap();
        entries.push(FileEntry { path: path.to_string(), size: bytes.len() as u64, sha256: h, ..Default::default() });
    }
    let port = serve_dir(srv.clone());
    let base_url = format!("http://127.0.0.1:{}/", port);

    let install = unique_tmp("install_cas");
    let client = default_client();
    let cb: ProgressCb = Arc::new(|_p| {});
    let outcome = download::download_all(
        &client, &install, vec![base_url.clone()], entries.clone(), 4, "cas".into(),
        Arc::new(Control::new()), cb,
    )
    .await
    .expect("CAS-загрузка должна пройти");
    assert_eq!(outcome, download::Outcome::Completed);

    // файлы разложены по их логическим путям и совпадают по хешу
    for (path, bytes) in &files {
        let got = std::fs::read(install.join(path)).expect("файл должен существовать");
        assert_eq!(&got, bytes, "{path}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cas_multi_falls_back_to_second_source() {
    // Два источника: файл есть только во втором → fallback по 404 должен его найти.
    let srv1 = unique_tmp("srv_multi1");
    let srv2 = unique_tmp("srv_multi2");
    let bytes: &[u8] = b"file only present in the second release";
    let h = l2_launcher_lib::l2_manifest::hash_bytes(bytes);
    std::fs::write(srv2.join(&h), bytes).unwrap(); // только во втором источнике
    let p1 = serve_dir(srv1.clone());
    let p2 = serve_dir(srv2.clone());
    let bases = vec![
        format!("http://127.0.0.1:{}/", p1),
        format!("http://127.0.0.1:{}/", p2),
    ];
    let entry = FileEntry { path: "system/x.dll".into(), size: bytes.len() as u64, sha256: h, ..Default::default() };

    let install = unique_tmp("install_multi");
    let client = default_client();
    let cb: ProgressCb = Arc::new(|_p| {});
    let outcome = download::download_all(
        &client, &install, bases, vec![entry], 2, "cas-multi".into(),
        Arc::new(Control::new()), cb,
    )
    .await
    .expect("cas-multi должен найти файл во втором источнике");
    assert_eq!(outcome, download::Outcome::Completed);
    assert_eq!(std::fs::read(install.join("system/x.dll")).unwrap(), bytes);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_stops_download() {
    let srv = unique_tmp("srv_cancel");
    write_file(&srv, "system/a.dat", &vec![1u8; 100_000]);
    let port = serve_dir(srv.clone());
    let base_url = format!("http://127.0.0.1:{}/", port);
    let entry = entry(&srv, "system/a.dat");

    let install = unique_tmp("install_cancel");
    let client = default_client();
    let control = Arc::new(Control::new());
    control.cancel(); // отменяем заранее → файл не должен скачаться
    let cb: ProgressCb = Arc::new(|_p| {});
    let outcome = download::download_all(
        &client, &install, vec![base_url.clone()], vec![entry], 2, "path".into(), control, cb,
    )
    .await
    .expect("отмена не должна быть ошибкой");
    assert_eq!(outcome, download::Outcome::Cancelled);
    assert!(!install.join("system/a.dat").exists(), "при отмене файл не появляется");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rejects_corrupt_download() {
    // Сервер отдаёт НЕ тот контент, что в манифесте → загрузка должна отклониться по хешу.
    let srv = unique_tmp("srv_bad");
    write_file(&srv, "system/core.dll", b"server has different bytes");
    let port = serve_dir(srv.clone());
    let base_url = format!("http://127.0.0.1:{}/", port);

    // Манифест с НЕВЕРНЫМ (чужим) хешем для файла.
    let wrong = FileEntry {
        path: "system/core.dll".into(),
        size: std::fs::metadata(srv.join("system/core.dll")).unwrap().len(),
        sha256: "0".repeat(64),
        ..Default::default()
    };
    let install = unique_tmp("install_bad");
    let client = default_client();
    let cb: ProgressCb = Arc::new(|_p| {});
    let res = download::download_all(
        &client, &install, vec![base_url.clone()], vec![wrong], 1, "path".into(),
        Arc::new(Control::new()), cb,
    )
    .await;
    assert!(res.is_err(), "файл с несовпадающим хешем не должен приниматься");
    // битый временный файл не должен оставлять валидный результат
    assert!(!install.join("system/core.dll").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn downloads_and_decompresses_zstd() {
    use l2_launcher_lib::l2_manifest::hash_bytes;

    // Компрессируемые данные + их .zst рядом (как на раздаче).
    let srv = unique_tmp("srv_zstd");
    let content: Vec<u8> = std::iter::repeat(*b"L2 Interlude zstd payload chunk\n")
        .take(8192)
        .flatten()
        .collect();
    write_file(&srv, "data/blob.bin", &content);
    {
        let inp = std::fs::File::open(srv.join("data/blob.bin")).unwrap();
        let out = std::fs::File::create(srv.join("data/blob.bin.zst")).unwrap();
        zstd::stream::copy_encode(inp, out, 19).unwrap();
    }
    let csize = std::fs::metadata(srv.join("data/blob.bin.zst")).unwrap().len();
    assert!(csize < content.len() as u64, "тест-данные должны сжиматься");

    let port = serve_dir(srv.clone());
    let base = format!("http://127.0.0.1:{}/", port);

    let entry = FileEntry {
        path: "data/blob.bin".into(),
        size: content.len() as u64,
        sha256: hash_bytes(&content),
        comp: Some("zstd".into()),
        csize: Some(csize),
        ..Default::default()
    };
    let install = unique_tmp("install_zstd");
    let client = default_client();
    let cb: ProgressCb = Arc::new(|_p| {});
    download::download_all(
        &client, &install, vec![base], vec![entry], 2, "path".into(),
        Arc::new(Control::new()), cb,
    )
    .await
    .expect("zstd-загрузка должна пройти");

    // Распаковано верно (сверка идёт по SHA-256 оригинала), временный .zst.part убран.
    assert_eq!(std::fs::read(install.join("data/blob.bin")).unwrap(), content);
    assert!(!install.join("data/blob.bin.zst.part").exists());
}
