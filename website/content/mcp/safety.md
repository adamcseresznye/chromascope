# Safety and data

This page says what the launch flags protect, what is written where, and who decides what. The short version: the assistant proposes, you approve.

## What the flags protect

- *Your files.* Chromascope never modifies an original data file. The assistant can only read inside the folders you named with `--allow-root`. Paths are resolved before they are used, so `..` and shortcuts cannot escape, and a path outside every root is refused.
- *Your window.* Without `--allow-changes` the assistant cannot select, zoom, apply integrations, run batches or replace what is in the window. Its previews do not touch your measurements.
- *Your folders.* Without `--allow-exports` it cannot write any file. With it, it writes new files only. An export never overwrites an existing file. Saving over a file you own (for example a session) goes through a temporary file in the same folder that is then renamed, so a crash cannot leave half a file.
- *Your projects.* Without `--allow-project-writes` it cannot create a project, save a revision, or approve or commit a review proposal. Revisions are append-only: an earlier revision is never lost.

The roots are trusted local storage, not a sandbox. Do not name a folder that another program is changing at the same time.

## What the assistant never decides

- Which converter program Chromascope runs (the operator decides).
- Whether a peak was reviewed by a person (only **✓ Accept peak** does that).
- Whether a failing QC rule passes. An acknowledgement stays a note with a reason.
- Whether a similarity score confirms an identity. It never does.

An approval that goes through `resolve_review_proposal` is attributed to the name the assistant was told, but it is not authenticated: there is no login. Treat names in the record as labels.

## What is recorded

The tab **AI activity** lists every call an assistant made in this session: the tool, the exact request (a request larger than 64 KiB is summarised by its size and its SHA-256), the outcome, and the permissions the program was started with. Failures are listed too. The list lives in memory for the session. To keep it, export it before you close the program: `prepare_analysis_report` collects it with the jobs into a report draft, and an audit package keeps the project evidence.

The results keep the units, settings and software versions they were made with, and the hash of the source file. Corrections made by an assistant stay distinct from a human review. Text written by an assistant is never stored as a measured value, and an assistant's explanation is marked as unverified in reports.

## What leaves your computer

The assistant is another program, and what it reads from a tool reply goes wherever that program sends it. If your assistant runs in the cloud, the results it reads (areas, names, retention times, pictures of a plot) are sent to its provider. Check the policy of your provider before you use confidential data, and consider a model that runs on your own computer for sensitive work.

Chromascope itself makes no network request on the assistant's behalf and runs no shell. The only network access of the desktop program is its once-a-day update check, described under [Settings](../manual/settings.md#update-check). The headless program never uses the network.

## Limits to know

- The program talks to the client that started it, over standard input and output. There is no network server and nobody else can connect.
- Jobs and the audit log are session-local.
- The queue holds 16 requests and runs one expensive operation at a time.
- Vendor import is tested only with a fake `msconvert`.
