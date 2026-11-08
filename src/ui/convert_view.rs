//! Shared conversion view used by all media tabs.

use std::path::PathBuf;

use egui::{Color32, RichText};

use crate::converters::document;
use crate::core::detector::{Tool, ToolStatus};
use crate::core::registry::{self, Format, MediaKind};

/// One file queued in a tab.
#[derive(Debug, Clone)]
pub struct FileItem {
    pub path: PathBuf,
    pub format: &'static Format,
    pub size: u64,
}

/// Per-tab UI state.
pub struct ConvertView {
    pub kind: MediaKind,
    pub files: Vec<FileItem>,
    pub target: Option<String>,
    pub output_dir: Option<PathBuf>,
    pub image_quality: u8,
    pub bitrate_kbps: Option<u32>,
    pub video_scale: String,
}

impl ConvertView {
    pub fn new(
        kind: MediaKind,
        image_quality: u8,
        bitrate_kbps: Option<u32>,
        video_scale: Option<String>,
    ) -> Self {
        Self {
            kind,
            files: Vec::new(),
            target: None,
            output_dir: None,
            image_quality,
            bitrate_kbps,
            video_scale: video_scale.unwrap_or_default(),
        }
    }

    pub fn add_path(&mut self, path: PathBuf) -> bool {
        let Some(format) = registry::detect_format(&path) else {
            return false;
        };
        if format.kind != self.kind {
            return false;
        }
        if registry::has_forbidden_extension(&path) || registry::looks_executable(&path) {
            return false;
        }
        if self.files.iter().any(|f| f.path == path) {
            return true;
        }
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        self.files.push(FileItem { path, format, size });
        true
    }

    pub fn clear(&mut self) {
        self.files.clear();
    }

    /// Targets supported by every queued file and by the installed tools.
    pub fn available_targets(&self, tools: &[ToolStatus]) -> Vec<&'static Format> {
        let Some(first) = self.files.first() else {
            return Vec::new();
        };
        let mut targets = registry::targets_for(first.format);
        targets.retain(|target| {
            self.files.iter().all(|file| {
                registry::is_supported(file.format, target.id)
                    && target_allowed(tools, file.format, target)
            })
        });
        targets
    }

    /// Renders the view. Returns true when the user asked to start conversion.
    pub fn show(&mut self, ui: &mut egui::Ui, tools: &[ToolStatus]) -> bool {
        let mut start = false;

        ui.horizontal(|ui| {
            ui.heading(format!("{} dönüşümü", self.kind.label()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Listeyi temizle").clicked() {
                    self.clear();
                }
                if ui.button("Dosya ekle…").clicked() {
                    self.pick_files();
                }
            });
        });
        ui.add_space(4.0);

        ui.label(
            RichText::new("Dosyaları bu pencereye sürükleyip bırakabilirsiniz.")
                .small()
                .color(Color32::GRAY),
        );

        let targets = self.available_targets(tools);
        if !targets.is_empty() {
            let valid = self
                .target
                .as_ref()
                .map(|t| targets.iter().any(|f| f.id == *t))
                .unwrap_or(false);
            if !valid {
                self.target = Some(targets[0].id.to_string());
            }
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label("Hedef biçim:");
            let selected = self.target.as_deref().unwrap_or("—");
            egui::ComboBox::from_id_salt(("hedef", self.kind.label()))
                .selected_text(selected)
                .width(180.0)
                .show_ui(ui, |ui| {
                    for target in &targets {
                        let label = if target.id == registry::EXTRACT {
                            target.label.to_string()
                        } else {
                            format!("{} (.{})", target.label, target.id)
                        };
                        ui.selectable_value(&mut self.target, Some(target.id.to_string()), label);
                    }
                });

            ui.separator();
            ui.label("Çıktı dizini:");
            let text = self
                .output_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Dosyanın yanına".to_string());
            ui.label(RichText::new(text).monospace());
            if ui.button("Seç…").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.output_dir = Some(dir);
                }
            }
            if self.output_dir.is_some() && ui.small_button("Sıfırla").clicked() {
                self.output_dir = None;
            }
        });

        self.show_options(ui);
        ui.add_space(8.0);
        ui.separator();

        if self.files.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("Henüz dosya yok")
                        .size(18.0)
                        .color(Color32::GRAY),
                );
                ui.label("Dosyaları sürükleyin veya “Dosya ekle…” düğmesini kullanın.");
            });
        } else {
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut remove: Option<usize> = None;
                    for (index, file) in self.files.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui
                                .small_button("✕")
                                .on_hover_text("Listeden çıkar")
                                .clicked()
                            {
                                remove = Some(index);
                            }
                            ui.label(
                                RichText::new(
                                    file.path
                                        .file_name()
                                        .map(|n| n.to_string_lossy().to_string())
                                        .unwrap_or_else(|| file.path.display().to_string()),
                                )
                                .strong(),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "{} · {}",
                                    file.format.id,
                                    human_size(file.size)
                                ))
                                .small()
                                .color(Color32::GRAY),
                            );
                        });
                    }
                    if let Some(index) = remove {
                        self.files.remove(index);
                    }
                });
        }

        ui.add_space(8.0);
        if targets.is_empty() && !self.files.is_empty() {
            ui.colored_label(
                Color32::from_rgb(200, 140, 0),
                "Bu dosyalar için ortak bir hedef biçim yok veya gerekli araç kurulu değil.",
            );
        }

        ui.horizontal(|ui| {
            let enabled = !self.files.is_empty()
                && self.target.is_some()
                && !targets.is_empty()
                && self.has_required_tool(tools);
            if ui
                .add_enabled(enabled, egui::Button::new("Dönüştür"))
                .clicked()
            {
                start = true;
            }
            if !self.has_required_tool(tools) {
                ui.colored_label(
                    Color32::from_rgb(200, 90, 60),
                    "Gerekli araç kurulu değil. Ayarlar → Araçlar bölümünden kurabilirsiniz.",
                );
            }
        });

        start
    }

    fn show_options(&mut self, ui: &mut egui::Ui) {
        match self.kind {
            MediaKind::Image => {
                ui.horizontal(|ui| {
                    ui.label("JPEG kalitesi:");
                    ui.add(egui::Slider::new(&mut self.image_quality, 1..=100));
                });
            }
            MediaKind::Audio => {
                ui.horizontal(|ui| {
                    let mut limited = self.bitrate_kbps.is_some();
                    if ui.checkbox(&mut limited, "Bit hızı sınırı").changed() {
                        self.bitrate_kbps = if limited { Some(192) } else { None };
                    }
                    if let Some(rate) = self.bitrate_kbps.as_mut() {
                        ui.add(egui::Slider::new(rate, 32..=320).suffix(" kbps"));
                    }
                });
            }
            MediaKind::Video => {
                ui.horizontal(|ui| {
                    let mut limited = self.bitrate_kbps.is_some();
                    if ui.checkbox(&mut limited, "Bit hızı sınırı").changed() {
                        self.bitrate_kbps = if limited { Some(4000) } else { None };
                    }
                    if let Some(rate) = self.bitrate_kbps.as_mut() {
                        ui.add(egui::Slider::new(rate, 256..=20000).suffix(" kbps"));
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Ölçek:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.video_scale)
                            .hint_text("örn. 1280:-2 (boş = kaynak)"),
                    );
                    if !self.video_scale.is_empty() {
                        ui.label(
                            RichText::new("Genişlik:yükseklik; -2 oranı korur.")
                                .small()
                                .color(Color32::GRAY),
                        );
                    }
                });
            }
            MediaKind::Document | MediaKind::Archive => {}
        }
    }

    fn has_required_tool(&self, tools: &[ToolStatus]) -> bool {
        if self.files.is_empty() {
            return true;
        }
        let target = self.target.as_deref().unwrap_or_default();
        let source = self.files[0].format;
        match source.kind {
            MediaKind::Audio | MediaKind::Video => tool_available(tools, Tool::Ffmpeg),
            MediaKind::Document => match document::strategy(source.id, target) {
                document::Strategy::Encoding => true,
                document::Strategy::LibreOffice => tool_available(tools, Tool::LibreOffice),
                document::Strategy::Pandoc => tool_available(tools, Tool::Pandoc),
            },
            _ => true,
        }
    }

    fn pick_files(&mut self) {
        let mut dialog = rfd::FileDialog::new();
        match self.kind {
            MediaKind::Image => {
                dialog = dialog.add_filter(
                    "Görseller",
                    &[
                        "jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff", "gif", "ico",
                    ],
                );
            }
            MediaKind::Audio => {
                dialog = dialog.add_filter(
                    "Ses dosyaları",
                    &["mp3", "wav", "flac", "ogg", "opus", "aac", "m4a"],
                );
            }
            MediaKind::Video => {
                dialog =
                    dialog.add_filter("Video dosyaları", &["mp4", "mkv", "webm", "avi", "mov"]);
            }
            MediaKind::Document => {
                dialog = dialog.add_filter(
                    "Belgeler",
                    &[
                        "pdf", "docx", "odt", "md", "html", "htm", "epub", "txt", "csv",
                    ],
                );
            }
            MediaKind::Archive => {
                dialog = dialog.add_filter("Arşivler", &["zip", "tar", "gz", "tgz"]);
            }
        }
        if let Some(paths) = dialog.pick_files() {
            for path in paths {
                self.add_path(path);
            }
        }
    }
}

fn target_allowed(tools: &[ToolStatus], source: &Format, target: &Format) -> bool {
    if source.kind != MediaKind::Document || target.kind == MediaKind::Archive {
        return true;
    }
    match document::strategy(source.id, target.id) {
        document::Strategy::Encoding => true,
        document::Strategy::LibreOffice => tool_available(tools, Tool::LibreOffice),
        document::Strategy::Pandoc => tool_available(tools, Tool::Pandoc),
    }
}

pub fn tool_available(tools: &[ToolStatus], tool: Tool) -> bool {
    tools
        .iter()
        .find(|s| s.tool == tool)
        .map(|s| s.available())
        .unwrap_or(false)
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_formats() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1024), "1.0 KiB");
        assert_eq!(human_size(1536), "1.5 KiB");
    }
}
