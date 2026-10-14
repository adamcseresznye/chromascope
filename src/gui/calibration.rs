//! Calibration review: pick the curve type and weighting, include or exclude standards and see
//! the fitted curve, back-calculated accuracy and a comparison of the alternative fits at once.
//! Everything here refits the standards already measured; no raw data are read again.
use crate::targeted as t;
use eframe::egui;
use eframe::egui::Color32;
use egui_plot::{HLine, Line, MarkerShape, Points, VLine};
use std::collections::BTreeSet;

const EXCLUDED: &str = "Excluded during calibration review";
pub(super) const FITS: [&str; 6] = [
    "Linear",
    "Quadratic",
    "Cubic (review needed)",
    "Log-log",
    "Average response factor",
    "Point-to-point",
];

/// Index into [`FITS`] describing a configuration.
pub(super) fn fit_choice(cfg: &t::CalibrationConfig) -> usize {
    match cfg.curve {
        t::Curve::LogLog => 3,
        t::Curve::PointToPoint => 5,
        t::Curve::Polynomial => {
            if cfg.degree == 1
                && cfg.intercept == t::Intercept::Zero
                && cfg.weighting == t::Weighting::InverseX2
            {
                4
            } else {
                cfg.degree.clamp(1, 3) - 1
            }
        }
    }
}
pub(super) fn set_fit_choice(cfg: &mut t::CalibrationConfig, choice: usize) {
    let leaving_average = fit_choice(cfg) == 4;
    match choice {
        0..=2 => {
            cfg.curve = t::Curve::Polynomial;
            cfg.degree = choice + 1;
            if leaving_average {
                cfg.intercept = t::Intercept::Free;
                cfg.weighting = t::Weighting::Unweighted;
            }
        }
        3 => cfg.curve = t::Curve::LogLog,
        4 => {
            cfg.curve = t::Curve::Polynomial;
            cfg.degree = 1;
            cfg.intercept = t::Intercept::Zero;
            cfg.weighting = t::Weighting::InverseX2;
        }
        _ => cfg.curve = t::Curve::PointToPoint,
    }
}
fn intercept_label(i: &t::Intercept) -> &'static str {
    match i {
        t::Intercept::Free => "Fitted",
        t::Intercept::Zero => "Through zero",
        t::Intercept::Fixed(_) => "Fixed at…",
    }
}
/// Curve type, weighting and origin dropdowns for one calibration. Returns whether anything
/// changed. Options that do not apply to the chosen curve are hidden.
pub(super) fn fit_controls(ui: &mut egui::Ui, id: impl std::hash::Hash, cfg: &mut t::CalibrationConfig) -> bool {
    let mut changed = false;
    let choice = fit_choice(cfg);
    ui.horizontal_wrapped(|ui| {
        ui.label("Fit");
        egui::ComboBox::from_id_salt((&id, "fit"))
            .selected_text(FITS[choice])
            .show_ui(ui, |ui| {
                for (i, name) in FITS.iter().enumerate() {
                    if ui.selectable_label(choice == i, *name).clicked() && choice != i {
                        set_fit_choice(cfg, i);
                        changed = true;
                    }
                }
            });
        let weighted = matches!(choice, 0..=3);
        if weighted {
            ui.label("Weighting");
            egui::ComboBox::from_id_salt((&id, "weighting"))
                .selected_text(cfg.weighting.label())
                .show_ui(ui, |ui| {
                    for w in [
                        t::Weighting::Unweighted,
                        t::Weighting::InverseX,
                        t::Weighting::InverseX2,
                        t::Weighting::InverseY,
                        t::Weighting::InverseY2,
                    ] {
                        changed |= ui
                            .selectable_value(&mut cfg.weighting, w.clone(), w.label())
                            .changed();
                    }
                });
        }
        if choice < 3 {
            ui.label("Origin");
            egui::ComboBox::from_id_salt((&id, "origin"))
                .selected_text(intercept_label(&cfg.intercept))
                .show_ui(ui, |ui| {
                    for i in [t::Intercept::Free, t::Intercept::Zero, t::Intercept::Fixed(0.0)] {
                        changed |= ui
                            .selectable_value(&mut cfg.intercept, i.clone(), intercept_label(&i))
                            .changed();
                    }
                });
            if let t::Intercept::Fixed(value) = &mut cfg.intercept {
                changed |= ui.add(egui::DragValue::new(value).speed(0.01)).changed();
            }
        }
    });
    changed
}

struct Standard {
    sample: String,
    x: f64,
    response: Option<f64>,
}
fn standards(batch: &t::BatchResult, target: &str) -> Vec<Standard> {
    batch
        .request
        .samples
        .iter()
        .filter(|s| s.role == t::Role::Standard)
        .filter_map(|s| {
            Some(Standard {
                sample: s.id.clone(),
                x: *s.nominal.get(target)?,
                response: batch
                    .results
                    .iter()
                    .find(|r| r.sample == s.id && r.target == target)
                    .and_then(|r| r.response),
            })
        })
        .collect()
}
fn worst_deviation(m: &t::Calibration) -> Option<f64> {
    m.points
        .iter()
        .filter_map(|p| p.accuracy_percent)
        .map(|a| (a - 100.0).abs())
        .fold(None, |m, v| Some(m.map_or(v, |m: f64| m.max(v))))
}
fn equation(m: &t::Calibration) -> String {
    let k = |v: f64| format!("{v:.4e}");
    match m.curve {
        t::Curve::Polynomial => {
            let terms: Vec<String> = m
                .coefficients
                .iter()
                .enumerate()
                .map(|(i, c)| match i {
                    0 => k(*c),
                    1 => format!("{} x", k(c / m.scale)),
                    n => format!("{} x^{n}", k(c / m.scale.powi(n as i32))),
                })
                .collect();
            format!("y = {}", terms.join(" + "))
        }
        t::Curve::LogLog => format!(
            "y = {} · x^{:.4}",
            k((m.coefficients[0] - m.coefficients[1] * m.scale.ln()).exp()),
            m.coefficients[1]
        ),
        t::Curve::PointToPoint => format!("{} straight segments between levels", m.knots.len() - 1),
    }
}

#[derive(Default)]
pub(super) struct State {
    target: String,
    batch: Option<uuid::Uuid>,
    cfg: Option<t::CalibrationConfig>,
    excluded: BTreeSet<String>,
    error: String,
}
impl State {
    fn reset(&mut self, batch: &t::BatchResult) {
        let target = batch
            .request
            .targets
            .iter()
            .find(|t| t.id == self.target && t.calibration.is_some())
            .or_else(|| batch.request.targets.iter().find(|t| t.calibration.is_some()));
        self.batch = Some(batch.batch_id);
        let Some(target) = target else {
            return;
        };
        self.target = target.id.clone();
        self.cfg = target.calibration.clone();
        self.excluded = batch
            .request
            .samples
            .iter()
            .filter(|s| s.exclusions.contains_key(&target.id))
            .map(|s| s.id.clone())
            .collect();
    }
}

/// Calibration curve of `target` (Xcalibur-style): title with equation, fit controls, the curve
/// with standards, QCs and unknowns and a crosshair on `selected_sample`, then the standards
/// and a comparison of fits. Changes are previewed; `Apply as new run` returns the recalculated
/// batch for the caller to retain.
pub(super) fn curve_pane(
    state: &mut State,
    batch: &t::BatchResult,
    target: &str,
    selected_sample: &str,
    plot_height: f32,
    ui: &mut egui::Ui,
) -> Option<t::BatchResult> {
    let Some(current) = batch.request.targets.iter().find(|t| t.id == target) else {
        return None;
    };
    let name = current.quantifier.extraction.extraction.name.clone();
    if current.calibration.is_none() {
        ui.strong(&name);
        ui.label("Internal standard: measured in every injection, not calibrated.");
        return None;
    }
    if state.target != target || state.batch != Some(batch.batch_id) || state.cfg.is_none() {
        state.target = target.to_owned();
        state.reset(batch);
    }
    let mut applied: Option<t::BatchResult> = None;
    let Some(mut cfg) = state.cfg.clone() else {
        return None;
    };
    let rows = standards(batch, target);
    let usable = |r: &Standard| r.response.is_some() && !state.excluded.contains(&r.sample);
    let points: Vec<(String, f64, f64)> = rows
        .iter()
        .filter(|r| usable(r))
        .map(|r| (r.sample.clone(), r.x, r.response.unwrap()))
        .collect();
    let mut changed = fit_controls(ui, "cal_fit", &mut cfg);
    // The range can only cover concentrations that are among the included standards.
    let low = points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let high = points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    if low.is_finite() && (cfg.range[0] < low || cfg.range[1] > high) {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("Range {}–{} is wider than the included standards ({low}–{high}).", cfg.range[0], cfg.range[1]),
            );
            if ui.small_button("Fit range to standards").clicked() {
                cfg.range = [low, high];
                cfg.loq = cfg.loq.clamp(cfg.lod, high);
                changed = true;
            }
        });
    }
    let model = t::fit(&points, &cfg);
    let original = current.calibration.clone();
    let original_excluded: BTreeSet<String> = batch
        .request
        .samples
        .iter()
        .filter(|s| s.exclusions.contains_key(target))
        .map(|s| s.id.clone())
        .collect();
    let differs = original.as_ref() != Some(&cfg) || original_excluded != state.excluded;
    if differs {
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(ui.visuals().warn_fg_color, "Preview — not applied yet.");
            if ui
                .add_enabled(model.is_ok(), egui::Button::new("Apply as new run"))
                .on_hover_text("Recalculates all concentrations with this calibration from the measured peaks and keeps it as a new run. Reviews on the earlier run are not carried over.")
                .clicked()
            {
                let mut request = batch.request.clone();
                for t in request.targets.iter_mut().filter(|t| t.id == target) {
                    t.calibration = Some(cfg.clone());
                }
                for s in request.samples.iter_mut().filter(|s| s.role == t::Role::Standard) {
                    if state.excluded.contains(&s.id) {
                        s.exclusions.entry(target.to_owned()).or_insert_with(|| EXCLUDED.into());
                    } else {
                        s.exclusions.remove(target);
                    }
                }
                match t::evaluate(request, batch.observations.clone(), batch.source_hashes.clone()) {
                    Ok(next) => {
                        state.error.clear();
                        applied = Some(next)
                    }
                    Err(e) => state.error = e.to_string(),
                }
            }
            if ui.button("Reset").clicked() {
                state.batch = None;
            }
        });
    }
    if !state.error.is_empty() {
        ui.colored_label(ui.visuals().error_fg_color, &state.error);
    }
    match &model {
        Ok(m) => {
            ui.label(egui::RichText::new(format!(
                "{name}   {}   R² = {}   W: {}",
                equation(m),
                m.r_squared.map_or("n/a".into(), |r| format!("{r:.4}")),
                if cfg.curve == t::Curve::PointToPoint { "–" } else { cfg.weighting.label() },
            )).strong());
            if !m.flags.is_empty() {
                ui.colored_label(ui.visuals().warn_fg_color, m.flags.join("; "));
            }
            // Every measured sample that can be placed on the curve.
            let markers: Vec<(String, t::Role, f64, f64)> = batch
                .request
                .samples
                .iter()
                .filter_map(|s| {
                    let row = batch.results.iter().find(|r| r.sample == s.id && r.target == target)?;
                    let y = row.response?;
                    let x = match s.role {
                        t::Role::Standard | t::Role::Qc => *s.nominal.get(target)?,
                        t::Role::Unknown => row.injected_concentration?,
                        t::Role::Blank => return None,
                    };
                    Some((s.id.clone(), s.role.clone(), x, y))
                })
                .collect();
            let (xmin, xmax) = if low.is_finite() { (low.min(0.0), high) } else { (0.0, 1.0) };
            let unit = cfg.unit.label();
            let y_label = if current.internal_standard.is_some() { "Area ratio" } else { "Area" };
            let tk = super::theme::Tokens::of(ui);
            super::plot_controls::plot(ui, "cal_curve")
                .height(plot_height)
                .x_axis_label(format!("Concentration ({unit})"))
                .y_axis_label(y_label)
                .show(ui, |p| {
                    p.line(Line::new(
                        "Fit",
                        (0..=200)
                            .map(|i| {
                                let x = xmin + (xmax - xmin) * i as f64 / 200.0;
                                [x, m.response(x)]
                            })
                            .collect::<Vec<_>>(),
                    ));
                    let group = |role: t::Role, shape: MarkerShape, colour: Color32, label: &str, included: bool| {
                        let pts: Vec<[f64; 2]> = markers
                            .iter()
                            .filter(|m| m.1 == role && (role != t::Role::Standard || usable_id(&rows, &state.excluded, &m.0) == included))
                            .map(|m| [m.2, m.3])
                            .collect();
                        (!pts.is_empty()).then(|| {
                            let mut points = Points::new(label.to_owned(), pts).shape(shape).color(colour).radius(5.0_f32);
                            if !included {
                                points = points.filled(false);
                            }
                            points
                        })
                    };
                    for points in [
                        group(t::Role::Standard, MarkerShape::Square, tk.role_standard, "Standards", true),
                        group(t::Role::Standard, MarkerShape::Square, tk.role_standard, "Excluded standards", false),
                        group(t::Role::Qc, MarkerShape::Diamond, tk.role_qc, "QCs", true),
                        group(t::Role::Unknown, MarkerShape::Circle, tk.role_unknown, "Unknowns", true),
                    ]
                    .into_iter()
                    .flatten()
                    {
                        p.points(points);
                    }
                    if let Some(chosen) = markers.iter().find(|m| m.0 == selected_sample) {
                        p.vline(VLine::new("Selected", chosen.2).color(tk.error).width(1.0_f32));
                        p.hline(HLine::new("Selected", chosen.3).color(tk.error).width(1.0_f32));
                    }
                });
        }
        Err(e) => {
            ui.colored_label(ui.visuals().error_fg_color, e.to_string());
        }
    }
    ui.collapsing("Standards used and fit comparison", |ui| {
        // Standards: tick to include, see how well each is back-calculated.
        egui::Grid::new("cal_standards").striped(true).show(ui, |ui| {
            for h in ["Use", "Standard", "Known conc.", "Response", "Back-calculated", "Accuracy %"] {
                ui.strong(h);
            }
            ui.end_row();
            for r in &rows {
                let mut on = !state.excluded.contains(&r.sample) && r.response.is_some();
                if ui.add_enabled(r.response.is_some(), egui::Checkbox::without_text(&mut on)).changed() {
                    if on {
                        state.excluded.remove(&r.sample);
                    } else {
                        state.excluded.insert(r.sample.clone());
                    }
                    changed = true;
                }
                ui.label(&r.sample);
                ui.label(format!("{}", r.x));
                ui.label(r.response.map_or("no peak".into(), |y| format!("{y:.4e}")));
                let point = model
                    .as_ref()
                    .ok()
                    .and_then(|m| m.points.iter().find(|p| p.sample == r.sample));
                ui.label(point.and_then(|p| p.back_calculated).map_or("–".into(), |v| format!("{v:.4}")));
                let accuracy = point.and_then(|p| p.accuracy_percent);
                let text = accuracy.map_or("–".into(), |a| format!("{a:.1}"));
                if accuracy.is_some_and(|a| (a - 100.0).abs() > cfg.accuracy_tolerance_percent) {
                    ui.colored_label(ui.visuals().warn_fg_color, text);
                } else {
                    ui.label(text);
                }
                ui.end_row();
            }
        });
        // Alternative fits on the same included standards.
        ui.add_space(4.0);
        ui.strong("Compare fits (same weighting and origin settings)");
        let mut pick = None;
        egui::Grid::new("cal_compare").striped(true).show(ui, |ui| {
            for h in ["Fit", "R²", "Worst deviation", "Notes", ""] {
                ui.strong(h);
            }
            ui.end_row();
            for (i, name) in FITS.iter().enumerate() {
                let mut other = cfg.clone();
                set_fit_choice(&mut other, i);
                ui.label(*name);
                match t::fit(&points, &other) {
                    Ok(m) => {
                        ui.label(m.r_squared.map_or("n/a".into(), |r| format!("{r:.5}")));
                        ui.label(worst_deviation(&m).map_or("n/a".into(), |d| format!("{d:.1} %")));
                        ui.label(m.flags.join("; "));
                    }
                    Err(e) => {
                        ui.label("–");
                        ui.label("–");
                        ui.small(e.to_string());
                    }
                }
                if fit_choice(&cfg) != i && ui.small_button("Use").clicked() {
                    pick = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = pick {
            set_fit_choice(&mut cfg, i);
            changed = true;
        }
    });
    if changed {
        state.cfg = Some(cfg);
    }
    applied
}
fn usable_id(rows: &[Standard], excluded: &BTreeSet<String>, sample: &str) -> bool {
    rows.iter()
        .find(|r| r.sample == sample)
        .is_some_and(|r| r.response.is_some() && !excluded.contains(sample))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_fit_choice_round_trips() {
        let mut cfg = t::CalibrationConfig {
            curve: t::Curve::Polynomial,
            degree: 1,
            intercept: t::Intercept::Free,
            weighting: t::Weighting::Unweighted,
            unit: t::Unit::NgMl,
            range: [1.0, 100.0],
            lod: 0.0,
            loq: 1.0,
            accuracy_tolerance_percent: 15.0,
            qc_cv_limit_percent: 15.0,
            blank_response_limit: 0.0,
        };
        for i in [3, 4, 0, 1, 2, 5, 4, 1] {
            set_fit_choice(&mut cfg, i);
            assert_eq!(fit_choice(&cfg), i, "{cfg:?}");
        }
        set_fit_choice(&mut cfg, 4);
        set_fit_choice(&mut cfg, 0);
        assert_eq!(cfg.intercept, t::Intercept::Free);
        assert_eq!(cfg.weighting, t::Weighting::Unweighted);
    }
}
