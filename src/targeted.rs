//! Targeted small-molecule quantification. Areas remain immutable evidence; no extrapolation.
use crate::domain::{EngineError, Result};
use nalgebra::{DMatrix, DVector};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

fn invalid(message: impl ToString) -> EngineError {
    EngineError::new("invalid_parameters", message)
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    NgMl,
    UgMl,
    MgL,
    NmolL,
    UmolL,
}
impl Unit {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NgMl => "ng/mL",
            Self::UgMl => "ug/mL",
            Self::MgL => "mg/L",
            Self::NmolL => "nmol/L",
            Self::UmolL => "umol/L",
        }
    }
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Standard,
    Blank,
    Qc,
    Unknown,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Weighting {
    Unweighted,
    InverseX,
    InverseX2,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Intercept {
    Free,
    Zero,
    Fixed(f64),
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CalibrationConfig {
    pub degree: usize,
    pub intercept: Intercept,
    pub weighting: Weighting,
    pub unit: Unit,
    /// Validated concentration limits in the stated unit, before dilution.
    pub range: [f64; 2],
    pub lod: f64,
    pub loq: f64,
    pub accuracy_tolerance_percent: f64,
    pub qc_cv_limit_percent: f64,
    pub blank_response_limit: f64,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ion {
    pub extraction: crate::quant::Analyte,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Qualifier {
    pub ion: Ion,
    pub ratio_range: [f64; 2],
    pub rt_tolerance_minutes: f64,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub id: String,
    pub quantifier: Ion,
    pub qualifiers: Vec<Qualifier>,
    pub internal_standard: Option<String>,
    /// Area bounds for this target when used as an internal standard.
    pub is_area_range: Option<[f64; 2]>,
    pub calibration: Option<CalibrationConfig>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub id: String,
    pub source: String,
    pub role: Role,
    pub injection_order: u32,
    pub dilution: f64,
    /// Nominal concentration in each target's stated unit, in the injected solution.
    pub nominal: BTreeMap<String, f64>,
    /// Explicit calibration exclusions with reasons, never inferred from QC flags.
    pub exclusions: BTreeMap<String, String>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchRequest {
    pub version: u32,
    pub name: String,
    pub targets: Vec<Target>,
    pub samples: Vec<Sample>,
    pub detection_smoothing: u8,
    pub minimum_height: f64,
    pub boundary_fraction: f64,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Present,
    Missing,
    Rejected,
    BelowDetection,
    BelowLoq,
    BelowRange,
    AboveRange,
    Failed,
    Ambiguous,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PointDiagnostic {
    pub sample: String,
    pub x: f64,
    pub response: f64,
    pub weight: f64,
    pub fitted: f64,
    pub residual: f64,
    pub back_calculated: Option<f64>,
    pub accuracy_percent: Option<f64>,
    pub flags: Vec<String>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Calibration {
    pub range: [f64; 2],
    pub unit: Unit,
    pub coefficients: Vec<f64>,
    pub scale: f64,
    pub condition_number: f64,
    pub residual_degrees_of_freedom: usize,
    pub r_squared: Option<f64>,
    pub weighted_rmse: f64,
    pub residual_standard_error: Option<f64>,
    pub points: Vec<PointDiagnostic>,
    pub flags: Vec<String>,
}
impl Calibration {
    pub fn response(&self, x: f64) -> f64 {
        self.coefficients
            .iter()
            .rev()
            .fold(0.0, |v, c| v * (x / self.scale) + c)
    }
    /// All distinct roots within the validated domain. Never chooses among multiple roots.
    pub fn inverse(&self, y: f64, range: [f64; 2]) -> Result<f64> {
        if !y.is_finite()
            || self.coefficients.len() < 2
            || self.coefficients.len() > 4
            || self.coefficients.iter().any(|v| !v.is_finite())
            || !self.scale.is_finite()
            || self.scale <= 0.0
            || range.iter().any(|v| !v.is_finite() || *v < 0.0)
            || range[0] >= range[1]
            || range[0] < self.range[0]
            || range[1] > self.range[1]
        {
            return Err(invalid("Nonfinite response"));
        }
        let c = &self.coefficients;
        let mut edges = vec![range[0], range[1]];
        if c.len() == 3 && c[2] != 0.0 {
            edges.push(-c[1] / (2.0 * c[2]) * self.scale);
        }
        if c.len() == 4 {
            let (a, b, d) = (3.0 * c[3], 2.0 * c[2], c[1]);
            if a == 0.0 {
                if b != 0.0 {
                    edges.push(-d / b * self.scale);
                }
            } else {
                let disc = b * b - 4.0 * a * d;
                if disc >= 0.0 {
                    edges.push((-b - disc.sqrt()) / (2.0 * a) * self.scale);
                    edges.push((-b + disc.sqrt()) / (2.0 * a) * self.scale);
                }
            }
        }
        edges.retain(|x| x.is_finite() && *x >= range[0] && *x <= range[1]);
        edges.sort_by(f64::total_cmp);
        edges.dedup();
        let mut roots = Vec::new();
        for &x in &edges {
            let residual = self.response(x) - y;
            // Only machine roundoff at the closed domain boundary is tolerated.
            // Callers retain a boundary_roundoff flag whenever this is used.
            if residual.abs()
                <= 64.0 * f64::EPSILON * y.abs().max(self.response(x).abs()).max(f64::MIN_POSITIVE)
            {
                roots.push(x);
            }
        }
        for w in edges.windows(2) {
            let (mut lo, mut hi) = (w[0], w[1]);
            let mut f = self.response(lo) - y;
            let end_f = self.response(hi) - y;
            if f == 0.0 || end_f == 0.0 || f.signum() == end_f.signum() {
                continue;
            }
            for _ in 0..100 {
                let mid = lo + (hi - lo) / 2.0;
                let m = self.response(mid) - y;
                if m == 0.0 {
                    lo = mid;
                    hi = mid;
                    break;
                }
                if f.signum() == m.signum() {
                    lo = mid;
                    f = m;
                } else {
                    hi = mid;
                }
            }
            roots.push(lo + (hi - lo) / 2.0);
        }
        roots.sort_by(f64::total_cmp);
        roots.dedup_by(|a, b| {
            (*a - *b).abs() <= 64.0 * f64::EPSILON * range[1].abs().max(f64::MIN_POSITIVE)
        });
        match roots.len() {
            1 => Ok(roots[0]),
            0 => Err(EngineError::new(
                "outside_calibration_range",
                "No inverse root in validated range",
            )),
            _ => Err(EngineError::new(
                "ambiguous_root",
                "Multiple valid inverse roots; review calibration",
            )),
        }
    }
}
pub fn fit(points: &[(String, f64, f64)], cfg: &CalibrationConfig) -> Result<Calibration> {
    validate_config(cfg)?;
    let free = matches!(cfg.intercept, Intercept::Free);
    let offset = match cfg.intercept {
        Intercept::Fixed(v) => v,
        _ => 0.0,
    };
    let columns = cfg.degree + usize::from(free);
    if points.len() <= columns {
        return Err(invalid(
            "Calibration requires more observations than fitted parameters",
        ));
    }
    let distinct: HashSet<_> = points.iter().map(|p| p.1.to_bits()).collect();
    if distinct.len() < cfg.degree + 1 {
        return Err(invalid("Insufficient distinct calibration levels"));
    }
    let scale = points.iter().map(|p| p.1.abs()).fold(0.0, f64::max);
    if scale <= 0.0 {
        return Err(invalid("Calibration concentration scale is zero"));
    }
    let mut weights = Vec::new();
    for (_, x, y) in points {
        if !x.is_finite() || !y.is_finite() || *x < 0.0 {
            return Err(invalid(
                "Calibration observations must be finite, with nonnegative concentrations",
            ));
        }
        let weight = match cfg.weighting {
            Weighting::Unweighted => 1.0,
            Weighting::InverseX if *x > 0.0 => 1.0 / x,
            Weighting::InverseX2 if *x > 0.0 => 1.0 / (x * x),
            _ => return Err(invalid(
                "1/x weighting requires positive concentrations; exclude zero standards explicitly",
            )),
        };
        if !weight.is_finite() || weight <= 0.0 {
            return Err(invalid("Invalid calibration weight"));
        }
        weights.push(weight);
    }
    if cfg.range[0] < points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min)
        || cfg.range[1] > points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max)
    {
        return Err(invalid(
            "Validated range exceeds observed standard concentrations",
        ));
    }
    let a = DMatrix::from_fn(points.len(), columns, |r, c| {
        weights[r].sqrt() * (points[r].1 / scale).powi((c + usize::from(!free)) as i32)
    });
    let b = DVector::from_iterator(
        points.len(),
        points
            .iter()
            .zip(&weights)
            .map(|(p, w)| (p.2 - offset) * w.sqrt()),
    );
    let svd = a.svd(true, true);
    let largest = svd.singular_values[0];
    let smallest = svd.singular_values[columns - 1];
    if smallest <= largest * 1e-12 {
        return Err(EngineError::new(
            "singular_calibration",
            "Rank-deficient calibration",
        ));
    }
    let solution = svd.solve(&b, largest * 1e-12).map_err(invalid)?;
    let mut coefficients = if free { Vec::new() } else { vec![offset] };
    coefficients.extend(solution.iter().copied());
    if coefficients.iter().any(|x| !x.is_finite()) {
        return Err(invalid("Calibration numerical overflow"));
    }
    let mut model = Calibration {
        range: cfg.range,
        unit: cfg.unit.clone(),
        coefficients,
        scale,
        condition_number: largest / smallest,
        residual_degrees_of_freedom: points.len() - columns,
        r_squared: None,
        weighted_rmse: 0.0,
        residual_standard_error: None,
        points: Vec::new(),
        flags: Vec::new(),
    };
    let mean = points.iter().map(|p| p.2).sum::<f64>() / points.len() as f64;
    let mut sse = 0.0;
    let mut wsse = 0.0;
    let mut sst = 0.0;
    for ((sample, x, y), w) in points.iter().zip(&weights) {
        let fitted = model.response(*x);
        let residual = y - fitted;
        let back = model.inverse(*y, cfg.range).ok();
        let accuracy = back.filter(|_| *x > 0.0).map(|v| (v / x) * 100.0);
        let mut flags = Vec::new();
        if back.is_some_and(|x| (x == cfg.range[0] || x == cfg.range[1]) && model.response(x) != *y)
        {
            flags.push("boundary_roundoff".into());
        }
        if back.is_none() {
            flags.push("inverse_unavailable".into());
        }
        if accuracy.is_some_and(|a| (a - 100.0).abs() > cfg.accuracy_tolerance_percent) {
            flags.push("calibration_accuracy".into());
        }
        sse += residual * residual;
        wsse += w * residual * residual;
        sst += (y - mean).powi(2);
        model.points.push(PointDiagnostic {
            sample: sample.clone(),
            x: *x,
            response: *y,
            weight: *w,
            fitted,
            residual,
            back_calculated: back,
            accuracy_percent: accuracy,
            flags,
        });
    }
    if !sse.is_finite() || !sst.is_finite() {
        return Err(invalid("Calibration diagnostic overflow"));
    }
    model.r_squared = if sst > 0.0 {
        Some(1.0 - sse / sst)
    } else {
        None
    };
    model.weighted_rmse = (wsse / weights.iter().sum::<f64>()).sqrt();
    model.residual_standard_error = Some((wsse / model.residual_degrees_of_freedom as f64).sqrt());
    if !model.weighted_rmse.is_finite() || !model.residual_standard_error.unwrap().is_finite() {
        return Err(invalid("Calibration diagnostic overflow"));
    }
    if cfg.degree == 3 {
        model.flags.push("cubic_requires_review".into());
    }
    // A nonmonotonic curve is retained as evidence, but inverse predictions are gated.
    let c = &model.coefficients;
    let mut probes = vec![cfg.range[0] / scale, cfg.range[1] / scale];
    if c.len() == 4 && c[3] != 0.0 {
        let vertex = -c[2] / (3.0 * c[3]);
        if vertex > probes[0] && vertex < probes[1] {
            probes.push(vertex);
        }
    }
    let derivatives: Vec<_> = probes
        .into_iter()
        .map(|x| {
            c.iter()
                .enumerate()
                .skip(1)
                .map(|(j, c)| j as f64 * c * x.powi(j as i32 - 1))
                .sum::<f64>()
        })
        .collect();
    if derivatives.iter().any(|d| *d < 0.0) && derivatives.iter().any(|d| *d > 0.0) {
        model.flags.push("nonmonotonic_calibration".into());
    }
    if model.condition_number > 1e8 {
        model.flags.push("ill_conditioned_calibration".into());
    }
    if model
        .points
        .iter()
        .any(|p| p.flags.iter().any(|f| f != "boundary_roundoff"))
    {
        model.flags.push("calibration_acceptance_failed".into());
    }
    Ok(model)
}
fn validate_config(c: &CalibrationConfig) -> Result<()> {
    if !(1..=3).contains(&c.degree)
        || !c.range[0].is_finite()
        || !c.range[1].is_finite()
        || c.range[0] < 0.0
        || c.range[0] >= c.range[1]
        || !c.lod.is_finite()
        || !c.loq.is_finite()
        || c.lod < 0.0
        || c.lod > c.loq
        || c.loq > c.range[1]
        || !c.accuracy_tolerance_percent.is_finite()
        || c.accuracy_tolerance_percent <= 0.0
        || !c.qc_cv_limit_percent.is_finite()
        || c.qc_cv_limit_percent <= 0.0
        || !c.blank_response_limit.is_finite()
        || c.blank_response_limit < 0.0
        || matches!(c.intercept,Intercept::Fixed(v) if !v.is_finite())
    {
        return Err(invalid(
            "Invalid calibration degree, limits, intercept or thresholds",
        ));
    }
    Ok(())
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Observation {
    pub sample: String,
    pub target: String,
    pub quantifier: crate::quant::Measurement,
    pub qualifiers: Vec<crate::quant::Measurement>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Concentration {
    pub sample: String,
    pub target: String,
    pub state: State,
    pub response: Option<f64>,
    pub injected_concentration: Option<f64>,
    pub concentration: Option<f64>,
    pub unit: Unit,
    pub dilution: f64,
    pub accuracy_percent: Option<f64>,
    pub qualifier_ratios: Vec<Option<f64>>,
    pub flags: Vec<String>,
    pub reviewed: bool,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Precision {
    pub target: String,
    pub role: Role,
    pub nominal: f64,
    pub sample_count: usize,
    pub n: usize,
    pub mean: Option<f64>,
    pub sd: Option<f64>,
    pub cv_percent: Option<f64>,
    pub flags: Vec<String>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Review {
    #[cfg_attr(feature = "mcp-headless", schemars(with = "String"))]
    pub id: uuid::Uuid,
    pub actor: String,
    pub reason: String,
    pub unix_ms: u64,
    pub sample: String,
    pub target: String,
    pub accepted: bool,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
pub struct BatchResult {
    pub schema_version: u32,
    #[cfg_attr(feature = "mcp-headless", schemars(with = "String"))]
    pub batch_id: uuid::Uuid,
    pub kernel_version: String,
    pub created_unix_ms: u64,
    pub request: BatchRequest,
    pub observations: Vec<Observation>,
    pub source_hashes: BTreeMap<String, String>,
    pub calibrations: BTreeMap<String, Calibration>,
    pub calibration_errors: BTreeMap<String, EngineError>,
    pub results: Vec<Concentration>,
    pub precision: Vec<Precision>,
    pub reviews: Vec<Review>,
}
pub fn validate(request: &BatchRequest) -> Result<()> {
    if request.version != 1
        || request.name.trim().is_empty()
        || request.samples.is_empty()
        || request.samples.len() > 256
        || request.targets.is_empty()
        || request.targets.len() > 64
    {
        return Err(invalid(
            "Batch requires version 1, name, 1..256 samples and 1..64 targets",
        ));
    }
    let mut ids = HashSet::new();
    for t in &request.targets {
        if t.id.trim().is_empty() || !ids.insert(t.id.clone()) {
            return Err(invalid("Target IDs must be nonempty and unique"));
        }
        if let Some(c) = &t.calibration {
            validate_config(c)?;
        }
        if t.qualifiers.len() > 8 {
            return Err(invalid("At most eight qualifiers per target"));
        }
        for q in &t.qualifiers {
            if !q.rt_tolerance_minutes.is_finite()
                || q.rt_tolerance_minutes < 0.0
                || q.ratio_range.iter().any(|x| !x.is_finite() || *x < 0.0)
                || q.ratio_range[0] > q.ratio_range[1]
            {
                return Err(invalid("Invalid qualifier ratio range"));
            }
        }
        if t.is_area_range
            .is_some_and(|r| r.iter().any(|x| !x.is_finite() || *x <= 0.0) || r[0] >= r[1])
        {
            return Err(invalid("Invalid IS area range"));
        }
        crate::quant::validate(&method(request, t)).map_err(invalid)?;
    }
    for t in &request.targets {
        if let Some(id) = &t.internal_standard {
            let s = request
                .targets
                .iter()
                .find(|s| &s.id == id)
                .ok_or_else(|| invalid("Unknown internal standard"))?;
            if id == &t.id || s.internal_standard.is_some() || s.is_area_range.is_none() {
                return Err(invalid(
                    "IS must be a separate target with area bounds and no IS dependency",
                ));
            }
        }
    }
    ids.clear();
    let mut orders = HashSet::new();
    for s in &request.samples {
        if s.id.trim().is_empty()
            || !ids.insert(s.id.clone())
            || s.source.trim().is_empty()
            || !orders.insert(s.injection_order)
            || !s.dilution.is_finite()
            || s.dilution <= 0.0
        {
            return Err(invalid(
                "Invalid sample ID, source, injection order or dilution",
            ));
        }
        for (id, x) in &s.nominal {
            if !request.targets.iter().any(|t| &t.id == id) || !x.is_finite() || *x < 0.0 {
                return Err(invalid("Invalid nominal target/concentration"));
            }
        }
        for (id, reason) in &s.exclusions {
            if !request.targets.iter().any(|t| &t.id == id) || reason.trim().is_empty() {
                return Err(invalid("Exclusion requires known target and reason"));
            }
        }
        if matches!(s.role, Role::Standard | Role::Qc) {
            for t in request.targets.iter().filter(|t| t.calibration.is_some()) {
                if !s.nominal.contains_key(&t.id) {
                    return Err(invalid(
                        "Every standard/QC requires nominal concentrations for calibrated targets",
                    ));
                }
            }
        }
    }
    Ok(())
}
fn method(r: &BatchRequest, t: &Target) -> crate::quant::Method {
    let mut analytes = vec![t.quantifier.extraction.clone()];
    analytes.extend(t.qualifiers.iter().map(|q| q.ion.extraction.clone()));
    crate::quant::Method {
        version: 1,
        name: r.name.clone(),
        detection_smoothing: r.detection_smoothing,
        minimum_height: r.minimum_height,
        boundary_fraction: r.boundary_fraction,
        analytes,
    }
}
pub fn run(request: BatchRequest, control: &crate::jobs::JobControl) -> Result<BatchResult> {
    validate(&request)?;
    let mut observations = Vec::new();
    let mut retained_points = 0usize;
    let mut hashes = BTreeMap::new();
    for s in &request.samples {
        for t in &request.targets {
            control.check()?;
            let req = crate::domain::Request {
                version: 1,
                operation_id: Default::default(),
                actor: "targeted-extraction".into(),
                operation: crate::domain::Operation::Quantify {
                    method: method(&request, t),
                },
            };
            let response = crate::engine::execute(std::path::Path::new(&s.source), req, control)?;
            if let Some(hash) = response.source_sha256 {
                if hashes.get(&s.id).is_some_and(|old| old != &hash) {
                    return Err(EngineError::new(
                        "changed_source",
                        "Source changed during batch",
                    ));
                }
                hashes.insert(s.id.clone(), hash);
            }
            let crate::engine::Output::Quantification {
                mut measurements, ..
            } = response.output
            else {
                unreachable!()
            };
            retained_points += measurements.iter().map(|m| m.trace.len()).sum::<usize>();
            if retained_points > 2_000_000 {
                return Err(EngineError::new(
                    "resource_limit",
                    "Targeted batch exceeds two million retained points",
                ));
            }
            let quantifier = measurements.remove(0);
            observations.push(Observation {
                sample: s.id.clone(),
                target: t.id.clone(),
                quantifier,
                qualifiers: measurements,
            });
        }
        let after = crate::project::source_identity_controlled(
            std::path::Path::new(&s.source),
            Some(control),
        )?
        .0;
        if hashes.get(&s.id) != Some(&after) {
            return Err(EngineError::new(
                "changed_source",
                "Source changed during extraction",
            ));
        }
    }
    evaluate(request, observations, hashes)
}
fn usable(m: &crate::quant::Measurement) -> bool {
    matches!(
        m.status,
        crate::quant::Status::Automatic
            | crate::quant::Status::Manual
            | crate::quant::Status::Reviewed
    ) && m.peak.as_ref().is_some_and(|p| p.area.is_finite())
}
/// Evaluates explicit measurement evidence; run() is the verified raw-file path.
pub fn evaluate(
    request: BatchRequest,
    observations: Vec<Observation>,
    hashes: BTreeMap<String, String>,
) -> Result<BatchResult> {
    validate(&request)?;
    if observations
        .iter()
        .flat_map(|o| std::iter::once(&o.quantifier).chain(o.qualifiers.iter()))
        .map(|m| m.trace.len())
        .sum::<usize>()
        > 2_000_000
    {
        return Err(EngineError::new(
            "resource_limit",
            "Targeted evidence exceeds two million retained points",
        ));
    }
    if observations.len() != request.samples.len() * request.targets.len() {
        return Err(invalid("Observation matrix is incomplete"));
    }
    let mut keys = HashSet::new();
    for o in &observations {
        let target = request
            .targets
            .iter()
            .find(|t| t.id == o.target)
            .ok_or_else(|| invalid("Unknown observed target"))?;
        if !request.samples.iter().any(|s| s.id == o.sample)
            || !keys.insert((&o.sample, &o.target))
            || o.qualifiers.len() != target.qualifiers.len()
        {
            return Err(invalid(
                "Unknown/duplicate observation or qualifier mismatch",
            ));
        }
        let expected_method = method(&request, target);
        let params = crate::quant::validate(&expected_method).map_err(invalid)?;
        for (index, m) in std::iter::once(&o.quantifier)
            .chain(o.qualifiers.iter())
            .enumerate()
        {
            if serde_json::to_value(&m.params).map_err(invalid)?
                != serde_json::to_value(&params[index]).map_err(invalid)?
                || m.analyte != expected_method.analytes[index].extraction.name
            {
                return Err(invalid(
                    "Observed ion parameters do not match target method",
                ));
            }

            crate::engine::validate_points(&m.trace)?;
            if let Some(p) = &m.peak {
                let bounds = expected_method.analytes[index].rt_window;
                if p.start < bounds[0] || p.end > bounds[1] {
                    return Err(invalid("Observed peak outside target RT window"));
                }
                let actual = crate::quant::measure(&m.trace, p.start, p.end).map_err(invalid)?;
                if actual != *p {
                    return Err(EngineError::new(
                        "corrupt_result",
                        "Peak differs from retained trace",
                    ));
                }
            }
        }
    }
    let find = |sample: &str, target: &str| {
        observations
            .iter()
            .find(|o| o.sample == sample && o.target == target)
            .unwrap()
    };
    let mut responses = BTreeMap::new();
    let mut flags = BTreeMap::new();
    for s in &request.samples {
        for t in &request.targets {
            let o = find(&s.id, &t.id);
            let mut f = Vec::new();
            if o.quantifier.peak.as_ref().is_some_and(|p| p.area <= 0.0) {
                f.push("nonpositive_analyte_area".into());
            }
            let mut value = if usable(&o.quantifier) {
                o.quantifier.peak.as_ref().map(|p| p.area)
            } else {
                None
            };
            if let Some(id) = &t.internal_standard {
                let is = find(&s.id, id);
                if !usable(&is.quantifier)
                    || is.quantifier.peak.as_ref().is_some_and(|p| p.area <= 0.0)
                {
                    f.push("internal_standard_missing_or_invalid".into());
                    if is.quantifier.peak.as_ref().is_some_and(|p| p.area <= 0.0) {
                        f.push("internal_standard_nonpositive".into());
                    }
                    if matches!(is.quantifier.status, crate::quant::Status::Ambiguous) {
                        f.push("internal_standard_ambiguous".into());
                    }
                    value = None;
                } else {
                    let area = is.quantifier.peak.as_ref().unwrap().area;
                    let bounds = request
                        .targets
                        .iter()
                        .find(|t| &t.id == id)
                        .unwrap()
                        .is_area_range
                        .unwrap();
                    if area < bounds[0] || area > bounds[1] {
                        f.push("internal_standard_area".into());
                    }
                    value = value.map(|v| v / area);
                }
            }
            if value.is_some_and(|v| !v.is_finite()) {
                return Err(invalid("Response overflow"));
            }
            for (q, m) in t.qualifiers.iter().zip(&o.qualifiers) {
                if !usable(m) {
                    f.push("qualifier_missing_or_invalid".into());
                } else if let Some(p) = o.quantifier.peak.as_ref().filter(|p| p.area != 0.0) {
                    if (m.peak.as_ref().unwrap().apex_rt - p.apex_rt).abs() > q.rt_tolerance_minutes
                    {
                        f.push("qualifier_rt".into());
                    }
                    let ratio = m.peak.as_ref().unwrap().area / p.area;
                    if !ratio.is_finite() || ratio < q.ratio_range[0] || ratio > q.ratio_range[1] {
                        f.push("qualifier_ratio".into());
                    }
                }
            }
            responses.insert((s.id.clone(), t.id.clone()), value);
            flags.insert((s.id.clone(), t.id.clone()), f);
        }
    }
    let mut calibrations = BTreeMap::new();
    let mut errors = BTreeMap::new();
    for t in &request.targets {
        if let Some(cfg) = &t.calibration {
            let mut points = Vec::new();
            let mut missing = false;
            for s in request
                .samples
                .iter()
                .filter(|s| s.role == Role::Standard && !s.exclusions.contains_key(&t.id))
            {
                if let Some(y) = responses[&(s.id.clone(), t.id.clone())] {
                    points.push((s.id.clone(), s.nominal[&t.id], y));
                } else {
                    missing = true;
                }
            }
            let model = if missing {
                Err(EngineError::new(
                    "invalid_standard",
                    "Included standard has no usable response; explicitly exclude with reason",
                ))
            } else {
                fit(&points, cfg)
            };
            match model {
                Ok(m) => {
                    if cfg.range[0] < points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min)
                        || cfg.range[1]
                            > points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max)
                    {
                        errors.insert(
                            t.id.clone(),
                            invalid("Validated range exceeds observed standard concentrations"),
                        );
                    } else {
                        calibrations.insert(t.id.clone(), m);
                    }
                }
                Err(e) => {
                    errors.insert(t.id.clone(), e);
                }
            }
        }
    }
    let mut results = Vec::new();
    for s in &request.samples {
        for t in &request.targets {
            let Some(cfg) = &t.calibration else {
                continue;
            };
            let o = find(&s.id, &t.id);
            let key = (s.id.clone(), t.id.clone());
            let response = responses[&key];
            let mut f = flags[&key].clone();
            let mut state = if matches!(
                o.quantifier.status,
                crate::quant::Status::Failed
                    | crate::quant::Status::Pending
                    | crate::quant::Status::Cancelled
            ) {
                State::Failed
            } else if matches!(o.quantifier.status, crate::quant::Status::Ambiguous) {
                State::Ambiguous
            } else {
                State::Missing
            };
            if matches!(
                o.quantifier.status,
                crate::quant::Status::Pending | crate::quant::Status::Cancelled
            ) {
                f.push("unprocessed_or_cancelled".into());
            }
            let mut injected = None;
            if let Some(y) = response {
                if s.role == Role::Blank {
                    state = State::Present;
                    if y > cfg.blank_response_limit {
                        f.push("blank_contamination".into());
                    }
                } else if let Some(m) = calibrations.get(&t.id) {
                    match m.inverse(y, cfg.range) {
                        Ok(x) => {
                            injected = Some(x);
                            if (x == cfg.range[0] || x == cfg.range[1]) && m.response(x) != y {
                                f.push("boundary_roundoff".into());
                            }
                            state = if x < cfg.lod {
                                State::BelowDetection
                            } else if x < cfg.loq {
                                State::BelowLoq
                            } else {
                                State::Present
                            };
                        }
                        Err(e) => {
                            state = if e.code == "ambiguous_root" {
                                State::Ambiguous
                            } else if !m.flags.iter().any(|f| f == "nonmonotonic_calibration") {
                                let low = m.response(cfg.range[0]);
                                let high = m.response(cfg.range[1]);
                                if (high > low && y < low) || (high < low && y > low) {
                                    State::BelowRange
                                } else {
                                    State::AboveRange
                                }
                            } else {
                                State::Failed
                            };
                            f.push(e.code);
                        }
                    }
                    if !m.flags.is_empty() {
                        f.extend(m.flags.clone());
                    }
                } else {
                    state = State::Failed;
                    f.push("calibration_failed".into());
                }
            }
            let concentration = if state == State::Present && s.role != Role::Blank {
                injected.map(|x| x * s.dilution)
            } else {
                None
            };
            if concentration.is_some_and(|v| !v.is_finite()) {
                return Err(invalid("Dilution overflow"));
            }
            let accuracy = injected.and_then(|x| {
                s.nominal
                    .get(&t.id)
                    .filter(|n| **n > 0.0)
                    .map(|n| (x / n) * 100.0)
            });
            if accuracy.is_some_and(|a| (a - 100.0).abs() > cfg.accuracy_tolerance_percent) {
                f.push("accuracy".into());
            }
            let ratios = o
                .qualifiers
                .iter()
                .map(|q| {
                    if usable(q)
                        && usable(&o.quantifier)
                        && o.quantifier.peak.as_ref().is_some_and(|p| p.area != 0.0)
                    {
                        Some(
                            q.peak.as_ref().unwrap().area
                                / o.quantifier.peak.as_ref().unwrap().area,
                        )
                    } else {
                        None
                    }
                })
                .collect();
            if let Some(reason) = s.exclusions.get(&t.id) {
                f.push(format!("calibration_excluded: {reason}"));
            }
            results.push(Concentration {
                sample: s.id.clone(),
                target: t.id.clone(),
                state,
                response,
                injected_concentration: injected,
                concentration,
                unit: cfg.unit.clone(),
                dilution: s.dilution,
                accuracy_percent: accuracy,
                qualifier_ratios: ratios,
                flags: f,
                reviewed: false,
            });
        }
    }
    let mut precision = Vec::new();
    for t in &request.targets {
        let Some(cfg) = &t.calibration else {
            continue;
        };
        for role in [Role::Standard, Role::Qc] {
            let mut groups: BTreeMap<u64, (usize, Vec<f64>)> = BTreeMap::new();
            for sample in request.samples.iter().filter(|s| s.role == role) {
                let group = groups.entry(sample.nominal[&t.id].to_bits()).or_default();
                group.0 += 1;
                let row = results
                    .iter()
                    .find(|r| r.sample == sample.id && r.target == t.id)
                    .unwrap();
                if row.state == State::Present && !sample.exclusions.contains_key(&t.id) {
                    if let Some(v) = row.injected_concentration {
                        group.1.push(v);
                    }
                }
            }
            for (nominal, (sample_count, values)) in groups {
                let n = values.len();
                if n == 0 {
                    precision.push(Precision {
                        target: t.id.clone(),
                        role: role.clone(),
                        nominal: f64::from_bits(nominal),
                        sample_count,
                        n,
                        mean: None,
                        sd: None,
                        cv_percent: None,
                        flags: vec!["no_valid_replicates".into()],
                    });
                    continue;
                }
                let mean = values.iter().map(|v| v / n as f64).sum::<f64>();
                let sd = if n > 1 {
                    Some(
                        (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1) as f64)
                            .sqrt(),
                    )
                } else {
                    None
                };
                let cv = sd.filter(|_| mean != 0.0).map(|s| 100.0 * (s / mean.abs()));
                if !mean.is_finite()
                    || sd.is_some_and(|v| !v.is_finite())
                    || cv.is_some_and(|v| !v.is_finite())
                {
                    return Err(invalid("Precision numerical overflow"));
                }
                let mut flags = Vec::new();
                if cv.is_some_and(|v| v > cfg.qc_cv_limit_percent) {
                    flags.push("precision_limit".into());
                }
                if n < 2 {
                    flags.push("insufficient_replicates".into());
                }
                if n < sample_count {
                    flags.push("incomplete_precision_group".into());
                }
                precision.push(Precision {
                    target: t.id.clone(),
                    role: role.clone(),
                    nominal: f64::from_bits(nominal),
                    sample_count,
                    n,
                    mean: Some(mean),
                    sd,
                    cv_percent: cv,
                    flags,
                });
            }
        }
    }
    Ok(BatchResult {
        schema_version: 1,
        batch_id: uuid::Uuid::new_v4(),
        kernel_version: format!("chromascope/{}/targeted-v1", env!("CARGO_PKG_VERSION")),
        created_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(invalid)?
            .as_millis()
            .try_into()
            .map_err(invalid)?,
        request,
        observations,
        source_hashes: hashes,
        calibrations,
        calibration_errors: errors,
        results,
        precision,
        reviews: Vec::new(),
    })
}
pub fn verify(batch: &BatchResult) -> Result<()> {
    if batch.schema_version != 1
        || batch.kernel_version != format!("chromascope/{}/targeted-v1", env!("CARGO_PKG_VERSION"))
        || batch.created_unix_ms == 0
    {
        return Err(EngineError::new(
            "unsupported_capability",
            "Unsupported targeted schema/kernel or missing timestamp",
        ));
    }
    if !batch.source_hashes.is_empty()
        && (batch.source_hashes.len() != batch.request.samples.len()
            || batch.request.samples.iter().any(|s| {
                !batch
                    .source_hashes
                    .get(&s.id)
                    .is_some_and(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
            }))
    {
        return Err(invalid("Incomplete or malformed raw-source hashes"));
    }

    let replay = evaluate(
        batch.request.clone(),
        batch.observations.clone(),
        batch.source_hashes.clone(),
    )?;
    let mut expected = replay.results.clone();
    let mut review_ids = HashSet::new();
    for review in &batch.reviews {
        if review.actor.trim().is_empty()
            || review.reason.trim().is_empty()
            || review.unix_ms == 0
            || !review_ids.insert(review.id)
        {
            return Err(invalid(
                "Review requires unique identity, timestamp, actor and reason",
            ));
        }
        let original = replay
            .results
            .iter()
            .find(|r| r.sample == review.sample && r.target == review.target)
            .ok_or_else(|| invalid("Unknown reviewed result"))?;
        if review.accepted && (original.state != State::Present || original.concentration.is_none())
        {
            return Err(invalid("Cannot accept an unavailable concentration"));
        }
        let row = expected
            .iter_mut()
            .find(|r| r.sample == review.sample && r.target == review.target)
            .unwrap();
        row.state = if review.accepted {
            original.state.clone()
        } else {
            State::Rejected
        };
        row.reviewed = review.accepted;
    }
    if expected != batch.results
        || replay.calibrations != batch.calibrations
        || serde_json::to_value(replay.calibration_errors).map_err(invalid)?
            != serde_json::to_value(&batch.calibration_errors).map_err(invalid)?
        || replay.precision != batch.precision
    {
        return Err(EngineError::new(
            "corrupt_result",
            "Quantification differs from replayed evidence/history",
        ));
    }
    Ok(())
}

fn replay_state(
    r: &BatchRequest,
    o: &[Observation],
    h: &BTreeMap<String, String>,
    sample: &str,
    target: &str,
) -> Result<State> {
    Ok(evaluate(r.clone(), o.to_vec(), h.clone())?
        .results
        .into_iter()
        .find(|r| r.sample == sample && r.target == target)
        .ok_or_else(|| invalid("Unknown result"))?
        .state)
}
pub fn review(
    batch: &BatchResult,
    expected_revision: usize,
    sample: &str,
    target: &str,
    accepted: bool,
    actor: &str,
    reason: &str,
) -> Result<BatchResult> {
    verify(batch)?;
    if expected_revision != batch.reviews.len() {
        return Err(EngineError::new(
            "stale_revision",
            "Review revision changed",
        ));
    }
    if actor.trim().is_empty() || reason.trim().is_empty() {
        return Err(invalid("Review requires actor and reason"));
    }
    let mut next = batch.clone();
    let state = replay_state(
        &batch.request,
        &batch.observations,
        &batch.source_hashes,
        sample,
        target,
    )?;
    let row = next
        .results
        .iter_mut()
        .find(|r| r.sample == sample && r.target == target)
        .ok_or_else(|| invalid("Unknown result"))?;
    if accepted && (state != State::Present || row.concentration.is_none()) {
        return Err(invalid(
            "Only present quantified concentrations can be accepted",
        ));
    }
    row.state = if accepted { state } else { State::Rejected };
    row.reviewed = accepted;
    next.reviews.push(Review {
        id: uuid::Uuid::new_v4(),
        actor: actor.into(),
        reason: reason.into(),
        unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(invalid)?
            .as_millis()
            .try_into()
            .map_err(invalid)?,
        sample: sample.into(),
        target: target.into(),
        accepted,
    });
    Ok(next)
}
pub fn csv(batch: &BatchResult) -> Result<String> {
    verify(batch)?;
    let mut out=String::from("sample,target,state,response,injected_concentration,concentration,unit,dilution,accuracy_percent,reviewed,flags,role,injection_order,nominal_concentration,response_unit,qualifier_ratios\n");
    let quote = |s: String| format!("\"{}\"", s.replace('"', "\"\""));
    let number = |n: Option<f64>| n.map(|v| v.to_string()).unwrap_or_default();
    for r in &batch.results {
        let target = batch
            .request
            .targets
            .iter()
            .find(|t| t.id == r.target)
            .unwrap();
        let sample = batch
            .request
            .samples
            .iter()
            .find(|s| s.id == r.sample)
            .unwrap();
        out.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            quote(r.sample.clone()),
            quote(r.target.clone()),
            quote(format!("{:?}", r.state)),
            number(r.response),
            number(r.injected_concentration),
            number(if r.state == State::Rejected {
                None
            } else {
                r.concentration
            }),
            quote(r.unit.label().into()),
            r.dilution,
            number(r.accuracy_percent),
            r.reviewed,
            quote(r.flags.join(";")),
            quote(format!("{:?}", sample.role)),
            sample.injection_order,
            number(sample.nominal.get(&r.target).copied()),
            quote(
                if target.internal_standard.is_some() {
                    "dimensionless area ratio"
                } else {
                    "instrument intensity * minute"
                }
                .into()
            ),
            quote(serde_json::to_string(&r.qualifier_ratios).map_err(invalid)?)
        ));
    }
    Ok(out)
}

/// Independent table of standard fits/residuals; full configuration and lineage stay in JSON.
pub fn calibration_csv(batch: &BatchResult) -> Result<String> {
    verify(batch)?;
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let num = |x: Option<f64>| x.map(|v| v.to_string()).unwrap_or_default();
    let mut out=String::from("target,sample,nominal,unit,response,weight,fitted,residual,back_calculated,accuracy_percent,flags\n");
    for (id, model) in &batch.calibrations {
        let unit = batch
            .request
            .targets
            .iter()
            .find(|t| &t.id == id)
            .unwrap()
            .calibration
            .as_ref()
            .unwrap()
            .unit
            .label();
        for p in &model.points {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{}\n",
                quote(id),
                quote(&p.sample),
                p.x,
                quote(unit),
                p.response,
                p.weight,
                p.fitted,
                p.residual,
                num(p.back_calculated),
                num(p.accuracy_percent),
                quote(&p.flags.join(";"))
            ));
        }
    }
    Ok(out)
}
