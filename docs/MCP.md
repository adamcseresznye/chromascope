# MCP workbench

Chromascope exposes its deterministic Rust analysis and live GUI to external MCP clients. It does not contain an LLM or require any particular provider. Enable the optional `mcp` feature to build the console server. The normal GUI remains the default binary and works without MCP.

## Build and connect

Rust 1.88 or newer is required by the current egui and rmcp dependencies.

```sh
cargo build --release --features mcp --bin chromascope-mcp
# Read-only numerical access, including opening datasets into the workspace:
chromascope-mcp --allow-root /absolute/path/to/data
# Permit GUI/analytical changes and new exports:
chromascope-mcp --allow-root /absolute/path/to/data --allow-root /absolute/path/to/reports --allow-changes --allow-exports
```

Use the built executable directly in a desktop client's local stdio MCP configuration. Do not wrap stdout in another logging program. Logs go to stderr. The client launches a visible Chromascope window; closing the window ends the server, and disconnecting stdio closes the window. Each client-launched process has its own workspace.

```json
{
  "mcpServers": {
    "chromascope": {
      "command": "C:\\tools\\chromascope-mcp.exe",
      "args": ["--allow-root", "C:\\MSData", "--allow-root", "C:\\Reports", "--allow-changes", "--allow-exports"]
    }
  }
}
```

On macOS/Linux, use an absolute executable path and native absolute directory paths. Existing directories are required. MCP clients with a different configuration layout should use the same executable and arguments. A graphical desktop is required for this runner; protocol and engine tests run without one.

## Architecture assessment and decision

The existing library already contains:

- `parser.rs`: mzdata format-aware readers, bounds and scan filter discovery, TIC/BPC/XIC, spectra by scan index, acquisition metadata. Readers are thread-owned and must not be moved between threads.
- `processing.rs`: extraction orchestration, duplicate RT aggregation, moving-average smoothing, display decimation, nearest scan lookup, interpolated-boundary trapezoidal integration with a straight chord baseline. Full-resolution processed traces drive viewer integration; display decimation never drives quantitation.
- `export.rs`: numerical chromatogram and spectrum CSV exporters.
- `gui/state.rs`: stable file IDs, GUI-owned readers, cached raw chromatograms, full-resolution processed data, display data, spectrum caches, user input, background channels and integration overlays.
- `gui/workspace.rs`: retained traces, comparison and overlay views, measurements with extraction parameters, sessions, plot ranges and SVG exporters.
- `gui/presets.rs`: validated multi-trace TOML presets with background extraction.
- `gui/quant.rs`: existing XIC method validation, detection smoothing, local peak selection, missing/ambiguous classifications, unsmoothed integration, original automatic results, manual correction, review, cancellation and reproducible CSV export.
- `import.rs`: optional ProteoWizard conversion, import lifetimes and cancellation. MCP deliberately accepts mzML/mzML.gz rather than invoking vendor converters.

Parsing, processing and numerical CSV export need no algorithm changes. Plot rendering, retained trace selection, presets and batch quantification need owner-thread adapters because their current orchestration is GUI-owned. Existing tests cover parsing, filters, integration, GUI interaction, sessions, exports, quantification and a fake msconvert integration.

The chosen server runs **inside a dedicated GUI-hosting executable**. `src/mcp.rs` implements rmcp tool routing, generated JSON schemas, filesystem policy and a bounded request queue. `src/gui/remote.rs` is the application adapter. The Tokio server thread only sends typed commands and receives serializable responses; egui's owner thread applies all application changes. File loading and chromatogram extraction create their own readers on workers, then return owned numerical/metadata results. The GUI opens its own reader after metadata loading. A request cancelled before application does not commit its results. Display extraction checks whether local selection/parameters changed while the worker ran; it returns `display_conflict: true` instead of replacing a newer view.

This avoids an additional daemon, IPC discovery, authentication channel and duplicated analytical engine. Its tradeoff is that clients must start the MCP-enabled GUI; it does not attach to an already running ordinary Chromascope process. A later stdio-to-local-IPC adapter can reuse these commands. Network/HTTP transports and an internal agent framework are intentionally absent.

## Tools

`tools/list` returns generated object schemas. Unknown fields are rejected. The general `workbench` tool takes `{"command":{"operation":"...",...}}`; the named tools below take their fields directly. Use named tools for normal orchestration. Empty tools take `{}`.

| Tool | Inputs and behavior |
|---|---|
| `list_datasets` | Stable IDs, path, loading/visibility/processing state and active ID |
| `list_mzml_files` | `directory`: list immediate mzML/mzML.gz files under an authorized root, sorted by path; no recursive traversal |
| `open_files` | `paths`: 1–64 explicitly supplied mzML/mzML.gz paths. Worker loading; per-file success/error. Adds datasets without selecting or extracting them |
| `dataset_metadata` | `dataset_id`: RT and m/z bounds, scan count, available MS levels, polarities and precursor filters |
| `list_scans` | `dataset_id`, `offset`, `limit` (1–1000): structured native IDs, RT, polarity, MS level, precursor/isolation/activation and scan windows |
| `scan_metadata` | `dataset_id`, `index`: existing detailed acquisition metadata as a text field in a structured result |
| `select_dataset` | `dataset_id`: restore selected dataset's cached GUI trace and spectrum state |
| `extract_chromatogram` | `dataset_id`, `kind` (`tic`, `bpc`, `xic`), `polarity` (`positive`, `negative`, `unknown`), `ms_level` (>0), `smoothing` (0–10), optional `mass`, `tolerance_ppm`, `mz_range`, `precursor_mz`, `acquisition` (`FS`, `SIM`, `MRM`), `display` (default false). XIC requires mass and ppm; m/z ranges apply to TIC/BPC |
| `chromatogram_data` | `dataset_id`, `offset`, `limit` (1–10000): full-resolution processed `[RT,intensity]` points from the latest MCP extraction for this dataset, with exact parameters |
| `spectrum` | `dataset_id`, exactly one of `index` or `retention_time_minutes`, `display` (default false), `offset`, `limit` (1–10000). RT lookup uses the nearest scan from the latest filtered extraction (or GUI chromatogram), not an averaged spectrum |
| `gui_state` | Current input, dataset IDs, bounds, overlay settings, retained trace summaries, measurements and operation history; raw arrays are omitted |
| `set_view` | Optional `retention_time_range`, `mz_range`, `overlay`. Increasing finite nonnegative ranges; ranges affect display, not analytical extraction |
| `set_display` | Optional `compare_samples`, `intensity_scale` (`individual`, `shared_highest`, `shared_custom`), positive `intensity_maximum`, GUI palette `line_color` (e.g. `Blue`), `line_width` (0.5–10) |
| `set_visibility` | `dataset_id`, `visible` |
| `select_trace` | `dataset_id`, `trace_index` from the current retained trace list. Restores original parameters and full-resolution points; indices can change after selection |
| `integrate` | `dataset_id`, `start_minutes`, `end_minutes`, `apply` (default false). Calculate with the existing interpolated-boundary chord-baseline algorithm. Applied changes require the same extraction to be displayed and add a reproducible measurement/overlay |
| `undo_integration` | Restore the previous MCP integration and remove its measurement, provided no intervening workspace edit makes rollback unsafe |
| `measurements` | Viewer integration measurements including extraction parameters and boundaries |
| `chromatogram_image`, `spectrum_image` | Current displayed plot as MCP `image/png` content plus structured state |
| `export_csv` | `dataset_id`, `spectrum`, `path`: full-resolution numerical data via existing exporters. Spectrum export uses the displayed cached spectrum |
| `export_figure` | `spectrum`, `path`: existing current-view SVG exporter |
| `apply_preset` | `toml`: existing multi-trace viewer preset; starts the application's background extraction |
| `start_quantification` | `method_toml`, `dataset_ids`: existing batch XIC detection/quantification, one running batch per process |
| `quantification_status` | Running/completed/total/message and result indices, original automatic peaks/status, current peaks/status, diagnostics and exact method/parameter snapshots. Does not transfer raw traces |
| `quantification_trace` | `result_index`, `offset`, `limit`: full-resolution unsmoothed quantitative XIC |
| `cancel_quantification` | Request cancellation; completed results survive |
| `review_quantification` | `result_index`, optional paired `start_minutes`/`end_minutes`, `reviewed` (default false): recalculate a manual boundary correction or select a result. `reviewed: true` is rejected: human acceptance is recorded through the GUI |
| `reset_quantification` | `result_index`: restore original automatic peak and status |
| `export_quantification` | `path`: existing CSV with method TOML, current quantitative results, diagnostic/review status and stale-method indication |
| `batch` | `commands`: 1–64 tagged operations, executed sequentially with per-operation results. No nesting. Partial successes remain; not an atomic transaction |
| `workbench` | Object-wrapped tagged operation interface to the same command model |

Optional fields can be omitted or set to null. Units are retention time **minutes**, target tolerance **ppm**, m/z **m/z**, intensity **instrument arbitrary units**, and area **intensity × minutes**. For precursor filtering the existing engine uses a 0.01 m/z tolerance. Unknown polarity follows the existing engine's semantics; consult available scan filters rather than assuming it means a particular sign. Read numerical pages for quantitative reasoning. Display images use decimated display traces, consistent with the GUI.

## Visual reasoning and end-to-end example

PNG generation reuses `plot_chromatogram`, `plot_mass_spectrum`, the GUI line styling, plot ranges and integration overlay. A detached egui workspace is rendered and tessellated, then the existing CPU test renderer encodes a PNG. This is a 1000×600 plot rendering, not a desktop screenshot or an exact pixel capture of the user's window. It does not alter live zoom or spectrum selection. SVG export also uses the existing figure code. The PNG renderer uses CPU triangle/texture rasterization and can differ from GPU antialiasing.

For a client with vision capability:

1. `list_mzml_files` in an authorized directory, then `open_files` with the returned paths; read IDs and `dataset_metadata`.
2. `extract_chromatogram` with `kind: "xic"`, target mass, ppm, filters and `display: true`.
3. Read `chromatogram_data` and request `chromatogram_image`. Interpret shoulders, splitting and interferences externally; keep interpretations separate from measured results.
4. Use `spectrum` at an ambiguous RT with `display: true`, then `spectrum_image` and `list_scans` to inspect product/precursor evidence.
5. Call `set_view` to zoom. Propose boundaries and first call `integrate` with `apply: false`.
6. After deciding to commit a correction, call `integrate` with the same boundaries and `apply: true`. Request an updated PNG and `measurements`. Record unresolved ambiguity for human review; use `undo_integration` when appropriate.
7. Export numerical data and figures to new files, or run a validated existing batch method and inspect its ambiguity diagnostics. Human acceptance remains a GUI action.

A runnable, provider-independent stdio example is included:

```sh
python examples/mcp_visual_workflow.py --server /absolute/path/chromascope-mcp --file /absolute/path/sample.mzML --mass 524.3 --ppm 10 --output /absolute/path/new-report-directory
```

The script exercises XIC → PNG → numeric integration → changed boundaries → updated PNG/results through the real protocol. It does not pretend to perform AI visual interpretation: its two demonstration intervals are chosen from measured RT points. A vision client supplies scientifically justified intervals instead.

Example natural-language request to your MCP client: “Open these three mzML files, extract positive MS1 XICs for the targets in this method, compare samples with shared intensity scaling, inspect ambiguous peaks and corresponding spectra, propose boundary corrections, and export a table and plots that preserve unresolved cases.” The client supplies paths and method TOML and orchestrates the tools; Chromascope performs the numerical operations.

## Reliability, security and limitations

- No original experimental files are modified. File paths are canonicalized against explicitly authorized roots. MCP opening supports only regular mzML/mzML.gz files. Vendor conversions remain available in the standalone GUI.
- Launch without `--allow-changes` to prohibit MCP selection, zoom, style, presets, applied integration and batch mutations. Numerical extraction/spectra and opening datasets remain available. `display` and `apply` default to false. Opening adds workspace entries but does not select a dataset.
- Exports require `--allow-exports`, an existing authorized parent directory and a new filename. Atomic `create_new` refuses overwrite. MCP does not execute shells, invoke converters, browse unauthorized directories or send network requests.
- Treat authorized directories as trusted local storage. This is not a sandbox against another process replacing authorized files or directory links during an operation. GUI-selected files outside the roots are denied for MCP analytical operations.
- View limits and processed extraction parameters are separate. Viewer integration uses the requested full-resolution processed trace, including requested smoothing; batch integration uses unsmoothed XIC data and only smooths detection. Baseline subtraction may yield negative areas; these are retained rather than silently clipped.
- MCP operations and failures are recorded in memory with their exact requests. Viewer measurements preserve parameters and boundaries; batch results preserve method snapshots, original automatic results and current manual results. Save/export results for persistent provenance. There is no persistent audit database or content hash of the input files.
- Integration rollback refuses to discard intervening measurements. Batch rollback returns to the original automatic result. Agent corrections remain distinct from human review. AI text/interpretations are not stored as measured data.
- Loading/extraction are worker operations with numerical results retained until replaced by a later extraction for the same dataset. Bounded pages avoid transferring complete raw datasets. GUI images and spectrum/metadata lookups are owner-thread operations; large images or scan metadata pages can briefly occupy the GUI thread.
- The queue holds at most 16 requests, one expensive MCP worker operation at a time. Batch quantification has progress and cancellation through status tools. Individual extraction/loading do not yet expose progress percentages or cooperative cancellation; a disconnected caller's result is not committed. There is no support for MCP resumable task resources.
- The server is local stdio only, requires a desktop, and hosts its own GUI. It cannot attach to an independently launched GUI. HTTP/IPC, a headless runner, persistent jobs, recursive directory discovery, session file APIs, spectral similarity/library matching and new scientific algorithms are future extensions.
- Multiple targets/samples use `batch`, presets and existing quantification methods. There is no fabricated spectral comparison score or new peak-quality model. Agent visual assessments remain hypotheses requiring review.

## Developer extension guide

Add a typed `Command` variant and object-shaped argument struct with serde validation and `schemars::JsonSchema` in `src/mcp.rs`. Add a named rmcp tool that forwards to the shared dispatcher. Implement its adapter in `src/gui/remote.rs`, or delegate to an existing GUI-owned service such as `quant.rs`. Apply filesystem policy and mutation authorization before execution. Expensive operations should open a worker-owned reader and return owned data through `BackgroundResult`; never send `MzData` or borrow the GUI across threads. Apply display results only on the GUI thread, accounting for intervening local edits. Reuse existing numerical/plot/export routines rather than copying algorithms. Return structured content and structured tool errors; PNGs use MCP image blocks.

Add tests for invalid arguments, authorization, numerical equivalence, owner-thread state transitions and the protocol schema. The included mzML fixture is approximately 0.5 MB and covers MS1/MS2 data. Test commands:

```sh
cargo fmt --all -- --check
cargo check --all-targets
cargo check --features mcp --all-targets
cargo clippy --features mcp --all-targets -- -D warnings
cargo test --all-targets
cargo test --features mcp --all-targets
cargo test --features mcp --doc
```


## Validation recorded for this implementation

On Windows with Rust 1.93.1: default-feature tests passed (197 unit tests and 2 import integration tests), MCP-feature tests passed (209 unit tests and 2 import integration tests), and all 6 doctests passed. Formatting, all-target compilation, and Clippy with `-D warnings` passed. Two real ProteoWizard integration tests remain ignored because they require installed converter/vendor data prerequisites.

The actual `chromascope-mcp.exe` was also launched through `examples/mcp_visual_workflow.py`: it advertised 32 tools, discovered and loaded the repository fixture, extracted an XIC, returned four PNG images, retrieved a spectrum, exported CSV/SVG, and recalculated the deliberately narrowed demonstration interval from 54.3503999710083 to 13.587599992752075 intensity×minutes. The PNG was visually inspected. This verifies the transport/GUI/engine loop on this platform; it does not certify scientific interpretation or cross-platform native window behavior.
