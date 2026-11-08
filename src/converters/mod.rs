//! Conversion engines.
//!
//! Every converter follows the same contract:
//! * validate the request,
//! * run the heavy work on a blocking thread or through a safe subprocess,
//! * write to the requested output path (the job layer owns finalisation),
//! * report progress as an integer percentage.

pub mod archive;
pub mod av;
pub mod document;
pub mod image;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};

use crate::core::registry::{self, MediaKind};
use crate::state::config::Config;

/// Progress callback; the value is clamped to 0..=100.
pub type ProgressFn = Arc<dyn Fn(u8) + Send + Sync>;

/// Reports progress, clamped to the valid range.
pub fn report(progress: &ProgressFn, value: u8) {
    progress(value.min(100));
}

/// User adjustable conversion options, derived from `config.toml`.
#[derive(Debug, Clone)]
pub struct ConvertOptions {
    pub image_quality: u8,
    pub bitrate_kbps: Option<u32>,
    pub video_scale: Option<String>,
    pub timeout: Duration,
    pub max_file_size: u64,
    pub archive_limits: archive::ArchiveLimits,
}

impl From<&Config> for ConvertOptions {
    fn from(config: &Config) -> Self {
        Self {
            image_quality: config.image_quality.clamp(1, 100),
            bitrate_kbps: config.bitrate_kbps,
            video_scale: config.video_scale.clone(),
            timeout: config.job_timeout(),
            max_file_size: config.max_file_size_bytes,
            archive_limits: archive::ArchiveLimits::default(),
        }
    }
}

/// A single fully owned conversion request.
#[derive(Debug, Clone)]
pub struct ConvertRequest {
    pub input: PathBuf,
    pub output: PathBuf,
    pub source: &'static registry::Format,
    pub target_id: String,
    pub options: ConvertOptions,
}

/// Dispatches a request to the right engine.
///
/// Archive targets are handled first: any format can be packed. Archive
/// sources are extracted into `output` (a directory).
pub async fn convert(request: &ConvertRequest, progress: ProgressFn) -> Result<PathBuf> {
    if !registry::is_supported(request.source, &request.target_id) {
        bail!(
            "{} → {} dönüşümü desteklenmiyor",
            request.source.id,
            request.target_id
        );
    }

    if request.target_id == registry::EXTRACT {
        archive::extract(
            &request.input,
            &request.output,
            request.source,
            &request.options,
            progress,
        )
        .await?;
        return Ok(request.output.clone());
    }

    if let Some(target) = registry::format_by_id(&request.target_id) {
        if target.kind == MediaKind::Archive {
            archive::create(
                &request.input,
                &request.output,
                target,
                &request.options,
                progress,
            )
            .await?;
            return Ok(request.output.clone());
        }
    }

    match request.source.kind {
        MediaKind::Image => {
            image::convert(
                &request.input,
                &request.output,
                &request.target_id,
                request.options.image_quality,
                progress,
            )
            .await?;
        }
        MediaKind::Audio | MediaKind::Video => {
            av::convert(request, progress).await?;
        }
        MediaKind::Document => {
            document::convert(request, progress).await?;
        }
        MediaKind::Archive => {
            bail!("arşiv kaynağı yalnızca ayıklanabilir");
        }
    }

    Ok(request.output.clone())
}
