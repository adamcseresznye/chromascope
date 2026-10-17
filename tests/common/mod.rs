//! Shared helpers of the stdio protocol tests.
#![allow(dead_code)]
use serde_json::{json, Value};
use std::io::{BufRead, Write};

fn call(
    input: &mut impl Write,
    output: &mut impl BufRead,
    id: u64,
    tool: &str,
    arguments: Value,
) -> Value {
    let message = json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":tool,"arguments":arguments}});
    writeln!(input, "{message}").unwrap();
    input.flush().unwrap();
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("{tool}: {e}: {line:?}"))
}

/// Runs one engine operation to its end over MCP and returns the reply in the shape the
/// synchronous `analytical_operation` tool (removed: `start_analysis` replaced it) used to give:
/// `result.structuredContent` is the whole response, or the error, and `result.isError` says which.
/// `arguments` are those of `start_analysis`: `{"path": ..., "request": ...}`.
pub fn run_operation(
    input: &mut impl Write,
    output: &mut impl BufRead,
    id: u64,
    arguments: Value,
) -> Value {
    let started = call(input, output, id, "start_analysis", arguments);
    if started["result"]["isError"] == true || started["error"].is_object() {
        return started;
    }
    let job = started["result"]["structuredContent"]["job_id"]
        .as_str()
        .unwrap_or_else(|| panic!("no job id: {started}"))
        .to_owned();
    for attempt in 0..3000u64 {
        let status = call(
            input,
            output,
            id + 1000 + attempt,
            "analysis_status",
            json!({"job_id": job}),
        );
        match status["result"]["structuredContent"]["state"].as_str() {
            Some("succeeded") => break,
            Some("failed") | Some("cancelled") => {
                let error = status["result"]["structuredContent"]["error"].clone();
                return json!({"jsonrpc":"2.0","id":id,"result":{"isError":true,"structuredContent":error}});
            }
            _ => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
    let report = call(
        input,
        output,
        id + 5000,
        "prepare_analysis_report",
        json!({"job_ids":[job],"explanation":"test read of the stored response"}),
    );
    let response =
        report["result"]["structuredContent"]["bundle"]["artifacts"][0]["response"].clone();
    assert!(response.is_object(), "no stored response: {report}");
    json!({"jsonrpc":"2.0","id":id,"result":{"isError":false,"structuredContent":response}})
}
