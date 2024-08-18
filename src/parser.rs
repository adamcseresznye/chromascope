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

use anyhow::anyhow;
use anyhow::Result;
use log::{debug, error, info, trace, warn};
use mzdata::io::mzml::MzMLReaderType;
use mzdata::spectrum::ScanPolarity;
use mzdata::{prelude::*, MzMLReader};
use std::fs::File;
use std::path::PathBuf;

/// Represents a data structure for storing mass spectrometry data.
const MS_LEVEL: u8 = 1;

/// Represents a data structure for storing mass spectrometry data.
pub struct MzData {
    /// An optional `String` representing the name of the data file.
    pub file_name: Option<String>,
    /// An optional vector of `usize`corresponding to the indices.
    pub index: Option<Vec<usize>>,
    /// An optional vector of `f32` values representing retention times.
    pub retention_time: Option<Vec<f32>>,
    /// An optional vector of `f32` values representing intensity values.
    pub intensity: Option<Vec<f32>>,
    /// An optional vector of `f32` values representing m/z (mass-to-charge) ratios.
    pub mz: Option<Vec<f32>>,
    /// A `Result` containing the `MzMLReaderType<File>`, which represents the parsed mass spectrometry file.
    pub msfile: Result<MzMLReaderType<File>>,
    /// An optional vector of tuples, each containing two `f64` values for plotting data points.
    pub plot_data: Option<Vec<[f64; 2]>>,
    /// An optional tuple containing two vectors: one for mass values (`Vec<f64>`) and one for corresponding intensity values (`Vec<f32>`).
    pub mass_spectrum: Option<(Vec<f64>, Vec<f32>)>,
}

/// Provides a default implementation for `MzData`.
impl Default for MzData {
    /// Creates a new instance of `MzData` with default values.
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for MzData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MzData")
            .field("file_name", &self.file_name)
            .field("retention_time", &self.retention_time)
            .field("intensity", &self.intensity)
            .field("mz", &self.mz)
            .field("msfile", &"Result<MzMLReaderType<File>>")
            .field("plot_data", &self.plot_data)
            .field("mass_spectrum", &self.mass_spectrum)
            .finish()
    }
}
impl MzData {
    /// Creates a new instance of `MzData` with default values.
    ///
    /// This method initializes all fields of `MzData` to `None`, except for the `msfile` field,
    /// which is set to an error indicating that the file has not been opened.
    ///
    /// # Returns
    ///
    /// A new instance of `MzData` with all fields initialized.
    pub fn new() -> Self {
        Self {
            file_name: None,
            index: None,
            retention_time: None,
            intensity: None,
            mz: None,
            msfile: Err(anyhow!("File not opened")),
            plot_data: None,
            mass_spectrum: None,
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
    /// ```
    /// use std::path::PathBuf;
    ///
    /// let mut example_struct = Mzdata::default();
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
                Ok(self)
            }
            Err(e) => {
                error!(
                    "Failed to open MzML file at path: {:?} with error: {:?}",
                    &path, e
                );
                Err(anyhow!("Failed to open MzML file: {:?}", e))
            }
        }
    }
    /// Method to read the Base Peak Intensity Chromatogram (BPIC) from the associated mass spectrometry file.
    ///
    /// # Parameters
    /// - `polarity: ScanPolarity` - The polarity of the mass spectrometry scans to be considered.
    ///
    /// # Returns
    /// - `Result<&mut Self>` - A mutable reference to the current instance of the struct, or an error if the operation fails.
    ///
    /// # Functionality
    /// 1. Logs an informational message about the attempt to read the BPIC.
    /// 2. Matches the `msfile` field, which is a `Result<MsFile, Error>`, and performs the following steps:
    ///     a. Iterates over the spectra in the `MsFile` and filters them based on the provided `polarity`.
    ///     b. For each filtered spectrum, extracts the retention time, intensity, m/z, and index, and stores them in separate vectors.
    ///     c. Assigns the extracted values to the corresponding fields in the current instance of the struct (`retention_time`, `intensity`, `mz`, `index`).
    /// 3. Logs a debug message indicating the successful extraction of the BPIC.
    /// 4. Logs a trace message with the details of the extracted BPIC (retention time, index, m/z, and intensity).
    /// 5. Returns the mutable reference to the current instance of the struct.
    ///
    /// # Errors
    /// If there is an error while accessing the `msfile` field, an error message is logged, and the function returns an error.
    pub fn get_bpic(&mut self, polarity: ScanPolarity) -> Result<&mut Self> {
        info!("Attempting to read BIC of {:?}", &self.file_name);
        match &mut self.msfile {
            Ok(reader) => {
                let (retention_time, intensity, mz, index) = reader
                    .iter()
                    .filter(|spectrum| spectrum.description.polarity == polarity)
                    .map(|spectrum| {
                        let retention_time = spectrum.start_time() as f32;
                        let intensity = spectrum.peaks().base_peak().intensity;
                        let mz = spectrum.peaks().base_peak().mz as f32;
                        let index = spectrum.index();
                        (retention_time, intensity, mz, index)
                    })
                    .fold(
                        (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                        |mut acc, (rt, int, mz, index)| {
                            acc.0.push(rt);
                            acc.1.push(int);
                            acc.2.push(mz);
                            acc.3.push(index);
                            acc
                        },
                    );

                self.retention_time = Some(retention_time);
                self.intensity = Some(intensity);
                self.mz = Some(mz);
                self.index = Some(index);
                debug!("Successfully extracted BIC from: {:?}", &self.file_name);
                trace!("Successfully extracted the BIC of {:?}. Rt is {:?}, Index is {:?}, Mz is {:?}, Intensity is {:?}, ", &self.file_name, &self.retention_time, &self.index, &self.mz, &self.intensity);
            }
            Err(e) => error!("Failed to get BIC due to {:?}", e),
        }
        Ok(self)
    }
    /// Method to read the Total Ion Chromatogram (TIC) from the associated mass spectrometry file.
    ///
    /// # Parameters
    /// - `polarity: ScanPolarity` - The polarity of the mass spectrometry scans to be considered.
    ///
    /// # Returns
    /// - `Result<&mut Self>` - A mutable reference to the current instance of the struct, or an error if the operation fails.
    ///
    /// # Functionality
    /// 1. Logs an informational message about the attempt to read the TIC.
    /// 2. Matches the `msfile` field, which is a `Result<MsFile, Error>`, and performs the following steps:
    ///     a. Initializes empty vectors for `retention_time`, `intensity`, and `index`.
    ///     b. Iterates over the spectra in the `MsFile` and filters them based on the provided `polarity`.
    ///     c. For each filtered spectrum, extracts the retention time, total ion intensity, and index, and appends them to the corresponding vectors.
    ///     d. Initializes an empty vector for `mz`.
    ///     e. Assigns the extracted vectors to the corresponding fields in the current instance of the struct (`retention_time`, `intensity`, `mz`, `index`).
    /// 3. Logs a debug message indicating the successful extraction of the TIC.
    /// 4. Logs a trace message with the details of the extracted TIC (retention time, index, m/z, and intensity).
    /// 5. Returns the mutable reference to the current instance of the struct.
    ///
    /// # Errors
    /// If there is an error while accessing the `msfile` field, an error message is logged, and the function returns an error.

    pub fn get_tic(&mut self, polarity: ScanPolarity) -> Result<&mut Self> {
        info!("Attempting to read TIC of {:?}", &self.file_name);
        match &mut self.msfile {
            Ok(reader) => {
                let mut retention_time = Vec::new();
                let mut intensity = Vec::new();
                let mut index = Vec::new();

                for spectrum in reader
                    .iter()
                    .filter(|spectrum| spectrum.description.polarity == polarity)
                {
                    retention_time.push(spectrum.start_time() as f32);
                    intensity.push(spectrum.peaks().tic());
                    index.push(spectrum.index());
                }

                let mz: Vec<f32> = Vec::new();

                self.retention_time = Some(retention_time);
                self.intensity = Some(intensity);
                self.mz = Some(mz);
                self.index = Some(index);
                debug!("Successfully extracted TIC from: {:?}", &self.file_name);
                trace!("Successfully extracted the BIC of {:?}. Rt is {:?}, Index is {:?}, Mz is {:?}, Intensity is {:?}, ", &self.file_name, &self.retention_time, &self.index, &self.mz, &self.intensity);
            }
            Err(e) => error!("Failed to get TIC due to {:?}", e),
        }
        Ok(self)
    }
    /// Method to read the Extracted Ion Chromatogram (XIC) for the specified mass and polarity from the associated mass spectrometry file.
    ///
    /// # Parameters
    /// - `mass: f64` - The mass value to be extracted.
    /// - `polarity: ScanPolarity` - The polarity of the mass spectrometry scans to be considered.
    /// - `mass_tolerance: f64` - The mass tolerance (in parts per million) to be used for peak extraction.
    ///
    /// # Returns
    /// - `Result<&mut Self>` - A mutable reference to the current instance of the struct, or an error if the operation fails.
    ///
    /// # Functionality
    /// 1. Logs an informational message about the attempt to read the XIC.
    /// 2. Initializes empty vectors for `retention_time`, `intensity`, `index`, and `mz` in the current instance of the struct.
    /// 3. Matches the `msfile` field, which is a `Result<MsFile, Error>`, and performs the following steps:
    ///     a. Iterates over the spectra in the `MsFile`.
    ///     b. For each spectrum, checks if the MS level is the expected level and the polarity matches the provided one.
    ///     c. If the conditions are met, the spectrum is cloned and converted to a centroided spectrum.
    ///     d. The centroided spectrum is then used to extract the peaks that match the provided mass and mass tolerance.
    ///     e. For each extracted peak, the retention time, intensity, and index are appended to the corresponding vectors in the current instance of the struct.
    /// 4. If the `index` vector was populated, it is sorted to ensure the data is in the correct order.
    /// 5. Logs a debug message indicating the successful extraction of the XIC.
    /// 6. Logs a trace message with the details of the extracted XIC (retention time, index, m/z, and intensity).
    /// 7. If no matching peaks were found, a warning message is logged.
    /// 8. Returns the mutable reference to the current instance of the struct.
    ///
    /// # Errors
    /// If there is an error while accessing the `msfile` field or converting the spectrum to a centroided spectrum, an error message is logged, and the function returns an error.

    pub fn get_xic(
        &mut self,
        mass: f64,
        polarity: ScanPolarity,
        mass_tolerance: f64,
    ) -> Result<&mut Self> {
        info!("Attempting to read XIC of {:?}", &self.file_name);

        self.retention_time = Some(Vec::new());
        self.intensity = Some(Vec::new());
        self.index = Some(Vec::new()); // if the self.index is cleared, when triple clicked one cannot extract the mass spectrum
        self.mz = Some(Vec::new());

        match &mut self.msfile {
            Ok(reader) => {
                for spectrum in reader.iter() {
                    if spectrum.description.ms_level == MS_LEVEL
                        && spectrum.description.polarity == polarity
                    {
                        let centroided = spectrum.clone().into_centroid()?;
                        let extracted_centroided = centroided
                            .peaks
                            .all_peaks_for(mass, Tolerance::PPM(mass_tolerance));

                        for peak in extracted_centroided {
                            if let Some(rt) = &mut self.retention_time {
                                rt.push(
                                    spectrum.description.acquisition.scans[0].start_time as f32,
                                );
                            };
                            if let Some(intensity) = &mut self.intensity {
                                intensity.push(peak.intensity);
                            };
                            if let Some(index) = &mut self.index {
                                index.push(peak.index as usize);
                            };
                        }
                    }
                }
                if let Some(index) = &mut self.index {
                    index.sort()
                }; // self.index was unordered in case of XIC

                debug!("Successfully extracted XIC from: {:?}", &self.file_name);
                trace!("Successfully extracted the XIC of {:?}. Rt is {:?}, Index is {:?}, Mz is {:?}, Intensity is {:?}, ", &self.file_name, &self.retention_time, &self.index, &self.mz, &self.intensity);

                if self.retention_time.is_none() {
                    warn!("No matching peaks found");
                }
            }
            Err(e) => error!("Failed to get XIC due to {:?}", e),
        }
        Ok(self)
    }

    /// Prepares the data for plotting by processing the retention times and intensities.
    ///
    /// # Returns
    /// - `Result<Vec<[f64; 2]>>` - A vector of data points, where each data point is an array of two `f64` values representing the retention time and the average intensity, or an error if the operation fails.
    ///
    /// # Functionality
    /// 1. Logs an informational message about the start of the data preparation for plotting.
    /// 2. Initializes an empty vector `data` to store the prepared data points.
    /// 3. Initializes variables `temp_rt` (to store the current retention time) and `temp_intensity_collector` (to store the intensities for the current retention time).
    /// 4. Checks if the `retention_time` and `intensity` fields in the current instance of the struct are not `None`.
    /// 5. If the fields are not `None`, the function performs the following steps:
    ///     a. Logs a trace message with the number of retention times and intensities being processed.
    ///     b. Iterates over the retention times and intensities, and for each unique retention time:
    ///         i. Calculates the average intensity for the current retention time and adds a data point (retention time, average intensity) to the `data` vector.
    ///         ii. Clears the `temp_intensity_collector` and updates the `temp_rt` variable.
    ///     c. After the loop, if there are any remaining intensities, the function adds a final data point to the `data` vector.
    /// 6. If the `retention_time` or `intensity` fields are `None`, the function logs a warning message.
    /// 7. Logs a debug message with the number of data points prepared for plotting.
    /// 8. Returns the `data` vector.
    ///
    /// # Errors
    /// The function does not return any errors, but it may log warning messages if the required data is missing.

    pub fn prepare_for_plot(&self) -> Result<Vec<[f64; 2]>> {
        info!(
            "Starting to prepare data for plotting {:?}",
            &self.file_name
        );

        let mut data = Vec::new();
        let mut temp_rt = 0.0;
        let mut temp_intensity_collector = Vec::new();

        if let (Some(retention_times), Some(intensities)) = (&self.retention_time, &self.intensity)
        {
            trace!(
                "Processing {} retention times and intensities",
                retention_times.len()
            );

            for (idx, &rt) in retention_times.iter().enumerate() {
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
                temp_intensity_collector.push(intensities[idx].into());
            }
            // The second if statement after the loop is needed to process the remaining intensities.
            if !temp_intensity_collector.is_empty() {
                data.push([
                    temp_rt as f64,
                    temp_intensity_collector.iter().sum::<f64>()
                        / temp_intensity_collector.len() as f64,
                ]);
                trace!("Added final data point for RT: {}", temp_rt);
            }
        } else {
            warn!("Retention times or intensities are missing");
        }

        debug!(
            "Prepared {} data points for plotting {:?}",
            data.len(),
            &self.file_name
        );

        Ok(data)
    }

    /// Method to smooth the provided data using a moving average filter.
    ///
    /// # Parameters
    /// - `data: Result<Vec<[f64; 2]>>` - The data to be smoothed, represented as a vector of arrays with two `f64` values (x and y).
    /// - `window_size: u8` - The size of the smoothing window.
    ///
    /// # Returns
    /// - `Result<&mut Self>` - A mutable reference to the current instance of the struct, with the smoothed data stored in the `plot_data` field, or an error if the operation fails.
    ///
    /// # Functionality
    /// 1. Logs an informational message about the start of the data smoothing process with the specified window size.
    /// 2. Unwraps the `data` parameter, which is a `Result<Vec<[f64; 2]>>`.
    /// 3. Logs a debug message with the number of data points received for smoothing.
    /// 4. Initializes an empty vector `smoothed_data` to store the smoothed data points.
    /// 5. Iterates over the input data points:
    ///     a. If the current index is less than the window size or greater than or equal to the length of the data minus the window size, the original data point is added to the `smoothed_data` vector.
    ///     b. Otherwise, the function calculates the average of the data points within the smoothing window (the current point and the `window_size` points before and after it) and adds the smoothed data point (original x-value, average y-value) to the `smoothed_data` vector.
    /// 6. Assigns the `smoothed_data` vector to the `plot_data` field in the current instance of the struct.
    /// 7. Logs a debug message indicating that the data smoothing is complete.
    /// 8. Returns the mutable reference to the current instance of the struct.
    ///
    /// # Errors
    /// If there is an error unwrapping the `data` parameter, the function returns the error.
    pub fn smooth_data(
        &mut self,
        data: Result<Vec<[f64; 2]>>,
        window_size: u8,
    ) -> Result<&mut Self> {
        info!("Starting data smoothing with window size: {}", window_size);

        let data = data?;
        debug!("Received {} data points for smoothing", data.len());

        let mut smoothed_data = Vec::new();
        let window_size_usize = window_size as usize;

        for i in 0..data.len() {
            if i < window_size_usize || i >= data.len() - window_size_usize {
                // Not enough data to smooth, keep original
                smoothed_data.push(data[i]);
                trace!("Keeping original data point at index {}", i);
            } else {
                // Calculate the average for the smoothing window
                let sum: f64 = data[i - window_size_usize..=i + window_size_usize]
                    .iter()
                    .map(|point| point[1])
                    .sum();
                let average = sum / (f64::from(window_size) * 2.0_f64 + 1.0_f64);
                smoothed_data.push([data[i][0], average]);
                trace!("Smoothed data point at index {}: {}", i, average);
            }
        }

        self.plot_data = Some(smoothed_data);
        debug!("Data smoothing complete",);

        Ok(self)
    }

    /// Method to retrieve the mass spectrum for the specified index from the associated mass spectrometry file.
    ///
    /// # Parameters
    /// - `index: usize` - The index of the mass spectrum to be retrieved.
    ///
    /// # Functionality
    /// 1. Logs an informational message about the start of the mass spectrum retrieval process for the specified index.
    /// 2. Matches the `msfile` field, which is a `Result<MsFile, Error>`, and performs the following steps:
    ///     a. Attempts to get the spectrum at the specified index using the `get_spectrum_by_index` method of the `MsFile`.
    ///     b. If a spectrum is found, the function extracts the m/z values and intensities from the spectrum's arrays.
    ///     c. If the extraction of m/z values and intensities is successful, the function stores the data in the `mass_spectrum` field of the current instance of the struct.
    /// 3. If no spectrum is found at the specified index, a warning message is logged.
    /// 4. If there is an error while accessing the `msfile` field or retrieving the spectrum, an error message is logged.
    /// 5. Logs a debug message indicating that the mass spectrum retrieval process is complete.
    ///
    /// # Notes
    /// This function does not return any value. It directly modifies the `mass_spectrum` field of the current instance of the struct.
    pub fn get_mass_spectrum_by_index(&mut self, index: usize) {
        info!("Starting to get mass spectrum at index: {:?}", &index);

        match &mut self.msfile {
            Ok(reader) => {
                if let Some(spec) = reader.get_spectrum_by_index(index) {
                    let arrays = spec.arrays.as_ref();
                    if let Some(arrays) = arrays {
                        let peaks = arrays.mzs().map(|mzs| mzs.to_vec());
                        let intensities = arrays.intensities().map(|ints| ints.to_vec());
                        if peaks.is_ok() && intensities.is_ok() {
                            self.mass_spectrum =
                                Some((peaks.clone().unwrap(), intensities.clone().unwrap()));
                            debug!(
                                "Successfully retrieved mass spectrum at index: {:?} with {} peaks and {} intensities",
                                index,
                                peaks.unwrap().len(),
                                intensities.unwrap().len()
                            );
                        }
                    } else {
                        warn!("No spectrum found at index: {:?}", index);
                    }
                }
            }
            Err(e) => error!("Failed to get mass spectrum at {:?} due to {:?}", &index, e),
        }

        debug!("Finished getting mass spectrum at index: {:?}", &index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    //use approx::assert_relative_eq;
    const TEST_FILE: &str = r"test_file\data_dependent_02.mzML"; //thermo example file converted to mzML (only Rt 10-12min)

    #[test]
    fn test_new() {
        let mzdata = MzData::new();
        assert!(mzdata.retention_time.is_none());
        assert!(mzdata.intensity.is_none());
        assert!(mzdata.mz.is_none());
        assert!(mzdata.msfile.is_err());
        assert!(mzdata.plot_data.is_none());
        assert!(mzdata.mass_spectrum.is_none());
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
        assert!(mzdata.msfile.is_ok());
    }

    #[test]
    fn test_get_xic() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(TEST_FILE);

        // Normalize the path to account for different separators
        let normalized_d = PathBuf::from(d.to_str().unwrap().replace("\\", "/"));

        let mut mzdata = MzData::new();

        mzdata.open_msfile(&normalized_d).unwrap();

        let result = mzdata.get_xic(722.43, ScanPolarity::Positive, 1000.0);
        assert!(result.is_ok());
        assert!(!mzdata.retention_time.is_none());
        assert!(!mzdata.intensity.is_none());
    }
    #[test]
    fn test_get_tic() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(TEST_FILE);

        // Normalize the path to account for different separators
        let normalized_d = PathBuf::from(d.to_str().unwrap().replace("\\", "/"));

        let mut mzdata = MzData::new();

        mzdata.open_msfile(&normalized_d).unwrap();

        let result = mzdata.get_tic(ScanPolarity::Positive);
        assert!(result.is_ok());
        assert!(!mzdata.retention_time.is_none());
        assert!(!mzdata.intensity.is_none());
        assert!(mzdata.mz.is_some());
    }

    #[test]
    fn test_smooth_data() {
        let mut mzdata = MzData::new();
        let data = vec![[1.0, 1.0], [2.0, 2.0], [3.0, 3.0], [4.0, 4.0], [5.0, 5.0]];

        let result = mzdata.smooth_data(Ok(data), 1);
        assert!(result.is_ok());

        let smoothed = mzdata.plot_data.unwrap();
        assert_eq!(smoothed.len(), 5);
        //assert_relative_eq!(smoothed[2][1], 3.0);
    }
}
