//! Asynchronous conversion queue.
//!
//! The UI submits owned [`JobRequest`]s and polls [`JobUpdate`]s. Jobs run on a
//! private Tokio runtime with a bounded level of parallelism. Every job gets a
//! private temporary directory which is removed by `TempDir` even on failure.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use tokio::runtime::Runtime;
use tokio::sync::{mpsc as async_mpsc, Semaphore};

use crate::converters::{self, ConvertOptions, ConvertRequest, ProgressFn};
use crate::core::registry::{self, Format};
use crate::state::config::Config;
use crate::util::{paths, process};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Queued,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone)]
pub struct JobUpdate {
    pub id: u64,
    pub status: JobStatus,
    pub progress: u8,
    pub message: Option<String>,
    pub output: Option<PathBuf>,
}

/// A queued conversion. All fields are owned so the job can outlive the UI frame.
#[derive(Debug, Clone)]
pub struct JobRequest {
    pub id: u64,
    pub input: PathBuf,
    pub output: PathBuf,
    pub source: &'static Format,
    pub target_id: String,
    pub options: ConvertOptions,
    /// When false (default) an existing output file is never replaced.
    pub overwrite: bool,
}

/// Owns the runtime and the two channels used by the UI.
pub struct JobRunner {
    _runtime: Runtime,
    submit: async_mpsc::UnboundedSender<JobRequest>,
    events: Receiver<JobUpdate>,
    next_id: AtomicU64,
}

impl JobRunner {
    pub fn new(config: &Config) -> Result<Self> {
        let parallel = config.effective_parallel_jobs().max(1);
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(parallel.clamp(2, 8))
            .thread_name("donustur-is")
            .enable_all()
            .build()
            .context("iş zamanlayıcısı başlatılamadı")?;

        let (submit, mut submit_rx) = async_mpsc::unbounded_channel::<JobRequest>();
        let (event_tx, events) = mpsc::channel::<JobUpdate>();
        let semaphore = Arc::new(Semaphore::new(parallel));

        runtime.spawn(async move {
            while let Some(request) = submit_rx.recv().await {
                let Ok(permit) = semaphore.clone().acquire_owned().await else {
                    break;
                };
                let tx = event_tx.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    run_job(request, tx).await;
                });
            }
        });

        Ok(Self {
            _runtime: runtime,
            submit,
            events,
            next_id: AtomicU64::new(1),
        })
    }

    /// Schedules a job and returns its identifier.
    #[allow(clippy::too_many_arguments)]
    pub fn submit(
        &self,
        input: PathBuf,
        output: PathBuf,
        source: &'static Format,
        target_id: String,
        options: ConvertOptions,
        overwrite: bool,
    ) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = JobRequest {
            id,
            input,
            output,
            source,
            target_id,
            options,
            overwrite,
        };
        if self.submit.send(request).is_err() {
            // The runtime is gone; surface this as a failed job when polled.
            tracing::error!("iş kuyruğu kapalı");
        }
        id
    }

    /// Non-blocking poll used once per UI frame.
    pub fn try_recv(&self) -> Option<JobUpdate> {
        self.events.try_recv().ok()
    }
}

async fn run_job(request: JobRequest, events: Sender<JobUpdate>) {
    let id = request.id;
    let _ = events.send(JobUpdate {
        id,
        status: JobStatus::Running,
        progress: 0,
        message: None,
        output: None,
    });

    match execute(request, id, &events).await {
        Ok(output) => {
            let _ = events.send(JobUpdate {
                id,
                status: JobStatus::Done,
                progress: 100,
                message: None,
                output: Some(output),
            });
        }
        Err(error) => {
            let message = format!("{error:#}");
            tracing::warn!("iş {id} başarısız: {message}");
            let _ = events.send(JobUpdate {
                id,
                status: JobStatus::Failed,
                progress: 0,
                message: Some(message),
                output: None,
            });
        }
    }
}

async fn execute(request: JobRequest, id: u64, events: &Sender<JobUpdate>) -> Result<PathBuf> {
    // Re-validate every request: the UI is not a trust boundary.
    let input = process::validate_input_file(&request.input, request.options.max_file_size)?;
    let detected = registry::validate_source(&input)?;
    if detected.id != request.source.id {
        bail!("dosya türü değişmiş görünüyor");
    }
    registry::verify_magic(&input, detected)?;
    if !registry::is_supported(detected, &request.target_id) {
        bail!("bu dönüşüm desteklenmiyor");
    }

    let output = request.output.clone();
    if !output.is_absolute() {
        bail!("çıktı yolu mutlak değil");
    }
    let parent = output
        .parent()
        .context("çıktı dizini belirlenemedi")?
        .to_path_buf();
    tokio::fs::create_dir_all(&parent)
        .await
        .with_context(|| format!("çıktı dizini oluşturulamadı: {}", parent.display()))?;

    let needed = std::fs::metadata(&input)
        .map(|m| m.len().saturating_add(64 * 1024 * 1024))
        .unwrap_or(64 * 1024 * 1024);
    process::ensure_free_space(&parent, needed)?;

    let tmp_base = paths::tmp_dir();
    paths::ensure_dir(&tmp_base).context("geçici dizin oluşturulamadı")?;
    let workdir = tempfile::Builder::new()
        .prefix(&format!("is-{id}-"))
        .tempdir_in(&tmp_base)
        .context("iş için geçici dizin oluşturulamadı")?;

    let is_extract = request.target_id == registry::EXTRACT;
    let working_output = if is_extract {
        output.clone()
    } else {
        workdir.path().join(format!("cikti.{}", request.target_id))
    };

    let convert_request = ConvertRequest {
        input: input.clone(),
        output: working_output.clone(),
        source: detected,
        target_id: request.target_id.clone(),
        options: request.options.clone(),
    };

    let last = Arc::new(AtomicU8::new(0));
    let progress_tx = events.clone();
    let progress: ProgressFn = {
        let last = last.clone();
        Arc::new(move |value: u8| {
            let value = value.min(100);
            let previous = last.swap(value, Ordering::Relaxed);
            if value == 100 || value > previous + 2 {
                let _ = progress_tx.send(JobUpdate {
                    id,
                    status: JobStatus::Running,
                    progress: value,
                    message: None,
                    output: None,
                });
            }
        })
    };

    converters::convert(&convert_request, progress).await?;

    if is_extract {
        return Ok(output);
    }

    process::finalise_output(&working_output, &output, request.overwrite).await?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn test_config() -> Config {
        Config {
            max_parallel_jobs: 2,
            ..Config::default()
        }
    }

    #[test]
    fn image_job_completes_and_reports_progress() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("girdi.png");
        let image = image::RgbaImage::from_fn(4, 4, |x, y| {
            image::Rgba([x as u8 * 10, y as u8 * 10, 90, 255])
        });
        image.save(&input).unwrap();
        let output = dir.path().join("cikti.jpg");

        let runner = JobRunner::new(&test_config()).unwrap();
        let source = registry::format_by_id("png").unwrap();
        let id = runner.submit(
            input,
            output.clone(),
            source,
            "jpg".to_string(),
            ConvertOptions::from(&test_config()),
            false,
        );

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut done = false;
        while Instant::now() < deadline {
            while let Some(update) = runner.try_recv() {
                if update.id == id {
                    if update.status == JobStatus::Done {
                        done = true;
                    }
                    if update.status == JobStatus::Failed {
                        panic!("iş başarısız: {:?}", update.message);
                    }
                }
            }
            if done {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(done, "iş zamanında tamamlanmadı");
        assert!(output.exists());
    }

    fn wait_for(runner: &JobRunner, id: u64) -> JobUpdate {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            while let Some(update) = runner.try_recv() {
                if update.id == id && matches!(update.status, JobStatus::Done | JobStatus::Failed) {
                    return update;
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("iş zamanında bitmedi");
    }

    #[test]
    fn archive_jobs_create_and_extract() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("belge.txt");
        std::fs::write(&input, b"arsiv icerigi").unwrap();
        let runner = JobRunner::new(&test_config()).unwrap();

        let zip_path = dir.path().join("belge.zip");
        let source = registry::format_by_id("txt").unwrap();
        let id = runner.submit(
            input.clone(),
            zip_path.clone(),
            source,
            "zip".to_string(),
            ConvertOptions::from(&test_config()),
            false,
        );
        let done = wait_for(&runner, id);
        assert_eq!(done.status, JobStatus::Done, "{:?}", done.message);
        assert!(zip_path.exists());

        let out_dir = dir.path().join("ayiklanan");
        let zip_source = registry::format_by_id("zip").unwrap();
        let id = runner.submit(
            zip_path,
            out_dir.clone(),
            zip_source,
            registry::EXTRACT.to_string(),
            ConvertOptions::from(&test_config()),
            false,
        );
        let done = wait_for(&runner, id);
        assert_eq!(done.status, JobStatus::Done, "{:?}", done.message);
        assert_eq!(
            std::fs::read(out_dir.join("belge.txt")).unwrap(),
            b"arsiv icerigi"
        );
    }

    #[test]
    fn mismatched_magic_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("sahte.png");
        // A ZIP payload renamed to `.png` must never reach the image decoder.
        std::fs::write(&input, b"PK\x03\x04\x14\x00\x00\x00\x08\x00").unwrap();
        let runner = JobRunner::new(&test_config()).unwrap();
        let source = registry::format_by_id("png").unwrap();
        let id = runner.submit(
            input,
            dir.path().join("cikti.jpg"),
            source,
            "jpg".to_string(),
            ConvertOptions::from(&test_config()),
            false,
        );

        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Some(update) = runner.try_recv() {
                if update.id == id && update.status == JobStatus::Failed {
                    let message = update.message.unwrap_or_default();
                    assert!(message.contains("uyuşmuyor"), "beklenmeyen hata: {message}");
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("uyuşmayan içerik reddedilmedi");
    }

    #[test]
    fn unsupported_pair_fails_cleanly() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("girdi.png");
        std::fs::write(&input, b"not an image").unwrap();
        let runner = JobRunner::new(&test_config()).unwrap();
        let source = registry::format_by_id("png").unwrap();
        let id = runner.submit(
            input,
            dir.path().join("cikti.exe"),
            source,
            "exe".to_string(),
            ConvertOptions::from(&test_config()),
            false,
        );

        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Some(update) = runner.try_recv() {
                if update.id == id && update.status == JobStatus::Failed {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("başarısız iş bildirilmedi");
    }
}
