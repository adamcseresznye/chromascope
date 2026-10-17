# Download and install

Pick your system. The buttons look up the newest release in your browser and download the file straight away. If your browser cannot reach GitHub, they open the [releases page](https://github.com/chromascope-ms/chromascope/releases) instead.

<p class="download-status" data-download-status>Looking up the newest release...</p>

## Windows

[Installer (.msi), 64-bit](https://github.com/chromascope-ms/chromascope/releases/latest){ .md-button .md-button--primary data-asset="-windows-x64.msi" }
[Portable zip, 64-bit](https://github.com/chromascope-ms/chromascope/releases/latest){ .md-button data-asset="-windows-x64-portable.zip" }

Other builds: [Installer, ARM64](https://github.com/chromascope-ms/chromascope/releases/latest){ data-asset="-windows-arm64.msi" } · [Installer, 32-bit](https://github.com/chromascope-ms/chromascope/releases/latest){ data-asset="-windows-x86.msi" }

1. Run the `.msi`. Windows asks for administrator approval, because the program installs for all users.
2. Windows SmartScreen may say *Windows protected your PC* and name an unknown publisher. The program is not code-signed. Choose *More info*, then *Run anyway*.
3. Start Chromascope from the Start menu.

The installer adds its folder to `PATH`. Open a new terminal afterwards if you want to use `chromascope-cli`. The portable zip needs no installation: unpack it anywhere and run `chromascope.exe`.

## macOS

[Disk image (.dmg), Apple silicon and Intel](https://github.com/chromascope-ms/chromascope/releases/latest){ .md-button .md-button--primary data-asset="-macos-universal.dmg" }
[Zip](https://github.com/chromascope-ms/chromascope/releases/latest){ .md-button data-asset="-macos-universal.zip" }

1. Open the `.dmg` and drag Chromascope to Applications.
2. Start it once. macOS refuses, because the app is not notarised.
3. Open System Settings, then Privacy & Security. Scroll down to the message about Chromascope and choose *Open Anyway*. On macOS 15 and later there is no right-click shortcut for this.

The command-line tools (`chromascope-cli`, `chromascope-mcp`, `chromascope-engine-mcp`) are in the folder *Command line tools* on the disk image. They are unsigned too, so macOS asks for the same approval the first time you run each one. The zip contains all four programs side by side.

## Linux

[Debian and Ubuntu (.deb)](https://github.com/chromascope-ms/chromascope/releases/latest){ .md-button data-asset="-linux-x86_64.deb" }
[Fedora and openSUSE (.rpm)](https://github.com/chromascope-ms/chromascope/releases/latest){ .md-button data-asset="-linux-x86_64.rpm" }
[AppImage](https://github.com/chromascope-ms/chromascope/releases/latest){ .md-button data-asset="-linux-x86_64.AppImage" }
[Archive (.tar.gz)](https://github.com/chromascope-ms/chromascope/releases/latest){ .md-button data-asset="-linux-x86_64.tar.gz" }

These are the x86_64 builds. The same four kinds of file exist for aarch64 (ARM) on the [releases page](https://github.com/chromascope-ms/chromascope/releases).

```sh
sudo apt install ./chromascope-<version>-linux-x86_64.deb
sudo dnf install ./chromascope-<version>-linux-x86_64.rpm
chmod +x chromascope-<version>-linux-x86_64.AppImage && ./chromascope-<version>-linux-x86_64.AppImage
```

The AppImage needs FUSE. If it is missing, run the AppImage with `--appimage-extract-and-run`.

## Check your download

Every release lists a file `SHA256SUMS.txt`. To check a file, compute its SHA-256 and compare it with the line for that file.

```sh
sha256sum chromascope-<version>-linux-x86_64.deb
```

On Windows use `certutil -hashfile <file> SHA256`.

## Updates

Chromascope looks once a day for a newer release and tells you on the status bar. It never downloads or installs anything by itself. See [Settings](../manual/settings.md#update-check) to turn this off.

## Build from source

You need Rust 1.88 or newer, a C/C++ toolchain, and on Ubuntu the packages `libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev`.

```sh
git clone https://github.com/chromascope-ms/chromascope.git
cd chromascope
cargo build --locked --release --bin chromascope
cargo build --locked --release --no-default-features --bin chromascope-cli
cargo build --locked --release --no-default-features --features mcp-headless --bin chromascope-engine-mcp
cargo build --locked --release --features mcp --bin chromascope-mcp
```

The programs are in `target/release/`. On Windows, close a running copy before you rebuild, or the linker cannot replace the file.
