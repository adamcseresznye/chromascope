use super::*;
use sha2::{Digest, Sha256};
fn first<'a>(m: &'a BTreeMap<String, Vec<String>>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| m.get(*k).and_then(|v| v.first()).map(String::as_str))
}
fn numeric(value: Option<&str>) -> Result<Option<f64>> {
    value
        .map(|v| {
            v.split_whitespace()
                .next()
                .unwrap_or("")
                .parse::<f64>()
                .map_err(|_| invalid(format!("Invalid numeric metadata: {v}")))
        })
        .transpose()
}
fn energy(value: Option<&str>) -> Option<Energy> {
    let text = value?;
    let words: Vec<_> = text.split_whitespace().collect();
    if words.len() != 2 {
        return None;
    }
    let number = words[0].parse::<f64>().ok()?;
    if !number.is_finite() || number < 0.0 {
        return None;
    }
    Some(Energy {
        value: number,
        unit: words[1].into(),
    })
}
fn finish(
    metadata: BTreeMap<String, Vec<String>>,
    mut peaks: Vec<[f64; 2]>,
    index: usize,
    format: &str,
) -> Result<Entry> {
    if peaks.is_empty() {
        return Err(invalid("Library entry contains no peaks"));
    }
    peaks.sort_by(|a, b| a[0].total_cmp(&b[0]));
    let accession = first(&metadata, &["ACCESSION", "DB#", "SPECTRUMID", "TITLE"])
        .unwrap_or("")
        .to_string();
    let accession = if accession.is_empty() {
        format!("entry-{}", index + 1)
    } else {
        accession
    };
    let name = first(&metadata, &["CH$NAME", "NAME", "TITLE"])
        .unwrap_or(&accession)
        .to_string();
    let precursor_type = first(
        &metadata,
        &[
            "AC$MASS_SPECTROMETRY: PRECURSOR_TYPE",
            "MS$FOCUSED_ION: PRECURSOR_TYPE",
            "PRECURSORTYPE",
            "PRECURSOR_TYPE",
            "ADDUCT",
        ],
    )
    .map(str::to_string);
    let mode = first(
        &metadata,
        &[
            "AC$MASS_SPECTROMETRY: ION_MODE",
            "IONMODE",
            "ION_MODE",
            "POLARITY",
        ],
    );
    let polarity = match mode.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("positive" | "pos" | "+") => Polarity::Positive,
        Some("negative" | "neg" | "-") => Polarity::Negative,
        Some(_) => return Err(invalid("Unrecognized library polarity")),
        None => match first(&metadata, &["CHARGE"]).and_then(|s| s.chars().last()) {
            Some('+') => Polarity::Positive,
            Some('-') => Polarity::Negative,
            _ => Polarity::Unknown,
        },
    };
    let precursor_mz = numeric(first(
        &metadata,
        &[
            "MS$FOCUSED_ION: PRECURSOR_M/Z",
            "PRECURSORMZ",
            "PRECURSOR_MZ",
            "PEPMASS",
        ],
    ))?;
    let ms = first(
        &metadata,
        &["AC$MASS_SPECTROMETRY: MS_TYPE", "MSLEVEL", "MS_LEVEL"],
    )
    .unwrap_or("MS2")
    .trim_start_matches("MS")
    .parse::<u8>()
    .map_err(|_| invalid("Invalid library MS level"))?;
    let spectrum = Spectrum {
        id: accession.clone(),
        peaks,
        ms_level: ms,
        representation: Representation::Centroid,
        polarity,
        precursor_mz,
        precursor_type,
        collision_energy: energy(first(
            &metadata,
            &[
                "AC$MASS_SPECTROMETRY: COLLISION_ENERGY",
                "COLLISIONENERGY",
                "COLLISION_ENERGY",
            ],
        )),
        instrument: first(
            &metadata,
            &[
                "AC$INSTRUMENT_TYPE",
                "INSTRUMENTTYPE",
                "INSTRUMENT_TYPE",
                "INSTRUMENT",
            ],
        )
        .map(str::to_string),
        rt_minutes: numeric(first(&metadata, &["RETENTIONTIME_MINUTES"]))?,
        metadata: BTreeMap::from([
            ("library_format".into(), format.into()),
            ("mz_unit".into(), "m/z".into()),
            (
                "intensity_unit".into(),
                "library intensity (arbitrary units)".into(),
            ),
            ("rt_unit".into(), "minute".into()),
        ]),
    };
    spectrum.validate()?;
    Ok(Entry {
        accession,
        name,
        formula: first(&metadata, &["CH$FORMULA", "FORMULA"]).map(str::to_string),
        structure: first(&metadata, &["CH$SMILES", "SMILES", "INCHI"]).map(str::to_string),
        spectrum,
        metadata,
    })
}
/// Strict local import; source declaration is required, per-record metadata and original bytes survive.
pub fn import_library(raw_text: String, format: String, source: LibrarySource) -> Result<Library> {
    if raw_text.len() > 32 * 1024 * 1024 {
        return Err(EngineError::new(
            "resource_limit",
            "Library text exceeds 32 MiB",
        ));
    }
    if [&source.name, &source.version, &source.url, &source.license]
        .iter()
        .any(|s| s.trim().is_empty())
    {
        return Err(invalid(
            "Library requires source name, version, URL and explicit license (no inferred license)",
        ));
    }
    if !["msp", "mgf", "massbank"].contains(&format.as_str()) {
        return Err(EngineError::new(
            "unsupported_capability",
            "Supported library formats: msp, mgf, massbank",
        ));
    }
    let (mut entries, mut metadata, mut peaks) = (
        Vec::new(),
        BTreeMap::<String, Vec<String>>::new(),
        Vec::new(),
    );
    let (mut remaining, mut in_peaks, mut mgf_open) = (None::<usize>, false, false);
    let mut continuation = None::<String>;
    for (line_index, line) in raw_text.lines().enumerate() {
        let line = if line_index == 0 {
            line.trim_start_matches('\u{feff}')
        } else {
            line
        }
        .trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let start = format == "mgf" && line == "BEGIN IONS";
        let end = (format == "mgf" && line == "END IONS") || (format == "massbank" && line == "//");
        let new_msp = format == "msp"
            && line.to_ascii_uppercase().starts_with("NAME:")
            && !metadata.is_empty();
        if start {
            if mgf_open || !metadata.is_empty() {
                return Err(invalid("Nested MGF block"));
            }
            mgf_open = true;
            continue;
        }
        if end || new_msp {
            if remaining.is_some_and(|n| n != 0) {
                return Err(invalid("Declared library peak count differs"));
            }
            entries.push(finish(
                std::mem::take(&mut metadata),
                std::mem::take(&mut peaks),
                entries.len(),
                &format,
            )?);
            remaining = None;
            continuation = None;
            in_peaks = false;
            mgf_open = false;
            if end {
                continue;
            }
        }
        if format == "mgf" && !mgf_open {
            return Err(invalid("MGF data outside BEGIN/END IONS"));
        }
        if entries.len() >= 20_000 {
            return Err(EngineError::new(
                "resource_limit",
                "At most 20000 library entries",
            ));
        }
        if let Some((key, value)) = if format == "mgf" {
            line.split_once('=')
        } else {
            line.split_once(':')
        } {
            let mut key = key.trim().to_ascii_uppercase();
            let mut value = value.trim().to_string();
            if format == "massbank"
                && [
                    "AC$MASS_SPECTROMETRY",
                    "MS$FOCUSED_ION",
                    "AC$CHROMATOGRAPHY",
                ]
                .contains(&key.as_str())
            {
                let (sub, rest) = value
                    .split_once(' ')
                    .ok_or_else(|| invalid("MassBank subtag requires value"))?;
                key = format!("{key}: {sub}");
                value = rest.trim().into();
            }
            if ["NUM PEAKS", "NUMPEAKS", "PK$NUM_PEAK"].contains(&key.as_str()) {
                remaining = Some(value.parse().map_err(|_| invalid("Invalid peak count"))?);
                in_peaks = format == "msp";
            }
            if key == "PK$PEAK" {
                in_peaks = true;
                continuation = None;
            } else if key == "PK$ANNOTATION" {
                in_peaks = false;
                continuation = Some(key.clone());
            }
            metadata.entry(key).or_default().push(value);
            continue;
        }
        if in_peaks || format == "mgf" {
            if remaining == Some(0) {
                return Err(invalid("More peaks than declared"));
            }
            let values: Vec<_> = line.split_whitespace().collect();
            if values.len() < 2 {
                return Err(invalid(format!(
                    "Malformed peak on line {}",
                    line_index + 1
                )));
            }
            let mz = values[0]
                .parse::<f64>()
                .map_err(|_| invalid("Invalid peak m/z"))?;
            let intensity = values[1]
                .parse::<f64>()
                .map_err(|_| invalid("Invalid peak intensity"))?;
            peaks.push([mz, intensity]);
            if let Some(n) = remaining.as_mut() {
                *n -= 1;
            }
        } else if let Some(key) = &continuation {
            metadata.entry(key.clone()).or_default().push(line.into());
        } else {
            return Err(invalid(format!(
                "Unrecognized library line {}",
                line_index + 1
            )));
        }
    }
    if mgf_open {
        return Err(invalid("Unterminated MGF block"));
    }
    if !metadata.is_empty() {
        if format == "massbank" {
            return Err(invalid("MassBank record requires // terminator"));
        }
        if remaining.is_some_and(|n| n != 0) {
            return Err(invalid("Peak count mismatch"));
        }
        entries.push(finish(metadata, peaks, entries.len(), &format)?);
    }
    if entries.is_empty() {
        return Err(invalid("Empty library"));
    }
    if entries
        .iter()
        .map(|e| e.spectrum.peaks.len())
        .sum::<usize>()
        > 1_000_000
    {
        return Err(EngineError::new(
            "resource_limit",
            "Library exceeds one million peaks",
        ));
    }
    let mut ids = BTreeSet::new();
    if entries.iter().any(|e| !ids.insert(&e.accession)) {
        return Err(invalid("Duplicate library accession"));
    }
    let mut precursor_index: Vec<_> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| e.spectrum.precursor_mz.map(|mz| (mz, i)))
        .collect();
    precursor_index.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut warnings = Vec::new();
    for e in &entries {
        if e.spectrum.precursor_mz.is_none() {
            warnings.push(format!("{}: no precursor m/z; not indexed", e.accession));
        }
        if e.spectrum.collision_energy.is_none() {
            warnings.push(format!("{}: collision energy missing or not scalar with explicit unit; original metadata retained",e.accession));
        }
        if first(&e.metadata, &["LICENSE"]).is_none() {
            warnings.push(format!(
                "{}: uses declared source license {}",
                e.accession, source.license
            ));
        }
    }
    Ok(Library {
        source,
        sha256: format!("{:x}", Sha256::digest(raw_text.as_bytes())),
        raw_text,
        format,
        entries,
        precursor_index,
        warnings,
    })
}
pub fn verify_library(l: &Library) -> Result<()> {
    if import_library(l.raw_text.clone(), l.format.clone(), l.source.clone())? != *l {
        return Err(EngineError::new(
            "corrupt_artifact",
            "Library import/index replay differs",
        ));
    }
    Ok(())
}
