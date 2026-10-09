//! Desktop adapter for the shared targeted quantification engine.
use crate::{quant::Method, targeted as t};
use eframe::egui;
use egui_plot::{Line, Points};
use std::sync::mpsc;

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(super) struct UiState {
    qc: super::qc::State,
    draft: String,
    history: Vec<t::BatchResult>,
    current_batch: usize,
    #[serde(skip)]
    receiver: Option<mpsc::Receiver<crate::domain::Result<t::BatchResult>>>,
    #[serde(skip)]
    control: Option<crate::jobs::JobControl>,
    selected: usize,
    reason: String,
    message: String,
}
impl Drop for UiState {
    fn drop(&mut self) {
        if let Some(control) = &self.control {
            control.cancel();
        }
    }
}
impl UiState {
    pub(super) fn busy(&self) -> bool {
        self.receiver.is_some()
    }
    pub(super) fn latest(&self) -> Option<&crate::targeted::BatchResult> {
        self.history.get(self.current_batch)
    }
    pub(super) fn batch_count(&self) -> usize {
        self.history.len()
    }
    /// Per-target calibration outcome of the current batch.
    /// Returns `(fitted_targets, errored_targets, total_targets)` using only
    /// the engine's own calibration maps — no independent acceptance policy.
    pub(super) fn calibration_summary(&self) -> (usize, usize, usize) {
        let Some(batch) = self.history.get(self.current_batch) else {
            return (0, 0, 0);
        };
        let total = batch.request.targets.len();
        let fitted = batch.calibrations.len();
        let errored = batch.calibration_errors.len();
        (fitted, errored, total)
    }
    /// Concentration results that are not successfully quantified in the
    /// current batch (missing, rejected, below/above limits, failed, ambiguous).
    pub(super) fn invalid_result_count(&self) -> usize {
        use crate::targeted::State as TState;
        self.history
            .get(self.current_batch)
            .map(|batch| {
                batch
                    .results
                    .iter()
                    .filter(|r| !matches!(r.state, TState::Present))
                    .count()
            })
            .unwrap_or(0)
    }
    /// Results explicitly reviewed by a scientist in the current batch.
    pub(super) fn reviewed_result_count(&self) -> usize {
        self.history
            .get(self.current_batch)
            .map(|batch| batch.results.iter().filter(|r| r.reviewed).count())
            .unwrap_or(0)
    }
    pub(super) fn total_result_count(&self) -> usize {
        self.history
            .get(self.current_batch)
            .map(|batch| batch.results.len())
            .unwrap_or(0)
    }
}
impl UiState {
    pub(super) fn retain(&mut self, batch: crate::targeted::BatchResult) -> Result<(), String> {
        crate::targeted::verify(&batch).map_err(|e| e.to_string())?;
        if self
            .history
            .iter()
            .any(|old| old.batch_id == batch.batch_id && old.reviews.len() >= batch.reviews.len())
        {
            return Ok(());
        }
        self.history.push(batch);
        self.current_batch = self.history.len() - 1;
        Ok(())
    }
    pub(super) fn retain_qc(&mut self, report: crate::qc::Report) -> Result<(), String> {
        self.qc.retain(report)
    }
    pub(super) fn suggestion(&self) -> Option<crate::domain::Operation> {
        let batch = self.history.get(self.current_batch)?;
        let row = batch.results.get(self.selected)?;
        Some(crate::domain::Operation::ReviewTargeted {
            batch: Box::new(batch.clone()),
            expected_revision: batch.reviews.len(),
            sample: row.sample.clone(),
            target: row.target.clone(),
            accepted: row.state != crate::targeted::State::Rejected,
            reason: "Review selected concentration".into(),
        })
    }
    pub(super) fn qc_panel(&mut self, ui: &mut egui::Ui) {
        super::qc::panel(&mut self.qc, self.history.get(self.current_batch), ui);
    }
    pub(super) fn qc_report_count(&self) -> usize {
        self.qc.report_count()
    }
    pub(super) fn qc_latest_status(&self) -> Option<crate::qc::Status> {
        self.qc.latest_status()
    }
    pub(super) fn qc_pending_reviews(&self) -> usize {
        self.qc.pending_review_count()
    }
}
fn write_new(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)
}
pub(super) fn panel(
    state: &mut UiState,
    method: &Method,
    samples: &[(String, String)],
    ui: &mut egui::Ui,
) {
    poll(state, ui.ctx());
    ui.collapsing("Targeted concentrations, calibration and QC", |ui| {
        ui.label("Standard/QC nominal levels describe the injected solution. Sample concentration = inverse response × dilution. Areas: instrument intensity × minute; response ratios are dimensionless.");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Configure targeted batch from selection").clicked() {
                let config = t::CalibrationConfig { degree: 1, intercept: t::Intercept::Free, weighting: t::Weighting::Unweighted, unit: t::Unit::NgMl, range: [1.0, 100.0], lod: 0.0, loq: 1.0, accuracy_tolerance_percent: 15.0, qc_cv_limit_percent: 15.0, blank_response_limit: 0.0 };
                let request = t::BatchRequest {
                    version: 1, name: method.name.clone(), detection_smoothing: method.detection_smoothing, minimum_height: method.minimum_height, boundary_fraction: method.boundary_fraction,
                    targets: method.analytes.iter().enumerate().map(|(i, a)| t::Target { id: format!("target-{}", i+1), quantifier: t::Ion { extraction: a.clone() }, qualifiers: vec![], internal_standard: None, is_area_range: None, calibration: Some(config.clone()) }).collect(),
                    samples: samples.iter().enumerate().map(|(i, (source, _))| t::Sample { id: format!("sample-{}", i+1), source: source.clone(), role: t::Role::Unknown, injection_order: i as u32 + 1, dilution: 1.0, nominal: Default::default(), exclusions: Default::default() }).collect(),
                };
                state.draft = serde_json::to_string_pretty(&request).unwrap();
            }
            if ui.button("Open targeted request…").clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file() {
                    match std::fs::read_to_string(path) { Ok(text) => state.draft = text, Err(e) => state.message = e.to_string() }
                }
            }
            if ui.button("Open targeted history…").clicked() {
                if let Some(path) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file() {
                    let result = std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|text| serde_json::from_str::<Vec<t::BatchResult>>(&text).map_err(|e| e.to_string())).and_then(|batches| { for batch in &batches { t::verify(batch).map_err(|e| e.to_string())?; } Ok(batches) });
                    match result { Ok(batches) => { state.history.extend(batches); state.current_batch=state.history.len().saturating_sub(1); state.selected = 0; }, Err(e) => state.message = e }
                }
            }
        });
        if let Ok(mut request) = serde_json::from_str::<t::BatchRequest>(&state.draft) {
            let mut changed = false;
            ui.collapsing("Sample sheet — roles, order, dilution, nominal levels", |ui| {
                ui.small("Assign each injection its role and nominal level. Unknown samples are quantified; standards and QCs calibrate and verify.");
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    egui::Grid::new("targeted_sheet").striped(true).show(ui, |ui| {
                        for label in ["Sample ID / source", "Role", "Injection order", "Dilution"] { ui.label(label); }
                        for target in request.targets.iter().filter(|t| t.calibration.is_some()) { ui.label(format!("{} nominal ({:?})", target.id, target.calibration.as_ref().unwrap().unit)); }
                        ui.end_row();
                        for (i, sample) in request.samples.iter_mut().enumerate() {
                            ui.vertical(|ui| { changed |= ui.text_edit_singleline(&mut sample.id).changed(); changed |= ui.text_edit_singleline(&mut sample.source).changed(); });
                            egui::ComboBox::from_id_salt(("targeted_role", i)).selected_text(format!("{:?}", sample.role)).show_ui(ui, |ui| { for role in [t::Role::Standard, t::Role::Blank, t::Role::Qc, t::Role::Unknown] { changed |= ui.selectable_value(&mut sample.role, role.clone(), format!("{role:?}")).changed(); } });
                            changed |= ui.add(egui::DragValue::new(&mut sample.injection_order)).changed();
                            changed |= ui.add(egui::DragValue::new(&mut sample.dilution).speed(0.1)).changed();
                            for target in request.targets.iter().filter(|t| t.calibration.is_some()) {
                                if matches!(sample.role, t::Role::Standard | t::Role::Qc) { changed |= ui.add(egui::DragValue::new(sample.nominal.entry(target.id.clone()).or_insert(0.0)).speed(0.1)).changed(); } else { ui.label("—"); }
                            }
                            ui.end_row();
                        }
                    });
                });
            });
            ui.collapsing("Calibration and analyte settings", |ui| {
                if ui.button("Add target from first target").clicked() {
                    if let Some(mut target) = request.targets.first().cloned() { target.id = format!("target-{}", request.targets.len()+1); request.targets.push(target); changed = true; }
                }
                for target in &mut request.targets {
                    ui.push_id(target.id.clone(), |ui| {
                        ui.collapsing(target.id.clone(), |ui| {
                            changed |= ui.text_edit_singleline(&mut target.id).changed();
                            ui.horizontal(|ui| {
                                ui.label("Internal standard target ID (empty = area response)");
                                let mut id = target.internal_standard.clone().unwrap_or_default();
                                if ui.text_edit_singleline(&mut id).changed() {target.internal_standard=if id.trim().is_empty(){None}else{Some(id)};changed=true;}
                            });
                            ion_editor(&mut target.quantifier, &mut changed, ui);
                            if ui.button("Add qualifier ion").clicked() { target.qualifiers.push(t::Qualifier { ion: target.quantifier.clone(), ratio_range: [0.0, 1.0], rt_tolerance_minutes: 0.1 }); changed = true; }
                            let mut remove_qualifier = None;
                            for (index, qualifier) in target.qualifiers.iter_mut().enumerate() {
                                ui.push_id(index, |ui| {
                                    ui.label(format!("Qualifier {}", index+1));
                                    if ui.button("Remove qualifier").clicked() { remove_qualifier = Some(index); }
                                    ion_editor(&mut qualifier.ion, &mut changed, ui);
                                    let [low, high] = &mut qualifier.ratio_range;
                                    ui.horizontal(|ui| {ui.label("Area ratio range");changed|=ui.add(egui::DragValue::new(low).speed(0.01)).changed();changed|=ui.add(egui::DragValue::new(high).speed(0.01)).changed();ui.label("RT tolerance (min)");changed|=ui.add(egui::DragValue::new(&mut qualifier.rt_tolerance_minutes).speed(0.01)).changed();});
                                });
                            }
                            if let Some(index) = remove_qualifier { target.qualifiers.remove(index); changed = true; }
                            let mut enabled = target.is_area_range.is_some();
                            if ui.checkbox(&mut enabled,"Apply internal-standard area bounds").changed() { target.is_area_range = enabled.then_some([0.0,1e12]); changed = true; }
                            if let Some(range) = &mut target.is_area_range {
                                let [low, high]=range;
                                ui.horizontal(|ui| {ui.label("IS area bounds (intensity × min)");changed|=ui.add(egui::DragValue::new(low)).changed();changed|=ui.add(egui::DragValue::new(high)).changed();});
                            }
                            if let Some(config) = &mut target.calibration {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label("Polynomial degree");changed|=ui.add(egui::DragValue::new(&mut config.degree).range(1..=3)).changed();
                                    egui::ComboBox::from_id_salt("weights").selected_text(format!("{:?}",config.weighting)).show_ui(ui,|ui| {for w in [t::Weighting::Unweighted,t::Weighting::InverseX,t::Weighting::InverseX2] {changed|=ui.selectable_value(&mut config.weighting,w.clone(),format!("{w:?}")).changed();}});
                                    egui::ComboBox::from_id_salt("intercept").selected_text(format!("{:?}",config.intercept)).show_ui(ui,|ui| {for intercept in [t::Intercept::Free,t::Intercept::Zero,t::Intercept::Fixed(0.0)] {changed|=ui.selectable_value(&mut config.intercept,intercept.clone(),format!("{intercept:?}")).changed();}});
                                    if let t::Intercept::Fixed(value)=&mut config.intercept {changed|=ui.add(egui::DragValue::new(value)).changed();}
                                    egui::ComboBox::from_id_salt("units").selected_text(config.unit.label()).show_ui(ui,|ui| {for unit in [t::Unit::NgMl,t::Unit::UgMl,t::Unit::MgL,t::Unit::NmolL,t::Unit::UmolL] {changed|=ui.selectable_value(&mut config.unit,unit.clone(),unit.label()).changed();}});
                                });
                                let [low, high]=&mut config.range;
                                ui.horizontal_wrapped(|ui| {
                                    for (label,value) in [("Range min",low),("Range max",high),("LOD",&mut config.lod),("LOQ",&mut config.loq),("Accuracy tolerance %",&mut config.accuracy_tolerance_percent),("Precision limit %",&mut config.qc_cv_limit_percent),("Blank response limit",&mut config.blank_response_limit)] {
                                        ui.label(label);changed|=ui.add(egui::DragValue::new(value).speed(0.1)).changed();
                                    }
                                });
                            }
                        });
                    });
                }
            });
            if changed { state.draft = serde_json::to_string_pretty(&request).unwrap(); }
        }
        super::forms::typed::<t::BatchRequest>(ui,"Additional sample exclusions and target controls",&mut state.draft);
        ui.collapsing("Advanced method / sample JSON editor", |ui| {
            ui.label("Edit degree (1/2/3), intercept (free/zero/fixed), weighting (unweighted/inverse_x/inverse_x2), range, LOD/LOQ, units and QC thresholds. Internal standards are separate targets with is_area_range, calibration=null; reference their ID in internal_standard. Qualifiers specify extraction and ratio_range. RT windows are in minutes. Exclusions map target ID to a reason. Saved runs retain their original request.");
            ui.add(egui::TextEdit::multiline(&mut state.draft).code_editor().desired_rows(12).desired_width(f32::INFINITY));
        });
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(state.receiver.is_none() && !state.draft.is_empty(), egui::Button::new("Run targeted quantification")).clicked() {
                let request = serde_json::from_str::<t::BatchRequest>(&state.draft).map_err(|e| crate::domain::EngineError::new("invalid_parameters", e));
                match request.and_then(|r| { t::validate(&r)?; Ok(r) }) {
                    Ok(request) => {
                        let (tx, rx) = mpsc::channel(); let control = crate::jobs::JobControl::default();
                        state.control = Some(control.clone()); state.receiver = Some(rx);
                        std::thread::spawn(move || { let _ = tx.send(t::run(request, &control)); });
                        state.message = "Quantifying raw sources…".into();
                    }
                    Err(e) => state.message = e.to_string(),
                }
            }
            if state.receiver.is_some() { ui.spinner(); if ui.button("Cancel targeted run").clicked() { if let Some(control) = &state.control { control.cancel(); } } }
            if ui.button("Save targeted request…").clicked() { if let Some(path) = rfd::FileDialog::new().set_file_name("targeted-request.json").save_file() { state.message = match write_new(&path, state.draft.as_bytes()) { Ok(()) => "Request saved".into(), Err(e) => e.to_string() }; } }
            if !state.history.is_empty() && ui.button("Save targeted history…").clicked() { if let Some(path) = rfd::FileDialog::new().set_file_name("targeted-history.json").save_file() { state.message = match serde_json::to_vec_pretty(&state.history).map_err(|e| e.to_string()).and_then(|bytes| write_new(&path, &bytes).map_err(|e| e.to_string())) { Ok(()) => "All runs and reviews saved".into(), Err(e) => e }; } }
        });
        ui.label(&state.message);
        results(state, ui);
    });
}
fn ion_editor(ion: &mut t::Ion, changed: &mut bool, ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        *changed |= ui
            .text_edit_singleline(&mut ion.extraction.extraction.name)
            .changed();
        if let Some(mass) = &mut ion.extraction.extraction.mass {
            ui.label("m/z");
            *changed |= ui.add(egui::DragValue::new(mass).speed(0.01)).changed();
        }
        ui.label("Tolerance (ppm)");
        *changed |= ui
            .add(egui::DragValue::new(&mut ion.extraction.extraction.ppm))
            .changed();
        ui.label("Expected RT (min)");
        *changed |= ui
            .add(egui::DragValue::new(&mut ion.extraction.expected_rt).speed(0.01))
            .changed();
        let [low, high] = &mut ion.extraction.rt_window;
        ui.label("RT window (min)");
        *changed |= ui.add(egui::DragValue::new(low).speed(0.01)).changed();
        *changed |= ui.add(egui::DragValue::new(high).speed(0.01)).changed();
    });
}
fn results(state: &mut UiState, ui: &mut egui::Ui) {
    let count = state.history.len();
    if count > 0 {
        egui::ComboBox::from_id_salt("targeted_history_run")
            .selected_text(format!("Run {}", state.current_batch + 1))
            .show_ui(ui, |ui| {
                for (index, batch) in state.history.iter().enumerate() {
                    if ui
                        .selectable_value(
                            &mut state.current_batch,
                            index,
                            format!(
                                "Run {}: {} ({})",
                                index + 1,
                                batch.request.name,
                                batch.batch_id
                            ),
                        )
                        .changed()
                    {
                        state.selected = 0;
                    }
                }
            });
    }
    super::qc::panel(&mut state.qc, state.history.get(state.current_batch), ui);
    let Some(batch) = state.history.get_mut(state.current_batch) else {
        return;
    };
    ui.label(format!(
        "{count} retained runs; {} review revisions",
        batch.reviews.len()
    ));
    for (target, error) in &batch.calibration_errors {
        ui.colored_label(egui::Color32::RED, format!("{target}: {error}"));
    }
    use super::table::{Cell, Row};
    let rows: Vec<_> = batch
        .results
        .iter()
        .map(|row| Row {
            key: format!("{}/{}", row.sample, row.target),
            cells: vec![
                Cell::text(&row.sample),
                Cell::text(&row.target),
                Cell::text(format!("{:?}", row.state)),
                if row.state == t::State::Rejected {
                    Cell::text("Rejected")
                } else {
                    Cell::number(row.concentration)
                },
                Cell::text(row.unit.label()),
                Cell::number(row.accuracy_percent),
                Cell::text(row.flags.join("; ")),
                Cell::text(row.reviewed),
            ],
        })
        .collect();
    let selected = rows.get(state.selected).map(|row| row.key.as_str());
    if let Some(index) = super::table::show(
        ui,
        "targeted_concentrations",
        &[
            "Sample",
            "Target",
            "State",
            "Concentration",
            "Unit",
            "Accuracy %",
            "Flags",
            "Reviewed",
        ],
        &rows,
        selected,
    ) {
        state.selected = index;
    }
    let Some(row) = batch.results.get(state.selected).cloned() else {
        return;
    };
    ui.horizontal_wrapped(|ui| {
        ui.label("Review reason");
        ui.text_edit_singleline(&mut state.reason);
        for (label, accepted) in [
            ("Accept concentration", true),
            ("Reject concentration", false),
        ] {
            if ui.button(label).clicked() {
                match t::review(
                    batch,
                    batch.reviews.len(),
                    &row.sample,
                    &row.target,
                    accepted,
                    "gui",
                    &state.reason,
                ) {
                    Ok(next) => *batch = next,
                    Err(e) => state.message = e.to_string(),
                }
            }
        }
        if ui.button("Export concentrations CSV…").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("concentrations.csv")
                .save_file()
            {
                state.message = match t::csv(batch)
                    .map_err(|e| e.to_string())
                    .and_then(|csv| write_new(&path, csv.as_bytes()).map_err(|e| e.to_string()))
                {
                    Ok(()) => "CSV exported".into(),
                    Err(e) => e,
                };
            }
        }
        if ui.button("Export calibration CSV…").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("calibration.csv")
                .save_file()
            {
                state.message = match t::calibration_csv(batch)
                    .map_err(|e| e.to_string())
                    .and_then(|csv| write_new(&path, csv.as_bytes()).map_err(|e| e.to_string()))
                {
                    Ok(()) => "Calibration CSV exported".into(),
                    Err(e) => e,
                };
            }
        }
    });
    if let Some(observation) = batch
        .observations
        .iter()
        .find(|o| o.sample == row.sample && o.target == row.target)
    {
        super::plot_controls::export(
            ui,
            &observation.quantifier.trace,
            "RT (min)",
            "Instrument intensity",
        );
        super::plot_controls::plot(ui, "targeted_ions")
            .height(150.0)
            .x_axis_label("RT (minutes)")
            .y_axis_label("Instrument intensity")
            .show(ui, |p| {
                p.line(Line::new(
                    "Quantifier",
                    observation.quantifier.trace.clone(),
                ));
                for (i, qualifier) in observation.qualifiers.iter().enumerate() {
                    p.line(Line::new(
                        format!("Qualifier {}", i + 1),
                        qualifier.trace.clone(),
                    ));
                }
            });
        ui.label(format!(
            "Qualifier ratios {:?}; response {:?}; injected concentration {:?}; dilution {}",
            row.qualifier_ratios, row.response, row.injected_concentration, row.dilution
        ));
    }
    if let Some(model) = batch.calibrations.get(&row.target) {
        let config = batch
            .request
            .targets
            .iter()
            .find(|t| t.id == row.target)
            .unwrap()
            .calibration
            .as_ref()
            .unwrap();
        ui.label(format!("R² {:?}; weighted RMSE {:.3e}; condition {:.3e}; residual df {}; coefficients in x/scale {:?}; scale {}; flags {}", model.r_squared, model.weighted_rmse, model.condition_number, model.residual_degrees_of_freedom, model.coefficients, model.scale, model.flags.join("; ")));
        super::plot_controls::plot(ui, "targeted_calibration")
            .height(160.0)
            .x_axis_label(format!("Concentration ({})", config.unit.label()))
            .y_axis_label(
                if batch
                    .request
                    .targets
                    .iter()
                    .find(|t| t.id == row.target)
                    .is_some_and(|t| t.internal_standard.is_some())
                {
                    "Area / IS area (dimensionless)"
                } else {
                    "Area (instrument intensity × min)"
                },
            )
            .show(ui, |p| {
                p.line(Line::new(
                    "Fit",
                    (0..=200)
                        .map(|i| {
                            let x = config.range[0]
                                + (config.range[1] - config.range[0]) * i as f64 / 200.0;
                            [x, model.response(x)]
                        })
                        .collect::<Vec<_>>(),
                ));
                p.points(Points::new(
                    "Standards",
                    model
                        .points
                        .iter()
                        .map(|v| [v.x, v.response])
                        .collect::<Vec<_>>(),
                ));
            });
        super::plot_controls::plot(ui, "targeted_residuals")
            .height(120.0)
            .x_axis_label(format!("Concentration ({})", config.unit.label()))
            .y_axis_label("Observed - fitted response")
            .show(ui, |p| {
                p.hline(egui_plot::HLine::new("Zero residual", 0.0));
                p.points(Points::new(
                    "Residuals",
                    model
                        .points
                        .iter()
                        .map(|v| [v.x, v.residual])
                        .collect::<Vec<_>>(),
                ));
            });
        ui.collapsing("Back-calculated standards", |ui| {
            super::table::records(
                ui,
                "calibration-standards",
                serde_json::to_value(&model.points).unwrap(),
                "sample",
                None,
            );
        });
        let residuals: Vec<_> = model.points.iter().map(|v| [v.x, v.residual]).collect();
        super::plot_controls::export(
            ui,
            &residuals,
            config.unit.label(),
            "Observed - fitted response",
        );
    }
    for precision in &batch.precision {
        ui.label(format!(
            "{:?} {} level {}: n {}/{}, mean {}, SD {:?}, CV {:?}%, {}",
            precision.role,
            precision.target,
            precision.nominal,
            precision.n,
            precision.sample_count,
            precision
                .mean
                .map(|v| v.to_string())
                .unwrap_or_else(|| "missing".into()),
            precision.sd,
            precision.cv_percent,
            precision.flags.join("; ")
        ));
    }
}

pub(super) fn poll(state: &mut UiState, ctx: &egui::Context) {
    if let Some(rx) = &state.receiver {
        match rx.try_recv() {
            Ok(result) => {
                state.receiver = None;
                state.control = None;
                match result {
                    Ok(batch) => {
                        state.history.push(batch);
                        state.current_batch = state.history.len() - 1;
                        state.selected = 0;
                        state.message =
                            "Run retained. Inspect flags, review concentrations, and save history."
                                .into();
                    }
                    Err(e) => state.message = e.to_string(),
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                state.receiver = None;
                state.message = "Targeted worker disconnected".into();
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as chromascope;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/targeted_support.rs"
    ));

    fn frame(
        state: &mut UiState,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1500.0, 1900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| panel(state, &Method::default(), &[], ui));
            },
        )
    }
    fn click(
        state: &mut UiState,
        ctx: &egui::Context,
        label: &str,
        frames: &mut Vec<egui::FullOutput>,
    ) {
        let output = frame(state, ctx, vec![]);
        let pos = super::super::test_render::text_center(&output.shapes, label)
            .unwrap_or_else(|| panic!("Missing {label}"));
        frames.push(output);
        frames.push(frame(
            state,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ],
        ));
        frames.push(frame(
            state,
            ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        ));
    }
    #[test]
    fn raw_batch_background_run_and_reversible_review_through_egui() {
        let temp = tempfile::tempdir().unwrap();
        let request = raw_request(temp.path());
        let expected = t::run(request.clone(), &crate::jobs::JobControl::default()).unwrap();
        let mut state = UiState::default();
        state.draft = serde_json::to_string_pretty(&request).unwrap();
        state.reason = "Compared raw triangular reference and ion ratios".into();
        let ctx = egui::Context::default();
        ctx.style_mut(|style| style.animation_time = 0.0);
        let mut frames = vec![frame(&mut state, &ctx, vec![])];
        click(
            &mut state,
            &ctx,
            "Targeted concentrations, calibration and QC",
            &mut frames,
        );
        click(&mut state, &ctx, "Run targeted quantification", &mut frames);
        assert!(state.receiver.is_some(), "{}", state.message);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while state.receiver.is_some() && std::time::Instant::now() < deadline {
            frames.push(frame(&mut state, &ctx, vec![]));
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(state.receiver.is_none(), "Worker did not finish");
        assert_eq!(state.history.len(), 1, "{}", state.message);
        assert_eq!(state.history[0].results, expected.results);
        click(&mut state, &ctx, "raw6", &mut frames);
        click(&mut state, &ctx, "Accept concentration", &mut frames);
        assert!(state.history[0].results[6].reviewed);
        click(&mut state, &ctx, "Reject concentration", &mut frames);
        assert_eq!(state.history[0].results[6].state, t::State::Rejected);
        click(&mut state, &ctx, "Accept concentration", &mut frames);
        assert_eq!(state.history[0].reviews.len(), 3);
        assert_eq!(
            state.history[0].observations[0].quantifier.trace,
            expected.observations[0].quantifier.trace
        );
        t::verify(&state.history[0]).unwrap();
        frames.push(frame(&mut state, &ctx, vec![]));
        if std::env::var_os("CHROMASCOPE_TARGETED_PREVIEW").is_some() {
            super::super::test_render::save(
                &ctx,
                frames,
                std::path::Path::new("target/targeted-review.png"),
                egui::vec2(1500.0, 1900.0),
            );
        }
    }
}
