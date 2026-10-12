"""Regenerate independent scientific references in a new directory, preserving originals.

Requires the existing NumPy/SciPy/mpmath reference environment. Compares full
numeric JSON: structure, strings and integers must be identical and floats must
agree to a tight relative tolerance, because BLAS/LAPACK kernels differ in the last
bits between platforms. Only the chromatography generator's file location differs.
"""

import hashlib
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys


RELATIVE_TOLERANCE = 1e-9
ABSOLUTE_TOLERANCE = 1e-12
# The targeted oracle fits degree-3 polynomials over y up to ~1.6e4 and reports residuals
# of order 1e-3, so last-bit differences between CPU kernels reach ~3e-12 absolute
# (measured by forcing OPENBLAS_CORETYPE). Other references are stable across kernels.
ABSOLUTE_TOLERANCES = {"targeted": 1e-8}


def first_difference(actual, expected, path="", absolute=ABSOLUTE_TOLERANCE):
    """Return a description of the first mismatch, or None when the values agree."""
    if isinstance(actual, bool) or isinstance(expected, bool):
        return None if actual is expected else f"{path}: {actual!r} != {expected!r}"
    if isinstance(actual, float) or isinstance(expected, float):
        if isinstance(actual, (int, float)) and isinstance(expected, (int, float)):
            if math.isclose(
                actual, expected, rel_tol=RELATIVE_TOLERANCE, abs_tol=absolute
            ):
                return None
        return f"{path}: {actual!r} != {expected!r}"
    if isinstance(actual, dict) and isinstance(expected, dict):
        if actual.keys() != expected.keys():
            return f"{path}: keys {sorted(actual)} != {sorted(expected)}"
        for key in actual:
            found = first_difference(
                actual[key], expected[key], f"{path}/{key}", absolute
            )
            if found:
                return found
        return None
    if isinstance(actual, list) and isinstance(expected, list):
        if len(actual) != len(expected):
            return f"{path}: length {len(actual)} != {len(expected)}"
        for index, (left, right) in enumerate(zip(actual, expected)):
            found = first_difference(left, right, f"{path}/{index}", absolute)
            if found:
                return found
        return None
    return None if actual == expected else f"{path}: {actual!r} != {expected!r}"


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
        difference = first_difference(
            actual,
            expected,
            absolute=ABSOLUTE_TOLERANCES.get(name, ABSOLUTE_TOLERANCE),
        )
        if difference:
            raise AssertionError(
                f"Independent reference mismatch: {name} at {difference}"
            )
        results.append(
            {
                "reference": name,
                "numeric_json_equal": True,
                "float_relative_tolerance": RELATIVE_TOLERANCE,
                "float_absolute_tolerance": ABSOLUTE_TOLERANCES.get(
                    name, ABSOLUTE_TOLERANCE
                ),
                "generator_sha256": hashlib.sha256(
                    (source / f"generate_{name}.py").read_bytes()
                ).hexdigest(),
                "retained_reference_sha256": hashlib.sha256(
                    (source / f"{name}.json").read_bytes()
                ).hexdigest(),
            }
        )
    (root / "results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
    print(
        "All five independent references reproduce (floats within the documented "
        "tolerances); originals preserved."
    )


if __name__ == "__main__":
    try:
        main()
    except AssertionError as error:
        # GitHub shows this as an annotation, which keeps the cause visible in the run summary.
        print(f"::error title=Reference mismatch::{error}")
        raise
