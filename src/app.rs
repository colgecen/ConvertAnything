//! Application state and the main update loop.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use egui::{Color32, RichText};

use crate::converters::ConvertOptions;
use crate::core::detector::{self, PackageManager, Tool, ToolStatus};
use crate::core::job::{JobRunner, JobStatus};
use crate::core::registry;
use crate::state::config::Config;
use crate::ui::convert_view::{tool_available, ConvertView};
use crate::ui::settings::{SettingsAction, SettingsView};
use crate::ui::setup_wizard::{SetupWizard, WizardAction};
use crate::ui::Tab;
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
    tab: Tab,
    views: Vec<ConvertView>,
    wizard: SetupWizard,
    settings: SettingsView,
    toast: Option<Toast>,
}

impl DonusturApp {
    pub fn new(cc: &eframe::CreationContext<'_>, mut config: Config) -> Self {
        let _ = cc;
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

        let views = Tab::CONVERT_TABS
            .iter()
            .filter_map(|tab| tab.kind())
            .map(|kind| {
                ConvertView::new(
                    kind,
                    config.image_quality,
                    config.bitrate_kbps,
                    config.video_scale.clone(),
                )
            })
            .collect();

        let wizard = SetupWizard::new(&config, &tools);
        let settings = SettingsView::new(&config);

        Self {
            config,
            tools,
            manager,
            runner,
            jobs: Vec::new(),
            tab: Tab::Image,
            views,
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

    /// Index of the conversion view backing `tab`.
    ///
    /// Views are stored in `Tab::CONVERT_TABS` order, but the lookup is by
    /// media kind so reordering the tabs can never silently mismatch them.
    fn view_index(&self, tab: Tab) -> Option<usize> {
        let kind = tab.kind()?;
        self.views.iter().position(|view| view.kind == kind)
    }

    fn tab_enabled(&self, tab: Tab) -> bool {
        match tab {
            Tab::Audio | Tab::Video => tool_available(&self.tools, Tool::Ffmpeg),
            Tab::Document => {
                tool_available(&self.tools, Tool::LibreOffice)
                    || tool_available(&self.tools, Tool::Pandoc)
            }
            Tab::Image | Tab::Archive | Tab::Settings => true,
        }
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
        let mut last_tab = None;
        for path in dropped {
            let Some(format) = registry::detect_format(&path) else {
                rejected += 1;
                continue;
            };
            let tab = Tab::for_kind(format.kind);
            if !self.tab_enabled(tab) {
                rejected += 1;
                continue;
            }
            let Some(index) = self.view_index(tab) else {
                rejected += 1;
                continue;
            };
            if self.views[index].add_path(path) {
                added += 1;
                last_tab = Some(tab);
            } else {
                rejected += 1;
            }
        }

        if let Some(tab) = last_tab {
            self.tab = tab;
        }
        if rejected > 0 {
            self.notify(
                format!(
                    "{added} dosya eklendi, {rejected} dosya reddedildi (tür/araç uyuşmazlığı)."
                ),
                rejected > added,
            );
        } else if added > 0 {
            self.notify(format!("{added} dosya eklendi."), false);
        }
    }

    fn enqueue(&mut self, index: usize) {
        let Some(runner) = self.runner.as_ref() else {
            self.notify("İş kuyruğu başlatılamadı.", true);
            return;
        };
        let (files, target, view_output, quality, bitrate, scale) = {
            let view = &self.views[index];
            (
                view.files.clone(),
                view.target.clone(),
                view.output_dir.clone(),
                view.image_quality,
                view.bitrate_kbps,
                view.video_scale.clone(),
            )
        };
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
            if let Some(index) = self.view_index(self.tab) {
                self.views[index].files.clear();
            }
            self.notify(format!("{submitted} iş kuyruğa eklendi."), false);
        }
    }

    fn show_convert(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.view_index(self.tab) else {
            return;
        };
        let tools = self.tools.clone();
        let start = self.views[index].show(ui, &tools);
        if start {
            self.enqueue(index);
        }
        self.show_jobs(ui);
    }

    fn show_jobs(&mut self, ui: &mut egui::Ui) {
        if self.jobs.is_empty() {
            return;
        }
        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(RichText::new("İşler").strong());
            if ui.small_button("Bitenleri temizle").clicked() {
                self.jobs
                    .retain(|job| matches!(job.status, JobStatus::Queued | JobStatus::Running));
            }
        });

        egui::ScrollArea::vertical()
            .id_salt("isler")
            .max_height(190.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for job in &self.jobs {
                    ui.horizontal(|ui| {
                        let (label, color) = match job.status {
                            JobStatus::Queued => ("Sırada", Color32::GRAY),
                            JobStatus::Running => ("Çalışıyor", Color32::from_rgb(90, 150, 210)),
                            JobStatus::Done => ("Tamam", Color32::from_rgb(80, 160, 90)),
                            JobStatus::Failed => ("Hata", Color32::from_rgb(200, 90, 60)),
                        };
                        ui.label(RichText::new(label).small().color(color).strong());
                        ui.label(RichText::new(&job.name).strong());
                        ui.label(
                            RichText::new(format!("→ .{}", job.target))
                                .small()
                                .color(Color32::GRAY),
                        );
                        ui.add(
                            egui::ProgressBar::new(job.progress as f32 / 100.0)
                                .desired_width(150.0)
                                .show_percentage(),
                        );
                    });
                    if let Some(message) = &job.message {
                        let text = truncate(message, 300);
                        ui.label(
                            RichText::new(text)
                                .small()
                                .color(Color32::from_rgb(200, 90, 60)),
                        );
                    }
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
                    "Kurulum atlandı. Eksik araçlara bağlı sekmeler kilitli kalacak.",
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
                        "Kurulum tamamlandı ancak FFmpeg hâlâ eksik. Ses/video sekmeleri kilitli.",
                        true,
                    );
                } else {
                    self.wizard.selected = missing;
                    self.notify(format!("Kurulum başarısız: {summary}"), true);
                }
            }
        }
    }

    fn draw_tabs(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("ust-cubuk").show(root, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("Dönüştür");
                ui.separator();
                for tab in Tab::CONVERT_TABS {
                    let enabled = self.tab_enabled(tab);
                    let response = ui.add_enabled_ui(enabled, |ui| {
                        ui.selectable_label(self.tab == tab, tab.label())
                    });
                    if response.inner.clicked() {
                        self.tab = tab;
                    }
                    if !enabled {
                        response.inner.on_hover_text("Gerekli araç kurulu değil");
                    }
                }
                ui.separator();
                if ui
                    .selectable_label(self.tab == Tab::Settings, "Ayarlar")
                    .clicked()
                {
                    self.tab = Tab::Settings;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.runner.is_none() {
                        ui.colored_label(Color32::from_rgb(200, 90, 60), "İş kuyruğu kapalı");
                    }
                    if ui
                        .button("Araçları yenile")
                        .on_hover_text("Harici araçların kurulu olup olmadığını yeniden denetle")
                        .clicked()
                    {
                        self.refresh_tools();
                    }
                });
            });
            ui.add_space(4.0);
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
            Color32::from_rgb(200, 90, 60)
        } else {
            Color32::from_rgb(80, 160, 90)
        };
        let text = toast.text.clone();
        egui::Panel::bottom("bildirim").show(root, |ui| {
            ui.horizontal(|ui| {
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

        self.draw_tabs(ui);
        self.draw_toast(ui);

        egui::CentralPanel::default().show(ui, |ui| {
            let hovering = ctx.input(|input| !input.raw.hovered_files.is_empty());
            if hovering {
                ui.add_space(2.0);
                ui.colored_label(
                    Color32::from_rgb(90, 150, 210),
                    "Dosyaları bırakmak için fareyi serbest bırakın.",
                );
            }
            match self.tab {
                Tab::Settings => self.show_settings(ui),
                _ => self.show_convert(ui),
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

/// Stable ordering helper used by tests.
#[cfg(test)]
pub fn category_tabs() -> Vec<(crate::core::registry::MediaKind, Tab)> {
    Tab::CONVERT_TABS
        .iter()
        .filter_map(|tab| tab.kind().map(|kind| (kind, *tab)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::registry::MediaKind;

    #[test]
    fn every_media_kind_has_a_tab() {
        let tabs = category_tabs();
        for kind in [
            MediaKind::Image,
            MediaKind::Audio,
            MediaKind::Video,
            MediaKind::Document,
            MediaKind::Archive,
        ] {
            assert!(tabs.iter().any(|(candidate, _)| *candidate == kind));
            assert_eq!(Tab::for_kind(kind).kind(), Some(kind));
        }
    }

    #[test]
    fn truncate_is_unicode_safe() {
        assert_eq!(truncate("kısa", 10), "kısa");
        assert_eq!(truncate("ğüşiöçğüşiöç", 4), "ğüşi…");
    }
}
