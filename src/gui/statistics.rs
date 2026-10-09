//! Statistical snapshots and interactive evidence selection.
use crate::{
    domain::{Operation, Request},
    engine::{Output, Response},
    jobs::JobControl,
    statistics::{Report, Settings, Table},
};
use eframe::egui;
use egui_plot::{Line, Points};
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(super) struct State {
    pub open: bool,
    pub draft: String,
    settings: String,
    history: Vec<Response>,
    selected: usize,
    sample: usize,
    feature: usize,
    message: String,
    #[serde(skip)]
    pending: Option<(
        usize,
        std::sync::mpsc::Receiver<crate::domain::Result<Response>>,
    )>,
    #[serde(skip)]
    control: JobControl,
}
fn save(bytes: &[u8]) -> std::result::Result<(), String> {
    use std::io::Write;
    if let Some(path) = rfd::FileDialog::new().save_file() {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .and_then(|mut f| f.write_all(bytes))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub(super) fn show(app: &mut super::MzViewerApp, ctx: &egui::Context) {
    let linked_matrix = if app.statistics.open {
        app.untargeted.latest_matrix()
    } else {
        None
    };
    let linked_targeted = if app.statistics.open {
        app.quant.latest_targeted()
    } else {
        None
    };
    let s = &mut app.statistics;
    if s.settings.is_empty() {
        s.settings = serde_json::to_string_pretty(&Settings::default()).unwrap();
    }
    if let Some((origin, rx)) = &s.pending {
        match rx.try_recv() {
            Ok(result) => {
                let origin = *origin;
                s.pending = None;
                match result {
                    Ok(r) => {
                        s.history.push(r);
                        if s.selected == origin {
                            s.selected = s.history.len() - 1;
                            s.sample = 0;
                            s.feature = 0;
                        }
                        s.message="Immutable analysis retained; select earlier revisions to undo preprocessing".into();
                    }
                    Err(e) => s.message = e.to_string(),
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                s.pending = None;
                s.message = "Statistics worker disconnected".into();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100))
            }
        }
    }
    let mut open = s.open;
    super::workbench::analytical_panel(ctx, "Statistical analysis workspace", &mut open, |ui| {
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    linked_matrix.is_some(),
                    egui::Button::new("Use current feature matrix"),
                )
                .clicked()
            {
                match crate::statistics::from_matrix(linked_matrix.as_ref().unwrap()) {
                    Ok(table) => s.draft = serde_json::to_string_pretty(&table).unwrap(),
                    Err(error) => s.message = error.to_string(),
                }
            }
            if ui
                .add_enabled(
                    linked_targeted.is_some(),
                    egui::Button::new("Use current targeted concentrations"),
                )
                .clicked()
            {
                match crate::statistics::from_targeted(linked_targeted.as_ref().unwrap()) {
                    Ok(table) => s.draft = serde_json::to_string_pretty(&table).unwrap(),
                    Err(error) => s.message = error.to_string(),
                }
            }
        });
        ui.label("Original quantities are retained. Null means unavailable. Group tests assume independent samples; imputation can bias inference. No automatic sample exclusions.");
        if ui
            .button("Load quantitative table / feature matrix / statistics response JSON")
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new().pick_file() {
                let result = (|| {
                    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                    let v: serde_json::Value =
                        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                    if let Ok(t) = serde_json::from_value::<Table>(v.clone()) {
                        s.draft = serde_json::to_string_pretty(&t).unwrap();
                        return Ok(());
                    }
                    let r: Response = serde_json::from_value(v.get("result").cloned().unwrap_or(v))
                        .map_err(|e| e.to_string())?;
                    match &r.output {
                        Output::FeatureMatrix { report } => {
                            crate::untargeted::verify_response(&r).map_err(|e| e.to_string())?;
                            s.draft = serde_json::to_string_pretty(
                                &crate::statistics::from_matrix(report)
                                    .map_err(|e| e.to_string())?,
                            )
                            .unwrap();
                        }
                        Output::TargetedQuantification { batch } => {
                            crate::targeted::verify(batch).map_err(|e| e.to_string())?;
                            s.draft = serde_json::to_string_pretty(
                                &crate::statistics::from_targeted(batch)
                                    .map_err(|e| e.to_string())?,
                            )
                            .unwrap();
                        }
                        Output::Statistics { report } => {
                            crate::statistics::verify_response(&r).map_err(|e| e.to_string())?;
                            s.draft = serde_json::to_string_pretty(&report.table).unwrap();
                            s.settings = serde_json::to_string_pretty(&report.settings).unwrap();
                            s.history.push(r);
                            s.selected = s.history.len() - 1;
                        }
                        _ => return Err(
                            "Select a quantitative table, feature matrix or statistics response"
                                .into(),
                        ),
                    }
                    Ok::<_, String>(())
                })();
                s.message = result
                    .err()
                    .unwrap_or_else(|| "Input loaded; edit metadata and explicit settings".into());
            }
        }
        step_navigator(ctx, ui, s);
        super::forms::typed::<Table>(
            ui,
            "Advanced sample design and quantity editor",
            &mut s.draft,
        );
        super::forms::typed::<Settings>(
            ui,
            "Advanced filters, exclusions and comparison groups",
            &mut s.settings,
        );
        ui.collapsing(
            "Advanced quantitative table JSON — expert exact-table audit",
            |ui| {
                ui.small("Routine loading happens in step 1 above. This text is retained so the exact frozen table can be audited; null means unavailable, never zero.");
                ui.add(
                    egui::TextEdit::multiline(&mut s.draft)
                        .code_editor()
                        .desired_rows(10)
                        .desired_width(f32::INFINITY),
                );
            },
        );
        ui.collapsing(
            "Advanced settings JSON — expert exact-parameter audit",
            |ui| {
                ui.small("Routine settings live in steps 2–4 above. This text is retained for exact-parameter review and reproducibility.");
                ui.add(
                    egui::TextEdit::multiline(&mut s.settings)
                        .code_editor()
                        .desired_rows(15)
                        .desired_width(f32::INFINITY),
                );
            },
        );
        if onboarding_step(ctx) == 4 {
            ui.strong("Step 5 — Run the analysis and inspect the results");
            let readiness = precheck(s);
            for issue in &readiness {
                ui.colored_label(ui.visuals().error_fg_color, format!("Blocked — {issue}"));
            }
            if readiness.is_empty() {
                if let (Ok(table), Ok(settings)) = (parsed_table(s), parsed_settings(s)) {
                    let (reference, comparison, _) = group_counts(&table, &settings);
                    ui.small(format!(
                    "Ready to run: {} samples · {} features · groups {reference} vs {comparison} · missing “{}” · {} transform · {} correction",
                    table.samples.len(),
                    table.features.len(),
                    settings.missing,
                    settings.transform,
                    settings.fdr
                ));
                }
            }
            if ui
                .add_enabled(
                    s.pending.is_none() && readiness.is_empty(),
                    egui::Button::new(
                        egui::RichText::new("Analyze and retain new revision").strong(),
                    )
                    .fill(ui.visuals().selection.bg_fill),
                )
                .on_hover_text(if readiness.is_empty() {
                    "Run the combined PCA, clustering, and group-comparison analysis"
                } else {
                    "Resolve the blocked item(s) above — see steps 1–4"
                })
                .clicked()
            {
                let input = serde_json::from_str::<Table>(&s.draft).and_then(|t| {
                    serde_json::from_str::<Settings>(&s.settings).map(|cfg| (t, cfg))
                });
                match input {
                    Ok((t, cfg)) => {
                        let (tx, rx) = std::sync::mpsc::channel();
                        s.pending = Some((s.selected, rx));
                        s.control = JobControl::default();
                        let control = s.control.clone();
                        std::thread::spawn(move || {
                            let request = Request {
                                version: 1,
                                operation_id: Default::default(),
                                actor: "desktop".into(),
                                operation: Operation::AnalyzeStatistics {
                                    table: Box::new(t),
                                    settings: cfg,
                                },
                            };
                            let _ = tx.send(crate::engine::execute(
                                std::path::Path::new("-"),
                                request,
                                &control,
                            ));
                        });
                    }
                    Err(e) => s.message = e.to_string(),
                }
            }
            if s.pending.is_some() && ui.button("Cancel statistics").clicked() {
                s.control.cancel();
            }
            ui.label(&s.message);
            egui::ComboBox::from_label("Retained analysis revision")
                .selected_text(format!("{}", s.selected + 1))
                .show_ui(ui, |ui| {
                    for (i, r) in s.history.iter().enumerate() {
                        ui.selectable_value(&mut s.selected, i, r.result_id.0.to_string());
                    }
                });
            if let Some(response) = s.history.get(s.selected) {
                if let Output::Statistics { report } = &response.output {
                    ui.horizontal(|ui| {
                        if ui
                            .button("Restore this revision's settings and original table")
                            .clicked()
                        {
                            s.draft = serde_json::to_string_pretty(&report.table).unwrap();
                            s.settings = serde_json::to_string_pretty(&report.settings).unwrap();
                        }
                        if ui.button("Save full reproducible JSON").clicked() {
                            s.message = save(&serde_json::to_vec_pretty(response).unwrap())
                                .err()
                                .unwrap_or_else(|| "Saved".into());
                        }
                        if ui.button("Export statistical CSV").clicked() {
                            s.message = crate::statistics::export_csv(report)
                                .map_err(|e| e.to_string())
                                .and_then(|csv| save(csv.as_bytes()))
                                .err()
                                .unwrap_or_else(|| "Saved".into());
                        }
                    });
                    plots(ui, report, &mut s.sample, &mut s.feature);
                }
            }
        }
    });
    s.open = open;
}
fn onboarding_step(ctx: &egui::Context) -> usize {
    ctx.data(|d| d.get_temp::<usize>(egui::Id::new("statistics_onboarding_step")))
        .unwrap_or(0)
}

/// Five-step guided onboarding: load → groups → analysis → review → run.
/// Power controls stay one level down in Advanced; JSON editors are the
/// intentional expert capability for exact-table and exact-parameter audit.
fn step_navigator(ctx: &egui::Context, ui: &mut egui::Ui, s: &mut State) {
    let mut step = onboarding_step(ctx);
    ui.horizontal_wrapped(|ui| {
        for (index, label) in [
            "1 · Load table",
            "2 · Sample groups",
            "3 · Analysis",
            "4 · Review",
            "5 · Run and inspect",
        ]
        .iter()
        .enumerate()
        {
            ui.selectable_value(&mut step, index, *label);
        }
    });
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("statistics_onboarding_step"), step));
    match step {
        0 => step_load(ui, s),
        1 => step_groups(ui, s),
        2 => step_analysis(ui, s),
        3 => step_review(ui, s),
        _ => {
            ui.small("Run the analysis below and inspect the retained revision. Restore any earlier revision to undo preprocessing.");
        }
    }
    ui.separator();
    if step < 4 {
        ui.horizontal_wrapped(|ui| {
            if step > 0 && ui.small_button("← Back").clicked() {
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::new("statistics_onboarding_step"), step - 1)
                });
            }
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("Continue →").strong())
                        .fill(ui.visuals().selection.bg_fill),
                )
                .clicked()
            {
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::new("statistics_onboarding_step"), step + 1)
                });
            }
        });
        ui.separator();
    }
}

fn parsed_table(s: &State) -> Result<Table, String> {
    serde_json::from_str(&s.draft).map_err(|e| e.to_string())
}

fn parsed_settings(s: &State) -> Result<Settings, String> {
    serde_json::from_str(&s.settings).map_err(|e| e.to_string())
}

/// Step 1: what is loaded, in one glance.
fn step_load(ui: &mut egui::Ui, s: &mut State) {
    ui.strong("Step 1 — Load or select the feature table");
    ui.small("Use the buttons above (feature matrix, targeted concentrations, or a saved table). Original quantities are retained; null means unavailable, never zero.");
    match parsed_table(s) {
        Ok(table) => {
            let missing = table
                .values
                .iter()
                .flatten()
                .filter(|v| v.is_none())
                .count();
            let total = table.values.iter().map(Vec::len).sum::<usize>();
            ui.label(format!(
                "{} samples · {} features · {} of {} values missing ({:.1}%)",
                table.samples.len(),
                table.features.len(),
                missing,
                total,
                if total > 0 {
                    missing as f64 / total as f64 * 100.0
                } else {
                    0.0
                }
            ));
            let keys: std::collections::BTreeSet<_> = table
                .samples
                .iter()
                .flat_map(|x| x.metadata.keys().cloned())
                .collect();
            if keys.is_empty() {
                ui.small("No sample metadata yet — step 2 will ask for group columns. Add metadata through the advanced table editor below.");
            } else {
                ui.small(format!(
                    "Sample metadata columns: {}",
                    keys.into_iter().collect::<Vec<_>>().join(", ")
                ));
            }
            if table.values.len() != table.samples.len() {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    "Table rows do not match the sample list — reload the table.",
                );
            }
        }
        Err(error) => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("No valid feature table yet: {error}"),
            );
        }
    }
}

/// Issues with the two-group definition, in understandable language.
fn group_issues(table: &Table, settings: &Settings) -> Vec<String> {
    let mut issues = Vec::new();
    let Some(groups) = &settings.groups else {
        return issues;
    };
    if groups.metadata_key.trim().is_empty() {
        issues.push("Choose the metadata column that defines the two groups.".into());
    }
    if groups.reference.trim().is_empty() || groups.comparison.trim().is_empty() {
        issues.push("Name both the reference and the comparison group.".into());
    }
    if !groups.reference.trim().is_empty() && groups.reference == groups.comparison {
        issues.push("Reference and comparison must be different groups.".into());
    }
    if issues.is_empty() {
        let (reference, comparison, unassigned) = group_counts(table, settings);
        if reference == 0 || comparison == 0 {
            issues.push(format!(
                "Each group must match at least one included sample (now {reference} vs {comparison}, {unassigned} unassigned)."
            ));
        }
        if reference < 2 || comparison < 2 {
            issues.push(format!(
                "Small groups ({reference} vs {comparison}): comparisons report unavailable statistics rather than guessing — add independent replicates for inference."
            ));
        }
    }
    issues
}

/// Included samples per group: `(reference, comparison, unassigned)`.
fn group_counts(table: &Table, settings: &Settings) -> (usize, usize, usize) {
    let Some(groups) = &settings.groups else {
        return (0, 0, 0);
    };
    let mut counts = (0, 0, 0);
    for sample in &table.samples {
        if settings.excluded_samples.contains_key(&sample.id) {
            continue;
        }
        match sample
            .metadata
            .get(&groups.metadata_key)
            .map(String::as_str)
        {
            Some(value) if value == groups.reference => counts.0 += 1,
            Some(value) if value == groups.comparison => counts.1 += 1,
            _ => counts.2 += 1,
        }
    }
    counts
}

/// Step 2: define groups and review every sample assignment.
fn step_groups(ui: &mut egui::Ui, s: &mut State) {
    ui.strong("Step 2 — Define sample groups and review assignments");
    ui.small("Group tests assume independent samples. Imputation can bias inference, and no sample is ever excluded automatically.");
    let mut settings = match parsed_settings(s) {
        Ok(settings) => settings,
        Err(error) => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Fix the analysis settings first: {error}"),
            );
            return;
        }
    };
    let table = match parsed_table(s) {
        Ok(table) => table,
        Err(_) => {
            ui.small("Load a feature table in step 1 first.");
            return;
        }
    };
    let original = serde_json::to_string(&settings).unwrap();
    let mut compare = settings.groups.is_some();
    ui.checkbox(&mut compare, "Compare two independent groups");
    if compare {
        let groups = settings
            .groups
            .get_or_insert_with(|| crate::statistics::Groups {
                metadata_key: "group".into(),
                reference: String::new(),
                comparison: String::new(),
            });
        let keys: std::collections::BTreeSet<_> = table
            .samples
            .iter()
            .flat_map(|x| x.metadata.keys().cloned())
            .collect();
        ui.horizontal_wrapped(|ui| {
            ui.label("Metadata column");
            egui::ComboBox::from_id_salt("stats_group_key")
                .selected_text(groups.metadata_key.as_str())
                .show_ui(ui, |ui| {
                    for key in &keys {
                        ui.selectable_value(&mut groups.metadata_key, key.clone(), key);
                    }
                });
            ui.label("Reference");
            ui.text_edit_singleline(&mut groups.reference);
            ui.label("Comparison");
            ui.text_edit_singleline(&mut groups.comparison);
        });
        if keys.is_empty() {
            ui.small("The loaded table has no metadata columns — add them in the advanced table editor below, then return here.");
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Confidence level");
            ui.add(
                egui::DragValue::new(&mut settings.confidence)
                    .range(0.001..=0.999)
                    .speed(0.001),
            );
        });
    } else {
        settings.groups = None;
    }
    if serde_json::to_string(&settings).unwrap() != original {
        s.settings = serde_json::to_string_pretty(&settings).unwrap();
    }
    // Assignment review.
    if let Some(groups) = &settings.groups {
        let (reference, comparison, unassigned) = group_counts(&table, &settings);
        ui.label(format!(
            "Reference “{}” ({reference}) vs comparison “{}” ({comparison}) · {unassigned} unassigned or excluded",
            groups.reference, groups.comparison
        ));
        egui::Grid::new("stats_assignments")
            .num_columns(3)
            .striped(true)
            .show(ui, |ui| {
                ui.label("Sample");
                ui.label(groups.metadata_key.as_str());
                ui.label("Assignment");
                ui.end_row();
                for sample in &table.samples {
                    ui.label(sample.id.as_str());
                    ui.label(
                        sample
                            .metadata
                            .get(&groups.metadata_key)
                            .map(String::as_str)
                            .unwrap_or("—"),
                    );
                    if let Some(reason) = settings.excluded_samples.get(&sample.id) {
                        ui.small(format!("Excluded: {reason}"));
                    } else {
                        match sample
                            .metadata
                            .get(&groups.metadata_key)
                            .map(String::as_str)
                        {
                            Some(value) if value == groups.reference => {
                                ui.small("Reference");
                            }
                            Some(value) if value == groups.comparison => {
                                ui.small("Comparison");
                            }
                            _ => {
                                ui.small("Unassigned — ignored by the comparison");
                            }
                        }
                    }
                    ui.end_row();
                }
            });
    } else {
        ui.small(
            "No group comparison — the analysis still reports the PCA overview and clustering.",
        );
    }
    for issue in group_issues(&table, &settings) {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!(" Needs attention — {issue}"),
        );
    }
}

/// Step 3: the one genuinely implemented analysis plus routine parameters.
fn step_analysis(ui: &mut egui::Ui, s: &mut State) {
    ui.strong("Step 3 — Select the analysis and configure routine parameters");
    ui.small("This workspace runs one combined, replayable analysis: a PCA overview, hierarchical clustering, and — when two groups are defined in step 2 — Welch group comparisons. No other statistical methods are implemented here.");
    python_status(ui);
    ui.small("Preprocessing order: exclusions → missingness filter → imputation → normalization → transformation → scaling. Welch tests use the pre-scaling values.");
    settings_controls_open(ui, &mut s.settings);
}

/// Compatibility wrapper: the existing collapsed power controls, opened.
fn settings_controls_open(ui: &mut egui::Ui, text: &mut String) {
    egui::CollapsingHeader::new("Routine preprocessing and comparison controls")
        .id_salt("stats_routine_controls")
        .default_open(true)
        .show(ui, |ui| {
            settings_fields(ui, text);
        });
    ui.collapsing("Advanced preprocessing controls", |ui| {
        ui.small("The same settings as above, exposed for exact review. Routine work never requires this section.");
        settings_controls(ui, text);
    });
}

/// Step 4: everything visible before execution.
fn step_review(ui: &mut egui::Ui, s: &mut State) {
    ui.strong("Step 4 — Review assumptions, handling, and corrections");
    let table = match parsed_table(s) {
        Ok(table) => table,
        Err(error) => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Load a valid feature table first: {error}"),
            );
            return;
        }
    };
    let settings = match parsed_settings(s) {
        Ok(settings) => settings,
        Err(error) => {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("Fix the analysis settings first: {error}"),
            );
            return;
        }
    };
    ui.label(format!(
        "Dataset: {} samples · {} features",
        table.samples.len(),
        table.features.len()
    ));
    if let Some(groups) = &settings.groups {
        let (reference, comparison, unassigned) = group_counts(&table, &settings);
        ui.label(format!(
            "Groups (column “{}”): “{}” {reference} vs “{}” {comparison} · {unassigned} unassigned or excluded",
            groups.metadata_key, groups.reference, groups.comparison
        ));
    } else {
        ui.label("Groups: none — overview only, no group comparisons.");
    }
    ui.label(format!(
        "Missing values: “{}” (at most {:.0}% missing per feature) · normalization “{}”{} · transformation “{}” · scaling “{}”",
        settings.missing,
        settings.max_missing_fraction * 100.0,
        settings.normalization,
        settings
            .internal_standard
            .as_ref()
            .map(|id| format!(" on {id}"))
            .unwrap_or_default(),
        settings.transform,
        settings.scaling
    ));
    ui.small("Assumptions: samples are independent; imputed values can bias inference; Welch tests run on pre-scaling values; no sample is excluded without your explicit reason below.");
    ui.label(format!(
        "Multiple-testing correction: {} ({})",
        settings.fdr,
        if settings.fdr == "by" {
            "Benjamini–Yekutieli, conservative under arbitrary dependence"
        } else {
            "Benjamini–Hochberg, assumes independence or suitable positive dependence"
        }
    ));
    exclusions_editor(ui, s, &table);
    // Pre-execution validation with the engine's own checks.
    let mut issues = group_issues(&table, &settings);
    if let Err(error) = crate::statistics::validate(&table, &settings) {
        issues.push(error.to_string());
    }
    if issues.is_empty() {
        ui.colored_label(
            super::plot_controls::scientific_color(ui.visuals().dark_mode, 3),
            "Ready — the configuration passes all checks. Continue to step 5 to run.",
        );
    } else {
        for issue in issues {
            ui.colored_label(ui.visuals().error_fg_color, format!("Blocked — {issue}"));
        }
    }
}

/// Explicit sample/feature exclusions with mandatory reasons.
fn exclusions_editor(ui: &mut egui::Ui, s: &mut State, table: &Table) {
    ui.strong("Exclusions (explicit, with reasons)");
    let mut settings = match parsed_settings(s) {
        Ok(settings) => settings,
        Err(_) => return,
    };
    let original = serde_json::to_string(&settings).unwrap();
    let mut remove_sample = None;
    for (id, reason) in settings.excluded_samples.iter_mut() {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Sample {id}"));
            ui.text_edit_singleline(reason);
            if ui.small_button("Include again").clicked() {
                remove_sample = Some(id.clone());
            }
        });
    }
    if let Some(id) = remove_sample {
        settings.excluded_samples.remove(&id);
    }
    let mut remove_feature = None;
    for (id, reason) in settings.excluded_features.iter_mut() {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Feature {id}"));
            ui.text_edit_singleline(reason);
            if ui.small_button("Include again").clicked() {
                remove_feature = Some(id.clone());
            }
        });
    }
    if let Some(id) = remove_feature {
        settings.excluded_features.remove(&id);
    }
    // Add-sample row with a mandatory reason.
    let reason_id = egui::Id::new("stats_exclusion_reason");
    let sample_id = egui::Id::new("statistics_exclusion_sample");
    let mut reason = ui
        .ctx()
        .data(|d| d.get_temp::<String>(reason_id))
        .unwrap_or_default();
    let mut chosen = ui
        .ctx()
        .data(|d| d.get_temp::<usize>(sample_id))
        .unwrap_or(0);
    let candidates: Vec<_> = table
        .samples
        .iter()
        .filter(|x| !settings.excluded_samples.contains_key(&x.id))
        .collect();
    ui.horizontal_wrapped(|ui| {
        if candidates.is_empty() {
            ui.small("Every sample already has an exclusion entry.");
        } else {
            if chosen >= candidates.len() {
                chosen = 0;
            }
            egui::ComboBox::from_id_salt("stats_exclude_sample")
                .selected_text(candidates[chosen].id.as_str())
                .show_ui(ui, |ui| {
                    for (index, sample) in candidates.iter().enumerate() {
                        ui.selectable_value(&mut chosen, index, sample.id.as_str());
                    }
                });
            ui.label("Reason");
            ui.text_edit_singleline(&mut reason);
            if ui
                .add_enabled(
                    !reason.trim().is_empty(),
                    egui::Button::new("Exclude sample"),
                )
                .on_hover_text("Exclusions require a reason; they are never inferred")
                .clicked()
            {
                settings
                    .excluded_samples
                    .insert(candidates[chosen].id.clone(), reason.trim().to_owned());
                reason.clear();
            }
        }
    });
    ui.ctx().data_mut(|d| {
        d.insert_temp(reason_id, reason);
        d.insert_temp(sample_id, chosen);
    });
    if serde_json::to_string(&settings).unwrap() != original {
        s.settings = serde_json::to_string_pretty(&settings).unwrap();
    }
}

/// Shared pre-execution checks: parsing, group assignment, engine validation.
fn precheck(s: &State) -> Vec<String> {
    let table = match parsed_table(s) {
        Ok(table) => table,
        Err(error) => return vec![format!("Load a valid feature table first: {error}")],
    };
    let settings = match parsed_settings(s) {
        Ok(settings) => settings,
        Err(error) => return vec![format!("Fix the analysis settings first: {error}")],
    };
    let mut issues = group_issues(&table, &settings);
    if let Err(error) = crate::statistics::validate(&table, &settings) {
        issues.push(error.to_string());
    }
    issues
}

/// Cached local-Python availability; the engine needs NumPy and SciPy.
fn python_status(ui: &mut egui::Ui) {
    let id = egui::Id::new("statistics_python_status");
    let status: Option<String> = ui.ctx().data(|d| d.get_temp(id));
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Check Python environment")
            .on_hover_text("Statistics run in a local Python with NumPy and SciPy")
            .clicked()
        {
            ui.ctx().data_mut(|d| {
                d.insert_temp(
                    id,
                    "Checking for local Python with NumPy and SciPy…".to_owned(),
                )
            });
            let ctx = ui.ctx().clone();
            std::thread::spawn(move || {
                ctx.data_mut(|d| d.insert_temp(id, check_python()));
                ctx.request_repaint();
            });
        }
        ui.small(
            status
                .as_deref()
                .unwrap_or("Statistics run in local Python with NumPy and SciPy — check availability before a long run."),
        );
    });
}

fn check_python() -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    let python: std::path::PathBuf = std::env::var_os("CHROMASCOPE_STATS_PYTHON")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "python".into());
    std::thread::spawn(move || {
        let result = std::process::Command::new(&python)
            .args([
                "-E",
                "-P",
                "-c",
                "import numpy, scipy; print(numpy.__version__, scipy.__version__)",
            ])
            .output();
        let _ = tx.send(result);
    });
    match rx.recv_timeout(std::time::Duration::from_secs(20)) {
        Ok(Ok(output)) if output.status.success() => format!(
            "Python ready — NumPy/SciPy {}",
            String::from_utf8_lossy(&output.stdout).trim()
        ),
        Ok(Ok(output)) => format!(
            "Python found, but NumPy/SciPy failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(300)
                .collect::<String>()
        ),
        Ok(Err(error)) => format!(
            "No usable Python ({error}). Set CHROMASCOPE_STATS_PYTHON or install Python with NumPy and SciPy."
        ),
        Err(_) => "Python check timed out after 20 s.".into(),
    }
}
fn settings_controls(ui: &mut egui::Ui, text: &mut String) {
    ui.collapsing("Preprocessing and comparison controls", |ui| {
        settings_fields(ui, text);
    });
}
fn settings_fields(ui: &mut egui::Ui, text: &mut String) {
    let Ok(mut cfg) = serde_json::from_str::<Settings>(text) else {
        ui.label("Correct the settings JSON to use these controls");
        return;
    };
    let original = serde_json::to_string(&cfg).unwrap();
    for (label, value, choices) in [
        (
            "Missing values",
            &mut cfg.missing,
            &["reject", "median", "half_minimum", "complete_features"][..],
        ),
        (
            "Normalization",
            &mut cfg.normalization,
            &["none", "total", "median", "internal_standard"][..],
        ),
        (
            "Transformation",
            &mut cfg.transform,
            &["none", "log2", "log10", "sqrt"][..],
        ),
        (
            "Scaling",
            &mut cfg.scaling,
            &["none", "center", "autoscale", "pareto"][..],
        ),
        (
            "Euclidean linkage",
            &mut cfg.linkage,
            &["average", "complete", "single", "ward"][..],
        ),
        ("FDR adjustment", &mut cfg.fdr, &["bh", "by"][..]),
    ] {
        egui::ComboBox::from_label(label)
            .selected_text(value.as_str())
            .show_ui(ui, |ui| {
                for &choice in choices {
                    ui.selectable_value(value, choice.into(), choice);
                }
            });
    }
    ui.horizontal(|ui| {
        ui.label("Maximum missing fraction");
        ui.add(
            egui::DragValue::new(&mut cfg.max_missing_fraction)
                .range(0. ..=1.)
                .speed(0.01),
        );
        ui.label("Explicit pseudocount (original feature units)");
        ui.add(
            egui::DragValue::new(&mut cfg.pseudocount)
                .range(0. ..=f64::MAX)
                .speed(0.01),
        );
    });
    if cfg.normalization == "internal_standard" {
        ui.horizontal(|ui| {
            ui.label("Internal-standard feature ID");
            ui.text_edit_singleline(cfg.internal_standard.get_or_insert_with(String::new));
        });
    }
    let mut compare = cfg.groups.is_some();
    ui.checkbox(&mut compare, "Compare two independent groups");
    if compare {
        let groups = cfg.groups.get_or_insert_with(|| crate::statistics::Groups {
            metadata_key: "group".into(),
            reference: String::new(),
            comparison: String::new(),
        });
        ui.horizontal(|ui| {
            ui.label("Metadata key");
            ui.text_edit_singleline(&mut groups.metadata_key);
            ui.label("Reference");
            ui.text_edit_singleline(&mut groups.reference);
            ui.label("Comparison");
            ui.text_edit_singleline(&mut groups.comparison);
        });
        ui.horizontal(|ui| {
            ui.label("Confidence level");
            ui.add(
                egui::DragValue::new(&mut cfg.confidence)
                    .range(0.001..=0.999)
                    .speed(0.001),
            );
        });
    } else {
        cfg.groups = None;
    }
    ui.label("Preprocessing order: exclusions → missingness filter → imputation → normalization → transformation → scaling. Welch tests use the pre-scaling values. BH assumes independence or suitable positive dependence; BY is more conservative.");
    if serde_json::to_string(&cfg).unwrap() != original {
        *text = serde_json::to_string_pretty(&cfg).unwrap();
    }
}
fn scatter(
    ui: &mut egui::Ui,
    id: &str,
    axes: [&str; 2],
    points: Vec<[f64; 2]>,
    labels: &[String],
) -> Option<usize> {
    let mut picked = None;
    super::plot_controls::export_scatter(ui, &points, axes[0], axes[1]);
    super::plot_controls::plot(ui, id)
        .height(220.)
        .x_axis_label(axes[0])
        .y_axis_label(axes[1])
        .show(ui, |p| {
            for (i, point) in points.iter().enumerate() {
                p.points(Points::new(labels[i].clone(), vec![*point]).radius(5_f32));
            }
            if p.response().clicked() {
                if let Some(pointer) = p.pointer_coordinate() {
                    picked = points
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| {
                            let screen =
                                p.screen_from_plot(egui_plot::PlotPoint::new(pointer.x, pointer.y));
                            let distance = |v: &[f64; 2]| {
                                p.screen_from_plot(egui_plot::PlotPoint::new(v[0], v[1]))
                                    .distance_sq(screen)
                            };
                            distance(a).total_cmp(&distance(b))
                        })
                        .map(|(i, _)| i);
                }
            }
        });
    picked
}
fn plots(ui: &mut egui::Ui, r: &Report, sample: &mut usize, feature: &mut usize) {
    let n = &r.numerics;
    ui.collapsing("Statistical comparison table", |ui| {
        let selected = n.comparisons.get(*feature).map(|c| c.feature_id.as_str());
        if let Some(index) = super::table::records(
            ui,
            "statistical_comparisons",
            serde_json::to_value(&n.comparisons).unwrap(),
            "feature_id",
            selected,
        ) {
            if let Some(position) = n
                .feature_indices
                .iter()
                .position(|i| r.table.features[*i].id == n.comparisons[index].feature_id)
            {
                *feature = position;
            }
        }
    });
    *sample = (*sample).min(n.sample_indices.len() - 1);
    *feature = (*feature).min(n.feature_indices.len() - 1);
    let samples: Vec<_> = n
        .sample_indices
        .iter()
        .map(|&i| r.table.samples[i].id.clone())
        .collect();
    let features: Vec<_> = n
        .feature_indices
        .iter()
        .map(|&i| r.table.features[i].id.clone())
        .collect();
    ui.label(format!(
        "{} samples × {} features; {} imputed cells; {} tested hypotheses; NumPy {} / SciPy {}",
        samples.len(),
        features.len(),
        n.imputed.len(),
        n.fdr_family_size,
        n.numpy_version,
        n.scipy_version
    ));
    ui.collapsing("Exclusion and imputation ledger",|ui|{ui.label(serde_json::to_string_pretty(&serde_json::json!({"samples":n.excluded_samples,"features":n.excluded_features,"imputed":n.imputed,"groups":n.group_membership})).unwrap());});
    ui.label(format!(
        "PCA: {} • explained variance {:?}; click a point to select sample",
        n.pca.state, n.pca.variance_ratio
    ));
    if let Some(i) = scatter(
        ui,
        "statistics-pca",
        ["PC1 score (processed data)", "PC2 score (0 if unavailable)"],
        n.pca
            .scores
            .iter()
            .map(|v| [v[0], v.get(1).copied().unwrap_or(0.)])
            .collect(),
        &samples,
    ) {
        *sample = i;
    }
    ui.label("PCA feature loadings: click to select a feature");
    if let Some(i) = scatter(
        ui,
        "statistics-loadings",
        ["PC1 loading", "PC2 loading (0 if unavailable)"],
        n.pca
            .loadings
            .iter()
            .map(|v| [v[0], v.get(1).copied().unwrap_or(0.)])
            .collect(),
        &features,
    ) {
        *feature = i;
    }
    let volcano: Vec<_> = n
        .comparisons
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            Some((
                i,
                [c.log2_fold_change?, -c.q?.max(f64::MIN_POSITIVE).log10()],
            ))
        })
        .collect();
    ui.label("Volcano: log2 fold change (normalized arithmetic means) vs −log10 FDR q; unavailable comparisons omitted");
    let labels: Vec<_> = volcano
        .iter()
        .map(|(i, _)| n.comparisons[*i].feature_id.clone())
        .collect();
    if volcano.is_empty() {
        ui.label("No available fold-change/FDR pairs. Declare groups with sufficient independent observations to compute this plot.");
    } else {
        if let Some(i) = scatter(
            ui,
            "statistics-volcano",
            [
                "log₂ fold change (comparison / reference)",
                "−log₁₀ FDR-adjusted q",
            ],
            volcano.iter().map(|(_, p)| *p).collect(),
            &labels,
        ) {
            if let Some(j) = features.iter().position(|f| f == &labels[i]) {
                *feature = j;
            }
        }
    }
    ui.label("Clustered heatmap: processed values, Euclidean distance; click a cell for sample/feature evidence");
    egui::ScrollArea::horizontal()
        .id_salt("heatmap-columns")
        .show(ui, |ui| {
            let width = 85.0;
            ui.horizontal(|ui| {
                ui.add_sized([width, 28.], egui::Label::new("sample / feature"));
                for &j in &n.feature_order {
                    ui.add_sized([width, 28.], egui::Label::new(&features[j]).truncate())
                        .on_hover_text(&features[j]);
                }
            });
            let maximum = n
                .processed
                .iter()
                .flatten()
                .map(|v| v.abs())
                .fold(0., f64::max)
                .max(f64::MIN_POSITIVE);
            egui::ScrollArea::vertical()
                .id_salt("heatmap-samples")
                .max_height(300.)
                .show_rows(ui, 28., n.sample_order.len(), |ui, range| {
                    for row in range {
                        let i = n.sample_order[row];
                        ui.push_id(&samples[i], |ui| {
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [width, 28.],
                                    egui::Label::new(&samples[i]).truncate(),
                                )
                                .on_hover_text(&samples[i]);
                                for &j in &n.feature_order {
                                    let v = n.processed[i][j];
                                    let magnitude = (v.abs() / maximum * 200.) as u8;
                                    let color = if v >= 0. {
                                        egui::Color32::from_rgb(55 + magnitude, 55, 55)
                                    } else {
                                        egui::Color32::from_rgb(55, 55, 55 + magnitude)
                                    };
                                    let text_color = if super::workbench::contrast_ratio(
                                        egui::Color32::WHITE,
                                        color,
                                    ) >= 4.5
                                    {
                                        egui::Color32::WHITE
                                    } else {
                                        egui::Color32::BLACK
                                    };
                                    if ui
                                        .add_sized(
                                            [width, 28.],
                                            egui::Button::new(
                                                egui::RichText::new(format!("{v:.3}"))
                                                    .color(text_color)
                                                    .monospace(),
                                            )
                                            .fill(color),
                                        )
                                        .on_hover_text(format!(
                                            "{} / {}\nFull precision: {v}",
                                            samples[i], features[j]
                                        ))
                                        .clicked()
                                    {
                                        *sample = i;
                                        *feature = j;
                                    }
                                }
                            });
                        });
                    }
                });
        });
    ui.collapsing(
        "Hierarchical dendrograms (merge height = Euclidean distance)",
        |ui| {
            dendrogram(
                ui,
                "sample-dendrogram",
                &n.sample_linkage,
                &n.sample_order,
                &samples,
            );
            dendrogram(
                ui,
                "feature-dendrogram",
                &n.feature_linkage,
                &n.feature_order,
                &features,
            );
        },
    );
    egui::ComboBox::from_label("Selected sample")
        .selected_text(&samples[*sample])
        .show_ui(ui, |ui| {
            for (i, id) in samples.iter().enumerate() {
                ui.selectable_value(sample, i, id);
            }
        });
    egui::ComboBox::from_label("Selected feature")
        .selected_text(&features[*feature])
        .show_ui(ui, |ui| {
            for (i, id) in features.iter().enumerate() {
                ui.selectable_value(feature, i, id);
            }
        });
    ui.label(format!(
        "Original {:?} {}; processed {}; metadata {:?}",
        r.table.values[n.sample_indices[*sample]][n.feature_indices[*feature]],
        r.table.features[n.feature_indices[*feature]].unit,
        n.processed[*sample][*feature],
        r.table.samples[n.sample_indices[*sample]].metadata
    ));
    if let Some(c) = n
        .comparisons
        .iter()
        .find(|c| c.feature_id == features[*feature])
    {
        ui.label(serde_json::to_string_pretty(c).unwrap());
    }
    if let Some(value) = &r.table.targeted {
        if let Ok(batch) = serde_json::from_value::<crate::targeted::BatchResult>(value.clone()) {
            if let Some(observation) = batch
                .observations
                .iter()
                .find(|o| o.sample == samples[*sample] && o.target == features[*feature])
            {
                ui.label(format!(
                    "Linked targeted raw source SHA256 {:?}; target {}",
                    batch.source_hashes.get(&observation.sample),
                    observation.target
                ));
                super::plot_controls::export(
                    ui,
                    &observation.quantifier.trace,
                    "RT (min)",
                    "Original quantifier intensity",
                );
                super::plot_controls::plot(ui, "statistics-targeted-eic")
                    .height(180.)
                    .x_axis_label("Retention time (min)")
                    .y_axis_label("Intensity (instrument units)")
                    .show(ui, |p| {
                        p.line(Line::new(
                            "Original quantifier EIC • minutes / intensity",
                            observation.quantifier.trace.clone(),
                        ))
                    });
                for (i, qualifier) in observation.qualifiers.iter().enumerate() {
                    super::plot_controls::export(
                        ui,
                        &qualifier.trace,
                        "RT (min)",
                        "Original qualifier intensity",
                    );
                    super::plot_controls::plot(ui, format!("statistics-qualifier-{i}"))
                        .height(160.)
                        .x_axis_label("Retention time (min)")
                        .y_axis_label("Intensity (instrument units)")
                        .show(ui, |p| {
                            p.line(Line::new(
                                "Original qualifier EIC • minutes / intensity",
                                qualifier.trace.clone(),
                            ))
                        });
                }
            }
        }
    }
    if let Some(matrix) = &r.table.matrix {
        if let Some(f) = matrix.features.iter().find(|f| f.id == features[*feature]) {
            if let Some(c) = f.cells.iter().find(|c| c.sample_id == samples[*sample]) {
                ui.label(format!(
                    "Linked raw source SHA256 {:?}; original feature {}",
                    matrix.source_hashes.get(&c.sample_id),
                    f.id
                ));
                super::plot_controls::export(ui, &c.eic, "Raw RT (s)", "Original MS1 intensity");
                super::plot_controls::plot(ui, "statistics-linked-eic")
                    .height(180.)
                    .x_axis_label("Raw retention time (s)")
                    .y_axis_label("Intensity (instrument units)")
                    .show(ui, |p| {
                        p.line(Line::new("Raw EIC • seconds / intensity", c.eic.clone()));
                    });
                super::plot_controls::export_spectrum(ui, &c.apex_spectrum, "MS1 intensity");
                super::plot_controls::plot(ui, "statistics-linked-ms1")
                    .height(180.)
                    .x_axis_label("m/z (Th)")
                    .y_axis_label("Intensity (instrument units)")
                    .show(ui, |p| {
                        p.points(Points::new(
                            "Apex MS1 • m/z / intensity",
                            c.apex_spectrum.clone(),
                        ));
                    });
                for ms2 in &c.ms2 {
                    super::plot_controls::export_spectrum(ui, &ms2.peaks, "MS/MS intensity");
                    super::plot_controls::plot(
                        ui,
                        format!("statistics-ms2-{}", ms2.original_index),
                    )
                    .height(160.)
                    .x_axis_label("m/z (Th)")
                    .y_axis_label("Intensity (instrument units)")
                    .show(ui, |p| {
                        p.points(Points::new(
                            format!("Native MS/MS {} • {}", ms2.native_id, ms2.association),
                            ms2.peaks.clone(),
                        ));
                    });
                }
            }
        }
    }
}
fn dendrogram(ui: &mut egui::Ui, id: &str, tree: &[[f64; 4]], order: &[usize], labels: &[String]) {
    if tree.is_empty() {
        return;
    }
    let mut positions = vec![[0., 0.]; labels.len() + tree.len()];
    for (i, &leaf) in order.iter().enumerate() {
        positions[leaf] = [i as f64, 0.];
    }
    let mut segments = Vec::new();
    for (i, row) in tree.iter().enumerate() {
        let a = positions[row[0] as usize];
        let b = positions[row[1] as usize];
        let height = row[2];
        segments.push((
            format!("merge {}", i + 1),
            vec![a, [a[0], height], [b[0], height], b],
        ));
        positions[labels.len() + i] = [(a[0] + b[0]) / 2., height];
    }
    super::plot_controls::export_series(
        ui,
        &segments,
        "Leaf position",
        "Linkage distance (processed data)",
    );
    super::plot_controls::plot(ui, id)
        .height(180.)
        .x_axis_label("Leaf position (see order below)")
        .y_axis_label("Linkage distance (processed data)")
        .show(ui, |p| {
            for (i, row) in tree.iter().enumerate() {
                let a = positions[row[0] as usize];
                let b = positions[row[1] as usize];
                let height = row[2];
                p.line(Line::new(
                    format!("merge {}", i + 1),
                    vec![a, [a[0], height], [b[0], height], b],
                ));
                positions[labels.len() + i] = [(a[0] + b[0]) / 2., height];
            }
        });
    ui.label(format!(
        "Leaf order: {}",
        order
            .iter()
            .map(|&i| labels[i].as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ));
}

impl State {
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some()
    }
}

impl State {
    pub(super) fn retain(&mut self, response: crate::engine::Response) {
        if !self
            .history
            .iter()
            .any(|old| old.result_id == response.result_id)
        {
            self.history.push(response);
            self.selected = self.history.len() - 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The five-step onboarding renders at minimum and desktop sizes in both
    /// themes, with each step heading reachable and the run gate visible.
    #[test]
    fn onboarding_renders_all_five_steps_without_panic() {
        let headings = [
            "Step 1 — Load or select the feature table",
            "Step 2 — Define sample groups and review assignments",
            "Step 3 — Select the analysis and configure routine parameters",
            "Step 4 — Review assumptions, handling, and corrections",
            "Step 5 — Run the analysis and inspect the results",
        ];
        for dark in [false, true] {
            for (width, height) in [(920.0, 620.0), (1280.0, 800.0)] {
                for (step, heading) in headings.iter().enumerate() {
                    // Fresh context per step keeps each preview self-contained
                    // (font atlas uploads stay inside its own frame set).
                    let ctx = egui::Context::default();
                    super::super::workbench::configure(&ctx, dark);
                    let mut app = crate::gui::MzViewerApp::default();
                    app.statistics.open = true;
                    // Synthetic but valid table so steps 2-4 exercise group and
                    // review logic instead of the empty-state branch.
                    app.statistics.draft = serde_json::to_string_pretty(&serde_json::json!({
                        "samples": [
                            {"id": "a", "metadata": {"group": "control"}},
                            {"id": "b", "metadata": {"group": "control"}},
                            {"id": "c", "metadata": {"group": "case"}},
                            {"id": "d", "metadata": {"group": "case"}}
                        ],
                        "features": [{"id": "x", "unit": "ng/mL"}],
                        "values": [[1.0], [2.0], [3.0], [4.0]],
                        "provenance": {"synthetic": true}
                    }))
                    .unwrap();
                    ctx.data_mut(|d| {
                        d.insert_temp(egui::Id::new("statistics_onboarding_step"), step)
                    });
                    // The standalone window needs layout passes before its
                    // text stabilizes; assert on the settled frame.
                    let mut frames = Vec::new();
                    for frame in 0..3 {
                        frames.push(ctx.run(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(width, height),
                                )),
                                time: Some(frame as f64 * 0.2),
                                ..Default::default()
                            },
                            |ctx| show(&mut app, ctx),
                        ));
                    }
                    let output = frames.last().unwrap();
                    assert!(
                        crate::gui::test_render::text_center(&output.shapes, heading).is_some(),
                        "missing {heading} at step {step} ({width}x{height}, dark={dark})"
                    );
                    if std::env::var_os("CHROMASCOPE_STATISTICS_PREVIEW").is_some()
                        && !dark
                        && (width, height) == (1280.0, 800.0)
                    {
                        crate::gui::test_render::save(
                            &ctx,
                            frames.clone(),
                            std::path::Path::new(&format!(
                                "target/statistics-onboarding-{step}.png"
                            )),
                            egui::vec2(width, height),
                        );
                    }
                    for label in [
                        "1 · Load table",
                        "2 · Sample groups",
                        "3 · Analysis",
                        "4 · Review",
                        "5 · Run and inspect",
                    ] {
                        assert!(
                            crate::gui::test_render::text_center(&output.shapes, label).is_some(),
                            "missing navigator {label} at step {step}"
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn heatmap_selection_and_software_plot_render() {
        let table:Table=serde_json::from_value(serde_json::json!({"samples":[{"id":"a","metadata":{"group":"control"}},{"id":"b","metadata":{"group":"control"}},{"id":"c","metadata":{"group":"case"}}],"features":[{"id":"x","unit":"ng/mL"},{"id":"y","unit":"ng/mL"}],"values":[[1,2],[2,4],[3,6]],"provenance":{"synthetic":true}})).unwrap();
        let report =
            crate::statistics::analyze(&table, &Settings::default(), &JobControl::default())
                .unwrap();
        let ctx = egui::Context::default();
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut sample = 2;
        let mut feature = 0;
        let mut frame = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1600., 1800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| plots(ui, &report, &mut sample, &mut feature));
                },
            )
        };
        let mut outputs = vec![frame(vec![]), frame(vec![])];
        let pos = crate::gui::test_render::text_center(&outputs.last().unwrap().shapes, "-2.000")
            .expect("heatmap cell visible");
        for pressed in [true, false] {
            outputs.push(frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ]));
        }
        drop(frame);
        assert_eq!((sample, feature), (0, 1));
        if std::env::var_os("CHROMASCOPE_STATISTICS_PREVIEW").is_some() {
            crate::gui::test_render::save(
                &ctx,
                outputs,
                std::path::Path::new("target/statistics-workspace.png"),
                egui::vec2(1600., 1800.),
            );
        }
    }
}
