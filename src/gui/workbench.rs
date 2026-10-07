//! Persistent controls and visual styling for the scientific workbench.
use super::{panels, state::StateChange, MzViewerApp};
use crate::plotting_parameters::{LineColor, PlotType};
use eframe::egui::{self, Color32, Context};

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
        1.0,
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
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.interact_size.y = 28.0;
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
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
            ui.separator();
            ui.small("Double-click: spectrum    Right-drag: integrate    Middle-drag: zoom");
        });
    });
}

pub fn inspector(app: &mut MzViewerApp, ctx: &Context) {
    if !app.workspace.view.inspector {
        return;
    }
    egui::SidePanel::right("workbench_inspector")
        .default_width(250.0)
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
                            "{} scans per window; used for calculation and export",
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
                    ui.label(format!("Area: {area:.4e} a.u.·min"));
                }
                if app.integration.start_rt.is_some() && ui.button("Clear integration").clicked() {
                    app.integration = Default::default();
                }
                ui.small("Right-drag across a peak to integrate.");
            });
        });
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
                        "grid-menu" => Some("Grid layout…"),
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
