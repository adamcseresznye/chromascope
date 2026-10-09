//! Real adapter regression: documented release example retains scientific missing
//! states, a failed QC after acknowledgement, and local reference search evidence.
#[test]
fn release_example_retains_evidence_and_refuses_existing_directory() {
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("evidence");
    let run = || {
        std::process::Command::new("python")
            .arg("examples/release_workflows.py")
            .arg("--cli")
            .arg(env!("CARGO_BIN_EXE_chromascope-cli"))
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap()
    };
    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let bytes = std::fs::read(output.join("summary.json")).unwrap();
    let summary: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        summary["targeted_concentrations"],
        serde_json::json!([null])
    );
    assert_eq!(summary["original_qc_status"], "fail");
    assert_eq!(summary["reviewed_qc_status"], "fail");
    assert_eq!(summary["qc_review_count"], 1);
    assert!(summary["spectral_candidates"].as_u64().unwrap() >= 1);
    assert!(output.join("targeted-export-csv.csv").is_file());
    assert!(output.join("qc-export-csv.csv").is_file());
    assert!(output.join("spectral-export-csv.csv").is_file());
    assert!(!run().status.success());
    assert_eq!(std::fs::read(output.join("summary.json")).unwrap(), bytes);
}

#[test]
fn cli_help_and_content_verified_relink_preserve_history() {
    use chromascope::project::Project;
    let cli = env!("CARGO_BIN_EXE_chromascope-cli");
    let help = std::process::Command::new(cli)
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8(help.stdout)
        .unwrap()
        .contains("project-relink"));
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("project");
    let mut project = Project::create(&root).unwrap();
    let original = temporary.path().join("original.mzML");
    std::fs::copy("test_file/data_dependent_02.mzML", &original).unwrap();
    let id = project.register(&original).unwrap();
    project.commit(&root, project.revision).unwrap();
    let revision = project.revision;
    let moved = temporary.path().join("moved.mzML");
    std::fs::copy(&original, &moved).unwrap();
    let identifier = serde_json::to_value(id).unwrap();
    let relink = |path: &std::path::Path| {
        std::process::Command::new(cli)
            .arg("project-relink")
            .arg(&root)
            .arg(identifier.as_str().unwrap())
            .arg(path)
            .output()
            .unwrap()
    };
    assert!(relink(&moved).status.success());
    let reopened = Project::open(&root).unwrap();
    assert_eq!(reopened.revision, revision + 1);
    assert_eq!(reopened.sources[0].id, id);
    assert_eq!(
        reopened.verified_source(id).unwrap(),
        moved.canonicalize().unwrap()
    );
    let changed = temporary.path().join("changed.mzML");
    std::fs::write(&changed, "different bytes").unwrap();
    assert!(!relink(&changed).status.success());
    assert_eq!(Project::open(&root).unwrap().revision, reopened.revision);
    assert_eq!(
        std::fs::read(&original).unwrap(),
        std::fs::read(&moved).unwrap()
    );
}
