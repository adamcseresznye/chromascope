//! Streaming mzML metadata validation before mzdata discards source units.
use crate::error::{ChromascopeError, Result};
use quick_xml::{events::Event, Reader};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

fn invalid(message: impl std::fmt::Display) -> ChromascopeError {
    ChromascopeError::MzDataError(format!("Invalid mzML source metadata: {message}"))
}

pub(crate) fn validate_mzml(path: &Path, control: Option<&crate::jobs::JobControl>) -> Result<()> {
    let name = path.to_string_lossy().to_ascii_lowercase();
    if !name.ends_with(".mzml") && !name.ends_with(".mzml.gz") {
        return Ok(());
    }
    let file = File::open(path).map_err(|e| invalid(e.to_string()))?;
    let input: Box<dyn Read> = if name.ends_with(".gz") {
        Box::new(flate2::read::MultiGzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut reader = Reader::from_reader(BufReader::new(input));
    reader.trim_text(true);
    let mut buffer = Vec::new();
    let mut scan_time: Option<bool> = None;
    let mut declared_count = None;
    let mut spectra = 0usize;
    let mut declared_chromatograms: Option<usize> = None;
    let mut chromatograms = 0usize;
    let mut root_seen = false;
    let mut open_tags = Vec::new();
    let mut spectrum_has_scan = None;
    let mut spectrum_length = None;
    let mut has_mz_array = false;
    let mut has_intensity_array = false;
    let mut in_binary_array = false;
    let mut chromatogram_length: Option<usize> = None;
    let mut chromatogram_has_time = false;
    let mut chromatogram_has_intensity = false;
    let mut in_chromatogram = false;
    loop {
        if let Some(control) = control {
            control.check().map_err(invalid)?;
        }
        let event = reader.read_event_into(&mut buffer).map_err(invalid)?;
        match &event {
            Event::Start(tag) => open_tags.push(tag.name().as_ref().to_vec()),
            Event::End(tag) if open_tags.pop().as_deref() != Some(tag.name().as_ref()) => {
                return Err(invalid("mismatched XML closing tag"));
            }
            _ => {}
        }
        // `cvParam` elements are always self-closing (`<cvParam .../>`), so
        // array/scan metadata must be handled for both Start and Empty.
        // Only Start opens multi-line state (spectrum/scan/chromatogram/
        // binary array); Empty chromatograms count immediately when length 0.
        let is_empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(tag) | Event::Empty(tag) => {
                let name = tag.local_name();
                if name.as_ref() == b"mzML" {
                    root_seen = true;
                }
                if name.as_ref() == b"spectrumList" {
                    for attr in tag.attributes() {
                        let attr = attr.map_err(invalid)?;
                        if attr.key.as_ref() == b"count" {
                            declared_count = Some(
                                attr.unescape_value()
                                    .map_err(invalid)?
                                    .parse::<usize>()
                                    .map_err(invalid)?,
                            );
                        }
                    }
                }
                if name.as_ref() == b"chromatogramList" {
                    for attr in tag.attributes() {
                        let attr = attr.map_err(invalid)?;
                        if attr.key.as_ref() == b"count" {
                            declared_chromatograms = Some(
                                attr.unescape_value()
                                    .map_err(invalid)?
                                    .parse::<usize>()
                                    .map_err(invalid)?,
                            );
                        }
                    }
                }
                if name.as_ref() == b"chromatogram" {
                    chromatograms += 1;
                    let mut length: Option<usize> = None;
                    for attr in tag.attributes() {
                        let attr = attr.map_err(invalid)?;
                        if attr.key.as_ref() == b"defaultArrayLength" {
                            length = Some(
                                attr.unescape_value()
                                    .map_err(invalid)?
                                    .parse::<usize>()
                                    .map_err(invalid)?,
                            );
                        }
                    }
                    if is_empty {
                        // Self-closing chromatograms carry no child arrays.
                        if length != Some(0) {
                            return Err(invalid(
                                "nonempty chromatogram requires declared length and time/intensity arrays",
                            ));
                        }
                    } else {
                        in_chromatogram = true;
                        chromatogram_length = length;
                        chromatogram_has_time = false;
                        chromatogram_has_intensity = false;
                    }
                }
                if name.as_ref() == b"spectrum" && !is_empty {
                    spectra += 1;
                    spectrum_has_scan = Some(false);
                    spectrum_length = None;
                    has_mz_array = false;
                    has_intensity_array = false;
                    for attr in tag.attributes() {
                        let attr = attr.map_err(invalid)?;
                        if attr.key.as_ref() == b"defaultArrayLength" {
                            spectrum_length = Some(
                                attr.unescape_value()
                                    .map_err(invalid)?
                                    .parse::<usize>()
                                    .map_err(invalid)?,
                            );
                        }
                    }
                }
                if name.as_ref() == b"binaryDataArray" && !is_empty {
                    in_binary_array = true;
                }
                if name.as_ref() == b"scan" && !is_empty {
                    if scan_time.is_some() {
                        return Err(invalid("nested scan metadata"));
                    }
                    scan_time = Some(false);
                    spectrum_has_scan = Some(true);
                }
                if name.as_ref() == b"cvParam" && in_chromatogram && in_binary_array {
                    for attr in tag.attributes() {
                        let attr = attr.map_err(invalid)?;
                        if attr.key.as_ref() == b"accession" {
                            let text =
                                attr.unescape_value().map_err(invalid)?.into_owned();
                            chromatogram_has_time |= text == "MS:1000595";
                            chromatogram_has_intensity |= text == "MS:1000515";
                        }
                    }
                }
                if name.as_ref() == b"cvParam" && spectrum_has_scan.is_some() {
                    let mut accession = None;
                    let mut value = None;
                    let mut unit = None;
                    let mut param_name = None;
                    for attr in tag.attributes() {
                        let attr = attr.map_err(invalid)?;
                        let text = attr.unescape_value().map_err(invalid)?.into_owned();
                        match attr.key.as_ref() {
                            b"accession" => accession = Some(text),
                            b"value" => value = Some(text),
                            b"unitAccession" => unit = Some(text),
                            b"name" => param_name = Some(text),
                            _ => {}
                        }
                    }
                    if in_binary_array {
                        has_mz_array |= accession.as_deref() == Some("MS:1000514");
                        has_intensity_array |= accession.as_deref() == Some("MS:1000515");
                    }
                    if accession.as_deref() == Some("MS:1000016")
                        || param_name.as_deref() == Some("scan start time")
                    {
                        if scan_time.is_none() {
                            return Err(invalid("scan start time occurs outside scan metadata"));
                        }
                        if scan_time == Some(true) {
                            return Err(invalid("duplicate scan start time"));
                        }
                        if accession.as_deref() != Some("MS:1000016")
                            || param_name.as_deref() != Some("scan start time")
                        {
                            return Err(invalid("inconsistent scan start time CV accession/name"));
                        }
                        let scale=match unit.as_deref() {
                            Some("UO:0000031")=>1.,Some("UO:0000010")=>60.,Some("UO:0000028")=>60000.,
                            _=>return Err(invalid("scan start time requires an explicit supported minute/second/millisecond unit")),
                        };
                        let time = value
                            .ok_or_else(|| invalid("missing scan start time value"))?
                            .parse::<f64>()
                            .map_err(invalid)?;
                        if !time.is_finite() || time < 0. || time / scale > f32::MAX as f64 {
                            return Err(invalid(
                                "nonfinite, negative or unrepresentable scan start time",
                            ));
                        }
                        scan_time = Some(true);
                    }
                }
            }
            Event::End(tag) if tag.local_name().as_ref() == b"scan" => {
                if scan_time.take() != Some(true) {
                    return Err(invalid("scan start time metadata is missing"));
                }
            }
            Event::End(tag) if tag.local_name().as_ref() == b"spectrum" => {
                if spectrum_has_scan.take() != Some(true) {
                    return Err(invalid("spectrum acquisition scan metadata is missing"));
                }
                match spectrum_length.take() {
                    Some(0) => {}
                    Some(_) if has_mz_array && has_intensity_array => {}
                    _ => {
                        return Err(invalid(
                            "nonempty spectrum requires declared length and m/z/intensity arrays",
                        ))
                    }
                }
            }
            Event::End(tag) if tag.local_name().as_ref() == b"binaryDataArray" => {
                in_binary_array = false
            }
            Event::End(tag) if tag.local_name().as_ref() == b"chromatogram" => {
                if !in_chromatogram {
                    return Err(invalid("mismatched chromatogram closing tag"));
                }
                in_chromatogram = false;
                match chromatogram_length.take() {
                    Some(0) => {}
                    Some(_) if chromatogram_has_time && chromatogram_has_intensity => {}
                    _ => {
                        return Err(invalid(
                            "nonempty chromatogram requires declared length and time/intensity arrays",
                        ))
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    if !root_seen || !open_tags.is_empty() || spectrum_has_scan.is_some() || scan_time.is_some()
    {
        return Err(invalid(
            "incomplete mzML root, spectrum count or scan metadata",
        ));
    }
    if in_chromatogram {
        return Err(invalid("incomplete mzML chromatogram metadata"));
    }
    // Spectrum-bearing files keep the original strict contract. A mismatched
    // chromatogram count does not fail these files: stored traces are exposed
    // best-effort and unreadable ones are skipped during bounds extraction.
    if spectra > 0 {
        if declared_count != Some(spectra) {
            return Err(invalid(
                "incomplete mzML root, spectrum count or scan metadata",
            ));
        }
        return Ok(());
    }
    // Chromatogram-only files (e.g. SRM/MRM with no <spectrumList>) are valid
    // when the chromatogram list count matches the observed chromatograms.
    if declared_chromatograms == Some(chromatograms) && chromatograms > 0 {
        return Ok(());
    }
    Err(invalid(
        "incomplete mzML root, spectrum count or scan metadata",
    ))
}
