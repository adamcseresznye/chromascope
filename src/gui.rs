//! # MzViewerApp Module

//! The `mzviewer` module provides a graphical user interface (GUI) for visualizing mass spectrometry data from MzML files.
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
use std::ops::Div;
use std::path::PathBuf;

use eframe::egui;
use egui::{Color32, Context, Ui};
use egui_plot::{Line, PlotPoints};
use log::{debug, error, info, warn};
use std::cmp::Ordering;

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

#[derive(Default)]
pub struct MzViewerApp {
    /// The parsed mass spectrometry data
    parsed_ms_data: parser::MzData,
    /// The plot data, prepared by the `process_plot_data` method
    plot_data: Option<Vec<[f64; 2]>>,
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
    /// This function clears the parsed measurement data and sets the plot data to `None`.
    pub fn reset_state(&mut self) {
        self.parsed_ms_data = parser::MzData::default();
        self.plot_data = None;
    }

    /// Processes the plot data based on the user's input.
    ///
    /// This function is responsible for retrieving the appropriate plot data (TIC, BPC, or XIC) from the `parsed_ms_data` object,
    /// preparing the data for plotting, and optionally smoothing the data if requested by the user.
    ///
    /// # Parameters
    /// - `&mut self`: A mutable reference to the current instance of the struct that contains the `parsed_ms_data` and `user_input` fields.
    ///
    /// # Returns
    /// - `Option<Vec<[f64; 2]>>`: An optional vector of 2-element arrays of `f64` values, representing the processed plot data. If there was an error during the processing, `None` is returned.
    fn process_plot_data(&mut self) -> Option<Vec<[f64; 2]>> {
        info!("Starting to process plot data");

        // Log user inputs
        debug!(
        "User input - mass: {:?}, polarity: {:?}, mass tolerance: {:?}, plot type: {:?}, smoothing: {}",
        self.user_input.mass,
        self.user_input.polarity,
        self.user_input.mass_tolerance,
        self.user_input.plot_type,
        self.user_input.smoothing
    );

        let result = match self.user_input.plot_type {
            PlotType::Tic => self.parsed_ms_data.get_tic(self.user_input.polarity),
            PlotType::Bpc => self.parsed_ms_data.get_bpic(self.user_input.polarity),
            PlotType::Xic => self.parsed_ms_data.get_xic(
                self.user_input.mass,
                self.user_input.polarity,
                self.user_input.mass_tolerance,
            ),
        };

        if result.is_err() {
            error!("Failed to get plot data for the specified plot type");
        }

        let prepared_data = self.parsed_ms_data.prepare_for_plot();
        if prepared_data.is_err() {
            error!("Failed to prepare data for plotting");
        }
        if self
            .parsed_ms_data
            .smooth_data(prepared_data, self.user_input.smoothing)
            .is_err()
        {
            error!("Failed to smooth data");
            return None;
        };

        let plot_data = &self.parsed_ms_data.plot_data;
        info!("Finished processing plot data");
        plot_data.clone()
    }

    /// Plots the chromatogram (TIC, BPC, or XIC) based on the user's input.
    ///
    /// This function is responsible for updating the plot data if the state has changed, and then rendering the plot using the `egui_plot` library.
    /// It also handles the user's triple-click event on the plot, which triggers the extraction of the mass spectrum at the clicked retention time.
    ///
    /// # Parameters
    /// - `&mut self`: A mutable reference to the current instance of the struct that contains the `user_input`, `plot_data`, `state_changed`, and `parsed_ms_data` fields.
    /// - `ui: &mut egui::Ui`: A mutable reference to the current `egui::Ui` instance, which is used to render the plot.
    ///
    /// # Returns
    /// - `egui::Response`: The response from the `egui_plot::Plot` widget, which can be used to handle user interactions with the plot.
    fn plot_chromatogram(&mut self, ui: &mut egui::Ui) -> egui::Response {
        if let Some(_path) = &self.user_input.file_path {
            // Only re-process the data if the state has changed
            if self.state_changed == StateChange::Changed {
                info!("State has changed, starting to plot chromatogram");
                self.plot_data = self.process_plot_data();
                self.state_changed = StateChange::Unchanged;
            }
        }

        let mut plot_bounds = None;

        let response = egui_plot::Plot::new("chromatogram")
            .width(ui.available_width() * 0.99)
            .height(ui.available_height() * 0.6)
            .show(ui, |plot_ui| {
                if let Some(data) = &self.plot_data {
                    plot_ui.line(
                        Line::new(PlotPoints::from(data.clone()))
                            .width(self.user_input.line_width)
                            .style(self.user_input.line_type.to_egui())
                            .color(self.user_input.line_color.to_egui()), //.name(format!("{:?}", self.user_input.plot_type)),
                    );
                } else {
                    warn!("No plot data available");
                }
                plot_bounds = Some(plot_ui.plot_bounds());
            })
            .response;

        if response.triple_clicked() {
            // this was added because when triple clicked on XIC the extracted mz spectrum was not accurate (gave different result compared to BIC and TIC)
            if self.user_input.plot_type != plotting_parameters::PlotType::Xic {
                let rt_clicked = self.determine_rt_clicked(&response, plot_bounds);
                info!("Triple click detected on plot at {:?}", &rt_clicked);

                if let Some(index) = self.find_closest_spectrum(rt_clicked) {
                    info!("Found closest spectrum at index: {}", index);
                    self.parsed_ms_data.get_mass_spectrum_by_index(index);
                } else {
                    warn!("No close spectrum found for the clicked retention time");
                }
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

    /// Finds the index of the mass spectrum closest to the given retention time.
    ///
    /// This function searches the `retention_time` array in the `parsed_ms_data` object to find the mass spectrum with the closest retention time to the given value.
    /// If an exact match is not found, it returns the index of the mass spectrum with the closest retention time.
    ///
    /// # Parameters
    /// - `&self`: A reference to the current instance of the struct that contains the `parsed_ms_data` field.
    /// - `clicked_rt: Option<f32>`: The retention time at which the user clicked on the plot, or `None` if no click was detected.
    ///
    /// # Returns
    /// - `Option<usize>`: The index of the mass spectrum with the closest retention time to the given value, or `None` if the retention time or index data is missing.
    fn find_closest_spectrum(&self, clicked_rt: Option<f32>) -> Option<usize> {
        if let Some(rt) = clicked_rt {
            if let (Some(retention_times), Some(indices)) = (
                &self.parsed_ms_data.retention_time,
                &self.parsed_ms_data.index,
            ) {
                match retention_times.binary_search_by(|spectrum| {
                    spectrum.partial_cmp(&rt).unwrap_or(Ordering::Equal)
                }) {
                    Ok(found_index) => {
                        info!("Exact Rt match found at index: {:?}", found_index);
                        Some(indices[found_index])
                    }
                    Err(found_index) => {
                        // If the exact RT is not found, return the closest one
                        info!(
                            "Closest Rt match not found, using nearest index: {:?}",
                            found_index
                        );
                        if found_index == 0 {
                            info!("Returning the first index: {:?}", indices.first());
                            indices.first().copied()
                        } else if found_index == indices.len() {
                            info!("Returning the last index: {:?}", indices.last());
                            indices.last().copied()
                        } else {
                            // Compare the two closest values and return the closer one
                            let prev = &retention_times[found_index - 1];
                            let next = &retention_times[found_index];
                            info!(
                                "Comparing previous: {:?} and next: {:?} for RT: {:?}",
                                prev, next, rt
                            );
                            if (rt - prev).abs() < (next - rt).abs() {
                                info!("Returning previous index: {:?}", indices[found_index - 1]);
                                Some(indices[found_index - 1])
                            } else {
                                info!("Returning next index: {:?}", indices[found_index]);
                                Some(indices[found_index])
                            }
                        }
                    }
                }
            } else {
                warn!("Retention time or index data is missing.");
                None
            }
        } else {
            warn!("No close RT match found. Mass spectrum can't be extracted/displayed.");
            None
        }
    }

    /// Plots the mass spectrum based on the data available in the `parsed_ms_data` object.
    ///
    /// This function creates a bar chart plot of the mass-to-charge (m/z) values and their corresponding intensities.
    /// The width of the bars is adjusted based on the zoom level of the plot to provide a better visual representation.
    ///
    /// # Parameters
    /// - `&mut self`: A mutable reference to the current instance of the struct that contains the `parsed_ms_data` and `user_input` fields.
    /// - `ui: &mut egui::Ui`: A mutable reference to the current `egui::Ui` instance, which is used to render the plot.
    ///
    /// # Returns
    /// - `egui::Response`: The response from the `egui_plot::Plot` widget, which can be used to handle user interactions with the plot.
    fn plot_mass_spectrum(&mut self, ui: &mut egui::Ui) -> egui::Response {
        if let Some((mz, intensity)) = &self.parsed_ms_data.mass_spectrum {
            info!("Mass spectrum data available. Plotting the spectrum.");

            // Create bar chart data
            let _bars: Vec<egui_plot::Bar> = mz
                .iter()
                .zip(intensity.iter())
                .map(|(&m, &i)| {
                    egui_plot::Bar::new(m, i.into())
                        .width(self.user_input.line_width.div(2.0).into()) // Adjust width of bars as needed
                        .fill(self.user_input.line_color.to_egui()) // Adjust color as needed
                })
                .collect();

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
            response
        } else {
            warn!("No mass spectrum data available");
            ui.label("No mass spectrum data available")
        }
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
                if ui
                    .button("File")
                    .on_hover_text("Click to Open File")
                    .clicked()
                {
                    debug!("File button clicked.");
                    self.reset_state();
                    /*
                    // todo: we should completely clear and get a brand new self
                    self.plot_data = None; // clears the plot_data if new file is opened
                    self.parsed_ms_data = parser::MzData::default(); // clears the parser::MzData struct if new file is opened
                    self.user_input.file_path = None; // clears the file_path if new file is opened
                    */
                    self.handle_file_selection();

                    info!("File selection handled.");
                }

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

    /// Handles the selection of a file by the user.
    ///
    /// This function is responsible for the following tasks:
    ///
    /// 1. Prompts the user to select a file.
    /// 2. If a file is selected, it updates the file path and the validity of the file using the `update_file_path_and_validity()` function.
    /// 3. If no file is selected, it sets the `invalid_file` field to `FileValidity::Invalid`.
    ///
    /// # Errors
    ///
    /// This function does not return any errors. If an error occurs during the file selection process, it will be handled by the `rfd::FileDialog::new().pick_file()` function.
    fn handle_file_selection(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            info!("File selected: {:?}", path);
            self.update_file_path_and_validity(&path);
        } else {
            warn!("No file selected. Setting file validity to Invalid.");
            self.invalid_file = FileValidity::Invalid;
        }
    }

    /// Updates the file path and validity based on the selected file.
    ///
    /// This function checks the file format and updates the corresponding fields in the struct. If the file format is valid, it opens the file and updates the `parsed_ms_data` field. If the file format is invalid, it sets the `invalid_file` field to `FileValidity::Invalid`.
    ///
    /// # Parameters
    ///
    /// - `path`: A reference to the selected file path.
    ///
    /// # Errors
    ///
    /// This function may encounter errors when attempting to open the selected file. These errors will be logged as warning messages.
    fn update_file_path_and_validity(&mut self, path: &PathBuf) {
        let file_path_str = path.display().to_string();
        info!("Updating file path and validity for: {}", file_path_str);

        if file_path_str.ends_with(FILE_FORMAT) {
            info!("File format is valid.");
            self.invalid_file = FileValidity::Valid;
            self.user_input.file_path = Some(file_path_str.clone());
            self.parsed_ms_data = parser::MzData::default();
            match self.parsed_ms_data.open_msfile(&path) {
                Ok(_) => info!("File opened successfully."),
                Err(e) => warn!("Failed to open file: {}", e),
            }
        } else {
            warn!("Invalid file format.");
            self.invalid_file = FileValidity::Invalid;
        }
    }

    /// Updates the file information panel in the user interface.
    ///
    /// This function is responsible for displaying the status of the selected file in the left-side panel of the application. It checks the validity of the selected file and displays the appropriate information to the user.
    ///
    /// # Parameters
    ///
    /// - `ctx`: A reference to the `egui::Context` object, which is used to render the user interface.
    ///
    /// # Functionality
    ///
    /// 1. If the selected file is invalid, it displays a warning message indicating the expected file format.
    /// 2. If the selected file is valid, it displays the file path and provides a checkbox that allows the user to close the file.
    /// 3. If no file is selected, it displays a message indicating that no file has been selected.
    ///
    /// # Errors
    ///
    /// This function does not return any errors. It handles the file validity and user interactions within the user interface.
    fn update_file_information_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("file_information_panel").show(ctx, |ui| {
            ui.label("Opened file:");
            ui.separator();

            match self.invalid_file {
                FileValidity::Invalid => {
                    warn!("Invalid file type. Please select an {} file.", FILE_FORMAT);
                    ui.colored_label(
                        Color32::LIGHT_RED,
                        format!("Invalid file type. Please select an {} file.", FILE_FORMAT),
                    );
                }
                FileValidity::Valid => match self.user_input.file_path {
                    Some(ref file_path) => {
                        info!("Valid file selected: {}", file_path);
                        self.checkbox_bool = true;
                        if ui
                            .checkbox(
                                &mut self.checkbox_bool,
                                egui::RichText::new(file_path).small(),
                            )
                            .on_hover_text("Click to Close File")
                            .clicked()
                        {
                            info!("File closed: {}", file_path);
                            self.plot_data = None;
                            self.user_input.file_path = None;
                            self.checkbox_bool = false;
                        }
                    }
                    None => {
                        warn!("No file selected");
                        ui.colored_label(Color32::LIGHT_RED, "No file selected".to_string());
                    }
                },
            };
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
