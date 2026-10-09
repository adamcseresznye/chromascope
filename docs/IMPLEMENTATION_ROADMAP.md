# Implementation roadmap

Baseline: 2026-10-08, commit `ff7975aedfdcd894e31dd1e85dc3707d2cdd01f5`.
Design authority: [ARCHITECTURE.md](ARCHITECTURE.md). Validation evidence:
[VALIDATION_REPORT.md](VALIDATION_REPORT.md). Resume instructions:
[SESSION_HANDOFF.md](SESSION_HANDOFF.md).

## Status discipline

Allowed engineering statuses: `NOT_STARTED`, `IN_PROGRESS`, `IMPLEMENTED`,
`VERIFIED`, `BLOCKED`. IMPLEMENTED means code/artifact exists; VERIFIED requires
recorded acceptance evidence and its scope. A design document does not establish
implementation. BLOCKED requires a concrete missing prerequisite, owner and
unblocking action. Scientific correctness and software test passage are separate
gates. Change this register as work lands, with commit, date, checks and remaining
limitations; do not mark a whole phase verified from one subtask.

| Historical baseline capability | Status at baseline | Evidence / limitation |
|---|---|---|
| mzML TIC/BPC/XIC and scan access | IMPLEMENTED | parser/processing and fixture tests; baseline outcome in validation report. No comprehensive vendor or scale certification. |
| Full-resolution integration, display decimation | IMPLEMENTED | Existing interpolation/trapezoidal and extrema-preservation tests. Scientific reference panel still needed. |
| Desktop viewer/presets/comparison | IMPLEMENTED | eframe GUI, embedded tests/software rendering; native interactive smoke test not performed in this audit. |
| Targeted batch peak selection and review | IMPLEMENTED | GUI quant module, unsmoothed area, automatic/manual state, JSON/CSV. No calibration/concentration engine. |
| MCP 32-tool GUI bridge | IMPLEMENTED | Commands, access policy, remote tests and Python example. No headless analytical service. |
| Local vendor conversion | IMPLEMENTED | Fake-converter integration; real instrument tests gated. |
| Version-1 viewer/batch/method persistence | IMPLEMENTED | Validation and round-trip tests; not a unified crash-safe project or provenance DAG. |
| Four engineering documents and narrow Git exceptions | VERIFIED | Local links, Git visibility and diff scope checked; validation report records acceptance. Files are ready to stage, not committed. |

## Ordered delivery units

The initial audit implemented only P0. The 2026-10-08 development session
implements a foundational vertical slice; partial statuses below deliberately
do not certify every acceptance gate. Dependencies use IDs so follow-up work can be
scheduled without a wholesale rewrite. Owners are engineering roles to assign,
not claims of existing staffing. Deliver each unit in a reviewable change.

| ID / owner | Status | Dependencies | Scope and acceptance gate |
|---|---|---|---|
| P0 audit / maintainer | VERIFIED | Existing checkout | Four Git-visible documents, actual baseline results, known gaps and preservation constraints. Gate passed: document links/statuses/diff checked; no production Rust or dependency changes. Documents remain uncommitted. |
| P1a numerical characterization / analytical engineer | IN_PROGRESS | P0 | Freeze current extraction/integration behavior with independently calculable synthetic traces and known scans. Gate: unsmoothed batch areas, interpolated bounds, duplicate RT, profile/centroid and source scan mapping are covered; any semantic change is versioned. |
| P1b identities and domain DTOs / core engineer | IN_PROGRESS | P1a | Newtype IDs, units, method/result/peak models and validating deserialization. Gate: rename/reorder/relink retain IDs; duplicate names allowed where scientifically distinct; NaN/Inf, unsorted arrays, bad lengths and bad references fail cleanly. |
| P1c shared engine / core engineer | IMPLEMENTED | P1b | Move pure quant detect/measure/validate and method types out of GUI; introduce typed operations with compatibility wrappers. Gate: existing GUI tests pass; GUI/CLI/MCP fixture operations yield equal quantities and parameters through the same kernel. |
| P2a project storage / persistence engineer | IN_PROGRESS | P1b | Storage spike then manifest, transactional metadata and immutable artifacts. Gate: source hash checks, single-writer behavior, interrupted-save recovery, corruption rejection, backup/restore and missing-source relink validated on local Windows and cloud-sync paths. |
| P2b migration and provenance / persistence engineer | IN_PROGRESS | P1c,P2a | Legacy session/batch/TOML importer and provenance DAG. Gate: original files preserved; all measurements/units/method snapshots mapped; repeated migration stable; future schemas rejected; hashes and lineage replay validated. |
| P3a job lifecycle / runtime engineer | IN_PROGRESS | P1c,P2a | Bounded scheduler, JobId, progress, terminal states, revision-aware application and cooperative cancellation. Gate: queued/running cancel, stale completion, worker failure, partial batches and restart interruption tests; no silent overwrite of manual review. |
| P3b streaming and budgets / runtime engineer | IN_PROGRESS | P3a,P1a | Replace whole-run spectrum collection with bounded chunks; multi-target scan traversal, disk spill, cache limits. Gate: equivalence within declared tolerances; fixed configured RAM/reader/thread bounds under increasing run sizes; <=2s cancellation outside documented blocking decode; oversized scan behavior tested. |
| P4a CLI and headless build / interface engineer | IN_PROGRESS | P1c,P2b,P3a | Feature boundaries and typed JSON CLI for inspect/extract/quantify/review/export/jobs. Gate: core-only build has no GUI dependency; exit/error codes documented; no display server needed; default desktop still launches. |
| P4b MCP engine adapter / interface engineer | IN_PROGRESS | P4a | Preserve current tool names through adapters, expose jobs/artifacts/revisions and optional view capability. Gate: stdio end-to-end requests with headless and GUI modes; schema/backpressure/paging/idempotency checks; permissions enforced by engine too. |
| P5a targeted quantification / analytical engineer | IMPLEMENTED | P2b,P3b,P4b | Calibration, internal standards, transition qualifiers, dilution/units and range validity. Gate: independent fit/residual/back-calculation references, known spike levels, missing standard/zero denominator and out-of-range fixtures; never export invalid concentrations as valid. |
| P5b QC / analytical engineer | IN_PROGRESS | P5a | Sample roles/order, blank/carryover/drift/precision rules and review flags. Gate: known failures and clean controls classified by saved thresholds; flags retain evidence; exclusion requires attributed decision. |
| P6a external adapter framework / integration engineer | IN_PROGRESS | P2b,P3a | Formalize msconvert first, capability and version handshake, typed imports and resource controls. Gate: wrong schema/version, failed/timeout/cancelled child, malicious paths, malformed output and cleanup tests; adapter version/hash in lineage. |
| P6b MS/MS identification / identification scientist | IMPLEMENTED | P6a,P5b | Bounded shared inspection/processing, MSP/MGF/MassBank index/search, scored overlays, formula/isotope/adduct hypotheses and referenced confidence annotations across GUI/CLI/MCP; see final delivery section. Broader curated negative/isomer panels, confidence calibration, large-library storage and decoy/FDR remain gates; declared DIA fails explicitly. |
| P7a untargeted features / analytical engineer | IMPLEMENTED | P3b,P6a,P5b | Shared local OpenMS 3.5.0 adapter, detection/isotope/adduct grouping, affine alignment, QT correspondence, explicit gap filling, missing states, blank/QC filtering, linked GUI matrix, CLI/MCP, progress/cancellation and verified sample checkpoints. Synthetic reference/transport gates pass; real instrument, nonlinear alignment and measured large-batch RSS gates remain open. See final delivery section. |
| P7a.1 metabolite/lipid hypotheses | IMPLEMENTED (bounded local slice) | P7a,P6b | Shared feature evidence ledger, formula/isotope/local-library proposal adapters, tentative adduct relationships, conservative LIPID MAPS sum/molecular-species labels, competing candidates, reasoned reversible GUI/CLI/MCP review and full-evidence export. Specialist fragmentation/nomenclature, calibrated confidence and legal instrument lipid/isomer panels remain gates. |
| P7b statistics / statistical engineer | IMPLEMENTED_LOCAL_SLICE; scientific/resource gates open | P7a | Frozen tables, explicit preprocessing, PCA/clustering, linked heatmap/volcano, Welch/CI/effects and BH/BY. Local references, GUI/CLI/MCP/project checks below; paired/covariate models, instrument studies and large-matrix storage remain open. |
| P8a unified human/AI review / interface engineer | IN_PROGRESS | P4b,P5b | Revision-bound proposals, reasons, accept/reject/undo and policy gates. Gate: stale proposals rejected; actor and before/after immutable; AI cannot bypass required review via another adapter. Extend to P6b/P7b evidence when available. |
| P8b reporting / reporting engineer | IN_PROGRESS | P8a,P5b | Revision-frozen CSV/SVG and HTML bundles with QC, uncertainty and lineage. Gate: round-trip numbers/units, escaped strings, deterministic artifact hashes, unresolved/partial visibility and approved-only policy tests. Identification/statistical sections depend on P6b/P7b. |
| P9 platform/scientific release / maintainer + scientist | IN_PROGRESS | P8b and all advertised capability gates | OS/MSRV/default/headless/MCP CI, real vendor fixture program and memory/latency benchmarks. Gate: reproducible build and support matrix, documented scientific tolerances, license inventory, restore drills and native UI smoke tests. |

The critical foundational path is P0 -> P1a -> P1b -> P1c, then project/provenance
and job foundations, then interfaces. Targeted QC is the first scientific
product increment; untargeted and identification can be delivered independently
once their common foundations are verified. Advanced reporting sections remain
unavailable until their supplying module is verified.

## First implementation session

1. Read the handoff; rerun baseline on the intended target and record results.
2. Add independently calculated characterization cases for the domain seam,
   including deserialize validation, without changing existing numeric outputs.
3. Extract the pure quant types/detection/measurement/validation to a core module
   in a small PR; keep GUI orchestration and persistence compatibility wrappers.
4. Establish persistent IDs and canonical method revisions before expanding
   remote tool surface or introducing calibration/stats.

Do not begin by replacing the GUI, adding an embedded model, adopting a new
workflow framework, splitting every module into crates, or upgrading libraries.
The roadmap is intentionally broader than the current audit authorization.

## Validation infrastructure and datasets

Reuse Cargo tests and the current mzML fixture. Add small synthetic/publicly
redistributable fixtures only when they validate a new behavior; include origin,
license, representation, expected outputs and tolerances. The existing profile
test relabels centroid arrays as profile and verifies stored-array behavior; it
is not a real profile-resolution scientific validation set.

P1 reference tolerance: exact IDs/parameters/scan mappings; unchanged algorithms
should match existing outputs. For independent area comparisons, start with
absolute 1e-6 or relative 1e-6 (whichever is larger), then justify tolerances from
source precision and scale. Calibration and identification thresholds require
scientist approval and saved validation protocols. Do not choose thresholds to
make tests pass. Scale tests measure process peak memory, queue bytes, reader
count, throughput and cancellation latency with declared hardware/input sizes.

Future CI additions: default-feature tests, true Rust 1.88 build, Windows/macOS
tests, headless feature builds, stdio workflow, legacy migration fixtures and
bounded-memory regression jobs. Keep expensive/vendor checks opt-in or separate
from fast CI, but publish their results before claiming vendor support.

## Dependency decisions and blockers

No new package is required by this audit. UUID/hash libraries are candidates for
P1b/P2b; SQLite for P2a; CLI parser for P4a. Columnar/matrix libraries and external
analytical engines are deferred to the modules that prove their need. Record
license, MSRV, platform support and rollback for every addition.

| Validation prerequisite | Status | Owner / unblocking action |
|---|---|---|
| Real vendor conversion validation | BLOCKED | Maintainer supplies compatible msconvert installation and licensed local instrument data, then runs the two ignored integration tests. This blocks vendor verification, not the shared engine design. |
| OS/MSRV verification | NOT_STARTED | Maintainer runs matrix on Linux/macOS and Rust 1.88; Windows 1.99 alone cannot establish this. |
| Scientific reference panels for new modules | NOT_STARTED | Scientist selects legal fixtures and locks expected methods/metrics before marking modules VERIFIED. |

## Risks to track through delivery

R1: Whole-run XIC collection and trace duplication cause input-sized memory growth
(P3b). R2: cancellation latency/stale workers (P3a). R3: session IDs and names
misassociate edits/results (P1b). R4: direct JSON writes and temporary converted
runs threaten durable recovery (P2a/P2b). R5: current area semantics and duplicate
RT averaging can change during refactoring (P1a). R6: limited fixture diversity
and metadata assumptions conceal instrument-specific failures (P9). R7: UI-thread
scan I/O stalls view and MCP (P3a/P4b). R8: external libraries/engines have licensing,
platform and schema constraints (P6a). R9: claims of AI identification or quantitative
validity outrun evidence (P5-P8). R10: local SQLite and cloud-sync locking require
measured recovery behavior (P2a). R11: release abort-on-panic precludes graceful
worker recovery (P9). R12: documentation and release packaging drift (P0/P9).

## Foundation delivery record — 2026-10-08 (uncommitted)

Implemented shared pure presets/quantification, typed analytical engine, UUID
project/dataset/method/result/operation/job identities, RT/area units, public
boundary validation and structured transport errors. Added headless CLI,
separate headless MCP capability, append-only local project revisions/artifacts,
source hashing/relink, immutable method import, provenance and bounded cooperative
workers. Default desktop and legacy MCP interfaces are retained. XIC now streams
spectra rather than collecting an acquisition of peak arrays. Existing scan maps,
RT duplicate averaging, smoothing and chord-baseline numerical kernels are retained.

P1c is IMPLEMENTED, pending the broader method/result identity and interface gates.
P1a/P1b and P2–P4 remain IN_PROGRESS: reference-panel coverage, full units/IDs,
viewer/batch migration, binary artifacts, backup/recovery, cloud-sync locking,
durable jobs, byte budgets/spill, idempotency, paged engine MCP, revision-bound
review and project GUI controls are outstanding. No later scientific phase is
implemented or implied. See the validation report for actual executed checks.

Next delivery: connect project source/method/result revision models to GUI session
and batch restore; import legacy viewer/batch files with stable mappings and
original automatic/manual results. Add recovery/backup and durable failed/cancelled
job history before claiming P2/P3 complete. Add a binary artifact representation
and measured RSS/cancellation tests before claiming bounded total memory.

## Advanced chromatography delivery - 2026-10-08 (uncommitted)

| ID / owner | Status | Scope and remaining acceptance gates |
|---|---|---|
| P5c chromatographic processing / analytical engineer | IMPLEMENTED | Shared versioned kernel; configurable SG with shifted polynomial edge windows and irregular RT; MAD noise; none/chord/AsLS/rolling-quantile baseline; prominence/SNR/FWHM gates; discrete apex, interpolated bounds and signed trapezoids; resolved overlap valley partitions; retained signal stages; manual/reference integration, batch operation, preview/review/restore revisions; GUI controls, CLI and headless MCP, JSON/CSV export. Synthetic analytic and independent SciPy/NumPy comparisons, actual CLI/MCP and egui interaction tests provide local evidence. Native/real instrument panels, large-run responsiveness, unresolved Gaussian/EMG deconvolution and automatic reference alignment remain open. |

This task is an independently delivered scientific vertical slice on the shared
foundation; it does not mark P2/P3/P4/P5a/P8 complete. Existing legacy detector,
moving average and chord-area semantics remain callable. Advanced settings are
explicit and immutable once saved; originals and corrections remain separate.
Reference-assisted integration means user-supplied bounds plus an explicit shift,
not an inferred reference match. Resolved overlaps conserve observed partition
area; unresolved components are not separately identified or fitted.

Implementation locations and reproducible usage are in the final architecture
section. Test fixtures include a regeneration script and SciPy/NumPy versions;
validation results and deferred gates are recorded in the validation report.
The next bounded improvements are cancellable background processing for the
advanced GUI panel, durable parameter/review revisions with timestamps and IDs,
and a scientist-selected instrument/reference panel before component fitting.
Retain old batches and all signal stages during those changes. Do not silently
replace this detector with Gaussian/EMG models or treat valley areas as isolated
component areas. No commit, push, publishing or source-data deletion occurred.


## Targeted small-molecule delivery - 2026-10-08 (uncommitted)

P5a is IMPLEMENTED for the shared raw mzML -> standards/IS calibration -> reviewed
concentrations vertical slice across desktop GUI, CLI and headless MCP. This is
local numerical/software acceptance, not real-assay certification or completion
of upstream P2/P3/P4 gates. Source code: `src/targeted.rs`, engine/domain adapters,
`src/gui/targeted.rs`; tests: `tests/targeted.rs`, generated raw mzML helper and
27 independent NumPy least-squares/root reference cases. Dependencies: nalgebra
0.33.3 (BSD-3-Clause, optional macro/glam dependencies disabled; std only); test
base64 reuses the existing locked package. Current Windows toolchain compiles it;
Rust 1.88/Linux/macOS compatibility remains a separate verification gate.

Delivered sample roles/order, nominal levels, reasoned exclusions, dilution and
explicit units; quantifier/qualifier extraction and RT/ratio gates; internal
standard pairing/area flags; linear/quadratic/optional cubic weighted calibration
with intercept choices; domain-only inverse roots, range/LOD/LOQ states, residuals,
back-calculated accuracy, standard/QC replicate SD/CV and regression diagnostics.
Retained raw evidence, snapshots, source hashes, UUID/timestamp batch/review
history, stale/corrupt rejection, GUI plots/review, lossless JSON and create-new
concentration/calibration CSV exports. Projects register all batch sources.

P5b remains IN_PROGRESS: blank contamination, qualifier coelution, IS response,
accuracy and precision are implemented; carryover sequences, pooled-QC drift and
broader assay policy are not. Next practical work: direct targeted boundary
correction/refit revisions and assay acceptance gates, CSV sheet import, dedicated
qualifier/IS controls, cancellable replay/exports, fused multi-ion extraction and
binary artifacts/durable partial jobs. Scientist-selected legal instrument spike
panels and a saved acceptance protocol are prerequisites for wider scientific
verification. No fabricated validation, threshold relaxation or raw-data mutation.

## Batch QC / method-validation delivery - 2026-10-08 (uncommitted)

P5b is IMPLEMENTED for the evidence-based software vertical slice. The shared QC
module supports 24 calculations, explicit method-specific acceptance rules,
required/optional gating and indeterminate evidence; retained targeted adapters;
separate designed method-validation studies with recovery/matrix/process/
selectivity/stability comparisons; configurable weighted calibration performance;
GUI dashboards/control and injection-order charts, historical evaluations and
reasoned review; CLI/MCP and full-evidence JSON/CSV exports. Every decision
retains calculation, observations, bounds, n and reason. Reviews never override
analytical acceptance, and all targeted raw evidence and previous results survive.

Local validation uses Python stdlib statistics/exact rational reference examples,
27 independent NumPy calibration cases, known failing batches, raw generated mzML,
actual CLI/MCP, project source verification and headless egui interaction/render.
This status is implementation with local numerical evidence, not certified assay,
regulatory, real instrument or native platform validation. P2/P3/P4 remain open.

Next gates: scientist-approved legal multi-run/lot/preparation panels and saved
acceptance protocols; worker-based cancellable QC replay; direct mass centroid
observations; CSV study sheets; inter-run variance components, normalized paired
matrix factors and uncertainty where supported by explicit experimental design.
Keep generic absent design indeterminate and preserve current artifacts/history.
Detailed equations, interface schemas and limits are in the final architecture
section; observed commands/results are in the final validation report.
## Advanced spectra / compound identification delivery - 2026-10-08 (uncommitted)

P6b is IMPLEMENTED for the bounded local inspection -> processing -> indexed
library search -> scored overlays -> evidence-based annotation software slice.
Shared `spectral` code supplies GUI, CLI and headless MCP. Delivered: MS1/MS2 and
isolation inspection; mzsignal profile centroiding; retained averaging/background
subtraction; one-to-one cosine fragment matching; metadata/license-preserving
MSP/MGF/MassBank import and precursor index; compatibility/quality filters, ranked
alternatives and annotated peaks; isotope spacing and optional CHNOPS nominal
abundance patterns; bounded formula/adduct hypotheses; source-verified linked
precursor XIC; conservative confidence, immutable review history, JSON and
create-new candidate CSV. Projects replay numerical evidence and verify registered
scan hashes before new commits. P6a and earlier durable-storage/job gates remain
open; this vertical slice does not claim they are complete.

Confidence never increases from similarity alone. Confirmed/probable/class states
require explicitly referenced reviewer evidence; absence stays Unknown. All
mass/energy tolerances, score components, source/license/version/hash, raw spectra,
preprocessing, exclusions, alternatives and reviewer attestations survive. Real
CC BY MassBank fixtures, independent Python reference calculations, synthetic
adduct/isobar/false-match tests, actual CLI/MCP and desktop interaction provide
local evidence; exact executed checks are in the final validation report.

Remaining P6b scientific gates: curated legal multi-compound unknown/negative,
isomer and adduct panels under multiple instruments/energies; scientist-approved
confidence protocol and standard evidence; decoy/FDR if introduced. Remaining
engineering gates: disk-backed large libraries/verified cache, cancellation within
kernels and synchronous GUI load/replay/export, fine-isotope models, DIA/purity
and coelution analysis with appropriate reference data. Known greedy non-optimal
assignments, nominal isotope limits and closed-shell CHNOPS restrictions are
explicit; none are replaced with fabricated results. Preserve earlier stage APIs,
raw data, immutable histories and missing compatibility states while extending.

## Untargeted LC-HRMS delivery — 2026-10-08 (uncommitted)

P7a now has a working multi-sample mzML → inspectable feature matrix vertical
slice across GUI, CLI and headless MCP. `src/untargeted.rs` defines typed sample
roles/metadata, configuration, matrix cells, explicit missing/filter states,
MS/MS evidence, alignment records, independent integrity replay and CSV exports.
The shared engine dispatches `untargeted_batch` and `export_feature_matrix`.
`src/adapters/openms_metabo.py` is embedded in the Rust binary and implements
protocol `openms-metabo-v1`, strictly requiring local pyOpenMS 3.5.0. No automatic
installation, network submission or raw-source mutation occurs.

OpenMS supplies MassTraceDetection, ElutionPeakDetection, FeatureFindingMetabo,
MetaboliteFeatureDeconvolution, MapAlignmentAlgorithmPoseClustering and
FeatureGroupingAlgorithmQT. Positive and negative runs use separate declared
batches, signed charges and appropriate adduct hypotheses. Profile centroiding
is explicitly enabled with PeakPickerHiRes; unspecified/profile MS1 otherwise
fails. Unknown polarity and absent full-scan MS1 fail rather than implying data.
Algorithm defaults plus every applied parameter, Python/OpenMS/adapter version,
script hash, source hashes, feature/consensus XML checksums, exact JSON numeric
checkpoints, launch request, stdout/stderr and terminal outcome are retained.

RT is seconds in this module, unlike the preserved legacy minute APIs. Affine
alignment records raw/reference landmarks, slope, intercept, residuals and RMS.
Raw RT remains unchanged. A blank without two landmarks is explicitly unaligned:
its raw checkpoint survives, cells are `alignment_unavailable`, and blank
filtering is indeterminate. Other alignment failures remain errors. QT links
aligned ions within configurable m/z ppm and RT seconds tolerances; adduct groups
are hypotheses and never collapse or replace original ion measurements.

Matrix intensity is unsmoothed monoisotopic EIC trapezoidal area in
intensity*seconds, using detected hull bounds or the explicitly projected gap
window. Original OpenMS monoisotopic FWHM area remains in `openms_intensity`.
Gap filling is opt-in, requires consecutive above-threshold scans, stores its
window/EIC, and labels filled values separately. Missing classifications include
below threshold, insufficient support, outside acquisition, disabled gap filling
and unavailable alignment. Missing intensity is null; zero is never substituted.
Blank median/sample median ratios, pooled-QC sample CV (at least three measured
QCs) and prevalence retain numbers, flags and included/excluded/indeterminate
states. All rows survive; an included-only export is a reversible view.

The GUI retains independent run history, pages 100 features, links individual
cells to sample metadata/EIC/apex MS1 and associated MS/MS plots, and exports new
JSON/CSV paths. CLI uses the same engine and provides matrix/filtered/observation
exports; Ctrl+C requests cooperative cancellation. MCP adds asynchronous start,
status, cancel and evidence paging, while existing analytical_operation remains.
Every MCP input and checkpoint directory is root-authorized. Completed sample
checkpoints are content/parameter/version-addressed and checksum-verified;
restart recomputes alignment/correspondence/evidence from these checkpoints.
Incomplete attempt directories stay explicit. XML text precision is not the
numeric authority: exact JSON values restore warm-cache correspondence.

Validation covers six-run positive and negative synthetic mzML references,
known Gaussian signals/affine drift, isotope envelopes, absent/narrow peaks,
false intermittent ions, blank contamination, QC variation, associated MS/MS,
independent window-area calculations, cache replay/corruption/budgets,
disabled filling/outside acquisition, real child cancellation, actual CLI/MCP,
project source registration and GUI cell selection/error-history retention.
Independent positive H/Na and negative H/Cl grouping references also pass.

Remaining gates: scientist-selected legal instrument reference mixtures and
unlabelled studies; nonlinear/low-landmark alignment; interference-aware gap
peak fitting; fine isotope chemistry; DIA/chimeric deconvolution and confidence;
disk-backed feature/evidence matrices, bounded RSS/disk benchmarks and durable
job IDs across restart; native dialogs/Ctrl+C/platform/MSRV/OneDrive recovery.
OpenMS owns one whole run at a time, and all feature maps plus retained window
evidence remain in memory. Limits/checkpoints do not establish bounded memory.
P6a's general adapter framework and the earlier storage/job gates remain open.
Exact final executed checks are appended to VALIDATION_REPORT.md.

Final local checks: formatting, strict all-target/all-feature Clippy, compilation,
315 default all-feature tests, 225 default headless MCP tests, one explicitly
executed OpenMS reference test, five Python references and all four binaries pass.
The scientific runtime test is opt-in, not silently counted from an ignored run.
Remaining real-instrument/resource/native gates remain open as described above.


## Feature annotation delivery � 2026-10-08

P7a.1 adds real annotation proposals and a review/export path to retained feature
matrices. Shared spectral chemistry and local library adapters preserve source
text, versions, spectra and parameters; no duplicate specialist search or
fragmentation implementation. Formula/isotope reports and response operations
replay before export/commit; registered raw hashes are required for project
commits. Accept is a tentative hypothesis decision, never compound confirmation.

Conservative ester lipid labels distinguish sum composition from unordered
molecular species; ambiguous PC 34:1 chains and structural isomers remain
separate hypotheses. Unsupported positional/linkage/class claims fail explicitly.
Tests cover hierarchy, isomers, H/Na neutral relationships, incorrect adduct/charge,
conflicting formula/MS/MS evidence, stale review, corruption, history, actual
CLI/MCP exports and egui review clicks. Final executed commands/counts are in
VALIDATION_REPORT.md. The local workflow helper is examples/annotation_workflow.py.

Next: specialist Goslin/SIRIUS/lipid-engine adapters with pinned schema/license
and legal reference outputs; broader lipid classes/ether/oxidized species;
scientist-curated instrument isomer/adduct/lipid panels and an explicit
confidence protocol; cancellation within kernels, disk-backed shared library
storage and native UI validation. General P7b statistics remains not started.

Final annotation gate results: 325 all-feature and 234 GUI-free tests pass;
strict formatting/lint/compilation and all binaries pass; software GUI review and
independent helper CSV/no-overwrite checks pass. The same three opt-in tests
remain ignored. These are local software/reference gates, not instrument identity
validation or comprehensive specialist lipid-engine support.


## P7b local statistical workspace delivery — 2026-10-08

Implemented deterministic descriptive/univariate analysis through one engine for
GUI/CLI/MCP: frozen tables, explicit preprocessing/exclusions/imputation, PCA,
Euclidean hierarchical clustering, linked heatmaps/loadings/scores/volcano, Welch
effects/CI and BH/BY. Quantitative CSV plus metadata/settings, retained untargeted
matrices and targeted concentrations have reproducible analysis/export paths.
Raw quantities/evidence and earlier GUI/project revisions survive each analysis;
strict replay rejects changed results. Known examples and independent mpmath
references cover inference/eigenvalues, constant/all-missing/rank-deficient cases,
preprocessing and clustering. CLI/MCP/export/project-history and software GUI
selection are checked; exact final commands/counts are in VALIDATION_REPORT.md.

Remaining gates: scientist-selected instrument studies and predeclared design/
normalization/imputation policies; paired/covariate/repeated-measure models with
independent references; interactive metadata/exclusion grids; source-linked
targeted spectra where actual evidence exists; large disk-backed matrices/bounded
RSS; asynchronous GUI import/replay/export, durable jobs, portable pinned-runtime
packaging and native/platform checks. No predictive validation or biological/
instrument certification is claimed. The earlier annotation-stage statement that
general P7b had not started is historical and superseded by this delivery.


## MCP analytical expansion — 2026-10-08

Delivered a substantial P4b/P8 interface increment: all existing shared-engine
operations have nested discoverable typed schemas; generalized retained jobs,
polling/cancellation, immutable result/text paging, dataset discovery, project
inspection, explicitly enabled revision-bound commits, six agent workflows,
attributed audit and checksummed reproducible JSON review drafts. Preserved
legacy MCP tools and all existing numerical/GUI/CLI/mzML boundaries.
Actual stdio example: examples/mcp_targeted_workflow.py; contracts/prompts:
docs/MCP_ANALYSIS.md; raw targeted protocol/reference tests: tests/engine_mcp.rs.

P8a/P8b remain PARTIAL, not complete: authenticated human approval/policy across
adapters, durable general jobs/full audit, idempotency, approved-only release
and HTML/PDF bundles are not implemented. Project create/register/relink/restore
remain CLI; draft reporting is content-only. No instrument/regulatory/RSS or
OneDrive recovery certification. Final executed local results appear in the
validation report and session handoff. No push/publish/data deletion requested.


## P8b reporting and project delivery - 2026-10-08

Delivered shared configured JSON/HTML inspection drafts; section selection for
metadata/methods/chromatograms/spectra/calibration/quantification/QC/annotations/
statistics/history/software; native CSV and quoted TSV; full-resolution vector
SVG for direct traces/spectra, targeted traces/calibration and chromatography
segments. Portable directory bundles preserve original revision/result bytes,
all historical raw inputs, source relocation, file SHA256 inventory and a local
software source snapshot/build recipe. GUI reopens retained evidence and configures
worker exports/reprocessing; CLI lifecycle/export/replay/method comparison and
MCP content export call the same module/analytical engine. Original work survives.

P8b remains IN_PROGRESS for authenticated approved-only releases, PDF/raster
layout, richer interactive reports, numerical-tolerance replay policies for all
operations, durable storage/resource/native/platform/instrument release gates.
Current acceptance is verified for raw mzML TIC, known triangle integration and
targeted calibrated raw-source workflows; do not claim universal replay for every
UUID-generating/external-adapter operation. Schema-1 defaults migrate as a new
explicit revision; unsupported versions fail without rewriting history. Read
PROJECT_DELIVERY.md for commands, format contracts, runtime/source requirements
and limitations. Final observed checks are in VALIDATION_REPORT.md.


## P9 performance/reliability verification increment - 2026-10-08

P9 is IN_PROGRESS. Delivered opt-in real-kernel benchmarks for 23 MB, 231 MB
and 1.16 GB synthetic mzML, CPU/RSS/I/O process-tree sampling, repeat-reader
extraction, two-reader concurrency, running cancellation, 64-sample targeted
calibration, integration, chromatography, 10k-entry spectral libraries, synced
project revisions/restore and cold/warm OpenMS detection/alignment checkpoints.
Independent reference regeneration preserves original fixtures and records hashes.
See [PERFORMANCE_VALIDATION.md](PERFORMANCE_VALIDATION.md) and the prioritized
[REMAINING_DEFECTS.md](REMAINING_DEFECTS.md); compact observed evidence is retained
in docs/performance/2026-10-08. Full test/build results are in VALIDATION_REPORT.md.

Fixed mixed missing TIC/BPC metadata, malformed-array false zeros/panics, RT
extrema, atomic scan admission and actual-byte hashing limits. Release workers
can unwind into structured failure. Raw outputs now use extraction-v2, retaining
prior revisions and refusing version-mismatched replay. No scientific algorithm
or original reference was replaced. Metadata-only bounds traversal and linear
fallback lookup reduce avoidable work; validation cost and noisy/contended
performance observations remain explicit.

Raw RT units now receive streaming mzML/gzip preflight and complete missing-window
peak bounds; direct gzip opening is repaired. Open gates: broader source-schema/
decompression/token-bound fuzzing, durable killed-job recovery and abandoned
writer-lock handling, bounded disk-backed evidence/cache history, controlled
release repetitions and worst-case workloads, real instrument scientific panels,
native GUI/vendor/MSRV/platform/OneDrive certification. P9 and foundational
P2/P3 resource/durability gates are not marked complete.


## Comprehensive development release audit - 2026-10-08

See [FEATURE_STATUS.md](FEATURE_STATUS.md) for current bounded capability and
GUI/CLI/MCP access evidence; historical baseline tables are labelled explicitly.
P6a is IN_PROGRESS because concrete msconvert/OpenMS/statistics subprocess
integrations exist, while the common framework remains incomplete. P9 is
IN_PROGRESS because performance/reliability/reference infrastructure exists;
platform/MSRV/native/vendor/instrument acceptance is incomplete. IMPLEMENTED
analytical slices do not satisfy all original scientific/resource acceptance
gates and are not promoted to VERIFIED by this documentation audit.

Release preparation adds README architecture/install/runtime/workflows/examples/
MCP/reproducibility/limitations, examples/README.md, real CLI release_workflows.py
and a no-overwrite/missing-concentration/QC-history/reference-search regression.
Release notes and prioritized backlog: RELEASE_NOTES.md and RELEASE_BACKLOG.md.
Existing delivery/performance/defects guides are made Git-visible; originals and
scientific references remain intact. CI now declares pinned scientific reference
runtimes and GUI-free/interface/reference checks; hosted CI remains unexecuted.
Final commands/results appear in VALIDATION_REPORT.md; preparation is not a
published release and no push/tag/publication is authorized or performed.


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


### P9 measured verification checkpoint - 2026-10-09

Comprehensive software/performance pass has documented measurements through
1.157 GB synthetic mzML; chromatograms, integrations, calibration, batch processing,
OpenMS detection/alignment/cache, spectral search and project operations are
measured with sampled CPU/RSS/I/O and concurrency. Latest suites pass (379
all-features / 282 headless), strict lint/format/build pass, independent numeric
references and native OpenMS checks pass. See VALIDATION_REPORT.md for reproducible
commands and PERFORMANCE_VALIDATION.md for measured evidence/limitations. P9
remains IN_PROGRESS for the remaining-defects register: CV groups/schema/resource
limits, durable recovery, GUI responsiveness, isolated release/platform/vendor
performance and representative instrument method validation.
