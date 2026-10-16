//! Window-state operations of the desktop MCP server: layout and per-trace management. Each
//! one calls the same workspace function the matching control in the window calls.
use super::*;

/// The arrangement of the plots, as `set_layout` takes it and `gui_state` reports it.
pub(super) fn layout(app: &MzViewerApp) -> Value {
    let view = &app.workspace.view;
    let mode = if view.compare_samples {
        "compare"
    } else if app.workspace.overlay {
        "overlay"
    } else {
        "grid"
    };
    json!({
        "mode":mode,
        "auto_grid":view.auto_grid,
        "rows":view.rows,
        "columns":view.columns,
        "page":app.workspace.page,
        "panels":{"files":view.files,"inspector":view.inspector,"spectrum":view.spectrum},
        "focus":view.focus_restore.is_some(),
    })
}

pub(super) fn set_layout(app: &mut MzViewerApp, command: Command) -> Result<Value, String> {
    let Command::SetLayout {
        mode,
        auto_grid,
        rows,
        columns,
        page,
        show_files,
        show_inspector,
        show_spectrum,
        focus,
    } = command
    else {
        return Err("Expected a layout request".into());
    };
    for (name, value) in [("rows", rows), ("columns", columns)] {
        if value.is_some_and(|v| !(1..=8).contains(&v)) {
            return Err(format!("{name} must be 1..8"));
        }
    }
    // Validate everything before changing anything.
    let mode = match mode.as_deref() {
        None => None,
        Some(m @ ("overlay" | "grid" | "compare")) => Some(m.to_owned()),
        Some(_) => return Err("mode: overlay, grid or compare".into()),
    };
    match mode.as_deref() {
        Some("overlay") => {
            app.workspace.view.compare_samples = false;
            app.workspace.overlay = true;
        }
        Some("grid") => {
            app.workspace.view.compare_samples = false;
            app.workspace.overlay = false;
        }
        Some("compare") => app.workspace.view.compare_samples = true,
        _ => {}
    }
    if let Some(v) = auto_grid {
        app.workspace.view.auto_grid = v;
    }
    if let Some(v) = rows {
        app.workspace.view.rows = v;
    }
    if let Some(v) = columns {
        app.workspace.view.columns = v;
    }
    if let Some(v) = page {
        app.workspace.page = v;
    }
    // Focus first, so explicit panel choices in the same call win over it.
    if let Some(v) = focus {
        if v != app.workspace.view.focus_restore.is_some() {
            app.workspace.view.toggle_focus();
        }
    }
    if let Some(v) = show_files {
        app.workspace.view.files = v;
    }
    if let Some(v) = show_inspector {
        app.workspace.view.inspector = v;
    }
    if let Some(v) = show_spectrum {
        app.workspace.view.spectrum = v;
    }
    Ok(layout(app))
}

/// The extraction a trace tool means: a retained trace by index, or the current one.
fn trace_params(
    app: &MzViewerApp,
    dataset: usize,
    trace_index: Option<usize>,
) -> Result<ProcessingParams, String> {
    let file = app.files.get(&dataset).ok_or("Unknown dataset")?;
    match trace_index {
        None => file
            .cache
            .last_processing_params
            .clone()
            .ok_or_else(|| "The dataset has no current extraction".into()),
        Some(index) => app
            .workspace
            .traces
            .get(&dataset)
            .and_then(|traces| traces.get(index))
            .map(|trace| trace.params.clone())
            .ok_or_else(|| "Unknown retained trace index".into()),
    }
}

fn trace_summary(app: &MzViewerApp, dataset: usize) -> Value {
    let retained = app.workspace.traces.get(&dataset).map_or(0, Vec::len);
    json!({
        "dataset_id":dataset,
        "retained_traces":retained,
        "has_current_trace":app.files.get(&dataset).is_some_and(|f| f.cache.last_processing_params.is_some()),
    })
}

pub(super) fn retain_trace(app: &mut MzViewerApp, dataset: usize) -> Result<Value, String> {
    let params = trace_params(app, dataset, None)?;
    let before = app.workspace.traces.get(&dataset).map_or(0, Vec::len);
    workspace::retain_current(app, dataset);
    let after = app.workspace.traces.get(&dataset).map_or(0, Vec::len);
    let mut summary = trace_summary(app, dataset);
    summary["retained"] = json!(after > before);
    summary["already_retained"] = json!(after == before);
    summary["trace"] = json!(workspace::trace_name(&params));
    Ok(summary)
}

pub(super) fn remove_trace(
    app: &mut MzViewerApp,
    dataset: usize,
    trace_index: Option<usize>,
) -> Result<Value, String> {
    let params = trace_params(app, dataset, trace_index)?;
    if app.async_state.is_processing {
        return Err("An extraction is running; retry when it has finished".into());
    }
    workspace::delete_key(app, dataset, &params);
    let mut summary = trace_summary(app, dataset);
    summary["removed"] = json!(workspace::trace_name(&params));
    Ok(summary)
}

pub(super) fn set_trace_visibility(
    app: &mut MzViewerApp,
    dataset: usize,
    trace_index: Option<usize>,
    visible: bool,
) -> Result<Value, String> {
    let params = trace_params(app, dataset, trace_index)?;
    match trace_index {
        None => {
            if visible {
                app.workspace.hidden_current.remove(&dataset);
            } else {
                app.workspace.hidden_current.insert(dataset);
                app.integration = Default::default();
            }
        }
        Some(index) => {
            if let Some(trace) = app
                .workspace
                .traces
                .get_mut(&dataset)
                .and_then(|traces| traces.get_mut(index))
            {
                trace.visible = visible;
            }
        }
    }
    let mut summary = trace_summary(app, dataset);
    summary["trace"] = json!(workspace::trace_name(&params));
    summary["visible"] = json!(visible);
    Ok(summary)
}
