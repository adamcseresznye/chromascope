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
