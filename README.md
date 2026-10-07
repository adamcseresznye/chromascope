<div align="center">

<img src="assets/icon.png" alt="Chromascope logo" width="120" />

# Chromascope

**A lightweight GUI for viewing mzML and importing vendor mass spectrometry data**

[![Rust](https://img.shields.io/badge/built%20with-Rust-orange?logo=rust)](https://www.rust-lang.org/)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)]()

<img src="assets/demo.gif" alt="Chromascope demo" width="860" />

</div>

---

## Why Chromascope?

mzML is an open, XML-based format widely used for storing and processing mass spectrometry data. While vendor-specific tools handle proprietary formats well, finding a truly lightweight GUI that works seamlessly with mzML files remains surprisingly difficult. 

Chromascope fills that gap by offering instant, no-setup inspection of your mass spectrometry data. The compiled binary is incredibly small (less than 5 MB on Windows), making it highly portable, fast, and perfect for smaller machines or low-resource environments.


## Features & Interactive Controls

Chromascope is designed to be highly interactive and easy to use. Here is what you can do and how to do it:

- **mzML & vendor imports**: Open `.mzML` and `.mzML.gz` directly. Other files and vendor dataset folders are converted locally through an optional ProteoWizard `msconvert` installation. Open additional files without replacing the current workspace.
- **View Mass Spectra**: **Double-click** anywhere on a chromatogram to extract and display the mass spectrum for that specific retention time.
- **Peak integration**: **Right-click and drag** across a peak on the chromatogram to perform a trapezoidal area integration.
- **Trace Extraction (TIC, BPC, XIC)**: Switch between Total Ion Chromatogram, Base Peak Chromatogram, and Extracted Ion Chromatogram modes. **Right-click the chromatogram** to open Plot Properties and select your desired trace.
- **Advanced filtering**: Filter your mass spectrometry data by MS level (e.g., MS1 vs MS2), polarity, precursor m/z, and specific m/z ranges directly from the Plot Properties menu.
- **Customizable display**: Personalize how your data is presented by adjusting line color, style, smoothing, and line width via the `Display` menu.
- **Cross-platform**: Runs as a standalone executable on Windows, macOS, and Linux without needing an installer.


## Usage

1. **Launch Chromascope**:
   - Run the application by executing the binary or running `cargo run` from the project directory.

2. **Open Data**:
   - Use `File → Open` for files, or `File → Open dataset folder…` for vendor directories. Configure ProteoWizard for vendor imports as described below.

3. **Explore Data**:
   - Switch between TIC, BPC, and XIC modes using the Plot Properties. Use the interactive controls (double-click, right-click drag) to extract a corresponding mass spectrum or integrate a peak area.

4. **Customizing Views**:
   - Adjust the display settings via the `Display` menu to customize how your data is presented.

## Vendor imports with ProteoWizard

1. Install [ProteoWizard](https://proteowizard.sourceforge.io/download.html) with the vendor readers needed for your instruments. Native proprietary readers generally require Windows and their associated runtimes. Support depends on the installed ProteoWizard build, vendor libraries, and instrument format/version; see its [format compatibility list](https://proteowizard.sourceforge.io/doc_users.html).
2. Chromascope looks for `msconvert` in this order: `CHROMASCOPE_MSCONVERT`, a saved path, `PATH`, then standard Windows ProteoWizard installation directories. Alternatively, use **File → ProteoWizard → Locate msconvert…** and select `msconvert.exe` (not `msConvertGUI.exe`). The chosen path is saved in your user configuration directory for future launches.
3. Use **File → Open** for individual vendor files, including Thermo `.raw`, SCIEX `.wiff`/`.wiff2`, or Shimadzu `.lcd`. Keep companion files beside the selected file. Use **File → Open dataset folder…** for complete Agilent/Bruker `.d` or Waters `.raw` dataset directories. Similar extensions can represent different formats; ProteoWizard identifies the dataset.
4. An import spinner appears while conversion and metadata loading run in the background. **Cancel** stops an importing dataset. Failed imports show converter diagnostics and preserve existing open files.

Conversion runs locally and never changes the source dataset. Chromascope requests mzML with 64-bit binary encoding and lossless zlib compression, without peak-picking or scan filters. SIM (selected ion monitoring) and SRM/MRM (selected reaction monitoring) acquisitions are requested as spectra so the viewer can calculate traces and display their measured ions; these are targeted measurements, not full-scan spectra. Every mzML run produced by a multi-sample dataset is added to the file list; select each run to inspect it. Converted files remain in a temporary workspace while their runs or processing workers are open, then are removed. Reopening a vendor dataset performs conversion again, so allow enough temporary disk space for all its runs. No data is uploaded.

The importer broadens file coverage; it does not add ion-mobility or imaging visualizations. Conversion does not guarantee preservation of every vendor-specific metadata field. Formats and platforms without a working vendor reader can still be opened after conversion to mzML elsewhere. Chromascope itself retains its existing spectrum intensity precision.

### Workbench interface

The file sidebar shows the active dataset, trace colors, visibility switches, and import cancellation. Drag the sidebar or inspector edge to resize it. Choose **TIC**, **BPC**, or **XIC** in the toolbar. The right inspector contains scan filters, mass range, smoothing, line appearance, selection measurements, and file information. For an XIC, enter a target m/z and tolerance in ppm, then click **Apply XIC**.

Double-click the chromatogram to inspect its mass spectrum. Right-drag across a peak to integrate; the inspector displays the result and a clear action. Middle-drag to box-zoom. Drag the horizontal divider between the plots to adjust their relative heights. The status bar reports importing and processing; the theme button switches between light and dark appearances.

### Traces, measurements, and sessions

- **Multiple XICs:** Apply an XIC, enter the next target, and apply again. Previous traces stay available with distinct colors in **Traces**. By default, visible traces appear in independent stacked panels with linked RT axes and individual intensity scales. Set **Rows** and **Columns** independently, or use **+ Row** and **+ Column**, to combine vertical and horizontal arrangements (for example 2×3 or 3×3). Additional traces are available with page controls. Narrow windows allow horizontal scrolling rather than forcing a different arrangement. Choose **Overlay** above the plots to compare all visible traces in one figure. The checkbox and delete button precede every trace name in the inspector and plot legends, including the active trace. Long names are truncated with their full text on hover. Toggle visibility, delete a trace, or select it to restore its filters and use it for integration and CSV export. TIC/BPC traces are retained too.
- **Scan inspection:** Double-click a chromatogram to select the nearest scan. Use **Previous / Next**, the left/right arrow keys, or enter a one-based scan number and click **Go**. The inspector shows the actual RT, native ID, MS level, polarity, profile/centroid representation, precursors, isolation/activation information, scan windows, injection time, and acquisition parameters when present. Adjacent navigation follows native acquisition order across all scan types.
- **Integration history:** Every completed right-drag adds a measurement. **File → Integration results** opens a table with file, trace/filter settings, bounds, and area. Remove individual rows or export CSV with source paths, baseline method, and complete extraction parameters. Clearing the selection does not remove measurements.
- **Saved sessions:** **File → Save session** (`Ctrl+S`, or `Cmd+S` on macOS) writes a `.chromascope` JSON file containing source paths, full-resolution current and retained traces, visibility, filters, measurements, selected scans, theme, chromatogram/spectrum zoom, line width, and plot split. **Open session** (`Ctrl+Shift+O`) restores the workspace and reopens source datasets; vendor data is converted again. Keep source files at their original paths. Missing runs are reported; restored measurements remain available. Session files contain trace data and can be larger than a settings-only file.
- **Figure export:** **File → Export figure (SVG)** exports visible chromatograms or the active mass spectrum as a scalable vector figure with axes, units, and acquisition/trace labels. Chromatogram figures use the full trace extent and full-resolution processed data, independently of the on-screen zoom, and follow the selected stacked/overlay layout (the current grid page).
- **Everyday controls:** `Ctrl+O` opens data, file/folder drag-and-drop imports datasets, and **File → Recent files** keeps the last 12 source paths across launches. Arrow keys navigate scans when a text field is not focused; `Esc` clears the integration selection.

Use the **Files**, **Inspector**, and **Spectrum** toggles in the top toolbar to retract or restore either sidebar or the spectrum pane. Each pane also has a **Collapse** action. The spectrum's **Close** action clears the selected spectrum and gives the chromatograms the full available height; collapsing it preserves the spectrum. Panel visibility, grid dimensions, sample-comparison mode, and the shared batch preset are saved in sessions. SVG exports follow the chosen grid dimensions.

### Trace presets and chromatogram context menu

Use **File → Preset editor…** to create a named analyte preset in a table. Add, duplicate, reorder, or delete analytes; edit acquisition, masses, ppm, smoothing, polarity, and layout. **Paste from Excel…** accepts tab-separated cells with column headers. **Save** and **Save As…** store the preset in any folder you choose; **Open preset…** and **Recent presets** reopen it for editing. **Apply to batch** applies the current table across samples, independently of saving. New/Open protects unsaved changes with Save, Discard, or Cancel. TOML remains the portable format behind the editor; existing presets are supported. See [`presets/example.toml`](presets/example.toml). A preset has `version = 1`, optional `overlay = false`, and 1–64 `[[traces]]` entries. Each entry specifies `name`, `acquisition` (`"FS"`, `"SIM"`, or `"MRM"`), optional `kind` (`"XIC"` by default, or `"TIC"`/`"BPC"`), `polarity` (`"positive"` by default, or `"negative"`), and `smoothing` (0–10). XICs need `mass` and optionally `ppm` (default 10). MRM entries require `precursor_mz`; `mass` is the product ion. Optional `ms_level` overrides the default of 1 for FS/SIM or 2 for MRM. TIC/BPC entries may specify `mz_range = [100.0, 700.0]`.

FS selects MS1 spectra excluding SIM/SRM-tagged spectra. SIM and MRM match explicit SIM/SRM spectrum metadata; an ordinary MS2 spectrum does not qualify as MRM. Targeted modes need spectra (vendor conversion requests these); a chromatogram-only source or a file without matching acquisition metadata will report that no matching scans were found. The entire preset is validated before extraction. Extraction runs in the background, and each sample’s results are committed only if all entries succeed. The preset replaces the displayed analyte set for that sample.

The loaded preset is shared across the entire batch, including files opened later. Select a sample in **Data files** to extract its analytes on the first visit; subsequent visits use cached results. The same analyte order and grid dimensions carry across samples. Only the selected sample is displayed by default. Enable **Compare samples** explicitly to display multiple samples together. If a sample cannot satisfy the preset, its failure is reported without repeatedly rerunning extraction; reload the preset to retry. Use **Clear batch preset** to return to manual extraction settings.

**Right-click** a chromatogram for spectrum inspection at the clicked RT, plot properties, presets, zoom reset, integration of the visible RT range, measurement history, CSV/figure export, and hide/delete actions for an independent panel. Right-drag still integrates a selected interval. Click a panel's legend name to make it the active trace for analysis.

### Chromatogram calculations and export

Full-resolution chromatograms are retained separately from the reduced drawing copy. Moving-average smoothing is applied at full resolution; integration and CSV export use that full-resolution processed trace, including the selected smoothing. Set smoothing to **0** for unsmoothed calculations and export. Rendering keeps at most 2,000 points and preserves bucket minima/maxima and trace endpoints. XICs retain zero-intensity scans, and TIC/BPC traces are derived from the selected spectra so their scan filters and spectrum lookup remain consistent across converted datasets.

## Installation

### Downloading pre-built binaries

You can download pre-built binaries for your operating system from the [Releases](https://github.com/adamcseresznye/chromascope/releases) page.


### Building from source

To build Chromascope from source, follow these steps:

1. **Clone the Repository**:
   ```bash
   git clone https://github.com/adamcseresznye/chromascope.git
   cd chromascope
   ```

2. **Build the Application**:
   ```bash
   cargo build --release
   ```

3. **Run the Application**:
   ```bash
   ./target/release/chromascope
   ```

## Contributing

We welcome contributions to Chromascope! If you have suggestions for new features, bug reports, or would like to contribute code, please open an issue or submit a pull request. For the contribution guidelines see [here](https://github.com/adamcseresznye/chromascope/blob/main/.github/CONTRIBUTING.md).


## License

Chromascope is licensed under the GPL-3.0 License. See the [LICENSE](https://github.com/adamcseresznye/chromascope/blob/main/LICENSE) file for more details.


## Acknowledgements

The project would not have been possible without these excellent libraries:
- [egui library](https://github.com/emilk/egui) 
- [mzdata](https://github.com/mobiusklein/mzdata)
