//! Durable identities, units and typed analytical requests.
use serde::{Deserialize, Serialize};
macro_rules! id {
    ($($name:ident),*) => {$ (
        #[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
        #[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(#[cfg_attr(feature = "mcp-headless", schemars(with = "String"))] pub uuid::Uuid);
        impl Default for $name { fn default() -> Self { Self(uuid::Uuid::new_v4()) } }
    )*};
}
id!(ProjectId, DatasetId, ResultId, MethodId, OperationId, JobId);
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct Minutes(pub f64);
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct IntensityMinutes(pub f64);
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    AnalyzeStatistics {
        table: Box<crate::statistics::Table>,
        settings: crate::statistics::Settings,
    },
    ExportStatistics {
        report: Box<crate::statistics::Report>,
    },
    ProposeFeatureAnnotations {
        ledger: Box<crate::annotation::Ledger>,
        config: crate::annotation::ProposalConfig,
        expected_revision: usize,
    },
    AddFeatureHypothesis {
        ledger: Box<crate::annotation::Ledger>,
        hypothesis: Box<crate::annotation::Hypothesis>,
        expected_revision: usize,
    },
    ReviewFeatureAnnotation {
        ledger: Box<crate::annotation::Ledger>,
        #[cfg_attr(feature = "mcp-headless", schemars(with = "String"))]
        hypothesis_id: uuid::Uuid,
        expected_revision: usize,
        reason: String,
        decision: crate::annotation::Decision,
    },
    ExportFeatureAnnotations {
        ledger: Box<crate::annotation::Ledger>,
    },
    UntargetedBatch {
        config: crate::untargeted::Config,
    },
    ExportFeatureMatrix {
        report: Box<crate::untargeted::Report>,
    },
    LinkedSpectralChromatogram {
        query: Box<crate::spectral::Processed>,
        params: crate::processing::ProcessingParams,
    },
    CompareSpectra {
        query: crate::spectral::Spectrum,
        reference: crate::spectral::Spectrum,
        tolerance: crate::spectral::Tolerance,
    },
    ExportSpectralCandidates {
        report: Box<crate::spectral::SearchReport>,
    },
    InspectSpectra {
        indices: Vec<usize>,
        background_indices: Vec<usize>,
        config: crate::spectral::Processing,
    },
    ProcessSpectra {
        spectra: Vec<crate::spectral::Spectrum>,
        background: Vec<crate::spectral::Spectrum>,
        config: crate::spectral::Processing,
    },
    ImportSpectralLibrary {
        text: String,
        format: String,
        source: crate::spectral::LibrarySource,
    },
    SearchSpectralLibrary {
        query: Box<crate::spectral::Processed>,
        library: Box<crate::spectral::Library>,
        config: crate::spectral::SearchConfig,
    },
    AnnotateSpectrum {
        report: Box<crate::spectral::SearchReport>,
        expected_revision: usize,
        annotation: crate::spectral::Annotation,
    },
    FormulaCandidates {
        config: crate::spectral::FormulaConfig,
    },
    AnalyzeIsotopes {
        spectrum: crate::spectral::Spectrum,
        config: crate::spectral::IsotopeConfig,
    },

    ValidateMethod {
        study: crate::qc::Study,
    },
    ReviewQc {
        report: Box<crate::qc::Report>,
        expected_revision: usize,
        rule_id: String,
        reason: String,
        acknowledged: bool,
    },
    EvaluateQc {
        study: crate::qc::Study,
    },
    EvaluateTargetedQc {
        batch: Box<crate::targeted::BatchResult>,
        rules: Vec<crate::qc::Rule>,
    },
    ExportQc {
        report: Box<crate::qc::Report>,
    },
    ExportChromatographicPeaks {
        analyses: Vec<crate::chromatography::Analysis>,
    },
    ProcessChromatograms {
        traces: Vec<crate::chromatography::Trace>,
        config: crate::chromatography::Config,
    },
    ReviseChromatogram {
        analysis: Box<crate::chromatography::Analysis>,
        expected_revision: usize,
        correction: crate::chromatography::Correction,
        preview: bool,
    },
    TargetedBatch {
        batch: crate::targeted::BatchRequest,
    },
    ReviewTargeted {
        batch: Box<crate::targeted::BatchResult>,
        expected_revision: usize,
        sample: String,
        target: String,
        accepted: bool,
        reason: String,
    },
    ExportTargeted {
        batch: Box<crate::targeted::BatchResult>,
    },
    Metadata,
    Spectrum {
        index: usize,
    },
    Extract {
        params: crate::processing::ProcessingParams,
    },
    Integrate {
        points: Vec<[f64; 2]>,
        start: Minutes,
        end: Minutes,
    },
    Quantify {
        method: crate::quant::Method,
    },
}
impl Operation {
    /// Operations of the hidden Identification, Untargeted and Statistics workflows.
    /// They stay in the enum so stored projects keep deserializing.
    pub fn is_experimental(&self) -> bool {
        matches!(
            self,
            Self::AnalyzeStatistics { .. }
                | Self::ExportStatistics { .. }
                | Self::ProposeFeatureAnnotations { .. }
                | Self::AddFeatureHypothesis { .. }
                | Self::ReviewFeatureAnnotation { .. }
                | Self::ExportFeatureAnnotations { .. }
                | Self::UntargetedBatch { .. }
                | Self::ExportFeatureMatrix { .. }
                | Self::LinkedSpectralChromatogram { .. }
                | Self::CompareSpectra { .. }
                | Self::ExportSpectralCandidates { .. }
                | Self::InspectSpectra { .. }
                | Self::ProcessSpectra { .. }
                | Self::ImportSpectralLibrary { .. }
                | Self::SearchSpectralLibrary { .. }
                | Self::AnnotateSpectrum { .. }
                | Self::FormulaCandidates { .. }
                | Self::AnalyzeIsotopes { .. }
        )
    }

    /// Snake-case names (the serde tag) of the operations hidden from clients.
    pub const EXPERIMENTAL_NAMES: &'static [&'static str] = &[
        "analyze_statistics",
        "export_statistics",
        "propose_feature_annotations",
        "add_feature_hypothesis",
        "review_feature_annotation",
        "export_feature_annotations",
        "untargeted_batch",
        "export_feature_matrix",
        "linked_spectral_chromatogram",
        "compare_spectra",
        "export_spectral_candidates",
        "inspect_spectra",
        "process_spectra",
        "import_spectral_library",
        "search_spectral_library",
        "annotate_spectrum",
        "formula_candidates",
        "analyze_isotopes",
    ];

    /// Reject hidden operations unless the `experimental` feature is enabled.
    ///
    /// The crate's own unit tests (`cfg(test)`) are exempt: they exercise the always-compiled
    /// experimental code through the GUI and engine in the default build. Integration tests
    /// link the library without `cfg(test)`, so `tests/experimental_gate.rs` and
    /// `tests/engine_mcp.rs` still verify the gate itself.
    pub fn ensure_available(&self) -> Result<()> {
        if !crate::EXPERIMENTAL && !cfg!(test) && self.is_experimental() {
            return Err(EngineError::new(
                "experimental_unavailable",
                "this operation belongs to an experimental workflow that is not enabled in this build",
            ));
        }
        Ok(())
    }
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub operation_id: OperationId,
    pub actor: String,
    pub operation: Operation,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct EngineError {
    pub code: String,
    pub message: String,
}
impl EngineError {
    pub fn new(code: &str, message: impl ToString) -> Self {
        Self {
            code: code.into(),
            message: message.to_string(),
        }
    }
}
pub type Result<T> = std::result::Result<T, EngineError>;

#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataState {
    Present,
    Missing,
}

#[cfg(test)]
mod tests {
    use super::Operation;

    fn snake(name: &str) -> String {
        let mut out = String::new();
        for (i, c) in name.chars().enumerate() {
            if c.is_ascii_uppercase() {
                if i > 0 {
                    out.push('_');
                }
                out.push(c.to_ascii_lowercase());
            } else {
                out.push(c);
            }
        }
        out
    }

    /// `is_experimental` and `EXPERIMENTAL_NAMES` are two hand-written lists of the same set;
    /// read both from the source so they cannot drift apart unnoticed.
    #[test]
    fn is_experimental_agrees_with_experimental_names() {
        let source = include_str!("domain.rs");
        let body = &source[source.find("pub fn is_experimental").unwrap()..];
        let body = &body[body.find("matches!(").unwrap()..];
        let body = &body[..body.find("\n        )\n").unwrap()];
        let mut from_match: Vec<String> = body
            .split("Self::")
            .skip(1)
            .map(|v| snake(v.split(|c: char| !c.is_alphanumeric()).next().unwrap()))
            .collect();
        from_match.sort();
        let mut names: Vec<String> = Operation::EXPERIMENTAL_NAMES
            .iter()
            .map(|s| s.to_string())
            .collect();
        names.sort();
        assert_eq!(names.len(), 18);
        assert_eq!(from_match, names);

        // Every listed name is a real serde tag of an Operation variant.
        let enum_start = source.find("pub enum Operation {").unwrap();
        let enum_body =
            &source[enum_start..source[enum_start..].find("\n}\n").unwrap() + enum_start];
        let variants: Vec<String> = enum_body
            .lines()
            .filter(|l| l.starts_with("    ") && !l.starts_with("     "))
            .filter_map(|l| {
                let word = l.trim().split(|c: char| !c.is_alphanumeric()).next()?;
                word.chars()
                    .next()?
                    .is_ascii_uppercase()
                    .then(|| snake(word))
            })
            .collect();
        for name in Operation::EXPERIMENTAL_NAMES {
            assert!(
                variants.iter().any(|v| v == name),
                "{name} is not an Operation variant"
            );
        }
        assert!(
            variants.len() > 18,
            "non-experimental operations must remain"
        );
    }
}
