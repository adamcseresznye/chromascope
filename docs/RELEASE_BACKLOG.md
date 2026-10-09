# Prioritized release backlog

Recorded 2026-10-08. These items remain after development-release preparation.
Priorities indicate practical release order; no scientific gate is waived by
passing software tests. Detailed existing findings: [REMAINING_DEFECTS.md](REMAINING_DEFECTS.md).

| Priority | Work / owner role | Acceptance evidence |
|---|---|---|
| P0 development-release gate | Stabilize one source snapshot and restore all-feature acceptance; maintainer | Full suite and builds now pass on their recorded slices. Strict lint, formatting and headless reruns pass. Bind one source snapshot to all artifacts with sufficient Windows disk/PDB resources. Do not infer stable snapshot acceptance from overlapping runs. |
| P0 before scientific release | Raw source/unit integrity; analytical engineer | New plain/gzip/time-unit/interior-bound fixtures exist in concurrent source work and are included in final checks. Remaining: bounded XML event/decompression handling, schema/binary/fuzz corpus, source mutation checks and instrument metadata panels. |
| P0 before scientific release | Instrument/reference study; scientist + maintainer | Licensed representative profile/centroid/vendor runs, blinded compounds/interferences/isomers, spike calibration/qualifier/QC studies, predeclared tolerances and independently reviewed results. No identities inferred from self-search examples. |
| P0 before operational release | Reproducible support matrix; maintainer | Windows/Linux/macOS and Rust 1.88/default/headless/all-feature CI, native dialogs/view/export smoke, opt-in vendor data and pinned runtime compatibility. Execute hosted CI; local inspection is insufficient. |
| P1 | Durable jobs/audit and killed-save recovery; runtime/persistence engineer | Restart states, fault injection at write/checkpoint stages, abandoned-lock evidence-based recovery, concurrent writers and local/OneDrive restore drills. Preserve originals. |
| P1 | Unified authenticated human/AI review; interface engineer | Revision-bound proposals, immutable actor/reason/history, stale rejection and approval policy enforced across every adapter; approved-only report release. |
| P1 | Disk-backed evidence/history and budgets; runtime engineer | Bound bytes across traces/libraries/matrices/history/serialization, eviction/disk spill, growing input and concurrent-reader tests; measured peak RSS and cancellation under adversarial workloads. |
| P1 | Portable verified runtime/software delivery; maintainer | Reproduce a clean snapshot build with locked Rust/Python artifacts and license inventory; verify binary/source evidence; preserve originals through relocation/replay. |
| P2 | Combined multi-target extraction and cached verified libraries; analytical engineer | Profile real target counts, retain exact quantities/source-change errors and independent reference tests; never optimize by weakening validation. |
| P2 | Async GUI import/replay/verification/export; interface engineer | Cancellable background execution, stale-result routing, native responsiveness under large retained evidence; coherent missing/failure views. |
| P2 | Identification negatives/isomers and confidence; scientist | Curated panels, metadata mismatch controls, decoy/FDR or calibrated confidence only after independent validation; keep unknown states. |
| P2 | Untargeted alignment/gap fidelity; scientist | Profile/nonlinear references, interference-aware gap fitting, real-sample missingness/false-feature criteria; raw/aligned and detected/filled distinctions retained. |
| P2 | Rich report layout and replay policy; reporting engineer | PDF/raster visual QA, supported figures and approval gates; operation-specific numeric tolerances justified independently, generated-ID exclusions explicit. |
| P3 | Broader statistics; statistician | Paired/covariate/repeated models and independent reference/design tests; metadata/exclusion grids, declared multiple-testing family and imputation caveats. |
| P3 | Specialist lipid/nomenclature and DIA adapters; scientist | Established compatible libraries, provenance/timeout/cancel contracts, real fragment/deconvolution references. No placeholder scientific outputs. |
| P3 | Common external adapter framework; integration engineer | Shared capability/version/lifecycle/resource protocol for msconvert/OpenMS/statistics; malformed output, timeout, cancellation and provenance tests. |

A development snapshot may be reviewed with these limits disclosed. A general
scientific/platform-ready release requires the relevant P0/P1 gates, an explicit
reviewed version/tag and user authorization before publication. No publication
or source/data deletion is part of this task.
