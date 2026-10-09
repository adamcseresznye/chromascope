//! Versioned local OpenMS subprocess adapter. Raw sources are never modified.
use crate::{
    domain::{EngineError, Result},
    jobs::JobControl,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub const ADAPTER_VERSION: &str = "openms-metabo-v1";
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub id: String,
    pub source: String,
    pub role: Role,
    #[serde(default)]
    pub metadata: serde_json::Value,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Sample,
    Blank,
    Qc,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub samples: Vec<Sample>,
    pub polarity: crate::spectral::Polarity,
    pub cache_directory: String,
    pub detection_ppm: f64,
    pub correspondence_ppm: f64,
    pub rt_tolerance_seconds: f64,
    pub noise_intensity: f64,
    pub min_trace_seconds: f64,
    pub min_trace_sample_rate: f64,
    pub peak_fwhm_seconds: f64,
    pub min_fwhm_seconds: f64,
    pub max_fwhm_seconds: f64,
    pub max_charge: i32,
    pub centroid_profile: bool,
    pub gap_fill: bool,
    pub gap_min_scans: usize,
    pub blank_ratio: f64,
    pub max_qc_cv: f64,
    pub min_sample_fraction: f64,
    pub timeout_seconds: u64,
    pub max_matrix_cells: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            samples: vec![],
            polarity: crate::spectral::Polarity::Positive,
            cache_directory: String::new(),
            detection_ppm: 10.,
            correspondence_ppm: 10.,
            rt_tolerance_seconds: 15.,
            noise_intensity: 1000.,
            min_trace_seconds: 5.,
            min_trace_sample_rate: 0.5,
            peak_fwhm_seconds: 5.,
            min_fwhm_seconds: 1.,
            max_fwhm_seconds: 60.,
            max_charge: 3,
            centroid_profile: false,
            gap_fill: false,
            gap_min_scans: 3,
            blank_ratio: 5.,
            max_qc_cv: 0.3,
            min_sample_fraction: 0.5,
            timeout_seconds: 86400,
            max_matrix_cells: 1_000_000,
        }
    }
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MissingState {
    Detected,
    GapFilled,
    NotDetected,
    BelowThreshold,
    OutsideAcquisition,
    GapFillDisabled,
    AlignmentUnavailable,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cell {
    pub sample_id: String,
    pub state: MissingState,
    pub intensity: Option<f64>,
    pub openms_intensity: Option<f64>,
    pub raw_rt_seconds: Option<f64>,
    pub aligned_rt_seconds: Option<f64>,
    pub raw_bounds_seconds: Option<[f64; 2]>,
    pub mz: Option<f64>,
    pub charge: Option<i32>,
    pub isotope_mz: Vec<f64>,
    pub adduct_group: Option<String>,
    pub adduct: Option<String>,
    pub eic: Vec<[f64; 2]>,
    pub apex_spectrum: Vec<[f64; 2]>,
    pub ms2: Vec<Ms2Evidence>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ms2Evidence {
    pub original_index: usize,
    pub native_id: String,
    pub raw_rt_seconds: f64,
    pub precursor_mz: f64,
    pub charge: i32,
    pub isolation_lower_offset_da: f64,
    pub isolation_upper_offset_da: f64,
    pub association: String,
    pub collision_energy_ev: Option<f64>,
    pub representation: String,
    pub peaks: Vec<[f64; 2]>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Alignment {
    pub sample_id: String,
    pub reference_sample_id: String,
    pub state: String,
    pub slope: Option<f64>,
    pub intercept_seconds: Option<f64>,
    #[serde(default)]
    pub landmarks_raw_reference_seconds: Vec<[f64; 2]>,
    #[serde(default)]
    pub residuals_seconds: Vec<f64>,
    pub rms_residual_seconds: Option<f64>,
    pub reason: Option<String>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Feature {
    pub id: String,
    pub mz: f64,
    pub aligned_rt_seconds: f64,
    pub cells: Vec<Cell>,
    pub flags: Vec<String>,
    pub blank_ratio: Option<f64>,
    pub qc_cv: Option<f64>,
    pub sample_fraction: Option<f64>,
    pub filter_state: FilterState,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FilterState {
    Included,
    Excluded,
    Indeterminate,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub version: u32,
    pub adapter_version: String,
    pub openms_version: String,
    pub config: Config,
    pub source_hashes: std::collections::BTreeMap<String, String>,
    pub features: Vec<Feature>,
    pub alignments: Vec<Alignment>,
    pub provenance: serde_json::Value,
    pub warnings: Vec<String>,
}
fn invalid(message: impl ToString) -> EngineError {
    EngineError::new("invalid_parameters", message)
}
fn fail(message: impl std::fmt::Display) -> EngineError {
    EngineError::new("adapter_failure", message)
}
pub fn validate(config: &Config) -> Result<()> {
    let mut ids = HashSet::new();
    if !(2..=256).contains(&config.samples.len())
        || config.samples.iter().any(|s| {
            s.id.trim().is_empty()
                || !ids.insert(&s.id)
                || !s.source.to_lowercase().ends_with(".mzml")
        })
    {
        return Err(invalid("Use 2-256 uniquely named mzML samples"));
    }
    if config.polarity == crate::spectral::Polarity::Unknown
        || config.cache_directory.trim().is_empty()
        || [
            config.detection_ppm,
            config.correspondence_ppm,
            config.rt_tolerance_seconds,
            config.noise_intensity,
            config.min_trace_seconds,
            config.peak_fwhm_seconds,
            config.min_fwhm_seconds,
            config.max_fwhm_seconds,
            config.blank_ratio,
            config.max_qc_cv,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v <= 0.)
        || !config.min_sample_fraction.is_finite()
        || !config.min_trace_sample_rate.is_finite()
        || config.min_trace_sample_rate <= 0.0
        || config.min_trace_sample_rate > 1.0
        || !(0.0..=1.0).contains(&config.min_sample_fraction)
        || config.min_fwhm_seconds > config.max_fwhm_seconds
        || !(1..=3).contains(&config.max_charge)
        || !(2..=1000).contains(&config.gap_min_scans)
        || !(1..=604800).contains(&config.timeout_seconds)
        || !(1..=10_000_000).contains(&config.max_matrix_cells)
    {
        return Err(invalid(
            "Invalid untargeted units, bounds, polarity, cache path or thresholds",
        ));
    }
    Ok(())
}
pub fn verify(report: &Report) -> Result<()> {
    validate(&report.config)?;
    let bad = || {
        EngineError::new(
            "corrupt_result",
            "Invalid untargeted matrix shape, values, states or provenance",
        )
    };
    if report.version != 1
        || report.adapter_version != ADAPTER_VERSION
        || report.openms_version != "3.5.0"
        || report.source_hashes.len() != report.config.samples.len()
        || report.alignments.len() != report.config.samples.len()
        || report
            .features
            .len()
            .saturating_mul(report.config.samples.len())
            > report.config.max_matrix_cells
    {
        return Err(bad());
    }
    for (alignment, sample) in report.alignments.iter().zip(&report.config.samples) {
        if alignment.sample_id != sample.id
            || !report
                .config
                .samples
                .iter()
                .any(|s| s.id == alignment.reference_sample_id)
        {
            return Err(bad());
        }
        if alignment.state == "unavailable" {
            if sample.role != Role::Blank
                || alignment.slope.is_some()
                || alignment.intercept_seconds.is_some()
                || alignment.reason.as_ref().is_none_or(|s| s.is_empty())
            {
                return Err(bad());
            }
        } else {
            let slope = alignment.slope.ok_or_else(bad)?;
            let intercept = alignment.intercept_seconds.ok_or_else(bad)?;
            if !slope.is_finite()
                || slope <= 0.
                || !intercept.is_finite()
                || !["aligned", "reference"].contains(&alignment.state.as_str())
            {
                return Err(bad());
            }
            if alignment.state == "reference"
                && (slope != 1. || intercept != 0. || alignment.reference_sample_id != sample.id)
            {
                return Err(bad());
            }
            if alignment.state == "aligned" && alignment.landmarks_raw_reference_seconds.len() < 2 {
                return Err(bad());
            }
            if alignment.landmarks_raw_reference_seconds.len() != alignment.residuals_seconds.len()
            {
                return Err(bad());
            }
            for (p, residual) in alignment
                .landmarks_raw_reference_seconds
                .iter()
                .zip(&alignment.residuals_seconds)
            {
                if p.iter().any(|v| !v.is_finite())
                    || !residual.is_finite()
                    || !close(slope * p[0] + intercept - p[1], *residual)
                {
                    return Err(bad());
                }
            }
            if !alignment.residuals_seconds.is_empty() {
                let rms = (alignment
                    .residuals_seconds
                    .iter()
                    .map(|v| v * v)
                    .sum::<f64>()
                    / alignment.residuals_seconds.len() as f64)
                    .sqrt();
                if alignment
                    .rms_residual_seconds
                    .is_none_or(|v| !close(v, rms))
                {
                    return Err(bad());
                }
            }
        }
    }
    let mut ids = HashSet::new();
    for feature in &report.features {
        if !ids.insert(&feature.id)
            || !feature.mz.is_finite()
            || feature.mz <= 0.
            || !feature.aligned_rt_seconds.is_finite()
            || feature.cells.len() != report.config.samples.len()
        {
            return Err(bad());
        }
        for ((cell, sample), alignment) in feature
            .cells
            .iter()
            .zip(&report.config.samples)
            .zip(&report.alignments)
        {
            let present = matches!(cell.state, MissingState::Detected | MissingState::GapFilled);
            if cell.sample_id != sample.id
                || present != cell.intensity.is_some()
                || cell.intensity.is_some_and(|v| !v.is_finite() || v < 0.)
                || cell
                    .raw_rt_seconds
                    .is_some_and(|v| !v.is_finite() || v < 0.)
                || cell
                    .eic
                    .iter()
                    .chain(&cell.apex_spectrum)
                    .flatten()
                    .any(|v| !v.is_finite())
                || cell.eic.windows(2).any(|w| w[0][0] > w[1][0])
                || cell
                    .eic
                    .iter()
                    .chain(&cell.apex_spectrum)
                    .flatten()
                    .any(|v| *v < 0.)
                || cell.mz.is_some_and(|v| !v.is_finite() || v <= 0.)
                || cell.aligned_rt_seconds.is_some_and(|v| !v.is_finite())
                || cell
                    .openms_intensity
                    .is_some_and(|v| !v.is_finite() || v < 0.)
                || (cell.state == MissingState::Detected) != cell.openms_intensity.is_some()
                || cell.isotope_mz.iter().any(|v| !v.is_finite() || *v <= 0.)
            {
                return Err(bad());
            }
            if present {
                let area = cell
                    .eic
                    .windows(2)
                    .map(|w| (w[1][0] - w[0][0]) * (w[0][1] + w[1][1]) / 2.)
                    .sum::<f64>();
                if cell.eic.len() < 2
                    || !close(area, cell.intensity.unwrap())
                    || cell.raw_rt_seconds.is_none()
                    || cell.aligned_rt_seconds.is_none()
                    || cell.mz.is_none()
                {
                    return Err(bad());
                }
                if !close(
                    alignment.slope.ok_or_else(bad)? * cell.raw_rt_seconds.unwrap()
                        + alignment.intercept_seconds.ok_or_else(bad)?,
                    cell.aligned_rt_seconds.unwrap(),
                ) {
                    return Err(bad());
                }
            }
            if let Some([lo, hi]) = cell.raw_bounds_seconds {
                if !lo.is_finite()
                    || !hi.is_finite()
                    || lo < 0.
                    || lo >= hi
                    || cell.eic.iter().any(|p| p[0] < lo || p[0] > hi)
                {
                    return Err(bad());
                }
            } else if present {
                return Err(bad());
            }
            if cell.state == MissingState::AlignmentUnavailable && alignment.state != "unavailable"
            {
                return Err(bad());
            }
            if cell.state == MissingState::GapFilled && !report.config.gap_fill {
                return Err(bad());
            }
            for scan in &cell.ms2 {
                if scan.native_id.is_empty()
                    || scan.association != "precursor_mz_and_raw_rt_window"
                    || [
                        scan.raw_rt_seconds,
                        scan.precursor_mz,
                        scan.isolation_lower_offset_da,
                        scan.isolation_upper_offset_da,
                    ]
                    .iter()
                    .any(|v| !v.is_finite() || *v < 0.)
                    || scan
                        .collision_energy_ev
                        .is_some_and(|v| !v.is_finite() || v <= 0.)
                    || !["centroid", "profile", "unknown"].contains(&scan.representation.as_str())
                    || scan
                        .peaks
                        .iter()
                        .flatten()
                        .any(|v| !v.is_finite() || *v < 0.)
                {
                    return Err(bad());
                }
            }
        }
        verify_filters(feature, &report.config).map_err(|_| bad())?;
    }
    for sample in &report.config.samples {
        if !report
            .source_hashes
            .get(&sample.id)
            .is_some_and(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(bad());
        }
    }
    Ok(())
}
fn close(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-8 * (1. + a.abs().max(b.abs()))
}
fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n.is_multiple_of(2) {
        (values[n / 2 - 1] + values[n / 2]) / 2.
    } else {
        values[n / 2]
    }
}
fn verify_filters(feature: &Feature, config: &Config) -> Result<()> {
    let values = |role| {
        feature
            .cells
            .iter()
            .zip(&config.samples)
            .filter(|(_, s)| s.role == role)
            .filter_map(|(c, _)| c.intensity)
            .collect::<Vec<_>>()
    };
    let samples = values(Role::Sample);
    let blanks = values(Role::Blank);
    let qcs = values(Role::Qc);
    let mut flags = vec![];
    let total = config
        .samples
        .iter()
        .filter(|s| s.role == Role::Sample)
        .count();
    let fraction = if total > 0 {
        Some(samples.len() as f64 / total as f64)
    } else {
        None
    };
    let ratio = if !samples.is_empty() && !blanks.is_empty() {
        let blank = median(blanks);
        if blank > 0. {
            let r = median(samples) / blank;
            if r < config.blank_ratio {
                flags.push("blank_contamination");
            }
            Some(r)
        } else {
            flags.push("blank_zero_signal");
            None
        }
    } else {
        flags.push("blank_filter_indeterminate");
        None
    };
    let mean = qcs.iter().sum::<f64>() / qcs.len() as f64;
    let cv = if qcs.len() >= 3 && mean > 0. {
        let v = (qcs.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (qcs.len() - 1) as f64)
            .sqrt()
            / mean;
        if v > config.max_qc_cv {
            flags.push("qc_not_reproducible");
        }
        Some(v)
    } else {
        flags.push("qc_filter_indeterminate");
        None
    };
    if fraction.is_some_and(|v| v < config.min_sample_fraction) {
        flags.push("low_sample_prevalence");
    }
    let excluded = flags.iter().any(|f| {
        [
            "blank_contamination",
            "qc_not_reproducible",
            "low_sample_prevalence",
        ]
        .contains(f)
    });
    let expected = if excluded {
        FilterState::Excluded
    } else if flags.iter().any(|f| f.ends_with("indeterminate")) {
        FilterState::Indeterminate
    } else {
        FilterState::Included
    };
    let same = |a: Option<f64>, b: Option<f64>| match (a, b) {
        (Some(a), Some(b)) => close(a, b),
        (None, None) => true,
        _ => false,
    };
    if feature.flags.iter().map(String::as_str).collect::<Vec<_>>() != flags
        || feature.filter_state != expected
        || !same(feature.blank_ratio, ratio)
        || !same(feature.qc_cv, cv)
        || !same(feature.sample_fraction, fraction)
    {
        return Err(invalid("Filter evidence mismatch"));
    }
    Ok(())
}
pub fn run(config: &Config, control: &JobControl) -> Result<Report> {
    validate(config)?;
    control.check()?;
    fs::create_dir_all(&config.cache_directory).map_err(fail)?;
    let attempt = Path::new(&config.cache_directory).join(format!("run-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&attempt).map_err(fail)?;
    fs::write(attempt.join("launch.json"),serde_json::to_vec_pretty(&serde_json::json!({"adapter_version":ADAPTER_VERSION,"config":config,"max_source_bytes":control.max_source_bytes,"max_scans":control.max_scans,"python_executable":std::env::var_os("CHROMASCOPE_OPENMS_PYTHON").unwrap_or_else(||"python".into()).to_string_lossy(),"started_unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()})).map_err(fail)?).map_err(fail)?;
    let result = run_attempt(config, control, &attempt);
    let outcome = match &result {
        Ok(_) => {
            use sha2::{Digest, Sha256};
            let bytes = fs::read(attempt.join("report.json")).map_err(fail)?;
            serde_json::json!({"state":"succeeded","report_sha256":format!("{:x}",Sha256::digest(bytes))})
        }
        Err(error) => {
            serde_json::json!({"state":if error.code=="cancelled"{"cancelled"}else{"failed"},"error":error})
        }
    };
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(attempt.join("outcome.json"))
        .map_err(fail)?;
    file.write_all(&serde_json::to_vec_pretty(&outcome).map_err(fail)?)
        .map_err(fail)?;
    file.sync_all().map_err(fail)?;
    result
}
fn run_attempt(config: &Config, control: &JobControl, attempt: &Path) -> Result<Report> {
    let mut hashes = std::collections::BTreeMap::new();
    for sample in &config.samples {
        let path = Path::new(&sample.source);
        if fs::metadata(path)
            .map_err(|e| EngineError::new("missing_source", e))?
            .len()
            > control.max_source_bytes
        {
            return Err(EngineError::new(
                "resource_limit",
                "Sample exceeds source budget",
            ));
        }
        let identity = crate::project::source_identity_controlled(path, Some(control))?;
        // Native OpenMS must receive the same raw-unit boundary as extraction.
        // Validate even when a sample checkpoint exists; hashing alone cannot
        // establish the meaning of acquisition-time CV values.
        crate::source_validation::validate_mzml(path, Some(control)).map_err(fail)?;
        hashes.insert(sample.id.clone(), identity.0);
    }
    let request = serde_json::json!({"version":1,"adapter_version":ADAPTER_VERSION,"config":config,"source_hashes":hashes,"max_scans":control.max_scans});
    fs::write(
        attempt.join("request.json"),
        serde_json::to_vec_pretty(&request).map_err(fail)?,
    )
    .map_err(fail)?;
    fs::write(
        attempt.join("adapter.py"),
        include_str!("adapters/openms_metabo.py"),
    )
    .map_err(fail)?;
    let stdout = fs::File::create(attempt.join("stdout.log")).map_err(fail)?;
    let stderr = fs::File::create(attempt.join("stderr.log")).map_err(fail)?;
    let python = std::env::var_os("CHROMASCOPE_OPENMS_PYTHON").unwrap_or_else(|| "python".into());
    let mut command = Command::new(python);
    command
        .arg(attempt.join("adapter.py"))
        .arg(attempt)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(fail)?;
    let started = Instant::now();
    let mut progress = 0;
    let outcome = loop {
        if let Err(e) = control.check() {
            let _ = child.kill();
            let _ = child.wait();
            break Err(e);
        }
        if started.elapsed() >= Duration::from_secs(config.timeout_seconds) {
            let _ = child.kill();
            let _ = child.wait();
            break Err(EngineError::new(
                "timeout",
                "OpenMS processing deadline exceeded",
            ));
        }
        if let Ok(text) = fs::read_to_string(attempt.join("progress.txt")) {
            if let Ok(n) = text.trim().parse::<usize>() {
                while progress < n {
                    if !control.step() {
                        break;
                    }
                    progress += 1;
                }
            }
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break Ok(()),
            Ok(Some(_)) => {
                let mut error = fs::read(attempt.join("error.json"))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<EngineError>(&bytes).ok())
                    .unwrap_or_else(|| {
                        EngineError::new(
                            "adapter_failure",
                            "OpenMS failed; inspect retained stderr.log",
                        )
                    });
                error.message =
                    format!("{}; retained attempt: {}", error.message, attempt.display());
                break Err(error);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(fail(e));
            }
        }
    };
    outcome?;
    if fs::metadata(attempt.join("report.json"))
        .map_err(fail)?
        .len()
        > 256 * 1024 * 1024
    {
        return Err(EngineError::new(
            "resource_limit",
            "Feature response exceeds 256 MiB; inspect retained adapter outputs",
        ));
    }
    let bytes = fs::read(attempt.join("report.json")).map_err(fail)?;
    let report: Report = serde_json::from_slice(&bytes).map_err(fail)?;
    verify(&report)?;
    if serde_json::to_value(&report.config).map_err(fail)?
        != serde_json::to_value(config).map_err(fail)?
    {
        return Err(EngineError::new(
            "corrupt_result",
            "Adapter parameters mismatch",
        ));
    }
    if report.source_hashes != hashes {
        return Err(fail(std::io::Error::other("Adapter input hashes mismatch")));
    }
    for sample in &config.samples {
        control.check()?;
        if crate::project::source_identity_controlled(Path::new(&sample.source), Some(control))?.0
            != hashes[&sample.id]
        {
            return Err(EngineError::new(
                "stale_source",
                "Raw source changed during processing",
            ));
        }
    }
    Ok(report)
}
pub fn observations_csv(report: &Report) -> Result<String> {
    verify(report)?;
    let mut out = "feature_id,mz,aligned_rt_seconds,sample_id,state,intensity_area_seconds,flags\n"
        .to_string();
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    for f in &report.features {
        for c in &f.cells {
            let state = serde_json::to_value(&c.state).unwrap();
            out.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                quote(&f.id),
                f.mz,
                f.aligned_rt_seconds,
                quote(&c.sample_id),
                state.as_str().unwrap(),
                c.intensity.map(|v| v.to_string()).unwrap_or_default(),
                quote(&f.flags.join(";"))
            ));
        }
    }
    Ok(out)
}
pub fn verify_response(response: &crate::engine::Response) -> Result<()> {
    if response.version != 1
        || response.request.version != 1
        || response.request.actor.trim().is_empty()
    {
        return Err(EngineError::new(
            "corrupt_result",
            "Invalid feature response envelope",
        ));
    }
    match (&response.request.operation, &response.output) {
        (
            crate::domain::Operation::UntargetedBatch { config },
            crate::engine::Output::FeatureMatrix { report },
        ) => {
            verify(report)?;
            if serde_json::to_value(config).map_err(fail)?
                != serde_json::to_value(&report.config).map_err(fail)?
            {
                return Err(EngineError::new(
                    "corrupt_result",
                    "Feature request/response parameter mismatch",
                ));
            }
            Ok(())
        }
        _ => Err(EngineError::new(
            "corrupt_result",
            "Requires an untargeted batch response",
        )),
    }
}
pub fn csv(report: &Report) -> Result<String> {
    matrix_csv(report, false)
}
pub fn matrix_csv(report: &Report, included_only: bool) -> Result<String> {
    verify(report)?;
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out = "feature_id,mz,aligned_rt_seconds,filter_state,flags".to_string();
    for sample in &report.config.samples {
        out.push_str(&format!(
            ",{},{}",
            quote(&format!("{}:intensity_area_seconds", sample.id)),
            quote(&format!("{}:state", sample.id))
        ));
    }
    out.push('\n');
    for feature in &report.features {
        if included_only && feature.filter_state != FilterState::Included {
            continue;
        }
        out.push_str(&format!(
            "{},{},{},{},{}",
            quote(&feature.id),
            feature.mz,
            feature.aligned_rt_seconds,
            serde_json::to_value(&feature.filter_state)
                .unwrap()
                .as_str()
                .unwrap(),
            quote(&feature.flags.join(";"))
        ));
        for cell in &feature.cells {
            out.push_str(&format!(
                ",{},{}",
                cell.intensity.map(|v| v.to_string()).unwrap_or_default(),
                serde_json::to_value(&cell.state).unwrap().as_str().unwrap()
            ));
        }
        out.push('\n');
    }
    Ok(out)
}
