<div align="center">

<img src="assets/icon_large.svg" alt="Chromascope logo" width="120" />

# Chromascope

**A lightweight GUI for reading and visualizing mzML mass spectrometry data**

[![Rust](https://img.shields.io/badge/built%20with-Rust-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)]()

<img src="assets/demo.gif" alt="Chromascope demo" width="860" />

</div>

---

## Why Chromascope?

mzML is an open, XML-based format widely used for storing and processing mass spectrometry data. While vendor-specific tools handle proprietary formats well, finding a lightweight GUI that works seamlessly with mzML files remains surprisingly difficult. Chromascope fills that gap — offering instant, no-setup inspection of your mass spectrometry data.

> **Chromascope** — a fusion of *chromatography* and *scope* (as in telescope) — embodies the spirit of exploration and analytical precision.

## Features


- **mzML File Support**: Chromascope supports the mzML format, a widely used open standard for mass spectrometry data.
- **User-Friendly Interface**: Easily plot TIC, BPC, or XIC by clicking on the chromatogram, with a triple-click revealing the mass spectrum at any selected retention time.
- **Customizable Display**: Adjust visual settings like smoothing, line color, and line style.
- **Dark Theme Support**: Enjoy an out-of-the-box dark theme for a comfortable viewing experience.
- **Cross-Platform**: The application is built to run smoothly on multiple operating systems, including Windows, macOS, and Linux.

## Usage

1. **Launch Chromascope**:
   - Run the application by executing the binary or running `cargo run` from the project directory.

2. **Open an mzML File**:
   - Use the `File` menu to load an mzML file into Chromascope.

3. **Explore Data**:
   - Once the mzML file is loaded, you can use the provided visualization tools to explore the mass spectrometry data. Click on the chromatogram to access options like TIC, BPC, and XIC. To view the mass spectrum at a specific retention time, simply triple-click on the chromatogram at that point.

4. **Customizing Views**:
   - Adjust the display settings via the `Display` menu to customize how your data is presented.

## Installation

### Downloading Pre-built Binaries

You can download pre-built binaries for your operating system from the [Releases](https://github.com/adamcseresznye/chromascope/releases) page.


### Building from Source

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

## Planned improvements
- [x] Provide support for handling and processing multiple files simultaneously.
- [ ] Enable display of SIM and MS2 chromatograms and spectra.
- [ ] Introduce functionality for simple peak integration.

## License

Chromascope is licensed under the MIT License. See the [LICENSE](https://github.com/adamcseresznye/chromascope/blob/main/LICENSE-MIT) file for more details.

## Contact

For any questions or support, feel free to open an issue on the GitHub repository.

## Acknowledgements

The project would not have been possible without these excellent libraries:
- [egui library](https://github.com/emilk/egui) 
- [mzdata](https://github.com/mobiusklein/mzdata)
