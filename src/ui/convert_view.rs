//! Unified bulk conversion view.
//!
//! A single screen collects files of any supported kind. The target list is
//! the intersection of what every queued file can become, so the bulk action
//! never silently skips a file.

use std::path::PathBuf;

use egui::{Color32, RichText, Stroke};

use crate::converters::document;
use crate::core::detector::{Tool, ToolStatus};
use crate::core::registry::{self, Format, MediaKind};

pub const ACCENT: Color32 = Color32::from_rgb(74, 144, 226);
pub const SUCCESS: Color32 = Color32::from_rgb(76, 175, 120);
pub const WARNING: Color32 = Color32::from_rgb(230, 168, 58);
pub const DANGER: Color32 = Color32::from_rgb(226, 96, 76);
pub const MUTED: Color32 = Color32::from_rgb(146, 152, 162);

/// One file in the shared queue.
#[derive(Debug, Clone)]
pub struct FileItem {
    pub path: PathBuf,
    pub format: &'static Format,
    pub size: u64,
}

/// Shared conversion state for all file kinds.
pub struct ConvertView {
    pub files: Vec<FileItem>,
    pub target: Option<String>,
    pub output_dir: Option<PathBuf>,
    pub image_quality: u8,
    pub bitrate_kbps: Option<u32>,
    pub video_scale: String,
}

impl ConvertView {
    pub fn new(image_quality: u8, bitrate_kbps: Option<u32>, video_scale: Option<String>) -> Self {
        Self {
            files: Vec::new(),
            target: None,
            output_dir: None,
            image_quality,
            bitrate_kbps,
            video_scale: video_scale.unwrap_or_default(),
        }
    }

    /// Adds a file of any supported kind. Returns false when rejected.
    pub fn add_path(&mut self, path: PathBuf) -> bool {
        let Some(format) = registry::detect_format(&path) else {
            return false;
        };
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
        self.target = None;
    }

    pub fn has_kind(&self, kind: MediaKind) -> bool {
        self.files.iter().any(|file| file.format.kind == kind)
    }

    pub fn kind_count(&self) -> usize {
        let mut seen = Vec::new();
        for file in &self.files {
            if !seen.contains(&file.format.kind) {
                seen.push(file.format.kind);
            }
        }
        seen.len()
    }

    pub fn total_size(&self) -> u64 {
        self.files.iter().map(|file| file.size).sum()
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
        let targets = self.available_targets(tools);
        self.sync_target(&targets);

        if self.files.is_empty() {
            if Self::show_empty_state(ui) {
                self.pick_files();
            }
            return false;
        }

        self.show_toolbar(ui);
        ui.add_space(10.0);
        self.show_file_list(ui);
        ui.add_space(12.0);
        self.show_options(ui, &targets);
        ui.add_space(12.0);
        self.show_actions(ui, &targets, tools)
    }

    fn sync_target(&mut self, targets: &[&'static Format]) {
        if targets.is_empty() {
            self.target = None;
            return;
        }
        let valid = self
            .target
            .as_deref()
            .map(|current| targets.iter().any(|f| f.id == current))
            .unwrap_or(false);
        if !valid {
            self.target = Some(targets[0].id.to_string());
        }
    }

    fn show_empty_state(ui: &mut egui::Ui) -> bool {
        let mut picked = false;
        card(ui, ACCENT.gamma_multiply(0.55), |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add_space(14.0);
                ui.label(RichText::new("⇩").size(42.0).color(ACCENT));
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Dosyalarınızı buraya bırakın")
                        .size(21.0)
                        .strong(),
                );
                ui.label(
                    RichText::new(
                        "Görsel, ses, video, belge ve arşiv dosyalarını karışık olarak ekleyebilirsiniz.",
                    )
                    .color(MUTED),
                );
                ui.add_space(14.0);
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("Dosya seç…").strong().color(Color32::WHITE),
                        )
                        .fill(ACCENT)
                        .min_size(egui::vec2(160.0, 36.0)),
                    )
                    .clicked()
                {
                    picked = true;
                }
                ui.add_space(14.0);
            });
        });
        picked
    }

    fn show_toolbar(&mut self, ui: &mut egui::Ui) {
        card(
            ui,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
            |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("{} dosya", self.files.len()))
                            .size(16.0)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} tür · {}",
                            self.kind_count(),
                            human_size(self.total_size())
                        ))
                        .small()
                        .color(MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Tümünü temizle").clicked() {
                            self.clear();
                        }
                        if ui
                            .add(
                                egui::Button::new(RichText::new("+  Dosya ekle").strong())
                                    .fill(ACCENT)
                                    .min_size(egui::vec2(130.0, 30.0)),
                            )
                            .clicked()
                        {
                            self.pick_files();
                        }
                    });
                });
            },
        );
    }

    fn show_file_list(&mut self, ui: &mut egui::Ui) {
        card(
            ui,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
            |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Dosyalar").strong());
                ui.add_space(6.0);

                let mut remove: Option<usize> = None;
                egui::ScrollArea::vertical()
                    .id_salt("dosya-listesi")
                    .max_height(300.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (index, file) in self.files.iter().enumerate() {
                            egui::Frame::default()
                                .fill(ui.visuals().faint_bg_color)
                                .corner_radius(8)
                                .inner_margin(egui::Margin::symmetric(10, 7))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.horizontal(|ui| {
                                        kind_badge(ui, file.format.kind);
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new(display_name(&file.path)).strong(),
                                            );
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} · {} · {}",
                                                    file.format.label,
                                                    file.format.id,
                                                    human_size(file.size)
                                                ))
                                                .small()
                                                .color(MUTED),
                                            );
                                        });
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .small_button("✕")
                                                    .on_hover_text("Listeden çıkar")
                                                    .clicked()
                                                {
                                                    remove = Some(index);
                                                }
                                            },
                                        );
                                    });
                                });
                            ui.add_space(4.0);
                        }
                    });
                if let Some(index) = remove {
                    self.files.remove(index);
                }
            },
        );
    }

    fn show_options(&mut self, ui: &mut egui::Ui, targets: &[&'static Format]) {
        card(
            ui,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
            |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Dönüşüm").strong());
                ui.add_space(6.0);

                ui.horizontal(|ui| {
                    ui.label("Hedef biçim:");
                    let selected = self
                        .target
                        .as_deref()
                        .map(target_text)
                        .unwrap_or_else(|| "—".to_string());
                    egui::ComboBox::from_id_salt("hedef-bicim")
                        .selected_text(selected)
                        .width(200.0)
                        .show_ui(ui, |ui| {
                            for target in targets {
                                ui.selectable_value(
                                    &mut self.target,
                                    Some(target.id.to_string()),
                                    target_text(target.id),
                                );
                            }
                        });
                    if !targets.is_empty() && self.kind_count() > 1 {
                        ui.label(
                            RichText::new("Yalnızca tüm dosyalara uyan ortak biçimler")
                                .small()
                                .color(MUTED),
                        );
                    }
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
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

                if self.has_kind(MediaKind::Image)
                    || self.has_kind(MediaKind::Audio)
                    || self.has_kind(MediaKind::Video)
                {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(4.0);
                    ui.label(RichText::new("Seçenekler").small().color(MUTED));
                }
                if self.has_kind(MediaKind::Image) {
                    ui.horizontal(|ui| {
                        ui.label("JPEG kalitesi:");
                        ui.add(egui::Slider::new(&mut self.image_quality, 1..=100));
                    });
                }
                if self.has_kind(MediaKind::Audio) || self.has_kind(MediaKind::Video) {
                    ui.horizontal(|ui| {
                        let mut limited = self.bitrate_kbps.is_some();
                        if ui.checkbox(&mut limited, "Bit hızı sınırı").changed() {
                            self.bitrate_kbps = if limited { Some(192) } else { None };
                        }
                        if let Some(rate) = self.bitrate_kbps.as_mut() {
                            ui.add(egui::Slider::new(rate, 32..=20000).suffix(" kbps"));
                        }
                    });
                }
                if self.has_kind(MediaKind::Video) {
                    ui.horizontal(|ui| {
                        ui.label("Ölçek:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.video_scale)
                                .hint_text("örn. 1280:-2 (boş = kaynak)"),
                        );
                    });
                }
            },
        );
    }

    fn show_actions(
        &mut self,
        ui: &mut egui::Ui,
        targets: &[&'static Format],
        tools: &[ToolStatus],
    ) -> bool {
        let mut start = false;
        if targets.is_empty() {
            ui.colored_label(
                WARNING,
                "Bu dosyalar için ortak bir hedef biçim yok. Arşiv dosyalarını tek başına \
                 dönüştürün veya farklı türleri ayrı ayrı ekleyin.",
            );
            ui.add_space(6.0);
        }

        ui.horizontal(|ui| {
            let tool_ready = self.has_required_tool(tools);
            let enabled =
                !self.files.is_empty() && self.target.is_some() && !targets.is_empty() && tool_ready;
            let fill = if enabled { SUCCESS } else { MUTED };
            if ui
                .add_enabled(
                    enabled,
                    egui::Button::new(
                        RichText::new("Dönüştür").size(16.0).strong().color(Color32::WHITE),
                    )
                    .fill(fill)
                    .min_size(egui::vec2(180.0, 40.0)),
                )
                .clicked()
            {
                start = true;
            }
            if !tool_ready {
                ui.colored_label(
                    DANGER,
                    "Gerekli araç kurulu değil. Ayarlar → Harici araçlar bölümünden kurabilirsiniz.",
                );
            }
        });
        start
    }

    fn has_required_tool(&self, tools: &[ToolStatus]) -> bool {
        if self.files.is_empty() {
            return true;
        }
        let target = self.target.as_deref().unwrap_or_default();
        // Packing into an archive (or extracting one) never needs the media
        // engines, even when the queued files are audio or video.
        let target_is_archive = target == registry::EXTRACT
            || registry::format_by_id(target)
                .map(|format| format.kind == MediaKind::Archive)
                .unwrap_or(false);
        if target_is_archive {
            return true;
        }
        self.files.iter().all(|file| match file.format.kind {
            MediaKind::Audio | MediaKind::Video => tool_available(tools, Tool::Ffmpeg),
            MediaKind::Document => match document::strategy(file.format.id, target) {
                document::Strategy::Encoding => true,
                document::Strategy::LibreOffice => tool_available(tools, Tool::LibreOffice),
                document::Strategy::Pandoc => tool_available(tools, Tool::Pandoc),
            },
            _ => true,
        })
    }

    fn pick_files(&mut self) {
        let extensions = [
            "jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff", "gif", "ico", "mp3", "wav", "flac",
            "ogg", "opus", "aac", "m4a", "mp4", "mkv", "webm", "avi", "mov", "pdf", "docx", "odt",
            "md", "markdown", "html", "htm", "epub", "txt", "csv", "zip", "tar", "gz", "tgz",
        ];
        let dialog = rfd::FileDialog::new().add_filter("Desteklenen dosyalar", &extensions);
        if let Some(paths) = dialog.pick_files() {
            for path in paths {
                self.add_path(path);
            }
        }
    }
}

/// Card container used across the screen for a consistent look.
fn card(ui: &mut egui::Ui, stroke: Color32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::default()
        .fill(ui.visuals().faint_bg_color)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(12)
        .inner_margin(egui::Margin::same(14))
        .show(ui, add);
}

fn kind_badge(ui: &mut egui::Ui, kind: MediaKind) {
    egui::Frame::default()
        .fill(kind_color(kind))
        .corner_radius(5)
        .inner_margin(egui::Margin::symmetric(7, 2))
        .show(ui, |ui| {
            ui.label(
                RichText::new(kind.label())
                    .small()
                    .strong()
                    .color(Color32::WHITE),
            );
        });
}

pub fn kind_color(kind: MediaKind) -> Color32 {
    match kind {
        MediaKind::Image => Color32::from_rgb(64, 150, 205),
        MediaKind::Audio => Color32::from_rgb(150, 112, 200),
        MediaKind::Video => Color32::from_rgb(205, 100, 150),
        MediaKind::Document => Color32::from_rgb(205, 145, 60),
        MediaKind::Archive => Color32::from_rgb(92, 160, 100),
    }
}

fn display_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

fn target_text(id: &str) -> String {
    if id == registry::EXTRACT {
        "Arşivi ayıkla".to_string()
    } else if let Some(format) = registry::format_by_id(id) {
        format!("{} (.{})", format.label, format.id)
    } else {
        id.to_string()
    }
}

fn target_allowed(tools: &[ToolStatus], source: &Format, target: &Format) -> bool {
    if target.kind == MediaKind::Archive {
        return true;
    }
    if matches!(source.kind, MediaKind::Audio | MediaKind::Video) {
        // Some distributions ship an FFmpeg build without certain encoders.
        return crate::converters::av::target_supported(target.id);
    }
    if source.kind != MediaKind::Document {
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
    use crate::core::detector::Tool;

    #[test]
    fn human_size_formats() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1024), "1.0 KiB");
        assert_eq!(human_size(1536), "1.5 KiB");
    }

    fn all_installed() -> Vec<ToolStatus> {
        Tool::ALL
            .into_iter()
            .map(|tool| ToolStatus {
                tool,
                path: Some(PathBuf::from("/usr/bin")).map(|p| p.join(tool.id())),
            })
            .collect()
    }

    #[test]
    fn renders_empty_and_populated_states() {
        let tools = all_installed();
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("a.png");
        image::RgbaImage::new(2, 2).save(&image).unwrap();

        let mut view = ConvertView::new(90, None, None);
        egui::__run_test_ui(|ui| {
            assert!(!view.show(ui, &tools));
        });

        assert!(view.add_path(image));
        egui::__run_test_ui(|ui| {
            assert!(!view.show(ui, &tools));
        });
        assert_eq!(view.files.len(), 1);
        assert!(view.target.is_some());
    }

    #[test]
    fn mixes_kinds_on_one_screen() {
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("a.png");
        image::RgbaImage::new(2, 2).save(&image).unwrap();
        let audio = dir.path().join("a.mp3");
        std::fs::write(&audio, b"ID3").unwrap();

        let mut view = ConvertView::new(90, None, None);
        assert!(view.add_path(image));
        assert!(view.add_path(audio), "ses dosyası da aynı listeye girmeli");
        assert_eq!(view.files.len(), 2);
        assert!(view.has_kind(MediaKind::Image));
        assert!(view.has_kind(MediaKind::Audio));
        assert_eq!(view.kind_count(), 2);

        // The only common targets for mixed kinds are archives.
        let targets = view.available_targets(&all_installed());
        assert!(targets.iter().all(|f| f.kind == MediaKind::Archive));
        assert!(targets.iter().any(|f| f.id == "zip"));
    }

    #[test]
    fn common_targets_exclude_formats_missing_from_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        image::RgbaImage::new(2, 2).save(&png).unwrap();
        let jpg = dir.path().join("b.jpg");
        std::fs::copy(&png, &jpg).unwrap();

        let mut view = ConvertView::new(90, None, None);
        assert!(view.add_path(png));
        assert!(view.add_path(jpg));

        let targets = view.available_targets(&all_installed());
        // PNG cannot stay PNG, JPG cannot stay JPG: neither is common.
        assert!(!targets.iter().any(|f| f.id == "png"));
        assert!(!targets.iter().any(|f| f.id == "jpg"));
        // Both can become WebP or be packed together.
        assert!(targets.iter().any(|f| f.id == "webp"));
        assert!(targets.iter().any(|f| f.id == "zip"));
    }

    #[test]
    fn rejects_executables_and_unknown_files() {
        let dir = tempfile::tempdir().unwrap();
        let elf = dir.path().join("b.png");
        std::fs::write(&elf, b"\x7fELF").unwrap();
        let unknown = dir.path().join("a.unknown");
        std::fs::write(&unknown, b"x").unwrap();

        let mut view = ConvertView::new(90, None, None);
        assert!(!view.add_path(elf), "ELF içerikli dosya reddedilmeli");
        assert!(!view.add_path(unknown), "bilinmeyen uzantı reddedilmeli");
        assert!(view.files.is_empty());
    }

    #[test]
    fn target_list_requires_installed_engine() {
        let dir = tempfile::tempdir().unwrap();
        let md = dir.path().join("a.md");
        std::fs::write(&md, b"# baslik\n").unwrap();

        let mut view = ConvertView::new(90, None, None);
        assert!(view.add_path(md));

        let none_installed: Vec<ToolStatus> = Tool::ALL
            .into_iter()
            .map(|tool| ToolStatus { tool, path: None })
            .collect();
        // Without LibreOffice/Pandoc only archive packing remains possible.
        let targets = view.available_targets(&none_installed);
        assert!(targets.iter().all(|f| f.kind == MediaKind::Archive));
    }

    #[test]
    fn archive_targets_do_not_require_media_tools() {
        let dir = tempfile::tempdir().unwrap();
        let audio = dir.path().join("a.mp3");
        std::fs::write(&audio, b"ID3").unwrap();

        let mut view = ConvertView::new(90, None, None);
        assert!(view.add_path(audio));

        let none_installed: Vec<ToolStatus> = Tool::ALL
            .into_iter()
            .map(|tool| ToolStatus { tool, path: None })
            .collect();

        // Packing mixed files into ZIP works without FFmpeg.
        view.target = Some("zip".to_string());
        assert!(view.has_required_tool(&none_installed));

        // A real audio target still needs FFmpeg.
        view.target = Some("flac".to_string());
        assert!(!view.has_required_tool(&none_installed));
    }

    #[test]
    fn duplicate_paths_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let txt = dir.path().join("a.txt");
        std::fs::write(&txt, b"metin").unwrap();

        let mut view = ConvertView::new(90, None, None);
        assert!(view.add_path(txt.clone()));
        assert!(view.add_path(txt));
        assert_eq!(view.files.len(), 1);
    }
}
