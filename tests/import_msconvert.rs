use chromascope::{import::prepare_input, processing::FileLoadingResult};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Run against local vendor data without checking proprietary datasets into Git.
#[test]
#[ignore = "requires ProteoWizard and CHROMASCOPE_TEST_VENDOR_FILE"]
fn real_msconvert_targeted_vendor_acquisition_has_spectra_and_xic() {
    let executable = chromascope::import::discover_msconvert().expect("configure msconvert");
    let source = PathBuf::from(
        std::env::var_os("CHROMASCOPE_TEST_VENDOR_FILE").expect("set vendor file path"),
    );
    let result = chromascope::processing::import_file_in_background(
        source,
        Some(executable),
        Arc::new(AtomicBool::new(false)),
        0,
    );
    let FileLoadingResult::Success {
        runs, workspace, ..
    } = result
    else {
        panic!("{result:?}");
    };
    assert!(!runs.is_empty());
    for run in runs {
        assert!(run.bounds.scan_count > 0);
        let mut data = chromascope::MzData::new();
        data.open_msfile(&PathBuf::from(&run.path)).unwrap();
        let (ms, polarity, precursor, _, _) = run.scan_filters[0];
        let tic = data.get_tic(ms, polarity, None, precursor).unwrap();
        assert!(!tic.index.is_empty());
        assert!(tic.intensity.iter().any(|&i| i > 0.0));
        let spectrum = data.get_mass_spectrum_by_index(tic.index[0]).unwrap();
        assert!(!spectrum.mz.is_empty());
        let xic = data
            .get_xic(spectrum.mz[0], ms, polarity, 10.0, precursor)
            .unwrap();
        assert_eq!(xic.index, tic.index);
        assert!(xic.intensity.iter().any(|&i| i > 0.0));
        println!(
            "{}: {} spectra, {} TIC points, {} XIC points",
            run.name,
            run.bounds.scan_count,
            tic.index.len(),
            xic.index.len()
        );
    }
    drop(workspace);
}

#[test]
fn profile_arrays_support_xic_and_range_bpc_without_peak_picking() {
    let fixture = tempfile::tempdir().unwrap();
    let profile = fixture.path().join("profile.mzML");
    let xml = std::fs::read_to_string("test_file/data_dependent_02.mzML")
        .unwrap()
        .replace("MS:1000127", "MS:1000128")
        .replace("centroid spectrum", "profile spectrum");
    std::fs::write(&profile, xml).unwrap();
    let mut data = chromascope::MzData::new();
    data.open_msfile(&profile).unwrap();
    use mzdata::spectrum::ScanPolarity;
    let chrom = data
        .get_xic(722.43, 1, ScanPolarity::Positive, 1000.0, None)
        .unwrap();
    assert!(!chrom.index.is_empty());
    let tolerance = 722.43 * 1000.0 / 1_000_000.0;
    for (&index, &intensity) in chrom.index.iter().zip(&chrom.intensity) {
        let spectrum = data.get_mass_spectrum_by_index(index).unwrap();
        let expected: f32 = spectrum
            .mz
            .iter()
            .zip(&spectrum.intensity)
            .filter(|(mz, _)| **mz >= 722.43 - tolerance && **mz <= 722.43 + tolerance)
            .map(|(_, &i)| i)
            .sum();
        assert_eq!(intensity, expected);
    }
    let bpc = data
        .get_bpic(1, ScanPolarity::Positive, Some((100.0, 2000.0)), None)
        .unwrap();
    assert!(bpc.intensity.iter().any(|&i| i > 0.0));
    for (&index, &intensity) in bpc.index.iter().zip(&bpc.intensity) {
        let spectrum = data.get_mass_spectrum_by_index(index).unwrap();
        let expected = spectrum
            .mz
            .iter()
            .zip(&spectrum.intensity)
            .filter(|(mz, _)| **mz >= 100.0 && **mz <= 2000.0)
            .map(|(_, &i)| i)
            .max_by(f32::total_cmp)
            .unwrap_or(0.0);
        assert_eq!(intensity, expected);
    }
}

#[test]
fn converter_process_multi_run_failure_cancellation_and_cleanup() {
    let fixture = tempfile::tempdir().unwrap();
    let executable = fixture.path().join(if cfg!(windows) {
        "msconvert.exe"
    } else {
        "msconvert"
    });
    let compilation = Command::new("rustc")
        .args(["--edition=2021", "tests/fixtures/fake_msconvert.rs", "-o"])
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        compilation.status.success(),
        "{}",
        String::from_utf8_lossy(&compilation.stderr)
    );
    let source = fixture.path().join("vendor dataset with spaces.d");
    std::fs::create_dir(&source).unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let result = chromascope::processing::import_file_in_background(
        source,
        Some(executable.clone()),
        cancelled.clone(),
        42,
    );
    match result {
        FileLoadingResult::Success {
            file_id,
            runs,
            workspace,
        } => {
            assert_eq!(file_id, 42);
            assert_eq!(runs.len(), 2);
            assert!(runs.iter().all(|r| r.bounds.scan_count > 0));
            let workspace = workspace.unwrap();
            let directory = workspace.path().to_path_buf();
            assert!(Path::new(&runs[0].path).is_file());
            drop(workspace);
            assert!(!directory.exists());
        }
        error => panic!("{error:?}"),
    }
    for (name, expected) in [
        ("fail.raw", "fixture vendor reader failed"),
        ("empty.raw", "no mzML runs"),
    ] {
        let source = fixture.path().join(name);
        std::fs::write(&source, b"vendor input").unwrap();
        let error = prepare_input(&source, Some(&executable), &cancelled).unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
    let source = fixture.path().join("sleep.raw");
    std::fs::write(&source, b"vendor input").unwrap();
    let ready = source.with_extension("ready");
    let flag = cancelled.clone();
    let worker = std::thread::spawn(move || prepare_input(&source, Some(&executable), &flag));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(ready.exists(), "fixture did not start");
    let started = Instant::now();
    cancelled.store(true, Ordering::Relaxed);
    assert!(worker.join().unwrap().unwrap_err().contains("cancelled"));
    assert!(started.elapsed() < Duration::from_secs(3));
}

/// Optional real backend test; vendor data is intentionally not required by CI.
#[test]
#[ignore = "requires an installed ProteoWizard msconvert"]
fn real_msconvert_roundtrip_preserves_tic_and_scan_mapping() {
    let executable = chromascope::import::discover_msconvert().expect("configure msconvert");
    let staging = tempfile::tempdir().unwrap();
    let output = Command::new(&executable)
        .arg("test_file/data_dependent_02.mzML")
        .arg("--mzXML")
        .arg("--outdir")
        .arg(staging.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source = staging.path().join("data_dependent_02.mzXML");
    let converted = prepare_input(&source, Some(&executable), &AtomicBool::new(false)).unwrap();
    assert_eq!(converted.paths.len(), 1);
    let mut original = chromascope::MzData::new();
    original
        .open_msfile(&PathBuf::from("test_file/data_dependent_02.mzML"))
        .unwrap();
    let mut imported = chromascope::MzData::new();
    imported.open_msfile(&converted.paths[0]).unwrap();
    use mzdata::spectrum::ScanPolarity;
    let a = original
        .get_tic(1, ScanPolarity::Positive, None, None)
        .unwrap();
    let b = imported
        .get_tic(1, ScanPolarity::Positive, None, None)
        .unwrap();
    assert_eq!(a.index, b.index);
    assert_eq!(a.retention_time.len(), b.retention_time.len());
    for ((&rt_a, &rt_b), (&i_a, &i_b)) in a
        .retention_time
        .iter()
        .zip(&b.retention_time)
        .zip(a.intensity.iter().zip(&b.intensity))
    {
        assert!((rt_a - rt_b).abs() < 1e-5);
        assert!((i_a - i_b).abs() <= i_a.abs() * 1e-5 + 1e-3);
    }
    for (&index, &rt) in b.index.iter().zip(&b.retention_time) {
        assert_eq!(
            imported
                .get_mass_spectrum_by_index(index)
                .unwrap()
                .retention_time,
            rt
        );
    }
}
