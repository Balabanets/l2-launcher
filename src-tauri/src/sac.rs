//! Обнаружение Smart App Control (SAC) — защиты Windows 11, которая блокирует
//! неподписанные программы на этапе загрузки. SAC нельзя настроить исключениями и
//! нельзя выключить из приложения (Microsoft это запрещает) — поэтому лаунчер может
//! только обнаружить SAC и провести игрока к ручному выключению. См.
//! docs/SMART_APP_CONTROL.md.
//!
//! Состояние читается из реестра:
//!   HKLM\SYSTEM\CurrentControlSet\Control\CI\Policy\VerifiedAndReputablePolicyState
//!     0 = выключен, 1 = включён (принуждение), 2 = режим оценки.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SacState {
    /// Выключен или недоступен (не Windows 11 / старая система).
    Off,
    /// Включён и принудительно блокирует неподписанный код.
    On,
    /// Режим оценки: Windows ещё решает, включать ли SAC.
    Evaluation,
    /// Значение есть, но не распознано.
    Unknown,
}

#[cfg(windows)]
pub fn state() -> SacState {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let val = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SYSTEM\CurrentControlSet\Control\CI\Policy")
        .and_then(|k| k.get_value::<u32, _>("VerifiedAndReputablePolicyState"));
    match val {
        Ok(0) => SacState::Off,
        Ok(1) => SacState::On,
        Ok(2) => SacState::Evaluation,
        Ok(_) => SacState::Unknown,
        // Ключа нет → система без SAC (Windows 10 и старше) → считаем выключенным.
        Err(_) => SacState::Off,
    }
}

#[cfg(not(windows))]
pub fn state() -> SacState {
    SacState::Off
}

/// Жёстко ли SAC блокирует запуск неподписанного клиента (принуждение).
/// Режим оценки не блокируем — там игра может запуститься, а режим авто-разрешается.
pub fn is_blocking(s: SacState) -> bool {
    matches!(s, SacState::On)
}

/// Открыть страницу «Управление приложениями и браузером» в «Безопасности Windows»,
/// где находится переключатель Smart App Control. Best-effort.
#[cfg(windows)]
pub fn open_settings() -> anyhow::Result<()> {
    use anyhow::Context;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // start "" "<uri>" — открывает URI обработчиком по умолчанию (Безопасность Windows).
    let mut cmd = Command::new("cmd");
    cmd.args(["/c", "start", "", "windowsdefender://appbrowser"]);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.spawn().context("не удалось открыть Безопасность Windows")?;
    Ok(())
}

#[cfg(not(windows))]
pub fn open_settings() -> anyhow::Result<()> {
    Ok(())
}
