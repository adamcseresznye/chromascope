use crate::gui::state::MzViewerApp;
use eframe::egui;
use egui_plot::PlotPoint;
use log::{error, info, warn};

/// Handles double-click events on the chromatogram to extract a mass spectrum.
pub fn handle_chromatogram_click(
    app: &mut MzViewerApp,
    response: egui::Response,
    pointer_coord: Option<PlotPoint>,
) {
    if !response.double_clicked() {
        return;
    }

    let Some(active_id) = app.active_file_id else {
        warn!("No active file selected for mass spectrum extraction");
        return;
    };

    let rt_clicked = pointer_coord.map(|p| p.x as f32);

    let Some(file) = app.files.get_mut(&active_id) else {
        warn!("Active file ID {} not found", active_id);
        return;
    };

    info!(
        "Double click detected on plot at {:?} for file: {}",
        rt_clicked, file.name
    );

    // Stored SRM/MRM traces have no backing spectra to inspect.
    if file.data.available_scan_filters.is_empty() && file.data.has_native_chromatograms() {
        warn!(
            "No spectra in {} — double-click spectrum lookup is unavailable for stored traces",
            file.name
        );
        return;
    }

    if let Some(rt) = rt_clicked {
        let maybe_index = file
            .cache
            .chromatogram
            .as_ref()
            .and_then(|chrom| crate::processing::find_closest_spectrum_index(chrom, rt));

        if let Some(index) = maybe_index {
            info!("Found closest spectrum at index: {}", index);
            match file.data.get_mass_spectrum_by_index(index) {
                Ok(spectrum) => {
                    app.user_input.retention_time_ms_spectrum = Some(spectrum.retention_time);
                    file.cache.mass_spectrum = Some(spectrum);
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
        app.integration.start_rt = rt;
        app.integration.end_rt = None;
        app.integration.result = None;
        app.integration.start_intensity = None;
        app.integration.end_intensity = None;
        info!("Integration drag started at RT: {:?}", rt);
    } else if response.dragged_by(egui::PointerButton::Secondary) {
        app.integration.end_rt = rt;
    } else if response.drag_stopped_by(egui::PointerButton::Secondary) {
        app.integration.end_rt = rt;
        compute_integration(app);
        info!(
            "Integration drag stopped. Result: {:?}",
            app.integration.result
        );
    }
}

/// Clamps an RT window to the first/last point of the trace.
///
/// Outside the data `integrate_peak` would extend the edge intensity flat, which makes the area
/// depend on how far the view overhangs the trace. The engine rejects such windows; the GUI
/// narrows them instead, so "Integrate visible RT range" and edge drags stay well defined.
/// Returns `None` when the window does not overlap the trace at all.
pub fn clamp_window_to_data(data: &[[f64; 2]], start: f64, end: f64) -> Option<(f64, f64)> {
    let first = data.first()?[0];
    let last = data.last()?[0];
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    let (start, end) = (start.max(first), end.min(last));
    (start < end).then_some((start, end))
}

/// Computes the trapezoidal area for the current start/end RT window.
///
/// The window is clamped to the trace first, and the stored start/end are the clamped values.
pub fn compute_integration(app: &mut MzViewerApp) {
    let (start, end) = match (app.integration.start_rt, app.integration.end_rt) {
        (Some(s), Some(e)) => (s.min(e), s.max(e)),
        _ => return,
    };

    let active_id = match app.active_file_id {
        Some(id) => id,
        None => return,
    };

    let Some(data) = app
        .files
        .get(&active_id)
        .and_then(|f| f.cache.plot_data.as_deref())
    else {
        app.integration.start_rt = Some(start);
        app.integration.end_rt = Some(end);
        warn!("No cached plot data available for integration");
        return;
    };

    let Some((start, end)) = clamp_window_to_data(data, start, end) else {
        let (first, last) = (data[0][0], data[data.len() - 1][0]);
        app.integration.result = None;
        app.show_error_dialog(format!(
            "Integration failed: the selected range does not overlap the trace ({first:.3}–{last:.3} min)"
        ));
        return;
    };
    app.integration.start_rt = Some(start);
    app.integration.end_rt = Some(end);

    let area_result = crate::processing::integrate_peak(data, start, end);
    let boundary_intensities = (
        crate::processing::interpolate_at(data, start),
        crate::processing::interpolate_at(data, end),
    );

    match area_result {
        Ok(area) => {
            app.integration.result = Some(area);
            // Cache the boundary intensities for the baseline chord rendering.
            app.integration.start_intensity = Some(boundary_intensities.0);
            app.integration.end_intensity = Some(boundary_intensities.1);
            super::workspace::record(app);
            info!("Peak area [{:.3}–{:.3} min] = {:.4e}", start, end, area);
        }
        Err(e) => {
            error!("Integration failed: {}", e);
            app.show_error_dialog(format!("Integration failed: {}", e));
            app.integration.result = None;
        }
    }
}
