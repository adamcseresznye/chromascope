use crate::gui::state::{MzViewerApp, OpenFile};
use eframe::egui;
use egui_plot::{Line, PlotPoint, PlotPoints, Polygon, VLine};
use log::{debug, info, warn};

/// Renders the chromatogram plot widget and returns the response plus coordinate data.
fn render_chromatogram(
    app: &mut MzViewerApp,
    ui: &mut egui::Ui,
    key: Option<&(super::state::FileId, crate::processing::ProcessingParams)>,
    height: f32,
    shared_maximum: Option<f64>,
) -> (
    egui::Response,
    Option<egui_plot::PlotBounds>,
    Option<PlotPoint>,
) {
    let mut plot_bounds = None;
    let mut pointer_coord = None;
    let restore_bounds = app
        .workspace
        .apply_bounds
        .then_some(app.workspace.bounds)
        .flatten();
    let id = key
        .map(|(id, p)| format!("chromatogram_{id}_{p:?}"))
        .unwrap_or_else(|| "chromatogram_overlay".into());
    let mut plot = egui_plot::Plot::new(id)
        .width(ui.available_width())
        .height(height.max(100.0))
        .x_axis_label("Retention time (min)")
        .y_axis_label("Intensity (a.u.)")
        .show_background(false)
        .show_grid([false, true])
        .allow_double_click_reset(false)
        .label_formatter(|name, value| {
            format!("{name}\nRT {:.4} min\nIntensity {:.3e}", value.x, value.y)
        })
        .y_axis_formatter(format_intensity_axis)
        .boxed_zoom_pointer_button(egui::PointerButton::Middle);
    if key.is_some() {
        plot = plot.link_axis("stacked_retention_time", [true, false]);
    }
    if shared_maximum.is_some() {
        plot = plot.allow_zoom([true, false]).allow_drag([true, false]);
    }
    if app.workspace.reset_plots {
        plot = plot.reset();
    }
    let response = plot
        .show(ui, |plot_ui| {
            if let Some([min, max]) = restore_bounds {
                if key.is_some() {
                    plot_ui.set_plot_bounds_x(min[0]..=max[0]);
                    plot_ui.set_auto_bounds([false, true]);
                } else {
                    plot_ui.set_plot_bounds(egui_plot::PlotBounds::from_min_max(min, max));
                }
            }
            for (id, params) in super::workspace::trace_keys(app, true) {
                if key.is_some_and(|(file, p)| *file != id || *p != params) {
                    continue;
                }
                let file = &app.files[&id];
                if file.cache.last_processing_params.as_ref() == Some(&params) {
                    if let Some(points) = &file.cache.display_data {
                        plot_ui.line(create_line_for_file(app, file, points));
                    }
                } else if let Some(t) = app
                    .workspace
                    .traces
                    .get(&id)
                    .and_then(|ts| ts.iter().find(|t| t.params == params))
                {
                    plot_ui.line(
                        Line::new(
                            format!("{} · {}", file.name, t.name),
                            t.display_points.clone(),
                        )
                        .color(t.color.to_egui())
                        .width(app.user_input.line_width),
                    );
                }
            }
            if let Some(maximum) = shared_maximum {
                plot_ui.set_plot_bounds_y(0.0..=maximum);
            }
            if key.map_or(true, |(id, p)| {
                app.active_file_id == Some(*id)
                    && app
                        .files
                        .get(id)
                        .and_then(|f| f.cache.last_processing_params.as_ref())
                        == Some(p)
            }) {
                render_integration_overlay(app, plot_ui);
            }
            plot_bounds = Some(plot_ui.plot_bounds());
            pointer_coord = plot_ui.pointer_coordinate();
        })
        .response;
    if let Some(b) = plot_bounds {
        app.workspace.bounds = Some([b.min(), b.max()]);
    }
    (response, plot_bounds, pointer_coord)
}

/// Creates a styled Line widget for a file's chromatogram data.
pub fn create_line_for_file(
    app: &MzViewerApp,
    file: &OpenFile,
    data: &[[f64; 2]],
) -> Line<'static> {
    Line::new(
        format!(
            "{} · {}",
            file.name,
            file.cache
                .last_processing_params
                .as_ref()
                .map(|p| super::workspace::display_name(app, file.id, p))
                .unwrap_or_default()
        ),
        PlotPoints::from(data.to_vec()),
    )
    .width(app.user_input.line_width)
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
    let drawing_data = match &file.cache.display_data {
        Some(d) => d,
        None => return,
    };

    let (s, e) = if start < end {
        (start, end)
    } else {
        (end, start)
    };

    // Interpolate boundaries from full-resolution data, even while dragging.
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
    top.extend(
        drawing_data
            .iter()
            .filter(|p| p[0] > s && p[0] < e)
            .copied(),
    );
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
    app.process_pending_update();
    let available_height = ui.available_height().max(130.0);
    let shared_maximum = super::workspace::shared_intensity_maximum(app);
    let response = ui
        .vertical(|ui| {
            if app.workspace.view.compare_samples {
                comparison_grid(app, ui, shared_maximum, available_height);
            } else if app.workspace.overlay {
                egui::ScrollArea::vertical()
                    .id_salt("overlay_legend")
                    .max_height(100.0)
                    .show(ui, |ui| {
                        for (id, p) in super::workspace::trace_keys(app, false) {
                            if app.files[&id].display.visible {
                                super::workspace::trace_row(app, ui, id, &p, true);
                            }
                        }
                    });
                let (response, bounds, point) = render_chromatogram(
                    app,
                    ui,
                    None,
                    (available_height - ui.min_rect().height()).max(120.0),
                    shared_maximum,
                );
                interactions(app, &response, bounds, point, None);
            } else {
                let keys = super::workspace::trace_keys(app, true);
                let capacity =
                    app.workspace.view.rows.clamp(1, 8) * app.workspace.view.columns.clamp(1, 8);
                let pages = keys.len().div_ceil(capacity).max(1);
                app.workspace.page = app.workspace.page.min(pages - 1);
                if pages > 1 {
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(app.workspace.page > 0, egui::Button::new("Previous page"))
                            .clicked()
                        {
                            app.workspace.page -= 1;
                        }
                        ui.label(format!("Page {} / {pages}", app.workspace.page + 1));
                        if ui
                            .add_enabled(
                                app.workspace.page + 1 < pages,
                                egui::Button::new("Next page"),
                            )
                            .clicked()
                        {
                            app.workspace.page += 1;
                        }
                    });
                }
                if keys.is_empty() {
                    if let Some(error) = app.active_file_id.and_then(|id|app.presets.failed.get(&id)) { ui.colored_label(ui.visuals().error_fg_color,format!("Preset could not be applied to this sample: {error}")); }
                    else if app.presets.specs.is_some() && app.async_state.is_processing { ui.spinner(); ui.label("Preparing this sample's preset analytes…"); }
                    else { ui.weak("No visible traces. Enable a trace in the inspector or extract a new one."); }
                }
                egui::ScrollArea::both()
                    .id_salt("stacked_chromatograms")
                    .max_height((available_height - 32.0).max(110.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let selected = keys
                            .iter()
                            .skip(app.workspace.page * capacity)
                            .take(capacity)
                            .collect::<Vec<_>>();
                        let width = ui.available_width();
                        let columns = app
                            .workspace
                            .view
                            .columns
                            .clamp(1, 8)
                            .min(selected.len().max(1));
                        let card_width = ((width - 8.0 * (columns - 1) as f32) / columns as f32)
                            .clamp(220.0, 760.0);
                        let height = ((available_height - 56.0)
                            / selected.len().div_ceil(columns).max(1) as f32
                            - 48.0)
                            .clamp(115.0, 280.0);
                        for row in selected.chunks(columns) {
                            let group_width = card_width * row.len() as f32
                                + ui.spacing().item_spacing.x * (row.len() - 1) as f32;
                            ui.horizontal(|ui| {
                                ui.add_space(((width - group_width) / 2.0).max(0.0));
                                for key in row {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(card_width, height + 48.0),
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            ui.push_id(
                                                format!("stack_{}_{:?}", key.0, key.1),
                                                |ui| {
                                                    egui::Frame::group(ui.style())
                                                        .inner_margin(8.0)
                                                        .show(ui, |ui| {
                                                            ui.set_width(
                                                                (card_width - 16.0).max(1.0),
                                                            );
                                                            super::workspace::trace_row(
                                                                app, ui, key.0, &key.1, true,
                                                            );
                                                            let (response, bounds, point) =
                                                                render_chromatogram(
                                                                    app,
                                                                    ui,
                                                                    Some(key),
                                                                    height,
                                                                    shared_maximum,
                                                                );
                                                            interactions(
                                                                app,
                                                                &response,
                                                                bounds,
                                                                point,
                                                                Some(key),
                                                            );
                                                        });
                                                },
                                            );
                                        },
                                    );
                                }
                            });
                            ui.add_space(8.0);
                        }
                    });
            }
        })
        .response;
    app.workspace.apply_bounds = false;
    app.workspace.reset_plots = false;
    response
}
fn comparison_grid(
    app: &mut MzViewerApp,
    ui: &mut egui::Ui,
    shared_maximum: Option<f64>,
    available_height: f32,
) {
    let layout = super::workspace::comparison_layout(app);
    if layout.samples.is_empty() || layout.analytes.is_empty() {
        ui.weak("Select visible samples and extract traces to compare them.");
        return;
    }
    let cell_width =
        ((ui.available_width() - 148.0) / layout.samples.len() as f32 - 8.0).clamp(260.0, 760.0);
    let plot_height =
        ((available_height - 60.0) / layout.analytes.len() as f32 - 48.0).clamp(130.0, 280.0);
    egui::ScrollArea::both()
        .id_salt("sample_analyte_matrix")
        .max_height(available_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("comparison_grid")
                .spacing(egui::vec2(8.0, 12.0))
                .show(ui, |ui| {
                    ui.allocate_ui(egui::vec2(140.0, 24.0), |ui| {
                        ui.strong("Analyte / sample");
                    });
                    for id in &layout.samples {
                        ui.allocate_ui(egui::vec2(cell_width, 24.0), |ui| {
                            let file = &app.files[id];
                            ui.add(
                                egui::Label::new(egui::RichText::new(&file.name).strong())
                                    .truncate(),
                            )
                            .on_hover_text(file.cache.source_path.as_deref().unwrap_or(&file.path));
                        });
                    }
                    ui.end_row();
                    for (name, params) in &layout.analytes {
                        ui.allocate_ui(egui::vec2(140.0, plot_height + 44.0), |ui| {
                            ui.add(egui::Label::new(name).wrap());
                        });
                        for id in &layout.samples {
                            ui.allocate_ui_with_layout(
                                egui::vec2(cell_width, plot_height + 44.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.push_id(("compare", *id, format!("{params:?}")), |ui| {
                                        egui::Frame::group(ui.style()).inner_margin(8.0).show(
                                            ui,
                                            |ui| {
                                                ui.set_width(cell_width - 16.0);
                                                ui.set_min_height(plot_height + 28.0);
                                                if layout
                                                    .visible
                                                    .iter()
                                                    .any(|(file, p)| file == id && p == params)
                                                {
                                                    let active = app.active_file_id == Some(*id)
                                                        && app.files[id]
                                                            .cache
                                                            .last_processing_params
                                                            .as_ref()
                                                            == Some(params);
                                                    if ui
                                                        .selectable_label(
                                                            active,
                                                            if active {
                                                                "Active trace"
                                                            } else {
                                                                "Select trace"
                                                            },
                                                        )
                                                        .clicked()
                                                    {
                                                        super::workspace::select_key(
                                                            app, *id, params,
                                                        );
                                                    }
                                                    let key = (*id, params.clone());
                                                    let (response, bounds, point) =
                                                        render_chromatogram(
                                                            app,
                                                            ui,
                                                            Some(&key),
                                                            plot_height,
                                                            shared_maximum,
                                                        );
                                                    interactions(
                                                        app,
                                                        &response,
                                                        bounds,
                                                        point,
                                                        Some(&key),
                                                    );
                                                } else {
                                                    ui.allocate_ui(
                                                        egui::vec2(
                                                            cell_width - 16.0,
                                                            plot_height + 28.0,
                                                        ),
                                                        |ui| {
                                                            ui.weak("No visible trace");
                                                            if let Some(error) =
                                                                app.presets.failed.get(id)
                                                            {
                                                                ui.label(error);
                                                            }
                                                        },
                                                    );
                                                }
                                            },
                                        );
                                    });
                                },
                            );
                        }
                        ui.end_row();
                    }
                });
        });
}
fn interactions(
    app: &mut MzViewerApp,
    response: &egui::Response,
    bounds: Option<egui_plot::PlotBounds>,
    point: Option<PlotPoint>,
    key: Option<&(super::state::FileId, crate::processing::ProcessingParams)>,
) {
    if !app.async_state.is_processing {
        if response.double_clicked()
            || response.drag_started_by(egui::PointerButton::Secondary)
            || response.secondary_clicked()
        {
            if let Some((id, p)) = key {
                super::workspace::select_key(app, *id, p);
            }
        }
        super::interactivity::handle_chromatogram_click(app, response.clone(), point);
        super::interactivity::handle_integration_drag(app, response, point);
    }
    let popup_id = response.id.with("clicked_rt");
    if response.secondary_clicked() {
        response
            .ctx
            .data_mut(|d| d.insert_temp(popup_id, point.map(|p| p.x)));
    }
    response.context_menu(|ui| {
        if ui
            .add_enabled(
                !app.async_state.is_processing,
                egui::Button::new("Inspect spectrum here"),
            )
            .clicked()
        {
            let rt = ui
                .ctx()
                .data(|d| d.get_temp::<Option<f64>>(popup_id))
                .flatten();
            if let (Some(id), Some(rt)) = (app.active_file_id, rt) {
                let scan = app
                    .files
                    .get(&id)
                    .and_then(|f| f.cache.chromatogram.as_ref())
                    .and_then(|c| crate::processing::find_closest_spectrum_index(c, rt as f32));
                if let Some(scan) = scan {
                    super::workspace::load_scan(app, id, scan);
                }
            }
            ui.close();
        }
        if ui.button("Plot properties…").clicked() {
            app.plot_properties_open = true;
            ui.close();
        }
        if ui.button("File information…").clicked() {
            if let Some(id) = key.map(|(id, _)| *id).or(app.active_file_id) {
                ui.ctx()
                    .data_mut(|d| d.insert_temp(egui::Id::new("file_info_open"), id));
            }
            ui.close();
        }
        if ui.button("Load trace preset…").clicked() {
            super::presets::load(app);
            ui.close();
        }
        if ui.button("Reset zoom").clicked() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("reset_chromatograms"), true));
            ui.close();
        }
        if ui
            .add_enabled(
                !app.async_state.is_processing,
                egui::Button::new("Integrate visible RT range"),
            )
            .clicked()
        {
            if let Some(b) = bounds {
                app.integration.start_rt = Some(b.min()[0]);
                app.integration.end_rt = Some(b.max()[0]);
                super::interactivity::compute_integration(app);
            }
            ui.close();
        }
        if ui.button("Clear integration").clicked() {
            app.integration = Default::default();
            ui.close();
        }
        if ui.button("Integration results…").clicked() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("results_open"), true));
            ui.close();
        }
        if ui.button("Export selected trace CSV…").clicked() {
            super::panels::handle_csv_export(app);
            ui.close();
        }
        if ui.button("Export chromatogram figure…").clicked() {
            super::workspace::export_figure(app, false);
            ui.close();
        }
        if let Some((id, p)) = key {
            ui.separator();
            if ui.button("Hide this trace").clicked() {
                if app
                    .files
                    .get(id)
                    .and_then(|f| f.cache.last_processing_params.as_ref())
                    == Some(p)
                {
                    app.workspace.hidden_current.insert(*id);
                    app.integration = Default::default();
                } else if let Some(t) = app
                    .workspace
                    .traces
                    .get_mut(id)
                    .and_then(|ts| ts.iter_mut().find(|t| &t.params == p))
                {
                    t.visible = false;
                }
                ui.close();
            }
            if ui
                .add_enabled(
                    !app.async_state.is_processing,
                    egui::Button::new("Delete this trace"),
                )
                .clicked()
            {
                super::workspace::delete_key(app, *id, p);
                ui.close();
            }
        }
    });
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

                let line_color = file.display.color;
                let restore_bounds = app
                    .workspace
                    .apply_spectrum_bounds
                    .then_some(app.workspace.spectrum_bounds)
                    .flatten();
                app.workspace.apply_spectrum_bounds = false;
                let mut spectrum_bounds = None;
                let response = egui_plot::Plot::new("mass_spectrum")
                    .width(ui.available_width() * 0.99)
                    .height(ui.available_height())
                    .x_axis_label("m/z")
                    .y_axis_label("Intensity (a.u.)")
                    .show_background(false)
                    .show_grid([false, true])
                    .grid_spacing(egui::Rangef::new(12.0, 1800.0))
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
                        if let Some([min, max]) = restore_bounds {
                            plot_ui.set_plot_bounds(egui_plot::PlotBounds::from_min_max(min, max));
                        }
                        let bounds = plot_ui.plot_bounds();
                        spectrum_bounds = Some([bounds.min(), bounds.max()]);
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
                app.workspace.spectrum_bounds = spectrum_bounds;
                return response;
            }
        }
    }

    warn!("No mass spectrum data available or no active file selected");
    ui.centered_and_justified(|ui| {
        ui.weak("Double-click a chromatogram to inspect its mass spectrum.")
    })
    .response
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
