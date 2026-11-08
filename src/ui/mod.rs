//! egui views.

pub mod convert_view;
pub mod settings;
pub mod setup_wizard;

use crate::core::registry::MediaKind;

/// Top level tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Image,
    Audio,
    Video,
    Document,
    Archive,
    Settings,
}

impl Tab {
    pub const CONVERT_TABS: [Tab; 5] = [
        Tab::Image,
        Tab::Audio,
        Tab::Video,
        Tab::Document,
        Tab::Archive,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Image => "Görsel",
            Tab::Audio => "Ses",
            Tab::Video => "Video",
            Tab::Document => "Belge",
            Tab::Archive => "Arşiv",
            Tab::Settings => "Ayarlar",
        }
    }

    pub fn kind(self) -> Option<MediaKind> {
        match self {
            Tab::Image => Some(MediaKind::Image),
            Tab::Audio => Some(MediaKind::Audio),
            Tab::Video => Some(MediaKind::Video),
            Tab::Document => Some(MediaKind::Document),
            Tab::Archive => Some(MediaKind::Archive),
            Tab::Settings => None,
        }
    }

    pub fn for_kind(kind: MediaKind) -> Tab {
        match kind {
            MediaKind::Image => Tab::Image,
            MediaKind::Audio => Tab::Audio,
            MediaKind::Video => Tab::Video,
            MediaKind::Document => Tab::Document,
            MediaKind::Archive => Tab::Archive,
        }
    }

    pub fn index(self) -> Option<usize> {
        Tab::CONVERT_TABS.iter().position(|t| *t == self)
    }
}
