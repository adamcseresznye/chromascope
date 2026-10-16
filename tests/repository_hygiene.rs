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

/// Paths of every tracked file, or `None` when Git is unavailable.
fn tracked_files() -> Option<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(
        String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

#[test]
fn tracked_files_carry_no_account_name_or_local_machine_path() {
    // Built at run time so that this file does not contain what it forbids.
    let needles = [
        ["s0212", "777"].concat(),
        ["universiteit", " antwerpen"].concat(),
        ["bureau", "blad"].concat(),
    ];
    let Some(files) = tracked_files() else {
        eprintln!("git is not available: tracked files were not scanned");
        return;
    };
    assert!(files.len() > 100, "unexpectedly few tracked files");
    let binary = [
        "png", "mp4", "ico", "icns", "gif", "ttf", "otf", "woff", "woff2",
    ];
    let mut offenders = Vec::new();
    for file in &files {
        let extension = Path::new(file)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if binary.contains(&extension.as_str()) {
            continue;
        }
        let Ok(bytes) = fs::read(root().join(file)) else {
            continue; // listed but deleted in the working tree
        };
        let text = String::from_utf8_lossy(&bytes).to_lowercase();
        for needle in &needles {
            if text.contains(needle.as_str()) {
                offenders.push(format!("{file}: {needle}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "personal data in tracked files:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn indexed_mzml_fixture_was_anonymised_without_moving_a_single_byte() {
    // test_file/data_dependent_02.mzML is an indexed mzML: a <sourceFile location> of a different
    // length would shift every byte offset in its index. The original path was replaced by a
    // string of exactly the same length, so the file keeps the size it had before.
    let bytes = fs::read(root().join("test_file/data_dependent_02.mzML")).unwrap();
    assert_eq!(bytes.len(), 517_498, "same-length edit only");
    let text = String::from_utf8_lossy(&bytes);
    let location = text
        .split("<sourceFile ")
        .nth(1)
        .and_then(|s| s.split("location=\"").nth(1))
        .and_then(|s| s.split('"').next())
        .expect("sourceFile location");
    assert!(location.starts_with("file:///C:"));
    assert_eq!(location.len(), 70, "length of the original location");
    assert!(location.contains("anonymised"));
}

#[test]
fn package_metadata_feeds_the_deb_and_rpm_packages() {
    // cargo-deb and cargo-generate-rpm take the package synopsis, URL and licence from
    // [package]; without them the packages ship with an empty description.
    let manifest: toml::Value = read("Cargo.toml").parse().unwrap();
    let package = &manifest["package"];
    let text = |key: &str| {
        package[key]
            .as_str()
            .unwrap_or_else(|| panic!("[package] {key} missing"))
            .to_owned()
    };
    let description = text("description");
    assert!(
        (40..=200).contains(&description.len()),
        "a one-line description: {description:?}"
    );
    assert!(!description.ends_with('.') && !description.contains('\n'));
    assert_eq!(
        text("repository"),
        "https://github.com/adamcseresznye/chromascope"
    );
    assert!(text("homepage").starts_with("https://"));
    assert_eq!(text("license"), "GPL-3.0-only");
    assert_eq!(text("readme"), "README.md");
    assert!(root().join("README.md").is_file());
    let keywords = package["keywords"].as_array().unwrap();
    assert!(
        (1..=5).contains(&keywords.len()),
        "crates.io allows at most five keywords"
    );
    assert!(!package["categories"].as_array().unwrap().is_empty());
    // The Debian package adds its own long description on top of the synopsis.
    assert!(
        manifest["package"]["metadata"]["deb"]["extended-description"]
            .as_str()
            .is_some_and(|d| d.len() > description.len())
    );
}

fn gui_sources() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
                out.push((path.display().to_string(), text));
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join("src/gui"), &mut out);
    out.push(("src/gui/mod.rs".into(), read("src/gui/mod.rs")));
    out
}

#[test]
fn the_audit_package_has_one_name_in_the_tab_the_window_and_the_workflow() {
    let sources = gui_sources();
    let all: String = sources.iter().map(|(_, text)| text.as_str()).collect();
    for old in [
        "Audit package (optional)",
        "Report and audit package",
        "\"Reports\"",
    ] {
        assert!(
            !all.contains(old),
            "{old:?} is no longer a name of this tab"
        );
    }
    // The literal exists once, as the constant every place uses.
    assert_eq!(all.matches("\"Audit package\"").count(), 1);
    let workflow = read("src/gui/workflow.rs");
    assert!(workflow.contains("const AUDIT_PACKAGE: &str = \"Audit package\";"));
    for user in [
        "src/gui/delivery.rs",
        "src/gui/workbench.rs",
        "src/gui/panels.rs",
    ] {
        assert!(
            read(user).contains("AUDIT_PACKAGE"),
            "{user} must use the constant"
        );
    }
    // Being optional is a status of the workflow step, not part of the name.
    assert!(workflow.contains("Optional. Export a readable report"));
}
