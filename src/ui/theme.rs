//! Shared visual language for every screen.
//!
//! Colours are defined once here; views use [`card`] and the button helpers so
//! the app keeps a consistent, modern look.

use egui::{Color32, CornerRadius, Margin, Response, RichText, Shadow, Stroke, Ui, Vec2};

pub const BG: Color32 = Color32::from_rgb(12, 15, 21);
pub const BG_SOFT: Color32 = Color32::from_rgb(16, 20, 28);
pub const CARD: Color32 = Color32::from_rgb(23, 27, 36);
pub const CARD_ALT: Color32 = Color32::from_rgb(29, 34, 45);
pub const INPUT: Color32 = Color32::from_rgb(17, 21, 29);
pub const BORDER: Color32 = Color32::from_rgb(42, 49, 63);
pub const BORDER_SOFT: Color32 = Color32::from_rgb(33, 39, 51);
pub const TEXT: Color32 = Color32::from_rgb(233, 237, 245);
pub const MUTED: Color32 = Color32::from_rgb(139, 149, 166);
pub const ACCENT: Color32 = Color32::from_rgb(92, 142, 255);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(35, 48, 79);
pub const SUCCESS: Color32 = Color32::from_rgb(72, 199, 142);
pub const WARNING: Color32 = Color32::from_rgb(240, 178, 71);
pub const DANGER: Color32 = Color32::from_rgb(240, 107, 107);
pub const WHITE: Color32 = Color32::from_rgb(248, 250, 255);

/// Applies the dark theme, spacing and rounded widget style.
pub fn install(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.window_fill = CARD;
        visuals.extreme_bg_color = INPUT;
        visuals.faint_bg_color = CARD_ALT;
        visuals.override_text_color = Some(TEXT);
        visuals.selection.bg_fill = ACCENT.gamma_multiply(0.45);
        visuals.selection.stroke = Stroke::new(1.0, WHITE);
        visuals.hyperlink_color = ACCENT;
        visuals.window_stroke = Stroke::new(1.0, BORDER);
        visuals.window_corner_radius = CornerRadius::same(14);
        visuals.menu_corner_radius = CornerRadius::same(10);

        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        visuals.widgets.inactive.bg_fill = CARD_ALT;
        visuals.widgets.inactive.weak_bg_fill = CARD_ALT;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SOFT);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(37, 44, 58);
        visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(37, 44, 58);
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, WHITE);
        visuals.widgets.active.bg_fill = ACCENT_SOFT;
        visuals.widgets.active.weak_bg_fill = ACCENT_SOFT;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0, WHITE);
        style.visuals = visuals;

        style.spacing.item_spacing = Vec2::new(9.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.spacing.scroll.bar_width = 7.0;
        for widget in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.noninteractive,
        ] {
            widget.corner_radius = CornerRadius::same(8);
        }
    });
}

/// A raised card with the shared border and soft shadow.
pub fn card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::default()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER_SOFT))
        .corner_radius(14)
        .inner_margin(Margin::same(16))
        .shadow(Shadow {
            offset: [0, 3],
            blur: 14,
            spread: 0,
            color: Color32::from_black_alpha(70),
        })
        .show(ui, add);
}

/// A card without a shadow, used inside other cards and for long lists.
pub fn flat_card(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::default()
        .fill(CARD)
        .stroke(Stroke::new(1.0, BORDER_SOFT))
        .corner_radius(14)
        .inner_margin(Margin::same(16))
        .show(ui, add);
}

/// Section label used inside cards.
pub fn section(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).size(15.0).strong());
}

/// Small muted helper text.
pub fn hint(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).small().color(MUTED));
}

/// Large accent-filled call to action.
pub fn primary_button(ui: &mut Ui, label: &str, size: Vec2) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label).size(15.0).strong().color(WHITE))
            .fill(ACCENT)
            .stroke(Stroke::new(1.0, ACCENT.gamma_multiply(1.2)))
            .corner_radius(10)
            .min_size(size),
    )
}

/// Accent-outline button used for secondary actions.
pub fn accent_button(ui: &mut Ui, label: &str, size: Vec2) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label).strong().color(ACCENT))
            .fill(ACCENT_SOFT)
            .stroke(Stroke::new(1.0, ACCENT.gamma_multiply(0.55)))
            .corner_radius(9)
            .min_size(size),
    )
}

/// Paints a soft vertical gradient inside `rect`.
pub fn vertical_gradient(ui: &Ui, rect: egui::Rect, top: Color32, bottom: Color32) {
    use egui::{pos2, Shape};
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(pos2(rect.left(), rect.top()), top);
    mesh.colored_vertex(pos2(rect.right(), rect.top()), top);
    mesh.colored_vertex(pos2(rect.right(), rect.bottom()), bottom);
    mesh.colored_vertex(pos2(rect.left(), rect.bottom()), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(mesh));
}

/// Draws a small filled status dot.
pub fn dot(ui: &mut Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(8.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
}
