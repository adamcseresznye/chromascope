//! Guards for the release packaging: licence notices must ship in every package and the
//! release workflow must not run an unpinned download.
use std::fs;

fn read(path: &str) -> String {
    fs::read_to_string(format!("{}/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{path}: {e}"))
        .replace("\r\n", "\n")
}

/// The text of one workflow step: from its `- name:` line up to the next step.
fn step<'a>(workflow: &'a str, name: &str) -> &'a str {
    let start = workflow
        .find(&format!("- name: {name}"))
        .unwrap_or_else(|| panic!("step {name} not found"));
    let rest = &workflow[start + 1..];
    let end = rest.find("\n      - ").map_or(rest.len(), |i| i + 1);
    &workflow[start..start + end]
}

#[test]
fn third_party_notices_cover_fonts_and_dependencies() {
    let licenses = read("THIRD_PARTY_LICENSES.txt");
    for needle in [
        "JetBrains Mono",
        "The JetBrains Mono Project Authors",
        "Inter 4.1",
        "The Inter Project Authors",
        "SIL OPEN FONT LICENSE Version 1.1",
        "egui 0.33.3",
        "mzdata 0.63",
        "serde 1.",
        "Permission is hereby granted, free of charge",
        "Apache License",
    ] {
        assert!(
            licenses.contains(needle),
            "THIRD_PARTY_LICENSES.txt lacks {needle:?}"
        );
    }
    let ofl = read("assets/fonts/jetbrains-mono/OFL.txt");
    assert!(ofl.starts_with("Copyright 2020 The JetBrains Mono Project Authors"));
    assert!(ofl.contains("SIL OPEN FONT LICENSE Version 1.1"));
    assert!(read("assets/fonts/jetbrains-mono/README.md").contains("OFL.txt"));
    // About window shows both font licences (include_str! also proves the files exist).
    let about = read("src/gui/information.rs");
    assert!(about.contains("fonts/inter/LICENSE.txt"));
    assert!(about.contains("fonts/jetbrains-mono/OFL.txt"));
}

#[test]
fn every_release_package_carries_the_licence_notices() {
    let workflow = read(".github/workflows/release.yml");
    let linux = step(&workflow, "Package (tar.gz, deb, rpm, AppImage)");
    // tar.gz staging copy and the AppImage doc directory.
    assert!(linux.contains("cp LICENSE THIRD_PARTY_LICENSES.txt README.md packaging/linux"));
    assert!(linux.contains("appdir/usr/share/doc/chromascope/"));
    assert!(linux.contains("cp LICENSE THIRD_PARTY_LICENSES.txt README.md appdir/"));
    let windows = step(&workflow, "Package (portable zip and MSI)");
    assert!(windows.contains("cp LICENSE THIRD_PARTY_LICENSES.txt README.md \"stage/$NAME/\""));
    assert!(windows.contains("-d LicenseDir=."));
    let macos = step(&workflow, "Package (zip and dmg)");
    assert!(macos.contains("cp universal/* LICENSE THIRD_PARTY_LICENSES.txt README.md"));
    assert!(macos.contains("Contents/Resources/\"") && macos.contains("stage/dmg/"));
    assert_eq!(
        macos.matches("THIRD_PARTY_LICENSES.txt").count(),
        3,
        "zip, app bundle and dmg root"
    );

    let wxs = read("packaging/windows/chromascope.wxs");
    assert!(wxs.contains(r"$(LicenseDir)\LICENSE"));
    assert!(wxs.contains(r"$(LicenseDir)\THIRD_PARTY_LICENSES.txt"));

    // WIX0367: a component with an auto-generated GUID may hold only one file. The MSI build
    // (found by a local dry run) failed when both licence files shared a component.
    for component in wxs.split("<Component ").skip(1) {
        let component = &component[..component.find("</Component>").unwrap()];
        if !component.contains("Guid=") {
            assert!(
                component.matches("<File ").count() <= 1,
                "component with auto GUID holds several files: {component}"
            );
        }
    }

    let cargo = read("Cargo.toml");
    let deb = &cargo[cargo.find("[package.metadata.deb]").unwrap()..];
    let deb = &deb[..deb.find("[package.metadata.generate-rpm]").unwrap()];
    assert!(deb.contains("THIRD_PARTY_LICENSES.txt"));
    let rpm = &cargo[cargo.find("[package.metadata.generate-rpm]").unwrap()..];
    let rpm = &rpm[..rpm.find("[dependencies]").unwrap()];
    assert!(rpm.contains("THIRD_PARTY_LICENSES.txt"));
}

#[test]
fn appimagetool_is_pinned_and_verified() {
    let workflow = read(".github/workflows/release.yml");
    assert!(
        !workflow.contains("/continuous/"),
        "appimagetool must not use the moving tag"
    );
    let linux = step(&workflow, "Package (tar.gz, deb, rpm, AppImage)");
    assert!(linux.contains("APPIMAGETOOL_VERSION=1.9.1"));
    let download = linux.find("curl -fsSL -o appimagetool").unwrap();
    let verify = linux.find("sha256sum -c -").expect("checksum verification");
    let run = linux.find("./appimagetool --no-appstream").unwrap();
    assert!(
        download < verify && verify < run,
        "verify between download and execution"
    );
    for arch in ["x86_64", "aarch64"] {
        let line = linux
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("{arch})")))
            .unwrap_or_else(|| panic!("no pinned hash for {arch}"));
        let hash = line
            .split("SHA256=")
            .nth(1)
            .unwrap()
            .split_whitespace()
            .next()
            .unwrap();
        assert_eq!(hash.len(), 64);
        assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));
    }
}

/// The text of one top-level job (`  name:` under `jobs:`), up to the next job.
fn job<'a>(workflow: &'a str, name: &str) -> &'a str {
    let header = format!("\n  {name}:\n");
    let start = workflow
        .find(&header)
        .unwrap_or_else(|| panic!("job {name} not found"))
        + 1;
    let mut end = workflow.len();
    let mut offset = start;
    for (index, line) in workflow[start..].split_inclusive('\n').enumerate() {
        // A line indented by exactly two spaces that ends in ':' opens the next job.
        let opens_job = line.starts_with("  ")
            && !line.starts_with("   ")
            && !line.trim_start().starts_with('#')
            && line.trim_end().ends_with(':');
        if index > 0 && opens_job {
            end = offset;
            break;
        }
        offset += line.len();
    }
    &workflow[start..end]
}

#[test]
fn release_is_gated_on_tests_and_ci_covers_other_platforms_and_the_minimum_rust() {
    let release = read(".github/workflows/release.yml");
    let tests = job(&release, "test");
    assert!(tests.contains("cargo test --locked\n"));
    assert!(tests.contains("--no-default-features --features mcp-headless"));
    for build in ["linux", "windows", "macos"] {
        assert!(
            job(&release, build).contains("needs: [version, test]"),
            "{build} must wait for the test job"
        );
    }
    // The publishing job waits for the builds, which wait for the tests.
    assert!(job(&release, "release").contains("needs: [linux, windows, macos]"));
    assert!(!tests.contains("continue-on-error"));

    let ci = read(".github/workflows/CI.yml");
    let platforms = job(&ci, "platforms");
    assert!(platforms.contains("windows-latest") && platforms.contains("macos-latest"));
    assert!(platforms.contains("cargo test --locked\n"));
    // CI compiles on exactly the Rust version Cargo.toml promises.
    let cargo = read("Cargo.toml");
    let msrv = cargo
        .lines()
        .find_map(|l| l.strip_prefix("rust-version = \""))
        .and_then(|l| l.strip_suffix('"'))
        .expect("rust-version in Cargo.toml");
    let msrv_job = job(&ci, "msrv");
    assert!(
        msrv_job.contains(&format!("dtolnay/rust-toolchain@{msrv}\n")),
        "msrv job must install Rust {msrv}"
    );
    assert!(msrv_job.contains("cargo check --locked --all-targets --all-features"));
    assert!(msrv_job.contains(&format!("({msrv})")));
}

#[test]
fn documentation_matches_the_shipped_app() {
    let readme = read("README.md");
    // The tab is called "Audit package"; "Reports…" and the old status line must be gone.
    assert!(!readme.contains("**Reports…**"));
    assert!(!readme.contains("preparing a release"));
    assert!(readme.contains("**Audit package**"));
    // Installation covers the installers and the first-launch warnings of every platform.
    for needle in [
        ".msi",
        "administrator",
        "SmartScreen",
        "Open Anyway",
        ".deb",
        ".AppImage",
        "THIRD_PARTY_LICENSES.txt",
        "CHANGELOG.md",
    ] {
        assert!(readme.contains(needle), "README lacks {needle:?}");
    }
    // macOS 15+ removed right-click > Open: the workflow comment may only say so, not advise it.
    let workflow = read(".github/workflows/release.yml");
    assert!(workflow.contains("Open Anyway"));
    assert!(!workflow.contains("users open it once via right-click"));

    let notes = read("docs/RELEASE_NOTES.md");
    assert!(!notes.contains("uncommitted working tree"));
    assert!(!notes.contains("no tag"));
    assert!(notes.contains("0.3.0"));

    // The changelog has a section for the version in Cargo.toml.
    let version = read("Cargo.toml")
        .lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .and_then(|l| l.strip_suffix('"'))
        .expect("version in Cargo.toml")
        .to_owned();
    let changelog = read("CHANGELOG.md");
    assert!(changelog.contains(&format!("## [{version}]")));
    assert!(changelog.contains("## [0.2.0]") && changelog.contains("## [0.1.0]"));

    // Numerical behaviour that users can run into is written down.
    let guide = read("docs/USER_GUIDE.md");
    assert!(guide.contains("within **0.01 Da**"));
    assert!(guide.contains("double precision and rounds once"));

    let status = read("docs/FEATURE_STATUS.md");
    assert!(!status.contains("Reports and AI activity"));
    assert!(!status.contains("Project reports and delivery"));
}
