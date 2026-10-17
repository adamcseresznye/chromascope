# Command line

`chromascope-cli` runs the same analysis engine as the window from a terminal or a script. It reads one JSON request on standard input and prints one JSON answer. The numbers are the numbers the window gives.

## The idea

A request has a version (always 1), an `operation_id` (a UUID you make up), an `actor` (your name or the name of the script) and an `operation`. The answer is `{ "ok": true, "result": ... }` or `{ "ok": false, "error": { "code": ..., "message": ... } }`, and the exit code is 1 on failure. Progress goes to standard error. Run the program without arguments to print its usage.

The first argument after `run` is the data set. Use `-` for operations that carry their own evidence.

```powershell
'{"version":1,"operation_id":"703ee3bb-3a18-47b1-b51d-dfe001963bfe","actor":"local-analyst","operation":{"operation":"metadata"}}' | chromascope-cli run sample.mzML
Get-Content targeted-request.json -Raw | chromascope-cli run -
```

Ready-made requests and Python helpers are in the [`examples` folder](https://github.com/chromascope-ms/chromascope/tree/main/examples) of the repository; its README explains each one. The targeted starter holds only an unknown sample, so it cannot establish a calibration; the QC starter holds authored demonstration inputs, not instrument-validation data. Use your own standards and justified settings for real work.

## Projects from the command line

```powershell
chromascope-cli project-create NEW_PROJECT_DIRECTORY
'{"version":1,...}' | chromascope-cli run sample.mzML NEW_PROJECT_DIRECTORY
chromascope-cli project-report NEW_PROJECT_DIRECTORY NEW_REPORT_DIRECTORY
chromascope-cli project-bundle NEW_PROJECT_DIRECTORY NEW_BUNDLE_DIRECTORY
chromascope-cli project-verify NEW_BUNDLE_DIRECTORY
```

`run DATASET PROJECT` registers the source and commits the answer to the project. Export paths must be new. A bundle is a folder with checksums, the historical raw inputs and software snapshots; `project-verify` checks it. A bundle also holds a file `INTEROPERABILITY.md` that describes how to replay it.

## Where it is

The installers put `chromascope-cli` next to the application (and, on Windows, on the `PATH`; open a new terminal after installing). On macOS it is in the folder *Command line tools* of the disk image. The CLI never contacts the network.
