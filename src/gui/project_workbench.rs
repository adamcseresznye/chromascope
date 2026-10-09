//! Project context for all desktop workspaces; expensive verification runs off-thread.
use super::MzViewerApp;
use crate::{domain::EngineError, project::Project};
use eframe::egui;
use serde_json::Value;
use std::path::{Path, PathBuf};
type Completion = crate::domain::Result<(Project, Option<Value>, Vec<crate::engine::Response>)>;
#[derive(Default)]
pub(super) struct State {
    root: PathBuf,
    project: Option<Project>,
    message: String,
    pending: Option<std::sync::mpsc::Receiver<Completion>>,
    pending_root: Option<PathBuf>,
    opening: bool,
}
fn snapshot(app: &MzViewerApp, ctx: &egui::Context) -> Value {
    let mut value = serde_json::json!({"version":1,"viewer":super::workspace::project_snapshot(app,ctx),"quant":app.quant,"spectral":app.spectral,"untargeted":app.untargeted,"statistics":app.statistics,"ai":app.ai,"delivery":app.delivery});
    value["quant"]["restore_samples"] =
        serde_json::to_value(app.quant.project_selection(&app.files)).unwrap();
    value["quant"]["selected_samples"] = serde_json::json!([]);
    value["quant"]["seen_samples"] = serde_json::json!([]);
    value
}
fn source_paths(value: &Value, paths: &mut Vec<PathBuf>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if key == "source" {
                    if let Some(path) = value.as_str() {
                        let p = PathBuf::from(path);
                        if p.is_file() && !paths.contains(&p) {
                            paths.push(p);
                        }
                    }
                }
                source_paths(value, paths);
            }
        }
        Value::Array(values) => {
            for value in values {
                source_paths(value, paths);
            }
        }
        _ => {}
    }
}
fn store(root: &Path, expected: u64, mut value: Value) -> Completion {
    verify_snapshot(&value)?;
    let mut project = Project::open(root)?;
    if project.revision != expected {
        return Err(EngineError::new(
            "stale_revision",
            "Project changed; reopen it before saving",
        ));
    }
    let mut paths = vec![];
    source_paths(&value, &mut paths);
    for path in paths {
        project.register(&path)?;
    }
    value["dataset_bindings"] = serde_json::to_value(&project.sources)
        .map_err(|e| EngineError::new("corrupt_project", e))?;
    for source in &project.sources {
        project.verified_source(source.id)?;
    }
    for section in ["spectral", "untargeted", "statistics"] {
        for raw in value[section]["history"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let response: crate::engine::Response = serde_json::from_value(raw.clone())
                .map_err(|e| EngineError::new("corrupt_project", e))?;
            if project
                .results
                .iter()
                .any(|r| r.result_id == response.result_id)
            {
                continue;
            }
            if let Some(source) = response
                .source_sha256
                .as_ref()
                .and_then(|hash| project.sources.iter().find(|source| &source.sha256 == hash))
                .or(project.sources.first())
            {
                project.add_result(root, source.id, &response)?;
            }
        }
    }
    // Typed targeted/QC snapshots are preserved exactly. Export through the same
    // engine additionally makes their evidence visible to project reporting.
    if let Some(source) = project.sources.first().map(|s| s.id) {
        let mut operations = vec![];
        for batch in value["quant"]["targeted"]["history"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let batch = serde_json::from_value(batch.clone())
                .map_err(|e| EngineError::new("corrupt_project", e))?;
            operations.push(crate::domain::Operation::ExportTargeted { batch });
        }
        for report in value["quant"]["targeted"]["qc"]["reports"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let report = serde_json::from_value(report.clone())
                .map_err(|e| EngineError::new("corrupt_project", e))?;
            operations.push(crate::domain::Operation::ExportQc { report });
        }
        for operation in operations {
            let request = crate::domain::Request {
                version: 1,
                operation_id: Default::default(),
                actor: "desktop-project".into(),
                operation,
            };
            let response = crate::engine::execute(
                Path::new("-"),
                request,
                &crate::jobs::JobControl::default(),
            )?;
            let output = serde_json::to_value(&response.output)
                .map_err(|e| EngineError::new("corrupt_project", e))?;
            let exists = project.results.iter().any(|artifact| {
                project
                    .load_result(root, artifact.result_id)
                    .ok()
                    .and_then(|old| serde_json::to_value(old.output).ok())
                    .as_ref()
                    == Some(&output)
            });
            if !exists {
                project.add_result(root, source, &response)?;
            }
        }
    }
    for raw in value["ai"]["proposals"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let proposal: crate::proposals::Proposal = serde_json::from_value(raw.clone())
            .map_err(|e| EngineError::new("corrupt_project", e))?;
        if let Some(existing) = project
            .review_proposals
            .iter_mut()
            .find(|old| old.id == proposal.id)
        {
            if proposal.events.len() > existing.events.len() {
                *existing = proposal;
            }
        } else {
            project.review_proposals.push(proposal);
        }
    }
    project.save_workspace(root, &value)?;
    project.commit(root, expected)?;
    Ok((project, None, vec![]))
}
fn restore(app: &mut MzViewerApp, ctx: &egui::Context, value: Value) -> Result<(), String> {
    if value["version"] != 1 {
        return Err("Unsupported workbench snapshot version".into());
    }
    let quant: super::quant::QuantState =
        serde_json::from_value(value["quant"].clone()).map_err(|e| e.to_string())?;
    let spectral: super::spectral::State =
        serde_json::from_value(value["spectral"].clone()).map_err(|e| e.to_string())?;
    let untargeted: super::untargeted::State =
        serde_json::from_value(value["untargeted"].clone()).map_err(|e| e.to_string())?;
    let statistics: super::statistics::State =
        serde_json::from_value(value["statistics"].clone()).map_err(|e| e.to_string())?;
    let ai = if value["ai"].is_null() {
        super::ai_review::State::default()
    } else {
        serde_json::from_value(value["ai"].clone()).map_err(|e| e.to_string())?
    };
    let delivery = if value["delivery"].is_null() {
        super::delivery::State::default()
    } else {
        serde_json::from_value(value["delivery"].clone()).map_err(|e| e.to_string())?
    };
    super::workspace::restore_project_snapshot(app, ctx, value["viewer"].clone())?;
    app.quant = quant;
    app.spectral = spectral;
    app.untargeted = untargeted;
    app.statistics = statistics;
    app.ai = ai;
    app.delivery = delivery;
    Ok(())
}
fn verify_snapshot(value: &Value) -> crate::domain::Result<()> {
    let failure = |e| EngineError::new("corrupt_project", e);
    if value["version"] != 1 {
        return Err(failure("Unsupported workbench version".to_string()));
    }
    super::quant::QuantState::verify_snapshot(value["quant"].clone()).map_err(failure)?;
    for section in ["spectral", "untargeted", "statistics"] {
        for response in value[section]["history"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let response: crate::engine::Response =
                serde_json::from_value(response.clone()).map_err(|e| failure(e.to_string()))?;
            match section {
                "spectral" => super::spectral::verify(&response)?,
                "untargeted" => crate::untargeted::verify_response(&response)?,
                _ => crate::statistics::verify_response(&response)?,
            }
        }
    }
    for batch in value["quant"]["targeted"]["history"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let batch = serde_json::from_value(batch.clone()).map_err(|e| failure(e.to_string()))?;
        crate::targeted::verify(&batch)?;
    }
    for report in value["quant"]["targeted"]["qc"]["reports"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let report = serde_json::from_value(report.clone()).map_err(|e| failure(e.to_string()))?;
        crate::qc::verify(&report)?;
    }
    for ledger in value["untargeted"]["annotations"]["history"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let ledger = serde_json::from_value(ledger.clone()).map_err(|e| failure(e.to_string()))?;
        crate::annotation::verify(&ledger)?;
    }
    Ok(())
}
pub(super) fn poll(app: &mut MzViewerApp, ctx: &egui::Context) {
    let mut state = std::mem::take(&mut app.project);
    if let Some(rx) = &state.pending {
        match rx.try_recv() {
            Ok(result) => {
                state.pending = None;
                match result {
                    Ok((project, workspace, results)) => {
                        let restore_sources = state.opening && workspace.is_none();
                        if restore_sources {
                            app.reset_state();
                            app.quant = Default::default();
                            app.spectral = Default::default();
                            app.untargeted = Default::default();
                            app.statistics = Default::default();
                            app.ai = Default::default();
                            app.delivery = Default::default();
                        }
                        state.opening = false;
                        if let Some(value) = workspace {
                            if let Err(error) = restore(app, ctx, value) {
                                state.message = error;
                                app.project = state;
                                return;
                            }
                        }
                        if restore_sources {
                            super::panels::queue_file_imports(
                                app,
                                project
                                    .sources
                                    .iter()
                                    .map(|source| source.path.clone())
                                    .collect(),
                            );
                        }
                        for response in results {
                            route_result(app, response);
                        }
                        app.ai.merge_project_reviews(&project.review_proposals);
                        state.message = format!("Project revision {} retained", project.revision);
                        if let Some(root) = state.pending_root.take() {
                            state.root = root;
                        }
                        state.project = Some(project);
                        app.delivery.set_project(&state.root);
                    }
                    Err(error) => state.message = error.to_string(),
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                state.pending = None;
                state.message = "Project worker disconnected; prior state retained".into();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100))
            }
        }
    }
    app.project = state;
}
pub(super) fn menu(app: &mut MzViewerApp, ui: &mut egui::Ui) {
    let label = app
        .project
        .project
        .as_ref()
        .map(|p| format!("Project · r{}", p.revision))
        .unwrap_or_else(|| "Project".into());
    let message = app.project.message.clone();
    ui.menu_button(label, |ui| controls(app, ui))
        .response
        .on_hover_text(message);
    if app.project.pending.is_some() {
        ui.spinner();
    }
}
pub(super) fn controls(app: &mut MzViewerApp, ui: &mut egui::Ui) {
    let ready = app.project.pending.is_none()
        && !app.async_state.is_processing
        && !app.quant.busy()
        && !app.spectral.busy()
        && !app.untargeted.busy()
        && !app.statistics.busy()
        && !app.delivery.busy()
        && !app.ai.busy()
        && !app.files.values().any(|f| f.is_loading);
    ui.vertical(|ui| {
        if let Some(project) = &app.project.project {
            ui.strong(format!("Project · revision {}", project.revision))
                .on_hover_text(format!("{}\n{}", project.id.0, app.project.root.display()));
        } else {
            ui.weak("Exploratory workspace · create a project to retain all analyses");
        }
        ui.add_enabled_ui(ready, |ui| {
            if ui.button("Create project…").clicked() {
                app.project.opening = false;
                if let Some(parent) = rfd::FileDialog::new()
                    .set_title("Choose parent folder for a new chromascope-project directory")
                    .pick_folder()
                {
                    let root = parent.join("chromascope-project");
                    let worker_root = root.clone();
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let _ = tx.send(Project::create(&worker_root).map(|p| (p, None, vec![])));
                    });
                    app.project.pending_root = Some(root);
                    app.project.pending = Some(rx);
                }
            }
            if ui.button("Open project…").clicked() {
                app.project.opening = true;
                if let Some(root) = rfd::FileDialog::new().pick_folder() {
                    let worker_root = root.clone();
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let result = (|| {
                            let project = Project::open(&worker_root)?;
                            let mut workspace = project.load_workspace(&worker_root)?;
                            if let Some(value) = &mut workspace {
                                relocate_snapshot(value, &project)?;
                            }
                            if let Some(value) = &workspace {
                                verify_snapshot(value)?;
                            }
                            let results = project
                                .results
                                .iter()
                                .map(|artifact| {
                                    project.load_result(&worker_root, artifact.result_id)
                                })
                                .collect::<crate::domain::Result<Vec<_>>>()?;
                            Ok((project, workspace, results))
                        })();
                        let _ = tx.send(result);
                    });
                    app.project.pending_root = Some(root);
                    app.project.pending = Some(rx);
                }
            }
            if ui
                .add_enabled(
                    app.project.project.is_some(),
                    egui::Button::new("Refresh project activity"),
                )
                .clicked()
            {
                let root = app.project.root.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let result = (|| {
                        let project = Project::open(&root)?;
                        let results = project
                            .results
                            .iter()
                            .map(|artifact| project.load_result(&root, artifact.result_id))
                            .collect::<crate::domain::Result<Vec<_>>>()?;
                        Ok((project, None, results))
                    })();
                    let _ = tx.send(result);
                });
                app.project.opening = false;
                app.project.pending = Some(rx);
            }
            if ui
                .add_enabled(
                    app.project.project.is_some(),
                    egui::Button::new("Save project revision"),
                )
                .clicked()
            {
                app.project.opening = false;
                let value = snapshot(app, ui.ctx());
                let expected = app.project.project.as_ref().unwrap().revision;
                let root = app.project.root.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let _ = tx.send(store(&root, expected, value));
                });
                app.project.pending = Some(rx);
            }
        });
        if app.project.pending.is_some() {
            ui.spinner();
            ui.small("Verifying and retaining project evidence…");
        }
        if !app.project.message.is_empty() {
            ui.small(&app.project.message);
        }
    });
}
impl State {
    pub(super) fn revision(&self) -> Option<u64> {
        self.project.as_ref().map(|p| p.revision)
    }
}

fn route_result(app: &mut MzViewerApp, response: crate::engine::Response) {
    use crate::engine::Output;
    match &response.output {
        Output::TargetedQuantification { batch } => {
            let _ = app.quant.retain_targeted(*batch.clone());
        }
        Output::QcReport { report } => {
            let _ = app.quant.retain_qc(*report.clone());
        }
        Output::FeatureAnnotations { ledger } => {
            let _ = app.untargeted.retain_annotations(*ledger.clone());
        }
        Output::FeatureMatrix { .. } => app.untargeted.retain(response),
        Output::Statistics { .. } => app.statistics.retain(response),
        Output::SpectralProcessing { .. }
        | Output::SpectralLibrary { .. }
        | Output::SpectralSearch { .. }
        | Output::FormulaCandidates { .. }
        | Output::IsotopeAnalysis { .. }
        | Output::SpectralComparison { .. } => app.spectral.retain(response),
        _ => {}
    }
}

fn relocate_snapshot(value: &mut Value, project: &Project) -> crate::domain::Result<()> {
    let bindings: Vec<crate::project::Source> = if value["dataset_bindings"].is_null() {
        return Ok(());
    } else {
        serde_json::from_value(value["dataset_bindings"].clone())
            .map_err(|e| EngineError::new("corrupt_project", e))?
    };
    fn replace(value: &mut Value, old: &str, new: &str) {
        match value {
            Value::String(text) if text == old => *text = new.into(),
            Value::Array(values) => {
                for value in values {
                    replace(value, old, new)
                }
            }
            Value::Object(map) => {
                for value in map.values_mut() {
                    replace(value, old, new)
                }
            }
            _ => {}
        }
    }
    for binding in bindings {
        let current = project
            .sources
            .iter()
            .find(|source| source.id == binding.id && source.sha256 == binding.sha256)
            .ok_or_else(|| {
                EngineError::new(
                    "corrupt_project",
                    "Snapshot dataset binding does not match project identity/checksum",
                )
            })?;
        let old = binding.path.to_string_lossy();
        let new = current.path.to_string_lossy();
        replace(&mut value["viewer"], &old, &new);
        replace(&mut value["quant"]["restore_samples"], &old, &new);
        for section in ["spectral", "untargeted", "statistics"] {
            {
                let field = "draft";
                if let Some(text) = value[section][field].as_str() {
                    if let Ok(mut draft) = serde_json::from_str::<Value>(text) {
                        replace(&mut draft, &old, &new);
                        value[section][field] = Value::String(
                            serde_json::to_string_pretty(&draft)
                                .map_err(|e| EngineError::new("corrupt_project", e))?,
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod project_desktop_tests {
    use super::*;
    #[test]
    fn desktop_snapshot_roundtrip_keeps_feature_evidence_and_rejects_stale_save() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join("project");
        let project = Project::create(&directory).unwrap();
        let ctx = egui::Context::default();
        let mut app = MzViewerApp::default();
        app.untargeted
            .retain(super::super::untargeted::tests::response());
        let value = snapshot(&app, &ctx);
        verify_snapshot(&value).unwrap();
        let expected_matrix =
            serde_json::to_value(app.untargeted.latest_matrix().unwrap()).unwrap();
        let (project, _, _) = store(&directory, project.revision, value.clone()).unwrap();
        assert_eq!(project.revision, 1);
        let error = store(&directory, 0, value)
            .err()
            .expect("Stale save must fail");
        assert!(error.to_string().contains("reopen"));
        let reopened = Project::open(&directory).unwrap();
        let value = reopened.load_workspace(&directory).unwrap().unwrap();
        let mut restored = MzViewerApp::default();
        restore(&mut restored, &ctx, value).unwrap();
        assert_eq!(
            serde_json::to_value(restored.untargeted.latest_matrix().unwrap()).unwrap(),
            expected_matrix
        );
    }
}
