# Development release notes

Prepared 2026-10-08 for the current uncommitted working tree. Package version
remains 0.3.0; no tag, commit, upload, push or publication was performed.
This document describes repository functionality beyond baseline commit
`ff7975aedfdcd894e31dd1e85dc3707d2cdd01f5`, not a published binary guarantee.

## Available development workflows

- Shared typed analytical engine with CLI and GUI-free stdio MCP; existing desktop,
  viewer MCP and mzML workflows retained. Generated schemas, jobs, discovery,
  cancellation, paging, attributed audit and review drafts support external agents.
- Configurable chromatographic signal stages, peak gates, full-resolution areas
  and reversible boundaries; targeted calibration, qualifiers/internal standards,
  dilution and explicit invalid/missing concentrations; evidence-based QC studies.
- Local spectral processing and MSP/MGF/MassBank search, conservative formula/
  isotope evidence and reversible feature annotation ledgers. Scores and accepted
  hypotheses are not confirmed identities or calibrated probabilities.
- Operator-provided OpenMS feature detection, affine alignment/correspondence,
  labelled window gap filling and explicit filters; local NumPy/SciPy descriptive,
  PCA/clustering and independent two-group Welch/BH/BY statistical workspace.
- Immutable project revisions, source hashes/relink/restore, JSON/HTML drafts,
  CSV/TSV/SVG and checksummed portable directory bundles with raw/history/software
  evidence. Reprocessing retains originals and reports exact-output mismatches.

## Release preparation changes

Rewrote README for actual architecture/install/workspaces/CLI/MCP/runtime/limits.
Added a complete capability/interface status matrix, operational examples guide,
real-CLI targeted/QC/spectral demonstration and regression test. Made existing
project-delivery/performance/defects guides Git-visible through narrow ignore
exceptions. Corrected stale roadmap headings/statuses without discarding history.
CI now declares pinned statistics/reference runtimes, checks the new Python helper,
runs GUI-free tests, builds all interfaces and regenerates independent references.
CI changes were inspected locally; hosted CI was not executed here.

## Compatibility and reproducibility

Existing GUI, desktop MCP, headless MCP, CLI and mzML tests remain in the final
checks. Raw acquisitions and original results are preserved; exports require new
paths. No dependency auto-installation occurs in application workflows. OpenMS
requires pyOpenMS 3.5.0; statistics runtime versions are recorded and strict replay
can require the original runtime. Prior extraction kernel changes are documented
in the architecture/validation history; do not silently equate different kernels.
A dirty checkout is not reproducible from Git HEAD alone: retain this full source,
Cargo.lock, inputs, requests/results and runtime versions.

## Known limitations and release gates

This is suitable for review as a development snapshot, not approved scientific
release. Native platform/vendor/MSRV tests, instrument studies, authentication,
durable general jobs/audit, bounded retained evidence memory and crash/cloud-sync
recovery remain gates. DDA local matching, affine alignment, conservative lipid
hypotheses and independent two-group inference are bounded scopes. PDF/raster
project delivery, DIA deconvolution, identification FDR, specialist lipid engines
and paired/covariate models are absent. See [feature matrix](FEATURE_STATUS.md),
[defects](REMAINING_DEFECTS.md), [backlog](RELEASE_BACKLOG.md) and the final dated
section of [validation](VALIDATION_REPORT.md) for exact observed checks.

### Acceptance status

Documentation and runnable examples are prepared. Completion follow-up on
2026-10-09: the full all-feature suite passed 374 tests, with zero failures and
five explicitly ignored opt-in tests. Debug and optimized four-executable builds
passed. Final headless suite passed 279 tests with zero failures and five
ignored; strict lint and formatting passed on their recorded slices. Earlier GUI, PDB/resource and strict-lint failures are retained in the
validation report; later lint/format/headless reruns are recorded there too.
Concurrent edits mean overlapping passes do not certify a single frozen source
snapshot. Platform/instrument and operational gates remain open. No version tag,
push or publication was performed.
