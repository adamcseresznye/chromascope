"""Create-new reproducible analysis bundle from a sample-major quantitative CSV.

Usage: python examples/statistics_workflow.py table.csv metadata.csv settings.json NEW_DIR --unit ng/mL
table.csv: sample_id,feature_a,feature_b,...; empty values are missing.
metadata.csv: sample_id,group,role,...; sample IDs must match exactly.
settings.json uses the shared Rust statistics Settings schema. No algorithms here.
"""

import argparse
import csv
import hashlib
import json
import os
from pathlib import Path
import subprocess
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("table", "metadata", "settings", "output"):
        parser.add_argument(name, type=Path)
    parser.add_argument("--unit", required=True)
    parser.add_argument(
        "--cli",
        default=(
            "target/debug/chromascope-cli.exe"
            if os.name == "nt"
            else "target/debug/chromascope-cli"
        ),
    )
    args = parser.parse_args()

    def records(path):
        with path.open(newline="", encoding="utf-8-sig") as stream:
            reader = csv.DictReader(stream)
            if (
                not reader.fieldnames
                or reader.fieldnames[0] != "sample_id"
                or len(set(reader.fieldnames)) != len(reader.fieldnames)
            ):
                raise ValueError("CSV requires unique headers beginning with sample_id")
            rows = list(reader)
            if any(None in row for row in rows):
                raise ValueError("CSV row has unexpected columns")
            return reader.fieldnames, rows

    headers, rows = records(args.table)
    _, metadata = records(args.metadata)
    ids = [row["sample_id"] for row in rows]
    meta_ids = [row["sample_id"] for row in metadata]
    if (
        len(set(ids)) != len(ids)
        or len(set(meta_ids)) != len(meta_ids)
        or set(ids) != set(meta_ids)
    ):
        raise ValueError("Unique table/metadata sample IDs must match exactly")
    meta = {
        row["sample_id"]: {k: v for k, v in row.items() if k != "sample_id"}
        for row in metadata
    }
    table = {
        "samples": [{"id": sid, "metadata": meta[sid]} for sid in ids],
        "features": [{"id": name, "unit": args.unit} for name in headers[1:]],
        "values": [
            [None if row[name] == "" else float(row[name]) for name in headers[1:]]
            for row in rows
        ],
        "provenance": {
            "input_sha256": {
                str(path.resolve()): hashlib.sha256(path.read_bytes()).hexdigest()
                for path in (args.table, args.metadata)
            },
            "quantity_unit": args.unit,
            "missing_encoding": "empty CSV cell",
        },
    }
    request = {
        "version": 1,
        "operation_id": str(uuid.uuid4()),
        "actor": "statistics CSV workflow",
        "operation": {
            "operation": "analyze_statistics",
            "table": table,
            "settings": json.loads(args.settings.read_text()),
        },
    }
    serialized = json.dumps(request, indent=2, allow_nan=False)
    args.output.mkdir(parents=False, exist_ok=False)
    (args.output / "request.json").write_text(serialized, encoding="utf-8")
    run = subprocess.run(
        [args.cli, "run", "-"],
        input=serialized.encode(),
        capture_output=True,
        check=False,
    )
    (args.output / "response.json").write_bytes(run.stdout)
    (args.output / "stderr.txt").write_bytes(run.stderr)
    if run.returncode:
        raise RuntimeError("Analysis failed; inspect retained response/stderr")
    export = subprocess.run(
        [args.cli, "export-statistics", str(args.output / "statistics.csv")],
        input=run.stdout,
        capture_output=True,
        check=False,
    )
    (args.output / "export.json").write_bytes(export.stdout)
    (args.output / "export-stderr.txt").write_bytes(export.stderr)
    if export.returncode:
        raise RuntimeError("Export failed; inspect retained export/stderr")
    print(args.output.resolve())


if __name__ == "__main__":
    main()
