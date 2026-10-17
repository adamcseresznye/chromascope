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

/// Interface sizes the View menu offers, in percent.
const INTERFACE_SIZES: [u16; 4] = [90, 100, 115, 130];

fn theme_name(preference: egui::ThemePreference) -> &'static str {
    match preference {
        egui::ThemePreference::System => "system",
        egui::ThemePreference::Light => "light",
        egui::ThemePreference::Dark => "dark",
    }
}

/// The display preferences, as the View menu shows them.
pub(super) fn settings(ctx: &egui::Context, updates: &super::super::update::Updates) -> Value {
    let (check_for_updates, update_available) =
        super::super::settings::with_update_prefs(|prefs| {
            (prefs.check_for_updates, updates.reported(prefs))
        });
    json!({
        "theme":theme_name(ctx.options(|o| o.theme_preference)),
        "interface_size":(ctx.zoom_factor() * 100.0).round() as u16,
        "mz_decimals":super::super::settings::mz_decimals(),
        "check_for_updates":check_for_updates,
        "update_check_blocked_by_environment":updates.opted_out(),
        "update_available":update_available,
        "choices":{
            "theme":["system","light","dark"],
            "interface_size":INTERFACE_SIZES,
            "mz_decimals":super::super::settings::MZ_DECIMAL_CHOICES,
        },
    })
}

pub(super) fn set_settings(
    ctx: &egui::Context,
    updates: &super::super::update::Updates,
    theme: Option<&str>,
    interface_size: Option<u16>,
    mz_decimals: Option<u8>,
    check_for_updates: Option<bool>,
) -> Result<Value, String> {
    // Check everything first, so a bad value leaves all preferences as they were.
    let theme = match theme {
        None => None,
        Some("system") => Some(egui::ThemePreference::System),
        Some("light") => Some(egui::ThemePreference::Light),
        Some("dark") => Some(egui::ThemePreference::Dark),
        Some(_) => return Err("theme: system, light or dark".into()),
    };
    if interface_size.is_some_and(|size| !INTERFACE_SIZES.contains(&size)) {
        return Err(format!(
            "interface_size: one of {INTERFACE_SIZES:?} percent"
        ));
    }
    if mz_decimals.is_some_and(|d| !super::super::settings::MZ_DECIMAL_CHOICES.contains(&d)) {
        return Err(format!(
            "mz_decimals: one of {:?}",
            super::super::settings::MZ_DECIMAL_CHOICES
        ));
    }
    if let Some(theme) = theme {
        ctx.set_theme(theme);
    }
    if let Some(decimals) = mz_decimals {
        super::super::settings::set_mz_decimals(ctx, decimals);
    }
    if let Some(check) = check_for_updates {
        super::super::settings::with_update_prefs(|prefs| prefs.check_for_updates = check);
    }
    match interface_size {
        Some(size) => super::super::settings::set_zoom(ctx, f32::from(size) / 100.0),
        None => super::super::settings::save(ctx),
    }
    let mut now = settings(ctx, updates);
    if let Some(size) = interface_size {
        // The window applies the new size at the start of its next frame; report it already.
        now["interface_size"] = json!(size);
    }
    Ok(now)
}

/// Switches the window to a workspace, through the same function the Analysis workflow menu uses.
pub(super) fn show_workspace(
    app: &mut MzViewerApp,
    ctx: &egui::Context,
    workspace: &str,
    stage: Option<&str>,
) -> Result<Value, String> {
    use super::super::workflow::Destination;
    if stage.is_some() && workspace != "quantification" {
        return Err("stage applies to the quantification workspace only".into());
    }
    let destination = match (workspace, stage) {
        ("data_explorer", _) => Destination::Data,
        ("quantification", None) => Destination::Quant,
        ("quantification", Some("review")) => Destination::QuantReview,
        ("quantification", Some("calibration")) => Destination::QuantCalibration,
        ("quantification", Some("qc")) => Destination::QuantQc,
        ("quantification", Some(_)) => return Err("stage: review, calibration or qc".into()),
        ("method_editor", _) => Destination::Method,
        ("audit_package", _) => Destination::AuditPackage,
        ("ai_activity", _) => Destination::Activity,
        // The workflows that are not part of this build cannot be shown.
        ("identification", _) if crate::EXPERIMENTAL => Destination::Identify,
        ("untargeted_analysis", _) if crate::EXPERIMENTAL => Destination::Untargeted,
        ("statistics", _) if crate::EXPERIMENTAL => Destination::Statistics,
        _ => return Err(
            "workspace: data_explorer, quantification, method_editor, audit_package or ai_activity"
                .into(),
        ),
    };
    super::super::workflow::go(app, ctx, destination);
    Ok(json!({"workspace":workspace,"stage":stage}))
}

/// Clears what the window shows, as the Clear spectrum and Clear batch preset commands do.
pub(super) fn clear_display(app: &mut MzViewerApp, what: &str) -> Result<Value, String> {
    match what {
        "spectrum" => {
            if let Some(file) = app.active_file_id.and_then(|id| app.files.get_mut(&id)) {
                file.cache.mass_spectrum = None;
            }
            app.user_input.retention_time_ms_spectrum = None;
            app.workspace.view.spectrum = false;
        }
        "batch_preset" => {
            if app.async_state.is_processing {
                return Err("An extraction is running; retry when it has finished".into());
            }
            app.presets.specs = None;
            app.presets.applied.clear();
            app.presets.failed.clear();
            app.state_changed = StateChange::Unchanged;
        }
        _ => return Err("what: spectrum or batch_preset".into()),
    }
    Ok(json!({"cleared":what}))
}
