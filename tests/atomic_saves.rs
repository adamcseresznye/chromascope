//! Every file the user saves must go through `atomic_write::write_atomic`, never `fs::write`
//! straight onto the existing file (a crash mid-write would corrupt the only copy).
use std::fs;

const USER_SAVE_FILES: &[&str] = &[
    "src/export.rs",
    "src/import.rs",
    "src/gui/workspace.rs", // session, integration CSV, SVG figures, recent files
    "src/gui/quant.rs",     // method, batch, results CSV
    "src/gui/preset_editor.rs", // presets and recent presets
    "src/gui/settings.rs",
    "src/gui/delivery.rs",
    "src/gui/examples.rs",
    "src/gui/project_workbench.rs",
    "src/gui/remote.rs",       // the MCP exports
    "src/gui/remote/files.rs", // MCP session, method, batch and preset saves
];

fn production_code(path: &str) -> String {
    let text = fs::read_to_string(format!("{}/{path}", env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{path}: {e}"));
    match text.find("#[cfg(test)]") {
        Some(i) => text[..i].to_owned(),
        None => text,
    }
}

#[test]
fn user_saves_use_the_atomic_helper() {
    for path in USER_SAVE_FILES {
        let code = production_code(path);
        assert!(
            !code.contains("fs::write("),
            "{path} overwrites a user file with fs::write; use atomic_write::write_atomic"
        );
        assert!(
            code.contains("atomic_write::write_atomic"),
            "{path} no longer saves through atomic_write"
        );
    }
}

#[test]
fn session_method_and_batch_saves_are_atomic() {
    let workspace = production_code("src/gui/workspace.rs");
    assert!(workspace.contains("write_atomic(path, b)"), "session save");
    let quant = production_code("src/gui/quant.rs");
    assert!(quant.contains("write_atomic(&path, &text)"), "method save");
    assert!(quant.contains("serde_json::to_vec(&batch)"), "batch save");
    let batch = &quant[quant.find("serde_json::to_vec(&batch)").unwrap() - 80..];
    assert!(batch.contains("write_atomic("), "batch save");
}

#[test]
fn helper_replaces_files_in_place_without_temp_leftovers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("batch.chromquant");
    chromascope::atomic_write::write_atomic(&path, b"v1").unwrap();
    chromascope::atomic_write::write_atomic(&path, b"v2 is longer").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"v2 is longer");
    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![std::ffi::OsString::from("batch.chromquant")]);
}
