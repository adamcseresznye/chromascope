use chromascope::{
    domain::{Operation, Request},
    jobs::JobControl,
    proposals, qc,
};
fn proposal() -> proposals::Proposal {
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("reference/qc.json")).unwrap();
    let study = serde_json::from_value(reference["study"].clone()).unwrap();
    let report = qc::evaluate(study).unwrap();
    let operation = Operation::ReviewQc {
        expected_revision: 0,
        rule_id: report.decisions[0].rule.id.clone(),
        reason: "Inspected original evidence".into(),
        acknowledged: true,
        report: Box::new(report),
    };
    proposals::prepare(
        std::path::Path::new("-"),
        Request {
            version: 1,
            operation_id: Default::default(),
            actor: "agent".into(),
            operation,
        },
        Some(7),
        "Evidence inspected".into(),
        vec!["qc-rule:0".into()],
        &JobControl::default(),
    )
    .unwrap()
}
#[test]
fn proposal_is_a_suggestion_until_attributed_resolution() {
    let pending = proposal();
    assert!(pending.events.is_empty());
    assert!(pending.applied.is_none());
    assert!(pending.before["reviews"].as_array().unwrap().is_empty());
    let control = JobControl::default();
    let (rejected, response) = proposals::resolve(
        std::path::Path::new("-"),
        &pending,
        &pending.before,
        Some(7),
        false,
        "reviewer",
        "Evidence insufficient",
        &control,
    )
    .unwrap();
    assert!(response.is_none());
    assert!(rejected.applied.is_none());
    assert_eq!(rejected.events[0].decision, "rejected");
    let (approved, response) = proposals::resolve(
        std::path::Path::new("-"),
        &pending,
        &pending.before,
        Some(7),
        true,
        "reviewer",
        "Checked against source",
        &control,
    )
    .unwrap();
    assert!(response.is_some());
    assert_eq!(approved.events[0].actor, "reviewer");
    let reversal =
        proposals::reversal(&approved, "reviewer", "Restore original acknowledgement").unwrap();
    if let Operation::ReviewQc {
        acknowledged,
        report,
        expected_revision,
        ..
    } = reversal.operation
    {
        assert!(!acknowledged);
        assert_eq!(expected_revision, 1);
        assert_eq!(report.reviews.len(), 1);
    } else {
        panic!("Wrong reversal operation");
    }
    assert!(proposals::resolve(
        std::path::Path::new("-"),
        &approved,
        &pending.before,
        Some(7),
        true,
        "reviewer",
        "Replay",
        &control
    )
    .is_err());
}
#[test]
fn stale_evidence_revision_and_tampered_preview_are_rejected() {
    let pending = proposal();
    let control = JobControl::default();
    let path = std::path::Path::new("-");
    assert!(proposals::resolve(
        path,
        &pending,
        &pending.before,
        Some(8),
        true,
        "reviewer",
        "Check",
        &control
    )
    .is_err());
    let mut current = pending.before.clone();
    current["status"] = serde_json::json!("fail");
    assert!(proposals::resolve(
        path,
        &pending,
        &current,
        Some(7),
        true,
        "reviewer",
        "Check",
        &control
    )
    .is_err());
    let mut tampered = pending.clone();
    tampered.proposed["status"] = serde_json::json!("fail");
    assert!(proposals::resolve(
        path,
        &tampered,
        &pending.before,
        Some(7),
        true,
        "reviewer",
        "Check",
        &control
    )
    .is_err());
}

#[cfg(feature = "mcp-headless")]
#[test]
fn actual_mcp_prepares_the_same_revision_bound_suggestion() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    let pending = proposal();
    let root = std::env::current_dir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-engine-mcp"))
        .arg("--allow-root")
        .arg(&root)
        .arg("--allow-project-writes")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    fn send(input: &mut impl Write, value: serde_json::Value) {
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
    }
    fn read(output: &mut impl BufRead) -> serde_json::Value {
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"proposal-contract-test","version":"1"}}}),
    );
    assert!(read(&mut output).get("result").is_some());
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"prepare_review_proposal","arguments":{"path":root.join("Cargo.toml"),"request":pending.request,"expected_project_revision":7,"reason":pending.reason,"evidence":pending.evidence}}}),
    );
    let response = read(&mut output);
    assert_ne!(response["result"]["isError"], true, "{response}");
    let actual: proposals::Proposal =
        serde_json::from_value(response["result"]["structuredContent"].clone()).unwrap();
    assert_eq!(actual.before, pending.before);
    assert_eq!(actual.before_sha256, pending.before_sha256);
    assert_eq!(actual.expected_project_revision, Some(7));
    assert!(actual.applied.is_none() && actual.events.is_empty());
    let (_, applied) = proposals::resolve(
        std::path::Path::new("-"),
        &actual,
        &actual.before,
        Some(7),
        true,
        "reviewer",
        "Evidence inspected",
        &JobControl::default(),
    )
    .unwrap();
    assert!(applied.is_some());
    let temp = tempfile::tempdir_in(root.join("target")).unwrap();
    let project_root = temp.path().join("project");
    let mut project = chromascope::project::Project::create(&project_root).unwrap();
    let source = root.join("Cargo.toml");
    let dataset = project.register(&source).unwrap();
    let study = serde_json::from_value(pending.before["study"].clone()).unwrap();
    let evaluated = chromascope::engine::execute(
        &source,
        Request {
            version: 1,
            operation_id: Default::default(),
            actor: "reference".into(),
            operation: Operation::EvaluateQc { study },
        },
        &JobControl::default(),
    )
    .unwrap();
    let report = if let chromascope::engine::Output::QcReport { report } = &evaluated.output {
        report.clone()
    } else {
        panic!("Expected QC report")
    };
    project
        .add_result(&project_root, dataset, &evaluated)
        .unwrap();
    project.commit(&project_root, 0).unwrap();
    let request = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "agent".into(),
        operation: Operation::ReviewQc {
            expected_revision: 0,
            rule_id: report.decisions[0].rule.id.clone(),
            reason: "Checked reference".into(),
            acknowledged: true,
            report,
        },
    };
    let suggestion = proposals::prepare(
        &source,
        request,
        Some(project.revision),
        "Reference inspected".into(),
        vec!["qc-reference".into()],
        &JobControl::default(),
    )
    .unwrap();
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"queue_review_proposal","arguments":{"directory":project_root,"dataset_id":dataset.0.to_string(),"proposal":suggestion}}}),
    );
    let queued = read(&mut output);
    assert_ne!(queued["result"]["isError"], true, "{queued}");
    let queued: proposals::Proposal =
        serde_json::from_value(queued["result"]["structuredContent"]["proposal"].clone()).unwrap();
    assert_eq!(queued.expected_project_revision, Some(2));
    assert!(queued.events.is_empty() && queued.applied.is_none());
    let retained = chromascope::project::Project::open(&project_root).unwrap();
    assert_eq!(retained.results.len(), 1);
    assert_eq!(retained.review_proposals.len(), 1);
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"resolve_review_proposal","arguments":{"directory":project_root,"dataset_id":dataset.0.to_string(),"proposal":queued,"approve":false,"actor":"human-reviewer","reason":"Insufficient evidence"}}}),
    );
    let rejected = read(&mut output);
    assert_ne!(rejected["result"]["isError"], true, "{rejected}");
    let retained = chromascope::project::Project::open(&project_root).unwrap();
    assert_eq!(retained.results.len(), 1);
    assert_eq!(retained.review_proposals.len(), 1);
    assert_eq!(retained.review_proposals[0].events[0].decision, "rejected");
    drop(input);
    child.kill().unwrap();
    child.wait().unwrap();
}
