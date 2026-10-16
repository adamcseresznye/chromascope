//! Guards for what the release workflow uploads: every file in `release-assets` is published.
use std::fs;

fn read(path: &str) -> String {
    fs::read_to_string(format!("{}/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .replace("\r\n", "\n")
}

#[test]
fn the_windows_build_leaves_no_debug_file_among_the_release_assets() {
    // The first dry run of the workflow uploaded a `.wixpdb` next to every MSI, and the publish job
    // would have attached it to the release. `-pdbtype none` stops wix from writing it.
    let workflow = read(".github/workflows/release.yml");
    let wix: Vec<&str> = workflow
        .lines()
        .filter(|line| line.trim_start().starts_with("wix build"))
        .collect();
    assert_eq!(wix.len(), 1, "one wix build step");
    assert!(
        wix[0].contains("-pdbtype none"),
        "wix build must not write a .wixpdb: {}",
        wix[0]
    );
}

#[test]
fn the_release_profile_is_fast_and_checks_arithmetic() {
    // Measured on 2026-10-16 (see the commit that set this): opt-level 3 ran the ignored release
    // benchmarks in 0.86 of the time of opt-level "z", for executables 1.45-1.9x the size.
    // Overflow checks turn a silent wrap into a panic that the job runner reports as a failed job
    // (`panic = "unwind"`), which is the safer failure for analytical numbers.
    let manifest: toml::Value = read("Cargo.toml").parse().unwrap();
    let release = &manifest["profile"]["release"];
    assert_eq!(release["opt-level"].as_integer(), Some(3));
    assert_eq!(release["overflow-checks"].as_bool(), Some(true));
    assert_eq!(release["panic"].as_str(), Some("unwind"));
    assert_eq!(release["lto"].as_bool(), Some(true));
}
