//! Tests that talk to the desktop server through the real MCP protocol: JSON-RPC lines over an
//! in-memory transport, with the window polled on this thread exactly as the GUI loop does.
use super::*;
use crate::mcp::{AccessPolicy, McpServer};
use rmcp::ServiceExt;
use std::{path::PathBuf, sync::Arc};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};

include!("../../../tests/fixtures/targeted_support.rs");

struct Io {
    read: BufReader<ReadHalf<DuplexStream>>,
    write: WriteHalf<DuplexStream>,
    id: u64,
}

/// A window plus a connected MCP client.
pub(super) struct Desktop {
    pub app: MzViewerApp,
    pub ctx: egui::Context,
    runtime: tokio::runtime::Runtime,
    io: Arc<tokio::sync::Mutex<Io>>,
    server: tokio::task::JoinHandle<()>,
}

pub(super) fn fixture_mzml() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_file/data_dependent_02.mzML")
}

impl Desktop {
    /// The repository root is always an allowed root; `extra` adds further directories.
    pub(super) fn start(
        extra: Vec<PathBuf>,
        changes: bool,
        exports: bool,
        project_writes: bool,
    ) -> Self {
        let mut roots = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))];
        roots.extend(extra);
        // Keep a test window's recent files and settings out of the developer's own.
        static CONFIG: std::sync::Once = std::sync::Once::new();
        CONFIG.call_once(|| {
            let directory = std::env::temp_dir().join("chromascope-test-config");
            std::fs::create_dir_all(&directory).unwrap();
            std::env::set_var("CHROMASCOPE_CONFIG_DIR", directory);
        });
        let policy = AccessPolicy::new(roots, changes, exports)
            .unwrap()
            .with_project_writes(project_writes);
        let ctx = egui::Context::default();
        let (server, bridge) = McpServer::new(ctx.clone(), policy).unwrap();
        let mut app = MzViewerApp::default();
        app.attach_mcp(bridge);
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (client, transport) = tokio::io::duplex(8 * 1024 * 1024);
        let server_task = runtime.spawn(async move {
            if let Ok(service) = server.serve(transport).await {
                let _ = service.waiting().await;
            }
        });
        let (read, write) = tokio::io::split(client);
        let mut desktop = Self {
            app,
            ctx,
            runtime,
            io: Arc::new(tokio::sync::Mutex::new(Io {
                read: BufReader::new(read),
                write,
                id: 0,
            })),
            server: server_task,
        };
        let init = desktop.rpc(
            "initialize",
            json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"desktop test","version":"1"}}),
        );
        assert!(
            init["result"]["capabilities"]["tools"].is_object(),
            "{init}"
        );
        desktop.notify("notifications/initialized");
        desktop
    }
    fn notify(&mut self, method: &str) {
        let io = self.io.clone();
        let line = format!("{}\n", json!({"jsonrpc":"2.0","method":method}));
        self.runtime.block_on(async move {
            io.lock()
                .await
                .write
                .write_all(line.as_bytes())
                .await
                .unwrap();
        });
    }
    /// One window frame's worth of the work `update` does for MCP-driven state.
    pub(super) fn pump(&mut self) {
        let ctx = self.ctx.clone();
        super::super::project_workbench::poll(&mut self.app, &ctx);
        super::super::ai_review::poll(&mut self.app, &ctx);
        self.app.poll_remote(&ctx);
        super::super::presets::poll(&mut self.app, &ctx);
        self.app.poll_file_loading_result(&ctx);
        super::super::workspace::restore_loaded(&mut self.app, &ctx);
        self.app.poll_processing_result(&ctx);
        self.app.process_pending_update();
        super::super::quant::poll(&mut self.app, &ctx);
    }
    /// Pumps the window until `done` holds (or fails after `seconds`).
    pub(super) fn pump_until(&mut self, seconds: u64, mut done: impl FnMut(&mut Self) -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
        while !done(self) {
            assert!(std::time::Instant::now() < deadline, "timed out waiting");
            self.pump();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    pub(super) fn rpc(&mut self, method: &str, params: Value) -> Value {
        let io = self.io.clone();
        let method = method.to_owned();
        let handle = self.runtime.spawn(async move {
            let mut io = io.lock().await;
            io.id += 1;
            let line = format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":io.id,"method":method,"params":params})
            );
            io.write.write_all(line.as_bytes()).await.unwrap();
            let mut reply = String::new();
            tokio::time::timeout(
                std::time::Duration::from_secs(120),
                io.read.read_line(&mut reply),
            )
            .await
            .expect("no reply")
            .unwrap();
            serde_json::from_str::<Value>(&reply).unwrap()
        });
        while !handle.is_finished() {
            self.pump();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        self.runtime.block_on(handle).unwrap()
    }
    /// The whole `tools/call` reply.
    pub(super) fn call(&mut self, name: &str, arguments: Value) -> Value {
        self.rpc("tools/call", json!({"name":name,"arguments":arguments}))
    }
    /// The structured result of a call that must succeed.
    pub(super) fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let reply = self.call(name, arguments);
        assert!(reply["error"].is_null(), "{name}: {reply}");
        assert_ne!(reply["result"]["isError"], true, "{name}: {reply}");
        reply["result"]["structuredContent"].clone()
    }
    /// The structured error of a call that must fail (a tool error, not a protocol error).
    pub(super) fn tool_error(&mut self, name: &str, arguments: Value) -> Value {
        let reply = self.call(name, arguments);
        assert_eq!(reply["result"]["isError"], true, "{name}: {reply}");
        reply["result"]["structuredContent"].clone()
    }
    pub(super) fn tools(&mut self) -> Vec<Value> {
        let listed = self.rpc("tools/list", json!({}));
        listed["result"]["tools"].as_array().unwrap().clone()
    }
    /// Opens the shipped fixture and returns its dataset id.
    pub(super) fn open_fixture(&mut self) -> u64 {
        let opened = self.tool("open_files", json!({"paths":[fixture_mzml()]}));
        assert_eq!(opened["files"][0]["success"], true, "{opened}");
        opened["files"][0]["dataset_id"].as_u64().unwrap()
    }
    pub(super) fn finish(self) {
        self.server.abort();
    }
}

fn start_request(operation: crate::domain::Operation) -> Value {
    serde_json::to_value(crate::domain::Request {
        version: 1,
        operation_id: Default::default(),
        actor: "desktop protocol test".into(),
        operation,
    })
    .unwrap()
}

fn wait_for_job(desktop: &mut Desktop, id: &str) -> Value {
    for _ in 0..2000 {
        let status = desktop.tool("analysis_status", json!({"job_id":id}));
        match status["state"].as_str().unwrap() {
            "succeeded" => return status,
            "failed" | "cancelled" => panic!("{status}"),
            _ => std::thread::sleep(std::time::Duration::from_millis(5)),
        }
    }
    panic!("job did not finish");
}

#[test]
fn one_server_lists_viewer_and_engine_tools_with_unique_descriptions() {
    let mut desktop = Desktop::start(vec![], false, false, false);
    let tools = desktop.tools();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for viewer in [
        "open_files",
        "extract_chromatogram",
        "integrate",
        "start_quantification",
    ] {
        assert!(names.contains(&viewer), "missing viewer tool {viewer}");
    }
    for engine in [
        "start_analysis",
        "analysis_status",
        "analysis_result",
        "cancel_analysis",
        "inspect_project",
        "export_project_report",
        "prepare_review_proposal",
        "queue_review_proposal",
        "resolve_review_proposal",
        "commit_analysis",
        "analysis_capabilities",
        "discover_datasets",
        "session_audit",
        "prepare_analysis_report",
        "analytical_operation",
    ] {
        assert!(names.contains(&engine), "missing engine tool {engine}");
    }
    let mut seen = std::collections::HashMap::new();
    for tool in &tools {
        let description = tool["description"].as_str().unwrap_or("").trim().to_owned();
        assert!(
            description.len() >= 40,
            "{} needs a description",
            tool["name"]
        );
        if let Some(other) = seen.insert(description, tool["name"].to_string()) {
            panic!("{other} and {} share a description", tool["name"]);
        }
        assert!(
            tool["annotations"]["readOnlyHint"].is_boolean()
                && tool["annotations"]["destructiveHint"].is_boolean(),
            "{} lacks annotations",
            tool["name"]
        );
    }
    let unique: std::collections::HashSet<_> = names.iter().collect();
    assert_eq!(unique.len(), names.len(), "a tool name is listed twice");
    desktop.finish();
}

#[test]
fn engine_tools_of_the_desktop_server_match_the_headless_server() {
    // The headless server is a subset: same names, same descriptions, same hints.
    let mut desktop = Desktop::start(vec![], false, false, false);
    let listed = desktop.tools();
    let headless =
        crate::engine_mcp::EngineMcp::new(vec![PathBuf::from(env!("CARGO_MANIFEST_DIR"))]).unwrap();
    for name in headless.tool_names() {
        let hidden = !crate::EXPERIMENTAL
            && [
                "start_untargeted",
                "untargeted_status",
                "cancel_untargeted",
                "untargeted_features",
            ]
            .contains(&name.as_str());
        let found = listed.iter().find(|t| t["name"] == name.as_str());
        if hidden {
            assert!(
                found.is_none(),
                "{name} must stay hidden in the default build"
            );
        } else {
            let tool = found.unwrap_or_else(|| panic!("{name} missing from the desktop server"));
            assert!(tool["description"].as_str().unwrap().len() > 20);
        }
    }
    desktop.finish();
}

#[test]
fn calibration_quantification_and_qc_appear_in_the_quantification_tab() {
    let temp = tempfile::tempdir().unwrap();
    let batch = raw_request(temp.path());
    let source = batch.samples[0].source.clone();
    let mut desktop = Desktop::start(vec![temp.path().to_owned()], true, false, false);
    assert!(!desktop.app.quant.has_targeted_batch());
    // 1. Build a calibration and quantify the unknowns on the shared engine.
    let started = desktop.tool(
        "start_analysis",
        json!({"path":source,"request":start_request(crate::domain::Operation::TargetedBatch{batch})}),
    );
    let job = started["job_id"].as_str().unwrap().to_owned();
    assert_eq!(started["window"]["state"], "waiting", "{started}");
    wait_for_job(&mut desktop, &job);
    desktop.pump_until(10, |d| d.app.quant.has_targeted_batch());
    // The person sees it in the Quantification tab without reopening anything.
    let status = desktop.tool("analysis_status", json!({"job_id":job}));
    assert_eq!(status["window"]["state"], "shown", "{status}");
    assert_eq!(status["window"]["tab"], "Quantification");
    let (calibrated, _, _) = desktop.app.quant.calibration_summary();
    assert!(calibrated > 0, "the calibration curve is in the tab");
    // 2. Evaluate QC on the quantified batch; the report lands in the same tab.
    let page = desktop.tool(
        "analysis_result",
        json!({"job_id":job,"pointer":"/output/batch","offset":0,"limit":10}),
    );
    assert!(page["value"].is_object(), "{page}");
    let response = {
        let report = desktop.tool(
            "prepare_analysis_report",
            json!({"job_ids":[job],"explanation":"test"}),
        );
        report["bundle"]["artifacts"][0]["response"].clone()
    };
    let batch: crate::targeted::BatchResult =
        serde_json::from_value(response["output"]["batch"].clone()).unwrap();
    let rule: crate::qc::Rule = serde_json::from_value(json!({"id":"strict_accuracy","target":"a","group":null,"role":"qc","batch":null,"metric":"accuracy","lower":99.9,"upper":100.1,"minimum_n":2,"required":true,"reference_group":null,"calibration":null})).unwrap();
    let qc = desktop.tool(
        "start_analysis",
        json!({"path":source,"request":start_request(crate::domain::Operation::EvaluateTargetedQc{batch:Box::new(batch),rules:vec![rule]})}),
    );
    let qc_job = qc["job_id"].as_str().unwrap().to_owned();
    wait_for_job(&mut desktop, &qc_job);
    desktop.pump_until(10, |d| d.app.quant.qc_report_count() == 1);
    // 3. Both calls are in the AI activity log.
    let history = &desktop.app.remote.as_ref().unwrap().history;
    let operations: Vec<&str> = history
        .iter()
        .filter_map(|e| e["request"]["operation"].as_str())
        .collect();
    assert!(
        operations
            .iter()
            .filter(|o| **o == "start_analysis")
            .count()
            >= 2,
        "{operations:?}"
    );
    assert!(
        operations.contains(&"show_analysis_result"),
        "{operations:?}"
    );
    desktop.finish();
}

#[test]
fn without_allow_changes_the_result_stays_an_mcp_job() {
    let temp = tempfile::tempdir().unwrap();
    let batch = raw_request(temp.path());
    let source = batch.samples[0].source.clone();
    let mut desktop = Desktop::start(vec![temp.path().to_owned()], false, false, false);
    let started = desktop.tool(
        "start_analysis",
        json!({"path":source,"request":start_request(crate::domain::Operation::TargetedBatch{batch})}),
    );
    let job = started["job_id"].as_str().unwrap().to_owned();
    assert_eq!(started["window"]["state"], "not_shown");
    assert!(started["window"]["reason"]
        .as_str()
        .unwrap()
        .contains("--allow-changes"));
    let done = wait_for_job(&mut desktop, &job);
    assert_eq!(done["window"]["state"], "not_shown");
    for _ in 0..50 {
        desktop.pump();
    }
    assert!(
        !desktop.app.quant.has_targeted_batch(),
        "the window must not change"
    );
    desktop.finish();
}

#[test]
fn desktop_integrate_uses_the_engine_path_and_result_shape() {
    let mut desktop = Desktop::start(vec![], true, false, false);
    let id = desktop.open_fixture();
    desktop.tool(
        "extract_chromatogram",
        json!({"dataset_id":id,"kind":"tic","polarity":"positive","ms_level":1,"smoothing":0,"display":true}),
    );
    let data = desktop.tool(
        "chromatogram_data",
        json!({"dataset_id":id,"offset":0,"limit":10000}),
    );
    let points: Vec<[f64; 2]> = serde_json::from_value(data["points"].clone()).unwrap();
    let (start, end) = (points[0][0], points[points.len() - 1][0]);
    let integrated = desktop.tool(
        "integrate",
        json!({"dataset_id":id,"start_minutes":start,"end_minutes":end,"apply":false}),
    );
    // The same numbers as start_analysis with the engine's integrate operation.
    let started = desktop.tool(
        "start_analysis",
        json!({"path":fixture_mzml(),"request":start_request(crate::domain::Operation::Integrate{points:points.clone(),start:crate::domain::Minutes(start),end:crate::domain::Minutes(end)})}),
    );
    let job = started["job_id"].as_str().unwrap().to_owned();
    wait_for_job(&mut desktop, &job);
    let engine = desktop.tool(
        "analysis_result",
        json!({"job_id":job,"pointer":"/output","offset":0,"limit":10}),
    );
    for key in ["kind", "area", "area_unit", "algorithm"] {
        assert_eq!(
            integrated[key], engine["value"][key],
            "{key}: {integrated} vs {engine}"
        );
    }
    assert_eq!(integrated["algorithm"], "chord_trapezoid_v1");
    assert_eq!(integrated["applied"], false);
    // Out-of-range bounds are refused by the engine's own check.
    let error = desktop.tool_error(
        "integrate",
        json!({"dataset_id":id,"start_minutes":start,"end_minutes":end + 1.0,"apply":false}),
    );
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("inside the trace"),
        "{error}"
    );
    desktop.finish();
}

/// `(dataset_id, stable_id)` of every open dataset, once the window has hashed them all.
fn stable_ids(desktop: &mut Desktop) -> Vec<(u64, String)> {
    for _ in 0..2000 {
        let listed = desktop.tool("list_datasets", json!({}));
        let datasets = listed["datasets"].as_array().unwrap();
        if datasets.iter().all(|d| d["stable_id"].is_string()) {
            return datasets
                .iter()
                .map(|d| {
                    (
                        d["dataset_id"].as_u64().unwrap(),
                        d["stable_id"].as_str().unwrap().to_owned(),
                    )
                })
                .collect();
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("stable ids were not computed");
}

#[test]
fn stable_dataset_ids_name_the_same_data_after_a_restart() {
    use sha2::{Digest, Sha256};
    let temp = tempfile::tempdir().unwrap();
    let batch = raw_request(temp.path());
    let (first, second) = (
        batch.samples[3].source.clone(),
        batch.samples[5].source.clone(),
    );
    let sha = |path: &str| format!("sha256:{:x}", Sha256::digest(std::fs::read(path).unwrap()));
    // One session opens first, then second; the next session opens them the other way round.
    let mut one = Desktop::start(vec![temp.path().to_owned()], true, false, false);
    one.tool(
        "open_files",
        json!({"paths":[first.clone(), second.clone()]}),
    );
    let ids = stable_ids(&mut one);
    assert_eq!(ids.len(), 2);
    assert_eq!(
        ids[0].1,
        sha(&first),
        "the stable id is the SHA-256 of the source file"
    );
    assert_eq!(ids[1].1, sha(&second));
    let mut two = Desktop::start(vec![temp.path().to_owned()], true, false, false);
    two.tool(
        "open_files",
        json!({"paths":[second.clone(), first.clone()]}),
    );
    let swapped = stable_ids(&mut two);
    assert_eq!(
        swapped[0].1,
        sha(&second),
        "the numbers differ between sessions"
    );
    for desktop in [&mut one, &mut two] {
        for (path, reference) in [(&first, sha(&first)), (&second, sha(&second))] {
            let by_stable = desktop.tool("dataset_metadata", json!({"dataset_id":reference}));
            assert!(
                by_stable["path"].as_str().unwrap().ends_with(
                    std::path::Path::new(path)
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                ),
                "{by_stable}"
            );
            // Bare hex and upper case name the same dataset; so does the plain number.
            let bare = reference.trim_start_matches("sha256:").to_ascii_uppercase();
            let by_bare = desktop.tool("dataset_metadata", json!({"dataset_id":bare}));
            assert_eq!(by_bare["dataset_id"], by_stable["dataset_id"]);
            let by_number = desktop.tool(
                "dataset_metadata",
                json!({"dataset_id":by_stable["dataset_id"]}),
            );
            assert_eq!(by_number["path"], by_stable["path"]);
        }
    }
    // Inside a batch, and for the commands that take a list.
    let in_batch = one.tool(
        "batch",
        json!({"commands":[{"operation":"dataset_metadata","dataset_id":sha(&second)}]}),
    );
    assert_eq!(
        in_batch["outcomes"][0]["result"]["structuredContent"]["scan_count"], 5,
        "{in_batch}"
    );
    // Unknown ids and ambiguous ids are explained, never guessed.
    let missing = one.tool_error(
        "dataset_metadata",
        json!({"dataset_id":sha(batch.samples[0].source.as_str())}),
    );
    assert!(
        missing["message"]
            .as_str()
            .unwrap()
            .contains("No open dataset has stable id"),
        "{missing}"
    );
    let garbage = one.tool_error("dataset_metadata", json!({"dataset_id":"not-an-id"}));
    assert!(
        garbage["message"].as_str().unwrap().contains("stable id"),
        "{garbage}"
    );
    one.tool("open_files", json!({"paths":[first.clone()]}));
    stable_ids(&mut one);
    let ambiguous = one.tool_error("dataset_metadata", json!({"dataset_id":sha(&first)}));
    assert!(
        ambiguous["message"]
            .as_str()
            .unwrap()
            .contains("matches 2 open datasets"),
        "{ambiguous}"
    );
    one.finish();
    two.finish();
}

/// Compiles the fixture that stands in for ProteoWizard's msconvert.
fn fake_msconvert(directory: &std::path::Path) -> PathBuf {
    let executable = directory.join(if cfg!(windows) {
        "msconvert.exe"
    } else {
        "msconvert"
    });
    let compiled = std::process::Command::new("rustc")
        .args(["--edition=2021"])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/fake_msconvert.rs"
        ))
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    executable
}

#[test]
fn vendor_data_opens_through_msconvert_and_stays_inside_the_roots() {
    let temp = tempfile::tempdir().unwrap();
    let mut desktop = Desktop::start(vec![temp.path().to_owned()], true, false, false);
    let vendor = temp.path().join("run one.raw");
    std::fs::write(&vendor, b"vendor bytes").unwrap();
    std::fs::write(temp.path().join("notes.txt"), b"not data").unwrap();
    // Vendor datasets are listed on request only.
    let plain = desktop.tool("list_mzml_files", json!({"directory":temp.path()}));
    assert!(plain.get("vendor_datasets").is_none(), "{plain}");
    let listed = desktop.tool(
        "list_mzml_files",
        json!({"directory":temp.path(),"include_vendor":true}),
    );
    assert_eq!(
        listed["vendor_datasets"].as_array().unwrap().len(),
        1,
        "{listed}"
    );
    assert_eq!(
        listed["msconvert_configured"],
        desktop.app.msconvert_path.is_some()
    );
    // No converter configured: the file is refused with the reason, nothing is opened.
    desktop.app.msconvert_path = None;
    let refused = desktop.tool("open_files", json!({"paths":[vendor.clone()]}));
    assert_eq!(refused["files"][0]["success"], false, "{refused}");
    assert!(refused["files"][0]["error"]
        .as_str()
        .unwrap()
        .contains("ProteoWizard"));
    assert!(desktop.app.files.is_empty());
    // With msconvert the run is converted; a two-sample file gives two datasets.
    desktop.app.msconvert_path = Some(fake_msconvert(temp.path()));
    let opened = desktop.tool("open_files", json!({"paths":[vendor.clone()]}));
    let outcome = &opened["files"][0];
    assert_eq!(outcome["success"], true, "{opened}");
    assert_eq!(outcome["vendor"], true);
    let datasets = outcome["datasets"].as_array().unwrap();
    assert_eq!(datasets.len(), 2);
    assert_eq!(datasets[1]["name"], "run one.raw / sample 2");
    // The converted runs live in a temporary folder outside every root; access is judged by
    // the file that was chosen, so every dataset tool still works on them.
    let id = datasets[1]["dataset_id"].as_u64().unwrap();
    let extracted = desktop.tool(
        "extract_chromatogram",
        json!({"dataset_id":id,"kind":"tic","polarity":"positive","ms_level":1,"smoothing":0,"display":true}),
    );
    assert!(
        extracted["point_count"].as_u64().unwrap() > 10,
        "{extracted}"
    );
    assert_eq!(extracted["displayed"], true);
    let metadata = desktop.tool("dataset_metadata", json!({"dataset_id":id}));
    let direct = desktop.open_fixture();
    let expected = desktop.tool("dataset_metadata", json!({"dataset_id":direct}));
    assert!(expected["scan_count"].as_u64().unwrap() > 0);
    assert_eq!(
        metadata["scan_count"], expected["scan_count"],
        "the converted run is the fixture"
    );
    // Anything outside the roots, and anything that is not data, is refused before it is read.
    let other = tempfile::tempdir().unwrap();
    let outside = other.path().join("outside.raw");
    std::fs::write(&outside, b"x").unwrap();
    let error = desktop.tool_error("open_files", json!({"paths":[outside]}));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("outside authorized roots"),
        "{error}"
    );
    let error = desktop.tool_error(
        "open_files",
        json!({"paths":[temp.path().join("notes.txt")]}),
    );
    assert!(
        error["message"].as_str().unwrap().contains("Only mzML"),
        "{error}"
    );
    desktop.finish();
}

#[test]
fn stored_chromatograms_are_listed_and_extracted() {
    let directory = tempfile::tempdir().unwrap();
    let path = crate::parser::tests::write_chromatogram_only_mzml(&directory, "srm.mzML");
    let mut desktop = Desktop::start(vec![directory.path().to_owned()], true, false, false);
    let opened = desktop.tool("open_files", json!({"paths":[path.clone()]}));
    let id = opened["files"][0]["dataset_id"].as_u64().unwrap();
    // The stored chromatograms are listed with their transitions.
    let metadata = desktop.tool("dataset_metadata", json!({"dataset_id":id}));
    assert_eq!(metadata["has_spectra"], false);
    let stored = metadata["chromatograms"].as_array().unwrap();
    assert_eq!(stored.len(), 2, "{metadata}");
    assert_eq!(stored[0]["label"], "TIC");
    assert_eq!(stored[1]["label"], "SRM 100 → 50");
    assert_eq!(stored[1]["precursor_mz"], 100.0);
    assert_eq!(stored[1]["product_mz"], 50.0);
    // Extracting one needs its index; the spectrum filters stay at their neutral values.
    let extracted = desktop.tool(
        "extract_chromatogram",
        json!({"dataset_id":id,"kind":"tic","polarity":"positive","ms_level":1,"smoothing":0,"native_chromatogram":1,"display":true}),
    );
    assert_eq!(
        extracted["parameters"]["native_chromatogram"], 1,
        "{extracted}"
    );
    assert_eq!(extracted["displayed"], true);
    assert_eq!(extracted["point_count"], stored[1]["point_count"]);
    let data = desktop.tool(
        "chromatogram_data",
        json!({"dataset_id":id,"offset":0,"limit":10000}),
    );
    let points: Vec<[f64; 2]> = serde_json::from_value(data["points"].clone()).unwrap();
    // Exactly what the GUI's own code reads from the file.
    let params: crate::processing::ProcessingParams =
        serde_json::from_value(extracted["parameters"].clone()).unwrap();
    let mut reader = crate::parser::MzData::new();
    reader.open_msfile(&path).unwrap();
    let (expected, _) = crate::processing::process_chromatogram(&mut reader, &params).unwrap();
    assert_eq!(points, expected);
    // The window shows the stored trace, as if the person had picked it.
    assert_eq!(desktop.app.user_input.native_chromatogram, Some(1));
    // Spectrum filters make no sense for a stored trace and are refused, not ignored.
    for extra in [
        json!({"mass":100.0,"tolerance_ppm":5.0}),
        json!({"mz_range":[1.0,2.0]}),
        json!({"precursor_mz":100.0}),
        json!({"kind":"bpc"}),
    ] {
        let mut arguments = json!({"dataset_id":id,"kind":"tic","polarity":"positive","ms_level":1,"smoothing":0,"native_chromatogram":1});
        arguments
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let error = desktop.tool_error("extract_chromatogram", arguments);
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("stored chromatogram"),
            "{error}"
        );
    }
    let error = desktop.tool_error(
        "extract_chromatogram",
        json!({"dataset_id":id,"kind":"tic","polarity":"positive","ms_level":1,"smoothing":0,"native_chromatogram":7}),
    );
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("stores no chromatogram 7"),
        "{error}"
    );
    desktop.finish();
}

#[test]
fn layout_can_be_changed_over_mcp_exactly_like_the_plot_menu() {
    let mut desktop = Desktop::start(vec![], true, false, false);
    let state = desktop.tool("gui_state", json!({}));
    assert_eq!(state["layout"]["mode"], "grid");
    assert_eq!(state["layout"]["auto_grid"], true);
    assert_eq!(
        state["layout"]["panels"],
        json!({"files":true,"inspector":true,"spectrum":true})
    );
    let overlay = desktop.tool("set_layout", json!({"mode":"overlay"}));
    assert_eq!(overlay["mode"], "overlay");
    assert!(desktop.app.workspace.overlay);
    // A fixed grid of 2 x 3, page 1, without the spectrum panel.
    let grid = desktop.tool(
        "set_layout",
        json!({"mode":"grid","auto_grid":false,"rows":2,"columns":3,"page":1,"show_spectrum":false}),
    );
    assert_eq!(
        (
            grid["mode"].as_str(),
            grid["rows"].as_u64(),
            grid["columns"].as_u64()
        ),
        (Some("grid"), Some(2), Some(3))
    );
    assert_eq!(grid["auto_grid"], false);
    assert_eq!(grid["page"], 1);
    assert_eq!(grid["panels"]["spectrum"], false);
    assert!(!desktop.app.workspace.overlay && !desktop.app.workspace.view.auto_grid);
    assert_eq!(
        (
            desktop.app.workspace.view.rows,
            desktop.app.workspace.view.columns
        ),
        (2, 3)
    );
    // Compare samples (samples by analytes), and focus mode hides every panel until it is left.
    assert_eq!(
        desktop.tool("set_layout", json!({"mode":"compare"}))["mode"],
        "compare"
    );
    let focus = desktop.tool("set_layout", json!({"focus":true}));
    assert_eq!(
        focus["panels"],
        json!({"files":false,"inspector":false,"spectrum":false})
    );
    assert_eq!(focus["focus"], true);
    let restored = desktop.tool("set_layout", json!({"focus":false}));
    assert_eq!(
        restored["panels"],
        json!({"files":true,"inspector":true,"spectrum":false})
    );
    // The report in gui_state is the same value.
    assert_eq!(desktop.tool("gui_state", json!({}))["layout"], restored);
    // Invalid values change nothing.
    for bad in [
        json!({"rows":9}),
        json!({"columns":0}),
        json!({"mode":"stacked"}),
    ] {
        let error = desktop.tool_error("set_layout", bad);
        assert!(
            error["message"].as_str().unwrap().contains("must be 1..8")
                || error["message"].as_str().unwrap().contains("mode:"),
            "{error}"
        );
    }
    assert_eq!(desktop.tool("gui_state", json!({}))["layout"], restored);
    desktop.finish();
    // Without --allow-changes the window is not touched.
    let mut read_only = Desktop::start(vec![], false, false, false);
    let error = read_only.tool_error("set_layout", json!({"mode":"overlay"}));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("--allow-changes"),
        "{error}"
    );
    assert!(!read_only.app.workspace.overlay);
    read_only.finish();
}

#[test]
fn retained_traces_can_be_kept_hidden_and_removed_over_mcp() {
    let mut desktop = Desktop::start(vec![], true, false, false);
    let id = desktop.open_fixture();
    let tic = json!({"dataset_id":id,"kind":"tic","polarity":"positive","ms_level":1,"smoothing":0,"display":true});
    desktop.tool("extract_chromatogram", tic);
    let retained = |d: &mut Desktop| -> Vec<Value> {
        let state = d.tool("gui_state", json!({}));
        state["retained_traces"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["dataset_id"] == id)
            .map(|t| t["traces"].as_array().unwrap().clone())
            .unwrap_or_default()
    };
    assert!(
        retained(&mut desktop).is_empty(),
        "the first extraction is the current one"
    );
    // Keep it explicitly; doing so twice keeps one copy.
    let kept = desktop.tool("retain_trace", json!({"dataset_id":id}));
    assert_eq!(
        (kept["retained"].as_bool(), kept["retained_traces"].as_u64()),
        (Some(true), Some(1)),
        "{kept}"
    );
    assert_eq!(
        desktop.tool("retain_trace", json!({"dataset_id":id}))["already_retained"],
        true
    );
    // A second extraction retains the first automatically, as in the window.
    let smooth = json!({"dataset_id":id,"kind":"tic","polarity":"positive","ms_level":1,"smoothing":3,"display":true});
    desktop.tool("extract_chromatogram", smooth);
    let traces = retained(&mut desktop);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0]["trace_index"], 0);
    assert_eq!(traces[0]["visible"], true);
    // Hide the retained trace, then the current one; both show in the state.
    let hidden = desktop.tool(
        "set_trace_visibility",
        json!({"dataset_id":id,"trace_index":0,"visible":false}),
    );
    assert_eq!(hidden["visible"], false);
    assert_eq!(retained(&mut desktop)[0]["visible"], false);
    desktop.tool(
        "set_trace_visibility",
        json!({"dataset_id":id,"visible":false}),
    );
    assert!(desktop
        .app
        .workspace
        .hidden_current
        .contains(&(id as usize)));
    desktop.tool(
        "set_trace_visibility",
        json!({"dataset_id":id,"visible":true}),
    );
    assert!(!desktop
        .app
        .workspace
        .hidden_current
        .contains(&(id as usize)));
    // Remove the retained trace; the current extraction stays.
    let removed = desktop.tool("remove_trace", json!({"dataset_id":id,"trace_index":0}));
    assert_eq!(removed["retained_traces"], 0, "{removed}");
    assert_eq!(removed["has_current_trace"], true);
    // Remove the current extraction too.
    let removed = desktop.tool("remove_trace", json!({"dataset_id":id}));
    assert_eq!(removed["has_current_trace"], false, "{removed}");
    // Nothing left to act on: a clear error each time.
    for (tool, arguments) in [
        ("remove_trace", json!({"dataset_id":id})),
        ("retain_trace", json!({"dataset_id":id})),
        ("remove_trace", json!({"dataset_id":id,"trace_index":4})),
        (
            "set_trace_visibility",
            json!({"dataset_id":id,"trace_index":4,"visible":true}),
        ),
    ] {
        let error = desktop.tool_error(tool, arguments);
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("current extraction")
                || error["message"]
                    .as_str()
                    .unwrap()
                    .contains("retained trace index"),
            "{tool}: {error}"
        );
    }
    desktop.finish();
}

#[test]
fn a_session_is_saved_and_restored_over_mcp() {
    let temp = tempfile::tempdir().unwrap();
    let mut desktop = Desktop::start(vec![temp.path().to_owned()], true, true, false);
    let id = desktop.open_fixture();
    desktop.tool(
        "extract_chromatogram",
        json!({"dataset_id":id,"kind":"tic","polarity":"positive","ms_level":1,"smoothing":0,"display":true}),
    );
    let data = desktop.tool(
        "chromatogram_data",
        json!({"dataset_id":id,"offset":0,"limit":10000}),
    );
    let points: Vec<[f64; 2]> = serde_json::from_value(data["points"].clone()).unwrap();
    desktop.tool(
        "integrate",
        json!({"dataset_id":id,"start_minutes":points[0][0],"end_minutes":points[points.len() - 1][0],"apply":true}),
    );
    desktop.tool("set_layout", json!({"mode":"overlay"}));
    let path = temp.path().join("work.chromascope");
    let saved = desktop.tool("save_session", json!({"path":path}));
    assert_eq!(saved["datasets"], 1);
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["version"], 1);
    // Saving again never replaces the file; a wrong extension or a missing flag is refused.
    let again = desktop.tool_error("save_session", json!({"path":path}));
    assert!(
        again["message"]
            .as_str()
            .unwrap()
            .contains("already exists"),
        "{again}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    let wrong = desktop.tool_error(
        "save_session",
        json!({"path":temp.path().join("work.json")}),
    );
    assert!(
        wrong["message"].as_str().unwrap().contains(".chromascope"),
        "{wrong}"
    );
    assert!(std::fs::read_dir(temp.path()).unwrap().all(|e| !e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".tmp")));
    // A different window loads it back: the dataset, the trace, the measurement, the layout.
    let mut other = Desktop::start(vec![temp.path().to_owned()], true, true, false);
    let restored = other.tool("load_session", json!({"path":path}));
    assert_eq!(restored["restored"], true, "{restored}");
    assert_eq!(restored["datasets"].as_array().unwrap().len(), 1);
    assert_eq!(restored["measurements"], 1);
    assert_eq!(restored["layout"]["mode"], "overlay");
    let state = other.tool("gui_state", json!({}));
    let restored_id = state["datasets"][0]["dataset_id"].as_u64().unwrap();
    assert_eq!(
        state["datasets"][0]["processing"]["plot_type"], "Tic",
        "{state}"
    );
    assert_eq!(
        state["measurements"][0]["area"],
        desktop.app.workspace.measurements[0].area
    );
    // The restored window works: extraction and numbers are those of the original.
    other.tool(
        "extract_chromatogram",
        json!({"dataset_id":restored_id,"kind":"tic","polarity":"positive","ms_level":1,"smoothing":0}),
    );
    let again = other.tool(
        "chromatogram_data",
        json!({"dataset_id":restored_id,"offset":0,"limit":10000}),
    );
    assert_eq!(again["points"], data["points"]);
    // Without --allow-exports nothing is written; without --allow-changes nothing is replaced.
    let mut guarded = Desktop::start(vec![temp.path().to_owned()], false, false, false);
    let error = guarded.tool_error(
        "save_session",
        json!({"path":temp.path().join("never.chromascope")}),
    );
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("--allow-exports"),
        "{error}"
    );
    let error = guarded.tool_error("load_session", json!({"path":path}));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("--allow-changes"),
        "{error}"
    );
    assert!(guarded.app.files.is_empty());
    // A session that names a source outside the roots is refused before anything changes.
    let mut outside: Value = serde_json::from_str(&text).unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let foreign = elsewhere.path().join("foreign.mzML");
    std::fs::copy(fixture_mzml(), &foreign).unwrap();
    outside["files"][0]["source"] = json!(foreign);
    let forged = temp.path().join("forged.chromascope");
    std::fs::write(&forged, serde_json::to_vec(&outside).unwrap()).unwrap();
    let mut victim = Desktop::start(vec![temp.path().to_owned()], true, true, false);
    let kept = victim.open_fixture();
    let error = victim.tool_error("load_session", json!({"path":forged}));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("outside authorized roots"),
        "{error}"
    );
    assert_eq!(victim.app.files.len(), 1, "the workspace is untouched");
    assert!(victim.app.files.contains_key(&(kept as usize)));
    // A damaged or unsupported file is an error, not a half-restored window.
    let broken = temp.path().join("broken.chromascope");
    std::fs::write(&broken, b"{ not json").unwrap();
    assert_ne!(
        victim.tool_error("load_session", json!({"path":broken}))["message"],
        Value::Null
    );
    outside["version"] = json!(9);
    let future = temp.path().join("future.chromascope");
    std::fs::write(&future, serde_json::to_vec(&outside).unwrap()).unwrap();
    let error = victim.tool_error("load_session", json!({"path":future}));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("Unsupported session version"),
        "{error}"
    );
    assert_eq!(victim.app.files.len(), 1);
    for d in [desktop, other, guarded, victim] {
        d.finish();
    }
}

#[test]
fn presets_are_listed_read_and_saved_over_mcp() {
    let temp = tempfile::tempdir().unwrap();
    let mut desktop = Desktop::start(vec![temp.path().to_owned()], true, true, false);
    let viewer = include_str!("../../../presets/example.toml");
    let method = include_str!("../../../presets/quantification-example.toml");
    std::fs::write(temp.path().join("traces.toml"), viewer).unwrap();
    std::fs::write(temp.path().join("assay.toml"), method).unwrap();
    std::fs::write(
        temp.path().join("broken.toml"),
        "version = 1
[[traces]]
ppm = -3",
    )
    .unwrap();
    std::fs::write(temp.path().join("notes.txt"), "ignored").unwrap();
    // The starter files, and a directory classified by what each file is.
    let listed = desktop.tool("list_presets", json!({"directory":temp.path()}));
    let sources: Vec<&str> = listed["examples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["source"].as_str().unwrap())
        .collect();
    assert_eq!(sources, ["example:viewer", "example:quantification"]);
    let files = listed["directory"]["files"].as_array().unwrap();
    assert_eq!(files.len(), 3, "{listed}");
    let kind_of = |name: &str| {
        files
            .iter()
            .find(|f| f["path"].as_str().unwrap().ends_with(name))
            .unwrap()["kind"]
            .clone()
    };
    assert_eq!(kind_of("traces.toml"), "viewer_preset");
    assert_eq!(kind_of("assay.toml"), "quantification_method");
    assert_eq!(kind_of("broken.toml"), "unreadable");
    // Reading returns the text and its meaning; the text is what apply_preset takes.
    let example = desktop.tool("get_preset", json!({"source":"example:viewer"}));
    assert_eq!(example["toml"], viewer);
    assert_eq!(example["kind"], "viewer_preset");
    assert!(!example["traces"].as_array().unwrap().is_empty());
    let from_file = desktop.tool(
        "get_preset",
        json!({"source":temp.path().join("assay.toml")}),
    );
    assert_eq!(from_file["kind"], "quantification_method");
    assert_eq!(from_file["analytes"][0]["name"], "Analyte A");
    desktop.open_fixture();
    desktop.tool("apply_preset", json!({"toml":example["toml"]}));
    // Saving validates first and never replaces.
    let target = temp.path().join("mine.toml");
    let saved = desktop.tool("save_preset", json!({"path":target,"toml":viewer}));
    assert_eq!(saved["traces"], example["traces"].as_array().unwrap().len());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), viewer);
    assert!(
        desktop
            .app
            .presets
            .editor
            .recent()
            .contains(&std::fs::canonicalize(&target).unwrap()),
        "the preset editor's Recent presets lists it"
    );
    let listed = desktop.tool("list_presets", json!({}));
    assert!(
        listed["recent"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p.as_str().unwrap().ends_with("mine.toml")),
        "{listed}"
    );
    let again = desktop.tool_error("save_preset", json!({"path":target,"toml":viewer}));
    assert!(
        again["message"]
            .as_str()
            .unwrap()
            .contains("already exists"),
        "{again}"
    );
    for (name, text, expect) in [
        (
            "bad.toml",
            "version = 1
[[traces]]
name='x'
acquisition='FS'
mass=100.0
ppm=-1.0",
            "Not a valid viewer preset",
        ),
        ("method.toml", method, "Not a viewer preset"),
        (
            "garbage.toml",
            "this is {not toml",
            "Not a valid viewer preset",
        ),
    ] {
        let error = desktop.tool_error(
            "save_preset",
            json!({"path":temp.path().join(name),"toml":text}),
        );
        assert!(
            error["message"].as_str().unwrap().contains(expect),
            "{name}: {error}"
        );
        assert!(
            !temp.path().join(name).exists(),
            "nothing is written for an invalid preset"
        );
    }
    let wrong = desktop.tool_error(
        "save_preset",
        json!({"path":temp.path().join("p.json"),"toml":viewer}),
    );
    assert!(
        wrong["message"].as_str().unwrap().contains(".toml"),
        "{wrong}"
    );
    // Paths outside the roots and unknown sources are refused.
    let elsewhere = tempfile::tempdir().unwrap();
    std::fs::write(elsewhere.path().join("x.toml"), viewer).unwrap();
    for source in [
        json!(elsewhere.path().join("x.toml")),
        json!("example:other"),
    ] {
        assert!(desktop.tool_error("get_preset", json!({"source":source}))["message"].is_string());
    }
    assert!(
        desktop.tool_error("list_presets", json!({"directory":elsewhere.path()}))["message"]
            .as_str()
            .unwrap()
            .contains("authorized")
    );
    // No --allow-exports, no file.
    let mut read_only = Desktop::start(vec![temp.path().to_owned()], false, false, false);
    let error = read_only.tool_error(
        "save_preset",
        json!({"path":temp.path().join("denied.toml"),"toml":viewer}),
    );
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("--allow-exports"),
        "{error}"
    );
    assert!(!temp.path().join("denied.toml").exists());
    desktop.finish();
    read_only.finish();
}

/// A one-analyte method that fits the fixture's retention-time range.
fn fixture_method(desktop: &mut Desktop, id: u64, name: &str) -> String {
    let metadata = desktop.tool("dataset_metadata", json!({"dataset_id":id}));
    let (start, end) = (
        metadata["retention_time_minutes"][0].as_f64().unwrap(),
        metadata["retention_time_minutes"][1].as_f64().unwrap(),
    );
    format!("version = 1\nname = '{name}'\ndetection_smoothing = 0\nminimum_height = 0.0\nboundary_fraction = 0.05\n[[analytes]]\nexpected_rt = {}\nrt_window = [{start}, {end}]\n[analytes.extraction]\nname = 'target'\nacquisition = 'FS'\nkind = 'XIC'\nmass = 524.3\nppm = 10.0\npolarity = 'positive'\nsmoothing = 0\n", (start + end) / 2.0)
}

/// Runs the method on the dataset and returns the finished batch status.
fn run_batch(desktop: &mut Desktop, id: u64, method: &str) -> Value {
    desktop.tool(
        "start_quantification",
        json!({"method_toml":method,"dataset_ids":[id]}),
    );
    for _ in 0..2000 {
        let status = desktop.tool("quantification_status", json!({}));
        if status["running"] == false {
            return status;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("the batch did not finish");
}

#[test]
fn methods_are_saved_and_opened_over_mcp() {
    let temp = tempfile::tempdir().unwrap();
    let mut desktop = Desktop::start(vec![temp.path().to_owned()], true, true, false);
    let id = desktop.open_fixture();
    let text = fixture_method(&mut desktop, id, "Saved over MCP");
    // A method given as text is checked, then saved; the window is not touched.
    let first = temp.path().join("assay.toml");
    let saved = desktop.tool("save_method", json!({"path":first,"method_toml":text}));
    assert_eq!(
        (
            saved["name"].as_str(),
            saved["analytes"].as_u64(),
            saved["from_window"].as_bool()
        ),
        (Some("Saved over MCP"), Some(1), Some(false))
    );
    let parsed: crate::quant::Method =
        toml::from_str(&std::fs::read_to_string(&first).unwrap()).unwrap();
    assert_eq!(parsed.name, "Saved over MCP");
    let before = desktop.app.quant.method_summary();
    assert!(!before.contains("Saved over MCP"), "{before}");
    // Opening it makes it the window's method; saving without text writes that method.
    let opened = desktop.tool("open_method", json!({"path":first}));
    assert_eq!(
        (opened["name"].as_str(), opened["analytes"].as_u64()),
        (Some("Saved over MCP"), Some(1)),
        "{opened}"
    );
    assert_eq!(
        desktop.app.quant.method_summary(),
        "Saved over MCP · 1 analytes"
    );
    let second = temp.path().join("copy.toml");
    desktop.tool("save_method", json!({"path":second}));
    assert_eq!(
        std::fs::read_to_string(&second).unwrap(),
        std::fs::read_to_string(&first).unwrap(),
        "a method survives open and save unchanged"
    );
    // Edits that were never saved are not replaced unless the caller says so.
    let edited = fixture_method(&mut desktop, id, "Edited, unsaved");
    run_batch(&mut desktop, id, &edited);
    let error = desktop.tool_error("open_method", json!({"path":first}));
    assert!(
        error["message"].as_str().unwrap().contains("never saved"),
        "{error}"
    );
    assert_eq!(
        desktop.app.quant.method_summary(),
        "Edited, unsaved · 1 analytes"
    );
    let forced = desktop.tool("open_method", json!({"path":first,"discard_unsaved":true}));
    assert_eq!(forced["name"], "Saved over MCP");
    assert!(
        forced["results_from_an_earlier_method"].as_u64().unwrap() >= 1,
        "{forced}"
    );
    // Bad input writes and changes nothing.
    for (arguments, expect) in [
        (
            json!({"path":temp.path().join("bad.toml"),"method_toml":"version = 1\nname = 'x'\nsurprise = true"}),
            "unknown field",
        ),
        (
            json!({"path":temp.path().join("bad2.toml"),"method_toml":"this is not toml ["}),
            "",
        ),
        (json!({"path":first}), "already exists"),
        (json!({"path":temp.path().join("m.txt")}), ".toml"),
    ] {
        let error = desktop.tool_error("save_method", arguments);
        assert!(
            error["message"].as_str().unwrap().contains(expect),
            "{error}"
        );
    }
    assert!(!temp.path().join("bad.toml").exists() && !temp.path().join("bad2.toml").exists());
    std::fs::write(
        temp.path().join("broken.toml"),
        "version = 1\nname = 'broken'\n[[analytes]]",
    )
    .unwrap();
    assert!(desktop.tool_error(
        "open_method",
        json!({"path":temp.path().join("broken.toml")})
    )["message"]
        .is_string());
    assert_eq!(
        desktop.app.quant.method_summary(),
        "Saved over MCP · 1 analytes"
    );
    // Permissions: exports to save, changes to open.
    let mut guarded = Desktop::start(vec![temp.path().to_owned()], false, false, false);
    assert!(guarded.tool_error(
        "save_method",
        json!({"path":temp.path().join("g.toml"),"method_toml":text})
    )["message"]
        .as_str()
        .unwrap()
        .contains("--allow-exports"));
    assert!(
        guarded.tool_error("open_method", json!({"path":first}))["message"]
            .as_str()
            .unwrap()
            .contains("--allow-changes")
    );
    desktop.finish();
    guarded.finish();
}

#[test]
fn batches_are_saved_and_loaded_over_mcp_with_their_manual_corrections() {
    let temp = tempfile::tempdir().unwrap();
    let mut desktop = Desktop::start(vec![temp.path().to_owned()], true, true, false);
    let id = desktop.open_fixture();
    let method = fixture_method(&mut desktop, id, "Batch over MCP");
    let status = run_batch(&mut desktop, id, &method);
    assert_eq!(status["results"].as_array().unwrap().len(), 1);
    // A manual correction must survive the file.
    let trace = desktop.tool(
        "quantification_trace",
        json!({"result_index":0,"offset":0,"limit":10000}),
    );
    let points: Vec<[f64; 2]> = serde_json::from_value(trace["points"].clone()).unwrap();
    let (a, z) = (points[1][0], points[points.len() - 2][0]);
    let corrected = desktop.tool(
        "review_quantification",
        json!({"result_index":0,"start_minutes":a,"end_minutes":z,"reviewed":false}),
    );
    let path = temp.path().join("run.chromquant");
    let saved = desktop.tool("save_batch", json!({"path":path}));
    assert_eq!(
        (saved["results"].as_u64(), saved["samples"].as_u64()),
        (Some(1), Some(1)),
        "{saved}"
    );
    let again = desktop.tool_error("save_batch", json!({"path":path}));
    assert!(
        again["message"]
            .as_str()
            .unwrap()
            .contains("already exists"),
        "{again}"
    );
    // A fresh window loads it: same method, same corrected peak, source reopened.
    let mut other = Desktop::start(vec![temp.path().to_owned()], true, true, false);
    let loaded = other.tool("load_batch", json!({"path":path}));
    assert_eq!(loaded["results"], 1, "{loaded}");
    assert_eq!(loaded["reopening"].as_array().unwrap().len(), 1);
    other.pump_until(10, |d| {
        !d.app.files.is_empty() && d.app.files.values().all(|f| !f.is_loading)
    });
    let restored = other.tool("quantification_status", json!({}));
    assert_eq!(
        restored["results"][0]["peak"],
        corrected["results"][0]["peak"]
    );
    assert_eq!(restored["results"][0]["status"], "Manual");
    assert_eq!(restored["method"], status["method"]);
    // A source outside the roots is not reopened; the stored results stay.
    let mut forged: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let foreign = elsewhere.path().join("foreign.mzML");
    std::fs::copy(fixture_mzml(), &foreign).unwrap();
    forged["samples"][0][0] = json!(foreign);
    let forged_path = temp.path().join("forged.chromquant");
    std::fs::write(&forged_path, serde_json::to_vec(&forged).unwrap()).unwrap();
    let mut third = Desktop::start(vec![temp.path().to_owned()], true, true, false);
    let loaded = third.tool("load_batch", json!({"path":forged_path}));
    assert_eq!(loaded["reopening"].as_array().unwrap().len(), 0);
    assert!(
        loaded["skipped"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("outside authorized roots"),
        "{loaded}"
    );
    assert_eq!(
        third.tool("quantification_status", json!({}))["results"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        third.app.files.is_empty(),
        "nothing outside the roots was opened"
    );
    // Damaged and unsupported files change nothing.
    let mut victim = Desktop::start(vec![temp.path().to_owned()], true, true, false);
    let mut future = forged.clone();
    future["version"] = json!(2);
    for (name, bytes, expect) in [
        ("broken.chromquant", b"{ nope".to_vec(), ""),
        (
            "future.chromquant",
            serde_json::to_vec(&future).unwrap(),
            "Unsupported batch version",
        ),
    ] {
        std::fs::write(temp.path().join(name), bytes).unwrap();
        let error = victim.tool_error("load_batch", json!({"path":temp.path().join(name)}));
        assert!(
            error["message"].as_str().unwrap().contains(expect),
            "{name}: {error}"
        );
        assert_eq!(
            victim.tool("quantification_status", json!({}))["results"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
    // Permissions.
    let mut guarded = Desktop::start(vec![temp.path().to_owned()], false, false, false);
    assert!(guarded.tool_error(
        "save_batch",
        json!({"path":temp.path().join("g.chromquant")})
    )["message"]
        .as_str()
        .unwrap()
        .contains("--allow-exports"));
    assert!(
        guarded.tool_error("load_batch", json!({"path":path}))["message"]
            .as_str()
            .unwrap()
            .contains("--allow-changes")
    );
    for d in [desktop, other, third, victim, guarded] {
        d.finish();
    }
}

#[test]
fn a_project_is_created_saved_reopened_and_refreshed_over_mcp() {
    let temp = tempfile::tempdir().unwrap();
    let mut desktop = Desktop::start(vec![temp.path().to_owned()], true, true, true);
    let id = desktop.open_fixture();
    let method = fixture_method(&mut desktop, id, "Project over MCP");
    let status = run_batch(&mut desktop, id, &method);
    // Create: the pointer file and the data folder appear, revision 1 holds the workspace.
    let pointer = temp.path().join("study.json");
    let created = desktop.tool("create_project", json!({"path":pointer}));
    assert_eq!(created["open"], true, "{created}");
    assert_eq!(created["revision"], 1);
    assert_eq!(created["datasets"].as_array().unwrap().len(), 1);
    let pointer_text: Value = serde_json::from_slice(&std::fs::read(&pointer).unwrap()).unwrap();
    assert_eq!(pointer_text["chromascope_project"], 1);
    let data = temp.path().join("study_data");
    assert!(data.is_dir());
    // The engine's own tool reads the same project.
    let inspected = desktop.tool("inspect_project", json!({"directory":data}));
    assert_eq!(inspected["revision"], 1);
    assert_eq!(inspected["sources"].as_array().unwrap().len(), 1);
    // The project's dataset UUID names the open dataset, like its stable id does.
    let uuid = inspected["sources"][0]["id"].as_str().unwrap().to_owned();
    stable_ids(&mut desktop);
    let by_uuid = desktop.tool("dataset_metadata", json!({"dataset_id":uuid}));
    assert_eq!(by_uuid["dataset_id"], id);
    // Save adds a revision; creating over an existing project is refused.
    desktop.tool("set_layout", json!({"mode":"overlay"}));
    let saved = desktop.tool("save_project", json!({}));
    assert_eq!(saved["revision"], 2, "{saved}");
    assert_eq!(
        desktop.tool("inspect_project", json!({"directory":data}))["revision"],
        2
    );
    let again = desktop.tool_error("create_project", json!({"path":pointer}));
    assert!(
        again["message"]
            .as_str()
            .unwrap()
            .contains("already exists"),
        "{again}"
    );
    // Another window opens it: dataset reopened, results and layout restored.
    let mut other = Desktop::start(vec![temp.path().to_owned()], true, true, false);
    let opened = other.tool("open_project", json!({"path":pointer}));
    assert_eq!(
        (opened["open"].as_bool(), opened["revision"].as_u64()),
        (Some(true), Some(2)),
        "{opened}"
    );
    assert_eq!(other.app.files.len(), 1);
    assert!(other.app.files.values().all(|f| !f.is_loading));
    let restored = other.tool("quantification_status", json!({}));
    assert_eq!(restored["results"][0]["peak"], status["results"][0]["peak"]);
    assert_eq!(
        other.tool("gui_state", json!({}))["layout"]["mode"],
        "overlay"
    );
    // Refresh reloads from disk; the data folder itself can be named instead of the pointer.
    assert_eq!(other.tool("refresh_project", json!({}))["revision"], 2);
    let mut third = Desktop::start(vec![temp.path().to_owned()], true, true, false);
    assert_eq!(
        third.tool("open_project", json!({"path":data}))["revision"],
        2
    );
    // Without a project there is nothing to refresh or save.
    let mut empty = Desktop::start(vec![temp.path().to_owned()], true, true, true);
    assert!(empty.tool_error("refresh_project", json!({}))["message"]
        .as_str()
        .unwrap()
        .contains("No project"));
    assert!(empty.tool_error("save_project", json!({}))["message"]
        .as_str()
        .unwrap()
        .contains("No project is open"));
    // Permissions: project writes to create or save, changes to open or refresh.
    let mut guarded = Desktop::start(vec![temp.path().to_owned()], false, true, false);
    for (tool, arguments, flag) in [
        (
            "create_project",
            json!({"path":temp.path().join("never.json")}),
            "--allow-project-writes",
        ),
        ("save_project", json!({}), "--allow-project-writes"),
        ("open_project", json!({"path":pointer}), "--allow-changes"),
        ("refresh_project", json!({}), "--allow-changes"),
    ] {
        let error = guarded.tool_error(tool, arguments);
        assert!(
            error["message"].as_str().unwrap().contains(flag),
            "{tool}: {error}"
        );
    }
    assert!(!temp.path().join("never.json").exists());
    // A dataset outside the roots stays out of projects, in both directions.
    let only_temp = AccessPolicy::new(vec![temp.path().to_owned()], true, true)
        .unwrap()
        .with_project_writes(true);
    desktop.app.remote.as_mut().unwrap().policy = only_temp.clone();
    let error = desktop.tool_error("save_project", json!({}));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("may not register"),
        "{error}"
    );
    assert_eq!(
        desktop.tool("inspect_project", json!({"directory":data}))["revision"],
        2
    );
    let error = desktop.tool_error(
        "create_project",
        json!({"path":temp.path().join("second.json")}),
    );
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("may not register"),
        "{error}"
    );
    assert!(!temp.path().join("second.json").exists() && !temp.path().join("second_data").exists());
    let mut closed = Desktop::start(vec![temp.path().to_owned()], true, true, false);
    closed.app.remote.as_mut().unwrap().policy = only_temp;
    let error = closed.tool_error("open_project", json!({"path":pointer}));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("outside authorized roots"),
        "{error}"
    );
    assert!(closed.app.files.is_empty());
    for d in [desktop, other, third, empty, guarded, closed] {
        d.finish();
    }
}

#[test]
fn bundles_are_built_verified_and_results_reprocessed_over_mcp() {
    let temp = tempfile::tempdir().unwrap();
    let batch = raw_request(temp.path());
    let source = batch.samples[0].source.clone();
    let mut desktop = Desktop::start(vec![temp.path().to_owned()], true, true, true);
    // A calibration the window shows, kept in a project.
    let started = desktop.tool(
        "start_analysis",
        json!({"path":source,"request":start_request(crate::domain::Operation::TargetedBatch{batch})}),
    );
    let job = started["job_id"].as_str().unwrap().to_owned();
    wait_for_job(&mut desktop, &job);
    desktop.pump_until(10, |d| d.app.quant.has_targeted_batch());
    // Nothing to export before a project exists.
    let none = desktop.tool_error(
        "build_bundle",
        json!({"destination":temp.path().join("early")}),
    );
    assert!(
        none["message"]
            .as_str()
            .unwrap()
            .contains("No project is open"),
        "{none}"
    );
    let created = desktop.tool(
        "create_project",
        json!({"path":temp.path().join("study.json")}),
    );
    assert_eq!(created["revision"], 1, "{created}");
    // The window has open warnings (the concentrations are not reviewed): the export stops.
    let bundle = temp.path().join("audit");
    let stopped = desktop.tool_error("build_bundle", json!({"destination":bundle}));
    let message = stopped["message"].as_str().unwrap();
    assert!(
        message.contains("acknowledge_warnings=true") && message.contains("not yet reviewed"),
        "{stopped}"
    );
    assert!(
        !bundle.exists(),
        "nothing is written before the warnings are acknowledged"
    );
    // Acknowledged, it is exported with the warnings in the bundle, and the bundle verifies.
    let built = desktop.tool(
        "build_bundle",
        json!({"destination":bundle,"acknowledge_warnings":true}),
    );
    assert_eq!(built["revision"], 1, "{built}");
    assert!(!built["warnings"].as_array().unwrap().is_empty());
    for file in [
        "report.html",
        "report.json",
        "checksums.json",
        "review-warnings.json",
    ] {
        assert!(bundle.join(file).is_file(), "{file}");
    }
    let warnings: Vec<String> =
        serde_json::from_slice(&std::fs::read(bundle.join("review-warnings.json")).unwrap())
            .unwrap();
    assert_eq!(warnings.len(), built["warnings"].as_array().unwrap().len());
    let listed: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(&std::fs::read(bundle.join("checksums.json")).unwrap()).unwrap();
    assert_eq!(listed.len() as u64, built["files"].as_u64().unwrap());
    let verified = desktop.tool("verify_bundle", json!({"path":bundle}));
    assert_eq!(verified["state"], "verified", "{verified}");
    // A changed file is found; so is a file that was added.
    let report = std::fs::read(bundle.join("report.json")).unwrap();
    std::fs::write(bundle.join("report.json"), b"{}").unwrap();
    assert!(
        desktop.tool_error("verify_bundle", json!({"path":bundle}))["message"]
            .as_str()
            .unwrap()
            .contains("differ")
    );
    std::fs::write(bundle.join("report.json"), report).unwrap();
    assert_eq!(
        desktop.tool("verify_bundle", json!({"path":bundle}))["state"],
        "verified"
    );
    // Reprocess a retained result of the project: recomputed from its source, identical.
    let inspected = desktop.tool(
        "inspect_project",
        json!({"directory":temp.path().join("study_data")}),
    );
    let result_id = inspected["results"][0]["result_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let again = desktop.tool("reprocess_result", json!({"result_id":result_id}));
    assert_eq!(again["reproduced"], true, "{again}");
    assert!(
        again.get("response").is_none(),
        "the response is left out unless asked for"
    );
    let full = desktop.tool("reprocess_result", json!({"result_id":result_id,"project":temp.path().join("study.json"),"include_response":true}));
    assert!(full["response"]["output"].is_object());
    // Refusals: an existing folder, a path outside the roots, a bad id, missing permissions.
    let exists = desktop.tool_error(
        "build_bundle",
        json!({"destination":bundle,"acknowledge_warnings":true}),
    );
    assert!(
        exists["message"]
            .as_str()
            .unwrap()
            .contains("already exists"),
        "{exists}"
    );
    let elsewhere = tempfile::tempdir().unwrap();
    for (tool, arguments) in [
        (
            "build_bundle",
            json!({"destination":elsewhere.path().join("x"),"acknowledge_warnings":true}),
        ),
        ("verify_bundle", json!({"path":elsewhere.path()})),
        ("reprocess_result", json!({"result_id":"not-a-uuid"})),
        (
            "reprocess_result",
            json!({"result_id":uuid::Uuid::new_v4().to_string()}),
        ),
    ] {
        let error = desktop.tool_error(tool, arguments);
        assert!(error["message"].is_string(), "{tool}: {error}");
    }
    assert!(!elsewhere.path().join("x").exists());
    let mut guarded = Desktop::start(vec![temp.path().to_owned()], false, false, false);
    let denied = guarded.tool_error(
        "build_bundle",
        json!({"destination":temp.path().join("denied"),"project":temp.path().join("study.json")}),
    );
    assert!(
        denied["message"]
            .as_str()
            .unwrap()
            .contains("--allow-exports"),
        "{denied}"
    );
    // verify and reprocess only read, so they work without any permission flag.
    assert_eq!(
        guarded.tool("verify_bundle", json!({"path":bundle}))["state"],
        "verified"
    );
    assert_eq!(
        guarded.tool(
            "reprocess_result",
            json!({"result_id":result_id,"project":temp.path().join("study.json")})
        )["reproduced"],
        true
    );
    // With the raw sources, the bundle carries them and still verifies.
    let portable = temp.path().join("portable");
    let built = desktop.tool(
        "build_bundle",
        json!({"destination":portable,"acknowledge_warnings":true,"include_sources":true}),
    );
    assert_eq!(built["sources_included"], true, "{built}");
    assert!(portable.join("sources").is_dir() && portable.join("source-map.json").is_file());
    assert_eq!(
        desktop.tool("verify_bundle", json!({"path":portable}))["state"],
        "verified"
    );
    desktop.finish();
    guarded.finish();
}

#[test]
fn display_preferences_are_read_and_changed_over_mcp() {
    let mut desktop = Desktop::start(vec![], true, false, false);
    let before = desktop.tool("get_settings", json!({}));
    assert_eq!(
        before["choices"]["theme"],
        json!(["system", "light", "dark"])
    );
    assert_eq!(
        before["choices"]["interface_size"],
        json!([90, 100, 115, 130])
    );
    assert_eq!(before["mz_decimals"], crate::gui::settings::mz_decimals());
    // Theme and interface size change the window and are kept for the next start.
    let changed = desktop.tool("set_settings", json!({"theme":"dark","interface_size":115}));
    assert_eq!(
        (
            changed["theme"].as_str(),
            changed["interface_size"].as_u64()
        ),
        (Some("dark"), Some(115)),
        "{changed}"
    );
    assert_eq!(
        desktop.ctx.options(|o| o.theme_preference),
        egui::ThemePreference::Dark
    );
    // The window applies the new size at the start of its next frame.
    let _ = desktop.ctx.run(egui::RawInput::default(), |_| {});
    assert!((desktop.ctx.zoom_factor() - 1.15).abs() < 1e-6);
    let file = crate::import::settings_path()
        .unwrap()
        .with_file_name("settings.json");
    let saved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(saved["theme"], "dark");
    assert!(
        (saved["zoom"].as_f64().unwrap() - 1.15).abs() < 1e-6,
        "{saved}"
    );
    assert_eq!(desktop.tool("get_settings", json!({}))["theme"], "dark");
    // One bad value refuses the whole call: nothing else is applied.
    for bad in [
        json!({"theme":"light","interface_size":101}),
        json!({"theme":"purple"}),
        json!({"theme":"light","mz_decimals":9}),
        json!({"interface_size":0}),
    ] {
        let error = desktop.tool_error("set_settings", bad);
        assert!(error["message"].is_string());
        let now = desktop.tool("get_settings", json!({}));
        assert_eq!(
            (now["theme"].as_str(), now["interface_size"].as_u64()),
            (Some("dark"), Some(115)),
            "{now}"
        );
    }
    // The decimals shown for an m/z (restored at once: it is process-wide).
    let original = crate::gui::settings::mz_decimals();
    let other = if original == 5 { 3 } else { 5 };
    let shown = desktop.tool("set_settings", json!({"mz_decimals":other}));
    assert_eq!(shown["mz_decimals"], other);
    assert_eq!(crate::gui::settings::mz_decimals(), other);
    desktop.tool(
        "set_settings",
        json!({"mz_decimals":original,"theme":"system","interface_size":100}),
    );
    assert_eq!(crate::gui::settings::mz_decimals(), original);
    let _ = desktop.ctx.run(egui::RawInput::default(), |_| {});
    let restored = desktop.tool("get_settings", json!({}));
    assert_eq!(
        (
            restored["theme"].as_str(),
            restored["interface_size"].as_u64()
        ),
        (Some("system"), Some(100))
    );
    // Reading needs no permission; changing does.
    let mut read_only = Desktop::start(vec![], false, false, false);
    assert!(read_only.tool("get_settings", json!({}))["theme"].is_string());
    let error = read_only.tool_error("set_settings", json!({"theme":"dark"}));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("--allow-changes"),
        "{error}"
    );
    desktop.finish();
    read_only.finish();
}

#[test]
fn the_viewer_tools_that_predate_the_parity_work_run_over_the_protocol() {
    let temp = tempfile::tempdir().unwrap();
    let mut desktop = Desktop::start(vec![temp.path().to_owned()], true, true, false);
    let id = desktop.open_fixture();
    let tic = |smoothing: u8| json!({"dataset_id":id,"kind":"tic","polarity":"positive","ms_level":1,"smoothing":smoothing,"display":true});
    // Choosing the dataset, then showing a trace.
    let selected = desktop.tool("select_dataset", json!({"dataset_id":id}));
    assert_eq!(selected["active_dataset_id"], id);
    // Selecting a dataset makes the window extract its default trace, as it does for a person.
    desktop.pump_until(10, |d| {
        !d.app.async_state.is_processing && d.app.state_changed == StateChange::Unchanged
    });
    desktop.tool("extract_chromatogram", tic(0));
    // How it is drawn and framed.
    let display = desktop.tool(
        "set_display",
        json!({"compare_samples":false,"intensity_scale":"shared_highest","line_width":3.0}),
    );
    assert_eq!(display["line_width"], 3.0, "{display}");
    let data = desktop.tool(
        "chromatogram_data",
        json!({"dataset_id":id,"offset":0,"limit":10000}),
    );
    let points: Vec<[f64; 2]> = serde_json::from_value(data["points"].clone()).unwrap();
    let (first, last) = (points[0][0], points[points.len() - 1][0]);
    let view = desktop.tool(
        "set_view",
        json!({"retention_time_range":[first, last],"overlay":true}),
    );
    assert_eq!(view["overlay"], true);
    assert_eq!(
        desktop.tool("set_visibility", json!({"dataset_id":id,"visible":false}))["visible"],
        false
    );
    assert_eq!(
        desktop.tool("set_visibility", json!({"dataset_id":id,"visible":true}))["visible"],
        true
    );
    // Scans and spectra.
    let scans = desktop.tool("list_scans", json!({"dataset_id":id,"offset":0,"limit":3}));
    assert_eq!(scans["scans"].as_array().unwrap().len(), 3);
    let scan = desktop.tool("scan_metadata", json!({"dataset_id":id,"index":0}));
    assert_eq!(scan["index"], 0);
    let spectrum = desktop.tool(
        "spectrum",
        json!({"dataset_id":id,"index":0,"display":true,"offset":0,"limit":5}),
    );
    assert_eq!(spectrum["points"].as_array().unwrap().len(), 5);
    // Pictures of what the window shows.
    for tool in ["chromatogram_image", "spectrum_image"] {
        let reply = desktop.call(tool, json!({}));
        let content = reply["result"]["content"].as_array().unwrap();
        assert!(
            content.iter().any(|c| c["type"] == "image"),
            "{tool}: no image"
        );
    }
    // Integrate, list, undo.
    desktop.tool(
        "integrate",
        json!({"dataset_id":id,"start_minutes":first,"end_minutes":last,"apply":true}),
    );
    assert_eq!(
        desktop.tool("measurements", json!({}))["measurements"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    desktop.tool("undo_integration", json!({}));
    assert_eq!(
        desktop.tool("measurements", json!({}))["measurements"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    // A second extraction retains the first, which select_trace brings back.
    desktop.tool("extract_chromatogram", tic(3));
    // The window's own default extraction (before the explicit one) is retained as well; the
    // one to bring back is the unsmoothed positive TIC with points.
    let retained = desktop.tool("gui_state", json!({}))["retained_traces"][0]["traces"].clone();
    let wanted = retained
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["parameters"]["smoothing"] == 0 && t["point_count"].as_u64().unwrap() > 0)
        .unwrap()["trace_index"]
        .as_u64()
        .unwrap();
    let back = desktop.tool(
        "select_trace",
        json!({"dataset_id":id,"trace_index":wanted}),
    );
    assert_eq!(back["parameters"]["smoothing"], 0, "{back}");
    let after = desktop.tool(
        "chromatogram_data",
        json!({"dataset_id":id,"offset":0,"limit":10000}),
    );
    assert_eq!(
        after["total"], data["total"],
        "the retained trace is back with all its points"
    );
    // Exports: numbers and figures, never over an existing file.
    for (tool, arguments, name) in [
        (
            "export_csv",
            json!({"dataset_id":id,"spectrum":false}),
            "trace.csv",
        ),
        (
            "export_csv",
            json!({"dataset_id":id,"spectrum":true}),
            "spectrum.csv",
        ),
        ("export_figure", json!({"spectrum":false}), "trace.svg"),
        ("export_figure", json!({"spectrum":true}), "spectrum.svg"),
    ] {
        let mut arguments = arguments;
        arguments["path"] = json!(temp.path().join(name));
        desktop.tool(tool, arguments.clone());
        let written = std::fs::read(temp.path().join(name)).unwrap();
        assert!(
            written.len() > 50,
            "{name}: {}",
            String::from_utf8_lossy(&written)
        );
        let again = desktop.tool_error(tool, arguments);
        assert!(
            again["message"]
                .as_str()
                .unwrap()
                .contains("already exists"),
            "{again}"
        );
    }
    // Quantification: results, export, cancel, reset.
    let method = fixture_method(&mut desktop, id, "Older tools");
    run_batch(&mut desktop, id, &method);
    let csv = temp.path().join("batch.csv");
    desktop.tool("export_quantification", json!({"path":csv}));
    assert!(std::fs::read_to_string(&csv)
        .unwrap()
        .contains("method_toml"));
    assert_eq!(
        desktop.tool("cancel_quantification", json!({}))["cancellation_requested"],
        true
    );
    let reset = desktop.tool("reset_quantification", json!({"result_index":0}));
    assert_eq!(
        reset["results"][0]["peak"],
        reset["results"][0]["automatic"]
    );
    desktop.finish();
}

#[test]
fn measurements_can_be_removed_and_the_integration_cleared_over_mcp() {
    let mut desktop = Desktop::start(vec![], true, false, false);
    let id = desktop.open_fixture();
    desktop.tool(
        "extract_chromatogram",
        json!({"dataset_id":id,"kind":"tic","polarity":"positive","ms_level":1,"smoothing":0,"display":true}),
    );
    let data = desktop.tool(
        "chromatogram_data",
        json!({"dataset_id":id,"offset":0,"limit":10000}),
    );
    let points: Vec<[f64; 2]> = serde_json::from_value(data["points"].clone()).unwrap();
    let (a, m, z) = (
        points[0][0],
        points[points.len() / 2][0],
        points[points.len() - 1][0],
    );
    for (start, end) in [(a, m), (m, z)] {
        desktop.tool(
            "integrate",
            json!({"dataset_id":id,"start_minutes":start,"end_minutes":end,"apply":true}),
        );
    }
    let listed = desktop.tool("measurements", json!({}));
    let list = listed["measurements"].as_array().unwrap().clone();
    assert_eq!(list.len(), 2);
    // Clearing removes the markers, not the recorded measurements.
    assert!(desktop.app.integration.result.is_some());
    assert_eq!(
        desktop.tool("clear_integration", json!({}))["cleared"],
        true
    );
    assert!(desktop.app.integration.result.is_none() && desktop.app.integration.start_rt.is_none());
    assert_eq!(desktop.app.workspace.measurements.len(), 2);
    // Removing the first leaves the second, in order.
    let removed = desktop.tool("remove_measurement", json!({"index":0}));
    assert_eq!(removed["remaining"], 1);
    assert_eq!(removed["removed"]["start"], list[0]["start"]);
    assert_eq!(
        desktop.tool("measurements", json!({}))["measurements"][0],
        list[1]
    );
    // The undo entries described the earlier list; undo refuses rather than guess.
    let error = desktop.tool_error("undo_integration", json!({}));
    assert!(error["message"].is_string(), "{error}");
    assert_eq!(desktop.app.workspace.measurements.len(), 1);
    let error = desktop.tool_error("remove_measurement", json!({"index":5}));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("Unknown measurement 5"),
        "{error}"
    );
    // Both need --allow-changes.
    let mut read_only = Desktop::start(vec![], false, false, false);
    for (tool, arguments) in [
        ("remove_measurement", json!({"index":0})),
        ("clear_integration", json!({})),
    ] {
        let error = read_only.tool_error(tool, arguments);
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("--allow-changes"),
            "{tool}: {error}"
        );
    }
    desktop.finish();
    read_only.finish();
}

#[test]
fn the_window_can_be_switched_between_workspaces_over_mcp() {
    let mut desktop = Desktop::start(vec![], true, false, false);
    let stage = |d: &Desktop| {
        d.ctx
            .data(|data| data.get_temp::<usize>(egui::Id::new("quantification_stage")))
    };
    let activity = |d: &Desktop| {
        d.ctx
            .data(|data| data.get_temp::<bool>(egui::Id::new("activity_open")))
            .unwrap_or(false)
    };
    assert!(!desktop.app.quant.active);
    desktop.tool("show_workspace", json!({"workspace":"quantification"}));
    assert!(desktop.app.quant.active);
    for (name, expected) in [("review", 0), ("calibration", 1), ("qc", 2)] {
        let shown = desktop.tool(
            "show_workspace",
            json!({"workspace":"quantification","stage":name}),
        );
        assert_eq!(shown["stage"], name);
        assert!(desktop.app.quant.active);
        assert_eq!(stage(&desktop), Some(expected), "{name}");
    }
    desktop.tool("show_workspace", json!({"workspace":"audit_package"}));
    assert!(desktop.app.delivery.open && !desktop.app.quant.active);
    desktop.tool("show_workspace", json!({"workspace":"ai_activity"}));
    assert!(activity(&desktop) && !desktop.app.delivery.open);
    desktop.tool("show_workspace", json!({"workspace":"method_editor"}));
    assert!(desktop.app.quant.active && !activity(&desktop));
    desktop.tool("show_workspace", json!({"workspace":"data_explorer"}));
    assert!(!desktop.app.quant.active && !desktop.app.delivery.open && !activity(&desktop));
    // Unknown names, a stage on the wrong workspace and workflows that are not in this build.
    let mut bad = vec![
        json!({"workspace":"nowhere"}),
        json!({"workspace":"data_explorer","stage":"qc"}),
        json!({"workspace":"quantification","stage":"final"}),
    ];
    if !crate::EXPERIMENTAL {
        bad.extend([
            json!({"workspace":"identification"}),
            json!({"workspace":"untargeted_analysis"}),
            json!({"workspace":"statistics"}),
        ]);
    }
    for arguments in bad {
        let error = desktop.tool_error("show_workspace", arguments.clone());
        assert!(
            error["message"].as_str().unwrap().contains("workspace:")
                || error["message"].as_str().unwrap().contains("stage"),
            "{arguments}: {error}"
        );
    }
    assert!(!desktop.app.quant.active);
    let mut read_only = Desktop::start(vec![], false, false, false);
    let error = read_only.tool_error("show_workspace", json!({"workspace":"quantification"}));
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("--allow-changes"),
        "{error}"
    );
    assert!(!read_only.app.quant.active);
    desktop.finish();
    read_only.finish();
}
