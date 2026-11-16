//! Small chooser window used by the file-manager context menu.
//!
//! `donustur --convert FILE [FILE…]` opens this screen, lists every format the
//! selected files can become and converts immediately after a click (or the
//! arrow keys + Enter).

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use egui::{Key, Margin, RichText};

use crate::converters::ConvertOptions;
use crate::core::detector;
use crate::core::job::{self, BatchItem, JobRunner, JobStatus};
use crate::core::registry;
use crate::i18n::{self, t};
use crate::state::config::Config;
use crate::ui::convert_view::{common_targets, target_label, FileItem};
use crate::ui::theme;

struct QuickJob {
    id: u64,
    name: String,
    output: PathBuf,
    status: JobStatus,
    progress: u8,
    message: Option<String>,
}

pub struct QuickConvert {
    files: Vec<FileItem>,
    targets: Vec<&'static registry::Format>,
    selected: usize,
    config: Config,
    runner: Option<JobRunner>,
    jobs: Vec<QuickJob>,
    started: bool,
    error: Option<String>,
}

impl QuickConvert {
    pub fn new(paths: Vec<PathBuf>, config: Config, target: Option<String>) -> Self {
        let tools = detector::detect_all();
        let mut files = Vec::new();
        let mut rejected = 0usize;
        for path in paths {
            let Some(format) = registry::detect_format(&path) else {
                rejected += 1;
                continue;
            };
            if registry::has_forbidden_extension(&path) || registry::looks_executable(&path) {
                rejected += 1;
                continue;
            }
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            files.push(FileItem { path, format, size });
        }
        let targets = common_targets(&files, &tools);
        let selected = target
            .as_deref()
            .and_then(|id| targets.iter().position(|format| format.id == id))
            .unwrap_or(0);
        let runner = match JobRunner::new(&config) {
            Ok(runner) => Some(runner),
            Err(error) => {
                tracing::error!("hızlı dönüşüm kuyruğu başlatılamadı: {error:#}");
                None
            }
        };
        let error = if rejected > 0 {
            Some(format!(
                "{} {}",
                rejected,
                t(
                    "file(s) were skipped because their type is unsupported.",
                    "dosya desteklenmeyen türde olduğu için atlandı."
                )
            ))
        } else {
            None
        };
        let mut app = Self {
            files,
            targets,
            selected,
            config,
            runner,
            jobs: Vec::new(),
            started: false,
            error,
        };
        if let Some(target) = target {
            if app.targets.iter().any(|format| format.id == target) {
                app.start();
            }
        }
        app
    }

    fn start(&mut self) {
        if self.started {
            return;
        }
        let Some(runner) = self.runner.as_ref() else {
            self.error = Some(
                t(
                    "The conversion queue could not start.",
                    "Dönüşüm kuyruğu başlatılamadı.",
                )
                .to_string(),
            );
            return;
        };
        let Some(target) = self.targets.get(self.selected) else {
            self.error = Some(
                t(
                    "Choose a target format first.",
                    "Önce bir hedef biçim seçin.",
                )
                .to_string(),
            );
            return;
        };
        let items: Vec<BatchItem> = self
            .files
            .iter()
            .map(|file| BatchItem {
                path: file.path.clone(),
                source: file.format,
            })
            .collect();
        let options = ConvertOptions::from(&self.config);
        let mut taken = HashSet::new();
        let submitted = job::submit_batch(
            runner,
            &items,
            target.id,
            None,
            &self.config,
            &options,
            &mut taken,
        );
        self.jobs = submitted
            .into_iter()
            .map(|job| QuickJob {
                id: job.id,
                name: job.name,
                output: job.output,
                status: JobStatus::Queued,
                progress: 0,
                message: None,
            })
            .collect();
        self.started = true;
    }

    fn poll(&mut self) -> bool {
        let Some(runner) = &self.runner else {
            return false;
        };
        let mut changes = false;
        while let Some(update) = runner.try_recv() {
            if let Some(job) = self.jobs.iter_mut().find(|job| job.id == update.id) {
                job.status = update.status;
                job.progress = if update.status == JobStatus::Done {
                    100
                } else {
                    update.progress
                };
                job.message = update.message;
                changes = true;
            }
        }
        changes
    }

    fn all_finished(&self) -> bool {
        !self.jobs.is_empty()
            && self
                .jobs
                .iter()
                .all(|job| matches!(job.status, JobStatus::Done | JobStatus::Failed))
    }

    fn show_chooser(&mut self, ui: &mut egui::Ui) {
        self.show_file_summary(ui);
        ui.add_space(10.0);

        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::section(ui, t("Target format", "Hedef biçim"));
            theme::hint(
                ui,
                t(
                    "Use the arrow keys and press Enter, or click a format.",
                    "Ok tuşlarıyla seçip Enter'a basın veya biçime tıklayın.",
                ),
            );
            ui.add_space(8.0);

            if self.targets.is_empty() {
                ui.colored_label(
                    theme::WARNING,
                    t(
                        "These files have no common target format.",
                        "Bu dosyalar için ortak bir hedef biçim yok.",
                    ),
                );
                return;
            }

            egui::ScrollArea::vertical()
                .id_salt("hizli-hedefler")
                .max_height(220.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (index, target) in self.targets.iter().enumerate() {
                        let label = target_label(target.id);
                        let response = ui.selectable_value(&mut self.selected, index, label);
                        if response.clicked() {
                            self.selected = index;
                        }
                    }
                });
        });

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let enabled = !self.targets.is_empty() && self.runner.is_some();
            if theme::primary_button(ui, t("Convert", "Dönüştür"), egui::vec2(170.0, 42.0))
                .clicked()
                && enabled
            {
                self.start();
            }
            if ui.button(t("Cancel", "Vazgeç")).clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if enabled {
                theme::hint(ui, "Enter");
            }
        });
    }

    fn show_file_summary(&self, ui: &mut egui::Ui) {
        theme::flat_card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                theme::section(
                    ui,
                    format!(
                        "{} {} {}",
                        self.files.len(),
                        i18n::files_label(self.files.len()),
                        t("selected", "seçildi")
                    ),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    theme::hint(
                        ui,
                        self.files
                            .iter()
                            .map(|file| file.format.id.to_ascii_uppercase())
                            .collect::<Vec<_>>()
                            .join(" · "),
                    );
                });
            });
            ui.add_space(4.0);
            for file in self.files.iter().take(5) {
                ui.add(
                    egui::Label::new(
                        RichText::new(
                            file.path
                                .file_name()
                                .map(|name| name.to_string_lossy().to_string())
                                .unwrap_or_else(|| file.path.display().to_string()),
                        )
                        .small(),
                    )
                    .truncate(),
                );
            }
            if self.files.len() > 5 {
                theme::hint(
                    ui,
                    format!("+{} {}", self.files.len() - 5, t("more", "daha")),
                );
            }
        });
    }

    fn show_status(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            let done = self
                .jobs
                .iter()
                .filter(|job| job.status == JobStatus::Done)
                .count();
            if self.all_finished() && done == self.jobs.len() {
                ui.label(
                    RichText::new(t("Conversion complete", "Dönüşüm tamamlandı"))
                        .size(17.0)
                        .strong()
                        .color(theme::SUCCESS),
                );
            } else if self.all_finished() {
                ui.label(
                    RichText::new(t("Some files failed", "Bazı dosyalar başarısız"))
                        .size(17.0)
                        .strong()
                        .color(theme::DANGER),
                );
            } else {
                ui.label(
                    RichText::new(t("Converting…", "Dönüştürülüyor…"))
                        .size(17.0)
                        .strong(),
                );
            }
            ui.add_space(8.0);
            for job in &self.jobs {
                let (label, color) = match job.status {
                    JobStatus::Queued => (t("Queued", "Sırada"), theme::MUTED),
                    JobStatus::Running => (t("Running", "Çalışıyor"), theme::ACCENT),
                    JobStatus::Done => (t("Done", "Tamam"), theme::SUCCESS),
                    JobStatus::Failed => (t("Failed", "Hata"), theme::DANGER),
                };
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).small().strong().color(color));
                    ui.add(egui::Label::new(RichText::new(&job.name).strong()).truncate());
                });
                ui.add(
                    egui::ProgressBar::new(job.progress as f32 / 100.0)
                        .desired_width(ui.available_width())
                        .corner_radius(6)
                        .fill(color)
                        .show_percentage(),
                );
                if let Some(message) = &job.message {
                    ui.label(RichText::new(message).small().color(theme::DANGER));
                }
                ui.add_space(6.0);
            }
        });

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if self.all_finished() {
                if theme::primary_button(ui, t("Close", "Kapat"), egui::vec2(140.0, 40.0)).clicked()
                {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
                if ui.button(t("Open folder", "Klasörü aç")).clicked() {
                    if let Some(parent) = self.jobs.first().and_then(|job| job.output.parent()) {
                        let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
                    }
                }
            } else {
                ui.spinner();
                theme::hint(
                    ui,
                    t(
                        "This window closes automatically when finished.",
                        "İş bitince bu pencere otomatik kapanacak.",
                    ),
                );
            }
        });
    }
}

impl eframe::App for QuickConvert {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.started {
            self.poll();
            if self.all_finished() {
                ctx.request_repaint_after(Duration::from_millis(200));
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }

        let (up, down, enter, escape) = ctx.input(|input| {
            (
                input.key_pressed(Key::ArrowUp),
                input.key_pressed(Key::ArrowDown),
                input.key_pressed(Key::Enter),
                input.key_pressed(Key::Escape),
            )
        });
        if !self.started {
            if up && !self.targets.is_empty() {
                self.selected = self.selected.saturating_sub(1);
            }
            if down && !self.targets.is_empty() {
                self.selected = (self.selected + 1).min(self.targets.len() - 1);
            }
            if enter {
                self.start();
            }
        }
        if escape {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(18, 16)),
            )
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("ConvertAnything")
                            .size(21.0)
                            .strong()
                            .color(theme::TEXT),
                    );
                    theme::hint(
                        ui,
                        if self.started {
                            t("Conversion status", "Dönüşüm durumu")
                        } else {
                            t(
                                "Choose a target format for the selected files",
                                "Seçilen dosyalar için hedef biçim seçin",
                            )
                        },
                    );
                });
                ui.add_space(12.0);

                if let Some(error) = &self.error {
                    ui.colored_label(theme::DANGER, error);
                    ui.add_space(8.0);
                }
                if self.started {
                    self.show_status(ui);
                } else {
                    self.show_chooser(ui);
                }
            });
    }
}
