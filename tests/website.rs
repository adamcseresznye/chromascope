//! The website (website/content) must stay true to the program.
//!
//! * Bold text in the pages is reserved for labels of the window: each one must be a string the
//!   window's source contains.
//! * Every tool, prompt, resource address and launch flag the MCP guide mentions must exist in
//!   the real servers (`tools/list`, `prompts/list`, `resources/templates/list`, the binaries'
//!   `--help`), and every tool, prompt and resource the servers offer must be in the guide.
//! * No page contains a placeholder or a reference to a missing picture.
//!
//! `mkdocs build --strict` (the Website workflow) checks the links.
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn files(directory: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, extension, out);
        } else if path.extension().is_some_and(|e| e == extension) {
            out.push(path);
        }
    }
}

/// A page of the site: its path below `website/content` and its text with LF line ends.
struct Page {
    name: String,
    text: String,
}

fn pages() -> Vec<Page> {
    let content = root().join("website/content");
    let mut paths = vec![];
    files(&content, "md", &mut paths);
    assert!(paths.len() >= 20, "found only {} pages", paths.len());
    paths.sort();
    paths
        .into_iter()
        .map(|path| Page {
            name: path
                .strip_prefix(&content)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/"),
            text: fs::read_to_string(&path).unwrap().replace("\r\n", "\n"),
        })
        .collect()
}

/// What a page is made of: fenced code blocks, inline code, bold text and the rest.
#[derive(Default)]
struct Parts {
    fences: Vec<String>,
    inline: Vec<String>,
    bold: Vec<String>,
}

fn parts(text: &str) -> Parts {
    let mut parts = Parts::default();
    let mut fence: Option<String> = None;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            match fence.take() {
                Some(block) => parts.fences.push(block),
                None => fence = Some(String::new()),
            }
            continue;
        }
        if let Some(block) = fence.as_mut() {
            block.push_str(line);
            block.push('\n');
            continue;
        }
        if line.trim_start().starts_with("<!--") {
            continue;
        }
        // Inline code first, then what is left of the line is searched for bold text.
        let mut rest = String::new();
        let mut in_code = false;
        let mut current = String::new();
        for c in line.chars() {
            if c == '`' {
                if in_code {
                    parts.inline.push(std::mem::take(&mut current));
                } else {
                    rest.push(' ');
                }
                in_code = !in_code;
            } else if in_code {
                current.push(c);
            } else {
                rest.push(c);
            }
        }
        let mut pieces = rest.split("**");
        pieces.next();
        while let (Some(inside), Some(_)) = (pieces.next(), pieces.next()) {
            parts.bold.push(inside.trim().to_owned());
        }
    }
    parts
}

// ---------------------------------------------------------------------------------------------
// Labels of the window
// ---------------------------------------------------------------------------------------------

/// The text of a string literal that starts at `start` (just after its opening quote).
fn literal_at(code: &[char], start: usize) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut i = start;
    while i < code.len() {
        match code[i] {
            '"' => return Some((out, i + 1)),
            '\\' => {
                let next = *code.get(i + 1)?;
                if next == 'u' && code.get(i + 2) == Some(&'{') {
                    let close = code[i..].iter().position(|c| *c == '}')? + i;
                    let hex: String = code[i + 3..close].iter().collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                    i = close + 1;
                } else {
                    out.push(match next {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                    i += 2;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    None
}

/// Every string literal in the window's production code (src/gui, without the test modules).
fn window_literals() -> BTreeSet<String> {
    let mut paths = vec![];
    files(&root().join("src/gui"), "rs", &mut paths);
    let mut found = BTreeSet::new();
    for path in paths {
        let text = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
        let production = &text[..text.find("#[cfg(test)]\nmod ").unwrap_or(text.len())];
        let code: Vec<char> = production.chars().collect();
        let mut i = 0;
        while i < code.len() {
            // Line comments hold no labels (and may hold a lone quote).
            if code[i] == '/' && code.get(i + 1) == Some(&'/') {
                while i < code.len() && code[i] != '\n' {
                    i += 1;
                }
            // A character literal such as '"' is not a string.
            } else if code[i] == '\''
                && code.get(i + 1) == Some(&'"')
                && code.get(i + 2) == Some(&'\'')
            {
                i += 3;
            } else if code[i] == '"' {
                match literal_at(&code, i + 1) {
                    Some((text, end)) => {
                        found.insert(text);
                        i = end;
                    }
                    None => break,
                }
            } else {
                i += 1;
            }
        }
    }
    assert!(
        found.len() > 500,
        "the scan found only {} literals",
        found.len()
    );
    found
}

#[test]
fn bold_text_in_the_pages_is_a_label_of_the_window() {
    let literals = window_literals();
    // The menu arrow is part of some literals ("Batch session ▾"); the manual leaves it out.
    let known = |label: &str| {
        literals.contains(label)
            || literals.contains(&format!("{label} ▾"))
            || literals.contains(&format!("{label} \u{25be}"))
    };
    let mut unknown = vec![];
    let mut total = 0;
    for page in pages() {
        for label in parts(&page.text).bold {
            total += 1;
            if !known(&label) {
                unknown.push(format!("{}: **{label}**", page.name));
            }
        }
    }
    assert!(
        total > 100,
        "only {total} bold labels were found: did the parser break?"
    );
    assert!(
        unknown.is_empty(),
        "bold text is reserved for labels that the window shows, but these are not in src/gui (rename them to the real label, or use italics):\n  {}",
        unknown.join("\n  ")
    );
}

#[test]
fn pages_have_no_placeholders_and_no_missing_pictures() {
    let forbidden = ["todo", "tbd", "lorem ipsum", "coming soon", "fixme"];
    for page in pages() {
        let lower = page.text.to_lowercase();
        for word in forbidden {
            let at = lower.match_indices(word).find(|(i, _)| {
                let before = lower[..*i].chars().next_back();
                let after = lower[i + word.len()..].chars().next();
                !before.is_some_and(char::is_alphanumeric)
                    && !after.is_some_and(char::is_alphanumeric)
            });
            assert!(
                at.is_none(),
                "{}: contains the placeholder {word:?}",
                page.name
            );
        }
        // A picture is only allowed when its file is there.
        for (at, _) in page.text.match_indices("![") {
            let rest = &page.text[at..];
            let target = rest
                .split_once("](")
                .and_then(|(_, tail)| tail.split_once(')'))
                .map(|(target, _)| target.trim().to_owned())
                .unwrap_or_default();
            let path = root()
                .join("website/content")
                .join(Path::new(&page.name).parent().unwrap())
                .join(&target);
            assert!(
                path.is_file(),
                "{}: the picture {target:?} does not exist",
                page.name
            );
        }
        assert!(
            !page.text.contains("<img"),
            "{}: use ![](file) so the file is checked",
            page.name
        );
    }
}

#[test]
fn download_buttons_match_what_the_release_workflow_builds() {
    let workflow = fs::read_to_string(root().join(".github/workflows/release.yml"))
        .unwrap()
        .replace("\r\n", "\n");
    let page = pages()
        .into_iter()
        .find(|p| p.name == "get-started/download.md")
        .unwrap();
    let mut seen = 0;
    for (at, _) in page.text.match_indices("data-asset=\"") {
        let rest = &page.text[at + "data-asset=\"".len()..];
        let suffix = &rest[..rest.find('"').unwrap()];
        seen += 1;
        // "-windows-x64.msi" -> platform "windows", extension ".msi"
        let trimmed = suffix.trim_start_matches('-');
        let platform = trimmed.split('-').next().unwrap();
        let extension = &trimmed[trimmed.rfind('.').unwrap()..];
        assert!(
            workflow.contains(platform),
            "no {platform} build in release.yml for {suffix}"
        );
        assert!(
            workflow.contains(extension),
            "release.yml never builds a {extension} file ({suffix})"
        );
        if suffix.ends_with("portable.zip") {
            assert!(workflow.contains("-portable.zip"));
        }
        if suffix.starts_with("-macos") {
            assert!(workflow.contains("macos-universal"));
        }
    }
    assert!(seen >= 8, "found only {seen} download buttons");
}

// ---------------------------------------------------------------------------------------------
// The MCP guide against the real servers
// ---------------------------------------------------------------------------------------------
#[cfg(feature = "mcp")]
mod mcp_guide {
    use super::*;
    use rmcp::ServiceExt;
    use serde_json::{json, Value};
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

    /// What the servers offer.
    struct Offer {
        /// Tools of `chromascope-mcp` in the full profile.
        tools: BTreeSet<String>,
        /// Every property name that a tool accepts.
        parameters: BTreeSet<String>,
        prompts: BTreeSet<String>,
        templates: BTreeSet<String>,
        /// Tools of `chromascope-engine-mcp`.
        engine_tools: BTreeSet<String>,
        desktop_flags: BTreeSet<String>,
        engine_flags: BTreeSet<String>,
        /// Engine operation names (the `operation` values of `start_analysis`).
        operations: BTreeSet<String>,
    }

    fn collect_properties(value: &Value, names: &mut BTreeSet<String>) {
        match value {
            Value::Object(map) => {
                if let Some(Value::Object(properties)) = map.get("properties") {
                    names.extend(properties.keys().cloned());
                }
                map.values().for_each(|v| collect_properties(v, names));
            }
            Value::Array(items) => items.iter().for_each(|v| collect_properties(v, names)),
            _ => {}
        }
    }

    fn collect_operations(value: &Value, names: &mut BTreeSet<String>) {
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
                map.values().for_each(|v| collect_operations(v, names));
            }
            Value::Array(items) => items.iter().for_each(|v| collect_operations(v, names)),
            _ => {}
        }
    }

    fn flags_in(text: &str) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        while i + 2 < chars.len() {
            let starts = chars[i] == '-'
                && chars[i + 1] == '-'
                && chars[i + 2].is_ascii_lowercase()
                && (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '-'));
            if starts {
                let mut end = i + 2;
                while end < chars.len()
                    && (chars[end].is_ascii_lowercase()
                        || chars[end] == '-'
                        || chars[end].is_ascii_digit())
                {
                    end += 1;
                }
                let flag: String = chars[i..end].iter().collect();
                found.insert(flag.trim_end_matches('-').to_owned());
                i = end;
            } else {
                i += 1;
            }
        }
        found
    }

    fn help_flags(exe: &str) -> BTreeSet<String> {
        let output = Command::new(exe).arg("--help").output().unwrap();
        assert!(output.status.success(), "{exe} --help failed");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let flags = flags_in(&text);
        assert!(flags.contains("--allow-root"), "{exe} --help: {text}");
        flags
    }

    /// Asks a server over stdio (a child process) for its tools.
    fn engine_tools(exe: &str) -> BTreeSet<String> {
        let root = tempfile::tempdir().unwrap();
        let mut child = Command::new(exe)
            .args(["--allow-root", root.path().to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut send = |value: Value| {
            writeln!(stdin, "{value}").unwrap();
            stdin.flush().unwrap();
        };
        send(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"website-test","version":"1"}}}),
        );
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        send(json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}));
        line.clear();
        stdout.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        drop(stdin);
        let _ = child.kill();
        let _ = child.wait();
        reply["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_owned())
            .collect()
    }

    fn desktop_offer() -> (
        BTreeSet<String>,
        BTreeSet<String>,
        BTreeSet<String>,
        BTreeSet<String>,
    ) {
        use chromascope::mcp::{AccessPolicy, McpServer, ToolProfile};
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let root = tempfile::tempdir().unwrap();
            let policy = AccessPolicy::new(vec![root.path().to_owned()], true, true)
                .unwrap()
                .with_project_writes(true);
            let (server, bridge) = McpServer::new(egui::Context::default(), policy).unwrap();
            let server = server.with_tool_profile(ToolProfile::Full);
            let (client, transport) = tokio::io::duplex(1 << 22);
            let task = tokio::spawn(async move {
                let _window = bridge;
                let _ = server.serve(transport).await.unwrap().waiting().await;
            });
            let (read, mut write) = tokio::io::split(client);
            let mut read = tokio::io::BufReader::new(read);
            let mut line = String::new();
            let request = |id: u32, method: &str| {
                format!(
                    "{}\n",
                    json!({"jsonrpc":"2.0","id":id,"method":method,"params":if method == "initialize" {json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"website-test","version":"1"}})} else {json!({})}})
                )
            };
            write.write_all(request(1, "initialize").as_bytes()).await.unwrap();
            read.read_line(&mut line).await.unwrap();
            write
                .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
                .await
                .unwrap();
            let mut ask = async |id: u32, method: &str| -> Value {
                write.write_all(request(id, method).as_bytes()).await.unwrap();
                let mut reply = String::new();
                read.read_line(&mut reply).await.unwrap();
                serde_json::from_str(&reply).unwrap()
            };
            let tools = ask(2, "tools/list").await;
            let prompts = ask(3, "prompts/list").await;
            let templates = ask(4, "resources/templates/list").await;
            task.abort();
            let names = |reply: &Value, list: &str, key: &str| -> BTreeSet<String> {
                reply["result"][list]
                    .as_array()
                    .unwrap_or_else(|| panic!("{list}: {reply}"))
                    .iter()
                    .map(|item| item[key].as_str().unwrap().to_owned())
                    .collect()
            };
            let mut parameters = BTreeSet::new();
            collect_properties(&tools["result"]["tools"], &mut parameters);
            (
                names(&tools, "tools", "name"),
                parameters,
                names(&prompts, "prompts", "name"),
                names(&templates, "resourceTemplates", "uriTemplate"),
            )
        })
    }

    fn offer() -> Offer {
        let (tools, parameters, prompts, templates) = desktop_offer();
        let schema =
            serde_json::to_value(rmcp::schemars::schema_for!(chromascope::domain::Request))
                .unwrap();
        let mut operations = BTreeSet::new();
        collect_operations(&schema, &mut operations);
        let desktop_flags = help_flags(env!("CARGO_BIN_EXE_chromascope-mcp"));
        let engine_flags = help_flags(env!("CARGO_BIN_EXE_chromascope-engine-mcp"));
        Offer {
            tools,
            parameters,
            prompts,
            templates,
            engine_tools: engine_tools(env!("CARGO_BIN_EXE_chromascope-engine-mcp")),
            desktop_flags,
            engine_flags,
            operations,
        }
    }

    /// Tools that exist only with the `experimental` feature.
    const HIDDEN_TOOLS: [&str; 4] = [
        "start_untargeted",
        "untargeted_status",
        "cancel_untargeted",
        "untargeted_features",
    ];

    fn mcp_pages() -> Vec<Page> {
        pages()
            .into_iter()
            .filter(|p| p.name.starts_with("mcp/"))
            .collect()
    }

    fn page<'a>(pages: &'a [Page], name: &str) -> &'a Page {
        pages
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("no page {name}"))
    }

    fn is_snake_case(token: &str) -> bool {
        let mut words = token.split('_');
        let first = words.next().unwrap_or("");
        !first.is_empty()
            && first.chars().next().unwrap().is_ascii_lowercase()
            && first
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            && token.contains('_')
            && words.all(|w| {
                !w.is_empty()
                    && w.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            })
    }

    /// `<id>` and `<n>` in the guide are `{id}` and `{n}` in the server.
    fn normalise_uri(uri: &str) -> String {
        uri.replace("<id>", "{id}").replace("<n>", "{n}")
    }

    fn uris_in(text: &str) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        for (at, _) in text.match_indices("chromascope://") {
            let rest = &text[at..];
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || "/:<>{}_-".contains(c)))
                .unwrap_or(rest.len());
            found.insert(normalise_uri(&rest[..end]));
        }
        found
    }

    #[test]
    fn the_guide_names_only_tools_prompts_resources_and_flags_that_exist() {
        let offer = offer();
        let hidden = !chromascope::EXPERIMENTAL;
        assert!(offer.tools.len() >= 70, "{} tools", offer.tools.len());
        assert!(offer.engine_tools.is_subset(&offer.tools));
        assert_eq!(offer.prompts.len(), 6);
        assert_eq!(
            offer.engine_flags,
            ["--allow-project-writes", "--allow-root"]
                .map(String::from)
                .into(),
            "the headless program's flags changed: update the guide"
        );
        let all_flags: BTreeSet<String> = offer
            .desktop_flags
            .union(&offer.engine_flags)
            .cloned()
            .collect();
        let external_flags = ["--help", "--appimage-extract", "--appimage-extract-and-run"];
        let mut known: BTreeSet<String> = offer.tools.union(&offer.engine_tools).cloned().collect();
        known.extend(offer.prompts.iter().cloned());
        known.extend(offer.parameters.iter().cloned());
        known.extend(offer.operations.iter().cloned());
        let mut statics: BTreeSet<String> = chromascope::mcp::static_resource_uris()
            .iter()
            .map(|u| (*u).to_owned())
            .collect();
        statics.extend(offer.templates.iter().map(|t| normalise_uri(t)));
        let mut problems = vec![];
        // The README states the counts too.
        let readme = Page {
            name: "README.md".into(),
            text: fs::read_to_string(root().join("README.md"))
                .unwrap()
                .replace("\r\n", "\n"),
        };
        for page in mcp_pages().into_iter().chain([readme]) {
            let parts = parts(&page.text);
            // The README is only checked for its counts (it talks about cargo flags too).
            if page.name != "README.md" {
                // Names written in code: every snake_case word must be something the servers know.
                for token in &parts.inline {
                    let token = token.trim();
                    if is_snake_case(token) && !known.contains(token) {
                        problems.push(format!(
                            "{}: `{token}` is not a tool, prompt, parameter or operation",
                            page.name
                        ));
                    }
                }
                // Flags, anywhere on the page.
                for flag in flags_in(&page.text) {
                    if !all_flags.contains(&flag) && !external_flags.contains(&flag.as_str()) {
                        problems.push(format!(
                            "{}: {flag} is not a launch flag of either program",
                            page.name
                        ));
                    }
                }
                // A configuration for one program may only use that program's flags.
                for block in &parts.fences {
                    let flags = flags_in(block);
                    if block.contains("chromascope-engine-mcp") {
                        for flag in flags.iter().filter(|f| {
                            !offer.engine_flags.contains(*f)
                                && !external_flags.contains(&f.as_str())
                        }) {
                            problems.push(format!(
                                "{}: {flag} in an example for the headless program",
                                page.name
                            ));
                        }
                    } else if block.contains("chromascope-mcp") {
                        for flag in flags.iter().filter(|f| {
                            !offer.desktop_flags.contains(*f)
                                && !external_flags.contains(&f.as_str())
                        }) {
                            problems.push(format!(
                                "{}: {flag} in an example for chromascope-mcp",
                                page.name
                            ));
                        }
                    }
                }
                // Resource addresses.
                for uri in uris_in(&page.text) {
                    if !statics.contains(&uri) {
                        problems.push(format!("{}: there is no resource {uri}", page.name));
                    }
                }
                // "Tools:" lines of the example prompts name tools, parameters and operations only.
                for line in page.text.lines().filter(|l| l.starts_with("- *Tools:*")) {
                    for token in parts_of_line(line) {
                        if !known.contains(&token) {
                            problems.push(format!(
                                "{}: `{token}` in a Tools line is unknown",
                                page.name
                            ));
                        }
                    }
                }
            }
            // Counts: "74 tools", "the 40 tools", "14 calculation tools".
            let words: Vec<&str> = page.text.split_whitespace().collect();
            for (i, word) in words.iter().enumerate() {
                let core = word.trim_matches(|c: char| !c.is_alphanumeric());
                if core.is_empty() || !core.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                let Ok(number) = core.parse::<usize>() else {
                    continue;
                };
                let follows = words[i + 1..words.len().min(i + 3)]
                    .iter()
                    .any(|w| w.trim_matches(|c: char| !c.is_alphanumeric()) == "tools");
                if follows && hidden {
                    let allowed = [
                        offer.tools.len(),
                        offer.engine_tools.len(),
                        chromascope::mcp::CORE_TOOLS.len(),
                    ];
                    if !allowed.contains(&number) {
                        problems.push(format!(
                            "{}: \"{number} tools\" but the servers offer {allowed:?}",
                            page.name
                        ));
                    }
                }
            }
        }
        assert!(
            problems.is_empty(),
            "the MCP guide is out of step with the program:\n  {}",
            problems.join("\n  ")
        );
    }

    /// The tokens in backticks on one line.
    fn parts_of_line(line: &str) -> Vec<String> {
        line.split('`')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn everything_the_servers_offer_is_in_the_guide() {
        let offer = offer();
        let pages = mcp_pages();
        let overview = &page(&pages, "mcp/index.md").text;
        let missing: Vec<&String> = offer
            .tools
            .iter()
            .filter(|t| !HIDDEN_TOOLS.contains(&t.as_str()))
            .filter(|t| !overview.contains(&format!("`{t}`")))
            .collect();
        assert!(
            missing.is_empty(),
            "tools missing from mcp/index.md: {missing:?}"
        );
        let prompts = &page(&pages, "mcp/workflow-prompts.md").text;
        for prompt in &offer.prompts {
            assert!(
                prompts.contains(&format!("### {prompt}")),
                "no section for the prompt {prompt}"
            );
        }
        let resources = &page(&pages, "mcp/resources.md").text;
        let written = uris_in(resources);
        let mut offered: BTreeSet<String> = chromascope::mcp::static_resource_uris()
            .iter()
            .map(|u| (*u).to_owned())
            .collect();
        offered.extend(offer.templates.iter().map(|t| normalise_uri(t)));
        assert_eq!(
            written, offered,
            "mcp/resources.md and the servers disagree"
        );
        // Every launch flag of both programs is explained in the setup page.
        let setup = &page(&pages, "mcp/setup.md").text;
        for flag in offer
            .desktop_flags
            .iter()
            .chain(&offer.engine_flags)
            .filter(|f| *f != "--help")
        {
            assert!(
                setup.contains(&format!("`{flag}")),
                "setup.md never explains {flag}"
            );
        }
    }

    #[test]
    fn the_list_of_tools_outside_the_core_profile_is_right() {
        let offer = offer();
        let pages = mcp_pages();
        let overview = &page(&pages, "mcp/index.md").text;
        let start = overview.find("<!-- not-in-core -->").expect("start marker");
        let end = overview
            .find("<!-- end-not-in-core -->")
            .expect("end marker");
        let written: BTreeSet<String> = overview[start..end]
            .lines()
            .flat_map(parts_of_line)
            .collect();
        let core: BTreeSet<&str> = chromascope::mcp::CORE_TOOLS.iter().copied().collect();
        assert_eq!(chromascope::mcp::CORE_TOOLS.len(), 40);
        let expected: BTreeSet<String> = offer
            .tools
            .iter()
            .filter(|t| !core.contains(t.as_str()) && !HIDDEN_TOOLS.contains(&t.as_str()))
            .cloned()
            .collect();
        assert_eq!(
            written, expected,
            "mcp/index.md lists the wrong tools as outside the core profile"
        );
    }

    #[test]
    fn the_example_prompts_are_complete() {
        let pages = mcp_pages();
        let text = &page(&pages, "mcp/example-prompts.md").text;
        let prompts = text.matches("```text").count();
        assert!(prompts >= 25, "only {prompts} example prompts");
        for label in [
            "- *The assistant will:*",
            "- *Tools:*",
            "- *Flags:*",
            "- *Check:*",
        ] {
            assert_eq!(
                text.matches(label).count(),
                prompts,
                "every prompt needs a {label} line"
            );
        }
    }
}
