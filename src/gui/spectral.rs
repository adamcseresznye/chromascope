//! Desktop spectral adapter. Background engine execution and immutable response history.
use crate::{
    domain::{Operation, Request},
    engine::{Output, Response},
    spectral::*,
};
use eframe::egui;
use egui_plot::Line;
use std::{path::PathBuf, sync::mpsc};
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct State {
    pub(super) open: bool,
    indices: String,
    background_indices: String,
    processing: String,
    source: String,
    search_config: String,
    draft: String,
    history: Vec<Response>,
    selected: usize,
    candidate: usize,
    show_raw: bool,
    observed_mass: String,
    message: String,
    #[serde(skip)]
    pending: Option<mpsc::Receiver<Result<Response, crate::domain::EngineError>>>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            indices: "0".into(),
            background_indices: String::new(),
            processing: serde_json::to_string_pretty(&Processing::default()).unwrap(),
            source: serde_json::to_string_pretty(&LibrarySource {
                name: String::new(),
                version: String::new(),
                url: String::new(),
                license: String::new(),
            })
            .unwrap(),
            search_config: serde_json::to_string_pretty(&SearchConfig::default()).unwrap(),
            draft: String::new(),
            history: vec![],
            selected: 0,
            candidate: 0,
            show_raw: false,
            observed_mass: String::new(),
            message: String::new(),
            pending: None,
        }
    }
}
fn parsed_indices(text: &str) -> Result<Vec<usize>, String> {
    if text.trim().is_empty() {
        return Ok(vec![]);
    }
    text.split(',')
        .map(|v| {
            v.trim()
                .parse()
                .map_err(|_| "Use comma-separated scan indices".into())
        })
        .collect()
}
impl State {
    pub(super) fn retain(&mut self, response: Response) {
        if !self
            .history
            .iter()
            .any(|r| r.result_id == response.result_id)
        {
            self.history.push(response);
            self.selected = self.history.len() - 1;
        }
    }
}
fn editor(ui: &mut egui::Ui, text: &mut String) {
    egui::ScrollArea::vertical()
        .max_height(160.0)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(text)
                    .code_editor()
                    .desired_width(f32::INFINITY),
            );
        });
}
pub(super) fn verify(response: &Response) -> Result<(), crate::domain::EngineError> {
    if let Output::Chromatogram { points, .. } = &response.output {
        crate::engine::validate_points(points)
    } else {
        crate::spectral::verify_response(response)
    }
}
fn latest_query(state: &State) -> Option<Processed> {
    state.history.iter().rev().find_map(|r| match &r.output {
        Output::SpectralProcessing { processed } => Some(*processed.clone()),
        Output::SpectralSearch { report } => Some(report.query.clone()),
        _ => None,
    })
}
fn latest_library(state: &State) -> Option<Library> {
    state.history.iter().rev().find_map(|r| match &r.output {
        Output::SpectralLibrary { library } => Some(*library.clone()),
        Output::SpectralSearch { report } => Some(report.library.clone()),
        _ => None,
    })
}
fn draft(state: &mut State, operation: Operation) {
    match serde_json::to_string_pretty(&operation) {
        Ok(text) => state.draft = text,
        Err(e) => state.message = e.to_string(),
    }
}
pub(super) fn show(app: &mut super::MzViewerApp, ctx: &egui::Context) {
    let path = app
        .active_file_id
        .and_then(|id| app.files.get(&id))
        .map(|f| PathBuf::from(&f.path));
    let lease = app
        .active_file_id
        .and_then(|id| app.files.get(&id))
        .and_then(|f| f.cache.import_workspace.clone());
    let s = &mut app.spectral;
    if let Some(rx) = &s.pending {
        match rx.try_recv() {
            Ok(Ok(response)) => {
                if let Output::SpectralProcessing { processed } = &response.output {
                    s.observed_mass = processed
                        .spectrum
                        .precursor_mz
                        .or_else(|| processed.spectrum.peaks.first().map(|p| p[0]))
                        .map(|mz| mz.to_string())
                        .unwrap_or_default();
                }
                s.history.push(response);
                s.selected = s.history.len() - 1;
                s.pending = None;
                s.message = "Engine result retained; unreviewed identities remain unknown".into();
            }
            Ok(Err(e)) => {
                s.message = e.to_string();
                s.pending = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                s.message = "Worker disconnected".into();
                s.pending = None;
            }
            Err(mpsc::TryRecvError::Empty) => ctx.request_repaint(),
        }
    }
    let mut open = s.open;
    super::workbench::analytical_panel(
        ctx,
        "Spectra and compound identification",
        &mut open,
        |ui| panel(s, path, lease, ui),
    );
    s.open = open;
}
fn panel(
    s: &mut State,
    path: Option<PathBuf>,
    lease: Option<std::sync::Arc<tempfile::TempDir>>,
    ui: &mut egui::Ui,
) {
    ui.label("MS1/MS2 inspection, retained processing, local libraries, candidate overlays and evidence-based annotations");

    ui.label(format!(
        "Active source: {}",
        path.as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "No acquisition selected".into())
    ));

    let embedded = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(egui::Id::new("central_workspaces")))
        .unwrap_or(false);
    let stage_key = egui::Id::new("spectral_stage");
    let mut stage = ui
        .ctx()
        .data(|d| d.get_temp::<usize>(stage_key))
        .unwrap_or(0);
    if embedded {
        ui.horizontal_wrapped(|ui| {
            for (index, label) in [
                "Inspect and process",
                "Libraries and search",
                "Formula and isotope hypotheses",
                "Evidence and review",
            ]
            .iter()
            .enumerate()
            {
                ui.selectable_value(&mut stage, index, *label);
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(stage_key, stage));
    }
    if !embedded || stage == 0 {
        super::forms::typed::<Processing>(ui, "Spectrum processing", &mut s.processing);
        ui.horizontal(|ui| {
            ui.label("Signal indices");
            ui.text_edit_singleline(&mut s.indices);
            ui.label("Background indices");
            ui.text_edit_singleline(&mut s.background_indices);
        });

        ui.collapsing(
            "Processing: mass tolerance, profile SNR, subtraction scale, peak filter",
            |ui| editor(ui, &mut s.processing),
        );

        if ui
            .button("Prepare acquisition inspection / average")
            .clicked()
        {
            let result = (|| {
                Ok::<_, String>(Operation::InspectSpectra {
                    indices: parsed_indices(&s.indices)?,
                    background_indices: parsed_indices(&s.background_indices)?,
                    config: serde_json::from_str(&s.processing).map_err(|e| e.to_string())?,
                })
            })();
            match result {
                Ok(op) => draft(s, op),
                Err(e) => s.message = e,
            }
        }
    }
    if !embedded || stage == 1 {
        super::forms::typed::<LibrarySource>(ui, "Library source and license", &mut s.source);
        ui.collapsing("Advanced library source JSON", |ui| {
            editor(ui, &mut s.source)
        });

        if ui
            .button("Import local MSP / MGF / MassBank library")
            .clicked()
        {
            if let Some(file) = rfd::FileDialog::new()
                .add_filter("Spectral library", &["msp", "mgf", "txt"])
                .pick_file()
            {
                let result = (|| {
                    let source = serde_json::from_str(&s.source).map_err(|e| e.to_string())?;
                    if std::fs::metadata(&file).map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024
                    {
                        return Err("Library exceeds 32 MiB".into());
                    }
                    let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
                    let format = match file.extension().and_then(|v| v.to_str()) {
                        Some("mgf") => "mgf",
                        Some("msp") => "msp",
                        _ => "massbank",
                    };
                    Ok::<_, String>(Operation::ImportSpectralLibrary {
                        text,
                        format: format.into(),
                        source,
                    })
                })();
                match result {
                    Ok(op) => draft(s, op),
                    Err(e) => s.message = e,
                }
            }
        }

        super::forms::typed::<SearchConfig>(
            ui,
            "Search compatibility and scoring",
            &mut s.search_config,
        );
        ui.collapsing("Advanced search settings JSON", |ui| {
            editor(ui, &mut s.search_config)
        });

        ui.horizontal(|ui|{

            if ui.button("Prepare library search").clicked(){
match (latest_query(s),latest_library(s),serde_json::from_str(&s.search_config)){
(Some(query),Some(library),Ok(config))=>draft(s,Operation::SearchSpectralLibrary{
query:Box::new(query),library:Box::new(library),config}
),_=>s.message="Inspect/process a spectrum, import a library and supply valid search settings first".into()}
}

            if ui.button("Prepare metadata amendment / reprocessing").clicked(){
if let Some(query)=latest_query(s){
draft(s,Operation::ProcessSpectra{
spectra:query.inputs,background:query.background,config:query.config}
);
}
}

        }
);
    }
    if !embedded || stage == 2 {
        ui.label("Unknown acquisition fields stay missing. Declare precursor type, instrument and collision energy with units in the reprocessing draft; native acquisition metadata stays retained. Search defaults reject missing compatibility.");

        ui.horizontal(|ui| {
            ui.label("Observed m/z for formula / monoisotopic m/z for isotope inspection");
            ui.text_edit_singleline(&mut s.observed_mass);
        });
        ui.horizontal(|ui| {
            if ui.button("Prepare formula hypotheses").clicked() {
                if let Some(q) = latest_query(s) {
                    let Ok(observed_mz) = s.observed_mass.trim().parse::<f64>() else {
                        s.message = "Enter the observed m/z for formula hypotheses".into();
                        return;
                    };
                    let config = FormulaConfig {
                        observed_mz,
                        polarity: q.spectrum.polarity,
                        adducts: vec![if q.spectrum.polarity == Polarity::Negative {
                            "[M-H]-"
                        } else {
                            "[M+H]+"
                        }
                        .into()],
                        tolerance: Tolerance {
                            value: 5.0,
                            unit: MassUnit::Ppm,
                        },
                        maximum_atoms: std::collections::BTreeMap::from([
                            ("C".into(), 40),
                            ("H".into(), 80),
                            ("N".into(), 10),
                            ("O".into(), 20),
                            ("S".into(), 2),
                        ]),
                        maximum_candidates: 100,
                    };
                    draft(s, Operation::FormulaCandidates { config });
                }
            }

            if ui.button("Prepare MS1 isotope inspection").clicked() {
                if let Some(q) = latest_query(s) {
                    let Ok(mass) = s.observed_mass.trim().parse::<f64>() else {
                        s.message = "Enter the observed monoisotopic m/z".into();
                        return;
                    };
                    draft(
                        s,
                        Operation::AnalyzeIsotopes {
                            spectrum: q.spectrum,
                            config: IsotopeConfig {
                                expected_atom_counts: None,
                                monoisotopic_mz: mass,
                                maximum_charge: 3,
                                maximum_isotopes: 4,
                                tolerance: Tolerance {
                                    value: 5.0,
                                    unit: MassUnit::Ppm,
                                },
                            },
                        },
                    );
                }
            }

            if ui.button("Prepare linked precursor XIC").clicked() {
                if let Some(q) = latest_query(s) {
                    if let Some(mass) = q.spectrum.precursor_mz {
                        let mut p = crate::processing::ProcessingParams {
                            acquisition: None,
                            plot_type: crate::plotting_parameters::PlotType::Xic,
                            ms_level: 1,
                            polarity: mzdata::spectrum::ScanPolarity::Unknown,
                            smoothing: 0,
                            xic_params: None,
                            mz_range: None,
                            precursor_mz: None,
                        };
                        p.plot_type = crate::plotting_parameters::PlotType::Xic;
                        p.ms_level = 1;
                        p.polarity = match q.spectrum.polarity {
                            Polarity::Negative => mzdata::spectrum::ScanPolarity::Negative,
                            Polarity::Positive => mzdata::spectrum::ScanPolarity::Positive,
                            Polarity::Unknown => mzdata::spectrum::ScanPolarity::Unknown,
                        };
                        match crate::validation::XicParams::new(
                            mass,
                            p.polarity,
                            10.0,
                            &crate::validation::DataBounds::unrestricted(),
                        ) {
                            Ok(x) => {
                                p.xic_params = Some(x);
                                draft(
                                    s,
                                    Operation::LinkedSpectralChromatogram {
                                        query: Box::new(q.clone()),
                                        params: p,
                                    },
                                );
                            }
                            Err(e) => s.message = e.to_string(),
                        }
                    }
                }
            }
        });
    }
    {
        super::forms::typed::<Operation>(
            ui,
            "Prepared operation settings and review evidence",
            &mut s.draft,
        );
        ui.collapsing(
            "Advanced operation JSON (all parameters and annotation evidence)",
            |ui| editor(ui, &mut s.draft),
        );

        if ui
            .add_enabled(
                s.pending.is_none() && !s.draft.is_empty(),
                egui::Button::new("Execute and retain engine result"),
            )
            .clicked()
        {
            match serde_json::from_str::<Operation>(&s.draft) {
                Ok(operation) => {
                    let request = Request {
                        version: 1,
                        operation_id: Default::default(),
                        actor: "desktop-spectral".into(),
                        operation,
                    };
                    let path = path.clone().unwrap_or_else(|| PathBuf::from("-"));
                    let (tx, rx) = mpsc::channel();
                    s.pending = Some(rx);
                    std::thread::spawn(move || {
                        let _lease = lease;
                        let _ = tx.send(crate::engine::execute(
                            &path,
                            request,
                            &crate::jobs::JobControl::default(),
                        ));
                    });
                }
                Err(e) => s.message = e.to_string(),
            }
        }
    }
    ui.label(&s.message);

    ui.horizontal(|ui| {
        if ui
            .button("Save full evidence history JSON (new file)")
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("spectral-history.json")
                .save_file()
            {
                use std::io::Write;
                let result = serde_json::to_vec_pretty(&s.history)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| {
                        std::fs::OpenOptions::new()
                            .create_new(true)
                            .write(true)
                            .open(path)
                            .and_then(|mut f| f.write_all(&bytes))
                            .map_err(|e| e.to_string())
                    });
                s.message = result.err().unwrap_or_else(|| "History saved".into());
            }
        }

        if ui.button("Load evidence history JSON").clicked() {
            if let Some(path) = rfd::FileDialog::new().pick_file() {
                let result = (|| {
                    if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 64 * 1024 * 1024
                    {
                        return Err("History exceeds 64 MiB".into());
                    }
                    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                    let history: Vec<Response> =
                        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                    for r in &history {
                        verify(r).map_err(|e| e.to_string())?;
                    }
                    Ok::<_, String>(history)
                })();
                match result {
                    Ok(history) => {
                        s.history.extend(history);
                        s.selected = s.history.len().saturating_sub(1);
                    }
                    Err(e) => s.message = e,
                }
            }
        }
    });

    egui::ComboBox::from_label("Retained result")
        .selected_text(s.selected.to_string())
        .show_ui(ui, |ui| {
            for (i, r) in s.history.iter().enumerate() {
                ui.selectable_value(&mut s.selected, i, format!("{i}: {}", r.result_id.0));
            }
        });

    let mut annotation_op = None;

    if let Some(response) = s.history.get(s.selected) {
        ui.label(format!(
            "{} | source SHA256 {:?}",
            response.kernel_version, response.source_sha256
        ));
        match &response.output {
            Output::SpectralComparison {
                query,
                reference,
                similarity,
                ..
            } => {
                overlay(ui, query, Some(reference));
                ui.label(format!(
                    "Cosine {}, {} matched fragments",
                    similarity.cosine,
                    similarity.matches.len()
                ));
            }
            Output::SpectralProcessing { processed } => {
                ui.checkbox(
                    &mut s.show_raw,
                    "Inspect first original spectrum (profile/centroid)",
                );
                let shown = if s.show_raw {
                    &processed.inputs[0]
                } else {
                    &processed.spectrum
                };
                ui.label(format!(
                    "MS{} | {:?} | RT {:?} minute | {} peaks",
                    shown.ms_level,
                    shown.representation,
                    shown.rt_minutes,
                    shown.peaks.len()
                ));
                overlay(ui, shown, None);
                for warning in &processed.warnings {
                    ui.label(warning);
                }
                ui.collapsing("Raw acquisition, isolation and processing evidence", |ui| {
                    ui.label(serde_json::to_string_pretty(processed).unwrap_or_default());
                });
            }
            Output::SpectralLibrary { library } => {
                ui.label(format!(
                    "{} entries, {} indexed; {} / {} / license {}; SHA256 {}",
                    library.entries.len(),
                    library.precursor_index.len(),
                    library.source.name,
                    library.source.version,
                    library.source.license,
                    library.sha256
                ));
                for warning in library.warnings.iter().take(20) {
                    ui.label(warning);
                }
                let rows: Vec<_> = library
                    .entries
                    .iter()
                    .map(|entry| super::table::Row {
                        key: entry.accession.clone(),
                        cells: vec![
                            super::table::Cell::text(&entry.accession),
                            super::table::Cell::text(&entry.name),
                            super::table::Cell::text(
                                entry.formula.as_deref().unwrap_or("unavailable"),
                            ),
                            super::table::Cell::number(entry.spectrum.precursor_mz),
                        ],
                    })
                    .collect();
                if let Some(index) = super::table::show(
                    ui,
                    "library_catalog",
                    &["Accession", "Name", "Formula", "Precursor m/z (Th)"],
                    &rows,
                    rows.get(s.candidate).map(|row| row.key.as_str()),
                ) {
                    s.candidate = index;
                }
                if let Some(entry) = library.entries.get(s.candidate) {
                    overlay(ui, &entry.spectrum, None);
                    ui.label(format!(
                        "{} | {} | {:?}",
                        entry.accession, entry.name, entry.structure
                    ));
                }
            }
            Output::SpectralSearch { report } => {
                ui.label(format!("Current annotation confidence: {:?}. Similarity is not identification confidence.", report.annotations.last().map(|a|a.confidence.clone()).unwrap_or(Confidence::Unknown)));
                for warning in &report.warnings {
                    ui.label(warning);
                }
                ui.label(format!(
                    "Excluded compatibility/quality: {:?}",
                    report.excluded
                ));

                use super::table::{Cell, Row};
                let rows: Vec<_> = report
                    .candidates
                    .iter()
                    .map(|candidate| Row {
                        key: candidate.accession.clone(),
                        cells: vec![
                            Cell::text(&candidate.name),
                            Cell::number(Some(candidate.similarity.cosine)),
                            Cell::number(Some(candidate.similarity.matches.len() as f64)),
                            Cell::number(Some(candidate.precursor_error_ppm)),
                            Cell::text(candidate.warnings.join("; ")),
                        ],
                    })
                    .collect();
                if let Some(index) = super::table::show(
                    ui,
                    "spectral_candidates",
                    &[
                        "Candidate",
                        "Cosine",
                        "Fragments",
                        "Precursor error (ppm)",
                        "Warnings",
                    ],
                    &rows,
                    rows.get(s.candidate).map(|row| row.key.as_str()),
                ) {
                    s.candidate = index;
                }

                if let Some(candidate) = report.candidates.get(s.candidate) {
                    let entry = &report.library.entries[candidate.entry_index];
                    overlay(ui, &report.query.spectrum, Some(&candidate.reference));
                    ui.label(format!("Formula {:?}; structure {:?}; precursor {:?}; instrument {:?}; energy {:?}",entry.formula,entry.structure,entry.spectrum.precursor_type,entry.spectrum.instrument,entry.spectrum.collision_energy));
                    ui.collapsing("Annotated fragment matches", |ui| {
                        super::table::records(
                            ui,
                            "spectral-fragments",
                            serde_json::to_value(&candidate.similarity.matches).unwrap(),
                            "query_mz",
                            None,
                        );
                    });
                    if ui
                        .button("Prepare reasoned annotation for selected candidate")
                        .clicked()
                    {
                        annotation_op = Some(Operation::AnnotateSpectrum {
                            report: report.clone(),
                            expected_revision: report.annotations.len(),
                            annotation: Annotation {
                                id: uuid::Uuid::nil(),
                                actor: String::new(),
                                reason: String::new(),
                                unix_ms: 0,
                                candidate_accession: Some(candidate.accession.clone()),
                                label: candidate.name.clone(),
                                confidence: Confidence::Unknown,
                                evidence: vec![],
                            },
                        });
                    }
                }

                if ui
                    .button("Prepare unknown or compound-class annotation")
                    .clicked()
                {
                    annotation_op = Some(Operation::AnnotateSpectrum {
                        report: report.clone(),
                        expected_revision: report.annotations.len(),
                        annotation: Annotation {
                            id: uuid::Uuid::nil(),
                            actor: String::new(),
                            reason: String::new(),
                            unix_ms: 0,
                            candidate_accession: None,
                            label: "Unknown feature".into(),
                            confidence: Confidence::Unknown,
                            evidence: vec![],
                        },
                    });
                }
                for a in &report.annotations {
                    ui.label(format!(
                        "{:?}: {} / {} / {}",
                        a.confidence, a.label, a.actor, a.reason
                    ));
                }
            }
            Output::Chromatogram { points, .. } => {
                super::plot_controls::export(ui, points, "RT (min)", "Instrument intensity");
                super::plot_controls::plot(ui, "spectral_xic")
                    .height(220.0)
                    .x_axis_label("RT (minute)")
                    .y_axis_label("instrument intensity")
                    .show(ui, |plot| {
                        plot.line(Line::new("Linked XIC", points.clone()))
                    });
            }
            Output::FormulaCandidates { report } => {
                ui.label("Formula and adduct hypotheses remain tentative; mass agreement alone does not establish identity.");
                for warning in &report.warnings {
                    ui.label(warning);
                }
                let rows: Vec<_> = report
                    .candidates
                    .iter()
                    .map(|candidate| super::table::Row {
                        key: format!("{}:{}", candidate.formula, candidate.adduct),
                        cells: vec![
                            super::table::Cell::text(&candidate.formula),
                            super::table::Cell::text(&candidate.adduct),
                            super::table::Cell::number(Some(candidate.predicted_mz)),
                            super::table::Cell::number(Some(candidate.error_ppm)),
                            super::table::Cell::number(Some(candidate.neutral_mass_da)),
                            super::table::Cell::number(Some(candidate.dbe)),
                        ],
                    })
                    .collect();
                if let Some(index) = super::table::show(
                    ui,
                    "formula_candidates",
                    &[
                        "Formula",
                        "Adduct",
                        "Predicted m/z (Th)",
                        "Error (ppm)",
                        "Neutral mass (Da)",
                        "DBE",
                    ],
                    &rows,
                    rows.get(s.candidate).map(|row| row.key.as_str()),
                ) {
                    s.candidate = index;
                }
                if let Some(candidate) = report.candidates.get(s.candidate) {
                    ui.label(format!(
                        "Selected hypothesis: {} {} | observed m/z {} Th | tolerance {} {:?}",
                        candidate.formula,
                        candidate.adduct,
                        report.config.observed_mz,
                        report.config.tolerance.value,
                        report.config.tolerance.unit
                    ));
                }
                ui.collapsing("Retained adduct evidence", |ui| {
                    ui.label(
                        serde_json::to_string_pretty(&report.adduct_hypotheses).unwrap_or_default(),
                    );
                });
            }
            Output::IsotopeAnalysis { report } => {
                for warning in &report.warnings {
                    ui.label(warning);
                }
                overlay(ui, &report.spectrum, None);
                let rows: Vec<_> = report
                    .hypotheses
                    .iter()
                    .map(|hypothesis| super::table::Row {
                        key: hypothesis.charge.to_string(),
                        cells: vec![
                            super::table::Cell::number(Some(hypothesis.charge as f64)),
                            super::table::Cell::number(Some(hypothesis.spacing_mz)),
                            super::table::Cell::number(hypothesis.carbon_estimate),
                            super::table::Cell::number(Some(hypothesis.peaks.len() as f64)),
                        ],
                    })
                    .collect();
                if let Some(index) = super::table::show(
                    ui,
                    "isotope_hypotheses",
                    &["Charge", "Spacing (Th)", "Carbon estimate", "Peaks"],
                    &rows,
                    rows.get(s.candidate).map(|row| row.key.as_str()),
                ) {
                    s.candidate = index;
                }
                if let Some(hypothesis) = report.hypotheses.get(s.candidate) {
                    ui.label(format!(
                        "Selected charge {} | isotope ratios to monoisotopic peak: {:?}",
                        hypothesis.charge, hypothesis.ratios_to_mono
                    ));
                }
                ui.collapsing("Retained nominal isotope evidence", |ui| {
                    ui.label(
                        serde_json::to_string_pretty(&report.nominal_comparisons)
                            .unwrap_or_default(),
                    );
                });
            }
            _ => {
                ui.collapsing("Retained analytical evidence", |ui| {
                    ui.label(serde_json::to_string_pretty(&response.output).unwrap_or_default());
                });
            }
        }
    }

    if let Some(op) = annotation_op {
        draft(s, op);
    }
}
fn overlay(ui: &mut egui::Ui, q: &Spectrum, reference: Option<&Spectrum>) {
    if q.representation == Representation::Profile {
        super::plot_controls::export(ui, &q.peaks, "m/z (Th)", "Profile intensity");
    } else {
        super::plot_controls::export_spectrum(ui, &q.peaks, "Centroid intensity");
    }
    let sticks = |s: &Spectrum, sign: f64| {
        let max = s.peaks.iter().map(|p| p[1].abs()).fold(0.0, f64::max);
        if s.representation == Representation::Profile {
            return s
                .peaks
                .iter()
                .map(|p| [p[0], if max > 0.0 { sign * p[1] / max } else { 0.0 }])
                .collect::<Vec<_>>();
        }
        s.peaks
            .iter()
            .flat_map(|p| {
                [
                    [p[0], 0.0],
                    [p[0], if max > 0.0 { sign * p[1] / max } else { 0.0 }],
                    [p[0], 0.0],
                ]
            })
            .collect::<Vec<_>>()
    };
    super::plot_controls::plot(ui, "spectral_overlay")
        .height(240.0)
        .x_axis_label("m/z")
        .y_axis_label("relative intensity (query + / reference -)")
        .show(ui, |plot| {
            plot.line(Line::new("Query", sticks(q, 1.0)));
            if let Some(r) = reference {
                plot.line(Line::new("Reference", sticks(r, -1.0)));
            }
        });
}
impl State {
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        app: &mut super::super::MzViewerApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 2000.0),
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
        label: &str,
        frames: &mut Vec<egui::FullOutput>,
    ) {
        let pos = super::super::test_render::scroll_to_text(label, frames, |events| {
            frame(app, ctx, events)
        });
        frames.push(frame(
            app,
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
            app,
            ctx,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
        ));
    }
    fn execute(operation: Operation) -> Response {
        crate::engine::execute(
            std::path::Path::new("-"),
            Request {
                version: 1,
                operation_id: Default::default(),
                actor: "gui-test".into(),
                operation,
            },
            &crate::jobs::JobControl::default(),
        )
        .unwrap()
    }
    #[test]
    fn desktop_search_overlay_and_annotation_keep_original_history() {
        let lib = import_library(
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/reference/MSBNK-Antwerp_Univ-AN111301.txt"
            ))
            .into(),
            "massbank".into(),
            LibrarySource {
                name: "MassBank Antwerp".into(),
                version: "2026.03".into(),
                url: "https://github.com/MassBank/MassBank-data".into(),
                license: "CC BY".into(),
            },
        )
        .unwrap();
        let processed = execute(Operation::ProcessSpectra {
            spectra: vec![lib.entries[0].spectrum.clone()],
            background: vec![],
            config: Processing::default(),
        });
        let imported = execute(Operation::ImportSpectralLibrary {
            text: lib.raw_text.clone(),
            format: lib.format.clone(),
            source: lib.source.clone(),
        });
        let mut app = super::super::MzViewerApp::default();
        app.spectral.open = true;
        app.spectral.history = vec![processed, imported];
        let ctx = egui::Context::default();
        ctx.style_mut(|s| s.animation_time = 0.0);
        let mut frames = vec![frame(&mut app, &ctx, vec![])];
        click(&mut app, &ctx, "Prepare library search", &mut frames);
        assert!(matches!(
            serde_json::from_str::<Operation>(&app.spectral.draft).unwrap(),
            Operation::SearchSpectralLibrary { .. }
        ));
        click(
            &mut app,
            &ctx,
            "Execute and retain engine result",
            &mut frames,
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.spectral.pending.is_some() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
            frames.push(frame(&mut app, &ctx, vec![]));
        }
        assert_eq!(app.spectral.history.len(), 3, "{}", app.spectral.message);
        if let Output::SpectralSearch { report } = &app.spectral.history[2].output {
            assert_eq!(report.candidates.len(), 1);
            assert!(report.annotations.is_empty());
        } else {
            panic!()
        }
        click(
            &mut app,
            &ctx,
            "Prepare reasoned annotation for selected candidate",
            &mut frames,
        );
        let mut operation: Operation = serde_json::from_str(&app.spectral.draft).unwrap();
        if let Operation::AnnotateSpectrum { annotation, .. } = &mut operation {
            annotation.reason = "Reviewed reference; additional standard evidence absent".into();
        } else {
            panic!()
        };
        draft(&mut app.spectral, operation);
        click(
            &mut app,
            &ctx,
            "Execute and retain engine result",
            &mut frames,
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.spectral.pending.is_some() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
            frames.push(frame(&mut app, &ctx, vec![]));
        }
        assert_eq!(app.spectral.history.len(), 4, "{}", app.spectral.message);
        if let Output::SpectralSearch { report } = &app.spectral.history[3].output {
            assert_eq!(report.annotations[0].confidence, Confidence::Unknown);
            verify_report(report).unwrap();
        } else {
            panic!()
        };
        if let Output::SpectralSearch { report } = &app.spectral.history[2].output {
            assert!(report.annotations.is_empty());
        }
        let saved = serde_json::to_vec(&app.spectral.history).unwrap();
        let restored: Vec<Response> = serde_json::from_slice(&saved).unwrap();
        for r in &restored {
            verify(r).unwrap();
        }
        if std::env::var_os("CHROMASCOPE_SPECTRAL_PREVIEW").is_some() {
            super::super::test_render::save(
                &ctx,
                frames,
                std::path::Path::new("target/spectral-review.png"),
                egui::vec2(1600.0, 2000.0),
            );
        }
    }
}
