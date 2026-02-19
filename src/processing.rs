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
use crate::parser::{ChromatogramData, MzData};
use std::cmp::Ordering;
use crate::plotting_parameters::PlotType;
use crate::validation::XicParams;
use log::{debug, info, trace};
use mzdata::spectrum::ScanPolarity;
#[cfg(not(target_arch = "wasm32"))]
use std::path::PathBuf;

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

/// Result of a background chromatogram processing task.
///
/// Sent from the background thread to the GUI thread over an `mpsc` channel.
/// Contains either the processed data (on success) or an error message.
#[cfg(not(target_arch = "wasm32"))]
pub enum ProcessingResult {
    Success {
        file_id: usize,
        plot_data: Vec<[f64; 2]>,
        chromatogram: ChromatogramData,
    },
    Error {
        file_id: usize,
        message: String,
    },
}

/// Entry point for background thread processing.
///
/// Re-opens the file on the background thread because `MzMLReaderType<File>` is
/// `!Send` and cannot be moved across thread boundaries. Calls
/// `process_chromatogram` (which uses Rayon internally) to extract the data.
///
/// # Arguments
/// * `path`    - Path to the mzML file (cloneable across thread boundary)
/// * `params`  - Processing parameters (cloned from GUI state before spawning)
/// * `file_id` - Stable `FileId` used to route the result back to the correct file
#[cfg(not(target_arch = "wasm32"))]
pub fn run_in_background(
    path: PathBuf,
    params: ProcessingParams,
    file_id: usize,
) -> ProcessingResult {
    let mut data = MzData::new();
    if let Err(e) = data.open_msfile(&path) {
        return ProcessingResult::Error {
            file_id,
            message: format!("{}", e),
        };
    }

    // Compute plot_data (smoothed Vec<[f64; 2]>).
    let plot_data = match process_chromatogram(&mut data, &params) {
        Ok(v) => v,
        Err(e) => {
            return ProcessingResult::Error {
                file_id,
                message: format!("{}", e),
            }
        }
    };

    // Re-extract raw ChromatogramData for spectrum-lookup cache.
    let chromatogram = match params.plot_type {
        PlotType::Tic => data.get_tic(params.ms_level, params.polarity, params.mz_range),
        PlotType::Bpc => data.get_bpic(params.ms_level, params.polarity, params.mz_range),
        PlotType::Xic => {
            if let Some(ref xic_params) = params.xic_params {
                data.get_xic(
                    xic_params.mass(),
                    params.ms_level,
                    xic_params.polarity(),
                    xic_params.mass_tolerance(),
                )
            } else {
                Err(ChromascopeError::MissingXicParams)
            }
        }
    };

    match chromatogram {
        Ok(c) => ProcessingResult::Success {
            file_id,
            plot_data,
            chromatogram: c,
        },
        Err(e) => ProcessingResult::Error {
            file_id,
            message: format!("{}", e),
        },
    }
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
    // Step 1: Extract raw chromatogram based on type, returning owned ChromatogramData
    let chromatogram = match params.plot_type {
        PlotType::Tic => data.get_tic(params.ms_level, params.polarity, params.mz_range)?,
        PlotType::Bpc => data.get_bpic(params.ms_level, params.polarity, params.mz_range)?,
        PlotType::Xic => {
            let xic_params = params
                .xic_params
                .as_ref()
                .ok_or(ChromascopeError::MissingXicParams)?;

            data.get_xic(
                xic_params.mass(),
                params.ms_level,
                xic_params.polarity(),
                xic_params.mass_tolerance(),
            )?
        }
    };

    // Step 2: Prepare for visualization (aggregate duplicates, format)
    let prepared = prepare_chromatogram_for_plot(&chromatogram)?;

    // Step 3: Apply smoothing filter and return result
    smooth_chromatogram(prepared, params.smoothing)
}

/// Prepare chromatogram data for plotting by averaging duplicate retention times.
///
/// Returns a vector of `[retention_time, average_intensity]` pairs suitable for plotting.
pub fn prepare_chromatogram_for_plot(chrom: &ChromatogramData) -> Result<Vec<[f64; 2]>> {
    use log::{debug, info, trace};
    info!("Starting to prepare data for plotting");

    if chrom.retention_time.is_empty() {
        return Ok(Vec::new());
    }

    let mut data = Vec::new();

    // FIX: Initialize with first actual RT value, not 0.0
    let mut temp_rt = chrom.retention_time[0];
    let mut temp_intensity_collector: Vec<f64> = Vec::new();

    trace!(
        "Processing {} retention times and intensities",
        chrom.retention_time.len()
    );

    for (idx, &rt) in chrom.retention_time.iter().enumerate() {
        if rt != temp_rt && !temp_intensity_collector.is_empty() {
            data.push([
                temp_rt as f64,
                temp_intensity_collector.iter().sum::<f64>()
                    / temp_intensity_collector.len() as f64,
            ]);
            trace!("Added data point for RT: {}", temp_rt);
            temp_intensity_collector.clear();
            temp_rt = rt;
        }
        temp_intensity_collector.push(chrom.intensity[idx] as f64);
    }

    if !temp_intensity_collector.is_empty() {
        data.push([
            temp_rt as f64,
            temp_intensity_collector.iter().sum::<f64>()
                / temp_intensity_collector.len() as f64,
        ]);
        trace!("Added final data point for RT: {}", temp_rt);
    }

    debug!("Prepared {} data points for plotting", data.len());

    Ok(data)
}

/// Find the closest spectrum index by retention time using binary search.
///
/// # Arguments
/// * `chrom` - Chromatogram data to search through
/// * `clicked_rt` - Target retention time to search for
///
/// # Returns
/// * `Some(usize)` - Spectrum index closest to the target retention time
/// * `None` - If retention time data is empty
pub fn find_closest_spectrum_index(chrom: &ChromatogramData, clicked_rt: f32) -> Option<usize> {
    use log::{info, warn};
    if chrom.retention_time.is_empty() {
        warn!("Retention time data is missing.");
        return None;
    }

    match chrom.retention_time.binary_search_by(|spectrum| {
        spectrum.partial_cmp(&clicked_rt).unwrap_or(Ordering::Equal)
    }) {
        Ok(found_index) => {
            info!("Exact RT match found at index: {:?}", found_index);
            Some(chrom.index[found_index])
        }
        Err(found_index) => {
            info!(
                "Closest RT match not found, using nearest index: {:?}",
                found_index
            );
            if found_index == 0 {
                info!("Returning the first index: {:?}", chrom.index.first());
                chrom.index.first().copied()
            } else if found_index == chrom.index.len() {
                info!("Returning the last index: {:?}", chrom.index.last());
                chrom.index.last().copied()
            } else {
                let prev = &chrom.retention_time[found_index - 1];
                let next = &chrom.retention_time[found_index];
                info!(
                    "Comparing previous: {:?} and next: {:?} for RT: {:?}",
                    prev, next, clicked_rt
                );
                if (clicked_rt - prev).abs() < (next - clicked_rt).abs() {
                    info!(
                        "Returning previous index: {:?}",
                        chrom.index[found_index - 1]
                    );
                    Some(chrom.index[found_index - 1])
                } else {
                    info!("Returning next index: {:?}", chrom.index[found_index]);
                    Some(chrom.index[found_index])
                }
            }
        }
    }
}

/// Apply moving average smoothing to plot data.
///
/// # Arguments
/// * `data` - Plot data as `[retention_time, intensity]` pairs
/// * `window_size` - Size of smoothing window (0–10, where 0 means no smoothing)
///
/// # Returns
/// * `Ok(Vec<[f64; 2]>)` - Smoothed plot data
/// * `Err(ChromascopeError::InvalidSmoothingWindow)` - If `window_size > 10`
pub fn smooth_chromatogram(data: Vec<[f64; 2]>, window_size: u8) -> Result<Vec<[f64; 2]>> {
    info!("Starting data smoothing with window size: {}", window_size);

    if window_size > 10 {
        return Err(ChromascopeError::InvalidSmoothingWindow(window_size));
    }

    debug!("Received {} data points for smoothing", data.len());

    let mut smoothed_data = Vec::new();
    let window_size_usize = window_size as usize;

    for i in 0..data.len() {
        if i < window_size_usize || i >= data.len() - window_size_usize {
            smoothed_data.push(data[i]);
            trace!("Keeping original data point at index {}", i);
        } else {
            let sum: f64 = data[i - window_size_usize..=i + window_size_usize]
                .iter()
                .map(|point| point[1])
                .sum();
            let average = sum / (f64::from(window_size) * 2.0_f64 + 1.0_f64);
            smoothed_data.push([data[i][0], average]);
            trace!("Smoothed data point at index {}: {}", i, average);
        }
    }

    debug!("Data smoothing complete");

    Ok(smoothed_data)
}

/// Computes the trapezoidal area under a chromatogram between two RT bounds.
///
/// # Arguments
/// * `data`     - Slice of \[rt, intensity\] pairs, expected sorted by rt
/// * `start_rt` - Integration window start (minutes)
/// * `end_rt`   - Integration window end (minutes)
///
/// # Errors
/// - `InvalidIntegrationRange` if start >= end
/// - `InvalidIntegrationRange` if fewer than 2 points fall in the window
pub fn integrate_peak(data: &[[f64; 2]], start_rt: f64, end_rt: f64) -> Result<f64> {
    if start_rt >= end_rt {
        return Err(ChromascopeError::InvalidIntegrationRange(format!(
            "start ({:.4}) must be less than end ({:.4})",
            start_rt, end_rt
        )));
    }

    let window: Vec<[f64; 2]> = data
        .iter()
        .filter(|p| p[0] >= start_rt && p[0] <= end_rt)
        .copied()
        .collect();

    if window.len() < 2 {
        return Err(ChromascopeError::InvalidIntegrationRange(format!(
            "Fewer than 2 data points between {:.4} and {:.4} min",
            start_rt, end_rt
        )));
    }

    let i_start = window.first().unwrap()[1];
    let i_end = window.last().unwrap()[1];
    let rt_start = window.first().unwrap()[0];
    let rt_end = window.last().unwrap()[0];
    let rt_span = rt_end - rt_start;

    // Trapezoidal sum of raw intensities
    let raw_area: f64 = window
        .windows(2)
        .map(|w| (w[1][0] - w[0][0]) * (w[0][1] + w[1][1]) / 2.0)
        .sum();

    // Subtract the baseline trapezoid (straight line from i_start to i_end)
    let baseline_area = rt_span * (i_start + i_end) / 2.0;

    Ok(raw_area - baseline_area)
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

    // ── integrate_peak tests ──────────────────────────────────────────────────

    #[test]
    fn test_integrate_peak_triangle() {
        // Two trapezoids: [0→1] avg=50 * width=1 = 50, [1→2] avg=50 * width=1 = 50 → total 100
        let data = vec![[0.0, 0.0], [1.0, 100.0], [2.0, 0.0]];
        let area = integrate_peak(&data, 0.0, 2.0).unwrap();
        assert!((area - 100.0).abs() < 1e-9, "got {}", area);
    }

    #[test]
    fn test_integrate_peak_rectangle() {
        let data = vec![[1.0, 100.0], [2.0, 100.0], [3.0, 100.0]];
        let area = integrate_peak(&data, 1.0, 3.0).unwrap();
        // With baseline subtraction, a flat line has 0 area relative to its own start/end points
        assert!((area - 0.0).abs() < 1e-9, "got {}", area);
    }

    #[test]
    fn test_integrate_peak_start_ge_end_fails() {
        let data = vec![[0.0, 50.0], [1.0, 100.0], [2.0, 50.0]];
        assert!(matches!(
            integrate_peak(&data, 2.0, 1.0),
            Err(ChromascopeError::InvalidIntegrationRange(_))
        ));
        assert!(matches!(
            integrate_peak(&data, 1.5, 1.5),
            Err(ChromascopeError::InvalidIntegrationRange(_))
        ));
    }

    #[test]
    fn test_integrate_peak_no_points_in_window_fails() {
        let data = vec![[0.0, 100.0], [2.0, 100.0]];
        // Window [0.5, 1.5] contains 0 points → error
        assert!(matches!(
            integrate_peak(&data, 0.5, 1.5),
            Err(ChromascopeError::InvalidIntegrationRange(_))
        ));
    }

    #[test]
    fn test_integrate_peak_subset() {
        let data = vec![[0.0, 0.0], [1.0, 100.0], [2.0, 100.0], [3.0, 0.0]];
        // Integration range [1.0, 2.0] is a flat line at 100.0.
        // Relative to the baseline (chord between p[1.0] and p[2.0]), the area is 0.
        let area = integrate_peak(&data, 1.0, 2.0).unwrap();
        assert!((area - 0.0).abs() < 1e-9, "got {}", area);
    }

    #[test]
    fn test_integrate_peak_slanted_baseline() {
        let data = vec![[0.0, 50.0], [1.0, 200.0], [2.0, 100.0]];
        // raw_area = (1-0)*(50+200)/2 + (2-1)*(200+100)/2 = 125 + 150 = 275
        // baseline_area = 2 * (50+100)/2 = 150
        // expected = 275 - 150 = 125
        let area = integrate_peak(&data, 0.0, 2.0).unwrap();
        assert!((area - 125.0).abs() < 1e-9, "got {}", area);
    }
}
