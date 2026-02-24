<div align="center">

<img src="assets/icon.png" alt="Chromascope logo" width="120" />

# Chromascope

**A lightweight GUI for reading and visualizing mzML mass spectrometry data**

[![Rust](https://img.shields.io/badge/built%20with-Rust-orange?logo=rust)](https://www.rust-lang.org/)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)]()

<img src="assets/demo.gif" alt="Chromascope demo" width="860" />

</div>

---

## Why Chromascope?

mzML is an open, XML-based format widely used for storing and processing mass spectrometry data. While vendor-specific tools handle proprietary formats well, finding a lightweight GUI that works seamlessly with mzML files remains surprisingly difficult. Chromascope fills that gap by offering instant, no-setup inspection of your mass spectrometry data.


## Features

- **Native mzML support**
- **Multi-file analysis** — load and compare multiple files simultaneously
- **TIC, BPC, and XIC extraction**
- **Customizable display** — line color, style, smoothing, line width
- **Cross-platform** — Windows, macOS, and Linux


## Interactive controls and Pro-tips

Chromascope is designed to be highly interactive. Here are a few essential controls that might not be immediately obvious:

- **View Mass Spectra**: **Double-click** anywhere on a chromatogram to extract and display the mass spectrum for that specific retention time.
- **Peak integration**: **Right-click and drag** across a peak on the chromatogram to perform a trapezoidal area integration.
- **Advanced filtering**: Use the Plot Properties (open via a right click on the chromatogram) to filter data by MS level (e.g., MS1 vs MS2), polarity, precursor m/z, and specific m/z ranges.

## Usage

1. **Launch Chromascope**:
   - Run the application by executing the binary or running `cargo run` from the project directory.

2. **Open mzML Files**:
   - Use the `File` menu to load one or more mzML files into Chromascope.

3. **Explore Data**:
   - Switch between TIC, BPC, and XIC modes using the Plot Properties. Use the interactive controls (double-click, right-click drag) to extract a corresponding mass spectrum or integrate a peak area.

4. **Customizing Views**:
   - Adjust the display settings via the `Display` menu to customize how your data is presented.

## Installation

### Downloading pre-built binaries

You can download pre-built binaries for your operating system from the [Releases](https://github.com/adamcseresznye/chromascope/releases) page.


### Building from source

To build Chromascope from source, follow these steps:

1. **Clone the Repository**:
   ```bash
   git clone https://github.com/adamcseresznye/chromascope.git
   cd chromascope
   ```

2. **Build the Application**:
   ```bash
   cargo build --release
   ```

3. **Run the Application**:
   ```bash
   ./target/release/chromascope
   ```

## Contributing

We welcome contributions to Chromascope! If you have suggestions for new features, bug reports, or would like to contribute code, please open an issue or submit a pull request. For the contribution guidelines see [here](https://github.com/adamcseresznye/chromascope/blob/main/.github/CONTRIBUTING.md).


## License

Chromascope is licensed under the GPL-3.0 License. See the [LICENSE](https://github.com/adamcseresznye/chromascope/blob/main/LICENSE) file for more details.


## Acknowledgements

The project would not have been possible without these excellent libraries:
- [egui library](https://github.com/emilk/egui) 
- [mzdata](https://github.com/mobiusklein/mzdata)
