use chromascope::{
    domain::{Operation, Request},
    engine::{self, Output},
    jobs::JobControl,
    spectral::Polarity,
    untargeted::*,
};
use std::{
    collections::BTreeMap,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
fn request(config: Config) -> Request {
    Request {
        version: 1,
        operation_id: Default::default(),
        actor: "untargeted-reference-test".into(),
        operation: Operation::UntargetedBatch { config },
    }
}
fn fixture() -> Report {
    let roles = [Role::Sample, Role::Blank, Role::Qc, Role::Qc, Role::Qc];
    let samples = roles
        .iter()
        .enumerate()
        .map(|(i, role)| Sample {
            id: format!("s{i}"),
            source: format!("s{i}.mzML"),
            role: *role,
            metadata: serde_json::json!({"synthetic":true}),
        })
        .collect::<Vec<_>>();
    let config = Config {
        samples: samples.clone(),
        cache_directory: "cache".into(),
        ..Default::default()
    };
    let cells = [20., 2., 4., 8., 12.]
        .iter()
        .enumerate()
        .map(|(i, a)| Cell {
            sample_id: format!("s{i}"),
            state: MissingState::Detected,
            intensity: Some(*a),
            openms_intensity: Some(*a / 2.),
            raw_rt_seconds: Some(1.),
            aligned_rt_seconds: Some(1.),
            raw_bounds_seconds: Some([0., 2.]),
            mz: Some(100.),
            charge: Some(1),
            isotope_mz: vec![100.],
            adduct_group: None,
            adduct: None,
            eic: vec![[0., 0.], [1., *a], [2., 0.]],
            apex_spectrum: vec![[100., *a]],
            ms2: vec![],
        })
        .collect();
    let alignments = samples
        .iter()
        .map(|s| Alignment {
            sample_id: s.id.clone(),
            reference_sample_id: s.id.clone(),
            state: "reference".into(),
            slope: Some(1.),
            intercept_seconds: Some(0.),
            landmarks_raw_reference_seconds: vec![],
            residuals_seconds: vec![],
            rms_residual_seconds: None,
            reason: None,
        })
        .collect();
    Report {
        version: 1,
        adapter_version: ADAPTER_VERSION.into(),
        openms_version: "3.5.0".into(),
        config,
        source_hashes: samples
            .iter()
            .map(|s| (s.id.clone(), "a".repeat(64)))
            .collect::<BTreeMap<_, _>>(),
        features: vec![Feature {
            id: "known-triangle".into(),
            mz: 100.,
            aligned_rt_seconds: 1.,
            cells,
            flags: vec!["qc_not_reproducible".into()],
            blank_ratio: Some(10.),
            qc_cv: Some(0.5),
            sample_fraction: Some(1.),
            filter_state: FilterState::Excluded,
        }],
        alignments,
        provenance: serde_json::json!({"synthetic":true}),
        warnings: vec![],
    }
}
#[test]
fn validates_units_modes_paths_and_thresholds() {
    let mut c = fixture().config;
    assert!(validate(&c).is_ok());
    c.polarity = Polarity::Negative;
    assert!(validate(&c).is_ok());
    c.polarity = Polarity::Unknown;
    assert!(validate(&c).is_err());
    c.polarity = Polarity::Positive;
    c.detection_ppm = f64::NAN;
    assert!(validate(&c).is_err());
    c.detection_ppm = 10.;
    c.min_trace_sample_rate = 0.;
    assert!(validate(&c).is_err());
    c.min_trace_sample_rate = 0.5;
    c.samples[1].id = c.samples[0].id.clone();
    assert!(validate(&c).is_err());
}
#[test]
fn independently_replays_area_alignment_and_qc_filters() {
    let r = fixture();
    verify(&r).unwrap();
    for variant in 0..5 {
        let mut r = r.clone();
        match variant {
            0 => r.features[0].cells[0].intensity = Some(30.),
            1 => r.features[0].qc_cv = Some(0.3),
            2 => r.features[0].filter_state = FilterState::Included,
            3 => r.alignments[0].slope = Some(2.),
            _ => r.features[0].cells[0].state = MissingState::NotDetected,
        };
        assert!(verify(&r).is_err());
    }
}
#[test]
fn matrix_and_observation_exports_preserve_filtered_evidence() {
    let r = fixture();
    let all = csv(&r).unwrap();
    assert_eq!(all.lines().count(), 2);
    assert!(all.contains("s0:intensity_area_seconds"));
    assert!(all.contains("excluded"));
    assert_eq!(matrix_csv(&r, true).unwrap().lines().count(), 1);
    assert_eq!(observations_csv(&r).unwrap().lines().count(), 6);
    let response = engine::Response {
        version: 1,
        result_id: Default::default(),
        request: request(r.config.clone()),
        kernel_version: ADAPTER_VERSION.into(),
        source_sha256: None,
        started_unix_ms: 0,
        finished_unix_ms: 1,
        output: Output::FeatureMatrix {
            report: Box::new(r),
        },
    };
    let temp = tempfile::tempdir().unwrap();
    for (command, rows) in [
        ("export-features", 1),
        ("export-features-filtered", 0),
        ("export-feature-observations", 5),
    ] {
        let path = temp.path().join(format!("{command}.csv"));
        let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
            .arg(command)
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&response).unwrap())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["result"]["rows"], rows);
        assert_eq!(
            std::fs::read_to_string(path).unwrap().lines().count(),
            rows as usize + 1
        );
    }
}
#[test]
fn cancellation_and_invalid_config_prevent_adapter_execution() {
    let control = JobControl::default();
    control.cancel();
    let c = fixture().config;
    assert_eq!(run(&c, &control).unwrap_err().code, "cancelled");
    let mut c = c;
    c.samples.clear();
    assert_eq!(
        engine::execute(Path::new("-"), request(c), &JobControl::default())
            .err()
            .unwrap()
            .code,
        "invalid_parameters"
    );
}
#[test]
#[ignore = "requires local pyOpenMS 3.5.0; run explicitly for scientific/CLI/cache validation"]
fn openms_multisample_reference_positive_negative_cli_cache_and_project() {
    let temp = tempfile::tempdir().unwrap();
    for negative in [false, true] {
        let root = temp
            .path()
            .join(if negative { "negative" } else { "positive" });
        let python =
            std::env::var_os("CHROMASCOPE_OPENMS_PYTHON").unwrap_or_else(|| "python".into());
        let mut cmd = Command::new(python);
        cmd.arg("tests/reference/generate_untargeted.py").arg(&root);
        if negative {
            cmd.arg("--negative");
        }
        let generated = cmd.output().unwrap();
        assert!(
            generated.status.success(),
            "{}",
            String::from_utf8_lossy(&generated.stderr)
        );
        let req: Request =
            serde_json::from_slice(&std::fs::read(root.join("request.json")).unwrap()).unwrap();
        let control = JobControl::default();
        let response = engine::execute(Path::new("-"), req.clone(), &control).unwrap();
        let report = match &response.output {
            Output::FeatureMatrix { report } => report,
            _ => panic!("Wrong output"),
        };
        verify(report).unwrap();
        assert_eq!(
            report.features.len(),
            8,
            "8 real Gaussian envelopes; intermittent false ions must be absent"
        );
        let truth: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("truth.json")).unwrap()).unwrap();
        for (j, f) in report.features.iter().enumerate() {
            assert!((f.mz - truth[0]["features"][j]["mz"].as_f64().unwrap()).abs() < 1e-5);
            assert!(
                (f.aligned_rt_seconds
                    - truth[0]["features"][j]["reference_rt_seconds"]
                        .as_f64()
                        .unwrap())
                .abs()
                    < 0.5
            );
            for (i, c) in f.cells.iter().enumerate() {
                if c.state == MissingState::Detected && report.config.samples[i].role != Role::Blank
                {
                    // Matrix areas use the declared OpenMS hull window, not the
                    // infinite Gaussian tails. Independently integrate the known
                    // continuous signal at 4000× finer spacing than acquisition.
                    let full_area = truth[i]["features"][j]["area_seconds"].as_f64().unwrap();
                    let center = truth[i]["features"][j]["raw_rt_seconds"].as_f64().unwrap();
                    let [lo, hi] = c.raw_bounds_seconds.unwrap();
                    let step = (hi - lo) / 4000.;
                    let amplitude = full_area / (3. * (2. * std::f64::consts::PI).sqrt());
                    let expected = (0..4000)
                        .map(|k| {
                            let x = lo + (k as f64 + 0.5) * step;
                            amplitude * (-0.5 * ((x - center) / 3.).powi(2)).exp() * step
                        })
                        .sum::<f64>();
                    assert!(
                        (c.intensity.unwrap() / expected - 1.).abs() < 0.002,
                        "Independent Gaussian area, feature {j} sample {i}"
                    );
                    assert!(
                        (c.aligned_rt_seconds.unwrap()
                            - truth[0]["features"][j]["reference_rt_seconds"]
                                .as_f64()
                                .unwrap())
                        .abs()
                            < 0.6
                    );
                    if let Some(charge) = c.charge {
                        assert!(if negative { charge <= 0 } else { charge >= 0 });
                    }
                }
            }
        }
        assert_eq!(
            report.features[5].cells[1].state,
            MissingState::BelowThreshold
        );
        assert!(report.features[5].cells[1].intensity.is_none());
        assert_eq!(report.features[6].cells[1].state, MissingState::GapFilled);
        assert!(report.features[2]
            .flags
            .contains(&"blank_contamination".into()));
        assert!(report.features[3]
            .flags
            .contains(&"qc_not_reproducible".into()));
        assert!(report.features[0].cells[0].isotope_mz.len() >= 2);
        assert!(!report.features[0].cells[0].ms2.is_empty());
        assert_eq!(control.completed_scans(), 12);
        let source = Path::new(&report.config.samples[0].source);
        let project_root = root.join("project");
        let mut p = chromascope::project::Project::create(&project_root).unwrap();
        let mut id = None;
        for s in &report.config.samples {
            let d = p.register(Path::new(&s.source)).unwrap();
            id.get_or_insert(d);
        }
        p.add_result(&project_root, id.unwrap(), &response).unwrap();
        p.commit(&project_root, 0).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
            .args(["run", source.to_str().unwrap()])
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
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let replay: engine::Response = serde_json::from_value(value["result"].clone()).unwrap();
        if let Output::FeatureMatrix { report: cached } = &replay.output {
            assert!(cached.provenance["checkpoints"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v["reused"] == true));
            assert_eq!(csv(report).unwrap(), csv(cached).unwrap());
        } else {
            panic!("Wrong CLI output");
        }
        #[cfg(feature = "mcp-headless")]
        if !negative {
            mcp_reference(&root, &req);
        }
        if !negative {
            let limited = JobControl::with_limits(100, 50_000_000_000).unwrap();
            assert_eq!(
                engine::execute(Path::new("-"), req.clone(), &limited)
                    .err()
                    .unwrap()
                    .code,
                "resource_limit",
                "Warm checkpoints must honor current scan budgets"
            );
            let mut disabled = req.clone();
            if let Operation::UntargetedBatch { config } = &mut disabled.operation {
                config.gap_fill = false;
            }
            let disabled =
                engine::execute(Path::new("-"), disabled, &JobControl::default()).unwrap();
            if let Output::FeatureMatrix { report } = &disabled.output {
                assert_eq!(
                    report.features[5].cells[1].state,
                    MissingState::GapFillDisabled
                );
                assert!(report.features[5].cells[1].intensity.is_none());
            } else {
                panic!();
            }
            let truncated = root.join("truncated.mzML");
            let python =
                std::env::var_os("CHROMASCOPE_OPENMS_PYTHON").unwrap_or_else(|| "python".into());
            let modified=Command::new(python).args(["-c","import sys,pyopenms as m;e=m.MSExperiment();m.MzMLFile().load(sys.argv[1],e);o=m.MSExperiment();[o.addSpectrum(s) for s in e if s.getRT()<=300.];m.MzMLFile().store(sys.argv[2],o)"]).arg(&report.config.samples[1].source).arg(&truncated).output().unwrap();
            assert!(modified.status.success());
            let mut shorter = req.clone();
            if let Operation::UntargetedBatch { config } = &mut shorter.operation {
                config.samples[1].source = truncated.to_string_lossy().into();
            }
            let shorter = engine::execute(Path::new("-"), shorter, &JobControl::default()).unwrap();
            if let Output::FeatureMatrix { report } = &shorter.output {
                assert_eq!(
                    report.features[7].cells[1].state,
                    MissingState::OutsideAcquisition
                );
                assert!(report.features[7].cells[1].intensity.is_none());
            } else {
                panic!();
            }
            let control = JobControl::default();
            let worker_control = control.clone();
            let mut interrupted = req.clone();
            let cancel_cache = root.join("real-child-cancel");
            if let Operation::UntargetedBatch { config } = &mut interrupted.operation {
                config.cache_directory = cancel_cache.to_string_lossy().into();
            }
            let worker = std::thread::spawn(move || {
                engine::execute(Path::new("-"), interrupted, &worker_control)
            });
            let start = std::time::Instant::now();
            loop {
                if cancel_cache.exists()
                    && std::fs::read_dir(&cancel_cache)
                        .unwrap()
                        .filter_map(|e| e.ok())
                        .any(|e| e.path().join("stdout.log").exists())
                {
                    break;
                }
                assert!(start.elapsed().as_secs() < 5);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            let start = std::time::Instant::now();
            control.cancel();
            let error = worker.join().unwrap().err().unwrap();
            assert_eq!(error.code, "cancelled");
            assert!(start.elapsed() < std::time::Duration::from_secs(2));
            let attempt = std::fs::read_dir(&cancel_cache)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            let outcome: serde_json::Value =
                serde_json::from_slice(&std::fs::read(attempt.join("outcome.json")).unwrap())
                    .unwrap();
            assert_eq!(outcome["state"], "cancelled");
        }
        let export = root.join("matrix.csv");
        for success in [true, false] {
            let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
                .args(["export-features", export.to_str().unwrap()])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(&output.stdout)
                .unwrap();
            assert_eq!(child.wait_with_output().unwrap().status.success(), success);
        }
        if !negative {
            let checkpoint = report.provenance["checkpoints"][0]["checkpoint"]
                .as_str()
                .unwrap();
            std::fs::write(Path::new(checkpoint).join("features.json"), b"{}").unwrap();
            assert!(engine::execute(Path::new("-"), req, &JobControl::default())
                .err()
                .unwrap()
                .message
                .contains("corrupt_cache"));
        }
    }
}
#[cfg(feature = "mcp-headless")]
fn mcp_reference(root: &Path, req: &Request) {
    use std::io::{BufRead, BufReader};
    let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-engine-mcp"))
        .arg("--allow-root")
        .arg(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    fn send(w: &mut impl Write, value: serde_json::Value) {
        writeln!(w, "{value}").unwrap();
        w.flush().unwrap();
    }
    fn read(r: &mut impl BufRead) -> serde_json::Value {
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"untargeted-reference","version":"1"}}}),
    );
    assert!(read(&mut output).get("result").is_some());
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    let path = match &req.operation {
        Operation::UntargetedBatch { config } => &config.samples[0].source,
        _ => unreachable!(),
    };
    let mut serial = 2;
    let mut call = |name: &str, args: serde_json::Value| {
        serial += 1;
        send(
            &mut input,
            serde_json::json!({"jsonrpc":"2.0","id":serial,"method":"tools/call","params":{"name":name,"arguments":args}}),
        );
        let v = read(&mut output);
        assert_ne!(v["result"]["isError"], true, "{v}");
        v["result"]["structuredContent"].clone()
    };
    let job = call(
        "start_untargeted",
        serde_json::json!({"path":path,"request":req}),
    );
    let id = job["job_id"].as_str().unwrap();
    let start = std::time::Instant::now();
    loop {
        let v = call("untargeted_status", serde_json::json!({"job_id":id}));
        if v["state"] == "succeeded" {
            assert_eq!(v["summary"]["features"], 8);
            break;
        }
        assert!(start.elapsed().as_secs() < 20, "{v}");
        assert_ne!(v["state"], "failed", "{v}");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let page = call(
        "untargeted_features",
        serde_json::json!({"job_id":id,"offset":0,"limit":2,"include_evidence":true}),
    );
    assert_eq!(page["total"], 8);
    assert_eq!(page["features"].as_array().unwrap().len(), 2);
    assert!(!page["features"][0]["cells"][0]["eic"]
        .as_array()
        .unwrap()
        .is_empty());
    let summary = call(
        "untargeted_features",
        serde_json::json!({"job_id":id,"offset":0,"limit":1,"include_evidence":false}),
    );
    assert!(summary["features"][0]["cells"][0]["eic"]
        .as_array()
        .unwrap()
        .is_empty());
    let mut canceled = req.clone();
    if let Operation::UntargetedBatch { config } = &mut canceled.operation {
        config.cache_directory = root.join("cancel-cache").to_string_lossy().into();
    }
    let job = call(
        "start_untargeted",
        serde_json::json!({"path":path,"request":canceled}),
    );
    let cancel_id = job["job_id"].as_str().unwrap();
    call("cancel_untargeted", serde_json::json!({"job_id":cancel_id}));
    let start = std::time::Instant::now();
    loop {
        let v = call("untargeted_status", serde_json::json!({"job_id":cancel_id}));
        if v["state"] == "cancelled" {
            assert_eq!(v["summary"]["error"]["code"], "cancelled");
            break;
        }
        assert!(start.elapsed().as_secs() < 3, "{v}");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    drop(call);
    let outside = tempfile::tempdir().unwrap();
    let illegal = outside.path().join("forbidden.mzML");
    std::fs::write(&illegal, b"test-only unauthorized source").unwrap();
    let mut rejected = req.clone();
    if let Operation::UntargetedBatch { config } = &mut rejected.operation {
        config.samples[1].source = illegal.to_string_lossy().into();
    }
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":999,"method":"tools/call","params":{"name":"start_untargeted","arguments":{"path":path,"request":rejected}}}),
    );
    let v = read(&mut output);
    assert_eq!(v["result"]["isError"], true);
    assert_eq!(v["result"]["structuredContent"]["code"], "unauthorized");
    drop(input);
    child.kill().unwrap();
    child.wait().unwrap();
}
