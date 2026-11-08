//! Settings tab.

use egui::{Color32, RichText};

use crate::core::detector::{PackageManager, ToolStatus};

use crate::state::config::Config;

#[derive(Debug, Clone)]
pub enum SettingsAction {
    None,
    Save(Config),
    RerunWizard,
}

pub struct SettingsView {
    pub draft: Config,
    pub dirty: bool,
}

impl SettingsView {
    pub fn new(config: &Config) -> Self {
        Self {
            draft: config.clone(),
            dirty: false,
        }
    }

    pub fn sync(&mut self, config: &Config) {
        self.draft = config.clone();
        self.dirty = false;
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        tools: &[ToolStatus],
        manager: Option<PackageManager>,
    ) -> SettingsAction {
        let mut action = SettingsAction::None;
        ui.heading("Ayarlar");
        ui.label(
            RichText::new(
                "Ayarlar yalnızca bu bilgisayarda saklanır; hiçbir veri dışarı gönderilmez.",
            )
            .small()
            .color(Color32::GRAY),
        );
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.group(|ui| {
                    ui.label(RichText::new("Genel").strong());
                    egui::Grid::new("genel-ayarlar")
                        .num_columns(2)
                        .spacing([16.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Varsayılan çıktı dizini");
                            ui.horizontal(|ui| {
                                let text = self
                                    .draft
                                    .output_dir
                                    .as_ref()
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_else(|| "Dosyanın yanına".to_string());
                                ui.label(RichText::new(text).monospace());
                                if ui.button("Seç…").clicked() {
                                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                        self.draft.output_dir = Some(dir);
                                        self.dirty = true;
                                    }
                                }
                                if self.draft.output_dir.is_some()
                                    && ui.small_button("Temizle").clicked()
                                {
                                    self.draft.output_dir = None;
                                    self.dirty = true;
                                }
                            });
                            ui.end_row();

                            ui.label("Paralel iş sayısı");
                            ui.horizontal(|ui| {
                                let mut auto = self.draft.max_parallel_jobs == 0;
                                if ui.checkbox(&mut auto, "Otomatik").changed() {
                                    self.draft.max_parallel_jobs = if auto {
                                        0
                                    } else {
                                        self.draft.effective_parallel_jobs()
                                    };
                                    self.dirty = true;
                                }
                                if !auto
                                    && ui
                                        .add(egui::Slider::new(
                                            &mut self.draft.max_parallel_jobs,
                                            1..=32,
                                        ))
                                        .changed()
                                {
                                    self.dirty = true;
                                }
                            });
                            ui.end_row();

                            ui.label("İş zaman sınırı");
                            if ui
                                .add(
                                    egui::Slider::new(&mut self.draft.job_timeout_secs, 60..=7200)
                                        .suffix(" sn"),
                                )
                                .changed()
                            {
                                self.dirty = true;
                            }
                            ui.end_row();
                        });
                });

                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.label(RichText::new("Güvenlik").strong());
                    egui::Grid::new("guvenlik-ayarlar")
                        .num_columns(2)
                        .spacing([16.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("Dosya boyutu sınırı");
                            let mut gib = (self.draft.max_file_size_bytes as f64
                                / (1024.0 * 1024.0 * 1024.0))
                                .clamp(0.05, 128.0);
                            if ui
                                .add(
                                    egui::Slider::new(&mut gib, 0.05..=64.0)
                                        .suffix(" GiB")
                                        .logarithmic(true),
                                )
                                .changed()
                            {
                                self.draft.max_file_size_bytes =
                                    (gib * 1024.0 * 1024.0 * 1024.0) as u64;
                                self.dirty = true;
                            }
                            ui.end_row();

                            ui.label("Mevcut dosyaların üzerine yaz");
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut self.draft.allow_overwrite, "");
                                ui.label(
                                    RichText::new(
                                        "Kapalıyken çakışan çıktılara -1, -2 eklenir (önerilir).",
                                    )
                                    .small()
                                    .color(Color32::GRAY),
                                );
                            });
                            ui.end_row();
                        });
                });

                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.label(RichText::new("Dönüşüm varsayılanları").strong());
                    egui::Grid::new("donusum-ayarlar")
                        .num_columns(2)
                        .spacing([16.0, 8.0])
                        .show(ui, |ui| {
                            ui.label("JPEG kalitesi (WebP kayıpsız)");
                            if ui
                                .add(egui::Slider::new(&mut self.draft.image_quality, 1..=100))
                                .changed()
                            {
                                self.dirty = true;
                            }
                            ui.end_row();

                            ui.label("Varsayılan video bit hızı");
                            ui.horizontal(|ui| {
                                let mut enabled = self.draft.bitrate_kbps.is_some();
                                if ui.checkbox(&mut enabled, "").changed() {
                                    self.draft.bitrate_kbps =
                                        if enabled { Some(4000) } else { None };
                                    self.dirty = true;
                                }
                                if let Some(rate) = self.draft.bitrate_kbps.as_mut() {
                                    if ui
                                        .add(egui::Slider::new(rate, 32..=50000).suffix(" kbps"))
                                        .changed()
                                    {
                                        self.dirty = true;
                                    }
                                }
                            });
                            ui.end_row();

                            ui.label("Video ölçeği");
                            let mut scale = self.draft.video_scale.clone().unwrap_or_default();
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut scale)
                                        .hint_text("örn. 1280:-2"),
                                )
                                .changed()
                            {
                                self.draft.video_scale = if scale.trim().is_empty() {
                                    None
                                } else {
                                    Some(scale.clone())
                                };
                                self.dirty = true;
                            }
                            ui.end_row();
                        });
                });

                ui.add_space(8.0);
                ui.group(|ui| {
                    ui.label(RichText::new("Harici araçlar").strong());
                    ui.label(
                    RichText::new(
                        "Dönüşüm motorları uygulamaya gömülü değildir; sistemde kurulu olmalıdır.",
                    )
                    .small()
                    .color(Color32::GRAY),
                );
                    ui.add_space(4.0);
                    for status in tools {
                        ui.horizontal(|ui| {
                            let (mark, color) = if status.available() {
                                ("kurulu", Color32::from_rgb(80, 160, 90))
                            } else {
                                ("eksik", Color32::from_rgb(200, 90, 60))
                            };
                            ui.label(RichText::new(mark).color(color).strong());
                            ui.label(status.tool.display_name());
                            if let Some(path) = &status.path {
                                ui.label(
                                    RichText::new(path.display().to_string())
                                        .small()
                                        .monospace()
                                        .color(Color32::GRAY),
                                );
                            }
                        });
                    }
                    ui.add_space(4.0);
                    ui.label(match manager {
                        Some(manager) => format!("Paket yöneticisi: {}", manager.display_name()),
                        None => "Paket yöneticisi algılanamadı; araçları elle kurmanız gerekir."
                            .to_string(),
                    });
                    if ui.button("Kurulum sihirbazını yeniden çalıştır").clicked() {
                        action = SettingsAction::RerunWizard;
                    }
                });

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.dirty, egui::Button::new("Ayarları kaydet"))
                        .clicked()
                    {
                        self.draft.normalise();
                        action = SettingsAction::Save(self.draft.clone());
                        self.dirty = false;
                    }
                    if self.dirty {
                        ui.label(
                            RichText::new("Kaydedilmemiş değişiklikler var.")
                                .small()
                                .color(Color32::from_rgb(200, 140, 0)),
                        );
                    }
                });
            });

        action
    }
}
