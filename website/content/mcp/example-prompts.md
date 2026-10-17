# Example prompts

Thirty-two prompts you can paste into your assistant, from a single look at a file to a complete calibration. Each one lists what the assistant will do, the tools it will call, the launch flags it needs and what you should check in the result.

!!! info "Replace the names with your own"
    The examples use `sample.mzML`, `standards`, the folder `C:\MSData` and the compound caffeine as stand-ins. Change them to your own files, folders and masses. The assistant can only reach files inside a folder you gave with `--allow-root`; every prompt needs that flag, so it is not repeated in the lists below.

The assistant chooses its own steps, so the tools it calls can differ a little from the list. The list shows what the guide expects. In the window, the tab **AI activity** shows what really happened.

## Open and inspect data

### 1. See what is in a folder

```text
Which mzML files are in C:\MSData? Give me the names and sizes.
```

- *The assistant will:* list the files of the folder (not its subfolders).
- *Tools:* `list_mzml_files`
- *Flags:* none besides `--allow-root`.
- *Check:* the names match what you see in the folder. Ask for `include_vendor` if you also want vendor files listed.

### 2. Open a file and show its TIC

```text
Open sample.mzML and show me the TIC.
```

- *The assistant will:* open the file, extract the total ion chromatogram and show it in the window.
- *Tools:* `open_files`, `extract_chromatogram`
- *Flags:* `--allow-changes` (without it the assistant can read the trace, but the window does not show it).
- *Check:* the trace in the window is the TIC of the right file.

### 3. Describe a file

```text
Open sample.mzML and tell me how many scans it has, the retention time range, the polarity and which MS levels are present.
```

- *The assistant will:* open the file and read its metadata and a few scans.
- *Tools:* `open_files`, `dataset_metadata`, `list_scans`
- *Flags:* none besides `--allow-root`.
- *Check:* compare the scan count and the time range with the file information in the window.

### 4. Open a whole folder and compare the files

```text
Open every mzML file in C:\MSData\batch1 and give me a table with the file name, the number of scans and the retention time range.
```

- *The assistant will:* list the folder, open the files (up to 64 in one call), and read each file's metadata.
- *Tools:* `list_mzml_files`, `open_files`, `list_datasets`, `dataset_metadata`
- *Flags:* none besides `--allow-root`.
- *Check:* the number of rows equals the number of files; a file that failed to open is named with its error.

## Chromatograms and spectra

### 5. Extract an ion chromatogram

```text
In sample.mzML extract the XIC for m/z 258.1109 with 10 ppm tolerance, positive polarity, MS level 1, and show it.
```

- *The assistant will:* extract the trace with exactly these settings and display it.
- *Tools:* `extract_chromatogram`
- *Flags:* `--allow-changes` to show it.
- *Check:* the settings under **Trace settings** repeat your mass, tolerance and polarity.

### 6. Compare one mass in three files

```text
Open run1.mzML, run2.mzML and run3.mzML. Extract the XIC for m/z 258.1109 at 10 ppm in each, and overlay them on one plot with a shared intensity scale.
```

- *The assistant will:* open the files, extract the trace in each, switch to overlay and set a shared scale.
- *Tools:* `open_files`, `extract_chromatogram`, `set_layout`, `set_display`
- *Flags:* `--allow-changes`.
- *Check:* three traces in one plot; the legend names the three files.

### 7. Look at the spectrum under a peak

```text
In sample.mzML show me the mass spectrum at 3.2 minutes and tell me the five most intense peaks.
```

- *The assistant will:* read the spectrum of the scan nearest that time, show it, and list the largest peaks.
- *Tools:* `spectrum`, `spectrum_image`
- *Flags:* `--allow-changes` to show the spectrum in the window.
- *Check:* the retention time of the spectrum is what you asked for (it is the nearest scan, so it may differ by a few seconds).

### 8. Ask for a second opinion on a peak shape

```text
Zoom in on the peak around 3.2 minutes in the XIC you just made. Does it look like it has a shoulder or a split top? Show me the picture you looked at.
```

- *The assistant will:* zoom, take a picture of the plot and read the points of the trace.
- *Tools:* `set_view`, `chromatogram_image`, `chromatogram_data`
- *Flags:* `--allow-changes` for the zoom.
- *Check:* look at the same view yourself. The assistant's reading of a picture is an interpretation, not a measurement.

### 9. Use a chromatogram stored in the file

```text
Does sample.mzML contain stored SRM or MRM chromatograms? If so show the first one.
```

- *The assistant will:* read the file's metadata, which lists stored chromatograms, and show one of them.
- *Tools:* `dataset_metadata`, `extract_chromatogram` with `native_chromatogram`
- *Flags:* `--allow-changes` to show it.
- *Check:* the label of the trace matches the transition named in the metadata.

## Integration

### 10. Preview an integration

```text
In the XIC for m/z 258.1109 integrate the peak between 3.11 and 3.36 minutes, but do not record it. Tell me the area and the apex.
```

- *The assistant will:* calculate the area and apex with a straight-line baseline and report them. Nothing is recorded.
- *Tools:* `integrate` (with `apply` left off)
- *Flags:* none besides `--allow-root`.
- *Check:* the area is in intensity times minutes; the window's measurements list has not changed.

### 11. See what smoothing does

```text
Extract the XIC for m/z 258.1109 twice, once with smoothing 0 and once with smoothing 5, and integrate 3.11 to 3.36 minutes in both without recording. How much does the area change?
```

- *The assistant will:* extract two traces and preview two integrations.
- *Tools:* `extract_chromatogram`, `integrate`
- *Flags:* none besides `--allow-root`.
- *Check:* the two areas differ by the percentage the assistant states. Use smoothing 0 for numbers you report.

### 12. Record an integration and save the table

```text
Record the integration between 3.11 and 3.36 minutes and save all measurements to C:\MSData\Reports\measurements.csv.
```

- *The assistant will:* apply the integration, which records it as a measurement, then write the CSV file.
- *Tools:* `integrate` (with `apply`), `measurements`, `export_measurements`
- *Flags:* `--allow-changes`, `--allow-exports`.
- *Check:* the file exists, has one row per measurement, and the extraction settings are in the row. Existing files are never overwritten, so use a new file name.

### 13. Take back the last integration

```text
Undo the last integration you recorded.
```

- *The assistant will:* undo it, unless something else changed in the meantime.
- *Tools:* `undo_integration`
- *Flags:* `--allow-changes`.
- *Check:* the measurement is gone from **File**, **Integration results…**. If the window changed since, the assistant reports that it cannot undo and you remove the row yourself.

## Batch quantification and review

### 14. Run a method on many samples

```text
Open all mzML files in C:\MSData\batch1. Read the method file C:\MSData\methods\caffeine.toml and run it on all of them. Show me the table of areas.
```

- *The assistant will:* open the files, read the method, start the batch, follow its progress and summarise the results.
- *Tools:* `list_mzml_files`, `open_files`, `get_preset`, `start_quantification`, `quantification_status`
- *Flags:* `--allow-changes`.
- *Check:* the Quantification and QC tab shows the batch; the method in the tab is the one you named.

### 15. Find the peaks that need review

```text
Which results of the batch need review, and why? List the sample, the compound and the reason.
```

- *The assistant will:* read every result with its confidence score and flags.
- *Tools:* `quantification_status`
- *Flags:* none besides `--allow-root`.
- *Check:* compare with the colours and flags in the review table. The confidence thresholds are defaults, not calibrated values.

### 16. Correct a boundary

```text
In sample 7, set the boundaries of caffeine to 3.15 and 3.30 minutes and tell me the new area.
```

- *The assistant will:* read the trace behind the result, move the boundaries and report the recomputed area.
- *Tools:* `quantification_trace`, `review_quantification`
- *Flags:* `--allow-changes`.
- *Check:* the peak now shows as *Manual*. It is not marked as reviewed; only you can press **✓ Accept peak**. `reset_quantification` brings the automatic peak back.

### 17. Save the batch results

```text
Export the batch results to C:\MSData\Reports\batch1-areas.csv.
```

- *The assistant will:* write the table with areas, review state, settings and a snapshot of the method.
- *Tools:* `export_quantification`
- *Flags:* `--allow-exports`.
- *Check:* the file has a row for every sample and compound; missing areas are empty, not zero.

## Calibration, concentrations and QC

### 18. The complete workflow

```text
Quantify caffeine in these 12 runs using standards 1 to 6. Show me the calibration and flag anything that needs review.
```

(Name the folder and the file that tells which run is which standard and at which level; the assistant cannot guess concentrations.)

- *The assistant will:* open the runs, extract and integrate caffeine in each, build a weighted calibration from the six standards, calculate the concentrations of the others, switch the window to the calibration stage and list the flags.
- *Tools:* `open_files`, `analysis_capabilities`, `start_analysis` (operations `targeted_batch` and `quantify`), `analysis_status`, `analysis_result`, `show_workspace`
- *Flags:* `--allow-changes` so that the result appears in the window.
- *Check:* every standard has the level you gave; the calibration range covers your unknowns; no concentration is reported outside it without a flag; missing values are empty, not zero. Look at each flagged peak yourself.

### 19. Explain a calibration

```text
Describe the calibration curve you just built: model, weighting, range, and the accuracy of each standard.
```

- *The assistant will:* read the stored calibration result.
- *Tools:* `analysis_result`
- *Flags:* none besides `--allow-root`.
- *Check:* the numbers match the calibration stage of the Quantification and QC tab.

### 20. Evaluate batch QC

```text
Evaluate the QC samples of this batch. Use a limit of 15 percent for the precision of the QC samples and tell me which rules pass, fail or are indeterminate.
```

- *The assistant will:* run a QC evaluation with the limit you stated and report each rule.
- *Tools:* `start_analysis` (operation `evaluate_targeted_qc`), `analysis_status`, `analysis_result`
- *Flags:* `--allow-changes` to see the report in the window.
- *Check:* the limit used is the one you gave, not one the assistant chose. A rule whose evidence is missing must be *indeterminate*, never a pass.

### 21. Find out why QC failed

```text
QC failed for the second batch. Explain which evidence is missing or out of range, and which samples caused it.
```

- *The assistant will:* read the QC report and the calls of this session.
- *Tools:* `analysis_result`, `session_audit`
- *Flags:* none besides `--allow-root`.
- *Check:* every claim names a sample and a number you can find in the report.

## Figures and exports

### 22. Save a figure

```text
Save the chromatogram on screen as an SVG figure in C:\MSData\Reports\xic.svg.
```

- *The assistant will:* write the figure at full resolution.
- *Tools:* `export_figure`
- *Flags:* `--allow-exports`.
- *Check:* open the SVG; the axes carry units and the trace labels.

### 23. Save the trace as numbers

```text
Save the points of the current chromatogram of sample.mzML to C:\MSData\Reports\xic.csv.
```

- *The assistant will:* write retention time and intensity as a CSV file.
- *Tools:* `export_csv`
- *Flags:* `--allow-exports`.
- *Check:* the first and last retention times match the trace; the file is new.

### 24. Save calibration and QC tables

```text
Save the calibration and the QC results as files in C:\MSData\Reports.
```

- *The assistant will:* write the results of the Quantification and QC tab to new files.
- *Tools:* `export_results`
- *Flags:* `--allow-exports`.
- *Check:* the files exist; the call fails with a clear message if the tab holds no results.

## Sessions, presets and methods

### 25. Save and continue later

```text
Save my workspace as a session in C:\MSData\Reports\monday.chromascope.
```

- *The assistant will:* write the session file with sources, traces, measurements and layout.
- *Tools:* `save_session`
- *Flags:* `--allow-exports`.
- *Check:* the file exists. Next time, ask it to load the session; every source must still be in an allowed folder.

### 26. Open yesterday's session

```text
Load the session C:\MSData\Reports\monday.chromascope.
```

- *The assistant will:* replace the current workspace with the saved one and reopen the source files.
- *Tools:* `load_session`
- *Flags:* `--allow-changes`.
- *Check:* the files and traces you expect are back. The current workspace is replaced, so save it first if you need it.

### 27. Build a preset for several compounds

```text
Make a viewer preset for caffeine (m/z 195.0877) and theobromine (m/z 181.0720), both 10 ppm, positive mode, save it as C:\MSData\presets\xanthines.toml, then apply it to the open files.
```

- *The assistant will:* write and validate the preset, then apply it to all open files.
- *Tools:* `list_presets`, `save_preset`, `apply_preset`
- *Flags:* `--allow-exports`, `--allow-changes`.
- *Check:* open the preset in **File**, **Viewer presets**, **Preset editor…**: the two compounds and their masses are as you gave them.

### 28. Keep a method

```text
Save the quantification method that is open in the Quantification tab to C:\MSData\methods\caffeine-v2.toml.
```

- *The assistant will:* write the method file.
- *Tools:* `save_method`
- *Flags:* `--allow-exports`.
- *Check:* the file is new and holds your compounds. Use `open_method` later to load it.

## Projects and audit packages

### 29. Start a project

```text
Create a project for this analysis at C:\MSData\Projects\caffeine-study.json and save the current state as a revision.
```

- *The assistant will:* create the project from the workspace and store a first revision, then save another.
- *Tools:* `create_project`, `save_project`
- *Flags:* `--allow-project-writes`.
- *Check:* the **Project** menu now shows the project name; revisions are listed and earlier ones stay available.

### 30. Build and verify an audit package

```text
Build an audit package for the open project in C:\MSData\Reports\caffeine-package and then verify it.
```

- *The assistant will:* write the package, then check every file against its checksum.
- *Tools:* `build_bundle`, `verify_bundle`
- *Flags:* `--allow-exports`.
- *Check:* the folder contains `report.html` and a checksum file. If the package is refused because of open warnings, the assistant must tell you what they are; it may only continue with your say-so.

### 31. Read an existing project

```text
Read the project C:\MSData\Projects\caffeine-study.json and summarise its revisions, datasets and saved methods.
```

- *The assistant will:* read the project without changing it.
- *Tools:* `inspect_project`
- *Flags:* none besides `--allow-root`.
- *Check:* the revision count equals what the **Project** menu shows after you open the project.

## Settings

### 32. Change the look of the window

```text
Switch the window to the dark theme, interface size 115 percent and 5 decimals for m/z. Then tell me whether a newer Chromascope version is available.
```

- *The assistant will:* change the preferences and read them back, including the update notice.
- *Tools:* `set_settings`, `get_settings`
- *Flags:* `--allow-changes`.
- *Check:* the window looks as asked. The answer about a newer version comes from the program's own daily check; it may say there is none.

## When a prompt does not work

If the assistant says a tool is refused, the message names the missing flag. See [MCP troubleshooting](troubleshooting.md). To get better results from your prompts, read [Writing good prompts](writing-good-prompts.md).
