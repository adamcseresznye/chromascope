use chromascope::{
    domain::{Operation, Request},
    engine::{self, Output},
    jobs::JobControl,
    quant,
    targeted::*,
};
use std::{
    collections::BTreeMap,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
fn config() -> CalibrationConfig {
    CalibrationConfig {
        degree: 1,
        intercept: Intercept::Free,
        weighting: Weighting::Unweighted,
        unit: Unit::NgMl,
        range: [0.5, 10.0],
        lod: 1.0,
        loq: 2.0,
        accuracy_tolerance_percent: 15.0,
        qc_cv_limit_percent: 15.0,
        blank_response_limit: 0.5,
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9 * (1.0 + b.abs()), "{a} vs {b}");
}
fn ion(name: &str) -> Ion {
    let mut a = quant::new_analyte(1);
    a.extraction.name = name.into();
    a.expected_rt = 1.0;
    a.rt_window = [0.0, 2.0];
    Ion { extraction: a }
}
fn batch() -> BatchRequest {
    let target = Target {
        id: "a".into(),
        quantifier: ion("quantifier"),
        qualifiers: vec![Qualifier {
            ion: ion("qualifier"),
            ratio_range: [0.4, 0.6],
            rt_tolerance_minutes: 0.1,
        }],
        internal_standard: Some("is".into()),
        is_area_range: None,
        calibration: Some(config()),
    };
    let is = Target {
        id: "is".into(),
        quantifier: ion("IS"),
        qualifiers: vec![],
        internal_standard: None,
        is_area_range: Some([50.0, 150.0]),
        calibration: None,
    };
    let mut samples = Vec::new();
    for (i, x) in [0.5, 1.0, 2.0, 5.0, 10.0].iter().enumerate() {
        samples.push(Sample {
            id: format!("std{i}"),
            source: "synthetic evidence".into(),
            role: Role::Standard,
            injection_order: i as u32,
            dilution: 1.0,
            nominal: BTreeMap::from([("a".into(), *x)]),
            exclusions: Default::default(),
        });
    }
    for (i, role) in [
        Role::Unknown,
        Role::Unknown,
        Role::Unknown,
        Role::Unknown,
        Role::Unknown,
        Role::Qc,
        Role::Qc,
        Role::Blank,
        Role::Unknown,
        Role::Unknown,
        Role::Unknown,
        Role::Unknown,
    ]
    .iter()
    .enumerate()
    {
        samples.push(Sample {
            id: format!("sample{i}"),
            source: "synthetic evidence".into(),
            role: role.clone(),
            injection_order: 5 + i as u32,
            dilution: if i == 0 { 3.0 } else { 1.0 },
            nominal: if *role == Role::Qc {
                BTreeMap::from([("a".into(), 4.0)])
            } else {
                Default::default()
            },
            exclusions: Default::default(),
        });
    }
    BatchRequest {
        version: 1,
        name: "reference".into(),
        targets: vec![target, is],
        samples,
        detection_smoothing: 0,
        minimum_height: 0.0,
        boundary_fraction: 0.05,
    }
}
fn measurement(area: Option<f64>, ion: &Ion) -> quant::Measurement {
    let method = quant::Method {
        analytes: vec![ion.extraction.clone()],
        ..Default::default()
    };
    let params = quant::validate(&method).unwrap().remove(0);
    let trace = vec![[0.0, 0.0], [1.0, area.unwrap_or(0.0)], [2.0, 0.0]];
    let peak = area.map(|_| quant::measure(&trace, 0.0, 2.0).unwrap());
    let status = if area.is_some() {
        quant::Status::Manual
    } else {
        quant::Status::Missing
    };
    quant::Measurement {
        chromatography: None,
        sample: "reference".into(),
        source: "synthetic".into(),
        run: "synthetic".into(),
        analyte: ion.extraction.extraction.name.clone(),
        params,
        method: serde_json::to_string(&method).unwrap(),
        trace,
        automatic: peak.clone(),
        peak,
        automatic_status: status.clone(),
        status,
        diagnostic: String::new(),
    }
}
fn evidence(request: &BatchRequest) -> Vec<Observation> {
    let values = [
        Some(3.0),
        Some(0.7),
        Some(1.5),
        Some(0.1),
        Some(11.0),
        Some(4.0),
        Some(4.2),
        Some(1.0),
        None,
        Some(3.0),
        Some(3.0),
        Some(3.0),
    ];
    let mut observations = Vec::new();
    for (i, s) in request.samples.iter().enumerate() {
        let x = if i < 5 {
            Some(s.nominal["a"])
        } else {
            values[i - 5]
        };
        for t in &request.targets {
            let area = if t.id == "is" {
                if i == 14 {
                    None
                } else {
                    Some(100.0)
                }
            } else {
                x.map(|v| v * 100.0)
            };
            let mut quantifier = measurement(area, &t.quantifier);
            if i == 15 && t.id == "a" {
                quantifier.status = quant::Status::Failed;
            }
            let qualifiers = t
                .qualifiers
                .iter()
                .map(|q| measurement(area.map(|v| v * if i == 16 { 0.8 } else { 0.5 }), &q.ion))
                .collect();
            observations.push(Observation {
                sample: s.id.clone(),
                target: t.id.clone(),
                quantifier,
                qualifiers,
            });
        }
    }
    observations
}
fn evaluated() -> BatchResult {
    let r = batch();
    evaluate(r.clone(), evidence(&r), Default::default()).unwrap()
}
#[test]
fn independent_numpy_regressions_all_models_weights_intercepts() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("reference/targeted.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let cfg: CalibrationConfig = serde_json::from_value(case["config"].clone()).unwrap();
        let xs = case["x"].as_array().unwrap();
        let ys = case["y"].as_array().unwrap();
        let points = xs
            .iter()
            .zip(ys)
            .enumerate()
            .map(|(i, (x, y))| (i.to_string(), x.as_f64().unwrap(), y.as_f64().unwrap()))
            .collect::<Vec<_>>();
        let fit = fit(&points, &cfg).unwrap();
        for (i, c) in fit.coefficients.iter().enumerate() {
            close(
                c / fit.scale.powi(i as i32),
                case["coefficients"][i].as_f64().unwrap(),
            );
        }
        close(fit.r_squared.unwrap(), case["r_squared"].as_f64().unwrap());
        close(fit.weighted_rmse, case["weighted_rmse"].as_f64().unwrap());
        close(
            fit.residual_standard_error.unwrap(),
            case["residual_standard_error"].as_f64().unwrap(),
        );
        for (i, p) in fit.points.iter().enumerate() {
            close(p.residual, case["residuals"][i].as_f64().unwrap());
            match (p.back_calculated, case["back_calculated"][i].as_f64()) {
                (Some(a), Some(b)) => close(a, b),
                (None, None) => {}
                other => panic!("Inverse mismatch {other:?}"),
            }
            match (p.accuracy_percent, case["accuracy_percent"][i].as_f64()) {
                (Some(a), Some(b)) => close(a, b),
                (None, None) => {}
                other => panic!("Accuracy mismatch {other:?}"),
            }
        }
        close(
            fit.inverse(case["query"].as_f64().unwrap(), cfg.range)
                .unwrap(),
            case["roots"][0].as_f64().unwrap(),
        );
    }
}
#[test]
fn inverse_roots_never_extrapolate_or_choose_ambiguous() {
    let cfg = CalibrationConfig {
        degree: 2,
        range: [0.0, 4.0],
        lod: 0.0,
        loq: 0.0,
        ..config()
    };
    let points = (0..=8)
        .map(|i| {
            let x = i as f64 / 2.0;
            (i.to_string(), x, (x - 2.0).powi(2))
        })
        .collect::<Vec<_>>();
    let m = fit(&points, &cfg).unwrap();
    assert_eq!(
        m.inverse(1.0, cfg.range).unwrap_err().code,
        "ambiguous_root"
    );
    assert_eq!(
        m.inverse(5.0, cfg.range).unwrap_err().code,
        "outside_calibration_range"
    );
    assert!(m.flags.contains(&"nonmonotonic_calibration".into()));
    let cfg = CalibrationConfig {
        degree: 3,
        range: [0.0, 4.0],
        lod: 0.0,
        loq: 0.0,
        ..config()
    };
    let points = (0..=16)
        .map(|i| {
            let x = i as f64 / 4.0;
            (i.to_string(), x, (x - 1.0) * (x - 2.0) * (x - 3.0))
        })
        .collect::<Vec<_>>();
    let m = fit(&points, &cfg).unwrap();
    assert_eq!(
        m.inverse(0.0, cfg.range).unwrap_err().code,
        "ambiguous_root"
    );
    assert!(m.flags.contains(&"cubic_requires_review".into()));
}
#[test]
fn responses_dilution_censoring_qc_and_is_flags() {
    let b = evaluated();
    let row = |id: &str| b.results.iter().find(|r| r.sample == id).unwrap();
    close(row("sample0").concentration.unwrap(), 9.0);
    close(row("sample0").response.unwrap(), 3.0);
    assert_eq!(row("sample1").state, State::BelowDetection);
    assert_eq!(row("sample2").state, State::BelowLoq);
    assert_eq!(row("sample3").state, State::BelowRange);
    assert_eq!(row("sample4").state, State::AboveRange);
    assert_eq!(row("sample8").state, State::Missing);
    assert_eq!(row("sample9").state, State::Missing);
    assert!(row("sample9")
        .flags
        .contains(&"internal_standard_missing_or_invalid".into()));
    assert_eq!(row("sample10").state, State::Failed);
    assert!(row("sample11").flags.contains(&"qualifier_ratio".into()));
    assert!(row("sample7").flags.contains(&"blank_contamination".into()));
    assert!(row("sample7").concentration.is_none());
    assert!(row("sample1").concentration.is_none());
    let qc = b.precision.iter().find(|p| p.role == Role::Qc).unwrap();
    close(qc.mean.unwrap(), 4.1);
    close(qc.sd.unwrap(), 0.2 / 2.0_f64.sqrt());
    close(qc.cv_percent.unwrap(), 100.0 * 0.2 / 2.0_f64.sqrt() / 4.1);
    verify(&b).unwrap();
}
#[test]
fn review_history_reject_restore_stale_corruption_and_lossless_roundtrip() {
    let b = evaluated();
    let original = b.observations[0].quantifier.trace.clone();
    let accepted = review(
        &b,
        0,
        "sample0",
        "a",
        true,
        "scientist",
        "Checked ions and standards",
    )
    .unwrap();
    assert!(
        accepted
            .results
            .iter()
            .find(|r| r.sample == "sample0")
            .unwrap()
            .reviewed
    );
    let rejected = review(
        &accepted,
        1,
        "sample0",
        "a",
        false,
        "scientist",
        "Reconsidered",
    )
    .unwrap();
    assert_eq!(
        rejected
            .results
            .iter()
            .find(|r| r.sample == "sample0")
            .unwrap()
            .state,
        State::Rejected
    );
    let restored = review(&rejected, 2, "sample0", "a", true, "scientist", "Restored").unwrap();
    assert_eq!(restored.reviews.len(), 3);
    assert_eq!(restored.observations[0].quantifier.trace, original);
    assert_eq!(
        review(&restored, 0, "sample0", "a", false, "scientist", "stale")
            .err()
            .unwrap()
            .code,
        "stale_revision"
    );
    assert!(review(&restored, 3, "sample1", "a", true, "scientist", "censored").is_err());
    let roundtrip: BatchResult =
        serde_json::from_slice(&serde_json::to_vec(&restored).unwrap()).unwrap();
    verify(&roundtrip).unwrap();
    let mut bad = roundtrip;
    bad.results[0].concentration = Some(999.0);
    assert_eq!(verify(&bad).unwrap_err().code, "corrupt_result");
    let mut bad = b.clone();
    bad.observations[0].quantifier.trace[1][1] += 1.0;
    assert_eq!(verify(&bad).unwrap_err().code, "corrupt_result");
    assert!(csv(&rejected).unwrap().contains("Rejected"));
}
#[test]
fn invalid_designs_weights_levels_and_missing_standards_fail() {
    let mut cfg = config();
    cfg.weighting = Weighting::InverseX;
    assert!(fit(
        &[
            ("a".into(), 0.0, 0.0),
            ("b".into(), 1.0, 1.0),
            ("c".into(), 2.0, 2.0)
        ],
        &cfg
    )
    .is_err());
    assert!(fit(&vec![("a".into(), 1.0, 1.0); 3], &config()).is_err());
    let mut request = batch();
    request.targets[0].internal_standard = Some("a".into());
    assert!(validate(&request).is_err());
    let mut request = batch();
    request.samples[0].dilution = 0.0;
    assert!(validate(&request).is_err());
    let mut request = batch();
    request.samples[0].nominal.clear();
    assert!(validate(&request).is_err());
    let r = batch();
    let mut o = evidence(&r);
    o[0].quantifier.status = quant::Status::Missing;
    let b = evaluate(r, o, Default::default()).unwrap();
    assert_eq!(b.calibration_errors["a"].code, "invalid_standard");
    assert!(b
        .results
        .iter()
        .filter(|r| r.sample == "sample0")
        .all(|r| r.state == State::Failed));
}
fn req(op: Operation) -> Request {
    Request {
        version: 1,
        operation_id: Default::default(),
        actor: "integration-test".into(),
        operation: op,
    }
}
#[test]
fn shared_engine_cli_review_export_no_overwrite() {
    let b = evaluated();
    let operation = Operation::ReviewTargeted {
        batch: Box::new(b),
        expected_revision: 0,
        sample: "sample0".into(),
        target: "a".into(),
        accepted: true,
        reason: "Independent reference".into(),
    };
    let request = req(operation);
    let direct = engine::execute(Path::new("-"), request.clone(), &JobControl::default()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
        .args(["run", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let response: engine::Response = serde_json::from_value(value["result"].clone()).unwrap();
    let Output::TargetedQuantification { batch } = response.output else {
        panic!()
    };
    let Output::TargetedQuantification { batch: expected } = direct.output else {
        panic!()
    };
    assert_eq!(batch.results, expected.results);
    assert_eq!(batch.reviews[0].reason, "Independent reference");
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("out.csv");
    for success in [true, false] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
            .args(["export-targeted", file.to_str().unwrap()])
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
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        csv(&batch).unwrap()
    );
}

include!("fixtures/targeted_support.rs");
#[test]
fn raw_mzml_to_reviewed_concentrations_via_engine_and_cli() {
    let temp = tempfile::tempdir().unwrap();
    let batch_request = raw_request(temp.path());
    let primary = batch_request.samples[0].source.clone();
    let before = std::fs::read(&primary).unwrap();
    let operation = req(Operation::TargetedBatch {
        batch: batch_request,
    });
    let direct = engine::execute(
        Path::new(&primary),
        operation.clone(),
        &JobControl::default(),
    )
    .unwrap();
    let Output::TargetedQuantification { batch: expected } = direct.output else {
        panic!()
    };
    assert!(expected.calibration_errors.is_empty());
    let unknown = expected
        .results
        .iter()
        .find(|r| r.sample == "raw6")
        .unwrap();
    assert_eq!(unknown.state, State::Present);
    close(unknown.concentration.unwrap(), 10.0);
    close(unknown.response.unwrap(), 0.5);
    close(unknown.qualifier_ratios[0].unwrap(), 0.5);
    assert_eq!(expected.source_hashes.len(), 8);
    close(
        expected.observations[0]
            .quantifier
            .peak
            .as_ref()
            .unwrap()
            .area,
        10.0,
    );
    verify(&expected).unwrap();
    let reviewed = review(
        &expected,
        0,
        "raw6",
        "a",
        true,
        "scientist",
        "Known triangular reference",
    )
    .unwrap();
    verify(&reviewed).unwrap();
    assert_eq!(std::fs::read(primary).unwrap(), before);
    let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
        .args(["run", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&operation).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let response: engine::Response = serde_json::from_value(value["result"].clone()).unwrap();
    let Output::TargetedQuantification { batch: actual } = response.output else {
        panic!()
    };
    assert_eq!(actual.results, expected.results);
    assert_eq!(actual.calibrations, expected.calibrations);
}
#[cfg(feature = "mcp-headless")]
#[test]
fn targeted_mcp_raw_review_export_and_nested_path_policy() {
    use std::io::{BufRead, BufReader};
    let temp = tempfile::tempdir().unwrap();
    let batch_request = raw_request(temp.path());
    let source = batch_request.samples[0].source.clone();
    let direct = chromascope::targeted::run(batch_request.clone(), &JobControl::default()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-engine-mcp"))
        .args(["--allow-root", temp.path().to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut writer = child.stdin.take().unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    fn send(writer: &mut impl Write, value: serde_json::Value) {
        writeln!(writer, "{value}").unwrap();
        writer.flush().unwrap();
    }
    fn read(reader: &mut impl BufRead) -> serde_json::Value {
        let mut s = String::new();
        reader.read_line(&mut s).unwrap();
        serde_json::from_str(&s).unwrap()
    }
    send(
        &mut writer,
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"targeted-test","version":"1"}}}),
    );
    assert!(read(&mut reader).get("result").is_some());
    send(
        &mut writer,
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    send(
        &mut writer,
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":source,"request":req(Operation::TargetedBatch{batch:batch_request.clone()})}}}),
    );
    let value = read(&mut reader);
    assert_ne!(value["result"]["isError"], true, "{value}");
    let response: engine::Response =
        serde_json::from_value(value["result"]["structuredContent"].clone()).unwrap();
    let Output::TargetedQuantification { batch: actual } = response.output else {
        panic!()
    };
    assert_eq!(actual.results, direct.results);
    send(
        &mut writer,
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":source,"request":req(Operation::ReviewTargeted{batch:actual,expected_revision:0,sample:"raw6".into(),target:"a".into(),accepted:true,reason:"Raw reference checked".into()})}}}),
    );
    let value = read(&mut reader);
    assert_ne!(value["result"]["isError"], true, "{value}");
    let response: engine::Response =
        serde_json::from_value(value["result"]["structuredContent"].clone()).unwrap();
    let Output::TargetedQuantification { batch: actual } = response.output else {
        panic!()
    };
    assert!(
        actual
            .results
            .iter()
            .find(|r| r.sample == "raw6")
            .unwrap()
            .reviewed
    );
    let expected_csv = csv(&actual).unwrap();
    send(
        &mut writer,
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":source,"request":req(Operation::ExportTargeted{batch:actual})}}}),
    );
    let value = read(&mut reader);
    assert_eq!(
        value["result"]["structuredContent"]["output"]["csv"],
        expected_csv
    );
    let outside = tempfile::tempdir().unwrap();
    let outside_path = outside.path().join("outside.mzML");
    std::fs::copy(&source, &outside_path).unwrap();
    let mut forbidden = batch_request;
    forbidden.samples[3].source = outside_path.to_str().unwrap().into();
    send(
        &mut writer,
        serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":source,"request":req(Operation::TargetedBatch{batch:forbidden})}}}),
    );
    let value = read(&mut reader);
    assert_eq!(value["result"]["structuredContent"]["code"], "unauthorized");
    drop(writer);
    child.wait().unwrap();
}

#[test]
fn signed_responses_zero_is_and_machine_boundary_flags_are_explicit() {
    let r = batch();
    let mut o = evidence(&r);
    let i = o
        .iter()
        .position(|o| o.sample == "sample0" && o.target == "a")
        .unwrap();
    o[i].quantifier = measurement(Some(-100.0), &r.targets[0].quantifier);
    let b = evaluate(r.clone(), o, Default::default()).unwrap();
    let row = b.results.iter().find(|r| r.sample == "sample0").unwrap();
    assert_eq!(row.response, Some(-1.0));
    assert_eq!(row.state, State::BelowRange);
    assert!(row.concentration.is_none());
    assert!(row.flags.contains(&"nonpositive_analyte_area".into()));
    let mut o = evidence(&r);
    let i = o
        .iter()
        .position(|o| o.sample == "sample0" && o.target == "is")
        .unwrap();
    o[i].quantifier = measurement(Some(0.0), &r.targets[1].quantifier);
    let b = evaluate(r, o, Default::default()).unwrap();
    let row = b.results.iter().find(|r| r.sample == "sample0").unwrap();
    assert_eq!(row.state, State::Missing);
    assert!(row
        .flags
        .contains(&"internal_standard_missing_or_invalid".into()));
    let temp = tempfile::tempdir().unwrap();
    let b = chromascope::targeted::run(raw_request(temp.path()), &JobControl::default()).unwrap();
    let model = &b.calibrations["a"];
    let row = b.results.iter().find(|r| r.sample == "raw0").unwrap();
    assert_eq!(row.state, State::Present);
    close(row.concentration.unwrap(), 1.0);
    if model.response(1.0) != row.response.unwrap() {
        assert!(row.flags.contains(&"boundary_roundoff".into()));
    }
    assert_eq!(
        model
            .inverse(model.response(1.0) - 1e-8, [1.0, 8.0])
            .unwrap_err()
            .code,
        "outside_calibration_range"
    );
}
#[test]
fn rt_qualifiers_is_bounds_exclusions_and_empty_precision_retain_evidence() {
    let mut r = batch();
    r.targets[1].is_area_range = Some([110.0, 150.0]);
    let mut o = evidence(&r);
    let i = o
        .iter()
        .position(|o| o.sample == "sample0" && o.target == "a")
        .unwrap();
    o[i].qualifiers[0].trace = vec![[0.0, 0.0], [1.2, 150.0], [2.0, 0.0]];
    o[i].qualifiers[0].peak = Some(quant::measure(&o[i].qualifiers[0].trace, 0.0, 2.0).unwrap());
    for obs in o
        .iter_mut()
        .filter(|o| o.sample == "sample5" || o.sample == "sample6")
    {
        if obs.target == "a" {
            obs.quantifier.status = quant::Status::Missing;
        }
    }
    let b = evaluate(r.clone(), o, Default::default()).unwrap();
    let row = b.results.iter().find(|r| r.sample == "sample0").unwrap();
    assert!(row.flags.contains(&"qualifier_rt".into()));
    assert!(row.flags.contains(&"internal_standard_area".into()));
    let qc = b.precision.iter().find(|p| p.role == Role::Qc).unwrap();
    assert_eq!(qc.n, 0);
    assert_eq!(qc.sample_count, 2);
    assert!(qc.mean.is_none());
    let mut r = batch();
    r.samples[0]
        .exclusions
        .insert("a".into(), "Documented standard failure".into());
    r.targets[0].calibration.as_mut().unwrap().range[0] = 1.0;
    let mut o = evidence(&r);
    o[0].quantifier.status = quant::Status::Missing;
    let b = evaluate(r, o, Default::default()).unwrap();
    assert!(b.calibration_errors.is_empty());
    assert_eq!(b.calibrations["a"].points.len(), 4);
    assert!(b.results[0]
        .flags
        .iter()
        .any(|f| f.contains("Documented standard failure")));
    verify(&b).unwrap();
}
#[test]
fn batch_sources_and_review_survive_project_reopen_with_checksums() {
    use chromascope::project::Project;
    let temp = tempfile::tempdir().unwrap();
    let batch = raw_request(temp.path());
    let anchor = batch.samples[0].source.clone();
    let root = temp.path().join("project");
    Project::create(&root).unwrap();
    let request = req(Operation::TargetedBatch { batch });
    let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
        .args(["run", "-", root.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let response: engine::Response = serde_json::from_value(value["result"].clone()).unwrap();
    let p = Project::open(&root).unwrap();
    assert_eq!(p.sources.len(), 8);
    assert_eq!(p.results.len(), 1);
    let saved = p.load_result(&root, response.result_id).unwrap();
    let Output::TargetedQuantification { batch } = saved.output else {
        panic!()
    };
    verify(&batch).unwrap();
    let reviewed = engine::execute(
        Path::new(&anchor),
        req(Operation::ReviewTargeted {
            batch,
            expected_revision: 0,
            sample: "raw6".into(),
            target: "a".into(),
            accepted: true,
            reason: "Verified source panel".into(),
        }),
        &JobControl::default(),
    )
    .unwrap();
    let mut p = p;
    let id = p
        .sources
        .iter()
        .find(|s| s.path == std::fs::canonicalize(&anchor).unwrap())
        .unwrap()
        .id;
    p.add_result(&root, id, &reviewed).unwrap();
    p.commit(&root, 1).unwrap();
    let mut p = Project::open(&root).unwrap();
    assert_eq!(p.results.len(), 2);
    assert!(root.join("revision-00000000000000000001.json").exists());
    std::fs::write(&anchor, b"Changed raw acquisition").unwrap();
    assert!(p.load_result(&root, reviewed.result_id).is_ok());
    assert!(p.add_result(&root, id, &reviewed).is_err());
}

#[test]
fn cancelled_missing_source_and_range_extension_fail_structurally() {
    let temp = tempfile::tempdir().unwrap();
    let mut r = raw_request(temp.path());
    let control = JobControl::default();
    control.cancel();
    assert_eq!(
        chromascope::targeted::run(r.clone(), &control)
            .err()
            .unwrap()
            .code,
        "cancelled"
    );
    r.samples[0].source = temp.path().join("absent.mzML").to_str().unwrap().into();
    assert_eq!(
        chromascope::targeted::run(r, &JobControl::default())
            .err()
            .unwrap()
            .code,
        "missing_source"
    );
    let b = evaluated();
    assert!(b.calibrations["a"].inverse(20.0, [0.5, 100.0]).is_err());
}
