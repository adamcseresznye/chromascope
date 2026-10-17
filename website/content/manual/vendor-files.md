# Vendor files

Chromascope opens mzML and mzML.gz directly. Files from instrument vendors are converted first, by a separate program, [ProteoWizard](https://proteowizard.sourceforge.io/download.html) (`msconvert`), which you install yourself.

## Set it up

1. Install ProteoWizard with the vendor readers you need. Most native readers work on Windows only. Which formats work depends on your ProteoWizard build and on the vendor libraries.
2. Chromascope looks for `msconvert` in this order: the environment variable `CHROMASCOPE_MSCONVERT`, the path saved earlier, your `PATH`, and the standard Windows ProteoWizard folders.
3. If it is not found, choose **File**, then **Vendor import settings**, then **Locate msconvert…** and select `msconvert.exe` (not `msConvertGUI.exe`). **Detect msconvert again** searches once more.

## Open the data

- Single files (Thermo `.raw`, SCIEX `.wiff` and `.wiff2`, Shimadzu `.lcd`): **File** then **Open data…**. Keep companion files next to them.
- Folders (Agilent and Bruker `.d`, Waters `.raw`): **File** then **Open dataset folder…**.

Conversion runs on your computer, never changes the original and never uploads anything. A spinner shows the progress, and **Cancel** stops an import. Every run of a multi-sample dataset is added to the file list. Converted files live in a temporary folder while the runs are open, so leave enough disk space; opening the data again converts again.

## Good to know

- The conversion asks for 64-bit mzML with lossless compression, no peak picking and no scan filters. SIM and SRM/MRM data are converted to spectra so that traces can be calculated.
- It adds no ion-mobility or imaging views, and not every vendor-specific field survives the conversion.
- Chromascope has been tested only with a fake `msconvert`, not with real vendor files. Check a converted file against your vendor software before you rely on it.
- If a format has no working reader, convert the file to mzML elsewhere and open that.
