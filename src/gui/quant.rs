//! Batch measurements are independent of exploratory viewer traces and measurements.
use super::theme::Tokens;
use super::MzViewerApp;
use crate::processing::{self, ProcessingResult};
use eframe::egui::{self, Color32};
use egui_plot::{Line, PlotPoints, Polygon, VLine};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
};

use super::integration_editor;
use crate::confidence::Tier;
use crate::integration::{self, Integration};
use crate::quant::{
    detect_assessed, measure, measure_for, new_analyte, refresh_consistency, validate, Measurement,
    Method, Status,
};
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
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) struct QuantState {
    targeted: super::targeted::UiState,
    /// Integration settings being tuned in the review card: (analyte, applied snapshot, draft).
    #[serde(skip)]
    draft: Option<(String, Integration, Integration)>,
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
    #[serde(skip)]
    rx: Option<mpsc::Receiver<Event>>,
    #[serde(skip)]
    cancel: Arc<AtomicBool>,
    total: usize,
    completed: usize,
    message: String,
    start: f64,
    end: f64,
    drag_boundary: Option<bool>,
    confirm_replace: bool,
    method_open: bool,
    /// Method being edited in the editor window; only OK moves it into `method`.
    #[serde(skip)]
    method_draft: Option<(u32, Method)>,
    /// Bumped whenever `method` is replaced wholesale (New/Open), invalidating the draft.
    #[serde(skip)]
    method_epoch: u32,
    batch_samples: Vec<(String, String)>,
    restore_samples: Option<Vec<(String, String)>>,
    pending_sources: Vec<PathBuf>,
    #[serde(skip)]
    pending_method: Option<MethodAction>,
}
impl QuantState {
    /// Opens the batch method editor (used by the workflow map).
    pub(super) fn open_method(&mut self) {
        self.method_open = true;
    }
}
impl Default for QuantState {
    fn default() -> Self {
        Self {
            targeted: super::targeted::UiState::default(),
            draft: None,
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
            method_draft: None,
            method_epoch: 0,
            batch_samples: vec![],
            restore_samples: None,
            pending_sources: vec![],
            pending_method: None,
        }
    }
}
impl QuantState {
    pub(super) fn busy(&self) -> bool {
        self.rx.is_some() || self.targeted.busy()
    }
    pub(super) fn latest_targeted(&self) -> Option<&crate::targeted::BatchResult> {
        self.targeted.latest()
    }
    /// Peak integrations that still need scientist review: missing, ambiguous,
    /// failed, cancelled, pending, or measured under a changed method.
    pub(super) fn unresolved_count(&self) -> usize {
        let fresh = fresh(&self.method);
        self.results
            .iter()
            .filter(|r| fresh.stale(r) || r.needs_review())
            .count()
    }
    pub(super) fn has_results(&self) -> bool {
        !self.results.is_empty()
    }
    pub(super) fn method_is_valid(&self) -> bool {
        validate(&self.method).is_ok()
    }
    pub(super) fn method_error(&self) -> String {
        validate(&self.method)
            .err()
            .unwrap_or_else(|| "The method is incomplete.".into())
    }
    pub(super) fn method_summary(&self) -> String {
        format!(
            "{} · {} analytes",
            self.method.name,
            self.method.analytes.len()
        )
    }
    pub(super) fn has_targeted_batch(&self) -> bool {
        self.targeted.batch_count() > 0
    }
    pub(super) fn calibration_summary(&self) -> (usize, usize, usize) {
        self.targeted.calibration_summary()
    }
    pub(super) fn invalid_result_count(&self) -> usize {
        self.targeted.invalid_result_count()
    }
    pub(super) fn review_progress(&self) -> (usize, usize) {
        (
            self.targeted.reviewed_result_count(),
            self.targeted.total_result_count(),
        )
    }
    pub(super) fn qc_report_count(&self) -> usize {
        self.targeted.qc_report_count()
    }
    pub(super) fn qc_status(&self) -> Option<crate::qc::Status> {
        self.targeted.qc_latest_status()
    }
    pub(super) fn qc_pending_reviews(&self) -> usize {
        self.targeted.qc_pending_reviews()
    }
    pub(super) fn verify_snapshot(value: serde_json::Value) -> Result<(), String> {
        let state: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        if !state.results.is_empty() {
            validate_batch(&Batch {
                version: 1,
                method: state.method.clone(),
                results: state.results.clone(),
                samples: state.batch_samples.clone(),
            })?;
        }
        Ok(())
    }
}
impl QuantState {
    pub(super) fn retain_targeted(
        &mut self,
        batch: crate::targeted::BatchResult,
    ) -> Result<(), String> {
        self.targeted.retain(batch)
    }
    pub(super) fn retain_qc(&mut self, report: crate::qc::Report) -> Result<(), String> {
        self.targeted.retain_qc(report)
    }
    pub(super) fn suggestion(&self) -> Option<crate::domain::Operation> {
        self.targeted.suggestion()
    }
    pub(super) fn retain_chromatography(
        &mut self,
        analysis: crate::chromatography::Analysis,
    ) -> Result<(), String> {
        crate::chromatography::verify(&analysis).map_err(|e| e.to_string())?;
        let result = self
            .results
            .iter_mut()
            .find(|r| {
                crate::chromatography::Trace::from_points(
                    format!("{} / {}", r.sample, r.analyte),
                    &r.trace,
                ) == analysis.raw
            })
            .ok_or("Review trace is absent from batch results")?;
        result.chromatography = Some(analysis);
        Ok(())
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
/// The method as it applies to one analyte: global settings plus that analyte only. Stored in
/// each result, so editing one analyte's settings leaves the other analytes' results current.
fn analyte_signature(method: &Method, analyte: &str) -> String {
    let mut m = method.clone();
    m.analytes.retain(|a| a.extraction.name == analyte);
    signature(&m)
}
/// Per-analyte snapshots of the current method, for cheap staleness checks.
struct Fresh(std::collections::HashMap<String, String>);
fn fresh(method: &Method) -> Fresh {
    Fresh(
        method
            .analytes
            .iter()
            .map(|a| {
                (
                    a.extraction.name.clone(),
                    analyte_signature(method, &a.extraction.name),
                )
            })
            .collect(),
    )
}
impl Fresh {
    /// True when the result was measured under different settings than the current method.
    /// Batches saved before per-analyte snapshots stored the whole method; those are compared
    /// through their analyte entry.
    fn stale(&self, r: &Measurement) -> bool {
        let Some(current) = self.0.get(&r.analyte) else {
            return true;
        };
        if &r.method == current {
            return false;
        }
        toml::from_str::<Method>(&r.method)
            .map(|m| analyte_signature(&m, &r.analyte) != *current)
            .unwrap_or(true)
    }
}
fn select(q: &mut QuantState, i: usize) {
    q.draft = None;
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
    let fresh = fresh(&method);
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
            if q.results.iter().any(|r| {
                r.source == source
                    && r.sample == sample
                    && r.analyte == a.extraction.name
                    && r.chromatography.is_some()
                    && fresh.stale(r)
            }) {
                return Err("Saved advanced results use a different method. Save this batch and create a new batch to retain its history.".into());
            }
            // Retain corrections only for exactly the same source, run and method.
            if q.results.iter().any(|r| {
                r.source == source
                    && r.sample == sample
                    && r.analyte == a.extraction.name
                    && !fresh.stale(r)
                    && (matches!(r.status, Status::Manual | Status::Reviewed)
                        || r.chromatography.is_some())
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
            chromatography: None,
            quality: None,
            sample: sample.clone(),
            source: source.clone(),
            run: path.clone(),
            analyte: a.extraction.name.clone(),
            params: params.clone(),
            method: analyte_signature(&method, &a.extraction.name),
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
                chromatography: None,
                quality: None,
                sample,
                source,
                run: path.clone(),
                analyte: a.extraction.name.clone(),
                params: params.clone(),
                method: analyte_signature(&method, &a.extraction.name),
                trace: vec![],
                automatic: None,
                peak: None,
                status: Status::Failed,
                automatic_status: Status::Failed,
                diagnostic: String::new(),
            };
            match processing::run_in_background(path.into(), params, 0) {
                ProcessingResult::Success { plot_data, .. } => {
                    let (peak, status, quality) = detect_assessed(&plot_data, &a, &method);
                    result.trace = plot_data;
                    result.quality = quality;
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
    super::targeted::poll(&mut app.quant.targeted, ctx);
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
                refresh_consistency(&mut q.results);
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
/// Snake-case name of a unit enum variant, as written in method files.
fn serde_name<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}
fn csv(results: &[Measurement], method: &Method) -> String {
    fn cell(s: &str) -> String {
        format!("\"{}\"", s.replace('"', "\"\""))
    }
    let mut out = "sample,source,run,analyte,method_name,status,needs_recalculation,review_state,confidence_score,confidence_tier,confidence_flags,area_intensity_min,apex_rt_min,start_rt_min,end_rt_min,height,acquisition,ms_level,polarity,target_mz,ppm,mda,precursor_mz,mz_range,baseline,area_source,algorithm,diagnostic,method_toml\n".to_owned();
    let fresh = fresh(method);
    for r in results {
        let p = r.peak.as_ref();
        let parsed = toml::from_str::<Method>(&r.method).ok();
        let method_name = parsed.as_ref().map(|m| m.name.clone()).unwrap_or_default();
        let cfg = parsed
            .as_ref()
            .and_then(|m| {
                m.analytes
                    .iter()
                    .find(|a| a.extraction.name == r.analyte)
                    .map(|a| a.integration_for(m))
            })
            .unwrap_or_default();
        let fields = vec![
            r.sample.clone(),
            r.source.clone(),
            r.run.clone(),
            r.analyte.clone(),
            method_name,
            format!("{:?}", r.status),
            fresh.stale(r).to_string(),
            r.review().label().to_owned(),
            r.quality
                .as_ref()
                .map(|q| q.score.to_string())
                .unwrap_or_default(),
            r.tier().map(|t| t.label().to_owned()).unwrap_or_default(),
            r.quality
                .as_ref()
                .map(|q| q.flags().join("; "))
                .unwrap_or_default(),
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
                .filter(|x| x.tolerance().unit == crate::validation::ToleranceUnit::Ppm)
                .map(|x| x.tolerance().value.to_string())
                .unwrap_or_default(),
            r.params
                .xic_params
                .as_ref()
                .filter(|x| x.tolerance().unit == crate::validation::ToleranceUnit::Mda)
                .map(|x| x.tolerance().value.to_string())
                .unwrap_or_default(),
            r.params
                .precursor_mz
                .map(|m| m.to_string())
                .unwrap_or_default(),
            format!("{:?}", r.params.mz_range),
            serde_name(&cfg.baseline),
            "unsmoothed".into(),
            serde_name(&cfg.algorithm),
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
        crate::atomic_write::write_atomic(&path, &text).map_err(|e| e.to_string())?;
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
/// Moves an edited method into the batch (in memory only; the file is untouched) and says how
/// many existing results now come from an earlier method.
fn commit_method(q: &mut QuantState, draft: &Method) {
    if signature(draft) == signature(&q.method) {
        return;
    }
    q.method = draft.clone();
    let fresh = fresh(&q.method);
    let stale = q.results.iter().filter(|r| fresh.stale(r)).count();
    q.message = if stale > 0 {
        format!(
            "Method updated. {stale} result(s) were measured with the previous method: choose Reprocess to update them. The method file is unchanged until you save it."
        )
    } else {
        "Method updated for this batch. The method file is unchanged until you save it.".into()
    };
}
fn change_method(q: &mut QuantState, action: MethodAction) {
    q.method_epoch = q.method_epoch.wrapping_add(1);
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
            crate::atomic_write::write_atomic(
                path,
                serde_json::to_vec(&batch).map_err(|e| e.to_string())?,
            )
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
        if let Some(a) = &r.chromatography {
            crate::chromatography::verify(a).map_err(|e| e.to_string())?;
            if a.raw
                != crate::chromatography::Trace::from_points(
                    format!("{} / {}", r.sample, r.analyte),
                    &r.trace,
                )
            {
                return Err("Advanced analysis differs from stored trace".into());
            }
        }
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
                .any(|p| !p[0].is_finite() || !p[1].is_finite() || p[0] < 0.0)
            || r.trace.windows(2).any(|w| w[0][0] >= w[1][0])
        {
            return Err("Invalid or duplicate result data in batch.".into());
        }
        for p in [&r.peak, &r.automatic].into_iter().flatten() {
            let actual = measure_for(&r.trace, p.start, p.end, &method.analytes[i], &method)?;
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
        .frame(super::theme::side_frame(ctx))
        .default_width(225.0)
        .resizable(true)
        .show(ctx, |ui| {
            crate::gui::widgets::section_title(ui, "Batch samples");
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
        ui.add_space(4.0);
        let current = fresh(&q.method);
        let unresolved = q
            .results
            .iter()
            .filter(|r| current.stale(r) || r.needs_review())
            .count();
        let stage_id = egui::Id::new("quantification_stage");
        let mut stage = ui.ctx().data(|d| d.get_temp::<usize>(stage_id)).unwrap_or(0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(12.0, 5.0);
            let first = if unresolved > 0 {
                format!("1 · Areas and peak review ({unresolved} need review)")
            } else {
                "1 · Areas and peak review".to_string()
            };
            for (index, label) in [
                (0, first.as_str()),
                (1, "2 · Calibration and concentrations"),
                (2, "3 · Batch QC and validation"),
            ] {
                if super::widgets::nav_tab(ui, stage == index, label).clicked() {
                    stage = index;
                }
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(stage_id, stage));
        ui.add_space(6.0);
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let selected = app
                    .files
                    .iter()
                    .filter(|(id, _)| q.selected_samples.contains(id))
                    .collect::<Vec<_>>();
                let importing = selected.iter().any(|(_, f)| f.is_loading);
                let batch_ready = if selected.is_empty() {
                    Some("Select at least one loaded sample on the left to enable Run batch.")
                } else if importing {
                    Some("Waiting for selected samples to finish importing…")
                } else if validate(&q.method).is_err() {
                    Some("Define a valid extraction method (Edit method…) before running.")
                } else {
                    None
                };
                let ready = batch_ready.is_none();
                let mut start_run = false;
                ui.horizontal_wrapped(|ui| {
                    ui.add_enabled_ui(q.rx.is_none(), |ui| {
                        if super::widgets::primary_button_enabled(ui, ready, "Run batch")
                            .on_hover_text(
                                "Select loaded samples and define a valid method to run the batch",
                            )
                            .clicked()
                        {
                            start_run = true;
                        }
                    });
                    if q.rx.is_some() {
                        ui.spinner();
                        ui.label(format!("{} / {}", q.completed, q.total));
                        if ui.button("Cancel").clicked() {
                            q.cancel.store(true, Ordering::Relaxed);
                            q.message = "Stopping after the current extraction…".into();
                        }
                    }
                    ui.separator();
                    ui.label(egui::RichText::new(format!("Method: {}", q.method.name)).strong());
                    ui.weak(format!("{} {}", q.method.analytes.len(), if q.method.analytes.len() == 1 { "analyte" } else { "analytes" }));
                    if signature(&q.method) != q.saved_method {
                        ui.colored_label(ui.visuals().warn_fg_color, "● unsaved")
                            .on_hover_text("Method has unsaved changes. Save it in the method editor for future batches.");
                    }
                    if ui.button("Edit method…").clicked() {
                        q.method_open = true;
                    }
                    let outdated = {
                        let fresh = fresh(&q.method);
                        q.results.iter().filter(|r| fresh.stale(r)).count()
                    };
                    if outdated > 0 && q.rx.is_none() {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!("● {outdated} result(s) use an earlier method"),
                        )
                        .on_hover_text("These peaks were integrated with a method that has since changed.");
                        if ui
                            .add_enabled(ready, egui::Button::new("Reprocess"))
                            .on_hover_text("Re-extract and re-integrate outdated results with the current method; manual corrections made under the old method are replaced")
                            .clicked()
                        {
                            start_run = true;
                        }
                    }
                    ui.separator();
                    ui.add_enabled_ui(q.rx.is_none(), |ui| {
                        ui.menu_button("Batch session ▾", |ui| {
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
                            match crate::atomic_write::write_atomic(path, csv(&q.results, &q.method)) {
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
                if start_run {
                    app.quant = std::mem::take(&mut q);
                    let result = run(app);
                    q = std::mem::take(&mut app.quant);
                    if let Err(e) = result {
                        q.message = e;
                    }
                }
                if let Some(reason) = batch_ready {
                    if q.rx.is_none() {
                        ui.weak(reason);
                    }
                }
                if !q.message.is_empty() {
                    ui.label(&q.message);
                }
            });
        ui.add_space(6.0);
        ui.separator();
        if stage == 0 {
            // Master/detail layout manages its own scrolling so the plot can use the full height.
            review_stage(&mut q, ui);
        } else if stage == 1 {
            // The calibration tab lays out and scrolls its own panes (results browser).
            let samples = if q.batch_samples.is_empty() {
                let mut samples: Vec<_> = app
                    .files
                    .values()
                    .filter(|f| !f.is_loading && q.selected_samples.contains(&f.id))
                    .map(|f| (f.path.clone(), f.name.clone()))
                    .collect();
                samples.sort();
                samples
            } else {
                q.batch_samples.clone()
            };
            super::targeted::panel(&mut q.targeted, &q.method, &samples, &q.results, ui);
            if let Some((source, analyte)) = q.targeted.take_revisit() {
                revisit(&mut q, ui.ctx(), &source, &analyte);
            }
        } else {
            egui::ScrollArea::vertical()
                .id_salt("quantification_content")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    q.targeted.qc_panel(ui);
                    super::workbench::scroll_new_focus(ui);
                });
        }
    });
    if q.method_open {
        let mut open = true;
        let (epoch, mut draft) = match q.method_draft.take() {
            Some((e, d)) if e == q.method_epoch => (e, d),
            _ => (q.method_epoch, q.method.clone()),
        };
        let mut decision = None;
        egui::Window::new("Batch quantification method")
            .max_height((ctx.content_rect().height() - 60.0).max(120.0))
            .vscroll(true)
            .open(&mut open)
            .default_size(egui::vec2(1100.0, 440.0))
            .resizable(true)
            .show(ctx, |ui| {
                ui.add_enabled_ui(q.rx.is_none() && q.pending_method.is_none(), |ui| {
                    method_editor(&mut q, &mut draft, &mut decision, ui);
                    super::workbench::scroll_new_focus(ui);
                });
            });
        if !open {
            decision = Some(false);
        }
        match decision {
            Some(true) => {
                commit_method(&mut q, &draft);
                q.method_open = false;
            }
            Some(false) => q.method_open = false,
            None => q.method_draft = Some((epoch, draft)),
        }
    }
    if q.confirm_replace {
        egui::Window::new("Open another batch?").max_height((ctx.content_rect().height()-60.0).max(120.0)).vscroll(true).collapsible(false).show(ctx, |ui| {
            ui.label("Opening a batch replaces the current method and results. Save this batch first to keep your corrections.");
            if ui.button("Save current batch…").clicked() { if let Err(e)=batch_file(&mut q,true) { q.message=e; } }
            if ui.button("Choose batch to open…").clicked() { q.confirm_replace=false; if let Err(e)=batch_file(&mut q,false) { q.message=e; } }
            if ui.button("Cancel").clicked() { q.confirm_replace=false; }
        });
    }
    if let Some(action) = q.pending_method {
        egui::Window::new("Unsaved method changes")
            .max_height((ctx.content_rect().height() - 60.0).max(120.0))
            .vscroll(true)
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

fn method_editor(
    q: &mut QuantState,
    draft: &mut Method,
    decision: &mut Option<bool>,
    ui: &mut egui::Ui,
) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            if ui.button("New").clicked() {commit_method(q,draft);request_method(q,MethodAction::New);}
            if ui.button("Open…").clicked() {commit_method(q,draft);request_method(q,MethodAction::Open);}
            if ui.button("Save").clicked() { save_draft(q,draft,false); }
            if ui.button("Save As…").clicked() { save_draft(q,draft,true); }
            if let Some(result)=super::examples::menu(ui) { q.message=match result {
                Ok(path)=>format!("Example saved to {}. Choose Open method for the quantification example; open extraction examples in the viewer preset editor.",path.display()),
                Err(e)=>e,
            }; }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let size = egui::vec2(88.0, 28.0);
                if ui
                    .add_enabled(
                        validate(draft).is_ok(),
                        egui::Button::new(egui::RichText::new("OK").strong())
                            .fill(ui.visuals().selection.bg_fill)
                            .min_size(size),
                    )
                    .on_hover_text("Use these settings for this batch (does not write the method file)")
                    .clicked()
                {
                    *decision = Some(true);
                }
                if ui
                    .add(egui::Button::new("Cancel").min_size(size))
                    .on_hover_text("Discard all edits made in this window")
                    .clicked()
                {
                    *decision = Some(false);
                }
            });
        });
        if signature(draft) != signature(&q.method) {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "● Edited. OK uses these settings for this batch; results from the previous method will then need reprocessing.",
            );
        }
        if signature(draft) != q.saved_method {
            ui.weak("Not saved to the method file. Use Save to keep these settings.");
        }
        ui.horizontal_wrapped(|ui| {ui.label("Method name");ui.add(egui::TextEdit::singleline(&mut draft.name).desired_width(280.0));});
        if let Some(path)=&q.method_path { ui.small(path.display().to_string()); }
        ui.horizontal_wrapped(|ui| {
            ui.label("Detection smoothing"); ui.add(egui::DragValue::new(&mut draft.detection_smoothing).range(0..=10));
            ui.label("Minimum peak prominence"); ui.add(egui::DragValue::new(&mut draft.minimum_height).range(0.0..=f64::MAX));
            ui.label("Boundary fraction"); ui.add(egui::DragValue::new(&mut draft.boundary_fraction).speed(0.01).range(0.0..=0.5));
        });
        ui.small("Defaults for every analyte: detection uses a smoothed trace; areas use unsmoothed data above the chosen baseline (straight line unless set). Use the Integration column to choose an algorithm and baseline per analyte. RT is in minutes.");
        let mut remove=None;
        let defaults = method_default(&*draft);
        egui::ScrollArea::both().max_height(230.0).show(ui,|ui| {
            egui::Grid::new("quant_method_table").striped(true).show(ui,|ui| {
                for h in ["Analyte","Mode","Target m/z","Precursor m/z","Tolerance","Polarity","MS level","Expected RT","Window start","Window end","Integration",""] { crate::gui::widgets::strong(ui, h); } ui.end_row();
                for (i,a) in draft.analytes.iter_mut().enumerate() {
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
                        ui.horizontal(|ui| {
                            let mut tolerance = a.extraction.tolerance();
                            ui.add(egui::DragValue::new(&mut tolerance.value).range(0.0..=1000.0));
                            crate::gui::widgets::tolerance_unit_select(ui, (i, "tolerance_unit"), &mut tolerance.unit);
                            a.extraction.set_tolerance(tolerance);
                        });
                        egui::ComboBox::from_id_salt((i,"polarity")).selected_text(&a.extraction.polarity).show_ui(ui,|ui| { for p in ["positive","negative"] { ui.selectable_value(&mut a.extraction.polarity,p.into(),p); } });
                        let mut level=a.extraction.ms_level.unwrap_or(if a.extraction.acquisition==processing::AcquisitionMode::MRM {2} else {1});
                        if ui.add(egui::DragValue::new(&mut level).range(1..=10)).changed() { a.extraction.ms_level=Some(level); }
                        ui.add(egui::DragValue::new(&mut a.expected_rt).speed(0.01).range(0.0..=1e6));
                        ui.add(egui::DragValue::new(&mut a.rt_window[0]).speed(0.01).range(0.0..=1e6));
                        ui.add(egui::DragValue::new(&mut a.rt_window[1]).speed(0.01).range(0.0..=1e6));
                        ui.menu_button(format!("{} ▾", integration_editor::short_name(a.integration.as_ref())), |ui| {
                            ui.set_max_width(380.0);
                            crate::gui::widgets::strong(ui, format!("Integration: {}", a.extraction.name));
                            let mut custom = a.integration.is_some();
                            if ui.checkbox(&mut custom, "Customise for this analyte").on_hover_text("Off: classic detection with the method-level smoothing, prominence and boundary fraction").changed() {
                                a.integration = custom.then(|| defaults.clone());
                            }
                            if let Some(cfg) = &mut a.integration {
                                integration_editor::editor(ui, cfg, a.rt_window);
                                if let Err(e) = cfg.validate() { ui.colored_label(ui.visuals().warn_fg_color, e); }
                            } else {
                                ui.small("Using the method defaults above.");
                            }
                        });
                        if ui.small_button("Remove").clicked() { remove=Some(i); }
                        ui.end_row();
                }
            });
        });
        if let Some(i)=remove { draft.analytes.remove(i); }
        if ui.add_enabled(draft.analytes.len()<64,egui::Button::new("Add analyte")).clicked() { draft.analytes.push(new_analyte(draft.analytes.len()+1)); }
        if let Err(e)=validate(&*draft) { ui.colored_label(Tokens::of(ui).error,e); }
        if !q.message.is_empty() { ui.small(&q.message); }
    });
}
/// Saves the edited method to its file. The batch keeps using its current method unless the
/// save succeeds, in which case the saved method becomes the batch's method too.
fn save_draft(q: &mut QuantState, draft: &Method, save_as: bool) {
    let previous = std::mem::replace(&mut q.method, draft.clone());
    match save_method(q, save_as) {
        Ok(()) if q.saved_method == signature(draft) => {
            q.method = previous;
            commit_method(q, draft);
            q.message = format!("Method saved. {}", q.message);
        }
        Ok(()) => q.method = previous,
        Err(e) => {
            q.method = previous;
            q.message = e;
        }
    }
}
/// Confidence as `(label, colour)`: green High, amber Medium, orange Low; outcomes that have
/// no peak (missing, failed, running) show their own state.
fn confidence_tone(visuals: &egui::Visuals, r: &Measurement, stale: bool) -> (String, Color32) {
    if stale {
        return ("Recalculate".into(), visuals.warn_fg_color);
    }
    match (&r.status, &r.quality, r.tier()) {
        (Status::Missing | Status::Failed, ..) => {
            (format!("{:?}", r.status), visuals.error_fg_color)
        }
        (Status::Pending | Status::Cancelled, ..) => {
            (format!("{:?}", r.status), visuals.weak_text_color())
        }
        (_, quality, tier) => {
            let good = Tokens::for_mode(visuals.dark_mode).good;
            let color = match tier {
                Some(Tier::High) => good,
                Some(Tier::Medium) => visuals.warn_fg_color,
                Some(Tier::Low) => visuals.error_fg_color,
                None => visuals.weak_text_color(),
            };
            let label = match (quality, tier) {
                (Some(q), Some(t)) => format!("{} {}", q.score, t.label()),
                (None, Some(t)) => t.label().to_owned(),
                _ => "No score".into(),
            };
            (label, color)
        }
    }
}
/// Opens the peak of `analyte` in `source` in the peak review (tab 1).
fn revisit(q: &mut QuantState, ctx: &egui::Context, source: &str, analyte: &str) {
    if let Some(i) = q
        .results
        .iter()
        .position(|r| r.source == source && r.analyte == analyte)
    {
        select(q, i);
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("quantification_stage"), 0_usize));
        super::table::scroll_to_selected(ctx, "quant_result_matrix");
        ctx.request_repaint();
    }
}
/// Medium-confidence automatic peaks nobody has looked at yet: the second pass once nothing is
/// flagged outright.
fn worth_a_look(r: &Measurement) -> bool {
    r.review() == crate::quant::Review::NotReviewed
        && r.peak.is_some()
        && r.tier() == Some(Tier::Medium)
}
/// Selects the next result after the current one that needs review (flagged, stale or failed);
/// when none does, the next unreviewed medium-confidence one. Returns whether it moved.
fn next_needing_review(q: &mut QuantState) -> bool {
    if q.results.is_empty() {
        return false;
    }
    let fresh = fresh(&q.method);
    let start = q.selected.map_or(0, |i| i + 1);
    let cycle = |pick: &dyn Fn(&Measurement) -> bool| {
        (0..q.results.len())
            .map(|offset| (start + offset) % q.results.len())
            .find(|&i| pick(&q.results[i]))
    };
    let found = cycle(&|r| r.needs_review() || fresh.stale(r)).or_else(|| cycle(&worth_a_look));
    match found {
        Some(i) => {
            select(q, i);
            true
        }
        None => false,
    }
}
fn empty_state(q: &mut QuantState, ui: &mut egui::Ui) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.heading("No batch results yet");
        ui.add_space(4.0);
        ui.label("Measure the peak area of every analyte in every selected sample.");
        ui.add_space(12.0);
        for step in [
            "1   Select samples in the panel on the left",
            "2   Define the analytes and their extraction windows",
            "3   Press Run batch, then review each peak here",
        ] {
            ui.label(step);
        }
        ui.add_space(12.0);
        if ui.button("Edit method…").clicked() {
            q.method_open = true;
        }
    });
}
/// Peak review: results table on the left, integration detail on the right.
fn review_stage(q: &mut QuantState, ui: &mut egui::Ui) {
    if q.results.is_empty() {
        empty_state(q, ui);
        return;
    }
    let fresh = fresh(&q.method);
    let unresolved = q
        .results
        .iter()
        .filter(|r| fresh.stale(r) || r.needs_review())
        .count();
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!(
                "{} {}",
                q.results.len(),
                if q.results.len() == 1 {
                    "result"
                } else {
                    "results"
                }
            ))
            .strong(),
        );
        if unresolved > 0 {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("● {unresolved} need review"),
            );
        } else {
            ui.colored_label(Tokens::of(ui).good, "● nothing needs review");
        }
    });
    ui.add_space(4.0);
    let size = ui.available_size();
    if size.x >= 900.0 {
        let left = (size.x * 0.42).clamp(380.0, 620.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(left, size.y),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_width(left);
                    results_table(q, ui, size.y - 64.0);
                },
            );
            ui.separator();
            let width = ui.available_width();
            ui.allocate_ui_with_layout(
                egui::vec2(width, size.y),
                egui::Layout::top_down(egui::Align::Min),
                |ui| review(q, ui),
            );
        });
    } else {
        egui::ScrollArea::vertical()
            .id_salt("quant_review_narrow")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                results_table(q, ui, 220.0);
                ui.separator();
                review(q, ui);
            });
    }
}
fn results_table(q: &mut QuantState, ui: &mut egui::Ui, body_height: f32) {
    use super::table::{Cell, Row};
    let fresh = fresh(&q.method);
    let visuals = ui.visuals().clone();
    let rows: Vec<_> = q
        .results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let stale = fresh.stale(r);
            let (label, color) = confidence_tone(&visuals, r, stale);
            let score = r.quality.as_ref().map(|q| q.score as f64);
            Row {
                key: format!("{i}/{}", r.source),
                cells: vec![
                    Cell::text(&r.sample),
                    Cell::text(&r.analyte),
                    Cell::number(r.peak.as_ref().map(|p| p.area)),
                    {
                        let mut cell = Cell::text(format!("● {label}")).colored(color);
                        cell.number = score;
                        cell
                    },
                    Cell::text(if stale || r.peak.is_none() {
                        String::new()
                    } else {
                        r.review().label().to_owned()
                    }),
                ],
            }
        })
        .collect();
    let selected = q.selected.and_then(|i| rows.get(i)).map(|r| r.key.as_str());
    if let Some(i) = super::table::show_with(
        ui,
        "quant_result_matrix",
        &[
            "Sample",
            "Analyte",
            "Area (a.u.·min)",
            "Confidence",
            "Review",
        ],
        &rows,
        selected,
        body_height,
        &[100.0, 70.0, 110.0, 105.0, 110.0],
    ) {
        select(q, i);
    }
}
/// What a confidence flag means for a reviewer and what usually helps.
fn flag_text(flag: &str) -> (&'static str, &'static str) {
    match flag {
        "competing_peak" => ("Competing peak", "Another peak in the RT window: confirm which one is the analyte; try the Valley algorithm or a narrower RT window."),
        "shoulder_or_split" => ("Shoulder / merged peaks", "The peak may hide a second compound: try the Valley / drop-line algorithm or adjust the boundaries."),
        "rt_offset" => ("RT off expected", "The apex is far from the expected retention time: confirm the identity or update the expected RT."),
        "tailing" => ("Tailing", "Long tail: lower the boundary fraction so the tail is included, or use the Valley algorithm."),
        "fronting" => ("Fronting", "The front of the peak is broad: check the boundaries and the injection."),
        "flank_truncated" => ("Peak flank cut short", "A flank does not return to baseline inside the integrated region: widen the boundaries."),
        "poor_peak_shape" => ("Poor peak shape", "The peak is far from Gaussian: check for coelution, tailing or noise."),
        "noisy_peak" => ("Noisy peak", "Many direction changes inside the peak: increase smoothing for detection."),
        "low_snr" => ("Low signal-to-noise", "Signal is close to the noise: the area is uncertain."),
        "few_points" => ("Few scans across the peak", "Too few points define the peak: the area is imprecise; consider a faster scan rate."),
        "boundary_not_at_baseline" => ("Boundaries above baseline", "The peak is cut off before it returns to baseline: lower the boundary fraction or widen the RT window."),
        "saturated_plateau" => ("Detector saturation", "Flat top of identical values: the area is underestimated; dilute and re-inject."),
        "unstable_area" => ("Area depends on settings", "Small changes to boundary or smoothing change the area a lot: review the boundaries."),
        "rt_differs_from_batch" => ("RT differs from other samples", "This sample's peak elutes away from the other samples: possible wrong peak or retention shift."),
        "width_differs_from_batch" => ("Width differs from other samples", "This peak is much wider or narrower than in other samples."),
        "no_signal" => ("No signal", "No signal above the baseline in the integrated region."),
        _ => ("Check", ""),
    }
}
/// Confidence details: reason chips plus the full list of checks.
fn quality_panel(ui: &mut egui::Ui, r: &Measurement) {
    let Some(quality) = &r.quality else {
        if r.status == Status::Ambiguous {
            ui.weak("Saved before confidence scores existed: the old Ambiguous state is shown as Low. Re-run to score it.");
        }
        return;
    };
    let warn = ui.visuals().warn_fg_color;
    ui.horizontal_wrapped(|ui| {
        let flags = quality.flags();
        if flags.is_empty() {
            ui.weak("No problems detected.");
        }
        for flag in flags {
            let (text, hint) = flag_text(flag);
            egui::Frame::new()
                .fill(warn.gamma_multiply(0.18))
                .stroke(egui::Stroke::new(1.0_f32, warn.gamma_multiply(0.6)))
                .corner_radius(8.0)
                .inner_margin(egui::Margin::symmetric(8, 2))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new(text).small().color(warn));
                })
                .response
                .on_hover_text(hint);
        }
    });
    egui::CollapsingHeader::new("Why this score")
        .id_salt(("quality_checks", &r.sample, &r.analyte))
        .show(ui, |ui| {
            egui::Grid::new(("quality_grid", &r.sample, &r.analyte))
                .num_columns(3)
                .spacing([12.0, 2.0])
                .show(ui, |ui| {
                    for c in &quality.checks {
                        let color = if c.flag.is_some() {
                            warn
                        } else {
                            ui.visuals().text_color()
                        };
                        ui.label(egui::RichText::new(&c.name).color(color));
                        ui.add(
                            egui::ProgressBar::new(c.score as f32)
                                .desired_width(70.0)
                                .desired_height(8.0),
                        );
                        ui.small(&c.value);
                        ui.end_row();
                    }
                });
            if quality.cap < 100 {
                ui.small(format!("Score limited to {} by a serious problem above.", quality.cap));
            }
            ui.small("High means nothing was flagged. Rule-based: weighted checks, with serious problems capping the score.");
        });
}
fn metric(ui: &mut egui::Ui, label: &str, value: String) {
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.small(label);
                ui.label(egui::RichText::new(value).strong().size(16.0));
            });
        });
}
fn review(q: &mut QuantState, ui: &mut egui::Ui) {
    if q.results.is_empty() {
        return;
    }
    let Some(i) = q.selected.filter(|&i| i < q.results.len()) else {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.weak("Select a result to inspect or adjust its integration.");
        });
        return;
    };
    let pane_height = ui.available_height();
    let stale = fresh(&q.method).stale(&q.results[i]);
    let (state_label, state_color) = confidence_tone(ui.visuals(), &q.results[i], stale);
    let mut apply = false;
    let mut reset = false;
    let mut accept = false;
    let mut skip_next = false;
    egui::ScrollArea::vertical()
        .id_salt("quant_review_detail")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(format!("{} / {}", q.results[i].sample, q.results[i].analyte));
                let r = &q.results[i];
                let text = if stale || r.peak.is_none() {
                    format!("● {state_label}")
                } else {
                    format!("● {state_label} · {}", r.review().label())
                };
                ui.colored_label(state_color, text);
            });
            if !q.results[i].diagnostic.is_empty() {
                ui.weak(&q.results[i].diagnostic);
            }
            if !stale {
                quality_panel(ui, &q.results[i]);
            }
            if q.results[i].trace.is_empty() {
                return;
            }
            if stale {
                ui.colored_label(
                    Tokens::of(ui).error,
                    "Method changed: use Reprocess above before adjusting this result.",
                );
            }
            let draft_changed = q.results[i]
                .peak
                .as_ref()
                .is_none_or(|p| p.start != q.start || p.end != q.end);
            if let Some(p) = &q.results[i].peak {
                ui.horizontal(|ui| {
                    metric(ui, "Area (a.u.·min)", format!("{:.4e}", p.area));
                    metric(ui, "Apex", format!("{:.3} min", p.apex_rt));
                    metric(ui, "Window", format!("{:.3} – {:.3} min", p.start, p.end));
                });
            }
            ui.add_space(4.0);
            let plot_height = if pane_height.is_finite() {
                (pane_height - 320.0).clamp(200.0, 640.0)
            } else {
                320.0
            };
            let mut mouse = None;
            let mut boundaries = None;
            let applied = applied_integration(q, i);
            let overlay = draft_overlay(q, i);
            let r = &q.results[i];
            let plot = super::plot_controls::plot(ui, ("quant_review", i))
                .height(plot_height)
                .x_axis_label("Retention time (min)")
                .y_axis_label("Intensity (a.u.)")
                .allow_boxed_zoom(false)
                .allow_drag(false);
            let tk = Tokens::of(ui);
            let response = plot.show(ui, |plot_ui| {
                let display = processing::decimate_for_display(&r.trace);
                plot_ui.line(
                    Line::new("Unsmoothed XIC", PlotPoints::from(display))
                        .color(tk.info),
                );
                plot_ui.vline(VLine::new("Start", q.start).color(tk.good));
                plot_ui.vline(VLine::new("End", q.end).color(tk.warn));
                if q.start < q.end {
                    if let Ok(profile) = integration::profile(&r.trace, q.start, q.end, &applied) {
                        plot_ui.line(
                            Line::new("Baseline", profile.baseline.clone()).color(tk.text_dim),
                        );
                        plot_ui.polygon(
                            Polygon::new("Integrated peak", profile_outline(&profile))
                                .stroke(egui::Stroke::new(
                                    1.0_f32,
                                    tk.info.gamma_multiply(0.4),
                                ))
                                .fill_color(tk.info.gamma_multiply(0.18)),
                        );
                    }
                }
                if let Some((peak, profile)) = &overlay {
                    let orange = tk.warn;
                    plot_ui.line(
                        Line::new("Preview baseline", profile.baseline.clone())
                            .color(orange)
                            .style(egui_plot::LineStyle::dashed_loose()),
                    );
                    plot_ui.polygon(
                        Polygon::new("Preview peak", profile_outline(profile))
                            .stroke(egui::Stroke::new(1.5_f32, orange))
                            .fill_color(tk.warn.gamma_multiply(0.16)),
                    );
                    plot_ui.vline(
                        VLine::new("Preview start", peak.start)
                            .color(orange)
                            .style(egui_plot::LineStyle::dashed_dense()),
                    );
                    plot_ui.vline(
                        VLine::new("Preview end", peak.end)
                            .color(orange)
                            .style(egui_plot::LineStyle::dashed_dense()),
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
            ui.add_space(6.0);
            let r = &q.results[i];
            ui.add_enabled_ui(!stale && q.rx.is_none(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label("Integration window");
                    ui.add(egui::DragValue::new(&mut q.start).speed(0.01).suffix(" min"));
                    ui.label("to");
                    ui.add(egui::DragValue::new(&mut q.end).speed(0.01).suffix(" min"));
                    apply |= ui
                        .add_enabled(draft_changed, egui::Button::new("Apply"))
                        .on_hover_text("Integrate between these boundaries")
                        .clicked();
                    reset = ui
                        .add_enabled(
                            r.peak != r.automatic || r.status != r.automatic_status,
                            egui::Button::new("Reset to automatic"),
                        )
                        .clicked();
                    ui.separator();
                    accept = ui
                        .add_enabled(
                            r.peak.is_some()
                                && !draft_changed
                                && matches!(r.status, Status::Automatic | Status::Ambiguous),
                            egui::Button::new(egui::RichText::new("✓ Accept peak").strong())
                                .fill(ui.visuals().selection.bg_fill),
                        )
                        .clicked();
                    skip_next = ui
                        .button("Next ›")
                        .on_hover_text("Jump to the next peak that needs a look, without accepting this one")
                        .clicked();
                });
            });
            if draft_changed && r.peak.is_some() {
                ui.colored_label(ui.visuals().warn_fg_color, "Boundary edits are not yet applied.");
            }
            ui.small("Drag a boundary with the left mouse button, or right-drag across the plot to set a new window. Edits affect only this sample and analyte.");
            super::plot_controls::export(
                ui,
                &q.results[i].trace,
                "RT (min)",
                "Instrument intensity (unsmoothed)",
            );
            ui.add_space(4.0);
            integration_card(q, i, ui);
        });
    if accept && q.results[i].status != Status::Manual {
        q.results[i].status = Status::Reviewed;
    }
    if skip_next && !reset && !apply && next_needing_review(q) {
        super::table::scroll_to_selected(ui.ctx(), "quant_result_matrix");
    }
    if reset {
        q.results[i].peak = q.results[i].automatic.clone();
        q.results[i].status = q.results[i].automatic_status.clone();
        select(q, i);
    }
    if apply {
        let analyte = q
            .method
            .analytes
            .iter()
            .find(|a| a.extraction.name == q.results[i].analyte)
            .cloned();
        let measured = match &analyte {
            Some(a) => measure_for(&q.results[i].trace, q.start, q.end, a, &q.method),
            None => measure(&q.results[i].trace, q.start, q.end),
        };
        match measured {
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
    serde_json::json!({"job_id":format!("quantification:{}",q.remote_job_id),"running":q.rx.is_some(),"completed":q.completed,"total":q.total,"message":q.message,"method":signature(&q.method),"results":q.results.iter().enumerate().map(|(i,r)|serde_json::json!({"result_index":i,"sample":r.sample,"source":r.source,"run":r.run,"analyte":r.analyte,"parameters":r.params,"method":r.method,"automatic":r.automatic,"peak":r.peak,"automatic_status":r.automatic_status,"status":r.status,"diagnostic":r.diagnostic,"review":r.review().label(),"needs_review":r.needs_review(),"confidence":r.quality,"point_count":r.trace.len()})).collect::<Vec<_>>()})
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
    let method = app.quant.method.clone();
    let stale = app
        .quant
        .results
        .get(index)
        .map(|r| fresh(&method).stale(r));
    let r = app
        .quant
        .results
        .get_mut(index)
        .ok_or("Unknown result index")?;
    if stale != Some(false) {
        return Err("Method changed; rerun before reviewing".into());
    }
    match (start, end) {
        (Some(a), Some(z)) => {
            if !a.is_finite() || !z.is_finite() {
                return Err("Finite boundaries required".into());
            }
            let analyte = method
                .analytes
                .iter()
                .find(|x| x.extraction.name == r.analyte)
                .ok_or("Result analyte is absent from the method.")?;
            r.peak = Some(measure_for(&r.trace, a, z, analyte, &method)?);
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

fn analyte_index(q: &QuantState, i: usize) -> Option<usize> {
    let name = &q.results.get(i)?.analyte;
    q.method
        .analytes
        .iter()
        .position(|a| &a.extraction.name == name)
}
/// Classic settings built from the method-level smoothing, prominence and boundary fraction.
fn method_default(method: &Method) -> Integration {
    Integration {
        smoothing: method.detection_smoothing,
        minimum_prominence: method.minimum_height,
        boundary_fraction: method.boundary_fraction,
        ..Integration::default()
    }
}
/// The settings currently applied to a result's analyte.
fn applied_integration(q: &QuantState, i: usize) -> Integration {
    analyte_index(q, i)
        .map(|k| q.method.analytes[k].integration_for(&q.method))
        .unwrap_or_default()
}
/// Closed outline of the integrated area: the signal, then the baseline back.
fn profile_outline(profile: &integration::Profile) -> Vec<[f64; 2]> {
    let mut points = processing::decimate_for_display(&profile.signal);
    points.extend(profile.baseline.iter().rev().copied());
    points
}
/// Peak and baseline the draft settings would give, when they differ from the applied ones.
fn draft_overlay(q: &QuantState, i: usize) -> Option<(crate::quant::Peak, integration::Profile)> {
    let k = analyte_index(q, i)?;
    let (name, _, draft) = q.draft.as_ref()?;
    let r = &q.results[i];
    if *name != r.analyte || *draft == applied_integration(q, i) || draft.validate().is_err() {
        return None;
    }
    let a = &q.method.analytes[k];
    let (peak, _) = integration::detect(&r.trace, a.rt_window, a.expected_rt, draft);
    let peak = peak?;
    let profile = integration::profile(&r.trace, peak.start, peak.end, draft).ok()?;
    Some((peak, profile))
}
/// Writes `cfg` into the analyte's method entry and re-integrates its stored traces.
fn apply_integration(q: &mut QuantState, k: usize, cfg: Integration) {
    let default = method_default(&q.method);
    q.method.analytes[k].integration = (cfg != default).then_some(cfg);
    let method = q.method.clone();
    let analyte = method.analytes[k].clone();
    let name = analyte.extraction.name.clone();
    let snapshot = analyte_signature(&method, &name);
    let mut count = 0;
    for r in q
        .results
        .iter_mut()
        .filter(|r| r.analyte == name && !r.trace.is_empty())
    {
        r.method = snapshot.clone();
        let kept = matches!(r.status, Status::Manual | Status::Reviewed);
        if kept {
            // Boundaries were chosen by a scientist; only the baseline-dependent area changes.
            if let Some(p) = &r.peak {
                if let Ok(updated) = measure_for(&r.trace, p.start, p.end, &analyte, &method) {
                    r.peak = Some(updated);
                }
            }
        }
        let (peak, status, quality) = detect_assessed(&r.trace, &analyte, &method);
        r.quality = quality;
        r.automatic = peak.clone();
        r.automatic_status = status.clone();
        if !kept {
            r.peak = peak;
            r.status = status;
        }
        count += 1;
    }
    refresh_consistency(&mut q.results);
    q.message = format!(
        "Integration settings applied to {count} sample(s) of {name}. Save the method to keep them in the file."
    );
}
/// Per-analyte integration settings with live preview on the selected result.
fn integration_card(q: &mut QuantState, i: usize, ui: &mut egui::Ui) {
    let Some(k) = analyte_index(q, i) else {
        return;
    };
    let name = q.results[i].analyte.clone();
    let applied = applied_integration(q, i);
    let (window, expected) = (
        q.method.analytes[k].rt_window,
        q.method.analytes[k].expected_rt,
    );
    let mut cfg = match q.draft.take() {
        Some((n, base, draft)) if n == name && base == applied => draft,
        _ => applied.clone(),
    };
    let default = method_default(&q.method);
    let mut action = None;
    egui::CollapsingHeader::new(
        egui::RichText::new(format!("Integration settings: {name}")).strong(),
    )
    .default_open(true)
    .show(ui, |ui| {
        ui.small("Saved per analyte in the method. Changes preview on this peak (orange); Apply stores them and re-integrates every sample of this analyte.");
        integration_editor::editor(ui, &mut cfg, window);
        let valid = cfg.validate();
        if let Err(e) = &valid {
            ui.colored_label(ui.visuals().warn_fg_color, e);
        }
        let differs = cfg != applied;
        if differs && valid.is_ok() {
            let r = &q.results[i];
            let (peak, _) = integration::detect(&r.trace, window, expected, &cfg);
            ui.horizontal_wrapped(|ui| match (&peak, &r.peak) {
                (Some(p), current) => {
                    metric(ui, "Preview area", format!("{:.4e}", p.area));
                    if let Some(c) = current.as_ref().filter(|c| c.area != 0.0) {
                        metric(
                            ui,
                            "Change",
                            format!("{:+.1} %", (p.area / c.area - 1.0) * 100.0),
                        );
                    }
                    metric(ui, "Apex", format!("{:.3} min", p.apex_rt));
                    metric(ui, "Window", format!("{:.3} – {:.3} min", p.start, p.end));
                }
                (None, _) => {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "No peak is found with these settings.",
                    );
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            let can_apply = differs && valid.is_ok() && q.rx.is_none();
            if ui
                .add_enabled(
                    can_apply,
                    egui::Button::new(
                        egui::RichText::new(format!("Apply to all {name} samples")).strong(),
                    )
                    .fill(ui.visuals().selection.bg_fill),
                )
                .on_hover_text(
                    "Re-integrates every sample of this analyte and stores the settings in the method",
                )
                .clicked()
            {
                action = Some(true);
            }
            if ui
                .add_enabled(differs, egui::Button::new("Revert"))
                .on_hover_text("Discard the changes and show the applied settings")
                .clicked()
            {
                action = Some(false);
            }
            if ui
                .button("Reset to classic defaults")
                .on_hover_text(
                    "Classic algorithm with the method-level smoothing, prominence and boundary fraction",
                )
                .clicked()
            {
                cfg = default.clone();
            }
        });
    });
    match action {
        Some(true) => {
            apply_integration(q, k, cfg);
            select(q, i);
        }
        Some(false) => q.draft = None,
        None => q.draft = Some((name, applied, cfg)),
    }
}

impl QuantState {
    pub(super) fn project_selection(
        &self,
        files: &std::collections::HashMap<super::state::FileId, super::state::OpenFile>,
    ) -> Vec<(String, String)> {
        let mut selected: Vec<_> = self
            .selected_samples
            .iter()
            .filter_map(|id| files.get(id))
            .map(|file| {
                (
                    file.cache
                        .source_path
                        .clone()
                        .unwrap_or_else(|| file.path.clone()),
                    file.name.clone(),
                )
            })
            .collect();
        selected.sort();
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quant::detect;
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
        let (peak, status, quality) = detect_assessed(&t, &m.analytes[0], &m);
        Measurement {
            chromatography: None,
            quality,
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
        // A second candidate no longer changes the status: it lowers the confidence instead.
        let (p, status, quality) = detect_assessed(&t, &m.analytes[0], &m);
        assert_eq!(status, Status::Automatic);
        assert_eq!(p.unwrap().apex_rt, 1.5);
        let quality = quality.unwrap();
        assert!(!quality.flags().is_empty());
        assert_ne!(quality.tier, Tier::High);
        // A peak clipped by the window edge cannot be High either.
        let t = vec![[0.0, 5.0], [1.0, 10.0], [2.0, 1.0], [3.0, 1.0]];
        let (_, status, quality) = detect_assessed(&t, &m.analytes[0], &m);
        assert_eq!(status, Status::Automatic);
        assert!(quality.is_none_or(|q| q.tier != Tier::High));
    }
    #[test]
    fn review_state_and_confidence_are_separate_axes() {
        let mut r = result();
        r.quality = Some(crate::confidence::Quality {
            score: 40,
            tier: Tier::Low,
            cap: 100,
            fwhm: None,
            checks: vec![],
        });
        assert!(r.needs_review() && r.low_confidence_unreviewed());
        assert_eq!(r.review().label(), "Not reviewed");
        r.status = Status::Reviewed;
        assert!(!r.needs_review() && !r.low_confidence_unreviewed());
        assert_eq!(r.review().label(), "Reviewed");
        assert_eq!(r.tier(), Some(Tier::Low), "confidence is kept after review");
        r.status = Status::Manual;
        assert_eq!(r.review().label(), "Edited");
        // Results saved before scores existed: Ambiguous reads as Low and unreviewed.
        let mut legacy = result();
        legacy.quality = None;
        legacy.status = Status::Ambiguous;
        assert_eq!(legacy.tier(), Some(Tier::Low));
        assert!(legacy.needs_review());
        let json = serde_json::to_string(&legacy).unwrap();
        assert!(!json.contains("quality"));
        assert!(serde_json::from_str::<Measurement>(&json)
            .unwrap()
            .quality
            .is_none());
    }
    #[test]
    fn ripples_in_a_long_tail_do_not_end_the_peak_early() {
        let mut m = method();
        m.boundary_fraction = 0.05;
        m.analytes[0].rt_window = [0.0, 8.0];
        let t = vec![
            [0.0, 0.0],
            [1.0, 100.0],
            [1.5, 1000.0],
            [2.0, 300.0],
            [2.5, 120.0],
            [3.0, 130.0],
            [3.5, 60.0],
            [4.0, 70.0],
            [4.5, 20.0],
            [5.0, 0.0],
            [8.0, 0.0],
        ];
        let (p, _) = detect(&t, &m.analytes[0], &m);
        // Ends where the tail falls below 5% of the apex (20 < 50), not at the ripple at 2.5.
        assert_eq!(p.unwrap().end, 4.5);
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
    fn advanced_batch_history_roundtrip_and_corruption_rejection() {
        use crate::chromatography::{self as c, Correction};
        let mut r = result();
        let a = c::process(
            c::Trace::from_points(format!("{} / {}", r.sample, r.analyte), &r.trace),
            c::Config {
                smoothing: None,
                baseline: c::Baseline::None,
                minimum_snr: 0.0,
                ..Default::default()
            },
        )
        .unwrap();
        r.chromatography = Some(
            c::revise(
                &a,
                0,
                Correction::Manual {
                    intervals: vec![[0.75, 2.6]],
                    reason: "GUI bounds".into(),
                },
                "gui",
                false,
            )
            .unwrap(),
        );
        let batch = Batch {
            version: 1,
            method: method(),
            results: vec![r],
            samples: vec![],
        };
        let bytes = serde_json::to_vec(&batch).unwrap();
        let mut restored: Batch = serde_json::from_slice(&bytes).unwrap();
        validate_batch(&restored).unwrap();
        assert_eq!(
            restored.results[0].chromatography,
            batch.results[0].chromatography
        );
        restored.results[0]
            .chromatography
            .as_mut()
            .unwrap()
            .revisions[0]
            .peaks[0]
            .area_intensity_minutes += 1.0;
        assert!(validate_batch(&restored).is_err());
    }
    fn two_analyte_method() -> Method {
        let mut m = method();
        let mut second = new_analyte(2);
        second.expected_rt = 1.5;
        second.rt_window = [0.0, 4.0];
        m.analytes.push(second);
        m
    }
    #[test]
    fn editing_one_analyte_leaves_other_analytes_results_current() {
        let m = two_analyte_method();
        let mut first = result();
        first.method = analyte_signature(&m, &m.analytes[0].extraction.name);
        let mut second = result();
        second.analyte = m.analytes[1].extraction.name.clone();
        second.method = analyte_signature(&m, &second.analyte);
        let mut legacy = result();
        legacy.analyte = m.analytes[1].extraction.name.clone();
        legacy.method = signature(&m);
        assert!(!fresh(&m).stale(&first) && !fresh(&m).stale(&second));
        assert!(
            !fresh(&m).stale(&legacy),
            "whole-method snapshots stay current"
        );
        let mut edited = m.clone();
        edited.analytes[0].integration = Some(Integration {
            baseline: integration::BaselineKind::Zero,
            ..Integration::default()
        });
        let f = fresh(&edited);
        assert!(f.stale(&first));
        assert!(!f.stale(&second) && !f.stale(&legacy));
        edited.analytes[1].integration = Some(Integration::default());
        assert!(fresh(&edited).stale(&legacy));
    }
    #[test]
    fn applying_settings_reintegrates_one_analyte_and_survives_batch_validation() {
        let mut q = QuantState::default();
        q.method = two_analyte_method();
        let mut first = result();
        first.analyte = q.method.analytes[0].extraction.name.clone();
        first.method = analyte_signature(&q.method, &first.analyte);
        let mut other = result();
        other.analyte = q.method.analytes[1].extraction.name.clone();
        other.method = analyte_signature(&q.method, &other.analyte);
        let untouched = other.clone();
        let before = first.peak.clone().unwrap().area;
        q.results = vec![first, other];
        let cfg = Integration {
            baseline: integration::BaselineKind::Zero,
            smoothing: 0,
            ..Integration::default()
        };
        apply_integration(&mut q, 0, cfg.clone());
        assert_eq!(q.method.analytes[0].integration, Some(cfg));
        assert!(q.method.analytes[1].integration.is_none());
        assert_eq!(q.results[1].peak, untouched.peak);
        assert!(q.results[0].peak.as_ref().unwrap().area >= before);
        assert!(!fresh(&q.method).stale(&q.results[0]));
        assert!(!fresh(&q.method).stale(&q.results[1]));
        // Stored settings reproduce: the batch validates and survives a method round trip.
        validate_batch(&Batch {
            version: 1,
            method: q.method.clone(),
            results: q.results.clone(),
            samples: vec![],
        })
        .unwrap();
        let text = signature(&q.method);
        assert!(text.contains("[analytes.integration]") && text.contains("baseline = \"zero\""));
        let restored: Method = toml::from_str(&text).unwrap();
        assert_eq!(signature(&restored), text);
        // Applying the method defaults again removes the custom table.
        let default = method_default(&q.method);
        apply_integration(&mut q, 0, default);
        assert!(q.method.analytes[0].integration.is_none());
    }
    #[test]
    fn integration_card_renders_with_a_live_preview_at_small_and_large_sizes() {
        for width in [520.0, 1200.0] {
            let mut q = QuantState::default();
            q.method = method();
            q.results = vec![result()];
            select(&mut q, 0);
            q.draft = Some((
                q.results[0].analyte.clone(),
                applied_integration(&q, 0),
                Integration {
                    baseline: integration::BaselineKind::Zero,
                    ..applied_integration(&q, 0)
                },
            ));
            let ctx = egui::Context::default();
            super::super::workbench::configure(&ctx, true);
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 1400.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| integration_card(&mut q, 0, ui));
                    });
                },
            );
            for label in ["Baseline", "Revert", "Preview area"] {
                assert!(
                    super::super::test_render::text_center(&output.shapes, label).is_some(),
                    "missing {label} at width {width}"
                );
            }
            assert!(q.draft.is_some());
        }
    }
    #[test]
    fn committing_an_edited_method_flags_old_results_without_touching_the_file() {
        let mut q = QuantState::default();
        q.method = method();
        q.saved_method = signature(&q.method);
        let mut r = result();
        r.method = analyte_signature(&q.method, &r.analyte);
        q.results = vec![r];
        let mut draft = q.method.clone();
        // An unchanged draft is a no-op.
        commit_method(&mut q, &draft);
        assert!(q.message.is_empty());
        draft.analytes[0].integration = Some(Integration {
            baseline: integration::BaselineKind::Zero,
            ..Integration::default()
        });
        commit_method(&mut q, &draft);
        assert!(q.message.contains("1 result(s)") && q.message.contains("Reprocess"));
        assert!(fresh(&q.method).stale(&q.results[0]));
        assert_ne!(
            signature(&q.method),
            q.saved_method,
            "the file text is unchanged until Save"
        );
    }
    #[test]
    fn method_window_edits_a_draft_until_ok() {
        let mut app = MzViewerApp::default();
        app.quant.active = true;
        app.quant.method = method();
        app.quant.method_open = true;
        let ctx = egui::Context::default();
        super::super::workbench::configure(&ctx, true);
        let run_frame = |app: &mut MzViewerApp| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1300.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ctx| show(app, ctx),
            )
        };
        // A new window is laid out invisibly on its first frame.
        run_frame(&mut app);
        let output = run_frame(&mut app);
        for label in ["OK", "Cancel"] {
            assert!(
                super::super::test_render::text_center(&output.shapes, label).is_some(),
                "missing {label}"
            );
        }
        // Edits live in the draft; the batch method is untouched until OK.
        let before = signature(&app.quant.method);
        app.quant.method_draft.as_mut().unwrap().1.name = "Edited".into();
        run_frame(&mut app);
        assert_eq!(signature(&app.quant.method), before);
        assert_eq!(app.quant.method_draft.as_ref().unwrap().1.name, "Edited");
    }
    #[test]
    fn confidence_and_review_are_shown_in_the_table_and_the_review_pane() {
        let mut q = QuantState::default();
        q.method = method();
        let mut r = result();
        r.method = analyte_signature(&q.method, &r.analyte);
        r.quality = Some(crate::confidence::Quality {
            score: 62,
            tier: Tier::Medium,
            cap: 100,
            fwhm: Some(0.2),
            checks: vec![crate::confidence::Check {
                name: "Asymmetry".into(),
                value: "2.60 at 10 % height".into(),
                score: 0.2,
                weight: 0.1,
                flag: Some("tailing".into()),
            }],
        });
        q.results = vec![r];
        select(&mut q, 0);
        let ctx = egui::Context::default();
        super::super::workbench::configure(&ctx, true);
        let mut output = None;
        for _ in 0..2 {
            output = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1400.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| review_stage(&mut q, ui));
                },
            ));
        }
        let output = output.unwrap();
        for label in ["Confidence", "Review", "Tailing", "Why this score"] {
            assert!(
                super::super::test_render::text_center(&output.shapes, label).is_some(),
                "missing {label}"
            );
        }
        assert!(
            super::super::test_render::text_center(&output.shapes, "● 62 Medium").is_some(),
            "table shows the score and tier"
        );
    }
    #[test]
    fn csv_quotes_names_keeps_missing_areas_blank_and_marks_stale_methods() {
        let mut missing = result();
        missing.peak = None;
        missing.quality = None;
        missing.automatic = None;
        missing.status = Status::Missing;
        missing.automatic_status = Status::Missing;
        let mut m = method();
        m.analytes[0].extraction.ppm = 20.0;
        let output = csv(&[missing], &m);
        assert!(output.contains("\"sample, \"\"A\"\"\""));
        assert!(output.contains(
            "\"Missing\",\"true\",\"Not reviewed\",\"\",\"\",\"\",\"\",\"\",\"\",\"\",\""
        ));
        assert!(output.contains("\"straight_line\",\"unsmoothed\",\"classic\""));
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
