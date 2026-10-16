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

const VIEWER_EXAMPLE: &str = include_str!("../../../presets/example.toml");
const METHOD_EXAMPLE: &str = include_str!("../../../presets/quantification-example.toml");

/// What a TOML file is, judged by the same parsers the window uses to open it.
fn describe(text: &str) -> Value {
    if toml::from_str::<crate::presets::Preset>(text).is_ok() {
        let bounds = crate::validation::DataBounds::unrestricted();
        return match crate::presets::parse(text, &bounds) {
            Ok((overlay, traces)) => {
                let document = toml::from_str::<crate::presets::Preset>(text).ok();
                json!({
                    "kind":"viewer_preset",
                    "valid":true,
                    "name":document.as_ref().map(|d| d.name.clone()).unwrap_or_default(),
                    "overlay":overlay,
                    "rows":document.as_ref().and_then(|d| d.rows),
                    "columns":document.as_ref().and_then(|d| d.columns),
                    "traces":traces.iter().map(|(name, params)| json!({"name":name,"description":workspace::trace_name(params),"parameters":params})).collect::<Vec<_>>(),
                })
            }
            Err(error) => json!({"kind":"viewer_preset","valid":false,"error":error}),
        };
    }
    match toml::from_str::<crate::quant::Method>(text) {
        Ok(method) => match crate::quant::validate(&method) {
            Ok(_) => json!({
                "kind":"quantification_method",
                "valid":true,
                "name":method.name,
                "analytes":method.analytes.iter().map(|a| json!({"name":a.extraction.name,"expected_rt_minutes":a.expected_rt,"rt_window_minutes":a.rt_window})).collect::<Vec<_>>(),
            }),
            Err(error) => json!({"kind":"quantification_method","valid":false,"error":error}),
        },
        Err(error) => json!({"kind":"unreadable","valid":false,"error":error.to_string()}),
    }
}

pub(super) fn list_presets(
    app: &MzViewerApp,
    b: &GuiBridge,
    directory: Option<String>,
) -> Result<Value, String> {
    let mut listing = json!({
        "examples":[
            {"source":"example:viewer","kind":"viewer_preset","filename":"extraction-example.toml","use":"apply_preset"},
            {"source":"example:quantification","kind":"quantification_method","filename":"quantification-example.toml","use":"start_quantification"},
        ],
        // Only presets an AI could open anyway: inside a root and still there.
        "recent":app.presets.editor.recent().iter().filter(|p| b.policy.file(&p.to_string_lossy(), "toml").is_ok()).collect::<Vec<_>>(),
    });
    if let Some(directory) = directory {
        let directory = b.policy.directory(&directory)?;
        let mut paths = vec![];
        for entry in std::fs::read_dir(&directory).map_err(err)? {
            let path = entry.map_err(err)?.path();
            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("toml"))
            {
                paths.push(path);
            }
        }
        paths.sort();
        let truncated = paths.len() > 500;
        paths.truncate(500);
        let files: Vec<Value> = paths
            .iter()
            .map(|path| {
                let mut entry = match std::fs::metadata(path) {
                    Ok(m) if m.len() > 1024 * 1024 => {
                        json!({"kind":"unreadable","valid":false,"error":"larger than 1 MiB"})
                    }
                    _ => match std::fs::read_to_string(path) {
                        Ok(text) => describe(&text),
                        Err(error) => {
                            json!({"kind":"unreadable","valid":false,"error":error.to_string()})
                        }
                    },
                };
                // The listing says what a file is; get_preset gives the details.
                if let Some(map) = entry.as_object_mut() {
                    map.retain(|key, _| {
                        matches!(key.as_str(), "kind" | "valid" | "name" | "error")
                    });
                    map.insert("path".into(), json!(path));
                }
                entry
            })
            .collect();
        listing["directory"] = json!({"path":directory,"files":files,"truncated":truncated});
    }
    Ok(listing)
}

pub(super) fn get_preset(b: &GuiBridge, source: &str) -> Result<Value, String> {
    let (label, text) = match source {
        "example:viewer" => (source.to_owned(), VIEWER_EXAMPLE.to_owned()),
        "example:quantification" => (source.to_owned(), METHOD_EXAMPLE.to_owned()),
        path => {
            let path = b.policy.file(path, "toml")?;
            if std::fs::metadata(&path).map_err(err)?.len() > 1024 * 1024 {
                return Err("The file is larger than 1 MiB".into());
            }
            (
                path.to_string_lossy().into_owned(),
                std::fs::read_to_string(&path).map_err(err)?,
            )
        }
    };
    let mut result = describe(&text);
    result["source"] = json!(label);
    result["toml"] = json!(text);
    Ok(result)
}

pub(super) fn save_preset(
    app: &mut MzViewerApp,
    b: &GuiBridge,
    path: &str,
    text: &str,
) -> Result<Value, String> {
    let path = b.policy.output_as(path, "toml")?;
    let described = describe(text);
    if described["kind"] != "viewer_preset" || described["valid"] != true {
        return Err(match described["error"].as_str() {
            Some(error) => format!("Not a valid viewer preset: {error}"),
            None => {
                "Not a viewer preset (a quantification method is saved with save_method)".into()
            }
        });
    }
    crate::atomic_write::write_atomic_new(&path, text).map_err(err)?;
    app.presets.editor.remember_saved(path.clone());
    Ok(
        json!({"path":path,"traces":described["traces"].as_array().map_or(0, Vec::len),"overlay":described["overlay"]}),
    )
}

pub(super) fn save_method(
    app: &mut MzViewerApp,
    b: &GuiBridge,
    path: &str,
    method_toml: Option<&str>,
) -> Result<Value, String> {
    let path = b.policy.output_as(path, "toml")?;
    super::super::quant::remote_save_method(&mut app.quant, &path, method_toml)
}

pub(super) fn open_method(
    app: &mut MzViewerApp,
    b: &GuiBridge,
    path: &str,
    discard_unsaved: bool,
) -> Result<Value, String> {
    let path = b.policy.file(path, "toml")?;
    super::super::quant::remote_open_method(&mut app.quant, &path, discard_unsaved)
}

pub(super) fn save_batch(
    app: &mut MzViewerApp,
    b: &GuiBridge,
    path: &str,
) -> Result<Value, String> {
    let path = b.policy.output_as(path, "chromquant")?;
    super::super::quant::remote_save_batch(&mut app.quant, &path)
}

/// Loads a batch and reopens the source datasets the policy allows, like the window does.
pub(super) fn load_batch(
    app: &mut MzViewerApp,
    b: &GuiBridge,
    path: &str,
) -> Result<Value, String> {
    let path = b.policy.file(path, "chromquant")?;
    let sources = super::super::quant::remote_load_batch(&mut app.quant, &path)?;
    let mut reopening = vec![];
    let mut skipped = vec![];
    for source in sources {
        let already_open = app
            .files
            .values()
            .any(|f| f.cache.source_path.as_deref().unwrap_or(&f.path) == source.to_string_lossy());
        if already_open {
            continue;
        }
        match b.policy.source(&source.to_string_lossy()) {
            Ok(allowed) => reopening.push(allowed),
            Err(reason) => skipped.push(json!({"source":source,"reason":reason})),
        }
    }
    if !reopening.is_empty() {
        super::super::panels::queue_file_imports(app, reopening.clone());
    }
    Ok(json!({
        "path":path,
        "results":super::super::quant::remote_result_count(&app.quant),
        "reopening":reopening,
        "skipped":skipped,
        "note":"Datasets reopen in the background; list_datasets shows them when loaded.",
    }))
}
