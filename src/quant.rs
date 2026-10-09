//! Shared targeted quantification; version 1 preserves legacy peak selection and chord areas.
use crate::presets::{Preset, TraceSpec};
use crate::processing::{self, ProcessingParams};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Analyte {
    pub expected_rt: f64,
    pub rt_window: [f64; 2],
    pub extraction: TraceSpec,
}

#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Method {
    pub version: u32,
    pub name: String,
    pub detection_smoothing: u8,
    /// Minimum height above the local baseline, in instrument intensity units.
    pub minimum_height: f64,
    /// Fraction of peak height used to locate its boundaries.
    pub boundary_fraction: f64,
    pub analytes: Vec<Analyte>,
}
impl Default for Method {
    fn default() -> Self {
        Self {
            version: 1,
            name: "New quantification method".into(),
            detection_smoothing: 2,
            minimum_height: 0.0,
            boundary_fraction: 0.05,
            analytes: vec![new_analyte(1)],
        }
    }
}
pub fn new_analyte(n: usize) -> Analyte {
    Analyte {
        expected_rt: 5.0,
        rt_window: [4.0, 6.0],
        extraction: TraceSpec {
            name: format!("Analyte {n}"),
            acquisition: processing::AcquisitionMode::FS,
            kind: "XIC".into(),
            mass: Some(100.0),
            ppm: 10.0,
            smoothing: 0,
            polarity: "positive".into(),
            ms_level: None,
            precursor_mz: None,
            mz_range: None,
        },
    }
}
pub fn validate(method: &Method) -> Result<Vec<ProcessingParams>, String> {
    if method.version != 1 || method.name.trim().is_empty() {
        return Err("Use method version 1 and enter a method name.".into());
    }
    if method.detection_smoothing > 10
        || !method.minimum_height.is_finite()
        || method.minimum_height < 0.0
        || !method.boundary_fraction.is_finite()
        || !(0.0..=0.5).contains(&method.boundary_fraction)
    {
        return Err("Detection smoothing must be 0–10, minimum height nonnegative, and boundary fraction 0–0.5.".into());
    }
    let mut names = HashSet::new();
    let mut params = Vec::new();
    if method.analytes.is_empty() || method.analytes.len() > 64 {
        return Err("A method needs 1–64 analytes.".into());
    }
    for a in &method.analytes {
        let [lo, hi] = a.rt_window;
        if !lo.is_finite()
            || !hi.is_finite()
            || lo < 0.0
            || lo >= hi
            || !a.expected_rt.is_finite()
            || !(lo..=hi).contains(&a.expected_rt)
        {
            return Err(format!(
                "{}: expected RT must be inside an increasing, nonnegative RT window (minutes).",
                a.extraction.name
            ));
        }
        if !names.insert(a.extraction.name.trim().to_owned()) {
            return Err("Analyte names must be unique.".into());
        }
        if a.extraction.kind != "XIC" || a.extraction.mz_range.is_some() {
            return Err("Batch quantification methods use XIC extraction; set kind = 'XIC' and omit mz_range.".into());
        }
        if a.extraction.smoothing != 0 {
            return Err("Extraction smoothing must be 0: batch areas use unsmoothed data.".into());
        }
        // Validate each extraction independently: two analytes may share a mass but differ in RT.
        let preset = Preset {
            name: method.name.clone(),
            rows: None,
            columns: None,
            version: 1,
            overlay: false,
            traces: vec![a.extraction.clone()],
        };
        let text = toml::to_string(&preset).map_err(|e| e.to_string())?;
        let (_, specs) =
            crate::presets::parse(&text, &crate::validation::DataBounds::unrestricted())?;
        params.push(specs[0].1.clone());
    }
    Ok(params)
}

#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Peak {
    pub start: f64,
    pub end: f64,
    pub apex_rt: f64,
    pub height: f64,
    pub area: f64,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Status {
    Pending,
    Cancelled,
    Automatic,
    Ambiguous,
    Missing,
    Manual,
    Reviewed,
    Failed,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Measurement {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chromatography: Option<crate::chromatography::Analysis>,
    pub sample: String,
    pub source: String,
    pub run: String,
    pub analyte: String,
    pub params: ProcessingParams,
    pub method: String,
    pub trace: Vec<[f64; 2]>,
    pub automatic: Option<Peak>,
    pub peak: Option<Peak>,
    pub automatic_status: Status,
    pub status: Status,
    pub diagnostic: String,
}
pub fn measure(trace: &[[f64; 2]], start: f64, end: f64) -> Result<Peak, String> {
    if trace.len() < 3
        || !start.is_finite()
        || !end.is_finite()
        || start >= end
        || start < trace[0][0]
        || end > trace[trace.len() - 1][0]
    {
        return Err("Integration bounds must be increasing and inside the measured trace.".into());
    }
    let apex = trace
        .iter()
        .filter(|p| p[0] >= start && p[0] <= end)
        .max_by(|a, b| a[1].total_cmp(&b[1]))
        .ok_or("The interval contains no scans.")?;
    let area = processing::integrate_peak(trace, start, end).map_err(|e| e.to_string())?;
    Ok(Peak {
        start,
        end,
        apex_rt: apex[0],
        height: apex[1],
        area,
    })
}
pub fn detect(trace: &[[f64; 2]], analyte: &Analyte, method: &Method) -> (Option<Peak>, Status) {
    let window: Vec<_> = trace
        .iter()
        .copied()
        .filter(|p| p[0] >= analyte.rt_window[0] && p[0] <= analyte.rt_window[1])
        .collect();
    if window.len() < 3 {
        return (None, Status::Missing);
    }
    let smooth = processing::smooth_chromatogram(window.clone(), method.detection_smoothing)
        .unwrap_or_else(|_| window.clone());
    let floor = smooth.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let mut candidates = Vec::new();
    for i in 1..smooth.len() - 1 {
        if smooth[i][1] <= smooth[i - 1][1] || smooth[i][1] < smooth[i + 1][1] {
            continue;
        }
        // Reject small ripples on an elevated baseline using local prominence.
        let mut valley_left = i;
        let mut valley_right = i;
        while valley_left > 0 && smooth[valley_left - 1][1] <= smooth[valley_left][1] {
            valley_left -= 1;
        }
        while valley_right + 1 < smooth.len()
            && smooth[valley_right + 1][1] <= smooth[valley_right][1]
        {
            valley_right += 1;
        }
        let prominence = smooth[i][1] - smooth[valley_left][1].max(smooth[valley_right][1]);
        if prominence <= method.minimum_height {
            continue;
        }
        let left_threshold = smooth[valley_left][1]
            + (smooth[i][1] - smooth[valley_left][1]) * method.boundary_fraction;
        let right_threshold = smooth[valley_right][1]
            + (smooth[i][1] - smooth[valley_right][1]) * method.boundary_fraction;
        let clip_threshold = floor + (smooth[i][1] - floor) * method.boundary_fraction;
        let mut left = i;
        let mut right = i;
        while left > valley_left && smooth[left][1] > left_threshold {
            left -= 1;
        }
        while right < valley_right && smooth[right][1] > right_threshold {
            right += 1;
        }
        if let Ok(peak) = measure(trace, smooth[left][0], smooth[right][0]) {
            if peak.area > 0.0 {
                candidates.push((
                    i,
                    peak,
                    (left == 0 && smooth[left][1] > clip_threshold)
                        || (right == smooth.len() - 1 && smooth[right][1] > clip_threshold),
                ));
            }
        }
    }
    candidates.sort_by(|a, b| {
        (smooth[a.0][0] - analyte.expected_rt)
            .abs()
            .total_cmp(&(smooth[b.0][0] - analyte.expected_rt).abs())
    });
    if let Some((_, peak, clipped)) = candidates.first() {
        (
            Some(peak.clone()),
            if candidates.len() > 1 || *clipped {
                Status::Ambiguous
            } else {
                Status::Automatic
            },
        )
    } else {
        (None, Status::Missing)
    }
}
