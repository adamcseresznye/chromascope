# Limitations

Please read this page before you rely on a result.

!!! warning "Not instrument-validated"
    Chromascope is tested with authored mzML files and independent numerical references. These tests show that the software computes what it says it computes. They do not certify an instrument, a method or regulatory compliance. Validate your own assay with your own standards.

## What is supported

- Chromatographic smoothing, baselines, noise estimates, peak detection and reversible boundaries.
- Weighted linear and quadratic calibration with dilution, qualifier and internal-standard checks.
- Explicit batch QC metrics, where an acknowledgement never turns a failure into a pass.
- Append-only projects, verified audit packages and draft reports (HTML, JSON, CSV, TSV, SVG).
- Files: mzML and mzML.gz directly; vendor files through a ProteoWizard `msconvert` you install.

## What is not supported

- DIA deconvolution.
- A calibrated identification probability or false-discovery rate.
- A general engine for lipid fragments.
- Paired, covariate or repeated-measures statistics.
- Authenticated approval and release of results. Approvals are attributed to a name, not authenticated.
- PDF or raster reports. Reports are review drafts.
- Restartable long jobs. Jobs and the AI audit log live in memory for the session; export before you close.
- Ion-mobility and imaging views.

## Things to know

- Vendor import has been tested only with a fake converter. Real vendor files and real ProteoWizard are not covered by the automated tests.
- The thresholds of the peak confidence score are defaults. They have not been calibrated on a labelled set of peaks.
- A complete check of every XML, binary and compression defect in an mzML file is not finished.
- Retained evidence and history grow memory use. Opening and replaying sessions run in the foreground and can pause the window.
- Native file dialogs and complete desktop workflows still need testing on every platform, and the installers have not been tested on every system.
- Saving a project to a cloud-synchronised folder such as OneDrive: recovery after a crash has not been verified there.
- Three workflows are built but hidden until they have been verified: spectral identification, untargeted analysis and statistics. They need a build with the `experimental` feature and are not described in this manual. Their runtimes are supplied by the operator: pyOpenMS 3.5.0 through the environment variable `CHROMASCOPE_OPENMS_PYTHON` for untargeted features, and Python 3.11 or newer with NumPy 1.26 or newer (below 3) and SciPy 1.11 or newer (below 2) through `CHROMASCOPE_STATS_PYTHON` for statistics.

## What is promised

- Raw files are never modified. Exports and saves create new files, and a save over an existing file goes through a temporary file, so a crash cannot leave half a file.
- Results keep their units, settings, software versions and the hashes of their sources. Missing values stay visibly missing.
- A missing program or runtime gives an error, never a quiet substitute.
- Exact replay of a result has limits (generated identifiers, external programs). Keep the complete JSON, the source files and the recorded environment, not only the CSV.
