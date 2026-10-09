//! Opt-in measured workloads. Synthetic inputs have independent exact expectations.
//! Run with --ignored --nocapture; CHROMASCOPE_BENCH_SCANS controls dataset size.
use base64::Engine as _;
use chromascope::{
    chromatography::{self, Baseline, Config, Trace},
    domain::{Minutes, Operation, Request},
    engine::{self, Output},
    jobs::JobControl,
    parser::MzData,
    project::{source_identity, Project},
    spectral::{self, LibrarySource, Processing, SearchConfig, Spectrum},
    targeted::{self, CalibrationConfig, Intercept, Unit, Weighting},
};
use mzdata::prelude::*;
use mzdata::spectrum::ScanPolarity;
use std::{io::Write, path::Path, time::Instant};
include!("fixtures/targeted_support.rs");

fn request(operation: Operation) -> Request {
    Request {
        version: 1,
        operation_id: Default::default(),
        actor: "performance-reference".into(),
        operation,
    }
}
fn measure<T>(name: &str, units: usize, f: impl FnOnce() -> T) -> T {
    let unix_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let started = Instant::now();
    let result = f();
    println!(
        "BENCH_JSON {}",
        serde_json::json!({"workload":name,"units":units,"started_unix_seconds":unix_seconds,"seconds":started.elapsed().as_secs_f64()})
    );
    result
}
fn large_mzml(path: &Path, scans: usize, peaks: usize) {
    let encode = |v: Vec<f64>| {
        base64::engine::general_purpose::STANDARD
            .encode(v.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>())
    };
    let masses = encode((0..peaks).map(|i| 100. + i as f64).collect());
    let intensities = encode((0..peaks).map(|i| (i + 1) as f64).collect());
    let array = |accession: &str, name: &str, data: &str| {
        format!("<binaryDataArray encodedLength=\"{}\"><cvParam cvRef=\"MS\" accession=\"MS:1000523\" name=\"64-bit float\"/><cvParam cvRef=\"MS\" accession=\"MS:1000576\" name=\"no compression\"/><cvParam cvRef=\"MS\" accession=\"{accession}\" name=\"{name}\"/><binary>{data}</binary></binaryDataArray>",data.len())
    };
    let arrays = format!(
        "{}{}",
        array("MS:1000514", "m/z array", &masses),
        array("MS:1000515", "intensity array", &intensities)
    );
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    write!(f,"<?xml version=\"1.0\"?><mzML xmlns=\"http://psi.hupo.org/ms/mzml\" version=\"1.1.0\"><cvList count=\"0\"/><fileDescription><fileContent/></fileDescription><softwareList count=\"0\"/><instrumentConfigurationList count=\"1\"><instrumentConfiguration id=\"IC1\"/></instrumentConfigurationList><dataProcessingList count=\"1\"><dataProcessing id=\"DP1\"/></dataProcessingList><run id=\"synthetic\" defaultInstrumentConfigurationRef=\"IC1\"><spectrumList count=\"{scans}\" defaultDataProcessingRef=\"DP1\">").unwrap();
    for i in 0..scans {
        write!(f,"<spectrum index=\"{i}\" id=\"scan={}\" defaultArrayLength=\"{peaks}\"><cvParam cvRef=\"MS\" accession=\"MS:1000511\" name=\"ms level\" value=\"1\"/><cvParam cvRef=\"MS\" accession=\"MS:1000130\" name=\"positive scan\"/><cvParam cvRef=\"MS\" accession=\"MS:1000127\" name=\"centroid spectrum\"/><scanList count=\"1\"><scan><cvParam cvRef=\"MS\" accession=\"MS:1000016\" name=\"scan start time\" value=\"{}\" unitCvRef=\"UO\" unitAccession=\"UO:0000031\" unitName=\"minute\"/><scanWindowList count=\"1\"><scanWindow><cvParam cvRef=\"MS\" accession=\"MS:1000501\" name=\"scan window lower limit\" value=\"100\"/><cvParam cvRef=\"MS\" accession=\"MS:1000500\" name=\"scan window upper limit\" value=\"{}\"/></scanWindow></scanWindowList></scan></scanList><binaryDataArrayList count=\"2\">{arrays}</binaryDataArrayList></spectrum>",i+1,i as f64/100.,100+peaks).unwrap();
    }
    write!(f, "</spectrumList></run></mzML>").unwrap();
    f.flush().unwrap();
}

#[test]
#[ignore = "opt-in performance measurements; creates a large synthetic mzML in a temporary directory"]
fn analytical_workloads() {
    let scans = std::env::var("CHROMASCOPE_BENCH_SCANS")
        .unwrap_or_else(|_| "1000".into())
        .parse::<usize>()
        .unwrap();
    assert!((2..=100_000).contains(&scans));
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("large.mzML");
    large_mzml(&path, scans, 1024);
    println!(
        "BENCH_JSON {}",
        serde_json::json!({"workload":"dataset","scans":scans,"peaks_per_scan":1024,"bytes":std::fs::metadata(&path).unwrap().len(),"synthetic":true,"build":"test profile; dependencies opt-level=2, core unoptimized"})
    );
    measure("source_sha256", scans, || source_identity(&path).unwrap());
    for level in [
        mzdata::io::DetailLevel::Full,
        mzdata::io::DetailLevel::Lazy,
        mzdata::io::DetailLevel::MetadataOnly,
    ] {
        let mut input = mzdata::MZReader::open_path(&path).unwrap();
        input.set_detail_level(level);
        measure(&format!("reader_iteration_{level:?}"), scans, || {
            assert_eq!(input.iter().count(), scans)
        });
    }
    let mut reader = measure("open_bounds", scans, || {
        let mut r = MzData::new();
        r.open_msfile(&path).unwrap();
        r
    });
    for repeat in 0..3 {
        let xic = measure(&format!("xic_repeat_{repeat}"), scans, || {
            reader
                .get_xic(250., 1, ScanPolarity::Positive, 10., None)
                .unwrap()
        });
        assert_eq!(xic.intensity, vec![151.; scans]);
        assert_eq!(xic.index, (0..scans).collect::<Vec<_>>());
    }
    let tic = measure("tic_missing_metadata", scans, || {
        reader
            .get_tic(1, ScanPolarity::Positive, None, None)
            .unwrap()
    });
    assert_eq!(tic.intensity, vec![(1024 * 1025 / 2) as f32; scans]);
    let bpc = measure("bpc_missing_metadata", scans, || {
        reader
            .get_bpic(1, ScanPolarity::Positive, None, None)
            .unwrap()
    });
    assert_eq!(bpc.intensity, vec![1024.; scans]);
    let points = (0..10001)
        .map(|i| {
            let x = i as f64 / 1000.;
            [x, 2. + 3. * x + 10. * (1. - (x - 5.).abs()).max(0.)]
        })
        .collect::<Vec<_>>();
    measure("batch_integration_1000", 1000, || {
        for _ in 0..1000 {
            let r = engine::execute(
                Path::new("-"),
                request(Operation::Integrate {
                    points: points.clone(),
                    start: Minutes(4.),
                    end: Minutes(6.),
                }),
                &JobControl::default(),
            )
            .unwrap();
            let Output::Integration { area, .. } = r.output else {
                panic!("integration output")
            };
            assert!((area.0 - 10.).abs() < 1e-9);
        }
    });
    measure("chromatography_sg_chord", points.len(), || {
        let a = chromatography::process(
            Trace::from_points("triangle".into(), &points),
            Config {
                baseline: Baseline::EndpointChord,
                minimum_snr: 0.,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!a.peaks().is_empty());
        chromatography::verify(&a).unwrap();
    });
    let cfg = CalibrationConfig {
        degree: 1,
        intercept: Intercept::Free,
        weighting: Weighting::InverseX2,
        unit: Unit::NgMl,
        range: [1., 100.],
        lod: 1.,
        loq: 1.,
        accuracy_tolerance_percent: 15.,
        qc_cv_limit_percent: 15.,
        blank_response_limit: 0.,
    };
    let standards = (1..=100)
        .map(|i| (i.to_string(), i as f64, 3. + 2. * i as f64))
        .collect::<Vec<_>>();
    measure("calibration_1000_fits", 1000, || {
        for _ in 0..1000 {
            let c = targeted::fit(&standards, &cfg).unwrap();
            assert!((c.response(50.) - 103.).abs() < 1e-9);
        }
    });
    let mut batch = raw_request(temp.path());
    let original = batch.samples.clone();
    for repeat in 1..8 {
        for s in &original {
            let mut s = s.clone();
            s.id = format!("{}-{repeat}", s.id);
            s.injection_order += repeat * 8;
            batch.samples.push(s);
        }
    }
    let result = measure("targeted_64_samples_calibration", 64, || {
        targeted::run(batch, &JobControl::default()).unwrap()
    });
    targeted::verify(&result).unwrap();
    let spectrum: Spectrum = serde_json::from_value(
        serde_json::from_str::<serde_json::Value>(include_str!(
            "../examples/spectral-request.json"
        ))
        .unwrap()["operation"]["spectra"][0]
            .clone(),
    )
    .unwrap();
    let query = spectral::process(vec![spectrum], vec![], Processing::default()).unwrap();
    let mut msp = String::new();
    for i in 0..10000 {
        msp.push_str(&format!("Name: synthetic-{i}\nDB#: ref-{i}\nPrecursorMZ: {}\nPrecursorType: [M+H]+\nIonMode: positive\nCollisionEnergy: 20 eV\nInstrumentType: LC-ESI-QTOF\nNum Peaks: 3\n50 10\n100 20\n150 30\n\n",195.0877+i as f64*0.01));
    }
    let library = measure("library_import_10000", 10000, || {
        spectral::import_library(
            msp,
            "msp".into(),
            LibrarySource {
                name: "authored synthetic".into(),
                version: "1".into(),
                url: "local:synthetic".into(),
                license: "CC0-1.0".into(),
            },
        )
        .unwrap()
    });
    measure("spectral_search_10000", 10000, || {
        let r = spectral::search(query, library, SearchConfig::default()).unwrap();
        assert!((r.candidates[0].similarity.cosine - 1.).abs() < 1e-12);
    });
    let root = temp.path().join("project");
    let mut p = Project::create(&root).unwrap();
    measure("project_register_large_source", scans, || {
        p.register(&path).unwrap()
    });
    measure("project_100_commits", 100, || {
        for revision in 0..100 {
            p.commit(&root, revision).unwrap();
        }
    });
    measure("project_open_100_revisions", 100, || {
        assert_eq!(Project::open(&root).unwrap().revision, 100)
    });
    measure("project_restore_revision", 1, || {
        assert_eq!(
            Project::restore_revision(&root, 10, 100).unwrap().revision,
            101
        )
    });
    measure("concurrent_xic_2_readers", scans * 2, || {
        std::thread::scope(|scope| {
            let handles = (0..2)
                .map(|_| {
                    scope.spawn(|| {
                        let mut r = MzData::new();
                        r.open_msfile(&path).unwrap();
                        let x = r
                            .get_xic(250., 1, ScanPolarity::Positive, 10., None)
                            .unwrap();
                        assert_eq!(x.intensity, vec![151.; scans]);
                    })
                })
                .collect::<Vec<_>>();
            for h in handles {
                h.join().unwrap();
            }
        })
    });
    let scheduler = chromascope::jobs::Scheduler::new(1, 1).unwrap();
    let control = JobControl::default();
    let params = chromascope::processing::ProcessingParams {
        acquisition: None,
        plot_type: chromascope::plotting_parameters::PlotType::Xic,
        ms_level: 1,
        polarity: ScanPolarity::Positive,
        smoothing: 0,
        xic_params: Some(
            chromascope::validation::XicParams::new(
                250.,
                ScanPolarity::Positive,
                10.,
                &chromascope::validation::DataBounds::unrestricted(),
            )
            .unwrap(),
        ),
        mz_range: None,
        precursor_mz: None,
    };
    let job = scheduler
        .submit(
            path,
            request(Operation::Extract { params }),
            control.clone(),
        )
        .unwrap();
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    while control.completed_scans() == 0 {
        assert!(Instant::now() < deadline, "worker did not enter extraction");
        assert!(!matches!(
            job.state(),
            chromascope::jobs::JobState::Succeeded | chromascope::jobs::JobState::Failed
        ));
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    measure("running_xic_cancellation_latency", 1, || {
        job.cancel();
        assert_eq!(job.wait().err().unwrap().code, "cancelled");
    });
}

#[test]
#[ignore = "requires existing pyOpenMS 3.5.0; cold/warm feature detection and affine alignment benchmark"]
fn openms_workloads() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("gaussian");
    let generated = std::process::Command::new(
        std::env::var_os("CHROMASCOPE_OPENMS_PYTHON").unwrap_or_else(|| "python".into()),
    )
    .arg("tests/reference/generate_untargeted.py")
    .arg(&root)
    .output()
    .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let request: Request =
        serde_json::from_slice(&std::fs::read(root.join("request.json")).unwrap()).unwrap();
    let cold = measure("openms_detect_align_cold_6_samples", 6, || {
        engine::execute(Path::new("-"), request.clone(), &JobControl::default()).unwrap()
    });
    let warm = measure("openms_checkpoint_reuse_align_6_samples", 6, || {
        engine::execute(Path::new("-"), request, &JobControl::default()).unwrap()
    });
    let (Output::FeatureMatrix { report: cold }, Output::FeatureMatrix { report: warm }) =
        (cold.output, warm.output)
    else {
        panic!("feature outputs")
    };
    chromascope::untargeted::verify(&cold).unwrap();
    chromascope::untargeted::verify(&warm).unwrap();
    assert_eq!(cold.features.len(), 8);
    assert_eq!(
        serde_json::to_value(&cold.features).unwrap(),
        serde_json::to_value(&warm.features).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&cold.alignments).unwrap(),
        serde_json::to_value(&warm.alignments).unwrap()
    );
}
