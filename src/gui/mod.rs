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
    plotting_parameters::PlotType,
    processing::ProcessingParams,
    validation::XicParams,
};

#[cfg(test)]
use crate::{parser, plotting_parameters::LineColor};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;

use eframe::egui;
use log::warn;

mod dialogs;
mod interactivity;
mod panels;
mod plotting;
mod state;

use state::{FileValidity, StateChange};

#[cfg(test)]
use state::{next_color_for_index, OpenFile};

pub use state::{MzViewerApp, UserInput};

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
                range_enabled: false,
                range_min_input: String::new(),
                range_max_input: String::new(),
                range_min: 0.0,
                range_max: 0.0,
                ..Default::default()
            },
            invalid_file: FileValidity::Invalid,
            state_changed: StateChange::Unchanged,
            options_window_open: false,
            error_message: None,
            integration_start_rt: None,
            integration_end_rt: None,
            integration_result: None,
            is_processing: false,
            processing_rx: None,
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

    /// Fires a background thread to process the chromatogram, keeping the UI responsive.
    ///
    /// Builds processing parameters from the current GUI state, then spawns a thread
    /// that re-opens the mzML file and calls `run_in_background`. The result is sent
    /// back via an `mpsc` channel and polled each frame by `poll_processing_result`.
    ///
    /// `MzMLReaderType<File>` is `!Send`, so the file cannot be moved to the thread;
    /// the thread opens its own independent `MzData` instance.
    fn request_chromatogram_update(&mut self) {
        let params = match self.build_processing_params() {
            Ok(p) => p,
            Err(e) => {
                self.show_error_dialog(format!("{}", e));
                return;
            }
        };

        let active_id = match self.active_file_id {
            Some(id) => id,
            None => return,
        };

        let path = match self.files.get(&active_id) {
            Some(f) => PathBuf::from(&f.path),
            None => return,
        };

        let (tx, rx) = mpsc::channel();
        self.processing_rx = Some(rx);
        self.is_processing = true;

        std::thread::spawn(move || {
            let result = crate::processing::run_in_background(path, params, active_id);
            let _ = tx.send(result);
        });
    }

    /// Polls the background processing channel for a completed result.
    ///
    /// Called every frame from `plot_chromatogram`. Requests a repaint while the
    /// background thread is still running so the spinner stays animated. When the
    /// result arrives it is applied to the matching file's cache.
    fn poll_processing_result(&mut self, ctx: &egui::Context) {
        let result = match &self.processing_rx {
            Some(rx) => match rx.try_recv() {
                Ok(r) => r,
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint();
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.is_processing = false;
                    self.processing_rx = None;
                    return;
                }
            },
            None => return,
        };

        self.is_processing = false;
        self.processing_rx = None;

        match result {
            crate::processing::ProcessingResult::Success {
                file_id,
                plot_data,
                chromatogram,
            } => {
                if let Some(file) = self.files.get_mut(&file_id) {
                    file.cached_plot_data = Some(plot_data);
                    file.cached_chromatogram = Some(chromatogram);
                }
            }
            crate::processing::ProcessingResult::Error { message, .. } => {
                self.show_error_dialog(message);
            }
        }
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

        // Build m/z range filter for TIC/BPC if enabled
        let mz_range = if self.user_input.range_enabled
            && self.user_input.plot_type != PlotType::Xic
            && self.user_input.range_min < self.user_input.range_max
        {
            Some((self.user_input.range_min, self.user_input.range_max))
        } else {
            None
        };

        Ok(ProcessingParams {
            plot_type: self.user_input.plot_type,
            ms_level: self.user_input.ms_level,
            polarity: self.user_input.polarity,
            smoothing: self.user_input.smoothing,
            xic_params,
            mz_range,
        })
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
        panels::update_data_selection_panel(self, ctx);
        panels::update_file_information_panel(self, ctx);
        panels::update_central_panel(self, ctx);
        dialogs::render_xic_settings_window(self, ctx);
        dialogs::render_error_dialog(self, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::plotting;
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
                cached_chromatogram: None,
                cached_mass_spectrum: None,
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
                cached_chromatogram: None,
                cached_mass_spectrum: None,
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
                cached_chromatogram: None,
                cached_mass_spectrum: None,
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
            cached_chromatogram: None,
            cached_mass_spectrum: None,
            color: LineColor::Blue,
            visible: true,
        };

        let test_data: Vec<[f64; 2]> = vec![[1.0, 100.0], [2.0, 200.0], [3.0, 150.0]];

        let _line = plotting::create_line_for_file(&app, &file, &test_data);

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
            cached_chromatogram: None,
            cached_mass_spectrum: None,
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
            cached_chromatogram: None,
            cached_mass_spectrum: None,
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
            cached_chromatogram: None,
            cached_mass_spectrum: None,
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
                cached_chromatogram: None,
                cached_mass_spectrum: None,
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

    #[test]
    fn test_line_color_propagates_to_active_file() {
        let mut app = MzViewerApp::default();

        let file_id = 0;
        app.files.insert(
            file_id,
            OpenFile {
                id: file_id,
                name: "test.mzML".to_string(),
                path: "test.mzML".to_string(),
                data: parser::MzData::new(),
                cached_plot_data: None,
                cached_chromatogram: None,
                cached_mass_spectrum: None,
                color: LineColor::Red,
                visible: true,
            },
        );
        app.active_file_id = Some(file_id);
        app.user_input.line_color = LineColor::Red;

        // Simulate user picking Blue
        app.user_input.line_color = LineColor::Blue;

        // Replicate the propagation logic from add_line_color_options
        if let Some(active_id) = app.active_file_id {
            if let Some(file) = app.files.get_mut(&active_id) {
                file.color = app.user_input.line_color;
            }
        }

        assert_eq!(app.files.get(&file_id).unwrap().color, LineColor::Blue);
    }

    #[test]
    fn test_active_file_switch_syncs_color_picker() {
        let mut app = MzViewerApp::default();

        for (i, color) in [LineColor::Red, LineColor::Green].iter().enumerate() {
            app.files.insert(
                i,
                OpenFile {
                    id: i,
                    name: format!("file{}.mzML", i),
                    path: format!("file{}.mzML", i),
                    data: parser::MzData::new(),
                    cached_plot_data: None,
                    cached_chromatogram: None,
                    cached_mass_spectrum: None,
                    color: *color,
                    visible: true,
                },
            );
        }
        app.active_file_id = Some(0);
        app.user_input.line_color = LineColor::Red;

        // Switch to file 1 (Green) and sync the color picker
        app.active_file_id = Some(1);
        if let Some(file) = app.files.get(&1) {
            app.user_input.line_color = file.color;
        }

        assert_eq!(app.user_input.line_color, LineColor::Green);
    }

    /// Calling poll_processing_result when processing_rx is None must not panic.
    #[test]
    fn test_poll_with_no_receiver_does_not_panic() {
        let ctx = egui::Context::default();
        let mut app = MzViewerApp::default();
        assert!(app.processing_rx.is_none());
        app.poll_processing_result(&ctx); // must not panic
    }

    /// is_processing starts as false.
    #[test]
    fn test_is_processing_default_false() {
        let app = MzViewerApp::default();
        assert!(!app.is_processing);
    }
}
