//! Local MCP transport and typed, bounded GUI command bridge.
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
};
use tokio::sync::oneshot;

/// One dataset of the window: by the number it has in the running window, or by an id that
/// survives restarts.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum DatasetRef {
    /// The `dataset_id` number from `list_datasets`. Valid only while the window stays open:
    /// numbers are handed out in the order files are opened.
    Index(usize),
    /// The `stable_id` from `list_datasets`: `sha256:` plus the SHA-256 of the dataset's source
    /// file (the bare hex also works), or the UUID of a dataset of the open project. It names the
    /// same data after a restart, whatever number the window gave it.
    Stable(String),
}
/// Listed as one short inline type (a number or a string) instead of a definition repeated in
/// every tool that takes a dataset; list_datasets explains the two forms.
impl schemars::JsonSchema for DatasetRef {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DatasetRef".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": ["integer", "string"]
        })
    }
}
impl From<usize> for DatasetRef {
    fn from(index: usize) -> Self {
        Self::Index(index)
    }
}
impl DatasetRef {
    /// The window's number for this dataset. A stable id has no number until the window has
    /// resolved it, so it maps to a number no dataset has.
    pub fn index(&self) -> usize {
        match self {
            Self::Index(index) => *index,
            Self::Stable(_) => usize::MAX,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields, tag = "operation", rename_all = "snake_case")]
pub enum Command {
    ListMzmlFiles {
        directory: String,
        #[serde(default)]
        include_vendor: bool,
    },
    SetDisplay {
        compare_samples: Option<bool>,
        intensity_scale: Option<String>,
        intensity_maximum: Option<f64>,
        line_color: Option<String>,
        line_width: Option<f32>,
    },
    SelectTrace {
        dataset_id: DatasetRef,
        trace_index: usize,
    },
    ListScans {
        dataset_id: DatasetRef,
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
        dataset_id: DatasetRef,
    },
    ScanMetadata {
        dataset_id: DatasetRef,
        index: usize,
    },
    SelectDataset {
        dataset_id: DatasetRef,
    },
    ExtractChromatogram {
        dataset_id: DatasetRef,
        kind: String,
        polarity: String,
        ms_level: u8,
        smoothing: u8,
        mass: Option<f64>,
        tolerance_ppm: Option<f64>,
        #[serde(default)]
        tolerance_mda: Option<f64>,
        mz_range: Option<[f64; 2]>,
        precursor_mz: Option<f64>,
        acquisition: Option<String>,
        #[serde(default)]
        native_chromatogram: Option<usize>,
        #[serde(default)]
        display: bool,
    },
    ChromatogramData {
        dataset_id: DatasetRef,
        offset: usize,
        limit: usize,
    },
    Spectrum {
        dataset_id: DatasetRef,
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
        #[serde(default)]
        reset: bool,
    },
    SetVisibility {
        dataset_id: DatasetRef,
        visible: bool,
    },
    Integrate {
        dataset_id: DatasetRef,
        start_minutes: f64,
        end_minutes: f64,
        #[serde(default)]
        apply: bool,
    },
    UndoIntegration {},
    SetLayout {
        mode: Option<String>,
        auto_grid: Option<bool>,
        rows: Option<usize>,
        columns: Option<usize>,
        page: Option<usize>,
        show_files: Option<bool>,
        show_inspector: Option<bool>,
        show_spectrum: Option<bool>,
        focus: Option<bool>,
    },
    RetainTrace {
        dataset_id: DatasetRef,
    },
    RemoveTrace {
        dataset_id: DatasetRef,
        trace_index: Option<usize>,
    },
    SetTraceVisibility {
        dataset_id: DatasetRef,
        trace_index: Option<usize>,
        visible: bool,
    },
    RemoveMeasurement {
        index: usize,
    },
    ClearIntegration {},
    ShowWorkspace {
        workspace: String,
        stage: Option<String>,
    },
    ExportResults {
        kind: String,
        path: String,
    },
    ImportResults {
        kind: String,
        path: String,
    },
    ClearDisplay {
        what: String,
    },
    ExportMeasurements {
        path: String,
    },
    /// Read-only listing and reading of the MCP resources (not offered as tools).
    ListResources {},
    ReadResource {
        uri: String,
    },
    Measurements {},
    ListPresets {
        directory: Option<String>,
    },
    GetPreset {
        source: String,
    },
    SavePreset {
        path: String,
        toml: String,
    },
    GetSettings {},
    SetSettings {
        theme: Option<String>,
        interface_size: Option<u16>,
        mz_decimals: Option<u8>,
    },
    BuildBundle {
        destination: String,
        project: Option<String>,
        #[serde(default)]
        include_sources: bool,
        #[serde(default)]
        acknowledge_warnings: bool,
        config: Option<crate::delivery::Config>,
    },
    VerifyBundle {
        path: String,
    },
    ReprocessResult {
        result_id: String,
        project: Option<String>,
        #[serde(default)]
        include_response: bool,
    },
    CreateProject {
        path: String,
    },
    OpenProject {
        path: String,
    },
    SaveProject {},
    RefreshProject {},
    SaveMethod {
        path: String,
        method_toml: Option<String>,
    },
    OpenMethod {
        path: String,
        #[serde(default)]
        discard_unsaved: bool,
    },
    SaveBatch {
        path: String,
    },
    LoadBatch {
        path: String,
    },
    SaveSession {
        path: String,
    },
    LoadSession {
        path: String,
    },
    PlotImage {
        spectrum: bool,
    },
    ExportCsv {
        dataset_id: DatasetRef,
        spectrum: bool,
        path: String,
    },
    ApplyPreset {
        toml: String,
    },
    StartQuantification {
        method_toml: String,
        dataset_ids: Vec<DatasetRef>,
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
impl Command {
    /// The dataset references of this command (those of commands nested in a batch are resolved
    /// when each one runs, so datasets opened earlier in the batch can be named).
    pub(crate) fn dataset_refs_mut(&mut self) -> Vec<&mut DatasetRef> {
        match self {
            Command::SelectTrace { dataset_id, .. }
            | Command::ListScans { dataset_id, .. }
            | Command::DatasetMetadata { dataset_id }
            | Command::ScanMetadata { dataset_id, .. }
            | Command::SelectDataset { dataset_id }
            | Command::ExtractChromatogram { dataset_id, .. }
            | Command::ChromatogramData { dataset_id, .. }
            | Command::Spectrum { dataset_id, .. }
            | Command::SetVisibility { dataset_id, .. }
            | Command::Integrate { dataset_id, .. }
            | Command::RetainTrace { dataset_id }
            | Command::RemoveTrace { dataset_id, .. }
            | Command::SetTraceVisibility { dataset_id, .. }
            | Command::ExportCsv { dataset_id, .. } => vec![dataset_id],
            Command::StartQuantification { dataset_ids, .. } => dataset_ids.iter_mut().collect(),
            _ => vec![],
        }
    }
}
/// Extensions of the vendor and interchange formats msconvert converts to mzML.
pub const VENDOR_EXTENSIONS: &[&str] = &[
    "raw", "d", "wiff", "wiff2", "lcd", "baf", "tdf", "tsf", "yep", "mzxml", "mz5", "cdf", "mgf",
];
fn has_extension(path: &Path, extension: &str) -> Result<(), String> {
    let matches = path
        .extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(extension));
    if matches {
        Ok(())
    } else {
        Err(format!("The file name must end in .{extension}"))
    }
}
/// Whether `path` is an existing file or directory with a vendor extension.
pub fn is_vendor_dataset(path: &Path) -> bool {
    path.exists()
        && path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .is_some_and(|e| VENDOR_EXTENSIONS.contains(&e.as_str()))
}
#[derive(Debug, Clone)]
pub struct AccessPolicy {
    roots: Vec<PathBuf>,
    pub allow_changes: bool,
    pub allow_exports: bool,
    /// Creating and saving projects, committing analyses and resolving review proposals.
    pub allow_project_writes: bool,
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
            allow_project_writes: false,
        })
    }
    /// Operator policy for project writes (`--allow-project-writes`); off by default.
    pub fn with_project_writes(mut self, allow: bool) -> Self {
        self.allow_project_writes = allow;
        self
    }
    /// The canonical authorised roots.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
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
    /// Something the window can import: an mzML/mzML.gz file, or a vendor dataset (a file or a
    /// directory such as `.d` or Waters `.raw`) that ProteoWizard's msconvert converts.
    pub fn source(&self, path: &str) -> Result<PathBuf, String> {
        let path = Path::new(path).canonicalize().map_err(|e| e.to_string())?;
        if !self.roots.iter().any(|r| path.starts_with(r)) {
            return Err("Path is outside authorized roots".into());
        }
        if crate::import::is_mzml(&path) || is_vendor_dataset(&path) {
            Ok(path)
        } else {
            Err(format!(
                "Only mzML/mzML.gz files and vendor datasets ({}) can be opened",
                VENDOR_EXTENSIONS.join(", ")
            ))
        }
    }
    /// An existing file inside a root, with the given extension (without the dot): a session,
    /// method, batch or preset to read.
    pub fn file(&self, path: &str, extension: &str) -> Result<PathBuf, String> {
        let path = Path::new(path).canonicalize().map_err(|e| e.to_string())?;
        if !self.roots.iter().any(|r| path.starts_with(r)) {
            return Err("Path is outside authorized roots".into());
        }
        if !path.is_file() {
            return Err("Not a file".into());
        }
        has_extension(&path, extension)?;
        Ok(path)
    }
    /// Like [`Self::output`], and the new file must have the given extension (without the dot).
    pub fn output_as(&self, path: &str, extension: &str) -> Result<PathBuf, String> {
        let target = self.output(path)?;
        has_extension(&target, extension)?;
        Ok(target)
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
        self.new_file(path)
    }
    /// A new file with the given extension, inside a root (no permission flag is checked).
    pub fn new_file_as(&self, path: &str, extension: &str) -> Result<PathBuf, String> {
        let target = self.new_file(path)?;
        has_extension(&target, extension)?;
        Ok(target)
    }
    /// Project operations (create, save) need the operator's explicit permission.
    pub fn require_project_writes(&self) -> Result<(), String> {
        if self.allow_project_writes {
            Ok(())
        } else {
            Err("Project writes require --allow-project-writes".into())
        }
    }
    /// A file that does not exist yet, inside a root (no permission flag is checked).
    pub fn new_file(&self, path: &str) -> Result<PathBuf, String> {
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
/// Messages from the server threads to the window that no client can send as a command: they
/// are produced only by the server itself, so an AI cannot inject results or log entries.
pub(crate) enum Internal {
    /// A tool call served by the headless engine, for the AI activity log.
    Activity(Value),
    /// A finished analysis (the JSON of an `engine::Response`) to put in the Quantification tab.
    Analysis {
        job_id: String,
        response: Box<Value>,
    },
}
/// Whether an analysis started over MCP is visible in the window.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum Route {
    /// Finished results will be placed in the window.
    Waiting,
    /// The result is in the window (`tab` says where).
    Shown { tab: String },
    /// The result stays an MCP job only (`reason` says why).
    NotShown { reason: String },
}
pub(crate) type RouteMap = Arc<Mutex<HashMap<String, Route>>>;
pub struct GuiBridge {
    pub(crate) pending: Option<crate::gui::remote::PendingOperation>,
    pub receiver: mpsc::Receiver<Envelope>,
    pub(crate) internal: mpsc::Receiver<Internal>,
    pub(crate) routed: RouteMap,
    /// Stable ids of the open datasets, computed in the background (see `DatasetRef`).
    pub(crate) stable: HashMap<usize, crate::gui::remote::StableState>,
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
    internal: mpsc::Sender<Internal>,
    context: egui::Context,
    tool_router: ToolRouter<Self>,
    /// The headless engine server, reused for every analysis, project and review tool.
    engine: crate::engine_mcp::EngineMcp,
    engine_tools: Arc<Vec<String>>,
    routed: RouteMap,
    policy: AccessPolicy,
}
impl McpServer {
    pub fn new(context: egui::Context, policy: AccessPolicy) -> Result<(Self, GuiBridge), String> {
        let (sender, receiver) = mpsc::sync_channel(16);
        let (internal, internal_receiver) = mpsc::channel();
        let engine = crate::engine_mcp::EngineMcp::new(policy.roots().to_vec())
            .map_err(|e| e.to_string())?
            .with_project_writes(policy.allow_project_writes);
        let routed = RouteMap::default();
        Ok((
            Self {
                sender,
                internal,
                context,
                tool_router: Self::tool_router(),
                engine_tools: Arc::new(engine.tool_names()),
                engine,
                routed: routed.clone(),
                policy: policy.clone(),
            },
            GuiBridge {
                pending: None,
                receiver,
                internal: internal_receiver,
                routed,
                stable: HashMap::new(),
                policy,
                history: vec![],
                undo: vec![],
                results: Default::default(),
            },
        ))
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
    fn set_route(&self, job_id: &str, route: Route) {
        if let Ok(mut routes) = self.routed.lock() {
            routes.insert(job_id.to_owned(), route);
        }
    }
    fn route_of(&self, job_id: &str) -> Option<Route> {
        self.routed.lock().ok()?.get(job_id).cloned()
    }
    fn tell_window(&self, event: Internal) -> bool {
        let sent = self.internal.send(event).is_ok();
        self.context.request_repaint();
        sent
    }
    /// Shows a finished analysis in the window once the engine has produced it. Runs only when
    /// the operator allowed changes: filling the Quantification tab replaces what the person
    /// sees there, which is a change to the window.
    fn watch_job(&self, job_id: String) {
        if !self.policy.allow_changes {
            self.set_route(
                &job_id,
                Route::NotShown {
                    reason: "The result is kept as an MCP job only; showing it in the window needs --allow-changes".into(),
                },
            );
            return;
        }
        self.set_route(&job_id, Route::Waiting);
        let server = self.clone();
        tokio::spawn(async move {
            loop {
                match server.engine.poll_job(&job_id) {
                    crate::engine_mcp::JobPoll::Pending => {
                        tokio::time::sleep(std::time::Duration::from_millis(25)).await
                    }
                    crate::engine_mcp::JobPoll::Done(response) => {
                        if !server.tell_window(Internal::Analysis {
                            job_id: job_id.clone(),
                            response,
                        }) {
                            server.set_route(
                                &job_id,
                                Route::NotShown {
                                    reason: "The window is closed".into(),
                                },
                            );
                        }
                        break;
                    }
                    crate::engine_mcp::JobPoll::Failed(error) => {
                        server.set_route(
                            &job_id,
                            Route::NotShown {
                                reason: format!("The job did not succeed: {}", error.message),
                            },
                        );
                        break;
                    }
                    crate::engine_mcp::JobPoll::Unknown => break,
                }
            }
        });
    }
    /// Serves one of the headless engine's tools, logs it in the AI activity view and ties the
    /// analysis jobs to the window.
    async fn call_engine_tool(
        &self,
        name: String,
        request: CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<CallToolResponse, rmcp::ErrorData> {
        let parameters = request
            .arguments
            .clone()
            .map(Value::Object)
            .unwrap_or(Value::Null);
        let job_id = request
            .arguments
            .as_ref()
            .and_then(|a| a.get("job_id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let result = self.engine.call_tool(request, context).await;
        let Ok(CallToolResponse::Complete(mut reply)) = result else {
            self.tell_window(Internal::Activity(activity_entry(
                &name,
                &parameters,
                Err("protocol error"),
            )));
            return result;
        };
        let failed = reply.is_error == Some(true);
        let message = reply
            .structured_content
            .as_ref()
            .and_then(|v| v["message"].as_str())
            .unwrap_or("The operation failed")
            .to_owned();
        self.tell_window(Internal::Activity(activity_entry(
            &name,
            &parameters,
            if failed { Err(&message) } else { Ok(()) },
        )));
        if !failed {
            let started = (name == "start_analysis")
                .then_some(reply.structured_content.as_ref())
                .flatten()
                .and_then(|v| v["job_id"].as_str())
                .map(str::to_owned);
            if let Some(id) = &started {
                self.watch_job(id.clone());
            }
            let window = started.or(job_id).filter(|_| {
                matches!(
                    name.as_str(),
                    "start_analysis" | "analysis_status" | "cancel_analysis"
                )
            });
            if let Some(route) = window.as_deref().and_then(|id| self.route_of(id)) {
                if let Some(Value::Object(mut map)) = reply.structured_content.take() {
                    map.insert("window".into(), json!(route));
                    reply = CallToolResult::structured(Value::Object(map));
                }
            }
        }
        Ok(CallToolResponse::Complete(reply))
    }
}
/// One AI activity entry; request payloads over 64 KiB are summarised so a large trace cannot
/// fill the window's memory.
fn activity_entry(tool: &str, parameters: &Value, outcome: Result<(), &str>) -> Value {
    let text = parameters.to_string();
    let parameters = if text.len() > 64 * 1024 {
        use sha2::{Digest, Sha256};
        json!({"truncated":true,"bytes":text.len(),"sha256":format!("{:x}",Sha256::digest(text.as_bytes()))})
    } else {
        parameters.clone()
    };
    match outcome {
        Ok(()) => json!({"request":{"operation":tool,"parameters":parameters},"success":true}),
        Err(error) => {
            json!({"request":{"operation":tool,"parameters":parameters},"success":false,"error":error})
        }
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
    pub dataset_id: DatasetRef,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScanMetadataArgs {
    pub dataset_id: DatasetRef,
    pub index: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SelectDatasetArgs {
    pub dataset_id: DatasetRef,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExtractChromatogramArgs {
    pub dataset_id: DatasetRef,
    /// `tic`, `bpc` or `xic`; `tic` for a stored chromatogram.
    pub kind: String,
    /// `positive`, `negative` or `unknown`.
    pub polarity: String,
    pub ms_level: u8,
    /// Smoothing level 0..10 (0 is none).
    pub smoothing: u8,
    pub mass: Option<f64>,
    /// XIC tolerance in ppm. Give this or `tolerance_mda`, not both.
    pub tolerance_ppm: Option<f64>,
    /// XIC tolerance in milli-daltons (mDa). Give this or `tolerance_ppm`, not both.
    #[serde(default)]
    pub tolerance_mda: Option<f64>,
    pub mz_range: Option<[f64; 2]>,
    pub precursor_mz: Option<f64>,
    pub acquisition: Option<String>,
    /// Extract a chromatogram stored in the file (SRM/MRM transitions, a stored TIC) instead of
    /// computing one from the spectra: the `index` of one of the `chromatograms` that
    /// dataset_metadata lists. Only smoothing applies: give kind `tic`, and omit mass, tolerance, mz_range,
    /// precursor_mz and acquisition.
    #[serde(default)]
    pub native_chromatogram: Option<usize>,
    #[serde(default)]
    pub display: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChromatogramDataArgs {
    pub dataset_id: DatasetRef,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpectrumArgs {
    pub dataset_id: DatasetRef,
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
    /// Restore the automatic view of the chromatograms and the spectrum, as Reset view does;
    /// ranges given in the same call are applied afterwards.
    #[serde(default)]
    pub reset: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetVisibilityArgs {
    pub dataset_id: DatasetRef,
    pub visible: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IntegrateArgs {
    pub dataset_id: DatasetRef,
    pub start_minutes: f64,
    pub end_minutes: f64,
    #[serde(default)]
    pub apply: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportCsvArgs {
    pub dataset_id: DatasetRef,
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
    pub dataset_ids: Vec<DatasetRef>,
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
        description = "List mzML and mzML.gz files in one directory (not recursive, max 10000, sorted); include_vendor=true adds vendor data that open_files converts. Directory inside an --allow-root.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn list_mzml_files(
        &self,
        Parameters(args): Parameters<ListMzmlFilesArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::ListMzmlFiles {
            directory: args.directory,
            include_vendor: args.include_vendor,
        })
        .await
    }
    #[tool(
        description = "Change how traces are drawn: compare_samples, intensity scale, line width, the active dataset's line colour. Presentation only. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
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
        description = "Re-activate a retained trace of a dataset by its gui_state index, restoring its extraction and data. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn select_trace(&self, Parameters(args): Parameters<SelectTraceArgs>) -> CallToolResult {
        self.dispatch(Command::SelectTrace {
            dataset_id: args.dataset_id,
            trace_index: args.trace_index,
        })
        .await
    }

    #[tool(
        description = "Page scan metadata of one dataset (native ID, retention time in minutes, MS level, polarity, precursors, windows), limit 1..1000.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
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
        description = "Page of the unsmoothed XIC behind one batch result as [minutes, intensity] points (limit 1..10000); areas are computed from it.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
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
        description = "Save the batch quantification as CSV (method snapshot, results, diagnostics, review status). Requires --allow-exports; new file inside an --allow-root.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn export_quantification(
        &self,
        Parameters(args): Parameters<ExportQuantificationArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::ExportQuantification { path: args.path })
            .await
    }
    #[tool(
        description = "Roll one batch result back to its automatic peak and original status. Requires --allow-changes; fails while a batch runs.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
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
        description = "Run 1-64 operations in order, one outcome each; saves round trips. Not a transaction (finished steps stay applied), no nesting; each step needs its own tool's permission.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn batch(&self, Parameters(args): Parameters<BatchArgs>) -> CallToolResult {
        self.dispatch(Command::Batch {
            commands: args.commands,
        })
        .await
    }
    #[tool(
        description = "Save the displayed chromatogram (or spectrum, with spectrum=true) as an SVG file. Requires --allow-exports; new file inside an --allow-root.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
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
        description = "Undo the last integration applied through MCP; fails if the workspace changed since. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn undo_integration(&self) -> CallToolResult {
        self.dispatch(Command::UndoIntegration {}).await
    }
    #[tool(
        description = "List the recorded integration measurements: dataset, trace, boundaries, area, extraction parameters.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn measurements(&self) -> CallToolResult {
        self.dispatch(Command::Measurements {}).await
    }
    #[tool(
        description = "Stop the running batch quantification; finished results are kept. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn cancel_quantification(&self) -> CallToolResult {
        self.dispatch(Command::CancelQuantification {}).await
    }

    #[tool(
        description = "Load 1-64 datasets and return a dataset_id per file: mzML/mzML.gz directly, vendor data (.raw, .d, .wiff...) via the configured msconvert. Needs no --allow-changes; paths inside an --allow-root.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn open_files(&self, Parameters(args): Parameters<OpenFilesArgs>) -> CallToolResult {
        let OpenFilesArgs { paths } = args;
        self.dispatch(Command::OpenFiles { paths }).await
    }

    #[tool(
        description = "Metadata of one dataset: scan count, retention-time and m/z ranges, scan filters present, stored chromatograms (index, label, SRM Q1/Q3; the index is native_chromatogram).",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn dataset_metadata(
        &self,
        Parameters(args): Parameters<DatasetMetadataArgs>,
    ) -> CallToolResult {
        let DatasetMetadataArgs { dataset_id } = args;
        self.dispatch(Command::DatasetMetadata { dataset_id }).await
    }

    #[tool(
        description = "Full metadata of one scan by zero-based index: native ID, retention time, MS level, polarity, precursor, instrument settings.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
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
        description = "Make one dataset the active one in the chromatogram and spectrum panels and clear the integration. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn select_dataset(
        &self,
        Parameters(args): Parameters<SelectDatasetArgs>,
    ) -> CallToolResult {
        let SelectDatasetArgs { dataset_id } = args;
        self.dispatch(Command::SelectDataset { dataset_id }).await
    }

    #[tool(
        description = "Extract a TIC, BPC or XIC from a dataset (or a stored SRM/MRM chromatogram via native_chromatogram) for chromatogram_data, integrate and export_csv. XIC needs mass and a tolerance. Read-only unless display=true (needs --allow-changes).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
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
            tolerance_mda,
            mz_range,
            precursor_mz,
            acquisition,
            native_chromatogram,
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
            tolerance_mda,
            mz_range,
            precursor_mz,
            acquisition,
            native_chromatogram,
            display,
        })
        .await
    }

    #[tool(
        description = "Page of the last extracted chromatogram of a dataset as [minutes, intensity] points (limit 1..10000).",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
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
        description = "Read one mass spectrum, by scan index or retention time in minutes (exactly one), as a page of [m/z, intensity] points (limit 1..10000). Shown in the window only with display=true (needs --allow-changes).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
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
        description = "Zoom the plots: retention-time range (minutes) and/or m/z range, overlay mode, or reset=true for the automatic view. Changes no data. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn set_view(&self, Parameters(args): Parameters<SetViewArgs>) -> CallToolResult {
        let SetViewArgs {
            retention_time_range,
            mz_range,
            overlay,
            reset,
        } = args;
        self.dispatch(Command::SetView {
            retention_time_range,
            mz_range,
            overlay,
            reset,
        })
        .await
    }

    #[tool(
        description = "Show or hide one dataset's trace in the comparison plot. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
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
        description = "Integrate a peak between two retention times (minutes) on a dataset's last extracted chromatogram, straight-chord baseline; returns the area in intensity*minutes. apply=true also records it (needs --allow-changes).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
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
        description = "Save a dataset's last extracted chromatogram (or its displayed spectrum, with spectrum=true) as CSV. Requires --allow-exports; new file inside an --allow-root.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
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
        description = "Apply a viewer preset (TOML text): sets overlay, rows and columns and schedules its traces for the open datasets (check gui_state). Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn apply_preset(&self, Parameters(args): Parameters<ApplyPresetArgs>) -> CallToolResult {
        let ApplyPresetArgs { toml } = args;
        self.dispatch(Command::ApplyPreset { toml }).await
    }

    #[tool(
        description = "Start a batch quantification: a method (TOML text) run on the given dataset_ids; results appear in the Quantification tab, poll with quantification_status. Replaces the previous results; requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
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
        description = "Correct one batch result's peak with start_minutes and end_minutes (area and status recomputed, automatic result kept). Marking reviewed is refused: a person does that. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
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
        description = "List loaded datasets: dataset_id (valid while the window is open), stable_id (SHA-256 of the source, survives restarts), name, path, state, visibility, last extraction. Either id goes in dataset_id of any tool.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn list_datasets(&self) -> CallToolResult {
        self.dispatch(Command::ListDatasets {}).await
    }
    #[tool(
        description = "Arrange the plots (overlay, grid, compare) and show or hide the files, trace settings and spectrum panels; focus mode. Presentation only. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn set_layout(&self, Parameters(args): Parameters<SetLayoutArgs>) -> CallToolResult {
        let SetLayoutArgs {
            mode,
            auto_grid,
            rows,
            columns,
            page,
            show_files,
            show_inspector,
            show_spectrum,
            focus,
        } = args;
        self.dispatch(Command::SetLayout {
            mode,
            auto_grid,
            rows,
            columns,
            page,
            show_files,
            show_inspector,
            show_spectrum,
            focus,
        })
        .await
    }
    #[tool(
        description = "Keep a dataset's current extraction as a retained trace so a later extraction shows next to it. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn retain_trace(&self, Parameters(args): Parameters<RetainTraceArgs>) -> CallToolResult {
        self.dispatch(Command::RetainTrace {
            dataset_id: args.dataset_id,
        })
        .await
    }
    #[tool(
        description = "Delete one trace of a dataset: a retained one by trace_index (see gui_state) or the current extraction. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn remove_trace(&self, Parameters(args): Parameters<RemoveTraceArgs>) -> CallToolResult {
        self.dispatch(Command::RemoveTrace {
            dataset_id: args.dataset_id,
            trace_index: args.trace_index,
        })
        .await
    }
    #[tool(
        description = "Show or hide one trace of a dataset: a retained one by trace_index (see gui_state) or the current extraction. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn set_trace_visibility(
        &self,
        Parameters(args): Parameters<SetTraceVisibilityArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::SetTraceVisibility {
            dataset_id: args.dataset_id,
            trace_index: args.trace_index,
            visible: args.visible,
        })
        .await
    }
    #[tool(
        description = "List extraction presets and starter files: shipped ones, recent ones inside an --allow-root and, with directory, the .toml files there, each classified.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn list_presets(&self, Parameters(args): Parameters<ListPresetsArgs>) -> CallToolResult {
        self.dispatch(Command::ListPresets {
            directory: args.directory,
        })
        .await
    }
    #[tool(
        description = "Read a preset or starter .toml file with its meaning (traces and layout, or a method's analytes); a valid reply goes straight to apply_preset or start_quantification.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_preset(&self, Parameters(args): Parameters<GetPresetArgs>) -> CallToolResult {
        self.dispatch(Command::GetPreset {
            source: args.source,
        })
        .await
    }
    #[tool(
        description = "Save a viewer preset (TOML text) as a new .toml file; validated first. Requires --allow-exports; new file inside an --allow-root.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn save_preset(&self, Parameters(args): Parameters<SavePresetArgs>) -> CallToolResult {
        self.dispatch(Command::SavePreset {
            path: args.path,
            toml: args.toml,
        })
        .await
    }
    #[tool(
        description = "Read the display preferences (theme, interface size in percent, m/z decimals) and the values each can take.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn get_settings(&self) -> CallToolResult {
        self.dispatch(Command::GetSettings {}).await
    }
    #[tool(
        description = "Change and keep display preferences: theme (system, light, dark), interface size in percent (90, 100, 115, 130), m/z decimals (2-6). Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn set_settings(&self, Parameters(args): Parameters<SetSettingsArgs>) -> CallToolResult {
        self.dispatch(Command::SetSettings {
            theme: args.theme,
            interface_size: args.interface_size,
            mz_decimals: args.mz_decimals,
        })
        .await
    }
    #[tool(
        description = "Delete one integration measurement by its position in measurements; the trace stays. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn remove_measurement(
        &self,
        Parameters(args): Parameters<RemoveMeasurementArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::RemoveMeasurement { index: args.index })
            .await
    }
    #[tool(
        description = "Clear the integration markers from the active chromatogram; recorded measurements stay. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn clear_integration(&self) -> CallToolResult {
        self.dispatch(Command::ClearIntegration {}).await
    }
    #[tool(
        description = "Switch the window to a workspace: Data explorer, Quantification and QC (at a stage), method editor, Audit package or AI activity. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn show_workspace(
        &self,
        Parameters(args): Parameters<ShowWorkspaceArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::ShowWorkspace {
            workspace: args.workspace,
            stage: args.stage,
        })
        .await
    }
    #[tool(
        description = "Write the Quantification tab's calibration or QC results to a new file (CSV or JSON, see kind); fails when the tab has none. Requires --allow-exports; new file inside an --allow-root.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn export_results(
        &self,
        Parameters(args): Parameters<ExportResultsArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::ExportResults {
            kind: args.kind,
            path: args.path,
        })
        .await
    }
    #[tool(
        description = "Load saved calibration runs or QC reports (.json from export_results) into the Quantification tab after checking each against its own evidence. Requires --allow-changes; file inside an --allow-root.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn import_results(
        &self,
        Parameters(args): Parameters<ImportResultsArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::ImportResults {
            kind: args.kind,
            path: args.path,
        })
        .await
    }
    #[tool(
        description = "Clear the mass spectrum panel or the shared batch preset; datasets, measurements and results stay. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn clear_display(
        &self,
        Parameters(args): Parameters<ClearDisplayArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::ClearDisplay { what: args.what })
            .await
    }
    #[tool(
        description = "Save the integration measurements (file, trace, start/end in minutes, area, parameters) as a new CSV. Requires --allow-exports; new .csv file inside an --allow-root.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn export_measurements(
        &self,
        Parameters(args): Parameters<ExportMeasurementsArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::ExportMeasurements { path: args.path })
            .await
    }
    #[tool(
        description = "Build the audit package in a new folder (report.html, evidence, checksums.json; sources optional). Open warnings need acknowledge_warnings=true. Requires --allow-exports; folder inside an --allow-root.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn build_bundle(&self, Parameters(args): Parameters<BuildBundleArgs>) -> CallToolResult {
        let BuildBundleArgs {
            destination,
            project,
            include_sources,
            acknowledge_warnings,
            config,
        } = args;
        self.dispatch(Command::BuildBundle {
            destination,
            project,
            include_sources,
            acknowledge_warnings,
            config,
        })
        .await
    }
    #[tool(
        description = "Check an audit package: every file in checksums.json present and unchanged, none missing or extra. Folder inside an --allow-root.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn verify_bundle(
        &self,
        Parameters(args): Parameters<VerifyBundleArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::VerifyBundle { path: args.path })
            .await
    }
    #[tool(
        description = "Recompute a retained project result from its verified source and compare it with the stored output. Needs the program version that made it. Project inside an --allow-root.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn reprocess_result(
        &self,
        Parameters(args): Parameters<ReprocessResultArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::ReprocessResult {
            result_id: args.result_id,
            project: args.project,
            include_response: args.include_response,
        })
        .await
    }
    #[tool(
        description = "Create a project (file plus _data folder) from the workspace: datasets, results, AI proposals, first revision. Fails while work runs. Requires --allow-project-writes; new .json path inside an --allow-root.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn create_project(
        &self,
        Parameters(args): Parameters<CreateProjectArgs>,
    ) -> CallToolResult {
        self.dispatch(Command::CreateProject { path: args.path })
            .await
    }
    #[tool(
        description = "Open a project in the window; every dataset must be inside an --allow-root or nothing changes. Replaces the workspace. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn open_project(&self, Parameters(args): Parameters<OpenProjectArgs>) -> CallToolResult {
        self.dispatch(Command::OpenProject { path: args.path })
            .await
    }
    #[tool(
        description = "Save the open project as a new revision of the workspace and results; earlier revisions stay restorable. Requires --allow-project-writes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn save_project(&self) -> CallToolResult {
        self.dispatch(Command::SaveProject {}).await
    }
    #[tool(
        description = "Reload the open project's results and review proposals into the window (work another session committed or queued). Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn refresh_project(&self) -> CallToolResult {
        self.dispatch(Command::RefreshProject {}).await
    }
    #[tool(
        description = "Save a quantification method as a new .toml file: the Quantification tab's, or with method_toml one you give. Requires --allow-exports; new file inside an --allow-root.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn save_method(&self, Parameters(args): Parameters<SaveMethodArgs>) -> CallToolResult {
        self.dispatch(Command::SaveMethod {
            path: args.path,
            method_toml: args.method_toml,
        })
        .await
    }
    #[tool(
        description = "Make a method .toml file the Quantification tab's method. Refuses to replace unsaved edits unless discard_unsaved=true. Requires --allow-changes.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn open_method(&self, Parameters(args): Parameters<OpenMethodArgs>) -> CallToolResult {
        self.dispatch(Command::OpenMethod {
            path: args.path,
            discard_unsaved: args.discard_unsaved,
        })
        .await
    }
    #[tool(
        description = "Save the batch quantification (method, results, review state, samples) as a new .chromquant file. Requires --allow-exports; new file inside an --allow-root.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn save_batch(&self, Parameters(args): Parameters<BatchFileArgs>) -> CallToolResult {
        self.dispatch(Command::SaveBatch { path: args.path }).await
    }
    #[tool(
        description = "Replace the Quantification tab's method and results with a saved .chromquant batch after checking it. Requires --allow-changes; discards the current batch results.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn load_batch(&self, Parameters(args): Parameters<BatchFileArgs>) -> CallToolResult {
        self.dispatch(Command::LoadBatch { path: args.path }).await
    }
    #[tool(
        description = "Save the workspace (datasets, traces, measurements, view) as a new .chromascope session file. Requires --allow-exports; new file inside an --allow-root.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn save_session(&self, Parameters(args): Parameters<SessionPathArgs>) -> CallToolResult {
        self.dispatch(Command::SaveSession { path: args.path })
            .await
    }
    #[tool(
        description = "Replace the workspace with a saved .chromascope session; every source must be inside an --allow-root or nothing changes. Requires --allow-changes; discards the current workspace.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            open_world_hint = false
        )
    )]
    async fn load_session(&self, Parameters(args): Parameters<SessionPathArgs>) -> CallToolResult {
        self.dispatch(Command::LoadSession { path: args.path })
            .await
    }
    #[tool(
        description = "Whole state of the window: datasets, active dataset, processing, plot bounds, layout, retained traces (trace_index), measurements, history, units. Use it at session start.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn gui_state(&self) -> CallToolResult {
        self.dispatch(Command::GuiState {}).await
    }
    #[tool(
        description = "Render the chromatogram shown in the window as a PNG with the plot state; nothing is written to disk.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn chromatogram_image(&self) -> CallToolResult {
        self.dispatch(Command::PlotImage { spectrum: false }).await
    }
    #[tool(
        description = "Render the mass spectrum shown in the window as a PNG with the plot state; nothing is written to disk.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn spectrum_image(&self) -> CallToolResult {
        self.dispatch(Command::PlotImage { spectrum: true }).await
    }
    #[tool(
        description = "Poll the batch quantification: progress, method signature and every result with its peak, status and review state; gives the result_index for the other quantification tools.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn quantification_status(&self) -> CallToolResult {
        self.dispatch(Command::QuantificationStatus {}).await
    }
}
#[tool_handler(router = self.tool_router)]
impl ServerHandler for McpServer {
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<CallToolResponse, rmcp::ErrorData> {
        let name = request.name.to_string();
        if self.engine_tools.contains(&name) {
            return self.call_engine_tool(name, request, context).await;
        }
        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(call).await
    }
    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<ListToolsResult, rmcp::ErrorData> {
        // The engine hides the experimental workflows in the default build; its list is
        // already filtered and sorted. The desktop tools come first, then the engine's.
        let mut listed = self.engine.list_tools(request, context).await?;
        let mut tools = self.tool_router.list_all();
        tools.iter_mut().for_each(crate::engine_mcp::compact_tool);
        tools.append(&mut listed.tools);
        listed.tools = tools;
        Ok(listed)
    }
    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<ListResourcesResult, rmcp::ErrorData> {
        let reply = self.dispatch(Command::ListResources {}).await;
        let listing = reply.structured_content.unwrap_or(Value::Null);
        let resources = listing["resources"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        Some(
                            Resource::new(item["uri"].as_str()?, item["name"].as_str()?)
                                .with_description(item["description"].as_str().unwrap_or_default())
                                .with_mime_type("application/json"),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(ListResourcesResult::with_all_items(resources))
    }
    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<ListResourceTemplatesResult, rmcp::ErrorData> {
        Ok(ListResourceTemplatesResult::with_all_items(
            crate::gui::remote::resource_templates()
                .iter()
                .map(|(uri, name, description)| {
                    ResourceTemplate::new(*uri, *name)
                        .with_description(*description)
                        .with_mime_type("application/json")
                })
                .collect(),
        ))
    }
    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<ReadResourceResponse, rmcp::ErrorData> {
        let reply = self
            .dispatch(Command::ReadResource {
                uri: request.uri.clone(),
            })
            .await;
        if reply.is_error == Some(true) {
            let message = reply
                .structured_content
                .as_ref()
                .and_then(|v| v["message"].as_str())
                .unwrap_or("The resource cannot be read")
                .to_owned();
            return Err(rmcp::ErrorData::resource_not_found(message, None));
        }
        let value = reply.structured_content.unwrap_or(Value::Null);
        Ok(ReadResourceResponse::Complete(ReadResourceResult::new(
            vec![ResourceContents::text(value.to_string(), request.uri)
                .with_mime_type("application/json")],
        )))
    }
    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<ListPromptsResult, rmcp::ErrorData> {
        Ok(ListPromptsResult::with_all_items(
            crate::engine_mcp::workflow_prompts(),
        ))
    }
    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<GetPromptResponse, rmcp::ErrorData> {
        let focus = request
            .arguments
            .as_ref()
            .and_then(|a| a.get("focus"))
            .and_then(Value::as_str);
        crate::engine_mcp::workflow_prompt(&request.name, focus, true)
            .map(GetPromptResponse::Complete)
    }
    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tool_router
            .get(name)
            .cloned()
            .or_else(|| self.engine.get_tool(name))
    }
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .build(),
        )
            .with_server_info(Implementation::new("chromascope", env!("CARGO_PKG_VERSION")))
            .with_instructions("One server for everything the person sees in the window. Inspect datasets first (list_datasets, gui_state). Use the typed viewer tools to open data, extract and integrate; start_analysis runs calibration, quantification and QC on the shared engine and, when launched with --allow-changes, shows the finished result in the Quantification tab (analysis_status says whether it did). Retrieve bounded numerical pages for quantitation; images aid visual review. Read-only unless explicitly enabled at launch (--allow-changes, --allow-exports, --allow-project-writes). AI interpretations are external to measured results.")
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetLayoutArgs {
    /// `overlay` (all traces in one plot), `grid` (one plot per trace) or `compare` (samples as
    /// columns, analytes as rows).
    pub mode: Option<String>,
    /// Grid: fit rows and columns to the window and show every trace.
    pub auto_grid: Option<bool>,
    /// Grid rows 1..8, used when auto_grid is off.
    pub rows: Option<usize>,
    /// Grid columns 1..8, used when auto_grid is off.
    pub columns: Option<usize>,
    /// Zero-based page of a fixed grid that holds more traces than fit.
    pub page: Option<usize>,
    /// Show or hide the data files panel.
    pub show_files: Option<bool>,
    /// Show or hide the trace settings panel.
    pub show_inspector: Option<bool>,
    /// Show or hide the mass spectrum panel.
    pub show_spectrum: Option<bool>,
    /// Focus mode: hide all three panels and restore them later.
    pub focus: Option<bool>,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetainTraceArgs {
    pub dataset_id: DatasetRef,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveTraceArgs {
    pub dataset_id: DatasetRef,
    /// Index of a retained trace (the `trace_index` in gui_state); omit for the current extraction.
    pub trace_index: Option<usize>,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetTraceVisibilityArgs {
    pub dataset_id: DatasetRef,
    /// Index of a retained trace (the `trace_index` in gui_state); omit for the current extraction.
    pub trace_index: Option<usize>,
    pub visible: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListPresetsArgs {
    /// A directory inside an --allow-root whose .toml files are checked and listed (not
    /// recursive, at most 500 files).
    pub directory: Option<String>,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GetPresetArgs {
    /// `example:viewer`, `example:quantification` or a .toml file inside an --allow-root.
    pub source: String,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SavePresetArgs {
    /// New .toml file inside an --allow-root.
    pub path: String,
    /// The viewer preset as TOML text (see get_preset example:viewer).
    pub toml: String,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetSettingsArgs {
    /// `system`, `light` or `dark`.
    pub theme: Option<String>,
    /// Interface size in percent: 90, 100, 115 or 130.
    pub interface_size: Option<u16>,
    /// Decimals shown for every on-screen m/z: 2 to 6. Exports always keep full precision.
    pub mz_decimals: Option<u8>,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveMeasurementArgs {
    /// Position in the list `measurements` returns (zero-based).
    pub index: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ShowWorkspaceArgs {
    /// `data_explorer`, `quantification`, `method_editor` (the method window of the
    /// Quantification tab), `audit_package` or `ai_activity`.
    pub workspace: String,
    /// Quantification stage: `review`, `calibration` or `qc`; the last one when omitted.
    pub stage: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportResultsArgs {
    /// `concentrations_csv`, `calibration_csv` (latest run), `targeted_history_json` (all runs),
    /// `qc_csv`, `qc_evidence_json` (selected report) or `qc_history_json` (all reports).
    pub kind: String,
    /// New file inside an --allow-root: .csv for the CSV kinds, .json for the JSON kinds.
    pub path: String,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportResultsArgs {
    /// `targeted_history_json` (calibration runs) or `qc_history_json` (QC reports).
    pub kind: String,
    /// Existing .json file inside an --allow-root.
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClearDisplayArgs {
    /// `spectrum` or `batch_preset`.
    pub what: String,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExportMeasurementsArgs {
    /// New .csv file inside an --allow-root.
    pub path: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BuildBundleArgs {
    /// New folder to create inside an --allow-root; it must not exist.
    pub destination: String,
    /// The project file or its `_data` folder; the open project when omitted.
    pub project: Option<String>,
    /// Also copy the raw source datasets (verified by checksum) into the bundle so it can be
    /// checked elsewhere.
    #[serde(default)]
    pub include_sources: bool,
    /// Export although the window shows open warnings (unresolved peaks, unreviewed results,
    /// failed QC); they are saved in the bundle as review-warnings.json.
    #[serde(default)]
    pub acknowledge_warnings: bool,
    /// Report title, sections and figure size; the defaults of the Audit package tab when omitted.
    pub config: Option<crate::delivery::Config>,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerifyBundleArgs {
    /// The bundle folder inside an --allow-root.
    pub path: String,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReprocessResultArgs {
    /// UUID of a result of the project (see inspect_project).
    pub result_id: String,
    /// The project file or its `_data` folder; the open project when omitted.
    pub project: Option<String>,
    /// Include the recomputed response in the reply (large for big results).
    #[serde(default)]
    pub include_response: bool,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateProjectArgs {
    /// New project file (.json) inside an --allow-root; the project's data goes in the
    /// `<name>_data` folder next to it.
    pub path: String,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenProjectArgs {
    /// The project file (.json) the person opens, or its `<name>_data` folder, inside an
    /// --allow-root.
    pub path: String,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveMethodArgs {
    /// New .toml file inside an --allow-root.
    pub path: String,
    /// A method as TOML text to check and save instead of the method in the window.
    pub method_toml: Option<String>,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenMethodArgs {
    /// Existing method .toml file inside an --allow-root.
    pub path: String,
    /// Replace the window's method even though it has edits that were never saved.
    #[serde(default)]
    pub discard_unsaved: bool,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchFileArgs {
    /// Path of the `.chromquant` batch file inside an --allow-root.
    pub path: String,
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionPathArgs {
    /// Path of the `.chromascope` session file.
    pub path: String,
}

/// The operation names a batch accepts: those of [`Command`], derived from its schema so the
/// list cannot drift, without the batch itself (batches do not nest).
fn batch_operation_names() -> Vec<String> {
    let schema = serde_json::to_value(schemars::schema_for!(Command)).unwrap_or(Value::Null);
    let mut names: Vec<String> = schema["oneOf"]
        .as_array()
        .map(|variants| {
            variants
                .iter()
                .filter_map(|variant| crate::engine_mcp::schema_tag(variant, "operation"))
                .filter(|name| name != "batch")
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}
/// Schema of the commands of a batch as listed in `tools/list`: the allowed operation names
/// instead of the full schema of every command. Each command is still parsed into [`Command`]
/// when the tool is called, so the checks are the same as before.
fn batch_commands_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "array",
        "minItems": 1,
        "maxItems": 64,
        "description": "Commands run in order. Each is the argument object of the matching tool plus \"operation\" (one of the names below), e.g. {\"operation\":\"set_view\",\"reset\":true}; see that tool's schema.",
        "items": {
            "type": "object",
            "required": ["operation"],
            "properties": {"operation": {"enum": batch_operation_names()}}
        }
    })
}
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchArgs {
    #[schemars(schema_with = "batch_commands_schema")]
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
pub struct ListMzmlFilesArgs {
    pub directory: String,
    /// Also list vendor datasets (`.raw`, `.d`, `.wiff`, ...), which open_files converts with
    /// msconvert.
    #[serde(default)]
    pub include_vendor: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListScansArgs {
    pub dataset_id: DatasetRef,
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
    pub dataset_id: DatasetRef,
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
        )
        .unwrap();
        assert!(!bridge.policy.allow_changes);
        let router = McpServer::tool_router();
        let tools = router.list_all();
        assert!(tools.len() >= 24);
        for tool in tools {
            assert!(tool.input_schema.contains_key("type"));
        }
    }
    /// Tools that change the window, results or files on disk; every other tool only reads.
    const WRITE_TOOLS: &[&str] = &[
        "open_files",
        "select_dataset",
        "extract_chromatogram",
        "spectrum",
        "set_view",
        "set_visibility",
        "set_display",
        "select_trace",
        "set_layout",
        "retain_trace",
        "remove_trace",
        "set_trace_visibility",
        "save_preset",
        "build_bundle",
        "set_settings",
        "clear_display",
        "export_measurements",
        "export_results",
        "import_results",
        "show_workspace",
        "remove_measurement",
        "clear_integration",
        "create_project",
        "open_project",
        "save_project",
        "refresh_project",
        "save_method",
        "open_method",
        "save_batch",
        "load_batch",
        "save_session",
        "load_session",
        "integrate",
        "undo_integration",
        "export_figure",
        "export_csv",
        "apply_preset",
        "start_quantification",
        "cancel_quantification",
        "review_quantification",
        "reset_quantification",
        "export_quantification",
        "batch",
    ];
    #[test]
    fn tool_descriptions_are_unique_nonempty_and_annotated() {
        let tools = McpServer::tool_router().list_all();
        assert_eq!(tools.len(), 60, "60 typed tools; workbench is not listed");
        assert!(
            tools.iter().all(|t| t.name != "workbench"),
            "workbench duplicated the typed tools and must not be listed"
        );
        let mut seen = std::collections::HashMap::new();
        for tool in &tools {
            let description = tool.description.as_deref().unwrap_or("").trim();
            assert!(
                description.len() >= 40,
                "{} needs a real description",
                tool.name
            );
            assert!(
                !description.contains("Chromascope") || !description.contains("operation."),
                "{} still has the shared boilerplate",
                tool.name
            );
            if let Some(other) = seen.insert(description.to_owned(), tool.name.to_string()) {
                panic!("{} and {} share a description", other, tool.name);
            }
            let hints = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("{} has no annotations", tool.name));
            let read_only = hints
                .read_only_hint
                .unwrap_or_else(|| panic!("{} lacks read_only_hint", tool.name));
            assert!(
                hints.destructive_hint.is_some(),
                "{} lacks destructive_hint",
                tool.name
            );
            assert_eq!(
                read_only,
                !WRITE_TOOLS.contains(&&*tool.name),
                "{}: read_only_hint disagrees with the permission it needs",
                tool.name
            );
            if read_only {
                assert_eq!(hints.destructive_hint, Some(false), "{}", tool.name);
            }
            // Tools gated by a launch flag must say which one.
            let gated = description.contains("--allow-changes")
                || description.contains("--allow-exports")
                || description.contains("--allow-project-writes");
            if !read_only && !matches!(&*tool.name, "open_files" | "batch") {
                let flag = if matches!(&*tool.name, "create_project" | "save_project") {
                    "--allow-project-writes"
                } else if tool.name.starts_with("export_")
                    || tool.name.starts_with("save_")
                    || tool.name == "build_bundle"
                {
                    "--allow-exports"
                } else {
                    "--allow-changes"
                };
                assert!(
                    gated && description.contains(flag),
                    "{} must state the launch flag it needs ({flag})",
                    tool.name
                );
            }
        }
        for name in WRITE_TOOLS {
            assert!(
                tools.iter().any(|t| t.name == *name),
                "{name} is not a listed tool"
            );
        }
    }
    #[test]
    fn protocol_initialize_discovery_and_invalid_tool_arguments() {
        use rmcp::ServiceExt;
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let policy=AccessPolicy::new(vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))],false,false).unwrap();
            let (server,_bridge)=McpServer::new(egui::Context::default(),policy).unwrap();
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
            let listed=discovered["result"]["tools"].as_array().unwrap();
            let desktop=McpServer::tool_router().list_all();
            assert_eq!(desktop.len(),60);
            assert!(desktop.iter().all(|d|listed.iter().any(|t|t["name"]==d.name.as_ref())),"every viewer tool is listed");
            let engine=crate::engine_mcp::EngineMcp::new(vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))]).unwrap().tool_names();
            let visible=engine.iter().filter(|n|crate::EXPERIMENTAL||!matches!(n.as_str(),"start_untargeted"|"untargeted_status"|"cancel_untargeted"|"untargeted_features")).count();
            assert_eq!(listed.len(),60+visible,"viewer tools plus the shared engine tools");
            assert!(listed.iter().all(|t|t["name"]!="workbench"));
            assert!(listed.iter().all(|t|t["annotations"]["readOnlyHint"].is_boolean()&&t["annotations"]["destructiveHint"].is_boolean()),"every tool must carry both hints on the wire");
            let find=|name:&str|listed.iter().find(|t|t["name"]==name).unwrap();
            assert_eq!(find("gui_state")["annotations"]["readOnlyHint"],true);
            assert_eq!(find("reset_quantification")["annotations"]["destructiveHint"],true);
            assert_eq!(find("export_csv")["annotations"]["readOnlyHint"],false);
            let invalid=rpc(&mut write,&mut read,json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"spectrum","arguments":{"dataset_id":"bad"}}})).await;
            assert!(invalid["error"].is_object() || invalid["result"]["isError"]==true);
            drop(write);drop(read);task.await.unwrap();
        });
    }
}
