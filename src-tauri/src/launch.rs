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

    // Важно: включаем САМУ причину ОС-ошибки в текст (anyhow по умолчанию её прячет).
    cmd.spawn().map_err(|e| {
        anyhow!(
            "не удалось запустить {} (рабочая папка {}): {e} [код ОС: {:?}]",
            exe.display(),
            cwd.display(),
            e.raw_os_error()
        )
    })?;
    Ok(())
}
