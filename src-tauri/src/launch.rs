//! Запуск игры. Игру запускает ТОЛЬКО лаунчер — после успешной проверки целостности.

use anyhow::{anyhow, bail, Context, Result};
use l2_manifest::Manifest;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Нормализованный абсолютный путь из относительного (компонентами, без смешения сепараторов).
fn resolve(install: &Path, rel: &str, what: &str) -> Result<PathBuf> {
    let joined = l2_manifest::safe_join(install, rel)
        .with_context(|| format!("небезопасный путь ({what}): {rel}"))?;
    // Абсолютизируем относительно install (install уже абсолютный).
    Ok(joined)
}

/// Запустить исполняемый файл клиента согласно манифесту.
pub fn launch_game(install: &Path, manifest: &Manifest, token: Option<&str>) -> Result<()> {
    let exe = resolve(install, &manifest.launch.exe, "exe")?;
    if !exe.exists() {
        bail!(
            "файл игры не найден: {}\nПроверь, что выбрана правильная папка установки в Настройках, и нажми «Проверить файлы».",
            exe.display()
        );
    }
    if !exe.is_file() {
        bail!("путь игры не является файлом: {}", exe.display());
    }

    let cwd = match &manifest.launch.cwd {
        Some(c) => resolve(install, c, "cwd")?,
        None => exe.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| install.to_path_buf()),
    };
    if !cwd.is_dir() {
        bail!("рабочая папка не найдена: {}", cwd.display());
    }

    let mut cmd = Command::new(&exe);
    cmd.current_dir(&cwd);
    cmd.args(&manifest.launch.args);
    if let Some(t) = token {
        cmd.env("L2_SESSION_TOKEN", t);
    }

    match cmd.spawn() {
        Ok(_) => Ok(()),
        // os error 740 — клиент требует прав администратора (манифест requireAdministrator).
        // CreateProcess так не умеет → перезапускаем через UAC (ShellExecute "runas").
        Err(e) if e.raw_os_error() == Some(740) => launch_elevated(&exe, &cwd, &manifest.launch.args)
            .with_context(|| format!("не удалось запустить с повышением прав: {}", exe.display())),
        Err(e) => Err(anyhow!(
            "не удалось запустить {} (рабочая папка {}): {e} [код ОС: {:?}]",
            exe.display(),
            cwd.display(),
            e.raw_os_error()
        )),
    }
}

/// Запуск с повышением прав (UAC) через PowerShell Start-Process -Verb RunAs.
/// Не блокирует: powershell стартует игру и завершается; на decline UAC игра просто не стартует.
fn launch_elevated(exe: &Path, cwd: &Path, args: &[String]) -> Result<()> {
    // Экранируем одинарные кавычки для PowerShell single-quoted строк.
    let q = |s: String| s.replace('\'', "''");
    let exe_s = q(exe.display().to_string());
    let cwd_s = q(cwd.display().to_string());
    let mut ps = format!(
        "Start-Process -FilePath '{exe_s}' -WorkingDirectory '{cwd_s}' -Verb RunAs"
    );
    if !args.is_empty() {
        let list = args
            .iter()
            .map(|a| format!("'{}'", a.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        ps.push_str(&format!(" -ArgumentList {list}"));
    }
    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
        .spawn()
        .map(|_| ())
        .context("не удалось запустить powershell для повышения прав")
}
