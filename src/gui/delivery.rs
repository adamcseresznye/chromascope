use eframe::egui;
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(super) struct State {
    pub(super) open: bool,
    project: String,
    destination: String,
    config: crate::delivery::Config,
    include_sources: bool,
    message: String,
    evidence: Option<serde_json::Value>,
    /// Scientist acknowledgment for exporting while warnings remain.
    #[serde(default)]
    accept_warnings: bool,
    /// Warning snapshot taken when a report export started (`None` while no
    /// export is in flight). Written into the bundle on success so the
    /// exported report preserves the warnings it was released with.
    #[serde(default)]
    export_warnings: Option<Vec<String>>,
    #[serde(skip)]
    pending: Option<std::sync::mpsc::Receiver<crate::domain::Result<serde_json::Value>>>,
}
impl State {
    pub(super) fn set_project(&mut self, root: &std::path::Path) {
        self.project = root.display().to_string();
    }
}
pub(super) fn show(app: &mut super::MzViewerApp, ctx: &egui::Context) {
    {
        let s = &mut app.delivery;
        if let Some(rx) = &s.pending {
            match rx.try_recv() {
                Ok(result) => {
                    let finished_export = s.export_warnings.take();
                    s.message = match result {
                        Ok(v) => {
                            if v.get("source_verification").is_some() {
                                s.evidence = Some(v.clone());
                                "Project evidence loaded; source verification is available below"
                                    .into()
                            } else {
                                match finished_export {
                                    Some(warnings) => {
                                        let note = if warnings.is_empty() {
                                            "Export complete — no open warnings at release.".into()
                                        } else {
                                            let destination =
                                                std::path::PathBuf::from(&s.destination);
                                            match write_warnings_file(&destination, &warnings) {
                                                Ok(()) => format!(
                                                    "Export complete with {} warning(s); preserved in review-warnings.json.",
                                                    warnings.len()
                                                ),
                                                Err(e) => format!(
                                                    "Export complete with {} warning(s), but review-warnings.json could not be written: {e}. Warnings: {}",
                                                    warnings.len(),
                                                    warnings.join(" | ")
                                                ),
                                            }
                                        };
                                        s.accept_warnings = false;
                                        note
                                    }
                                    None => v.to_string(),
                                }
                            }
                        }
                        Err(e) => e.to_string(),
                    };
                    s.pending = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    s.message = "Export worker disconnected".into();
                    s.pending = None;
                }
                Err(_) => ctx.request_repaint(),
            }
        }
    }
    let mut open = app.delivery.open;
    super::workbench::analytical_panel(ctx, "Reports and exports", &mut open, |ui| {
        release_review(app, ctx, ui);
        ui.separator();
        let s = &mut app.delivery;
        ui.label(
            "Open an engine project directory. Export retains all evidence and original history.",
        );
        ui.horizontal(|ui| {
            ui.label("Project");
            ui.text_edit_singleline(&mut s.project);
            if ui.button("Browse").clicked() {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    s.project = p.display().to_string();
                }
            }
        });
        ui.label("New output directory (must not exist)");
        ui.add(
            egui::TextEdit::singleline(&mut s.destination)
                .hint_text("e.g. C:/reports/review-01 — a new folder that does not exist yet"),
        );
        if !s.destination.is_empty() && std::path::Path::new(&s.destination).exists() {
            ui.small("That output path already exists. Choose a new folder name.");
        }
        ui.label("Report title");
        ui.add(
            egui::TextEdit::singleline(&mut s.config.title)
                .hint_text("Chromascope analytical review draft"),
        );
        ui.label("Include report sections");
        ui.horizontal_wrapped(|ui| {
            for &section in crate::delivery::SECTIONS {
                let mut enabled = s.config.sections.iter().any(|x| x == section);
                if ui.checkbox(&mut enabled, section).changed() {
                    s.config.sections.retain(|x| x != section);
                    if enabled {
                        s.config.sections.push(section.into());
                    }
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("SVG dimensions");
            ui.add(egui::DragValue::new(&mut s.config.figure_width).range(320..=16384));
            ui.add(egui::DragValue::new(&mut s.config.figure_height).range(240..=16384));
        });
        ui.checkbox(
            &mut s.include_sources,
            "Portable bundle with verified raw sources",
        );
        if ui
            .add_enabled(
                s.pending.is_none(),
                egui::Button::new("Reopen project evidence / verify sources"),
            )
            .clicked()
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let root = std::path::PathBuf::from(&s.project);
            let config = s.config.clone();
            std::thread::spawn(move || {
                let _ = tx.send(crate::delivery::report(&root, &config));
            });
            s.pending = Some(rx);
            s.message = "Loading project evidence and verifying sources…".into();
        }
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                ui.label(&s.message);
                if let Some(evidence) = &s.evidence {
                    ui.label(format!(
                        "Project {} revision {}",
                        evidence["project"]["id"], evidence["project"]["revision"]
                    ));
                    for result in evidence["results"]
                        .as_array()
                        .map(Vec::as_slice)
                        .unwrap_or(&[])
                    {
                        let id = result["result_id"].as_str().unwrap_or("");
                        egui::CollapsingHeader::new(format!(
                            "{}: {}",
                            id, result["output"]["kind"]
                        ))
                        .id_salt(id)
                        .show(ui, |ui| {
                            ui.label(serde_json::to_string_pretty(result).unwrap_or_default());
                            if ui
                                .add_enabled(
                                    s.pending.is_none(),
                                    egui::Button::new("Reprocess and compare retained evidence"),
                                )
                                .clicked()
                            {
                                match serde_json::from_value(result["result_id"].clone()) {
                                    Ok(id) => {
                                        let root = std::path::PathBuf::from(&s.project);
                                        let (tx, rx) = std::sync::mpsc::channel();
                                        std::thread::spawn(move || {
                                            let _ = tx.send(crate::delivery::reprocess(&root, id));
                                        });
                                        s.pending = Some(rx);
                                    }
                                    Err(e) => s.message = e.to_string(),
                                }
                            }
                        });
                    }
                }
            });
    });
    app.delivery.open = open;
}

/// Final review-and-release: one visible finish for the analysis-to-report
/// workflow. Viewer CSV/SVG and batch CSV remain available for intermediate
/// use; this section is the release artifact. Exports are classified as
/// blocked (missing preconditions), released with warnings (scientist
/// acknowledgment required, warnings preserved in `review-warnings.json`),
/// or clean. Criteria come only from the configured method and retained
/// engine reports.
fn release_review(app: &mut super::MzViewerApp, ctx: &egui::Context, ui: &mut egui::Ui) {
    ui.heading("Final review and release");
    ui.small("The report is the release artifact. Intermediate viewer CSV/SVG and batch CSV exports stay where they are for exploration; finish here.");
    let blockers = super::workflow::export_blockers(app);
    let warnings = super::workflow::export_warnings(app);
    for blocker in &blockers {
        ui.colored_label(ui.visuals().error_fg_color, format!("Blocked — {blocker}"));
    }
    if warnings.is_empty() && blockers.is_empty() {
        ui.small("No open warnings: peaks resolved, calibration fitted, QC reviewed.");
    }
    for warning in &warnings {
        ui.colored_label(ui.visuals().warn_fg_color, format!("Warning — {warning}"));
    }
    let mut chosen: Option<super::workflow::Destination> = None;
    ui.horizontal_wrapped(|ui| {
        ui.small("Resolve warnings where the evidence lives:");
        if ui.small_button("Data explorer").clicked() {
            chosen = Some(super::workflow::Destination::Data);
        }
        if ui.small_button("Quantification and QC").clicked() {
            chosen = Some(super::workflow::Destination::Quant);
        }
        if ui.small_button("AI activity").clicked() {
            chosen = Some(super::workflow::Destination::Activity);
        }
    });
    // Readiness snapshot for the export gate below.
    let (has_project, has_destination, destination_exists) = app.delivery.export_readiness();
    let busy = app.delivery.busy();
    if !warnings.is_empty() && blockers.is_empty() {
        ui.checkbox(
            &mut app.delivery.accept_warnings,
            format!(
                "I reviewed {} warning(s) above; export with warnings preserved in review-warnings.json",
                warnings.len()
            ),
        );
    }
    let ready = !busy
        && blockers.is_empty()
        && has_project
        && has_destination
        && !destination_exists
        && (warnings.is_empty() || app.delivery.accept_warnings);
    let reason = if busy {
        "An export or evidence load is already running…"
    } else if !blockers.is_empty() {
        "Resolve the blocked item(s) above to enable export."
    } else if !has_project {
        "Open a project first."
    } else if !has_destination {
        "Choose a new output folder below."
    } else if destination_exists {
        "That output path already exists. Choose a new folder name."
    } else if !warnings.is_empty() && !app.delivery.accept_warnings {
        "Acknowledge the warnings above to export with them preserved."
    } else {
        "Open a project, then choose a new output folder to export"
    };
    if ui
        .add_enabled(
            ready,
            egui::Button::new(egui::RichText::new("Export report and project").strong())
                .fill(ui.visuals().selection.bg_fill),
        )
        .on_hover_text(reason)
        .clicked()
    {
        let warnings_snapshot = warnings.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let root = std::path::PathBuf::from(&app.delivery.project);
        let dest = std::path::PathBuf::from(&app.delivery.destination);
        let config = app.delivery.config.clone();
        let sources = app.delivery.include_sources;
        std::thread::spawn(move || {
            let _ = tx.send(crate::delivery::export(&root, &dest, &config, sources));
        });
        app.delivery.pending = Some(rx);
        app.delivery.export_warnings = Some(warnings_snapshot);
        app.delivery.message = if warnings.is_empty() {
            "Exporting...".into()
        } else {
            format!(
                "Exporting with {} acknowledged warning(s)...",
                warnings.len()
            )
        };
    }
    if !ready && (busy || !blockers.is_empty() || !warnings.is_empty()) {
        ui.small(reason);
    }
    if let Some(destination) = chosen {
        super::workflow::go(app, ctx, destination);
    }
}

/// Preserve acknowledged export warnings inside the released bundle.
/// Writes `review-warnings.json` next to the exported report so the release
/// carries the warnings it was approved with.
pub(super) fn write_warnings_file(
    destination: &std::path::Path,
    warnings: &[String],
) -> Result<(), String> {
    let path = destination.join("review-warnings.json");
    let text = serde_json::to_string_pretty(warnings).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

impl State {
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub(super) fn set_project_if_empty(&mut self, root: &std::path::Path) {
        if self.project.is_empty() {
            self.project = root.display().to_string();
        }
    }
    /// Project-centered readiness for the final review-and-release step.
    /// Returns `(has_project, has_destination, destination_exists)`.
    pub(super) fn export_readiness(&self) -> (bool, bool, bool) {
        (
            !self.project.is_empty(),
            !self.destination.is_empty(),
            !self.destination.is_empty() && std::path::Path::new(&self.destination).exists(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acknowledged_warnings_are_preserved_as_json() {
        let dir = tempfile::tempdir().unwrap();
        let warnings = vec!["1 peak integration(s) are unresolved (test)".into()];
        write_warnings_file(dir.path(), &warnings).unwrap();
        let stored: Vec<String> = serde_json::from_slice(
            &std::fs::read(dir.path().join("review-warnings.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(stored, warnings);
    }
    #[test]
    fn report_configuration_software_render() {
        let mut app = super::super::MzViewerApp::default();
        app.delivery.open = true;
        let ctx = egui::Context::default();
        ctx.style_mut(|s| s.animation_time = 0.);
        let size = egui::vec2(1600., 1800.);
        let mut outputs = vec![];
        for _ in 0..3 {
            outputs.push(ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| show(&mut app, ctx),
            ));
        }
        let shapes = &outputs.last().unwrap().shapes;
        assert!(crate::gui::test_render::text_center(shapes, "calibration").is_some());
        assert!(crate::gui::test_render::text_center(
            shapes,
            "Portable bundle with verified raw sources"
        )
        .is_some());
        if std::env::var_os("CHROMASCOPE_DELIVERY_PREVIEW").is_some() {
            crate::gui::test_render::save(
                &ctx,
                outputs,
                std::path::Path::new("target/delivery-workspace.png"),
                size,
            );
        }
    }
}
