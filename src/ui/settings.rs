//! Settings screen.

use egui::RichText;

use crate::core::detector::{PackageManager, ToolStatus};
use crate::i18n::t;
use crate::install;
use crate::state::config::Config;
use crate::ui::theme;

#[derive(Debug, Clone)]
pub enum SettingsAction {
    None,
    Save(Config),
    RerunWizard,
    InstallIntegration,
    InstallApp,
    SetAutostart(bool),
}

pub struct SettingsView {
    pub draft: Config,
    pub dirty: bool,
    pub autostart: bool,
}

impl SettingsView {
    pub fn new(config: &Config) -> Self {
        Self {
            draft: config.clone(),
            dirty: false,
            autostart: install::autostart_enabled(),
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
        ui.horizontal(|ui| {
            ui.label(RichText::new(t("Settings", "Ayarlar")).size(22.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                theme::hint(
                    ui,
                    t(
                        "Stored on this computer only; nothing is sent anywhere.",
                        "Yalnızca bu bilgisayarda saklanır; hiçbir veri dışarı gönderilmez.",
                    ),
                );
            });
        });
        ui.add_space(10.0);

        egui::ScrollArea::vertical()
            .id_salt("ayarlar")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.card_general(ui);
                ui.add_space(12.0);
                self.card_security(ui);
                ui.add_space(12.0);
                self.card_defaults(ui);
                ui.add_space(12.0);
                self.card_tools(ui, tools, manager, &mut action);
                ui.add_space(12.0);
                self.card_system(ui, &mut action);
                ui.add_space(16.0);

                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.dirty,
                            egui::Button::new(
                                RichText::new(t("Save settings", "Ayarları kaydet"))
                                    .strong()
                                    .color(theme::WHITE),
                            )
                            .fill(theme::ACCENT)
                            .corner_radius(9)
                            .min_size(egui::vec2(150.0, 34.0)),
                        )
                        .clicked()
                    {
                        self.draft.normalise();
                        action = SettingsAction::Save(self.draft.clone());
                        self.dirty = false;
                    }
                    if self.dirty {
                        ui.label(
                            RichText::new(t(
                                "There are unsaved changes.",
                                "Kaydedilmemiş değişiklikler var.",
                            ))
                            .small()
                            .color(theme::WARNING),
                        );
                    }
                });
            });

        action
    }

    fn card_general(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::section(ui, t("General", "Genel"));
            ui.add_space(6.0);
            egui::Grid::new("genel-ayarlar")
                .num_columns(2)
                .spacing([18.0, 10.0])
                .show(ui, |ui| {
                    ui.label(t("Language", "Dil"));
                    ui.horizontal(|ui| {
                        let current = self.draft.language.as_str();
                        let label = match current {
                            "tr" => "Türkçe",
                            "en" => "English",
                            _ => t("Automatic (system)", "Otomatik (sistem)"),
                        };
                        let previous = self.draft.language.clone();
                        egui::ComboBox::from_id_salt("dil-secimi")
                            .selected_text(label)
                            .width(190.0)
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_value(
                                        &mut self.draft.language,
                                        "auto".to_string(),
                                        t("Automatic (system)", "Otomatik (sistem)"),
                                    )
                                    .clicked()
                                {
                                    self.dirty = true;
                                }
                                if ui
                                    .selectable_value(
                                        &mut self.draft.language,
                                        "tr".to_string(),
                                        "Türkçe",
                                    )
                                    .clicked()
                                {
                                    self.dirty = true;
                                }
                                if ui
                                    .selectable_value(
                                        &mut self.draft.language,
                                        "en".to_string(),
                                        "English",
                                    )
                                    .clicked()
                                {
                                    self.dirty = true;
                                }
                            });
                        if self.draft.language != previous {
                            self.draft.language_explicit = true;
                            self.dirty = true;
                        }
                        theme::hint(
                            ui,
                            t("applies after restart", "yeniden başlatınca uygulanır"),
                        );
                    });
                    ui.end_row();

                    ui.label(t("Default output folder", "Varsayılan çıktı dizini"));
                    ui.horizontal(|ui| {
                        let text = self
                            .draft
                            .output_dir
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| {
                                t("Next to the source", "Dosyanın yanına").to_string()
                            });
                        ui.add(
                            egui::Label::new(RichText::new(text).small().color(theme::TEXT))
                                .truncate(),
                        );
                        if ui.button(t("Choose…", "Seç…")).clicked() {
                            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                self.draft.output_dir = Some(dir);
                                self.dirty = true;
                            }
                        }
                        if self.draft.output_dir.is_some()
                            && ui.small_button(t("Clear", "Temizle")).clicked()
                        {
                            self.draft.output_dir = None;
                            self.dirty = true;
                        }
                    });
                    ui.end_row();

                    ui.label(t("Parallel jobs", "Paralel iş sayısı"));
                    ui.horizontal(|ui| {
                        let mut auto = self.draft.max_parallel_jobs == 0;
                        if ui.checkbox(&mut auto, t("Automatic", "Otomatik")).changed() {
                            self.draft.max_parallel_jobs = if auto {
                                0
                            } else {
                                self.draft.effective_parallel_jobs()
                            };
                            self.dirty = true;
                        }
                        if !auto
                            && ui
                                .add(egui::Slider::new(&mut self.draft.max_parallel_jobs, 1..=32))
                                .changed()
                        {
                            self.dirty = true;
                        }
                    });
                    ui.end_row();

                    ui.label(t("Job timeout", "İş zaman sınırı"));
                    if ui
                        .add(
                            egui::Slider::new(&mut self.draft.job_timeout_secs, 60..=7200)
                                .suffix(t(" sec", " sn")),
                        )
                        .changed()
                    {
                        self.dirty = true;
                    }
                    ui.end_row();
                });
        });
    }

    fn card_security(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::section(ui, t("Security", "Güvenlik"));
            ui.add_space(6.0);
            egui::Grid::new("guvenlik-ayarlar")
                .num_columns(2)
                .spacing([18.0, 10.0])
                .show(ui, |ui| {
                    ui.label(t("File size limit", "Dosya boyutu sınırı"));
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
                        self.draft.max_file_size_bytes = (gib * 1024.0 * 1024.0 * 1024.0) as u64;
                        self.dirty = true;
                    }
                    ui.end_row();

                    ui.label(t(
                        "Overwrite existing files",
                        "Mevcut dosyaların üzerine yaz",
                    ));
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.draft.allow_overwrite, "");
                        theme::hint(
                            ui,
                            t(
                                "When off, clashing outputs get -1, -2 (recommended).",
                                "Kapalıyken çakışan çıktılara -1, -2 eklenir (önerilir).",
                            ),
                        );
                    });
                    ui.end_row();
                });
        });
    }

    fn card_defaults(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::section(ui, t("Conversion defaults", "Dönüşüm varsayılanları"));
            ui.add_space(6.0);
            egui::Grid::new("donusum-ayarlar")
                .num_columns(2)
                .spacing([18.0, 10.0])
                .show(ui, |ui| {
                    ui.label(t(
                        "JPEG quality (WebP is lossless)",
                        "JPEG kalitesi (WebP kayıpsız)",
                    ));
                    if ui
                        .add(egui::Slider::new(&mut self.draft.image_quality, 1..=100))
                        .changed()
                    {
                        self.dirty = true;
                    }
                    ui.end_row();

                    ui.label(t("Default video bitrate", "Varsayılan video bit hızı"));
                    ui.horizontal(|ui| {
                        let mut enabled = self.draft.bitrate_kbps.is_some();
                        if ui.checkbox(&mut enabled, "").changed() {
                            self.draft.bitrate_kbps = if enabled { Some(4000) } else { None };
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

                    ui.label(t("Video scale", "Video ölçeği"));
                    let mut scale = self.draft.video_scale.clone().unwrap_or_default();
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut scale)
                                .hint_text(t("e.g. 1280:-2", "örn. 1280:-2")),
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
    }

    fn card_tools(
        &mut self,
        ui: &mut egui::Ui,
        tools: &[ToolStatus],
        manager: Option<PackageManager>,
        action: &mut SettingsAction,
    ) {
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::section(ui, t("External tools", "Harici araçlar"));
            theme::hint(
                ui,
                t(
                    "Conversion engines are not bundled; they must be installed on the system.",
                    "Dönüşüm motorları uygulamaya gömülü değildir; sistemde kurulu olmalıdır.",
                ),
            );
            ui.add_space(8.0);

            for status in tools {
                ui.horizontal(|ui| {
                    let (mark, color) = if status.available() {
                        (t("installed", "kurulu"), theme::SUCCESS)
                    } else {
                        (t("missing", "eksik"), theme::DANGER)
                    };
                    theme::dot(ui, color);
                    ui.label(RichText::new(mark).small().strong().color(color));
                    ui.label(status.tool.display_name());
                    if let Some(path) = &status.path {
                        theme::hint(ui, path.display().to_string());
                    }
                });
            }

            ui.add_space(6.0);
            theme::hint(
                ui,
                match manager {
                    Some(manager) => format!(
                        "{} {}",
                        t("Package manager:", "Paket yöneticisi:"),
                        manager.display_name()
                    ),
                    None => t(
                        "No package manager detected; install tools manually.",
                        "Paket yöneticisi algılanamadı; araçları elle kurmanız gerekir.",
                    )
                    .to_string(),
                },
            );
            ui.add_space(8.0);
            if ui
                .button(t(
                    "Run setup wizard again",
                    "Kurulum sihirbazını yeniden çalıştır",
                ))
                .clicked()
            {
                *action = SettingsAction::RerunWizard;
            }
        });
    }

    fn card_system(&mut self, ui: &mut egui::Ui, action: &mut SettingsAction) {
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::section(ui, t("System integration", "Sistem entegrasyonu"));
            match install::installed_executable() {
                Some(path) => {
                    ui.horizontal(|ui| {
                        theme::dot(ui, theme::SUCCESS);
                        ui.label(
                            RichText::new(t("App installed", "Uygulama kurulu"))
                                .small()
                                .strong()
                                .color(theme::SUCCESS),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(path.display().to_string())
                                    .small()
                                    .color(theme::MUTED),
                            )
                            .truncate(),
                        );
                    });
                }
                None => {
                    theme::hint(
                        ui,
                        t(
                            "Not installed yet; the app runs from its current location.",
                            "Henüz kurulmadı; uygulama bulunduğu konumdan çalışıyor.",
                        ),
                    );
                }
            }
            ui.add_space(8.0);

            let mut autostart = self.autostart;
            if ui
                .checkbox(
                    &mut autostart,
                    t("Start with the session", "Oturum açılışında başlat"),
                )
                .changed()
            {
                self.autostart = autostart;
                *action = SettingsAction::SetAutostart(autostart);
            }
            theme::hint(
                ui,
                t(
                    "The app opens automatically after you log in.",
                    "Oturum açtığınızda uygulama kendiliğinden açılır.",
                ),
            );

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if theme::accent_button(
                    ui,
                    t("Install app", "Uygulamayı kur"),
                    egui::vec2(140.0, 32.0),
                )
                .on_hover_text(t(
                    "Copies the AppImage to ~/.local/bin, registers the context menu and enables login autostart.",
                    "AppImage'ı ~/.local/bin'e kopyalar, sağ tık menüsünü kurar ve oturum açılışında başlatmayı açar.",
                ))
                .clicked()
                {
                    *action = SettingsAction::InstallApp;
                }
                if ui
                    .button(t(
                        "Repair context menu",
                        "Sağ tık menüsünü onar",
                    ))
                    .clicked()
                {
                    *action = SettingsAction::InstallIntegration;
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::detector::Tool;

    fn statuses() -> Vec<ToolStatus> {
        Tool::ALL
            .into_iter()
            .map(|tool| ToolStatus { tool, path: None })
            .collect()
    }

    #[test]
    fn renders_without_duplicate_id_panics() {
        let mut view = SettingsView::new(&Config::default());
        let tools = statuses();
        // Two consecutive frames: egui detects duplicate widget ids only on
        // the second pass.
        egui::__run_test_ui(|ui| {
            let _ = view.show(ui, &tools, Some(PackageManager::Dnf));
        });
        egui::__run_test_ui(|ui| {
            let _ = view.show(ui, &tools, None);
        });
    }

    #[test]
    fn sync_resets_dirty_flag() {
        let mut view = SettingsView::new(&Config::default());
        view.dirty = true;
        let config = Config {
            image_quality: 55,
            ..Config::default()
        };
        view.sync(&config);
        assert!(!view.dirty);
        assert_eq!(view.draft.image_quality, 55);
    }
}
