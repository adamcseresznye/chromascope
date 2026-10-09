//! Shared analytical service; GUI compatibility APIs and headless clients use the same kernels.
use crate::{
    domain::*,
    jobs::JobControl,
    parser::{ChromatogramData, MassSpectrum, MzData},
    processing::{self, ProcessingParams},
    quant,
    validation::DataBounds,
};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Output {
    Statistics {
        report: Box<crate::statistics::Report>,
    },
    StatisticsTable {
        csv: String,
    },
    FeatureAnnotations {
        ledger: Box<crate::annotation::Ledger>,
    },
    FeatureAnnotationTable {
        csv: String,
    },
    FeatureMatrix {
        report: Box<crate::untargeted::Report>,
    },
    FeatureMatrixTable {
        csv: String,
        filtered_csv: String,
        observations_csv: String,
    },
    SpectralComparison {
        query: crate::spectral::Spectrum,
        reference: crate::spectral::Spectrum,
        tolerance: crate::spectral::Tolerance,
        similarity: crate::spectral::Similarity,
    },
    SpectralCandidateTable {
        csv: String,
    },
    SpectralProcessing {
        processed: Box<crate::spectral::Processed>,
    },
    SpectralLibrary {
        library: Box<crate::spectral::Library>,
    },
    SpectralSearch {
        report: Box<crate::spectral::SearchReport>,
    },
    FormulaCandidates {
        report: crate::spectral::FormulaReport,
    },
    IsotopeAnalysis {
        report: crate::spectral::IsotopeReport,
    },

    QcReport {
        report: Box<crate::qc::Report>,
    },
    QcTable {
        csv: String,
    },
    TargetedQuantification {
        batch: Box<crate::targeted::BatchResult>,
    },
    TargetedTable {
        csv: String,
        calibration_csv: String,
    },
    ChromatographicPeakTable {
        csv: String,
        area_unit: String,
    },
    ChromatographicProcessing {
        analyses: Vec<crate::chromatography::Analysis>,
        preview: bool,
    },
    Metadata {
        bounds: DataBounds,
        retention_time_unit: String,
        mass_unit: String,
    },
    Spectrum {
        state: DataState,
        spectrum: MassSpectrum,
        metadata: serde_json::Value,
        retention_time_unit: String,
        intensity_unit: String,
    },
    Chromatogram {
        state: DataState,
        points: Vec<[f64; 2]>,
        raw: ChromatogramData,
        retention_time_unit: String,
        intensity_unit: String,
    },
    Integration {
        area_unit: String,
        area: IntensityMinutes,
        algorithm: String,
    },
    Quantification {
        measurements: Vec<quant::Measurement>,
        area_unit: String,
    },
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Serialize, Deserialize)]
pub struct Response {
    pub version: u32,
    pub result_id: ResultId,
    pub request: Request,
    pub kernel_version: String,
    pub source_sha256: Option<String>,
    pub started_unix_ms: u128,
    pub finished_unix_ms: u128,
    pub output: Output,
}
fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
pub fn validate_points(points: &[[f64; 2]]) -> Result<()> {
    if points
        .iter()
        .any(|p| !p[0].is_finite() || !p[1].is_finite() || p[0] < 0.0)
        || points.windows(2).any(|w| w[0][0] > w[1][0])
    {
        return Err(EngineError::new(
            "invalid_parameters",
            "Trace must contain finite intensities and sorted nonnegative RT minutes",
        ));
    }
    Ok(())
}
pub fn validate_params(p: &ProcessingParams) -> crate::error::Result<()> {
    use crate::error::ChromascopeError as E;
    if p.smoothing > 10 {
        return Err(E::InvalidSmoothingWindow(p.smoothing));
    }
    if p.ms_level == 0
        || p.precursor_mz.is_some_and(|m| !m.is_finite() || m <= 0.0)
        || p.mz_range
            .is_some_and(|(a, b)| !a.is_finite() || !b.is_finite() || a < 0.0 || a >= b)
    {
        return Err(E::MzDataError("Invalid extraction parameters".into()));
    }
    if p.plot_type == crate::plotting_parameters::PlotType::Xic {
        let x = p.xic_params.as_ref().ok_or(E::MissingXicParams)?;
        crate::validation::XicParams::new(
            x.mass(),
            x.polarity(),
            x.mass_tolerance(),
            &DataBounds::unrestricted(),
        )?;
        if p.mz_range.is_some() || x.polarity() != p.polarity {
            return Err(E::MzDataError("Conflicting XIC filters".into()));
        }
    }
    Ok(())
}
pub fn execute(path: &Path, request: Request, control: &JobControl) -> Result<Response> {
    control.check()?;
    if request.version != 1 || request.actor.trim().is_empty() {
        return Err(EngineError::new(
            "invalid_parameters",
            "Use request version 1 and a nonempty actor",
        ));
    }
    let started = now();
    let mut source_sha256 = None;
    let fail = |e| EngineError::new("adapter_failure", e);
    let output = if let Operation::AnalyzeStatistics { table, settings } = &request.operation {
        Output::Statistics {
            report: Box::new(crate::statistics::analyze(table, settings, control)?),
        }
    } else if let Operation::ExportStatistics { report } = &request.operation {
        Output::StatisticsTable {
            csv: crate::statistics::export_csv(report)?,
        }
    } else if let Operation::ProposeFeatureAnnotations {
        ledger,
        config,
        expected_revision,
    } = &request.operation
    {
        Output::FeatureAnnotations {
            ledger: Box::new(crate::annotation::propose(
                ledger,
                config,
                *expected_revision,
            )?),
        }
    } else if let Operation::AddFeatureHypothesis {
        ledger,
        hypothesis,
        expected_revision,
    } = &request.operation
    {
        Output::FeatureAnnotations {
            ledger: Box::new(crate::annotation::add(
                ledger,
                *hypothesis.clone(),
                *expected_revision,
            )?),
        }
    } else if let Operation::ReviewFeatureAnnotation {
        ledger,
        hypothesis_id,
        expected_revision,
        reason,
        decision,
    } = &request.operation
    {
        Output::FeatureAnnotations {
            ledger: Box::new(crate::annotation::review(
                ledger,
                *hypothesis_id,
                *expected_revision,
                &request.actor,
                reason,
                *decision,
            )?),
        }
    } else if let Operation::ExportFeatureAnnotations { ledger } = &request.operation {
        Output::FeatureAnnotationTable {
            csv: crate::annotation::csv(ledger)?,
        }
    } else if let Operation::UntargetedBatch { config } = &request.operation {
        Output::FeatureMatrix {
            report: Box::new(crate::untargeted::run(config, control)?),
        }
    } else if let Operation::ExportFeatureMatrix { report } = &request.operation {
        Output::FeatureMatrixTable {
            csv: crate::untargeted::csv(report)?,
            filtered_csv: crate::untargeted::matrix_csv(report, true)?,
            observations_csv: crate::untargeted::observations_csv(report)?,
        }
    } else if let Some(output) = crate::spectral::execute_operation(&request)? {
        output
    } else if let Operation::ReviewQc {
        report,
        expected_revision,
        rule_id,
        reason,
        acknowledged,
    } = &request.operation
    {
        Output::QcReport {
            report: Box::new(crate::qc::review(
                report,
                *expected_revision,
                rule_id,
                &request.actor,
                reason,
                *acknowledged,
            )?),
        }
    } else if let Operation::ValidateMethod { study } = &request.operation {
        if study.purpose != "method_validation" {
            return Err(EngineError::new(
                "invalid_parameters",
                "Method validation requires an explicit method_validation study design",
            ));
        }
        Output::QcReport {
            report: Box::new(crate::qc::evaluate(study.clone())?),
        }
    } else if let Operation::EvaluateQc { study } = &request.operation {
        Output::QcReport {
            report: Box::new(crate::qc::evaluate(study.clone())?),
        }
    } else if let Operation::EvaluateTargetedQc { batch, rules } = &request.operation {
        Output::QcReport {
            report: Box::new(crate::qc::evaluate(crate::qc::targeted_study(
                batch,
                rules.clone(),
            )?)?),
        }
    } else if let Operation::ExportQc { report } = &request.operation {
        Output::QcTable {
            csv: crate::qc::csv(report)?,
        }
    } else if let Operation::TargetedBatch { batch } = &request.operation {
        Output::TargetedQuantification {
            batch: Box::new(crate::targeted::run(batch.clone(), control)?),
        }
    } else if let Operation::ReviewTargeted {
        batch,
        expected_revision,
        sample,
        target,
        accepted,
        reason,
    } = &request.operation
    {
        Output::TargetedQuantification {
            batch: Box::new(crate::targeted::review(
                batch,
                *expected_revision,
                sample,
                target,
                *accepted,
                &request.actor,
                reason,
            )?),
        }
    } else if let Operation::ExportTargeted { batch } = &request.operation {
        Output::TargetedTable {
            csv: crate::targeted::csv(batch)?,
            calibration_csv: crate::targeted::calibration_csv(batch)?,
        }
    } else if let Operation::ExportChromatographicPeaks { analyses } = &request.operation {
        if analyses.is_empty() || analyses.len() > 64 {
            return Err(EngineError::new(
                "resource_limit",
                "Export requires 1..64 analyses",
            ));
        }
        for a in analyses {
            control.check()?;
            crate::chromatography::verify(a)?;
        }
        Output::ChromatographicPeakTable {
            csv: crate::chromatography::peaks_csv(analyses),
            area_unit: "instrument intensity * minute".into(),
        }
    } else if let Operation::ProcessChromatograms { traces, config } = &request.operation {
        if traces.is_empty()
            || traces.len() > 64
            || traces.iter().map(|t| t.samples.len()).sum::<usize>() > 1_000_000
        {
            return Err(EngineError::new(
                "resource_limit",
                "Batch requires 1..64 traces and at most one million total samples",
            ));
        }
        let mut analyses = Vec::new();
        for trace in traces {
            control.check()?;
            analyses.push(crate::chromatography::process(
                trace.clone(),
                config.clone(),
            )?);
        }
        Output::ChromatographicProcessing {
            analyses,
            preview: false,
        }
    } else if let Operation::ReviseChromatogram {
        analysis,
        expected_revision,
        correction,
        preview,
    } = &request.operation
    {
        Output::ChromatographicProcessing {
            analyses: vec![crate::chromatography::revise(
                analysis,
                *expected_revision,
                correction.clone(),
                &request.actor,
                *preview,
            )?],
            preview: *preview,
        }
    } else if let Operation::Integrate { points, start, end } = &request.operation {
        validate_points(points)?;
        if points.is_empty()
            || !start.0.is_finite()
            || !end.0.is_finite()
            || start.0 >= end.0
            || start.0 < points[0][0]
            || end.0 > points[points.len() - 1][0]
        {
            return Err(EngineError::new(
                "invalid_parameters",
                "Integration interval must be inside the trace",
            ));
        }
        let area = processing::integrate_peak(points, start.0, end.0)
            .map_err(|e| EngineError::new("invalid_parameters", e))?;
        Output::Integration {
            area_unit: "instrument intensity * minute".into(),
            area: IntensityMinutes(area),
            algorithm: "chord_trapezoid_v1".into(),
        }
    } else {
        let metadata =
            std::fs::metadata(path).map_err(|e| EngineError::new("missing_source", e))?;
        if metadata.len() > control.max_source_bytes {
            return Err(EngineError::new(
                "resource_limit",
                "Source exceeds byte budget",
            ));
        }
        source_sha256 = Some(crate::project::source_identity_controlled(path, Some(control))?.0);
        control.check()?;
        let mut data = MzData::new();
        data.job_control = Some(control.clone());
        let opened = data.open_msfile(&path.to_path_buf()).map(|_| ());
        control.check()?;
        opened.map_err(|e| fail(e.to_string()))?;
        control.check()?;
        if data.bounds.scan_count > control.max_scans {
            return Err(EngineError::new(
                "resource_limit",
                "Source exceeds scan budget",
            ));
        }
        control.reset_progress();
        match &request.operation {
            Operation::InspectSpectra {
                indices,
                background_indices,
                config,
            } => {
                if indices.is_empty()
                    || indices.len() > 256
                    || background_indices.len() > 256
                    || indices
                        .iter()
                        .chain(background_indices)
                        .any(|i| *i >= data.bounds.scan_count)
                {
                    return Err(EngineError::new(
                        "invalid_parameters",
                        "Select 1..256 signal scans and <=256 background scans within source",
                    ));
                }
                let unique: std::collections::BTreeSet<_> =
                    indices.iter().chain(background_indices).collect();
                if unique.len() != indices.len() + background_indices.len() {
                    return Err(EngineError::new("invalid_parameters", "Signal/background scan sets must be disjoint and contain no duplicate indices"));
                }
                let mut total_points = 0usize;
                let mut spectra = Vec::new();
                let mut background = Vec::new();
                for (selected, destination) in [
                    (indices, &mut spectra),
                    (background_indices, &mut background),
                ] {
                    for index in selected {
                        control.check()?;
                        let scan = data
                            .spectral_scan(*index)
                            .map_err(|e| fail(e.to_string()))?;
                        total_points += scan.peaks.len();
                        if total_points > 1_000_000 {
                            return Err(EngineError::new(
                                "resource_limit",
                                "Inspection exceeds one million total points",
                            ));
                        }
                        destination.push(scan);
                    }
                }
                for scan in spectra.iter_mut().chain(&mut background) {
                    if let Some(hash) = &source_sha256 {
                        scan.metadata.insert("source_sha256".into(), hash.clone());
                    }
                }
                Output::SpectralProcessing {
                    processed: Box::new(crate::spectral::process(
                        spectra,
                        background,
                        config.clone(),
                    )?),
                }
            }
            Operation::Metadata => Output::Metadata {
                bounds: data.bounds,
                retention_time_unit: "minute".into(),
                mass_unit: "m/z".into(),
            },
            Operation::Spectrum { index } => {
                if *index >= data.bounds.scan_count {
                    return Err(EngineError::new(
                        "invalid_parameters",
                        "Scan index outside dataset",
                    ));
                }
                let metadata = data
                    .scan_metadata_structured(*index)
                    .map_err(|e| fail(e.to_string()))?;
                let spectrum = data
                    .get_mass_spectrum_by_index(*index)
                    .map_err(|e| fail(e.to_string()))?;
                if spectrum.mz.len() != spectrum.intensity.len()
                    || spectrum.mz.iter().any(|v| !v.is_finite())
                    || spectrum.intensity.iter().any(|v| !v.is_finite())
                {
                    return Err(EngineError::new(
                        "adapter_failure",
                        "Invalid spectrum arrays",
                    ));
                }
                Output::Spectrum {
                    state: if spectrum.mz.is_empty() {
                        DataState::Missing
                    } else {
                        DataState::Present
                    },
                    spectrum,
                    metadata,
                    retention_time_unit: "minute".into(),
                    intensity_unit: "instrument intensity".into(),
                }
            }
            Operation::Extract { params }
            | Operation::LinkedSpectralChromatogram { params, .. } => {
                if let Operation::LinkedSpectralChromatogram { query, .. } = &request.operation {
                    crate::spectral::verify_processed(query)?;
                    if query
                        .inputs
                        .iter()
                        .any(|scan| scan.metadata.get("source_sha256") != source_sha256.as_ref())
                    {
                        return Err(EngineError::new(
                            "invalid_parameters",
                            "Linked chromatogram source differs from retained spectrum acquisition",
                        ));
                    }
                    let precursor = query.spectrum.precursor_mz.ok_or_else(|| {
                        EngineError::new(
                            "invalid_parameters",
                            "Linked precursor XIC requires precursor m/z",
                        )
                    })?;
                    if params.ms_level != 1
                        || params.plot_type != crate::plotting_parameters::PlotType::Xic
                        || params.xic_params.as_ref().is_none_or(|x| {
                            (x.mass() - precursor).abs() > query.config.tolerance.da(precursor)
                        })
                    {
                        return Err(EngineError::new(
                            "invalid_parameters",
                            "Linked evidence must be MS1 XIC of the retained precursor",
                        ));
                    }
                    let expected_polarity = match query.spectrum.polarity {
                        crate::spectral::Polarity::Positive => {
                            mzdata::spectrum::ScanPolarity::Positive
                        }
                        crate::spectral::Polarity::Negative => {
                            mzdata::spectrum::ScanPolarity::Negative
                        }
                        crate::spectral::Polarity::Unknown => {
                            mzdata::spectrum::ScanPolarity::Unknown
                        }
                    };
                    if params.polarity != expected_polarity {
                        return Err(EngineError::new(
                            "invalid_parameters",
                            "Linked XIC polarity conflicts with spectrum",
                        ));
                    }
                }
                validate_params(params).map_err(|e| EngineError::new("invalid_parameters", e))?;
                let extraction = processing::process_chromatogram(&mut data, params);
                control.check()?;
                let (points, raw) = extraction.map_err(|e| fail(e.to_string()))?;
                validate_points(&points)?;
                Output::Chromatogram {
                    state: if points.is_empty() {
                        DataState::Missing
                    } else {
                        DataState::Present
                    },
                    points,
                    raw,
                    retention_time_unit: "minute".into(),
                    intensity_unit: "instrument intensity".into(),
                }
            }
            Operation::Quantify { method } => {
                let params = quant::validate(method)
                    .map_err(|e| EngineError::new("invalid_parameters", e))?;
                let mut measurements = Vec::new();
                for (a, p) in method.analytes.iter().zip(params) {
                    control.check()?;
                    control.reset_progress();
                    let extraction = processing::process_chromatogram(&mut data, &p);
                    control.check()?;
                    let (trace, _) = extraction.map_err(|e| fail(e.to_string()))?;
                    validate_points(&trace)?;
                    let (peak, status) = quant::detect(&trace, a, method);
                    measurements.push(quant::Measurement {
                        chromatography: None,
                        sample: path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into(),
                        source: path.to_string_lossy().into(),
                        run: path.to_string_lossy().into(),
                        analyte: a.extraction.name.clone(),
                        params: p,
                        method: serde_json::to_string(method).map_err(|e| fail(e.to_string()))?,
                        trace,
                        automatic: peak.clone(),
                        peak,
                        automatic_status: status.clone(),
                        status,
                        diagnostic: String::new(),
                    });
                }
                Output::Quantification {
                    measurements,
                    area_unit: "instrument intensity * minute".into(),
                }
            }
            Operation::AnalyzeStatistics { .. }
            | Operation::ExportStatistics { .. }
            | Operation::ProposeFeatureAnnotations { .. }
            | Operation::AddFeatureHypothesis { .. }
            | Operation::ReviewFeatureAnnotation { .. }
            | Operation::ExportFeatureAnnotations { .. }
            | Operation::UntargetedBatch { .. }
            | Operation::ExportFeatureMatrix { .. }
            | Operation::CompareSpectra { .. }
            | Operation::ExportSpectralCandidates { .. }
            | Operation::ProcessSpectra { .. }
            | Operation::ImportSpectralLibrary { .. }
            | Operation::SearchSpectralLibrary { .. }
            | Operation::AnnotateSpectrum { .. }
            | Operation::FormulaCandidates { .. }
            | Operation::AnalyzeIsotopes { .. }
            | Operation::TargetedBatch { .. }
            | Operation::EvaluateQc { .. }
            | Operation::ReviewQc { .. }
            | Operation::ValidateMethod { .. }
            | Operation::EvaluateTargetedQc { .. }
            | Operation::ExportQc { .. }
            | Operation::ReviewTargeted { .. }
            | Operation::ExportTargeted { .. }
            | Operation::Integrate { .. }
            | Operation::ProcessChromatograms { .. }
            | Operation::ExportChromatographicPeaks { .. }
            | Operation::ReviseChromatogram { .. } => unreachable!(),
        }
    };
    control.check()?;
    let linked_spectral = matches!(
        &request.operation,
        Operation::LinkedSpectralChromatogram { .. }
    );
    Ok(Response {
        version: 1,
        result_id: ResultId::default(),
        request,
        kernel_version: format!(
            "chromascope/{}/{}",
            env!("CARGO_PKG_VERSION"),
            if matches!(
                output,
                Output::Statistics { .. } | Output::StatisticsTable { .. }
            ) {
                "statistics-v1"
            } else if matches!(
                output,
                Output::FeatureMatrix { .. } | Output::FeatureMatrixTable { .. }
            ) {
                "openms-metabo-v1"
            } else if linked_spectral
                || matches!(
                    output,
                    Output::SpectralComparison { .. }
                        | Output::SpectralCandidateTable { .. }
                        | Output::SpectralProcessing { .. }
                        | Output::SpectralLibrary { .. }
                        | Output::SpectralSearch { .. }
                        | Output::FormulaCandidates { .. }
                        | Output::IsotopeAnalysis { .. }
                )
            {
                "spectral-v1"
            } else if matches!(output, Output::QcReport { .. } | Output::QcTable { .. }) {
                "qc-v1"
            } else if matches!(
                output,
                Output::TargetedQuantification { .. } | Output::TargetedTable { .. }
            ) {
                "targeted-v1"
            } else if matches!(
                output,
                Output::ChromatographicProcessing { .. } | Output::ChromatographicPeakTable { .. }
            ) {
                "chromatography-v1"
            } else if matches!(
                output,
                Output::Metadata { .. }
                    | Output::Chromatogram { .. }
                    | Output::Quantification { .. }
            ) {
                // Mixed-metadata repair and complete RT extrema change defective
                // source outputs. Preserve v1 results; do not silently replay them.
                "extraction-v2"
            } else {
                "legacy-v1"
            }
        ),
        source_sha256,
        started_unix_ms: started,
        finished_unix_ms: now(),
        output,
    })
}
