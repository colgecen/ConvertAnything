//! User-level installation: copy the AppImage to a stable location, register
//! the desktop/context-menu integration and (optionally) start at login.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::integration;
use crate::util::paths;

/// Result of a successful installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    pub executable: PathBuf,
    pub desktop: PathBuf,
    pub service_menus: usize,
    pub autostart: Option<PathBuf>,
    pub copied: bool,
}

fn installed_name(source: &Path) -> &'static str {
    let appimage = source
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("AppImage"))
        .unwrap_or(false)
        || std::env::var_os("APPIMAGE").is_some();
    if appimage {
        "Donustur.AppImage"
    } else {
        "donustur"
    }
}

/// The executable path used by autostart when the app was installed before.
pub fn installed_executable() -> Option<PathBuf> {
    for name in ["Donustur.AppImage", "donustur"] {
        let candidate = paths::bin_dir().join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn copy_executable(source: &Path, dest: &Path) -> Result<bool> {
    if !source.is_file() {
        bail!(
            "{}",
            crate::i18n::t(
                "The running executable could not be found.",
                "Çalışan uygulama dosyası bulunamadı."
            )
        );
    }
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(source), std::fs::canonicalize(dest)) {
        if a == b {
            return Ok(false);
        }
    }
    let parent = dest.parent().context("kurulum dizini belirlenemedi")?;
    paths::ensure_dir(parent)
        .with_context(|| format!("kurulum dizini oluşturulamadı: {}", parent.display()))?;

    let mut temp =
        tempfile::NamedTempFile::new_in(parent).context("geçici kurulum dosyası oluşturulamadı")?;
    let mut input = std::fs::File::open(source)
        .with_context(|| format!("uygulama dosyası açılamadı: {}", source.display()))?;
    std::io::copy(&mut input, &mut temp).context("uygulama dosyası kopyalanamadı")?;
    temp.as_file()
        .sync_all()
        .context("kopya diske yazılamadı")?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755))
        .context("uygulama izni ayarlanamadı")?;
    temp.persist(dest)
        .map_err(|error| error.error)
        .with_context(|| format!("uygulama yerine taşınamadı: {}", dest.display()))?;
    Ok(true)
}

fn autostart_entry(exec: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=ConvertAnything\n\
         Name[tr]=Dönüştür\n\
         GenericName=File Converter\n\
         GenericName[tr]=Dosya Dönüştürücü\n\
         Comment=Image, audio, video, document and archive converter\n\
         Comment[tr]=Görsel, ses, video, belge ve arşiv dönüştürücü\n\
         Exec={}\n\
         Icon=donustur\n\
         Terminal=false\n\
         StartupWMClass=donustur\n\
         X-GNOME-Autostart-enabled=true\n",
        integration::exec_quote(exec)
    )
}

/// Writes an autostart entry into `config_home/autostart`.
pub fn set_autostart_at(config_home: &Path, exec: &Path, enabled: bool) -> Result<PathBuf> {
    let dir = config_home.join("autostart");
    let path = dir.join("donustur.desktop");
    if enabled {
        paths::ensure_dir(&dir).with_context(|| {
            format!("otomatik başlatma dizini oluşturulamadı: {}", dir.display())
        })?;
        std::fs::write(&path, autostart_entry(exec))
            .with_context(|| format!("otomatik başlatma dosyası yazılamadı: {}", path.display()))?;
    } else if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("otomatik başlatma dosyası silinemedi: {}", path.display()))?;
    }
    Ok(path)
}

/// True when the login autostart entry is installed.
pub fn autostart_enabled() -> bool {
    paths::autostart_file().is_file()
}

/// Enables/disables login autostart for the installed (or running) app.
pub fn set_autostart(enabled: bool) -> Result<PathBuf> {
    let exec = installed_executable().unwrap_or_else(integration::executable_path);
    set_autostart_at(&paths::config_home(), &exec, enabled)
}

/// Installs the app under `bin_dir`, writes integration files and optionally
/// enables login autostart. Testable variant with explicit directories.
pub fn install_at(
    bin_dir: &Path,
    data_home: &Path,
    config_home: &Path,
    source: &Path,
    enable_autostart: bool,
) -> Result<InstallReport> {
    let dest = bin_dir.join(installed_name(source));
    let copied = copy_executable(source, &dest)?;
    let integration = integration::install_to(data_home, &dest)?;
    let autostart = if enable_autostart {
        Some(set_autostart_at(config_home, &dest, true)?)
    } else {
        None
    };
    Ok(InstallReport {
        executable: dest,
        desktop: integration.desktop,
        service_menus: integration.service_menus.len(),
        autostart,
        copied,
    })
}

/// Installs the running app for the current user.
pub fn install_app(enable_autostart: bool) -> Result<InstallReport> {
    let source = integration::executable_path();
    install_at(
        &paths::bin_dir(),
        &paths::data_home(),
        &paths::config_home(),
        &source,
        enable_autostart,
    )
}

/// Removes the files created by [`install_app`].
pub fn uninstall() -> Result<()> {
    let _ = set_autostart(false);
    for name in ["Donustur.AppImage", "donustur"] {
        let path = paths::bin_dir().join(name);
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }

    let applications = paths::data_home().join("applications/donustur.desktop");
    let _ = std::fs::remove_file(applications);
    let icon = paths::data_home().join("icons/hicolor/256x256/apps/donustur.png");
    let _ = std::fs::remove_file(icon);
    let servicemenus = paths::data_home().join("kio/servicemenus");
    if let Ok(entries) = std::fs::read_dir(&servicemenus) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("donustur-convert") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_names_depend_on_extension() {
        assert_eq!(
            installed_name(Path::new("Donustur.AppImage")),
            "Donustur.AppImage"
        );
        assert_eq!(installed_name(Path::new("donustur")), "donustur");
    }

    #[test]
    fn install_copies_binary_and_writes_autostart() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Kaynak.AppImage");
        std::fs::write(&source, b"appimage-bytes").unwrap();
        let bin = dir.path().join("bin");
        let data = dir.path().join("data");
        let config = dir.path().join("config");

        let report = install_at(&bin, &data, &config, &source, true).unwrap();
        assert!(report.copied);
        assert_eq!(report.executable, bin.join("Donustur.AppImage"));
        assert_eq!(
            std::fs::read(&report.executable).unwrap(),
            b"appimage-bytes"
        );
        use std::os::unix::fs::PermissionsExt;
        assert!(
            std::fs::metadata(&report.executable)
                .unwrap()
                .permissions()
                .mode()
                & 0o111
                != 0
        );
        let autostart = report.autostart.unwrap();
        let text = std::fs::read_to_string(autostart).unwrap();
        assert!(text.contains("X-GNOME-Autostart-enabled=true"));
        assert!(text.contains(&integration::exec_quote(&report.executable)));
        assert!(integration::is_installed_for(&report.executable, &data));
        assert!(report.service_menus > 0);
    }

    #[test]
    fn autostart_can_be_removed() {
        let dir = tempfile::tempdir().unwrap();
        let exec = dir.path().join("Donustur.AppImage");
        std::fs::write(&exec, b"x").unwrap();
        let config = dir.path().join("config");
        let path = set_autostart_at(&config, &exec, true).unwrap();
        assert!(path.is_file());
        set_autostart_at(&config, &exec, false).unwrap();
        assert!(!path.exists());
    }
}
