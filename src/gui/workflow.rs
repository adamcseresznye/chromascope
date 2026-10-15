//! One coherent project-centered analysis-to-report workflow.
//!
//! The workbench keeps several capable tools (data explorer, quantification and QC,
//! audit package). This module is the map between them: it reads the current
//! application state, reports which of the steps are complete, which need attention
//! and which one comes next, and sends each action to the place where it is done.
//! Navigation preserves the selected project, dataset, analyte, and result context;
//! it only changes which workspace is shown.
//!
//! No analytical algorithm lives here. Acceptance criteria always come from the
//! configured method or the retained engine reports; this module never invents a
//! universal assay policy.
use super::MzViewerApp;
use eframe::egui;

/// Where a workflow action leads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Destination {
    Data,
    /// File dialog to import datasets.
    OpenData,
    Quant,
    /// Quantification tab, stage 1: areas and peak review.
    QuantReview,
    /// Quantification tab, stage 2: calibration and concentrations.
    QuantCalibration,
    /// Quantification tab, stage 3: batch QC and validation.
    QuantQc,
    /// Quantification tab with the method editor open.
    Method,
    Identify,
    Untargeted,
    Statistics,
    /// Audit package workspace.
    Reports,
    Activity,
    /// Project dialogs; these do not change the visible workspace.
    OpenProject,
    SaveProject,
}

/// Progress of a single workflow step.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum StepState {
    /// Complete; nothing further required.
    Done,
    /// Started, but something needs the scientist's attention before release.
    Attention,
    /// Not complete yet.
    Pending,
}

#[derive(Clone)]
pub(super) struct StepStatus {
    pub number: usize,
    pub title: &'static str,
    pub state: StepState,
    pub detail: String,
    /// Buttons for this step, primary first.
    pub actions: Vec<(&'static str, Destination)>,
}

/// Steps that make up the analysis proper; the last step (audit package) is optional.
const REQUIRED_STEPS: usize = 7;

const TITLES: [&str; 8] = [
    "Project",
    "Import data",
    "Quantification method",
    "Peak review",
    "Calibration and concentrations",
    "Batch QC",
    "Review results",
    "Report and audit package (optional)",
];

/// Switch the visible workspace, preserving project, dataset, analyte, and
/// result context. Also links the open project into the audit package when empty.
pub(super) fn go(app: &mut MzViewerApp, ctx: &egui::Context, destination: Destination) {
    match destination {
        Destination::OpenProject => {
            super::project_workbench::open_project(app);
            return;
        }
        Destination::SaveProject => {
            super::project_workbench::save_project(app, ctx);
            return;
        }
        _ => {}
    }
    app.quant.active = matches!(
        destination,
        Destination::Quant
            | Destination::QuantReview
            | Destination::QuantCalibration
            | Destination::QuantQc
            | Destination::Method
    );
    let stage = match destination {
        Destination::QuantReview | Destination::Method => Some(0_usize),
        Destination::QuantCalibration => Some(1),
        Destination::QuantQc => Some(2),
        _ => None,
    };
    if let Some(stage) = stage {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("quantification_stage"), stage));
    }
    if destination == Destination::Method {
        app.quant.open_method();
    }
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
    if destination == Destination::OpenData {
        super::panels::handle_file_selection(app);
    }
    if destination == Destination::Reports && app.project.has_project() {
        let root = std::path::PathBuf::from(app.project.root_label());
        app.delivery.set_project_if_empty(&root);
    }
}

/// Compute the workflow status from live application state.
pub(super) fn steps(app: &MzViewerApp) -> Vec<StepStatus> {
    use Destination as D;
    use StepState::{Attention, Done, Pending};
    let loaded = app.files.values().filter(|f| !f.is_loading).count();
    let importing = app.files.values().filter(|f| f.is_loading).count();
    let unresolved = app.quant.unresolved_count();
    let has_results = app.quant.has_results();
    let method_valid = app.quant.method_is_valid();
    let (fitted, errored, total_targets) = app.quant.calibration_summary();
    let has_batch = app.quant.has_targeted_batch();
    let invalid = app.quant.invalid_result_count();
    let (reviewed, total) = app.quant.review_progress();
    let qc_reports = app.quant.qc_report_count();
    let qc_status = app.quant.qc_status();
    let qc_pending = app.quant.qc_pending_reviews();
    let (proposals, awaiting) = app.ai.review_summary();
    let has_project = app.project.has_project();

    let project = StepStatus {
        number: 1,
        title: TITLES[0],
        state: if has_project { Done } else { Pending },
        detail: match app.project.revision() {
            Some(revision) => format!(
                "Project open (saved revision {revision}). Save it to keep your latest work."
            ),
            None => "Create a project to keep your analyses, or open an existing one.".into(),
        },
        actions: if has_project {
            vec![("Save project", D::SaveProject), ("Open…", D::OpenProject)]
        } else {
            vec![
                ("Create project…", D::SaveProject),
                ("Open…", D::OpenProject),
            ]
        },
    };
    let data = StepStatus {
        number: 2,
        title: TITLES[1],
        state: if loaded > 0 { Done } else { Pending },
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
            "Open mzML files or a vendor folder.".into()
        },
        actions: vec![("Import data…", D::OpenData)],
    };
    let method = StepStatus {
        number: 3,
        title: TITLES[2],
        state: if !method_valid {
            Attention
        } else if loaded > 0 {
            Done
        } else {
            Pending
        },
        detail: if method_valid {
            if loaded > 0 {
                app.quant.method_summary()
            } else {
                "Define the analytes once data is imported.".into()
            }
        } else {
            app.quant.method_error()
        },
        actions: vec![("Edit method", D::Method)],
    };
    let peaks = StepStatus {
        number: 4,
        title: TITLES[3],
        state: if !has_results {
            Pending
        } else if unresolved > 0 {
            Attention
        } else {
            Done
        },
        detail: if !has_results {
            "Run the batch, then check each integrated peak.".into()
        } else if unresolved > 0 {
            format!(
                "{unresolved} peak(s) missing, ambiguous, failed or measured with older settings."
            )
        } else {
            "All peak integrations resolved.".into()
        },
        actions: vec![("Review peaks", D::QuantReview)],
    };
    let calibration = StepStatus {
        number: 5,
        title: TITLES[4],
        state: if !has_batch {
            Pending
        } else if errored > 0 || invalid > 0 {
            Attention
        } else {
            Done
        },
        detail: if !has_batch {
            "Set up the standards and fit the calibration curve.".into()
        } else if errored > 0 {
            format!("{errored} of {total_targets} calibration model(s) failed to fit.")
        } else if invalid > 0 {
            format!(
                "{invalid} concentration(s) missing, rejected or outside the calibration range."
            )
        } else {
            format!("{fitted} calibration model(s) fitted; concentrations calculated.")
        },
        actions: vec![("Calibration", D::QuantCalibration)],
    };
    let qc_ok = matches!(qc_status, Some(crate::qc::Status::Pass)) && qc_pending == 0;
    let qc = StepStatus {
        number: 6,
        title: TITLES[5],
        state: if qc_reports == 0 {
            Pending
        } else if qc_ok {
            Done
        } else {
            Attention
        },
        detail: if qc_reports == 0 {
            "Check standards, QC samples and blanks against pass/fail limits.".into()
        } else {
            match qc_status {
                Some(crate::qc::Status::Pass) if qc_pending == 0 => "Batch QC passed.".into(),
                Some(crate::qc::Status::Pass) => {
                    format!("QC passed; {qc_pending} item(s) await acknowledgement.")
                }
                Some(crate::qc::Status::Fail) => {
                    "Batch QC failed. Resolve or acknowledge before release.".into()
                }
                _ => "Batch QC is indeterminate. Review before release.".into(),
            }
        },
        actions: vec![("Batch QC", D::QuantQc)],
    };
    let results = StepStatus {
        number: 7,
        title: TITLES[6],
        state: if total == 0 {
            Pending
        } else if reviewed < total || awaiting > 0 {
            Attention
        } else {
            Done
        },
        detail: if total == 0 {
            "Accept or reject each concentration once they are calculated.".into()
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
            format!("All {total} results reviewed; {proposals} proposal(s) decided.")
        } else {
            format!("All {total} results reviewed.")
        },
        actions: vec![("Review results", D::QuantCalibration)],
    };
    let report = StepStatus {
        number: 8,
        title: TITLES[7],
        state: Pending,
        detail: if has_project {
            "Export a readable report and the evidence behind it to a new folder.".into()
        } else {
            "Needs a project: create or open one in step 1.".into()
        },
        actions: vec![("Open", D::Reports)],
    };
    vec![
        project,
        data,
        method,
        peaks,
        calibration,
        qc,
        results,
        report,
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
        blockers.push(
            "No project is open — the audit package exports retained project evidence.".into(),
        );
    }
    blockers
}

/// The first required step that is not complete: the one to work on next.
fn current_step(list: &[StepStatus]) -> Option<usize> {
    list.iter()
        .take(REQUIRED_STEPS)
        .find(|s| s.state != StepState::Done)
        .map(|s| s.number)
}

/// Status marker drawn as shapes so it never depends on font coverage:
/// check = done, "!" = needs attention, solid dot = next step, ring = not started.
fn status_icon(ui: &mut egui::Ui, state: StepState, current: bool, tokens: &super::theme::Tokens) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
    let (c, r) = (rect.center(), 8.0_f32);
    let painter = ui.painter();
    let stroke = |color| egui::Stroke::new(1.8_f32, color);
    match state {
        StepState::Done => {
            painter.circle_filled(c, r, tokens.good);
            let ink = egui::Color32::WHITE;
            painter.line_segment(
                [c + egui::vec2(-3.8, 0.2), c + egui::vec2(-1.0, 3.2)],
                stroke(ink),
            );
            painter.line_segment(
                [c + egui::vec2(-1.0, 3.2), c + egui::vec2(4.0, -2.8)],
                stroke(ink),
            );
        }
        StepState::Attention => {
            painter.circle_filled(c, r, tokens.warn);
            let ink = egui::Color32::BLACK;
            painter.line_segment(
                [c + egui::vec2(0.0, -4.2), c + egui::vec2(0.0, 0.8)],
                stroke(ink),
            );
            painter.circle_filled(c + egui::vec2(0.0, 3.6), 1.1, ink);
        }
        StepState::Pending if current => {
            painter.circle_stroke(c, r, stroke(tokens.accent));
            painter.circle_filled(c, 3.6, tokens.accent);
        }
        StepState::Pending => {
            painter.circle_stroke(c, r, stroke(tokens.text_dim));
        }
    }
}

/// The popup contents: progress, then one row per step with its buttons.
fn popup(
    ui: &mut egui::Ui,
    list: &[StepStatus],
    warnings: usize,
    project_ready: bool,
) -> Option<Destination> {
    let current = current_step(list);
    let done = list
        .iter()
        .take(REQUIRED_STEPS)
        .filter(|s| s.state == StepState::Done)
        .count();
    let mut chosen: Option<Destination> = None;
    let tokens = super::theme::Tokens::of(ui);
    ui.set_min_width(520.0);
    crate::gui::widgets::strong(ui, "Analysis workflow");
    ui.add(
        egui::ProgressBar::new(done as f32 / REQUIRED_STEPS as f32)
            .desired_height(6.0)
            .corner_radius(3),
    );
    ui.small(format!(
            "{done} of {REQUIRED_STEPS} steps complete. Each button opens the place where that step is done."
        ));
    if !warnings == 0 {
        ui.small(format!("{} open warning(s) for release", warnings));
    }
    ui.add_space(4.0);
    for step in list {
        if step.number == REQUIRED_STEPS + 1 {
            ui.separator();
        }
        let is_current = current == Some(step.number);
        egui::Frame::new()
            .fill(if is_current {
                tokens.accent_soft
            } else {
                egui::Color32::TRANSPARENT
            })
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    status_icon(ui, step.state, is_current, &tokens);
                    ui.vertical(|ui| {
                        let title = format!("{}. {}", step.number, step.title);
                        if is_current || step.state == StepState::Attention {
                            crate::gui::widgets::strong(ui, title);
                        } else {
                            ui.label(title);
                        }
                        ui.label(
                            egui::RichText::new(&step.detail)
                                .small()
                                .color(tokens.text_dim),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        for (i, (label, destination)) in step.actions.iter().enumerate().rev() {
                            let project_action = matches!(
                                destination,
                                Destination::OpenProject | Destination::SaveProject
                            );
                            let primary = i == 0 && is_current;
                            let button = egui::Button::new(*label);
                            let button = if primary {
                                button.fill(tokens.accent_soft)
                            } else {
                                button
                            };
                            if ui
                                .add_enabled(!project_action || project_ready, button)
                                .clicked()
                            {
                                chosen = Some(*destination);
                                ui.close();
                            }
                        }
                    });
                });
            });
    }
    chosen
}

/// Navigation-bar button "Workflow"; its popup lists every step with its state and
/// sends each action to the place where it is done. Returns the destination the
/// user picked; the caller navigates after it has settled the active tab.
pub(super) fn status_chip(app: &MzViewerApp, ui: &mut egui::Ui) -> Option<Destination> {
    let list = steps(app);
    let current = current_step(&list);
    let warnings = export_warnings(app);
    let project_ready = super::project_workbench::project_actions_ready(app);
    let label = match current.and_then(|n| list.get(n - 1)) {
        Some(step) => format!(
            "Workflow · step {} of {REQUIRED_STEPS}: {} \u{25be}",
            step.number, step.title
        ),
        None => "Workflow · analysis complete \u{25be}".to_string(),
    };
    let mut chosen: Option<Destination> = None;
    ui.menu_button(label, |ui| {
        chosen = popup(ui, &list, warnings.len(), project_ready);
    })
    .response
    .on_hover_text("Where you are in the analysis, from project to report");
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_cover_project_to_report() {
        let app = MzViewerApp::default();
        let list = steps(&app);
        assert_eq!(list.len(), REQUIRED_STEPS + 1);
        assert_eq!(list.first().unwrap().number, 1);
        assert_eq!(
            list.last().unwrap().title,
            "Report and audit package (optional)"
        );
        // An empty workspace starts at the project step, which offers create and open.
        assert_eq!(current_step(&list), Some(1));
        let labels: Vec<_> = list[0].actions.iter().map(|a| a.0).collect();
        assert_eq!(labels, ["Create project…", "Open…"]);
        assert!(!export_blockers(&app).is_empty());
    }

    #[test]
    fn every_step_has_an_action_that_leads_somewhere() {
        let app = MzViewerApp::default();
        for step in steps(&app) {
            assert!(
                !step.actions.is_empty(),
                "step {} has no button",
                step.number
            );
        }
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
        go(&mut app, &ctx, Destination::Reports);
        assert!(app.delivery.open && !app.quant.active);
    }

    #[test]
    fn workflow_popup_preview() {
        let mut list = steps(&MzViewerApp::default());
        // A mid-analysis picture: project and data done, peaks need attention.
        list[0].state = StepState::Done;
        list[0].detail =
            "Project open (saved revision 3). Save it to keep your latest work.".into();
        list[0].actions[0].0 = "Save project";
        list[1].state = StepState::Done;
        list[1].detail = "12 datasets loaded".into();
        list[2].state = StepState::Done;
        list[2].detail = "New quantification method · 1 analytes".into();
        list[3].state = StepState::Attention;
        list[3].detail =
            "9 peak(s) missing, ambiguous, failed or measured with older settings.".into();
        let ctx = egui::Context::default();
        ctx.style_mut(|s| s.animation_time = 0.0);
        let size = egui::vec2(640.0, 640.0);
        let mut outputs = vec![];
        for _ in 0..3 {
            outputs.push(ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        ui.set_min_width(520.0);
                        popup(ui, &list, 2, true);
                    });
                },
            ));
        }
        assert!(crate::gui::test_render::text_center(
            &outputs.last().unwrap().shapes,
            "Review peaks"
        )
        .is_some());
        if std::env::var_os("CHROMASCOPE_WORKFLOW_PREVIEW").is_some() {
            crate::gui::test_render::save(
                &ctx,
                outputs,
                std::path::Path::new("target/workflow-popup.png"),
                size,
            );
        }
    }

    #[test]
    fn warnings_cite_method_or_engine_state() {
        let app = MzViewerApp::default();
        // No batch yet: no spurious calibration warnings.
        assert!(export_warnings(&app).is_empty());
    }
}
