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
