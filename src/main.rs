mod gui;
mod line_color;
mod line_type;
mod parser;
mod plot_type;

use gui::*;

fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "MzViewer",
        native_options,
        Box::new(|cc| Box::new(MzViewerApp::new(cc))),
    );
}
