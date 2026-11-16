//! Format registry and the conversion support matrix.
//!
//! This module is the single source of truth: the UI and the converters both
//! derive their behaviour from it. Adding a format means adding it here first.

use std::path::Path;

use anyhow::{bail, Result};

use crate::i18n::t;

/// Broad category of a file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Image,
    Audio,
    Video,
    Document,
    Archive,
}

impl MediaKind {
    pub fn label(self) -> &'static str {
        match self {
            MediaKind::Image => t("Image", "Görsel"),
            MediaKind::Audio => t("Audio", "Ses"),
            MediaKind::Video => t("Video", "Video"),
            MediaKind::Document => t("Document", "Belge"),
            MediaKind::Archive => t("Archive", "Arşiv"),
        }
    }
}

/// A supported format. `id` is also the canonical file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: MediaKind,
    /// Extra extensions that map to this format.
    pub aliases: &'static [&'static str],
}

impl Format {
    /// Localised label used by the UI.
    pub fn display_label(&self) -> &'static str {
        match self.id {
            "txt" => t("Text", "Metin"),
            "docx" => t("Word document", "Word belgesi"),
            "odt" => t("OpenDocument", "OpenDocument"),
            _ => self.label,
        }
    }
}

pub const FORMATS: &[Format] = &[
    // Images
    Format {
        id: "jpg",
        label: "JPEG",
        kind: MediaKind::Image,
        aliases: &["jpeg"],
    },
    Format {
        id: "png",
        label: "PNG",
        kind: MediaKind::Image,
        aliases: &[],
    },
    Format {
        id: "webp",
        label: "WebP",
        kind: MediaKind::Image,
        aliases: &[],
    },
    Format {
        id: "bmp",
        label: "BMP",
        kind: MediaKind::Image,
        aliases: &[],
    },
    Format {
        id: "tiff",
        label: "TIFF",
        kind: MediaKind::Image,
        aliases: &["tif"],
    },
    Format {
        id: "gif",
        label: "GIF",
        kind: MediaKind::Image,
        aliases: &[],
    },
    Format {
        id: "ico",
        label: "ICO",
        kind: MediaKind::Image,
        aliases: &[],
    },
    // Audio
    Format {
        id: "mp3",
        label: "MP3",
        kind: MediaKind::Audio,
        aliases: &[],
    },
    Format {
        id: "wav",
        label: "WAV",
        kind: MediaKind::Audio,
        aliases: &[],
    },
    Format {
        id: "flac",
        label: "FLAC",
        kind: MediaKind::Audio,
        aliases: &[],
    },
    Format {
        id: "ogg",
        label: "OGG",
        kind: MediaKind::Audio,
        aliases: &[],
    },
    Format {
        id: "opus",
        label: "Opus",
        kind: MediaKind::Audio,
        aliases: &[],
    },
    Format {
        id: "aac",
        label: "AAC",
        kind: MediaKind::Audio,
        aliases: &[],
    },
    Format {
        id: "m4a",
        label: "M4A",
        kind: MediaKind::Audio,
        aliases: &[],
    },
    // Video
    Format {
        id: "mp4",
        label: "MP4",
        kind: MediaKind::Video,
        aliases: &[],
    },
    Format {
        id: "mkv",
        label: "Matroska",
        kind: MediaKind::Video,
        aliases: &[],
    },
    Format {
        id: "webm",
        label: "WebM",
        kind: MediaKind::Video,
        aliases: &[],
    },
    Format {
        id: "avi",
        label: "AVI",
        kind: MediaKind::Video,
        aliases: &[],
    },
    Format {
        id: "mov",
        label: "MOV",
        kind: MediaKind::Video,
        aliases: &[],
    },
    // Documents
    Format {
        id: "pdf",
        label: "PDF",
        kind: MediaKind::Document,
        aliases: &[],
    },
    Format {
        id: "docx",
        label: "Word",
        kind: MediaKind::Document,
        aliases: &[],
    },
    Format {
        id: "odt",
        label: "OpenDocument",
        kind: MediaKind::Document,
        aliases: &[],
    },
    Format {
        id: "md",
        label: "Markdown",
        kind: MediaKind::Document,
        aliases: &["markdown"],
    },
    Format {
        id: "html",
        label: "HTML",
        kind: MediaKind::Document,
        aliases: &["htm"],
    },
    Format {
        id: "epub",
        label: "EPUB",
        kind: MediaKind::Document,
        aliases: &[],
    },
    Format {
        id: "txt",
        label: "Metin",
        kind: MediaKind::Document,
        aliases: &[],
    },
    Format {
        id: "csv",
        label: "CSV",
        kind: MediaKind::Document,
        aliases: &[],
    },
    // Archives
    Format {
        id: "zip",
        label: "ZIP",
        kind: MediaKind::Archive,
        aliases: &[],
    },
    Format {
        id: "tar.gz",
        label: "TAR.GZ",
        kind: MediaKind::Archive,
        aliases: &["tgz"],
    },
    Format {
        id: "tar",
        label: "TAR",
        kind: MediaKind::Archive,
        aliases: &[],
    },
];

/// Pseudo target: extract an archive into a directory.
pub const EXTRACT: &str = "extract";

/// Executable/script extensions that may never be converted.
pub const FORBIDDEN_EXTENSIONS: &[&str] = &[
    "exe", "msi", "dll", "com", "bat", "cmd", "scr", "elf", "so", "o", "a", "sh", "bash", "zsh",
    "fish", "py", "pl", "rb", "js", "jar", "class", "appimage", "bin", "run", "desktop", "service",
    "pkg", "deb", "rpm", "wasm",
];

/// Looks up a format by canonical id or alias.
pub fn format_by_id(id: &str) -> Option<&'static Format> {
    let id = id.trim().trim_start_matches('.').to_ascii_lowercase();
    FORMATS
        .iter()
        .find(|f| f.id == id || f.aliases.iter().any(|alias| *alias == id))
}

/// Detects a format from a file name, including compound extensions.
pub fn detect_format(path: &Path) -> Option<&'static Format> {
    let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
    let mut best: Option<(&Format, usize)> = None;
    for format in FORMATS {
        let suffixes = std::iter::once(format.id).chain(format.aliases.iter().copied());
        for suffix in suffixes {
            let needle = format!(".{suffix}");
            if name.ends_with(&needle) {
                let len = needle.len();
                if best.map(|(_, l)| len > l).unwrap_or(true) {
                    best = Some((format, len));
                }
            }
        }
    }
    best.map(|(f, _)| f)
}

/// Returns true for extensions that are always rejected.
pub fn has_forbidden_extension(path: &Path) -> bool {
    let Some(name) = path.file_name() else {
        return true;
    };
    let name = name.to_string_lossy().to_ascii_lowercase();
    FORBIDDEN_EXTENSIONS
        .iter()
        .any(|ext| name.ends_with(&format!(".{ext}")))
}

/// Checks leading magic bytes for executable formats.
///
/// The read is bounded to 512 bytes and never interprets file contents.
pub fn looks_executable(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 512];
    let Ok(read) = file.read(&mut head) else {
        return false;
    };
    let head = &head[..read];
    head.starts_with(b"\x7fELF")
        || head.starts_with(b"MZ")
        || head.starts_with(b"#!")
        || head.starts_with(&[0xCA, 0xFE, 0xBA, 0xBE])
        || head.starts_with(&[0xFE, 0xED, 0xFA, 0xCE])
        || head.starts_with(&[0xFE, 0xED, 0xFA, 0xCF])
        || head.starts_with(&[0xCE, 0xFA, 0xED, 0xFE])
        || head.starts_with(&[0xCF, 0xFA, 0xED, 0xFE])
        || head.starts_with(&[0xCA, 0xFE, 0xBA, 0xBF])
}

/// Container/extension pairs that legitimately share magic bytes.
///
/// Office documents and EPUB are ZIP containers, `tar.gz` is GZIP, and
/// `m4a`/`aac` live inside MP4 containers.
fn magic_families(kind: MediaKind) -> &'static [&'static str] {
    match kind {
        MediaKind::Image => &["jpg", "png", "webp", "bmp", "tiff", "gif", "ico"],
        MediaKind::Audio => &[
            "mp3", "wav", "flac", "ogg", "opus", "aac", "m4a", "mp4", "mov", "m4b",
        ],
        MediaKind::Video => &["mp4", "m4v", "mov", "mkv", "webm", "avi", "gif"],
        MediaKind::Document => &[
            "pdf", "zip", "gz", "tar", "rtf", "html", "xml", "epub", "docx", "xlsx", "pptx", "odt",
            "ods", "odp", "doc", "xls",
        ],
        MediaKind::Archive => &["zip", "gz", "tar", "bz2", "xz", "zst", "7z"],
    }
}

/// Checks the file's magic bytes against the declared format.
///
/// A mismatch between *families* (for example a ZIP renamed to `.mp4`) is
/// rejected. Within a family the exact subtype is not enforced because
/// containers are intentionally interchangeable (`docx`/`odt`/`epub` are all
/// ZIP, `m4a` is MP4). Files whose type cannot be sniffed (plain text) are
/// accepted and validated later by the converter.
pub fn verify_magic(path: &Path, declared: &Format) -> Result<()> {
    if looks_executable(path) {
        bail!("dosya içeriği yürütülebilir bir biçim olarak algılandı");
    }
    let Some(sniffed) = infer::get_from_path(path).ok().flatten() else {
        return Ok(());
    };
    let ext = sniffed.extension().to_ascii_lowercase();
    if ext == declared.id || declared.aliases.contains(&ext.as_str()) {
        return Ok(());
    }
    if magic_families(declared.kind).contains(&ext.as_str()) {
        return Ok(());
    }
    bail!(
        "dosya içeriği uzantıyla uyuşmuyor (içerik: {}, beklenen: {})",
        sniffed.mime_type(),
        declared.id
    )
}

/// Validates that a path is a convertible, non-executable file.
pub fn validate_source(path: &Path) -> Result<&'static Format> {
    let Some(format) = detect_format(path) else {
        bail!(
            "desteklenmeyen dosya türü: {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
    };
    if has_forbidden_extension(path) {
        bail!("yürütülebilir dosyalar dönüştürülemez");
    }
    if looks_executable(path) {
        bail!("dosya içeriği yürütülebilir bir biçim olarak algılandı");
    }
    Ok(format)
}

/// Conversion targets offered for a source format.
///
/// The list is intentionally conservative: a target is only offered when the
/// selected engine can produce a valid result on a clean installation.
pub fn targets_for(source: &Format) -> Vec<&'static Format> {
    let mut result: Vec<&'static Format> = Vec::new();

    match source.kind {
        MediaKind::Image => {
            for format in FORMATS.iter().filter(|f| f.kind == MediaKind::Image) {
                if format.id != source.id {
                    result.push(format);
                }
            }
        }
        MediaKind::Audio => {
            for format in FORMATS.iter().filter(|f| f.kind == MediaKind::Audio) {
                if format.id != source.id {
                    result.push(format);
                }
            }
        }
        MediaKind::Video => {
            for format in FORMATS.iter().filter(|f| f.kind == MediaKind::Video) {
                if format.id != source.id {
                    result.push(format);
                }
            }
            if let Some(gif) = format_by_id("gif") {
                result.push(gif);
            }
        }
        MediaKind::Document => {
            for id in document_targets(source.id) {
                if let Some(format) = format_by_id(id) {
                    if format.id != source.id || *id == "txt" || *id == "csv" {
                        result.push(format);
                    }
                }
            }
        }
        MediaKind::Archive => {}
    }

    // Any non-archive file may be packed into an archive.
    if source.kind != MediaKind::Archive {
        for id in ["zip", "tar.gz"] {
            if let Some(format) = format_by_id(id) {
                result.push(format);
            }
        }
    } else if let Some(extract) = format_by_id_extract() {
        result.push(extract);
    }

    result
}

/// `EXTRACT` is not a real format; this synthetic value keeps the UI simple.
pub fn format_by_id_extract() -> Option<&'static Format> {
    static EXTRACT_FORMAT: Format = Format {
        id: EXTRACT,
        label: "Arşivi ayıkla",
        kind: MediaKind::Archive,
        aliases: &[],
    };
    Some(&EXTRACT_FORMAT)
}

fn document_targets(source_id: &str) -> &'static [&'static str] {
    match source_id {
        // PDF is a target only: reliable PDF parsing is outside the scope.
        "pdf" => &[],
        "md" => &["html", "docx", "epub", "odt", "txt"],
        "html" => &["md", "docx", "epub", "txt", "pdf"],
        "docx" => &["md", "html", "txt", "pdf", "odt", "epub"],
        "odt" => &["docx", "html", "txt", "pdf"],
        "epub" => &["md", "html", "docx", "txt"],
        "txt" => &["md", "html", "docx", "epub", "pdf", "txt"],
        "csv" => &["txt", "csv", "pdf"],
        _ => &[],
    }
}

/// Checks the support matrix for a concrete source/target pair.
pub fn is_supported(source: &Format, target_id: &str) -> bool {
    if source.kind == MediaKind::Archive {
        return target_id == EXTRACT;
    }
    targets_for(source).iter().any(|f| f.id == target_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_compound_extension() {
        assert_eq!(detect_format(Path::new("a.tar.gz")).unwrap().id, "tar.gz");
        assert_eq!(detect_format(Path::new("a.tgz")).unwrap().id, "tar.gz");
        assert_eq!(detect_format(Path::new("A.JPEG")).unwrap().id, "jpg");
        assert_eq!(detect_format(Path::new("a.txt")).unwrap().id, "txt");
        assert!(detect_format(Path::new("a.unknown")).is_none());
        assert!(detect_format(Path::new("no-extension")).is_none());
    }

    #[test]
    fn aliases_are_resolved() {
        assert_eq!(format_by_id("jpeg").unwrap().id, "jpg");
        assert_eq!(format_by_id(".PNG").unwrap().id, "png");
        assert_eq!(format_by_id("htm").unwrap().id, "html");
        assert!(format_by_id("exe").is_none());
    }

    #[test]
    fn images_exclude_self_and_offer_archives() {
        let jpg = format_by_id("jpg").unwrap();
        let targets = targets_for(jpg);
        assert!(targets.iter().any(|f| f.id == "png"));
        assert!(!targets.iter().any(|f| f.id == "jpg"));
        assert!(targets.iter().any(|f| f.id == "zip"));
        assert!(targets.iter().any(|f| f.id == "tar.gz"));
    }

    #[test]
    fn video_can_become_gif() {
        let mp4 = format_by_id("mp4").unwrap();
        assert!(targets_for(mp4).iter().any(|f| f.id == "gif"));
    }

    #[test]
    fn audio_targets_do_not_include_video() {
        let mp3 = format_by_id("mp3").unwrap();
        let targets = targets_for(mp3);
        assert!(targets.iter().any(|f| f.id == "flac"));
        assert!(!targets.iter().any(|f| f.id == "mp4"));
    }

    #[test]
    fn document_matrix_is_conservative() {
        let pdf = format_by_id("pdf").unwrap();
        assert!(targets_for(pdf).iter().all(|f| f.id != "docx"));
        let md = format_by_id("md").unwrap();
        assert!(is_supported(md, "docx"));
        assert!(is_supported(md, "epub"));
        assert!(!is_supported(md, "pdf"));
    }

    #[test]
    fn txt_and_csv_allow_same_target_for_encoding() {
        let txt = format_by_id("txt").unwrap();
        assert!(is_supported(txt, "txt"));
        let csv = format_by_id("csv").unwrap();
        assert!(is_supported(csv, "csv"));
        assert!(is_supported(csv, "txt"));
    }

    #[test]
    fn archives_only_extract() {
        let zip = format_by_id("zip").unwrap();
        let targets = targets_for(zip);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].id, EXTRACT);
        assert!(is_supported(zip, EXTRACT));
    }

    #[test]
    fn forbidden_extensions_include_scripts_and_binaries() {
        for name in ["a.exe", "a.sh", "a.elf", "a.so", "a.AppImage", "a.rpm"] {
            assert!(has_forbidden_extension(&PathBuf::from(name)), "{name}");
        }
        assert!(!has_forbidden_extension(Path::new("a.png")));
    }

    #[test]
    fn magic_detection_finds_elf_and_scripts() {
        let dir = tempfile::tempdir().unwrap();
        let elf = dir.path().join("fake.png");
        std::fs::write(&elf, b"\x7fELF....").unwrap();
        assert!(looks_executable(&elf));
        let script = dir.path().join("fake.txt");
        std::fs::write(&script, b"#!/bin/sh\necho hi\n").unwrap();
        assert!(looks_executable(&script));
        let png = dir.path().join("real.png");
        std::fs::write(&png, [0x89, b'P', b'N', b'G', 0x0D]).unwrap();
        assert!(!looks_executable(&png));
    }

    #[test]
    fn magic_check_accepts_matching_and_family_content() {
        let dir = tempfile::tempdir().unwrap();
        // Real PNG header is enough for `infer`.
        let png = dir.path().join("a.jpg");
        std::fs::write(
            &png,
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0",
        )
        .unwrap();
        // A JPG extension holding a PNG is still an image: allowed.
        assert!(verify_magic(&png, format_by_id("jpg").unwrap()).is_ok());

        // Office documents are ZIP containers.
        let docx = dir.path().join("a.docx");
        std::fs::write(&docx, b"PK\x03\x04\x14\x00\x00\x00\x08\x00").unwrap();
        assert!(verify_magic(&docx, format_by_id("docx").unwrap()).is_ok());
    }

    #[test]
    fn magic_check_rejects_cross_family_content() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("sahte.mp4");
        std::fs::write(&zip, b"PK\x03\x04\x14\x00\x00\x00\x08\x00").unwrap();
        let result = verify_magic(&zip, format_by_id("mp4").unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn magic_check_accepts_archives_and_targz() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("a.zip");
        std::fs::write(&zip, b"PK\x03\x04\x14\x00\x00\x00\x08\x00").unwrap();
        assert!(verify_magic(&zip, format_by_id("zip").unwrap()).is_ok());

        let tgz = dir.path().join("a.tar.gz");
        // GZIP header: 1f 8b 08 ...
        std::fs::write(&tgz, [0x1f, 0x8b, 0x08, 0x00, 0, 0, 0, 0, 0, 0xff]).unwrap();
        assert!(verify_magic(&tgz, format_by_id("tar.gz").unwrap()).is_ok());
    }

    #[test]
    fn magic_check_allows_unsniffable_text() {
        let dir = tempfile::tempdir().unwrap();
        let txt = dir.path().join("notlar.txt");
        std::fs::write(&txt, b"duz metin, imzasi yok").unwrap();
        assert!(verify_magic(&txt, format_by_id("txt").unwrap()).is_ok());
    }

    #[test]
    fn validate_source_rejects_executables() {
        let dir = tempfile::tempdir().unwrap();
        let elf = dir.path().join("evil.png");
        std::fs::write(&elf, b"\x7fELF....").unwrap();
        assert!(validate_source(&elf).is_err());

        let script = dir.path().join("evil.sh");
        std::fs::write(&script, b"#!/bin/sh\n").unwrap();
        assert!(validate_source(&script).is_err());

        let zip = dir.path().join("normal.zip");
        std::fs::write(&zip, b"PK\x03\x04").unwrap();
        assert_eq!(validate_source(&zip).unwrap().id, "zip");
    }
}
