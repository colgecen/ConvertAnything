//! Audio/video conversion through FFmpeg.
//!
//! FFmpeg is never bundled. Arguments are passed as an array (never through a
//! shell), the environment is sanitised by [`crate::util::process`], progress
//! is parsed from `-progress pipe:2`, and the job is killed on timeout.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::converters::{report, ConvertRequest, ProgressFn};
use crate::core::detector;
use crate::util::process;

/// Oldest FFmpeg major version accepted (see SECURITY.md §6).
pub const MIN_FFMPEG_MAJOR: u32 = 4;

/// Extracts the major version from an `ffmpeg -version` banner.
///
/// Accepts both `ffmpeg version 7.1.1` and git snapshots such as
/// `ffmpeg version n6.0-...`.
pub fn parse_ffmpeg_major(banner: &str) -> Option<u32> {
    let line = banner
        .lines()
        .find(|line| line.trim_start().starts_with("ffmpeg version"))?;
    let token = line.split_whitespace().nth(2)?;
    let digits: String = token
        .trim_start_matches('n')
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// Verifies the installed FFmpeg is recent enough. Result is cached.
fn ensure_ffmpeg_supported(ffmpeg: &Path) -> Result<()> {
    static CHECK: OnceLock<Result<(), String>> = OnceLock::new();
    let cached = CHECK.get_or_init(|| {
        let output = std::process::Command::new(ffmpeg)
            .arg("-version")
            .output()
            .map_err(|error| format!("FFmpeg sürümü okunamadı: {error}"))?;
        let banner = String::from_utf8_lossy(&output.stdout).into_owned();
        match parse_ffmpeg_major(&banner) {
            Some(major) if major >= MIN_FFMPEG_MAJOR => Ok(()),
            Some(major) => Err(format!(
                "FFmpeg sürümü çok eski (ana sürüm {major}, en az {MIN_FFMPEG_MAJOR} gerekli)"
            )),
            None => Err("FFmpeg sürüm bilgisi çözümlenemedi".to_string()),
        }
    });
    cached.clone().map_err(anyhow::Error::msg)
}

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

/// Set of encoders reported by `ffmpeg -encoders`, probed once per process.
///
/// Fedora's `ffmpeg-free` has no `libx264`, so the H.264 encoder must be chosen
/// at runtime instead of being hard-coded.
fn available_encoders() -> &'static HashSet<String> {
    static CACHE: OnceLock<HashSet<String>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let Ok(output) = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
        else {
            return HashSet::new();
        };
        parse_encoder_list(&String::from_utf8_lossy(&output.stdout))
    })
}

/// Parses the `ffmpeg -encoders` table into a set of encoder names.
fn parse_encoder_list(text: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(flags) = parts.next() else { continue };
        // Encoder rows start with a six-character flag column such as `V....D`.
        if flags.len() != 6 || !flags.chars().all(|c| c.is_ascii_uppercase() || c == '.') {
            continue;
        }
        if let Some(name) = parts.next() {
            // Section header rows look like `V..... = Video`.
            if name != "=" {
                set.insert(name.to_string());
            }
        }
    }
    set
}

/// Chooses the best available H.264 encoder.
fn h264_encoder() -> Option<&'static str> {
    let encoders = available_encoders();
    if encoders.contains("libx264") {
        Some("libx264")
    } else if encoders.contains("libopenh264") {
        Some("libopenh264")
    } else {
        None
    }
}

/// Appends H.264 encoder options, degrading gracefully.
fn push_h264_args(args: &mut Vec<String>, bitrate: &Option<String>) {
    match h264_encoder() {
        Some("libx264") => {
            args.extend(["-c:v", "libx264", "-preset", "medium"].map(String::from));
            match bitrate {
                Some(rate) => args.extend(["-b:v".to_string(), rate.clone()]),
                None => args.extend(["-crf", "23"].map(String::from)),
            }
        }
        Some("libopenh264") => {
            // OpenH264 has no CRF mode; a target bitrate is always required.
            args.extend(["-c:v", "libopenh264"].map(String::from));
            let rate = bitrate.clone().unwrap_or_else(|| "2M".to_string());
            args.extend(["-b:v".to_string(), rate]);
        }
        _ => {
            // Last resort that every FFmpeg build ships: MPEG-4 Part 2.
            args.extend(["-c:v", "mpeg4", "-q:v", "5"].map(String::from));
        }
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
        "-i",
    ]
    .map(String::from)
    .to_vec();
    args.push(input);
    // Strip metadata: it must be an output option, i.e. placed after `-i`.
    args.extend(["-map_metadata", "-1"].map(String::from));

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
                push_h264_args(&mut args, &bitrate);
                args.extend(["-c:a", "aac", "-b:a", "192k"].map(String::from));
                if target_id == "mp4" {
                    args.extend(["-movflags", "+faststart"].map(String::from));
                }
                if let Some(scale) = &scale {
                    args.extend(["-vf".to_string(), format!("scale={scale}")]);
                }
            }
            "mkv" => {
                push_h264_args(&mut args, &bitrate);
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
    ensure_ffmpeg_supported(&ffmpeg)?;
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

    let workdir = process::sandbox_workdir();
    let result =
        process::run_captured(cmd, timeout, None, Some(workdir.as_path()), Some(handler)).await?;
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
    fn parses_ffmpeg_version_banner() {
        assert_eq!(
            parse_ffmpeg_major("ffmpeg version 7.1.1 Copyright (c) 2000-2024"),
            Some(7)
        );
        assert_eq!(
            parse_ffmpeg_major("ffmpeg version n6.0-15-g1234ab Copyright"),
            Some(6)
        );
        assert_eq!(parse_ffmpeg_major("ffmpeg version 4.4.2"), Some(4));
        assert_eq!(parse_ffmpeg_major("tanınmayan çıktı"), None);
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

    fn ffmpeg_available() -> bool {
        crate::core::detector::find_binary("ffmpeg").is_some()
    }

    fn real_request(
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
                timeout: Duration::from_secs(120),
                max_file_size: 64 * 1024 * 1024,
                archive_limits: ArchiveLimits::default(),
            },
        }
    }

    #[test]
    fn encoder_list_is_parsed() {
        let table = "Encoders:\n V..... = Video\n ------\n V....D libx264              H.264\n A....D aac                  AAC\n";
        let set = parse_encoder_list(table);
        assert!(set.contains("libx264"));
        assert!(set.contains("aac"));
        assert!(!set.contains("Encoders:"));
        assert!(!set.contains("="));
    }

    fn encoder_present(name: &str) -> bool {
        available_encoders().contains(name)
    }

    /// End-to-end check against the real binary; skipped when ffmpeg is absent.
    ///
    /// Each target is only exercised when the required encoder exists, so the
    /// test is portable across Fedora (`ffmpeg-free`) and full builds.
    #[tokio::test]
    async fn real_ffmpeg_covers_every_target() {
        if !ffmpeg_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("kaynak.mp4");
        let generated = tokio::process::Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=160x120:rate=10",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440",
                "-t",
                "1",
                "-y",
            ])
            .arg(&input)
            .status()
            .await
            .expect("ffmpeg çalıştırılamadı");
        assert!(generated.success());

        // (target, required encoders)
        let targets: &[(&str, &[&str])] = &[
            ("mp4", &[]),
            ("mkv", &[]),
            ("mov", &[]),
            ("webm", &["libvpx-vp9", "libopus"]),
            ("avi", &["mpeg4", "libmp3lame"]),
            ("gif", &[]),
            ("mp3", &["libmp3lame"]),
            ("wav", &["pcm_s16le"]),
            ("flac", &["flac"]),
            ("ogg", &["libvorbis"]),
            ("opus", &["libopus"]),
            ("aac", &["aac"]),
            ("m4a", &["aac"]),
        ];

        let source = crate::core::registry::format_by_id("mp4").unwrap();
        for (target, encoders) in targets {
            if !encoders.iter().all(|name| encoder_present(name)) {
                continue;
            }
            let output = dir.path().join(format!("cikti.{target}"));
            let request = real_request(input.clone(), output.clone(), source, target);
            convert(&request, std::sync::Arc::new(|_| {}))
                .await
                .unwrap_or_else(|error| panic!("{target} dönüşümü başarısız: {error:#}"));
            let meta = std::fs::metadata(&output).unwrap();
            assert!(meta.len() > 512, "{target} çıktısı çok küçük");
        }
    }
}
