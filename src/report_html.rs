//! Human-readable analytical report (HTML with inline SVG, print-friendly).
//!
//! Built from the retained targeted batch, QC evaluation and integrated peaks that
//! `delivery::report` places under `review`. Machine-readable evidence is written
//! separately by the audit package; nothing here is raw JSON.
use crate::delivery::escape;
use crate::qc;
use crate::targeted::{BatchResult, Calibration, Concentration, Curve, Role, State, Target};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fmt::Write;

pub(crate) const STYLE: &str = "\
:root{--ink:#1c2430;--dim:#5b6675;--line:#d9dee5;--bg:#f5f7fa;--good:#1a7f4b;--warn:#b26a00;--bad:#c0392b;--accent:#176b87}\
*{box-sizing:border-box}\
body{font:14px/1.45 system-ui,Segoe UI,Helvetica,Arial,sans-serif;color:var(--ink);max-width:980px;margin:32px auto;padding:0 20px}\
h1{font-size:26px;margin:0 0 4px}h2{font-size:19px;margin:34px 0 10px;padding-bottom:5px;border-bottom:2px solid var(--line)}\
h3{font-size:15px;margin:20px 0 8px}p{margin:6px 0}.dim{color:var(--dim)}.small{font-size:12px}\
table{border-collapse:collapse;width:100%;margin:8px 0 14px;font-size:13px}\
th{text-align:left;background:var(--bg);font-weight:600;border-bottom:2px solid var(--line)}\
th,td{padding:5px 8px;vertical-align:top}td{border-bottom:1px solid var(--line)}\
td.n,th.n{text-align:right;font-variant-numeric:tabular-nums}\
tr.bad td{background:#fdf1ef}tr.warn td{background:#fff8e8}\
.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:10px;margin:14px 0}\
.card{border:1px solid var(--line);border-radius:8px;padding:10px 12px;background:#fff}\
.card .k{font-size:11px;text-transform:uppercase;letter-spacing:.04em;color:var(--dim)}\
.card .v{font-size:20px;font-weight:600;margin-top:2px}.card .s{font-size:12px;color:var(--dim)}\
.pill{display:inline-block;padding:1px 8px;border-radius:10px;font-size:12px;font-weight:600;background:#e8ecf1;color:var(--ink);white-space:nowrap}\
.pill.good{background:#dff3e8;color:var(--good)}.pill.warn{background:#fdefd2;color:var(--warn)}.pill.bad{background:#fbe1dd;color:var(--bad)}\
ul.attn{margin:6px 0;padding-left:20px}ul.attn li{margin:3px 0}\
.note{border-left:4px solid var(--warn);background:#fff8e8;padding:8px 12px;margin:10px 0}\
.note.bad{border-color:var(--bad);background:#fdf1ef}\
.plots{display:flex;flex-wrap:wrap;gap:16px;align-items:flex-start}\
.eq{font-family:ui-monospace,Consolas,monospace;font-size:13px;background:var(--bg);padding:6px 10px;border-radius:6px;display:inline-block}\
.peaks{display:grid;grid-template-columns:1fr 1fr;gap:14px}\
figure.peak{margin:0;border:1px solid var(--line);border-radius:8px;padding:8px;background:#fff;break-inside:avoid}\
figure.peak.flag{border-color:var(--warn)}figure.peak.bad{border-color:var(--bad)}\
figure.peak svg{width:100%;height:auto;display:block}figcaption{font-size:12px;margin-top:4px}\
svg text{font-family:system-ui,Segoe UI,Helvetica,Arial,sans-serif}\
footer{margin-top:40px;padding-top:10px;border-top:1px solid var(--line);font-size:12px;color:var(--dim)}\
@page{size:A4;margin:14mm}\
@media print{body{margin:0;max-width:none;padding:0;font-size:11.5px}h2{break-after:avoid}table,figure,.card,.note{break-inside:avoid}\
.pb{break-before:page}.peaks{grid-template-columns:1fr 1fr}}";

/// Presentation-ready copy of everything the report needs from a project.
struct Peak {
    file: String,
    analyte: String,
    status: String,
    start: f64,
    end: f64,
    apex: f64,
    area: f64,
    has_peak: bool,
    tier: String,
    score: i64,
    flags: Vec<String>,
    trace: Vec<[f64; 2]>,
}

fn base(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_string()
}
fn words(s: &str) -> String {
    s.replace('_', " ")
}
fn camel_words(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}
/// Four significant figures without trailing noise; scientific outside a readable range.
fn sig(x: f64) -> String {
    if !x.is_finite() {
        return "–".into();
    }
    if x == 0.0 {
        return "0".into();
    }
    let a = x.abs();
    if !(0.01..10_000.0).contains(&a) {
        return format!("{x:.3e}");
    }
    let decimals = if a >= 1000.0 {
        0
    } else if a >= 100.0 {
        1
    } else if a >= 10.0 {
        2
    } else {
        3
    };
    format!("{x:.decimals$}")
}
fn area_text(x: f64) -> String {
    if x.abs() >= 1e6 {
        format!("{x:.3e}")
    } else {
        format!("{x:.0}")
    }
}
fn pct(x: f64) -> String {
    format!("{x:+.1}%")
}
fn state_label(s: &State) -> &'static str {
    match s {
        State::Present => "Quantified",
        State::Missing => "No peak",
        State::Rejected => "Rejected",
        State::BelowDetection => "Below LOD",
        State::BelowLoq => "Below LOQ",
        State::BelowRange => "Below range",
        State::AboveRange => "Above range",
        State::Failed => "Failed",
        State::Ambiguous => "Ambiguous",
    }
}
fn state_is_problem(s: &State) -> bool {
    !matches!(s, State::Present | State::BelowLoq | State::BelowDetection)
}
fn role_label(r: &Role) -> &'static str {
    match r {
        Role::Standard => "Standard",
        Role::Blank => "Blank",
        Role::Qc => "QC",
        Role::Unknown => "Unknown",
    }
}
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}
fn date_text(unix_ms: u64) -> String {
    let secs = (unix_ms / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        rem / 3600,
        rem % 3600 / 60
    )
}

fn peaks_of(review: &Value) -> Vec<Peak> {
    review["peaks"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .map(|m| {
            let peak = &m["peak"];
            let num = |k: &str| peak[k].as_f64().unwrap_or(0.0);
            Peak {
                file: base(m["source"].as_str().unwrap_or("")),
                analyte: m["analyte"].as_str().unwrap_or("").to_string(),
                status: m["status"].as_str().unwrap_or("").to_string(),
                start: num("start"),
                end: num("end"),
                apex: num("apex_rt"),
                area: num("area"),
                has_peak: peak.is_object(),
                tier: m["quality"]["tier"].as_str().unwrap_or("").to_string(),
                score: m["quality"]["score"].as_i64().unwrap_or(0),
                flags: m["quality"]["flags"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|f| f.as_str().map(words))
                    .collect(),
                trace: m["trace"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|p| Some([p[0].as_f64()?, p[1].as_f64()?]))
                    .collect(),
            }
        })
        .collect()
}
fn reviewed(status: &str) -> bool {
    matches!(status, "Reviewed" | "Manual")
}
fn tier_class(tier: &str) -> &'static str {
    match tier {
        "high" => "good",
        "medium" => "warn",
        "low" => "bad",
        _ => "",
    }
}

// ---------- SVG helpers ----------

struct Frame {
    x0: f64,
    y0: f64,
    w: f64,
    h: f64,
    xmin: f64,
    xmax: f64,
    ymin: f64,
    ymax: f64,
}
impl Frame {
    fn px(&self, x: f64) -> f64 {
        self.x0 + (x - self.xmin) / (self.xmax - self.xmin).max(f64::MIN_POSITIVE) * self.w
    }
    fn py(&self, y: f64) -> f64 {
        self.y0 + self.h - (y - self.ymin) / (self.ymax - self.ymin).max(f64::MIN_POSITIVE) * self.h
    }
}
/// Round tick positions covering `[lo, hi]` with about `count` steps.
fn ticks(lo: f64, hi: f64, count: usize) -> Vec<f64> {
    let span = (hi - lo).max(f64::MIN_POSITIVE);
    let raw = span / count as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|m| m * mag)
        .find(|s| *s >= raw)
        .unwrap_or(10.0 * mag);
    let mut t = (lo / step).ceil() * step;
    let mut out = vec![];
    while t <= hi + step * 1e-9 && out.len() < 40 {
        out.push(t);
        t += step;
    }
    out
}
fn tick_text(v: f64) -> String {
    if v != 0.0 && (v.abs() >= 1e5 || v.abs() < 1e-2) {
        format!("{v:.1e}")
    } else {
        let s = format!("{v:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}
fn axes(svg: &mut String, f: &Frame, xlabel: &str, ylabel: &str, xticks: &[f64], yticks: &[f64]) {
    for t in yticks {
        let y = f.py(*t);
        let _ = write!(
            svg,
            "<line x1=\"{:.1}\" y1=\"{y:.1}\" x2=\"{:.1}\" y2=\"{y:.1}\" stroke=\"#e3e7ec\"/><text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" text-anchor=\"end\" fill=\"#5b6675\">{}</text>",
            f.x0,
            f.x0 + f.w,
            f.x0 - 5.0,
            y + 3.0,
            tick_text(*t)
        );
    }
    for t in xticks {
        let x = f.px(*t);
        let _ = write!(
            svg,
            "<line x1=\"{x:.1}\" y1=\"{:.1}\" x2=\"{x:.1}\" y2=\"{:.1}\" stroke=\"#e3e7ec\"/><text x=\"{x:.1}\" y=\"{:.1}\" font-size=\"10\" text-anchor=\"middle\" fill=\"#5b6675\">{}</text>",
            f.y0,
            f.y0 + f.h,
            f.y0 + f.h + 14.0,
            tick_text(*t)
        );
    }
    let _ = write!(
        svg,
        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"none\" stroke=\"#9aa5b1\"/>\
         <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" text-anchor=\"middle\" fill=\"#1c2430\">{}</text>\
         <text transform=\"translate(12 {:.1}) rotate(-90)\" font-size=\"11\" text-anchor=\"middle\" fill=\"#1c2430\">{}</text>",
        f.x0,
        f.y0,
        f.w,
        f.h,
        f.x0 + f.w / 2.0,
        f.y0 + f.h + 32.0,
        escape(xlabel),
        f.y0 + f.h / 2.0,
        escape(ylabel)
    );
}

fn calibration_svg(cal: &Calibration, unit: &str, tolerance: f64, others: &[(f64, f64)]) -> String {
    let (w, h) = (520.0, 340.0);
    let xmax_data = cal
        .points
        .iter()
        .map(|p| p.x)
        .chain(others.iter().map(|o| o.0))
        .fold(cal.range[1], f64::max);
    let ymax_data = cal
        .points
        .iter()
        .map(|p| p.response)
        .chain(others.iter().map(|o| o.1))
        .chain([cal.response(cal.range[1])])
        .fold(0.0, f64::max);
    let f = Frame {
        x0: 70.0,
        y0: 14.0,
        w: w - 70.0 - 14.0,
        h: h - 14.0 - 48.0,
        xmin: 0.0,
        xmax: xmax_data * 1.05,
        ymin: 0.0,
        ymax: (ymax_data * 1.08).max(1.0),
    };
    let mut s = format!("<svg viewBox=\"0 0 {w} {h}\" width=\"{w}\" height=\"{h}\" role=\"img\" aria-label=\"Calibration curve\">");
    axes(
        &mut s,
        &f,
        &format!("Concentration ({unit})"),
        "Response (area)",
        &ticks(f.xmin, f.xmax, 6),
        &ticks(f.ymin, f.ymax, 5),
    );
    let n = 80;
    let path = (0..=n)
        .map(|i| {
            let x = cal.range[0] + (cal.range[1] - cal.range[0]) * i as f64 / n as f64;
            format!(
                "{}{:.1},{:.1}",
                if i == 0 { "M" } else { "L" },
                f.px(x),
                f.py(cal.response(x))
            )
        })
        .collect::<String>();
    let _ = write!(
        s,
        "<path d=\"{path}\" fill=\"none\" stroke=\"#c0392b\" stroke-width=\"1.6\"/>"
    );
    for (x, y) in others {
        let _ = write!(
            s,
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"4.5\" fill=\"#f0a04b\" stroke=\"#8a5200\"/>",
            f.px(*x),
            f.py(*y)
        );
    }
    for p in &cal.points {
        let off = p
            .accuracy_percent
            .is_some_and(|a| (a - 100.0).abs() > tolerance)
            || !p.flags.is_empty();
        let _ = write!(
            s,
            "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"8\" height=\"8\" fill=\"{}\" stroke=\"#10213a\"/>",
            f.px(p.x) - 4.0,
            f.py(p.response) - 4.0,
            if off { "#c0392b" } else { "#3b82d6" }
        );
    }
    let (lx, ly) = (f.x0 + 10.0, f.y0 + 14.0);
    let _ = write!(
        s,
        "<rect x=\"{lx}\" y=\"{:.1}\" width=\"8\" height=\"8\" fill=\"#3b82d6\"/><text x=\"{:.1}\" y=\"{ly}\" font-size=\"10\">Standards</text>\
         <circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"4\" fill=\"#f0a04b\"/><text x=\"{:.1}\" y=\"{ly}\" font-size=\"10\">QC / unknowns</text>\
         <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#c0392b\" stroke-width=\"1.6\"/><text x=\"{:.1}\" y=\"{ly}\" font-size=\"10\">Fit</text></svg>",
        ly - 8.0,
        lx + 12.0,
        lx + 92.0,
        ly - 4.0,
        lx + 100.0,
        lx + 180.0,
        ly - 4.0,
        lx + 196.0,
        ly - 4.0,
        lx + 200.0
    );
    s
}

fn interp(trace: &[[f64; 2]], x: f64) -> f64 {
    match trace.windows(2).find(|w| w[0][0] <= x && x <= w[1][0]) {
        Some(w) => {
            let t = (x - w[0][0]) / (w[1][0] - w[0][0]).max(f64::MIN_POSITIVE);
            w[0][1] + t * (w[1][1] - w[0][1])
        }
        None => trace.first().map_or(0.0, |p| p[1]),
    }
}
fn peak_svg(p: &Peak) -> String {
    let (w, h) = (380.0, 170.0);
    let (lo, hi) = if p.has_peak && p.end > p.start {
        let pad = (p.end - p.start) * 0.7;
        (p.start - pad, p.end + pad)
    } else {
        (
            p.trace.first().map_or(0.0, |t| t[0]),
            p.trace.last().map_or(1.0, |t| t[0]),
        )
    };
    let pts: Vec<[f64; 2]> = p
        .trace
        .iter()
        .copied()
        .filter(|t| t[0] >= lo && t[0] <= hi)
        .collect();
    let ymax = pts.iter().map(|t| t[1]).fold(0.0, f64::max);
    let f = Frame {
        x0: 54.0,
        y0: 8.0,
        w: w - 54.0 - 10.0,
        h: h - 8.0 - 36.0,
        xmin: lo,
        xmax: hi,
        ymin: 0.0,
        ymax: (ymax * 1.08).max(1.0),
    };
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {h}\" role=\"img\" aria-label=\"Chromatogram {}\">",
        escape(&p.file)
    );
    axes(
        &mut s,
        &f,
        "Retention time (min)",
        "Intensity",
        &ticks(f.xmin, f.xmax, 5),
        &ticks(f.ymin, f.ymax, 3),
    );
    if pts.len() > 1 {
        if p.has_peak {
            let (ys, ye) = (interp(&p.trace, p.start), interp(&p.trace, p.end));
            let mut poly = format!("{:.1},{:.1} ", f.px(p.start), f.py(ys));
            for t in pts.iter().filter(|t| t[0] > p.start && t[0] < p.end) {
                let _ = write!(poly, "{:.1},{:.1} ", f.px(t[0]), f.py(t[1]));
            }
            let _ = write!(poly, "{:.1},{:.1}", f.px(p.end), f.py(ye));
            let _ = write!(
                s,
                "<polygon points=\"{poly}\" fill=\"#176b87\" fill-opacity=\"0.25\"/>\
                 <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#6b7785\" stroke-dasharray=\"4 3\"/>\
                 <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#2c9a5f\" stroke-dasharray=\"2 3\"/>\
                 <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#d98b1f\" stroke-dasharray=\"2 3\"/>",
                f.px(p.start),
                f.py(ys),
                f.px(p.end),
                f.py(ye),
                f.px(p.start),
                f.y0,
                f.px(p.start),
                f.y0 + f.h,
                f.px(p.end),
                f.y0,
                f.px(p.end),
                f.y0 + f.h
            );
        }
        let path = pts
            .iter()
            .enumerate()
            .map(|(i, t)| {
                format!(
                    "{}{:.1},{:.1}",
                    if i == 0 { "M" } else { "L" },
                    f.px(t[0]),
                    f.py(t[1])
                )
            })
            .collect::<String>();
        let _ = write!(
            s,
            "<path d=\"{path}\" fill=\"none\" stroke=\"#176b87\" stroke-width=\"1.3\"/>"
        );
    }
    s.push_str("</svg>");
    s
}

// ---------- calibration text ----------

fn curve_label(cal: &Calibration) -> &'static str {
    match cal.curve {
        Curve::Polynomial => match cal.coefficients.len() {
            2 => "Linear",
            3 => "Quadratic",
            _ => "Cubic",
        },
        Curve::LogLog => "Log-log",
        Curve::PointToPoint => "Point-to-point",
    }
}
fn equation(cal: &Calibration) -> String {
    match cal.curve {
        Curve::Polynomial => {
            let mut out = String::from("y = ");
            let mut first = true;
            for (j, c) in cal.coefficients.iter().enumerate() {
                let a = c / cal.scale.powi(j as i32);
                if a == 0.0 {
                    continue;
                }
                let sign = if a < 0.0 { "−" } else { "+" };
                if first {
                    if a < 0.0 {
                        out.push('−');
                    }
                } else {
                    let _ = write!(out, " {sign} ");
                }
                first = false;
                let mag = sig(a.abs());
                match j {
                    0 => out.push_str(&mag),
                    1 => {
                        let _ = write!(out, "{mag}·x");
                    }
                    _ => {
                        let sup = match j {
                            2 => "²".to_string(),
                            3 => "³".to_string(),
                            n => format!("^{n}"),
                        };
                        let _ = write!(out, "{mag}·x{sup}");
                    }
                }
            }
            out
        }
        Curve::LogLog => {
            let b = cal.coefficients.get(1).copied().unwrap_or(0.0);
            let a = cal.coefficients.first().copied().unwrap_or(0.0);
            format!("ln y = {} + {}·ln(x / {})", sig(a), sig(b), sig(cal.scale))
        }
        Curve::PointToPoint => "straight segments between calibration levels".into(),
    }
}

// ---------- sections ----------

struct Ctx<'a> {
    batch: &'a BatchResult,
    results: Vec<Concentration>,
    peaks: Vec<Peak>,
    qc: Option<qc::Report>,
}
impl Ctx<'_> {
    fn analyte_name(t: &Target) -> String {
        t.quantifier.extraction.extraction.name.clone()
    }
    fn peak_for(&self, file: &str, target: &Target) -> Option<&Peak> {
        let name = Self::analyte_name(target);
        let single = self
            .batch
            .request
            .targets
            .iter()
            .filter(|t| t.calibration.is_some())
            .count()
            == 1;
        self.peaks
            .iter()
            .find(|p| p.file == file && (p.analyte == name || single))
    }
    fn targets(&self) -> impl Iterator<Item = &Target> {
        self.batch
            .request
            .targets
            .iter()
            .filter(|t| t.calibration.is_some())
    }
}

fn summary(h: &mut String, c: &Ctx) {
    let samples = &c.batch.request.samples;
    let count = |r: Role| samples.iter().filter(|s| s.role == r).count();
    let (verdict, class) = match c.qc.as_ref().map(|r| &r.status) {
        Some(qc::Status::Pass) => ("Pass", "good"),
        Some(qc::Status::Fail) => ("Fail", "bad"),
        Some(qc::Status::Indeterminate) => ("Indeterminate", "warn"),
        None => ("Not evaluated", ""),
    };
    let peaks_total = c.peaks.len();
    let peaks_done = c.peaks.iter().filter(|p| reviewed(&p.status)).count();
    h.push_str("<div class=\"cards\">");
    let _ = write!(
        h,
        "<div class=\"card\"><div class=\"k\">Samples</div><div class=\"v\">{}</div><div class=\"s\">{} standards · {} QC · {} unknown · {} blank</div></div>",
        samples.len(),
        count(Role::Standard),
        count(Role::Qc),
        count(Role::Unknown),
        count(Role::Blank)
    );
    let _ = write!(
        h,
        "<div class=\"card\"><div class=\"k\">Batch QC</div><div class=\"v\"><span class=\"pill {class}\">{verdict}</span></div><div class=\"s\">{}</div></div>",
        escape(c.qc.as_ref().map_or("QC rules have not been evaluated", |r| r.acceptance.reason.as_str()))
    );
    for t in c.targets() {
        let (cfg, cal) = (
            t.calibration.as_ref().unwrap(),
            c.batch.calibrations.get(&t.id),
        );
        let (value, sub) = match cal {
            Some(cal) => (
                cal.r_squared.map_or("–".into(), |r| format!("R² {r:.4}")),
                format!("{}, weighting {}", curve_label(cal), cfg.weighting.label()),
            ),
            None => ("Not fitted".into(), "see calibration section".into()),
        };
        let _ = write!(
            h,
            "<div class=\"card\"><div class=\"k\">Calibration · {}</div><div class=\"v\">{}</div><div class=\"s\">{}</div></div>",
            escape(&Ctx::analyte_name(t)),
            escape(&value),
            escape(&sub)
        );
    }
    let _ = write!(
        h,
        "<div class=\"card\"><div class=\"k\">Peaks reviewed</div><div class=\"v\">{peaks_done} of {peaks_total}</div><div class=\"s\">integrations checked by a person</div></div></div>"
    );

    // Everything that needs a person to look at it.
    let mut items: Vec<String> = vec![];
    if let Some(r) = &c.qc {
        for d in r.decisions.iter().filter(|d| d.status != qc::Status::Pass) {
            items.push(format!(
                "<b>QC</b> · {}: {:?} — {}",
                escape(&rule_title(&d.rule)),
                d.status,
                escape(&d.reason)
            ));
        }
    }
    let name_of = |id: &str| {
        c.batch
            .request
            .targets
            .iter()
            .find(|t| t.id == id)
            .map_or(id.to_string(), Ctx::analyte_name)
    };
    for (target, e) in &c.batch.calibration_errors {
        items.push(format!(
            "<b>Calibration</b> · {}: {}",
            escape(&name_of(target)),
            escape(&e.message)
        ));
    }
    let cal_flags: BTreeSet<String> = c
        .batch
        .calibrations
        .values()
        .flat_map(|cal| cal.flags.iter().cloned())
        .collect();
    for (target, cal) in &c.batch.calibrations {
        for f in &cal.flags {
            items.push(format!(
                "<b>Calibration</b> · {}: {}",
                escape(&name_of(target)),
                escape(&words(f))
            ));
        }
    }
    for t in c.targets() {
        for s in &c.batch.request.samples {
            let Some(r) = c
                .results
                .iter()
                .find(|r| r.sample == s.id && r.target == t.id)
            else {
                continue;
            };
            let flags: Vec<String> = r
                .flags
                .iter()
                .filter(|f| !cal_flags.contains(*f))
                .map(|f| words(f))
                .collect();
            if state_is_problem(&r.state) || !flags.is_empty() {
                let mut what = vec![];
                if state_is_problem(&r.state) {
                    what.push(state_label(&r.state).to_string());
                }
                what.extend(flags);
                items.push(format!(
                    "<b>{}</b> · {} ({}): {}",
                    escape(&s.id),
                    escape(&Ctx::analyte_name(t)),
                    role_label(&s.role),
                    escape(&what.join("; "))
                ));
            }
        }
    }
    let unreviewed: Vec<&Peak> = c
        .peaks
        .iter()
        .filter(|p| !reviewed(&p.status) && (!p.has_peak || p.tier != "high"))
        .collect();
    for p in unreviewed.iter().filter(|p| !p.has_peak) {
        items.push(format!(
            "<b>{}</b> · {}: no peak found",
            escape(&p.file),
            escape(&p.analyte)
        ));
    }
    let with_peak: Vec<String> = unreviewed
        .iter()
        .filter(|p| p.has_peak)
        .map(|p| escape(&p.file))
        .collect();
    if !with_peak.is_empty() {
        items.push(format!(
            "<b>{} peak(s) not yet reviewed</b> (automatic integration, below high confidence): {}",
            with_peak.len(),
            with_peak.join(", ")
        ));
    }
    h.push_str("<h3>Needs attention</h3>");
    if items.is_empty() {
        h.push_str("<p><span class=\"pill good\">Nothing flagged</span> All peaks are reviewed, the calibration is clean and every result is quantified.</p>");
    } else {
        let _ = write!(
            h,
            "<p class=\"dim\">{} item(s) to check before relying on these results.</p><ul class=\"attn\">",
            items.len()
        );
        for i in items {
            let _ = write!(h, "<li>{i}</li>");
        }
        h.push_str("</ul>");
    }
}

fn method_section(h: &mut String, c: &Ctx, value: &Value) {
    h.push_str("<h2>Method and data</h2>");
    let r = &c.batch.request;
    h.push_str("<table><tr><th>Analyte</th><th class=\"n\">m/z</th><th class=\"n\">Expected RT (min)</th><th>Curve</th><th>Range</th><th class=\"n\">LOD</th><th class=\"n\">LOQ</th><th class=\"n\">Accuracy tolerance</th></tr>");
    for t in c.targets() {
        let (a, cfg) = (&t.quantifier.extraction, t.calibration.as_ref().unwrap());
        let _ = write!(
            h,
            "<tr><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td>{}, {}, {}</td><td>{} – {} {}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">±{}%</td></tr>",
            escape(&Ctx::analyte_name(t)),
            a.extraction.mass.map_or("–".into(), |m| format!("{m:.4}")),
            sig(a.expected_rt),
            match cfg.curve {
                Curve::Polynomial => match cfg.degree {
                    1 => "linear",
                    2 => "quadratic",
                    _ => "cubic",
                },
                Curve::LogLog => "log-log",
                Curve::PointToPoint => "point-to-point",
            },
            escape(cfg.weighting.label()),
            match cfg.intercept {
                crate::targeted::Intercept::Free => "free intercept",
                crate::targeted::Intercept::Zero => "zero intercept",
                crate::targeted::Intercept::Fixed(_) => "fixed intercept",
            },
            sig(cfg.range[0]),
            sig(cfg.range[1]),
            cfg.unit.label(),
            sig(cfg.lod),
            sig(cfg.loq),
            sig(cfg.accuracy_tolerance_percent)
        );
    }
    h.push_str("</table>");
    let _ = write!(
        h,
        "<p class=\"small dim\">Integration: smoothing {}, minimum height {}, boundary fraction {}. Batch “{}” created {}. {} raw files.</p>",
        r.detection_smoothing,
        sig(r.minimum_height),
        sig(r.boundary_fraction),
        escape(&r.name),
        date_text(c.batch.created_unix_ms),
        r.samples.len()
    );
    let _ = write!(
        h,
        "<p class=\"small dim\">Software: Chromascope {} ({}). Project {} revision {}.</p>",
        env!("CARGO_PKG_VERSION"),
        escape(&c.batch.kernel_version),
        escape(value["project"]["id"].as_str().unwrap_or("–")),
        value["project"]["revision"].as_u64().unwrap_or(0)
    );
}

fn calibration_section(h: &mut String, c: &Ctx) {
    h.push_str("<h2 class=\"pb\">Calibration</h2>");
    for t in c.targets() {
        let cfg = t.calibration.as_ref().unwrap();
        let name = Ctx::analyte_name(t);
        let _ = write!(h, "<h3>{}</h3>", escape(&name));
        if let Some(e) = c.batch.calibration_errors.get(&t.id) {
            let _ = write!(
                h,
                "<div class=\"note bad\"><b>Calibration could not be fitted.</b> {}</div>",
                escape(&e.message)
            );
            continue;
        }
        let Some(cal) = c.batch.calibrations.get(&t.id) else {
            h.push_str("<p class=\"dim\">No calibration was produced for this analyte.</p>");
            continue;
        };
        let unit = cfg.unit.label();
        let others: Vec<(f64, f64)> = c
            .results
            .iter()
            .filter(|r| r.target == t.id)
            .filter(|r| {
                c.batch
                    .request
                    .samples
                    .iter()
                    .any(|s| s.id == r.sample && matches!(s.role, Role::Qc | Role::Unknown))
            })
            .filter_map(|r| Some((r.injected_concentration?, r.response?)))
            .collect();
        let _ = write!(
            h,
            "<p>{} fit, weighting {}. <span class=\"eq\">{}</span> &nbsp; R² = {}</p>",
            curve_label(cal),
            escape(cfg.weighting.label()),
            escape(&equation(cal)),
            cal.r_squared.map_or("–".into(), |r| format!("{r:.5}"))
        );
        for f in &cal.flags {
            let _ = write!(
                h,
                "<div class=\"note\"><b>Calibration note:</b> {}</div>",
                escape(&words(f))
            );
        }
        h.push_str("<div class=\"plots\">");
        h.push_str(&calibration_svg(
            cal,
            unit,
            cfg.accuracy_tolerance_percent,
            &others,
        ));
        h.push_str("<div style=\"flex:1;min-width:300px\">");
        let _ = write!(
            h,
            "<table><tr><th>Standard</th><th class=\"n\">Nominal ({unit})</th><th class=\"n\">Response</th><th class=\"n\">Back-calc. ({unit})</th><th class=\"n\">% diff</th><th>Note</th></tr>"
        );
        let mut points: Vec<_> = cal.points.iter().collect();
        points.sort_by(|a, b| a.x.total_cmp(&b.x));
        for p in points {
            let diff = p.accuracy_percent.map(|a| a - 100.0);
            let bad = diff.is_some_and(|d| d.abs() > cfg.accuracy_tolerance_percent);
            let _ = write!(
                h,
                "<tr class=\"{}\"><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td>{}</td></tr>",
                if bad { "bad" } else { "" },
                escape(&p.sample),
                sig(p.x),
                area_text(p.response),
                p.back_calculated.map_or("–".into(), sig),
                diff.map_or("–".into(), pct),
                escape(&p.flags.iter().map(|f| words(f)).collect::<Vec<_>>().join("; "))
            );
        }
        h.push_str("</table>");
        let _ = write!(
            h,
            "<p class=\"small dim\">Acceptance: back-calculated standards within ±{}% of nominal. Red rows are outside it.</p></div></div>",
            sig(cfg.accuracy_tolerance_percent)
        );
    }
}

fn concentration_section(h: &mut String, c: &Ctx) {
    h.push_str("<h2 class=\"pb\">Concentrations</h2>");
    let mut samples: Vec<_> = c.batch.request.samples.iter().collect();
    samples.sort_by_key(|s| s.injection_order);
    for t in c.targets() {
        let cal_flags: BTreeSet<&String> = c
            .batch
            .calibrations
            .get(&t.id)
            .map(|cal| cal.flags.iter().collect())
            .unwrap_or_default();
        let unit = t.calibration.as_ref().unwrap().unit.label();
        if c.targets().count() > 1 {
            let _ = write!(h, "<h3>{}</h3>", escape(&Ctx::analyte_name(t)));
        }
        let _ = write!(
            h,
            "<table><tr><th class=\"n\">#</th><th>Sample</th><th>File</th><th>Type</th><th class=\"n\">Response</th><th class=\"n\">Nominal</th><th class=\"n\">Concentration ({unit})</th><th class=\"n\">% of nominal</th><th>Peak</th><th>Flags</th></tr>"
        );
        for s in samples.iter() {
            let r = c
                .results
                .iter()
                .find(|r| r.sample == s.id && r.target == t.id);
            let file = base(&s.source);
            let peak = c.peak_for(&file, t);
            let (conc, bad) = match r {
                Some(r) => match &r.state {
                    State::Present => (r.concentration.map_or("–".into(), sig), false),
                    State::BelowLoq | State::BelowDetection => (
                        format!(
                            "{} <span class=\"dim\">({})</span>",
                            r.injected_concentration
                                .map_or("–".into(), |x| sig(x * r.dilution)),
                            state_label(&r.state)
                        ),
                        false,
                    ),
                    other => (
                        format!("<span class=\"pill bad\">{}</span>", state_label(other)),
                        true,
                    ),
                },
                None => ("–".into(), false),
            };
            let flags = r
                .map(|r| {
                    r.flags
                        .iter()
                        .filter(|f| !cal_flags.contains(f))
                        .map(|f| words(f))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let peak_cell = match peak {
                Some(p) if p.has_peak => format!(
                    "<span class=\"pill {}\">{} {}</span> {}",
                    tier_class(&p.tier),
                    capitalize(&p.tier),
                    p.score,
                    if reviewed(&p.status) { "✓" } else { "" }
                ),
                Some(_) => "<span class=\"pill bad\">no peak</span>".into(),
                None => "–".into(),
            };
            let _ = write!(
                h,
                "<tr class=\"{}\"><td class=\"n\">{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td class=\"n\">{}</td><td>{}</td><td>{}</td></tr>",
                if bad {
                    "bad"
                } else if !flags.is_empty() {
                    "warn"
                } else {
                    ""
                },
                s.injection_order,
                escape(&s.id),
                escape(&file),
                role_label(&s.role),
                r.and_then(|r| r.response).map_or("–".into(), area_text),
                s.nominal.get(&t.id).map_or("–".into(), |n| sig(*n)),
                conc,
                r.and_then(|r| r.accuracy_percent)
                    .map_or("–".into(), |a| format!("{a:.1}%")),
                peak_cell,
                escape(&flags.join("; "))
            );
        }
        h.push_str("</table>");
    }
    h.push_str("<p class=\"small dim\">Concentrations include the sample dilution factor. “Peak” is the automatic integration confidence score (✓ = reviewed by a person). Values below the LOQ are shown for information and are not reportable.</p>");
}

fn rule_title(rule: &qc::Rule) -> String {
    let who = match rule.role.as_deref() {
        Some("standard") => "Standards",
        Some("qc") => "QC samples",
        Some("blank") => "Blanks",
        Some("unknown") => "Unknowns",
        _ => "All samples",
    };
    format!("{who}: {}", camel_words(&format!("{:?}", rule.metric)))
}
fn qc_section(h: &mut String, c: &Ctx) {
    h.push_str("<h2>Batch QC</h2>");
    let Some(r) = &c.qc else {
        h.push_str("<p class=\"dim\">Batch QC was not evaluated for this project.</p>");
        return;
    };
    let class = match r.status {
        qc::Status::Pass => "good",
        qc::Status::Fail => "bad",
        qc::Status::Indeterminate => "warn",
    };
    let _ = write!(
        h,
        "<p><span class=\"pill {class}\">{:?}</span> {}</p><table><tr><th>Check</th><th>Target</th><th>Result</th><th class=\"n\">Value</th><th>Limit</th><th class=\"n\">n</th><th>Note</th></tr>",
        r.status,
        escape(&r.acceptance.reason)
    );
    for d in &r.decisions {
        let limit = match d.rule.metric {
            qc::Metric::MaximumAccuracyDeviation => format!("± {}%", sig(d.rule.upper)),
            _ if d.rule.lower == -f64::MAX => format!("≤ {}", sig(d.rule.upper)),
            _ => format!("{} – {}", sig(d.rule.lower), sig(d.rule.upper)),
        };
        let class = match d.status {
            qc::Status::Pass => "good",
            qc::Status::Fail => "bad",
            qc::Status::Indeterminate => "warn",
        };
        let _ = write!(
            h,
            "<tr><td>{}</td><td>{}</td><td><span class=\"pill {class}\">{:?}</span></td><td class=\"n\">{} {}</td><td>{}</td><td class=\"n\">{}</td><td>{}</td></tr>",
            escape(&rule_title(&d.rule)),
            escape(&d.rule.target),
            d.status,
            d.value.map_or("–".into(), sig),
            escape(&d.unit),
            escape(&limit),
            d.n,
            escape(&d.reason)
        );
    }
    h.push_str("</table>");
    if !r.reviews.is_empty() {
        h.push_str("<h3>Review notes</h3><ul class=\"attn\">");
        for e in &r.reviews {
            let _ = write!(
                h,
                "<li>{} — {} ({})</li>",
                escape(&e.actor),
                escape(&e.reason),
                escape(&e.rule_id)
            );
        }
        h.push_str("</ul>");
    }
}

fn peak_section(h: &mut String, c: &Ctx) {
    h.push_str("<h2 class=\"pb\">Peaks</h2><p class=\"dim\">Integrated area is shaded; dashed lines mark the integration start, end and baseline. Orange-framed peaks are unreviewed or flagged.</p>");
    for t in c.targets() {
        if c.targets().count() > 1 {
            let _ = write!(h, "<h3>{}</h3>", escape(&Ctx::analyte_name(t)));
        }
        h.push_str("<div class=\"peaks\">");
        let mut samples: Vec<_> = c.batch.request.samples.iter().collect();
        samples.sort_by_key(|s| s.injection_order);
        for s in samples {
            let file = base(&s.source);
            let Some(p) = c.peak_for(&file, t) else {
                continue;
            };
            let class = if !p.has_peak {
                "bad"
            } else if !reviewed(&p.status) && p.tier != "high" || !p.flags.is_empty() {
                "flag"
            } else {
                ""
            };
            let _ = write!(
                h,
                "<figure class=\"peak {class}\">{}<figcaption><b>{}</b> · {} · {}<br>",
                peak_svg(p),
                escape(&s.id),
                escape(&file),
                role_label(&s.role)
            );
            if p.has_peak {
                let _ = write!(
                    h,
                    "RT {:.2} min · area {} · <span class=\"pill {}\">{} {}</span> <span class=\"pill\">{}</span>",
                    p.apex,
                    area_text(p.area),
                    tier_class(&p.tier),
                    capitalize(&p.tier),
                    p.score,
                    if reviewed(&p.status) { "Reviewed" } else { "Not reviewed" }
                );
                if !p.flags.is_empty() {
                    let _ = write!(
                        h,
                        "<br><span class=\"dim\">{}</span>",
                        escape(&p.flags.join("; "))
                    );
                }
            } else {
                h.push_str("<span class=\"pill bad\">No peak detected</span>");
            }
            h.push_str("</figcaption></figure>");
        }
        h.push_str("</div>");
    }
}

/// The report body for a project, or `None` when it holds no targeted quantification.
pub(crate) fn body(sections: &[String], value: &Value) -> Option<String> {
    let review = value.get("review").filter(|r| r.is_object())?;
    let batch: BatchResult = serde_json::from_value(review["batch"].clone()).ok()?;
    let mut results = batch.results.clone();
    for rev in &batch.reviews {
        if let Some(row) = results
            .iter_mut()
            .find(|r| r.sample == rev.sample && r.target == rev.target)
        {
            if !rev.accepted {
                row.state = State::Rejected;
            }
            row.reviewed = rev.accepted;
        }
    }
    let ctx = Ctx {
        batch: &batch,
        results,
        peaks: peaks_of(review),
        qc: review
            .get("qc")
            .and_then(|q| serde_json::from_value(q.clone()).ok()),
    };
    let has = |s: &str| sections.iter().any(|x| x == s);
    let mut h = String::new();
    h.push_str("<h2>Summary</h2>");
    summary(&mut h, &ctx);
    if has("methods") || has("metadata") {
        method_section(&mut h, &ctx, value);
    }
    if has("calibration") {
        calibration_section(&mut h, &ctx);
    }
    if has("quantification") {
        concentration_section(&mut h, &ctx);
    }
    if has("qc") {
        qc_section(&mut h, &ctx);
    }
    if has("chromatograms") {
        peak_section(&mut h, &ctx);
    }
    Some(h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as chromascope;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/targeted_support.rs"
    ));
    #[test]
    fn formatting_helpers() {
        assert_eq!(sig(0.0), "0");
        assert_eq!(sig(1.9360), "1.936");
        assert_eq!(sig(123456.0), "1.235e5");
        assert_eq!(date_text(0), "1970-01-01 00:00 UTC");
        assert_eq!(date_text(1_760_000_000_000), "2025-10-09 08:53 UTC");
        assert_eq!(base("C:\\a\\b\\x.raw"), "x.raw");
        assert_eq!(ticks(0.0, 64.0, 6), vec![0.0, 20.0, 40.0, 60.0]);
    }
    #[test]
    fn report_has_real_tables_and_figures() {
        let dir = tempfile::tempdir().unwrap();
        let batch =
            crate::targeted::run(raw_request(dir.path()), &crate::jobs::JobControl::default())
                .unwrap();
        let value = serde_json::json!({
            "review": {"batch": batch, "qc": null, "peaks": []},
            "project": {"id": "p", "revision": 1}
        });
        let sections: Vec<String> = crate::delivery::visible_sections()
            .map(Into::into)
            .collect();
        let html = body(&sections, &value).expect("report body");
        for needle in [
            "Summary",
            "Calibration",
            "Concentrations",
            "<table",
            "<svg",
            "Batch QC",
        ] {
            assert!(html.contains(needle), "missing {needle}");
        }
        assert!(!html.contains("&quot;"), "no raw JSON in a human report");
    }
    #[test]
    fn no_review_means_no_body() {
        assert!(body(&[], &serde_json::json!({})).is_none());
    }
}
