//! Shared, validated legacy trace and method schema.
use crate::{
    plotting_parameters::PlotType,
    processing::{AcquisitionMode, ProcessingParams},
};
use mzdata::spectrum::ScanPolarity;
use serde::{Deserialize, Serialize};
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub rows: Option<usize>,
    #[serde(default)]
    pub columns: Option<usize>,
    pub version: u32,
    #[serde(default)]
    pub overlay: bool,
    pub traces: Vec<TraceSpec>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct TraceSpec {
    pub name: String,
    pub acquisition: AcquisitionMode,
    #[serde(default = "xic")]
    pub kind: String,
    pub mass: Option<f64>,
    #[serde(default = "ppm")]
    pub ppm: f64,
    #[serde(default)]
    pub smoothing: u8,
    #[serde(default = "positive")]
    pub polarity: String,
    pub ms_level: Option<u8>,
    pub precursor_mz: Option<f64>,
    pub mz_range: Option<[f64; 2]>,
}
fn xic() -> String {
    "XIC".into()
}
fn ppm() -> f64 {
    10.0
}
fn positive() -> String {
    "positive".into()
}
pub fn parse(
    text: &str,
    bounds: &crate::validation::DataBounds,
) -> Result<(bool, Vec<(String, ProcessingParams)>), String> {
    let p: Preset = toml::from_str(text).map_err(|e| e.to_string())?;
    if p.rows.is_some_and(|v| !(1..=8).contains(&v))
        || p.columns.is_some_and(|v| !(1..=8).contains(&v))
    {
        return Err("Grid rows and columns must be between 1 and 8.".into());
    }
    if p.version != 1 {
        return Err("Unsupported preset version; use version = 1.".into());
    }
    if p.traces.is_empty() || p.traces.len() > 64 {
        return Err("A preset must contain 1–64 traces.".into());
    }
    let mut specs = Vec::new();
    for t in p.traces {
        if t.name.trim().is_empty() {
            return Err("Every trace needs a non-empty name.".into());
        }
        let polarity = match t.polarity.as_str() {
            "positive" => ScanPolarity::Positive,
            "negative" => ScanPolarity::Negative,
            _ => {
                return Err(format!(
                    "{}: polarity must be positive or negative.",
                    t.name
                ))
            }
        };
        let kind = match t.kind.to_uppercase().as_str() {
            "TIC" => PlotType::Tic,
            "BPC" => PlotType::Bpc,
            "XIC" => PlotType::Xic,
            _ => return Err(format!("{}: kind must be TIC, BPC, or XIC.", t.name)),
        };
        bounds
            .validate_smoothing(t.smoothing)
            .map_err(|e| e.to_string())?;
        let ms_level = t
            .ms_level
            .unwrap_or(if t.acquisition == AcquisitionMode::MRM {
                2
            } else {
                1
            });
        if ms_level == 0 {
            return Err("ms_level must be at least 1.".into());
        }
        if t.acquisition == AcquisitionMode::MRM && t.precursor_mz.is_none() {
            return Err(format!(
                "{}: MRM needs precursor_mz; mass is the product ion m/z.",
                t.name
            ));
        }
        if t.precursor_mz.is_some_and(|m| !m.is_finite() || m <= 0.0) {
            return Err("precursor_mz must be finite and positive.".into());
        }
        if t.mz_range
            .is_some_and(|[a, b]| !a.is_finite() || !b.is_finite() || a < 0.0 || a >= b)
        {
            return Err("mz_range must be an increasing pair of positive masses.".into());
        }
        if kind == PlotType::Xic && t.mz_range.is_some() {
            return Err("Use mass and ppm for XIC; mz_range is for TIC/BPC.".into());
        }
        let xic_params = if kind == PlotType::Xic {
            Some(
                crate::validation::XicParams::new(
                    t.mass
                        .ok_or_else(|| format!("{}: XIC needs mass.", t.name))?,
                    polarity,
                    t.ppm,
                    bounds,
                )
                .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let params = ProcessingParams {
            acquisition: Some(t.acquisition),
            plot_type: kind,
            ms_level,
            polarity,
            smoothing: t.smoothing,
            xic_params,
            mz_range: t.mz_range.map(|[a, b]| (a, b)),
            precursor_mz: t.precursor_mz,
        };
        if specs.iter().any(|(_, p)| p == &params) {
            return Err(format!(
                "{} duplicates another trace's extraction settings.",
                t.name
            ));
        }
        specs.push((t.name, params));
    }
    Ok((p.overlay, specs))
}
