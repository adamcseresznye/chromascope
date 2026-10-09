# Chromascope UX remediation

Date: 2026-10-09 (America/Los_Angeles). Scope: UX01–UX18 in
[UX_UI_AUDIT.md](UX_UI_AUDIT.md). The shared engine remains authoritative;
source hashes, stable identities and append-only review/project revisions are retained.
UX01-UX18 implementation and the local verification pass are complete. The matrix
distinguishes automated verification, native evidence and external acceptance limits;
it does not certify an analytical method.

## Implementation and verification matrix

Paths below are relative to the repository. Native images live in `docs/ux-evidence`.

| Issue / original problem | Implemented behavior and principal files | Verification / evidence |
|---|---|---|
| UX01: Fragmented analytical launch bars | Exclusive central Data, Quant/QC, Identification, Untargeted, Statistics, Reports and AI workspaces; drafts/workers retained. `gui/workbench.rs`, `panels.rs`, `mod.rs` | Navigation event regression; native central workspace images |
| UX02: Unlabelled statistical axes | Statistical score/loading/fold-change/FDR/linkage axes and linked evidence retain explicit units. `gui/statistics.rs` | Statistics numerical/reference and heatmap selection/render tests |
| UX03: Ambiguous calibration response units | Calibration response derives area versus IS ratio; residual zero reference, standards and fit diagnostics. `gui/targeted.rs` | Targeted independent regression, inverse roots, flags and GUI review tests |
| UX04: Raw versus processed evidence unclear | Simultaneous unsmoothed/processed viewer traces with cached display reduction; advanced raw/smoothed/baseline/corrected segments. Integration states its full-resolution calculation source. `gui/plotting.rs`, `workbench.rs`, `quant.rs` | Raw-area/smoothing/irregular-scan integration tests; native raw/processed trace inspection; wrapped-control height regression |
| UX05: Light-theme baseline contrast | Theme-aware dashed endpoint chord and integration boundaries. `gui/plotting.rs` | Theme/render checks; numerical area tests unchanged |
| UX06: Gesture-only integration bounds | Finite, increasing, in-domain numeric RT integration using the existing kernel. `gui/workbench.rs` | Numeric integration and domain-validation tests; prior native integration evidence |
| UX07: Missing activity and job visibility | Project/analysis worker status, recoverable errors and MCP session history; asynchronous report verification and SVG exports. `gui/ai_review.rs`, `delivery.rs`, `plot_controls.rs`, `project_workbench.rs` | Worker/recovery tests and native AI activity |
| UX08: No coherent per-proposal review gate | Shared typed proposals, original/proposed values, reason/evidence, approve/reject/revise/reversal, source/revision checks and retained events. MCP prepare/queue/resolve persists the same review contract. `proposals.rs`, `engine_mcp.rs`, `gui/ai_review.rs`, `project.rs` | `tests/proposals.rs`: isolation, replay, rejection/approval, stale/tampered evidence, reversal, actual MCP queue/resolve; native activity |
| UX09: JSON-heavy routine setup | Typed serialization-backed routine editors, optional fields/collections and DTO-specific sample roles; advanced JSON disclosed separately. `gui/forms.rs`, `targeted.rs`, `qc.rs`, `spectral.rs`, `untargeted.rs`, `annotation.rs`, `statistics.rs`, `delivery.rs` | Existing typed engine roundtrips and GUI interactions; unchanged values retain full precision |
| UX10: Disconnected session/project/report context | Create/open/save/refresh project, content-addressed workbench snapshot, verified sources, result routing, path relinking and direct matrix/concentration handoffs. `gui/project_workbench.rs`, `workspace.rs`, `project.rs` | Workspace hash/precision/UUID tests; desktop snapshot/stale-save regression; native CLI project open and revision save |
| UX11: Dense quantitative vertical hierarchy | Quantification stages separate area review, calibration/concentrations and QC; contextual rule detail. `gui/quant.rs`, `qc.rs` | Small/large quant rendering and review interactions; native QC stage |
| UX12: Fixed diagnostic window | Bounded resizable scrollable original diagnostics. `gui/dialogs.rs` | Error/recovery regressions; source/native containment inspection |
| UX13: Weak selected text and trace contrast | Theme text/secondary/status/analytical palettes, textual acceptance flags, user-color retention with contrast warning/explicit alternative; trace names use readable text plus color swatch. `gui/workbench.rs`, `workspace.rs`, `plot_controls.rs`, `statistics.rs` | Contrast ≥4.5 text/palette test; native light/dark theme inspection; no automatic recoloring of user evidence |
| UX14: Limited keyboard and semantic access | Ctrl/Alt workspace navigation, Tab/focus, numeric RT/scan/integration and selectable linked table cells; AccessKit enabled and form labels attached. `Cargo.toml`, `gui/workbench.rs`, `forms.rs`, `table.rs` | Event modifier regression; native keypad Ctrl navigation and visible Tab focus; focused-control scrolling regression; screen-reader acceptance remains external |
| UX15: Overflow at small window sizes | Central workspaces and bounded scrolling dialogs; wrapped controls/report options. `gui/workbench.rs`, `delivery.rs`, `dialogs.rs`, `preset_editor.rs`, `information.rs` | Small/large renders; native large/minimum-size workspace matrix |
| UX16: Unsortable long result grids | Shared full-model filter/numeric sort, stable original selection, numeric alignment, precision hover, keyboard column sizing, virtualized rows. Linked concentrations, batch peaks, QC decisions/observations, libraries/candidates/fragments/formulas/isotopes, features and comparisons; heatmap rows virtualized. `gui/table.rs`, domain GUI modules | Numeric identity/filter regression; linked evidence GUI tests; native QC table |
| UX17: Inconsistent plot controls | Common reset, zoom, pan, legend and bounds; explicit scientific axes; full-resolution line/scatter/stick/multiseries SVG exports retain domain gestures. `gui/plot_controls.rs`, domain GUI modules, `delivery.rs` | Signed/repeated-x scatter and disconnected-series SVG tests; native common controls |
| UX18: Smoothing described as scans rather than points | Smoothing radius described in trace points, window length and edge truncation. `gui/workbench.rs` | Duplicate-RT/preparation and smoothing regressions |

## Verification record

- `cargo fmt --check`: passed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed;
  `target/ux-remediation-clippy-complete.log`.
- `cargo test --all-features`: **381 passed, zero failed, five ignored** across
  21 summaries, including GUI rendering, focus/plot layout, numerical references,
  CLI/MCP contracts and project/proposal persistence;
  `target/ux-remediation-tests-complete.log`.
- `cargo test --no-default-features --features mcp-headless`: **282 passed, zero
  failed, five ignored**; `target/ux-remediation-headless-complete.log`. The final
  subsequent change affects only GUI plot height/label layout.
- `cargo build --all-features --bins`: passed. After the final GUI label correction,
  `cargo build --all-features --bin chromascope` passed again;
  `target/ux-remediation-build-complete.log`.
- `chromascope-cli project-verify target/ux-native-report-20261009`: passed,
  **12 files verified** after native report export.
- Source/binary SHA256 manifest: `target/ux-remediation-final-hashes.json`. Final
  native executable: `target/ux-remediation-final-verified.exe`, SHA256
  `20f932b8cbbb419a4b5dfaea9e8254e1c5aa7c1efd592ff05fe04b56a9352761`.

### Native workflow evidence

Actual Windows eframe inspection covered all seven workspaces in light/dark themes
and larger/narrow windows. Advanced-workspace captures are
`verified-{quant,identification,untargeted,statistics,reports,ai}-{theme}-{size}.jpg`.
The native minimum-size pass used a 920 x 600 client; final narrow captures are
approximately 920 x 620 clients. Final Data captures use Focus mode with both raw
and processed evidence: `final-data-{dark,light}-{large,small}.jpg`.
`final-identification-keyboard-scroll.jpg` shows a previously lower retained-result
control scrolled into view on Shift+Tab. The actual tested navigation shortcut was
Ctrl plus numeric keypad 1-7; number-row/Alt behavior was not established on this
keyboard layout. Event-level tests cover modifier handling.

Baseline: `remediation-before.jpg`. Earlier `*-review.png` and
`statistics-workspace.png` are software renders with synthetic fixtures, distinct
from native images. Earlier native captures labelled before-layout/pre-layout
record defects and are not final acceptance images.

The native project binds a synthetic QC study to shipped acquisition
`test_file/data_dependent_02.mzML`. CLI created it; desktop opened revision 1 and
saved a workspace snapshot in revision 2. MCP prepared and queued a review proposal
in revision 3. Desktop inspected original/proposed values, approved it with an
explicit reason and saved revision 4. The final rebuilt desktop reopened revision 4
with traces and retained history, then saved final display settings in revision 5.
Evidence: `verified-native-mcp-proposal-before-approval.jpg`,
`verified-native-mcp-proposal-approved.jpg`, `verified-native-proposal-project-revision4.jpg`,
`verified-native-report-export.jpg`. Acquisition evidence does not establish the
synthetic QC study as an assay validation.

### Retained failures and corrections

Earlier failures are preserved in the validation history. The native proposal probe
initially used an older CLI fixture with different calibration explanatory text;
canonical evidence validation rejected it. Loading the retained project result
resolved the probe, with no relaxed validation. The focused-scroll regression first
failed because simulated frames did not advance animation time; advancing input
time made the unchanged visibility assertion meaningful. The last plot test initially
assumed PlotResponse included axes; it represents the drawing canvas. Its canvas
minimum now checks 100 pixels while the unchanged total-card-height assertion guards
axis containment. Native inspection confirms the two-line units label and wrapped
controls. A mistaken `verify-bundle` CLI probe failed with usage; the documented
`project-verify` command succeeded. No numerical assertion was weakened.

## Practical limits

AccessKit exposure and keyboard controls do not establish screen-reader/platform
certification. Reviewer actor names are retained attribution, not authenticated identity.
Legacy MCP command authorization remains its existing launch policy; the explicit
prepare/queue/resolve proposal tools supply the new per-proposal review gate.
Five ignored tests require external adapter/performance prerequisites; ordinary
regressions do not replace those scientific acceptance runs. The shared checkout
contains concurrent work; no commit, push, publication or release certification was performed.

## Native-driven corrections

Inspection caught corrupt UTF-8 punctuation in quantitative controls, selected
trace names painted in their plot color, and report controls requiring unnecessary
vertical travel. Quantitative symbols are corrected; names now use theme-readable
text with a separate retained-color swatch; report sections wrap. Report source
verification and full-resolution SVG generation/write execute on workers.

The minimum-size inspection also found plot controls consuming axis space and
keyboard focus moving below the viewport. Plot budgets now subtract wrapped
control height, preserve a 150-pixel minimum plot height and split the vertical
intensity/units label across two lines; newly focused controls request scrolling. Both corrections
have targeted regression tests.
