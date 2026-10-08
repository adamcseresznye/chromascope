//! Commands are applied exclusively on the egui owner thread.
use super::{plotting, state::*, workspace, MzViewerApp};
use crate::{
    mcp::{Command, GuiBridge},
    parser::MzData,
    plotting_parameters::PlotType,
    processing::{self, ProcessingParams},
    validation::XicParams,
};
use mzdata::spectrum::ScanPolarity;
use rmcp::model::CallToolResult;
use serde_json::{json, Value};

pub(crate) struct PendingOperation {
    receiver: std::sync::mpsc::Receiver<Result<BackgroundResult, String>>,
    reply: tokio::sync::oneshot::Sender<CallToolResult>,
    request: Value,
    selection: Option<usize>,
    input: UserInput,
}
enum BackgroundResult {
    Opened(Vec<(std::path::PathBuf, Result<processing::LoadedRun, String>)>),
    Extracted {
        dataset_id: usize,
        display: bool,
        points: Vec<[f64; 2]>,
        chrom: crate::parser::ChromatogramData,
        params: ProcessingParams,
    },
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
fn extraction_parameters(
    command: Command,
    bounds: &crate::validation::DataBounds,
) -> Result<ProcessingParams, String> {
    let Command::ExtractChromatogram {
        kind,
        polarity,
        ms_level,
        smoothing,
        mass,
        tolerance_ppm,
        mz_range,
        precursor_mz,
        acquisition,
        ..
    } = command
    else {
        return Err("Expected extraction request".into());
    };
    if ms_level == 0 || smoothing > 10 {
        return Err("MS level must be positive; smoothing must be 0..10".into());
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
        || precursor_mz.is_some_and(|v| !v.is_finite() || v <= 0.0)
    {
        return Err("Mass and tolerance must be finite; precursor must be positive".into());
    }
    if kind != PlotType::Xic && (mass.is_some() || tolerance_ppm.is_some()) {
        return Err("Mass and ppm are XIC parameters; omit them for TIC/BPC".into());
    }
    if kind == PlotType::Xic && mz_range.is_some() {
        return Err("XIC uses mass and ppm; mz_range applies to TIC/BPC".into());
    }
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
                XicParams::new(
                    mass.ok_or("XIC requires mass")?,
                    polarity,
                    tolerance_ppm.ok_or("XIC requires tolerance_ppm")?,
                    bounds,
                )
                .map_err(err)?,
            )
        } else {
            None
        },
        mz_range: mz_range.map(range).transpose()?.map(|[a, z]| (a, z)),
        precursor_mz,
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
        if let Some(pending) = bridge.pending.take() {
            let result = match pending.receiver.try_recv() {
                Ok(result) => result,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    bridge.pending = Some(pending);
                    self.remote = Some(bridge);
                    ctx.request_repaint_after(std::time::Duration::from_millis(50));
                    return;
                }
                Err(_) => Err("Analytical worker stopped before returning a result".into()),
            };
            if !pending.reply.is_closed() {
                let result = result.and_then(|r| self.apply_background(r, &pending, &mut bridge));
                Self::reply_remote(result, pending.request, pending.reply, &mut bridge);
            }
        }
        // One command per frame preserves fairness with local input.
        if let Ok(envelope) = bridge.receiver.try_recv() {
            if !envelope.reply.is_closed() {
                let command = envelope.command;
                let entry = serde_json::to_value(&command).unwrap_or(Value::Null);
                match self.start_background(&command, &bridge, ctx) {
                    Ok(Some(receiver)) => {
                        bridge.pending = Some(PendingOperation {
                            receiver,
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
            Command::OpenFiles { .. } | Command::ExtractChromatogram { .. }
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
                    .map(|p| b.policy.input(p))
                    .collect::<Result<Vec<_>, _>>()?;
                std::thread::spawn(move || {
                    let runs = paths
                        .into_iter()
                        .map(|path| {
                            let mut data = MzData::new();
                            let result = data
                                .open_msfile(&path)
                                .map(|data| processing::LoadedRun {
                                    name: path.file_name().unwrap().to_string_lossy().into_owned(),
                                    path: path.to_string_lossy().into_owned(),
                                    bounds: data.bounds,
                                    scan_filters: data.available_scan_filters.clone(),
                                })
                                .map_err(err);
                            (path, result)
                        })
                        .collect();
                    let _ = sender.send(Ok(BackgroundResult::Opened(runs)));
                    context.request_repaint();
                });
            }
            Command::ExtractChromatogram {
                dataset_id,
                display,
                ..
            } => {
                let file = self.files.get(dataset_id).ok_or("Unknown dataset")?;
                let path = b.policy.input(&file.path)?;
                let bounds = file.data.bounds;
                let params = extraction_parameters(command.clone(), &bounds)?;
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
            BackgroundResult::Opened(runs) => {
                let mut outcomes = vec![];
                for (path, result) in runs {
                    match result.and_then(|run| {
                        // A new GUI-owned reader; the worker's reader never crosses threads.
                        let mut data = MzData::new();
                        data.open_reader_only(&path).map_err(err)?;
                        data.bounds = run.bounds;
                        data.available_scan_filters = run.scan_filters;
                        let id = self.next_file_id;
                        self.next_file_id += 1;
                        self.files.insert(
                            id,
                            OpenFile {
                                id,
                                name: run.name,
                                path: run.path,
                                data,
                                display: FileDisplaySettings {
                                    color: next_color_for_index(id),
                                    visible: true,
                                },
                                cache: Default::default(),
                                is_loading: false,
                            },
                        );
                        Ok(id)
                    }) {
                        Ok(id) => {
                            outcomes.push(json!({"path":path,"dataset_id":id,"success":true}))
                        }
                        Err(error) => {
                            outcomes.push(json!({"path":path,"success":false,"error":error}))
                        }
                    }
                }
                self.invalid_file = if self.files.is_empty() {
                    FileValidity::Invalid
                } else {
                    FileValidity::Valid
                };
                json!({"files":outcomes})
            }
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
    fn remote_validate(&self, c: &Command, b: &GuiBridge) -> Result<(), String> {
        let dataset = match c {
            Command::SelectTrace { dataset_id, .. } => Some(*dataset_id),
            Command::ListScans { dataset_id, .. } => Some(*dataset_id),
            Command::DatasetMetadata { dataset_id }
            | Command::ScanMetadata { dataset_id, .. }
            | Command::SelectDataset { dataset_id }
            | Command::ExtractChromatogram { dataset_id, .. }
            | Command::ChromatogramData { dataset_id, .. }
            | Command::Spectrum { dataset_id, .. }
            | Command::SetVisibility { dataset_id, .. }
            | Command::Integrate { dataset_id, .. }
            | Command::ExportCsv { dataset_id, .. } => Some(*dataset_id),
            _ => None,
        };
        if let Some(id) = dataset {
            let f = self.files.get(&id).ok_or("Unknown dataset")?;
            b.policy.input(&f.path)?;
        }
        if let Command::StartQuantification { dataset_ids, .. } = c {
            for id in dataset_ids {
                b.policy
                    .input(&self.files.get(id).ok_or("Unknown dataset")?.path)?;
            }
        }
        if matches!(c, Command::ApplyPreset { .. }) {
            for file in self.files.values() {
                b.policy.input(&file.path)?;
            }
        }
        let changes = matches!(
            c,
            Command::SelectDataset { .. }
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
        let mut datasets: Vec<_> = self.files.values().map(|f| json!({"dataset_id":f.id,"name":f.name,"path":f.path,"loading":f.is_loading,"visible":f.display.visible,"processing":f.cache.last_processing_params})).collect();
        datasets.sort_by_key(|f| f["dataset_id"].as_u64());
        json!({"datasets":datasets,"active_dataset_id":self.active_file_id,"parameters":self.user_input,"chromatogram_bounds":self.workspace.bounds,"spectrum_bounds":self.workspace.spectrum_bounds,"overlay":self.workspace.overlay,"measurements":self.workspace.measurements,"retained_traces":self.workspace.traces.iter().map(|(id,traces)|json!({"dataset_id":id,"traces":traces.iter().map(|t|json!({"name":t.name,"parameters":t.params,"point_count":t.points.len(),"visible":t.visible})).collect::<Vec<_>>()})).collect::<Vec<_>>(),"history":b.history,"units":{"retention_time":"minutes","mz":"m/z","intensity":"arbitrary units","area":"intensity*minutes"}})
    }
    fn remote_command(
        &mut self,
        c: Command,
        b: &mut GuiBridge,
        ctx: &egui::Context,
    ) -> Result<CallToolResult, String> {
        self.remote_validate(&c, b)?;
        let value = match c {
            Command::ListMzmlFiles { directory } => {
                let directory = b.policy.directory(&directory)?;
                let mut files = Vec::new();
                for entry in std::fs::read_dir(&directory).map_err(err)? {
                    let path = entry.map_err(err)?.path();
                    if crate::import::is_mzml(&path) && path.is_file() {
                        files.push(b.policy.input(&path.to_string_lossy())?);
                        if files.len() > 10000 {
                            return Err("Directory contains more than 10000 mzML files; use a smaller authorized directory".into());
                        }
                    }
                }
                files.sort();
                json!({"directory":directory,"files":files,"recursive":false})
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
            Command::ListScans {
                dataset_id,
                offset,
                limit,
            } => {
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
                use std::io::Write;
                std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .map_err(err)?
                    .write_all(csv.as_bytes())
                    .map_err(err)?;
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
                use std::io::Write;
                std::fs::OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .open(&path)
                    .map_err(err)?
                    .write_all(svg.as_bytes())
                    .map_err(err)?;
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
                    .map(|p| b.policy.input(p))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut outcomes = vec![];
                for path in paths {
                    let mut data = MzData::new();
                    match data.open_msfile(&path) {
                        Ok(_) => {
                            let id = self.next_file_id;
                            self.next_file_id += 1;
                            let name = path.file_name().unwrap().to_string_lossy().into_owned();
                            self.files.insert(
                                id,
                                OpenFile {
                                    id,
                                    name,
                                    path: path.to_string_lossy().into_owned(),
                                    data,
                                    display: FileDisplaySettings {
                                        color: next_color_for_index(id),
                                        visible: true,
                                    },
                                    cache: Default::default(),
                                    is_loading: false,
                                },
                            );
                            outcomes.push(json!({"dataset_id":id,"path":path,"success":true}));
                        }
                        Err(e) => outcomes
                            .push(json!({"path":path,"success":false,"error":e.to_string()})),
                    }
                }
                self.invalid_file = if self.files.is_empty() {
                    FileValidity::Invalid
                } else {
                    FileValidity::Valid
                };
                json!({"files":outcomes})
            }
            Command::DatasetMetadata { dataset_id } => {
                let f = self.files.get(&dataset_id).ok_or("Unknown dataset")?;
                let d = f.data.bounds;
                json!({"dataset_id":dataset_id,"path":f.path,"scan_count":d.scan_count,"retention_time_minutes":[d.min_rt,d.max_rt],"mz_range":[d.min_mz,d.max_mz],"scan_filters":f.data.available_scan_filters.iter().map(|(ms,p,pre,lo,hi)|json!({"ms_level":ms,"polarity":format!("{p:?}"),"precursor_mz":pre,"mz_range":[lo,hi]})).collect::<Vec<_>>()})
            }
            Command::ScanMetadata { dataset_id, index } => {
                json!({"dataset_id":dataset_id,"index":index,"metadata":self.files.get_mut(&dataset_id).ok_or("Unknown dataset")?.data.scan_metadata(index).map_err(err)?})
            }
            Command::SelectDataset { dataset_id } => {
                let f = self.files.get(&dataset_id).ok_or("Unknown dataset")?;
                self.active_file_id = Some(dataset_id);
                self.user_input.file_path = Some(f.path.clone());
                self.user_input.line_color = f.display.color;
                workspace::activate_file(self, dataset_id);
                self.integration = Default::default();
                json!({"active_dataset_id":dataset_id})
            }
            command @ Command::ExtractChromatogram {
                dataset_id,
                display,
                ..
            } => {
                let f = self.files.get_mut(&dataset_id).ok_or("Unknown dataset")?;
                let params = extraction_parameters(command.clone(), &f.data.bounds)?;
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
            } => {
                let rt = retention_time_range.map(range).transpose()?;
                let mz = mz_range.map(range).transpose()?;
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
                range([start_minutes, end_minutes])?;
                let (points, _, params) = b
                    .results
                    .get(&dataset_id)
                    .ok_or("Extract a chromatogram first")?;
                if start_minutes < points.first().ok_or("Empty chromatogram")?[0]
                    || end_minutes > points.last().unwrap()[0]
                {
                    return Err("Integration bounds outside extracted trace".into());
                }
                let area =
                    processing::integrate_peak(points, start_minutes, end_minutes).map_err(err)?;
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
                json!({"area":area,"area_unit":"intensity*minutes","baseline":"straight chord","start_minutes":start_minutes,"end_minutes":end_minutes,"parameters":params,"applied":apply,"review_status":"unreviewed"})
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
                let path = b.policy.output(&path)?;
                // Generate with existing CSV exporters; create_new prevents a check/write overwrite race.
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
                let mut out = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(err)?;
                std::io::copy(
                    &mut std::fs::File::open(temp.path()).map_err(err)?,
                    &mut out,
                )
                .map_err(err)?;
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
                super::quant::remote_start(self, &method_toml, dataset_ids)?;
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
        self.user_input.range_enabled = p.mz_range.is_some();
        if let Some((a, z)) = p.mz_range {
            self.user_input.range_min = ValidatedInput::new(a);
            self.user_input.range_max = ValidatedInput::new(z);
        }
        if let Some(x) = p.xic_params {
            self.user_input.mass = ValidatedInput::new(x.mass());
            self.user_input.mass_tolerance = ValidatedInput::new(x.mass_tolerance());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::{AccessPolicy, McpServer};
    fn setup(changes: bool) -> (MzViewerApp, GuiBridge, egui::Context) {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let policy = AccessPolicy::new(vec![root], changes, true).unwrap();
        let ctx = egui::Context::default();
        let (_, bridge) = McpServer::new(ctx.clone(), policy);
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
            dataset_id: 0,
            kind: "tic".into(),
            polarity: "positive".into(),
            ms_level: 1,
            smoothing: 0,
            mass: None,
            tolerance_ppm: None,
            mz_range: None,
            precursor_mz: None,
            acquisition: None,
            display,
        }
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
                    dataset_id: 0,
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
            dataset_id: 0,
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
                dataset_id: 0,
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
                    dataset_id: 0,
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
                    dataset_id: 0,
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
                    commands: vec![extract(false), Command::DatasetMetadata { dataset_id: 999 }],
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
        let (server, bridge) = McpServer::new(ctx.clone(), policy);
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
        let (server, bridge) = McpServer::new(ctx.clone(), b.policy);
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
                    dataset_ids: vec![0],
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
                    dataset_id: 0,
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
            dataset_id: 0,
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
