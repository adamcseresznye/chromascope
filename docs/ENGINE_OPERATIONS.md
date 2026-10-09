# Shared engine operation inventory

Audited against `src/domain.rs` and `src/engine.rs`, 2026-10-08.
All rows use CLI `run SOURCE|-` and headless MCP `start_analysis` with the
same typed request; `analytical_operation` remains a legacy JSON route.
This is a static dispatch inventory, not a claim of native GUI or scientific
certification. Workspaces and test evidence are mapped in [FEATURE_STATUS.md](FEATURE_STATUS.md).

| Operation tag | Engine dispatch exists |
|---|---|
| `analyze_statistics` | Yes |
| `export_statistics` | Yes |
| `propose_feature_annotations` | Yes |
| `add_feature_hypothesis` | Yes |
| `review_feature_annotation` | Yes |
| `export_feature_annotations` | Yes |
| `untargeted_batch` | Yes |
| `export_feature_matrix` | Yes |
| `linked_spectral_chromatogram` | Yes |
| `compare_spectra` | Yes |
| `export_spectral_candidates` | Yes |
| `inspect_spectra` | Yes |
| `process_spectra` | Yes |
| `import_spectral_library` | Yes |
| `search_spectral_library` | Yes |
| `annotate_spectrum` | Yes |
| `formula_candidates` | Yes |
| `analyze_isotopes` | Yes |
| `validate_method` | Yes |
| `review_qc` | Yes |
| `evaluate_qc` | Yes |
| `evaluate_targeted_qc` | Yes |
| `export_qc` | Yes |
| `export_chromatographic_peaks` | Yes |
| `process_chromatograms` | Yes |
| `revise_chromatogram` | Yes |
| `targeted_batch` | Yes |
| `review_targeted` | Yes |
| `export_targeted` | Yes |
| `metadata` | Yes |
| `spectrum` | Yes |
| `extract` | Yes |
| `integrate` | Yes |
| `quantify` | Yes |

The request schema is generated from actual DTOs through MCP
`analysis_capabilities`. CLI errors and project lifecycle are separate contracts.
No view-only GUI command is invented as an analytical operation.
