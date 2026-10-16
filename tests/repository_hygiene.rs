//! Guards for what the repository tracks and ignores: data files, personal data, package metadata.
use std::{fs, path::Path, process::Command};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> String {
    fs::read_to_string(root().join(path))
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .replace("\r\n", "\n")
}

/// `Some(true)` when Git ignores `path`, `None` when Git is unavailable.
fn ignored(path: &str) -> Option<bool> {
    let status = Command::new("git")
        .args(["check-ignore", "-q", "--no-index", path])
        .current_dir(root())
        .status()
        .ok()?;
    match status.code()? {
        0 => Some(true),
        1 => Some(false),
        _ => None,
    }
}

#[test]
fn gitignore_keeps_mass_spectrometry_data_out_of_git_at_any_depth() {
    let text = read(".gitignore");
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    // A bare ".mzML" only matches a file with exactly that name; these match every file.
    for pattern in ["*.mzML", "*.mzml.gz", "*.mzXML", "*.raw", "*.d/"] {
        assert!(lines.contains(&pattern), ".gitignore lacks {pattern}");
    }
    for bare in [".mzML", ".raw", ".mzXML"] {
        assert!(
            !lines.contains(&bare),
            "{bare} only matches a file of that exact name"
        );
    }
    let Some(_) = ignored("probe.mzML") else {
        eprintln!("git is not available: only the pattern text was checked");
        return;
    };
    for path in [
        "data/run.mzML",
        "a/b/c/sample.mzML",
        "sample.mzml",
        "a/sample.mzML.gz",
        "a/sample.mzml.gz",
        "x/sample.mzXML",
        "sample.raw",
        "run1.d/analysis.baf",
    ] {
        assert_eq!(ignored(path), Some(true), "{path} must be ignored");
    }
    // The authored fixtures stay addable, and ordinary files are untouched.
    for path in [
        "test_file/new_fixture.mzML",
        "tests/fixtures/deep/new.mzML",
        "README.md",
        "src/main.rs",
        "examples/data.mzML.md",
    ] {
        assert_eq!(ignored(path), Some(false), "{path} must not be ignored");
    }
}
