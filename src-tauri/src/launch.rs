//! Запуск игры. Игру запускает ТОЛЬКО лаунчер — после успешной проверки целостности.

use anyhow::{bail, Context, Result};
use l2_manifest::Manifest;
use std::path::Path;
use std::process::Command;

/// Запустить исполняемый файл клиента согласно манифесту.
/// `token` (Слой 2) пробрасывается в окружение игры для серверной проверки.
pub fn launch_game(install: &Path, manifest: &Manifest, token: Option<&str>) -> Result<()> {
    let exe = install.join(&manifest.launch.exe);
    if !exe.is_file() {
        bail!("исполняемый файл не найден: {}", exe.display());
    }
    let cwd = match &manifest.launch.cwd {
        Some(c) => install.join(c),
        None => exe.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| install.to_path_buf()),
    };

    let mut cmd = Command::new(&exe);
    cmd.current_dir(&cwd);
    cmd.args(&manifest.launch.args);
    if let Some(t) = token {
        // Слой 2: игровой клиент/сервер может проверить эту сессию.
        cmd.env("L2_SESSION_TOKEN", t);
    }

    cmd.spawn().with_context(|| format!("не удалось запустить {}", exe.display()))?;
    Ok(())
}
