# What an AI can do with Chromascope

Chromascope can be driven by an AI assistant. You keep working in the Chromascope window; the assistant does the clicking you would otherwise do, by sending commands over a standard called MCP (Model Context Protocol). You say what you want in plain words, the assistant picks the steps, and Chromascope does the numerical work and shows the result in your window.

Three things stay true however you use it:

- *Chromascope contains no AI.* The assistant is a separate program of your choice (for example Claude Desktop). It reasons; Chromascope calculates. Every area, every calibration and every QC verdict is computed by the same code that the buttons use, so a number from the assistant is the number you get by clicking.
- *You decide what the assistant may do.* When you start the program you give it permissions with launch flags. With none, the assistant can only look at files inside the folders you name.
- *Everything is recorded.* The tab **AI activity** lists every call, with its exact request, as it happens.

## Two programs

| Program | Window | Use it for |
|---|---|---|
| `chromascope-mcp` | Yes, the normal Chromascope window | Everyday work. You watch the assistant work, and its results appear in the Quantification and QC tab. 74 tools. |
| `chromascope-engine-mcp` | No | Scripts, servers and computers without a screen. 14 tools, the calculation tools only. |

Both come with every installer. See [Setup](setup.md).

## The permission flags in one table

| Flag | What it allows |
|---|---|
| (none, only `--allow-root`) | Look: open files inside the named folders, extract traces, read spectra, preview integrations, read results. Nothing in the window changes. |
| `--allow-changes` | Change what is in the window: select, zoom, lay out, apply integrations, run a batch, open methods, sessions and projects, change preferences. |
| `--allow-exports` | Write new files: figures, CSV tables, methods, sessions, batches, presets, audit packages. Existing files are never overwritten. |
| `--allow-project-writes` | Create projects, save revisions, approve and commit review proposals. |

`--allow-root` is needed for everything: it names the folders the assistant may use, and nothing outside them can be read or written. Details and recommendations are in [Setup](setup.md#the-launch-flags).

## What the assistant can do, by task

The last column says which launch flag the tool needs. "read" means `--allow-root` alone. A tool with two entries needs the second one only for the extra effect named.

### Open and inspect data

| The assistant can | Tools | Flag |
|---|---|---|
| list the mzML files in a folder (and vendor data if asked) | `list_mzml_files`, `discover_datasets` | read |
| open up to 64 files; vendor files are converted with your `msconvert` | `open_files` | read |
| list the open datasets and describe the whole state of the window | `list_datasets`, `gui_state` | read |
| read scan count, ranges, filters and stored chromatograms of a file | `dataset_metadata` | read |
| page through scans and read the full metadata of one scan | `list_scans`, `scan_metadata` | read |
| make a dataset the active one | `select_dataset` | `--allow-changes` |

### Chromatograms and spectra

| The assistant can | Tools | Flag |
|---|---|---|
| extract a TIC, base peak or extracted-ion chromatogram, or show a stored SRM/MRM trace | `extract_chromatogram` | read; `--allow-changes` to show it in the window |
| read the points of the extracted trace | `chromatogram_data` | read |
| read a mass spectrum by scan number or retention time | `spectrum` | read; `--allow-changes` to show it |
| look at the plots as pictures | `chromatogram_image`, `spectrum_image` | read |
| zoom, change the layout, show or hide panels, change colours and scaling | `set_view`, `set_layout`, `set_display` | `--allow-changes` |
| show or hide datasets and traces, keep a trace, choose or delete one | `set_visibility`, `set_trace_visibility`, `retain_trace`, `select_trace`, `remove_trace` | `--allow-changes` |
| clear the spectrum panel or the shared batch preset | `clear_display` | `--allow-changes` |

### Integration

| The assistant can | Tools | Flag |
|---|---|---|
| calculate the area of a peak between two times without recording it (a preview) | `integrate` | read |
| record that integration in the window | `integrate` with `apply` | `--allow-changes` |
| undo the last integration, delete a measurement, clear the markers | `undo_integration`, `remove_measurement`, `clear_integration` | `--allow-changes` |
| list the recorded measurements | `measurements` | read |
| save the measurements as a CSV file | `export_measurements` | `--allow-exports` |

### Batch quantification and review

| The assistant can | Tools | Flag |
|---|---|---|
| run a method over many samples | `start_quantification` | `--allow-changes` |
| follow the progress, read every result and the trace behind a peak | `quantification_status`, `quantification_trace` | read |
| correct the boundaries of a result, restore the automatic peak, stop the batch | `review_quantification`, `reset_quantification`, `cancel_quantification` | `--allow-changes` |
| save the batch results as a CSV file | `export_quantification` | `--allow-exports` |

### Calibration, concentrations and QC

These use the shared analysis engine. The assistant starts a job with `start_analysis`, follows it, and reads the stored result page by page. With `--allow-changes` a finished calibration or QC study also appears in the Quantification and QC tab; without it the result stays with the assistant and the window does not show it.

| The assistant can | Tools | Flag |
|---|---|---|
| read the full list of engine operations, their units and the six workflow recipes | `analysis_capabilities` | read |
| start a calibration, quantification or QC job, follow it, read it and cancel it | `start_analysis`, `analysis_status`, `analysis_result`, `cancel_analysis` | read |
| prepare a suggested correction for you to review (nothing is changed) | `prepare_review_proposal` | read |
| read what the assistant did in this session, and prepare a report of it | `session_audit`, `prepare_analysis_report` | read |
| save calibration or QC results as a file | `export_results` | `--allow-exports` |
| load saved calibration runs and QC reports | `import_results` | `--allow-changes` |
| switch the window to the tab, and the stage, where you can see the result | `show_workspace` | `--allow-changes` |

### Figures and exports

| The assistant can | Tools | Flag |
|---|---|---|
| save the trace or the spectrum as a CSV file | `export_csv` | `--allow-exports` |
| save the visible chromatogram or the spectrum as an SVG figure | `export_figure` | `--allow-exports` |
| save measurements, batch results, calibration or QC tables | `export_measurements`, `export_quantification`, `export_results` | `--allow-exports` |
| do several of the steps above in one call (not a transaction) | `batch` | each step needs its own tool's flag |

### Sessions, presets and methods

| The assistant can | Tools | Flag |
|---|---|---|
| list and read preset and method files | `list_presets`, `get_preset` | read |
| write a new preset or method file | `save_preset`, `save_method` | `--allow-exports` |
| apply a preset to the open datasets, or make a method file the method of the tab | `apply_preset`, `open_method` | `--allow-changes` |
| save the viewer workspace or the batch as a new file | `save_session`, `save_batch` | `--allow-exports` |
| load a saved workspace or batch, replacing the current one | `load_session`, `load_batch` | `--allow-changes` |

### Projects and audit packages

| The assistant can | Tools | Flag |
|---|---|---|
| read an existing project and prepare a report draft from it (nothing is written) | `inspect_project`, `export_project_report` | read |
| open a project in the window, or reload it | `open_project`, `refresh_project` | `--allow-changes` |
| create a project, or save a new revision | `create_project`, `save_project` | `--allow-project-writes` |
| build an audit package in a new folder | `build_bundle` | `--allow-exports` |
| check a package, or recompute a stored result and compare it | `verify_bundle`, `reprocess_result` | read |
| queue a review proposal, approve or reject it, commit a job to a project | `queue_review_proposal`, `resolve_review_proposal`, `commit_analysis` | `--allow-project-writes` |

### Settings

| The assistant can | Tools | Flag |
|---|---|---|
| read the preferences, whether the daily update check is on, and whether a newer version exists | `get_settings` | read |
| change the theme, interface size, m/z decimals or the update check | `set_settings` | `--allow-changes` |

## What the assistant cannot do, on purpose

- *Choose the msconvert program.* Picking the executable that converts vendor files decides which program Chromascope runs. That is the operator's decision (the environment variable `CHROMASCOPE_MSCONVERT` or the window's menu), never an assistant's. `open_files` and `list_mzml_files` only report whether a converter is configured.
- *Mark a peak as reviewed by a human.* `review_quantification` can correct boundaries, but it refuses to set the reviewed state. Only you can press **✓ Accept peak**, so a reviewed peak always means a person looked at it.
- *Turn a failing QC rule into a pass.* An acknowledgement records a reason; the verdict stays.
- *Touch anything outside the `--allow-root` folders.* Paths are resolved before they are used, so `..` and shortcuts cannot escape. Without a root nothing can be opened.
- *Overwrite a file.* Exports create new files only.
- *Put a result into the window by itself.* Calibration and QC results reach the window through a private route that only a finished engine job uses.
- *Run a shell, or use the network.*
- *Use unfinished workflows.* Identification, untargeted analysis and statistics are hidden in the standard build.

## Fewer tools for clients that cap them

Some clients accept only about 40 tools. Start `chromascope-mcp` with `--tool-profile core` and it lists only 40 tools for the main workflows. A call to any other tool is refused with a message that names the flag to use. These tools are not in the core profile:

<!-- not-in-core -->
- `scan_metadata`, `discover_datasets`
- `set_layout`, `set_display`, `set_visibility`, `set_trace_visibility`, `select_trace`, `retain_trace`, `remove_trace`, `clear_display`
- `remove_measurement`, `clear_integration`
- `cancel_quantification`
- `import_results`
- `list_presets`, `get_preset`, `save_preset`, `apply_preset`, `save_method`, `open_method`
- `save_session`, `load_session`, `save_batch`, `load_batch`
- `get_settings`, `set_settings`
- `refresh_project`, `reprocess_result`, `inspect_project`, `export_project_report`
- `prepare_analysis_report`, `session_audit`
- `queue_review_proposal`, `resolve_review_proposal`
<!-- end-not-in-core -->

Use the default `full` profile unless your client has a limit.

## Next

[Setup](setup.md), then [example prompts](example-prompts.md).
