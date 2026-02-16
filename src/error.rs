//! Custom error types for Chromascope
//!
//! This module defines all failure modes that can occur during
//! MzML file processing, chromatogram extraction, and data visualization.

use thiserror::Error;

/// All possible errors that can occur in Chromascope operations
#[derive(Error, Debug)]
pub enum ChromascopeError {
    /// Attempted to operate on a file that hasn't been opened
    #[error("File not opened: {0}")]
    FileNotOpened(String),

    /// Invalid mass value provided (must be positive)
    #[error("Invalid mass value {0} - must be positive")]
    InvalidMass(f64),

    /// Mass value is outside the file's m/z range
    #[error("Mass {mass:.2} is outside file range [{min:.2} - {max:.2}] m/z")]
    MassOutOfRange { mass: f64, min: f64, max: f64 },

    /// Invalid mass tolerance provided (must be 0-1000 ppm)
    #[error("Invalid mass tolerance {0} ppm - must be between 0 and 1000")]
    InvalidMassTolerance(f64),

    /// No chromatogram data available for plotting
    #[error("No chromatogram data available - extract TIC, BPC, or XIC first")]
    NoPlotData,

    /// No retention time data available
    #[error("No retention time data available")]
    NoRetentionTimeData,

    /// No mass spectrum data available at requested index
    #[error("No mass spectrum data available at index {0}")]
    NoMassSpectrumData(usize),

    /// XIC parameters required but not provided
    #[error("XIC parameters required for XIC extraction")]
    MissingXicParams,

    /// Invalid smoothing window size
    #[error("Invalid smoothing window {0} - must be between 0 and 10")]
    InvalidSmoothingWindow(u8),

    /// Smoothing window is too large relative to the number of scans
    #[error("Smoothing window {window} is too large for {scan_count} scans (recommended max: {recommended_max})")]
    SmoothingWindowTooLarge {
        window: u8,
        scan_count: usize,
        recommended_max: u8,
    },

    /// File IO error occurred
    #[error("File IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// MzML parsing error from mzdata crate
    #[error("MzML parsing error: {0}")]
    MzDataError(String),
}

/// Project-wide Result type alias
///
/// This reduces verbosity while maintaining clarity about error types.
/// Every function returning Result in this project will use ChromascopeError.
pub type Result<T> = std::result::Result<T, ChromascopeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_mass_error_message() {
        let err = ChromascopeError::InvalidMass(-100.0);
        let msg = err.to_string();
        assert!(msg.contains("Invalid mass value"));
        assert!(msg.contains("-100"));
        assert!(msg.contains("must be positive"));
    }

    #[test]
    fn test_invalid_tolerance_error_message() {
        let err = ChromascopeError::InvalidMassTolerance(5000.0);
        let msg = err.to_string();
        assert!(msg.contains("Invalid mass tolerance"));
        assert!(msg.contains("5000"));
        assert!(msg.contains("between 0 and 1000"));
    }

    #[test]
    fn test_file_not_opened_error() {
        let err = ChromascopeError::FileNotOpened("test.mzML".to_string());
        let msg = err.to_string();
        assert!(msg.contains("File not opened"));
        assert!(msg.contains("test.mzML"));
    }

    #[test]
    fn test_result_type_alias_works() {
        fn example_function() -> Result<i32> {
            Ok(42)
        }

        let result = example_function();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_no_plot_data_error() {
        let err = ChromascopeError::NoPlotData;
        let msg = err.to_string();
        assert!(msg.contains("No chromatogram data available"));
        assert!(msg.contains("extract TIC, BPC, or XIC first"));
    }

    #[test]
    fn test_no_retention_time_data_error() {
        let err = ChromascopeError::NoRetentionTimeData;
        let msg = err.to_string();
        assert!(msg.contains("No retention time data available"));
    }

    #[test]
    fn test_no_mass_spectrum_data_error() {
        let err = ChromascopeError::NoMassSpectrumData(42);
        let msg = err.to_string();
        assert!(msg.contains("No mass spectrum data available"));
        assert!(msg.contains("42"));
    }

    #[test]
    fn test_missing_xic_params_error() {
        let err = ChromascopeError::MissingXicParams;
        let msg = err.to_string();
        assert!(msg.contains("XIC parameters required"));
    }

    #[test]
    fn test_invalid_smoothing_window_error() {
        let err = ChromascopeError::InvalidSmoothingWindow(15);
        let msg = err.to_string();
        assert!(msg.contains("Invalid smoothing window"));
        assert!(msg.contains("15"));
        assert!(msg.contains("between 0 and 10"));
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: ChromascopeError = io_err.into();
        let msg = err.to_string();
        assert!(msg.contains("File IO error"));
        assert!(msg.contains("file not found"));
    }

    #[test]
    fn test_mzdata_error() {
        let err = ChromascopeError::MzDataError("Invalid XML format".to_string());
        let msg = err.to_string();
        assert!(msg.contains("MzML parsing error"));
        assert!(msg.contains("Invalid XML format"));
    }
}
