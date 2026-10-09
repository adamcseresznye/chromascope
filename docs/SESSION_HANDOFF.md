# Session handoff

Audit date: 2026-10-08 (America/Los_Angeles).
Baseline: `ff7975aedfdcd894e31dd1e85dc3707d2cdd01f5`, Chromascope 0.3.0.
The original audit established the plan. The latest development delivery below
supersedes the audit-only implementation state; later scientific phases still
require their own validation gates.

## Read first

1. [VALIDATION_REPORT.md](VALIDATION_REPORT.md): actual checkout, baseline results
   and source-observed risks; distinguishes skipped checks from failures.
2. [ARCHITECTURE.md](ARCHITECTURE.md): shared engine, IDs/units, project and provenance
   contracts, jobs, scientific modules and human/AI review.
3. [IMPLEMENTATION_ROADMAP.md](IMPLEMENTATION_ROADMAP.md): dependency order, status
   meanings and acceptance gates. Update this register with each delivery.

## Work done and preserved

Inspected the Cargo package/workspace/lockfile, Rust source modules and embedded
tests, GUI flows/state, mzdata extraction, quantification, persistence/import,
MCP policy/dispatch, exports, fixture/integration tests, examples, guides, assets,
CI, release and wasm configuration. Ran baseline formatting, compilation, strict
lint and all-feature tests. Default-path tests and both normal executable builds
also passed.

Created the four requested engineering documents. Changed only `.gitignore` to
permit these specific documents while retaining other ignored docs behavior.
No production Rust, numerical algorithms, schemas, dependencies, tests, existing
guides or workflows were changed. No commit, staging, PR or deployment performed.
Two pre-existing deleted assets were left untouched:
`assets/chromascope-0.3.0.png` and `assets/demo.gif`.

All-feature baseline: 209 unit tests + 2 integration tests + 6 doctests passed;
2 real-converter integration tests ignored. Default path: 197 unit tests + 2
integration tests + 6 doctests passed, with the same 2 ignored. Formatting,
all-target/all-feature check, strict Clippy and both executable builds passed.
P0 documentation is VERIFIED locally; the four files are ready to stage and remain
uncommitted. The global Cargo config deprecation warning is
host configuration, not a repository regression. Windows Rust 1.99 results do
not prove Rust 1.88, Linux/macOS, release or native GUI behavior.

## Design decisions to carry forward

- Retain the existing parser/processing APIs and GUI while extracting pure
  quantification/domain code through small compatibility-preserving changes.
- One engine implementation serves GUI, CLI and MCP. GUI view commands stay
  separate from analytical execution. Headless support is an explicit feature
  change; no-default-features is not headless today.
- Persist typed IDs and immutable method/result revisions; names and paths are
  display/source attributes. Sample identity differs from acquisition identity.
- Projects use schema-versioned metadata plus immutable artifacts, source hashes,
  migration and crash recovery. Storage dependency choice requires a spike.
- All computations and decisions carry provenance; AI proposes and engine policy
  controls acceptance. Existing MCP root/change/export rules remain.
- Jobs enforce byte/thread/reader/cache/disk budgets and cancellation inside work
  units. A bounded queue alone does not bound memory.
- Preserve full-resolution numeric inputs, detection-versus-integration smoothing,
  original scan mappings, automatic/manual results, chord baseline and area units.
- Start with targeted quantification/calibration/QC; identification, untargeted,
  stats and reports depend on validated shared foundations.

## Recommended next bounded task

P1a/P1b characterization and domain extraction are next, after explicit task
scope is agreed. Start with `src/gui/quant.rs` pure Method/Analyte/Peak/Measurement,
validate/detect/measure functions and their existing tests. Keep GUI batch worker,
widgets and legacy files intact until shared DTOs/IDs are ready. Review validation
of NaN, deserialized XicParams, vector lengths and large scan-count smoothing
before exposing a general engine boundary. Corrections are separate production
tasks; this audit deliberately did not change behavior.

The first vertical slice should load the included mzML, extract an XIC, integrate
known bounds and return an immutable result with IDs and parameters through a
shared service. Prove the same result via GUI adapter, CLI and MCP before widening
the tool API. Project persistence and jobs follow their roadmap dependencies.

## Reproduction and outstanding checks

From the repository root, use commands recorded in the validation report.
Run checks sequentially to avoid Cargo lock waits. Do not use cargo fmt without
`--check` as an audit action. Keep Cargo.lock and source semantics stable.
Real vendor verification needs msconvert and locally licensed datasets; ignored
tests include exact setup requirements. Linux/macOS/MSRV/native GUI/stdio visual
example/release/scale/security/scientific panels remain unverified.

No engineering implementation unit is VERIFIED merely because these documents
exist. At handoff, consult the final P0 status and document-validation record.
Assign engineering/scientific owners before new module acceptance. For each
follow-up session record commit/toolchain, initial user changes, commands and
outputs, changed semantics, evidence, blockers and the next bounded task here.

## Latest development handoff — 2026-10-08

The shared foundation is implemented and uncommitted. Read the final architecture
update and implementation-validation section before the historical audit notes.
New modules: domain (UUIDs, RT/area units, requests/errors), engine (typed analytical
service and provenance), presets/quant (pure code extracted from GUI), project
(immutable revisions and hashed artifacts/sources), jobs (bounded worker-owned
readers/cancellation), engine_mcp (optional headless transport). New binaries:
chromascope-cli and chromascope-engine-mcp. Default `gui` behavior is retained;
`mcp` still supplies the existing GUI bridge; `mcp-headless` works independently.

GUI and desktop MCP continue to use shared processing and extracted quant/preset
functions. Existing desktop tool names, batch/session files, scan indices and
numerical semantics survive. No arbitrary or placeholder scientific algorithm
was introduced. Fixes include finite mass/deserialized XIC validation, smoothing
limit cast overflow, array-length/RT validation, finite integration inputs,
XIC array order/length validation and streaming XIC peak arrays.

Projects reference immutable raw files; repeated registration preserves dataset
identity and rejects changed content at an existing registered path. Verified
relink supports source moves. New revision files preserve past results/methods;
result blobs are checksummed. Explicit legacy TOML method import preserves the
original file and mapping across repeated imports. CLI project failures retain
requests/errors. Unknown project schemas fail rather than being rewritten.

All-feature formatting/check/strict Clippy/tests pass: 210 library, 10 engine
integration, 2 converter integration, 6 doctests; 2 vendor tests remain ignored.
New engine tests include CLI, actual headless MCP stdio, project reopen/relink,
corruption/stale revision, method import, numerical/reference equivalence and
cooperative cancel/resource checks. Core-only tests (126 library + 8 engine integration + 2 converter + 6 doctests),
GUI-free MCP compilation and all four executable builds also pass; see the
validation report for commands. No native GUI or scale certification.

Continue P1b/P2–P4 gates rather than claiming the entire roadmap done. Next:
connect projects to GUI/MCP dataset/method/result revision routing; add lossless
viewer/batch migration and durable analyte/peak/review IDs; persist job lifecycle;
add binary numerical blobs/spill and explicit decoded-array budgets; establish
recovery/backup and OneDrive locking tests. Keep old viewer/batch save available
until migration equivalence is proven. A crashed writer may leave writer.lock;
there is no automatic stale-lock deletion. Do not silently discard immutable
revision history or orphan artifacts during recovery.

Initial `.gitignore`/doc work and deleted assets remain preserved. Do not stage,
commit, push or publish without authorization. Re-run checks after changes using
the exact commands in the validation report, and distinguish measured results
from deferred acceptance gates.

Final follow-through: actual MCP stdio engine tests also passed with GUI disabled
(9 tests before historical restore addition). `project-restore` now restores a
saved snapshot as a new revision with its original identities, preserving every
previous revision. Final all-feature count: 210 library + 10 engine integration +
2 converter + 6 doctests, 2 ignored; strict Clippy, format and diff checks pass.

## Advanced chromatography handoff - 2026-10-08

This supersedes the earlier recommended next-task scope for this delivery. The
existing uncommitted foundation, .gitignore edits and asset deletions were
preserved. Inspect the final architecture section for equations, units, API
shapes, GUI workflow and limitations; the roadmap adds P5c as IMPLEMENTED, with
local evidence separated from broader scientific/platform verification.

New shared module: src/chromatography.rs. Adapters: Operation/Output in domain and
engine; existing CLI run plus export-peaks; existing headless MCP analytical_operation
accepts process_chromatograms, revise_chromatogram and export_chromatographic_peaks.
The engine returns chromatography-v1 provenance for these operations. GUI uses
an opt-in advanced review panel after existing batch extraction, without changing
legacy detection, measurements or desktop MCP tools. Advanced analyses are an
optional serde-default field in Measurement: old batch JSON remains readable.
New batch saves retain advanced raw inputs, stage arrays, immutable automatic
peaks, reasons/actors and append-only corrections. Saved analyses are replayed
before restore/review/export; corrupt and stale proposals fail. Preview requires
explicit GUI Apply; successful and failed CLI previews leave project revisions
and source registration untouched. Project::add_result rejects previews.

The illustrative input examples/chromatography-request.json contains an
explicit synthetic Gaussian plus drift, not expected or hardcoded output.
Run it through the headless CLI as documented. tests/chromatography.rs contains
scientific references and actual CLI workflow/no-overwrite/preview-project tests.
tests/reference/generate_chromatography.py regenerates independent NumPy/SciPy
fixtures (NumPy 2.3.3, SciPy 1.16.2, deterministic seed 143); those packages are
used for fixture generation only, not installed into or required by Rust runtime.
Engine tests extend actual MCP stdio with processing and correction preview.
GUI tests roundtrip advanced history and click preview/apply/manual/accept in
software egui frames. Optional CHROMASCOPE_QUANT_PREVIEW=1 writes
 target/chromatography-review.png; that render was generated and visually inspected.
It establishes software layout/control behavior, not native OS/file-dialog behavior.

Scientific refinements from references: prominence SNR rejects small ripples on
residual baseline drift; FWHM interpolates adjacent half-height crossings and is
null when clipped; width gates also apply after overlap partitioning; negative
intensities and signed areas remain intact. Independent SG/baseline arrays and
areas are compared with fixed tolerances; existing tests were not weakened.
Batch validation now permits finite negative intensity while retaining RT,
method, peak and history checks. CSV uses full roundtrip f64 formatting, explicit
units, nullable values, revision/review flags and sample/analyte names. Output
file creation refuses existing CSV paths; full lineage stays in JSON artifacts.

Remaining work is explicit: processing in the advanced GUI panel is synchronous
and may pause large-run frames; kernel cancellation is between traces, not inside
loops. Resource caps do not certify RSS. Noise estimation assumes independent
approximately Gaussian noise and local smoothness; short/coarse/correlated traces
need review. Positive chromatographic peaks are detected; negative signals can be
manually integrated but are not declared positive peaks. Reference bounds use a
provided shift, without automatic identity or alignment. Valley partitioning
handles resolved maxima, not unresolved component deconvolution. No Gaussian/EMG
fits, auto alignment, durable review UUID/timestamp DAG or parameter-change
revisions were fabricated. Saved configuration cannot be edited destructively;
a different extraction method requires saving the batch and starting a new batch.

Next: move advanced compute/preview and batch processing to cancellable workers
with stale-result protection; preserve analysis provenance while adding method
parameter revisions; compare supported baselines/peak gates on a legal real
instrument reference panel before introducing component fitting. Continue the
foundation's existing durable jobs/project/recovery gates separately.
Final executed commands and counts are in the final validation report section.
No staging, commit, push, publish, raw-file deletion or reset of existing work.


## Targeted small-molecule handoff - 2026-10-08

Read the final architecture section for schema, equations, workflow and limits.
The initial dirty foundation and deleted assets were preserved; all work remains
uncommitted. The new shared `targeted` module performs calibration/concentration
analysis, while raw ions use the existing legacy unsmoothed XIC/detection/chord
kernel. Engine operations are targeted_batch, review_targeted and export_targeted.
CLI run accepts the same envelope; export-targeted/export-targeted-calibration
consume saved responses and refuse existing paths. Headless MCP checks every
nested sample source against allowed roots. GUI adds an independent targeted
panel with editable sample/assay definitions, background runs/cancel, current and
historical run selection, ion/calibration/residual plots, reasoned review and
separate complete history JSON. Old batch/session behavior stays available.

nalgebra 0.33.3 provides scaled weighted SVD least squares, not normal equations.
Independent NumPy 2.3.3 LAPACK lstsq (unscaled powers) and polynomial roots provide
27 fixtures; generator is tests/reference/generate_targeted.py. Numerical
comparison tolerance is 1e-9*(1+abs(reference)). Synthetic mzML acquisitions from
tests/fixtures/targeted_support.rs have independently known triangular areas;
production always reads/decodes/extracts the actual files. Fixtures are generated
locally and contain no third-party instrument data. The example request uses the
existing legal mzML with unknown roles and intentionally no invented standards.

Review stores UUID/time/actor/reason and accepts only available concentrations;
reject/accept preserves numbers and all decisions. Numeric artifacts are replayed
before review/load/export, including traces, selected areas, RT windows, extraction
filters, calibration, precision and result/history consistency. Projects verify
all raw-source hashes before result commit and retain prior revisions. Read-only
historical artifact inspection does not require current raw files; new commits do.

Important limits: thresholds flag evidence and require review; no universal assay
acceptance policy, inferred LOD/LOQ, uncertainty intervals, carryover or robust
outlier selection. Below-domain signals have no extrapolated concentrations.
Only machine boundary roundoff is admitted, with an explicit boundary_roundoff
flag. Missing standards fail that calibration unless excluded with a reason.
Signed signals are preserved; IS denominator zero/negative fails explicitly.
Raw acquisition failures/cancellation do not commit partial new batches. Direct
targeted manual peak revision/refit is not yet exposed; preserve existing area/
advanced workflows and create new retained targeted batches for method changes.
Detailed definitions/qualifier addition/IS-only targets use JSON; CSV sheet
import is pending. No native desktop or real instrument certification, bounded
RSS, durable partial jobs or cloud-sync recovery is claimed. Extend these gates
without silently changing the preserved legacy detector or weakening references.

Final validation results and exact commands are in VALIDATION_REPORT.md. No push,
publish, staging, commit, raw-data deletion, reset or replacement of existing work.

## Batch QC / method-validation handoff - 2026-10-08

This delivery supersedes the earlier statement that carryover and assay policy
are pending in P5b. Read the final architecture/validation sections for equations,
24 metrics, schemas, evidence and limits. All initial dirty work and deleted
assets were preserved; this stage adds no dependencies and remains uncommitted.

Shared code: `src/qc.rs`; interfaces: domain/engine, headless MCP description,
CLI `run`/`export-qc`, project QC verification, `src/gui/qc.rs` connected to the
retained targeted panel. Tests/fixtures: `tests/qc.rs`,
`tests/reference/generate_qc.py` (stdlib only), `tests/reference/qc.json`,
`examples/qc-request.json`; existing independent NumPy targeted fixtures are
reused without modification. Desktop QC uses the engine; external study designs
and targeted evidence retain all numeric inputs, units, rules and provenance.

A required failure fails acceptance; unavailable/insufficient required evidence
is indeterminate. Every decision and the aggregate batch acceptance retain a
calculation, threshold, reason and evidence. Independent reference control-chart
bands and injection-order/RT/mass/IS slopes are descriptive, with explicit units.
Method-validation comparisons require disjoint declared preparation groups at
matched levels and an explicit reference; no unavailable design is invented.
Configurable calibration metrics reuse the existing weighted SVD fitter and
retain coefficients, roots, residuals and diagnostics. GUI thresholds come only
from saved assay settings; the full rule/design/calibration JSON remains editable.
Mass errors are externally supplied ppm observations, never inferred from an
extraction tolerance or fabricated from an unavailable centroid measurement.

QC reports have UUID/time, immutable original study/evidence, replay-checked
numeric/acceptance records and review chains. Acknowledgement/reopen records
actor/reason/UUID/time and rejects stale revisions, without changing analytical
status. Historical evaluations stay selectable. Save QC history JSON retains all
runs/review revisions; load verifies each report before adding it. Create-new
CSV includes rule, formula, observations, statistics, calibration fit, units and
status; full JSON retains original targeted raw traces and decisions. New project
QC commits check every registered underlying source; past results stay intact.

Try the synthetic explicit-design example:

```powershell
Get-Content examples/qc-request.json -Raw | target/debug/chromascope-cli run -
# Save the resulting engine response, then export into a new path:
Get-Content saved-qc-response.json -Raw | target/debug/chromascope-cli export-qc qc-new.csv
```

For a real retained batch, open Batch QC and method validation, Configure from
retained batch, edit Acceptance thresholds or JSON, and Evaluate and retain QC.
Original raw evidence is held outside the draft text but embedded in the saved
report. To modify the original quantification, use a new retained targeted batch;
editing QC observations cannot bypass replay of the immutable source snapshot.
External method-validation studies use `purpose: method_validation` and
`validate_method`; generic QC uses `evaluate_qc`. MCP uses the identical envelope
via `analytical_operation`, with an existing allowed-root path for authorization.

Final validation: formatting, strict all-target/all-feature Clippy, compilation,
all-feature tests (281 passed), GUI-free MCP tests (193 passed), all four binary
builds and diff whitespace pass. Two pre-existing vendor tests remain ignored.
Scientific evidence is 24 independent Python examples plus 27 NumPy fits checked
under three metrics, known failing batches, real decoding of synthetic mzML,
actual CLI/MCP, project replay/source checks and egui evaluation/review clicks.
The test-only software render was inspected; native dialogs are unverified.

Next practical gates: scientist-selected legal instrument multi-run/lot studies
and method-specific acceptance protocols; cancellable background QC/replay with
stale routing; direct source-linked centroid mass errors; CSV study/sample sheets;
inter-run variance components, normalized paired matrix factors, uncertainty and
trend/run-pattern policy only with adequate design. Keep missing/ambiguous states
explicit, generic input grids complete, raw data/results/history immutable, and
acceptance separate from acknowledgements. No claims of regulatory certification,
bounded RSS, OS portability, durable partial jobs or OneDrive recovery. No commit,
staging, push, publish, user raw-data deletion or reset occurred.
## Advanced spectra / compound identification handoff - 2026-10-08

Read the final architecture section for formulas, units, schemas and boundaries.
The initial dirty foundation, documents, .gitignore edits and deleted assets were
preserved; no staging, commit, push, publish, reset or user-data deletion occurred.
New shared files: `src/spectral.rs`, `src/spectral/library.rs`,
`src/spectral/chemistry.rs`; adapters: domain/engine/parser/project, CLI and
engine MCP; desktop state and `src/gui/spectral.rs`. New dependency is mzsignal
1.1.12 with the existing nalgebra backend, plus compatible nalgebra macro crate.
No Python runtime dependency: the reference generator/workflow helper use stdlib.

The desktop button opens the independent spectrum workbench. Prepare operations,
review/edit explicit JSON parameters and execute on a background worker; select
retained results, raw/processed spectra, mirror overlays and candidate fragments.
Import local MSP/MGF/MassBank with a source/version/URL/license declaration.
Missing raw adduct/instrument metadata is not guessed; reprocess a new declared
metadata snapshot if needed. A reasoned confidence annotation appends history.
Unknown or compound class can be recorded even with no hit. Confirmed identity
requires referenced same-method authentic-standard/RT/diagnostic/alternative
attestations; the software does not authenticate those external attestations.
CLI/MCP use the same operations and kernels. `export-spectral` refuses existing
CSV paths; JSON retains full evidence. `examples/spectral-request.json` contains
explicit synthetic input. `examples/spectral_workflow.py` composes local library
import/search using actual CLI and saves create-new request/response evidence.

Raw acquisition scans keep SHA256, original/native ID, RT, ions/charge/isolation,
activation and original metadata. Linked precursor XIC requires matching raw
source hash and polarity; no automatic cross-file/coelution assertion. Project
commits replay spectral numerics and check registered hashes. History reload
replays spectra, libraries, scores, formulas/isotopes and annotation constraints.
No automatic identity, confidence probability or FDR is generated. Averaging is
fixed-anchor tolerance grouping with absent peaks as zero; raw/signed subtraction
survives the positive scoring filter. Greedy assignment can be non-optimal and is
explicitly tested. Nominal isotope convolution is not fine isotope structure;
closed-shell CHNOPS + seven adduct types are bounded hypotheses, not all chemistry.

Fixtures: unmodified CC BY MassBank release 2026.03 Antwerp records AN111301 and
AN111302 (one compound, 5/10 eV), with source URLs/authors/licenses/SHA256 in
`tests/reference/spectral_sources.json`. `generate_spectral.py` independently
regenerates unique-mass cross-energy cosine and exhaustive Decimal formula
references. Synthetic references cover Gaussian profile centroids, negative
subtraction, sparse false matches, tied isobars, H/Na ambiguity, charge spacing,
carbon binomial/sulfur nominal patterns, corrupt replay and stale/undo review.
Actual CLI/export/no-overwrite, MCP import/search/review/root policy, project
history and source-linked XIC are tested. Software egui clicks search and record
an Unknown annotation while retaining previous history; optional
CHROMASCOPE_SPECTRAL_PREVIEW=1 renders target/spectral-review.png. Native dialogs
remain unverified. Exact final check commands/counts are in VALIDATION_REPORT.md.

Next: scientist-curated legal multi-compound positive/negative/isomer/adduct panels
and an explicit confidence protocol; reliable instrument/acquisition metadata
mapping; large-library disk index/cache and cancellation within loops; GUI import/
replay/export workers. Do not infer identities, RT comparability or precursor
purity from cosine. Declared DIA rejects search; undeclared acquisition warns,
without asserting DDA purity. No deconvolution, FDR, resolved isotopologue fit,
remote identification or general fragment chemistry was fabricated. Keep the
prior project/OneDrive recovery, durable jobs, native/MSRV/platform and RSS gates
separate from this locally validated software delivery.
Final result: formatting, strict all-target/all-feature lint, compilation,
310 all-feature tests and 221 GUI-free MCP tests passed; the same two vendor
tests remain ignored. All four executables build; final diff whitespace and
both unmodified MassBank fixture hashes pass. Final software screenshot was
rendered and inspected. Complete checks/limitations are in VALIDATION_REPORT.md.

## Untargeted LC-HRMS handoff — 2026-10-08

Implemented P7a's local multi-sample processing/review slice; keep the earlier
foundation, targeted, QC and spectral limitations in force. Initial dirty files,
untracked prior-stage modules and deleted assets remain intact. No staging,
commit, push, publish, reset or raw user-data deletion occurred.

Relevant code: `src/untargeted.rs`, `src/adapters/openms_metabo.py`; engine/domain,
CLI, headless MCP, project commit checks and `src/gui/untargeted.rs` adapters.
The Python adapter is embedded and included in Cargo packaging. Optional local
runtime must provide pyOpenMS 3.5.0; set CHROMASCOPE_OPENMS_PYTHON to its executable
if PATH python differs. No auto-installation or remote data submission. The new
Rust dependency ctrlc 3.5.2 supports CLI cooperative cancellation; its published
metadata declares MIT/Apache-2.0 and Rust 1.69, while this repository keeps 1.88.

Desktop: open Untargeted LC-HRMS feature matrix, select mzML files and an existing
checkpoint directory, edit the explicit configuration JSON including roles,
metadata, polarity, tolerances, noise/FWHM/sample rate, gap flag and filters, then
Process / resume verified checkpoints. Matrix cells link to metadata, raw EIC,
apex MS1 and selectable associated MS/MS. RT units are seconds here. Histories
append; all/included CSV and full-evidence JSON exports refuse existing paths.
JSON load accepts either direct engine responses or CLI wrappers and independently
checks request/config, areas, alignment and filter evidence. Gap-filled cells and
all missing/filter states remain visible; indeterminate filter evidence is never
exported as included. Processing uses a worker; cancellation retains completed
sample checkpoints and the failed/cancelled attempt.

CLI preparation helper (edit the example sheet to real source paths first):

```powershell
python examples/untargeted_workflow.py examples/untargeted-samples.json NEW_OUTPUT --polarity positive --gap-fill
# Or submit a version-1 request directly; '-' selects the first batch source:
Get-Content request.json -Raw | target/debug/chromascope-cli run -
# Optional third argument commits all registered sources + result into a project.
Get-Content response.json -Raw | target/debug/chromascope-cli export-features matrix-all-new.csv
Get-Content response.json -Raw | target/debug/chromascope-cli export-features-filtered matrix-included-new.csv
Get-Content response.json -Raw | target/debug/chromascope-cli export-feature-observations observations-new.csv
```

The helper creates a new directory, saves request/response and exports all three
views. Request operation is `untargeted_batch` with `config`; defaults are filled
in and retained by the engine. `export_feature_matrix` with `report` returns all,
filtered and long CSV text through the shared operation. CLI Ctrl+C requests
cancellation; native dispatch remains a separate validation gate.

MCP keeps analytical_operation and adds start_untargeted (path plus same request),
untargeted_status (job_id), cancel_untargeted (job_id), untargeted_features
(job_id/offset/limit/include_evidence). Every sample/checkpoint path must be inside
an allowed canonical root; missing cache directories require an allowed existing
parent. Pages retain result ID and missing/group/filter evidence; full arrays are
optional. Server retains up to 16 feature jobs; export evidence before restarting.
No persistent job ID or partial final matrix is advertised. Completed sample
checkpoints resume after process/server restart; alignment/correspondence and
raw evidence/gap extraction recompute. Corrupt checkpoints fail explicitly.

Scientific/reference tests: `tests/untargeted.rs`, independent six-run positive/
negative generator, Python adduct/window/filter references. Run the OpenMS gate
explicitly with `cargo test --locked --all-features --test untargeted -- --ignored`;
only this opt-in test needs the external scientific runtime. The prior two ignored
vendor tests still require licensed vendor data/msconvert. Default tests do not
silently pretend the external OpenMS test ran.

Preserve distinctions: original OpenMS FWHM area versus EIC window trapezoid area;
raw versus aligned RT; detected versus filled/missing; excluded versus indeterminate;
per-sample adduct hypotheses versus confirmed identities. Negative deconvolution
requires negative charge ranges and normalized priors. Exact JSON detection
values restore warm-cache featureXML precision. Sparse centroid data can require
an explicitly chosen trace sample-rate policy. Missing blank alignment never
receives an invented identity transform or gap fill.

Next practical gates: legal scientist-selected instrument mixtures/unknowns and
predeclared feature/RT/false-feature/missingness criteria; profile and nonlinear
alignment references; interference-aware gap fitting; disk-backed matrices and
bounded RSS/disk/latency tests; durable job/storage recovery and native/platform
checks. OpenMS loads a complete run per pass; all feature maps/evidence survive in
RAM. Current limits and checkpoints are not memory certification or OneDrive
recovery. No DIA deconvolution, FDR or identity confidence was fabricated.
Final exact check counts/results are in VALIDATION_REPORT.md.

Final checks: 315 all-feature default Rust tests and 225 GUI-free MCP default
tests passed; the additional opt-in positive/negative OpenMS reference test was
explicitly run and passed, as did five independent Python references. Formatting,
strict all-target/all-feature Clippy, compilation and all four executable builds
pass. The software GUI render was inspected. CLI helper exports were independently
counted (8 all / 4 included / 48 observations); corrected export metadata matches.
The two original vendor tests remain ignored. Exact commands, observed Windows
lease failures, corrected issues and remaining scientific/resource/native gates
are recorded in VALIDATION_REPORT.md. No push, publish or user-data deletion.


## Feature metabolomics / lipidomics annotation handoff � 2026-10-08

New shared module src/annotation.rs; GUI src/gui/annotation.rs is opened inside
the retained untargeted matrix. Engine/domain expose propose_feature_annotations,
add_feature_hypothesis, review_feature_annotation and export_feature_annotations.
CLI run and MCP analytical_operation use these same operations; CLI
export-annotations consumes an engine response and refuses an existing path.
No new dependencies, package installation, remote service calls, staging,
commit, push, publish, data deletion or reset. Initial dirty changes/deleted
assets remain intact.

Start a ledger from a selected matrix, prepare formula/adduct bounds for the
selected feature/sample, edit parameters or load a retained local spectral-library
response (imported via the spectral workbench), and compute tentative hypotheses.
Review compares candidates and native/query/reference spectra, with explicit
lipid resolution and source evidence. Prepare competing revision copies a
candidate into an editable draft with a fresh ID. Accept/reject/reopen requires
a reason, appends history, and never changes confidence or promotes identity.
Save full ledger JSON for GUI reload; saved engine responses use CLI export.
examples/annotation_workflow.py creates request/response/logs/ledger/CSV in a new
directory from a real retained matrix, explicitly chosen adducts/atom bounds and
optional retained library. The CLI executable must already be built.

Ledgers embed all original matrix/raw evidence and source hashes. Formula
candidate selections and isotope/library evidence replay and link to native
feature/sample spectra. Formula versus library/adduct incompatibility cannot
be combined as supporting evidence. UUID isomers stay separate; generated
confidence is Unknown. Human elevated confidence needs referenced diagnostic
and alternatives evidence; ConfirmedIdentity is always forbidden here.
Adduct relationships retain tentative same-formula neutral mass/raw RT
comparisons and tolerances. Lipid PC 34:1 vs PC 16:0_18:1 follows the conservative
LIPID MAPS subset documented in ARCHITECTURE.md; sn/slash, DB positions and
stereochemistry remain unresolved. Unsupported classes/linkages fail explicitly.

Tests: tests/annotation.rs plus GUI annotation test. New tests use authored
synthetic spectra/isomer examples, published lipid hierarchy and independent
known glucose/H/Na masses; existing MassBank/Decimal/NumPy/OpenMS references
remain unchanged. Project tests verify retained revisions and reject changed
review evidence. Software GUI click/render checks do not validate native dialogs.
Exact final checks/counts and corrected failures are in VALIDATION_REPORT.md.

Limits/next steps: no automatic specialist lipid-fragment engine, Goslin full
parser, SIRIUS/CSI remote search, calibrated probability/FDR or instrument
certification. Existing MassBank/MSP/MGF reproducible library adapters are the
implemented identification path. 256 candidates and 10000 reviews per ledger;
large evidence/history/library copies remain in RAM. Proposal runs on a GUI
worker; replay/import/review/export is synchronous and in-kernel cancellation
is pending. Human evidence citations/attestations are recorded, not authenticated.
Empty searches have no invented candidates; engine responses retain attempts,
while direct GUI ledger snapshots do not retain a separate empty-search log.
Prior native/platform/OneDrive/storage/RSS validation gates remain in force.


Final annotation checks: 325 all-feature and 234 GUI-free MCP tests pass; the
same vendor/OpenMS opt-in tests remain ignored and were not rerun in this stage.
Strict lint, formatting, compilation, all four binaries and diff whitespace
pass. Independent helper/CSV/no-overwrite smoke checks pass. Final software GUI
review screenshot was inspected. Exact commands and limits are recorded in
VALIDATION_REPORT.md. ProbableStructure additionally requires an actual retained
structure identifier; the supported lipid sum/chain labels retain unresolved
positions and cannot be promoted to that confidence. No original work was reset
or overwritten and no staging/commit/push/publish/raw-data deletion occurred.


## Statistics / quantitative omics handoff — 2026-10-08

P7b's local deterministic descriptive/univariate workspace is implemented.
Shared engine: src/statistics.rs and embedded src/adapters/statistics.py;
GUI: src/gui/statistics.rs; adapters: domain/engine/project/CLI/headless MCP.
No Cargo dependency additions, installations, staging, commit, push, publish,
reset or raw-data deletion. Initial dirty work and deleted assets were preserved.
Existing docs contained invalid UTF-8 bytes; roadmap edits and appended sections
preserved those inherited bytes rather than rewriting earlier documentation.

Open Statistics and quantitative omics in the desktop. Load a generic table,
retained feature-matrix response, targeted batch response or statistics response.
The table is sample-major with explicit units and null unavailable values.
Add group definitions to sample metadata without modifying original source fields.
Use preprocessing controls or full settings JSON (filters/exclusion reasons/groups
remain explicit), then Analyze and retain new revision. Earlier results remain
selectable; restore their original table/settings for reversible preprocessing.
PCA score/loadings, volcano and clustered heatmap selections link to samples/
features and retained EIC/MS1/native MS/MS; targeted batches link quantifier/
qualifier chromatograms. Generic concentration inputs have no invented raw links.
Save full JSON for complete replay/history evidence; CSV preserves quantitative
columns, settings, sample design, warnings, effects/CI/p/q and provenance.

Requires local Python >=3.11, NumPy >=1.26,<3 and SciPy >=1.11,<2.
CHROMASCOPE_STATS_PYTHON selects an existing executable. Nothing auto-installs.
The validated environment is Python 3.13.7 / NumPy 2.3.3 / SciPy 1.16.2;
independent reference generator uses mpmath 1.3.0. App statistics absence is an
explicit unsupported-capability error; statistical tests require this runtime.
Replay checks versions/numerics strictly; preserve the original runtime for
saved analysis verification. Transformations/normalization/imputation never
replace original quantities or prior project revisions. No numerical fallback.

Try the explicitly synthetic CSV example in a NEW directory:

```powershell
python examples/statistics_workflow.py examples/statistics-table.csv examples/statistics-metadata.csv examples/statistics-settings.json NEW_OUTPUT --unit ng/mL
Get-Content NEW_OUTPUT/request.json -Raw | target/debug/chromascope-cli run -
Get-Content NEW_OUTPUT/response.json -Raw | target/debug/chromascope-cli export-statistics statistics-new.csv
```

The helper retains hashed CSV inputs, request/response, logs and result CSV, and
refuses existing output directories. CLI run SOURCE PROJECT can commit a generic
table against the declared source; for retained matrices/targeted batches, run -
PROJECT registers every embedded underlying sample source before replay/commit.
MCP analytical_operation uses analyze_statistics (table/settings) and
export_statistics (report), with an existing allowed-root path. It does not need
GUI state or write projects. All adapter operations call the same engine.

Tests: tests/statistics.rs, GUI heatmap selection/software render, authored
triangular matrix helper and existing raw targeted mzML helper. Independent
mpmath incomplete-beta/quantile and covariance-eigen references are generated
by tests/reference/generate_statistics.py into statistics.json, with known
collinear/linkage examples. They never import production NumPy/SciPy/adapter.
Actual CLI, no-overwrite, MCP, project append/restore/source validation and
source-linked table integrity are checked. Validation report records exact
commands/counts, corrected failures and numerical tolerances.

Limits/next steps: independent two-group Welch only; no paired/covariate/ANOVA/
repeated-measure/predictive methods. Imputation enters inference and can bias it.
BH/BY assumptions, family membership, insufficient/constant states and SciPy
warnings remain visible. Clustering is Euclidean and quadratic; bounds are
256 samples/2048 features, snapshots in RAM, output capped at 128 MiB, MCP at
16 MiB. GUI import/replay/export is synchronous; analysis compute is cancellable.
No large-matrix RSS certification, native-dialog validation, instrument study
validation, regulatory certification or OneDrive recovery is claimed. Next:
scientist-selected design/normalization policies and legal instrument references,
metadata/exclusion grids, paired/covariate models with independent references,
disk-backed storage and asynchronous replay/export. Prior-stage limits persist.


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


## MCP analytical expansion handoff — 2026-10-08

Read docs/MCP_ANALYSIS.md and src/engine_mcp.rs. New typed start_analysis accepts
exact shared-engine Request, with conditional JsonSchema derives across domain
DTOs. analysis_capabilities exposes request/response schemas and six recipes.
Dataset discovery; general status/cancel; JSON-pointer array/text paging; project
inspection/commit; session audit; and checksummed review-draft reporting are live.
Original five headless tools and desktop MCP are preserved. No scientific
algorithm/dependency is added. Actor/AI explanation is attributed, unverified.

Launch with --allow-root DIRECTORY. --allow-project-writes explicitly enables
commit_analysis; it requires existing project/dataset UUID, expected revision,
retained successful job, approval actor/reason. add_result enforces existing
raw hash/replay validation and preview rejection. Project agent_commits persist
attributed approvals and applied revisions; restore preserves that history.
No final-release approval is inferred. Create/register/relink/restore with CLI.

examples/mcp_targeted_workflow.py runs real stdio targeted/calibration/optional
explicit QC/export/report into a create-new directory with input hashes, complete
responses, transcript and stderr. tests/engine_mcp.rs uses synthetic real mzML
standards/QCs/unknown/blank with independent known triangular areas; exercises
protocol schemas, calibration, null blank, QC failure, review/export, paging,
commit/stale policy/history, preview refusal and actual helper/no-overwrite.

Remaining work: durable/restartable full audit/general jobs, authenticated human
approval gates across adapters, idempotency, native progress/prompts/resources,
project lifecycle tools, approved-only HTML/PDF release, bounded disk-backed
evidence/RSS and instrument/platform gates. Reports select supplied job IDs and
remain drafts; completeness is not scientifically certified. Session audit is
in memory; export before shutdown. Limit: 16 retained jobs, 16 MiB transport,
2 workers/16 queued; serialization and retained history are not bounded-RSS proof.
Initial dirty work and deleted assets were preserved; no commit/push/publish/reset
or user source overwrite/deletion performed. Exact check results follow.

Final MCP checks: 343 all-feature tests and 251 headless MCP tests pass; three
existing vendor/OpenMS gates remain ignored. Four new protocol/actual stdio
integration tests plus one unit paging/checksum test pass. Strict all-target/
all-feature Clippy, Cargo formatting, diff whitespace, Python Black/compilation
pass. No original test was edited. Discovery-order regression was fixed by
preserving legacy tool order. See VALIDATION_REPORT.md for scientific tolerances
and limitations; final compilation/binary checks are recorded there separately.

Final compilation/binary gates after the last Rust change:
- cargo check --locked --all-features --quiet: PASS.
- cargo build --locked --all-features --bins --quiet: PASS (four binaries).
Strict Clippy was repeated successfully on the final Rust state.


## Reporting/project delivery handoff - 2026-10-08

Shared implementation src/delivery.rs; GUI src/gui/delivery.rs; adapters in CLI,
engine_mcp and Project::open/restore relocation. No scientific engine replacement
or Cargo dependencies. Read docs/PROJECT_DELIVERY.md before continuing.

Desktop Project reports and delivery configures all report sections/title/SVG
dimensions, reopens source states and full retained evidence, exports on a worker,
and reprocesses selected immutable results on a worker without replacing them.
CLI: project-report PROJECT NEW_DIRECTORY [CONFIG.json]; project-bundle same;
project-verify; project-sources; project-reprocess PROJECT RESULT_UUID;
project-compare-methods LEFT.json RIGHT.json; project-migrate PROJECT. Existing
run/import/restore/export/GUI/MCP/mzML paths remain intact. MCP
export_project_report returns JSON/HTML content within the existing transport
bound and allowed source/root policy, without filesystem writes.

Bundles are directories, not zip archives. Every historical snapshot/result byte
is preserved; source-map.json resolves original paths to content-verified bundled
inputs inside the moved bundle. checksums.json covers every delivered file,
including raw inputs and a local software snapshot. project-verify detects missing,
changed and extra files; copy the entire directory. New project commits invalidate
the original export inventory; generate a new delivery rather than changing it.
Partial failed directories remain available and are never automatically deleted.
Software snapshot needs the build's original repository path available; it is
explicit local evidence, not authenticated binary/source equivalence.

Replay verifies all current inputs and kernel identity, relocates paths only in
new requests/output comparison, and retains new response separately. Exact-output
comparison excludes only TargetedBatch's batch_id/created_unix_ms and explicitly
lists those pointers. Other generated IDs, runtime differences, cache paths or
external nondeterminism can produce a reported mismatch; do not broaden exclusions
without independent validation. Tests cover raw TIC and independently known
triangle/calibration/unknown concentration, moved bundles, original-byte history,
missing/changed sources, unsafe mappings, inventory corruption, HTML escaping,
CSV/TSV correctness, method missing-vs-null, schema defaults/future rejection,
CLI no-overwrite, MCP protocol and software GUI configuration rendering.

Limits: reports are review drafts; no PDF/PNG/TIFF, authenticated approved-only
release, auto-provisioned Python/R runtimes, bounded RAM/disk certification or
native-dialog/OneDrive/instrument validation. HTML full-evidence blocks are verbose;
only the supported figure types have SVG views. Python/R data loading and pinned
scientific-runtime instructions are documented; no remote submission or install.
Evidence reopening/verification stays synchronous; export/reprocessing workers
have no user cancellation yet. Prior stage limits remain. No staging/commit/push/
publish/reset or user-data deletion/overwrite. Final checks follow in validation.

Final delivery checks: 352 all-feature and 259 headless tests pass; same three
opt-in tests ignored. Strict Clippy, formatting, whitespace, all-feature compile
and four executable builds pass. Actual portable CLI/replay smoke and delivered
software headless compilation pass. Software GUI screenshot inspected. Detailed
commands, tolerances, corrected failures and open gates are in VALIDATION_REPORT.md.


## Performance/reliability verification handoff - 2026-10-08

Read docs/PERFORMANCE_VALIDATION.md and docs/REMAINING_DEFECTS.md first. New
tests/performance.rs exercises actual scientific kernels on generated raw mzML;
tests/reference/profile_performance.py builds/profiles it and retains wall time,
sampled process-tree CPU/RSS/I/O and binary/environment identity. Output must
be new. Run --scans 1000 10000 50000 --openms for the observed size range.
The core is unoptimized in this test profile; these are not release benchmarks.
A 50k run overlapped compilation and is explicitly labeled contention evidence.
Do not rebuild a running Windows test executable; a failed LNK1104 log is retained.

Shared parser changes: metadata-only window/RT inspection, actual RT extrema,
traversal count check, shared TIC/BPC summaries, per-scan missing CV fallback,
valid supplied metadata retention, finite/sorted/equal-length array checks and
linear fallback cursor. Original sums, inclusive windows and tie semantics remain.
The old all-zero retry remains. Extraction results/metadata/legacy quantification
now carry extraction-v2; integration retains legacy-v1. Old project results
remain readable but exact reprocess refuses a changed kernel label.

JobControl scan admission uses atomic CAS; hashing checks actual bytes, including
a growing source. Scheduler panic recovery uses a tested production guard;
release panic policy is unwind. This does not handle native crashes, OOM or
process termination. New tests/reliability.rs checks known mixed-metadata
values/scan maps, malformed arrays, unsorted RT, non-XML files, cancellation,
GUI compatibility/engine/actual CLI parity, simulated interrupted temporary
saves, abandoned locks, reversible restore and eight concurrent project writers.

The independent NumPy/SciPy/Decimal/mpmath references reproduce exactly in a
separate directory using tests/reference/verify_references.py. Existing reference
JSON and MassBank inputs are unchanged. Opt-in positive/negative OpenMS
Gaussian/affine/cache/project/CLI/MCP test and five Python OpenMS references
were explicitly run. Default suites still report opt-in tests as ignored;
benchmark executions are separate from default regression counts.

Late source probe reproduced unknown-unit and sampled-mass-bound defects.
New src/source_validation.rs validates raw RT units/values and XML/count/scan
completeness for mzML/gzip before reader conversion. Missing/partial scan windows
now inspect all peak arrays; DataReader uses mzdata's gzip opener. quick-xml 0.30
and flate2 are promoted existing locked dependencies, with no version upgrades.
Unknown/missing/minute/second/millisecond/truncated/count/interior/mixed-window/
gzip cases are covered. Full source schema, token/decompression memory limits
and fuzzing remain open; this is not universal CV/instrument certification.
No native instrument/platform/cloud-sync or regulatory certification; no durable
restartable general jobs or bounded-RSS history. Preserve earlier-stage limits.
Exact final commands/counts and retained logs appear in VALIDATION_REPORT.md.
No staging/commit/push/publish/reset or user-source overwrite/deletion. Existing
uncommitted work and deleted assets were preserved.


## Development release preparation handoff - 2026-10-08

Start with README.md, FEATURE_STATUS.md, ENGINE_OPERATIONS.md, RELEASE_NOTES.md,
RELEASE_BACKLOG.md and examples/README.md. The feature matrix is current scoped
interface evidence; architecture/roadmap baseline absence statements are now
explicitly historical. P6a/P9 are IN_PROGRESS; no broad scientific acceptance
stage was promoted to VERIFIED. Existing dirty work/deleted media were retained.

New examples/release_workflows.py invokes the real CLI for unknown-only targeted
extraction, explicitly strict synthetic QC failure/acknowledgement and retained
MassBank self-search. It saves original/request/response/stderr/hashes/CSV; no
scientific results or identities are invented. examples/targeted_reference.rs
reuses the authored raw triangular test input generator for a fully runnable
calibrated CLI/MCP example. It creates new inputs/request only, never results.
The prior targeted/spectral/statistics/annotation/MCP helpers remain operational.

CLI now has --help and project-relink PROJECT DATASET_UUID SOURCE, calling
Project::relink and commit. Changed content fails; identity and original source
bytes/history remain. tests/release_workflows.rs verifies actual helpers, missing
concentrations, QC failures after acknowledgement, reference search/CSV, refusal
of existing directories, CLI help and hash-verified relink. Existing scientific
reference/test assertions were not weakened or replaced.

CI declares Python 3.13 with pinned NumPy 2.3.3/SciPy 1.16.2/mpmath 1.3.0/
Black 25.9.0, headless tests, all executable builds and reference regeneration.
Hosted CI/platform/MSRV/native/vendor/instrument gates remain unexecuted here.
Existing PROJECT_DELIVERY/PERFORMANCE_VALIDATION/REMAINING_DEFECTS guides and new
release docs are now Git-visible via narrow ignore exceptions; no staging.

Other concurrent work added mzML/gzip source-unit preflight, full fallback mass
bounds and updated GUI navigation. This task preserved it, reconciled README
labels and fixed one strict Clippy match-style issue in source_validation.rs.
Final full-tree validation uses target/development-release-validation to avoid
overwriting the running target/debug desktop. A Windows access-denied build and
a strict-lint failure were retained; partial copied build cache was retained
after stopping only this task cache-copy helper. Final commands/counts follow
in VALIDATION_REPORT.md; source hashes record the checked snapshot.

Remaining: bounded XML/decompression/binary validation, scientist-selected legal
instrument panels, cross-platform/MSRV/native checks, durable jobs/audit/storage
and killed-save/OneDrive recovery, authenticated review, disk-backed evidence,
async replay/export and operation-specific replay tolerance. Development release
preparation does not authorize tag/push/publish or certify a scientific release.
No commit, push, publication, raw-data deletion or original-result replacement.


### Performance finalization follow-up - 2026-10-08

See PERFORMANCE_VALIDATION.md and REMAINING_DEFECTS.md for retained 1k/10k/50k
(1.157 GB) workloads, sampled CPU/RSS/I/O and contested timing observations.
The legal-empty-scan regression now retains scan positions and explicit Missing
spectrum state. Native OpenMS inputs/checkpoint reuse share extraction's raw-unit
preflight. Source CV group/full-schema/resource-bound validation remains open.
GUI test interactions now scroll real rendered controls after the concurrent form
expansion; no scientific/history assertion is removed. Final checks, retained
failures and precise outcomes are appended to VALIDATION_REPORT.md. No user data,
prior results, existing dirty development or deleted media were reset; no push or
publication. P9 remains IN_PROGRESS pending the documented wider acceptance gates.


### Release preparation acceptance handoff

README, feature/operation inventories, release notes/backlog and runnable examples
are prepared. Latest complete headless run: 275 passed, zero failed, five ignored.
Later all-feature run: 225 library passed, two rendered-control failures; no full
pass claim. Debug build later failed LNK1318 PDB. Concurrent busy helpers were
moved before test modules for strict lint without changing assertions. Final
reruns and source stability remain required; see the dated release audit in
VALIDATION_REPORT.md and retained target/release-audit logs. Calibrated synthetic
CLI and real MCP examples ran successfully; transcripts/results retained.
Preserve existing dirty work and media deletions. No publication authorization.


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

### Completion follow-up - 2026-10-09

Outstanding all-feature checks completed: 374 passed, zero failed, five ignored;
debug four-bin build passed (1m49s), optimized four-bin build passed (14m24s).
Strict all-target/all-feature lint passed (clippy-oct09.log); final formatting
and diff checks passed. Later optional-field/single-draft-loop lint repairs
preserve behavior. Previous failures remain in the validation report. New project
toolbar and AI proposal controls are present and labelled experimental in the
README/matrix; authenticated and native/recovery acceptance remains open.
Current source manifest: target/release-audit/source-oct09-hashes.json. Overlapping
checks are not certification of a frozen snapshot while other edits continue.
Last headless rerun results follow in VALIDATION_REPORT.md. No push/publication.

Last headless rerun: `cargo test --locked --no-default-features --features
mcp-headless -j1` exited 0: 279 passed, 0 failed, 5 ignored
(headless-oct09.log). All queued checks are complete. New proposal integration
tests are included. Five ignored prerequisites remain unexecuted. Software
checks pass on their recorded source slices; final publication/scientific gates
and concurrent-source snapshot caveats remain open. No publication performed.


### Performance pass completed checks - 2026-10-09

Latest combined-tree suites: 379 passed all-features and 282 passed headless,
zero failed; five ignored per normal run. Three ignored scientific/benchmark
checks ran separately and passed; the two real vendor checks remain unavailable.
Strict Clippy, format, all-target check, debug binaries and optimized headless
binaries passed. Five independent reference sets reproduced exactly and five
Python OpenMS tests passed. Compact evidence and final log/source hashes are in
performance/2026-10-08/final-oct09; complete commands/results/failure audit are in
VALIDATION_REPORT.md and performance observations in PERFORMANCE_VALIDATION.md.

Preserve corrected extraction-v2, legacy result provenance and raw/reference
inputs. Do not overwrite dirty concurrent GUI/project/release development. Next
priorities: legal CV-group source compatibility, killed-process recovery, bounded
artifact budgets, asynchronous GUI work, isolated release/platform/vendor study
and representative instrument validation. REMAINING_DEFECTS.md records severity
and evidence. Windows executable/PDB locks, contended timeout and disk exhaustion
were retained, repaired or retried without weakening assertions or deleting data.
No push/publication; P9 remains IN_PROGRESS, not formal certification.


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

Final native app remains open: target/ux-remediation-final-verified.exe. Synthetic
verification project target/ux-native-project-20261009 is at revision 5, preserving
final Data display bounds/Focus mode and the revision-4 attributed review history.
