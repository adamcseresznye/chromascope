#![warn(
    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo
)]

use anyhow::anyhow;
use anyhow::Result;
use log::{debug, error, info, warn, trace};
use mzdata::io::mzml::MzMLReaderType;
use mzdata::spectrum::ScanPolarity;
use mzdata::{prelude::*, MzMLReader};
use std::fs::File;
use std::path::PathBuf;

const MS_LEVEL: u8 = 1;

pub struct MzData {
    pub file_name: String,
    pub index: Vec<usize>,
    pub retention_time: Vec<f32>,
    pub intensity: Vec<f32>,
    pub mz: Vec<f32>,
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
            file_name: String::new(),
            index: Vec::new(),
            retention_time: Vec::new(),
            intensity: Vec::new(),
            mz: Vec::new(),
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
                self.file_name = path.display().to_string();
                debug!("Successfully opened MzML file at path: {:?}", &path);
                Ok(self)
            }
            Err(e) => {
                error!(
                    "Failed to open MzML file at path: {:?} with error: {:?}",
                    &path, e
                );
                Err(e.into())
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

                self.retention_time = retention_time;
                self.intensity = intensity;
                self.mz = mz;
                self.index = index;
                trace!("Successfully extracted the BIC of {:?}. Rt is {:?}, Index is {:?}, Mz is {:?}, Intensity is {:?}, ", &self.file_name, &self.retention_time, &self.index, &self.mz, &self.intensity);
            }
            Err(e) => error!("Failed to get BIC due to {:?}", e),
        }
        Ok(self)
    }

    pub fn get_tic(&mut self, polarity: ScanPolarity) -> Result<&mut Self> {
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

                self.retention_time = retention_time;
                self.index = index;
                self.intensity = intensity;
                self.mz = mz;

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

        self.retention_time.clear(); // the rt should be cleared before otherwise the XIC is overlayed on the TIC or BIC
        //self.intensity.clear(); 
        //self.index.clear(); // if the index is cleared, when triple clicked one cannot extract the mass spectrum

        //self.mz.clear();

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

    pub fn get_mass_spectrum_by_index(&mut self, index: usize) {
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
                }
            }

            _ => println!("_ arm variant"),
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

        mzdata.open_msfile(d).unwrap();

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
