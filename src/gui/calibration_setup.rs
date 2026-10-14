//! What the user decides on the calibration tab: which injection is a standard of which level,
//! what each level contains, and how every analyte is calibrated. From that the targeted batch
//! request is built, and its evidence is taken from the peaks already measured in tab 1.
use crate::quant::{Analyte, Measurement, Method};
use crate::sample_sheet::{stem, Level, Row};
use crate::targeted as t;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// A standard or QC point the user took out of the calibration, with the reason.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Exclusion {
    pub sample: String,
    pub analyte: String,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct AnalyteSetup {
    pub name: String,
    /// This analyte is an internal standard: it is measured, not quantified.
    pub is_standard: bool,
    /// Name of the internal-standard analyte used for this one; none = external calibration.
    pub internal_standard: Option<String>,
    pub cfg: t::CalibrationConfig,
    /// Acceptable peak area of this analyte when it is used as an internal standard.
    pub area_bounds: [f64; 2],
}
impl AnalyteSetup {
    fn new(name: &str) -> Self {
        AnalyteSetup {
            name: name.into(),
            is_standard: false,
            internal_standard: None,
            cfg: t::CalibrationConfig {
                curve: t::Curve::Polynomial,
                degree: 1,
                intercept: t::Intercept::Free,
                weighting: t::Weighting::Unweighted,
                unit: t::Unit::NgMl,
                // Both limits are fitted to the standards when the batch is built.
                range: [0.0, 1.0],
                lod: 0.0,
                loq: 0.0,
                accuracy_tolerance_percent: 15.0,
                qc_cv_limit_percent: 15.0,
                blank_response_limit: 0.0,
            },
            area_bounds: [1.0, 1e30],
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(super) struct Setup {
    pub rows: Vec<Row>,
    pub levels: Vec<Level>,
    pub analytes: Vec<AnalyteSetup>,
    pub exclusions: Vec<Exclusion>,
}

/// "2" before "10", and "run-9" before "run-10".
fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let number = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut n = String::new();
                    while let Some(c) = it.next_if(char::is_ascii_digit) {
                        n.push(c);
                    }
                    n.trim_start_matches('0').to_owned()
                };
                let (na, nb) = (number(&mut a), number(&mut b));
                let order = na.len().cmp(&nb.len()).then(na.cmp(&nb));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

impl Setup {
    /// Keeps the setup in step with the loaded samples and the method's analytes: new ones are
    /// added with defaults, vanished ones dropped, everything else is left as the user set it.
    pub fn sync(&mut self, samples: &[(String, String)], analytes: &[Analyte]) {
        self.rows
            .retain(|r| samples.iter().any(|s| s.0 == r.source));
        let mut new: Vec<&str> = samples
            .iter()
            .map(|s| s.0.as_str())
            .filter(|p| !self.rows.iter().any(|r| r.source == *p))
            .collect();
        new.sort_by(|a, b| natural(&stem(a), &stem(b)));
        for path in new {
            let mut row = Row::new(path);
            // Sample IDs must be unique.
            while self.rows.iter().any(|r| r.id == row.id) {
                row.id.push('\'');
            }
            self.rows.push(row);
        }
        let names: Vec<&str> = analytes
            .iter()
            .map(|a| a.extraction.name.as_str())
            .collect();
        self.analytes.retain(|a| names.contains(&a.name.as_str()));
        for name in &names {
            if !self.analytes.iter().any(|a| a.name == *name) {
                self.analytes.push(AnalyteSetup::new(name));
            }
        }
        self.analytes
            .sort_by_key(|a| names.iter().position(|n| *n == a.name));
        let standards: Vec<String> = self
            .analytes
            .iter()
            .filter(|a| a.is_standard)
            .map(|a| a.name.clone())
            .collect();
        for a in &mut self.analytes {
            if a.is_standard
                || a.internal_standard
                    .as_ref()
                    .is_some_and(|i| !standards.contains(i))
            {
                a.internal_standard = None;
            }
        }
        self.exclusions.retain(|e| {
            self.rows.iter().any(|r| r.id == e.sample) && names.contains(&e.analyte.as_str())
        });
    }
    pub fn analyte_names(&self) -> Vec<String> {
        self.analytes.iter().map(|a| a.name.clone()).collect()
    }
    /// Remembers the fit settings and exclusions of a batch the user adjusted in the curve review.
    pub fn absorb(&mut self, batch: &t::BatchResult) {
        let name_of = |id: &str| {
            batch
                .request
                .targets
                .iter()
                .find(|t| t.id == id)
                .map(|t| t.quantifier.extraction.extraction.name.clone())
        };
        for target in &batch.request.targets {
            if let (Some(cfg), Some(a)) = (
                &target.calibration,
                self.analytes
                    .iter_mut()
                    .find(|a| a.name == target.quantifier.extraction.extraction.name),
            ) {
                a.cfg = cfg.clone();
            }
        }
        self.exclusions = batch
            .request
            .samples
            .iter()
            .flat_map(|s| {
                s.exclusions.iter().filter_map(|(target, reason)| {
                    Some(Exclusion {
                        sample: s.id.clone(),
                        analyte: name_of(target)?,
                        reason: reason.clone(),
                    })
                })
            })
            .collect();
    }
    /// The targeted batch request described by this setup, or everything still missing.
    pub fn build_request(&self, method: &Method) -> Result<t::BatchRequest, Vec<String>> {
        let mut problems = Vec::new();
        if self.analytes.is_empty() {
            problems.push("The method has no analytes (set it up in tab 1).".to_owned());
        }
        if self.rows.is_empty() {
            problems.push("No samples: select samples in tab 1 and run the batch.".to_owned());
        }
        let target_id = |name: &str| {
            self.analytes
                .iter()
                .position(|a| a.name == name)
                .map(|i| format!("target-{}", i + 1))
        };
        let level = |name: &str| self.levels.iter().find(|l| l.name == name);
        let excluded = |sample: &str, analyte: &str| {
            self.exclusions
                .iter()
                .find(|e| e.sample == sample && e.analyte == analyte)
        };
        let calibrated: Vec<&AnalyteSetup> =
            self.analytes.iter().filter(|a| !a.is_standard).collect();
        let mut seen = std::collections::HashSet::new();
        for r in &self.rows {
            if r.id.trim().is_empty() || !seen.insert(r.id.as_str()) {
                problems.push(format!("Sample ID '{}' is empty or used twice.", r.id));
            }
            if !r.dilution.is_finite() || r.dilution <= 0.0 {
                problems.push(format!("Sample '{}' needs a dilution above 0.", r.id));
            }
            if matches!(r.role, t::Role::Standard | t::Role::Qc) {
                match level(&r.level) {
                    None => problems.push(format!(
                        "{} '{}' has no level ({}).",
                        crate::sample_sheet::role_name(&r.role),
                        r.id,
                        if r.level.is_empty() {
                            "choose one"
                        } else {
                            "unknown level"
                        }
                    )),
                    Some(l) => {
                        for a in &calibrated {
                            if !l.conc.contains_key(&a.name) {
                                problems.push(format!(
                                    "Level '{}' has no concentration for {}.",
                                    l.name, a.name
                                ));
                            }
                        }
                    }
                }
            }
        }
        problems.sort();
        problems.dedup();
        let mut ranges = Vec::new();
        for a in &calibrated {
            let points: Vec<f64> = self
                .rows
                .iter()
                .filter(|r| r.role == t::Role::Standard && excluded(&r.id, &a.name).is_none())
                .filter_map(|r| level(&r.level)?.conc.get(&a.name).copied())
                .collect();
            let low = points.iter().copied().fold(f64::INFINITY, f64::min);
            let high = points.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if points.is_empty() || low >= high {
                problems.push(format!(
                    "{} needs standards at two or more different levels.",
                    a.name
                ));
            }
            ranges.push([low, high]);
        }
        if !problems.is_empty() {
            return Err(problems);
        }
        let mut ranges = ranges.into_iter();
        let targets = method
            .analytes
            .iter()
            .filter_map(|analyte| {
                let a = self
                    .analytes
                    .iter()
                    .find(|a| a.name == analyte.extraction.name)?;
                let calibration = (!a.is_standard).then(|| {
                    let range = ranges.next().unwrap();
                    let mut cfg = a.cfg.clone();
                    cfg.range = range;
                    cfg.loq = if cfg.loq > 0.0 {
                        cfg.loq.min(range[1])
                    } else {
                        range[0]
                    };
                    cfg.lod = cfg.lod.min(cfg.loq);
                    cfg
                });
                Some(t::Target {
                    id: target_id(&a.name)?,
                    quantifier: t::Ion {
                        extraction: analyte.clone(),
                    },
                    qualifiers: vec![],
                    internal_standard: a.internal_standard.as_deref().and_then(target_id),
                    is_area_range: a.is_standard.then_some(a.area_bounds),
                    calibration,
                })
            })
            .collect::<Vec<_>>();
        let samples = self
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let needs_levels = matches!(r.role, t::Role::Standard | t::Role::Qc);
                let nominal = if needs_levels {
                    self.analytes
                        .iter()
                        .filter(|a| !a.is_standard)
                        .filter_map(|a| {
                            Some((target_id(&a.name)?, *level(&r.level)?.conc.get(&a.name)?))
                        })
                        .collect()
                } else {
                    Default::default()
                };
                t::Sample {
                    id: r.id.clone(),
                    source: r.source.clone(),
                    role: r.role.clone(),
                    injection_order: i as u32 + 1,
                    dilution: r.dilution,
                    nominal,
                    exclusions: self
                        .exclusions
                        .iter()
                        .filter(|e| e.sample == r.id)
                        .filter_map(|e| Some((target_id(&e.analyte)?, e.reason.clone())))
                        .collect(),
                }
            })
            .collect();
        Ok(t::BatchRequest {
            version: 1,
            name: if method.name.trim().is_empty() {
                "Batch".into()
            } else {
                method.name.clone()
            },
            targets,
            samples,
            detection_smoothing: method.detection_smoothing,
            minimum_height: method.minimum_height,
            boundary_fraction: method.boundary_fraction,
        })
    }
}

/// Every (sample, analyte) of `request` that has no peak measured in tab 1.
pub(super) fn missing_peaks(results: &[Measurement], request: &t::BatchRequest) -> Vec<String> {
    let mut missing = Vec::new();
    for s in &request.samples {
        for target in &request.targets {
            let name = &target.quantifier.extraction.extraction.name;
            if !results
                .iter()
                .any(|m| m.source == s.source && &m.analyte == name)
            {
                missing.push(format!(
                    "No peak measured for {name} in {} — run the batch in tab 1.",
                    stem(&s.source)
                ));
            }
        }
    }
    missing
}
/// The peaks measured in tab 1 as evidence for `request`; lists every (sample, analyte) that has
/// not been measured yet.
pub(super) fn observations(
    results: &[Measurement],
    request: &t::BatchRequest,
) -> Result<Vec<t::Observation>, Vec<String>> {
    let missing = missing_peaks(results, request);
    if !missing.is_empty() {
        return Err(missing);
    }
    let mut out = Vec::new();
    for s in &request.samples {
        for target in &request.targets {
            let name = &target.quantifier.extraction.extraction.name;
            let m = results
                .iter()
                .find(|m| m.source == s.source && &m.analyte == name)
                .expect("checked by missing_peaks");
            out.push(t::Observation {
                sample: s.id.clone(),
                target: target.id.clone(),
                quantifier: m.clone(),
                qualifiers: vec![],
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample_sheet::apply;

    #[test]
    fn natural_order_puts_numbers_in_numeric_order() {
        let mut names = vec!["261009-10", "261009-7", "261009-9", "261009-8"];
        names.sort_by(|a, b| natural(a, b));
        assert_eq!(names, ["261009-7", "261009-8", "261009-9", "261009-10"]);
    }
    fn analyte(name: &str, mass: f64) -> Analyte {
        let mut a = crate::quant::new_analyte(1);
        a.extraction.name = name.into();
        a.extraction.mass = Some(mass);
        a.expected_rt = 1.0;
        a.rt_window = [0.0, 2.0];
        a
    }
    fn setup() -> (Setup, Method) {
        let method = Method {
            analytes: vec![analyte("Analyte", 100.0), analyte("IS", 105.0)],
            ..Default::default()
        };
        let samples: Vec<(String, String)> = (7..=12)
            .rev()
            .map(|n| (format!("C:/d/26-{n}.mzML"), format!("26-{n}")))
            .collect();
        let mut s = Setup::default();
        s.sync(&samples, &method.analytes);
        (s, method)
    }
    #[test]
    fn sync_orders_new_samples_naturally_and_keeps_user_choices() {
        let (mut s, method) = setup();
        assert_eq!(s.rows[0].id, "26-7");
        assert_eq!(s.rows[5].id, "26-12");
        s.rows[0].role = t::Role::Standard;
        s.analytes[1].is_standard = true;
        s.analytes[0].internal_standard = Some("IS".into());
        let more: Vec<(String, String)> = s
            .rows
            .iter()
            .map(|r| (r.source.clone(), r.id.clone()))
            .chain([("C:/d/26-13.mzML".to_owned(), "26-13".to_owned())])
            .collect();
        s.sync(&more, &method.analytes);
        assert_eq!(s.rows.len(), 7);
        assert_eq!(s.rows[0].role, t::Role::Standard);
        assert_eq!(s.rows[6].id, "26-13");
        assert_eq!(s.analytes[0].internal_standard.as_deref(), Some("IS"));
        // Losing the internal standard from the method clears the choice.
        s.sync(&more, &method.analytes[..1]);
        assert_eq!(s.analytes.len(), 1);
        assert_eq!(s.analytes[0].internal_standard, None);
    }
    #[test]
    fn building_lists_what_is_missing_then_wires_levels_and_internal_standards() {
        let (mut s, method) = setup();
        let problems = s.build_request(&method).err().unwrap();
        assert!(
            problems.iter().any(|p| p.contains("two or more")),
            "{problems:?}"
        );
        let sheet = "sample type,file name,sample id,dilution,level\n\
                     standard,26-7,s1,1,1\nstandard,26-8,s2,1,2\nstandard,26-9,s3,2,3\n\
                     unknown,26-10,u1,1,\n";
        apply(&mut s.rows, sheet).unwrap();
        s.analytes[1].is_standard = true;
        s.analytes[0].internal_standard = Some("IS".into());
        let problems = s.build_request(&method).err().unwrap();
        assert!(
            problems.iter().any(|p| p.contains("no level")),
            "{problems:?}"
        );
        s.levels = ["1", "2", "3"]
            .iter()
            .zip([1.0, 5.0, 10.0])
            .map(|(n, c)| Level {
                name: (*n).into(),
                conc: [("Analyte".to_owned(), c)].into(),
            })
            .collect();
        let request = s.build_request(&method).unwrap();
        assert_eq!(request.samples[0].id, "s1");
        assert_eq!(request.samples[2].dilution, 2.0);
        assert_eq!(request.samples[2].nominal["target-1"], 10.0);
        assert!(request.samples[3].nominal.is_empty());
        let analyte = &request.targets[0];
        assert_eq!(analyte.internal_standard.as_deref(), Some("target-2"));
        let cfg = analyte.calibration.as_ref().unwrap();
        assert_eq!((cfg.range, cfg.loq), ([1.0, 10.0], 1.0));
        let is = &request.targets[1];
        assert!(is.calibration.is_none() && is.is_area_range.is_some());
        crate::targeted::validate(&request).unwrap();
        // An excluded standard no longer counts for the range, and survives the rebuild.
        s.exclusions.push(Exclusion {
            sample: "s1".into(),
            analyte: "Analyte".into(),
            reason: "bad".into(),
        });
        let request = s.build_request(&method).unwrap();
        assert_eq!(
            request.targets[0].calibration.as_ref().unwrap().range,
            [5.0, 10.0]
        );
        assert_eq!(request.samples[0].exclusions["target-1"], "bad");
    }
}
