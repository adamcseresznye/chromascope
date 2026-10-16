//! Project operations of the desktop MCP server: create, open, save and refresh a project,
//! the same worker-thread operations the Project menu starts. Each replies when the window has
//! finished, and every dataset involved is checked against the access policy first.
use super::super::project_workbench as workbench;
use super::*;

fn ready(app: &MzViewerApp) -> Result<(), String> {
    if workbench::project_actions_ready(app) {
        Ok(())
    } else {
        Err(
            "An import, analysis or project operation is still running; retry when it has finished"
                .into(),
        )
    }
}

/// Everything a save would register must be a dataset the AI may use.
fn authorize_workspace(
    app: &MzViewerApp,
    ctx: &egui::Context,
    b: &GuiBridge,
) -> Result<(), String> {
    for source in workbench::workspace_sources(app, ctx) {
        b.policy
            .source(&source.to_string_lossy())
            .map_err(|reason| {
                format!(
                    "The workspace holds a dataset ({}) that a project may not register: {reason}",
                    source.display()
                )
            })?;
    }
    Ok(())
}

pub(super) fn create(
    app: &mut MzViewerApp,
    ctx: &egui::Context,
    b: &GuiBridge,
    path: &str,
) -> Result<Wait, String> {
    b.policy.require_project_writes()?;
    let pointer = b.policy.new_file_as(path, "json")?;
    ready(app)?;
    authorize_workspace(app, ctx, b)?;
    let before = app.project.completions();
    workbench::begin_create(app, ctx, pointer)?;
    Ok(Wait::Project { before })
}

pub(super) fn open(app: &mut MzViewerApp, b: &GuiBridge, path: &str) -> Result<Wait, String> {
    ready(app)?;
    let given = std::path::Path::new(path);
    let root = if given.is_dir() {
        b.policy.directory(path)?
    } else {
        let file = b.policy.file(path, "json")?;
        b.policy
            .directory(&workbench::data_root(&file)?.to_string_lossy())?
    };
    // Read the project first: every dataset it registers is opened by the window, so each one
    // that exists must lie inside a root before anything is touched.
    let project = crate::project::Project::open(&root).map_err(|e| e.message)?;
    for source in &project.sources {
        if source.path.exists() {
            b.policy.source(&source.path.to_string_lossy())?;
        }
    }
    let before = app.project.completions();
    workbench::begin_open(app, root);
    Ok(Wait::Project { before })
}

pub(super) fn save(
    app: &mut MzViewerApp,
    ctx: &egui::Context,
    b: &GuiBridge,
) -> Result<Wait, String> {
    b.policy.require_project_writes()?;
    ready(app)?;
    let Some(root) = app.project.open_root() else {
        return Err("No project is open; create_project makes one".into());
    };
    // The project folder is written to, so it must be one the AI may write in.
    b.policy.directory(&root.to_string_lossy())?;
    authorize_workspace(app, ctx, b)?;
    let before = app.project.completions();
    workbench::begin_save(app, ctx);
    Ok(Wait::Project { before })
}

pub(super) fn refresh(app: &mut MzViewerApp, b: &GuiBridge) -> Result<Wait, String> {
    ready(app)?;
    let Some(root) = app.project.open_root() else {
        return Err("No project is open".into());
    };
    b.policy.directory(&root.to_string_lossy())?;
    let before = app.project.completions();
    workbench::begin_refresh(app);
    Ok(Wait::Project { before })
}

/// The reply once a project operation has finished or failed.
pub(super) fn finished(app: &MzViewerApp, before: u64) -> Option<Result<Value, String>> {
    if app.project.is_busy() || workspace::restore_in_progress(app) {
        return None;
    }
    if app.project.completions() > before {
        Some(Ok(app.project.summary()))
    } else {
        // The worker reported an error: the project line of the window says which.
        Some(Err(app.project.message().to_owned()))
    }
}
