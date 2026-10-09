//! User-facing preset authoring; TOML remains a portable file format behind native dialogs.
use super::{
    presets::{Preset, TraceSpec},
    state::MzViewerApp,
};
use crate::processing::AcquisitionMode;
use std::path::PathBuf;

#[derive(Clone)]
struct Row {
    name: String,
    acquisition: AcquisitionMode,
    kind: String,
    mass: String,
    precursor: String,
    ppm: String,
    smoothing: String,
    polarity: String,
    ms_level: String,
    range_min: String,
    range_max: String,
}
impl Default for Row {
    fn default() -> Self {
        Self {
            name: "New analyte".into(),
            acquisition: AcquisitionMode::FS,
            kind: "XIC".into(),
            mass: String::new(),
            precursor: String::new(),
            ppm: "10".into(),
            smoothing: "0".into(),
            polarity: "positive".into(),
            ms_level: String::new(),
            range_min: String::new(),
            range_max: String::new(),
        }
    }
}
fn number(text: &str, label: &str) -> Result<f64, String> {
    let n: f64 = text
        .trim()
        .parse()
        .map_err(|_| format!("Enter a numeric {label}."))?;
    if !n.is_finite() || n <= 0.0 {
        return Err(format!("{label} must be positive and finite."));
    }
    Ok(n)
}
impl Row {
    fn from_spec(t: TraceSpec) -> Self {
        Self {
            name: t.name,
            acquisition: t.acquisition,
            kind: t.kind.to_uppercase(),
            mass: t.mass.map(|n| n.to_string()).unwrap_or_default(),
            precursor: t.precursor_mz.map(|n| n.to_string()).unwrap_or_default(),
            ppm: t.ppm.to_string(),
            smoothing: t.smoothing.to_string(),
            polarity: t.polarity,
            ms_level: t.ms_level.map(|n| n.to_string()).unwrap_or_default(),
            range_min: t.mz_range.map(|r| r[0].to_string()).unwrap_or_default(),
            range_max: t.mz_range.map(|r| r[1].to_string()).unwrap_or_default(),
        }
    }
    fn spec(&self) -> Result<TraceSpec, String> {
        if !["XIC", "TIC", "BPC"].contains(&self.kind.as_str()) {
            return Err("Trace must be XIC, TIC, or BPC.".into());
        }
        if !["positive", "negative"].contains(&self.polarity.as_str()) {
            return Err("Polarity must be positive or negative.".into());
        }
        if self.name.trim().is_empty() {
            return Err("Enter an analyte name.".into());
        }
        let mass = if self.kind == "XIC" {
            Some(number(&self.mass, "target/product m/z")?)
        } else {
            None
        };
        let precursor_mz =
            if self.acquisition == AcquisitionMode::MRM || !self.precursor.trim().is_empty() {
                Some(number(&self.precursor, "precursor m/z")?)
            } else {
                None
            };
        let ppm = if self.kind == "XIC" {
            let v: f64 = self
                .ppm
                .trim()
                .parse()
                .map_err(|_| "Enter a numeric ppm tolerance.")?;
            if !v.is_finite() || !(0.0..=1000.0).contains(&v) {
                return Err("Tolerance must be 0–1000 ppm.".into());
            }
            v
        } else {
            10.0
        };
        let smoothing: u8 = self
            .smoothing
            .trim()
            .parse()
            .map_err(|_| "Smoothing must be a whole number from 0 to 10.")?;
        if smoothing > 10 {
            return Err("Smoothing must be 0–10.".into());
        }
        let ms_level = if self.ms_level.trim().is_empty() {
            None
        } else {
            let n: u8 = self
                .ms_level
                .trim()
                .parse()
                .map_err(|_| "MS level must be a positive whole number.")?;
            if n == 0 {
                return Err("MS level must be at least 1.".into());
            }
            Some(n)
        };
        let mz_range = if self.kind == "XIC"
            || (self.range_min.trim().is_empty() && self.range_max.trim().is_empty())
        {
            None
        } else {
            let a = number(&self.range_min, "range minimum")?;
            let b = number(&self.range_max, "range maximum")?;
            if a >= b {
                return Err("Range maximum must exceed its minimum.".into());
            }
            Some([a, b])
        };
        Ok(TraceSpec {
            name: self.name.trim().into(),
            acquisition: self.acquisition,
            kind: self.kind.clone(),
            mass,
            precursor_mz,
            ppm,
            smoothing,
            polarity: self.polarity.clone(),
            ms_level,
            mz_range,
        })
    }
}
#[derive(Clone)]
struct Draft {
    name: String,
    rows: Vec<Row>,
    overlay: bool,
    grid_rows: usize,
    columns: usize,
    path: Option<PathBuf>,
}
impl Default for Draft {
    fn default() -> Self {
        Self {
            name: "My analyte preset".into(),
            rows: vec![Row::default()],
            overlay: false,
            grid_rows: 3,
            columns: 2,
            path: None,
        }
    }
}
impl Draft {
    fn read(path: PathBuf) -> Result<Self, String> {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        super::presets::parse(&text, &crate::validation::DataBounds::unrestricted())?;
        let p: Preset = toml::from_str(&text).map_err(|e| e.to_string())?;
        Ok(Self {
            name: if p.name.is_empty() {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            } else {
                p.name
            },
            rows: p.traces.into_iter().map(Row::from_spec).collect(),
            overlay: p.overlay,
            grid_rows: p.rows.unwrap_or(3),
            columns: p.columns.unwrap_or(2),
            path: Some(path),
        })
    }
    fn text(&self) -> Result<String, String> {
        if self.name.trim().is_empty() {
            return Err("Give this preset a name.".into());
        }
        let mut traces = Vec::new();
        for (i, r) in self.rows.iter().enumerate() {
            traces.push(
                r.spec()
                    .map_err(|e| format!("Row {} ({}): {e}", i + 1, r.name))?,
            );
        }
        let p = Preset {
            name: self.name.trim().into(),
            version: 1,
            rows: Some(self.grid_rows),
            columns: Some(self.columns),
            overlay: self.overlay,
            traces,
        };
        let text = toml::to_string_pretty(&p).map_err(|e| e.to_string())?;
        super::presets::parse(&text, &crate::validation::DataBounds::unrestricted())?;
        Ok(text)
    }
}
pub(super) struct EditorState {
    pub open: bool,
    draft: Draft,
    dirty: bool,
    pending: Option<Draft>,
    error: Option<String>,
    status: String,
    recent: Vec<PathBuf>,
    paste_open: bool,
    paste: String,
}
impl EditorState {
    fn empty() -> Self {
        Self {
            open: false,
            draft: Draft::default(),
            dirty: false,
            pending: None,
            error: None,
            status: String::new(),
            recent: Vec::new(),
            paste_open: false,
            paste: String::new(),
        }
    }
}
impl Default for EditorState {
    fn default() -> Self {
        let mut editor = Self::empty();
        editor.recent = recent_path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        editor
    }
}
fn write_preset(path: &std::path::Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    let mut file =
        tempfile::NamedTempFile::new_in(path.parent().unwrap_or_else(|| std::path::Path::new(".")))
            .map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
fn recent_path() -> Option<PathBuf> {
    crate::import::settings_path().map(|p| p.with_file_name("recent-presets.json"))
}
impl EditorState {
    fn remember(&mut self, path: PathBuf) {
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(12);
        if let Some(p) = recent_path() {
            let result = (|| -> Result<(), String> {
                std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
                std::fs::write(
                    p,
                    serde_json::to_vec(&self.recent).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())
            })();
            if let Err(e) = result {
                log::warn!("Cannot save recent presets: {e}");
            }
        }
    }
    fn replace(&mut self, draft: Draft) {
        if self.dirty {
            self.pending = Some(draft);
        } else {
            self.install(draft);
        }
    }
    fn install(&mut self, draft: Draft) {
        if let Some(p) = &draft.path {
            self.remember(p.clone());
        }
        self.draft = draft;
        self.dirty = false;
        self.error = None;
        self.status = "Preset opened. Apply it when you are ready.".into();
    }
    fn save(&mut self, save_as: bool) -> bool {
        let text = match self.draft.text() {
            Ok(s) => s,
            Err(e) => {
                self.error = Some(e);
                return false;
            }
        };
        let path = if save_as || self.draft.path.is_none() {
            let name = self
                .draft
                .name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>();
            let mut dialog = rfd::FileDialog::new()
                .set_title("Save analyte preset")
                .add_filter("Chromascope preset", &["toml"])
                .set_file_name(format!("{name}.toml"));
            if let Some(parent) = self.draft.path.as_ref().and_then(|p| p.parent()) {
                dialog = dialog.set_directory(parent);
            }
            dialog.save_file()
        } else {
            self.draft.path.clone()
        };
        let Some(path) = path else {
            return false;
        };
        if let Err(e) = write_preset(&path, &text) {
            self.error = Some(format!("Cannot save preset: {e}"));
            return false;
        }
        self.remember(path.clone());
        self.status = format!("Saved to {}", path.display());
        self.draft.path = Some(path);
        self.dirty = false;
        self.error = None;
        true
    }
}
pub(super) fn open_file(app: &mut MzViewerApp) {
    app.presets.editor.open = true;
    if let Some(path) = rfd::FileDialog::new()
        .set_title("Open analyte preset")
        .add_filter("Chromascope preset", &["toml"])
        .pick_file()
    {
        match Draft::read(path) {
            Ok(d) => app.presets.editor.replace(d),
            Err(e) => app.presets.editor.error = Some(e),
        }
    }
}
fn cell(ui: &mut egui::Ui, text: &mut String, width: f32) -> bool {
    ui.add(egui::TextEdit::singleline(text).desired_width(width))
        .changed()
}
fn pasted_rows(text: &str) -> Result<Vec<Row>, String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header = lines
        .next()
        .ok_or("Paste a table with a header row.")?
        .split('\t')
        .map(|s| s.trim().to_lowercase())
        .collect::<Vec<_>>();
    let name = header
        .iter()
        .position(|s| s == "name")
        .ok_or("The first row needs a Name column.")?;
    let mass = header
        .iter()
        .position(|s| {
            ["mass", "target m/z", "product m/z", "target/product m/z"].contains(&s.as_str())
        })
        .ok_or("The first row needs a Mass or Target m/z column.")?;
    let mut rows = Vec::new();
    for (i, line) in lines.enumerate() {
        let cells = line.split('\t').collect::<Vec<_>>();
        let get = |index: usize| cells.get(index).map(|s| s.trim()).unwrap_or("");
        let mut row = Row {
            name: get(name).into(),
            mass: get(mass).into(),
            ..Default::default()
        };
        for (col, key) in header.iter().enumerate() {
            let value = get(col);
            if value.is_empty() {
                continue;
            }
            match key.as_str() {
                "acquisition" => {
                    row.acquisition = match value.to_uppercase().as_str() {
                        "FS" => AcquisitionMode::FS,
                        "SIM" => AcquisitionMode::SIM,
                        "MRM" => AcquisitionMode::MRM,
                        _ => {
                            return Err(format!(
                                "Pasted row {}: acquisition must be FS, SIM, or MRM.",
                                i + 1
                            ))
                        }
                    }
                }
                "precursor m/z" | "precursor_mz" => row.precursor = value.into(),
                "ppm" | "tolerance (ppm)" => row.ppm = value.into(),
                "smoothing" => row.smoothing = value.into(),
                "polarity" => {
                    row.polarity = value.to_lowercase();
                    if !["positive", "negative"].contains(&row.polarity.as_str()) {
                        return Err(format!("Pasted row {}: invalid polarity.", i + 1));
                    }
                }
                "kind" => row.kind = value.to_uppercase(),
                _ => {}
            }
        }
        row.spec()
            .map_err(|e| format!("Pasted row {}: {e}", i + 1))?;
        rows.push(row);
    }
    if rows.is_empty() {
        return Err("No analyte rows found.".into());
    }
    Ok(rows)
}
fn labeled_cell(ui: &mut egui::Ui, label: &str, value: &mut String, width: f32) -> bool {
    ui.vertical(|ui| {
        ui.small(label);
        cell(ui, value, width)
    })
    .inner
}

fn analyte_rows(ui: &mut egui::Ui, rows: &mut [Row], dirty: &mut bool) -> Option<(usize, u8)> {
    let mut action = None;
    let count = rows.len();
    for (i, row) in rows.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.strong(format!("Analyte {}", i + 1));
                    *dirty |= cell(ui, &mut row.name, 180.0);
                    if ui
                        .add_enabled(count < 64, egui::Button::new("Duplicate"))
                        .clicked()
                    {
                        action = Some((i, 0));
                    }
                    if ui
                        .add_enabled(i > 0, egui::Button::new("Move up"))
                        .clicked()
                    {
                        action = Some((i, 1));
                    }
                    if ui
                        .add_enabled(i + 1 < count, egui::Button::new("Move down"))
                        .clicked()
                    {
                        action = Some((i, 2));
                    }
                    if ui.button("Delete").clicked() {
                        action = Some((i, 3));
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let before = (row.acquisition, row.kind.clone(), row.polarity.clone());
                    ui.vertical(|ui| {
                        ui.small("Acquisition");
                        egui::ComboBox::from_id_salt("mode")
                            .width(65.0)
                            .selected_text(format!("{:?}", row.acquisition))
                            .show_ui(ui, |ui| {
                                for mode in [
                                    AcquisitionMode::FS,
                                    AcquisitionMode::SIM,
                                    AcquisitionMode::MRM,
                                ] {
                                    ui.selectable_value(
                                        &mut row.acquisition,
                                        mode,
                                        format!("{mode:?}"),
                                    );
                                }
                            });
                    });
                    ui.vertical(|ui| {
                        ui.small("Trace");
                        egui::ComboBox::from_id_salt("kind")
                            .width(55.0)
                            .selected_text(&row.kind)
                            .show_ui(ui, |ui| {
                                for kind in ["XIC", "TIC", "BPC"] {
                                    ui.selectable_value(&mut row.kind, kind.into(), kind);
                                }
                            });
                    });
                    if row.acquisition == AcquisitionMode::MRM || !row.precursor.is_empty() {
                        *dirty |= labeled_cell(ui, "Precursor m/z", &mut row.precursor, 100.0);
                    }
                    if row.kind == "XIC" {
                        *dirty |= labeled_cell(ui, "Target/product m/z", &mut row.mass, 120.0);
                        *dirty |= labeled_cell(ui, "Tolerance (ppm)", &mut row.ppm, 90.0);
                    } else {
                        *dirty |= labeled_cell(ui, "Range min", &mut row.range_min, 90.0);
                        *dirty |= labeled_cell(ui, "Range max", &mut row.range_max, 90.0);
                    }
                    *dirty |= labeled_cell(ui, "Smoothing", &mut row.smoothing, 70.0);
                    ui.vertical(|ui| {
                        ui.small("Polarity");
                        egui::ComboBox::from_id_salt("polarity")
                            .width(85.0)
                            .selected_text(&row.polarity)
                            .show_ui(ui, |ui| {
                                for polarity in ["positive", "negative"] {
                                    ui.selectable_value(
                                        &mut row.polarity,
                                        polarity.into(),
                                        polarity,
                                    );
                                }
                            });
                    });
                    *dirty |= labeled_cell(ui, "MS level", &mut row.ms_level, 60.0);
                    *dirty |= before != (row.acquisition, row.kind.clone(), row.polarity.clone());
                });
                if let Err(error) = row.spec() {
                    if row.mass.trim().is_empty() && row.kind == "XIC" {
                        ui.weak("Enter a target m/z to configure this analyte.");
                    } else {
                        ui.colored_label(ui.visuals().error_fg_color, error);
                    }
                }
            });
            ui.add_space(6.0);
        });
    }
    action
}

pub(super) fn show(app: &mut MzViewerApp, ctx: &egui::Context) {
    let mut editor = std::mem::replace(&mut app.presets.editor, EditorState::empty());
    let mut open = editor.open;
    let mut apply = None;
    egui::Window::new("Viewer extraction preset").max_height((ctx.content_rect().height()-60.0).max(120.0)).vscroll(true).open(&mut open).default_size(egui::vec2(900.0,650.0)).resizable(true).show(ctx,|ui|{
        ui.horizontal_wrapped(|ui|{
            if ui.button("New").clicked(){editor.replace(Draft::default());}
            if ui.button("Open…").clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("Chromascope preset",&["toml"]).pick_file(){match Draft::read(path){Ok(d)=>editor.replace(d),Err(e)=>editor.error=Some(e)}}}
            ui.menu_button("Recent presets",|ui|{for path in editor.recent.clone(){if ui.button(path.file_stem().unwrap_or_default().to_string_lossy()).on_hover_text(path.display().to_string()).clicked(){match Draft::read(path){Ok(d)=>editor.replace(d),Err(e)=>editor.error=Some(e)}ui.close();}}});
            if ui.button("Save").clicked(){editor.save(false);}
            if ui.button("Save As…").clicked(){editor.save(true);}
            if let Some(result)=super::examples::menu(ui) { match result {
                Ok(path)=>{editor.error=None;editor.status=format!("Example saved to {}. Open extraction presets here; quantification methods in Batch Quantification → Edit method.",path.display());},
                Err(e)=>editor.error=Some(e),
            } }
            if ui.add_enabled(!app.async_state.is_processing && app.active_file_id.is_some(),egui::Button::new("Apply to viewer samples")).on_hover_text("Extract these traces across the viewer samples; use Batch Quantification for automated integration").clicked(){match editor.draft.text(){Ok(text)=>apply=Some(text),Err(e)=>editor.error=Some(e)}}
            if editor.dirty{ui.label("Unsaved changes");}
        });
        ui.horizontal_wrapped(|ui|{ui.label("Preset name");editor.dirty|=cell(ui,&mut editor.draft.name,240.0);ui.label("Rows");editor.dirty|=ui.add(egui::DragValue::new(&mut editor.draft.grid_rows).range(1..=8)).changed();ui.label("Columns");editor.dirty|=ui.add(egui::DragValue::new(&mut editor.draft.columns).range(1..=8)).changed();editor.dirty|=ui.checkbox(&mut editor.draft.overlay,"Overlay analytes").changed();});
        if let Some(path)=&editor.draft.path{ui.small(path.display().to_string());}else{ui.small("Save As lets you choose any folder for this preset.");}
        let mut action=None;
        egui::ScrollArea::vertical().id_salt("preset_table").max_height(420.0).auto_shrink([false, true]).show(ui, |ui| {
            action = analyte_rows(ui, &mut editor.draft.rows, &mut editor.dirty);
        });
        if let Some((i,op))=action{match op{0 if editor.draft.rows.len()<64=>{let mut row=editor.draft.rows[i].clone();row.name.push_str(" copy");editor.draft.rows.insert(i+1,row);},1 if i>0=>editor.draft.rows.swap(i,i-1),2 if i+1<editor.draft.rows.len()=>editor.draft.rows.swap(i,i+1),3=>{editor.draft.rows.remove(i);},_=>{}}editor.dirty=true;}
        ui.horizontal_wrapped(|ui|{if ui.add_enabled(editor.draft.rows.len()<64,egui::Button::new("Add analyte")).clicked(){editor.draft.rows.push(Row::default());editor.dirty=true;}ui.toggle_value(&mut editor.paste_open,"Paste from Excel…");ui.small("Blank MS level uses FS/SIM = 1, MRM = 2. MRM target is the product ion.");});
        if editor.paste_open{ui.label("Paste tab-separated cells with headers: Name, Acquisition, Precursor m/z, Mass, ppm, Smoothing, Polarity.");ui.add(egui::TextEdit::multiline(&mut editor.paste).desired_rows(4));if ui.button("Append pasted analytes").clicked(){match pasted_rows(&editor.paste){Ok(rows) if editor.draft.rows.len()+rows.len()<=64=>{editor.draft.rows.extend(rows);editor.dirty=true;editor.paste.clear();editor.paste_open=false;editor.error=None;},Ok(_)=>editor.error=Some("A preset supports at most 64 analytes.".into()),Err(e)=>editor.error=Some(e)}}}
        if let Some(error)=&editor.error{ui.colored_label(ui.visuals().error_fg_color,error);}
        if !editor.status.is_empty(){ui.small(&editor.status);}
    });
    editor.open = open;
    if editor.pending.is_some() {
        egui::Window::new("Unsaved preset changes")
            .max_height((ctx.content_rect().height() - 60.0).max(120.0))
            .vscroll(true)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("Save your changes before replacing this preset?");
                ui.horizontal(|ui| {
                    if ui.button("Save and continue").clicked() && editor.save(false) {
                        let next = editor.pending.take().unwrap();
                        editor.install(next);
                    }
                    if ui.button("Discard changes").clicked() {
                        let next = editor.pending.take().unwrap();
                        editor.install(next);
                    }
                    if ui.button("Cancel").clicked() {
                        editor.pending = None;
                    }
                });
            });
    }
    if let Some(text) = apply {
        match super::presets::apply_document(app, &text) {
            Ok(()) => {
                editor.error = None;
                editor.status =
                    "Applied to viewer samples. Select a sample to inspect its analytes.".into();
            }
            Err(e) => editor.error = Some(e),
        }
    }
    app.presets.editor = editor;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn analytes_render_below_each_other_at_narrow_and_wide_widths() {
        for width in [480.0, 1100.0] {
            let ctx = egui::Context::default();
            let mut rows = vec![Row::default(), Row::default(), Row::default()];
            let mut dirty = false;
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 1000.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        analyte_rows(ui, &mut rows, &mut dirty);
                    });
                },
            );
            let positions: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let egui::Shape::Text(text) = &shape.shape {
                        if text.galley.text().starts_with("Analyte ") {
                            return Some(text.pos);
                        }
                    }
                    None
                })
                .collect();
            assert_eq!(positions.len(), 3);
            assert!(positions
                .windows(2)
                .all(|p| p[1].y > p[0].y && (p[1].x - p[0].x).abs() < 1.0));
            assert!(!dirty);
        }
    }

    #[test]
    fn editor_round_trip_preserves_transition_filters_and_grid() {
        let draft = Draft {
            name: "Transitions".into(),
            rows: vec![Row {
                name: "Analyte A".into(),
                acquisition: AcquisitionMode::MRM,
                mass: "100.0".into(),
                precursor: "483.2383".into(),
                ppm: "5".into(),
                smoothing: "3".into(),
                polarity: "negative".into(),
                ms_level: "2".into(),
                ..Default::default()
            }],
            overlay: false,
            grid_rows: 3,
            columns: 3,
            path: None,
        };
        let text = draft.text().unwrap();
        let p: Preset = toml::from_str(&text).unwrap();
        assert_eq!(p.name, "Transitions");
        assert_eq!(p.columns, Some(3));
        assert_eq!(p.traces[0].precursor_mz, Some(483.2383));
        assert_eq!(p.traces[0].polarity, "negative");
        let (_, specs) =
            super::super::presets::parse(&text, &crate::validation::DataBounds::unrestricted())
                .unwrap();
        assert_eq!(specs[0].1.xic_params.unwrap().mass(), 100.0);
    }
    #[test]
    fn saved_file_can_be_reopened_and_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("analytes.toml");
        let mut draft = Draft::default();
        draft.rows[0].mass = "483.2383".into();
        write_preset(&path, &draft.text().unwrap()).unwrap();
        let loaded = Draft::read(path.clone()).unwrap();
        assert_eq!(loaded.rows[0].mass, "483.2383");
        draft.name = "Updated analytes".into();
        write_preset(&path, &draft.text().unwrap()).unwrap();
        assert_eq!(Draft::read(path).unwrap().name, "Updated analytes");
    }

    #[test]
    fn replacing_dirty_draft_preserves_changes_until_decision() {
        let mut editor = EditorState::empty();
        editor.draft.name = "Unsaved analytes".into();
        editor.dirty = true;
        editor.replace(Draft::default());
        assert_eq!(editor.draft.name, "Unsaved analytes");
        assert!(editor.pending.is_some());
        let replacement = editor.pending.take().unwrap();
        editor.install(replacement);
        assert!(!editor.dirty);
        assert_eq!(editor.draft.name, "My analyte preset");
    }

    #[test]
    fn invalid_cells_and_excel_rows_are_rejected_without_silent_defaults() {
        let row = Row {
            mass: "wrong".into(),
            ..Default::default()
        };
        assert!(row.spec().is_err());
        let rows=pasted_rows("Name\tAcquisition\tPrecursor m/z\tMass\tppm\tSmoothing\tPolarity\nA\tMRM\t483.0\t100.0\t5\t2\tnegative").unwrap();
        assert_eq!(rows[0].spec().unwrap().precursor_mz, Some(483.0));
        assert!(pasted_rows("Name\tMass\nA\twrong").is_err());
        assert!(pasted_rows("Name\tMass\tPolarity\nA\t100\tunknown").is_err());
    }
}
