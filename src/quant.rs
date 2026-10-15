//! Shared targeted quantification; version 1 preserves legacy peak selection and chord areas.
use crate::confidence::{self, Quality, Tier};
use crate::integration::{self, Integration};
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
    /// Per-analyte integration settings; absent means the method-level classic defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integration: Option<Integration>,
}
impl Analyte {
    /// The settings actually used for this analyte: its own, or the method-level classic ones.
    pub fn integration_for(&self, method: &Method) -> Integration {
        self.integration.clone().unwrap_or_else(|| Integration {
            smoothing: method.detection_smoothing,
            minimum_prominence: method.minimum_height,
            boundary_fraction: method.boundary_fraction,
            ..Integration::default()
        })
    }
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
            mda: None,
            smoothing: 0,
            polarity: "positive".into(),
            ms_level: None,
            precursor_mz: None,
            mz_range: None,
        },
        integration: None,
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
        if let Some(cfg) = &a.integration {
            cfg.validate()
                .map_err(|e| format!("{}: integration: {e}", a.extraction.name))?;
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
    /// How far the automatic integration can be trusted, and why (see [`crate::confidence`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<Quality>,
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
/// Peak between explicit bounds with the legacy straight-line baseline.
pub fn measure(trace: &[[f64; 2]], start: f64, end: f64) -> Result<Peak, String> {
    integration::measure(trace, start, end, &Integration::default())
}
/// Peak between explicit bounds, integrated with the analyte's own baseline setting.
pub fn measure_for(
    trace: &[[f64; 2]],
    start: f64,
    end: f64,
    analyte: &Analyte,
    method: &Method,
) -> Result<Peak, String> {
    integration::measure(trace, start, end, &analyte.integration_for(method))
}
/// Whether a person has looked at the result; independent of the confidence score.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Review {
    NotReviewed,
    /// Accepted by a reviewer as integrated.
    Reviewed,
    /// Boundaries were set by hand.
    Edited,
}
impl Review {
    pub fn label(self) -> &'static str {
        match self {
            Review::NotReviewed => "Not reviewed",
            Review::Reviewed => "Reviewed",
            Review::Edited => "Edited",
        }
    }
}
impl Measurement {
    pub fn review(&self) -> Review {
        match self.status {
            Status::Reviewed => Review::Reviewed,
            Status::Manual => Review::Edited,
            _ => Review::NotReviewed,
        }
    }
    /// Confidence tier of the automatic result; `Ambiguous` results saved before confidence
    /// scores existed count as low.
    pub fn tier(&self) -> Option<Tier> {
        match (&self.quality, &self.status) {
            (Some(q), _) => Some(q.tier),
            (None, Status::Ambiguous) => Some(Tier::Low),
            _ => None,
        }
    }
    /// True when a person or AI reviewer should look at the result before it is relied on.
    pub fn needs_review(&self) -> bool {
        match self.status {
            Status::Missing
            | Status::Failed
            | Status::Cancelled
            | Status::Pending
            | Status::Ambiguous => true,
            Status::Reviewed | Status::Manual => false,
            Status::Automatic => self.tier() == Some(Tier::Low),
        }
    }
    /// A low-confidence automatic peak that no one has reviewed yet.
    pub fn low_confidence_unreviewed(&self) -> bool {
        self.review() == Review::NotReviewed
            && self.peak.is_some()
            && self.tier() == Some(Tier::Low)
    }
}
/// Like [`detect`], also judging how far the automatic result can be trusted.
pub fn detect_assessed(
    trace: &[[f64; 2]],
    analyte: &Analyte,
    method: &Method,
) -> (Option<Peak>, Status, Option<Quality>) {
    let cfg = analyte.integration_for(method);
    let found = integration::candidates(trace, analyte.rt_window, analyte.expected_rt, &cfg);
    let quality = confidence::assess(trace, analyte.rt_window, analyte.expected_rt, &cfg, &found);
    match found.into_iter().next() {
        Some(c) => (Some(c.peak), Status::Automatic, quality),
        None => (None, Status::Missing, None),
    }
}
/// Adds the across-samples checks (retention time and width against the batch) to every
/// result's confidence. Safe to call repeatedly.
pub fn refresh_consistency(results: &mut [Measurement]) {
    let mut qualities: Vec<Option<Quality>> = results.iter().map(|r| r.quality.clone()).collect();
    let peaks: Vec<Option<(String, f64)>> = results
        .iter()
        .map(|r| r.automatic.as_ref().map(|p| (r.analyte.clone(), p.apex_rt)))
        .collect();
    confidence::batch_consistency(&mut qualities, &peaks);
    for (r, q) in results.iter_mut().zip(qualities) {
        r.quality = q;
    }
}
pub fn detect(trace: &[[f64; 2]], analyte: &Analyte, method: &Method) -> (Option<Peak>, Status) {
    integration::detect(
        trace,
        analyte.rt_window,
        analyte.expected_rt,
        &analyte.integration_for(method),
    )
}
