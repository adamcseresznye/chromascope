//! # backend for parsing MzML files for plotting

//! The `parser` module provides functionality for reading and processing mass spectrometry data from MzML files. It allows users to extract various types of data, including Base Peak Intensity (BIC), Total Ion Chromatogram (TIC), and Extracted Ion Chromatogram (XIC). Additionally, it offers methods for data smoothing and preparation for plotting.

//! ## Overview

//!The main struct in this crate is `MzData`, which encapsulates the data and methods necessary for handling MzML files. The struct includes fields for storing file information, retention times, intensities, mass-to-charge ratios (m/z), and more.

//!## Features

//!- **File Handling**: Open and read MzML files.
//!- **Data Extraction**: Extract BIC, TIC, and XIC based on specified parameters.
//!- **Data Processing**: Smooth data for better visualization and analysis.
//!- **Plot Preparation**: Prepare data for plotting with appropriate formatting.

#![warn(clippy::all)]

use crate::error::{ChromascopeError, Result};
use crate::validation::DataBounds;
use log::{debug, error, info, trace, warn};
use mzdata::io::mzml::MzMLReaderType;
use mzdata::spectrum::ScanPolarity;
use mzdata::{prelude::*, MzMLReader};
use std::cmp::Ordering;
use std::fs::File;
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

/// Represents extracted chromatogram data from a mass spectrometry file.
/// Returned by `get_tic()`, `get_bpic()`, and `get_xic()` methods.
#[derive(Debug, Clone)]
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

impl ChromatogramData {
    /// Prepare chromatogram data for plotting by averaging duplicate retention times.
    ///
    /// Returns a vector of `[retention_time, average_intensity]` pairs suitable for plotting.
    ///
    /// # Deprecated
    /// Use [`crate::processing::prepare_chromatogram_for_plot`] instead.
    #[deprecated(note = "Use crate::processing::prepare_chromatogram_for_plot instead")]
    pub fn prepare_for_plot(&self) -> Result<Vec<[f64; 2]>> {
        crate::processing::prepare_chromatogram_for_plot(self)
    }

    /// Find the closest spectrum index by retention time using binary search.
    ///
    /// # Arguments
    /// * `clicked_rt` - Target retention time to search for
    ///
    /// # Returns
    /// * `Some(usize)` - Spectrum index closest to the target retention time
    /// * `None` - If retention time data is empty
    ///
    /// # Deprecated
    /// Use [`crate::processing::find_closest_spectrum_index`] instead.
    #[deprecated(note = "Use crate::processing::find_closest_spectrum_index instead")]
    pub fn get_closest_index(&self, clicked_rt: f32) -> Option<usize> {
        crate::processing::find_closest_spectrum_index(self, clicked_rt)
    }
}

/// Represents a mass spectrum at a specific retention time.
#[derive(Debug, Clone)]
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
    /// An optional `String` representing the name of the data file.
    file_name: Option<String>,
    /// A `Result` containing the `MzMLReaderType<File>`, which represents the parsed mass spectrometry file.
    msfile: Result<MzMLReaderType<File>>,
    /// Valid parameter ranges for this file (extracted during opening)
    pub bounds: DataBounds,
    /// Vector of unique (ms_level, polarity) combinations found in this file
    /// Extracted during file opening for populating UI dropdowns
    pub available_scan_filters: Vec<(u8, ScanPolarity)>,
}

impl core::fmt::Debug for MzData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MzData")
            .field("file_name", &self.file_name)
            .field("msfile", &"Result<MzMLReaderType<File>>")
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
            file_name: None,
            msfile: Err(ChromascopeError::FileNotOpened(
                "No file opened yet".to_string(),
            )),
            bounds: DataBounds::unrestricted(),
            available_scan_filters: Vec::new(),
        }
    }
    /// Opens an MzML file at the specified path and sets it as the current file for the `self` object.
    ///
    /// # Arguments
    /// * `path` - A reference to a `PathBuf` representing the file path of the MzML file to be opened.
    ///
    /// # Returns
    /// * `Result<&mut Self>` - A result containing either a reference to the `self` object if the file was successfully opened, or an error if the file could not be opened.
    ///
    /// # Errors
    /// This function may return the following errors:
    /// * `anyhow::Error` - If the MzML file could not be opened for any reason.
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
        info!("Attempting to open MzML file at path: {:?}", &path);

        match MzMLReader::open_path(&path) {
            Ok(reader) => {
                self.msfile = Ok(reader);
                self.file_name = Some(path.display().to_string());
                debug!("Successfully opened MzML file at path: {:?}", &path);

                // Extract data bounds for validation
                self.extract_bounds()?;

                Ok(self)
            }
            Err(e) => {
                error!(
                    "Failed to open MzML file at path: {:?} with error: {:?}",
                    &path, e
                );
                Err(ChromascopeError::MzDataError(format!(
                    "Failed to open MzML file: {:?}",
                    e
                )))
            }
        }
    }

    /// Extracts min/max m/z, RT, and scan count from the opened file.
    ///
    /// Called automatically during open_msfile. Iterates through all spectra
    /// to determine the valid parameter ranges for this file.
    ///
    /// The m/z range is extracted from all peaks across all spectra, then rounded
    /// down (floor) and up (ceil) for nice display values.
    ///
    /// # Returns
    /// * `Ok(())` - If bounds were successfully extracted
    /// * `Err(ChromascopeError::FileNotOpened)` - If no peaks found in file
    ///
    /// # Errors
    /// Returns error if:
    /// - File is not opened
    /// - No valid peaks found in any spectrum
    fn extract_bounds(&mut self) -> Result<()> {
        info!("Extracting data bounds from {:?}", &self.file_name);

        let reader = self
            .msfile
            .as_mut()
            .map_err(|e| ChromascopeError::FileNotOpened(format!("{}", e)))?;

        // Collect all spectra once; the mzdata iterator is not parallel-safe directly.
        let spectra: Vec<_> = reader.iter().collect();

        #[cfg(not(target_arch = "wasm32"))]
        let (min_mz, max_mz, min_rt, max_rt) = spectra
            .par_iter()
            .map(|spectrum| {
                let rt = spectrum.start_time() as f32;
                let (mut lo, mut hi) = (f64::MAX, f64::MIN);
                if let Some(arrays) = spectrum.arrays.as_ref() {
                    if let Ok(mzs) = arrays.mzs() {
                        for &mz in mzs.iter() {
                            lo = lo.min(mz);
                            hi = hi.max(mz);
                        }
                    }
                }
                (lo, hi, rt, rt)
            })
            .reduce(
                || (f64::MAX, f64::MIN, f32::MAX, f32::MIN),
                |(lo1, hi1, rlo1, rhi1), (lo2, hi2, rlo2, rhi2)| {
                    (lo1.min(lo2), hi1.max(hi2), rlo1.min(rlo2), rhi1.max(rhi2))
                },
            );

        #[cfg(target_arch = "wasm32")]
        let (min_mz, max_mz, min_rt, max_rt) = {
            let mut lo = f64::MAX;
            let mut hi = f64::MIN;
            let mut rlo = f32::MAX;
            let mut rhi = f32::MIN;
            for spectrum in &spectra {
                let rt = spectrum.start_time() as f32;
                rlo = rlo.min(rt);
                rhi = rhi.max(rt);
                if let Some(arrays) = spectrum.arrays.as_ref() {
                    if let Ok(mzs) = arrays.mzs() {
                        for &mz in mzs.iter() {
                            lo = lo.min(mz);
                            hi = hi.max(mz);
                        }
                    }
                }
            }
            (lo, hi, rlo, rhi)
        };

        if min_mz == f64::MAX || max_mz == f64::MIN {
            return Err(ChromascopeError::FileNotOpened(
                "No valid peaks found in file".into(),
            ));
        }

        // Collect unique (ms_level, polarity) pairs — small, not worth parallelising.
        let mut scan_filters: Vec<(u8, ScanPolarity)> = Vec::new();
        for spectrum in &spectra {
            let pair = (
                spectrum.description.ms_level,
                spectrum.description.polarity,
            );
            if !scan_filters.contains(&pair) {
                scan_filters.push(pair);
            }
        }

        let scan_count = spectra.len();
        let min_mz = min_mz.floor();
        let max_mz = max_mz.ceil();

        self.bounds = DataBounds {
            min_mz,
            max_mz,
            min_rt,
            max_rt,
            scan_count,
        };
        self.available_scan_filters = scan_filters;

        info!(
            "Extracted bounds: m/z [{:.2}-{:.2}], RT [{:.2}-{:.2}] min, {} scans, {} unique scan filters",
            min_mz, max_mz, min_rt, max_rt, scan_count, self.available_scan_filters.len()
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
    ) -> Result<ChromatogramData> {
        info!(
            "Attempting to read BIC of {:?} at MS{} {:?}",
            &self.file_name, ms_level, polarity
        );

        if self.msfile.is_err() {
            return Err(ChromascopeError::FileNotOpened(
                "File must be opened before extracting chromatogram".to_string(),
            ));
        }

        let reader = self.msfile.as_mut().unwrap();
        let spectra: Vec<_> = reader.iter().collect();

        #[cfg(not(target_arch = "wasm32"))]
        let mut results: Vec<(f32, f32, f32, usize)> = spectra
            .par_iter()
            .filter(|s| {
                s.description.ms_level == ms_level && s.description.polarity == polarity
            })
            .map(|spectrum| {
                let rt = spectrum.start_time() as f32;
                let idx = spectrum.index();
                let (intensity, mz) = if let Some((min_mz, max_mz)) = mz_range {
                    let centroided = spectrum.clone().into_centroid().unwrap_or_else(|_| {
                        warn!("Failed to centroid spectrum at RT {}", rt);
                        // Return a zero-intensity peak on failure
                        spectrum.clone().into_centroid().unwrap()
                    });
                    let max_peak = centroided
                        .peaks
                        .iter()
                        .filter(|p| p.mz >= min_mz && p.mz <= max_mz)
                        .max_by(|a, b| {
                            a.intensity
                                .partial_cmp(&b.intensity)
                                .unwrap_or(Ordering::Equal)
                        });
                    if let Some(peak) = max_peak {
                        (peak.intensity, peak.mz as f32)
                    } else {
                        (0.0_f32, 0.0_f32)
                    }
                } else {
                    let bp = spectrum.peaks().base_peak();
                    (bp.intensity, bp.mz as f32)
                };
                (rt, intensity, mz, idx)
            })
            .collect();

        #[cfg(not(target_arch = "wasm32"))]
        results.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

        #[cfg(target_arch = "wasm32")]
        let results: Vec<(f32, f32, f32, usize)> = spectra
            .iter()
            .filter(|s| {
                s.description.ms_level == ms_level && s.description.polarity == polarity
            })
            .map(|spectrum| {
                let rt = spectrum.start_time() as f32;
                let idx = spectrum.index();
                let (intensity, mz) = if let Some((min_mz, max_mz)) = mz_range {
                    let centroided = spectrum.clone().into_centroid().unwrap_or_else(|_| {
                        warn!("Failed to centroid spectrum at RT {}", rt);
                        spectrum.clone().into_centroid().unwrap()
                    });
                    let max_peak = centroided
                        .peaks
                        .iter()
                        .filter(|p| p.mz >= min_mz && p.mz <= max_mz)
                        .max_by(|a, b| {
                            a.intensity
                                .partial_cmp(&b.intensity)
                                .unwrap_or(Ordering::Equal)
                        });
                    if let Some(peak) = max_peak {
                        (peak.intensity, peak.mz as f32)
                    } else {
                        (0.0_f32, 0.0_f32)
                    }
                } else {
                    let bp = spectrum.peaks().base_peak();
                    (bp.intensity, bp.mz as f32)
                };
                (rt, intensity, mz, idx)
            })
            .collect();

        debug!("Successfully extracted BIC from: {:?}", &self.file_name);
        trace!(
            "Successfully extracted the BIC of {:?}. {} data points",
            &self.file_name,
            results.len()
        );

        let retention_time = results.iter().map(|(rt, _, _, _)| *rt).collect();
        let intensity = results.iter().map(|(_, i, _, _)| *i).collect();
        let mz = results.iter().map(|(_, _, m, _)| *m).collect();
        let index = results.iter().map(|(_, _, _, idx)| *idx).collect();

        Ok(ChromatogramData {
            retention_time,
            intensity,
            mz,
            index,
        })
    }

    /// Extract Total Ion Chromatogram (TIC).
    ///
    /// Returns owned `ChromatogramData` instead of mutating self.
    ///
    /// # Arguments
    /// * `ms_level` - MS level to filter
    /// * `polarity` - Scan polarity to filter
    /// * `mz_range` - Optional m/z range to restrict TIC calculation
    ///
    /// # Returns
    /// * `Ok(ChromatogramData)` - Owned chromatogram data (`mz` field will be empty)
    /// * `Err(ChromascopeError::FileNotOpened)` - If file not opened
    pub fn get_tic(
        &mut self,
        ms_level: u8,
        polarity: ScanPolarity,
        mz_range: Option<(f64, f64)>,
    ) -> Result<ChromatogramData> {
        info!(
            "Attempting to read TIC of {:?} at MS{} {:?}",
            &self.file_name, ms_level, polarity
        );

        if self.msfile.is_err() {
            return Err(ChromascopeError::FileNotOpened(
                "File must be opened before extracting chromatogram".to_string(),
            ));
        }

        let reader = self.msfile.as_mut().unwrap();
        let spectra: Vec<_> = reader.iter().collect();

        #[cfg(not(target_arch = "wasm32"))]
        let mut results: Vec<(f32, f32, usize)> = spectra
            .par_iter()
            .filter(|s| {
                s.description.ms_level == ms_level && s.description.polarity == polarity
            })
            .map(|spectrum| {
                let rt = spectrum.start_time() as f32;
                let idx = spectrum.index();
                let tic = if let Some((min_mz, max_mz)) = mz_range {
                    let centroided = spectrum.clone().into_centroid().unwrap_or_else(|_| {
                        warn!("Failed to centroid spectrum at RT {}", rt);
                        spectrum.clone().into_centroid().unwrap()
                    });
                    centroided
                        .peaks
                        .iter()
                        .filter(|p| p.mz >= min_mz && p.mz <= max_mz)
                        .map(|p| p.intensity)
                        .sum()
                } else {
                    spectrum.peaks().tic()
                };
                (rt, tic, idx)
            })
            .collect();

        #[cfg(not(target_arch = "wasm32"))]
        results.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

        #[cfg(target_arch = "wasm32")]
        let results: Vec<(f32, f32, usize)> = spectra
            .iter()
            .filter(|s| {
                s.description.ms_level == ms_level && s.description.polarity == polarity
            })
            .map(|spectrum| {
                let rt = spectrum.start_time() as f32;
                let idx = spectrum.index();
                let tic = if let Some((min_mz, max_mz)) = mz_range {
                    let centroided = spectrum.clone().into_centroid().unwrap_or_else(|_| {
                        warn!("Failed to centroid spectrum at RT {}", rt);
                        spectrum.clone().into_centroid().unwrap()
                    });
                    centroided
                        .peaks
                        .iter()
                        .filter(|p| p.mz >= min_mz && p.mz <= max_mz)
                        .map(|p| p.intensity)
                        .sum()
                } else {
                    spectrum.peaks().tic()
                };
                (rt, tic, idx)
            })
            .collect();

        debug!("Successfully extracted TIC from: {:?}", &self.file_name);
        trace!(
            "Successfully extracted the TIC of {:?}. {} data points",
            &self.file_name,
            results.len()
        );

        let retention_time = results.iter().map(|(rt, _, _)| *rt).collect();
        let intensity = results.iter().map(|(_, i, _)| *i).collect();
        let index = results.iter().map(|(_, _, idx)| *idx).collect();

        Ok(ChromatogramData {
            retention_time,
            intensity,
            mz: Vec::new(),
            index,
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
    ) -> Result<ChromatogramData> {
        info!(
            "Attempting to read XIC of {:?} at MS{} {:?}",
            &self.file_name, ms_level, polarity
        );

        if mass <= 0.0 {
            return Err(ChromascopeError::InvalidMass(mass));
        }
        if !(0.0..=1000.0).contains(&mass_tolerance) {
            return Err(ChromascopeError::InvalidMassTolerance(mass_tolerance));
        }

        if self.msfile.is_err() {
            return Err(ChromascopeError::FileNotOpened(
                "File must be opened before extracting chromatogram".to_string(),
            ));
        }

        let reader = self.msfile.as_mut().unwrap();
        let spectra: Vec<_> = reader.iter().collect();

        // into_centroid is the most expensive per-spectrum call — highest parallelism gain.
        #[cfg(not(target_arch = "wasm32"))]
        let mut results: Vec<(f32, f32, usize)> = spectra
            .par_iter()
            .filter(|s| {
                s.description.ms_level == ms_level && s.description.polarity == polarity
            })
            .filter_map(|spectrum| {
                let spectrum_rt =
                    spectrum.description.acquisition.scans[0].start_time as f32;
                let spectrum_idx = spectrum.index();
                let centroided = spectrum.clone().into_centroid().ok()?;
                let total_intensity: f32 = centroided
                    .peaks
                    .all_peaks_for(mass, Tolerance::PPM(mass_tolerance))
                    .iter()
                    .map(|p| p.intensity)
                    .sum();
                if total_intensity > 0.0 {
                    Some((spectrum_rt, total_intensity, spectrum_idx))
                } else {
                    None
                }
            })
            .collect();

        #[cfg(not(target_arch = "wasm32"))]
        results.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

        #[cfg(target_arch = "wasm32")]
        let mut results: Vec<(f32, f32, usize)> = Vec::new();
        #[cfg(target_arch = "wasm32")]
        for spectrum in spectra
            .iter()
            .filter(|s| s.description.ms_level == ms_level && s.description.polarity == polarity)
        {
            let spectrum_idx = spectrum.index();
            let spectrum_rt = spectrum.description.acquisition.scans[0].start_time as f32;
            let centroided = spectrum.clone().into_centroid().map_err(|e| {
                ChromascopeError::MzDataError(format!("Failed to centroid spectrum: {:?}", e))
            })?;
            let total_intensity: f32 = centroided
                .peaks
                .all_peaks_for(mass, Tolerance::PPM(mass_tolerance))
                .iter()
                .map(|p| p.intensity)
                .sum();
            if total_intensity > 0.0 {
                results.push((spectrum_rt, total_intensity, spectrum_idx));
            }
        }

        debug!("Successfully extracted XIC from: {:?}", &self.file_name);
        trace!(
            "Successfully extracted the XIC of {:?}. {} data points",
            &self.file_name,
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
            .map_err(|e| ChromascopeError::FileNotOpened(format!("{}", e)))?;

        let spec = reader.get_spectrum_by_index(index).ok_or_else(|| {
            ChromascopeError::MzDataError(format!("No spectrum found at index: {}", index))
        })?;

        let arrays = spec
            .arrays
            .as_ref()
            .ok_or_else(|| ChromascopeError::MzDataError("Spectrum has no arrays".into()))?;

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

    /// Returns a reference to the file name.
    pub fn file_name(&self) -> &Option<String> {
        &self.file_name
    }

    /// Returns a reference to the msfile Result.
    pub fn msfile(&self) -> &Result<MzMLReaderType<File>> {
        &self.msfile
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use std::path::PathBuf;
    const TEST_FILE: &str = r"test_file\data_dependent_02.mzML"; //thermo example file converted to mzML (only Rt 10-12min)

    /// Helper function to create a normalized test file path
    fn get_test_file_path() -> PathBuf {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(TEST_FILE);
        PathBuf::from(d.to_str().unwrap().replace("\\", "/"))
    }

    /// Helper function to create and open an MzData parser
    fn setup_test_parser() -> MzData {
        let mut mzdata = MzData::new();
        let path = get_test_file_path();
        mzdata.open_msfile(&path).unwrap();
        mzdata
    }

    /// Helper function to create parser with TIC already extracted
    fn setup_with_tic() -> (MzData, ChromatogramData) {
        let mut mzdata = setup_test_parser();
        let chrom = mzdata.get_tic(1, ScanPolarity::Positive, None).unwrap();
        (mzdata, chrom)
    }

    // ── Parallel correctness tests ────────────────────────────────────────────

    /// Parallel TIC must return data and must be non-decreasing in RT.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_parallel_tic_matches_sequential() {
        let mut data = setup_test_parser();
        let result = data.get_tic(1, ScanPolarity::Positive, None).unwrap();
        assert!(!result.retention_time.is_empty(), "TIC should have data points");
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
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_parallel_xic_output_sorted_by_rt() {
        let mut data = setup_test_parser();
        // Use a wide tolerance to ensure we get hits across the file.
        let result = data.get_xic(722.43, 1, ScanPolarity::Positive, 1000.0).unwrap();
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
        assert!(mzdata.msfile().is_err());
        assert!(mzdata.file_name().is_none());
    }

    #[test]
    fn test_open_msfile() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(TEST_FILE);

        // Normalize the path to account for different separators
        let normalized_d = PathBuf::from(d.to_str().unwrap().replace("\\", "/"));

        let mut mzdata = MzData::new();
        let result = mzdata.open_msfile(&normalized_d);
        assert!(result.is_ok());
        assert!(mzdata.msfile().is_ok());
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
            let total_spectra = bounds.scan_count as usize;
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

        let total_spectra = bounds.scan_count as usize;

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

        let result = mzdata.get_xic(722.43, 1, ScanPolarity::Positive, 1000.0);
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

        let result = mzdata.get_tic(1, ScanPolarity::Positive, None);
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

        let result = mzdata.get_bpic(1, ScanPolarity::Positive, None);
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
            chrom.retention_time.len() > 0,
            "Should have extracted some data points"
        );
    }

    #[test]
    fn test_get_bpic_negative_polarity() {
        let mut mzdata = setup_test_parser();

        let result = mzdata.get_bpic(1, ScanPolarity::Negative, None);
        assert!(result.is_ok());
        // Negative polarity may have no data in this test file
    }

    #[test]
    fn test_get_bpic_unknown_polarity() {
        let mut mzdata = setup_test_parser();

        // Unknown polarity should succeed but may return empty data if file has no such spectra
        let result = mzdata.get_bpic(1, ScanPolarity::Unknown, None);
        assert!(result.is_ok());
    }

    // ========== Tests for ChromatogramData::prepare_for_plot() ==========

    #[test]
    fn test_prepare_for_plot_after_tic() {
        let (mut mzdata, _) = setup_with_tic();
        let chrom = mzdata.get_tic(1, ScanPolarity::Positive, None).unwrap();

        let result = chrom.prepare_for_plot();
        assert!(result.is_ok());

        let plot_data = result.unwrap();
        assert!(plot_data.len() > 0, "Should have plot data");

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

        let result = chrom.prepare_for_plot();
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_prepare_for_plot_averages_duplicates() {
        let mut mzdata = setup_test_parser();

        let chrom = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0)
            .unwrap();

        let result = chrom.prepare_for_plot();
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
        let chrom = mzdata.get_tic(1, ScanPolarity::Positive, None).unwrap();

        let result = chrom.prepare_for_plot();
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
        let chrom = mzdata.get_tic(1, ScanPolarity::Positive, None).unwrap();
        assert!(!chrom.retention_time.is_empty());
        let first_rt = chrom.retention_time[0];

        let result = chrom.prepare_for_plot().unwrap();
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
        let chrom = mzdata.get_tic(1, ScanPolarity::Positive, None).unwrap();

        if !chrom.retention_time.is_empty() {
            let exact_rt = chrom.retention_time[0];

            let result = chrom.get_closest_index(exact_rt);
            assert!(result.is_some(), "Should find index for exact RT match");

            let found_index = result.unwrap();
            assert_eq!(found_index, chrom.index[0]);
        }
    }

    #[test]
    fn test_get_closest_index_between_points() {
        let (mut mzdata, _) = setup_with_tic();
        let chrom = mzdata.get_tic(1, ScanPolarity::Positive, None).unwrap();

        if chrom.retention_time.len() >= 2 {
            let rt1 = chrom.retention_time[0];
            let rt2 = chrom.retention_time[1];
            let between_rt = (rt1 + rt2) / 2.0;

            let result = chrom.get_closest_index(between_rt);
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
        let chrom = mzdata.get_tic(1, ScanPolarity::Positive, None).unwrap();

        if !chrom.retention_time.is_empty() {
            let before_rt = chrom.retention_time[0] - 1.0;

            let result = chrom.get_closest_index(before_rt);
            assert!(result.is_some());
            assert_eq!(result.unwrap(), chrom.index[0]);
        }
    }

    #[test]
    fn test_get_closest_index_after_last() {
        let (mut mzdata, _) = setup_with_tic();
        let chrom = mzdata.get_tic(1, ScanPolarity::Positive, None).unwrap();

        if !chrom.retention_time.is_empty() {
            let after_rt = chrom.retention_time[chrom.retention_time.len() - 1] + 1.0;

            let result = chrom.get_closest_index(after_rt);
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

        let result = chrom.get_closest_index(10.0);
        assert!(result.is_none());
    }

    // ========== Multiple coexisting extractions ==========

    #[test]
    fn test_multiple_extractions_coexist() {
        let mut mzdata = setup_test_parser();

        let tic = mzdata.get_tic(1, ScanPolarity::Positive, None).unwrap();
        let xic = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0)
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

        let tic = mzdata.get_tic(1, ScanPolarity::Positive, None).unwrap();
        let tic_rt_count = tic.retention_time.len();

        let _xic = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0)
            .unwrap();

        let _bic = mzdata.get_bpic(1, ScanPolarity::Positive, None).unwrap();

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

        let _result = mzdata.get_xic(722.43, 1, ScanPolarity::Positive, 0.0);
        // 0.0 is valid (within 0..=1000), should not return error from validation
    }

    #[test]
    fn test_get_xic_negative_tolerance() {
        let mut mzdata = setup_test_parser();

        let result = mzdata.get_xic(722.43, 1, ScanPolarity::Positive, -1.0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::InvalidMassTolerance(_)
        ));
    }

    #[test]
    fn test_get_xic_no_matching_peaks() {
        let mut mzdata = setup_test_parser();

        let result = mzdata.get_xic(50000.0, 1, ScanPolarity::Positive, 0.0001);
        assert!(result.is_ok());
        let chrom = result.unwrap();
        // Empty chromatogram for very specific m/z not in file
        assert!(chrom.retention_time.is_empty() || chrom.intensity.iter().all(|&i| i > 0.0));
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
            let result = test_data.get_tic(1, polarity, None);
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
            let result = test_data.get_xic(722.43, 1, polarity, 1000.0);
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

        let chrom = mzdata.get_tic(1, ScanPolarity::Positive, None).unwrap();
        let plot_data = chrom.prepare_for_plot().unwrap();
        assert!(plot_data.len() > 0);

        let smoothed = crate::processing::smooth_chromatogram(plot_data, 3).unwrap();
        assert!(smoothed.len() > 0);
    }

    #[test]
    fn test_full_pipeline_xic() {
        let mut mzdata = setup_test_parser();

        let chrom = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0)
            .unwrap();

        let plot_data = chrom.prepare_for_plot().unwrap();
        assert!(plot_data.len() > 0);
    }

    #[test]
    fn test_get_mass_spectrum_after_extraction() {
        let (mut mzdata, chrom) = setup_with_tic();

        if !chrom.retention_time.is_empty() {
            let mid_idx_in_chrom = chrom.retention_time.len() / 2;
            let mid_rt = chrom.retention_time[mid_idx_in_chrom];

            if let Some(spectrum_idx) = chrom.get_closest_index(mid_rt) {
                let spectrum = mzdata.get_mass_spectrum_by_index(spectrum_idx).unwrap();
                assert!(!spectrum.mz.is_empty());
            }
        }
    }

    // ========== Error Propagation Tests ==========

    #[test]
    fn test_get_bpic_file_not_opened() {
        let mut mzdata = MzData::new();
        let result = mzdata.get_bpic(1, ScanPolarity::Positive, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::FileNotOpened(_)
        ));
    }

    #[test]
    fn test_get_tic_file_not_opened() {
        let mut mzdata = MzData::new();
        let result = mzdata.get_tic(1, ScanPolarity::Positive, None);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::FileNotOpened(_)
        ));
    }

    #[test]
    fn test_get_xic_file_not_opened() {
        let mut mzdata = MzData::new();
        let result = mzdata.get_xic(722.43, 1, ScanPolarity::Positive, 1000.0);
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

        let result = mzdata.get_xic(-100.0, 1, ScanPolarity::Positive, 10.0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::InvalidMass(_)
        ));

        let result = mzdata.get_xic(0.0, 1, ScanPolarity::Positive, 10.0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::InvalidMass(_)
        ));
    }

    #[test]
    fn test_invalid_tolerance_returns_error() {
        let mut mzdata = MzData::new();

        let result = mzdata.get_xic(100.0, 1, ScanPolarity::Positive, -5.0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ChromascopeError::InvalidMassTolerance(_)
        ));

        let result = mzdata.get_xic(100.0, 1, ScanPolarity::Positive, 5000.0);
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
        let result = mzdata.get_xic(100.5, 1, ScanPolarity::Positive, 10.0);
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
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0)
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
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0)
            .unwrap();

        for &intensity in chrom.intensity.iter() {
            assert!(
                intensity > 0.0,
                "All XIC intensities should be positive, got: {}",
                intensity
            );
        }
    }

    #[test]
    fn test_xic_triple_click_finds_correct_spectrum() {
        let mut mzdata = setup_test_parser();

        let chrom = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0)
            .unwrap();

        if !chrom.retention_time.is_empty() {
            let mid_idx = chrom.retention_time.len() / 2;
            let target_rt = chrom.retention_time[mid_idx];
            let expected_spectrum_idx = chrom.index[mid_idx];

            let found_idx = chrom.get_closest_index(target_rt);

            assert!(found_idx.is_some());
            assert_eq!(found_idx.unwrap(), expected_spectrum_idx);
        }
    }

    #[test]
    fn test_xic_structure_matches_tic() {
        let mut mzdata = setup_test_parser();

        let xic = mzdata
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0)
            .unwrap();
        let tic = mzdata.get_tic(1, ScanPolarity::Positive, None).unwrap();

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
            .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0)
            .unwrap();

        let tic = mzdata.get_tic(1, ScanPolarity::Positive, None).unwrap();
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
            let result = mzdata.get_xic(mass, 1, ScanPolarity::Positive, 1000.0);
            assert!(result.is_ok());
        }
    }
}
