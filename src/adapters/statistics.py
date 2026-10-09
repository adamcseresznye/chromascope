"""Local statistics-v1 adapter. No file/network inputs or arbitrary expressions.

NumPy owns transforms/SVD; SciPy owns linkage, Welch inference and BH/BY.
Rows are samples, columns are features. All operations retain their parameters.
"""

import json
import sys
import warnings

try:
    import numpy as np
    import scipy
    from scipy import stats
    from scipy.cluster import hierarchy
except ImportError as error:
    json.dump(
        {
            "error": "Local scientific runtime unavailable: " + str(error),
            "code": "unsupported_capability",
        },
        sys.stdout,
    )
    sys.exit(1)


def analyze(request):
    if not ((1, 26) <= tuple(map(int, np.__version__.split(".")[:2])) < (3, 0)) or not (
        (1, 11) <= tuple(map(int, scipy.__version__.split(".")[:2])) < (2, 0)
    ):
        raise ValueError("statistics-v1 requires NumPy >=1.26,<3 and SciPy >=1.11,<2")
    table, cfg = request["table"], request["settings"]
    selected = []
    excluded = []
    for i, sample in enumerate(table["samples"]):
        reason = cfg["excluded_samples"].get(sample["id"])
        if reason is None:
            for key, value in cfg["metadata_equals"].items():
                if sample["metadata"].get(key) != value:
                    reason = "metadata filter: " + key
                    break
        (selected if reason is None else excluded).append(
            i if reason is None else {"sample_id": sample["id"], "reason": reason}
        )
    if len(selected) < 2:
        raise ValueError("At least two included samples are required")
    raw = np.array(
        [[np.nan if v is None else v for v in table["values"][i]] for i in selected],
        dtype=float,
    )
    missing_fraction = np.mean(np.isnan(raw), axis=0)
    keep = []
    removed = []
    for j, feature in enumerate(table["features"]):
        reason = cfg["excluded_features"].get(feature["id"])
        if reason is None and (
            missing_fraction[j] > cfg["max_missing_fraction"]
            or np.all(np.isnan(raw[:, j]))
        ):
            reason = "missingness threshold or all missing"
        if (
            reason is None
            and cfg["missing"] == "complete_features"
            and np.any(np.isnan(raw[:, j]))
        ):
            reason = "complete-feature policy"
        (keep if reason is None else removed).append(
            j if reason is None else {"feature_id": feature["id"], "reason": reason}
        )
    if not keep:
        raise ValueError("No features survive the declared exclusions")
    x = raw[:, keep].copy()
    imputed = []
    for j, original in enumerate(keep):
        mask = np.isnan(x[:, j])
        if not mask.any():
            continue
        if cfg["missing"] in ("reject", "complete_features"):
            raise ValueError(
                "Missing values require an explicit imputation or exclusion policy"
            )
        observed = x[~mask, j]
        if cfg["missing"] == "median":
            value = float(np.median(observed))
        else:
            positive = observed[observed > 0]
            if not len(positive):
                raise ValueError(
                    "Half-minimum imputation requires positive observed values"
                )
            value = float(np.min(positive) / 2)
        for i in np.flatnonzero(mask):
            imputed.append(
                {
                    "sample_id": table["samples"][selected[i]]["id"],
                    "feature_id": table["features"][original]["id"],
                    "value": value,
                }
            )
        x[mask, j] = value
    factors = np.ones(len(selected))
    normalization = cfg["normalization"]
    if normalization != "none":
        if normalization == "total":
            base = np.sum(x, axis=1)
        elif normalization == "median":
            base = np.median(x, axis=1)
        else:
            names = [table["features"][j]["id"] for j in keep]
            if cfg["internal_standard"] not in names:
                raise ValueError("Internal standard must survive feature filters")
            base = x[:, names.index(cfg["internal_standard"])]
        if np.any(base <= 0):
            raise ValueError("Normalization denominators must be positive")
        factors = base / np.median(base)
        x = x / factors[:, None]
    normalized = x.copy()
    transform = cfg["transform"]
    if transform != "none":
        x = x + cfg["pseudocount"]
        if transform in ("log2", "log10"):
            if np.any(x <= 0):
                raise ValueError(
                    "Log requires positive normalized value + explicit pseudocount"
                )
            x = np.log2(x) if transform == "log2" else np.log10(x)
        else:
            x = np.sqrt(x)
    inference = x.copy()
    center = np.mean(x, axis=0) if cfg["scaling"] != "none" else np.zeros(len(keep))
    scale = np.ones(len(keep))
    if cfg["scaling"] in ("autoscale", "pareto"):
        scale = np.std(x, axis=0, ddof=1)
        scale[scale == 0] = 1
        if cfg["scaling"] == "pareto":
            scale = np.sqrt(scale)
    x = (x - center) / scale
    if not np.all(np.isfinite(x)):
        raise ValueError("Preprocessing overflow produced nonfinite values")
    # PCA always centers, independent of display/scaling choice.
    centered = x - np.mean(x, axis=0)
    u, singular, vt = np.linalg.svd(centered, full_matrices=False)
    for k in range(len(singular)):
        pivot = np.argmax(np.abs(vt[k]))
        if vt[k, pivot] < 0:
            vt[k] *= -1
            u[:, k] *= -1
    variance = singular**2 / (len(selected) - 1)
    total = float(np.sum(variance))
    count = min(cfg["components"], len(singular))
    pca = {
        "scores": (u[:, :count] * singular[:count]).tolist(),
        "loadings": vt[:count].T.tolist(),
        "variance": variance[:count].tolist(),
        "variance_ratio": (
            variance[:count] / total if total > 0 else np.zeros(count)
        ).tolist(),
        "state": "present" if total > 0 else "constant",
        "center": np.mean(x, axis=0).tolist(),
    }
    sample_tree = hierarchy.linkage(
        x, method=cfg["linkage"], metric="euclidean", optimal_ordering=True
    )
    feature_tree = (
        hierarchy.linkage(
            x.T, method=cfg["linkage"], metric="euclidean", optimal_ordering=True
        )
        if len(keep) > 1
        else np.empty((0, 4))
    )
    comparisons = []
    group_membership = None
    if cfg["groups"] is not None:
        groups = cfg["groups"]
        labels = [
            table["samples"][i]["metadata"].get(groups["metadata_key"])
            for i in selected
        ]
        aidx = [i for i, label in enumerate(labels) if label == groups["reference"]]
        bidx = [i for i, label in enumerate(labels) if label == groups["comparison"]]
        group_membership = {
            "reference": [table["samples"][selected[i]]["id"] for i in aidx],
            "comparison": [table["samples"][selected[i]]["id"] for i in bidx],
            "unassigned": [
                table["samples"][selected[i]]["id"]
                for i in range(len(selected))
                if i not in aidx and i not in bidx
            ],
        }
        for j, original in enumerate(keep):
            a, b = inference[aidx, j], inference[bidx, j]
            row = {
                "feature_id": table["features"][original]["id"],
                "n_reference": len(a),
                "n_comparison": len(b),
                "state": "insufficient",
                "mean_reference": float(np.mean(a)) if len(a) else None,
                "mean_comparison": float(np.mean(b)) if len(b) else None,
                "difference": None,
                "ci_low": None,
                "ci_high": None,
                "cohen_d": None,
                "log2_fold_change": None,
                "t": None,
                "df": None,
                "p": None,
                "q": None,
            }
            # Fold change uses normalized, untransformed arithmetic means.
            if len(a) and len(b):
                ma, mb = np.mean(normalized[aidx, j]), np.mean(normalized[bidx, j])
                if ma > 0 and mb > 0:
                    row["log2_fold_change"] = float(np.log2(mb / ma))
                row["difference"] = float(np.mean(b) - np.mean(a))
            if len(a) >= 2 and len(b) >= 2:
                va, vb = np.var(a, ddof=1), np.var(b, ddof=1)
                if va + vb == 0:
                    row["state"] = "constant"
                else:
                    with warnings.catch_warnings(record=True) as notices:
                        warnings.simplefilter("always", RuntimeWarning)
                        test = stats.ttest_ind(b, a, equal_var=False)
                    row["warnings"] = [str(w.message) for w in notices]
                    ci = test.confidence_interval(confidence_level=cfg["confidence"])
                    pooled = np.sqrt(
                        ((len(a) - 1) * va + (len(b) - 1) * vb) / (len(a) + len(b) - 2)
                    )
                    row.update(
                        state="present",
                        t=float(test.statistic),
                        df=float(test.df),
                        p=float(test.pvalue),
                        ci_low=float(ci.low),
                        ci_high=float(ci.high),
                        cohen_d=float(row["difference"] / pooled),
                    )
            comparisons.append(row)
        tested = [r for r in comparisons if r["p"] is not None]
        if tested:
            qs = stats.false_discovery_control(
                [r["p"] for r in tested], method=cfg["fdr"]
            )
            for row, q in zip(tested, qs):
                row["q"] = float(q)
    return {
        "python_version": sys.version.split()[0],
        "group_membership": group_membership,
        "adapter_version": "statistics-v1",
        "numpy_version": np.__version__,
        "scipy_version": scipy.__version__,
        "sample_indices": selected,
        "feature_indices": keep,
        "excluded_samples": excluded,
        "excluded_features": removed,
        "imputed": imputed,
        "normalization_factors": factors.tolist(),
        "center": center.tolist(),
        "scale": scale.tolist(),
        "processed": x.tolist(),
        "pca": pca,
        "sample_linkage": sample_tree.tolist(),
        "feature_linkage": feature_tree.tolist(),
        "sample_order": hierarchy.leaves_list(sample_tree).tolist(),
        "feature_order": (
            hierarchy.leaves_list(feature_tree).tolist() if len(keep) > 1 else [0]
        ),
        "comparisons": comparisons,
        "inference_scale": transform,
        "fdr_family_size": sum(r["p"] is not None for r in comparisons),
    }


if __name__ == "__main__":
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("error", RuntimeWarning)
            result = analyze(json.load(sys.stdin))
        json.dump({"result": result}, sys.stdout, allow_nan=False)
    except Exception as error:
        json.dump({"error": str(error)}, sys.stdout)
        sys.exit(1)
