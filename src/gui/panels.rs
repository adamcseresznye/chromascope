use super::plotting;
use crate::gui::state::{
    next_color_for_index, FileCache, FileDisplaySettings, FileValidity, MzViewerApp, OpenFile,
    StateChange,
};
use crate::{
    parser,
    plotting_parameters::{LineColor, PlotType},
};
use eframe::egui;
use egui::{Color32, Context, Ui};
use log::{debug, error, info, warn};
use mzdata::spectrum::ScanPolarity;
/// Updates the data selection panel in the user interface.
///
/// This function creates a top panel in the UI that contains the following elements:
/// - A "File" button that allows the user to select a file to open.
/// - A "Display" menu button that allows the user to configure the display options.
/// - A light/dark mode toggle button that allows the user to switch between light and dark themes.
///
/// When the "File" button is clicked, the function handles the file selection process, clears the existing plot data and parser data, and updates the user input accordingly.
///
/// When the "Display" menu button is clicked, the function calls the `add_display_options` function to add the display options to the menu.
///
/// When the light/dark mode toggle button is clicked, the function updates the visuals of the UI based on the user's selection.
///
/// # Parameters
/// - `&mut self`: A mutable reference to the current instance of the struct that contains the `plot_data`, `parsed_ms_data`, `user_input`, and other relevant fields.
/// - `ctx: &Context`: A reference to the `egui::Context` instance, which is used to update the UI's visuals.
pub fn update_data_selection_panel(app: &mut MzViewerApp, ctx: &Context) {
    egui::TopBottomPanel::top("data_selection_panel").show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut app.quant.active, false, "Viewer");
            ui.selectable_value(&mut app.quant.active, true, "Batch Quantification");
            ui.separator();
            ui.menu_button("File", |ui| {
                if ui.button("Open data…").on_hover_text("Open one or more data files").clicked() {
                    debug!("File open button clicked.");
                    handle_file_selection(app);
                    info!("File selection handled.");
                    ui.close();
                }

                if ui
                    .button("Open dataset folder…")
                    .on_hover_text("Open a vendor dataset directory, such as .d or Waters .raw")
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        queue_file_imports(app, vec![path]);
                    }
                    ui.close();
                }
                super::workspace::recent_menu(app, ui);
                ui.separator();
                if !app.quant.active { super::workspace::menu(app, ui); ui.separator(); }
                ui.menu_button("Vendor import settings", |ui| {
                    ui.label("Vendor data is converted locally to temporary mzML.");
                    if let Some(path) = &app.msconvert_path {
                        ui.label(format!("msconvert: {}", path.display()));
                    } else {
                        ui.label("msconvert is not configured.");
                    }
                    if ui.button("Locate msconvert…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            match crate::import::save_msconvert(&path) {
                                Ok(()) => app.msconvert_path = Some(path),
                                Err(e) => app.show_error_dialog(e),
                            }
                        }
                        ui.close();
                    }
                    if ui
                        .button("Detect msconvert again")
                        .on_hover_text("Search the environment, saved location, PATH, and standard installation folders")
                        .clicked()
                    {
                        app.msconvert_path = crate::import::discover_msconvert();
                    }
                    ui.hyperlink_to(
                        "Get ProteoWizard",
                        "https://proteowizard.sourceforge.io/download.html",
                    );
                });

                if !app.quant.active {
                    ui.menu_button("Export", |ui| {
                        let has_trace=app.active_file_id.and_then(|id|app.files.get(&id)).is_some_and(|f|f.cache.plot_data.is_some());
                        if ui
                            .add_enabled(has_trace,egui::Button::new("Chromatogram CSV…"))
                            .on_hover_text("Export the active chromatogram to CSV")
                            .clicked()
                        {
                            debug!("Export Chromatogram clicked.");
                            handle_csv_export(app);
                            ui.close();
                        }

                        // Greyed out with tooltip if no spectrum is loaded yet
                        let has_spectrum = app
                            .active_file_id
                            .and_then(|id| app.files.get(&id))
                            .and_then(|f| f.cache.mass_spectrum.as_ref())
                            .is_some();

                        ui.add_enabled_ui(has_spectrum, |ui| {
                            let btn = ui.button("Spectrum CSV…").on_hover_text(if has_spectrum {
                                "Export the mass spectrum at the current retention time to CSV"
                            } else {
                                "Double-click the chromatogram first to load a spectrum"
                            });
                            if btn.clicked() {
                                debug!("Export Mass Spectrum clicked.");
                                handle_spectrum_csv_export(app);
                                ui.close();
                            }
                        });
                        ui.separator();
                        super::workspace::figure_menu(app,ui);
                    });
                }
                ui.separator();
                if ui.button("About Chromascope…").clicked() {
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("about_open"), true));
                    ui.close();
                }
            });

            if !app.quant.active {
                ui.menu_button("Display", |ui| {
                    debug!("Display menu button clicked.");
                    add_display_options(app, ui);
                    info!("Display options added.");
                });
            }

            {
                let is_dark = ui.visuals().dark_mode;
                if ui
                    .button(if is_dark { "Light theme" } else { "Dark theme" })
                    .clicked()
                {
                    debug!("Visuals toggle button clicked.");
                    super::workbench::configure(ctx, !is_dark);
                    info!("Visuals updated.");
                }
            }
        });
        if !app.quant.active {
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                let loaded=app.active_file_id.and_then(|id|app.files.get(&id)).is_some_and(|f|!f.is_loading);
                ui.add_enabled_ui(loaded,|ui|super::workbench::trace_buttons(app,ui));
                ui.separator();
                ui.weak("Panels:");
                let panels_changed = ui.toggle_value(&mut app.workspace.view.files, "Data files").on_hover_text("Show or hide the left panel for selecting samples, managing visible files, and cancelling imports").clicked()
                    | ui.toggle_value(&mut app.workspace.view.inspector, "Trace settings").on_hover_text("Show or hide the right panel for extraction filters, smoothing, trace appearance, and measurements").clicked()
                    | ui.toggle_value(&mut app.workspace.view.spectrum, "Mass spectrum").on_hover_text("Show or hide the lower spectrum plot and scan navigation; hiding preserves the selected scan").clicked();
                if panels_changed { app.workspace.view.focus_restore = None; }
                ui.separator();
                if ui.selectable_label(app.workspace.view.focus_restore.is_some(), "Focus mode").on_hover_text("Hide all three panels to focus on chromatograms. Turn off to restore your previous panel setup. Opening a panel leaves focus mode.").clicked() {
                    app.workspace.view.toggle_focus();
                }
                if let Some(file) = app.active_file_id.and_then(|id| app.files.get(&id)) {
                    ui.separator();
                    ui.add(egui::Label::new(&file.name).truncate())
                        .on_hover_text(file.cache.source_path.as_deref().unwrap_or(&file.path));
                }
            });
        }
    });
}

/// Adds the display options to the provided `egui::Ui` instance.
///
/// This function creates a series of menu buttons that allow the user to adjust the following display options:
/// - Smoothing level: Adjusts the level of moving average smoothing applied to the plot data.
/// - Line width: Adjusts the width of the lines in the plot.
/// - Line color: Allows the user to select the color of the lines in the plot.
///
/// When the user changes any of these options, the function updates the corresponding fields in the `user_input` struct and sets the `state_changed` flag to indicate that the plot data needs to be re-processed.
///
/// # Parameters
/// - `&mut self`: A mutable reference to the current instance of the struct that contains the `user_input` and `state_changed` fields.
/// - `ui: &mut Ui`: A mutable reference to the `egui::Ui` instance where the display options will be added.
pub fn add_display_options(app: &mut MzViewerApp, ui: &mut Ui) {
    ui.menu_button("Smoothing", |ui| {
        let slider = egui::Slider::new(&mut app.user_input.smoothing, 0..=10);
        let response = ui.add(slider);
        if response.changed() {
            app.state_changed = StateChange::Changed;
            info!("Smoothing level changed to {}", app.user_input.smoothing);
        }
        response.on_hover_text("Adjust the level of moving average smoothing");

        // Show warning if smoothing is too large for file
        if let Some(active_id) = app.active_file_id {
            if let Some(file) = app.files.get(&active_id) {
                let bounds = &file.data.bounds;
                let max_reasonable = (bounds.scan_count / 10).max(3) as u8;

                if app.user_input.smoothing > max_reasonable && bounds.scan_count > 0 {
                    ui.add_space(5.0);
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 100, 100),
                        format!(
                            "⚠ Window ({}) is large for {} scans. Max recommended: {}",
                            app.user_input.smoothing, bounds.scan_count, max_reasonable
                        ),
                    );
                }
            }
        }
    });

    ui.menu_button("Line width", |ui| {
        let slider = egui::Slider::new(&mut app.user_input.line_width, 0.1..=5.0);
        let response = ui.add(slider);
        if response.changed() {
            app.state_changed = StateChange::Changed;
            info!("Line width changed to {}", app.user_input.line_width);
        }
        response.on_hover_text("Adjust the line width");
    });

    ui.menu_button("Line color", |ui| {
        debug!("Line color menu button clicked.");
        add_line_color_options(app, ui);
        info!("Line color options added.");
    });
}

/// Adds the line color options to the provided `egui::Ui` instance.
///
/// This function creates a horizontal layout of radio buttons that allow the user to select the color of the lines in the plot.
/// The available colors are: Red, Blue, Green, Yellow, Black, and White.
///
/// When the user selects a new color, the function updates the `user_input.line_color` field accordingly.
///
/// # Parameters
/// - `&mut self`: A mutable reference to the current instance of the struct that contains the `user_input` field.
/// - `ui: &mut Ui`: A mutable reference to the `egui::Ui` instance where the line color options will be added.
pub fn add_line_color_options(app: &mut MzViewerApp, ui: &mut Ui) {
    let previous_color = app.user_input.line_color;

    ui.horizontal(|ui| {
        ui.radio_value(&mut app.user_input.line_color, LineColor::Red, "Red");
        ui.radio_value(&mut app.user_input.line_color, LineColor::Blue, "Blue");
        ui.radio_value(&mut app.user_input.line_color, LineColor::Green, "Green");
        ui.radio_value(&mut app.user_input.line_color, LineColor::Yellow, "Yellow");
        ui.radio_value(&mut app.user_input.line_color, LineColor::White, "White");
        ui.radio_value(&mut app.user_input.line_color, LineColor::Gray, "Gray");
        ui.radio_value(&mut app.user_input.line_color, LineColor::Cyan, "Cyan");
        ui.radio_value(&mut app.user_input.line_color, LineColor::Orange, "Orange");
        ui.radio_value(
            &mut app.user_input.line_color,
            LineColor::Magenta,
            "Magenta",
        );
        ui.radio_value(&mut app.user_input.line_color, LineColor::Gold, "Gold");
    });

    // Propagate new color to the active file's chromatogram line
    if app.user_input.line_color != previous_color {
        if let Some(active_id) = app.active_file_id {
            if let Some(file) = app.files.get_mut(&active_id) {
                file.display.color = app.user_input.line_color;
                info!(
                    "Active file '{}' chromatogram color updated to {:?}",
                    file.name, file.display.color
                );
            }
        }
    }

    info!("Line color changed.")
}

/// Handles the selection of files by the user.
///
/// This function is responsible for the following tasks:
///
/// 1. Prompts the user to select one or more files.
/// 2. For each selected file, validates the format and creates an OpenFile struct.
/// 3. Appends all valid files to the files vector.
/// 4. Sets the active file to the first newly added file.
/// 5. Triggers plot data generation for the active file.
///
/// # Errors
///
/// This function does not return any errors. If an error occurs during the file selection process,
/// it will be handled by the `rfd::FileDialog::new().pick_files()` function.
pub fn handle_file_selection(app: &mut MzViewerApp) {
    if let Some(paths) = rfd::FileDialog::new().pick_files() {
        queue_file_imports(app, paths);
    }
}

/// Add datasets without replacing existing files. An empty/cancelled selection
/// does not change any workspace state.
pub(super) fn queue_file_imports(app: &mut MzViewerApp, paths: Vec<std::path::PathBuf>) {
    let mut first_id = None;
    for path in paths {
        super::workspace::remember(app, path.clone());
        let file_id = app.next_file_id;
        app.next_file_id += 1;
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let color = next_color_for_index(file_id);
        let cache = FileCache {
            source_path: Some(path.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let cancelled = cache.import_cancel.clone();
        app.files.insert(
            file_id,
            OpenFile {
                id: file_id,
                name,
                path: path.to_string_lossy().into_owned(),
                data: parser::MzData::new(),
                display: FileDisplaySettings {
                    color,
                    visible: true,
                },
                cache,
                is_loading: true,
            },
        );
        first_id.get_or_insert(file_id);
        let tx = app.async_state.file_loading_tx.clone();
        let executable = app.msconvert_path.clone();
        std::thread::spawn(move || {
            let result =
                crate::processing::import_file_in_background(path, executable, cancelled, file_id);
            let _ = tx.send(result);
        });
    }
    if let Some(id) = first_id {
        app.active_file_id = Some(id);
        app.invalid_file = FileValidity::Valid;
        app.integration = Default::default();
        app.user_input.retention_time_ms_spectrum = None;
        if app.user_input.plot_type == PlotType::Xic && app.user_input.mass.value <= 0.0 {
            app.user_input.plot_type = PlotType::Tic;
        }
        app.state_changed = StateChange::Changed;
    }
}

/// Handles exporting the plot data from the active file to a CSV file.
///
/// This function is responsible for the following tasks:
///
/// 1. Checks if an active file is selected and has plot data available.
/// 2. Prompts the user to select a save location for the CSV file.
/// 3. Delegates to the export module to write the CSV file.
/// 4. Uses the active file's name as the default CSV filename.
///
/// # Errors
///
/// Displays error dialog if export fails. All I/O errors are handled internally.
pub fn handle_csv_export(app: &mut MzViewerApp) {
    // Check if an active file is selected
    let active_id = match app.active_file_id {
        Some(id) => id,
        None => {
            warn!("No active file selected for CSV export");
            return;
        }
    };

    let active_file = match app.files.get(&active_id) {
        Some(file) => file,
        None => {
            warn!("Active file ID {} not found", active_id);
            return;
        }
    };

    // Check if plot data exists for the active file
    let data = match &active_file.cache.plot_data {
        Some(d) => d,
        None => {
            warn!(
                "No plot data available to export for file: {}",
                active_file.name
            );
            return;
        }
    };

    // Create default filename from active file name
    let default_name = format!(
        "{}_chromatogram.csv",
        std::path::Path::new(&active_file.path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
    );

    // Open save dialog
    let dialog = rfd::FileDialog::new()
        .add_filter("CSV", &["csv"])
        .set_file_name(&default_name);

    if let Some(path) = dialog.save_file() {
        info!(
            "CSV export path selected: {:?} for file: {}",
            path, active_file.name
        );

        // Use the export module - GUI coordinates, business logic implements
        match crate::export::export_chromatogram_csv(data, &path) {
            Ok(_) => {
                info!("CSV successfully exported to: {:?}", path);
            }
            Err(e) => {
                error!("Failed to export CSV to {:?}: {}", path, e);
                app.show_error_dialog(format!("Export failed: {}", e));
            }
        }
    } else {
        warn!("No file path selected for CSV export.");
    }
}

/// Handles exporting the cached mass spectrum of the active file to a CSV file.
///
/// Requires a spectrum to have been loaded via double-click on the chromatogram.
/// Default filename includes the source file stem and the current retention time.
pub fn handle_spectrum_csv_export(app: &mut MzViewerApp) {
    let active_id = match app.active_file_id {
        Some(id) => id,
        None => {
            warn!("No active file for spectrum export");
            return;
        }
    };

    let active_file = match app.files.get(&active_id) {
        Some(f) => f,
        None => {
            warn!("Active file ID {} not found", active_id);
            return;
        }
    };

    let spectrum = match &active_file.cache.mass_spectrum {
        Some(s) => s,
        None => {
            // Should not reach here because the button is disabled when no spectrum
            // is cached, but guard defensively.
            warn!("No mass spectrum cached — button should have been disabled");
            app.show_error_dialog(
                "No spectrum loaded. Double-click the chromatogram at the desired RT first."
                    .to_string(),
            );
            return;
        }
    };

    let rt_label = app
        .user_input
        .retention_time_ms_spectrum
        .map(|rt| format!("{:.3}min", rt))
        .unwrap_or_else(|| "unknown_rt".to_string());

    let default_name = format!(
        "{}_spectrum_{}.csv",
        std::path::Path::new(&active_file.path)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy(),
        rt_label
    );

    let dialog = rfd::FileDialog::new()
        .add_filter("CSV", &["csv"])
        .set_file_name(&default_name);

    if let Some(path) = dialog.save_file() {
        info!("Spectrum export path: {:?}", path);
        match crate::export::export_spectrum_csv(spectrum, &path) {
            Ok(_) => info!("Spectrum CSV exported to {:?}", path),
            Err(e) => {
                error!("Spectrum export failed: {}", e);
                app.show_error_dialog(format!("Spectrum export failed: {}", e));
            }
        }
    }
}

/// Creates an OpenFile struct from a file path.
///
/// Updates the file information panel in the user interface.
///
/// This function is responsible for displaying all opened files in the left-side panel of the application.
/// It allows users to toggle file visibility, select the active file, and close individual files.
///
/// # Parameters
///
/// - `ctx`: A reference to the `egui::Context` object, which is used to render the user interface.
///
/// # Functionality
///
/// 1. Displays a list of all opened files with checkboxes to toggle visibility.
/// 2. Allows clicking on file names to set them as the active file.
/// 3. Provides close buttons to remove individual files.
/// 4. Highlights the currently active file.
/// 5. If no files are opened, displays a message indicating this.
pub fn update_file_information_panel(app: &mut MzViewerApp, ctx: &egui::Context) {
    if !app.workspace.view.files {
        return;
    }
    egui::SidePanel::left("file_information_panel")
        .default_width(220.0)
        .width_range(180.0..=380.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("Data files");
                if ui.small_button("Hide").clicked() {
                    app.workspace.view.files = false;
                }
            });
            ui.small(format!(
                "{} {}",
                app.files.len(),
                if app.files.len() == 1 {
                    "dataset"
                } else {
                    "datasets"
                }
            ));
            ui.separator();
            let mut remove = None;
            let mut select = None;
            egui::ScrollArea::vertical().show(ui, |ui| {
                if app.files.is_empty() {
                    ui.weak("Open data to start inspecting chromatograms and spectra.");
                }
                let mut files: Vec<_> = app.files.iter_mut().collect();
                files.sort_by_key(|(id, _)| **id);
                for (id, file) in files {
                    let active = app.active_file_id == Some(*id);
                    let fill = if active {
                        ui.visuals().selection.bg_fill
                    } else {
                        Color32::TRANSPARENT
                    };
                    egui::Frame::new()
                        .fill(fill)
                        .inner_margin(8)
                        .corner_radius(5)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                if file.is_loading {
                                    ui.spinner();
                                } else {
                                    ui.checkbox(&mut file.display.visible, "")
                                        .on_hover_text("Show this trace");
                                    let (marker, _) = ui.allocate_exact_size(
                                        egui::vec2(10.0, 18.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().circle_filled(
                                        marker.center(),
                                        4.0,
                                        file.display.color.to_egui(),
                                    );
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button(if file.is_loading {
                                                "Cancel"
                                            } else {
                                                "×"
                                            })
                                            .on_hover_text("Close dataset")
                                            .clicked()
                                        {
                                            remove = Some(*id);
                                        }
                                        let response = ui.add(
                                            egui::Button::selectable(
                                                active,
                                                egui::RichText::new(&file.name),
                                            )
                                            .truncate(),
                                        );
                                        if response
                                            .on_hover_text(
                                                file.cache
                                                    .source_path
                                                    .as_deref()
                                                    .unwrap_or(&file.path),
                                            )
                                            .clicked()
                                        {
                                            select = Some(*id);
                                        }
                                    },
                                );
                            });
                            ui.small(if file.is_loading {
                                "Importing…"
                            } else if active {
                                "Active dataset"
                            } else {
                                "Overlay trace"
                            });
                        });
                    ui.add_space(6.0);
                }
            });
            if let Some(id) = select {
                if app.active_file_id != Some(id) {
                    app.active_file_id = Some(id);
                    app.integration = Default::default();
                    app.user_input.retention_time_ms_spectrum = None;
                    if let Some(file) = app.files.get(&id) {
                        app.user_input.line_color = file.display.color;
                    }
                    super::workspace::activate_file(app, id);
                }
            }
            if let Some(id) = remove {
                app.files.remove(&id);
                app.workspace.traces.remove(&id);
                if app.active_file_id == Some(id) {
                    app.active_file_id = None;
                }
                app.user_input.retention_time_ms_spectrum = None;
                app.repair_active_file();
            }
        });
}

/// Linked plots with a draggable divider and a clear empty state.
pub fn update_central_panel(app: &mut MzViewerApp, ctx: &Context) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::central_panel(&ctx.style())
                .fill(ctx.style().visuals.extreme_bg_color)
                .inner_margin(16),
        )
        .show(ctx, |ui| {
            if app.files.is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.vertical_centered(|ui| {
                        ui.heading("Inspect your mass spectrometry data");
                        ui.label(
                            "Open mzML or a vendor dataset to view chromatograms and spectra.",
                        );
                        ui.add_space(12.0);
                        if ui.button("Open data…").clicked() {
                            handle_file_selection(app);
                        }
                        ui.small("For directory datasets, use File > Open dataset folder.");
                    });
                });
                return;
            }
            ui.horizontal_wrapped(|ui| {
                ui.strong("Chromatograms");
                if let Some(specs) = &app.presets.specs {
                    ui.small(format!("Shared preset · {} analytes", specs.len()));
                }

                ui.add_enabled_ui(!app.workspace.view.compare_samples, |ui| {
                ui.selectable_value(&mut app.workspace.overlay, false, "Grid");
                ui.selectable_value(&mut app.workspace.overlay, true, "Overlay");
                if !app.workspace.overlay {
                    ui.menu_button("Grid layout…", |ui| {
                        egui::Grid::new("grid_layout_controls").show(ui, |ui| {
                            ui.label("Rows");
                            ui.add(egui::DragValue::new(&mut app.workspace.view.rows).range(1..=8));
                            if ui.small_button("+ Row").clicked() {
                                app.workspace.view.rows = (app.workspace.view.rows + 1).min(8);
                            }
                            ui.end_row();
                            ui.label("Columns");
                            ui.add(
                                egui::DragValue::new(&mut app.workspace.view.columns).range(1..=8),
                            );
                            if ui.small_button("+ Column").clicked() {
                                app.workspace.view.columns =
                                    (app.workspace.view.columns + 1).min(8);
                            }
                            ui.end_row();
                        });
                    });
                }
                });
                ui.checkbox(&mut app.workspace.view.compare_samples, "Compare samples")
                    .on_hover_text("Samples become columns and analytes become rows. Turn off to restore your individual grid or overlay layout.");
                if app.workspace.view.compare_samples { ui.weak("Samples × analytes"); }
                use super::workspace::IntensityScale;
                let previous = (app.workspace.view.intensity_scale, app.workspace.view.intensity_maximum);
                egui::ComboBox::from_id_salt("intensity_scale")
                    .selected_text(match app.workspace.view.intensity_scale {
                        IntensityScale::Individual => "Scale: Individual",
                        IntensityScale::SharedHighest => "Scale: Shared highest peak",
                        IntensityScale::SharedCustom => "Scale: Shared custom maximum",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut app.workspace.view.intensity_scale, IntensityScale::Individual, "Individual");
                        ui.selectable_value(&mut app.workspace.view.intensity_scale, IntensityScale::SharedHighest, "Shared highest peak");
                        ui.selectable_value(&mut app.workspace.view.intensity_scale, IntensityScale::SharedCustom, "Shared custom maximum");
                    }).response.on_hover_text("Shared scales use the same intensity axis for every visible trace; data is not normalized. The automatic maximum includes all visible traces across grid pages.");
                if app.workspace.view.intensity_scale == IntensityScale::SharedCustom {
                    ui.label("Maximum (a.u.)");
                    ui.add(egui::DragValue::new(&mut app.workspace.view.intensity_maximum).range(1.0..=f64::MAX).speed(1000.0));
                }
                if previous != (app.workspace.view.intensity_scale, app.workspace.view.intensity_maximum) {
                    app.workspace.reset_plots = true;
                }
            });
            let divider_id = egui::Id::new("workbench_plot_split");
            let mut fraction = ctx.data_mut(|data| *data.get_temp_mut_or(divider_id, 0.58_f32));
            let available = ui.available_size();
            let chromatogram_height = if app.workspace.view.spectrum {
                ((available.y - 44.0) * fraction).max(130.0)
            } else {
                available.y
            };
            ui.allocate_ui_with_layout(
                egui::vec2(available.x, chromatogram_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    plotting::plot_chromatogram(app, ui, ctx);
                },
            );
            if !app.workspace.view.spectrum {
                return;
            }
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 8.0), egui::Sense::drag());
            ui.painter().hline(
                rect.x_range(),
                rect.center().y,
                ui.visuals().widgets.noninteractive.bg_stroke,
            );
            let response = response
                .on_hover_cursor(egui::CursorIcon::ResizeVertical)
                .on_hover_text("Drag to resize plots");
            if response.dragged() {
                fraction = (fraction
                    + ctx.input(|input| input.pointer.delta().y) / available.y.max(1.0))
                .clamp(0.3, 0.75);
                ctx.data_mut(|data| data.insert_temp(divider_id, fraction));
            }
            ui.horizontal_wrapped(|ui| {
                ui.strong("Mass spectrum");
                if ui.small_button("Hide").clicked() {
                    app.workspace.view.spectrum = false;
                }
                if ui
                    .small_button("Clear spectrum")
                    .on_hover_text("Clear the selected spectrum and expand chromatograms")
                    .clicked()
                {
                    if let Some(file) = app.active_file_id.and_then(|id| app.files.get_mut(&id)) {
                        file.cache.mass_spectrum = None;
                    }
                    app.user_input.retention_time_ms_spectrum = None;
                    app.workspace.view.spectrum = false;
                }
                if let Some(rt) = app.user_input.retention_time_ms_spectrum {
                    ui.weak(format!("RT {rt:.3} min"));
                }
            });
            super::workspace::scan_navigation(app, ui);
            plotting::plot_mass_spectrum(app, ui);
        });
}

/// Adds the plot properties UI elements to the provided `Ui`.
///
/// This function is responsible for rendering the UI elements that allow the user to customize the properties of the plots, such as the polarity and plot type.
///
/// # Parameters
///
/// - `ui`: A mutable reference to the `egui::Ui` object, which is used to render the UI elements.
/// # Errors
///
/// This function does not return any errors. It handles the rendering of the plot properties UI elements within the provided `Ui`.
pub fn add_plot_properties(app: &mut MzViewerApp, ui: &mut Ui) {
    egui::Grid::new("PlotPropertiesGrid")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            // Row 1: Scan Filter (dropdown selector)
            add_scan_filter_dropdown(app, ui);
            ui.end_row();

            // Row 2: Plot Type
            add_plot_type_options(app, ui);
            ui.end_row();

            // Row 3: Range (only for TIC/BPC) — inline, no separate window
            if app.user_input.plot_type != PlotType::Xic {
                ui.label("Range (m/z):");
                add_range_options(app, ui);
                ui.end_row();
            }
        });
}

/// Shows the range enable checkbox and a “Set Range…” button inline in the
/// Plot Properties grid. The actual TextEdit inputs live in a separate
/// persistent window (`render_range_window`) because egui context menus close
/// on any click, making TextEdit unusable inside them.
pub fn add_range_options(app: &mut MzViewerApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        if ui.checkbox(&mut app.user_input.range_enabled, "").changed() {
            app.state_changed = StateChange::Changed;
            info!("Range filtering toggled: {}", app.user_input.range_enabled);
        }

        if app.user_input.range_enabled {
            // Show current committed values as a read-only label
            ui.label(
                egui::RichText::new(format!(
                    "{:.0} \u{2013} {:.0} m/z",
                    app.user_input.range_min.value, app.user_input.range_max.value
                ))
                .color(egui::Color32::from_rgb(100, 149, 237)),
            );
            if ui.small_button("Set Range\u{2026}").clicked() {
                app.user_input.range_window_open = true;
            }
        } else {
            ui.colored_label(egui::Color32::GRAY, "Full range");
            if ui.small_button("Set Range\u{2026}").clicked() {
                app.user_input.range_window_open = true;
            }
        }
    });
}

/// Displays scan filter information from the active file.
///
/// Shows polarity, scan type, and m/z range in a format similar to
/// Thermo's Qual Browser scan filter display.
pub fn add_scan_filter_dropdown(app: &mut MzViewerApp, ui: &mut Ui) {
    ui.label("Scan filter");

    // Get available scan filters from active file
    let available_filters = if let Some(active_id) = app.active_file_id {
        if let Some(file) = app.files.get(&active_id) {
            file.data.available_scan_filters.to_vec()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    if available_filters.is_empty() {
        ui.colored_label(egui::Color32::GRAY, "No file opened");
        return;
    }

    // Format current selection using the per-filter m/z range
    let current_selection = available_filters
        .iter()
        .find(|(lvl, pol, pre, _, _)| {
            *lvl == app.user_input.ms_level
                && *pol == app.user_input.polarity
                && *pre == app.user_input.precursor_mz
        })
        .map(|(lvl, pol, pre, lo, hi)| format_filter_label(lvl, pol, pre, lo, hi))
        .unwrap_or_else(|| {
            format!(
                "MS{} {}",
                app.user_input.ms_level,
                polarity_label(&app.user_input.polarity)
            )
        });

    egui::ComboBox::from_label("")
        .width(ui.available_width())
        .truncate()
        .selected_text(current_selection)
        .show_ui(ui, |ui| {
            for (ms_level, polarity, precursor, filter_min_mz, filter_max_mz) in &available_filters
            {
                let label = format_filter_label(
                    ms_level,
                    polarity,
                    precursor,
                    filter_min_mz,
                    filter_max_mz,
                );

                let is_selected = app.user_input.ms_level == *ms_level
                    && app.user_input.polarity == *polarity
                    && app.user_input.precursor_mz == *precursor;

                if ui.selectable_label(is_selected, &label).clicked() {
                    app.user_input.ms_level = *ms_level;
                    app.user_input.polarity = *polarity;
                    app.user_input.precursor_mz = *precursor;
                    app.state_changed = StateChange::Changed;
                    info!("Scan filter changed to: {}", label);
                }
            }
        });
}

/// Formats a scan filter label for display in the dropdown.
///
/// For MS1 (no precursor): `"MS1 Positive [m/z 150.00 - 1000.00]"`
/// For MS2 (with precursor): `"MS2 Positive 159.7894 [m/z 50.00 - 1000.00]"`
fn format_filter_label(
    ms_level: &u8,
    polarity: &ScanPolarity,
    precursor: &Option<f64>,
    lo: &f64,
    hi: &f64,
) -> String {
    match precursor {
        Some(mz) => format!(
            "MS{} {} {:.4} [m/z {:.2} - {:.2}]",
            ms_level,
            polarity_label(polarity),
            mz,
            lo,
            hi
        ),
        None => format!(
            "MS{} {} [m/z {:.2} - {:.2}]",
            ms_level,
            polarity_label(polarity),
            lo,
            hi
        ),
    }
}

/// Returns a human-readable label for the given scan polarity.
fn polarity_label(p: &ScanPolarity) -> &'static str {
    match p {
        ScanPolarity::Positive => "Positive",
        ScanPolarity::Negative => "Negative",
        _ => "Unknown",
    }
}

/// Adds the plot type options UI elements to the provided `Ui`.
///
/// This function renders the UI elements that allow the user to select the type of plot to display, such as TIC, Base Peak, or XIC. It updates the `user_input.plot_type` and related fields based on the user's selection.
///
/// # Parameters
///
/// - `ui`: A mutable reference to the `egui::Ui` object, which is used to render the UI elements.
pub fn add_plot_type_options(app: &mut MzViewerApp, ui: &mut Ui) {
    ui.label("Plot Type");
    ui.horizontal(|ui| {
        if ui
            .radio_value(&mut app.user_input.plot_type, PlotType::Tic, "TIC")
            .clicked()
        {
            app.user_input.plot_type = PlotType::Tic;
            app.state_changed = StateChange::Changed;
        }
        if ui
            .radio_value(&mut app.user_input.plot_type, PlotType::Bpc, "Base Peak")
            .clicked()
        {
            app.user_input.plot_type = PlotType::Bpc;
            app.state_changed = StateChange::Changed;
        }
        if ui
            .radio_value(&mut app.user_input.plot_type, PlotType::Xic, "XIC")
            .clicked()
        {
            app.user_input.plot_type = PlotType::Xic;
            app.options_window_open = true;
        }
    });
}
