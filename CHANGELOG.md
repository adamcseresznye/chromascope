# Changelog

All notable changes to Chromascope. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versions follow [Semantic Versioning](https://semver.org/). Each release page on GitHub carries the
installers; [docs/RELEASE_NOTES.md](docs/RELEASE_NOTES.md) describes the current release in prose.

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
- **Desktop MCP server** (`chromascope-mcp`): an AI client drives the live window. Launch flags
  `--allow-root`, `--allow-changes` and `--allow-exports` decide what it may touch; every call is
  recorded under **AI activity**.
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

- **Experimental workflows are hidden.** Identification, Untargeted analysis and Statistics are
  compiled but only exposed when built with `--features experimental`. Hidden operations are
  rejected with `experimental_unavailable` by the engine itself, so project re-processing and AI
  review proposals cannot run them either.
- **Desktop MCP tools** have unique descriptions that say what each tool does, when to use it and
  which launch flag it needs, and carry read-only/destructive annotations. The duplicate
  `workbench` tool is no longer listed (`batch` still accepts every command): 31 tools.
- The Reports tab is now **Audit package**; its workflow step, window title and documentation use
  that single name.
- Release assets are named `chromascope-<version>-<os>-<arch>.<ext>`. v0.2.0 published only the
  GUI binary under target-triple names.
- Dependencies: `uuid` is held at 1.26.0, the newest release that builds on Rust 1.88.

### Fixed

- **Data explorer integration.** "Integrate visible RT range" and a right-drag that starts outside
  the trace used to extend the edge intensity flat, so the area changed with the zoom and the
  recorded start/end fell outside the data. Windows are now clamped to the data, and a window that
  misses the trace is rejected.
- **Saving can no longer destroy your only copy.** Sessions, methods, batches, CSV/SVG exports,
  presets and settings are written to a temporary file in the same folder and renamed into place.
- Report replay after relocating sources into a bundle; NaN scan windows; stale reference output
  in CI.

### Security and licensing

- The embedded JetBrains Mono font's SIL Open Font License text ships with the font and is shown in
  **File > About Chromascope… > Credits & licenses**, next to Inter's.
- `appimagetool` is pinned to a tagged release and verified against a SHA-256 before it runs.
- Test data and reference files no longer contain an account name or local folder paths.

## [0.2.0] - 2026-03-01

Multi-file support, per-precursor MS2 scan filters, peak integration, spectrum CSV export, newer
egui/eframe, GPL-3.0 licence. Published as GUI binaries only.

## [0.1.0] - 2024-08-17

First public release: lightweight mzML viewer with TIC, BPC and XIC extraction.
