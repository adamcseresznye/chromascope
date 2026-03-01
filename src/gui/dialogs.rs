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
        // Use a local copy for `.open()` to avoid a simultaneous mutable borrow
        // of `app` inside the closure (needed to set `app.options_window_open = false`
        // from the Confirm button).
        let mut window_open = true;
        let mut close_on_confirm = false;

        egui::Window::new("XIC settings")
            .open(&mut window_open)
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
                ui.label("m/z:");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut app.user_input.mass.text)
                            .hint_text("Enter m/z"),
                    )
                    .lost_focus()
                {
                    // Capture text before sync (sync may revert it on failure).
                    let original_mass_text = app.user_input.mass.text.clone();
                    // Use the pending tolerance text for cross-field validation so that
                    // typing tolerance first does not block mass from being committed.
                    let pending_tolerance: f64 = app
                        .user_input
                        .mass_tolerance
                        .text
                        .parse()
                        .unwrap_or(app.user_input.mass_tolerance.value);
                    let valid = app.user_input.mass.sync_on_focus_lost(|m| {
                        let temp_bounds = crate::validation::DataBounds::unrestricted();
                        XicParams::new(*m, app.user_input.polarity, pending_tolerance, &temp_bounds)
                            .is_ok()
                    });
                    match valid {
                        Ok(()) => {
                            app.state_changed = StateChange::Changed;
                        }
                        // Only report an error if the user actually typed something.
                        // An empty field on first open should not produce a dialog.
                        Err(_) if !original_mass_text.trim().is_empty() => {
                            error!("Invalid mass value: {}", original_mass_text);
                            error_message = Some(format!("Invalid m/z: '{}'", original_mass_text));
                        }
                        _ => {}
                    }
                };
                ui.label("Mass tolerance (ppm):");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut app.user_input.mass_tolerance.text)
                            .hint_text("Enter mass tolerance in ppm"),
                    )
                    .lost_focus()
                {
                    // Capture text before sync so we can report what the user typed.
                    let original_tol_text = app.user_input.mass_tolerance.text.clone();
                    let current_mass = app.user_input.mass.value;
                    // Validate: tolerance must be in range for XicParams
                    let valid = app.user_input.mass_tolerance.sync_on_focus_lost(|t| {
                        let temp_bounds = crate::validation::DataBounds::unrestricted();
                        XicParams::new(current_mass, app.user_input.polarity, *t, &temp_bounds)
                            .is_ok()
                    });
                    match valid {
                        Ok(()) => {
                            app.state_changed = StateChange::Changed;
                        }
                        // Only report an error if the user actually typed something.
                        Err(_) if !original_tol_text.trim().is_empty() => {
                            error!("Invalid mass tolerance: {}", original_tol_text);
                            error_message =
                                Some(format!("Invalid mass tolerance: '{}'", original_tol_text));
                        }
                        _ => {}
                    }
                };

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(4.0);

                // "Confirm" button — forces both fields to sync even when the
                // TextEdit still has focus (i.e. the user hasn't clicked away yet).
                // Without this button, closing the window via the X while the
                // tolerance field is still focused would leave mass_tolerance.value
                // at its default (0.0), causing an "invalid mass tolerance 0" error.
                if ui.button("Confirm").clicked() {
                    // --- sync mass ---
                    let pending_tolerance: f64 = app
                        .user_input
                        .mass_tolerance
                        .text
                        .parse()
                        .unwrap_or(app.user_input.mass_tolerance.value);
                    let mass_valid = app.user_input.mass.sync_on_focus_lost(|m| {
                        let temp_bounds = crate::validation::DataBounds::unrestricted();
                        XicParams::new(*m, app.user_input.polarity, pending_tolerance, &temp_bounds)
                            .is_ok()
                    });

                    // --- sync mass_tolerance ---
                    let current_mass = app.user_input.mass.value;
                    let tol_valid = app.user_input.mass_tolerance.sync_on_focus_lost(|t| {
                        let temp_bounds = crate::validation::DataBounds::unrestricted();
                        XicParams::new(current_mass, app.user_input.polarity, *t, &temp_bounds)
                            .is_ok()
                    });

                    match (mass_valid, tol_valid) {
                        (Ok(()), Ok(())) => {
                            app.state_changed = StateChange::Changed;
                            close_on_confirm = true;
                        }
                        (Err(_), _) => {
                            error_message =
                                Some(format!("Invalid mass: '{}'", app.user_input.mass.text));
                        }
                        (_, Err(_)) => {
                            error_message = Some(format!(
                                "Invalid mass tolerance: '{}'",
                                app.user_input.mass_tolerance.text
                            ));
                        }
                    }
                }
            });

        // Propagate window-open state back from the local copy.
        // The X button sets window_open=false; the Confirm button sets close_on_confirm=true.
        if !window_open || close_on_confirm {
            app.options_window_open = false;
        }

        // Show error dialog outside of the closure to avoid borrow checker issues
        if let Some(msg) = error_message {
            app.error_message = Some(msg);
        }
    }
}

/// Renders the m/z range filter window when open.
///
/// This is a persistent `egui::Window` (not a context-menu popup) so that
/// `TextEdit` widgets can receive keyboard input without the container closing.
pub fn render_range_window(app: &mut MzViewerApp, ctx: &egui::Context) {
    if !app.user_input.range_window_open {
        return;
    }

    let mut error_message: Option<String> = None;

    egui::Window::new("m/z Range Filter")
        .open(&mut app.user_input.range_window_open)
        .resizable(false)
        .collapsible(false)
        .show(ctx, |ui| {
            // Enable checkbox
            if ui
                .checkbox(&mut app.user_input.range_enabled, "Enable range filtering")
                .changed()
            {
                app.state_changed = StateChange::Changed;
            }

            if app.user_input.range_enabled {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Min:");
                    let min_resp = ui.add(
                        egui::TextEdit::singleline(&mut app.user_input.range_min.text)
                            .desired_width(80.0)
                            .hint_text("0.0"),
                    );
                    if min_resp.lost_focus() {
                        match app.user_input.range_min.sync_on_focus_lost(|v| *v >= 0.0) {
                            Ok(()) => {
                                app.state_changed = StateChange::Changed;
                                log::info!("Range min set to: {}", app.user_input.range_min.value);
                            }
                            Err(_) if !app.user_input.range_min.text.is_empty() => {
                                error_message = Some("Min m/z must be non-negative".to_string());
                            }
                            _ => {}
                        }
                    }

                    ui.label("- Max:");
                    let max_resp = ui.add(
                        egui::TextEdit::singleline(&mut app.user_input.range_max.text)
                            .desired_width(80.0)
                            .hint_text("2000.0"),
                    );
                    if max_resp.lost_focus() {
                        let min_val = app.user_input.range_min.value;
                        match app
                            .user_input
                            .range_max
                            .sync_on_focus_lost(|v| *v > min_val)
                        {
                            Ok(()) => {
                                app.state_changed = StateChange::Changed;
                                log::info!("Range max set to: {}", app.user_input.range_max.value);
                            }
                            Err(_) if !app.user_input.range_max.text.is_empty() => {
                                error_message =
                                    Some("Max m/z must be greater than min m/z".to_string());
                            }
                            _ => {}
                        }
                    }
                });

                // Show file's actual m/z range as a hint
                if let Some(active_id) = app.active_file_id {
                    if let Some(file) = app.files.get(&active_id) {
                        let b = &file.data.bounds;
                        ui.add_space(2.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "File range: {:.0} \u{2013} {:.0} m/z",
                                b.min_mz, b.max_mz
                            ))
                            .small()
                            .color(egui::Color32::GRAY),
                        );
                    }
                }
            }
        });

    // Deferred error display — avoids borrow conflict inside the window closure
    if let Some(msg) = error_message {
        app.show_error_dialog(msg);
    }
}

/// Renders the Plot Properties window when open.
///
/// Uses a persistent `egui::Window` so that `ComboBox` dropdowns inside it
/// don't close when clicked (unlike `context_menu` which closes on any click).
pub fn render_plot_properties_window(app: &mut MzViewerApp, ctx: &egui::Context) {
    if !app.plot_properties_open {
        return;
    }
    // Use a local copy for `.open()` to avoid a simultaneous mutable borrow
    // of `app` inside the closure (needed for `add_plot_properties`).
    let mut open = true;
    egui::Window::new("Plot Properties")
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .show(ctx, |ui| {
            ui.separator();
            super::panels::add_plot_properties(app, ui);
            ui.separator();
        });
    app.plot_properties_open = open;
}
