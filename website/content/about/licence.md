# Licence and notices

## Chromascope

Chromascope is free software, licensed under the [GNU General Public License, version 3 only](https://github.com/chromascope-ms/chromascope/blob/main/LICENSE) (GPL-3.0-only). You may use, study, share and change it under the terms of that licence. If you distribute a changed version, you must distribute its source under the same licence.

Copyright Adam Cseresznye.

## Third-party software

Chromascope is built from open-source Rust libraries, among them egui and eframe (the window), mzdata and mzsignal (reading and processing mass-spectrometry data), nalgebra, the Rust MCP SDK, and, for the once-a-day update check, ureq and rustls. Every package of a release contains the file `THIRD_PARTY_LICENSES.txt`, which reproduces the licence notice of each library. You can also read [the same file on GitHub](https://github.com/chromascope-ms/chromascope/blob/main/THIRD_PARTY_LICENSES.txt).

## Fonts

The program embeds the fonts Inter (SIL Open Font License 1.1) and JetBrains Mono (SIL Open Font License 1.1). Their licences are under **File**, **About Chromascope…**, **Credits & licenses**, and in the release packages.

## Reference data

Some test records of the project come from MassBank and carry per-record attribution. It is listed in `tests/reference/spectral_sources.json` in the source repository.

## This website

The website is built with [MkDocs](https://www.mkdocs.org/) and the [Material for MkDocs](https://squidfunk.github.io/mkdocs-material/) theme.
