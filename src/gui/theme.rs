//! Central colour tokens and the egui `Visuals` built from them.
//!
//! Widgets read colours from `Tokens` instead of hard-coding them, and every
//! widget state (inactive, hovered, active, open) gets an explicit fill and
//! stroke so the app no longer shows egui's default look.

use eframe::egui::{self, epaint::Shadow, Color32, CornerRadius, Stroke, Visuals};

#[derive(Clone, Copy)]
pub(super) struct Tokens {
    /// Window/panel background.
    pub panel: Color32,
    /// Plot and text-input background (lowest surface).
    pub canvas: Color32,
    /// Controls at rest.
    pub control: Color32,
    pub control_hover: Color32,
    pub control_active: Color32,
    /// Menus and popups.
    pub menu: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_disabled: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    /// Text on a filled accent control (kept at >= 4.5:1).
    pub on_accent: Color32,
    pub accent_soft: Color32,
    pub warn: Color32,
    pub error: Color32,
    pub good: Color32,
    pub info: Color32,
    pub role_standard: Color32,
    pub role_qc: Color32,
    pub role_blank: Color32,
    pub role_unknown: Color32,
}

impl Tokens {
    pub(super) fn light() -> Self {
        Self {
            panel: Color32::from_rgb(243, 245, 248),
            canvas: Color32::WHITE,
            control: Color32::from_rgb(234, 238, 243),
            control_hover: Color32::from_rgb(223, 229, 237),
            control_active: Color32::from_rgb(208, 216, 227),
            menu: Color32::WHITE,
            border: Color32::from_rgb(220, 226, 233),
            border_strong: Color32::from_rgb(116, 128, 146),
            text: Color32::from_rgb(28, 37, 49),
            text_dim: Color32::from_rgb(75, 87, 104),
            text_disabled: Color32::from_rgb(150, 160, 172),
            accent: Color32::from_rgb(59, 91, 219),
            accent_hover: Color32::from_rgb(45, 74, 194),
            on_accent: Color32::WHITE,
            accent_soft: Color32::from_rgb(224, 231, 252),
            warn: Color32::from_rgb(140, 85, 15),
            error: Color32::from_rgb(148, 34, 34),
            good: Color32::from_rgb(20, 110, 70),
            info: Color32::from_rgb(25, 104, 179),
            role_standard: Color32::from_rgb(68, 120, 200),
            role_qc: Color32::from_rgb(130, 80, 190),
            role_blank: Color32::from_rgb(120, 120, 120),
            role_unknown: Color32::from_rgb(200, 100, 40),
        }
    }

    pub(super) fn dark() -> Self {
        Self {
            panel: Color32::from_rgb(34, 39, 46),
            canvas: Color32::from_rgb(27, 31, 37),
            control: Color32::from_rgb(48, 55, 65),
            control_hover: Color32::from_rgb(58, 66, 78),
            control_active: Color32::from_rgb(70, 80, 94),
            menu: Color32::from_rgb(42, 48, 57),
            border: Color32::from_rgb(58, 66, 78),
            border_strong: Color32::from_rgb(110, 122, 138),
            text: Color32::from_rgb(231, 235, 241),
            text_dim: Color32::from_rgb(184, 196, 210),
            text_disabled: Color32::from_rgb(110, 120, 133),
            accent: Color32::from_rgb(122, 147, 240),
            accent_hover: Color32::from_rgb(146, 168, 246),
            on_accent: Color32::from_rgb(18, 24, 38),
            accent_soft: Color32::from_rgb(44, 55, 92),
            warn: Color32::from_rgb(245, 195, 90),
            error: Color32::from_rgb(255, 150, 150),
            good: Color32::from_rgb(134, 218, 176),
            info: Color32::from_rgb(119, 185, 241),
            role_standard: Color32::from_rgb(90, 150, 230),
            role_qc: Color32::from_rgb(170, 120, 220),
            role_blank: Color32::from_rgb(150, 150, 150),
            role_unknown: Color32::from_rgb(235, 140, 70),
        }
    }

    pub(super) fn of(ui: &eframe::egui::Ui) -> Self {
        Self::for_mode(ui.visuals().dark_mode)
    }

    pub(super) fn for_mode(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }
}

fn shadow(offset_y: i8, blur: u8, alpha: u8) -> Shadow {
    Shadow {
        offset: [0, offset_y],
        blur,
        spread: 0,
        color: Color32::from_black_alpha(alpha),
    }
}

/// Frame for the top toolbar and bottom status bar.
pub(super) fn bar_frame(ctx: &egui::Context, vertical: i8) -> egui::Frame {
    egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(12, vertical))
}

/// Frame for the data, batch and inspector side panels.
pub(super) fn side_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(12, 10))
}

pub(super) fn visuals(dark: bool) -> Visuals {
    let t = Tokens::for_mode(dark);
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    v.dark_mode = dark;
    v.panel_fill = t.panel;
    v.window_fill = t.menu;
    v.extreme_bg_color = t.canvas;
    v.faint_bg_color = t.control;
    v.code_bg_color = t.control;
    v.window_stroke = Stroke::new(1.0_f32, t.border);
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    v.window_shadow = shadow(8, 28, if dark { 110 } else { 40 });
    v.popup_shadow = shadow(4, 16, if dark { 100 } else { 32 });
    v.override_text_color = Some(t.text);
    v.weak_text_color = Some(t.text_dim);
    v.warn_fg_color = t.warn;
    v.error_fg_color = t.error;
    v.hyperlink_color = t.accent;
    v.selection.bg_fill = t.accent_soft;
    v.selection.stroke = Stroke::new(1.0_f32, t.accent);

    let radius = CornerRadius::same(4);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = t.panel;
    w.noninteractive.weak_bg_fill = t.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, t.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, t.text);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = t.control;
    w.inactive.weak_bg_fill = t.control;
    w.inactive.bg_stroke = Stroke::new(1.0_f32, t.border);
    w.inactive.fg_stroke = Stroke::new(1.0_f32, t.text);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = t.control_hover;
    w.hovered.weak_bg_fill = t.control_hover;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, t.border_strong);
    w.hovered.fg_stroke = Stroke::new(1.0_f32, t.text);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;

    w.active.bg_fill = t.control_active;
    w.active.weak_bg_fill = t.control_active;
    w.active.bg_stroke = Stroke::new(1.0_f32, t.accent);
    w.active.fg_stroke = Stroke::new(1.0_f32, t.text);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;

    w.open.bg_fill = t.control_hover;
    w.open.weak_bg_fill = t.control_hover;
    w.open.bg_stroke = Stroke::new(1.0_f32, t.border_strong);
    w.open.fg_stroke = Stroke::new(1.0_f32, t.text);
    w.open.corner_radius = radius;
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::workbench::contrast_ratio;

    #[test]
    fn tokens_meet_contrast_targets() {
        for t in [Tokens::light(), Tokens::dark()] {
            assert!(contrast_ratio(t.on_accent, t.accent) >= 4.5);
            assert!(contrast_ratio(t.on_accent, t.accent_hover) >= 4.5);
            for surface in [t.panel, t.canvas, t.menu] {
                assert!(contrast_ratio(t.text, surface) >= 7.0);
                assert!(contrast_ratio(t.text_dim, surface) >= 4.5);
                for c in [t.good, t.info, t.warn, t.error, t.accent] {
                    assert!(contrast_ratio(c, surface) >= 3.0);
                }
            }
            assert!(contrast_ratio(t.border_strong, t.canvas) >= 3.0);
        }
    }

    #[test]
    fn every_trace_colour_is_visible_on_its_theme() {
        use crate::plotting_parameters::LineColor::*;
        for dark in [false, true] {
            let canvas = Tokens::for_mode(dark).canvas;
            for c in [
                Red, Green, Blue, Yellow, White, Gray, Cyan, Orange, Magenta, Gold,
            ] {
                let ratio = contrast_ratio(c.to_egui_for(dark), canvas);
                assert!(ratio >= 3.0, "{c:?} dark={dark} ratio={ratio:.2}");
            }
        }
    }
}
