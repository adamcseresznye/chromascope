//! Explicit proposal review with full before/after evidence and retained decisions.
use super::MzViewerApp;
use crate::{
    domain::{Operation, Request},
    engine::{Output, Response},
    proposals::Proposal,
};
use eframe::egui;
use serde_json::Value;
type ProposalOutcome = crate::domain::Result<(Proposal, Option<Response>)>;
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(super) struct State {
    proposals: Vec<Proposal>,
    selected: usize,
    draft: String,
    reason: String,
    evidence: String,
    review_reason: String,
    message: String,
    #[serde(skip)]
    pending: Option<std::sync::mpsc::Receiver<ProposalOutcome>>,
}
fn current(app: &MzViewerApp, before: &Value) -> Option<Value> {
    let value =
        serde_json::json!({"quant":app.quant,"spectral":app.spectral,"untargeted":app.untargeted});
    crate::proposals::find_current(&value, before)
}
fn diff(before: &Value, after: &Value, path: String, rows: &mut Vec<super::table::Row>) {
    if before == after {
        return;
    }
    match (before, after) {
        (Value::Object(a), Value::Object(b)) => {
            let keys: std::collections::BTreeSet<_> = a.keys().chain(b.keys()).collect();
            for key in keys {
                diff(&before[key], &after[key], format!("{path}/{key}"), rows);
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            for i in 0..a.len().max(b.len()) {
                diff(
                    a.get(i).unwrap_or(&Value::Null),
                    b.get(i).unwrap_or(&Value::Null),
                    format!("{path}/{i}"),
                    rows,
                );
            }
        }
        _ => rows.push(super::table::Row {
            key: path.clone(),
            cells: vec![
                super::table::Cell::text(path),
                super::table::Cell::text(before),
                super::table::Cell::text(after),
            ],
        }),
    }
}
pub(super) fn retain(app: &mut MzViewerApp, response: Response) -> Result<(), String> {
    match &response.output {
        Output::TargetedQuantification { batch } => app.quant.retain_targeted(*batch.clone()),
        Output::QcReport { report } => app.quant.retain_qc(*report.clone()),
        Output::ChromatographicProcessing {
            analyses,
            preview: false,
        } => {
            for analysis in analyses {
                app.quant.retain_chromatography(analysis.clone())?;
            }
            Ok(())
        }
        Output::SpectralSearch { .. } => {
            app.spectral.retain(response);
            Ok(())
        }
        Output::FeatureAnnotations { ledger } => app.untargeted.retain_annotations(*ledger.clone()),
        _ => Err("Unsupported applied review output".into()),
    }
}
pub(super) fn poll(app: &mut MzViewerApp, ctx: &egui::Context) {
    let mut state = std::mem::take(&mut app.ai);
    if let Some(rx) = &state.pending {
        match rx.try_recv() {
            Ok(result) => {
                state.pending = None;
                match result {
                    Ok((proposal, response)) => {
                        if let Some(response) = response {
                            if let Err(error) = retain(app, response) {
                                state.message = error;
                                app.ai = state;
                                return;
                            }
                        }
                        if let Some(index) =
                            state.proposals.iter().position(|old| old.id == proposal.id)
                        {
                            state.proposals[index] = proposal;
                            state.selected = index;
                        } else {
                            state.proposals.push(proposal);
                            state.selected = state.proposals.len() - 1;
                        }
                        state.message="Suggestion/review retained. Save the project revision to persist this activity.".into();
                    }
                    Err(error) => state.message = error.to_string(),
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                state.pending = None;
                state.message = "Proposal worker disconnected; original evidence retained".into();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100))
            }
        }
    }
    app.ai = state;
}
pub(super) fn panel(app: &mut MzViewerApp, ui: &mut egui::Ui) {
    let revision = app.project.revision();
    let mut state = std::mem::take(&mut app.ai);
    ui.heading("Review proposals");
    ui.label("Observations do not change evidence. Suggestions retain original and proposed values. Approval applies a new engine review revision; rejection retains the suggestion without changing results.");
    if ui.button("Load MCP / saved review proposal…").clicked() {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Proposal JSON", &["json"])
            .pick_file()
        {
            match std::fs::read(path)
                .map_err(|e| e.to_string())
                .and_then(|bytes| {
                    serde_json::from_slice::<Proposal>(&bytes).map_err(|e| e.to_string())
                }) {
                Ok(proposal) => {
                    state.proposals.push(proposal);
                    state.selected = state.proposals.len() - 1;
                }
                Err(error) => state.message = error,
            }
        }
    }
    ui.collapsing("Prepare a new review suggestion", |ui| {
        if ui
            .add_enabled(
                app.quant.suggestion().is_some(),
                egui::Button::new("Prepare selected concentration review"),
            )
            .clicked()
        {
            if let Some(operation) = app.quant.suggestion() {
                state.draft = serde_json::to_string_pretty(&operation).unwrap();
            }
        }
        ui.label("Revision-bound review operation");
        super::forms::guided::<Operation>(
            ui,
            "Proposed review controls",
            "Routine review fields match the selected concentration. Approval always requires your reason below.",
            &mut state.draft,
        );
        ui.collapsing("Advanced operation JSON — expert exact-operation audit", |ui| {
            ui.small("Routine fields are in the guided controls above. This text is retained so the exact proposed operation can be audited before approval.");
            ui.add(
                egui::TextEdit::multiline(&mut state.draft)
                    .code_editor()
                    .desired_rows(6)
                    .desired_width(f32::INFINITY),
            );
        });
        ui.label("Proposal reason");
        ui.text_edit_singleline(&mut state.reason);
        ui.label("Evidence references (one per line)");
        ui.add(egui::TextEdit::multiline(&mut state.evidence).desired_rows(2));
        if ui
            .add_enabled(
                state.pending.is_none(),
                egui::Button::new("Preview and retain suggestion"),
            )
            .clicked()
        {
            match serde_json::from_str::<Operation>(&state.draft) {
                Ok(operation) => {
                    let request = Request {
                        version: 1,
                        operation_id: Default::default(),
                        actor: "desktop-suggestion".into(),
                        operation,
                    };
                    let reason = state.reason.clone();
                    let evidence = state.evidence.lines().map(str::to_owned).collect();
                    let (tx, rx) = std::sync::mpsc::channel();
                    std::thread::spawn(move || {
                        let _ = tx.send(
                            crate::proposals::prepare(
                                std::path::Path::new("-"),
                                request,
                                revision,
                                reason,
                                evidence,
                                &Default::default(),
                            )
                            .map(|p| (p, None)),
                        );
                    });
                    state.pending = Some(rx);
                }
                Err(error) => state.message = error.to_string(),
            }
        }
    });
    egui::ComboBox::from_label("Proposal")
        .selected_text(format!("{}", state.selected + 1))
        .show_ui(ui, |ui| {
            for (index, proposal) in state.proposals.iter().enumerate() {
                ui.selectable_value(
                    &mut state.selected,
                    index,
                    format!(
                        "{} · {}",
                        proposal.id,
                        proposal
                            .events
                            .last()
                            .map(|e| e.decision.as_str())
                            .unwrap_or("suggestion")
                    ),
                );
            }
        });
    if let Some(proposal) = state.proposals.get(state.selected).cloned() {
        ui.label(format!(
            "Proposer: {} · project revision {:?}",
            proposal.request.actor, proposal.expected_project_revision
        ));
        ui.label(&proposal.reason);
        for evidence in &proposal.evidence {
            ui.label(format!("Evidence: {evidence}"));
        }
        let mut rows = vec![];
        diff(
            &proposal.before,
            &proposal.proposed,
            String::new(),
            &mut rows,
        );
        super::table::show(
            ui,
            "proposal_changes",
            &["Field", "Original", "Proposed"],
            &rows,
            None,
        );
        ui.small("All changed fields are available through filtering and scrolling; complete originals and proposals remain in the saved proposal JSON.");
        ui.label("Review / reversal reason");
        ui.text_edit_singleline(&mut state.review_reason);
        ui.horizontal_wrapped(|ui|{
            for (label,approve) in [("Approve proposal",true),("Reject proposal",false)]{
                if ui.add_enabled(state.pending.is_none()&&proposal.events.is_empty()&&!state.review_reason.trim().is_empty(),egui::Button::new(label)).clicked(){
                    if let Some(current)=current(app,&proposal.before){let reason=state.review_reason.clone();let proposal=proposal.clone();let(tx,rx)=std::sync::mpsc::channel();std::thread::spawn(move||{let _=tx.send(crate::proposals::resolve(std::path::Path::new("-"),&proposal,&current,revision,approve,"desktop-reviewer",&reason,&Default::default()));});state.pending=Some(rx);}else{state.message="Proposal source is absent from this workbench. Reopen its project before reviewing.".into();}
                }
            }
            if ui.add_enabled(proposal.applied.is_some()&&!state.review_reason.trim().is_empty(),egui::Button::new("Prepare reversal")).clicked(){match crate::proposals::reversal(&proposal,"desktop-reviewer",&state.review_reason){Ok(request)=>{state.draft=serde_json::to_string_pretty(&request.operation).unwrap();state.reason=state.review_reason.clone();state.evidence=proposal.id.to_string();},Err(error)=>state.message=error.to_string()}}
            if ui.button("Prepare revised suggestion").clicked(){state.draft=serde_json::to_string_pretty(&proposal.request.operation).unwrap();state.reason=proposal.reason.clone();state.evidence=proposal.evidence.join("\n");}
            if ui.button("Save proposal and decisions…").clicked(){if let Some(path)=rfd::FileDialog::new().save_file(){use std::io::Write;state.message=serde_json::to_vec_pretty(&proposal).map_err(|e|e.to_string()).and_then(|bytes|std::fs::OpenOptions::new().create_new(true).write(true).open(path).and_then(|mut f|f.write_all(&bytes)).map_err(|e|e.to_string())).err().unwrap_or_else(||"Proposal saved".into());}}
        });
        for event in &proposal.events {
            ui.label(format!(
                "{} · {} · {}",
                event.decision, event.actor, event.reason
            ));
        }
    }
    if state.pending.is_some() {
        ui.spinner();
        ui.label("Verifying shared-engine proposal…");
    }
    ui.label(&state.message);
    app.ai = state;
}

impl State {
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some()
    }
    /// `(total_proposals, awaiting_decision)` — proposals without any
    /// recorded approve/reject event still need scientist review.
    pub(super) fn review_summary(&self) -> (usize, usize) {
        let awaiting = self
            .proposals
            .iter()
            .filter(|p| p.events.is_empty())
            .count();
        (self.proposals.len(), awaiting)
    }
}

impl State {
    pub(super) fn merge_project_reviews(&mut self, proposals: &[Proposal]) {
        for proposal in proposals {
            if let Some(old) = self.proposals.iter_mut().find(|old| old.id == proposal.id) {
                if proposal.events.len() > old.events.len() {
                    *old = proposal.clone();
                }
            } else {
                self.proposals.push(proposal.clone());
            }
        }
    }
}
