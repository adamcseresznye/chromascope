//! Retained traces, reproducible measurements and portable workbench sessions.
use super::state::{FileId, MzViewerApp, StateChange, UserInput};
use crate::{
    parser::ChromatogramData, plotting_parameters::LineColor, processing::ProcessingParams,
};
use egui::{Context, Key, KeyboardShortcut, Modifiers};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Trace {
    pub name: String,
    pub points: Vec<[f64; 2]>,
    #[serde(skip)]
    pub display_points: Vec<[f64; 2]>,
    pub chromatogram: ChromatogramData,
    pub params: ProcessingParams,
    pub color: LineColor,
    pub visible: bool,
    #[serde(default)]
    pub order: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Measurement {
    pub file: String,
    pub source: String,
    pub trace: String,
    pub params: ProcessingParams,
    pub start: f64,
    pub end: f64,
    pub area: f64,
}
#[derive(Serialize, Deserialize)]
struct SavedFile {
    source: String,
    run: String,
    current: Option<Trace>,
    traces: Vec<Trace>,
    visible: bool,
    scan: Option<usize>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct ViewSettings {
    pub focus_restore: Option<[bool; 3]>,
    pub intensity_scale: IntensityScale,
    pub intensity_maximum: f64,
    pub horizontal: bool,
    pub rows: usize,
    pub columns: usize,
    pub compare_samples: bool,
    pub files: bool,
    pub inspector: bool,
    pub spectrum: bool,
}
impl Default for ViewSettings {
    fn default() -> Self {
        Self {
            focus_restore: None,
            intensity_scale: IntensityScale::Individual,
            intensity_maximum: 1_000_000.0,
            horizontal: false,
            rows: 3,
            columns: 2,
            compare_samples: false,
            files: true,
            inspector: true,
            spectrum: true,
        }
    }
}
impl ViewSettings {
    pub fn toggle_focus(&mut self) {
        if let Some([files, inspector, spectrum]) = self.focus_restore.take() {
            self.files = files;
            self.inspector = inspector;
            self.spectrum = spectrum;
        } else {
            self.focus_restore = Some([self.files, self.inspector, self.spectrum]);
            self.files = false;
            self.inspector = false;
            self.spectrum = false;
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum IntensityScale {
    #[default]
    Individual,
    SharedHighest,
    SharedCustom,
}

pub(super) fn shared_intensity_maximum(app: &MzViewerApp) -> Option<f64> {
    match app.workspace.view.intensity_scale {
        IntensityScale::Individual => None,
        IntensityScale::SharedCustom => {
            let value = app.workspace.view.intensity_maximum;
            (value.is_finite() && value > 0.0).then_some(value)
        }
        IntensityScale::SharedHighest => {
            let mut maximum: f64 = 0.0;
            for (id, params) in trace_keys(app, true) {
                let file = &app.files[&id];
                let points = if file.cache.last_processing_params.as_ref() == Some(&params) {
                    file.cache.plot_data.as_deref()
                } else {
                    app.workspace
                        .traces
                        .get(&id)
                        .and_then(|traces| traces.iter().find(|t| t.params == params))
                        .map(|t| t.points.as_slice())
                };
                if let Some(points) = points {
                    maximum = points
                        .iter()
                        .filter(|p| p[1].is_finite())
                        .map(|p| p[1])
                        .fold(maximum, f64::max);
                }
            }
            Some(if maximum > 0.0 { maximum * 1.05 } else { 1.0 })
        }
    }
}
#[derive(Serialize, Deserialize)]
struct Session {
    version: u32,
    files: Vec<SavedFile>,
    active: Option<usize>,
    input: UserInput,
    measurements: Vec<Measurement>,
    dark: bool,
    bounds: Option<[[f64; 2]; 2]>,
    #[serde(default)]
    spectrum_bounds: Option<[[f64; 2]; 2]>,
    split: Option<f32>,
    #[serde(default)]
    overlay: bool,
    #[serde(default)]
    view: ViewSettings,
    #[serde(default)]
    batch_preset: Option<Vec<(String, ProcessingParams)>>,
}
impl Session {
    fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported session version.".into());
        }
        if self.active.is_some_and(|i| i >= self.files.len()) {
            return Err("Invalid active file in session.".into());
        }
        if self
            .bounds
            .into_iter()
            .chain(self.spectrum_bounds)
            .any(|[min, max]| {
                min.iter().chain(max.iter()).any(|x| !x.is_finite())
                    || min[0] >= max[0]
                    || min[1] >= max[1]
            })
        {
            return Err("Invalid plot bounds in session.".into());
        }
        for file in &self.files {
            for trace in file.current.iter().chain(file.traces.iter()) {
                let c = &trace.chromatogram;
                if c.retention_time.len() != c.index.len()
                    || c.intensity.len() != c.index.len()
                    || trace
                        .points
                        .iter()
                        .any(|p| p.iter().any(|x| !x.is_finite()))
                    || trace.points.windows(2).any(|w| w[0][0] > w[1][0])
                {
                    return Err("Invalid trace data in session.".into());
                }
                if trace.params.smoothing > 10 {
                    return Err("Invalid smoothing in session.".into());
                }
                if let Some(x) = trace.params.xic_params {
                    crate::validation::XicParams::new(
                        x.mass(),
                        x.polarity(),
                        x.mass_tolerance(),
                        &crate::validation::DataBounds::unrestricted(),
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }
}
pub(super) struct Workspace {
    pub traces: HashMap<FileId, Vec<Trace>>,
    pub measurements: Vec<Measurement>,
    pub recent: Vec<PathBuf>,
    pending: Vec<(String, SavedFile)>,
    restore_active: Option<(String, String)>,
    pub restore_input: Option<UserInput>,
    pub bounds: Option<[[f64; 2]; 2]>,
    pub apply_bounds: bool,
    pub spectrum_bounds: Option<[[f64; 2]; 2]>,
    pub apply_spectrum_bounds: bool,
    pub names: HashMap<(FileId, String), String>,
    pub order: HashMap<(FileId, String), usize>,
    next_order: usize,
    pub view: ViewSettings,
    pub overlay: bool,
    pub page: usize,
    pub hidden_current: std::collections::HashSet<FileId>,
    pub reset_plots: bool,
    metadata: Option<(FileId, usize, String)>,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            traces: HashMap::new(),
            measurements: Vec::new(),
            recent: recent_path()
                .and_then(|p| std::fs::read(p).ok())
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default(),
            pending: Vec::new(),
            restore_active: None,
            restore_input: None,
            bounds: None,
            apply_bounds: false,
            spectrum_bounds: None,
            apply_spectrum_bounds: false,
            names: Default::default(),
            order: Default::default(),
            next_order: 0,
            view: ViewSettings::default(),
            overlay: false,
            page: 0,
            hidden_current: Default::default(),
            reset_plots: false,
            metadata: None,
        }
    }
}
fn recent_path() -> Option<PathBuf> {
    crate::import::settings_path().map(|p| p.with_file_name("recent-files.json"))
}
pub(super) fn remember(app: &mut MzViewerApp, path: PathBuf) {
    app.workspace.recent.retain(|p| p != &path);
    app.workspace.recent.insert(0, path);
    app.workspace.recent.truncate(12);
    if let Some(p) = recent_path() {
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            std::fs::create_dir_all(p.parent().unwrap())?;
            std::fs::write(p, serde_json::to_vec(&app.workspace.recent)?)?;
            Ok(())
        })();
        if let Err(e) = result {
            log::warn!("Cannot save recent files: {e}");
        }
    }
}
pub(super) fn trace_name(params: &ProcessingParams) -> String {
    let target = params
        .xic_params
        .map(|x| format!(" {:.4} ± {} ppm", x.mass(), x.mass_tolerance()))
        .unwrap_or_default();
    let mode = params
        .acquisition
        .map(|m| format!(" / {m:?}"))
        .unwrap_or_default();
    format!(
        "{:?}{mode}{target} · MS{} · {:?} · smooth {}{}{}",
        params.plot_type,
        params.ms_level,
        params.polarity,
        params.smoothing,
        params
            .precursor_mz
            .map(|m| format!(" · precursor {m:.4}"))
            .unwrap_or_default(),
        params
            .mz_range
            .map(|(a, b)| format!(" · m/z {a:.2}–{b:.2}"))
            .unwrap_or_default()
    )
}
pub(super) fn display_name(app: &MzViewerApp, id: FileId, params: &ProcessingParams) -> String {
    app.workspace
        .names
        .get(&(id, format!("{params:?}")))
        .cloned()
        .unwrap_or_else(|| trace_name(params))
}
fn current(app: &MzViewerApp, id: FileId) -> Option<Trace> {
    let file = app.files.get(&id)?;
    let params = file.cache.last_processing_params.clone()?;
    Some(Trace {
        name: display_name(app, id, &params),
        order: *app
            .workspace
            .order
            .get(&(id, format!("{params:?}")))
            .unwrap_or(&0),
        points: file.cache.plot_data.clone()?,
        display_points: file.cache.display_data.clone().unwrap_or_default(),
        chromatogram: file.cache.chromatogram.clone()?,
        params,
        color: file.display.color,
        visible: !app.workspace.hidden_current.contains(&id),
    })
}
pub(super) fn register_order(app: &mut MzViewerApp, id: FileId, params: &ProcessingParams) {
    let key = (id, format!("{params:?}"));
    if !app.workspace.order.contains_key(&key) {
        let next = app.workspace.next_order;
        app.workspace.next_order += 1;
        app.workspace.order.insert(key, next);
    }
}
pub(super) fn retain_current(app: &mut MzViewerApp, id: FileId) {
    if let Some(p) = app
        .files
        .get(&id)
        .and_then(|f| f.cache.last_processing_params.clone())
    {
        register_order(app, id, &p);
    }
    if let Some(trace) = current(app, id) {
        let traces = app.workspace.traces.entry(id).or_default();
        if !traces.iter().any(|t| t.params == trace.params) {
            traces.push(trace);
        }
    }
}
pub(super) fn select_trace(app: &mut MzViewerApp, id: FileId, index: usize) {
    let selected = app
        .workspace
        .traces
        .get_mut(&id)
        .and_then(|t| (index < t.len()).then(|| t.remove(index)));
    if let Some(t) = selected {
        retain_current(app, id);
        apply_trace(app, id, t);
        app.integration = Default::default();
        app.state_changed = StateChange::Unchanged;
    }
}
pub(super) fn activate_file(app: &mut MzViewerApp, id: FileId) {
    let cached = current(app, id);
    let ready = cached.is_some();
    if let Some(t) = cached {
        apply_trace(app, id, t);
    }
    app.user_input.retention_time_ms_spectrum = app
        .files
        .get(&id)
        .and_then(|f| f.cache.mass_spectrum.as_ref())
        .map(|s| s.retention_time);
    app.state_changed = if ready {
        StateChange::Unchanged
    } else {
        StateChange::Changed
    };
}
fn apply_trace(app: &mut MzViewerApp, id: FileId, t: Trace) {
    app.workspace
        .order
        .insert((id, format!("{:?}", t.params)), t.order);
    app.workspace.next_order = app.workspace.next_order.max(t.order + 1);
    app.workspace
        .names
        .insert((id, format!("{:?}", t.params)), t.name.clone());
    if t.visible {
        app.workspace.hidden_current.remove(&id);
    } else {
        app.workspace.hidden_current.insert(id);
    }
    let p = &t.params;
    app.user_input.acquisition = p.acquisition;
    app.user_input.plot_type = p.plot_type;
    app.user_input.ms_level = p.ms_level;
    app.user_input.polarity = p.polarity;
    app.user_input.smoothing = p.smoothing;
    app.user_input.precursor_mz = p.precursor_mz;
    app.user_input.range_enabled = p.mz_range.is_some();
    if let Some((a, b)) = p.mz_range {
        app.user_input.range_min = super::state::ValidatedInput::new(a);
        app.user_input.range_max = super::state::ValidatedInput::new(b);
    }
    if let Some(x) = p.xic_params {
        app.user_input.mass = super::state::ValidatedInput::new(x.mass());
        app.user_input.mass_tolerance = super::state::ValidatedInput::new(x.mass_tolerance());
    }
    app.user_input.line_color = t.color;
    if let Some(f) = app.files.get_mut(&id) {
        f.display.color = t.color;
        f.cache.display_data = Some(crate::processing::decimate_for_display(&t.points));
        f.cache.plot_data = Some(t.points);
        f.cache.chromatogram = Some(t.chromatogram);
        f.cache.last_processing_params = Some(t.params);
    }
}
pub(super) fn trace_keys(app: &MzViewerApp, visible_only: bool) -> Vec<(FileId, ProcessingParams)> {
    let mut keys = Vec::new();
    let mut ids: Vec<_> = app.files.keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let f = &app.files[&id];
        if !app.workspace.view.compare_samples && app.active_file_id != Some(id) {
            continue;
        }
        if app.presets.specs.is_some() && !app.presets.applied.contains(&id) {
            continue;
        }
        if visible_only && !f.display.visible {
            continue;
        }
        if let Some(p) = &f.cache.last_processing_params {
            if !visible_only || !app.workspace.hidden_current.contains(&id) {
                keys.push((id, p.clone()));
            }
        }
        if let Some(traces) = app.workspace.traces.get(&id) {
            for t in traces {
                if (!visible_only || t.visible)
                    && Some(&t.params) != f.cache.last_processing_params.as_ref()
                {
                    keys.push((id, t.params.clone()));
                }
            }
        }
    }
    if let Some(specs) = &app.presets.specs {
        keys.retain(|(_, p)| specs.iter().any(|(_, s)| s == p));
    }
    keys.sort_by_key(|(id, p)| {
        (
            *app.workspace
                .order
                .get(&(*id, format!("{p:?}")))
                .unwrap_or(&usize::MAX),
            *id,
            format!("{p:?}"),
        )
    });
    if let Some(specs) = &app.presets.specs {
        keys.sort_by_key(|(id, p)| {
            (
                *id,
                specs.iter().position(|(_, s)| s == p).unwrap_or(usize::MAX),
            )
        });
    }
    keys
}
pub(super) fn select_key(app: &mut MzViewerApp, id: FileId, params: &ProcessingParams) {
    if app.async_state.is_processing {
        return;
    }
    app.active_file_id = Some(id);
    if app
        .files
        .get(&id)
        .and_then(|f| f.cache.last_processing_params.as_ref())
        == Some(params)
    {
        activate_file(app, id);
    } else if let Some(i) = app
        .workspace
        .traces
        .get(&id)
        .and_then(|ts| ts.iter().position(|t| &t.params == params))
    {
        select_trace(app, id, i);
    }
}
pub(super) struct ComparisonLayout {
    pub samples: Vec<FileId>,
    pub analytes: Vec<(String, ProcessingParams)>,
    pub visible: Vec<(FileId, ProcessingParams)>,
}
pub(super) fn comparison_layout(app: &MzViewerApp) -> ComparisonLayout {
    let mut samples: Vec<_> = app
        .files
        .iter()
        .filter(|(_, f)| f.display.visible && !f.is_loading)
        .map(|(id, _)| *id)
        .collect();
    samples.sort_unstable();
    let visible = trace_keys(app, true);
    let mut analytes = app.presets.specs.clone().unwrap_or_default();
    if analytes.is_empty() {
        for (id, params) in &visible {
            if !analytes.iter().any(|(_, p)| p == params) {
                analytes.push((display_name(app, *id, params), params.clone()));
            }
        }
    }
    ComparisonLayout {
        samples,
        analytes,
        visible,
    }
}
pub(super) fn delete_key(app: &mut MzViewerApp, id: FileId, params: &ProcessingParams) {
    if app.async_state.is_processing {
        return;
    }
    if app
        .files
        .get(&id)
        .and_then(|f| f.cache.last_processing_params.as_ref())
        == Some(params)
    {
        if let Some(f) = app.files.get_mut(&id) {
            f.cache.plot_data = None;
            f.cache.display_data = None;
            f.cache.chromatogram = None;
            f.cache.last_processing_params = None;
        }
        app.workspace.hidden_current.remove(&id);
        if let Some(t) = app.workspace.traces.get_mut(&id).and_then(|t| t.pop()) {
            let input = app.user_input.clone();
            apply_trace(app, id, t);
            if app.active_file_id != Some(id) {
                app.user_input = input;
            }
        }
        app.state_changed = StateChange::Unchanged;
        app.integration = Default::default();
    } else if let Some(ts) = app.workspace.traces.get_mut(&id) {
        ts.retain(|t| &t.params != params);
    }
}
/// Fixed controls first; only the name consumes remaining width. Full label is available on hover.
pub(super) fn trace_row(
    app: &mut MzViewerApp,
    ui: &mut egui::Ui,
    id: FileId,
    params: &ProcessingParams,
    include_file: bool,
) -> (egui::Rect, egui::Rect) {
    let is_current = app
        .files
        .get(&id)
        .and_then(|f| f.cache.last_processing_params.as_ref())
        == Some(params);
    let retained = app
        .workspace
        .traces
        .get(&id)
        .and_then(|ts| ts.iter().find(|t| &t.params == params));
    let color = if is_current {
        app.files[&id].display.color
    } else {
        retained.map(|t| t.color).unwrap_or_default()
    };
    let mut visible = if is_current {
        !app.workspace.hidden_current.contains(&id)
    } else {
        retained.is_some_and(|t| t.visible)
    };
    let name = retained
        .filter(|_| !is_current)
        .map(|t| t.name.clone())
        .unwrap_or_else(|| display_name(app, id, params));
    let name = if include_file {
        format!("{} · {name}", app.files[&id].name)
    } else {
        name
    };
    let active = is_current && app.active_file_id == Some(id);
    let mut controls = (egui::Rect::NOTHING, egui::Rect::NOTHING);
    let mut delete = false;
    let mut select = false;
    ui.push_id((id, format!("{params:?}")), |ui| {
        ui.horizontal(|ui| {
            let visibility = ui
                .checkbox(&mut visible, "")
                .on_hover_text("Show / hide trace");
            controls.0 = visibility.rect;
            if visibility.changed() {
                if is_current {
                    if visible {
                        app.workspace.hidden_current.remove(&id);
                    } else {
                        app.workspace.hidden_current.insert(id);
                        app.integration = Default::default();
                    }
                } else if let Some(t) = app
                    .workspace
                    .traces
                    .get_mut(&id)
                    .and_then(|ts| ts.iter_mut().find(|t| &t.params == params))
                {
                    t.visible = visible;
                }
            }
            let deletion = ui
                .add_enabled(
                    !app.async_state.is_processing,
                    egui::Button::new("×").min_size(egui::vec2(24.0, 24.0)),
                )
                .on_hover_text("Delete trace from workspace and figures");
            controls.1 = deletion.rect;
            delete = deletion.clicked();
            ui.colored_label(color.to_egui(), "●")
                .on_hover_text("Retained trace color in figures");
            let response = ui
                .add_sized(
                    [ui.available_width().max(1.0), 24.0],
                    egui::Button::selectable(active, name.clone()).truncate(),
                )
                .on_hover_text(format!("{name}\n{}", trace_name(params)));
            select = response.clicked();
        });
    });
    if delete {
        delete_key(app, id, params);
    } else if select {
        select_key(app, id, params);
    }
    controls
}
pub(super) fn inspector(app: &mut MzViewerApp, ui: &mut egui::Ui) {
    ui.separator();
    ui.heading("Traces");
    ui.small("Use the controls before each name to show/hide or delete a trace. Click its name to select it.");
    if let Some(id) = app.active_file_id {
        let rows = trace_keys(app, false)
            .into_iter()
            .filter(|(file, _)| *file == id)
            .collect::<Vec<_>>();
        for (file, params) in rows {
            trace_row(app, ui, file, &params, false);
        }
    }
    ui.separator();
    ui.heading(format!(
        "Measurements ({})",
        app.workspace.measurements.len()
    ));
    if ui.button("Results table…").clicked() {
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new("results_open"), true));
    }
}
pub(super) fn scan_navigation(app: &mut MzViewerApp, ui: &mut egui::Ui) {
    let Some(id) = app.active_file_id else { return };
    let Some(file) = app.files.get(&id) else {
        return;
    };
    let count = file.data.bounds.scan_count;
    let scan = file.cache.mass_spectrum.as_ref().map(|s| s.index);
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(scan.is_some_and(|s| s > 0), egui::Button::new("Previous"))
            .clicked()
        {
            navigate(app, -1);
        }
        if ui
            .add_enabled(
                scan.is_some_and(|s| s + 1 < count),
                egui::Button::new("Next"),
            )
            .clicked()
        {
            navigate(app, 1);
        }
        if count > 0 {
            let edit_id = egui::Id::new(("scan_jump", id));
            let mut number = ui
                .ctx()
                .data_mut(|d| *d.get_temp_mut_or(edit_id, scan.unwrap_or(0) + 1));
            ui.add(
                egui::DragValue::new(&mut number)
                    .range(1..=count)
                    .prefix("Scan "),
            );
            ui.ctx().data_mut(|d| d.insert_temp(edit_id, number));
            if ui.button("Go").clicked() {
                load_scan(app, id, number.clamp(1, count) - 1);
            }
        }
        if let Some(file) = app.files.get_mut(&id) {
            if let Some(s) = &file.cache.mass_spectrum {
                ui.weak(format!("{} / {}", s.index + 1, count));
                let index = s.index;
                if app
                    .workspace
                    .metadata
                    .as_ref()
                    .map(|(file, scan, _)| (*file, *scan))
                    != Some((id, index))
                {
                    app.workspace.metadata = file
                        .data
                        .scan_metadata(index)
                        .ok()
                        .map(|text| (id, index, text));
                }
                if let Some((_, _, text)) = &app.workspace.metadata {
                    ui.menu_button("Acquisition details…", |ui| {
                        ui.set_max_width(420.0);
                        egui::ScrollArea::vertical()
                            .max_height(300.0)
                            .show(ui, |ui| {
                                ui.label(text);
                            });
                    });
                }
            }
        }
    });
}
pub(super) fn navigate(app: &mut MzViewerApp, delta: isize) {
    let Some(id) = app.active_file_id else { return };
    let Some(f) = app.files.get(&id) else { return };
    let index = f.cache.mass_spectrum.as_ref().map(|s| s.index).unwrap_or(0);
    let Some(next) = index.checked_add_signed(delta) else {
        return;
    };
    if next < f.data.bounds.scan_count {
        load_scan(app, id, next);
    }
}
pub(super) fn load_scan(app: &mut MzViewerApp, id: FileId, index: usize) {
    if let Some(f) = app.files.get_mut(&id) {
        match f.data.get_mass_spectrum_by_index(index) {
            Ok(s) => {
                app.user_input.retention_time_ms_spectrum = Some(s.retention_time);
                f.cache.mass_spectrum = Some(s);
            }
            Err(e) => app.show_error_dialog(e.to_string()),
        }
    }
}
pub(super) fn record(app: &mut MzViewerApp) {
    let Some(id) = app.active_file_id else { return };
    let Some(f) = app.files.get(&id) else { return };
    if let (Some(start), Some(end), Some(area), Some(params)) = (
        app.integration.start_rt,
        app.integration.end_rt,
        app.integration.result,
        f.cache.last_processing_params.clone(),
    ) {
        app.workspace.measurements.push(Measurement {
            file: f.name.clone(),
            source: f.cache.source_path.clone().unwrap_or(f.path.clone()),
            trace: display_name(app, id, &params),
            params,
            start,
            end,
            area,
        });
    }
}
fn csv_field(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
fn measurements_csv(rows: &[Measurement]) -> String {
    let mut csv =
        "file,source,trace,start_rt_min,end_rt_min,area_au_min,baseline,parameters_json\n"
            .to_owned();
    for r in rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{},endpoint chord,{}\n",
            csv_field(&r.file),
            csv_field(&r.source),
            csv_field(&r.trace),
            r.start,
            r.end,
            r.area,
            csv_field(&serde_json::to_string(&r.params).unwrap())
        ));
    }
    csv
}
pub(super) fn results(app: &mut MzViewerApp, ctx: &Context) {
    let mut open = ctx
        .data(|d| d.get_temp::<bool>(egui::Id::new("results_open")))
        .unwrap_or(false);
    egui::Window::new("Integration results").max_height((ctx.content_rect().height()-60.0).max(120.0)).vscroll(true).open(&mut open).default_width(800.0).show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.add_enabled(!app.workspace.measurements.is_empty(), egui::Button::new("Export CSV…")).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name("integrations.csv").save_file() {
                    if let Err(e) = std::fs::write(p,measurements_csv(&app.workspace.measurements)) { app.show_error_dialog(e.to_string()); }
                }
            }
        });
        use super::table::{Cell,Row};
        let rows:Vec<_> = app.workspace.measurements.iter().enumerate().map(|(index,r)| Row { key:format!("{}:{}:{}:{index}",r.source,r.start,r.end), cells:vec![Cell::text(&r.file),Cell::text(&r.trace),Cell::number(Some(r.start)),Cell::number(Some(r.end)),Cell::number(Some(r.area))] }).collect();
        let selection_id = egui::Id::new("integration_result_selection");
        let mut selected = ctx.data(|d|d.get_temp::<String>(selection_id));
        if let Some(index) = super::table::show(ui,"integration-results", &["File","Trace / parameters","Start (min)","End (min)","Area (a.u.·min)"],&rows, selected.as_deref()) {
            selected = Some(rows[index].key.clone());
        }
        if let Some(index) = rows.iter().position(|row| Some(&row.key) == selected.as_ref()) {
            ui.collapsing("Selected integration evidence",|ui| { ui.label(serde_json::to_string_pretty(&app.workspace.measurements[index]).unwrap()); });
            if ui.button("Remove selected measurement").clicked() { app.workspace.measurements.remove(index); selected=None; }
        }
        ctx.data_mut(|d| {if let Some(selected)=selected {d.insert_temp(selection_id, selected);} else {d.remove::<String>(selection_id);}});
        ui.small("Full-resolution processed trace; endpoint-chord baseline. Parameters are stored with each measurement.");
    });
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("results_open"), open));
}
pub(super) fn menu(app: &mut MzViewerApp, ui: &mut egui::Ui) {
    ui.menu_button("Viewer presets", |ui| {
        if app.presets.specs.is_some() && ui.button("Clear shared batch preset").clicked() {
            if !app.async_state.is_processing {
                app.presets.specs = None;
                app.presets.applied.clear();
                app.presets.failed.clear();
                app.state_changed = StateChange::Unchanged;
            }
            ui.close();
        }
        if ui.button("Preset editor…").clicked() {
            app.presets.editor.open = true;
            ui.close();
        }
        if ui.button("Open preset…").clicked() {
            super::presets::load(app);
            ui.close();
        }
    });
    ui.separator();
    if ui
        .button("Save viewer session…")
        .on_hover_text("Save the viewer workspace (Ctrl+S / Cmd+S)")
        .clicked()
    {
        save_session(app, ui.ctx());
        ui.close();
    }
    if ui
        .button("Open viewer session…")
        .on_hover_text("Restore a viewer workspace (Ctrl+Shift+O / Cmd+Shift+O)")
        .clicked()
    {
        open_session(app, ui.ctx());
        ui.close();
    }
    if ui.button("Integration results…").clicked() {
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::new("results_open"), true));
        ui.close();
    }
}
pub(super) fn recent_menu(app: &mut MzViewerApp, ui: &mut egui::Ui) {
    ui.menu_button("Recent files", |ui| {
        if app.workspace.recent.is_empty() {
            ui.weak("No recent files");
        }
        for p in app.workspace.recent.clone() {
            if ui
                .button(p.file_name().unwrap_or_default().to_string_lossy())
                .on_hover_text(p.display().to_string())
                .clicked()
            {
                super::panels::queue_file_imports(app, vec![p]);
                ui.close();
            }
        }
    });
}
pub(super) fn figure_menu(app: &mut MzViewerApp, ui: &mut egui::Ui) {
    ui.menu_button("Figures (SVG)", |ui| {
        if ui
            .add_enabled(
                !trace_keys(app, true).is_empty(),
                egui::Button::new("Visible chromatograms…"),
            )
            .clicked()
        {
            export_figure(app, false);
            ui.close();
        }
        let has_spectrum = app
            .active_file_id
            .and_then(|id| app.files.get(&id))
            .and_then(|f| f.cache.mass_spectrum.as_ref())
            .is_some();
        if ui
            .add_enabled(has_spectrum, egui::Button::new("Active mass spectrum…"))
            .clicked()
        {
            export_figure(app, true);
            ui.close();
        }
    });
}
pub(super) fn input(app: &mut MzViewerApp, ctx: &Context) {
    app.workspace.reset_plots = ctx
        .data_mut(|d| d.remove_temp::<bool>(egui::Id::new("reset_chromatograms")))
        .unwrap_or(false);
    let paths = ctx.input(|i| {
        i.raw
            .dropped_files
            .iter()
            .filter_map(|f| f.path.clone())
            .collect::<Vec<_>>()
    });
    if !paths.is_empty() {
        super::panels::queue_file_imports(app, paths);
    }
    if app.quant.active {
        super::quant::shortcuts(app, ctx);
        return;
    }
    if ctx.input_mut(|i| {
        i.consume_shortcut(&KeyboardShortcut::new(
            Modifiers::COMMAND | Modifiers::SHIFT,
            Key::O,
        ))
    }) {
        open_session(app, ctx);
    } else if ctx
        .input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::O)))
    {
        super::panels::handle_file_selection(app);
    }
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::S))) {
        save_session(app, ctx);
    }
    if !ctx.wants_keyboard_input() {
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowLeft)) {
            navigate(app, -1);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowRight)) {
            navigate(app, 1);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            app.integration = Default::default();
        }
    }
    results(app, ctx);
}
fn capture_session(app: &MzViewerApp, ctx: &Context) -> Session {
    let mut ids: Vec<_> = app.files.keys().copied().collect();
    ids.sort_unstable();
    let active = app
        .active_file_id
        .and_then(|id| ids.iter().position(|i| *i == id));
    let files = ids
        .into_iter()
        .map(|id| {
            let f = &app.files[&id];
            SavedFile {
                source: f.cache.source_path.clone().unwrap_or(f.path.clone()),
                run: f.name.clone(),
                current: current(app, id),
                traces: app.workspace.traces.get(&id).cloned().unwrap_or_default(),
                visible: f.display.visible,
                scan: f.cache.mass_spectrum.as_ref().map(|s| s.index),
            }
        })
        .collect();
    Session {
        version: 1,
        files,
        active,
        input: app.user_input.clone(),
        measurements: app.workspace.measurements.clone(),
        dark: ctx.style().visuals.dark_mode,
        bounds: app.workspace.bounds,
        spectrum_bounds: app.workspace.spectrum_bounds,
        overlay: app.workspace.overlay,
        view: app.workspace.view.clone(),
        batch_preset: app.presets.specs.clone(),
        split: ctx.data(|d| d.get_temp::<f32>(egui::Id::new("workbench_plot_split"))),
    }
}
pub(super) fn project_snapshot(app: &MzViewerApp, ctx: &Context) -> serde_json::Value {
    serde_json::to_value(capture_session(app, ctx)).unwrap()
}
fn save_session(app: &mut MzViewerApp, ctx: &Context) {
    if app.files.values().any(|f| f.is_loading)
        || app.async_state.is_processing
        || app.state_changed == StateChange::Changed
    {
        app.show_error_dialog(
            "Wait for imports and trace updates to finish before saving a session.".into(),
        );
        return;
    }
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Chromascope session", &["chromascope"])
        .set_file_name("session.chromascope")
        .save_file()
    else {
        return;
    };
    let session = capture_session(app, ctx);
    if let Err(e) = serde_json::to_vec_pretty(&session)
        .map_err(|e| e.to_string())
        .and_then(|b| std::fs::write(path, b).map_err(|e| e.to_string()))
    {
        app.show_error_dialog(e);
    }
}
fn open_session(app: &mut MzViewerApp, ctx: &Context) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Chromascope session", &["chromascope"])
        .pick_file()
    else {
        return;
    };
    match std::fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_slice::<Session>(&b).map_err(|e| e.to_string()))
    {
        Ok(s) => {
            if let Err(e) = s.validate() {
                app.show_error_dialog(e);
                return;
            }
            restore_session(app, ctx, s);
        }
        Err(e) => app.show_error_dialog(format!("Cannot open session: {e}")),
    }
}
pub(super) fn restore_project_snapshot(
    app: &mut MzViewerApp,
    ctx: &Context,
    value: serde_json::Value,
) -> Result<(), String> {
    let session: Session = serde_json::from_value(value).map_err(|e| e.to_string())?;
    session.validate()?;
    restore_session(app, ctx, session);
    Ok(())
}
fn restore_session(app: &mut MzViewerApp, ctx: &Context, s: Session) {
    app.reset_state();
    app.workspace.traces.clear();
    app.async_state.processing_rx = None;
    app.async_state.is_processing = false;
    app.workspace.measurements = s.measurements;
    app.workspace.restore_active = s
        .active
        .and_then(|i| s.files.get(i))
        .map(|f| (f.source.clone(), f.run.clone()));
    app.workspace.restore_input = Some(s.input);
    app.workspace.pending = s.files.into_iter().map(|f| (f.source.clone(), f)).collect();
    let mut sources: Vec<_> = app
        .workspace
        .pending
        .iter()
        .map(|(p, _)| PathBuf::from(p))
        .collect();
    sources.sort();
    sources.dedup();
    super::panels::queue_file_imports(app, sources);
    app.workspace.overlay = s.overlay;
    app.workspace.view = s.view;
    app.presets.specs = s.batch_preset;
    app.presets.applied.clear();
    app.presets.failed.clear();
    app.workspace.bounds = s.bounds;
    app.workspace.spectrum_bounds = s.spectrum_bounds;
    app.workspace.apply_bounds = false;
    super::workbench::configure(ctx, s.dark);
    if let Some(split) = s.split {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("workbench_plot_split"), split));
    }
}
pub(super) fn restore_loaded(app: &mut MzViewerApp, _ctx: &Context) {
    if app.workspace.pending.is_empty() && app.workspace.restore_input.is_none() {
        return;
    }
    let mut unresolved = Vec::new();
    for (source, saved) in std::mem::take(&mut app.workspace.pending) {
        let id = app
            .files
            .iter()
            .find(|(_, f)| {
                !f.is_loading
                    && f.name == saved.run
                    && f.cache.source_path.as_deref() == Some(&source)
            })
            .map(|(id, _)| *id);
        if let Some(id) = id {
            let mut traces = saved.traces;
            for t in &mut traces {
                t.display_points = crate::processing::decimate_for_display(&t.points);
                app.workspace
                    .order
                    .insert((id, format!("{:?}", t.params)), t.order);
                app.workspace.next_order = app.workspace.next_order.max(t.order + 1);
            }
            app.workspace.traces.insert(id, traces);
            if app.presets.specs.is_some()
                && (saved.current.is_some()
                    || app.workspace.traces.get(&id).is_some_and(|t| !t.is_empty()))
            {
                app.presets.applied.insert(id);
            }
            if let Some(t) = saved.current {
                apply_trace(app, id, t);
            }
            if let Some(f) = app.files.get_mut(&id) {
                f.display.visible = saved.visible;
            }
            if let Some(scan) = saved.scan {
                load_scan(app, id, scan);
            }
        } else {
            unresolved.push((source, saved));
        }
    }
    app.workspace.pending = unresolved;
    if !app.files.values().any(|f| f.is_loading) {
        if !app.workspace.pending.is_empty() {
            app.show_error_dialog("Some session runs could not be restored. Check that their source datasets are available.".into());
            app.workspace.pending.clear();
        }
        if let Some((source, run)) = app.workspace.restore_active.take() {
            app.active_file_id = app
                .files
                .iter()
                .find(|(_, f)| f.name == run && f.cache.source_path.as_deref() == Some(&source))
                .map(|(id, _)| *id)
                .or(app.active_file_id);
        }
        if let Some(input) = app.workspace.restore_input.take() {
            app.user_input = input;
        }
        app.state_changed = StateChange::Unchanged;
        app.workspace.apply_bounds = true;
        app.workspace.apply_spectrum_bounds = true;
    }
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub(super) fn figure_svg(app: &MzViewerApp) -> Option<String> {
    if app.workspace.view.compare_samples {
        return comparison_svg(app);
    }
    let mut lines = Vec::new();
    for (id, params) in trace_keys(app, true) {
        let f = &app.files[&id];
        if f.cache.last_processing_params.as_ref() == Some(&params) {
            if let Some(points) = &f.cache.plot_data {
                lines.push((
                    format!("{} · {}", f.name, display_name(app, id, &params)),
                    points.as_slice(),
                    f.display.color,
                ));
            }
        } else if let Some(t) = app
            .workspace
            .traces
            .get(&id)
            .and_then(|ts| ts.iter().find(|t| t.params == params))
        {
            lines.push((
                format!("{} · {}", f.name, t.name),
                t.points.as_slice(),
                t.color,
            ));
        }
    }
    if !app.workspace.overlay && lines.len() > 1 {
        return grid_svg_scaled(
            &lines,
            app.workspace.page,
            app.user_input.line_width,
            app.workspace.view.rows.clamp(1, 8),
            app.workspace.view.columns.clamp(1, 8),
            shared_intensity_maximum(app),
        );
    }
    if lines.is_empty() {
        return None;
    }
    let points: Vec<_> = lines
        .iter()
        .flat_map(|(_, p, _)| p.iter())
        .filter(|p| p[0].is_finite() && p[1].is_finite())
        .collect();
    if points.is_empty() {
        return None;
    }
    let xmin = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let xmax = points
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max)
        .max(xmin + 0.001);
    let ymax = shared_intensity_maximum(app)
        .unwrap_or_else(|| points.iter().map(|p| p[1]).fold(1.0, f64::max) * 1.05);
    let legend_lines: usize = lines
        .iter()
        .map(|(name, _, _)| name.chars().count().div_ceil(130).max(1))
        .sum();
    let height = 580 + legend_lines * 22;
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="{height}" viewBox="0 0 1200 {height}"><rect width="100%" height="100%" fill="white"/><g font-family="Arial,sans-serif" font-size="14" fill="#222"><text x="90" y="30" font-size="22">Chromascope · chromatograms</text><path d="M90 60V480H1160" fill="none" stroke="#222"/><text x="500" y="540">Retention time (min)</text><text transform="translate(22 330) rotate(-90)">Intensity (a.u.)</text>"##
    );
    for i in 0..=5 {
        let x = xmin + (xmax - xmin) * i as f64 / 5.0;
        let y = ymax * i as f64 / 5.0;
        svg.push_str(&format!(r#"<text x="{}" y="505" text-anchor="middle">{x:.3}</text><text x="80" y="{}" text-anchor="end">{y:.2e}</text>"#,90.0+i as f64*214.0,485.0-i as f64*84.0));
    }
    svg.push_str(r#"<defs><clipPath id="overlay-plot"><rect x="90" y="60" width="1070" height="420"/></clipPath></defs>"#);
    let mut legend_row = 0;
    for (label, data, color) in &lines {
        let c = if *color == LineColor::White {
            egui::Color32::BLACK
        } else {
            color.to_egui()
        };
        let color = format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
        let coords = data
            .iter()
            .filter(|p| p[0].is_finite() && p[1].is_finite())
            .map(|p| {
                format!(
                    "{:.3},{:.3}",
                    90.0 + (p[0] - xmin) / (xmax - xmin) * 1070.0,
                    480.0 - p[1] / ymax * 420.0
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        svg.push_str(&format!(
            r#"<polyline clip-path="url(#overlay-plot)" fill="none" stroke="{color}" stroke-width="{}" points="{coords}"/>"#,
            app.user_input.line_width
        ));
        let chars: Vec<_> = label.chars().collect();
        for chunk in chars.chunks(130) {
            let text: String = chunk.iter().collect();
            svg.push_str(&format!(
                r#"<text x="90" y="{}" fill="{color}">{}</text>"#,
                570 + legend_row * 22,
                escape(&text)
            ));
            legend_row += 1;
        }
    }
    svg.push_str("</g></svg>");
    Some(svg)
}
fn comparison_svg(app: &MzViewerApp) -> Option<String> {
    let layout = comparison_layout(app);
    if layout.samples.is_empty() || layout.analytes.is_empty() {
        return None;
    }
    let width = 200 + layout.samples.len() * 1200;
    let height = 50 + layout.analytes.len() * 340;
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}"><rect width="100%" height="100%" fill="white"/>"#
    );
    for (column, id) in layout.samples.iter().enumerate() {
        svg.push_str(&format!(
            r#"<text x="{}" y="28" font-family="Arial,sans-serif" font-size="18">{}</text>"#,
            290 + column * 1200,
            escape(&app.files[id].name)
        ));
    }
    for (row, (name, params)) in layout.analytes.iter().enumerate() {
        svg.push_str(&format!(
            r#"<text x="10" y="{}" font-family="Arial,sans-serif" font-size="16">{}</text>"#,
            90 + row * 340,
            escape(name)
        ));
        for (column, id) in layout.samples.iter().enumerate() {
            let file = &app.files[id];
            let points = if !layout.visible.iter().any(|(f, p)| f == id && p == params) {
                None
            } else if file.cache.last_processing_params.as_ref() == Some(params) {
                file.cache
                    .plot_data
                    .as_deref()
                    .map(|p| (p, file.display.color))
            } else {
                app.workspace
                    .traces
                    .get(id)
                    .and_then(|ts| ts.iter().find(|t| &t.params == params))
                    .map(|t| (t.points.as_slice(), t.color))
            };
            let x = 200 + column * 1200;
            let y = 50 + row * 340;
            if let Some((points, color)) = points {
                if let Some(cell) = grid_svg_scaled(
                    &[(String::new(), points, color)],
                    0,
                    app.user_input.line_width,
                    1,
                    1,
                    shared_intensity_maximum(app),
                ) {
                    // Unique clip IDs for each embedded plot.
                    let cell = cell.replace("plot-0", &format!("matrix-{row}-{column}"));
                    svg.push_str(&cell.replacen("<svg ", &format!("<svg x=\"{x}\" y=\"{y}\" "), 1));
                }
            } else {
                svg.push_str(&format!(r#"<text x="{}" y="{}" font-family="Arial,sans-serif" font-size="16">No visible trace</text>"#, x + 90, y + 100));
            }
        }
    }
    svg.push_str("</svg>");
    Some(svg)
}
#[cfg(test)]
fn stacked_svg(
    lines: &[(String, &[[f64; 2]], LineColor)],
    page: usize,
    line_width: f32,
    horizontal: bool,
) -> Option<String> {
    grid_svg(
        lines,
        page,
        line_width,
        if horizontal { 4 } else { 8 },
        if horizontal { 2 } else { 1 },
    )
}
#[cfg(test)]
fn grid_svg(
    lines: &[(String, &[[f64; 2]], LineColor)],
    page: usize,
    line_width: f32,
    rows: usize,
    columns: usize,
) -> Option<String> {
    grid_svg_scaled(lines, page, line_width, rows, columns, None)
}
fn grid_svg_scaled(
    lines: &[(String, &[[f64; 2]], LineColor)],
    page: usize,
    line_width: f32,
    rows: usize,
    columns: usize,
    shared_maximum: Option<f64>,
) -> Option<String> {
    let capacity = rows.clamp(1, 8) * columns.clamp(1, 8);
    let count = lines.len().saturating_sub(page * capacity).min(capacity);
    if count == 0 {
        return None;
    }
    let selected = lines
        .iter()
        .skip(page * capacity)
        .take(capacity)
        .collect::<Vec<_>>();
    let xmin = selected
        .iter()
        .flat_map(|(_, data, _)| data.iter())
        .map(|p| p[0])
        .fold(f64::INFINITY, f64::min);
    if !xmin.is_finite() {
        return None;
    }
    let xmax = selected
        .iter()
        .flat_map(|(_, data, _)| data.iter())
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max)
        .max(xmin + 0.001);
    let columns = columns.min(count);
    let height = count.div_ceil(columns) * 340;
    let width = columns * 1200;
    let mut svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><rect width="100%" height="100%" fill="white"/>"#
    );
    for (index, (label, data, color)) in selected.iter().enumerate() {
        let ymax =
            shared_maximum.unwrap_or_else(|| data.iter().map(|p| p[1]).fold(1.0, f64::max) * 1.05);
        let c = if *color == LineColor::White {
            egui::Color32::BLACK
        } else {
            color.to_egui()
        };
        let color = format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
        svg.push_str(&format!(r##"<g transform="translate({} {})" font-family="Arial,sans-serif" font-size="13" fill="#222"><path d="M90 80V260H1160" fill="none" stroke="#222"/><text x="510" y="315">Retention time (min)</text><text transform="translate(22 230) rotate(-90)">Intensity (a.u.)</text>"##,(index%columns)*1200,(index/columns)*340));
        let chars: Vec<_> = label.chars().collect();
        for (row, chunk) in chars.chunks(130).take(3).enumerate() {
            svg.push_str(&format!(
                r#"<text x="90" y="{}" fill="{color}">{}</text>"#,
                25 + row * 20,
                escape(&chunk.iter().collect::<String>())
            ));
        }
        for i in 0..=5 {
            let x = xmin + (xmax - xmin) * i as f64 / 5.0;
            let y = ymax * i as f64 / 5.0;
            svg.push_str(&format!(r#"<text x="{}" y="282" text-anchor="middle">{x:.3}</text><text x="80" y="{}" text-anchor="end">{y:.2e}</text>"#,90.0+i as f64*214.0,265.0-i as f64*36.0));
        }
        let coords = data
            .iter()
            .filter(|p| p.iter().all(|v| v.is_finite()))
            .map(|p| {
                format!(
                    "{:.3},{:.3}",
                    90.0 + (p[0] - xmin) / (xmax - xmin) * 1070.0,
                    260.0 - p[1] / ymax * 180.0
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        svg.push_str(&format!(r#"<defs><clipPath id="plot-{index}"><rect x="90" y="80" width="1070" height="180"/></clipPath></defs><polyline clip-path="url(#plot-{index})" fill="none" stroke="{color}" stroke-width="{line_width}" points="{coords}"/></g>"#));
    }
    svg.push_str("</svg>");
    Some(svg)
}
pub(super) fn spectrum_svg(app: &MzViewerApp) -> Option<String> {
    let f = app.files.get(&app.active_file_id?)?;
    let scan = f.cache.mass_spectrum.as_ref()?;
    let peaks: Vec<_> = scan
        .mz
        .iter()
        .copied()
        .zip(scan.intensity.iter().copied())
        .filter(|(mz, i)| mz.is_finite() && i.is_finite())
        .collect();
    let xmin = peaks.iter().map(|(m, _)| *m).fold(f64::INFINITY, f64::min);
    if !xmin.is_finite() {
        return None;
    }
    let xmax = peaks
        .iter()
        .map(|(m, _)| *m)
        .fold(f64::NEG_INFINITY, f64::max)
        .max(xmin + 0.001);
    let ymax = peaks.iter().map(|(_, i)| f64::from(*i)).fold(1.0, f64::max) * 1.05;
    let title = escape(&format!(
        "{} · scan {} · RT {:.5} min",
        f.name,
        scan.index + 1,
        scan.retention_time
    ));
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="560" viewBox="0 0 1200 560"><rect width="100%" height="100%" fill="white"/><g fill="#222" font-family="Arial,sans-serif" font-size="14"><text x="90" y="30" font-size="20">{title}</text><path d="M90 60V480H1160" fill="none" stroke="#222"/><text x="610" y="540">m/z</text><text transform="translate(22 330) rotate(-90)">Intensity (a.u.)</text>"##
    );
    for i in 0..=5 {
        let x = xmin + (xmax - xmin) * i as f64 / 5.0;
        let y = ymax * i as f64 / 5.0;
        svg.push_str(&format!(r#"<text x="{}" y="505" text-anchor="middle">{x:.3}</text><text x="80" y="{}" text-anchor="end">{y:.2e}</text>"#,90.0+i as f64*214.0,485.0-i as f64*84.0));
    }
    let color = f.display.color.to_egui();
    let color = format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b());
    for (mz, intensity) in peaks {
        let x = 90.0 + (mz - xmin) / (xmax - xmin) * 1070.0;
        let y = 480.0 - f64::from(intensity) / ymax * 420.0;
        svg.push_str(&format!(
            r#"<path d="M{x:.3} 480V{y:.3}" stroke="{color}"/>"#
        ));
    }
    svg.push_str("</g></svg>");
    Some(svg)
}
pub(super) fn export_figure(app: &mut MzViewerApp, spectrum: bool) {
    let Some(svg) = (if spectrum {
        spectrum_svg(app)
    } else {
        figure_svg(app)
    }) else {
        app.show_error_dialog("Load a chromatogram before exporting a figure.".into());
        return;
    };
    if let Some(p) = rfd::FileDialog::new()
        .add_filter("Scalable vector graphics", &["svg"])
        .set_file_name(if spectrum {
            "spectrum.svg"
        } else {
            "chromatograms.svg"
        })
        .save_file()
    {
        if let Err(e) = std::fs::write(p, svg) {
            app.show_error_dialog(e.to_string());
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_round_trip_preserves_filters_and_measurements() {
        let input = UserInput {
            mass: super::super::state::ValidatedInput::new(483.0),
            polarity: mzdata::spectrum::ScanPolarity::Negative,
            ..Default::default()
        };
        let session = Session {
            version: 1,
            files: vec![],
            active: None,
            input,
            measurements: vec![],
            dark: false,
            bounds: Some([[1.0, 0.0], [5.0, 100.0]]),
            spectrum_bounds: None,
            overlay: false,
            view: ViewSettings::default(),
            batch_preset: None,
            split: Some(0.6),
        };
        let json = serde_json::to_string(&session).unwrap();
        let restored: Session = serde_json::from_str(&json).unwrap();
        assert!(restored.input == session.input);
        assert_eq!(restored.bounds, session.bounds);
        assert_eq!(csv_field("a,\"b\""), "\"a,\"\"b\"\"\"");
    }
    fn app_with_trace() -> MzViewerApp {
        use super::super::state::{FileCache, FileDisplaySettings, OpenFile};
        let mut app = MzViewerApp::default();
        let params = ProcessingParams {
            acquisition: None,
            plot_type: crate::plotting_parameters::PlotType::Xic,
            ms_level: 1,
            polarity: mzdata::spectrum::ScanPolarity::Positive,
            smoothing: 0,
            xic_params: Some(
                crate::validation::XicParams::new(
                    483.0,
                    mzdata::spectrum::ScanPolarity::Positive,
                    10.0,
                    &crate::validation::DataBounds::unrestricted(),
                )
                .unwrap(),
            ),
            mz_range: None,
            precursor_mz: None,
        };
        let points = vec![[1.0, 0.0], [2.0, 100.0], [3.0, 0.0]];
        app.files.insert(
            7,
            OpenFile {
                id: 7,
                name: "sample & test.mzML".into(),
                path: "sample.mzML".into(),
                data: crate::parser::MzData::new(),
                display: FileDisplaySettings {
                    color: LineColor::Blue,
                    visible: true,
                },
                is_loading: false,
                cache: FileCache {
                    plot_data: Some(points.clone()),
                    display_data: Some(points),
                    chromatogram: Some(ChromatogramData {
                        retention_time: vec![1.0, 2.0, 3.0],
                        intensity: vec![0.0, 100.0, 0.0],
                        mz: vec![],
                        index: vec![0, 1, 2],
                    }),
                    last_processing_params: Some(params),
                    source_path: Some("sample.mzML".into()),
                    ..Default::default()
                },
            },
        );
        app.active_file_id = Some(7);
        app.next_file_id = 8;
        app
    }
    #[test]
    fn retained_xics_select_without_losing_full_resolution_or_parameters() {
        let mut app = app_with_trace();
        retain_current(&mut app, 7);
        retain_current(&mut app, 7);
        assert_eq!(app.workspace.traces[&7].len(), 1);
        let old = current(&app, 7).unwrap();
        let mut new = old.clone();
        new.params.xic_params = Some(
            crate::validation::XicParams::new(
                500.0,
                new.params.polarity,
                5.0,
                &crate::validation::DataBounds::unrestricted(),
            )
            .unwrap(),
        );
        new.points[1][1] = 200.0;
        apply_trace(&mut app, 7, new);
        select_trace(&mut app, 7, 0);
        assert_eq!(app.user_input.mass.value, 483.0);
        assert_eq!(app.user_input.mass_tolerance.value, 10.0);
        assert_eq!(app.files[&7].cache.plot_data.as_ref().unwrap(), &old.points);
        assert_eq!(
            app.workspace.traces[&7][0]
                .params
                .xic_params
                .unwrap()
                .mass(),
            500.0
        );
        assert_eq!(app.workspace.traces[&7].len(), 1);
    }
    #[test]
    fn integrations_keep_parameters_after_trace_edits_and_escape_csv() {
        let mut app = app_with_trace();
        app.integration.start_rt = Some(1.0);
        app.integration.end_rt = Some(3.0);
        super::super::interactivity::compute_integration(&mut app);
        assert_eq!(app.workspace.measurements.len(), 1);
        assert_eq!(app.workspace.measurements[0].area, 100.0);
        app.user_input.mass.value = 999.0;
        app.integration = Default::default();
        assert_eq!(
            app.workspace.measurements[0]
                .params
                .xic_params
                .unwrap()
                .mass(),
            483.0
        );
        let csv = measurements_csv(&app.workspace.measurements);
        assert!(csv.contains("endpoint chord"));
        assert!(csv.contains("483"));
        assert!(csv.contains("\"\"mass\"\""));
    }
    #[test]
    fn session_restores_trace_to_new_file_id_and_keeps_selection() {
        let mut app = app_with_trace();
        let trace = current(&app, 7).unwrap();
        let saved = SavedFile {
            source: "sample.mzML".into(),
            run: app.files[&7].name.clone(),
            current: Some(trace.clone()),
            traces: vec![trace.clone()],
            visible: false,
            scan: None,
        };
        let input = UserInput {
            mass: super::super::state::ValidatedInput::new(483.0),
            ..Default::default()
        };
        let session = Session {
            version: 1,
            files: vec![saved],
            active: Some(0),
            input: input.clone(),
            measurements: vec![],
            dark: true,
            bounds: Some([[1.0, 0.0], [3.0, 120.0]]),
            spectrum_bounds: None,
            overlay: false,
            view: ViewSettings::default(),
            batch_preset: None,
            split: Some(0.58),
        };
        let decoded: Session =
            serde_json::from_slice(&serde_json::to_vec(&session).unwrap()).unwrap();
        decoded.validate().unwrap();
        let mut file = app.files.remove(&7).unwrap();
        file.id = 42;
        file.cache.plot_data = None;
        file.cache.last_processing_params = None;
        app.files.insert(42, file);
        app.workspace.pending = decoded
            .files
            .into_iter()
            .map(|f| (f.source.clone(), f))
            .collect();
        app.workspace.restore_input = Some(decoded.input);
        app.workspace.restore_active = Some(("sample.mzML".into(), "sample & test.mzML".into()));
        restore_loaded(&mut app, &Context::default());
        assert_eq!(app.active_file_id, Some(42));
        assert!(!app.files[&42].display.visible);
        assert_eq!(
            app.files[&42].cache.plot_data.as_ref().unwrap(),
            &trace.points
        );
        assert_eq!(app.workspace.traces[&42][0].display_points.len(), 3);
        assert!(app.user_input == input);
        assert!(app.workspace.restore_input.is_none());
    }
    #[test]
    fn missing_session_source_finishes_recovery_with_error() {
        let mut app = MzViewerApp::default();
        app.workspace.pending.push((
            "missing.mzML".into(),
            SavedFile {
                source: "missing.mzML".into(),
                run: "missing".into(),
                current: None,
                traces: vec![],
                visible: true,
                scan: None,
            },
        ));
        app.workspace.restore_input = Some(UserInput::default());
        restore_loaded(&mut app, &Context::default());
        assert!(app.error_message.is_some());
        assert!(app.workspace.pending.is_empty());
        assert!(app.workspace.restore_input.is_none());
    }
    #[test]
    fn svg_exports_escape_labels_and_respect_visibility() {
        let mut app = app_with_trace();
        let svg = figure_svg(&app).unwrap();
        assert!(svg.contains("sample &amp; test.mzML"));
        assert!(svg.contains("Retention time (min)"));
        assert!(svg.contains("483.0000"));
        app.files.get_mut(&7).unwrap().display.visible = false;
        assert!(figure_svg(&app).is_none());
        app.files.get_mut(&7).unwrap().cache.mass_spectrum = Some(crate::parser::MassSpectrum {
            mz: vec![100.0, 483.0],
            intensity: vec![10.0, 100.0],
            index: 1,
            retention_time: 2.0,
        });
        let svg = spectrum_svg(&app).unwrap();
        assert!(svg.contains("scan 2"));
        assert!(svg.contains("RT 2.00000"));
        assert!(svg.contains(">m/z</text>"));
    }
    #[test]
    fn adjacent_scan_navigation_uses_native_indices_and_actual_rt() {
        let mut app = app_with_trace();
        let f = app.files.get_mut(&7).unwrap();
        f.data
            .open_msfile(&PathBuf::from("test_file/data_dependent_02.mzML"))
            .unwrap();
        load_scan(&mut app, 7, 0);
        navigate(&mut app, -1);
        assert_eq!(app.files[&7].cache.mass_spectrum.as_ref().unwrap().index, 0);
        navigate(&mut app, 1);
        let scan = app.files[&7].cache.mass_spectrum.as_ref().unwrap();
        assert_eq!(scan.index, 1);
        assert_eq!(
            app.user_input.retention_time_ms_spectrum,
            Some(scan.retention_time)
        );
        let metadata = app
            .files
            .get_mut(&7)
            .unwrap()
            .data
            .scan_metadata(1)
            .unwrap();
        assert!(metadata.contains("Native ID:"));
        assert!(metadata.contains("MS level:"));
        assert!(metadata.contains("Actual RT:"));
        let last = app.files[&7].data.bounds.scan_count - 1;
        load_scan(&mut app, 7, last);
        navigate(&mut app, 1);
        assert_eq!(
            app.files[&7].cache.mass_spectrum.as_ref().unwrap().index,
            last
        );
    }
    #[test]
    fn background_update_retains_previous_xic_with_a_distinct_color() {
        let mut app = app_with_trace();
        let previous = current(&app, 7).unwrap();
        let mut next = previous.clone();
        next.params.xic_params = Some(
            crate::validation::XicParams::new(
                500.0,
                next.params.polarity,
                5.0,
                &crate::validation::DataBounds::unrestricted(),
            )
            .unwrap(),
        );
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(crate::processing::ProcessingResult::Success {
            file_id: 7,
            plot_data: next.points.clone(),
            params: next.params.clone(),
            chromatogram: next.chromatogram.clone(),
        })
        .unwrap();
        app.async_state.processing_rx = Some(rx);
        app.async_state.is_processing = true;
        app.poll_processing_result(&Context::default());
        assert_eq!(app.workspace.traces[&7].len(), 1);
        assert_eq!(app.workspace.traces[&7][0].params, previous.params);
        assert_ne!(app.files[&7].display.color, previous.color);
        assert_eq!(
            app.files[&7].cache.last_processing_params.as_ref(),
            Some(&next.params)
        );
        assert!(!app.async_state.is_processing);
        select_trace(&mut app, 7, 0);
        assert_eq!(app.user_input.mass.value, 483.0);
        assert_eq!(app.files[&7].display.color, previous.color);
    }
    #[test]
    fn corrupt_session_trace_is_rejected_before_restoration() {
        let app = app_with_trace();
        let mut trace = current(&app, 7).unwrap();
        trace.points.reverse();
        let session = Session {
            version: 1,
            files: vec![SavedFile {
                source: "sample.mzML".into(),
                run: "sample".into(),
                current: Some(trace),
                traces: vec![],
                visible: true,
                scan: None,
            }],
            active: Some(0),
            input: UserInput::default(),
            measurements: vec![],
            dark: false,
            bounds: None,
            spectrum_bounds: None,
            overlay: false,
            view: ViewSettings::default(),
            batch_preset: None,
            split: None,
        };
        assert!(session.validate().is_err());
    }
    #[test]
    fn long_trace_names_keep_hide_and_delete_controls_accessible() {
        let mut app = app_with_trace();
        let params = app.files[&7].cache.last_processing_params.clone().unwrap();
        app.workspace.names.insert(
            (7, format!("{params:?}")),
            "A very long trace name ".repeat(30),
        );
        let ctx = Context::default();
        let mut controls = (egui::Rect::NOTHING, egui::Rect::NOTHING);
        let mut frame = |app: &mut MzViewerApp, events: Vec<egui::Event>| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(230.0, 150.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        controls = trace_row(app, ui, 7, &params, false);
                    });
                },
            );
            controls
        };
        let (hide, delete) = frame(&mut app, vec![]);
        assert!(hide.right() < delete.left());
        assert!(delete.right() <= 230.0);
        let click = |p: egui::Pos2, pressed| {
            vec![
                egui::Event::PointerMoved(p),
                egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        frame(&mut app, click(hide.center(), true));
        frame(&mut app, click(hide.center(), false));
        assert!(app.workspace.hidden_current.contains(&7));
        assert!(trace_keys(&app, true).is_empty());
        assert_eq!(trace_keys(&app, false).len(), 1);
        frame(&mut app, click(delete.center(), true));
        frame(&mut app, click(delete.center(), false));
        assert!(trace_keys(&app, false).is_empty());
        assert!(app.files[&7].cache.plot_data.is_none());
    }
    #[test]
    fn selecting_retained_panel_preserves_panel_order_and_delete_removes_it() {
        let mut app = app_with_trace();
        retain_current(&mut app, 7);
        let mut t = current(&app, 7).unwrap();
        t.params.smoothing = 3;
        t.order = 1;
        apply_trace(&mut app, 7, t);
        let before = trace_keys(&app, true);
        select_key(&mut app, before[0].0, &before[0].1);
        assert_eq!(trace_keys(&app, true), before);
        delete_key(&mut app, before[0].0, &before[0].1);
        assert_eq!(trace_keys(&app, false).len(), 1);
        assert!(trace_keys(&app, false)
            .iter()
            .all(|(_, p)| p != &before[0].1));
    }
    #[test]
    fn stacked_figures_limit_panels_and_overlay_is_explicit() {
        let mut app = app_with_trace();
        app.workspace.view.rows = 4;
        app.workspace.view.columns = 2;
        retain_current(&mut app, 7);
        for smoothing in 1..10 {
            let mut t = current(&app, 7).unwrap();
            t.params.smoothing = smoothing;
            t.order = usize::from(smoothing);
            t.name = trace_name(&t.params);
            app.workspace.traces.entry(7).or_default().push(t);
        }
        assert!(!app.workspace.overlay);
        let svg = figure_svg(&app).unwrap();
        assert_eq!(svg.matches("<g transform=\"translate(").count(), 8);
        app.workspace.page = 1;
        let svg = figure_svg(&app).unwrap();
        assert_eq!(svg.matches("<g transform=\"translate(").count(), 2);
        app.workspace.overlay = true;
        let svg = figure_svg(&app).unwrap();
        assert_eq!(svg.matches("<polyline").count(), 10);
    }
    #[test]
    fn view_settings_round_trip_and_old_sessions_default_to_visible_panes() {
        let view = ViewSettings {
            focus_restore: None,
            intensity_scale: IntensityScale::SharedCustom,
            intensity_maximum: 12345.0,
            horizontal: true,
            rows: 3,
            columns: 2,
            compare_samples: false,
            files: false,
            inspector: false,
            spectrum: false,
        };
        let restored: ViewSettings =
            serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
        assert!(restored.horizontal);
        assert!(!restored.files);
        assert!(!restored.inspector);
        assert!(!restored.spectrum);
        assert_eq!(restored.intensity_scale, IntensityScale::SharedCustom);
        assert_eq!(restored.intensity_maximum, 12345.0);
        let old: ViewSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.intensity_scale, IntensityScale::Individual);
        assert!(!old.horizontal);
        assert!(old.files && old.inspector && old.spectrum);
    }
    #[test]
    fn shared_scale_uses_visible_full_resolution_traces_and_custom_limits() {
        let mut app = app_with_trace();
        app.workspace.view.intensity_scale = IntensityScale::SharedHighest;
        // Display decimation must not determine the analytical peak maximum.
        app.files.get_mut(&7).unwrap().cache.display_data = Some(vec![[1.0, 0.0]]);
        assert_eq!(shared_intensity_maximum(&app), Some(105.0));
        app.files.get_mut(&7).unwrap().display.visible = false;
        assert_eq!(shared_intensity_maximum(&app), Some(1.0));
        app.workspace.view.intensity_scale = IntensityScale::SharedCustom;
        app.workspace.view.intensity_maximum = 50.0;
        assert_eq!(shared_intensity_maximum(&app), Some(50.0));
        let a = [[0.0, 10.0], [1.0, 100.0]];
        let b = [[0.0, 10.0], [1.0, 20.0]];
        let lines = vec![
            ("a".into(), a.as_slice(), LineColor::Blue),
            ("b".into(), b.as_slice(), LineColor::Red),
        ];
        let svg = grid_svg_scaled(&lines, 0, 2.0, 1, 2, Some(100.0)).unwrap();
        assert_eq!(svg.matches("1.00e2").count(), 2);
        assert!(svg.contains("1160.000,224.000"));
        assert!(svg.contains("clip-path"));
    }
    #[test]
    fn comparison_matrix_aligns_missing_analytes_and_preserves_individual_layout() {
        let mut app = app_with_trace();
        let mut other = app_with_trace().files.remove(&7).unwrap();
        other.id = 8;
        other.name = "Second sample".into();
        other
            .cache
            .last_processing_params
            .as_mut()
            .unwrap()
            .smoothing = 2;
        app.files.insert(8, other);
        app.workspace.view.compare_samples = true;
        app.workspace.overlay = true;
        app.workspace.view.rows = 4;
        app.workspace.view.columns = 3;
        let layout = comparison_layout(&app);
        assert_eq!(layout.samples, vec![7, 8]);
        assert_eq!(layout.analytes.len(), 2);
        assert_eq!(layout.visible.len(), 2);
        let svg = figure_svg(&app).unwrap();
        assert_eq!(svg.matches("No visible trace").count(), 2);
        assert_eq!(svg.matches("<polyline").count(), 2);
        assert!(svg.contains("Second sample"));
        assert_eq!(
            (app.workspace.view.rows, app.workspace.view.columns),
            (4, 3)
        );
        assert!(app.workspace.overlay);
        app.files.get_mut(&8).unwrap().display.visible = false;
        assert_eq!(comparison_layout(&app).samples, vec![7]);
        app.workspace.view.compare_samples = false;
        assert!(!figure_svg(&app).unwrap().contains("No visible trace"));
    }
    #[test]
    fn focus_mode_restores_the_previous_panels_after_session_round_trip() {
        let mut view = ViewSettings {
            inspector: false,
            ..Default::default()
        };
        view.toggle_focus();
        assert_eq!([view.files, view.inspector, view.spectrum], [false; 3]);
        assert_eq!(view.focus_restore, Some([true, false, true]));
        let mut restored: ViewSettings =
            serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
        restored.toggle_focus();
        assert_eq!(
            [restored.files, restored.inspector, restored.spectrum],
            [true, false, true]
        );
        assert!(restored.focus_restore.is_none());
    }
    #[test]
    fn horizontal_figure_uses_columns_and_vertical_figure_uses_rows() {
        let data = vec![[0.0, 0.0], [1.0, 10.0], [2.0, 0.0]];
        let lines = vec![
            ("A".into(), data.as_slice(), LineColor::Blue),
            ("B".into(), data.as_slice(), LineColor::Green),
            ("C".into(), data.as_slice(), LineColor::Red),
        ];
        let horizontal = stacked_svg(&lines, 0, 2.0, true).unwrap();
        assert!(horizontal.contains("width=\"2400\""));
        assert!(horizontal.contains("translate(1200 0)"));
        assert!(horizontal.contains("translate(0 340)"));
        let vertical = stacked_svg(&lines, 0, 2.0, false).unwrap();
        assert!(vertical.contains("width=\"1200\""));
        assert!(vertical.contains("translate(0 680)"));
    }
    #[test]
    fn configurable_grid_can_show_nine_analytes_on_one_page() {
        let data = vec![[0.0, 0.0], [1.0, 10.0], [2.0, 0.0]];
        let lines = (0..9)
            .map(|i| (format!("Analyte {i}"), data.as_slice(), LineColor::Blue))
            .collect::<Vec<_>>();
        let svg = grid_svg(&lines, 0, 2.0, 3, 3).unwrap();
        assert_eq!(svg.matches("<polyline").count(), 9);
        assert!(svg.contains("translate(2400 680)"));
        assert!(grid_svg(&lines, 1, 2.0, 3, 3).is_none());
    }
}
