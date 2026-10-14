//! Desktop adapter for the shared targeted quantification engine.
use crate::{
    quant::{Measurement, Method},
    targeted as t,
};
use eframe::egui;
use egui_plot::Line;
use std::sync::mpsc;

mod browser;

/// The calibration tab shows either the setup steps or the results browser.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Mode {
    #[default]
    Setup,
    Browser,
}

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
    #[serde(skip)]
    cal: super::calibration::State,
    #[serde(skip)]
    mode: Mode,
    #[serde(skip)]
    mode_chosen: bool,
    /// Analyte whose results the browser shows.
    #[serde(skip)]
    browse_target: String,
    #[serde(skip)]
    filter: browser::Filter,
    /// 0 = peak, 1 = trends.
    #[serde(skip)]
    peak_tab: usize,
    /// What the user set up for the calibration: samples, levels and per-analyte calibration.
    #[serde(default)]
    setup: super::calibration_setup::Setup,
    /// Signature of the peaks and settings behind the latest calculation (detects stale results).
    #[serde(default)]
    calculated: Option<u64>,
    #[serde(skip)]
    pending_signature: Option<u64>,
    /// Hashes of raw files already read, so recalculating does not hash them again.
    #[serde(skip)]
    hashes: std::collections::BTreeMap<String, String>,
    /// A peak the user wants to look at again in tab 1: (source file, analyte).
    #[serde(skip)]
    revisit: Option<(String, String)>,
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
    pub(super) fn take_revisit(&mut self) -> Option<(String, String)> {
        self.revisit.take()
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
fn role_label(role: &t::Role) -> &'static str {
    match role {
        t::Role::Standard => "Standard",
        t::Role::Qc => "QC",
        t::Role::Blank => "Blank",
        t::Role::Unknown => "Unknown",
    }
}
fn section(title: &str, open: bool, ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    egui::CollapsingHeader::new(egui::RichText::new(title).strong())
        .default_open(open)
        .show(ui, body);
}
/// Reads a CSV the user picked and applies it with `apply`; the outcome goes to `message`.
fn import_csv(
    message: &mut String,
    what: &str,
    apply: impl FnOnce(&str) -> crate::domain::Result<Vec<String>>,
) -> bool {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("CSV", &["csv", "txt", "tsv"])
        .pick_file()
    else {
        return false;
    };
    match std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|text| apply(&text).map_err(|e| e.to_string()))
    {
        Ok(notes) => {
            *message = if notes.is_empty() {
                format!("{what} imported")
            } else {
                format!("{what} imported. {}", notes.join(" · "))
            };
            true
        }
        Err(e) => {
            *message = format!("{what} not imported: {e}");
            false
        }
    }
}
fn save_csv(message: &mut String, name: &str, text: &str) {
    if let Some(path) = rfd::FileDialog::new().set_file_name(name).save_file() {
        *message = match write_new(&path, text.as_bytes()) {
            Ok(()) => "Template saved".into(),
            Err(e) => e.to_string(),
        };
    }
}
fn signature(request: &t::BatchRequest, results: &[Measurement]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(request).unwrap_or_default().hash(&mut hasher);
    for s in &request.samples {
        for target in &request.targets {
            let name = &target.quantifier.extraction.extraction.name;
            if let Some(m) = results.iter().find(|m| m.source == s.source && &m.analyte == name) {
                (&m.source, &m.analyte, format!("{:?}", m.status)).hash(&mut hasher);
                if let Some(p) = &m.peak {
                    (p.start.to_bits(), p.end.to_bits(), p.area.to_bits()).hash(&mut hasher);
                }
            }
        }
    }
    hasher.finish()
}
/// Identity of a raw file cheap enough to check on every calculation.
fn file_key(path: &str) -> String {
    let meta = std::fs::metadata(path).ok();
    format!(
        "{path}|{}|{:?}",
        meta.as_ref().map_or(0, |m| m.len()),
        meta.and_then(|m| m.modified().ok())
    )
}
fn samples_section(state: &mut UiState, ui: &mut egui::Ui) {
    section("1 · Samples", true, ui, |ui| {
        ui.small("One row per injection, in injection order. Say which are standards, QCs or blanks and which level each standard belongs to.");
        if state.setup.rows.is_empty() {
            ui.label("No samples yet. Select samples in the panel on the left and run the batch in tab 1.");
            return;
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Import sample sheet (CSV)…")
                .on_hover_text("Columns: sample type, file name, sample id, dilution, level")
                .clicked()
            {
                let rows = &mut state.setup.rows;
                import_csv(&mut state.message, "Sample sheet", |text| {
                    crate::sample_sheet::apply(rows, text)
                });
            }
            if ui.button("Save sheet template…").clicked() {
                save_csv(&mut state.message, "sample-sheet.csv", &crate::sample_sheet::template(&state.setup.rows));
            }
        });
        let levels: Vec<String> = state.setup.levels.iter().map(|l| l.name.clone()).collect();
        egui::ScrollArea::horizontal().id_salt("cal_samples_scroll").show(ui, |ui| {
            egui::Grid::new("cal_samples").striped(true).show(ui, |ui| {
                for h in ["Sample type", "File name", "Sample ID", "Dilution", "Level"] {
                    ui.strong(h);
                }
                ui.end_row();
                for (i, row) in state.setup.rows.iter_mut().enumerate() {
                    egui::ComboBox::from_id_salt(("cal_role", i))
                        .selected_text(role_label(&row.role))
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            for role in [t::Role::Standard, t::Role::Qc, t::Role::Blank, t::Role::Unknown] {
                                ui.selectable_value(&mut row.role, role.clone(), role_label(&role));
                            }
                        });
                    ui.label(crate::sample_sheet::stem(&row.source));
                    ui.add(egui::TextEdit::singleline(&mut row.id).desired_width(110.0));
                    ui.add(egui::DragValue::new(&mut row.dilution).speed(0.1).range(0.0001..=1e9));
                    if matches!(row.role, t::Role::Standard | t::Role::Qc) {
                        egui::ComboBox::from_id_salt(("cal_level", i))
                            .selected_text(if row.level.is_empty() { "choose…" } else { row.level.as_str() })
                            .width(90.0)
                            .show_ui(ui, |ui| {
                                for l in &levels {
                                    ui.selectable_value(&mut row.level, l.clone(), l);
                                }
                            });
                    } else {
                        ui.weak("–");
                    }
                    ui.end_row();
                }
            });
        });
    });
}
fn levels_section(state: &mut UiState, ui: &mut egui::Ui) {
    section("2 · Levels — known concentrations", true, ui, |ui| {
        ui.small("A level is one prepared solution. Enter what it contains for every analyte; standards and QCs point to a level in step 1. Internal standards need no concentration.");
        let names: Vec<String> = state.setup.analytes.iter().filter(|a| !a.is_standard).map(|a| a.name.clone()).collect();
        let all = state.setup.analyte_names();
        ui.horizontal_wrapped(|ui| {
            if ui.button("Add level").clicked() {
                let next = (1..).map(|n| n.to_string()).find(|n| !state.setup.levels.iter().any(|l| &l.name == n)).unwrap();
                state.setup.levels.push(crate::sample_sheet::Level { name: next, ..Default::default() });
            }
            if ui.button("Import levels (CSV)…").on_hover_text("Columns: level, then one column per analyte").clicked() {
                let levels = &mut state.setup.levels;
                import_csv(&mut state.message, "Levels", |text| crate::sample_sheet::apply_levels(levels, &all, text));
            }
            if ui.button("Save level template…").clicked() {
                save_csv(&mut state.message, "levels.csv", &crate::sample_sheet::levels_template(&state.setup.levels, &names));
            }
        });
        if state.setup.levels.is_empty() {
            ui.label("No levels yet — add one, or import them from a CSV.");
            return;
        }
        let units: Vec<&'static str> = state.setup.analytes.iter().filter(|a| !a.is_standard).map(|a| a.cfg.unit.label()).collect();
        let mut remove = None;
        egui::ScrollArea::horizontal().id_salt("cal_levels_scroll").show(ui, |ui| {
            egui::Grid::new("cal_levels").striped(true).show(ui, |ui| {
                ui.strong("Level");
                for (name, unit) in names.iter().zip(&units) {
                    ui.strong(format!("{name} ({unit})"));
                }
                ui.end_row();
                for (i, level) in state.setup.levels.iter_mut().enumerate() {
                    ui.add(egui::TextEdit::singleline(&mut level.name).desired_width(70.0));
                    for name in &names {
                        let known = level.conc.get(name).copied();
                        let mut value = known.unwrap_or(0.0);
                        let set = known.is_some();
                        if ui
                            .add(egui::DragValue::new(&mut value).speed(0.1).range(0.0..=1e12).custom_formatter(move |n, _| if set { format!("{n}") } else { "–".into() }))
                            .changed()
                        {
                            level.conc.insert(name.clone(), value);
                        }
                    }
                    if ui.small_button("✕").on_hover_text("Remove this level").clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
        });
        if let Some(i) = remove {
            state.setup.levels.remove(i);
        }
    });
}
fn analytes_section(state: &mut UiState, ui: &mut egui::Ui) {
    section("3 · Analytes and calibration", true, ui, |ui| {
        ui.small("External calibration uses the analyte's peak area. Internal calibration divides it by the area of an internal standard in the same injection. Ion and retention-time settings stay in the method (tab 1).");
        let standards: Vec<String> = state.setup.analytes.iter().filter(|a| a.is_standard).map(|a| a.name.clone()).collect();
        let mut copy_from = None;
        for (i, a) in state.setup.analytes.iter_mut().enumerate() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.strong(&a.name);
                    ui.checkbox(&mut a.is_standard, "Internal standard (measured, not quantified)");
                });
                if a.is_standard {
                    ui.horizontal(|ui| {
                        ui.label("Acceptable peak area");
                        ui.add(egui::DragValue::new(&mut a.area_bounds[0]).speed(1000.0));
                        ui.label("to");
                        ui.add(egui::DragValue::new(&mut a.area_bounds[1]).speed(1000.0));
                        ui.small("peaks outside this range are flagged");
                    });
                    return;
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label("Calibration");
                    let shown = match &a.internal_standard {
                        Some(i) => format!("Internal standard: {i}"),
                        None => "External standard (peak area)".to_owned(),
                    };
                    egui::ComboBox::from_id_salt(("cal_mode", i)).selected_text(shown).show_ui(ui, |ui| {
                        ui.selectable_value(&mut a.internal_standard, None, "External standard (peak area)");
                        for name in &standards {
                            ui.selectable_value(&mut a.internal_standard, Some(name.clone()), format!("Internal standard: {name}"));
                        }
                    });
                    if standards.is_empty() {
                        ui.small("Mark an analyte above as internal standard to offer internal calibration.");
                    }
                });
                super::calibration::fit_controls(ui, ("cal_setup_fit", i), &mut a.cfg);
                ui.horizontal_wrapped(|ui| {
                    ui.label("Unit");
                    egui::ComboBox::from_id_salt(("cal_unit", i)).selected_text(a.cfg.unit.label()).show_ui(ui, |ui| {
                        for unit in [t::Unit::NgMl, t::Unit::UgMl, t::Unit::MgL, t::Unit::NmolL, t::Unit::UmolL] {
                            ui.selectable_value(&mut a.cfg.unit, unit.clone(), unit.label());
                        }
                    });
                    if ui.small_button("Copy these settings to all analytes").clicked() {
                        copy_from = Some(i);
                    }
                });
                ui.collapsing("Limits", |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (label, value, hint) in [
                            ("LOD", &mut a.cfg.lod, "limit of detection (concentration)"),
                            ("LOQ", &mut a.cfg.loq, "limit of quantification; 0 = the lowest standard"),
                            ("Accuracy tolerance %", &mut a.cfg.accuracy_tolerance_percent, "allowed deviation of standards and QCs"),
                            ("Precision limit %", &mut a.cfg.qc_cv_limit_percent, "allowed CV of replicates"),
                            ("Blank response limit", &mut a.cfg.blank_response_limit, "blanks above this response are flagged"),
                        ] {
                            ui.label(label).on_hover_text(hint);
                            ui.add(egui::DragValue::new(value).speed(0.1).range(0.0..=1e12));
                        }
                    });
                });
            });
        }
        if let Some(i) = copy_from {
            let source = state.setup.analytes[i].clone();
            for a in state.setup.analytes.iter_mut().filter(|a| !a.is_standard) {
                a.cfg = t::CalibrationConfig { lod: a.cfg.lod, loq: a.cfg.loq, ..source.cfg.clone() };
                a.internal_standard = source.internal_standard.clone();
            }
        }
    });
}
fn calculate_section(state: &mut UiState, method: &Method, measurements: &[Measurement], ui: &mut egui::Ui) {
    section("4 · Calculate concentrations", true, ui, |ui| {
        let request = state.setup.build_request(method);
        let mut problems = match &request {
            Err(p) => p.clone(),
            Ok(r) => super::calibration_setup::missing_peaks(measurements, r),
        };
        if measurements.is_empty() {
            problems.insert(0, "No peaks measured yet: run the batch in tab 1.".into());
        }
        for p in &problems {
            ui.colored_label(ui.visuals().warn_fg_color, format!("• {p}"));
        }
        let busy = state.receiver.is_some();
        if ui
            .add_enabled(problems.is_empty() && !busy, egui::Button::new(egui::RichText::new("Calculate concentrations").strong()))
            .on_hover_text("Fits the calibration lines and calculates every concentration from the peaks measured in tab 1. Runs instantly; repeat after re-integrating a peak.")
            .clicked()
        {
            if let Ok(request) = request.as_ref() {
                match super::calibration_setup::observations(measurements, request) {
                    Ok(observations) => start_calculation(state, request.clone(), observations, signature(request, measurements)),
                    Err(missing) => state.message = missing.join(" "),
                }
            }
        }
        if busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Calculating…");
            });
        } else if let (Ok(request), Some(done)) = (&request, state.calculated) {
            if done != signature(request, measurements) {
                ui.colored_label(ui.visuals().warn_fg_color, "Peaks or settings changed since the last calculation: press Calculate concentrations to update the results below.");
            }
        }
        if !state.message.is_empty() {
            ui.small(&state.message);
        }
    });
}
fn start_calculation(state: &mut UiState, request: t::BatchRequest, observations: Vec<t::Observation>, signature: u64) {
    let (tx, rx) = mpsc::channel();
    let control = crate::jobs::JobControl::default();
    state.control = Some(control.clone());
    state.receiver = Some(rx);
    state.pending_signature = Some(signature);
    state.draft = serde_json::to_string_pretty(&request).unwrap_or_default();
    state.message = "Calculating from the measured peaks…".into();
    let cache = state.hashes.clone();
    std::thread::spawn(move || {
        let result = (|| {
            let mut hashes = std::collections::BTreeMap::new();
            for s in &request.samples {
                let hash = match cache.get(&file_key(&s.source)) {
                    Some(h) => h.clone(),
                    None => crate::project::source_identity_controlled(std::path::Path::new(&s.source), Some(&control))?.0,
                };
                hashes.insert(s.id.clone(), hash);
            }
            t::evaluate(request, observations, hashes)
        })();
        let _ = tx.send(result);
    });
}
/// JSON and raw-file tools for power users and for exchanging runs.
fn advanced_section(state: &mut UiState, ui: &mut egui::Ui) {
    section("Advanced (JSON and raw-file run)", false, ui, |ui| {
        ui.small("The request below is what step 4 calculates. Running it from the raw files re-extracts every peak and ignores the integrations in tab 1.");
        ui.horizontal_wrapped(|ui| {
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
            if ui.button("Save targeted request…").clicked() { if let Some(path) = rfd::FileDialog::new().set_file_name("targeted-request.json").save_file() { state.message = match write_new(&path, state.draft.as_bytes()) { Ok(()) => "Request saved".into(), Err(e) => e.to_string() }; } }
            if !state.history.is_empty() && ui.button("Save targeted history…").clicked() { if let Some(path) = rfd::FileDialog::new().set_file_name("targeted-history.json").save_file() { state.message = match serde_json::to_vec_pretty(&state.history).map_err(|e| e.to_string()).and_then(|bytes| write_new(&path, &bytes).map_err(|e| e.to_string())) { Ok(()) => "All runs and reviews saved".into(), Err(e) => e }; } }
        });
        super::forms::typed::<t::BatchRequest>(ui,"Additional sample exclusions and target controls",&mut state.draft);
        ui.collapsing("Method / sample JSON editor", |ui| {
            ui.label("Edit degree (1/2/3), intercept (free/zero/fixed), weighting (unweighted/inverse_x/inverse_x2/inverse_y/inverse_y2), curve (polynomial/log_log/point_to_point), range, LOD/LOQ, units and QC thresholds. Internal standards are separate targets with is_area_range, calibration=null; reference their ID in internal_standard. Qualifiers specify extraction and ratio_range. RT windows are in minutes. Exclusions map target ID to a reason. Saved runs retain their original request.");
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
        });
    });
}
pub(super) fn panel(
    state: &mut UiState,
    method: &Method,
    samples: &[(String, String)],
    measurements: &[Measurement],
    ui: &mut egui::Ui,
) {
    poll(state, ui.ctx());
    state.setup.sync(samples, &method.analytes);
    if !state.mode_chosen {
        state.mode_chosen = true;
        if !state.history.is_empty() {
            state.mode = Mode::Browser;
        }
    }
    header(state, method, measurements, ui);
    if state.mode == Mode::Setup || state.history.is_empty() {
        egui::ScrollArea::vertical()
            .id_salt("targeted_setup_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(next_step(state, method, measurements));
                ui.add_space(4.0);
                samples_section(state, ui);
                levels_section(state, ui);
                analytes_section(state, ui);
                calculate_section(state, method, measurements, ui);
                advanced_section(state, ui);
            });
        return;
    }
    let outcome = browser::show(state, ui);
    if let Some(next) = outcome.applied {
        state.setup.absorb(&next);
        match state.retain(next) {
            Ok(()) => {
                state.selected = 0;
                state.message = "New run created with the changed calibration".into();
            }
            Err(e) => state.message = e,
        }
    }
    if let Some(accepted) = outcome.review {
        let current = state.current_batch;
        if let Some(batch) = state.history.get(current) {
            if let Some(row) = batch.results.get(state.selected) {
                match t::review(batch, batch.reviews.len(), &row.sample, &row.target, accepted, "gui", &state.reason) {
                    Ok(next) => state.history[current] = next,
                    Err(e) => state.message = e.to_string(),
                }
            }
        }
    }
}
/// Mode switch, run selector and recalculation, shown above both views.
fn header(state: &mut UiState, method: &Method, measurements: &[Measurement], ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        super::widgets::nav_value(ui, &mut state.mode, Mode::Setup, "Setup");
        ui.add_enabled_ui(!state.history.is_empty(), |ui| {
            super::widgets::nav_value(ui, &mut state.mode, Mode::Browser, "Results browser");
        });
        if state.mode == Mode::Browser && !state.history.is_empty() {
            egui::ComboBox::from_id_salt("targeted_history_run")
                .selected_text(format!("Run {} of {}", state.current_batch + 1, state.history.len()))
                .show_ui(ui, |ui| {
                    for (index, batch) in state.history.iter().enumerate() {
                        if ui
                            .selectable_value(&mut state.current_batch, index, format!("Run {}: {} ({})", index + 1, batch.request.name, batch.batch_id))
                            .changed()
                        {
                            state.selected = 0;
                        }
                    }
                });
            let ready = state.setup.build_request(method).is_ok_and(|r| {
                !measurements.is_empty() && super::calibration_setup::missing_peaks(measurements, &r).is_empty()
            });
            if ui
                .add_enabled(ready && state.receiver.is_none(), egui::Button::new("Recalculate"))
                .on_hover_text("Calculate again from the peaks in tab 1 with the current setup")
                .clicked()
            {
                if let Ok(request) = state.setup.build_request(method) {
                    match super::calibration_setup::observations(measurements, &request) {
                        Ok(observations) => {
                            let sig = signature(&request, measurements);
                            start_calculation(state, request, observations, sig)
                        }
                        Err(missing) => state.message = missing.join(" "),
                    }
                }
            }
            if let (Ok(request), Some(done)) = (state.setup.build_request(method), state.calculated) {
                if done != signature(&request, measurements) && state.receiver.is_none() {
                    ui.colored_label(ui.visuals().warn_fg_color, "Peaks or settings changed since this calculation — press Recalculate.");
                }
            }
        }
        if state.receiver.is_some() {
            ui.spinner();
        }
    });
    ui.separator();
}
fn next_step(state: &UiState, method: &Method, measurements: &[Measurement]) -> String {
    if measurements.is_empty() {
        "Start in tab 1: select samples, run the batch and review the peaks. Then come back here.".into()
    } else if let Err(p) = state.setup.build_request(method) {
        format!("Next: fill in steps 1–3 ({} thing(s) still missing, listed in step 4).", p.len())
    } else if state.history.is_empty() || state.calculated.is_none() {
        "Next: press Calculate concentrations (step 4).".into()
    } else {
        "Check each calibration line, then look for flagged samples in the results and revisit those peaks.".into()
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
                        for sample in &batch.request.samples {
                            if let Some(hash) = batch.source_hashes.get(&sample.id) {
                                state.hashes.insert(file_key(&sample.source), hash.clone());
                            }
                        }
                        if let Some(signature) = state.pending_signature.take() {
                            state.calculated = Some(signature);
                        }
                        state.history.push(batch);
                        state.current_batch = state.history.len() - 1;
                        state.selected = 0;
                        state.mode = Mode::Browser;
                        state.mode_chosen = true;
                        state.message =
                            "Run retained. Inspect flags, review concentrations, and save history."
                                .into();
                    }
                    Err(e) => {
                        state.pending_signature = None;
                        state.message = e.to_string()
                    }
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
                    .show(ctx, |ui| panel(state, &Method::default(), &[], &[], ui));
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
    fn the_results_browser_renders_wide_and_narrow_and_follows_the_analyte_list() {
        let temp = tempfile::tempdir().unwrap();
        let batch = t::run(raw_request(temp.path()), &crate::jobs::JobControl::default()).unwrap();
        let mut state = UiState::default();
        state.history.push(batch);
        state.mode = Mode::Browser;
        state.mode_chosen = true;
        let ctx = egui::Context::default();
        for width in [700.0, 1500.0] {
            for _ in 0..2 {
                ctx.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 900.0))),
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default().show(ctx, |ui| panel(&mut state, &Method::default(), &[], &[], ui));
                    },
                );
            }
        }
        let calibrated = state.history[0].request.targets.iter().find(|t| t.calibration.is_some()).unwrap().id.clone();
        assert_eq!(state.browse_target, calibrated);
        assert_eq!(state.history[0].results[state.selected].target, calibrated);
    }
    #[test]
    fn every_setup_section_renders_with_samples_levels_and_two_analytes() {
        let analyte = |name: &str| {
            let mut a = crate::quant::new_analyte(1);
            a.extraction.name = name.into();
            a
        };
        let method = Method {
            analytes: vec![analyte("Analyte"), analyte("IS")],
            ..Default::default()
        };
        let samples: Vec<(String, String)> = (1..=4)
            .map(|n| (format!("C:/d/s{n}.mzML"), format!("s{n}")))
            .collect();
        let mut state = UiState::default();
        let ctx = egui::Context::default();
        let mut run = |state: &mut UiState| {
            ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default()
                    .show(ctx, |ui| panel(state, &method, &samples, &[], ui));
            })
        };
        run(&mut state);
        assert_eq!(state.setup.rows.len(), 4);
        state.setup.analytes[1].is_standard = true;
        state.setup.analytes[0].internal_standard = Some("IS".into());
        state.setup.levels.push(crate::sample_sheet::Level::default());
        run(&mut state);
        run(&mut state);
        assert!(state.setup.analytes[0].internal_standard.is_some());
    }
    #[test]
    fn peaks_from_tab_one_give_the_raw_run_result_and_follow_manual_integration() {
        let temp = tempfile::tempdir().unwrap();
        let mut request = raw_request(temp.path());
        for target in &mut request.targets {
            target.qualifiers.clear();
        }
        let raw = t::run(request.clone(), &crate::jobs::JobControl::default()).unwrap();
        let mut results: Vec<Measurement> =
            raw.observations.iter().map(|o| o.quantifier.clone()).collect();
        let observations =
            super::super::calibration_setup::observations(&results, &request).unwrap();
        let same = t::evaluate(request.clone(), observations, raw.source_hashes.clone()).unwrap();
        assert_eq!(same.results, raw.results);
        // A hand-adjusted peak is used as it is.
        let target = request.targets.iter().find(|t| t.calibration.is_some()).unwrap();
        let name = &target.quantifier.extraction.extraction.name;
        let method = Method {
            analytes: vec![target.quantifier.extraction.clone()],
            detection_smoothing: request.detection_smoothing,
            minimum_height: request.minimum_height,
            boundary_fraction: request.boundary_fraction,
            ..Default::default()
        };
        let m = results.iter_mut().find(|m| &m.analyte == name).unwrap();
        let old = m.peak.clone().unwrap();
        let width = old.end - old.start;
        m.peak = Some(
            crate::quant::measure_for(
                &m.trace,
                old.start + 0.2 * width,
                old.end - 0.2 * width,
                &method.analytes[0],
                &method,
            )
            .unwrap(),
        );
        m.status = crate::quant::Status::Manual;
        let observations =
            super::super::calibration_setup::observations(&results, &request).unwrap();
        let edited = t::evaluate(request.clone(), observations, raw.source_hashes.clone()).unwrap();
        assert_ne!(edited.results, raw.results);
        // Missing peaks are reported instead of guessed.
        results.pop();
        assert!(super::super::calibration_setup::observations(&results, &request).is_err());
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
            "Advanced (JSON and raw-file run)",
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
