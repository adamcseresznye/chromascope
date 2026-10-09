"""Local shared-engine annotation; no remote submission or package installation."""
import argparse
import json
from pathlib import Path
import subprocess
import uuid


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("matrix", type=Path)
    parser.add_argument("output", type=Path, help="new directory")
    parser.add_argument("--feature-id", required=True)
    parser.add_argument("--sample-id", required=True)
    parser.add_argument("--adduct", action="append", required=True)
    parser.add_argument("--atom-bounds", required=True, help='JSON, e.g. {"C":60,"H":120,"O":20}')
    parser.add_argument("--ppm", type=float, default=5.0)
    parser.add_argument("--library", type=Path, help="retained imported library response JSON")
    parser.add_argument("--ms2-index", type=int, default=0)
    parser.add_argument("--allow-missing-metadata", action="store_true")
    parser.add_argument("--cli", default=str(Path(__file__).resolve().parents[1] / "target/debug/chromascope-cli"))
    args = parser.parse_args()
    saved = json.loads(args.matrix.read_text(encoding="utf-8-sig"))
    response = saved.get("result", saved)
    if response["output"]["kind"] != "feature_matrix":
        parser.error("Input must be a retained feature matrix engine response")
    matrix = response["output"]["report"]
    feature = next(f for f in matrix["features"] if f["id"] == args.feature_id)
    cell = next(c for c in feature["cells"] if c["sample_id"] == args.sample_id)
    library = None
    if args.library:
        saved = json.loads(args.library.read_text(encoding="utf-8-sig"))
        library = saved.get("result", saved)["output"]["library"]
    config = {
        "feature_id": args.feature_id, "sample_id": args.sample_id,
        "formula": {"observed_mz": cell["mz"], "polarity": matrix["config"]["polarity"],
                    "adducts": args.adduct, "tolerance": {"value": args.ppm, "unit": "ppm"},
                    "maximum_atoms": json.loads(args.atom_bounds), "maximum_candidates": 100},
        "library": library, "ms2_index": args.ms2_index,
        "search": {"precursor_tolerance": {"value": args.ppm, "unit": "ppm"},
                   "fragment_tolerance": {"value": 0.02, "unit": "da"},
                   "minimum_matches": 3, "minimum_cosine": 0.7, "maximum_candidates": 20,
                   "allow_missing_metadata": args.allow_missing_metadata,
                   "require_same_instrument": not args.allow_missing_metadata, "energy_tolerance": 5.0},
    }
    request = {"version": 1, "operation_id": str(uuid.uuid4()), "actor": "local-annotation-helper",
               "operation": {"operation": "propose_feature_annotations", "expected_revision": 0,
                             "config": config, "ledger": {"version": 1, "matrix": matrix,
                             "hypotheses": [], "reviews": [], "adduct_relationships": []}}}
    args.output.mkdir(parents=True, exist_ok=False)
    (args.output / "request.json").write_text(json.dumps(request, indent=2), encoding="utf-8")
    result = subprocess.run([args.cli, "run", "-"], input=json.dumps(request), text=True, capture_output=True, check=False)
    (args.output / "response.json").write_text(result.stdout, encoding="utf-8")
    (args.output / "stderr.txt").write_text(result.stderr, encoding="utf-8")
    if result.returncode:
        raise SystemExit(f"Engine failed; retained request/response/logs in {args.output}")
    subprocess.run([args.cli, "export-annotations", str(args.output / "annotations.csv")],
                   input=result.stdout, text=True, check=True)
    saved = json.loads(result.stdout)
    ledger = saved.get("result", saved)["output"]["ledger"]
    (args.output / "ledger.json").write_text(json.dumps(ledger, indent=2), encoding="utf-8")
    print(f"Retained {len(ledger['hypotheses'])} tentative hypotheses; no confirmed identities")


if __name__ == "__main__":
    main()
