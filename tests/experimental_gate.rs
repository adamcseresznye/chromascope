//! The experimental-workflow gate lives in `engine::execute`, so every route into the engine is
//! covered, not just the CLI and MCP callers: re-running stored results and AI proposals too.
#![cfg(not(feature = "experimental"))]
use chromascope::{
    delivery,
    domain::*,
    engine,
    jobs::JobControl,
    project::Project,
    proposals::{self, Proposal},
    spectral::LibrarySource,
};
use std::path::{Path, PathBuf};

fn experimental_request() -> Request {
    Request {
        version: 1,
        operation_id: OperationId::default(),
        actor: "gate-test".into(),
        operation: Operation::ImportSpectralLibrary {
            text: String::new(),
            format: "msp".into(),
            source: LibrarySource {
                name: "n".into(),
                version: "1".into(),
                url: "u".into(),
                license: "l".into(),
            },
        },
    }
}
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("test_file/data_dependent_02.mzML")
}
fn rejected<T>(result: Result<T>) -> EngineError {
    match result {
        Err(e) => e,
        Ok(_) => panic!("the experimental operation must be rejected"),
    }
}
fn code(e: &EngineError) -> &str {
    &e.code
}

#[test]
fn engine_execute_rejects_experimental_operations() {
    let e = rejected(engine::execute(
        &fixture(),
        experimental_request(),
        &JobControl::default(),
    ));
    assert_eq!(code(&e), "experimental_unavailable");
}

#[test]
fn proposals_reject_experimental_operations() {
    let e = rejected(proposals::prepare(
        &fixture(),
        experimental_request(),
        None,
        "reason".into(),
        vec!["evidence".into()],
        &JobControl::default(),
    ));
    assert_eq!(code(&e), "experimental_unavailable");

    let proposal = Proposal {
        version: 1,
        id: uuid::Uuid::new_v4(),
        request: experimental_request(),
        expected_project_revision: None,
        before: serde_json::json!({}),
        before_sha256: String::new(),
        proposed: serde_json::json!({}),
        reason: "r".into(),
        evidence: vec!["e".into()],
        events: vec![],
        applied: None,
    };
    let e = rejected(proposals::resolve(
        &fixture(),
        &proposal,
        &serde_json::json!({}),
        None,
        true,
        "reviewer",
        "approve",
        &JobControl::default(),
    ));
    assert_eq!(code(&e), "experimental_unavailable");
}

/// A project written by an experimental build may hold an experimental result; reprocessing it in
/// a shipped build must be refused instead of running the hidden operation.
#[test]
fn reprocess_rejects_stored_experimental_results() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let mut p = Project::create(&root).unwrap();
    let dataset = p.register(&fixture()).unwrap();
    let mut response = engine::execute(
        &fixture(),
        Request {
            version: 1,
            operation_id: OperationId::default(),
            actor: "gate-test".into(),
            operation: Operation::Integrate {
                points: vec![[0., 0.], [1., 10.], [2., 0.]],
                start: Minutes(0.),
                end: Minutes(2.),
            },
        },
        &JobControl::default(),
    )
    .unwrap();
    // Simulate the stored record of an experimental run.
    response.request = experimental_request();
    p.add_result(&root, dataset, &response).unwrap();
    p.commit(&root, 0).unwrap();
    let e = rejected(delivery::reprocess(&root, response.result_id));
    assert_eq!(code(&e), "experimental_unavailable");
}
