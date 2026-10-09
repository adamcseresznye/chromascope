//! Spectrum processing and conservative library evidence. Raw inputs are never mutated.
use crate::domain::{EngineError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
fn invalid(message: impl ToString) -> EngineError {
    EngineError::new("invalid_parameters", message)
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Tolerance {
    pub value: f64,
    pub unit: MassUnit,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MassUnit {
    Da,
    Ppm,
}
impl Tolerance {
    pub fn validate(&self) -> Result<()> {
        if !self.value.is_finite()
            || self.value <= 0.0
            || self.value
                > match self.unit {
                    MassUnit::Da => 1.0,
                    MassUnit::Ppm => 1000.0,
                }
        {
            return Err(invalid(
                "Tolerance must be positive and <=1 Da or <=1000 ppm",
            ));
        }
        Ok(())
    }
    pub fn da(&self, mz: f64) -> f64 {
        match self.unit {
            MassUnit::Da => self.value,
            MassUnit::Ppm => mz * self.value * 1e-6,
        }
    }
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Polarity {
    Positive,
    Negative,
    Unknown,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Representation {
    Centroid,
    Profile,
    Unknown,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Spectrum {
    pub id: String,
    /// Strictly increasing [m/z, intensity] pairs; intensity is native instrument or library arbitrary units.
    pub peaks: Vec<[f64; 2]>,
    pub ms_level: u8,
    pub representation: Representation,
    pub polarity: Polarity,
    pub precursor_mz: Option<f64>,
    pub precursor_type: Option<String>,
    /// Numeric energy and unit are both required; ranges/free text stay in metadata.
    pub collision_energy: Option<Energy>,
    pub instrument: Option<String>,
    /// Retention time in minutes; no automatic library/acquisition RT comparability.
    pub rt_minutes: Option<f64>,
    pub metadata: BTreeMap<String, String>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Energy {
    pub value: f64,
    pub unit: String,
}
impl Spectrum {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty()
            || self.ms_level == 0
            || self.peaks.len() > 100_000
            || self
                .peaks
                .iter()
                .any(|p| !p[0].is_finite() || p[0] <= 0.0 || !p[1].is_finite())
            || self.peaks.windows(2).any(|w| w[0][0] >= w[1][0])
            || self
                .precursor_mz
                .is_some_and(|x| !x.is_finite() || x <= 0.0)
            || self.rt_minutes.is_some_and(|x| !x.is_finite() || x < 0.0)
        {
            return Err(invalid("Spectrum requires ID, MS level, strictly increasing positive m/z and finite intensities/metadata"));
        }
        if self
            .collision_energy
            .as_ref()
            .is_some_and(|e| !e.value.is_finite() || e.value < 0.0 || e.unit.trim().is_empty())
        {
            return Err(invalid(
                "Collision energy requires finite nonnegative value and explicit unit",
            ));
        }
        Ok(())
    }
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Processing {
    pub tolerance: Tolerance,
    pub centroid_snr: Option<f32>,
    pub background_scale: f64,
    pub relative_threshold: f64,
}
impl Default for Processing {
    fn default() -> Self {
        Self {
            tolerance: Tolerance {
                value: 0.01,
                unit: MassUnit::Da,
            },
            centroid_snr: None,
            background_scale: 1.0,
            relative_threshold: 0.01,
        }
    }
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Processed {
    pub state: crate::domain::DataState,
    pub inputs: Vec<Spectrum>,
    pub background: Vec<Spectrum>,
    pub config: Processing,
    pub averaged: Spectrum,
    pub signed_subtracted: Vec<[f64; 2]>,
    pub spectrum: Spectrum,
    pub warnings: Vec<String>,
}
fn centroid(s: &Spectrum, snr: Option<f32>) -> Result<Spectrum> {
    s.validate()?;
    let mut result = s.clone();
    match s.representation {
        Representation::Centroid => (),
        Representation::Unknown => {
            return Err(EngineError::new(
                "unsupported_capability",
                "Unknown representation cannot be searched or averaged",
            ))
        }
        Representation::Profile => {
            let threshold =
                snr.ok_or_else(|| invalid("Profile processing requires explicit centroid_snr"))?;
            if !threshold.is_finite() || threshold <= 0.0 {
                return Err(invalid("centroid_snr must be finite positive"));
            }
            if s.peaks.len() < 3 {
                return Err(invalid(
                    "Profile centroiding requires at least three samples",
                ));
            }
            let picker = mzsignal::peak_picker::PeakPicker {
                signal_to_noise_threshold: threshold,
                ..Default::default()
            };
            let mz: Vec<_> = s.peaks.iter().map(|p| p[0]).collect();
            let intensity: Vec<_> = s.peaks.iter().map(|p| p[1] as f32).collect();
            if intensity.iter().any(|i| !i.is_finite()) {
                return Err(invalid("Profile intensity exceeds f32 mzsignal range"));
            }
            let mut peaks = Vec::new();
            picker
                .discover_peaks(&mz, &intensity, &mut peaks)
                .map_err(|e| EngineError::new("adapter_failure", e))?;
            result.peaks = peaks.iter().map(|p| [p.mz, p.intensity as f64]).collect();
            result.representation = Representation::Centroid;
        }
    }
    Ok(result)
}
fn compatible(a: &Spectrum, b: &Spectrum) -> bool {
    a.ms_level == b.ms_level
        && a.polarity == b.polarity
        && a.precursor_type == b.precursor_type
        && a.collision_energy == b.collision_energy
        && a.instrument == b.instrument
}
fn average(inputs: &[Spectrum], config: &Processing) -> Result<Spectrum> {
    let first = inputs
        .first()
        .ok_or_else(|| invalid("At least one spectrum required"))?;
    let mut all = Vec::new();
    for s in inputs {
        if !compatible(first, s)
            || first
                .precursor_mz
                .zip(s.precursor_mz)
                .is_some_and(|(a, b)| (a - b).abs() > config.tolerance.da(a))
            || first.precursor_mz.is_some() != s.precursor_mz.is_some()
        {
            return Err(invalid(
                "Averaging requires compatible MS level/polarity/precursor/energy/instrument",
            ));
        }
        all.extend(centroid(s, config.centroid_snr)?.peaks);
    }
    all.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let mut out = first.clone();
    out.id = "average".into();
    out.representation = Representation::Centroid;
    out.peaks.clear();
    let mut i = 0;
    while i < all.len() {
        let anchor = all[i][0];
        let mut j = i + 1;
        while j < all.len() && all[j][0] - anchor <= config.tolerance.da(anchor) {
            j += 1;
        }
        let weight: f64 = all[i..j].iter().map(|p| p[1].max(0.0)).sum();
        let mz = if weight > 0.0 {
            all[i..j].iter().map(|p| p[0] * p[1].max(0.0)).sum::<f64>() / weight
        } else {
            anchor
        };
        out.peaks.push([
            mz,
            all[i..j].iter().map(|p| p[1]).sum::<f64>() / inputs.len() as f64,
        ]);
        i = j;
    }
    out.rt_minutes = if inputs.iter().all(|s| s.rt_minutes.is_some()) {
        Some(inputs.iter().map(|s| s.rt_minutes.unwrap()).sum::<f64>() / inputs.len() as f64)
    } else {
        None
    };
    out.validate()?;
    Ok(out)
}
pub fn process(
    inputs: Vec<Spectrum>,
    background: Vec<Spectrum>,
    config: Processing,
) -> Result<Processed> {
    config.tolerance.validate()?;
    if config
        .centroid_snr
        .is_some_and(|v| !v.is_finite() || v <= 0.0)
    {
        return Err(invalid("centroid_snr must be finite positive"));
    }
    if inputs.len() > 256
        || background.len() > 256
        || inputs
            .iter()
            .chain(&background)
            .map(|s| s.peaks.len())
            .sum::<usize>()
            > 1_000_000
    {
        return Err(EngineError::new(
            "resource_limit",
            "At most 256 scans per group and one million total points",
        ));
    }
    if !config.background_scale.is_finite()
        || config.background_scale < 0.0
        || !config.relative_threshold.is_finite()
        || !(0.0..=1.0).contains(&config.relative_threshold)
    {
        return Err(invalid("Invalid background scale/relative threshold"));
    }
    let averaged = average(&inputs, &config)?;
    let bg = if background.is_empty() {
        None
    } else {
        Some(average(&background, &config)?)
    };
    if bg.as_ref().is_some_and(|b| {
        !compatible(&averaged, b)
            || averaged.precursor_mz.is_some() != b.precursor_mz.is_some()
            || averaged
                .precursor_mz
                .zip(b.precursor_mz)
                .is_some_and(|(x, y)| (x - y).abs() > config.tolerance.da(x))
    }) {
        return Err(invalid("Background acquisition incompatible with signal"));
    }
    // One-to-one mass assignment also preserves unmatched background-only negative peaks.
    let mut signed = averaged.peaks.clone();
    if let Some(b) = bg {
        let pairs = assign(&averaged.peaks, &b.peaks, &config.tolerance)?;
        let mut used = BTreeSet::new();
        for (i, j) in pairs {
            signed[i][1] -= config.background_scale * b.peaks[j][1];
            used.insert(j);
        }
        for (j, p) in b.peaks.iter().enumerate() {
            if !used.contains(&j) {
                signed.push([p[0], -config.background_scale * p[1]]);
            }
        }
        signed.sort_by(|a, b| a[0].total_cmp(&b[0]));
    }
    // Exact duplicate masses (e.g. zero signal plus background-only peak) combine algebraically.
    let mut combined: Vec<[f64; 2]> = Vec::new();
    for peak in signed {
        if let Some(last) = combined.last_mut().filter(|p| p[0] == peak[0]) {
            last[1] += peak[1];
        } else {
            combined.push(peak);
        }
    }
    let signed = combined;
    if signed.iter().any(|p| !p[1].is_finite()) {
        return Err(invalid(
            "Background subtraction overflowed finite intensity range",
        ));
    }
    let max = signed.iter().map(|p| p[1].max(0.0)).fold(0.0, f64::max);
    let mut spectrum = averaged.clone();
    spectrum.id = "processed".into();
    spectrum.peaks = signed
        .iter()
        .filter(|p| p[1] > 0.0 && p[1] >= max * config.relative_threshold)
        .copied()
        .collect();
    let mut warnings=vec!["Averaging treats absent centroid peaks as zero; tolerance bins use a fixed lowest-mass anchor".into()];
    if inputs.len() > 1 && inputs.iter().any(|s| s.instrument.is_none()) {
        warnings.push("Unknown instrument compatibility during averaging".into());
    }
    if spectrum.peaks.len() < 3 {
        warnings.push("Low-quality spectrum: fewer than three retained peaks".into());
    }
    let state = if spectrum.peaks.is_empty() {
        crate::domain::DataState::Missing
    } else {
        crate::domain::DataState::Present
    };
    Ok(Processed {
        state,
        inputs,
        background,
        config,
        averaged,
        signed_subtracted: signed,
        spectrum,
        warnings,
    })
}
/// Product-prioritized one-to-one greedy cosine assignment (matchms CosineGreedy definition).
fn assign(a: &[[f64; 2]], b: &[[f64; 2]], tol: &Tolerance) -> Result<Vec<(usize, usize)>> {
    let mut possible = Vec::new();
    for (i, p) in a.iter().enumerate() {
        let low = b.partition_point(|q| q[0] < p[0] - tol.da(p[0]));
        let high = b.partition_point(|q| q[0] <= p[0] + tol.da(p[0]));
        for (j, q) in b.iter().enumerate().take(high).skip(low) {
            if p[1] == 0.0 || q[1] == 0.0 {
                continue;
            }
            if possible.len() >= 1_000_000 {
                return Err(EngineError::new(
                    "resource_limit",
                    "Fragment assignment exceeds one million possible pairs; narrow tolerance",
                ));
            }
            possible.push((p[1].abs() * q[1].abs(), i, j));
        }
    }
    possible.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let (mut ai, mut bi) = (BTreeSet::new(), BTreeSet::new());
    Ok(possible
        .into_iter()
        .filter_map(|(_, i, j)| {
            if !ai.contains(&i) && !bi.contains(&j) {
                ai.insert(i);
                bi.insert(j);
                Some((i, j))
            } else {
                None
            }
        })
        .collect())
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FragmentMatch {
    pub query_index: usize,
    pub library_index: usize,
    pub query_mz: f64,
    pub library_mz: f64,
    pub error_da: f64,
    pub error_ppm: f64,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Similarity {
    pub cosine: f64,
    pub matched_query_fraction: f64,
    pub matches: Vec<FragmentMatch>,
}
pub fn compare(a: &Spectrum, b: &Spectrum, tol: &Tolerance) -> Result<Similarity> {
    a.validate()?;
    b.validate()?;
    tol.validate()?;
    if a.representation != Representation::Centroid || b.representation != Representation::Centroid
    {
        return Err(invalid("Similarity requires centroid spectra"));
    }
    let scale_a = a.peaks.iter().map(|p| p[1].max(0.0)).fold(0.0, f64::max);
    let scale_b = b.peaks.iter().map(|p| p[1].max(0.0)).fold(0.0, f64::max);
    if scale_a == 0.0 || scale_b == 0.0 {
        return Ok(Similarity {
            cosine: 0.0,
            matched_query_fraction: 0.0,
            matches: vec![],
        });
    }
    let pa: Vec<_> = a
        .peaks
        .iter()
        .map(|p| [p[0], p[1].max(0.0) / scale_a])
        .collect();
    let pb: Vec<_> = b
        .peaks
        .iter()
        .map(|p| [p[0], p[1].max(0.0) / scale_b])
        .collect();
    let pairs = assign(&pa, &pb, tol)?;
    let dot: f64 = pairs.iter().map(|(i, j)| pa[*i][1] * pb[*j][1]).sum();
    let norm = (pa.iter().map(|p| p[1] * p[1]).sum::<f64>()
        * pb.iter().map(|p| p[1] * p[1]).sum::<f64>())
    .sqrt();
    Ok(Similarity {
        cosine: (dot / norm).clamp(0.0, 1.0),
        matched_query_fraction: pairs.iter().map(|(i, _)| pa[*i][1]).sum::<f64>()
            / pa.iter().map(|p| p[1]).sum::<f64>(),
        matches: pairs
            .iter()
            .map(|(i, j)| FragmentMatch {
                query_index: *i,
                library_index: *j,
                query_mz: pa[*i][0],
                library_mz: pb[*j][0],
                error_da: pa[*i][0] - pb[*j][0],
                error_ppm: (pa[*i][0] - pb[*j][0]) / pb[*j][0] * 1e6,
            })
            .collect(),
    })
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct LibrarySource {
    pub name: String,
    pub version: String,
    pub url: String,
    pub license: String,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub accession: String,
    pub name: String,
    pub formula: Option<String>,
    pub structure: Option<String>,
    pub spectrum: Spectrum,
    pub metadata: BTreeMap<String, Vec<String>>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Library {
    pub source: LibrarySource,
    pub sha256: String,
    pub raw_text: String,
    pub format: String,
    pub entries: Vec<Entry>,
    pub precursor_index: Vec<(f64, usize)>,
    pub warnings: Vec<String>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SearchConfig {
    pub precursor_tolerance: Tolerance,
    pub fragment_tolerance: Tolerance,
    pub minimum_matches: usize,
    pub minimum_cosine: f64,
    pub maximum_candidates: usize,
    pub allow_missing_metadata: bool,
    pub require_same_instrument: bool,
    pub energy_tolerance: f64,
}
impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            precursor_tolerance: Tolerance {
                value: 10.0,
                unit: MassUnit::Ppm,
            },
            fragment_tolerance: Tolerance {
                value: 0.02,
                unit: MassUnit::Da,
            },
            minimum_matches: 3,
            minimum_cosine: 0.7,
            maximum_candidates: 20,
            allow_missing_metadata: false,
            require_same_instrument: true,
            energy_tolerance: 5.0,
        }
    }
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Candidate {
    pub reference: Spectrum,
    pub entry_index: usize,
    pub accession: String,
    pub name: String,
    pub precursor_error_ppm: f64,
    pub similarity: Similarity,
    pub warnings: Vec<String>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchReport {
    pub score_algorithm: String,
    pub total_candidates: usize,
    #[cfg_attr(feature = "mcp-headless", schemars(with = "String"))]
    pub id: uuid::Uuid,
    pub query: Processed,
    pub library: Library,
    pub config: SearchConfig,
    pub candidates: Vec<Candidate>,
    pub excluded: BTreeMap<String, usize>,
    pub annotations: Vec<Annotation>,
    pub warnings: Vec<String>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    ConfirmedIdentity,
    ProbableStructure,
    CompoundClass,
    Unknown,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub description: String,
    pub reference: String,
    pub same_method_standard: bool,
    pub rt_match: bool,
    pub diagnostic_fragments: bool,
    pub resolves_alternatives: bool,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Annotation {
    #[cfg_attr(feature = "mcp-headless", schemars(with = "String"))]
    pub id: uuid::Uuid,
    pub actor: String,
    pub reason: String,
    pub unix_ms: u64,
    pub candidate_accession: Option<String>,
    pub label: String,
    pub confidence: Confidence,
    pub evidence: Vec<Evidence>,
}
pub fn search(query: Processed, library: Library, config: SearchConfig) -> Result<SearchReport> {
    verify_processed(&query)?;
    verify_library(&library)?;
    config.precursor_tolerance.validate()?;
    config.fragment_tolerance.validate()?;
    if config.minimum_matches == 0
        || config.maximum_candidates == 0
        || config.maximum_candidates > 100
        || !config.minimum_cosine.is_finite()
        || !(0.0..=1.0).contains(&config.minimum_cosine)
        || !config.energy_tolerance.is_finite()
        || config.energy_tolerance < 0.0
    {
        return Err(invalid("Invalid search score/count/energy parameters"));
    }
    let q = &query.spectrum;
    if q.ms_level != 2
        || q.metadata
            .get("acquisition_mode")
            .is_some_and(|v| v.eq_ignore_ascii_case("dia"))
    {
        return Err(EngineError::new(
            "unsupported_capability",
            "Library identification supports MS2 DDA only; DIA requires deconvolution",
        ));
    }
    let precursor = q
        .precursor_mz
        .ok_or_else(|| invalid("MS2 search requires precursor m/z"))?;
    let (mut candidates, mut excluded) = (Vec::new(), BTreeMap::new());
    let low = library
        .precursor_index
        .partition_point(|(mz, _)| *mz < precursor - config.precursor_tolerance.da(precursor));
    let high = library
        .precursor_index
        .partition_point(|(mz, _)| *mz <= precursor + config.precursor_tolerance.da(precursor));
    for (_, index) in &library.precursor_index[low..high] {
        let e = &library.entries[*index];
        let s = &e.spectrum;
        let mut warnings = Vec::new();
        let rejection = (|| {
            if s.ms_level != 2 {
                return Some("ms_level");
            }
            if q.polarity != Polarity::Unknown
                && s.polarity != Polarity::Unknown
                && q.polarity != s.polarity
            {
                return Some("polarity");
            }
            if q.polarity == Polarity::Unknown || s.polarity == Polarity::Unknown {
                if !config.allow_missing_metadata {
                    return Some("missing_polarity");
                }
                warnings.push("Missing polarity".into());
            }
            for (key, a, b, required) in [
                (
                    "precursor_type",
                    q.precursor_type.as_ref(),
                    s.precursor_type.as_ref(),
                    true,
                ),
                (
                    "instrument",
                    q.instrument.as_ref(),
                    s.instrument.as_ref(),
                    config.require_same_instrument,
                ),
            ] {
                match (a, b) {
                    (Some(a), Some(b)) if a != b && required => return Some(key),
                    (None, _) | (_, None) => {
                        if required && !config.allow_missing_metadata {
                            return Some(key);
                        }
                        warnings.push(format!("Missing {key} compatibility"));
                    }
                    (Some(a), Some(b)) if a != b => warnings.push(format!("Different {key}")),
                    _ => (),
                }
            }
            match (&q.collision_energy, &s.collision_energy) {
                (Some(a), Some(b))
                    if a.unit != b.unit || (a.value - b.value).abs() > config.energy_tolerance =>
                {
                    return Some("collision_energy")
                }
                (None, _) | (_, None) => {
                    if !config.allow_missing_metadata {
                        return Some("missing_collision_energy");
                    }
                    warnings.push("Missing collision energy".into());
                }
                _ => (),
            };
            None
        })();
        if let Some(reason) = rejection {
            *excluded.entry(reason.into()).or_insert(0) += 1;
            continue;
        }
        let reference = process(vec![s.clone()], vec![], query.config.clone())?.spectrum;
        let similarity = compare(q, &reference, &config.fragment_tolerance)?;
        if similarity.matches.len() < config.minimum_matches
            || similarity.cosine < config.minimum_cosine
        {
            *excluded.entry("score_or_peak_count".into()).or_insert(0) += 1;
            continue;
        }
        candidates.push(Candidate {
            reference,
            entry_index: *index,
            accession: e.accession.clone(),
            name: e.name.clone(),
            precursor_error_ppm: (precursor - s.precursor_mz.unwrap()) / s.precursor_mz.unwrap()
                * 1e6,
            similarity,
            warnings,
        });
    }
    candidates.sort_by(|a, b| {
        b.similarity
            .cosine
            .total_cmp(&a.similarity.cosine)
            .then(b.similarity.matches.len().cmp(&a.similarity.matches.len()))
            .then(
                a.precursor_error_ppm
                    .abs()
                    .total_cmp(&b.precursor_error_ppm.abs()),
            )
            .then(a.accession.cmp(&b.accession))
    });
    let total_candidates = candidates.len();
    candidates.truncate(config.maximum_candidates);
    let mut warnings=vec!["Cosine is similarity evidence, not an identification probability or FDR; all unreviewed features remain unknown".into()];
    warnings.extend(query.warnings.clone());
    if total_candidates > candidates.len() {
        warnings.push(format!(
            "Candidate list truncated: {} of {} compatible matches retained",
            candidates.len(),
            total_candidates
        ));
    }
    if total_candidates > 1 {
        warnings
            .push("Multiple candidates remain; isomers/isobars require orthogonal evidence".into());
    }
    if !q.metadata.contains_key("acquisition_mode") {
        warnings.push(
            "Acquisition mode undeclared; coisolated/chimeric fragments may cause false matches"
                .into(),
        );
    }
    Ok(SearchReport {
        score_algorithm:
            "cosine_greedy_v1; intensity_power=1; mz_power=0; positive intensities only".into(),
        total_candidates,
        id: uuid::Uuid::new_v4(),
        query,
        library,
        config,
        candidates,
        excluded,
        annotations: vec![],
        warnings,
    })
}
pub fn verify_processed(p: &Processed) -> Result<()> {
    if process(p.inputs.clone(), p.background.clone(), p.config.clone())? != *p {
        return Err(EngineError::new(
            "corrupt_artifact",
            "Spectrum processing replay differs",
        ));
    }
    Ok(())
}
pub fn verify_report(r: &SearchReport) -> Result<()> {
    let replay = search(r.query.clone(), r.library.clone(), r.config.clone())?;
    if replay.score_algorithm != r.score_algorithm
        || replay.total_candidates != r.total_candidates
        || replay.candidates != r.candidates
        || replay.excluded != r.excluded
        || replay.warnings != r.warnings
    {
        return Err(EngineError::new(
            "corrupt_artifact",
            "Search replay differs",
        ));
    }
    if r.id.is_nil() {
        return Err(invalid("Search report requires non-nil identity"));
    }
    let mut ids = BTreeSet::new();
    for a in &r.annotations {
        validate_annotation(r, a)?;
        if a.id.is_nil() || a.unix_ms == 0 || !ids.insert(a.id) {
            return Err(invalid("Duplicate annotation ID"));
        }
    }
    Ok(())
}
fn validate_annotation(r: &SearchReport, a: &Annotation) -> Result<()> {
    if a.actor.trim().is_empty()
        || a.reason.trim().is_empty()
        || a.label.trim().is_empty()
        || a.evidence
            .iter()
            .any(|e| e.description.trim().is_empty() || e.reference.trim().is_empty())
    {
        return Err(invalid(
            "Annotation requires actor/reason/label and referenced evidence",
        ));
    }
    if a.candidate_accession
        .as_ref()
        .is_some_and(|id| !r.candidates.iter().any(|c| &c.accession == id))
    {
        return Err(invalid("Candidate absent from retained search"));
    }
    match a.confidence {
        Confidence::ConfirmedIdentity if a.candidate_accession.is_none() || !a.evidence.iter().any(|e|e.same_method_standard&&e.rt_match&&e.diagnostic_fragments&&e.resolves_alternatives)=>return Err(invalid("Confirmed identity requires same-method authentic standard, RT and diagnostic fragments, and resolution of alternatives")),
        Confidence::ProbableStructure if a.candidate_accession.is_none() || !a.evidence.iter().any(|e|e.diagnostic_fragments&&e.resolves_alternatives)=>return Err(invalid("Probable structure requires diagnostic evidence resolving alternatives; cosine alone is insufficient")),
        Confidence::CompoundClass if !a.evidence.iter().any(|e|e.diagnostic_fragments)=>return Err(invalid("Compound class requires referenced diagnostic evidence")),_=>()
    }
    Ok(())
}
pub fn annotate(
    r: &SearchReport,
    expected_revision: usize,
    mut annotation: Annotation,
    actor: &str,
) -> Result<SearchReport> {
    verify_report(r)?;
    if expected_revision != r.annotations.len() {
        return Err(EngineError::new(
            "stale_revision",
            "Annotation revision changed",
        ));
    }
    annotation.id = uuid::Uuid::new_v4();
    annotation.actor = actor.into();
    annotation.unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    validate_annotation(r, &annotation)?;
    let mut next = r.clone();
    next.annotations.push(annotation);
    Ok(next)
}
mod library;
pub use library::{import_library, verify_library};
mod chemistry;
pub use chemistry::*;
/// Shared operations used identically by desktop, CLI and headless MCP.
pub fn execute_operation(
    request: &crate::domain::Request,
) -> Result<Option<crate::engine::Output>> {
    use crate::domain::Operation;
    use crate::engine::Output;
    Ok(Some(match &request.operation {
        Operation::CompareSpectra {
            query,
            reference,
            tolerance,
        } => Output::SpectralComparison {
            query: query.clone(),
            reference: reference.clone(),
            tolerance: tolerance.clone(),
            similarity: compare(query, reference, tolerance)?,
        },
        Operation::ExportSpectralCandidates { report } => Output::SpectralCandidateTable {
            csv: candidates_csv(report)?,
        },
        Operation::ProcessSpectra {
            spectra,
            background,
            config,
        } => Output::SpectralProcessing {
            processed: Box::new(process(
                spectra.clone(),
                background.clone(),
                config.clone(),
            )?),
        },
        Operation::ImportSpectralLibrary {
            text,
            format,
            source,
        } => Output::SpectralLibrary {
            library: Box::new(import_library(
                text.clone(),
                format.clone(),
                source.clone(),
            )?),
        },
        Operation::SearchSpectralLibrary {
            query,
            library,
            config,
        } => Output::SpectralSearch {
            report: Box::new(search(*query.clone(), *library.clone(), config.clone())?),
        },
        Operation::AnnotateSpectrum {
            report,
            expected_revision,
            annotation,
        } => Output::SpectralSearch {
            report: Box::new(annotate(
                report,
                *expected_revision,
                annotation.clone(),
                &request.actor,
            )?),
        },
        Operation::FormulaCandidates { config } => Output::FormulaCandidates {
            report: formula_candidates(config.clone())?,
        },
        Operation::AnalyzeIsotopes { spectrum, config } => Output::IsotopeAnalysis {
            report: isotopes(spectrum.clone(), config.clone())?,
        },
        _ => return Ok(None),
    }))
}
/// Candidate export is evidence, not a list of automatically identified compounds.
pub fn candidates_csv(report: &SearchReport) -> Result<String> {
    verify_report(report)?;
    let quote = |text: &str| format!("\"{}\"", text.replace('"', "\"\""));
    let mut csv=String::from("report_id,library_name,library_version,source_url,library_sha256,license,accession,name,precursor_error_ppm,cosine,matched_fragments,matched_query_intensity_fraction,precursor_tolerance,precursor_tolerance_unit,fragment_tolerance,fragment_tolerance_unit,current_confidence,annotation_reason,compatibility_warnings\n");
    for c in &report.candidates {
        let e = &report.library.entries[c.entry_index];
        let license = e
            .metadata
            .get("LICENSE")
            .and_then(|v| v.first())
            .unwrap_or(&report.library.source.license);
        let annotation = report
            .annotations
            .last()
            .filter(|a| a.candidate_accession.as_deref() == Some(c.accession.as_str()));
        let confidence = annotation
            .map(|a| format!("{:?}", a.confidence))
            .unwrap_or_else(|| "Unknown".into());
        let fields = vec![
            report.id.to_string(),
            report.library.source.name.clone(),
            report.library.source.version.clone(),
            report.library.source.url.clone(),
            report.library.sha256.clone(),
            license.clone(),
            c.accession.clone(),
            c.name.clone(),
            c.precursor_error_ppm.to_string(),
            c.similarity.cosine.to_string(),
            c.similarity.matches.len().to_string(),
            c.similarity.matched_query_fraction.to_string(),
            report.config.precursor_tolerance.value.to_string(),
            format!("{:?}", report.config.precursor_tolerance.unit),
            report.config.fragment_tolerance.value.to_string(),
            format!("{:?}", report.config.fragment_tolerance.unit),
            confidence,
            annotation.map(|a| a.reason.clone()).unwrap_or_default(),
            c.warnings.join("; "),
        ];
        csv.push_str(
            &fields
                .iter()
                .map(|f| quote(f))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push('\n');
    }
    Ok(csv)
}
/// Validate retained spectral numeric evidence before load/export/project commit.
pub fn verify_response(response: &crate::engine::Response) -> Result<()> {
    use crate::engine::Output;
    let valid = match &response.output {
        Output::SpectralProcessing { processed } => {
            verify_processed(processed)?;
            true
        }
        Output::SpectralLibrary { library } => {
            verify_library(library)?;
            true
        }
        Output::SpectralSearch { report } => {
            verify_report(report)?;
            true
        }
        Output::SpectralComparison {
            query,
            reference,
            tolerance,
            similarity,
        } => compare(query, reference, tolerance)? == *similarity,
        Output::FormulaCandidates { report } => {
            formula_candidates(report.config.clone())? == *report
        }
        Output::IsotopeAnalysis { report } => {
            isotopes(report.spectrum.clone(), report.config.clone())? == *report
        }
        Output::SpectralCandidateTable { csv } => match &response.request.operation {
            crate::domain::Operation::ExportSpectralCandidates { report } => {
                candidates_csv(report)? == *csv
            }
            _ => false,
        },
        _ => {
            return Err(EngineError::new(
                "unsupported_capability",
                "Response is not spectral evidence",
            ))
        }
    };
    if !valid {
        return Err(EngineError::new(
            "corrupt_artifact",
            "Spectral numeric evidence differs on replay",
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn scan(intensity: f64) -> Spectrum {
        Spectrum {
            id: "scan".into(),
            peaks: vec![[100.0, intensity]],
            ms_level: 1,
            representation: Representation::Centroid,
            polarity: Polarity::Positive,
            precursor_mz: None,
            precursor_type: None,
            collision_energy: None,
            instrument: None,
            rt_minutes: Some(0.0),
            metadata: BTreeMap::new(),
        }
    }
    #[test]
    fn zero_signal_background_preserves_signed_peak_and_missing_state() {
        let p = process(vec![scan(0.0)], vec![scan(5.0)], Processing::default()).unwrap();
        assert_eq!(p.signed_subtracted, vec![[100.0, -5.0]]);
        assert_eq!(p.state, crate::domain::DataState::Missing);
        assert!(p.spectrum.peaks.is_empty());
        verify_processed(&p).unwrap();
    }
    #[test]
    fn subtraction_overflow_and_unused_invalid_centroid_snr_rejected() {
        assert!(process(
            vec![scan(1.0)],
            vec![scan(f64::MAX)],
            Processing {
                background_scale: 2.0,
                ..Default::default()
            }
        )
        .is_err());
        assert!(process(
            vec![scan(1.0)],
            vec![],
            Processing {
                centroid_snr: Some(f32::NAN),
                ..Default::default()
            }
        )
        .is_err());
    }
    #[test]
    fn incompatible_background_precursor_presence_is_rejected() {
        let mut background = scan(1.0);
        background.precursor_mz = Some(100.0);
        assert!(process(vec![scan(2.0)], vec![background], Processing::default()).is_err());
    }
}
