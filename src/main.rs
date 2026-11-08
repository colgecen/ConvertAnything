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

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Dönüştür")
        .with_app_id("donustur")
        .with_inner_size([980.0, 680.0])
        .with_min_inner_size([760.0, 520.0]);
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Dönüştür",
        options,
        Box::new(move |cc| Ok(Box::new(app::DonusturApp::new(cc, config)))),
    )
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
