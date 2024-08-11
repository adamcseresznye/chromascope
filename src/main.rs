#![windows_subsystem = "windows"]

mod gui;
mod parser;
mod plotting_parameters;

use egui::IconData;
use env_logger;
use gui::*;
use log::{error, info};
use std::process;

fn main() {
    env_logger::init();

    // include icon in the compiled binary
    let icon_image = image::load_from_memory(include_bytes!(r"../assets/chromascope_icon.png"))
        .expect("Should be able to open icon PNG file");

    let width = icon_image.width();
    let height = icon_image.height();
    let icon_rgba8 = icon_image.into_rgba8().to_vec();
    let icon_data = IconData {
        rgba: icon_rgba8,
        width,
        height,
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_icon(icon_data),
        ..Default::default()
    };

    match eframe::run_native(
        "Chromascope",
        native_options,
        Box::new(|cc| Box::new(MzViewerApp::new(cc))),
    ) {
        Ok(_) => {
            info!("Application exited succesfully.");
            process::exit(0)
        }
        Err(e) => {
            error!("Error occured: {:?}.", e);
            process::exit(1)
        }
    }
}
