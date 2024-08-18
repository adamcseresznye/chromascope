<p align="center">
<img src="assets/original_icons/logo1.png" alt="Chromascope logo" width="200" />
</p>

# Chromascope

Chromascope is a lightweight and easy-to-use GUI application designed for reading and visualizing mzML mass spectrometry data.

## Why

mzML is an open, XML-based format, commonly used for storing and processing mass spectrometry data. While vendor-specific mass spectrometry files are straightforward to open and read, finding a GUI-based tool that handles mzML files with the same ease can be challenging. This project aims to offer a simple, lightweight application for quickly inspecting mass spectrometry data.

## The name

Chromascope, a fusion of ‘chromatography’ and ‘scope, as in telescope,’ embodies the spirit of exploration, in-depth analysis, and the pursuit of insight as you work with mass spectrometry data.

## Features

- **mzML File Support**: Chromascope supports the mzML format, a widely used open standard for mass spectrometry data.
- **User-Friendly Interface**: Easily plot TIC, BPC, or XIC by clicking on the chromatogram, with a triple-click revealing the mass spectrum at any selected retention time.
- **Customizable Display**: Adjust visual settings like smoothing, line color, and line style.
- **Dark Theme Support**: Enjoy an out-of-the-box dark theme for a comfortable viewing experience.
- **Cross-Platform**: The application is built to run smoothly on multiple operating systems, including Windows, macOS, and Linux.

## Usage

<p align="center">
<img src="assets/demo.gif" alt="demo" width="600" />
</p>

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

We welcome contributions to Chromascope! If you have suggestions for new features, bug reports, or would like to contribute code, please open an issue or submit a pull request on our [GitHub repository](https://github.com/adamcseresznye/chromascope).

### Development Setup

1. **Fork the Repository**:
   - Create a fork of the Chromascope repository on GitHub.

2. **Clone Your Fork**:
   ```bash
   git clone https://github.com/adamcseresznye/chromascope.git
   cd chromascope
   ```

3. **Create a New Branch**:
   ```bash
   git checkout -b feature/your-feature-name
   ```

4. **Make Your Changes**:
   - Implement your changes, then commit them.

5. **Push to Your Fork**:
   ```bash
   git push origin feature/your-feature-name
   ```

6. **Submit a Pull Request**:
   - Open a pull request on the original repository to merge your changes.

## Planned improvements
- 🚧 Enable display of MS2 chromatograms and spectra.
- 🚧 Introduce functionality for simple peak integration.
- 🚧 Provide support for handling and processing multiple files simultaneously.

## License

Chromascope is licensed under the MIT License. See the [LICENSE](https://github.com/adamcseresznye/chromascope/blob/main/LICENSE-MIT) file for more details.

## Contact

For any questions or support, feel free to open an issue on the GitHub repository.

## Acknowledgements

The project would not have been possible without these excellent libraries:
- [egui library](https://github.com/emilk/egui) 
- [mzdata](https://github.com/mobiusklein/mzdata)