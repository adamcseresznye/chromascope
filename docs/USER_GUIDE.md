# Chromascope user guide

Detailed instructions for the interactive desktop workbench. For external AI clients, tool schemas, permissions, and visual automation, see the [MCP guide](MCP.md).

- [Quick start](#quick-start)
- [Vendor imports](#vendor-imports-with-proteowizard)
- [Workbench interface](#workbench-interface)
- [Traces, measurements, and sessions](#traces-measurements-and-sessions)
- [Presets and sample comparison](#trace-presets-and-chromatogram-context-menu)
- [Intensity scaling](#intensity-scaling)
- [Chromatogram calculations](#chromatogram-calculations-and-export)
- [Batch quantification](#batch-quantification)
- [Installation and development checks](#installation)

## Features & Interactive Controls

Chromascope is designed to be highly interactive and easy to use. Here is what you can do and how to do it:

- **Across instrument formats**: Open `.mzML` and `.mzML.gz` directly, and import supported vendor files and dataset folders through an optional ProteoWizard `msconvert` installation. Open additional files without replacing the current workspace.
- **View Mass Spectra**: **Double-click** anywhere on a chromatogram to extract and display the mass spectrum for that specific retention time.
- **Peak integration**: **Right-click and drag** across a peak on the chromatogram to perform a trapezoidal area integration.
- **Batch Quantification**: Apply a saved TOML method across samples, review automatically selected peaks, adjust boundaries manually, and export results to CSV.
- **Reusable presets and sessions**: Save viewer extraction presets, quantification methods, and separate viewer/batch sessions. Download starter TOML files directly from either editor.
- **Trace Extraction (TIC, BPC, XIC)**: Switch between Total Ion Chromatogram, Base Peak Chromatogram, and Extracted Ion Chromatogram modes. **Right-click the chromatogram** to open Plot Properties and select your desired trace.
- **Advanced filtering**: Filter your mass spectrometry data by MS level (e.g., MS1 vs MS2), polarity, precursor m/z, and specific m/z ranges directly from the Plot Properties menu.
- **Customizable display**: Personalize how your data is presented by adjusting line color, style, smoothing, and line width via the `Display` menu.
- **Cross-platform**: Runs as a standalone executable on Windows, macOS, and Linux without needing an installer.


## Quick start


1. **Launch Chromascope**:
   - Run the application by executing the binary or running `cargo run` from the project directory.

2. **Open Data**:
   - Use `File > Open data…` for files, or `File > Open dataset folder…` for vendor directories. Configure ProteoWizard for vendor imports as described below.

3. **Explore Data**:
   - Switch between TIC, BPC, and XIC modes using the Plot Properties. Use the interactive controls (double-click, right-click drag) to extract a corresponding mass spectrum or integrate a peak area.

4. **Customizing Views**:
   - Adjust the display settings via the `Display` menu to customize how your data is presented.

The toolbar starts with **Viewer** and **Batch Quantification**. **File > About Chromascope…** shows the logo, application version, author, project links, and software/font licenses.

## Vendor imports with ProteoWizard

1. Install [ProteoWizard](https://proteowizard.sourceforge.io/download.html) with the vendor readers needed for your instruments. Native proprietary readers generally require Windows and their associated runtimes. Support depends on the installed ProteoWizard build, vendor libraries, and instrument format/version; see its [format compatibility list](https://proteowizard.sourceforge.io/doc_users.html).
2. Chromascope looks for `msconvert` in this order: `CHROMASCOPE_MSCONVERT`, a saved path, `PATH`, then standard Windows ProteoWizard installation directories. Alternatively, use **File > Vendor import settings > Locate msconvert…** and select `msconvert.exe` (not `msConvertGUI.exe`). The chosen path is saved in your user configuration directory for future launches.
3. Use **File > Open data…** for individual vendor files, including Thermo `.raw`, SCIEX `.wiff`/`.wiff2`, or Shimadzu `.lcd`. Keep companion files beside the selected file. Use **File > Open dataset folder…** for complete Agilent/Bruker `.d` or Waters `.raw` dataset directories. Similar extensions can represent different formats; ProteoWizard identifies the dataset.
4. An import spinner appears while conversion and metadata loading run in the background. **Cancel** stops an importing dataset. Failed imports show converter diagnostics and preserve existing open files.

Conversion runs locally and never changes the source dataset. Chromascope requests mzML with 64-bit binary encoding and lossless zlib compression, without peak-picking or scan filters. SIM (selected ion monitoring) and SRM/MRM (selected reaction monitoring) acquisitions are requested as spectra so the viewer can calculate traces and display their measured ions; these are targeted measurements, not full-scan spectra. Every mzML run produced by a multi-sample dataset is added to the file list; select each run to inspect it. Converted files remain in a temporary workspace while their runs or processing workers are open, then are removed. Reopening a vendor dataset performs conversion again, so allow enough temporary disk space for all its runs. No data is uploaded.

The importer broadens file coverage; it does not add ion-mobility or imaging visualizations. Conversion does not guarantee preservation of every vendor-specific metadata field. Formats and platforms without a working vendor reader can still be opened after conversion to mzML elsewhere. Chromascope itself retains its existing spectrum intensity precision.

### Workbench interface

The file sidebar shows the active dataset, trace colors, visibility switches, and import cancellation. Drag the sidebar or inspector edge to resize it. Choose **TIC**, **BPC**, or **XIC** in the toolbar. The right inspector contains extraction settings, scan filters, mass range, smoothing, line appearance, retained traces, and measurements. For an XIC, enter a target m/z and tolerance in ppm, then click **Apply XIC**. Scan navigation and acquisition details sit above the spectrum; dataset information is accessed by right-clicking its chromatogram and choosing **File information…**. The interface uses embedded Inter fonts and offers light and dark themes.

Double-click the chromatogram to inspect its mass spectrum. Right-drag across a peak to integrate; the inspector displays the result and a clear action. Middle-drag to box-zoom. Drag the horizontal divider between the plots to adjust their relative heights. The status bar reports importing and processing; the theme button switches between light and dark appearances.

### Traces, measurements, and sessions

- **Multiple XICs:** Apply an XIC, enter the next target, and apply again. Previous traces stay available with distinct colors in **Traces**. By default, visible traces appear in independent stacked panels with linked RT axes and individual intensity scales. Open **Grid layout…** and set **Rows** and **Columns** independently, or use **+ Row** and **+ Column**, to combine vertical and horizontal arrangements (for example 2×3 or 3×3). Additional traces are available with page controls. Narrow windows allow horizontal scrolling rather than forcing a different arrangement. Choose **Overlay** above the plots to compare all visible traces in one figure. The checkbox and delete button precede every trace name in the inspector and plot legends, including the active trace. Long names are truncated with their full text on hover. Toggle visibility, delete a trace, or select it to restore its filters and use it for integration and CSV export. TIC/BPC traces are retained too.
- **Scan inspection:** Double-click a chromatogram to select the nearest scan. Use **Previous / Next** above the spectrum, the left/right arrow keys, or enter a one-based scan number and click **Go**. The spectrum header shows the actual RT. **Acquisition details…** beside the scan controls shows the native ID, MS level, polarity, profile/centroid representation, precursors, isolation/activation information, scan windows, injection time, and acquisition parameters when present. Adjacent navigation follows native acquisition order across all scan types. Right-click a chromatogram and choose **File information…** to see its dataset path, scan count, and RT/mass ranges.
- **Integration history:** Every completed right-drag adds a measurement. **File > Integration results** opens a table with file, trace/filter settings, bounds, and area. Remove individual rows or export CSV with source paths, baseline method, and complete extraction parameters. Clearing the selection does not remove measurements.
- **Saved sessions:** **File > Save viewer session** (`Ctrl+S`, or `Cmd+S` on macOS) writes a `.chromascope` JSON file containing source paths, full-resolution current and retained traces, visibility, filters, measurements, selected scans, theme, chromatogram/spectrum zoom, line width, and plot split. **Open viewer session** (`Ctrl+Shift+O`) restores the workspace and reopens source datasets; vendor data is converted again. Keep source files at their original paths. Missing runs are reported; restored measurements remain available. Session files contain trace data and can be larger than a settings-only file.
- **Figure export:** **File > Export > Figures (SVG)** exports visible chromatograms or the active mass spectrum as a scalable vector figure with axes, units, and acquisition/trace labels. Chromatogram figures use the full trace extent and full-resolution processed data, independently of the on-screen zoom, and follow the selected stacked/overlay layout (the current grid page).
- **Everyday controls:** `Ctrl+O` opens data, file/folder drag-and-drop imports datasets, and **File > Recent files** keeps the last 12 source paths across launches. Arrow keys navigate scans when a text field is not focused; `Esc` clears the integration selection.

The toolbar groups visibility controls under **Panels:**. Toggle **Data files** to show the sample list, **Trace settings** to show extraction/display controls and measurements, or **Mass spectrum** to show the spectrum and scan navigation. **Focus mode** hides all three together; turning it off restores their previous visibility. Opening an individual panel exits focus mode. Viewer sessions preserve the focus setting and the panel visibility to restore. Each pane also has a **Hide** action. The spectrum's **Clear spectrum** action clears the selected spectrum and gives the chromatograms the full available height; collapsing it preserves the spectrum. Panel visibility, grid dimensions, sample-comparison mode, and the shared batch preset are saved in sessions. SVG exports follow the chosen grid dimensions.

### Trace presets and chromatogram context menu

Use **File > Viewer presets > Preset editor…** to create a named analyte preset in a table. Add, duplicate, reorder, or delete analytes; edit acquisition, masses, ppm, smoothing, polarity, and layout. **Paste from Excel…** accepts tab-separated cells with column headers. **Save** and **Save As…** store the preset in any folder you choose; **Open…** and **Recent presets** reopen it for editing. **Apply to viewer samples** applies the current table across samples, independently of saving. New/Open protects unsaved changes with Save, Discard, or Cancel. TOML remains the portable format behind the editor; existing presets are supported. See [`presets/example.toml`](../presets/example.toml). A preset has `version = 1`, optional `overlay = false`, and 1–64 `[[traces]]` entries. Each entry specifies `name`, `acquisition` (`"FS"`, `"SIM"`, or `"MRM"`), optional `kind` (`"XIC"` by default, or `"TIC"`/`"BPC"`), `polarity` (`"positive"` by default, or `"negative"`), and `smoothing` (0–10). XICs need `mass` and optionally `ppm` (default 10). MRM entries require `precursor_mz`; `mass` is the product ion. Optional `ms_level` overrides the default of 1 for FS/SIM or 2 for MRM. TIC/BPC entries may specify `mz_range = [100.0, 700.0]`.

FS selects MS1 spectra excluding SIM/SRM-tagged spectra. SIM and MRM match explicit SIM/SRM spectrum metadata; an ordinary MS2 spectrum does not qualify as MRM. Targeted modes need spectra (vendor conversion requests these); a chromatogram-only source or a file without matching acquisition metadata will report that no matching scans were found. The entire preset is validated before extraction. Extraction runs in the background, and each sample’s results are committed only if all entries succeed. The preset replaces the displayed analyte set for that sample.

The loaded preset is shared across the entire batch, including files opened later. Select a sample in **Data files** to extract its analytes on the first visit; subsequent visits use cached results. The same analyte order and grid dimensions carry across samples. Only the selected sample is displayed by default. Enable **Compare samples** to display a matrix with samples as columns and analytes as rows. Sample names appear above each column; missing or hidden traces leave an empty cell so analytes stay aligned. Comparison temporarily overrides the individual grid/overlay layout without changing its settings. Turn it off to return to the previous layout. The matrix scrolls horizontally and vertically when needed, and SVG export follows the comparison matrix. If a sample cannot satisfy the preset, its failure is reported without repeatedly rerunning extraction; reload the preset to retry. Use **File > Viewer presets > Clear shared batch preset** to return to manual extraction settings.

**Right-click** a chromatogram for spectrum inspection at the clicked RT, file information, plot properties, presets, zoom reset, integration of the visible RT range, measurement history, CSV/figure export, and hide/delete actions for an independent panel. Right-drag still integrates a selected interval. Click a panel's legend name to make it the active trace for analysis.

### Intensity scaling

Use the **Scale** dropdown above the chromatograms to choose **Individual**, **Shared highest peak**, or **Shared custom maximum**. Individual scaling fits each trace independently. Shared highest peak uses a common zero-based intensity axis with 5% headroom above the highest peak among all visible traces, including other grid pages and compared samples. Shared custom maximum lets you enter the upper intensity limit in a.u.; peaks above this limit are clipped visually. These settings change only the display, not the intensities or integration results. Viewer sessions preserve the scale settings, and SVG figures use the same scale. Hide a trace (for example a dominant TIC) to exclude it from automatic shared scaling.

### Chromatogram calculations and export

Full-resolution chromatograms are retained separately from the reduced drawing copy. Moving-average smoothing is applied at full resolution; integration and CSV export use that full-resolution processed trace, including the selected smoothing. Set smoothing to **0** for unsmoothed calculations and export. Rendering keeps at most 2,000 points and preserves bucket minima/maxima and trace endpoints. XICs retain zero-intensity scans, and TIC/BPC traces are derived from the selected spectra so their scan filters and spectrum lookup remain consistent across converted datasets.

### Batch quantification

Both the viewer preset editor and the batch method editor offer **Save example preset…** with **Viewer extraction** and **Batch Quantification** starters. Choose one and save the TOML file anywhere on disk, then open it in the matching editor. Examples are included in the executable, so no internet connection is needed. Saving an example leaves your current edits intact. Adapt the illustrative masses, acquisition modes, RT windows, and detection settings to your samples before applying it.

Choose **Batch Quantification** at the top of the window for a separate workspace for measuring peak areas across samples. Viewer integrations and batch results are independent.

1. Add files or vendor dataset folders and select the loaded runs in **Batch samples**.
2. Choose **Edit method…**. Define each analyte's name, FS/SIM/MRM acquisition, target/product m/z, optional precursor m/z, ppm tolerance, polarity, MS level, expected RT, and RT search window in minutes. MRM requires a precursor. Save the method to TOML using **Save** or **Save As…**; reopen it for future batches. The method uses the existing extraction fields inside each analyte's `extraction` table. See [`presets/quantification-example.toml`](../presets/quantification-example.toml).
3. Choose **Run batch**. Extraction runs in a background worker, one sample/analyte at a time. Progress counts extractions; **Cancel** stops after the current extraction. Completed results remain available and unprocessed results are marked **Cancelled**.
4. Click a cell in the sample × analyte area table to review the peak. **Next unresolved** visits missing, ambiguous, failed, cancelled, or outdated results. Drag either boundary with the left mouse button, right-drag a new interval, or enter exact bounds and choose **Apply bounds**. The area updates from the unsmoothed trace and the result becomes **Manual**. **Accept peak** marks a peak **Reviewed**. **Reset to automatic** restores the original automatic result.
5. **Export CSV…** exports all rows in the batch, with sample/source/run identifiers, analyte, area, apex RT, bounds, height, review status, recalculation flag, extraction parameters, diagnostics, and the original method TOML snapshot. Missing, failed, cancelled, and pending areas remain blank rather than zero. Old analytes/results remain visible when the method changes and are explicitly marked for recalculation.

Peak detection uses a moving average (0–10 neighbouring scans on each side), local maxima, and a minimum local prominence (`minimum_height`) above neighbouring valleys. Among candidates inside the RT window, it chooses the apex nearest the expected RT. Boundaries descend towards the neighbouring valleys until `boundary_fraction` of local height remains. Multiple qualifying candidates or a selected peak clipped by the window are marked **Ambiguous** for review. These are simple, configurable detection rules; they do not deconvolve overlapping peaks. All areas use full-resolution **unsmoothed** XICs with a straight line between the boundary intensities, trapezoidal integration, and interpolated endpoints. Area units are intensity × minutes; apex height is the measured intensity. Batch extraction smoothing must remain 0. Concentration calibration and internal-standard normalization are not included.

Methods store reusable settings, while **Batch session > Save batch…** (`Ctrl+S` / `Cmd+S` in this workspace) writes a `.chromquant` JSON session with selected source/run references, the method, extracted traces, automatic results, and manual corrections. **Batch session > Open batch…** restores these and reopens available sources; vendor datasets are converted again. Stored traces remain reviewable when sources are missing. Keep source datasets at their original paths. Rerunning the identical method preserves **Manual** and **Reviewed** results. Editing any method setting marks previous results **Recalculate**; running the new method replaces affected measurements. Save the old batch first if you need to retain those corrections. Viewer sessions continue to use `.chromascope` and are saved independently.

## Installation

### Downloading pre-built binaries

You can download pre-built binaries for your operating system from the [Releases](https://github.com/adamcseresznye/chromascope/releases) page.

To build the current version locally, use Rust 1.88 or newer and follow the instructions below. On Windows the normal build output is `target\release\chromascope.exe`.


### Building from source

On Ubuntu, install the GUI development libraries used by CI:

```sh
sudo apt-get install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev
```

To build Chromascope from source, follow these steps:

1. **Clone the Repository**:
   ```bash
   git clone https://github.com/adamcseresznye/chromascope.git
   cd chromascope
   ```

2. **Build the Application**:
   ```bash
   cargo build --release --locked
   ```

3. **Run the Application**:
   Windows (PowerShell):
   ```powershell
   .\target\release\chromascope.exe
   ```
   macOS / Linux:
   ```bash
   ./target/release/chromascope
   ```

Close an older copy running from the same output path before rebuilding on Windows. Otherwise Windows may prevent the build from replacing the executable.

### Development checks

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
```

CI runs these checks on Ubuntu with the required Linux GUI libraries and a Rust dependency cache. Real ProteoWizard integration tests are ignored by default because they require a local converter installation and, for vendor acquisition testing, a dataset supplied through `CHROMASCOPE_TEST_VENDOR_FILE`.
