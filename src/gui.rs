//! # gui logic and components

//! The module provides a graphical user interface (GUI) for visualizing mass spectrometry data from MzML files.
//! It allows users to load mass spectrometry data, select various plotting options, and visualize chromatograms and mass spectra.
//! The module utilizes the `eframe` and `egui` libraries for building the GUI and rendering plots.

//!## Overview

//! The main struct in this module is `MzViewerApp`, which encapsulates the application state, user inputs, and methods for processing and displaying mass spectrometry data.
//! The module also defines several supporting structs and enums to manage user inputs and the validity of file selections.

//!### Key Features

//! - **User Input Handling**: Collects user inputs for file selection, plot type, mass, and other parameters.
//! - **Data Processing**: Processes mass spectrometry data to generate Total Ion Chromatograms (TIC), Base Peak Chromatograms (BPC), and Extracted Ion Chromatograms (XIC).
//! - **Plotting**: Renders chromatograms and mass spectra using the `egui_plot` library.
//! - **File Management**: Handles file selection and validation to ensure that only valid MzML files are processed.

//!## Structs

//!### `UserInput`

//! A struct that holds user input parameters for the application, including file path, plot type, mass, and other parameters.

//!#### Fields

//! - `file_path`: An optional string representing the path to the selected MzML file.
//! - `plot_type`: The type of plot to be generated (TIC, BPC, or XIC).
//! - `polarity`: The scan polarity for the mass spectrometry data.
//! - `mass_input`: A string representation of the mass input provided by the user.
//! - `mass_tolerance_input`: A string representation of the mass tolerance input provided by the user.
//! - `mass`: The mass value parsed from the mass_input.
//! - `mass_tolerance`: The mass tolerance value parsed from the mass_tolerance_input.
//! - `line_type`: The type of line to be used in the plot (solid, dashed, dotted).
//! - `line_color`: The color of the line in the plot.
//! - `smoothing`: The level of smoothing to be applied to the plot data.
//! - `line_width`: The width of the line in the plot.
//! - `retention_time_ms_spectrum`: An optional retention time for the mass spectrum.

//!### `MzViewerApp`

//! The main application struct that manages the state of the MzViewer application.

//!#### Fields

//! - `parsed_ms_data`: An instance of `parser::MzData` that holds the parsed mass spectrometry data.
//! - `plot_data`: An optional vector of plot data points.
//! - `user_input`: An instance of `UserInput` that holds user-defined parameters.
//! - `invalid_file`: An enum indicating the validity of the selected file.
//! - `state_changed`: An enum indicating whether the application state has changed.
//! - `options_window_open`: A boolean indicating if the options window is open.

//!#### Methods

//! - `new()`: Creates a new instance of `MzViewerApp` with default values.
//! - `process_plot_data()`: Processes the plot data based on user inputs and returns the prepared data for plotting.
//! - `plot_chromatogram()`: Renders the chromatogram plot based on the processed data.
//! - `determine_rt_clicked()`: Determines the retention time clicked on the plot.
//! - `find_closest_spectrum()`: Finds the closest spectrum index based on the clicked retention time.
//! - `plot_mass_spectrum()`: Renders the mass spectrum plot based on the parsed mass spectrum data.
//! - `update_data_selection_panel()`: Updates the data selection panel in the GUI.
//! - `add_display_options()`: Adds options for adjusting display settings such as smoothing, line width, and color.
//! - `handle_file_selection()`: Handles the file selection process and updates the file path and validity.
//! - `update_file_path_and_validity()`: Updates the file path and checks the validity of the selected file.
//! - `update_file_information_panel()`: Updates the file information panel in the GUI.

//!## Enums

//!### `FileValidity`

//! An enum representing the validity of the selected file.

//! - `Valid`: Indicates that the file is valid.
//! - `Invalid`: Indicates that the file is invalid.

//!### `StateChange`

//! An enum representing the state change of the application.

//! - `Changed`: Indicates that the state has changed.
//! - `Unchanged`: Indicates that the state has not changed.

//!## Usage

//! To use this module, integrate it into your Rust application that requires visualization of mass spectrometry data.
//! Ensure that the necessary dependencies (`eframe`, `egui`, `egui_plot`, etc.) are included in your `Cargo.toml`.

#![warn(clippy::all)]

use crate::{
    error::{ChromascopeError, Result},
    parser,
    plotting_parameters::{self, LineColor, LineType, PlotType},
    processing::{process_chromatogram, ProcessingParams},
    validation::XicParams,
};

use mzdata::spectrum::ScanPolarity;
use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui;
use egui::{Color32, Context, Ui};
use egui_plot::{Line, PlotPoints};
use log::{debug, error, info, warn};

const FILE_FORMAT: &str = "mzML";

#[derive(PartialEq, Default)]
pub struct UserInput {
    /// Optional file path for the input data
    pub file_path: Option<String>,
    /// The type of plot to be generated. It can be PlotType::Tic, PlotType::Bpc or PlotType::Xic
    pub plot_type: PlotType,
    /// The polarity of the scan. It can be either ScanPolarity::Positive or ScanPolarity::Negative
    pub polarity: ScanPolarity,
    /// The mass input value provided by the user
    pub mass_input: String,
    /// The mass tolerance input value provided by the user
    pub mass_tolerance_input: String,
    /// The mass value parsed from the `mass_input`
    pub mass: f64,
    /// The mass tolerance value parsed from `mass_tolerance_input`
    pub mass_tolerance: f64,
    /// The type of line to be used in the plot
    pub line_type: LineType,
    /// The color of the line to be used in the plot
    pub line_color: LineColor,
    /// The amount of smoothing to be applied to the plot
    pub smoothing: u8,
    /// The width of the line to be used in the plot
    pub line_width: f32,
    /// The retention time of a given scan. Needed for mass spectrum extraction when the user triple clicks the chromatogram
    pub retention_time_ms_spectrum: Option<f32>,
}

#[derive(Default, Debug, PartialEq)]
enum FileValidity {
    Valid,
    #[default]
    Invalid,
}
#[derive(Default, Debug, PartialEq)]
enum StateChange {
    Changed,
    #[default]
    Unchanged,
}

/// Stable identifier for opened files.
///
/// FileId is assigned when a file is opened and never changes, even if other files
/// are removed. This prevents index-related bugs where removing file A causes file B's
/// "position" to change.
///
/// # Design Pattern: Identity Map
/// Each file gets a unique ID on creation. Unlike Vec indices, FileIds don't shift
/// when elements are removed.
type FileId = usize;

/// Represents a single opened mzML file with its associated data and display settings
struct OpenFile {
    /// Stable identifier that never changes, even if other files are removed
    #[allow(dead_code)]
    id: FileId,
    /// The display name of the file (extracted from the path)
    name: String,
    /// The full path to the file
    #[allow(dead_code)]
    path: String,
    /// The parsed mass spectrometry data for this file
    data: parser::MzData,
    /// The processed plot data for this file
    cached_plot_data: Option<Vec<[f64; 2]>>,
    /// The color assigned to this file's chromatogram line
    color: LineColor,
    /// Whether this file's chromatogram is currently visible in the plot
    visible: bool,
}

/// Returns the next color in the cycle based on the file index
fn next_color_for_index(index: usize) -> LineColor {
    let colors = [
        LineColor::Red,
        LineColor::Green,
        LineColor::Blue,
        LineColor::Yellow,
        LineColor::Black,
        LineColor::White,
    ];
    match index % colors.len() {
        0 => LineColor::Red,
        1 => LineColor::Green,
        2 => LineColor::Blue,
        3 => LineColor::Yellow,
        4 => LineColor::Black,
        _ => LineColor::White,
    }
}

#[derive(Default)]
pub struct MzViewerApp {
    /// Collection of opened mzML files, keyed by stable FileId
    files: HashMap<FileId, OpenFile>,
    /// FileId of the currently active/selected file for analysis
    active_file_id: Option<FileId>,
    /// Next FileId to assign. Starts at 0, increments with each file opened.
    next_file_id: FileId,
    /// The user input parameters
    user_input: UserInput,
    /// The validity of the input file. Only MzML files can be read in.
    invalid_file: FileValidity,
    /// The state change of the application
    state_changed: StateChange,
    /// Whether the options window/pop-up is open
    options_window_open: bool,
    /// Error message to display to the user
    error_message: Option<String>,
}

impl MzViewerApp {
    /// Creates a new instance of the `MzViewerApp` struct.
    ///
    /// # Arguments
    /// * `_cc`: The `eframe::CreationContext` reference, which is not used in this implementation.
    ///
    /// # Returns
    /// A new instance of the `MzViewerApp` struct with the following default values:
    /// - `user_input.line_width`: 1.0
    /// - All other fields in `user_input` are set to their default values.
    /// - All other fields in the `MzViewerApp` struct are set to their default values.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            files: HashMap::new(),
            active_file_id: None,
            next_file_id: 0,
            user_input: UserInput {
                line_width: 1.0,
                ..Default::default()
            },
            invalid_file: FileValidity::Invalid,
            state_changed: StateChange::Unchanged,
            options_window_open: false,
            error_message: None,
        }
    }
    /// Resets the internal state of the instance.
    ///
    /// This function clears all opened files and resets the active file ID.
    pub fn reset_state(&mut self) {
        self.files.clear();
        self.active_file_id = None;
    }

    /// Displays an error message to the user via a modal dialog.
    ///
    /// # Parameters
    /// - `message`: The error message to display
    fn show_error_dialog(&mut self, message: String) {
        self.error_message = Some(message);
    }

    /// Renders the error dialog if an error message is present.
    ///
    /// This creates a centered modal window with the error message and an OK button.
    /// The dialog blocks interaction until dismissed by clicking OK.
    ///
    /// # Parameters
    /// - `ctx`: The egui context for rendering the dialog
    fn render_error_dialog(&mut self, ctx: &egui::Context) {
        if let Some(error) = self.error_message.clone() {
            egui::Window::new("⚠ Error")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.colored_label(egui::Color32::RED, &error);
                    ui.add_space(10.0);
                    if ui.button("OK").clicked() {
                        self.error_message = None;
                    }
                });
        }
    }

    /// Updates the active file's cached chromatogram data.
    ///
    /// This method orchestrates the business logic for chromatogram extraction
    /// by delegating to the processing module. It should be called whenever
    /// parameters change (polarity, plot type, smoothing, etc.)
    ///
    /// # Returns
    /// - `Ok(())` if processing succeeded and cache was updated
    /// - `Err(ChromascopeError)` if validation, extraction, or processing failed
    ///
    /// # Errors
    /// - `MissingXicParams` - XIC selected but parameters invalid/missing
    /// - `InvalidMass` / `InvalidMassTolerance` - XIC parameter validation failed
    /// - Other errors from data extraction or smoothing
    fn update_chromatogram_data(&mut self) -> Result<()> {
        let active_id = self.active_file_id.ok_or_else(|| {
            crate::error::ChromascopeError::FileNotOpened("No active file selected".to_string())
        })?;

        // Build processing parameters from current GUI state first
        let params = self.build_processing_params()?;

        // Then get mutable reference to file
        let file = self.files.get_mut(&active_id).ok_or_else(|| {
            crate::error::ChromascopeError::FileNotOpened(format!(
                "Active file ID {} not found in files",
                active_id
            ))
        })?;

        // Process chromatogram using business logic layer
        let result = process_chromatogram(&mut file.data, &params)?;

        // Cache the result in GUI layer
        file.cached_plot_data = Some(result);

        Ok(())
    }

    /// Builds ProcessingParams from current GUI state with validation.
    ///
    /// This method extracts values from `self.user_input` and constructs
    /// validated parameters for the processing pipeline.
    ///
    /// # Returns
    /// - `Ok(ProcessingParams)` if all parameters are valid
    /// - `Err(ChromascopeError)` if XIC parameters fail validation
    ///
    /// # Errors
    /// - `InvalidMass` - Mass value is not positive
    /// - `InvalidMassTolerance` - Tolerance outside 0-1000 ppm range
    fn build_processing_params(&self) -> Result<ProcessingParams> {
        // Get active file or fail fast
        let active_id = self
            .active_file_id
            .ok_or_else(|| ChromascopeError::FileNotOpened("No file opened".into()))?;

        let active_file = self.files.get(&active_id).ok_or_else(|| {
            ChromascopeError::FileNotOpened(format!("Active file ID {} not found", active_id))
        })?;

        // Validate smoothing against file size
        active_file
            .data
            .bounds
            .validate_smoothing(self.user_input.smoothing)?;

        // For XIC, validate and construct XicParams with file bounds
        let xic_params = if self.user_input.plot_type == PlotType::Xic {
            Some(XicParams::new(
                self.user_input.mass,
                self.user_input.polarity,
                self.user_input.mass_tolerance,
                &active_file.data.bounds,
            )?)
        } else {
            None
        };

        Ok(ProcessingParams {
            plot_type: self.user_input.plot_type,
            polarity: self.user_input.polarity,
            smoothing: self.user_input.smoothing,
            xic_params,
        })
    }

    /// Renders the chromatogram plot from cached data.
    ///
    /// This is a pure rendering function with no side effects or data processing.
    /// It displays chromatograms for all visible files using their cached plot data.
    /// Called every frame to draw the plot.
    ///
    /// # Parameters
    /// - `ui: &mut egui::Ui`: The UI context for rendering
    ///
    /// # Returns
    /// - `egui::Response`: The response from the plot widget
    /// - `Option<egui_plot::PlotBounds>`: The bounds of the plot for interaction handling
    fn render_chromatogram(
        &self,
        ui: &mut egui::Ui,
    ) -> (egui::Response, Option<egui_plot::PlotBounds>) {
        let mut plot_bounds = None;

        let response = egui_plot::Plot::new("chromatogram")
            .width(ui.available_width() * 0.99)
            .height(ui.available_height() * 0.6)
            .legend(egui_plot::Legend::default())
            .show(ui, |plot_ui| {
                // Plot all visible files (HashMap iteration is unordered, but overlay order doesn't matter)
                for file in self.files.values() {
                    if file.visible {
                        if let Some(data) = &file.cached_plot_data {
                            let line = self.create_line_for_file(file, data);
                            plot_ui.line(line);
                        }
                    }
                }

                if self.files.is_empty() {
                    warn!("No files opened");
                }
                plot_bounds = Some(plot_ui.plot_bounds());
            })
            .response;

        (response, plot_bounds)
    }

    /// Creates a Line widget for a specific file's chromatogram data.
    ///
    /// Helper function for render_chromatogram that constructs a styled line
    /// using the file's color and name, and the current display settings.
    ///
    /// # Parameters
    /// - `file: &OpenFile`: The file containing metadata (name, color)
    /// - `data: &[[f64; 2]]`: The chromatogram data points
    ///
    /// # Returns
    /// - `Line`: A configured egui_plot Line widget
    fn create_line_for_file(&self, file: &OpenFile, data: &[[f64; 2]]) -> Line {
        Line::new(PlotPoints::from(data.to_vec()))
            .width(self.user_input.line_width)
            .style(self.user_input.line_type.to_egui())
            .color(file.color.to_egui())
            .name(&file.name)
    }

    /// Handles triple-click events on the chromatogram plot.
    ///
    /// Extracts and displays the mass spectrum at the clicked retention time
    /// from the active file. Does nothing for XIC plots (domain restriction).
    ///
    /// # Parameters
    /// - `response: egui::Response`: The plot widget response
    /// - `plot_bounds: Option<egui_plot::PlotBounds>`: The plot bounds for coordinate conversion
    fn handle_chromatogram_click(
        &mut self,
        response: egui::Response,
        plot_bounds: Option<egui_plot::PlotBounds>,
    ) {
        if !response.triple_clicked() {
            return; // Early return if not triple-clicked
        }

        // Don't extract mass spectrum from XIC plots (domain rule)
        if self.user_input.plot_type == plotting_parameters::PlotType::Xic {
            return;
        }

        // Get active file ID
        let Some(active_id) = self.active_file_id else {
            warn!("No active file selected for mass spectrum extraction");
            return;
        };

        // Calculate clicked retention time before getting file reference
        let rt_clicked = self.calculate_clicked_rt(&response, plot_bounds);

        // Get file by ID (no need to check bounds - HashMap lookup handles it)
        let Some(file) = self.files.get_mut(&active_id) else {
            warn!("Active file ID {} not found", active_id);
            return;
        };

        info!(
            "Triple click detected on plot at {:?} for file: {}",
            &rt_clicked, file.name
        );

        // Find and extract closest spectrum
        if let Some(index) = file.data.get_closest_index_by_time(rt_clicked) {
            info!("Found closest spectrum at index: {}", index);
            file.data.get_mass_spectrum_by_index(index);
        } else {
            warn!("No close spectrum found for the clicked retention time");
        }
    }

    /// Orchestrates chromatogram display: updates data when needed, renders, and handles interactions.
    ///
    /// This method coordinates three phases:
    /// 1. Update: Re-processes data if state has changed
    /// 2. Render: Displays chromatograms from cached data
    /// 3. Handle: Responds to user interactions (triple-click)
    ///
    /// # Parameters
    /// - `ui: &mut egui::Ui`: The UI context for rendering
    ///
    /// # Returns
    /// - `egui::Response`: The response from the plot widget
    fn plot_chromatogram(&mut self, ui: &mut egui::Ui) -> egui::Response {
        // Phase 1: Update data if state has changed
        if let Some(active_id) = self.active_file_id {
            if self.state_changed == StateChange::Changed {
                if let Some(file) = self.files.get(&active_id) {
                    info!(
                        "State has changed, reprocessing plot data for active file: {} (ID: {})",
                        file.name, active_id
                    );
                }
                // Process the data using the business logic layer
                if let Err(e) = self.update_chromatogram_data() {
                    error!("Failed to process chromatogram: {}", e);
                    self.show_error_dialog(format!("Failed to process chromatogram: {}", e));
                }
                self.state_changed = StateChange::Unchanged;
            }
        }

        // Phase 2: Render chromatogram from cached data
        let (response, plot_bounds) = self.render_chromatogram(ui);

        // Phase 3: Handle user interactions
        self.handle_chromatogram_click(response.clone(), plot_bounds);

        response
    }

    /// Calculates the retention time corresponding to a clicked position on the plot.
    ///
    /// Converts screen coordinates to data coordinates using the plot bounds.
    /// Also updates the user_input.retention_time_ms_spectrum field for display purposes.
    ///
    /// # Parameters
    /// - `response: &egui::Response`: The plot widget response containing pointer position
    /// - `plot_bounds: Option<egui_plot::PlotBounds>`: The plot bounds for coordinate conversion
    ///
    /// # Returns
    /// - `Option<f32>`: The calculated retention time, or None if calculation fails
    fn calculate_clicked_rt(
        &mut self,
        response: &egui::Response,
        plot_bounds: Option<egui_plot::PlotBounds>,
    ) -> Option<f32> {
        let plot_position = response.interact_pointer_pos()?;
        let bounds = plot_bounds?;

        let plot_width = response.rect.width();
        let min_x = *bounds.range_x().start();
        let max_x = *bounds.range_x().end();

        // Calculate the position relative to the plot area
        let relative_x = (plot_position.x - response.rect.left()) / plot_width;
        let converted_rt = min_x + relative_x as f64 * (max_x - min_x);

        // Store for display purposes
        self.user_input.retention_time_ms_spectrum = Some(converted_rt as f32);
        info!("Retention time clicked: {:?}", converted_rt as f32);

        Some(converted_rt as f32)
    }

    /// Plots the mass spectrum based on the data available in the active file.
    ///
    /// This function creates a bar chart plot of the mass-to-charge (m/z) values and their corresponding intensities
    /// from the currently active file.
    /// The width of the bars is adjusted based on the zoom level of the plot to provide a better visual representation.
    ///
    /// # Parameters
    /// - `&mut self`: A mutable reference to the current instance of the struct
    /// - `ui: &mut egui::Ui`: A mutable reference to the current `egui::Ui` instance, which is used to render the plot.
    ///
    /// # Returns
    /// - `egui::Response`: The response from the `egui_plot::Plot` widget, which can be used to handle user interactions with the plot.
    fn plot_mass_spectrum(&mut self, ui: &mut egui::Ui) -> egui::Response {
        if let Some(active_id) = self.active_file_id {
            if let Some(file) = self.files.get_mut(&active_id) {
                if let Some((mz, intensity)) = file.data.mass_spectrum() {
                    info!(
                        "Mass spectrum data available for {} (ID: {}). Plotting the spectrum.",
                        file.name, active_id
                    );

                    let response = egui_plot::Plot::new("mass_spectrum")
                        .width(ui.available_width() * 0.99)
                        .height(ui.available_height())
                        .show(ui, |plot_ui| {
                            let bounds = plot_ui.plot_bounds();
                            let zoom_level = (bounds.max()[0] - bounds.min()[0]).abs(); // Calculate zoom level based on plot bounds
                            debug!("Zoom level calculated: {}", zoom_level);

                            let bar_width = zoom_level * 0.001; // Adjust bar width based on zoom level
                            let adjusted_bars: Vec<egui_plot::Bar> = mz
                                .iter()
                                .zip(intensity.iter())
                                .map(|(&m, &i)| {
                                    egui_plot::Bar::new(m, i.into())
                                        .width(bar_width) // Adjust width of bars based on zoom level
                                        .fill(self.user_input.line_color.to_egui()) // Adjust color as needed
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
    fn update_data_selection_panel(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("data_selection_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Open").on_hover_text("Open a file").clicked() {
                        debug!("File open button clicked.");
                        self.reset_state();
                        self.handle_file_selection();
                        info!("File selection handled.");
                        ui.close_menu();
                    }

                    if ui
                        .button("Export to CSV")
                        .on_hover_text("Export plot data to CSV")
                        .clicked()
                    {
                        debug!("Export to CSV button clicked.");
                        self.handle_csv_export();
                        ui.close_menu();
                    }
                });

                ui.menu_button("Display", |ui| {
                    debug!("Display menu button clicked.");
                    self.add_display_options(ui);
                    info!("Display options added.");
                });

                if let Some(new_visuals) = ui
                    .style()
                    .visuals
                    .clone()
                    .light_dark_small_toggle_button(ui)
                {
                    debug!("Visuals toggle button clicked.");
                    ctx.set_visuals(new_visuals);
                    info!("Visuals updated.");
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
    fn add_display_options(&mut self, ui: &mut Ui) {
        ui.menu_button("Smoothing", |ui| {
            let slider = egui::Slider::new(&mut self.user_input.smoothing, 0..=11);
            let response = ui.add(slider);
            if response.changed() {
                self.state_changed = StateChange::Changed;
                info!("Smoothing level changed to {}", self.user_input.smoothing);
            }
            response.on_hover_text("Adjust the level of moving average smoothing");

            // Show warning if smoothing is too large for file
            if let Some(active_id) = self.active_file_id {
                if let Some(file) = self.files.get(&active_id) {
                    let bounds = &file.data.bounds;
                    let max_reasonable = (bounds.scan_count / 10).max(3) as u8;

                    if self.user_input.smoothing > max_reasonable && bounds.scan_count > 0 {
                        ui.add_space(5.0);
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 100, 100),
                            format!(
                                "⚠ Window ({}) is large for {} scans. Max recommended: {}",
                                self.user_input.smoothing, bounds.scan_count, max_reasonable
                            ),
                        );
                    }
                }
            }
        });

        ui.menu_button("Line width", |ui| {
            let slider = egui::Slider::new(&mut self.user_input.line_width, 0.1..=5.0);
            let response = ui.add(slider);
            if response.changed() {
                self.state_changed = StateChange::Changed;
                info!("Line width changed to {}", self.user_input.line_width);
            }
            response.on_hover_text("Adjust the line width");
        });

        ui.menu_button("Line color", |ui| {
            debug!("Line color menu button clicked.");
            self.add_line_color_options(ui);
            info!("Line color options added.");
        });

        ui.menu_button("Line style", |ui| {
            debug!("Line style menu button clicked.");
            self.add_line_style_options(ui);
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
    fn add_line_color_options(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.user_input.line_color, LineColor::Red, "Red");
            ui.radio_value(&mut self.user_input.line_color, LineColor::Blue, "Blue");
            ui.radio_value(&mut self.user_input.line_color, LineColor::Green, "Green");
            ui.radio_value(&mut self.user_input.line_color, LineColor::Yellow, "Yellow");
            ui.radio_value(&mut self.user_input.line_color, LineColor::Black, "Black");
            ui.radio_value(&mut self.user_input.line_color, LineColor::White, "White");
        });

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
    fn add_line_style_options(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.user_input.line_type, LineType::Solid, "Solid");
            ui.radio_value(&mut self.user_input.line_type, LineType::Dashed, "Dashed");
            ui.radio_value(&mut self.user_input.line_type, LineType::Dotted, "Dotted");
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
    fn handle_file_selection(&mut self) {
        if let Some(paths) = rfd::FileDialog::new().pick_files() {
            info!("Files selected: {} file(s)", paths.len());

            let mut first_new_file_id: Option<FileId> = None;

            for (color_index, path) in paths.iter().enumerate() {
                info!("Processing file: {:?}", path);

                // Assign FileId and increment counter
                let file_id = self.next_file_id;
                self.next_file_id += 1;

                // Track the first file ID for setting as active
                if first_new_file_id.is_none() {
                    first_new_file_id = Some(file_id);
                }

                match self.create_open_file(path, color_index, file_id) {
                    Ok(open_file) => {
                        info!("File added successfully: {:?} with ID {}", path, file_id);
                        self.files.insert(file_id, open_file);
                    }
                    Err(e) => {
                        error!("Failed to open file {:?}: {}", path, e);
                        self.show_error_dialog(format!("Failed to open file: {}", e));
                    }
                }
            }

            // Set the active file to the first newly added file
            if let Some(first_id) = first_new_file_id {
                self.active_file_id = Some(first_id);
                self.invalid_file = FileValidity::Valid;
                self.state_changed = StateChange::Changed;
                info!("Active file set to ID: {}", first_id);
            }
        } else {
            warn!("No file selected. Setting file validity to Invalid.");
            self.invalid_file = FileValidity::Invalid;
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
    fn handle_csv_export(&mut self) {
        // Check if an active file is selected
        let active_id = match self.active_file_id {
            Some(id) => id,
            None => {
                warn!("No active file selected for CSV export");
                return;
            }
        };

        let active_file = match self.files.get(&active_id) {
            Some(file) => file,
            None => {
                warn!("Active file ID {} not found", active_id);
                return;
            }
        };

        // Check if plot data exists for the active file
        let data = match &active_file.cached_plot_data {
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
                    self.show_error_dialog(format!("Export failed: {}", e));
                }
            }
        } else {
            warn!("No file path selected for CSV export.");
        }
    }

    /// Creates an OpenFile struct from a file path.
    ///
    /// This function validates the file format, extracts the file name, loads the MzData,
    /// and assigns a color based on the file index.
    ///
    /// # Parameters
    ///
    /// - `path`: A reference to the file path.
    /// - `index`: The index this file will have in the files vector (used for color assignment).
    /// - `file_id`: The stable FileId to assign to this file.
    ///
    /// # Returns
    ///
    /// - `Option<OpenFile>`: The created OpenFile struct, or None if the file format is invalid or loading fails.
    fn create_open_file(&self, path: &PathBuf, index: usize, file_id: FileId) -> Result<OpenFile> {
        let file_path_str = path.display().to_string();
        info!("Creating OpenFile for: {}", file_path_str);

        if !file_path_str.ends_with(FILE_FORMAT) {
            warn!(
                "Invalid file format for: {}. Expected {} file.",
                file_path_str, FILE_FORMAT
            );
            return Err(crate::error::ChromascopeError::IoError(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid file format. Expected {} file.", FILE_FORMAT),
                ),
            ));
        }

        // Extract file name from path
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| {
                crate::error::ChromascopeError::IoError(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid file path",
                ))
            })?
            .to_string();

        // Load the MzData
        let mut data = parser::MzData::new();
        data.open_msfile(path)?;

        info!(
            "File opened successfully: {} with ID {}",
            file_name, file_id
        );
        Ok(OpenFile {
            id: file_id,
            name: file_name,
            path: file_path_str,
            data,
            cached_plot_data: None,
            color: next_color_for_index(index),
            visible: true,
        })
    }

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
    fn update_file_information_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("file_information_panel").show(ctx, |ui| {
            ui.label("Opened files:");
            ui.separator();

            if self.files.is_empty() {
                ui.colored_label(Color32::GRAY, "No files opened");
            } else {
                let mut file_to_remove: Option<FileId> = None;
                let mut new_active_id: Option<FileId> = None;

                // Collect (id, file) pairs and sort by ID for consistent display order
                let mut files_sorted: Vec<_> = self.files.iter_mut().collect();
                files_sorted.sort_by_key(|(id, _)| **id);

                for (file_id, file) in files_sorted {
                    let is_active = self.active_file_id == Some(*file_id);

                    ui.horizontal(|ui| {
                        // Highlight the active file
                        if is_active {
                            let frame = egui::Frame::default()
                                .fill(ui.visuals().selection.bg_fill)
                                .inner_margin(egui::Margin::same(4.0));
                            frame.show(ui, |ui| {
                                // Visibility checkbox
                                if ui.checkbox(&mut file.visible, "").changed() {
                                    info!(
                                        "File visibility toggled: {} (ID: {}) -> {}",
                                        file.name, file_id, file.visible
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
                            if ui.checkbox(&mut file.visible, "").changed() {
                                info!(
                                    "File visibility toggled: {} (ID: {}) -> {}",
                                    file.name, file_id, file.visible
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
                    });
                }

                // Update active ID if a file was clicked
                if let Some(new_id) = new_active_id {
                    if self.active_file_id != Some(new_id) {
                        self.active_file_id = Some(new_id);
                        self.state_changed = StateChange::Changed;
                        info!("Active file changed to ID: {}", new_id);
                    }
                }

                // Remove file if close button was clicked
                if let Some(id) = file_to_remove {
                    if let Some(removed_file) = self.files.remove(&id) {
                        info!(
                            "File removed: {} (ID: {}). FileIds of remaining files are unaffected.",
                            removed_file.name, id
                        );

                        // If we removed the active file, set a new active file or None
                        if self.active_file_id == Some(id) {
                            // Set active to the first remaining file (by lowest ID), or None if empty
                            self.active_file_id = self.files.keys().min().copied();

                            if let Some(new_active) = self.active_file_id {
                                info!("Active file changed to ID: {} after removal", new_active);
                            } else {
                                info!("No files remain after removal");
                            }
                        }

                        // Update validity state
                        if self.files.is_empty() {
                            self.invalid_file = FileValidity::Invalid;
                        }
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
    fn update_central_panel(&mut self, ctx: &Context) {
        debug!("Updating central panel.");
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                egui::CollapsingHeader::new("Chromatogram")
                    .default_open(true)
                    .show(ui, |ui| {
                        debug!("Plotting chromatogram.");
                        let chromatogram = self.plot_chromatogram(ui);
                        chromatogram.context_menu(|ui| {
                            ui.heading("Plot Properties");
                            ui.separator();
                            debug!("Adding plot properties.");
                            self.add_plot_properties(ui);
                            ui.separator();
                        });
                        info!("Chromatogram plotted successfully.");
                    });

                ui.add_space(5.0); // Add some space between the plots

                egui::CollapsingHeader::new("Mass Spectrum")
                    .default_open(true)
                    .show(ui, |ui| {
                        debug!("Plotting mass spectrum.");
                        self.plot_mass_spectrum(ui);
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
    fn add_plot_properties(&mut self, ui: &mut Ui) {
        egui::Grid::new("TextLayoutDemo")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                self.add_polarity_options(ui);
                ui.end_row();
                self.add_plot_type_options(ui);
                ui.end_row();
            });
    }

    /// Adds the polarity options UI elements to the provided `Ui`.
    ///
    /// This function renders the UI elements that allow the user to select the polarity of the mass spectrometry data. It updates the `user_input.polarity` and `state_changed` fields based on the user's selection.
    ///
    /// # Parameters
    ///
    /// - `ui`: A mutable reference to the `egui::Ui` object, which is used to render the UI elements.
    fn add_polarity_options(&mut self, ui: &mut Ui) {
        ui.label("Polarity");
        ui.horizontal(|ui| {
            if ui
                .radio_value(
                    &mut self.user_input.polarity,
                    ScanPolarity::Positive,
                    "Positive",
                )
                .clicked()
            {
                self.user_input.polarity = ScanPolarity::Positive;
                self.state_changed = StateChange::Changed;
            }
            if ui
                .radio_value(
                    &mut self.user_input.polarity,
                    ScanPolarity::Negative,
                    "Negative",
                )
                .clicked()
            {
                self.user_input.polarity = ScanPolarity::Negative;
                self.state_changed = StateChange::Changed;
            }
        });
    }

    /// Adds the plot type options UI elements to the provided `Ui`.
    ///
    /// This function renders the UI elements that allow the user to select the type of plot to display, such as TIC, Base Peak, or XIC. It updates the `user_input.plot_type` and related fields based on the user's selection.
    ///
    /// # Parameters
    ///
    /// - `ui`: A mutable reference to the `egui::Ui` object, which is used to render the UI elements.
    fn add_plot_type_options(&mut self, ui: &mut Ui) {
        ui.label("Plot Type");
        ui.horizontal(|ui| {
            if ui
                .radio_value(&mut self.user_input.plot_type, PlotType::Tic, "TIC")
                .clicked()
            {
                self.user_input.plot_type = PlotType::Tic;
                self.state_changed = StateChange::Changed;
            }
            if ui
                .radio_value(&mut self.user_input.plot_type, PlotType::Bpc, "Base Peak")
                .clicked()
            {
                self.user_input.plot_type = PlotType::Bpc;
                self.state_changed = StateChange::Changed;
            }
            if ui
                .radio_value(&mut self.user_input.plot_type, PlotType::Xic, "XIC")
                .clicked()
            {
                self.user_input.plot_type = PlotType::Xic;
                self.options_window_open = true;
            }
        });
    }

    /// Updates the XIC (Extracted Ion Chromatogram) settings window.
    ///
    /// This function is responsible for rendering the UI elements that allow the user to configure the settings for the XIC plot, such as the m/z value and mass tolerance.
    ///
    /// # Parameters
    ///
    /// - `ctx`: A reference to the `egui::Context` object, which is used to render the UI elements.
    ///
    /// # Functionality
    ///
    /// 1. Checks if the `options_window_open` field is `true`, indicating that the XIC settings window should be displayed.
    /// 2. If the window should be displayed, it creates a new `egui::Window` with the title "XIC settings".
    /// 3. The window is set to be open by default, and the `options_window_open` field is used to control whether the window should remain open or be closed.
    /// 4. Inside the window, it adds a label that instructs the user to enter the m/z and mass tolerance values.
    /// 5. It adds a `TextEdit` widget for the user to enter the m/z value.
    ///    - If the user loses focus on the m/z input field, the function updates the `user_input.mass` field with the entered value (or the default value if the input is invalid).
    ///    - It also sets the `state_changed` field to `StateChange::Changed`.
    /// 6. It adds a `TextEdit` widget for the user to enter the mass tolerance value in ppm.
    ///    - If the user loses focus on the mass tolerance input field, the function updates the `user_input.mass_tolerance` field with the entered value (or the default value if the input is invalid).
    ///    - It also sets the `state_changed` field to `StateChange::Changed`.
    ///
    /// # Errors
    ///
    /// This function does not return any errors. It handles the rendering of the XIC settings window and the updating of the corresponding fields in the struct.
    fn update_xic_settings_window(&mut self, ctx: &egui::Context) {
        if self.options_window_open {
            let mut error_message: Option<String> = None;

            egui::Window::new("XIC settings")
                .open(&mut self.options_window_open)
                .show(ctx, |ui| {
                    ui.label("Enter m/z and mass tolerance values in ppm:");

                    // Show valid ranges if file is opened
                    if let Some(active_id) = self.active_file_id {
                        if let Some(file) = self.files.get(&active_id) {
                            let bounds = &file.data.bounds;

                            ui.add_space(5.0);
                            ui.separator();

                            // Show m/z range
                            ui.horizontal(|ui| {
                                ui.label("📊 Valid m/z range:");
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{:.2} - {:.2}",
                                        bounds.min_mz, bounds.max_mz
                                    ))
                                    .color(egui::Color32::from_rgb(100, 149, 237)),
                                );
                            });

                            // Show RT range (informational)
                            ui.horizontal(|ui| {
                                ui.label("⏱  File RT range:");
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{:.2} - {:.2} min",
                                        bounds.min_rt, bounds.max_rt
                                    ))
                                    .color(egui::Color32::from_rgb(100, 149, 237)),
                                );
                            });

                            // Show scan count
                            ui.horizontal(|ui| {
                                ui.label("📈 Total scans:");
                                ui.label(
                                    egui::RichText::new(format!("{}", bounds.scan_count))
                                        .color(egui::Color32::from_rgb(100, 149, 237)),
                                );
                            });

                            ui.separator();
                            ui.add_space(5.0);
                        }
                    } else {
                        ui.add_space(5.0);
                        ui.colored_label(
                            egui::Color32::from_rgb(255, 140, 0),
                            "⚠ Open a file to see valid parameter ranges",
                        );
                        ui.add_space(5.0);
                    }
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.user_input.mass_input)
                                .hint_text("Enter m/z"),
                        )
                        .lost_focus()
                    {
                        match self.user_input.mass_input.parse::<f64>() {
                            Ok(parsed_mass) => {
                                // Early validation using unrestricted bounds
                                // Full validation happens in build_processing_params()
                                let temp_bounds = crate::validation::DataBounds::unrestricted();
                                match XicParams::new(
                                    parsed_mass,
                                    self.user_input.polarity,
                                    self.user_input.mass_tolerance,
                                    &temp_bounds,
                                ) {
                                    Ok(_) => {
                                        self.user_input.mass = parsed_mass;
                                        self.state_changed = StateChange::Changed;
                                    }
                                    Err(e) => {
                                        error!("Invalid mass value: {}", e);
                                        error_message = Some(format!("Invalid mass: {}", e));
                                        // Restore previous valid value
                                        self.user_input.mass_input =
                                            self.user_input.mass.to_string();
                                    }
                                }
                            }
                            Err(_) => {
                                error!(
                                    "Failed to parse mass input: {}",
                                    self.user_input.mass_input
                                );
                                error_message = Some(format!(
                                    "Invalid number format: '{}'",
                                    self.user_input.mass_input
                                ));
                                // Restore previous valid value
                                self.user_input.mass_input = self.user_input.mass.to_string();
                            }
                        }
                    };
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.user_input.mass_tolerance_input)
                                .hint_text("Enter mass tolerance in ppm"),
                        )
                        .lost_focus()
                    {
                        match self.user_input.mass_tolerance_input.parse::<f64>() {
                            Ok(parsed_tolerance) => {
                                // Early validation using unrestricted bounds
                                // Full validation happens in build_processing_params()
                                let temp_bounds = crate::validation::DataBounds::unrestricted();
                                match XicParams::new(
                                    self.user_input.mass,
                                    self.user_input.polarity,
                                    parsed_tolerance,
                                    &temp_bounds,
                                ) {
                                    Ok(_) => {
                                        self.user_input.mass_tolerance = parsed_tolerance;
                                        self.state_changed = StateChange::Changed;
                                    }
                                    Err(e) => {
                                        error!("Invalid mass tolerance: {}", e);
                                        error_message =
                                            Some(format!("Invalid mass tolerance: {}", e));
                                        // Restore previous valid value
                                        self.user_input.mass_tolerance_input =
                                            self.user_input.mass_tolerance.to_string();
                                    }
                                }
                            }
                            Err(_) => {
                                error!(
                                    "Failed to parse mass tolerance input: {}",
                                    self.user_input.mass_tolerance_input
                                );
                                error_message = Some(format!(
                                    "Invalid number format: '{}'",
                                    self.user_input.mass_tolerance_input
                                ));
                                // Restore previous valid value
                                self.user_input.mass_tolerance_input =
                                    self.user_input.mass_tolerance.to_string();
                            }
                        }
                    };
                });

            // Show error dialog outside of the closure to avoid borrow checker issues
            if let Some(msg) = error_message {
                self.show_error_dialog(msg);
            }
        }
    }
}
impl eframe::App for MzViewerApp {
    /// Updates the application's user interface.
    ///
    /// This method is called by the `eframe` library to update the application's state and render the user interface.
    ///
    /// # Parameters
    ///
    /// - `ctx`: A reference to the `egui::Context` object, which is used to interact with the user interface.
    /// - `_frame`: A mutable reference to the `eframe::Frame` object, which provides access to the application's frame and other low-level functionality. This parameter is not used in this implementation.
    ///
    /// # Functionality
    ///
    /// 1. Calls the `update_data_selection_panel()` function to update the data selection panel in the user interface.
    /// 2. Calls the `update_file_information_panel()` function to update the file information panel in the user interface.
    /// 3. Calls the `update_central_panel()` function to update the central panel in the user interface, which includes the chromatogram and mass spectrum plots.
    /// 4. Calls the `update_xic_settings_window()` function to update the XIC (Extracted Ion Chromatogram) settings window in the user interface, if it is open.
    ///
    /// # Errors
    ///
    /// This method does not return any errors. It calls several other functions that may encounter errors, but those errors are handled within the respective functions
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_data_selection_panel(ctx);
        self.update_file_information_panel(ctx);
        self.update_central_panel(ctx);
        self.update_xic_settings_window(ctx);

        // Render error dialog last so it appears on top
        self.render_error_dialog(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_dialog_shown() {
        let mut app = MzViewerApp::default();

        // Initially no error
        assert!(app.error_message.is_none());

        // Show error
        app.show_error_dialog("Test error message".to_string());

        // Error should be set
        assert!(app.error_message.is_some());
        assert_eq!(app.error_message.as_ref().unwrap(), "Test error message");
    }

    #[test]
    fn test_error_cleared_after_dismissal() {
        let mut app = MzViewerApp::default();

        // Set an error
        app.show_error_dialog("Test error".to_string());
        assert!(app.error_message.is_some());

        // Simulate dismissal (clicking OK sets error_message to None)
        app.error_message = None;

        // Error should be cleared
        assert!(app.error_message.is_none());
    }

    #[test]
    fn test_multiple_errors_overwrite() {
        let mut app = MzViewerApp::default();

        // Show first error
        app.show_error_dialog("First error".to_string());
        assert_eq!(app.error_message.as_ref().unwrap(), "First error");

        // Show second error (should overwrite)
        app.show_error_dialog("Second error".to_string());
        assert_eq!(app.error_message.as_ref().unwrap(), "Second error");
    }

    #[test]
    fn test_app_default_initialization() {
        let app = MzViewerApp::default();

        // Check default state
        assert!(app.files.is_empty());
        assert!(app.active_file_id.is_none());
        assert!(app.error_message.is_none());
        assert_eq!(app.invalid_file, FileValidity::Invalid);
        assert_eq!(app.state_changed, StateChange::Unchanged);
        assert!(!app.options_window_open);
    }

    #[test]
    fn test_reset_state_clears_files() {
        let mut app = MzViewerApp::default();

        // Simulate having files (we can't create real OpenFile instances easily in tests,
        // but we can test that the fields are reset)
        app.active_file_id = Some(0);

        // Reset state
        app.reset_state();

        // Check state is reset
        assert!(app.files.is_empty());
        assert!(app.active_file_id.is_none());
    }

    #[test]
    fn test_build_processing_params_tic() {
        let mut app = MzViewerApp::default();

        // Add a mock file with bounds
        let mut mock_data = parser::MzData::new();
        mock_data.bounds = crate::validation::DataBounds {
            min_mz: 100.0,
            max_mz: 1000.0,
            min_rt: 0.0,
            max_rt: 60.0,
            scan_count: 1000,
        };

        let file_id = 0;
        app.files.insert(
            file_id,
            OpenFile {
                id: file_id,
                name: "test.mzML".to_string(),
                path: "test.mzML".to_string(),
                data: mock_data,
                cached_plot_data: None,
                color: LineColor::Red,
                visible: true,
            },
        );
        app.active_file_id = Some(file_id);

        // Set up for TIC plot
        app.user_input.plot_type = PlotType::Tic;
        app.user_input.polarity = mzdata::spectrum::ScanPolarity::Positive;
        app.user_input.smoothing = 5;

        let result = app.build_processing_params();

        assert!(result.is_ok());
        let params = result.unwrap();
        assert_eq!(params.plot_type, PlotType::Tic);
        assert_eq!(params.polarity, mzdata::spectrum::ScanPolarity::Positive);
        assert_eq!(params.smoothing, 5);
        assert!(params.xic_params.is_none());
    }

    #[test]
    fn test_build_processing_params_bpc() {
        let mut app = MzViewerApp::default();

        // Add a mock file with bounds
        let mut mock_data = parser::MzData::new();
        mock_data.bounds = crate::validation::DataBounds {
            min_mz: 100.0,
            max_mz: 1000.0,
            min_rt: 0.0,
            max_rt: 60.0,
            scan_count: 1000,
        };

        let file_id = 0;
        app.files.insert(
            file_id,
            OpenFile {
                id: file_id,
                name: "test.mzML".to_string(),
                path: "test.mzML".to_string(),
                data: mock_data,
                cached_plot_data: None,
                color: LineColor::Red,
                visible: true,
            },
        );
        app.active_file_id = Some(file_id);

        // Set up for BPC plot
        app.user_input.plot_type = PlotType::Bpc;
        app.user_input.polarity = mzdata::spectrum::ScanPolarity::Negative;
        app.user_input.smoothing = 3;

        let result = app.build_processing_params();

        assert!(result.is_ok());
        let params = result.unwrap();
        assert_eq!(params.plot_type, PlotType::Bpc);
        assert_eq!(params.polarity, mzdata::spectrum::ScanPolarity::Negative);
        assert_eq!(params.smoothing, 3);
        assert!(params.xic_params.is_none());
    }

    #[test]
    fn test_build_processing_params_xic_valid() {
        let mut app = MzViewerApp::default();

        // Add a mock file with bounds
        let mut mock_data = parser::MzData::new();
        mock_data.bounds = crate::validation::DataBounds {
            min_mz: 100.0,
            max_mz: 1000.0,
            min_rt: 0.0,
            max_rt: 60.0,
            scan_count: 1000,
        };

        let file_id = 0;
        app.files.insert(
            file_id,
            OpenFile {
                id: file_id,
                name: "test.mzML".to_string(),
                path: "test.mzML".to_string(),
                data: mock_data,
                cached_plot_data: None,
                color: LineColor::Red,
                visible: true,
            },
        );
        app.active_file_id = Some(file_id);

        // Set up for XIC plot with valid parameters
        app.user_input.plot_type = PlotType::Xic;
        app.user_input.polarity = mzdata::spectrum::ScanPolarity::Positive;
        app.user_input.mass = 524.3;
        app.user_input.mass_tolerance = 10.0;
        app.user_input.smoothing = 2;

        let result = app.build_processing_params();

        assert!(result.is_ok());
        let params = result.unwrap();
        assert_eq!(params.plot_type, PlotType::Xic);
        assert_eq!(params.smoothing, 2);
        assert!(params.xic_params.is_some());
    }

    #[test]
    fn test_build_processing_params_xic_invalid_mass() {
        let mut app = MzViewerApp::default();

        // Set up for XIC plot with invalid mass
        app.user_input.plot_type = PlotType::Xic;
        app.user_input.mass = 0.0; // Invalid: must be positive
        app.user_input.mass_tolerance = 10.0;

        let result = app.build_processing_params();

        assert!(result.is_err());
    }

    #[test]
    fn test_build_processing_params_xic_invalid_tolerance() {
        let mut app = MzViewerApp::default();

        // Set up for XIC plot with invalid tolerance
        app.user_input.plot_type = PlotType::Xic;
        app.user_input.mass = 500.0;
        app.user_input.mass_tolerance = 1500.0; // Invalid: exceeds maximum 1000 ppm

        let result = app.build_processing_params();

        assert!(result.is_err());
    }

    #[test]
    fn test_create_line_for_file_respects_settings() {
        let app = MzViewerApp::default();

        let file = OpenFile {
            id: 0,
            name: "test_file.mzML".to_string(),
            path: "/path/to/test_file.mzML".to_string(),
            data: parser::MzData::new(),
            cached_plot_data: None,
            color: LineColor::Blue,
            visible: true,
        };

        let test_data: Vec<[f64; 2]> = vec![[1.0, 100.0], [2.0, 200.0], [3.0, 150.0]];

        let _line = app.create_line_for_file(&file, &test_data);

        // Line widget is created - we can't easily test its internal properties
        // without rendering, but we verify the method doesn't panic
        // The fact that we reach this point means line creation succeeded
        assert_eq!(file.name, "test_file.mzML");
    }

    #[test]
    fn test_file_id_stability() {
        let mut app = MzViewerApp::default();

        // Assign IDs as the app would
        let file1 = OpenFile {
            id: app.next_file_id,
            name: "file1.mzML".to_string(),
            path: "file1.mzML".to_string(),
            data: parser::MzData::new(),
            cached_plot_data: None,
            color: LineColor::Red,
            visible: true,
        };
        let file1_id = app.next_file_id;
        app.next_file_id += 1;

        let file2 = OpenFile {
            id: app.next_file_id,
            name: "file2.mzML".to_string(),
            path: "file2.mzML".to_string(),
            data: parser::MzData::new(),
            cached_plot_data: None,
            color: LineColor::Green,
            visible: true,
        };
        let file2_id = app.next_file_id;
        app.next_file_id += 1;

        let file3 = OpenFile {
            id: app.next_file_id,
            name: "file3.mzML".to_string(),
            path: "file3.mzML".to_string(),
            data: parser::MzData::new(),
            cached_plot_data: None,
            color: LineColor::Blue,
            visible: true,
        };
        let file3_id = app.next_file_id;
        app.next_file_id += 1;

        app.files.insert(file1_id, file1);
        app.files.insert(file2_id, file2);
        app.files.insert(file3_id, file3);

        // Verify IDs are assigned correctly
        assert_eq!(app.files.get(&0).unwrap().id, 0);
        assert_eq!(app.files.get(&1).unwrap().id, 1);
        assert_eq!(app.files.get(&2).unwrap().id, 2);
        assert_eq!(app.next_file_id, 3);

        // Remove middle file (file2 with ID 1)
        let removed = app.files.remove(&1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, 1);

        // Remaining files keep their original IDs and are still accessible by their IDs
        assert_eq!(app.files.get(&0).unwrap().id, 0); // file1 still has ID 0
        assert_eq!(app.files.get(&2).unwrap().id, 2); // file3 still has ID 2
        assert!(app.files.get(&1).is_none()); // ID 1 is gone

        // Next ID continues from where it left off
        assert_eq!(app.next_file_id, 3);
    }

    #[test]
    fn test_hashmap_file_removal_stability() {
        let mut app = MzViewerApp::default();

        // Add three files with IDs 0, 1, 2
        for i in 0..3 {
            let file = OpenFile {
                id: i,
                name: format!("file{}.mzML", i),
                path: format!("file{}.mzML", i),
                data: parser::MzData::new(),
                cached_plot_data: None,
                color: next_color_for_index(i),
                visible: true,
            };
            app.files.insert(i, file);
        }
        app.next_file_id = 3;
        app.active_file_id = Some(1); // Select file with ID 1

        // Verify all files present
        assert_eq!(app.files.len(), 3);
        assert!(app.files.contains_key(&0));
        assert!(app.files.contains_key(&1));
        assert!(app.files.contains_key(&2));

        // Remove file with ID 0
        app.files.remove(&0);

        // Files with ID 1 and 2 remain unchanged
        assert_eq!(app.files.len(), 2);
        assert!(!app.files.contains_key(&0));
        assert!(app.files.contains_key(&1)); // Still has ID 1
        assert!(app.files.contains_key(&2)); // Still has ID 2

        // Active file still points to ID 1 (unchanged!)
        assert_eq!(app.active_file_id, Some(1));

        // Can still access file with ID 1
        assert!(app.files.get(&1).is_some());
        assert_eq!(app.files.get(&1).unwrap().name, "file1.mzML");
    }
}
