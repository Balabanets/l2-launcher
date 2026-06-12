//! Классификация файлов клиента по группам (`_launcher/groups/*.txt`).
//! Единый источник правды для генератора манифеста и лаунчера.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Class {
    /// Обычный хэш-синк.
    Managed,
    /// Языковой набор (качается по требованию). group = "lang-ru" / "lang-en".
    Optional(String),
    /// Дефолт: писать только если файла нет; при апдейте не перезаписывать.
    SeedOnce,
    /// Состоянием владеет лаунчер (perf/язык); переприменяется после апдейта.
    LauncherOwned,
    /// Не входит в манифест (preserve / d3d8 / dgVoodoo / system/WindowsInfo.ini).
    Excluded,
}

#[derive(Debug, Default, Clone)]
pub struct GroupLists {
    pub lang_ru: Vec<String>,
    pub lang_en: Vec<String>,
    pub seed_once: Vec<String>,
    pub preserve_files: Vec<String>,
    pub preserve_dirs: Vec<String>,
    pub launcher_owned: Vec<String>,
}

/// Нормализация строки списка: убрать inline-комментарий, ведущий '/', привести разделитель.
fn norm(line: &str) -> Option<String> {
    let s = line.split('#').next().unwrap_or("").trim();
    if s.is_empty() {
        return None;
    }
    Some(s.trim_start_matches('/').replace('\\', "/"))
}

impl GroupLists {
    /// Загрузить списки из `<launcher_dir>/groups/*.txt`.
    pub fn load(launcher_dir: &Path) -> Self {
        let g = launcher_dir.join("groups");
        let read = |name: &str| -> Vec<String> {
            std::fs::read_to_string(g.join(name))
                .unwrap_or_default()
                .lines()
                .filter_map(norm)
                .collect()
        };
        let mut gl = GroupLists {
            lang_ru: read("lang-ru.txt"),
            lang_en: read("lang-en.txt"),
            seed_once: read("seed-once.txt"),
            launcher_owned: read("launcher-owned.txt"),
            ..Default::default()
        };
        for p in read("preserve.txt") {
            if p.ends_with('/') {
                gl.preserve_dirs.push(p);
            } else {
                gl.preserve_files.push(p);
            }
        }
        gl
    }

    /// Классифицировать относительный путь (через '/', без ведущего '/').
    pub fn classify(&self, rel: &str) -> Class {
        let r = rel.replace('\\', "/");
        let lower = r.to_ascii_lowercase();

        // 1. Файлы/папки игрока — никогда в манифест.
        if self.preserve_files.iter().any(|p| p.eq_ignore_ascii_case(&r))
            || self
                .preserve_dirs
                .iter()
                .any(|d| lower.starts_with(&d.to_ascii_lowercase()))
        {
            return Class::Excluded;
        }
        // 2. d3d8/dgVoodoo (только из perf) и system/WindowsInfo.ini (seed из _launcher/defaults).
        if lower == "system/d3d8.dll"
            || lower == "system/dgvoodoo.conf"
            || lower == "system/windowsinfo.ini"
        {
            return Class::Excluded;
        }
        // 3. launcher-owned (l2.ini, Localization.ini).
        if self.launcher_owned.iter().any(|p| p.eq_ignore_ascii_case(&r)) {
            return Class::LauncherOwned;
        }
        // 4. Языковые наборы.
        if self.lang_ru.iter().any(|p| p.eq_ignore_ascii_case(&r)) {
            return Class::Optional("lang-ru".into());
        }
        if self.lang_en.iter().any(|p| p.eq_ignore_ascii_case(&r)) {
            return Class::Optional("lang-en".into());
        }
        // 5. seed-once.
        if self.seed_once.iter().any(|p| p.eq_ignore_ascii_case(&r)) {
            return Class::SeedOnce;
        }
        Class::Managed
    }
}
