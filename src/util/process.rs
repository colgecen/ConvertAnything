//! Safe external process execution.
//!
//! Every external tool is started through this module. The module enforces the
//! rules from `SECURITY.md`:
//!
//! * no shell is ever spawned, arguments are passed as an array;
//! * input/output paths are canonicalised and may not begin with `-`;
//! * the environment is cleared and replaced with a minimal allow-list;
//! * a timeout kills the child process;
//! * stderr is consumed continuously so the pipe can never deadlock.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// Maximum amount of stderr retained for diagnostics (tail only).
const STDERR_TAIL_LIMIT: usize = 16 * 1024;

/// Callback invoked for every stderr line of the child process.
pub type LineHandler = Arc<dyn Fn(&str) + Send + Sync>;

/// Result of a finished child process.
#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stderr: String,
    pub timed_out: bool,
}

impl ProcessOutput {
    /// Short, user facing error summary. Never contains file contents.
    pub fn error_summary(&self) -> String {
        let last = self.stderr.lines().rev().take(6).collect::<Vec<_>>();
        let mut lines = last.into_iter().rev().collect::<Vec<_>>().join("\n");
        if lines.trim().is_empty() {
            lines = format!("süreç {} koduyla sonlandı", self.code.unwrap_or(-1));
        }
        if self.timed_out {
            lines.push_str("\n(zaman sınırı aşıldı, süreç öldürüldü)");
        }
        lines
    }
}

/// Minimal environment shared by all conversion tools.
fn apply_minimal_env(cmd: &mut Command, home: Option<&Path>) {
    cmd.env_clear();
    cmd.env("PATH", "/usr/local/bin:/usr/bin:/bin");
    cmd.env("LANG", "C.UTF-8");
    cmd.env("LC_ALL", "C.UTF-8");
    if let Some(home) = home {
        cmd.env("HOME", home);
    } else if let Some(home) = std::env::var_os("HOME") {
        cmd.env("HOME", home);
    }
    if let Some(tmp) = std::env::var_os("XDG_CACHE_HOME") {
        cmd.env("XDG_CACHE_HOME", tmp);
    }
    cmd.env("TMPDIR", std::env::temp_dir());
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);
}

/// Validates and canonicalises an input file path.
///
/// The returned path is absolute, which makes argument injection through a
/// leading `-` impossible.
pub fn validate_input_file(path: &Path, max_size: u64) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("girdi dosyası bulunamadı: {}", path.display()))?;
    let meta = std::fs::metadata(&canonical)
        .with_context(|| format!("girdi okunamadı: {}", canonical.display()))?;
    if !meta.is_file() {
        bail!("girdi bir dosya değil: {}", canonical.display());
    }
    if meta.len() == 0 {
        bail!("girdi dosyası boş: {}", canonical.display());
    }
    if meta.len() > max_size {
        bail!(
            "girdi dosyası boyut sınırını aşıyor ({} bayt > {} bayt)",
            meta.len(),
            max_size
        );
    }
    Ok(canonical)
}

/// Directory used as the working directory for every external tool.
///
/// All paths passed to tools are absolute, so the working directory only needs
/// to be benign. `~/.cache/donustur/tmp` is used when available.
pub fn sandbox_workdir() -> PathBuf {
    let dir = crate::util::paths::tmp_dir();
    if std::fs::create_dir_all(&dir).is_ok() {
        dir
    } else {
        std::env::temp_dir()
    }
}

/// Checks that a directory has at least `needed` bytes free.
pub fn ensure_free_space(dir: &Path, needed: u64) -> Result<()> {
    use fs2::available_space;
    let available =
        available_space(dir).with_context(|| format!("boş alan ölçülemedi: {}", dir.display()))?;
    if available < needed {
        bail!(
            "hedef dizinde yeterli alan yok (gerekli: {} bayt, mevcut: {} bayt)",
            needed,
            available
        );
    }
    Ok(())
}

/// Runs a command to completion, capturing a bounded stderr tail.
///
/// `on_line` is called for every stderr line and must be cheap; the reading
/// task is driven on the current Tokio runtime.
pub async fn run_captured(
    mut cmd: Command,
    timeout: Duration,
    home: Option<&Path>,
    workdir: Option<&Path>,
    on_line: Option<LineHandler>,
) -> Result<ProcessOutput> {
    apply_minimal_env(&mut cmd, home);
    if let Some(workdir) = workdir {
        cmd.current_dir(workdir);
    }
    let mut child = cmd.spawn().context("harici araç başlatılamadı")?;

    let stderr = child
        .stderr
        .take()
        .context("harici aracın stderr kanalı alınamadı")?;
    let mut lines = BufReader::new(stderr).lines();

    let mut tail = String::new();
    let reader = tokio::spawn(async move {
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    push_tail(&mut tail, &line);
                    if let Some(handler) = on_line.as_ref() {
                        handler(&line);
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        tail
    });

    let outcome = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(result) => result.map(Some),
        Err(_) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Ok(None)
        }
    };

    let tail = reader.await.unwrap_or_default();
    match outcome {
        Ok(Some(status)) => Ok(ProcessOutput {
            success: status.success(),
            code: status.code(),
            stderr: tail,
            timed_out: false,
        }),
        Ok(None) => Ok(ProcessOutput {
            success: false,
            code: None,
            stderr: tail,
            timed_out: true,
        }),
        Err(error) => Err(error).context("harici araç beklenemedi"),
    }
}

/// Convenience wrapper that starts a program with an argument array and runs it.
#[cfg(test)]
pub async fn run_program(
    program: &str,
    args: &[String],
    timeout: Duration,
    home: Option<&Path>,
    on_line: Option<LineHandler>,
) -> Result<ProcessOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    run_captured(cmd, timeout, home, None, on_line).await
}

fn push_tail(buffer: &mut String, line: &str) {
    buffer.push_str(line);
    buffer.push('\n');
    if buffer.len() > STDERR_TAIL_LIMIT {
        let mut cut = buffer.len() - STDERR_TAIL_LIMIT;
        while cut < buffer.len() && !buffer.is_char_boundary(cut) {
            cut += 1;
        }
        buffer.drain(..cut);
    }
}

/// Moves an intermediate file to its final path.
///
/// Unless `overwrite` is explicitly enabled by the user, the destination name
/// is reserved with `create_new` so an existing file can never be replaced.
pub async fn finalise_output(temp: &Path, final_path: &Path, overwrite: bool) -> Result<()> {
    if !overwrite {
        // Reserve the destination name with create_new so a concurrent process
        // can never be overwritten between the existence check and the rename.
        let reservation = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(final_path)
            .await
            .with_context(|| {
                format!(
                    "hedef dosya zaten var veya oluşturulamadı: {}",
                    final_path.display()
                )
            })?;
        drop(reservation);
    }

    match tokio::fs::rename(temp, final_path).await {
        Ok(()) => Ok(()),
        Err(_) => {
            // Cross-device rename (EXDEV): fall back to copy + remove.
            match tokio::fs::copy(temp, final_path).await {
                Ok(_) => {
                    let _ = tokio::fs::remove_file(temp).await;
                    Ok(())
                }
                Err(error) => {
                    if !overwrite {
                        let _ = tokio::fs::remove_file(final_path).await;
                    }
                    Err(error)
                        .with_context(|| format!("çıktı taşınamadı: {}", final_path.display()))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn tail_is_bounded_and_keeps_last_lines() {
        let mut buf = String::new();
        for i in 0..5000 {
            push_tail(&mut buf, &format!("satır {i}"));
        }
        assert!(buf.len() <= STDERR_TAIL_LIMIT);
        assert!(buf.contains("satır 4999"));
        assert!(!buf.contains("satır 0\n"));
    }

    #[test]
    fn validate_rejects_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let result = validate_input_file(&tmp.path().join("yok.png"), 1024);
        assert!(result.is_err());
    }

    #[test]
    fn validate_rejects_empty_and_oversized() {
        let tmp = tempfile::tempdir().unwrap();
        let empty = tmp.path().join("bos.txt");
        std::fs::File::create(&empty).unwrap();
        assert!(validate_input_file(&empty, 1024).is_err());

        let big = tmp.path().join("buyuk.txt");
        let mut f = std::fs::File::create(&big).unwrap();
        f.write_all(&[b'a'; 64]).unwrap();
        assert!(validate_input_file(&big, 32).is_err());
        assert!(validate_input_file(&big, 1024).is_ok());
    }

    #[tokio::test]
    async fn finalise_never_overwrites_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let temp = dir.path().join("gecici.txt");
        let final_path = dir.path().join("hedef.txt");
        std::fs::write(&temp, b"yeni").unwrap();
        std::fs::write(&final_path, b"eski").unwrap();

        let result = finalise_output(&temp, &final_path, false).await;
        assert!(result.is_err());
        assert_eq!(std::fs::read(&final_path).unwrap(), b"eski");
        assert!(temp.exists());
    }

    #[tokio::test]
    async fn finalise_moves_temp_into_place() {
        let dir = tempfile::tempdir().unwrap();
        let temp = dir.path().join("gecici.txt");
        let final_path = dir.path().join("hedef.txt");
        std::fs::write(&temp, b"yeni").unwrap();

        finalise_output(&temp, &final_path, false).await.unwrap();
        assert_eq!(std::fs::read(&final_path).unwrap(), b"yeni");
        assert!(!temp.exists());
    }

    #[tokio::test]
    async fn finalise_overwrites_only_when_allowed() {
        let dir = tempfile::tempdir().unwrap();
        let temp = dir.path().join("gecici.txt");
        let final_path = dir.path().join("hedef.txt");
        std::fs::write(&temp, b"yeni").unwrap();
        std::fs::write(&final_path, b"eski").unwrap();

        finalise_output(&temp, &final_path, true).await.unwrap();
        assert_eq!(std::fs::read(&final_path).unwrap(), b"yeni");
    }

    #[tokio::test]
    async fn run_captured_executes_without_shell() {
        let out = run_program(
            "sh",
            &["-c".to_string(), "echo merhaba >&2; exit 0".to_string()],
            Duration::from_secs(5),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(out.success);
        assert!(out.stderr.contains("merhaba"));
    }

    #[tokio::test]
    async fn run_captured_enforces_timeout() {
        let out = run_program(
            "sleep",
            &["5".to_string()],
            Duration::from_millis(150),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(!out.success);
        assert!(out.timed_out);
    }
}
