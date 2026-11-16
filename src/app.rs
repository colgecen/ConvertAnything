//! Application state and the main update loop.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Margin, RichText, Stroke};

use crate::converters::ConvertOptions;
use crate::core::detector::{self, PackageManager, ToolStatus};
use crate::core::job::{self, BatchItem, JobRunner, JobStatus};
use crate::i18n::t;
use crate::install;
use crate::integration;
use crate::state::config::Config;
use crate::ui::convert_view::ConvertView;
use crate::ui::settings::{SettingsAction, SettingsView};
use crate::ui::setup_wizard::{SetupWizard, WizardAction};
use crate::ui::theme;
use crate::ui::Screen;

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
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        mut config: Config,
        initial_paths: Vec<PathBuf>,
    ) -> Self {
        theme::install(&cc.egui_ctx);
        let tools = detector::detect_all();
        let manager = detector::detect_package_manager();

        // If every tool is present there is nothing to ask the user.
        if !config.setup_done && detector::missing_tools(&tools).is_empty() {
            config.setup_done = true;
            let _ = config.save();
        }

        // Running a downloaded AppImage is enough: copy it to the stable
        // location, register menus and enable login autostart automatically.
        let running_appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
        let installed = install::installed_executable();
        let mut startup_notice: Option<(String, bool)> = None;
        if install::should_install(running_appimage.as_deref(), installed.as_deref()) {
            let enable_autostart = installed.is_none() || install::autostart_enabled();
            match install::install_app(enable_autostart) {
                Ok(report) => {
                    startup_notice = Some((
                        format!(
                            "{} {} · {}",
                            t("Installed for this user:", "Bu kullanıcı için kuruldu:"),
                            report.executable.display(),
                            t(
                                "it will start with your session",
                                "oturum açılışında başlayacak"
                            )
                        ),
                        false,
                    ));
                }
                Err(error) => {
                    tracing::warn!("otomatik kurulum başarısız: {error:#}");
                    startup_notice = Some((
                        format!(
                            "{} {error:#}",
                            t(
                                "Automatic installation failed:",
                                "Otomatik kurulum başarısız:"
                            )
                        ),
                        true,
                    ));
                }
            }
        }

        // Register/refresh the file-manager integration (idempotent, user data
        // dir only). Prefer the installed copy when it exists.
        let menu_exec =
            install::installed_executable().unwrap_or_else(integration::executable_path);
        if let Err(error) = integration::ensure_installed_for(&menu_exec) {
            tracing::warn!("dosya yöneticisi menüsü kurulamadı: {error:#}");
        }

        let runner = match JobRunner::new(&config) {
            Ok(runner) => Some(runner),
            Err(error) => {
                tracing::error!("iş kuyruğu başlatılamadı: {error:#}");
                None
            }
        };

        let mut view = ConvertView::new(
            config.image_quality,
            config.bitrate_kbps,
            config.video_scale.clone(),
        );
        for path in initial_paths {
            view.add_path(path);
        }

        let wizard = SetupWizard::new(&config, &tools);
        let settings = SettingsView::new(&config);

        let mut app = Self {
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
        };
        if let Some((text, error)) = startup_notice {
            app.notify(text, error);
        }
        app
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
        self.add_paths(dropped);
    }

    fn add_paths(&mut self, paths: Vec<PathBuf>) {
        let mut added = 0usize;
        let mut rejected = 0usize;
        for path in paths {
            if self.view.add_path(path) {
                added += 1;
            } else {
                rejected += 1;
            }
        }
        if rejected > 0 {
            self.notify(
                format!(
                    "{} {}, {} {}",
                    added,
                    t("file(s) added", "dosya eklendi"),
                    rejected,
                    t(
                        "file(s) rejected (unsupported type or content)",
                        "dosya reddedildi (tür veya içerik uyuşmazlığı)"
                    )
                ),
                rejected > added,
            );
        } else if added > 0 {
            self.notify(
                format!("{} {}", added, t("file(s) added.", "dosya eklendi.")),
                false,
            );
        }
    }

    fn enqueue(&mut self) {
        let (files, target, view_output, quality, bitrate, scale) = (
            self.view.files.clone(),
            self.view.target.clone(),
            self.view.output_dir.clone(),
            self.view.image_quality,
            self.view.bitrate_kbps,
            self.view.video_scale.clone(),
        );
        let Some(target) = target else {
            self.notify(
                t(
                    "Choose a target format first.",
                    "Önce bir hedef biçim seçin.",
                ),
                true,
            );
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

        let submitted = {
            let Some(runner) = self.runner.as_ref() else {
                self.notify(
                    t(
                        "The conversion queue could not start.",
                        "İş kuyruğu başlatılamadı.",
                    ),
                    true,
                );
                return;
            };
            let items: Vec<BatchItem> = files
                .iter()
                .map(|file| BatchItem {
                    path: file.path.clone(),
                    source: file.format,
                })
                .collect();
            job::submit_batch(
                runner,
                &items,
                &target,
                view_output.as_deref(),
                &self.config,
                &options,
                &mut taken,
            )
        };

        let count = submitted.len();
        for job in submitted {
            self.jobs.push(JobRow {
                id: job.id,
                name: job.name,
                target: job.target,
                output: Some(job.output),
                status: JobStatus::Queued,
                progress: 0,
                message: None,
            });
        }
        if count > 0 {
            self.view.files.clear();
            self.notify(
                format!("{} {}", count, t("job(s) queued.", "iş kuyruğa eklendi.")),
                false,
            );
        }
    }

    fn show_convert(&mut self, ui: &mut egui::Ui) {
        let tools = self.tools.clone();
        self.view.show(ui, &tools);
        self.show_jobs(ui);
    }

    fn show_jobs(&mut self, ui: &mut egui::Ui) {
        if self.jobs.is_empty() {
            return;
        }
        ui.add_space(14.0);
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                theme::section(ui, t("Jobs", "İşler"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .small_button(t("Clear finished", "Bitenleri temizle"))
                        .clicked()
                    {
                        self.jobs.retain(|job| {
                            matches!(job.status, JobStatus::Queued | JobStatus::Running)
                        });
                    }
                });
            });
            ui.add_space(8.0);

            for job in &self.jobs {
                let (label, color) = match job.status {
                    JobStatus::Queued => (t("Queued", "Sırada"), theme::MUTED),
                    JobStatus::Running => (t("Running", "Çalışıyor"), theme::ACCENT),
                    JobStatus::Done => (t("Done", "Tamam"), theme::SUCCESS),
                    JobStatus::Failed => (t("Failed", "Hata"), theme::DANGER),
                };
                egui::Frame::default()
                    .fill(theme::CARD_ALT)
                    .corner_radius(10)
                    .inner_margin(Margin::symmetric(12, 9))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            theme::dot(ui, color);
                            ui.label(RichText::new(label).small().strong().color(color));
                            ui.add(egui::Label::new(RichText::new(&job.name).strong()).truncate());
                            ui.label(
                                RichText::new(format!(".{}", job.target))
                                    .small()
                                    .color(theme::MUTED),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add(
                                        egui::ProgressBar::new(job.progress as f32 / 100.0)
                                            .desired_width(150.0)
                                            .corner_radius(6)
                                            .fill(color)
                                            .show_percentage(),
                                    );
                                },
                            );
                        });
                        if let Some(output) = &job.output {
                            if job.status == JobStatus::Done {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(output.display().to_string())
                                            .small()
                                            .color(theme::MUTED),
                                    )
                                    .truncate(),
                                );
                            }
                        }
                        if let Some(message) = &job.message {
                            ui.label(
                                RichText::new(truncate(message, 300))
                                    .small()
                                    .color(theme::DANGER),
                            );
                        }
                    });
                ui.add_space(6.0);
            }
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
                    Ok(()) => self.notify(t("Settings saved.", "Ayarlar kaydedildi."), false),
                    Err(error) => self.notify(
                        format!(
                            "{} {error:#}",
                            t("Settings could not be saved:", "Ayarlar kaydedilemedi:")
                        ),
                        true,
                    ),
                }
                self.settings.sync(&self.config);
                if !self.any_job_active() {
                    match JobRunner::new(&self.config) {
                        Ok(runner) => self.runner = Some(runner),
                        Err(error) => self.notify(
                            format!(
                                "{} {error:#}",
                                t(
                                    "The conversion queue could not be refreshed:",
                                    "İş kuyruğu yenilenemedi:"
                                )
                            ),
                            true,
                        ),
                    }
                } else {
                    self.notify(
                        t(
                            "The parallel job setting will apply after running jobs finish.",
                            "Paralel iş ayarı, çalışan işler bitince uygulanacak.",
                        ),
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
            SettingsAction::InstallIntegration => match integration::ensure_installed() {
                Ok(installed) => self.notify(
                    format!(
                        "{} {} {}",
                        t("Context menu installed:", "Sağ tık menüsü kuruldu:"),
                        installed.service_menus.len(),
                        t("menus", "menü")
                    ),
                    false,
                ),
                Err(error) => self.notify(
                    format!(
                        "{} {error:#}",
                        t(
                            "Context menu could not be installed:",
                            "Sağ tık menüsü kurulamadı:"
                        )
                    ),
                    true,
                ),
            },
            SettingsAction::InstallApp => match install::install_app(true) {
                Ok(report) => {
                    self.settings.autostart = true;
                    self.notify(
                        format!(
                            "{} {} ({} {})",
                            t("App installed:", "Uygulama kuruldu:"),
                            report.executable.display(),
                            report.service_menus,
                            t("menus", "menü")
                        ),
                        false,
                    );
                }
                Err(error) => self.notify(
                    format!("{} {error:#}", t("Install failed:", "Kurulum başarısız:")),
                    true,
                ),
            },
            SettingsAction::SetAutostart(enabled) => match install::set_autostart(enabled) {
                Ok(path) => {
                    self.settings.autostart = enabled;
                    self.notify(
                        if enabled {
                            format!(
                                "{} {}",
                                t(
                                    "Login autostart enabled:",
                                    "Oturum açılışında başlatma açıldı:"
                                ),
                                path.display()
                            )
                        } else {
                            t(
                                "Login autostart disabled.",
                                "Oturum açılışında başlatma kapatıldı.",
                            )
                            .to_string()
                        },
                        false,
                    );
                }
                Err(error) => self.notify(
                    format!(
                        "{} {error:#}",
                        t(
                            "Autostart could not be changed:",
                            "Otomatik başlatma değiştirilemedi:"
                        )
                    ),
                    true,
                ),
            },
        }
    }

    fn handle_wizard_action(&mut self, action: WizardAction) {
        match action {
            WizardAction::StartInstall(tools) => {
                if tools.is_empty() {
                    self.notify(
                        t("No tool selected to install.", "Kurulacak araç seçilmedi."),
                        true,
                    );
                    return;
                }
                match self.manager {
                    Some(manager) => self.wizard.start_install(manager, tools),
                    None => self.notify(
                        t(
                            "No package manager found; install the tools manually.",
                            "Paket yöneticisi bulunamadı; araçları elle kurun.",
                        ),
                        true,
                    ),
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
                    self.notify(
                        format!(
                            "{} {error:#}",
                            t("Settings could not be saved:", "Ayar kaydedilemedi:")
                        ),
                        true,
                    );
                }
                self.wizard.open = false;
                self.notify(
                    t(
                        "Setup skipped. Conversions that need missing tools stay locked.",
                        "Kurulum atlandı. Eksik araçlara bağlı dönüşümler kilitli kalacak.",
                    ),
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
                    self.notify(t("Setup complete.", "Kurulum tamamlandı."), false);
                } else if success {
                    self.wizard.selected = missing;
                    self.notify(
                        t(
                            "Setup finished but FFmpeg is still missing. Audio/video conversions stay locked.",
                            "Kurulum tamamlandı ancak FFmpeg hâlâ eksik. Ses/video dönüşümleri kilitli.",
                        ),
                        true,
                    );
                } else {
                    self.wizard.selected = missing;
                    self.notify(
                        format!("{} {summary}", t("Setup failed:", "Kurulum başarısız:")),
                        true,
                    );
                }
            }
        }
    }

    fn draw_header(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("ust-cubuk")
            .frame(
                egui::Frame::default()
                    .fill(theme::BG_SOFT)
                    .inner_margin(Margin::symmetric(20, 12)),
            )
            .show(root, |ui| {
                theme::vertical_gradient(
                    ui,
                    ui.max_rect(),
                    egui::Color32::from_rgb(20, 25, 35),
                    theme::BG_SOFT,
                );
                ui.horizontal(|ui| {
                    let title = ui.add(
                        egui::Button::new(
                            RichText::new(t("Convert", "Dönüştür"))
                                .size(22.0)
                                .strong()
                                .color(theme::TEXT),
                        )
                        .frame(false),
                    );
                    if title
                        .on_hover_text(t("Go to the conversion screen", "Dönüştürme ekranına dön"))
                        .clicked()
                    {
                        self.screen = Screen::Convert;
                    }
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(t(
                            "One screen, batch conversion",
                            "Tek ekranda toplu dosya dönüşümü",
                        ))
                        .small()
                        .color(theme::MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let settings_label = if self.screen == Screen::Settings {
                            t("Convert", "Dönüştür")
                        } else {
                            t("Settings", "Ayarlar")
                        };
                        if ui.button(settings_label).clicked() {
                            self.screen = if self.screen == Screen::Settings {
                                Screen::Convert
                            } else {
                                Screen::Settings
                            };
                        }
                        if ui
                            .button(t("Refresh tools", "Araçları yenile"))
                            .on_hover_text(t(
                                "Check again whether the external tools are installed",
                                "Harici araçların kurulu olup olmadığını yeniden denetle",
                            ))
                            .clicked()
                        {
                            self.refresh_tools();
                        }
                        if self.runner.is_none() {
                            ui.colored_label(
                                theme::DANGER,
                                t("Job queue is off", "İş kuyruğu kapalı"),
                            );
                        }
                    });
                });
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [
                        egui::pos2(rect.left(), rect.bottom()),
                        egui::pos2(rect.right(), rect.bottom()),
                    ],
                    Stroke::new(1.0, theme::BORDER),
                );
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
        let color = if toast.error {
            theme::DANGER
        } else {
            theme::SUCCESS
        };
        let text = toast.text.clone();
        egui::Panel::bottom("bildirim")
            .frame(
                egui::Frame::default()
                    .fill(theme::BG_SOFT)
                    .inner_margin(Margin::symmetric(20, 9)),
            )
            .show(root, |ui| {
                ui.horizontal(|ui| {
                    theme::dot(ui, color);
                    ui.colored_label(color, text);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(t("Close", "Kapat")).clicked() {
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

        if self.screen == Screen::Convert && !self.view.files.is_empty() {
            egui::Panel::bottom("eylem-cubugu")
                .frame(
                    egui::Frame::default()
                        .fill(theme::BG)
                        .inner_margin(Margin::symmetric(18, 10)),
                )
                .show(ui, |ui| {
                    let tools = self.tools.clone();
                    if self.view.show_actions_bar(ui, &tools) {
                        self.enqueue();
                    }
                });
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .inner_margin(Margin::symmetric(18, 12))
                    .fill(theme::BG),
            )
            .show(ui, |ui| {
                let hovering = ctx.input(|input| !input.raw.hovered_files.is_empty());
                if hovering {
                    ui.add_space(2.0);
                    ui.colored_label(
                        theme::ACCENT,
                        t(
                            "Release the mouse to drop the files.",
                            "Dosyaları bırakmak için fareyi serbest bırakın.",
                        ),
                    );
                }
                match self.screen {
                    Screen::Settings => self.show_settings(ui),
                    Screen::Convert => {
                        // Everything scrolls together so the action bar stays
                        // reachable even in a small window.
                        egui::ScrollArea::vertical()
                            .id_salt("donusum-ekrani")
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                self.show_convert(ui);
                            });
                    }
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

    #[test]
    fn truncate_is_unicode_safe() {
        assert_eq!(truncate("kısa", 10), "kısa");
        assert_eq!(truncate("ğüşiöçğüşiöç", 4), "ğüşi…");
    }

    #[test]
    fn file_size_helper_is_available() {
        assert_eq!(crate::ui::convert_view::human_size(1536), "1.5 KiB");
    }
}
