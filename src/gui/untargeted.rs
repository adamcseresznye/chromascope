//! Desktop feature matrix review, backed by the same headless engine operation.
use crate::{
    domain::{Operation, Request},
    engine::{Output, Response},
    jobs::JobControl,
    untargeted::{Config, Role, Sample},
};
use eframe::egui;
use egui_plot::Line;
use std::{path::PathBuf, sync::mpsc};
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct State {
    pub(super) open: bool,
    draft: String,
    history: Vec<Response>,
    selected: usize,
    feature: usize,
    sample: usize,
    page: usize,
    ms2: usize,
    annotations: super::annotation::State,
    message: String,
    #[serde(skip)]
    control: JobControl,
    #[serde(skip)]
    pending: Option<mpsc::Receiver<crate::domain::Result<Response>>>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            draft: serde_json::to_string_pretty(&Config::default()).unwrap(),
            history: vec![],
            selected: 0,
            feature: 0,
            sample: 0,
            page: 0,
            ms2: 0,
            annotations: Default::default(),
            message: String::new(),
            control: JobControl::default(),
            pending: None,
        }
    }
}
/// Guided routine setup: sample grouping, feature detection, alignment, and
/// filtering. Every control edits the same typed `Config` behind the draft;
/// invalid values are rejected with a scientific explanation and the draft
/// keeps its last valid state. Expert settings (charge range, gap-fill
/// mechanics, timeouts, matrix limits, checkpoint mechanics) stay in the
/// advanced sections below.
fn routine_setup(ui: &mut egui::Ui, s: &mut State) {
    egui::CollapsingHeader::new("Routine setup — samples, detection, alignment, filtering")
        .id_salt("untargeted_routine_setup")
        .default_open(true)
        .show(ui, |ui| {
            let mut config: Config = match serde_json::from_str(&s.draft) {
                Ok(config) => config,
                Err(error) => {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!("Correct the advanced configuration first: {error}"),
                    );
                    return;
                }
            };
            let mut changed = false;
            ui.strong("Samples and roles");
            ui.small("Blanks reveal background features and QCs track repeatability. Roles only group samples — no sample is ever excluded silently.");
            let (samples, blanks, qcs) = (
                config.samples.iter().filter(|x| x.role == Role::Sample).count(),
                config.samples.iter().filter(|x| x.role == Role::Blank).count(),
                config.samples.iter().filter(|x| x.role == Role::Qc).count(),
            );
            ui.small(format!(
                "{} samples · {} blanks · {} QCs · {:?} polarity",
                samples, blanks, qcs, config.polarity
            ));
            if config.samples.is_empty() {
                ui.weak("No samples yet — use project datasets or select mzML files above.");
            }
            let mut remove = None;
            for (index, sample) in config.samples.iter_mut().enumerate() {
                ui.push_id(("routine_sample", index), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(sample.id.clone());
                        egui::ComboBox::from_id_salt(("routine_role", index))
                            .selected_text(format!("{:?}", sample.role))
                            .show_ui(ui, |ui| {
                                for role in [Role::Sample, Role::Blank, Role::Qc] {
                                    changed |= ui
                                        .selectable_value(
                                            &mut sample.role,
                                            role,
                                            format!("{role:?}"),
                                        )
                                        .changed();
                                }
                            });
                        if ui.small_button("Remove").clicked() {
                            remove = Some(index);
                        }
                    });
                    ui.add(egui::Label::new(sample.source.clone()).truncate())
                        .on_hover_text(sample.source.clone());
                });
            }
            if let Some(index) = remove {
                config.samples.remove(index);
                changed = true;
            }
            ui.separator();
            ui.strong("Feature detection");
            ui.small("Mass tolerance links neighboring scans into one trace; noise intensity drops detector chatter; peak width keeps plausible chromatography.");
            ui.horizontal_wrapped(|ui| {
                ui.label("Polarity");
                egui::ComboBox::from_id_salt("routine_polarity")
                    .selected_text(format!("{:?}", config.polarity))
                    .show_ui(ui, |ui| {
                        for polarity in [
                            crate::spectral::Polarity::Positive,
                            crate::spectral::Polarity::Negative,
                        ] {
                            changed |= ui
                                .selectable_value(
                                    &mut config.polarity,
                                    polarity,
                                    format!("{polarity:?}"),
                                )
                                .changed();
                        }
                    });
                changed |= ui
                    .checkbox(
                        &mut config.centroid_profile,
                        "Profile spectra (pick peaks before tracing)",
                    )
                    .changed();
            });
            egui::Grid::new("routine_detection")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Detection tolerance (ppm)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.detection_ppm)
                                .range(0.1..=1000.0)
                                .speed(0.5),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Noise intensity");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.noise_intensity)
                                .range(0.0..=1.0e9)
                                .speed(100.0),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Minimum trace length (s)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.min_trace_seconds)
                                .range(1.0..=3600.0)
                                .speed(0.5),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Typical peak width (s)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.peak_fwhm_seconds)
                                .range(0.5..=3600.0)
                                .speed(0.5),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Accepted width range (s)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.min_fwhm_seconds)
                                .range(0.5..=3600.0)
                                .speed(0.5),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.max_fwhm_seconds)
                                .range(0.5..=3600.0)
                                .speed(0.5),
                        )
                        .changed();
                    ui.end_row();
                });
            ui.separator();
            ui.strong("Alignment across samples");
            ui.small("Retention-time tolerance links the same compound across runs; correspondence tolerance links its mass.");
            egui::Grid::new("routine_alignment")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Correspondence tolerance (ppm)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.correspondence_ppm)
                                .range(0.1..=1000.0)
                                .speed(0.5),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Retention-time tolerance (s)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.rt_tolerance_seconds)
                                .range(1.0..=300.0)
                                .speed(0.5),
                        )
                        .changed();
                    ui.end_row();
                });
            ui.separator();
            ui.strong("Filtering");
            ui.small("Blank ratio, QC repeatability, and sample coverage flag rows — flagged features stay in the matrix, never deleted.");
            egui::Grid::new("routine_filtering")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Blank ratio (sample ÷ blank)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.blank_ratio)
                                .range(1.0..=1000.0)
                                .speed(0.1),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Maximum QC variation (fraction)");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.max_qc_cv)
                                .range(0.0..=5.0)
                                .speed(0.01),
                        )
                        .changed();
                    ui.end_row();
                    ui.label("Minimum sample fraction");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.min_sample_fraction)
                                .range(0.0..=1.0)
                                .speed(0.05),
                        )
                        .changed();
                    ui.end_row();
                });
            ui.horizontal_wrapped(|ui| {
                changed |= ui
                    .checkbox(&mut config.gap_fill, "Fill gaps by re-integration")
                    .on_hover_text("Re-integrate missing values from raw data instead of leaving them missing")
                    .changed();
                if config.gap_fill {
                    ui.label("Minimum scans");
                    changed |= ui
                        .add(
                            egui::DragValue::new(&mut config.gap_min_scans).range(1..=100),
                        )
                        .changed();
                }
            });
            if changed {
                match check_routine(&config) {
                    Ok(()) => {
                        s.draft = serde_json::to_string_pretty(&config).unwrap();
                    }
                    Err(problem) => s.message = problem,
                }
            }
        });
}

/// Routine-range validation in scientific language. Expert ranges stay with
/// the engine; this only rejects values that cannot be chromatography.
fn check_routine(config: &Config) -> Result<(), String> {
    if !(0.1..=1000.0).contains(&config.detection_ppm) {
        return Err(
            "Detection tolerance must be 0.1–1000 ppm; wider windows merge neighboring compounds."
                .into(),
        );
    }
    if !(0.1..=1000.0).contains(&config.correspondence_ppm) {
        return Err(
            "Correspondence tolerance must be 0.1–1000 ppm; wider windows merge distinct features across samples."
                .into(),
        );
    }
    if !(1.0..=300.0).contains(&config.rt_tolerance_seconds) {
        return Err(
            "Retention-time tolerance must be 1–300 s; wider windows link unrelated peaks across runs."
                .into(),
        );
    }
    if !config.noise_intensity.is_finite() || config.noise_intensity < 0.0 {
        return Err("Noise intensity must be a finite nonnegative intensity.".into());
    }
    if !(1.0..=3600.0).contains(&config.min_trace_seconds) {
        return Err("Minimum trace length must be 1–3600 s.".into());
    }
    if !(0.5..=3600.0).contains(&config.min_fwhm_seconds)
        || !(0.5..=3600.0).contains(&config.peak_fwhm_seconds)
        || !(0.5..=3600.0).contains(&config.max_fwhm_seconds)
        || config.min_fwhm_seconds > config.peak_fwhm_seconds
        || config.peak_fwhm_seconds > config.max_fwhm_seconds
    {
        return Err(
            "Peak widths must satisfy minimum ≤ typical ≤ maximum within 0.5–3600 s.".into(),
        );
    }
    if !config.blank_ratio.is_finite() || config.blank_ratio < 1.0 {
        return Err("Blank ratio below 1 keeps every background feature; use 1 or higher.".into());
    }
    if !config.max_qc_cv.is_finite() || config.max_qc_cv < 0.0 {
        return Err("QC variation limit must be a finite nonnegative number.".into());
    }
    if !(0.0..=1.0).contains(&config.min_sample_fraction) {
        return Err("Sample fraction must be 0–1.".into());
    }
    if config.gap_min_scans < 1 {
        return Err("Gap filling needs at least one supporting scan.".into());
    }
    Ok(())
}
pub(super) fn show(app: &mut super::MzViewerApp, ctx: &egui::Context) {
    let mut loaded: Vec<_> = app
        .files
        .values()
        .filter(|f| !f.is_loading)
        .map(|f| (f.id, f.path.clone(), f.name.clone()))
        .collect();
    loaded.sort_by_key(|f| f.0);
    let s = &mut app.untargeted;
    if let Some(rx) = &s.pending {
        match rx.try_recv() {
            Ok(Ok(response)) => {
                s.history.push(response);
                s.selected = s.history.len() - 1;
                s.feature = 0;
                s.pending = None;
                s.message = "Feature matrix retained with source-linked evidence".into();
            }
            Ok(Err(e)) => {
                s.message = e.to_string();
                s.pending = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                s.message = "Worker disconnected".into();
                s.pending = None;
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }
    let mut open = s.open;
    super::workbench::analytical_panel(ctx, "Untargeted LC-HRMS", &mut open, |ui| {
        if ui
            .add_enabled(
                s.pending.is_none() && !loaded.is_empty(),
                egui::Button::new("Use project datasets"),
            )
            .clicked()
        {
            if let Ok(mut config) = serde_json::from_str::<Config>(&s.draft) {
                config.samples = loaded
                    .iter()
                    .map(|(id, path, name)| Sample {
                        id: format!("dataset-{id}"),
                        source: path.clone(),
                        role: Role::Sample,
                        metadata: serde_json::json!({"name":name}),
                    })
                    .collect();
                s.draft = serde_json::to_string_pretty(&config).unwrap();
            }
        }
        ui.small("Requires the OpenMS 3.5.0 runtime; results record its version and refuse other versions. Retention times are in seconds and intensities are monoisotopic MS1 intensity × seconds. Checkpointing lets long multi-sample runs resume after interruption.");
        ui.small("A checkpoint directory stores verified intermediate results so processing can resume. Results from a different OpenMS version are rejected, not silently reused.");
        if ui
            .add_enabled(
                s.pending.is_none(),
                egui::Button::new("Select multi-sample mzML files"),
            )
            .clicked()
        {
            if let Some(files) = rfd::FileDialog::new()
                .add_filter("mzML", &["mzML"])
                .pick_files()
            {
                let mut config: Config = serde_json::from_str(&s.draft).unwrap_or_default();
                config.samples = files.iter().enumerate().map(|(i,p)| Sample { id: format!("sample-{}",i+1), source:p.to_string_lossy().into(),role:Role::Sample,metadata:serde_json::json!({"name":p.file_name().map(|n|n.to_string_lossy().to_string())}) }).collect();
                s.draft = serde_json::to_string_pretty(&config).unwrap();
            }
        }
        if ui
            .add_enabled(
                s.pending.is_none(),
                egui::Button::new("Select checkpoint directory"),
            )
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                if let Ok(mut config) = serde_json::from_str::<Config>(&s.draft) {
                    config.cache_directory = path.to_string_lossy().into();
                    s.draft = serde_json::to_string_pretty(&config).unwrap();
                }
            }
        }
        routine_setup(ui, s);
        super::forms::typed::<Config>(
            ui,
            "Advanced sample sheet and full processing settings",
            &mut s.draft,
        );
        ui.collapsing("Advanced configuration JSON — expert full-parameter audit", |ui| {
            ui.small("Routine samples, detection, alignment, and filtering are configured in the guided section above. This editor is intentionally retained for exact-parameter review and reproducibility.");
            ui.add(
                egui::TextEdit::multiline(&mut s.draft)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(16),
            );
        });
        if ui
            .add_enabled(
                s.pending.is_none(),
                egui::Button::new("Process / resume verified checkpoints"),
            )
            .clicked()
        {
            match serde_json::from_str::<Config>(&s.draft) {
                Ok(config) => {
                    let request = Request {
                        version: 1,
                        operation_id: Default::default(),
                        actor: "desktop".into(),
                        operation: Operation::UntargetedBatch { config },
                    };
                    let (tx, rx) = mpsc::channel();
                    s.control = JobControl::default();
                    let control = s.control.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(crate::engine::execute(
                            &PathBuf::from("-"),
                            request,
                            &control,
                        ));
                    });
                    s.pending = Some(rx);
                }
                Err(e) => s.message = e.to_string(),
            }
        }
        if s.pending.is_some() {
            ui.label(format!(
                "Completed sample phases: {} (detection, then evidence)",
                s.control.completed_scans()
            ));
            if ui.button("Cancel processing").clicked() {
                s.control.cancel();
            }
        }
        ui.label(&s.message);
        if ui.button("Load retained feature matrix JSON").clicked() {
            if let Some(path) = rfd::FileDialog::new().pick_file() {
                let result = (|| {
                    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                    let value: serde_json::Value =
                        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                    let response: Response =
                        serde_json::from_value(value.get("result").cloned().unwrap_or(value))
                            .map_err(|e| e.to_string())?;
                    crate::untargeted::verify_response(&response).map_err(|e| e.to_string())?;
                    Ok::<_, String>(response)
                })();
                match result {
                    Ok(r) => {
                        s.history.push(r);
                        s.selected = s.history.len() - 1;
                        s.feature = 0;
                    }
                    Err(e) => s.message = e,
                }
            }
        }
        egui::ComboBox::from_label("Retained run")
            .selected_text(format!("{}", s.selected + 1))
            .show_ui(ui, |ui| {
                for (i, r) in s.history.iter().enumerate() {
                    ui.selectable_value(&mut s.selected, i, r.result_id.0.to_string());
                }
            });
        if let Some(response) = s.history.get(s.selected) {
            if let Output::FeatureMatrix { report } = &response.output {
                super::annotation::show(ui, &mut s.annotations, report, s.feature, s.sample);
                ui.horizontal(|ui| {
                    for (label, kind) in [
                        ("Save full evidence JSON", 0),
                        ("Export all features", 1),
                        ("Export included features", 2),
                    ] {
                        if ui.button(label).clicked() {
                            if let Some(path) = rfd::FileDialog::new().save_file() {
                                use std::io::Write;
                                let result = (|| {
                                    let bytes = if kind == 0 {
                                        serde_json::to_vec_pretty(response)
                                            .map_err(|e| e.to_string())?
                                    } else {
                                        crate::untargeted::matrix_csv(report, kind == 2)
                                            .map_err(|e| e.to_string())?
                                            .into_bytes()
                                    };
                                    std::fs::OpenOptions::new()
                                        .write(true)
                                        .create_new(true)
                                        .open(path)
                                        .and_then(|mut f| f.write_all(&bytes))
                                        .map_err(|e| e.to_string())
                                })();
                                s.message =
                                    result.err().unwrap_or_else(|| "Saved new artifact".into());
                            }
                        }
                    }
                });
                ui.label(format!(
                    "{} features × {} samples; all flagged rows retained",
                    report.features.len(),
                    report.config.samples.len()
                ));
                ui.collapsing("Alignment and provenance", |ui| {
                    ui.label(serde_json::to_string_pretty(&report.alignments).unwrap());
                    ui.label(serde_json::to_string_pretty(&report.provenance).unwrap());
                });
                use super::table::{Cell, Row};
                let mut headers = vec!["Feature", "m/z (Th)", "Aligned RT (s)"];
                headers.extend(
                    report
                        .config
                        .samples
                        .iter()
                        .map(|sample| sample.id.as_str()),
                );
                headers.push("Filter / flags");
                let cache_key = egui::Id::new(("feature-table-model", response.result_id.0));
                let cached = ui
                    .ctx()
                    .data(|d| d.get_temp::<std::sync::Arc<Vec<Row>>>(cache_key));
                let rows = cached.unwrap_or_else(|| {
                    let rows: Vec<_> = report
                        .features
                        .iter()
                        .map(|feature| {
                            let mut cells = vec![
                                Cell::text(&feature.id),
                                Cell::number(Some(feature.mz)),
                                Cell::number(Some(feature.aligned_rt_seconds)),
                            ];
                            cells.extend(feature.cells.iter().map(|cell| Cell {
                                text: format!(
                                    "{} ({:?})",
                                    cell.intensity
                                        .map(|v| format!("{v:.1}"))
                                        .unwrap_or_else(|| "Missing".into()),
                                    cell.state
                                ),
                                number: cell.intensity,
                            }));
                            cells.push(Cell::text(format!(
                                "{:?}: {}",
                                feature.filter_state,
                                feature.flags.join("; ")
                            )));
                            Row {
                                key: feature.id.clone(),
                                cells,
                            }
                        })
                        .collect();
                    let rows = std::sync::Arc::new(rows);
                    ui.ctx()
                        .data_mut(|d| d.insert_temp(cache_key, rows.clone()));
                    rows
                });
                if let Some(index) = super::table::show(
                    ui,
                    "feature-matrix-grid",
                    &headers,
                    rows.as_ref(),
                    rows.get(s.feature).map(|row| row.key.as_str()),
                ) {
                    s.feature = index;
                    let column = super::table::selected_column(ui, "feature-matrix-grid");
                    if column >= 3 && column < 3 + report.config.samples.len() {
                        s.sample = column - 3;
                    }
                }
                egui::ComboBox::from_label("Linked sample")
                    .selected_text(
                        report
                            .config
                            .samples
                            .get(s.sample)
                            .map(|v| v.id.as_str())
                            .unwrap_or("Select"),
                    )
                    .show_ui(ui, |ui| {
                        for (i, sample) in report.config.samples.iter().enumerate() {
                            ui.selectable_value(&mut s.sample, i, &sample.id);
                        }
                    });
                if let Some(feature) = report.features.get(s.feature) {
                    if let Some(cell) = feature.cells.get(s.sample) {
                        ui.label(serde_json::to_string(&report.config.samples[s.sample]).unwrap());
                        let number = |v: Option<f64>| {
                            v.map(|v| format!("{v:.4}"))
                                .unwrap_or_else(|| "Missing".into())
                        };
                        ui.label(format!("{:?}; EIC area {}; original OpenMS FWHM area {}; raw RT {} s; aligned RT {} s",cell.state,number(cell.intensity),number(cell.openms_intensity),number(cell.raw_rt_seconds),number(cell.aligned_rt_seconds)));
                        ui.label(format!(
                            "Isotope m/z {:?}; adduct {}; group {}",
                            cell.isotope_mz,
                            cell.adduct.as_deref().unwrap_or("Unassigned"),
                            cell.adduct_group.as_deref().unwrap_or("Unassigned")
                        ));
                        super::plot_controls::export(ui, &cell.eic, "Raw RT (s)", "MS1 intensity");
                        super::plot_controls::plot(ui, "feature-eic")
                            .height(180.)
                            .x_axis_label("Raw RT (seconds)")
                            .y_axis_label("MS1 intensity")
                            .show(ui, |plot| {
                                plot.line(Line::new("Linked EIC", cell.eic.clone()));
                            });
                        super::plot_controls::export_spectrum(
                            ui,
                            &cell.apex_spectrum,
                            "MS1 intensity",
                        );
                        super::plot_controls::plot(ui, "feature-spectrum")
                            .height(180.)
                            .x_axis_label("m/z (Th)")
                            .y_axis_label("MS1 intensity")
                            .show(ui, |plot| {
                                plot.line(Line::new(
                                    "Apex MS1 spectrum",
                                    sticks(&cell.apex_spectrum),
                                ));
                            });
                        if cell.ms2.is_empty() {
                            ui.label("Associated MS/MS evidence: Missing");
                        } else {
                            egui::ComboBox::from_label("Associated MS/MS scan")
                                .selected_text(
                                    cell.ms2
                                        .get(s.ms2)
                                        .or(cell.ms2.first())
                                        .map(|v| v.native_id.as_str())
                                        .unwrap_or("Missing"),
                                )
                                .show_ui(ui, |ui| {
                                    for (i, scan) in cell.ms2.iter().enumerate() {
                                        ui.selectable_value(&mut s.ms2, i, &scan.native_id);
                                    }
                                });
                            if let Some(scan) = cell.ms2.get(s.ms2).or(cell.ms2.first()) {
                                ui.label(format!("Scan {} • raw RT {:.2} s • precursor {:.6} Th • {} • energy {} eV",scan.original_index,scan.raw_rt_seconds,scan.precursor_mz,scan.representation,number(scan.collision_energy_ev)));
                                super::plot_controls::export_spectrum(
                                    ui,
                                    &scan.peaks,
                                    "MS/MS intensity",
                                );
                                super::plot_controls::plot(ui, "feature-ms2")
                                    .height(180.)
                                    .x_axis_label("m/z (Th)")
                                    .y_axis_label("MS/MS intensity")
                                    .show(ui, |plot| {
                                        plot.line(Line::new(&scan.native_id, sticks(&scan.peaks)));
                                    });
                            }
                        }
                    }
                }
            }
        }
    });
    s.open = open;
}
impl State {
    pub(super) fn retain_annotations(
        &mut self,
        ledger: crate::annotation::Ledger,
    ) -> Result<(), String> {
        self.annotations.retain(ledger)
    }
    pub(super) fn latest_matrix(&self) -> Option<&crate::untargeted::Report> {
        self.history.get(self.selected).and_then(|r| {
            if let Output::FeatureMatrix { report } = &r.output {
                Some(report.as_ref())
            } else {
                None
            }
        })
    }
}
fn sticks(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    points
        .iter()
        .flat_map(|p| [[p[0], 0.], *p, [p[0], 0.]])
        .collect()
}

impl State {
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some() || self.annotations.busy()
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
pub(crate) mod tests {
    use super::*;
    pub(crate) fn response() -> Response {
        let config = Config {
            samples: vec![
                Sample {
                    id: "sample-a".into(),
                    source: "missing-test-a.mzML".into(),
                    role: Role::Sample,
                    metadata: serde_json::json!({"synthetic":true}),
                },
                Sample {
                    id: "blank".into(),
                    source: "missing-test-blank.mzML".into(),
                    role: Role::Blank,
                    metadata: serde_json::json!({"synthetic":true}),
                },
            ],
            cache_directory: "target/gui-untargeted-test".into(),
            ..Default::default()
        };
        let report:crate::untargeted::Report=serde_json::from_value(serde_json::json!({
            "version":1,"adapter_version":crate::untargeted::ADAPTER_VERSION,"openms_version":"3.5.0","config":config,
            "source_hashes":{"sample-a":"a".repeat(64),"blank":"b".repeat(64)},
            "features":[{"id":"synthetic-triangle","mz":100.,"aligned_rt_seconds":1.,"filter_state":"indeterminate","flags":["qc_filter_indeterminate"],"blank_ratio":10.,"qc_cv":null,"sample_fraction":1.,
                "cells":[{"sample_id":"sample-a","state":"detected","intensity":20.,"openms_intensity":10.,"raw_rt_seconds":1.,"aligned_rt_seconds":1.,"raw_bounds_seconds":[0.,2.],"mz":100.,"charge":1,"isotope_mz":[100.],"adduct_group":null,"adduct":null,"eic":[[0.,0.],[1.,20.],[2.,0.]],"apex_spectrum":[[100.,20.]],"ms2":[]},
                         {"sample_id":"blank","state":"detected","intensity":2.,"openms_intensity":1.,"raw_rt_seconds":1.,"aligned_rt_seconds":1.,"raw_bounds_seconds":[0.,2.],"mz":100.,"charge":1,"isotope_mz":[100.],"adduct_group":null,"adduct":null,"eic":[[0.,0.],[1.,2.],[2.,0.]],"apex_spectrum":[[100.,2.]],"ms2":[]}]}],
            "alignments":[{"sample_id":"sample-a","reference_sample_id":"sample-a","state":"reference","slope":1.,"intercept_seconds":0.},{"sample_id":"blank","reference_sample_id":"blank","state":"reference","slope":1.,"intercept_seconds":0.}],
            "provenance":{"synthetic":true},"warnings":[]})).unwrap();
        let response = Response {
            version: 1,
            result_id: Default::default(),
            request: Request {
                version: 1,
                operation_id: Default::default(),
                actor: "gui-test".into(),
                operation: Operation::UntargetedBatch { config },
            },
            kernel_version: crate::untargeted::ADAPTER_VERSION.into(),
            source_sha256: None,
            started_unix_ms: 0,
            finished_unix_ms: 1,
            output: Output::FeatureMatrix {
                report: Box::new(report),
            },
        };
        crate::untargeted::verify_response(&response).unwrap();
        response
    }
    fn frame(
        app: &mut super::super::MzViewerApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600., 1600.),
                )),
                events,
                ..Default::default()
            },
            |ctx| show(app, ctx),
        )
    }
    fn click(
        app: &mut super::super::MzViewerApp,
        ctx: &egui::Context,
        text: &str,
        frames: &mut Vec<egui::FullOutput>,
    ) {
        let pos = super::super::test_render::scroll_to_text(text, frames, |events| {
            frame(app, ctx, events)
        });
        for pressed in [true, false] {
            frames.push(frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            ));
        }
    }
    #[test]
    fn routine_setup_rejects_non_chromatographic_values() {
        assert!(check_routine(&Config::default()).is_ok());
        assert!(check_routine(&Config {
            detection_ppm: 0.0,
            ..Default::default()
        })
        .is_err());
        assert!(check_routine(&Config {
            min_fwhm_seconds: 10.0,
            peak_fwhm_seconds: 5.0,
            ..Default::default()
        })
        .is_err());
        assert!(check_routine(&Config {
            blank_ratio: 0.5,
            ..Default::default()
        })
        .is_err());
    }
    #[test]
    fn matrix_links_sample_eic_and_retains_original_on_worker_failure() {
        let mut app = super::super::MzViewerApp::default();
        app.untargeted.open = true;
        app.untargeted.history.push(response());
        let original = serde_json::to_string(&app.untargeted.history).unwrap();
        let ctx = egui::Context::default();
        ctx.style_mut(|s| s.animation_time = 0.);
        let mut frames = vec![frame(&mut app, &ctx, vec![])];
        frames.push(frame(&mut app, &ctx, vec![]));
        click(&mut app, &ctx, "2.0 (Detected)", &mut frames);
        assert_eq!(app.untargeted.sample, 1);
        let draft = match &app.untargeted.history[0].request.operation {
            Operation::UntargetedBatch { config } => serde_json::to_string(config).unwrap(),
            _ => unreachable!(),
        };
        app.untargeted.draft = draft;
        click(
            &mut app,
            &ctx,
            "Process / resume verified checkpoints",
            &mut frames,
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while app.untargeted.pending.is_some() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
            frames.push(frame(&mut app, &ctx, vec![]));
        }
        assert!(app.untargeted.message.contains("missing_source"));
        assert_eq!(
            serde_json::to_string(&app.untargeted.history).unwrap(),
            original
        );
        if std::env::var_os("CHROMASCOPE_UNTARGETED_PREVIEW").is_some() {
            super::super::test_render::save(
                &ctx,
                frames,
                PathBuf::from("target/untargeted-review.png").as_path(),
                egui::vec2(1600., 1600.),
            );
        }
    }
}
