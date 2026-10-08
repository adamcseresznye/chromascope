//! Local MCP transport and typed, bounded GUI command bridge.
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, tag = "operation", rename_all = "snake_case")]
pub enum Command {
    ListMzmlFiles {
        directory: String,
    },
    SetDisplay {
        compare_samples: Option<bool>,
        intensity_scale: Option<String>,
        intensity_maximum: Option<f64>,
        line_color: Option<String>,
        line_width: Option<f32>,
    },
    SelectTrace {
        dataset_id: usize,
        trace_index: usize,
    },
    ListScans {
        dataset_id: usize,
        offset: usize,
        limit: usize,
    },
    QuantificationTrace {
        result_index: usize,
        offset: usize,
        limit: usize,
    },
    ExportQuantification {
        path: String,
    },
    ResetQuantification {
        result_index: usize,
    },
    Batch {
        commands: Vec<Command>,
    },
    ExportFigure {
        spectrum: bool,
        path: String,
    },
    ListDatasets {},
    GuiState {},
    OpenFiles {
        paths: Vec<String>,
    },
    DatasetMetadata {
        dataset_id: usize,
    },
    ScanMetadata {
        dataset_id: usize,
        index: usize,
    },
    SelectDataset {
        dataset_id: usize,
    },
    ExtractChromatogram {
        dataset_id: usize,
        kind: String,
        polarity: String,
        ms_level: u8,
        smoothing: u8,
        mass: Option<f64>,
        tolerance_ppm: Option<f64>,
        mz_range: Option<[f64; 2]>,
        precursor_mz: Option<f64>,
        acquisition: Option<String>,
        #[serde(default)]
        display: bool,
    },
    ChromatogramData {
        dataset_id: usize,
        offset: usize,
        limit: usize,
    },
    Spectrum {
        dataset_id: usize,
        index: Option<usize>,
        retention_time_minutes: Option<f64>,
        #[serde(default)]
        display: bool,
        offset: usize,
        limit: usize,
    },
    SetView {
        retention_time_range: Option<[f64; 2]>,
        mz_range: Option<[f64; 2]>,
        overlay: Option<bool>,
    },
    SetVisibility {
        dataset_id: usize,
        visible: bool,
    },
    Integrate {
        dataset_id: usize,
        start_minutes: f64,
        end_minutes: f64,
        #[serde(default)]
        apply: bool,
    },
    UndoIntegration {},
    Measurements {},
    PlotImage {
        spectrum: bool,
    },
    ExportCsv {
        dataset_id: usize,
        spectrum: bool,
        path: String,
    },
    ApplyPreset {
        toml: String,
    },
    StartQuantification {
        method_toml: String,
        dataset_ids: Vec<usize>,
    },
    QuantificationStatus {},
    CancelQuantification {},
    ReviewQuantification {
        result_index: usize,
        start_minutes: Option<f64>,
        end_minutes: Option<f64>,
        #[serde(default)]
        reviewed: bool,
    },
}
#[derive(Debug, Clone)]
pub struct AccessPolicy {
    roots: Vec<PathBuf>,
    pub allow_changes: bool,
    pub allow_exports: bool,
}
impl AccessPolicy {
    pub fn new(
        roots: Vec<PathBuf>,
        allow_changes: bool,
        allow_exports: bool,
    ) -> Result<Self, String> {
        if roots.is_empty() {
            return Err("At least one --allow-root is required".into());
        }
        let roots: Vec<PathBuf> = roots
            .into_iter()
            .map(|p| p.canonicalize().map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        if roots.iter().any(|p: &PathBuf| !p.is_dir()) {
            return Err("Authorized roots must be existing directories".into());
        }
        Ok(Self {
            roots,
            allow_changes,
            allow_exports,
        })
    }
    pub fn input(&self, path: &str) -> Result<PathBuf, String> {
        let path = Path::new(path).canonicalize().map_err(|e| e.to_string())?;
        if !self.roots.iter().any(|r| path.starts_with(r)) {
            return Err("Path is outside authorized roots".into());
        }
        if !path.is_file() || !crate::import::is_mzml(&path) {
            return Err("Only mzML/mzML.gz files are accepted".into());
        }
        Ok(path)
    }
    pub fn directory(&self, path: &str) -> Result<PathBuf, String> {
        let path = Path::new(path).canonicalize().map_err(|e| e.to_string())?;
        if !path.is_dir() || !self.roots.iter().any(|r| path.starts_with(r)) {
            return Err("Directory must be inside an authorized root".into());
        }
        Ok(path)
    }
    pub fn output(&self, path: &str) -> Result<PathBuf, String> {
        if !self.allow_exports {
            return Err("Exports require --allow-exports".into());
        }
        let path = Path::new(path);
        let parent = path
            .parent()
            .ok_or("Export needs a parent directory")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !self.roots.iter().any(|r| parent.starts_with(r)) {
            return Err("Export outside authorized roots".into());
        }
        let name = path.file_name().ok_or("Missing filename")?;
        let target = parent.join(name);
        if target.exists() {
            return Err("Export target already exists; overwriting is prohibited".into());
        }
        Ok(target)
    }
}
pub struct Envelope {
    pub command: Command,
    pub reply: oneshot::Sender<CallToolResult>,
}
pub struct GuiBridge {
    pub(crate) pending: Option<crate::gui::remote::PendingOperation>,
    pub receiver: mpsc::Receiver<Envelope>,
    pub policy: AccessPolicy,
    pub history: Vec<Value>,
    pub(crate) undo: Vec<(usize, usize, crate::gui::state::IntegrationState)>,
    pub results: std::collections::HashMap<
        usize,
        (
            Vec<[f64; 2]>,
            crate::parser::ChromatogramData,
            crate::processing::ProcessingParams,
        ),
    >,
}
#[derive(Clone)]
pub struct McpServer {
    sender: mpsc::SyncSender<Envelope>,
    context: egui::Context,
    tool_router: ToolRouter<Self>,
}
impl McpServer {
    pub fn new(context: egui::Context, policy: AccessPolicy) -> (Self, GuiBridge) {
        let (sender, receiver) = mpsc::sync_channel(16);
        (
            Self {
                sender,
                context,
                tool_router: Self::tool_router(),
            },
            GuiBridge {
                pending: None,
                receiver,
                policy,
                history: vec![],
                undo: vec![],
                results: Default::default(),
            },
        )
    }
    pub(crate) async fn dispatch(&self, command: Command) -> CallToolResult {
        if let Command::Batch { commands } = command {
            if commands.is_empty()
                || commands.len() > 64
                || commands.iter().any(|c| matches!(c, Command::Batch { .. }))
            {
                return CallToolResult::structured_error(
                    json!({"code":"invalid_batch","message":"Use 1..64 operations without nested batches"}),
                );
            }
            let mut outcomes = Vec::new();
            for command in commands {
                let request = serde_json::to_value(&command).unwrap_or(Value::Null);
                let result = self.dispatch_single(command).await;
                outcomes.push(json!({"request":request,"result":result}));
            }
            return CallToolResult::structured(json!({"outcomes":outcomes}));
        }
        self.dispatch_single(command).await
    }
    async fn dispatch_single(&self, command: Command) -> CallToolResult {
        let (reply, receive) = oneshot::channel();
        if self.sender.try_send(Envelope { command, reply }).is_err() {
            return CallToolResult::structured_error(
                json!({"code":"busy_or_closed", "message":"GUI command queue unavailable"}),
            );
        }
        self.context.request_repaint();
        receive.await.unwrap_or_else(|_| {
            CallToolResult::structured_error(
                json!({"code":"gui_closed","message":"GUI closed before command completed"}),
            )
        })
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenFilesArgs {
    pub paths: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DatasetMetadataArgs {
    pub dataset_id: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScanMetadataArgs {
    pub dataset_id: usize,
    pub index: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectDatasetArgs {
    pub dataset_id: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractChromatogramArgs {
    pub dataset_id: usize,
    pub kind: String,
    pub polarity: String,
    pub ms_level: u8,
    pub smoothing: u8,
    pub mass: Option<f64>,
    pub tolerance_ppm: Option<f64>,
    pub mz_range: Option<[f64; 2]>,
    pub precursor_mz: Option<f64>,
    pub acquisition: Option<String>,
    #[serde(default)]
    pub display: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChromatogramDataArgs {
    pub dataset_id: usize,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpectrumArgs {
    pub dataset_id: usize,
    pub index: Option<usize>,
    pub retention_time_minutes: Option<f64>,
    #[serde(default)]
    pub display: bool,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetViewArgs {
    pub retention_time_range: Option<[f64; 2]>,
    pub mz_range: Option<[f64; 2]>,
    pub overlay: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetVisibilityArgs {
    pub dataset_id: usize,
    pub visible: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IntegrateArgs {
    pub dataset_id: usize,
    pub start_minutes: f64,
    pub end_minutes: f64,
    #[serde(default)]
    pub apply: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportCsvArgs {
    pub dataset_id: usize,
    pub spectrum: bool,
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApplyPresetArgs {
    pub toml: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartQuantificationArgs {
    pub method_toml: String,
    pub dataset_ids: Vec<usize>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReviewQuantificationArgs {
    pub result_index: usize,
    pub start_minutes: Option<f64>,
    pub end_minutes: Option<f64>,
    #[serde(default)]
    pub reviewed: bool,
}

#[tool_router]
impl McpServer {
    #[tool(
        description = "List mzML/mzML.gz files directly inside an explicitly authorized directory, sorted by path. No recursive traversal; pass returned paths to open_files in batches of up to 64."
    )]
    async fn list_mzml_files(
        &self,
        Parameters(args): Parameters<ListMzmlFilesArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::ListMzmlFiles {
            directory: args.directory,
        })
        .await
    }
    #[tool(
        description = "Change supported GUI display settings. Intensity scale: individual/shared_highest/shared_custom; positive custom maximum. Line width 0.5..10; color from the GUI palette. compare_samples displays multiple datasets."
    )]
    async fn set_display(&self, Parameters(args): Parameters<SetDisplayArgs>) -> CallToolResult {
        self.dispatch(Command::SetDisplay {
            compare_samples: args.compare_samples,
            intensity_scale: args.intensity_scale,
            intensity_maximum: args.intensity_maximum,
            line_color: args.line_color,
            line_width: args.line_width,
        })
        .await
    }
    #[tool(
        description = "Select a retained chromatogram trace by its current index from gui_state, restoring its original extraction parameters and full-resolution data."
    )]
    async fn select_trace(&self, Parameters(args): Parameters<SelectTraceArgs>) -> CallToolResult {
        self.dispatch(Command::SelectTrace {
            dataset_id: args.dataset_id,
            trace_index: args.trace_index,
        })
        .await
    }

    #[tool(
        description = "Page acquisition scan metadata, including native scan IDs, retention times in minutes, MS level, polarity, precursor ions, isolation windows, activation and scan windows. Limit 1..1000."
    )]
    async fn list_scans(&self, Parameters(args): Parameters<ListScansArgs>) -> CallToolResult {
        self.dispatch(Command::ListScans {
            dataset_id: args.dataset_id,
            offset: args.offset,
            limit: args.limit,
        })
        .await
    }
    #[tool(
        description = "Retrieve unsmoothed full-resolution XIC data for one batch quantification result, in bounded pages. Quantitative areas use this data, independently of detection smoothing."
    )]
    async fn quantification_trace(
        &self,
        Parameters(args): Parameters<QuantificationTraceArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::QuantificationTrace {
            result_index: args.result_index,
            offset: args.offset,
            limit: args.limit,
        })
        .await
    }
    #[tool(
        description = "Export batch quantification CSV using the existing exporter, including method snapshots, current results, diagnostics and review status. Requires authorized export path; refuses overwrite."
    )]
    async fn export_quantification(
        &self,
        Parameters(args): Parameters<ExportQuantificationArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::ExportQuantification { path: args.path })
            .await
    }
    #[tool(
        description = "Rollback one batch result to its original automatically detected peak and original status; requires change authorization."
    )]
    async fn reset_quantification(
        &self,
        Parameters(args): Parameters<ResetQuantificationArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::ResetQuantification {
            result_index: args.result_index,
        })
        .await
    }
    #[tool(
        description = "Execute 1..64 operations sequentially with per-operation outcomes. No nested batches. Completed operations are retained on partial failure; this is not a transaction."
    )]
    async fn batch(&self, Parameters(args): Parameters<BatchArgs>) -> CallToolResult {
        self.dispatch(Command::Batch {
            commands: args.commands,
        })
        .await
    }
    #[tool(
        description = "Export the current chromatogram or spectrum visualization as SVG using the existing GUI exporter. Requires --allow-exports and a new file under an authorized root."
    )]
    async fn export_figure(
        &self,
        Parameters(args): Parameters<ExportFigureArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::ExportFigure {
            spectrum: args.spectrum,
            path: args.path,
        })
        .await
    }
    #[tool(
        description = "Undo the last MCP integration when the workspace has not changed; restore previous boundaries and preserve original data."
    )]
    async fn undo_integration(&self) -> CallToolResult {
        self.dispatch(Command::UndoIntegration {}).await
    }
    #[tool(
        description = "List reproducible viewer integration measurements and extraction parameters."
    )]
    async fn measurements(&self) -> CallToolResult {
        self.dispatch(Command::Measurements {}).await
    }
    #[tool(
        description = "Request cancellation of the current batch quantification; completed results are retained."
    )]
    async fn cancel_quantification(&self) -> CallToolResult {
        self.dispatch(Command::CancelQuantification {}).await
    }

    #[tool(
        description = "Chromascope open_files operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn open_files(&self, Parameters(args): Parameters<OpenFilesArgs>) -> CallToolResult {
        let OpenFilesArgs { paths } = args;
        self.dispatch(Command::OpenFiles { paths }).await
    }

    #[tool(
        description = "Chromascope dataset_metadata operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn dataset_metadata(
        &self,
        Parameters(args): Parameters<DatasetMetadataArgs>,
    ) -> CallToolResult {
        let DatasetMetadataArgs { dataset_id } = args;
        self.dispatch(Command::DatasetMetadata { dataset_id }).await
    }

    #[tool(
        description = "Chromascope scan_metadata operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn scan_metadata(
        &self,
        Parameters(args): Parameters<ScanMetadataArgs>,
    ) -> CallToolResult {
        let ScanMetadataArgs { dataset_id, index } = args;
        self.dispatch(Command::ScanMetadata { dataset_id, index })
            .await
    }

    #[tool(
        description = "Chromascope select_dataset operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn select_dataset(
        &self,
        Parameters(args): Parameters<SelectDatasetArgs>,
    ) -> CallToolResult {
        let SelectDatasetArgs { dataset_id } = args;
        self.dispatch(Command::SelectDataset { dataset_id }).await
    }

    #[tool(
        description = "Chromascope extract_chromatogram operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn extract_chromatogram(
        &self,
        Parameters(args): Parameters<ExtractChromatogramArgs>,
    ) -> CallToolResult {
        let ExtractChromatogramArgs {
            dataset_id,
            kind,
            polarity,
            ms_level,
            smoothing,
            mass,
            tolerance_ppm,
            mz_range,
            precursor_mz,
            acquisition,
            display,
        } = args;
        self.dispatch(Command::ExtractChromatogram {
            dataset_id,
            kind,
            polarity,
            ms_level,
            smoothing,
            mass,
            tolerance_ppm,
            mz_range,
            precursor_mz,
            acquisition,
            display,
        })
        .await
    }

    #[tool(
        description = "Chromascope chromatogram_data operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn chromatogram_data(
        &self,
        Parameters(args): Parameters<ChromatogramDataArgs>,
    ) -> CallToolResult {
        let ChromatogramDataArgs {
            dataset_id,
            offset,
            limit,
        } = args;
        self.dispatch(Command::ChromatogramData {
            dataset_id,
            offset,
            limit,
        })
        .await
    }

    #[tool(
        description = "Chromascope spectrum operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn spectrum(&self, Parameters(args): Parameters<SpectrumArgs>) -> CallToolResult {
        let SpectrumArgs {
            dataset_id,
            index,
            retention_time_minutes,
            display,
            offset,
            limit,
        } = args;
        self.dispatch(Command::Spectrum {
            dataset_id,
            index,
            retention_time_minutes,
            display,
            offset,
            limit,
        })
        .await
    }

    #[tool(
        description = "Chromascope set_view operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn set_view(&self, Parameters(args): Parameters<SetViewArgs>) -> CallToolResult {
        let SetViewArgs {
            retention_time_range,
            mz_range,
            overlay,
        } = args;
        self.dispatch(Command::SetView {
            retention_time_range,
            mz_range,
            overlay,
        })
        .await
    }

    #[tool(
        description = "Chromascope set_visibility operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn set_visibility(
        &self,
        Parameters(args): Parameters<SetVisibilityArgs>,
    ) -> CallToolResult {
        let SetVisibilityArgs {
            dataset_id,
            visible,
        } = args;
        self.dispatch(Command::SetVisibility {
            dataset_id,
            visible,
        })
        .await
    }

    #[tool(
        description = "Chromascope integrate operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn integrate(&self, Parameters(args): Parameters<IntegrateArgs>) -> CallToolResult {
        let IntegrateArgs {
            dataset_id,
            start_minutes,
            end_minutes,
            apply,
        } = args;
        self.dispatch(Command::Integrate {
            dataset_id,
            start_minutes,
            end_minutes,
            apply,
        })
        .await
    }

    #[tool(
        description = "Chromascope export_csv operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn export_csv(&self, Parameters(args): Parameters<ExportCsvArgs>) -> CallToolResult {
        let ExportCsvArgs {
            dataset_id,
            spectrum,
            path,
        } = args;
        self.dispatch(Command::ExportCsv {
            dataset_id,
            spectrum,
            path,
        })
        .await
    }

    #[tool(
        description = "Chromascope apply_preset operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn apply_preset(&self, Parameters(args): Parameters<ApplyPresetArgs>) -> CallToolResult {
        let ApplyPresetArgs { toml } = args;
        self.dispatch(Command::ApplyPreset { toml }).await
    }

    #[tool(
        description = "Chromascope start_quantification operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn start_quantification(
        &self,
        Parameters(args): Parameters<StartQuantificationArgs>,
    ) -> CallToolResult {
        let StartQuantificationArgs {
            method_toml,
            dataset_ids,
        } = args;
        self.dispatch(Command::StartQuantification {
            method_toml,
            dataset_ids,
        })
        .await
    }

    #[tool(
        description = "Chromascope review_quantification operation. Retention time in minutes, m/z tolerance in ppm; bounded data pages up to 10000. Launch policy controls changes and exports."
    )]
    async fn review_quantification(
        &self,
        Parameters(args): Parameters<ReviewQuantificationArgs>,
    ) -> CallToolResult {
        let ReviewQuantificationArgs {
            result_index,
            start_minutes,
            end_minutes,
            reviewed,
        } = args;
        self.dispatch(Command::ReviewQuantification {
            result_index,
            start_minutes,
            end_minutes,
            reviewed,
        })
        .await
    }
    #[tool(
        description = "Execute a typed Chromascope operation. Units: retention time minutes, tolerance ppm, intensity arbitrary units, integrated area intensity*minutes. Read-only by default; display/apply changes require launch authorization. Supports datasets, numerical retrieval, spectra, integration, plots, presets, existing batch quantification and review."
    )]
    async fn workbench(&self, Parameters(args): Parameters<WorkbenchArgs>) -> CallToolResult {
        self.dispatch(args.command).await
    }
    #[tool(
        description = "List live loaded datasets, stable IDs, loading status and active selection."
    )]
    async fn list_datasets(&self) -> CallToolResult {
        self.dispatch(Command::ListDatasets {}).await
    }
    #[tool(
        description = "Inspect current GUI parameters, plot bounds, retained traces, measurements, and operation history."
    )]
    async fn gui_state(&self) -> CallToolResult {
        self.dispatch(Command::GuiState {}).await
    }
    #[tool(
        description = "Return current chromatogram plot as MCP PNG image with structured state. Uses the GUI's plotting code."
    )]
    async fn chromatogram_image(&self) -> CallToolResult {
        self.dispatch(Command::PlotImage { spectrum: false }).await
    }
    #[tool(
        description = "Return current mass spectrum plot as MCP PNG image with structured state. Uses the GUI's plotting code."
    )]
    async fn spectrum_image(&self) -> CallToolResult {
        self.dispatch(Command::PlotImage { spectrum: true }).await
    }
    #[tool(
        description = "Poll existing batch quantification progress and reproducible results, including automatic and reviewed boundaries."
    )]
    async fn quantification_status(&self) -> CallToolResult {
        self.dispatch(Command::QuantificationStatus {}).await
    }
}
#[tool_handler(router = self.tool_router)]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("chromascope", env!("CARGO_PKG_VERSION")))
            .with_instructions("Use named tools or workbench command schemas for analytical actions. Inspect datasets first. Retrieve bounded numerical pages for quantitation; images aid visual review. Read-only unless explicitly enabled at launch. AI interpretations are external to measured results.")
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchArgs {
    pub commands: Vec<Command>,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportFigureArgs {
    pub spectrum: bool,
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkbenchArgs {
    pub command: Command,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListMzmlFilesArgs {
    pub directory: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListScansArgs {
    pub dataset_id: usize,
    pub offset: usize,
    pub limit: usize,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuantificationTraceArgs {
    pub result_index: usize,
    pub offset: usize,
    pub limit: usize,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportQuantificationArgs {
    pub path: String,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResetQuantificationArgs {
    pub result_index: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetDisplayArgs {
    pub compare_samples: Option<bool>,
    pub intensity_scale: Option<String>,
    pub intensity_maximum: Option<f64>,
    pub line_color: Option<String>,
    pub line_width: Option<f32>,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectTraceArgs {
    pub dataset_id: usize,
    pub trace_index: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn access_policy_denies_escape_and_overwrite() {
        let allowed = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let source = allowed.path().join("fixture.mzML");
        std::fs::write(&source, "test").unwrap();
        let policy = AccessPolicy::new(vec![allowed.path().to_owned()], false, true).unwrap();
        assert!(policy.input(source.to_str().unwrap()).is_ok());
        let outside = other.path().join("fixture.mzML");
        std::fs::write(&outside, "test").unwrap();
        assert!(policy.input(outside.to_str().unwrap()).is_err());
        assert!(policy.output(source.to_str().unwrap()).is_err());
        assert!(policy
            .output(other.path().join("out.csv").to_str().unwrap())
            .is_err());
        assert!(policy
            .input(allowed.path().join("../missing.mzML").to_str().unwrap())
            .is_err());
        assert!(AccessPolicy::new(vec![], false, false).is_err());
    }
    #[test]
    fn schemas_and_unknown_arguments() {
        assert!(serde_json::from_value::<Command>(
            json!({"operation":"list_datasets","surprise":true})
        )
        .is_err());
        assert!(
            serde_json::from_value::<ExtractChromatogramArgs>(json!({"dataset_id":0})).is_err()
        );
        let (_, bridge) = McpServer::new(
            egui::Context::default(),
            AccessPolicy::new(
                vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))],
                false,
                false,
            )
            .unwrap(),
        );
        assert!(!bridge.policy.allow_changes);
        let router = McpServer::tool_router();
        let tools = router.list_all();
        assert!(tools.len() >= 24);
        for tool in tools {
            assert!(tool.input_schema.contains_key("type"));
        }
    }
    #[test]
    fn protocol_initialize_discovery_and_invalid_tool_arguments() {
        use rmcp::ServiceExt;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let policy=AccessPolicy::new(vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))],false,false).unwrap();
            let (server,_bridge)=McpServer::new(egui::Context::default(),policy);
            let (client,transport)=tokio::io::duplex(65536);
            let task=tokio::spawn(async move {server.serve(transport).await.unwrap().waiting().await.unwrap();});
            let (read,mut write)=tokio::io::split(client);let mut read=BufReader::new(read);
            async fn rpc(write:&mut tokio::io::WriteHalf<tokio::io::DuplexStream>,read:&mut BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,v:Value)->Value {
                write.write_all(format!("{v}\n").as_bytes()).await.unwrap();let mut line=String::new();
                tokio::time::timeout(std::time::Duration::from_secs(5),read.read_line(&mut line)).await.unwrap().unwrap();serde_json::from_str(&line).unwrap()
            }
            let init=rpc(&mut write,&mut read,json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).await;
            assert!(init["result"]["capabilities"]["tools"].is_object());
            write.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n").await.unwrap();
            let discovered=rpc(&mut write,&mut read,json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}})).await;
            assert!(discovered["result"]["tools"].as_array().unwrap().len()>=24);
            let invalid=rpc(&mut write,&mut read,json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"spectrum","arguments":{"dataset_id":"bad"}}})).await;
            assert!(invalid["error"].is_object() || invalid["result"]["isError"]==true);
            drop(write);drop(read);task.await.unwrap();
        });
    }
}
