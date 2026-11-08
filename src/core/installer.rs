//! Installation of missing external tools via `pkexec` + dnf/pacman.
//!
//! SECURITY: only fixed package names from an allow-list are ever passed to the
//! package manager. User input is never interpolated into a command line, and
//! `sudo`/`sh` are never used.

use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio::process::Command;

use crate::core::detector::{PackageManager, Tool};

/// Per-distribution package names. Keep this table the single source of truth.
pub fn packages_for(manager: PackageManager, tool: Tool) -> &'static [&'static str] {
    match (manager, tool) {
        // Fedora ships `ffmpeg-free` in the official repositories; it installs
        // the same `ffmpeg` binary. RPM Fusion users may replace it manually.
        (PackageManager::Dnf, Tool::Ffmpeg) => &["ffmpeg-free"],
        (PackageManager::Dnf, Tool::LibreOffice) => &["libreoffice"],
        (PackageManager::Dnf, Tool::Pandoc) => &["pandoc"],
        (PackageManager::Dnf, Tool::SevenZip) => &["p7zip"],

        (PackageManager::Pacman, Tool::Ffmpeg) => &["ffmpeg"],
        (PackageManager::Pacman, Tool::LibreOffice) => &["libreoffice-fresh"],
        (PackageManager::Pacman, Tool::Pandoc) => &["pandoc"],
        (PackageManager::Pacman, Tool::SevenZip) => &["p7zip"],
    }
}

/// Outcome of an installation attempt.
#[derive(Debug, Clone)]
pub struct InstallReport {
    pub success: bool,
    pub summary: String,
}

/// Builds the `pkexec` argument vector for the given tools.
///
/// Returns an error for an empty tool list or if a package name is not in the
/// allow-list (defence in depth against future refactors).
pub fn install_args(manager: PackageManager, tools: &[Tool]) -> Result<Vec<String>> {
    if tools.is_empty() {
        bail!("kurulacak araç seçilmedi");
    }
    let mut packages: Vec<&'static str> = Vec::new();
    for tool in tools {
        for pkg in packages_for(manager, *tool) {
            if !packages.contains(pkg) {
                packages.push(pkg);
            }
        }
    }
    if packages.is_empty() {
        bail!("paket listesi boş");
    }
    for pkg in &packages {
        if !is_safe_package_name(pkg) {
            bail!("geçersiz paket adı: {pkg}");
        }
    }

    let mut args: Vec<String> = vec![manager.binary().to_string()];
    match manager {
        PackageManager::Dnf => {
            args.extend(["install", "-y", "--setopt=install_weak_deps=False"].map(String::from));
        }
        PackageManager::Pacman => {
            args.extend(["-S", "--noconfirm", "--needed"].map(String::from));
        }
    }
    // `--` terminates option parsing; package names are fixed anyway.
    args.push("--".to_string());
    args.extend(packages.into_iter().map(String::from));
    Ok(args)
}

fn is_safe_package_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '+'))
}

/// Installs the given tools through a single polkit prompt.
pub async fn install(
    manager: PackageManager,
    tools: &[Tool],
    timeout: Duration,
) -> Result<InstallReport> {
    let args = install_args(manager, tools)?;

    let mut cmd = Command::new("pkexec");
    cmd.args(&args)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env_remove("LD_PRELOAD")
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("LD_AUDIT")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(result) => result.context("pkexec başlatılamadı")?,
        Err(_) => {
            return Ok(InstallReport {
                success: false,
                summary: "Kurulum zaman aşımına uğradı; polkit penceresi kapatıldı.".to_string(),
            })
        }
    };

    let mut text = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout);
    if text.is_empty() {
        text = stdout.trim().to_string();
    }
    // Keep the report short and free of raw file data.
    let summary = summarise(&text, output.status.success());
    Ok(InstallReport {
        success: output.status.success(),
        summary,
    })
}

fn summarise(text: &str, success: bool) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let tail: Vec<&str> = lines.iter().rev().take(8).copied().collect();
    let mut summary = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
    if summary.len() > 2000 {
        summary.truncate(2000);
    }
    if summary.is_empty() {
        summary = if success {
            "Kurulum tamamlandı.".to_string()
        } else {
            "Kurulum başarısız oldu.".to_string()
        };
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dnf_args_use_allow_listed_packages() {
        let args = install_args(PackageManager::Dnf, &[Tool::Ffmpeg]).unwrap();
        assert!(args[0] == "dnf" || args[0] == "dnf5");
        assert!(args.contains(&"install".to_string()));
        assert!(args.contains(&"-y".to_string()));
        assert!(args.contains(&"--".to_string()));
        assert!(args.contains(&"ffmpeg-free".to_string()));
        assert!(!args.iter().any(|a| a.contains(';')));
    }

    #[test]
    fn pacman_args_suppress_prompts() {
        let args = install_args(
            PackageManager::Pacman,
            &[Tool::Ffmpeg, Tool::Pandoc, Tool::LibreOffice],
        )
        .unwrap();
        assert_eq!(args[0], "pacman");
        assert!(args.contains(&"--noconfirm".to_string()));
        assert!(args.contains(&"--needed".to_string()));
        assert!(args.contains(&"libreoffice-fresh".to_string()));
        assert!(args.contains(&"pandoc".to_string()));
        assert!(args.contains(&"ffmpeg".to_string()));
    }

    #[test]
    fn duplicate_packages_are_removed() {
        let args = install_args(
            PackageManager::Dnf,
            &[Tool::Ffmpeg, Tool::Ffmpeg, Tool::Pandoc],
        )
        .unwrap();
        assert_eq!(args.iter().filter(|a| *a == "ffmpeg-free").count(), 1);
    }

    #[test]
    fn empty_tool_list_is_rejected() {
        assert!(install_args(PackageManager::Dnf, &[]).is_err());
    }

    #[test]
    fn package_validation_blocks_injection() {
        assert!(is_safe_package_name("ffmpeg-free"));
        assert!(!is_safe_package_name("-rm"));
        assert!(!is_safe_package_name("a;b"));
        assert!(!is_safe_package_name("a b"));
        assert!(!is_safe_package_name(""));
    }

    #[test]
    fn summary_is_bounded() {
        let long = "x".repeat(5000);
        let summary = summarise(&long, false);
        assert!(summary.len() <= 2000);
    }
}
