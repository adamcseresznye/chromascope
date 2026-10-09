# Development feature-status matrix

Audited 2026-10-08 against the working tree, not just Git HEAD. Package 0.3.0;
existing uncommitted implementation is included. Status applies to the bounded
scope named in each row, not instrument certification or full roadmap acceptance.

- **Fully implemented**: the named local slice has code, an accessible interface
  and automated evidence; broader validation gates can still be open.
- **Partially implemented**: substantive code exists but the named acceptance
  contract or workflow is incomplete.
- **Integrated**: an established external library/engine performs the algorithm;
  runtime/setup and validation limits remain explicit.
- **Experimental**: evidence exists for a narrow hypothesis/fixture or prototype,
  without general scientific confidence or operational certification.
- **Not implemented**: no accessible implementation meeting the named scope.

GUI workspaces below were traced from `gui/mod.rs`, `gui/workbench.rs`, `gui/quant.rs`
and the workspace modules. Current navigation is Data explorer, Quantification
and QC, Identification, Untargeted analysis, Statistics, Reports and AI activity;
workspace window titles can differ. CLI engine operations use `run SOURCE` with a tagged
`domain::Operation`; headless MCP uses `start_analysis` or `analytical_operation`.
That shared operation route is abbreviated **Engine**. Desktop MCP retains its
32 legacy viewer/batch tools; advanced operations use the headless interface.
Automated GUI rendering/click tests do not certify native dialogs.

| Roadmap / capability | Status | GUI access | CLI / MCP access | Implementation and evidence / remaining gate |
|---|---|---|---|---|
| P1a mzML / mzML.gz metadata, TIC/BPC/XIC, scan access | Fully implemented | Viewer file/plot controls | Engine metadata/spectrum/extract; desktop MCP | parser/processing; engine/reliability tests. New plain/gzip raw-unit and interior-bound regression fixtures; complete schema/decompression certification remains open. |
| P1a full-resolution trapezoids/interpolated boundaries, decimation | Fully implemented | Integration gestures and batch | Engine integrate/quantify; desktop MCP | processing/quant; kernel and fixture tests. Raw versus display values preserved. |
| Viewer overlays/grid/stacked, axes, presets, CSV/SVG | Fully implemented | Retained workbench and preset controls | Desktop MCP view/export tools; method CLI import | gui/workbench/workspace/presets; embedded tests. CLI has no window layout. |
| P1c shared analytical engine | Fully implemented | Workspace adapters and shared legacy kernels | CLI / headless MCP Engine | domain/engine/quant and analytical modules; adapter parity tests. Presentation remains GUI-specific. |
| P1b IDs/units/validation/missing states | Partially implemented | Retained results and flags | Engine DTOs/errors | domain + module validators; malformed/stale tests. Not all designed entities have durable typed IDs; full source-schema validation gate open. |
| Local ProteoWizard conversion | Integrated | Import controls | Desktop MCP opening supported paths | import; fake converter tests. Real vendor/runtime tests are opt-in. |
| P2a append-only project snapshots/hashes/restore | Partially implemented | Project create/open toolbar, workspace snapshots, reports and delivery | project-* CLI; inspect/commit MCP | project/delivery tests: source/revision/checksum/relink/restore. Crash/OneDrive/restart/abandoned-lock gates open. |
| P2b legacy viewer/batch/method migration and provenance | Partially implemented | Existing sessions remain loadable | project-import-method and project-migrate | project tests. Limited importer/schema defaults, not the complete designed provenance DAG. |
| P3a bounded workers, progress/cancel/stale revisions | Partially implemented | Workspace workers/cancel | CLI Ctrl-C; MCP status/cancel | jobs + protocol tests. General jobs/audit are session-local; some GUI operations synchronous. |
| P3b scan streaming, byte/scan admission | Partially implemented | Shared extraction | Engine | parser/jobs/reliability/performance tests. No disk spill or bounded-RSS retained history; repeated target traversal. |
| P4a analytical CLI / GUI-free build | Partially implemented | N/A | chromascope-cli; no-default-features | Actual CLI integration and headless tests. No durable general job-control CLI or complete operational lifecycle. |
| P4b original desktop MCP | Fully implemented | Live window | chromascope-mcp, 32 legacy tools | mcp/gui/remote tests. GUI thread required. |
| P4b general analytical MCP/discovery/schema/paging | Partially implemented | Headless; optional separate desktop | chromascope-engine-mcp, discovered tool schemas | engine_mcp tests and real stdio helper. No idempotency, durable audit/jobs or native progress notifications. |
| P5c SG / MAD / chord / AsLS / rolling quantile / peak gates | Fully implemented | Batch chromatography controls | Engine process/revise/export chromatograms | chromatography; independent NumPy/SciPy reference and GUI tests. No Gaussian/EMG unresolved-peak deconvolution. |
| P5c reversible manual/reference boundaries and previews | Fully implemented | Batch review | Engine revise_chromatogram | Immutable revisions, preview commit rejection and stale checks. Automatic reference alignment absent. |
| P5a weighted linear/quadratic calibration, dilution | Fully implemented | Targeted concentrations section | Engine targeted_batch/review/export | targeted; independent fits and raw triangular standards/unknown tests. Scientific study gate open; invalid concentration stays null. |
| P5a qualifiers/internal standards/range/LOD/LOQ flags | Fully implemented | Targeted evidence and review | Same targeted operations | targeted tests; explicit settings. No automatic LOD/LOQ study design. |
| P5b accuracy/precision/blank/carryover/drift/QC studies | Partially implemented | Batch QC and method validation | evaluate_qc/evaluate_targeted_qc/validate_method/review_qc/export_qc | qc: 24 metrics, independent references, reasoned reviews. Operator-designed observations/rules; instrument method validation absent. |
| P6a common external-adapter framework | Partially implemented | Existing import/workspace adapters | Existing local runtimes | msconvert, OpenMS and statistics adapters exist independently. Common lifecycle/capability framework not delivered. |
| P6b spectral processing/averaging/background/overlays | Fully implemented | Spectra and compound identification | inspect/process/compare/linked spectral operations | spectral + mzsignal; native/raw and synthetic tests. Declared DIA fails explicitly. |
| P6b local MSP/MGF/MassBank import and DDA search | Fully implemented | Spectral library workspace | import/search/export_spectral operations | spectral/library; retained MassBank + independent reference tests. Bounded local library, no decoy/FDR or broad isomer calibration. |
| P6b formula/isotope/adduct hypotheses | Experimental | Spectral workspace | formula_candidates/analyze_isotopes | spectral/chemistry; known masses/reference tests. Tentative evidence, not calibrated identification. |
| P7a detection/isotope/adduct grouping/alignment/QT correspondence | Integrated | Untargeted LC-HRMS | untargeted_batch; MCP feature jobs | untargeted + local pyOpenMS 3.5.0. Positive/negative synthetic opt-in reference; no general nonlinear alignment or instrument certification. |
| P7a labelled window gap filling/blank/QC filters | Experimental | Linked retained feature matrix | export_feature_matrix; feature CLI exports | Explicit window trapezoids and missing states; independent replay tests. No interference-aware gap fitting or certified scale. |
| P7a.1 reversible feature annotation ledgers | Fully implemented | Annotation inside retained matrix | propose/add/review/export_feature_annotations | annotation tests; competing hypotheses and preserved evidence. Acceptance does not promote confidence. |
| P7a.1 conservative lipid sum/chain labels | Experimental | Feature annotation drafts | Annotation engine DTOs | annotation restricted classes and hierarchy tests. No sn/DB/stereochemistry or specialist fragmentation inference. |
| P7b explicit preprocessing/PCA/clustering/Welch/CI/BH/BY | Integrated | Statistics and quantitative omics | analyze_statistics/export_statistics | statistics + local NumPy/SciPy; independent mpmath references. Bounded local two-group slice, not paired/covariate models. |
| P7b raw-linked heatmap/volcano/PCA/history restore | Fully implemented | Statistics workspace | Retained JSON/CSV through Engine | statistics/gui tests. Links require actual retained raw evidence; generic tables have none. |
| P8a reasoned reviews/revision checks/attributed audit | Partially implemented | Analytical panels and experimental AI activity proposal review | Engine reviews; MCP proposal/resolve/commit/audit | project/analytical/proposal tests. Actor attribution is not authentication; no unified human approval policy across adapters. |
| P8b draft JSON/HTML/CSV/TSV/SVG reporting | Partially implemented | Project reports and delivery | project-report/bundle/verify/sources/reprocess/compare-methods; MCP export_project_report | delivery tests. No approved-only release, PDF/raster layouts or universal replay equivalence. |
| P8b portable raw/history/software directory bundle | Fully implemented | Delivery export | project-bundle/verify | Checksummed moved-bundle tests. Directory, not ZIP; original build repository needed for software snapshot. |
| P9 measured synthetic performance/reference infrastructure | Experimental | Test-only previews | reference profiler and opt-in tests | performance/reliability/reference tools. Existing measurements are not release-profile benchmarks or bounded-RSS certification. |
| P9 Windows/MSRV/macOS/Linux/native/instrument release acceptance | Partially implemented | Native smoke pending | CI currently Ubuntu quality only | Local Windows checks available; expanded CI/runtime provisioning and independent platform gates missing. |
| DIA deconvolution / confidence FDR / calibrated identity probability | Not implemented | Explicit unsupported DIA | No implementation | Must not infer identity from scores. |
| Gaussian/EMG unresolved chromatographic deconvolution | Not implemented | No algorithm | No operation | Valley partitions only. |
| Specialist lipid fragmentation/Goslin/SIRIUS remote search | Not implemented | No adapter | No operation | Conservative local hypotheses only. |
| Paired/covariate/ANOVA/repeated/predictive statistics | Not implemented | No method | No operation | Independent two-group Welch only. |
| Durable general jobs/audit and authenticated approvals | Not implemented | No restart/policy UI | No restart/policy API | Prerequisite for approved-only operational release. |
| Disk-backed analytical arrays/history and universal replay tolerance | Not implemented | No storage/tolerance workflow | No contract | Existing limits/replay mismatches are explicit. |
| PDF/PNG/TIFF project report export | Not implemented | No delivery option | No delivery command | Desktop MCP PNG view capture exists separately. |

## Roadmap audit disposition

Early architecture/roadmap baseline statements that calibration, CLI or headless
MCP are absent are historical and superseded by later delivery records and this
matrix. P6a cannot remain NOT_STARTED: three concrete subprocess integrations
exist, though the common framework remains partial. P9 cannot remain NOT_STARTED:
performance/reliability/reference work exists, but release acceptance is partial.
P5a/P6b/P7a/P7b IMPLEMENTED labels mean their named local slices, not every
original gate. No whole scientific stage is promoted to VERIFIED here.

Every `domain::Operation` is dispatched in `engine.rs`, callable through CLI run
and headless MCP typed/legacy analysis routes. Not every operation has a separate
GUI button: workspaces prepare/edit requests or expose relevant review controls.
Project lifecycle commands use `project`/`delivery`, and view-only legacy MCP tools
use GUI adapters. Future capabilities in the design are explicitly absent above.

The [complete operation inventory](ENGINE_OPERATIONS.md) lists every actual
analytical request tag. CLI `project-relink PROJECT DATASET_UUID SOURCE` now
exposes the existing hash-verified core relink with a new immutable revision;
different content fails without committing. Method migration has a CLI command;
legacy viewer/batch sessions remain GUI persistence paths rather than invented
project import commands.

### Late concurrent increments and release acceptance

The checkout continued to change during release validation. New project-workbench,
form/table/plot controls, accessibility configuration and revision-bound review
proposal code are present. Headless MCP exposes proposal preparation/resolution;
these increments are **experimental** until the final integrated checks and native
workflow review pass. Their presence does not promote the broader authenticated
review or complete project-workspace gates to implemented. GUI integration remains
partial where the intended workflow still requires requests or external helpers.

See the final development-release section in VALIDATION_REPORT.md for passing
slices and later failures. A previous passing test run does not certify this
changing checkout or the retained unvalidated source snapshots.
