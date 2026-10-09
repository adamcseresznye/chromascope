# Headless AI-assisted analysis

The existing `chromascope-mcp` desktop interface and five original headless
tools retain their names and behavior. `chromascope-engine-mcp` exposes the same
shared analytical engine used by CLI/GUI; it contains no replacement algorithms.

```powershell
cargo build --locked --no-default-features --features mcp-headless --bin chromascope-engine-mcp
target/debug/chromascope-engine-mcp --allow-root DATA_DIRECTORY
```

Paths must resolve within an operator-supplied root. The server never installs
scientific runtimes or sends data to remote engines. Project writes require the
additional operator flag `--allow-project-writes`; it is absent by default.

## Discovery and execution

| Tool | Contract |
|---|---|
| `analysis_capabilities` | Complete generated request/response JSON Schemas, units, effect policy and six workflow recipes/example prompts. |
| `discover_datasets` | One directory, sorted mzML/mzML.gz paths and sizes; 1–1000 rows per page. Listing checksum detects directory changes; re-list if it changes. |
| `start_analysis` | Typed `{path,request}`: version 1, UUID operation ID, nonempty actor, tagged analytical operation. Returns an immutable session job ID without project writes. |
| `analysis_status` | State, completed work-unit count and structured error. Count means scans or kernel phases; there is no invented percentage or ETA. |
| `cancel_analysis` | Cooperative request. Completion can win a cancellation race; decoding/kernel boundaries limit responsiveness. |
| `analysis_result` | RFC 6901 pointer into a successful immutable response; 1–1000 array items or Unicode string characters. Objects expose one level of scalar values, nested object keys and array counts. Job ID/response SHA256 bind pages. |
| `inspect_project` | Existing project revision, datasets, methods, result references and attributed commit history. |
| `commit_analysis` | Existing directory/dataset UUID, successful job ID, expected project revision, approval actor and reason. Explicit write policy, raw hash/replay checks, source authorization and existing append-only project commits apply. |
| `session_audit` | Offset/limit paging of session calls, parameters and outcome hashes/errors. |
| `prepare_analysis_report` | Selected job IDs and optional attributed explanation; full JSON review draft with requests, original results/flags/units, source and artifact hashes, failures/pending states, and audit. Content only; no file overwrite. |

New tools carry MCP read-only/destructive/open-world annotations. Starting jobs
and cancellation alter retained session state; commit is the only new tool that
writes projects. Legacy feature-job tools use the same retained job map.

The operation schema includes metadata, spectra, chromatogram extraction,
integration, quantification methods, chromatographic processing and reversible
boundary corrections; targeted batches with standards/calibration/qualifiers/
internal standards/dilution; QC and validation studies; spectral libraries,
matching/formula/isotope evidence; untargeted features and tentative annotation;
statistics and analytical CSV exports. Parameters are generated from real Rust
DTOs, including nested structures and enums. UUID and finite/physical parameter
validation remains the engine's responsibility. JSON-RPC malformed/type errors
are protocol errors; scientific/access/job errors use structured `code,message`
with MCP `isError=true`. Failed job status itself is a successful observation
with an explicit `error` field.

Example result requests:

```json
{"job_id":"JOB_UUID","pointer":"/output/batch/results","offset":0,"limit":25}
```

```json
{"job_id":"JOB_UUID","pointer":"/request/operation","offset":0,"limit":25}
```

Requests/responses are limited to 16 MiB; at most 16 jobs are retained per server
session, with two workers and a bounded scheduler queue. Object rows can contain
large evidence arrays; reduce the page size or descend to the arrays with a
pointer. Large full reports must be assembled from result/audit pages. Limits
do not certify bounded RSS: JSON serialization, retained evidence and session
audit currently reside in memory.

## Targeted workflow and review

1. Discover datasets and inspect metadata. Supply an explicit sample sheet,
   targets, quantifier/qualifiers, standards, internal standards, units, dilution,
   calibration weighting/range and scientifically justified thresholds.
2. Start `targeted_batch`, poll status, inspect calibrations and quantitative
   results. Examine raw evidence, flags, nulls and questionable integrations.
3. Evaluate targeted QC with explicit rules. Missing required evidence remains
   indeterminate; acknowledgement never makes a failed rule pass.
4. Preview alternative chromatographic boundaries with
   `revise_chromatogram preview=true`. Preview results cannot commit. Review
   original and corrected evidence; produce a reasoned revision only after the
   applicable user review. Stale revisions fail explicitly.
5. Export targeted/QC tables. Optionally commit retained results to a previously
   created/registered project with an explicit dataset association and revision.
6. Prepare a report containing original, QC, corrected/reviewed and export job
   IDs. Retain its complete JSON and checksum, or assemble paged evidence.

Prompts are discoverable in `analysis_capabilities`: investigate chromatographic
anomalies, review integration, construct calibration, troubleshoot QC, compare
processing methods, and prepare a report. They are guidance for the calling
agent; they do not infer thresholds, execute hidden approvals or replace evidence.

Run the real stdio example against a targeted request containing existing sources:

```powershell
python examples/mcp_targeted_workflow.py request.json NEW_OUTPUT --server target/debug/chromascope-engine-mcp.exe --qc-rules rules.json
```

The helper retains input hashes, capabilities, original request, response
snapshots, CSV, draft report, transcript and stderr. It refuses an existing output
directory. QC rules are optional user-supplied data; no hardcoded QC decision,
review or numerical implementation is present in the helper.

## Evidence and remaining boundaries

Every report is a **review draft**, not approved scientific release. AI narrative
is explicitly unverified. Tool actor/approval identity is client-attributed,
not authenticated proof of a human. Similarity/accepted feature hypotheses remain
tentative identification. No approved-only certification is exposed.

Full tool-call audit and general job IDs are session-local; export before server
shutdown. Project commits durably retain the response/request, attributed
approval actor/reason and applied revision; restoration retains that history.
Project creation, source registration/relink, legacy method import and historical
restore remain CLI operations; the new MCP commit tool does not silently create
or register projects. Existing engine QC/replay rules and optional scientific
runtime requirements apply unchanged.

Still open: authenticated human approvals across adapters, durable/restartable
general job/audit storage, native MCP progress notifications/prompts/resources,
approved-only HTML/PDF release bundles, idempotency, disk-backed paging/RSS
benchmarks, and scientist-curated legal instrument workflows. Synthetic raw mzML
and independent analytical/reference tests establish local software behavior,
not instrument or regulatory validation.
