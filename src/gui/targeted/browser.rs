//! The results browser, modelled on Thermo Quan Browser: the results of the selected analyte as a
//! table, the analytes on the right, the integrated peak and the calibration curve below.
use super::*;
use crate::gui::concentration_view::{colour, health, Health};
use crate::gui::table::{self, Cell, Row};
use crate::quant::Status;
use egui::Color32;
use egui_plot::{Polygon, Text};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Filter {
    #[default]
    All,
    Standards,
    Qcs,
    Blanks,
    Unknowns,
}
impl Filter {
    const ALL: [Filter; 5] = [Filter::All, Filter::Standards, Filter::Qcs, Filter::Blanks, Filter::Unknowns];
    fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Standards => "Standards",
            Filter::Qcs => "QCs",
            Filter::Blanks => "Blanks",
            Filter::Unknowns => "Unknowns",
        }
    }
    fn keeps(self, role: &t::Role) -> bool {
        matches!(
            (self, role),
            (Filter::All, _)
                | (Filter::Standards, t::Role::Standard)
                | (Filter::Qcs, t::Role::Qc)
                | (Filter::Blanks, t::Role::Blank)
                | (Filter::Unknowns, t::Role::Unknown)
        )
    }
}
/// What the caller must do once the borrows of the state are released.
#[derive(Default)]
pub(super) struct Outcome {
    pub applied: Option<t::BatchResult>,
    /// `Some(true)` = accept, `Some(false)` = reject the selected concentration.
    pub review: Option<bool>,
}

const HEADERS: [&str; 12] = [
    "File name",
    "Sample type",
    "Sample ID",
    "Integration",
    "Area",
    "ISTD area",
    "Response",
    "Specified amount",
    "Calculated amount",
    "% Diff",
    "Confidence",
    "Flags",
];
fn number(value: Option<f64>, decimals: usize) -> Cell {
    Cell {
        text: value.map_or("NA".into(), |v| format!("{v:.decimals$}")),
        number: value,
        color: None,
    }
}
fn role_name(role: &t::Role) -> &'static str {
    match role {
        t::Role::Standard => "Standard",
        t::Role::Qc => "QC",
        t::Role::Blank => "Blank",
        t::Role::Unknown => "Unknown",
    }
}
fn integration(status: &Status) -> &'static str {
    match status {
        Status::Automatic => "Automatic",
        Status::Manual => "Manual",
        Status::Reviewed => "Reviewed",
        Status::Ambiguous => "Ambiguous",
        Status::Missing => "Missing",
        Status::Failed => "Failed",
        Status::Pending => "Pending",
        Status::Cancelled => "Cancelled",
    }
}
fn observation<'a>(batch: &'a t::BatchResult, sample: &str, target: &str) -> Option<&'a t::Observation> {
    batch.observations.iter().find(|o| o.sample == sample && o.target == target)
}
/// The table rows for one analyte: (index into `batch.results`, row).
pub(super) fn rows_for(dark: bool, batch: &t::BatchResult, target: &str, filter: Filter) -> Vec<(usize, Row)> {
    let internal = batch
        .request
        .targets
        .iter()
        .find(|t| t.id == target)
        .and_then(|t| t.internal_standard.clone());
    let mut samples: Vec<&t::Sample> = batch.request.samples.iter().collect();
    samples.sort_by_key(|s| s.injection_order);
    samples
        .into_iter()
        .filter(|s| filter.keeps(&s.role))
        .filter_map(|s| {
            let index = batch.results.iter().position(|r| r.sample == s.id && r.target == target)?;
            let r = &batch.results[index];
            let obs = observation(batch, &s.id, target);
            let area = obs.and_then(|o| o.quantifier.peak.as_ref()).map(|p| p.area);
            let istd = internal
                .as_deref()
                .and_then(|is| observation(batch, &s.id, is))
                .and_then(|o| o.quantifier.peak.as_ref())
                .map(|p| p.area);
            let specified = matches!(s.role, t::Role::Standard | t::Role::Qc)
                .then(|| s.nominal.get(target).copied())
                .flatten();
            let confidence = obs
                .and_then(|o| o.quantifier.quality.as_ref())
                .map_or("–", |q| q.tier.label());
            let tint = colour(dark, health(r));
            let mut calculated = number(r.concentration.or(r.injected_concentration), 3);
            calculated.color = Some(tint);
            Some((
                index,
                Row {
                    key: format!("{}/{}", s.id, target),
                    cells: vec![
                        Cell::text(crate::sample_sheet::stem(&s.source)).colored(tint),
                        Cell::text(role_name(&s.role)),
                        Cell::text(&s.id),
                        Cell::text(obs.map_or("–", |o| integration(&o.quantifier.status))),
                        number(area, 0),
                        number(istd, 0),
                        number(r.response, if internal.is_some() { 4 } else { 0 }),
                        number(specified, 3),
                        calculated,
                        number(r.accuracy_percent.map(|a| a - 100.0), 2),
                        Cell::text(confidence),
                        Cell::text(if r.state == t::State::Present {
                            r.flags.join("; ")
                        } else {
                            format!("{:?}; {}", r.state, r.flags.join("; "))
                        })
                        .colored(tint),
                    ],
                },
            ))
        })
        .collect()
}

fn peak_pane(ui: &mut egui::Ui, batch: &t::BatchResult, selected: usize, height: f32, revisit: &mut Option<(String, String)>) {
    let Some(row) = batch.results.get(selected) else {
        ui.label("Select a result in the table.");
        return;
    };
    let Some(obs) = observation(batch, &row.sample, &row.target) else {
        ui.label("No chromatogram retained for this result.");
        return;
    };
    let m = &obs.quantifier;
    ui.label(egui::RichText::new(format!(
        "{} ({})  ·  {}",
        crate::sample_sheet::stem(&m.source),
        integration(&m.status),
        m.analyte
    ))
    .strong());
    let tk = crate::gui::theme::Tokens::of(ui);
    super::super::plot_controls::plot_x_only(ui, "browser_peak")
        .height(height)
        .x_axis_label("Time (min)")
        .y_axis_label("Intensity")
        .show(ui, |p| {
            p.line(Line::new("Trace", m.trace.clone()));
            if let Some(peak) = &m.peak {
                let y = |x| crate::processing::interpolate_at(&m.trace, x);
                if let Some(target) = batch.request.targets.iter().find(|t| t.id == row.target) {
                    let method = crate::quant::Method {
                        detection_smoothing: batch.request.detection_smoothing,
                        minimum_height: batch.request.minimum_height,
                        boundary_fraction: batch.request.boundary_fraction,
                        ..Default::default()
                    };
                    let cfg = target.quantifier.extraction.integration_for(&method);
                    if let Ok(profile) = crate::integration::profile(&m.trace, peak.start, peak.end, &cfg) {
                        p.line(Line::new("Baseline", profile.baseline.clone()).color(tk.text_dim));
                        let mut outline = crate::processing::decimate_for_display(&profile.signal);
                        outline.extend(profile.baseline.iter().rev().copied());
                        p.polygon(
                            Polygon::new("Integrated area", outline)
                                .fill_color(tk.info.gamma_multiply(0.35))
                                .stroke(egui::Stroke::new(1.0_f32, tk.info.gamma_multiply(0.63))),
                        );
                    }
                }
                p.text(Text::new(
                    "Apex",
                    egui_plot::PlotPoint::new(peak.apex_rt, y(peak.apex_rt)),
                    format!("RT: {:.2}", peak.apex_rt),
                ));
            }
        });
    if ui
        .button("Revisit this peak in tab 1 ›")
        .on_hover_text("Open the peak in Areas and peak review to check or re-integrate it, then recalculate here")
        .clicked()
    {
        let source = batch.request.samples.iter().find(|s| s.id == row.sample).map(|s| s.source.clone());
        let analyte = batch.request.targets.iter().find(|t| t.id == row.target).map(|t| t.quantifier.extraction.extraction.name.clone());
        *revisit = source.zip(analyte);
    }
}
fn trends_pane(ui: &mut egui::Ui, batch: &t::BatchResult, target: &str, selected: &mut usize) {
    crate::gui::concentration_view::trends(ui, batch, selected);
    let precision: Vec<_> = batch.precision.iter().filter(|p| p.target == target).collect();
    if !precision.is_empty() {
        ui.collapsing("Replicate precision per level", |ui| {
            for p in precision {
                ui.label(format!(
                    "{} level {}: n {}/{}, mean {}, CV {}, {}",
                    role_name(&p.role),
                    p.nominal,
                    p.n,
                    p.sample_count,
                    p.mean.map_or("missing".into(), |v| format!("{v:.3}")),
                    p.cv_percent.map_or("–".into(), |v| format!("{v:.1} %")),
                    p.flags.join("; ")
                ));
            }
        });
    }
}
fn analyte_list(ui: &mut egui::Ui, batch: &t::BatchResult, target: &mut String, selected: &mut usize) {
    crate::gui::widgets::strong(ui, "Analytes");
    for t in &batch.request.targets {
        let rows: Vec<&t::Concentration> = batch.results.iter().filter(|r| r.target == t.id).collect();
        let (dot, tip) = if t.calibration.is_none() {
            (ui.visuals().weak_text_color(), "Internal standard".to_owned())
        } else {
            let worst = rows.iter().map(|r| health(r)).fold(Health::Good, |a, b| match (a, b) {
                (Health::Bad, _) | (_, Health::Bad) => Health::Bad,
                (Health::Check, _) | (_, Health::Check) => Health::Check,
                _ => Health::Good,
            });
            let n = rows.iter().filter(|r| health(r) != Health::Good).count();
            (colour(ui.visuals().dark_mode, worst), format!("{n} of {} results need a look", rows.len()))
        };
        let name = t.quantifier.extraction.extraction.name.clone();
        let label = egui::RichText::new(format!(
            "● {name}{}",
            if t.calibration.is_none() { " (ISTD)" } else { "" }
        ));
        let chosen = *target == t.id;
        let response = ui.selectable_label(chosen, label.color(dot)).on_hover_text(tip);
        if response.clicked() && !chosen {
            // Keep looking at the same sample when switching analyte.
            let sample = batch.results.get(*selected).map(|r| r.sample.clone());
            *target = t.id.clone();
            if let Some(i) = sample.and_then(|s| batch.results.iter().position(|r| r.sample == s && r.target == t.id)) {
                *selected = i;
            } else if let Some(i) = batch.results.iter().position(|r| r.target == t.id) {
                *selected = i;
            }
        }
    }
}

fn table_pane(
    ui: &mut egui::Ui,
    rows: &[Row],
    indices: &[usize],
    selected_key: Option<&str>,
    selected: &mut usize,
    filter: &mut Filter,
    height: f32,
) {
    let weights = [120., 80., 90., 90., 90., 90., 80., 100., 100., 70., 80., 220.];
    if let Some(i) = table::show_with(ui, "targeted_browser_rows", &HEADERS, rows, selected_key, height, &weights) {
        *selected = indices[i];
    }
    ui.horizontal(|ui| {
        for f in Filter::ALL {
            if ui.selectable_label(*filter == f, f.label()).clicked() {
                *filter = f;
            }
        }
    });
}
fn peak_and_trends(
    ui: &mut egui::Ui,
    batch: &t::BatchResult,
    target: &str,
    selected: &mut usize,
    tab: &mut usize,
    revisit: &mut Option<(String, String)>,
    height: f32,
) {
    ui.horizontal(|ui| {
        crate::gui::widgets::nav_value(ui, tab, 0, "Peak");
        crate::gui::widgets::nav_value(ui, tab, 1, "Trends");
    });
    if *tab == 0 {
        peak_pane(ui, batch, *selected, (height - 120.0).max(120.0), revisit);
    } else {
        trends_pane(ui, batch, target, selected);
    }
}
fn curve(
    ui: &mut egui::Ui,
    cal: &mut crate::gui::calibration::State,
    batch: &t::BatchResult,
    target: &str,
    sample: &str,
    outcome: &mut Outcome,
    height: f32,
) {
    if let Some(next) = crate::gui::calibration::curve_pane(cal, batch, target, sample, (height - 190.0).max(140.0), ui) {
        outcome.applied = Some(next);
    }
}

/// Draws the browser for the current run. Layout: results table (top left), peak and curve
/// (bottom), analyte list (right); stacked in one scroll area when the window is narrow.
pub(super) fn show(state: &mut UiState, ui: &mut egui::Ui) -> Outcome {
    let mut outcome = Outcome::default();
    let UiState {
        history,
        current_batch,
        selected,
        cal,
        filter,
        browse_target,
        peak_tab,
        reason,
        message,
        revisit,
        ..
    } = state;
    let Some(batch) = history.get(*current_batch) else {
        ui.label("Nothing calculated yet. Use Setup, then Calculate concentrations.");
        return outcome;
    };
    if *selected >= batch.results.len() {
        *selected = 0;
    }
    if !batch.request.targets.iter().any(|t| &t.id == browse_target && t.calibration.is_some()) {
        let first = batch
            .request
            .targets
            .iter()
            .find(|t| t.calibration.is_some())
            .map(|t| t.id.clone());
        *browse_target = first.unwrap_or_default();
        if let Some(i) = batch.results.iter().position(|r| &r.target == browse_target) {
            *selected = i;
        }
    }
    // The selection always belongs to the analyte being browsed.
    if batch.results.get(*selected).is_none_or(|r| &r.target != browse_target) {
        if let Some(i) = batch.results.iter().position(|r| &r.target == browse_target) {
            *selected = i;
        }
    }
    for (target, error) in &batch.calibration_errors {
        ui.colored_label(ui.visuals().error_fg_color, format!("{target}: {error}"));
    }
    // Toolbar: review the selected concentration, export, revisit.
    ui.horizontal_wrapped(|ui| {
        ui.label("Review reason");
        ui.add(egui::TextEdit::singleline(reason).desired_width(180.0));
        for (label, accepted) in [("Accept concentration", true), ("Reject concentration", false)] {
            if ui.button(label).clicked() {
                outcome.review = Some(accepted);
            }
        }
        ui.menu_button("Export ▾", |ui| {
            if ui.button("Export concentrations CSV…").clicked() {
                if let Some(path) = rfd::FileDialog::new().set_file_name("concentrations.csv").save_file() {
                    *message = match t::csv(batch).map_err(|e| e.to_string()).and_then(|csv| write_new(&path, csv.as_bytes()).map_err(|e| e.to_string())) {
                        Ok(()) => "CSV exported".into(),
                        Err(e) => e,
                    };
                }
                ui.close();
            }
            if ui.button("Export calibration CSV…").clicked() {
                if let Some(path) = rfd::FileDialog::new().set_file_name("calibration.csv").save_file() {
                    *message = match t::calibration_csv(batch).map_err(|e| e.to_string()).and_then(|csv| write_new(&path, csv.as_bytes()).map_err(|e| e.to_string())) {
                        Ok(()) => "Calibration CSV exported".into(),
                        Err(e) => e,
                    };
                }
                ui.close();
            }
        });
        if !message.is_empty() {
            ui.small(message.as_str());
        }
    });
    let size = ui.available_size();
    let selected_sample = batch.results.get(*selected).map(|r| r.sample.clone()).unwrap_or_default();
    let (indices, table_rows): (Vec<usize>, Vec<Row>) = rows_for(ui.visuals().dark_mode, batch, browse_target, *filter).into_iter().unzip();
    let selected_key = batch.results.get(*selected).map(|r| format!("{}/{}", r.sample, r.target));
    let list_width = 190.0;
    if size.x >= 900.0 {
        let left = size.x - list_width - 12.0;
        let top = (size.y * 0.42).clamp(170.0, 460.0);
        egui::ScrollArea::vertical().id_salt("browser_wide").auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(egui::vec2(left, size.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.set_width(left);
                table_pane(ui, &table_rows, &indices, selected_key.as_deref(), selected, filter, top - 40.0);
                ui.add_space(4.0);
                let bottom = (size.y - top - 12.0).max(420.0);
                let half = (left - 14.0) / 2.0;
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(half, bottom), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(half);
                        egui::ScrollArea::vertical().id_salt("browser_peak_scroll").max_height(bottom).auto_shrink([false, false]).show(ui, |ui| {
                            peak_and_trends(ui, batch, browse_target, selected, peak_tab, revisit, bottom);
                        });
                    });
                    ui.separator();
                    ui.allocate_ui_with_layout(egui::vec2(half, bottom), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(half);
                        egui::ScrollArea::vertical().id_salt("browser_curve_scroll").max_height(bottom).auto_shrink([false, false]).show(ui, |ui| {
                            curve(ui, cal, batch, browse_target, &selected_sample, &mut outcome, bottom);
                        });
                    });
                });
            });
            ui.separator();
            ui.allocate_ui_with_layout(egui::vec2(list_width, size.y), egui::Layout::top_down(egui::Align::Min), |ui| {
                egui::ScrollArea::vertical().id_salt("browser_list_scroll").auto_shrink([false, false]).show(ui, |ui| {
                    analyte_list(ui, batch, browse_target, selected);
                });
            });
        });
        });
    } else {
        egui::ScrollArea::vertical().id_salt("browser_narrow").auto_shrink([false, false]).show(ui, |ui| {
            analyte_list(ui, batch, browse_target, selected);
            ui.separator();
            table_pane(ui, &table_rows, &indices, selected_key.as_deref(), selected, filter, 240.0);
            ui.separator();
            peak_and_trends(ui, batch, browse_target, selected, peak_tab, revisit, 320.0);
            ui.separator();
            curve(ui, cal, batch, browse_target, &selected_sample, &mut outcome, 420.0);
        });
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as chromascope;
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/targeted_support.rs"));

    #[test]
    fn rows_follow_the_selected_analyte_and_the_role_tab() {
        let temp = tempfile::tempdir().unwrap();
        let request = raw_request(temp.path());
        let batch = t::run(request, &crate::jobs::JobControl::default()).unwrap();
        let target = batch
            .request
            .targets
            .iter()
            .find(|t| t.calibration.is_some())
            .unwrap()
            .id
            .clone();
        let all = rows_for(false, &batch, &target, Filter::All);
        assert_eq!(all.len(), batch.request.samples.len());
        let standards = rows_for(false, &batch, &target, Filter::Standards);
        assert_eq!(
            standards.len(),
            batch.request.samples.iter().filter(|s| s.role == t::Role::Standard).count()
        );
        assert!(standards.iter().all(|(_, r)| r.cells[1].text == "Standard"));
        // Index points back at the matching result.
        let (index, row) = &all[0];
        assert_eq!(row.key, format!("{}/{}", batch.results[*index].sample, target));
        assert_eq!(row.cells.len(), HEADERS.len());
        // A standard shows its specified amount, an unknown shows NA.
        let specified = |f: Filter| rows_for(false, &batch, &target, f).first().map(|r| r.1.cells[7].text.clone());
        assert_ne!(specified(Filter::Standards).unwrap(), "NA");
        if let Some(unknown) = specified(Filter::Unknowns) {
            assert_eq!(unknown, "NA");
        }
    }
}
