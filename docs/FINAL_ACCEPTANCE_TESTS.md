# Chromascope final acceptance tests

Date: 2026-10-09. Build: 0.3.0 plus the final UX pass in this working tree
(`src/gui/workflow.rs` new; `delivery`, `plot_controls`, `spectral`,
`untargeted`, `statistics`, `targeted`, `quant`, `panels`, `forms`,
`ai_review`, `qc`, `project_workbench` extended). No analytical algorithm
was changed; all edits are presentation, navigation, gating, or test-only.

Evidence images: `docs/ux-evidence/`. Files prefixed `native-final-` are
captures of the real Windows desktop application (`chromascope.exe`, debug,
driven with real mouse/keyboard input and the repository dataset
`test_file/data_dependent_02.mzML`). Files prefixed `final-` are
software-rendered egui frames produced by the real layout code in this tree.
The two classes are never mixed.

Environment limits (recorded honestly, not worked around):

- Primary screen is 1536×864, so native 1920×1080 and 1440×900 inspection
  is impossible here. Native sizes inspected: ~1300×800 and 920×620.
- Synthetic mouse input goes through OS DPI virtualization this session
  could not calibrate exactly, so small click targets sometimes missed;
  every claimed native result below was confirmed on a screenshot.
- Background applications repeatedly stole window focus; keyboard results
  were re-verified after refocusing.
- No OpenMS runtime, ProteoWizard, or vendor datasets are installed, so
  untargeted execution, vendor conversion, and MCP client sessions were not
  exercised end to end. Screen-reader acceptance remains external
  (AccessKit enabled, not certified).

## 1. Results

| # | Workflow | Expected | Actual | Evidence | Limits |
|---|---|---|---|---|---|
| 1 | Launch, empty state (dark) | Guided empty state with project/banner affordance | Banner shows step 9 blocked state, `Next: Export report`, collapsible guide, `Open data…` | `native-final-empty-dark-1300.png` | Light theme native not captured; software renders cover light |
| 2 | File menu + open dialog | Native `File` menu; file picker opens | Menu opened; file picker opened; invalid-path error handled by OS dialog | `native-filemenu2.png`, `native-row1.png` | One mis-click opened the folder picker first; recovered with Esc |
| 3 | Import real mzML + TIC | 1 dataset loads; TIC extracts automatically | `data_dependent_02.mzML` active; TIC peak ~10.8 min; workflow steps 2–4 flip to Complete live | `native-final-tic-dark-1300.png` | — |
| 4 | Plot toolbar at 920×620 (dark) | Single row; nothing wraps onto axes/peaks; secondaries in overflow | `Reset view · Zoom + · Zoom − · Legend` visible; `More` reachable by horizontal scroll; axes clear | `native-final-data-dark-920.png`, software `final-data-dark-920.png` | `More` is half-visible at exactly 920 px and needs a small scroll; axes/plot never covered |
| 5 | Legend behavior | Small legend, movable when it covers peaks | Small legend top-right, clear of the peak; corner choice in `More`; toggling works | `native-final-tic-dark-1300.png`, toolbar render test | Legend-inside-plot retained by engine design; outside-plot list not implemented |
| 6 | Double-click spectrum | Spectrum loads with scan navigation | `RT 10.011 min`, `Scan 1`, `1 / 53`, Previous/Next/Go all live | `native-final-spectrum-dark-1300.png` | Clicks landing on the legend do not reach the plot (motivates the movable legend); numeric RT alternative visible but not clicked |
| 7 | Quantification workspace | Banner, cross-links, stages, batch run, review | Banner + `← Inspect data` / `Continue to Reports →`; `Run batch` executed; `Batch complete`; stage tab shows `1 unresolved`; results table `Missing` | `native-final-batch-dark-1300.png` | Default method targets m/z 100 (absent), so the honest result is `Missing`; calibration/QC stages opened but not run with standards |
| 8 | Workflow warnings live | Unresolved peak surfaces in banner and step 9 | `1 warning(s) will accompany export` appears in banner and step 9 detail | `native-quant-920-dark.png`, `native-final-batch-dark-1300.png` | — |
| 9 | Method editor | Guided form opens, edits, collapses, drags | Window opened, dragged, collapsed via title bar; detection/analyte grid editable | `native-methoded.png`, `native-drag.png` | Close `×` sits off-screen at 1300 px when the window is at default width (pre-existing window sizing, recorded for backlog) |
| 10 | Reports release section | Blocked state with reason, disabled export, contextual links | `Blocked — No project is open…`, disabled `Export report and project`, reason line, Data/QC/AI links | `native-final-release-dark-1300.png`, software `final-reports-dark-1600.png` | Export-with-warnings and `review-warnings.json` verified headless (unit test `acknowledged_warnings_are_preserved_as_json`, delivery suite); no native project created (folder-dialog automation limit) |
| 11 | Statistics onboarding | 5-step navigator, step panels, validation, run gate | Step 1 renders natively with honest empty-table error and `Continue`; steps 1–5 asserted in render test at 920/1280, both themes; `Analyze` gated on `precheck` | `native-final-statistics-dark-920.png`, `native-final-statistics-dark-1300.png`, software `final-statistics-groups-1280.png`, `final-statistics-review-1280.png` | Steps 2–5 clicked only headless; statistics *execution* needs local Python+NumPy/SciPy (present: 3.13.7, used by the passing adapter test) but long runs were not triggered natively |
| 12 | Identification guided forms | Default-open guided forms; JSON demoted to Advanced | `Spectrum processing` open with help; `Advanced processing JSON — expert…` collapsed; file-name (not UNC) source label | `native-final-identification-dark-1300.png` | Library search not executed (no library/mock spectra loaded natively) |
| 13 | Untargeted routine setup | Default-open routine section; validation in scientific language | `Routine setup — samples, detection, alignment, filtering` open; polarity/tolerance/noise/width controls; `check_routine` unit-tested | `native-final-untargeted-dark-1300.png` | OpenMS execution not available here; covered by engine tests only |
| 14 | Keyboard navigation | Alt/Ctrl+1–7 switch workspaces | Alt+1 → Data explorer, Alt+5 → Statistics verified on screenshots | `native-stats-920.png`, `native-data-920-dark2.png` | Full Tab-order walkthrough and other shortcuts not traced; unit test covers modifier handling |
| 15 | Project persistence | Save/reopen retains revisions | Headless: project round-trip, revision, corruption, and workspace tests all pass | `cargo test` project/workspace suites | No native project created or reopened (dialog-automation limit) |
| 16 | AI activity/proposals | Review/approve with reasons | Headless proposal/ai_review suites pass | `cargo test` proposals/engine_mcp suites | No native proposal approved (none existed in the exploratory session) |
| 17 | Resize behavior | Usable at 920×620; chrome ≤118 px | Two-row wrapped nav; banner wraps to two rows; plots keep priority; header bound asserted | `native-final-quant-dark-920.png`, `native-final-data-dark-920.png`, compact-header test | 1920×1080 and 1440×900 not testable on this screen |
| 18 | Regression suite | fmt, clippy, check, all tests | `cargo fmt --check` clean; `clippy -D warnings` clean; `cargo check` clean; **389 passed, 0 failed** (lib 242 + 17 integration suites; 5 ignored requiring external runtimes) | `test-final.txt` (temp dir, not committed) | One transient Windows file-lock failure in `delivery` rename seen once; passes in isolation and in the final full run |

## 2. What was implemented (this pass)

- **P1 — one coherent workflow** (`gui/workflow.rs` new): nine-step
  project-centered guide with live `Done/Ready/Needs review/Blocked/Not
  started` states computed from real state (project, datasets, method
  validity, plots, unresolved peaks incl. stale-method, calibration
  fits/errors, concentration validity, QC verdicts, review coverage, AI
  proposals, export readiness). Compact `Next:` banner in Data explorer and
  Quantification; contextual `← Inspect data` / `Continue to Reports →`
  links that preserve selection. Reports gained a `Final review and
  release` section unifying the three export exits: blocked (missing
  preconditions, export disabled with reason), warnings (explicit
  acknowledgment required), clean. Exports with warnings write
  `review-warnings.json` into the bundle. Acceptance criteria come only
  from the configured method and retained engine reports.
- **P2 — plot toolbar** (`gui/plot_controls.rs`): single non-wrapping row
  in a horizontal scroll; essential `Reset/Zoom/Legend` stay visible while
  pan, copy-bounds, and legend-corner selection moved to a `More` overflow
  menu; small legend text; per-plot legend corner memory. Regression tests
  pin single-row overflow and the existing axes-inside-card bound.
- **P3 — routine configuration** (`gui/forms.rs` `guided()` default-open
  variant; `spectral.rs`, `untargeted.rs`, `targeted.rs`, `ai_review.rs`):
  routine sections open by default with one-line scientific orientation;
  JSON editors relabeled as intentional expert audit capabilities with
  explanations. Untargeted gained a faithful routine form (samples/roles,
  detection, alignment, filtering) with `check_routine` validation in
  scientific language; expert settings untouched.
- **P4 — statistics onboarding** (`gui/statistics.rs`): 5-step guided flow
  (load → groups → analysis → review → run) with sample/group/exclusion/
  transform/FDR summaries visible before execution, group-assignment
  validation, engine `validate()` pre-check gating `Analyze`, cached
  local-Python availability check, and an honest statement that the single
  combined analysis (PCA, clustering, optional Welch+BH/BY) is the only
  implemented method. Advanced JSON editors retained as expert audit.
- **P5 — polish**: two-row workflow banner (no truncation), list-layout
  guide below 950 px, scroll-bounded guide in the explorer panel, OpenMS
  checkpoint/version messaging clarified, consistent advanced labeling.

## 3. Remaining limitations and blockers

1. No native light-theme capture (theme toggle clicks missed under OS DPI
   virtualization); both themes verified in software renders + contrast
   unit tests.
2. No native project create/save/reopen or export-with-warnings run;
   covered headless (project, delivery, proposals suites + new warnings
   unit test + release-section render test).
3. No native full calibration/QC/AI-decision run (needs standards, study
   rules, proposals); engine suites cover the numerics.
4. 1920×1080 / 1440×900 native inspection impossible on the 1536×864
   screen; 1300×800 and 920×620 verified natively.
5. `More` needs a small horizontal scroll at exactly 920 px; legend stays
   inside-plot by engine design (movable, not external).
6. Method-editor close `×` can sit off-screen at default width (pre-existing;
   collapse/drag work).
7. Screen-reader acceptance remains external; OpenMS/vendor/MCP
   end-to-end runs need those runtimes.

## 4. Verdict

The final build was inspected natively (real Windows desktop GUI, real
mouse/keyboard, real repository mzML). End-to-end native coverage runs
from file import through TIC, spectrum, batch review, and release gating;
analytical numerics, project persistence, calibration, QC, statistics, and
the warning-preserving export are covered by the green headless suites
plus software-rendered layout tests in both themes. No tests were
weakened, no warnings suppressed, no evidence fabricated; the two
transient environment failures seen during the pass (OneDrive quota/page
file in the prior pass; one Windows file-lock rename here) were recorded
and re-verified rather than hidden.
