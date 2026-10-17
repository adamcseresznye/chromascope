# Troubleshooting and FAQ

## The program will not start or is blocked

*Windows says "Windows protected your PC".* The program is not code-signed. Choose *More info*, then *Run anyway*.

*macOS says the app cannot be opened.* Start it once, then open System Settings, Privacy & Security, and choose *Open Anyway* next to the message about Chromascope. The command-line tools in the disk image need the same approval, one by one.

*The Linux AppImage does not start.* It needs FUSE. Run it with `--appimage-extract-and-run` instead.

*I cannot rebuild on Windows.* Close the running copy first, or the linker cannot replace the file.

## My file does not open

*A vendor file or folder gives an error about `msconvert`.* The converter is not found. Install ProteoWizard and use **File**, **Vendor import settings**, **Locate msconvert…**, see [Vendor files](vendor-files.md).

*An mzML file is refused.* The error names the problem. Files are checked while they are read, but a thorough check of every possible mzML defect is not complete, so a damaged file may fail late. Try the file in another tool or convert it again.

## The trace is empty or missing

- *Apply XIC does nothing.* Both the **Target m/z** and the **Tolerance** must be numbers above zero.
- *The XIC is flat.* Check the polarity, the MS level and the acquisition type under **Trace settings**, and widen the tolerance. A tolerance of 10 ppm is only 0.007 at m/z 700.
- *"No matching scans".* A precursor filter accepts scans within 0.01 Da of the value you typed. Choose a precursor that is listed for the file. SIM and MRM traces need spectra; a file that holds only stored chromatograms has none.
- *Peaks look different after smoothing.* Smoothing changes the picture and the viewer's integration. Set **Smoothing** to 0 for unsmoothed numbers. Batch quantification always integrates unsmoothed data.

## A number looks wrong

- *The area is negative.* A baseline can lie above the signal between your boundaries. Negative areas are kept, not clipped, so you can see the problem.
- *The batch area differs from the Data explorer area.* They are independent calculations: the batch uses unsmoothed traces and its own detection settings. Compare the boundaries first.
- *A concentration is missing.* Missing, failed and pending areas stay empty and are never counted as zero. Look at the flags next to the sample and at the calibration range.
- *A QC rule says indeterminate.* The evidence the rule needs is absent. Acknowledging a review records your reason but never turns a failure into a pass.

## Saved work

*A session will not reopen its files.* The source files must be at their original paths. Missing runs are reported and the measurements you made remain available.

*Where are my preferences?* In `settings.json`, see [Settings](settings.md).

## Questions

*Does Chromascope send my data anywhere?* No. Vendor conversion runs on your computer. The only network use of the desktop program is the once-a-day [update check](settings.md#update-check), which sends an ordinary web request and nothing from your work.

*Can I use it for regulated work?* It has not been validated on instruments and it does not certify compliance. See [Limitations](limitations.md).

*Does it contain an AI?* No. If you connect an assistant over MCP, the assistant is a separate program of your choice. See [AI assistants](../mcp/index.md).

*Where do I report a problem?* Open an issue at [github.com/chromascope-ms/chromascope/issues](https://github.com/chromascope-ms/chromascope/issues). Say which version you use (**File**, then **About Chromascope…**) and what you did.
