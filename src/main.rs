#![forbid(unsafe_code)]
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

mod app;
mod converters;
mod core;
mod state;
mod ui;
mod util;

use state::config::Config;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result<()> {
    init_tracing();
    let config = Config::load();

    if let Err(error) = util::paths::ensure_dir(&util::paths::tmp_dir()) {
        tracing::warn!("geçici dizin oluşturulamadı: {error}");
    }
    // Remove leftovers from a previous crash.
    util::paths::clean_tmp_dir();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Dönüştür")
            .with_inner_size([980.0, 680.0])
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Dönüştür",
        options,
        Box::new(move |cc| Ok(Box::new(app::DonusturApp::new(cc, config)))),
    )
}

fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("donustur=info,warn"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}
