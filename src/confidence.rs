//! Rule-based confidence that an automatic peak integration found the right peak and measured
//! the right area. Every deduction names its reason, so a reviewer sees why a peak needs a
//! second look. No learned model is involved; weights and limits are documented defaults.
//!
//! The metrics follow the usual peak-quality criteria (peak shape and symmetry, modality,
//! jaggedness, signal-to-noise, sampling, boundary placement, consistency across samples), see
//! e.g. MetaClean and TargetedMSQC, plus an integration-robustness check that re-integrates
//! with perturbed settings.
use crate::chromatography;
use crate::integration::{self, Algorithm, Candidate, Integration};
use crate::processing;
use serde::{Deserialize, Serialize};

/// Scores at or above this are `High` (the peak can proceed as is).
pub const HIGH_FROM: u8 = 80;
/// Scores below this are `Low` (the peak needs a human or AI review).
pub const LOW_BELOW: u8 = 55;

#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    High,
    Medium,
    Low,
}
impl Tier {
    pub fn from_score(score: u8) -> Tier {
        if score >= HIGH_FROM {
            Tier::High
        } else if score < LOW_BELOW {
            Tier::Low
        } else {
            Tier::Medium
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Tier::High => "High",
            Tier::Medium => "Medium",
            Tier::Low => "Low",
        }
    }
}

#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    /// The measured quantity in words, e.g. "asymmetry 2.6".
    pub value: String,
    /// 0 (bad) to 1 (good).
    pub score: f64,
    pub weight: f64,
    /// Machine-readable reason, present when the check lowered the confidence.
    pub flag: Option<String>,
}

#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quality {
    /// 0-100.
    pub score: u8,
    pub tier: Tier,
    /// Critical problems (clipped peak, SNR below 3, ...) limit the score no matter how good
    /// the rest is; flagged checks limit it further (see `recompute`).
    pub cap: u8,
    /// Full width at half height of the automatic peak, minutes.
    pub fwhm: Option<f64>,
    pub checks: Vec<Check>,
}
impl Quality {
    /// Reasons that lowered the confidence.
    pub fn flags(&self) -> Vec<&str> {
        self.checks
            .iter()
            .filter_map(|c| c.flag.as_deref())
            .collect()
    }
    fn recompute(&mut self) {
        let weight: f64 = self.checks.iter().map(|c| c.weight).sum();
        let mean = if weight > 0.0 {
            self.checks.iter().map(|c| c.score * c.weight).sum::<f64>() / weight
        } else {
            0.0
        };
        // High means nothing significant was flagged: one flag keeps a peak below High, and a
        // severe flag or several flags keep it Medium at best, however good the rest of the
        // peak is. A mild flag (low weight, only slightly under its limit) just lowers the mean.
        let flagged: Vec<&Check> = self
            .checks
            .iter()
            .filter(|c| c.flag.is_some() && (c.weight >= 0.08 || c.score < 0.25))
            .collect();
        let flag_cap = match flagged.len() {
            0 => 100,
            1 if flagged[0].score >= 0.25 => HIGH_FROM - 1,
            _ => HIGH_FROM - 11,
        };
        self.score = ((mean * 100.0).round() as u8).min(self.cap).min(flag_cap);
        self.tier = Tier::from_score(self.score);
    }
}

/// 0 at `bad`, 1 at `good`, linear between (works in either direction).
fn ramp(x: f64, bad: f64, good: f64) -> f64 {
    ((x - bad) / (good - bad)).clamp(0.0, 1.0)
}

struct Builder {
    checks: Vec<Check>,
    cap: u8,
}
impl Builder {
    fn add(&mut self, name: &str, value: String, score: f64, weight: f64, flag: &str, limit: f64) {
        self.checks.push(Check {
            name: name.into(),
            value,
            score,
            weight,
            flag: (score < limit).then(|| flag.to_owned()),
        });
    }
    fn cap(&mut self, limit: u8) {
        self.cap = self.cap.min(limit);
    }
}

/// Crossing of `level` walking from `apex` in one direction through the series.
fn crossing(c: &[[f64; 2]], apex: usize, level: f64, forward: bool) -> Option<f64> {
    let mut j = apex;
    loop {
        let next = if forward {
            (j + 1 < c.len()).then_some(j + 1)?
        } else {
            j.checked_sub(1)?
        };
        if c[next][1] <= level {
            let (a, b) = (c[j], c[next]);
            return Some(if a[1] == b[1] {
                a[0]
            } else {
                a[0] + (b[0] - a[0]) * (level - a[1]) / (b[1] - a[1])
            });
        }
        j = next;
    }
}
fn pearson(a: &[f64], b: &[f64]) -> Option<f64> {
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        sab += (x - ma) * (y - mb);
        saa += (x - ma).powi(2);
        sbb += (y - mb).powi(2);
    }
    (saa > 0.0 && sbb > 0.0).then(|| sab / (saa * sbb).sqrt())
}
/// Settings variants used to test how much the area depends on the exact settings.
fn perturbations(cfg: &Integration) -> Vec<Integration> {
    let mut out = Vec::new();
    for factor in [0.5, 2.0] {
        let f = (cfg.boundary_fraction * factor).min(0.5);
        if f != cfg.boundary_fraction {
            out.push(Integration {
                boundary_fraction: f,
                ..cfg.clone()
            });
        }
    }
    if cfg.algorithm == Algorithm::SavgolSnr {
        for window in [cfg.sg_window.saturating_sub(2), cfg.sg_window + 2] {
            if window >= 3 && window > cfg.sg_degree {
                out.push(Integration {
                    sg_window: window,
                    ..cfg.clone()
                });
            }
        }
    } else {
        for smoothing in [cfg.smoothing.saturating_sub(1), (cfg.smoothing + 1).min(10)] {
            if smoothing != cfg.smoothing {
                out.push(Integration {
                    smoothing,
                    ..cfg.clone()
                });
            }
        }
    }
    out
}

/// Judges the first (selected) candidate. `None` when there is no peak to judge.
pub fn assess(
    trace: &[[f64; 2]],
    rt_window: [f64; 2],
    expected_rt: f64,
    cfg: &Integration,
    candidates: &[Candidate],
) -> Option<Quality> {
    let chosen = candidates.first()?;
    let peak = &chosen.peak;
    let window: Vec<[f64; 2]> = trace
        .iter()
        .copied()
        .filter(|p| p[0] >= rt_window[0] && p[0] <= rt_window[1])
        .collect();
    let raw: Vec<[f64; 2]> = window
        .iter()
        .copied()
        .filter(|p| p[0] >= peak.start && p[0] <= peak.end)
        .collect();
    if window.len() < 5 || raw.len() < 3 {
        return None;
    }
    let (y_start, y_end) = (
        processing::interpolate_at(trace, peak.start),
        processing::interpolate_at(trace, peak.end),
    );
    let line = |t: f64| y_start + (y_end - y_start) * (t - peak.start) / (peak.end - peak.start);
    let floor = window.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    // Smoothed, baseline-corrected shape of the integrated region.
    let smooth: Vec<[f64; 2]> = integration::smoothed(&window, cfg)
        .into_iter()
        .filter(|p| p[0] >= peak.start && p[0] <= peak.end)
        .map(|p| [p[0], p[1] - line(p[0])])
        .collect();
    let apex = smooth
        .iter()
        .enumerate()
        .max_by(|a, b| a.1[1].total_cmp(&b.1[1]))
        .map(|(i, _)| i)?;
    let height = smooth[apex][1];
    let mut b = Builder {
        checks: Vec::new(),
        cap: 100,
    };
    if height.is_nan() || height <= 0.0 || smooth.len() < 3 {
        b.add(
            "Signal",
            "no signal above the baseline".into(),
            0.0,
            1.0,
            "no_signal",
            1.0,
        );
        b.cap(20);
        let mut q = Quality {
            score: 0,
            tier: Tier::Low,
            cap: b.cap,
            fwhm: None,
            checks: b.checks,
        };
        q.recompute();
        return Some(q);
    }
    let left = crossing(&smooth, apex, height / 2.0, false);
    let right = crossing(&smooth, apex, height / 2.0, true);
    let fwhm = left.zip(right).map(|(l, r)| r - l).filter(|w| *w > 0.0);
    let unit = fwhm.unwrap_or((peak.end - peak.start) / 3.0).max(1e-6);

    // 1. Other peaks competing for the same window, and shoulders on this one.
    let others = &candidates[1..];
    if others.is_empty() {
        b.add(
            "Competing peaks",
            "none".into(),
            1.0,
            0.15,
            "competing_peak",
            0.7,
        );
    } else {
        let span = (peak.height - floor).max(f64::MIN_POSITIVE);
        let (rel, at) = others
            .iter()
            .map(|o| ((o.peak.height - floor) / span, o.apex_rt))
            .fold((0.0_f64, 0.0), |m, v| if v.0 > m.0 { v } else { m });
        let mut score = ramp(rel, 0.6, 0.1);
        if rel > 1.0 {
            score = score.min(0.1);
        }
        if rel >= 0.5 {
            b.cap(65);
        }
        b.add(
            "Competing peaks",
            format!(
                "another peak at {at:.2} min ({:.0} % of this one)",
                rel * 100.0
            ),
            score,
            0.15,
            "competing_peak",
            0.7,
        );
    }
    let d2: Vec<f64> = (1..smooth.len() - 1)
        .map(|i| smooth[i - 1][1] - 2.0 * smooth[i][1] + smooth[i + 1][1])
        .collect();
    let strongest = d2.iter().copied().fold(0.0_f64, f64::min);
    // Curvature below this is indistinguishable from noise (second difference of white noise
    // has sigma * sqrt(6)).
    let noise_sigma = {
        let x: Vec<f64> = window.iter().map(|p| p[0]).collect();
        let y: Vec<f64> = window.iter().map(|p| p[1]).collect();
        chromatography::noise_sigma(&x, &y)
    };
    let d2_noise = if noise_sigma.is_finite() {
        noise_sigma * 6.0_f64.sqrt()
    } else {
        0.0
    };
    // Lobes of strongly negative curvature, each separated from the next by a stretch of
    // clearly positive curvature. Ripples and shallow dips between them do not count.
    let lobe_depth = (0.25 * strongest.abs()).max(4.0 * d2_noise);
    let gap_height = (0.10 * strongest.abs()).max(4.0 * d2_noise);
    let mut lobes = 0;
    let mut inside = false;
    let mut gap_max = f64::NEG_INFINITY;
    for d in &d2 {
        let negative = strongest < 0.0 && *d < -lobe_depth;
        if negative {
            if !inside && (lobes == 0 || gap_max > gap_height) {
                lobes += 1;
            }
            inside = true;
            gap_max = f64::NEG_INFINITY;
        } else {
            inside = false;
            gap_max = gap_max.max(*d);
        }
    }
    b.add(
        "Peak modality",
        if lobes > 1 {
            format!("{lobes} curvature maxima: shoulder or merged peaks")
        } else {
            "single maximum".into()
        },
        if lobes > 1 { 0.3 } else { 1.0 },
        0.08,
        "shoulder_or_split",
        0.7,
    );

    // 2. Retention time agreement with the expected RT.
    let dev = peak.apex_rt - expected_rt;
    b.add(
        "Retention time",
        format!("{dev:+.3} min from expected"),
        ramp(dev.abs() / unit, 3.0, 0.75),
        0.08,
        "rt_offset",
        0.5,
    );

    // 3. Shape: asymmetry at 10 % height, similarity to a Gaussian, jaggedness.
    let tenth = height * 0.1;
    match (
        crossing(&smooth, apex, tenth, false),
        crossing(&smooth, apex, tenth, true),
    ) {
        (Some(l), Some(r)) if smooth[apex][0] > l && r > smooth[apex][0] => {
            let asym = (r - smooth[apex][0]) / (smooth[apex][0] - l);
            let worst = asym.max(1.0 / asym);
            b.add(
                "Asymmetry",
                format!("{asym:.2} at 10 % height"),
                ramp(worst, 3.0, 1.6),
                0.10,
                if asym >= 1.0 { "tailing" } else { "fronting" },
                0.6,
            );
        }
        _ => b.add(
            "Asymmetry",
            "flank does not return to 10 % height inside the integrated region".into(),
            0.5,
            0.10,
            "flank_truncated",
            0.6,
        ),
    }
    match fwhm {
        Some(w) => {
            let sigma = w / 2.354_82;
            let apex_rt = smooth[apex][0];
            let ys: Vec<f64> = smooth.iter().map(|p| p[1]).collect();
            let gs: Vec<f64> = smooth
                .iter()
                .map(|p| height * (-((p[0] - apex_rt) / sigma).powi(2) / 2.0).exp())
                .collect();
            let r = pearson(&ys, &gs).unwrap_or(0.0);
            b.add(
                "Gaussian similarity",
                format!("r = {r:.3}"),
                ramp(r, 0.80, 0.97),
                0.08,
                "poor_peak_shape",
                0.4,
            );
        }
        None => b.add(
            "Gaussian similarity",
            "peak width could not be measured".into(),
            0.4,
            0.08,
            "poor_peak_shape",
            0.5,
        ),
    }
    let threshold = 0.02 * height;
    let diffs: Vec<f64> = raw
        .windows(2)
        .map(|w| w[1][1] - w[0][1])
        .filter(|d| d.abs() > threshold)
        .collect();
    let changes = diffs
        .windows(2)
        .filter(|w| w[0].signum() != w[1].signum())
        .count();
    let jag = changes as f64 / (raw.len() - 2).max(1) as f64;
    b.add(
        "Jaggedness",
        format!("{:.0} % direction changes", jag * 100.0),
        ramp(jag, 0.45, 0.15),
        0.05,
        "noisy_peak",
        0.5,
    );

    // 4. Signal to noise.
    let snr = if noise_sigma.is_finite() && noise_sigma > 0.0 {
        height / noise_sigma
    } else {
        f64::INFINITY
    };
    if snr < 3.0 {
        b.cap(50);
    }
    b.add(
        "Signal to noise",
        if snr.is_finite() {
            format!("SNR {snr:.0}")
        } else {
            "no measurable noise".into()
        },
        if snr.is_finite() {
            ramp(snr.max(1.0).ln(), 3.0_f64.ln(), 20.0_f64.ln())
        } else {
            1.0
        },
        0.15,
        "low_snr",
        0.5,
    );

    // 5. Points across the peak.
    let across = match (left, right) {
        (Some(l), Some(r)) => raw.iter().filter(|p| p[0] >= l && p[0] <= r).count(),
        _ => raw.len() / 3,
    };
    if across < 5 {
        b.cap(70);
    }
    b.add(
        "Sampling",
        format!(
            "{across} scans across the half width, {} across the peak",
            raw.len()
        ),
        ramp(across as f64, 3.0, 8.0).min(ramp(raw.len() as f64, 5.0, 12.0)),
        0.10,
        "few_points",
        0.6,
    );

    // 6. Boundaries: not clipped by the window, ending near the baseline.
    let top = (peak.height - floor).max(f64::MIN_POSITIVE);
    let edge = (y_start.max(y_end) - floor).max(0.0) / top;
    let mut boundary = ramp(edge, 0.35, 0.08);
    let mut value = format!("boundaries at {:.0} % of the peak height", edge * 100.0);
    if chosen.clipped {
        boundary = boundary.min(0.25);
        value = "peak runs into the edge of the RT window".into();
        b.cap(70);
    }
    b.add(
        "Boundaries",
        value,
        boundary,
        0.10,
        "boundary_not_at_baseline",
        0.5,
    );

    // 7. Detector saturation: a flat top of identical maxima.
    let top_value = raw.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
    let plateau = raw.iter().filter(|p| p[1] == top_value).count();
    if plateau >= 3 {
        b.cap(70);
    }
    b.add(
        "Saturation",
        if plateau >= 3 {
            format!("{plateau} scans at the same maximum")
        } else {
            "no flat top".into()
        },
        if plateau >= 3 { 0.2 } else { 1.0 },
        0.05,
        "saturated_plateau",
        0.5,
    );

    // 8. Does the area depend on the exact settings?
    if cfg.algorithm != Algorithm::Manual {
        let area = peak.area;
        let mut worst = 0.0_f64;
        for variant in perturbations(cfg) {
            let found = integration::candidates(trace, rt_window, expected_rt, &variant)
                .into_iter()
                .filter(|c| (c.apex_rt - chosen.apex_rt).abs() <= 2.0 * unit)
                .min_by(|a, b| {
                    (a.apex_rt - chosen.apex_rt)
                        .abs()
                        .total_cmp(&(b.apex_rt - chosen.apex_rt).abs())
                });
            let change = match found {
                Some(c) if area != 0.0 => ((c.peak.area - area) / area).abs(),
                _ => 1.0,
            };
            worst = worst.max(change);
        }
        if worst >= 0.35 {
            b.cap(60);
        }
        b.add(
            "Area stability",
            format!(
                "area changes up to {:.0} % with slightly different settings",
                worst * 100.0
            ),
            ramp(worst, 0.35, 0.05),
            0.14,
            "unstable_area",
            0.5,
        );
    }

    let mut q = Quality {
        score: 0,
        tier: Tier::Low,
        cap: b.cap,
        fwhm,
        checks: b.checks,
    };
    q.recompute();
    Some(q)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n.is_multiple_of(2) {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    } else {
        v[n / 2]
    }
}
/// Compares each analyte's peaks across the samples of a batch and adds the result as extra
/// checks. `peaks[i]` is (analyte, apex RT) of the i-th quality; entries without a peak are
/// skipped. Needs four or more samples per analyte.
pub fn batch_consistency(qualities: &mut [Option<Quality>], peaks: &[Option<(String, f64)>]) {
    const NAMES: [&str; 2] = ["Retention time vs batch", "Peak width vs batch"];
    for q in qualities.iter_mut().flatten() {
        q.checks.retain(|c| !NAMES.contains(&c.name.as_str()));
    }
    let mut analytes: Vec<&str> = peaks.iter().flatten().map(|(a, _)| a.as_str()).collect();
    analytes.sort_unstable();
    analytes.dedup();
    for analyte in analytes {
        let idx: Vec<usize> = (0..peaks.len())
            .filter(|&i| {
                qualities[i].is_some() && peaks[i].as_ref().is_some_and(|(a, _)| a == analyte)
            })
            .collect();
        if idx.len() < 4 {
            continue;
        }
        let rts: Vec<f64> = idx.iter().map(|&i| peaks[i].as_ref().unwrap().1).collect();
        let med = median(rts.clone());
        let mad = median(rts.iter().map(|r| (r - med).abs()).collect());
        let widths: Vec<f64> = idx
            .iter()
            .filter_map(|&i| qualities[i].as_ref().and_then(|q| q.fwhm))
            .collect();
        let med_width = (!widths.is_empty()).then(|| median(widths));
        let scale = (1.4826 * mad).max(0.5 * med_width.unwrap_or(0.0)).max(0.02);
        for &i in &idx {
            let rt = peaks[i].as_ref().unwrap().1;
            let z = (rt - med).abs() / scale;
            let fwhm = qualities[i].as_ref().and_then(|q| q.fwhm);
            let q = qualities[i].as_mut().unwrap();
            q.checks.push(Check {
                name: NAMES[0].into(),
                value: format!("{:+.3} min from the batch median", rt - med),
                score: ramp(z, 6.0, 2.0),
                weight: 0.10,
                flag: (z > 4.0).then(|| "rt_differs_from_batch".into()),
            });
            if let (Some(w), Some(m)) = (fwhm, med_width) {
                let ratio = (w / m).max(m / w);
                q.checks.push(Check {
                    name: NAMES[1].into(),
                    value: format!("{:.2} x the batch median width", w / m),
                    score: ramp(ratio, 2.5, 1.5),
                    weight: 0.05,
                    flag: (ratio > 2.0).then(|| "width_differs_from_batch".into()),
                });
            }
            q.recompute();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gauss(center: f64, sigma: f64, height: f64) -> impl Fn(f64) -> f64 {
        move |t| height * (-((t - center) / sigma).powi(2) / 2.0).exp()
    }
    fn trace(step: f64, f: impl Fn(f64) -> f64) -> Vec<[f64; 2]> {
        (0..=(10.0 / step) as usize)
            .map(|i| {
                let t = i as f64 * step;
                [t, f(t)]
            })
            .collect()
    }
    fn noise(i: usize, amp: f64) -> f64 {
        let mut s = (i as u64 + 1).wrapping_mul(6364136223846793005);
        s ^= s >> 29;
        s = s.wrapping_mul(1442695040888963407);
        ((s >> 40) % 2001) as f64 / 1000.0 * amp - amp
    }
    fn judge(t: &[[f64; 2]], expected: f64) -> Option<Quality> {
        let cfg = Integration::default();
        let c = integration::candidates(t, [3.0, 7.0], expected, &cfg);
        assess(t, [3.0, 7.0], expected, &cfg, &c)
    }
    fn flags(q: &Quality) -> Vec<String> {
        q.flags().into_iter().map(str::to_owned).collect()
    }
    #[test]
    fn a_clean_peak_is_high_confidence() {
        let t = trace(0.02, gauss(5.0, 0.12, 100_000.0));
        let q = judge(&t, 5.0).unwrap();
        assert!(q.score >= HIGH_FROM, "{q:#?}");
        assert_eq!(q.tier, Tier::High);
        assert!(q.flags().is_empty(), "{:?}", q.flags());
        assert!(q.fwhm.unwrap() > 0.25 && q.fwhm.unwrap() < 0.32);
    }
    #[test]
    fn a_clean_peak_on_an_offset_baseline_in_a_tight_window_is_high() {
        // Baseline sits higher on the left than on the right, and the window starts close to
        // the peak: neither may be mistaken for a clipped or multi-lobed peak.
        let t = trace(0.01, |x| {
            gauss(5.0, 0.04, 100_000.0)(x) + 2_000.0 - 150.0 * x
        });
        let cfg = Integration::default();
        let window = [4.8, 5.4];
        let c = integration::candidates(&t, window, 5.0, &cfg);
        let q = assess(&t, window, 5.0, &cfg, &c).unwrap();
        assert!(!c[0].clipped);
        assert!(q.flags().is_empty(), "{:?}", q.checks);
        assert!(q.score >= 90, "{q:#?}");
    }
    #[test]
    fn tailing_is_flagged() {
        // Exponentially modified Gaussian: sharp front, long tail.
        let t = trace(0.02, |x| {
            let g = gauss(5.0, 0.1, 100_000.0)(x);
            let tail = if x > 5.0 {
                100_000.0 * (-(x - 5.0) / 0.5).exp()
            } else {
                0.0
            };
            g.max(tail)
        });
        let q = judge(&t, 5.0).unwrap();
        assert!(flags(&q).contains(&"tailing".to_owned()), "{:?}", flags(&q));
        assert!(q.score < HIGH_FROM);
    }
    #[test]
    fn a_second_peak_in_the_window_lowers_confidence() {
        let t = trace(0.02, |x| {
            gauss(4.5, 0.1, 100_000.0)(x) + gauss(5.6, 0.1, 80_000.0)(x)
        });
        let q = judge(&t, 4.5).unwrap();
        assert!(flags(&q).contains(&"competing_peak".to_owned()));
        assert!(q.score <= 65, "{}", q.score);
    }
    #[test]
    fn a_shoulder_is_detected() {
        let t = trace(0.02, |x| {
            gauss(5.0, 0.12, 100_000.0)(x) + gauss(5.28, 0.1, 55_000.0)(x)
        });
        let q = judge(&t, 5.0).unwrap();
        assert!(
            flags(&q)
                .iter()
                .any(|f| f == "shoulder_or_split" || f == "competing_peak"),
            "{:?}",
            flags(&q)
        );
        assert_ne!(q.tier, Tier::High);
    }
    #[test]
    fn noise_sparse_clipped_and_saturated_peaks_are_penalised() {
        // Noisy peak close to the noise floor.
        let t: Vec<[f64; 2]> = trace(0.02, gauss(5.0, 0.12, 300.0))
            .into_iter()
            .enumerate()
            .map(|(i, p)| [p[0], p[1] + 100.0 + noise(i, 90.0)])
            .collect();
        let q = judge(&t, 5.0);
        assert!(q.is_none_or(|q| q.tier != Tier::High && !q.flags().is_empty()));
        // Sparse sampling.
        let sparse = trace(0.2, gauss(5.0, 0.2, 100_000.0));
        let q = judge(&sparse, 5.0).unwrap();
        assert!(
            flags(&q).contains(&"few_points".to_owned()),
            "{:?}",
            flags(&q)
        );
        assert!(q.tier != Tier::High);
        // Peak running into the window edge.
        let clipped = trace(0.02, gauss(3.2, 0.3, 100_000.0));
        let q = judge(&clipped, 3.2).unwrap();
        assert!(q.score <= 70, "{q:#?}");
        // Flat top.
        let sat = trace(0.02, |x| gauss(5.0, 0.15, 400_000.0)(x).min(100_000.0));
        let q = judge(&sat, 5.0).unwrap();
        assert!(flags(&q).contains(&"saturated_plateau".to_owned()));
        assert!(q.score <= 70);
    }
    #[test]
    fn an_rt_far_from_expected_is_flagged() {
        let t = trace(0.02, gauss(5.0, 0.1, 100_000.0));
        let q = judge(&t, 6.2).unwrap();
        assert!(flags(&q).contains(&"rt_offset".to_owned()));
    }
    #[test]
    fn scores_stay_within_range_and_caps_hold() {
        let t = trace(0.02, gauss(5.0, 0.12, 100_000.0));
        let mut q = judge(&t, 5.0).unwrap();
        q.cap = 40;
        q.recompute();
        assert_eq!((q.score, q.tier), (40, Tier::Low));
        assert_eq!(Tier::from_score(80), Tier::High);
        assert_eq!(Tier::from_score(54), Tier::Low);
        assert_eq!(Tier::from_score(55), Tier::Medium);
    }
    #[test]
    fn batch_consistency_flags_the_outlier_sample() {
        let apexes = [5.00, 5.01, 4.99, 5.02, 5.00, 5.9];
        let mut qualities: Vec<Option<Quality>> = apexes
            .iter()
            .map(|a| judge(&trace(0.02, gauss(*a, 0.12, 100_000.0)), *a))
            .collect();
        let peaks: Vec<Option<(String, f64)>> =
            apexes.iter().map(|a| Some(("A".to_owned(), *a))).collect();
        batch_consistency(&mut qualities, &peaks);
        let outlier = qualities[5].as_ref().unwrap();
        assert!(outlier.flags().contains(&"rt_differs_from_batch"));
        assert!(qualities[0].as_ref().unwrap().flags().is_empty());
        // Re-running replaces, not duplicates, the batch checks.
        batch_consistency(&mut qualities, &peaks);
        assert_eq!(
            qualities[5]
                .as_ref()
                .unwrap()
                .checks
                .iter()
                .filter(|c| c.name.ends_with("vs batch"))
                .count(),
            2
        );
    }
}
