//! XDG directory resolution, temporary file management and safe output paths.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Application name used for XDG directories.
pub const APP_DIR: &str = "donustur";

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn xdg_dir(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home_dir().join(fallback))
}

/// `$XDG_CONFIG_HOME/donustur` (defaults to `~/.config/donustur`).
pub fn config_dir() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join(APP_DIR)
}

/// `$XDG_CACHE_HOME/donustur` (defaults to `~/.cache/donustur`).
pub fn cache_dir() -> PathBuf {
    xdg_dir("XDG_CACHE_HOME", ".cache").join(APP_DIR)
}

/// Directory used for intermediate conversion files.
pub fn tmp_dir() -> PathBuf {
    cache_dir().join("tmp")
}

/// Path of `config.toml`.
pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

/// Creates a directory (and its parents) if missing.
pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

/// Removes stale temporary files left behind by a previous crash.
///
/// The operation is best effort: unreadable entries are ignored. Only direct
/// children of the temporary directory are touched, never symlinked targets.
pub fn clean_tmp_dir() {
    let dir = tmp_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            let _ = std::fs::remove_dir_all(&path);
        } else {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Strips directory components from a user supplied file name.
pub fn safe_file_name(name: &str) -> Option<String> {
    let name = name.replace('\\', "/");
    let leaf = name.rsplit('/').next()?.trim();
    if leaf.is_empty() || leaf == "." || leaf == ".." {
        return None;
    }
    // NUL and control characters are never valid in a Linux file name.
    if leaf.chars().any(|c| c == '\0' || c.is_control()) {
        return None;
    }
    Some(leaf.to_string())
}

/// Produces a non-conflicting output path inside `dir`.
///
/// If `stem.ext` already exists, `stem-1.ext`, `stem-2.ext`, ... is returned.
/// The caller is still expected to create the file with `create_new` semantics
/// where possible; this function only resolves the common case.
#[cfg(test)]
pub fn unique_output_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let stem = sanitise_stem(stem);
    let ext = ext.trim_start_matches('.');
    let mut candidate = dir.join(format_name(&stem, ext));
    if !candidate.exists() {
        return candidate;
    }
    for index in 1..=u32::MAX {
        candidate = dir.join(format_name(&format!("{stem}-{index}"), ext));
        if !candidate.exists() {
            return candidate;
        }
    }
    // Practically unreachable; keep a deterministic fallback.
    dir.join(format_name(
        &format!("{stem}-{}-x", std::process::id()),
        ext,
    ))
}

/// Resolves collisions for a set of prospective names without touching disk.
///
/// This is useful when several jobs target the same directory at once. Names
/// already reserved by `taken` are skipped.
pub fn unique_output_path_reserved(
    dir: &Path,
    stem: &str,
    ext: &str,
    taken: &mut HashSet<PathBuf>,
) -> PathBuf {
    let mut index = 0u32;
    loop {
        let name = if index == 0 {
            format_name(&sanitise_stem(stem), ext)
        } else {
            format_name(&format!("{}-{index}", sanitise_stem(stem)), ext)
        };
        let candidate = dir.join(name);
        if !candidate.exists() && taken.insert(candidate.clone()) {
            return candidate;
        }
        index = index.saturating_add(1);
    }
}

fn sanitise_stem(stem: &str) -> String {
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "cikti".to_string()
    } else {
        trimmed
    }
}

fn format_name(stem: &str, ext: &str) -> String {
    if ext.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}.{ext}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_file_name_strips_directories() {
        assert_eq!(safe_file_name("/etc/passwd").as_deref(), Some("passwd"));
        assert_eq!(
            safe_file_name("../../evil.txt").as_deref(),
            Some("evil.txt")
        );
        assert_eq!(safe_file_name("normal.txt").as_deref(), Some("normal.txt"));
        assert_eq!(safe_file_name(".."), None);
        assert_eq!(safe_file_name(""), None);
    }

    #[test]
    fn unique_path_appends_counter() {
        let dir = tempfile::tempdir().expect("tempdir");
        let base = dir.path().join("foto.png");
        std::fs::write(&base, b"x").expect("write");
        let second = unique_output_path(dir.path(), "foto", "png");
        assert_eq!(second.file_name().unwrap(), "foto-1.png");
        std::fs::write(&second, b"x").expect("write");
        let third = unique_output_path(dir.path(), "foto", "png");
        assert_eq!(third.file_name().unwrap(), "foto-2.png");
    }

    #[test]
    fn unique_path_reserved_skips_reservations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut taken = HashSet::new();
        let a = unique_output_path_reserved(dir.path(), "a", "mp4", &mut taken);
        let b = unique_output_path_reserved(dir.path(), "a", "mp4", &mut taken);
        assert_eq!(a.file_name().unwrap(), "a.mp4");
        assert_eq!(b.file_name().unwrap(), "a-1.mp4");
    }

    #[test]
    fn sanitise_handles_weird_stems() {
        assert_eq!(sanitise_stem("..."), "cikti");
        assert_eq!(sanitise_stem("a/b"), "a_b");
    }
}
