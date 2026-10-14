//! Hand-painted controls that replace egui's stock selectable labels and
//! buttons: a segmented control, underline navigation tabs, an accent
//! primary button and a titled card. Colours come from `theme::Tokens`.

use eframe::egui::{
    self, CornerRadius, Response, Sense, Stroke, StrokeKind, TextStyle, WidgetInfo,
    WidgetType,
};

use super::icons::{self, Icon};
use super::theme::Tokens;

fn tokens(ui: &egui::Ui) -> Tokens {
    Tokens::for_mode(ui.visuals().dark_mode)
}

fn hover_amount(ui: &egui::Ui, response: &Response) -> f32 {
    ui.ctx()
        .animate_bool_responsive(response.id.with("hover"), response.hovered() || response.has_focus())
}

/// A rounded track holding `segment`s; the selected one is raised.
pub(super) fn segmented<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let t = tokens(ui);
    egui::Frame::new()
        .fill(t.control)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(6)
        .inner_margin(2)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(add).inner
        })
        .inner
}

/// One option inside `segmented` (or a standalone toggle when `selected` is a flag).
pub(super) fn segment(ui: &mut egui::Ui, selected: bool, icon: Option<Icon>, label: &str) -> Response {
    let t = tokens(ui);
    let font = TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, t.text);
    let icon_size = 14.0;
    let icon_w = if icon.is_some() { icon_size + 6.0 } else { 0.0 };
    let size = egui::vec2(
        galley.size().x + icon_w + 20.0,
        (ui.spacing().interact_size.y - 6.0).max(galley.size().y + 6.0),
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, ui.is_enabled(), selected, label));
    if ui.is_rect_visible(rect) {
        let enabled = ui.is_enabled();
        let hover = hover_amount(ui, &response);
        let radius = CornerRadius::same(4);
        let (fill, stroke, fg) = if selected {
            (t.canvas, Stroke::new(1.0, t.border), t.accent)
        } else {
            (
                t.control_hover.gamma_multiply(hover),
                Stroke::NONE,
                if hover > 0.5 { t.text } else { t.text_dim },
            )
        };
        let fg = if enabled { fg } else { t.text_disabled };
        ui.painter().rect(rect, radius, fill, stroke, StrokeKind::Inside);
        let mut x = rect.min.x + 10.0;
        if let Some(icon) = icon {
            let r = egui::Rect::from_min_size(
                egui::pos2(x, rect.center().y - icon_size / 2.0),
                egui::vec2(icon_size, icon_size),
            );
            icons::paint(ui.painter(), r, icon, fg);
            x += icon_w;
        }
        ui.painter().galley(
            egui::pos2(x, rect.center().y - galley.size().y / 2.0),
            galley,
            fg,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Navigation tab: plain text with an accent underline when selected.
pub(super) fn nav_tab(ui: &mut egui::Ui, selected: bool, label: &str) -> Response {
    let t = tokens(ui);
    let font = TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, t.text);
    let size = egui::vec2(galley.size().x + 20.0, ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::SelectableLabel, ui.is_enabled(), selected, label));
    if ui.is_rect_visible(rect) {
        let hover = hover_amount(ui, &response);
        if !selected && hover > 0.0 {
            ui.painter().rect_filled(
                rect.shrink2(egui::vec2(0.0, 2.0)),
                CornerRadius::same(4),
                t.control_hover.gamma_multiply(hover * 0.7),
            );
        }
        let fg = if !ui.is_enabled() {
            t.text_disabled
        } else if selected {
            t.text
        } else {
            t.text_dim
        };
        ui.painter()
            .galley(egui::pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0), galley, fg);
        if selected {
            let y = rect.max.y - 1.0;
            ui.painter().line_segment(
                [egui::pos2(rect.min.x + 6.0, y), egui::pos2(rect.max.x - 6.0, y)],
                Stroke::new(2.0, t.accent),
            );
        }
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Accent-filled button for the main action of a view.
pub(super) fn primary_button(ui: &mut egui::Ui, label: &str) -> Response {
    let t = tokens(ui);
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.weak_bg_fill = t.accent;
        w.hovered.weak_bg_fill = t.accent_hover;
        w.active.weak_bg_fill = t.accent_hover;
        for state in [&mut w.inactive, &mut w.hovered, &mut w.active] {
            state.bg_stroke = Stroke::NONE;
            state.fg_stroke = Stroke::new(1.0, t.on_accent);
        }
        let min = egui::vec2(0.0, ui.spacing().interact_size.y);
        ui.add(egui::Button::new(egui::RichText::new(label).color(t.on_accent)).min_size(min))
    })
    .inner
}

/// A bordered surface with a small title, for grouping related controls.
pub(super) fn card<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let t = tokens(ui);
    egui::Frame::new()
        .fill(t.canvas)
        .stroke(Stroke::new(1.0, t.border))
        .corner_radius(6)
        .inner_margin(10)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(egui::RichText::new(title).font(egui::FontId::new(
                14.0,
                egui::FontFamily::Name("Inter Medium".into()),
            )));
            ui.add_space(2.0);
            add(ui)
        })
        .inner
}

/// Drop-in for `ui.selectable_value` that renders as a navigation tab.
pub(super) fn nav_value<T: PartialEq>(ui: &mut egui::Ui, current: &mut T, value: T, label: &str) -> Response {
    let mut response = nav_tab(ui, *current == value, label);
    if response.clicked() && *current != value {
        *current = value;
        response.mark_changed();
    }
    response
}
