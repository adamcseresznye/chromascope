# Changelog

All notable changes to Chromascope. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versions follow [Semantic Versioning](https://semver.org/). Each release page on GitHub carries the
installers.

## [0.3.0] - unreleased

The first release since v0.2.0 (2026-03-01). Chromascope grows from an mzML viewer into a
quantification workbench with a command-line interface and two MCP servers.

### Added

- **Quantification and QC workspace.** Batch quantification with per-analyte integration settings
  (classic, valley/drop-line, Savitzky-Golay + SNR, fixed window), weighted linear and quadratic
  calibration, internal standards, qualifiers, dilution, extrapolation of end standards, rule-based
  peak confidence (0-100 with named reasons), explicit peak review, and batch QC and method
  validation with reasoned acknowledgements that can never turn a failing rule into a pass.
- **Shared analytical engine** (`engine::execute`) behind the desktop app, the JSON command-line
  interface (`chromascope-cli`) and the headless MCP server (`chromascope-engine-mcp`), so a number
  from the GUI is the number from the CLI.
- **Projects and the audit package.** Append-only project revisions with source hashes, relink and
  restore; readable HTML report, JSON, CSV, TSV and SVG exports; portable, checksummed bundles that
  can be verified and re-processed without replacing the original results.
- **Desktop MCP server** (`chromascope-mcp`): an AI client does over MCP everything a person does
  in the window, with the same result and the same audit trail. 75 tools in the default build: 60
  that drive the window and the 15 engine tools (calibration, QC, review proposals, projects). A
  calibration or QC computed by an AI appears in the Quantification tab without reopening anything.
  Launch flags `--allow-root`, `--allow-changes`, `--allow-exports` and
  `--allow-project-writes` decide what it may touch; every call is recorded under **AI activity**.
  Every tool has its own description that says what it does, when to use it and which flag it
  needs, and carries read-only/destructive annotations. `chromascope-engine-mcp` is a subset of
  this server, kept for scripts and machines without a desktop.
  New window tools: vendor files through msconvert in `open_files`, stored SRM/MRM chromatograms
  (`native_chromatogram`), `set_layout`, `retain_trace`, `remove_trace`, `set_trace_visibility`,
  `save_session` / `load_session`, `list_presets` / `get_preset` / `save_preset`, `save_method` /
  `open_method`, `save_batch` / `load_batch`, `create_project` / `open_project` / `save_project` /
  `refresh_project`, `build_bundle` / `verify_bundle` / `reprocess_result`, `get_settings` /
  `set_settings`, `export_results` / `import_results`, `show_workspace`, `remove_measurement`,
  `clear_integration`, `clear_display`, `export_measurements` and `set_view` with `reset`.
- **Stable dataset IDs.** `list_datasets` shows a `stable_id` (the SHA-256 of the source file) next
  to the number; every tool accepts either, or a project dataset's UUID, so an AI cannot point at
  the wrong file after a restart.
- **MCP prompts and resources.** The six workflow recipes (`investigate_chromatographic_anomalies`,
  `review_integration`, `construct_calibration`, `troubleshoot_qc`, `compare_processing_methods`,
  `prepare_report`) are offered as prompts by both servers. The desktop server publishes the open
  datasets, retained traces, quantification results, QC reports and project revisions as
  `chromascope://` resources.
- **No window control without an MCP tool.** `src/gui/actions.rs` maps every button, menu item and
  input to its tool, and `tests/mcp_parity.rs` fails when one is missing, when a listed tool
  is not offered, or when no test calls a tool over the protocol. One action is withheld on purpose:
  choosing the msconvert program.
- **Experimental workflows are hidden.** Identification, Untargeted analysis and Statistics are
  compiled but only exposed when built with `--features experimental`. The engine itself rejects
  their operations with `experimental_unavailable`, so project re-processing and AI review
  proposals cannot run them either, and none of their tools, prompts or workspaces is listed over
  MCP.
- **Saves are atomic.** Sessions, methods, batches, presets, settings and CSV/SVG exports are
  written to a temporary file in the same folder and renamed into place, so a crash or a full disk
  cannot destroy your only copy. Exports that create a new file never replace an existing one.
- **Numerical behaviour is written down.** Precursor filtering matches scans within 0.01 Da (a miss
  is an error that names the window, not an empty trace), trace points are summed in double
  precision and rounded once to the stored 32-bit value, and settings are read field by field so
  one unreadable value does not reset the others. See the user guide.
- **Vendor file import** through a separately installed ProteoWizard `msconvert`.
- **m/z tolerance in ppm or mDa** for XIC extraction and methods, and a user-set number of m/z
  decimals (View menu); exports keep full precision.
- **Appearance.** Light, dark or system theme, interface size, Inter and JetBrains Mono fonts;
  theme, size and m/z decimals live in one `settings.json`.
- **Installers for every platform:** Windows MSI (x64, arm64, x86) and portable zip, macOS universal
  dmg and zip, Linux deb, rpm, AppImage and tar.gz (x86_64, aarch64), with `SHA256SUMS.txt`.
  Every package ships `LICENSE` and `THIRD_PARTY_LICENSES.txt`.
- Hosted CI now also runs the tests on Windows and macOS and compiles on the minimum supported Rust
  (1.88); the release workflow runs the tests before it builds any installer.

### Changed

- The Reports tab is now **Audit package**; its workflow step, window title and documentation use
  that single name.
- Window titles use sentence case ("Plot properties", "m/z range filter").
- Release assets are named `chromascope-<version>-<os>-<arch>.<ext>`. v0.2.0 published only the
  GUI binary under target-triple names.

### Fixed

- **Data explorer integration.** "Integrate visible RT range" and a right-drag that starts outside
  the trace used to extend the edge intensity flat, so the area changed with the zoom and the
  recorded start/end fell outside the data. Windows are now clamped to the data, and a window that
  misses the trace is rejected.
- Report replay after relocating sources into a bundle; NaN scan windows; stale reference output
  in CI.

### Security and licensing

- The embedded JetBrains Mono font's SIL Open Font License text ships with the font and is shown in
  **File > About Chromascope… > Credits & licenses**, next to Inter's.
- `appimagetool` is pinned to a tagged release and verified against a SHA-256 before it runs.
- CSV exports prefix a text cell that starts with `=`, `+`, `-`, `@`, a tab or a carriage return
  with an apostrophe so a spreadsheet cannot run it as a formula; numbers are untouched.
- Test data and reference files no longer contain an account name or local folder paths. The git
  history still does.
- Every path an AI names goes through the same authorisation (`--allow-root`), exports and new
  saves never overwrite an existing file, and an AI can neither mark a peak as human-reviewed nor
  choose the msconvert program.

## [0.2.0] - 2026-03-01

Multi-file support, per-precursor MS2 scan filters, peak integration, spectrum CSV export, newer
egui/eframe, GPL-3.0 licence. Published as GUI binaries only.

## [0.1.0] - 2024-08-17

First public release: lightweight mzML viewer with TIC, BPC and XIC extraction.
