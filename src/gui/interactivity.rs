use crate::gui::state::MzViewerApp;
use eframe::egui;
use egui_plot::PlotPoint;
use log::{error, info, warn};

/// Handles triple-click events on the chromatogram to extract a mass spectrum.
pub fn handle_chromatogram_click(
    app: &mut MzViewerApp,
    response: egui::Response,
    plot_bounds: Option<egui_plot::PlotBounds>,
) {
    if !response.triple_clicked() {
        return;
    }

    let Some(active_id) = app.active_file_id else {
        warn!("No active file selected for mass spectrum extraction");
        return;
    };

    let rt_clicked = calculate_clicked_rt(app, &response, plot_bounds);

    let Some(file) = app.files.get_mut(&active_id) else {
        warn!("Active file ID {} not found", active_id);
        return;
    };

    info!(
        "Triple click detected on plot at {:?} for file: {}",
        &rt_clicked, file.name
    );

    if let Some(rt) = rt_clicked {
        let maybe_index = file
            .cached_chromatogram
            .as_ref()
            .and_then(|chrom| crate::processing::find_closest_spectrum_index(chrom, rt));

        if let Some(index) = maybe_index {
            info!("Found closest spectrum at index: {}", index);
            match file.data.get_mass_spectrum_by_index(index) {
                Ok(spectrum) => {
                    file.cached_mass_spectrum = Some(spectrum);
                }
                Err(e) => {
                    warn!("Failed to get mass spectrum at index {}: {:?}", index, e);
                }
            }
        } else {
            warn!("No close spectrum found for the clicked retention time");
        }
    } else {
        warn!("No retention time determined from click position");
    }
}

/// Handles right-click drag events for peak integration.
pub fn handle_integration_drag(
    app: &mut MzViewerApp,
    response: &egui::Response,
    pointer_coord: Option<PlotPoint>,
) {
    let rt = pointer_coord.map(|p| p.x);

    if response.drag_started_by(egui::PointerButton::Secondary) {
        app.integration_start_rt = rt;
        app.integration_end_rt = None;
        app.integration_result = None;
        app.integration_start_intensity = None;
        app.integration_end_intensity = None;
        info!("Integration drag started at RT: {:?}", rt);
    } else if response.dragged_by(egui::PointerButton::Secondary) {
        app.integration_end_rt = rt;
    } else if response.drag_stopped_by(egui::PointerButton::Secondary) {
        app.integration_end_rt = rt;
        compute_integration(app);
        info!(
            "Integration drag stopped. Result: {:?}",
            app.integration_result
        );
    }
}

/// Computes the trapezoidal area for the current start/end RT window.
pub fn compute_integration(app: &mut MzViewerApp) {
    let (start, end) = match (app.integration_start_rt, app.integration_end_rt) {
        (Some(s), Some(e)) => (s.min(e), s.max(e)),
        _ => return,
    };
    app.integration_start_rt = Some(start);
    app.integration_end_rt = Some(end);

    let active_id = match app.active_file_id {
        Some(id) => id,
        None => return,
    };

    let area_result = app
        .files
        .get(&active_id)
        .and_then(|f| f.cached_plot_data.as_deref())
        .map(|data| crate::processing::integrate_peak(data, start, end));

    match area_result {
        Some(Ok(area)) => {
            app.integration_result = Some(area);
            // Cache the boundary intensities for the baseline chord rendering.
            if let Some(active_id) = app.active_file_id {
                if let Some(data) = app
                    .files
                    .get(&active_id)
                    .and_then(|f| f.cached_plot_data.as_deref())
                {
                    app.integration_start_intensity =
                        Some(crate::processing::interpolate_at(data, start));
                    app.integration_end_intensity =
                        Some(crate::processing::interpolate_at(data, end));
                }
            }
            info!("Peak area [{:.3}–{:.3} min] = {:.4e}", start, end, area);
        }
        Some(Err(e)) => {
            error!("Integration failed: {}", e);
            app.show_error_dialog(format!("Integration failed: {}", e));
            app.integration_result = None;
        }
        None => {
            warn!("No cached plot data available for integration");
        }
    }
}

/// Converts a screen click position to a retention time in plot coordinates.
pub fn calculate_clicked_rt(
    app: &mut MzViewerApp,
    response: &egui::Response,
    plot_bounds: Option<egui_plot::PlotBounds>,
) -> Option<f32> {
    let plot_position = response.interact_pointer_pos()?;
    let bounds = plot_bounds?;

    let plot_width = response.rect.width();
    let min_x = *bounds.range_x().start();
    let max_x = *bounds.range_x().end();

    let relative_x = (plot_position.x - response.rect.left()) / plot_width;
    let converted_rt = min_x + relative_x as f64 * (max_x - min_x);

    app.user_input.retention_time_ms_spectrum = Some(converted_rt as f32);
    info!("Retention time clicked: {:?}", converted_rt as f32);

    Some(converted_rt as f32)
}
