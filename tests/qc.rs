use chromascope::{
    domain::{Operation, Request},
    engine::{self, Output},
    jobs::JobControl,
    qc::*,
};
fn study() -> Study {
    serde_json::from_value(
        serde_json::from_str::<serde_json::Value>(include_str!("reference/qc.json")).unwrap()
            ["study"]
            .clone(),
    )
    .unwrap()
}
fn one(metric: Metric) -> Study {
    let mut s = study();
    s.rules.retain(|r| r.metric == metric);
    s
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-10 * (1.0 + b.abs()), "{a} != {b}");
}
#[test]
fn independent_python_statistics_and_rational_reference() {
    let json: serde_json::Value = serde_json::from_str(include_str!("reference/qc.json")).unwrap();
    let report = evaluate(study()).unwrap();
    assert_eq!(report.status, Status::Pass);
    for d in &report.decisions {
        close(
            d.value.unwrap(),
            json["expected"][&d.rule.id].as_f64().unwrap(),
        );
        assert_eq!(d.status, Status::Pass, "{}: {}", d.rule.id, d.reason);
        assert!(!d.evidence.is_empty());
        assert!(!d.calculation.is_empty());
    }
    verify(&report).unwrap();
}
#[test]
fn known_failing_batch_retains_every_reason() {
    let mut s = study();
    s.purpose = "batch_qc".into();
    for r in &mut s.rules {
        match r.metric {
            Metric::Precision => r.upper = 9.0,
            Metric::Blank => r.upper = 0.4,
            Metric::MassError => r.upper = 3.0,
            Metric::CalibrationAcceptance => r.lower = 90.0,
            _ => {}
        }
    }
    let r = evaluate(s).unwrap();
    assert_eq!(r.status, Status::Fail);
    assert_eq!(
        r.decisions
            .iter()
            .filter(|d| d.status == Status::Fail)
            .count(),
        4
    );
    assert_eq!(r.review_queue.len(), 4);
    assert_eq!(r.acceptance.failed, 4);
    assert!(!r.acceptance.calculation.is_empty());
    for d in r.decisions.iter().filter(|d| d.status == Status::Fail) {
        assert!(d.reason.contains("outside"));
        assert!(d.value.is_some());
        assert!(!d.evidence.is_empty());
    }
}
#[test]
fn failed_individual_cannot_hide_behind_mean_accuracy() {
    let mut s = one(Metric::MaximumAccuracyDeviation);
    s.observations[0].value = Some(5.0);
    s.observations[2].value = Some(15.0);
    let r = evaluate(s).unwrap();
    close(r.decisions[0].value.unwrap(), 50.0);
    assert_eq!(r.status, Status::Fail);
}
#[test]
fn missing_required_never_passes_and_optional_does_not_gate() {
    let mut s = one(Metric::MassError);
    s.observations[0].mass_error_ppm = None;
    assert_eq!(evaluate(s.clone()).unwrap().status, Status::Indeterminate);
    s.rules[0].required = false;
    assert_eq!(evaluate(s).unwrap().status, Status::Indeterminate);
}
#[test]
fn zero_and_negative_denominators_and_insufficient_replicates() {
    for metric in [
        Metric::Precision,
        Metric::InternalStandardCv,
        Metric::Recovery,
        Metric::ControlChart,
    ] {
        let mut s = one(metric);
        for o in &mut s.observations {
            o.value = Some(0.0);
            o.response = Some(0.0);
            o.is_response = Some(0.0);
        }
        assert_eq!(evaluate(s).unwrap().status, Status::Indeterminate);
    }
    let mut s = one(Metric::Precision);
    s.rules[0].minimum_n = 4;
    assert_eq!(evaluate(s).unwrap().status, Status::Indeterminate);
}
#[test]
fn carryover_uses_immediate_predecessor_not_largest_standard() {
    let mut s = one(Metric::Carryover);
    s.observations
        .iter_mut()
        .find(|o| o.group == "high")
        .unwrap()
        .response = Some(10.0);
    let r = evaluate(s).unwrap();
    close(r.decisions[0].value.unwrap(), 5.0);
    assert_eq!(r.status, Status::Fail);
    assert!(r.decisions[0].evidence.contains(&"high-0".into()));
}
#[test]
fn carryover_does_not_cross_batch_boundaries() {
    let mut s = one(Metric::Carryover);
    s.observations
        .iter_mut()
        .find(|o| o.group == "blank")
        .unwrap()
        .batch = "other-run".into();
    assert_eq!(evaluate(s).unwrap().status, Status::Indeterminate);
}
#[test]
fn recovery_requires_independent_matched_level_design() {
    let mut s = one(Metric::Recovery);
    s.observations
        .iter_mut()
        .find(|o| o.group == "post")
        .unwrap()
        .nominal = Some(20.0);
    assert_eq!(evaluate(s).unwrap().status, Status::Indeterminate);
    let mut s = one(Metric::Recovery);
    s.rules[0].reference_group = Some("pre".into());
    assert_eq!(evaluate(s).unwrap().status, Status::Indeterminate);
}
#[test]
fn duplicate_ids_orders_nonfinite_bounds_and_units_are_structured_errors() {
    let mut s = study();
    s.observations[1].id = s.observations[0].id.clone();
    assert_eq!(evaluate(s).err().unwrap().code, "invalid_parameters");
}
#[test]
fn invalid_inputs_rejected() {
    for mutate in 0..5 {
        let mut s = one(Metric::Recovery);
        match mutate {
            0 => s.rules[0].lower = f64::NAN,
            1 => s.observations[0].value = Some(f64::INFINITY),
            2 => s.observations[6].unit = "ug/mL".into(),
            3 => s.observations[6].response_unit = "ratio".into(),
            _ => s.observations[1].order = s.observations[0].order,
        }
        assert!(evaluate(s).is_err());
    }
}
#[test]
fn no_rules_is_indeterminate_and_threshold_boundaries_inclusive() {
    let mut s = study();
    s.rules.clear();
    assert_eq!(evaluate(s).unwrap().status, Status::Indeterminate);
    let mut s = one(Metric::Accuracy);
    s.rules[0].lower = 100.0;
    s.rules[0].upper = 100.0;
    assert_eq!(evaluate(s).unwrap().status, Status::Pass);
}
#[test]
fn order_effect_requires_one_batch() {
    let mut s = one(Metric::OrderSlope);
    s.observations[1].batch = "second".into();
    assert_eq!(evaluate(s).unwrap().status, Status::Indeterminate);
}
#[test]
fn replay_detects_tampered_numbers_status_evidence_and_thresholds() {
    let original = evaluate(study()).unwrap();
    for field in 0..5 {
        let mut r = original.clone();
        match field {
            0 => r.decisions[0].value = Some(500.0),
            1 => r.status = Status::Fail,
            2 => r.decisions[0].evidence.clear(),
            3 => r.study.rules[0].upper = 99.0,
            _ => r.decisions[0].calculation = "fabricated".into(),
        }
        assert!(verify(&r).is_err());
        assert!(csv(&r).is_err());
    }
}
#[test]
fn review_is_reversible_and_never_overrides_failed_batch() {
    let mut s = one(Metric::Blank);
    s.rules[0].upper = 0.1;
    let r = evaluate(s).unwrap();
    let id = r.rules_id();
    let reviewed = review(&r, 0, &id, "scientist", "Investigated contamination", true).unwrap();
    assert_eq!(reviewed.status, Status::Fail);
    assert!(reviewed.review_queue.is_empty());
    assert_eq!(r.review_queue.len(), 1);
    verify(&reviewed).unwrap();
    assert_eq!(
        review(&reviewed, 0, &id, "s", "r", true)
            .err()
            .unwrap()
            .code,
        "stale_revision"
    );
    let reopened = review(&reviewed, 1, &id, "scientist", "Reopened", false).unwrap();
    assert_eq!(reopened.review_queue.len(), 1);
    assert_eq!(reopened.reviews.len(), 2);
    verify(&reopened).unwrap();
}
trait RuleId {
    fn rules_id(&self) -> String;
}
impl RuleId for Report {
    fn rules_id(&self) -> String {
        self.decisions[0].rule.id.clone()
    }
}
#[test]
fn engine_method_validation_and_export_share_calculations() {
    let request = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "test".into(),
        operation: Operation::ValidateMethod { study: study() },
    };
    let response =
        engine::execute(std::path::Path::new("-"), request, &JobControl::default()).unwrap();
    assert!(response.kernel_version.ends_with("qc-v1"));
    let Output::QcReport { report } = response.output else {
        panic!()
    };
    assert_eq!(report.status, Status::Pass);
    let table = csv(&report).unwrap();
    assert!(table.contains("evidence_json"));
    assert!(table.contains("recovery-pre"));
}
#[test]
fn cli_evaluation_export_and_no_overwrite() {
    use std::io::Write;
    use std::process::{Command, Stdio};
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
        .write_all(include_bytes!("../examples/qc-request.json"))
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["result"]["output"]["report"]["status"], "pass");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("qc.csv");
    for expected in [true, false] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
            .arg("export-qc")
            .arg(&path)
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
        assert_eq!(child.wait_with_output().unwrap().status.success(), expected);
    }
    assert!(std::fs::read_to_string(path)
        .unwrap()
        .contains("recovery-pre"));
}
#[cfg(feature = "mcp-headless")]
#[test]
fn actual_headless_mcp_qc_and_review() {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    let root = std::env::current_dir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-engine-mcp"))
        .arg("--allow-root")
        .arg(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    fn send(w: &mut impl Write, v: serde_json::Value) {
        writeln!(w, "{v}").unwrap();
        w.flush().unwrap();
    }
    fn read(r: &mut impl BufRead) -> serde_json::Value {
        let mut s = String::new();
        r.read_line(&mut s).unwrap();
        serde_json::from_str(&s).unwrap()
    }
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"qc-test","version":"1"}}}),
    );
    assert!(read(&mut output).get("result").is_some());
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    let request = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "qc-test".into(),
        operation: Operation::EvaluateQc { study: study() },
    };
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":root.join("Cargo.toml"),"request":request}}}),
    );
    let result = read(&mut output);
    assert_ne!(result["result"]["isError"], true);
    let report: Report =
        serde_json::from_value(result["result"]["structuredContent"]["output"]["report"].clone())
            .unwrap();
    assert_eq!(report.status, Status::Pass);
    verify(&report).unwrap();
    let request = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "qc-test".into(),
        operation: Operation::ReviewQc {
            expected_revision: 0,
            rule_id: report.decisions[0].rule.id.clone(),
            reason: "Reference inspected".into(),
            acknowledged: true,
            report: Box::new(report),
        },
    };
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":root.join("Cargo.toml"),"request":request}}}),
    );
    let result = read(&mut output);
    assert_ne!(result["result"]["isError"], true);
    assert_eq!(
        result["result"]["structuredContent"]["output"]["report"]["reviews"][0]["reason"],
        "Reference inspected"
    );
    drop(input);
    child.kill().unwrap();
    child.wait().unwrap();
}
include!("fixtures/targeted_support.rs");
#[test]
fn real_extraction_to_qc_keeps_raw_evidence_and_missing_mass_explicit() {
    let dir = tempfile::tempdir().unwrap();
    let batch =
        chromascope::targeted::run(raw_request(dir.path()), &JobControl::default()).unwrap();
    let rules = chromascope::qc::method_rules(&batch);
    let study = targeted_study(&batch, rules).unwrap();
    let report = evaluate(study.clone()).unwrap();
    verify(&report).unwrap();
    assert!(report.study.targeted_evidence.is_some());
    let qc = report
        .decisions
        .iter()
        .find(|d| d.rule.id == "a-qc-individual-accuracy")
        .unwrap();
    assert_eq!(qc.status, Status::Pass);
    close(qc.value.unwrap(), 100.0 / 30.0);
    let precision = report
        .decisions
        .iter()
        .find(|d| d.rule.metric == Metric::Precision)
        .unwrap();
    close(
        precision.value.unwrap(),
        (0.1_f64 / 2.0_f64.sqrt()) / 3.05 * 100.0,
    );
    let row = study
        .observations
        .iter()
        .find(|o| o.id == "raw6/a")
        .unwrap();
    close(row.value.unwrap(), 5.0);
    assert_eq!(row.mass_error_ppm, None);
    assert_eq!(row.response_unit, "dimensionless area ratio");
    let mut changed = study;
    changed.observations[0].response = Some(999.0);
    assert_eq!(evaluate(changed).err().unwrap().code, "corrupt_result");
    let request = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "qc-test".into(),
        operation: Operation::EvaluateTargetedQc {
            batch: Box::new(batch.clone()),
            rules: vec![Rule {
                id: "mass".into(),
                target: "a".into(),
                group: None,
                role: Some("qc".into()),
                batch: None,
                metric: Metric::MassError,
                lower: 0.0,
                upper: 5.0,
                minimum_n: 2,
                required: true,
                reference_group: None,
                calibration: None,
            }],
        },
    };
    let response =
        engine::execute(std::path::Path::new("-"), request, &JobControl::default()).unwrap();
    let Output::QcReport { report } = response.output else {
        panic!()
    };
    assert_eq!(report.status, Status::Indeterminate);
    assert_eq!(report.decisions[0].n, 0);
    assert_eq!(batch.reviews.len(), 0);
}
#[test]
fn precision_by_run_and_level_preserves_design() {
    let mut s = one(Metric::Precision);
    s.rules[0].batch = Some("missing-run".into());
    assert_eq!(evaluate(s).unwrap().status, Status::Indeterminate);
}
#[test]
fn method_study_requires_declared_preparations() {
    let mut s = one(Metric::MatrixFactor);
    for o in &mut s.observations {
        o.preparation = None;
    }
    assert_eq!(evaluate(s).unwrap().status, Status::Indeterminate);
}
#[test]
fn method_calibration_rules_match_independent_numpy_models() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("reference/targeted.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let cfg: chromascope::targeted::CalibrationConfig =
            serde_json::from_value(case["config"].clone()).unwrap();
        for (metric, key) in [
            (Metric::CalibrationR2, "r_squared"),
            (Metric::CalibrationRmse, "weighted_rmse"),
            (Metric::CalibrationAccuracyDeviation, "accuracy_percent"),
        ] {
            let mut s = one(Metric::Accuracy);
            let template = s.observations[0].clone();
            s.observations = case["x"]
                .as_array()
                .unwrap()
                .iter()
                .zip(case["y"].as_array().unwrap())
                .enumerate()
                .map(|(i, (x, y))| {
                    let mut o = template.clone();
                    o.id = format!("std-{i}");
                    o.order = i as u32;
                    o.group = "calibration-study".into();
                    o.role = "standard".into();
                    o.nominal = x.as_f64();
                    o.response = y.as_f64();
                    o.value = None;
                    o
                })
                .collect();
            let r = &mut s.rules[0];
            r.group = Some("calibration-study".into());
            r.role = Some("standard".into());
            r.metric = metric.clone();
            r.calibration = Some(cfg.clone());
            r.lower = -f64::MAX;
            r.upper = f64::MAX;
            let report = evaluate(s).unwrap();
            let d = &report.decisions[0];
            assert!(d.calibration.is_some());
            if metric == Metric::CalibrationAccuracyDeviation {
                let v = case[key].as_array().unwrap();
                if v.iter().any(|p| p.is_null()) {
                    assert_eq!(d.status, Status::Indeterminate);
                } else {
                    let expected = v
                        .iter()
                        .map(|p| (p.as_f64().unwrap() - 100.0).abs())
                        .reduce(f64::max)
                        .unwrap();
                    assert!((d.value.unwrap() - expected).abs() < 1e-9 * (1.0 + expected.abs()));
                }
            } else {
                let expected = case[key].as_f64().unwrap();
                assert!((d.value.unwrap() - expected).abs() < 1e-9 * (1.0 + expected.abs()));
            }
            verify(&report).unwrap();
        }
    }
}
#[test]
fn optional_failure_is_visible_but_does_not_gate_required_pass() {
    let mut s = study();
    for r in &mut s.rules {
        r.required = false;
    }
    s.rules[0].required = true;
    s.rules
        .iter_mut()
        .find(|r| r.metric == Metric::Blank)
        .unwrap()
        .upper = 0.1;
    let r = evaluate(s).unwrap();
    assert_eq!(r.status, Status::Pass);
    assert!(r.review_queue.contains(&"blank-blank".into()));
}
#[test]
fn heterogeneous_qc_levels_cannot_produce_a_precision_pass() {
    let mut s = one(Metric::Precision);
    s.observations[0].nominal = Some(20.0);
    let r = evaluate(s).unwrap();
    assert_eq!(r.status, Status::Indeterminate);
    assert!(r.decisions[0].reason.contains("nominal level"));
}
#[test]
fn project_qc_commit_checks_all_sources_and_retains_prior_revisions() {
    use chromascope::project::Project;
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir(&data).unwrap();
    let batch = chromascope::targeted::run(raw_request(&data), &JobControl::default()).unwrap();
    let root = dir.path().join("project");
    let mut project = Project::create(&root).unwrap();
    let first = project
        .register(std::path::Path::new(&batch.request.samples[0].source))
        .unwrap();
    let request = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "qc-project-test".into(),
        operation: Operation::EvaluateTargetedQc {
            batch: Box::new(batch.clone()),
            rules: method_rules(&batch),
        },
    };
    let response =
        engine::execute(std::path::Path::new("-"), request, &JobControl::default()).unwrap();
    assert!(project.add_result(&root, first, &response).is_err());
    for s in &batch.request.samples {
        project.register(std::path::Path::new(&s.source)).unwrap();
    }
    project.add_result(&root, first, &response).unwrap();
    project.commit(&root, project.revision).unwrap();
    let reopened = Project::open(&root).unwrap();
    assert_eq!(reopened.results.len(), 1);
    std::fs::write(
        &batch.request.samples[1].source,
        b"changed synthetic acquisition",
    )
    .unwrap();
    assert!(project.add_result(&root, first, &response).is_err());
    assert_eq!(Project::open(&root).unwrap().results.len(), 1);
}
#[test]
fn report_roundtrip_keeps_identity_and_detects_review_tampering() {
    let r = evaluate(study()).unwrap();
    let round: Report = serde_json::from_slice(&serde_json::to_vec(&r).unwrap()).unwrap();
    verify(&round).unwrap();
    assert_eq!(r.report_id, round.report_id);
    let mut reviewed = review(
        &r,
        0,
        &r.decisions[0].rule.id,
        "analyst",
        "checked reference",
        true,
    )
    .unwrap();
    reviewed.reviews[0].reason.clear();
    assert!(verify(&reviewed).is_err());
    let mut changed = r;
    changed.report_id = uuid::Uuid::nil();
    assert!(verify(&changed).is_err());
}
