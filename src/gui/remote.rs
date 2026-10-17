//! Commands are applied exclusively on the egui owner thread.
use super::{plotting, state::*, workspace, MzViewerApp};
use crate::{
    mcp::{Command, DatasetRef, GuiBridge},
    parser::MzData,
    plotting_parameters::PlotType,
    processing::{self, ProcessingParams},
    validation::XicParams,
};
use mzdata::spectrum::ScanPolarity;
use rmcp::model::CallToolResult;
use serde_json::{json, Value};

pub(crate) struct PendingOperation {
    source: PendingSource,
    reply: tokio::sync::oneshot::Sender<CallToolResult>,
    request: Value,
    selection: Option<usize>,
    input: UserInput,
}
/// What a pending operation waits for.
enum PendingSource {
    /// A worker thread's result.
    Worker(std::sync::mpsc::Receiver<Result<BackgroundResult, String>>),
    /// The window itself finishing something it started (a session being restored, a project
    /// being opened), which goes on over several frames.
    Window {
        wait: Wait,
        started: std::time::Instant,
    },
}
/// Window work an operation waits for before it replies.
pub(crate) enum Wait {
    /// The datasets and traces of a loaded session.
    Session,
    /// A project operation: done when the number of successful ones has grown past `before`
    /// and the window has finished reopening datasets.
    Project { before: u64 },
}
/// How long the window may take to finish what a client waits for.
const WAIT_LIMIT: std::time::Duration = std::time::Duration::from_secs(600);
/// Runs of one imported input, and the temporary folder that keeps a converted run alive.
type Imported = (
    Vec<processing::LoadedRun>,
    Option<std::sync::Arc<tempfile::TempDir>>,
);
enum BackgroundResult {
    /// A result that is already complete.
    Value(Value),
    Opened(Vec<(std::path::PathBuf, Result<Imported, String>)>),
    Extracted {
        dataset_id: usize,
        display: bool,
        points: Vec<[f64; 2]>,
        chrom: crate::parser::ChromatogramData,
        params: ProcessingParams,
    },
}

/// The URI templates of the resources, for the server's resource template listing.
pub fn resource_templates() -> &'static [(&'static str, &'static str, &'static str)] {
    resources::TEMPLATES
}

/// The stable id of one open dataset: the SHA-256 of its source file, computed off the window
/// thread the first time the dataset is seen.
pub(crate) enum StableState {
    Hashing(std::sync::mpsc::Receiver<Result<String, String>>),
    Ready(String),
    Failed(String),
}
impl StableState {
    fn json(&self) -> Value {
        match self {
            Self::Hashing(_) => json!({"stable_id":null,"stable_id_state":"computing"}),
            Self::Ready(hash) => {
                json!({"stable_id":format!("sha256:{hash}"),"stable_id_state":"ready"})
            }
            Self::Failed(error) => {
                json!({"stable_id":null,"stable_id_state":format!("failed: {error}")})
            }
        }
    }
}
/// The file whose bytes identify a dataset: the original single source file when there is one
/// (a vendor `.raw` file, an mzML), otherwise the file the window reads (a converted run).
fn identity_path(file: &OpenFile) -> std::path::PathBuf {
    match file.cache.source_path.as_deref() {
        Some(source) if std::path::Path::new(source).is_file() => source.into(),
        _ => (&file.path).into(),
    }
}

fn range(r: [f64; 2]) -> Result<[f64; 2], String> {
    if !r.iter().all(|x| x.is_finite() && *x >= 0.0) || r[0] >= r[1] {
        return Err("Range must be finite, nonnegative and increasing".into());
    }
    Ok(r)
}
fn page(offset: usize, limit: usize) -> Result<(), String> {
    if limit == 0 || limit > 10000 || offset.checked_add(limit).is_none() {
        return Err("Page limit must be 1..10000 and offset must not overflow".into());
    }
    Ok(())
}
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
/// Runs the shared engine's `integrate` operation on an extracted trace and returns its output
/// (`kind`, `area`, `area_unit`, `algorithm`), the same value `start_analysis` produces.
fn engine_integration(points: &[[f64; 2]], start: f64, end: f64) -> Result<Value, String> {
    use crate::domain::{Minutes, Operation, Request};
    let request = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "chromascope-mcp".into(),
        operation: Operation::Integrate {
            points: points.to_vec(),
            start: Minutes(start),
            end: Minutes(end),
        },
    };
    let response = crate::engine::execute(
        std::path::Path::new("-"),
        request,
        &crate::jobs::JobControl::default(),
    )
    .map_err(|e| e.message)?;
    serde_json::to_value(response.output).map_err(err)
}
/// Imports every path (converting vendor data with `msconvert` when needed), one after another.
fn import_all(
    paths: Vec<std::path::PathBuf>,
    msconvert: Option<std::path::PathBuf>,
) -> Vec<(std::path::PathBuf, Result<Imported, String>)> {
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    paths
        .into_iter()
        .map(|path| {
            let loaded = match processing::import_file_in_background(
                path.clone(),
                msconvert.clone(),
                cancelled.clone(),
                0,
            ) {
                processing::FileLoadingResult::Success {
                    runs, workspace, ..
                } => Ok((runs, workspace)),
                processing::FileLoadingResult::Error { message, .. } => Err(message),
            };
            (path, loaded)
        })
        .collect()
}
fn extraction_parameters(
    command: Command,
    bounds: &crate::validation::DataBounds,
    stored: &[crate::parser::NativeChromatogramInfo],
) -> Result<ProcessingParams, String> {
    let Command::ExtractChromatogram {
        kind,
        polarity,
        ms_level,
        smoothing,
        mass,
        tolerance_ppm,
        tolerance_mda,
        mz_range,
        precursor_mz,
        acquisition,
        native_chromatogram,
        ..
    } = command
    else {
        return Err("Expected extraction request".into());
    };
    if ms_level == 0 || smoothing > 10 {
        return Err("MS level must be positive; smoothing must be 0..10".into());
    }
    if let Some(index) = native_chromatogram {
        // A chromatogram stored in the file: nothing is computed from the spectra, so the
        // spectrum filters have no meaning and are refused rather than silently ignored.
        if !stored.iter().any(|c| c.index == index) {
            return Err(format!(
                "The file stores no chromatogram {index}; dataset_metadata lists the stored chromatograms ({} in this file)",
                stored.len()
            ));
        }
        if mass.is_some()
            || tolerance_ppm.is_some()
            || tolerance_mda.is_some()
            || mz_range.is_some()
            || precursor_mz.is_some()
            || acquisition.is_some()
            || kind != "tic"
        {
            return Err("A stored chromatogram takes only native_chromatogram and smoothing; omit kind, mass, tolerances, mz_range, precursor_mz and acquisition".into());
        }
        return Ok(ProcessingParams {
            acquisition: None,
            plot_type: PlotType::Tic,
            polarity: match polarity.as_str() {
                "positive" => ScanPolarity::Positive,
                "negative" => ScanPolarity::Negative,
                "unknown" => ScanPolarity::Unknown,
                _ => return Err("Polarity: positive, negative or unknown".into()),
            },
            ms_level,
            smoothing,
            xic_params: None,
            mz_range: None,
            precursor_mz: None,
            native_chromatogram: Some(index),
        });
    }
    let polarity = match polarity.as_str() {
        "positive" => ScanPolarity::Positive,
        "negative" => ScanPolarity::Negative,
        "unknown" => ScanPolarity::Unknown,
        _ => return Err("Polarity: positive, negative or unknown".into()),
    };
    let kind = match kind.as_str() {
        "tic" => PlotType::Tic,
        "bpc" => PlotType::Bpc,
        "xic" => PlotType::Xic,
        _ => return Err("Kind: tic, bpc or xic".into()),
    };
    if mass.is_some_and(|v| !v.is_finite())
        || tolerance_ppm.is_some_and(|v| !v.is_finite())
        || tolerance_mda.is_some_and(|v| !v.is_finite())
        || precursor_mz.is_some_and(|v| !v.is_finite() || v <= 0.0)
    {
        return Err("Mass and tolerance must be finite; precursor must be positive".into());
    }
    if kind != PlotType::Xic
        && (mass.is_some() || tolerance_ppm.is_some() || tolerance_mda.is_some())
    {
        return Err(
            "Mass and tolerance (ppm or mDa) are XIC parameters; omit them for TIC/BPC".into(),
        );
    }
    if kind == PlotType::Xic && mz_range.is_some() {
        return Err("XIC uses mass and a tolerance; mz_range applies to TIC/BPC".into());
    }
    let tolerance = match (tolerance_ppm, tolerance_mda) {
        (Some(_), Some(_)) => {
            return Err("Give tolerance_ppm or tolerance_mda, not both".into());
        }
        (Some(v), None) => Some(crate::validation::MassTolerance::ppm(v)),
        (None, Some(v)) => Some(crate::validation::MassTolerance::mda(v)),
        (None, None) => None,
    };
    let acquisition = match acquisition.as_deref() {
        None => None,
        Some("FS") => Some(processing::AcquisitionMode::FS),
        Some("SIM") => Some(processing::AcquisitionMode::SIM),
        Some("MRM") => Some(processing::AcquisitionMode::MRM),
        _ => return Err("Acquisition: FS, SIM or MRM".into()),
    };
    let params = ProcessingParams {
        acquisition,
        plot_type: kind,
        polarity,
        ms_level,
        smoothing,
        xic_params: if kind == PlotType::Xic {
            Some(
                XicParams::with_tolerance(
                    mass.ok_or("XIC requires mass")?,
                    polarity,
                    tolerance.ok_or("XIC requires tolerance_ppm or tolerance_mda")?,
                    bounds,
                )
                .map_err(err)?,
            )
        } else {
            None
        },
        mz_range: mz_range.map(range).transpose()?.map(|[a, z]| (a, z)),
        precursor_mz,
        native_chromatogram: None,
    };

    Ok(params)
}

impl MzViewerApp {
    pub fn attach_mcp(&mut self, bridge: GuiBridge) {
        self.remote = Some(bridge);
    }
    pub(crate) fn poll_remote(&mut self, ctx: &egui::Context) {
        let Some(mut bridge) = self.remote.take() else {
            return;
        };
        self.drain_internal(&mut bridge);
        self.refresh_stable_ids(&mut bridge);
        if let Some(pending) = bridge.pending.take() {
            let result = match &pending.source {
                PendingSource::Worker(receiver) => match receiver.try_recv() {
                    Ok(result) => result,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        bridge.pending = Some(pending);
                        self.remote = Some(bridge);
                        ctx.request_repaint_after(std::time::Duration::from_millis(50));
                        return;
                    }
                    Err(_) => Err("Analytical worker stopped before returning a result".into()),
                },
                PendingSource::Window { wait, started } => {
                    match self.check_wait(wait, *started, &bridge) {
                        None => {
                            bridge.pending = Some(pending);
                            self.remote = Some(bridge);
                            ctx.request_repaint_after(std::time::Duration::from_millis(50));
                            return;
                        }
                        Some(done) => done.map(BackgroundResult::Value),
                    }
                }
            };
            if !pending.reply.is_closed() {
                let result = result.and_then(|r| self.apply_background(r, &pending, &mut bridge));
                Self::reply_remote(result, pending.request, pending.reply, &mut bridge);
            }
        }
        // One command per frame preserves fairness with local input.
        if let Ok(envelope) = bridge.receiver.try_recv() {
            if !envelope.reply.is_closed() {
                let mut command = envelope.command;
                let entry = serde_json::to_value(&command).unwrap_or(Value::Null);
                if let Err(message) = self.resolve_dataset_refs(&mut command, &bridge) {
                    Self::reply_remote(Err(message), entry, envelope.reply, &mut bridge);
                    self.remote = Some(bridge);
                    ctx.request_repaint();
                    return;
                }
                match self.start_window_wait(&command, &mut bridge, ctx) {
                    Ok(Some(wait)) => {
                        bridge.pending = Some(PendingOperation {
                            source: PendingSource::Window {
                                wait,
                                started: std::time::Instant::now(),
                            },
                            reply: envelope.reply,
                            request: entry,
                            selection: self.active_file_id,
                            input: self.user_input.clone(),
                        });
                        self.remote = Some(bridge);
                        ctx.request_repaint_after(std::time::Duration::from_millis(50));
                        return;
                    }
                    Err(message) => {
                        Self::reply_remote(Err(message), entry, envelope.reply, &mut bridge);
                        self.remote = Some(bridge);
                        ctx.request_repaint();
                        return;
                    }
                    Ok(None) => {}
                }
                match self.start_background(&command, &bridge, ctx) {
                    Ok(Some(receiver)) => {
                        bridge.pending = Some(PendingOperation {
                            source: PendingSource::Worker(receiver),
                            reply: envelope.reply,
                            request: entry,
                            selection: self.active_file_id,
                            input: self.user_input.clone(),
                        });
                        self.remote = Some(bridge);
                        ctx.request_repaint_after(std::time::Duration::from_millis(50));
                        return;
                    }
                    Err(message) => {
                        Self::reply_remote(Err(message), entry, envelope.reply, &mut bridge);
                        self.remote = Some(bridge);
                        ctx.request_repaint();
                        return;
                    }
                    Ok(None) => {}
                }
                let result = self.remote_command(command, &mut bridge, ctx);
                Self::reply_remote(result, entry, envelope.reply, &mut bridge);
            }
            ctx.request_repaint();
        }
        self.remote = Some(bridge);
    }
    /// Starts the operations whose reply depends on the window finishing work over several
    /// frames, after the same permission checks as every other command.
    fn start_window_wait(
        &mut self,
        command: &Command,
        b: &mut GuiBridge,
        ctx: &egui::Context,
    ) -> Result<Option<Wait>, String> {
        match command {
            Command::LoadSession { path } => {
                self.remote_validate(command, b)?;
                files::load_session(self, ctx, b, path).map(|()| Some(Wait::Session))
            }
            Command::CreateProject { path } => projects::create(self, ctx, b, path).map(Some),
            Command::OpenProject { path } => {
                self.remote_validate(command, b)?;
                projects::open(self, b, path).map(Some)
            }
            Command::SaveProject {} => projects::save(self, ctx, b).map(Some),
            Command::RefreshProject {} => {
                self.remote_validate(command, b)?;
                projects::refresh(self, b).map(Some)
            }
            _ => Ok(None),
        }
    }
    /// `None` while the window is still working; the reply once it is done or failed.
    fn check_wait(
        &self,
        wait: &Wait,
        started: std::time::Instant,
        bridge: &GuiBridge,
    ) -> Option<Result<Value, String>> {
        if let Wait::Project { before } = wait {
            return match projects::finished(self, *before) {
                None => (started.elapsed() > WAIT_LIMIT).then(|| {
                    Err("The window did not finish in ten minutes; check gui_state".to_owned())
                }),
                done => done,
            };
        }
        let done = match wait {
            Wait::Session | Wait::Project { .. } => !workspace::restore_in_progress(self),
        };
        if !done {
            return (started.elapsed() > WAIT_LIMIT).then(|| {
                Err("The window did not finish in ten minutes; check gui_state".to_owned())
            });
        }
        Some(Ok(match wait {
            Wait::Project { .. } => unreachable!("handled above"),
            Wait::Session => {
                let state = self.remote_state(bridge);
                json!({
                    "restored":true,
                    "datasets":state["datasets"],
                    "measurements":self.workspace.measurements.len(),
                    "layout":ops::layout(self),
                    "warning":self.error_message,
                })
            }
        }))
    }
    /// Starts hashing every dataset that has no stable id yet and collects finished hashes.
    fn refresh_stable_ids(&self, bridge: &mut GuiBridge) {
        bridge.stable.retain(|id, _| self.files.contains_key(id));
        for (id, file) in &self.files {
            if file.is_loading || bridge.stable.contains_key(id) {
                continue;
            }
            let path = identity_path(file);
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(
                    crate::project::source_identity(&path)
                        .map(|(hash, _)| hash)
                        .map_err(|e| e.message),
                );
            });
            bridge.stable.insert(*id, StableState::Hashing(receiver));
        }
        for state in bridge.stable.values_mut() {
            let next = match state {
                StableState::Hashing(receiver) => match receiver.try_recv() {
                    Ok(Ok(hash)) => Some(StableState::Ready(hash)),
                    Ok(Err(error)) => Some(StableState::Failed(error)),
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        Some(StableState::Failed("the hashing worker stopped".into()))
                    }
                },
                _ => None,
            };
            if let Some(next) = next {
                *state = next;
            }
        }
    }
    /// Replaces every stable id of `command` by the window's number for that dataset.
    fn resolve_dataset_refs(
        &self,
        command: &mut Command,
        bridge: &GuiBridge,
    ) -> Result<(), String> {
        for reference in command.dataset_refs_mut() {
            if let DatasetRef::Stable(text) = reference {
                let index = self.resolve_stable(text, bridge)?;
                *reference = DatasetRef::Index(index);
            }
        }
        Ok(())
    }
    fn resolve_stable(&self, text: &str, bridge: &GuiBridge) -> Result<usize, String> {
        let trimmed = text.trim();
        let bare = trimmed.strip_prefix("sha256:").unwrap_or(trimmed);
        let wanted = match uuid::Uuid::parse_str(bare) {
            Ok(uuid) => self
                .project
                .source_sha256(uuid)
                .ok_or_else(|| format!("The open project has no dataset {uuid}"))?,
            Err(_) => bare.to_ascii_lowercase(),
        };
        let mut found: Vec<usize> = bridge
            .stable
            .iter()
            .filter(|(_, state)| matches!(state, StableState::Ready(hash) if *hash == wanted))
            .map(|(id, _)| *id)
            .collect();
        found.sort_unstable();
        match found.as_slice() {
            [one] => Ok(*one),
            [] => Err(format!(
                "No open dataset has stable id {text}. Call list_datasets: a dataset gets its stable id a moment after it opens, and only datasets that are open can be named."
            )),
            many => Err(format!(
                "Stable id {text} matches {} open datasets ({many:?}): the same file is open more than once. Use dataset_id.",
                many.len()
            )),
        }
    }
    /// Applies what the server threads tell the window: activity of the headless engine tools
    /// and finished analyses. Neither can be sent by a client.
    fn drain_internal(&mut self, bridge: &mut GuiBridge) {
        use crate::mcp::{Internal, Route};
        while let Ok(event) = bridge.internal.try_recv() {
            match event {
                Internal::Activity(mut entry) => {
                    entry["sequence"] = json!(bridge.history.len());
                    bridge.history.push(entry);
                }
                Internal::Analysis { job_id, response } => {
                    let outcome = serde_json::from_value::<crate::engine::Response>(*response)
                        .map_err(|e| e.to_string())
                        .and_then(|response| {
                            super::project_workbench::route_result(self, response)
                        });
                    let (route, entry) = match outcome {
                        Ok(Some(tab)) => (
                            Route::Shown { tab: tab.into() },
                            json!({"request":{"operation":"show_analysis_result","parameters":{"job_id":job_id,"tab":tab}},"success":true}),
                        ),
                        Ok(None) => (
                            Route::NotShown {
                                reason: "This kind of result has no view in the window; read it with analysis_result".into(),
                            },
                            Value::Null,
                        ),
                        Err(error) => (
                            Route::NotShown { reason: error.clone() },
                            json!({"request":{"operation":"show_analysis_result","parameters":{"job_id":job_id}},"success":false,"error":error}),
                        ),
                    };
                    if let Ok(mut routes) = bridge.routed.lock() {
                        routes.insert(job_id, route);
                    }
                    if !entry.is_null() {
                        let mut entry = entry;
                        entry["sequence"] = json!(bridge.history.len());
                        bridge.history.push(entry);
                    }
                }
            }
        }
    }
    fn reply_remote(
        result: Result<CallToolResult, String>,
        request: Value,
        reply: tokio::sync::oneshot::Sender<CallToolResult>,
        bridge: &mut GuiBridge,
    ) {
        let response = match result {
            Ok(result) => {
                bridge.history.push(
                    json!({"sequence":bridge.history.len(),"request":request,"success":true}),
                );
                result
            }
            Err(message) => {
                bridge.history.push(json!({"sequence":bridge.history.len(),"request":request,"success":false,"error":message}));
                CallToolResult::structured_error(
                    json!({"code":"operation_failed","message":message}),
                )
            }
        };
        let _ = reply.send(response);
    }
    fn start_background(
        &self,
        command: &Command,
        b: &GuiBridge,
        ctx: &egui::Context,
    ) -> Result<Option<std::sync::mpsc::Receiver<Result<BackgroundResult, String>>>, String> {
        if !matches!(
            command,
            Command::OpenFiles { .. }
                | Command::ExtractChromatogram { .. }
                | Command::BuildBundle { .. }
                | Command::VerifyBundle { .. }
                | Command::ReprocessResult { .. }
        ) {
            return Ok(None);
        }
        self.remote_validate(command, b)?;
        let (sender, receiver) = std::sync::mpsc::channel();
        let context = ctx.clone();
        match command {
            Command::OpenFiles { paths } => {
                if paths.is_empty() || paths.len() > 64 {
                    return Err("Open 1..64 files per batch".into());
                }
                let paths = paths
                    .iter()
                    .map(|p| b.policy.source(p))
                    .collect::<Result<Vec<_>, _>>()?;
                let executable = self.msconvert_path.clone();
                std::thread::spawn(move || {
                    let _ =
                        sender.send(Ok(BackgroundResult::Opened(import_all(paths, executable))));
                    context.request_repaint();
                });
            }
            Command::ExtractChromatogram {
                dataset_id,
                display,
                ..
            } => {
                let dataset_id = &dataset_id.index();
                let file = self.files.get(dataset_id).ok_or("Unknown dataset")?;
                self.authorize_dataset(file, b)?;
                // The reader opens the file the window reads (the converted run for vendor data).
                let path = std::path::PathBuf::from(&file.path);
                let bounds = file.data.bounds;
                let stored = file.data.available_chromatograms.clone();
                let params = extraction_parameters(command.clone(), &bounds, &stored)?;
                let dataset_id = *dataset_id;
                let display = *display;
                std::thread::spawn(move || {
                    let result = (|| {
                        let mut data = MzData::new();
                        data.open_reader_only(&path).map_err(err)?;
                        data.bounds = bounds;
                        let (points, chrom) =
                            processing::process_chromatogram(&mut data, &params).map_err(err)?;
                        Ok(BackgroundResult::Extracted {
                            dataset_id,
                            display,
                            points,
                            chrom,
                            params,
                        })
                    })();
                    let _ = sender.send(result);
                    context.request_repaint();
                });
            }
            Command::BuildBundle {
                destination,
                project,
                include_sources,
                acknowledge_warnings,
                config,
            } => {
                let destination = b.policy.output(destination)?;
                let root = projects::target_root(self, b, project.as_deref())?;
                let config = config.clone().unwrap_or_default();
                let warnings = super::workflow::export_warnings(self);
                if !warnings.is_empty() && !acknowledge_warnings {
                    return Err(format!(
                        "{} warning(s) are open in the window: {}. Fix them, or repeat with acknowledge_warnings=true to export anyway; they are then saved in the bundle as review-warnings.json",
                        warnings.len(),
                        warnings.join("; ")
                    ));
                }
                let include_sources = *include_sources;
                std::thread::spawn(move || {
                    let result = (|| {
                        let mut summary =
                            crate::delivery::export(&root, &destination, &config, include_sources)
                                .map_err(|e| e.to_string())?;
                        if !warnings.is_empty() {
                            let text = serde_json::to_string_pretty(&warnings).map_err(err)?;
                            crate::delivery::add_bundle_file(
                                &destination,
                                "review-warnings.json",
                                text.as_bytes(),
                            )
                            .map_err(|e| e.to_string())?;
                        }
                        summary["warnings"] = json!(warnings);
                        summary["files"] = json!(
                            summary["files"].as_u64().unwrap_or(0)
                                + u64::from(!warnings.is_empty())
                        );
                        Ok(BackgroundResult::Value(summary))
                    })();
                    let _ = sender.send(result);
                    context.request_repaint();
                });
            }
            Command::VerifyBundle { path } => {
                let directory = b.policy.directory(path)?;
                std::thread::spawn(move || {
                    let result = crate::delivery::verify_bundle(&directory)
                        .map(BackgroundResult::Value)
                        .map_err(|e| e.to_string());
                    let _ = sender.send(result);
                    context.request_repaint();
                });
            }
            Command::ReprocessResult {
                result_id,
                project,
                include_response,
            } => {
                let id = uuid::Uuid::parse_str(result_id)
                    .map(crate::domain::ResultId)
                    .map_err(|e| format!("result_id is not a UUID: {e}"))?;
                let root = projects::target_root(self, b, project.as_deref())?;
                let include_response = *include_response;
                std::thread::spawn(move || {
                    let result = crate::delivery::reprocess(&root, id)
                        .map(|mut value| {
                            if !include_response {
                                if let Some(map) = value.as_object_mut() {
                                    map.remove("response");
                                }
                            }
                            BackgroundResult::Value(value)
                        })
                        .map_err(|e| e.to_string());
                    let _ = sender.send(result);
                    context.request_repaint();
                });
            }
            _ => unreachable!(),
        }
        Ok(Some(receiver))
    }
    fn apply_background(
        &mut self,
        result: BackgroundResult,
        pending: &PendingOperation,
        b: &mut GuiBridge,
    ) -> Result<CallToolResult, String> {
        let value = match result {
            BackgroundResult::Value(value) => value,
            BackgroundResult::Opened(imported) => self.register_imports(imported),
            BackgroundResult::Extracted {
                dataset_id,
                display,
                points,
                chrom,
                params,
            } => {
                let count = points.len();
                let unchanged = self.active_file_id == pending.selection
                    && self.user_input == pending.input
                    && !self.async_state.is_processing;
                let displayed = display && unchanged && self.files.contains_key(&dataset_id);
                if displayed {
                    self.remote_display(dataset_id, &points, &chrom, &params);
                }
                b.results
                    .insert(dataset_id, (points, chrom, params.clone()));
                json!({"dataset_id":dataset_id,"point_count":count,"parameters":params,"displayed":displayed,"display_conflict":display && !displayed,"retention_time_unit":"minutes","intensity_unit":"arbitrary units"})
            }
        };
        Ok(CallToolResult::structured(value))
    }
    /// File-level metadata of one dataset (the `dataset_metadata` tool and the dataset resource).
    fn dataset_metadata_value(&self, dataset_id: usize) -> Result<Value, String> {
        let f = self.files.get(&dataset_id).ok_or("Unknown dataset")?;
        let d = f.data.bounds;
        Ok(
            json!({"dataset_id":dataset_id,"path":f.path,"scan_count":d.scan_count,"retention_time_minutes":[d.min_rt,d.max_rt],"mz_range":[d.min_mz,d.max_mz],"scan_filters":f.data.available_scan_filters.iter().map(|(ms,p,pre,lo,hi)|json!({"ms_level":ms,"polarity":format!("{p:?}"),"precursor_mz":pre,"mz_range":[lo,hi]})).collect::<Vec<_>>(),"has_spectra":f.data.has_spectra(),"chromatograms":f.data.available_chromatograms}),
        )
    }
    /// Adds the imported runs to the window and reports one outcome per requested input.
    fn register_imports(
        &mut self,
        imported: Vec<(std::path::PathBuf, Result<Imported, String>)>,
    ) -> Value {
        let mut outcomes = vec![];
        for (path, result) in imported {
            let registered = result.and_then(|(runs, workspace)| {
                let mut datasets = vec![];
                for run in runs {
                    // A new window-owned reader; the worker's reader never crosses threads.
                    let mut data = MzData::new();
                    data.open_reader_only(&std::path::PathBuf::from(&run.path))
                        .map_err(err)?;
                    data.bounds = run.bounds;
                    data.available_scan_filters = run.scan_filters;
                    data.available_chromatograms = run.chromatograms;
                    data.available_acquisitions = run.acquisitions;
                    let id = self.next_file_id;
                    self.next_file_id += 1;
                    self.files.insert(
                        id,
                        OpenFile {
                            id,
                            name: run.name.clone(),
                            path: run.path,
                            data,
                            display: FileDisplaySettings {
                                color: next_color_for_index(id),
                                visible: true,
                            },
                            cache: FileCache {
                                // What the person chose; a converted run lives in a temporary
                                // folder outside every root, so access is checked against this.
                                source_path: Some(path.to_string_lossy().into_owned()),
                                import_workspace: workspace.clone(),
                                ..Default::default()
                            },
                            is_loading: false,
                        },
                    );
                    datasets.push(json!({"dataset_id":id,"name":run.name}));
                }
                Ok(datasets)
            });
            match registered {
                Ok(datasets) => outcomes.push(json!({
                    "path":path,
                    "dataset_id":datasets.first().map(|d| d["dataset_id"].clone()),
                    "datasets":datasets,
                    "vendor":!crate::import::is_mzml(&path),
                    "success":true
                })),
                Err(error) => outcomes.push(json!({"path":path,"success":false,"error":error})),
            }
        }
        self.invalid_file = if self.files.is_empty() {
            FileValidity::Invalid
        } else {
            FileValidity::Valid
        };
        json!({"files":outcomes})
    }
    /// Whether the dataset's source lies inside an authorised root. A converted vendor run is
    /// read from a temporary folder, so the file the person chose is what is checked.
    fn authorize_dataset(&self, file: &OpenFile, b: &GuiBridge) -> Result<(), String> {
        let source = file.cache.source_path.as_deref().unwrap_or(&file.path);
        b.policy.source(source).map(|_| ())
    }
    fn remote_validate(&self, c: &Command, b: &GuiBridge) -> Result<(), String> {
        let dataset = match c {
            Command::RetainTrace { dataset_id }
            | Command::RemoveTrace { dataset_id, .. }
            | Command::SetTraceVisibility { dataset_id, .. } => Some(dataset_id.index()),
            Command::SelectTrace { dataset_id, .. } => Some(dataset_id.index()),
            Command::ListScans { dataset_id, .. } => Some(dataset_id.index()),
            Command::DatasetMetadata { dataset_id }
            | Command::ScanMetadata { dataset_id, .. }
            | Command::SelectDataset { dataset_id }
            | Command::ExtractChromatogram { dataset_id, .. }
            | Command::ChromatogramData { dataset_id, .. }
            | Command::Spectrum { dataset_id, .. }
            | Command::SetVisibility { dataset_id, .. }
            | Command::Integrate { dataset_id, .. }
            | Command::ExportCsv { dataset_id, .. } => Some(dataset_id.index()),
            _ => None,
        };
        if let Some(id) = dataset {
            let f = self.files.get(&id).ok_or("Unknown dataset")?;
            self.authorize_dataset(f, b)?;
        }
        if let Command::StartQuantification { dataset_ids, .. } = c {
            for id in dataset_ids {
                self.authorize_dataset(self.files.get(&id.index()).ok_or("Unknown dataset")?, b)?;
            }
        }
        if matches!(c, Command::ApplyPreset { .. }) {
            for file in self.files.values() {
                self.authorize_dataset(file, b)?;
            }
        }
        let changes = matches!(
            c,
            Command::SelectDataset { .. }
                | Command::ClearDisplay { .. }
                | Command::ImportResults { .. }
                | Command::ShowWorkspace { .. }
                | Command::RemoveMeasurement { .. }
                | Command::ClearIntegration {}
                | Command::SetSettings { .. }
                | Command::LoadSession { .. }
                | Command::OpenProject { .. }
                | Command::RefreshProject {}
                | Command::OpenMethod { .. }
                | Command::LoadBatch { .. }
                | Command::SetLayout { .. }
                | Command::RetainTrace { .. }
                | Command::RemoveTrace { .. }
                | Command::SetTraceVisibility { .. }
                | Command::SetDisplay { .. }
                | Command::SelectTrace { .. }
                | Command::SetView { .. }
                | Command::SetVisibility { .. }
                | Command::UndoIntegration {}
                | Command::ApplyPreset { .. }
                | Command::StartQuantification { .. }
                | Command::CancelQuantification {}
                | Command::ReviewQuantification { .. }
                | Command::ResetQuantification { .. }
        ) || matches!(
            c,
            Command::ExtractChromatogram { display: true, .. }
                | Command::Spectrum { display: true, .. }
                | Command::Integrate { apply: true, .. }
        );
        if changes && !b.policy.allow_changes {
            return Err("This operation requires --allow-changes".into());
        }
        if changes && (self.async_state.is_processing || self.files.values().any(|f| f.is_loading))
        {
            return Err("GUI loading/processing in progress; retry when idle".into());
        }
        Ok(())
    }

    fn remote_state(&self, b: &GuiBridge) -> Value {
        let mut datasets: Vec<_> = self.files.values().map(|f| {
            let mut entry = json!({"dataset_id":f.id,"name":f.name,"path":f.path,"loading":f.is_loading,"visible":f.display.visible,"processing":f.cache.last_processing_params});
            let stable = b.stable.get(&f.id).map_or(json!({"stable_id":null,"stable_id_state":"computing"}), StableState::json);
            if let (Some(entry), Some(stable)) = (entry.as_object_mut(), stable.as_object()) {
                entry.extend(stable.clone());
            }
            entry
        }).collect();
        datasets.sort_by_key(|f| f["dataset_id"].as_u64());
        json!({"datasets":datasets,"active_dataset_id":self.active_file_id,"parameters":self.user_input,"chromatogram_bounds":self.workspace.bounds,"spectrum_bounds":self.workspace.spectrum_bounds,"overlay":self.workspace.overlay,"measurements":self.workspace.measurements,"layout":ops::layout(self),"retained_traces":self.workspace.traces.iter().map(|(id,traces)|json!({"dataset_id":id,"traces":traces.iter().enumerate().map(|(i,t)|json!({"trace_index":i,"name":t.name,"parameters":t.params,"point_count":t.points.len(),"visible":t.visible})).collect::<Vec<_>>()})).collect::<Vec<_>>(),"history":b.history,"units":{"retention_time":"minutes","mz":"m/z","intensity":"arbitrary units","area":"intensity*minutes"}})
    }
    fn remote_command(
        &mut self,
        c: Command,
        b: &mut GuiBridge,
        ctx: &egui::Context,
    ) -> Result<CallToolResult, String> {
        let mut c = c;
        self.resolve_dataset_refs(&mut c, b)?;
        self.remote_validate(&c, b)?;
        let value = match c {
            Command::ListMzmlFiles {
                directory,
                include_vendor,
            } => {
                let directory = b.policy.directory(&directory)?;
                let mut files = Vec::new();
                let mut vendor = Vec::new();
                for entry in std::fs::read_dir(&directory).map_err(err)? {
                    let path = entry.map_err(err)?.path();
                    if crate::import::is_mzml(&path) {
                        files.push(b.policy.source(&path.to_string_lossy())?);
                    } else if include_vendor && crate::mcp::is_vendor_dataset(&path) {
                        vendor.push(b.policy.source(&path.to_string_lossy())?);
                    }
                    if files.len() + vendor.len() > 10000 {
                        return Err("Directory contains more than 10000 datasets; use a smaller authorized directory".into());
                    }
                }
                files.sort();
                vendor.sort();
                let mut listing = json!({"directory":directory,"files":files,"recursive":false});
                if include_vendor {
                    listing["vendor_datasets"] = json!(vendor);
                    listing["msconvert_configured"] = json!(self.msconvert_path.is_some());
                }
                listing
            }
            Command::SetDisplay {
                compare_samples,
                intensity_scale,
                intensity_maximum,
                line_color,
                line_width,
            } => {
                let scale = match intensity_scale.as_deref() {
                    None => None,
                    Some("individual") => Some(workspace::IntensityScale::Individual),
                    Some("shared_highest") => Some(workspace::IntensityScale::SharedHighest),
                    Some("shared_custom") => Some(workspace::IntensityScale::SharedCustom),
                    _ => return Err("Unknown intensity scale".into()),
                };
                if intensity_maximum.is_some_and(|v| !v.is_finite() || v <= 0.0)
                    || line_width.is_some_and(|v| !v.is_finite() || !(0.5..=10.0).contains(&v))
                {
                    return Err(
                        "Positive finite intensity maximum and line width 0.5..10 required".into(),
                    );
                }
                let color = line_color
                    .map(|name| {
                        serde_json::from_value::<crate::plotting_parameters::LineColor>(
                            Value::String(name),
                        )
                        .map_err(err)
                    })
                    .transpose()?;
                if let Some(c) = color {
                    let id = self
                        .active_file_id
                        .ok_or("Select a dataset before changing color")?;
                    self.files
                        .get_mut(&id)
                        .ok_or("Unknown dataset")?
                        .display
                        .color = c;
                    self.user_input.line_color = c;
                }
                if let Some(c) = compare_samples {
                    self.workspace.view.compare_samples = c;
                }
                if let Some(s) = scale {
                    self.workspace.view.intensity_scale = s;
                }
                if let Some(m) = intensity_maximum {
                    self.workspace.view.intensity_maximum = m;
                }
                if let Some(w) = line_width {
                    self.user_input.line_width = w;
                }
                json!({"view":self.workspace.view,"line_width":self.user_input.line_width,"line_color":self.user_input.line_color})
            }
            Command::SelectTrace {
                dataset_id,
                trace_index,
            } => {
                let dataset_id = dataset_id.index();
                if self
                    .workspace
                    .traces
                    .get(&dataset_id)
                    .is_none_or(|t| trace_index >= t.len())
                {
                    return Err("Unknown retained trace index".into());
                }
                self.active_file_id = Some(dataset_id);
                workspace::select_trace(self, dataset_id, trace_index);
                let f = &self.files[&dataset_id];
                if let (Some(points), Some(chrom), Some(params)) = (
                    &f.cache.plot_data,
                    &f.cache.chromatogram,
                    &f.cache.last_processing_params,
                ) {
                    b.results
                        .insert(dataset_id, (points.clone(), chrom.clone(), params.clone()));
                }
                json!({"active_dataset_id":dataset_id,"parameters":self.user_input})
            }
            command @ Command::SetLayout { .. } => ops::set_layout(self, command)?,
            Command::RetainTrace { dataset_id } => ops::retain_trace(self, dataset_id.index())?,
            Command::RemoveTrace {
                dataset_id,
                trace_index,
            } => ops::remove_trace(self, dataset_id.index(), trace_index)?,
            Command::SetTraceVisibility {
                dataset_id,
                trace_index,
                visible,
            } => ops::set_trace_visibility(self, dataset_id.index(), trace_index, visible)?,
            Command::GetSettings {} => ops::settings(ctx, &self.updates),
            Command::SetSettings {
                theme,
                interface_size,
                mz_decimals,
                check_for_updates,
            } => ops::set_settings(
                ctx,
                &self.updates,
                theme.as_deref(),
                interface_size,
                mz_decimals,
                check_for_updates,
            )?,
            Command::RemoveMeasurement { index } => {
                let total = self.workspace.measurements.len();
                if index >= total {
                    return Err(format!(
                        "Unknown measurement {index}; the list holds {total}"
                    ));
                }
                let removed = self.workspace.measurements.remove(index);
                // Undo entries count the measurements that existed; they no longer match.
                b.undo.clear();
                json!({"removed":removed,"remaining":self.workspace.measurements.len()})
            }
            Command::ClearIntegration {} => {
                self.integration = Default::default();
                json!({"cleared":true})
            }
            Command::ShowWorkspace { workspace, stage } => {
                ops::show_workspace(self, ctx, &workspace, stage.as_deref())?
            }
            Command::ExportResults { kind, path } => files::export_results(self, b, &kind, &path)?,
            Command::ImportResults { kind, path } => files::import_results(self, b, &kind, &path)?,
            Command::ClearDisplay { what } => ops::clear_display(self, &what)?,
            Command::ExportMeasurements { path } => {
                let path = b.policy.output_as(&path, "csv")?;
                let csv = workspace::measurements_csv_text(self);
                crate::atomic_write::write_atomic_new(&path, &csv).map_err(err)?;
                json!({"path":path,"measurements":self.workspace.measurements.len()})
            }
            Command::ListResources {} => resources::list(self, b),
            Command::ReadResource { uri } => resources::read(self, b, &uri)?,
            Command::ListPresets { directory } => files::list_presets(self, b, directory)?,
            Command::GetPreset { source } => files::get_preset(b, &source)?,
            Command::SavePreset { path, toml } => files::save_preset(self, b, &path, &toml)?,
            Command::SaveMethod { path, method_toml } => {
                files::save_method(self, b, &path, method_toml.as_deref())?
            }
            Command::OpenMethod {
                path,
                discard_unsaved,
            } => files::open_method(self, b, &path, discard_unsaved)?,
            Command::SaveBatch { path } => files::save_batch(self, b, &path)?,
            Command::LoadBatch { path } => files::load_batch(self, b, &path)?,
            Command::SaveSession { path } => files::save_session(self, ctx, b, &path)?,
            Command::BuildBundle { .. }
            | Command::VerifyBundle { .. }
            | Command::ReprocessResult { .. }
            | Command::LoadSession { .. }
            | Command::CreateProject { .. }
            | Command::OpenProject { .. }
            | Command::SaveProject {}
            | Command::RefreshProject {} => {
                return Err("This operation replies when the window has finished, so it cannot run inside a batch; call it on its own".into())
            }
            Command::ListScans {
                dataset_id,
                offset,
                limit,
            } => {
                let dataset_id = dataset_id.index();
                page(offset, limit)?;
                if limit > 1000 {
                    return Err("Scan metadata page limit is 1000".into());
                }
                let f = self.files.get_mut(&dataset_id).ok_or("Unknown dataset")?;
                let count = f.data.bounds.scan_count;
                let scans = (offset..offset.saturating_add(limit).min(count))
                    .map(|i| f.data.scan_metadata_structured(i).map_err(err))
                    .collect::<Result<Vec<_>, _>>()?;
                json!({"dataset_id":dataset_id,"total":count,"offset":offset,"scans":scans})
            }
            Command::QuantificationTrace {
                result_index,
                offset,
                limit,
            } => {
                page(offset, limit)?;
                super::quant::remote_trace(self, result_index, offset, limit)?
            }
            Command::ExportQuantification { path } => {
                let path = b.policy.output(&path)?;
                let csv = super::quant::remote_csv(self)?;
                crate::atomic_write::write_atomic_new(&path, csv).map_err(err)?;
                json!({"path":path})
            }
            Command::ResetQuantification { result_index } => {
                super::quant::remote_reset(self, result_index)?;
                super::quant::remote_status(self)
            }
            Command::Batch { commands } => {
                if commands.is_empty()
                    || commands.len() > 64
                    || commands.iter().any(|c| matches!(c, Command::Batch { .. }))
                {
                    return Err("Batch requires 1..64 operations without nested batches".into());
                }
                let mut results = Vec::new();
                for command in commands {
                    let request = serde_json::to_value(&command).map_err(err)?;
                    let outcome = match self.remote_command(command, b, ctx) {
                        Ok(result) => json!({"request":request,"result":result}),
                        Err(error) => json!({"request":request,"error":error}),
                    };
                    b.history.push(outcome.clone());
                    results.push(outcome);
                }
                json!({"outcomes":results})
            }
            Command::ExportFigure { spectrum, path } => {
                let path = b.policy.output(&path)?;
                let svg = if spectrum {
                    workspace::spectrum_svg(self)
                } else {
                    workspace::figure_svg(self)
                }
                .ok_or("No displayed plot")?;
                crate::atomic_write::write_atomic_new(&path, svg).map_err(err)?;
                json!({"path":path,"format":"image/svg+xml"})
            }
            Command::ListDatasets {} => {
                json!({"datasets":self.remote_state(b)["datasets"],"active_dataset_id":self.active_file_id})
            }
            Command::GuiState {} => self.remote_state(b),
            Command::OpenFiles { paths } => {
                if paths.is_empty() || paths.len() > 64 {
                    return Err("Open 1..64 files per batch".into());
                }
                let paths = paths
                    .iter()
                    .map(|p| b.policy.source(p))
                    .collect::<Result<Vec<_>, _>>()?;
                let imported = import_all(paths, self.msconvert_path.clone());
                self.register_imports(imported)
            }
            Command::DatasetMetadata { dataset_id } => {
                self.dataset_metadata_value(dataset_id.index())?
            }
            Command::ScanMetadata { dataset_id, index } => {
                let dataset_id = dataset_id.index();
                json!({"dataset_id":dataset_id,"index":index,"metadata":self.files.get_mut(&dataset_id).ok_or("Unknown dataset")?.data.scan_metadata(index).map_err(err)?})
            }
            Command::SelectDataset { dataset_id } => {
                let dataset_id = dataset_id.index();
                let f = self.files.get(&dataset_id).ok_or("Unknown dataset")?;
                self.active_file_id = Some(dataset_id);
                self.user_input.file_path = Some(f.path.clone());
                self.user_input.line_color = f.display.color;
                workspace::activate_file(self, dataset_id);
                self.integration = Default::default();
                json!({"active_dataset_id":dataset_id})
            }
            ref command @ Command::ExtractChromatogram {
                ref dataset_id,
                display,
                ..
            } => {
                let dataset_id = dataset_id.index();
                let f = self.files.get_mut(&dataset_id).ok_or("Unknown dataset")?;
                let params = extraction_parameters(
                    command.clone(),
                    &f.data.bounds,
                    &f.data.available_chromatograms,
                )?;
                let (points, chrom) =
                    processing::process_chromatogram(&mut f.data, &params).map_err(err)?;
                let count = points.len();
                if display {
                    self.remote_display(dataset_id, &points, &chrom, &params);
                }
                b.results
                    .insert(dataset_id, (points, chrom, params.clone()));
                json!({"dataset_id":dataset_id,"point_count":count,"parameters":params,"displayed":display,"retention_time_unit":"minutes","intensity_unit":"arbitrary units","data_tool":"chromatogram_data"})
            }
            Command::ChromatogramData {
                dataset_id,
                offset,
                limit,
            } => {
                let dataset_id = dataset_id.index();
                page(offset, limit)?;
                let (points, _, params) = b
                    .results
                    .get(&dataset_id)
                    .ok_or("Extract a chromatogram first")?;
                json!({"dataset_id":dataset_id,"parameters":params,"total":points.len(),"offset":offset,"points":points.iter().skip(offset).take(limit).collect::<Vec<_>>(),"units":["minutes","arbitrary units"]})
            }
            Command::Spectrum {
                dataset_id,
                index,
                retention_time_minutes,
                display,
                offset,
                limit,
            } => {
                let dataset_id = dataset_id.index();
                page(offset, limit)?;
                if index.is_some() == retention_time_minutes.is_some() {
                    return Err("Specify exactly one of index or retention_time_minutes".into());
                }
                let f = self.files.get_mut(&dataset_id).ok_or("Unknown dataset")?;
                let index = if let Some(i) = index {
                    i
                } else {
                    let rt = retention_time_minutes.unwrap();
                    if !rt.is_finite()
                        || rt < f.data.bounds.min_rt as f64
                        || rt > f.data.bounds.max_rt as f64
                    {
                        return Err("Retention time outside dataset".into());
                    }
                    let chrom = b
                        .results
                        .get(&dataset_id)
                        .map(|r| &r.1)
                        .or(f.cache.chromatogram.as_ref())
                        .ok_or("Extract a filtered chromatogram before RT lookup")?;
                    processing::find_closest_spectrum_index(chrom, rt as f32)
                        .ok_or("No matching scan")?
                };
                let spectrum = f.data.get_mass_spectrum_by_index(index).map_err(err)?;
                let result = json!({"dataset_id":dataset_id,"index":spectrum.index,"retention_time_minutes":spectrum.retention_time,"total":spectrum.mz.len(),"offset":offset,"points":spectrum.mz.iter().zip(&spectrum.intensity).skip(offset).take(limit).map(|(mz,i)|json!([mz,i])).collect::<Vec<_>>()});
                if display {
                    self.user_input.retention_time_ms_spectrum = Some(spectrum.retention_time);
                    f.cache.mass_spectrum = Some(spectrum);
                    self.active_file_id = Some(dataset_id);
                    self.workspace.view.spectrum = true;
                }
                result
            }
            Command::SetView {
                retention_time_range,
                mz_range,
                overlay,
                reset,
            } => {
                let rt = retention_time_range.map(range).transpose()?;
                let mz = mz_range.map(range).transpose()?;
                if reset {
                    // The automatic view: what the Reset view command asks the plots for.
                    self.workspace.bounds = None;
                    self.workspace.spectrum_bounds = None;
                    self.workspace.apply_bounds = false;
                    self.workspace.apply_spectrum_bounds = false;
                    ctx.data_mut(|d| d.insert_temp(egui::Id::new("reset_chromatograms"), true));
                }
                if let Some([a, z]) = rt {
                    let y = self.workspace.bounds.unwrap_or([[0.0, 0.0], [1.0, 1.0]]);
                    self.workspace.bounds = Some([[a, y[0][1]], [z, y[1][1]]]);
                    self.workspace.apply_bounds = true;
                }
                if let Some([a, z]) = mz {
                    let y = self
                        .workspace
                        .spectrum_bounds
                        .unwrap_or([[0.0, 0.0], [1.0, 1.0]]);
                    self.workspace.spectrum_bounds = Some([[a, y[0][1]], [z, y[1][1]]]);
                    self.workspace.apply_spectrum_bounds = true;
                }
                if let Some(o) = overlay {
                    self.workspace.overlay = o;
                }
                json!({"chromatogram_bounds":self.workspace.bounds,"spectrum_bounds":self.workspace.spectrum_bounds,"overlay":self.workspace.overlay})
            }
            Command::SetVisibility {
                dataset_id,
                visible,
            } => {
                let dataset_id = dataset_id.index();
                self.files
                    .get_mut(&dataset_id)
                    .ok_or("Unknown dataset")?
                    .display
                    .visible = visible;
                json!({"dataset_id":dataset_id,"visible":visible})
            }
            Command::Integrate {
                dataset_id,
                start_minutes,
                end_minutes,
                apply,
            } => {
                let dataset_id = dataset_id.index();
                range([start_minutes, end_minutes])?;
                let (points, _, params) = b
                    .results
                    .get(&dataset_id)
                    .ok_or("Extract a chromatogram first")?;
                // One integration path: the shared engine's `integrate` operation, with its
                // bounds check, algorithm and result shape. The window only adds its own fields.
                let integration = engine_integration(points, start_minutes, end_minutes)?;
                let area = integration["area"]
                    .as_f64()
                    .ok_or("The engine returned no area")?;
                if apply {
                    let f = self.files.get(&dataset_id).ok_or("Unknown dataset")?;
                    if f.cache.last_processing_params.as_ref() != Some(params)
                        || self.active_file_id != Some(dataset_id)
                    {
                        return Err("Display this extraction before applying integration".into());
                    }
                    b.undo.push((
                        dataset_id,
                        self.workspace.measurements.len(),
                        self.integration.clone(),
                    ));
                    self.integration = IntegrationState {
                        start_rt: Some(start_minutes),
                        end_rt: Some(end_minutes),
                        result: Some(area),
                        start_intensity: Some(processing::interpolate_at(points, start_minutes)),
                        end_intensity: Some(processing::interpolate_at(points, end_minutes)),
                    };
                    self.workspace.measurements.push(workspace::Measurement {
                        file: f.name.clone(),
                        source: f.path.clone(),
                        trace: workspace::trace_name(params),
                        params: params.clone(),
                        start: start_minutes,
                        end: end_minutes,
                        area,
                    });
                }
                let mut result = integration;
                result["baseline"] = json!("straight chord");
                result["start_minutes"] = json!(start_minutes);
                result["end_minutes"] = json!(end_minutes);
                result["parameters"] = json!(params);
                result["applied"] = json!(apply);
                result["review_status"] = json!("unreviewed");
                result
            }
            Command::UndoIntegration {} => {
                let (id, len, previous) = b.undo.last().ok_or("No MCP integration to undo")?;
                if self.workspace.measurements.len() != len + 1 || self.active_file_id != Some(*id)
                {
                    return Err(
                        "Workspace changed after integration; undo would discard unrelated edits"
                            .into(),
                    );
                }
                let len = *len;
                let previous = previous.clone();
                b.undo.pop();
                self.workspace.measurements.truncate(len);
                self.integration = previous;
                json!({"undone":true})
            }
            Command::Measurements {} => json!({"measurements":self.workspace.measurements}),
            Command::PlotImage { spectrum } => {
                return self.remote_image(spectrum, b, ctx.style().visuals.dark_mode);
            }
            Command::ExportCsv {
                dataset_id,
                spectrum,
                path,
            } => {
                let dataset_id = dataset_id.index();
                let path = b.policy.output(&path)?;
                // Generate with the existing CSV exporters, then publish the finished file without
                // ever replacing one.
                let temp = tempfile::NamedTempFile::new().map_err(err)?;
                if spectrum {
                    let s = self
                        .files
                        .get(&dataset_id)
                        .and_then(|f| f.cache.mass_spectrum.as_ref())
                        .ok_or("Display a spectrum first")?;
                    crate::export::export_spectrum_csv(s, temp.path()).map_err(err)?;
                } else {
                    let r = b.results.get(&dataset_id).ok_or("Extract first")?;
                    crate::export::export_chromatogram_csv(&r.0, temp.path()).map_err(err)?;
                }
                let csv = std::fs::read(temp.path()).map_err(err)?;
                crate::atomic_write::write_atomic_new(&path, csv).map_err(err)?;
                json!({"path":path})
            }
            Command::ApplyPreset { toml } => {
                super::presets::apply_document(self, &toml)?;
                json!({"scheduled":true})
            }
            Command::StartQuantification {
                method_toml,
                dataset_ids,
            } => {
                super::quant::remote_start(
                    self,
                    &method_toml,
                    dataset_ids.iter().map(DatasetRef::index).collect(),
                )?;
                super::quant::remote_status(self)
            }
            Command::QuantificationStatus {} => {
                super::quant::poll(self, ctx);
                super::quant::remote_status(self)
            }
            Command::CancelQuantification {} => {
                super::quant::remote_cancel(self);
                json!({"cancellation_requested":true})
            }
            Command::ReviewQuantification {
                result_index,
                start_minutes,
                end_minutes,
                reviewed,
            } => {
                super::quant::remote_review(
                    self,
                    result_index,
                    start_minutes,
                    end_minutes,
                    reviewed,
                )?;
                super::quant::remote_status(self)
            }
        };
        Ok(CallToolResult::structured(value))
    }
    fn remote_display(
        &mut self,
        dataset_id: usize,
        points: &[[f64; 2]],
        chrom: &crate::parser::ChromatogramData,
        params: &ProcessingParams,
    ) {
        self.active_file_id = Some(dataset_id);
        self.apply_processing_result(processing::ProcessingResult::Success {
            file_id: dataset_id,
            plot_data: points.to_vec(),
            chromatogram: chrom.clone(),
            params: params.clone(),
        });
        self.user_input.file_path = Some(self.files[&dataset_id].path.clone());
        self.user_input.line_color = self.files[&dataset_id].display.color;
        self.remote_parameters(params);
    }
    fn remote_parameters(&mut self, p: &ProcessingParams) {
        self.user_input.plot_type = p.plot_type;
        self.user_input.ms_level = p.ms_level;
        self.user_input.polarity = p.polarity;
        self.user_input.smoothing = p.smoothing;
        self.user_input.precursor_mz = p.precursor_mz;
        self.user_input.acquisition = p.acquisition;
        self.user_input.native_chromatogram = p.native_chromatogram;
        self.user_input.range_enabled = p.mz_range.is_some();
        if let Some((a, z)) = p.mz_range {
            self.user_input.range_min = ValidatedInput::new(a);
            self.user_input.range_max = ValidatedInput::new(z);
        }
        if let Some(x) = p.xic_params {
            self.user_input.mass = ValidatedInput::new(x.mass());
            self.user_input.mass_tolerance = ValidatedInput::new(x.tolerance().value);
            self.user_input.tolerance_unit = x.tolerance().unit;
        }
        self.state_changed = StateChange::Unchanged;
    }
    fn remote_image(
        &mut self,
        spectrum: bool,
        b: &GuiBridge,
        dark: bool,
    ) -> Result<CallToolResult, String> {
        if spectrum
            && self
                .active_file_id
                .and_then(|i| self.files.get(&i))
                .and_then(|f| f.cache.mass_spectrum.as_ref())
                .is_none()
        {
            return Err("No displayed spectrum".into());
        }
        if !spectrum && !self.files.values().any(|f| f.cache.plot_data.is_some()) {
            return Err("No displayed chromatogram".into());
        }
        // Render a detached workspace using the exact plotting functions. Live UI is untouched.
        let mut snapshot = MzViewerApp {
            user_input: self.user_input.clone(),
            active_file_id: self.active_file_id,
            integration: self.integration.clone(),
            ..Default::default()
        };
        snapshot.workspace.bounds = self.workspace.bounds;
        snapshot.workspace.apply_bounds = true;
        snapshot.workspace.spectrum_bounds = self.workspace.spectrum_bounds;
        snapshot.workspace.apply_spectrum_bounds = true;
        snapshot.workspace.overlay = self.workspace.overlay;
        snapshot.workspace.view = self.workspace.view.clone();
        snapshot.workspace.traces = self.workspace.traces.clone();
        snapshot.workspace.hidden_current = self.workspace.hidden_current.clone();
        snapshot.workspace.names = self.workspace.names.clone();
        snapshot.workspace.order = self.workspace.order.clone();
        for (id, f) in &self.files {
            snapshot.files.insert(
                *id,
                OpenFile {
                    id: *id,
                    name: f.name.clone(),
                    path: f.path.clone(),
                    data: MzData::new(),
                    display: FileDisplaySettings {
                        color: f.display.color,
                        visible: f.display.visible,
                    },
                    cache: FileCache {
                        plot_data: f.cache.plot_data.clone(),
                        display_data: f.cache.display_data.clone(),
                        chromatogram: f.cache.chromatogram.clone(),
                        mass_spectrum: f.cache.mass_spectrum.clone(),
                        last_processing_params: f.cache.last_processing_params.clone(),
                        ..Default::default()
                    },
                    is_loading: false,
                },
            );
        }
        let context = egui::Context::default();
        super::workbench::configure(&context, dark);
        let size = egui::vec2(1000.0, 600.0);
        let mut outputs = vec![];
        for _ in 0..3 {
            outputs.push(context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if spectrum {
                            plotting::plot_mass_spectrum(&mut snapshot, ui);
                        } else {
                            plotting::plot_chromatogram(&mut snapshot, ui, ctx);
                        }
                    });
                },
            ));
        }
        let png = super::test_render::png(&context, outputs, size).map_err(err)?;
        use rmcp::model::ContentBlock;
        let mut result = CallToolResult::structured(
            json!({"state":self.remote_state(b),"width":1000,"height":600,"render":"egui plot, detached frame","spectrum":spectrum}),
        );
        result.content.push(ContentBlock::image(
            {
                use base64::Engine;
                base64::engine::general_purpose::STANDARD.encode(png)
            },
            "image/png",
        ));
        Ok(result)
    }
}

mod files;
mod ops;
mod projects;
#[cfg(test)]
mod protocol_tests;
mod resources;
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{AccessPolicy, McpServer};
    fn setup(changes: bool) -> (MzViewerApp, GuiBridge, egui::Context) {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let policy = AccessPolicy::new(vec![root], changes, true).unwrap();
        let ctx = egui::Context::default();
        let (_, bridge) = McpServer::new(ctx.clone(), policy).unwrap();
        (MzViewerApp::default(), bridge, ctx)
    }
    fn open(app: &mut MzViewerApp, b: &mut GuiBridge, ctx: &egui::Context) {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_file/data_dependent_02.mzML");
        let r = app
            .remote_command(
                Command::OpenFiles {
                    paths: vec![p.to_string_lossy().into_owned()],
                },
                b,
                ctx,
            )
            .unwrap();
        assert_eq!(r.structured_content.unwrap()["files"][0]["success"], true);
    }
    fn extract(display: bool) -> Command {
        Command::ExtractChromatogram {
            dataset_id: 0.into(),
            kind: "tic".into(),
            polarity: "positive".into(),
            ms_level: 1,
            smoothing: 0,
            mass: None,
            tolerance_ppm: None,
            tolerance_mda: None,
            mz_range: None,
            precursor_mz: None,
            acquisition: None,
            native_chromatogram: None,
            display,
        }
    }
    #[test]
    fn xic_tolerance_takes_ppm_or_mda_but_not_both() {
        use crate::validation::MassTolerance;
        let bounds = crate::validation::DataBounds::unrestricted();
        let xic = |ppm: Option<f64>, mda: Option<f64>| Command::ExtractChromatogram {
            dataset_id: 0.into(),
            kind: "xic".into(),
            polarity: "positive".into(),
            ms_level: 1,
            smoothing: 0,
            mass: Some(500.0),
            tolerance_ppm: ppm,
            tolerance_mda: mda,
            mz_range: None,
            precursor_mz: None,
            acquisition: None,
            native_chromatogram: None,
            display: false,
        };
        let tolerance = |ppm, mda| {
            extraction_parameters(xic(ppm, mda), &bounds, &[])
                .map(|p| p.xic_params.unwrap().tolerance())
        };
        assert_eq!(tolerance(Some(10.0), None), Ok(MassTolerance::ppm(10.0)));
        assert_eq!(tolerance(None, Some(5.0)), Ok(MassTolerance::mda(5.0)));
        assert!(tolerance(Some(10.0), Some(5.0)).is_err());
        assert!(tolerance(None, None).is_err());
        assert!(tolerance(None, Some(2000.0)).is_err());
        let mut tic = xic(None, Some(5.0));
        if let Command::ExtractChromatogram { kind, .. } = &mut tic {
            *kind = "tic".into();
        }
        assert!(extraction_parameters(tic, &bounds, &[]).is_err());
    }
    #[test]
    fn numerical_round_trip_and_read_only_ui_preservation() {
        let (mut app, mut b, ctx) = setup(false);
        open(&mut app, &mut b, &ctx);
        let input = app.user_input.clone();
        app.remote_command(extract(false), &mut b, &ctx).unwrap();
        assert!(app.user_input == input);
        assert_eq!(app.active_file_id, None);
        assert!(app.files[&0].cache.plot_data.is_none());
        let points = &b.results[&0].0;
        let mut reader = MzData::new();
        reader
            .open_msfile(&std::path::PathBuf::from(app.files[&0].path.clone()))
            .unwrap();
        let (expected, _) =
            processing::process_chromatogram(&mut reader, &b.results[&0].2).unwrap();
        assert_eq!(*points, expected);
        let r = app
            .remote_command(
                Command::Spectrum {
                    dataset_id: 0.into(),
                    index: None,
                    retention_time_minutes: Some(points[0][0]),
                    display: false,
                    offset: 0,
                    limit: 100,
                },
                &mut b,
                &ctx,
            )
            .unwrap();
        assert!(r.structured_content.unwrap()["total"].as_u64().unwrap() > 0);
        assert!(app.files[&0].cache.mass_spectrum.is_none());
    }
    #[test]
    fn integration_updates_and_undo_restores_previous_boundaries() {
        let (mut app, mut b, ctx) = setup(true);
        open(&mut app, &mut b, &ctx);
        app.remote_command(extract(true), &mut b, &ctx).unwrap();
        let points = &b.results[&0].0;
        let a = points[0][0];
        let z = points[points.len() - 1][0];
        let expected = processing::integrate_peak(points, a, z).unwrap();
        let c = Command::Integrate {
            dataset_id: 0.into(),
            start_minutes: a,
            end_minutes: z,
            apply: true,
        };
        app.remote_command(c.clone(), &mut b, &ctx).unwrap();
        assert_eq!(app.integration.result, Some(expected));
        app.remote_command(c, &mut b, &ctx).unwrap();
        assert_eq!(app.workspace.measurements.len(), 2);
        app.remote_command(Command::UndoIntegration {}, &mut b, &ctx)
            .unwrap();
        assert_eq!(app.workspace.measurements.len(), 1);
        assert_eq!(app.integration.result, Some(expected));
        app.remote_command(Command::UndoIntegration {}, &mut b, &ctx)
            .unwrap();
        assert!(app.workspace.measurements.is_empty());
        assert_eq!(app.integration.result, None);
    }
    #[test]
    fn images_are_png_and_capture_does_not_change_live_bounds() {
        let (mut app, mut b, ctx) = setup(true);
        open(&mut app, &mut b, &ctx);
        app.remote_command(extract(true), &mut b, &ctx).unwrap();
        app.remote_command(
            Command::Spectrum {
                dataset_id: 0.into(),
                index: Some(0),
                retention_time_minutes: None,
                display: true,
                offset: 0,
                limit: 100,
            },
            &mut b,
            &ctx,
        )
        .unwrap();
        let bounds = app.workspace.bounds;
        for spectrum in [false, true] {
            let result = app
                .remote_command(Command::PlotImage { spectrum }, &mut b, &ctx)
                .unwrap();
            let value = serde_json::to_value(result).unwrap();
            let content = value["content"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["type"] == "image")
                .unwrap();
            use base64::Engine;
            let png = base64::engine::general_purpose::STANDARD
                .decode(content["data"].as_str().unwrap())
                .unwrap();
            let image = image::load_from_memory(&png).unwrap();
            assert_eq!(image.width(), 1000);
            assert_eq!(image.height(), 600);
            let unique: std::collections::HashSet<_> =
                image.to_rgba8().pixels().map(|p| p.0).collect();
            assert!(unique.len() > 100, "Plot must contain rendered content");
        }
        assert_eq!(app.workspace.bounds, bounds);
    }
    #[test]
    fn invalid_inputs_and_read_only_changes_are_rejected() {
        let (mut app, mut b, ctx) = setup(false);
        open(&mut app, &mut b, &ctx);
        assert!(app.remote_command(extract(true), &mut b, &ctx).is_err());
        assert!(app
            .remote_command(
                Command::ChromatogramData {
                    dataset_id: 0.into(),
                    offset: 0,
                    limit: 10001
                },
                &mut b,
                &ctx
            )
            .is_err());
        assert!(range([f64::NAN, 1.0]).is_err());
        assert!(range([2.0, 1.0]).is_err());
        assert!(app
            .remote_command(
                Command::Spectrum {
                    dataset_id: 0.into(),
                    index: Some(usize::MAX),
                    retention_time_minutes: None,
                    display: false,
                    offset: 0,
                    limit: 1
                },
                &mut b,
                &ctx
            )
            .is_err());
        assert!(app
            .remote_command(Command::OpenFiles { paths: vec![] }, &mut b, &ctx)
            .is_err());
    }
    #[test]
    fn batch_records_partial_failure_and_preserves_successes() {
        let (mut app, mut b, ctx) = setup(false);
        open(&mut app, &mut b, &ctx);
        let r = app
            .remote_command(
                Command::Batch {
                    commands: vec![
                        extract(false),
                        Command::DatasetMetadata {
                            dataset_id: 999.into(),
                        },
                    ],
                },
                &mut b,
                &ctx,
            )
            .unwrap()
            .structured_content
            .unwrap();
        assert!(r["outcomes"][0]["result"].is_object());
        assert!(r["outcomes"][1]["error"].is_string());
        assert!(b.results.contains_key(&0));
    }
    #[test]
    fn queue_applies_on_owner_thread_and_records_errors() {
        let (mut app, b, ctx) = setup(false);
        let policy = b.policy.clone();
        let (server, bridge) = McpServer::new(ctx.clone(), policy).unwrap();
        app.attach_mcp(bridge);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let call = runtime.spawn(async move { server.dispatch(Command::GuiState {}).await });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !call.is_finished() && std::time::Instant::now() < deadline {
            app.poll_remote(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(call.is_finished());
        let result = runtime.block_on(call).unwrap();
        assert!(result.structured_content.is_some());
        assert_eq!(app.remote.as_ref().unwrap().history.len(), 1);
    }
    fn invoke(
        app: &mut MzViewerApp,
        server: &McpServer,
        runtime: &tokio::runtime::Runtime,
        ctx: &egui::Context,
        command: Command,
    ) -> CallToolResult {
        let server = server.clone();
        let call = runtime.spawn(async move { server.dispatch(command).await });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !call.is_finished() && std::time::Instant::now() < deadline {
            app.poll_remote(ctx);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(call.is_finished(), "MCP request did not finish");
        runtime.block_on(call).unwrap()
    }
    #[test]
    fn asynchronous_loading_extraction_and_display_conflict() {
        let (mut app, b, ctx) = setup(true);
        let (server, bridge) = McpServer::new(ctx.clone(), b.policy).unwrap();
        app.attach_mcp(bridge);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test_file/data_dependent_02.mzML")
            .to_string_lossy()
            .into_owned();
        let opened = invoke(
            &mut app,
            &server,
            &runtime,
            &ctx,
            Command::OpenFiles { paths: vec![path] },
        );
        assert_eq!(
            opened.structured_content.unwrap()["files"][0]["success"],
            true
        );
        let result = invoke(&mut app, &server, &runtime, &ctx, extract(false));
        assert_ne!(result.is_error, Some(true));
        assert_eq!(app.active_file_id, None);
        assert!(app.files[&0].cache.plot_data.is_none());
        let s = server.clone();
        let call = runtime.spawn(async move { s.dispatch(extract(true)).await });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while app.remote.as_ref().unwrap().pending.is_none() && std::time::Instant::now() < deadline
        {
            app.poll_remote(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(app.remote.as_ref().unwrap().pending.is_some());
        app.user_input.smoothing = 3; // Local edit after worker submission.
        while !call.is_finished() && std::time::Instant::now() < deadline {
            app.poll_remote(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(call.is_finished());
        let result = runtime.block_on(call).unwrap().structured_content.unwrap();
        assert_eq!(result["display_conflict"], true);
        assert_eq!(result["displayed"], false);
        assert_eq!(app.user_input.smoothing, 3);
        let displayed = invoke(&mut app, &server, &runtime, &ctx, extract(true));
        assert_eq!(displayed.structured_content.unwrap()["displayed"], true);
        assert_eq!(app.active_file_id, Some(0));
        assert_eq!(app.user_input.smoothing, 0);
    }
    #[test]
    fn quantification_method_progress_manual_recalculation_and_reset() {
        let (mut app, mut b, ctx) = setup(true);
        open(&mut app, &mut b, &ctx);
        let bounds = app.files[&0].data.bounds;
        let method=format!("version = 1\nname = 'MCP fixture'\ndetection_smoothing = 0\nminimum_height = 0.0\nboundary_fraction = 0.05\n[[analytes]]\nexpected_rt = {}\nrt_window = [{}, {}]\n[analytes.extraction]\nname = 'target'\nacquisition = 'FS'\nkind = 'XIC'\nmass = 524.3\nppm = 10.0\npolarity = 'positive'\nsmoothing = 0\n",(bounds.min_rt+bounds.max_rt)/2.0,bounds.min_rt,bounds.max_rt);
        let started = app
            .remote_command(
                Command::StartQuantification {
                    method_toml: method,
                    dataset_ids: vec![0.into()],
                },
                &mut b,
                &ctx,
            )
            .unwrap()
            .structured_content
            .unwrap();
        assert_eq!(started["job_id"], "quantification:1");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let finished = loop {
            let state = app
                .remote_command(Command::QuantificationStatus {}, &mut b, &ctx)
                .unwrap()
                .structured_content
                .unwrap();
            if state["running"] == false {
                break state;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert_eq!(finished["completed"], 1);
        assert_eq!(finished["results"].as_array().unwrap().len(), 1);
        let trace = app
            .remote_command(
                Command::QuantificationTrace {
                    result_index: 0,
                    offset: 0,
                    limit: 10000,
                },
                &mut b,
                &ctx,
            )
            .unwrap()
            .structured_content
            .unwrap();
        let points: Vec<[f64; 2]> = serde_json::from_value(trace["points"].clone()).unwrap();
        assert!(points.len() >= 3);
        let a = points[0][0];
        let z = points[points.len() - 1][0];
        let expected = processing::integrate_peak(&points, a, z).unwrap();
        assert!(app
            .remote_command(
                Command::ReviewQuantification {
                    result_index: 0,
                    start_minutes: Some(a),
                    end_minutes: Some(z),
                    reviewed: true
                },
                &mut b,
                &ctx
            )
            .is_err());
        let reviewed = app
            .remote_command(
                Command::ReviewQuantification {
                    result_index: 0,
                    start_minutes: Some(a),
                    end_minutes: Some(z),
                    reviewed: false,
                },
                &mut b,
                &ctx,
            )
            .unwrap()
            .structured_content
            .unwrap();
        assert_eq!(
            reviewed["results"][0]["peak"]["area"].as_f64().unwrap(),
            expected
        );
        assert_eq!(reviewed["results"][0]["status"], "Manual");
        assert_eq!(
            reviewed["results"][0]["automatic"],
            finished["results"][0]["automatic"]
        );
        let reset = app
            .remote_command(
                Command::ResetQuantification { result_index: 0 },
                &mut b,
                &ctx,
            )
            .unwrap()
            .structured_content
            .unwrap();
        assert_eq!(
            reset["results"][0]["peak"],
            finished["results"][0]["automatic"]
        );
        assert!(super::super::quant::remote_csv(&app)
            .unwrap()
            .contains("method_toml"));
        app.remote_command(Command::CancelQuantification {}, &mut b, &ctx)
            .unwrap();
    }
    #[test]
    fn metadata_filter_validation_exports_and_existing_files_are_preserved() {
        let (mut app, mut b, ctx) = setup(true);
        open(&mut app, &mut b, &ctx);
        let data = app
            .remote_command(
                Command::ListScans {
                    dataset_id: 0.into(),
                    offset: 0,
                    limit: 2,
                },
                &mut b,
                &ctx,
            )
            .unwrap()
            .structured_content
            .unwrap();
        assert_eq!(data["scans"].as_array().unwrap().len(), 2);
        assert!(data["scans"][0]["ms_level"].is_number());
        assert!(data["scans"][0]["retention_time_minutes"].is_number());
        let mut command = extract(false);
        if let Command::ExtractChromatogram { mass, .. } = &mut command {
            *mass = Some(524.3);
        }
        assert!(app.remote_command(command, &mut b, &ctx).is_err());
        app.remote_command(extract(true), &mut b, &ctx).unwrap();
        let directory = tempfile::tempdir().unwrap();
        b.policy = AccessPolicy::new(
            vec![
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
                directory.path().to_owned(),
            ],
            true,
            true,
        )
        .unwrap();
        let target = directory
            .path()
            .join("trace.csv")
            .to_string_lossy()
            .into_owned();
        let command = Command::ExportCsv {
            dataset_id: 0.into(),
            spectrum: false,
            path: target.clone(),
        };
        app.remote_command(command.clone(), &mut b, &ctx).unwrap();
        let original = std::fs::read(&target).unwrap();
        assert!(original.len() > 100);
        assert!(app.remote_command(command, &mut b, &ctx).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), original);
        let svg = directory.path().join("trace.svg");
        app.remote_command(
            Command::ExportFigure {
                spectrum: false,
                path: svg.to_string_lossy().into_owned(),
            },
            &mut b,
            &ctx,
        )
        .unwrap();
        assert!(std::fs::read_to_string(svg).unwrap().contains("<svg"));
    }
}
