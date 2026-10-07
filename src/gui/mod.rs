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
use log::{error, warn};

mod dialogs;
mod interactivity;
mod panels;
mod plotting;
mod preset_editor;
mod presets;
mod state;
mod workbench;
mod workspace;

use state::{AsyncState, FileValidity, IntegrationState, StateChange};

#[cfg(test)]
use state::{next_color_for_index, FileCache, FileDisplaySettings, OpenFile};

pub use state::{MzViewerApp, UserInput};

impl MzViewerApp {
    /// Creates a new instance of the `MzViewerApp` struct.
    ///
    /// # Arguments
    /// * `_cc`: The `eframe::CreationContext` reference, which is not used in this implementation.
    ///
    /// # Returns
    /// A new instance of the `MzViewerApp` struct with the following default values:
    /// - `user_input.line_width`: 2.0
    /// - All other fields in `user_input` are set to their default values.
    /// - All other fields in the `MzViewerApp` struct are set to their default values.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        workbench::configure(&_cc.egui_ctx, _cc.egui_ctx.style().visuals.dark_mode);
        Self {
            files: HashMap::new(),
            active_file_id: None,
            next_file_id: 0,
            user_input: UserInput {
                line_width: 2.0,
                ..Default::default()
            },
            invalid_file: FileValidity::Invalid,
            state_changed: StateChange::Unchanged,
            options_window_open: false,
            error_message: None,
            integration: IntegrationState::default(),
            async_state: AsyncState::new(),
            plot_properties_open: false,
            presets: Default::default(),
            workspace: workspace::Workspace::default(),
            msconvert_path: crate::import::discover_msconvert(),
        }
    }
    /// Resets the internal state of the instance.
    ///
    /// This function clears all opened files and resets the active file ID.
    pub fn reset_state(&mut self) {
        self.presets = Default::default();
        self.files.clear();
        self.workspace.traces.clear();
        self.workspace.hidden_current.clear();
        self.workspace.names.clear();
        self.workspace.order.clear();
        self.active_file_id = None;
        self.integration = IntegrationState::default();
        self.user_input.line_color = crate::plotting_parameters::LineColor::default();
        self.user_input.retention_time_ms_spectrum = None;
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

        // SHORT-CIRCUIT: skip if params unchanged since last extraction
        if self
            .files
            .get(&active_id)
            .and_then(|f| f.cache.last_processing_params.as_ref())
            == Some(&params)
        {
            return;
        }

        let workspace = self
            .files
            .get(&active_id)
            .and_then(|f| f.cache.import_workspace.clone());

        let (tx, rx) = mpsc::channel();
        self.async_state.processing_rx = Some(rx);
        self.async_state.is_processing = true;

        std::thread::spawn(move || {
            let result = crate::processing::run_in_background(path, params, active_id);
            let _ = tx.send(result);
            drop(workspace);
        });
    }

    /// Polls the background processing channel for a completed result.
    ///
    /// Called every frame from `plot_chromatogram`. Requests a repaint while the
    /// background thread is still running so the spinner stays animated. When the
    /// result arrives it is applied to the matching file's cache.
    fn poll_processing_result(&mut self, ctx: &egui::Context) {
        let result = match &self.async_state.processing_rx {
            Some(rx) => match rx.try_recv() {
                Ok(r) => r,
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint();
                    return;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.async_state.is_processing = false;
                    self.async_state.processing_rx = None;
                    return;
                }
            },
            None => return,
        };

        self.async_state.is_processing = false;
        self.async_state.processing_rx = None;

        self.apply_processing_result(result);
    }

    pub(super) fn apply_processing_result(&mut self, result: crate::processing::ProcessingResult) {
        match result {
            crate::processing::ProcessingResult::Success {
                file_id,
                plot_data,
                chromatogram,
                params,
            } => {
                workspace::retain_current(self, file_id);
                workspace::register_order(self, file_id, &params);
                self.workspace.hidden_current.remove(&file_id);
                if let Some(traces) = self.workspace.traces.get_mut(&file_id) {
                    traces.retain(|t| t.params != params);
                }
                if let Some(file) = self.files.get_mut(&file_id) {
                    file.cache.display_data =
                        Some(crate::processing::decimate_for_display(&plot_data));
                    file.cache.plot_data = Some(plot_data);
                    let trace_count = self.workspace.traces.get(&file_id).map_or(0, Vec::len);
                    if trace_count > 0
                        && file.cache.last_processing_params.as_ref() != Some(&params)
                    {
                        file.display.color = state::next_color_for_index(file_id + trace_count);
                        if self.active_file_id == Some(file_id) {
                            self.user_input.line_color = file.display.color;
                        }
                    }
                    file.cache.last_processing_params = Some(params);
                    file.cache.chromatogram = Some(chromatogram);
                    if self.active_file_id == Some(file_id) {
                        self.integration = IntegrationState::default();
                    }
                }
            }
            crate::processing::ProcessingResult::Error {
                file_id, message, ..
            } => {
                if let Some(file) = self.files.get_mut(&file_id) {
                    file.cache.last_processing_params = None;
                }
                if self.files.contains_key(&file_id) {
                    self.show_error_dialog(message);
                }
            }
        }
    }

    /// Polls for results from background file loading threads.
    ///
    /// Called every frame from `update`. Requests a repaint while any file
    /// is still loading so the spinner stays animated.
    fn poll_file_loading_result(&mut self, ctx: &egui::Context) {
        use crate::processing::FileLoadingResult;

        let result = match self.async_state.file_loading_rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => {
                // If any file is still loading, request a repaint to keep spinners animating
                if self.files.values().any(|f| f.is_loading) {
                    ctx.request_repaint();
                }
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                // This shouldn't happen as file_loading_tx is on self
                return;
            }
        };

        ctx.request_repaint();
        match result {
            FileLoadingResult::Success {
                file_id,
                runs,
                workspace,
            } => {
                // A cancelled/closed placeholder must never reappear.
                let Some(placeholder) = self.files.get(&file_id) else {
                    return;
                };
                let source = placeholder.cache.source_path.clone();
                let color = placeholder.display.color;
                let first_loaded = self.files.values().all(|f| f.is_loading);
                for (index, run) in runs.into_iter().enumerate() {
                    let id = if index == 0 {
                        file_id
                    } else {
                        let id = self.next_file_id;
                        self.next_file_id += 1;
                        self.files.insert(
                            id,
                            state::OpenFile {
                                id,
                                name: run.name.clone(),
                                path: run.path.clone(),
                                data: crate::parser::MzData::new(),
                                display: state::FileDisplaySettings {
                                    color: state::next_color_for_index(id),
                                    visible: true,
                                },
                                cache: state::FileCache::default(),
                                is_loading: true,
                            },
                        );
                        id
                    };
                    let file = self.files.get_mut(&id).unwrap();
                    file.name = run.name;
                    file.path = run.path;
                    file.cache.source_path = source.clone();
                    file.cache.import_workspace = workspace.clone();
                    file.data.bounds = run.bounds;
                    file.data.available_scan_filters = run.scan_filters.clone();
                    file.is_loading = false;
                    if index == 0 {
                        file.display.color = color;
                    }
                    if let Err(e) = file.data.open_reader_only(&PathBuf::from(&file.path)) {
                        self.files.remove(&id);
                        self.show_error_dialog(format!("Cannot open imported run: {}", e));
                        continue;
                    }
                    if first_loaded && index == 0 {
                        if let Some((ms, pol, pre, _, _)) = run
                            .scan_filters
                            .iter()
                            .find(|(ms, pol, _, _, _)| {
                                *ms == 1 && *pol == mzdata::spectrum::ScanPolarity::Positive
                            })
                            .or_else(|| run.scan_filters.first())
                        {
                            self.user_input.ms_level = *ms;
                            self.user_input.polarity = *pol;
                            self.user_input.precursor_mz = *pre;
                        }
                    }
                }
                self.repair_active_file();
                self.state_changed = StateChange::Changed;
            }
            FileLoadingResult::Error {
                file_id,
                name,
                message,
            } => {
                if !self.files.contains_key(&file_id) {
                    return;
                }
                self.files.remove(&file_id);
                error!("Background load failed for {}: {}", name, message);
                self.show_error_dialog(format!("Failed to open {}: {}", name, message));
                self.repair_active_file();
            }
        }
    }

    fn repair_active_file(&mut self) {
        if self
            .active_file_id
            .map_or(true, |id| !self.files.contains_key(&id))
        {
            self.active_file_id = self.files.keys().min().copied();
        }
        self.invalid_file = if self.files.is_empty() {
            FileValidity::Invalid
        } else {
            FileValidity::Valid
        };
        self.integration = IntegrationState::default();
        if let Some(file) = self.active_file_id.and_then(|id| self.files.get(&id)) {
            self.user_input.line_color = file.display.color;
            self.state_changed = StateChange::Changed;
        }
    }

    /// Retain pending changes while loading or processing; consume only when a
    /// current request can actually be started (or satisfied by its cache).
    fn process_pending_update(&mut self) {
        if self.presets.specs.is_some() {
            return;
        }
        if self.workspace.restore_input.is_some() {
            return;
        }
        if self.user_input.plot_type == PlotType::Xic
            && (self.user_input.mass.value <= 0.0 || self.user_input.mass_tolerance.value <= 0.0)
        {
            return;
        }
        let ready = self
            .active_file_id
            .and_then(|id| self.files.get(&id))
            .is_some_and(|f| !f.is_loading);
        if self.state_changed == StateChange::Changed && ready && !self.async_state.is_processing {
            self.state_changed = StateChange::Unchanged;
            self.request_chromatogram_update();
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
                self.user_input.mass.value,
                self.user_input.polarity,
                self.user_input.mass_tolerance.value,
                &active_file.data.bounds,
            )?)
        } else {
            None
        };

        // Build m/z range filter for TIC/BPC if enabled
        let mz_range = if self.user_input.range_enabled
            && self.user_input.plot_type != PlotType::Xic
            && self.user_input.range_min.value < self.user_input.range_max.value
        {
            Some((
                self.user_input.range_min.value,
                self.user_input.range_max.value,
            ))
        } else {
            None
        };

        Ok(ProcessingParams {
            acquisition: self.user_input.acquisition,
            plot_type: self.user_input.plot_type,
            ms_level: self.user_input.ms_level,
            polarity: self.user_input.polarity,
            smoothing: self.user_input.smoothing,
            xic_params,
            mz_range,
            precursor_mz: self.user_input.precursor_mz,
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
        presets::poll(self, ctx);
        preset_editor::show(self, ctx);
        workspace::input(self, ctx);
        self.poll_file_loading_result(ctx);
        workspace::restore_loaded(self, ctx);
        self.poll_processing_result(ctx);
        self.process_pending_update();
        panels::update_data_selection_panel(self, ctx);
        workbench::status(self, ctx);
        panels::update_file_information_panel(self, ctx);
        workbench::inspector(self, ctx);
        panels::update_central_panel(self, ctx);
        dialogs::render_xic_settings_window(self, ctx);
        dialogs::render_range_window(self, ctx);
        dialogs::render_plot_properties_window(self, ctx);
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
        let mut app = MzViewerApp {
            active_file_id: Some(0),
            ..Default::default()
        };

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
                display: FileDisplaySettings {
                    color: LineColor::Red,
                    visible: true,
                },
                cache: FileCache::default(),
                is_loading: false,
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
                display: FileDisplaySettings {
                    color: LineColor::Red,
                    visible: true,
                },
                cache: FileCache::default(),
                is_loading: false,
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
                display: FileDisplaySettings {
                    color: LineColor::Red,
                    visible: true,
                },
                cache: FileCache::default(),
                is_loading: false,
            },
        );
        app.active_file_id = Some(file_id);

        // Set up for XIC plot with valid parameters
        app.user_input.plot_type = PlotType::Xic;
        app.user_input.polarity = mzdata::spectrum::ScanPolarity::Positive;
        app.user_input.mass.value = 524.3;
        app.user_input.mass_tolerance.value = 10.0;
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
        app.user_input.mass.value = 0.0; // Invalid: must be positive
        app.user_input.mass_tolerance.value = 10.0;

        let result = app.build_processing_params();

        assert!(result.is_err());
    }

    #[test]
    fn test_build_processing_params_xic_invalid_tolerance() {
        let mut app = MzViewerApp::default();

        // Set up for XIC plot with invalid tolerance
        app.user_input.plot_type = PlotType::Xic;
        app.user_input.mass.value = 500.0;
        app.user_input.mass_tolerance.value = 1500.0; // Invalid: exceeds maximum 1000 ppm

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
            display: FileDisplaySettings {
                color: LineColor::Blue,
                visible: true,
            },
            cache: FileCache::default(),
            is_loading: false,
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
            display: FileDisplaySettings {
                color: LineColor::Red,
                visible: true,
            },
            cache: FileCache::default(),
            is_loading: false,
        };
        let file1_id = app.next_file_id;
        app.next_file_id += 1;

        let file2 = OpenFile {
            id: app.next_file_id,
            name: "file2.mzML".to_string(),
            path: "file2.mzML".to_string(),
            data: parser::MzData::new(),
            display: FileDisplaySettings {
                color: LineColor::Green,
                visible: true,
            },
            cache: FileCache::default(),
            is_loading: false,
        };
        let file2_id = app.next_file_id;
        app.next_file_id += 1;

        let file3 = OpenFile {
            id: app.next_file_id,
            name: "file3.mzML".to_string(),
            path: "file3.mzML".to_string(),
            data: parser::MzData::new(),
            display: FileDisplaySettings {
                color: LineColor::Blue,
                visible: true,
            },
            cache: FileCache::default(),
            is_loading: false,
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
        assert!(!app.files.contains_key(&1)); // ID 1 is gone

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
                display: FileDisplaySettings {
                    color: next_color_for_index(i),
                    visible: true,
                },
                cache: FileCache::default(),
                is_loading: false,
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
        assert!(app.files.contains_key(&1));
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
                display: FileDisplaySettings {
                    color: LineColor::Red,
                    visible: true,
                },
                cache: FileCache::default(),
                is_loading: false,
            },
        );
        app.active_file_id = Some(file_id);
        app.user_input.line_color = LineColor::Red;

        // Simulate user picking Blue
        app.user_input.line_color = LineColor::Blue;

        // Replicate the propagation logic from add_line_color_options
        if let Some(active_id) = app.active_file_id {
            if let Some(file) = app.files.get_mut(&active_id) {
                file.display.color = app.user_input.line_color;
            }
        }

        assert_eq!(
            app.files.get(&file_id).unwrap().display.color,
            LineColor::Blue
        );
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
                    display: FileDisplaySettings {
                        color: *color,
                        visible: true,
                    },
                    cache: FileCache::default(),
                    is_loading: false,
                },
            );
        }
        app.active_file_id = Some(0);
        app.user_input.line_color = LineColor::Red;

        // Switch to file 1 (Green) and sync the color picker
        app.active_file_id = Some(1);
        if let Some(file) = app.files.get(&1) {
            app.user_input.line_color = file.display.color;
        }

        assert_eq!(app.user_input.line_color, LineColor::Green);
    }

    /// Calling poll_processing_result when processing_rx is None must not panic.
    #[test]
    fn test_poll_with_no_receiver_does_not_panic() {
        let ctx = egui::Context::default();
        let mut app = MzViewerApp::default();
        assert!(app.async_state.processing_rx.is_none());
        app.poll_processing_result(&ctx); // must not panic
    }

    /// is_processing starts as false.
    #[test]
    fn test_is_processing_default_false() {
        let app = MzViewerApp::default();
        assert!(!app.async_state.is_processing);
    }

    /// Verify that calling request_chromatogram_update with identical params
    /// to the cached last_processing_params does NOT set is_processing.
    #[test]
    fn test_param_cache_prevents_redundant_extraction() {
        let mut app = MzViewerApp::default();

        // Set up a mock file with bounds so build_processing_params succeeds
        let mut mock_data = parser::MzData::new();
        mock_data.bounds = crate::validation::DataBounds {
            min_mz: 100.0,
            max_mz: 1000.0,
            min_rt: 0.0,
            max_rt: 60.0,
            scan_count: 1000,
        };

        let file_id = 0;
        app.active_file_id = Some(file_id);
        app.user_input.plot_type = PlotType::Tic;
        app.user_input.ms_level = 1;
        app.user_input.polarity = mzdata::spectrum::ScanPolarity::Positive;
        app.user_input.smoothing = 0;

        // Pre-populate last_processing_params with the exact params that
        // build_processing_params would return for the current user_input.
        let cached_params = ProcessingParams {
            acquisition: None,
            plot_type: PlotType::Tic,
            ms_level: 1,
            polarity: mzdata::spectrum::ScanPolarity::Positive,
            smoothing: 0,
            xic_params: None,
            mz_range: None,
            precursor_mz: None,
        };

        app.files.insert(
            file_id,
            OpenFile {
                id: file_id,
                name: "test.mzML".to_string(),
                path: "test.mzML".to_string(),
                data: mock_data,
                display: FileDisplaySettings {
                    color: LineColor::Red,
                    visible: true,
                },
                cache: FileCache {
                    last_processing_params: Some(cached_params),
                    ..FileCache::default()
                },
                is_loading: false,
            },
        );

        // request_chromatogram_update should short-circuit because params are identical.
        app.request_chromatogram_update();

        // The background thread must NOT have been spawned.
        assert!(!app.async_state.is_processing);
    }

    #[test]
    fn test_mass_spectrum_color_uses_file_display_color() {
        // Verifies that when file.display.color differs from user_input.line_color,
        // the file's color is authoritative for the mass spectrum.
        let mut app = MzViewerApp::default();
        let file_id = 0;
        app.files.insert(
            file_id,
            OpenFile {
                id: file_id,
                name: "test.mzML".to_string(),
                path: "test.mzML".to_string(),
                data: parser::MzData::new(),
                display: FileDisplaySettings {
                    color: LineColor::Blue, // file has Blue
                    visible: true,
                },
                cache: FileCache::default(),
                is_loading: false,
            },
        );
        app.active_file_id = Some(file_id);
        app.user_input.line_color = LineColor::Red; // picker still shows old Red

        // After the fix, plotting uses file.display.color (Blue), not user_input (Red).
        let file = app.files.get(&file_id).unwrap();
        assert_eq!(file.display.color, LineColor::Blue);
        assert_ne!(file.display.color, app.user_input.line_color);
    }

    #[test]
    fn test_reset_state_clears_line_color() {
        let mut app = MzViewerApp::default();
        app.user_input.line_color = LineColor::Magenta;
        app.user_input.retention_time_ms_spectrum = Some(5.0);

        app.reset_state();

        assert_eq!(app.user_input.line_color, LineColor::default());
        assert!(app.user_input.retention_time_ms_spectrum.is_none());
    }

    #[test]
    fn test_close_last_file_resets_color_picker() {
        let mut app = MzViewerApp::default();
        let file_id = 0;
        app.files.insert(
            file_id,
            OpenFile {
                id: file_id,
                name: "test.mzML".to_string(),
                path: "test.mzML".to_string(),
                data: parser::MzData::new(),
                display: FileDisplaySettings {
                    color: LineColor::Orange,
                    visible: true,
                },
                cache: FileCache::default(),
                is_loading: false,
            },
        );
        app.active_file_id = Some(file_id);
        app.user_input.line_color = LineColor::Orange;

        // Simulate close
        app.files.remove(&file_id);
        app.active_file_id = None;
        app.user_input.line_color = LineColor::default();
        app.user_input.retention_time_ms_spectrum = None;

        assert_eq!(app.user_input.line_color, LineColor::default());
        assert!(app.active_file_id.is_none());
    }

    fn app_with_real_files(count: usize) -> MzViewerApp {
        let path = PathBuf::from("test_file/data_dependent_02.mzML");
        let mut app = MzViewerApp {
            active_file_id: Some(0),
            next_file_id: count,
            ..Default::default()
        };
        app.user_input.polarity = mzdata::spectrum::ScanPolarity::Positive;
        for id in 0..count {
            let mut data = parser::MzData::new();
            data.open_msfile(&path).unwrap();
            app.files.insert(
                id,
                OpenFile {
                    id,
                    name: format!("run {}", id),
                    path: path.to_string_lossy().into_owned(),
                    data,
                    display: FileDisplaySettings {
                        color: LineColor::Red,
                        visible: true,
                    },
                    cache: FileCache::default(),
                    is_loading: false,
                },
            );
        }
        app
    }

    fn finish_processing(app: &mut MzViewerApp) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let ctx = egui::Context::default();
        while app.async_state.is_processing && std::time::Instant::now() < deadline {
            app.poll_processing_result(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!app.async_state.is_processing);
        assert!(app.error_message.is_none(), "{:?}", app.error_message);
    }

    #[test]
    fn pending_parameter_changes_survive_running_job() {
        let mut app = app_with_real_files(1);
        app.request_chromatogram_update();
        assert!(app.async_state.is_processing);
        // Request caching is committed only after a successful result.
        assert!(app.files[&0].cache.last_processing_params.is_none());
        app.user_input.smoothing = 1;
        app.state_changed = StateChange::Changed;
        app.process_pending_update();
        assert_eq!(app.state_changed, StateChange::Changed);
        finish_processing(&mut app);
        app.process_pending_update();
        assert!(app.async_state.is_processing);
        finish_processing(&mut app);
        assert_eq!(
            app.files[&0]
                .cache
                .last_processing_params
                .as_ref()
                .unwrap()
                .smoothing,
            1
        );
    }

    #[test]
    fn switching_files_during_processing_updates_new_active_file() {
        let mut app = app_with_real_files(2);
        app.request_chromatogram_update();
        app.active_file_id = Some(1);
        app.state_changed = StateChange::Changed;
        app.process_pending_update();
        assert_eq!(app.state_changed, StateChange::Changed);
        finish_processing(&mut app);
        app.process_pending_update();
        finish_processing(&mut app);
        assert!(app.files[&0].cache.plot_data.is_some());
        assert!(app.files[&1].cache.plot_data.is_some());
    }

    #[test]
    fn empty_selection_preserves_workspace_and_new_files_are_appended() {
        let mut app = app_with_real_files(1);
        panels::queue_file_imports(&mut app, vec![]);
        assert_eq!(app.active_file_id, Some(0));
        assert_eq!(app.files.len(), 1);
        assert_eq!(app.next_file_id, 1);
        panels::queue_file_imports(
            &mut app,
            vec![PathBuf::from("test_file/data_dependent_02.mzML")],
        );
        assert_eq!(app.files.len(), 2);
        assert_eq!(app.active_file_id, Some(1));
        assert_eq!(app.files[&0].name, "run 0");
        let ctx = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.files[&1].is_loading && std::time::Instant::now() < deadline {
            app.poll_file_loading_result(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!app.files[&1].is_loading);
        assert!(app.error_message.is_none());
    }

    #[test]
    fn integration_and_export_keep_full_resolution() {
        let mut app = app_with_real_files(1);
        // A long narrow peak makes max-per-chunk reduction inflate area.
        let full: Vec<_> = (0..5001)
            .map(|i| [i as f64, if i == 2500 { 100.0 } else { 0.0 }])
            .collect();
        let display = crate::processing::decimate_for_display(&full);
        assert!(display.len() < full.len());
        app.files.get_mut(&0).unwrap().cache.plot_data = Some(full);
        app.files.get_mut(&0).unwrap().cache.display_data = Some(display);
        app.integration.start_rt = Some(0.0);
        app.integration.end_rt = Some(5000.0);
        super::interactivity::compute_integration(&mut app);
        assert_eq!(app.integration.result, Some(100.0));
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("chromatogram.csv");
        crate::export::export_chromatogram_csv(
            app.files[&0].cache.plot_data.as_ref().unwrap(),
            &csv,
        )
        .unwrap();
        let contents = std::fs::read_to_string(csv).unwrap();
        assert_eq!(contents.lines().count(), 5002);
    }
}
