//! Versioned chromatographic processing. RT is minutes; area is intensity * minute.
//! Smoothing uses local polynomial least squares on actual RT, with shifted full
//! windows at edges (SciPy's interp convention on a uniform grid). No padding.
use crate::domain::{EngineError, Result};
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub rt: f64,
    pub intensity: Option<f64>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    pub name: String,
    pub samples: Vec<Sample>,
}
impl Trace {
    pub fn from_points(name: String, points: &[[f64; 2]]) -> Self {
        Self {
            name,
            samples: points
                .iter()
                .map(|p| Sample {
                    rt: p[0],
                    intensity: Some(p[1]),
                })
                .collect(),
        }
    }
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "model", rename_all = "snake_case", deny_unknown_fields)]
pub enum Baseline {
    None,
    EndpointChord,
    /// Positive-peak asymmetric least squares: W + lambda D' D. D is an
    /// RT-aware second difference, normalized by median sampling interval.
    AsymmetricLeastSquares {
        lambda: f64,
        asymmetry: f64,
        iterations: usize,
    },
    /// Lower rolling quantile, followed by a local mean. Useful for broad drift;
    /// windows must exceed peak widths. Can underestimate negative noise.
    RollingQuantile {
        window: usize,
        quantile: f64,
    },
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SavitzkyGolay {
    pub window: usize,
    pub degree: usize,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub smoothing: Option<SavitzkyGolay>,
    pub baseline: Baseline,
    pub minimum_height: f64,
    pub minimum_prominence: f64,
    pub minimum_snr: f64,
    pub minimum_width_minutes: f64,
    pub maximum_width_minutes: f64,
    pub boundary_fraction: f64,
    /// Gaps larger than this split the trace. None accepts any finite spacing.
    pub maximum_gap_minutes: Option<f64>,
    /// false integrates corrected raw data; true integrates corrected smooth data.
    pub integrate_smoothed: bool,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            smoothing: Some(SavitzkyGolay {
                window: 7,
                degree: 2,
            }),
            baseline: Baseline::AsymmetricLeastSquares {
                lambda: 1e5,
                asymmetry: 0.01,
                iterations: 10,
            },
            minimum_height: 0.0,
            minimum_prominence: 0.0,
            minimum_snr: 3.0,
            minimum_width_minutes: 0.0,
            maximum_width_minutes: 1e6,
            boundary_fraction: 0.01,
            maximum_gap_minutes: None,
            integrate_smoothed: false,
        }
    }
}
fn invalid(message: &str) -> EngineError {
    EngineError::new("invalid_parameters", message)
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.smoothing.as_ref().is_some_and(|s| {
            s.window < 3
                || s.window > 501
                || s.window % 2 == 0
                || s.degree > 5
                || s.degree >= s.window
        }) {
            return Err(invalid(
                "SG requires odd window 3..501, degree 0..5 below window",
            ));
        }
        if [
            self.minimum_height,
            self.minimum_prominence,
            self.minimum_snr,
            self.minimum_width_minutes,
            self.maximum_width_minutes,
            self.boundary_fraction,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.0)
            || self.minimum_width_minutes > self.maximum_width_minutes
            || self.maximum_width_minutes == 0.0
            || self.boundary_fraction > 0.5
            || self
                .maximum_gap_minutes
                .is_some_and(|v| !v.is_finite() || v <= 0.0)
        {
            return Err(invalid(
                "Invalid detection thresholds, width, boundary fraction or gap (minutes)",
            ));
        }
        match self.baseline {
            Baseline::AsymmetricLeastSquares {
                lambda,
                asymmetry,
                iterations,
            } if !lambda.is_finite()
                || lambda <= 0.0
                || !asymmetry.is_finite()
                || asymmetry <= 0.0
                || asymmetry >= 0.5
                || iterations == 0
                || iterations > 100 =>
            {
                return Err(invalid(
                    "AsLS requires positive lambda, 0 < asymmetry < 0.5, iterations 1..100",
                ))
            }
            Baseline::RollingQuantile { window, quantile }
                if !(3..=10001).contains(&window)
                    || window % 2 == 0
                    || !quantile.is_finite()
                    || !(0.0..=0.5).contains(&quantile) =>
            {
                return Err(invalid(
                    "Rolling quantile requires odd window 3..10001 and quantile 0..0.5",
                ))
            }
            _ => (),
        }
        Ok(())
    }
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Signals {
    pub rt_minutes: Vec<f64>,
    pub raw: Vec<f64>,
    pub smoothed: Vec<f64>,
    pub baseline: Vec<f64>,
    pub corrected_raw: Vec<f64>,
    pub corrected_smoothed: Vec<f64>,
    /// The exact signed, baseline-corrected signal used for trapezoids.
    pub integration_signal: Vec<f64>,
    pub noise_sigma: f64,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Peak {
    pub segment: usize,
    pub start_minutes: f64,
    pub end_minutes: f64,
    pub apex_minutes: f64,
    pub height: f64,
    pub prominence: f64,
    pub fwhm_minutes: Option<f64>,
    pub area_intensity_minutes: f64,
    pub snr: Option<f64>,
    pub flags: Vec<String>,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Correction {
    Manual {
        intervals: Vec<[f64; 2]>,
        reason: String,
    },
    /// Transfer bounds using an explicit reference-to-target RT shift. No
    /// implicit alignment or inferred identity is claimed.
    Reference {
        intervals: Vec<[f64; 2]>,
        shift_minutes: f64,
        reason: String,
    },
    Restore {
        revision: usize,
        reason: String,
    },
    Accept {
        reason: String,
    },
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Revision {
    pub actor: String,
    pub correction: Correction,
    pub peaks: Vec<Peak>,
    pub reviewed: bool,
}
#[cfg_attr(feature = "mcp-headless", derive(rmcp::schemars::JsonSchema))]
#[cfg_attr(feature = "mcp-headless", schemars(crate = "rmcp::schemars"))]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Analysis {
    pub algorithm: String,
    pub raw: Trace,
    pub config: Config,
    pub segments: Vec<Signals>,
    pub automatic: Vec<Peak>,
    pub revisions: Vec<Revision>,
    pub warnings: Vec<String>,
    pub rt_unit: String,
    pub intensity_unit: String,
    pub area_unit: String,
}
impl Analysis {
    pub fn peaks(&self) -> &[Peak] {
        self.revisions.last().map_or(&self.automatic, |r| &r.peaks)
    }
}
fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n == 0 {
        0.0
    } else if n.is_multiple_of(2) {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    } else {
        v[n / 2]
    }
}
fn quantile(mut v: Vec<f64>, q: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    let p = q * (v.len() - 1) as f64;
    let i = p.floor() as usize;
    v[i] + (v[p.ceil() as usize] - v[i]) * p.fract()
}
/// Robust MAD of interpolation residuals, divided by their independent-noise
/// standard deviation. RT-weighted neighbors remove linear drift even on an
/// irregular grid. Assumes independent, approximately Gaussian noise.
pub fn noise_sigma(x: &[f64], y: &[f64]) -> f64 {
    let r: Vec<_> = (1..y.len().saturating_sub(1))
        .map(|i| {
            let a = (x[i + 1] - x[i]) / (x[i + 1] - x[i - 1]);
            let b = 1.0 - a;
            (y[i] - a * y[i - 1] - b * y[i + 1]) / (1.0 + a * a + b * b).sqrt()
        })
        .collect();
    let m = median(r.clone());
    median(r.into_iter().map(|v| (v - m).abs()).collect()) / 0.6744897501960817
}
/// Reorthogonalized QR local least squares; centered/scaled RT limits conditioning.
#[allow(clippy::needless_range_loop)] // Triangular QR indexing expresses dependencies explicitly.
fn smooth(x: &[f64], y: &[f64], s: &SavitzkyGolay) -> Result<Vec<f64>> {
    let n = y.len();
    let w = s.window;
    (0..n)
        .map(|i| {
            let lo = i.saturating_sub(w / 2).min(n - w);
            let scale = x[lo + w - 1] - x[lo];
            let z: Vec<_> = x[lo..lo + w].iter().map(|v| (v - x[i]) / scale).collect();
            let mut q: Vec<Vec<f64>> = Vec::new();
            let k = s.degree + 1;
            let mut r = vec![vec![0.0; k]; k];
            for j in 0..k {
                let mut v: Vec<_> = z.iter().map(|z| z.powi(j as i32)).collect();
                for _ in 0..2 {
                    for a in 0..j {
                        let dot: f64 = q[a].iter().zip(&v).map(|(a, b)| a * b).sum();
                        r[a][j] += dot;
                        for (v, q) in v.iter_mut().zip(&q[a]) {
                            *v -= dot * q;
                        }
                    }
                }
                let norm = v.iter().map(|v| v * v).sum::<f64>().sqrt();
                if norm < 1e-12 {
                    return Err(invalid("Ill-conditioned local polynomial sampling"));
                }
                r[j][j] = norm;
                q.push(v.into_iter().map(|v| v / norm).collect());
            }
            let mut c: Vec<f64> = q
                .iter()
                .map(|q| q.iter().zip(&y[lo..lo + w]).map(|(a, b)| a * b).sum())
                .collect();
            for j in (0..k).rev() {
                for a in j + 1..k {
                    c[j] -= r[j][a] * c[a];
                }
                c[j] /= r[j][j];
            }
            Ok(c[0])
        })
        .collect()
}
// Cholesky solve of a symmetric pentadiagonal matrix, O(n) storage/time.
fn band_solve(diag: &[f64], sub1: &[f64], sub2: &[f64], rhs: &[f64]) -> Result<Vec<f64>> {
    let n = diag.len();
    let mut d = vec![0.0; n];
    let mut a = vec![0.0; n];
    let mut b = vec![0.0; n];
    for i in 0..n {
        if i >= 2 {
            b[i] = sub2[i] / d[i - 2];
        }
        if i >= 1 {
            a[i] = (sub1[i] - b[i] * a[i - 1]) / d[i - 1];
        }
        let v = diag[i] - a[i] * a[i] - b[i] * b[i];
        if !v.is_finite() || v <= 0.0 {
            return Err(invalid(
                "Baseline system is numerically singular; reduce lambda or sampling disparity",
            ));
        }
        d[i] = v.sqrt();
    }
    let mut z = vec![0.0; n];
    for i in 0..n {
        z[i] = (rhs[i]
            - if i > 0 { a[i] * z[i - 1] } else { 0.0 }
            - if i > 1 { b[i] * z[i - 2] } else { 0.0 })
            / d[i];
    }
    for i in (0..n).rev() {
        z[i] = (z[i]
            - if i + 1 < n { a[i + 1] * z[i + 1] } else { 0.0 }
            - if i + 2 < n { b[i + 2] * z[i + 2] } else { 0.0 })
            / d[i];
    }
    Ok(z)
}
fn baseline(x: &[f64], y: &[f64], model: &Baseline) -> Result<Vec<f64>> {
    let n = y.len();
    match *model {
        Baseline::None => Ok(vec![0.0; n]),
        Baseline::EndpointChord => Ok(x
            .iter()
            .map(|t| y[0] + (y[n - 1] - y[0]) * (t - x[0]) / (x[n - 1] - x[0]))
            .collect()),
        Baseline::RollingQuantile {
            window,
            quantile: q,
        } => {
            let low: Vec<_> = (0..n)
                .map(|i| {
                    quantile(
                        y[i.saturating_sub(window / 2)..(i + window / 2 + 1).min(n)].to_vec(),
                        q,
                    )
                })
                .collect();
            Ok((0..n)
                .map(|i| {
                    let v = &low[i.saturating_sub(window / 2)..(i + window / 2 + 1).min(n)];
                    v.iter().sum::<f64>() / v.len() as f64
                })
                .collect())
        }
        Baseline::AsymmetricLeastSquares {
            lambda,
            asymmetry,
            iterations,
        } => {
            let spacing = median(x.windows(2).map(|w| w[1] - w[0]).collect());
            let mut pd = vec![0.0; n];
            let mut p1 = vec![0.0; n];
            let mut p2 = vec![0.0; n];
            for i in 1..n - 1 {
                let h0 = (x[i] - x[i - 1]) / spacing;
                let h1 = (x[i + 1] - x[i]) / spacing;
                let c = [
                    2.0 / (h0 * (h0 + h1)),
                    -2.0 / (h0 * h1),
                    2.0 / (h1 * (h0 + h1)),
                ];
                for j in 0..3 {
                    pd[i - 1 + j] += lambda * c[j] * c[j];
                }
                p1[i] += lambda * c[0] * c[1];
                p1[i + 1] += lambda * c[1] * c[2];
                p2[i + 1] += lambda * c[0] * c[2];
            }
            let mut weights = vec![1.0; n];
            let mut z = y.to_vec();
            for _ in 0..iterations {
                let d: Vec<_> = pd.iter().zip(&weights).map(|(a, b)| a + b).collect();
                let rhs: Vec<_> = y.iter().zip(&weights).map(|(a, b)| a * b).collect();
                z = band_solve(&d, &p1, &p2, &rhs)?;
                for i in 0..n {
                    weights[i] = if y[i] > z[i] {
                        asymmetry
                    } else {
                        1.0 - asymmetry
                    };
                }
            }
            Ok(z)
        }
    }
}
fn crossing(x0: f64, y0: f64, x1: f64, y1: f64, level: f64) -> f64 {
    if y1 == y0 {
        (x0 + x1) / 2.0
    } else {
        x0 + (x1 - x0) * (level - y0) / (y1 - y0)
    }
}
fn at(x: &[f64], y: &[f64], t: f64) -> f64 {
    let i = x.partition_point(|v| *v < t);
    if i == 0 {
        y[0]
    } else if i == x.len() {
        y[y.len() - 1]
    } else {
        y[i - 1] + (y[i] - y[i - 1]) * (t - x[i - 1]) / (x[i] - x[i - 1])
    }
}
fn measure(s: &Signals, segment: usize, start: f64, end: f64) -> Result<Peak> {
    let x = &s.rt_minutes;
    let y = &s.corrected_smoothed;
    if !start.is_finite()
        || !end.is_finite()
        || start >= end
        || start < x[0]
        || end > x[x.len() - 1]
    {
        return Err(invalid(
            "Bounds must lie within one contiguous segment (minutes)",
        ));
    }
    let mut nodes = vec![(start, at(x, &s.integration_signal, start))];
    nodes.extend(
        x.iter()
            .zip(&s.integration_signal)
            .filter(|(t, _)| **t > start && **t < end)
            .map(|(t, v)| (*t, *v)),
    );
    nodes.push((end, at(x, &s.integration_signal, end)));
    let area: f64 = nodes
        .windows(2)
        .map(|w| (w[1].0 - w[0].0) * (w[0].1 + w[1].1) / 2.0)
        .sum();
    if !area.is_finite() {
        return Err(invalid("Integration area overflow"));
    }
    let mut candidates = vec![(start, at(x, y, start))];
    candidates.extend(
        x.iter()
            .zip(y)
            .filter(|(t, _)| **t > start && **t < end)
            .map(|(t, v)| (*t, *v)),
    );
    candidates.push((end, at(x, y, end)));
    let apex = candidates
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap();
    let half = apex.1 / 2.0;
    let left = candidates
        .windows(2)
        .rfind(|w| w[1].0 <= apex.0 && w[0].1 <= half && w[1].1 > half);
    let right = candidates
        .windows(2)
        .find(|w| w[0].0 >= apex.0 && w[0].1 > half && w[1].1 <= half);
    let mut flags = vec![];
    let width = if let (Some(l), Some(r)) = (left, right) {
        Some(
            crossing(r[0].0, r[0].1, r[1].0, r[1].1, half)
                - crossing(l[0].0, l[0].1, l[1].0, l[1].1, half),
        )
    } else {
        flags.push("truncated_half_height".into());
        None
    };
    let snr = if s.noise_sigma > 0.0 {
        Some(apex.1 / s.noise_sigma)
    } else {
        None
    };
    if snr.is_some_and(|v| !v.is_finite()) {
        return Err(invalid("Signal-to-noise ratio overflow"));
    }
    Ok(Peak {
        segment,
        start_minutes: start,
        end_minutes: end,
        apex_minutes: apex.0,
        height: apex.1,
        prominence: (apex.1 - at(x, y, start).max(at(x, y, end))).max(0.0),
        fwhm_minutes: width,
        area_intensity_minutes: area,
        snr,
        flags,
    })
}
fn detect(s: &Signals, segment: usize, c: &Config) -> Result<Vec<Peak>> {
    let y = &s.corrected_smoothed;
    let x = &s.rt_minutes;
    let n = y.len();
    let mut peaks = vec![];
    let mut maxima = vec![];
    let mut i = 1;
    while i < n - 1 {
        if y[i] > y[i - 1] {
            let mut j = i;
            while j + 1 < n && y[j + 1] == y[i] {
                j += 1;
            }
            if j + 1 < n && y[j] > y[j + 1] {
                maxima.push((i + j) / 2);
            }
            i = j;
        }
        i += 1;
    }
    for apex in maxima {
        let height = y[apex];
        if height <= 0.0 || height < c.minimum_height || height < c.minimum_snr * s.noise_sigma {
            continue;
        }
        let mut left = apex;
        while left > 0 && y[left - 1] <= height {
            left -= 1;
        }
        let mut right = apex;
        while right + 1 < n && y[right + 1] <= height {
            right += 1;
        }
        let li = (left..=apex)
            .min_by(|&a, &b| y[a].total_cmp(&y[b]))
            .unwrap();
        let ri = (apex..=right)
            .min_by(|&a, &b| y[a].total_cmp(&y[b]))
            .unwrap();
        let prominence = height - y[li].max(y[ri]);
        if prominence <= 0.0
            || prominence < c.minimum_prominence
            || prominence < c.minimum_snr * s.noise_sigma
        {
            continue;
        }
        let level = height * c.boundary_fraction;
        let mut l = apex;
        while l > li && y[l] > level {
            l -= 1;
        }
        let mut r = apex;
        while r < ri && y[r] > level {
            r += 1;
        }
        let start = if y[l] <= level && l < apex {
            crossing(x[l], y[l], x[l + 1], y[l + 1], level)
        } else {
            x[l]
        };
        let end = if y[r] <= level && r > apex {
            crossing(x[r - 1], y[r - 1], x[r], y[r], level)
        } else {
            x[r]
        };
        if start >= end {
            continue;
        }
        let mut p = measure(s, segment, start, end)?;
        p.apex_minutes = x[apex];
        p.height = height;
        p.prominence = prominence;
        if p.fwhm_minutes
            .is_some_and(|w| w < c.minimum_width_minutes || w > c.maximum_width_minutes)
            || (p.fwhm_minutes.is_none()
                && (c.minimum_width_minutes > 0.0 || c.maximum_width_minutes < x[n - 1] - x[0]))
        {
            continue;
        }
        if li == 0 || ri == n - 1 {
            p.flags.push("trace_edge_boundary".into());
        }
        peaks.push(p);
    }
    // Resolved maxima are partitioned at the intervening minimum. No component
    // deconvolution is implied: each area is the observed signal on its side.
    for i in 1..peaks.len() {
        if peaks[i - 1].end_minutes > peaks[i].start_minutes {
            let lo = x.partition_point(|t| *t < peaks[i - 1].apex_minutes);
            let hi = x.partition_point(|t| *t <= peaks[i].apex_minutes);
            let valley = (lo..hi).min_by(|&a, &b| y[a].total_cmp(&y[b])).unwrap();
            let t = x[valley];
            let a = peaks[i - 1].start_minutes;
            let b = peaks[i].end_minutes;
            peaks[i - 1] = measure(s, segment, a, t)?;
            peaks[i] = measure(s, segment, t, b)?;
            peaks[i - 1].flags.push("valley_split_overlap".into());
            peaks[i].flags.push("valley_split_overlap".into());
        }
    }
    // Partitioning may truncate a half-height crossing. Apply width gates to
    // the final reported boundaries as well as to the initial candidate.
    peaks.retain(|p| {
        p.fwhm_minutes.map_or(
            c.minimum_width_minutes == 0.0 && c.maximum_width_minutes >= x[n - 1] - x[0],
            |w| w >= c.minimum_width_minutes && w <= c.maximum_width_minutes,
        )
    });
    Ok(peaks)
}
pub fn process(raw: Trace, config: Config) -> Result<Analysis> {
    config.validate()?;
    if raw.samples.len() > 1_000_000 {
        return Err(EngineError::new(
            "resource_limit",
            "Trace exceeds one million samples",
        ));
    }
    if raw
        .samples
        .iter()
        .any(|p| !p.rt.is_finite() || p.rt < 0.0 || p.intensity.is_some_and(|v| !v.is_finite()))
        || raw.samples.windows(2).any(|w| w[0].rt >= w[1].rt)
    {
        return Err(invalid(
            "RT must be finite, nonnegative, strictly increasing; intensity finite or null",
        ));
    }
    let mut chunks: Vec<Vec<&Sample>> = vec![];
    let mut chunk = vec![];
    let mut warnings = vec![];
    for p in &raw.samples {
        if (p.intensity.is_none()
            || chunk.last().is_some_and(|last: &&Sample| {
                config
                    .maximum_gap_minutes
                    .is_some_and(|g| p.rt - last.rt > g)
            }))
            && !chunk.is_empty()
        {
            chunks.push(std::mem::take(&mut chunk));
        }
        if p.intensity.is_some() {
            chunk.push(p);
        } else {
            warnings.push(format!("Missing intensity at {} min; segment split", p.rt));
        }
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    let mut segments = vec![];
    let mut automatic = vec![];
    for chunk in chunks {
        if chunk.len() < 3 {
            warnings
                .push("Segment with fewer than three samples retained only in raw input".into());
            continue;
        }
        let x: Vec<_> = chunk.iter().map(|p| p.rt).collect();
        let y: Vec<_> = chunk.iter().map(|p| p.intensity.unwrap()).collect();
        let smoothed = if let Some(s) = &config.smoothing {
            if s.window > y.len() {
                warnings.push("Segment shorter than SG window: smoothing bypassed".into());
                y.clone()
            } else {
                smooth(&x, &y, s)?
            }
        } else {
            y.clone()
        };
        let b = baseline(&x, &smoothed, &config.baseline)?;
        let corrected_raw: Vec<_> = y.iter().zip(&b).map(|(a, b)| a - b).collect();
        let corrected_smoothed: Vec<_> = smoothed.iter().zip(&b).map(|(a, b)| a - b).collect();
        let s = Signals {
            noise_sigma: noise_sigma(&x, &y),
            rt_minutes: x,
            raw: y,
            smoothed,
            baseline: b,
            integration_signal: if config.integrate_smoothed {
                corrected_smoothed.clone()
            } else {
                corrected_raw.clone()
            },
            corrected_raw,
            corrected_smoothed,
        };
        automatic.extend(detect(&s, segments.len(), &config)?);
        segments.push(s);
    }
    warnings.push("Valley-partition areas are observed areas, not component deconvolution; unresolved components may form one peak".into());
    if automatic.is_empty() {
        warnings.push("No peaks meet the configured criteria".into());
    }
    if segments.len() > 1 {
        warnings.push(
            "Disconnected segments: no integration across missing data or configured gaps".into(),
        );
    }
    if segments.iter().any(|s| s.raw.iter().any(|v| *v < 0.0)) {
        warnings.push("Negative intensities preserved; signed areas are reported".into());
    }
    if segments.iter().any(|s| !s.noise_sigma.is_finite())
        || segments
            .iter()
            .flat_map(|s| {
                [
                    &s.smoothed,
                    &s.baseline,
                    &s.corrected_raw,
                    &s.corrected_smoothed,
                ]
            })
            .flatten()
            .any(|v| !v.is_finite())
        || automatic
            .iter()
            .any(|p| !p.area_intensity_minutes.is_finite())
    {
        return Err(invalid("Numerical overflow during processing"));
    }
    Ok(Analysis {
        algorithm: "chromatography-v1".into(),
        raw,
        config,
        segments,
        automatic,
        revisions: vec![],
        warnings,
        rt_unit: "minute".into(),
        intensity_unit: "instrument intensity".into(),
        area_unit: "instrument intensity * minute".into(),
    })
}
fn correction_peaks(a: &Analysis, c: &Correction) -> Result<Vec<Peak>> {
    let (intervals, shift, reason) = match c {
        Correction::Manual { intervals, reason } => (intervals, 0.0, reason),
        Correction::Reference {
            intervals,
            shift_minutes,
            reason,
        } => (intervals, *shift_minutes, reason),
        Correction::Restore { revision, reason } => {
            if reason.trim().is_empty() {
                return Err(invalid("A correction reason is required"));
            }
            return if *revision == 0 {
                Ok(a.automatic.clone())
            } else {
                a.revisions
                    .get(revision - 1)
                    .map(|r| r.peaks.clone())
                    .ok_or_else(|| invalid("Unknown historical revision"))
            };
        }
        Correction::Accept { reason } => {
            if reason.trim().is_empty() || a.peaks().is_empty() {
                return Err(invalid("Review requires peaks and a reason"));
            }
            return Ok(a.peaks().to_vec());
        }
    };
    if reason.trim().is_empty()
        || !shift.is_finite()
        || intervals.is_empty()
        || intervals.len() > 10000
    {
        return Err(invalid(
            "Specify intervals, finite RT shift and a correction reason",
        ));
    }
    let mut out = vec![];
    for [start, end] in intervals {
        let lo = start + shift;
        let hi = end + shift;
        let (segment, s) = a
            .segments
            .iter()
            .enumerate()
            .find(|(_, s)| lo >= s.rt_minutes[0] && hi <= s.rt_minutes[s.rt_minutes.len() - 1])
            .ok_or_else(|| invalid("Interval crosses a missing region or lies outside trace"))?;
        let mut p = measure(s, segment, lo, hi)?;
        p.flags.push(
            if matches!(c, Correction::Reference { .. }) {
                "reference_assisted"
            } else {
                "manual"
            }
            .into(),
        );
        out.push(p);
    }
    out.sort_by(|a, b| a.start_minutes.total_cmp(&b.start_minutes));
    if out
        .windows(2)
        .any(|w| w[0].end_minutes > w[1].start_minutes)
    {
        return Err(invalid(
            "Integration intervals must not overlap; use explicit valley bounds",
        ));
    }
    Ok(out)
}
/// Validate supplied state by numerical replay before modifying/reviewing it.
/// Revision zero is automatic; undo appends a restore revision, deleting nothing.
pub fn verify(a: &Analysis) -> Result<()> {
    if a.revisions.len() > 10000 {
        return Err(EngineError::new(
            "resource_limit",
            "Too many correction revisions",
        ));
    }
    let mut verified = process(a.raw.clone(), a.config.clone())?;
    for r in &a.revisions {
        let peaks = correction_peaks(&verified, &r.correction)?;
        if peaks != r.peaks
            || r.actor.trim().is_empty()
            || r.reviewed != matches!(r.correction, Correction::Accept { .. })
        {
            return Err(EngineError::new(
                "corrupt_result",
                "Revision replay differs",
            ));
        }
        verified.revisions.push(r.clone());
    }
    if verified != *a {
        return Err(EngineError::new(
            "corrupt_result",
            "Processed signals differ from replay",
        ));
    }
    Ok(())
}
pub fn revise(
    a: &Analysis,
    expected_revision: usize,
    correction: Correction,
    actor: &str,
    preview: bool,
) -> Result<Analysis> {
    if expected_revision != a.revisions.len() {
        return Err(EngineError::new(
            "stale_revision",
            "Chromatogram revision changed",
        ));
    }
    if actor.trim().is_empty() {
        return Err(invalid("An actor is required"));
    }
    verify(a)?;
    let mut verified = a.clone();
    let peaks = correction_peaks(a, &correction)?;
    let revision = Revision {
        actor: actor.into(),
        reviewed: matches!(correction, Correction::Accept { .. }),
        correction,
        peaks,
    };
    verified.revisions.push(revision);
    // Preview returns a proposed copy. Callers must not commit this copy; the
    // engine labels preview output separately and GUI requires explicit apply.
    if preview {
        verified
            .warnings
            .push("Non-destructive preview; proposal is not committed".into());
    }
    Ok(verified)
}

/// Lossless numeric CSV export of the current peak revision; raw data, signal
/// stages and full provenance remain in the JSON analysis/batch artifact.
pub fn peaks_csv(analyses: &[Analysis]) -> String {
    let mut out=String::from("trace,revision,reviewed,segment,start_min,end_min,apex_min,height,prominence,fwhm_min,area_intensity_min,snr,flags\n");
    let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    for a in analyses {
        for p in a.peaks() {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                quote(&a.raw.name),
                a.revisions.len(),
                a.revisions.last().is_some_and(|r| r.reviewed),
                p.segment,
                p.start_minutes,
                p.end_minutes,
                p.apex_minutes,
                p.height,
                p.prominence,
                p.fwhm_minutes.map(|v| v.to_string()).unwrap_or_default(),
                p.area_intensity_minutes,
                p.snr.map(|v| v.to_string()).unwrap_or_default(),
                quote(&p.flags.join(";"))
            ));
        }
    }
    out
}
