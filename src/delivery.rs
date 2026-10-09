//! Read-only reporting and portable append-only project delivery.
use crate::{
    domain::*,
    engine::{self, Output},
    jobs::JobControl,
    project::Project,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, io::Write, path::Path};
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub title: String,
    pub sections: Vec<String>,
    pub figure_width: u32,
    pub figure_height: u32,
}
pub const SECTIONS: &[&str] = &[
    "metadata",
    "methods",
    "chromatograms",
    "spectra",
    "calibration",
    "quantification",
    "qc",
    "annotations",
    "statistics",
    "evidence",
    "history",
    "versions",
];
impl Default for Config {
    fn default() -> Self {
        Self {
            title: "Chromascope analytical review draft".into(),
            sections: SECTIONS.iter().map(|s| (*s).into()).collect(),
            figure_width: 2400,
            figure_height: 1200,
        }
    }
}
fn err(e: impl ToString) -> EngineError {
    EngineError::new("adapter_failure", e)
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn validate(c: &Config) -> Result<()> {
    if c.title.trim().is_empty()
        || c.title.len() > 4096
        || !(320..=16384).contains(&c.figure_width)
        || !(240..=16384).contains(&c.figure_height)
        || c.sections.iter().any(|s| !SECTIONS.contains(&s.as_str()))
    {
        return Err(EngineError::new(
            "invalid_parameters",
            "Invalid report title, sections or figure dimensions",
        ));
    }
    Ok(())
}
/// Missing/changed sources do not prevent inspection of immutable retained results.
pub fn verification(project: &Project) -> Value {
    json!(project.sources.iter().map(|s| match project.verified_source(s.id) {
        Ok(_) => json!({"dataset_id":s.id,"path":s.path,"sha256":s.sha256,"bytes":s.bytes,"state":"verified"}),
        Err(e) => json!({"dataset_id":s.id,"path":s.path,"sha256":s.sha256,"bytes":s.bytes,"state":"unavailable_or_changed","error":e})
    }).collect::<Vec<_>>())
}
pub fn report(root: &Path, config: &Config) -> Result<Value> {
    validate(config)?;
    let p = Project::open(root)?;
    let results = p
        .results
        .iter()
        .map(|a| p.load_result(root, a.result_id))
        .collect::<Result<Vec<_>>>()?;
    Ok(
        json!({"delivery_schema_version":1,"status":"review_draft","config":config,"project":p,"source_verification":verification(&p),"software":{"chromascope":env!("CARGO_PKG_VERSION"),"cargo_lock_sha256":lock_hash()},"results":results,"limitations":["Review decisions are attributed, not authenticated scientific release approval.","Runtime/library versions are retained per result. Reprocessing requires those compatible runtimes."]}),
    )
}
fn lock_hash() -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(include_bytes!("../Cargo.lock")))
}
pub fn html(value: &Value) -> Result<String> {
    let c: Config = serde_json::from_value(value["config"].clone()).map_err(err)?;
    validate(&c)?;
    let mut h = format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><title>{}</title><style>body{{font:16px system-ui;max-width:1100px;margin:40px auto;padding:20px}}pre{{white-space:pre-wrap;overflow-wrap:anywhere;background:#f4f5f6;padding:16px}}svg{{width:100%;height:auto}}details{{margin:12px 0}}</style><h1>{}</h1><p>Review draft. Missing evidence and unresolved decisions remain in the full machine-readable results.</p>",escape(&c.title),escape(&c.title));
    for section in &c.sections {
        let content = match section.as_str() {
            "metadata" => {
                json!({"project_id":value["project"]["id"],"revision":value["project"]["revision"],"sources":value["source_verification"],"sample_metadata":value["results"].as_array().map(Vec::as_slice).unwrap_or(&[]).iter().filter_map(|r| {
                    ["/output/batch/request/samples","/output/report/config/samples","/output/report/table/samples","/output/report/study/samples","/request/operation/batch/samples"].iter().find_map(|pointer|r.pointer(pointer).map(|samples|json!({"result_id":r["result_id"],"pointer":pointer,"samples":samples})))
                }).collect::<Vec<_>>()})
            }
            "methods" => {
                json!({"methods":value["project"]["methods"],"requests":value["results"].as_array().unwrap_or(&vec![]).iter().map(|r| &r["request"]).collect::<Vec<_>>()})
            }
            "history" => value["project"].clone(),
            "versions" => {
                json!({"software":value["software"],"analytical_versions":value["results"].as_array().map(Vec::as_slice).unwrap_or(&[]).iter().map(|r|json!({"result_id":r["result_id"],"kernel_version":r["kernel_version"],"provenance":r.pointer("/output/report/provenance")})).collect::<Vec<_>>()})
            }
            "evidence" => value["results"].clone(),
            _ => {
                let rows = value["results"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let selected = rows
                    .iter()
                    .filter(|r| {
                        let kind = r["output"]["kind"].as_str().unwrap_or("");
                        match section.as_str() {
                            "chromatograms" => matches!(
                                kind,
                                "chromatogram"
                                    | "chromatographic_processing"
                                    | "targeted_quantification"
                                    | "feature_matrix"
                            ),
                            "spectra" => {
                                kind.contains("spectr")
                                    || matches!(kind, "feature_annotations" | "feature_matrix")
                            }
                            "calibration" | "quantification" => {
                                matches!(kind, "targeted_quantification" | "quantification")
                            }
                            "qc" => kind == "qc_report",
                            "annotations" => matches!(
                                kind,
                                "feature_annotations"
                                    | "spectral_search"
                                    | "formula_candidates"
                                    | "isotope_analysis"
                            ),
                            "statistics" => kind == "statistics",
                            _ => false,
                        }
                    })
                    .collect::<Vec<_>>();
                if selected.is_empty() {
                    json!({"state":"not_available","reason":"No corresponding retained analysis"})
                } else {
                    json!(selected)
                }
            }
        };
        h.push_str(&format!(
            "<h2>{}</h2><pre>{}</pre>",
            escape(section),
            escape(&serde_json::to_string_pretty(&content).map_err(err)?)
        ));
    }
    if c.sections
        .iter()
        .any(|s| s == "evidence" || s == "chromatograms" || s == "spectra")
    {
        for row in value["results"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            let response: engine::Response = serde_json::from_value(row.clone()).map_err(err)?;
            if let Output::Chromatogram {
                points,
                retention_time_unit,
                intensity_unit,
                ..
            } = &response.output
            {
                if !points.is_empty() {
                    h.push_str(&svg(points, retention_time_unit, intensity_unit, &c)?);
                }
            }
            for (name, figure) in analytical_figures(&response, &c)? {
                h.push_str(&format!(
                    "<details><summary>{}: {}</summary>{}</details>",
                    response.result_id.0,
                    escape(&name),
                    figure
                ));
            }
            for (name, csv) in table(&response)? {
                h.push_str(&format!(
                    "<details><summary>{}: {}</summary><pre>{}</pre></details>",
                    response.result_id.0,
                    escape(name),
                    escape(&csv)
                ));
            }
        }
    }
    h.push_str("</html>");
    Ok(h)
}
fn write(root: &Path, name: &str, bytes: &[u8]) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(root.join(name))
        .map_err(err)?;
    f.write_all(bytes).map_err(err)?;
    f.sync_all().map_err(err)
}
fn table(response: &engine::Response) -> Result<Vec<(&'static str, String)>> {
    Ok(match &response.output {
        Output::TargetedQuantification { batch } => vec![
            ("quantification", crate::targeted::csv(batch)?),
            ("calibration", crate::targeted::calibration_csv(batch)?),
        ],
        Output::QcReport { report } => vec![("qc", crate::qc::csv(report)?)],
        Output::Statistics { report } => {
            vec![("statistics", crate::statistics::export_csv(report)?)]
        }
        Output::FeatureMatrix { report } => vec![
            ("features", crate::untargeted::matrix_csv(report, false)?),
            ("observations", crate::untargeted::observations_csv(report)?),
        ],
        Output::FeatureAnnotations { ledger } => {
            vec![("annotations", crate::annotation::csv(ledger)?)]
        }
        Output::SpectralSearch { report } => vec![(
            "spectral_candidates",
            crate::spectral::candidates_csv(report)?,
        )],
        Output::ChromatographicProcessing { analyses, .. } => {
            vec![("peaks", crate::chromatography::peaks_csv(analyses))]
        }
        _ => vec![],
    })
}
fn analytical_figures(r: &engine::Response, c: &Config) -> Result<Vec<(String, String)>> {
    let mut figures = vec![];
    if let Output::TargetedQuantification { batch } = &r.output {
        for (i, o) in batch.observations.iter().enumerate() {
            for (j, m) in std::iter::once(&o.quantifier)
                .chain(o.qualifiers.iter())
                .enumerate()
            {
                if !m.trace.is_empty() {
                    figures.push((
                        format!("targeted-{i}-{j}"),
                        svg(&m.trace, "retention time (minutes)", "intensity", c)?,
                    ));
                }
            }
        }
        for (i, (target, model)) in batch.calibrations.iter().enumerate() {
            let ratio = batch
                .request
                .targets
                .iter()
                .find(|t| &t.id == target)
                .is_some_and(|t| t.internal_standard.is_some());
            let unit = if ratio {
                "area ratio (dimensionless)"
            } else {
                "intensity*minute"
            };
            let mut points = model
                .points
                .iter()
                .map(|p| [p.x, p.response])
                .collect::<Vec<_>>();
            points.sort_by(|a, b| a[0].total_cmp(&b[0]));
            if !points.is_empty() {
                figures.push((
                    format!("calibration-{i}-observed"),
                    svg(&points, model.unit.label(), unit, c)?,
                ));
            }
            let fit = (0..=256)
                .map(|j| {
                    let x = model.range[0] + (model.range[1] - model.range[0]) * j as f64 / 256.;
                    [x, model.response(x)]
                })
                .collect::<Vec<_>>();
            figures.push((
                format!("calibration-{i}-fitted"),
                svg(&fit, model.unit.label(), unit, c)?,
            ));
        }
    }
    if let Output::ChromatographicProcessing { analyses, .. } = &r.output {
        for (i, a) in analyses.iter().enumerate() {
            // Preserve missing gaps by exporting each existing contiguous segment separately.
            for (j, segment) in a.segments.iter().enumerate() {
                for (name, y) in [
                    ("raw", &segment.raw),
                    ("integration", &segment.integration_signal),
                ] {
                    if y.len() != segment.rt_minutes.len() {
                        return Err(EngineError::new("corrupt_project", "Signal lengths differ"));
                    }
                    let points = segment
                        .rt_minutes
                        .iter()
                        .zip(y)
                        .map(|(x, y)| [*x, *y])
                        .collect::<Vec<_>>();
                    if !points.is_empty() {
                        figures.push((
                            format!("chromatography-{i}-{j}-{name}"),
                            svg(&points, &a.rt_unit, &a.intensity_unit, c)?,
                        ));
                    }
                }
            }
        }
    }
    Ok(figures)
}
/// RFC4180-style delimiter conversion, preserving embedded commas, quotes and newlines.
pub fn tsv(csv: &str) -> Result<String> {
    let mut out = String::new();
    let mut quoted = false;
    let mut chars = csv.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            out.push(ch);
            if quoted && chars.peek() == Some(&'"') {
                out.push(chars.next().unwrap());
            } else {
                quoted = !quoted;
            }
        } else if ch == ',' && !quoted {
            out.push('\t');
        } else {
            out.push(ch);
        }
    }
    if quoted {
        return Err(EngineError::new(
            "invalid_parameters",
            "Unterminated CSV quote",
        ));
    }
    Ok(out)
}
pub fn svg(points: &[[f64; 2]], x_unit: &str, y_unit: &str, c: &Config) -> Result<String> {
    validate(c)?;
    engine::validate_points(points)?;
    if points.is_empty() {
        return Err(EngineError::new("invalid_parameters", "No figure points"));
    }
    let xmin = points[0][0];
    let xmax = points.last().unwrap()[0];
    let ymin = points.iter().map(|p| p[1]).fold(0.0, f64::min);
    let ymax = points.iter().map(|p| p[1]).fold(0.0, f64::max);
    let w = f64::from(c.figure_width);
    let h = f64::from(c.figure_height);
    let coords = points
        .iter()
        .map(|p| {
            format!(
                "{},{}",
                80.0 + (p[0] - xmin) / (xmax - xmin).max(f64::EPSILON) * (w - 120.0),
                h - 70.0 - (p[1] - ymin) / (ymax - ymin).max(f64::EPSILON) * (h - 110.0)
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    Ok(format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/><path d=\"M80 40 V{} H{}\" fill=\"none\" stroke=\"black\"/><polyline points=\"{}\" fill=\"none\" stroke=\"#176b87\" stroke-width=\"2\"/><text x=\"80\" y=\"25\">{}: {} to {}</text><text x=\"80\" y=\"{}\">{}: {} to {}</text></svg>",c.figure_width,c.figure_height,c.figure_width,c.figure_height,h-70.0,w-40.0,coords,escape(y_unit),ymin,ymax,h-20.0,escape(x_unit),xmin,xmax))
}
/// Centroid spectrum sticks do not imply signal between observed masses.
pub fn spectrum_svg(points: &[[f64; 2]], unit: &str, c: &Config) -> Result<String> {
    let mut figure = svg(points, "m/z", unit, c)?;
    let xmin = points[0][0];
    let xmax = points.last().unwrap()[0];
    let ymin = points.iter().map(|p| p[1]).fold(0.0, f64::min);
    let ymax = points.iter().map(|p| p[1]).fold(0.0, f64::max);
    let w = f64::from(c.figure_width);
    let h = f64::from(c.figure_height);
    let zero = h - 70.0 - (-ymin) / (ymax - ymin).max(f64::EPSILON) * (h - 110.0);
    let path = points
        .iter()
        .map(|p| {
            let x = 80.0 + (p[0] - xmin) / (xmax - xmin).max(f64::EPSILON) * (w - 120.0);
            let y = h - 70.0 - (p[1] - ymin) / (ymax - ymin).max(f64::EPSILON) * (h - 110.0);
            format!("M{x} {zero}V{y}")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let start = figure
        .find("<polyline")
        .ok_or_else(|| err("Missing spectrum plot"))?;
    let end = start
        + figure[start..]
            .find("/>")
            .ok_or_else(|| err("Missing spectrum plot end"))?
        + 2;
    figure.replace_range(
        start..end,
        &format!("<path d=\"{path}\" fill=\"none\" stroke=\"#176b87\" stroke-width=\"2\"/>"),
    );
    Ok(figure)
}
/// A new directory only. Failure leaves inspectable partial output; never replaces user files.
pub fn export(
    root: &Path,
    destination: &Path,
    config: &Config,
    include_sources: bool,
) -> Result<Value> {
    let _lock = crate::project::WriterLock::acquire(root)?;
    let value = report(root, config)?;
    let p = Project::open(root)?;
    if include_sources {
        for s in &p.sources {
            p.verified_source(s.id)?;
        }
    }
    fs::create_dir(destination).map_err(err)?;
    write(
        destination,
        "report.json",
        &serde_json::to_vec_pretty(&value).map_err(err)?,
    )?;
    write(destination, "report.html", html(&value)?.as_bytes())?;
    write(destination, "Cargo.lock", include_bytes!("../Cargo.lock"))?;
    write(
        destination,
        "INTEROPERABILITY.md",
        include_bytes!("../docs/PROJECT_DELIVERY.md"),
    )?;
    for a in &p.results {
        let r = p.load_result(root, a.result_id)?;
        for (name, figure) in analytical_figures(&r, config)? {
            write(
                destination,
                &format!("{}-{name}.svg", r.result_id.0),
                figure.as_bytes(),
            )?;
        }
        for (name, csv) in table(&r)? {
            write(
                destination,
                &format!("{}-{name}.csv", r.result_id.0),
                csv.as_bytes(),
            )?;
            write(
                destination,
                &format!("{}-{name}.tsv", r.result_id.0),
                tsv(&csv)?.as_bytes(),
            )?;
        }
        if let Output::Chromatogram {
            points,
            retention_time_unit,
            intensity_unit,
            ..
        } = &r.output
        {
            write(
                destination,
                &format!("{}.svg", r.result_id.0),
                svg(points, retention_time_unit, intensity_unit, config)?.as_bytes(),
            )?;
        }
        if let Output::Spectrum {
            spectrum,
            intensity_unit,
            ..
        } = &r.output
        {
            if spectrum.mz.len() != spectrum.intensity.len() {
                return Err(EngineError::new(
                    "corrupt_project",
                    "Spectrum array lengths differ",
                ));
            }
            let points = spectrum
                .mz
                .iter()
                .zip(&spectrum.intensity)
                .map(|(x, y)| [*x, f64::from(*y)])
                .collect::<Vec<_>>();
            if !points.is_empty() {
                write(
                    destination,
                    &format!("{}.svg", r.result_id.0),
                    spectrum_svg(&points, intensity_unit, config)?.as_bytes(),
                )?;
            }
        }
    }
    // Keep all original snapshots and result bytes, including superseded history.
    fs::create_dir(destination.join("results")).map_err(err)?;
    for entry in fs::read_dir(root).map_err(err)? {
        let entry = entry.map_err(err)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("revision-") && name.ends_with(".json") {
            write(destination, &name, &fs::read(entry.path()).map_err(err)?)?;
        }
    }
    for entry in fs::read_dir(root.join("results")).map_err(err)? {
        let entry = entry.map_err(err)?;
        if entry.path().is_file() {
            write(
                &destination.join("results"),
                &entry.file_name().to_string_lossy(),
                &fs::read(entry.path()).map_err(err)?,
            )?;
        }
    }
    if include_sources {
        fs::create_dir(destination.join("sources")).map_err(err)?;
        let mut map = BTreeMap::new();
        // Historical datasets must also travel; reject missing history rather than silently omit it.
        for entry in fs::read_dir(destination).map_err(err)? {
            let entry = entry.map_err(err)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("revision-") && name.ends_with(".json") {
                let old: Project =
                    serde_json::from_slice(&fs::read(entry.path()).map_err(err)?).map_err(err)?;
                for source in old.sources {
                    let path = p
                        .sources
                        .iter()
                        .find(|s| s.id == source.id)
                        .map(|s| s.path.clone())
                        .unwrap_or(source.path.clone());
                    if crate::project::source_identity(&path)?
                        != (source.sha256.clone(), source.bytes)
                    {
                        return Err(EngineError::new(
                            "corrupt_project",
                            "Historical source changed",
                        ));
                    }
                    let suffix = if path.extension().is_some_and(|e| e == "gz") {
                        "mzML.gz"
                    } else {
                        path.extension()
                            .and_then(|s| s.to_str())
                            .filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric()))
                            .unwrap_or("bin")
                    };
                    let relative = format!("sources/{}.{suffix}", source.sha256);
                    if !destination.join(&relative).exists() {
                        fs::copy(&path, destination.join(&relative)).map_err(err)?;
                    }
                    if crate::project::source_identity(&destination.join(&relative))?
                        != (source.sha256.clone(), source.bytes)
                    {
                        return Err(EngineError::new(
                            "corrupt_project",
                            "Source changed during copy",
                        ));
                    }
                    map.insert(source.path.to_string_lossy().into_owned(), relative);
                }
            }
        }
        write(
            destination,
            "source-map.json",
            &serde_json::to_vec_pretty(&map).map_err(err)?,
        )?;
    }
    if include_sources {
        // Deliver the local source tree as explicit software evidence; never claim build authentication.
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
        let software = destination.join("software");
        fs::create_dir(&software).map_err(err)?;
        for name in ["Cargo.toml", "Cargo.lock", "LICENSE"] {
            write(
                &software,
                name,
                &fs::read(repository.join(name)).map_err(err)?,
            )?;
        }
        for name in ["src", "docs", "examples", "assets"] {
            let original = repository.join(name);
            if original.is_dir() {
                copy_tree(&original, &software.join(name))?;
            }
        }
        write(&software,"SNAPSHOT.md",b"Local source snapshot from the build's repository path. This is not authenticated proof of binary/source equivalence. Build headless: cargo build --locked --no-default-features --features mcp-headless --bins. External scientific Python runtimes remain required as recorded in each result.
")?;
    }
    let mut checksums = BTreeMap::new();
    inventory(destination, destination, &mut checksums)?;
    write(
        destination,
        "checksums.json",
        &serde_json::to_vec_pretty(&checksums).map_err(err)?,
    )?;
    Ok(
        json!({"directory":destination,"project_id":p.id,"revision":p.revision,"files":checksums.len(),"sources_included":include_sources}),
    )
}
fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir(destination).map_err(err)?;
    for e in fs::read_dir(source).map_err(err)? {
        let e = e.map_err(err)?;
        let kind = e.file_type().map_err(err)?;
        if kind.is_symlink() {
            return Err(EngineError::new(
                "invalid_parameters",
                "Software snapshot symlinks forbidden",
            ));
        }
        if kind.is_dir() {
            copy_tree(&e.path(), &destination.join(e.file_name()))?;
        } else {
            write(
                destination,
                &e.file_name().to_string_lossy(),
                &fs::read(e.path()).map_err(err)?,
            )?;
        }
    }
    Ok(())
}
fn inventory(root: &Path, directory: &Path, hashes: &mut BTreeMap<String, String>) -> Result<()> {
    for e in fs::read_dir(directory).map_err(err)? {
        let e = e.map_err(err)?;
        if e.file_type().map_err(err)?.is_symlink() {
            return Err(EngineError::new(
                "corrupt_project",
                "Bundle symlinks forbidden",
            ));
        }
        if e.path().is_dir() {
            inventory(root, &e.path(), hashes)?;
        } else {
            hashes.insert(
                e.path()
                    .strip_prefix(root)
                    .map_err(err)?
                    .to_string_lossy()
                    .replace('\\', "/"),
                crate::project::source_identity(&e.path())?.0,
            );
        }
    }
    Ok(())
}
pub fn verify_bundle(root: &Path) -> Result<Value> {
    let expected: BTreeMap<String, String> =
        serde_json::from_slice(&fs::read(root.join("checksums.json")).map_err(err)?)
            .map_err(err)?;
    let mut actual = BTreeMap::new();
    inventory(root, root, &mut actual)?;
    actual.remove("checksums.json");
    if actual != expected {
        return Err(EngineError::new(
            "corrupt_project",
            "Bundle file inventory/checksums differ",
        ));
    }
    Ok(json!({"state":"verified","files":actual.len()}))
}
/// Resolve original absolute paths through a portable map. Constrain every mapping to bundle root.
pub(crate) fn relocate(root: &Path, p: &mut Project) -> Result<BTreeMap<String, String>> {
    let file = root.join("source-map.json");
    if !file.exists() {
        return Ok(BTreeMap::new());
    }
    let map: BTreeMap<String, String> =
        serde_json::from_slice(&fs::read(file).map_err(err)?).map_err(err)?;
    let base = fs::canonicalize(root).map_err(err)?;
    let mut resolved = BTreeMap::new();
    for (old, relative) in map {
        let rel = Path::new(&relative);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err(EngineError::new("corrupt_project", "Unsafe source mapping"));
        }
        let path = base.join(rel);
        if path.exists() && !fs::canonicalize(&path).map_err(err)?.starts_with(&base) {
            return Err(EngineError::new(
                "corrupt_project",
                "Source mapping escapes bundle",
            ));
        }
        resolved.insert(old, path.to_string_lossy().into_owned());
    }
    for s in &mut p.sources {
        if let Some(path) = resolved.get(&s.path.to_string_lossy().into_owned()) {
            s.path = path.into();
        }
    }
    Ok(resolved)
}
fn replace_paths(value: &mut Value, map: &BTreeMap<String, String>) {
    match value {
        Value::String(s) => {
            if let Some(new) = map.get(s) {
                *s = new.clone();
            }
        }
        Value::Array(a) => {
            for v in a {
                replace_paths(v, map)
            }
        }
        Value::Object(o) => {
            for v in o.values_mut() {
                replace_paths(v, map)
            }
        }
        _ => {}
    }
}
/// Recompute without changing original results. Compare analytical output exactly; IDs/timing stay separate.
pub fn reprocess(root: &Path, id: ResultId) -> Result<Value> {
    let mut p = Project::open(root)?;
    let old = p.load_result(root, id)?;
    if !old
        .kernel_version
        .starts_with(&format!("chromascope/{}/", env!("CARGO_PKG_VERSION")))
    {
        return Err(EngineError::new(
            "unsupported_capability",
            "Kernel version differs; preserve original runtime",
        ));
    }
    let map = relocate(root, &mut p)?;
    for s in &p.sources {
        p.verified_source(s.id)?;
    }
    let a = p
        .results
        .iter()
        .find(|a| a.result_id == id)
        .ok_or_else(|| err("Unknown result"))?;
    let mut request = serde_json::to_value(&old.request).map_err(err)?;
    replace_paths(&mut request, &map);
    let request: Request = serde_json::from_value(request).map_err(err)?;
    let new = engine::execute(
        &p.verified_source(a.dataset_id)?,
        request,
        &JobControl::default(),
    )?;
    let mut expected = serde_json::to_value(&old.output).map_err(err)?;
    replace_paths(&mut expected, &map);
    if new.kernel_version != old.kernel_version {
        return Err(EngineError::new(
            "unsupported_capability",
            "Analytical kernel changed",
        ));
    }
    let mut actual = serde_json::to_value(&new.output).map_err(err)?;
    let mut excluded = Vec::new();
    // Fresh targeted-batch identity/time are run metadata, not analytical evidence.
    if matches!(old.request.operation, Operation::TargetedBatch { .. }) {
        for pointer in ["/batch/batch_id", "/batch/created_unix_ms"] {
            if let (Some(a), Some(b)) = (expected.pointer_mut(pointer), actual.pointer_mut(pointer))
            {
                *a = Value::Null;
                *b = Value::Null;
                excluded.push(pointer);
            }
        }
    }
    Ok(
        json!({"original_result_id":id,"comparison":"exact_analytical_output","reproduced":expected==actual,"excluded_run_metadata_pointers":excluded,"response":new}),
    )
}
/// Parameter-level method comparison; no claim of analytical equivalence.
pub fn compare_methods(left: &Value, right: &Value) -> Value {
    fn walk(a: &Value, b: &Value, path: String, out: &mut Vec<Value>) {
        if a == b {
            return;
        }
        if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
            let keys = a
                .keys()
                .chain(b.keys())
                .collect::<std::collections::BTreeSet<_>>();
            for k in keys {
                if a.contains_key(k) != b.contains_key(k) {
                    out.push(json!({"pointer":format!("{path}/{}",k.replace('~',"~0").replace('/',"~1")),"left":a.get(k),"right":b.get(k),"left_state":if a.contains_key(k){"present"}else{"missing"},"right_state":if b.contains_key(k){"present"}else{"missing"}}));
                    continue;
                }
                walk(
                    a.get(k).unwrap_or(&Value::Null),
                    b.get(k).unwrap_or(&Value::Null),
                    format!("{path}/{}", k.replace('~', "~0").replace('/', "~1")),
                    out,
                );
            }
        } else {
            out.push(json!({"pointer":path,"left":a,"right":b}));
        }
    }
    let mut differences = vec![];
    walk(left, right, String::new(), &mut differences);
    json!({"equal":left==right,"differences":differences})
}

/// Scatter evidence permits signed axes, repeated x values and unsorted samples.
/// Points remain independent; the export never implies an interpolation path.
pub fn scatter_svg(points: &[[f64; 2]], x_unit: &str, y_unit: &str, c: &Config) -> Result<String> {
    validate(c)?;
    if points.is_empty() || points.iter().flatten().any(|v| !v.is_finite()) {
        return Err(err("Scatter points must be nonempty and finite"));
    }
    let bounds = |axis: usize| {
        let min = points.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
        let max = points
            .iter()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        if min == max {
            (min - 0.5, max + 0.5)
        } else {
            (min, max)
        }
    };
    let (xmin, xmax) = bounds(0);
    let (ymin, ymax) = bounds(1);
    let w = c.figure_width as f64;
    let h = c.figure_height as f64;
    let mut circles = String::new();
    for p in points {
        let x = 80. + (p[0] - xmin) / (xmax - xmin) * (w - 120.);
        let y = h - 70. - (p[1] - ymin) / (ymax - ymin) * (h - 110.);
        circles.push_str(&format!(
            "<circle cx=\"{x}\" cy=\"{y}\" r=\"4\" fill=\"#176b87\"><title>{}, {}</title></circle>",
            p[0], p[1]
        ));
    }
    Ok(format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {w} {h}\"><rect width=\"100%\" height=\"100%\" fill=\"white\"/><path d=\"M80 40 V{} H{}\" fill=\"none\" stroke=\"black\"/>{circles}<text x=\"80\" y=\"25\">{}: {ymin} to {ymax}</text><text x=\"80\" y=\"{}\">{}: {xmin} to {xmax}</text></svg>",c.figure_width,c.figure_height,h-70.,w-40.,escape(y_unit),h-20.,escape(x_unit)))
}

/// Independent named segments preserve gaps and dendrogram branches in SVG.
pub fn series_svg(
    series: &[(String, Vec<[f64; 2]>)],
    x_unit: &str,
    y_unit: &str,
    c: &Config,
) -> Result<String> {
    let points: Vec<_> = series
        .iter()
        .flat_map(|(_, points)| points.iter().copied())
        .collect();
    let mut svg = scatter_svg(&points, x_unit, y_unit, c)?;
    let bounds = |axis: usize| {
        let min = points.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
        let max = points
            .iter()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        if min == max {
            (min - 0.5, max + 0.5)
        } else {
            (min, max)
        }
    };
    let (xmin, xmax) = bounds(0);
    let (ymin, ymax) = bounds(1);
    let w = c.figure_width as f64;
    let h = c.figure_height as f64;
    let colors = ["#176b87", "#146e46", "#8c550f", "#942222"];
    let mut lines = String::new();
    for (i, (name, points)) in series.iter().enumerate() {
        let coordinates = points
            .iter()
            .map(|p| {
                format!(
                    "{},{}",
                    80. + (p[0] - xmin) / (xmax - xmin) * (w - 120.),
                    h - 70. - (p[1] - ymin) / (ymax - ymin) * (h - 110.)
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        lines.push_str(&format!("<g><title>{}</title><polyline points=\"{coordinates}\" fill=\"none\" stroke=\"{}\" stroke-width=\"2\"/></g>",escape(name),colors[i%colors.len()]));
    }
    let start = svg
        .find("<circle")
        .ok_or_else(|| err("Missing figure points"))?;
    let end = svg
        .rfind("</circle>")
        .ok_or_else(|| err("Missing figure points"))?
        + 9;
    svg.replace_range(start..end, &lines);
    Ok(svg)
}
