//! Application state and the main update loop.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::RichText;

use crate::converters::ConvertOptions;
use crate::core::detector::{self, PackageManager, ToolStatus};
use crate::core::job::{JobRunner, JobStatus};
use crate::core::registry;
use crate::state::config::Config;
use crate::ui::convert_view::{ConvertView, ACCENT, DANGER, MUTED, SUCCESS};
use crate::ui::settings::{SettingsAction, SettingsView};
use crate::ui::setup_wizard::{SetupWizard, WizardAction};
use crate::ui::Screen;
use crate::util::paths;

struct JobRow {
    id: u64,
    name: String,
    target: String,
    output: Option<PathBuf>,
    status: JobStatus,
    progress: u8,
    message: Option<String>,
}

struct Toast {
    text: String,
    error: bool,
    until: Instant,
}

pub struct DonusturApp {
    config: Config,
    tools: Vec<ToolStatus>,
    manager: Option<PackageManager>,
    runner: Option<JobRunner>,
    jobs: Vec<JobRow>,
    screen: Screen,
    view: ConvertView,
    wizard: SetupWizard,
    settings: SettingsView,
    toast: Option<Toast>,
}

impl DonusturApp {
    pub fn new(cc: &eframe::CreationContext<'_>, mut config: Config) -> Self {
        configure_style(&cc.egui_ctx);
        let tools = detector::detect_all();
        let manager = detector::detect_package_manager();

        // If every tool is present there is nothing to ask the user.
        if !config.setup_done && detector::missing_tools(&tools).is_empty() {
            config.setup_done = true;
            let _ = config.save();
        }

        let runner = match JobRunner::new(&config) {
            Ok(runner) => Some(runner),
            Err(error) => {
                tracing::error!("iş kuyruğu başlatılamadı: {error:#}");
                None
            }
        };

        let view = ConvertView::new(
            config.image_quality,
            config.bitrate_kbps,
            config.video_scale.clone(),
        );

        let wizard = SetupWizard::new(&config, &tools);
        let settings = SettingsView::new(&config);

        Self {
            config,
            tools,
            manager,
            runner,
            jobs: Vec::new(),
            screen: Screen::Convert,
            view,
            wizard,
            settings,
            toast: None,
        }
    }

    fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            error,
            until: Instant::now() + Duration::from_secs(7),
        });
    }

    fn refresh_tools(&mut self) {
        self.tools = detector::detect_all();
        self.manager = detector::detect_package_manager();
    }

    fn poll_jobs(&mut self) {
        let mut updates = Vec::new();
        if let Some(runner) = &self.runner {
            while let Some(update) = runner.try_recv() {
                updates.push(update);
            }
        }
        for update in updates {
            if let Some(job) = self.jobs.iter_mut().find(|j| j.id == update.id) {
                job.status = update.status;
                job.progress = update.progress;
                job.message = update.message;
                if let Some(output) = update.output {
                    job.output = Some(output);
                }
                if update.status == JobStatus::Done {
                    job.progress = 100;
                }
            }
        }
    }

    fn any_job_active(&self) -> bool {
        self.jobs
            .iter()
            .any(|j| matches!(j.status, JobStatus::Queued | JobStatus::Running))
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        if self.wizard.open {
            return;
        }
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .map(|file| file.path().to_path_buf())
                .collect()
        });
        if dropped.is_empty() {
            return;
        }

        let mut added = 0usize;
        let mut rejected = 0usize;
        for path in dropped {
            if self.view.add_path(path) {
                added += 1;
            } else {
                rejected += 1;
            }
        }

        if rejected > 0 {
            self.notify(
                format!(
                    "{added} dosya eklendi, {rejected} dosya reddedildi (tür/içerik uyuşmazlığı)."
                ),
                rejected > added,
            );
        } else if added > 0 {
            self.notify(format!("{added} dosya eklendi."), false);
        }
    }

    fn enqueue(&mut self) {
        let Some(runner) = self.runner.as_ref() else {
            self.notify("İş kuyruğu başlatılamadı.", true);
            return;
        };
        let (files, target, view_output, quality, bitrate, scale) = (
            self.view.files.clone(),
            self.view.target.clone(),
            self.view.output_dir.clone(),
            self.view.image_quality,
            self.view.bitrate_kbps,
            self.view.video_scale.clone(),
        );
        let Some(target) = target else {
            self.notify("Önce bir hedef biçim seçin.", true);
            return;
        };
        if files.is_empty() {
            return;
        }

        let mut options = ConvertOptions::from(&self.config);
        options.image_quality = quality.clamp(1, 100);
        options.bitrate_kbps = bitrate;
        options.video_scale = if scale.trim().is_empty() {
            None
        } else {
            Some(scale)
        };

        let mut taken: HashSet<PathBuf> = self
            .jobs
            .iter()
            .filter(|job| matches!(job.status, JobStatus::Queued | JobStatus::Running))
            .filter_map(|job| job.output.clone())
            .collect();

        let mut submitted = 0usize;
        for file in files {
            let out_dir = view_output
                .clone()
                .or_else(|| self.config.output_dir.clone())
                .or_else(|| file.path.parent().map(|p| p.to_path_buf()))
                .unwrap_or_else(|| PathBuf::from("."));
            let stem = file
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("cikti")
                .to_string();
            // Extraction always gets a fresh directory; files only overwrite
            // when the user explicitly enabled it in Settings.
            let overwrite = self.config.allow_overwrite && target != registry::EXTRACT;
            let output = if overwrite {
                let ext = if target == registry::EXTRACT {
                    ""
                } else {
                    &target
                };
                paths::explicit_output_path(&out_dir, &stem, ext, &mut taken)
            } else if target == registry::EXTRACT {
                paths::unique_output_path_reserved(&out_dir, &stem, "", &mut taken)
            } else {
                paths::unique_output_path_reserved(&out_dir, &stem, &target, &mut taken)
            };
            let name = file
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| file.path.display().to_string());
            let id = runner.submit(
                file.path.clone(),
                output.clone(),
                file.format,
                target.clone(),
                options.clone(),
                overwrite,
            );
            self.jobs.push(JobRow {
                id,
                name,
                target: target.clone(),
                output: Some(output),
                status: JobStatus::Queued,
                progress: 0,
                message: None,
            });
            submitted += 1;
        }

        if submitted > 0 {
            self.view.files.clear();
            self.notify(format!("{submitted} iş kuyruğa eklendi."), false);
        }
    }

    fn show_convert(&mut self, ui: &mut egui::Ui) {
        let tools = self.tools.clone();
        let start = self.view.show(ui, &tools);
        if start {
            self.enqueue();
        }
        self.show_jobs(ui);
    }

    fn show_jobs(&mut self, ui: &mut egui::Ui) {
        if self.jobs.is_empty() {
            return;
        }
        ui.add_space(10.0);
        egui::Frame::default()
            .fill(ui.visuals().faint_bg_color)
            .stroke(egui::Stroke::new(
                1.0,
                ui.visuals().widgets.noninteractive.bg_stroke.color,
            ))
            .corner_radius(12)
            .inner_margin(egui::Margin::same(14))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("İşler").size(16.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Bitenleri temizle").clicked() {
                            self.jobs.retain(|job| {
                                matches!(job.status, JobStatus::Queued | JobStatus::Running)
                            });
                        }
                    });
                });
                ui.add_space(6.0);

                egui::ScrollArea::vertical()
                    .id_salt("isler")
                    .max_height(200.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for job in &self.jobs {
                            let (label, color) = match job.status {
                                JobStatus::Queued => ("Sırada", MUTED),
                                JobStatus::Running => ("Çalışıyor", ACCENT),
                                JobStatus::Done => ("Tamam", SUCCESS),
                                JobStatus::Failed => ("Hata", DANGER),
                            };
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(label).small().color(color).strong());
                                ui.label(RichText::new(&job.name).strong());
                                ui.label(
                                    RichText::new(format!("→ .{}", job.target))
                                        .small()
                                        .color(MUTED),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::ProgressBar::new(job.progress as f32 / 100.0)
                                                .desired_width(160.0)
                                                .corner_radius(6)
                                                .fill(color)
                                                .show_percentage(),
                                        );
                                    },
                                );
                            });
                            if let Some(message) = &job.message {
                                let text = truncate(message, 300);
                                ui.label(RichText::new(text).small().color(DANGER));
                            }
                        }
                    });
            });
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        let tools = self.tools.clone();
        let action = self.settings.show(ui, &tools, self.manager);
        match action {
            SettingsAction::None => {}
            SettingsAction::Save(new_config) => {
                self.config = new_config;
                self.config.normalise();
                match self.config.save() {
                    Ok(()) => self.notify("Ayarlar kaydedildi.", false),
                    Err(error) => self.notify(format!("Ayarlar kaydedilemedi: {error:#}"), true),
                }
                self.settings.sync(&self.config);
                if !self.any_job_active() {
                    match JobRunner::new(&self.config) {
                        Ok(runner) => self.runner = Some(runner),
                        Err(error) => {
                            self.notify(format!("İş kuyruğu yenilenemedi: {error:#}"), true)
                        }
                    }
                } else {
                    self.notify(
                        "Paralel iş ayarı, çalışan işler bitince uygulanacak.",
                        false,
                    );
                }
            }
            SettingsAction::RerunWizard => {
                self.config.setup_done = false;
                let _ = self.config.save();
                self.refresh_tools();
                self.wizard.reopen(&self.tools);
            }
        }
    }

    fn handle_wizard_action(&mut self, action: WizardAction) {
        match action {
            WizardAction::StartInstall(tools) => {
                if tools.is_empty() {
                    self.notify("Kurulacak araç seçilmedi.", true);
                    return;
                }
                match self.manager {
                    Some(manager) => self.wizard.start_install(manager, tools),
                    None => self.notify("Paket yöneticisi bulunamadı; araçları elle kurun.", true),
                }
            }
            WizardAction::Skip => {
                let missing: Vec<String> = detector::missing_tools(&self.tools)
                    .into_iter()
                    .map(|tool| tool.id().to_string())
                    .collect();
                self.config.setup_done = true;
                self.config.skipped_tools = missing;
                if let Err(error) = self.config.save() {
                    self.notify(format!("Ayar kaydedilemedi: {error:#}"), true);
                }
                self.wizard.open = false;
                self.notify(
                    "Kurulum atlandı. Eksik araçlara bağlı dönüşümler kilitli kalacak.",
                    false,
                );
            }
            WizardAction::Installed { success, summary } => {
                self.refresh_tools();
                let missing = detector::missing_tools(&self.tools);
                if success && !missing.iter().any(|tool| tool.is_critical()) {
                    self.config.setup_done = true;
                    self.config.skipped_tools =
                        missing.iter().map(|tool| tool.id().to_string()).collect();
                    let _ = self.config.save();
                    self.wizard.open = false;
                    self.notify("Kurulum tamamlandı.", false);
                } else if success {
                    self.wizard.selected = missing;
                    self.notify(
                        "Kurulum tamamlandı ancak FFmpeg hâlâ eksik. Ses/video dönüşümleri kilitli.",
                        true,
                    );
                } else {
                    self.wizard.selected = missing;
                    self.notify(format!("Kurulum başarısız: {summary}"), true);
                }
            }
        }
    }

    fn draw_header(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("ust-cubuk")
            .frame(
                egui::Frame::default()
                    .fill(root.visuals().faint_bg_color)
                    .inner_margin(egui::Margin::symmetric(18, 12)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("◆").size(22.0).color(ACCENT));
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Dönüştür").size(20.0).strong());
                        ui.label(
                            RichText::new("Tek ekranda toplu dosya dönüşümü")
                                .small()
                                .color(MUTED),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let settings_label = if self.screen == Screen::Settings {
                            "← Dönüştür"
                        } else {
                            "⚙ Ayarlar"
                        };
                        if ui.button(settings_label).clicked() {
                            self.screen = if self.screen == Screen::Settings {
                                Screen::Convert
                            } else {
                                Screen::Settings
                            };
                        }
                        if ui
                            .button("Araçları yenile")
                            .on_hover_text(
                                "Harici araçların kurulu olup olmadığını yeniden denetle",
                            )
                            .clicked()
                        {
                            self.refresh_tools();
                        }
                        if self.runner.is_none() {
                            ui.colored_label(DANGER, "İş kuyruğu kapalı");
                        }
                    });
                });
            });
    }

    fn draw_toast(&mut self, root: &mut egui::Ui) {
        let Some(toast) = &self.toast else {
            return;
        };
        if Instant::now() > toast.until {
            self.toast = None;
            return;
        }
        let color = if toast.error { DANGER } else { SUCCESS };
        let text = toast.text.clone();
        egui::Panel::bottom("bildirim")
            .frame(
                egui::Frame::default()
                    .fill(root.visuals().faint_bg_color)
                    .inner_margin(egui::Margin::symmetric(18, 8)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(color, "●");
                    ui.colored_label(color, text);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Kapat").clicked() {
                            self.toast = None;
                        }
                    });
                });
            });
    }
}

impl eframe::App for DonusturApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        self.poll_jobs();
        self.handle_drops(&ctx);

        if self.any_job_active() || self.wizard.is_installing() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.toast.is_some() {
            ctx.request_repaint_after(Duration::from_millis(250));
        }

        self.draw_header(ui);
        self.draw_toast(ui);

        egui::CentralPanel::default().show(ui, |ui| {
            let hovering = ctx.input(|input| !input.raw.hovered_files.is_empty());
            if hovering {
                ui.add_space(2.0);
                ui.colored_label(ACCENT, "Dosyaları bırakmak için fareyi serbest bırakın.");
            }
            match self.screen {
                Screen::Settings => self.show_settings(ui),
                Screen::Convert => self.show_convert(ui),
            }
        });

        let action = self.wizard.show(&ctx, &self.tools, self.manager);
        if let Some(action) = action {
            self.handle_wizard_action(action);
        }

        // Keep the settings draft in sync when the wizard changes tool state.
        if self.wizard.open {
            self.settings.draft.skipped_tools = self.config.skipped_tools.clone();
        }
    }
}

/// Applies a soft, modern look once at startup.
fn configure_style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 7.0);
        style.spacing.button_padding = egui::vec2(11.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        for widget in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
        ] {
            widget.corner_radius = egui::CornerRadius::same(7);
        }
        style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(7);
        style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.55);
        style.visuals.hyperlink_color = ACCENT;
    });
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::registry::MediaKind;
    use crate::ui::convert_view::{kind_color, FileItem};

    #[test]
    fn truncate_is_unicode_safe() {
        assert_eq!(truncate("kısa", 10), "kısa");
        assert_eq!(truncate("ğüşiöçğüşiöç", 4), "ğüşi…");
    }

    #[test]
    fn every_media_kind_has_a_badge_colour() {
        for kind in [
            MediaKind::Image,
            MediaKind::Audio,
            MediaKind::Video,
            MediaKind::Document,
            MediaKind::Archive,
        ] {
            // Just ensure the mapping is exhaustive and visible.
            assert_ne!(kind_color(kind), egui::Color32::TRANSPARENT);
        }
    }

    #[test]
    fn file_item_keeps_metadata() {
        let format = registry::format_by_id("png").unwrap();
        let item = FileItem {
            path: PathBuf::from("/tmp/a.png"),
            format,
            size: 12,
        };
        assert_eq!(item.format.id, "png");
        assert_eq!(crate::ui::convert_view::human_size(item.size), "12 B");
    }
}
