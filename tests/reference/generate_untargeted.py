"""Generate a legal synthetic multi-sample reference; no expected output is fed to algorithms.

Analytic truth is Gaussian area and declared affine RT drift, independent of
OpenMS feature detection. mzML serialization uses pyOpenMS; no raw user data.
"""
import json
import math
from pathlib import Path
import sys
import uuid
import pyopenms as ms
import numpy as np


def generate(root, negative=False):
    root = Path(root).resolve()
    root.mkdir(parents=True, exist_ok=False)
    samples = []
    truth = []
    masses = [181.070664, 251.12345, 301.14159, 351.1618, 401.182, 451.203, 501.224, 551.245]
    rts = [50., 95., 140., 185., 230., 275., 320., 365.]
    for i, role in enumerate(["sample", "sample", "blank", "qc", "qc", "qc"]):
        path = root / f"sample-{i}.mzML"
        exp = ms.MSExperiment()
        shift = [0., 4., -2., 1., 3., -1.][i]
        scale = 1. + i * 0.0005
        amplitudes = [10000. * (1. + j * .2) for j in range(len(masses))]
        if role == "blank":
            amplitudes = [a * (0.8 if j in (2, 7) else 0.01) for j, a in enumerate(amplitudes)]
        if role == "qc":
            amplitudes[3] *= [1., 1.8, 0.2][i - 3]
        if i == 1:
            amplitudes[5] = 0.  # Known absent; must never become zero/detected.
        for index in range(820):
            rt = index * .5
            peaks = []
            for j, (mz, expected) in enumerate(zip(masses, rts)):
                sigma = .8 if (i == 1 and j == 6) else 3.
                center = scale * expected + shift
                intensity = amplitudes[j] * math.exp(-.5 * ((rt - center) / sigma) ** 2)
                if intensity > .01:
                    peaks.extend([(mz + isotope * 1.003354835, intensity * fraction)
                                  for isotope, fraction in enumerate([1., .22, .0242])])
            # Sporadic false ions have no persistent mass trace.
            if index % 47 == 0:
                peaks.append((700. + index * .017, 5000.))
            spectrum = ms.MSSpectrum()
            spectrum.setRT(rt)
            spectrum.setMSLevel(1)
            spectrum.setNativeID(f"scan={index+1}")
            spectrum.setType(1)
            settings = spectrum.getInstrumentSettings()
            settings.setPolarity(2 if negative else 1)
            spectrum.setInstrumentSettings(settings)
            peaks.sort()
            spectrum.set_peaks((np.array([p[0] for p in peaks]), np.array([p[1] for p in peaks], dtype=np.float32)))
            exp.addSpectrum(spectrum)
            if index == 100:
                fragment = ms.MSSpectrum()
                fragment.setRT(rt)
                fragment.setMSLevel(2)
                fragment.setNativeID("ms2-evidence")
                fragment.setType(1)
                fragment.setInstrumentSettings(settings)
                precursor = ms.Precursor()
                precursor.setMZ(masses[0])
                precursor.setCharge(-1 if negative else 1)
                precursor.setIsolationWindowLowerOffset(.5)
                precursor.setIsolationWindowUpperOffset(.5)
                fragment.setPrecursors([precursor])
                fragment.set_peaks((np.array([60., 90., 120.]), np.array([300., 800., 500.], dtype=np.float32)))
                exp.addSpectrum(fragment)
        ms.MzMLFile().store(str(path), exp)
        sample_id = f"sample-{i}"
        samples.append({"id": sample_id, "source": str(path), "role": role,
                        "metadata": {"synthetic": True, "injection_order": i+1}})
        truth.append({"sample_id": sample_id, "shift_seconds": shift, "scale": scale,
                      "features": [{"mz": mz, "reference_rt_seconds": rt, "raw_rt_seconds": scale*rt+shift,
                                    "area_seconds": a*3.*math.sqrt(2.*math.pi), "present": a>0}
                                   for mz, rt, a in zip(masses, rts, amplitudes)]})
    config = {"samples": samples, "cache_directory": str(root / "cache"),
              "polarity": "negative" if negative else "positive", "noise_intensity": 100.,
              "min_trace_seconds": 4., "min_trace_sample_rate": 0.1, "min_fwhm_seconds": 3., "gap_fill": True, "rt_tolerance_seconds": 8.}
    request = {"version": 1, "operation_id": str(uuid.uuid4()), "actor": "synthetic-reference",
               "operation": {"operation": "untargeted_batch", "config": config}}
    (root / "request.json").write_text(json.dumps(request, indent=2))
    (root / "truth.json").write_text(json.dumps(truth, indent=2))
    return root


if __name__ == "__main__":
    generate(sys.argv[1], "--negative" in sys.argv[2:])
