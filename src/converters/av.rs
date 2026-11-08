//! Audio/video conversion through FFmpeg.
//!
//! FFmpeg is never bundled. Arguments are passed as an array (never through a
//! shell), the environment is sanitised by [`crate::util::process`], progress
//! is parsed from `-progress pipe:2`, and the job is killed on timeout.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::converters::{report, ConvertRequest, ProgressFn};
use crate::core::detector;
use crate::util::process;

/// Parses `HH:MM:SS.mmm` (or `MM:SS.mmm`) into seconds.
pub fn parse_timestamp(value: &str) -> Option<f64> {
    let mut parts = value.trim().split(':').collect::<Vec<_>>();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let seconds: f64 = parts.pop()?.parse().ok()?;
    let minutes: f64 = parts.pop()?.parse().ok()?;
    let hours: f64 = if let Some(hours) = parts.pop() {
        hours.parse().ok()?
    } else {
        0.0
    };
    if !(0.0..=3600.0).contains(&seconds)
        || !(0.0..=60.0).contains(&minutes)
        || !(0.0..=1000.0).contains(&hours)
    {
        return None;
    }
    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

/// Parses FFmpeg's `Duration:` status line.
fn parse_duration_line(line: &str) -> Option<f64> {
    let rest = line.trim().strip_prefix("Duration:")?;
    let stamp = rest.trim().split(',').next()?.trim();
    parse_timestamp(stamp)
}

/// Sanitises a user supplied scale expression before it reaches the filter graph.
fn safe_scale(scale: &str) -> Option<String> {
    let scale = scale.trim();
    if scale.is_empty() || scale.len() > 32 {
        return None;
    }
    if scale
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '+' | '.' | '/'))
    {
        Some(scale.to_string())
    } else {
        None
    }
}

fn bitrate_arg(options: &crate::converters::ConvertOptions) -> Option<String> {
    options
        .bitrate_kbps
        .filter(|kbps| (8..=200_000).contains(kbps))
        .map(|kbps| format!("{kbps}k"))
}

/// Builds the FFmpeg argument vector. Pure function, unit tested.
pub fn build_args(
    input: &Path,
    output: &Path,
    target_id: &str,
    options: &crate::converters::ConvertOptions,
) -> Result<Vec<String>> {
    if !input.is_absolute() || !output.is_absolute() {
        bail!("FFmpeg için mutlak yollar gerekli");
    }
    let input = input.to_string_lossy().to_string();
    let output = output.to_string_lossy().to_string();
    if input.starts_with('-') || output.starts_with('-') {
        bail!("geçersiz yol argümanı");
    }

    let mut args: Vec<String> = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "info",
        "-progress",
        "pipe:2",
        "-nostats",
        "-y",
        "-map_metadata",
        "-1",
        "-i",
    ]
    .map(String::from)
    .to_vec();
    args.push(input);

    let bitrate = bitrate_arg(options);
    let scale = options.video_scale.as_deref().and_then(safe_scale);
    let is_audio_target = matches!(
        target_id,
        "mp3" | "wav" | "flac" | "ogg" | "opus" | "aac" | "m4a"
    );

    if is_audio_target {
        args.push("-vn".to_string());
        match target_id {
            "mp3" => {
                args.extend(["-c:a", "libmp3lame"].map(String::from));
                match &bitrate {
                    Some(rate) => args.extend(["-b:a".to_string(), rate.clone()]),
                    None => args.extend(["-q:a", "2"].map(String::from)),
                }
            }
            "wav" => args.extend(["-c:a", "pcm_s16le"].map(String::from)),
            "flac" => args.extend(["-c:a", "flac"].map(String::from)),
            "ogg" => {
                args.extend(["-c:a", "libvorbis"].map(String::from));
                if let Some(rate) = &bitrate {
                    args.extend(["-b:a".to_string(), rate.clone()]);
                }
            }
            "opus" => {
                args.extend(["-c:a", "libopus"].map(String::from));
                if let Some(rate) = &bitrate {
                    args.extend(["-b:a".to_string(), rate.clone()]);
                }
            }
            "aac" | "m4a" => {
                args.extend(["-c:a", "aac"].map(String::from));
                if let Some(rate) = &bitrate {
                    args.extend(["-b:a".to_string(), rate.clone()]);
                }
            }
            _ => unreachable!(),
        }
    } else {
        match target_id {
            "mp4" | "mov" => {
                args.extend(["-c:v", "libx264", "-preset", "medium"].map(String::from));
                match &bitrate {
                    Some(rate) => args.extend(["-b:v".to_string(), rate.clone()]),
                    None => args.extend(["-crf", "23"].map(String::from)),
                }
                args.extend(["-c:a", "aac", "-b:a", "192k"].map(String::from));
                if target_id == "mp4" {
                    args.extend(["-movflags", "+faststart"].map(String::from));
                }
                if let Some(scale) = &scale {
                    args.extend(["-vf".to_string(), format!("scale={scale}")]);
                }
            }
            "mkv" => {
                args.extend(["-c:v", "libx264", "-preset", "medium"].map(String::from));
                match &bitrate {
                    Some(rate) => args.extend(["-b:v".to_string(), rate.clone()]),
                    None => args.extend(["-crf", "23"].map(String::from)),
                }
                args.extend(["-c:a", "aac"].map(String::from));
                if let Some(scale) = &scale {
                    args.extend(["-vf".to_string(), format!("scale={scale}")]);
                }
            }
            "webm" => {
                args.extend(["-c:v", "libvpx-vp9", "-crf", "32", "-b:v", "0"].map(String::from));
                if let Some(scale) = &scale {
                    args.extend(["-vf".to_string(), format!("scale={scale}")]);
                }
                args.extend(["-c:a", "libopus"].map(String::from));
            }
            "avi" => {
                args.extend(["-c:v", "mpeg4", "-q:v", "5"].map(String::from));
                args.extend(["-c:a", "libmp3lame"].map(String::from));
                if let Some(scale) = &scale {
                    args.extend(["-vf".to_string(), format!("scale={scale}")]);
                }
            }
            "gif" => {
                args.push("-an".to_string());
                let scale_expr = scale.unwrap_or_else(|| "w='min(640,iw)':h=-2".to_string());
                let filter = format!(
                    "fps=12,scale={scale_expr}:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse"
                );
                args.extend([
                    "-vf".to_string(),
                    filter,
                    "-loop".to_string(),
                    "0".to_string(),
                ]);
            }
            other => bail!("desteklenmeyen FFmpeg hedefi: {other}"),
        }
    }

    args.push(output);
    Ok(args)
}

#[derive(Debug, Default)]
struct AvProgress {
    total: Option<f64>,
    last_percent: u8,
}

/// Converts audio/video.
pub async fn convert(request: &ConvertRequest, progress: ProgressFn) -> Result<()> {
    let input = request
        .input
        .canonicalize()
        .with_context(|| format!("girdi bulunamadı: {}", request.input.display()))?;
    let output: PathBuf = request.output.clone();
    if !output.is_absolute() {
        bail!("çıktı yolu mutlak değil");
    }
    let args = build_args(&input, &output, &request.target_id, &request.options)?;

    let ffmpeg = detector::find_binary("ffmpeg").unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let state = Arc::new(Mutex::new(AvProgress::default()));
    let line_state = state.clone();
    let line_progress = progress.clone();

    report(&progress, 0);

    let handler: process::LineHandler = Arc::new(move |line| {
        if let Ok(mut state) = line_state.lock() {
            if let Some(duration) = parse_duration_line(line) {
                if duration > 0.0 {
                    state.total = Some(duration);
                }
            }
            let current = if let Some(value) = line.trim().strip_prefix("out_time_us=") {
                value.parse::<f64>().ok().map(|us| us / 1_000_000.0)
            } else if let Some(value) = line.trim().strip_prefix("out_time_ms=") {
                value.parse::<f64>().ok().map(|us| us / 1_000_000.0)
            } else {
                None
            };
            if let (Some(current), Some(total)) = (current, state.total) {
                if total > 0.0 {
                    let pct = ((current / total) * 100.0).clamp(0.0, 99.0) as u8;
                    if pct > state.last_percent {
                        state.last_percent = pct;
                        report(&line_progress, pct);
                    }
                }
            }
            if line.trim() == "progress=end" {
                report(&line_progress, 100);
            }
        }
    });

    let timeout = if request.options.timeout.is_zero() {
        Duration::from_secs(30 * 60)
    } else {
        request.options.timeout
    };

    let mut cmd = tokio::process::Command::new(ffmpeg);
    cmd.args(&args);

    let result = process::run_captured(cmd, timeout, None, Some(handler)).await?;
    if result.timed_out {
        bail!("FFmpeg zaman sınırını aştı ve öldürüldü");
    }
    if !result.success {
        bail!("FFmpeg başarısız oldu: {}", result.error_summary());
    }
    let meta = tokio::fs::metadata(&output)
        .await
        .context("FFmpeg çıktısı oluşmadı")?;
    if meta.len() == 0 {
        bail!("FFmpeg boş bir çıktı üretti");
    }
    report(&progress, 100);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::converters::archive::ArchiveLimits;
    use crate::converters::ConvertOptions;

    fn options() -> ConvertOptions {
        ConvertOptions {
            image_quality: 90,
            bitrate_kbps: Some(128),
            video_scale: Some("1280:-2".into()),
            timeout: Duration::from_secs(60),
            max_file_size: 1024 * 1024,
            archive_limits: ArchiveLimits::default(),
        }
    }

    #[test]
    fn parses_timestamps() {
        assert_eq!(parse_timestamp("01:02:03.5"), Some(3723.5));
        assert_eq!(parse_timestamp("02:03"), Some(123.0));
        assert_eq!(parse_timestamp("bozuk"), None);
        assert_eq!(parse_timestamp("99:99:99"), None);
    }

    #[test]
    fn parses_duration_line() {
        let line = "  Duration: 00:01:30.25, start: 0.000000, bitrate: 120 kb/s";
        assert_eq!(parse_duration_line(line), Some(90.25));
        assert_eq!(parse_duration_line("Duration: N/A, bitrate: N/A"), None);
    }

    #[test]
    fn args_are_array_safe_and_use_absolute_paths() {
        let args = build_args(
            Path::new("/tmp/girdi.mp4"),
            Path::new("/tmp/cikti.webm"),
            "webm",
            &options(),
        )
        .unwrap();
        assert!(args.contains(&"/tmp/girdi.mp4".to_string()));
        assert!(args.contains(&"/tmp/cikti.webm".to_string()));
        assert!(args.contains(&"libvpx-vp9".to_string()));
        assert!(args.iter().any(|a| a == "scale=1280:-2"));
        assert!(!args.iter().any(|a| a.contains(';')));
    }

    #[test]
    fn rejects_relative_paths() {
        assert!(build_args(
            Path::new("girdi.mp4"),
            Path::new("/tmp/c.mp4"),
            "mp4",
            &options()
        )
        .is_err());
    }

    #[test]
    fn audio_target_has_no_video_stream() {
        let args = build_args(
            Path::new("/tmp/a.mp4"),
            Path::new("/tmp/a.mp3"),
            "mp3",
            &options(),
        )
        .unwrap();
        assert!(args.contains(&"-vn".to_string()));
        assert!(args.contains(&"libmp3lame".to_string()));
    }

    #[test]
    fn gif_target_disables_audio_and_uses_palette() {
        let args = build_args(
            Path::new("/tmp/a.mp4"),
            Path::new("/tmp/a.gif"),
            "gif",
            &options(),
        )
        .unwrap();
        assert!(args.contains(&"-an".to_string()));
        assert!(args.iter().any(|a| a.contains("palettegen")));
    }

    #[test]
    fn unsafe_scale_is_ignored() {
        assert_eq!(safe_scale("1280:-2"), Some("1280:-2".to_string()));
        assert_eq!(safe_scale("1280:-2; rm -rf /"), None);
        assert_eq!(safe_scale(""), None);
    }
}
