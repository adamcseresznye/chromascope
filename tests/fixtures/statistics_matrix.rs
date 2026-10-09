// Reuse the existing authored triangular feature-matrix input; no production output substitution.
use chromascope::untargeted::*;
use std::collections::BTreeMap;
pub fn report() -> Report {
    let roles = [Role::Sample, Role::Blank, Role::Qc, Role::Qc, Role::Qc];
    let samples = roles
        .iter()
        .enumerate()
        .map(|(i, role)| Sample {
            id: format!("s{i}"),
            source: format!("s{i}.mzML"),
            role: *role,
            metadata: serde_json::json!({"synthetic":true}),
        })
        .collect::<Vec<_>>();
    let config = Config {
        samples: samples.clone(),
        cache_directory: "cache".into(),
        ..Default::default()
    };
    let cells = [20., 2., 4., 8., 12.]
        .iter()
        .enumerate()
        .map(|(i, a)| Cell {
            sample_id: format!("s{i}"),
            state: MissingState::Detected,
            intensity: Some(*a),
            openms_intensity: Some(*a / 2.),
            raw_rt_seconds: Some(1.),
            aligned_rt_seconds: Some(1.),
            raw_bounds_seconds: Some([0., 2.]),
            mz: Some(100.),
            charge: Some(1),
            isotope_mz: vec![100.],
            adduct_group: None,
            adduct: None,
            eic: vec![[0., 0.], [1., *a], [2., 0.]],
            apex_spectrum: vec![[100., *a]],
            ms2: vec![],
        })
        .collect();
    let alignments = samples
        .iter()
        .map(|s| Alignment {
            sample_id: s.id.clone(),
            reference_sample_id: s.id.clone(),
            state: "reference".into(),
            slope: Some(1.),
            intercept_seconds: Some(0.),
            landmarks_raw_reference_seconds: vec![],
            residuals_seconds: vec![],
            rms_residual_seconds: None,
            reason: None,
        })
        .collect();
    Report {
        version: 1,
        adapter_version: ADAPTER_VERSION.into(),
        openms_version: "3.5.0".into(),
        config,
        source_hashes: samples
            .iter()
            .map(|s| (s.id.clone(), "a".repeat(64)))
            .collect::<BTreeMap<_, _>>(),
        features: vec![Feature {
            id: "known-triangle".into(),
            mz: 100.,
            aligned_rt_seconds: 1.,
            cells,
            flags: vec!["qc_not_reproducible".into()],
            blank_ratio: Some(10.),
            qc_cv: Some(0.5),
            sample_fraction: Some(1.),
            filter_state: FilterState::Excluded,
        }],
        alignments,
        provenance: serde_json::json!({"synthetic":true}),
        warnings: vec![],
    }
}
