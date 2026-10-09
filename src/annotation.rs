//! Feature hypotheses, with immutable analytical evidence and reversible review.
use crate::{
    domain::{EngineError, Result},
    spectral as s, untargeted as u,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
fn invalid(message: &str) -> EngineError {
    EngineError::new("invalid_parameters", message)
}

/// Conservative subset of Liebisch et al. 2020: ester glycerolipids/phospholipids.
/// No sn-position, double-bond position, ether or stereochemical inference.
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Lipid {
    pub class: String,
    pub carbons: u16,
    pub double_bonds: u16,
    /// Empty = sum composition; nonempty = identified, unordered acyl chains.
    pub chains: Vec<[u16; 2]>,
}
impl Lipid {
    pub fn label(&self) -> Result<String> {
        let count = match self.class.as_str() {
            "PC" | "PE" | "PG" | "PI" | "PS" | "PA" | "DG" => 2,
            "LPC" | "LPE" | "MG" => 1,
            "TG" => 3,
            _ => {
                return Err(EngineError::new(
                    "unsupported_capability",
                    "Unsupported lipid class/linkage; use a specialist nomenclature adapter",
                ))
            }
        };
        if self.carbons == 0
            || self.double_bonds >= self.carbons
            || (!self.chains.is_empty()
                && (self.chains.len() != count
                    || self.chains.iter().any(|c| c[0] == 0 || c[1] >= c[0])
                    || self.chains.iter().map(|c| u32::from(c[0])).sum::<u32>()
                        != u32::from(self.carbons)
                    || self.chains.iter().map(|c| u32::from(c[1])).sum::<u32>()
                        != u32::from(self.double_bonds)))
        {
            return Err(invalid("Invalid lipid composition or chain totals"));
        }
        let mut chains = self.chains.clone();
        chains.sort();
        let composition = if chains.is_empty() {
            format!("{}:{}", self.carbons, self.double_bonds)
        } else {
            chains
                .iter()
                .map(|c| format!("{}:{}", c[0], c[1]))
                .collect::<Vec<_>>()
                .join("_")
        };
        Ok(format!("{} {composition}", self.class))
    }
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hypothesis {
    #[cfg_attr(feature = "mcp-headless", schemars(with = "String"))]
    pub id: uuid::Uuid,
    pub feature_id: String,
    pub sample_id: String,
    pub label: String,
    /// Explicit candidate structure identifier; distinct isomers remain distinct hypotheses.
    pub structure: Option<String>,
    pub lipid: Option<Lipid>,
    pub formula: Option<s::FormulaReport>,
    pub formula_candidate: Option<usize>,
    pub isotope: Option<s::IsotopeReport>,
    /// Reproducible MassBank/MSP/MGF adapter results, including original library text.
    pub msms: Option<Box<s::SearchReport>>,
    pub accession: Option<String>,
    pub confidence: s::Confidence,
    pub evidence: Vec<s::Evidence>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Review {
    #[cfg_attr(feature = "mcp-headless", schemars(with = "String"))]
    pub id: uuid::Uuid,
    #[cfg_attr(feature = "mcp-headless", schemars(with = "String"))]
    pub hypothesis_id: uuid::Uuid,
    pub actor: String,
    pub reason: String,
    pub unix_ms: u64,
    pub decision: Decision,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Accept,
    Reject,
    Reopen,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposalConfig {
    pub feature_id: String,
    pub sample_id: String,
    pub formula: s::FormulaConfig,
    pub library: Option<Box<s::Library>>,
    pub search: s::SearchConfig,
    pub ms2_index: usize,
}
/// Uses existing scientific kernels and reproducible library adapters; no identity promotion.
pub fn propose(ledger: &Ledger, config: &ProposalConfig, revision: usize) -> Result<Ledger> {
    verify(ledger)?;
    stale(ledger, revision)?;
    let feature = ledger
        .matrix
        .features
        .iter()
        .find(|f| f.id == config.feature_id)
        .ok_or_else(|| invalid("Feature absent"))?;
    let cell = feature
        .cells
        .iter()
        .find(|c| c.sample_id == config.sample_id)
        .ok_or_else(|| invalid("Sample absent"))?;
    let formula = s::formula_candidates(config.formula.clone())?;
    let spectrum = s::Spectrum {
        id: format!("{}:{}:apex", feature.id, cell.sample_id),
        peaks: cell.apex_spectrum.clone(),
        ms_level: 1,
        representation: s::Representation::Centroid,
        polarity: ledger.matrix.config.polarity,
        precursor_mz: None,
        precursor_type: None,
        collision_energy: None,
        instrument: None,
        rt_minutes: cell.raw_rt_seconds.map(|x| x / 60.),
        metadata: Default::default(),
    };
    let isotope = s::isotopes(
        spectrum,
        s::IsotopeConfig {
            expected_atom_counts: None,
            monoisotopic_mz: config.formula.observed_mz,
            maximum_charge: 6,
            maximum_isotopes: 6,
            tolerance: config.formula.tolerance.clone(),
        },
    )?;
    let mut next = ledger.clone();
    let base=Hypothesis {id:uuid::Uuid::new_v4(),feature_id:feature.id.clone(),sample_id:cell.sample_id.clone(),label:String::new(),structure:None,lipid:None,formula:Some(formula.clone()),formula_candidate:None,isotope:Some(isotope),msms:None,accession:None,confidence:s::Confidence::Unknown,evidence:vec![s::Evidence {description:"Bounded CHNOPS exact-mass hypothesis; mass and isotope spacing do not resolve structures".into(),reference:format!("chromascope {} spectral chemistry; retained config and feature matrix",env!("CARGO_PKG_VERSION")),same_method_standard:false,rt_match:false,diagnostic_fragments:false,resolves_alternatives:false}]};
    for (i, f) in formula.candidates.iter().enumerate() {
        let mut h = base.clone();
        h.id = uuid::Uuid::new_v4();
        h.label = format!("{} {}", f.formula, f.adduct);
        h.formula_candidate = Some(i);
        next.hypotheses.push(h);
    }
    if let Some(library) = &config.library {
        let scan = cell
            .ms2
            .get(config.ms2_index)
            .ok_or_else(|| invalid("Requested associated MS/MS scan missing"))?;
        let representation = match scan.representation.as_str() {
            "centroid" => s::Representation::Centroid,
            "profile" => s::Representation::Profile,
            _ => s::Representation::Unknown,
        };
        let query = s::Spectrum {
            id: scan.native_id.clone(),
            peaks: scan.peaks.clone(),
            ms_level: 2,
            representation,
            polarity: ledger.matrix.config.polarity,
            precursor_mz: Some(scan.precursor_mz),
            precursor_type: cell.adduct.clone(),
            collision_energy: scan.collision_energy_ev.map(|value| s::Energy {
                value,
                unit: "eV".into(),
            }),
            instrument: None,
            rt_minutes: Some(scan.raw_rt_seconds / 60.),
            metadata: Default::default(),
        };
        let processed = s::process(vec![query], vec![], s::Processing::default())?;
        let report = s::search(processed, *library.clone(), config.search.clone())?;
        for candidate in &report.candidates {
            let entry = &report.library.entries[candidate.entry_index];
            let mut h = base.clone();
            h.id = uuid::Uuid::new_v4();
            h.label = entry.name.clone();
            h.structure = entry.structure.clone();
            h.formula = None;
            h.formula_candidate = None;
            h.accession = Some(entry.accession.clone());
            h.msms = Some(Box::new(report.clone()));
            h.evidence.push(s::Evidence {description:"Tentative spectral-library match; retained alternatives, score, preprocessing and reference spectra".into(),reference:format!("{} {} {} accession {} SHA256 {}",report.library.source.name,report.library.source.version,report.library.source.url,entry.accession,report.library.sha256),same_method_standard:false,rt_match:false,diagnostic_fragments:false,resolves_alternatives:false});
            next.hypotheses.push(h);
        }
    }
    next.adduct_relationships = relationships(&next);
    verify(&next)?;
    Ok(next)
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ledger {
    pub version: u32,
    pub matrix: Box<u::Report>,
    pub hypotheses: Vec<Hypothesis>,
    pub reviews: Vec<Review>,
    #[serde(default)]
    pub adduct_relationships: Vec<AdductRelationship>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AdductRelationship {
    #[cfg_attr(feature = "mcp-headless", schemars(with = "String"))]
    pub first: uuid::Uuid,
    #[cfg_attr(feature = "mcp-headless", schemars(with = "String"))]
    pub second: uuid::Uuid,
    pub neutral_mass_difference_da: f64,
    pub raw_rt_difference_seconds: f64,
    pub tolerance_ppm: f64,
    pub tolerance_seconds: f64,
    pub algorithm: String,
}
fn relationships(ledger: &Ledger) -> Vec<AdductRelationship> {
    let mut result = vec![];
    for (i, a) in ledger.hypotheses.iter().enumerate() {
        for b in ledger.hypotheses.iter().skip(i + 1) {
            if a.sample_id != b.sample_id || a.feature_id == b.feature_id {
                continue;
            }
            let (Some(af), Some(bf), Some(ai), Some(bi)) = (
                &a.formula,
                &b.formula,
                a.formula_candidate,
                b.formula_candidate,
            ) else {
                continue;
            };
            let (Some(ac), Some(bc)) = (af.candidates.get(ai), bf.candidates.get(bi)) else {
                continue;
            };
            if ac.formula != bc.formula || ac.adduct == bc.adduct {
                continue;
            }
            let (Some(am), Some(bm)) = (
                af.adduct_hypotheses.iter().find(|x| x.adduct == ac.adduct),
                bf.adduct_hypotheses.iter().find(|x| x.adduct == bc.adduct),
            ) else {
                continue;
            };
            let rt = |h: &Hypothesis| {
                ledger
                    .matrix
                    .features
                    .iter()
                    .find(|f| f.id == h.feature_id)
                    .and_then(|f| f.cells.iter().find(|c| c.sample_id == h.sample_id))
                    .and_then(|c| c.raw_rt_seconds)
            };
            let (Some(ar), Some(br)) = (rt(a), rt(b)) else {
                continue;
            };
            let mass_difference = (am.neutral_mass_da - bm.neutral_mass_da).abs();
            let rt_difference = (ar - br).abs();
            if mass_difference
                <= am.neutral_mass_da * ledger.matrix.config.correspondence_ppm * 1e-6
                && rt_difference <= ledger.matrix.config.rt_tolerance_seconds
            {
                result.push(AdductRelationship {first:a.id,second:b.id,neutral_mass_difference_da:mass_difference,raw_rt_difference_seconds:rt_difference,tolerance_ppm:ledger.matrix.config.correspondence_ppm,tolerance_seconds:ledger.matrix.config.rt_tolerance_seconds,algorithm:"tentative_same_formula_distinct_adduct_neutral_mass_coelution_v1; not proof of shared compound".into()});
            }
        }
    }
    result
}
fn validate_hypothesis(matrix: &u::Report, h: &Hypothesis) -> Result<()> {
    let feature = matrix
        .features
        .iter()
        .find(|f| f.id == h.feature_id)
        .ok_or_else(|| invalid("Feature absent"))?;
    let cell = feature
        .cells
        .iter()
        .find(|c| c.sample_id == h.sample_id)
        .ok_or_else(|| invalid("Sample absent"))?;
    let mz = cell
        .mz
        .ok_or_else(|| invalid("Missing observation cannot support an annotation"))?;
    let close = |x: f64| (x - mz).abs() <= mz * matrix.config.correspondence_ppm * 1e-6;
    if h.id.is_nil()
        || h.label.trim().is_empty()
        || h.structure.as_ref().is_some_and(|x| x.trim().is_empty())
        || h.evidence.is_empty()
        || h.evidence
            .iter()
            .any(|e| e.description.trim().is_empty() || e.reference.trim().is_empty())
    {
        return Err(invalid(
            "Hypothesis requires identity, label and referenced evidence",
        ));
    }
    if let Some(lipid) = &h.lipid {
        if lipid.label()? != h.label {
            return Err(invalid(
                "Lipid label exceeds declared structural resolution",
            ));
        }
        if !lipid.chains.is_empty() && !h.evidence.iter().any(|e| e.diagnostic_fragments) {
            return Err(invalid(
                "Molecular species requires referenced chain diagnostic evidence",
            ));
        }
    }
    if let Some(r) = &h.formula {
        let replay = s::formula_candidates(r.config.clone())?;
        if replay != *r
            || !close(r.config.observed_mz)
            || r.config.polarity != matrix.config.polarity
        {
            return Err(invalid(
                "Formula evidence is corrupt or unrelated to feature/polarity",
            ));
        }
        if h.formula_candidate.is_none_or(|i| i >= r.candidates.len()) {
            return Err(invalid("Formula evidence contains no candidates"));
        }
        let selected = &r.candidates[h.formula_candidate.unwrap()];
        let adduct = r
            .adduct_hypotheses
            .iter()
            .find(|a| a.adduct == selected.adduct)
            .ok_or_else(|| invalid("Selected formula adduct absent"))?;
        if cell.charge.is_some_and(|charge| adduct.charge != charge) {
            return Err(invalid("Selected adduct charge conflicts with observation"));
        }
        if cell.adduct.as_ref().is_some_and(|a| a != &selected.adduct) {
            return Err(invalid(
                "Selected formula adduct conflicts with retained feature assignment",
            ));
        }
    }
    if h.formula.is_none() && h.formula_candidate.is_some() {
        return Err(invalid("Formula selection requires evidence"));
    }
    if let Some(r) = &h.isotope {
        if s::isotopes(r.spectrum.clone(), r.config.clone())? != *r
            || r.spectrum.peaks != cell.apex_spectrum
            || !close(r.config.monoisotopic_mz)
            || r.spectrum.polarity != matrix.config.polarity
            || r.spectrum.rt_minutes != cell.raw_rt_seconds.map(|x| x / 60.)
        {
            return Err(invalid(
                "Isotope evidence is corrupt or unrelated to apex spectrum",
            ));
        }
    }
    if let Some(r) = &h.msms {
        s::verify_report(r)?;
        if r.query.inputs.len() != 1 || !r.query.background.is_empty() {
            return Err(invalid(
                "Feature MS/MS requires one linked scan without background",
            ));
        }
        let q = &r.query.inputs[0];
        if q.polarity != matrix.config.polarity
            || q.ms_level != 2
            || !cell.ms2.iter().any(|scan| {
                scan.native_id == q.id
                    && scan.peaks == q.peaks
                    && q.precursor_mz == Some(scan.precursor_mz)
                    && q.rt_minutes == Some(scan.raw_rt_seconds / 60.)
                    && q.precursor_type == cell.adduct
                    && q.representation
                        == match scan.representation.as_str() {
                            "centroid" => s::Representation::Centroid,
                            "profile" => s::Representation::Profile,
                            _ => s::Representation::Unknown,
                        }
                    && q.collision_energy
                        == scan.collision_energy_ev.map(|value| s::Energy {
                            value,
                            unit: "eV".into(),
                        })
                    && cell.raw_bounds_seconds.is_some_and(|bounds| {
                        scan.raw_rt_seconds >= bounds[0] && scan.raw_rt_seconds <= bounds[1]
                    })
                    && (scan.precursor_mz - mz).abs() <= mz * matrix.config.detection_ppm * 1e-6
            })
        {
            return Err(invalid(
                "MS/MS query must retain feature-associated native scan, peaks, precursor and RT",
            ));
        }
        let candidate = r
            .candidates
            .iter()
            .find(|c| Some(&c.accession) == h.accession.as_ref())
            .ok_or_else(|| invalid("Candidate absent from MS/MS evidence"))?;
        let entry = &r.library.entries[candidate.entry_index];
        if h.lipid.is_none() && (h.label != entry.name || h.structure != entry.structure) {
            return Err(invalid("Identity conflicts with library candidate"));
        }
        if let Some(f) = &h.formula {
            if candidate
                .reference
                .precursor_type
                .as_ref()
                .is_some_and(|adduct| &f.candidates[h.formula_candidate.unwrap()].adduct != adduct)
            {
                return Err(invalid("Formula and MS/MS adduct evidence conflict"));
            }
            if entry.formula.as_ref().is_some_and(|formula| {
                &f.candidates[h.formula_candidate.unwrap()].formula != formula
            }) {
                return Err(invalid(
                    "Formula and MS/MS identification evidence conflict",
                ));
            }
        }
        if !candidate.warnings.is_empty() && h.confidence != s::Confidence::Unknown {
            return Err(invalid(
                "Incompatible spectral evidence cannot support elevated confidence",
            ));
        }
    } else if h.accession.is_some() {
        return Err(invalid("Accession requires retained MS/MS search"));
    }
    match h.confidence {
        s::Confidence::ConfirmedIdentity => return Err(invalid("Feature hypotheses cannot be confirmed identities; authentic-standard confirmation uses the spectral workflow")),
        s::Confidence::ProbableStructure if h.msms.is_none() || h.structure.is_none() || h.lipid.is_some() || !h.evidence.iter().any(|e|e.diagnostic_fragments && e.resolves_alternatives) => return Err(invalid("Probable structure requires MS/MS and diagnostic evidence resolving alternatives")),
        s::Confidence::CompoundClass if !h.evidence.iter().any(|e|e.diagnostic_fragments) => return Err(invalid("Class annotation requires diagnostic evidence")),
        _ => (),
    }
    Ok(())
}
pub fn verify(ledger: &Ledger) -> Result<()> {
    if ledger.hypotheses.len() > 256 || ledger.reviews.len() > 10_000 {
        return Err(EngineError::new(
            "resource_limit",
            "Annotation ledger allows 256 hypotheses and 10000 reviews; partition larger studies",
        ));
    }
    u::verify(&ledger.matrix)?;
    if ledger.version != 1 {
        return Err(invalid("Unsupported annotation schema"));
    }
    let mut ids = BTreeSet::new();
    for h in &ledger.hypotheses {
        validate_hypothesis(&ledger.matrix, h)?;
        if !ids.insert(h.id) {
            return Err(invalid("Duplicate hypothesis identity"));
        }
    }
    let mut review_ids = BTreeSet::new();
    for r in &ledger.reviews {
        if !ids.contains(&r.hypothesis_id)
            || r.id.is_nil()
            || !review_ids.insert(r.id)
            || r.actor.trim().is_empty()
            || r.reason.trim().is_empty()
            || r.unix_ms == 0
        {
            return Err(invalid("Invalid review history"));
        }
    }
    if ledger.adduct_relationships != relationships(ledger) {
        return Err(invalid("Adduct relationship replay differs"));
    }
    Ok(())
}
pub fn add(ledger: &Ledger, h: Hypothesis, expected_revision: usize) -> Result<Ledger> {
    verify(ledger)?;
    stale(ledger, expected_revision)?;
    let mut next = ledger.clone();
    next.hypotheses.push(h);
    next.adduct_relationships = relationships(&next);
    verify(&next)?;
    Ok(next)
}
fn stale(l: &Ledger, revision: usize) -> Result<()> {
    if revision != l.hypotheses.len() + l.reviews.len() {
        return Err(EngineError::new(
            "stale_revision",
            "Annotation ledger changed",
        ));
    }
    Ok(())
}
pub fn review(
    ledger: &Ledger,
    id: uuid::Uuid,
    revision: usize,
    actor: &str,
    reason: &str,
    decision: Decision,
) -> Result<Ledger> {
    verify(ledger)?;
    stale(ledger, revision)?;
    let mut next = ledger.clone();
    next.reviews.push(Review {
        id: uuid::Uuid::new_v4(),
        hypothesis_id: id,
        actor: actor.into(),
        reason: reason.into(),
        decision,
        unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    });
    verify(&next)?;
    Ok(next)
}
/// Every candidate exports, including rejected alternatives. Acceptance is review, not identity confirmation.
pub fn csv(ledger: &Ledger) -> Result<String> {
    verify(ledger)?;
    let mut out=String::from("feature_id,sample_id,hypothesis_id,label,structure,confidence,review_status,evidence_json,source_sha256,feature_evidence_json,adduct_relationships_json,review_history_json,matrix_provenance_json\n");
    for h in &ledger.hypotheses {
        let state = ledger
            .reviews
            .iter()
            .rev()
            .find(|r| r.hypothesis_id == h.id)
            .map(|r| format!("{:?}", r.decision))
            .unwrap_or_else(|| "Unreviewed".into());
        let fields = [
            h.feature_id.clone(),
            h.sample_id.clone(),
            h.id.to_string(),
            h.label.clone(),
            h.structure.clone().unwrap_or_default(),
            format!("{:?}", h.confidence),
            state,
            serde_json::to_string(h).map_err(|e| EngineError::new("serialization_failure", e))?,
            ledger.matrix.source_hashes[&h.sample_id].clone(),
            serde_json::to_string(
                &ledger
                    .matrix
                    .features
                    .iter()
                    .find(|f| f.id == h.feature_id)
                    .unwrap()
                    .cells
                    .iter()
                    .find(|c| c.sample_id == h.sample_id)
                    .unwrap(),
            )
            .map_err(|e| EngineError::new("serialization_failure", e))?,
            serde_json::to_string(
                &ledger
                    .adduct_relationships
                    .iter()
                    .filter(|r| r.first == h.id || r.second == h.id)
                    .collect::<Vec<_>>(),
            )
            .map_err(|e| EngineError::new("serialization_failure", e))?,
            serde_json::to_string(
                &ledger
                    .reviews
                    .iter()
                    .filter(|r| r.hypothesis_id == h.id)
                    .collect::<Vec<_>>(),
            )
            .map_err(|e| EngineError::new("serialization_failure", e))?,
            serde_json::to_string(&ledger.matrix.provenance)
                .map_err(|e| EngineError::new("serialization_failure", e))?,
        ];
        out.push_str(
            &fields
                .iter()
                .map(|s| format!("\"{}\"", s.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    Ok(out)
}
/// Verify immutable response contents against the retained operation, not just valid numbers.
pub fn verify_response(response: &crate::engine::Response) -> Result<()> {
    use crate::{domain::Operation, engine::Output};
    if response.version != 1
        || response.request.version != 1
        || response.request.actor.trim().is_empty()
    {
        return Err(invalid("Invalid annotation response envelope"));
    }
    match (&response.request.operation, &response.output) {
        (
            Operation::ExportFeatureAnnotations { ledger },
            Output::FeatureAnnotationTable { csv: table },
        ) if csv(ledger)? == *table => Ok(()),
        (operation, Output::FeatureAnnotations { ledger: actual }) => {
            verify(actual)?;
            let mut expected = match operation {
                Operation::AddFeatureHypothesis {
                    ledger,
                    hypothesis,
                    expected_revision,
                } => add(ledger, *hypothesis.clone(), *expected_revision)?,
                Operation::ReviewFeatureAnnotation {
                    ledger,
                    hypothesis_id,
                    expected_revision,
                    reason,
                    decision,
                } => {
                    let mut next = review(
                        ledger,
                        *hypothesis_id,
                        *expected_revision,
                        &response.request.actor,
                        reason,
                        *decision,
                    )?;
                    if next.reviews.len() != actual.reviews.len() {
                        return Err(invalid("Review response count differs"));
                    }
                    let generated = next.reviews.last_mut().unwrap();
                    let retained = actual.reviews.last().unwrap();
                    generated.id = retained.id;
                    generated.unix_ms = retained.unix_ms;
                    next
                }
                Operation::ProposeFeatureAnnotations {
                    ledger,
                    config,
                    expected_revision,
                } => {
                    let mut next = propose(ledger, config, *expected_revision)?;
                    if next.hypotheses.len() != actual.hypotheses.len() {
                        return Err(invalid("Hypothesis response count differs"));
                    }
                    for (generated, retained) in next
                        .hypotheses
                        .iter_mut()
                        .zip(&actual.hypotheses)
                        .skip(ledger.hypotheses.len())
                    {
                        generated.id = retained.id;
                        if let (Some(g), Some(r)) = (&mut generated.msms, &retained.msms) {
                            g.id = r.id;
                        }
                    }
                    next
                }
                _ => return Err(invalid("Annotation response operation mismatch")),
            };
            expected.adduct_relationships = relationships(&expected);
            let encode = |l: &Ledger| {
                serde_json::to_value(l).map_err(|e| EngineError::new("serialization_failure", e))
            };
            if encode(&expected)? != encode(actual)? {
                return Err(EngineError::new(
                    "corrupt_artifact",
                    "Annotation operation replay differs",
                ));
            }
            Ok(())
        }
        _ => Err(invalid("Annotation response operation mismatch")),
    }
}
