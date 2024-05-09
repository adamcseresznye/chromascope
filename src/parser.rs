use anyhow::Result;
use mzdata::prelude::*;
use mzdata::spectrum::ScanPolarity;
use mzdata::spectrum::SignalContinuity;
use mzdata::MZReader;

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

    pub fn mzdata_is_empty(&self) -> bool {
        match (self.intensity.is_empty(), self.retention_time.is_empty()) {
            (true, true) => true,
            _ => false,
        }
    }
}

pub fn get_xic(
    path: &str,
    mass: f64,
    polarity: ScanPolarity,
    mass_tolerance: f64,
    ms_level: u8,
) -> Result<MzData> {
    let reader = MZReader::open_path(path)?;
    let mut mzdata = MzData::new();

    for spectrum in reader {
        if spectrum.description.ms_level == ms_level
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
    Ok(mzdata
        .retention_time
        .iter()
        .zip(&mzdata.intensity)
        .map(|(&rt, &int)| [rt as f64, int as f64])
        .collect())
}
