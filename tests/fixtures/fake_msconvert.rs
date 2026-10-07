// Standalone process fixture compiled by the integration test using rustc.
use std::{env, fs, path::PathBuf, thread, time::Duration};

fn main() {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let source = PathBuf::from(&args[0]);
    let output = PathBuf::from(&args[args.iter().position(|s| s == "--outdir").unwrap() + 1]);
    match source.file_name().unwrap().to_string_lossy().as_ref() {
        "fail.raw" => { eprintln!("fixture vendor reader failed"); std::process::exit(7); }
        "empty.raw" => {}
        "sleep.raw" => {
            fs::write(source.with_extension("ready"), b"ready").unwrap();
            thread::sleep(Duration::from_secs(30));
        }
        _ => {
            // Targeted acquisitions need these options for spectral output.
            if !args.iter().any(|s| s == "--simAsSpectra") || !args.iter().any(|s| s == "--srmAsSpectra") {
                eprintln!("SIM/SRM spectrum import options are missing");
                std::process::exit(8);
            }
            let data = include_bytes!("../../test_file/data_dependent_02.mzML");
            fs::write(output.join("sample 1.mzML"), data).unwrap();
            fs::write(output.join("sample 2.mzML"), data).unwrap();
        }
    }
}
