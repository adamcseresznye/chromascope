"""Independent calibration oracle: NumPy LAPACK lstsq in unscaled powers and polynomial roots.
No Rust implementation is imported. Run python tests/reference/generate_targeted.py.
"""
from pathlib import Path
import json
import numpy as np
x = np.array([0.5, 1., 2., 4., 8., 16., 32.])
noise = np.array([0.02, -0.05, 0.08, -0.12, 0.17, -0.2, 0.11])
cases = []
for degree in [1, 2, 3]:
    for intercept in [dict(kind='free'), dict(kind='zero'), dict(kind='fixed',value=1.25)]:
        for weighting in ['unweighted', 'inverse_x', 'inverse_x2']:
            y = 2 + 3*x + (0.03*x*x if degree>=2 else 0) + (0.0005*x**3 if degree>=3 else 0) + noise
            free = intercept['kind']=='free'
            offset = intercept.get('value',0)
            powers = np.arange(0 if free else 1, degree+1)
            w = np.ones_like(x) if weighting=='unweighted' else x**(-1 if weighting=='inverse_x' else -2)
            a = x[:,None]**powers
            fitted_coefficients, *_ = np.linalg.lstsq(a*np.sqrt(w[:,None]), (y-offset)*np.sqrt(w), rcond=None)
            coefficients = fitted_coefficients if free else np.r_[offset,fitted_coefficients]
            fitted = np.polynomial.polynomial.polyval(x,coefficients)
            residual = y-fitted
            query = float(np.polynomial.polynomial.polyval(7.3,coefficients))
            roots = np.polynomial.polynomial.polyroots(coefficients-np.r_[query,np.zeros(degree)])
            roots = [float(z.real) for z in roots if abs(z.imag)<1e-9 and .5<=z.real<=32]
            back = []
            for observation in y:
                inverse_roots = np.polynomial.polynomial.polyroots(coefficients-np.r_[observation,np.zeros(degree)])
                inverse_roots = [float(z.real) for z in inverse_roots if abs(z.imag)<1e-9 and .5<=z.real<=32]
                back.append(inverse_roots[0] if len(inverse_roots)==1 else None)
            cases.append(dict(back_calculated=back, accuracy_percent=[100*v/nominal if v is not None else None for nominal,v in zip(x,back)], config=dict(degree=degree,intercept=intercept,weighting=weighting,unit='ng_ml',range=[.5,32.],lod=.5,loq=1.,accuracy_tolerance_percent=15.,qc_cv_limit_percent=15.,blank_response_limit=0.),x=x.tolist(),y=y.tolist(),coefficients=coefficients.tolist(),residuals=residual.tolist(),r_squared=float(1-np.sum(residual**2)/np.sum((y-y.mean())**2)),weighted_rmse=float(np.sqrt(np.sum(w*residual**2)/w.sum())),residual_standard_error=float(np.sqrt(np.sum(w*residual**2)/(len(x)-len(powers)))),query=query,roots=roots))
Path(__file__).with_name('targeted.json').write_text(json.dumps(dict(numpy=np.__version__,algorithm='numpy.linalg.lstsq unscaled powers; numpy.polynomial.polynomial.polyroots',cases=cases),indent=2)+'\n',encoding='utf-8')
