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
        super::forms::typed::<Table>(
            ui,
            "Sample design, metadata and original quantities",
            &mut s.draft,
        );
        super::forms::typed::<Settings>(
            ui,
            "Filters, exclusions and comparison groups",
            &mut s.settings,
        );
        ui.collapsing(
            "Advanced original quantitative table JSON (sample-major)",
            |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut s.draft)
                        .code_editor()
                        .desired_rows(10)
                        .desired_width(f32::INFINITY),
                );
            },
        );
        ui.collapsing(
            "Analysis settings, filters, exclusions and group definitions",
            |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut s.settings)
                        .code_editor()
                        .desired_rows(15)
                        .desired_width(f32::INFINITY),
                );
            },
        );
        settings_controls(ui, &mut s.settings);
        if ui
            .add_enabled(
                s.pending.is_none(),
                egui::Button::new("Analyze and retain new revision"),
            )
            .clicked()
        {
            let input = serde_json::from_str::<Table>(&s.draft)
                .and_then(|t| serde_json::from_str::<Settings>(&s.settings).map(|cfg| (t, cfg)));
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
    });
    s.open = open;
}
fn settings_controls(ui: &mut egui::Ui, text: &mut String) {
    ui.collapsing("Preprocessing and comparison controls",|ui|{
        let Ok(mut cfg)=serde_json::from_str::<Settings>(text)else{ui.label("Correct the settings JSON to use these controls");return;};
        let original=serde_json::to_string(&cfg).unwrap();
        for (label,value,choices) in [
            ("Missing values",&mut cfg.missing,&["reject","median","half_minimum","complete_features"][..]),
            ("Normalization",&mut cfg.normalization,&["none","total","median","internal_standard"][..]),
            ("Transformation",&mut cfg.transform,&["none","log2","log10","sqrt"][..]),
            ("Scaling",&mut cfg.scaling,&["none","center","autoscale","pareto"][..]),
            ("Euclidean linkage",&mut cfg.linkage,&["average","complete","single","ward"][..]),
            ("FDR adjustment",&mut cfg.fdr,&["bh","by"][..]),
        ]{egui::ComboBox::from_label(label).selected_text(value.as_str()).show_ui(ui,|ui|{for &choice in choices{ui.selectable_value(value,choice.into(),choice);}});}
        ui.horizontal(|ui|{ui.label("Maximum missing fraction");ui.add(egui::DragValue::new(&mut cfg.max_missing_fraction).range(0. ..=1.).speed(0.01));ui.label("Explicit pseudocount (original feature units)");ui.add(egui::DragValue::new(&mut cfg.pseudocount).range(0. ..=f64::MAX).speed(0.01));});
        if cfg.normalization=="internal_standard"{ui.horizontal(|ui|{ui.label("Internal-standard feature ID");ui.text_edit_singleline(cfg.internal_standard.get_or_insert_with(String::new));});}
        let mut compare=cfg.groups.is_some();ui.checkbox(&mut compare,"Compare two independent groups");
        if compare{let groups=cfg.groups.get_or_insert_with(||crate::statistics::Groups{metadata_key:"group".into(),reference:String::new(),comparison:String::new()});ui.horizontal(|ui|{ui.label("Metadata key");ui.text_edit_singleline(&mut groups.metadata_key);ui.label("Reference");ui.text_edit_singleline(&mut groups.reference);ui.label("Comparison");ui.text_edit_singleline(&mut groups.comparison);});ui.horizontal(|ui|{ui.label("Confidence level");ui.add(egui::DragValue::new(&mut cfg.confidence).range(0.001..=0.999).speed(0.001));});}else{cfg.groups=None;}
        ui.label("Preprocessing order: exclusions → missingness filter → imputation → normalization → transformation → scaling. Welch tests use the pre-scaling values. BH assumes independence or suitable positive dependence; BY is more conservative.");
        if serde_json::to_string(&cfg).unwrap()!=original{*text=serde_json::to_string_pretty(&cfg).unwrap();}
    });
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
