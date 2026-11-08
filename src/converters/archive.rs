//! ZIP and TAR(.GZ) packing/extraction.
//!
//! Extraction is hostile-input aware: path traversal, symlinks, zip bombs
//! (ratio and total size) and excessive entry counts are rejected. The output
//! directory is removed again on any failure.

use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::core::registry::Format;
use crate::util::paths;

use super::{report, ProgressFn};

/// Defensive limits for archive extraction.
#[derive(Debug, Clone)]
pub struct ArchiveLimits {
    /// Maximum uncompressed/compressed ratio (100 = 100x).
    pub max_ratio: u64,
    /// Maximum total uncompressed size.
    pub max_total_bytes: u64,
    /// Maximum size of a single entry.
    pub max_entry_bytes: u64,
    /// Maximum number of entries.
    pub max_entries: u64,
    /// Maximum nesting depth. Nested archives are never recursed into, so the
    /// effective depth is always 1; the field is kept for forward compatibility.
    #[allow(dead_code)]
    pub max_depth: u32,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_ratio: 100,
            max_total_bytes: 4 * 1024 * 1024 * 1024,
            max_entry_bytes: 2 * 1024 * 1024 * 1024,
            max_entries: 100_000,
            max_depth: 3,
        }
    }
}

/// Packs a single file into `output` (zip/tar/tar.gz).
pub async fn create(
    input: &Path,
    output: &Path,
    target: &Format,
    options: &super::ConvertOptions,
    progress: ProgressFn,
) -> Result<()> {
    let input = input.to_path_buf();
    let output = output.to_path_buf();
    let target_id = target.id.to_string();
    let max_size = options.max_file_size;

    tokio::task::spawn_blocking(move || {
        let meta = std::fs::metadata(&input)
            .with_context(|| format!("girdi okunamadı: {}", input.display()))?;
        if !meta.is_file() {
            bail!("yalnızca tek dosya arşivlenebilir");
        }
        if meta.len() > max_size {
            bail!("dosya boyut sınırını aşıyor");
        }
        let name = input
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(paths::safe_file_name)
            .context("girdi dosya adı geçersiz")?;

        report(&progress, 10);
        match target_id.as_str() {
            "zip" => create_zip(&input, &output, &name, &progress)?,
            "tar.gz" => create_tar(&input, &output, &name, true, &progress)?,
            "tar" => create_tar(&input, &output, &name, false, &progress)?,
            other => bail!("desteklenmeyen arşiv hedefi: {other}"),
        }
        report(&progress, 100);
        Ok(())
    })
    .await
    .context("arşiv oluşturma görevi çöktü")??;

    Ok(())
}

fn create_zip(input: &Path, output: &Path, name: &str, progress: &ProgressFn) -> Result<()> {
    let file = File::create(output)
        .with_context(|| format!("arşiv oluşturulamadı: {}", output.display()))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644);
    writer
        .start_file(name, options)
        .context("ZIP girdisi başlatılamadı")?;
    let mut source = File::open(input).context("girdi açılamadı")?;
    io::copy(&mut source, &mut writer).context("ZIP'e yazılamadı")?;
    report(progress, 85);
    writer.finish().context("ZIP tamamlanamadı")?;
    Ok(())
}

fn create_tar(
    input: &Path,
    output: &Path,
    name: &str,
    gzip: bool,
    progress: &ProgressFn,
) -> Result<()> {
    let file = File::create(output)
        .with_context(|| format!("arşiv oluşturulamadı: {}", output.display()))?;
    if gzip {
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        builder
            .append_path_with_name(input, name)
            .context("TAR.GZ'e yazılamadı")?;
        report(progress, 85);
        let encoder = builder.into_inner().context("TAR kapatılamadı")?;
        encoder.finish().context("GZ akışı tamamlanamadı")?;
    } else {
        let mut builder = tar::Builder::new(file);
        builder
            .append_path_with_name(input, name)
            .context("TAR'a yazılamadı")?;
        report(progress, 85);
        builder.finish().context("TAR tamamlanamadı")?;
    }
    Ok(())
}

/// Extracts an archive into `output` (which must not exist yet).
pub async fn extract(
    input: &Path,
    output: &Path,
    source: &Format,
    options: &super::ConvertOptions,
    progress: ProgressFn,
) -> Result<()> {
    let input = input.to_path_buf();
    let output = output.to_path_buf();
    let source_id = source.id.to_string();
    let limits = options.archive_limits.clone();

    tokio::task::spawn_blocking(move || {
        if output.exists() {
            bail!("çıktı dizini zaten var: {}", output.display());
        }
        std::fs::create_dir(&output)
            .with_context(|| format!("çıktı dizini oluşturulamadı: {}", output.display()))?;
        let mut guard = Cleanup::new(output.clone());

        report(&progress, 10);
        let archive_size = std::fs::metadata(&input)
            .with_context(|| format!("arşiv okunamadı: {}", input.display()))?
            .len();
        if archive_size == 0 {
            bail!("arşiv boş");
        }

        match source_id.as_str() {
            "zip" => extract_zip(&input, &output, archive_size, &limits, &progress)?,
            "tar.gz" => extract_tar(&input, &output, archive_size, &limits, true, &progress)?,
            "tar" => extract_tar(&input, &output, archive_size, &limits, false, &progress)?,
            other => bail!("desteklenmeyen arşiv kaynağı: {other}"),
        }

        guard.disarm();
        report(&progress, 100);
        Ok(())
    })
    .await
    .context("arşiv ayıklama görevi çöktü")??;

    Ok(())
}

fn extract_zip(
    input: &Path,
    output: &Path,
    archive_size: u64,
    limits: &ArchiveLimits,
    progress: &ProgressFn,
) -> Result<()> {
    let file = File::open(input).context("ZIP açılamadı")?;
    let mut archive = zip::ZipArchive::new(file).context("ZIP çözümlenemedi")?;
    if archive.len() as u64 > limits.max_entries {
        bail!("arşivde çok fazla girdi var");
    }

    let mut total: u64 = 0;
    let count = archive.len();
    for index in 0..count {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("ZIP girdisi {index} okunamadı"))?;
        let relative = entry
            .enclosed_name()
            .and_then(|p| relative_safe_path(&p))
            .context("arşivde güvenli olmayan yol reddedildi")?;

        if entry.is_dir() {
            let dir = output.join(relative);
            std::fs::create_dir_all(&dir).context("arşiv dizini oluşturulamadı")?;
            continue;
        }
        if !entry.is_file() {
            bail!("arşivde yalnızca normal dosyalar ayıklanabilir");
        }
        if entry.size() > limits.max_entry_bytes {
            bail!("arşiv girdisi boyut sınırını aşıyor");
        }

        let destination = output.join(&relative);
        ensure_within(output, &destination)?;
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).context("üst dizin oluşturulamadı")?;
        }
        let mut out = File::create(&destination).context("arşiv girdisi yazılamadı")?;
        let remaining = limits.max_total_bytes.saturating_sub(total);
        let mut limited = entry.by_ref().take(remaining.saturating_add(1));
        let written = io::copy(&mut limited, &mut out).context("arşiv girdisi kopyalanamadı")?;
        if written > remaining {
            bail!("arşiv toplam boyut sınırını aşıyor");
        }
        total += written;
        check_ratio(total, archive_size, limits)?;

        if count > 0 {
            let pct = 10 + ((index as u64 + 1) * 85 / count as u64) as u8;
            report(progress, pct);
        }
    }
    check_ratio(total, archive_size, limits)?;
    Ok(())
}

fn extract_tar(
    input: &Path,
    output: &Path,
    archive_size: u64,
    limits: &ArchiveLimits,
    gzip: bool,
    progress: &ProgressFn,
) -> Result<()> {
    let file = File::open(input).context("TAR açılamadı")?;
    let reader: Box<dyn Read> = if gzip {
        Box::new(flate2::read::GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut archive = tar::Archive::new(reader);
    let entries = archive.entries().context("TAR çözümlenemedi")?;

    let mut total: u64 = 0;
    let mut processed: u64 = 0;
    for (index, entry) in entries.enumerate() {
        if index as u64 >= limits.max_entries {
            bail!("arşivde çok fazla girdi var");
        }
        let mut entry = entry.context("TAR girdisi okunamadı")?;
        let kind = entry.header().entry_type();
        if kind.is_symlink() || kind.is_hard_link() {
            bail!("arşivde sembolik/katı bağlantı reddedildi");
        }
        let path = entry.path().context("TAR girdi yolu okunamadı")?;
        let relative =
            relative_safe_path(&path).context("arşivde güvenli olmayan yol reddedildi")?;

        if kind.is_dir() {
            std::fs::create_dir_all(output.join(relative))
                .context("arşiv dizini oluşturulamadı")?;
            continue;
        }
        if !kind.is_file() {
            // Devices, FIFOs and other special entries are never extracted.
            continue;
        }
        let size = entry.size();
        if size > limits.max_entry_bytes {
            bail!("arşiv girdisi boyut sınırını aşıyor");
        }

        let destination = output.join(&relative);
        ensure_within(output, &destination)?;
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).context("üst dizin oluşturulamadı")?;
        }
        let mut out = File::create(&destination).context("arşiv girdisi yazılamadı")?;
        let remaining = limits.max_total_bytes.saturating_sub(total);
        let mut limited = (&mut entry).take(remaining.saturating_add(1));
        let written = io::copy(&mut limited, &mut out).context("arşiv girdisi kopyalanamadı")?;
        if written > remaining {
            bail!("arşiv toplam boyut sınırını aşıyor");
        }
        total += written;
        processed += 1;
        check_ratio(total, archive_size, limits)?;
        let pct = (10 + processed.min(85)) as u8;
        report(progress, pct);
    }
    check_ratio(total, archive_size, limits)?;
    Ok(())
}

fn check_ratio(total: u64, archive_size: u64, limits: &ArchiveLimits) -> Result<()> {
    let allowed = archive_size.saturating_mul(limits.max_ratio.max(1));
    if total > allowed {
        bail!(
            "arşiv açılma oranı sınırı aştı (olası zip bomb): {} / {}",
            total,
            archive_size
        );
    }
    Ok(())
}

/// Rejects absolute paths, `..` and Windows prefixes.
fn relative_safe_path(raw: &Path) -> Option<PathBuf> {
    if raw.as_os_str().is_empty() || raw.is_absolute() {
        return None;
    }
    let mut out = PathBuf::new();
    for component in raw.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Lexical containment check; archive paths are already relative and clean.
fn ensure_within(base: &Path, destination: &Path) -> Result<()> {
    if !destination.starts_with(base) {
        bail!("arşiv yolu hedef dizinin dışına çıkıyor");
    }
    Ok(())
}

/// Removes a partially extracted directory unless disarmed.
struct Cleanup {
    path: PathBuf,
    armed: bool,
}

impl Cleanup {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::converters::ConvertOptions;
    use std::io::Write;
    use std::sync::Arc;

    fn options(limits: ArchiveLimits) -> ConvertOptions {
        ConvertOptions {
            image_quality: 90,
            bitrate_kbps: None,
            video_scale: None,
            timeout: std::time::Duration::from_secs(30),
            max_file_size: 64 * 1024 * 1024,
            archive_limits: limits,
        }
    }

    fn noop() -> ProgressFn {
        Arc::new(|_| {})
    }

    #[tokio::test]
    async fn zip_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("not.txt");
        std::fs::write(&input, b"merhaba arsiv").unwrap();
        let archive = dir.path().join("not.zip");
        let target = crate::core::registry::format_by_id("zip").unwrap();
        create(
            &input,
            &archive,
            target,
            &options(ArchiveLimits::default()),
            noop(),
        )
        .await
        .unwrap();
        assert!(archive.exists());

        let out = dir.path().join("cikti");
        extract(
            &archive,
            &out,
            target,
            &options(ArchiveLimits::default()),
            noop(),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(out.join("not.txt")).unwrap(),
            b"merhaba arsiv"
        );
    }

    #[tokio::test]
    async fn targz_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("veri.csv");
        std::fs::write(&input, b"a,b\n1,2\n").unwrap();
        let archive = dir.path().join("veri.tar.gz");
        let target = crate::core::registry::format_by_id("tar.gz").unwrap();
        create(
            &input,
            &archive,
            target,
            &options(ArchiveLimits::default()),
            noop(),
        )
        .await
        .unwrap();
        let out = dir.path().join("cikti");
        extract(
            &archive,
            &out,
            target,
            &options(ArchiveLimits::default()),
            noop(),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(out.join("veri.csv")).unwrap(), b"a,b\n1,2\n");
    }

    #[tokio::test]
    async fn plain_tar_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("q.txt");
        std::fs::write(&input, b"duz tar").unwrap();
        let archive = dir.path().join("q.tar");
        let target = crate::core::registry::format_by_id("tar").unwrap();
        create(
            &input,
            &archive,
            target,
            &options(ArchiveLimits::default()),
            noop(),
        )
        .await
        .unwrap();
        let out = dir.path().join("cikti");
        extract(
            &archive,
            &out,
            target,
            &options(ArchiveLimits::default()),
            noop(),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(out.join("q.txt")).unwrap(), b"duz tar");
    }

    #[tokio::test]
    async fn tar_symlink_entry_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("link.tar");
        {
            let file = File::create(&archive_path).unwrap();
            let mut builder = tar::Builder::new(file);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_mode(0o777);
            header.set_cksum();
            builder
                .append_link(&mut header, "kisa", "/etc/passwd")
                .unwrap();
            builder.finish().unwrap();
        }
        let out = dir.path().join("cikti");
        let tar_fmt = crate::core::registry::format_by_id("tar").unwrap();
        let result = extract(
            &archive_path,
            &out,
            tar_fmt,
            &options(ArchiveLimits::default()),
            noop(),
        )
        .await;
        assert!(result.is_err());
        assert!(!out.exists());
    }

    #[tokio::test]
    async fn zip_bomb_ratio_is_rejected_and_cleaned_up() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("sifir.bin");
        std::fs::write(&input, vec![0u8; 2 * 1024 * 1024]).unwrap();
        let archive = dir.path().join("bomb.zip");
        let zip_fmt = crate::core::registry::format_by_id("zip").unwrap();
        create(
            &input,
            &archive,
            zip_fmt,
            &options(ArchiveLimits::default()),
            noop(),
        )
        .await
        .unwrap();

        let strict = ArchiveLimits {
            max_ratio: 2,
            ..ArchiveLimits::default()
        };
        let out = dir.path().join("bomb-cikti");
        let result = extract(&archive, &out, zip_fmt, &options(strict), noop()).await;
        assert!(result.is_err());
        assert!(!out.exists(), "kısmi çıktı temizlenmeliydi");
    }

    #[tokio::test]
    async fn traversal_entry_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let archive_path = dir.path().join("kotu.zip");
        {
            let file = File::create(&archive_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("../kacak.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"x").unwrap();
            writer.finish().unwrap();
        }
        let out = dir.path().join("cikti");
        let zip_fmt = crate::core::registry::format_by_id("zip").unwrap();
        let result = extract(
            &archive_path,
            &out,
            zip_fmt,
            &options(ArchiveLimits::default()),
            noop(),
        )
        .await;
        assert!(result.is_err());
        assert!(!out.exists());
        assert!(!dir.path().join("kacak.txt").exists());
    }

    #[test]
    fn relative_paths_are_validated() {
        assert_eq!(
            relative_safe_path(Path::new("a/b.txt")),
            Some(PathBuf::from("a/b.txt"))
        );
        assert_eq!(relative_safe_path(Path::new("/etc/passwd")), None);
        assert_eq!(relative_safe_path(Path::new("../x")), None);
        assert_eq!(relative_safe_path(Path::new("a/../../x")), None);
        assert_eq!(relative_safe_path(Path::new(".")), None);
    }
}
