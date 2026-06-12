//! Portable со стабильным «домом».
//!
//! Проблема: portable-exe запускают из Загрузок/где угодно; самообновление
//! (`self_replace`) подменяет файл, и пользовательский ярлык может «протухнуть».
//!
//! Решение: при первом запуске НЕ из «дома» копируем себя в
//! `%LOCALAPPDATA%\L2 Interlude\L2 Interlude.exe`, создаём ярлыки (Рабочий стол +
//! меню Пуск) на этот стабильный путь и перезапускаемся оттуда. Самообновление
//! затем меняет именно домашний exe — путь не меняется, ярлык остаётся рабочим.

#[cfg(windows)]
pub fn ensure_installed() {
    use std::path::PathBuf;

    let Ok(cur) = std::env::current_exe() else { return };
    let Some(local) = std::env::var_os("LOCALAPPDATA") else { return };
    let home_dir = PathBuf::from(local).join("L2 Interlude");
    let home_exe = home_dir.join("L2 Interlude.exe");

    let same = |a: &PathBuf, b: &PathBuf| {
        let na = a.canonicalize().unwrap_or_else(|_| a.clone());
        let nb = b.canonicalize().unwrap_or_else(|_| b.clone());
        na == nb
    };

    // Уже запущены из «дома» — только освежим ярлыки (на случай ручного удаления).
    if same(&cur, &home_exe) {
        let _ = create_shortcuts(&home_exe);
        return;
    }

    // Иначе — устанавливаемся в «дом» и перезапускаемся оттуда. Любая ошибка
    // (нет прав и т.п.) → просто продолжаем работать из текущего места.
    if std::fs::create_dir_all(&home_dir).is_err() {
        return;
    }
    if std::fs::copy(&cur, &home_exe).is_err() {
        return;
    }
    let _ = create_shortcuts(&home_exe);
    if std::process::Command::new(&home_exe).spawn().is_ok() {
        std::process::exit(0);
    }
}

#[cfg(windows)]
fn create_shortcuts(target: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    use mslnk::ShellLink;
    use std::path::PathBuf;

    let make = |dir: PathBuf| {
        let _ = std::fs::create_dir_all(&dir);
        let lnk = dir.join("L2 Interlude.lnk");
        if let Ok(sl) = ShellLink::new(target) {
            let _ = sl.create_lnk(&lnk);
        }
    };

    if let Some(up) = std::env::var_os("USERPROFILE") {
        make(PathBuf::from(up).join("Desktop"));
    }
    if let Some(ad) = std::env::var_os("APPDATA") {
        make(
            PathBuf::from(ad)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
        );
    }
    Ok(())
}

/// На не-Windows — ничего не делаем (dev на Linux/Mac).
#[cfg(not(windows))]
pub fn ensure_installed() {}
