//! One coherent project-centered analysis-to-report workflow.
//!
//! The workbench keeps several capable tools (viewer, batch quantification,
//! calibration, QC, reports). This module is the map between them: it reads
//! the current application state, reports where the scientist stands in the
//! nine-step sequence, what is complete, what still needs review, and which
//! action comes next. Navigation preserves the selected project, dataset,
//! analyte, and result context — it only changes which workspace is shown.
//!
//! No analytical algorithm lives here. Acceptance criteria always come from
//! the configured method or the retained engine reports; this module never
//! invents a universal assay policy.
use super::MzViewerApp;
use eframe::egui;

/// Workspace destination for a workflow step action.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Destination {
    Data,
    Quant,
    Identify,
    Untargeted,
    Statistics,
    Reports,
    Activity,
}

/// Progress state of a single workflow step.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum StepState {
    /// Complete; nothing further required.
    Done,
    /// Ready to work on; preconditions are met.
    Ready,
    /// Needs scientist attention before release (warnings preserved on export).
    Attention,
    /// Cannot proceed until a precondition is resolved.
    Blocked,
    /// Not started yet.
    Todo,
}

pub(super) struct StepStatus {
    pub number: usize,
    pub title: &'static str,
    pub state: StepState,
    pub detail: String,
    pub destination: Destination,
    pub action: &'static str,
}

const TITLES: [&str; 9] = [
    "Open or create a project",
    "Import and select datasets",
    "Configure the analytical method",
    "Inspect chromatograms and spectra",
    "Review and resolve peak integrations",
    "Fit calibration models and calculate concentrations",
    "Review batch QC and validation results",
    "Review and approve final results",
    "Export the report and supporting data",
];

/// Switch the visible workspace, preserving project, dataset, analyte, and
/// result context. Also links the open project into Reports when empty.
pub(super) fn go(app: &mut MzViewerApp, ctx: &egui::Context, destination: Destination) {
    app.quant.active = destination == Destination::Quant;
    app.spectral.open = destination == Destination::Identify;
    app.untargeted.open = destination == Destination::Untargeted;
    app.statistics.open = destination == Destination::Statistics;
    app.delivery.open = destination == Destination::Reports;
    ctx.data_mut(|d| {
        d.insert_temp(
            egui::Id::new("activity_open"),
            destination == Destination::Activity,
        );
    });
    if destination == Destination::Reports && app.project.has_project() {
        let root = std::path::PathBuf::from(app.project.root_label());
        app.delivery.set_project_if_empty(&root);
    }
}

/// Compute the nine-step status from live application state.
pub(super) fn steps(app: &MzViewerApp) -> Vec<StepStatus> {
    let loaded = app.files.values().filter(|f| !f.is_loading).count();
    let importing = app.files.values().filter(|f| f.is_loading).count();
    let has_plot = app.files.values().any(|f| f.cache.plot_data.is_some());
    let has_spectrum = app.files.values().any(|f| f.cache.mass_spectrum.is_some());
    let unresolved = app.quant.unresolved_count();
    let has_results = app.quant.has_results();
    let method_valid = app.quant.method_is_valid();
    let method_error = if method_valid {
        String::new()
    } else {
        app.quant.method_error()
    };
    let (fitted, errored, total_targets) = app.quant.calibration_summary();
    let has_batch = app.quant.has_targeted_batch();
    let invalid = app.quant.invalid_result_count();
    let (reviewed, total) = app.quant.review_progress();
    let qc_reports = app.quant.qc_report_count();
    let qc_status = app.quant.qc_status();
    let qc_pending = app.quant.qc_pending_reviews();
    let (proposals, awaiting) = app.ai.review_summary();
    let (has_project, has_destination, destination_exists) = app.delivery.export_readiness();

    vec![
        StepStatus {
            number: 1,
            title: TITLES[0],
            state: if app.project.has_project() {
                StepState::Done
            } else {
                StepState::Todo
            },
            detail: if let Some(revision) = app.project.revision() {
                format!("Project revision {revision} retains all analyses")
            } else {
                "Exploratory workspace — create a project to retain revisions".into()
            },
            destination: Destination::Data,
            action: "Open project",
        },
        StepStatus {
            number: 2,
            title: TITLES[1],
            state: if loaded > 0 {
                StepState::Done
            } else if importing > 0 {
                StepState::Ready
            } else {
                StepState::Todo
            },
            detail: if loaded > 0 {
                format!(
                    "{loaded} dataset{} loaded{}",
                    if loaded == 1 { "" } else { "s" },
                    match app.active_file_id.and_then(|id| app.files.get(&id)) {
                        Some(file) => format!(" · active: {}", file.name),
                        None => String::new(),
                    }
                )
            } else if importing > 0 {
                format!("Importing {importing} dataset(s)…")
            } else {
                "No datasets yet — open mzML or a vendor folder".into()
            },
            destination: Destination::Data,
            action: "Open data",
        },
        StepStatus {
            number: 3,
            title: TITLES[2],
            state: if method_valid {
                StepState::Done
            } else {
                StepState::Attention
            },
            detail: if method_valid {
                app.quant.method_summary()
            } else {
                method_error
            },
            destination: Destination::Quant,
            action: "Edit method",
        },
        StepStatus {
            number: 4,
            title: TITLES[3],
            state: if has_plot {
                StepState::Done
            } else if loaded > 0 {
                StepState::Ready
            } else {
                StepState::Todo
            },
            detail: if has_plot {
                if has_spectrum {
                    "Chromatogram and spectrum inspected".into()
                } else {
                    "Chromatogram extracted — double-click to inspect a spectrum".into()
                }
            } else if loaded > 0 {
                "Choose TIC, BPC, or XIC and extract a trace".into()
            } else {
                "Available after datasets are imported".into()
            },
            destination: Destination::Data,
            action: "Inspect data",
        },
        StepStatus {
            number: 5,
            title: TITLES[4],
            state: if !has_results {
                if has_plot && method_valid {
                    StepState::Ready
                } else {
                    StepState::Todo
                }
            } else if unresolved > 0 {
                StepState::Attention
            } else {
                StepState::Done
            },
            detail: if !has_results {
                "Run a batch to measure peaks".into()
            } else if unresolved > 0 {
                format!("{unresolved} unresolved — missing, ambiguous, failed, or measured under a changed method")
            } else {
                "All peak integrations resolved".into()
            },
            destination: Destination::Quant,
            action: "Review peaks",
        },
        StepStatus {
            number: 6,
            title: TITLES[5],
            state: if !has_batch {
                StepState::Todo
            } else if errored > 0 || invalid > 0 {
                StepState::Attention
            } else {
                StepState::Done
            },
            detail: if !has_batch {
                "Configure a targeted batch from the measured peaks".into()
            } else if errored > 0 {
                format!(
                    "{errored} of {total_targets} calibration model(s) failed — see the engine error per target"
                )
            } else if invalid > 0 {
                format!("{invalid} concentration result(s) are missing, rejected, or outside the configured range")
            } else {
                format!("{fitted} calibration model(s) fitted; concentrations calculated")
            },
            destination: Destination::Quant,
            action: "Calibrate",
        },
        StepStatus {
            number: 7,
            title: TITLES[6],
            state: if qc_reports == 0 {
                StepState::Todo
            } else {
                match qc_status {
                    Some(crate::qc::Status::Pass) if qc_pending == 0 => StepState::Done,
                    _ => StepState::Attention,
                }
            },
            detail: if qc_reports == 0 {
                "Evaluate batch QC against the configured study rules".into()
            } else {
                match qc_status {
                    Some(crate::qc::Status::Pass) if qc_pending == 0 => {
                        "Batch QC passes per the configured rules".into()
                    }
                    Some(crate::qc::Status::Pass) => {
                        format!("QC passes, but {qc_pending} report(s) still await acknowledgement")
                    }
                    Some(crate::qc::Status::Fail) => {
                        "Batch QC FAILS per the configured rules — resolve or acknowledge before release".into()
                    }
                    _ => {
                        "Batch QC is indeterminate per the configured rules — review before release".into()
                    }
                }
            },
            destination: Destination::Quant,
            action: "Review QC",
        },
        StepStatus {
            number: 8,
            title: TITLES[7],
            state: if total == 0 {
                StepState::Todo
            } else if reviewed < total || awaiting > 0 {
                StepState::Attention
            } else {
                StepState::Done
            },
            detail: if total == 0 {
                "Available after concentrations are calculated".into()
            } else if reviewed < total {
                format!(
                    "{reviewed} of {total} results reviewed{}",
                    if awaiting > 0 {
                        format!(" · {awaiting} AI proposal(s) awaiting decision")
                    } else {
                        String::new()
                    }
                )
            } else if awaiting > 0 {
                format!("{awaiting} AI proposal(s) awaiting decision")
            } else if proposals > 0 {
                format!("All {total} results reviewed; {proposals} proposal(s) decided")
            } else {
                format!("All {total} results reviewed")
            },
            destination: Destination::Activity,
            action: "Review results",
        },
        StepStatus {
            number: 9,
            title: TITLES[8],
            state: if !has_project {
                StepState::Blocked
            } else if has_destination && !destination_exists {
                StepState::Ready
            } else {
                StepState::Todo
            },
            detail: if !has_project {
                "Open a project first — the report exports project evidence".into()
            } else if has_destination && !destination_exists {
                "Project and new output folder are set — ready to export".into()
            } else if destination_exists {
                "That output folder already exists — choose a new folder name".into()
            } else {
                "Choose a new output folder in Reports".into()
            },
            destination: Destination::Reports,
            action: "Export report",
        },
    ]
}

/// Explicit warnings that must accompany any release export.
/// Each warning cites the configured method or retained engine report —
/// never an invented universal policy.
pub(super) fn export_warnings(app: &MzViewerApp) -> Vec<String> {
    let mut warnings = Vec::new();
    let unresolved = app.quant.unresolved_count();
    if app.quant.has_results() && unresolved > 0 {
        warnings.push(format!(
            "{unresolved} peak integration(s) are unresolved (missing, ambiguous, failed, or measured under a changed method)"
        ));
    }
    let (fitted, errored, total_targets) = app.quant.calibration_summary();
    if app.quant.has_targeted_batch() {
        if errored > 0 {
            warnings.push(format!(
                "{errored} of {total_targets} calibration model(s) failed to fit — see engine errors per target"
            ));
        }
        if fitted == 0 && errored == 0 {
            warnings.push("No calibration models are fitted yet".into());
        }
        let invalid = app.quant.invalid_result_count();
        if invalid > 0 {
            warnings.push(format!(
                "{invalid} concentration result(s) are missing, rejected, or outside the configured calibration range"
            ));
        }
        let (reviewed, total) = app.quant.review_progress();
        if total > 0 && reviewed < total {
            warnings.push(format!(
                "{} of {total} concentration result(s) are not yet reviewed",
                total - reviewed
            ));
        }
    }
    match app.quant.qc_status() {
        Some(crate::qc::Status::Fail) => {
            warnings.push("Batch QC verdict is FAIL per the configured study rules".into())
        }
        Some(crate::qc::Status::Indeterminate) => {
            warnings.push("Batch QC verdict is indeterminate per the configured study rules".into())
        }
        _ => {}
    }
    if app.quant.qc_pending_reviews() > 0 {
        warnings.push(format!(
            "{} QC report(s) have unacknowledged rules",
            app.quant.qc_pending_reviews()
        ));
    }
    let (_, awaiting) = app.ai.review_summary();
    if awaiting > 0 {
        warnings.push(format!(
            "{awaiting} AI proposal(s) await an approve/reject decision"
        ));
    }
    warnings
}

/// Blocked reasons that prevent a trustworthy project-report export.
pub(super) fn export_blockers(app: &MzViewerApp) -> Vec<String> {
    let mut blockers = Vec::new();
    if !app.project.has_project() {
        blockers.push("No project is open — the report exports retained project evidence.".into());
    }
    blockers
}

fn state_glyph(state: StepState) -> &'static str {
    match state {
        StepState::Done => "●",
        StepState::Ready => "○",
        StepState::Attention => "◐",
        StepState::Blocked => "■",
        StepState::Todo => "○",
    }
}

/// Compact banner naming the single most important next action.
pub(super) fn banner(app: &mut MzViewerApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    let list = steps(app);
    let next = list
        .iter()
        .find(|s| s.state == StepState::Blocked)
        .or_else(|| list.iter().find(|s| s.state == StepState::Attention))
        .or_else(|| {
            list.iter()
                .find(|s| s.state == StepState::Todo || s.state == StepState::Ready)
        });
    let Some(next) = next else {
        ui.horizontal_wrapped(|ui| {
            ui.strong("Analysis complete — review the release checklist in Reports.");
            if ui.button("Open Reports").clicked() {
                go(app, ctx, Destination::Reports);
            }
        });
        return;
    };
    let warnings = export_warnings(app);
    ui.horizontal_wrapped(|ui| {
        let glyph = state_glyph(next.state);
        ui.label(format!(
            "{glyph} Step {} of 9 — {}",
            next.number, next.title
        ));
        if !warnings.is_empty() && next.number >= 5 {
            ui.small(format!(
                "{} warning(s) will accompany export",
                warnings.len()
            ));
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.small(&next.detail);
        if ui
            .add(
                egui::Button::new(egui::RichText::new(format!("Next: {}", next.action)).strong())
                    .fill(ui.visuals().selection.bg_fill),
            )
            .on_hover_text(format!("Go to step {}: {}", next.number, next.title))
            .clicked()
        {
            go(app, ctx, next.destination);
        }
    });
}

/// Full numbered guide with per-step status and contextual navigation.
/// Uses a compact list below 950 px so step text and actions are never
/// squeezed out of view at typical window sizes.
pub(super) fn guide(app: &mut MzViewerApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.heading("Analysis workflow");
    ui.small("Follow the numbered steps from project to report. Your project, datasets, and results stay selected while you move.");
    if ui.available_width() < 950.0 {
        step_list(app, ctx, ui);
        return;
    }
    let list = steps(app);
    let done = list.iter().filter(|s| s.state == StepState::Done).count();
    ui.small(format!("{done} of 9 steps complete"));
    let mut chosen: Option<Destination> = None;
    egui::Grid::new("analysis_workflow_steps")
        .num_columns(3)
        .striped(true)
        .show(ui, |ui| {
            for step in &list {
                let glyph = state_glyph(step.state);
                let label = match step.state {
                    StepState::Done => "Complete",
                    StepState::Ready => "Ready",
                    StepState::Attention => "Needs review",
                    StepState::Blocked => "Blocked",
                    StepState::Todo => "Not started",
                };
                ui.label(format!("{glyph} {}. {}", step.number, step.title));
                ui.small(format!("{label} — {}", step.detail));
                if ui.button(step.action).clicked() {
                    chosen = Some(step.destination);
                }
                ui.end_row();
            }
        });
    if let Some(destination) = chosen {
        go(app, ctx, destination);
    }
}

/// Non-grid fallback used where a table layout is unavailable.
pub(super) fn step_list(app: &mut MzViewerApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    let list = steps(app);
    let mut chosen: Option<Destination> = None;
    for step in &list {
        let glyph = state_glyph(step.state);
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{glyph} {}. {}", step.number, step.title));
            if ui.small_button(step.action).clicked() {
                chosen = Some(step.destination);
            }
        });
        ui.small(&step.detail);
    }
    if let Some(destination) = chosen {
        go(app, ctx, destination);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nine_steps_cover_project_to_report() {
        let app = MzViewerApp::default();
        let list = steps(&app);
        assert_eq!(list.len(), 9);
        assert_eq!(list.first().unwrap().number, 1);
        assert_eq!(list.last().unwrap().number, 9);
        assert_eq!(
            list.last().unwrap().title,
            "Export the report and supporting data"
        );
        // An empty workspace blocks only the project-gated report export.
        assert_eq!(list.last().unwrap().state, StepState::Blocked);
        assert!(!export_blockers(&app).is_empty());
    }

    #[test]
    fn navigation_preserves_selection_context() {
        let mut app = MzViewerApp {
            active_file_id: Some(7),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        go(&mut app, &ctx, Destination::Quant);
        assert!(app.quant.active);
        assert_eq!(app.active_file_id, Some(7));
        go(&mut app, &ctx, Destination::Data);
        assert!(!app.quant.active);
        assert_eq!(app.active_file_id, Some(7));
    }

    #[test]
    fn warnings_cite_method_or_engine_state() {
        let app = MzViewerApp::default();
        // No batch yet: no spurious calibration warnings.
        assert!(export_warnings(&app).is_empty());
    }
}
