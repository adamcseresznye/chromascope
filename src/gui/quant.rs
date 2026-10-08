//! Batch measurements are independent of exploratory viewer traces and measurements.
use super::{
    presets::{Preset, TraceSpec},
    MzViewerApp,
};
use crate::processing::{self, ProcessingParams, ProcessingResult};
use eframe::egui::{self, Color32};
use egui_plot::{Line, Plot, PlotPoints, Polygon, VLine};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Analyte {
    expected_rt: f64,
    rt_window: [f64; 2],
    extraction: TraceSpec,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Method {
    version: u32,
    name: String,
    detection_smoothing: u8,
    /// Minimum height above the local baseline, in instrument intensity units.
    minimum_height: f64,
    /// Fraction of peak height used to locate its boundaries.
    boundary_fraction: f64,
    analytes: Vec<Analyte>,
}
impl Default for Method {
    fn default() -> Self {
        Self {
            version: 1,
            name: "New quantification method".into(),
            detection_smoothing: 2,
            minimum_height: 0.0,
            boundary_fraction: 0.05,
            analytes: vec![new_analyte(1)],
        }
    }
}
fn new_analyte(n: usize) -> Analyte {
    Analyte {
        expected_rt: 5.0,
        rt_window: [4.0, 6.0],
        extraction: TraceSpec {
            name: format!("Analyte {n}"),
            acquisition: processing::AcquisitionMode::FS,
            kind: "XIC".into(),
            mass: Some(100.0),
            ppm: 10.0,
            smoothing: 0,
            polarity: "positive".into(),
            ms_level: None,
            precursor_mz: None,
            mz_range: None,
        },
    }
}
fn validate(method: &Method) -> Result<Vec<ProcessingParams>, String> {
    if method.version != 1 || method.name.trim().is_empty() {
        return Err("Use method version 1 and enter a method name.".into());
    }
    if method.detection_smoothing > 10
        || !method.minimum_height.is_finite()
        || method.minimum_height < 0.0
        || !method.boundary_fraction.is_finite()
        || !(0.0..=0.5).contains(&method.boundary_fraction)
    {
        return Err("Detection smoothing must be 0–10, minimum height nonnegative, and boundary fraction 0–0.5.".into());
    }
    let mut names = HashSet::new();
    let mut params = Vec::new();
    if method.analytes.is_empty() || method.analytes.len() > 64 {
        return Err("A method needs 1–64 analytes.".into());
    }
    for a in &method.analytes {
        let [lo, hi] = a.rt_window;
        if !lo.is_finite()
            || !hi.is_finite()
            || lo < 0.0
            || lo >= hi
            || !a.expected_rt.is_finite()
            || !(lo..=hi).contains(&a.expected_rt)
        {
            return Err(format!(
                "{}: expected RT must be inside an increasing, nonnegative RT window (minutes).",
                a.extraction.name
            ));
        }
        if !names.insert(a.extraction.name.trim().to_owned()) {
            return Err("Analyte names must be unique.".into());
        }
        if a.extraction.kind != "XIC" || a.extraction.mz_range.is_some() {
            return Err("Batch quantification methods use XIC extraction; set kind = 'XIC' and omit mz_range.".into());
        }
        if a.extraction.smoothing != 0 {
            return Err("Extraction smoothing must be 0: batch areas use unsmoothed data.".into());
        }
        // Validate each extraction independently: two analytes may share a mass but differ in RT.
        let preset = Preset {
            name: method.name.clone(),
            rows: None,
            columns: None,
            version: 1,
            overlay: false,
            traces: vec![a.extraction.clone()],
        };
        let text = toml::to_string(&preset).map_err(|e| e.to_string())?;
        let (_, specs) =
            super::presets::parse(&text, &crate::validation::DataBounds::unrestricted())?;
        params.push(specs[0].1.clone());
    }
    Ok(params)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Peak {
    start: f64,
    end: f64,
    apex_rt: f64,
    height: f64,
    area: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
enum Status {
    Pending,
    Cancelled,
    Automatic,
    Ambiguous,
    Missing,
    Manual,
    Reviewed,
    Failed,
}
#[derive(Clone, Serialize, Deserialize)]
struct Measurement {
    sample: String,
    source: String,
    run: String,
    analyte: String,
    params: ProcessingParams,
    method: String,
    trace: Vec<[f64; 2]>,
    automatic: Option<Peak>,
    peak: Option<Peak>,
    automatic_status: Status,
    status: Status,
    diagnostic: String,
}
fn measure(trace: &[[f64; 2]], start: f64, end: f64) -> Result<Peak, String> {
    if trace.len() < 3
        || !start.is_finite()
        || !end.is_finite()
        || start >= end
        || start < trace[0][0]
        || end > trace[trace.len() - 1][0]
    {
        return Err("Integration bounds must be increasing and inside the measured trace.".into());
    }
    let apex = trace
        .iter()
        .filter(|p| p[0] >= start && p[0] <= end)
        .max_by(|a, b| a[1].total_cmp(&b[1]))
        .ok_or("The interval contains no scans.")?;
    let area = processing::integrate_peak(trace, start, end).map_err(|e| e.to_string())?;
    Ok(Peak {
        start,
        end,
        apex_rt: apex[0],
        height: apex[1],
        area,
    })
}
fn detect(trace: &[[f64; 2]], analyte: &Analyte, method: &Method) -> (Option<Peak>, Status) {
    let window: Vec<_> = trace
        .iter()
        .copied()
        .filter(|p| p[0] >= analyte.rt_window[0] && p[0] <= analyte.rt_window[1])
        .collect();
    if window.len() < 3 {
        return (None, Status::Missing);
    }
    let smooth = processing::smooth_chromatogram(window.clone(), method.detection_smoothing)
        .unwrap_or_else(|_| window.clone());
    let floor = smooth.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let mut candidates = Vec::new();
    for i in 1..smooth.len() - 1 {
        if smooth[i][1] <= smooth[i - 1][1] || smooth[i][1] < smooth[i + 1][1] {
            continue;
        }
        // Reject small ripples on an elevated baseline using local prominence.
        let mut valley_left = i;
        let mut valley_right = i;
        while valley_left > 0 && smooth[valley_left - 1][1] <= smooth[valley_left][1] {
            valley_left -= 1;
        }
        while valley_right + 1 < smooth.len()
            && smooth[valley_right + 1][1] <= smooth[valley_right][1]
        {
            valley_right += 1;
        }
        let prominence = smooth[i][1] - smooth[valley_left][1].max(smooth[valley_right][1]);
        if prominence <= method.minimum_height {
            continue;
        }
        let left_threshold = smooth[valley_left][1]
            + (smooth[i][1] - smooth[valley_left][1]) * method.boundary_fraction;
        let right_threshold = smooth[valley_right][1]
            + (smooth[i][1] - smooth[valley_right][1]) * method.boundary_fraction;
        let clip_threshold = floor + (smooth[i][1] - floor) * method.boundary_fraction;
        let mut left = i;
        let mut right = i;
        while left > valley_left && smooth[left][1] > left_threshold {
            left -= 1;
        }
        while right < valley_right && smooth[right][1] > right_threshold {
            right += 1;
        }
        if let Ok(peak) = measure(trace, smooth[left][0], smooth[right][0]) {
            if peak.area > 0.0 {
                candidates.push((
                    i,
                    peak,
                    (left == 0 && smooth[left][1] > clip_threshold)
                        || (right == smooth.len() - 1 && smooth[right][1] > clip_threshold),
                ));
            }
        }
    }
    candidates.sort_by(|a, b| {
        (smooth[a.0][0] - analyte.expected_rt)
            .abs()
            .total_cmp(&(smooth[b.0][0] - analyte.expected_rt).abs())
    });
    if let Some((_, peak, clipped)) = candidates.first() {
        (
            Some(peak.clone()),
            if candidates.len() > 1 || *clipped {
                Status::Ambiguous
            } else {
                Status::Automatic
            },
        )
    } else {
        (None, Status::Missing)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    version: u32,
    method: Method,
    results: Vec<Measurement>,
    #[serde(default)]
    samples: Vec<(String, String)>,
}
enum Event {
    Result(Box<Measurement>),
    Done(bool),
}
#[derive(Clone, Copy)]
enum MethodAction {
    Open,
    New,
}
pub(super) struct QuantState {
    #[cfg(feature = "mcp")]
    remote_job_id: u64,
    pub active: bool,
    method: Method,
    method_path: Option<PathBuf>,
    saved_method: String,
    selected_samples: HashSet<usize>,
    seen_samples: HashSet<usize>,
    results: Vec<Measurement>,
    selected: Option<usize>,
    rx: Option<mpsc::Receiver<Event>>,
    cancel: Arc<AtomicBool>,
    total: usize,
    completed: usize,
    message: String,
    start: f64,
    end: f64,
    drag_boundary: Option<bool>,
    confirm_replace: bool,
    method_open: bool,
    batch_samples: Vec<(String, String)>,
    restore_samples: Option<Vec<(String, String)>>,
    pending_sources: Vec<PathBuf>,
    pending_method: Option<MethodAction>,
}
impl Default for QuantState {
    fn default() -> Self {
        Self {
            #[cfg(feature = "mcp")]
            remote_job_id: 0,
            active: false,
            method: Method::default(),
            method_path: None,
            saved_method: String::new(),
            selected_samples: HashSet::new(),
            seen_samples: HashSet::new(),
            results: vec![],
            selected: None,
            rx: None,
            cancel: Arc::new(AtomicBool::new(false)),
            total: 0,
            completed: 0,
            message: String::new(),
            start: 0.0,
            end: 0.0,
            drag_boundary: None,
            confirm_replace: false,
            method_open: false,
            batch_samples: vec![],
            restore_samples: None,
            pending_sources: vec![],
            pending_method: None,
        }
    }
}
impl Drop for QuantState {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn signature(method: &Method) -> String {
    toml::to_string(method).unwrap_or_default()
}
fn select(q: &mut QuantState, i: usize) {
    q.selected = Some(i);
    if let Some(p) = &q.results[i].peak {
        q.start = p.start;
        q.end = p.end;
    } else if let Some(a) = q
        .method
        .analytes
        .iter()
        .find(|a| a.extraction.name == q.results[i].analyte)
    {
        q.start = a.rt_window[0];
        q.end = a.rt_window[1];
    } else if let (Some(a), Some(b)) = (q.results[i].trace.first(), q.results[i].trace.last()) {
        q.start = a[0];
        q.end = b[0];
    }
}
fn run(app: &mut MzViewerApp) -> Result<(), String> {
    if app.quant.rx.is_some() {
        return Err("A batch is already running.".into());
    }
    if app
        .files
        .iter()
        .any(|(id, f)| app.quant.selected_samples.contains(id) && f.is_loading)
    {
        return Err(
            "Wait for all selected samples to finish importing before running the batch.".into(),
        );
    }
    let params = validate(&app.quant.method)?;
    let q = &mut app.quant;
    let method = q.method.clone();
    let snapshot = signature(&method);
    let mut samples: Vec<_> = app
        .files
        .iter()
        .filter(|(id, f)| q.selected_samples.contains(id) && !f.is_loading)
        .map(|(_, f)| {
            (
                f.name.clone(),
                f.cache
                    .source_path
                    .clone()
                    .unwrap_or_else(|| f.path.clone()),
                f.path.clone(),
                f.cache.import_workspace.clone(),
            )
        })
        .collect();
    samples.sort_by(|a, b| (&a.1, &a.0).cmp(&(&b.1, &b.0)));
    if samples.is_empty() {
        return Err("Select at least one loaded sample.".into());
    }
    let mut jobs = vec![];
    for (sample, source, path, lease) in samples {
        for (a, params) in method.analytes.iter().zip(&params) {
            // Retain corrections only for exactly the same source, run and method.
            if q.results.iter().any(|r| {
                r.source == source
                    && r.sample == sample
                    && r.analyte == a.extraction.name
                    && r.method == snapshot
                    && matches!(r.status, Status::Manual | Status::Reviewed)
            }) {
                continue;
            }
            jobs.push((
                sample.clone(),
                source.clone(),
                path.clone(),
                lease.clone(),
                a.clone(),
                params.clone(),
            ));
        }
    }
    q.total = jobs.len();
    q.completed = 0;
    for (sample, source, path, _, a, params) in &jobs {
        let pending = Measurement {
            sample: sample.clone(),
            source: source.clone(),
            run: path.clone(),
            analyte: a.extraction.name.clone(),
            params: params.clone(),
            method: snapshot.clone(),
            trace: vec![],
            automatic: None,
            peak: None,
            automatic_status: Status::Pending,
            status: Status::Pending,
            diagnostic: "Awaiting extraction.".into(),
        };
        if let Some(i) = q.results.iter().position(|r| {
            r.source == *source && r.sample == *sample && r.analyte == a.extraction.name
        }) {
            q.results[i] = pending;
        } else {
            q.results.push(pending);
        }
    }
    q.cancel = Arc::new(AtomicBool::new(false));
    let cancel = q.cancel.clone();
    let (tx, rx) = mpsc::channel();
    q.rx = Some(rx);
    q.message = "Running batch; manual corrections for this method are preserved.".into();
    std::thread::spawn(move || {
        for (sample, source, path, _lease, a, params) in jobs {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let mut result = Measurement {
                sample,
                source,
                run: path.clone(),
                analyte: a.extraction.name.clone(),
                params: params.clone(),
                method: snapshot.clone(),
                trace: vec![],
                automatic: None,
                peak: None,
                status: Status::Failed,
                automatic_status: Status::Failed,
                diagnostic: String::new(),
            };
            match processing::run_in_background(path.into(), params, 0) {
                ProcessingResult::Success { plot_data, .. } => {
                    let (peak, status) = detect(&plot_data, &a, &method);
                    result.trace = plot_data;
                    result.automatic = peak.clone();
                    result.peak = peak;
                    result.automatic_status = status.clone();
                    result.status = status;
                }
                ProcessingResult::Error { message, .. } => result.diagnostic = message,
            }
            if tx.send(Event::Result(Box::new(result))).is_err() {
                return;
            }
        }
        let _ = tx.send(Event::Done(cancel.load(Ordering::Relaxed)));
    });
    Ok(())
}
pub(super) fn poll(app: &mut MzViewerApp, ctx: &egui::Context) {
    let q = &mut app.quant;
    loop {
        let event = match q.rx.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(e)) => e,
            Some(Err(mpsc::TryRecvError::Disconnected)) => {
                q.rx = None;
                q.message = "Batch worker stopped.".into();
                for r in &mut q.results {
                    if r.status == Status::Pending {
                        r.status = Status::Failed;
                        r.automatic_status = Status::Failed;
                        r.diagnostic =
                            "Batch worker stopped before this extraction completed.".into();
                    }
                }
                break;
            }
            _ => break,
        };
        match event {
            Event::Result(result) => {
                if let Some(i) = q.results.iter().position(|r| {
                    r.source == result.source
                        && r.sample == result.sample
                        && r.analyte == result.analyte
                }) {
                    q.results[i] = *result;
                    if q.selected == Some(i) {
                        select(q, i);
                    }
                } else {
                    q.results.push(*result);
                }
                q.completed += 1;
            }
            Event::Done(cancelled) => {
                q.rx = None;
                if cancelled {
                    for r in &mut q.results {
                        if r.status == Status::Pending {
                            r.status = Status::Cancelled;
                            r.automatic_status = Status::Cancelled;
                            r.diagnostic = "Batch cancelled before this extraction.".into();
                        }
                    }
                }
                q.message = if cancelled {
                    "Batch cancelled. Completed results are retained."
                } else {
                    "Batch complete. Review missing and ambiguous peaks before export."
                }
                .into();
                break;
            }
        }
    }
    if q.rx.is_some() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}
pub(super) fn shortcuts(app: &mut MzViewerApp, ctx: &egui::Context) {
    if ctx.input_mut(|i| {
        i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::O,
        ))
    }) {
        super::panels::handle_file_selection(app);
    }
    if app.quant.rx.is_none()
        && ctx.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::S,
            ))
        })
    {
        if let Err(e) = batch_file(&mut app.quant, true) {
            app.quant.message = e;
        }
    }
}
fn csv(results: &[Measurement], method: &Method) -> String {
    fn cell(s: &str) -> String {
        format!("\"{}\"", s.replace('"', "\"\""))
    }
    let mut out = "sample,source,run,analyte,method_name,status,needs_recalculation,area_intensity_min,apex_rt_min,start_rt_min,end_rt_min,height,acquisition,ms_level,polarity,target_mz,ppm,precursor_mz,mz_range,baseline,area_source,diagnostic,method_toml\n".to_owned();
    let current = signature(method);
    for r in results {
        let p = r.peak.as_ref();
        let method_name = toml::from_str::<Method>(&r.method)
            .map(|m| m.name)
            .unwrap_or_default();
        let fields = vec![
            r.sample.clone(),
            r.source.clone(),
            r.run.clone(),
            r.analyte.clone(),
            method_name,
            format!("{:?}", r.status),
            (r.method != current).to_string(),
            p.map(|p| p.area.to_string()).unwrap_or_default(),
            p.map(|p| p.apex_rt.to_string()).unwrap_or_default(),
            p.map(|p| p.start.to_string()).unwrap_or_default(),
            p.map(|p| p.end.to_string()).unwrap_or_default(),
            p.map(|p| p.height.to_string()).unwrap_or_default(),
            format!("{:?}", r.params.acquisition),
            r.params.ms_level.to_string(),
            format!("{:?}", r.params.polarity),
            r.params
                .xic_params
                .as_ref()
                .map(|x| x.mass().to_string())
                .unwrap_or_default(),
            r.params
                .xic_params
                .as_ref()
                .map(|x| x.mass_tolerance().to_string())
                .unwrap_or_default(),
            r.params
                .precursor_mz
                .map(|m| m.to_string())
                .unwrap_or_default(),
            format!("{:?}", r.params.mz_range),
            "linear".into(),
            "unsmoothed".into(),
            r.diagnostic.clone(),
            r.method.clone(),
        ];
        out.push_str(&fields.iter().map(|s| cell(s)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

fn save_method(q: &mut QuantState, save_as: bool) -> Result<(), String> {
    validate(&q.method)?;
    let path = if save_as || q.method_path.is_none() {
        rfd::FileDialog::new()
            .add_filter("Quantification method", &["toml"])
            .set_file_name("quant-method.toml")
            .save_file()
    } else {
        q.method_path.clone()
    };
    if let Some(path) = path {
        let text = signature(&q.method);
        std::fs::write(&path, &text).map_err(|e| e.to_string())?;
        q.saved_method = text;
        q.method_path = Some(path);
        q.message = "Method saved.".into();
    }
    Ok(())
}
fn open_method(q: &mut QuantState) -> Result<(), String> {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("Quantification method", &["toml"])
        .pick_file()
    {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let method: Method = toml::from_str(&text).map_err(|e| e.to_string())?;
        validate(&method)?;
        q.method = method;
        q.saved_method = signature(&q.method);
        q.method_path = Some(path);
        q.message =
            "Method opened. Existing results retain their original method snapshots.".into();
    }
    Ok(())
}
fn change_method(q: &mut QuantState, action: MethodAction) {
    match action {
        MethodAction::Open => {
            if let Err(e) = open_method(q) {
                q.message = e;
            }
        }
        MethodAction::New => {
            q.method = Method::default();
            q.method_path = None;
            q.saved_method.clear();
        }
    }
}
fn request_method(q: &mut QuantState, action: MethodAction) {
    let current = signature(&q.method);
    if current != q.saved_method && current != signature(&Method::default()) {
        q.pending_method = Some(action);
    } else {
        change_method(q, action);
    }
}
fn batch_file(q: &mut QuantState, save: bool) -> Result<(), String> {
    let dialog = rfd::FileDialog::new().add_filter("Quantification batch", &["chromquant"]);
    if save {
        if let Some(path) = dialog.set_file_name("batch.chromquant").save_file() {
            validate(&q.method)?;
            let batch = Batch {
                version: 1,
                method: q.method.clone(),
                results: q.results.clone(),
                samples: q.batch_samples.clone(),
            };
            std::fs::write(path, serde_json::to_vec(&batch).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            q.message = "Batch saved, including traces and manual corrections.".into();
        }
    } else if let Some(path) = dialog.pick_file() {
        let batch: Batch = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        validate_batch(&batch)?;
        q.method = batch.method;
        q.results = batch.results;
        q.selected = None;
        q.method_path = None;
        q.saved_method = String::new();
        q.pending_sources = batch
            .samples
            .iter()
            .map(|(source, _)| PathBuf::from(source))
            .collect();
        q.pending_sources.sort();
        q.pending_sources.dedup();
        q.restore_samples = Some(batch.samples.clone());
        q.batch_samples = batch.samples;
        q.seen_samples.clear();
        q.selected_samples.clear();
        q.message="Batch restored. Reopening its source datasets; stored results remain available if a source is missing.".into();
    }
    Ok(())
}
fn validate_batch(batch: &Batch) -> Result<(), String> {
    if batch.version != 1 {
        return Err("Unsupported batch version.".into());
    }
    validate(&batch.method)?;
    let mut keys = HashSet::new();
    for r in &batch.results {
        let method: Method = toml::from_str(&r.method).map_err(|e| e.to_string())?;
        let params = validate(&method)?;
        let i = method
            .analytes
            .iter()
            .position(|a| a.extraction.name == r.analyte)
            .ok_or("Result analyte is absent from its method.")?;
        if matches!(
            r.status,
            Status::Manual | Status::Reviewed | Status::Automatic | Status::Ambiguous
        ) && r.peak.is_none()
            || matches!(
                r.status,
                Status::Missing | Status::Failed | Status::Pending | Status::Cancelled
            ) && r.peak.is_some()
            || matches!(r.automatic_status, Status::Manual | Status::Reviewed)
            || matches!(r.automatic_status, Status::Automatic | Status::Ambiguous)
                != r.automatic.is_some()
        {
            return Err("Inconsistent peak status in batch.".into());
        }
        if params[i] != r.params
            || !keys.insert((&r.source, &r.sample, &r.analyte))
            || r.trace
                .iter()
                .any(|p| !p[0].is_finite() || !p[1].is_finite() || p[0] < 0.0 || p[1] < 0.0)
            || r.trace.windows(2).any(|w| w[0][0] >= w[1][0])
        {
            return Err("Invalid or duplicate result data in batch.".into());
        }
        for p in [&r.peak, &r.automatic].into_iter().flatten() {
            let actual = measure(&r.trace, p.start, p.end)?;
            if !p.area.is_finite()
                || (actual.area - p.area).abs() > 1e-6 * actual.area.abs().max(1.0)
                || actual.apex_rt != p.apex_rt
                || actual.height != p.height
            {
                return Err("Invalid integration in batch.".into());
            }
        }
    }
    Ok(())
}

pub(super) fn show(app: &mut MzViewerApp, ctx: &egui::Context) {
    let mut q = std::mem::take(&mut app.quant);
    egui::SidePanel::left("quant_samples")
        .default_width(225.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.heading("Batch samples");
            ui.small("Select the loaded runs to quantify.");
            if ui.button("Add samples…").clicked() {
                if let Some(paths) = rfd::FileDialog::new().pick_files() {
                    super::panels::queue_file_imports(app, paths);
                }
            }
            if ui.button("Add dataset folder…").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    super::panels::queue_file_imports(app, vec![path]);
                }
            }
            ui.add_enabled_ui(q.rx.is_none(), |ui| {
                ui.horizontal(|ui| {
                    if ui.small_button("Select all").clicked() {
                        q.selected_samples.extend(app.files.keys());
                    }
                    if ui.small_button("Select none").clicked() {
                        q.selected_samples.clear();
                    }
                });
                let mut files: Vec<_> = app.files.iter().collect();
                files.sort_by_key(|(id, _)| **id);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (&id, f) in files {
                        if !f.is_loading && q.seen_samples.insert(id) {
                            let key = (
                                f.cache
                                    .source_path
                                    .clone()
                                    .unwrap_or_else(|| f.path.clone()),
                                f.name.clone(),
                            );
                            if q.restore_samples
                                .as_ref()
                                .is_none_or(|samples| samples.contains(&key))
                            {
                                q.selected_samples.insert(id);
                            }
                        }
                        let mut selected = q.selected_samples.contains(&id);
                        ui.horizontal(|ui| {
                            let check = ui
                                .add_enabled(!f.is_loading, egui::Checkbox::new(&mut selected, ""));
                            let label = ui
                                .add_enabled(
                                    !f.is_loading,
                                    egui::Label::new(&f.name)
                                        .truncate()
                                        .sense(egui::Sense::click()),
                                )
                                .on_hover_text(f.cache.source_path.as_deref().unwrap_or(&f.path));
                            if label.clicked() {
                                selected = !selected;
                            }
                            if check.changed() || label.clicked() {
                                if selected {
                                    q.selected_samples.insert(id);
                                } else {
                                    q.selected_samples.remove(&id);
                                }
                            }
                            if f.is_loading {
                                ui.spinner();
                            }
                        });
                    }
                });
            });
            ui.separator();
            let selected = app
                .files
                .iter()
                .filter(|(id, _)| q.selected_samples.contains(id))
                .count();
            ui.small(format!("{selected} selected / {} loaded", app.files.len()));
            ui.small("Areas are intensity × minutes. No concentration calibration is applied.");
        });
    q.batch_samples = app
        .files
        .iter()
        .filter(|(id, _)| q.selected_samples.contains(id))
        .map(|(_, f)| {
            (
                f.cache
                    .source_path
                    .clone()
                    .unwrap_or_else(|| f.path.clone()),
                f.name.clone(),
            )
        })
        .collect();
    if let Some(samples) = &q.restore_samples {
        for sample in samples {
            if !app.files.values().any(|f| {
                f.cache.source_path.as_deref().unwrap_or(&f.path) == sample.0 && f.name == sample.1
            }) {
                q.batch_samples.push(sample.clone());
            }
        }
    }
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("Batch Quantification");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Edit method…").clicked() {
                q.method_open = true;
            }
            ui.add_enabled_ui(q.rx.is_none(), |ui| {
                let selected = app
                    .files
                    .iter()
                    .filter(|(id, _)| q.selected_samples.contains(id))
                    .collect::<Vec<_>>();
                let ready = !selected.is_empty()
                    && selected.iter().all(|(_, f)| !f.is_loading)
                    && validate(&q.method).is_ok();
                if ui
                    .add_enabled(
                        ready,
                        egui::Button::new(egui::RichText::new("Run batch").strong())
                            .fill(ui.visuals().selection.bg_fill),
                    )
                    .on_hover_text(
                        "Select loaded samples and define a valid method to run the batch",
                    )
                    .clicked()
                {
                    app.quant = std::mem::take(&mut q);
                    let result = run(app);
                    q = std::mem::take(&mut app.quant);
                    if let Err(e) = result {
                        q.message = e;
                    }
                }
                ui.menu_button("Batch session", |ui| {
                    if ui.button("Open batch…").clicked() {
                        q.confirm_replace = true;
                        ui.close();
                    }
                    if ui.button("Save batch…").clicked() {
                        if let Err(e) = batch_file(&mut q, true) {
                            q.message = e;
                        }
                        ui.close();
                    }
                });
            });
            if q.rx.is_some() {
                ui.spinner();
                ui.label(format!("{} / {}", q.completed, q.total));
                if ui.button("Cancel").clicked() {
                    q.cancel.store(true, Ordering::Relaxed);
                    q.message = "Stopping after the current extraction…".into();
                }
            }
            if ui
                .add_enabled(
                    !q.results.is_empty() && q.rx.is_none(),
                    egui::Button::new("Export CSV…"),
                )
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .set_file_name("quantification.csv")
                    .save_file()
                {
                    match std::fs::write(path, csv(&q.results, &q.method)) {
                        Ok(()) => {
                            q.message =
                                "Results exported, including review status and method snapshots."
                                    .into()
                        }
                        Err(e) => q.message = e.to_string(),
                    }
                }
            }
        });
        if !q.message.is_empty() {
            ui.label(&q.message);
        }
        ui.separator();
        ui.label(format!(
            "Method: {} · {} analytes",
            q.method.name,
            q.method.analytes.len()
        ));
        if signature(&q.method) != q.saved_method {
            ui.small(
                "Method has unsaved changes. Save it in the method editor for future batches.",
            );
        }
        ui.separator();
        results_table(&mut q, ui);
        ui.separator();
        review(&mut q, ui);
    });
    if q.method_open {
        let mut open = true;
        egui::Window::new("Batch quantification method")
            .open(&mut open)
            .default_size(egui::vec2(1100.0, 440.0))
            .resizable(true)
            .show(ctx, |ui| {
                ui.add_enabled_ui(q.rx.is_none() && q.pending_method.is_none(), |ui| {
                    method_editor(&mut q, ui);
                });
            });
        q.method_open = open;
    }
    if q.confirm_replace {
        egui::Window::new("Open another batch?").collapsible(false).show(ctx, |ui| {
            ui.label("Opening a batch replaces the current method and results. Save this batch first to keep your corrections.");
            if ui.button("Save current batch…").clicked() { if let Err(e)=batch_file(&mut q,true) { q.message=e; } }
            if ui.button("Choose batch to open…").clicked() { q.confirm_replace=false; if let Err(e)=batch_file(&mut q,false) { q.message=e; } }
            if ui.button("Cancel").clicked() { q.confirm_replace=false; }
        });
    }
    if let Some(action) = q.pending_method {
        egui::Window::new("Unsaved method changes")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Save your method changes before continuing?");
                ui.horizontal(|ui| {
                    if ui.button("Save and continue").clicked() {
                        match save_method(&mut q, false) {
                            Ok(()) if signature(&q.method) == q.saved_method => {
                                q.pending_method = None;
                                change_method(&mut q, action);
                            }
                            Err(e) => q.message = e,
                            _ => {}
                        }
                    }
                    if ui.button("Discard changes").clicked() {
                        q.pending_method = None;
                        change_method(&mut q, action);
                    }
                    if ui.button("Cancel").clicked() {
                        q.pending_method = None;
                    }
                });
            });
    }
    app.quant = q;
    let sources = std::mem::take(&mut app.quant.pending_sources);
    let mut missing = Vec::new();
    let mut imports = Vec::new();
    for path in sources {
        if !path.exists() {
            missing.push(path.display().to_string());
        } else if !app
            .files
            .values()
            .any(|f| f.cache.source_path.as_deref().unwrap_or(&f.path) == path.to_string_lossy())
        {
            imports.push(path);
        }
    }
    if !imports.is_empty() {
        super::panels::queue_file_imports(app, imports);
    }
    if !missing.is_empty() {
        app.quant.message = format!(
            "Batch restored; missing source datasets: {}. Stored results remain available.",
            missing.join(", ")
        );
    }
}

fn method_editor(q: &mut QuantState, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        ui.horizontal_wrapped(|ui| {
            if ui.button("New").clicked() {request_method(q,MethodAction::New);}
            if ui.button("Open…").clicked() {request_method(q,MethodAction::Open);}
            if ui.button("Save").clicked() { if let Err(e)=save_method(q,false) { q.message=e; } }
            if ui.button("Save As…").clicked() { if let Err(e)=save_method(q,true) { q.message=e; } }
            if let Some(result)=super::examples::menu(ui) { q.message=match result {
                Ok(path)=>format!("Example saved to {}. Choose Open method for the quantification example; open extraction examples in the viewer preset editor.",path.display()),
                Err(e)=>e,
            }; }
            if signature(&q.method)!=q.saved_method { ui.small("Unsaved changes"); }
        });
        ui.horizontal_wrapped(|ui| {ui.label("Method name");ui.add(egui::TextEdit::singleline(&mut q.method.name).desired_width(280.0));});
        if let Some(path)=&q.method_path { ui.small(path.display().to_string()); }
        ui.horizontal_wrapped(|ui| {
            ui.label("Detection smoothing"); ui.add(egui::DragValue::new(&mut q.method.detection_smoothing).range(0..=10));
            ui.label("Minimum peak prominence"); ui.add(egui::DragValue::new(&mut q.method.minimum_height).range(0.0..=f64::MAX));
            ui.label("Boundary fraction"); ui.add(egui::DragValue::new(&mut q.method.boundary_fraction).speed(0.01).range(0.0..=0.5));
        });
        ui.small("Detection uses a smoothed trace. Areas use unsmoothed data and a straight-line baseline. RT is in minutes.");
        let mut remove=None;
        egui::ScrollArea::both().max_height(230.0).show(ui,|ui| {
            egui::Grid::new("quant_method_table").striped(true).show(ui,|ui| {
                for h in ["Analyte","Mode","Target m/z","Precursor m/z","ppm","Polarity","MS level","Expected RT","Window start","Window end",""] { ui.strong(h); } ui.end_row();
                for (i,a) in q.method.analytes.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(&mut a.extraction.name).desired_width(130.0).min_size(egui::vec2(130.0,28.0)));
                        egui::ComboBox::from_id_salt((i,"mode")).width(60.0).selected_text(format!("{:?}",a.extraction.acquisition)).show_ui(ui,|ui| {
                            for mode in [processing::AcquisitionMode::FS,processing::AcquisitionMode::SIM,processing::AcquisitionMode::MRM] { ui.selectable_value(&mut a.extraction.acquisition,mode,format!("{mode:?}")); }
                        });
                        ui.add(egui::DragValue::new(a.extraction.mass.get_or_insert(100.0)).speed(0.01).range(0.0001..=1e8));
                        let mut use_precursor=a.extraction.precursor_mz.is_some();
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut use_precursor,"").changed() { a.extraction.precursor_mz=if use_precursor {Some(100.0)} else {None}; }
                            if let Some(m)=&mut a.extraction.precursor_mz { ui.add(egui::DragValue::new(m).speed(0.01).range(0.0001..=1e8)); }
                        });
                        ui.add(egui::DragValue::new(&mut a.extraction.ppm).range(0.0..=1000.0));
                        egui::ComboBox::from_id_salt((i,"polarity")).selected_text(&a.extraction.polarity).show_ui(ui,|ui| { for p in ["positive","negative"] { ui.selectable_value(&mut a.extraction.polarity,p.into(),p); } });
                        let mut level=a.extraction.ms_level.unwrap_or(if a.extraction.acquisition==processing::AcquisitionMode::MRM {2} else {1});
                        if ui.add(egui::DragValue::new(&mut level).range(1..=10)).changed() { a.extraction.ms_level=Some(level); }
                        ui.add(egui::DragValue::new(&mut a.expected_rt).speed(0.01).range(0.0..=1e6));
                        ui.add(egui::DragValue::new(&mut a.rt_window[0]).speed(0.01).range(0.0..=1e6));
                        ui.add(egui::DragValue::new(&mut a.rt_window[1]).speed(0.01).range(0.0..=1e6));
                        if ui.small_button("Remove").clicked() { remove=Some(i); }
                        ui.end_row();
                }
            });
        });
        if let Some(i)=remove { q.method.analytes.remove(i); }
        if ui.add_enabled(q.method.analytes.len()<64,egui::Button::new("Add analyte")).clicked() { q.method.analytes.push(new_analyte(q.method.analytes.len()+1)); }
        if let Err(e)=validate(&q.method) { ui.colored_label(Color32::from_rgb(210,110,50),e); }
        if !q.message.is_empty() { ui.small(&q.message); }
    });
}
fn results_table(q: &mut QuantState, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.strong("Batch results");
        if ui.button("Next unresolved").clicked() && !q.results.is_empty() {
            let start = q.selected.map_or(0, |i| i + 1);
            if let Some(i) = (0..q.results.len())
                .map(|offset| (start + offset) % q.results.len())
                .find(|&i| {
                    matches!(
                        q.results[i].status,
                        Status::Missing
                            | Status::Ambiguous
                            | Status::Failed
                            | Status::Cancelled
                            | Status::Pending
                    ) || q.results[i].method != signature(&q.method)
                })
            {
                select(q, i);
            }
        }
    });
    if q.results.is_empty() {
        ui.label("Define your method, select samples, then run the batch. Click an area to review its peak.");
        return;
    }
    let current = signature(&q.method);
    let mut samples: Vec<_> = q
        .results
        .iter()
        .map(|r| (r.source.clone(), r.sample.clone()))
        .collect();
    samples.sort();
    samples.dedup();
    let mut analytes: Vec<_> = q
        .method
        .analytes
        .iter()
        .map(|a| a.extraction.name.clone())
        .collect();
    for r in &q.results {
        if !analytes.contains(&r.analyte) {
            analytes.push(r.analyte.clone());
        }
    }
    let mut clicked = None;
    egui::ScrollArea::both().max_height(220.0).show(ui, |ui| {
        egui::Grid::new("quant_result_matrix")
            .striped(true)
            .show(ui, |ui| {
                ui.strong("Sample / area");
                for a in &analytes {
                    ui.strong(a);
                }
                ui.end_row();
                for (source, sample) in samples {
                    ui.label(&sample).on_hover_text(&source);
                    for a in &analytes {
                        if let Some(i) = q.results.iter().position(|r| {
                            r.source == source && r.sample == sample && r.analyte == *a
                        }) {
                            let r = &q.results[i];
                            let stale = r.method != current;
                            let label = format!(
                                "{} · {}",
                                r.peak
                                    .as_ref()
                                    .map(|p| format!("{:.3}", p.area))
                                    .unwrap_or_else(|| "—".into()),
                                if stale {
                                    "Recalculate".into()
                                } else {
                                    format!("{:?}", r.status)
                                }
                            );
                            if ui
                                .selectable_label(q.selected == Some(i), label)
                                .on_hover_text(&r.diagnostic)
                                .clicked()
                            {
                                clicked = Some(i);
                            }
                        } else {
                            ui.label("Pending");
                        }
                    }
                    ui.end_row();
                }
            });
    });
    if let Some(i) = clicked {
        select(q, i);
    }
}
fn review(q: &mut QuantState, ui: &mut egui::Ui) {
    let Some(i) = q.selected.filter(|&i| i < q.results.len()) else {
        ui.small("Select a result to inspect or adjust its integration.");
        return;
    };
    let r = &q.results[i];
    ui.strong(format!("{} / {}", r.sample, r.analyte));
    if r.trace.is_empty() {
        ui.label(&r.diagnostic);
        return;
    }
    let stale = r.method != signature(&q.method);
    if stale {
        ui.colored_label(
            Color32::from_rgb(210, 110, 50),
            "Method changed: rerun this sample before adjusting the result.",
        );
    }
    let mut apply = false;
    let mut reset = false;
    let draft_changed = r
        .peak
        .as_ref()
        .is_none_or(|p| p.start != q.start || p.end != q.end);
    if let Some(p) = &r.peak {
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!("Recorded area: {:.4e} a.u.·min", p.area));
            ui.label(format!("Apex: {:.3} min", p.apex_rt));
            ui.label(format!("{:?}", r.status));
        });
    }
    ui.add_enabled_ui(!stale && q.rx.is_none(), |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Start (min)");
            ui.add(egui::DragValue::new(&mut q.start).speed(0.01));
            ui.label("End (min)");
            ui.add(egui::DragValue::new(&mut q.end).speed(0.01));
            apply = ui
                .add_enabled(draft_changed, egui::Button::new("Apply bounds"))
                .clicked();
            reset = ui
                .add_enabled(
                    r.peak != r.automatic || r.status != r.automatic_status,
                    egui::Button::new("Reset to automatic"),
                )
                .clicked();
        });
    });
    let accept = ui
        .add_enabled(
            !stale
                && q.rx.is_none()
                && r.peak.is_some()
                && !draft_changed
                && matches!(r.status, Status::Automatic | Status::Ambiguous),
            egui::Button::new("Accept peak"),
        )
        .clicked();
    ui.small("Drag a boundary with the left mouse button, or right-drag an interval. Manual edits affect only this sample and analyte.");
    ui.small(if draft_changed && r.peak.is_some() {
        "Boundary edits are not yet applied."
    } else {
        " "
    });
    let mut mouse = None;
    let mut boundaries = None;
    let plot = Plot::new(("quant_review", i))
        .height(ui.available_height().max(100.0))
        .x_axis_label("Retention time (min)")
        .y_axis_label("Intensity")
        .allow_boxed_zoom(false)
        .allow_drag(false);
    let response = plot.show(ui, |plot_ui| {
        let display = processing::decimate_for_display(&r.trace);
        plot_ui.line(
            Line::new("Unsmoothed XIC", PlotPoints::from(display))
                .color(Color32::from_rgb(75, 150, 215)),
        );
        plot_ui.vline(VLine::new("Start", q.start).color(Color32::from_rgb(50, 160, 115)));
        plot_ui.vline(VLine::new("End", q.end).color(Color32::from_rgb(220, 140, 60)));
        if q.start < q.end {
            let y0 = processing::interpolate_at(&r.trace, q.start);
            let y1 = processing::interpolate_at(&r.trace, q.end);
            plot_ui
                .line(Line::new("Baseline", vec![[q.start, y0], [q.end, y1]]).color(Color32::GRAY));
            let mut points = vec![[q.start, y0]];
            let region: Vec<_> = r
                .trace
                .iter()
                .copied()
                .filter(|p| p[0] > q.start && p[0] < q.end)
                .collect();
            points.extend(processing::decimate_for_display(&region));
            points.push([q.end, y1]);
            plot_ui.polygon(
                Polygon::new("Integrated peak", points)
                    .stroke(egui::Stroke::new(
                        1.0_f32,
                        Color32::from_rgba_unmultiplied(75, 150, 215, 100),
                    ))
                    .fill_color(Color32::from_rgba_unmultiplied(75, 150, 215, 45)),
            );
        }
        // This plot does not pan; pointer_coordinate compensates for drag movement
        // and would otherwise leave an integration handle at its press position.
        mouse = plot_ui
            .ctx()
            .input(|i| i.pointer.latest_pos())
            .map(|pos| plot_ui.plot_from_screen(pos).x);
        boundaries = Some((
            plot_ui
                .screen_from_plot(egui_plot::PlotPoint::new(q.start, 0.0))
                .x,
            plot_ui
                .screen_from_plot(egui_plot::PlotPoint::new(q.end, 0.0))
                .x,
        ));
    });
    #[cfg(test)]
    ui.ctx().data_mut(|d| {
        d.insert_temp(
            egui::Id::new("quant_test_plot"),
            (response.response.rect, response.transform),
        )
    });
    if !stale && q.rx.is_none() {
        let resp = &response.response;
        if let Some(x) = mouse {
            if resp.drag_started_by(egui::PointerButton::Secondary) {
                q.start = ui
                    .input(|i| i.pointer.press_origin())
                    .map(|pos| response.transform.value_from_position(pos).x)
                    .unwrap_or(x);
                q.end = x;
                q.drag_boundary = None;
            }
            if resp.dragged_by(egui::PointerButton::Secondary) {
                q.end = x;
            }
            if resp.drag_stopped_by(egui::PointerButton::Secondary) {
                q.end = x;
                if q.start > q.end {
                    std::mem::swap(&mut q.start, &mut q.end);
                }
                apply = true;
            }
            if resp.drag_started_by(egui::PointerButton::Primary) {
                if let (Some((s, e)), Some(pos)) =
                    (boundaries, ui.input(|i| i.pointer.press_origin()))
                {
                    if (pos.x - s).abs().min((pos.x - e).abs()) < 15.0 {
                        q.drag_boundary = Some((pos.x - s).abs() < (pos.x - e).abs());
                    }
                }
            }
            if resp.dragged_by(egui::PointerButton::Primary) {
                if let Some(start) = q.drag_boundary {
                    if start {
                        q.start = x;
                    } else {
                        q.end = x;
                    }
                    apply = true;
                }
            }
        }
        if resp.drag_stopped_by(egui::PointerButton::Primary) {
            q.drag_boundary = None;
        }
    }
    if accept && q.results[i].status != Status::Manual {
        q.results[i].status = Status::Reviewed;
    }
    if reset {
        q.results[i].peak = q.results[i].automatic.clone();
        q.results[i].status = q.results[i].automatic_status.clone();
        select(q, i);
    }
    if apply {
        match measure(&q.results[i].trace, q.start, q.end) {
            Ok(peak) => {
                q.results[i].peak = Some(peak);
                q.results[i].status = Status::Manual;
                q.message =
                    "Manual integration saved in this batch. Save batch to retain it on disk."
                        .into();
            }
            Err(e) => q.message = e,
        }
    }
}

#[cfg(feature = "mcp")]
pub(super) fn remote_start(
    app: &mut MzViewerApp,
    text: &str,
    ids: Vec<usize>,
) -> Result<(), String> {
    if app.quant.rx.is_some() {
        return Err("Batch already running".into());
    }
    let method: Method = toml::from_str(text).map_err(|e| e.to_string())?;
    validate(&method)?;
    if ids.is_empty() || ids.iter().any(|id| !app.files.contains_key(id)) {
        return Err("Select existing dataset IDs".into());
    }
    app.quant.method = method;
    app.quant.selected_samples = ids.into_iter().collect();
    run(app)?;
    app.quant.remote_job_id += 1;
    Ok(())
}
#[cfg(feature = "mcp")]
pub(super) fn remote_status(app: &MzViewerApp) -> serde_json::Value {
    let q = &app.quant;
    serde_json::json!({"job_id":format!("quantification:{}",q.remote_job_id),"running":q.rx.is_some(),"completed":q.completed,"total":q.total,"message":q.message,"method":signature(&q.method),"results":q.results.iter().enumerate().map(|(i,r)|serde_json::json!({"result_index":i,"sample":r.sample,"source":r.source,"run":r.run,"analyte":r.analyte,"parameters":r.params,"method":r.method,"automatic":r.automatic,"peak":r.peak,"automatic_status":r.automatic_status,"status":r.status,"diagnostic":r.diagnostic,"point_count":r.trace.len()})).collect::<Vec<_>>()})
}
#[cfg(feature = "mcp")]
pub(super) fn remote_cancel(app: &MzViewerApp) {
    app.quant.cancel.store(true, Ordering::Relaxed);
}
#[cfg(feature = "mcp")]
pub(super) fn remote_review(
    app: &mut MzViewerApp,
    index: usize,
    start: Option<f64>,
    end: Option<f64>,
    reviewed: bool,
) -> Result<(), String> {
    if reviewed {
        return Err("Human review must be recorded through the GUI".into());
    }
    if app.quant.rx.is_some() {
        return Err("Wait for batch completion".into());
    }
    let signature = signature(&app.quant.method);
    let r = app
        .quant
        .results
        .get_mut(index)
        .ok_or("Unknown result index")?;
    if r.method != signature {
        return Err("Method changed; rerun before reviewing".into());
    }
    match (start, end) {
        (Some(a), Some(z)) => {
            if !a.is_finite() || !z.is_finite() {
                return Err("Finite boundaries required".into());
            }
            r.peak = Some(measure(&r.trace, a, z)?);
            r.status = Status::Manual;
        }
        (None, None) => {}
        _ => return Err("Both boundaries are required".into()),
    }
    select(&mut app.quant, index);
    Ok(())
}

#[cfg(feature = "mcp")]
pub(super) fn remote_trace(
    app: &MzViewerApp,
    index: usize,
    offset: usize,
    limit: usize,
) -> Result<serde_json::Value, String> {
    let r = app.quant.results.get(index).ok_or("Unknown result index")?;
    Ok(
        serde_json::json!({"result_index":index,"parameters":r.params,"method":r.method,"total":r.trace.len(),"offset":offset,"points":r.trace.iter().skip(offset).take(limit).collect::<Vec<_>>(),"units":["minutes","arbitrary units"],"smoothing":0}),
    )
}
#[cfg(feature = "mcp")]
pub(super) fn remote_csv(app: &MzViewerApp) -> Result<String, String> {
    if app.quant.rx.is_some() {
        return Err("Wait for batch completion before exporting".into());
    }
    if app.quant.results.is_empty() {
        return Err("No batch results".into());
    }
    Ok(csv(&app.quant.results, &app.quant.method))
}
#[cfg(feature = "mcp")]
pub(super) fn remote_reset(app: &mut MzViewerApp, index: usize) -> Result<(), String> {
    if app.quant.rx.is_some() {
        return Err("Wait for batch completion".into());
    }
    let r = app
        .quant
        .results
        .get_mut(index)
        .ok_or("Unknown result index")?;
    r.peak = r.automatic.clone();
    r.status = r.automatic_status.clone();
    select(&mut app.quant, index);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn method() -> Method {
        let mut m = Method {
            detection_smoothing: 0,
            ..Default::default()
        };
        m.analytes[0].expected_rt = 1.5;
        m.analytes[0].rt_window = [0.0, 4.0];
        m
    }
    fn trace() -> Vec<[f64; 2]> {
        vec![
            [0.0, 0.0],
            [0.5, 0.0],
            [1.0, 5.0],
            [1.5, 10.0],
            [2.2, 2.0],
            [3.0, 0.0],
            [4.0, 0.0],
        ]
    }
    fn result() -> Measurement {
        let m = method();
        let t = trace();
        let (peak, status) = detect(&t, &m.analytes[0], &m);
        Measurement {
            sample: "sample, \"A\"".into(),
            source: "source.mzML".into(),
            run: "source.mzML".into(),
            analyte: m.analytes[0].extraction.name.clone(),
            params: validate(&m).unwrap()[0].clone(),
            method: signature(&m),
            trace: t,
            automatic: peak.clone(),
            peak,
            automatic_status: status.clone(),
            status,
            diagnostic: String::new(),
        }
    }
    #[test]
    fn irregular_scan_spacing_integrates_the_raw_peak() {
        let m = method();
        let (peak, status) = detect(&trace(), &m.analytes[0], &m);
        let p = peak.unwrap();
        assert_eq!(status, Status::Automatic);
        assert_eq!((p.start, p.end, p.apex_rt), (0.5, 3.0, 1.5));
        assert!((p.area - 10.0).abs() < 1e-10);
        let manual = measure(&trace(), 0.75, 2.6).unwrap();
        assert!(manual.area < p.area);
        assert!(measure(&trace(), -1.0, 2.0).is_err());
        assert!(measure(&trace(), 2.0, 1.0).is_err());
    }
    #[test]
    fn missing_noise_multiple_candidates_and_clipped_peaks_are_flagged() {
        let mut m = method();
        assert_eq!(
            detect(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]], &m.analytes[0], &m).1,
            Status::Missing
        );
        m.minimum_height = 5.0;
        assert_eq!(
            detect(&[[0.0, 0.0], [1.0, 1.0], [2.0, 0.0]], &m.analytes[0], &m).1,
            Status::Missing
        );
        m.minimum_height = 0.0;
        let t = vec![
            [0.0, 0.0],
            [0.5, 2.0],
            [1.0, 0.0],
            [1.5, 10.0],
            [2.0, 0.0],
            [3.0, 0.0],
        ];
        let (p, status) = detect(&t, &m.analytes[0], &m);
        assert_eq!(status, Status::Ambiguous);
        assert_eq!(p.unwrap().apex_rt, 1.5);
        let t = vec![[0.0, 5.0], [1.0, 10.0], [2.0, 1.0], [3.0, 1.0]];
        assert_eq!(detect(&t, &m.analytes[0], &m).1, Status::Ambiguous);
    }
    #[test]
    fn local_prominence_rejects_ripples_on_an_elevated_baseline() {
        let mut m = method();
        m.minimum_height = 2.0;
        let t = vec![
            [0.0, 0.0],
            [0.5, 100.0],
            [1.0, 101.0],
            [1.5, 100.0],
            [2.0, 110.0],
            [3.0, 100.0],
            [4.0, 0.0],
        ];
        let (p, status) = detect(&t, &m.analytes[0], &m);
        assert_eq!(p.unwrap().apex_rt, 2.0);
        assert_eq!(status, Status::Automatic);
    }
    #[test]
    fn dragging_boundaries_and_intervals_changes_only_the_selected_result() {
        let ctx = egui::Context::default();
        let mut q = QuantState::default();
        q.method = method();
        q.results = vec![result(), result()];
        select(&mut q, 0);
        let frame = |q: &mut QuantState, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 700.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| review(q, ui));
                },
            );
            ctx.data(|d| {
                d.get_temp::<(egui::Rect, egui_plot::PlotTransform)>(egui::Id::new(
                    "quant_test_plot",
                ))
                .unwrap()
            })
        };
        frame(&mut q, vec![]);
        let (_, transform) = frame(&mut q, vec![]);
        let origin = transform.position_from_point(&egui_plot::PlotPoint::new(q.start, 5.0));
        let target = transform.position_from_point(&egui_plot::PlotPoint::new(0.85, 5.0));
        let button = |pos, button, pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut q,
            vec![
                egui::Event::PointerMoved(origin),
                button(origin, egui::PointerButton::Primary, true),
            ],
        );
        frame(&mut q, vec![egui::Event::PointerMoved(target)]);
        frame(
            &mut q,
            vec![button(target, egui::PointerButton::Primary, false)],
        );
        assert_eq!(q.results[0].status, Status::Manual);
        assert!(
            (q.results[0].peak.as_ref().unwrap().start - 0.85).abs() < 0.02,
            "actual start {}",
            q.results[0].peak.as_ref().unwrap().start
        );
        assert_eq!(q.results[1].status, Status::Automatic);
        let (_, transform) = frame(&mut q, vec![]);
        let start = transform.position_from_point(&egui_plot::PlotPoint::new(0.75, 5.0));
        let end = transform.position_from_point(&egui_plot::PlotPoint::new(2.6, 5.0));
        frame(
            &mut q,
            vec![
                egui::Event::PointerMoved(start),
                button(start, egui::PointerButton::Secondary, true),
            ],
        );
        frame(&mut q, vec![egui::Event::PointerMoved(end)]);
        frame(
            &mut q,
            vec![button(end, egui::PointerButton::Secondary, false)],
        );
        let p = q.results[0].peak.as_ref().unwrap();
        assert!((p.start - 0.75).abs() < 0.02, "actual start {}", p.start);
        assert!((p.end - 2.6).abs() < 0.02);
        assert!(p.area < q.results[0].automatic.as_ref().unwrap().area);
    }
    #[test]
    fn smoothing_changes_detection_but_not_the_area_source() {
        let mut m = method();
        m.detection_smoothing = 1;
        let t = trace();
        let (p, _) = detect(&t, &m.analytes[0], &m);
        let p = p.unwrap();
        assert_eq!(
            p.area,
            processing::integrate_peak(&t, p.start, p.end).unwrap()
        );
    }
    #[test]
    fn method_roundtrip_validates_rt_windows_and_allows_isomer_extractions() {
        let mut m = method();
        let mut a = m.analytes[0].clone();
        a.extraction.name = "Isomer".into();
        a.expected_rt = 3.0;
        m.analytes.push(a);
        assert_eq!(validate(&m).unwrap().len(), 2);
        let restored: Method = toml::from_str(&signature(&m)).unwrap();
        assert_eq!(signature(&m), signature(&restored));
        m.analytes[1].expected_rt = 10.0;
        assert!(validate(&m).is_err());
        m.analytes[1].expected_rt = 3.0;
        m.analytes[1].extraction.name = m.analytes[0].extraction.name.clone();
        assert!(validate(&m).is_err());
        m.analytes.remove(1);
        m.analytes[0].extraction.acquisition = processing::AcquisitionMode::MRM;
        assert!(validate(&m).is_err());
        m.analytes[0].extraction.precursor_mz = Some(483.0);
        assert!(validate(&m).is_ok());
    }
    #[test]
    fn batch_roundtrip_retains_manual_corrections_and_rejects_corrupt_data() {
        let mut r = result();
        r.peak = Some(measure(&r.trace, 0.75, 2.6).unwrap());
        r.status = Status::Manual;
        let batch = Batch {
            version: 1,
            method: method(),
            results: vec![r],
            samples: vec![("source.mzML".into(), "Sample A".into())],
        };
        let bytes = serde_json::to_vec(&batch).unwrap();
        let mut restored: Batch = serde_json::from_slice(&bytes).unwrap();
        validate_batch(&restored).unwrap();
        assert_eq!(restored.results[0].status, Status::Manual);
        assert_ne!(restored.results[0].peak, restored.results[0].automatic);
        restored.results[0].peak.as_mut().unwrap().area += 1.0;
        assert!(validate_batch(&restored).is_err());
        let mut restored: Batch = serde_json::from_slice(&bytes).unwrap();
        restored.results[0].trace[2][0] = 0.0;
        assert!(validate_batch(&restored).is_err());
    }
    #[test]
    fn csv_quotes_names_keeps_missing_areas_blank_and_marks_stale_methods() {
        let mut missing = result();
        missing.peak = None;
        missing.automatic = None;
        missing.status = Status::Missing;
        missing.automatic_status = Status::Missing;
        let mut m = method();
        m.analytes[0].extraction.ppm = 20.0;
        let output = csv(&[missing], &m);
        assert!(output.contains("\"sample, \"\"A\"\"\""));
        assert!(output.contains("\"Missing\",\"true\",\"\",\"\",\"\",\"\",\"\""));
        assert!(output.contains("\"linear\",\"unsmoothed\""));
    }
    fn sample(app: &mut MzViewerApp, id: usize) {
        let path = PathBuf::from("test_file/data_dependent_02.mzML");
        let mut data = crate::parser::MzData::new();
        data.open_msfile(&path).unwrap();
        app.files.insert(
            id,
            super::super::state::OpenFile {
                id,
                name: format!("Sample {id}"),
                path: path.to_string_lossy().into_owned(),
                data,
                display: super::super::state::FileDisplaySettings {
                    color: crate::plotting_parameters::LineColor::Blue,
                    visible: true,
                },
                cache: Default::default(),
                is_loading: false,
            },
        );
        app.quant.selected_samples.insert(id);
    }
    fn finish(app: &mut MzViewerApp) {
        let ctx = egui::Context::default();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while app.quant.rx.is_some() {
            assert!(std::time::Instant::now() < until, "batch worker timeout");
            poll(app, &ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
    #[test]
    fn batch_extracts_all_samples_preserves_corrections_and_invalidates_method_edits() {
        let mut app = MzViewerApp::default();
        sample(&mut app, 1);
        sample(&mut app, 2);
        app.quant.method = method();
        app.quant.method.analytes[0].extraction.mass = Some(722.43);
        app.quant.method.analytes[0].extraction.ppm = 1000.0;
        run(&mut app).unwrap();
        finish(&mut app);
        assert_eq!(app.quant.results.len(), 2);
        assert!(app.quant.results.iter().all(|r| !r.trace.is_empty()));
        let r = &mut app.quant.results[0];
        let start = r.trace[0][0];
        let end = r.trace.last().unwrap()[0];
        r.peak = Some(measure(&r.trace, start, end).unwrap());
        r.status = Status::Manual;
        let corrected = r.peak.clone();
        run(&mut app).unwrap();
        assert_eq!(app.quant.total, 1);
        finish(&mut app);
        assert_eq!(app.quant.results[0].peak, corrected);
        assert_eq!(app.quant.results[0].status, Status::Manual);
        app.quant.method.analytes[0].extraction.ppm = 900.0;
        run(&mut app).unwrap();
        assert_eq!(app.quant.total, 2);
        finish(&mut app);
        assert_ne!(app.quant.results[0].status, Status::Manual);
        assert!(app
            .quant
            .results
            .iter()
            .all(|r| r.method == signature(&app.quant.method)));
    }
    #[test]
    fn empty_or_cancelled_batch_keeps_results_and_does_not_touch_viewer() {
        let mut app = MzViewerApp::default();
        app.quant.results.push(result());
        let mut pending = result();
        pending.sample = "Waiting sample".into();
        pending.peak = None;
        pending.automatic = None;
        pending.status = Status::Pending;
        pending.automatic_status = Status::Pending;
        pending.trace.clear();
        app.quant.results.push(pending);
        assert!(run(&mut app).is_err());
        let (tx, rx) = mpsc::channel();
        app.quant.rx = Some(rx);
        tx.send(Event::Done(true)).unwrap();
        poll(&mut app, &egui::Context::default());
        assert_eq!(app.quant.results.len(), 2);
        assert_eq!(app.quant.results[1].status, Status::Cancelled);
        assert!(app.quant.message.contains("cancelled"));
        assert!(app.workspace.traces.is_empty());
        assert!(app.integration.result.is_none());
    }
    #[test]
    fn quant_workspace_renders_results_and_review_at_small_and_large_sizes() {
        for (size, dark, editor) in [
            (egui::vec2(800.0, 600.0), false, false),
            (egui::vec2(1440.0, 900.0), false, false),
            (egui::vec2(1440.0, 900.0), true, true),
        ] {
            let mut app = MzViewerApp::default();
            app.quant.active = true;
            app.quant.method = method();
            app.quant.results.push(result());
            sample(&mut app, 1);
            sample(&mut app, 2);
            app.quant.method_open = editor;
            select(&mut app.quant, 0);
            let ctx = egui::Context::default();
            super::super::workbench::configure(&ctx, dark);
            let mut outputs = vec![];
            for frame in 0..4 {
                let output = ctx.run(
                    egui::RawInput {
                        time: Some(frame as f64 * 0.2),
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ctx| {
                        super::super::panels::update_data_selection_panel(&mut app, ctx);
                        show(&mut app, ctx);
                    },
                );
                assert!(!output.shapes.is_empty());
                assert_eq!(app.quant.results.len(), 1);
                outputs.push(output);
            }
            if std::env::var_os("CHROMASCOPE_QUANT_PREVIEW").is_some() {
                super::super::test_render::save(
                    &ctx,
                    outputs,
                    &PathBuf::from(format!("target/quant-preview-{}-{dark}.png", size.x)),
                    size,
                );
            }
        }
    }
    #[test]
    fn shipped_method_example_is_valid_and_roundtrips() {
        let m: Method =
            toml::from_str(include_str!("../../presets/quantification-example.toml")).unwrap();
        assert_eq!(validate(&m).unwrap().len(), 2);
    }
    #[test]
    fn replacing_an_edited_method_requires_a_decision_and_keeps_the_current_method() {
        let mut q = QuantState::default();
        q.method.name = "Edited assay".into();
        request_method(&mut q, MethodAction::New);
        assert!(q.pending_method.is_some());
        assert_eq!(q.method.name, "Edited assay");
        // Cancelling the pending action keeps the edited method intact.
        q.pending_method = None;
        assert_eq!(q.method.name, "Edited assay");
        request_method(&mut q, MethodAction::New);
        let action = q.pending_method.take().unwrap();
        change_method(&mut q, action);
        assert_eq!(q.method.name, Method::default().name);
    }
}
