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
    #[serde(skip)]
    pending: Option<std::sync::mpsc::Receiver<crate::domain::Result<serde_json::Value>>>,
}
impl State {
    pub(super) fn set_project(&mut self, root: &std::path::Path) {
        self.project = root.display().to_string();
    }
}
pub(super) fn show(app: &mut super::MzViewerApp, ctx: &egui::Context) {
    let s = &mut app.delivery;
    if let Some(rx) = &s.pending {
        match rx.try_recv() {
            Ok(result) => {
                s.message = match result {
                    Ok(v) => {
                        if v.get("source_verification").is_some() {
                            s.evidence = Some(v.clone());
                            "Project evidence loaded; source verification is available below".into()
                        } else {
                            v.to_string()
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
    super::workbench::analytical_panel(ctx, "Reports and exports", &mut s.open, |ui| {
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
        ui.text_edit_singleline(&mut s.destination);
        ui.label("Report title");
        ui.text_edit_singleline(&mut s.config.title);
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
        if ui
            .add_enabled(
                s.pending.is_none(),
                egui::Button::new("Export report and project"),
            )
            .clicked()
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let root = std::path::PathBuf::from(&s.project);
            let dest = std::path::PathBuf::from(&s.destination);
            let config = s.config.clone();
            let sources = s.include_sources;
            std::thread::spawn(move || {
                let _ = tx.send(crate::delivery::export(&root, &dest, &config, sources));
            });
            s.pending = Some(rx);
            s.message = "Exporting...".into();
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
}

impl State {
    pub(super) fn busy(&self) -> bool {
        self.pending.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
