use chromascope::{
    delivery::{self, Config},
    domain::*,
    engine,
    jobs::JobControl,
    project::Project,
};
use serde_json::json;
use std::{fs, path::Path, process::Command};
fn request(operation: Operation) -> Request {
    Request {
        version: 1,
        operation_id: OperationId::default(),
        actor: "delivery-reference".into(),
        operation,
    }
}
fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("test_file/data_dependent_02.mzML")
}
#[test]
fn portable_roundtrip_replay_history_and_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("original");
    let mut p = Project::create(&root).unwrap();
    let dataset = p.register(&fixture()).unwrap();
    let response = engine::execute(
        &fixture(),
        request(Operation::Integrate {
            points: vec![[0., 0.], [1., 10.], [2., 0.]],
            start: Minutes(0.),
            end: Minutes(2.),
        }),
        &JobControl::default(),
    )
    .unwrap();
    let id = response.result_id;
    p.add_result(&root, dataset, &response).unwrap();
    p.commit(&root, 0).unwrap();
    let bundle = temp.path().join("bundle");
    delivery::export(&root, &bundle, &Config::default(), true).unwrap();
    delivery::verify_bundle(&bundle).unwrap();
    let moved = temp.path().join("moved");
    fs::rename(&bundle, &moved).unwrap();
    let reopened = Project::open(&moved).unwrap();
    assert_eq!(reopened.id, p.id);
    assert_eq!(reopened.revision, 1);
    assert!(reopened
        .verified_source(dataset)
        .unwrap()
        .starts_with(fs::canonicalize(&moved).unwrap()));
    assert_eq!(delivery::reprocess(&moved, id).unwrap()["reproduced"], true);
    // Independent triangular area, with explicit intensity*minute units.
    let result = reopened.load_result(&moved, id).unwrap();
    assert_eq!(serde_json::to_value(result.output).unwrap()["area"], 10.0);
    let original = fs::read(root.join("revision-00000000000000000001.json")).unwrap();
    assert_eq!(
        original,
        fs::read(moved.join("revision-00000000000000000001.json")).unwrap()
    );
    Project::restore_revision(&moved, 0, 1).unwrap();
    assert_eq!(Project::open(&moved).unwrap().results.len(), 0);
    // Restore retains artifacts/history; new commits explicitly invalidate export inventory.
    assert!(delivery::verify_bundle(&moved).is_err());
    fs::write(moved.join("report.html"), "changed").unwrap();
    assert!(delivery::verify_bundle(&moved).is_err());
}
#[test]
fn missing_source_inspection_and_no_overwrite() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.mzML");
    fs::copy(fixture(), &source).unwrap();
    let root = temp.path().join("project");
    let mut p = Project::create(&root).unwrap();
    p.register(&source).unwrap();
    p.commit(&root, 0).unwrap();
    fs::rename(&source, temp.path().join("detached.mzML")).unwrap();
    let report = delivery::report(&root, &Config::default()).unwrap();
    assert_eq!(
        report["source_verification"][0]["state"],
        "unavailable_or_changed"
    );
    assert!(delivery::export(
        &root,
        &temp.path().join("portable"),
        &Config::default(),
        true
    )
    .is_err());
    assert!(!temp.path().join("portable").exists());
    let out = temp.path().join("report");
    delivery::export(&root, &out, &Config::default(), false).unwrap();
    let bytes = fs::read(out.join("report.json")).unwrap();
    assert!(delivery::export(&root, &out, &Config::default(), false).is_err());
    assert_eq!(bytes, fs::read(out.join("report.json")).unwrap());
}
#[test]
fn escaped_report_tsv_svg_and_method_comparison() {
    assert_eq!(
        delivery::tsv("a,b\n\"x,y\",\"z\"\"q\"\n").unwrap(),
        "a\tb\n\"x,y\"\t\"z\"\"q\"\n"
    );
    assert!(delivery::tsv("\"unterminated").is_err());
    let c = Config {
        title: "<script>unsafe</script>".into(),
        ..Config::default()
    };
    let html =
        delivery::html(&json!({"config":c,"results":[],"project":{},"software":{}})).unwrap();
    assert!(!html.contains("<script>"));
    assert!(html.contains("&lt;script&gt;"));
    let svg = delivery::svg(&[[0., 0.], [1., 10.], [2., 0.]], "minute", "intensity", &c).unwrap();
    assert!(svg.contains("width=\"2400\""));
    assert!(svg.contains("minute: 0 to 2"));
    assert!(svg.contains("intensity: 0 to 10"));
    assert!(delivery::svg(&[[f64::NAN, 0.]], "x", "y", &c).is_err());
    let missing = delivery::compare_methods(&json!({}), &json!({"threshold":null}));
    assert_eq!(missing["differences"][0]["left_state"], "missing");
    let comparison = delivery::compare_methods(&json!({"a/b":1}), &json!({"a/b":2}));
    assert_eq!(comparison["differences"][0]["pointer"], "/a~1b");
}
#[test]
fn cli_export_and_schema_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    Project::create(&root).unwrap();
    let out = temp.path().join("out");
    let output = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
        .args([
            "project-report",
            root.to_str().unwrap(),
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    delivery::verify_bundle(&out).unwrap();
    let revision = root.join("revision-00000000000000000000.json");
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(&revision).unwrap()).unwrap();
    for k in ["failures", "agent_commits", "restored_from_revision"] {
        v.as_object_mut().unwrap().remove(k);
    }
    fs::write(&revision, serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(Project::open(&root).unwrap().agent_commits.is_empty());
    v["schema_version"] = json!(999);
    fs::write(&revision, serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(Project::open(&root).is_err());
}
#[test]
fn raw_extraction_reproduces_after_portable_relocation() {
    use chromascope::{plotting_parameters::PlotType, processing::ProcessingParams};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let mut p = Project::create(&root).unwrap();
    let dataset = p.register(&fixture()).unwrap();
    let params = ProcessingParams {
        acquisition: None,
        plot_type: PlotType::Tic,
        ms_level: 1,
        polarity: mzdata::spectrum::ScanPolarity::Positive,
        smoothing: 0,
        xic_params: None,
        mz_range: None,
        precursor_mz: None,
    };
    let r = engine::execute(
        &fixture(),
        request(Operation::Extract { params }),
        &JobControl::default(),
    )
    .unwrap();
    let id = r.result_id;
    p.add_result(&root, dataset, &r).unwrap();
    p.commit(&root, 0).unwrap();
    let out = temp.path().join("bundle");
    delivery::export(&root, &out, &Config::default(), true).unwrap();
    assert!(out.join(format!("{}.svg", id.0)).exists());
    assert_eq!(delivery::reprocess(&out, id).unwrap()["reproduced"], true);
    let source = Project::open(&out)
        .unwrap()
        .verified_source(dataset)
        .unwrap();
    fs::rename(source, out.join("missing.mzML")).unwrap();
    assert!(Project::open(&out).is_ok());
    assert!(delivery::reprocess(&out, id).is_err());
    assert!(delivery::verify_bundle(&out).is_err());
}

include!("fixtures/targeted_support.rs");
#[test]
fn calibration_quantification_tables_and_replay_reference() {
    let temp = tempfile::tempdir().unwrap();
    let batch = raw_request(temp.path());
    let source = std::path::PathBuf::from(&batch.samples[0].source);
    let root = temp.path().join("project");
    let mut p = Project::create(&root).unwrap();
    for sample in &batch.samples {
        p.register(Path::new(&sample.source)).unwrap();
    }
    let dataset = p.register(&source).unwrap();
    let r = engine::execute(
        &source,
        request(Operation::TargetedBatch { batch }),
        &JobControl::default(),
    )
    .unwrap();
    let id = r.result_id;
    if let engine::Output::TargetedQuantification { batch } = &r.output {
        let unknown_id = batch
            .request
            .samples
            .iter()
            .find(|s| s.role == chromascope::targeted::Role::Unknown)
            .unwrap()
            .id
            .clone();
        let unknown = batch
            .results
            .iter()
            .find(|r| r.sample == unknown_id && r.target == "a")
            .unwrap();
        assert!((unknown.concentration.unwrap() - 10.0).abs() < 1e-7);
    }
    p.add_result(&root, dataset, &r).unwrap();
    p.commit(&root, 0).unwrap();
    let out = temp.path().join("bundle");
    delivery::export(&root, &out, &Config::default(), true).unwrap();
    assert!(out.join(format!("{}-calibration.csv", id.0)).exists());
    assert!(out.join(format!("{}-quantification.tsv", id.0)).exists());
    assert!(out
        .join(format!("{}-calibration-0-fitted.svg", id.0))
        .exists());
    let replay = delivery::reprocess(&out, id).unwrap();
    assert_eq!(replay["reproduced"], true, "{replay}");
    assert_eq!(
        replay["excluded_run_metadata_pointers"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    delivery::verify_bundle(&out).unwrap();
}

#[test]
fn unsafe_mapping_and_changed_sources_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let mut p = Project::create(&root).unwrap();
    let source = temp.path().join("source.mzML");
    fs::copy(fixture(), &source).unwrap();
    let id = p.register(&source).unwrap();
    p.commit(&root, 0).unwrap();
    fs::write(&source, "changed acquisition").unwrap();
    assert!(p.verified_source(id).is_err());
    assert_eq!(
        delivery::verification(&p)[0]["state"],
        "unavailable_or_changed"
    );
    fs::write(
        root.join("source-map.json"),
        serde_json::to_vec(&json!({source.to_string_lossy().to_string():"../source.mzML"}))
            .unwrap(),
    )
    .unwrap();
    assert!(Project::open(&root).is_err());
}

#[test]
fn scatter_export_retains_signed_duplicate_coordinates_without_connecting_samples() {
    let svg = delivery::scatter_svg(
        &[[2., 1.], [-1., 3.], [2., -4.]],
        "PC1 score",
        "PC2 score",
        &Config::default(),
    )
    .unwrap();
    assert_eq!(svg.matches("<circle").count(), 3);
    assert!(!svg.contains("polyline"));
    assert!(svg.contains("<title>-1, 3</title>"));
    assert!(delivery::scatter_svg(&[[f64::NAN, 1.]], "x", "y", &Config::default()).is_err());
}

#[test]
fn multi_segment_export_preserves_gaps_and_branch_identity() {
    let svg = delivery::series_svg(
        &[
            ("raw <source>".into(), vec![[0., 0.], [1., 2.]]),
            ("corrected".into(), vec![[3., 1.], [4., 0.]]),
        ],
        "RT (min)",
        "Intensity",
        &Config::default(),
    )
    .unwrap();
    assert_eq!(svg.matches("<polyline").count(), 2);
    assert!(!svg.contains("<circle"));
    assert!(svg.contains("raw &lt;source&gt;"));
}
