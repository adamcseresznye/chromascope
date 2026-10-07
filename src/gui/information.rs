//! Product information and contextual dataset metadata.
use super::{state::FileId, MzViewerApp};
use eframe::egui::{self, Context};

pub(super) fn show(app: &MzViewerApp, ctx: &Context) {
    let about_id = egui::Id::new("about_open");
    let mut open = ctx.data(|d| d.get_temp::<bool>(about_id).unwrap_or(false));
    if open {
        egui::Window::new("About Chromascope")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(430.0)
            .show(ctx, |ui| {
                let texture_id = egui::Id::new("about_logo");
                let texture = ctx.data(|d| d.get_temp::<egui::TextureHandle>(texture_id)).or_else(|| {
                    let image = image::load_from_memory(include_bytes!("../../assets/icon.png")).ok()?.into_rgba8();
                    let size = [image.width() as usize, image.height() as usize];
                    let texture = ctx.load_texture("Chromascope logo", egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()), egui::TextureOptions::LINEAR);
                    ctx.data_mut(|d| d.insert_temp(texture_id, texture.clone()));
                    Some(texture)
                });
                ui.horizontal(|ui| {
                    if let Some(texture) = texture {
                        ui.image((texture.id(), egui::vec2(80.0, 80.0)));
                    }
                    ui.vertical(|ui| {
                        ui.heading("Chromascope");
                        ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                        ui.label(format!("Created by {}", env!("CARGO_PKG_AUTHORS")));
                    });
                });
                ui.separator();
                ui.label("A lightweight mass spectrometry viewer and batch peak-integration workbench.");
                ui.label("Inspect chromatograms and spectra, integrate peaks, review batch results, and export CSV files. Save extraction presets and quantification methods for reuse.");
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    ui.hyperlink_to("Project & documentation", "https://github.com/adamcseresznye/chromascope");
                    ui.hyperlink_to("Report an issue", "https://github.com/adamcseresznye/chromascope/issues");
                });
                ui.hyperlink_to("GPL-3.0-only license", "https://github.com/adamcseresznye/chromascope/blob/main/LICENSE");
                egui::CollapsingHeader::new("Credits & licenses").show(ui, |ui| {
                    ui.label("Built with Rust, egui/eframe, and mzdata.");
                    ui.hyperlink_to("Inter by Rasmus Andersson · SIL Open Font License", "https://rsms.me/inter/");
                    egui::ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                        ui.small(include_str!("../../assets/fonts/inter/LICENSE.txt"));
                    });
                });
            });
        ctx.data_mut(|d| d.insert_temp(about_id, open));
    }
    let file_id = egui::Id::new("file_info_open");
    if let Some(id) = ctx.data(|d| d.get_temp::<FileId>(file_id)) {
        let mut open = true;
        egui::Window::new("File information")
            .open(&mut open)
            .collapsible(false)
            .default_width(420.0)
            .show(ctx, |ui| {
                if let Some(file) = app.files.get(&id) {
                    ui.heading(&file.name);
                    ui.label(format!("{} scans", file.data.bounds.scan_count));
                    ui.label(format!(
                        "Retention time: {:.2}–{:.2} min",
                        file.data.bounds.min_rt, file.data.bounds.max_rt
                    ));
                    ui.label(format!(
                        "m/z range: {:.2}–{:.2}",
                        file.data.bounds.min_mz, file.data.bounds.max_mz
                    ));
                    ui.separator();
                    ui.label("Source path");
                    ui.label(file.cache.source_path.as_deref().unwrap_or(&file.path));
                } else {
                    ui.label("This dataset has been removed from the workspace.");
                }
            });
        if !open {
            ctx.data_mut(|d| d.remove::<FileId>(file_id));
        }
    }
}
