//! Files the desktop MCP server reads and writes for the person: sessions, presets, methods,
//! batches. Every path goes through the access policy, saves are written whole or not at all
//! and never replace an existing file.
use super::*;

pub(super) fn save_session(
    app: &mut MzViewerApp,
    ctx: &egui::Context,
    b: &GuiBridge,
    path: &str,
) -> Result<Value, String> {
    let path = b.policy.output_as(path, "chromascope")?;
    let bytes = workspace::session_bytes(app, ctx)?;
    crate::atomic_write::write_atomic_new(&path, &bytes).map_err(err)?;
    Ok(json!({"path":path,"bytes":bytes.len(),"datasets":app.files.len()}))
}

/// Replaces the workspace with a saved session; the caller waits for the datasets to load.
pub(super) fn load_session(
    app: &mut MzViewerApp,
    ctx: &egui::Context,
    b: &mut GuiBridge,
    path: &str,
) -> Result<(), String> {
    let path = b.policy.file(path, "chromascope")?;
    let bytes = std::fs::read(&path).map_err(err)?;
    let policy = b.policy.clone();
    workspace::open_session_bytes(app, ctx, &bytes, &|source| {
        policy.source(source).map(|_| ())
    })?;
    // The datasets the cached extractions belong to are gone.
    b.results.clear();
    b.undo.clear();
    Ok(())
}
