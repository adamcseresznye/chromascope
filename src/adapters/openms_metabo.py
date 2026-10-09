"""Chromascope OpenMS adapter protocol v1; local, sequential raw-run ownership.

OpenMS handles mass traces, chromatographic splitting, isotope assembly, adduct
deconvolution, affine alignment and QT correspondence. Gap filling is explicitly
labelled windowed MS1 trapezoidal integration, never an invented detected peak.
"""
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import sys
import traceback
import time
import uuid

VERSION = "openms-metabo-v1"


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def encode(value):
    if isinstance(value, bytes):
        return value.decode()
    if isinstance(value, dict):
        return {encode(k): encode(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [encode(v) for v in value]
    return value


def save(path, value):
    # Checkpoints become visible only after a full JSON write.
    temp = path.with_name(path.name + "." + str(uuid.uuid4()) + ".tmp")
    with open(temp, "x", encoding="utf-8") as stream:
        json.dump(value, stream, allow_nan=False, sort_keys=True)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temp, path)


def configure(algorithm, values, provenance):
    params = algorithm.getDefaults()
    for key, value in values.items():
        params.setValue(key, value)
    algorithm.setParameters(params)
    provenance[type(algorithm).__name__] = encode(params.asDict())
    return algorithm


def read_run(ms, sample, c, max_scans):
    import numpy as np
    exp = ms.MSExperiment()
    ms.MzMLFile().load(sample["source"], exp)
    if exp.size() > max_scans:
        raise RuntimeError("resource_limit: sample scan budget exceeded")
    selected = ms.MSExperiment()
    ms1 = []
    ms2 = []
    polarity = 1 if c["polarity"] == "positive" else 2
    for index, spectrum in enumerate(exp):
        if spectrum.getInstrumentSettings().getPolarity() != polarity:
            continue
        masses, intensities = spectrum.get_peaks()
        if not math.isfinite(spectrum.getRT()) or spectrum.getRT() < 0 or not np.isfinite(masses).all() or not np.isfinite(intensities).all() or (masses <= 0).any() or (intensities < 0).any() or (np.diff(masses) < 0).any():
            raise RuntimeError("invalid_source: finite nonnegative RT/intensity and sorted positive m/z required")
        if spectrum.getMSLevel() == 1:
            if spectrum.getType() != 1:  # SpectrumSettings.CENTROID
                if not c["centroid_profile"]:
                    raise RuntimeError("Profile or unspecified MS1 type: declare centroid_profile=true")
                centroid = ms.MSSpectrum()
                ms.PeakPickerHiRes().pick(spectrum, centroid)
                spectrum = centroid
            selected.addSpectrum(spectrum)
            ms1.append((index, spectrum))
        elif spectrum.getMSLevel() == 2:
            ms2.append((index, spectrum))
    if not ms1:
        raise RuntimeError("No MS1 spectra in declared polarity; full-scan discovery unavailable")
    selected.sortSpectra(True)
    selected.updateRanges()
    selected.setMetaValue("chromascope_raw_scan_count", exp.size())
    ms1.sort(key=lambda v: v[1].getRT())
    return selected, ms1, ms2


def bounds(feature):
    points = feature.getConvexHull().getHullPoints()
    if len(points):
        return [float(min(p[0] for p in points)), float(max(p[0] for p in points))]
    raise RuntimeError("OpenMS feature lacks retained chromatographic hull")


def adduct_groups(ms, fm, c, provenance):
    negative = c["polarity"] == "negative"
    decon = configure(ms.MetaboliteFeatureDeconvolution(), {
        "charge_min": -c["max_charge"] if negative else 1,
        "charge_max": -1 if negative else c["max_charge"],
        "charge_span_max": c["max_charge"],
        "negative_mode": "true" if negative else "false",
        "potential_adducts": ([b"H-1:-:0.7", b"Cl:-:0.2", b"H-3O-1:-:0.1"] if negative
                              else [b"H:+:0.4", b"Na:+:0.25", b"NH4:+:0.25", b"K:+:0.1"]),
        "unit": "ppm", "mass_max_diff": c["correspondence_ppm"],
        "retention_max_diff": c["rt_tolerance_seconds"],
        "retention_max_diff_local": c["rt_tolerance_seconds"],
    }, provenance)
    annotated, groups, edges = ms.FeatureMap(), ms.ConsensusMap(), ms.ConsensusMap()
    decon.compute(fm, annotated, groups, edges)
    memberships = {}
    for group in groups:
        handles = group.getFeatureList()
        group_id = str(uuid.uuid5(uuid.NAMESPACE_OID, ":".join(sorted(str(h.getUniqueId()) for h in handles))))
        for handle in handles:
            memberships[str(handle.getUniqueId())] = group_id
    annotations = {str(f.getUniqueId()): encode(f.getMetaValue("dc_charge_adducts"))
                   for f in annotated if f.metaValueExists("dc_charge_adducts")}
    return memberships, annotations


def detect(ms, exp, c, provenance):
    mtd = configure(ms.MassTraceDetection(), {
        "mass_error_ppm": c["detection_ppm"], "noise_threshold_int": c["noise_intensity"],
        "min_trace_length": c["min_trace_seconds"], "quant_method": "area",
        "min_sample_rate": c["min_trace_sample_rate"],
    }, provenance)
    traces = []
    mtd.run(exp, traces, 0)
    epd = configure(ms.ElutionPeakDetection(), {
        "chrom_fwhm": c["peak_fwhm_seconds"], "width_filtering": "fixed",
        "min_fwhm": c["min_fwhm_seconds"], "max_fwhm": c["max_fwhm_seconds"],
    }, provenance)
    split = []
    epd.detectPeaks(traces, split)
    ffm = configure(ms.FeatureFindingMetabo(), {
        "charge_lower_bound": 1, "charge_upper_bound": c["max_charge"],
        "chrom_fwhm": c["peak_fwhm_seconds"], "report_convex_hulls": "true",
        "isotope_filtering_model": "metabolites (5% RMS)", "remove_single_traces": "false",
        "report_summed_ints": "false", "use_smoothed_intensities": "false",
        "report_smoothed_intensities": "false",
    }, provenance)
    fm = ms.FeatureMap()
    ffm.run(split, fm, [])
    fm.setUniqueIds()
    fm.updateRanges()
    if c["polarity"] == "negative":
        for i in range(fm.size()):
            feature = fm[i]
            feature.setCharge(-abs(feature.getCharge()))
            fm[i] = feature
    if not fm.size():
        return fm, {}
    memberships, annotations = adduct_groups(ms, fm, c, provenance)
    # Retain original ion m/z, charges and feature areas. Decharged masses are
    # hypotheses and must not replace ions used for correspondence or EICs.
    info = {}
    for feature in fm:
        key = str(feature.getUniqueId())
        isotope_mz = []
        for hull in feature.getConvexHulls():
            pts = hull.getHullPoints()
            if len(pts):
                isotope_mz.append(float(sum(p[1] for p in pts) / len(pts)))
        info[key] = {"raw_bounds_seconds": bounds(feature), "raw_rt_seconds": feature.getRT(),
                     "openms_intensity": feature.getIntensity(),
                     "mz": feature.getMZ(), "charge": feature.getCharge() or None,
                     "isotope_mz": isotope_mz, "adduct_group": memberships.get(key),
                     "adduct": str(annotations[key]) if key in annotations else None}
    return fm, info


def eic(ms1, mz, interval, ppm):
    tolerance = mz * ppm / 1e6
    points = []
    for _, spectrum in ms1:
        rt = spectrum.getRT()
        if interval[0] <= rt <= interval[1]:
            masses, intensities = spectrum.get_peaks()
            # Vectorized search uses sorted OpenMS spectra, bounded to one scan.
            lo = masses.searchsorted(mz - tolerance, side="left")
            hi = masses.searchsorted(mz + tolerance, side="right")
            points.append([rt, float(intensities[lo:hi].sum())])
    # Duplicate acquisition RTs are averaged, preserving deterministic integration.
    grouped = {}
    for rt, intensity in points:
        grouped.setdefault(rt, []).append(intensity)
    return [[rt, statistics.mean(values)] for rt, values in sorted(grouped.items())]


def integrate(points):
    return sum((b[0] - a[0]) * (a[1] + b[1]) / 2 for a, b in zip(points, points[1:]))


def max_consecutive_signal(points, threshold):
    longest = current = 0
    for _, intensity in points:
        current = current + 1 if intensity >= threshold else 0
        longest = max(longest, current)
    return longest


def evidence(cell, ms1, ms2, ppm):
    if not cell["eic"] or cell["intensity"] is None:
        return
    apex = max(cell["eic"], key=lambda p: p[1])[0]
    _, scan = min(ms1, key=lambda v: abs(v[1].getRT() - apex))
    cell["apex_spectrum"] = [[float(m), float(i)] for m, i in zip(*scan.get_peaks())]
    lo, hi = cell["raw_bounds_seconds"]
    for index, scan in ms2:
        if not lo <= scan.getRT() <= hi:
            continue
        for precursor in scan.getPrecursors():
            if abs(precursor.getMZ() - cell["mz"]) <= cell["mz"] * ppm / 1e6:
                cell["ms2"].append({"original_index": index, "native_id": scan.getNativeID(),
                    "raw_rt_seconds": scan.getRT(), "precursor_mz": precursor.getMZ(),
                    "charge": precursor.getCharge(), "isolation_lower_offset_da": precursor.getIsolationWindowLowerOffset(),
                    "isolation_upper_offset_da": precursor.getIsolationWindowUpperOffset(),
                    "association": "precursor_mz_and_raw_rt_window", "collision_energy_ev": precursor.getActivationEnergy() if precursor.getActivationEnergy() > 0 else None,
                    "representation": {1: "centroid", 2: "profile"}.get(scan.getType(), "unknown"),
                    "peaks": [[float(m), float(i)] for m, i in zip(*scan.get_peaks())]})
                break


def filter_row(row, samples, c):
    values = {role: [cell["intensity"] for cell, sample in zip(row["cells"], samples)
                     if sample["role"] == role and cell["intensity"] is not None]
              for role in ("sample", "blank", "qc")}
    flags = []
    count = sum(s["role"] == "sample" for s in samples)
    fraction = len(values["sample"]) / count if count else None
    ratio = None
    if values["blank"] and values["sample"]:
        blank = statistics.median(values["blank"])
        if blank > 0:
            ratio = statistics.median(values["sample"]) / blank
            if ratio < c["blank_ratio"]:
                flags.append("blank_contamination")
        else:
            flags.append("blank_zero_signal")
    else:
        flags.append("blank_filter_indeterminate")
    cv = None
    if len(values["qc"]) >= 3 and statistics.mean(values["qc"]) > 0:
        cv = statistics.stdev(values["qc"]) / statistics.mean(values["qc"])
        if cv > c["max_qc_cv"]:
            flags.append("qc_not_reproducible")
    else:
        flags.append("qc_filter_indeterminate")
    if fraction is not None and fraction < c["min_sample_fraction"]:
        flags.append("low_sample_prevalence")
    row.update(flags=flags, blank_ratio=ratio, qc_cv=cv, sample_fraction=fraction)
    excluded = any(flag in flags for flag in ("blank_contamination", "qc_not_reproducible", "low_sample_prevalence"))
    indeterminate = any(flag.endswith("indeterminate") for flag in flags)
    row["filter_state"] = "excluded" if excluded else "indeterminate" if indeterminate else "included"


def main(attempt):
    import pyopenms as ms
    if ms.__version__ != "3.5.0":
        raise RuntimeError("unsupported_version: adapter requires pyOpenMS 3.5.0")
    request = json.loads((attempt / "request.json").read_text())
    if request["version"] != 1 or request["adapter_version"] != VERSION:
        raise RuntimeError("unsupported_schema")
    c = request["config"]
    maps, infos, cache_records = [], [], []
    provenance = {"python_version": sys.version, "adapter_sha256": digest(attempt / "adapter.py"),
                  "attempt_directory": str(attempt), "algorithms": {}, "units": {
                      "rt": "seconds", "mz": "Th", "intensity": "unsmoothed monoisotopic EIC trapezoid intensity*seconds",
                      "openms_intensity": "original OpenMS monoisotopic FWHM area intensity*seconds",
                      "gap_fill": "unsmoothed windowed trapezoid intensity*seconds", "qc_cv": "fraction"}}
    if c["centroid_profile"]:
        provenance["algorithms"]["PeakPickerHiRes"] = encode(ms.PeakPickerHiRes().getDefaults().asDict())
    cache = Path(c["cache_directory"])
    for i, sample in enumerate(c["samples"]):
        identity = request["source_hashes"][sample["id"]]
        if digest(sample["source"]) != identity:
            raise RuntimeError("stale_source: source hash changed")
        key = hashlib.sha256(json.dumps({"source": identity, "config": c, "openms": ms.__version__,
                                       "adapter": provenance["adapter_sha256"]}, sort_keys=True).encode()).hexdigest()
        entry = cache / ("detection-" + key)
        if entry.exists():
            manifest = json.loads((entry / "manifest.json").read_text())
            if set(manifest["outputs"]) != {"features.featureXML", "features.json"} or manifest["key"] != key or manifest["source_sha256"] != identity:
                raise RuntimeError("corrupt_cache: checkpoint identity/output schema mismatch")
            if manifest["scan_count"] > request["max_scans"]:
                raise RuntimeError("resource_limit: cached sample scan budget exceeded")
            for name, sha in manifest["outputs"].items():
                if digest(entry / name) != sha:
                    raise RuntimeError("corrupt_cache: checkpoint checksum mismatch")
            fm = ms.FeatureMap()
            ms.FeatureXMLFile().load(str(entry / "features.featureXML"), fm)
            info = json.loads((entry / "features.json").read_text())
            # featureXML text precision is not the numerical checkpoint authority.
            # Restore exact JSON doubles before alignment/correspondence replay.
            for feature_index in range(fm.size()):
                feature = fm[feature_index]
                exact = info[str(feature.getUniqueId())]
                feature.setRT(exact["raw_rt_seconds"])
                feature.setMZ(exact["mz"])
                feature.setIntensity(exact["openms_intensity"])
                fm[feature_index] = feature
            provenance["algorithms"].update(manifest["algorithms"])
            reused = True
        else:
            exp, _, _ = read_run(ms, sample, c, request["max_scans"])
            scan_count = int(exp.getMetaValue("chromascope_raw_scan_count"))
            fm, info = detect(ms, exp, c, provenance["algorithms"])
            del exp
            temporary = cache / ("checkpoint-" + str(uuid.uuid4()))
            temporary.mkdir()
            ms.FeatureXMLFile().store(str(temporary / "features.featureXML"), fm)
            save(temporary / "features.json", info)
            manifest = {"key": key, "source_sha256": identity, "scan_count": scan_count, "algorithms": provenance["algorithms"],
                        "outputs": {name: digest(temporary / name) for name in ("features.featureXML", "features.json")}}
            save(temporary / "manifest.json", manifest)
            # Competing attempts never replace an already-completed checkpoint.
            for retry in range(21):
                try:
                    temporary.rename(entry)
                    break
                except FileExistsError:
                    raise RuntimeError("cache_conflict: another job completed this checkpoint; retry")
                except PermissionError:
                    if retry == 20:
                        raise
                    time.sleep(.1)
            reused = False
        cache_records.append({"sample_id": sample["id"], "checkpoint": str(entry), "reused": reused, **manifest})
        if fm.size() > 100000 or sum(v.size() for v in maps) + fm.size() > c["max_matrix_cells"]:
            raise RuntimeError("resource_limit: detected feature/cell budget exceeded")
        maps.append(fm)
        infos.append(info)
        (attempt / "progress.txt").write_text(str(i + 1))
    ref = max(range(len(maps)), key=lambda i: maps[i].size())
    if maps[ref].size() < 2:
        raise RuntimeError("insufficient_alignment_landmarks: need at least two detected features in reference")
    aligner = configure(ms.MapAlignmentAlgorithmPoseClustering(), {
        "pairfinder:distance_MZ:max_difference": c["correspondence_ppm"],
        "pairfinder:distance_MZ:unit": "ppm",
        "pairfinder:distance_RT:max_difference": c["rt_tolerance_seconds"] * 10,
        "superimposer:mz_pair_max_distance": max(f.getMZ() for f in maps[ref]) * c["correspondence_ppm"] / 1e6,
    }, provenance["algorithms"])
    aligner.setReference(maps[ref])
    transforms, alignments = [], []
    for i, fm in enumerate(maps):
        trafo = ms.TransformationDescription()
        if i == ref:
            trafo.fitModel("identity")
            pairs = []
            state = "reference"
        elif fm.size() < 2 and c["samples"][i]["role"] == "blank":
            transforms.append(None)
            alignments.append({"sample_id": c["samples"][i]["id"], "reference_sample_id": c["samples"][ref]["id"],
                               "state": "unavailable", "reason": "Fewer than two blank landmarks; raw checkpoint retained; no RT transform inferred"})
            maps[i] = ms.FeatureMap()
            continue
        elif fm.size() < 2:
            raise RuntimeError("insufficient_alignment_landmarks: sample cannot be aligned or gap-filled reliably")
        else:
            aligner.align(fm, trafo)  # Failure is fatal, never silently bypassed.
            pairs = [[p.first, p.second] for p in trafo.getDataPoints()]
            if len(pairs) < 2:
                raise RuntimeError("insufficient_alignment_landmarks: fewer than two matched landmarks")
            state = "aligned"
            ms.MapAlignmentTransformer().transformRetentionTimes(fm, trafo, True)
        slope = trafo.apply(1.) - trafo.apply(0.)
        intercept = trafo.apply(0.)
        if not math.isfinite(slope) or slope <= 0 or not math.isfinite(intercept):
            raise RuntimeError("invalid_alignment: transformation is not finite and monotone")
        transforms.append((slope, intercept))
        residuals = [slope * x + intercept - y for x, y in pairs]
        alignments.append({"sample_id": c["samples"][i]["id"], "reference_sample_id": c["samples"][ref]["id"],
                           "state": state, "slope": slope, "intercept_seconds": intercept,
                           "landmarks_raw_reference_seconds": pairs,
                           "residuals_seconds": residuals,
                           "rms_residual_seconds": math.sqrt(statistics.mean(v*v for v in residuals)) if residuals else None})
        ms.FeatureXMLFile().store(str(attempt / f"aligned-{i}.featureXML"), fm)
    group = configure(ms.FeatureGroupingAlgorithmQT(), {
        "distance_RT:max_difference": c["rt_tolerance_seconds"],
        "distance_MZ:max_difference": c["correspondence_ppm"], "distance_MZ:unit": "ppm",
    }, provenance["algorithms"])
    consensus = ms.ConsensusMap()
    headers = consensus.getColumnHeaders()
    for i, fm in enumerate(maps):
        header = ms.ColumnHeader()
        header.filename = c["samples"][i]["source"]
        header.size = fm.size()
        header.unique_id = fm.getUniqueId()
        headers[i] = header
    consensus.setColumnHeaders(headers)
    group.group(maps, consensus)
    if consensus.size() * len(c["samples"]) > c["max_matrix_cells"]:
        raise RuntimeError("resource_limit: feature matrix cell budget exceeded")
    consensus.setUniqueIds()
    ms.ConsensusXMLFile().store(str(attempt / "consensus.consensusXML"), consensus)
    rows = []
    for f in sorted(consensus, key=lambda v: (v.getMZ(), v.getRT(), v.getUniqueId())):
        cells = []
        handles = {h.getMapIndex(): h for h in f.getFeatureList()}
        for i, sample in enumerate(c["samples"]):
            cell = {"sample_id": sample["id"], "state": "not_detected", "intensity": None,
                    "openms_intensity": None,
                    "raw_rt_seconds": None, "aligned_rt_seconds": None, "raw_bounds_seconds": None,
                    "mz": None, "charge": None, "isotope_mz": [], "adduct_group": None,
                    "adduct": None, "eic": [], "apex_spectrum": [], "ms2": []}
            if i in handles:
                h = handles[i]
                cell.update(infos[i][str(h.getUniqueId())])
                cell.update(state="detected", intensity=h.getIntensity(), openms_intensity=h.getIntensity(), aligned_rt_seconds=h.getRT())
            cells.append(cell)
        rows.append({"id": str(uuid.uuid5(uuid.NAMESPACE_URL, json.dumps([request["source_hashes"], c, round(f.getMZ(), 8), round(f.getRT(), 6)], sort_keys=True))),
                     "mz": f.getMZ(), "aligned_rt_seconds": f.getRT(), "cells": cells})
    # Read one raw run at a time; retain only feature windows and linked evidence.
    for i, sample in enumerate(c["samples"]):
        if transforms[i] is None:
            for row in rows:
                row["cells"][i]["state"] = "alignment_unavailable"
            continue
        exp, ms1, ms2 = read_run(ms, sample, c, request["max_scans"])
        slope, intercept = transforms[i]
        coverage = [ms1[0][1].getRT(), ms1[-1][1].getRT()]
        for row in rows:
            cell = row["cells"][i]
            if cell["state"] == "detected":
                cell["eic"] = eic(ms1, cell["mz"], cell["raw_bounds_seconds"], c["detection_ppm"])
                cell["intensity"] = integrate(cell["eic"])
            else:
                raw_rt = (row["aligned_rt_seconds"] - intercept) / slope
                interval = [raw_rt - c["rt_tolerance_seconds"] / slope, raw_rt + c["rt_tolerance_seconds"] / slope]
                if interval[0] < coverage[0] or interval[1] > coverage[1]:
                    cell["state"] = "outside_acquisition"
                elif not c["gap_fill"]:
                    cell["state"] = "gap_fill_disabled"
                else:
                    points = eic(ms1, row["mz"], interval, c["correspondence_ppm"])
                    cell["eic"] = points
                    cell["raw_bounds_seconds"] = interval
                    cell["mz"] = row["mz"]
                    if len(points) < c["gap_min_scans"]:
                        cell["state"] = "not_detected"
                    elif max_consecutive_signal(points, c["noise_intensity"]) < c["gap_min_scans"]:
                        cell["state"] = "below_threshold"
                    else:
                        apex = max(points, key=lambda p: p[1])[0]
                        cell.update(state="gap_filled", intensity=integrate(points), raw_rt_seconds=apex,
                                    aligned_rt_seconds=slope * apex + intercept)
            evidence(cell, ms1, ms2, c["correspondence_ppm"])
        del exp, ms1, ms2
        (attempt / "progress.txt").write_text(str(len(c["samples"]) + i + 1))
    for row in rows:
        filter_row(row, c["samples"], c)
    provenance["checkpoints"] = cache_records
    provenance["outputs"] = {p.name: digest(p) for p in attempt.iterdir()
                             if p.suffix in (".featureXML", ".consensusXML")}
    warnings = ["Adduct/isotope groups are hypotheses, not compound identities.",
                "MS/MS is associated by precursor mass and RT; no DIA deconvolution or purity claim.",
                "Gap-filled window areas may include interference; original detected values remain unchanged.",
                "QC and blank flags retain every row; indeterminate evidence never passes silently."]
    report = {"version": 1, "adapter_version": VERSION, "openms_version": ms.__version__, "config": c,
              "source_hashes": request["source_hashes"], "features": rows, "alignments": alignments,
              "provenance": provenance, "warnings": warnings}
    save(attempt / "report.json", report)


if __name__ == "__main__":
    attempt = Path(sys.argv[1])
    try:
        main(attempt)
    except Exception as error:
        prefix = str(error).split(":")[0]
        code = prefix if prefix in ("unsupported_version", "unsupported_schema", "resource_limit", "stale_source", "corrupt_cache", "cache_conflict", "invalid_source", "insufficient_alignment_landmarks", "invalid_alignment") else "adapter_failure"
        save(attempt / "error.json", {"code": code, "message": str(error), "traceback": traceback.format_exc()})
        traceback.print_exc()
        sys.exit(1)
