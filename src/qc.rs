//! Evidence-preserving batch QC and explicitly designed method-validation studies.
use crate::domain::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    Accuracy,
    MaximumAccuracyDeviation,
    ControlChart,
    Precision,
    Blank,
    Carryover,
    InternalStandardCv,
    RtDrift,
    MassError,
    OrderSlope,
    RtDriftSlope,
    MassDriftSlope,
    InternalStandardSlope,
    Missingness,
    CalibrationAcceptance,
    CalibrationR2,
    CalibrationRmse,
    CalibrationAccuracyDeviation,
    Recovery,
    MatrixFactor,
    ProcessEfficiency,
    Selectivity,
    Stability,
    DilutionIntegrity,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Preparation {
    PreExtraction,
    PostExtraction,
    Neat,
    Blank,
    Fresh,
    Stored,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub id: String,
    pub target: String,
    pub batch: String,
    pub group: String,
    pub role: String,
    pub order: u32,
    pub unit: String,
    pub response_unit: String,
    pub value: Option<f64>,
    pub nominal: Option<f64>,
    pub response: Option<f64>,
    pub is_response: Option<f64>,
    pub rt_minutes: Option<f64>,
    pub expected_rt_minutes: Option<f64>,
    pub mass_error_ppm: Option<f64>,
    pub calibration_accepted: Option<bool>,
    /// Identifies the immutable result, acquisition or external study record.
    pub source: String,
    #[serde(default)]
    pub preparation: Option<Preparation>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub target: String,
    pub group: Option<String>,
    pub role: Option<String>,
    #[serde(default)]
    pub batch: Option<String>,
    pub metric: Metric,
    pub lower: f64,
    pub upper: f64,
    pub minimum_n: usize,
    pub required: bool,
    /// Recovery/matrix/selectivity/stability comparisons use an explicitly named group.
    pub reference_group: Option<String>,
    #[serde(default)]
    pub calibration: Option<crate::targeted::CalibrationConfig>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Study {
    pub version: u32,
    pub method: String,
    pub purpose: String,
    pub observations: Vec<Observation>,
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub targeted_evidence: Option<Box<crate::targeted::BatchResult>>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pass,
    Fail,
    Indeterminate,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Decision {
    pub rule: Rule,
    pub status: Status,
    pub value: Option<f64>,
    pub unit: String,
    pub calculation: String,
    pub reason: String,
    pub evidence: Vec<String>,
    pub n: usize,
    pub statistics: BTreeMap<String, f64>,
    pub calibration: Option<crate::targeted::Calibration>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    #[cfg_attr(feature = "mcp-headless", schemars(with = "String"))]
    pub report_id: uuid::Uuid,
    pub created_unix_ms: u64,
    pub study: Study,
    pub status: Status,
    pub decisions: Vec<Decision>,
    pub review_queue: Vec<String>,
    pub reviews: Vec<Review>,
    pub acceptance: BatchAcceptance,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BatchAcceptance {
    pub required_rules: Vec<String>,
    pub passed: usize,
    pub failed: usize,
    pub indeterminate: usize,
    pub threshold: String,
    pub calculation: String,
    pub reason: String,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Review {
    #[cfg_attr(feature = "mcp-headless", schemars(with = "String"))]
    pub id: uuid::Uuid,
    pub unix_ms: u64,
    pub rule_id: String,
    pub actor: String,
    pub reason: String,
    pub acknowledged: bool,
}
fn invalid(s: &str) -> EngineError {
    EngineError::new("invalid_parameters", s)
}
fn mean(v: &[f64]) -> Option<f64> {
    (!v.is_empty())
        .then(|| v.iter().sum::<f64>() / v.len() as f64)
        .filter(|v| v.is_finite())
}
fn cv(v: &[f64]) -> Option<f64> {
    let m = mean(v)?;
    if v.len() < 2 || m <= 0.0 {
        return None;
    }
    Some((v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (v.len() - 1) as f64).sqrt() / m * 100.0)
}
fn statistics(prefix: &str, values: &[f64], out: &mut BTreeMap<String, f64>) {
    if let Some(center) = mean(values) {
        out.insert(format!("{prefix}_mean"), center);
        if values.len() > 1 {
            let sd = (values.iter().map(|v| (v - center).powi(2)).sum::<f64>()
                / (values.len() - 1) as f64)
                .sqrt();
            if sd.is_finite() {
                out.insert(format!("{prefix}_sample_sd"), sd);
            }
        }
    }
}
pub fn evaluate(study: Study) -> Result<Report> {
    if let Some(batch) = &study.targeted_evidence {
        let replay = targeted_study(batch, study.rules.clone())?;
        if replay.observations != study.observations
            || replay.method != study.method
            || replay.purpose != study.purpose
        {
            return Err(EngineError::new(
                "corrupt_result",
                "Targeted QC observations differ from original evidence",
            ));
        }
    }
    if study.version != 1
        || study.method.trim().is_empty()
        || !matches!(study.purpose.as_str(), "batch_qc" | "method_validation")
        || study.observations.len() > 16384
        || study.rules.len() > 1024
    {
        return Err(invalid(
            "Use version 1, method/purpose and bounded observations/rules",
        ));
    }
    let mut ids = HashSet::new();
    let mut orders = BTreeMap::new();
    for o in &study.observations {
        if o.id.trim().is_empty()
            || !ids.insert(&o.id)
            || o.target.trim().is_empty()
            || o.source.trim().is_empty()
            || o.unit.trim().is_empty()
            || o.response_unit.trim().is_empty()
            || o.role.trim().is_empty()
            || o.batch.trim().is_empty()
            || o.group.trim().is_empty()
            || [
                o.value,
                o.nominal,
                o.response,
                o.is_response,
                o.rt_minutes,
                o.expected_rt_minutes,
                o.mass_error_ppm,
            ]
            .iter()
            .flatten()
            .any(|v| !v.is_finite())
            || o.nominal.is_some_and(|v| v < 0.0)
            || o.rt_minutes.is_some_and(|v| v < 0.0)
            || o.expected_rt_minutes.is_some_and(|v| v < 0.0)
        {
            return Err(invalid(
                "Invalid or duplicate observation; units and source evidence are required",
            ));
        }
        if orders
            .insert((&o.batch, &o.target, o.order), &o.id)
            .is_some()
        {
            return Err(invalid("Injection order must be unique per batch/target"));
        }
    }
    ids.clear();
    let mut decisions = Vec::new();
    for rule in &study.rules {
        if rule.id.trim().is_empty()
            || !ids.insert(&rule.id)
            || !rule.lower.is_finite()
            || !rule.upper.is_finite()
            || rule.lower > rule.upper
            || rule.minimum_n == 0
        {
            return Err(invalid("Invalid rule ID, bounds or minimum_n"));
        }
        let mut obs: Vec<_> = study
            .observations
            .iter()
            .filter(|o| {
                o.target == rule.target
                    && rule.group.as_ref().is_none_or(|g| g == &o.group)
                    && rule.role.as_ref().is_none_or(|r| r == &o.role)
                    && rule.batch.as_ref().is_none_or(|b| b == &o.batch)
            })
            .collect();
        obs.sort_by_key(|o| (&o.batch, o.order));
        let refs: Vec<_> = study
            .observations
            .iter()
            .filter(|o| {
                o.target == rule.target
                    && rule.reference_group.as_ref().is_some_and(|g| g == &o.group)
            })
            .collect();
        let units: HashSet<_> = obs.iter().chain(refs.iter()).map(|o| &o.unit).collect();
        if units.len() > 1 {
            return Err(invalid(
                "Rule combines incompatible units; convert explicitly before evaluation",
            ));
        }
        let response_units: HashSet<_> = obs
            .iter()
            .chain(refs.iter())
            .map(|o| &o.response_unit)
            .collect();
        if response_units.len() > 1 {
            return Err(invalid("Rule combines incompatible response units"));
        }
        let values: Vec<_> = obs.iter().filter_map(|o| o.value).collect();
        let mut components = BTreeMap::new();
        statistics("value", &values, &mut components);
        statistics(
            "reference_value",
            &refs.iter().filter_map(|o| o.value).collect::<Vec<_>>(),
            &mut components,
        );
        statistics(
            "response",
            &obs.iter().filter_map(|o| o.response).collect::<Vec<_>>(),
            &mut components,
        );
        statistics(
            "reference_response",
            &refs.iter().filter_map(|o| o.response).collect::<Vec<_>>(),
            &mut components,
        );
        let mut evidence: Vec<_> = obs.iter().map(|o| o.id.clone()).collect();
        let mut issues = Vec::new();
        let mut calibration = None;
        if obs.is_empty() {
            issues.push("No observations match target/group/role/batch selection".to_string());
        }
        let (value, n, unit, formula) = match rule.metric {
            Metric::CalibrationR2
            | Metric::CalibrationRmse
            | Metric::CalibrationAccuracyDeviation => {
                let points: Vec<_> = obs
                    .iter()
                    .filter_map(|o| Some((o.id.clone(), o.nominal?, o.response?)))
                    .collect();
                if let Some(config) = &rule.calibration {
                    if obs.iter().any(|o| o.unit != config.unit.label()) {
                        return Err(invalid("Calibration units differ from observation units"));
                    }
                    match if obs.iter().any(|o| o.role != "standard") {
                        Err(invalid(
                            "Calibration must select standard-role observations",
                        ))
                    } else {
                        crate::targeted::fit(&points, config)
                    } {
                        Ok(fit) => calibration = Some(fit),
                        Err(e) => issues.push(format!("Calibration fit unavailable: {e}")),
                    }
                } else {
                    issues.push(
                        "Calibration metric requires an explicit calibration model/configuration"
                            .into(),
                    );
                }
                let value = calibration.as_ref().and_then(|fit| match rule.metric {
                    Metric::CalibrationR2 => fit.r_squared,
                    Metric::CalibrationRmse => Some(fit.weighted_rmse),
                    _ => {
                        if fit.points.iter().all(|p| p.accuracy_percent.is_some()) {
                            fit.points
                                .iter()
                                .filter_map(|p| p.accuracy_percent.map(|v| (v - 100.0).abs()))
                                .reduce(f64::max)
                        } else {
                            None
                        }
                    }
                });
                (value,points.len(),match rule.metric {Metric::CalibrationR2=>"dimensionless",Metric::CalibrationAccuracyDeviation=>"%",_=>obs.first().map(|o|o.response_unit.as_str()).unwrap_or("response")},"Configured weighted SVD calibration; R2=1-SSE/SST; RMSE=sqrt(sum(w*residual²)/sum(w)); max absolute back-calculated accuracy deviation")
            }
            Metric::MaximumAccuracyDeviation => {
                let v: Vec<_> = obs
                    .iter()
                    .filter_map(|o| {
                        Some((100.0 * o.value? / o.nominal.filter(|v| *v > 0.0)? - 100.0).abs())
                    })
                    .collect();
                (
                    v.iter().copied().reduce(f64::max),
                    v.len(),
                    "%",
                    "max(abs(100 * measured / nominal - 100)); every replicate",
                )
            }
            Metric::ControlChart => {
                evidence.extend(refs.iter().map(|o| o.id.clone()));
                let reference: Vec<_> = refs.iter().filter_map(|o| o.value).collect();
                let levels: HashSet<_> = obs
                    .iter()
                    .chain(refs.iter())
                    .filter_map(|o| o.nominal.map(f64::to_bits))
                    .collect();
                let value = mean(&reference).and_then(|center| {
                    if reference.len() < 2 || levels.len() > 1 {
                        return None;
                    }
                    let sd = (reference.iter().map(|v| (v - center).powi(2)).sum::<f64>()
                        / (reference.len() - 1) as f64)
                        .sqrt();
                    if sd <= 0.0 {
                        return None;
                    }
                    values
                        .iter()
                        .map(|v| ((v - center) / sd).abs())
                        .reduce(f64::max)
                });
                (
                    value,
                    values.len().min(reference.len()),
                    "reference SD",
                    "max(abs(value - independent reference mean) / reference sample SD)",
                )
            }
            Metric::Accuracy | Metric::DilutionIntegrity => {
                let v: Vec<_> = obs
                    .iter()
                    .filter_map(|o| Some(o.value? / o.nominal.filter(|x| *x > 0.0)? * 100.0))
                    .collect();
                (mean(&v), v.len(), "%", "mean(100 * measured / nominal)")
            }
            Metric::Precision => (
                if obs
                    .iter()
                    .filter_map(|o| o.nominal.map(f64::to_bits))
                    .collect::<HashSet<_>>()
                    .len()
                    > 1
                {
                    issues.push("Precision requires one nominal level or a declared pooled QC without nominal levels".into());
                    None
                } else {
                    cv(&values)
                },
                values.len(),
                "%",
                "100 * sample SD / mean; SD denominator n-1",
            ),
            Metric::InternalStandardCv => {
                let v: Vec<_> = obs.iter().filter_map(|o| o.is_response).collect();
                (
                    cv(&v),
                    v.len(),
                    "%",
                    "100 * sample SD(IS response) / mean(IS response)",
                )
            }
            Metric::Blank => {
                let v: Vec<_> = obs.iter().filter_map(|o| o.response).collect();
                (
                    v.iter().copied().reduce(f64::max),
                    v.len(),
                    obs.first()
                        .map(|o| o.response_unit.as_str())
                        .unwrap_or("response"),
                    "maximum blank response (signed)",
                )
            }
            Metric::RtDrift => {
                let v: Vec<_> = obs
                    .iter()
                    .filter_map(|o| Some((o.rt_minutes? - o.expected_rt_minutes?).abs()))
                    .collect();
                (
                    v.iter().copied().reduce(f64::max),
                    v.len(),
                    "minute",
                    "max(abs(observed RT - expected RT))",
                )
            }
            Metric::MassError => {
                let v: Vec<_> = obs
                    .iter()
                    .filter_map(|o| o.mass_error_ppm.map(f64::abs))
                    .collect();
                (
                    v.iter().copied().reduce(f64::max),
                    v.len(),
                    "ppm",
                    "max(abs(observed mass error ppm))",
                )
            }
            Metric::Missingness => (
                (!obs.is_empty())
                    .then(|| 100.0 * (obs.len() - values.len()) as f64 / obs.len() as f64),
                obs.len(),
                "%",
                "100 * missing measured values / expected observations",
            ),
            Metric::CalibrationAcceptance => {
                let v: Vec<_> = obs.iter().filter_map(|o| o.calibration_accepted).collect();
                ((!v.is_empty()).then(||100.0*v.iter().filter(|x|**x).count() as f64/v.len() as f64),v.len(),"%","100 * accepted standards / expected standards; missing acceptance remains indeterminate")
            }
            Metric::OrderSlope
            | Metric::RtDriftSlope
            | Metric::MassDriftSlope
            | Metric::InternalStandardSlope => {
                let v: Vec<_> = obs
                    .iter()
                    .filter_map(|o| {
                        Some((
                            o.order as f64,
                            match rule.metric {
                                Metric::RtDriftSlope => o.rt_minutes? - o.expected_rt_minutes?,
                                Metric::MassDriftSlope => o.mass_error_ppm?,
                                Metric::InternalStandardSlope => o.is_response?,
                                _ => o.value?,
                            },
                        ))
                    })
                    .collect();
                let batches: HashSet<_> = obs.iter().map(|o| &o.batch).collect();
                let x: Vec<_> = v.iter().map(|p| p.0).collect();
                let y: Vec<_> = v.iter().map(|p| p.1).collect();
                let slope = mean(&x).zip(mean(&y)).and_then(|(mx, my)| {
                    let den = x.iter().map(|x| (x - mx).powi(2)).sum::<f64>();
                    (v.len() > 1
                        && den > 0.0
                        && batches.len() == 1
                        && (rule.metric != Metric::OrderSlope
                            || obs
                                .iter()
                                .filter_map(|o| o.nominal.map(f64::to_bits))
                                .collect::<HashSet<_>>()
                                .len()
                                <= 1))
                        .then(|| v.iter().map(|(x, y)| (x - mx) * (y - my)).sum::<f64>() / den)
                });
                (
                    slope,
                    v.len(),
                    match rule.metric {
                        Metric::RtDriftSlope => "minute / injection",
                        Metric::MassDriftSlope => "ppm / injection",
                        Metric::InternalStandardSlope => {
                            "instrument intensity * minute / injection"
                        }
                        _ => "declared value unit / injection",
                    },
                    "OLS slope(selected signal vs injection order), one batch; association only",
                )
            }
            Metric::Carryover => {
                let mut ratios = Vec::new();
                for o in &obs {
                    let prev = study
                        .observations
                        .iter()
                        .filter(|p| p.target == o.target && p.batch == o.batch && p.order < o.order)
                        .max_by_key(|p| p.order);
                    if let Some(p) =
                        prev.filter(|p| o.role == "blank" && p.response_unit == o.response_unit)
                    {
                        evidence.push(p.id.clone());
                        if let Some(v) = o
                            .response
                            .zip(p.response)
                            .filter(|(_, r)| *r > 0.0)
                            .map(|(b, r)| b / r * 100.0)
                        {
                            ratios.push(v);
                        }
                    }
                }
                (
                    ratios.iter().copied().reduce(f64::max),
                    ratios.len(),
                    "%",
                    "max(100 * selected blank response / immediately preceding injection response)",
                )
            }
            Metric::Recovery
            | Metric::MatrixFactor
            | Metric::ProcessEfficiency
            | Metric::Selectivity
            | Metric::Stability => {
                evidence.extend(refs.iter().map(|o| o.id.clone()));
                let a: Vec<_> = obs.iter().filter_map(|o| o.response).collect();
                let b: Vec<_> = refs.iter().filter_map(|o| o.response).collect();
                let distinct: HashSet<_> = obs
                    .iter()
                    .chain(refs.iter())
                    .filter_map(|o| o.nominal.map(f64::to_bits))
                    .collect();
                let matched = matches!(rule.metric, Metric::Selectivity)
                    || (distinct.len() == 1
                        && obs
                            .iter()
                            .chain(refs.iter())
                            .all(|o| o.nominal.is_some_and(|v| v > 0.0)));
                let independent = !obs.iter().any(|o| refs.iter().any(|r| r.id == o.id));
                let (test_preparation, reference_preparation) = match rule.metric {
                    Metric::Recovery => (Preparation::PreExtraction, Preparation::PostExtraction),
                    Metric::MatrixFactor => (Preparation::PostExtraction, Preparation::Neat),
                    Metric::ProcessEfficiency => (Preparation::PreExtraction, Preparation::Neat),
                    Metric::Selectivity => (Preparation::Blank, Preparation::Neat),
                    _ => (Preparation::Stored, Preparation::Fresh),
                };
                let designed = obs
                    .iter()
                    .all(|o| o.preparation.as_ref() == Some(&test_preparation))
                    && refs
                        .iter()
                        .all(|o| o.preparation.as_ref() == Some(&reference_preparation));
                if !matched {
                    issues.push(
                        "Test and reference require the same positive nominal concentration".into(),
                    );
                }
                if !independent {
                    issues.push("Test and reference groups overlap".into());
                }
                if !designed {
                    issues.push(format!("Expected preparations: test={test_preparation:?}, reference={reference_preparation:?}"));
                }
                if refs.is_empty() {
                    issues.push("Reference group has no observations".into());
                }
                if mean(&b).is_none_or(|v| v <= 0.0) {
                    issues.push("Reference response mean must be available and positive".into());
                }
                let value = mean(&a)
                    .zip(mean(&b))
                    .filter(|(_, b)| *b > 0.0 && matched && independent && designed)
                    .map(|(a, b)| 100.0 * a / b);
                (value,a.len().min(b.len()),"%","100 * mean(test response) / mean(explicit reference-group response); matched concentration/preparation required")
            }
        };
        let complete = match rule.metric {
            Metric::ControlChart => {
                obs.iter().all(|o| o.value.is_some())
                    && refs.iter().all(|o| o.value.is_some())
                    && !obs.iter().any(|o| refs.iter().any(|r| r.id == o.id))
            }
            Metric::Missingness => true,
            Metric::Recovery
            | Metric::MatrixFactor
            | Metric::ProcessEfficiency
            | Metric::Selectivity
            | Metric::Stability => {
                refs.iter().all(|o| o.response.is_some())
                    && obs.iter().all(|o| o.response.is_some())
            }
            _ => n == obs.len(),
        };
        let value = value.filter(|v| v.is_finite());
        if n < rule.minimum_n {
            issues.push(format!("n={n} is below minimum_n={}", rule.minimum_n));
        }
        if !complete {
            issues.push(
                "Selected or reference observations have unavailable required measurements".into(),
            );
        }
        if value.is_none() && issues.is_empty() {
            issues.push(match rule.metric {
            Metric::Precision|Metric::InternalStandardCv=>"CV requires at least two finite measurements and a positive mean",
            Metric::Accuracy|Metric::MaximumAccuracyDeviation|Metric::DilutionIntegrity=>"Accuracy requires measured values and positive nominal concentrations",
            Metric::ControlChart=>"Control chart requires independent reference observations with positive sample SD",
            Metric::OrderSlope|Metric::RtDriftSlope|Metric::MassDriftSlope|Metric::InternalStandardSlope=>"Slope requires at least two observations with distinct injection order in one batch",
            Metric::Carryover=>"Carryover requires blanks with a preceding injection and positive predecessor response in the same batch",
            _=>"Calculation unavailable or numerical overflow; inspect retained observations",
        }.into());
        }
        let status = if value.is_none() || n < rule.minimum_n || !complete {
            Status::Indeterminate
        } else if value.is_some_and(|v| v >= rule.lower && v <= rule.upper) {
            Status::Pass
        } else {
            Status::Fail
        };
        evidence.sort();
        evidence.dedup();
        let reason = match status {
            Status::Indeterminate => issues.join("; "),
            Status::Pass => format!("Value within inclusive [{}, {}]", rule.lower, rule.upper),
            Status::Fail => format!("Value outside inclusive [{}, {}]", rule.lower, rule.upper),
        };
        decisions.push(Decision {
            rule: rule.clone(),
            status,
            value,
            unit: unit.into(),
            calculation: formula.into(),
            reason,
            evidence,
            n,
            statistics: components,
            calibration,
        });
    }
    let required: Vec<_> = decisions.iter().filter(|d| d.rule.required).collect();
    let status = if required.iter().any(|d| d.status == Status::Fail) {
        Status::Fail
    } else if required.is_empty() || required.iter().any(|d| d.status == Status::Indeterminate) {
        Status::Indeterminate
    } else {
        Status::Pass
    };
    let review_queue = decisions
        .iter()
        .filter(|d| d.status != Status::Pass)
        .map(|d| d.rule.id.clone())
        .collect();
    let passed = required.iter().filter(|d| d.status == Status::Pass).count();
    let failed = required.iter().filter(|d| d.status == Status::Fail).count();
    let indeterminate = required
        .iter()
        .filter(|d| d.status == Status::Indeterminate)
        .count();
    let acceptance=BatchAcceptance{
        required_rules:required.iter().map(|d|d.rule.id.clone()).collect(),passed,failed,indeterminate,
        threshold:"All required rules must pass; at least one required rule".into(),
        calculation:"Fail if required failures > 0; else Indeterminate if required count = 0 or unavailable required checks > 0; else Pass".into(),
        reason:format!("{} required checks: {passed} passed, {failed} failed, {indeterminate} indeterminate; optional checks do not gate acceptance",required.len()),
    };
    Ok(Report {
        schema_version: 1,
        report_id: uuid::Uuid::new_v4(),
        created_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        study,
        status,
        decisions,
        review_queue,
        reviews: Vec::new(),
        acceptance,
    })
}
pub fn verify(report: &Report) -> Result<()> {
    if report.report_id.is_nil() || report.created_unix_ms == 0 {
        return Err(EngineError::new(
            "corrupt_result",
            "QC report lacks identity/time",
        ));
    }
    let mut replay = evaluate(report.study.clone())?;
    let mut ids = HashSet::new();
    for review in &report.reviews {
        if !ids.insert(review.id)
            || review.actor.trim().is_empty()
            || review.reason.trim().is_empty()
            || review.unix_ms == 0
            || !replay.decisions.iter().any(|d| d.rule.id == review.rule_id)
        {
            return Err(EngineError::new(
                "corrupt_result",
                "Invalid QC review history",
            ));
        }
        update_queue(&mut replay, &review.rule_id, review.acknowledged);
    }
    if replay.schema_version != report.schema_version
        || replay.status != report.status
        || replay.decisions != report.decisions
        || replay.review_queue != report.review_queue
        || replay.acceptance != report.acceptance
    {
        return Err(EngineError::new(
            "corrupt_result",
            "QC report differs from replayed evidence",
        ));
    }
    Ok(())
}
fn update_queue(report: &mut Report, id: &str, acknowledged: bool) {
    report.review_queue.retain(|r| r != id);
    if !acknowledged
        && report
            .decisions
            .iter()
            .any(|d| d.rule.id == id && d.status != Status::Pass)
    {
        report.review_queue.push(id.into());
    }
    // Keep queue in rule declaration order, independent of review event order.
    report
        .review_queue
        .sort_by_key(|id| report.decisions.iter().position(|d| &d.rule.id == id));
}
pub fn review(
    report: &Report,
    expected_revision: usize,
    rule_id: &str,
    actor: &str,
    reason: &str,
    acknowledged: bool,
) -> Result<Report> {
    verify(report)?;
    if expected_revision != report.reviews.len() {
        return Err(EngineError::new(
            "stale_revision",
            "QC review revision changed",
        ));
    }
    if actor.trim().is_empty()
        || reason.trim().is_empty()
        || !report.decisions.iter().any(|d| d.rule.id == rule_id)
    {
        return Err(invalid("Review requires actor, reason and known rule"));
    }
    let mut next = report.clone();
    next.reviews.push(Review {
        id: uuid::Uuid::new_v4(),
        unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        rule_id: rule_id.into(),
        actor: actor.into(),
        reason: reason.into(),
        acknowledged,
    });
    update_queue(&mut next, rule_id, acknowledged);
    Ok(next)
}
pub fn csv(report: &Report) -> Result<String> {
    verify(report)?;
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out="rule,target,metric,status,value,unit,lower,upper,n,required,calculation,reason,evidence_json,statistics_json,observations_json,batch_status,review_pending,rule_json,calibration_json,report_id\n".to_string();
    for d in &report.decisions {
        out.push_str(&format!(
            "{},{},{:?},{:?},{},{},{},{},{},{},{},{},{},{},{},{:?},{},{},{},{}\n",
            quote(&d.rule.id),
            quote(&d.rule.target),
            d.rule.metric,
            d.status,
            d.value.map(|x| x.to_string()).unwrap_or_default(),
            quote(&d.unit),
            d.rule.lower,
            d.rule.upper,
            d.n,
            d.rule.required,
            quote(&d.calculation),
            quote(&d.reason),
            quote(&serde_json::to_string(&d.evidence).unwrap()),
            quote(&serde_json::to_string(&d.statistics).unwrap()),
            quote(
                &serde_json::to_string(
                    &report
                        .study
                        .observations
                        .iter()
                        .filter(|o| d.evidence.contains(&o.id))
                        .collect::<Vec<_>>()
                )
                .unwrap()
            ),
            report.status,
            report.review_queue.contains(&d.rule.id),
            quote(&serde_json::to_string(&d.rule).unwrap()),
            quote(&serde_json::to_string(&d.calibration).unwrap()),
            report.report_id,
        ));
    }
    Ok(out)
}
/// Convert verified targeted evidence without inventing mass errors or study design.
pub fn targeted_study(batch: &crate::targeted::BatchResult, rules: Vec<Rule>) -> Result<Study> {
    crate::targeted::verify(batch)?;
    let mut observations = Vec::new();
    for sample in &batch.request.samples {
        for target in &batch.request.targets {
            let result = batch
                .results
                .iter()
                .find(|r| r.sample == sample.id && r.target == target.id);
            let peak = batch
                .observations
                .iter()
                .find(|o| o.sample == sample.id && o.target == target.id)
                .and_then(|o| o.quantifier.peak.as_ref());
            let is_response = target
                .internal_standard
                .as_ref()
                .and_then(|id| {
                    batch
                        .observations
                        .iter()
                        .find(|o| o.sample == sample.id && &o.target == id)
                })
                .and_then(|o| o.quantifier.peak.as_ref())
                .map(|p| p.area);
            let standard = batch
                .calibrations
                .get(&target.id)
                .and_then(|c| c.points.iter().find(|p| p.sample == sample.id));
            observations.push(Observation {
                id: format!("{}/{}", sample.id, target.id),
                target: target.id.clone(),
                batch: batch.batch_id.to_string(),
                group: sample
                    .nominal
                    .get(&target.id)
                    .map(|v| format!("level:{v}"))
                    .unwrap_or_else(|| "unassigned".into()),
                role: match sample.role {
                    crate::targeted::Role::Standard => "standard",
                    crate::targeted::Role::Blank => "blank",
                    crate::targeted::Role::Qc => "qc",
                    crate::targeted::Role::Unknown => "unknown",
                }
                .into(),
                order: sample.injection_order,
                unit: target
                    .calibration
                    .as_ref()
                    .map(|c| c.unit.label())
                    .unwrap_or("instrument intensity * minute")
                    .into(),
                response_unit: if target.internal_standard.is_some() {
                    "dimensionless area ratio"
                } else {
                    "instrument intensity * minute"
                }
                .into(),
                value: if sample.role == crate::targeted::Role::Standard {
                    standard.and_then(|p| p.back_calculated)
                } else if target.calibration.is_none() {
                    peak.map(|p| p.area)
                } else {
                    result
                        .filter(|r| r.state == crate::targeted::State::Present)
                        .and_then(|r| r.injected_concentration)
                },
                nominal: sample.nominal.get(&target.id).copied(),
                response: if target.calibration.is_none() {
                    peak.map(|p| p.area)
                } else {
                    result.and_then(|r| r.response)
                },
                is_response,
                rt_minutes: peak.map(|p| p.apex_rt),
                expected_rt_minutes: Some(target.quantifier.extraction.expected_rt),
                mass_error_ppm: None,
                calibration_accepted: if sample.role == crate::targeted::Role::Standard {
                    Some(standard.is_some_and(|p| {
                        p.back_calculated.is_some()
                            && p.flags.iter().all(|f| f == "boundary_roundoff")
                    }))
                } else {
                    None
                },
                source: format!(
                    "targeted:{};source_sha256:{};review_revision:{}",
                    batch.batch_id,
                    batch
                        .source_hashes
                        .get(&sample.id)
                        .map(String::as_str)
                        .unwrap_or("unavailable"),
                    batch.reviews.len()
                ),
                preparation: if sample.role == crate::targeted::Role::Blank {
                    Some(Preparation::Blank)
                } else {
                    None
                },
            });
        }
    }
    Ok(Study {
        version: 1,
        method: batch.request.name.clone(),
        purpose: "batch_qc".into(),
        observations,
        rules,
        targeted_evidence: Some(Box::new(batch.clone())),
    })
}
/// Defaults are copied from the retained assay, never from a universal framework.
pub fn method_rules(batch: &crate::targeted::BatchResult) -> Vec<Rule> {
    let mut rules = Vec::new();
    for target in &batch.request.targets {
        let Some(config) = &target.calibration else {
            continue;
        };
        let base = Rule {
            id: target.id.clone(),
            target: target.id.clone(),
            group: None,
            role: None,
            batch: None,
            metric: Metric::Accuracy,
            lower: 0.0,
            upper: 0.0,
            minimum_n: 1,
            required: true,
            reference_group: None,
            calibration: None,
        };
        for role in ["standard", "qc"] {
            let mut accuracy = base.clone();
            accuracy.id = format!("{}-{role}-individual-accuracy", target.id);
            accuracy.role = Some(role.into());
            accuracy.metric = Metric::MaximumAccuracyDeviation;
            accuracy.upper = config.accuracy_tolerance_percent;
            rules.push(accuracy);
        }
        let mut blank = base.clone();
        blank.id = format!("{}-blank", target.id);
        blank.role = Some("blank".into());
        blank.metric = Metric::Blank;
        blank.lower = -f64::MAX;
        blank.upper = config.blank_response_limit;
        rules.push(blank);
        let levels: std::collections::BTreeSet<_> = batch
            .request
            .samples
            .iter()
            .filter(|s| s.role == crate::targeted::Role::Qc)
            .filter_map(|s| s.nominal.get(&target.id).map(|v| format!("level:{v}")))
            .collect();
        for level in levels {
            let mut precision = base.clone();
            precision.id = format!("{}-qc-precision-{level}", target.id);
            precision.role = Some("qc".into());
            precision.group = Some(level);
            precision.metric = Metric::Precision;
            precision.minimum_n = 2;
            precision.upper = config.qc_cv_limit_percent;
            rules.push(precision);
        }
    }
    rules
}
