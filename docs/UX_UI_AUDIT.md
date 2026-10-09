# Chromascope UX and scientific interface audit

Audit date: 2026-10-08, America/Los_Angeles. Package: 0.3.0.
Scope: the supplied working tree, including pre-existing uncommitted analytical
modules. This report assesses the desktop adapter and its relationship to the
shared engine; it does not certify an assay or instrument.

## 1. Executive assessment

Chromascope has a strong scientific foundation: extraction parameters travel with
traces, integration uses full-resolution data, batch review preserves automatic
selections, and newer engine workflows retain immutable evidence. The interface
has grown faster than its information architecture. Separate launch bars, long
forms and JSON-oriented configuration make the advanced functionality feel like
extensions to a viewer rather than stages of one analytical workbench.

This delivery consolidates workflow navigation, clarifies scientific units and
processing semantics, improves calibration/statistical plots, adds numerical
exploratory integration, and exposes the existing MCP activity record. It preserves
numerical algorithms, CLI operations, MCP commands and existing results.
The 2026-10-09 remediation adds typed configuration, unified project snapshots,
shared proposal review, staged central workspaces, reusable tables and plot controls.
The original workflow observations below are retained as baseline findings; the
issue register and [remediation matrix](UX_UI_REMEDIATION.md) describe current behavior.
Platform screen-reader acceptance and instrument-specific validation remain external gates.

Primary personas:

- Analytical scientist: inspect acquisition, extract ions and defend integrations.
- Quantitative assay analyst: configure standards/IS, review samples and QC.
- Omics researcher: detect/alignment features, assess identity and statistics.
- Method developer: tune processing with retained raw evidence and revisions.
- Automation operator: inspect agent actions, permissions and reproducibility.

## 2. Repository, architecture and evidence limits

`parser` and `processing` implement acquisition access and viewer kernels.
`quant`, `targeted`, `qc`, `spectral`, `untargeted`, `annotation`, `statistics`
and `delivery` implement analytical domains. `engine`, `domain`, `jobs` and
`project` provide typed operations, jobs and retained evidence. `src/gui` is an
egui/eframe presentation layer. The GUI MCP bridge and headless engine MCP are
distinct surfaces. Architecture/roadmap documents contain historical baseline
descriptions; source inspection is authoritative for the current working tree.

Native Windows UI interaction and screenshots are available. The installed debug
executable used for the **before** capture predates some newer source workflows:
the image is a baseline executable capture, not proof that every source launcher
was present in that binary. Source inspection confirmed the separate identification
top bar and untargeted/statistics/reports bottom bars. The revised executable was
built and exercised with `test_file/data_dependent_02.mzML`.

Evidence types must remain distinct:

- Native inspection: actual eframe window and Windows file dialog.
- Software rendering: actual egui frames rasterized by `gui/test_render.rs`;
  synthetic test data are not measured samples or native screenshots.
- Source walkthrough: control order, parameters, validation and result handling.
- Automated tests: regression/reference evidence, not a complete human workflow.

## 3. Current and recommended information architecture

Before: Viewer / Batch Quantification, File / Display / theme, viewer controls,
identification in a separate top panel, and three separate bottom launch panels.
Calibration and QC are nested within batch quantification. Omics annotation lives
inside feature review. Reports use an engine project directory rather than the
viewer session. MCP history was available programmatically without a desktop view.

Implemented navigation:

| Destination | Scientific task | Behavior |
|---|---|---|
| Data explorer | Import, metadata, chromatograms, spectra, exploratory integration | Main workspace |
| Quantification and QC | Methods, batch areas, review, calibration, acceptance | Main workspace |
| Identification… | MS/MS processing, libraries, candidate evidence | Retained analysis window |
| Untargeted analysis… | Detection, alignment, feature/omics evidence | Retained analysis window |
| Statistics… | Preprocessing, comparisons, PCA and clustering | Retained analysis window |
| Reports… | Project evidence export | Retained analysis window |
| AI activity… | MCP permissions, requests and errors | Session activity window |

Ellipses deliberately indicate windows, rather than falsely presenting six
independent full-screen tabs. Closing a window preserves its analysis state.
Chromatography belongs with exploration and peak review; lipidomics/metabolomics
belong with feature evidence rather than requiring additional top-level tabs.
Keep File for import/persistence/export, Display for appearance, and one consistent
workflow row. Future work should migrate large analysis windows into central
workspaces with a shared project context without discarding drafts or worker state.

## 4. Workflow-by-workflow walkthrough

These are source/control walkthroughs unless native evidence is stated. Interaction
counts describe minimum transitions, excluding typing, optional filters and file
dialog navigation; they are not timed usability-study measurements.

| Workflow | Route and assessment | Priority / follow-up |
|---|---|---|
| A Import / inspect | Open data → file dialog → dataset selection → acquisition details. Native mzML import succeeds. Vendor folder import and converter configuration are separate, with cancellation and local conversion feedback. Dataset labels can truncate; full paths are available on hover. | Medium: expose metadata beside dataset selection; test real vendor conversion separately. |
| B Chromatograms / spectra | TIC/BPC selection → double-click trace → previous/next scan or scan number. Native apex spectrum inspection succeeds. RT is minutes; scan selection is linked to acquisition. Gesture help is persistent. | High: provide keyboard spectrum selection from a numeric RT as an alternative to double-click. |
| C XIC | Select XIC → enter m/z and ppm → Apply XIC. Invalid parameters remain editable and validation explains the failure. Retained trace parameters support reproducibility. | Medium: add explicit tolerance half-width explanation and acquisition-context help. |
| D Processing / integration | Smoothing radius → inspect → integrate. Viewer moving average affects calculation/export; batch detection smoothing is distinct from unsmoothed quantitative area. Baseline chord and boundaries are drawn. New numeric boundaries reuse the same integration and measurement history. | High: side-by-side raw/processed evidence and explicit baseline method selection context. |
| E Manual peak review | Quantification → result row → bounds → apply correction with existing review/history controls. Method-stale results are guarded. Automatic selection remains available. | Medium: reduce vertical travel and keep selected result visible during correction. |
| F Calibration / concentration | Batch → targeted configuration → sample roles/nominal levels/IS → execute → calibration and residual evidence. Structured controls coexist with advanced JSON. Fit labels now distinguish area from dimensionless IS ratio. | High: dedicated sequential calibration form; make invalid concentration explanations prominent. |
| G QC / acceptance | Targeted batch → QC configuration → evaluation → decision evidence → acknowledgement. Text statuses supplement colors; missing required evidence prevents a pass. Acknowledgement does not change acceptance. | High: surface unresolved QC count in the main batch header. |
| H MS/MS / identity | Identification → source scans → processing → library query → scored evidence → annotation review. Retained revisions and unknown/unreviewed status avoid claiming confirmation. | High: replace remaining JSON-heavy source/library setup with typed controls and linked precursor context. |
| I Feature detection / alignment | Untargeted → samples/configuration → OpenMS job → matrix → selected feature/EIC/MS1/MS2. External engine/version requirements are distinct from missing data. Raw feature RT is seconds. | High: integrated job progress and guided adapter setup; retain explicit seconds/minutes distinction. |
| J Metabolomics / lipidomics | Feature matrix → annotation candidates → formula/adduct/isotope or lipid evidence → review history. Candidate identity is evidence-dependent, not automatic confirmation. | High: contextual confidence explanations and ranked candidate tables. |
| K Statistics | Statistics → quantitative matrix/response → settings/groups → execute → PCA/loadings/volcano/heatmap → linked evidence. Exclusions, imputation and revisions are retained. Statistical axes were missing and are now explicit. | High: guided metadata mapping; typed import from CSV in the GUI, not just engine/CLI paths. |
| L Reports / exports | Viewer CSV/SVG or batch CSV; Reports → engine project → sections/output destination → export. These are different persistence models, currently requiring an explicit handoff. | High: unified project selection and viewer-to-project save path. |
| M MCP / AI | Client launches authorized MCP executable → calls → GUI/engine actions → history. New AI activity shows permissions and recent request outcomes with expandable parameters. | High: proposals/evidence/approval/reversal are not one coherent GUI flow yet. Do not describe activity history as an approval system. |

## 5. Issue register and acceptance criteria

Severity describes user/scientific impact; status is limited to this delivery.

| ID / severity | Location and problem | User impact | Recommended solution / acceptance criteria | Status |
|---|---|---|---|---|
| UX01 High | `gui/{spectral,untargeted,statistics,delivery}` independent launcher bars | Fragmented discovery and lost plot height | One consistent workflow navigation; no duplicate launcher panels; all analytical entry points remain reachable | Implemented |
| UX02 High | `gui/statistics` plots lack axis labels | PCA, q and raw evidence units must be inferred | Label scores/loadings, fold-change direction, FDR q, linkage distance, RT and m/z on the actual axes | Implemented |
| UX03 High | `gui/targeted` calibration response labeled ambiguously | Area may be mistaken for IS ratio | Derive ordinate label from target IS configuration; show fit/standard legend and zero residual reference | Implemented |
| UX04 High | Viewer smoothing/integration description | Analysts may mistake processed area for raw area | Explain calculation/export effect and chord baseline; raw trace accessible with smoothing 0; no algorithm changes | Implemented: simultaneous raw/processed overlay and calculation-source labels |
| UX05 Medium | `gui/plotting` yellow baseline in light theme | Integration baseline has weak contrast | Theme-aware amber/brown dashed chord, distinguishable from trace and boundaries | Implemented |
| UX06 High | `gui/workbench` integration gesture only | Precise boundaries and keyboard access are difficult | Typed start/end minutes, finite/increasing/in-domain validation, compute via existing kernel, retain measurement | Implemented |
| UX07 High | GUI MCP history absent | Operators cannot inspect agent activity without tool/client logs | Concise success/failure entries, permissions and pending data-work indication; expandable request/error details | Implemented: worker/activity status and separate proposal review |
| UX08 High | GUI MCP changes execute under launch permissions | No consistent evidence-backed per-proposal approve/reject/undo flow | Typed proposed change + source revision + reason/evidence; approve/reject before mutation; reverse with new revision | Implemented: shared prepare/queue/resolve contract, GUI review and append-only reversal |
| UX09 High | Advanced analysis JSON configuration | High cognitive load; schema errors interrupt analysis | Typed forms with sample-role/units validation and advanced JSON behind disclosure; preserve exact serialized methods | Implemented: typed routine forms and optional advanced JSON |
| UX10 High | Viewer session / engine project / report handoff | Analysts repeat imports and lose workflow context | Unified project selector and explicit artifact transfer; source hashes/revisions retained end to end | Implemented; see remediation evidence and limits |
| UX11 Medium | Quantification dense vertical workspace | Review controls/tables fall below viewport | Scrollable central workspace; next stage always reachable at minimum size | Implemented: three quantitative stages with scrolling and retained context |
| UX12 Medium | Error window has fixed size | Long converter/engine diagnostics can be inaccessible | Resizable, scrollable diagnostics and clearly named dismissal; preserve original error text | Implemented |
| UX13 Medium | Text and swatches / palette | White/yellow traces in light mode and blue selected text can have weak contrast | Verify selected text, traces and focus at both themes; names plus swatches; retain user colors, warn on low contrast and offer an explicit visible-color choice | Implemented; see remediation evidence and limits |
| UX14 High | Spectrum gesture / accessibility | Plot actions and heatmap cells are difficult with keyboard/screen reader | Keyboard scan/RT/feature selection; semantic labels; test AccessKit/assistive technology and visible focus | Implemented: numeric scan/RT/integration, keyboard navigation and focus scrolling; screen-reader acceptance external |
| UX15 Medium | Reports and analytical dialogs can exceed window height | Small-window controls become inaccessible | Vertical scrolling, bounded default size; keyboard traversal and minimum-size checks | Implemented: central scrolling, bounded dialogs and wrapped report controls |
| UX16 Medium | Tables use grids and long identifiers | Slow comparison, limited sorting/filtering and virtualized rows | Stable selection, sortable headers, aligned numeric columns, text flags, row virtualization | Implemented; see remediation evidence and limits |
| UX17 Medium | Plot behavior differs across domains | Zoom/reset and selection learning does not transfer | Shared plot helpers for axis/legend/reset/selection with scientific unit input; retain domain-specific gestures | Implemented; see remediation evidence and limits |
| UX18 Low | Moving-average text calls points scans | Duplicate-RT preparation may collapse scans | Describe radius in trace points, maximum window length and edge truncation | Implemented |

No Critical defect was established by this UX inspection. This does not imply
that numerical or release validation is complete. The remediation adds proposal and project provenance controls. Software UX
verification does not establish authenticated reviewer identity or assay certification.

## 6. Visual design, accessibility and reusable system

The existing Inter font is appropriate for dense instrument software; retain
fallback fonts for scientific symbols. Use a quiet instrument-workbench palette:
dark panel #272E37, dark plot #20252C, light panel #F3F5F8, white plot,
selection #293F58 (dark) / #E0EDFC (light), blue focus #77B9F1 / #1968B3.
Scientific status must always include a word/flag/evidence, never color alone.

Shared `workbench` dimensions now name body 14 px, small 12 px, control height
28 px and inspector default 270 px. Headings use Inter Medium at 18 px; spacing
uses 8 px and button padding 10 × 6 px. Panels remain resizable; navigation wraps.
Prefer bounded scroll areas to enormous dialogs and quiet separators to repeated
cards. Plots should occupy the largest central region, tables should align values
by units/precision, and raw versus processed data should be named in legends.

Accessibility: typed integration provides a non-gesture alternative; controls
retain standard egui keyboard focus and text tooltips. Screen-reader support,
contrast measurements, high-DPI testing across monitors and full keyboard-only
scientific workflows are not certified. Tiny unlabeled swatches, plot selection
and heatmap interaction still need semantic alternatives.

## 7. Implementation boundaries

Changes are presentation-only except a new route to the existing exploratory
integration command. No extraction, smoothing, baseline, fit, detection,
alignment, statistical or annotation algorithms were changed for this redesign.
Existing scientific revisions, review state and export semantics remain intact.
Calibration labels derive from target metadata; raw statistical linked EIC units
distinguish targeted minutes from untargeted seconds. Spectrum axes use m/z (Th)
and intensity remains in instrument units, avoiding unsupported physical units.

## 8. Validation and evidence

Native baseline and modified builds were launched. Native import and apex scan
inspection succeeded on the bundled mzML fixture. Screenshots are retained in
`docs/ux-evidence`; native captures are JPEG images.

- [Baseline executable](ux-evidence/before-native.jpg)
- [Revised navigation](ux-evidence/after-native-navigation.jpg)
- [Imported mzML](ux-evidence/after-native-data.jpg)
- [Retained TIC and XIC](ux-evidence/after-native-xic.jpg)
- [Identification window](ux-evidence/after-native-identification.jpg)
- [Numeric integration and light-theme baseline](ux-evidence/after-native-integration-light.jpg)
- [AI activity empty state](ux-evidence/after-native-ai.jpg)
- [Reports and exports window](ux-evidence/after-native-reports.jpg)
- [Light workbench, synthetic fixture](ux-evidence/rendered/light.png)
- [Dark minimum-width workbench, synthetic fixture](ux-evidence/rendered/dark.png)
- [Sample comparison, synthetic fixture](ux-evidence/rendered/comparison.png)
- [Quantification at 800 px, synthetic fixture](ux-evidence/quant-preview-800-false.png)
- [Targeted calibration and evidence, test fixture](ux-evidence/targeted-review.png)
- [QC decision and review, test fixture](ux-evidence/qc-review.png)
- [Spectral evidence review, test fixture](ux-evidence/spectral-review.png)
- [Feature matrix, test fixture](ux-evidence/untargeted-review.png)
- [Statistics and heatmap selection, test fixture](ux-evidence/statistics-workspace.png)

Rendered images were inspected for workspace hierarchy and visible scientific
labels. The 800 px quantitative render demonstrates wrapped navigation and a
scrollable workspace; it also shows that review controls still require scrolling.
Statistics retain a visible missing-comparison state rather than inventing volcano
points. These software renders use synthetic/reference data, not native assays.

Final validation:

| Check | Observed result |
|---|---|
| `cargo build --locked --bin chromascope` | Passed; default desktop build preserved |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | Passed |
| `cargo test --locked --all-features` | Passed: 366 tests, 0 failures, 5 ignored across library, integration and documentation suites |
| GUI rendering | 90 GUI tests exercised; 89 passed on initial preview run, one failed only because the preview directory did not exist; directory created and workbench render rerun passed |
| Native mzML | Import, TIC, 524.3 m/z / 10 ppm XIC retention, and apex spectrum inspection succeeded |
| Native numeric integration | Typed 10.700–11.400 min on fixture TIC; one recorded measurement, area approximately 1.2606e8 instrument intensity·min; bounds/shading/chord visible |
| Native themes/navigation | Dark/light toggle, numeric input typing, inspector scrolling, identification, reports and AI empty-state windows inspected |
| Responsive rendering | Viewer at 920 px in dark mode and quantification at 800/1440 px with wrapping and retained results passed existing layout tests |
| Patch hygiene | `git diff --check` passed; source and audit/evidence remain uncommitted |

Full test output: [validation-tests.txt](ux-evidence/validation-tests.txt).
The five ignored tests require real ProteoWizard/vendor data, opt-in performance
workloads or local pyOpenMS 3.5.0. They were not silently treated as passes.

One earlier full-suite run used a CLI binary compiled before concurrently edited
help/relink source and failed its CLI help test; the final rebuilt suite passed.
Formatting initially encountered a Windows mapped-file write error while builds
were active; the sequential rerun passed. Native automation had transient launch
timeouts, then recovered; final integration/theme evidence comes from an isolated
copy of the built executable so it did not lock Cargo's output binary.

Native numerical input was exercised using keyboard typing/Return. Complete
keyboard-only navigation, actual DPI changes, native window resizing, report
generation through the dialog, live MCP activity with a connected client and the
external OpenMS workflow were not manually completed. Relevant existing tests
exercise core report/MCP/analysis behavior and rendered layouts; this is a narrower
claim than end-to-end human validation of every scientific workflow.

## 9. Prioritized next improvements

1. Finish typed configuration and unify project/artifact handoffs.
2. Add a proposal review model shared by GUI/MCP, with reason, evidence and reversal.
3. Split long quantitative forms into stages while retaining current results.
4. Standardize scientific plots/tables and numerical keyboard selections.
5. Validate contrast, assistive technology, DPI and representative vendor/omics data.

The implemented changes are a concrete improvement to the current workbench,
not a declaration that the full professional-platform objective has been reached.
