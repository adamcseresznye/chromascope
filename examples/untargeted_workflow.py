"""Prepare and run a local untargeted sample sheet with create-new evidence files.

Requires a built chromascope-cli and pyOpenMS 3.5.0 in the Python executable
selected by CHROMASCOPE_OPENMS_PYTHON (or PATH python). No data leave the machine.
"""
import argparse
import json
from pathlib import Path
import subprocess
import uuid


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("sample_sheet", help="JSON array: id, source, role (sample/blank/qc), metadata")
    parser.add_argument("output", help="New evidence directory")
    parser.add_argument("--cli", default="target/debug/chromascope-cli")
    parser.add_argument("--polarity", required=True, choices=["positive", "negative"])
    parser.add_argument("--parameters", help="Optional JSON object of explicit Config overrides")
    parser.add_argument("--gap-fill", action="store_true")
    args = parser.parse_args()
    samples = json.loads(Path(args.sample_sheet).read_text())
    for sample in samples:
        sample["source"] = str(Path(sample["source"]).resolve(strict=True))
    out = Path(args.output).resolve()
    out.mkdir(parents=True, exist_ok=False)
    config = json.loads(Path(args.parameters).read_text()) if args.parameters else {}
    config.update(samples=samples, polarity=args.polarity, cache_directory=str(out / "checkpoints"), gap_fill=args.gap_fill)
    request = {"version": 1, "operation_id": str(uuid.uuid4()), "actor": "local-untargeted-workflow",
               "operation": {"operation": "untargeted_batch", "config": config}}
    text = json.dumps(request, indent=2)
    (out / "request.json").write_text(text)
    process = subprocess.run([args.cli, "run", "-"], input=text, text=True, stdout=subprocess.PIPE)
    (out / "response.json").write_text(process.stdout)
    if process.returncode:
        raise SystemExit(process.returncode)
    for command, name in [("export-features", "matrix-all.csv"),
                          ("export-features-filtered", "matrix-included.csv"),
                          ("export-feature-observations", "observations.csv")]:
        subprocess.run([args.cli, command, str(out / name)], input=process.stdout, text=True, check=True)


if __name__ == "__main__":
    main()
