# Setup

This page connects an AI assistant to Chromascope. It takes about ten minutes. You need an assistant that can start a local MCP program over standard input and output (a "stdio" server), for example Claude Desktop. The steps are the same for other clients: they all need the path of a program and a list of arguments.

## 1. Find the programs

Every installer contains the two MCP programs next to the application.

| System | Where the programs are |
|---|---|
| Windows (MSI) | `C:\Program Files\Chromascope\chromascope-mcp.exe` and `chromascope-engine-mcp.exe`. The 32-bit installer uses `C:\Program Files (x86)\Chromascope`. |
| Windows (portable zip) | In the folder where you unpacked it. |
| macOS (dmg) | In the folder *Command line tools* on the disk image. They are not inside the app. Copy `chromascope-mcp` and `chromascope-engine-mcp` to a folder of your own, for example `/Users/<you>/chromascope-tools/`. |
| Linux (deb, rpm) | `/usr/bin/chromascope-mcp` and `/usr/bin/chromascope-engine-mcp`. |
| Linux (AppImage, tar.gz) | In the unpacked folder. The AppImage keeps them in `usr/bin`; run it with `--appimage-extract` to reach them. |

On macOS the programs are not notarised. Run `chromascope-mcp --help` once in Terminal; if macOS refuses, open System Settings, Privacy & Security, and choose *Open Anyway*, then run it again. Do this for each program.

To check that the program works, run it with `--help`. It prints the usage and exits without opening a window:

```sh
chromascope-mcp --help
```

## 2. Choose a folder for the assistant

The assistant may only use files inside folders you name with `--allow-root`. Choose the folder that holds your data and, if you want exports, a folder for new files. A folder must exist. You can name several.

## 3. Put it in your client's configuration

In Claude Desktop open *Settings*, then *Developer*, then *Edit Config*. That opens the file `claude_desktop_config.json`. Add a server called `chromascope`, save the file and restart the client.

=== "Windows"

    Windows paths in JSON use a double backslash.

    ```json
    {
      "mcpServers": {
        "chromascope": {
          "command": "C:\\Program Files\\Chromascope\\chromascope-mcp.exe",
          "args": [
            "--allow-root", "C:\\MSData",
            "--allow-root", "C:\\MSData\\Reports",
            "--allow-changes",
            "--allow-exports"
          ]
        }
      }
    }
    ```

=== "macOS"

    ```json
    {
      "mcpServers": {
        "chromascope": {
          "command": "/Users/<you>/chromascope-tools/chromascope-mcp",
          "args": [
            "--allow-root", "/Users/<you>/MSData",
            "--allow-root", "/Users/<you>/MSData/Reports",
            "--allow-changes",
            "--allow-exports"
          ]
        }
      }
    }
    ```

=== "Linux"

    ```json
    {
      "mcpServers": {
        "chromascope": {
          "command": "/usr/bin/chromascope-mcp",
          "args": [
            "--allow-root", "/home/<you>/msdata",
            "--allow-root", "/home/<you>/msdata/reports",
            "--allow-changes",
            "--allow-exports"
          ]
        }
      }
    }
    ```

Replace the folders and `<you>` with your own. The configuration file is in `%APPDATA%\Claude` on Windows and in `~/Library/Application Support/Claude` on macOS.

Other clients that start local programs need the same two things, the command and its arguments. With Claude Code, for example:

```sh
claude mcp add chromascope -- "/usr/bin/chromascope-mcp" --allow-root /home/<you>/msdata --allow-changes --allow-exports
```

!!! warning "Do not wrap the program"
    The client talks to the program over standard output. Do not start it through another program that prints to standard output, and do not add a logging wrapper. Chromascope writes its own log to standard error.

## The launch flags

| Flag | What it does | Start with this |
|---|---|---|
| `--allow-root DIRECTORY` | Names a folder the assistant may read from and write into. Repeat it for more folders. Paths are resolved first, so `..` and shortcuts cannot reach outside. Without any root nothing can be opened. | Always. Name your data folder and nothing broader. |
| `--allow-changes` | Lets the assistant change what is in the window: select and zoom, apply integrations, run a batch, open methods, sessions and projects, change preferences. | Leave it off for your first session. Add it when you trust the workflow. |
| `--allow-exports` | Lets the assistant write new files: figures, tables, methods, sessions, batches, presets, audit packages. It never overwrites an existing file. | Add it when you want files. Use a separate reports folder as a second root. |
| `--allow-project-writes` | Lets the assistant create projects, save revisions, and approve and commit review proposals. | Leave it off until you work with projects. |
| `--tool-profile core` or `--tool-profile full` | Chooses how many tools the program lists. `core` lists the 40 tools of the main workflows, for clients that accept only about 40; other tools are refused when called. `full` lists all 74. | `full` (the default), unless your client complains about too many tools. |

A sensible path to follow:

1. First session: `--allow-root` only. The assistant can open files, draw traces in its own answer, read spectra and preview integrations. Nothing in your window changes. This is the safest way to see what it does.
2. Add `--allow-changes` so it can show traces in the window, zoom and run batches.
3. Add `--allow-exports` and a reports folder when you want tables and figures written.
4. Add `--allow-project-writes` only when you keep a project and want the assistant to save revisions.

The headless program takes `--allow-root` and `--allow-project-writes`. It has no window, so `--allow-changes`, `--allow-exports` and `--tool-profile` do not exist there.

## The program without a window

`chromascope-engine-mcp` offers the 14 calculation tools only. Use it on a server, in a script, or when you do not need a window. Its results stay with the assistant; nothing is drawn.

=== "Windows"

    ```json
    {
      "mcpServers": {
        "chromascope-analysis": {
          "command": "C:\\Program Files\\Chromascope\\chromascope-engine-mcp.exe",
          "args": ["--allow-root", "C:\\MSData"]
        }
      }
    }
    ```

=== "macOS and Linux"

    ```json
    {
      "mcpServers": {
        "chromascope-analysis": {
          "command": "/usr/bin/chromascope-engine-mcp",
          "args": ["--allow-root", "/home/<you>/msdata"]
        }
      }
    }
    ```

Its jobs and its audit log live in memory, so export what you need before you close it. `commit_analysis` also needs `--allow-project-writes`, an existing project, an expected revision, an actor and a reason.

## Check that it works

1. Restart your client after you saved the configuration. A Chromascope window opens, titled *Chromascope — MCP*. (The headless program opens no window.)
2. Ask the assistant: *List the mzML files in `C:\MSData`.* (use your folder). It should call `list_mzml_files` and answer with your files.
3. Look at the tab **AI activity** in the window. The call is listed there with its exact request and the permissions the program was started with.
4. If you started it with `--allow-changes`, ask: *Open one of those files and show me its TIC.* The trace appears in the window.

If the window does not open or the client shows no tools, see [MCP troubleshooting](troubleshooting.md).

## Good to know

- Closing the window ends the connection, and disconnecting the client closes the window.
- Each time a client starts the program it gets its own empty workspace. Save a session if you want to continue later.
- A graphical desktop is required for `chromascope-mcp`.
- There is no network transport. The program talks to the client that started it, nobody else.
