//! Shared editor for one analyte's integration settings (review card and method window).
use crate::integration::{Algorithm, BaselineKind, Integration, PeakSelection};
use eframe::egui;

fn row(ui: &mut egui::Ui, label: &str, tip: &str, add: impl FnOnce(&mut egui::Ui) -> bool) -> bool {
    ui.label(label).on_hover_text(tip);
    let changed = add(ui);
    ui.end_row();
    changed
}
fn number(
    ui: &mut egui::Ui,
    value: &mut f64,
    speed: f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
    tip: &str,
) -> bool {
    ui.add(
        egui::DragValue::new(value)
            .speed(speed)
            .range(range)
            .suffix(suffix),
    )
    .on_hover_text(tip)
    .changed()
}

/// Edits `cfg`; returns true when any setting changed. Only the settings the chosen
/// algorithm uses are shown. `rt_window` supplies the defaults of the fixed window.
pub(super) fn editor(ui: &mut egui::Ui, cfg: &mut Integration, rt_window: [f64; 2]) -> bool {
    let mut changed = false;
    // Inline choices: dropdowns close the menu this editor may be shown in.
    ui.horizontal_wrapped(|ui| {
        for a in Algorithm::ALL {
            changed |= super::widgets::segment_value(ui, &mut cfg.algorithm, a, a.label()).changed();
        }
    });
    ui.small(cfg.algorithm.description());
    ui.add_space(2.0);
    egui::Grid::new("integration_params")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            match cfg.algorithm {
                Algorithm::Manual => {
                    let mut start = cfg.manual_start.unwrap_or(rt_window[0]);
                    let mut end = cfg.manual_end.unwrap_or(rt_window[1]);
                    if row(ui, "Window start", "Integration starts here (minutes). Defaults to the analyte RT window.", |ui| {
                        number(ui, &mut start, 0.01, 0.0..=1e6, " min", "Start of the fixed integration window")
                    }) {
                        cfg.manual_start = Some(start);
                        changed = true;
                    }
                    if row(ui, "Window end", "Integration ends here (minutes). Defaults to the analyte RT window.", |ui| {
                        number(ui, &mut end, 0.01, 0.0..=1e6, " min", "End of the fixed integration window")
                    }) {
                        cfg.manual_end = Some(end);
                        changed = true;
                    }
                }
                alg => {
                    if alg == Algorithm::SavgolSnr {
                        let mut window = cfg.sg_window;
                        if row(ui, "Smoothing window", "Savitzky–Golay window in scans (odd).", |ui| {
                            ui.add(egui::DragValue::new(&mut window).range(3..=501))
                                .on_hover_text("Odd number of scans; larger smooths more")
                                .changed()
                        }) {
                            cfg.sg_window = window | 1;
                            changed = true;
                        }
                        changed |= row(ui, "Polynomial degree", "Degree of the local Savitzky–Golay polynomial (2–3 is typical).", |ui| {
                            ui.add(egui::DragValue::new(&mut cfg.sg_degree).range(0..=5))
                                .changed()
                        });
                        changed |= row(ui, "Minimum SNR", "The peak must rise this many noise standard deviations above its base.", |ui| {
                            number(ui, &mut cfg.minimum_snr, 0.1, 0.0..=1e6, "", "Signal-to-noise threshold")
                        });
                        changed |= row(ui, "Minimum width", "Narrower peaks (full width at half height) are rejected. 0 = off.", |ui| {
                            number(ui, &mut cfg.minimum_width_minutes, 0.001, 0.0..=1e3, " min", "Minimum FWHM; 0 disables")
                        });
                        changed |= row(ui, "Maximum width", "Wider peaks (full width at half height) are rejected. 0 = off.", |ui| {
                            number(ui, &mut cfg.maximum_width_minutes, 0.01, 0.0..=1e3, " min", "Maximum FWHM; 0 disables")
                        });
                    } else {
                        let mut smoothing = cfg.smoothing as i32;
                        if row(ui, "Smoothing", "Moving-average half-width in scans used for peak detection only; areas use unsmoothed data.", |ui| {
                            ui.add(egui::DragValue::new(&mut smoothing).range(0..=10))
                                .on_hover_text("0 = no smoothing")
                                .changed()
                        }) {
                            cfg.smoothing = smoothing as u8;
                            changed = true;
                        }
                    }
                    changed |= row(ui, "Minimum prominence", "A peak must rise this far above its higher neighbouring valley (instrument intensity).", |ui| {
                        number(ui, &mut cfg.minimum_prominence, 100.0, 0.0..=f64::MAX, "", "Minimum height above the surrounding valleys")
                    });
                    changed |= row(ui, "Boundary fraction", "The peak ends where the signal falls to this fraction of its height. Smaller values extend into the tail.", |ui| {
                        number(ui, &mut cfg.boundary_fraction, 0.005, 0.0..=0.5, "", "0.01 = integrate down to 1% of the peak height")
                    });
                    if alg == Algorithm::Valley {
                        changed |= row(ui, "Valley depth", "Neighbouring peaks are split at their valley when the dip is deeper than this fraction of the peak height.", |ui| {
                            number(ui, &mut cfg.valley_depth, 0.01, 0.0..=1.0, "", "Smaller values split more readily")
                        });
                    }
                    row(ui, "Peak selection", "Which candidate to report when several peaks are found in the window.", |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for s in PeakSelection::ALL {
                                changed |= super::widgets::segment_value(ui, &mut cfg.peak_selection, s, s.label()).changed();
                            }
                        });
                        false
                    });
                }
            }
            row(ui, "Baseline", "How the area under the peak is corrected for background.", |ui| {
                ui.horizontal_wrapped(|ui| {
                    for b in BaselineKind::ALL {
                        changed |= super::widgets::segment_value(ui, &mut cfg.baseline, b, b.label()).changed();
                    }
                });
                false
            });
            if cfg.baseline == BaselineKind::Als {
                changed |= row(ui, "ALS smoothness (λ)", "Larger values give a stiffer, smoother baseline.", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut cfg.als_lambda)
                            .speed(1000.0)
                            .range(1.0..=1e12),
                    )
                    .changed()
                });
                changed |= row(ui, "ALS asymmetry", "Weight of points above the baseline (0–0.5); smaller keeps the baseline under the peak.", |ui| {
                    number(ui, &mut cfg.als_asymmetry, 0.001, 0.0001..=0.4999, "", "Typical 0.001–0.05")
                });
                changed |= row(ui, "ALS iterations", "Reweighting passes.", |ui| {
                    ui.add(egui::DragValue::new(&mut cfg.als_iterations).range(1..=100))
                        .changed()
                });
            }
        });
    changed
}

/// One-word algorithm name for compact tables.
pub(super) fn short_name(cfg: Option<&Integration>) -> &'static str {
    match cfg.map(|c| c.algorithm) {
        None => "Default",
        Some(Algorithm::Classic) => "Classic",
        Some(Algorithm::Valley) => "Valley",
        Some(Algorithm::SavgolSnr) => "SG + SNR",
        Some(Algorithm::Manual) => "Fixed window",
    }
}
