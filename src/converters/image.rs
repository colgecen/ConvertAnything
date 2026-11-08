//! Image conversion backed by the pure-Rust `image` crate.

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::webp::WebPEncoder;
use image::{DynamicImage, ExtendedColorType, ImageFormat};

use super::{report, ProgressFn};

/// Decodes `input`, re-encodes it as `target_id` and writes `output`.
///
/// Runs entirely on a blocking thread. The image crate validates the payload,
/// so a file with a misleading extension is rejected here.
pub async fn convert(
    input: &Path,
    output: &Path,
    target_id: &str,
    quality: u8,
    progress: ProgressFn,
) -> Result<()> {
    let input: PathBuf = input.to_path_buf();
    let output: PathBuf = output.to_path_buf();
    let target = target_id.to_string();
    let quality = quality.clamp(1, 100);

    tokio::task::spawn_blocking(move || -> Result<()> {
        report(&progress, 5);
        let mut reader = image::ImageReader::open(&input)
            .with_context(|| format!("görsel açılamadı: {}", input.display()))?
            .with_guessed_format()
            .context("görsel biçimi algılanamadı")?;
        // Decompression-bomb guard: bound dimensions and total allocation.
        reader.limits(decode_limits());
        let image = reader
            .decode()
            .with_context(|| format!("görsel çözümlenemedi: {}", input.display()))?;
        report(&progress, 55);

        write_image(&image, &output, &target, quality)?;
        report(&progress, 100);
        Ok(())
    })
    .await
    .context("görsel dönüşüm görevi çöktü")??;

    Ok(())
}

/// Decoder limits sized for desktop use, not for gigantic satellite imagery.
fn decode_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(30_000);
    limits.max_image_height = Some(30_000);
    limits.max_alloc = Some(1024 * 1024 * 1024);
    limits
}

fn write_image(image: &DynamicImage, output: &Path, target: &str, quality: u8) -> Result<()> {
    let file = std::fs::File::create(output)
        .with_context(|| format!("çıktı oluşturulamadı: {}", output.display()))?;
    let mut writer = BufWriter::new(file);

    match target {
        "jpg" => {
            let rgb = image.to_rgb8();
            let mut encoder = JpegEncoder::new_with_quality(&mut writer, quality);
            encoder
                .encode_image(&DynamicImage::ImageRgb8(rgb))
                .context("JPEG kodlanamadı")?;
        }
        "webp" => {
            // The pure-Rust encoder only supports lossless VP8L.
            let rgba = image.to_rgba8();
            let (width, height) = rgba.dimensions();
            let encoder = WebPEncoder::new_lossless(&mut writer);
            encoder
                .encode(rgba.as_raw(), width, height, ExtendedColorType::Rgba8)
                .context("WebP kodlanamadı")?;
        }
        other => {
            let format = match other {
                "png" => ImageFormat::Png,
                "bmp" => ImageFormat::Bmp,
                "tiff" => ImageFormat::Tiff,
                "gif" => ImageFormat::Gif,
                "ico" => ImageFormat::Ico,
                unknown => bail!("desteklenmeyen görsel hedefi: {unknown}"),
            };
            image
                .write_to(&mut writer, format)
                .with_context(|| format!("{other} kodlanamadı"))?;
        }
    }

    writer.flush().context("çıktı diske yazılamadı")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::Arc;

    fn progress_counter() -> (ProgressFn, Arc<AtomicU8>) {
        let max = Arc::new(AtomicU8::new(0));
        let sink = max.clone();
        (
            Arc::new(move |value: u8| {
                sink.fetch_max(value, Ordering::SeqCst);
            }),
            max,
        )
    }

    fn sample_png(path: &Path) {
        let image = image::RgbaImage::from_fn(8, 8, |x, y| {
            image::Rgba([(x * 30) as u8, (y * 30) as u8, 120, 255])
        });
        image.save(path).unwrap();
    }

    #[tokio::test]
    async fn converts_png_to_jpg() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("a.png");
        let output = dir.path().join("a.jpg");
        sample_png(&input);
        let (progress, max) = progress_counter();
        convert(&input, &output, "jpg", 80, progress).await.unwrap();
        assert!(output.exists());
        assert_eq!(max.load(Ordering::SeqCst), 100);
        let decoded = image::open(&output).unwrap();
        assert_eq!(decoded.dimensions(), (8, 8));
    }

    #[tokio::test]
    async fn converts_png_to_every_supported_target() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("kaynak.png");
        sample_png(&input);
        for target in ["jpg", "webp", "bmp", "tiff", "gif", "ico"] {
            let output = dir.path().join(format!("cikti.{target}"));
            let (progress, _) = progress_counter();
            convert(&input, &output, target, 85, progress)
                .await
                .unwrap_or_else(|error| panic!("{target} dönüşümü başarısız: {error:#}"));
            let meta = std::fs::metadata(&output).unwrap();
            assert!(meta.len() > 32, "{target} çıktısı çok küçük");
            let decoded = image::open(&output)
                .unwrap_or_else(|error| panic!("{target} çıktısı okunamadı: {error}"));
            assert_eq!(decoded.dimensions(), (8, 8), "{target} boyutu yanlış");
        }
    }

    #[tokio::test]
    async fn rejects_invalid_payload_despite_extension() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("sahte.png");
        let output = dir.path().join("sahte.jpg");
        std::fs::write(&input, b"bu bir gorsel degil").unwrap();
        let (progress, _) = progress_counter();
        assert!(convert(&input, &output, "jpg", 80, progress).await.is_err());
        assert!(!output.exists());
    }

    #[tokio::test]
    async fn rejects_unknown_target() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("a.png");
        let output = dir.path().join("a.xyz");
        sample_png(&input);
        let (progress, _) = progress_counter();
        assert!(convert(&input, &output, "xyz", 80, progress).await.is_err());
    }
}
