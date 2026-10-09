//! Reference calculations and cross-interface contracts use the existing redistributable mzML.
use chromascope::{
    domain::*,
    engine::{self, Output},
    jobs::{JobControl, Scheduler},
    parser::MzData,
    plotting_parameters::PlotType,
    processing::{self, ProcessingParams},
    project::Project,
    validation::{DataBounds, XicParams},
};
use mzdata::spectrum::ScanPolarity;
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("test_file/data_dependent_02.mzML")
}
fn request(operation: Operation) -> Request {
    Request {
        version: 1,
        operation_id: OperationId::default(),
        actor: "reference-test".into(),
        operation,
    }
}
fn params(kind: PlotType) -> ProcessingParams {
    ProcessingParams {
        acquisition: None,
        plot_type: kind,
        ms_level: 1,
        polarity: ScanPolarity::Positive,
        smoothing: 0,
        xic_params: if kind == PlotType::Xic {
            Some(
                XicParams::new(
                    524.3,
                    ScanPolarity::Positive,
                    10.0,
                    &DataBounds::unrestricted(),
                )
                .unwrap(),
            )
        } else {
            None
        },
        mz_range: None,
        precursor_mz: None,
    }
}
#[test]
fn engine_legacy_and_cli_numerical_equivalence() {
    for kind in [PlotType::Tic, PlotType::Bpc, PlotType::Xic] {
        let p = params(kind);
        let mut reader = MzData::new();
        reader.open_msfile(&fixture()).unwrap();
        let (expected, raw) = processing::process_chromatogram(&mut reader, &p).unwrap();
        let req = request(Operation::Extract { params: p });
        let response = engine::execute(&fixture(), req.clone(), &JobControl::default()).unwrap();
        if let Output::Chromatogram {
            points,
            raw: actual,
            ..
        } = response.output
        {
            assert_eq!(points, expected);
            assert_eq!(actual.index, raw.index);
            assert_eq!(actual.intensity, raw.intensity);
        } else {
            panic!("Wrong output");
        }
        let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
            .args(["run", fixture().to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&req).unwrap())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let actual: Vec<[f64; 2]> =
            serde_json::from_value(value["result"]["output"]["points"].clone()).unwrap();
        assert_eq!(actual, expected);
    }
}
#[test]
fn independent_chord_area_and_invalid_inputs() {
    // Sloping chord y=1+t under triangular excess: base 2 minutes, height 4 => area 4.
    let points = vec![[0., 1.], [1., 6.], [2., 3.]];
    let response = engine::execute(
        Path::new("unused"),
        request(Operation::Integrate {
            points: points.clone(),
            start: Minutes(0.),
            end: Minutes(2.),
        }),
        &JobControl::default(),
    )
    .unwrap();
    match response.output {
        Output::Integration { area, .. } => assert!((area.0 - 4.).abs() < 1e-6),
        _ => panic!("Wrong output"),
    }
    for invalid in [vec![[1., 1.], [0., 2.]], vec![[0., f64::NAN]]] {
        assert!(engine::validate_points(&invalid).is_err());
    }
    assert!(engine::execute(
        Path::new("unused"),
        request(Operation::Integrate {
            points,
            start: Minutes(-1.),
            end: Minutes(2.)
        }),
        &JobControl::default()
    )
    .is_err());
    assert!(DataBounds::unrestricted().validate_mass(f64::NAN).is_err());
    assert!(serde_json::from_str::<XicParams>(
        r#"{"mass":-1,"mass_tolerance":10,"polarity":"positive"}"#
    )
    .is_err());
    let mut large = DataBounds::unrestricted();
    large.scan_count = 25600;
    assert!(large.validate_smoothing(10).is_ok());
    let mut p = params(PlotType::Tic);
    p.ms_level = 0;
    assert_eq!(
        engine::execute(
            &fixture(),
            request(Operation::Extract { params: p }),
            &JobControl::default()
        )
        .err()
        .unwrap()
        .code,
        "invalid_parameters"
    );
}
#[test]
fn immutable_project_roundtrip_stale_commit_corruption_and_relink() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let mut p = Project::create(&root).unwrap();
    let id = p.register(&fixture()).unwrap();
    let response = engine::execute(
        &fixture(),
        request(Operation::Metadata),
        &JobControl::default(),
    )
    .unwrap();
    let result_id = response.result_id;
    p.add_result(&root, id, &response).unwrap();
    p.commit(&root, 0).unwrap();
    let mut reopened = Project::open(&root).unwrap();
    assert_eq!(reopened.id, p.id);
    assert_eq!(reopened.sources[0].id, id);
    assert_eq!(
        reopened
            .load_result(&root, result_id)
            .unwrap()
            .request
            .operation_id,
        response.request.operation_id
    );
    let copy = temp.path().join("moved.mzML");
    std::fs::copy(fixture(), &copy).unwrap();
    reopened.relink(id, &copy).unwrap();
    reopened.commit(&root, 1).unwrap();
    assert_eq!(p.commit(&root, 1).err().unwrap().code, "stale_revision");
    assert!(root.join("revision-00000000000000000001.json").exists());
    std::fs::write(&copy, b"replacement").unwrap();
    assert!(reopened.verified_source(id).is_err());
    let artifact = root
        .join("results")
        .join(format!("{}.json", reopened.results[0].sha256));
    std::fs::write(artifact, b"corrupt").unwrap();
    assert!(Project::open(&root).is_err());
}
#[test]
fn scheduler_cancellation_and_limits() {
    let scheduler = Scheduler::new(1, 2).unwrap();
    let control = JobControl::default();
    control.cancel();
    let job = scheduler
        .submit(fixture(), request(Operation::Metadata), control)
        .unwrap();
    assert_eq!(job.wait().err().unwrap().code, "cancelled");
    let control = JobControl::with_limits(2_000_000, 1).unwrap();
    assert_eq!(
        engine::execute(&fixture(), request(Operation::Metadata), &control)
            .err()
            .unwrap()
            .code,
        "resource_limit"
    );
    assert!(Scheduler::new(0, 1).is_err());
}
#[test]
fn missing_source_and_future_schema_fail_explicitly() {
    assert_eq!(
        engine::execute(
            Path::new("does-not-exist.mzML"),
            request(Operation::Metadata),
            &JobControl::default()
        )
        .err()
        .unwrap()
        .code,
        "missing_source"
    );
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("p");
    let p = Project::create(&root).unwrap();
    let mut value = serde_json::to_value(p).unwrap();
    value["schema_version"] = serde_json::json!(99);
    std::fs::write(
        root.join("revision-00000000000000000000.json"),
        serde_json::to_vec(&value).unwrap(),
    )
    .unwrap();
    assert_eq!(
        Project::open(&root).err().unwrap().code,
        "unsupported_capability"
    );
}

#[test]
fn method_migration_preserves_source_and_stable_identity() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("p");
    let mut p = Project::create(&root).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("presets/quantification-example.toml");
    let before = std::fs::read(&source).unwrap();
    let id = p.import_legacy_method(&source).unwrap();
    assert_eq!(p.import_legacy_method(&source).unwrap(), id);
    assert_eq!(p.methods.len(), 1);
    p.commit(&root, 0).unwrap();
    assert_eq!(Project::open(&root).unwrap().methods[0].id, id);
    assert_eq!(std::fs::read(&source).unwrap(), before);
    assert_eq!(
        Project::create(&root).err().unwrap().code,
        "corrupt_project"
    );
}
#[test]
fn quantification_uses_shared_unsmoothed_peak_kernel() {
    let mut method = chromascope::quant::Method {
        detection_smoothing: 0,
        ..Default::default()
    };
    let a = &mut method.analytes[0];
    a.extraction.mass = Some(524.3);
    a.expected_rt = 10.95;
    a.rt_window = [10., 12.];
    let params = chromascope::quant::validate(&method).unwrap();
    let mut reader = MzData::new();
    reader.open_msfile(&fixture()).unwrap();
    let (trace, _) = processing::process_chromatogram(&mut reader, &params[0]).unwrap();
    let (peak, status) = chromascope::quant::detect(&trace, &method.analytes[0], &method);
    let response = engine::execute(
        &fixture(),
        request(Operation::Quantify { method }),
        &JobControl::default(),
    )
    .unwrap();
    match response.output {
        Output::Quantification { measurements, .. } => {
            assert_eq!(measurements[0].peak, peak);
            assert_eq!(measurements[0].status, status);
            assert_eq!(measurements[0].trace, trace);
        }
        _ => panic!("Wrong output"),
    }
}
#[cfg(feature = "mcp-headless")]
#[test]
fn headless_mcp_stdio_matches_engine_and_enforces_roots() {
    use std::io::{BufRead, BufReader};
    let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-engine-mcp"))
        .args([
            "--allow-root",
            fixture().parent().unwrap().to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    fn send(writer: &mut impl Write, value: serde_json::Value) {
        writeln!(writer, "{value}").unwrap();
        writer.flush().unwrap();
    }
    fn read(reader: &mut impl BufRead) -> serde_json::Value {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"engine-test","version":"1"}}}),
    );
    assert!(read(&mut stdout).get("result").is_some());
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    let list = read(&mut stdout);
    assert_eq!(list["result"]["tools"][0]["name"], "analytical_operation");
    let req = request(Operation::Extract {
        params: params(PlotType::Xic),
    });
    let expected = engine::execute(&fixture(), req.clone(), &JobControl::default()).unwrap();
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":fixture(),"request":req}}}),
    );
    let response = read(&mut stdout);
    assert_ne!(response["result"]["isError"], serde_json::json!(true));
    // Deserialize the declared DTO before comparing: raw arrays are f32 while
    // JSON Values use f64. Compare exact source f32 and processed f64 values.
    let actual: Output =
        serde_json::from_value(response["result"]["structuredContent"]["output"].clone()).unwrap();
    match (actual, expected.output) {
        (
            Output::Chromatogram {
                points: a,
                raw: ar,
                retention_time_unit: au,
                intensity_unit: ai,
                state: ast,
            },
            Output::Chromatogram {
                points: b,
                raw: br,
                retention_time_unit: bu,
                intensity_unit: bi,
                state: bst,
            },
        ) => {
            assert_eq!(a, b);
            assert_eq!(ar.retention_time, br.retention_time);
            assert_eq!(ar.intensity, br.intensity);
            assert_eq!(ar.index, br.index);
            assert_eq!(ar.mz, br.mz);
            assert_eq!((au, ai, ast), (bu, bi, bst));
        }
        _ => panic!("Wrong output"),
    }
    let req = request(Operation::ProcessChromatograms {
        traces: vec![chromascope::chromatography::Trace::from_points(
            "stdio".into(),
            &[[0.0, 0.0], [1.0, 10.0], [2.0, 0.0]],
        )],
        config: chromascope::chromatography::Config {
            smoothing: None,
            baseline: chromascope::chromatography::Baseline::None,
            minimum_snr: 0.0,
            ..Default::default()
        },
    });
    let expected = engine::execute(&fixture(), req.clone(), &JobControl::default()).unwrap();
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":fixture(),"request":req}}}),
    );
    let response = read(&mut stdout);
    assert_ne!(response["result"]["isError"], serde_json::json!(true));
    let actual: Output =
        serde_json::from_value(response["result"]["structuredContent"]["output"].clone()).unwrap();
    let (
        Output::ChromatographicProcessing { analyses: a, .. },
        Output::ChromatographicProcessing { analyses: b, .. },
    ) = (actual, expected.output)
    else {
        panic!("Wrong chromatographic output");
    };
    assert_eq!(a, b);
    let analysis = a[0].clone();
    let req = request(Operation::ReviseChromatogram {
        analysis: Box::new(analysis),
        expected_revision: 0,
        correction: chromascope::chromatography::Correction::Manual {
            intervals: vec![[0.0, 2.0]],
            reason: "MCP preview".into(),
        },
        preview: true,
    });
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":fixture(),"request":req}}}),
    );
    let response = read(&mut stdout);
    assert_eq!(
        response["result"]["structuredContent"]["output"]["preview"],
        true
    );
    let outside = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    send(
        &mut stdin,
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":outside,"request":request(Operation::Metadata)}}}),
    );
    let denied = read(&mut stdout);
    assert_eq!(
        denied["result"]["structuredContent"]["code"],
        "unauthorized"
    );
    drop(stdin);
    child.wait().unwrap();
}

#[test]
fn spectrum_metadata_and_cli_project_reopen() {
    let response = engine::execute(
        &fixture(),
        request(Operation::Spectrum { index: 0 }),
        &JobControl::default(),
    )
    .unwrap();
    match response.output {
        Output::Spectrum {
            spectrum,
            metadata,
            retention_time_unit,
            intensity_unit,
            state,
        } => {
            assert_eq!(spectrum.index, 0);
            assert_eq!(metadata["index"], 0);
            assert!(metadata["native_id"]
                .as_str()
                .is_some_and(|s| !s.is_empty()));
            assert_eq!(spectrum.mz.len(), spectrum.intensity.len());
            assert_eq!(retention_time_unit, "minute");
            assert_eq!(intensity_unit, "instrument intensity");
            assert_eq!(state, DataState::Present);
        }
        _ => panic!("Wrong output"),
    }
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let created = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
        .args(["project-create", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(created.status.success());
    let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
        .args(["run", fixture().to_str().unwrap(), root.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request(Operation::Metadata)).unwrap())
        .unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    let inspect = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
        .args(["project-inspect", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(inspect.status.success());
    let value: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(value["result"]["revision"], 1);
    assert_eq!(value["result"]["sources"].as_array().unwrap().len(), 1);
    assert_eq!(value["result"]["results"].as_array().unwrap().len(), 1);
}

#[test]
fn restore_is_a_new_revision_and_keeps_original_results() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("p");
    let mut p = Project::create(&root).unwrap();
    let id = p.register(&fixture()).unwrap();
    p.commit(&root, 0).unwrap();
    let restored = Project::restore_revision(&root, 0, 1).unwrap();
    assert_eq!(restored.revision, 2);
    assert!(restored.sources.is_empty());
    assert_eq!(restored.restored_from_revision, Some(0));
    let restored_again = Project::restore_revision(&root, 1, 2).unwrap();
    assert_eq!(restored_again.revision, 3);
    assert_eq!(restored_again.sources[0].id, id);
    assert!(root.join("revision-00000000000000000001.json").exists());
    assert_eq!(
        Project::restore_revision(&root, 1, 2).err().unwrap().code,
        "stale_revision"
    );
}
