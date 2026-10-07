//! Starter presets are embedded so they are available in standalone binaries.
use eframe::egui;
use std::path::PathBuf;

pub(super) fn menu(ui: &mut egui::Ui) -> Option<Result<PathBuf, String>> {
    let mut result = None;
    ui.menu_button("Save example preset…", |ui| {
        ui.label("Save a starter TOML file, then open it in its editor.");
        for (label, filename, text) in [
            (
                "Viewer extraction",
                "extraction-example.toml",
                include_str!("../../presets/example.toml"),
            ),
            (
                "Batch Quantification",
                "quantification-example.toml",
                include_str!("../../presets/quantification-example.toml"),
            ),
        ] {
            if ui.button(label).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_title(format!("Save {label} example"))
                    .add_filter("TOML preset", &["toml"])
                    .set_file_name(filename)
                    .save_file()
                {
                    result = Some(
                        std::fs::write(&path, text)
                            .map(|()| path)
                            .map_err(|e| format!("Cannot save example preset: {e}")),
                    );
                }
                ui.close();
            }
        }
    });
    result
}
