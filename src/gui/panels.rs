use super::plotting;
use crate::gui::state::{
    next_color_for_index, FileCache, FileDisplaySettings, FileId, FileValidity, MzViewerApp,
    OpenFile, StateChange,
};
use crate::{
    parser,
    plotting_parameters::{LineColor, LineType, PlotType},
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
        ui.horizontal(|ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open").on_hover_text("Open a file").clicked() {
                    debug!("File open button clicked.");
                    app.reset_state();
                    handle_file_selection(app);
                    info!("File selection handled.");
                    ui.close();
                }

                ui.menu_button("Export", |ui| {
                    if ui
                        .button("Chromatogram")
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
                        let btn = ui.button("Mass Spectrum").on_hover_text(if has_spectrum {
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
                });
            });

            ui.menu_button("Display", |ui| {
                debug!("Display menu button clicked.");
                add_display_options(app, ui);
                info!("Display options added.");
            });

            {
                let is_dark = ui.visuals().dark_mode;
                if ui
                    .button(if is_dark { "☀ Light" } else { "🌙 Dark" })
                    .clicked()
                {
                    debug!("Visuals toggle button clicked.");
                    ctx.set_visuals(if is_dark {
                        egui::Visuals::light()
                    } else {
                        egui::Visuals::dark()
                    });
                    info!("Visuals updated.");
                }
            }
        });
    });
}

/// Adds the display options to the provided `egui::Ui` instance.
///
/// This function creates a series of menu buttons that allow the user to adjust the following display options:
/// - Smoothing level: Adjusts the level of moving average smoothing applied to the plot data.
/// - Line width: Adjusts the width of the lines in the plot.
/// - Line color: Allows the user to select the color of the lines in the plot.
/// - Line style: Allows the user to select the style of the lines in the plot.
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

    ui.menu_button("Line style", |ui| {
        debug!("Line style menu button clicked.");
        add_line_style_options(app, ui);
        info!("Line style options added.");
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

/// Adds the line style options to the provided `egui::Ui` instance.
///
/// This function creates a horizontal layout of radio buttons that allow the user to select the style of the lines in the plot.
/// The available line styles are: Solid, Dashed, and Dotted.
///
/// When the user selects a new line style, the function updates the `user_input.line_type` field accordingly.
///
/// # Parameters
/// - `&mut self`: A mutable reference to the current instance of the struct that contains the `user_input` field.
/// - `ui: &mut Ui`: A mutable reference to the `egui::Ui` instance where the line style options will be added.
pub fn add_line_style_options(app: &mut MzViewerApp, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.radio_value(&mut app.user_input.line_type, LineType::Solid, "Solid");
        ui.radio_value(&mut app.user_input.line_type, LineType::Dashed, "Dashed");
        ui.radio_value(&mut app.user_input.line_type, LineType::Dotted, "Dotted");
    });
    info!("Line style changed.")
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
        info!("Files selected: {} file(s)", paths.len());

        let mut first_new_file_id: Option<FileId> = None;

        for (color_index, path) in paths.iter().enumerate() {
            let path_str = path.display().to_string();

            if !path_str.to_lowercase().ends_with("mzml") {
                warn!("Invalid file format: {}", path_str);
                app.show_error_dialog(format!("Not an mzML file: {}", path_str));
                continue;
            }

            // Assign FileId and increment counter
            let file_id = app.next_file_id;
            app.next_file_id += 1;

            // Track the first file ID for setting as active
            if first_new_file_id.is_none() {
                first_new_file_id = Some(file_id);
            }

            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Loading…")
                .to_string();
            let color = next_color_for_index(color_index);

            // Insert placeholder immediately — spinner shows in UI right away
            app.files.insert(
                file_id,
                OpenFile {
                    id: file_id,
                    name,
                    path: path_str,
                    data: parser::MzData::new(),
                    display: FileDisplaySettings {
                        color,
                        visible: true,
                    },
                    cache: FileCache::default(),
                    is_loading: true,
                },
            );

            info!(
                "Placeholder inserted for file ID {}, spawning background thread",
                file_id
            );

            let tx_clone = app.async_state.file_loading_tx.clone();
            let path_clone = path.clone();
            std::thread::spawn(move || {
                let result = crate::processing::open_file_in_background(path_clone, file_id, color);
                let _ = tx_clone.send(result);
            });
        }

        // Set the active file to the first newly added file
        if let Some(first_id) = first_new_file_id {
            app.active_file_id = Some(first_id);
            app.invalid_file = FileValidity::Valid;

            // Ensure we're in a safe state for initial processing
            // If plot type is XIC but mass is invalid, switch to TIC
            if app.user_input.plot_type == PlotType::Xic && app.user_input.mass.value <= 0.0 {
                info!(
                    "Switching to TIC plot type for initial file opening (XIC requires valid mass)"
                );
                app.user_input.plot_type = PlotType::Tic;
            }

            app.state_changed = StateChange::Changed;
            info!("Active file set to ID: {}", first_id);
        }
    } else {
        warn!("No file selected. Setting file validity to Invalid.");
        app.invalid_file = FileValidity::Invalid;
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
    let default_name = active_file.name.replace(".mzML", "_chromatogram.csv");

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

    let default_name = active_file
        .name
        .replace(".mzML", &format!("_spectrum_{}.csv", rt_label));

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
    egui::SidePanel::left("file_information_panel").show(ctx, |ui| {
        ui.label("Opened files:");
        ui.separator();

        if app.files.is_empty() {
            ui.colored_label(Color32::GRAY, "No files opened");
        } else {
            let mut file_to_remove: Option<FileId> = None;
            let mut new_active_id: Option<FileId> = None;

            // Collect (id, file) pairs and sort by ID for consistent display order
            let mut files_sorted: Vec<_> = app.files.iter_mut().collect();
            files_sorted.sort_by_key(|(id, _)| **id);

            for (file_id, file) in files_sorted {
                let is_active = app.active_file_id == Some(*file_id);

                ui.horizontal(|ui| {
                    if file.is_loading {
                        ui.spinner();
                        ui.label(
                            egui::RichText::new(&file.name)
                                .small()
                                .italics()
                                .color(egui::Color32::GRAY),
                        );
                        // Skip close/select buttons while loading
                    } else {
                        // Highlight the active file
                        if is_active {
                            let frame = egui::Frame::default()
                                .fill(ui.visuals().selection.bg_fill)
                                .inner_margin(egui::Margin::same(4));
                            frame.show(ui, |ui| {
                                // Visibility checkbox
                                if ui.checkbox(&mut file.display.visible, "").changed() {
                                    info!(
                                        "File visibility toggled: {} (ID: {}) -> {}",
                                        file.name, file_id, file.display.visible
                                    );
                                }

                                // File name label (clickable to set as active)
                                if ui
                                    .selectable_label(true, egui::RichText::new(&file.name).small())
                                    .on_hover_text(format!("Active file (ID: {})", file_id))
                                    .clicked()
                                {
                                    new_active_id = Some(*file_id);
                                }

                                // Close button
                                if ui.small_button("❌").on_hover_text("Close file").clicked() {
                                    file_to_remove = Some(*file_id);
                                    info!(
                                        "Close button clicked for file: {} (ID: {})",
                                        file.name, file_id
                                    );
                                }
                            });
                        } else {
                            // Visibility checkbox
                            if ui.checkbox(&mut file.display.visible, "").changed() {
                                info!(
                                    "File visibility toggled: {} (ID: {}) -> {}",
                                    file.name, file_id, file.display.visible
                                );
                            }

                            // File name label (clickable to set as active)
                            if ui
                                .selectable_label(false, egui::RichText::new(&file.name).small())
                                .on_hover_text(format!(
                                    "Click to set as active file (ID: {})",
                                    file_id
                                ))
                                .clicked()
                            {
                                new_active_id = Some(*file_id);
                            }

                            // Close button
                            if ui.small_button("❌").on_hover_text("Close file").clicked() {
                                file_to_remove = Some(*file_id);
                                info!(
                                    "Close button clicked for file: {} (ID: {})",
                                    file.name, file_id
                                );
                            }
                        }
                    }
                });
            }

            // Update active ID if a file was clicked
            if let Some(new_id) = new_active_id {
                if app.active_file_id != Some(new_id) {
                    app.active_file_id = Some(new_id);
                    app.state_changed = StateChange::Changed;
                    // Sync the color picker to reflect the newly active file's color
                    if let Some(file) = app.files.get(&new_id) {
                        app.user_input.line_color = file.display.color;
                    }
                    info!("Active file changed to ID: {}", new_id);
                }
            }

            // Remove file if close button was clicked
            if let Some(id) = file_to_remove {
                if let Some(removed_file) = app.files.remove(&id) {
                    info!(
                        "File removed: {} (ID: {}). FileIds of remaining files are unaffected.",
                        removed_file.name, id
                    );

                    // If we removed the active file, set a new active file or None
                    if app.active_file_id == Some(id) {
                        // Set active to the first remaining file (by lowest ID), or None if empty
                        app.active_file_id = app.files.keys().min().copied();

                        if let Some(new_active) = app.active_file_id {
                            // Sync color picker to the newly active file's color
                            if let Some(new_file) = app.files.get(&new_active) {
                                app.user_input.line_color = new_file.display.color;
                            }
                            info!("Active file changed to ID: {} after removal", new_active);
                        } else {
                            info!("No files remain after removal");
                        }
                    }

                    // Update validity state
                    if app.files.is_empty() {
                        app.invalid_file = FileValidity::Invalid;
                        app.user_input.line_color = LineColor::default();
                        app.user_input.retention_time_ms_spectrum = None;
                    }
                    app.integration = crate::gui::state::IntegrationState::default();
                }
            }
        }
    });
}

/// Updates the central panel of the user interface.
///
/// This function is responsible for rendering the main content area of the application, which includes the chromatogram and mass spectrum plots.
///
/// # Parameters
///
/// - `ctx`: A reference to the `egui::Context` object, which is used to render the user interface.
///
/// # Functionality
///
/// 1. Displays a `CentralPanel` that fills the available space in the center of the application.
/// 2. Adds a `ScrollArea` to the central panel, allowing the user to scroll the content if it exceeds the available space.
/// 3. Renders a `CollapsingHeader` for the chromatogram plot, which can be expanded or collapsed by the user.
///    - Calls the `plot_chromatogram()` function to generate the chromatogram plot.
///    - Adds a context menu to the chromatogram plot, which allows the user to access the plot properties.
///    - Calls the `add_plot_properties()` function to add the plot properties to the context menu.
/// 4. Adds some vertical space between the chromatogram and mass spectrum plots.
/// 5. Renders a `CollapsingHeader` for the mass spectrum plot, which can be expanded or collapsed by the user.
///    - Calls the `plot_mass_spectrum()` function to generate the mass spectrum plot.
///
/// # Errors
///
/// This function does not return any errors. It handles the rendering of the central panel and the associated plots within the user interface.
pub fn update_central_panel(app: &mut MzViewerApp, ctx: &Context) {
    debug!("Updating central panel.");
    egui::CentralPanel::default().show(ctx, |ui| {
        egui::ScrollArea::both().show(ui, |ui| {
            egui::CollapsingHeader::new("Chromatogram")
                .default_open(true)
                .show(ui, |ui| {
                    debug!("Plotting chromatogram.");
                    let chromatogram = plotting::plot_chromatogram(app, ui, ctx);
                    if chromatogram.secondary_clicked() {
                        app.plot_properties_open = true;
                    }

                    // Integration result bar — shown only when relevant
                    match (
                        app.integration.start_rt,
                        app.integration.end_rt,
                        app.integration.result,
                    ) {
                        (Some(s), Some(e), Some(area)) => {
                            ui.horizontal(|ui| {
                                ui.colored_label(
                                    egui::Color32::from_rgb(60, 200, 60),
                                    format!("Peak Area [{:.3} – {:.3} min] = {:.4e}", s, e, area),
                                );
                                if ui.small_button("✕ Clear").clicked() {
                                    app.integration.start_rt = None;
                                    app.integration.end_rt = None;
                                    app.integration.result = None;
                                }
                            });
                        }
                        (Some(s), None, _) => {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                format!("∫ Drag to set end … (start: {:.3} min)", s),
                            );
                        }
                        _ => {}
                    }

                    info!("Chromatogram plotted successfully.");
                });

            ui.add_space(5.0); // Add some space between the plots

            egui::CollapsingHeader::new("Mass Spectrum")
                .default_open(true)
                .show(ui, |ui| {
                    debug!("Plotting mass spectrum.");
                    plotting::plot_mass_spectrum(app, ui);
                    info!("Mass spectrum plotted successfully.");
                });
        });
    });
    info!("Central panel updated successfully.");
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
    ui.label("Scan Filter:");

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
        .selected_text(current_selection)
        .show_ui(ui, |ui| {
            for (ms_level, polarity, precursor, filter_min_mz, filter_max_mz) in &available_filters
            {
                let label = format_filter_label(ms_level, polarity, precursor, filter_min_mz, filter_max_mz);

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
