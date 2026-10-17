# Built-in workflow prompts

Chromascope offers six ready-made tasks as MCP *prompts*. A prompt is a stored request that your assistant's program can list and start for you, so you do not have to find the words yourself. Both programs, `chromascope-mcp` and `chromascope-engine-mcp`, offer the same six.

## How to start one

1. Connect the assistant as described in [Setup](setup.md).
2. Open your client's menu for prompts. Where it is differs between clients; look for a prompt or attachment menu that lists the server `chromascope`. Choose the prompt by its name.
3. If the client asks for the optional argument `focus`, type what you want the assistant to look at (see below). You may leave it empty.
4. Send the message the client prepared.

If your client has no prompt menu, you lose nothing: describe the task in your own words, using the lines under "When to use" below as a model. `analysis_capabilities` returns the same six recipes as data, so an assistant can also look them up itself.

Every prompt takes one optional argument, `focus`: *what to look at*, for example a sample name, a compound, a job ID or a retention time. The prompt adds it to the request as "Focus: ...". The prompts tell the assistant how to work: find the data, run the calculations as engine jobs with `start_analysis`, follow them with `analysis_status`, read them with `analysis_result`, apply the review rule of the task, and state what is measured and what is only its interpretation. With `chromascope-mcp` the text also tells it how to use your open window, and, when `--allow-changes` is on, to show the result with `show_workspace`.

## The six prompts

### investigate_chromatographic_anomalies

*What it asks:* Investigate this unexpected peak without changing the original result.

*When to use it:* A peak looks odd (a shoulder, a split top, an unexpected signal in a blank) and you want the evidence gathered before you decide anything. The assistant reads the metadata, extracts the trace and the spectrum, processes the chromatogram and looks at the QC flags.

*Review rule it follows:* Compare unsmoothed evidence, baseline and flags; do not invent causes.

*A good `focus`:* `sample 7, 3.2 min`

### review_integration

*What it asks:* Preview alternative peak bounds and present both areas for review.

*When to use it:* You doubt the automatic boundaries and want to see the effect of other ones before you change anything. The assistant keeps the original and the corrected result, each with its reason.

*Review rule it follows:* Use preview first; retain original and corrected revisions with reasons.

*A good `focus`:* `caffeine in sample 7`

### construct_calibration

*What it asks:* Build a weighted calibration from my standards and quantify unknowns.

*When to use it:* You have run standards of known levels and want a calibration and the concentrations of the unknowns. You must tell it the units, which sample is which, the range, any dilution, the weighting and the thresholds; the prompt asks the assistant to declare them and review accuracy, qualifiers and missing values.

*Review rule it follows:* Declare units, roles, range, dilution, weighting and thresholds; review accuracy, qualifiers and missing states.

*A good `focus`:* `caffeine, standards 1 to 6`

### troubleshoot_qc

*What it asks:* Explain QC failures using recorded evidence and identify missing measurements.

*When to use it:* A QC evaluation failed or came back indeterminate and you want to know why. It reads the QC report and says which measurement is missing or out of range.

*Review rule it follows:* Acknowledgement does not erase failures; required absent evidence remains indeterminate.

*A good `focus`:* `batch 2`

### compare_processing_methods

*What it asks:* Compare two processing configurations on the same raw chromatogram.

*When to use it:* You are choosing between two sets of integration or smoothing settings and want the difference in numbers, not in words.

*Review rule it follows:* Run separate jobs with frozen parameters; compare evidence and units, not explanations alone.

*A good `focus`:* `smoothing 0 against smoothing 5, caffeine`

### prepare_report

*What it asks:* Prepare a reproducible draft including questionable results and review history.

*When to use it:* At the end, when you want a report of what was done, including the doubtful results, with the jobs it came from. The report is a review draft. It is never an approved release. The assistant uses `session_audit` and `prepare_analysis_report` to collect the calls and the results.

*Review rule it follows:* Include all relevant job IDs; the report is a review draft, never an approved scientific release.

*A good `focus`:* `all jobs of this session`

## Why use them

The prompts keep the assistant on a procedure that was written for laboratory work: it works on stored evidence, compares like with like, and keeps what you decided separate from what it suggested. For anything else, use your own words; see [Example prompts](example-prompts.md) and [Writing good prompts](writing-good-prompts.md).
