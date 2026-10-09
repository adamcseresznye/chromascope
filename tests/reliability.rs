//! Adversarial raw-file and transactional recovery checks; no user data is modified.
use base64::Engine as _;
use chromascope::{
    domain::{Operation, Request},
    engine,
    jobs::JobControl,
    parser::MzData,
    project::Project,
};
use mzdata::spectrum::ScanPolarity;
use std::path::Path;
include!("fixtures/targeted_support.rs");

fn request(operation: Operation) -> Request {
    Request {
        version: 1,
        operation_id: Default::default(),
        actor: "reliability-reference".into(),
        operation,
    }
}
fn mixed_source(root: &Path) -> std::path::PathBuf {
    let batch = raw_request(root);
    let path = std::path::PathBuf::from(&batch.samples[0].source);
    let text = std::fs::read_to_string(&path).unwrap();
    // A valid metadata value differs from raw arrays and must remain authoritative.
    let marker = "<scanList count=\"1\">";
    let text=text.replacen(marker,"<cvParam cvRef=\"MS\" accession=\"MS:1000285\" name=\"total ion current\" value=\"123\"/><cvParam cvRef=\"MS\" accession=\"MS:1000505\" name=\"base peak intensity\" value=\"55\"/><cvParam cvRef=\"MS\" accession=\"MS:1000504\" name=\"base peak m/z\" value=\"123\"/><scanList count=\"1\">",1);
    std::fs::write(&path, text).unwrap();
    path
}
#[test]
fn mixed_missing_metadata_is_recovered_without_replacing_supplied_values() {
    let temp = tempfile::tempdir().unwrap();
    let path = mixed_source(temp.path());
    let mut reader = MzData::new();
    reader.open_msfile(&path).unwrap();
    let tic = reader
        .get_tic(1, ScanPolarity::Positive, None, None)
        .unwrap();
    assert_eq!(tic.intensity, vec![123., 0., 230., 0., 0.]);
    let bpc = reader
        .get_bpic(1, ScanPolarity::Positive, None, None)
        .unwrap();
    assert_eq!(bpc.intensity, vec![55., 0., 200., 0., 0.]);
    assert_eq!(bpc.mz[0], 123.);
    assert_eq!(bpc.mz[2], 300.);
    assert_eq!(tic.index, vec![0, 1, 2, 3, 4]);
    assert_eq!(bpc.index, tic.index);
}
#[test]
fn unsorted_acquisition_has_actual_rt_extrema_and_preserved_scan_mapping() {
    let temp = tempfile::tempdir().unwrap();
    let path = mixed_source(temp.path());
    let text = std::fs::read_to_string(&path).unwrap().replace(
        "name=\"scan start time\" value=\"1\"",
        "name=\"scan start time\" value=\"9\"",
    );
    std::fs::write(&path, text).unwrap();
    let mut reader = MzData::new();
    reader.open_msfile(&path).unwrap();
    assert_eq!(reader.bounds.min_rt, 0.);
    assert_eq!(reader.bounds.max_rt, 9.);
    let trace = reader
        .get_tic(1, ScanPolarity::Positive, None, None)
        .unwrap();
    assert_eq!(trace.index, vec![0, 1, 3, 4, 2]);
    assert_eq!(trace.retention_time.last(), Some(&9.));
}
#[test]
fn malformed_arrays_are_errors_in_every_extraction_path_and_reader_recovers() {
    let temp = tempfile::tempdir().unwrap();
    let path = mixed_source(temp.path());
    let encode = |v: &[f64]| {
        base64::engine::general_purpose::STANDARD
            .encode(v.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>())
    };
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace(&encode(&[20., 10., 200.]), &encode(&[20., 10.]));
    std::fs::write(&path, text).unwrap();
    let mut reader = MzData::new();
    assert!(reader.open_msfile(&path).is_err());
    reader.open_reader_only(&path).unwrap();
    for range in [None, Some((100., 350.))] {
        assert!(reader
            .get_tic(1, ScanPolarity::Positive, range, None)
            .is_err());
        assert!(reader
            .get_bpic(1, ScanPolarity::Positive, range, None)
            .is_err());
    }
    assert!(reader
        .get_xic(100., 1, ScanPolarity::Positive, 10., None)
        .is_err());
    // Decoder detail must be restored even when a preceding request failed.
    assert_eq!(reader.get_mass_spectrum_by_index(0).unwrap().mz.len(), 3);
}

#[test]
fn raw_time_units_convert_explicitly_and_unknown_missing_invalid_values_fail() {
    let temp = tempfile::tempdir().unwrap();
    let batch = raw_request(temp.path());
    let original = std::fs::read_to_string(&batch.samples[0].source).unwrap();
    for (unit, name, scale) in [
        ("UO:0000031", "minute", 1.),
        ("UO:0000010", "second", 60.),
        ("UO:0000028", "millisecond", 60000.),
    ] {
        let mut text = original.clone();
        for time in [0., 0.5, 1., 1.5, 2.] {
            text = text.replace(
                &format!("name=\"scan start time\" value=\"{time}\""),
                &format!("name=\"scan start time\" value=\"{}\"", time * scale),
            );
        }
        text = text.replace(
            "unitAccession=\"UO:0000031\" unitName=\"minute\"",
            &format!("unitAccession=\"{unit}\" unitName=\"{name}\""),
        );
        let path = temp.path().join(format!("{name}.mzML"));
        std::fs::write(&path, text).unwrap();
        let mut reader = MzData::new();
        reader.open_msfile(&path).unwrap();
        assert_eq!(reader.bounds.max_rt, 2.);
        assert_eq!(
            reader
                .get_tic(1, ScanPolarity::Positive, None, None)
                .unwrap()
                .retention_time,
            vec![0., 0.5, 1., 1.5, 2.]
        );
    }
    for (label, text) in [
        ("unknown", original.replace("UO:0000031", "UO:9999999")),
        (
            "missing_unit",
            original.replace(
                " unitCvRef=\"UO\" unitAccession=\"UO:0000031\" unitName=\"minute\"",
                "",
            ),
        ),
        (
            "not_numeric",
            original.replace(
                "name=\"scan start time\" value=\"1\"",
                "name=\"scan start time\" value=\"invalid\"",
            ),
        ),
        (
            "missing_time",
            original
                .replace("MS:1000016", "MS:9999999")
                .replace("scan start time", "unrelated metadata"),
        ),
        (
            "negative",
            original.replace(
                "name=\"scan start time\" value=\"1\"",
                "name=\"scan start time\" value=\"-1\"",
            ),
        ),
        (
            "truncated",
            original.trim_end_matches("</mzML>").to_string(),
        ),
        (
            "wrong_count",
            original.replace("spectrumList count=\"5\"", "spectrumList count=\"6\""),
        ),
    ] {
        let path = temp.path().join(format!("{label}.mzML"));
        std::fs::write(&path, text).unwrap();
        assert!(
            MzData::new().open_msfile(&path).is_err(),
            "accepted {label}"
        );
    }
}

#[test]
fn complete_no_window_bounds_include_interior_masses_and_gzip_validation() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let batch = raw_request(temp.path());
    let original = std::fs::read_to_string(&batch.samples[0].source).unwrap();
    let encode = |v: &[f64]| {
        base64::engine::general_purpose::STANDARD
            .encode(v.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>())
    };
    let common = encode(&[100., 200., 300.]);
    let interior = encode(&[10., 200., 900.]);
    let position = original.find("id=\"scan=3\"").unwrap();
    let mut text = original[..position].to_string();
    text.push_str(&original[position..].replacen(&common, &interior, 1));
    let path = temp.path().join("interior.mzML");
    std::fs::write(&path, &text).unwrap();
    let mut reader = MzData::new();
    reader.open_msfile(&path).unwrap();
    assert_eq!(reader.bounds.min_mz, 10.);
    assert_eq!(reader.bounds.max_mz, 900.);
    let mixed = temp.path().join("mixed-windows.mzML");
    let window="<scanWindowList count=\"1\"><scanWindow><cvParam cvRef=\"MS\" accession=\"MS:1000501\" name=\"scan window lower limit\" value=\"100\"/><cvParam cvRef=\"MS\" accession=\"MS:1000500\" name=\"scan window upper limit\" value=\"350\"/></scanWindow></scanWindowList></scan>";
    std::fs::write(&mixed, text.replacen("</scan>", window, 1)).unwrap();
    let mut mixed_reader = MzData::new();
    mixed_reader.open_msfile(&mixed).unwrap();
    assert_eq!(mixed_reader.bounds.min_mz, 10.);
    assert_eq!(mixed_reader.bounds.max_mz, 900.);
    let gzip = temp.path().join("interior.mzML.gz");
    let mut output = flate2::write::GzEncoder::new(
        std::fs::File::create(&gzip).unwrap(),
        flate2::Compression::default(),
    );
    output.write_all(text.as_bytes()).unwrap();
    output.finish().unwrap();
    let mut compressed = MzData::new();
    compressed.open_msfile(&gzip).unwrap();
    assert_eq!(compressed.bounds.min_mz, 10.);
    assert_eq!(compressed.bounds.max_mz, 900.);
    let invalid = temp.path().join("invalid.mzML.gz");
    let mut output = flate2::write::GzEncoder::new(
        std::fs::File::create(&invalid).unwrap(),
        flate2::Compression::default(),
    );
    output
        .write_all(text.replace("UO:0000031", "UO:9999999").as_bytes())
        .unwrap();
    output.finish().unwrap();
    assert!(MzData::new().open_msfile(&invalid).is_err());
}
#[test]
fn project_interrupted_temporary_write_and_abandoned_lock_preserve_history() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let mut p = Project::create(&root).unwrap();
    p.commit(&root, 0).unwrap();
    let original = std::fs::read(root.join("revision-00000000000000000001.json")).unwrap();
    std::fs::write(root.join("partial-save.tmp"), b"{truncated").unwrap();
    std::fs::write(root.join("writer.lock"), b"interrupted writer").unwrap();
    assert_eq!(Project::open(&root).unwrap().revision, 1);
    assert_eq!(p.commit(&root, 1).unwrap_err().code, "stale_revision");
    assert_eq!(p.revision, 1);
    assert_eq!(
        std::fs::read(root.join("revision-00000000000000000001.json")).unwrap(),
        original
    );
    // Only remove the lock deliberately authored by this test, not an app lock.
    std::fs::remove_file(root.join("writer.lock")).unwrap();
    p.commit(&root, 1).unwrap();
    let restored = Project::restore_revision(&root, 1, 2).unwrap();
    assert_eq!(restored.revision, 3);
    assert_eq!(
        std::fs::read(root.join("revision-00000000000000000001.json")).unwrap(),
        original
    );
}
#[test]
fn malformed_file_and_cancelled_work_have_explicit_errors() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("broken.mzML");
    std::fs::write(&path, b"this is not XML").unwrap();
    assert!(engine::execute(&path, request(Operation::Metadata), &JobControl::default()).is_err());
    let control = JobControl::default();
    control.cancel();
    assert_eq!(
        engine::execute(&path, request(Operation::Metadata), &control)
            .err()
            .unwrap()
            .code,
        "cancelled"
    );
}

#[test]
fn concurrent_project_writers_admit_one_revision_and_keep_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    Project::create(&root).unwrap();
    let original = std::fs::read(root.join("revision-00000000000000000000.json")).unwrap();
    let barrier = std::sync::Barrier::new(8);
    let outcomes = std::thread::scope(|scope| {
        let handles = (0..8)
            .map(|_| {
                let root = &root;
                let barrier = &barrier;
                scope.spawn(move || {
                    let mut p = Project::open(root).unwrap();
                    barrier.wait();
                    p.commit(root, 0)
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    for error in outcomes.iter().filter_map(|r| r.as_ref().err()) {
        assert_eq!(error.code, "stale_revision");
    }
    assert_eq!(Project::open(&root).unwrap().revision, 1);
    assert_eq!(
        std::fs::read(root.join("revision-00000000000000000000.json")).unwrap(),
        original
    );
    assert!(!root.join("writer.lock").exists());
}

#[test]
fn repaired_traces_are_equivalent_in_gui_compatibility_engine_and_cli() {
    use chromascope::{
        plotting_parameters::PlotType,
        processing::{self, ProcessingParams},
    };
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let temp = tempfile::tempdir().unwrap();
    let path = mixed_source(temp.path());
    for kind in [PlotType::Tic, PlotType::Bpc] {
        let params = ProcessingParams {
            acquisition: None,
            plot_type: kind,
            ms_level: 1,
            polarity: ScanPolarity::Positive,
            smoothing: 0,
            xic_params: None,
            mz_range: None,
            precursor_mz: None,
        };
        let mut reader = MzData::new();
        reader.open_msfile(&path).unwrap();
        let (expected, raw) = processing::process_chromatogram(&mut reader, &params).unwrap();
        let req = request(Operation::Extract { params });
        let result = engine::execute(&path, req.clone(), &JobControl::default()).unwrap();
        let engine::Output::Chromatogram {
            points,
            raw: engine_raw,
            ..
        } = result.output
        else {
            panic!("trace output")
        };
        assert_eq!(points, expected);
        assert_eq!(engine_raw.intensity, raw.intensity);
        assert_eq!(engine_raw.index, raw.index);
        let mut child = Command::new(env!("CARGO_BIN_EXE_chromascope-cli"))
            .arg("run")
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let actual: Vec<[f64; 2]> =
            serde_json::from_value(value["result"]["output"]["points"].clone()).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(
            value["result"]["request"],
            serde_json::to_value(&req).unwrap()
        );
    }
}

#[test]
fn declared_empty_scans_retain_positions_and_missing_arrays_in_nonempty_scans_fail() {
    let temp = tempfile::tempdir().unwrap();
    let batch = raw_request(temp.path());
    let original = std::fs::read_to_string(&batch.samples[0].source).unwrap();
    let lo = original.find("<binaryDataArrayList").unwrap();
    let hi = lo
        + original[lo..].find("</binaryDataArrayList>").unwrap()
        + "</binaryDataArrayList>".len();
    let mut text = original[..lo].to_string();
    text.push_str("<binaryDataArrayList count=\"0\"/>");
    text.push_str(&original[hi..]);
    let bad = temp.path().join("missing-nonempty-arrays.mzML");
    std::fs::write(&bad, &text).unwrap();
    assert!(MzData::new().open_msfile(&bad).is_err());
    text = text.replacen("defaultArrayLength=\"3\"", "defaultArrayLength=\"0\"", 1);
    let path = temp.path().join("empty-scan.mzML");
    std::fs::write(&path, text).unwrap();
    let mut reader = MzData::new();
    reader.open_msfile(&path).unwrap();
    let tic = reader
        .get_tic(1, ScanPolarity::Positive, None, None)
        .unwrap();
    assert_eq!(tic.intensity, vec![0., 0., 230., 0., 0.]);
    assert_eq!(tic.index, vec![0, 1, 2, 3, 4]);
    let xic = reader
        .get_xic(100., 1, ScanPolarity::Positive, 10., None)
        .unwrap();
    assert_eq!(xic.intensity, vec![0., 0., 20., 0., 0.]);
    assert_eq!(xic.index, tic.index);
    let scan = reader.get_mass_spectrum_by_index(0).unwrap();
    assert!(scan.mz.is_empty());
    assert!(scan.intensity.is_empty());
    let result = engine::execute(
        &path,
        request(Operation::Spectrum { index: 0 }),
        &JobControl::default(),
    )
    .unwrap();
    let engine::Output::Spectrum { state, .. } = result.output else {
        panic!("spectrum output")
    };
    assert_eq!(state, chromascope::domain::DataState::Missing);
}

#[test]
fn untargeted_rejects_unknown_raw_time_units_before_native_processing() {
    let temp = tempfile::tempdir().unwrap();
    let batch = raw_request(temp.path());
    let source = &batch.samples[0].source;
    let text = std::fs::read_to_string(source)
        .unwrap()
        .replace("UO:0000031", "UO:9999999");
    std::fs::write(source, text).unwrap();
    let config = chromascope::untargeted::Config {
        samples: vec![
            chromascope::untargeted::Sample {
                id: "invalid-unit".into(),
                source: source.clone(),
                role: chromascope::untargeted::Role::Sample,
                metadata: serde_json::json!({"synthetic":true}),
            },
            chromascope::untargeted::Sample {
                id: "valid-reference".into(),
                source: batch.samples[1].source.clone(),
                role: chromascope::untargeted::Role::Sample,
                metadata: serde_json::json!({"synthetic":true}),
            },
        ],
        cache_directory: temp.path().join("checkpoints").to_string_lossy().into(),
        ..Default::default()
    };
    let error = engine::execute(
        Path::new("-"),
        request(Operation::UntargetedBatch { config }),
        &JobControl::default(),
    )
    .err()
    .expect("Invalid source units must fail before the external adapter runs");
    assert_eq!(error.code, "adapter_failure");
    assert!(
        error
            .message
            .contains("supported minute/second/millisecond unit"),
        "{error}"
    );
}
