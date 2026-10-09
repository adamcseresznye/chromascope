//! Frozen quantitative tables and explicit, replayable local statistical analysis.
use crate::domain::{EngineError, Result};
use crate::jobs::JobControl;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub id: String,
    pub metadata: BTreeMap<String, String>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feature {
    pub id: String,
    pub unit: String,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub samples: Vec<Sample>,
    pub features: Vec<Feature>,
    /// Sample-major original values; null means unavailable, never zero.
    pub values: Vec<Vec<Option<f64>>>,
    pub provenance: serde_json::Value,
    #[serde(default)]
    pub matrix: Option<Box<crate::untargeted::Report>>,
    #[serde(default)]
    pub targeted: Option<serde_json::Value>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Groups {
    pub metadata_key: String,
    pub reference: String,
    pub comparison: String,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub excluded_samples: BTreeMap<String, String>,
    pub excluded_features: BTreeMap<String, String>,
    pub metadata_equals: BTreeMap<String, String>,
    pub max_missing_fraction: f64,
    pub missing: String,
    pub normalization: String,
    pub internal_standard: Option<String>,
    pub transform: String,
    pub pseudocount: f64,
    pub scaling: String,
    pub components: usize,
    pub linkage: String,
    pub groups: Option<Groups>,
    pub confidence: f64,
    pub fdr: String,
    pub timeout_seconds: u64,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            excluded_samples: BTreeMap::new(),
            excluded_features: BTreeMap::new(),
            metadata_equals: BTreeMap::new(),
            max_missing_fraction: 1.,
            missing: "reject".into(),
            normalization: "none".into(),
            internal_standard: None,
            transform: "none".into(),
            pseudocount: 0.,
            scaling: "center".into(),
            components: 2,
            linkage: "average".into(),
            groups: None,
            confidence: 0.95,
            fdr: "bh".into(),
            timeout_seconds: 120,
        }
    }
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pca {
    pub scores: Vec<Vec<f64>>,
    pub loadings: Vec<Vec<f64>>,
    pub variance: Vec<f64>,
    pub variance_ratio: Vec<f64>,
    pub state: String,
    pub center: Vec<f64>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Comparison {
    #[serde(default)]
    pub warnings: Vec<String>,
    pub feature_id: String,
    pub n_reference: usize,
    pub n_comparison: usize,
    pub state: String,
    pub mean_reference: Option<f64>,
    pub mean_comparison: Option<f64>,
    pub difference: Option<f64>,
    pub ci_low: Option<f64>,
    pub ci_high: Option<f64>,
    pub cohen_d: Option<f64>,
    pub log2_fold_change: Option<f64>,
    pub t: Option<f64>,
    pub df: Option<f64>,
    pub p: Option<f64>,
    pub q: Option<f64>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Numerics {
    pub python_version: String,
    pub group_membership: Option<serde_json::Value>,
    pub adapter_version: String,
    pub numpy_version: String,
    pub scipy_version: String,
    pub sample_indices: Vec<usize>,
    pub feature_indices: Vec<usize>,
    pub excluded_samples: Vec<serde_json::Value>,
    pub excluded_features: Vec<serde_json::Value>,
    pub imputed: Vec<serde_json::Value>,
    pub normalization_factors: Vec<f64>,
    pub center: Vec<f64>,
    pub scale: Vec<f64>,
    pub processed: Vec<Vec<f64>>,
    pub pca: Pca,
    pub sample_linkage: Vec<[f64; 4]>,
    pub feature_linkage: Vec<[f64; 4]>,
    pub sample_order: Vec<usize>,
    pub feature_order: Vec<usize>,
    pub comparisons: Vec<Comparison>,
    pub inference_scale: String,
    pub fdr_family_size: usize,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub version: u32,
    pub table: Table,
    pub settings: Settings,
    pub numerics: Numerics,
}
fn invalid(s: impl ToString) -> EngineError {
    EngineError::new("invalid_parameters", s)
}
pub fn validate(table: &Table, s: &Settings) -> Result<()> {
    use std::collections::HashSet;
    let sample_ids: HashSet<_> = table.samples.iter().map(|v| &v.id).collect();
    let feature_ids: HashSet<_> = table.features.iter().map(|v| &v.id).collect();
    if !(2..=256).contains(&table.samples.len()) || !(1..=2048).contains(&table.features.len()) {
        return Err(EngineError::new(
            "resource_limit",
            "Use 2â€“256 samples and 1â€“2048 features; clustering is quadratic",
        ));
    }
    if sample_ids.len() != table.samples.len()
        || feature_ids.len() != table.features.len()
        || table.samples.iter().any(|v| v.id.trim().is_empty())
        || table
            .features
            .iter()
            .any(|v| v.id.trim().is_empty() || v.unit.trim().is_empty())
        || table.values.len() != table.samples.len()
        || table.values.iter().any(|r| {
            r.len() != table.features.len() || r.iter().flatten().any(|v| !v.is_finite() || *v < 0.)
        })
    {
        return Err(invalid("Unique nonempty IDs, explicit units, aligned rows and finite nonnegative quantities required"));
    }
    if s.excluded_samples
        .iter()
        .any(|(id, r)| !sample_ids.contains(id) || r.trim().is_empty())
        || s.excluded_features
            .iter()
            .any(|(id, r)| !feature_ids.contains(id) || r.trim().is_empty())
        || s.metadata_equals.keys().any(|k| k.trim().is_empty())
    {
        return Err(invalid(
            "Exclusions require existing IDs and reasons; metadata keys must be nonempty",
        ));
    }
    if !["reject", "median", "half_minimum", "complete_features"].contains(&s.missing.as_str())
        || !["none", "total", "median", "internal_standard"].contains(&s.normalization.as_str())
        || !["none", "log2", "log10", "sqrt"].contains(&s.transform.as_str())
        || !["none", "center", "autoscale", "pareto"].contains(&s.scaling.as_str())
        || !["average", "complete", "single", "ward"].contains(&s.linkage.as_str())
        || !["bh", "by"].contains(&s.fdr.as_str())
        || !s.max_missing_fraction.is_finite()
        || !(0. ..=1.).contains(&s.max_missing_fraction)
        || !s.pseudocount.is_finite()
        || s.pseudocount < 0.
        || !s.confidence.is_finite()
        || s.confidence <= 0.
        || s.confidence >= 1.
        || !(1..=10).contains(&s.components)
        || !(1..=600).contains(&s.timeout_seconds)
    {
        return Err(invalid(
            "Unsupported statistical setting or out-of-range parameter",
        ));
    }
    if s.normalization == "internal_standard"
        && !s
            .internal_standard
            .as_ref()
            .is_some_and(|id| feature_ids.contains(id))
    {
        return Err(invalid("Select an existing internal-standard feature"));
    }
    if s.normalization == "total"
        && table
            .features
            .iter()
            .any(|f| f.unit != table.features[0].unit)
    {
        return Err(invalid("Total normalization requires compatible units"));
    }
    if s.normalization == "median"
        && table
            .features
            .iter()
            .any(|f| f.unit != table.features[0].unit)
    {
        return Err(invalid("Median normalization requires compatible units"));
    }
    if let Some(g) = &s.groups {
        if g.metadata_key.trim().is_empty()
            || g.reference.is_empty()
            || g.comparison.is_empty()
            || g.reference == g.comparison
        {
            return Err(invalid(
                "Declare distinct groups using an explicit metadata key",
            ));
        }
    }
    if let Some(matrix) = &table.matrix {
        crate::untargeted::verify(matrix)?;
        let expected = from_matrix(matrix)?;
        if !source_matches(table, &expected)? {
            return Err(invalid("Table does not match retained source matrix"));
        }
    }
    if let Some(value) = &table.targeted {
        let batch: crate::targeted::BatchResult =
            serde_json::from_value(value.clone()).map_err(invalid)?;
        crate::targeted::verify(&batch)?;
        let expected = from_targeted(&batch)?;
        if table.matrix.is_some() || !source_matches(table, &expected)? {
            return Err(invalid("Table does not match retained targeted batch"));
        }
    }
    Ok(())
}
fn source_matches(table: &Table, expected: &Table) -> Result<bool> {
    Ok(table.samples.len() == expected.samples.len()
        && table.samples.iter().zip(&expected.samples).all(|(a, b)| {
            a.id == b.id && b.metadata.iter().all(|(k, v)| a.metadata.get(k) == Some(v))
        })
        && serde_json::to_value((&table.features, &table.values, &table.provenance))
            .map_err(invalid)?
            == serde_json::to_value((&expected.features, &expected.values, &expected.provenance))
                .map_err(invalid)?)
}
pub fn from_targeted(batch: &crate::targeted::BatchResult) -> Result<Table> {
    let features: Vec<_> = batch
        .request
        .targets
        .iter()
        .filter_map(|t| {
            t.calibration.as_ref().map(|c| Feature {
                id: t.id.clone(),
                unit: c.unit.label().into(),
            })
        })
        .collect();
    let samples: Vec<_> = batch
        .request
        .samples
        .iter()
        .map(|s| Sample {
            id: s.id.clone(),
            metadata: [
                ("role".into(), format!("{:?}", s.role).to_lowercase()),
                ("injection_order".into(), s.injection_order.to_string()),
            ]
            .into(),
        })
        .collect();
    let values = samples
        .iter()
        .map(|s| {
            features
                .iter()
                .map(|f| {
                    batch
                        .results
                        .iter()
                        .find(|r| r.sample == s.id && r.target == f.id)
                        .and_then(|r| {
                            if r.state == crate::targeted::State::Present {
                                r.concentration
                            } else {
                                None
                            }
                        })
                })
                .collect()
        })
        .collect();
    Ok(Table {
        samples,
        features,
        values,
        provenance: serde_json::json!({"batch_id":batch.batch_id,"source_hashes":batch.source_hashes,"quantity":"dilution-corrected concentration; present values only; review flags retained"}),
        matrix: None,
        targeted: Some(serde_json::to_value(batch).map_err(invalid)?),
    })
}
pub fn from_matrix(matrix: &crate::untargeted::Report) -> Result<Table> {
    let indices: Vec<_> = matrix
        .features
        .iter()
        .enumerate()
        .filter(|(_, f)| f.filter_state == crate::untargeted::FilterState::Included)
        .map(|(i, _)| i)
        .collect();
    Ok(Table {
        samples: matrix
            .config
            .samples
            .iter()
            .map(|s| Sample {
                id: s.id.clone(),
                metadata: s
                    .metadata
                    .as_object()
                    .map(|m| {
                        m.iter()
                            .filter_map(|(k, v)| {
                                if v.is_string() {
                                    v.as_str().map(|s| (k.clone(), s.into()))
                                } else if v.is_number() || v.is_boolean() {
                                    Some((k.clone(), v.to_string()))
                                } else {
                                    None
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect(),
        features: indices
            .iter()
            .map(|&j| Feature {
                id: matrix.features[j].id.clone(),
                unit: "intensity*seconds".into(),
            })
            .collect(),
        values: matrix
            .config
            .samples
            .iter()
            .map(|s| {
                indices
                    .iter()
                    .map(|&j| {
                        matrix.features[j]
                            .cells
                            .iter()
                            .find(|c| c.sample_id == s.id)
                            .and_then(|c| c.intensity)
                    })
                    .collect()
            })
            .collect(),
        provenance: serde_json::json!({"source_hashes":matrix.source_hashes,"matrix_provenance":matrix.provenance,"selection":"included features only; original matrix retained"}),
        matrix: Some(Box::new(matrix.clone())),
        targeted: None,
    })
}
fn adapter_io(error: std::io::Error) -> EngineError {
    EngineError::new(
        "adapter_failure",
        format!("Statistics adapter I/O failed: {error}"),
    )
}
pub fn analyze(table: &Table, settings: &Settings, control: &JobControl) -> Result<Report> {
    validate(table, settings)?;
    control.check()?;
    let dir = tempfile::tempdir().map_err(adapter_io)?;
    let input = dir.path().join("input.json");
    let output = dir.path().join("output.json");
    let errors = dir.path().join("stderr.txt");
    fs::write(
        &input,
        serde_json::to_vec(&serde_json::json!({"table":{"samples":table.samples,"features":table.features,"values":table.values},"settings":settings}))
            .map_err(invalid)?,
    )
    .map_err(adapter_io)?;
    let python = std::env::var_os("CHROMASCOPE_STATS_PYTHON").unwrap_or_else(|| "python".into());
    let mut child = Command::new(python)
        .args(["-E", "-P", "-c", include_str!("adapters/statistics.py")])
        .env("OPENBLAS_NUM_THREADS", "1")
        .env("OMP_NUM_THREADS", "1")
        .stdin(Stdio::from(fs::File::open(input).map_err(adapter_io)?))
        .stdout(Stdio::from(fs::File::create(&output).map_err(adapter_io)?))
        .stderr(Stdio::from(fs::File::create(&errors).map_err(adapter_io)?))
        .spawn()
        .map_err(|e| {
            EngineError::new(
                "unsupported_capability",
                format!("Local Python with NumPy and SciPy required: {e}"),
            )
        })?;
    let start = Instant::now();
    let status = loop {
        if let Err(e) = control.check() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        if start.elapsed() > Duration::from_secs(settings.timeout_seconds) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(EngineError::new(
                "resource_limit",
                "Statistics adapter timed out",
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(EngineError::new("adapter_failure", e));
            }
        }
    };
    if fs::metadata(&output).map_err(adapter_io)?.len() > 128 * 1024 * 1024 {
        return Err(EngineError::new(
            "resource_limit",
            "Statistics output exceeds 128 MiB",
        ));
    }
    let bytes = fs::read(output).map_err(adapter_io)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
        EngineError::new(
            "adapter_failure",
            format!(
                "Invalid adapter output: {e}; {}",
                fs::read_to_string(errors)
                    .unwrap_or_default()
                    .chars()
                    .take(2000)
                    .collect::<String>()
            ),
        )
    })?;
    if !status.success() {
        return Err(EngineError::new(
            if value["code"] == "unsupported_capability" {
                "unsupported_capability"
            } else {
                "adapter_failure"
            },
            value
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Scientific adapter failed"),
        ));
    }
    let numerics: Numerics = serde_json::from_value(value["result"].clone()).map_err(invalid)?;
    Ok(Report {
        version: 1,
        table: table.clone(),
        settings: settings.clone(),
        numerics,
    })
}
pub fn verify(report: &Report) -> Result<()> {
    if report.version != 1 {
        return Err(invalid("Unsupported statistics schema"));
    }
    let replay = analyze(&report.table, &report.settings, &JobControl::default())?;
    if serde_json::to_value(&replay).map_err(invalid)?
        != serde_json::to_value(report).map_err(invalid)?
    {
        return Err(EngineError::new(
            "corrupt_project",
            "Statistics replay differs; retain the recorded Python/library environment",
        ));
    }
    Ok(())
}
pub fn verify_response(response: &crate::engine::Response) -> Result<()> {
    match (&response.request.operation, &response.output) {
        (
            crate::domain::Operation::AnalyzeStatistics { table, settings },
            crate::engine::Output::Statistics { report },
        ) => {
            if serde_json::to_value((table, settings)).map_err(invalid)?
                != serde_json::to_value((&report.table, &report.settings)).map_err(invalid)?
            {
                return Err(invalid("Statistics request/report mismatch"));
            }
            verify(report)
        }
        (
            crate::domain::Operation::ExportStatistics { report },
            crate::engine::Output::StatisticsTable { csv },
        ) => {
            if &export_csv(report)? != csv {
                return Err(invalid("Statistics export mismatch"));
            }
            Ok(())
        }
        _ => Err(invalid("Statistics operation/output mismatch")),
    }
}
pub fn export_csv(report: &Report) -> Result<String> {
    verify(report)?;
    fn quote(s: &str) -> String {
        format!("\"{}\"", s.replace('"', "\"\""))
    }
    fn number(n: Option<f64>) -> String {
        n.map(|v| v.to_string()).unwrap_or_default()
    }
    let mut csv="feature,original_unit,inference_scale,state,warnings,n_reference,n_comparison,mean_reference,mean_comparison,difference,ci_low,ci_high,cohen_d,log2_fold_change,t,df,p,q,settings,samples,original_values,processed_values,provenance\n".to_string();
    let settings = quote(&serde_json::to_string(&report.settings).map_err(invalid)?);
    let samples = quote(&serde_json::to_string(&report.table.samples).map_err(invalid)?);
    let provenance = quote(&serde_json::to_string(&report.table.provenance).map_err(invalid)?);
    for (j, &original) in report.numerics.feature_indices.iter().enumerate() {
        let id = &report.table.features[original].id;
        let descriptive = Comparison {
            feature_id: id.clone(),
            state: "descriptive_only".into(),
            ..Default::default()
        };
        let r = report
            .numerics
            .comparisons
            .iter()
            .find(|r| &r.feature_id == id)
            .unwrap_or(&descriptive);
        let unit = &report
            .table
            .features
            .iter()
            .find(|f| f.id == r.feature_id)
            .ok_or_else(|| invalid("Unknown feature"))?
            .unit;
        csv.push_str(
            &[
                quote(&r.feature_id),
                quote(unit),
                quote(&report.numerics.inference_scale),
                quote(&r.state),
                quote(&serde_json::to_string(&r.warnings).map_err(invalid)?),
                r.n_reference.to_string(),
                r.n_comparison.to_string(),
                number(r.mean_reference),
                number(r.mean_comparison),
                number(r.difference),
                number(r.ci_low),
                number(r.ci_high),
                number(r.cohen_d),
                number(r.log2_fold_change),
                number(r.t),
                number(r.df),
                number(r.p),
                number(r.q),
                settings.clone(),
                samples.clone(),
                quote(
                    &serde_json::to_string(
                        &report
                            .table
                            .values
                            .iter()
                            .map(|row| row[original])
                            .collect::<Vec<_>>(),
                    )
                    .map_err(invalid)?,
                ),
                quote(
                    &serde_json::to_string(
                        &report
                            .numerics
                            .processed
                            .iter()
                            .map(|row| row[j])
                            .collect::<Vec<_>>(),
                    )
                    .map_err(invalid)?,
                ),
                provenance.clone(),
            ]
            .join(","),
        );
        csv.push('\n');
    }
    Ok(csv)
}

#[cfg(test)]
mod reliability_tests {
    #[test]
    fn exhausted_storage_is_an_adapter_failure_not_invalid_parameters() {
        let error = super::adapter_io(std::io::Error::new(
            std::io::ErrorKind::StorageFull,
            "reference storage exhaustion",
        ));
        assert_eq!(error.code, "adapter_failure");
        assert!(error.message.contains("reference storage exhaustion"));
    }
}
