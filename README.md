<div align="center">

<img src="assets/icon.png" alt="Chromascope logo" width="120" />

# Chromascope

**An open-source, AI-ready LC–MS analysis workbench built in Rust.**

Explore, visualize, process, and quantify mass spectrometry data through an interactive desktop interface—or automate analytical workflows with external AI agents through the Model Context Protocol (MCP).

[![Rust](https://img.shields.io/badge/built%20with-Rust-orange?logo=rust)](https://www.rust-lang.org/)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)](#installation)
[![MCP](https://img.shields.io/badge/MCP-32%20tools-blue)](docs/MCP.md)

[Quick start](#quick-start) · [User guide](docs/USER_GUIDE.md) · [MCP guide](docs/MCP.md) · [Releases](https://github.com/adamcseresznye/chromascope/releases)

</div>

## AI-powered LC–MS workflows with MCP

Chromascope exposes its analytical capabilities through the **Model Context Protocol (MCP)**, allowing compatible AI agents to interact directly with the live application.

In this 30-second demonstration, an AI agent using OpenCode autonomously:

- Opens an mzML dataset.
- Extracts an ion chromatogram at m/z 258.1109.
- Applies smoothing and adjusts the chromatogram view.
- Inspects the peak and proposes integration boundaries.
- Calculates the integrated peak area without modifying the original results.

**Watch the demonstration:**

<!-- For inline playback on GitHub, upload assets/demo.mp4 as an issue or discussion attachment and replace the link below with the hosted video URL. -->
[Watch the demonstration](assets/demo.mp4)

Chromascope's optional MCP server provides **32 tools** for data exploration, chromatogram extraction, mass spectral inspection, visualization, integration, and batch quantification.

For installation, configuration, and the complete tool reference, see the [MCP documentation](docs/MCP.md).

## Key features

- **Chromatograms and spectra:** Extract TICs, BPCs, and XICs; inspect spectra and acquisition metadata; filter by MS level, polarity, precursor, acquisition mode, and m/z range.
- **Sample comparison:** Retain multiple traces, switch between stacked, grid, and overlay views, and compare aligned analytes across samples with individual or shared intensity scales.
- **Peak integration:** Calculate full-resolution trapezoidal areas with interpolated boundaries and a straight chord baseline. Preserve extraction parameters and measurement history.
- **Batch quantification:** Apply reusable XIC methods, review automatic peak selections and ambiguity diagnostics, adjust boundaries, and export reproducible results.
- **Portable workflows:** Save TOML presets and methods, viewer and batch sessions, numerical CSV data, and SVG figures.
- **Instrument access:** Open mzML/mzML.gz directly and import supported vendor files or dataset folders locally through optional ProteoWizard conversion.

The desktop GUI works independently of MCP. The current application version is **0.3.0**; features in this repository may be newer than published release binaries.

## AI-agent integration through MCP

The optional MCP server exposes **32 tools** for numerical analysis, visual inspection, batch operations, and structured GUI control. Compatible clients can load datasets, extract chromatograms, inspect spectra, retrieve PNG plot images, zoom and compare views, propose integration boundaries, and run the existing quantification workflow.

External clients supply AI reasoning and workflow orchestration. Chromascope performs the numerical calculations in Rust and returns measured data, exact processing parameters, and visual feedback. Scientific interpretation and uncertain peak selections require appropriate review.

```sh
cargo build --release --locked --features mcp --bin chromascope-mcp
```

Configure your client to launch `chromascope-mcp` with explicitly authorized data directories. Analytical and GUI changes require `--allow-changes`; exports require `--allow-exports`. The server hosts a live Chromascope window and communicates over local stdio.

Example request to an MCP-compatible client:

> Open my LC–MS samples, extract XICs for m/z 258.1109, compare chromatographic peak shapes across runs, investigate suspicious features, and propose integration boundaries for review.

See the [MCP guide](docs/MCP.md) for architecture, supported tools, image responses, permissions, and limitations, or start with the [client configuration example](examples/mcp-client.json).

### Try the visual workflow

The [runnable stdio example](examples/mcp_visual_workflow.py) demonstrates file discovery → XIC extraction → PNG inspection → changed integration boundaries → numerical recalculation → spectrum inspection → CSV/SVG export. It has been exercised against the real GUI and the included mzML fixture. Its demonstration boundaries test the API; a scientific workflow supplies justified boundaries through an external client or human review.

```sh
python examples/mcp_visual_workflow.py --server /absolute/path/chromascope-mcp --file /absolute/path/sample.mzML --mass 524.3 --ppm 10 --output /absolute/path/new-report-directory
```

Use your built executable and sample paths; on Windows the server is `target\release\chromascope-mcp.exe`. The example saves plot images, numerical exports, and a JSON record of its requests and results. See the [end-to-end walkthrough](docs/MCP.md#visual-reasoning-and-end-to-end-example).

## Example workflows

- **Inspect an LC–MS run:** Open a dataset, select an acquisition filter, extract a target XIC, and inspect the spectrum at a chromatographic feature.
- **Compare samples:** Apply a shared analyte preset, display samples as columns and analytes as rows, and use shared intensity scaling to inspect differences.
- **Quantify and review a batch:** Run a saved method, visit unresolved results, correct boundaries where justified, and export areas with method snapshots and diagnostics.
- **Automate iterative analysis:** Let an external MCP client combine numerical retrieval and plot images, request additional views, propose changes, and preserve unresolved cases for review.

## Installation

Download desktop binaries from [GitHub Releases](https://github.com/adamcseresznye/chromascope/releases), or build the current source with **Rust 1.88 or newer**:

```sh
git clone https://github.com/adamcseresznye/chromascope.git
cd chromascope
cargo build --release --locked --bin chromascope
```

Run `target\release\chromascope.exe` on Windows, or `./target/release/chromascope` on macOS/Linux. Build the MCP-enabled executable separately using the command above. Platform dependencies and vendor import setup are covered in the [user guide](docs/USER_GUIDE.md#installation).

## Quick start

1. Launch Chromascope and use **File > Open data…** to open mzML data. Vendor datasets require [ProteoWizard setup](docs/USER_GUIDE.md#vendor-imports-with-proteowizard).
2. Choose **TIC**, **BPC**, or **XIC**. For an XIC, enter the target m/z and ppm tolerance, then choose **Apply XIC**.
3. Double-click a chromatogram to inspect its spectrum. Right-drag across a peak to integrate; middle-drag to box-zoom.
4. Retain additional traces for comparison, or switch to **Batch Quantification** to apply a reusable method across samples.
5. Review the results and export numerical data or figures. Save a session to continue later.

## Documentation

| Guide | What it covers |
|---|---|
| [User guide](docs/USER_GUIDE.md) | Interface controls, imports, trace comparison, presets, sessions, integration, batch quantification, and exports |
| [MCP guide](docs/MCP.md) | Server architecture, client setup, tool reference, visual workflows, scientific reliability, security, and developer extensions |
| [Viewer preset](presets/example.toml) | Portable multi-analyte extraction settings |
| [Quantification method](presets/quantification-example.toml) | Reusable XIC method with RT windows and detection settings |

## Contributing and license

Bug reports, ideas, and pull requests are welcome. See the [contribution guide](.github/CONTRIBUTING.md) for development and reporting instructions.

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
```

Chromascope is licensed under [GPL-3.0-only](LICENSE).

Built with [egui/eframe](https://github.com/emilk/egui), [mzdata](https://github.com/mobiusklein/mzdata), and the [Rust MCP SDK](https://github.com/modelcontextprotocol/rust-sdk). The embedded [Inter](https://rsms.me/inter/) font by Rasmus Andersson uses the [SIL Open Font License](assets/fonts/inter/LICENSE.txt).
