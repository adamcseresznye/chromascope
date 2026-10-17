# Settings

All preferences are in the **View** menu and are saved in one file, `settings.json`, in your user configuration folder: `%APPDATA%\chromascope` on Windows, `~/Library/Application Support/chromascope` on macOS and `~/.config/chromascope` on Linux. Set the environment variable `CHROMASCOPE_CONFIG_DIR` to a folder of your own if you want a portable installation to keep its files elsewhere. If the file is missing or damaged the defaults are used, and one unreadable value never resets the others.

## Appearance

- **Theme**: System, Light or Dark.
- **Interface size**: 90, 100, 115 or 130 percent.
- **m/z decimals**: how many decimals an m/z shows on screen, from 2 to 6. Exports always keep the full precision.

The **View** menu also shows or hides **Data files**, **Trace settings** and **Mass spectrum** in the Data explorer.

## Update check

Chromascope looks for a newer version once a day. When there is one, the status bar says "Chromascope X.Y.Z is available" with two links: **Download** opens the release page in your browser, **Dismiss** hides the notice for that version. Nothing is downloaded or installed by the program.

What is sent: one ordinary web request to `github.com`, made in the background after the window has opened. It carries no name, no file name and no data from your work, only what every web request contains: your IP address and the text `Chromascope/<version>`. It does not use the GitHub programming interface. When you are offline or behind a firewall, nothing is shown and nothing is repeated before the next day.

To switch it off, clear **Check for updates** in the **View** menu, or set the environment variable `CHROMASCOPE_NO_UPDATE_CHECK=1`, for example on a laboratory computer. The command-line tool and the headless MCP server never contact the network.

## Other places with settings

- The path of `msconvert` is set under **File**, **Vendor import settings**.
- What an AI assistant may do is decided when you start `chromascope-mcp`, see [Setup](../mcp/setup.md).
