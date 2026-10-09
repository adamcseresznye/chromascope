# Operational examples

Build `cargo build --locked --all-features --bins` first. Python helpers use the
real executable and do not implement alternative numerical algorithms. Always
supply a NEW output directory; inspect retained JSON and stderr on failure.
Commands below use Windows debug executable paths; adjust for other platforms.

## Targeted extraction, QC review and local compound-search demonstration

```powershell
python examples/release_workflows.py --cli target/debug/chromascope-cli.exe --output NEW_RELEASE_EXAMPLE
```

This runnable example uses the included mzML for targeted extraction with the
editable targeted request. It has no standards: missing calibration/concentration
is intentional. Supply actual standards, known levels, sample roles and justified
method settings before interpreting concentrations. The full calibrated raw
workflow is exercised with authored triangular mzML in `tests/targeted.rs` and
`tests/engine_mcp.rs`; those are software references, not instrument studies.

The helper evaluates the synthetic `qc-request.json` study with a deliberately
strict 0% precision limit (demonstration only, not a recommended threshold), acknowledges one
computed failure with an explicit demonstration reason, and retains original and
reviewed reports. Failure remains failure. Export operations retain CSV content
inside engine responses and save corresponding CSV files; complete JSON preserves richer evidence and history.

Identification imports the existing CC BY MassBank record, processes that same
record and performs an explicitly labelled self-search. This demonstrates the
operational matching interface, not blinded unknown identification. It retains
library metadata, query, scored candidates and export response without assigning
confirmed identity. Attribution/hashes: `tests/reference/spectral_sources.json`.

## Search an actual unknown spectrum against a local library

Prepare an `inspect_spectra` request with the actual scan indices/processing
settings through the GUI spectral workspace or headless MCP schema; retain its
CLI response as `YOUR_QUERY.json`. Then:

```powershell
python examples/spectral_workflow.py --cli target/debug/chromascope-cli.exe --library YOUR_LIBRARY.msp --format msp --source YOUR_SOURCE.json --query YOUR_QUERY.json --config examples/spectral-search.json --output NEW_SEARCH
```

Source metadata must describe YOUR library's actual version, URL and license.
The bundled spectral request is a synthetic unknown, not a measured compound.
Search filters polarity/precursor/adduct/instrument/energy metadata; empty results
are valid and do not invent candidates. Formula/isotope hypotheses remain tentative.

## AI-assisted targeted analysis through real stdio MCP

For a fully runnable **synthetic** calibrated example, generate authored raw
triangles using the same independently calculable fixture as the raw-targeted
integration tests. The generator supplies inputs only, never numerical results:

```powershell
cargo run --locked --no-default-features --example targeted_reference -- NEW_SYNTHETIC_INPUTS
Get-Content NEW_SYNTHETIC_INPUTS/request.json -Raw | target/debug/chromascope-cli.exe run -
python examples/mcp_targeted_workflow.py NEW_SYNTHETIC_INPUTS/request.json NEW_SYNTHETIC_AI_DRAFT --server target/debug/chromascope-engine-mcp.exe
```

This supplies four standards, two QC injections, an unknown and blank, plus
quantifier/qualifier/internal-standard/dilution settings. It is an explicit
software reference, not a measured method or recommended acceptance policy.

```powershell
python examples/mcp_targeted_workflow.py YOUR_REQUEST.json NEW_AI_DRAFT --server target/debug/chromascope-engine-mcp.exe --qc-rules YOUR_RULES.json
```

Supply a `targeted_batch` request with existing raw paths, standards/calibration,
internal standards/qualifiers as appropriate, dilution and units. Optional QC rules
are a JSON array using the `qc::Rule` schema. The helper discovers schemas, runs
real asynchronous jobs, retrieves evidence, optionally evaluates QC and retains
CSV, audit, transcript and draft report. It demonstrates agent-facing orchestration;
no language model or automatic scientific approval is embedded. See
[headless MCP](../docs/MCP_ANALYSIS.md). For visual agent interaction use
`mcp_visual_workflow.py` and the [desktop guide](../docs/MCP.md).

## Other operational slices

- `statistics_workflow.py` consumes sample-major CSV, metadata and explicit settings;
  bundled CSVs are synthetic. Requires the documented NumPy/SciPy runtime.
- `untargeted_workflow.py` uses explicitly supplied sample paths and local pyOpenMS
  3.5.0. Review raw/aligned RT and detected/filled/missing values.
- `annotation_workflow.py` uses a retained matrix and explicitly supplied adduct/
  formula bounds, optionally a retained local library. It does not confirm identity.
- `chromatography-request.json`, `qc-request.json`, `spectral-request.json` are
  inspectable synthetic engine requests. `targeted-request.json` is an editable
  unknown-only raw extraction example. Do not interpret examples as validated methods.

Each helper's `--help` or module docstring documents its actual arguments. Preserve
full request/response/runtime/source evidence and review original versus revised
values. CLI errors return nonzero status and structured codes; incomplete output
folders remain for diagnosis. Nothing publishes or submits data remotely.
