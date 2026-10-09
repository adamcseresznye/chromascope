"""Exercise operational release workflows through the real CLI, preserving evidence.

Run from any directory with --cli EXECUTABLE --output NEW_DIRECTORY.
The bundled targeted request has no standards: concentration must stay missing.
QC uses authored demonstration observations. Spectral search is explicitly a
MassBank record self-search, not an unknown identification or validation study.
For AI-assisted orchestration use mcp_targeted_workflow.py with your own design.
"""

import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    cli = args.cli.resolve(strict=True)
    inputs = [
        root / "examples/targeted-request.json",
        root / "examples/qc-request.json",
        root / "examples/spectral-source.json",
        root / "examples/spectral-search.json",
        root / "tests/reference/MSBNK-Antwerp_Univ-AN111301.txt",
        root / "test_file/data_dependent_02.mzML",
    ]
    hashes = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs}
    args.output.mkdir(exist_ok=False)

    def save(name, value):
        with (args.output / name).open("x", encoding="utf-8") as stream:
            json.dump(value, stream, indent=2, allow_nan=False)

    def load(path):
        return json.loads(path.read_text(encoding="utf-8-sig"))

    def run(name, operation):
        request = {
            "version": 1,
            "operation_id": str(uuid.uuid4()),
            "actor": "release example; demonstration actions only",
            "operation": operation,
        }
        save(name + "-request.json", request)
        proc = subprocess.run(
            [str(cli), "run", "-"],
            input=json.dumps(request, allow_nan=False).encode("utf-8"),
            capture_output=True,
            timeout=300,
        )
        (args.output / (name + "-stdout.json")).write_bytes(proc.stdout)
        (args.output / (name + "-stderr.txt")).write_bytes(proc.stderr)
        if proc.returncode:
            raise RuntimeError(f"{name} failed; inspect retained stdout/stderr")
        envelope = json.loads(proc.stdout)
        if not envelope["ok"]:
            raise RuntimeError(envelope)
        output = envelope["result"]["output"]
        for key, value in output.items():
            if key.endswith("csv") and isinstance(value, str):
                with (args.output / f"{name}-{key}.csv").open(
                    "x", encoding="utf-8", newline=""
                ) as stream:
                    stream.write(value)
        return output

    save("inputs.json", hashes)
    targeted = load(inputs[0])["operation"]
    targeted["batch"]["samples"][0]["source"] = str(inputs[-1])
    batch = run("targeted", targeted)["batch"]
    run("targeted-export", {"operation": "export_targeted", "batch": batch})
    qc_operation = load(inputs[1])["operation"]
    # Deliberately strict demonstration rule, not a recommended method threshold.
    # The engine computes precision from the authored variable QC observations.
    for configured in qc_operation["study"]["rules"]:
        if configured["metric"] == "precision":
            configured["upper"] = 0.0
    qc = run("qc", qc_operation)["report"]
    rule = next(d["rule"]["id"] for d in qc["decisions"] if d["status"] == "fail")
    reviewed = run(
        "qc-review",
        {
            "operation": "review_qc",
            "report": qc,
            "expected_revision": len(qc["reviews"]),
            "rule_id": rule,
            "reason": "Demonstration acknowledgement; failure remains unresolved",
            "acknowledged": True,
        },
    )["report"]
    run("qc-export", {"operation": "export_qc", "report": reviewed})
    library = run(
        "library",
        {
            "operation": "import_spectral_library",
            "text": inputs[4].read_text(encoding="utf-8"),
            "format": "massbank",
            "source": load(inputs[2]),
        },
    )["library"]
    spectrum = copy.deepcopy(library["entries"][0]["spectrum"])
    spectrum["metadata"]["origin"] = "Explicit retained MassBank self-search demo"
    processed = run(
        "spectrum",
        {
            "operation": "process_spectra",
            "spectra": [spectrum],
            "background": [],
            "config": {
                "tolerance": {"value": 0.01, "unit": "da"},
                "centroid_snr": None,
                "background_scale": 1.0,
                "relative_threshold": 0.0,
            },
        },
    )["processed"]
    search = run(
        "search",
        {
            "operation": "search_spectral_library",
            "query": processed,
            "library": library,
            "config": load(inputs[3]),
        },
    )["report"]
    run(
        "spectral-export", {"operation": "export_spectral_candidates", "report": search}
    )
    save(
        "summary.json",
        {
            "targeted_concentrations": [r["concentration"] for r in batch["results"]],
            "original_qc_status": qc["status"],
            "reviewed_qc_status": reviewed["status"],
            "qc_review_count": len(reviewed["reviews"]),
            "spectral_candidates": len(search["candidates"]),
            "spectral_scope": "record self-search; no unknown identity assigned",
        },
    )
    print(f"Evidence retained in {args.output.resolve()}")


if __name__ == "__main__":
    main()
