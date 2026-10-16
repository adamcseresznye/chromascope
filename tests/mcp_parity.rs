//! Nothing a person can do in the window may be GUI-only.
//!
//! `chromascope::gui::actions` lists what a person can do and the MCP tool that does the same.
//! These tests fail when
//! * a control is added to (or removed from) the window without an entry there,
//! * an entry names a tool the server does not list, or a tool is removed from the server,
//! * a tool the server lists belongs to no entry, or has no test that calls it over the protocol.
//!
//! A new feature is not finished until its MCP tool, its entry and its protocol test exist.
#![cfg(feature = "mcp")]
use chromascope::gui::actions::{ACTIONS, FURNITURE, WITHHELD};
use chromascope::mcp::{AccessPolicy, McpServer};
use rmcp::ServiceExt;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// The names `tools/list` returns, over a real MCP connection.
fn listed_tools() -> Vec<String> {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let root = tempfile::tempdir().unwrap();
        let policy = AccessPolicy::new(vec![root.path().to_owned()], true, true)
            .unwrap()
            .with_project_writes(true);
        let (server, bridge) = McpServer::new(egui::Context::default(), policy).unwrap();
        let (client, transport) = tokio::io::duplex(1 << 20);
        let task = tokio::spawn(async move {
            let _window = bridge;
            let _ = server.serve(transport).await.unwrap().waiting().await;
        });
        let (read, mut write) = tokio::io::split(client);
        let mut read = BufReader::new(read);
        let ask = |id: u32, method: &str| {
            format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":id,"method":method,"params":if method == "initialize" {json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"parity","version":"1"}})} else {json!({})}})
            )
        };
        let mut line = String::new();
        write.write_all(ask(1, "initialize").as_bytes()).await.unwrap();
        read.read_line(&mut line).await.unwrap();
        write
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .await
            .unwrap();
        write.write_all(ask(2, "tools/list").as_bytes()).await.unwrap();
        line.clear();
        read.read_line(&mut line).await.unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        let names = reply["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect();
        drop((write, read));
        task.abort();
        names
    })
}

#[test]
fn the_registry_is_well_formed() {
    let mut seen = BTreeSet::new();
    for action in ACTIONS {
        assert!(
            seen.insert(action.name),
            "two actions are called '{}'",
            action.name
        );
        assert!(!action.place.is_empty(), "{}: say where it is", action.name);
        assert!(
            !action.tools.is_empty() || !action.operations.is_empty(),
            "'{}' has no MCP tool: add the tool, or move it to WITHHELD with the reason",
            action.name
        );
        if action.experimental {
            assert!(
                action.tools.is_empty() && !action.operations.is_empty(),
                "'{}' belongs to a workflow that is not in the default build: name the engine operations that do it",
                action.name
            );
        }
    }
    for withheld in WITHHELD {
        assert!(
            withheld.reason.len() >= 40,
            "'{}': a withheld action needs a real reason",
            withheld.name
        );
    }
    for (label, reason) in FURNITURE {
        assert!(reason.len() >= 8, "'{label}': say why it is not an action");
    }
}

#[test]
fn every_tool_an_action_names_is_listed_and_every_listed_tool_belongs_to_an_action() {
    let listed: BTreeSet<String> = listed_tools().into_iter().collect();
    assert!(
        listed.len() > 60,
        "the desktop server lists the viewer and engine tools"
    );
    let mut used = BTreeSet::new();
    for action in ACTIONS {
        for tool in action.tools {
            if action.experimental && !chromascope::EXPERIMENTAL {
                continue;
            }
            assert!(
                listed.contains(*tool),
                "'{}' maps to the tool '{tool}', which tools/list does not return: it was removed or renamed",
                action.name
            );
            used.insert(tool.to_string());
        }
    }
    let orphans: Vec<_> = listed.difference(&used).collect();
    assert!(
        orphans.is_empty(),
        "tools that no action in chromascope::gui::actions uses (add the window control they stand for): {orphans:?}"
    );
}

#[test]
fn every_tool_is_called_over_the_protocol_by_some_test() {
    let root = env!("CARGO_MANIFEST_DIR");
    let mut sources = String::new();
    for file in [
        "src/gui/remote/protocol_tests.rs",
        "tests/engine_mcp.rs",
        "tests/desktop_mcp.rs",
        "tests/mcp_parity.rs",
    ] {
        sources.push_str(&std::fs::read_to_string(format!("{root}/{file}")).unwrap());
    }
    let untested: Vec<_> = listed_tools()
        .into_iter()
        .filter(|tool| !sources.contains(&format!("\"{tool}\"")))
        .collect();
    assert!(
        untested.is_empty(),
        "tools without a test that calls them through the MCP protocol: {untested:?}"
    );
}

#[test]
fn engine_operations_named_by_actions_exist_and_hidden_ones_stay_hidden() {
    use chromascope::domain::{Operation, Request};
    let schema = serde_json::to_value(rmcp::schemars::schema_for!(Request)).unwrap();
    let mut all = BTreeSet::new();
    collect_operation_names(&schema, &mut all);
    assert!(
        all.len() >= 34,
        "found only {} operations in the request schema",
        all.len()
    );
    let hidden: BTreeSet<&str> = Operation::EXPERIMENTAL_NAMES.iter().copied().collect();
    let mut reached_hidden = BTreeSet::new();
    for action in ACTIONS {
        for operation in action.operations {
            assert!(
                all.contains(*operation),
                "'{}': there is no engine operation '{operation}'",
                action.name
            );
            if action.experimental {
                assert!(
                    hidden.contains(operation),
                    "'{}' is marked experimental but '{operation}' is not",
                    action.name
                );
                reached_hidden.insert(*operation);
            } else {
                assert!(!hidden.contains(operation), "'{}' uses the hidden operation '{operation}' without being marked experimental", action.name);
            }
        }
    }
    let missing: Vec<_> = hidden.difference(&reached_hidden).collect();
    assert!(
        missing.is_empty(),
        "experimental operations no window control reaches: {missing:?}"
    );
}

fn collect_operation_names(value: &Value, names: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            if let Some(tag) = map
                .get("properties")
                .and_then(|p| p.get("operation"))
                .and_then(|o| {
                    o.get("const")
                        .or_else(|| o.get("enum").and_then(|e| e.get(0)))
                })
                .and_then(Value::as_str)
            {
                names.insert(tag.to_owned());
            }
            map.values().for_each(|v| collect_operation_names(v, names));
        }
        Value::Array(items) => items.iter().for_each(|v| collect_operation_names(v, names)),
        _ => {}
    }
}

/// Text of the literal that starts at `start` (just after its opening quote), unescaped.
fn read_literal(code: &str, start: usize) -> Option<(String, usize)> {
    let bytes = code.as_bytes();
    let mut out = String::new();
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Some((out, i + 1)),
            b'\\' => {
                let rest = &code[i + 1..];
                if let Some(hex) = rest.strip_prefix("u{") {
                    let end = hex.find('}')?;
                    out.push(char::from_u32(u32::from_str_radix(&hex[..end], 16).ok()?)?);
                    i += 1 + 2 + end + 1;
                } else {
                    let escaped = rest.chars().next()?;
                    out.push(match escaped {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                    i += 1 + escaped.len_utf8();
                }
            }
            _ => {
                let c = code[i..].chars().next()?;
                out.push(c);
                i += c.len_utf8();
            }
        }
    }
    None
}

/// The string literals among the arguments of the call whose "(" ends at `open`: only the first
/// for a menu (its closure holds other controls, scanned on their own), all of them otherwise
/// (`if on { "Hide" } else { "Show" }`).
fn literal_arguments(code: &str, open: usize, only_first: bool) -> Vec<String> {
    let bytes = code.as_bytes();
    let mut found = vec![];
    let mut depth = 1;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            b'"' => {
                let Some((text, end)) = read_literal(code, i + 1) else {
                    break;
                };
                found.push(text);
                if only_first {
                    break;
                }
                i = end;
                continue;
            }
            // A character literal such as '"' or ')' is not an argument.
            b'\''
                if code[i..].starts_with("'\"'")
                    || code[i..].starts_with("')'")
                    || code[i..].starts_with("'('") =>
            {
                i += 2
            }
            _ => {}
        }
        i += 1;
    }
    found
}

/// Calls that put a clickable or toggleable control in the window.
const CONTROLS: &[&str] = &[
    ".button(",
    "small_button(",
    "menu_button(",
    "icon_item(",
    ".checkbox(",
    "segment(",
    "primary_button_enabled(",
    "primary_button(",
    "selectable_label(",
    "Button::new(",
    "Button::selectable(",
    "radio_value(",
    "selectable_value(",
    "labeled_cell(",
];
/// Widgets that take a value; the label written just before one names an input.
const INPUTS: &[&str] = &[
    "DragValue::",
    "Slider::",
    "TextEdit::",
    "ComboBox::",
    "labeled_cell(",
    "radio_value(",
    "selectable_value(",
    "selectable_label(",
    "segment(",
    ".checkbox(",
];

fn gui_files(directory: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            gui_files(&path, files);
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
}

/// Every fixed control label in the window's production code, with the files that have it.
fn window_labels() -> BTreeMap<String, BTreeSet<String>> {
    let mut files = vec![];
    gui_files(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/gui"),
        &mut files,
    );
    let mut labels: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for file in files {
        let text = std::fs::read_to_string(&file)
            .unwrap()
            .replace("\r\n", "\n");
        let code = &text[..text.find("#[cfg(test)]").unwrap_or(text.len())];
        let mut add = |label: String| {
            // Labels built at run time ("{} samples") and identifiers are not fixed text.
            // Hint sentences are not control labels.
            if label.len() > 80
                || label.contains(['{', '_'])
                || label.chars().filter(|c| c.is_alphabetic()).count() < 2
            {
                return;
            }
            labels
                .entry(label)
                .or_default()
                .insert(file.file_name().unwrap().to_string_lossy().into_owned());
        };
        for call in CONTROLS {
            for (at, _) in code.match_indices(call) {
                for label in literal_arguments(code, at + call.len(), *call == "menu_button(") {
                    add(label);
                }
            }
        }
        // A caption written next to an input: `ui.label("Weighting"); ui.add(DragValue...)`.
        for (at, _) in code.match_indices(".label(") {
            let Some(label) = literal_arguments(code, at + ".label(".len(), true)
                .into_iter()
                .next()
            else {
                continue;
            };
            let after = &code[at..];
            let window_end = after[".label(".len()..]
                .find(".label(")
                .map_or(after.len(), |n| n + ".label(".len())
                .min(600);
            let window = &after[..window_end.min(after.len())];
            if INPUTS.iter().any(|input| window.contains(input)) {
                add(label);
            }
        }
    }
    labels
}

#[test]
fn every_control_in_the_window_has_an_action() {
    let found = window_labels();
    assert!(
        found.len() > 150,
        "the scan found only {} labels; did the window code move?",
        found.len()
    );
    let mut known: BTreeSet<&str> = BTreeSet::new();
    for action in ACTIONS {
        known.extend(action.labels.iter().copied());
    }
    for withheld in WITHHELD {
        known.extend(withheld.labels.iter().copied());
    }
    known.extend(FURNITURE.iter().map(|(label, _)| *label));
    let unmapped: Vec<String> = found
        .iter()
        .filter(|(label, _)| !known.contains(label.as_str()))
        .map(|(label, files)| {
            format!(
                "{label:?} ({})",
                files.iter().cloned().collect::<Vec<_>>().join(", ")
            )
        })
        .collect();
    assert!(
        unmapped.is_empty(),
        "controls in the window with no entry in chromascope::gui::actions. Give each the MCP tool that does the same (or, for a control that starts nothing, a FURNITURE reason):\n  {}",
        unmapped.join("\n  ")
    );
    let stale: Vec<&&str> = known
        .iter()
        .filter(|label| !found.contains_key(**label))
        .collect();
    assert!(
        stale.is_empty(),
        "entries in chromascope::gui::actions for controls that are no longer in the window: {stale:?}"
    );
}
