# Chromascope

Chromascope is an easy-to-use GUI application designed for handling and reading mzML mass spectrometry data. It provides a user-friendly interface to visualize and analyze mass spectrometry data, making it accessible for both beginners and experienced users in the field of mass spectrometry.

## Features

- **mzML File Support**: Chromascope supports the mzML format, a widely used open standard for mass spectrometry data.
- **Intuitive User Interface**: The GUI is designed with simplicity in mind, allowing users to easily navigate and interact with their data.
- **Data Visualization**: Chromascope provides various visualization tools to help users analyze and interpret their mass spectrometry data.
- **Customizable Views**: Users can customize their view settings to match their specific needs and preferences.
- **Cross-Platform**: The application is built to run smoothly on multiple operating systems, including Windows, macOS, and Linux.

## Installation

### Prerequisites

Before installing Chromascope, ensure that you have the following installed on your system:

- **Rust** (for building the application from source)
- **Cargo** (Rust package manager)
- **Git** (to clone the repository)

### Building from Source

To build Chromascope from source, follow these steps:

1. **Clone the Repository**:
   ```bash
   git clone https://github.com/your-username/chromascope.git
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

### Downloading Pre-built Binaries

If you prefer not to build from source, you can download pre-built binaries for your operating system from the [Releases](https://github.com/your-username/chromascope/releases) page.

## Usage

1. **Launch Chromascope**:
   - Run the application by executing the binary or running `cargo run` from the project directory.

2. **Open an mzML File**:
   - Use the `File -> Open` menu to load an mzML file into Chromascope.

3. **Explore Data**:
   - Once the mzML file is loaded, you can use the visualization tools provided to explore the mass spectrometry data.

4. **Customizing Views**:
   - Adjust the display settings via the `View` menu to customize how your data is presented.

## Contributing

We welcome contributions to Chromascope! If you have suggestions for new features, bug reports, or would like to contribute code, please open an issue or submit a pull request on our [GitHub repository](https://github.com/your-username/chromascope).

### Development Setup

1. **Fork the Repository**:
   - Create a fork of the Chromascope repository on GitHub.

2. **Clone Your Fork**:
   ```bash
   git clone https://github.com/your-username/chromascope.git
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

## License

Chromascope is licensed under the MIT License. See the [LICENSE](LICENSE) file for more details.

## Contact

For any questions or support, feel free to open an issue on the GitHub repository.
