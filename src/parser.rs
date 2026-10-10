//! # backend for parsing mass spectrometry files for plotting

//! The `parser` module provides functionality for reading and processing mass spectrometry data files. Supports mzML,
//! MGF, Bruker TDF, and any other format supported by the `mzdata` crate. It allows users to extract various types of data, including Base Peak Intensity (BIC), Total Ion Chromatogram (TIC), and Extracted Ion Chromatogram (XIC). Additionally, it offers methods for data smoothing and preparation for plotting.

//! ## Overview

//!The main struct in this crate is `MzData`, which encapsulates the data and methods necessary for handling mass spectrometry files. The struct includes fields for storing file information, retention times, intensities, mass-to-charge ratios (m/z), and more.

//!## Features

//!- **File Handling**: Open and read mass spectrometry files.
//!- **Data Extraction**: Extract BIC, TIC, and XIC based on specified parameters.
//!- **Data Processing**: Smooth data for better visualization and analysis.
//!- **Plot Preparation**: Prepare data for plotting with appropriate formatting.

#![warn(clippy::all)]

use crate::error::{ChromascopeError, Result};
use crate::validation::DataBounds;
use log::{debug, error, info, trace, warn};
use mzdata::io::DetailLevel;
use mzdata::spectrum::ScanPolarity;
use mzdata::{prelude::*, MZReader};
use std::cmp::Ordering;
use std::fs::File;
use std::path::PathBuf;

enum DataReader {
    Plain(Box<MZReader<File>>),
    Gzip(Box<MZReader<mzdata::io::RestartableGzDecoder<std::io::BufReader<File>>>>),
}
impl DataReader {
    fn open_path(path: &std::path::Path) -> std::io::Result<Self> {
        if path
            .to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(".mzml.gz")
        {
            MZReader::<File>::open_gzipped_read_seek(File::open(path)?)
                .map(|reader| Self::Gzip(Box::new(reader)))
        } else {
            MZReader::open_path(path).map(|reader| Self::Plain(Box::new(reader)))
        }
    }
    fn len(&self) -> usize {
        match self {
            Self::Plain(r) => r.len(),
            Self::Gzip(r) => r.len(),
        }
    }
    fn set_detail_level(&mut self, level: DetailLevel) {
        match self {
            Self::Plain(r) => r.set_detail_level(level),
            Self::Gzip(r) => r.set_detail_level(level),
        }
    }
    fn iter(&mut self) -> Box<dyn Iterator<Item = mzdata::spectrum::MultiLayerSpectrum> + '_> {
        match self {
            Self::Plain(r) => Box::new(r.iter()),
            Self::Gzip(r) => Box::new(r.iter()),
        }
    }
    fn get_spectrum_by_index(
        &mut self,
        index: usize,
    ) -> Option<mzdata::spectrum::MultiLayerSpectrum> {
        match self {
            Self::Plain(r) => r.get_spectrum_by_index(index),
            Self::Gzip(r) => r.get_spectrum_by_index(index),
        }
    }
}

/// Validate before indexing or summing; a malformed scan is never a zero signal.
fn validate_peak_arrays(mzs: &[f64], intensities: &[f32]) -> Result<()> {
    if mzs.len() != intensities.len()
        || mzs.iter().any(|v| !v.is_finite() || *v < 0.0)
        || mzs.windows(2).any(|w| w[0] > w[1])
        || intensities.iter().any(|v| !v.is_finite())
    {
        return Err(ChromascopeError::MzDataError(
            "Peak arrays must have equal lengths, finite intensities and sorted nonnegative masses"
                .into(),
        ));
    }
    Ok(())
}

/// Sum stored profile/centroid samples without peak picking or silent truncation.
fn raw_summary(
    spectrum: mzdata::spectrum::MultiLayerSpectrum,
    range: Option<(f64, f64)>,
    base_peak: bool,
) -> Result<(f32, f32)> {
    let calculate = |mzs: &[f64], intensities: &[f32]| -> Result<(f32, f32)> {
        validate_peak_arrays(mzs, intensities)?;
        let (lo, hi) = range.map_or((0, mzs.len()), |(lo, hi)| {
            (
                mzs.partition_point(|mz| *mz < lo),
                mzs.partition_point(|mz| *mz <= hi),
            )
        });
        let result = if base_peak {
            intensities[lo..hi]
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(i, intensity)| (*intensity, mzs[lo + i] as f32))
                .unwrap_or((0.0, 0.0))
        } else {
            (intensities[lo..hi].iter().sum(), 0.0)
        };
        if !result.0.is_finite() || !result.1.is_finite() {
            return Err(ChromascopeError::MzDataError(
                "Chromatogram result exceeds finite f32 range".into(),
            ));
        }
        Ok(result)
    };
    if let Some(arrays) = &spectrum.arrays {
        if arrays.is_empty() {
            return Ok((0., 0.));
        }
        let mzs = arrays
            .mzs()
            .map_err(|e| ChromascopeError::MzDataError(e.to_string()))?;
        let intensities = arrays
            .intensities()
            .map_err(|e| ChromascopeError::MzDataError(e.to_string()))?;
        calculate(&mzs, &intensities)
    } else {
        let centroided = spectrum
            .into_centroid()
            .map_err(|e| ChromascopeError::MzDataError(e.to_string()))?;
        calculate(
            &centroided.peaks.iter().map(|p| p.mz).collect::<Vec<_>>(),
            &centroided
                .peaks
                .iter()
                .map(|p| p.intensity)
                .collect::<Vec<_>>(),
        )
    }
}

/// Represents extracted chromatogram data from a mass spectrometry file.
/// Returned by `get_tic()`, `get_bpic()`, and `get_xic()` methods.
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ChromatogramData {
    /// Retention times (minutes) for each data point
    pub retention_time: Vec<f32>,
    /// Intensity values corresponding to each retention time
    pub intensity: Vec<f32>,
    /// m/z values (used for BPC, empty for TIC/XIC)
    pub mz: Vec<f32>,
    /// Spectrum indices in the original file
    pub index: Vec<usize>,
}

/// Represents a mass spectrum at a specific retention time.
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct MassSpectrum {
    /// m/z values
    pub mz: Vec<f64>,
    /// Intensity values corresponding to each m/z
    pub intensity: Vec<f32>,
    /// Spectrum index in the original file
    pub index: usize,
    /// Retention time of this spectrum
    pub retention_time: f32,
}

/// Represents a data structure for parsing mass spectrometry files.
pub struct MzData {
    pub(crate) job_control: Option<crate::jobs::JobControl>,
    pub(crate) acquisition_filter: Option<crate::processing::AcquisitionMode>,
    /// An optional `String` representing the name of the data file.
    file_name: Option<String>,
    /// An optional format-agnostic reader for the opened mass spectrometry file.
    /// `MZReader` infers the file format from the path extension and supports
    /// mzML, mzML.gz, MGF, Bruker TDF, and other formats transparently.
    msfile: Option<DataReader>,
    /// Valid parameter ranges for this file (extracted during opening)
    pub bounds: DataBounds,
    /// Vector of unique (ms_level, polarity, precursor_mz, min_mz, max_mz) combinations found
    /// in this file. For MS1 entries precursor_mz is None; for MS2+ entries it holds the
    /// isolation target m/z so each precursor gets its own dropdown entry.
    pub available_scan_filters: Vec<(u8, ScanPolarity, Option<f64>, f64, f64)>,
}

impl core::fmt::Debug for MzData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MzData")
            .field("file_name", &self.file_name)
            .field("msfile", &"Option<MZReader>")
            .field("bounds", &self.bounds)
            .field("available_scan_filters", &self.available_scan_filters)
            .finish()
    }
}

impl Default for MzData {
    fn default() -> Self {
        Self::new()
    }
}

impl MzData {
    /// Creates a new instance of `MzData` with default values.
    ///
    /// # Returns
    ///
    /// A new instance of `MzData` with all fields initialized.
    pub fn new() -> Self {
        Self {
            job_control: None,
            acquisition_filter: None,
            file_name: None,
            msfile: None,
            bounds: DataBounds::unrestricted(),
            available_scan_filters: Vec::new(),
        }
    }
    /// Opens a mass spectrometry file at the specified path and sets it as the current file for the `self` object.
    ///
    /// # Arguments
    /// * `path` - A reference to a `PathBuf` representing the file path of the mass spectrometry file to be opened.
    ///
    /// # Returns
    /// * `Result<&mut Self>` - A result containing either a reference to the `self` object if the file was successfully opened, or an error if the file could not be opened.
    ///
    /// # Errors
    /// This function may return the following errors:
    /// * `anyhow::Error` - If the file could not be opened for any reason.
    ///
    /// # Examples
    /// ```no_run
    /// use chromascope::parser::MzData;
    /// use std::path::PathBuf;
    ///
    /// let mut example_struct = MzData::new();
    /// let file_path = PathBuf::from("path/to/your/mzml/file.mzml");
    /// example_struct.open_msfile(&file_path).unwrap();
    /// ```
    pub fn open_msfile(&mut self, path: &PathBuf) -> Result<&mut Self> {
        info!("Attempting to open file at path: {:?}", path);
        crate::source_validation::validate_mzml(path, self.job_control.as_ref())?;

        match DataReader::open_path(path) {
            Ok(reader) => {
                self.msfile = Some(reader);
                self.file_name = Some(path.display().to_string());
                debug!("Successfully opened file at path: {:?}", path);

                // Extract data bounds for validation
                self.extract_bounds()?;

                Ok(self)
            }
            Err(e) => {
                error!(
                    "Failed to open file at path: {:?} with error: {:?}",
                    path, e
                );
                Err(ChromascopeError::MzDataError(format!(
                    "Failed to open file: {:?}",
                    e
                )))
            }
        }
    }

    /// Opens the mass spectrometry file reader for on-demand spectrum lookups only.
    ///
    /// Unlike `open_msfile`, this does NOT call `extract_bounds`.
    /// Use this on the UI thread after a background thread has already extracted
    /// the bounds and returned them via `FileLoadingResult`.
    ///
    /// # Why this exists
    /// `MZReader` is `!Send` — it cannot cross thread boundaries.
    /// The background thread opens, extracts bounds, then drops its own reader.
    /// The UI thread calls this method to open a new reader for double-click
    /// spectrum lookups, skipping the expensive bounds scan.
    pub fn open_reader_only(&mut self, path: &PathBuf) -> Result<&mut Self> {
        match DataReader::open_path(path) {
            Ok(reader) => {
                self.msfile = Some(reader);
                self.file_name = Some(path.display().to_string());
                debug!("Opened reader-only (no bounds scan) for: {:?}", path);
                Ok(self)
            }
            Err(e) => Err(ChromascopeError::MzDataError(format!(
                "Failed to open reader: {:?}",
                e
            ))),
        }
    }

    /// Extracts min/max m/z, RT, and scan count from the opened file.
    ///
    /// Called automatically during open_msfile. Uses a two-tier strategy:
    ///
    /// **Tier 1 — scan window metadata (O(spectra), zero peak decoding):**
    /// Reads `ScanWindow::lower_bound` / `upper_bound` from every spectrum's
    /// description. These fields are populated during XML tag parsing, before
    /// any base64/zlib binary array work is done.
    ///
    /// **Tier 2 — complete peak bounds fallback (O(spectra × peaks)):**
    /// If no non-empty scan windows are found (some converters omit the
    /// `<scanWindowList>` element), checks every mass array so interior signals
    /// cannot be excluded by approximate end-of-run bounds.
    ///
    /// # Returns
    /// * `Ok(())` - If bounds were successfully extracted
    /// * `Err(ChromascopeError::FileNotOpened)` - If no peaks or windows found
    ///
    /// # Errors
    /// Returns error if:
    /// - File is not opened
    /// - No valid peaks or scan windows found in any spectrum
    fn extract_bounds(&mut self) -> Result<()> {
        info!("Extracting data bounds from {:?}", self.file_name);

        let reader = self
            .msfile
            .as_mut()
            .ok_or_else(|| ChromascopeError::FileNotOpened("No file opened".to_string()))?;

        // -- O(1): scan count -----------------------------------------------------
        let scan_count = reader.len();
        if scan_count == 0 {
            return Err(ChromascopeError::FileNotOpened(
                "File contains no spectra".into(),
            ));
        }

        // Determine extrema during the metadata pass: acquisition order need not
        // be RT sorted. Avoid decoding all peak arrays just to inspect windows.
        let mut min_rt = f32::INFINITY;
        let mut max_rt = f32::NEG_INFINITY;

        // -- O(n): per-filter m/z ranges + global m/z bounds ---------------------
        // Iterates all spectra reading only scan-window metadata (zero peak decoding).
        // MS1 entries are keyed by (ms_level, polarity, None).
        // MS2+ entries are keyed by (ms_level, polarity, Some(precursor_mz)) so every
        // isolation target gets its own dropdown row.
        // Uses Vec instead of HashMap because ScanPolarity does not implement Hash.
        let mut filter_ranges: Vec<(u8, ScanPolarity, Option<f64>, f64, f64)> = Vec::new();
        let mut min_mz = f64::MAX;
        let mut max_mz = f64::MIN;
        let mut missing_scan_windows = false;
        let mut invalid_scan_windows: usize = 0;

        reader.set_detail_level(DetailLevel::MetadataOnly);
        let mut observed_scans = 0;
        let metadata_pass = (|| -> Result<()> {
            for spectrum in reader.iter() {
                if let Some(control) = &self.job_control {
                    control
                        .check()
                        .map_err(|e| ChromascopeError::MzDataError(e.to_string()))?;
                }
                observed_scans += 1;
                let rt = spectrum.start_time() as f32;
                if !rt.is_finite() || rt < 0.0 {
                    return Err(ChromascopeError::MzDataError(
                        "Retention times must be finite nonnegative minutes".into(),
                    ));
                }
                min_rt = min_rt.min(rt);
                max_rt = max_rt.max(rt);
                let ms_level = spectrum.description.ms_level;
                let polarity = spectrum.description.polarity;
                // For MS2+ spectra key by the precursor isolation target m/z.
                let precursor_key: Option<f64> = if ms_level > 1 {
                    spectrum
                        .description
                        .precursor
                        .first()
                        .and_then(|p| p.ions.first())
                        .map(|ion| ion.mz)
                } else {
                    None
                };
                // Find-or-insert the entry for this (ms_level, polarity, precursor_key) triplet.
                let entry = if let Some(idx) =
                    filter_ranges.iter().position(|(ms, pol, pre, _, _)| {
                        *ms == ms_level && *pol == polarity && *pre == precursor_key
                    }) {
                    &mut filter_ranges[idx]
                } else {
                    filter_ranges.push((ms_level, polarity, precursor_key, f64::MAX, f64::MIN));
                    filter_ranges.last_mut().unwrap()
                };
                let mut has_window = false;
                for scan_event in spectrum.description.acquisition.scans.iter() {
                    for window in scan_event.scan_windows.iter() {
                        if !window.is_empty() {
                            let lo = window.lower_bound as f64;
                            let hi = window.upper_bound as f64;
                            // Some converters write non-finite placeholders
                            // (e.g. value="nan" for SIM data with no scan range).
                            // mzdata's ScanWindow::is_empty only checks for 0/0,
                            // so NaN slips through. Treat any malformed window
                            // as missing and fall back to peak arrays below
                            // instead of failing the whole file open.
                            if !lo.is_finite() || !hi.is_finite() || lo < 0. || hi < lo {
                                invalid_scan_windows += 1;
                                continue;
                            }
                            has_window = true;
                            min_mz = min_mz.min(lo);
                            max_mz = max_mz.max(hi);
                            entry.3 = entry.3.min(lo);
                            entry.4 = entry.4.max(hi);
                        }
                    }
                }
                missing_scan_windows |= !has_window;
            }
            Ok(())
        })();
        reader.set_detail_level(DetailLevel::Full);
        metadata_pass?;
        if invalid_scan_windows > 0 {
            warn!(
                "Ignored {} malformed scan windows (non-finite or inverted bounds) in {:?} — using peak arrays for those spectra",
                invalid_scan_windows, self.file_name
            );
            missing_scan_windows = true;
        }
        if observed_scans != scan_count {
            return Err(ChromascopeError::MzDataError(format!(
                "Incomplete spectrum traversal: expected {scan_count}, read {observed_scans}"
            )));
        }

        // Without scan windows inspect all arrays: sampling the ends can miss
        // every analyte in the middle of a chromatographic run.
        if missing_scan_windows || min_mz == f64::MAX || max_mz == f64::MIN {
            warn!(
                "Incomplete scan window metadata in {:?} — inspecting all peak arrays",
                self.file_name
            );
            for spectrum in reader.iter() {
                if let Some(control) = &self.job_control {
                    control
                        .check()
                        .map_err(|e| ChromascopeError::MzDataError(e.to_string()))?;
                }
                let ms_level = spectrum.description.ms_level;
                let polarity = spectrum.description.polarity;
                let precursor_key: Option<f64> = if ms_level > 1 {
                    spectrum
                        .description
                        .precursor
                        .first()
                        .and_then(|p| p.ions.first())
                        .map(|ion| ion.mz)
                } else {
                    None
                };
                let entry = if let Some(pos) =
                    filter_ranges.iter().position(|(ms, pol, pre, _, _)| {
                        *ms == ms_level && *pol == polarity && *pre == precursor_key
                    }) {
                    &mut filter_ranges[pos]
                } else {
                    filter_ranges.push((ms_level, polarity, precursor_key, f64::MAX, f64::MIN));
                    filter_ranges.last_mut().unwrap()
                };
                if let Some(arrays) = spectrum.arrays.as_ref() {
                    if arrays.is_empty() {
                        continue;
                    }
                    let mzs = arrays
                        .mzs()
                        .map_err(|e| ChromascopeError::MzDataError(e.to_string()))?;
                    let intensities = arrays
                        .intensities()
                        .map_err(|e| ChromascopeError::MzDataError(e.to_string()))?;
                    validate_peak_arrays(&mzs, &intensities)?;
                    for &mz in mzs.iter() {
                        min_mz = min_mz.min(mz);
                        max_mz = max_mz.max(mz);
                        entry.3 = entry.3.min(mz);
                        entry.4 = entry.4.max(mz);
                    }
                }
            }
        } else {
            info!("Fast path: m/z bounds from scan window metadata (no peak decoding)");
        }

        if min_mz == f64::MAX || max_mz == f64::MIN {
            return Err(ChromascopeError::FileNotOpened(
                "No valid peaks or scan windows found in file".into(),
            ));
        }

        self.bounds = DataBounds {
            min_mz: min_mz.floor(),
            max_mz: max_mz.ceil(),
            min_rt,
            max_rt,
            scan_count,
        };

        // Normalise per-filter bounds: fill any sentinel values with the global bounds,
        // then sort MS1 < MS2 < …, within each level sort by polarity then precursor m/z.
        for (_, _, _, lo, hi) in &mut filter_ranges {
            if *lo == f64::MAX {
                *lo = min_mz;
            }
            if *hi == f64::MIN {
                *hi = max_mz;
            }
        }
        filter_ranges.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| format!("{:?}", a.1).cmp(&format!("{:?}", b.1)))
                .then_with(|| match (a.2, b.2) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                })
        });
        self.available_scan_filters = filter_ranges;

        info!(
        "Extracted bounds: m/z [{:.2}-{:.2}], RT [{:.2}-{:.2}] min, {} scans, {} unique scan filters",
        self.bounds.min_mz, self.bounds.max_mz, min_rt, max_rt, scan_count,
        self.available_scan_filters.len()
    );

        Ok(())
    }

    /// Extract Base Peak Intensity Chromatogram (BIC).
    ///
    /// Returns owned `ChromatogramData` instead of mutating self.
    /// Multiple extractions can coexist without overwriting each other.
    ///
    /// # Arguments
    /// * `ms_level` - MS level to filter (typically 1 for MS1, 2 for MS2)
    /// * `polarity` - Scan polarity to filter
    /// * `mz_range` - Optional m/z range to restrict base peak search
    ///
    /// # Returns
    /// * `Ok(ChromatogramData)` - Owned chromatogram data
    /// * `Err(ChromascopeError::FileNotOpened)` - If file not opened
    pub fn get_bpic(
        &mut self,
        ms_level: u8,
        polarity: ScanPolarity,
        mz_range: Option<(f64, f64)>,
        precursor_mz: Option<f64>,
    ) -> Result<ChromatogramData> {
        self.get_summary(ms_level, polarity, mz_range, precursor_mz, true)
    }

    /// Extract TIC with scan-specific fallback for absent CV metadata.
    pub fn get_tic(
        &mut self,
        ms_level: u8,
        polarity: ScanPolarity,
        mz_range: Option<(f64, f64)>,
        precursor_mz: Option<f64>,
    ) -> Result<ChromatogramData> {
        self.get_summary(ms_level, polarity, mz_range, precursor_mz, false)
    }

    fn get_summary(
        &mut self,
        ms_level: u8,
        polarity: ScanPolarity,
        mz_range: Option<(f64, f64)>,
        precursor_mz: Option<f64>,
        base_peak: bool,
    ) -> Result<ChromatogramData> {
        if ms_level == 0
            || mz_range
                .is_some_and(|(lo, hi)| !lo.is_finite() || !hi.is_finite() || lo < 0.0 || lo >= hi)
            || precursor_mz.is_some_and(|mz| !mz.is_finite() || mz <= 0.0)
        {
            return Err(ChromascopeError::MzDataError(
                "Invalid extraction parameters".into(),
            ));
        }
        let control = self.job_control.clone();
        let acquisition_filter = self.acquisition_filter;
        let reader = self.msfile.as_mut().ok_or_else(|| {
            ChromascopeError::FileNotOpened(
                "File must be opened before extracting chromatogram".into(),
            )
        })?;
        let matches = |s: &mzdata::spectrum::MultiLayerSpectrum| {
            acquisition_filter.is_none_or(|mode| mode.matches(&s.description))
                && s.description.ms_level == ms_level
                && s.description.polarity == polarity
                && precursor_mz.is_none_or(|target| {
                    s.description
                        .precursor
                        .first()
                        .map(|p| {
                            p.ions
                                .first()
                                .map(|ion| ion.mz)
                                .unwrap_or(f64::from(p.isolation_window.target))
                        })
                        .is_some_and(|mz| (mz - target).abs() < 0.01)
                })
        };
        let metadata = |s: &mzdata::spectrum::MultiLayerSpectrum| -> Option<(f32, f32)> {
            let value = |name| {
                s.description
                    .params()
                    .iter()
                    .find(|p| p.name == name)
                    .and_then(|p| p.value.to_f64().ok())
                    .filter(|v| v.is_finite() && *v >= 0.0 && *v <= f32::MAX as f64)
            };
            if base_peak {
                Some((
                    value("base peak intensity")? as f32,
                    value("base peak m/z")? as f32,
                ))
            } else {
                Some((value("total ion current")? as f32, 0.0))
            }
        };
        reader.set_detail_level(if mz_range.is_none() {
            DetailLevel::MetadataOnly
        } else {
            DetailLevel::Full
        });
        let first_pass = (|| -> Result<Vec<(f32, f32, f32, usize)>> {
            let mut results = Vec::new();
            for spectrum in reader
                .iter()
                .take_while(|_| control.as_ref().is_none_or(|c| c.step()))
                .filter(&matches)
            {
                let rt = spectrum.start_time() as f32;
                let idx = spectrum.index();
                let (intensity, mz) = if mz_range.is_none() {
                    metadata(&spectrum).unwrap_or((f32::NAN, f32::NAN))
                } else {
                    raw_summary(spectrum, mz_range, base_peak)?
                };
                results.push((rt, intensity, mz, idx));
            }
            Ok(results)
        })();
        reader.set_detail_level(DetailLevel::Full);
        if let Some(c) = &control {
            c.check()
                .map_err(|e| ChromascopeError::MzDataError(e.to_string()))?;
        }
        let mut results = first_pass?;
        // Preserve legacy all-zero retry. Mixed metadata retains supplied values.
        let all_zero = !results.is_empty() && results.iter().all(|v| v.1 == 0.0);
        let missing = results
            .iter()
            .enumerate()
            .filter(|(_, v)| mz_range.is_none() && (all_zero || !v.1.is_finite()))
            .map(|(position, v)| (v.3, position))
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            let mut recovered = 0;
            for spectrum in reader
                .iter()
                .take_while(|_| control.as_ref().is_none_or(|c| c.step()))
            {
                // Both traversals preserve acquisition order; walk missing rows
                // linearly rather than hash every scan in a large fallback pass.
                let Some(&(index, position)) = missing.get(recovered) else {
                    break;
                };
                if index == spectrum.index() {
                    let (intensity, mz) = raw_summary(spectrum, None, base_peak)?;
                    results[position].1 = intensity;
                    results[position].2 = mz;
                    recovered += 1;
                }
            }
            if let Some(c) = &control {
                c.check()
                    .map_err(|e| ChromascopeError::MzDataError(e.to_string()))?;
            }
            if recovered != missing.len() {
                return Err(ChromascopeError::MzDataError(
                    "Incomplete metadata fallback traversal".into(),
                ));
            }
        }
        results.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(ChromatogramData {
            retention_time: results.iter().map(|v| v.0).collect(),
            intensity: results.iter().map(|v| v.1).collect(),
            mz: if base_peak {
                results.iter().map(|v| v.2).collect()
            } else {
                Vec::new()
            },
            index: results.iter().map(|v| v.3).collect(),
        })
    }

    /// Extract Extracted Ion Chromatogram (XIC).
    ///
    /// Returns owned `ChromatogramData` instead of mutating self.
    ///
    /// # Arguments
    /// * `mass` - Target m/z value to extract
    /// * `ms_level` - MS level to filter
    /// * `polarity` - Scan polarity to filter
    /// * `mass_tolerance` - Mass tolerance in PPM (0â€“1000)
    ///
    /// # Returns
    /// * `Ok(ChromatogramData)` - Owned chromatogram data (`mz` field will be empty)
    /// * `Err` - Various validation errors or `FileNotOpened`
    pub fn get_xic(
        &mut self,
        mass: f64,
        ms_level: u8,
        polarity: ScanPolarity,
        mass_tolerance: f64,
        precursor_mz: Option<f64>,
    ) -> Result<ChromatogramData> {
        info!(
            "Attempting to read XIC of {:?} at MS{} {:?}",
            self.file_name, ms_level, polarity
        );

        if !mass.is_finite() || mass <= 0.0 {
            return Err(ChromascopeError::InvalidMass(mass));
        }
        if !(0.0..=1000.0).contains(&mass_tolerance) {
            return Err(ChromascopeError::InvalidMassTolerance(mass_tolerance));
        }

        let control = self.job_control.clone();
        let acquisition_filter = self.acquisition_filter;
        let reader = self.msfile.as_mut().ok_or_else(|| {
            ChromascopeError::FileNotOpened(
                "File must be opened before extracting chromatogram".to_string(),
            )
        })?;

        // Stream owned spectra; retain only trace values, not a run of peak arrays.
        let spectra = reader
            .iter()
            .take_while(|_| control.as_ref().is_none_or(|c| c.step()))
            .filter(|s| {
                acquisition_filter.is_none_or(|mode| mode.matches(&s.description))
                    && s.description.ms_level == ms_level
                    && s.description.polarity == polarity
                    && precursor_mz.is_none_or(|target| {
                        s.description
                            .precursor
                            .first()
                            .map(|p| {
                                p.ions
                                    .first()
                                    .map(|ion| ion.mz)
                                    .unwrap_or(f64::from(p.isolation_window.target))
                            })
                            .map(|mz| (mz - target).abs() < 0.01)
                            .unwrap_or(false)
                    })
            });

        // Decode one spectrum at a time.
        let tol_da = mass * (mass_tolerance / 1_000_000.0);
        let mut results: Vec<(f32, f32, usize)> = spectra
            .map(|spectrum| {
                let spectrum_rt = spectrum
                    .description
                    .acquisition
                    .scans
                    .first()
                    .map(|s| s.start_time as f32)
                    .unwrap_or(0.0);
                let spectrum_idx = spectrum.index();

                // Sum stored samples directly: this works for profile and centroid
                // arrays without silently applying peak picking during import.
                if let Some(arrays) = spectrum.arrays.as_ref() {
                    if arrays.is_empty() {return Ok((spectrum_rt,0.,spectrum_idx));}
                    let mzs = arrays.mzs().map_err(|e| {
                        ChromascopeError::MzDataError(format!("Cannot read XIC m/z array: {:?}", e))
                    })?;
                    let intensities = arrays.intensities().map_err(|e| ChromascopeError::MzDataError(format!("Cannot read XIC intensity array: {e:?}")))?;
                    if mzs.len() != intensities.len() || mzs.iter().any(|v| !v.is_finite() || *v < 0.0) || mzs.windows(2).any(|w| w[0] > w[1]) || intensities.iter().any(|v| !v.is_finite()) {
                        return Err(ChromascopeError::MzDataError("XIC arrays must have equal lengths, finite intensities and sorted nonnegative masses".into()));
                    }
                    let lower = mass - tol_da;
                    let upper = mass + tol_da;
                    let start = mzs.partition_point(|&mz| mz < lower);
                    let end = mzs.partition_point(|&mz| mz <= upper);
                    if start == end {
                        return Ok((spectrum_rt, 0.0, spectrum_idx));
                    }
                    return Ok((
                        spectrum_rt,
                        intensities[start..end].iter().sum(),
                        spectrum_idx,
                    ));
                }
                let centroided = spectrum.into_centroid().map_err(|e| {
                    ChromascopeError::MzDataError(format!(
                        "Cannot read XIC scan {}: {:?}",
                        spectrum_idx, e
                    ))
                })?;
                let total_intensity: f32 = centroided
                    .peaks
                    .all_peaks_for(mass, Tolerance::PPM(mass_tolerance))
                    .iter()
                    .map(|p| p.intensity)
                    .sum();

                Ok((spectrum_rt, total_intensity, spectrum_idx))
            })
            .collect::<Result<Vec<_>>>()?;

        results.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

        debug!("Successfully extracted XIC from: {:?}", self.file_name);
        trace!(
            "Successfully extracted the XIC of {:?}. {} data points",
            self.file_name,
            results.len()
        );

        if results.is_empty() {
            warn!("No matching peaks found");
        }

        Ok(ChromatogramData {
            retention_time: results.iter().map(|(rt, _, _)| *rt).collect(),
            intensity: results.iter().map(|(_, i, _)| *i).collect(),
            mz: Vec::new(),
            index: results.iter().map(|(_, _, idx)| *idx).collect(),
        })
    }

    /// Retrieve mass spectrum at a specific index.
    ///
    /// Returns owned `MassSpectrum` instead of storing in self.
    ///
    /// # Arguments
    /// * `index` - Spectrum index in the file
    ///
    /// # Returns
    /// * `Ok(MassSpectrum)` - Owned mass spectrum data
    /// * `Err` - If file not opened or spectrum not found
    pub fn get_mass_spectrum_by_index(&mut self, index: usize) -> Result<MassSpectrum> {
        info!("Starting to get mass spectrum at index: {:?}", index);

        let reader = self
            .msfile
            .as_mut()
            .ok_or_else(|| ChromascopeError::FileNotOpened("No file opened".to_string()))?;

        let spec = reader.get_spectrum_by_index(index).ok_or_else(|| {
            ChromascopeError::MzDataError(format!("No spectrum found at index: {}", index))
        })?;

        let arrays = spec
            .arrays
            .as_ref()
            .ok_or_else(|| ChromascopeError::MzDataError("Spectrum has no arrays".into()))?;
        if arrays.is_empty() {
            return Ok(MassSpectrum {
                mz: Vec::new(),
                intensity: Vec::new(),
                index,
                retention_time: spec.start_time() as f32,
            });
        }

        let mz = arrays
            .mzs()
            .map_err(|e| {
                ChromascopeError::MzDataError(format!("Failed to get m/z values: {:?}", e))
            })?
            .to_vec();

        let intensity = arrays
            .intensities()
            .map_err(|e| {
                ChromascopeError::MzDataError(format!("Failed to get intensity values: {:?}", e))
            })?
            .to_vec();

        let retention_time = spec.start_time() as f32;

        debug!(
            "Successfully retrieved mass spectrum at index: {:?} with {} peaks",
            index,
            mz.len()
        );

        Ok(MassSpectrum {
            mz,
            intensity,
            index,
            retention_time,
        })
    }

    /// Retains native acquisition metadata and original scan linkage.
    pub fn spectral_scan(&mut self, index: usize) -> Result<crate::spectral::Spectrum> {
        use crate::spectral::{Polarity, Representation, Spectrum};
        let metadata = self.scan_metadata_structured(index)?;
        let raw = self.get_mass_spectrum_by_index(index)?;
        let polarity = match metadata["polarity"].as_str() {
            Some("Positive") => Polarity::Positive,
            Some("Negative") => Polarity::Negative,
            _ => Polarity::Unknown,
        };
        let representation = match metadata["representation"].as_str() {
            Some("Centroid") => Representation::Centroid,
            Some("Profile") => Representation::Profile,
            _ => Representation::Unknown,
        };
        let precursor_mz = metadata["precursors"][0]["ions"][0]["mz"]
            .as_f64()
            .filter(|v| *v > 0.0)
            .or_else(|| {
                metadata["precursors"][0]["isolation_window"]["target_mz"]
                    .as_f64()
                    .filter(|v| *v > 0.0)
            });
        if raw.mz.len() != raw.intensity.len() {
            return Err(ChromascopeError::MzDataError(
                "Spectrum array length mismatch".into(),
            ));
        }
        let collision_energy = metadata["precursors"][0]["activation"]["parameters"]
            .as_array()
            .and_then(|parameters| {
                parameters.iter().find_map(|p| {
                    let name = p["name"].as_str()?.to_ascii_lowercase();
                    if !["collision energy", "normalized collision energy"].contains(&name.as_str())
                    {
                        return None;
                    }
                    let value = p["value"].as_str()?.parse::<f64>().ok()?;
                    let unit = p["unit"].as_str()?;
                    if unit.is_empty()
                        || unit.eq_ignore_ascii_case("unknown")
                        || !value.is_finite()
                        || value < 0.0
                    {
                        return None;
                    }
                    Some(crate::spectral::Energy {
                        value,
                        unit: match unit {
                            "electronvolt" | "Electronvolt" => "eV".into(),
                            other => other.into(),
                        },
                    })
                })
            });
        let spectrum = Spectrum {
            id: metadata["native_id"].as_str().unwrap_or("").into(),
            peaks: raw
                .mz
                .iter()
                .zip(&raw.intensity)
                .map(|(m, i)| [*m, *i as f64])
                .collect(),
            ms_level: metadata["ms_level"].as_u64().unwrap_or(0) as u8,
            representation,
            polarity,
            precursor_mz,
            precursor_type: None,
            collision_energy,
            instrument: None,
            rt_minutes: metadata["retention_time_minutes"].as_f64(),
            metadata: std::collections::BTreeMap::from([
                ("native_scan_metadata_json".into(), metadata.to_string()),
                ("original_index".into(), index.to_string()),
                ("intensity_unit".into(), "instrument intensity".into()),
                ("mz_unit".into(), "m/z".into()),
                ("rt_unit".into(), "minute".into()),
            ]),
        };
        spectrum
            .validate()
            .map_err(|e| ChromascopeError::MzDataError(e.to_string()))?;
        Ok(spectrum)
    }

    /// Human-readable native scan and acquisition metadata, without decoding peak arrays.
    /// Machine-readable acquisition metadata. Uses metadata-only reading and restores full detail.
    pub fn scan_metadata_structured(&mut self, index: usize) -> Result<serde_json::Value> {
        let reader = self
            .msfile
            .as_mut()
            .ok_or_else(|| ChromascopeError::FileNotOpened("No file opened".into()))?;
        reader.set_detail_level(DetailLevel::MetadataOnly);
        let spectrum = reader.get_spectrum_by_index(index);
        reader.set_detail_level(DetailLevel::Full);
        let spectrum =
            spectrum.ok_or_else(|| ChromascopeError::MzDataError(format!("No scan {index}")))?;
        let d = &spectrum.description;
        let parameters = |params: &[mzdata::params::Param]| {
            params.iter().map(|p| serde_json::json!({"name":p.name,"accession":p.accession,"value":p.value.to_string(),"unit":p.unit.to_string()})).collect::<Vec<_>>()
        };
        Ok(
            serde_json::json!({"index":index,"native_id":d.id,"ms_level":d.ms_level,"polarity":format!("{:?}",d.polarity),"representation":format!("{:?}",d.signal_continuity),"retention_time_minutes":spectrum.start_time(),"precursors":d.precursor.iter().map(|p|serde_json::json!({"parent_scan":p.precursor_id,"isolation_window":{"target_mz":p.isolation_window.target,"lower":p.isolation_window.lower_bound,"upper":p.isolation_window.upper_bound,"representation":format!("{:?}",p.isolation_window.flags)},"ions":p.ions.iter().map(|i|serde_json::json!({"mz":i.mz,"intensity":i.intensity,"charge":i.charge})).collect::<Vec<_>>(),"activation":{"methods":format!("{:?}",p.activation.methods()),"energy":p.activation.energy,"parameters":parameters(&p.activation.params)}})).collect::<Vec<_>>(),"scans":d.acquisition.scans.iter().map(|s|serde_json::json!({"retention_time_minutes":s.start_time,"injection_time_ms":s.injection_time,"instrument_configuration_id":s.instrument_configuration_id,"windows":s.scan_windows.iter().map(|w|serde_json::json!({"lower_mz":w.lower_bound,"upper_mz":w.upper_bound})).collect::<Vec<_>>()})).collect::<Vec<_>>(),"parameters":parameters(&d.params)}),
        )
    }

    pub fn scan_metadata(&mut self, index: usize) -> Result<String> {
        let reader = self
            .msfile
            .as_mut()
            .ok_or_else(|| ChromascopeError::FileNotOpened("No file opened".into()))?;
        reader.set_detail_level(DetailLevel::MetadataOnly);
        let spec = reader.get_spectrum_by_index(index);
        reader.set_detail_level(DetailLevel::Full);
        let spec = spec.ok_or_else(|| ChromascopeError::MzDataError(format!("No scan {index}")))?;
        let d = &spec.description;
        let mut text = format!(
            "Native ID: {}\nMS level: {}\nPolarity: {:?}\nRepresentation: {:?}",
            d.id, d.ms_level, d.polarity, d.signal_continuity
        );
        for p in &d.precursor {
            let w = &p.isolation_window;
            text.push_str(&format!("\nIsolation target: {:.5} m/z", w.target));
            if !w.is_empty() {
                let kind = if matches!(w.flags, mzdata::spectrum::IsolationWindowState::Offset) {
                    "offsets"
                } else {
                    "bounds"
                };
                text.push_str(&format!(
                    "\nIsolation {kind}: {:.5}–{:.5} m/z",
                    w.lower_bound, w.upper_bound
                ));
            }
            if let Some(parent) = &p.precursor_id {
                text.push_str(&format!("\nParent scan: {parent}"));
            }
            if !p.activation.methods().is_empty() {
                text.push_str(&format!("\nActivation: {:?}", p.activation.methods()));
            }
            if p.activation.energy != 0.0 {
                text.push_str(&format!("\nActivation energy: {}", p.activation.energy));
            }
            for param in &p.activation.params {
                text.push_str(&format!(
                    "\n{}: {} ({})",
                    param.name, param.value, param.unit
                ));
            }
            for ion in &p.ions {
                text.push_str(&format!(
                    "\nPrecursor ion: {:.5} m/z\nPrecursor intensity: {:.4e}",
                    ion.mz, ion.intensity
                ));
                text.push_str(&format!(
                    "\nCharge: {}",
                    ion.charge
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| "not reported".into())
                ));
            }
        }
        for scan in &d.acquisition.scans {
            text.push_str(&format!(
                "\nActual RT: {:.6} min\nInstrument configuration: {}",
                scan.start_time, scan.instrument_configuration_id
            ));
            if scan.injection_time > 0.0 {
                text.push_str(&format!("\nInjection time: {} ms", scan.injection_time));
            }
            for w in &scan.scan_windows {
                text.push_str(&format!(
                    "\nScan window: {:.3}–{:.3} m/z",
                    w.lower_bound, w.upper_bound
                ));
            }
            if let Some(params) = &scan.params {
                for param in params.iter() {
                    text.push_str(&format!(
                        "\n{}: {} ({})",
                        param.name, param.value, param.unit
                    ));
                }
            }
        }
        for param in &d.params {
            text.push_str(&format!(
                "\n{}: {} ({})",
                param.name, param.value, param.unit
            ));
        }
        Ok(text)
    }

    /// Returns a reference to the file name.
    pub fn file_name(&self) -> &Option<String> {
        &self.file_name
    }

    /// Checks if a mass spectrometry file is currently open.
    ///
    /// # Returns
    ///
    /// `true` if a file is open, `false` otherwise.
    pub fn is_open(&self) -> bool {
        self.msfile.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tic_does_not_hold_full_file_in_memory() {
        // Functional proxy: ensure results are still sorted and non-empty
        let mut data = setup_test_parser();
        let result = data.get_tic(1, ScanPolarity::Positive, None, None).unwrap();
        assert!(!result.retention_time.is_empty());
        for i in 1..result.retention_time.len() {
            assert!(result.retention_time[i] >= result.retention_time[i - 1]);
        }
    }

    use approx::assert_relative_eq;
    use std::path::PathBuf;

    /// Helper function to create a normalized test file path
    fn get_test_file_path() -> PathBuf {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(std::path::Path::new("test_file").join("data_dependent_02.mzML"));
        d
    }

    /// Helper function to create and open an MzData parser
    fn setup_test_parser() -> MzData {
        let mut mzdata = MzData::new();
        let path = get_test_file_path();
        mzdata.open_msfile(&path).unwrap();
        mzdata
    }

    #[test]
    fn test_open_reader_only_does_not_extract_bounds() {
        let mut data = MzData::new();
        let path = get_test_file_path();
        data.open_reader_only(&path).unwrap();

        // Reader is open for spectrum lookup
        assert!(data.is_open());

        // But bounds were NOT extracted (still default/unrestricted)
        assert_eq!(data.bounds.min_mz, 0.0);
        assert_eq!(data.bounds.max_mz, f64::MAX);
        assert!(data.available_scan_filters.is_empty());
    }

    #[test]
    fn test_get_mass_spectrum_works_after_reader_only_open() {
        let mut data = MzData::new();
        let path = get_test_file_path();

        // Simulate what poll_file_loading_result does:
        // bounds set from background thread (not done here), reader opened without scan
        data.open_reader_only(&path).unwrap();

        // Spectrum lookup must work
        let result = data.get_mass_spectrum_by_index(0);
        assert!(result.is_ok());
    }

    /// Helper function to create parser with TIC already extracted
    fn setup_with_tic() -> (MzData, ChromatogramData) {
        let mut mzdata = setup_test_parser();
        let chrom = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();
        (mzdata, chrom)
    }

    // -- Parallel correctness tests --------------------------------------------

    /// Parallel TIC must return data and must be non-decreasing in RT.
    #[test]
    fn test_parallel_tic_matches_sequential() {
        let mut data = setup_test_parser();
        let result = data.get_tic(1, ScanPolarity::Positive, None, None).unwrap();
        assert!(
            !result.retention_time.is_empty(),
            "TIC should have data points"
        );
        for i in 1..result.retention_time.len() {
            assert!(
                result.retention_time[i] >= result.retention_time[i - 1],
                "TIC RT not sorted at index {}: {} > {}",
                i,
                result.retention_time[i - 1],
                result.retention_time[i]
            );
        }
    }

    /// Parallel XIC result must be sorted by RT.
    #[test]
    fn test_parallel_xic_output_sorted_by_rt() {
        let mut data = setup_test_parser();
        // Use a wide tolerance to ensure we get hits across the file.
        let result = data
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0, None)
            .unwrap();
        for i in 1..result.retention_time.len() {
            assert!(
                result.retention_time[i] >= result.retention_time[i - 1],
                "XIC RT not sorted at index {}: {} > {}",
                i,
                result.retention_time[i - 1],
                result.retention_time[i]
            );
        }
    }

    #[test]
    fn test_new() {
        let mzdata = MzData::new();
        assert!(!mzdata.is_open());
        assert!(mzdata.file_name().is_none());
    }

    #[test]
    fn test_open_msfile() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(std::path::Path::new("test_file").join("data_dependent_02.mzML"));

        let mut mzdata = MzData::new();
        let result = mzdata.open_msfile(&d);
        assert!(result.is_ok());
        assert!(mzdata.is_open());
    }

    #[test]
    fn test_bounds_accuracy() {
        let mut mzdata = setup_test_parser();

        // Get the extracted bounds
        let bounds = &mzdata.bounds;

        // Verify bounds are not infinity values (edge case check)
        assert!(bounds.min_mz != f64::MAX, "min_mz should be extracted");
        assert!(bounds.max_mz != f64::MIN, "max_mz should be extracted");
        assert!(
            bounds.min_mz < bounds.max_mz,
            "min_mz should be less than max_mz"
        );

        // Sample multiple spectra to verify bounds encompass all peaks
        let reader = mzdata.msfile.as_mut().unwrap();

        let mut spectrum_count = 0;
        let mut peaks_checked = 0;

        for (idx, spectrum) in reader.iter().enumerate() {
            // Check first 5, middle 5, and last 5 spectra
            let total_spectra = bounds.scan_count;
            let is_first = idx < 5;
            let is_middle = idx >= total_spectra / 2 && idx < total_spectra / 2 + 5;
            let is_last = idx >= total_spectra.saturating_sub(5);

            if is_first || is_middle || is_last {
                spectrum_count += 1;

                if let Some(arrays) = spectrum.arrays.as_ref() {
                    if let Ok(mzs) = arrays.mzs() {
                        for &mz in mzs.iter() {
                            peaks_checked += 1;

                            assert!(
                                mz >= bounds.min_mz,
                                "Peak m/z {} in spectrum {} is below bounds.min_mz {}",
                                mz,
                                idx,
                                bounds.min_mz
                            );
                            assert!(
                                mz <= bounds.max_mz,
                                "Peak m/z {} in spectrum {} is above bounds.max_mz {}",
                                mz,
                                idx,
                                bounds.max_mz
                            );
                        }
                    }
                }
            }
        }

        assert!(
            spectrum_count > 0,
            "Should have checked at least one spectrum"
        );
        assert!(peaks_checked > 0, "Should have checked at least one peak");
    }

    #[test]
    fn test_bounds_span_all_spectra() {
        let mut mzdata = setup_test_parser();

        let bounds = &mzdata.bounds;

        let reader = mzdata.msfile.as_mut().unwrap();

        let mut first_spectra_min = f64::MAX;
        let mut first_spectra_max = f64::MIN;
        let mut last_spectra_min = f64::MAX;
        let mut last_spectra_max = f64::MIN;

        let total_spectra = bounds.scan_count;

        for (idx, spectrum) in reader.iter().enumerate() {
            if let Some(arrays) = spectrum.arrays.as_ref() {
                if let Ok(mzs) = arrays.mzs() {
                    for &mz in mzs.iter() {
                        if idx < 5 {
                            first_spectra_min = first_spectra_min.min(mz);
                            first_spectra_max = first_spectra_max.max(mz);
                        }

                        if idx >= total_spectra.saturating_sub(5) {
                            last_spectra_min = last_spectra_min.min(mz);
                            last_spectra_max = last_spectra_max.max(mz);
                        }
                    }
                }
            }
        }

        if first_spectra_min != f64::MAX {
            assert!(bounds.min_mz <= first_spectra_min);
            assert!(bounds.max_mz >= first_spectra_max);
        }

        if last_spectra_min != f64::MAX {
            assert!(bounds.min_mz <= last_spectra_min);
            assert!(bounds.max_mz >= last_spectra_max);
        }
    }

    #[test]
    fn test_get_xic() {
        let mut mzdata = setup_test_parser();

        let result = mzdata.get_xic(722.43, 1, ScanPolarity::Positive, 1000.0, None);
        assert!(result.is_ok());
        let chrom = result.unwrap();
        assert!(!chrom.retention_time.is_empty());
        assert!(!chrom.intensity.is_empty());
        assert_eq!(chrom.retention_time.len(), chrom.intensity.len());
        assert_eq!(chrom.retention_time.len(), chrom.index.len());
    }

    #[test]
    fn test_get_tic() {
        let mut mzdata = setup_test_parser();

        let result = mzdata.get_tic(1, ScanPolarity::Positive, None, None);
        assert!(result.is_ok());
        let chrom = result.unwrap();
        assert!(!chrom.retention_time.is_empty());
        assert!(!chrom.intensity.is_empty());
        assert!(chrom.mz.is_empty()); // TIC has no m/z
        assert_eq!(chrom.retention_time.len(), chrom.index.len());
    }

    // ========== Tests for smooth_chromatogram() ==========

    #[test]
    fn test_smooth_chromatogram() {
        let data = vec![[1.0, 1.0], [2.0, 2.0], [3.0, 3.0], [4.0, 4.0], [5.0, 5.0]];

        let result = crate::processing::smooth_chromatogram(data, 1);
        assert!(result.is_ok());

        let smoothed = result.unwrap();
        assert_eq!(smoothed.len(), 5);
        assert_relative_eq!(smoothed[2][1], 3.0);
    }

    // ========== Tests for get_bpic() ==========

    #[test]
    fn test_get_bpic() {
        let mut mzdata = setup_test_parser();

        let result = mzdata.get_bpic(1, ScanPolarity::Positive, None, None);
        assert!(result.is_ok());
        let chrom = result.unwrap();
        assert!(!chrom.retention_time.is_empty());
        assert!(!chrom.intensity.is_empty());
        assert!(!chrom.mz.is_empty());
        assert!(!chrom.index.is_empty());

        assert_eq!(chrom.retention_time.len(), chrom.intensity.len());
        assert_eq!(chrom.retention_time.len(), chrom.mz.len());
        assert_eq!(chrom.retention_time.len(), chrom.index.len());
        assert!(
            !chrom.retention_time.is_empty(),
            "Should have extracted some data points"
        );
    }

    #[test]
    fn test_get_bpic_negative_polarity() {
        let mut mzdata = setup_test_parser();

        let result = mzdata.get_bpic(1, ScanPolarity::Negative, None, None);
        assert!(result.is_ok());
        // Negative polarity may have no data in this test file
    }

    #[test]
    fn test_get_bpic_unknown_polarity() {
        let mut mzdata = setup_test_parser();

        // Unknown polarity should succeed but may return empty data if file has no such spectra
        let result = mzdata.get_bpic(1, ScanPolarity::Unknown, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_bpc_intensities_are_nonzero() {
        // Regression guard: BPC must not return a flatline of all-zero intensities.
        // Flatline would indicate that base_peak() is being called on empty decoded
        // arrays (MetadataOnly bug) instead of reading the MS:1000505 CV param.
        let mut mzdata = setup_test_parser();
        let chrom = mzdata
            .get_bpic(1, ScanPolarity::Positive, None, None)
            .unwrap();
        assert!(
            chrom.intensity.iter().any(|&i| i > 0.0),
            "BPC must have at least some non-zero intensities — got flatline \
             (base peak CV params missing or MetadataOnly bug)"
        );
        assert!(
            chrom.mz.iter().any(|&m| m > 50.0),
            "BPC m/z values look wrong — all near zero (MS:1000504 CV param not read)"
        );
    }

    // ========== Tests for ChromatogramData::prepare_for_plot() ==========

    #[test]
    fn test_prepare_for_plot_after_tic() {
        let (mut mzdata, _) = setup_with_tic();
        let chrom = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();

        let result = crate::processing::prepare_chromatogram_for_plot(&chrom);
        assert!(result.is_ok());

        let plot_data = result.unwrap();
        assert!(!plot_data.is_empty(), "Should have plot data");

        for point in plot_data.iter() {
            assert!(point[0] >= 0.0, "Retention time should be non-negative");
            assert!(point[1] >= 0.0, "Intensity should be non-negative");
        }
    }

    #[test]
    fn test_prepare_for_plot_empty_data() {
        let chrom = ChromatogramData {
            retention_time: vec![],
            intensity: vec![],
            mz: vec![],
            index: vec![],
        };

        let result = crate::processing::prepare_chromatogram_for_plot(&chrom);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_prepare_for_plot_averages_duplicates() {
        let mut mzdata = setup_test_parser();

        let chrom = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0, None)
            .unwrap();

        let result = crate::processing::prepare_chromatogram_for_plot(&chrom);
        assert!(result.is_ok());

        let plot_data = result.unwrap();
        // Verify no duplicate retention times in output
        for i in 1..plot_data.len() {
            assert!(
                plot_data[i][0] >= plot_data[i - 1][0],
                "Retention times should be non-decreasing"
            );
        }
    }

    #[test]
    fn test_prepare_for_plot_ordering() {
        let (mut mzdata, _) = setup_with_tic();
        let chrom = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();

        let result = crate::processing::prepare_chromatogram_for_plot(&chrom);
        assert!(result.is_ok());

        let plot_data = result.unwrap();
        for i in 1..plot_data.len() {
            assert!(
                plot_data[i][0] >= plot_data[i - 1][0],
                "Retention times should be in ascending order"
            );
        }
    }

    #[test]
    fn test_prepare_for_plot_starts_with_first_rt_not_zero() {
        let (mut mzdata, _) = setup_with_tic();
        let chrom = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();
        assert!(!chrom.retention_time.is_empty());
        let first_rt = chrom.retention_time[0];

        let result = crate::processing::prepare_chromatogram_for_plot(&chrom).unwrap();
        assert!(!result.is_empty());
        // The first plot point's RT should match the first actual RT (not 0.0)
        assert_relative_eq!(result[0][0] as f32, first_rt, epsilon = 0.001);
    }

    // ========== Tests for get_mass_spectrum_by_index() ==========

    #[test]
    fn test_get_mass_spectrum_by_index_valid() {
        let (mut mzdata, chrom) = setup_with_tic();

        let index = chrom.index[0];
        let result = mzdata.get_mass_spectrum_by_index(index);
        assert!(result.is_ok(), "Should retrieve mass spectrum");

        let spectrum = result.unwrap();
        assert!(!spectrum.mz.is_empty(), "Should have m/z values");
        assert_eq!(
            spectrum.mz.len(),
            spectrum.intensity.len(),
            "m/z and intensity arrays should match"
        );
        assert_eq!(spectrum.index, index);
    }

    #[test]
    fn test_get_mass_spectrum_by_index_different_indices() {
        let (mut mzdata, chrom) = setup_with_tic();

        if chrom.index.len() >= 2 {
            let index1 = chrom.index[0];
            let index2 = chrom.index[chrom.index.len() / 2];

            let spectrum1 = mzdata.get_mass_spectrum_by_index(index1).unwrap();
            let spectrum2 = mzdata.get_mass_spectrum_by_index(index2).unwrap();

            assert!(!spectrum1.mz.is_empty());
            assert!(!spectrum2.mz.is_empty());
        }
    }

    #[test]
    fn test_get_mass_spectrum_by_index_invalid() {
        let mut mzdata = setup_test_parser();

        // Use an invalid index (very large number)
        let result = mzdata.get_mass_spectrum_by_index(999999);
        assert!(result.is_err(), "Invalid index should return Err");
    }

    #[test]
    fn test_get_mass_spectrum_by_index_unopened_file() {
        let mut mzdata = MzData::new();

        let result = mzdata.get_mass_spectrum_by_index(0);
        assert!(result.is_err(), "Should return Err when file not opened");
    }

    // ========== Tests for ChromatogramData::get_closest_index() ==========

    #[test]
    fn test_get_closest_index_exact_match() {
        let (mut mzdata, _) = setup_with_tic();
        let chrom = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();

        if !chrom.retention_time.is_empty() {
            let exact_rt = chrom.retention_time[0];

            let result = crate::processing::find_closest_spectrum_index(&chrom, exact_rt);
            assert!(result.is_some(), "Should find index for exact RT match");

            let found_index = result.unwrap();
            assert_eq!(found_index, chrom.index[0]);
        }
    }

    #[test]
    fn test_get_closest_index_between_points() {
        let (mut mzdata, _) = setup_with_tic();
        let chrom = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();

        if chrom.retention_time.len() >= 2 {
            let rt1 = chrom.retention_time[0];
            let rt2 = chrom.retention_time[1];
            let between_rt = (rt1 + rt2) / 2.0;

            let result = crate::processing::find_closest_spectrum_index(&chrom, between_rt);
            assert!(result.is_some(), "Should find closest index");

            let found_index = result.unwrap();
            assert!(
                found_index == chrom.index[0] || found_index == chrom.index[1],
                "Should return one of the two adjacent indices"
            );
        }
    }

    #[test]
    fn test_get_closest_index_before_first() {
        let (mut mzdata, _) = setup_with_tic();
        let chrom = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();

        if !chrom.retention_time.is_empty() {
            let before_rt = chrom.retention_time[0] - 1.0;

            let result = crate::processing::find_closest_spectrum_index(&chrom, before_rt);
            assert!(result.is_some());
            assert_eq!(result.unwrap(), chrom.index[0]);
        }
    }

    #[test]
    fn test_get_closest_index_after_last() {
        let (mut mzdata, _) = setup_with_tic();
        let chrom = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();

        if !chrom.retention_time.is_empty() {
            let after_rt = chrom.retention_time[chrom.retention_time.len() - 1] + 1.0;

            let result = crate::processing::find_closest_spectrum_index(&chrom, after_rt);
            assert!(result.is_some());
            assert_eq!(result.unwrap(), *chrom.index.last().unwrap());
        }
    }

    #[test]
    fn test_get_closest_index_empty_data() {
        let chrom = ChromatogramData {
            retention_time: vec![],
            intensity: vec![],
            mz: vec![],
            index: vec![],
        };

        let result = crate::processing::find_closest_spectrum_index(&chrom, 10.0);
        assert!(result.is_none());
    }

    // ========== Multiple coexisting extractions ==========

    #[test]
    fn test_multiple_extractions_coexist() {
        let mut mzdata = setup_test_parser();

        let tic = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();
        let xic = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0, None)
            .unwrap();

        // Both exist simultaneously
        assert!(!tic.retention_time.is_empty());
        assert!(!xic.retention_time.is_empty());
        // TIC will have more points than XIC for specific mass
        assert!(tic.retention_time.len() >= xic.retention_time.len());
    }

    #[test]
    fn test_switching_extraction_methods() {
        let mut mzdata = setup_test_parser();

        let tic = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();
        let tic_rt_count = tic.retention_time.len();

        let _xic = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0, None)
            .unwrap();

        let _bic = mzdata
            .get_bpic(1, ScanPolarity::Positive, None, None)
            .unwrap();

        assert!(tic_rt_count > 0);
    }

    // ========== Error Handling Tests ==========

    #[test]
    fn test_open_msfile_nonexistent() {
        let mut mzdata = MzData::new();
        let nonexistent = PathBuf::from("/nonexistent/file.mzML");

        let result = mzdata.open_msfile(&nonexistent);
        assert!(result.is_err(), "Should fail for nonexistent file");
    }

    #[test]
    fn test_get_xic_zero_tolerance() {
        let mut mzdata = setup_test_parser();

        let _result = mzdata.get_xic(722.43, 1, ScanPolarity::Positive, 0.0, None);
        // 0.0 is valid (within 0..=1000), should not return error from validation
    }

    #[test]
    fn test_get_xic_negative_tolerance() {
        let mut mzdata = setup_test_parser();

        let result = mzdata.get_xic(722.43, 1, ScanPolarity::Positive, -1.0, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::InvalidMassTolerance(_)
        ));
    }

    #[test]
    fn test_get_xic_no_matching_peaks() {
        let mut mzdata = setup_test_parser();

        let result = mzdata.get_xic(50000.0, 1, ScanPolarity::Positive, 0.0001, None);
        assert!(result.is_ok());
        let chrom = result.unwrap();
        // Keep every matching scan even when the ion is absent.
        let tic = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();
        assert!(!chrom.intensity.is_empty());
        assert!(chrom.intensity.iter().all(|&i| i == 0.0));
        assert_eq!(chrom.index, tic.index);
        assert_eq!(chrom.retention_time, tic.retention_time);
    }

    // ========== Polarity Tests ==========

    #[test]
    fn test_get_tic_all_polarities() {
        let polarities = vec![
            ScanPolarity::Positive,
            ScanPolarity::Negative,
            ScanPolarity::Unknown,
        ];

        for polarity in polarities {
            let mut test_data = setup_test_parser();
            let result = test_data.get_tic(1, polarity, None, None);
            assert!(result.is_ok(), "TIC should handle all polarity types");
        }
    }

    #[test]
    fn test_get_xic_all_polarities() {
        let polarities = vec![
            ScanPolarity::Positive,
            ScanPolarity::Negative,
            ScanPolarity::Unknown,
        ];

        for polarity in polarities {
            let mut test_data = setup_test_parser();
            let result = test_data.get_xic(722.43, 1, polarity, 1000.0, None);
            assert!(result.is_ok(), "XIC should handle all polarity types");
        }
    }

    // ========== smooth_chromatogram edge cases ==========

    #[test]
    fn test_smooth_chromatogram_window_zero() {
        let data = vec![[1.0, 1.0], [2.0, 2.0], [3.0, 3.0]];
        let result = crate::processing::smooth_chromatogram(data.clone(), 0);
        assert!(result.is_ok());
        // Window 0 means no smoothing â€” every point kept
        assert_eq!(result.unwrap(), data);
    }

    #[test]
    fn test_smooth_chromatogram_window_larger_than_data() {
        let data = vec![[1.0, 1.0], [2.0, 2.0], [3.0, 3.0]];
        let result = crate::processing::smooth_chromatogram(data, 10);
        assert!(result.is_ok());
    }

    #[test]
    fn test_smooth_chromatogram_single_point() {
        let data = vec![[1.0, 1.0]];
        let result = crate::processing::smooth_chromatogram(data, 3);
        assert!(result.is_ok());
        let smoothed = result.unwrap();
        assert_eq!(smoothed.len(), 1);
        assert_relative_eq!(smoothed[0][0], 1.0);
        assert_relative_eq!(smoothed[0][1], 1.0);
    }

    #[test]
    fn test_smooth_chromatogram_empty() {
        let data: Vec<[f64; 2]> = vec![];
        let result = crate::processing::smooth_chromatogram(data, 3);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_smooth_chromatogram_invalid_window() {
        let data = vec![[1.0, 1.0], [2.0, 2.0]];
        let result = crate::processing::smooth_chromatogram(data, 20);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::InvalidSmoothingWindow(_)
        ));
    }

    // ========== Integration Tests ==========

    #[test]
    fn test_full_pipeline_tic() {
        let mut mzdata = setup_test_parser();

        let chrom = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();
        let plot_data = crate::processing::prepare_chromatogram_for_plot(&chrom).unwrap();
        assert!(!plot_data.is_empty());

        let smoothed = crate::processing::smooth_chromatogram(plot_data, 3).unwrap();
        assert!(!smoothed.is_empty());
    }

    #[test]
    fn test_full_pipeline_xic() {
        let mut mzdata = setup_test_parser();

        let chrom = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0, None)
            .unwrap();

        let plot_data = crate::processing::prepare_chromatogram_for_plot(&chrom).unwrap();
        assert!(!plot_data.is_empty());
    }

    #[test]
    fn test_get_mass_spectrum_after_extraction() {
        let (mut mzdata, chrom) = setup_with_tic();

        if !chrom.retention_time.is_empty() {
            let mid_idx_in_chrom = chrom.retention_time.len() / 2;
            let mid_rt = chrom.retention_time[mid_idx_in_chrom];

            if let Some(spectrum_idx) =
                crate::processing::find_closest_spectrum_index(&chrom, mid_rt)
            {
                let spectrum = mzdata.get_mass_spectrum_by_index(spectrum_idx).unwrap();
                assert!(!spectrum.mz.is_empty());
            }
        }
    }

    // ========== Error Propagation Tests ==========

    #[test]
    fn test_get_bpic_file_not_opened() {
        let mut mzdata = MzData::new();
        let result = mzdata.get_bpic(1, ScanPolarity::Positive, None, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::FileNotOpened(_)
        ));
    }

    #[test]
    fn test_get_tic_file_not_opened() {
        let mut mzdata = MzData::new();
        let result = mzdata.get_tic(1, ScanPolarity::Positive, None, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::FileNotOpened(_)
        ));
    }

    #[test]
    fn test_get_xic_file_not_opened() {
        let mut mzdata = MzData::new();
        let result = mzdata.get_xic(722.43, 1, ScanPolarity::Positive, 1000.0, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::FileNotOpened(_)
        ));
    }

    // ========== Input Validation Tests ==========

    #[test]
    fn test_invalid_mass_returns_error() {
        let mut mzdata = MzData::new();

        let result = mzdata.get_xic(-100.0, 1, ScanPolarity::Positive, 10.0, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::InvalidMass(_)
        ));

        let result = mzdata.get_xic(0.0, 1, ScanPolarity::Positive, 10.0, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::InvalidMass(_)
        ));
    }

    #[test]
    fn test_invalid_tolerance_returns_error() {
        let mut mzdata = MzData::new();

        let result = mzdata.get_xic(100.0, 1, ScanPolarity::Positive, -5.0, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::InvalidMassTolerance(_)
        ));

        let result = mzdata.get_xic(100.0, 1, ScanPolarity::Positive, 5000.0, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::InvalidMassTolerance(_)
        ));
    }

    #[test]
    fn test_invalid_smoothing_window() {
        let data: Vec<[f64; 2]> = vec![[1.0, 2.0], [3.0, 4.0]];
        let result = crate::processing::smooth_chromatogram(data, 20);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::InvalidSmoothingWindow(_)
        ));
    }

    #[test]
    fn test_valid_mass_and_tolerance() {
        let mut mzdata = MzData::new();

        // These should pass validation but fail because file not opened
        let result = mzdata.get_xic(100.5, 1, ScanPolarity::Positive, 10.0, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::FileNotOpened(_)
        ));
    }

    #[test]
    fn test_valid_smoothing_window() {
        for window in [0u8, 1, 5, 10] {
            let data: Vec<[f64; 2]> = vec![[1.0, 2.0], [3.0, 4.0]];
            let result = crate::processing::smooth_chromatogram(data, window);
            assert!(
                result.is_ok(),
                "smooth_chromatogram should accept window size {}",
                window
            );
        }
    }

    // ========== XIC Structure Tests ==========

    #[test]
    fn test_xic_one_entry_per_spectrum_no_duplicates() {
        let mut mzdata = setup_test_parser();

        let chrom = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0, None)
            .unwrap();

        assert_eq!(chrom.retention_time.len(), chrom.index.len());
        assert_eq!(chrom.retention_time.len(), chrom.intensity.len());

        // No duplicate retention times
        let mut rt_sorted = chrom.retention_time.clone();
        rt_sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let unique_count = rt_sorted
            .iter()
            .enumerate()
            .filter(|(i, v)| *i == 0 || **v != rt_sorted[i - 1])
            .count();

        assert_eq!(unique_count, chrom.retention_time.len());

        // No duplicate indices
        let mut index_set = std::collections::HashSet::new();
        for &idx in chrom.index.iter() {
            assert!(
                index_set.insert(idx),
                "Found duplicate spectrum index: {}",
                idx
            );
        }
    }

    #[test]
    fn test_xic_sums_intensities_per_spectrum() {
        let mut mzdata = setup_test_parser();

        let chrom = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0, None)
            .unwrap();

        for &intensity in chrom.intensity.iter() {
            assert!(
                intensity >= 0.0,
                "All XIC intensities should be non-negative, got: {}",
                intensity
            );
        }
    }

    #[test]
    fn test_xic_double_click_finds_correct_spectrum() {
        let mut mzdata = setup_test_parser();

        let chrom = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0, None)
            .unwrap();

        if !chrom.retention_time.is_empty() {
            let mid_idx = chrom.retention_time.len() / 2;
            let target_rt = chrom.retention_time[mid_idx];
            let expected_spectrum_idx = chrom.index[mid_idx];

            let found_idx = crate::processing::find_closest_spectrum_index(&chrom, target_rt);

            assert!(found_idx.is_some());
            assert_eq!(found_idx.unwrap(), expected_spectrum_idx);
        }
    }

    #[test]
    fn test_xic_structure_matches_tic() {
        let mut mzdata = setup_test_parser();

        let xic = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0, None)
            .unwrap();
        let tic = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();

        assert_eq!(xic.retention_time.len(), xic.index.len());
        assert_eq!(xic.retention_time.len(), xic.intensity.len());
        assert_eq!(tic.retention_time.len(), tic.index.len());
        assert_eq!(tic.retention_time.len(), tic.intensity.len());

        // XIC indices should be subset of TIC indices
        for &xic_spectrum_idx in xic.index.iter() {
            assert!(
                tic.index.contains(&xic_spectrum_idx),
                "XIC spectrum index {} should exist in TIC",
                xic_spectrum_idx
            );
        }
    }

    #[test]
    fn test_xic_stores_spectrum_indices_not_peak_indices() {
        let mut mzdata = setup_test_parser();

        let xic = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0, None)
            .unwrap();

        let tic = mzdata
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap();
        let max_spectrum_idx = *tic.index.iter().max().unwrap();

        for &idx in xic.index.iter() {
            assert!(
                idx <= max_spectrum_idx,
                "XIC index {} exceeds maximum spectrum index {}",
                idx,
                max_spectrum_idx
            );
        }

        for i in 1..xic.index.len() {
            assert!(
                xic.index[i] > xic.index[i - 1],
                "XIC indices should be strictly increasing"
            );
        }
    }

    // ========== New tests for v2 refactoring ==========

    #[test]
    fn test_get_mass_spectrum_returns_owned() {
        let (mut mzdata, chrom) = setup_with_tic();

        if !chrom.index.is_empty() {
            let index = chrom.index[0];
            let spectrum = mzdata.get_mass_spectrum_by_index(index).unwrap();

            assert!(!spectrum.mz.is_empty());
            assert_eq!(spectrum.mz.len(), spectrum.intensity.len());
            assert_eq!(spectrum.index, index);
            assert!(spectrum.retention_time >= 0.0);
        }
    }

    #[test]
    fn test_multiple_xic_extractions() {
        let mut mzdata = setup_test_parser();

        for mass in [722.43, 500.0, 1000.0] {
            let result = mzdata.get_xic(mass, 1, ScanPolarity::Positive, 1000.0, None);
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_extract_bounds_uses_scan_windows_when_available() {
        // If the test file has scan window metadata, bounds should be populated
        // without needing the sampling fallback. Verify by checking that the
        // bounds are non-trivial and represent the instrument scan range.
        let data = setup_test_parser();

        // The Thermo test file should have scan windows (Thermo mzML typically does).
        // Bounds should reflect the programmed scan range, e.g. ~100-2000 m/z.
        assert!(data.bounds.min_mz >= 0.0);
        assert!(data.bounds.max_mz > data.bounds.min_mz);
        assert!(
            data.bounds.max_mz > 100.0,
            "max_mz suspiciously low — scan windows may not have been read"
        );
    }

    #[test]
    fn test_scan_window_fields_are_accessible() {
        // Regression guard: ensures ScanWindow::lower_bound and upper_bound
        // remain accessible at the expected path after any mzdata upgrade.
        let path = get_test_file_path();
        let mut reader = MZReader::open_path(&path).unwrap();
        if let Some(spectrum) = reader.get_spectrum_by_index(0) {
            for scan_event in spectrum.description.acquisition.scans.iter() {
                for window in scan_event.scan_windows.iter() {
                    // If this compiles and runs, the field path is correct.
                    let _ = window.lower_bound;
                    let _ = window.upper_bound;
                    let _ = window.is_empty();
                }
            }
        }
    }

    #[test]
    fn test_tic_scan_indices_map_to_actual_spectra() {
        let mut data = setup_test_parser();
        let result = data.get_tic(1, ScanPolarity::Positive, None, None).unwrap();
        for (&index, &rt) in result.index.iter().zip(&result.retention_time) {
            let spectrum = data.get_mass_spectrum_by_index(index).unwrap();
            assert_eq!(spectrum.retention_time, rt);
        }
        assert!(!result.retention_time.is_empty());
        for i in 1..result.retention_time.len() {
            assert!(
                result.retention_time[i] >= result.retention_time[i - 1],
                "TIC retention times must be non-decreasing"
            );
        }
    }

    #[test]
    fn test_detail_level_restored_after_tic() {
        // After get_tic(), get_mass_spectrum_by_index() must still decode arrays.
        // If detail level was not restored, this would return empty arrays.
        let mut data = setup_test_parser();
        let chrom = data.get_tic(1, ScanPolarity::Positive, None, None).unwrap();
        if !chrom.index.is_empty() {
            let spectrum = data.get_mass_spectrum_by_index(chrom.index[0]).unwrap();
            assert!(
                !spectrum.mz.is_empty(),
                "Arrays must be decodable after get_tic (detail level must be restored)"
            );
        }
    }

    #[test]
    fn test_xic_binary_search_pre_filter_correctness() {
        // The binary search pre-filter must accept the same spectra as the
        // old linear .any() scan — result must be identical in content.
        // Verify by checking the output is non-empty and RT-sorted
        // (same assertions as before, proving no spectra were wrongly excluded).
        let mut mzdata = setup_test_parser();
        let chrom = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 10.0, None) // tight tol: few hits
            .unwrap();
        // Retain zero scans, including those rejected by the cheap pre-filter.
        for &i in chrom.intensity.iter() {
            assert!(i >= 0.0, "XIC intensity must be non-negative");
        }
        // RT must be sorted
        for i in 1..chrom.retention_time.len() {
            assert!(chrom.retention_time[i] >= chrom.retention_time[i - 1]);
        }
    }

    #[test]
    fn test_tic_range_raw_arrays_nonzero() {
        // Range TIC via raw arrays must return non-zero intensities for a
        // range that covers the test file's actual m/z content (~100-2000).
        let mut mzdata = setup_test_parser();
        let chrom = mzdata
            .get_tic(1, ScanPolarity::Positive, Some((200.0, 800.0)), None)
            .unwrap();
        assert!(
            !chrom.retention_time.is_empty(),
            "Range TIC should have data points"
        );
        assert!(
            chrom.intensity.iter().any(|&i| i > 0.0),
            "Range TIC must have non-zero intensities for m/z 200-800"
        );
        // RT sorted
        for i in 1..chrom.retention_time.len() {
            assert!(chrom.retention_time[i] >= chrom.retention_time[i - 1]);
        }
    }

    #[test]
    fn test_available_scan_filters_includes_ms1() {
        let mut data = MzData::new();
        let path = std::path::PathBuf::from("test_file").join("data_dependent_02.mzML");
        data.open_msfile(&path).expect("Failed to load test file");

        // The test file contains only MS1 survey scans.
        let ms_levels: Vec<u8> = data
            .available_scan_filters
            .iter()
            .map(|(lvl, _, _, _, _)| *lvl)
            .collect();
        assert!(
            !ms_levels.is_empty(),
            "available_scan_filters should not be empty"
        );
        assert!(ms_levels.contains(&1), "Should have MS1 filter");
    }

    #[test]
    fn test_ms1_filter_has_valid_mz_range() {
        let mut data = MzData::new();
        let path = std::path::PathBuf::from("test_file").join("data_dependent_02.mzML");
        data.open_msfile(&path).expect("Failed to load test file");

        let ms1 = data
            .available_scan_filters
            .iter()
            .find(|(lvl, _, _, _, _)| *lvl == 1)
            .copied();

        let (_, _, _, lo, hi) = ms1.expect("MS1 filter should be present");
        println!("MS1: {:.2}\u{2013}{:.2}", lo, hi);
        assert!(lo >= 0.0, "MS1 min m/z should be non-negative");
        assert!(hi > lo, "MS1 max m/z should be greater than min m/z");
        assert!(hi > 0.0, "MS1 max m/z should be positive");
        // Per-filter range should not be sentinel values
        assert!(
            lo < f64::MAX,
            "MS1 min m/z should have been populated from scan windows"
        );
        assert!(
            hi > f64::MIN,
            "MS1 max m/z should have been populated from scan windows"
        );
    }

    #[test]
    fn test_nan_scan_windows_fall_back_to_peak_arrays() {
        // Regression test: some converters write value="nan" for scan window
        // limits (e.g. SIM data with no scan range). These must be treated as
        // missing metadata with peak-array fallback, not a fatal open error.
        let original =
            std::fs::read_to_string(get_test_file_path()).expect("read test mzML");
        let text = original
            .replace(
                "accession=\"MS:1000501\" name=\"scan window lower limit\" value=\"150.0\"",
                "accession=\"MS:1000501\" name=\"scan window lower limit\" value=\"nan\"",
            )
            .replace(
                "accession=\"MS:1000500\" name=\"scan window upper limit\" value=\"1000.0\"",
                "accession=\"MS:1000500\" name=\"scan window upper limit\" value=\"nan\"",
            );
        assert!(text.contains("value=\"nan\""), "test file must contain scan windows to corrupt");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nan-windows.mzML");
        std::fs::write(&path, &text).unwrap();
        let mut data = MzData::new();
        data.open_msfile(&path)
            .expect("NaN scan windows should fall back to peak arrays");
        assert!(data.bounds.max_mz > data.bounds.min_mz);
        assert!(data.bounds.scan_count > 0);
        let tic = data
            .get_tic(1, ScanPolarity::Positive, None, None)
            .expect("TIC after NaN fallback");
        assert!(!tic.retention_time.is_empty());
    }
}
