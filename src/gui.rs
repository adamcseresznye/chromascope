#![warn(clippy::all)]

use crate::{line_color::LineColor, line_type::LineType, parser, plot_type::PlotType};

use mzdata::spectrum::ScanPolarity;
use std::path::PathBuf;

use eframe::egui;
use egui::{Color32, Context, Ui};
use egui_plot::{Line, PlotPoints};
use mzdata::spectrum::RefPeakDataLevel;
use mzdata::{prelude::*, MzMLReader};

const FILE_FORMAT: &str = "mzML";

#[derive(Default)]
pub struct MzViewerApp {
    file_path: Option<String>,
    plot_type: PlotType,
    polarity: ScanPolarity,
    plot_data: Option<Vec<[f64; 2]>>,
    mass_input: String,
    mass_tolerance_input: String,
    line_type: LineType,
    line_color: LineColor,

    invalid_file: bool,
    state_changed: bool,

    mass: f64,
    mass_tolerance: f64,
    options_window_open: bool,
    checkbox_bool: bool,
    smoothing: u8,
    line_width: f32,

    rt: f32,
}

impl MzViewerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            line_width: 1.0,
            ..Default::default()
        }
    }

    fn process_plot_data(&self, path: &str) -> Option<Vec<[f64; 2]>> {
        let parsed_data = match self.plot_type {
            PlotType::Tic => parser::get_tic(path, self.polarity),
            PlotType::Bpc => parser::get_bpic(path, self.polarity),
            PlotType::Xic => parser::get_xic(path, self.mass, self.polarity, self.mass_tolerance),
        };

        let prepared_data = parser::prepare_for_plot(parsed_data);
        let smoothed_data = parser::smooth_data(prepared_data, self.smoothing);
        smoothed_data.ok()
    }

    fn plot_chromatogram(&mut self, ui: &mut egui::Ui) -> egui::Response {
        if let Some(path) = &self.file_path {
            // Only re-process the data if the state has changed
            if self.state_changed {
                self.plot_data = self.process_plot_data(path);
                self.state_changed = false;
            };
        }

        egui_plot::Plot::new("chromatogram")
            .width(ui.available_width() * 0.99)
            .height(ui.available_height() * 0.6)
            .show(ui, |plot_ui| {
                if let Some(data) = &self.plot_data {
                    plot_ui.line(
                        Line::new(PlotPoints::from(data.clone()))
                            .width(self.line_width)
                            .style(self.line_type.to_egui())
                            .color(self.line_color.to_egui())
                            .name(format!("{:?}", self.plot_type)),
                    )
                }
            })
            .response
    }

    fn plot_mass_spectrum(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let mz = [100.0, 125.0, 135.3];
        let intensity = [100.0, 125.0, 135.3];

        // Create bar chart data
        let bars: Vec<egui_plot::Bar> = mz
            .iter()
            .zip(intensity.iter())
            .map(|(&m, &i)| {
                egui_plot::Bar::new(m, i.into())
                    .width(0.25) // Adjust width of bars as needed
                    .fill(self.line_color.to_egui()) // Adjust color as needed
            })
            .collect();

        egui_plot::Plot::new("mass_spectrum")
            .width(ui.available_width() * 0.99)
            .height(ui.available_height())
            .show(ui, |plot_ui| {
                plot_ui.bar_chart(egui_plot::BarChart::new(bars));

                // Customize axes
                //plot_ui.set_plot_bounds(egui_plot::PlotBounds::from_min_max(
                //    [95.0, 0.0],
                //    [205.0, 110.0]
                //));
            })
            .response
    }

    fn update_data_selection_panel(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("data_selection_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .button("File")
                    .on_hover_text("Click to Open File")
                    .clicked()
                {
                    self.plot_data = None; // clears the plot_data if new file is opened
                    self.file_path = None; // clears the file_path if new file is opened
                    self.handle_file_selection();
                }

                ui.menu_button("Display", |ui| {
                    self.add_display_options(ui);
                });

                if let Some(new_visuals) = ui
                    .style()
                    .visuals
                    .clone()
                    .light_dark_small_toggle_button(ui)
                {
                    ctx.set_visuals(new_visuals);
                }
            });
        });
    }

    fn add_display_options(&mut self, ui: &mut Ui) {
        ui.menu_button("Smoothing", |ui| {
            let slider = egui::Slider::new(&mut self.smoothing, 0..=11);
            let response = ui.add(slider);
            if response.changed() {
                self.state_changed = true;
            }
            response.on_hover_text("Adjust the level of moving average smoothing");
        });

        ui.menu_button("Line width", |ui| {
            let slider = egui::Slider::new(&mut self.line_width, 0.1..=5.0);
            let response = ui.add(slider);
            if response.changed() {
                self.state_changed = true;
            }
            response.on_hover_text("Adjust the line width");
        });

        ui.menu_button("Line color", |ui| {
            self.add_line_color_options(ui);
        });

        ui.menu_button("Line style", |ui| {
            self.add_line_style_options(ui);
        });
    }

    fn add_line_color_options(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.line_color, LineColor::Red, "Red");
            ui.radio_value(&mut self.line_color, LineColor::Blue, "Blue");
            ui.radio_value(&mut self.line_color, LineColor::Green, "Green");
            ui.radio_value(&mut self.line_color, LineColor::Yellow, "Yellow");
            ui.radio_value(&mut self.line_color, LineColor::Black, "Black");
            ui.radio_value(&mut self.line_color, LineColor::White, "White");
        });
    }

    fn add_line_style_options(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.line_type, LineType::Solid, "Solid");
            ui.radio_value(&mut self.line_type, LineType::Dashed, "Dashed");
            ui.radio_value(&mut self.line_type, LineType::Dotted, "Dotted");
        });
    }

    fn handle_file_selection(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.update_file_path_and_validity(path);
        } else {
            self.invalid_file = true;
        }
    }

    fn update_file_path_and_validity(&mut self, path: PathBuf) {
        let file_path_str = path.display().to_string();
        if file_path_str.ends_with(FILE_FORMAT) {
            self.invalid_file = false;
            self.file_path = Some(file_path_str.clone());
        } else {
            self.invalid_file = true;
        }
    }

    fn update_file_information_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("file_information_panel").show(ctx, |ui| {
            ui.label("Opened file:");
            ui.separator();

            match self.invalid_file {
                true => {
                    ui.colored_label(
                        Color32::LIGHT_RED,
                        format!("Invalid file type. Please select an {} file.", FILE_FORMAT),
                    );
                }
                false => match self.file_path {
                    Some(ref file_path) => {
                        self.checkbox_bool = true;
                        if ui
                            .checkbox(
                                &mut self.checkbox_bool,
                                egui::RichText::new(file_path).small(),
                            )
                            .on_hover_text("Click to Close File")
                            .clicked()
                        {
                            self.plot_data = None;
                            self.file_path = None;
                            self.checkbox_bool = false;
                        }
                    }
                    None => {
                        ui.colored_label(Color32::LIGHT_RED, "No file selected".to_string());
                    }
                },
            };
        });
    }

    fn update_central_panel(&mut self, ctx: &Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                egui::CollapsingHeader::new("Chromatogram")
                    .default_open(true)
                    .show(ui, |ui| {
                        let chromatogram = self.plot_chromatogram(ui);
                        chromatogram.context_menu(|ui| {
                            ui.heading("Plot Properties");
                            ui.separator();
                            self.add_plot_properties(ui);
                            ui.separator();
                        });
                    });

                ui.add_space(5.0); // Add some space between the plots

                egui::CollapsingHeader::new("Mass Spectrum")
                    .default_open(true)
                    .show(ui, |ui| {
                        self.plot_mass_spectrum(ui);
                    });
            });
        });
    }

    fn add_plot_properties(&mut self, ui: &mut Ui) {
        egui::Grid::new("TextLayoutDemo")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                self.add_polarity_options(ui);
                ui.end_row();
                self.add_plot_type_options(ui);
                ui.end_row();
            });
    }

    fn add_polarity_options(&mut self, ui: &mut Ui) {
        ui.label("Polarity");
        ui.horizontal(|ui| {
            if ui
                .radio_value(&mut self.polarity, ScanPolarity::Positive, "Positive")
                .clicked()
            {
                self.polarity = ScanPolarity::Positive;
                self.state_changed = true;
            }
            if ui
                .radio_value(&mut self.polarity, ScanPolarity::Negative, "Negative")
                .clicked()
            {
                self.polarity = ScanPolarity::Negative;
                self.state_changed = true;
            }
        });
    }

    fn add_plot_type_options(&mut self, ui: &mut Ui) {
        ui.label("Plot Type");
        ui.horizontal(|ui| {
            if ui
                .radio_value(&mut self.plot_type, PlotType::Tic, "TIC")
                .clicked()
            {
                self.plot_type = PlotType::Tic;
                self.state_changed = true;
            }
            if ui
                .radio_value(&mut self.plot_type, PlotType::Bpc, "Base Peak")
                .clicked()
            {
                self.plot_type = PlotType::Bpc;
                self.state_changed = true;
            }
            if ui
                .radio_value(&mut self.plot_type, PlotType::Xic, "XIC")
                .clicked()
            {
                self.plot_type = PlotType::Xic;
                self.options_window_open = true;
            }
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
                                .hint_text("Enter mass tolerance in mmu"),
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
