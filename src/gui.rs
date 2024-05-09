use crate::parser;
use crate::prepare_for_plot;
use mzdata::spectrum::ScanPolarity;

use eframe::egui;
use egui_plot::{Legend, Line, Plot, PlotPoints};
use rfd;

#[derive(Default)]
pub struct MyEguiApp {
    file_path: Option<String>,
    plot_data: Option<Vec<[f64; 2]>>,
}

impl MyEguiApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("My top panel").show(ctx, |ui| {
            if ui.button("🗀").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    self.file_path = Some(path.display().to_string());
                    let mut tic =
                        parser::get_tic(&self.file_path.as_ref().unwrap(), ScanPolarity::Positive);
                    if tic
                        .as_ref()
                        .map_or(false, |mzdata| mzdata.mzdata_is_empty())
                    {
                        tic = parser::get_tic(
                            &self.file_path.as_ref().unwrap(),
                            ScanPolarity::Negative,
                        );
                    }
                    if let Ok(data) = prepare_for_plot(tic) {
                        self.plot_data = Some(data);
                    } else {
                        egui::Window::new("Error").show(ui.ctx(), |ui| {
                            ui.label("MzData cannot be displayed.");
                        });
                    }
                }
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let my_plot = Plot::new("My Plot").legend(Legend::default());

            if let Some(plot_data) = &self.plot_data {
                let plot_response = my_plot.show(ui, |plot_ui| {
                    plot_ui.line(Line::new(PlotPoints::from(plot_data.clone())).name("curve"));
                });

                plot_response.response.context_menu(|ui| {
                    ui.menu_button("My menu", |ui| {
                        if ui.button("Close the menu").clicked() {
                            ui.close_menu();
                        }
                    });
                });
            }
        });
    }
}
