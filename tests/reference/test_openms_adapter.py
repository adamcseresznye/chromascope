"""Independent chemistry/interval/false-gap references for adapter v1.
Run explicitly with local pyOpenMS 3.5.0; synthetic test inputs are CC0.
"""
import importlib.util
from pathlib import Path
import unittest
import numpy as np
import pyopenms as ms

spec = importlib.util.spec_from_file_location("adapter", Path(__file__).parents[2] / "src/adapters/openms_metabo.py")
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)


class ReferenceTests(unittest.TestCase):
    def test_eic_mass_bounds_are_inclusive(self):
        spectrum = ms.MSSpectrum()
        spectrum.setRT(1.)
        spectrum.set_peaks((np.array([99., 100., 101., 101.1]), np.array([1., 2., 3., 999.], dtype=np.float32)))
        self.assertEqual(adapter.eic([(0, spectrum)], 100., [0., 2.], 10000.), [[1., 6.]])
    def test_positive_adduct_pair_and_rt_false_pair(self):
        # Known proton/sodium ions from neutral M=100. Neutral adduct shift
        # 21.981942 Da is independently declared, not obtained from grouping.
        fm = ms.FeatureMap()
        for i, mz in enumerate([101.0072764666, 122.9892184666, 222.9892184666]):
            feature = ms.Feature()
            feature.setUniqueId(i + 1)
            feature.setMZ(mz)
            feature.setRT(50. if i < 2 else 150.)
            feature.setCharge(1)
            feature.setIntensity(10000.)
            hull = ms.ConvexHull2D()
            hull.setHullPoints(np.array([[feature.getRT()-5, mz], [feature.getRT()+5, mz]], dtype=np.float32))
            feature.setConvexHulls([hull])
            fm.push_back(feature)
        original = [(f.getMZ(), f.getCharge(), f.getIntensity()) for f in fm]
        memberships, annotations = adapter.adduct_groups(ms, fm, {"polarity": "positive", "max_charge": 1,
            "correspondence_ppm": 10., "rt_tolerance_seconds": 10.}, {})
        self.assertEqual(memberships["1"], memberships["2"])
        self.assertNotEqual(memberships.get("3"), memberships["1"])
        self.assertEqual(annotations["1"], "H1")
        self.assertEqual(annotations["2"], "Na1")
        self.assertEqual(original, [(f.getMZ(), f.getCharge(), f.getIntensity()) for f in fm])

    def test_negative_chloride_adduct_pair(self):
        fm = ms.FeatureMap()
        # Neutral M=200: [M-H]- and [M+35Cl]- including electron mass.
        for i, mz in enumerate([198.9927235334, 234.969401261]):
            f = ms.Feature()
            f.setUniqueId(i+1)
            f.setMZ(mz)
            f.setRT(60.)
            f.setCharge(-1)
            f.setIntensity(10000.)
            hull = ms.ConvexHull2D()
            hull.setHullPoints(np.array([[55., mz], [65., mz]], dtype=np.float32))
            f.setConvexHulls([hull])
            fm.push_back(f)
        groups, annotations = adapter.adduct_groups(ms, fm, {"polarity": "negative", "max_charge": 1,
            "correspondence_ppm": 10., "rt_tolerance_seconds": 10.}, {})
        self.assertEqual(groups["1"], groups["2"])
        self.assertEqual(annotations["1"], "H-1")
        self.assertEqual(annotations["2"], "Cl1")

    def test_gap_fill_rejects_isolated_false_spikes(self):
        self.assertEqual(adapter.max_consecutive_signal([[0,100],[1,0],[2,100],[3,0],[4,100]], 50), 1)
        self.assertEqual(adapter.max_consecutive_signal([[0,0],[1,100],[2,100],[3,100],[4,0]], 50), 3)
        self.assertEqual(adapter.integrate([[0,0],[1,4],[2,0]]), 4.)

    def test_filter_indeterminate_does_not_pass(self):
        row = {"cells": [{"intensity": 20.}, {"intensity": None}]}
        adapter.filter_row(row, [{"role": "sample"}, {"role": "blank"}],
                           {"blank_ratio": 5., "max_qc_cv": .3, "min_sample_fraction": .5})
        self.assertEqual(row["filter_state"], "indeterminate")
        self.assertIsNone(row["blank_ratio"])
        self.assertIsNone(row["qc_cv"])


if __name__ == "__main__":
    unittest.main()
