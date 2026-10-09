//! Persistent controls and visual styling for the scientific workbench.
use super::{panels, state::StateChange, MzViewerApp};
use crate::plotting_parameters::{LineColor, PlotType};
use eframe::egui::{self, Color32, Context};

// Shared dimensions keep controls legible at native and scaled display sizes.
const BODY_SIZE: f32 = 14.0;
const SMALL_SIZE: f32 = 12.0;
const CONTROL_HEIGHT: f32 = 28.0;
const PANEL_WIDTH: f32 = 270.0;

/// Workflow launchers stay in one place; analytical windows retain their state.
pub fn navigation(app: &mut MzViewerApp, ui: &mut egui::Ui) {
    let destinations = [
        "Data explorer",
        "Quant/QC",
        "Identification",
        "Untargeted",
        "Statistics",
        "Reports",
        "AI review",
    ];
    let mut selected = if app.quant.active {
        1
    } else if app.spectral.open {
        2
    } else if app.untargeted.open {
        3
    } else if app.statistics.open {
        4
    } else if app.delivery.open {
        5
    } else if ui
        .ctx()
        .data(|d| d.get_temp::<bool>(egui::Id::new("activity_open")))
        .unwrap_or(false)
    {
        6
    } else {
        0
    };
    ui.horizontal_wrapped(|ui| {
        for (index, label) in destinations.iter().enumerate() {
            if ui.selectable_label(selected == index, *label).clicked() {
                selected = index;
            }
        }
    });
    ui.ctx().input(|input| {
        for (index, key) in [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
            egui::Key::Num6,
            egui::Key::Num7,
        ]
        .iter()
        .enumerate()
        {
            if input.events.iter().any(|event| matches!(event, egui::Event::Key { key: pressed_key, pressed: true, modifiers, .. } if pressed_key == key && (modifiers.alt || modifiers.ctrl))) {
                selected = index;
            }
        }
    });
    app.quant.active = selected == 1;
    app.spectral.open = selected == 2;
    app.untargeted.open = selected == 3;
    app.statistics.open = selected == 4;
    app.delivery.open = selected == 5;
    ui.ctx().data_mut(|d| {
        d.insert_temp(egui::Id::new("activity_open"), selected == 6);
        d.insert_temp(egui::Id::new("central_workspaces"), true);
    });
}

/// Analytical content uses the full remaining viewport in the application.
/// Standalone module render tests can still exercise a window without a shell.
pub(super) fn analytical_panel(
    ctx: &Context,
    title: &str,
    open: &mut bool,
    contents: impl FnOnce(&mut egui::Ui),
) {
    if !*open {
        return;
    }
    if ctx
        .data(|d| d.get_temp::<bool>(egui::Id::new("central_workspaces")))
        .unwrap_or(false)
    {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(title);
            egui::ScrollArea::both().id_salt(title).show(ui, |ui| {
                contents(ui);
                scroll_new_focus(ui);
            });
        });
    } else {
        egui::Window::new(title)
            .open(open)
            .default_width(1050.0)
            .vscroll(true)
            .show(ctx, |ui| {
                contents(ui);
                scroll_new_focus(ui);
            });
    }
}

/// Bring a newly keyboard/accessibility-focused control into its workspace viewport.
/// Run inside the scroll area so wheel scrolling is unaffected on later frames.
pub(super) fn scroll_new_focus(ui: &egui::Ui) {
    if let Some(id) = ui.ctx().memory(|memory| memory.focused()) {
        if let Some(response) = ui.ctx().read_response(id) {
            if response.gained_focus() && response.rect.intersects(ui.min_rect()) {
                response.scroll_to_me(Some(egui::Align::Center));
            }
        }
    }
}

pub(super) fn advanced_active(app: &MzViewerApp, ctx: &Context) -> bool {
    app.spectral.open
        || app.untargeted.open
        || app.statistics.open
        || app.delivery.open
        || ctx
            .data(|d| d.get_temp::<bool>(egui::Id::new("activity_open")))
            .unwrap_or(false)
}

pub fn activity(app: &mut MzViewerApp, ctx: &Context) {
    let id = egui::Id::new("activity_open");
    let mut open = ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    analytical_panel(ctx, "AI / MCP activity", &mut open, |ui| {
        super::ai_review::panel(app, ui);
        ui.separator();
        #[cfg(feature = "mcp")]
        if let Some(bridge) = &app.remote {
            ui.heading("Connected MCP session");
            ui.label(format!("{} completed requests", bridge.history.len()));
            ui.label(format!(
                "Analytical changes: {} · Exports: {}",
                if bridge.policy.allow_changes {
                    "Allowed"
                } else {
                    "Blocked"
                },
                if bridge.policy.allow_exports {
                    "Allowed"
                } else {
                    "Blocked"
                }
            ));
            if bridge.pending.is_some() {
                ui.spinner();
                ui.label("Reading or extracting dataset data…");
            }
            ui.label("Legacy requests execute under server launch permissions. Revision-bound suggestions use the review proposal controls above; approval and rejection retain separate attributed decisions.");
            for entry in bridge.history.iter().rev().take(100) {
                let success = entry["success"].as_bool().unwrap_or(false);
                let request = &entry["request"];
                let tool = request["operation"].as_str().unwrap_or("MCP request");
                ui.collapsing(
                    format!(
                        "{} · {} · {}",
                        entry["sequence"],
                        tool,
                        if success { "Completed" } else { "Failed" }
                    ),
                    |ui| {
                        if let Some(error) = entry["error"].as_str() {
                            ui.label(error);
                        }
                        ui.label("Request details (data, parameters and requested changes)");
                        ui.monospace(serde_json::to_string_pretty(request).unwrap_or_default());
                    },
                );
            }
            ui.small("Showing the latest 100 requests. Peak corrections and engine revisions retain their own scientific review history.");
        } else {
            ui.label("No MCP client is connected.");
            ui.label("Launch chromascope-mcp from your client to inspect session activity here.");
        }
        #[cfg(not(feature = "mcp"))]
        {
            let _ = app;
            ui.label("This desktop build runs without MCP.");
            ui.label("Use the MCP-enabled executable to connect an external agent. Numerical analysis remains available in this workbench.");
        }
    });
    ctx.data_mut(|d| d.insert_temp(id, open));
}

pub fn configure(ctx: &Context, dark: bool) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in [
        (
            "Inter",
            include_bytes!("../../assets/fonts/inter/Inter-Regular.ttf").as_slice(),
        ),
        (
            "Inter Medium",
            include_bytes!("../../assets/fonts/inter/Inter-Medium.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), egui::FontData::from_static(bytes).into());
    }
    // Keep egui's existing fonts behind Inter for symbols and language coverage.
    let fallback = fonts.families[&egui::FontFamily::Proportional].clone();
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "Inter".into());
    let heading_family = egui::FontFamily::Name("Inter Medium".into());
    let mut heading_fonts = vec!["Inter Medium".into()];
    heading_fonts.extend(fallback);
    fonts.families.insert(heading_family.clone(), heading_fonts);
    ctx.set_fonts(fonts);
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.panel_fill = if dark {
        Color32::from_rgb(39, 46, 55)
    } else {
        Color32::from_rgb(243, 245, 248)
    };
    visuals.extreme_bg_color = if dark {
        Color32::from_rgb(32, 37, 44)
    } else {
        Color32::WHITE
    };
    visuals.selection.bg_fill = if dark {
        Color32::from_rgb(41, 63, 88)
    } else {
        Color32::from_rgb(224, 237, 252)
    };
    visuals.selection.stroke = egui::Stroke::new(
        1.0_f32,
        if dark {
            Color32::from_rgb(119, 185, 241)
        } else {
            Color32::from_rgb(25, 104, 179)
        },
    );
    visuals.widgets.noninteractive.bg_stroke.color = if dark {
        Color32::from_rgb(58, 70, 84)
    } else {
        Color32::from_rgb(220, 226, 233)
    };
    visuals.override_text_color = Some(if dark {
        Color32::from_rgb(231, 235, 241)
    } else {
        Color32::from_rgb(28, 37, 49)
    });
    visuals.weak_text_color = Some(if dark {
        Color32::from_rgb(184, 196, 210)
    } else {
        Color32::from_rgb(75, 87, 104)
    });
    visuals.warn_fg_color = super::plot_controls::scientific_color(dark, 2);
    visuals.error_fg_color = super::plot_controls::scientific_color(dark, 4);
    visuals.hyperlink_color = super::plot_controls::scientific_color(dark, 1);
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.interact_size.y = CONTROL_HEIGHT;
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(BODY_SIZE));
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::proportional(BODY_SIZE),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::proportional(SMALL_SIZE),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(18.0, heading_family),
        );
    });
}

pub fn trace_buttons(app: &mut MzViewerApp, ui: &mut egui::Ui) {
    for (kind, label) in [
        (PlotType::Tic, "TIC"),
        (PlotType::Bpc, "BPC"),
        (PlotType::Xic, "XIC"),
    ] {
        if ui
            .selectable_label(app.user_input.plot_type == kind, label)
            .clicked()
            && app.user_input.plot_type != kind
        {
            app.user_input.plot_type = kind;
            // Wait for a valid target before extracting an XIC.
            if kind != PlotType::Xic
                || app.user_input.mass.value > 0.0 && app.user_input.mass_tolerance.value > 0.0
            {
                app.state_changed = StateChange::Changed;
            } else {
                app.state_changed = StateChange::Unchanged;
            }
        }
    }
}

pub fn status(app: &MzViewerApp, ctx: &Context) {
    egui::TopBottomPanel::bottom("workbench_status").show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            let loading = app.files.values().filter(|file| file.is_loading).count();
            if loading > 0 {
                ui.spinner();
                ui.small(format!("Importing {loading} dataset(s)…"));
            } else if app.async_state.is_processing {
                ui.spinner();
                ui.small("Updating chromatogram…");
            } else {
                ui.small(format!(
                    "{} {} loaded",
                    app.files.len(),
                    if app.files.len() == 1 {
                        "file"
                    } else {
                        "files"
                    }
                ));
            }
            let low_contrast = app.files.values().any(|file| file.display.visible && contrast_ratio(file.display.color.to_egui(),ui.visuals().panel_fill)<3.0)
                || app.workspace.traces.values().flatten().any(|trace|trace.visible && contrast_ratio(trace.color.to_egui(),ui.visuals().panel_fill)<3.0);
            if low_contrast { ui.colored_label(ui.visuals().warn_fg_color,"Low-contrast trace color · adjust Appearance or switch theme"); }
            ui.separator();
            if advanced_active(app,ctx) || app.quant.active {
                ui.small("Alt/Ctrl+1–7: workspace · Tab: next control · Enter/Space: activate · Plot controls: zoom, pan, reset");
            } else { ui.small("Double-click: spectrum · Right-drag: integrate · Middle-drag: zoom · Numeric alternatives in Selection"); }
        });
    });
}

pub fn inspector(app: &mut MzViewerApp, ctx: &Context) {
    if !app.workspace.view.inspector {
        return;
    }
    egui::SidePanel::right("workbench_inspector")
        .default_width(PANEL_WIDTH)
        .width_range(220.0..=380.0)
        .resizable(true)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Trace settings");
                    if ui
                        .small_button("Hide")
                        .on_hover_text("Restore this pane with Inspector in the toolbar")
                        .clicked()
                    {
                        app.workspace.view.inspector = false;
                    }
                });
                let editable = app.presets.specs.is_none()
                    && app
                        .active_file_id
                        .and_then(|id| app.files.get(&id))
                        .is_some_and(|f| !f.is_loading);
                if app.presets.specs.is_some() {
                    ui.small("Extraction settings are shared across the batch.");
                    if ui
                        .add_enabled(
                            !app.async_state.is_processing,
                            egui::Button::new("Clear batch preset"),
                        )
                        .clicked()
                    {
                        app.presets.specs = None;
                        app.presets.applied.clear();
                        app.presets.failed.clear();
                        app.state_changed = StateChange::Unchanged;
                    }
                }
                ui.add_enabled_ui(editable, |ui| {
                    ui.label("Acquisition");
                    let previous_mode = app.user_input.acquisition;
                    egui::ComboBox::from_id_salt("acquisition_mode")
                        .selected_text(
                            previous_mode
                                .map(|m| format!("{m:?}"))
                                .unwrap_or_else(|| "All acquisition types".into()),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut app.user_input.acquisition,
                                None,
                                "All acquisition types",
                            );
                            for mode in [
                                crate::processing::AcquisitionMode::FS,
                                crate::processing::AcquisitionMode::SIM,
                                crate::processing::AcquisitionMode::MRM,
                            ] {
                                ui.selectable_value(
                                    &mut app.user_input.acquisition,
                                    Some(mode),
                                    format!("{mode:?}"),
                                );
                            }
                        });
                    if previous_mode != app.user_input.acquisition {
                        app.state_changed = StateChange::Changed;
                    }

                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        panels::add_scan_filter_dropdown(app, ui);
                    });
                    ui.separator();
                    if app.user_input.plot_type == PlotType::Xic {
                        xic_controls(app, ui);
                    } else {
                        ui.label("Mass range");
                        panels::add_range_options(app, ui);
                    }
                    ui.separator();
                    ui.label("Smoothing");
                    let raw_id=egui::Id::new("show_unsmoothed_trace");
                    let mut show_raw=ctx.data(|d|d.get_temp::<bool>(raw_id)).unwrap_or(true);
                    ui.checkbox(&mut show_raw,"Overlay unsmoothed trace");
                    ctx.data_mut(|d|d.insert_temp(raw_id,show_raw));
                    if ui
                        .add(
                            egui::Slider::new(&mut app.user_input.smoothing, 0..=10).text("radius"),
                        )
                        .changed()
                    {
                        app.state_changed = StateChange::Changed;
                    }
                    if app.user_input.smoothing == 0 {
                        ui.small("Off · original trace");
                    } else {
                        ui.small(format!(
                            "Up to {} trace points per moving-average window; used for calculation and export",
                            2 * u16::from(app.user_input.smoothing) + 1
                        ));
                    }
                });
                ui.add_space(10.0);
                egui::CollapsingHeader::new("Appearance")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Slider::new(&mut app.user_input.line_width, 0.5..=4.0)
                                .text("Line width"),
                        );
                        if contrast_ratio(app.user_input.line_color.to_egui(), ui.visuals().panel_fill) < 3.0 {
                            ui.label("Trace color has low contrast in this theme.");
                            if ui.button("Use visible trace color").clicked() {
                                app.user_input.line_color = if ui.visuals().dark_mode { LineColor::Cyan } else { LineColor::Blue };
                                if let Some(file) = app.active_file_id.and_then(|id| app.files.get_mut(&id)) { file.display.color = app.user_input.line_color; }
                            }
                        }
                        let previous = app.user_input.line_color;
                        ui.horizontal_wrapped(|ui| {
                            for (color, name) in [
                                (LineColor::Blue, "Blue"),
                                (LineColor::Orange, "Orange"),
                                (LineColor::Green, "Green"),
                                (LineColor::Red, "Red"),
                                (LineColor::Magenta, "Purple"),
                                (LineColor::Gray, "Gray"),
                                (LineColor::Cyan, "Cyan"),
                                (LineColor::Gold, "Gold"),
                                (LineColor::Yellow, "Yellow"),
                                (LineColor::White, "White"),
                            ] {
                                let selected = app.user_input.line_color == color;
                                let response = ui
                                    .add(
                                        egui::Button::new(" ")
                                            .selected(selected)
                                            .min_size(egui::vec2(26.0, 26.0)),
                                    )
                                    .on_hover_text(name);
                                ui.painter().circle_filled(
                                    response.rect.center(),
                                    5.0,
                                    color.to_egui(),
                                );
                                response.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::WidgetType::Button,
                                        true,
                                        selected,
                                        name,
                                    )
                                });
                                if response.clicked() {
                                    app.user_input.line_color = color;
                                }
                            }
                        });
                        if previous != app.user_input.line_color {
                            if let Some(file) =
                                app.active_file_id.and_then(|id| app.files.get_mut(&id))
                            {
                                file.display.color = app.user_input.line_color;
                            }
                        }
                    });
                super::workspace::inspector(app, ui);
                ui.heading("Selection");
                numeric_spectrum(app,ui);
                if let Some(rt) = app.user_input.retention_time_ms_spectrum {
                    ui.label(format!("Retention time: {rt:.3} min"));
                } else {
                    ui.small("Double-click a trace to inspect a spectrum.");
                }
                if let (Some(start), Some(end)) = (app.integration.start_rt, app.integration.end_rt)
                {
                    ui.label(format!(
                        "Integration: {:.3}–{:.3} min",
                        start.min(end),
                        start.max(end)
                    ));
                }
                if let Some(area) = app.integration.result {
                    ui.label(format!("Area: {area:.4e} intensity·min"));
                }
                if app.integration.start_rt.is_some() && ui.button("Clear integration").clicked() {
                    app.integration = Default::default();
                }
                ui.small("Right-drag across a peak to integrate.");
                numeric_integration(app, ui);
                if let Some(points)=app.active_file_id.and_then(|id|app.files.get(&id)).and_then(|file|file.cache.plot_data.as_ref()) {
                    super::plot_controls::export(ui,points,"Retention time (min)","Intensity (instrument units)");
                }
                ui.small("Integration uses the full-resolution processed trace and a straight line between endpoints as baseline. Set smoothing to 0 to inspect unsmoothed data.");
            });
        });
}

fn numeric_spectrum(app: &mut MzViewerApp, ui: &mut egui::Ui) {
    let Some(file_id) = app.active_file_id else {
        return;
    };
    let key = egui::Id::new(("numeric_spectrum", file_id));
    let mut rt = ui
        .ctx()
        .data(|d| d.get_temp::<f64>(key))
        .unwrap_or_else(|| app.user_input.retention_time_ms_spectrum.unwrap_or(0.) as f64);
    ui.horizontal_wrapped(|ui| {
        ui.label("Spectrum at RT (min)");
        ui.add(egui::DragValue::new(&mut rt).speed(0.01));
    });
    if ui.button("Inspect nearest acquired scan").clicked() {
        let domain = app
            .files
            .get(&file_id)
            .and_then(|file| file.cache.chromatogram.as_ref())
            .and_then(|raw| {
                let low = raw.retention_time.iter().copied().reduce(f32::min)?;
                let high = raw.retention_time.iter().copied().reduce(f32::max)?;
                Some([low as f64, high as f64])
            });
        if rt.is_finite() && domain.is_some_and(|bounds| rt >= bounds[0] && rt <= bounds[1]) {
            if let Some(file) = app.files.get_mut(&file_id) {
                if let Some(index) =
                    file.cache.chromatogram.as_ref().and_then(|raw| {
                        crate::processing::find_closest_spectrum_index(raw, rt as f32)
                    })
                {
                    match file.data.get_mass_spectrum_by_index(index) {
                        Ok(spectrum) => {
                            app.user_input.retention_time_ms_spectrum =
                                Some(spectrum.retention_time);
                            file.cache.mass_spectrum = Some(spectrum);
                            app.workspace.view.spectrum = true;
                        }
                        Err(error) => app.error_message = Some(error.to_string()),
                    }
                }
            }
        } else {
            app.error_message =
                Some("Retention time must lie within the acquired chromatogram range".into());
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(key, rt));
}
fn numeric_integration(app: &mut MzViewerApp, ui: &mut egui::Ui) {
    let Some(file_id) = app.active_file_id else {
        return;
    };
    let Some(points) = app
        .files
        .get(&file_id)
        .and_then(|f| f.cache.plot_data.as_ref())
    else {
        return;
    };
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return;
    };
    let domain = [first[0], last[0]];
    // Include extraction parameters so drafts cannot leak across different traces.
    let id = egui::Id::new((
        "numeric_integration",
        file_id,
        format!("{:?}", app.files[&file_id].cache.last_processing_params),
    ));
    let mut bounds = ui
        .ctx()
        .data(|d| d.get_temp::<[f64; 2]>(id))
        .unwrap_or(domain);
    ui.collapsing("Enter integration boundaries", |ui| {
        ui.horizontal(|ui| {
            ui.label("Start (min)");
            ui.add(egui::DragValue::new(&mut bounds[0]).speed(0.001).max_decimals(5));
        });
        ui.horizontal(|ui| {
            ui.label("End (min)");
            ui.add(egui::DragValue::new(&mut bounds[1]).speed(0.001).max_decimals(5));
        });
        let valid = bounds.iter().all(|x| x.is_finite()) && bounds[0] >= domain[0] && bounds[1] <= domain[1] && bounds[0] < bounds[1];
        if !valid { ui.label(format!("Use increasing boundaries within {:.5}–{:.5} min.", domain[0], domain[1])); }
        if ui.add_enabled(valid && !app.async_state.is_processing, egui::Button::new("Integrate and record area")).clicked() {
            app.integration = super::state::IntegrationState { start_rt: Some(bounds[0]), end_rt: Some(bounds[1]), ..Default::default() };
            super::interactivity::compute_integration(app);
        }
        ui.small("Creates an exploratory measurement. Batch results are reviewed in Quantification and QC.");
    });
    ui.ctx().data_mut(|d| d.insert_temp(id, bounds));
}

fn xic_controls(app: &mut MzViewerApp, ui: &mut egui::Ui) {
    ui.label("Target m/z");
    ui.add(
        egui::TextEdit::singleline(&mut app.user_input.mass.text)
            .hint_text("e.g. 483.0000")
            .desired_width(f32::INFINITY),
    );
    ui.label("Tolerance (ppm)");
    ui.add(
        egui::TextEdit::singleline(&mut app.user_input.mass_tolerance.text)
            .hint_text("e.g. 10")
            .desired_width(f32::INFINITY),
    );
    if ui.button("Apply XIC").clicked() {
        let parsed = app
            .user_input
            .mass
            .text
            .parse::<f64>()
            .ok()
            .zip(app.user_input.mass_tolerance.text.parse::<f64>().ok());
        let bounds = app
            .active_file_id
            .and_then(|id| app.files.get(&id))
            .map(|file| file.data.bounds)
            .unwrap_or_else(crate::validation::DataBounds::unrestricted);
        match parsed.map(|(mass, tolerance)| {
            crate::validation::XicParams::new(mass, app.user_input.polarity, tolerance, &bounds)
                .map(|_| (mass, tolerance))
        }) {
            Some(Ok((mass, tolerance))) => {
                app.user_input.mass.value = mass;
                app.user_input.mass_tolerance.value = tolerance;
                app.state_changed = StateChange::Changed;
            }
            Some(Err(error)) => app.show_error_dialog(error.to_string()),
            None => {
                app.show_error_dialog("Enter numeric values for target m/z and tolerance.".into())
            }
        }
    }
}

pub(super) fn contrast_ratio(a: egui::Color32, b: egui::Color32) -> f64 {
    fn luminance(c: egui::Color32) -> f64 {
        let linear = |v: u8| {
            let v = v as f64 / 255.;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
    }
    let a = luminance(a);
    let b = luminance(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::state::{FileCache, FileDisplaySettings, OpenFile};

    /// Exercise real egui layout at the minimum window size and on both themes.
    /// Optional mesh output allows visual inspection without a desktop session.
    #[test]
    fn workbench_frames_fit_and_preserve_cached_data() {
        for (name, dark, width, loaded) in [
            ("empty", false, 1280.0, false),
            ("light", false, 1280.0, true),
            ("xic", false, 1280.0, true),
            ("dark", true, 920.0, true),
            ("horizontal", true, 1280.0, true),
            ("full-chromatograms", false, 1280.0, true),
            ("file-menu", false, 1280.0, true),
            ("grid-menu", true, 920.0, true),
            ("preset-editor", false, 920.0, true),
            ("about", true, 920.0, true),
            ("shared-scale", true, 1280.0, true),
            ("comparison", true, 1280.0, true),
            ("readme", true, 1280.0, true),
        ] {
            let ctx = Context::default();
            configure(&ctx, dark);
            let mut app = MzViewerApp::default();
            app.user_input.polarity = mzdata::spectrum::ScanPolarity::Positive;
            app.user_input.line_color = LineColor::Blue;
            if name == "xic" {
                app.user_input.plot_type = PlotType::Xic;
                app.user_input.mass = super::super::state::ValidatedInput::new(483.0);
                app.user_input.mass_tolerance = super::super::state::ValidatedInput::new(10.0);
            }
            if loaded {
                let mut data = crate::parser::MzData::new();
                data.bounds = crate::validation::DataBounds {
                    min_mz: 100.0,
                    max_mz: 700.0,
                    min_rt: 0.0,
                    max_rt: 10.0,
                    scan_count: 240,
                };
                data.available_scan_filters = vec![(
                    1,
                    mzdata::spectrum::ScanPolarity::Positive,
                    None,
                    100.0,
                    700.0,
                )];
                let points: Vec<_> = (0..240)
                    .map(|i| {
                        let x = f64::from(i) / 24.0;
                        [x, 800000.0 * (-((x - 4.82) / 0.32).powi(2)).exp()]
                    })
                    .collect();
                app.files.insert(
                    0,
                    OpenFile {
                        id: 0,
                        name: "A long acquisition filename 261002-14.raw".into(),
                        path: "test.raw".into(),
                        data,
                        display: FileDisplaySettings {
                            color: LineColor::Blue,
                            visible: true,
                        },
                        is_loading: false,
                        cache: FileCache {
                            plot_data: Some(points.clone()),
                            display_data: Some(points),
                            last_processing_params: Some(crate::processing::ProcessingParams {
                                acquisition: None,
                                plot_type: app.user_input.plot_type,
                                ms_level: 1,
                                polarity: app.user_input.polarity,
                                smoothing: 0,
                                xic_params: None,
                                mz_range: None,
                                precursor_mz: None,
                            }),
                            mass_spectrum: Some(crate::parser::MassSpectrum {
                                mz: vec![153.0, 389.0, 483.0, 501.0],
                                intensity: vec![120000.0, 250000.0, 800000.0, 140000.0],
                                index: 128,
                                retention_time: 4.82,
                            }),
                            ..Default::default()
                        },
                    },
                );
                app.active_file_id = Some(0);
                app.user_input.retention_time_ms_spectrum = Some(4.82);
            }
            if name == "horizontal"
                || name == "full-chromatograms"
                || name == "shared-scale"
                || name == "comparison"
                || name == "readme"
            {
                app.workspace.view.horizontal = name == "horizontal";
                app.workspace.view.spectrum = false;
                app.workspace.view.files = false;
                app.workspace.view.inspector = false;
                let file = &app.files[&0];
                let points = file.cache.plot_data.as_ref().unwrap().clone();
                let params = file.cache.last_processing_params.as_ref().unwrap().clone();
                for i in 1..8 {
                    let mut p = params.clone();
                    p.smoothing = i;
                    app.workspace.traces.entry(0).or_default().push(
                        super::super::workspace::Trace {
                            name: format!("Additional ion {i}"),
                            points: points.clone(),
                            display_points: points.clone(),
                            chromatogram: crate::parser::ChromatogramData {
                                retention_time: vec![],
                                intensity: vec![],
                                index: vec![],
                                mz: vec![],
                            },
                            params: p,
                            color: LineColor::Green,
                            visible: true,
                            order: usize::from(i),
                        },
                    );
                }
                if name == "shared-scale" {
                    app.workspace.view.intensity_scale =
                        super::super::workspace::IntensityScale::SharedHighest;
                    for (i, trace) in app
                        .workspace
                        .traces
                        .get_mut(&0)
                        .unwrap()
                        .iter_mut()
                        .enumerate()
                    {
                        for point in &mut trace.points {
                            point[1] /= (i + 2) as f64;
                        }
                        trace.display_points = trace.points.clone();
                    }
                }
                if name == "comparison" || name == "readme" {
                    app.workspace.traces.get_mut(&0).unwrap().truncate(2);
                    app.workspace.view.compare_samples = true;
                    let params = app.files[&0].cache.last_processing_params.clone();
                    let points = app.files[&0].cache.plot_data.clone();
                    app.files.insert(
                        1,
                        OpenFile {
                            id: 1,
                            name: "Comparison sample 02.mzML".into(),
                            path: "sample02.mzML".into(),
                            data: crate::parser::MzData::new(),
                            display: FileDisplaySettings {
                                color: LineColor::Orange,
                                visible: true,
                            },
                            is_loading: false,
                            cache: FileCache {
                                plot_data: points.clone(),
                                display_data: points,
                                last_processing_params: params,
                                ..Default::default()
                            },
                        },
                    );
                }
            }
            let height = if width < 1000.0 { 600.0 } else { 820.0 };
            if name == "readme" {
                app.workspace.traces.get_mut(&0).unwrap().truncate(1);
                app.files.get_mut(&0).unwrap().name = "Sample 01.mzML".into();
                app.files.get_mut(&1).unwrap().name = "Sample 02.mzML".into();
                app.workspace.view.intensity_scale =
                    super::super::workspace::IntensityScale::SharedHighest;
                for (i, trace) in app
                    .workspace
                    .traces
                    .get_mut(&0)
                    .unwrap()
                    .iter_mut()
                    .enumerate()
                {
                    trace.params.plot_type = PlotType::Xic;
                    trace.params.smoothing = 0;
                    trace.params.xic_params = Some(
                        crate::validation::XicParams::new(
                            483.0 + i as f64 * 4.0,
                            app.user_input.polarity,
                            10.0,
                            &crate::validation::DataBounds::unrestricted(),
                        )
                        .unwrap(),
                    );
                    for point in &mut trace.points {
                        point[1] *= 0.5 / (i + 1) as f64;
                    }
                    trace.display_points = trace.points.clone();
                }
                let mut traces = app.workspace.traces[&0].clone();
                for trace in &mut traces {
                    trace.color = LineColor::Orange;
                    for point in &mut trace.points {
                        point[1] *= 0.7;
                    }
                    trace.display_points = trace.points.clone();
                }
                app.workspace.traces.insert(1, traces);
            }
            if name == "preset-editor" {
                app.presets.editor.open = true;
            }
            if name == "about" {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("about_open"), true));
            }
            let height = if name == "readme" { 1000.0 } else { height };
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, height),
                )),
                ..Default::default()
            };
            let mut atlas = egui::ColorImage::new([1, 1], vec![Color32::WHITE]);
            let mut outputs = Vec::new();
            for frame in 0..3 {
                input.time = Some(frame as f64 * 0.2);
                let output = ctx.run(input.clone(), |ctx| {
                    panels::update_data_selection_panel(&mut app, ctx);
                    status(&app, ctx);
                    panels::update_file_information_panel(&mut app, ctx);
                    inspector(&mut app, ctx);
                    panels::update_central_panel(&mut app, ctx);
                    super::super::information::show(&app, ctx);
                    if name == "preset-editor" {
                        super::super::preset_editor::show(&mut app, ctx);
                    }
                });
                input.events.clear();
                if frame == 0 {
                    let target = match name {
                        "file-menu" => Some("File"),
                        "grid-menu" => Some("Plot options"),
                        _ => None,
                    };
                    if let Some(target) = target {
                        let pos = super::super::test_render::text_center(&output.shapes, target)
                            .expect("menu button must be visible");
                        input.events = vec![
                            egui::Event::PointerMoved(pos),
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: true,
                                modifiers: egui::Modifiers::NONE,
                            },
                            egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: false,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ];
                    }
                }
                for (_, delta) in &output.textures_delta.set {
                    let egui::ImageData::Color(image) = &delta.image;
                    if let Some([x, y]) = delta.pos {
                        for row in 0..image.size[1] {
                            for col in 0..image.size[0] {
                                atlas[(x + col, y + row)] = image[(col, row)];
                            }
                        }
                    } else {
                        atlas = (**image).clone();
                    }
                }
                outputs.push(output.clone());
                let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
                assert!(!meshes.is_empty());
                for primitive in &meshes {
                    assert!(primitive.clip_rect.is_finite());
                    if let egui::epaint::Primitive::Mesh(mesh) = &primitive.primitive {
                        assert!(mesh.vertices.iter().all(|v| v.pos.is_finite()));
                    }
                }
                if frame == 2 {
                    if let Ok(directory) = std::env::var("CHROMASCOPE_PREVIEW_DIR") {
                        save_mesh_preview(&directory, name, width, height, &atlas, &meshes);
                        super::super::test_render::save(
                            &ctx,
                            outputs.clone(),
                            &std::path::Path::new(&directory).join(format!("{name}.png")),
                            egui::vec2(width, height),
                        );
                    }
                }
            }
            assert!(!app.async_state.is_processing);
            if loaded {
                assert_eq!(app.files[&0].cache.plot_data.as_ref().unwrap().len(), 240);
            }
        }
    }

    fn save_mesh_preview(
        directory: &str,
        name: &str,
        width: f32,
        height: f32,
        atlas: &egui::ColorImage,
        meshes: &[egui::ClippedPrimitive],
    ) {
        use std::io::Write;
        let directory = std::path::Path::new(directory);
        let pixels: Vec<u8> = atlas
            .pixels
            .iter()
            .flat_map(|color| color.to_array())
            .collect();
        image::save_buffer(
            directory.join(format!("{name}-atlas.png")),
            &pixels,
            atlas.size[0] as u32,
            atlas.size[1] as u32,
            image::ColorType::Rgba8,
        )
        .unwrap();
        let mut file = std::fs::File::create(directory.join(format!("{name}-meshes.bin"))).unwrap();
        for value in [width, height] {
            file.write_all(&value.to_le_bytes()).unwrap();
        }
        for primitive in meshes {
            if let egui::epaint::Primitive::Mesh(mesh) = &primitive.primitive {
                for value in [
                    primitive.clip_rect.min.x,
                    primitive.clip_rect.min.y,
                    primitive.clip_rect.max.x,
                    primitive.clip_rect.max.y,
                ] {
                    file.write_all(&value.to_le_bytes()).unwrap();
                }
                for count in [mesh.vertices.len(), mesh.indices.len()] {
                    file.write_all(&(count as u32).to_le_bytes()).unwrap();
                }
                for vertex in &mesh.vertices {
                    for value in [vertex.pos.x, vertex.pos.y, vertex.uv.x, vertex.uv.y] {
                        file.write_all(&value.to_le_bytes()).unwrap();
                    }
                    file.write_all(&vertex.color.to_array()).unwrap();
                }
                for index in &mesh.indices {
                    file.write_all(&index.to_le_bytes()).unwrap();
                }
            }
        }
    }
}

#[cfg(test)]
mod keyboard_contract_tests {
    use super::*;
    #[test]
    fn navigation_uses_key_event_modifiers_after_modifier_release() {
        let mut app = MzViewerApp::default();
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Key {
                    key: egui::Key::Num4,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        alt: true,
                        ..Default::default()
                    },
                }],
                modifiers: Default::default(),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| navigation(&mut app, ui));
            },
        );
        assert!(app.untargeted.open);
        assert!(!app.quant.active && !app.spectral.open && !app.statistics.open);
    }
    #[test]
    fn theme_text_and_secondary_text_meet_readability_contrast() {
        for dark in [false, true] {
            let ctx = egui::Context::default();
            configure(&ctx, dark);
            let visuals = ctx.style().visuals.clone();
            assert!(contrast_ratio(visuals.text_color(), visuals.panel_fill) >= 4.5);
            assert!(contrast_ratio(visuals.weak_text_color(), visuals.panel_fill) >= 4.5);
            for color in [
                visuals.warn_fg_color,
                visuals.error_fg_color,
                visuals.hyperlink_color,
            ] {
                assert!(contrast_ratio(color, visuals.panel_fill) >= 4.5);
            }
            for role in 0..5 {
                assert!(
                    contrast_ratio(
                        super::super::plot_controls::scientific_color(dark, role),
                        visuals.panel_fill
                    ) >= 4.5
                );
            }
        }
    }
}

#[cfg(test)]
mod focus_scroll_tests {
    use super::*;
    #[test]
    fn lower_workspace_controls_scroll_into_view_when_focused() {
        let ctx = egui::Context::default();
        ctx.style_mut(|style| style.animation_time = 0.0);
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("central_workspaces"), true));
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(920.0, 600.0));
        let mut last = egui::Rect::NOTHING;
        for frame in 0..4 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    time: Some(frame as f64 * 0.25),
                    ..Default::default()
                },
                |ctx| {
                    analytical_panel(ctx, "Focus regression", &mut true, |ui| {
                        for row in 0..40 {
                            let response = ui.button(format!("Review evidence {row}"));
                            if row == 39 {
                                last = response.rect;
                                if frame == 0 {
                                    response.request_focus();
                                }
                            }
                        }
                    });
                },
            );
        }
        assert!(
            last.bottom() <= screen.bottom(),
            "the focused review control must be visible without a mouse wheel: {last:?}"
        );
        assert!(last.top() >= screen.top());
    }
}
