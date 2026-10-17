# MCP troubleshooting

## The client does not see the server

1. *Is the path right?* Run the same command in a terminal: `chromascope-mcp --help` (with the full path). It must print a usage text. If it does not, the path is wrong or the program is blocked (on macOS, approve it under System Settings, Privacy & Security, *Open Anyway*).
2. *Is the configuration valid JSON?* A missing comma or an unescaped backslash is the usual cause. On Windows write `C:\\Program Files\\...` with double backslashes.
3. *Did you restart the client?* Most clients read the configuration only at start.
4. *Does a folder in `--allow-root` exist?* The program refuses to start when a root is missing. Run the command in a terminal to see the message; a misspelled option is reported by name as an unknown option.
5. *Does the client say it has too many tools?* Add `--tool-profile core` to the arguments.
6. *Did something else print on standard output?* Do not start the program through a wrapper that adds text. Chromascope logs to standard error.

## The window does not open

- `chromascope-mcp` needs a graphical desktop. It cannot run on a server without a screen. Use `chromascope-engine-mcp` there.
- A closed window ends the connection, so the client must start the program again.
- Run the command in a terminal. Logs go to standard error; set `RUST_LOG=debug` to see more.

## A tool is refused

The message says why. The common ones:

| The assistant reports | What it means | What to do |
|---|---|---|
| it needs `--allow-changes` | The call would change the window. | Restart the program with `--allow-changes` if you want that. |
| it needs `--allow-exports` | The call would write a file. | Add `--allow-exports`. |
| it needs `--allow-project-writes` | The call would change a project. | Add `--allow-project-writes`. |
| the path is outside every allowed root | The file or folder is not inside an `--allow-root` folder. | Add the folder as another `--allow-root`, or use a file inside an existing one. |
| the file exists | Exports never overwrite. | Ask for a new file name. |
| the tool is not in the core profile | You started with `--tool-profile core`. | Use `--tool-profile full`, or leave the flag out. |
| it cannot mark a peak as reviewed | This is deliberate. | Press **✓ Accept peak** in the window. |
| no converter is configured | A vendor file needs `msconvert`. | Set it in **File**, **Vendor import settings**; the assistant cannot do it for you. |
| the result is not shown in the window | The engine job finished without `--allow-changes`. | Restart with `--allow-changes`, or ask for the result in the chat. |

To add a flag, change the `args` in your client's configuration and restart the client. A running window cannot be given new permissions.

## The numbers differ from what I did by hand

- Ask for the settings used (tolerance, smoothing, polarity, MS level) and compare them with yours.
- Viewer integration uses the processed trace, including smoothing. Batch integration uses unsmoothed traces. Set smoothing to 0 to compare.
- `integrate` uses a straight-line baseline between your two boundaries and returns the area in intensity times minutes.

## Where to look

- The tab **AI activity** in the window shows every call and failure with its exact request.
- `session_audit` lets the assistant read the same list.
- Report problems at [github.com/chromascope-ms/chromascope/issues](https://github.com/chromascope-ms/chromascope/issues). Say which program (`chromascope-mcp` or `chromascope-engine-mcp`), the flags you used, your client, and the failing call from **AI activity**.
