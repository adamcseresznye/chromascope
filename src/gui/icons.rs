//! Small vector icons painted with egui's `Painter` on a 16-unit grid, so they
//! scale with the UI, follow the theme colours and need no icon font.

use eframe::egui::{self, epaint::PathStroke, Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, StrokeKind};

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Icon set kept complete for use across panels.
pub(super) enum Icon {
    Chart,
    Grid,
    Overlay,
    Focus,
    Check,
    Warning,
    Chevron,
    Folder,
    Export,
    Info,
    Reset,
    ZoomIn,
    ZoomOut,
    Plus,
    Close,
}

pub(super) fn paint(p: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let k = rect.width().min(rect.height()) / 16.0;
    let pt = |x: f32, y: f32| Pos2::new(rect.min.x + x * k, rect.min.y + y * k);
    let stroke = Stroke::new((1.5 * k).max(1.0), color);
    let line = |pts: &[(f32, f32)]| {
        let points: Vec<Pos2> = pts.iter().map(|&(x, y)| pt(x, y)).collect();
        p.add(Shape::line(points, stroke));
    };
    let boxed = |x0: f32, y0: f32, x1: f32, y1: f32| {
        p.rect_stroke(
            Rect::from_min_max(pt(x0, y0), pt(x1, y1)),
            CornerRadius::same(1),
            stroke,
            StrokeKind::Inside,
        );
    };
    match icon {
        Icon::Chart => {
            line(&[(1.5, 14.0), (14.5, 14.0)]);
            line(&[(1.5, 11.0), (5.0, 6.5), (8.0, 9.5), (14.5, 2.5)]);
        }
        Icon::Grid => {
            boxed(2.0, 2.0, 7.0, 7.0);
            boxed(9.0, 2.0, 14.0, 7.0);
            boxed(2.0, 9.0, 7.0, 14.0);
            boxed(9.0, 9.0, 14.0, 14.0);
        }
        Icon::Overlay => {
            boxed(2.0, 2.0, 10.0, 10.0);
            boxed(6.0, 6.0, 14.0, 14.0);
        }
        Icon::Focus => {
            line(&[(2.0, 6.0), (2.0, 2.0), (6.0, 2.0)]);
            line(&[(10.0, 2.0), (14.0, 2.0), (14.0, 6.0)]);
            line(&[(14.0, 10.0), (14.0, 14.0), (10.0, 14.0)]);
            line(&[(6.0, 14.0), (2.0, 14.0), (2.0, 10.0)]);
        }
        Icon::Check => line(&[(3.0, 8.5), (6.5, 12.0), (13.0, 4.5)]),
        Icon::Warning => {
            p.add(Shape::closed_line(
                vec![pt(8.0, 2.0), pt(14.5, 13.5), pt(1.5, 13.5)],
                PathStroke::new(stroke.width, color),
            ));
            line(&[(8.0, 6.5), (8.0, 9.5)]);
            p.circle_filled(pt(8.0, 11.7), 0.9 * k, color);
        }
        Icon::Chevron => line(&[(4.0, 6.0), (8.0, 10.0), (12.0, 6.0)]),
        Icon::Export => {
            line(&[(8.0, 10.0), (8.0, 2.0)]);
            line(&[(4.5, 5.5), (8.0, 2.0), (11.5, 5.5)]);
            line(&[(2.5, 10.0), (2.5, 13.5), (13.5, 13.5), (13.5, 10.0)]);
        }
        Icon::Info => {
            p.circle_stroke(pt(8.0, 8.0), 6.2 * k, stroke);
            line(&[(8.0, 7.2), (8.0, 11.2)]);
            p.circle_filled(pt(8.0, 4.9), 0.9 * k, color);
        }
        Icon::Reset => {
            let arc: Vec<(f32, f32)> = (0..=10)
                .map(|i| {
                    let a = (-40.0_f32 + i as f32 * 28.0).to_radians();
                    (8.0 + 5.5 * a.cos(), 8.0 + 5.5 * a.sin())
                })
                .collect();
            line(&arc);
            line(&[(2.2, 2.8), (5.4, 3.2), (4.4, 6.2)]);
        }
        Icon::ZoomIn | Icon::ZoomOut => {
            p.circle_stroke(pt(7.0, 7.0), 4.8 * k, stroke);
            line(&[(10.6, 10.6), (14.0, 14.0)]);
            line(&[(4.8, 7.0), (9.2, 7.0)]);
            if icon == Icon::ZoomIn {
                line(&[(7.0, 4.8), (7.0, 9.2)]);
            }
        }
        Icon::Plus => {
            line(&[(8.0, 3.0), (8.0, 13.0)]);
            line(&[(3.0, 8.0), (13.0, 8.0)]);
        }
        Icon::Close => {
            line(&[(4.0, 4.0), (12.0, 12.0)]);
            line(&[(12.0, 4.0), (4.0, 12.0)]);
        }
        Icon::Folder => {
            line(&[(1.5, 4.0), (1.5, 13.0), (14.5, 13.0), (14.5, 5.5), (7.5, 5.5), (6.5, 3.5), (1.5, 3.5), (1.5, 4.0)]);
        }
    }
}

/// Reserve a square of `size` points and paint `icon` into it.
#[allow(dead_code)]
pub(super) fn show(ui: &mut egui::Ui, icon: Icon, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    paint(ui.painter(), rect, icon, color);
}
