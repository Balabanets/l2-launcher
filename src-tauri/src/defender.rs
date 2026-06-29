//! Однократное добавление папки игры в исключения Windows Defender.
//!
//! L2.exe — неподписанный игровой клиент Unreal Engine с MinHook (перехват WinAPI для
//! клиентской защиты). Эвристика/ML Defender ложно классифицирует такую связку как
//! троян и блокирует/карантинит файл → игра не запускается, лаунчер уходит в цикл
//! «недостающий файл → докачать → снова карантин». Статический анализ exe показал
//! отсутствие инъекций в чужие процессы, сети C2 и персистентности — это ложное
//! срабатывание (см. docs/AV_FALSE_POSITIVE.md).
//!
//! Решение без аккаунтов Microsoft и без действий игрока: лаунчер один раз добавляет
//! папку установки в исключения Defender (`Add-MpPreference -ExclusionPath`) с
//! повышением прав через UAC. На не-Windows — no-op.

use anyhow::Result;
use std::path::Path;

/// Добавить папку установки в исключения Windows Defender (с повышением прав).
/// Идемпотентно: повторный вызов для того же пути безвреден. Путь может ещё не
/// существовать — правило задаётся по пути, а не по содержимому.
#[cfg(windows)]
pub fn ensure_exclusion(install: &Path) -> Result<()> {
    use anyhow::Context;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let path = install.display().to_string();
    // Команда для элевированного powershell. Одинарные кавычки внутри single-quoted
    // строки PowerShell экранируются удвоением ('').
    let inner = format!("Add-MpPreference -ExclusionPath '{}'", path.replace('\'', "''"));
    // Внешний powershell поднимает второй с правами админа (UAC -Verb RunAs).
    // -Wait — чтобы дождаться завершения элевированного процесса; при отказе UAC
    // Start-Process бросает ошибку → внешний powershell вернёт ненулевой код.
    let outer = format!(
        "Start-Process powershell -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList \
         '-NoProfile','-WindowStyle','Hidden','-Command','{}'",
        inner.replace('\'', "''")
    );

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &outer]);
    cmd.creation_flags(CREATE_NO_WINDOW);

    let status = cmd
        .status()
        .context("не удалось запустить powershell для исключения Defender")?;
    if !status.success() {
        anyhow::bail!("добавление исключения Defender отклонено или не удалось");
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn ensure_exclusion(_install: &Path) -> Result<()> {
    Ok(())
}
