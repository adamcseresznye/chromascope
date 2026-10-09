# Project delivery and interoperability

`chromascope-cli project-report PROJECT NEW_DIRECTORY [CONFIG.json]` exports a
review draft HTML report, full machine-readable report.json, original immutable
project revisions/results, native analytical CSV/TSV tables, and vector SVG
figures for direct chromatogram/spectrum responses, targeted quantifier/qualifier
traces, calibration observations/fits and chromatographic raw/integration segments. SVG is resolution
independent; dimensions are configurable (320–16384 by 240–16384). No decimation.
`project-bundle` has the same arguments and additionally includes verified raw
sources from every historical snapshot. Existing output directories are refused.
Partial failed exports are retained and lack a completed checksums manifest.

Config fields: title, sections (metadata/methods/chromatograms/spectra/calibration/quantification/qc/annotations/statistics/evidence/history/versions),
figure_width, figure_height. Presentation selection never removes original
machine-readable evidence. Evidence includes retained metadata, calibration,
quantification, QC, annotations, statistics, spectra/chromatograms, parameters,
units, missing states, provenance and reviews where those analyses were actually
performed. Absent analyses are not invented. HTML is an inspection draft with
full structured evidence, not a certified release or PDF layout.

Copy the entire bundle to a collaborator. Run `project-verify BUNDLE`, then
`project-inspect BUNDLE` and `project-sources BUNDLE`. project-inspect also works
when raw sources are missing; numerical result checksums remain mandatory.
The portable source map confines paths inside the bundle and preserves original
result bytes/requests. `project-reprocess BUNDLE RESULT_UUID` re-executes the
original request through the shared engine and compares output exactly, without
committing or replacing original results. A mismatch is explicitly reported;
TargetedBatch comparisons explicitly exclude only /batch/batch_id and
/batch/created_unix_ms, and list those pointers. All other analytical values,
parameters, flags, reviews and versions are compared exactly. Generated IDs/time
inside other operations can still make comparisons fail. This is not a numerical-tolerance or scientific-equivalence
claim. External OpenMS/statistics operations require the original compatible
Python/library runtimes documented in ARCHITECTURE.md and retained provenance.
Cargo.lock and its SHA256 are exported; bundled software is not installed.
Portable exports also include a local software source snapshot (src/docs/examples/
assets, Cargo.toml, Cargo.lock, LICENSE), with file checksums and a headless build
recipe. This requires the build repository to remain available at its original
path. A source snapshot is not authenticated proof of compiled-binary equivalence.
Raw sources retain their original bytes (including gzipped mzML).

Project schema 1 is the current schema; old schema-1 snapshots with omitted
optional history fields migrate in memory through serde defaults.
`project-migrate PROJECT` materializes these current defaults as a new revision;
all original snapshots remain intact. Future/unknown
schemas fail without rewriting originals. No schema-0 format is fabricated.
`project-compare-methods LEFT.json RIGHT.json` compares parameters by JSON pointer.
Historical restoration uses the existing project-restore command; sources are
verified before reprocessing. Bundles/checksums are local audit evidence, not
cryptographically signed or authenticated approvals. Keep a read-only delivery
copy; new commits invalidate its export inventory until a new export is made.

Python:
```python
import json, pathlib
p = pathlib.Path("BUNDLE")
report = json.loads((p / "report.json").read_text(encoding="utf-8"))
for response in report["results"]:
    print(response["request"], response["output"])
# pandas.read_csv(csv_path) or read_csv(tsv_path, sep="\t")
# Preserve null/missing flags and explicit units; do not replace missing with zero.
```
R:
```r
report <- jsonlite::fromJSON("BUNDLE/report.json", simplifyVector=FALSE)
table <- read.csv("BUNDLE/RESULT-quantification.csv", check.names=FALSE)
# read.delim(..., check.names=FALSE) for TSV. UUIDs remain strings.
```
Run CLI requests from Python subprocess or R system2 with retained request JSON;
never evaluate report strings as code. See examples/*_workflow.py for supported
scientific runtime requirements and raw-data reproducibility workflows.

Desktop: Project reports and delivery opens configuration, directory selection,
source states and collapsible retained requests/results. Reprocess compares on a
worker; export also runs on a worker. Evidence reopening/verification is currently
synchronous. MCP export_project_report accepts directory/config and returns full
JSON/HTML content within the existing 16 MiB transport bound; every source or
missing source parent must be inside an allowed root. MCP never writes the bundle.
CLI commands return existing structured ok/result/error envelopes.

Limitations: HTML structured-evidence inspection remains verbose; PDF, PNG/TIFF,
interactive report charts, authenticated approved-only release and automated
Python/R environment provisioning remain pending. Centroid spectrum SVG uses sticks at observed masses; raw numerical spectra
remain authoritative. SVG filenames use result
UUID and deterministic indices; source names and links are retained in JSON.
No bounded-RSS, native-dialog, instrument-study or OneDrive recovery claim.
Export takes the existing single-writer lock to freeze cooperative project edits.
It does not lock externally modified raw files; hashes are verified after copy.
Partial export directories are never automatically removed or overwritten.
