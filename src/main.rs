mod gui;
mod line_color;
mod line_type;
mod parser;
mod plot_type;

use gui::*;
use std::process;

fn main() {
    let native_options = eframe::NativeOptions::default();
    match eframe::run_native(
        "MzViewer",
        native_options,
        Box::new(|cc| Box::new(MzViewerApp::new(cc))),
    ) {
        Ok(_) => {
            println!("Application exited succesfully.");
            process::exit(0)
        }
        Err(e) => {
            eprintln!("Error occured: {:?}.", e);
            process::exit(1)
        }
    }
}
