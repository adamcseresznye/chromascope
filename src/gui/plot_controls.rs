//! Consistent keyboard-operable plot navigation, preserving domain gestures.
use eframe::egui;
use egui_plot::{Plot, PlotBounds, PlotMemory};
type NamedSeries = (String, Vec<[f64; 2]>);
type ExportCompletion = std::sync::mpsc::Receiver<Result<(), String>>;
#[derive(Clone)]
struct ExportJob(std::sync::Arc<std::sync::Mutex<ExportCompletion>>);
pub(super) fn plot(ui: &mut egui::Ui, source: impl std::hash::Hash) -> Plot<'static> {
    let id = ui.make_persistent_id(source);
    let mut reset = false;
    let mut legend = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(id.with("legend")))
        .unwrap_or(true);
    // Scientists can move the legend when it covers peaks or annotations.
    // 0 = top right (default), 1 = top left, 2 = bottom left, 3 = bottom right.
    let mut corner = ui
        .ctx()
        .data(|d| d.get_temp::<u8>(id.with("legend_corner")))
        .unwrap_or(0);
    let mut memory = PlotMemory::load(ui.ctx(), id);
    let (mut zoom_in, mut zoom_out, mut left, mut right) = (false, false, false, false);
    ui.push_id(id, |ui| {
        // A single non-wrapping row: secondary actions live in the overflow
        // menu so controls never wrap onto axes, peaks, or annotations at
        // narrow widths. The row scrolls horizontally instead of wrapping.
        egui::ScrollArea::horizontal()
            .id_salt(id.with("toolbar_scroll"))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    reset = ui
                        .small_button("Reset view")
                        .on_hover_text("Restore the automatic view (keyboard: Tab to focus, Enter to activate)")
                        .clicked();
                    zoom_in = ui
                        .small_button("Zoom +")
                        .on_hover_text("Zoom in on retention time")
                        .clicked();
                    zoom_out = ui
                        .small_button("Zoom −")
                        .on_hover_text("Zoom out on retention time")
                        .clicked();
                    ui.checkbox(&mut legend, "Legend")
                        .on_hover_text("Show or hide the trace legend");
                    ui.menu_button("More", |ui| {
                        left = ui
                            .small_button("Pan ←")
                            .on_hover_text("Pan earlier in retention time")
                            .clicked();
                        right = ui
                            .small_button("Pan →")
                            .on_hover_text("Pan later in retention time")
                            .clicked();
                        if memory.is_some()
                            && ui
                                .small_button("Copy view bounds")
                                .on_hover_text("Copy the current axis bounds to the clipboard")
                                .clicked()
                        {
                            if let Some(memory) = &memory {
                                let bounds = *memory.bounds();
                                let min = bounds.min();
                                let max = bounds.max();
                                ui.ctx().copy_text(format!(
                                    "x [{}, {}]; y [{}, {}]",
                                    min[0], max[0], min[1], max[1]
                                ));
                            }
                            ui.close();
                        }
                        ui.separator();
                        ui.label("Legend position");
                        for (index, name) in [
                            "Top right",
                            "Top left",
                            "Bottom left",
                            "Bottom right",
                        ]
                        .iter()
                        .enumerate()
                        {
                            ui.radio_value(&mut corner, index as u8, *name);
                        }
                        ui.small("Move the legend when it covers peaks or annotations.");
                    });
                });
            });
        if let Some(memory) = &mut memory {
            let bounds = *memory.bounds();
            let min = bounds.min();
            let max = bounds.max();
            let factor = if zoom_in {
                0.8
            } else if zoom_out {
                1.25
            } else {
                1.
            };
            let pan = if left {
                -0.2
            } else if right {
                0.2
            } else {
                0.
            };
            if zoom_in || zoom_out || left || right {
                let center = [
                    (min[0] + max[0]) / 2. + pan * (max[0] - min[0]),
                    (min[1] + max[1]) / 2.,
                ];
                let half = [
                    (max[0] - min[0]) * factor / 2.,
                    (max[1] - min[1]) * factor / 2.,
                ];
                memory.set_bounds(PlotBounds::from_min_max(
                    [center[0] - half[0], center[1] - half[1]],
                    [center[0] + half[0], center[1] + half[1]],
                ));
                memory.auto_bounds = egui::Vec2b::FALSE;
            }
        }
    });
    if let Some(memory) = memory {
        memory.store(ui.ctx(), id);
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(id.with("legend"), legend));
    ui.ctx()
        .data_mut(|d| d.insert_temp(id.with("legend_corner"), corner));
    let mut plot = Plot::new(id)
        .id(id)
        .show_grid([true, true])
        .boxed_zoom_pointer_button(egui::PointerButton::Middle);
    if legend {
        // Small legend text keeps peak tops and integration boundaries
        // readable at narrow widths in both themes.
        plot = plot.legend(
            egui_plot::Legend::default()
                .position(match corner {
                    1 => egui_plot::Corner::LeftTop,
                    2 => egui_plot::Corner::LeftBottom,
                    3 => egui_plot::Corner::RightBottom,
                    _ => egui_plot::Corner::RightTop,
                })
                .text_style(egui::TextStyle::Small),
        );
    }
    if reset {
        plot = plot.reset();
    }
    plot
}
pub(super) fn export(ui: &mut egui::Ui, points: &[[f64; 2]], x: &str, y: &str) {
    export_kind(ui, points, x, y, 0, None);
}
pub(super) fn export_scatter(ui: &mut egui::Ui, points: &[[f64; 2]], x: &str, y: &str) {
    export_kind(ui, points, x, y, 1, None);
}
pub(super) fn export_spectrum(ui: &mut egui::Ui, points: &[[f64; 2]], y: &str) {
    export_kind(ui, points, "m/z (Th)", y, 2, None);
}
pub(super) fn export_series(
    ui: &mut egui::Ui,
    series: &[(String, Vec<[f64; 2]>)],
    x: &str,
    y: &str,
) {
    export_kind(ui, &[], x, y, 3, Some(series));
}
fn export_kind(
    ui: &mut egui::Ui,
    points: &[[f64; 2]],
    x: &str,
    y: &str,
    kind: u8,
    series: Option<&[NamedSeries]>,
) {
    let job_id = egui::Id::new("plot_export_job");
    let status_id = egui::Id::new("plot_export_status");
    let job = ui.ctx().data(|d| d.get_temp::<ExportJob>(job_id));
    let mut busy = false;
    if let Some(job) = job {
        match job.0.lock().unwrap().try_recv() {
            Ok(result) => ui.ctx().data_mut(|d| {
                d.remove::<ExportJob>(job_id);
                d.insert_temp(
                    status_id,
                    result
                        .err()
                        .unwrap_or_else(|| "Saved full-resolution SVG".into()),
                );
            }),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => ui.ctx().data_mut(|d| {
                d.remove::<ExportJob>(job_id);
                d.insert_temp(status_id, "SVG export worker disconnected".to_owned());
            }),
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                busy = true;
                ui.ctx().request_repaint();
            }
        }
    }
    if ui
        .add_enabled(!busy, egui::Button::new("Export full-resolution SVG…"))
        .clicked()
    {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("SVG", &["svg"])
            .save_file()
        {
            let points = points.to_vec();
            let series = series.map(<[NamedSeries]>::to_vec);
            let x = x.to_owned();
            let y = y.to_owned();
            let (tx, rx) = std::sync::mpsc::channel();
            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    job_id,
                    ExportJob(std::sync::Arc::new(std::sync::Mutex::new(rx))),
                );
                d.insert_temp(status_id, "Exporting full-resolution SVG…".to_owned());
            });
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                use std::io::Write;
                let config = crate::delivery::Config::default();
                let result = match kind {
                    1 => crate::delivery::scatter_svg(&points, &x, &y, &config),
                    2 => crate::delivery::spectrum_svg(&points, &y, &config),
                    3 => crate::delivery::series_svg(
                        series.as_deref().unwrap_or(&[]),
                        &x,
                        &y,
                        &config,
                    ),
                    _ => crate::delivery::svg(&points, &x, &y, &config),
                }
                .map_err(|e| e.to_string())
                .and_then(|svg| {
                    std::fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(path)
                        .and_then(|mut f| f.write_all(svg.as_bytes()))
                        .map_err(|e| e.to_string())
                });
                let _ = tx.send(result);
                ctx.request_repaint();
            });
        }
    }
    if let Some(status) = ui
        .ctx()
        .data(|d| d.get_temp::<String>(egui::Id::new("plot_export_status")))
    {
        ui.small(status);
    }
}

/// Fixed analytical roles use colors visible against their active plot surface.
pub(super) fn scientific_color(dark: bool, role: usize) -> egui::Color32 {
    let colors = if dark {
        [
            [184, 196, 210],
            [119, 185, 241],
            [245, 195, 90],
            [134, 218, 176],
            [255, 150, 150],
        ]
    } else {
        [
            [75, 87, 104],
            [25, 104, 179],
            [140, 85, 15],
            [20, 110, 70],
            [148, 34, 34],
        ]
    };
    let [r, g, b] = colors[role.min(colors.len() - 1)];
    egui::Color32::from_rgb(r, g, b)
}

#[cfg(test)]
mod toolbar_tests {
    use super::*;

    /// The toolbar keeps one non-wrapping row at narrow widths: essential
    /// controls stay visible while pan/copy/legend-position wait in More.
    #[test]
    fn toolbar_keeps_single_row_with_overflow_at_minimum_width() {
        for dark in [false, true] {
            let ctx = egui::Context::default();
            super::super::workbench::configure(&ctx, dark);
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        // Narrow plot card: side panels open at a 920 px window.
                        egui::vec2(480.0, 400.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let _ = plot(ui, "toolbar_regression");
                    });
                },
            );
            for label in ["Reset view", "Zoom +", "Zoom −", "Legend", "More"] {
                assert!(
                    super::super::test_render::text_center(&output.shapes, label).is_some(),
                    "essential toolbar control {label} must stay visible at minimum width"
                );
            }
            // Secondary actions wait in the overflow menu instead of wrapping
            // onto axes or annotations.
            for label in ["Pan ←", "Copy view bounds", "Legend position"] {
                assert!(
                    super::super::test_render::text_center(&output.shapes, label).is_none(),
                    "secondary control {label} must not crowd the plot row"
                );
            }
        }
    }
}
