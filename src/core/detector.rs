//! External tool and package manager detection.

use std::path::{Path, PathBuf};

/// External conversion engines. They are never bundled with the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Ffmpeg,
    LibreOffice,
    Pandoc,
    SevenZip,
}

impl Tool {
    pub const ALL: [Tool; 4] = [
        Tool::Ffmpeg,
        Tool::LibreOffice,
        Tool::Pandoc,
        Tool::SevenZip,
    ];

    /// Stable identifier used in `config.toml`.
    pub fn id(self) -> &'static str {
        match self {
            Tool::Ffmpeg => "ffmpeg",
            Tool::LibreOffice => "soffice",
            Tool::Pandoc => "pandoc",
            Tool::SevenZip => "7z",
        }
    }

    /// Executable looked up in `PATH`.
    pub fn binary(self) -> &'static str {
        self.id()
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Tool::Ffmpeg => "FFmpeg (ses/video)",
            Tool::LibreOffice => "LibreOffice (belge)",
            Tool::Pandoc => "Pandoc (belge)",
            Tool::SevenZip => "7-Zip (arşiv)",
        }
    }

    /// FFmpeg is required for audio/video; the others are optional.
    pub fn is_critical(self) -> bool {
        matches!(self, Tool::Ffmpeg)
    }

    #[cfg(test)]
    pub fn from_id(id: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.id() == id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolStatus {
    pub tool: Tool,
    pub path: Option<PathBuf>,
}

impl ToolStatus {
    pub fn available(&self) -> bool {
        self.path.is_some()
    }
}

/// Finds an executable in the current `PATH`.
pub fn find_binary(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    find_binary_in(binary, std::env::split_paths(&path))
}

/// Testable lookup helper. Returns the first executable file match.
pub fn find_binary_in<I>(binary: &str, dirs: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    if binary.is_empty() || binary.contains('/') {
        return None;
    }
    for dir in dirs {
        let candidate = dir.join(binary);
        if is_executable(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() && meta.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

/// Checks a single tool.
pub fn detect(tool: Tool) -> ToolStatus {
    ToolStatus {
        tool,
        path: find_binary(tool.binary()),
    }
}

/// Checks every supported tool.
pub fn detect_all() -> Vec<ToolStatus> {
    Tool::ALL.into_iter().map(detect).collect()
}

/// Supported package managers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageManager {
    Dnf,
    Pacman,
}

impl PackageManager {
    pub fn display_name(self) -> &'static str {
        match self {
            PackageManager::Dnf => "dnf",
            PackageManager::Pacman => "pacman",
        }
    }

    /// Executable used for installs (first match wins).
    pub fn binary(self) -> &'static str {
        match self {
            PackageManager::Dnf => {
                if find_binary("dnf").is_some() {
                    "dnf"
                } else {
                    "dnf5"
                }
            }
            PackageManager::Pacman => "pacman",
        }
    }
}

/// Detects `dnf`/`dnf5` (Fedora) or `pacman` (Arch) via `PATH`.
pub fn detect_package_manager() -> Option<PackageManager> {
    if find_binary("dnf").is_some() || find_binary("dnf5").is_some() {
        return Some(PackageManager::Dnf);
    }
    if find_binary("pacman").is_some() {
        return Some(PackageManager::Pacman);
    }
    None
}

/// Testable variant that inspects explicit binary directories.
#[cfg(test)]
pub fn detect_package_manager_in<I>(dirs: I) -> Option<PackageManager>
where
    I: IntoIterator<Item = PathBuf> + Clone,
{
    let dirs: Vec<PathBuf> = dirs.into_iter().collect();
    if find_binary_in("dnf", dirs.iter().cloned()).is_some()
        || find_binary_in("dnf5", dirs.iter().cloned()).is_some()
    {
        return Some(PackageManager::Dnf);
    }
    if find_binary_in("pacman", dirs.iter().cloned()).is_some() {
        return Some(PackageManager::Pacman);
    }
    None
}

/// Names of missing tools, in stable order.
pub fn missing_tools(statuses: &[ToolStatus]) -> Vec<Tool> {
    statuses
        .iter()
        .filter(|s| !s.available())
        .map(|s| s.tool)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn make_exe(dir: &Path, name: &str) {
        let path = dir.join(name);
        std::fs::write(&path, b"#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }

    #[test]
    fn tool_ids_are_stable_and_parsable() {
        for tool in Tool::ALL {
            assert_eq!(Tool::from_id(tool.id()), Some(tool));
        }
        assert_eq!(Tool::from_id("yok"), None);
    }

    #[test]
    fn find_binary_requires_executable() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("ffmpeg");
        std::fs::write(&plain, b"x").unwrap();
        assert!(find_binary_in("ffmpeg", [dir.path().to_path_buf()]).is_none());
        make_exe(dir.path(), "ffmpeg");
        assert!(find_binary_in("ffmpeg", [dir.path().to_path_buf()]).is_some());
    }

    #[test]
    fn find_binary_rejects_paths() {
        let dir = tempfile::tempdir().unwrap();
        make_exe(dir.path(), "ffmpeg");
        assert!(find_binary_in("./ffmpeg", [dir.path().to_path_buf()]).is_none());
    }

    #[test]
    fn package_manager_detection_prefers_dnf() {
        let dir = tempfile::tempdir().unwrap();
        make_exe(dir.path(), "pacman");
        assert_eq!(
            detect_package_manager_in([dir.path().to_path_buf()]),
            Some(PackageManager::Pacman)
        );
        make_exe(dir.path(), "dnf5");
        assert_eq!(
            detect_package_manager_in([dir.path().to_path_buf()]),
            Some(PackageManager::Dnf)
        );
    }
}
