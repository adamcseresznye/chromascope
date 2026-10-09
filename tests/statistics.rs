use approx::assert_relative_eq;
use chromascope::{
    domain::{Operation, Request},
    engine::{self, Output},
    jobs::JobControl,
    statistics::*,
};
fn table(rows: &[Vec<Option<f64>>]) -> Table {
    Table {
        samples: (0..rows.len())
            .map(|i| Sample {
                id: format!("s{i}"),
                metadata: [
                    (
                        "group".into(),
                        if i < rows.len() / 2 {
                            "control".into()
                        } else {
                            "case".into()
                        },
                    ),
                    ("role".into(), "sample".into()),
                ]
                .into(),
            })
            .collect(),
        features: (0..rows[0].len())
            .map(|i| Feature {
                id: format!("f{i}"),
                unit: "ng/mL".into(),
            })
            .collect(),
        values: rows.to_vec(),
        provenance: serde_json::json!({"synthetic":true}),
        matrix: None,
        targeted: None,
    }
}
fn settings() -> Settings {
    Settings {
        groups: Some(Groups {
            metadata_key: "group".into(),
            reference: "control".into(),
            comparison: "case".into(),
        }),
        ..Default::default()
    }
}
fn fixture() -> Table {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("reference/statistics.json")).unwrap();
    table(
        &serde_json::from_value::<Vec<Vec<f64>>>(value["rows"].clone())
            .unwrap()
            .iter()
            .map(|r| r.iter().map(|v| Some(*v)).collect())
            .collect::<Vec<_>>(),
    )
}
#[test]
fn independent_welch_intervals_effects_fdr_and_pca() {
    let t = fixture();
    let r = analyze(&t, &settings(), &JobControl::default()).unwrap();
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("reference/statistics.json")).unwrap();
    for (i, row) in r.numerics.comparisons.iter().take(2).enumerate() {
        let v = serde_json::to_value(row).unwrap();
        for key in [
            "difference",
            "ci_low",
            "ci_high",
            "cohen_d",
            "log2_fold_change",
            "t",
            "df",
            "p",
            "q",
        ] {
            assert_relative_eq!(
                v[key].as_f64().unwrap(),
                reference["welch"][i][key].as_f64().unwrap(),
                epsilon = 1e-11,
                max_relative = 1e-10
            );
        }
    }
    assert_eq!(r.numerics.comparisons[2].state, "constant");
    assert!(r.numerics.comparisons[2].p.is_none());
    assert_eq!(r.numerics.fdr_family_size, 2);
    for (i, v) in r.numerics.pca.variance.iter().enumerate() {
        assert_relative_eq!(
            *v,
            reference["pca_variance"][i].as_f64().unwrap(),
            epsilon = 1e-10
        );
    }
    assert_eq!(r.table.values, t.values);
    verify(&r).unwrap();
}
#[test]
fn exact_rank_deficient_and_average_linkage_examples() {
    let t = table(&[
        vec![Some(1.), Some(2.)],
        vec![Some(2.), Some(4.)],
        vec![Some(3.), Some(6.)],
    ]);
    let r = analyze(&t, &Settings::default(), &JobControl::default()).unwrap();
    assert_relative_eq!(r.numerics.pca.variance[0], 5., epsilon = 1e-12);
    assert!(r.numerics.pca.variance[1] < 1e-20);
    for (i, expected) in [-5_f64.sqrt(), 0., 5_f64.sqrt()].iter().enumerate() {
        assert_relative_eq!(r.numerics.pca.scores[i][0], *expected, epsilon = 1e-12);
    }
    let r = analyze(
        &table(&[vec![Some(0.)], vec![Some(1.)], vec![Some(4.)]]),
        &Settings::default(),
        &JobControl::default(),
    )
    .unwrap();
    // Optimal leaf ordering may swap sibling orientation without changing the tree.
    let mut tree = r.numerics.sample_linkage.clone();
    for merge in &mut tree {
        if merge[0] > merge[1] {
            merge.swap(0, 1);
        }
    }
    assert_eq!(tree, vec![[0., 1., 1., 2.], [2., 3., 3.5, 3.]]);
}
#[test]
fn explicit_missing_exclusions_normalization_and_reversibility() {
    let mut t = fixture();
    t.values[0][0] = None;
    assert!(analyze(&t, &settings(), &JobControl::default()).is_err());
    let cfg = Settings {
        missing: "median".into(),
        normalization: "total".into(),
        transform: "log2".into(),
        scaling: "autoscale".into(),
        excluded_samples: [("s5".into(), "predeclared damaged injection".into())].into(),
        ..settings()
    };
    let r = analyze(&t, &cfg, &JobControl::default()).unwrap();
    assert_eq!(r.numerics.imputed.len(), 1);
    assert_eq!(r.numerics.imputed[0]["value"], 3.5);
    assert_eq!(r.numerics.sample_indices.len(), 5);
    assert_eq!(r.table.values[0][0], None);
    for j in 0..3 {
        let mean = r.numerics.processed.iter().map(|v| v[j]).sum::<f64>() / 5.;
        assert_relative_eq!(mean, 0., epsilon = 1e-12);
    }
    let original = analyze(&fixture(), &settings(), &JobControl::default()).unwrap();
    assert_eq!(original.table.values, fixture().values);
    let csv = export_csv(&r).unwrap();
    assert_eq!(csv.lines().count(), 4);
    assert!(csv.contains("predeclared damaged injection"));
}
#[test]
fn missingness_all_missing_and_constant_states() {
    let t = table(&[
        vec![None, Some(2.)],
        vec![None, Some(2.)],
        vec![None, Some(2.)],
        vec![None, Some(2.)],
    ]);
    let r = analyze(&t, &settings(), &JobControl::default()).unwrap();
    assert_eq!(r.numerics.feature_indices, vec![1]);
    assert_eq!(r.numerics.pca.state, "constant");
    assert_eq!(r.numerics.pca.variance_ratio, vec![0.]);
    assert_eq!(r.numerics.comparisons[0].state, "constant");
    assert!(analyze(
        &table(&[vec![None], vec![None]]),
        &Settings::default(),
        &JobControl::default()
    )
    .is_err());
    let mut t = fixture();
    t.values[0][0] = None;
    let r = analyze(
        &t,
        &Settings {
            missing: "complete_features".into(),
            ..Default::default()
        },
        &JobControl::default(),
    )
    .unwrap();
    assert_eq!(r.numerics.feature_indices, vec![1, 2]);
}
#[test]
fn half_minimum_internal_standard_and_pareto_known_values() {
    let t = table(&[
        vec![None, Some(2.)],
        vec![Some(4.), Some(4.)],
        vec![Some(8.), Some(8.)],
    ]);
    let cfg = Settings {
        missing: "half_minimum".into(),
        normalization: "internal_standard".into(),
        internal_standard: Some("f1".into()),
        scaling: "none".into(),
        ..Default::default()
    };
    let r = analyze(&t, &cfg, &JobControl::default()).unwrap();
    assert_eq!(r.numerics.normalization_factors, vec![0.5, 1., 2.]);
    assert_eq!(
        r.numerics.processed,
        vec![vec![4., 4.], vec![4., 4.], vec![4., 4.]]
    );
    let r = analyze(
        &table(&[vec![Some(1.)], vec![Some(2.)], vec![Some(3.)]]),
        &Settings {
            scaling: "pareto".into(),
            ..Default::default()
        },
        &JobControl::default(),
    )
    .unwrap();
    assert_eq!(r.numerics.scale, vec![1.]);
    assert_eq!(r.numerics.processed, vec![vec![-1.], vec![0.], vec![1.]]);
}
#[test]
fn invalid_design_and_corrupt_results_are_rejected() {
    let mut t = fixture();
    t.samples[1].id = t.samples[0].id.clone();
    assert!(validate(&t, &settings()).is_err());
    let mut t = fixture();
    t.values[0][0] = Some(f64::NAN);
    assert!(validate(&t, &settings()).is_err());
    let mut t = fixture();
    t.features[0].unit = "umol/L".into();
    assert!(validate(
        &t,
        &Settings {
            normalization: "total".into(),
            ..settings()
        }
    )
    .is_err());
    let c = JobControl::default();
    c.cancel();
    assert_eq!(
        analyze(&fixture(), &settings(), &c).unwrap_err().code,
        "cancelled"
    );
    let mut r = analyze(&fixture(), &settings(), &JobControl::default()).unwrap();
    r.numerics.comparisons[0].p = Some(0.);
    assert_eq!(verify(&r).unwrap_err().code, "corrupt_project");
}
#[test]
fn engine_roundtrip_exports_and_request_integrity() {
    let request = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "reference test".into(),
        operation: Operation::AnalyzeStatistics {
            table: Box::new(fixture()),
            settings: settings(),
        },
    };
    let response =
        engine::execute(std::path::Path::new("-"), request, &JobControl::default()).unwrap();
    verify_response(&response).unwrap();
    let mut response: engine::Response =
        serde_json::from_slice(&serde_json::to_vec(&response).unwrap()).unwrap();
    if let Operation::AnalyzeStatistics { settings, .. } = &mut response.request.operation {
        settings.confidence = 0.9;
    }
    assert!(verify_response(&response).is_err());
    let Output::Statistics { report } = response.output else {
        panic!()
    };
    let export = engine::execute(
        std::path::Path::new("-"),
        Request {
            version: 1,
            operation_id: Default::default(),
            actor: "test".into(),
            operation: Operation::ExportStatistics { report },
        },
        &JobControl::default(),
    )
    .unwrap();
    verify_response(&export).unwrap();
}

#[test]
fn cli_create_new_export_and_project_revision_restore() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let request = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "CLI reference".into(),
        operation: Operation::AnalyzeStatistics {
            table: Box::new(fixture()),
            settings: settings(),
        },
    };
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
    verify_response(&response).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let csv = temp.path().join("statistics.csv");
    let export = || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
            .arg("export-statistics")
            .arg(&csv)
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
        child.wait_with_output().unwrap()
    };
    assert!(export().status.success());
    let original = std::fs::read(&csv).unwrap();
    assert!(!export().status.success());
    assert_eq!(std::fs::read(&csv).unwrap(), original);
    let root = temp.path().join("project");
    let mut project = chromascope::project::Project::create(&root).unwrap();
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("test_file/data_dependent_02.mzML");
    let dataset = project.register(&source).unwrap();
    project.add_result(&root, dataset, &response).unwrap();
    project.commit(&root, 0).unwrap();
    let reopened = chromascope::project::Project::open(&root).unwrap();
    assert_eq!(reopened.results.len(), 1);
    let restored = chromascope::project::Project::restore_revision(&root, 0, 1).unwrap();
    assert_eq!(restored.revision, 2);
    assert!(restored.results.is_empty());
    assert!(root.join("revision-00000000000000000001.json").exists());
    let mut corrupt: engine::Response = serde_json::from_value(value["result"].clone()).unwrap();
    if let Output::Statistics { report } = &mut corrupt.output {
        report.numerics.processed[0][0] += 1.;
    }
    assert!(project.add_result(&root, dataset, &corrupt).is_err());
}

include!("fixtures/targeted_support.rs");
#[path = "fixtures/statistics_matrix.rs"]
mod matrix_fixture;
#[test]
fn feature_matrix_conversion_keeps_original_filters_and_raw_evidence() {
    let mut matrix = matrix_fixture::report();
    let mut included = matrix.features[0].clone();
    included.id = "stable-triangle".into();
    included.flags.clear();
    included.qc_cv = Some(0.);
    included.filter_state = chromascope::untargeted::FilterState::Included;
    for cell in &mut included.cells[2..] {
        cell.intensity = Some(8.);
        cell.openms_intensity = Some(4.);
        cell.eic[1][1] = 8.;
        cell.apex_spectrum[0][1] = 8.;
    }
    matrix.features.push(included);
    chromascope::untargeted::verify(&matrix).unwrap();
    let mut table = from_matrix(&matrix).unwrap();
    assert_eq!(table.features.len(), 1);
    assert_eq!(table.features[0].unit, "intensity*seconds");
    assert_eq!(table.values[0][0], Some(20.));
    for sample in &mut table.samples {
        sample.metadata.insert(
            "group".into(),
            if sample.id == "s0" || sample.id == "s1" {
                "control".into()
            } else {
                "case".into()
            },
        );
    }
    let report = analyze(&table, &settings(), &JobControl::default()).unwrap();
    verify(&report).unwrap();
    let reference: serde_json::Value =
        serde_json::from_str(include_str!("reference/statistics.json")).unwrap();
    let comparison = serde_json::to_value(&report.numerics.comparisons[0]).unwrap();
    for key in ["t", "df", "p", "difference", "ci_low", "ci_high", "cohen_d"] {
        assert_relative_eq!(
            comparison[key].as_f64().unwrap(),
            reference["one_constant_group"][key].as_f64().unwrap(),
            epsilon = 1e-10,
            // SciPy inverse-t at df=1 differs from the 60-digit beta-integral
            // reference by ~2e-11 relative; use the same 1e-10 relative
            // scientific tolerance as the other independent Welch examples.
            max_relative = 1e-10
        );
    }
    assert!(!report.numerics.comparisons[0].warnings.is_empty());
    assert_eq!(report.table.matrix.as_ref().unwrap().features.len(), 2);
    assert_eq!(
        report.table.matrix.as_ref().unwrap().features[1].cells[0].eic,
        vec![[0., 0.], [1., 20.], [2., 0.]]
    );
    let mut corrupt = table.clone();
    corrupt.values[0][0] = Some(1.);
    assert!(validate(&corrupt, &settings()).is_err());
    let csv = export_csv(&report).unwrap();
    assert!(csv.contains("stable-triangle"));
    assert!(!csv.contains("known-triangle"));
}
#[cfg(feature = "mcp-headless")]
#[test]
fn actual_mcp_statistics_and_export() {
    use std::{
        io::{BufRead, BufReader, Write},
        process::{Command, Stdio},
    };
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
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"statistics-test","version":"1"}}}),
    );
    assert!(read(&mut output).get("result").is_some());
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    let mut operation = Operation::AnalyzeStatistics {
        table: Box::new(fixture()),
        settings: settings(),
    };
    for id in 2..=3 {
        let request = Request {
            version: 1,
            operation_id: Default::default(),
            actor: "MCP analyst".into(),
            operation,
        };
        send(
            &mut input,
            serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":root.join("Cargo.toml"),"request":request}}}),
        );
        let value = read(&mut output);
        assert_ne!(value["result"]["isError"], true, "{value}");
        let response: engine::Response =
            serde_json::from_value(value["result"]["structuredContent"].clone()).unwrap();
        verify_response(&response).unwrap();
        assert_eq!(response.request.actor, "MCP analyst");
        operation = match response.output {
            Output::Statistics { report } => Operation::ExportStatistics { report },
            Output::StatisticsTable { csv } => {
                assert!(csv.contains("cohen_d"));
                assert!(csv.contains("constant"));
                Operation::Metadata
            }
            _ => panic!(),
        };
    }
    drop(input);
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn transformations_scaling_metadata_filters_and_by_family() {
    let table = table(&[
        vec![Some(1.), Some(4.)],
        vec![Some(3.), Some(16.)],
        vec![Some(7.), Some(64.)],
        vec![Some(15.), Some(256.)],
    ]);
    let r = analyze(
        &table,
        &Settings {
            transform: "log2".into(),
            pseudocount: 1.,
            scaling: "none".into(),
            ..Default::default()
        },
        &JobControl::default(),
    )
    .unwrap();
    assert_eq!(
        r.numerics
            .processed
            .iter()
            .map(|v| v[0])
            .collect::<Vec<_>>(),
        vec![1., 2., 3., 4.]
    );
    let r = analyze(
        &table,
        &Settings {
            transform: "sqrt".into(),
            scaling: "none".into(),
            ..Default::default()
        },
        &JobControl::default(),
    )
    .unwrap();
    assert_eq!(
        r.numerics
            .processed
            .iter()
            .map(|v| v[1])
            .collect::<Vec<_>>(),
        vec![2., 4., 8., 16.]
    );
    let r = analyze(
        &table,
        &Settings {
            normalization: "median".into(),
            scaling: "none".into(),
            metadata_equals: [("group".into(), "control".into())].into(),
            ..Default::default()
        },
        &JobControl::default(),
    )
    .unwrap();
    assert_eq!(r.numerics.sample_indices, vec![0, 1]);
    assert_eq!(r.numerics.excluded_samples.len(), 2);
    assert_relative_eq!(
        r.numerics.normalization_factors[0],
        2.5 / 6.,
        epsilon = 1e-12
    );
    let r = analyze(
        &fixture(),
        &Settings {
            fdr: "by".into(),
            ..settings()
        },
        &JobControl::default(),
    )
    .unwrap();
    let bh = analyze(&fixture(), &settings(), &JobControl::default()).unwrap();
    for (a, b) in r
        .numerics
        .comparisons
        .iter()
        .take(2)
        .zip(&bh.numerics.comparisons)
    {
        assert_relative_eq!(a.q.unwrap(), b.q.unwrap() * 1.5, epsilon = 1e-12);
    }
    let r = analyze(
        &table,
        &Settings {
            transform: "log10".into(),
            scaling: "none".into(),
            ..Default::default()
        },
        &JobControl::default(),
    )
    .unwrap();
    assert_relative_eq!(r.numerics.processed[1][0], 3_f64.log10(), epsilon = 1e-12);
    let r = analyze(
        &table,
        &Settings {
            scaling: "autoscale".into(),
            ..Default::default()
        },
        &JobControl::default(),
    )
    .unwrap();
    for j in 0..2 {
        assert_relative_eq!(
            r.numerics
                .processed
                .iter()
                .map(|v| v[j] * v[j])
                .sum::<f64>()
                / 3.,
            1.,
            epsilon = 1e-12
        );
    }
    assert!(analyze(
        &table,
        &Settings {
            metadata_equals: [("absent".into(), "x".into())].into(),
            ..Default::default()
        },
        &JobControl::default()
    )
    .is_err());
}
#[test]
fn targeted_concentrations_source_links_and_metadata_design() {
    let temp = tempfile::tempdir().unwrap();
    let request = raw_request(temp.path());
    let source = std::path::PathBuf::from(&request.samples[0].source);
    let response = engine::execute(
        &source,
        Request {
            version: 1,
            operation_id: Default::default(),
            actor: "source reference".into(),
            operation: Operation::TargetedBatch { batch: request },
        },
        &JobControl::default(),
    )
    .unwrap();
    let Output::TargetedQuantification { batch } = response.output else {
        panic!()
    };
    let mut table = from_targeted(&batch).unwrap();
    assert_eq!(table.features[0].unit, "ng/mL");
    assert_relative_eq!(table.values[6][0].unwrap(), 10., epsilon = 1e-7);
    assert_eq!(table.values[7][0], None);
    for s in &mut table.samples {
        s.metadata.insert(
            "group".into(),
            if s.id == "raw4" || s.id == "raw5" {
                "case".into()
            } else {
                "control".into()
            },
        );
    }
    let cfg = Settings {
        excluded_samples: [("raw7".into(), "predeclared blank exclusion".into())].into(),
        ..settings()
    };
    let report = analyze(&table, &cfg, &JobControl::default()).unwrap();
    assert!(report.table.targeted.is_some());
    assert_eq!(report.numerics.comparisons[0].n_comparison, 2);
    let mut changed = table.clone();
    changed.values[0][0] = Some(999.);
    assert!(validate(&changed, &cfg).is_err());
}
