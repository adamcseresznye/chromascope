"""Independent high-precision references: mpmath beta integral, covariance eigensystem.

Does not import NumPy/SciPy or the production adapter. Run with mpmath 1.3.0.
"""

import json
from pathlib import Path
import mpmath as m

m.mp.dps = 60


def welch(a, b):
    a, b = list(map(m.mpf, a)), list(map(m.mpf, b))
    mean = lambda x: sum(x) / len(x)
    var = lambda x: sum((v - mean(x)) ** 2 for v in x) / (len(x) - 1)
    va, vb = var(a) / len(a), var(b) / len(b)
    df = (va + vb) ** 2 / (va**2 / (len(a) - 1) + vb**2 / (len(b) - 1))
    se, difference = m.sqrt(va + vb), mean(b) - mean(a)
    t = difference / se
    survival = lambda t: m.betainc(
        df / 2, m.mpf("0.5"), 0, df / (df + t * t), regularized=True
    )
    lo, hi = m.mpf(0), m.mpf(100)
    for _ in range(250):
        mid = (lo + hi) / 2
        if survival(mid) > m.mpf("0.05"):
            lo = mid
        else:
            hi = mid
    critical = (lo + hi) / 2
    pooled = m.sqrt(
        ((len(a) - 1) * var(a) + (len(b) - 1) * var(b)) / (len(a) + len(b) - 2)
    )
    return {
        k: float(v)
        for k, v in dict(
            difference=difference,
            t=t,
            df=df,
            p=survival(abs(t)),
            ci_low=difference - critical * se,
            ci_high=difference + critical * se,
            cohen_d=difference / pooled,
            log2_fold_change=m.log(mean(b) / mean(a), 2),
        ).items()
    }


rows = [[1, 2, 5], [2, 4, 5], [3, 6, 5], [4, 7, 5], [6, 9, 5], [8, 11, 5]]
references = [
    welch([r[j] for r in rows[:3]], [r[j] for r in rows[3:]]) for j in range(2)
]
ps = [r["p"] for r in references]
order = sorted(range(len(ps)), key=ps.__getitem__)
previous = m.mpf(1)
for rank in range(len(ps), 0, -1):
    i = order[rank - 1]
    previous = min(previous, m.mpf(str(ps[i])) * len(ps) / rank)
    references[i]["q"] = float(previous)
center = [sum(m.mpf(r[j]) for r in rows) / len(rows) for j in range(3)]
cov = m.matrix(
    [
        [
            sum((m.mpf(r[j]) - center[j]) * (m.mpf(r[k]) - center[k]) for r in rows)
            / (len(rows) - 1)
            for k in range(3)
        ]
        for j in range(3)
    ]
)
eigen, vectors = m.eigsy(cov)
variances = list(reversed([float(v) for v in eigen]))
# Exact known collinear example: (1,2),(2,4),(3,6): covariance [[1,2],[2,4]].
out = {
    "generator": "mpmath 1.3.0; 60 decimal digits; incomplete beta and covariance eigensystem",
    "rows": rows,
    "welch": references,
    "pca_variance": variances,
    "collinear_variance": [5.0, 0.0],
    "collinear_scores": [-(5**0.5), 0.0, 5**0.5],
    "bh_example": {"p": [0.01, 0.04, 0.03, 0.002], "q": [0.02, 0.04, 0.04, 0.008]},
    "average_linkage_1d": [[0, 1, 1, 2], [2, 3, 3.5, 3]],
}
out["one_constant_group"] = welch([20, 2], [8, 8, 8])
Path(__file__).with_name("statistics.json").write_text(json.dumps(out, indent=2) + "\n")
