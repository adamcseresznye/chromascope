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
    let mut memory = PlotMemory::load(ui.ctx(), id);
    ui.push_id(id, |ui| {
        ui.horizontal_wrapped(|ui| {
            reset = ui.small_button("Reset view").clicked();
            let zoom_in = ui.small_button("Zoom +").clicked();
            let zoom_out = ui.small_button("Zoom −").clicked();
            let left = ui.small_button("Pan ←").clicked();
            let right = ui.small_button("Pan →").clicked();
            ui.checkbox(&mut legend, "Legend");
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
                if ui.small_button("Copy view bounds").clicked() {
                    ui.ctx().copy_text(format!(
                        "x [{}, {}]; y [{}, {}]",
                        min[0], max[0], min[1], max[1]
                    ));
                }
            }
        });
    });
    if let Some(memory) = memory {
        memory.store(ui.ctx(), id);
    }
    ui.ctx()
        .data_mut(|d| d.insert_temp(id.with("legend"), legend));
    let mut plot = Plot::new(id)
        .id(id)
        .show_grid([true, true])
        .boxed_zoom_pointer_button(egui::PointerButton::Middle);
    if legend {
        plot = plot.legend(egui_plot::Legend::default());
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
