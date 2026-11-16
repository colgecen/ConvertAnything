//! Desktop integration: application entry and file-manager context menu.
//!
//! Installation only writes files below `$XDG_DATA_HOME` (three small files)
//! and never runs a shell or touches the network.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::core::registry::{self, Format};
use crate::i18n::t;
use crate::util::paths;

/// AppImage path when running from an AppImage, otherwise the real binary.
pub fn executable_path() -> PathBuf {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_else(|| PathBuf::from("donustur"))
}

/// Maps registry format ids to MIME types used by file managers.
const FORMAT_MIMES: &[(&str, &str)] = &[
    ("jpg", "image/jpeg"),
    ("png", "image/png"),
    ("webp", "image/webp"),
    ("bmp", "image/bmp"),
    ("tiff", "image/tiff"),
    ("gif", "image/gif"),
    ("ico", "image/x-icon"),
    ("mp3", "audio/mpeg"),
    ("wav", "audio/x-wav"),
    ("flac", "audio/flac"),
    ("ogg", "audio/ogg"),
    ("opus", "audio/ogg"),
    ("aac", "audio/aac"),
    ("m4a", "audio/mp4"),
    ("mp4", "video/mp4"),
    ("mkv", "video/x-matroska"),
    ("webm", "video/webm"),
    ("avi", "video/x-msvideo"),
    ("mov", "video/quicktime"),
    ("pdf", "application/pdf"),
    (
        "docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    ("odt", "application/vnd.oasis.opendocument.text"),
    ("md", "text/markdown"),
    ("html", "text/html"),
    ("epub", "application/epub+zip"),
    ("txt", "text/plain"),
    ("csv", "text/csv"),
    ("zip", "application/zip"),
    ("tar.gz", "application/x-compressed-tar"),
    ("tar", "application/x-tar"),
];

/// MIME type for a registry format, if known.
pub fn mime_for(format_id: &str) -> Option<&'static str> {
    FORMAT_MIMES
        .iter()
        .find(|(id, _)| *id == format_id)
        .map(|(_, mime)| *mime)
}

/// Semicolon separated MIME list for `.desktop` files.
pub fn mime_list() -> String {
    let mut seen = Vec::new();
    for (_, mime) in FORMAT_MIMES {
        if !seen.contains(mime) {
            seen.push(*mime);
        }
    }
    format!("{};", seen.join(";"))
}

/// Quotes a path for the `Exec=` key (desktop entry escaping rules).
pub fn exec_quote(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let mut escaped = String::with_capacity(raw.len() + 2);
    escaped.push('"');
    for ch in raw.chars() {
        if matches!(ch, '"' | '`' | '$' | '\\') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped.push('"');
    escaped
}

fn desktop_entry(exec: &Path) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=ConvertAnything\n\
         Name[tr]=Dönüştür\n\
         GenericName=File Converter\n\
         GenericName[tr]=Dosya Dönüştürücü\n\
         Comment=Image, audio, video, document and archive converter\n\
         Comment[tr]=Görsel, ses, video, belge ve arşiv dönüştürücü\n\
         Exec={} %F\n\
         MimeType={}\n\
         Icon=donustur\n\
         Terminal=false\n\
         Categories=Utility;Office;Graphics;AudioVideo;\n\
         Keywords=convert;converter;donustur;image;photo;audio;video;document;pdf;archive;zip;ffmpeg;offline;\n\
         StartupWMClass=donustur\n",
        exec_quote(exec),
        mime_list()
    )
}

fn action_key(id: &str) -> String {
    id.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn action_name(id: &str) -> String {
    if id == registry::EXTRACT {
        t("Extract archive", "Arşivi ayıkla").to_string()
    } else if let Some(format) = registry::format_by_id(id) {
        format!("{} (.{})", format.display_label(), format.id)
    } else {
        id.to_string()
    }
}

/// Builds one service menu per MIME type.
///
/// KDE applies the menu whose `MimeType` matches the clicked file, so the
/// submenu only lists formats that the clicked file can actually become. No
/// extra chooser window is needed: choosing an entry converts immediately.
fn service_menus(exec: &Path) -> Vec<(String, String)> {
    let mut groups: Vec<(&'static str, Vec<&'static Format>)> = Vec::new();
    for format in registry::FORMATS {
        let Some(mime) = mime_for(format.id) else {
            continue;
        };
        if let Some(group) = groups.iter_mut().find(|(known, _)| *known == mime) {
            group.1.push(format);
        } else {
            groups.push((mime, vec![format]));
        }
    }

    let mut menus = Vec::new();
    for (mime, formats) in groups {
        let mut targets: Vec<&'static Format> = Vec::new();
        for source in &formats {
            for target in registry::targets_for(source) {
                if !targets.iter().any(|known| known.id == target.id) {
                    targets.push(target);
                }
            }
        }
        // A single-format MIME type never needs a conversion to itself.
        if formats.len() == 1 {
            targets.retain(|target| target.id != formats[0].id);
        }
        if targets.is_empty() {
            continue;
        }

        let keys = targets
            .iter()
            .map(|target| action_key(target.id))
            .collect::<Vec<_>>()
            .join(";");
        let mut content = format!(
            "[Desktop Entry]\n\
             Type=Service\n\
             ServiceTypes=KonqPopupMenu/Plugin\n\
             MimeType={mime};\n\
             Actions={keys};\n\
             X-KDE-ServiceTypes=KonqPopupMenu/Plugin\n\
             X-KDE-Priority=TopLevel\n\
             X-KDE-Ordering=60\n\
             X-KDE-Submenu=Convert with ConvertAnything\n\
             X-KDE-Submenu[tr]=ConvertAnything ile dönüştür\n\
             Icon=donustur\n"
        );
        for target in &targets {
            let name = action_name(target.id);
            content.push_str(&format!(
                "\n[Desktop Action {}]\n\
                 Name={name}\n\
                 Name[tr]={name}\n\
                 Exec={} --convert %F --to {}\n\
                 Icon=donustur\n",
                action_key(target.id),
                exec_quote(exec),
                target.id
            ));
        }

        let slug: String = mime
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect();
        menus.push((format!("donustur-convert-{slug}.desktop"), content));
    }
    menus
}

/// Paths written by [`install_to`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub desktop: PathBuf,
    pub service_menus: Vec<PathBuf>,
    pub icon: PathBuf,
}

/// Installs the desktop entry and KDE service menus under `data_home`.
pub fn install_to(data_home: &Path, exec: &Path) -> Result<Installed> {
    let applications = data_home.join("applications");
    let servicemenus = data_home.join("kio/servicemenus");
    let icons = data_home.join("icons/hicolor/512x512/apps");
    for dir in [&applications, &servicemenus, &icons] {
        paths::ensure_dir(dir)
            .with_context(|| format!("dizin oluşturulamadı: {}", dir.display()))?;
    }

    let desktop = applications.join("donustur.desktop");
    let icon = icons.join("donustur.png");
    std::fs::write(&desktop, desktop_entry(exec))
        .with_context(|| format!("masaüstü dosyası yazılamadı: {}", desktop.display()))?;
    std::fs::write(&icon, include_bytes!("../assets/icon.png"))
        .with_context(|| format!("simge yazılamadı: {}", icon.display()))?;
    // Remove the 256 px icon written by older versions.
    let _ = std::fs::remove_file(data_home.join("icons/hicolor/256x256/apps/donustur.png"));

    let menus = service_menus(exec);
    let expected: Vec<&str> = menus.iter().map(|(name, _)| name.as_str()).collect();
    // Remove menus generated by older versions.
    if let Ok(entries) = std::fs::read_dir(&servicemenus) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("donustur-convert") && !expected.contains(&name.as_str()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let mut written = Vec::with_capacity(menus.len());
    for (name, content) in menus {
        let path = servicemenus.join(&name);
        std::fs::write(&path, content)
            .with_context(|| format!("servis menüsü yazılamadı: {}", path.display()))?;
        // KIO refuses user-owned service menus that are not executable.
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("servis menüsü izni ayarlanamadı: {}", path.display()))?;
        written.push(path);
    }
    if !cfg!(test) {
        refresh_file_manager_cache();
    }

    Ok(Installed {
        desktop,
        service_menus: written,
        icon,
    })
}

/// Asks KDE to pick up new service menus without blocking the UI.
fn refresh_file_manager_cache() {
    use std::process::Stdio;
    for binary in ["kbuildsycoca6", "kbuildsycoca5"] {
        let spawned = std::process::Command::new(binary)
            .arg("--noincremental")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = spawned {
            // Reap the helper so it does not stay as a zombie for the app's
            // whole lifetime.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return;
        }
    }
}

/// The real XDG data home (respects `XDG_DATA_HOME`).
pub fn data_home() -> PathBuf {
    paths::data_home()
}

/// True when every expected service menu exists and is up to date.
pub fn is_installed_for(exec: &Path, data_home: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let dir = data_home.join("kio/servicemenus");
    service_menus(exec).iter().all(|(name, content)| {
        let path = dir.join(name);
        let current = std::fs::read_to_string(&path)
            .map(|actual| actual == *content)
            .unwrap_or(false);
        let executable = std::fs::metadata(&path)
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false);
        current && executable
    })
}

/// Installs/refreshes the context menu. Safe to call on every startup.
pub fn ensure_installed() -> Result<Installed> {
    ensure_installed_for(&executable_path())
}

/// Installs/refreshes the context menu for a specific executable.
pub fn ensure_installed_for(exec: &Path) -> Result<Installed> {
    let data = data_home();
    if is_installed_for(exec, &data) {
        let servicemenus = data.join("kio/servicemenus");
        return Ok(Installed {
            desktop: data.join("applications/donustur.desktop"),
            service_menus: service_menus(exec)
                .into_iter()
                .map(|(name, _)| servicemenus.join(name))
                .collect(),
            icon: data.join("icons/hicolor/512x512/apps/donustur.png"),
        });
    }
    install_to(&data, exec)
}

/// Checks every registry format has a MIME mapping; used by tests and debug builds.
#[cfg(test)]
fn assert_mime_coverage() {
    for format in crate::core::registry::FORMATS {
        assert!(
            mime_for(format.id).is_some(),
            "MIME eşlemesi eksik: {}",
            format.id
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_format_has_a_mime_type() {
        assert_mime_coverage();
        assert!(mime_list().ends_with(';'));
    }

    #[test]
    fn exec_quoting_handles_spaces_and_specials() {
        let quoted = exec_quote(Path::new("/tmp/a b/$x\"y\\.AppImage"));
        assert_eq!(quoted, "\"/tmp/a b/\\$x\\\"y\\\\.AppImage\"");
    }

    #[test]
    fn install_writes_target_menus_and_icon() {
        let dir = tempfile::tempdir().unwrap();
        let exec = dir.path().join("Uygulama Klasörü/Donustur.AppImage");
        let installed = install_to(dir.path(), &exec).unwrap();

        let desktop = std::fs::read_to_string(&installed.desktop).unwrap();
        assert!(!desktop.contains("--convert"));
        assert!(desktop.contains("MimeType="));
        assert!(desktop.contains("image/png"));
        assert!(desktop.contains(&exec_quote(&exec)));

        assert!(!installed.service_menus.is_empty());
        let jpeg = installed
            .service_menus
            .iter()
            .find(|path| path.to_string_lossy().contains("image-jpeg"))
            .expect("JPEG servis menüsü");
        let service = std::fs::read_to_string(jpeg).unwrap();
        assert!(service.contains("MimeType=image/jpeg;"));
        assert!(service.contains("--to png"));
        assert!(!service.contains("--to jpg"));
        assert!(service.contains("X-KDE-Submenu[tr]=ConvertAnything ile dönüştür"));
        assert!(service.contains(&exec_quote(&exec)));

        // Every generated menu points at the current executable and has actions.
        for path in &installed.service_menus {
            let text = std::fs::read_to_string(path).unwrap();
            assert!(text.contains("Actions="));
            assert!(text.contains(&exec_quote(&exec)));
            use std::os::unix::fs::PermissionsExt;
            assert!(std::fs::metadata(path).unwrap().permissions().mode() & 0o111 != 0);
        }

        assert!(installed.icon.is_file());
        assert!(is_installed_for(&exec, dir.path()));
    }

    #[test]
    fn every_convertible_source_gets_a_menu() {
        let menus = service_menus(Path::new("/usr/bin/donustur"));
        for format in registry::FORMATS {
            if format.kind == registry::MediaKind::Archive {
                continue;
            }
            let mime = mime_for(format.id).unwrap();
            let slug: String = mime
                .chars()
                .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
                .collect();
            assert!(
                menus.iter().any(|(name, _)| name.contains(&slug)),
                "menü yok: {} ({mime})",
                format.id
            );
        }
    }
}
