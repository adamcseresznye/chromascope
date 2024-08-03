#![deny(clippy::all)]

use anyhow::anyhow;
use anyhow::{Ok, Result};
use mzdata::io::mgf::MGFReaderType;
use mzdata::io::mzml::MzMLReaderType;
use mzdata::io::MZReaderType;
use mzdata::spectrum::ArrayType;
use mzdata::spectrum::{self, RefPeakDataLevel};
use mzdata::spectrum::{ScanPolarity, SignalContinuity};
use mzdata::{prelude::*, MzMLReader};
use std::fmt::Debug;
use std::fs::File;

const MS_LEVEL: u8 = 1;

type MassSpectrum = (Vec<f64>, Vec<f32>);

#[derive(Debug)]
pub struct SingleSpectrum {
    pub rt: f64,
    pub index: usize,
}

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
    pub list_of_spectra: Vec<SingleSpectrum>,
    pub index: Vec<usize>,
    pub min_max_rt: Option<(f32, f32)>,
    pub min_max_index: Option<(usize, usize)>,
    pub retention_time: Vec<f32>,
    pub intensity: Vec<f32>,
    pub mz: Vec<f32>,
    pub msfile: Result<MZFileReaderEnum>,
    pub plot_data: Option<Vec<[f64; 2]>>,
    pub mass_spectrum: Option<(Vec<f64>, Vec<f32>)>,
}
impl Default for MzData {
    fn default() -> Self {
        Self::new()
    }
}
impl MzData {
    pub fn new() -> Self {
        Self {
            list_of_spectra: Vec::new(),
            index: Vec::new(),
            min_max_rt: None,
            min_max_index: None,
            retention_time: Vec::new(),
            intensity: Vec::new(),
            mz: Vec::new(),
            msfile: Err(anyhow!("File not opened")),
            plot_data: Some(Vec::new()),
            mass_spectrum: Some((Vec::new(), Vec::new())),
        }
    }
    pub fn open_msfile(&mut self, path: &str) -> Result<&mut Self> {
        let reader = MzMLReader::open_path(path)?;
        self.msfile = Ok(MZFileReaderEnum::MzMLReader(DebugMzMLReaderType(reader)));
        println!("Index {:?}", self.msfile);

        match &mut self.msfile {
            std::result::Result::Ok(MZFileReaderEnum::MzMLReader(DebugMzMLReaderType(reader))) => {
                let min_index = reader.iter().nth(0).map(|x| x.index()).unwrap_or(0);
                let max_index = reader.iter().last().map(|x| x.index()).unwrap_or(0);
                self.min_max_index = Some((min_index, max_index));

                println!("{:?}", self.min_max_index)
            }

            _ => println!("_ arm variant"),
        }

        Ok(self)
    }

    pub fn get_bpic(&mut self, polarity: ScanPolarity) -> Result<&mut Self> {
        match &mut self.msfile {
            Result::Ok(MZFileReaderEnum::MzMLReader(DebugMzMLReaderType(reader))) => {
                let (retention_time, intensity, mz, index) = reader
                    .iter()
                    .filter(|spectrum| spectrum.description.polarity == polarity)
                    .map(|spectrum| {
                        let retention_time = spectrum.start_time() as f32;
                        let intensity = spectrum.peaks().base_peak().intensity;
                        let mz = spectrum.peaks().base_peak().mz as f32;
                        let index = spectrum.index() as usize;
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

                self.retention_time = retention_time;
                self.intensity = intensity;
                self.mz = mz;
                self.min_max_rt = Some((
                    self.retention_time.iter().nth(0).copied().unwrap_or(0.0),
                    self.retention_time.iter().last().copied().unwrap_or(0.0),
                ));
                self.index = index;
                Ok(self)
            }
            _ => Err(anyhow!(
                "Expected MzMLReader, but found something else or an error"
            )),
        }
    }

    pub fn get_tic(&mut self, polarity: ScanPolarity) -> Result<&mut Self> {
        match &mut self.msfile {
            Result::Ok(MZFileReaderEnum::MzMLReader(DebugMzMLReaderType(reader))) => {
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

                self.retention_time = retention_time;
                self.index = index;
                self.intensity = intensity;
                self.mz = mz;
                self.min_max_rt = Some((
                    self.retention_time.iter().nth(0).copied().unwrap_or(0.0),
                    self.retention_time.iter().last().copied().unwrap_or(0.0),
                ));

                Ok(self)
            }
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
        println!("Starting XIC extraction");
        println!(
            "Mass: {}, Polarity: {:?}, Mass Tolerance: {}",
            mass, polarity, mass_tolerance
        );

        self.retention_time.clear();
        self.intensity.clear();
        //self.mz.clear();
        self.index.clear();

        match &mut self.msfile {
            std::result::Result::Ok(MZFileReaderEnum::MzMLReader(DebugMzMLReaderType(reader))) => {
                for spectrum in reader.iter() {
                    if spectrum.description.ms_level == 1
                        && spectrum.description.polarity == polarity
                    {
                        let rt = spectrum.start_time() as f32;

                        let centroided = spectrum.clone().into_centroid()?;
                        let sg = centroided
                            .peaks
                            .all_peaks_for(mass, Tolerance::PPM(mass_tolerance));

                        for peak in sg {
                            self.retention_time
                                .push(spectrum.description.acquisition.scans[0].start_time as f32);
                            self.intensity.push(peak.intensity);
                            self.index.push(peak.index as usize)
                        }
                    }
                }

                println!("Indexes: {:?}", self.index);
                println!("Rts: {:?}", self.retention_time);
                //println!("XIC points extracted: {}", self.retention_time.len());

                if self.retention_time.is_empty() {
                    println!("No matching peaks found");
                    return Err(anyhow!(
                        "No matching peaks found for the given mass and tolerance"
                    ));
                }

                self.min_max_rt = Some((
                    *self.retention_time.first().unwrap(),
                    *self.retention_time.last().unwrap(),
                ));

                Ok(self)
            }
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
        //the second if statement after the loop is needed to process the remaining intensities.
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

    pub fn get_mass_spectrum_by_time(&mut self, rt: f32) -> Result<&mut Self> {
        let solution: MassSpectrum = match &mut self.msfile {
            std::result::Result::Ok(MZFileReaderEnum::MzMLReader(DebugMzMLReaderType(reader))) => {
                if let Some(spec) = reader.get_spectrum_by_time(rt.into()) {
                    let peaks = spec.peaks();

                    match peaks {
                        RefPeakDataLevel::RawData(raw_data) => {
                            let peaks = raw_data.mzs()?.to_vec();
                            let intensities = raw_data.intensities()?.to_vec();
                            (peaks, intensities)
                        }
                        RefPeakDataLevel::Centroid(centroid_peaks) => {
                            let peaks = centroid_peaks.iter().map(|x| x.mz).collect::<Vec<_>>();
                            let intensities = centroid_peaks
                                .iter()
                                .map(|x| x.intensity)
                                .collect::<Vec<_>>();
                            (peaks, intensities)
                        }
                        RefPeakDataLevel::Deconvoluted(deconv_peaks) => {
                            let peaks = deconv_peaks.iter().map(|x| x.mz()).collect::<Vec<_>>();
                            let intensities =
                                deconv_peaks.iter().map(|x| x.intensity).collect::<Vec<_>>();
                            (peaks, intensities)
                        }
                        RefPeakDataLevel::Missing => {
                            println!("Spectrum not found at the specified time");
                            (vec![], vec![])
                        }
                    }
                } else {
                    return Err(anyhow!("Spectrum not found at the specified time"));
                }
            }
            _ => {
                return Err(anyhow!(
                    "Expected MzMLReader, but found something else or an error"
                ))
            }
        };

        self.mass_spectrum = Some(solution);
        Ok(self)
    }
    pub fn get_mass_spectrum_by_index(&mut self, index: usize) {
        // Invalid Index: If the index provided is out of range or does not correspond to any spectrum in the reader, get_spectrum_by_index might return None.
        // Empty Data: If the reader has no spectra loaded or available, any index might result in None.
        // Corrupted Data: If the data being read is corrupted or improperly formatted, the reader might fail to retrieve a spectrum

        match &mut self.msfile {
            std::result::Result::Ok(MZFileReaderEnum::MzMLReader(DebugMzMLReaderType(reader))) => {
                if let Some(spec) = reader.get_spectrum_by_index(index.into()) {
                    let peaks = &spec.arrays.as_ref().unwrap().mzs().unwrap().to_vec();
                    let intensities = &spec
                        .arrays
                        .as_ref()
                        .unwrap()
                        .intensities()
                        .unwrap()
                        .to_vec();
                    self.mass_spectrum = Some((peaks.to_vec(), intensities.to_vec()));
                }
            }
            std::result::Result::Ok(MZFileReaderEnum::MZReader(DebugMZReaderType(reader))) => {
                if let Some(spec) = reader.get_spectrum_by_index(index.into()) {
                    println!("MZReader Index nr: {:?} The masses are {:?}", index, spec);
                } else {
                    println!("This is the else arm of the MZReader")
                }
            }
            _ => println!("_ arm variant"),
        }
    }
}

/*
fn main() -> Result<()> {
    let file_path = r"C:\Users\s0212777\OneDrive - Universiteit Antwerpen\rust_projects\mammamia\test_file\data_dependent_02.mzML";
    let mut mzdata = MzData::default();

    mzdata.open_msfile(file_path)?;

    mzdata.get_xic(722.43, ScanPolarity::Positive, 0.05)?;

    let plot_ready = mzdata.prepare_for_plot();
    mzdata.smooth_data(plot_ready, 3)?;
    mzdata.get_mass_spectrum(10.92)?;
    println!("{:?}", mzdata.mass_spectrum);
    Ok(())
}

fn main() -> std::io::Result<()> {
    let file_path = r"C:\Users\s0212777\OneDrive - Universiteit Antwerpen\rust_projects\mz_viewer\data\test_BOTH.mzML";
    let mut reader = mzdata::MZReader::open_path(file_path)?;
    let mut solution: (Vec<f64>, Vec<f32>) = (vec![], vec![]);

    if let Some(spec) = reader.get_spectrum_by_time(56.05) {
        let peaks = spec.peaks();

        solution = match peaks {
            RefPeakDataLevel::RawData(raw_data) => {
                let peaks = raw_data.mzs()?.to_vec();
                let intensities = raw_data.intensities()?.to_vec();
                (peaks, intensities)
            }
            RefPeakDataLevel::Centroid(centroid_peaks) => {
                let peaks = centroid_peaks.iter().map(|x| x.mz).collect::<Vec<_>>();
                let intensities = centroid_peaks
                    .iter()
                    .map(|x| x.intensity)
                    .collect::<Vec<_>>();
                (peaks, intensities)
            }
            RefPeakDataLevel::Deconvoluted(deconv_peaks) => {
                let peaks = deconv_peaks.iter().map(|x| x.mz()).collect::<Vec<_>>();
                let intensities = deconv_peaks.iter().map(|x| x.intensity).collect::<Vec<_>>();
                (peaks, intensities)
            }
            RefPeakDataLevel::Missing => {
                println!("Spectrum not found at the specified time");
                (vec![], vec![])
            }
        };
    }
    println!("MZ: {:?}", solution.0);

    println!("INTENSITY: {:?}", solution.1);

    Ok(())
}
*/

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
