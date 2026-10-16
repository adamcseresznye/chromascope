//! What the window holds, as MCP resources with stable URIs, so a client can read it without
//! a tool call:
//!
//! * `chromascope://datasets`, `chromascope://dataset/<id>`, `chromascope://dataset/<id>/traces`
//! * `chromascope://quantification`, `chromascope://quantification/run/<n>`
//! * `chromascope://qc`, `chromascope://qc/<n>`
//! * `chromascope://project`, `chromascope://project/revision/<n>`
//!
//! `<id>` is the dataset's stable id (the SHA-256 of its source file, which survives a restart)
//! or its number in the running window. Everything is read-only.
use super::*;

/// Resources are for reading; a trace past this many points is cut (the total says how many
/// there are, `chromatogram_data` pages through them).
const TRACE_POINT_LIMIT: usize = 10_000;
/// A resource larger than this is refused rather than sent whole.
const RESOURCE_LIMIT: usize = 16 * 1024 * 1024;

/// URI templates, for clients that build URIs themselves.
pub(crate) const TEMPLATES: &[(&str, &str, &str)] = &[
    (
        "chromascope://dataset/{id}",
        "dataset",
        "File-level metadata of one open dataset: scans, retention-time and m/z range, scan filters and stored chromatograms. {id} is the stable id or the number from the datasets resource.",
    ),
    (
        "chromascope://dataset/{id}/traces",
        "dataset traces",
        "The current and the retained chromatograms of one dataset, with their extraction parameters and (up to 10000) points each.",
    ),
    (
        "chromascope://quantification/run/{n}",
        "calibration run",
        "One calibration and concentration run of the Quantification tab, complete with its request, calibrations, results and reviews.",
    ),
    (
        "chromascope://qc/{n}",
        "QC report",
        "One QC report of the Quantification tab, complete with its study, decisions and reviews.",
    ),
    (
        "chromascope://project/revision/{n}",
        "project revision",
        "One saved revision of the open project, as stored.",
    ),
];

fn bytes_of(value: &Value) -> usize {
    value.to_string().len()
}

fn resource(uri: String, name: String, description: &str) -> Value {
    json!({"uri":uri,"name":name,"description":description})
}

/// The id used in the URI of a dataset: its stable id once known, else its number.
fn dataset_segment(b: &GuiBridge, id: usize) -> String {
    match b.stable.get(&id) {
        Some(StableState::Ready(hash)) => hash.clone(),
        _ => id.to_string(),
    }
}

pub(super) fn list(app: &MzViewerApp, b: &GuiBridge) -> Value {
    let mut resources = vec![resource(
        "chromascope://datasets".into(),
        "open datasets".into(),
        "Every open dataset with its number, stable id, path, loading state and last extraction.",
    )];
    let mut ids: Vec<usize> = app.files.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let file = &app.files[&id];
        let segment = dataset_segment(b, id);
        resources.push(resource(
            format!("chromascope://dataset/{segment}"),
            format!("dataset {}", file.name),
            "File-level metadata of this dataset.",
        ));
        let has_traces = file.cache.last_processing_params.is_some()
            || app.workspace.traces.get(&id).is_some_and(|t| !t.is_empty());
        if has_traces {
            resources.push(resource(
                format!("chromascope://dataset/{segment}/traces"),
                format!("traces of {}", file.name),
                "The current and the retained chromatograms of this dataset.",
            ));
        }
    }
    let runs = app.quant.remote_batches();
    if app.quant.has_results() || !runs.is_empty() {
        resources.push(resource(
            "chromascope://quantification".into(),
            "quantification results".into(),
            "The batch quantification results, the method and a list of the calibration runs.",
        ));
    }
    for (index, run) in runs.iter().enumerate() {
        resources.push(resource(
            format!("chromascope://quantification/run/{index}"),
            format!("calibration run {}", run.request.name),
            "A calibration and concentration run with its request, calibrations and reviews.",
        ));
    }
    let reports = app.quant.remote_qc_reports();
    if !reports.is_empty() {
        resources.push(resource(
            "chromascope://qc".into(),
            "QC reports".into(),
            "The QC reports of the Quantification tab with their verdicts.",
        ));
    }
    for index in 0..reports.len() {
        resources.push(resource(
            format!("chromascope://qc/{index}"),
            format!("QC report {index}"),
            "A QC report with its study, decisions and reviews.",
        ));
    }
    if let Some(project) = app.project.revision() {
        resources.push(resource(
            "chromascope://project".into(),
            "open project".into(),
            "The open project: id, revision, registered datasets and results.",
        ));
        for revision in 1..=project {
            resources.push(resource(
                format!("chromascope://project/revision/{revision}"),
                format!("project revision {revision}"),
                "A saved revision of the open project, as stored.",
            ));
        }
    }
    json!({"resources":resources})
}

/// The retained and current traces of one dataset, points cut at [`TRACE_POINT_LIMIT`].
fn traces(app: &MzViewerApp, id: usize) -> Value {
    let cut = |points: &[[f64; 2]]| -> Value {
        json!({"total_points":points.len(),"truncated":points.len() > TRACE_POINT_LIMIT,"points":&points[..points.len().min(TRACE_POINT_LIMIT)]})
    };
    let file = &app.files[&id];
    let current = match (&file.cache.last_processing_params, &file.cache.plot_data) {
        (Some(params), Some(points)) => {
            let mut trace = json!({"parameters":params,"visible":!app.workspace.hidden_current.contains(&id),"name":workspace::display_name(app, id, params)});
            if let (Some(trace), Some(cut)) = (trace.as_object_mut(), cut(points).as_object()) {
                trace.extend(cut.clone());
            }
            trace
        }
        _ => Value::Null,
    };
    let retained: Vec<Value> = app
        .workspace
        .traces
        .get(&id)
        .map(|traces| {
            traces
                .iter()
                .enumerate()
                .map(|(index, t)| {
                    let mut trace = json!({"trace_index":index,"name":t.name,"parameters":t.params,"visible":t.visible});
                    if let (Some(trace), Some(cut)) = (trace.as_object_mut(), cut(&t.points).as_object()) {
                        trace.extend(cut.clone());
                    }
                    trace
                })
                .collect()
        })
        .unwrap_or_default();
    json!({"dataset_id":id,"retention_time_unit":"minutes","intensity_unit":"arbitrary units","current":current,"retained":retained})
}

pub(super) fn read(app: &MzViewerApp, b: &GuiBridge, uri: &str) -> Result<Value, String> {
    let path = uri
        .strip_prefix("chromascope://")
        .ok_or_else(|| format!("Unknown resource {uri}: URIs start with chromascope://"))?;
    let parts: Vec<&str> = path.split('/').collect();
    let number = |text: &str| -> Result<usize, String> {
        text.parse::<usize>()
            .map_err(|_| format!("Unknown resource {uri}: {text:?} is not a number"))
    };
    let value = match parts.as_slice() {
        ["datasets"] => {
            let state = app.remote_state(b);
            json!({"datasets":state["datasets"],"active_dataset_id":state["active_dataset_id"]})
        }
        ["dataset", id, rest @ ..] if rest.is_empty() || rest == ["traces"] => {
            let index = if id.len() <= 9 && id.chars().all(|c| c.is_ascii_digit()) {
                number(id)?
            } else {
                app.resolve_stable(id, b)?
            };
            if !app.files.contains_key(&index) {
                return Err(format!("No open dataset {id}"));
            }
            app.authorize_dataset(&app.files[&index], b)?;
            if rest.is_empty() {
                app.dataset_metadata_value(index)?
            } else {
                traces(app, index)
            }
        }
        ["quantification"] => {
            let runs: Vec<Value> = app
                .quant
                .remote_batches()
                .iter()
                .enumerate()
                .map(|(index, run)| {
                    json!({"run":index,"uri":format!("chromascope://quantification/run/{index}"),"batch_id":run.batch_id,"name":run.request.name,"results":run.results.len(),"reviews":run.reviews.len(),"created_unix_ms":run.created_unix_ms})
                })
                .collect();
            json!({"batch":super::super::quant::remote_status(app),"calibration_runs":runs,"qc_reports":app.quant.remote_qc_reports().len()})
        }
        ["quantification", "run", n] => {
            let n = number(n)?;
            let run = app
                .quant
                .remote_batches()
                .get(n)
                .ok_or_else(|| format!("No calibration run {n}"))?;
            serde_json::to_value(run).map_err(err)?
        }
        ["qc"] => {
            let reports: Vec<Value> = app
                .quant
                .remote_qc_reports()
                .iter()
                .enumerate()
                .map(|(index, report)| {
                    json!({"report":index,"uri":format!("chromascope://qc/{index}"),"report_id":report.report_id,"status":report.status,"decisions":report.decisions.len(),"reviews":report.reviews.len(),"unacknowledged_rules":report.review_queue.len().saturating_sub(report.reviews.len()),"acceptance":report.acceptance})
                })
                .collect();
            json!({"reports":reports})
        }
        ["qc", n] => {
            let n = number(n)?;
            let report = app
                .quant
                .remote_qc_reports()
                .get(n)
                .ok_or_else(|| format!("No QC report {n}"))?;
            serde_json::to_value(report).map_err(err)?
        }
        ["project"] => {
            if app.project.open_root().is_none() {
                return Err("No project is open".into());
            }
            app.project.summary()
        }
        ["project", "revision", n] => {
            let n = number(n)?;
            let root = app.project.open_root().ok_or("No project is open")?;
            let root = b.policy.directory(&root.to_string_lossy())?;
            let revision = app.project.revision().unwrap_or(0) as usize;
            if n == 0 || n > revision {
                return Err(format!("The open project has revisions 1 to {revision}"));
            }
            let file = root.join(format!("revision-{n:020}.json"));
            let text = std::fs::read_to_string(&file).map_err(err)?;
            serde_json::from_str(&text).map_err(err)?
        }
        _ => return Err(format!("Unknown resource {uri}")),
    };
    if bytes_of(&value) > RESOURCE_LIMIT {
        return Err(format!(
            "{uri} is larger than 16 MiB; read it in pages with the tools instead"
        ));
    }
    Ok(value)
}
