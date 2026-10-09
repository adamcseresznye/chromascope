use chromascope::{
    domain::{Operation, Request},
    engine::{self, Output},
    jobs::JobControl,
    spectral::*,
};
use std::{collections::BTreeMap, path::Path};
fn spectrum(peaks: Vec<[f64; 2]>) -> Spectrum {
    Spectrum {
        id: "unknown".into(),
        peaks,
        ms_level: 2,
        representation: Representation::Centroid,
        polarity: Polarity::Positive,
        precursor_mz: Some(195.0877),
        precursor_type: Some("[M+H]+".into()),
        collision_energy: Some(Energy {
            value: 20.0,
            unit: "eV".into(),
        }),
        instrument: Some("LC-ESI-QTOF".into()),
        rt_minutes: Some(2.0),
        metadata: BTreeMap::from([("acquisition_mode".into(), "dda".into())]),
    }
}
fn source() -> LibrarySource {
    LibrarySource {
        name: "Test synthetic library".into(),
        version: "1".into(),
        url: "local:synthetic-reference".into(),
        license: "CC0-1.0 (test authored)".into(),
    }
}
fn msp() -> String {
    "Name: caffeine-like synthetic\nDB#: ref-1\nFormula: C8H10N4O2\nSMILES: CN1C=NC2=C1C(=O)N(C(=O)N2C)C\nPrecursorMZ: 195.0877\nPrecursorType: [M+H]+\nIonMode: positive\nCollisionEnergy: 20 eV\nInstrumentType: LC-ESI-QTOF\nNum Peaks: 3\n50.0 10\n100.0 20\n150.0 30\n".into()
}
fn query() -> Processed {
    process(
        vec![spectrum(vec![[50.0, 10.0], [100.0, 20.0], [150.0, 30.0]])],
        vec![],
        Processing::default(),
    )
    .unwrap()
}
fn report() -> SearchReport {
    search(
        query(),
        import_library(msp(), "msp".into(), source()).unwrap(),
        SearchConfig::default(),
    )
    .unwrap()
}
fn request(operation: Operation) -> Request {
    Request {
        version: 1,
        operation_id: Default::default(),
        actor: "reference-analyst".into(),
        operation,
    }
}
fn annotation(confidence: Confidence) -> Annotation {
    Annotation {
        id: uuid::Uuid::nil(),
        actor: String::new(),
        reason: "Reviewed all candidates".into(),
        unix_ms: 0,
        candidate_accession: Some("ref-1".into()),
        label: "candidate".into(),
        confidence,
        evidence: vec![],
    }
}
#[test]
fn cosine_exact_partial_disjoint_and_one_to_one() {
    let a = spectrum(vec![[50.0, 3.0], [100.0, 4.0]]);
    let b = spectrum(vec![[50.001, 4.0], [150.0, 3.0]]);
    let tol = Tolerance {
        value: 0.01,
        unit: MassUnit::Da,
    };
    let s = compare(&a, &b, &tol).unwrap();
    assert!((s.cosine - 12.0 / 25.0).abs() < 1e-14);
    assert_eq!(s.matches.len(), 1);
    assert!((s.matched_query_fraction - 3.0 / 7.0).abs() < 1e-14);
    assert!((compare(&a, &a, &tol).unwrap().cosine - 1.0).abs() < 1e-14);
    let b = spectrum(vec![[70.0, 4.0], [150.0, 3.0]]);
    assert_eq!(compare(&a, &b, &tol).unwrap().cosine, 0.0);
    let crowded = spectrum(vec![[50.0, 3.0], [50.005, 4.0]]);
    let single = spectrum(vec![[50.003, 5.0]]);
    let s = compare(&crowded, &single, &tol).unwrap();
    assert_eq!(s.matches.len(), 1);
    assert_eq!(s.matches[0].query_index, 1);
    assert!((s.cosine - 0.8).abs() < 1e-14);
}
#[test]
fn ppm_tolerance_is_mass_dependent_and_negative_peaks_do_not_score() {
    let a = spectrum(vec![[50.0, -10.0], [100.0, 10.0], [1000.0, 10.0]]);
    let b = spectrum(vec![[50.0, 100.0], [100.005, 10.0], [1000.005, 10.0]]);
    let s = compare(
        &a,
        &b,
        &Tolerance {
            value: 10.0,
            unit: MassUnit::Ppm,
        },
    )
    .unwrap();
    assert_eq!(s.matches.len(), 1);
    assert_eq!(s.matches[0].query_index, 2);
    assert!(s.cosine < 0.1);
}
#[test]
fn average_and_background_keep_raw_and_negative_values() {
    let mut a = spectrum(vec![[50.0, 10.0], [100.0, 20.0]]);
    let mut b = spectrum(vec![[50.002, 30.0], [150.0, 10.0]]);
    a.rt_minutes = Some(1.0);
    b.rt_minutes = Some(3.0);
    let bg = spectrum(vec![[100.0, 30.0], [200.0, 5.0]]);
    let raw = vec![a, b];
    let p = process(
        raw.clone(),
        vec![bg],
        Processing {
            relative_threshold: 0.0,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(p.inputs, raw);
    assert!((p.averaged.peaks[0][0] - 50.0015).abs() < 1e-12);
    assert_eq!(p.averaged.peaks[0][1], 20.0);
    assert_eq!(p.averaged.peaks[1], [100.0, 10.0]);
    assert!(p.signed_subtracted.contains(&[100.0, -20.0]));
    assert!(p.signed_subtracted.contains(&[200.0, -5.0]));
    assert_eq!(p.spectrum.peaks.len(), 2);
    assert_eq!(p.spectrum.rt_minutes, Some(2.0));
    verify_processed(&p).unwrap();
}
#[test]
fn incompatible_averaging_and_unknown_representation_fail() {
    let a = spectrum(vec![[50.0, 10.0]]);
    let mut b = a.clone();
    b.collision_energy.as_mut().unwrap().value = 30.0;
    assert!(process(vec![a.clone(), b], vec![], Processing::default()).is_err());
    let mut b = a;
    b.representation = Representation::Unknown;
    assert_eq!(
        process(vec![b], vec![], Processing::default())
            .unwrap_err()
            .code,
        "unsupported_capability"
    );
}
#[test]
fn mzsignal_profile_centroid_recovers_gaussian_reference() {
    let peaks = (0..201)
        .map(|i| {
            let mz = 99.9 + i as f64 * 0.001;
            [mz, 1000.0 * (-0.5 * ((mz - 100.0) / 0.01).powi(2)).exp()]
        })
        .collect();
    let mut s = spectrum(peaks);
    s.representation = Representation::Profile;
    assert!(process(vec![s.clone()], vec![], Processing::default()).is_err());
    let p = process(
        vec![s.clone()],
        vec![],
        Processing {
            centroid_snr: Some(1.0),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(p.inputs[0], s);
    assert_eq!(p.spectrum.representation, Representation::Centroid);
    assert_eq!(p.spectrum.peaks.len(), 1);
    assert!((p.spectrum.peaks[0][0] - 100.0).abs() < 1e-5);
    assert!((p.spectrum.peaks[0][1] - 1000.0).abs() < 0.01);
}
#[test]
fn strict_library_metadata_index_and_replay() {
    let lib = import_library(msp(), "msp".into(), source()).unwrap();
    assert_eq!(lib.precursor_index, vec![(195.0877, 0)]);
    assert_eq!(lib.entries[0].formula.as_deref(), Some("C8H10N4O2"));
    assert_eq!(lib.raw_text, msp());
    assert_eq!(lib.sha256.len(), 64);
    verify_library(&lib).unwrap();
    let mut changed = lib;
    changed.entries[0].spectrum.peaks[0][1] += 1.0;
    assert_eq!(
        verify_library(&changed).unwrap_err().code,
        "corrupt_artifact"
    );
    assert!(import_library(
        msp().replace("Num Peaks: 3", "Num Peaks: 4"),
        "msp".into(),
        source()
    )
    .is_err());
    let mut declaration = source();
    declaration.license.clear();
    assert!(import_library(msp(), "msp".into(), declaration).is_err());
}
#[test]
fn mgf_import_and_unterminated_block() {
    let mgf="BEGIN IONS\nTITLE=mgf-test\nPEPMASS=195.0877 1000\nCHARGE=1+\nADDUCT=[M+H]+\nCOLLISION_ENERGY=20 eV\nINSTRUMENT=LC-ESI-QTOF\n50 10\n100 20\n150 30\nEND IONS\n";
    let l = import_library(mgf.into(), "mgf".into(), source()).unwrap();
    assert_eq!(l.entries[0].spectrum.precursor_mz, Some(195.0877));
    assert_eq!(l.entries[0].spectrum.polarity, Polarity::Positive);
    assert!(import_library(mgf.replace("END IONS", ""), "mgf".into(), source()).is_err());
}
#[test]
fn known_match_is_unknown_until_review_and_confidence_requires_evidence() {
    let r = report();
    assert_eq!(r.candidates.len(), 1);
    assert!((r.candidates[0].similarity.cosine - 1.0).abs() < 1e-14);
    assert!(r.annotations.is_empty());
    for confidence in [
        Confidence::ConfirmedIdentity,
        Confidence::ProbableStructure,
        Confidence::CompoundClass,
    ] {
        assert!(annotate(&r, 0, annotation(confidence), "analyst").is_err());
    }
    let mut a = annotation(Confidence::ConfirmedIdentity);
    a.evidence.push(Evidence{description:"Authentic standard acquired with same LC and MS settings, RT matched, diagnostic ions resolved alternatives".into(),reference:"local:standard-run-uuid".into(),same_method_standard:true,rt_match:true,diagnostic_fragments:true,resolves_alternatives:true});
    let reviewed = annotate(&r, 0, a, "analyst").unwrap();
    assert_eq!(reviewed.annotations.len(), 1);
    assert!(r.annotations.is_empty());
    let unknown = annotate(&reviewed, 1, annotation(Confidence::Unknown), "analyst").unwrap();
    assert_eq!(unknown.annotations.len(), 2);
    assert_eq!(unknown.annotations[0], reviewed.annotations[0]);
    assert_eq!(
        annotate(&reviewed, 0, annotation(Confidence::Unknown), "analyst")
            .unwrap_err()
            .code,
        "stale_revision"
    );
    verify_report(&unknown).unwrap();
}
#[test]
fn isobaric_alternatives_retained_and_similarity_alone_cannot_disambiguate() {
    let text = format!(
        "{}\n{}",
        msp(),
        msp()
            .replace("ref-1", "ref-isomer")
            .replace("caffeine-like synthetic", "isobaric synthetic alternative")
    );
    let r = search(
        query(),
        import_library(text, "msp".into(), source()).unwrap(),
        SearchConfig::default(),
    )
    .unwrap();
    assert_eq!(r.candidates.len(), 2);
    assert_eq!(
        r.candidates[0].similarity.cosine,
        r.candidates[1].similarity.cosine
    );
    assert!(r.warnings.iter().any(|w| w.contains("isomers/isobars")));
    assert!(annotate(&r, 0, annotation(Confidence::ProbableStructure), "analyst").is_err());
}
#[test]
fn search_filters_wrong_polarity_adduct_instrument_energy_and_missing() {
    for (key, value, reason) in [
        ("positive", "negative", "polarity"),
        ("[M+H]+", "[M+Na]+", "precursor_type"),
        ("LC-ESI-QTOF", "LC-ESI-Orbitrap", "instrument"),
        ("20 eV", "20 NCE", "collision_energy"),
    ] {
        let l = import_library(msp().replace(key, value), "msp".into(), source()).unwrap();
        let r = search(query(), l, SearchConfig::default()).unwrap();
        assert!(r.candidates.is_empty());
        assert_eq!(r.excluded[reason], 1);
    }
    let l = import_library(
        msp().replace("CollisionEnergy: 20 eV\n", ""),
        "msp".into(),
        source(),
    )
    .unwrap();
    let r = search(query(), l.clone(), SearchConfig::default()).unwrap();
    assert!(r.candidates.is_empty());
    let r = search(
        query(),
        l,
        SearchConfig {
            allow_missing_metadata: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.candidates.len(), 1);
    assert!(r.candidates[0]
        .warnings
        .iter()
        .any(|w| w.contains("Missing collision")));
}
#[test]
fn low_quality_single_ion_false_match_dia_and_outside_precursor_rejected() {
    let l = import_library(msp(), "msp".into(), source()).unwrap();
    let q = process(
        vec![spectrum(vec![[150.0, 1000.0]])],
        vec![],
        Processing::default(),
    )
    .unwrap();
    assert!(search(q, l.clone(), SearchConfig::default())
        .unwrap()
        .candidates
        .is_empty());
    let mut s = spectrum(vec![[50.0, 10.0], [100.0, 20.0], [150.0, 30.0]]);
    s.precursor_mz = Some(196.0);
    assert!(search(
        process(vec![s.clone()], vec![], Processing::default()).unwrap(),
        l.clone(),
        SearchConfig::default()
    )
    .unwrap()
    .candidates
    .is_empty());
    s.metadata.insert("acquisition_mode".into(), "dia".into());
    assert_eq!(
        search(
            process(vec![s], vec![], Processing::default()).unwrap(),
            l,
            SearchConfig::default()
        )
        .unwrap_err()
        .code,
        "unsupported_capability"
    );
}
#[test]
fn invalid_spectrum_and_resource_limits_explicit() {
    for peaks in [
        vec![[50.0, f64::NAN]],
        vec![[50.0, 1.0], [50.0, 2.0]],
        vec![[100.0, 1.0], [50.0, 1.0]],
        vec![[0.0, 1.0]],
    ] {
        assert!(spectrum(peaks).validate().is_err());
    }
    let q = spectrum(vec![[50.0, 10.0]]);
    assert_eq!(
        process(vec![q; 257], vec![], Processing::default())
            .unwrap_err()
            .code,
        "resource_limit"
    );
    let mut r = report();
    r.candidates[0].similarity.cosine = 0.5;
    assert_eq!(verify_report(&r).unwrap_err().code, "corrupt_artifact");
}
fn formula_config() -> FormulaConfig {
    FormulaConfig {
        observed_mz: 195.087652032,
        polarity: Polarity::Positive,
        adducts: vec!["[M+H]+".into(), "[M+Na]+".into()],
        tolerance: Tolerance {
            value: 5.0,
            unit: MassUnit::Ppm,
        },
        maximum_atoms: BTreeMap::from([
            ("C".into(), 12),
            ("H".into(), 30),
            ("N".into(), 6),
            ("O".into(), 6),
        ]),
        maximum_candidates: 100,
    }
}
#[test]
fn formula_known_caffeine_mass_adduct_hypotheses_and_bounds() {
    let r = formula_candidates(formula_config()).unwrap();
    let caffeine = r
        .candidates
        .iter()
        .find(|c| c.formula == "C8H10N4O2" && c.adduct == "[M+H]+")
        .unwrap();
    assert!((caffeine.neutral_mass_da - 194.08037558).abs() < 1e-8);
    assert_eq!(caffeine.dbe, 6.0);
    assert!(r.candidates.iter().all(|c| c.error_ppm.abs() <= 5.001));
    let mut cfg = formula_config();
    cfg.maximum_atoms.insert("C".into(), 500);
    cfg.maximum_atoms.insert("S".into(), 500);
    assert_eq!(formula_candidates(cfg).unwrap_err().code, "resource_limit");
    let mut cfg = formula_config();
    cfg.polarity = Polarity::Negative;
    assert!(formula_candidates(cfg).is_err());
}
#[test]
fn carbon_isotope_spacing_charge_and_missing_states() {
    let mut s = spectrum(vec![
        [100.0, 1000.0],
        [101.00335483507, 108.15728292732236],
        [102.00670967014, 5.0],
    ]);
    s.ms_level = 1;
    s.precursor_mz = None;
    let config = IsotopeConfig {
        expected_atom_counts: None,
        monoisotopic_mz: 100.0,
        maximum_charge: 3,
        maximum_isotopes: 3,
        tolerance: Tolerance {
            value: 5.0,
            unit: MassUnit::Ppm,
        },
    };
    let r = isotopes(s.clone(), config.clone()).unwrap();
    assert_eq!(r.hypotheses.len(), 1);
    assert_eq!(r.hypotheses[0].charge, 1);
    assert!((r.hypotheses[0].carbon_estimate.unwrap() - 10.0).abs() < 1e-8);
    s.peaks = vec![[100.0, 10.0], [100.501677417535, 2.0]];
    assert_eq!(isotopes(s, config).unwrap().hypotheses[0].charge, 2);
}
#[test]
fn real_massbank_reference_preserves_license_annotations_and_energy() {
    let text = format!(
        "{}{}",
        include_str!("reference/MSBNK-Antwerp_Univ-AN111301.txt"),
        include_str!("reference/MSBNK-Antwerp_Univ-AN111302.txt")
    );
    let source = LibrarySource {
        name: "MassBank Antwerp University".into(),
        version: "2026.03".into(),
        url: "https://github.com/MassBank/MassBank-data/tree/2026.03/Antwerp_Univ".into(),
        license: "CC BY (per-record)".into(),
    };
    let lib = import_library(text, "massbank".into(), source).unwrap();
    assert_eq!(lib.entries.len(), 2);
    assert_eq!(lib.entries[0].spectrum.peaks.len(), 26);
    assert_eq!(lib.entries[1].spectrum.peaks.len(), 21);
    assert_eq!(lib.entries[0].metadata["LICENSE"], vec!["CC BY"]);
    assert_eq!(lib.entries[0].metadata["PK$ANNOTATION"].len(), 27);
    let query = process(
        vec![lib.entries[0].spectrum.clone()],
        vec![],
        Processing::default(),
    )
    .unwrap();
    let r = search(
        query,
        lib,
        SearchConfig {
            energy_tolerance: 0.0,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.candidates.len(), 1);
    assert!(r.candidates[0].similarity.cosine > 0.999999);
    assert_eq!(r.excluded["collision_energy"], 1);
    assert!(r.annotations.is_empty());
    verify_report(&r).unwrap();
}
#[test]
fn engine_cli_identical_search_and_annotation() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let op = Operation::SearchSpectralLibrary {
        query: Box::new(query()),
        library: Box::new(import_library(msp(), "msp".into(), source()).unwrap()),
        config: SearchConfig::default(),
    };
    let req = request(op);
    let expected = engine::execute(Path::new("-"), req.clone(), &JobControl::default()).unwrap();
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
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let actual: engine::Response = serde_json::from_value(value["result"].clone()).unwrap();
    match (actual.output, expected.output) {
        (Output::SpectralSearch { report: a }, Output::SpectralSearch { report: b }) => {
            assert_eq!(a.candidates, b.candidates);
            verify_report(&a).unwrap();
            let reviewed = engine::execute(
                Path::new("-"),
                request(Operation::AnnotateSpectrum {
                    report: a,
                    expected_revision: 0,
                    annotation: annotation(Confidence::Unknown),
                }),
                &JobControl::default(),
            )
            .unwrap();
            assert!(matches!(reviewed.output, Output::SpectralSearch { .. }));
        }
        _ => panic!("Incorrect outputs"),
    }
}
#[test]
fn native_scan_inspection_retains_isolation_and_rt() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_file/data_dependent_02.mzML");
    let response = engine::execute(
        &path,
        request(Operation::InspectSpectra {
            indices: vec![0],
            background_indices: vec![],
            config: Processing {
                centroid_snr: Some(1.0),
                ..Default::default()
            },
        }),
        &JobControl::default(),
    )
    .unwrap();
    assert!(response.source_sha256.is_some());
    match response.output {
        Output::SpectralProcessing { processed } => {
            assert!(processed.inputs[0]
                .metadata
                .contains_key("native_scan_metadata_json"));
            assert_eq!(processed.inputs[0].metadata["original_index"], "0");
            assert!(processed.inputs[0].rt_minutes.is_some());
        }
        _ => panic!("Incorrect output"),
    }
}
#[cfg(feature = "mcp-headless")]
#[test]
fn actual_mcp_import_search_annotation_and_root_policy() {
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
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"spectral-test","version":"1"}}}),
    );
    assert!(read(&mut output).get("result").is_some());
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    let path = root.join("Cargo.toml");
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":path,"request":request(Operation::ImportSpectralLibrary{text:msp(),format:"msp".into(),source:source()})}}}),
    );
    let value = read(&mut output);
    assert_ne!(value["result"]["isError"], true, "{value}");
    let library: Library =
        serde_json::from_value(value["result"]["structuredContent"]["output"]["library"].clone())
            .unwrap();
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":path,"request":request(Operation::SearchSpectralLibrary{query:Box::new(query()),library:Box::new(library),config:SearchConfig::default()})}}}),
    );
    let value = read(&mut output);
    assert_ne!(value["result"]["isError"], true, "{value}");
    let report: SearchReport =
        serde_json::from_value(value["result"]["structuredContent"]["output"]["report"].clone())
            .unwrap();
    verify_report(&report).unwrap();
    assert_eq!(report.candidates.len(), 1);
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":path,"request":request(Operation::AnnotateSpectrum{report:Box::new(report),expected_revision:0,annotation:annotation(Confidence::Unknown)})}}}),
    );
    let value = read(&mut output);
    assert_ne!(value["result"]["isError"], true, "{value}");
    assert_eq!(
        value["result"]["structuredContent"]["output"]["report"]["annotations"][0]["actor"],
        "reference-analyst"
    );
    let outside = tempfile::NamedTempFile::new().unwrap();
    send(
        &mut input,
        serde_json::json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"analytical_operation","arguments":{"path":outside.path(),"request":request(Operation::FormulaCandidates{config:formula_config()})}}}),
    );
    let value = read(&mut output);
    assert_eq!(value["result"]["isError"], true);
    drop(input);
    child.kill().unwrap();
    child.wait().unwrap();
}
#[test]
fn project_spectral_replay_and_immutable_history() {
    use chromascope::project::Project;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    let raw = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_file/data_dependent_02.mzML");
    let mut project = Project::create(&root).unwrap();
    let id = project.register(&raw).unwrap();
    let response = engine::execute(
        Path::new("-"),
        request(Operation::SearchSpectralLibrary {
            query: Box::new(query()),
            library: Box::new(import_library(msp(), "msp".into(), source()).unwrap()),
            config: SearchConfig::default(),
        }),
        &JobControl::default(),
    )
    .unwrap();
    project.add_result(&root, id, &response).unwrap();
    project.commit(&root, 0).unwrap();
    assert_eq!(Project::open(&root).unwrap().results.len(), 1);
    let mut bad = response;
    match &mut bad.output {
        Output::SpectralSearch { report } => report.candidates[0].similarity.cosine = 0.5,
        _ => panic!(),
    };
    assert!(project.add_result(&root, id, &bad).is_err());
    assert_eq!(Project::open(&root).unwrap().results.len(), 1);
}
#[test]
fn independent_massbank_dot_product_and_exhaustive_formula_references() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("reference/spectral.json")).unwrap();
    let l = import_library(
        format!(
            "{}{}",
            include_str!("reference/MSBNK-Antwerp_Univ-AN111301.txt"),
            include_str!("reference/MSBNK-Antwerp_Univ-AN111302.txt")
        ),
        "massbank".into(),
        source(),
    )
    .unwrap();
    for case in fixture["massbank_cross_energy"].as_array().unwrap() {
        let cfg = Processing {
            relative_threshold: case["relative_threshold"].as_f64().unwrap(),
            ..Default::default()
        };
        let a = process(vec![l.entries[0].spectrum.clone()], vec![], cfg.clone()).unwrap();
        let b = process(vec![l.entries[1].spectrum.clone()], vec![], cfg).unwrap();
        let score = compare(
            &a.spectrum,
            &b.spectrum,
            &Tolerance {
                value: 0.02,
                unit: MassUnit::Da,
            },
        )
        .unwrap();
        assert!((score.cosine - case["cosine"].as_f64().unwrap()).abs() < 1e-12);
        assert_eq!(
            score.matches.len(),
            case["matches"].as_u64().unwrap() as usize
        );
    }
    let result = formula_candidates(formula_config()).unwrap();
    let cases = fixture["formulas"].as_array().unwrap();
    assert_eq!(result.candidates.len(), cases.len());
    for case in cases {
        let c = result
            .candidates
            .iter()
            .find(|c| {
                c.formula == case["formula"].as_str().unwrap()
                    && c.adduct == case["adduct"].as_str().unwrap()
            })
            .unwrap();
        assert!((c.neutral_mass_da - case["neutral_mass_da"].as_f64().unwrap()).abs() < 1e-10);
        assert!((c.predicted_mz - case["predicted_mz"].as_f64().unwrap()).abs() < 1e-10);
        assert_eq!(c.dbe, case["dbe"].as_f64().unwrap());
    }
}
#[test]
fn adduct_ambiguity_retains_multiple_neutral_formulas() {
    let neutral = 24.0 * 12.0 + 40.0 * 1.00782503223 + 10.0 * 15.99491461957;
    let r = formula_candidates(FormulaConfig {
        observed_mz: neutral + 22.989220702091,
        polarity: Polarity::Positive,
        adducts: vec!["[M+H]+".into(), "[M+Na]+".into()],
        tolerance: Tolerance {
            value: 5.0,
            unit: MassUnit::Ppm,
        },
        maximum_atoms: BTreeMap::from([("C".into(), 26), ("H".into(), 40), ("O".into(), 10)]),
        maximum_candidates: 100,
    })
    .unwrap();
    assert!(r
        .candidates
        .iter()
        .any(|c| c.formula == "C24H40O10" && c.adduct == "[M+Na]+"));
    assert!(r
        .candidates
        .iter()
        .any(|c| c.formula == "C26H38O10" && c.adduct == "[M+H]+"));
}
#[test]
fn greedy_ambiguity_and_dense_pair_cap_are_explicit() {
    let a = spectrum(vec![[50.0, 9.0], [50.02, 10.0]]);
    let b = spectrum(vec![[50.015, 10.0], [50.03, 9.0]]);
    let score = compare(
        &a,
        &b,
        &Tolerance {
            value: 0.02,
            unit: MassUnit::Da,
        },
    )
    .unwrap();
    assert!((score.cosine - 100.0 / 181.0).abs() < 1e-12);
    assert!(score.cosine < 180.0 / 181.0);
    let dense = spectrum((0..1001).map(|i| [50.0 + i as f64 * 1e-6, 1.0]).collect());
    assert_eq!(
        compare(
            &dense,
            &dense,
            &Tolerance {
                value: 0.01,
                unit: MassUnit::Da
            }
        )
        .unwrap_err()
        .code,
        "resource_limit"
    );
}
#[test]
fn cli_candidate_export_is_create_new_and_preserves_confidence() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let response = engine::execute(
        Path::new("-"),
        request(Operation::SearchSpectralLibrary {
            query: Box::new(query()),
            library: Box::new(import_library(msp(), "msp".into(), source()).unwrap()),
            config: SearchConfig::default(),
        }),
        &JobControl::default(),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("candidates.csv");
    let run = || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
            .arg("export-spectral")
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
        child.wait_with_output().unwrap()
    };
    assert!(run().status.success());
    let before = std::fs::read(&path).unwrap();
    let text = String::from_utf8(before.clone()).unwrap();
    assert!(text.contains("fragment_tolerance_unit"));
    assert!(text.contains("Unknown"));
    assert!(text.contains("ref-1"));
    assert!(!run().status.success());
    assert_eq!(std::fs::read(path).unwrap(), before);
}
#[test]
fn json_annotation_roundtrip_and_linked_source_checks() {
    let reviewed = annotate(&report(), 0, annotation(Confidence::Unknown), "analyst").unwrap();
    let req = request(Operation::AnnotateSpectrum {
        report: Box::new(reviewed),
        expected_revision: 1,
        annotation: annotation(Confidence::Unknown),
    });
    let encoded = serde_json::to_vec(&req).unwrap();
    let decoded: Request = serde_json::from_slice(&encoded).unwrap();
    let response = engine::execute(Path::new("-"), decoded, &JobControl::default()).unwrap();
    let saved: engine::Response =
        serde_json::from_slice(&serde_json::to_vec(&response).unwrap()).unwrap();
    if let Output::SpectralSearch { report } = saved.output {
        verify_report(&report).unwrap();
        assert_eq!(report.annotations.len(), 2);
    } else {
        panic!()
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_file/data_dependent_02.mzML");
    let x = chromascope::validation::XicParams::new(
        195.0877,
        mzdata::spectrum::ScanPolarity::Positive,
        10.0,
        &chromascope::validation::DataBounds::unrestricted(),
    )
    .unwrap();
    let params = chromascope::processing::ProcessingParams {
        acquisition: None,
        plot_type: chromascope::plotting_parameters::PlotType::Xic,
        ms_level: 1,
        polarity: mzdata::spectrum::ScanPolarity::Positive,
        smoothing: 0,
        xic_params: Some(x),
        mz_range: None,
        precursor_mz: None,
    };
    assert!(engine::execute(
        &path,
        request(Operation::LinkedSpectralChromatogram {
            query: Box::new(query()),
            params: params.clone()
        }),
        &JobControl::default()
    )
    .is_err());
    let mut original = query().inputs;
    original[0].metadata.insert(
        "source_sha256".into(),
        chromascope::project::source_identity(&path).unwrap().0,
    );
    let linked = process(original, vec![], Processing::default()).unwrap();
    let response = engine::execute(
        &path,
        request(Operation::LinkedSpectralChromatogram {
            query: Box::new(linked),
            params,
        }),
        &JobControl::default(),
    )
    .unwrap();
    assert!(matches!(response.output, Output::Chromatogram { .. }));
}
#[test]
fn nominal_isotope_convolution_matches_binomial_carbon_and_sulfur_reference() {
    let counts = BTreeMap::from([("C".into(), 10)]);
    let p = nominal_pattern(counts.clone(), 4).unwrap();
    for k in 0..=4 {
        let coefficient = (1..=k).fold(1.0, |value, j| value * (10 - j + 1) as f64 / j as f64);
        let expected = coefficient * 0.9893f64.powi(10 - k) * 0.0107f64.powi(k);
        assert!((p.probabilities[k as usize] - expected).abs() < 1e-14);
    }
    let s = nominal_pattern(BTreeMap::from([("S".into(), 1)]), 4).unwrap();
    assert_eq!(s.probabilities, vec![0.9499, 0.0075, 0.0425, 0.0, 0.0001]);
    let mut input = spectrum(
        p.probabilities
            .iter()
            .enumerate()
            .map(|(k, p)| [100.0 + k as f64 * 1.00335483507, p * 1000.0])
            .collect(),
    );
    input.ms_level = 1;
    let r = isotopes(
        input,
        IsotopeConfig {
            expected_atom_counts: Some(counts),
            monoisotopic_mz: 100.0,
            maximum_charge: 1,
            maximum_isotopes: 4,
            tolerance: Tolerance {
                value: 5.0,
                unit: MassUnit::Ppm,
            },
        },
    )
    .unwrap();
    assert!((r.nominal_comparisons[0].cosine - 1.0).abs() < 1e-14);
    assert!(r.nominal_pattern.unwrap().retained_probability < 1.0);
}
