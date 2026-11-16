#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod app;
mod converters;
mod core;
mod i18n;
mod install;
mod integration;
mod state;
mod ui;
mod util;

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use i18n::t;
use state::config::Config;
use tracing_subscriber::EnvFilter;

use crate::converters::ConvertOptions;
use crate::core::detector;
use crate::core::job::{self, BatchItem, JobRunner, JobStatus};
use crate::ui::convert_view::ConvertView;

#[derive(Default)]
struct Cli {
    paths: Vec<PathBuf>,
    target: Option<String>,
    quick: bool,
    install_app: bool,
    uninstall_app: bool,
    install_integration: bool,
    help: bool,
    version: bool,
}

fn main() -> eframe::Result<()> {
    init_tracing();
    let cli = parse_args();
    let config = Config::load();
    i18n::set(i18n::resolve(&config.language));

    if cli.help {
        print_help();
        return Ok(());
    }
    if cli.version {
        println!("ConvertAnything {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if let Err(error) = util::paths::ensure_dir(&util::paths::tmp_dir()) {
        tracing::warn!("geçici dizin oluşturulamadı: {error}");
    }
    // Remove leftovers from a previous crash.
    util::paths::clean_tmp_dir();

    if cli.install_app {
        return run_install();
    }
    if cli.uninstall_app {
        return run_uninstall();
    }

    if cli.install_integration {
        match integration::ensure_installed() {
            Ok(installed) => {
                println!(
                    "{} {} {}",
                    t("Context menu installed:", "Sağ tık menüsü kuruldu:"),
                    installed.service_menus.len(),
                    t("menus", "menü")
                );
            }
            Err(error) => eprintln!(
                "{} {error:#}",
                t(
                    "Context menu could not be installed:",
                    "Sağ tık menüsü kurulamadı:"
                )
            ),
        }
        return Ok(());
    }

    if cli.quick || cli.target.is_some() {
        if let Some(target) = cli.target {
            return run_headless_convert(cli.paths, config, target);
        }
        return run_quick_convert(cli.paths, config, None);
    }

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(t("ConvertAnything", "Dönüştür"))
        .with_app_id("donustur")
        .with_inner_size([1000.0, 760.0])
        .with_min_inner_size([760.0, 560.0]);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        t("ConvertAnything", "Dönüştür"),
        options,
        Box::new(move |cc| Ok(Box::new(app::DonusturApp::new(cc, config, cli.paths)))),
    )
}

/// Installs the app for the current user and enables login autostart.
fn run_install() -> eframe::Result<()> {
    match install::install_app(true) {
        Ok(report) => {
            println!(
                "{} {}",
                t("Installed:", "Kuruldu:"),
                report.executable.display()
            );
            println!(
                "{} {}",
                t("Context menus:", "Sağ tık menüleri:"),
                report.service_menus
            );
            if let Some(autostart) = report.autostart {
                println!(
                    "{} {}",
                    t("Autostart:", "Otomatik başlatma:"),
                    autostart.display()
                );
            }
            Ok(())
        }
        Err(error) => {
            eprintln!("{} {error:#}", t("Install failed:", "Kurulum başarısız:"));
            std::process::exit(1);
        }
    }
}

fn run_uninstall() -> eframe::Result<()> {
    match install::uninstall() {
        Ok(()) => {
            println!(
                "{}",
                t(
                    "ConvertAnything was removed from this user account.",
                    "ConvertAnything bu kullanıcı hesabından kaldırıldı."
                )
            );
            Ok(())
        }
        Err(error) => {
            eprintln!(
                "{} {error:#}",
                t("Uninstall failed:", "Kaldırma başarısız:")
            );
            std::process::exit(1);
        }
    }
}

/// Converts immediately without opening a window. Used by the file-manager
/// context menu, which already knows the target format.
fn run_headless_convert(paths: Vec<PathBuf>, config: Config, target: String) -> eframe::Result<()> {
    match headless_convert(&paths, &config, &target) {
        Ok(count) => {
            let summary = t("Conversion complete", "Dönüşüm tamamlandı");
            let body = format!("{} {} ({target})", count, i18n::files_label(count));
            desktop_notify(summary, &body, false);
            println!("{body}");
            Ok(())
        }
        Err(error) => {
            let summary = t("Conversion failed", "Dönüşüm başarısız");
            let body = format!("{error:#}");
            desktop_notify(summary, &body, true);
            eprintln!("{body}");
            std::process::exit(1);
        }
    }
}

fn headless_convert(paths: &[PathBuf], config: &Config, target_id: &str) -> anyhow::Result<usize> {
    let tools = detector::detect_all();
    let mut view = ConvertView::new(
        config.image_quality,
        config.bitrate_kbps,
        config.video_scale.clone(),
    );
    for path in paths {
        view.add_path(path.clone());
    }
    if view.files.is_empty() {
        anyhow::bail!(
            "{}",
            t(
                "No supported file was selected.",
                "Desteklenen dosya seçilmedi."
            )
        );
    }
    let targets = view.available_targets(&tools);
    if !targets.iter().any(|format| format.id == target_id) {
        anyhow::bail!(
            "{} {target_id}",
            t(
                "This target format is not available:",
                "Bu hedef biçim kullanılamıyor:"
            )
        );
    }

    let runner = JobRunner::new(config)?;
    let options = ConvertOptions::from(config);
    let items: Vec<BatchItem> = view
        .files
        .iter()
        .map(|file| BatchItem {
            path: file.path.clone(),
            source: file.format,
        })
        .collect();
    let mut taken = HashSet::new();
    let submitted = job::submit_batch(
        &runner, &items, target_id, None, config, &options, &mut taken,
    );

    let mut active: HashSet<u64> = submitted.iter().map(|job| job.id).collect();
    let mut failures = Vec::new();
    let mut done = 0usize;
    let started = Instant::now();
    let deadline = config.job_timeout() + Duration::from_secs(60);

    while !active.is_empty() {
        while let Some(update) = runner.try_recv() {
            if !active.contains(&update.id) {
                continue;
            }
            match update.status {
                JobStatus::Done => {
                    active.remove(&update.id);
                    done += 1;
                }
                JobStatus::Failed => {
                    active.remove(&update.id);
                    failures.push(update.message.unwrap_or_else(|| "?".to_string()));
                }
                JobStatus::Queued | JobStatus::Running => {}
            }
        }
        if started.elapsed() > deadline {
            anyhow::bail!(
                "{}",
                t("Conversion timed out.", "Dönüşüm zaman aşımına uğradı.")
            );
        }
        std::thread::sleep(Duration::from_millis(120));
    }

    if !failures.is_empty() {
        anyhow::bail!(
            "{} {}",
            t("Some files failed:", "Bazı dosyalar başarısız:"),
            failures.join("; ")
        );
    }
    Ok(done)
}

/// Best-effort desktop notification.
///
/// `notify-send` can block on a broken notification service and, when launched
/// from an AppImage, keep the FUSE mount alive. A transient user unit gives it
/// its own process without inheriting the AppImage file descriptors.
fn desktop_notify(summary: &str, body: &str, error: bool) {
    use std::process::Stdio;
    let mut command = std::process::Command::new("systemd-run");
    command
        .arg("--user")
        .arg("--collect")
        .arg("--no-block")
        .arg("--quiet")
        .arg("notify-send")
        .arg("--app-name")
        .arg("ConvertAnything")
        .arg("--icon")
        .arg("donustur");
    if error {
        command.arg("--urgency").arg("critical");
    }
    command.arg(summary).arg(body);
    let _ = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Opens the small target chooser used by `donustur --convert FILE` when no
/// format was supplied.
fn run_quick_convert(
    paths: Vec<PathBuf>,
    config: Config,
    target: Option<String>,
) -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ConvertAnything")
        .with_app_id("donustur")
        .with_inner_size([520.0, 620.0])
        .with_min_inner_size([420.0, 440.0]);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "ConvertAnything",
        options,
        Box::new(move |cc| {
            ui::theme::install(&cc.egui_ctx);
            Ok(Box::new(ui::quick_convert::QuickConvert::new(
                paths, config, target,
            )))
        }),
    )
}

fn parse_args() -> Cli {
    let mut cli = Cli::default();
    let mut args = std::env::args_os().skip(1).peekable();
    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy();
        match text.as_ref() {
            "--convert" => {
                cli.quick = true;
                while let Some(next) = args.peek() {
                    let next_text = next.to_string_lossy();
                    if next_text.starts_with("--") {
                        break;
                    }
                    if let Some(path) = args.next() {
                        cli.paths.push(PathBuf::from(path));
                    }
                }
            }
            "--to" => {
                if let Some(target) = args.next() {
                    cli.target = Some(target.to_string_lossy().to_string());
                }
            }
            "--install-integration" => cli.install_integration = true,
            "--install" => cli.install_app = true,
            "--uninstall" => cli.uninstall_app = true,
            "--help" | "-h" => cli.help = true,
            "--version" | "-V" => cli.version = true,
            _ if !text.starts_with('-') => cli.paths.push(PathBuf::from(arg)),
            _ => {}
        }
    }
    cli
}

fn print_help() {
    println!(
        "ConvertAnything {}\n\n{}\n  donustur [FILES…]          {}\n  donustur --convert FILES… [--to FORMAT]\n  donustur --install | --uninstall\n\n{}\n  --convert FILES…        {}\n  --to FORMAT             {}\n  --install               {}\n  --uninstall             {}\n  --install-integration   {}\n  -h, --help              {}\n  -V, --version           {}",
        env!("CARGO_PKG_VERSION"),
        t("Usage:", "Kullanım:"),
        t(
            "open the main screen with the given files",
            "verilen dosyalarla ana ekranı açar"
        ),
        t("Options:", "Seçenekler:"),
        t(
            "open the target chooser for the given files",
            "verilen dosyalar için hedef biçim seçiciyi açar"
        ),
        t(
            "convert immediately to this format id (e.g. png)",
            "doğrudan bu biçime dönüştür (örn. png)"
        ),
        t(
            "install to ~/.local/bin, add context menus and enable login autostart",
            "~/.local/bin'e kur, sağ tık menülerini ekle ve oturum açılışında başlat"
        ),
        t(
            "remove the installed app, menus and autostart entry",
            "kurulu uygulamayı, menüleri ve otomatik başlatmayı kaldır"
        ),
        t(
            "only install the file manager context menu",
            "yalnızca dosya yöneticisi sağ tık menüsünü kur"
        ),
        t("show this help", "bu yardımı gösterir"),
        t("show the version", "sürümü gösterir"),
    );
}

/// Decodes the bundled PNG icon for the window manager / task bar.
fn window_icon() -> Option<egui::IconData> {
    const ICON: &[u8] = include_bytes!("../assets/donustur.png");
    let image = image::load_from_memory(ICON).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Some(egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    })
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("donustur=info,warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_icon_decodes() {
        let icon = super::window_icon().expect("gömülü ikon çözümlenemedi");
        assert_eq!(icon.width, 256);
        assert_eq!(icon.height, 256);
        assert_eq!(
            icon.rgba.len() as u32,
            icon.width * icon.height * 4,
            "RGBA tamponu boyutla uyuşmalı"
        );
    }
}
