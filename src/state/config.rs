//! `config.toml` loading, validation and atomic saving.
//!
//! The schema is forward/backward compatible: unknown keys are ignored and
//! missing keys fall back to [`Config::default`] through `#[serde(default)]`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::util::paths;

/// Default maximum input size: 4 GiB.
pub const DEFAULT_MAX_FILE_SIZE: u64 = 4 * 1024 * 1024 * 1024;
/// Default per-job timeout: 30 minutes.
pub const DEFAULT_JOB_TIMEOUT_SECS: u64 = 30 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Set to true once the first-run wizard has been completed or skipped.
    pub setup_done: bool,
    /// Tools the user explicitly chose to skip. Missing tools stay locked.
    pub skipped_tools: Vec<String>,
    /// Maximum accepted input file size in bytes.
    pub max_file_size_bytes: u64,
    /// Number of parallel jobs. 0 means "number of CPU cores".
    pub max_parallel_jobs: usize,
    /// Per-job wall clock limit in seconds.
    pub job_timeout_secs: u64,
    /// Default output directory. `None` means "next to the input file".
    pub output_dir: Option<PathBuf>,
    /// Whether the user allowed overwriting existing files. Off by default.
    pub allow_overwrite: bool,
    /// JPEG/WebP encoding quality (1-100).
    pub image_quality: u8,
    /// Default video/audio bitrate in kbit/s. `None` uses encoder defaults.
    pub bitrate_kbps: Option<u32>,
    /// Default scale expression such as `1280:-2`. `None` keeps the source size.
    pub video_scale: Option<String>,
    /// Preferred UI language. Only Turkish is shipped today.
    pub language: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            setup_done: false,
            skipped_tools: Vec::new(),
            max_file_size_bytes: DEFAULT_MAX_FILE_SIZE,
            max_parallel_jobs: 0,
            job_timeout_secs: DEFAULT_JOB_TIMEOUT_SECS,
            output_dir: None,
            allow_overwrite: false,
            image_quality: 90,
            bitrate_kbps: None,
            video_scale: None,
            language: "tr".to_string(),
        }
    }
}

impl Config {
    /// Loads the configuration. A missing or corrupt file yields defaults.
    pub fn load() -> Self {
        Self::load_from(&paths::config_file()).unwrap_or_default()
    }

    /// Fallible variant used by tests and by the settings screen.
    pub fn load_from(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("ayar dosyası okunamadı: {}", path.display()))?;
        let mut config: Config = toml::from_str(&text)
            .with_context(|| format!("ayar dosyası çözümlenemedi: {}", path.display()))?;
        config.normalise();
        Ok(config)
    }

    /// Loads defaults without touching the filesystem.
    #[cfg(test)]
    pub fn load_or_default(path: &Path) -> Self {
        Self::load_from(path).unwrap_or_default()
    }

    /// Saves the configuration atomically to the default location.
    pub fn save(&self) -> Result<()> {
        self.save_to(&paths::config_file())
    }

    /// Atomically writes the configuration, creating parent directories.
    ///
    /// A temporary file is written first and then renamed, so a crash cannot
    /// leave a half-written `config.toml` behind.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            paths::ensure_dir(parent)
                .with_context(|| format!("ayar dizini oluşturulamadı: {}", parent.display()))?;
        }
        let text = toml::to_string_pretty(self).context("ayar dosyası serileştirilemedi")?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(parent)
            .context("geçici ayar dosyası oluşturulamadı")?;
        std::io::Write::write_all(&mut tmp, text.as_bytes())
            .context("geçici ayar dosyası yazılamadı")?;
        tmp.as_file()
            .sync_all()
            .context("ayar dosyası diske yazılamadı")?;
        tmp.persist(path)
            .map_err(|e| e.error)
            .context("ayar dosyası yerine taşınamadı")?;
        Ok(())
    }

    /// Clamps out-of-range values loaded from a hand-edited file.
    pub fn normalise(&mut self) {
        self.image_quality = self.image_quality.clamp(1, 100);
        if self.max_file_size_bytes == 0 {
            self.max_file_size_bytes = DEFAULT_MAX_FILE_SIZE;
        }
        if self.job_timeout_secs == 0 {
            self.job_timeout_secs = DEFAULT_JOB_TIMEOUT_SECS;
        }
        if self.max_parallel_jobs > 64 {
            self.max_parallel_jobs = 64;
        }
        if self.language.trim().is_empty() {
            self.language = "tr".to_string();
        }
        // Only fixed, known tool identifiers may be skipped.
        self.skipped_tools
            .retain(|name| matches!(name.as_str(), "ffmpeg" | "soffice" | "pandoc" | "7z"));
        self.skipped_tools.sort();
        self.skipped_tools.dedup();
        if let Some(scale) = self.video_scale.as_ref() {
            let valid = !scale.is_empty()
                && scale.len() <= 32
                && scale
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | '+' | '.' | '/'));
            if !valid {
                self.video_scale = None;
            }
        }
    }

    /// Effective number of parallel jobs (never zero).
    pub fn effective_parallel_jobs(&self) -> usize {
        if self.max_parallel_jobs > 0 {
            self.max_parallel_jobs
        } else {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
                .clamp(1, 16)
        }
    }

    pub fn job_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.job_timeout_secs.clamp(5, 24 * 60 * 60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert!(!c.setup_done);
        assert_eq!(c.max_file_size_bytes, DEFAULT_MAX_FILE_SIZE);
        assert!(!c.allow_overwrite);
        assert!(c.effective_parallel_jobs() >= 1);
    }

    #[test]
    fn roundtrip_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let c = Config {
            setup_done: true,
            image_quality: 75,
            output_dir: Some(PathBuf::from("/tmp/çıktı")),
            ..Config::default()
        };
        c.save_to(&path).unwrap();

        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(c, loaded);
    }

    #[test]
    fn missing_keys_use_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "setup_done = true\n").unwrap();
        let loaded = Config::load_from(&path).unwrap();
        assert!(loaded.setup_done);
        assert_eq!(loaded.image_quality, 90);
    }

    #[test]
    fn corrupt_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "bu = [geçersiz").unwrap();
        assert_eq!(Config::load_or_default(&path), Config::default());
    }

    #[test]
    fn normalise_clamps_values_and_filters_tools() {
        let mut c = Config {
            image_quality: 200,
            max_file_size_bytes: 0,
            job_timeout_secs: 0,
            skipped_tools: vec![
                "ffmpeg".into(),
                "rm -rf /".into(),
                "ffmpeg".into(),
                "pandoc".into(),
            ],
            video_scale: Some("1280:-2; rm".into()),
            ..Config::default()
        };
        c.normalise();
        assert_eq!(c.image_quality, 100);
        assert_eq!(c.max_file_size_bytes, DEFAULT_MAX_FILE_SIZE);
        assert_eq!(c.job_timeout_secs, DEFAULT_JOB_TIMEOUT_SECS);
        assert_eq!(c.skipped_tools, vec!["ffmpeg", "pandoc"]);
        assert_eq!(c.video_scale, None);
    }

    #[test]
    fn valid_scale_is_kept() {
        let mut c = Config {
            video_scale: Some("1280:-2".into()),
            ..Config::default()
        };
        c.normalise();
        assert_eq!(c.video_scale.as_deref(), Some("1280:-2"));
    }
}
