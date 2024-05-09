mod gui;
mod parser;

use gui::*;
use parser::*;

fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "MzViewer",
        native_options,
        Box::new(|cc| Box::new(MyEguiApp::new(cc))),
    );
}
