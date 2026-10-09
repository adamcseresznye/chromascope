//! Candidate comparison and reasoned, reversible review using the shared ledger.
use crate::annotation::{Decision, Hypothesis, Ledger};
use eframe::egui;
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(super) struct State {
    history: Vec<Ledger>,
    selected: usize,
    draft: String,
    reason: String,
    proposal: String,
    message: String,
    #[serde(skip)]
    pending: Option<(
        usize,
        std::sync::mpsc::Receiver<crate::domain::Result<Ledger>>,
    )>,
}
impl State {
    pub(super) fn retain(&mut self, ledger: Ledger) -> Result<(), String> {
        crate::annotation::verify(&ledger).map_err(|e| e.to_string())?;
        self.history.push(ledger);
        self.selected = self.history.len() - 1;
        Ok(())
    }
}
pub(super) fn show(
    ui: &mut egui::Ui,
    state: &mut State,
    matrix: &crate::untargeted::Report,
    feature: usize,
    sample: usize,
) {
    if let Some((origin, rx)) = &state.pending {
        match rx.try_recv() {
            Ok(result) => {
                let origin = *origin;
                state.pending = None;
                match result {
                    Ok(l) => {
                        state.history.push(l);
                        if state.selected == origin {
                            state.selected = state.history.len() - 1;
                        }
                        state.message = "Computed evidence retained as a new revision".into()
                    }
                    Err(e) => state.message = e.to_string(),
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ui
                .ctx()
                .request_repaint_after(std::time::Duration::from_millis(100)),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                state.pending = None;
                state.message = "Annotation worker disconnected".into()
            }
        }
    }
    ui.collapsing("Metabolite / lipid hypotheses and review", |ui| {
        ui.label("All identities are tentative. Accept retains a hypothesis; it does not confirm a compound. Conflicting candidates remain visible.");
        if ui.button("Start annotation ledger from this matrix").clicked() {
            state.history.push(Ledger {version:1,matrix:Box::new(matrix.clone()),hypotheses:vec![],reviews:vec![],adduct_relationships:vec![]});
            state.selected=state.history.len()-1;
        }
        if ui.button("Prepare hypothesis for selected feature/sample").clicked() {
            if let Some(f)=matrix.features.get(feature) { if let Some(c)=f.cells.get(sample) {
                let h=Hypothesis {id:uuid::Uuid::new_v4(),feature_id:f.id.clone(),sample_id:c.sample_id.clone(),label:"Enter tentative candidate label".into(),structure:None,lipid:None,formula:None,formula_candidate:None,isotope:None,msms:None,accession:None,confidence:crate::spectral::Confidence::Unknown,evidence:vec![]};
                state.draft=serde_json::to_string_pretty(&h).unwrap();
            }}
        }
        if ui.button("Prepare formula/isotope/library search parameters").clicked() {
            if let Some(f)=matrix.features.get(feature) { if let Some(c)=f.cells.get(sample) {
                let config=crate::annotation::ProposalConfig {feature_id:f.id.clone(),sample_id:c.sample_id.clone(),formula:crate::spectral::FormulaConfig { observed_mz:c.mz.unwrap_or(f.mz),polarity:matrix.config.polarity,adducts:vec![if matrix.config.polarity==crate::spectral::Polarity::Negative {"[M-H]-".into()}else{"[M+H]+".into()}],tolerance:crate::spectral::Tolerance {value:5.,unit:crate::spectral::MassUnit::Ppm},maximum_atoms:[("C".into(),60),("H".into(),120),("N".into(),4),("O".into(),20),("P".into(),2),("S".into(),2)].into(),maximum_candidates:100},library:None,search:Default::default(),ms2_index:0};
                state.proposal=serde_json::to_string_pretty(&config).unwrap();
            }}
        }
        super::forms::typed::<crate::annotation::ProposalConfig>(ui,"Formula, isotope and library settings",&mut state.proposal);
        super::forms::typed::<Hypothesis>(ui,"Candidate identity, confidence and lipid evidence",&mut state.draft);
        ui.collapsing("Formula bounds, adduct hypotheses and optional imported library",|ui| {ui.add(egui::TextEdit::multiline(&mut state.proposal).code_editor().desired_rows(12).desired_width(f32::INFINITY));});
        if ui.button("Use retained spectral library JSON for identification").clicked() {if let Some(path)=rfd::FileDialog::new().pick_file() {
            let result=(|| {let bytes=std::fs::read(path).map_err(|e|e.to_string())?;let v:serde_json::Value=serde_json::from_slice(&bytes).map_err(|e|e.to_string())?;let response:crate::engine::Response=serde_json::from_value(v.get("result").cloned().unwrap_or(v)).map_err(|e|e.to_string())?;crate::spectral::verify_response(&response).map_err(|e|e.to_string())?;
                let crate::engine::Output::SpectralLibrary {library}=response.output else {return Err("Select a retained imported spectral-library response".into());};let mut config:crate::annotation::ProposalConfig=serde_json::from_str(&state.proposal).map_err(|e|e.to_string())?;config.library=Some(library);state.proposal=serde_json::to_string_pretty(&config).map_err(|e|e.to_string())?;Ok::<_,String>(())})();state.message=result.err().unwrap_or_else(||"Reproducible local library adapter configured".into());
        }}
        ui.label("Paste retained formula/isotope or library-search evidence into candidate JSON. Evidence must reference the selected native spectra and include source citations. Lipid chains [] mean sum composition; unordered chains use underscores.");
        ui.collapsing("Advanced hypothesis JSON", |ui| { ui.add(egui::TextEdit::multiline(&mut state.draft).code_editor().desired_rows(12).desired_width(f32::INFINITY)); });
        egui::ComboBox::from_label("Annotation revision").selected_text(format!("{}",state.selected+1)).show_ui(ui, |ui| {
            for (i,l) in state.history.iter().enumerate() { ui.selectable_value(&mut state.selected,i,format!("{}: {} candidates, {} reviews",i+1,l.hypotheses.len(),l.reviews.len())); }
        });
        let mut next=None;
        if let Some(ledger)=state.history.get(state.selected) {
            if ui.add_enabled(state.pending.is_none(),egui::Button::new("Compute tentative formula / isotope / library hypotheses")).clicked() {
                match serde_json::from_str::<crate::annotation::ProposalConfig>(&state.proposal) {
                    Ok(config)=>{let ledger=ledger.clone();let (tx,rx)=std::sync::mpsc::channel();state.pending=Some((state.selected,rx));std::thread::spawn(move||{let result=crate::annotation::propose(&ledger,&config,ledger.hypotheses.len()+ledger.reviews.len());let _=tx.send(result);});},
                    Err(e)=>state.message=e.to_string(),
                }
            }
            if ui.button("Validate and retain competing hypothesis").clicked() {
                let result=serde_json::from_str::<Hypothesis>(&state.draft).map_err(|e|e.to_string()).and_then(|h|crate::annotation::add(ledger,h,ledger.hypotheses.len()+ledger.reviews.len()).map_err(|e|e.to_string()));
                match result {Ok(l)=>next=Some(l),Err(e)=>state.message=e}
            }
            ui.horizontal(|ui| { ui.label("Review reason");ui.text_edit_singleline(&mut state.reason); });
            ui.collapsing("Tentative adduct relationships",|ui| {ui.label(serde_json::to_string_pretty(&ledger.adduct_relationships).unwrap());});
            for h in &ledger.hypotheses {
                ui.separator();
                if ui.button(format!("Prepare competing revision {}",h.id)).clicked() {let mut candidate=h.clone();candidate.id=uuid::Uuid::new_v4();state.draft=serde_json::to_string_pretty(&candidate).unwrap();}
                let status=ledger.reviews.iter().rev().find(|r|r.hypothesis_id==h.id).map(|r|format!("{:?}",r.decision)).unwrap_or_else(||"Unreviewed".into());
                ui.label(format!("{} • {} / {} • {:?} • {}",h.label,h.feature_id,h.sample_id,h.confidence,status));
                if let Some(lipid)=&h.lipid {ui.label(if lipid.chains.is_empty() {"Sum composition: chains, sn positions, double-bond positions and stereochemistry unresolved"}else{"Molecular species: sn positions, double-bond positions and stereochemistry unresolved"});}
                ui.collapsing(format!("Evidence {}",h.id),|ui| {
                    if let Some(formula)=&h.formula {if let Some(index)=h.formula_candidate {let f=&formula.candidates[index];ui.label(format!("{} {}: mass error {:.4} ppm; neutral mass {:.8} Da",f.formula,f.adduct,f.error_ppm,f.neutral_mass_da));}}
                    if let Some(search)=&h.msms {if let Some(candidate)=search.candidates.iter().find(|c|Some(&c.accession)==h.accession.as_ref()) {
                        ui.label(format!("Cosine {:.4}; {} fragments matched; source {} {} accession {}",candidate.similarity.cosine,candidate.similarity.matches.len(),search.library.source.name,search.library.source.version,candidate.accession));
                        super::plot_controls::plot(ui,format!("annotation-mirror-{}",h.id)).height(180.).x_axis_label("m/z (Th)").y_axis_label("MS/MS intensity (native units; reference mirrored)").show(ui,|plot| {
                            let sticks=|peaks:&[[f64;2]],sign:f64|peaks.iter().flat_map(|p|[[p[0],0.],[p[0],p[1]*sign],[p[0],0.]]).collect::<Vec<_>>();
                            plot.line(egui_plot::Line::new("Observed",sticks(&search.query.spectrum.peaks,1.)));
                            plot.line(egui_plot::Line::new("Reference",sticks(&candidate.reference.peaks,-1.)));
                        });
                    }}
                    ui.label(serde_json::to_string_pretty(h).unwrap());
                    if let Some(f)=ledger.matrix.features.iter().find(|f|f.id==h.feature_id) { if let Some(c)=f.cells.iter().find(|c|c.sample_id==h.sample_id) {ui.label(serde_json::to_string_pretty(c).unwrap());}}
                });
                ui.horizontal(|ui| {for (label,decision) in [("Accept tentative",Decision::Accept),("Reject",Decision::Reject),("Reopen / undo",Decision::Reopen)] {
                    if ui.button(label).clicked() {match crate::annotation::review(ledger,h.id,ledger.hypotheses.len()+ledger.reviews.len(),"desktop",&state.reason,decision) {Ok(l)=>next=Some(l),Err(e)=>state.message=e.to_string()}}
                }});
            }
            ui.horizontal(|ui| {for (label,json) in [("Save annotation evidence JSON",true),("Export annotations CSV",false)] {
                if ui.button(label).clicked() {if let Some(path)=rfd::FileDialog::new().save_file() {
                    let result=(|| {crate::annotation::verify(ledger).map_err(|e|e.to_string())?;let bytes=if json {serde_json::to_vec_pretty(ledger).map_err(|e|e.to_string())?}else{crate::annotation::csv(ledger).map_err(|e|e.to_string())?.into_bytes()};
                        use std::io::Write;std::fs::OpenOptions::new().create_new(true).write(true).open(path).and_then(|mut f|f.write_all(&bytes)).map_err(|e|e.to_string())})();state.message=result.err().unwrap_or_else(||"Saved new annotation artifact".into());
                }}
            }});
        }
        if let Some(l)=next {state.history.push(l);state.selected=state.history.len()-1;state.message="Retained new revision; original evidence preserved".into();}
        if ui.button("Load annotation evidence JSON").clicked() {if let Some(path)=rfd::FileDialog::new().pick_file() {
            let result=(|| {let bytes=std::fs::read(path).map_err(|e|e.to_string())?;let l:Ledger=serde_json::from_slice(&bytes).map_err(|e|e.to_string())?;crate::annotation::verify(&l).map_err(|e|e.to_string())?;Ok::<_,String>(l)})();
            match result {Ok(l)=>{state.history.push(l);state.selected=state.history.len()-1},Err(e)=>state.message=e}
        }}
        ui.label(&state.message);
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
        ctx: &egui::Context,
        state: &mut State,
        matrix: &crate::untargeted::Report,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1700., 2000.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| show(ui, state, matrix, 0, 0));
            },
        )
    }
    #[test]
    fn evidence_review_clicks_retain_unknown_and_reversible_history() {
        let response = crate::gui::untargeted::tests::response();
        let crate::engine::Output::FeatureMatrix { report: matrix } = response.output else {
            panic!()
        };
        let h:Hypothesis=serde_json::from_value(serde_json::json!({"id":uuid::Uuid::new_v4(),"feature_id":"synthetic-triangle","sample_id":"sample-a","label":"Unresolved isomer","structure":null,"lipid":null,"formula":null,"formula_candidate":null,"isotope":null,"msms":null,"accession":null,"confidence":"unknown","evidence":[{"description":"No structural evidence","reference":"local:synthetic","same_method_standard":false,"rt_match":false,"diagnostic_fragments":false,"resolves_alternatives":false}]})).unwrap();
        let original = Ledger {
            version: 1,
            matrix: matrix.clone(),
            hypotheses: vec![],
            reviews: vec![],
            adduct_relationships: vec![],
        };
        let mut state = State {
            history: vec![crate::annotation::add(&original, h, 0).unwrap()],
            reason: "Keep as tentative pending more evidence".into(),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        ctx.style_mut(|s| s.animation_time = 0.);
        let mut output = frame(&ctx, &mut state, &matrix, vec![]);
        let mut frames = vec![output.clone()];
        for label in [
            "Metabolite / lipid hypotheses and review",
            "Accept tentative",
            "Reject",
            "Reopen / undo",
        ] {
            output = frame(&ctx, &mut state, &matrix, vec![]);
            frames.push(output.clone());
            let pos = crate::gui::test_render::text_center(&output.shapes, label)
                .unwrap_or_else(|| panic!("Missing {label}"));
            for pressed in [true, false] {
                output = frame(
                    &ctx,
                    &mut state,
                    &matrix,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                );
                frames.push(output.clone());
            }
        }
        assert_eq!(state.history.len(), 4);
        assert!(state.history[0].reviews.is_empty());
        let latest = state.history.last().unwrap();
        assert_eq!(latest.reviews.len(), 3);
        assert_eq!(
            latest.hypotheses[0].confidence,
            crate::spectral::Confidence::Unknown
        );
        assert_eq!(latest.reviews.last().unwrap().decision, Decision::Reopen);
        frames.push(frame(&ctx, &mut state, &matrix, vec![]));
        if std::env::var_os("CHROMASCOPE_ANNOTATION_PREVIEW").is_some() {
            crate::gui::test_render::save(
                &ctx,
                frames,
                std::path::Path::new("target/annotation-review.png"),
                egui::vec2(1700., 2000.),
            );
        }
    }
}
