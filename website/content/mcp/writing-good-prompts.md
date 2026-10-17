# Writing good prompts

An assistant works best when your request is specific. These habits save time and make the result easier to trust.

## Say where things are

Name the folder and the files. The assistant can only use folders you gave with `--allow-root`, and a vague request such as "my caffeine data" costs it several calls to find out which files you mean. Write `the files in C:\MSData\batch1` and say which are standards, blanks, QC samples and unknowns. If a sample sheet exists, give its path.

## Give the numbers that decide the result

The assistant cannot know your method. State:

- the target m/z and the tolerance, with the unit: `m/z 195.0877, 10 ppm`;
- polarity and MS level, and the acquisition type for targeted work;
- the expected retention time and a search window;
- for calibration: the units, the level of every standard, the fit and weighting you want, and any dilution;
- for QC: the limits. Do not let the assistant choose acceptance limits for you.

If you leave something out, a good assistant asks. If it does not ask and picks a value, read its answer to see which value it used.

## Ask for a preview before a change

Integrations, batch runs and loads change what is in your window. Say so in the prompt: *"Preview the integration and show me the area before you record anything."* The tool `integrate` previews by default and records only when told to. A request such as *"Run the method on all samples, but wait for my OK before you export"* keeps you in the loop.

## Ask for the evidence

Add one of these to a request that matters:

- *"Show me the points or the picture you looked at."*
- *"Give me the job ID and the settings you used."*
- *"List every sample you changed and every file you wrote."*
- *"Say what is measured and what is your interpretation."*

The evidence is in the window too: the tab **AI activity** lists every call with its exact request.

## Start small

Run the new workflow on two samples before you use it on sixty. Check one area by hand. Look at one chromatogram yourself.

## Keep the permissions tight until you trust it

Start with only `--allow-root`. Add `--allow-changes` when you are happy with what the assistant reads, `--allow-exports` when you want files, and `--allow-project-writes` last. See [Setup](setup.md#the-launch-flags).

## Review flagged peaks yourself

An assistant can correct boundaries, but it cannot mark a peak as reviewed; that is your decision, made with **✓ Accept peak** in the window. Treat the assistant's reading of a picture as a suggestion. The numbers come from Chromascope; the explanation of why a peak looks odd is the assistant's opinion.

## Use the right words

| Say | Avoid |
|---|---|
| "XIC for m/z 258.1109 at 10 ppm" | "the caffeine peak" without a mass |
| "integrate between 3.11 and 3.36 minutes" | "integrate the peak" when the file has several |
| "preview only" | "check the area" (it may record it) |
| "save to a new file C:\MSData\Reports\x.csv" | "save it" (it needs a path inside an allowed folder, and a new name) |
| "use smoothing 0 for the reported numbers" | "clean up the trace" |

## When you get a wrong answer

Ask the assistant to name the tool call that produced the number, then compare it with the call in **AI activity**. If the settings differ from what you meant, say so and repeat the step. Nothing the assistant did to the window is permanent until you save it; **File**, then **Open viewer session…** returns to an earlier state.
