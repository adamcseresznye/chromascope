//! Optional headless MCP capability; existing GUI tool names and policy remain unchanged.
use crate::{
    domain::{EngineError, Request},
    jobs::{JobControl, Scheduler},
};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use std::{path::PathBuf, sync::Arc};
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnalysisArgs {
    pub path: String,
    /// Version-1 engine request with operation_id UUID, actor and typed operation.
    pub request: serde_json::Value,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProposalArgs {
    pub path: String,
    pub request: Request,
    pub expected_project_revision: Option<u64>,
    pub reason: String,
    pub evidence: Vec<String>,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueueProposalArgs {
    pub directory: String,
    pub dataset_id: String,
    pub proposal: serde_json::Value,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResolveProposalArgs {
    pub directory: String,
    pub dataset_id: String,
    pub proposal: serde_json::Value,
    pub approve: bool,
    pub actor: String,
    pub reason: String,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct FeatureJobArgs {
    pub job_id: String,
}
fn feature_reply(result: crate::domain::Result<serde_json::Value>) -> CallToolResult {
    let result = result.and_then(|value| {
        if serde_json::to_vec(&value)
            .map_err(|e| EngineError::new("adapter_failure", e))?
            .len()
            > 16 * 1024 * 1024
        {
            return Err(EngineError::new(
                "resource_limit",
                "Feature response exceeds 16 MiB; use smaller pages or local retained artifacts",
            ));
        }
        Ok(value)
    });
    match result {
        Ok(value) => CallToolResult::structured(value),
        Err(error) => CallToolResult::structured_error(serde_json::json!(error)),
    }
}
fn page_limit(limit: usize) -> crate::domain::Result<()> {
    if !(1..=1000).contains(&limit) {
        return Err(EngineError::new("invalid_parameters", "Use 1-1000 items"));
    }
    Ok(())
}
fn page_values(
    values: &[serde_json::Value],
    offset: usize,
    limit: usize,
    identity: serde_json::Value,
) -> crate::domain::Result<serde_json::Value> {
    if offset > values.len() {
        return Err(EngineError::new(
            "invalid_parameters",
            "Offset beyond result",
        ));
    }
    let end = offset.saturating_add(limit).min(values.len());
    Ok(
        serde_json::json!({"identity":identity,"total":values.len(),"offset":offset,"next_offset":if end<values.len(){Some(end)}else{None},"items":values[offset..end]}),
    )
}
fn digest(value: &serde_json::Value) -> crate::domain::Result<String> {
    use sha2::{Digest, Sha256};
    Ok(format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(value).map_err(|e| EngineError::new("adapter_failure", e))?
        )
    ))
}
fn outcome(result: &crate::domain::Result<serde_json::Value>) -> serde_json::Value {
    match result {
        Ok(value) => {
            serde_json::json!({"ok":true,"sha256":digest(value).ok(),"job_id":value.get("job_id"),"revision":value.get("revision")})
        }
        Err(error) => serde_json::json!({"ok":false,"error":error}),
    }
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct FeaturePageArgs {
    pub job_id: String,
    pub offset: usize,
    pub limit: usize,
    pub include_evidence: bool,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StartAnalysisArgs {
    pub path: String,
    pub request: Request,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResultPageArgs {
    pub job_id: String,
    /// RFC 6901 pointer into the immutable response. Empty selects the whole response.
    pub pointer: String,
    pub offset: usize,
    pub limit: usize,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryArgs {
    pub directory: String,
    pub offset: usize,
    pub limit: usize,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectArgs {
    pub directory: String,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommitArgs {
    /// Existing project dataset UUID to associate with this result.
    pub dataset_id: String,
    pub directory: String,
    pub job_id: String,
    pub expected_revision: u64,
    /// Client-attributed approval; not authenticated proof of human identity.
    pub approval_actor: String,
    pub reason: String,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReportArgs {
    pub job_ids: Vec<String>,
    /// AI narrative is attributed and explicitly unverified.
    pub explanation: String,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectReportArgs {
    pub directory: String,
    pub config: crate::delivery::Config,
}
#[derive(serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditArgs {
    pub offset: usize,
    pub limit: usize,
}
struct FeatureJob {
    handle: crate::jobs::JobHandle,
    result: Option<crate::domain::Result<crate::engine::Response>>,
}
#[derive(Clone)]
pub struct EngineMcp {
    roots: Arc<Vec<PathBuf>>,
    scheduler: Arc<Scheduler>,
    tool_router: ToolRouter<Self>,
    feature_jobs: Arc<std::sync::Mutex<std::collections::HashMap<String, FeatureJob>>>,
    audit: Arc<std::sync::Mutex<Vec<serde_json::Value>>>,
    project_writes: bool,
}
impl EngineMcp {
    pub fn new(roots: Vec<PathBuf>) -> crate::domain::Result<Self> {
        if roots.is_empty() {
            return Err(EngineError::new(
                "unauthorized",
                "At least one allowed root is required",
            ));
        }
        let roots = roots
            .into_iter()
            .map(|p| {
                std::fs::canonicalize(p).map_err(|e| EngineError::new("invalid_parameters", e))
            })
            .collect::<crate::domain::Result<Vec<_>>>()?;
        if roots.iter().any(|p| !p.is_dir()) {
            return Err(EngineError::new(
                "invalid_parameters",
                "Roots must be directories",
            ));
        }
        Ok(Self {
            roots: Arc::new(roots),
            scheduler: Arc::new(Scheduler::new(2, 16)?),
            tool_router: Self::tool_router(),
            feature_jobs: Default::default(),
            audit: Default::default(),
            project_writes: false,
        })
    }
    /// Explicit operator policy. Defaults to read-only project access.
    pub fn with_project_writes(mut self, enabled: bool) -> Self {
        self.project_writes = enabled;
        self
    }
    fn record(&self, tool: &str, parameters: serde_json::Value, outcome: serde_json::Value) {
        let mut audit = self.audit.lock().unwrap_or_else(|e| e.into_inner());
        let sequence = audit.len();
        audit.push(serde_json::json!({"sequence":sequence,"unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis(),"tool":tool,"parameters":parameters,"outcome":outcome}));
    }
    fn authorize_request(&self, request: &Request) -> crate::domain::Result<()> {
        if let crate::domain::Operation::TargetedBatch { batch } = &request.operation {
            for sample in &batch.samples {
                self.authorize(&sample.source)?;
            }
        }
        if let crate::domain::Operation::UntargetedBatch { config } = &request.operation {
            self.authorize_features(config)?;
        }
        Ok(())
    }
    fn response_value(&self, id: &str) -> crate::domain::Result<serde_json::Value> {
        let mut jobs = self
            .feature_jobs
            .lock()
            .map_err(|e| EngineError::new("adapter_failure", e))?;
        let job = jobs
            .get_mut(id)
            .ok_or_else(|| EngineError::new("invalid_parameters", "Unknown job"))?;
        if job.result.is_none() {
            job.result = job.handle.try_result();
        }
        let response = job
            .result
            .as_ref()
            .ok_or_else(|| EngineError::new("not_ready", "Job has no result"))?
            .as_ref()
            .map_err(Clone::clone)?;
        serde_json::to_value(response).map_err(|e| EngineError::new("adapter_failure", e))
    }
    pub fn authorize(&self, path: &str) -> crate::domain::Result<PathBuf> {
        let path =
            std::fs::canonicalize(path).map_err(|e| EngineError::new("missing_source", e))?;
        if !self.roots.iter().any(|root| path.starts_with(root)) {
            return Err(EngineError::new(
                "unauthorized",
                "Source outside allowed roots",
            ));
        }
        Ok(path)
    }
    fn authorize_features(&self, config: &crate::untargeted::Config) -> crate::domain::Result<()> {
        crate::untargeted::validate(config)?;
        for sample in &config.samples {
            self.authorize(&sample.source)?;
        }
        let cache = std::path::Path::new(&config.cache_directory);
        let check = if cache.exists() {
            cache
        } else {
            cache.parent().ok_or_else(|| {
                EngineError::new("unauthorized", "Cache requires an allowed existing parent")
            })?
        };
        self.authorize(&check.to_string_lossy())?;
        Ok(())
    }
}
#[tool_router]
impl EngineMcp {
    #[tool(
        description = "Discover the complete shared-engine request JSON Schema, scientific units, modification policy and six agent workflow recipes. All operation variants are executable with start_analysis; legacy tools remain available.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn analysis_capabilities(&self) -> CallToolResult {
        feature_reply(Ok(serde_json::json!({
            "request_schema":schemars::schema_for!(Request),
            "response_schema":schemars::schema_for!(crate::engine::Response),
            "units":{"retention_time":"minute","area":"instrument intensity * minute","concentration":"declared by calibration"},
            "policy":{"project_writes_enabled":self.project_writes,"analysis":"immutable retained computation; no project commit","preview":"revise_chromatogram preview=true; cannot commit","proposal":"annotation/boundary/review revisions are proposed until explicitly committed","commit":"existing project, expected revision, client-attributed approval; never scientific certification","report":"review draft; unresolved/failed/missing evidence preserved; AI narrative unverified"},
            "workflows":[
                {"name":"investigate_chromatographic_anomalies","prompt":"Investigate this unexpected peak without changing the original result.","operations":["metadata","extract","spectrum","process_chromatograms","evaluate_qc"],"review":"Compare unsmoothed evidence, baseline and flags; do not invent causes."},
                {"name":"review_integration","prompt":"Preview alternative peak bounds and present both areas for review.","operations":["extract","process_chromatograms","revise_chromatogram"],"review":"Use preview=true first; retain original and corrected revisions with reasons."},
                {"name":"construct_calibration","prompt":"Build a weighted calibration from my standards and quantify unknowns.","operations":["targeted_batch","evaluate_targeted_qc","review_targeted","export_targeted"],"review":"Declare units, roles, range, dilution, weighting and thresholds; review accuracy, qualifiers and missing states."},
                {"name":"troubleshoot_qc","prompt":"Explain QC failures using recorded evidence and identify missing measurements.","operations":["evaluate_targeted_qc","evaluate_qc","review_qc","export_qc"],"review":"Acknowledgement does not erase failures; required absent evidence remains indeterminate."},
                {"name":"compare_processing_methods","prompt":"Compare two processing configurations on the same raw chromatogram.","operations":["extract","process_chromatograms","quantify","validate_method"],"review":"Run separate jobs with frozen parameters; compare evidence and units, not explanations alone."},
                {"name":"prepare_report","prompt":"Prepare a reproducible draft including questionable results and review history.","operations":["export_targeted","export_qc","export_statistics"],"tools":["analysis_status","analysis_result","session_audit","prepare_analysis_report"],"review":"Include all relevant job IDs; report is a review draft, never approved scientific release."}
            ]
        })))
    }
    #[tool(
        description = "Read-only paginated discovery of mzML and mzML.gz files in one allowed directory. Sorted paths, byte counts; does not register or alter sources. Offsets refer to the returned listing SHA256; restart paging if directory changes.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn discover_datasets(
        &self,
        Parameters(args): Parameters<DiscoveryArgs>,
    ) -> CallToolResult {
        let result = (|| {
            page_limit(args.limit)?;
            let directory = self.authorize(&args.directory)?;
            let mut entries = Vec::new();
            for entry in
                std::fs::read_dir(directory).map_err(|e| EngineError::new("missing_source", e))?
            {
                let entry = entry.map_err(|e| EngineError::new("missing_source", e))?;
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if name.ends_with(".mzml") || name.ends_with(".mzml.gz") {
                    let path = self.authorize(&entry.path().to_string_lossy())?;
                    let metadata = std::fs::metadata(&path)
                        .map_err(|e| EngineError::new("missing_source", e))?;
                    if metadata.is_file() {
                        entries.push(serde_json::json!({"path":path,"bytes":metadata.len()}));
                    }
                }
                if entries.len() > 10000 {
                    return Err(EngineError::new(
                        "resource_limit",
                        "Directory exceeds 10000 datasets",
                    ));
                }
            }
            entries.sort_by_key(|v| v["path"].as_str().unwrap_or_default().to_owned());
            let fingerprint = digest(&serde_json::json!(entries))?;
            page_values(
                &entries,
                args.offset,
                args.limit,
                serde_json::json!({"listing_sha256":fingerprint}),
            )
        })();
        feature_reply(result)
    }
    #[tool(
        description = "Start any typed shared-engine operation asynchronously. Returns job ID; use analysis_status, cancel_analysis and analysis_result. Computation retains an immutable proposal/result and never commits a project. Existing validation, replay, units and source authorization apply.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn start_analysis(
        &self,
        Parameters(args): Parameters<StartAnalysisArgs>,
    ) -> CallToolResult {
        let parameters = serde_json::json!({"path":args.path,"request":args.request});
        let result = (|| {
            if serde_json::to_vec(&parameters)
                .map_err(|e| EngineError::new("invalid_parameters", e))?
                .len()
                > 16 * 1024 * 1024
            {
                return Err(EngineError::new("resource_limit", "Request exceeds 16 MiB"));
            }
            let path = self.authorize(&args.path)?;
            self.authorize_request(&args.request)?;
            let mut jobs = self
                .feature_jobs
                .lock()
                .map_err(|e| EngineError::new("adapter_failure", e))?;
            if jobs.len() >= 16 {
                return Err(EngineError::new(
                    "resource_limit",
                    "At most 16 retained jobs per session",
                ));
            }
            let handle = self
                .scheduler
                .submit(path, args.request, JobControl::default())?;
            let id = handle.id.0.to_string();
            jobs.insert(
                id.clone(),
                FeatureJob {
                    handle,
                    result: None,
                },
            );
            Ok(serde_json::json!({"job_id":id,"effect":"retained_computation","committed":false}))
        })();
        feature_reply(result)
    }
    #[tool(
        description = "Read any retained analysis job's state, completed work units and explicit failure. Progress counts are kernel-dependent, not a percentage. Results are immutable and paged separately.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn analysis_status(
        &self,
        Parameters(args): Parameters<FeatureJobArgs>,
    ) -> CallToolResult {
        let result = (|| {
            let mut jobs = self
                .feature_jobs
                .lock()
                .map_err(|e| EngineError::new("adapter_failure", e))?;
            let job = jobs
                .get_mut(&args.job_id)
                .ok_or_else(|| EngineError::new("invalid_parameters", "Unknown job"))?;
            if job.result.is_none() {
                job.result = job.handle.try_result();
            }
            Ok(
                serde_json::json!({"job_id":args.job_id,"state":job.handle.state(),"completed_work_units":job.handle.control.completed_scans(),"error":job.result.as_ref().and_then(|r|r.as_ref().err())}),
            )
        })();
        feature_reply(result)
    }
    #[tool(
        description = "Request cooperative cancellation of any analysis job. Kernel cancellation boundaries apply; completion may win the race. Completed results and raw data are preserved.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn cancel_analysis(
        &self,
        Parameters(args): Parameters<FeatureJobArgs>,
    ) -> CallToolResult {
        let result = (|| {
            let jobs = self
                .feature_jobs
                .lock()
                .map_err(|e| EngineError::new("adapter_failure", e))?;
            let job = jobs
                .get(&args.job_id)
                .ok_or_else(|| EngineError::new("invalid_parameters", "Unknown job"))?;
            job.handle.cancel();
            Ok(
                serde_json::json!({"job_id":args.job_id,"state":job.handle.state(),"cancellation_requested":true}),
            )
        })();
        feature_reply(result)
    }
    #[tool(
        description = "Read an immutable job response by RFC6901 JSON pointer. Arrays page 1-1000 elements; strings page 1-1000 Unicode characters; objects return key names and one level of scalar values/array counts. Pointer / selects the literal empty key; empty string selects response. Response SHA256 binds every page to the original result, including parameters and provenance. No retained evidence is changed.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn analysis_result(
        &self,
        Parameters(args): Parameters<ResultPageArgs>,
    ) -> CallToolResult {
        let result = (|| {
            page_limit(args.limit)?;
            let response = self.response_value(&args.job_id)?;
            let hash = digest(&response)?;
            let value = response
                .pointer(&args.pointer)
                .ok_or_else(|| EngineError::new("invalid_parameters", "Unknown JSON pointer"))?;
            let identity = serde_json::json!({"job_id":args.job_id,"response_sha256":hash,"pointer":args.pointer});
            if let Some(values) = value.as_array() {
                return page_values(values, args.offset, args.limit, identity);
            }
            if let Some(text) = value.as_str() {
                let total = text.chars().count();
                if args.offset > total {
                    return Err(EngineError::new("invalid_parameters", "Offset beyond text"));
                }
                let end = args.offset.saturating_add(args.limit).min(total);
                let page: String = text.chars().skip(args.offset).take(args.limit).collect();
                return Ok(
                    serde_json::json!({"identity":identity,"total_characters":total,"offset":args.offset,"next_offset":if end<total{Some(end)}else{None},"text":page}),
                );
            }
            if args.offset != 0 {
                return Err(EngineError::new(
                    "invalid_parameters",
                    "Only arrays support nonzero offsets",
                ));
            }
            let selected = if let Some(map) = value.as_object() {
                serde_json::Value::Object(map.iter().map(|(k,v)| (k.clone(), if let Some(a)=v.as_array(){serde_json::json!({"type":"array","count":a.len()})}else if let Some(o)=v.as_object(){serde_json::json!({"type":"object","keys":o.keys().collect::<Vec<_>>()})}else{v.clone()})).collect())
            } else {
                value.clone()
            };
            Ok(serde_json::json!({"identity":identity,"value":selected}))
        })();
        feature_reply(result)
    }
    #[tool(
        description = "Export a configured full-evidence project review draft as JSON and HTML content. Includes verified artifacts, parameters, source states, reviews and software lock digest. No writes. CLI project-bundle provides portable inputs.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn export_project_report(
        &self,
        Parameters(args): Parameters<ProjectReportArgs>,
    ) -> CallToolResult {
        feature_reply((|| {
            let path = self.authorize(&args.directory)?;
            let project = crate::project::Project::open(&path)?;
            for source in &project.sources {
                if source.path.exists() {
                    self.authorize(&source.path.to_string_lossy())?;
                } else {
                    let parent = source.path.parent().ok_or_else(|| {
                        EngineError::new("unauthorized", "No authorized source parent")
                    })?;
                    self.authorize(&parent.to_string_lossy())?;
                }
            }
            let report = crate::delivery::report(&path, &args.config)?;
            Ok(serde_json::json!({"html":crate::delivery::html(&report)?,"report":report}))
        })())
    }
    #[tool(
        description = "Read an existing project's revision, registered datasets, saved methods and immutable result references. No project writes.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn inspect_project(&self, Parameters(args): Parameters<ProjectArgs>) -> CallToolResult {
        feature_reply((|| {
            let path = self.authorize(&args.directory)?;
            let project = crate::project::Project::open(&path)?;
            serde_json::to_value(project).map_err(|e| EngineError::new("adapter_failure", e))
        })())
    }
    #[tool(
        description = "Prepare a revision-bound review suggestion with original/proposed values, reason and evidence references. No result or project is modified. Save returned proposal JSON for the desktop AI activity review. Numerical validation uses the shared engine.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn prepare_review_proposal(
        &self,
        Parameters(args): Parameters<ProposalArgs>,
    ) -> CallToolResult {
        feature_reply((|| {
            let path = self.authorize(&args.path)?;
            self.authorize_request(&args.request)?;
            let proposal = crate::proposals::prepare(
                &path,
                args.request,
                args.expected_project_revision,
                args.reason,
                args.evidence,
                &JobControl::default(),
            )?;
            serde_json::to_value(proposal).map_err(|e| EngineError::new("adapter_failure", e))
        })())
    }
    #[tool(
        description = "Approve or reject a pending review proposal as a new project revision. Requires operator --allow-project-writes, actor/reason and unchanged project/evidence revisions. Approval identity is attributed, not authenticated. Preserve original results; use a new proposal for reversal.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn resolve_review_proposal(
        &self,
        Parameters(args): Parameters<ResolveProposalArgs>,
    ) -> CallToolResult {
        feature_reply((|| {
            if !self.project_writes {
                return Err(EngineError::new(
                    "unauthorized",
                    "Operator has not enabled project writes",
                ));
            }
            let root = self.authorize(&args.directory)?;
            let mut project = crate::project::Project::open(&root)?;
            let proposal: crate::proposals::Proposal = serde_json::from_value(args.proposal)
                .map_err(|e| EngineError::new("invalid_parameters", e))?;
            self.authorize_request(&proposal.request)?;
            if proposal.expected_project_revision != Some(project.revision) {
                return Err(EngineError::new(
                    "stale_revision",
                    "Project changed since proposal preparation",
                ));
            }
            let dataset = crate::domain::DatasetId(
                uuid::Uuid::parse_str(&args.dataset_id)
                    .map_err(|e| EngineError::new("invalid_parameters", e))?,
            );
            let path = project.verified_source(dataset)?;
            self.authorize(&path.to_string_lossy())?;
            let mut values = vec![];
            if let Some(mut workspace) = project.load_workspace(&root)? {
                if let Some(map) = workspace.as_object_mut() {
                    map.remove("ai");
                }
                values.push(workspace);
            }
            for result in &project.results {
                values.push(
                    serde_json::to_value(project.load_result(&root, result.result_id)?)
                        .map_err(|e| EngineError::new("corrupt_project", e))?,
                );
            }
            let current =
                crate::proposals::find_current(&serde_json::Value::Array(values), &proposal.before)
                    .ok_or_else(|| {
                        EngineError::new(
                            "stale_revision",
                            "Proposal source is absent from this project",
                        )
                    })?;
            let (resolved, response) = crate::proposals::resolve(
                &path,
                &proposal,
                &current,
                Some(project.revision),
                args.approve,
                &args.actor,
                &args.reason,
                &JobControl::default(),
            )?;
            let revision = project.revision;
            if let Some(response) = response {
                project.add_result(&root, dataset, &response)?;
            }
            if let Some(old) = project
                .review_proposals
                .iter_mut()
                .find(|old| old.id == resolved.id)
            {
                *old = resolved.clone();
            } else {
                project.review_proposals.push(resolved.clone());
            }
            project.commit(&root, revision)?;
            Ok(
                serde_json::json!({"proposal":resolved,"revision":project.revision,"effect":if args.approve{"committed_modification"}else{"rejected_suggestion"}}),
            )
        })())
    }
    #[tool(
        description = "Retain a pending review suggestion in a shared project without applying its analytical change. Requires --allow-project-writes and unchanged source/project revision. Desktop Refresh project activity exposes its evidence for approval/rejection. Returns the new revision-bound pending proposal.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn queue_review_proposal(
        &self,
        Parameters(args): Parameters<QueueProposalArgs>,
    ) -> CallToolResult {
        feature_reply((|| {
            if !self.project_writes {
                return Err(EngineError::new(
                    "unauthorized",
                    "Project writes are blocked by server launch policy",
                ));
            }
            let root = self.authorize(&args.directory)?;
            let mut project = crate::project::Project::open(&root)?;
            let revision = project.revision;
            let supplied: crate::proposals::Proposal = serde_json::from_value(args.proposal)
                .map_err(|e| EngineError::new("invalid_parameters", e))?;
            if project
                .review_proposals
                .iter()
                .any(|old| old.id == supplied.id)
            {
                return Err(EngineError::new(
                    "stale_revision",
                    "Proposal identity is already retained; prepare a new suggestion",
                ));
            }
            self.authorize_request(&supplied.request)?;
            let dataset = crate::domain::DatasetId(
                uuid::Uuid::parse_str(&args.dataset_id)
                    .map_err(|e| EngineError::new("invalid_parameters", e))?,
            );
            let path = project.verified_source(dataset)?;
            self.authorize(&path.to_string_lossy())?;
            let mut values = vec![];
            if let Some(mut workspace) = project.load_workspace(&root)? {
                if let Some(map) = workspace.as_object_mut() {
                    map.remove("ai");
                }
                values.push(workspace);
            }
            for artifact in &project.results {
                values.push(
                    serde_json::to_value(project.load_result(&root, artifact.result_id)?)
                        .map_err(|e| EngineError::new("corrupt_project", e))?,
                );
            }
            let current =
                crate::proposals::find_current(&serde_json::Value::Array(values), &supplied.before)
                    .ok_or_else(|| {
                        EngineError::new("stale_revision", "Proposal source is absent from project")
                    })?;
            // Resolution validation is read-only; discard its rejection event. Re-prepare
            // through the engine so imported previews cannot become trusted evidence.
            crate::proposals::resolve(
                &path,
                &supplied,
                &current,
                Some(revision),
                false,
                "project-suggestion-validation",
                "Validate retained suggestion",
                &JobControl::default(),
            )?;
            let mut pending = crate::proposals::prepare(
                &path,
                supplied.request,
                Some(revision.checked_add(1).ok_or_else(|| {
                    EngineError::new("resource_limit", "Project revision is exhausted")
                })?),
                supplied.reason,
                supplied.evidence,
                &JobControl::default(),
            )?;
            pending.id = supplied.id;
            project.review_proposals.push(pending.clone());
            project.commit(&root, revision)?;
            Ok(
                serde_json::json!({"proposal":pending,"revision":project.revision,"effect":"retained_suggestion"}),
            )
        })())
    }
    #[tool(
        description = "Commit a successful retained job to an existing project as a new reversible revision, with expected revision and attributed approval/reason. Requires operator --allow-project-writes. Preview cannot commit. Does not approve scientific release or erase flags. All source hashes/replay and project validation remain enforced.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn commit_analysis(&self, Parameters(args): Parameters<CommitArgs>) -> CallToolResult {
        let result = (|| {
            if !self.project_writes {
                return Err(EngineError::new(
                    "unauthorized",
                    "Operator has not enabled project writes",
                ));
            }
            if args.reason.trim().is_empty() || args.approval_actor.trim().is_empty() {
                return Err(EngineError::new(
                    "invalid_parameters",
                    "Approval actor and reason are required",
                ));
            }
            let root = self.authorize(&args.directory)?;
            let value = self.response_value(&args.job_id)?;
            let response: crate::engine::Response = serde_json::from_value(value)
                .map_err(|e| EngineError::new("adapter_failure", e))?;
            let mut project = crate::project::Project::open(&root)?;
            if project.revision != args.expected_revision {
                return Err(EngineError::new(
                    "stale_revision",
                    "Project revision changed",
                ));
            }
            let dataset_id = crate::domain::DatasetId(
                uuid::Uuid::parse_str(&args.dataset_id)
                    .map_err(|e| EngineError::new("invalid_parameters", e))?,
            );
            if !project.sources.iter().any(|s| s.id == dataset_id) {
                return Err(EngineError::new("invalid_parameters", "Unknown dataset"));
            }
            for source in &project.sources {
                if source.path.exists() {
                    self.authorize(&source.path.to_string_lossy())?;
                } else {
                    let parent = source.path.parent().ok_or_else(|| {
                        EngineError::new("unauthorized", "No authorized source parent")
                    })?;
                    self.authorize(&parent.to_string_lossy())?;
                }
            }
            project.add_result(&root, dataset_id, &response)?;
            project.agent_commits.push(crate::project::AgentCommit {
                result_id: response.result_id,
                revision: args
                    .expected_revision
                    .checked_add(1)
                    .ok_or_else(|| EngineError::new("resource_limit", "Revision overflow"))?,
                actor: args.approval_actor.clone(),
                reason: args.reason.clone(),
                identity_verified: false,
            });
            project.commit(&root, args.expected_revision)?;
            Ok(
                serde_json::json!({"revision":project.revision,"result_id":response.result_id,"effect":"committed_modification","scientific_release_approved":false}),
            )
        })();
        feature_reply(result)
    }
    #[tool(
        description = "Page attributed session tool calls, parameters, observations, proposals, cancellations and applied project changes. In-memory session history; export with prepare_analysis_report before shutdown. Client approvals and AI statements are not independent evidence.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn session_audit(&self, Parameters(args): Parameters<AuditArgs>) -> CallToolResult {
        feature_reply((|| {
            page_limit(args.limit)?;
            let audit = self
                .audit
                .lock()
                .map_err(|e| EngineError::new("adapter_failure", e))?;
            page_values(
                &audit,
                args.offset,
                args.limit,
                serde_json::json!({"scope":"session","durable":false}),
            )
        })())
    }
    #[tool(
        description = "Prepare a reproducible JSON review-draft report from selected retained jobs. Includes original requests, numerical outputs/units/flags, errors or pending states, SHA256 checksums and session audit. AI explanation remains explicitly unverified. Returns content only, never overwrites files or represents an approved release. Large bundles require result/audit paging.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn prepare_analysis_report(
        &self,
        Parameters(args): Parameters<ReportArgs>,
    ) -> CallToolResult {
        let result = (|| {
            if args.job_ids.is_empty() || args.job_ids.len() > 16 || args.explanation.len() > 65536
            {
                return Err(EngineError::new(
                    "invalid_parameters",
                    "Use 1-16 jobs and at most 65536 narrative bytes",
                ));
            }
            let mut artifacts = Vec::new();
            for id in &args.job_ids {
                let state = self
                    .feature_jobs
                    .lock()
                    .map_err(|e| EngineError::new("adapter_failure", e))?
                    .get(id)
                    .ok_or_else(|| EngineError::new("invalid_parameters", "Unknown report job"))?
                    .handle
                    .state();
                match self.response_value(id) {
                    Ok(response) => artifacts.push(serde_json::json!({"job_id":id,"sha256":digest(&response)?,"response":response})),
                    Err(error) => artifacts.push(serde_json::json!({"job_id":id,"state":state,"error":error})),
                }
            }
            let audit = self
                .audit
                .lock()
                .map_err(|e| EngineError::new("adapter_failure", e))?
                .clone();
            let bundle = serde_json::json!({"schema_version":1,"status":"review_draft","scientific_release_approved":false,"ai_explanation":{"text":args.explanation,"independently_verified":false},"artifacts":artifacts,"audit":audit,"limitations":["Session approvals are client-attributed, not authenticated human identity","Missing/failed/pending artifacts and original QC/review states remain visible","Replay requires source hashes and compatible scientific runtimes"]});
            Ok(serde_json::json!({"bundle_sha256":digest(&bundle)?,"bundle":bundle}))
        })();
        feature_reply(result)
    }
    #[tool(
        description = "Start an untargeted_batch version-1 engine request asynchronously. pyOpenMS 3.5.0 required locally. Every mzML source and checkpoint path must be within allowed roots. Returns a job_id for progress, cancellation and feature paging; completed sample checkpoints permit restart."
    )]
    async fn start_untargeted(&self, Parameters(args): Parameters<AnalysisArgs>) -> CallToolResult {
        let result = (|| {
            let path = self.authorize(&args.path)?;
            if serde_json::to_vec(&args.request)
                .map_err(|e| EngineError::new("invalid_parameters", e))?
                .len()
                > 16 * 1024 * 1024
            {
                return Err(EngineError::new("resource_limit", "Request exceeds 16 MiB"));
            }
            let request: Request = serde_json::from_value(args.request)
                .map_err(|e| EngineError::new("invalid_parameters", e))?;
            if let crate::domain::Operation::UntargetedBatch { config } = &request.operation {
                self.authorize_features(config)?;
            } else {
                return Err(EngineError::new(
                    "invalid_parameters",
                    "Requires untargeted_batch",
                ));
            }
            let mut jobs = self
                .feature_jobs
                .lock()
                .map_err(|e| EngineError::new("adapter_failure", e))?;
            if jobs.len() >= 16 {
                return Err(EngineError::new("resource_limit","At most 16 retained feature jobs per MCP server; export evidence then restart server"));
            }
            let handle = self
                .scheduler
                .submit(path, request, JobControl::default())?;
            let id = handle.id.0.to_string();
            jobs.insert(
                id.clone(),
                FeatureJob {
                    handle,
                    result: None,
                },
            );
            Ok(serde_json::json!({"job_id":id}))
        })();
        feature_reply(result)
    }
    #[tool(
        description = "Read untargeted job state, completed sample phases and matrix summary. Failure/cancellation stays explicit; poll feature pages after success."
    )]
    async fn untargeted_status(
        &self,
        Parameters(args): Parameters<FeatureJobArgs>,
    ) -> CallToolResult {
        let result: crate::domain::Result<serde_json::Value> = (|| {
            let mut jobs = self
                .feature_jobs
                .lock()
                .map_err(|e| EngineError::new("adapter_failure", e))?;
            let job = jobs
                .get_mut(&args.job_id)
                .ok_or_else(|| EngineError::new("invalid_parameters", "Unknown feature job"))?;
            if job.result.is_none() {
                job.result = job.handle.try_result();
            }
            let summary = match &job.result {
                Some(Ok(r)) => match &r.output {
                    crate::engine::Output::FeatureMatrix { report } => {
                        serde_json::json!({"result_id":r.result_id,"features":report.features.len(),"samples":report.config.samples,"alignments":report.alignments,"provenance":report.provenance,"warnings":report.warnings})
                    }
                    _ => serde_json::Value::Null,
                },
                Some(Err(e)) => serde_json::json!({"error":e}),
                None => serde_json::Value::Null,
            };
            Ok(
                serde_json::json!({"state":job.handle.state(),"completed_sample_phases":job.handle.control.completed_scans(),"summary":summary}),
            )
        })();
        feature_reply(result)
    }
    #[tool(
        description = "Cancel an untargeted job. The local OpenMS process is terminated and completed source-hashed sample checkpoints remain resumable."
    )]
    async fn cancel_untargeted(
        &self,
        Parameters(args): Parameters<FeatureJobArgs>,
    ) -> CallToolResult {
        let result: crate::domain::Result<serde_json::Value> = (|| {
            let jobs = self
                .feature_jobs
                .lock()
                .map_err(|e| EngineError::new("adapter_failure", e))?;
            let job = jobs
                .get(&args.job_id)
                .ok_or_else(|| EngineError::new("invalid_parameters", "Unknown feature job"))?;
            job.handle.cancel();
            Ok(serde_json::json!({"job_id":args.job_id,"cancellation_requested":true}))
        })();
        feature_reply(result)
    }
    #[tool(
        description = "Page a completed untargeted feature matrix, 1-100 rows per request. include_evidence returns linked EIC/apex spectra/MS2; false retains metadata, values, states and grouping without large arrays. Never changes retained results."
    )]
    async fn untargeted_features(
        &self,
        Parameters(args): Parameters<FeaturePageArgs>,
    ) -> CallToolResult {
        let result = (|| {
            if !(1..=100).contains(&args.limit) {
                return Err(EngineError::new("invalid_parameters", "Use 1-100 rows"));
            }
            let mut jobs = self
                .feature_jobs
                .lock()
                .map_err(|e| EngineError::new("adapter_failure", e))?;
            let job = jobs
                .get_mut(&args.job_id)
                .ok_or_else(|| EngineError::new("invalid_parameters", "Unknown feature job"))?;
            if job.result.is_none() {
                job.result = job.handle.try_result();
            }
            let response = job
                .result
                .as_ref()
                .ok_or_else(|| EngineError::new("not_ready", "Feature job has no result"))?
                .as_ref()
                .map_err(Clone::clone)?;
            let report = match &response.output {
                crate::engine::Output::FeatureMatrix { report } => report,
                _ => return Err(EngineError::new("adapter_failure", "Wrong job output")),
            };
            if args.offset > report.features.len() {
                return Err(EngineError::new(
                    "invalid_parameters",
                    "Offset beyond matrix",
                ));
            }
            let mut rows = report
                .features
                .iter()
                .skip(args.offset)
                .take(args.limit)
                .cloned()
                .collect::<Vec<_>>();
            if !args.include_evidence {
                for row in &mut rows {
                    for cell in &mut row.cells {
                        cell.eic.clear();
                        cell.apex_spectrum.clear();
                        cell.ms2.clear();
                    }
                }
            }
            let value = serde_json::json!({"result_id":response.result_id,"total":report.features.len(),"offset":args.offset,"features":rows});
            if serde_json::to_vec(&value)
                .map_err(|e| EngineError::new("adapter_failure", e))?
                .len()
                > 16 * 1024 * 1024
            {
                return Err(EngineError::new(
                    "resource_limit",
                    "Page exceeds 16 MiB; reduce limit or omit evidence",
                ));
            }
            Ok(value)
        })();
        feature_reply(result)
    }
    #[tool(
        description = "Execute propose_feature_annotations (ledger/config/expected_revision; bounded formula/isotope and optional reproducible local library search), add_feature_hypothesis (ledger/hypothesis/expected_revision), review_feature_annotation (ledger/hypothesis_id/expected_revision/reason/decision accept/reject/reopen), export_feature_annotations (ledger; CSV). Feature identities remain tentative; acceptance never confirms identity. Also execute linked_spectral_chromatogram (source-verified precursor MS1 XIC), compare_spectra (centroid query/reference/tolerance), export_spectral_candidates (report; CSV), inspect_spectra (scan indices/background_indices/config), process_spectra (spectra/background/config), import_spectral_library (text, msp/mgf/massbank format, source/license), search_spectral_library (query/library/config), annotate_spectrum (report/expected_revision/annotation), formula_candidates (CHNOPS bounds/adducts), analyze_isotopes (centroid MS1/config). Similarity never confirms identity. Also execute evaluate_qc (study), validate_method (explicit method_validation study), evaluate_targeted_qc (verified batch/rules), review_qc (report/expected_revision/rule_id/reason/acknowledged) or export_qc (report; CSV). QC decisions retain evidence/calculation/threshold/reason and required missing evidence is indeterminate. Also execute targeted_batch (sample sheet, targets, calibration), review_targeted (batch, expected_revision, sample, target, accepted, reason), export_targeted (batch; CSV) and shared engine metadata, spectrum, extract, integrate, quantify, process_chromatograms (traces/config), revise_chromatogram (analysis/expected_revision/correction/preview) or export_chromatographic_peaks (analyses; returns CSV text). Chromatography supports SG, AsLS/chord/quantile baselines, batch peaks, manual/reference bounds, accept and restore revisions. Missing intensity is null. Paths must be within allowed roots. Version 1 request matches CLI JSON; results preserve signal stages, units and history. Preview is never committed. Statistics: analyze_statistics (table/settings) and export_statistics (report) use explicit frozen metadata, imputation, normalization, transformations, PCA, Euclidean clustering and Welch/BH/BY comparisons through local NumPy/SciPy. No project writes or GUI changes."
    )]
    async fn analytical_operation(
        &self,
        Parameters(args): Parameters<AnalysisArgs>,
    ) -> CallToolResult {
        let result: crate::domain::Result<String> = async {
            let path = self.authorize(&args.path)?;
            let bytes = serde_json::to_vec(&args.request)
                .map_err(|e| EngineError::new("invalid_parameters", e))?;
            if bytes.len() > 16 * 1024 * 1024 {
                return Err(EngineError::new("resource_limit", "Request exceeds 16 MiB"));
            }
            let request: Request = serde_json::from_slice(&bytes)
                .map_err(|e| EngineError::new("invalid_parameters", e))?;
            if let crate::domain::Operation::TargetedBatch { batch } = &request.operation {
                for sample in &batch.samples {
                    self.authorize(&sample.source)?;
                }
            }
            if let crate::domain::Operation::UntargetedBatch { config } = &request.operation {
                self.authorize_features(config)?;
            }
            let job = self
                .scheduler
                .submit(path, request, JobControl::default())?;
            let response = tokio::task::spawn_blocking(move || job.wait())
                .await
                .map_err(|e| EngineError::new("adapter_failure", e))??;
            let text = serde_json::to_string(&response)
                .map_err(|e| EngineError::new("adapter_failure", e))?;
            if text.len() > 16 * 1024 * 1024 {
                return Err(EngineError::new(
                    "resource_limit",
                    "Result exceeds 16 MiB; use CLI for full arrays",
                ));
            }
            Ok(text)
        }
        .await;
        match result {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(value) => CallToolResult::structured(value),
                Err(error) => CallToolResult::structured_error(
                    serde_json::json!({"code":"adapter_failure","message":error.to_string()}),
                ),
            },
            Err(error) => CallToolResult::structured_error(serde_json::json!(error)),
        }
    }
}
#[tool_handler(router = self.tool_router)]
impl ServerHandler for EngineMcp {
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<CallToolResponse, rmcp::ErrorData> {
        let name = request.name.to_string();
        let parameters = serde_json::json!(request.arguments);
        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        let result = self.tool_router.call(call).await;
        let observation = match &result {
            Ok(CallToolResponse::Complete(reply)) => {
                if let Some(value) = &reply.structured_content {
                    let observed = if reply.is_error == Some(true) {
                        serde_json::json!({"ok":false,"error":value})
                    } else {
                        outcome(&Ok(value.clone()))
                    };
                    serde_json::json!({"is_error":reply.is_error.unwrap_or(false),"observation":observed})
                } else {
                    serde_json::json!({"is_error":reply.is_error.unwrap_or(false)})
                }
            }
            Ok(_) => serde_json::json!({"protocol_state":"not_complete"}),
            Err(error) => serde_json::json!({"protocol_error":error}),
        };
        self.record(&name, parameters, observation);
        result
    }
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> std::result::Result<ListToolsResult, rmcp::ErrorData> {
        let legacy = [
            "analytical_operation",
            "cancel_untargeted",
            "start_untargeted",
            "untargeted_features",
            "untargeted_status",
        ];
        let mut tools = self.tool_router.list_all();
        tools.sort_by_key(|tool| {
            (
                legacy
                    .iter()
                    .position(|name| *name == tool.name)
                    .unwrap_or(legacy.len()),
                tool.name.to_string(),
            )
        });
        let supports_cache_hints = context
            .protocol_version()
            .is_some_and(|version| version >= ProtocolVersion::V_2026_07_28);
        Ok(ListToolsResult {
            result_type: Some(ResultType::COMPLETE),
            tools,
            meta: None,
            next_cursor: None,
            ttl_ms: supports_cache_hints.then_some(0),
            cache_scope: supports_cache_hints.then_some(CacheScope::Public),
        })
    }
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("chromascope-engine",env!("CARGO_PKG_VERSION")))
            .with_instructions("Headless analytical capability. Discover typed engine operations and workflow recipes with analysis_capabilities; start_analysis provides retained jobs, status/cancellation and immutable result paging. Reports are reproducible review drafts; AI narratives and client approvals are not independently verified scientific evidence. Project commits require explicit operator policy and expected revisions. Legacy visual tools are provided by chromascope-mcp. Scientific units: RT minutes, area instrument intensity * minute. Targeted operations explicitly return calibrated concentrations with declared units, dilution, flags and reversible review history.")
    }
}

#[cfg(test)]
mod interface_tests {
    use super::*;
    #[test]
    fn immutable_paging_bounds_and_checksums() {
        let values = vec![
            serde_json::Value::Null,
            serde_json::json!({"value":0,"unit":"ng/mL"}),
        ];
        let page = page_values(
            &values,
            0,
            1,
            serde_json::json!({"sha256":digest(&serde_json::json!(values)).unwrap()}),
        )
        .unwrap();
        assert!(page["items"][0].is_null());
        assert_eq!(page["next_offset"], 1);
        let last = page_values(&values, 1, usize::MAX, serde_json::Value::Null).unwrap();
        assert_eq!(last["items"][0]["value"], 0);
        assert!(last["next_offset"].is_null());
        assert!(page_values(&values, 3, 1, serde_json::Value::Null).is_err());
        assert!(page_limit(0).is_err());
        assert!(page_limit(1001).is_err());
        assert_ne!(
            digest(&serde_json::json!({"value":null})).unwrap(),
            digest(&serde_json::json!({"value":0})).unwrap()
        );
    }
}
