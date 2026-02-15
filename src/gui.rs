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
//! - `checkbox_bool`: A boolean for managing checkbox states.

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
    parser,
    plotting_parameters::{self, LineColor, LineType, PlotType},
};

use mzdata::spectrum::ScanPolarity;
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

#[derive(Default)]
enum FileValidity {
    Valid,
    #[default]
    Invalid,
}
#[derive(Default, PartialEq)]
enum StateChange {
    Changed,
    #[default]
    Unchanged,
}

/// Represents a single opened mzML file with its associated data and display settings
struct OpenFile {
    /// The display name of the file (extracted from the path)
    name: String,
    /// The full path to the file
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
    /// Collection of opened mzML files
    files: Vec<OpenFile>,
    /// Index of the currently active/selected file for analysis
    active_file_index: Option<usize>,
    /// The user input parameters
    user_input: UserInput,
    /// The validity of the input file. Only MzML files can be read in.
    invalid_file: FileValidity,
    /// The state change of the application
    state_changed: StateChange,
    /// Whether the options window/pop-up is open
    options_window_open: bool,
    /// A boolean value for a checkbox/file selector
    checkbox_bool: bool,
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
            user_input: UserInput {
                line_width: 1.0,
                ..Default::default()
            },
            ..Default::default()
        }
    }
    /// Resets the internal state of the instance.
    ///
    /// This function clears all opened files and resets the active file index.
    pub fn reset_state(&mut self) {
        self.files.clear();
        self.active_file_index = None;
    }

    /// Plots the chromatogram (TIC, BPC, or XIC) for all visible files.
    ///
    /// This function is responsible for updating the plot data if the state has changed for the active file,
    /// and then rendering plots for all visible files using the `egui_plot` library.
    /// It also handles the user's triple-click event on the plot, which triggers the extraction of the mass spectrum
    /// at the clicked retention time from the active file.
    ///
    /// # Parameters
    /// - `&mut self`: A mutable reference to the current instance of the struct
    /// - `ui: &mut egui::Ui`: A mutable reference to the current `egui::Ui` instance, which is used to render the plot.
    ///
    /// # Returns
    /// - `egui::Response`: The response from the `egui_plot::Plot` widget, which can be used to handle user interactions with the plot.
    fn plot_chromatogram(&mut self, ui: &mut egui::Ui) -> egui::Response {
        // Only re-process the data for the active file if the state has changed
        if let Some(active_idx) = self.active_file_index {
            if self.state_changed == StateChange::Changed && active_idx < self.files.len() {
                info!("State has changed, reprocessing plot data for active file: {}", self.files[active_idx].name);
                // Process the data and update plot_data for the active file
                let user_input_clone = &self.user_input;
                if let Some(file) = self.files.get_mut(active_idx) {
                    let result = match user_input_clone.plot_type {
                        PlotType::Tic => file.data.get_tic(user_input_clone.polarity),
                        PlotType::Bpc => file.data.get_bpic(user_input_clone.polarity),
                        PlotType::Xic => file.data.get_xic(
                            user_input_clone.mass,
                            user_input_clone.polarity,
                            user_input_clone.mass_tolerance,
                        ),
                    };

                    if result.is_err() {
                        error!("Failed to get plot data for the specified plot type");
                    }

                    let prepared_data = file.data.prepare_for_plot();
                    if prepared_data.is_err() {
                        error!("Failed to prepare data for plotting");
                    }
                    
                    if file.data.smooth_data(prepared_data, user_input_clone.smoothing).is_ok() {
                        file.cached_plot_data = file.data.plot_data().clone();
                    } else {
                        error!("Failed to smooth data");
                    }
                }
                self.state_changed = StateChange::Unchanged;
            }
        }

        let mut plot_bounds = None;

        let response = egui_plot::Plot::new("chromatogram")
            .width(ui.available_width() * 0.99)
            .height(ui.available_height() * 0.6)
            .legend(egui_plot::Legend::default())
            .show(ui, |plot_ui| {
                // Plot all visible files
                for file in &self.files {
                    if file.visible {
                        if let Some(data) = &file.cached_plot_data {
                            plot_ui.line(
                                Line::new(PlotPoints::from(data.clone()))
                                    .width(self.user_input.line_width)
                                    .style(self.user_input.line_type.to_egui())
                                    .color(file.color.to_egui())
                                    .name(&file.name),
                            );
                        }
                    }
                }
                
                if self.files.is_empty() {
                    warn!("No files opened");
                }
                plot_bounds = Some(plot_ui.plot_bounds());
            })
            .response;

        if response.triple_clicked() {
            // Extract mass spectrum from the active file
            if let Some(active_idx) = self.active_file_index {
                if active_idx < self.files.len() {
                    // this was added because when triple clicked on XIC the extracted mz spectrum was not accurate
                    if self.user_input.plot_type != plotting_parameters::PlotType::Xic {
                        let rt_clicked = self.determine_rt_clicked(&response, plot_bounds);
                        info!("Triple click detected on plot at {:?} for file: {}", &rt_clicked, self.files[active_idx].name);

                        if let Some(index) = self.files[active_idx].data.get_closest_index_by_time(rt_clicked) {
                            info!("Found closest spectrum at index: {}", index);
                            self.files[active_idx].data.get_mass_spectrum_by_index(index);
                        } else {
                            warn!("No close spectrum found for the clicked retention time");
                        }
                    }
                }
            } else {
                warn!("No active file selected for mass spectrum extraction");
            }
        }

        response
    }

    /// Determines the retention time at the location where the user triple-clicked on the plot.
    ///
    /// This function calculates the retention time based on the user's click position on the plot and the plot's bounds.
    ///
    /// # Parameters
    /// - `&mut self`: A mutable reference to the current instance of the struct that contains the `user_input` field.
    /// - `response: &egui::Response`: A reference to the `egui::Response` object returned by the `egui_plot::Plot` widget.
    /// - `plot_bounds: Option<egui_plot::PlotBounds>`: An optional reference to the plot's bounds, which are used to calculate the retention time.
    ///
    /// # Returns
    /// - `Option<f32>`: The calculated retention time at the clicked location, or `None` if the plot position or bounds are not available.
    fn determine_rt_clicked(
        &mut self,
        response: &egui::Response,
        plot_bounds: Option<egui_plot::PlotBounds>,
    ) -> Option<f32> {
        if let Some(plot_position) = response.interact_pointer_pos() {
            if let Some(bounds) = plot_bounds {
                let plot_width = response.rect.width();

                let min_x = *bounds.range_x().start();
                let max_x = *bounds.range_x().end();

                // Calculate the position relative to the plot area, not the response area
                let relative_x = (plot_position.x - response.rect.left()) / plot_width;

                let converted_rt = min_x + relative_x as f64 * (max_x - min_x);

                self.user_input.retention_time_ms_spectrum = Some(converted_rt as f32);
                info!(
                    "Retention time clicked: {:?}",
                    self.user_input.retention_time_ms_spectrum
                );

                return Some(converted_rt as f32);
            } else {
                warn!("Plot bounds are None");
            }
        } else {
            warn!("No plot position detected");
        }
        None
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
        if let Some(active_idx) = self.active_file_index {
            if active_idx < self.files.len() {
                if let Some((mz, intensity)) = self.files[active_idx].data.mass_spectrum() {
                    info!("Mass spectrum data available for {}. Plotting the spectrum.", self.files[active_idx].name);

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
                    
                    if ui.button("Export to CSV").on_hover_text("Export plot data to CSV").clicked() {
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
            let start_index = self.files.len();
            
            for path in paths {
                info!("Processing file: {:?}", path);
                if let Some(open_file) = self.create_open_file(&path, start_index + self.files.len() - start_index) {
                    self.files.push(open_file);
                    info!("File added successfully: {:?}", path);
                }
            }
            
            // Set the active file to the first newly added file
            if !self.files.is_empty() {
                self.active_file_index = Some(start_index);
                self.invalid_file = FileValidity::Valid;
                self.state_changed = StateChange::Changed;
                info!("Active file set to index: {}", start_index);
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
    /// 3. Writes the plot data to the selected file in CSV format with headers "Retention Time,Intensity".
    /// 4. Uses the active file's name as the default CSV filename.
    ///
    /// # Errors
    ///
    /// This function does not return errors. Any I/O errors during file writing will be logged as error messages.
    fn handle_csv_export(&self) {
        // Check if an active file is selected
        if self.active_file_index.is_none() {
            warn!("No active file selected for CSV export");
            return;
        }
        
        let active_idx = self.active_file_index.unwrap();
        if active_idx >= self.files.len() {
            warn!("Active file index out of bounds");
            return;
        }
        
        let active_file = &self.files[active_idx];
        
        // Check if plot data exists for the active file
        if active_file.cached_plot_data.is_none() {
            warn!("No plot data available to export for file: {}", active_file.name);
            return;
        }

        // Create default filename from active file name
        let default_name = active_file.name.replace(".mzML", "_chromatogram.csv");
        
        // Open save dialog
        let dialog = rfd::FileDialog::new()
            .add_filter("CSV", &["csv"])
            .set_file_name(&default_name);

        if let Some(path) = dialog.save_file() {
            info!("CSV export path selected: {:?} for file: {}", path, active_file.name);
            
            // Build CSV content
            let data = active_file.cached_plot_data.as_ref().unwrap();
            let mut csv_content = String::from("Retention Time,Intensity\n");
            
            for [retention_time, intensity] in data.iter() {
                csv_content.push_str(&format!("{},{}\n", retention_time, intensity));
            }

            // Write to file
            match std::fs::write(&path, csv_content) {
                Ok(_) => info!("CSV successfully exported to: {:?}", path),
                Err(e) => error!("Failed to export CSV to {:?}: {}", path, e),
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
    ///
    /// # Returns
    ///
    /// - `Option<OpenFile>`: The created OpenFile struct, or None if the file format is invalid or loading fails.
    fn create_open_file(&self, path: &PathBuf, index: usize) -> Option<OpenFile> {
        let file_path_str = path.display().to_string();
        info!("Creating OpenFile for: {}", file_path_str);

        if !file_path_str.ends_with(FILE_FORMAT) {
            warn!("Invalid file format for: {}. Expected {} file.", file_path_str, FILE_FORMAT);
            return None;
        }

        // Extract file name from path
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&file_path_str)
            .to_string();

        // Load the MzData
        let mut data = parser::MzData::new();
        match data.open_msfile(path) {
            Ok(_) => {
                info!("File opened successfully: {}", file_name);
                Some(OpenFile {
                    name: file_name,
                    path: file_path_str,
                    data,
                    cached_plot_data: None,
                    color: next_color_for_index(index),
                    visible: true,
                })
            }
            Err(e) => {
                warn!("Failed to open file {}: {}", file_name, e);
                None
            }
        }
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
                let mut file_to_remove: Option<usize> = None;
                let mut new_active_index: Option<usize> = None;
                
                for (idx, file) in self.files.iter_mut().enumerate() {
                    let is_active = self.active_file_index == Some(idx);
                    
                    ui.horizontal(|ui| {
                        // Highlight the active file
                        if is_active {
                            let frame = egui::Frame::default()
                                .fill(ui.visuals().selection.bg_fill)
                                .inner_margin(egui::Margin::same(4.0));
                            frame.show(ui, |ui| {
                                // Visibility checkbox
                                if ui.checkbox(&mut file.visible, "").changed() {
                                    info!("File visibility toggled: {} -> {}", file.name, file.visible);
                                }
                                
                                // File name label (clickable to set as active)
                                if ui.selectable_label(true, egui::RichText::new(&file.name).small())
                                    .on_hover_text("Active file")
                                    .clicked() {
                                    new_active_index = Some(idx);
                                }
                                
                                // Close button
                                if ui.small_button("❌").on_hover_text("Close file").clicked() {
                                    file_to_remove = Some(idx);
                                    info!("Close button clicked for file: {}", file.name);
                                }
                            });
                        } else {
                            // Visibility checkbox
                            if ui.checkbox(&mut file.visible, "").changed() {
                                info!("File visibility toggled: {} -> {}", file.name, file.visible);
                            }
                            
                            // File name label (clickable to set as active)
                            if ui.selectable_label(false, egui::RichText::new(&file.name).small())
                                .on_hover_text("Click to set as active file")
                                .clicked() {
                                new_active_index = Some(idx);
                            }
                            
                            // Close button
                            if ui.small_button("❌").on_hover_text("Close file").clicked() {
                                file_to_remove = Some(idx);
                                info!("Close button clicked for file: {}", file.name);
                            }
                        }
                    });
                }
                
                // Update active index if a file was clicked
                if let Some(new_idx) = new_active_index {
                    if self.active_file_index != Some(new_idx) {
                        self.active_file_index = Some(new_idx);
                        self.state_changed = StateChange::Changed;
                        info!("Active file changed to index: {}", new_idx);
                    }
                }
                
                // Remove file if close button was clicked
                if let Some(idx) = file_to_remove {
                    let removed_file = self.files.remove(idx);
                    info!("File removed: {}", removed_file.name);
                    
                    // Adjust active_file_index
                    if let Some(active_idx) = self.active_file_index {
                        if active_idx == idx {
                            // Removed the active file
                            self.active_file_index = if self.files.is_empty() {
                                None
                            } else {
                                Some(idx.min(self.files.len() - 1))
                            };
                        } else if active_idx > idx {
                            // Active file is after the removed file, decrement index
                            self.active_file_index = Some(active_idx - 1);
                        }
                    }
                    
                    // Update validity state
                    if self.files.is_empty() {
                        self.invalid_file = FileValidity::Invalid;
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
            egui::Window::new("XIC settings")
                .open(&mut self.options_window_open)
                .show(ctx, |ui| {
                    ui.label("Enter m/z and mass tolerance values in ppm:");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.user_input.mass_input)
                                .hint_text("Enter m/z"),
                        )
                        .lost_focus()
                    {
                        self.user_input.mass = self
                            .user_input
                            .mass_input
                            .parse()
                            .unwrap_or(self.user_input.mass);
                        self.state_changed = StateChange::Changed;
                    };
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.user_input.mass_tolerance_input)
                                .hint_text("Enter mass tolerance in ppm"),
                        )
                        .lost_focus()
                    {
                        self.user_input.mass_tolerance = self
                            .user_input
                            .mass_tolerance_input
                            .parse()
                            .unwrap_or(self.user_input.mass_tolerance);
                        self.state_changed = StateChange::Changed
                    };
                });
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
    }
}
