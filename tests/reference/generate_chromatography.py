"""Regenerate independent fixtures with NumPy/SciPy, no Chromascope imports.
Synthetic, CC0 test data. Run: python tests/reference/generate_chromatography.py.
AsLS uses SciPy sparse D2 penalty and spsolve, independently of Rust band solve.
"""
import json
from pathlib import Path
import numpy as np
import scipy
from scipy.signal import savgol_filter, find_peaks
from scipy.sparse import diags
from scipy.sparse.linalg import spsolve

x = np.linspace(0, 8, 161)
rng = np.random.default_rng(143)
y = 4 + .6*x + .025*x*x + 80*np.exp(-.5*((x-3)/.3)**2) + rng.normal(0,.1,len(x))
smoothed = savgol_filter(y, 7, 2, mode='interp')
d = diags([np.ones(len(x)-2), -2*np.ones(len(x)-2), np.ones(len(x)-2)], [0,1,2], shape=(len(x)-2,len(x))).tocsc()
w = np.ones(len(x))
for _ in range(10):
    z = spsolve(diags(w).tocsc() + 1e5*(d.T @ d), w*smoothed)
    w = np.where(smoothed > z, .01, .99)
# Independent discrete apex, interpolated interval endpoints and trapezoids.
lo, hi = 2.1, 3.9
xx = np.concatenate(([lo], x[(x>lo)&(x<hi)], [hi]))
yy = np.interp(xx,x,y-z)
apex_indices, _ = find_peaks(smoothed-z)
apex = max(apex_indices, key=lambda i: (smoothed-z)[i])
# Irregular-grid local least-squares reference uses NumPy lstsq.
irregular = x + .006*np.sin(np.arange(len(x)))
irregular[0] = 0
yi = 3 + .4*irregular + 20*np.exp(-.5*((irregular-3)/.5)**2)
si=[]
for i in range(len(x)):
    lo_i = min(max(i-3,0),len(x)-7)
    xx_i = irregular[lo_i:lo_i+7] - irregular[i]
    si.append(np.linalg.lstsq(np.vander(xx_i,3,increasing=True),yi[lo_i:lo_i+7],rcond=None)[0][0])
result = dict(provenance=dict(generator=__file__, numpy=np.__version__, scipy=scipy.__version__, seed=143, license='CC0', sg='scipy.signal.savgol_filter(window=7,degree=2,mode=interp)', baseline='10 AsLS solves: scipy.sparse.linalg.spsolve, lambda=1e5,p=.01'), x=x.tolist(), raw=y.tolist(), smoothed=smoothed.tolist(), baseline=z.tolist(), interval=[lo,hi], area=float(np.trapezoid(yy,xx)), apex=float(x[apex]), irregular=irregular.tolist(), irregular_raw=yi.tolist(), irregular_smoothed=si)
Path(__file__).with_name('chromatography.json').write_text(json.dumps(result, indent=2)+'\n')
