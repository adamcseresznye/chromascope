//! The desktop MCP server in the default build: the experimental workflows (Identification,
//! Untargeted analysis, Statistics) stay invisible and unreachable, for the viewer tools and for
//! everything the server shares with the headless engine.
#![cfg(feature = "mcp")]
use chromascope::mcp::{AccessPolicy, McpServer, ToolProfile, CORE_TOOLS};
use rmcp::ServiceExt;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};

struct Client {
    read: BufReader<ReadHalf<DuplexStream>>,
    write: WriteHalf<DuplexStream>,
    id: u64,
}
impl Client {
    async fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        let line = format!(
            "{}\n",
            json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params})
        );
        self.write.write_all(line.as_bytes()).await.unwrap();
        let mut reply = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.read.read_line(&mut reply),
        )
        .await
        .unwrap()
        .unwrap();
        serde_json::from_str(&reply).unwrap()
    }
    async fn call(&mut self, name: &str, arguments: Value) -> Value {
        self.rpc("tools/call", json!({"name":name,"arguments":arguments}))
            .await
    }
}

/// A connected client. Calls that need the window to answer are not made here: the tools used
/// are the ones the server answers on its own threads.
async fn connect(root: &std::path::Path) -> (Client, tokio::task::JoinHandle<()>) {
    connect_with(root, ToolProfile::default()).await
}
async fn connect_with(
    root: &std::path::Path,
    profile: ToolProfile,
) -> (Client, tokio::task::JoinHandle<()>) {
    let policy = AccessPolicy::new(vec![root.to_owned()], true, true)
        .unwrap()
        .with_project_writes(true);
    let (server, _bridge) = McpServer::new(egui::Context::default(), policy).unwrap();
    let server = server.with_tool_profile(profile);
    let (client, transport) = tokio::io::duplex(1024 * 1024);
    let task = tokio::spawn(async move {
        let _keep_window_alive = _bridge;
        server
            .serve(transport)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let (read, write) = tokio::io::split(client);
    let mut client = Client {
        read: BufReader::new(read),
        write,
        id: 0,
    };
    let init = client
        .rpc(
            "initialize",
            json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"hidden workflows test","version":"1"}}),
        )
        .await;
    assert!(init["result"]["capabilities"]["tools"].is_object());
    client
        .write
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .unwrap();
    (client, task)
}

#[test]
fn experimental_workflows_are_hidden_on_the_desktop_server() {
    if chromascope::EXPERIMENTAL {
        return; // the experimental build exposes them on purpose
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let temp = tempfile::tempdir().unwrap();
        let anchor = temp.path().join("anchor.mzML");
        std::fs::write(&anchor, b"anchor").unwrap();
        let (mut client, task) = connect(temp.path()).await;
        let listed = client.rpc("tools/list", json!({})).await;
        let tools = listed["result"]["tools"].as_array().unwrap();
        assert!(tools.len() > 40, "viewer and engine tools are listed");
        let mut hidden: Vec<&str> = chromascope::domain::Operation::EXPERIMENTAL_NAMES.to_vec();
        assert_eq!(hidden.len(), 18);
        hidden.extend([
            "untargeted",
            "feature_annotation",
            "feature_matrix",
            "spectral_library",
            "isotope",
        ]);
        for tool in tools {
            let text = tool.to_string();
            for word in &hidden {
                assert!(!text.contains(word), "{} still mentions {word}", tool["name"]);
            }
        }
        // The recipes offered as prompts must not mention the hidden workflows either.
        let prompts = client.rpc("prompts/list", json!({})).await;
        assert_eq!(prompts["result"]["prompts"].as_array().unwrap().len(), 6);
        for prompt in prompts["result"]["prompts"].as_array().unwrap() {
            let got = client
                .rpc("prompts/get", json!({"name":prompt["name"]}))
                .await;
            let text = got.to_string();
            for word in &hidden {
                assert!(!text.contains(word), "prompt {} mentions {word}", prompt["name"]);
            }
        }
        let capabilities = client.call("analysis_capabilities", json!({})).await;
        let text = capabilities["result"]["structuredContent"].to_string();
        for word in hidden.iter().chain(&["annotation"]) {
            assert!(!text.contains(word), "capabilities still mention {word}");
        }
        let request = json!({"version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"actor":"AI","operation":{"operation":"import_spectral_library","text":"","format":"msp","source":{"name":"n","version":"1","url":"u","license":"l"}}});
        let args = json!({"path":anchor,"request":request});
        assert!(
            !tools.iter().any(|t| t["name"] == "analytical_operation"),
            "analytical_operation was removed; start_analysis replaces it"
        );
        for (tool, arguments) in [
            ("start_analysis", args.clone()),
            (
                "prepare_review_proposal",
                json!({"path":anchor,"request":request,"reason":"r","evidence":[]}),
            ),
        ] {
            let reply = client.call(tool, arguments).await;
            assert_eq!(
                reply["result"]["structuredContent"]["code"], "experimental_unavailable",
                "{tool}: {reply}"
            );
        }
        for tool in [
            "start_untargeted",
            "untargeted_status",
            "cancel_untargeted",
            "untargeted_features",
        ] {
            let reply = client.call(tool, args.clone()).await;
            assert!(reply["error"].is_object(), "{tool} must be unknown: {reply}");
        }
        drop(client);
        task.await.unwrap();
    });
}

/// The tool list is what an AI client reads before it does anything: it must stay small. The
/// full engine request schema (about 38 KB) used to be embedded in `start_analysis` and in
/// `prepare_review_proposal`, and every batch command schema in `batch`; now they are short and
/// `analysis_capabilities` returns the complete schema. The requests are still parsed in full when
/// a tool is called, so the checks are as strict as before.
#[test]
fn the_tool_list_stays_small_and_requests_are_still_validated_in_full() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let temp = tempfile::tempdir().unwrap();
        let anchor = temp.path().join("anchor.mzML");
        std::fs::write(&anchor, b"anchor").unwrap();
        let (mut client, task) = connect(temp.path()).await;
        let listed = client.rpc("tools/list", json!({})).await;
        let tools = listed["result"]["tools"].as_array().unwrap();
        let size = serde_json::to_string(tools).unwrap().len();
        // The experimental build lists four more tools and more operations.
        let limit = if chromascope::EXPERIMENTAL { 46_000 } else { 40_000 };
        assert!(
            size < limit,
            "tools/list is {size} bytes (limit {limit}); shorten descriptions or schemas, do not embed the engine schema"
        );
        let find = |name: &str| tools.iter().find(|t| t["name"] == name).unwrap();
        for name in ["start_analysis", "prepare_review_proposal"] {
            let schema = find(name)["inputSchema"].to_string();
            assert!(schema.len() < 1000, "{name} lists a {} byte schema", schema.len());
            assert!(!schema.contains("targeted_batch"), "{name} embeds the operations");
            assert_eq!(find(name)["inputSchema"]["properties"]["request"]["type"], "object");
        }
        // batch: the allowed operation names, not the schema of every command.
        let batch = find("batch")["inputSchema"].clone();
        assert!(batch.to_string().len() < 2500, "batch schema is {} bytes", batch.to_string().len());
        let names: Vec<&str> = batch["properties"]["commands"]["items"]["properties"]["operation"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        for needed in ["set_view", "extract_chromatogram", "integrate", "start_quantification"] {
            assert!(names.contains(&needed), "batch does not list {needed}");
        }
        assert!(!names.contains(&"batch"), "batches do not nest");
        // Every operation a batch lists is accepted by the parser (its arguments are checked
        // when the command runs), so the list cannot drift from the real set.
        let complete_schema = client.call("analysis_capabilities", json!({})).await;
        assert!(complete_schema["result"]["structuredContent"]["request_schema"]["$defs"]
            .to_string()
            .contains("targeted_batch"));

        // Validation did not loosen: each of these is refused and nothing is started.
        let engine = |operation: Value| {
            json!({"path": anchor, "request": {"version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"actor":"AI","operation":operation}})
        };
        let refused = |reply: &Value| {
            reply["error"].is_object() || reply["result"]["isError"] == true
        };
        for tool in ["start_analysis"] {
            for (label, arguments) in [
                ("unknown operation", engine(json!({"operation":"no_such_operation"}))),
                (
                    "unknown field in the request",
                    {
                        let mut arguments = engine(json!({"operation":"metadata"}));
                        arguments["request"]["surprise"] = json!(true);
                        arguments
                    },
                ),
                ("request is not an object", json!({"path":anchor,"request":"metadata"})),
                ("request missing", json!({"path":anchor})),
                (
                    "operation field of the wrong type",
                    engine(json!({"operation":"extract","kind":5})),
                ),
            ] {
                let reply = client.call(tool, arguments).await;
                assert!(refused(&reply), "{tool}: {label} was accepted: {reply}");
                assert!(
                    reply["result"]["structuredContent"]["job_id"].is_null(),
                    "{tool}: {label} started a job"
                );
            }
        }
        let outside = tempfile::tempdir().unwrap();
        let elsewhere = outside.path().join("other.mzML");
        std::fs::write(&elsewhere, b"x").unwrap();
        let reply = client
            .call(
                "start_analysis",
                json!({"path":elsewhere,"request":engine(json!({"operation":"metadata"}))["request"]}),
            )
            .await;
        assert_eq!(reply["result"]["structuredContent"]["code"], "unauthorized", "{reply}");
        for (label, commands) in [
            ("empty", json!([])),
            (
                "nested",
                json!([{"operation":"batch","commands":[{"operation":"list_datasets"}]}]),
            ),
            ("65 commands", json!(vec![json!({"operation":"list_datasets"}); 65])),
        ] {
            let reply = client.call("batch", json!({"commands":commands})).await;
            assert_eq!(
                reply["result"]["structuredContent"]["code"], "invalid_batch",
                "batch {label}: {reply}"
            );
        }
        for (label, commands) in [
            ("unknown operation", json!([{"operation":"no_such_operation"}])),
            ("missing field", json!([{"operation":"select_dataset"}])),
            ("unknown field", json!([{"operation":"list_datasets","surprise":1}])),
            ("no operation", json!([{"dataset_id":0}])),
        ] {
            let reply = client.call("batch", json!({"commands":commands})).await;
            assert!(refused(&reply), "batch {label} was accepted: {reply}");
        }
        drop(client);
        task.await.unwrap();
    });
}

/// `--tool-profile`: some clients cap the number of tools, so `core` lists the main workflows
/// only (at most 40 tools) and refuses every other tool when it is called; `full` (the default)
/// lists everything, which is what the parity test checks.
#[test]
fn the_core_profile_lists_the_main_workflows_and_refuses_every_other_tool() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let temp = tempfile::tempdir().unwrap();
        let (mut full, full_task) = connect(temp.path()).await;
        let listed = full.rpc("tools/list", json!({})).await;
        let full_names: Vec<String> = listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect();
        assert!(full_names.len() >= 70, "the default profile is full: {} tools", full_names.len());
        for tool in CORE_TOOLS {
            assert!(full_names.iter().any(|n| n == tool), "core names a tool that does not exist: {tool}");
        }
        drop(full);
        full_task.await.unwrap();

        let (mut client, task) = connect_with(temp.path(), ToolProfile::Core).await;
        let init_again = client.rpc("tools/list", json!({})).await;
        let tools = init_again["result"]["tools"].as_array().unwrap();
        assert!(tools.len() <= 40, "core lists {} tools", tools.len());
        let mut names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        names.sort_unstable();
        let mut expected = CORE_TOOLS.to_vec();
        expected.sort_unstable();
        assert_eq!(names, expected, "core lists exactly the core tools");
        // The main workflows are all there.
        for workflow in [
            "open_files", "dataset_metadata", "extract_chromatogram", "spectrum", "integrate",
            "start_quantification", "review_quantification", "export_csv", "create_project",
            "open_project", "start_analysis", "analysis_status", "analysis_result",
            "analysis_capabilities",
        ] {
            assert!(names.contains(&workflow), "core lacks {workflow}");
        }
        // A tool outside the profile is refused as unknown, whichever server part serves it.
        for outside in [
            "set_layout", "get_settings", "load_session", "save_preset", "retain_trace",
            "inspect_project", "session_audit", "queue_review_proposal", "discover_datasets",
        ] {
            assert!(!names.contains(&outside));
            let reply = client.call(outside, json!({})).await;
            assert_eq!(reply["error"]["code"], -32602, "{outside}: {reply}");
            let message = reply["error"]["message"].as_str().unwrap();
            assert!(
                message.contains("core") && message.contains("--tool-profile full"),
                "{outside}: {message}"
            );
        }
        // A tool of the profile still works; one that needs no window answers here.
        let capabilities = client.call("analysis_capabilities", json!({})).await;
        assert!(capabilities["result"]["structuredContent"]["request_schema"].is_object());
        // A batch cannot reach a tool the profile leaves out ...
        let batch = tools.iter().find(|t| t["name"] == "batch").unwrap();
        let operations = batch["inputSchema"]["properties"]["commands"]["items"]["properties"]["operation"]["enum"]
            .as_array()
            .unwrap();
        assert!(operations.iter().any(|o| o == "integrate"));
        assert!(!operations.iter().any(|o| o == "set_layout" || o == "get_settings"));
        let reply = client
            .call("batch", json!({"commands":[{"operation":"list_datasets"},{"operation":"set_layout","mode":"grid"}]}))
            .await;
        assert_eq!(reply["result"]["structuredContent"]["code"], "tool_not_in_profile", "{reply}");
        // ... and a batch of core operations is still checked like any other.
        let reply = client.call("batch", json!({"commands":[]})).await;
        assert_eq!(reply["result"]["structuredContent"]["code"], "invalid_batch");
        // Prompts and the instructions are unchanged except for the note about the profile.
        let prompts = client.rpc("prompts/list", json!({})).await;
        assert_eq!(prompts["result"]["prompts"].as_array().unwrap().len(), 6);
        drop(client);
        task.await.unwrap();
    });
}

#[test]
fn the_tool_profile_flag_is_validated_before_a_window_opens() {
    use std::process::Command;
    let exe = env!("CARGO_BIN_EXE_chromascope-mcp");
    let root = env!("CARGO_MANIFEST_DIR");
    for args in [
        vec!["--allow-root", root, "--tool-profile", "bogus"],
        vec!["--allow-root", root, "--tool-profile"],
    ] {
        let output = Command::new(exe)
            .args(&args)
            .env("CHROMASCOPE_NO_UPDATE_CHECK", "1")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{args:?} must fail");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("tool profile") || error.contains("--tool-profile"),
            "{args:?}: {error}"
        );
    }
    let help = Command::new(exe)
        .arg("--help")
        .env("CHROMASCOPE_NO_UPDATE_CHECK", "1")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&help.stderr);
    assert!(
        text.contains("--tool-profile core|full") && text.contains("40"),
        "{text}"
    );
    assert_eq!("core".parse::<ToolProfile>(), Ok(ToolProfile::Core));
    assert_eq!("full".parse::<ToolProfile>(), Ok(ToolProfile::Full));
    assert!("Core".parse::<ToolProfile>().is_err());
    assert_eq!(ToolProfile::default(), ToolProfile::Full);
}
