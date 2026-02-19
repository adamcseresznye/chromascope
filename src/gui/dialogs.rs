use crate::gui::state::{MzViewerApp, StateChange};
use crate::validation::XicParams;
use eframe::egui;
use log::error;

/// Renders the error dialog if an error message is present.
///
/// This creates a centered modal window with the error message and an OK button.
/// The dialog blocks interaction until dismissed by clicking OK.
pub fn render_error_dialog(app: &mut MzViewerApp, ctx: &egui::Context) {
    if let Some(error) = app.error_message.clone() {
        egui::Window::new("⚠ Error")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.colored_label(egui::Color32::RED, &error);
                ui.add_space(10.0);
                if ui.button("OK").clicked() {
                    app.error_message = None;
                }
            });
    }
}

/// Renders the XIC settings modal window when it is open.
pub fn render_xic_settings_window(app: &mut MzViewerApp, ctx: &egui::Context) {
    if app.options_window_open {
        let mut error_message: Option<String> = None;

        egui::Window::new("XIC settings")
            .open(&mut app.options_window_open)
            .show(ctx, |ui| {
                ui.label("Enter m/z and mass tolerance values in ppm:");

                ui.add_space(5.0);
                ui.separator();

                // Show valid ranges if file is opened
                if let Some(active_id) = app.active_file_id {
                    if let Some(file) = app.files.get(&active_id) {
                        let bounds = &file.data.bounds;

                        ui.add_space(5.0);
                        ui.separator();

                        // Show m/z range
                        ui.horizontal(|ui| {
                            ui.label("📊 Valid m/z range:");
                            ui.label(
                                egui::RichText::new(format!(
                                    "{:.2} - {:.2}",
                                    bounds.min_mz, bounds.max_mz
                                ))
                                .color(egui::Color32::from_rgb(100, 149, 237)),
                            );
                        });

                        // Show RT range (informational)
                        ui.horizontal(|ui| {
                            ui.label("⏱  File RT range:");
                            ui.label(
                                egui::RichText::new(format!(
                                    "{:.2} - {:.2} min",
                                    bounds.min_rt, bounds.max_rt
                                ))
                                .color(egui::Color32::from_rgb(100, 149, 237)),
                            );
                        });

                        // Show scan count
                        ui.horizontal(|ui| {
                            ui.label("📈 Total scans:");
                            ui.label(
                                egui::RichText::new(format!("{}", bounds.scan_count))
                                    .color(egui::Color32::from_rgb(100, 149, 237)),
                            );
                        });

                        ui.separator();
                        ui.add_space(5.0);
                    }
                } else {
                    ui.add_space(5.0);
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 140, 0),
                        "⚠ Open a file to see valid parameter ranges",
                    );
                    ui.add_space(5.0);
                }
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut app.user_input.mass_input)
                            .hint_text("Enter m/z"),
                    )
                    .lost_focus()
                {
                    match app.user_input.mass_input.parse::<f64>() {
                        Ok(parsed_mass) => {
                            let temp_bounds = crate::validation::DataBounds::unrestricted();
                            match XicParams::new(
                                parsed_mass,
                                app.user_input.polarity,
                                app.user_input.mass_tolerance,
                                &temp_bounds,
                            ) {
                                Ok(_) => {
                                    app.user_input.mass = parsed_mass;
                                    app.state_changed = StateChange::Changed;
                                }
                                Err(e) => {
                                    error!("Invalid mass value: {}", e);
                                    error_message = Some(format!("Invalid mass: {}", e));
                                    app.user_input.mass_input = app.user_input.mass.to_string();
                                }
                            }
                        }
                        Err(_) => {
                            error!("Failed to parse mass input: {}", app.user_input.mass_input);
                            error_message = Some(format!(
                                "Invalid number format: '{}'",
                                app.user_input.mass_input
                            ));
                            app.user_input.mass_input = app.user_input.mass.to_string();
                        }
                    }
                };
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut app.user_input.mass_tolerance_input)
                            .hint_text("Enter mass tolerance in ppm"),
                    )
                    .lost_focus()
                {
                    match app.user_input.mass_tolerance_input.parse::<f64>() {
                        Ok(parsed_tolerance) => {
                            let temp_bounds = crate::validation::DataBounds::unrestricted();
                            match XicParams::new(
                                app.user_input.mass,
                                app.user_input.polarity,
                                parsed_tolerance,
                                &temp_bounds,
                            ) {
                                Ok(_) => {
                                    app.user_input.mass_tolerance = parsed_tolerance;
                                    app.state_changed = StateChange::Changed;
                                }
                                Err(e) => {
                                    error!("Invalid mass tolerance: {}", e);
                                    error_message = Some(format!("Invalid mass tolerance: {}", e));
                                    app.user_input.mass_tolerance_input =
                                        app.user_input.mass_tolerance.to_string();
                                }
                            }
                        }
                        Err(_) => {
                            error!(
                                "Failed to parse mass tolerance input: {}",
                                app.user_input.mass_tolerance_input
                            );
                            error_message = Some(format!(
                                "Invalid number format: '{}'",
                                app.user_input.mass_tolerance_input
                            ));
                            app.user_input.mass_tolerance_input =
                                app.user_input.mass_tolerance.to_string();
                        }
                    }
                };
            });

        // Show error dialog outside of the closure to avoid borrow checker issues
        if let Some(msg) = error_message {
            app.error_message = Some(msg);
        }
    }
}
