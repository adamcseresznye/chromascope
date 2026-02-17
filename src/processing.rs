//! Business logic layer for chromatogram processing.
//!
//! This module provides high-level orchestration of data operations
//! without any GUI dependencies. Functions here can be used in CLI tools,
//! web services, or GUI applications.
//!
//! # Architecture
//!
//! This follows the Service Layer Pattern, where business logic is decoupled
//! from presentation (GUI) and data access (parser) concerns. The processing
//! module coordinates calls to `MzData` methods but doesn't handle UI events
//! or file I/O directly.

use crate::error::{ChromascopeError, Result};
use crate::parser::MzData;
use crate::plotting_parameters::PlotType;
use crate::validation::XicParams;
use mzdata::spectrum::ScanPolarity;

/// Parameters for processing a chromatogram extraction.
///
/// Bundles all configuration needed for the processing pipeline.
/// This struct is immutable by design - create a new instance if parameters change.
///
/// # Example
/// ```
/// use chromascope::processing::ProcessingParams;
/// use chromascope::plotting_parameters::PlotType;
/// use mzdata::spectrum::ScanPolarity;
///
/// let params = ProcessingParams {
///     plot_type: PlotType::Tic,
///     ms_level: 1,
///     polarity: ScanPolarity::Positive,
///     smoothing: 2,
///     xic_params: None,
///     mz_range: None,
/// };
/// ```
#[derive(Debug, Clone)]
pub struct ProcessingParams {
    /// Type of chromatogram to extract (TIC, BPC, or XIC)
    pub plot_type: PlotType,
    /// MS level filter (e.g., 1 for MS1, 2 for MS2)
    pub ms_level: u8,
    /// Ion polarity filter (Positive, Negative, or Unknown)
    pub polarity: ScanPolarity,
    /// Smoothing window size (0-10, where 0 means no smoothing)
    pub smoothing: u8,
    /// XIC-specific parameters (required only when plot_type is Xic)
    pub xic_params: Option<XicParams>,
    /// Optional m/z range filter for TIC/BPC (min_mz, max_mz)
    pub mz_range: Option<(f64, f64)>,
}

/// High-level orchestration of chromatogram processing.
///
/// This is your business logic layer - no GUI dependencies.
/// Coordinates extraction, preparation, and smoothing of chromatogram data.
///
/// # Processing Pipeline
/// 1. **Extract raw chromatogram** based on type (TIC/BPC/XIC)
///    - Mutates `MzData` internal fields (retention_time, intensity, etc.)
/// 2. **Prepare for visualization** (aggregate duplicates, format as [f64; 2] pairs)
///    - Returns a new Vec without mutating
/// 3. **Apply smoothing filter** if window size > 0
///    - Mutates `MzData.plot_data` field with smoothed results
/// 4. **Return the final result** as owned Vec
///
/// # Arguments
/// * `data` - The MzData instance to process (will be mutated)
/// * `params` - Configuration for extraction and processing
///
/// # Returns
/// * `Ok(Vec<[f64; 2]>)` - Chromatogram data as (retention_time, intensity) pairs
/// * `Err(ChromascopeError)` - If extraction, preparation, or smoothing fails
///
/// # Errors
/// - `MissingXicParams` - XIC plot type selected but no XIC parameters provided
/// - `InvalidMass` - XIC mass value is invalid (checked during XicParams construction)
/// - `InvalidMassTolerance` - Mass tolerance outside valid range
/// - `FileNotOpened` - Attempted to process data before opening file
/// - `NoPlotData` - No data available after processing
/// - `InvalidSmoothingWindow` - Smoothing window outside 0-10 range
///
/// # Example
/// ```no_run
/// use chromascope::processing::{process_chromatogram, ProcessingParams};
/// use chromascope::parser::MzData;
/// use chromascope::plotting_parameters::PlotType;
/// use mzdata::spectrum::ScanPolarity;
/// use std::path::PathBuf;
///
/// let mut data = MzData::new();
/// data.open_msfile(&PathBuf::from("data.mzML"))?;
///
/// let params = ProcessingParams {
///     plot_type: PlotType::Tic,
///     ms_level: 1,
///     polarity: ScanPolarity::Positive,
///     smoothing: 2,
///     xic_params: None,
///     mz_range: None,
/// };
///
/// let result = process_chromatogram(&mut data, &params)?;
/// println!("Extracted {} data points", result.len());
/// # Ok::<(), chromascope::error::ChromascopeError>(())
/// ```
pub fn process_chromatogram(data: &mut MzData, params: &ProcessingParams) -> Result<Vec<[f64; 2]>> {
    // Step 1: Extract raw chromatogram based on type
    // Each method mutates data's internal fields and returns &mut Self for chaining
    match params.plot_type {
        PlotType::Tic => {
            data.get_tic(params.ms_level, params.polarity, params.mz_range)?;
        }
        PlotType::Bpc => {
            data.get_bpic(params.ms_level, params.polarity, params.mz_range)?;
        }
        PlotType::Xic => {
            // XIC requires validated parameters
            let xic_params = params
                .xic_params
                .as_ref()
                .ok_or(ChromascopeError::MissingXicParams)?;

            data.get_xic(
                xic_params.mass(),
                params.ms_level,
                xic_params.polarity(),
                xic_params.mass_tolerance(),
            )?;
        }
    }

    // Step 2: Prepare for visualization (aggregate duplicates, format)
    // This returns a new Vec without mutating data
    let prepared = data.prepare_for_plot()?;

    // Step 3: Apply smoothing filter
    // This mutates data.plot_data field with smoothed results
    data.smooth_data(Ok(prepared), params.smoothing)?;

    // Step 4: Return the final result as owned data
    // Clone from the internal cache and return ownership to caller
    data.plot_data()
        .as_ref()
        .cloned()
        .ok_or(ChromascopeError::NoPlotData)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // Helper to load test file
    fn load_test_file() -> MzData {
        let mut data = MzData::new();
        let path = PathBuf::from("test_file/data_dependent_02.mzML");
        data.open_msfile(&path).expect("Failed to load test file");
        data
    }

    #[test]
    fn test_process_tic() {
        // Arrange
        let mut data = load_test_file();
        let params = ProcessingParams {
            plot_type: PlotType::Tic,
            polarity: ScanPolarity::Positive,
            smoothing: 0,
            xic_params: None,
            ms_level: 1,
            mz_range: None,
        };

        // Act
        let result = process_chromatogram(&mut data, &params);

        // Assert
        assert!(result.is_ok(), "TIC processing should succeed");
        let chromatogram = result.unwrap();
        assert!(!chromatogram.is_empty(), "TIC should have data points");
        assert!(
            chromatogram[0][0] < chromatogram[chromatogram.len() - 1][0],
            "Retention times should be increasing"
        );
    }

    #[test]
    fn test_process_bpc() {
        // Arrange
        let mut data = load_test_file();
        let params = ProcessingParams {
            plot_type: PlotType::Bpc,
            polarity: ScanPolarity::Positive,
            smoothing: 0,
            xic_params: None,
            ms_level: 1,
            mz_range: None,
        };

        // Act
        let result = process_chromatogram(&mut data, &params);

        // Assert
        assert!(result.is_ok(), "BPC processing should succeed");
        let chromatogram = result.unwrap();
        assert!(!chromatogram.is_empty(), "BPC should have data points");
    }

    #[test]
    fn test_process_xic_without_params_fails() {
        // Arrange
        let mut data = load_test_file();
        let params = ProcessingParams {
            plot_type: PlotType::Xic,
            polarity: ScanPolarity::Positive,
            smoothing: 0,
            xic_params: None, // Missing required XIC params
            ms_level: 1,
            mz_range: None,
        };

        // Act
        let result = process_chromatogram(&mut data, &params);

        // Assert
        assert!(result.is_err(), "XIC without params should fail");
        assert!(
            matches!(result.unwrap_err(), ChromascopeError::MissingXicParams),
            "Should return MissingXicParams error"
        );
    }

    #[test]
    fn test_process_xic_with_valid_params() {
        // Arrange
        let mut data = load_test_file();
        let bounds = crate::validation::DataBounds {
            min_mz: 100.0,
            max_mz: 1000.0,
            min_rt: 0.0,
            max_rt: 60.0,
            scan_count: 1000,
        };
        let xic_params = XicParams::new(524.3, ScanPolarity::Positive, 10.0, &bounds)
            .expect("Valid XIC params should construct");

        let params = ProcessingParams {
            plot_type: PlotType::Xic,
            polarity: ScanPolarity::Positive,
            smoothing: 0,
            xic_params: Some(xic_params),
            ms_level: 1,
            mz_range: None,
        };

        // Act
        let result = process_chromatogram(&mut data, &params);

        // Assert
        assert!(result.is_ok(), "XIC with valid params should succeed");
        let chromatogram = result.unwrap();
        // XIC may have fewer points than TIC/BPC, but should have some data
        assert!(!chromatogram.is_empty(), "XIC should have data points");
    }

    #[test]
    fn test_process_with_smoothing() {
        // Arrange
        let mut data = load_test_file();
        let params = ProcessingParams {
            plot_type: PlotType::Bpc,
            polarity: ScanPolarity::Positive,
            smoothing: 3,
            xic_params: None,
            ms_level: 1,
            mz_range: None,
        };

        // Act
        let result = process_chromatogram(&mut data, &params);

        // Assert
        assert!(result.is_ok(), "BPC with smoothing should succeed");
        let chromatogram = result.unwrap();
        assert!(
            !chromatogram.is_empty(),
            "Smoothed chromatogram should have data"
        );
    }

    #[test]
    fn test_process_negative_polarity() {
        // Arrange
        let mut data = load_test_file();
        let params = ProcessingParams {
            plot_type: PlotType::Tic,
            polarity: ScanPolarity::Negative,
            smoothing: 0,
            xic_params: None,
            ms_level: 1,
            mz_range: None,
        };

        // Act
        let result = process_chromatogram(&mut data, &params);

        // Assert
        // May succeed with empty data or fail depending on file content
        // The key is that it doesn't panic
        if let Ok(chromatogram) = result {
            // If file has negative polarity scans, we should get data
            println!("Negative polarity: {} data points", chromatogram.len());
        }
    }

    #[test]
    fn test_process_excessive_smoothing_fails() {
        // Arrange
        let mut data = load_test_file();
        let params = ProcessingParams {
            plot_type: PlotType::Tic,
            polarity: ScanPolarity::Positive,
            smoothing: 11, // Invalid: > 10
            xic_params: None,
            ms_level: 1,
            mz_range: None,
        };

        // Act
        let result = process_chromatogram(&mut data, &params);

        // Assert
        assert!(result.is_err(), "Excessive smoothing should fail");
        assert!(
            matches!(
                result.unwrap_err(),
                ChromascopeError::InvalidSmoothingWindow(_)
            ),
            "Should return InvalidSmoothingWindow error"
        );
    }
}
