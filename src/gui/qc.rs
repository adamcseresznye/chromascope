//! QC presentation adapter; all decisions are computed by the shared engine.
use crate::qc;
use eframe::egui;
use egui_plot::{HLine, Line, Points};
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(super) struct State {
    draft: String,
    reports: Vec<qc::Report>,
    selected: usize,
    message: String,
    reason: String,
    draft_evidence: Option<Box<crate::targeted::BatchResult>>,
}
fn save(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)
}
impl State {
    pub(super) fn retain(&mut self, report: qc::Report) -> Result<(), String> {
        qc::verify(&report).map_err(|e| e.to_string())?;
        self.reports.push(report);
        self.selected = self.reports.len() - 1;
        Ok(())
    }
}
fn execute(operation: crate::domain::Operation) -> Result<qc::Report, String> {
    let request = crate::domain::Request {
        version: 1,
        operation_id: Default::default(),
        actor: "desktop-qc".into(),
        operation,
    };
    let response = crate::engine::execute(
        std::path::Path::new("-"),
        request,
        &crate::jobs::JobControl::default(),
    )
    .map_err(|e| e.to_string())?;
    match response.output {
        crate::engine::Output::QcReport { report } => Ok(*report),
        _ => Err("Unexpected engine output".into()),
    }
}
fn plotted_value(o: &qc::Observation, metric: &qc::Metric) -> Option<f64> {
    use qc::Metric::*;
    match metric {
        RtDrift | RtDriftSlope => Some(o.rt_minutes? - o.expected_rt_minutes?),
        MassError | MassDriftSlope => o.mass_error_ppm,
        InternalStandardCv | InternalStandardSlope => o.is_response,
        Blank
        | Carryover
        | Recovery
        | MatrixFactor
        | ProcessEfficiency
        | Selectivity
        | Stability
        | CalibrationR2
        | CalibrationRmse
        | CalibrationAccuracyDeviation => o.response,
        Accuracy | DilutionIntegrity | MaximumAccuracyDeviation => {
            Some(o.value? / o.nominal.filter(|v| *v > 0.0)? * 100.0)
        }
        _ => o.value,
    }
}
fn dashboard(
    report: &qc::Report,
    reason: &mut String,
    ui: &mut egui::Ui,
) -> Option<(String, bool)> {
    ui.colored_label(
        match report.status {
            qc::Status::Pass => super::plot_controls::scientific_color(ui.visuals().dark_mode, 3),
            qc::Status::Fail => super::plot_controls::scientific_color(ui.visuals().dark_mode, 4),
            qc::Status::Indeterminate => {
                super::plot_controls::scientific_color(ui.visuals().dark_mode, 2)
            }
        },
        egui::RichText::new(format!(
            "{} status: {:?}",
            if report.study.purpose == "method_validation" {
                "Method validation"
            } else {
                "Batch"
            },
            report.status
        ))
        .heading(),
    );
    ui.label(format!(
        "{} decisions; {} pending review items; {} retained review events",
        report.decisions.len(),
        report.review_queue.len(),
        report.reviews.len()
    ));
    ui.label(&report.acceptance.reason);
    ui.collapsing("Batch acceptance calculation", |ui| {
        ui.label(&report.acceptance.calculation);
        ui.label(&report.acceptance.threshold);
        ui.label(format!(
            "Required rule evidence: {}",
            report.acceptance.required_rules.join(", ")
        ));
    });
    ui.label("Review reason (acknowledgement does not change analytical acceptance)");
    ui.text_edit_singleline(reason);
    let mut review = None;
    ui.collapsing("Review queue", |ui| {
        for id in &report.review_queue {
            let d = report.decisions.iter().find(|d| &d.rule.id == id).unwrap();
            ui.label(format!("{}: {:?} — {}", d.rule.id, d.status, d.reason));
        }
    });
    let embedded = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(egui::Id::new("central_workspaces")))
        .unwrap_or(false);
    let selection_id = ui.make_persistent_id(("qc_rule_selection", report.report_id));
    let mut selected = ui
        .ctx()
        .data(|d| d.get_temp::<String>(selection_id))
        .unwrap_or_default();
    if embedded {
        use super::table::{Cell, Row};
        let rows: Vec<_> = report
            .decisions
            .iter()
            .map(|d| Row {
                key: d.rule.id.clone(),
                cells: vec![
                    Cell::text(&d.rule.id),
                    Cell::text(&d.rule.target),
                    Cell::text(format!("{:?}", d.status)),
                    Cell::number(d.value),
                    Cell::text(&d.unit),
                    Cell::number(Some(d.n as f64)),
                    Cell::text(&d.reason),
                ],
            })
            .collect();
        if let Some(index) = super::table::show(
            ui,
            "qc-rule-decisions",
            &["Rule", "Target", "Status", "Value", "Unit", "n", "Reason"],
            &rows,
            Some(&selected),
        ) {
            selected = rows[index].key.clone();
        }
        if selected.is_empty() {
            if let Some(row) = rows.first() {
                selected = row.key.clone();
            }
        }
        ui.ctx()
            .data_mut(|d| d.insert_temp(selection_id, selected.clone()));
    }
    for d in &report.decisions {
        if embedded && d.rule.id != selected {
            continue;
        }
        ui.push_id(&d.rule.id, |ui| {
            ui.collapsing(format!("{} / {}: {:?} = {:?} {}",d.rule.id,d.rule.target,d.status,d.value,d.unit), |ui| {
                ui.label(format!("{}; limits [{}, {}], n={}, required={}",d.calculation,d.rule.lower,d.rule.upper,d.n,d.rule.required));
                ui.label(&d.reason);
                ui.horizontal(|ui| {
                    if ui.add_enabled(!reason.trim().is_empty(),egui::Button::new("Acknowledge review")).clicked(){review=Some((d.rule.id.clone(),true));}
                    if ui.add_enabled(!reason.trim().is_empty(),egui::Button::new("Reopen review")).clicked(){review=Some((d.rule.id.clone(),false));}
                });
                let observations: Vec<_>=report.study.observations.iter().filter(|o|d.evidence.contains(&o.id)).collect();
                ui.label("Injection-order observations; aggregate CV/ratio thresholds apply to the decision, not raw plotted values.");
                use qc::Metric::*;
                let plot_unit=match d.rule.metric {Accuracy|DilutionIntegrity|MaximumAccuracyDeviation=>"% accuracy",RtDrift|RtDriftSlope=>"minute",MassError|MassDriftSlope=>"ppm",InternalStandardCv|InternalStandardSlope=>"instrument intensity * minute",Blank|Carryover|Recovery|MatrixFactor|ProcessEfficiency|Selectivity|Stability|CalibrationR2|CalibrationRmse|CalibrationAccuracyDeviation=>observations.first().map(|o|o.response_unit.as_str()).unwrap_or("response"),_=>observations.first().map(|o|o.unit.as_str()).unwrap_or("value")};
                let groups:std::collections::BTreeSet<_>=observations.iter().map(|o|(&o.batch,&o.group)).collect();
                let series:Vec<_>=groups.into_iter().map(|(batch,group)|{let mut points:Vec<_>=observations.iter().filter(|o|&o.batch==batch&&&o.group==group).filter_map(|o|Some([o.order as f64,plotted_value(o,&d.rule.metric)?])).collect();points.sort_by(|a,b|a[0].total_cmp(&b[0]));(format!("{batch}/{group}"),points)}).collect();
                super::plot_controls::export_series(ui,&series,"Injection order",plot_unit);
                super::plot_controls::plot(ui,"qc_order").height(170.0).x_axis_label("Injection order").y_axis_label(plot_unit).show(ui, |plot| {
                    let series:std::collections::BTreeSet<_>=observations.iter().map(|o|(&o.batch,&o.group)).collect();
                    for (batch,group) in series {
                        let mut points:Vec<_>=observations.iter().filter(|o|&o.batch==batch && &o.group==group).filter_map(|o|Some([o.order as f64,plotted_value(o,&d.rule.metric)?])).collect();
                        points.sort_by(|a,b|a[0].total_cmp(&b[0]));
                        plot.points(Points::new(format!("{batch}/{group}"),points.clone()).radius(4.0_f32));
                        plot.line(Line::new(format!("{batch}/{group}"),points));
                    }
                    match d.rule.metric {
                        Accuracy|DilutionIntegrity|Blank=>{if d.rule.lower != -f64::MAX {plot.hline(HLine::new("lower acceptance",d.rule.lower));}plot.hline(HLine::new("upper acceptance",d.rule.upper));}
                        MaximumAccuracyDeviation=>{plot.hline(HLine::new("lower accuracy",100.0-d.rule.upper));plot.hline(HLine::new("upper accuracy",100.0+d.rule.upper));}
                        RtDrift|MassError=>{plot.hline(HLine::new("negative bound",-d.rule.upper));plot.hline(HLine::new("positive bound",d.rule.upper));}
                        ControlChart=>{
                            if let Some((center,sd))=d.statistics.get("reference_value_mean").zip(d.statistics.get("reference_value_sample_sd")) {
                                plot.hline(HLine::new("reference mean",*center));
                                plot.hline(HLine::new("lower control",center-d.rule.upper*sd));
                                plot.hline(HLine::new("upper control",center+d.rule.upper*sd));
                            }
                        }
                        _=>{}
                    }
                });
                ui.collapsing("Underlying observations", |ui| { super::table::records(ui,"qc_observations",serde_json::to_value(&observations).unwrap(),"id",None); });
                if let Some(fit)=&d.calibration {ui.collapsing("Calibration evidence",|ui|{ui.label(format!("Coefficients (scaled by {}): {:?}; R²={:?}; weighted RMSE={}",fit.scale,fit.coefficients,fit.r_squared,fit.weighted_rmse));super::table::records(ui,"qc_calibration_points",serde_json::to_value(&fit.points).unwrap(),"sample",None);});}
            });
        });
    }
    ui.collapsing("Review history", |ui| {
        for event in &report.reviews {
            ui.label(format!(
                "{}: {} — {} (acknowledged: {})",
                event.actor, event.rule_id, event.reason, event.acknowledged
            ));
        }
    });
    review
}
pub(super) fn panel(
    state: &mut State,
    batch: Option<&crate::targeted::BatchResult>,
    ui: &mut egui::Ui,
) {
    ui.collapsing("Batch QC and method validation", |ui| {
        ui.label("Assay-specific rules retain calculations, evidence and prior evaluations. Missing required evidence prevents a pass.");
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(batch.is_some(),egui::Button::new("Configure from retained batch")).clicked() {
                let batch=batch.unwrap();
                match qc::targeted_study(batch,qc::method_rules(batch)) {Ok(mut study)=>{state.draft_evidence=study.targeted_evidence.take();state.draft=serde_json::to_string_pretty(&study).unwrap();},Err(e)=>state.message=e.to_string()}
            }
            if ui.button("Open study / report…").clicked() {
                if let Some(path)=rfd::FileDialog::new().add_filter("JSON",&["json"]).pick_file() {
                    let result=std::fs::read_to_string(path).map_err(|e|e.to_string()).and_then(|text| {
                        if let Ok(reports)=serde_json::from_str::<Vec<qc::Report>>(&text) {
                            if reports.is_empty() {return Err("QC history is empty".into());}
                            for report in &reports {qc::verify(report).map_err(|e|e.to_string())?;}
                            state.reports.extend(reports);state.selected=state.reports.len()-1;
                        } else if let Ok(report)=serde_json::from_str::<qc::Report>(&text) {
                            qc::verify(&report).map_err(|e|e.to_string())?;
                            state.reports.push(report);state.selected=state.reports.len()-1;
                        } else {
                            let mut study:qc::Study=serde_json::from_str(&text).map_err(|e|e.to_string())?;
                            state.draft_evidence=study.targeted_evidence.take();
                            state.draft=serde_json::to_string_pretty(&study).unwrap();
                        }
                        Ok(())
                    });
                    if let Err(e)=result {state.message=e;}
                }
            }
        });
        ui.label("Study groups identify levels, runs, matrix lots and preparations. Rule fields: metric, target, group, role, batch, lower, upper, minimum_n, required, reference_group. Purpose: batch_qc or method_validation.");
        ui.collapsing("Acceptance thresholds", |ui| {
            if let Ok(mut study)=serde_json::from_str::<qc::Study>(&state.draft) {
                let mut changed=false;
                for rule in &mut study.rules {
                    ui.push_id(&rule.id,|ui| {
                        ui.label(format!("{} / {} / {:?}",rule.id,rule.target,rule.metric));
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Lower (inclusive)");changed|=ui.add(egui::DragValue::new(&mut rule.lower).speed(0.1)).changed();
                            ui.label("Upper (inclusive)");changed|=ui.add(egui::DragValue::new(&mut rule.upper).speed(0.1)).changed();
                            ui.label("Minimum n");changed|=ui.add(egui::DragValue::new(&mut rule.minimum_n).range(1..=16384)).changed();
                            changed|=ui.checkbox(&mut rule.required,"Required for acceptance").changed();
                        });
                    });
                }
                if changed {state.draft=serde_json::to_string_pretty(&study).unwrap();}
            } else {ui.label("Load or configure a study to edit its thresholds.");}
        });
        super::forms::typed::<qc::Study>(ui,"Study design, observations and acceptance rules",&mut state.draft);
        ui.collapsing("Available calculations", |ui| {ui.label("accuracy, maximum_accuracy_deviation, precision, blank, carryover, internal_standard_cv, rt_drift, mass_error, order_slope, rt_drift_slope, mass_drift_slope, internal_standard_slope, missingness, calibration_acceptance, calibration_r2, calibration_rmse, calibration_accuracy_deviation, control_chart, recovery, matrix_factor, process_efficiency, selectivity, stability, dilution_integrity");});
        ui.collapsing("Advanced study JSON", |ui| { egui::ScrollArea::vertical().id_salt("qc_draft_scroll").max_height(220.0).show(ui, |ui| {ui.add(egui::TextEdit::multiline(&mut state.draft).desired_rows(7).desired_width(ui.available_width()).code_editor());}); });
        if ui.button("Evaluate and retain QC").clicked() {
            let result=serde_json::from_str::<qc::Study>(&state.draft).map_err(|e|e.to_string()).and_then(|mut study| {
                if study.targeted_evidence.is_none() {study.targeted_evidence=state.draft_evidence.clone();}
                execute(if study.purpose=="method_validation" {crate::domain::Operation::ValidateMethod{study}} else {crate::domain::Operation::EvaluateQc{study}})
            });
            match result {Ok(report)=>{state.reports.push(report);state.selected=state.reports.len()-1;state.message="QC evaluation retained".into();},Err(e)=>state.message=e}
        }
        ui.label(&state.message);
        if state.reports.is_empty() {return;}
        egui::ComboBox::from_id_salt("qc_history").selected_text(format!("QC evaluation {}",state.selected+1)).show_ui(ui, |ui| {
            for i in 0..state.reports.len(){ui.selectable_value(&mut state.selected,i,format!("Evaluation {}",i+1));}
        });
        let report=&state.reports[state.selected];
        ui.horizontal(|ui| {
            if ui.button("Save QC history JSON…").clicked() {
                if let Some(path)=rfd::FileDialog::new().save_file() {
                    state.message=match serde_json::to_vec_pretty(&state.reports).map_err(|e|e.to_string()).and_then(|b|save(&path,&b).map_err(|e|e.to_string())) {Ok(())=>"QC history saved".into(),Err(e)=>e};
                }
            }
            for (label,json) in [("Save QC evidence JSON…",true),("Export QC CSV…",false)] {
                if ui.button(label).clicked() {
                    if let Some(path)=rfd::FileDialog::new().save_file() {
                        let bytes=if json {serde_json::to_vec_pretty(report).map_err(|e|e.to_string())} else {qc::csv(report).map(String::into_bytes).map_err(|e|e.to_string())};
                        state.message=match bytes.and_then(|b|save(&path,&b).map_err(|e|e.to_string())) {Ok(())=>"Export saved".into(),Err(e)=>e};
                    }
                }
            }
        });
        if let Some((rule_id,acknowledged))=dashboard(report,&mut state.reason,ui) {
            let result=execute(crate::domain::Operation::ReviewQc{report:Box::new(report.clone()),expected_revision:report.reviews.len(),rule_id,reason:state.reason.clone(),acknowledged});
            match result {Ok(next)=>{state.reports.push(next);state.selected=state.reports.len()-1;state.message="Review revision retained".into();},Err(e)=>state.message=e}
        }
    });
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
        state: &mut State,
        batch: &crate::targeted::BatchResult,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1500.0, 1600.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| panel(state, Some(batch), ui));
            },
        )
    }
    fn click(
        state: &mut State,
        batch: &crate::targeted::BatchResult,
        ctx: &egui::Context,
        label: &str,
        frames: &mut Vec<egui::FullOutput>,
    ) {
        let output = frame(state, batch, ctx, vec![]);
        let pos = super::super::test_render::text_center(&output.shapes, label)
            .unwrap_or_else(|| panic!("Missing {label}"));
        frames.push(output);
        frames.push(frame(
            state,
            batch,
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
            batch,
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
    fn qc_dashboard_evaluation_and_review_preserve_failed_status() {
        let dir = tempfile::tempdir().unwrap();
        let batch =
            crate::targeted::run(raw_request(dir.path()), &crate::jobs::JobControl::default())
                .unwrap();
        let json: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/reference/qc.json"
        )))
        .unwrap();
        let mut study: qc::Study = serde_json::from_value(json["study"].clone()).unwrap();
        study.rules.retain(|r| r.metric == qc::Metric::Blank);
        study.rules[0].upper = 0.1;
        let mut state = State {
            draft: serde_json::to_string_pretty(&study).unwrap(),
            reason: "Contamination investigated; rerun required".into(),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        ctx.style_mut(|s| s.animation_time = 0.0);
        let mut frames = vec![frame(&mut state, &batch, &ctx, vec![])];
        click(
            &mut state,
            &batch,
            &ctx,
            "Batch QC and method validation",
            &mut frames,
        );
        // Independent method studies remain usable without a targeted run.
        let standalone = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1500.0, 1600.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| panel(&mut state, None, ui));
            },
        );
        assert!(super::super::test_render::text_center(
            &standalone.shapes,
            "Evaluate and retain QC"
        )
        .is_some());
        frames.push(standalone);
        click(
            &mut state,
            &batch,
            &ctx,
            "Evaluate and retain QC",
            &mut frames,
        );
        assert_eq!(state.reports.len(), 1, "{}", state.message);
        assert_eq!(state.reports[0].status, qc::Status::Fail);
        let d = &state.reports[0].decisions[0];
        let label = format!(
            "{} / {}: {:?} = {:?} {}",
            d.rule.id, d.rule.target, d.status, d.value, d.unit
        );
        click(&mut state, &batch, &ctx, &label, &mut frames);
        click(&mut state, &batch, &ctx, "Acknowledge review", &mut frames);
        assert_eq!(state.reports.len(), 2, "{}", state.message);
        assert_eq!(state.reports[1].status, qc::Status::Fail);
        assert!(state.reports[1].review_queue.is_empty());
        assert_eq!(state.reports[0].reviews.len(), 0);
        qc::verify(&state.reports[1]).unwrap();
        frames.push(frame(&mut state, &batch, &ctx, vec![]));
        if std::env::var_os("CHROMASCOPE_QC_PREVIEW").is_some() {
            super::super::test_render::save(
                &ctx,
                frames,
                std::path::Path::new("target/qc-review.png"),
                egui::vec2(1500.0, 1600.0),
            );
        }
    }
}
