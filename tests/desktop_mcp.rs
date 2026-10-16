//! The desktop MCP server in the default build: the experimental workflows (Identification,
//! Untargeted analysis, Statistics) stay invisible and unreachable, for the viewer tools and for
//! everything the server shares with the headless engine.
#![cfg(feature = "mcp")]
use chromascope::mcp::{AccessPolicy, McpServer};
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
    let policy = AccessPolicy::new(vec![root.to_owned()], true, true)
        .unwrap()
        .with_project_writes(true);
    let (server, _bridge) = McpServer::new(egui::Context::default(), policy).unwrap();
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
        for tool in ["start_analysis", "analytical_operation"] {
            let reply = client.call(tool, args.clone()).await;
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
