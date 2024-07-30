#![deny(clippy::all)]

use mzdata::io::mgf::MGFReaderType;
use mzdata::io::mzml::MzMLReaderType;
use mzdata::io::MZReaderType;
use mzdata::spectrum::RefPeakDataLevel;
use mzdata::spectrum::{ScanPolarity, SignalContinuity};
use mzdata::{prelude::*, MzMLReader};
use std::fmt::Debug;
use std::fs::File;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use anyhow::{anyhow, Result};

const MS_LEVEL: u8 = 1;

type MassSpectrum = (Vec<f64>, Vec<f32>);

// Wrapper structs with public visibility
pub struct DebugMZReaderType(pub MZReaderType<File>);
pub struct DebugMGFReaderType(pub MGFReaderType<File>);
pub struct DebugMzMLReaderType(pub MzMLReaderType<File>);

impl Debug for DebugMZReaderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DebugMZReaderType")
    }
}

impl Debug for DebugMGFReaderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DebugMGFReaderType")
    }
}

impl Debug for DebugMzMLReaderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DebugMzMLReaderType")
    }
}

#[derive(Debug)]
pub enum MZFileReaderEnum {
    MZReader(DebugMZReaderType),
    MGFReader(DebugMGFReaderType),
    MzMLReader(DebugMzMLReaderType),
}

#[derive(Debug)]
pub struct MzData {
    pub retention_time: Vec<f32>,
    pub intensity: Vec<f32>,
    pub mz: Vec<f32>,
    pub msfile: Arc<Mutex<Result<MZFileReaderEnum, anyhow::Error>>>,
    pub plot_data: Option<Vec<[f64; 2]>>,
    pub mass_spectrum: Option<(Vec<f64>, Vec<f32>)>,
}

impl Default for MzData {
    fn default() -> Self {
        Self::new()
    }
}

// Manually implement Clone for MzData
impl Clone for MzData {
    fn clone(&self) -> Self {
        MzData {
            retention_time: self.retention_time.clone(),
            intensity: self.intensity.clone(),
            mz: self.mz.clone(),
            msfile: Arc::new(Mutex::new(Err(anyhow::Error::msg("Clone not supported for msfile")))), // Use Arc and Mutex// or handle it as needed
            plot_data: self.plot_data.clone(),
            mass_spectrum: self.mass_spectrum.clone(),
        }
    }
}

impl MzData {
    pub fn new() -> Self {
        Self {
            retention_time: Vec::new(),
            intensity: Vec::new(),
            mz: Vec::new(),
            msfile: Arc::new(Mutex::new(Err(anyhow::Error::msg("File not opened")))),
            plot_data: Some(Vec::new()),
            mass_spectrum: Some((Vec::new(), Vec::new())),
        }
    }

    pub fn open_msfile(&mut self, path: &str) -> Result<&mut Self> {
        let reader = MzMLReader::open_path(path)?;
        {
            let mut msfile_lock = self.msfile.lock().map_err(|_| anyhow!("Failed to lock msfile"))?;
            *msfile_lock = Ok(MZFileReaderEnum::MzMLReader(DebugMzMLReaderType(reader)));
        }
        Ok(self)
    }

    pub fn get_bpic(&mut self, polarity: ScanPolarity) -> Result<&mut Self> {
        // Lock `msfile` to access its contents
        let msfile_lock = self.msfile.lock().map_err(|_| anyhow!("Failed to lock msfile"))?;
        
        // Access the locked data
        match &*msfile_lock {
            Ok(MZFileReaderEnum::MzMLReader(DebugMzMLReaderType(reader))) => {
                // Gather data from reader
                let (retention_time, intensity, mz) = reader
                    .iter()
                    .filter(|spectrum| spectrum.description.polarity == polarity)
                    .map(|spectrum| {
                        let retention_time = spectrum.start_time() as f32;
                        let intensity = spectrum.peaks().base_peak().intensity;
                        let mz = spectrum.peaks().base_peak().mz as f32;
                        (retention_time, intensity, mz)
                    })
                    .fold(
                        (Vec::new(), Vec::new(), Vec::new()),
                        |(mut rt_acc, mut int_acc, mut mz_acc), (rt, int, mz)| {
                            rt_acc.push(rt);
                            int_acc.push(int);
                            mz_acc.push(mz);
                            (rt_acc, int_acc, mz_acc)
                        },
                    );
    
                // Release the lock before mutating `self`
                drop(msfile_lock);
    
                // Mutate `self` after releasing the lock
                self.retention_time = retention_time;
                self.intensity = intensity;
                self.mz = mz;
                
                Ok(self)
            }
            Err(e) => Err(anyhow!("Error accessing msfile: {:?}", e)),
            _ => Err(anyhow!(
                "Expected MzMLReader, but found something else or an error"
            )),
        }
    }
    
    

    pub fn get_tic(&mut self, polarity: ScanPolarity) -> Result<&mut Self> {
        // Lock `msfile` to access its contents
        let msfile_lock = self.msfile.lock().map_err(|_| anyhow!("Failed to lock msfile"))?;
        
        // Access the locked data
        match &*msfile_lock {
            Ok(MZFileReaderEnum::MzMLReader(DebugMzMLReaderType(reader))) => {
                // Process the data from the reader
                let (retention_time, intensity): (Vec<_>, Vec<_>) = reader
                    .iter()
                    .filter(|spectrum| spectrum.description.polarity == polarity)
                    .map(|spectrum| {
                        let retention_time = spectrum.start_time() as f32;
                        let total_intensity = spectrum.peaks().tic();
                        (retention_time, total_intensity)
                    })
                    .unzip();
                let mz: Vec<f32> = Vec::new();

                // Release the lock before mutating `self`
                drop(msfile_lock);

                // Mutate `self` after releasing the lock
                self.retention_time = retention_time;
                self.intensity = intensity;
                self.mz = mz;
                
                Ok(self)
            }
            Err(e) => Err(anyhow!("Error accessing msfile: {:?}", e)),
            _ => Err(anyhow!(
                "Expected MzMLReader, but found something else or an error"
            )),
        }
    }

    pub fn get_xic(
        &mut self,
        mass: f64,
        polarity: ScanPolarity,
        mass_tolerance: f64,
    ) -> Result<&mut Self> {
        // Lock `msfile` to access its contents
        let msfile_lock = self.msfile.lock().map_err(|_| anyhow!("Failed to lock msfile"))?;
        
        match &mut *msfile_lock {
            Ok(MZFileReaderEnum::MzMLReader(DebugMzMLReaderType(reader))) => {
                for spectrum in reader {
                    if spectrum.description.ms_level == MS_LEVEL
                        && spectrum.description.polarity == polarity
                        && spectrum.signal_continuity() == SignalContinuity::Centroid
                    {
                        let peak_picked = spectrum.clone().into_centroid()?;
                        let matching_peaks = peak_picked
                            .peaks
                            .all_peaks_for(mass, Tolerance::Da(mass_tolerance));

                        for peak in matching_peaks {
                            self.retention_time
                                .push(spectrum.description.acquisition.scans[0].start_time as f32);
                            self.intensity.push(peak.intensity);
                        }
                    }
                }
                Ok(self)
            }
            Err(e) => Err(anyhow!("Error accessing msfile: {:?}", e)),
            _ => Err(anyhow!(
                "Expected MzMLReader, but found something else or an error"
            )),
        }
    }

    pub fn prepare_for_plot(&self) -> Result<Vec<[f64; 2]>> {
        let mut data = Vec::new();
        let mut temp_rt = 0.0;
        let mut temp_intensity_collector = Vec::new();

        for (idx, &rt) in self.retention_time.iter().enumerate() {
            if rt != temp_rt && !temp_intensity_collector.is_empty() {
                data.push([
                    temp_rt as f64,
                    temp_intensity_collector.iter().sum::<f64>()
                        / temp_intensity_collector.len() as f64,
                ]);
                temp_intensity_collector.clear();
                temp_rt = rt;
            }
            temp_intensity_collector.push(self.intensity[idx].into());
        }
        // The second if statement after the loop is needed to process the remaining intensities.
        if !temp_intensity_collector.is_empty() {
            data.push([
                temp_rt as f64,
                temp_intensity_collector.iter().sum::<f64>()
                    / temp_intensity_collector.len() as f64,
            ]);
        }

        Ok(data)
    }

    pub fn smooth_data(
        &mut self,
        data: Result<Vec<[f64; 2]>>,
        window_size: u8,
    ) -> Result<&mut Self> {
        let data = data?;
        let mut smoothed_data = Vec::new();
        let window_size_usize = window_size as usize;
        for i in 0..data.len() {
            if i < window_size_usize || i >= data.len() - window_size_usize {
                // Not enough data to smooth, keep original
                smoothed_data.push(data[i]);
            } else {
                // Calculate the average for the smoothing window
                let sum: f64 = data[i - window_size_usize..=i + window_size_usize]
                    .iter()
                    .map(|point| point[1])
                    .sum();
                let average = sum / (window_size as f64 * 2.0 + 1.0);
                smoothed_data.push([data[i][0], average]);
            }
        }
        self.plot_data = Some(smoothed_data);
        Ok(self)
    }

    pub fn get_mass_spectrum(&mut self, rt: f32) -> Result<&mut Self> {
        println!("Entering get_mass_spectrum with rt: {}", rt);

        let start_time = Instant::now();
        let timeout = Duration::from_secs(5); // 5 second timeout

        // Lock `msfile` to access its contents
        let msfile_lock = self.msfile.lock().map_err(|_| anyhow!("Failed to lock msfile"))?;
        
        match &mut *msfile_lock {
            Ok(MZFileReaderEnum::MzMLReader(DebugMzMLReaderType(reader))) => {
                println!("Attempting to get spectrum by time");

                while start_time.elapsed() < timeout {
                    match reader.get_spectrum_by_time(rt.into()) {
                        Some(spec) => {
                            println!("Got spectrum, extracting peaks");
                            let peaks = spec.peaks();

                            let extracted_ms_spectrum = match peaks {
                                RefPeakDataLevel::RawData(raw_data) => {
                                    println!("Processing RawData");
                                    let peaks = raw_data.mzs()?;
                                    let intensities = raw_data.intensities()?;
                                    println!(
                                        "RawData processed. Peaks: {}, Intensities: {}",
                                        peaks.len(),
                                        intensities.len()
                                    );
                                    (peaks.to_vec(), intensities.to_vec())
                                }
                                RefPeakDataLevel::Centroid(centroid_peaks) => {
                                    println!("Processing Centroid data");
                                    let peaks =
                                        centroid_peaks.iter().map(|x| x.mz).collect::<Vec<_>>();
                                    let intensities = centroid_peaks
                                        .iter()
                                        .map(|x| x.intensity)
                                        .collect::<Vec<_>>();
                                    println!("Centroid data processed. Peaks: {}", peaks.len());
                                    (peaks, intensities)
                                }
                                RefPeakDataLevel::Deconvoluted(deconv_peaks) => {
                                    println!("Processing Deconvoluted data");
                                    let peaks =
                                        deconv_peaks.iter().map(|x| x.mz()).collect::<Vec<_>>();
                                    let intensities = deconv_peaks
                                        .iter()
                                        .map(|x| x.intensity)
                                        .collect::<Vec<_>>();
                                    println!("Deconvoluted data processed. Peaks: {}", peaks.len());
                                    (peaks, intensities)
                                }
                                RefPeakDataLevel::Missing => {
                                    println!("Spectrum not found at the specified time (Missing)");
                                    (vec![], vec![])
                                }
                            };

                            println!(
                                "Extracted spectrum with {} data points",
                                extracted_ms_spectrum.0.len()
                            );
                            self.mass_spectrum = Some(extracted_ms_spectrum);
                            return Ok(self);
                        }
                        None => {
                            println!("Spectrum not found, retrying...");
                            std::thread::sleep(Duration::from_millis(100));
                        }
                    }
                }

                println!("Timeout occurred while getting spectrum by time");
                Err(anyhow!("Timeout occurred while getting spectrum by time"))
            }
            Err(e) => Err(anyhow!("Error accessing msfile: {:?}", e)),
            _ => Err(anyhow!(
                "Expected MzMLReader, but found something else or an error"
            )),
        }
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
        assert!(mzdata.retention_time.is_empty());
        assert!(mzdata.intensity.is_empty());
        assert!(mzdata.mz.is_empty());
        assert!(mzdata.msfile.is_err());
        assert!(mzdata.plot_data.is_some());
        assert!(mzdata.mass_spectrum.is_some());
    }

    #[test]
    fn test_open_msfile() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(TEST_FILE);

        let mut mzdata = MzData::new();
        let result = mzdata.open_msfile(d.display().to_string().as_str());
        assert!(result.is_ok());
        assert!(mzdata.msfile.is_ok());
    }

    #[test]
    fn test_get_xic() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(TEST_FILE);

        let mut mzdata = MzData::new();

        mzdata
            .open_msfile(d.display().to_string().as_str())
            .unwrap();

        let result = mzdata.get_xic(722.43, ScanPolarity::Positive, 0.05);
        assert!(result.is_ok());
        assert!(!mzdata.retention_time.is_empty());
        assert!(!mzdata.intensity.is_empty());
    }

    #[test]
    fn test_get_tic() {
        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push(TEST_FILE);

        let mut mzdata = MzData::new();

        mzdata
            .open_msfile(d.display().to_string().as_str())
            .unwrap();

        let result = mzdata.get_tic(ScanPolarity::Positive);
        assert!(result.is_ok());
        assert!(!mzdata.retention_time.is_empty());
        assert!(!mzdata.intensity.is_empty());
        assert!(mzdata.mz.is_empty());
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
