//! Minimal runtime localisation (Turkish/English).
//!
//! The language is resolved once at startup from `config.toml` and, when the
//! setting is `auto`, from the POSIX locale environment variables. No external
//! dependency or network access is involved.

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Tr,
    En,
}

static CURRENT: OnceLock<Lang> = OnceLock::new();

/// Resolves a config value (`auto`, `tr`, `en`) to a concrete language.
pub fn resolve(setting: &str) -> Lang {
    match setting.trim().to_ascii_lowercase().as_str() {
        "tr" | "tr_tr" | "turkish" | "türkçe" => Lang::Tr,
        "en" | "en_us" | "en_gb" | "english" => Lang::En,
        _ => detect(),
    }
}

/// Detects the language from the usual POSIX locale variables.
pub fn detect() -> Lang {
    detect_from(|key| std::env::var(key).ok())
}

/// Testable detector.
pub fn detect_from(mut get: impl FnMut(&str) -> Option<String>) -> Lang {
    for key in ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"] {
        let Some(value) = get(key) else { continue };
        if let Some(lang) = parse_locale(&value) {
            return lang;
        }
    }
    // This project is Turkish-first; an unknown locale stays Turkish.
    Lang::Tr
}

/// Parses values such as `tr_TR.UTF-8`, `en_US` or `de_DE:en`.
pub fn parse_locale(value: &str) -> Option<Lang> {
    let first = value.split(':').next().unwrap_or(value);
    let lower = first.to_ascii_lowercase();
    let base = lower.split(['.', '@']).next().unwrap_or(lower.as_str());
    if base.starts_with("tr") {
        Some(Lang::Tr)
    } else if base.starts_with("en") {
        Some(Lang::En)
    } else {
        None
    }
}

/// Installs the resolved language. Subsequent calls are ignored.
pub fn set(lang: Lang) {
    let _ = CURRENT.set(lang);
}

pub fn current() -> Lang {
    *CURRENT.get().unwrap_or(&Lang::Tr)
}

/// Plural-aware word for "file".
pub fn files_label(count: usize) -> &'static str {
    match current() {
        Lang::Tr => "dosya",
        Lang::En if count == 1 => "file",
        Lang::En => "files",
    }
}

/// Plural-aware word for "type".
pub fn types_label(count: usize) -> &'static str {
    match current() {
        Lang::Tr => "tür",
        Lang::En if count == 1 => "type",
        Lang::En => "types",
    }
}

/// Returns the string matching the active language.
pub fn t(en: &'static str, tr: &'static str) -> &'static str {
    match current() {
        Lang::Tr => tr,
        Lang::En => en,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_parsing_handles_variants() {
        assert_eq!(parse_locale("tr_TR.UTF-8"), Some(Lang::Tr));
        assert_eq!(parse_locale("tr"), Some(Lang::Tr));
        assert_eq!(parse_locale("en_US.UTF-8"), Some(Lang::En));
        assert_eq!(parse_locale("en_GB@euro"), Some(Lang::En));
        assert_eq!(parse_locale("de_DE.UTF-8"), None);
        assert_eq!(parse_locale("C"), None);
    }

    #[test]
    fn detection_prefers_the_first_usable_variable() {
        let lang = detect_from(|key| match key {
            "LC_ALL" => Some("de_DE.UTF-8".into()),
            "LC_MESSAGES" => Some("en_US.UTF-8".into()),
            _ => None,
        });
        assert_eq!(lang, Lang::En);
    }

    #[test]
    fn detection_falls_back_to_turkish() {
        assert_eq!(detect_from(|_| None), Lang::Tr);
    }

    #[test]
    fn explicit_resolution_wins() {
        assert_eq!(resolve("tr"), Lang::Tr);
        assert_eq!(resolve("en"), Lang::En);
    }
}
