//! Business logic layer for chromatogram processing.
//!
//! This module provides high-level orchestration of data operations
//! without any GUI dependencies.

use crate::error::{ChromascopeError, Result};
use crate::parser::{ChromatogramData, MzData};
use crate::plotting_parameters::{LineColor, PlotType};
use crate::validation::{DataBounds, XicParams};
use log::{debug, info, trace};
use mzdata::spectrum::ScanPolarity;
use std::cmp::Ordering;
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
///     acquisition: None,
///     plot_type: PlotType::Tic,
///     ms_level: 1,
///     polarity: ScanPolarity::Positive,
///     smoothing: 2,
///     xic_params: None,
///     mz_range: None,
///     precursor_mz: None,
/// };
/// ```
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub struct ProcessingParams {
    #[serde(default)]
    pub acquisition: Option<AcquisitionMode>,
    /// Type of chromatogram to extract (TIC, BPC, or XIC)
    pub plot_type: PlotType,
    /// MS level filter (e.g., 1 for MS1, 2 for MS2)
    pub ms_level: u8,
    /// Ion polarity filter (Positive, Negative, or Unknown)
    #[serde(with = "crate::processing::polarity_serde")]
    pub polarity: ScanPolarity,
    /// Smoothing window size (0-10, where 0 means no smoothing)
    pub smoothing: u8,
    /// XIC-specific parameters (required only when plot_type is Xic)
    pub xic_params: Option<XicParams>,
    /// Optional m/z range filter for TIC/BPC (min_mz, max_mz)
    pub mz_range: Option<(f64, f64)>,
    /// Optional precursor m/z filter. None means no precursor filtering (MS1 or all MS2).
    /// Some(mz) restricts extraction to spectra whose precursor isolation target is
    /// within 0.01 Da of `mz` — used to render per-precursor MS2 chromatograms.
    pub precursor_mz: Option<f64>,
}

/// Result of a background chromatogram processing task.
///
/// Sent from the background thread to the GUI thread over an `mpsc` channel.
/// Contains either the processed data (on success) or an error message.
#[derive(Debug)]
pub enum ProcessingResult {
    Success {
        file_id: usize,
        plot_data: Vec<[f64; 2]>,
        params: ProcessingParams,
        chromatogram: ChromatogramData,
    },
    Error {
        file_id: usize,
        message: String,
        params: ProcessingParams,
    },
}

/// Result of a background file-loading task.
/// Sent from the background thread to the UI thread via mpsc channel.
#[derive(Debug)]
pub struct LoadedRun {
    pub name: String,
    pub path: String,
    pub bounds: DataBounds,
    pub scan_filters: Vec<(u8, ScanPolarity, Option<f64>, f64, f64)>,
}

#[derive(Debug)]
pub enum FileLoadingResult {
    Success {
        file_id: usize,
        runs: Vec<LoadedRun>,
        workspace: Option<std::sync::Arc<tempfile::TempDir>>,
    },
    Error {
        file_id: usize,
        name: String,
        message: String,
    },
}

/// Imports all runs and extracts metadata off the UI thread.
pub fn import_file_in_background(
    path: PathBuf,
    executable: Option<PathBuf>,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    file_id: usize,
) -> FileLoadingResult {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let result = (|| {
        let imported = crate::import::prepare_input(&path, executable.as_deref(), &cancelled)?;
        let multiple = imported.paths.len() > 1;
        let mut runs = Vec::new();
        for run_path in imported.paths {
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                return Err("Import cancelled.".to_string());
            }
            let mut data = MzData::new();
            data.open_msfile(&run_path).map_err(|e| e.to_string())?;
            let run_name = if multiple {
                format!(
                    "{} / {}",
                    name,
                    run_path.file_stem().unwrap_or_default().to_string_lossy()
                )
            } else {
                name.clone()
            };
            runs.push(LoadedRun {
                name: run_name,
                path: run_path.to_string_lossy().into_owned(),
                bounds: data.bounds,
                scan_filters: data.available_scan_filters.clone(),
            });
        }
        Ok((runs, imported.workspace))
    })();
    match result {
        Ok((runs, workspace)) => FileLoadingResult::Success {
            file_id,
            runs,
            workspace,
        },
        Err(message) => FileLoadingResult::Error {
            file_id,
            name,
            message,
        },
    }
}

/// Opens an mzML input without an external converter.
pub fn open_file_in_background(
    path: PathBuf,
    file_id: usize,
    _color: LineColor,
) -> FileLoadingResult {
    import_file_in_background(
        path,
        None,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        file_id,
    )
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
pub fn run_in_background(
    path: PathBuf,
    params: ProcessingParams,
    file_id: usize,
) -> ProcessingResult {
    let mut data = MzData::new();
    // open_reader_only skips extract_bounds — bounds were already collected
    // during file load by open_file_in_background and stored on OpenFile.
    if let Err(e) = data.open_reader_only(&path) {
        return ProcessingResult::Error {
            file_id,
            message: format!("{}", e),
            params,
        };
    }

    match process_chromatogram(&mut data, &params) {
        Ok((plot_data, chromatogram)) => ProcessingResult::Success {
            file_id,
            plot_data,
            chromatogram,
            params,
        },
        Err(e) => ProcessingResult::Error {
            file_id,
            message: format!("{}", e),
            params,
        },
    }
}

/// High-level orchestration of chromatogram processing.
///
/// Coordinates extraction, preparation, and smoothing of chromatogram data.
///
/// # Processing Pipeline
/// 1. **Extract raw chromatogram** based on type (TIC/BPC/XIC)
///    - Mutates `MzData` internal fields (retention_time, intensity, etc.)
/// 2. **Prepare full-resolution data** (aggregate duplicates, format as [f64; 2] pairs)
///    - Returns a new Vec without mutating
/// 3. **Apply smoothing filter** if window size > 0
///    - Smooths full-resolution data before any display reduction
/// 4. **Return the final result** as owned Vec
///
/// # Arguments
/// * `data` - The MzData instance to process (will be mutated)
/// * `params` - Configuration for extraction and processing
///
/// # Returns
/// * `Ok((Vec<[f64; 2]>, ChromatogramData))` - A tuple of the smoothed plot data
///   (retention_time, intensity) pairs and the raw extracted chromatogram
///   (used for spectrum-lookup by the GUI)
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
///     acquisition: None,
///     plot_type: PlotType::Tic,
///     ms_level: 1,
///     polarity: ScanPolarity::Positive,
///     smoothing: 2,
///     xic_params: None,
///     mz_range: None,
///     precursor_mz: None,
/// };
///
/// let (plot_data, chromatogram) = process_chromatogram(&mut data, &params)?;
/// println!("Extracted {} data points", plot_data.len());
/// println!("Raw chromatogram has {} scans", chromatogram.retention_time.len());
/// # Ok::<(), chromascope::error::ChromascopeError>(())
/// ```
pub fn process_chromatogram(
    data: &mut MzData,
    params: &ProcessingParams,
) -> Result<(Vec<[f64; 2]>, ChromatogramData)> {
    data.acquisition_filter = params.acquisition;
    // Step 1: Extract raw chromatogram based on type, returning owned ChromatogramData
    let chromatogram = match params.plot_type {
        PlotType::Tic => data.get_tic(
            params.ms_level,
            params.polarity,
            params.mz_range,
            params.precursor_mz,
        )?,
        PlotType::Bpc => data.get_bpic(
            params.ms_level,
            params.polarity,
            params.mz_range,
            params.precursor_mz,
        )?,
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
                params.precursor_mz,
            )?
        }
    };

    if let Some(mode) = params.acquisition {
        if chromatogram.index.is_empty() {
            return Err(ChromascopeError::MzDataError(format!("No matching {mode:?} scans. The source must contain matching acquisition metadata and spectra for this mode.")));
        }
    }
    // Step 2: Prepare for visualization (aggregate duplicates, format)
    let prepared = prepare_chromatogram_for_plot(&chromatogram)?;

    // Step 3: Apply smoothing filter and return both plot data and raw chromatogram
    let plot_data = smooth_chromatogram(prepared, params.smoothing)?;
    Ok((plot_data, chromatogram))
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
            temp_intensity_collector.iter().sum::<f64>() / temp_intensity_collector.len() as f64,
        ]);
        trace!("Added final data point for RT: {}", temp_rt);
    }

    debug!("Prepared {} data points for plotting", data.len());

    Ok(data)
}

/// Reduce only the rendering copy, keeping endpoints and extrema in each bucket.
/// Scientific calculations and exports must use the full-resolution input.
pub fn decimate_for_display(data: &[[f64; 2]]) -> Vec<[f64; 2]> {
    const MAX_PLOT_POINTS: usize = 2000;
    if data.len() <= MAX_PLOT_POINTS {
        return data.to_vec();
    }
    let bucket_size = (data.len() - 2).div_ceil((MAX_PLOT_POINTS - 2) / 2);
    let mut result = vec![data[0]];
    for chunk in data[1..data.len() - 1].chunks(bucket_size) {
        let min = chunk
            .iter()
            .enumerate()
            .min_by(|a, b| a.1[1].total_cmp(&b.1[1]))
            .unwrap()
            .0;
        let max = chunk
            .iter()
            .enumerate()
            .max_by(|a, b| a.1[1].total_cmp(&b.1[1]))
            .unwrap()
            .0;
        result.push(chunk[min.min(max)]);
        if min != max {
            result.push(chunk[min.max(max)]);
        }
    }
    result.push(data[data.len() - 1]);
    result
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

    match chrom
        .retention_time
        .binary_search_by(|spectrum| spectrum.partial_cmp(&clicked_rt).unwrap_or(Ordering::Equal))
    {
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

/// Linearly interpolates the intensity at a given RT between two adjacent data points.
fn interpolate_intensity(p0: [f64; 2], p1: [f64; 2], rt: f64) -> f64 {
    if (p1[0] - p0[0]).abs() < f64::EPSILON {
        return p0[1];
    }
    let t = (rt - p0[0]) / (p1[0] - p0[0]);
    p0[1] + t * (p1[1] - p0[1])
}

/// Returns the interpolated intensity of the chromatogram at any given RT.
///
/// Clamps to the first or last data point outside the covered range.
/// Sorts are not performed — `data` must be sorted ascending by RT.
pub fn interpolate_at(data: &[[f64; 2]], rt: f64) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let idx = data.partition_point(|p| p[0] < rt);
    if idx == 0 {
        return data[0][1];
    }
    if idx >= data.len() {
        return data[data.len() - 1][1];
    }
    let p0 = data[idx - 1];
    let p1 = data[idx];
    if (p1[0] - p0[0]).abs() < f64::EPSILON {
        return p0[1];
    }
    let t = (rt - p0[0]) / (p1[0] - p0[0]);
    p0[1] + t * (p1[1] - p0[1])
}

/// Computes the baseline-corrected trapezoidal area above the chord connecting
/// the exact start and end RTs.
///
/// Boundary intensities are **linearly interpolated** from the nearest data
/// points so the result is independent of how data points happen to land
/// relative to the drag endpoints.
///
/// # Arguments
/// * `data`     - Slice of \[rt, intensity\] pairs, sorted ascending by rt
/// * `start_rt` - Integration window start (minutes)
/// * `end_rt`   - Integration window end (minutes)
///
/// # Errors
/// - `InvalidIntegrationRange` if `start_rt >= end_rt`
/// - `InvalidIntegrationRange` if the window cannot yield at least 2 integration points
pub fn integrate_peak(data: &[[f64; 2]], start_rt: f64, end_rt: f64) -> Result<f64> {
    if start_rt >= end_rt {
        return Err(ChromascopeError::InvalidIntegrationRange(format!(
            "start ({:.4}) must be less than end ({:.4})",
            start_rt, end_rt
        )));
    }

    if data.is_empty() {
        return Err(ChromascopeError::InvalidIntegrationRange(format!(
            "No data points available between {:.4} and {:.4} min",
            start_rt, end_rt
        )));
    }

    // left_idx: first index where data[i][0] >= start_rt
    let left_idx = data.partition_point(|p| p[0] < start_rt);
    // right_idx: first index where data[i][0] > end_rt
    let right_idx = data.partition_point(|p| p[0] <= end_rt);

    // Interpolate intensity at the left boundary.
    let i_start = if left_idx == 0 {
        data[0][1]
    } else if left_idx >= data.len() {
        data[data.len() - 1][1]
    } else if data[left_idx - 1][0] < start_rt {
        interpolate_intensity(data[left_idx - 1], data[left_idx], start_rt)
    } else {
        data[left_idx][1]
    };

    // Interpolate intensity at the right boundary.
    let i_end = if right_idx == 0 {
        data[0][1]
    } else if right_idx >= data.len() {
        data[data.len() - 1][1]
    } else if data[right_idx - 1][0] < end_rt {
        interpolate_intensity(data[right_idx - 1], data[right_idx], end_rt)
    } else {
        data[right_idx - 1][1]
    };

    // Build the window: interpolated left boundary + interior points + interpolated right boundary.
    let mut window: Vec<[f64; 2]> = Vec::with_capacity(right_idx - left_idx + 2);
    window.push([start_rt, i_start]);
    window.extend_from_slice(&data[left_idx..right_idx]);
    window.push([end_rt, i_end]);

    // Remove consecutive points with the same RT (boundary may coincide with a data point).
    window.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-9);

    if window.len() < 2 {
        return Err(ChromascopeError::InvalidIntegrationRange(format!(
            "Fewer than 2 integration points between {:.4} and {:.4} min",
            start_rt, end_rt
        )));
    }

    let rt_span = end_rt - start_rt;

    // Trapezoidal sum of raw intensities.
    let raw_area: f64 = window
        .windows(2)
        .map(|w| (w[1][0] - w[0][0]) * (w[0][1] + w[1][1]) / 2.0)
        .sum();

    // Subtract chord baseline (straight line from i_start to i_end over rt_span).
    let baseline_area = rt_span * (i_start + i_end) / 2.0;

    Ok(raw_area - baseline_area)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to load test file
    // Helper to load test file
    fn load_test_file() -> MzData {
        let mut data = MzData::new();
        let path = std::path::Path::new("test_file").join("data_dependent_02.mzML");
        data.open_msfile(&path).expect("Failed to load test file");
        data
    }

    #[test]
    fn test_process_tic() {
        // Arrange
        let mut data = load_test_file();
        let params = ProcessingParams {
            acquisition: None,
            plot_type: PlotType::Tic,
            polarity: ScanPolarity::Positive,
            smoothing: 0,
            xic_params: None,
            ms_level: 1,
            mz_range: None,
            precursor_mz: None,
        };

        // Act
        let result = process_chromatogram(&mut data, &params);

        // Assert
        assert!(result.is_ok(), "TIC processing should succeed");
        let (plot_data, chromatogram) = result.unwrap();
        assert!(!plot_data.is_empty(), "TIC should have data points");
        assert!(
            !chromatogram.retention_time.is_empty(),
            "Raw chromatogram should be non-empty"
        );
        assert!(
            plot_data[0][0] < plot_data[plot_data.len() - 1][0],
            "Retention times should be increasing"
        );
    }

    #[test]
    fn test_process_bpc() {
        // Arrange
        let mut data = load_test_file();
        let params = ProcessingParams {
            acquisition: None,
            plot_type: PlotType::Bpc,
            polarity: ScanPolarity::Positive,
            smoothing: 0,
            xic_params: None,
            ms_level: 1,
            mz_range: None,
            precursor_mz: None,
        };

        // Act
        let result = process_chromatogram(&mut data, &params);

        // Assert
        assert!(result.is_ok(), "BPC processing should succeed");
        let (plot_data, chromatogram) = result.unwrap();
        assert!(!plot_data.is_empty(), "BPC should have data points");
        assert!(
            !chromatogram.retention_time.is_empty(),
            "Raw chromatogram should be non-empty"
        );
    }

    #[test]
    fn test_process_xic_without_params_fails() {
        // Arrange
        let mut data = load_test_file();
        let params = ProcessingParams {
            acquisition: None,
            plot_type: PlotType::Xic,
            polarity: ScanPolarity::Positive,
            smoothing: 0,
            xic_params: None, // Missing required XIC params
            ms_level: 1,
            mz_range: None,
            precursor_mz: None,
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
            acquisition: None,
            plot_type: PlotType::Xic,
            polarity: ScanPolarity::Positive,
            smoothing: 0,
            xic_params: Some(xic_params),
            ms_level: 1,
            mz_range: None,
            precursor_mz: None,
        };

        // Act
        let result = process_chromatogram(&mut data, &params);

        // Assert
        assert!(result.is_ok(), "XIC with valid params should succeed");
        let (plot_data, chromatogram) = result.unwrap();
        // XIC may have fewer points than TIC/BPC, but should have some data
        assert!(!plot_data.is_empty(), "XIC should have data points");
        assert!(
            !chromatogram.retention_time.is_empty(),
            "Raw chromatogram should be non-empty"
        );
    }

    #[test]
    fn test_process_with_smoothing() {
        // Arrange
        let mut data = load_test_file();
        let params = ProcessingParams {
            acquisition: None,
            plot_type: PlotType::Bpc,
            polarity: ScanPolarity::Positive,
            smoothing: 3,
            xic_params: None,
            ms_level: 1,
            mz_range: None,
            precursor_mz: None,
        };

        // Act
        let result = process_chromatogram(&mut data, &params);

        // Assert
        assert!(result.is_ok(), "BPC with smoothing should succeed");
        let (plot_data, chromatogram) = result.unwrap();
        assert!(
            !plot_data.is_empty(),
            "Smoothed chromatogram should have data"
        );
        assert!(
            !chromatogram.retention_time.is_empty(),
            "Raw chromatogram should be non-empty"
        );
    }

    #[test]
    fn test_process_negative_polarity() {
        // Arrange
        let mut data = load_test_file();
        let params = ProcessingParams {
            acquisition: None,
            plot_type: PlotType::Tic,
            polarity: ScanPolarity::Negative,
            smoothing: 0,
            xic_params: None,
            ms_level: 1,
            mz_range: None,
            precursor_mz: None,
        };

        // Act
        let result = process_chromatogram(&mut data, &params);

        // Assert
        // May succeed with empty data or fail depending on file content
        // The key is that it doesn't panic
        if let Ok((plot_data, _chromatogram)) = result {
            // If file has negative polarity scans, we should get data
            println!("Negative polarity: {} data points", plot_data.len());
        }
    }

    #[test]
    fn test_process_excessive_smoothing_fails() {
        // Arrange
        let mut data = load_test_file();
        let params = ProcessingParams {
            acquisition: None,
            plot_type: PlotType::Tic,
            polarity: ScanPolarity::Positive,
            smoothing: 11, // Invalid: > 10
            xic_params: None,
            ms_level: 1,
            mz_range: None,
            precursor_mz: None,
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
    fn test_process_chromatogram_returns_both_plot_data_and_raw_chromatogram() {
        // Ensures we never regress to the double-read pattern.
        // The tuple must contain the smoothed plot data AND the indexable raw chromatogram.
        let mut data = load_test_file();
        let params = ProcessingParams {
            acquisition: None,
            plot_type: PlotType::Tic,
            polarity: ScanPolarity::Positive,
            smoothing: 2,
            xic_params: None,
            ms_level: 1,
            mz_range: None,
            precursor_mz: None,
        };

        let (plot_data, chromatogram) =
            process_chromatogram(&mut data, &params).expect("TIC processing should succeed");

        // plot_data is smoothed [f64; 2] pairs — used for rendering
        assert!(!plot_data.is_empty(), "plot_data must be non-empty");

        // chromatogram is the raw extraction — used for double-click spectrum lookup
        assert!(
            !chromatogram.retention_time.is_empty(),
            "raw chromatogram must be non-empty"
        );
        assert!(
            !chromatogram.index.is_empty(),
            "raw chromatogram must carry spectrum indices"
        );

        // Lengths must be consistent (plot_data may differ due to duplicate RT aggregation,
        // but both must be non-trivially populated)
        assert!(!plot_data.is_empty());
        assert_eq!(chromatogram.retention_time.len(), chromatogram.index.len());
    }

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
    fn test_integrate_peak_no_interior_points_interpolates() {
        // Both data points bracket the window — no interior data points but
        // both boundaries can be interpolated from [0.0, 100.0] → [2.0, 100.0].
        // Interpolated intensities: start=100, end=100 → flat line, area = 0.
        let data = vec![[0.0, 100.0], [2.0, 100.0]];
        let area = integrate_peak(&data, 0.5, 1.5).unwrap();
        assert!((area - 0.0).abs() < 1e-9, "got {}", area);
    }

    #[test]
    fn test_integrate_peak_empty_data_fails() {
        let data: Vec<[f64; 2]> = vec![];
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

    #[test]
    fn test_integrate_peak_interpolated_boundary() {
        // Data points at 0.0, 1.0, 2.0. Drag from 0.5 to 1.5 — boundaries fall between points.
        let data = vec![[0.0, 0.0], [1.0, 100.0], [2.0, 0.0]];
        // At rt=0.5, interp intensity = 50. At rt=1.5, interp intensity = 50.
        // window = [[0.5, 50], [1.0, 100], [1.5, 50]]
        // raw_area = 0.5*(50+100)/2 + 0.5*(100+50)/2 = 37.5 + 37.5 = 75
        // baseline_area = 1.0 * (50+50)/2 = 50
        // expected = 75 - 50 = 25
        let area = integrate_peak(&data, 0.5, 1.5).unwrap();
        assert!((area - 25.0).abs() < 1e-9, "got {}", area);
    }

    #[test]
    fn test_integrate_peak_fronting_peak() {
        // Steep rise, gradual fall — fronting peak
        let data = vec![
            [0.0, 10.0],
            [0.5, 200.0],
            [1.0, 150.0],
            [1.5, 80.0],
            [2.0, 20.0],
        ];
        // Should return a positive area above the chord from 10.0 to 20.0
        let area = integrate_peak(&data, 0.0, 2.0).unwrap();
        assert!(
            area > 0.0,
            "Fronting peak should have positive area, got {}",
            area
        );
    }

    #[test]
    fn test_interpolate_at_midpoint() {
        let data = vec![[0.0, 0.0], [1.0, 100.0], [2.0, 0.0]];
        let val = interpolate_at(&data, 0.5);
        assert!((val - 50.0).abs() < 1e-9, "got {}", val);
    }

    #[test]
    fn test_interpolate_at_exact_point() {
        let data = vec![[0.0, 0.0], [1.0, 100.0], [2.0, 0.0]];
        assert!((interpolate_at(&data, 0.0) - 0.0).abs() < 1e-9);
        assert!((interpolate_at(&data, 1.0) - 100.0).abs() < 1e-9);
        assert!((interpolate_at(&data, 2.0) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_interpolate_at_clamping() {
        let data = vec![[1.0, 50.0], [2.0, 100.0]];
        // Before first point → clamp to first
        assert!((interpolate_at(&data, 0.0) - 50.0).abs() < 1e-9);
        // After last point → clamp to last
        assert!((interpolate_at(&data, 3.0) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_decimation_caps_at_max_points() {
        let chrom = ChromatogramData {
            retention_time: (0..5000).map(|i| i as f32 * 0.01).collect(),
            intensity: (0..5000).map(|i| i as f32).collect(),
            mz: vec![],
            index: (0..5000).collect(),
        };
        let full = prepare_chromatogram_for_plot(&chrom).unwrap();
        assert_eq!(full.len(), 5000);
        let result = decimate_for_display(&full);
        assert_eq!(result.first(), full.first());
        assert_eq!(result.last(), full.last());
        assert!(
            result.len() <= 2000,
            "Expected ≤ 2000 points, got {}",
            result.len()
        );
    }
}

pub(crate) mod polarity_serde {
    use mzdata::spectrum::ScanPolarity;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(p: &ScanPolarity, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match p {
            ScanPolarity::Positive => "positive",
            ScanPolarity::Negative => "negative",
            _ => "unknown",
        })
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ScanPolarity, D::Error> {
        match String::deserialize(d)?.as_str() {
            "positive" => Ok(ScanPolarity::Positive),
            "negative" => Ok(ScanPolarity::Negative),
            "unknown" => Ok(ScanPolarity::Unknown),
            _ => Err(serde::de::Error::custom("Invalid polarity")),
        }
    }
}

/// Acquisition types are matched from spectrum CV metadata, rather than inferred from MS level alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AcquisitionMode {
    FS,
    SIM,
    MRM,
}
impl AcquisitionMode {
    pub fn matches(self, description: &mzdata::spectrum::SpectrumDescription) -> bool {
        let mut sim = false;
        let mut mrm = false;
        for p in description.params.iter().chain(
            description
                .acquisition
                .scans
                .iter()
                .flat_map(|scan| scan.params.iter().flat_map(|ps| ps.iter())),
        ) {
            let name = p.name.to_ascii_lowercase();
            sim |= p.accession == Some(1000582)
                || name == "sim spectrum"
                || name == "selected ion monitoring spectrum";
            mrm |= p.accession == Some(1000583)
                || name == "srm spectrum"
                || name == "mrm spectrum"
                || name == "selected reaction monitoring spectrum";
        }
        match self {
            Self::FS => !sim && !mrm && description.ms_level == 1,
            Self::SIM => sim,
            Self::MRM => mrm,
        }
    }
}

#[cfg(test)]
mod acquisition_tests {
    use super::*;
    #[test]
    fn targeted_modes_require_correct_spectrum_metadata() {
        let mut d = mzdata::spectrum::SpectrumDescription {
            ms_level: 1,
            ..Default::default()
        };
        assert!(AcquisitionMode::FS.matches(&d));
        assert!(!AcquisitionMode::SIM.matches(&d));
        d.params.push(mzdata::params::Param {
            name: "SIM spectrum".into(),
            accession: Some(1000582),
            ..Default::default()
        });
        assert!(AcquisitionMode::SIM.matches(&d));
        assert!(!AcquisitionMode::FS.matches(&d));
        assert!(!AcquisitionMode::MRM.matches(&d));
        d.ms_level = 2;
        d.params.clear();
        assert!(!AcquisitionMode::MRM.matches(&d));
        d.params.push(mzdata::params::Param {
            name: "SRM spectrum".into(),
            accession: Some(1000583),
            ..Default::default()
        });
        assert!(AcquisitionMode::MRM.matches(&d));
        assert!(!AcquisitionMode::SIM.matches(&d));
    }
    #[test]
    fn acquisition_filter_changes_actual_extraction_and_can_be_cleared() {
        let mut data = MzData::new();
        data.open_msfile(&PathBuf::from("test_file/data_dependent_02.mzML"))
            .unwrap();
        let mut params = ProcessingParams {
            acquisition: Some(AcquisitionMode::FS),
            plot_type: PlotType::Tic,
            ms_level: 1,
            polarity: ScanPolarity::Positive,
            smoothing: 0,
            xic_params: None,
            mz_range: None,
            precursor_mz: None,
        };
        let (_, full) = process_chromatogram(&mut data, &params).unwrap();
        assert!(!full.index.is_empty());
        params.acquisition = Some(AcquisitionMode::SIM);
        assert!(process_chromatogram(&mut data, &params).is_err());
        params.acquisition = None;
        let (_, unfiltered) = process_chromatogram(&mut data, &params).unwrap();
        assert_eq!(unfiltered.index, full.index);
    }
}
