# Repository audit and validation report

Date: 2026-10-08, America/Los_Angeles. Baseline commit:
`ff7975aedfdcd894e31dd1e85dc3707d2cdd01f5`. Package: Chromascope 0.3.0.
Scope: repository inventory, source/build/test review and local Cargo baseline;
architecture and roadmap are design artifacts, not production implementation.

## Environment and attribution

Windows PowerShell checkout under `OneDrive - Universiteit Antwerpen`.
`rustc 1.99.0 (b940084d7 2026-09-28)` and
`cargo 1.99.0 (5f94df478 2026-08-27)`; MSVC Windows target.
Manifest edition is 2021 and declared MSRV is 1.88. No toolchain pin exists.
There was no AGENTS.md in the repository file inventory.

Initial `git status --short` contained only these pre-existing deletions:

```text
 D assets/chromascope-0.3.0.png
 D assets/demo.gif
```

They were not restored, removed or staged by this audit. Production Rust,
Cargo.toml, Cargo.lock, tests, fixtures, guides and workflow files remain unchanged.
Formatting/check/Clippy/all-feature tests were started before edits; subsequent
default tests/build used the same unchanged production sources. New work consists
of four engineering documents and narrow Git ignore exceptions. No new production
or test failure is attributable to a source change because none was made.

Every Cargo command emitted the existing host warning that
`C:\Users\s0212777\.cargo\config` is deprecated in favor of `config.toml`.
This is outside the repository and was left unchanged. Concurrent Cargo processes
briefly waited for the build-directory lock; this was not a build failure.

## Executed baseline checks

| Command | Outcome | Scope / actual evidence |
|---|---|---|
| `cargo metadata --locked --no-deps --format-version 1` | PASS, exit 0 | One workspace member, library, desktop binary, MCP feature-gated binary and import integration target. |
| `cargo fmt --all -- --check` | PASS, exit 0 | No formatting diff or diagnostic beyond host config warning. |
| `cargo check --locked --all-targets --all-features` | PASS, exit 0 | Dev profile completed in 1m 13s. All targets including MCP type-check. |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS, exit 0 | Completed in 19.18s, no crate lint warnings. Host Cargo config warning is not a Rust lint. |
| `cargo test --locked --all-features` | PASS, exit 0 | Test build 1m 56s; 209 library tests passed (1.72s), 2 integration tests passed (0.82s), 2 ignored, 6 doctests passed (1.54s). Binary test targets each have zero tests. |
| `cargo test --locked --no-default-features --quiet` | PASS, exit 0 | 197 unit tests (0.23s), 2 integration tests (0.81s), 2 ignored and 6 doctests (1.74s). Current default feature set is empty, so this exercises the ordinary non-MCP path, not a headless core. |
| `cargo build --locked --all-features --bins` | PASS, exit 0 | Normal desktop and MCP executable linking completed in 44.87s, including lock wait. |

Existing failing checks observed: none. Intentional skipped checks: the two
ignored converter tests. New failing checks observed: none.
Build elapsed times include local cache and lock effects; they are not analytical
performance benchmarks. Baseline outputs were observed through the command tool;
no archived full console log is claimed. Commands above reproduce the baseline.

## Tests and coverage limits

Tests cover core errors/exports, XIC validation and filtering, source scan mapping,
TIC/BPC/XIC extraction, integration on synthetic shapes and interpolated
boundaries, moving-average smoothing, decimation, GUI caches and selections,
presets/editor round trips, viewer sessions, batch detection/ambiguity/corrections,
MCP policies/queue/batches/numerical responses/images and software-rendered frames.
The integration suite builds a fake converter with rustc and tests multi-run
success, process failure, cancellation and temporary cleanup.

One included acquisition fixture, `test_file/data_dependent_02.mzML`, supplies
many checks. The profile-array test changes CV tags in this fixture; it does not
demonstrate real high-resolution profile acquisition correctness. The test named
`test_tic_does_not_hold_full_file_in_memory` establishes functional behavior rather
than measuring a process memory ceiling. No large-run RSS, latency or scientific
reference-panel validation was performed.

Skipped integration tests:

- `real_msconvert_roundtrip_preserves_tic_and_scan_mapping`: requires installed
  ProteoWizard msconvert.
- `real_msconvert_targeted_vendor_acquisition_has_spectra_and_xic`: requires
  ProteoWizard and `CHROMASCOPE_TEST_VENDOR_FILE`.

To verify them on a configured machine:

```powershell
cargo test --locked --all-features --test import_msconvert real_msconvert_roundtrip_preserves_tic_and_scan_mapping -- --ignored --exact
$env:CHROMASCOPE_TEST_VENDOR_FILE = 'C:\path\to\licensed-vendor-data.raw'
cargo test --locked --all-features --test import_msconvert real_msconvert_targeted_vendor_acquisition_has_spectra_and_xic -- --ignored --exact
```

Configure msconvert via its existing discovery/settings mechanism. Do not put
proprietary data into Git. No native GUI launch, real MCP stdio visual example,
release build, wasm build, Linux/macOS runtime, Rust 1.88 build, dependency
vulnerability audit or external scientific database/library verification was
performed. This audit makes no claim about those results.

## Repository and dependency observations

- The code is one Cargo package, not an established set of separate engine crates.
  `src/lib.rs` always exports GUI; even no-default-features builds depend on GUI.
- 32 MCP tools exist in `src/mcp.rs`. rmcp schema inputs and `Command` are typed,
  but execution uses the GUI bridge and response JSON. No analytical CLI exists.
- `Cargo.lock` is tracked. Observed locked versions include mzdata 0.63.3,
  egui/eframe 0.33.3, egui_plot 0.34.1, rmcp 3.5.1, Tokio 1.53.2,
  Rayon 1.11.0, image 0.25.9 and TOML 0.8.23. Manifest ranges and locked resolution
  are different concepts; neither was modified.
- Linux quality CI runs format, strict all-feature lint and all-feature tests on
  stable; no default-feature, MSRV or OS quality matrix. Release uploads desktop
  binaries on Linux/macOS/Windows, without an MCP binary upload configuration.
- Release optimization is `z`, LTO, one codegen unit, panic abort, stripped symbols
  and disabled overflow checks. Debug dependencies are optimized at level 2.
  Throughput and release panic behavior remain unmeasured.
- `.cargo/config.toml` contains a wasm web-sys flag; PWA assets still identify an
  egui template and refer to absent template artifacts. No active web entrypoint
  or wasm validation establishes web support.
- Existing tracked docs/MCP.md and docs/USER_GUIDE.md survive the broad original
  docs ignore rule because they were already tracked. That rule blocked new
  engineering docs. The audit changed it to `docs/*` plus four explicit exceptions.
- Cargo package include patterns cover `docs/**`, source, fonts, presets and
  examples, but not the mzML fixture or integration-test fake-converter source.
  Package-source tests may therefore differ from checkout tests; verify packaged
  sources before distributing them as a reproducible test bundle.
- GUI module/parser comments describe some older fields or broader format support
  than verified paths. Runtime source and test evidence take precedence.

## Concrete engineering risks and follow-up validation

These findings are source-observed risks, not newly failing baseline tests.

| Finding and location | Consequence | Follow-up gate |
|---|---|---|
| XIC collects a Vec of matching spectra before `into_par_iter`, parser.rs `get_xic` | Memory grows with selected scans/arrays; concurrent traces compound it. | P3b bounded chunks, RSS measurements and equivalent output. |
| `quant.rs` result channel is unbounded; cancellation checked before each extraction | Full traces accumulate; one long extraction delays cancellation. | P3a/P3b byte budgets and cancel during a large extraction. |
| Parser worker opens and some spectrum lookups on GUI thread; MCP waits on owner thread reply | UI/MCP latency depends on reader operations; dispatch has no general deadline. | P3a/P4b responsiveness, disconnect and deadline tests. |
| Session FileId is usize; analytes/results matched using names and source/sample strings | Rename/relink/reorder can break durable association; session identity is not persistent. | P1b ID stability and migration mappings. |
| Viewer/batch/session saves use direct fs::write | Interrupted writes may lose the last usable save. Preset editor already uses temp file + sync + persist and is a reusable pattern. | P2a transactional commit and interrupted-write restore. |
| Converted runs are temporary with lease lifetime; sessions record source/run paths | Reopen depends on reimport/relink and converter behavior, not a durable content artifact. | P2b multi-run restore after temporary cleanup. |
| `DataBounds::validate_mass` compares without a finite check; `XicParams` derives Deserialize | NaN can escape mass construction comparisons; deserialization bypasses constructor. Some frontends guard values, but no universal engine boundary does. | P1b constructor/deserialize finite-value tests. |
| `validate_smoothing` casts `(scan_count / 10).max(3)` to u8 | Large counts can wrap the recommended limit and reject otherwise sensible windows. | P1a/P1b large-count boundary characterization before correction. |
| `prepare_chromatogram_for_plot` indexes intensity by RT length; export spectrum zips arrays | Mismatched public vectors can panic or silently truncate. | P1b lengths validated before processing/export. |
| Integration assumes finite sorted data and primarily validates start < end; callers differ in range validation | Core callers can bypass frontend checks; outside-range interpolation clamps endpoints. | P1a freeze behavior, P1b explicit operation range contract. |
| Bounds take first/last RT and sample first/last spectra if windows absent | Unsorted acquisitions or internal extreme masses can mislead bounds/validation. | P9 unsorted/no-window/heterogeneous acquisitions. |
| XIC uses partition_point on m/z arrays; TIC/BPC use metadata values with all-zero fallback | Sorted mass arrays assumed; mixed missing CV metadata may not trigger fallback when some scans are nonzero. | P1a/P9 mixed metadata and unsorted-array fixtures; explicit validation/error policy. |
| Duplicate RT averaged before numeric processing; precursor filter uses fixed 0.01 Da and metadata conventions | Scientific semantics need explicit versioning; changes affect areas/acquisition selection. | P1a reference tests, P5 versioned extraction policies. |
| MCP output checks target existence before writing | No-overwrite and path policy need concurrent filesystem/symlink race validation; sequential tests alone do not prove atomicity. | P4b create-new export and malicious/concurrent path tests. |
| Release panic abort; no immutable actor/review DAG | Worker panic cannot be recovered in process; result history lacks full audit lineage. | P3/P2b failure and provenance gates. |

Calibration, concentration, QC engine, MS/MS identification, untargeted features,
alignment/matrices, statistics, external analytical adapters beyond conversion,
unified projects, durable IDs, general jobs and review DAG are NOT_STARTED under
the target design. Existing integration/review tools are foundations, not proof
that these larger capabilities have been completed.

## Audit change validation

P0 documentation status: VERIFIED within the local audit scope. All four files
exist, their relative Markdown document links resolve, their status vocabulary
matches the roadmap, and Git reports them as untracked additions ready to stage
rather than ignored files. `git diff --check` passed. Final scope checks confirm
no change to tracked Rust, Cargo manifests/lockfile, tests, fixtures or workflows;
only `.gitignore` changed alongside the four new documents. The two asset
deletions remain exactly as observed at entry. No staging or commit was performed.

Only minimal documentation infrastructure is introduced. Existing Cargo tests
and CI already provide the baseline; no redundant test runner or implementation-
mirroring tests were added. No algorithm rewrites, dependencies, CI changes, new
fixtures or production feature implementations were introduced.

## Foundation implementation validation — 2026-10-08

Uncommitted checkout based on the same baseline; Windows rustc 1.99.0. Existing
`.gitignore` edits, four untracked engineering documents and two deleted assets
were present at entry and preserved. No staging, commit, push, publishing or
user-data deletion occurred. Production changes are the shared modules, feature
boundaries, headless runners, persistence, validation and streaming described in
the architecture update. Cargo.lock adds sha2 0.10.9 and enables existing UUID
features; no existing analytical-library version upgrade was requested.

Executed final all-feature checks:

| Command | Observed result |
|---|---|
| `cargo fmt --all -- --check` | PASS after formatting |
| `cargo check --locked --all-targets --all-features` | PASS, 4.32s; later all-target Clippy also compiled restore changes |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS, 2.28s after restore addition |
| `cargo test --locked --all-features --quiet` | PASS: 210 library tests, 10 new engine integration tests, 2 converter integration tests, 6 doctests; 2 real-converter tests ignored |
| `cargo tree --locked --no-default-features -e normal --prefix none` inspected for egui/eframe/rfd/image/egui_plot | No GUI dependency matches |

New reference coverage: analytically calculated triangular excess above a sloped
chord gives exactly 4 intensity*minutes (absolute tolerance 1e-6); TIC/BPC/XIC
engine/legacy/CLI f64 values and source mappings compare exactly. Headless MCP is
exercised through actual stdio initialize/tools/list/tools/call, with exact typed
f64 processed and f32 raw arrays and a denied out-of-root request. Quantification
matches the extracted legacy detector/status/unsmoothed trace. Tests also cover
scan native identity/units, CLI project-create/run/inspect, stable project/dataset/
method identities, immutable previous revisions, source relinking/checksums,
corrupt result rejection, optimistic stale writes, repeated read-only legacy TOML
method import, missing source, unsupported future schema, invalid parameters,
nonfinite/unsorted traces, deserialized XIC validation, large-count smoothing,
pre-cancelled scheduler work and cancellation/scan-budget checks during work units.

Transient failures were fixed without relaxing existing tests: extraction
validation initially replaced InvalidSmoothingWindow with a generic error; the
original variant is restored. Exact CLI equivalence exposed serde_json f64 parsing
rounding; float_roundtrip is enabled. The MCP test originally compared untyped
JSON Values (f64) against in-memory raw arrays (f32); it now deserializes the
actual declared Output DTO and requires exact values for both raw f32 and processed
f64 arrays, mappings and units. No existing scientific test was weakened.

Limitations: these checks do not establish native GUI interaction, real vendor
conversion, MSRV/Linux/macOS, OneDrive crash durability, bounded process RSS,
decoded-spectrum memory ceilings, <=2s cancellation latency, backup/restore,
durable/restartable jobs, legacy viewer/batch migrations or full project GUI
restore. The new synchronous MCP runner has bounded admission/response sizes,
but does not expose polling/cancel tools or artifact paging. Progress is scan work,
not a measured ETA. Numerical artifacts remain JSON; full memory/disk-budget and
binary-spill acceptance gates are open. Calibration/QC/identification/untargeted/
statistics/reporting phases remain unimplemented. The two external-converter
prerequisites and pre-existing global Cargo config warning remain unchanged.

Additional completed checks in the same final checkout:

- `cargo test --locked --no-default-features --quiet`: PASS, 126 core library
  tests, 8 engine integration tests, 2 converter tests and 6 doctests; 2 ignored.
- `cargo check --locked --no-default-features --features mcp-headless --all-targets`:
  PASS, 26.81s; the headless MCP feature compiles independently of GUI.
- `cargo build --locked --all-features --bins`: PASS, 2.02s, desktop, desktop MCP,
  CLI and engine MCP executables linked.
- `git diff --check`: PASS. Existing asset deletions and `.gitignore` work retained.

The lockfile format was regenerated from 3 to 4 by the current Cargo; no existing
locked package versions changed. MSRV remains untested, as above.

Historical restore now has an explicit `Project::restore_revision` / CLI
`project-restore` path. Its reference test checks new revision allocation,
restored dataset identity, retained originals and stale restore rejection. Strict
Clippy was rerun after this addition (PASS). Earlier core-only counts (8 engine
integration) precede the restore test; the final all-feature count is 10.

`cargo test --locked --no-default-features --features mcp-headless --test engine
--quiet` passed all 9 then-current engine tests, including actual stdio numerical
and permission checks, with GUI disabled. After historical restore was added,
all-feature tests were rerun: 210 library + 10 engine integration + 2 converter +
6 doctests passed, 2 vendor tests ignored. Final formatting and diff checks passed.

## Advanced chromatography validation - 2026-10-08

Uncommitted Windows checkout, Rust 1.99.0; the prior uncommitted foundation,
.gitignore edits and two asset deletions were preserved. No staging, commit,
push, publication, raw-data deletion or package upgrade. This delivery adds
src/chromatography.rs, typed operations/outputs, GUI adapters and optional batch
analysis persistence, export support, synthetic/reference tests and documentation.
Cargo.toml/Cargo.lock changes visible in Git belong to the pre-existing foundation;
this delivery did not add runtime dependencies or modify those package choices.

Observed final all-feature results:

| Command | Observed result |
|---|---|
| `cargo fmt --all -- --check` | PASS after formatting the final width-gate edit |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | PASS; final all-target compilation included GUI interaction test and batch configuration protection |
| `cargo check --locked --all-targets --all-features` | PASS; later strict all-target Clippy/build/tests also compile the final refinements |
| `cargo build --locked --all-features --bins` | PASS; GUI, GUI MCP, CLI and engine MCP linked |
| `cargo test --locked --all-features --quiet` | PASS in final source: 212 library, 12 chromatography integration/reference, 10 engine integration, 2 converter integration, 6 doctests; 2 real converter tests ignored (242 passed, 2 ignored) |
| `cargo test --locked --no-default-features --quiet` | PASS: 126 library, 12 chromatography, 9 engine, 2 converter, 6 doctests; 2 ignored. This run preceded the final noise/SNR overflow guards; final GUI-free MCP validation is recorded below. |
| `git diff --check` | PASS; existing host Git LF/CRLF notices and Cargo config deprecation warning remain |

Independent references: tests/reference/generate_chromatography.py uses NumPy
2.3.3 and SciPy 1.16.2 already available on the host, deterministic seed 143,
synthetic CC0 signals and no Chromascope import. It generates uniform SG interp
arrays, independent sparse AsLS solves, irregular-grid NumPy polynomial fits,
SciPy peak apex and interpolated-boundary NumPy trapezoids. These are locked in
tests/reference/chromatography.json and tested offline by Rust; Python/SciPy is
not a runtime requirement and no scientific production result is hardcoded.
Tolerances fixed in tests: SG/irregular polynomial reference absolute or relative
1e-10; independent AsLS baseline/area absolute or relative 1e-7. Baseline uses a
different sparse solver, not a reimplementation of the Rust banded solver.
Gaussian full-interval area matches H*sigma*sqrt(2*pi) to 1e-8; FWHM matches
2*sqrt(2*ln(2))*sigma within 3e-5 minutes at 0.005-minute sampling. Asymmetric
causal gamma peak x^2*exp(-x/tau) matches its exact mode and 2*tau^3 integral
within 1e-6. Observed valley-split overlap areas sum to the independently
integrated union within 1e-10. Between-scan endpoints have an exact triangular
area reference; clipped widths are null. Discrete Gaussian/SciPy apices match.
Noise sigma is compared with 20,000 independently generated Box-Muller Gaussian
samples (sigma 3, tolerance 0.12); width gates, sloped drift and rolling-quantile
baseline behavior are covered separately. Tests include irregular RT, polynomial
edges, null samples, acquisition gaps, empty/missing results, negative intensity,
nonfinite/duplicate RT, invalid SG/AsLS parameters, overlapping manual intervals,
reference RT shifts, review, stale revision, undo and corruption/replay rejection.

Transport/persistence evidence: actual CLI processing matches direct engine
analysis arrays exactly; batch traces roundtrip; export-peaks writes lossless CSV
and rejects a second write to the same path. Both successful and stale/failed
CLI previews with an explicit project leave revision, sources and results
unchanged. Project::add_result independently rejects a preview. Engine peak-table
export is tested against the same CSV helper. Existing stdio MCP test now also
compares advanced processing exactly against engine results and exercises manual
preview, while retaining existing extraction/units and denied-root checks.
GUI batch validation replays advanced history and rejects changed areas/signals.
Old batches omit the optional analysis field and retain their previous behavior.
Finite negative input intensities are now admitted by batch validation as required;
finite/sorted RT and legacy measured-peak validation remain in place.

GUI evidence: an egui software-input test opens the advanced panel and clicks
Preview processing / Apply / Preview manual bounds / Apply / Preview accept /
Apply. It asserts no saved mutation before apply and the recorded reviewed
revision afterward. It also processes an unprocessed second row after changing a
hidden draft, asserting the displayed saved configuration is used. Optional
CHROMASCOPE_QUANT_PREVIEW=1 generated target/chromatography-review.png using the
existing CPU renderer; that image was visually inspected for controls, stage
plot, boundaries, units and peak summary. Existing small/large/light/dark GUI
render and quantification tests still pass. This is not a native desktop/file-
dialog smoke test. Export through a native dialog is therefore not claimed
visually verified, although its create-new writer compiles and equivalent CLI
behavior is exercised.

Reference-driven fixes made without weakening existing tests: height-only SNR
initially allowed local ripples on residual drift, so detection now also gates
prominence by noise. Half-height crossing interpolation uses adjacent interval
samples and reports null when clipped, rather than inventing a width from the
whole interval. Final overlap partitions are also width-gated. Numeric overflow
in area/noise/SNR fails structurally. Saved GUI configuration comes from the
selected analysis and controls batch processing, avoiding stale hidden-draft
parameters. A transient Windows text-encoding problem during an edit was fixed;
final UTF-8 source compiles, formats and passes strict lint. No test assertion or
scientific tolerance in the existing suite was relaxed, and ignored converter
prerequisites were not removed.

Scope limits and prerequisites: large-trace GUI processing is synchronous;
cooperative cancellation is currently checked between engine traces, not inside
new numerical loops. Input/transport caps do not establish bounded RSS or a
latency target. Gaussian/EMG component fitting, unresolved mixture deconvolution,
automatic reference identity/alignment and parameter-change/review UUID/timestamp
DAGs remain unimplemented. The noise estimator assumes local smoothness and
independent approximately Gaussian noise; coarse, short or correlated signals
require review. SG short-segment bypass, clipped widths, signed signals and
missing/gap segmentation are explicitly visible. Native interactions, MSRV,
Linux/macOS, real instrument reference panels and cloud-sync recovery are not
certified. A legal instrument dataset with known independent reference bounds/
areas and a scientist-defined protocol is still needed for real-data scientific
acceptance. Real converter verification still needs msconvert and licensed
vendor data; those two tests remain ignored. These prerequisites do not block
the delivered synthetic/shared-engine workflow, but block wider certification.

Final GUI-free validation in the final numerical source:
`cargo test --locked --no-default-features --features mcp-headless --quiet` PASS:
126 library + 12 chromatography + 10 engine + 2 converter + 6 doctests;
2 vendor tests ignored (156 passed, 2 ignored). This includes actual stdio
advanced processing/manual preview with GUI disabled and the final numeric
overflow checks. Final all-feature count remains 242 passed, 2 ignored.


## Targeted small-molecule quantification validation - 2026-10-08

Checkout remains uncommitted at baseline ff7975aedfdcd894e31dd1e85dc3707d2cdd01f5,
on Windows with rustc 1.99.0 / cargo 1.99.0. Initial source/docs/foundation edits
and deleted assets were present before this task and preserved. New production
changes add targeted DTOs/kernel, engine operations, CLI exports, nested-source
MCP checks, project multi-source result checks and independent GUI controls.
Legacy extraction/detection/integration algorithms and old interfaces remain.
New dependencies are nalgebra 0.33.3 (std only) and dev base64 0.22 (already locked).

Independent reference generator: tests/reference/generate_targeted.py, NumPy
2.3.3. The 27 locked cases cross degree 1/2/3, free/zero/fixed intercept, and
unweighted/1/x/1/x² fits. NumPy LAPACK lstsq uses unscaled polynomial columns,
independently of production's scaled nalgebra SVD matrix. NumPy polynomial roots
provide valid-root predictions and per-standard back-calculations/accuracy.
Rust comparisons cover every coefficient (converted to raw concentration powers),
residual, R², weighted RMSE, residual standard error, inverse estimate and
back-calculated accuracy at tolerance 1e-9*(1+abs(reference)). References are
scientific test data, never production output tables. Python is not a runtime
dependency. Analytic quadratic/cubic cases assert ambiguous roots are rejected;
meaningful below/above-domain predictions and expanded ranges are rejected.

Twelve targeted integration/reference tests cover response ratios, dilution,
concentration units, below-detection/LOQ/range/above-range/missing/failed/ambiguous
states, signed negative areas, zero IS, IS area bounds and missingness, qualifier
ratios/coelution, calibration exclusion reasons, invalid sample designs, missing
standards, invalid weights/levels, empty precision groups, n-1 SD and CV against
closed-form calculations, stale review, reject/restore, exact JSON roundtrip and
corrupt areas/traces/results. Cancellation/missing sources produce structured
errors. Only machine boundary roundoff has a disclosed flag; a 1e-8 outside-range
response is still rejected rather than clamped.

Raw acceptance fixtures are generated mzML files with independent triangular
peak areas: masses 100/200/300 for quantifier/qualifier/IS; 5 scans; known levels
1/2/4/8, QCs 3/3.1, unknown 5 and a zero-signal blank. The tests write binary
arrays into actual files, then use mzdata and the preserved XIC/chord kernel.
Quantifier area at level 1 is 10 intensity*minutes; IS area 100; unknown response
0.5 and injected concentration 5 ng/mL; dilution 2 yields 10 ng/mL. Engine, actual
CLI subprocess and actual MCP stdio agree exactly. MCP review/export are executed,
and a batch with a nested sample outside allowed roots is rejected. Raw bytes
remain unchanged through extraction/review. No third-party instrument data or
hardcoded scientific production results were introduced.

Project tests use actual CLI multi-source registration, immutable commit/reopen,
result checksum reads and a subsequent review commit. All eight acquisition
sources are registered; original revision/result remains. A changed raw source
prevents a new commit, while historical numerical artifacts remain inspectable.
CLI concentration export is lossless and refuses an existing output file; engine
MCP export returns the same table text. Calibration/residual CSV is separately
available in engine output, GUI and CLI. JSON retains complete inputs and reviews.

GUI unit acceptance uses software egui pointer events to open the targeted panel,
run raw mzML quantification on a background worker, select the unknown and click
accept/reject/accept with an explicit reason. It compares numerical rows to the
shared kernel, asserts all three review events and unchanged original traces,
and replays the final batch. CHROMASCOPE_TARGETED_PREVIEW=1 generated
`target/targeted-review.png`; it was visually inspected for sample states/units,
review controls, quantifier/qualifier traces, fit/residual plots and precision.
Animation is disabled in the test for deterministic pointer geometry. Native
OS dialogs/desktop interaction are not claimed tested.

Observed fixes during implementation: serde's nested enum buffering does not
support the initially chosen u128 review timestamp; new targeted timestamps use
checked u64 milliseconds. Floating SVD roundoff initially misclassified an exact
boundary standard; a response-scaled 64-epsilon boundary tolerance now emits an
explicit boundary_roundoff flag, without meaningful extrapolation. A blank test
input initially lay below its own contamination threshold; the input was corrected
to a known contaminated blank, preserving the threshold/assertion. A temporary
encoding error in a newly created GUI file was repaired in UTF-8 before validation.
No existing test assertions, tolerances or ignored vendor prerequisites were
weakened or removed.

Executed checks (final source):

- `python tests/reference/generate_targeted.py`: PASS; regeneration and independent
  back-calculated accuracy comparisons pass offline in Rust.
- `cargo fmt -- --check`: PASS.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: PASS.
- `cargo check --locked --all-targets --all-features`: PASS during adapter integration;
  final strict Clippy and test compilation also cover all targets/features.
- `cargo test --locked --all-features --quiet`: PASS, 213 library + 12 chromatography
  + 10 engine + 2 converter + 12 targeted + 6 doctests = 255 passed; 2 real-vendor
  converter tests remain ignored.
- `cargo test --locked --no-default-features --features mcp-headless --quiet`: PASS,
  126 library + 12 chromatography + 10 engine + 2 converter + 12 targeted + 6 doctests
  = 168 passed; the same 2 vendor tests ignored. Actual targeted MCP stdio runs
  with GUI disabled.
- `cargo build --locked --all-features --bins`: PASS; desktop, legacy MCP, CLI and
  analytical MCP executable builds.
- `git diff --check`: PASS. Host Cargo config deprecation and Git CRLF notices are
  existing environment warnings, not failed checks.

The checked-in editable example was executed through the CLI: request succeeds,
source evidence is extracted, and the analyte is explicitly failed because no
real standard levels are supplied. No concentration is invented. This example
is not the known-level reference test and is not claimed quantitative validation.

Remaining gates: legal real-instrument reference panels and a scientist-approved
assay protocol; native dialogs, Rust 1.88/Linux/macOS; OneDrive recovery; measured
RSS/cancellation latency, binary storage and partial-batch recovery. New raw runs
reopen/hash sources per target; batch point caps do not prevent a single large
extraction allocation. Missing files/cancellation fail the new batch rather than
persisting partial results. Detailed replay/exports may block GUI frames for
large saved batches. Direct targeted boundary corrections/refit revisions, CSV
sample-sheet import, dedicated qualifier-add/remove controls, automatic LOD/LOQ,
carryover, confidence intervals/uncertainty and enforced assay acceptance policy
are pending. Thresholds are saved review diagnostics; explicit human review is
not regulatory certification. Full limitations/next steps are in architecture,
roadmap and handoff. No commit, push, publishing or user raw-file mutation.

## Batch QC and method-validation validation - 2026-10-08

Environment: Windows, rustc 1.99.0 (b940084d7 2026-09-28), Cargo 1.99.0,
Python 3.13.7. Baseline user changes and both deleted assets were preserved.
No dependency or lockfile change was needed for this QC stage; weighted fits
reuse the existing nalgebra dependency. No user raw files were changed; raw
integration fixtures are synthetic acquisitions created in test temporary dirs.

Scientific/reference evidence:

- `python tests/reference/generate_qc.py`: PASS. Python stdlib statistics and
  exact Fraction arithmetic generate independent examples for all 24 metrics,
  including the noiseless y=2x+1 calibration. Checked-in fixture is
  `tests/reference/qc.json`; editable synthetic engine example is
  `examples/qc-request.json`. These are input/reference fixtures, never production
  substitutions or results for user datasets.
- `tests/qc.rs`: 25 tests, including all 24 reference calculations, 27 independent
  NumPy calibration fixtures * three configured metrics (R2, RMSE, back-calculated
  accuracy), four known required failures with retained reasons/observations,
  opposite replicate deviations hidden by a passing mean, missing evidence,
  zero denominators, insufficient n, incompatible units, duplicate order/ID,
  carryover immediate predecessor and batch boundaries, preparation/nominal
  design requirements, heterogeneous precision levels, optional gating, report
  replay/tampering, UUID/JSON roundtrip, reversible review/stale revisions,
  generated raw mzML -> original targeted evidence -> QC, actual CLI/export/
  no-overwrite, actual MCP stdio evaluation/review, and project verification of
  all sources without replacing existing revisions/results. All pass.
- Existing NumPy fixtures remain unmodified; the QC fit checks use the same
  1e-9*(1+abs(reference)) tolerance as existing targeted comparisons. Independent
  QC statistics/ratios/slopes use 1e-10*(1+abs(reference)). No existing tests,
  thresholds or reference tolerances were weakened.
- `gui::qc::tests::qc_dashboard_evaluation_and_review_preserve_failed_status`:
  PASS. Real synthetic mzML batch is retained, egui clicks evaluate the study,
  inspect a failed blank and acknowledge it without changing failed acceptance.
  Historical evaluation remains unreviewed and numeric evidence replays.
  `CHROMASCOPE_QC_PREVIEW=1` renders `target/qc-review.png`; the rendered controls,
  failing decision and acceptance/injection-order plot were visually inspected.
  This is software egui evidence, not native dialog or OS testing.

Final executed check sequence:

- `cargo fmt -- --check`: PASS.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: PASS.
- `cargo check --locked --all-targets --all-features`: PASS; strict Clippy also
  compiled all targets/features after final source updates.
- `cargo test --locked --all-features --quiet`: PASS, 214 library + 12
  chromatography + 10 engine + 2 converter + 25 QC + 12 targeted + 6 doctests =
  281 passed; the existing two real-vendor converter tests remain ignored.
- `cargo test --locked --no-default-features --features mcp-headless --quiet`:
  PASS, 126 library + the same integrations/doctests = 193 passed; two vendor
  tests ignored. Actual QC MCP stdio runs with GUI disabled.
- `cargo build --locked --all-features --bins`: PASS for desktop, legacy GUI MCP,
  CLI and headless analytical MCP.
- `git diff --check`: PASS. Existing host Cargo config deprecation and Git CRLF
  notices are environment warnings. During development, strict lint caught a
  newly typed plot-radius f32 fallback and the egui test caught an unbounded JSON
  editor pushing controls off-screen; both were corrected before final checks.

The shared module exposes calculations, exact input evidence, configurable
inclusive thresholds and required/optional acceptance through all analytical
surfaces. Report and batch acceptance records replay; review acknowledges/reopens
without overriding failed acceptance. CSV carries actual underlying observations,
rule configuration, calibration fits and descriptive intermediates; JSON carries
full targeted raw evidence and complete history. Project commits verify every
underlying targeted raw-source hash. Saved GUI history preserves old evaluations.

Limitations/gates: synthetic examples establish local numeric/software behavior,
not assay validity or regulatory certification. Scientists must supply suitable
multi-run/lot/preparation designs and assay-specific thresholds. External studies
must include the complete expected injection/observation grid, source identities,
units and preparation labels. Missing raw centroid mass-error observations remain
null; required mass checks are indeterminate. Method precision is within-run or
pooled descriptive CV, without variance-component ANOVA. No inferred missing
injections, paired/IS-normalized matrix-factor study, automatic endogenous
subtraction, trend p-values, uncertainty or Westgard run patterns are fabricated.
Recovery and matrix metrics use disjoint preparation groups at matched positive
levels and ratios of means; per-lot acceptance needs per-lot rules. GUI computation
and replay remain synchronous; transport/observation caps are not RSS/latency
certification. Real instrument panels, native dialogs, Rust 1.88, Linux/macOS,
OneDrive crash recovery, worker cancellation within QC rules and durable partial
job persistence remain separate gates. No commit, push, publishing or staging.
## Advanced spectra / compound identification validation - 2026-10-08

Validation is local Windows Rust 1.99.0 / Cargo 1.99.0 evidence on the existing
uncommitted workspace, not a claim of Rust 1.88, cross-platform or assay validation.
The initial dirty files/documents and deleted assets remain preserved. The actual
shared engine, parser, CLI, MCP, project and desktop adaptations are described in
the final architecture section. No placeholder scientific results or weakened
existing reference tolerances were introduced.

### Scientific and workflow evidence

- Three new core unit tests: zero signal/background preserves a negative signed
  peak and Missing positive-signal state; finite subtraction overflow and invalid
  unused centroid SNR reject; inconsistent known/missing background precursor
  metadata rejects. The latter test caught a missing presence comparison, fixed
  before final full-suite validation.
- `tests/spectral.rs`: 25 passing integration/reference tests. Exact/partial/
  disjoint cosine and one-to-one fragment assignment; ppm versus Da behavior;
  negative peaks excluded from similarity; raw-preserving averages and signed
  subtraction; incompatible averaging; mzsignal profile Gaussian reference;
  strict MSP/MGF/MassBank import, metadata/license/index/replay; malformed peak
  counts and incomplete MGF; unknown/probable/class/confirmed evidence constraints,
  stale/reversal history; tied isobars and incompatible polarity/adduct/instrument/
  energy exclusions; explicitly allowed missing metadata flags; one-ion false
  matches, precursor mismatches and declared DIA; corrupt replay, source-linked
  XIC and project history; actual CLI, create-new CSV and MCP/root policy.
- Unmodified CC BY MassBank records MSBNK-Antwerp_Univ-AN111301 (26 peaks, 5 eV)
  and AN111302 (21 peaks, 10 eV), release 2026.03, both Tributyl acetylcitrate.
  Attribution, original license/copyright, raw bytes and metadata are retained;
  source URLs and SHA256 are in `tests/reference/spectral_sources.json`.
  A self-match stays unannotated/unknown even though the library contains its
  reference-standard confidence comment; exact energy filtering excludes the
  other energy when configured to zero tolerance. This is ONE compound, not a
  broad real-world positive/negative identification panel.
- Independent `tests/reference/generate_spectral.py` uses stdlib dense dot
  products for unique-mass real cross-energy assignments at 0/1/10% filters and
  exhaustive Decimal CHNO atom-count enumeration. The cosine references are
  0.9093622549721259, 0.9092429627473966, 0.8836983698213137 with 18/16/6 matches;
  fixed comparison tolerance is 1e-12. Formula masses compare at 1e-10 Da;
  candidate sets and DBE compare exactly. Production solves H inside the mass
  interval; the independent reference enumerates every H count.
- Gaussian profile centroid at 100 m/z, sigma 0.01 and amplitude 1000 is recovered
  within 1e-5 m/z and 0.01 intensity. Synthetic formulas recover the conventional
  caffeine monoisotopic mass and retain competing H/Na neutral formulas at a
  high enough mass for 5 ppm ambiguity. No spectral result is declared caffeine
  from the formula test. Carbon isotope charge spacing/ratio and carbon binomial
  / sulfur M+2 nominal probability references pass fixed assertions.
- Dense assignment exceeds its one-million-pair budget with a structured error.
  An ambiguous two-peak example gives greedy 100/181 versus optimal 180/181;
  this limitation is explicitly tested, not hidden by labelling the algorithm
  optimal. No estimated identification probability or FDR is generated.
- Actual CLI import/search/annotation and `export-spectral` use the shared engine;
  export cannot replace an existing file. Actual headless MCP stdio imports a
  library, searches, appends an Unknown annotation and rejects outside-root paths.
  Tagged JSON annotation timestamps use u64: an earlier MCP annotation failure
  motivated that correction and the explicit saved-response roundtrip test.
- `gui::spectral::tests::desktop_search_overlay_and_annotation_keep_original_history`
  passes software egui clicks through prepare/search/execute/candidate overlay /
  reasoned Unknown annotation; original search remains unreviewed and all saved
  responses replay after JSON roundtrip. CHROMASCOPE_SPECTRAL_PREVIEW=1 produced
  `target/spectral-review.png`, visually inspected after final GUI updates. Controls,
  candidate mirror plot, metadata, current confidence and history are visible.
  This is software layout/interaction evidence, not native OS/dialog testing.
- The local `examples/spectral_workflow.py` helper was run against the real CLI,
  declared MassBank fixture source and saved synthetic processing response.
  Incompatible precursor masses yielded zero candidates and Unknown status;
  four import/search request/response files were retained in a new target
  evidence directory. Python helper and generator syntax compilation passed.

### Final executed checks

- `cargo fmt -- --check`: PASS.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: PASS.
- `cargo check --locked --all-targets --all-features`: PASS.
- `cargo test --locked --all-features --quiet`: PASS, 218 library + 12
  chromatography + 10 engine + 2 converter + 25 QC + 25 spectral + 12 targeted +
  6 doctests = **310 passed**, with the same two pre-existing vendor tests ignored.
  The final repetition set CHROMASCOPE_SPECTRAL_PREVIEW=1 to render final GUI state.
- `cargo test --locked --no-default-features --features mcp-headless --quiet`:
  PASS, 129 library + the same integrations/doctests = **221 passed**; two vendor
  tests ignored. This includes actual GUI-free spectral MCP stdio.
- `git diff --check`: PASS before the final documentation append; the final
  documentation whitespace check is recorded below. Cargo config deprecation
  and Git line-ending notices are host warnings, not scientific validation.

### Remaining acceptance gates

Software users can inspect retained unknown spectra, search/import compatible
libraries, compare candidates and append defensible evidence annotations across
GUI/CLI/MCP. This does not establish confidence calibration on real unknowns.
External evidence is reviewer-attested, not automatically authenticated. Legal
scientist-curated multi-compound negative/isomer/adduct panels, instrument/energy
transfer panels, authentic-standard evidence and method-specific confidence
protocols are still required. Nominal isotope models assume representative
natural abundance and integrate coarse bins; no fine structure, labelling,
coelution or precursor purity is certified. Formula bounds are closed-shell
CHNOPS, not exhaustive chemical space. Declared DIA has no supported deconvolution.

No decoy/FDR, learned/modified-cosine ranking, remote database identification,
fragment formula assignment, automatic adduct grouping, or retention-time
comparability is fabricated. Library replay reparses the retained text; large
library disk indexing/cache is pending. Array/pair/search caps do not establish
RSS or latency limits; cancellation is not inside numeric loops. Desktop compute
uses a worker, while file load/history replay/export can pause frames. Native
OS/dialogs, MSRV/Linux/macOS, durable jobs and OneDrive recovery remain the earlier
separate gates. Existing user work, source files and prior artifacts were not
reset, overwritten, deleted, staged, committed, pushed or published.
Final follow-through: `cargo build --locked --all-features --bins` PASS for all four
executables (desktop, legacy GUI MCP, CLI and headless analytical MCP). Final
`git diff --check` PASS; both downloaded MassBank record hashes still match their
provenance manifest. Documentation includes scope, observed checks, limits and
next gates. The implementation remains uncommitted; initial user work is retained.

## Untargeted LC-HRMS validation — 2026-10-08

This section describes observed local software/reference validation, not real
instrument performance or a claim that all P7a/P9 scientific gates are verified.
The initial uncommitted repository state and previously deleted assets remain.
No user raw data were written, replaced or deleted; no staging/commit/push/publish.

### Reference design and independent checks

`tests/reference/generate_untargeted.py` generates six runs in each polarity:
two study samples, one blank and three pooled QCs; each run has 820 centroid MS1
scans at 0.5 s cadence and one associated MS/MS scan. Eight Gaussian isotope
envelopes have independently declared m/z, RT, amplitude and affine drift.
One study sample omits an envelope and contains a narrower envelope; blanks
contain two contaminating envelopes; QC amplitudes vary deliberately for one
feature. Intermittent high-intensity ions vary m/z between scans and are known
false trace candidates. Synthetic data require no external license and are
redistributable test inputs; this is not an instrument mixture reference.

The explicit sparse-data method sets minimum trace sample rate 0.1 and noise
100 intensity units. Initial runs with the OpenMS default sample-rate criterion
missed many sparse traces; this limitation is retained, and sample rate is an
explicit recorded parameter rather than a hidden fallback. The full Gaussian
area is ground truth for the complete analytic signal; matrix areas deliberately
integrate only the retained feature hull window. Reference tests independently
integrate that known continuous Gaussian within each actual declared window at
4000 midpoint steps, requiring relative agreement <0.002 (0.2%). This tests the
specified window-area quantity, not an assertion that truncated tails vanish.
Original OpenMS FWHM area is separately retained and is not the matrix area.

The opt-in Rust reference test checks exactly eight consensus features in both
polarities (no extra intermittent false ions), m/z error <1e-5 Th, consensus RT
error <0.5 s and per-cell aligned RT error <0.6 s against known signal positions.
It checks absent signal remains null/below_threshold, a narrow feature is
explicitly gap_filled, isotope grouping retains at least two trace masses,
blank/QC failure flags match the designed failures, and MS/MS scan evidence is
linked. These cadence-specific criteria are fixed software reference assertions,
not a general instrument accuracy or false-discovery-rate estimate.

The same reference exercise tests exact wide-CSV warm-cache reproducibility,
source/parameter/version checkpoint lineage, checksum corruption rejection,
current scan-budget enforcement on warm checkpoints, gap_fill_disabled and
outside_acquisition classifications, persisted source-registered project results,
actual CLI execution and create-new/no-overwrite exports, actual MCP asynchronous
start/status/evidence paging/summary paging/cancellation and outside-root rejection.
A live local Python adapter is cancelled with a <2 s test bound, retaining a
cancelled outcome. Native console Ctrl+C dispatch and large-file decoding latency
are not established by that test. Checkpoint restart is per completed sample;
job IDs and a partial final matrix do not survive server restart.

`tests/reference/test_openms_adapter.py` independently declares proton/sodium
and deprotonated/chloride exact-mass pairs. It verifies actual positive H/Na and
negative H/Cl grouping, a non-coeluting false pair, and preservation of original
ion masses/charges/intensities. Tests caught and corrected signed charge ranges
and normalized negative-adduct priors; OpenMS negative_mode alone is insufficient.
Other Python references check inclusive m/z window bounds, triangle area,
consecutive-signal support versus isolated false gap spikes, and indeterminate
blank/QC evidence. OpenMS can print a charge-ladder warning for the small designed
negative pair; that is retained library output, not identification confidence.

Four default Rust tests cover finite units/modes/configuration, independent area/
alignment/filter replay with corrupt cases, all/included/observation exports,
explicit missing states and pre-execution cancellation. The egui test selects a
matrix cell and verifies linked sample selection, routes processing through the
shared engine on a worker, and proves a missing-source failure leaves the original
history unchanged. A software render of the table/EIC/MS1 display is inspected;
native dialogs and real desktop event dispatch remain unverified.

### Scope and open acceptance gates

A local multi-sample centroid mzML dataset now produces a source-hashed,
inspectable matrix through all three interfaces with OpenMS 3.5.0 available.
Profile centroiding is an explicit option and lacks a new instrument/profile
reference panel. Whole acquisitions are loaded sequentially, while all feature
maps/matrix/evidence remain in memory: no bounded-RSS claim. Coarse progress counts
sample detection/evidence phases. Detection/blank filtering can be indeterminate
for sparse/low-landmark blanks; those cases never receive invented transformations.
Pose clustering is affine only. Gap filling is labeled window integration and
can include interference; there is no fitted/deconvolved gap peak, DIA/chimeric
handling, compound identity, fine-isotope fit or FDR. Default filter thresholds
are configurable method settings, not universally validated assay thresholds.

Still required: scientist-chosen legal instrument reference mixtures with
independent feature counts/RT landmarks/false features/missingness, representative
blanks and replicate designs; profile/nonlinear drift studies; large batch RSS,
disk/latency measurements; durable metadata/job recovery and verified OneDrive
crash behavior; native dialogs/Ctrl+C and Windows/MSRV/Linux/macOS release matrix.
The broader P6a framework and prior project/storage/job gates remain open.
Final command results are appended below after the final code checks.

### Final executed untargeted checks

- `cargo fmt -- --check`: PASS.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: PASS.
- `cargo check --locked --all-targets --all-features`: PASS.
- `cargo test --locked --all-features --quiet`: PASS, 219 library + 12
  chromatography + 10 engine + 2 converter + 25 QC + 25 spectral + 12 targeted +
  4 untargeted + 6 doctests = **315 passed**. The two original vendor tests and
  one explicitly opt-in OpenMS test are ignored in this default command.
- `cargo test --locked --all-features --test untargeted -- --ignored --nocapture`:
  PASS, **1 additional OpenMS reference test explicitly executed**. It exercises
  both six-run polarities, actual CLI/MCP, cache/restart/corruption/budgets,
  missingness, project provenance and child cancellation. Last execution took
  19.51 s. No external test is counted as passing merely because it was ignored.
- `python tests/reference/test_openms_adapter.py`: PASS, **5 references**.
  Python syntax compilation for adapter/helper/generator/references: PASS.
- The CLI sample-sheet helper completed actual six-run processing and produced
  full JSON, eight-row all-feature matrix, four-row included matrix and 48-cell
  observations CSV. Export metadata row counts are independently checked against
  emitted rows; a discrepancy in filtered/observation counts was corrected.
- The egui reference was rendered using CHROMASCOPE_UNTARGETED_PREVIEW=1; final
  software table/cell/EIC/MS1 image was inspected at target/untargeted-review.png.
- `git diff --check`: PASS; final status preserves initial user changes/deletions.

A Windows executable lease failure occurred when a helper CLI run overlapped
Cargo replacing chromascope-cli.exe; checks were rerun after the helper exited.
A checkpoint-directory rename lease was also observed on this OneDrive path;
short bounded retries were added, while persistent errors still fail visibly.
Neither event establishes general OneDrive crash recovery. Cargo config
 deprecation and Git line-ending notices are host warnings. No reference criteria
were loosened: independent window-area checks are tighter than the initial
incorrect full-tail comparison, and cold/warm matrices must match exactly.
The final headless/build follow-through results are appended after completion.

Final follow-through: `cargo test --locked --no-default-features --features
mcp-headless --quiet` PASS (**225 passed**, with the same two vendor and one
opt-in OpenMS tests ignored by the default command). `cargo build --locked
--all-features --bins` PASS for desktop, legacy GUI MCP, CLI and headless MCP.
The last full all-feature regression is 315 passed; strict lint/compilation/
formatting pass after the export row-count correction. Helper CSVs were read
with the independent Python CSV reader: 8 all rows, 4 included rows, 48 observation
rows. The corrected export metadata agrees with actual rows. Final diff whitespace
passes; all work remains uncommitted with original user files/deletions preserved.


## Feature metabolomics / lipidomics annotation validation � 2026-10-08

Scope: actual shared proposal/ledger/review/export implementation, not a claim
of instrument identification accuracy. New tests/annotation.rs has nine tests;
the GUI adds one actual software egui click/history test. Existing tests and
scientific reference fixtures were not weakened or rewritten.

Executed regression evidence:
- `cargo test --locked --all-features --quiet`: PASS, **325 passed**, two existing
  vendor tests and the existing opt-in OpenMS reference test ignored. The external
  OpenMS gate was not rerun in this annotation stage; its prior validation remains
  documented separately rather than counted as newly executed.
- `cargo test --locked --no-default-features --features mcp-headless --quiet`:
  PASS, **234 passed**, same three ignored tests.
- `cargo test --locked --all-features --lib gui::annotation::tests --quiet` with
  CHROMASCOPE_ANNOTATION_PREVIEW=1: PASS; software review panel rendered at
  target/annotation-review.png and inspected. Accept/reject/reopen append history,
  preserve originals and Unknown identity confidence. Native dialogs unverified.
- Strict all-target/all-feature Clippy, compilation, formatting and final build
  follow-through are recorded below after the final source checks.
- Python workflow source compiled with Python `compile()` without creating pyc:
  PASS. No Python/scientific dependency was installed.

Reference/edge evidence: published LIPID MAPS sum vs unordered chain hierarchy;
PC 34:1, PC 16:0_18:1 and PC 16:1_18:0 ambiguity, canonical chain order and invalid
sum/slash/class rejection. Independently known glucose neutral monoisotopic mass
180.06338810418 Da and protonated m/z 181.070664570801, plus Na-H offset
21.98194423547 Da, check actual shared formula enumeration/neutral relationships.
Coelution is tentative and noncoelution removes relationships. Authored synthetic
MSP isomer references with different structures but identical spectra retain both
Unknown candidates. Formula/MS/MS identity conflicts, incorrect adduct/charge,
unrelated native scan, corrupted isotope/formula/relationship evidence and
changed response candidates/review reasons fail. No synthetic fixture is
represented as a legal real-instrument benchmark.

Actual CLI run/export succeeds and a second export to the same path fails.
Actual MCP initialize/propose/review/export succeeds with the declared actor and
Unknown confidence. Project tests require registered source hashes, retain
original/reviewed revisions, and reject altered review or CSV response content.
Full response replay compares against the immutable request, normalizing only
new UUID/time metadata. CSV retains original cell spectra, source hashes, scores,
formulas/parameters, relationship evidence and complete review events; JSON
retains the whole matrix. New data are never assigned a confirmed identity.

Corrected during development: test glob-import ambiguity (explicit imports);
a synthetic fixture used the wrong MS/MS association marker (corrected to the
existing engine contract); nested enum JSON rejected u128 review timestamps
(changed to the established u64 representation and actual MCP rerun); strict
Clippy required the GUI test module after production items. No assertion was
weakened to hide these failures. Windows/OneDrive reported an incremental-cache
finalization Access denied warning during one compilation; the test command
completed successfully. No cache/user data deletion or reset was used.

Limitations: supported ester glycerolipid/phospholipid subset only; no full
Goslin nomenclature or specialist lipid-fragment/SIRIUS search engine, remote
submission, sn/DB position/stereochemistry inference, calibrated confidence,
FDR, large-library index or instrument certification. The implemented external
identification path is reproducible local MassBank/MSP/MGF library ingestion
and matching via existing adapters. Human citations/attestations are not
externally authenticated. Formula and nominal-isotope limitations remain.
256 hypotheses/10000 reviews per ledger; evidence snapshots stay in RAM.
Background proposal compute has stale-selection protection; kernel cancellation
and synchronous GUI replay/import/export remain gates. All earlier native/MSRV/
platform/RSS/storage/OneDrive recovery limits remain applicable.


Final follow-through after scan-metadata and project-table integrity checks:
- `cargo test --locked --all-features --quiet` with
  CHROMASCOPE_ANNOTATION_PREVIEW=1: PASS, **325 passed**, same three ignored tests;
  final screenshot shows the reopened tentative annotation and was inspected.
- `cargo test --locked --no-default-features --features mcp-headless --quiet`:
  PASS, **234 passed**, same three ignored tests.
- `cargo clippy --locked --all-targets --all-features -- -D warnings`: PASS.
- `cargo check --locked --all-features --quiet`: PASS.
- `cargo build --locked --all-features --bins --quiet`: PASS, all four executables.
- `cargo fmt --all -- --check`: PASS. `git diff --check`: PASS.
- Workflow helper executed against an explicitly authored synthetic matrix in
  a new target/annotation-smoke UUID directory: PASS. Independent Python
  csv.DictReader checks one C6H12O6 [M+H]+ Unknown row, original apex m/z,
  declared source hash and retained formula evidence. Repeating the helper
  refuses the existing directory and preserves the CSV byte-for-byte. This
  smoke fixture is not a newly acquired or instrument-validated feature matrix.

No dependency additions or installs, staging, commit, push, publish, raw-data
modification/deletion, reset or overwrite of prior work occurred. Existing Git
changes and the previously deleted assets remain as found. Cargo's deprecated
host config and Git CRLF conversion notices are host warnings, not scientific
validation. Specialist lipid/remote engines and unsupported structural resolution
remain explicitly documented work, not simulated successful identification.


## Statistical workspace validation — 2026-10-08

Implemented shared frozen-table statistics, local NumPy/SciPy adapter, GUI,
CLI/MCP/export and project replay/source/revision integration. No placeholder
numeric results or algorithms; original table/source evidence and past results
remain immutable. Runtime: Python 3.13.7, NumPy 2.3.3, SciPy 1.16.2; independent
reference generator mpmath 1.3.0 at 60 decimal digits. Python libraries were
already installed; this stage installed nothing and changed no dependencies.

Independent reference evidence: incomplete-beta Student-t probabilities and
bisection quantiles, Welch means/variance/df/CI/Cohen d/fold change and manual BH;
mpmath covariance eigensystem; analytically collinear PCA and 1D average-linkage
examples. No production NumPy/SciPy or adapter imports in the reference generator.
Reference JSON regeneration is byte-identical, SHA256
287c045798a42a395370af7e8602e1f81934cae73c6cc83d7866a6520c77f520.
Welch comparisons use 1e-10 relative tolerance (1e-11 absolute for general
examples; 1e-10 absolute for the one-constant-group fixture). The df=1 CI showed
~2.3e-9 absolute / ~2e-11 relative difference from 60-digit quantiles due to SciPy
inverse-t precision; it is checked under that stated scale-aware tolerance, not
represented as exact arithmetic. PCA covariance eigenvalues agree within 1e-10.
Known normalization/imputation/transform/scaling/linkage results are also checked.

New tests exercise reject/median/half-minimum/complete-feature missing policies,
all-missing/constant/rank-deficient and one-constant-group cases, total/median/IS
normalization, log2/log10/sqrt, center/autoscale/pareto, metadata filters/reasons,
BH/BY families, unsupported/invalid parameters, cancellation, changed replay
numbers/request settings/source tables, source-matrix filtering and preserved raw
EIC/MS1, actual targeted decoding/concentration/units/flags/calibration snapshots,
actual CLI/MCP analysis/export, no-overwrite, project commit/reopen/restore and
software heatmap selection. GUI render is optionally produced by
CHROMASCOPE_STATISTICS_PREVIEW=1 at target/statistics-workspace.png. These are
software/synthetic numerical checks, not instrument validation or native dialogs.

Corrected during development: -I excluded user-site scientific packages; changed
to -E -P (ignore Python env overrides/exclude CWD, preserve installed site libs).
Linkage test initially assumed fixed left/right sibling order; reference comparison
now verifies unordered child membership and exact merge heights/counts while
retaining optimal leaf ordering. A test borrow/move error was fixed. SciPy warns
for a single constant group even when Welch inference is valid; adapter now
retains warnings and checks that case against independent high-precision output.
The initial absolute-only df=1 CI assertion was replaced by the same documented
relative scientific tolerance used by the other Welch references. All original
regression tests remain intact; no failed reference was replaced by production
output. Initial documentation non-UTF-8 bytes were preserved through binary-safe
edits/appends. No push/publish/reset/raw-data overwrite or deletion occurred.

Standalone CSV helper smoke passed in new directory
target/statistics-smoke-3cd7801c-9e22-4b6a-b009-a04d8cab3786: three retained feature
rows; independent csv.DictReader p-value/original-column checks; constant feature
has empty p/q; repeating helper refuses directory and preserves CSV SHA256.

Python Black 25.9.0 formatting/check and py_compile pass for the three new Python
files. Dedicated Python lint packages (ruff/pyflakes/flake8/pylint) are unavailable
on this host; those lint checks are unverified. Rust strict Clippy is executed.
No linter was installed or its unavailable check reported as a pass.

Limits: two independent groups, no paired/covariate/repeated-measure/predictive
model; imputation bias and BH/BY assumptions explicit. Source settings and group
membership/exclusions/family size are frozen; no random method/seed needed.
Raw evidence is never invented for external tables. 256 samples/2048 features,
quadratic clustering and in-memory histories; 128 MiB subprocess/MCP 16 MiB caps
are admission limits, not RSS certification. GUI replay/import/export remains
synchronous. Strict replay requires a compatible retained runtime. Native dialogs,
platform/MSRV/OneDrive recovery and legal instrument studies remain open. Prior
vendor/OpenMS ignored scientific gates are unchanged and not silently claimed.


Final statistics checks (after the last code changes):
- cargo test --locked --all-features --quiet with CHROMASCOPE_STATISTICS_PREVIEW=1:
  PASS, 338 tests passed; the same three pre-existing vendor/OpenMS tests ignored.
- cargo test --locked --no-default-features --features mcp-headless --quiet:
  PASS, 246 tests passed; the same three ignored tests.
- cargo clippy --locked --all-targets --all-features -- -D warnings: PASS.
- cargo check --locked --all-features --quiet: PASS.
- cargo build --locked --all-features --bins --quiet: PASS, all four executables.
- cargo fmt --all -- --check and git diff --check: PASS.
- python -m black --check src/adapters/statistics.py examples/statistics_workflow.py
  tests/reference/generate_statistics.py: PASS; py_compile for those files: PASS.
- Independent reference regeneration/hash and actual CSV helper/no-overwrite
  smoke: PASS. Final software GUI screenshot rendered and inspected; selection
  shows original quantity/unit and the missing-volcano state explicitly.
Dedicated Python static lint remains unavailable; native dialogs/real-instrument/
platform/RSS/OneDrive gates remain unverified. Cargo deprecated-config and Git
CRLF conversion notices are inherited host warnings. No staging, commit, push,
publish, reset or user raw-data deletion/overwrite occurred.


## MCP analytical expansion validation — 2026-10-08

Inspected Git status and architecture/roadmap/validation/handoff before editing.
The checkout already contained extensive unstaged/untracked development and two
deleted assets; those were preserved. No dependency version change, installation,
staging, commit, push, publication, reset or user raw-data deletion/overwrite.

Implemented typed generated request/response schemas for every shared-engine
operation; general retained jobs/progress polling/cancellation; dataset discovery;
immutable JSON-pointer array/Unicode text paging; project inspection and explicitly
enabled revision-bound commit; six agent recipes/example prompts; transport-level
audit (including malformed/query/legacy calls); checksummed JSON review drafts.
Project approvals are client-attributed, identity_verified=false, and persist
through append-only commit/history restore. Scientific algorithms remain shared.

Executed local checks:
- cargo test --locked --all-features --quiet: PASS, 343 passed, 3 ignored.
- cargo test --locked --no-default-features --features mcp-headless --quiet:
  PASS, 251 passed, 3 ignored.
- New tests: one unit paging/checksum/missing-state test and four MCP integration
  tests, including real stdio binary/example execution.
- cargo clippy --locked --all-targets --all-features -- -D warnings: PASS.
- cargo fmt --all -- --check: PASS.
- git diff --check: PASS.
- python -m black --check examples/mcp_targeted_workflow.py: PASS.
- python -m py_compile examples/mcp_targeted_workflow.py: PASS.

Scientific/protocol evidence: raw synthetic mzML standards/QCs/unknown/blank
with independently known triangular areas; fitted concentration of diluted
unknown is 10 ng/mL within 1e-7; blank is unavailable, never fabricated zero.
Explicit strict accuracy rule calculates a QC failure. Original and reviewed
results are retained, tables preserve units and report SHA256 recomputes exactly.
Typed discovery includes all operation categories and request/response schemas;
permissions/malformed UUIDs/invalid paging/terminal cancellation are tested.
Stale project/review revisions fail, previews cannot commit, known failed
scientific invalid_parameters jobs remain explicit report entries, and restore
preserves approval history and original snapshots. The actual Python stdio
helper retains replay/transcript/CSV/report artifacts and refuses an existing
output directory without changing the prior report bytes. Existing independent
chromatography/calibration/QC/spectral/statistics references run in both suites.

An initial full regression run failed because alphabetical discovery moved the
legacy analytical_operation tool away from index zero; preserved the original
five-tool ordering and appended new tools. Existing tests were unchanged. Final
full suites pass. Compile-time schema/transport integration errors were corrected
before final checks; no failed reference was weakened or replaced by output.

The three ignored vendor/OpenMS tests remain opt-in and were not counted as
executed validation. Python dedicated lint tools ruff/pyflakes/flake8/pylint are
unavailable (importlib discovery checked); no such lint pass is claimed. Rust
strict lint, Python formatting/compilation and real helper execution were run.

Limits: reports are selected-job review drafts, not approved-only release or
scientific completeness certification. AI explanations and supplied human identity
are unverified. Jobs/full call audit are session-local; durable restart and
authenticated review remain open. 16 retained jobs and 16 MiB transport limits
do not establish bounded RAM for evidence/serialization/audit. Native progress
notifications/prompts/resources, idempotency, project lifecycle tools, HTML/PDF
release, RSS/platform/OneDrive recovery and legal instrument studies remain open.
The synthetic data are realistic encoded mzML acquisitions, not instrument study
certification. Cargo deprecated-config and Git CRLF notices are inherited.

Final compilation/binary gates after the last Rust change:
- cargo check --locked --all-features --quiet: PASS.
- cargo build --locked --all-features --bins --quiet: PASS (four binaries).
Strict Clippy was repeated successfully on the final Rust state.


## Reporting/project delivery validation - 2026-10-08

Final executed checks after the final Rust changes:
- cargo clippy --locked --all-targets --all-features -- -D warnings: PASS.
- cargo test --locked --all-features --quiet (CHROMASCOPE_DELIVERY_PREVIEW=1):
  PASS, 352 passed, 0 failed, 3 existing vendor/OpenMS tests ignored.
- cargo test --locked --no-default-features --features mcp-headless --quiet:
  PASS, 259 passed, 0 failed, same 3 ignored.
- cargo check --locked --all-features --quiet: PASS.
- cargo build --locked --all-features --bins --quiet: PASS, four binaries.
- cargo fmt --all -- --check and git diff --check: PASS.
Logs: target/delivery-final-{clippy,all-features,headless,check,binaries,format,
whitespace}.log. The opt-in tests were not rerun or counted as executed.

Added seven delivery integration/reference tests, one MCP protocol test and one
software GUI configuration/render test. Raw redistributable mzML TIC replays
exactly after source relocation. Authored triangle input integrates to 10
intensity*minutes independently; synthetic raw standards/internal-standard/
unknown reproduce the expected 10 ng/mL concentration within 1e-7. Targeted
replay excludes ONLY the batch UUID/creation time, reports those exact pointers,
and compares all remaining numerical/parameter/flag/evidence output exactly.
Original artifact/snapshot bytes and reversible restoration are checked. Missing/
changed raw inputs remain inspectable but prevent portable copying/reprocessing;
unsafe traversal maps fail, and missing/modified/extra inventory files fail
verification. No-overwrite behavior, schema defaults/future rejection, quoted
TSV conversion, escaped HTML, finite SVG axes/dimensions, and method missing
versus explicit-null differences are covered. Existing scientific references
remain unchanged and passed in both suites.

Actual CLI smoke independently created a project, committed known integration,
exported a portable bundle including raw input/local software source, verified
its inventory and replayed it with reproduced=true. Bundle location is retained
in target/delivery-smoke-location.txt. Delivered software was compiled using
cargo check --locked --manifest-path BUNDLE/software/Cargo.toml --target-dir
TARGET --no-default-features --features mcp-headless --quiet: PASS; log
target/delivery-source-snapshot.log. This is a compilation check, not authenticated
binary/source equivalence or platform certification. Cargo.lock/software
snapshot checksums and original requests/input hashes remain exported.

Software GUI rendering was executed and target/delivery-workspace.png inspected:
all twelve section choices, source inclusion, title/dimensions and reopen/export
controls are visible. Native dialogs/window interactions and HTML/PDF visual
layout were not separately validated. No PDF/raster export is advertised. Python
and R loading recipes are documented; no new Python implementation/dependency
was introduced and no R runtime execution is claimed.

Initial delivery tests exposed an incorrect package-only version comparison
(the engine records chromascope/VERSION/KERNEL) and Windows canonical-path prefix
normalization. Fixed the kernel guard and canonical expected location; numerical
assertions were preserved. The first full stage regression passed 350/258 tests;
after additional mapping/missing-state/GUI tests and final changes, the final
352/259 suites above passed. No original test was weakened or removed.

Limits: review drafts only; no authenticated approved-only release, PDF/PNG/TIFF,
automatic Python/R environment provisioning, archive compression or universal
replay-equivalence normalization. External adapters still need documented original
runtimes; some operations' generated IDs/cache paths can produce a reported exact
comparison mismatch. Local software snapshot export needs the build repository
available. Evidence/report serialization and snapshot copying are in memory and
not bounded-RSS certification. GUI evidence reopening is synchronous; export and
replay workers have no cancellation controls. Cooperative project locking and
after-copy source hashes do not certify OneDrive recovery/external source locking.
Native/platform/instrument/regulatory gates remain unverified. No staging,
commit, push, publish, reset or user raw-data overwrite/deletion occurred.
Inherited deprecated Cargo config/CRLF notices remain host warnings.


### Performance/reliability final-pass failure evidence (2026-10-08)

This pass retained failed checks and corrected implementation defects rather than
removing assertions. Logs in `target/performance-*` include: Windows LNK1104
(running benchmark executable; profiler now copies it), transient missing GUI
navigation during concurrent editing, strict Clippy style rejection, reader borrow
compile rejection, missing-array adversarial opening rejection, incorrect gzip
opener, and the raw-source probe's legitimate empty-scan regression. The final
empty/nonempty regression distinguishes missing scientific evidence from corruption.

Finalization also observed an invalid UTF-8 middle-dot label in concurrently edited
`src/gui/quant.rs` (repaired without changing the displayed unit; an initial broad
byte replacement was immediately corrected and UTF-8 verified), two GUI click
checks whose controls were outside their enlarged form viewport (the click helpers
now scroll rendered controls and retain all history/numerical assertions), and a
Windows LNK1318 PDB linker error. The opt-in OpenMS MCP scientific test also failed
its existing 20-second running-job limit under competing builds, while retaining
state=running and completed_sample_phases=11. Its time assertion was not relaxed.
Separate final reruns and their outcomes follow; none of these failures is counted
as a pass or silently erased. Other development continued in this same dirty tree.

Native OpenMS input now uses the same explicit raw-time-unit preflight as extraction,
including checkpoint reuse. A no-native-process reliability test rejects unknown
units before execution. Raw source hashes, previous results, failed adapter attempts
and checkpoint evidence remain preserved. Referenceable CV group compatibility,
full source schema, token/decompression/retained-memory bounds and real instrument
method validation remain open.


## Development-release preparation audit - 2026-10-08

This task audited source/interface dispatch against the roadmap, rebuilt README,
added FEATURE_STATUS/ENGINE_OPERATIONS/RELEASE_NOTES/RELEASE_BACKLOG, and added
runnable release workflows and a synthetic calibrated mzML input generator.
No release tag, commit, push, publication or scientific certification was made.
The original dirty checkout and original raw inputs were preserved.

### Executed evidence and exact outcomes

Logs are retained locally in `target/release-audit/` (ignored, not distributable
CI evidence). Environment: Windows, Rust/cargo 1.99.0, Python 3.13.7, NumPy 2.3.3,
SciPy 1.16.2, mpmath 1.3.0, Black 25.9.0. Rust 1.88 is declared but not tested.

| Command / check | Exact observed result | Evidence / scope |
|---|---|---|
| `cargo test --locked --all-features` using isolated target directory | Exit 0; 366 passed, 0 failed, 5 ignored | tests-all-final.log and tests-all-confirmed.log; earlier changing-source slices |
| `cargo clippy --locked --all-targets --all-features -- -D warnings` | Exit 0 | clippy-final.log / clippy-last.log; earlier source slices |
| `cargo check --locked --all-features` | Exit 0 | check-final.log and later check-serial.log |
| `cargo build --locked --all-features --bins` | Exit 0 | build-final.log / build-last.log; earlier four executable build |
| `cargo build --locked --release --all-features --bins --target-dir target` | Exit 0, optimized build completed in 11m30s | build-release.log; earlier source slice, not current-tree certification |
| `cargo test --locked --all-features -j1` | Exit 101; library 225 passed, 2 failed, 0 ignored; integration suites not reached | tests-all-serial.log; spectral rendered search control and untargeted rendered observation missing after scrolling |
| `cargo test --locked --no-default-features --features mcp-headless -j1` | Exit 0; 275 passed, 0 failed, 5 ignored | tests-headless-serial.log |
| `cargo clippy --locked --all-targets --all-features -j1 -- -D warnings` | Exit 101 | clippy-serial.log; then clippy-release-close.log found five items-after-test-module errors introduced by concurrent busy helpers; moved those methods before tests, rerun required |
| `cargo build --locked --all-features --bins -j1` | Exit 101 | build-serial.log: linker LNK1318 PDB error for chromascope-engine-mcp |
| `cargo fmt --all -- --check` | Exit 0 | fmt-release-close.log; formatting rerun after method reorder |
| Independent scientific reference regeneration | Exit 0; five retained reference families reproduced exactly | references/ and reference verification logs; targeted, QC, spectral, statistics, chromatography |
| Python release helper Black and bytecode compilation | Exit 0 | retained Python validation logs; no Ruff/flake8 claim |
| New release-workflow integration tests | 2 passed | included in earlier full suite and later headless suite; CLI relink integrity and operational helper/no-overwrite assertions |
| Synthetic input generator | Exit 0; repeat into same directory exit 1 | calibrated-inputs/ retained; generates raw inputs, never hardcoded analytical outputs |
| Calibrated example through real MCP stdio | Exit 0 | calibrated-mcp.log and calibrated-mcp/ discovery, transcript, responses, tables, draft |
| Calibrated example through CLI `run -` with request on stdin | Exit 0 | calibrated-cli-corrected.json/.stderr; initial incorrect invocation was retained as exit 1, not concealed |

The five ignored tests require vendor prerequisites (two), pinned native OpenMS
reference prerequisites (one), or explicit performance opt-in (two). They are not
passed tests. Hosted CI, Linux/macOS, native desktop interaction, MSRV, vendor
conversion, instrument acceptance and opt-in performance runs were not executed
by this release-preparation task. Other session reports remain separate evidence.

### Blockers and reproducibility boundary

Other chats continued changing GUI, parser, proposals and Cargo configuration in
this same checkout throughout validation. Source manifests and two source copies
were retained; neither copy is claimed to be a fully validated release snapshot.
Earlier successes cannot certify the later checkout. Windows file locks initially
blocked replacement of running debug executables; an isolated generated build
cache avoided those executable locks. Later checks also hit insufficient disk,
PDB/linker and paging-file errors. All failed logs were retained; no assertions
were weakened or results fabricated. Later compile failures caused by concurrent
QuantState changes were repaired in that work, but require integrated verification.

Automatic approval review rejected generated-cache cleanup as blocked by policy;
no more specific reason was supplied and no cleanup was performed. No original
raw acquisition or result was deleted. Final acceptance requires a stable source
snapshot, adequate Windows build resources, passing strict lint and all-feature
GUI/integration/build checks, and evidence matched to that snapshot. The current
release status is preparation complete, acceptance open. See RELEASE_BACKLOG.md.

### Later repaired GUI rerun

`cargo test --locked --all-features --lib -j1` exited 0: 228 passed, zero
failed, zero ignored (tests-gui-release-close.log). The scrolling failures from
the prior run are retained above but did not recur after the concurrent rendered
control helper updates. This library-only pass does not certify the full
integration suites or builds. Local document links: 69 checked, zero missing.
`git diff --check` exited 0. A close-source hash manifest contains 153 files.

## Completion follow-up - 2026-10-09

Outstanding commands completed: `cargo test --locked --all-features -j1`
exited 0 with 374 passed, zero failed, five ignored (tests-all-release-repaired.log).
`cargo build --locked --all-features --bins -j1` exited 0 in 1m49s
(build-release-repaired.log); `cargo build --locked --all-features --release
--bins --target-dir target -j1` exited 0 in 14m24s (build-release-serial.log).
Earlier GUI/PDB/resource failures are retained above, superseded for these
passing runs, not erased. Later strict lint found two new style issues
(collapsible optional-field if and single-element draft loop); both were
corrected without changing behavior. Lint/format/headless rerun results follow.
Concurrent-source caveats and unexecuted scientific/platform gates still apply.

Final strict lint: `cargo clippy --locked --all-targets --all-features -j1
-- -D warnings` exited 0 (clippy-oct09.log, 49.78s). Formatting detected
a concurrent statistics edit after that command; `cargo fmt --all` applied
formatting, then `cargo fmt --all -- --check` exited 0 (fmt-oct09-final.log).
`git diff --check` exited 0 (diff-oct09-final.log). Earlier format failures
remain retained. Formatting changes preserve assertions and analytical behavior.


### Finalization continuation - 2026-10-09

Continuation retains the October 8 evidence and failed checks. The combined suite
reached statistics but an actual ENOSPC failure stopped the MCP statistics test;
no assertion was removed. Generated build dependencies and incremental caches
were compressed reversibly with Windows NTFS compact; source/raw/reference files
were not deleted or compressed. Source-side statistics adapter I/O now maps to
adapter_failure rather than blaming input parameters, with a StorageFull test.
GUI click regression helpers use the painted content clip; both complete original
interaction assertions passed separately. Strict Clippy failures in concurrently
added forms/project code were corrected without suppressing lints; all formatting
and final checks are rerun on the combined checkout. Final counts and executable
digests follow in the completed-check section.

Last headless rerun: `cargo test --locked --no-default-features --features
mcp-headless -j1` exited 0: 279 passed, 0 failed, 5 ignored
(headless-oct09.log). All queued checks are complete. New proposal integration
tests are included. Five ignored prerequisites remain unexecuted. Software
checks pass on their recorded source slices; final publication/scientific gates
and concurrent-source snapshot caveats remain open. No publication performed.


### Completed performance/reliability verification - 2026-10-09

These are the latest completed checks for this performance pass, supplementing
historical and overlapping release/GUI records above. Full command logs remain in
`target/performance-finalized-oct09-*.log`; compact check exits, log SHA-256,
source inventories, measured executable hashes and reference digests are retained
in `docs/performance/2026-10-08/final-oct09`. The final all-features/headless reruns
include changes made by concurrent project/export/GUI development. Source hashes
identify the observed checkout; overlapping development means this is not proof
of a frozen release snapshot.

| Check | Reproducible command | Actual result |
|---|---|---|
| All-feature Rust unit/integration/doctests, including MCP E2E and GUI interactions | `cargo test --locked --all-features` | Exit 0; 379 passed, 0 failed, 5 ignored |
| GUI-free CLI/MCP/shared-engine suites | `cargo test --locked --no-default-features --features mcp-headless` | Exit 0; 282 passed, 0 failed, 5 ignored |
| All-target compilation | `cargo check --locked --all-features --all-targets` | Exit 0 |
| Debug binaries | `cargo build --locked --all-features --bins` | Exit 0 |
| Optimized CLI/MCP binaries | `cargo build --locked --release --no-default-features --features mcp-headless --bins` | Exit 0 |
| Strict lint | `cargo clippy --locked --all-features --all-targets -- -D warnings` | Exit 0 |
| Formatting and whitespace | `cargo fmt --all -- --check`; `git diff --check` | Both exit 0 |
| Native independent OpenMS reference/negative/CLI/MCP/project/cache | `cargo test --locked --no-default-features --features mcp-headless --test untargeted openms_multisample_reference_positive_negative_cli_cache_and_project -- --ignored --nocapture` | Exit 0; 1 passed, 22.78 s; original internal timeout retained |
| Python OpenMS unit/reference checks | `python tests/reference/test_openms_adapter.py` | Exit 0; 5 tests |
| Independent reference regeneration | `python tests/reference/verify_references.py NEW_REFERENCE_OUTPUT` | Exit 0; all 5 complete numeric reference sets match exactly |
| Performance with resource sampling | `python tests/reference/profile_performance.py NEW_OUTPUT --scans 1000 10000 --openms` | Exit 0; exact output assertions passed; earlier 50000-scan run retained |
| New Python tooling | Black check and py_compile on both reference/profiling helpers | Exit 0 |

Five ignored tests in normal Cargo output do not mean five unexecuted scientific
checks: both opt-in benchmarks and the OpenMS scientific test were run separately.
Two real ProteoWizard/vendor tests remain unexecuted because the prerequisite
executable/licensed acquisition was unavailable. No test was disabled or assertion
weakened to obtain these results.

The measured synthetic plain mzML workload reaches 50000 scans / 1,157,311,284
bytes. Latest 10k measurements: open/preflight 1.1316 s; XIC 0.7253 s; missing-CV
TIC/BPC 1.1247/1.2486 s; sampled process-tree RSS 86.27 MiB. Fixed workloads:
1000 integrations 0.4176 s, 1000 weighted fits 0.4338 s, 64-sample targeted batch
0.2483 s, 10k-entry spectral import/search 0.2652/0.3665 s, 100 synced project
commits 1.7310 s. Six-sample OpenMS detection/alignment cold/checkpoint reuse
2.5918/1.9852 s with identical complete feature/alignment JSON. CPU/RSS/I/O and
concurrency evidence and the slower contended 50k run are documented in
PERFORMANCE_VALIDATION.md. These are observations under uncontrolled background
load/cache, not release throughput, p95, bounded-memory certification or a net
speedup claim. Safety checks add measurable fallback cost.

Implemented repairs cover raw RT units/truncated XML/counts, true RT/mass extrema,
malformed arrays, mixed valid/missing TIC/BPC metadata, gzip opening, legal empty
spectra/Missing states, reader-detail restoration, race-free scan budgets,
actual-read byte budgets, release panic recovery and adapter disk-I/O error
classification. Metadata-only discovery and selective fallback avoid unnecessary
decoding; original results retain their engine version, while corrected extraction
uses extraction-v2. Tests exercise cancellation after progress, concurrent readers
and writers, large batches, partial project writes, abandoned locks, source
preservation, GUI-compatible extraction and actual CLI/shared-engine parity. MCP
protocol tests and native CLI/MCP references exercise the same shared engine.
This demonstrates tested paths, not exhaustive parity of every GUI parameter.

Failures remain auditable: Windows LNK1104/access-denied executable locks, PDB
LNK1318, transient concurrent-source compile/lint errors, expanded-form click
failures, disk exhaustion and the earlier contended OpenMS timeout. The generated
PDB was retained; reversible NTFS compression of generated build caches restored
disk headroom without deleting source/raw/reference data. Statistics storage
failures now report adapter_failure with a regression test. Real rendered control
scrolling restored original GUI assertions. The final headless retry passed after
the executable lock cleared; no unrelated process was terminated. The erroneous
manual CLI verb probe was retained and corrected before accepting release output.
Failed check log digests are in retained-failed-checks.json; earlier evidence and
failure explanations remain in this report and performance guide.

Remaining acceptance gates are explicit in REMAINING_DEFECTS.md: source XML/CV
reference-group compatibility and fuzz/resource limits; durable killed-job recovery
and abandoned-lock policy; resident artifact/history budgets and repeated source
traversals; synchronous GUI responsiveness; isolated release repeats/platform/vendor
matrix; representative instrument datasets and method-specific scientific study.
Authored triangular/Gaussian cases, independent numerical references and existing
DDA/MassBank fixtures support software verification only. No formal scientific
validation, certified identification/concentration/FDR, or power-loss/OneDrive
recovery guarantee is claimed. P9 remains IN_PROGRESS. No push, publication,
existing-work reset or user-data deletion was performed.


## UX01-UX18 remediation verification (2026-10-09)

The local desktop remediation pass is complete; see UX_UI_REMEDIATION.md for the
issue-by-issue files, evidence and external limits. Central staged workspaces, typed
forms, project snapshots/handoffs, shared MCP/GUI proposals, virtualized numeric
tables, common plot controls, theme contrast and keyboard/numeric alternatives are
implemented. Native inspection corrected punctuation, selected-name contrast,
report wrapping, plot-label/card height and focused-control scrolling.

Latest recorded all-feature suite: 381 passed, zero failed, five ignored. Headless:
282 passed, zero failed, five ignored. Strict all-target/all-feature Clippy and
format passed. All-feature binaries built; final GUI rebuilt after label correction.
Logs: target/ux-remediation-{clippy,tests,headless,build}-complete.log. Source/binary
hashes: target/ux-remediation-final-hashes.json. No warning suppression or scientific
assertion weakening. Earlier failed probes/test assumptions remain documented in
the remediation report; results from other work slices above remain historical.

Native light/dark and narrow/large inspection covered seven workspaces. Final Data
plots show simultaneous raw/processed traces; Shift+Tab scrolls retained results into
view. CLI-created synthetic QC project opened/saved in GUI, MCP queued a proposal
and desktop persisted attributed approval at revision 4, reopened in the final
executable. Native report export passed CLI verification for all 12 files.

Screen-reader/platform certification, reviewer authentication and instrument/assay
acceptance are external limits. Five default-suite tests remain ignored for adapter/
performance prerequisites; separate performance results elsewhere are not implied
by this pass. Shared concurrent edits were preserved. No commit, push, publication,
user-data deletion or scientific/release certification was performed.
