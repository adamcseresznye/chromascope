#![warn(
    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo
)]

use anyhow::anyhow;
use anyhow::Result;
use log::{debug, error, info, trace, warn};
use mzdata::io::mzml::MzMLReaderType;
use mzdata::spectrum::ScanPolarity;
use mzdata::{prelude::*, MzMLReader};
use std::fs::File;
use std::path::PathBuf;

const MS_LEVEL: u8 = 1;

pub struct MzData {
    pub file_name: Option<String>,
    pub index: Option<Vec<usize>>,
    pub retention_time: Option<Vec<f32>>,
    pub intensity: Option<Vec<f32>>,
    pub mz: Option<Vec<f32>>,
    pub msfile: Result<MzMLReaderType<File>>,
    pub plot_data: Option<Vec<[f64; 2]>>,
    pub mass_spectrum: Option<(Vec<f64>, Vec<f32>)>,
}

impl Default for MzData {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MzData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
    pub fn open_msfile(&mut self, path: PathBuf) -> Result<&mut Self> {
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

    pub fn get_xic(
        &mut self,
        mass: f64,
        polarity: ScanPolarity,
        mass_tolerance: f64,
    ) -> Result<&mut Self> {
        info!("Attempting to read XIC of {:?}", &self.file_name);

        self.retention_time = Some(Vec::new());
        self.intensity = Some(Vec::new());
        //self.index = Some(Vec::new()); if the self.index is cleared, when triple clicked one cannot extract the mass spectrum
        self.mz = Some(Vec::new());

        match &mut self.msfile {
            Ok(reader) => {
                for spectrum in reader.iter() {
                    if spectrum.description.ms_level == MS_LEVEL
                        && spectrum.description.polarity == polarity
                    {
                        let centroided = spectrum.clone().into_centroid()?;
                        let sg = centroided
                            .peaks
                            .all_peaks_for(mass, Tolerance::PPM(mass_tolerance));

                        for peak in sg {
                            if let Some(rt) = &mut self.retention_time {
                                rt.push(spectrum.description.acquisition.scans[0].start_time as f32)
                            };
                            if let Some(intensity) = &mut self.intensity {
                                intensity.push(peak.intensity)
                            };
                            if let Some(index) = &mut self.index {
                                index.push(peak.index as usize)
                            };
                        }
                    }
                }

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
                let average = sum / (window_size as f64 * 2.0 + 1.0);
                smoothed_data.push([data[i][0], average]);
                trace!("Smoothed data point at index {}: {}", i, average);
            }
        }

        self.plot_data = Some(smoothed_data);
        debug!(
            "Data smoothing complete. Smoothed {} data points.",
            self.plot_data.as_ref().unwrap().len()
        );

        Ok(self)
    }

    pub fn get_mass_spectrum_by_index(&mut self, index: usize) {
        info!("Starting to get mass spectrum at index: {:?}", &index);

        match &mut self.msfile {
            Ok(reader) => {
                if let Some(spec) = reader.get_spectrum_by_index(index) {
                    let peaks = &spec.arrays.as_ref().unwrap().mzs().unwrap().to_vec();
                    let intensities = &spec
                        .arrays
                        .as_ref()
                        .unwrap()
                        .intensities()
                        .unwrap()
                        .to_vec();
                    self.mass_spectrum = Some((peaks.to_vec(), intensities.to_vec()));

                    debug!(
                        "Successfully retrieved mass spectrum at index: {:?} with {} peaks and {} intensities",
                        index,
                        peaks.len(),
                        intensities.len()
                    );
                } else {
                    warn!("No spectrum found at index: {:?}", index);
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

        let mut mzdata = MzData::new();
        let result = mzdata.open_msfile(d);
        assert!(result.is_ok());
        assert!(mzdata.msfile.is_ok());
    }

    #[test]
    fn test_get_xic() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(TEST_FILE);

        let mut mzdata = MzData::new();

        mzdata.open_msfile(d).unwrap();

        let result = mzdata.get_xic(722.43, ScanPolarity::Positive, 1000.0);
        assert!(result.is_ok());
        assert!(!mzdata.retention_time.is_none());
        assert!(!mzdata.intensity.is_none());
    }
    #[test]
    fn test_get_tic() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(TEST_FILE);

        let mut mzdata = MzData::new();

        mzdata.open_msfile(d).unwrap();

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
