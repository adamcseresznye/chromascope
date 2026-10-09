use super::*;
/// Monoisotopic neutral atom masses (Da); ion offsets include electron/proton masses.
const ATOMS: [(&str, f64); 6] = [
    ("C", 12.0),
    ("H", 1.00782503223),
    ("N", 14.00307400443),
    ("O", 15.99491461957),
    ("S", 31.9720711744),
    ("P", 30.97376199842),
];
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FormulaConfig {
    pub observed_mz: f64,
    pub polarity: Polarity,
    pub adducts: Vec<String>,
    pub tolerance: Tolerance,
    pub maximum_atoms: BTreeMap<String, u16>,
    pub maximum_candidates: usize,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FormulaCandidate {
    pub formula: String,
    pub adduct: String,
    pub neutral_mass_da: f64,
    pub predicted_mz: f64,
    pub error_ppm: f64,
    pub dbe: f64,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FormulaReport {
    pub adduct_hypotheses: Vec<AdductHypothesis>,
    pub config: FormulaConfig,
    pub candidates: Vec<FormulaCandidate>,
    pub warnings: Vec<String>,
}
fn adduct(name: &str) -> Result<(f64, i32, u32)> {
    match name {
        "[M+H]+" => Ok((1.007276466621, 1, 1)),
        "[M-H]-" => Ok((-1.007276466621, -1, 1)),
        "[M+Na]+" => Ok((22.989220702091, 1, 1)),
        "[M+NH4]+" => Ok((18.033825553, 1, 1)),
        "[M+K]+" => Ok((38.963157906, 1, 1)),
        "[M+2H]2+" => Ok((2.014552933242, 2, 1)),
        "[2M+H]+" => Ok((1.007276466621, 1, 2)),
        _ => Err(EngineError::new(
            "unsupported_capability",
            format!("Unsupported adduct {name}"),
        )),
    }
}
pub fn formula_candidates(config: FormulaConfig) -> Result<FormulaReport> {
    config.tolerance.validate()?;
    if !config.observed_mz.is_finite()
        || config.observed_mz <= 0.0
        || config.observed_mz > 2000.0
        || config.adducts.is_empty()
        || config.adducts.len() > 16
        || config.maximum_candidates == 0
        || config.maximum_candidates > 1000
        || config
            .maximum_atoms
            .keys()
            .any(|k| !ATOMS.iter().any(|(a, _)| a == k))
    {
        return Err(invalid("Formula search requires m/z<=2000, supported CHNOPS bounds, 1..16 adducts and <=1000 candidates"));
    }
    let bounds: Vec<u16> = ATOMS
        .iter()
        .map(|(a, _)| config.maximum_atoms.get(*a).copied().unwrap_or(0))
        .collect();
    // Enumerate heavy atoms, solve H analytically in mass interval. Cap actual search space.
    let space = [0, 2, 3, 4, 5]
        .iter()
        .try_fold(1usize, |acc, i| acc.checked_mul(bounds[*i] as usize + 1))
        .unwrap_or(usize::MAX);
    if space > 2_000_000 || bounds.iter().any(|n| *n > 500) {
        return Err(EngineError::new(
            "resource_limit",
            "Formula heavy-atom search exceeds two million combinations or atom bound exceeds 500",
        ));
    }
    let mut candidates = Vec::new();
    for name in &config.adducts {
        let (offset, charge, multimer) = adduct(name)?;
        if config.polarity == Polarity::Unknown
            || (charge > 0) != (config.polarity == Polarity::Positive)
        {
            return Err(invalid("Adduct charge conflicts with explicit polarity"));
        }
        let neutral = (config.observed_mz * charge.abs() as f64 - offset) / multimer as f64;
        if neutral <= 0.0 {
            return Err(invalid(
                "Adduct hypothesis implies nonpositive neutral mass",
            ));
        }
        let delta = config.tolerance.da(config.observed_mz) * charge.abs() as f64 / multimer as f64;
        for c in 0..=bounds[0] {
            for n in 0..=bounds[2] {
                for o in 0..=bounds[3] {
                    for s in 0..=bounds[4] {
                        for p in 0..=bounds[5] {
                            let mass = c as f64 * ATOMS[0].1
                                + n as f64 * ATOMS[2].1
                                + o as f64 * ATOMS[3].1
                                + s as f64 * ATOMS[4].1
                                + p as f64 * ATOMS[5].1;
                            let h_low =
                                ((neutral - delta - mass) / ATOMS[1].1).ceil().max(0.0) as u16;
                            let h_high = ((neutral + delta - mass) / ATOMS[1].1)
                                .floor()
                                .min(bounds[1] as f64);
                            if h_high < 0.0 {
                                continue;
                            }
                            for h in h_low..=h_high as u16 {
                                let dbe =
                                    1.0 + c as f64 - (h as f64) / 2.0 + (n as f64 + p as f64) / 2.0;
                                if dbe < 0.0 || dbe.fract() != 0.0 {
                                    continue;
                                }
                                let atoms = [c, h, n, o, s, p];
                                if atoms.iter().all(|x| *x == 0) {
                                    continue;
                                }
                                let formula = ATOMS
                                    .iter()
                                    .zip(atoms)
                                    .filter(|(_, count)| *count > 0)
                                    .map(|((a, _), count)| {
                                        if count == 1 {
                                            a.to_string()
                                        } else {
                                            format!("{a}{count}")
                                        }
                                    })
                                    .collect::<String>();
                                let neutral_mass_da = mass + h as f64 * ATOMS[1].1;
                                let predicted_mz = (neutral_mass_da * multimer as f64 + offset)
                                    / charge.abs() as f64;
                                if (predicted_mz - config.observed_mz).abs()
                                    > config.tolerance.da(config.observed_mz)
                                {
                                    continue;
                                }
                                candidates.push(FormulaCandidate {
                                    formula,
                                    adduct: name.clone(),
                                    neutral_mass_da,
                                    predicted_mz,
                                    error_ppm: (config.observed_mz - predicted_mz) / predicted_mz
                                        * 1e6,
                                    dbe,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    candidates.sort_by(|a, b| {
        a.error_ppm
            .abs()
            .total_cmp(&b.error_ppm.abs())
            .then(a.formula.cmp(&b.formula))
            .then(a.adduct.cmp(&b.adduct))
    });
    let truncated = candidates.len() > config.maximum_candidates;
    candidates.truncate(config.maximum_candidates);
    let mut warnings=vec!["CHNOPS closed-shell candidates with integer nonnegative DBE only; exact mass does not identify structure or resolve adduct ambiguity".into(),"No isotope likelihood or chemical plausibility beyond explicit atom bounds/DBE is inferred".into()];
    if truncated {
        warnings.push("Candidate output truncated by maximum_candidates".into());
    }
    let adduct_hypotheses = config
        .adducts
        .iter()
        .map(|name| {
            let (offset, charge, multimer) = adduct(name)?;
            Ok(AdductHypothesis {
                adduct: name.clone(),
                charge,
                multimer,
                neutral_mass_da: (config.observed_mz * charge.abs() as f64 - offset)
                    / multimer as f64,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(FormulaReport {
        adduct_hypotheses,
        config,
        candidates,
        warnings,
    })
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IsotopeConfig {
    /// Optional CHNOPS composition for nominal (not fine structure) abundance convolution.
    #[serde(default)]
    pub expected_atom_counts: Option<BTreeMap<String, u16>>,
    pub monoisotopic_mz: f64,
    pub maximum_charge: u8,
    pub maximum_isotopes: u8,
    pub tolerance: Tolerance,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IsotopeHypothesis {
    pub charge: u8,
    pub spacing_mz: f64,
    pub peaks: Vec<[f64; 2]>,
    pub ratios_to_mono: Vec<f64>,
    pub carbon_estimate: Option<f64>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IsotopeReport {
    pub nominal_pattern: Option<NominalPattern>,
    pub nominal_comparisons: Vec<NominalComparison>,
    pub spectrum: Spectrum,
    pub config: IsotopeConfig,
    pub hypotheses: Vec<IsotopeHypothesis>,
    pub warnings: Vec<String>,
}
pub fn isotopes(spectrum: Spectrum, config: IsotopeConfig) -> Result<IsotopeReport> {
    spectrum.validate()?;
    config.tolerance.validate()?;
    if spectrum.ms_level != 1
        || spectrum.representation != Representation::Centroid
        || !config.monoisotopic_mz.is_finite()
        || config.monoisotopic_mz <= 0.0
        || config.maximum_charge == 0
        || config.maximum_charge > 6
        || config.maximum_isotopes == 0
        || config.maximum_isotopes > 6
    {
        return Err(invalid("Isotope analysis requires centroid MS1, positive mono m/z, charges/isotope counts 1..6"));
    }
    let nearest = |mass: f64| {
        spectrum
            .peaks
            .iter()
            .filter(|p| (p[0] - mass).abs() <= config.tolerance.da(mass) && p[1] > 0.0)
            .min_by(|a, b| (a[0] - mass).abs().total_cmp(&(b[0] - mass).abs()))
            .copied()
    };
    let mono = nearest(config.monoisotopic_mz)
        .ok_or_else(|| invalid("Monoisotopic peak missing or nonpositive"))?;
    let mut hypotheses = Vec::new();
    for charge in 1..=config.maximum_charge {
        let spacing_mz = 1.00335483507 / charge as f64;
        let mut peaks = vec![mono];
        for k in 1..=config.maximum_isotopes {
            if let Some(p) = nearest(mono[0] + k as f64 * spacing_mz) {
                if peaks.iter().any(|previous| previous[0] == p[0]) {
                    break;
                }
                peaks.push(p);
            } else {
                break;
            }
        }
        if peaks.len() > 1 {
            let ratios_to_mono: Vec<_> = peaks.iter().map(|p| p[1] / mono[1]).collect();
            let carbon_estimate = Some(ratios_to_mono[1] / (0.0107 / 0.9893));
            if ratios_to_mono.iter().any(|v| !v.is_finite())
                || carbon_estimate.is_some_and(|v| !v.is_finite())
            {
                return Err(invalid("Isotope ratio exceeds finite range"));
            }
            hypotheses.push(IsotopeHypothesis {
                charge,
                spacing_mz,
                peaks,
                ratios_to_mono,
                carbon_estimate,
            });
        }
    }
    let nominal_pattern = config
        .expected_atom_counts
        .as_ref()
        .map(|counts| nominal_pattern(counts.clone(), config.maximum_isotopes))
        .transpose()?;
    let mut nominal_comparisons = Vec::new();
    if let Some(pattern) = &nominal_pattern {
        for charge in 1..=config.maximum_charge {
            let mut observed_intensities = vec![0.0; pattern.probabilities.len()];
            for peak in &spectrum.peaks {
                let shift = ((peak[0] - mono[0]) * charge as f64).round();
                if shift >= 0.0 && shift < observed_intensities.len() as f64 {
                    observed_intensities[shift as usize] += peak[1].max(0.0);
                }
            }
            if observed_intensities.iter().any(|v| !v.is_finite()) {
                return Err(invalid("Nominal isotope intensity overflow"));
            }
            let maximum = observed_intensities.iter().copied().fold(0.0, f64::max);
            let scaled: Vec<_> = observed_intensities.iter().map(|v| v / maximum).collect();
            let norm = (scaled.iter().map(|v| v * v).sum::<f64>()
                * pattern.probabilities.iter().map(|v| v * v).sum::<f64>())
            .sqrt();
            let cosine = scaled
                .iter()
                .zip(&pattern.probabilities)
                .map(|(a, b)| a * b)
                .sum::<f64>()
                / norm;
            nominal_comparisons.push(NominalComparison {
                charge,
                observed_intensities,
                cosine: cosine.clamp(0.0, 1.0),
            });
        }
    }
    Ok(IsotopeReport{nominal_pattern,nominal_comparisons,spectrum,config,hypotheses,warnings:vec!["Spacing hypotheses use 13C-12C; coelution, overlap and non-carbon isotopes can confound charge and carbon estimate".into(),"Carbon estimate assumes M+1 entirely from natural 13C; no identity confidence or full isotope formula fit".into()]})
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdductHypothesis {
    pub adduct: String,
    pub charge: i32,
    pub multimer: u32,
    pub neutral_mass_da: f64,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NominalPattern {
    pub atom_counts: BTreeMap<String, u16>,
    pub probabilities: Vec<f64>,
    pub retained_probability: f64,
    pub abundance_source: String,
    pub warnings: Vec<String>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NominalComparison {
    pub charge: u8,
    pub observed_intensities: Vec<f64>,
    pub cosine: f64,
}
/// Natural-abundance generating-function convolution into nominal M+k bins.
/// No fine isotopologue structure, enrichment or isotope-dependent response is inferred.
pub fn nominal_pattern(
    counts: BTreeMap<String, u16>,
    maximum_isotopes: u8,
) -> Result<NominalPattern> {
    if maximum_isotopes == 0
        || maximum_isotopes > 6
        || counts.values().map(|n| *n as usize).sum::<usize>() > 1000
        || counts.keys().any(|k| !ATOMS.iter().any(|(a, _)| a == k))
        || counts.values().all(|n| *n == 0)
    {
        return Err(invalid("Nominal isotope pattern requires nonempty CHNOPS composition, <=1000 atoms and 1..6 isotope bins"));
    }
    let mut probabilities = vec![0.0; maximum_isotopes as usize + 1];
    probabilities[0] = 1.0;
    for (atom, count) in &counts {
        let isotope: &[(usize, f64)] = match atom.as_str() {
            "C" => &[(0, 0.9893), (1, 0.0107)],
            "H" => &[(0, 0.999885), (1, 0.000115)],
            "N" => &[(0, 0.99636), (1, 0.00364)],
            "O" => &[(0, 0.99757), (1, 0.00038), (2, 0.00205)],
            "S" => &[(0, 0.9499), (1, 0.0075), (2, 0.0425), (4, 0.0001)],
            "P" => &[(0, 1.0)],
            _ => unreachable!(),
        };
        for _ in 0..*count {
            let mut next = vec![0.0; probabilities.len()];
            for (i, p) in probabilities.iter().enumerate() {
                for (shift, abundance) in isotope {
                    if i + shift < next.len() {
                        next[i + shift] += p * abundance;
                    }
                }
            }
            probabilities = next;
        }
    }
    let retained_probability = probabilities.iter().sum();
    Ok(NominalPattern{atom_counts:counts,probabilities,retained_probability,abundance_source:"NIST Atomic Weights and Isotopic Compositions representative abundances (C/H/N/O/S/P); https://physics.nist.gov/cgi-bin/Compositions/stand_alone.pl?all=all".into(),warnings:vec!["Nominal isotope bins use rounded mass shifts; not a resolved fine-isotopologue model".into(),"Natural abundances vary; isotopic labelling/enrichment and overlapping compounds invalidate this model".into(),"Observed nominal bins integrate positive signal within +/-0.5/charge m/z; unrelated peaks can inflate similarity".into()]})
}
