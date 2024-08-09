#![warn(
    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo
)]

use crate::{line_color::LineColor, line_type::LineType, parser, plot_type::PlotType};

use mzdata::spectrum::ScanPolarity;
use std::ops::Div;
use std::path::PathBuf;

use eframe::egui;
use egui::{Color32, Context, Ui};
use egui_plot::{Line, PlotPoints};
use log::{debug, error, info, trace, warn};
use std::cmp::Ordering;

const FILE_FORMAT: &str = "mzML";

#[derive(Default)]
pub struct UserInput {
    file_path: Option<String>,
    plot_type: PlotType,
    polarity: ScanPolarity,
    mass_input: String,
    mass_tolerance_input: String,
    mass: f64,
    mass_tolerance: f64,
    line_type: LineType,
    line_color: LineColor,
    smoothing: u8,
    line_width: f32,
    retention_time_ms_spectrum: Option<f32>,
}

#[derive(Default)]
enum FileValidity {
    Valid,
    #[default]
    Invalid,
}
#[derive(Default, PartialEq)]
enum StateChange {
    Changed,
    #[default]
    Unchanged,
}

#[derive(Default)]
pub struct MzViewerApp {
    parsed_ms_data: parser::MzData,
    plot_data: Option<Vec<[f64; 2]>>,
    user_input: UserInput,

    invalid_file: FileValidity,
    state_changed: StateChange,

    options_window_open: bool,
    checkbox_bool: bool,
}

impl MzViewerApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            user_input: UserInput {
                line_width: 1.0,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn process_plot_data(&mut self) -> Option<Vec<[f64; 2]>> {
        info!("Starting to process plot data");

        // Log user inputs
        debug!(
        "User input - mass: {:?}, polarity: {:?}, mass tolerance: {:?}, plot type: {:?}, smoothing: {}",
        self.user_input.mass,
        self.user_input.polarity,
        self.user_input.mass_tolerance,
        self.user_input.plot_type,
        self.user_input.smoothing
    );

        let result = match self.user_input.plot_type {
            PlotType::Tic => self.parsed_ms_data.get_tic(self.user_input.polarity),
            PlotType::Bpc => self.parsed_ms_data.get_bpic(self.user_input.polarity),
            PlotType::Xic => self.parsed_ms_data.get_xic(
                self.user_input.mass,
                self.user_input.polarity,
                self.user_input.mass_tolerance,
            ),
        };

        if result.is_err() {
            error!("Failed to get plot data for the specified plot type");
        }

        let prepared_data = self.parsed_ms_data.prepare_for_plot();
        if prepared_data.is_err() {
            error!("Failed to prepare data for plotting");
        }
        if self
            .parsed_ms_data
            .smooth_data(prepared_data, self.user_input.smoothing)
            .is_err()
        {
            error!("Failed to smooth data");
            return None;
        };

        let plot_data = &self.parsed_ms_data.plot_data;
        info!("Finished processing plot data");
        plot_data.clone()
    }

    fn plot_chromatogram(&mut self, ui: &mut egui::Ui) -> egui::Response {
        if let Some(_path) = &self.user_input.file_path {
            // Only re-process the data if the state has changed
            if self.state_changed == StateChange::Changed {
                info!("State has changed, starting to plot chromatogram");
                self.plot_data = self.process_plot_data();
                self.state_changed = StateChange::Unchanged;
            }
        }

        let mut plot_bounds = None;

        let response = egui_plot::Plot::new("chromatogram")
            .width(ui.available_width() * 0.99)
            .height(ui.available_height() * 0.6)
            .show(ui, |plot_ui| {
                if let Some(data) = &self.plot_data {
                    plot_ui.line(
                        Line::new(PlotPoints::from(data.clone()))
                            .width(self.user_input.line_width)
                            .style(self.user_input.line_type.to_egui())
                            .color(self.user_input.line_color.to_egui()), //.name(format!("{:?}", self.user_input.plot_type)),
                    );
                } else {
                    warn!("No plot data available");
                }
                plot_bounds = Some(plot_ui.plot_bounds());
            })
            .response;

        if response.triple_clicked() {
            let rt_clicked = self.determine_rt_clicked(&response, plot_bounds);
            info!("Triple click detected on plot at {:?}", &rt_clicked);

            if let Some(index) = self.find_closest_spectrum(rt_clicked) {
                info!("Found closest spectrum at index: {}", index);
                self.parsed_ms_data.get_mass_spectrum_by_index(index);
            } else {
                warn!("No close spectrum found for the clicked retention time");
            }
        }

        response
    }

    fn determine_rt_clicked(
        &mut self,
        response: &egui::Response,
        plot_bounds: Option<egui_plot::PlotBounds>,
    ) -> Option<f32> {
        if let Some(plot_position) = response.interact_pointer_pos() {
            if let Some(bounds) = plot_bounds {
                let plot_width = response.rect.width();

                let min_x = *bounds.range_x().start();
                let max_x = *bounds.range_x().end();

                // Calculate the position relative to the plot area, not the response area
                let relative_x = (plot_position.x - response.rect.left()) / plot_width;

                let converted_rt = min_x + relative_x as f64 * (max_x - min_x);

                self.user_input.retention_time_ms_spectrum = Some(converted_rt as f32);
                info!(
                    "Retention time clicked: {:?}",
                    self.user_input.retention_time_ms_spectrum
                );

                return Some(converted_rt as f32);
            } else {
                warn!("Plot bounds are None");
            }
        } else {
            warn!("No plot position detected");
        }
        None
    }

    fn find_closest_spectrum(&self, clicked_rt: Option<f32>) -> Option<usize> {
        if let Some(rt) = clicked_rt {
            if let (Some(retention_times), Some(indices)) = (
                &self.parsed_ms_data.retention_time,
                &self.parsed_ms_data.index,
            ) {
                match retention_times.binary_search_by(|spectrum| {
                    spectrum.partial_cmp(&rt).unwrap_or(Ordering::Equal)
                }) {
                    Ok(found_index) => {
                        info!("Exact Rt match found at index: {:?}", found_index);
                        Some(indices[found_index])
                    }
                    Err(found_index) => {
                        // If the exact RT is not found, return the closest one
                        info!("Closest Rt match not found, using nearest index: {:?}", found_index);
                        if found_index == 0 {
                            indices.first().copied()
                        } else if found_index == indices.len() {
                            indices.last().copied()
                        } else {
                            // Compare the two closest values and return the closer one
                            let prev = &retention_times[found_index - 1];
                            let next = &retention_times[found_index];
                            if (rt - prev).abs() < (next - rt).abs() {
                                Some(indices[found_index - 1])
                            } else {
                                Some(indices[found_index])
                            }
                        }
                    }
                }
            } else {
                warn!("Retention time or index data is missing.");
                None
            }
        } else {
            warn!("No close RT match found. Mass spectrum can't be extracted/displayed.");
            None
        }
    }
    

    fn plot_mass_spectrum(&mut self, ui: &mut egui::Ui) -> egui::Response {
        if let Some((mz, intensity)) = &self.parsed_ms_data.mass_spectrum {
            // Create bar chart data
            let bars: Vec<egui_plot::Bar> = mz
                .iter()
                .zip(intensity.iter())
                .map(|(&m, &i)| {
                    egui_plot::Bar::new(m, i.into())
                        .width(self.user_input.line_width.div(2.0).into()) // Adjust width of bars as needed
                        .fill(self.user_input.line_color.to_egui()) // Adjust color as needed
                })
                .collect();

            let response = egui_plot::Plot::new("mass_spectrum")
                .width(ui.available_width() * 0.99)
                .height(ui.available_height())
                .show(ui, |plot_ui| {
                    let bounds = plot_ui.plot_bounds();
                    let zoom_level = (bounds.max()[0] - bounds.min()[0]).abs(); // Calculate zoom level based on plot bounds

                    let bar_width = zoom_level * 0.001; // Adjust bar width based on zoom level
                    let adjusted_bars: Vec<egui_plot::Bar> = mz
                        .iter()
                        .zip(intensity.iter())
                        .map(|(&m, &i)| {
                            egui_plot::Bar::new(m, i.into())
                                .width(bar_width) // Adjust width of bars based on zoom level
                                .fill(self.user_input.line_color.to_egui()) // Adjust color as needed
                                .name(format!("m/z = {:.4}", m))
                        })
                        .collect();

                    plot_ui.bar_chart(egui_plot::BarChart::new(adjusted_bars));
                })
                .response;

            response
        } else {
            ui.label("No mass spectrum data available")
        }
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
                    self.user_input.file_path = None; // clears the file_path if new file is opened
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
            let slider = egui::Slider::new(&mut self.user_input.smoothing, 0..=11);
            let response = ui.add(slider);
            if response.changed() {
                self.state_changed = StateChange::Changed;
            }
            response.on_hover_text("Adjust the level of moving average smoothing");
        });

        ui.menu_button("Line width", |ui| {
            let slider = egui::Slider::new(&mut self.user_input.line_width, 0.1..=5.0);
            let response = ui.add(slider);
            if response.changed() {
                self.state_changed = StateChange::Changed;
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
            ui.radio_value(&mut self.user_input.line_color, LineColor::Red, "Red");
            ui.radio_value(&mut self.user_input.line_color, LineColor::Blue, "Blue");
            ui.radio_value(&mut self.user_input.line_color, LineColor::Green, "Green");
            ui.radio_value(&mut self.user_input.line_color, LineColor::Yellow, "Yellow");
            ui.radio_value(&mut self.user_input.line_color, LineColor::Black, "Black");
            ui.radio_value(&mut self.user_input.line_color, LineColor::White, "White");
        });
    }

    fn add_line_style_options(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.user_input.line_type, LineType::Solid, "Solid");
            ui.radio_value(&mut self.user_input.line_type, LineType::Dashed, "Dashed");
            ui.radio_value(&mut self.user_input.line_type, LineType::Dotted, "Dotted");
        });
    }

    fn handle_file_selection(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.update_file_path_and_validity(path);
        } else {
            self.invalid_file = FileValidity::Invalid;
        }
    }

    fn update_file_path_and_validity(&mut self, path: PathBuf) {
        let file_path_str = path.display().to_string();
        if file_path_str.ends_with(FILE_FORMAT) {
            self.invalid_file = FileValidity::Valid;
            self.user_input.file_path = Some(file_path_str.clone());
            self.parsed_ms_data = parser::MzData::default();
            self.parsed_ms_data.open_msfile(path).ok();
        } else {
            self.invalid_file = FileValidity::Invalid;
        }
    }

    fn update_file_information_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("file_information_panel").show(ctx, |ui| {
            ui.label("Opened file:");
            ui.separator();

            match self.invalid_file {
                FileValidity::Invalid => {
                    ui.colored_label(
                        Color32::LIGHT_RED,
                        format!("Invalid file type. Please select an {} file.", FILE_FORMAT),
                    );
                }
                FileValidity::Valid => match self.user_input.file_path {
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
                            self.user_input.file_path = None;
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
                .radio_value(
                    &mut self.user_input.polarity,
                    ScanPolarity::Positive,
                    "Positive",
                )
                .clicked()
            {
                self.user_input.polarity = ScanPolarity::Positive;
                self.state_changed = StateChange::Changed;
            }
            if ui
                .radio_value(
                    &mut self.user_input.polarity,
                    ScanPolarity::Negative,
                    "Negative",
                )
                .clicked()
            {
                self.user_input.polarity = ScanPolarity::Negative;
                self.state_changed = StateChange::Changed;
            }
        });
    }

    fn add_plot_type_options(&mut self, ui: &mut Ui) {
        ui.label("Plot Type");
        ui.horizontal(|ui| {
            if ui
                .radio_value(&mut self.user_input.plot_type, PlotType::Tic, "TIC")
                .clicked()
            {
                self.user_input.plot_type = PlotType::Tic;
                self.state_changed = StateChange::Changed;
            }
            if ui
                .radio_value(&mut self.user_input.plot_type, PlotType::Bpc, "Base Peak")
                .clicked()
            {
                self.user_input.plot_type = PlotType::Bpc;
                self.state_changed = StateChange::Changed;
            }
            if ui
                .radio_value(&mut self.user_input.plot_type, PlotType::Xic, "XIC")
                .clicked()
            {
                self.user_input.plot_type = PlotType::Xic;
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
                            egui::TextEdit::singleline(&mut self.user_input.mass_input)
                                .hint_text("Enter m/z"),
                        )
                        .lost_focus()
                    {
                        self.user_input.mass = self
                            .user_input
                            .mass_input
                            .parse()
                            .unwrap_or(self.user_input.mass);
                        self.state_changed = StateChange::Changed;
                    };
                    if ui
                        .add(
                            egui::TextEdit::singleline(&mut self.user_input.mass_tolerance_input)
                                .hint_text("Enter mass tolerance in mmu"),
                        )
                        .lost_focus()
                    {
                        self.user_input.mass_tolerance = self
                            .user_input
                            .mass_tolerance_input
                            .parse()
                            .unwrap_or(self.user_input.mass_tolerance);
                        self.state_changed = StateChange::Changed
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
