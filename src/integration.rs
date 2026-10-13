//! Per-analyte peak integration shared by the GUI, engine, targeted QC, CLI and MCP.
//!
//! An [`Integration`] stored in a method fully determines how an analyte's peak is found
//! (algorithm), bounded and measured (baseline), so a method file reproduces results.
use crate::chromatography::{self, Baseline, SavitzkyGolay};
use crate::processing;
use crate::quant::{Peak, Status};
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Algorithm {
    /// Moving-average detection, boundaries where the peak falls to a fraction of its height.
    #[default]
    Classic,
    /// Like classic, but adjacent peaks are split at their valley once the dip is deep enough.
    Valley,
    /// Savitzky–Golay smoothing with signal-to-noise and width acceptance limits.
    SavgolSnr,
    /// No detection: integrate a fixed retention-time window.
    Manual,
}
impl Algorithm {
    pub const ALL: [Algorithm; 4] = [
        Algorithm::Classic,
        Algorithm::Valley,
        Algorithm::SavgolSnr,
        Algorithm::Manual,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Algorithm::Classic => "Classic (smoothed apex)",
            Algorithm::Valley => "Valley / drop-line",
            Algorithm::SavgolSnr => "Savitzky–Golay + SNR",
            Algorithm::Manual => "Fixed window",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Algorithm::Classic => "Finds the apex on a moving-average trace and ends the peak where it falls to the boundary fraction of its height.",
            Algorithm::Valley => "Like classic, but neighbouring peaks are separated at the valley between them when the dip is deeper than the valley depth.",
            Algorithm::SavgolSnr => "Savitzky–Golay smoothing; peaks must exceed a signal-to-noise ratio and lie within a width range.",
            Algorithm::Manual => "No peak search: always integrates the fixed window below. Use for compounds where detection is unreliable.",
        }
    }
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BaselineKind {
    /// Straight line between the two integration boundaries.
    #[default]
    StraightLine,
    /// Integrate from zero intensity.
    Zero,
    /// Horizontal line at the lower of the two boundary intensities.
    ValleyDrop,
    /// Asymmetric-least-squares curve fitted around the peak.
    Als,
}
impl BaselineKind {
    pub const ALL: [BaselineKind; 4] = [
        BaselineKind::StraightLine,
        BaselineKind::Zero,
        BaselineKind::ValleyDrop,
        BaselineKind::Als,
    ];
    pub fn label(self) -> &'static str {
        match self {
            BaselineKind::StraightLine => "Straight line",
            BaselineKind::Zero => "Zero (drop to axis)",
            BaselineKind::ValleyDrop => "Horizontal valley drop",
            BaselineKind::Als => "Asymmetric least squares",
        }
    }
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PeakSelection {
    /// Candidate closest to the expected retention time.
    #[default]
    NearestExpectedRt,
    Tallest,
    LargestArea,
}
impl PeakSelection {
    pub const ALL: [PeakSelection; 3] = [
        PeakSelection::NearestExpectedRt,
        PeakSelection::Tallest,
        PeakSelection::LargestArea,
    ];
    pub fn label(self) -> &'static str {
        match self {
            PeakSelection::NearestExpectedRt => "Nearest expected RT",
            PeakSelection::Tallest => "Tallest",
            PeakSelection::LargestArea => "Largest area",
        }
    }
}

/// Every integration setting of one analyte. Missing keys take the defaults below, so a
/// partial `[analytes.integration]` table is valid.
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Integration {
    pub algorithm: Algorithm,
    pub baseline: BaselineKind,
    pub peak_selection: PeakSelection,
    /// Moving-average half-width in scans (classic, valley).
    pub smoothing: u8,
    /// A peak must rise this far above its higher neighbouring valley (intensity units).
    pub minimum_prominence: f64,
    /// Peak ends where the signal falls to this fraction of the peak height above its valley.
    pub boundary_fraction: f64,
    /// Valley: dips shallower than this fraction of the peak height are not split.
    pub valley_depth: f64,
    /// Savitzky–Golay window (odd, scans) and polynomial degree.
    pub sg_window: usize,
    pub sg_degree: usize,
    /// Minimum peak height above its base, in multiples of the trace noise (SG + SNR).
    pub minimum_snr: f64,
    /// Full width at half height limits in minutes; 0 disables the limit (SG + SNR).
    pub minimum_width_minutes: f64,
    pub maximum_width_minutes: f64,
    pub als_lambda: f64,
    pub als_asymmetry: f64,
    pub als_iterations: usize,
    /// Fixed window for the manual algorithm; defaults to the analyte's RT window.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manual_start: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manual_end: Option<f64>,
}
impl Default for Integration {
    fn default() -> Self {
        Self {
            algorithm: Algorithm::Classic,
            baseline: BaselineKind::StraightLine,
            peak_selection: PeakSelection::NearestExpectedRt,
            smoothing: 2,
            minimum_prominence: 0.0,
            boundary_fraction: 0.05,
            valley_depth: 0.1,
            sg_window: 7,
            sg_degree: 2,
            minimum_snr: 3.0,
            minimum_width_minutes: 0.0,
            maximum_width_minutes: 0.0,
            als_lambda: 1e5,
            als_asymmetry: 0.01,
            als_iterations: 10,
            manual_start: None,
            manual_end: None,
        }
    }
}
impl Integration {
    pub fn validate(&self) -> Result<(), String> {
        let fin = |v: f64| v.is_finite() && v >= 0.0;
        if self.smoothing > 10 {
            return Err("smoothing must be 0–10".into());
        }
        if !fin(self.minimum_prominence) || !fin(self.minimum_snr) {
            return Err("minimum prominence and SNR must be nonnegative".into());
        }
        if !(0.0..=0.5).contains(&self.boundary_fraction) || !self.boundary_fraction.is_finite() {
            return Err("boundary fraction must be 0–0.5".into());
        }
        if !(0.0..=1.0).contains(&self.valley_depth) || !self.valley_depth.is_finite() {
            return Err("valley depth must be 0–1".into());
        }
        if self.sg_window < 3
            || self.sg_window > 501
            || self.sg_window.is_multiple_of(2)
            || self.sg_degree > 5
            || self.sg_degree >= self.sg_window
        {
            return Err(
                "Savitzky–Golay window must be odd (3–501) with degree below it (≤5)".into(),
            );
        }
        if !fin(self.minimum_width_minutes)
            || !fin(self.maximum_width_minutes)
            || (self.maximum_width_minutes > 0.0
                && self.minimum_width_minutes > self.maximum_width_minutes)
        {
            return Err("peak width limits must be nonnegative with minimum ≤ maximum".into());
        }
        if !(self.als_lambda.is_finite() && self.als_lambda > 0.0)
            || !(self.als_asymmetry > 0.0 && self.als_asymmetry < 0.5)
            || !(1..=100).contains(&self.als_iterations)
        {
            return Err("ALS needs lambda > 0, asymmetry in (0, 0.5) and 1–100 iterations".into());
        }
        for v in [self.manual_start, self.manual_end].into_iter().flatten() {
            if !fin(v) {
                return Err("fixed window bounds must be nonnegative".into());
            }
        }
        if let (Some(s), Some(e)) = (self.manual_start, self.manual_end) {
            if s >= e {
                return Err("fixed window start must be before its end".into());
            }
        }
        Ok(())
    }
}

/// Lowest point reached walking down one flank of the peak at `apex`. Ripples smaller than
/// `fraction` of the apex height above the running minimum are crossed, so noise in a long
/// tail does not end the peak early; with `fraction = 0` any rise ends the walk.
fn valley(smooth: &[[f64; 2]], apex: usize, forward: bool, fraction: f64) -> usize {
    let mut best = apex;
    let mut j = apex;
    loop {
        j = match (forward, j) {
            (true, j) if j + 1 < smooth.len() => j + 1,
            (false, j) if j > 0 => j - 1,
            _ => return best,
        };
        if smooth[j][1] <= smooth[best][1] {
            best = j;
        } else if smooth[j][1] - smooth[best][1] > (smooth[apex][1] - smooth[best][1]) * fraction {
            return best;
        }
    }
}

pub(crate) fn smoothed(window: &[[f64; 2]], cfg: &Integration) -> Vec<[f64; 2]> {
    if cfg.algorithm == Algorithm::SavgolSnr {
        let n = window.len();
        let w = if cfg.sg_window > n {
            if n % 2 == 1 {
                n
            } else {
                n - 1
            }
        } else {
            cfg.sg_window
        };
        if w > cfg.sg_degree && w >= 3 {
            let x: Vec<f64> = window.iter().map(|p| p[0]).collect();
            let y: Vec<f64> = window.iter().map(|p| p[1]).collect();
            let s = SavitzkyGolay {
                window: w,
                degree: cfg.sg_degree,
            };
            if let Ok(z) = chromatography::smooth(&x, &y, &s) {
                return x.into_iter().zip(z).map(|(x, y)| [x, y]).collect();
            }
        }
        return window.to_vec();
    }
    processing::smooth_chromatogram(window.to_vec(), cfg.smoothing)
        .unwrap_or_else(|_| window.to_vec())
}

/// Full width at half height of the peak between `left..=right`, if both flanks cross.
fn fwhm(smooth: &[[f64; 2]], apex: usize, left: usize, right: usize, base: f64) -> Option<f64> {
    let half = base + (smooth[apex][1] - base) / 2.0;
    let mut l = None;
    for j in (left..apex).rev() {
        if smooth[j][1] <= half {
            let (a, b) = (smooth[j], smooth[j + 1]);
            l = Some(a[0] + (b[0] - a[0]) * (half - a[1]) / (b[1] - a[1]));
            break;
        }
    }
    let mut r = None;
    for j in apex..right {
        if smooth[j + 1][1] <= half {
            let (a, b) = (smooth[j], smooth[j + 1]);
            r = Some(a[0] + (b[0] - a[0]) * (half - a[1]) / (b[1] - a[1]));
            break;
        }
    }
    Some(r? - l?)
}

/// A peak found in the RT window, with what the confidence checks need to know about it.
#[derive(Clone, Debug)]
pub struct Candidate {
    pub peak: Peak,
    /// Smoothed apex position.
    pub apex_rt: f64,
    /// The peak runs into the edge of the RT window while still high.
    pub clipped: bool,
}
/// Finds the analyte's peak in `trace` (the full chromatogram) inside `rt_window`. Whether the
/// result can be trusted is judged separately by [`crate::confidence`]; status is `Automatic`
/// whenever a peak was found.
pub fn detect(
    trace: &[[f64; 2]],
    rt_window: [f64; 2],
    expected_rt: f64,
    cfg: &Integration,
) -> (Option<Peak>, Status) {
    match candidates(trace, rt_window, expected_rt, cfg)
        .into_iter()
        .next()
    {
        Some(c) => (Some(c.peak), Status::Automatic),
        None => (None, Status::Missing),
    }
}
/// Every peak the algorithm finds in the window, the selected one first.
pub fn candidates(
    trace: &[[f64; 2]],
    rt_window: [f64; 2],
    expected_rt: f64,
    cfg: &Integration,
) -> Vec<Candidate> {
    if cfg.algorithm == Algorithm::Manual {
        let start = cfg.manual_start.unwrap_or(rt_window[0]);
        let end = cfg.manual_end.unwrap_or(rt_window[1]);
        return match measure(trace, start, end, cfg) {
            Ok(peak) if peak.area.is_finite() => vec![Candidate {
                apex_rt: peak.apex_rt,
                peak,
                clipped: false,
            }],
            _ => vec![],
        };
    }
    let window: Vec<_> = trace
        .iter()
        .copied()
        .filter(|p| p[0] >= rt_window[0] && p[0] <= rt_window[1])
        .collect();
    if window.len() < 3 {
        return vec![];
    }
    let smooth = smoothed(&window, cfg);
    let floor = smooth.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let tolerance = if cfg.algorithm == Algorithm::Valley {
        cfg.valley_depth
    } else {
        cfg.boundary_fraction
    };
    let sigma = if cfg.algorithm == Algorithm::SavgolSnr {
        let x: Vec<f64> = window.iter().map(|p| p[0]).collect();
        let y: Vec<f64> = window.iter().map(|p| p[1]).collect();
        chromatography::noise_sigma(&x, &y)
    } else {
        0.0
    };
    let mut candidates = Vec::new();
    for i in 1..smooth.len() - 1 {
        if smooth[i][1] <= smooth[i - 1][1] || smooth[i][1] < smooth[i + 1][1] {
            continue;
        }
        // Reject small ripples on an elevated baseline using local prominence.
        let valley_left = valley(&smooth, i, false, tolerance);
        let valley_right = valley(&smooth, i, true, tolerance);
        let base = smooth[valley_left][1].max(smooth[valley_right][1]);
        let prominence = smooth[i][1] - base;
        if prominence <= cfg.minimum_prominence {
            continue;
        }
        if cfg.algorithm == Algorithm::SavgolSnr
            && sigma.is_finite()
            && sigma > 0.0
            && prominence < cfg.minimum_snr * sigma
        {
            continue;
        }
        let left_threshold = smooth[valley_left][1]
            + (smooth[i][1] - smooth[valley_left][1]) * cfg.boundary_fraction;
        let right_threshold = smooth[valley_right][1]
            + (smooth[i][1] - smooth[valley_right][1]) * cfg.boundary_fraction;
        let clip_threshold = floor + (smooth[i][1] - floor) * cfg.boundary_fraction;
        let mut left = i;
        let mut right = i;
        while left > valley_left && smooth[left][1] > left_threshold {
            left -= 1;
        }
        while right < valley_right && smooth[right][1] > right_threshold {
            right += 1;
        }
        if cfg.algorithm == Algorithm::SavgolSnr {
            if let Some(width) = fwhm(&smooth, i, left, right, base) {
                if width < cfg.minimum_width_minutes
                    || (cfg.maximum_width_minutes > 0.0 && width > cfg.maximum_width_minutes)
                {
                    continue;
                }
            }
        }
        if let Ok(peak) = measure(trace, smooth[left][0], smooth[right][0], cfg) {
            if peak.area > 0.0 {
                candidates.push(Candidate {
                    apex_rt: smooth[i][0],
                    peak,
                    clipped: (left == 0 && smooth[left][1] > clip_threshold)
                        || (right == smooth.len() - 1 && smooth[right][1] > clip_threshold),
                });
            }
        }
    }
    match cfg.peak_selection {
        PeakSelection::NearestExpectedRt => candidates.sort_by(|a, b| {
            (a.apex_rt - expected_rt)
                .abs()
                .total_cmp(&(b.apex_rt - expected_rt).abs())
        }),
        PeakSelection::Tallest => {
            candidates.sort_by(|a, b| b.peak.height.total_cmp(&a.peak.height))
        }
        PeakSelection::LargestArea => {
            candidates.sort_by(|a, b| b.peak.area.total_cmp(&a.peak.area))
        }
    }
    candidates
}

/// The signal and baseline sampled across an integration interval, with interpolated edges.
pub struct Profile {
    pub signal: Vec<[f64; 2]>,
    pub baseline: Vec<[f64; 2]>,
}
fn check_bounds(trace: &[[f64; 2]], start: f64, end: f64) -> Result<(), String> {
    if trace.len() < 3
        || !start.is_finite()
        || !end.is_finite()
        || start >= end
        || start < trace[0][0]
        || end > trace[trace.len() - 1][0]
    {
        return Err("Integration bounds must be increasing and inside the measured trace.".into());
    }
    Ok(())
}
/// Signal and baseline between `start` and `end` for the configured baseline.
pub fn profile(
    trace: &[[f64; 2]],
    start: f64,
    end: f64,
    cfg: &Integration,
) -> Result<Profile, String> {
    check_bounds(trace, start, end)?;
    let (y0, y1) = (
        processing::interpolate_at(trace, start),
        processing::interpolate_at(trace, end),
    );
    let mut signal = vec![[start, y0]];
    signal.extend(trace.iter().copied().filter(|p| p[0] > start && p[0] < end));
    signal.push([end, y1]);
    let baseline: Vec<[f64; 2]> = match cfg.baseline {
        BaselineKind::StraightLine => signal
            .iter()
            .map(|p| [p[0], y0 + (y1 - y0) * (p[0] - start) / (end - start)])
            .collect(),
        BaselineKind::Zero => signal.iter().map(|p| [p[0], 0.0]).collect(),
        BaselineKind::ValleyDrop => {
            let low = y0.min(y1);
            signal.iter().map(|p| [p[0], low]).collect()
        }
        BaselineKind::Als => {
            let pad = end - start;
            let segment: Vec<[f64; 2]> = trace
                .iter()
                .copied()
                .filter(|p| p[0] >= start - pad && p[0] <= end + pad)
                .collect();
            if segment.len() < 5 {
                return Err("Too few scans around the peak for an ALS baseline.".into());
            }
            let x: Vec<f64> = segment.iter().map(|p| p[0]).collect();
            let y: Vec<f64> = segment.iter().map(|p| p[1]).collect();
            let z = chromatography::baseline(
                &x,
                &y,
                &Baseline::AsymmetricLeastSquares {
                    lambda: cfg.als_lambda,
                    asymmetry: cfg.als_asymmetry,
                    iterations: cfg.als_iterations,
                },
            )
            .map_err(|e| e.to_string())?;
            let curve: Vec<[f64; 2]> = x.into_iter().zip(z).map(|(x, y)| [x, y]).collect();
            signal
                .iter()
                .map(|p| [p[0], processing::interpolate_at(&curve, p[0])])
                .collect()
        }
    };
    Ok(Profile { signal, baseline })
}
/// Peak between explicit bounds, with the area taken above the configured baseline.
pub fn measure(
    trace: &[[f64; 2]],
    start: f64,
    end: f64,
    cfg: &Integration,
) -> Result<Peak, String> {
    check_bounds(trace, start, end)?;
    let apex = trace
        .iter()
        .filter(|p| p[0] >= start && p[0] <= end)
        .max_by(|a, b| a[1].total_cmp(&b[1]))
        .ok_or("The interval contains no scans.")?;
    let area = if cfg.baseline == BaselineKind::StraightLine {
        processing::integrate_peak(trace, start, end).map_err(|e| e.to_string())?
    } else {
        let p = profile(trace, start, end, cfg)?;
        p.signal
            .windows(2)
            .zip(p.baseline.windows(2))
            .map(|(s, b)| (s[1][0] - s[0][0]) * ((s[0][1] - b[0][1]) + (s[1][1] - b[1][1])) / 2.0)
            .sum()
    };
    Ok(Peak {
        start,
        end,
        apex_rt: apex[0],
        height: apex[1],
        area,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn gaussian(center: f64, sigma: f64, height: f64) -> impl Fn(f64) -> f64 {
        move |t| height * (-((t - center) / sigma).powi(2) / 2.0).exp()
    }
    fn trace(f: impl Fn(f64) -> f64) -> Vec<[f64; 2]> {
        (0..=400)
            .map(|i| {
                let t = i as f64 * 0.025;
                [t, f(t)]
            })
            .collect()
    }
    #[test]
    fn defaults_validate_and_round_trip_through_toml() {
        let cfg = Integration::default();
        cfg.validate().unwrap();
        let text = toml::to_string(&cfg).unwrap();
        assert_eq!(toml::from_str::<Integration>(&text).unwrap(), cfg);
        // A partial table takes defaults for the rest.
        let partial: Integration =
            toml::from_str("algorithm = 'savgol_snr'\nsg_window = 9").unwrap();
        assert_eq!(partial.algorithm, Algorithm::SavgolSnr);
        assert_eq!(partial.boundary_fraction, 0.05);
        assert!(toml::from_str::<Integration>("bogus = 1").is_err());
    }
    #[test]
    fn invalid_settings_are_rejected() {
        for cfg in [
            Integration {
                sg_window: 8,
                ..Default::default()
            },
            Integration {
                boundary_fraction: 0.9,
                ..Default::default()
            },
            Integration {
                als_asymmetry: 0.7,
                ..Default::default()
            },
            Integration {
                manual_start: Some(5.0),
                manual_end: Some(4.0),
                ..Default::default()
            },
        ] {
            assert!(cfg.validate().is_err());
        }
    }
    #[test]
    fn baselines_give_expected_areas_on_a_gaussian() {
        let t = trace(gaussian(5.0, 0.2, 1000.0));
        let exact = 1000.0 * 0.2 * (2.0 * std::f64::consts::PI).sqrt();
        for baseline in BaselineKind::ALL {
            let cfg = Integration {
                baseline,
                ..Default::default()
            };
            let p = measure(&t, 3.5, 6.5, &cfg).unwrap();
            // ALS follows the foot of the peak slightly, so it reads a few percent low.
            let tolerance = if baseline == BaselineKind::Als {
                0.1
            } else {
                0.01
            };
            assert!(
                (p.area - exact).abs() / exact < tolerance,
                "{baseline:?}: {}",
                p.area
            );
        }
    }
    #[test]
    fn zero_and_valley_drop_baselines_differ_on_a_raised_baseline() {
        let t = trace(|x| 500.0 + gaussian(5.0, 0.2, 1000.0)(x));
        let straight = measure(&t, 4.0, 6.0, &Integration::default()).unwrap().area;
        let zero = measure(
            &t,
            4.0,
            6.0,
            &Integration {
                baseline: BaselineKind::Zero,
                ..Default::default()
            },
        )
        .unwrap()
        .area;
        let drop = measure(
            &t,
            4.0,
            6.0,
            &Integration {
                baseline: BaselineKind::ValleyDrop,
                ..Default::default()
            },
        )
        .unwrap()
        .area;
        assert!((zero - straight - 500.0 * 2.0).abs() < 1.0);
        assert!((drop - straight).abs() < 1.0);
        let als = measure(
            &t,
            4.0,
            6.0,
            &Integration {
                baseline: BaselineKind::Als,
                ..Default::default()
            },
        )
        .unwrap()
        .area;
        assert!((als - straight).abs() / straight < 0.05);
    }
    #[test]
    fn classic_matches_the_legacy_detection() {
        let t = trace(gaussian(5.0, 0.15, 800.0));
        let (peak, status) = detect(&t, [3.0, 7.0], 5.0, &Integration::default());
        let p = peak.unwrap();
        assert_eq!(status, Status::Automatic);
        assert!((p.apex_rt - 5.0).abs() < 0.03);
    }
    #[test]
    fn savgol_snr_rejects_noise_and_accepts_a_real_peak() {
        let mut state = 12345u64;
        let noisy: Vec<[f64; 2]> = (0..=400)
            .map(|i| {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let n = ((state >> 33) % 21) as f64 - 10.0;
                [i as f64 * 0.025, 1000.0 + n * 10.0]
            })
            .collect();
        let cfg = Integration {
            algorithm: Algorithm::SavgolSnr,
            minimum_snr: 20.0,
            ..Default::default()
        };
        assert_eq!(detect(&noisy, [3.0, 7.0], 5.0, &cfg).1, Status::Missing);
        let real = trace(|x| gaussian(5.0, 0.15, 5000.0)(x) + ((x * 40.0) as i64 % 3) as f64);
        assert!(detect(&real, [3.0, 7.0], 5.0, &cfg).0.is_some());
        let wide = Integration {
            minimum_width_minutes: 5.0,
            ..cfg
        };
        assert_eq!(detect(&real, [3.0, 7.0], 5.0, &wide).1, Status::Missing);
    }
    #[test]
    fn valley_depth_controls_splitting_of_adjacent_peaks() {
        let f = |x: f64| gaussian(4.8, 0.12, 1000.0)(x) + gaussian(5.2, 0.12, 700.0)(x);
        let t = trace(f);
        let split = Integration {
            algorithm: Algorithm::Valley,
            baseline: BaselineKind::ValleyDrop,
            valley_depth: 0.05,
            smoothing: 0,
            ..Default::default()
        };
        let (a, _) = detect(&t, [3.0, 7.0], 4.8, &split);
        let a = a.unwrap();
        assert!(a.end < 5.1, "first peak ends at the valley: {}", a.end);
        let merged = Integration {
            valley_depth: 0.9,
            ..split
        };
        let (b, _) = detect(&t, [3.0, 7.0], 4.8, &merged);
        assert!(b.unwrap().end > 5.3);
    }
    #[test]
    fn fixed_window_and_peak_selection() {
        let t = trace(|x| gaussian(4.0, 0.1, 300.0)(x) + gaussian(6.0, 0.1, 900.0)(x));
        let fixed = Integration {
            algorithm: Algorithm::Manual,
            manual_start: Some(3.5),
            manual_end: Some(4.5),
            ..Default::default()
        };
        let (p, status) = detect(&t, [3.0, 7.0], 5.0, &fixed);
        assert_eq!(
            (p.as_ref().unwrap().start, status),
            (3.5, Status::Automatic)
        );
        let nearest = Integration {
            smoothing: 0,
            ..Default::default()
        };
        let tallest = Integration {
            peak_selection: PeakSelection::Tallest,
            ..nearest.clone()
        };
        let (p, status) = detect(&t, [3.0, 7.0], 4.1, &nearest);
        assert!((p.unwrap().apex_rt - 4.0).abs() < 0.05);
        assert_eq!(status, Status::Automatic);
        assert_eq!(candidates(&t, [3.0, 7.0], 4.1, &nearest).len(), 2);
        let (p, status) = detect(&t, [3.0, 7.0], 4.1, &tallest);
        assert!((p.unwrap().apex_rt - 6.0).abs() < 0.05);
        assert_eq!(status, Status::Automatic);
    }
}
