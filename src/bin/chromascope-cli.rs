//! Headless JSON adapter. No UI, display server or converter is required for mzML.
use chromascope::{
    domain::{EngineError, Request},
    engine,
    jobs::JobControl,
    project::Project,
};
use std::{io::Read, path::Path};
const HELP: &str = "Chromascope shared-engine JSON CLI
Usage:
  run DATASET|- [PROJECT] < request.json
  project-create DIRECTORY
  project-inspect DIRECTORY
  project-restore DIRECTORY REVISION
  project-import-method DIRECTORY METHOD.toml
  project-relink DIRECTORY DATASET_UUID SOURCE
  project-report PROJECT NEW_DIRECTORY [CONFIG.json]
  project-bundle PROJECT NEW_DIRECTORY [CONFIG.json]
  project-verify BUNDLE
  project-sources PROJECT
  project-reprocess PROJECT RESULT_UUID
  project-compare-methods LEFT.json RIGHT.json
  project-migrate PROJECT
  export-peaks|export-targeted|export-targeted-calibration|export-qc|
  export-spectral|export-features|export-features-filtered|
  export-feature-observations|export-annotations|export-statistics NEW_FILE < response.json

Requests use version 1, operation_id UUID, actor, and tagged operation.
Results/errors are JSON ok/result or ok/error envelopes; errors exit 1.
Progress is stderr. RT is minutes unless a field explicitly states seconds.
Exports refuse existing destinations. Project writes append revisions.
See README.md, examples/README.md and docs/MCP_ANALYSIS.md for schemas/workflows.";
fn run() -> Result<serde_json::Value, EngineError> {
    let control = JobControl::default();
    let interrupt = control.clone();
    ctrlc::set_handler(move || interrupt.cancel())
        .map_err(|e| EngineError::new("adapter_failure", e))?;
    let args: Vec<_> = std::env::args().skip(1).collect();
    let invalid = || EngineError::new("invalid_parameters", HELP);
    match args.first().map(String::as_str) {
        Some(
            "export-peaks"
            | "export-targeted"
            | "export-targeted-calibration"
            | "export-qc"
            | "export-spectral"
            | "export-features"
            | "export-features-filtered"
            | "export-feature-observations"
            | "export-annotations"
            | "export-statistics",
        ) if args.len() == 2 => {
            use std::io::Write;
            let mut input = String::new();
            std::io::stdin()
                .take(64 * 1024 * 1024 + 1)
                .read_to_string(&mut input)
                .map_err(|e| EngineError::new("invalid_parameters", e))?;
            if input.len() > 64 * 1024 * 1024 {
                return Err(EngineError::new(
                    "resource_limit",
                    "Export input exceeds 64 MiB",
                ));
            }
            let value: serde_json::Value = serde_json::from_str(&input)
                .map_err(|e| EngineError::new("invalid_parameters", e))?;
            let response: engine::Response =
                serde_json::from_value(value.get("result").cloned().unwrap_or(value))
                    .map_err(|e| EngineError::new("invalid_parameters", e))?;
            if args[0] == "export-annotations" {
                chromascope::annotation::verify_response(&response)?;
            }
            if args[0] == "export-statistics" {
                chromascope::statistics::verify_response(&response)?;
            }
            let (csv, rows) = match response.output {
                engine::Output::Statistics { report } if args[0] == "export-statistics" => (
                    chromascope::statistics::export_csv(&report)?,
                    report.numerics.feature_indices.len(),
                ),
                engine::Output::FeatureAnnotations { ledger }
                    if args[0] == "export-annotations" =>
                {
                    (
                        chromascope::annotation::csv(&ledger)?,
                        ledger.hypotheses.len(),
                    )
                }
                engine::Output::FeatureMatrix { report }
                    if args[0].starts_with("export-feature") =>
                {
                    (
                        if args[0] == "export-feature-observations" {
                            chromascope::untargeted::observations_csv(&report)?
                        } else {
                            chromascope::untargeted::matrix_csv(
                                &report,
                                args[0] == "export-features-filtered",
                            )?
                        },
                        if args[0] == "export-feature-observations" {
                            report.features.len() * report.config.samples.len()
                        } else if args[0] == "export-features-filtered" {
                            report
                                .features
                                .iter()
                                .filter(|f| {
                                    f.filter_state == chromascope::untargeted::FilterState::Included
                                })
                                .count()
                        } else {
                            report.features.len()
                        },
                    )
                }
                engine::Output::SpectralSearch { report } if args[0] == "export-spectral" => (
                    chromascope::spectral::candidates_csv(&report)?,
                    report.candidates.len(),
                ),
                engine::Output::QcReport { report } if args[0] == "export-qc" => {
                    (chromascope::qc::csv(&report)?, report.decisions.len())
                }
                engine::Output::TargetedQuantification { batch }
                    if args[0].starts_with("export-targeted") =>
                {
                    (
                        if args[0] == "export-targeted-calibration" {
                            chromascope::targeted::calibration_csv(&batch)?
                        } else {
                            chromascope::targeted::csv(&batch)?
                        },
                        batch.results.len(),
                    )
                }
                engine::Output::ChromatographicProcessing {
                    analyses,
                    preview: false,
                } if args[0] == "export-peaks" => {
                    for a in &analyses {
                        chromascope::chromatography::verify(a)?;
                    }
                    (
                        chromascope::chromatography::peaks_csv(&analyses),
                        analyses.len(),
                    )
                }
                _ => {
                    return Err(EngineError::new(
                        "invalid_parameters",
                        "Export requires the corresponding committed engine response",
                    ))
                }
            };
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&args[1])
                .and_then(|mut f| f.write_all(csv.as_bytes()))
                .map_err(|e| EngineError::new("adapter_failure", e))?;
            Ok(if args[0] == "export-peaks" {
                serde_json::json!({"path":args[1],"traces":rows})
            } else {
                serde_json::json!({"path":args[1],"rows":rows})
            })
        }
        Some("project-report" | "project-bundle") if args.len() == 3 || args.len() == 4 => {
            let config = if args.len() == 4 {
                serde_json::from_slice(
                    &std::fs::read(&args[3])
                        .map_err(|e| EngineError::new("invalid_parameters", e))?,
                )
                .map_err(|e| EngineError::new("invalid_parameters", e))?
            } else {
                chromascope::delivery::Config::default()
            };
            chromascope::delivery::export(
                Path::new(&args[1]),
                Path::new(&args[2]),
                &config,
                args[0] == "project-bundle",
            )
        }
        Some("project-verify") if args.len() == 2 => {
            chromascope::delivery::verify_bundle(Path::new(&args[1]))
        }
        Some("project-sources") if args.len() == 2 => Ok(chromascope::delivery::verification(
            &Project::open(Path::new(&args[1]))?,
        )),
        Some("project-reprocess") if args.len() == 3 => {
            let id = serde_json::from_value(serde_json::json!(args[2]))
                .map_err(|e| EngineError::new("invalid_parameters", e))?;
            chromascope::delivery::reprocess(Path::new(&args[1]), id)
        }
        Some("project-compare-methods") if args.len() == 3 => {
            let read = |path: &str| -> Result<serde_json::Value, EngineError> {
                serde_json::from_slice(
                    &std::fs::read(path).map_err(|e| EngineError::new("invalid_parameters", e))?,
                )
                .map_err(|e| EngineError::new("invalid_parameters", e))
            };
            Ok(chromascope::delivery::compare_methods(
                &read(&args[1])?,
                &read(&args[2])?,
            ))
        }
        Some("project-migrate") if args.len() == 2 => {
            let root = Path::new(&args[1]);
            let mut p = Project::open(root)?;
            p.commit(root, p.revision)?;
            Ok(
                serde_json::json!({"schema_version":p.schema_version,"revision":p.revision,"migration":"materialize current schema defaults; original snapshots preserved"}),
            )
        }
        Some("project-create") if args.len() == 2 => {
            serde_json::to_value(Project::create(Path::new(&args[1]))?)
                .map_err(|e| EngineError::new("adapter_failure", e))
        }
        Some("project-inspect") if args.len() == 2 => {
            serde_json::to_value(Project::open(Path::new(&args[1]))?)
                .map_err(|e| EngineError::new("adapter_failure", e))
        }
        Some("project-restore") if args.len() == 3 => {
            let root = Path::new(&args[1]);
            let current = Project::open(root)?;
            let target = args[2]
                .parse::<u64>()
                .map_err(|e| EngineError::new("invalid_parameters", e))?;
            serde_json::to_value(Project::restore_revision(root, target, current.revision)?)
                .map_err(|e| EngineError::new("adapter_failure", e))
        }
        Some("project-import-method") if args.len() == 3 => {
            let root = Path::new(&args[1]);
            let mut p = Project::open(root)?;
            let id = p.import_legacy_method(Path::new(&args[2]))?;
            p.commit(root, p.revision)?;
            Ok(serde_json::json!({"method_id":id,"revision":p.revision}))
        }
        Some("project-relink") if args.len() == 4 => {
            let root = Path::new(&args[1]);
            let mut project = Project::open(root)?;
            let id = serde_json::from_value(serde_json::json!(args[2]))
                .map_err(|e| EngineError::new("invalid_parameters", e))?;
            project.relink(id, Path::new(&args[3]))?;
            project.commit(root, project.revision)?;
            Ok(serde_json::json!({"dataset_id":id,"revision":project.revision}))
        }
        Some("run") if args.len() == 2 || args.len() == 3 => {
            let mut input = String::new();
            std::io::stdin()
                .take(16 * 1024 * 1024 + 1)
                .read_to_string(&mut input)
                .map_err(|e| EngineError::new("invalid_parameters", e))?;
            if input.len() > 16 * 1024 * 1024 {
                return Err(EngineError::new("resource_limit", "Request exceeds 16 MiB"));
            }
            let request: Request = serde_json::from_str(&input)
                .map_err(|e| EngineError::new("invalid_parameters", e))?;
            let preview = matches!(
                request.operation,
                chromascope::domain::Operation::ReviseChromatogram { preview: true, .. }
            );
            let targeted_samples = match &request.operation {
                chromascope::domain::Operation::TargetedBatch { batch } => Some(&batch.samples),
                chromascope::domain::Operation::ReviewTargeted { batch, .. } => {
                    Some(&batch.request.samples)
                }
                chromascope::domain::Operation::EvaluateTargetedQc { batch, .. } => {
                    Some(&batch.request.samples)
                }
                chromascope::domain::Operation::EvaluateQc { study }
                | chromascope::domain::Operation::ValidateMethod { study } => {
                    study.targeted_evidence.as_ref().map(|b| &b.request.samples)
                }
                chromascope::domain::Operation::ReviewQc { report, .. }
                | chromascope::domain::Operation::ExportQc { report } => report
                    .study
                    .targeted_evidence
                    .as_ref()
                    .map(|b| &b.request.samples),
                _ => None,
            };
            let statistical_table = match &request.operation {
                chromascope::domain::Operation::AnalyzeStatistics { table, .. } => {
                    Some(table.as_ref())
                }
                chromascope::domain::Operation::ExportStatistics { report } => Some(&report.table),
                _ => None,
            };
            let statistical_sources: Vec<String> = if let Some(table) = statistical_table {
                if let Some(matrix) = &table.matrix {
                    matrix
                        .config
                        .samples
                        .iter()
                        .map(|s| s.source.clone())
                        .collect()
                } else if let Some(value) = &table.targeted {
                    let batch: chromascope::targeted::BatchResult =
                        serde_json::from_value(value.clone())
                            .map_err(|e| EngineError::new("invalid_parameters", e))?;
                    batch
                        .request
                        .samples
                        .iter()
                        .map(|s| s.source.clone())
                        .collect()
                } else {
                    vec![]
                }
            } else {
                vec![]
            };
            let primary_source = if args[1] == "-" {
                if let Some(source) = statistical_sources.first() {
                    source.as_str()
                } else if let chromascope::domain::Operation::UntargetedBatch { config } =
                    &request.operation
                {
                    config
                        .samples
                        .first()
                        .map(|s| s.source.as_str())
                        .unwrap_or(&args[1])
                } else {
                    targeted_samples
                        .and_then(|samples| samples.first())
                        .map(|s| s.source.as_str())
                        .unwrap_or(&args[1])
                }
            } else {
                &args[1]
            };
            let source = Path::new(primary_source);
            let mut project = if args.len() == 3 {
                Some(Project::open(Path::new(&args[2]))?)
            } else {
                None
            };
            let dataset = if let Some(p) = project.as_mut().filter(|_| !preview) {
                for source in &statistical_sources {
                    p.register(Path::new(source))?;
                }
                if let chromascope::domain::Operation::UntargetedBatch { config } =
                    &request.operation
                {
                    for sample in &config.samples {
                        p.register(Path::new(&sample.source))?;
                    }
                }
                if let Some(samples) = targeted_samples {
                    for sample in samples {
                        p.register(Path::new(&sample.source))?;
                    }
                }
                Some(p.register(source)?)
            } else {
                None
            };
            let scheduler = chromascope::jobs::Scheduler::new(1, 1)?;
            let job = scheduler.submit(source.to_path_buf(), request.clone(), control)?;
            let mut progress = 0;
            let outcome = loop {
                if let Some(result) = job.try_result() {
                    break result;
                }
                let current = job.control.completed_scans();
                if current != progress {
                    eprintln!("Processing progress: {current} completed scan/sample phases");
                    progress = current;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            };
            let response = match outcome {
                Ok(response) => response,
                Err(error) => {
                    if let Some(p) = project.as_mut().filter(|_| !preview) {
                        p.record_failure(request, error.clone());
                        p.commit(Path::new(&args[2]), p.revision)?;
                    }
                    return Err(error);
                }
            };
            if let Some(p) = project.as_mut().filter(|_| {
                !matches!(
                    response.output,
                    engine::Output::ChromatographicProcessing { preview: true, .. }
                )
            }) {
                p.add_result(Path::new(&args[2]), dataset.unwrap(), &response)?;
                p.commit(Path::new(&args[2]), p.revision)?;
            }
            serde_json::to_value(response).map_err(|e| EngineError::new("adapter_failure", e))
        }
        _ => Err(invalid()),
    }
}
fn main() {
    if matches!(std::env::args().nth(1).as_deref(), Some("--help" | "-h")) {
        println!("{HELP}");
        return;
    }
    match run() {
        Ok(value) => println!("{}", serde_json::json!({"ok":true,"result":value})),
        Err(error) => {
            println!("{}", serde_json::json!({"ok":false,"error":error}));
            std::process::exit(1);
        }
    }
}
