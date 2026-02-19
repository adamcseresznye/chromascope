use crate::gui::state::{MzViewerApp, OpenFile, StateChange};
use eframe::egui;
use egui_plot::{Legend, Line, PlotPoint, PlotPoints, Polygon, VLine};
use log::{debug, error, info, warn};

/// Renders the chromatogram plot widget and returns the response plus coordinate data.
pub fn render_chromatogram(
    app: &MzViewerApp,
    ui: &mut egui::Ui,
) -> (
    egui::Response,
    Option<egui_plot::PlotBounds>,
    Option<PlotPoint>,
) {
    if app.is_processing {
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
            format!("Rt = {:.2} min\nIntensity = {:.0}", value.x, value.y)
        })
        .boxed_zoom_pointer_button(egui::PointerButton::Middle)
        .show(ui, |plot_ui| {
            for file in app.files.values() {
                if file.visible {
                    if let Some(data) = &file.cached_plot_data {
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
pub fn create_line_for_file(app: &MzViewerApp, file: &OpenFile, data: &[[f64; 2]]) -> Line {
    Line::new(PlotPoints::from(data.to_vec()))
        .width(app.user_input.line_width)
        .style(app.user_input.line_type.to_egui())
        .color(file.color.to_egui())
        .name(&file.name)
}

/// Draws the integration region shading and boundary lines on the chromatogram plot.
pub fn render_integration_overlay(app: &MzViewerApp, plot_ui: &mut egui_plot::PlotUi) {
    let start = match app.integration_start_rt {
        Some(s) => s,
        None => return,
    };

    plot_ui.vline(
        VLine::new(start)
            .color(egui::Color32::from_rgb(0, 180, 0))
            .width(2.0)
            .style(egui_plot::LineStyle::Dashed { length: 6.0 })
            .name("Integration start"),
    );

    let end = match app.integration_end_rt {
        Some(e) => e,
        None => return,
    };

    plot_ui.vline(
        VLine::new(end)
            .color(egui::Color32::from_rgb(0, 180, 0))
            .width(2.0)
            .style(egui_plot::LineStyle::Dashed { length: 6.0 })
            .name("Integration end"),
    );

    let active_id = match app.active_file_id {
        Some(id) => id,
        None => return,
    };
    let file = match app.files.get(&active_id) {
        Some(f) => f,
        None => return,
    };
    let data = match &file.cached_plot_data {
        Some(d) => d,
        None => return,
    };

    let (s, e) = if start < end {
        (start, end)
    } else {
        (end, start)
    };

    let region: Vec<[f64; 2]> = data
        .iter()
        .filter(|p| p[0] >= s && p[0] <= e)
        .copied()
        .collect();

    if region.len() >= 2 {
        let i_start = region.first().unwrap()[1];
        let i_end = region.last().unwrap()[1];

        let mut poly = region.clone();
        poly.push([e, i_end]);
        poly.push([s, i_start]);

        plot_ui.polygon(
            Polygon::new(PlotPoints::from(poly))
                .fill_color(egui::Color32::from_rgba_unmultiplied(60, 200, 60, 80))
                .name("Integration region"),
        );
    }
}

/// Orchestrates chromatogram display: update → render → handle interactions.
pub fn plot_chromatogram(
    app: &mut MzViewerApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
) -> egui::Response {
    #[cfg(not(target_arch = "wasm32"))]
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

        #[cfg(not(target_arch = "wasm32"))]
        {
            if !app.is_processing {
                app.request_chromatogram_update();
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            if let Err(e) = app.update_chromatogram_data() {
                error!("Failed to process chromatogram: {}", e);
                app.show_error_dialog(format!("Failed to process chromatogram: {}", e));
            }
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
            if let Some(spectrum) = &file.cached_mass_spectrum {
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
                            format!("m/z = {:.4}\nIntensity = {:.0}", value.x, value.y)
                        } else {
                            format!("{}\nIntensity = {:.0}", name, value.y)
                        }
                    })
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

                        plot_ui.bar_chart(egui_plot::BarChart::new(adjusted_bars));
                    })
                    .response;
                return response;
            }
        }
    }

    warn!("No mass spectrum data available or no active file selected");
    ui.label("No mass spectrum data available")
}
