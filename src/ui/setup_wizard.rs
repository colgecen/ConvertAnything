//! First-run setup wizard.

use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use anyhow::Context;
use egui::{Align2, Color32, RichText};

use crate::core::detector::{self, PackageManager, Tool, ToolStatus};
use crate::core::installer::{self, InstallReport};

#[derive(Debug, Clone)]
pub enum WizardAction {
    StartInstall(Vec<Tool>),
    Skip,
    Installed { success: bool, summary: String },
}

pub struct SetupWizard {
    pub open: bool,
    pub selected: Vec<Tool>,
    pub installing: bool,
    pub message: Option<(String, bool)>,
    install_rx: Option<Receiver<anyhow::Result<InstallReport>>>,
}

impl SetupWizard {
    pub fn new(config: &crate::state::config::Config, tools: &[ToolStatus]) -> Self {
        let selected = detector::missing_tools(tools);
        Self {
            open: !config.setup_done,
            selected,
            installing: false,
            message: None,
            install_rx: None,
        }
    }

    /// Re-opens the wizard and preselects every currently missing tool.
    pub fn reopen(&mut self, tools: &[ToolStatus]) {
        self.open = true;
        self.installing = false;
        self.install_rx = None;
        self.message = None;
        self.selected = detector::missing_tools(tools);
    }

    pub fn is_installing(&self) -> bool {
        self.installing
    }

    pub fn start_install(&mut self, manager: PackageManager, tools: Vec<Tool>) {
        if tools.is_empty() {
            return;
        }
        self.installing = true;
        self.message = Some((
            "Kurulum başlatıldı. Polkit penceresinde parolanızı girebilirsiniz.".to_string(),
            true,
        ));
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<InstallReport> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .context("kurulum çalışma zamanı başlatılamadı")?;
                runtime.block_on(installer::install(
                    manager,
                    &tools,
                    Duration::from_secs(15 * 60),
                ))
            })();
            let _ = tx.send(result);
        });
        self.install_rx = Some(rx);
    }

    fn poll_install(&mut self) -> Option<WizardAction> {
        let result = match self.install_rx.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(result)) => Some(result),
            Some(Err(TryRecvError::Empty)) | None => return None,
            Some(Err(TryRecvError::Disconnected)) => {
                self.install_rx = None;
                self.installing = false;
                self.message = Some((
                    "Kurulum işlemi beklenmedik şekilde durdu.".to_string(),
                    false,
                ));
                return Some(WizardAction::Installed {
                    success: false,
                    summary: "Kurulum işlemi beklenmedik şekilde durdu.".to_string(),
                });
            }
        };
        self.install_rx = None;
        self.installing = false;

        match result {
            Some(Ok(report)) => {
                self.message = Some((report.summary.clone(), report.success));
                Some(WizardAction::Installed {
                    success: report.success,
                    summary: report.summary,
                })
            }
            Some(Err(error)) => {
                let summary = format!("{error:#}");
                self.message = Some((summary.clone(), false));
                Some(WizardAction::Installed {
                    success: false,
                    summary,
                })
            }
            None => None,
        }
    }

    /// Draws the modal wizard and reports any requested action.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        tools: &[ToolStatus],
        manager: Option<PackageManager>,
    ) -> Option<WizardAction> {
        if let Some(action) = self.poll_install() {
            return Some(action);
        }
        if !self.open {
            return None;
        }

        let missing = detector::missing_tools(tools);
        let mut action = None;
        let mut window_open = true;

        egui::Window::new("İlk kurulum")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_max_width(520.0);
                ui.heading("Dönüştür'e hoş geldiniz");
                ui.label(
                    "Dönüşüm motorları uygulamaya gömülü değildir. Eksik araçları bir kez \
                     kurmanız yeterlidir; kurulum yalnızca sabit paket adlarıyla ve pkexec \
                     üzerinden yapılır.",
                );
                ui.add_space(8.0);
                ui.separator();

                for status in tools {
                    ui.horizontal(|ui| {
                        if status.available() {
                            ui.label(RichText::new("✔").color(Color32::from_rgb(80, 160, 90)));
                            ui.label(status.tool.display_name());
                            ui.label(RichText::new("kurulu").small().color(Color32::GRAY));
                        } else {
                            let selected = self.selected.contains(&status.tool);
                            let mut checked = selected;
                            if ui.checkbox(&mut checked, "").changed() {
                                if checked {
                                    self.selected.push(status.tool);
                                } else {
                                    self.selected.retain(|t| *t != status.tool);
                                }
                            }
                            ui.label(status.tool.display_name());
                            if status.tool.is_critical() {
                                ui.label(
                                    RichText::new("gerekli")
                                        .small()
                                        .color(Color32::from_rgb(200, 90, 60)),
                                );
                            } else {
                                ui.label(
                                    RichText::new("isteğe bağlı").small().color(Color32::GRAY),
                                );
                            }
                        }
                    });
                }

                ui.add_space(8.0);
                ui.separator();

                if missing.is_empty() {
                    ui.colored_label(
                        Color32::from_rgb(80, 160, 90),
                        "Tüm dönüşüm araçları hazır.",
                    );
                } else {
                    match manager {
                        Some(manager) => {
                            ui.label(
                                RichText::new(format!(
                                    "Kurulum için {} kullanılacak; polkit sizden yetki isteyecek.",
                                    manager.display_name()
                                ))
                                .small()
                                .color(Color32::GRAY),
                            );
                        }
                        None => {
                            ui.colored_label(
                                Color32::from_rgb(200, 90, 60),
                                "Paket yöneticisi algılanamadı. Araçları dağıtımınızın \
                                 paket yöneticisiyle elle kurabilirsiniz.",
                            );
                        }
                    }
                }

                if let Some((text, success)) = &self.message {
                    ui.add_space(4.0);
                    let color = if *success {
                        Color32::from_rgb(80, 160, 90)
                    } else {
                        Color32::from_rgb(200, 90, 60)
                    };
                    ui.colored_label(color, text);
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if self.installing {
                        ui.spinner();
                        ui.label("Kurulum sürüyor…");
                    } else {
                        let can_install = manager.is_some() && !self.selected.is_empty();
                        if ui
                            .add_enabled(can_install, egui::Button::new("Seçilenleri kur"))
                            .clicked()
                        {
                            action = Some(WizardAction::StartInstall(self.selected.clone()));
                        }
                        if ui.button("Şimdilik atla").clicked() {
                            action = Some(WizardAction::Skip);
                        }
                        if missing.is_empty() && ui.button("Başla").clicked() {
                            action = Some(WizardAction::Skip);
                        }
                    }
                });
            });

        if !window_open && !self.installing {
            // Closing the window is equivalent to skipping for this session.
            self.open = false;
            return Some(WizardAction::Skip);
        }

        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::config::Config;

    fn statuses(missing: &[Tool]) -> Vec<ToolStatus> {
        Tool::ALL
            .into_iter()
            .map(|tool| ToolStatus {
                tool,
                path: if missing.contains(&tool) {
                    None
                } else {
                    Some(std::path::PathBuf::from("/usr/bin"))
                },
            })
            .collect()
    }

    #[test]
    fn opens_only_when_setup_is_incomplete() {
        let tools = statuses(&[Tool::Ffmpeg]);
        let wizard = SetupWizard::new(&Config::default(), &tools);
        assert!(wizard.open);
        assert_eq!(wizard.selected, vec![Tool::Ffmpeg]);

        let done = Config {
            setup_done: true,
            ..Config::default()
        };
        let wizard = SetupWizard::new(&done, &tools);
        assert!(!wizard.open);
    }

    #[test]
    fn closing_the_wizard_counts_as_skip() {
        let tools = statuses(&[Tool::Ffmpeg]);
        let mut wizard = SetupWizard::new(&Config::default(), &tools);
        wizard.open = false;
        egui::__run_test_ui(|ui| {
            let action = wizard.show(ui.ctx(), &tools, Some(PackageManager::Dnf));
            assert!(action.is_none());
        });
    }

    #[test]
    fn reopened_wizard_preselects_missing_tools() {
        let mut tools = statuses(&[Tool::Ffmpeg]);
        let mut wizard = SetupWizard::new(
            &Config {
                setup_done: true,
                ..Config::default()
            },
            &tools,
        );
        tools[0].path = Some(std::path::PathBuf::from("/usr/bin/ffmpeg"));
        wizard.reopen(&tools);
        assert!(wizard.open);
        assert!(wizard.selected.is_empty());
    }
}
