use crate::gui::state::{MzViewerApp, OpenFile, StateChange};
use eframe::egui;
use egui_plot::{Legend, Line, PlotPoint, PlotPoints, Polygon, VLine};
use log::{debug, info, warn};

/// Renders the chromatogram plot widget and returns the response plus coordinate data.
pub fn render_chromatogram(
    app: &MzViewerApp,
    ui: &mut egui::Ui,
) -> (
    egui::Response,
    Option<egui_plot::PlotBounds>,
    Option<PlotPoint>,
) {
    if app.async_state.is_processing {
        let response = ui
            .centered_and_justified(|ui| {
                ui.spinner();
                ui.label("Loading chromatogram…");
            })
            .response;
        return (response, None, None);
    }

    let mut plot_bounds = None;
    let mut pointer_coord: Option<PlotPoint> = None;

    let response = egui_plot::Plot::new("chromatogram")
        .width(ui.available_width() * 0.99)
        .height(ui.available_height() * 0.6)
        .legend(Legend::default())
        .label_formatter(|_name, value| {
            format!("Rt = {:.2} min\nIntensity = {:.2e}", value.x, value.y) // ← CHANGED
        })
        .y_axis_formatter(format_intensity_axis) // ← ADDED
        .boxed_zoom_pointer_button(egui::PointerButton::Middle)
        .show(ui, |plot_ui| {
            for file in app.files.values() {
                if file.display.visible {
                    if let Some(data) = &file.cache.plot_data {
                        let line = create_line_for_file(app, file, data);
                        plot_ui.line(line);
                    }
                }
            }

            if app.files.is_empty() {
                warn!("No files opened");
            }

            render_integration_overlay(app, plot_ui);

            plot_bounds = Some(plot_ui.plot_bounds());
            pointer_coord = plot_ui.pointer_coordinate();
        })
        .response;

    (response, plot_bounds, pointer_coord)
}

/// Creates a styled Line widget for a file's chromatogram data.
pub fn create_line_for_file(
    app: &MzViewerApp,
    file: &OpenFile,
    data: &[[f64; 2]],
) -> Line<'static> {
    Line::new(file.name.clone(), PlotPoints::from(data.to_vec()))
        .width(app.user_input.line_width)
        .style(app.user_input.line_type.to_egui())
        .color(file.display.color.to_egui())
}

/// Draws the integration region shading and boundary lines on the chromatogram plot.
pub fn render_integration_overlay(app: &MzViewerApp, plot_ui: &mut egui_plot::PlotUi) {
    let start = match app.integration.start_rt {
        Some(s) => s,
        None => return,
    };

    plot_ui.vline(
        VLine::new("Integration start", start)
            .color(egui::Color32::from_rgb(0, 180, 0))
            .width(2.0)
            .style(egui_plot::LineStyle::Dashed { length: 6.0 }),
    );

    let end = match app.integration.end_rt {
        Some(e) => e,
        None => return,
    };

    plot_ui.vline(
        VLine::new("Integration end", end)
            .color(egui::Color32::from_rgb(0, 180, 0))
            .width(2.0)
            .style(egui_plot::LineStyle::Dashed { length: 6.0 }),
    );

    let active_id = match app.active_file_id {
        Some(id) => id,
        None => return,
    };
    let file = match app.files.get(&active_id) {
        Some(f) => f,
        None => return,
    };
    let data = match &file.cache.plot_data {
        Some(d) => d,
        None => return,
    };

    let (s, e) = if start < end {
        (start, end)
    } else {
        (end, start)
    };

    // Use cached interpolated boundary intensities when available; else fall back to
    // the nearest data-point intensities so the overlay renders during a drag.
    let i_start = app
        .integration
        .start_intensity
        .unwrap_or_else(|| interpolate_in_slice(data, s));
    let i_end = app
        .integration
        .end_intensity
        .unwrap_or_else(|| interpolate_in_slice(data, e));

    // Top edge: left interpolated boundary → interior curve points → right interpolated boundary.
    let mut top: Vec<[f64; 2]> = Vec::new();
    top.push([s, i_start]);
    top.extend(data.iter().filter(|p| p[0] > s && p[0] < e).copied());
    top.push([e, i_end]);

    if top.len() < 2 {
        return;
    }

    // Shade the area between the curve and the chord by emitting one convex
    // Polygon quad per segment.  Each individual trapezoid (two adjacent curve
    // points on top, chord values at the same x on the bottom) is always
    // convex, so egui_plot::Polygon fills it correctly regardless of the
    // overall peak shape.
    let fill_color = egui::Color32::from_rgba_unmultiplied(60, 200, 60, 80);
    for i in 0..top.len().saturating_sub(1) {
        let x0 = top[i][0];
        let x1 = top[i + 1][0];
        let quad = vec![
            top[i],
            top[i + 1],
            [x1, chord_y(x1, s, i_start, e, i_end)],
            [x0, chord_y(x0, s, i_start, e, i_end)],
        ];
        plot_ui.polygon(
            Polygon::new("", PlotPoints::from(quad))
                .fill_color(fill_color)
                .stroke(egui::Stroke::NONE),
        );
    }

    // Draw the baseline chord as a dashed yellow line so the user can see
    // exactly where the integration baseline sits.
    let baseline = egui_plot::Line::new("Baseline chord", vec![[s, i_start], [e, i_end]])
        .color(egui::Color32::YELLOW)
        .width(1.5)
        .style(egui_plot::LineStyle::Dashed { length: 6.0 });
    plot_ui.line(baseline);
}

/// Linearly interpolates the y-value on the chord connecting `(x0, y0)` to
/// `(x1, y1)` at the given `x`.
fn chord_y(x: f64, x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
    let dx = x1 - x0;
    if dx.abs() < f64::EPSILON {
        return y0;
    }
    y0 + (x - x0) / dx * (y1 - y0)
}

/// Returns the linearly interpolated intensity at `rt` from `data` without
/// caching — used as a fallback during live dragging before the integration
/// result has been committed.
fn interpolate_in_slice(data: &[[f64; 2]], rt: f64) -> f64 {
    crate::processing::interpolate_at(data, rt)
}

/// Orchestrates chromatogram display: update → render → handle interactions.
pub fn plot_chromatogram(
    app: &mut MzViewerApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
) -> egui::Response {
    app.poll_processing_result(ctx);

    if app.state_changed == StateChange::Changed && app.active_file_id.is_some() {
        if let Some(active_id) = app.active_file_id {
            if let Some(file) = app.files.get(&active_id) {
                info!(
                    "State has changed, reprocessing plot data for active file: {} (ID: {})",
                    file.name, active_id
                );
            }
        }

        if !app.async_state.is_processing {
            app.request_chromatogram_update();
        }

        app.state_changed = StateChange::Unchanged;
    }

    let (response, plot_bounds, pointer_coord) = render_chromatogram(app, ui);

    super::interactivity::handle_chromatogram_click(app, response.clone(), plot_bounds);
    super::interactivity::handle_integration_drag(app, &response, pointer_coord);

    response
}

/// Plots the mass spectrum bar chart for the active file's cached spectrum.
pub fn plot_mass_spectrum(app: &mut MzViewerApp, ui: &mut egui::Ui) -> egui::Response {
    if let Some(active_id) = app.active_file_id {
        if let Some(file) = app.files.get_mut(&active_id) {
            if let Some(spectrum) = &file.cache.mass_spectrum {
                let mz = spectrum.mz.clone();
                let intensity = spectrum.intensity.clone();
                info!(
                    "Mass spectrum data available for {} (ID: {}). Plotting the spectrum.",
                    file.name, active_id
                );

                let line_color = app.user_input.line_color;
                let response = egui_plot::Plot::new("mass_spectrum")
                    .width(ui.available_width() * 0.99)
                    .height(ui.available_height())
                    .label_formatter(|name, value| {
                        if name.is_empty() {
                            format!("m/z = {:.4}\nIntensity = {:.2e}", value.x, value.y)
                        // ← CHANGED
                        } else {
                            format!("{}\nIntensity = {:.2e}", name, value.y) // ← CHANGED
                        }
                    })
                    .y_axis_formatter(format_intensity_axis) // ← ADDED
                    .show(ui, |plot_ui| {
                        let bounds = plot_ui.plot_bounds();
                        let zoom_level = (bounds.max()[0] - bounds.min()[0]).abs();
                        debug!("Zoom level calculated: {}", zoom_level);

                        let bar_width = zoom_level * 0.001;
                        let adjusted_bars: Vec<egui_plot::Bar> = mz
                            .iter()
                            .zip(intensity.iter())
                            .map(|(&m, &i)| {
                                egui_plot::Bar::new(m, i.into())
                                    .width(bar_width)
                                    .fill(line_color.to_egui())
                                    .name(format!("m/z = {:.4}", m))
                            })
                            .collect();

                        plot_ui.bar_chart(egui_plot::BarChart::new("Mass Spectrum", adjusted_bars));
                    })
                    .response;
                return response;
            }
        }
    }

    warn!("No mass spectrum data available or no active file selected");
    ui.label("No mass spectrum data available")
}

/// Formats intensity axis labels in scientific notation for better readability at high zoom levels.
pub fn format_intensity_axis(
    mark: egui_plot::GridMark,
    _range: &std::ops::RangeInclusive<f64>,
) -> String {
    let v = mark.value;
    if v == 0.0 {
        return "0".to_string();
    }
    let exp = v.abs().log10().floor() as i32;
    let mantissa = v / 10f64.powi(exp);
    if (mantissa - mantissa.round()).abs() < 0.05 {
        format!("{:.2}e{}", mantissa, exp)
    } else {
        format!("{:.3}e{}", mantissa, exp)
    }
}
