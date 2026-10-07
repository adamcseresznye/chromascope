//! Validated TOML trace extraction presets, applied to the selected source in a background worker.
use super::state::{MzViewerApp, StateChange};
use crate::{
    plotting_parameters::PlotType,
    processing::{AcquisitionMode, ProcessingParams, ProcessingResult},
};
use mzdata::spectrum::ScanPolarity;
use serde::{Deserialize, Serialize};
use std::sync::mpsc;

#[derive(Default)]
pub(super) struct PresetState {
    pub rx: Option<mpsc::Receiver<Vec<(String, ProcessingResult)>>>,
    pub specs: Option<Vec<(String, ProcessingParams)>>,
    pub applied: std::collections::HashSet<usize>,
    pub failed: std::collections::HashMap<usize, String>,
    pub inflight: Option<usize>,
    pub editor: super::preset_editor::EditorState,
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub(super) struct Preset {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub rows: Option<usize>,
    #[serde(default)]
    pub columns: Option<usize>,
    pub version: u32,
    #[serde(default)]
    pub overlay: bool,
    pub traces: Vec<TraceSpec>,
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub(super) struct TraceSpec {
    pub name: String,
    pub acquisition: AcquisitionMode,
    #[serde(default = "xic")]
    pub kind: String,
    pub mass: Option<f64>,
    #[serde(default = "ppm")]
    pub ppm: f64,
    #[serde(default)]
    pub smoothing: u8,
    #[serde(default = "positive")]
    pub polarity: String,
    pub ms_level: Option<u8>,
    pub precursor_mz: Option<f64>,
    pub mz_range: Option<[f64; 2]>,
}
fn xic() -> String {
    "XIC".into()
}
fn ppm() -> f64 {
    10.0
}
fn positive() -> String {
    "positive".into()
}
pub(super) fn parse(
    text: &str,
    bounds: &crate::validation::DataBounds,
) -> Result<(bool, Vec<(String, ProcessingParams)>), String> {
    let p: Preset = toml::from_str(text).map_err(|e| e.to_string())?;
    if p.rows.is_some_and(|v| !(1..=8).contains(&v))
        || p.columns.is_some_and(|v| !(1..=8).contains(&v))
    {
        return Err("Grid rows and columns must be between 1 and 8.".into());
    }
    if p.version != 1 {
        return Err("Unsupported preset version; use version = 1.".into());
    }
    if p.traces.is_empty() || p.traces.len() > 64 {
        return Err("A preset must contain 1–64 traces.".into());
    }
    let mut specs = Vec::new();
    for t in p.traces {
        if t.name.trim().is_empty() {
            return Err("Every trace needs a non-empty name.".into());
        }
        let polarity = match t.polarity.as_str() {
            "positive" => ScanPolarity::Positive,
            "negative" => ScanPolarity::Negative,
            _ => {
                return Err(format!(
                    "{}: polarity must be positive or negative.",
                    t.name
                ))
            }
        };
        let kind = match t.kind.to_uppercase().as_str() {
            "TIC" => PlotType::Tic,
            "BPC" => PlotType::Bpc,
            "XIC" => PlotType::Xic,
            _ => return Err(format!("{}: kind must be TIC, BPC, or XIC.", t.name)),
        };
        bounds
            .validate_smoothing(t.smoothing)
            .map_err(|e| e.to_string())?;
        let ms_level = t
            .ms_level
            .unwrap_or(if t.acquisition == AcquisitionMode::MRM {
                2
            } else {
                1
            });
        if ms_level == 0 {
            return Err("ms_level must be at least 1.".into());
        }
        if t.acquisition == AcquisitionMode::MRM && t.precursor_mz.is_none() {
            return Err(format!(
                "{}: MRM needs precursor_mz; mass is the product ion m/z.",
                t.name
            ));
        }
        if t.precursor_mz.is_some_and(|m| !m.is_finite() || m <= 0.0) {
            return Err("precursor_mz must be finite and positive.".into());
        }
        if t.mz_range
            .is_some_and(|[a, b]| !a.is_finite() || !b.is_finite() || a < 0.0 || a >= b)
        {
            return Err("mz_range must be an increasing pair of positive masses.".into());
        }
        if kind == PlotType::Xic && t.mz_range.is_some() {
            return Err("Use mass and ppm for XIC; mz_range is for TIC/BPC.".into());
        }
        let xic_params = if kind == PlotType::Xic {
            Some(
                crate::validation::XicParams::new(
                    t.mass
                        .ok_or_else(|| format!("{}: XIC needs mass.", t.name))?,
                    polarity,
                    t.ppm,
                    bounds,
                )
                .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let params = ProcessingParams {
            acquisition: Some(t.acquisition),
            plot_type: kind,
            ms_level,
            polarity,
            smoothing: t.smoothing,
            xic_params,
            mz_range: t.mz_range.map(|[a, b]| (a, b)),
            precursor_mz: t.precursor_mz,
        };
        if specs.iter().any(|(_, p)| p == &params) {
            return Err(format!(
                "{} duplicates another trace's extraction settings.",
                t.name
            ));
        }
        specs.push((t.name, params));
    }
    Ok((p.overlay, specs))
}
pub(super) fn load(app: &mut MzViewerApp) {
    super::preset_editor::open_file(app);
}
pub(super) fn apply_document(app: &mut MzViewerApp, text: &str) -> Result<(), String> {
    if app.async_state.is_processing {
        return Err("Wait for extraction to finish before applying a different preset.".into());
    }
    let document: Preset = toml::from_str(text).map_err(|e| e.to_string())?;
    let (overlay, specs) = parse(text, &crate::validation::DataBounds::unrestricted())?;
    app.presets.specs = Some(specs);
    app.presets.applied.clear();
    app.presets.failed.clear();
    app.workspace.overlay = overlay;
    app.workspace.view.compare_samples = false;
    app.workspace.page = 0;
    if let Some(rows) = document.rows {
        app.workspace.view.rows = rows;
    }
    if let Some(columns) = document.columns {
        app.workspace.view.columns = columns;
    }
    app.state_changed = StateChange::Unchanged;
    schedule(app);
    Ok(())
}
pub(super) fn schedule(app: &mut MzViewerApp) {
    if app.async_state.is_processing
        || app.presets.rx.is_some()
        || app.presets.specs.is_none()
        || app.workspace.restore_input.is_some()
    {
        return;
    }
    let Some(id) = app.active_file_id else {
        return;
    };
    if app.presets.applied.contains(&id) || app.presets.failed.contains_key(&id) {
        return;
    }
    let Some(file) = app.files.get(&id).filter(|f| !f.is_loading) else {
        return;
    };
    let path = std::path::PathBuf::from(&file.path);
    let workspace = file.cache.import_workspace.clone();
    let specs = app.presets.specs.clone().unwrap();
    let (tx, rx) = mpsc::channel();
    app.presets.rx = Some(rx);
    app.presets.inflight = Some(id);
    app.async_state.is_processing = true;
    app.state_changed = StateChange::Unchanged;
    std::thread::spawn(move || {
        let results = specs
            .into_iter()
            .map(|(name, params)| {
                (
                    name,
                    crate::processing::run_in_background(path.clone(), params, id),
                )
            })
            .collect();
        let _ = tx.send(results);
        drop(workspace);
    });
}
pub(super) fn poll(app: &mut MzViewerApp, ctx: &egui::Context) {
    let result = match app.presets.rx.as_ref().map(|rx| rx.try_recv()) {
        Some(Ok(results)) => results,
        Some(Err(mpsc::TryRecvError::Empty)) => {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return;
        }
        Some(Err(mpsc::TryRecvError::Disconnected)) => {
            app.presets.rx = None;
            if let Some(id) = app.presets.inflight.take() {
                app.presets
                    .failed
                    .insert(id, "Extraction worker stopped".into());
            }
            app.async_state.is_processing = false;
            app.show_error_dialog("Preset extraction worker stopped unexpectedly.".into());
            return;
        }
        None => {
            schedule(app);
            return;
        }
    };
    app.presets.rx = None;
    let completed = app.presets.inflight.take();
    app.async_state.is_processing = false;
    // Commit the batch only if every extraction succeeded, preserving the previous workspace on failure.
    let errors: Vec<_> = result
        .iter()
        .filter_map(|(name, r)| {
            if let ProcessingResult::Error { message, .. } = r {
                Some(format!("{name}: {message}"))
            } else {
                None
            }
        })
        .collect();
    if !errors.is_empty() {
        if let Some(id) = completed {
            app.presets.failed.insert(id, errors.join("\n"));
        }
        app.show_error_dialog(errors.join("\n"));
        return;
    }
    if let Some(id) = completed {
        if !app.files.contains_key(&id) {
            schedule(app);
            return;
        }
        app.workspace.traces.remove(&id);
        app.workspace.order.retain(|(file, _), _| *file != id);
        if let Some(f) = app.files.get_mut(&id) {
            f.cache.last_processing_params = None;
            f.cache.plot_data = None;
            f.cache.display_data = None;
            f.cache.chromatogram = None;
        }
        app.presets.applied.insert(id);
    }
    for (name, r) in result {
        if let ProcessingResult::Success {
            file_id, params, ..
        } = &r
        {
            app.workspace
                .names
                .insert((*file_id, format!("{params:?}")), name);
        }
        app.apply_processing_result(r);
    }
    if let Some(id) = app.active_file_id.filter(|id| Some(*id) == completed) {
        super::workspace::activate_file(app, id);
    }
    app.state_changed = StateChange::Unchanged;
    schedule(app);
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preset_validates_modes_transitions_and_ranges() {
        let bounds = crate::validation::DataBounds::unrestricted();
        let text = "version = 1\n[[traces]]\nname = 'Transition'\nacquisition = 'MRM'\nmass = 100.0\nprecursor_mz = 483.0\nppm = 5.0\nsmoothing = 2";
        let (overlay, traces) = parse(text, &bounds).unwrap();
        assert!(!overlay);
        assert_eq!(traces[0].1.ms_level, 2);
        assert_eq!(traces[0].1.acquisition, Some(AcquisitionMode::MRM));
        assert_eq!(traces[0].1.xic_params.unwrap().mass(), 100.0);
        assert!(parse(&text.replace("precursor_mz = 483.0", ""), &bounds).is_err());
        assert!(parse(&text.replace("ppm = 5.0", "ppm = -1.0"), &bounds).is_err());
        assert!(parse(&text.replace("smoothing = 2", "smoothing = 25"), &bounds).is_err());
        assert!(parse(&text.replace("version = 1", "version = 2"), &bounds).is_err());
        assert!(parse(&format!("{text}\nunknown = 1"), &bounds).is_err());
    }
    fn batch_app() -> MzViewerApp {
        use super::super::state::{FileCache, FileDisplaySettings, OpenFile};
        let mut app = MzViewerApp::default();
        let path = std::path::PathBuf::from("test_file/data_dependent_02.mzML");
        for id in [10, 11] {
            let mut data = crate::parser::MzData::new();
            data.open_msfile(&path).unwrap();
            app.files.insert(
                id,
                OpenFile {
                    id,
                    name: format!("sample-{id}"),
                    path: path.to_string_lossy().into_owned(),
                    data,
                    display: FileDisplaySettings {
                        color: crate::plotting_parameters::LineColor::Blue,
                        visible: true,
                    },
                    cache: FileCache::default(),
                    is_loading: false,
                },
            );
        }
        app.active_file_id = Some(10);
        let (_,specs) = parse("version=1\n[[traces]]\nname='Full scan'\nacquisition='FS'\nkind='TIC'\n[[traces]]\nname='Ion'\nacquisition='FS'\nmass=483.0\nppm=10.0",&crate::validation::DataBounds::unrestricted()).unwrap();
        app.presets.specs = Some(specs);
        app
    }
    #[test]
    fn shared_preset_follows_sample_switches_and_caches_without_clutter() {
        let mut app = batch_app();
        schedule(&mut app);
        assert_eq!(app.presets.inflight, Some(10));
        app.active_file_id = Some(11); // Switch while the first sample is still extracting.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.presets.applied.len() < 2 {
            assert!(
                std::time::Instant::now() < deadline,
                "Batch extraction timed out"
            );
            poll(&mut app, &egui::Context::default());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.active_file_id, Some(11));
        let keys = super::super::workspace::trace_keys(&app, true);
        assert_eq!(keys.len(), 2);
        assert!(keys.iter().all(|(id, _)| *id == 11));
        app.active_file_id = Some(10);
        super::super::workspace::activate_file(&mut app, 10);
        poll(&mut app, &egui::Context::default());
        assert!(app.presets.rx.is_none());
        assert!(!app.async_state.is_processing);
        assert!(super::super::workspace::trace_keys(&app, true)
            .iter()
            .all(|(id, _)| *id == 10));
        app.workspace.view.compare_samples = true;
        assert_eq!(super::super::workspace::trace_keys(&app, true).len(), 4);
    }
    #[test]
    fn failed_sample_is_not_retried_on_every_frame() {
        let mut app = batch_app();
        app.presets.specs.as_mut().unwrap()[0].1.acquisition = Some(AcquisitionMode::SIM);
        schedule(&mut app);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.presets.rx.is_some() {
            assert!(std::time::Instant::now() < deadline);
            poll(&mut app, &egui::Context::default());
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(app.presets.failed.contains_key(&10));
        poll(&mut app, &egui::Context::default());
        assert!(app.presets.rx.is_none());
        assert!(super::super::workspace::trace_keys(&app, true).is_empty());
    }
}
