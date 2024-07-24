use anyhow::{Ok, Result};
use mzdata::io::mzml::MzMLReaderType;
use mzdata::spectrum::{ScanPolarity, SignalContinuity};
use mzdata::{prelude::*, MzMLReader};
use mzdata::io::MZReaderType;
use mzdata::io::mgf::MGFReaderType;
use std::fs::File;

const MS_LEVEL: u8 = 1;


pub enum MZFileReaderEnum {
    MZReader(MZReaderType<File>),
    MGFReader(MGFReaderType<File>),
    MzMLReader(MzMLReaderType<File>),
}


#[derive(Debug, Clone)]
pub struct MzData {
    pub retention_time: Vec<f32>,
    pub intensity: Vec<f32>,
    pub mz: Vec<f32>,
    pub msfile: Result<MZFileReaderEnum>
}

impl MzData {
    fn new() -> Self {
        Self {
            retention_time: Vec::new(),
            intensity: Vec::new(),
            mz: Vec::new(),
            msfile: Ok(MZFileReaderEnum::MZReader(Box<dyn Trait>))
        }
    }
    pub fn open_msfile() {}

    pub fn get_xic(
        &mut self,
        path: &str,
        mass: f64,
        polarity: ScanPolarity,
        mass_tolerance: f64,
    ) -> Result<&mut Self> {
        let reader = MzMLReader::open_path(path)?;

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
    pub fn get_tic(&mut self, path: &str, polarity: ScanPolarity) -> Result<&mut Self> {
        let mut reader = MzMLReader::open_path(path)?;

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

        self.retention_time = retention_time;
        self.intensity = intensity;
        self.mz = mz;
        Ok(self)
    }
    pub fn get_bpic(&mut self, path: &str, polarity: ScanPolarity) -> Result<&mut Self> {
        let mut reader = MzMLReader::open_path(path)?;

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
                |mut acc, (rt, int, mz)| {
                    acc.0.push(rt);
                    acc.1.push(int);
                    acc.2.push(mz);
                    acc
                },
            );

        self.retention_time = retention_time;
        self.intensity = intensity;
        self.mz = mz;
        Ok(self)
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

    pub fn smooth_data(data: Result<Vec<[f64; 2]>>, window_size: u8) -> Result<Vec<[f64; 2]>> {
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
        Ok(smoothed_data)
    }
}

pub fn get_xic(
    path: &str,
    mass: f64,
    polarity: ScanPolarity,
    mass_tolerance: f64,
) -> Result<MzData> {
    let reader = MzMLReader::open_path(path)?;
    let mut mzdata = MzData::new();

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
                mzdata
                    .retention_time
                    .push(spectrum.description.acquisition.scans[0].start_time as f32);
                mzdata.intensity.push(peak.intensity);
            }
        }
    }

    Ok(mzdata)
}
pub fn get_tic(path: &str, polarity: ScanPolarity) -> Result<MzData> {
    let mut reader = MzMLReader::open_path(path)?;

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

    Ok(MzData {
        retention_time,
        intensity,
        mz,
    })
}
pub fn get_bpic(path: &str, polarity: ScanPolarity) -> Result<MzData> {
    let mut reader = MzMLReader::open_path(path)?;

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
            |mut acc, (rt, int, mz)| {
                acc.0.push(rt);
                acc.1.push(int);
                acc.2.push(mz);
                acc
            },
        );

    Ok(MzData {
        retention_time,
        intensity,
        mz,
    })
}

pub fn prepare_for_plot(mzdata: Result<MzData>) -> Result<Vec<[f64; 2]>> {
    let mzdata = mzdata?;
    let mut data = Vec::new();
    let mut temp_rt = 0.0;
    let mut temp_intensity_collector = Vec::new();

    for (idx, &rt) in mzdata.retention_time.iter().enumerate() {
        if rt != temp_rt && !temp_intensity_collector.is_empty() {
            data.push([
                temp_rt as f64,
                temp_intensity_collector.iter().sum::<f64>()
                    / temp_intensity_collector.len() as f64,
            ]);
            temp_intensity_collector.clear();
            temp_rt = rt;
        }
        temp_intensity_collector.push(mzdata.intensity[idx].into());
    }
    //the second if statement after the loop is needed to process the remaining intensities.
    if !temp_intensity_collector.is_empty() {
        data.push([
            temp_rt as f64,
            temp_intensity_collector.iter().sum::<f64>() / temp_intensity_collector.len() as f64,
        ]);
    }

    Ok(data)
}

pub fn smooth_data(data: Result<Vec<[f64; 2]>>, window_size: u8) -> Result<Vec<[f64; 2]>> {
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
    Ok(smoothed_data)
}
