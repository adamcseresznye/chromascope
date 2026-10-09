# Chromascope usability and visual-quality review

Date: 2026-10-09 (America/Los_Angeles). Package: 0.3.0. Scope: independent
real-world evaluation of the desktop workbench as built from this working tree,
plus directly implemented refinements. No analytical algorithms were changed.
This report does not certify an assay, instrument, or regulatory submission.

Prior baseline: [UX_UI_AUDIT.md](UX_UI_AUDIT.md) (UX01–UX18 observations) and
[UX_UI_REMEDIATION.md](UX_UI_REMEDIATION.md) (implemented UX01–UX18 matrix).
This review challenges that baseline rather than re-confirming it.

## 1. How the application was evaluated

- Built the current desktop (`cargo build --locked --bin chromascope`, debug)
  and inspected every major workspace through native-equivalent evidence:
  existing native captures in `docs/ux-evidence/verified-*-{dark,light}-{large,small}.jpg`
  (Windows eframe, `test_file/data_dependent_02.mzML`, ~920×620 and large
  clients, both themes) plus fresh software-rendered egui frames produced by
  the real layout code in this tree (`gui/workbench.rs`,
  `gui/panels.rs`) with synthetic chromatograms.
- New renders in this review (software-rendered egui fixtures, not native
  screenshots — the distinction is kept explicit throughout):
  - `ux-evidence/usability-review-data-light-1280.png` — Data explorer, light, 1280 px.
  - `ux-evidence/usability-review-data-dark-920.png` — Data explorer, dark, 920 px.
  - `ux-evidence/usability-review-xic-light-1280.png` — XIC controls, light, 1280 px.
- Before images: the `verified-*.jpg` native captures were taken against a
  slightly older build (separate project buttons row, short nav labels
  `Quant/QC`, `Untargeted`, `AI review`; `Quantification and batch QC`
  heading; `AI / MCP activity` title). They remain valid evidence of the
  problems fixed below because the surrounding layout is unchanged.
- After images: the three new renders above, produced after the changes in
  section 5, plus the passing layout-test assertions that pin the new behavior.
- Source walkthrough of `gui/workbench.rs`, `panels.rs`, `quant.rs`,
  `spectral.rs`, `delivery.rs`, `preset_editor.rs` for control order,
  terminology, empty states, and disabled-button feedback.
- Full verification: `cargo fmt --all -- --check`, `cargo clippy --locked
  --all-targets --all-features -- -D warnings`, `cargo check --locked
  --all-features`, `cargo test --locked --all-features` (see section 6).
  Native end-to-end interaction (typing into a live window, file dialogs,
  OpenMS runs, MCP client sessions, DPI/resize dragging) was not repeated in
  this pass; the prior native evidence is reused and its limits are stated.

## 2. Overall UX assessment

Chromascope is a genuinely capable workbench, but it still reads as several
strong tools sharing a window rather than one guided instrument workbench.
The strongest parts are the Data explorer (fast trace extraction, honest
raw/processed overlay, full-resolution integration) and the evidence-retention
model (revisions, review history, explicit missing states). The weakest parts
are first-run guidance and cross-workspace coherence:

- A new analyst can open an mzML file and see a TIC within seconds (good),
  but the path from "I see a peak" to "I quantified it with calibration and
  exported a report" is undiscoverable without reading the user guide. The
  critical actions (Apply XIC, Run batch, stage 2 calibration, Export report)
  are visually equal to every secondary control.
- Three persistence models compete for attention: viewer session, batch
  `.chromquant` files, and engine projects. The project menu consolidation
  (single `Project` menu instead of four buttons) is a real improvement over
  the captured baseline, but nothing in the Quantification workspace tells the
  user which model they are in.
- Terminology was inconsistent in three places (`Quant/QC` vs
  `Quantification and batch QC`; `AI review` vs `AI / MCP activity` vs
  `AI activity`; `Batch Quantification` in preset hints). Fixed in this pass
  (section 5.1).
- Empty states are honest (no invented data) but repetitive and passive: the
  Quantification workspace previously showed two near-identical guidance
  labels stacked vertically above a large empty panel. Fixed in this pass
  (section 5.3).

## 3. Visual design assessment

The Inter typeface, 14/12 px scale, 8 px spacing, 28 px controls, and quiet
instrument palette (dark panel #272E37, dark plot #20252C, light panel
#F3F5F8) are appropriate and were retained. Contrast tests (≥4.5 for text and
scientific palette roles) pass in both themes. Against that foundation:

- Hierarchy is flat. Headings, body labels, and helper text share color and
  near-identical weight; primary actions (`Run batch`, `Apply XIC`, `Export
  report and project`) previously used the same button style as `Cancel` and
  `Select none`. This pass gives the three primary actions an accent fill and
  strong label.
- The header consumed four stacked rows in the baseline captures (brand +
  menus, project buttons, workspace tabs, trace toolbar). The current tree
  already compresses this to one wrapped menu/navigation row plus one trace
  toolbar row; this pass keeps that compression while restoring full,
  unambiguous workspace names. Cost: at 920 px the navigation wraps to two
  rows (~under 118 px total chrome, asserted by test) instead of one. That is
  the correct trade — clarity over 30 px.
- Plot-to-chrome ratio is acceptable at 1280 px (see new light render) but
  tight at 920×620 in dark mode: in-plot control rows (Reset/Zoom/Pan/Legend/
  Copy) wrap onto the axes and the legend overlaps the peak (see new dark
  render). The existing `wrapped_plot_controls_leave_axes_inside_card_height`
  regression still passes (axes stay inside the card, canvas ≥100 px), so this
  is recorded as a remaining limitation, not a regression.
- Tables, dialogs, and status bar are consistent. The status bar now also
  reports project context (`Project rN` or `Exploratory · no project`),
  addressing the "which model am I in?" problem at zero layout cost.
- Truncation is handled correctly everywhere checked (dataset names, scan
  filter, figure names) with full paths on hover — except Identification,
  which painted a raw `\\?\`-prefixed UNC path as a full-width label. Fixed
  in this pass (file name + hover).

## 4. Workflow-specific findings

| # | Workflow (first-time lens) | Finding |
|---|---|---|
| 1 | Open an mzML file | Easy: `File > Open data…` or the empty-state `Open data…` button. Vendor folder path (`Open dataset folder…`) is two clicks away and conversion feedback exists. No change needed. |
| 2 | Display a TIC / XIC | TIC is one click. XIC was the worst discoverability gap: target/tolerance fields live in the right inspector, and `Apply XIC` looked like plain text among ten equal controls with no explanation of ± semantics. Fixed: half-width explanation, live `Current extraction` summary, accent-filled `Apply XIC`. |
| 3 | Inspect a mass spectrum | Double-click works natively (prior evidence), and numeric `Spectrum at RT (min)` + `Inspect nearest acquired scan` is the keyboard alternative. It was buried below Appearance/Traces. Fixed: `Selection` section moved above `Appearance` with a one-line next-step guide. |
| 4 | Smooth a chromatogram | Slider present; `Off · original trace` and window-size explanation retained. Unsmoothed overlay defaults on. No change needed. |
| 5 | Integrate a peak | Right-drag works but is undiscoverable; the typed boundary form was inside a collapsed `Enter integration boundaries` below the fold. Still collapsed by default (progressive disclosure is correct), but the `Selection` move puts it one viewport closer, and the section header now states the batch handoff. |
| 6 | Review/correct boundaries | Quantification review plot + Start/End drag values + Apply/Reset/Accept are sound and scientifically transparent (unsmoothed data, chord baseline). Two gaps fixed: no unresolved count on the stage tab (added `N unresolved`), and duplicate empty guidance (deduplicated). |
| 7 | Configure/run targeted quantification | The `Run batch` button was disabled with no visible reason (disabled-button tooltips do not reliably show). Fixed: an inline reason line (`Select at least one loaded sample…` / `Waiting…` / `Define a valid extraction method…`). |
| 8 | Inspect a calibration curve | Reachable only via stage `2 · Calibration and concentrations` — now numbered so the sequence reads. Curve labels/axes were fixed in the prior remediation; no new issue found. |
| 9 | Evaluate QC results | Stage `3 · Batch QC and validation` now numbered; unresolved-peak count surfaces on stage 1. The prior `Next unresolved` control is retained. No new issue found. |
| 10 | Export results | Three exits remain (viewer CSV/SVG, batch CSV, Reports export) with no single "finish" affordance. Reduced but not solved: Reports now validates inline (existing-output warning, disabled Export until project + new folder are set) and the Export button is primary-styled. Unifying the three exits into one review-and-release flow is the largest remaining workflow gap. |

## 5. Improvements implemented (this pass, presentation-only)

No extraction, smoothing, baseline, fit, detection, alignment, statistical,
annotation, or persistence algorithm was changed. All edits are in
`src/gui/` plus two test stabilizations.

### 5.1 One consistent workspace language

- `workbench.rs` navigation: `Quant/QC` → `Quantification and QC`,
  `Untargeted` → `Untargeted analysis`, `AI review` → `AI activity`.
- `workbench.rs` activity window title: `AI / MCP activity` → `AI activity`.
- `quant.rs` heading: `Quantification and batch QC` → `Quantification and QC`;
  breadcrumb numbered (`1 Select samples → 2 define … → 5 export`).
- `quant.rs` stages numbered: `1 · Areas and peak review`,
  `2 · Calibration and concentrations`, `3 · Batch QC and validation`.
- `spectral.rs` stages numbered `1–4`; `preset_editor.rs` hints updated from
  `Batch Quantification` to `Quantification and QC`.
- `panels.rs` compact-header test updated to the full names; height bound
  relaxed 76 → 118 px to allow the two-row wrapped navigation at 920 px.

### 5.2 Primary actions look primary

- `Apply XIC` (`workbench.rs`), `Run batch` (already filled, now paired with
  an inline enable reason in `quant.rs`), and `Export report and project`
  (`delivery.rs`, also gated on non-empty project/destination plus
  must-not-exist validation) use accent fill + strong labels.

### 5.3 Quantification hierarchy and feedback

- Stage 1 tab carries a live unresolved count
  (`1 · Areas and peak review (N unresolved)`, counting Missing / Ambiguous /
  Failed / Cancelled / Pending and stale-method results).
- Inline blocked reason under the batch toolbar instead of a hover-only
  tooltip on a disabled button.
- Empty state rewritten as a single numbered guide; the duplicate
  `Select a result…` message in `review()` is suppressed when there are no
  results at all.

### 5.4 Inspector ordered by frequency

- Right `Trace settings` order is now Acquisition → scan filter → XIC/range →
  Smoothing → Traces → Measurements → Selection → Appearance (was: …
  Smoothing → Appearance → Traces → Selection). `Selection` gained a one-line
  orientation (`Inspect a spectrum, then integrate a peak. Batch review happens
  in Quantification and QC.`).

### 5.5 XIC semantics stated where the values are entered

- `Tolerance is a ± half-width around the target m/z…` plus a live
  `Current extraction: 483.0000 m/z ± 10.0 ppm` summary (addresses audit item
  C on tolerance explanation without touching validation).

### 5.6 Path displays and Reports hardening

- `spectral.rs`: `Active source: \\?\C:\…` full-width label replaced by file
  name + full-path hover, with an explicit empty hint pointing back to the
  Data explorer.
- `delivery.rs`: destination field has an example hint and an
  already-exists warning; report title has a hint; Export is disabled until
  project + new-folder preconditions hold.

### 5.7 Status bar context

- `workbench.rs` `status()`: appends `Project rN` or
  `Exploratory · no project`, so the persistence model is always visible.

### 5.8 Test adjustments (no assertions weakened)

- `panels.rs` compact-header test: label assertions run from frame 1, because
  the wrapped full-name navigation needs one egui layout pass to stabilize at
  920 px (proven by a temporary shape-dump: frame 0 lacks `Reports`/`AI
  activity`; frames 1–2 contain all ten controls at both widths and themes).
- `workbench.rs` frame test: menu-click interaction armed on frame 1 instead
  of frame 0 for the same stabilization reason (`Plot options` at 920 px).

## 6. Verification results

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | Pass, no warnings |
| `cargo check --locked --all-features` | Pass |
| `cargo build --locked --bin chromascope` (default target) | Pass |
| `cargo test --locked --all-features` (redirected target dir; `-j 2` — see limits) | **382 passed, 0 failed, 5 ignored** across lib (235), integration suites, bins, and doc-tests (6). Ignored tests require external runtimes (ProteoWizard/vendor, pyOpenMS 3.5.0, opt-in performance) and were not treated as passes. |
| `cargo test --locked --lib` GUI/render subset | 222/222 (default features); layout, focus-scroll, contrast, navigation-modifier, and plot-budget regressions all pass |
| `git diff --check` | Pass |
| Fresh renders | `usability-review-data-light-1280.png`, `usability-review-data-dark-920.png`, `usability-review-xic-light-1280.png` generated by the real layout tests post-change and visually inspected (section 3 notes) |

Environment limits encountered honestly: the default `target/` lives on a
OneDrive-synced path that ran out of quota during test-binary linking
(`os error 112`), and unbounded parallel linking exhausted the page file
(`os error 1455`). Verification therefore ran with
`CARGO_TARGET_DIR` in a local temp directory and `-j 2`. Results are
identical code and lockfile; only artifact placement and job count differ.

## 7. Before / after

- Before (baseline behavior, native captures): `ux-evidence/verified-data-dark-large.jpg`
  (four-row chrome, short nav labels, `AI activity` workspace with old button
  row), `verified-quant-dark-large.jpg` / `verified-quant-dark-small.jpg`
  (duplicate empty guidance, unnumbered stages, no unresolved count, no
  enable reason), `verified-identification-dark-large.jpg` (raw UNC source
  path, unnumbered stages, `Signal indices`), `verified-reports-dark-large.jpg`
  (bare output field, unvalidated Export, uniform buttons).
- After (this pass, software renders): `ux-evidence/usability-review-data-light-1280.png`
  (single wrapped menu/nav row with full names, reordered inspector with
  `Selection` guide, project-aware status bar),
  `ux-evidence/usability-review-data-dark-920.png` (two-row wrapped nav at
  minimum width, plot still dominant),
  `ux-evidence/usability-review-xic-light-1280.png` (XIC half-width help,
  current-extraction summary, primary `Apply XIC`).
- Diffs are confined to the six files in `git status`
  (`delivery.rs`, `panels.rs`, `preset_editor.rs`, `quant.rs`, `spectral.rs`,
  `workbench.rs`) plus the three new evidence images and this document.

## 8. Remaining limitations (what still blocks production-quality feel)

1. **No unified finish.** Viewer CSV/SVG, batch CSV, and Reports export are
   still three exits. A first-time user who integrated a peak has no single
   visible path to a reviewed, calibrated, exported result.
2. **920×620 density.** In-plot control rows wrap over axes/legends in dark
   mode at minimum width. Axes stay inside the card (regression-guarded), but
   a collapsible or overflow-menu plot toolbar would be a genuine improvement.
3. **Advanced JSON still prominent.** Typed forms exist, but Identification /
   Untargeted / Statistics still lead with JSON-adjacent concepts
   (`Advanced operation JSON`, sample-major table JSON). Routine work should
   never require reading those words.
4. **Statistics onboarding.** The workspace is a stack of collapsed technical
   sections. It needs a 3-step guided form (load → groups → run) with the
   current power controls behind `Advanced`.
5. **Untargeted/OpenMS setup.** Version-pinned runtime messaging
   (`OpenMS 3.5.0`) and checkpoint-directory mechanics remain expert-only.
6. **No native re-verification in this pass.** Final light/dark minimum-size
   inspection was software-rendered; native typing, dialogs, DPI changes, live
   MCP sessions, and real vendor conversion were covered by prior evidence
   only. Screen-reader acceptance remains external (AccessKit enabled,
   not certified).
7. **Legend-inside-plot overlap** at default settings can cover peak tops;
   consider an outside-plot legend or smaller default at narrow widths.

## 9. Final assessment (evidence-backed answers)

1. **Does Chromascope feel like one coherent scientific application?**
   Mostly, but not fully. Navigation, status context, and terminology are now
   consistent across all seven destinations, and state is retained across
   windows. The three persistence models and three export exits still break
   the illusion of one workbench.
2. **Can a new analytical scientist discover common functionality intuitively?**
   For viewing (open → TIC → spectrum → smooth → integrate): yes, with the
   XIC and Selection improvements. For quantification-to-report: only partly —
   the numbered stages, unresolved count, and inline enable reasons help, but
   the end-to-end path still requires guidance.
3. **Is the interface visually polished and aesthetically consistent?**
   Consistent yes; polished mostly. Typography, spacing, palette, tables, and
   dialogs follow one system with passing contrast. Flat hierarchy was
   improved via primary-action styling and numbered stages, but 920 px dark
   mode still shows control crowding over plots.
4. **Are chromatograms and spectra given appropriate visual priority?**
   Yes at ≥1280 px: plots dominate, legends/axes/units are explicit,
   raw/processed overlay is honest, integration shading/baseline is clear.
   At 920×620 the priority holds (plot remains largest region) but wrapped
   controls encroach — the known, guarded limitation.
5. **Are workflows efficient for experienced LC-MS users?**
   Yes for experts: keyboard shortcuts (Ctrl/Alt+1–7, Tab, numeric RT/scan/
   boundaries), retained traces, Next-unresolved, drag boundaries, and batch
   preservation all serve speed. Routine multi-sample review still requires
   scrolling between table and plot.
6. **Is the interface unnecessarily cluttered anywhere?**
   Less than baseline (project row consolidated, duplicate brand removed
   earlier, duplicate empty states removed now), but Identification,
   Untargeted, and Statistics still expose advanced mechanics too eagerly.
7. **Does the application maintain scientific transparency and rigor?**
   Yes — and this pass preserves it: calculation-source labels, chord
   baselines, full-resolution integration, method-stale guards,
   original-vs-reviewed retention, explicit missing states, and unit-labeled
   axes are all untouched. No numerical code changed.
8. **What, if anything, still prevents production-quality feel?**
    Items 1–4 of section 8, in that order: unified review-and-release flow,
    minimum-width plot toolbar, JSON-forward advanced setup, and Statistics
    onboarding — plus native re-verification of the final build.

## 10. Final implementation pass (2026-10-09, this working tree)

All six follow-up priorities were implemented directly (presentation,
navigation, and gating only — no analytical algorithm changed) and the
final build was inspected in the real Windows desktop GUI with real
mouse/keyboard input and `test_file/data_dependent_02.mzML`. Full
procedure, per-workflow results, and honest limits are recorded in
[FINAL_ACCEPTANCE_TESTS.md](FINAL_ACCEPTANCE_TESTS.md).

### What changed

- **Unified analysis-to-report workflow (new `src/gui/workflow.rs`).**
  A nine-step project-centered guide (project → datasets → method →
  inspect → peak review → calibration → QC → approval → export) with live
  Done/Ready/Needs-review/Blocked/Not-started states computed from real
  application state, a compact `Next:` banner in Data explorer and
  Quantification, and contextual links that preserve
  project/dataset/analyte/result context. Reports gained a `Final review
  and release` section unifying viewer CSV/SVG, batch CSV, and project
  export: blocked exports stay disabled with reasons, warning exports need
  explicit acknowledgment, and acknowledged warnings are preserved as
  `review-warnings.json` in the bundle. Criteria come only from the
  configured method and retained engine reports.
- **Plot toolbar (rewrote `plot_controls::plot`).** Single non-wrapping
  row in a horizontal scroll; Reset/Zoom/Legend stay visible while pan,
  copy-bounds, and legend-corner selection moved to a `More` overflow
  menu; small legend text with per-plot corner memory. Verified natively
  at 920×620 (dark) and in renders (both themes).
- **Routine configuration (`forms::guided`, spectral, untargeted,
  targeted, ai_review).** Guided forms open by default with scientific
  orientation; JSON editors relabeled as intentional expert audit with
  explanations. Untargeted gained a faithful routine setup
  (samples/roles, detection, alignment, filtering) validated by
  `check_routine`; OpenMS version/checkpoint mechanics explained in plain
  language.
- **Statistics onboarding (restructured `statistics.rs`).** Five guided
  steps (load → groups → analysis → review → run) with counts, assignment
  tables, exclusion editor, assumption/missing-value/transform/FDR review,
  engine-`validate()` run gating, Python-environment check, and an honest
  statement of the single implemented analysis. Advanced controls open
  progressively; JSON stays as expert audit.
- **Polish.** Two-row workflow banner, list-layout guide below 950 px,
  scroll-bounded guide so plots keep their space, clarified OpenMS
  messaging. Two issues found *by* native inspection were fixed in this
  pass (unreachable grid action buttons; guide pushing plots out of
  reach).

### Verification results

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | Pass |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | Pass, no warnings |
| `cargo check --locked --all-features` | Pass |
| `cargo test --locked --all-features` (`-j 2`, local target dir) | **389 passed, 0 failed** (lib 242 + 17 integration suites; 5 ignored requiring external runtimes) |
| `git diff --check` | Pass |
| Native inspection | Real desktop GUI: import → TIC → spectrum → batch review → release gating; 1300×800 and 920×620 dark; Alt+1/Alt+5 keyboard; see `FINAL_ACCEPTANCE_TESTS.md` for the per-workflow table |

New tests pin the behavior: workflow step computation/navigation,
single-row toolbar overflow, statistics five-step rendering (920/1280,
both themes), `check_routine` rejection, warnings-file preservation, and
the pre-existing axes-inside-card and header-budget regressions (still
passing). One transient Windows file-lock failure in a delivery rename
test was observed once, passes in isolation, and passed in the final full
run — recorded, not hidden.

### After images (this pass)

- Native (real desktop, dark): `ux-evidence/native-final-empty-dark-1300.png`,
  `native-final-tic-dark-1300.png`, `native-final-spectrum-dark-1300.png`,
  `native-final-batch-dark-1300.png`, `native-final-release-dark-1300.png`,
  `native-final-data-dark-920.png`, `native-final-quant-dark-920.png`,
  `native-final-statistics-dark-920.png`,
  `native-final-statistics-dark-1300.png`,
  `native-final-untargeted-dark-1300.png`,
  `native-final-identification-dark-1300.png`.
- Software renders (real layout code, both themes): `ux-evidence/final-data-light-1280.png`,
  `final-data-dark-920.png`, `final-xic-light-1280.png`,
  `final-quant-light-800.png`, `final-reports-dark-1600.png`,
  `final-statistics-groups-1280.png`, `final-statistics-review-1280.png`.

### Remaining limitations (updates section 8)

Section 8 items 1–4 and 7 are resolved by this pass (unified finish,
toolbar overflow, JSON-forward setup, statistics onboarding, movable
legend). Still open: native light-theme capture, native
project-save/reopen and warning-preserving export run, native full
calibration/QC/AI-decision runs, 1920×1080 and 1440×900 inspection (the
1536×864 screen cannot show them), OpenMS/vendor/MCP end-to-end runs, and
external screen-reader certification. None is masked: each is recorded
with its precise blocker in `FINAL_ACCEPTANCE_TESTS.md`.
