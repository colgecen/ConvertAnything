//! egui views.

pub mod convert_view;
pub mod settings;
pub mod setup_wizard;

/// Top level screens.
///
/// Conversion is no longer split into per-media tabs: a single screen accepts
/// files of any supported kind and converts them together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Convert,
    Settings,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screens_are_distinct() {
        assert_ne!(Screen::Convert, Screen::Settings);
    }
}
