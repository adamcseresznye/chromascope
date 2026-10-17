# Resources

An MCP *resource* is a piece of information that the program publishes so that your client can read it without making a tool call. Some clients let you attach a resource to a conversation; others read them on their own. `chromascope-mcp` publishes what is in your window as resources. `chromascope-engine-mcp` has no window and publishes none.

You do not have to use resources. Everything here is also available through tools such as `gui_state`, `dataset_metadata` and `export_results`. Resources are convenient when you want the assistant to start from what you have on screen.

## What is published

| Address | Content | Same as |
|---|---|---|
| `chromascope://datasets` | The open datasets: number, stable ID, path, loading state, last extraction. | `list_datasets` |
| `chromascope://dataset/<id>` | One dataset: scans, retention-time and m/z range, scan filters, stored chromatograms. | `dataset_metadata` |
| `chromascope://dataset/<id>/traces` | The current and retained traces of a dataset, with their extraction settings and up to 10,000 points each. | `chromatogram_data` (which pages) |
| `chromascope://quantification` | The batch quantification results, the method and a list of the calibration runs. | `quantification_status` |
| `chromascope://quantification/run/<n>` | One calibration and concentration run with its request, calibrations, results and reviews. | `export_results` |
| `chromascope://qc` | The QC reports of the Quantification and QC tab with their verdicts. | |
| `chromascope://qc/<n>` | One QC report with its study, decisions and reviews. | |
| `chromascope://project` | The open project: ID, revision, registered datasets and results. | `inspect_project` |
| `chromascope://project/revision/<n>` | One saved revision of the open project, as stored. | |

In the addresses, `<id>` is the stable ID of a dataset (the SHA-256 of its file, the same after a restart) or the number from `chromascope://datasets`, and `<n>` is a zero-based position for runs and reports and a revision number for projects (it starts at 1).

Only what exists is listed: before you open a file only `chromascope://datasets` appears, and the quantification, QC and project addresses appear once the window holds such results. The clients ask for the list with `resources/list` and for the address patterns with `resources/templates/list`.

## When a client uses them

- *At the start of a conversation,* to see which files are open and which results exist, without the assistant calling several tools.
- *After a long calculation,* to read a finished calibration or QC report exactly as the window holds it.
- *When you attach a resource by hand,* to point the assistant at one dataset or one report.

An address that does not exist (a dataset that is not open, a run number that is too large) gives an error that says so. A project revision can be read only when its folder lies inside an `--allow-root` folder.
