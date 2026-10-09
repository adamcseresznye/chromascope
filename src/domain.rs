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
