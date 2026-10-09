//! Generate explicit synthetic raw inputs for the operational targeted/MCP examples.
//! These are software-reference triangles, not measured standards or assay validation.
//! Numerical results must come from the shared engine; none are generated here.
include!("../tests/fixtures/targeted_support.rs");

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    if arguments.len() != 1 {
        return Err("Usage: cargo run --example targeted_reference -- NEW_DIRECTORY".into());
    }
    let root = std::path::PathBuf::from(&arguments[0]);
    std::fs::create_dir(&root)?;
    let root = root.canonicalize()?;
    let request = chromascope::domain::Request {
        version: 1,
        operation_id: Default::default(),
        actor: "synthetic triangular raw-input example; no scientific approval".into(),
        operation: chromascope::domain::Operation::TargetedBatch {
            batch: raw_request(&root),
        },
    };
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("request.json"))?
        .write_all(&serde_json::to_vec_pretty(&request)?)?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("README.txt"))?
        .write_all(b"Explicit synthetic mzML triangles: four standards, two QC, unknown, blank; quantifier, qualifier, internal standard and dilution. Software-interface reference only. Run request.json through the real CLI or MCP. No numerical result is fabricated by this generator.\n")?;
    println!(
        "Generated synthetic raw inputs and request: {}",
        root.display()
    );
    Ok(())
}
