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
                        egui::TextEdit::singleline(&mut app.user_input.mass.text)
                            .hint_text("Enter m/z"),
                    )
                    .lost_focus()
                {
                    let current_mass = app.user_input.mass.value;
                    let current_tolerance = app.user_input.mass_tolerance.value;
                    // Validate: mass must be positive and pass XicParams validation
                    let valid = app.user_input.mass.sync_on_focus_lost(|m| {
                        let temp_bounds = crate::validation::DataBounds::unrestricted();
                        XicParams::new(*m, app.user_input.polarity, current_tolerance, &temp_bounds).is_ok()
                    });
                    match valid {
                        Ok(()) => {
                            app.state_changed = StateChange::Changed;
                        }
                        Err(_) => {
                            error!("Invalid mass value: {}", app.user_input.mass.text);
                            error_message = Some(format!("Invalid mass: '{}'", current_mass));
                        }
                    }
                };
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut app.user_input.mass_tolerance.text)
                            .hint_text("Enter mass tolerance in ppm"),
                    )
                    .lost_focus()
                {
                    let current_mass = app.user_input.mass.value;
                    // Validate: tolerance must be in range for XicParams
                    let valid = app.user_input.mass_tolerance.sync_on_focus_lost(|t| {
                        let temp_bounds = crate::validation::DataBounds::unrestricted();
                        XicParams::new(current_mass, app.user_input.polarity, *t, &temp_bounds).is_ok()
                    });
                    match valid {
                        Ok(()) => {
                            app.state_changed = StateChange::Changed;
                        }
                        Err(_) => {
                            error!("Invalid mass tolerance: {}", app.user_input.mass_tolerance.text);
                            error_message = Some(format!(
                                "Invalid mass tolerance: '{}'",
                                app.user_input.mass_tolerance.text
                            ));
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
