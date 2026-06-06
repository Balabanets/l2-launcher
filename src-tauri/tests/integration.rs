//! Интеграционный тест ядра лаунчера: загрузка с локального HTTP-сервера,
//! проверка целостности, обнаружение подмены и починка.
//!
//! GUI не требуется — проверяется именно логика апдейтера.

use l2_launcher_lib::l2_manifest::{hash_file, FileEntry, LaunchSpec, Manifest};
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
        files: vec![
            entry(&srv, "system/l2.exe"),
            entry(&srv, "system/core.dll"),
            entry(&srv, "textures/a.utx"),
        ],
        critical: vec!["system/*.dll".into(), "system/*.exe".into()],
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
    let cb: download::ProgressCb = Arc::new(move |_p| {
        calls2.fetch_add(1, Ordering::Relaxed);
    });
    let client = default_client();
    download::download_all(&client, &install, &base_url, diff.to_fetch(), 4, cb)
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
    let cb2: download::ProgressCb = Arc::new(|_p| {});
    download::download_all(&client, &install, &base_url, diff.to_fetch(), 4, cb2)
        .await
        .expect("починка должна пройти");
    let report = verify::verify_critical(&install, &manifest);
    assert!(report.ok, "после починки целостность восстановлена");
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
    };
    let install = unique_tmp("install_bad");
    let client = default_client();
    let cb: download::ProgressCb = Arc::new(|_p| {});
    let res = download::download_all(&client, &install, &base_url, vec![wrong], 1, cb).await;
    assert!(res.is_err(), "файл с несовпадающим хешем не должен приниматься");
    // битый временный файл не должен оставлять валидный результат
    assert!(!install.join("system/core.dll").exists());
}
