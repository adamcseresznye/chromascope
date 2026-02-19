use crate::{
    parser,
    plotting_parameters::{LineColor, LineType, PlotType},
};
use mzdata::spectrum::ScanPolarity;
use std::collections::HashMap;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc;

#[derive(PartialEq)]
pub struct UserInput {
    /// Optional file path for the input data
    pub file_path: Option<String>,
    /// The type of plot to be generated. It can be PlotType::Tic, PlotType::Bpc or PlotType::Xic
    pub plot_type: PlotType,
    /// The MS level to filter (e.g., 1 for MS1, 2 for MS2)
    pub ms_level: u8,
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
    /// Whether to use range filtering for TIC/BPC plots
    pub range_enabled: bool,
    /// User input for minimum m/z range
    pub range_min_input: String,
    /// User input for maximum m/z range
    pub range_max_input: String,
    /// Parsed minimum m/z value
    pub range_min: f64,
    /// Parsed maximum m/z value
    pub range_max: f64,
}

impl Default for UserInput {
    fn default() -> Self {
        Self {
            file_path: None,
            plot_type: PlotType::default(),
            ms_level: 1, // Default to MS1
            polarity: ScanPolarity::default(),
            mass_input: String::default(),
            mass_tolerance_input: String::default(),
            mass: f64::default(),
            mass_tolerance: f64::default(),
            line_type: LineType::default(),
            line_color: LineColor::default(),
            smoothing: u8::default(),
            line_width: f32::default(),
            retention_time_ms_spectrum: None,
            range_enabled: bool::default(),
            range_min_input: String::default(),
            range_max_input: String::default(),
            range_min: f64::default(),
            range_max: f64::default(),
        }
    }
}

#[derive(Default, Debug, PartialEq)]
pub(crate) enum FileValidity {
    Valid,
    #[default]
    Invalid,
}
#[derive(Default, Debug, PartialEq)]
pub(crate) enum StateChange {
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
pub(crate) type FileId = usize;

/// Represents a single opened mzML file with its associated data and display settings
pub(crate) struct OpenFile {
    /// Stable identifier that never changes, even if other files are removed
    #[allow(dead_code)]
    pub(crate) id: FileId,
    /// The display name of the file (extracted from the path)
    pub(crate) name: String,
    /// The full path to the file
    #[allow(dead_code)]
    pub(crate) path: String,
    /// The parsed mass spectrometry data for this file
    pub(crate) data: parser::MzData,
    /// The processed plot data for this file
    pub(crate) cached_plot_data: Option<Vec<[f64; 2]>>,
    /// The last extracted chromatogram (used for triple-click spectrum lookup)
    pub(crate) cached_chromatogram: Option<parser::ChromatogramData>,
    /// The last retrieved mass spectrum (populated on triple-click)
    pub(crate) cached_mass_spectrum: Option<parser::MassSpectrum>,
    /// The color assigned to this file's chromatogram line
    pub(crate) color: LineColor,
    /// Whether this file's chromatogram is currently visible in the plot
    pub(crate) visible: bool,
}

/// Returns the next color in the cycle based on the file index
pub(crate) fn next_color_for_index(index: usize) -> LineColor {
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
    pub(crate) files: HashMap<FileId, OpenFile>,
    /// FileId of the currently active/selected file for analysis
    pub(crate) active_file_id: Option<FileId>,
    /// Next FileId to assign. Starts at 0, increments with each file opened.
    pub(crate) next_file_id: FileId,
    /// The user input parameters
    pub(crate) user_input: UserInput,
    /// The validity of the input file. Only MzML files can be read in.
    pub(crate) invalid_file: FileValidity,
    /// The state change of the application
    pub(crate) state_changed: StateChange,
    /// Whether the options window/pop-up is open
    pub(crate) options_window_open: bool,
    /// Error message to display to the user
    pub(crate) error_message: Option<String>,
    /// RT where the user started right-click dragging (minutes)
    pub(crate) integration_start_rt: Option<f64>,
    /// RT at the current drag position — updated every frame during drag
    pub(crate) integration_end_rt: Option<f64>,
    /// Computed trapezoidal area, set on drag release
    pub(crate) integration_result: Option<f64>,
    /// Whether the background processing thread is currently running
    pub(crate) is_processing: bool,
    /// Receiver for results from the background processing thread (native only)
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) processing_rx: Option<mpsc::Receiver<crate::processing::ProcessingResult>>,
}
