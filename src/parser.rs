use anyhow::Result;
use mzdata::prelude::*;
use mzdata::spectrum::ScanPolarity;
use mzdata::spectrum::SignalContinuity;
use mzdata::MZReader;

const MS_LEVEL: u8 = 1;

#[derive(Debug)]
pub struct MzData {
    pub retention_time: Vec<f32>,
    pub intensity: Vec<f32>,
    pub mz: Vec<f32>,
}

impl MzData {
    fn new() -> Self {
        Self {
            retention_time: Vec::new(),
            intensity: Vec::new(),
            mz: Vec::new(),
        }
    }
}

pub fn get_xic(
    path: &str,
    mass: f64,
    polarity: ScanPolarity,
    mass_tolerance: f64,
) -> Result<MzData> {
    let reader = MZReader::open_path(path)?;
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
    let mut reader = MZReader::open_path(path)?;

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
    let mut reader = MZReader::open_path(path)?;

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

    // Create a vector of tuples where each tuple is (retention_time, intensity)
    let mut data: Vec<_> = mzdata
        .retention_time
        .iter()
        .zip(&mzdata.intensity)
        .collect();

    // Sort the data by retention_time
    data.sort_by(|a, b| a.0.partial_cmp(b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Convert the sorted data to the desired format
    let result: Vec<[f64; 2]> = data
        .into_iter()
        .map(|(&rt, &int)| [rt as f64, int as f64])
        .collect();

    Ok(result)
}

pub fn smooth_data(data: Result<Vec<[f64; 2]>>, window_size: usize) -> Result<Vec<[f64; 2]>> {
    let data = data?;
    let mut smoothed_data = Vec::new();
    for i in 0..data.len() {
        if i < window_size || i >= data.len() - window_size {
            // Not enough data to smooth, keep original
            smoothed_data.push(data[i]);
        } else {
            // Calculate the average for the smoothing window
            let sum: f64 = data[i - window_size..=i + window_size]
                .iter()
                .map(|point| point[1])
                .sum();
            let average = sum / (window_size * 2 + 1) as f64;
            smoothed_data.push([data[i][0], average]);
        }
    }
    Ok(smoothed_data)
}
