//! Overviews of a calculated batch that make a wrong integration easy to spot: a sample × analyte
//! matrix coloured by how far each result can be trusted, and trend plots over the injection order.
use super::theme::Tokens;
use crate::targeted as t;
use eframe::egui::{self, Color32};
use egui_plot::{HLine, Points};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Health {
    Good,
    /// Usable but flagged: look at the peak.
    Check,
    /// No usable concentration.
    Bad,
}
pub(super) fn health(row: &t::Concentration) -> Health {
    use t::State::*;
    if matches!(row.state, Missing | Rejected | BelowRange | AboveRange | Failed | Ambiguous) {
        Health::Bad
    } else if row.state != Present
        || row.flags.iter().any(|f| !f.starts_with("calibration_excluded"))
    {
        Health::Check
    } else {
        Health::Good
    }
}
pub(super) fn colour(dark: bool, health: Health) -> Color32 {
    let t = Tokens::for_mode(dark);
    match health {
        Health::Good => t.good,
        Health::Check => t.warn,
        Health::Bad => t.error,
    }
}
#[cfg(test)]
fn cell_text(row: &t::Concentration) -> String {
    use t::State::*;
    match row.state {
        Present => row
            .concentration
            .or(row.injected_concentration)
            .map_or("–".into(), |c| format!("{c:.3}")),
        Missing => "no peak".into(),
        Rejected => "rejected".into(),
        BelowDetection => "< LOD".into(),
        BelowLoq => "< LOQ".into(),
        BelowRange => "below range".into(),
        AboveRange => "above range".into(),
        Failed => "failed".into(),
        Ambiguous => "ambiguous".into(),
    }
}
fn role_colour(dark: bool, role: &t::Role) -> Color32 {
    let t = Tokens::for_mode(dark);
    match role {
        t::Role::Standard => t.role_standard,
        t::Role::Qc => t.role_qc,
        t::Role::Blank => t.role_blank,
        t::Role::Unknown => t.role_unknown,
    }
}
fn role_label(role: &t::Role) -> &'static str {
    match role {
        t::Role::Standard => "standard",
        t::Role::Qc => "QC",
        t::Role::Blank => "blank",
        t::Role::Unknown => "unknown",
    }
}
fn analyte_name(batch: &t::BatchResult, target: &str) -> String {
    batch
        .request
        .targets
        .iter()
        .find(|t| t.id == target)
        .map_or(target.to_owned(), |t| t.quantifier.extraction.extraction.name.clone())
}

fn nearest_sample(batch: &t::BatchResult, order: f64) -> Option<&t::Sample> {
    batch
        .request
        .samples
        .iter()
        .min_by(|a, b| {
            (a.injection_order as f64 - order)
                .abs()
                .total_cmp(&(b.injection_order as f64 - order).abs())
        })
        .filter(|s| (s.injection_order as f64 - order).abs() < 0.6)
}
/// Trend plots for the analyte of the selected result, over the injection order: response,
/// internal-standard area (when used) and accuracy of the standards and QCs. A click selects the
/// nearest sample's result.
pub(super) fn trends(ui: &mut egui::Ui, batch: &t::BatchResult, selected: &mut usize) {
    let Some(current) = batch.results.get(*selected) else {
        return;
    };
    let Some(target) = batch.request.targets.iter().find(|t| t.id == current.target) else {
        return;
    };
    let name = analyte_name(batch, &target.id);
    let order_of = |sample: &str| {
        batch
            .request
            .samples
            .iter()
            .find(|s| s.id == sample)
            .map(|s| (s.injection_order as f64, s.role.clone()))
    };
    let mut clicked: Option<f64> = None;
    let mut draw = |ui: &mut egui::Ui,
                    id: &str,
                    title: &str,
                    y_label: &str,
                    points: Vec<(f64, f64, t::Role, bool)>,
                    lines: &[(f64, &str)]| {
        ui.small(title);
        let dark = ui.visuals().dark_mode;
        let response = super::plot_controls::plot(ui, ("conc_trend", id))
            .height(150.0)
            .x_axis_label("Injection order")
            .y_axis_label(y_label.to_owned())
            .show(ui, |p| {
                for role in [t::Role::Standard, t::Role::Qc, t::Role::Blank, t::Role::Unknown] {
                    let group: Vec<[f64; 2]> = points
                        .iter()
                        .filter(|p| p.2 == role && !p.3)
                        .map(|p| [p.0, p.1])
                        .collect();
                    if !group.is_empty() {
                        p.points(
                            Points::new(role_label(&role), group)
                                .color(role_colour(dark, &role))
                                .radius(4.0_f32),
                        );
                    }
                }
                let chosen: Vec<[f64; 2]> =
                    points.iter().filter(|p| p.3).map(|p| [p.0, p.1]).collect();
                if !chosen.is_empty() {
                    p.points(
                        Points::new("Selected", chosen)
                            .color(Color32::WHITE)
                            .radius(7.0_f32)
                            .filled(false),
                    );
                }
                for (y, label) in lines {
                    p.hline(HLine::new(*label, *y));
                }
            });
        if response.response.clicked() {
            if let Some(pos) = response.response.hover_pos() {
                clicked = Some(response.transform.value_from_position(pos).x);
            }
        }
    };
    ui.strong(format!("{name}: trends over the run"));
    let selected_sample = current.sample.clone();
    // Response used for calibration: peak area, or area ratio to the internal standard.
    let responses: Vec<_> = batch
        .results
        .iter()
        .filter(|r| r.target == target.id)
        .filter_map(|r| {
            let (x, role) = order_of(&r.sample)?;
            Some((x, r.response?, role, r.sample == selected_sample))
        })
        .collect();
    draw(
        ui,
        "response",
        if target.internal_standard.is_some() {
            "Response (analyte area ÷ internal-standard area)"
        } else {
            "Response (peak area)"
        },
        "Response",
        responses,
        &[],
    );
    if let Some(is) = target.internal_standard.as_ref() {
        let areas: Vec<_> = batch
            .observations
            .iter()
            .filter(|o| &o.target == is)
            .filter_map(|o| {
                let (x, role) = order_of(&o.sample)?;
                Some((x, o.quantifier.peak.as_ref()?.area, role, o.sample == selected_sample))
            })
            .collect();
        let bounds = batch
            .request
            .targets
            .iter()
            .find(|t| &t.id == is)
            .and_then(|t| t.is_area_range)
            .filter(|b| b[1] / b[0] < 1e6);
        let lines: Vec<(f64, &str)> = bounds
            .map(|b| vec![(b[0], "Lowest accepted area"), (b[1], "Highest accepted area")])
            .unwrap_or_default();
        draw(
            ui,
            "is_area",
            &format!("Internal standard area ({}) — a drift or dropout points to a bad injection or integration", analyte_name(batch, is)),
            "Area",
            areas,
            &lines,
        );
    }
    if let Some(cfg) = target.calibration.as_ref() {
        let accuracy: Vec<_> = batch
            .results
            .iter()
            .filter(|r| r.target == target.id)
            .filter_map(|r| {
                let (x, role) = order_of(&r.sample)?;
                matches!(role, t::Role::Standard | t::Role::Qc)
                    .then_some((x, r.accuracy_percent?, role, r.sample == selected_sample))
            })
            .collect();
        if !accuracy.is_empty() {
            let tol = cfg.accuracy_tolerance_percent;
            draw(
                ui,
                "accuracy",
                "Accuracy of standards and QCs (% of the known concentration)",
                "Accuracy %",
                accuracy,
                &[(100.0 - tol, "Lower limit"), (100.0, "Exact"), (100.0 + tol, "Upper limit")],
            );
        }
    }
    if let Some(order) = clicked {
        if let Some(sample) = nearest_sample(batch, order) {
            if let Some(i) = batch
                .results
                .iter()
                .position(|r| r.sample == sample.id && r.target == target.id)
            {
                *selected = i;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(state: t::State, flags: &[&str]) -> t::Concentration {
        t::Concentration {
            sample: "s".into(),
            target: "target-1".into(),
            state,
            response: Some(1.0),
            injected_concentration: Some(1.0),
            concentration: Some(1.0),
            unit: t::Unit::NgMl,
            dilution: 1.0,
            accuracy_percent: None,
            qualifier_ratios: vec![],
            flags: flags.iter().map(|f| (*f).to_owned()).collect(),
            reviewed: false,
        }
    }
    #[test]
    fn results_are_graded_by_state_and_flags() {
        assert_eq!(health(&row(t::State::Present, &[])), Health::Good);
        assert_eq!(
            health(&row(t::State::Present, &["calibration_excluded: bad point"])),
            Health::Good
        );
        assert_eq!(health(&row(t::State::Present, &["low_confidence_peak"])), Health::Check);
        assert_eq!(health(&row(t::State::BelowLoq, &[])), Health::Check);
        assert_eq!(health(&row(t::State::Missing, &[])), Health::Bad);
        assert_eq!(health(&row(t::State::AboveRange, &["x"])), Health::Bad);
        assert_eq!(cell_text(&row(t::State::Present, &[])), "1.000");
        assert_eq!(cell_text(&row(t::State::BelowDetection, &[])), "< LOD");
    }
}
