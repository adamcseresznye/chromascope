//! What a person can do in the window, and the MCP tool that does the same.
//!
//! Every control the window shows is listed here, grouped by what it does. The test
//! `tests/mcp_parity.rs` reads the window's source, finds every fixed control label and fails
//! when one has no entry, when an entry names a tool the server does not list, and when a tool
//! belongs to no entry or is never called over the protocol by a test. So a new feature in the
//! window is not finished until it has its MCP tool, its entry here and its protocol test.
//!
//! * [`ACTIONS`]: an action and the tools (or, for work the engine does, the engine operations
//!   reached through `start_analysis`) that do it.
//! * [`WITHHELD`]: actions an AI deliberately cannot do, with the reason.
//! * [`FURNITURE`]: controls that start no action of their own (menu titles, dialog buttons,
//!   form editing steps, purely visual toggles), with the reason.

/// One thing a person can do and how an AI does the same.
pub struct Action {
    pub name: &'static str,
    /// Where in the window it is.
    pub place: &'static str,
    /// The labels of the controls that start it, as written in the window's source.
    pub labels: &'static [&'static str],
    /// The MCP tools that do it.
    pub tools: &'static [&'static str],
    /// Engine operations (run through `start_analysis`) that do it.
    pub operations: &'static [&'static str],
    /// Belongs to a workflow that is only in builds with the `experimental` feature.
    pub experimental: bool,
}

/// An action an AI is deliberately not offered.
pub struct Withheld {
    pub name: &'static str,
    pub labels: &'static [&'static str],
    pub reason: &'static str,
}

const fn act(
    name: &'static str,
    place: &'static str,
    labels: &'static [&'static str],
    tools: &'static [&'static str],
) -> Action {
    Action {
        name,
        place,
        labels,
        tools,
        operations: &[],
        experimental: false,
    }
}
const fn engine(
    name: &'static str,
    place: &'static str,
    labels: &'static [&'static str],
    tools: &'static [&'static str],
    operations: &'static [&'static str],
) -> Action {
    Action {
        name,
        place,
        labels,
        tools,
        operations,
        experimental: false,
    }
}
const fn hidden(
    name: &'static str,
    place: &'static str,
    labels: &'static [&'static str],
    tools: &'static [&'static str],
    operations: &'static [&'static str],
) -> Action {
    Action {
        name,
        place,
        labels,
        tools,
        operations,
        experimental: true,
    }
}

pub const ACTIONS: &[Action] = &[
    // ---- Data explorer: files
    act(
        "Open data files and dataset folders",
        "File menu, Add samples, drag and drop",
        &[
            "Open data…",
            "Open dataset folder…",
            "Add dataset folder…",
            "Add samples…",
            "Recent files",
        ],
        &["open_files", "list_mzml_files", "discover_datasets"],
    ),
    act(
        "Look at the open datasets and the state of the window",
        "Data files panel, status line",
        &[],
        &[
            "list_datasets",
            "gui_state",
            "dataset_metadata",
            "scan_metadata",
            "list_scans",
        ],
    ),
    act(
        "Read file and acquisition details",
        "Plot menu and trace settings",
        &["File information…", "Acquisition details…"],
        &["dataset_metadata", "scan_metadata", "list_scans"],
    ),
    act(
        "Choose which dataset is active",
        "Data files panel",
        &[],
        &["select_dataset"],
    ),
    act(
        "Show or hide a dataset",
        "Data files panel",
        &[],
        &["set_visibility"],
    ),
    // ---- Data explorer: traces
    act(
        "Choose the trace (TIC, base peak, XIC), its filters and smoothing",
        "Trace settings",
        &[
            "TIC",
            "Base Peak",
            "XIC",
            "Smoothing",
            "Set Range…",
            "Enable range filtering",
            "All acquisition types",
            "Precursor m/z",
            "MS level",
            "Apply XIC",
        ],
        &["extract_chromatogram", "chromatogram_data"],
    ),
    act(
        "Show a chromatogram stored in the file (SRM/MRM)",
        "Trace settings",
        &[],
        &["extract_chromatogram", "dataset_metadata"],
    ),
    act(
        "Return to an earlier trace, hide it, or delete it",
        "Trace list and plot menu",
        &[
            "Delete this trace",
            "Hide this trace",
            "Select trace",
            "Active trace",
        ],
        &[
            "select_trace",
            "set_trace_visibility",
            "remove_trace",
            "retain_trace",
        ],
    ),
    act(
        "Show the unsmoothed trace behind a smoothed one",
        "Trace settings",
        &["Overlay unsmoothed trace"],
        &["chromatogram_data", "extract_chromatogram"],
    ),
    // ---- Data explorer: spectra
    act(
        "Inspect the spectrum at a retention time or scan, and step through scans",
        "Chromatogram double-click, scan navigation",
        &[
            "Inspect spectrum here",
            "Inspect nearest acquired scan",
            "Next",
            "Previous",
            "Next page",
            "Previous page",
        ],
        &["spectrum", "list_scans", "scan_metadata"],
    ),
    act(
        "Clear the spectrum or the shared batch preset",
        "Spectrum panel, Trace settings",
        &[
            "Clear spectrum",
            "Clear batch preset",
            "Clear shared batch preset",
        ],
        &["clear_display"],
    ),
    // ---- Data explorer: integration
    act(
        "Integrate a peak and record its area",
        "Chromatogram, Trace settings",
        &["Integrate and record area", "Integrate visible RT range"],
        &["integrate", "undo_integration"],
    ),
    act(
        "Clear the integration markers",
        "Chromatogram, Trace settings",
        &["Clear integration"],
        &["clear_integration"],
    ),
    act(
        "See, remove and export the recorded measurements",
        "Integration results window",
        &[
            "Integration results…",
            "Results table…",
            "Remove selected measurement",
        ],
        &["measurements", "remove_measurement", "export_measurements"],
    ),
    // ---- Data explorer: presentation and figures
    act(
        "Draw the traces: colour, width, scale and plot properties",
        "Trace settings, Plot properties",
        &[
            "Line color",
            "Line width",
            "Maximum (a.u.)",
            "Plot properties…",
            "Use visible trace color",
        ],
        &["set_display"],
    ),
    act(
        "Zoom, pan and reset the view",
        "Plot toolbar and plot menu",
        &[
            "Zoom +",
            "Zoom −",
            "Pan left",
            "Pan right",
            "Reset view",
            "Reset zoom",
        ],
        &["set_view"],
    ),
    act(
        "Arrange the plots: overlay, grid, compare samples, panels, focus",
        "Toolbar and View menu",
        &[
            "Grid",
            "Overlay",
            "Automatic",
            "Grid layout…",
            "+ Row",
            "+ Column",
            "Compare samples",
            "Data files",
            "Trace settings",
            "Mass spectrum",
            "Focus",
            "Hide spectrum panel",
        ],
        &["set_layout"],
    ),
    act(
        "Export the trace or spectrum as CSV",
        "File > Export, plot menu",
        &[
            "Chromatogram CSV…",
            "Spectrum CSV…",
            "Export selected trace CSV…",
        ],
        &["export_csv"],
    ),
    act(
        "Export figures as SVG",
        "File > Export, plot toolbar",
        &[
            "Visible chromatograms…",
            "Active mass spectrum…",
            "Export chromatogram figure…",
            "Export full-resolution SVG…",
            "Figures (SVG)",
        ],
        &["export_figure"],
    ),
    act(
        "See the plots as the window draws them",
        "The chromatogram and spectrum panels",
        &[],
        &["chromatogram_image", "spectrum_image"],
    ),
    act(
        "Save and open the viewer workspace",
        "File menu",
        &["Save viewer session…", "Open viewer session…"],
        &["save_session", "load_session"],
    ),
    act(
        "Open, apply and save trace presets",
        "Viewer presets menu, preset editor",
        &[
            "Open preset…",
            "Load trace preset…",
            "Apply to viewer samples",
            "Append pasted analytes",
            "Save example preset…",
            "Recent presets",
            "Preset editor…",
            "Open…",
            "Save",
            "Save As…",
            "New",
            "Duplicate",
            "Delete",
            "Move up",
            "Move down",
            "Overlay analytes",
        ],
        &["list_presets", "get_preset", "save_preset", "apply_preset"],
    ),
    act(
        "Choose theme, interface size and m/z decimals",
        "View menu",
        &["Theme", "m/z decimals", "Interface size"],
        &["get_settings", "set_settings"],
    ),
    // ---- Quantification and QC: batch
    act(
        "Choose the samples and run the batch",
        "Quantification tab, Batch samples",
        &["Run batch", "Select all", "Select none", "Reprocess"],
        &["start_quantification", "quantification_status"],
    ),
    act(
        "Stop a running batch",
        "Quantification tab",
        &["Cancel targeted run"],
        &["cancel_quantification", "cancel_analysis"],
    ),
    act(
        "Edit, open and save the method",
        "Quantification tab, method window",
        &[
            "Edit method…",
            "Add analyte",
            "Remove",
            "Customise for this analyte",
            "Discard changes",
            "Save and continue",
            "OK",
        ],
        &[
            "start_quantification",
            "open_method",
            "save_method",
            "get_preset",
        ],
    ),
    act(
        "Save and open the batch",
        "Quantification tab",
        &[
            "Batch session ▾",
            "Open batch…",
            "Choose batch to open…",
            "Save batch…",
            "Save current batch…",
        ],
        &["save_batch", "load_batch"],
    ),
    act(
        "Review a peak: read its trace, correct the boundaries, restore the automatic peak",
        "Quantification tab, review stage",
        &[],
        &[
            "quantification_status",
            "quantification_trace",
            "review_quantification",
            "reset_quantification",
        ],
    ),
    act(
        "Export the batch results as CSV",
        "Quantification tab",
        &["Export CSV…"],
        &["export_quantification", "export_measurements"],
    ),
    // ---- Quantification and QC: calibration and concentrations (engine)
    engine(
        "Build a calibration and quantify unknowns",
        "Quantification tab, calibration stage",
        &[
            "Run targeted quantification",
            "Calculate concentrations",
            "Recalculate",
            "Add level",
            "Apply as new run",
            "Import levels (CSV)…",
            "Import sample sheet (CSV)…",
            "Open targeted request…",
            "Save targeted request…",
            "Copy these settings to all analytes",
            "External standard (peak area)",
            "Internal standard (measured, not quantified)",
            "Extrapolate end standards",
            "Fit range to standards",
            "Origin",
            "Weighting",
            "Use",
            "Reset",
        ],
        &[
            "start_analysis",
            "analysis_status",
            "analysis_result",
            "analysis_capabilities",
        ],
        &["targeted_batch", "quantify", "process_chromatograms"],
    ),
    engine(
        "Review a concentration and accept or reject it",
        "Quantification tab, calibration stage",
        &["Prepare selected concentration review"],
        &[
            "start_analysis",
            "prepare_review_proposal",
            "queue_review_proposal",
            "resolve_review_proposal",
        ],
        &["review_targeted"],
    ),
    engine(
        "Revise a chromatogram's peak boundaries with the advanced analysis",
        "Quantification tab, review stage",
        &[],
        &["start_analysis"],
        &["revise_chromatogram", "export_chromatographic_peaks"],
    ),
    act(
        "Save and open calibration runs and QC reports, export their tables",
        "Quantification tab, calibration and QC stages",
        &[
            "Open targeted history…",
            "Save targeted history…",
            "Open study / report…",
            "Save QC history JSON…",
            "Save QC evidence JSON…",
            "Export QC CSV…",
            "Export concentrations CSV…",
            "Export calibration CSV…",
            "Export ▾",
        ],
        &["export_results", "import_results"],
    ),
    engine(
        "Evaluate batch QC and method validation, and acknowledge QC rules",
        "Quantification tab, QC stage",
        &[
            "Evaluate batch QC",
            "Acknowledge review",
            "Reopen review",
            "Reset limits to defaults",
        ],
        &["start_analysis"],
        &[
            "evaluate_qc",
            "evaluate_targeted_qc",
            "review_qc",
            "export_qc",
            "validate_method",
        ],
    ),
    act(
        "Revisit a peak in the review stage",
        "Quantification tab, results browser",
        &["Revisit this peak in tab 1 ›"],
        &["show_workspace", "quantification_status"],
    ),
    // ---- Workflow, projects, audit package
    act(
        "Switch between the workspaces",
        "Analysis workflow menu, tabs",
        &[
            "Data explorer",
            "Quantification and QC",
            "AI activity",
        ],
        &["show_workspace"],
    ),
    act(
        "Open and refresh a project",
        "Project menu",
        &["Open project…", "Refresh project activity"],
        &["open_project", "refresh_project", "inspect_project"],
    ),
    act(
        "Create or save a project",
        "Project menu",
        &["Create project…", "Save project"],
        &["create_project", "save_project"],
    ),
    act(
        "Build the audit package and choose what goes in it",
        "Audit package tab",
        &[
            "Export audit package",
            "Change…",
            "Choose…",
            "SVG dimensions",
            "Portable bundle with verified raw sources",
        ],
        &[
            "build_bundle",
            "export_project_report",
            "prepare_analysis_report",
        ],
    ),
    act(
        "Check retained evidence against its sources and recompute a result",
        "Audit package tab",
        &[
            "Reopen project evidence / verify sources",
            "Reprocess and compare retained evidence",
        ],
        &[
            "reprocess_result",
            "verify_bundle",
            "inspect_project",
            "export_project_report",
        ],
    ),
    // ---- AI activity
    act(
        "Read the AI activity and the audit of an AI session",
        "AI activity workspace",
        &[],
        &["session_audit", "gui_state"],
    ),
    engine(
        "Prepare, load and decide on AI review proposals",
        "AI activity workspace",
        &[
            "Load MCP / saved review proposal…",
            "Prepare reversal",
            "Prepare revised suggestion",
            "Preview and retain suggestion",
            "Save proposal and decisions…",
            "Evidence references (one per line)",
            "Revision-bound review operation",
            "suggestion",
        ],
        &[
            "prepare_review_proposal",
            "queue_review_proposal",
            "resolve_review_proposal",
            "commit_analysis",
            "save_project",
        ],
        &["review_targeted", "revise_chromatogram"],
    ),
    act(
        "Run an analysis job and follow it",
        "Every engine-backed stage",
        &[],
        &[
            "start_analysis",
            "analysis_status",
            "analysis_result",
            "cancel_analysis",
        ],
    ),
    // ---- Fields of the forms (the inputs next to which a control acts)
    act(
        "Fill in the trace settings: target m/z, tolerance, acquisition, retention-time window, spectrum time",
        "Trace settings and the XIC and range windows",
        &[
            "Acquisition",
            "Target m/z",
            "Tolerance",
            "Mass tolerance:",
            "m/z:",
            "Min:",
            "- Max:",
            "Start (min)",
            "End (min)",
            "Spectrum at RT (min)",
            "Plot Type",
        ],
        &["extract_chromatogram", "integrate", "spectrum", "set_view"],
    ),
    act(
        "Fill in the method fields",
        "Quantification tab, method window",
        &[
            "Method name",
            "Detection smoothing",
            "Minimum peak prominence",
            "Boundary fraction",
        ],
        &["start_quantification", "save_method", "open_method"],
    ),
    act(
        "Fill in the preset editor fields",
        "Preset editor",
        &[
            "Columns",
            "Rows",
            "Range min",
            "Range max",
            "Target/product m/z",
        ],
        &["save_preset", "get_preset", "set_layout"],
    ),
    engine(
        "Fill in the calibration and QC fields",
        "Quantification tab, calibration and QC stages",
        &[
            "Acceptable peak area",
            "Calibration",
            "Fit",
            "Unit",
            "at most",
            "to",
        ],
        &["start_analysis"],
        &["targeted_batch", "evaluate_targeted_qc"],
    ),
    act(
        "Fill in the audit package fields",
        "Audit package tab",
        &["Include report sections", "Report title", "Save package to"],
        &["build_bundle"],
    ),
    act(
        "Run several window operations in one call",
        "not a window control: a person clicks them one after the other",
        &[],
        &["batch"],
    ),
    // ---- Experimental workflows (hidden in the default build)
    hidden(
        "Identification: inspect, process and compare spectra, search libraries",
        "Identification workspace",
        &[
            "Execute and retain engine result",
            "Import local MSP / MGF / MassBank library",
            "Inspect first original spectrum (profile/centroid)",
            "Load evidence history JSON",
            "Prepare MS1 isotope inspection",
            "Prepare acquisition inspection / average",
            "Prepare formula hypotheses",
            "Prepare library search",
            "Prepare linked precursor XIC",
            "Prepare metadata amendment / reprocessing",
            "Prepare reasoned annotation for selected candidate",
            "Prepare unknown or compound-class annotation",
            "Save full evidence history JSON (new file)",
        ],
        &[],
        &[
            "inspect_spectra",
            "process_spectra",
            "compare_spectra",
            "import_spectral_library",
            "search_spectral_library",
            "annotate_spectrum",
            "formula_candidates",
            "analyze_isotopes",
            "linked_spectral_chromatogram",
            "export_spectral_candidates",
        ],
    ),
    hidden(
        "Untargeted analysis: build and export a feature matrix",
        "Untargeted workspace",
        &[
            "Cancel processing",
            "Fill gaps by re-integration",
            "Load retained feature matrix JSON",
            "Process / resume verified checkpoints",
            "Profile spectra (pick peaks before tracing)",
            "Select checkpoint directory",
            "Select multi-sample mzML files",
            "Use project datasets",
            "Accepted width range (s)",
            "Associated MS/MS evidence: Missing",
            "Blank ratio (sample ÷ blank)",
            "Correspondence tolerance (ppm)",
            "Detection tolerance (ppm)",
            "Maximum QC variation (fraction)",
            "Minimum sample fraction",
            "Minimum scans",
            "Minimum trace length (s)",
            "Noise intensity",
            "Polarity",
            "Retention-time tolerance (s)",
            "Typical peak width (s)",
        ],
        &[
            "start_untargeted",
            "untargeted_status",
            "cancel_untargeted",
            "untargeted_features",
        ],
        &["untargeted_batch", "export_feature_matrix"],
    ),
    hidden(
        "Annotate features: propose, review and export annotations",
        "Untargeted workspace, annotation",
        &[
            "Compute tentative formula / isotope / library hypotheses",
            "Load annotation evidence JSON",
            "Prepare formula/isotope/library search parameters",
            "Prepare hypothesis for selected feature/sample",
            "Start annotation ledger from this matrix",
            "Use retained spectral library JSON for identification",
            "Validate and retain competing hypothesis",
        ],
        &[],
        &[
            "propose_feature_annotations",
            "add_feature_hypothesis",
            "review_feature_annotation",
            "export_feature_annotations",
        ],
    ),
    hidden(
        "Statistics: test groups and export tables",
        "Statistics workspace",
        &[
            "Analyze and retain new revision",
            "Continue →",
            "← Back",
            "Confidence level",
            "Explicit pseudocount (original feature units)",
            "Internal-standard feature ID",
            "Metadata column",
            "Cancel statistics",
            "Check Python environment",
            "Compare two independent groups",
            "Exclude sample",
            "Export statistical CSV",
            "Include again",
            "Load quantitative table / feature matrix / statistics response JSON",
            "Maximum missing fraction",
            "Restore this revision's settings and original table",
            "Save full reproducible JSON",
            "Use current feature matrix",
            "Use current targeted concentrations",
        ],
        &[],
        &["analyze_statistics", "export_statistics"],
    ),
];

/// Actions an AI deliberately cannot do.
pub const WITHHELD: &[Withheld] = &[
    Withheld {
        name: "Choose the msconvert converter program",
        labels: &[
            "Vendor import settings",
            "Locate msconvert…",
            "Detect msconvert again",
        ],
        reason: "Picking the program that converts vendor files means choosing which executable the application runs. That is the operator's decision (the CHROMASCOPE_MSCONVERT setting or the window's menu), never an AI's; open_files and list_mzml_files report whether a converter is configured.",
    },
];

/// Controls that start no action of their own.
pub const FURNITURE: &[(&str, &str)] = &[
    (
        "About Chromascope…",
        "opens the About window: information only",
    ),
    ("File", "menu title"),
    ("View", "menu title"),
    ("Export", "menu title"),
    ("Viewer presets", "menu title"),
    ("Plot options", "menu title"),
    ("More", "menu title"),
    (
        "Hide",
        "closes a side panel; set_layout shows or hides panels",
    ),
    ("Cancel", "closes a dialog without doing anything"),
    (
        "Confirm",
        "answers a dialog; the action it confirms has its own entry",
    ),
    ("Dismiss", "closes an error dialog"),
    (
        "Legend",
        "shows or hides the legend of one on-screen plot; images always draw it",
    ),
    (
        "Legend position",
        "moves the legend of one on-screen plot; images always draw it",
    ),
    (
        "Copy view bounds",
        "puts the axis bounds on the clipboard; gui_state returns them",
    ),
    (
        "Add entry",
        "editing step of a typed form: the request is given to start_analysis as JSON",
    ),
    (
        "Remove entry",
        "editing step of a typed form: the request is given to start_analysis as JSON",
    ),
    (
        "Add row from current template",
        "editing step of a typed form",
    ),
    ("Remove row", "editing step of a typed form"),
    ("Configure", "opens a typed form for an engine request"),
    (
        "Save level template…",
        "writes an empty CSV template for typing calibration levels in a spreadsheet",
    ),
    (
        "Save sheet template…",
        "writes an empty CSV template for typing a sample sheet in a spreadsheet",
    ),
];
