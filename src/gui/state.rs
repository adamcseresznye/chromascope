use crate::{
    parser,
    plotting_parameters::{LineColor, LineType, PlotType},
    processing::ProcessingParams,
};
use mzdata::spectrum::ScanPolarity;
use std::collections::HashMap;
use std::sync::mpsc;

/// A validated text input that keeps a last-known-good value alongside the raw text.
///
/// - `text` — the raw string shown in the `TextEdit` widget.
/// - `value` — the last successfully parsed, validated value.
///
/// When focus is lost, call `sync_on_focus_lost` to attempt parsing. Invalid
/// input is rejected and `text` is reverted to the previous `value`, ensuring
/// the UI never drifts from a known-good state.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidatedInput<T> {
    pub text: String,
    pub value: T,
}

impl<T: ToString + Copy + Default> Default for ValidatedInput<T> {
    fn default() -> Self {
        Self {
            text: String::new(),
            value: T::default(),
        }
    }
}

impl<T: ToString + Copy> ValidatedInput<T> {
    /// Create a new `ValidatedInput` with both `text` and `value` initialised
    /// from `default_value`.
    pub fn new(default_value: T) -> Self {
        Self {
            text: default_value.to_string(),
            value: default_value,
        }
    }

    /// Attempt to parse `text` and validate the result with `validate`.
    ///
    /// On success the `value` is updated.  On failure `text` is reverted to
    /// `value.to_string()` (last-known-good state) and an `Err` is returned.
    pub fn sync_on_focus_lost<F>(&mut self, validate: F) -> Result<(), String>
    where
        T: std::str::FromStr,
        F: Fn(&T) -> bool,
    {
        match self.text.parse::<T>() {
            Ok(parsed) if validate(&parsed) => {
                self.value = parsed;
                Ok(())
            }
            _ => {
                self.text = self.value.to_string(); // revert to last good state
                Err("Invalid input".to_string())
            }
        }
    }
}

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
    /// The m/z value entered by the user — text for the TextEdit, value for processing.
    pub mass: ValidatedInput<f64>,
    /// The mass tolerance in ppm — text for the TextEdit, value for processing.
    pub mass_tolerance: ValidatedInput<f64>,
    /// The type of line to be used in the plot
    pub line_type: LineType,
    /// The color of the line to be used in the plot
    pub line_color: LineColor,
    /// The amount of smoothing to be applied to the plot
    pub smoothing: u8,
    /// The width of the line to be used in the plot
    pub line_width: f32,
    /// The retention time of a given scan. Needed for mass spectrum extraction when the user double clicks the chromatogram
    pub retention_time_ms_spectrum: Option<f32>,
    /// Whether to use range filtering for TIC/BPC plots
    pub range_enabled: bool,
    /// Minimum m/z range — text for the TextEdit, value for processing.
    pub range_min: ValidatedInput<f64>,
    /// Maximum m/z range — text for the TextEdit, value for processing.
    pub range_max: ValidatedInput<f64>,
    /// Whether the m/z range filter window is open
    pub range_window_open: bool,
    /// Precursor m/z filter. None for MS1 or unfiltered MS2; Some(mz) for a specific precursor.
    pub precursor_mz: Option<f64>,
}

impl Default for UserInput {
    fn default() -> Self {
        Self {
            file_path: None,
            plot_type: PlotType::default(),
            ms_level: 1, // Default to MS1
            polarity: ScanPolarity::default(),
            mass: ValidatedInput::default(),
            mass_tolerance: ValidatedInput::default(),
            line_type: LineType::default(),
            line_color: LineColor::default(),
            smoothing: u8::default(),
            line_width: f32::default(),
            retention_time_ms_spectrum: None,
            range_enabled: bool::default(),
            range_min: ValidatedInput::default(),
            range_max: ValidatedInput::default(),
            range_window_open: false,
            precursor_mz: None,
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

/// Holds all state for the peak integration feature.
#[derive(Debug, Default)]
pub(crate) struct IntegrationState {
    /// RT where the user started right-click dragging (minutes)
    pub(crate) start_rt: Option<f64>,
    /// RT at the current drag position — updated every frame during drag
    pub(crate) end_rt: Option<f64>,
    /// Computed trapezoidal area, set on drag release
    pub(crate) result: Option<f64>,
    /// Interpolated intensity at the integration start point — used to render the baseline chord
    pub(crate) start_intensity: Option<f64>,
    /// Interpolated intensity at the integration end point — used to render the baseline chord
    pub(crate) end_intensity: Option<f64>,
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

/// Display/presentation settings for an opened file.
#[derive(Debug)]
pub(crate) struct FileDisplaySettings {
    /// The color assigned to this file's chromatogram line
    pub(crate) color: LineColor,
    /// Whether this file's chromatogram is currently visible in the plot
    pub(crate) visible: bool,
}

/// Cached computation results for an opened file.
#[derive(Debug, Default)]
pub(crate) struct FileCache {
    /// The processed plot data for this file
    pub(crate) plot_data: Option<Vec<[f64; 2]>>,
    /// The last extracted chromatogram (used for double-click spectrum lookup)
    pub(crate) chromatogram: Option<parser::ChromatogramData>,
    /// The last retrieved mass spectrum (populated on double-click)
    pub(crate) mass_spectrum: Option<parser::MassSpectrum>,
    /// The last set of processing params used for extraction. Used to skip
    /// redundant background thread spawns when nothing extraction-relevant changed.
    pub(crate) last_processing_params: Option<ProcessingParams>,
}

/// Represents a single opened mzML file — thin coordinator over data, display, and cache.
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
    /// Display/presentation settings (color, visibility)
    pub(crate) display: FileDisplaySettings,
    /// Cached computation results (plot data, chromatogram, spectrum, last params)
    pub(crate) cache: FileCache,
    /// True while the background thread is still reading the file metadata.
    /// The file list shows a spinner when this is true.
    pub(crate) is_loading: bool,
}

/// Returns the next color in the cycle based on the file index
pub(crate) fn next_color_for_index(index: usize) -> LineColor {
    let colors = [
        LineColor::Red,
        LineColor::Green,
        LineColor::Blue,
        LineColor::Yellow,
        LineColor::White,
        LineColor::Gray,
        LineColor::Cyan,
        LineColor::Orange,
        LineColor::Magenta,
        LineColor::Gold,
    ];
    colors[index % colors.len()]
}

/// Holds all async-machinery fields (background threads, channels).
#[derive(Debug)]
pub(crate) struct AsyncState {
    /// Whether the background processing thread is currently running
    pub(crate) is_processing: bool,
    /// Receiver for results from the background processing thread (native only)
    pub(crate) processing_rx: Option<mpsc::Receiver<crate::processing::ProcessingResult>>,
    /// Sender for results from background file-loading threads.
    pub(crate) file_loading_tx: mpsc::SyncSender<crate::processing::FileLoadingResult>,
    /// Receives results from background file-loading threads.
    pub(crate) file_loading_rx: mpsc::Receiver<crate::processing::FileLoadingResult>,
}

impl AsyncState {
    pub(crate) fn new() -> Self {
        let (file_loading_tx, file_loading_rx) = mpsc::sync_channel(32);
        Self {
            is_processing: false,
            processing_rx: None,
            file_loading_tx,
            file_loading_rx,
        }
    }
}

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
    /// Peak integration state (drag-select region + result)
    pub(crate) integration: IntegrationState,
    /// Async machinery (background threads, channels)
    pub(crate) async_state: AsyncState,
    /// Whether the Plot Properties window is open
    pub(crate) plot_properties_open: bool,
}

impl Default for MzViewerApp {
    fn default() -> Self {
        Self {
            files: HashMap::default(),
            active_file_id: None,
            next_file_id: 0,
            user_input: UserInput::default(),
            invalid_file: FileValidity::default(),
            state_changed: StateChange::default(),
            options_window_open: false,
            error_message: None,
            integration: IntegrationState::default(),
            async_state: AsyncState::new(),
            plot_properties_open: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_integration_state_default() {
        let state = IntegrationState::default();
        assert!(state.start_rt.is_none());
        assert!(state.end_rt.is_none());
        assert!(state.result.is_none());
        assert!(state.start_intensity.is_none());
        assert!(state.end_intensity.is_none());
    }

    #[test]
    fn test_file_cache_default() {
        let cache = FileCache::default();
        assert!(cache.plot_data.is_none());
        assert!(cache.chromatogram.is_none());
        assert!(cache.mass_spectrum.is_none());
        assert!(cache.last_processing_params.is_none());
    }

    #[test]
    fn test_validated_input_default() {
        let input: ValidatedInput<f64> = ValidatedInput::default();
        assert_eq!(input.value, 0.0);
        assert_eq!(input.text, "");
    }

    #[test]
    fn test_validated_input_new() {
        let input = ValidatedInput::new(42.0_f64);
        assert_eq!(input.value, 42.0);
        assert_eq!(input.text, "42");
    }

    #[test]
    fn test_validated_input_sync_accepts_valid() {
        let mut input = ValidatedInput::new(10.0_f64);
        input.text = "524.3".to_string();
        let result = input.sync_on_focus_lost(|v| *v > 0.0);
        assert!(result.is_ok());
        assert_eq!(input.value, 524.3);
        assert_eq!(input.text, "524.3");
    }

    #[test]
    fn test_validated_input_sync_rejects_invalid_parse() {
        let mut input = ValidatedInput::new(10.0_f64);
        input.text = "not_a_number".to_string();
        let result = input.sync_on_focus_lost(|_| true);
        assert!(result.is_err());
        // Text is reverted to last-known-good
        assert_eq!(input.text, "10");
        assert_eq!(input.value, 10.0);
    }

    #[test]
    fn test_validated_input_sync_rejects_failing_validation() {
        let mut input = ValidatedInput::new(10.0_f64);
        input.text = "0.0".to_string();
        // validate rejects zero
        let result = input.sync_on_focus_lost(|v| *v > 0.0);
        assert!(result.is_err());
        assert_eq!(input.value, 10.0); // unchanged
        assert_eq!(input.text, "10"); // reverted
    }
}
