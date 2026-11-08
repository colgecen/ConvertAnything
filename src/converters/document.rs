//! Document conversion through LibreOffice headless and Pandoc.
//!
//! * plain text/CSV → UTF-8: handled in-process (no external tool);
//! * any document → PDF: LibreOffice headless;
//! * Markdown/EPUB involved: Pandoc;
//! * other office formats: LibreOffice headless.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::converters::{report, ConvertRequest, ProgressFn};
use crate::core::detector;
use crate::util::process;

/// Which engine handles a source/target pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Encoding,
    LibreOffice,
    Pandoc,
}

/// Chooses the engine. Kept pure so it can be tested without any tool installed.
pub fn strategy(source_id: &str, target_id: &str) -> Strategy {
    let text = |id: &str| matches!(id, "txt" | "csv");
    if text(source_id) && text(target_id) {
        return Strategy::Encoding;
    }
    if target_id == "pdf" {
        return Strategy::LibreOffice;
    }
    if matches!(source_id, "md" | "epub") || matches!(target_id, "md" | "epub") {
        return Strategy::Pandoc;
    }
    Strategy::LibreOffice
}

pub async fn convert(request: &ConvertRequest, progress: ProgressFn) -> Result<()> {
    match strategy(request.source.id, &request.target_id) {
        Strategy::Encoding => {
            encoding_to_utf8(request, progress).await?;
        }
        Strategy::LibreOffice => {
            libreoffice_convert(request, progress).await?;
        }
        Strategy::Pandoc => {
            pandoc_convert(request, progress).await?;
        }
    }
    Ok(())
}

/// Encoding detection needs the whole file in memory, so text conversion has
/// a stricter cap than the general file size limit.
const MAX_TEXT_BYTES: u64 = 256 * 1024 * 1024;

/// Converts TXT/CSV from a legacy encoding to UTF-8.
async fn encoding_to_utf8(request: &ConvertRequest, progress: ProgressFn) -> Result<()> {
    let input = request.input.clone();
    let output = request.output.clone();
    let max_size = request.options.max_file_size.min(MAX_TEXT_BYTES);

    tokio::task::spawn_blocking(move || {
        report(&progress, 10);
        let meta = std::fs::metadata(&input)
            .with_context(|| format!("metin dosyası okunamadı: {}", input.display()))?;
        if meta.len() > max_size {
            bail!(
                "metin dosyası {} MiB sınırını aşıyor; kodlama dönüşümü bellekte yapılır",
                max_size / (1024 * 1024)
            );
        }
        let bytes = std::fs::read(&input)
            .with_context(|| format!("metin dosyası okunamadı: {}", input.display()))?;
        if bytes.len() as u64 > max_size {
            bail!("dosya boyut sınırını aşıyor");
        }

        // Strip an existing BOM before redetection.
        let (bytes, already_utf8) = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            (&bytes[3..], true)
        } else if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
            (&bytes[..], false)
        } else {
            (&bytes[..], std::str::from_utf8(&bytes).is_ok())
        };

        report(&progress, 50);
        let decoded = if already_utf8 {
            String::from_utf8_lossy(bytes).into_owned()
        } else {
            let mut detector =
                chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
            detector.feed(bytes, true);
            let encoding = detector.guess(None, chardetng::Utf8Detection::Allow);
            let (text, _, had_errors) = encoding.decode(bytes);
            if had_errors {
                bail!("metin kodlaması güvenilir biçimde çözümlenemedi");
            }
            text.into_owned()
        };

        std::fs::write(&output, decoded.as_bytes())
            .with_context(|| format!("çıktı yazılamadı: {}", output.display()))?;
        report(&progress, 100);
        Ok(())
    })
    .await
    .context("kodlama dönüşümü görevi çöktü")??;

    Ok(())
}

async fn libreoffice_convert(request: &ConvertRequest, progress: ProgressFn) -> Result<()> {
    let soffice = detector::find_binary("soffice")
        .or_else(|| detector::find_binary("libreoffice"))
        .context("LibreOffice (soffice) kurulu değil")?;

    let input = request
        .input
        .canonicalize()
        .with_context(|| format!("girdi bulunamadı: {}", request.input.display()))?;
    let output = request.output.clone();
    let filter = librefilter(&request.target_id)?;

    // An isolated profile avoids clashing with a running LibreOffice instance.
    let profile = tempfile::Builder::new()
        .prefix("soffice-profil-")
        .tempdir()
        .context("geçici LibreOffice profili oluşturulamadı")?;
    let outdir = tempfile::Builder::new()
        .prefix("soffice-cikti-")
        .tempdir()
        .context("geçici çıktı dizini oluşturulamadı")?;

    let args = soffice_args(&input, outdir.path(), profile.path(), filter);

    report(&progress, 10);
    let timeout = request.options.timeout;
    let mut cmd = tokio::process::Command::new(soffice);
    cmd.args(&args);
    let workdir = process::sandbox_workdir();
    let result = process::run_captured(
        cmd,
        timeout,
        Some(profile.path()),
        Some(workdir.as_path()),
        None,
    )
    .await?;
    if result.timed_out {
        bail!("LibreOffice zaman sınırını aştı ve öldürüldü");
    }
    if !result.success {
        bail!("LibreOffice başarısız oldu: {}", result.error_summary());
    }
    report(&progress, 90);

    let produced = find_produced_file(outdir.path(), &request.target_id)
        .context("LibreOffice beklenen çıktıyı üretmedi")?;
    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("çıktı dizini oluşturulamadı")?;
    }
    tokio::fs::rename(&produced, &output)
        .await
        .or_else(|_| std::fs::copy(&produced, &output).map(|_| ()))
        .with_context(|| format!("çıktı taşınamadı: {}", output.display()))?;
    report(&progress, 100);
    Ok(())
}

/// Builds the LibreOffice headless command line. Pure, hence unit tested.
fn soffice_args(input: &Path, outdir: &Path, profile: &Path, filter: &str) -> Vec<String> {
    let profile_uri = path_to_file_uri(profile);
    vec![
        "--headless".to_string(),
        "--norestore".to_string(),
        "--invisible".to_string(),
        "--nologo".to_string(),
        "--nodefault".to_string(),
        "--nolockcheck".to_string(),
        format!("-env:UserInstallation={profile_uri}"),
        "--convert-to".to_string(),
        filter.to_string(),
        "--outdir".to_string(),
        outdir.to_string_lossy().to_string(),
        input.to_string_lossy().to_string(),
    ]
}

/// Builds the Pandoc command line. Pure, hence unit tested.
fn pandoc_args(input: &Path, output: &Path, target_id: &str) -> Vec<String> {
    let mut args = vec![
        input.to_string_lossy().to_string(),
        "-o".to_string(),
        output.to_string_lossy().to_string(),
    ];
    if target_id == "html" {
        args.push("--standalone".to_string());
    }
    args
}

fn librefilter(target_id: &str) -> Result<&'static str> {
    Ok(match target_id {
        "pdf" => "pdf:writer_pdf_Export",
        "docx" => "docx:MS Word 2007 XML",
        "odt" => "odt",
        "html" => "html",
        "txt" => "txt:Text (encoded):UTF8",
        other => bail!("LibreOffice için desteklenmeyen hedef: {other}"),
    })
}

fn find_produced_file(dir: &Path, target_id: &str) -> Option<PathBuf> {
    let expected_ext = target_id.to_ascii_lowercase();
    let mut fallback = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let extension = path
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if extension == expected_ext {
            return Some(path);
        }
        fallback.get_or_insert(path);
    }
    // LibreOffice sometimes writes a differently cased extension.
    fallback
}

async fn pandoc_convert(request: &ConvertRequest, progress: ProgressFn) -> Result<()> {
    let pandoc = detector::find_binary("pandoc").context("Pandoc kurulu değil")?;
    let input = request
        .input
        .canonicalize()
        .with_context(|| format!("girdi bulunamadı: {}", request.input.display()))?;
    let output = request.output.clone();
    if let Some(parent) = output.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .context("çıktı dizini oluşturulamadı")?;
    }

    let args = pandoc_args(&input, &output, &request.target_id);

    report(&progress, 10);
    let mut cmd = tokio::process::Command::new(pandoc);
    cmd.args(&args);
    let workdir = process::sandbox_workdir();
    let result = process::run_captured(
        cmd,
        request.options.timeout,
        None,
        Some(workdir.as_path()),
        None,
    )
    .await?;
    if result.timed_out {
        bail!("Pandoc zaman sınırını aştı ve öldürüldü");
    }
    if !result.success {
        bail!("Pandoc başarısız oldu: {}", result.error_summary());
    }
    let meta = tokio::fs::metadata(&output)
        .await
        .context("Pandoc çıktısı oluşmadı")?;
    if meta.len() == 0 {
        bail!("Pandoc boş bir çıktı üretti");
    }
    report(&progress, 100);
    Ok(())
}

/// Converts a filesystem path into a `file://` URI.
fn path_to_file_uri(path: &Path) -> String {
    let mut encoded = String::from("file://");
    for byte in path.to_string_lossy().bytes() {
        let unreserved =
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~');
        if unreserved {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::converters::archive::ArchiveLimits;
    use crate::converters::ConvertOptions;
    use std::sync::Arc;

    fn request(
        input: PathBuf,
        output: PathBuf,
        source: &'static crate::core::registry::Format,
        target: &str,
    ) -> ConvertRequest {
        ConvertRequest {
            input,
            output,
            source,
            target_id: target.to_string(),
            options: ConvertOptions {
                image_quality: 90,
                bitrate_kbps: None,
                video_scale: None,
                timeout: std::time::Duration::from_secs(30),
                max_file_size: 16 * 1024 * 1024,
                archive_limits: ArchiveLimits::default(),
            },
        }
    }

    fn noop() -> ProgressFn {
        Arc::new(|_| {})
    }

    #[test]
    fn strategy_selection() {
        assert_eq!(strategy("txt", "csv"), Strategy::Encoding);
        assert_eq!(strategy("csv", "csv"), Strategy::Encoding);
        assert_eq!(strategy("docx", "pdf"), Strategy::LibreOffice);
        assert_eq!(strategy("md", "docx"), Strategy::Pandoc);
        assert_eq!(strategy("docx", "epub"), Strategy::Pandoc);
        assert_eq!(strategy("docx", "odt"), Strategy::LibreOffice);
    }

    #[test]
    fn file_uri_encodes_spaces() {
        assert_eq!(path_to_file_uri(Path::new("/tmp/a b")), "file:///tmp/a%20b");
    }

    #[test]
    fn filter_mapping_is_fixed() {
        assert_eq!(librefilter("pdf").unwrap(), "pdf:writer_pdf_Export");
        assert!(librefilter("exe").is_err());
    }

    #[test]
    fn soffice_args_use_absolute_paths_and_fixed_filter() {
        let args = soffice_args(
            Path::new("/tmp/girdi.docx"),
            Path::new("/tmp/cikti"),
            Path::new("/tmp/profil"),
            "pdf:writer_pdf_Export",
        );
        assert!(args.iter().any(|a| a == "--headless"));
        assert!(args
            .iter()
            .any(|a| a.starts_with("-env:UserInstallation=file://")));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--convert-to" && w[1] == "pdf:writer_pdf_Export"));
        assert!(args
            .windows(2)
            .any(|w| { w[0] == "--outdir" && w[1] == "/tmp/cikti" }));
        assert_eq!(args.last().unwrap(), "/tmp/girdi.docx");
        assert!(!args.iter().any(|a| a.contains(';')));
    }

    #[test]
    fn pandoc_args_add_standalone_only_for_html() {
        let html = pandoc_args(Path::new("/tmp/a.md"), Path::new("/tmp/a.html"), "html");
        assert!(html.contains(&"--standalone".to_string()));
        let docx = pandoc_args(Path::new("/tmp/a.md"), Path::new("/tmp/a.docx"), "docx");
        assert!(!docx.contains(&"--standalone".to_string()));
        assert_eq!(docx[1], "-o");
        assert_eq!(docx[2], "/tmp/a.docx");
    }

    #[tokio::test]
    async fn converts_windows_1254_to_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("eski.txt");
        let output = dir.path().join("yeni.txt");
        let (encoded, _, _) = encoding_rs::WINDOWS_1254.encode("Türkçe ışık: ğüşiöç");
        std::fs::write(&input, encoded.as_ref()).unwrap();

        let source = crate::core::registry::format_by_id("txt").unwrap();
        let req = request(input, output.clone(), source, "txt");
        convert(&req, noop()).await.unwrap();

        let text = std::fs::read_to_string(&output).unwrap();
        assert_eq!(text, "Türkçe ışık: ğüşiöç");
    }

    /// End-to-end LibreOffice check; slow, so opt-in with `--ignored`.
    #[tokio::test]
    #[ignore = "LibreOffice gerektirir ve yavaştır"]
    async fn real_libreoffice_converts_docx_to_pdf() {
        let Some(soffice) = crate::core::detector::find_binary("soffice") else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("ornek.docx");
        let output = dir.path().join("ornek.pdf");

        // Use LibreOffice itself to produce the fixture, so the test only needs
        // the same dependency as the conversion under test.
        std::fs::write(dir.path().join("ornek.txt"), "Başlık\n\nMerhaba dünya.\n").unwrap();
        let generated = tokio::process::Command::new(&soffice)
            .args(["--headless", "--convert-to", "docx", "--outdir"])
            .arg(dir.path())
            .arg(dir.path().join("ornek.txt"))
            .output()
            .await
            .expect("soffice başlatılamadı");
        assert!(
            generated.status.success() && input.exists(),
            "docx fixture üretilemedi: {}",
            String::from_utf8_lossy(&generated.stderr)
        );

        let source = crate::core::registry::format_by_id("docx").unwrap();
        let request = request(input, output.clone(), source, "pdf");
        convert(&request, noop()).await.unwrap();
        assert!(std::fs::metadata(&output).unwrap().len() > 1024);
    }

    #[tokio::test]
    async fn utf8_input_is_copied_without_bom() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("a.csv");
        let output = dir.path().join("b.csv");
        std::fs::write(&input, b"\xEF\xBB\xBFa,b\n1,2\n").unwrap();
        let source = crate::core::registry::format_by_id("csv").unwrap();
        let req = request(input, output.clone(), source, "csv");
        convert(&req, noop()).await.unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), b"a,b\n1,2\n");
    }
}
