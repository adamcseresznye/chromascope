//! CSV sample sheet and level table for targeted batches: sample type, dilution and calibration
//! level are typed once in a spreadsheet instead of cell by cell in the app. A level (1, 2, QC-low…)
//! names a set of known concentrations, one per analyte, kept in a separate level table.
use crate::domain::{EngineError, Result};
use crate::targeted::Role;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

fn invalid(message: impl ToString) -> EngineError {
    EngineError::new("invalid_parameters", message)
}
/// One injection of the batch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub source: String,
    pub id: String,
    pub role: Role,
    pub dilution: f64,
    /// Name of a [`Level`]; empty for samples without known concentrations.
    pub level: String,
}
impl Row {
    pub fn new(source: &str) -> Self {
        Row {
            source: source.into(),
            id: stem(source),
            role: Role::Unknown,
            dilution: 1.0,
            level: String::new(),
        }
    }
}
/// Known concentrations of the analytes in one prepared solution.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Level {
    pub name: String,
    /// Analyte name -> concentration in that analyte's unit.
    pub conc: BTreeMap<String, f64>,
}

fn fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' | ';' | '\t' if !quoted => out.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    out.push(field);
    out.into_iter().map(|f| f.trim().to_owned()).collect()
}
pub fn stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned())
}
fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned())
}
pub fn role_name(role: &Role) -> &'static str {
    match role {
        Role::Standard => "standard",
        Role::Blank => "blank",
        Role::Qc => "qc",
        Role::Unknown => "unknown",
    }
}
fn parse_role(text: &str) -> Option<Role> {
    match text.to_lowercase().as_str() {
        "standard" | "std" | "cal" | "calibration" => Some(Role::Standard),
        "blank" => Some(Role::Blank),
        "qc" => Some(Role::Qc),
        "unknown" | "sample" => Some(Role::Unknown),
        _ => None,
    }
}
fn clean(name: &str) -> String {
    name.replace([',', ';', '"', '\t'], " ")
}
fn table(text: &str) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let mut lines = text
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|l| !l.trim().is_empty());
    let header = fields(lines.next().ok_or_else(|| invalid("The file is empty"))?);
    Ok((header, lines.map(fields).collect()))
}
fn find(header: &[String], names: &[&str]) -> Option<usize> {
    header
        .iter()
        .position(|h| names.iter().any(|n| h.eq_ignore_ascii_case(n)))
}

/// A sheet listing the injections, ready to be filled in a spreadsheet.
pub fn template(rows: &[Row]) -> String {
    let mut out = String::from("sample type,file name,sample id,dilution,level\n");
    for r in rows {
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            role_name(&r.role),
            clean(&stem(&r.source)),
            clean(&r.id),
            r.dilution,
            clean(&r.level)
        ));
    }
    out
}
/// Fills sample type, sample ID, dilution and level of `rows` from a sheet with the columns
/// `sample type, file name, sample id, dilution, level`. Rows are matched by file name (with or
/// without extension, case-insensitive); the injection order becomes the order of the sheet.
/// Unknown columns and unmatched rows are reported in the returned notes, never created. `rows`
/// is left untouched when the sheet has an error.
pub fn apply(rows: &mut Vec<Row>, text: &str) -> Result<Vec<String>> {
    let (header, body) = table(text)?;
    let file_col = find(&header, &["file name", "file", "filename"])
        .ok_or_else(|| invalid("The sheet needs a 'file name' column"))?;
    let type_col = find(&header, &["sample type", "type", "role"]);
    let id_col = find(&header, &["sample id", "id", "sample"]);
    let dilution_col = find(&header, &["dilution"]);
    let level_col = find(&header, &["level"]);
    let known = [Some(file_col), type_col, id_col, dilution_col, level_col];
    let mut notes: Vec<String> = header
        .iter()
        .enumerate()
        .filter(|(i, h)| !h.is_empty() && !known.contains(&Some(*i)))
        .map(|(_, h)| format!("Column '{h}' is not used"))
        .collect();
    let mut updated = rows.clone();
    let mut order = Vec::new();
    for (n, row) in body.iter().enumerate() {
        let line = n + 2;
        let label = row.get(file_col).cloned().unwrap_or_default();
        let key = label.to_lowercase();
        let Some(index) = updated.iter().position(|r| {
            file_name(&r.source).to_lowercase() == key || stem(&r.source).to_lowercase() == key
        }) else {
            notes.push(format!(
                "Row {line}: '{label}' is not among the loaded samples"
            ));
            continue;
        };
        if order.contains(&index) {
            return Err(invalid(format!("File '{label}' appears twice")));
        }
        order.push(index);
        let cell = |c: Option<usize>| c.and_then(|c| row.get(c)).filter(|v| !v.is_empty());
        let r = &mut updated[index];
        if let Some(v) = cell(type_col) {
            r.role = parse_role(v)
                .ok_or_else(|| invalid(format!("Row {line}: unknown sample type '{v}'")))?;
        }
        if let Some(v) = cell(id_col) {
            r.id = v.clone();
        }
        if let Some(v) = cell(dilution_col) {
            r.dilution = v
                .parse()
                .map_err(|_| invalid(format!("Row {line}: dilution '{v}' is not a number")))?;
        }
        r.level = cell(level_col).cloned().unwrap_or_default();
    }
    let mut seen = HashSet::new();
    if let Some(r) = updated.iter().find(|r| !seen.insert(r.id.clone())) {
        return Err(invalid(format!("Sample ID '{}' is used twice", r.id)));
    }
    for (i, r) in updated.iter().enumerate() {
        if !order.contains(&i) {
            notes.push(format!(
                "Loaded sample '{}' is not in the sheet",
                stem(&r.source)
            ));
        }
    }
    // Sheet order first, then anything the sheet did not mention.
    let mut sorted: Vec<Row> = order.iter().map(|&i| updated[i].clone()).collect();
    sorted.extend(
        updated
            .iter()
            .enumerate()
            .filter(|(i, _)| !order.contains(i))
            .map(|(_, r)| r.clone()),
    );
    *rows = sorted;
    Ok(notes)
}

/// Level table template: one column per analyte.
pub fn levels_template(levels: &[Level], analytes: &[String]) -> String {
    let mut out = String::from("level");
    for a in analytes {
        out.push(',');
        out.push_str(&clean(a));
    }
    out.push('\n');
    for l in levels {
        out.push_str(&clean(&l.name));
        for a in analytes {
            out.push(',');
            if let Some(v) = l.conc.get(a) {
                out.push_str(&v.to_string());
            }
        }
        out.push('\n');
    }
    out
}
/// Fills the level table from a sheet `level, <analyte>, <analyte>…` (concentrations). Levels
/// are updated by name and added when new; columns matching no analyte are reported.
pub fn apply_levels(
    levels: &mut Vec<Level>,
    analytes: &[String],
    text: &str,
) -> Result<Vec<String>> {
    let (header, body) = table(text)?;
    let level_col = find(&header, &["level"])
        .ok_or_else(|| invalid("The level sheet needs a 'level' column"))?;
    let mut notes = Vec::new();
    let mut columns = Vec::new();
    for (i, h) in header
        .iter()
        .enumerate()
        .filter(|(i, h)| *i != level_col && !h.is_empty())
    {
        match analytes.iter().find(|a| a.eq_ignore_ascii_case(h)) {
            Some(a) => columns.push((i, a.clone())),
            None => notes.push(format!("Column '{h}' matches no analyte and was ignored")),
        }
    }
    let mut updated = levels.clone();
    for (n, row) in body.iter().enumerate() {
        let name = row.get(level_col).cloned().unwrap_or_default();
        if name.is_empty() {
            return Err(invalid(format!("Row {}: the level has no name", n + 2)));
        }
        let index = updated
            .iter()
            .position(|l| l.name == name)
            .unwrap_or_else(|| {
                updated.push(Level {
                    name: name.clone(),
                    conc: BTreeMap::new(),
                });
                updated.len() - 1
            });
        for (c, analyte) in &columns {
            if let Some(v) = row.get(*c).filter(|v| !v.is_empty()) {
                let value: f64 = v.parse().map_err(|_| {
                    invalid(format!(
                        "Row {}: concentration '{v}' is not a number",
                        n + 2
                    ))
                })?;
                if !value.is_finite() || value < 0.0 {
                    return Err(invalid(format!(
                        "Row {}: concentration '{v}' is not valid",
                        n + 2
                    )));
                }
                updated[index].conc.insert(analyte.clone(), value);
            }
        }
    }
    *levels = updated;
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rows() -> Vec<Row> {
        ["261009-10", "261009-7", "261009-15"]
            .iter()
            .map(|f| Row::new(&format!("C:/data/{f}.mzML")))
            .collect()
    }
    #[test]
    fn the_users_sheet_layout_fills_type_id_dilution_level_and_order() {
        let mut r = rows();
        let sheet = "sample type\tfile name\tsample id\tinj vol\tdilution\tlevel\n\
                     standard\t261009-7\tsample 1\t1\t1\t1\n\
                     standard\t261009-10\tsample 4\t1\t2\t4\n\
                     unknown\t261009-15\tsample 9\t1\t1\t\n";
        let notes = apply(&mut r, sheet).unwrap();
        assert_eq!(
            r.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["sample 1", "sample 4", "sample 9"]
        );
        assert_eq!(r[0].role, Role::Standard);
        assert_eq!(r[0].level, "1");
        assert_eq!(r[1].dilution, 2.0);
        assert_eq!(r[2].role, Role::Unknown);
        assert_eq!(r[2].level, "");
        assert_eq!(notes, ["Column 'inj vol' is not used"]);
    }
    #[test]
    fn unmatched_rows_are_reported_and_bad_values_change_nothing() {
        let mut r = rows();
        let notes = apply(&mut r, "file name,sample type\nGhost,standard\n").unwrap();
        assert!(notes.iter().any(|n| n.contains("Ghost")));
        assert!(notes.iter().any(|n| n.contains("261009-7")));
        let before = r.clone();
        assert!(apply(&mut r, "sample type\nstandard\n").is_err());
        assert!(apply(&mut r, "file name,sample type\n261009-7,wizard\n").is_err());
        assert!(apply(&mut r, "file name,dilution\n261009-7,abc\n").is_err());
        assert!(apply(&mut r, "file name\n261009-7\n261009-7\n").is_err());
        assert!(apply(&mut r, "file name,sample id\n261009-7,a\n261009-10,a\n").is_err());
        assert_eq!(r, before);
    }
    #[test]
    fn the_template_round_trips() {
        let mut r = rows();
        let sheet = template(&r);
        assert!(sheet.starts_with("sample type,file name,sample id,dilution,level\n"));
        assert!(apply(&mut r, &sheet).unwrap().is_empty());
    }
    #[test]
    fn levels_are_merged_by_name_per_analyte() {
        let analytes = ["Analyte".to_owned(), "IS".to_owned()];
        let mut levels = vec![Level {
            name: "1".into(),
            conc: BTreeMap::from([("IS".to_owned(), 5.0)]),
        }];
        let notes = apply_levels(
            &mut levels,
            &analytes,
            "level,analyte,Other\n1,0.5,9\n2,1,\n",
        )
        .unwrap();
        assert_eq!(levels.len(), 2);
        assert_eq!(levels[0].conc["Analyte"], 0.5);
        assert_eq!(levels[0].conc["IS"], 5.0);
        assert_eq!(levels[1].conc["Analyte"], 1.0);
        assert_eq!(notes, ["Column 'Other' matches no analyte and was ignored"]);
        let before = levels.clone();
        assert!(apply_levels(&mut levels, &analytes, "level,Analyte\n3,abc\n").is_err());
        assert!(apply_levels(&mut levels, &analytes, "level,Analyte\n3,-1\n").is_err());
        assert_eq!(levels, before);
        let sheet = levels_template(&levels, &analytes);
        assert!(apply_levels(&mut levels, &analytes, &sheet)
            .unwrap()
            .is_empty());
        assert_eq!(levels, before);
    }
}
