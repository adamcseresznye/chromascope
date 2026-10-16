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
