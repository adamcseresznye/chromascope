# Audit package and projects

A *project* keeps the history of your analysis. An *audit package* is a folder you can hand to someone else, who can then check that nothing was changed.

## Projects

Choose **Project** and then **Create project…** to start one from the current workspace, or **Open project…** to continue one. **Save project** stores the workspace and results as a new revision. Revisions are never overwritten: earlier ones stay available, each with the hashes of the source files it used. **Refresh project activity** reloads work that another session saved.

## Build the audit package

Go to the tab **Audit package**. Choose what goes in with **Include report sections**, give the package a **Report title**, choose where to save it with **Save package to**, then choose **Export audit package**. You get a folder with a readable HTML report, the evidence behind every number and a file of checksums. The option **Portable bundle with verified raw sources** also copies the source files, so the package can be checked elsewhere.

Open warnings (for example peaks that still need review) stop the export until you confirm them.

## Check and recompute

**Reopen project evidence / verify sources** checks that the source files still match their stored hashes. **Reprocess and compare retained evidence** recomputes a stored result from its verified source and compares it with what was saved. A result can only be recomputed with the program version that produced it.

## What the package is, and is not

The report is a review draft. It lists questionable results and the review history next to the good ones, and it states plainly that an AI's explanation is unverified. It is not an approval, a signature or a regulatory release.
