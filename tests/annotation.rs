use chromascope::annotation::{csv, verify, verify_response};
use chromascope::{
    annotation::*,
    domain::{Operation, Request},
    engine::{self, Output},
    jobs::JobControl,
    spectral::*,
    untargeted::*,
};
use std::{
    collections::BTreeMap,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
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
fn ledger() -> Ledger {
    Ledger {
        version: 1,
        matrix: Box::new(fixture()),
        hypotheses: vec![],
        reviews: vec![],
        adduct_relationships: vec![],
    }
}
fn hypothesis() -> Hypothesis {
    Hypothesis {
        id: uuid::Uuid::new_v4(),
        feature_id: "known-triangle".into(),
        sample_id: "s0".into(),
        label: "unresolved metabolite".into(),
        structure: None,
        lipid: None,
        formula: None,
        formula_candidate: None,
        isotope: None,
        msms: None,
        accession: None,
        confidence: Confidence::Unknown,
        evidence: vec![Evidence {
            description: "Unresolved exact-mass observation".into(),
            reference: "local:synthetic reference".into(),
            same_method_standard: false,
            rt_match: false,
            diagnostic_fragments: false,
            resolves_alternatives: false,
        }],
    }
}
#[test]
fn lipid_hierarchy_does_not_invent_chain_or_sn_resolution() {
    let mut lipid = Lipid {
        class: "PC".into(),
        carbons: 34,
        double_bonds: 1,
        chains: vec![],
    };
    assert_eq!(lipid.label().unwrap(), "PC 34:1");
    lipid.chains = vec![[18, 1], [16, 0]];
    assert_eq!(lipid.label().unwrap(), "PC 16:0_18:1");
    lipid.chains.reverse();
    assert_eq!(lipid.label().unwrap(), "PC 16:0_18:1");
    lipid.chains = vec![[16, 1], [18, 0]];
    assert_eq!(lipid.label().unwrap(), "PC 16:1_18:0");
    lipid.chains = vec![[16, 0], [16, 1]];
    assert!(lipid.label().is_err());
    lipid.class = "Cer".into();
    assert!(lipid.label().is_err());
    let mut h = hypothesis();
    h.lipid = Some(Lipid {
        class: "PC".into(),
        carbons: 34,
        double_bonds: 1,
        chains: vec![[16, 0], [18, 1]],
    });
    h.label = "PC 16:0/18:1".into();
    assert!(add(&ledger(), h.clone(), 0).is_err());
    h.label = "PC 16:0_18:1".into();
    assert!(add(&ledger(), h.clone(), 0).is_err());
    h.evidence[0].diagnostic_fragments = true;
    assert!(add(&ledger(), h, 0).is_ok());
}
#[test]
fn competing_isomers_reviews_stale_and_export_preserve_originals() {
    let original = ledger();
    let mut a = hypothesis();
    a.structure = Some("isomer-a".into());
    let mut b = a.clone();
    b.id = uuid::Uuid::new_v4();
    b.structure = Some("isomer-b".into());
    let l = add(&original, a.clone(), 0).unwrap();
    let l = add(&l, b.clone(), 1).unwrap();
    let l = review(
        &l,
        a.id,
        2,
        "analyst",
        "tentative mass evidence",
        Decision::Accept,
    )
    .unwrap();
    assert!(review(&l, b.id, 2, "analyst", "stale", Decision::Reject).is_err());
    let l = review(
        &l,
        b.id,
        3,
        "analyst",
        "alternative lacks evidence",
        Decision::Reject,
    )
    .unwrap();
    let l = review(
        &l,
        a.id,
        4,
        "analyst",
        "reconsider alternatives",
        Decision::Reopen,
    )
    .unwrap();
    assert_eq!(l.reviews.len(), 3);
    assert!(original.hypotheses.is_empty());
    let csv = csv(&l).unwrap();
    assert!(csv.contains("isomer-a"));
    assert!(csv.contains("isomer-b"));
    assert!(csv.contains("Unknown"));
    assert!(!csv.contains("ConfirmedIdentity"));
    let round: Ledger = serde_json::from_str(&serde_json::to_string(&l).unwrap()).unwrap();
    verify(&round).unwrap();
    let mut h = hypothesis();
    h.confidence = Confidence::ConfirmedIdentity;
    assert!(add(&original, h, 0).is_err());
}
fn glucose() -> (Ledger, ProposalConfig) {
    let mut l = ledger();
    let mz = 181.070664570801;
    l.matrix.features[0].mz = mz;
    for c in &mut l.matrix.features[0].cells {
        c.mz = Some(mz);
        c.isotope_mz = vec![mz];
        c.apex_spectrum = vec![[mz, c.intensity.unwrap()]];
    }
    let c = ProposalConfig {
        feature_id: "known-triangle".into(),
        sample_id: "s0".into(),
        formula: FormulaConfig {
            observed_mz: mz,
            polarity: Polarity::Positive,
            adducts: vec!["[M+H]+".into()],
            tolerance: Tolerance {
                value: 0.1,
                unit: MassUnit::Ppm,
            },
            maximum_atoms: [("C".into(), 6), ("H".into(), 12), ("O".into(), 6)].into(),
            maximum_candidates: 20,
        },
        library: None,
        search: SearchConfig::default(),
        ms2_index: 0,
    };
    (l, c)
}
#[test]
fn reference_glucose_formula_isotope_and_incorrect_adduct() {
    // Neutral mass independently computed from published monoisotopic C/H/O values.
    let (l, c) = glucose();
    let generated = propose(&l, &c, 0).unwrap();
    assert_eq!(generated.hypotheses.len(), 1);
    let h = &generated.hypotheses[0];
    assert_eq!(h.label, "C6H12O6 [M+H]+");
    assert_eq!(h.confidence, Confidence::Unknown);
    assert!(h.isotope.is_some());
    assert!(
        (h.formula.as_ref().unwrap().candidates[0].neutral_mass_da - 180.06338810418).abs() < 1e-10
    );
    let mut wrong = generated.clone();
    wrong.matrix.features[0].cells[0].adduct = Some("[M+Na]+".into());
    assert!(verify(&wrong).is_err());
    let mut wrong = generated.clone();
    wrong.matrix.features[0].cells[0].charge = Some(2);
    assert!(verify(&wrong).is_err());
    let mut wrong = generated.clone();
    wrong.hypotheses[0].formula.as_mut().unwrap().candidates[0].error_ppm = 1.;
    assert!(verify(&wrong).is_err());
    let mut wrong = generated.clone();
    wrong.hypotheses[0].isotope.as_mut().unwrap().spectrum.peaks[0][1] += 1.;
    assert!(verify(&wrong).is_err());
    let mut wrong = c.clone();
    wrong.formula.observed_mz = 195.;
    assert!(propose(&l, &wrong, 0).is_err());
}
fn library_config() -> (Ledger, ProposalConfig) {
    let (mut l, mut c) = glucose();
    l.matrix.features[0].cells[0].ms2.push(Ms2Evidence {
        original_index: 4,
        native_id: "scan=4".into(),
        raw_rt_seconds: 1.,
        precursor_mz: c.formula.observed_mz,
        charge: 1,
        isolation_lower_offset_da: 0.5,
        isolation_upper_offset_da: 0.5,
        association: "precursor_mz_and_raw_rt_window".into(),
        collision_energy_ev: Some(20.),
        representation: "centroid".into(),
        peaks: vec![[50., 10.], [100., 20.], [150., 30.]],
    });
    let record = |name: &str, id: &str, structure: &str| {
        format!("Name: {name}\nDB#: {id}\nFormula: C6H12O6\nSMILES: {structure}\nPrecursorMZ: 181.070664570801\nPrecursorType: [M+H]+\nIonMode: positive\nCollisionEnergy: 20 eV\nNum Peaks: 3\n50 10\n100 20\n150 30\n\n")
    };
    c.library = Some(Box::new(
        import_library(
            record("glucose synthetic", "a", "OCC(O)C(O)C(O)C(O)C=O")
                + &record("fructose synthetic", "b", "OCC(O)C(O)C(O)C(=O)CO"),
            "msp".into(),
            LibrarySource {
                name: "Synthetic isomer test".into(),
                version: "1".into(),
                url: "local:authored-test".into(),
                license: "CC0".into(),
            },
        )
        .unwrap(),
    ));
    c.search.allow_missing_metadata = true;
    c.search.require_same_instrument = false;
    (l, c)
}
#[test]
fn library_isomers_remain_unknown_and_conflicting_evidence_fails() {
    let (l, c) = library_config();
    let generated = propose(&l, &c, 0).unwrap();
    let hits = generated
        .hypotheses
        .iter()
        .filter(|h| h.msms.is_some())
        .collect::<Vec<_>>();
    assert_eq!(hits.len(), 2);
    assert_ne!(hits[0].structure, hits[1].structure);
    assert!(hits.iter().all(|h| h.confidence == Confidence::Unknown));
    let mut h = hits[0].clone();
    h.formula = Some(generated.hypotheses[0].formula.clone().unwrap());
    h.formula_candidate = Some(0);
    assert!(add(&l, h.clone(), 0).is_ok());
    let mut bad = c.clone();
    let raw = bad
        .library
        .as_ref()
        .unwrap()
        .raw_text
        .replace("C6H12O6", "C5H10O5");
    bad.library = Some(Box::new(
        import_library(
            raw,
            "msp".into(),
            bad.library.as_ref().unwrap().source.clone(),
        )
        .unwrap(),
    ));
    let conflicts = propose(&l, &bad, 0).unwrap();
    let mut conflict = conflicts
        .hypotheses
        .iter()
        .find(|h| h.msms.is_some())
        .unwrap()
        .clone();
    conflict.formula = h.formula.clone();
    conflict.formula_candidate = Some(0);
    assert!(add(&l, conflict, 0).is_err());
    h.msms.as_mut().unwrap().query.inputs[0].id = "other-scan".into();
    assert!(add(&l, h, 0).is_err());
}
#[test]
fn engine_cli_review_and_no_overwrite_export() {
    let (l, c) = glucose();
    let req = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "test".into(),
        operation: Operation::ProposeFeatureAnnotations {
            ledger: Box::new(l),
            config: c,
            expected_revision: 0,
        },
    };
    let response = engine::execute(Path::new("-"), req.clone(), &JobControl::default()).unwrap();
    let Output::FeatureAnnotations { ledger } = &response.output else {
        panic!()
    };
    assert_eq!(ledger.hypotheses.len(), 1);
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
        .write_all(&serde_json::to_vec(&req).unwrap())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("annotations.csv");
    for success in [true, false] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
            .arg("export-annotations")
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
        assert_eq!(child.wait_with_output().unwrap().status.success(), success);
    }
    assert!(std::fs::read_to_string(path).unwrap().contains("Unknown"));
}

#[test]
fn coeluting_h_na_relationship_is_tentative_and_replayed() {
    let (mut l, c) = glucose();
    let mut sodium = l.matrix.features[0].clone();
    sodium.id = "sodium-feature".into();
    let shift = 21.98194423547;
    sodium.mz += shift;
    for cell in &mut sodium.cells {
        let mz = cell.mz.unwrap() + shift;
        cell.mz = Some(mz);
        cell.isotope_mz = vec![mz];
        cell.apex_spectrum = vec![[mz, cell.intensity.unwrap()]];
    }
    l.matrix.features.push(sodium);
    let l = propose(&l, &c, 0).unwrap();
    let mut na = c.clone();
    na.feature_id = "sodium-feature".into();
    na.formula.observed_mz += shift;
    na.formula.adducts = vec!["[M+Na]+".into()];
    let l = propose(&l, &na, 1).unwrap();
    assert_eq!(l.adduct_relationships.len(), 1);
    assert!(l.adduct_relationships[0].neutral_mass_difference_da < 1e-10);
    assert!(l.adduct_relationships[0].algorithm.contains("tentative"));
    let mut bad = l.clone();
    bad.adduct_relationships[0].tolerance_ppm = 999.;
    assert!(verify(&bad).is_err());
    let mut separated = l.clone();
    let f = &mut separated.matrix.features[1];
    f.aligned_rt_seconds = 40.;
    for cell in &mut f.cells {
        cell.raw_rt_seconds = Some(40.);
        cell.aligned_rt_seconds = Some(40.);
        cell.raw_bounds_seconds = Some([39., 41.]);
        cell.eic = vec![[39., 0.], [40., cell.intensity.unwrap()], [41., 0.]];
    }
    for h in &mut separated.hypotheses {
        if h.feature_id == "sodium-feature" {
            let isotope = h.isotope.as_mut().unwrap();
            isotope.spectrum.rt_minutes = Some(40. / 60.);
            *isotope =
                chromascope::spectral::isotopes(isotope.spectrum.clone(), isotope.config.clone())
                    .unwrap();
        }
    }
    separated.adduct_relationships.clear();
    verify(&separated).unwrap();
}
#[test]
fn response_replay_rejects_changed_history_or_candidate() {
    let (l, c) = glucose();
    let req = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "test".into(),
        operation: Operation::ProposeFeatureAnnotations {
            ledger: Box::new(l),
            config: c,
            expected_revision: 0,
        },
    };
    let mut r = engine::execute(Path::new("-"), req, &JobControl::default()).unwrap();
    verify_response(&r).unwrap();
    if let Output::FeatureAnnotations { ledger } = &mut r.output {
        ledger.hypotheses[0].label = "invented structure".into();
    }
    assert!(verify_response(&r).is_err());
}
#[cfg(feature = "mcp-headless")]
#[test]
fn actual_mcp_propose_review_export() {
    use std::io::{BufRead, BufReader};
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
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"annotation-test","version":"1"}}}),
    );
    assert!(read(&mut output).get("result").is_some());
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    let (l, c) = glucose();
    let mut operation = Operation::ProposeFeatureAnnotations {
        ledger: Box::new(l),
        config: c,
        expected_revision: 0,
    };
    for id in 2..=4 {
        let request = Request {
            version: 1,
            operation_id: Default::default(),
            actor: "mcp-analyst".into(),
            operation,
        };
        send(
            &mut input,
            serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":root.join("Cargo.toml"),"request":request}}}),
        );
        let v = read(&mut output);
        assert_ne!(v["result"]["isError"], true, "{v}");
        let response: engine::Response =
            serde_json::from_value(v["result"]["structuredContent"].clone()).unwrap();
        verify_response(&response).unwrap();
        operation = match response.output {
            Output::FeatureAnnotations { ledger } if id == 2 => {
                Operation::ReviewFeatureAnnotation {
                    hypothesis_id: ledger.hypotheses[0].id,
                    expected_revision: 1,
                    ledger,
                    reason: "tentative hypothesis accepted".into(),
                    decision: Decision::Accept,
                }
            }
            Output::FeatureAnnotations { ledger } => {
                assert_eq!(ledger.reviews[0].actor, "mcp-analyst");
                Operation::ExportFeatureAnnotations { ledger }
            }
            Output::FeatureAnnotationTable { csv } => {
                assert!(csv.contains("Unknown"));
                assert!(!csv.contains("ConfirmedIdentity"));
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
fn project_annotations_preserve_history_and_require_registered_sources() {
    use chromascope::project::Project;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    let raw = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_file/data_dependent_02.mzML");
    let mut project = Project::create(&root).unwrap();
    let id = project.register(&raw).unwrap();
    let (mut l, c) = glucose();
    for hash in l.matrix.source_hashes.values_mut() {
        *hash = project.sources[0].sha256.clone();
    }
    let req = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "project-test".into(),
        operation: Operation::ProposeFeatureAnnotations {
            ledger: Box::new(l),
            config: c,
            expected_revision: 0,
        },
    };
    let response = engine::execute(Path::new("-"), req, &JobControl::default()).unwrap();
    project.add_result(&root, id, &response).unwrap();
    project.commit(&root, 0).unwrap();
    assert_eq!(Project::open(&root).unwrap().results.len(), 1);
    let Output::FeatureAnnotations { ledger } = &response.output else {
        panic!()
    };
    let request = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "project-test".into(),
        operation: Operation::ReviewFeatureAnnotation {
            ledger: ledger.clone(),
            hypothesis_id: ledger.hypotheses[0].id,
            expected_revision: 1,
            reason: "tentative evidence review".into(),
            decision: Decision::Accept,
        },
    };
    let reviewed = engine::execute(Path::new("-"), request, &JobControl::default()).unwrap();
    project.add_result(&root, id, &reviewed).unwrap();
    project.commit(&root, 1).unwrap();
    assert_eq!(Project::open(&root).unwrap().results.len(), 2);
    let Output::FeatureAnnotations {
        ledger: reviewed_ledger,
    } = &reviewed.output
    else {
        panic!()
    };
    let export_request = Request {
        version: 1,
        operation_id: Default::default(),
        actor: "project-test".into(),
        operation: Operation::ExportFeatureAnnotations {
            ledger: reviewed_ledger.clone(),
        },
    };
    let mut table =
        engine::execute(Path::new("-"), export_request, &JobControl::default()).unwrap();
    verify_response(&table).unwrap();
    if let Output::FeatureAnnotationTable { csv } = &mut table.output {
        csv.push_str("fabricated");
    }
    assert!(project.add_result(&root, id, &table).is_err());
    let mut bad = reviewed;
    if let Output::FeatureAnnotations { ledger } = &mut bad.output {
        ledger.reviews[0].reason = "rewritten".into();
    }
    assert!(project.add_result(&root, id, &bad).is_err());
    assert_eq!(Project::open(&root).unwrap().results.len(), 2);
}
