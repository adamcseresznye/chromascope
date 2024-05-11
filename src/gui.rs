use crate::parser;

use egui::Color32;
use mzdata::spectrum::ScanPolarity;

use eframe::egui;
use egui_plot::{Line, PlotPoints};
use rfd;

#[derive(PartialEq, Clone, Debug)]
enum PlotType {
    XIC,
    BPC,
    TIC,
}

impl Default for PlotType {
    fn default() -> Self {
        PlotType::TIC
    }
}
const FILE_FORMAT: &str = "mzML";

#[derive(Default)]
pub struct MzViewerApp {
    file_path: Option<String>,
    plot_type: PlotType,
    polarity: ScanPolarity,
    plot_data: Option<Vec<[f64; 2]>>,
    mass_input: String,
    mass_tolerance_input: String,

    invalid_file: bool,
    state_changed: bool,

    mass: f64,
    mass_tolerance: f64,
    options_window_open: bool,
}

impl MzViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }

    fn process_plot_data(&mut self, path: &str) -> Option<Vec<[f64; 2]>> {
        let parsed_data = match self.plot_type {
            PlotType::TIC => parser::get_tic(path, self.polarity),
            PlotType::BPC => parser::get_bpic(path, self.polarity),
            PlotType::XIC => parser::get_xic(path, self.mass, self.polarity, self.mass_tolerance),
        };
        parser::prepare_for_plot(parsed_data).ok()
    }

    fn plot_chromatogram(&mut self, ui: &mut egui::Ui) -> egui::Response {
        if let Some(path) = &self.file_path {
            // Only re-process the data if the state has changed
            if self.state_changed {
                let path_clone = path.clone();
                self.plot_data = self.process_plot_data(path_clone.as_str());
                self.state_changed = false;
            };
        }

        egui_plot::Plot::new("chromatogram")
            .show(ui, |plot_ui| {
                if let Some(data) = &self.plot_data {
                    plot_ui.line(
                        Line::new(PlotPoints::from(data.clone()))
                            .name(format!("{:?}", self.plot_type)),
                    )
                }
            })
            .response
    }
    fn update_data_selection_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("data_selection_panel").show(ctx, |ui| {
            if ui.button("🗀 Open File").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    let file_path_str = path.display().to_string();
                    if file_path_str.ends_with(FILE_FORMAT) {
                        self.invalid_file = false;
                        self.file_path = Some(file_path_str.clone());
                    }
                } else {
                    self.invalid_file = true
                }
            }
        });
    }

    fn update_file_information_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("file_information_panel").show(ctx, |ui| {
            ui.label("Opened file:");
            ui.add_space(12.0);
            if self.invalid_file {
                ui.colored_label(
                    Color32::LIGHT_RED,
                    format!("Invalid file type. Please select an {} file.", FILE_FORMAT),
                )
            } else {
                ui.small(format!(
                    "{:?}",
                    self.file_path
                        .as_ref()
                        .map_or("No file selected".to_string(), ToString::to_string)
                ))
            }
        });
    }

    fn update_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let chromatogram = self.plot_chromatogram(ui);
            chromatogram.context_menu(|ui| {
                ui.heading("Global Options");
                ui.separator();
                egui::Grid::new("TextLayoutDemo")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("Polarity");
                        ui.horizontal(|ui| {
                            if ui
                                .add(egui::RadioButton::new(
                                    self.polarity == ScanPolarity::Positive,
                                    "Positive",
                                ))
                                .clicked()
                            {
                                self.polarity = ScanPolarity::Positive;
                                self.state_changed = true;
                            }
                            if ui
                                .add(egui::RadioButton::new(
                                    self.polarity == ScanPolarity::Negative,
                                    "Negative",
                                ))
                                .clicked()
                            {
                                self.polarity = ScanPolarity::Negative;
                                self.state_changed = true;
                            }
                        });
                        ui.end_row();

                        ui.label("Scan Type");
                        ui.horizontal(|ui| {
                            if ui
                                .add(egui::RadioButton::new(
                                    self.plot_type == PlotType::TIC,
                                    "Total Ion Chromatogram",
                                ))
                                .clicked()
                            {
                                self.plot_type = PlotType::TIC;
                                self.state_changed = true;
                            }
                            if ui
                                .add(egui::RadioButton::new(
                                    self.plot_type == PlotType::BPC,
                                    "Base Peak Chromatogram",
                                ))
                                .clicked()
                            {
                                self.plot_type = PlotType::BPC;
                                self.state_changed = true;
                            }
                            if ui
                                .add(egui::RadioButton::new(
                                    self.plot_type == PlotType::XIC,
                                    "Extracted Ion Chromatogram",
                                ))
                                .clicked()
                            {
                                self.plot_type = PlotType::XIC;
                                self.options_window_open = true;
                            }
                        });
                        ui.end_row();
                    });
                ui.separator();
            });
        });
    }

    fn update_xic_settings_window(&mut self, ctx: &egui::Context) {
        if self.options_window_open {
            egui::Window::new("XIC settings")
                .open(&mut self.options_window_open)
                .show(ctx, |ui| {
                    ui.label("Enter m/z and mass tolerance values:");
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.mass_input).hint_text("Enter m/z"),
                        )
                        .lost_focus()
                    {
                        self.mass = self.mass_input.parse().unwrap_or(self.mass);
                        self.state_changed = true;
                    };
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.mass_tolerance_input)
                                .hint_text("Enter mass tolerance"),
                        )
                        .lost_focus()
                    {
                        self.mass_tolerance = self
                            .mass_tolerance_input
                            .parse()
                            .unwrap_or(self.mass_tolerance);
                        self.state_changed = true
                    };
                });
        }
    }
}
impl eframe::App for MzViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_data_selection_panel(ctx);
        self.update_file_information_panel(ctx);
        self.update_central_panel(ctx);
        self.update_xic_settings_window(ctx);
    }
}
