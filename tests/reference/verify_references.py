"""Regenerate independent scientific references in a new directory, preserving originals.

Requires the existing NumPy/SciPy/mpmath reference environment. Compares full
numeric JSON exactly; only the chromatography generator's file location differs.
"""

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys


def main():
    root = Path(sys.argv[1]).resolve()
    root.mkdir(parents=True, exist_ok=False)
    (root / "examples").mkdir()
    source = Path(__file__).resolve().parent
    for path in source.iterdir():
        if path.suffix in (".py", ".txt"):
            shutil.copy2(path, root / path.name)
    results = []
    for name in ("targeted", "qc", "spectral", "statistics", "chromatography"):
        proc = subprocess.run(
            [sys.executable, str(root / f"generate_{name}.py")],
            cwd=root,
            capture_output=True,
            text=True,
        )
        (root / f"{name}.log").write_text(proc.stdout + proc.stderr, encoding="utf-8")
        if proc.returncode:
            raise RuntimeError(f"{name}: {proc.stderr}")
        actual = json.loads((root / f"{name}.json").read_text(encoding="utf-8"))
        expected = json.loads((source / f"{name}.json").read_text(encoding="utf-8"))
        if name == "chromatography":
            actual["provenance"].pop("generator")
            expected["provenance"].pop("generator")
        if actual != expected:
            raise AssertionError(f"Independent reference mismatch: {name}")
        results.append(
            {
                "reference": name,
                "numeric_json_equal": True,
                "generator_sha256": hashlib.sha256(
                    (source / f"generate_{name}.py").read_bytes()
                ).hexdigest(),
                "retained_reference_sha256": hashlib.sha256(
                    (source / f"{name}.json").read_bytes()
                ).hexdigest(),
            }
        )
    (root / "results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
    print("All five independent references reproduce exactly; originals preserved.")


if __name__ == "__main__":
    main()
