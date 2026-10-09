// Synthetic raw mzML acquisitions with analytically known triangular areas.
// This test helper produces inputs, never substitutes production results.
fn raw_request(root: &std::path::Path) -> chromascope::targeted::BatchRequest {
    use chromascope::{quant, targeted::*};
    use base64::Engine;
    let encode = |values: &[f64]| base64::engine::general_purpose::STANDARD.encode(values.iter().flat_map(|v|v.to_le_bytes()).collect::<Vec<_>>());
    let mut samples = Vec::new();
    for (i, (level, role)) in [(1.0,Role::Standard),(2.0,Role::Standard),(4.0,Role::Standard),(8.0,Role::Standard),(3.0,Role::Qc),(3.1,Role::Qc),(5.0,Role::Unknown),(0.0,Role::Blank)].into_iter().enumerate() {
        let mut spectra = String::new();
        for scan in 0..5 {
            let time = scan as f64/2.0;
            let factor = if scan == 2 {1.0}else{0.0};
            let mass = encode(&[100.0,200.0,300.0]);
            let intensity = encode(&[factor*20.0*level,factor*10.0*level,factor*200.0]);
            let array = |accession:&str,name:&str,binary:&str| format!("<binaryDataArray encodedLength=\"{}\"><cvParam cvRef=\"MS\" accession=\"MS:1000523\" name=\"64-bit float\"/><cvParam cvRef=\"MS\" accession=\"MS:1000576\" name=\"no compression\"/><cvParam cvRef=\"MS\" accession=\"{accession}\" name=\"{name}\"/><binary>{binary}</binary></binaryDataArray>",binary.len());
            spectra.push_str(&format!("<spectrum index=\"{scan}\" id=\"scan={}\" defaultArrayLength=\"3\"><cvParam cvRef=\"MS\" accession=\"MS:1000511\" name=\"ms level\" value=\"1\"/><cvParam cvRef=\"MS\" accession=\"MS:1000130\" name=\"positive scan\"/><cvParam cvRef=\"MS\" accession=\"MS:1000127\" name=\"centroid spectrum\"/><scanList count=\"1\"><scan><cvParam cvRef=\"MS\" accession=\"MS:1000016\" name=\"scan start time\" value=\"{time}\" unitCvRef=\"UO\" unitAccession=\"UO:0000031\" unitName=\"minute\"/></scan></scanList><binaryDataArrayList count=\"2\">{}{}</binaryDataArrayList></spectrum>",scan+1,array("MS:1000514","m/z array",&mass),array("MS:1000515","intensity array",&intensity)));
        }
        let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><mzML xmlns=\"http://psi.hupo.org/ms/mzml\" version=\"1.1.0\"><cvList count=\"2\"><cv id=\"MS\" fullName=\"PSI-MS\" version=\"4.1\" URI=\"https://purl.obolibrary.org/obo/ms.obo\"/><cv id=\"UO\" fullName=\"Unit Ontology\" version=\"1\" URI=\"https://purl.obolibrary.org/obo/uo.obo\"/></cvList><fileDescription><fileContent><cvParam cvRef=\"MS\" accession=\"MS:1000579\" name=\"MS1 spectrum\"/></fileContent></fileDescription><softwareList count=\"0\"/><instrumentConfigurationList count=\"1\"><instrumentConfiguration id=\"IC1\"/></instrumentConfigurationList><dataProcessingList count=\"1\"><dataProcessing id=\"DP1\"/></dataProcessingList><run id=\"reference\" defaultInstrumentConfigurationRef=\"IC1\"><spectrumList count=\"5\" defaultDataProcessingRef=\"DP1\">{spectra}</spectrumList></run></mzML>");
        let path = root.join(format!("sample{i}.mzML"));
        std::fs::write(&path,xml).unwrap();
        samples.push(Sample{id:format!("raw{i}"),source:path.to_str().unwrap().into(),role:role.clone(),injection_order:i as u32,dilution:if role==Role::Unknown{2.0}else{1.0},nominal:if matches!(role,Role::Standard|Role::Qc){std::collections::BTreeMap::from([("a".into(),if role==Role::Qc{3.0}else{level})])}else{Default::default()},exclusions:Default::default()});
    }
    let ion = |name:&str,mass:f64| {let mut a=quant::new_analyte(1);a.expected_rt=1.0;a.rt_window=[0.0,2.0];a.extraction.mass=Some(mass);a.extraction.name=name.into();Ion{extraction:a}};
    BatchRequest{version:1,name:"Synthetic raw triangular reference".into(),detection_smoothing:0,minimum_height:0.0,boundary_fraction:0.05,samples,targets:vec![
        Target{id:"a".into(),quantifier:ion("quantifier",100.0),qualifiers:vec![Qualifier{ion:ion("qualifier",200.0),ratio_range:[0.4,0.6],rt_tolerance_minutes:0.1}],internal_standard:Some("is".into()),is_area_range:None,calibration:Some(CalibrationConfig{degree:1,intercept:Intercept::Zero,weighting:Weighting::InverseX2,unit:Unit::NgMl,range:[1.0,8.0],lod:1.0,loq:1.0,accuracy_tolerance_percent:15.0,qc_cv_limit_percent:15.0,blank_response_limit:0.01})},
        Target{id:"is".into(),quantifier:ion("IS",300.0),qualifiers:vec![],internal_standard:None,is_area_range:Some([80.0,120.0]),calibration:None}
    ]}
}
