//! Local ProteoWizard imports. Originals are never modified; converted runs live
//! in an owned temporary directory until their readers and workers are finished.
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
pub struct ImportedData {
    pub paths: Vec<PathBuf>,
    pub workspace: Option<Arc<tempfile::TempDir>>,
}

pub fn is_mzml(path: &Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    path.is_file() && (name.ends_with(".mzml") || name.ends_with(".mzml.gz"))
}

pub(crate) fn settings_path() -> Option<PathBuf> {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
    }?;
    Some(base.join("chromascope").join("msconvert-path.txt"))
}

/// Environment override, saved setting, then PATH discovery.
pub fn discover_msconvert() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("CHROMASCOPE_MSCONVERT") {
        return Some(PathBuf::from(path));
    }
    if let Some(path) = settings_path().and_then(|p| fs::read_to_string(p).ok()) {
        if !path.trim().is_empty() {
            return Some(PathBuf::from(path.trim()));
        }
    }
    let executable = if cfg!(windows) {
        "msconvert.exe"
    } else {
        "msconvert"
    };
    let on_path = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(executable))
            .find(|p| p.is_file())
    });
    on_path.or_else(discover_installed_msconvert)
}

fn discover_installed_msconvert() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        let Some(base) = std::env::var_os(variable) else {
            continue;
        };
        let root = PathBuf::from(base).join("ProteoWizard");
        if root.join("msconvert.exe").is_file() {
            return Some(root.join("msconvert.exe"));
        }
        if let Ok(entries) = fs::read_dir(root) {
            let mut candidates: Vec<_> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path().join("msconvert.exe"))
                .filter(|p| p.is_file())
                .collect();
            candidates.sort();
            if let Some(path) = candidates.pop() {
                return Some(path);
            }
        }
    }
    None
}

pub fn save_msconvert(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err("Select the msconvert executable, not a directory.".into());
    }
    let settings = settings_path().ok_or("Cannot locate the application settings directory.")?;
    fs::create_dir_all(settings.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(settings, path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())
}

fn conversion_command(executable: &Path, source: &Path, output: &Path) -> Command {
    let mut command = Command::new(executable);
    // Expose SIM/SRM acquisitions as spectra for the spectrum-based viewer.
    // No peak-picking or scan filters are applied.
    command
        .arg(source)
        .args([
            "--mzML",
            "--64",
            "--zlib",
            "--simAsSpectra",
            "--srmAsSpectra",
        ])
        .arg("--outdir")
        .arg(output);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command
}

fn log_tail(path: &Path) -> String {
    let Ok(mut file) = File::open(path) else {
        return String::new();
    };
    let length = file.metadata().map(|m| m.len()).unwrap_or(0);
    let _ = file.seek(SeekFrom::Start(length.saturating_sub(8192)));
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn prepare_input(
    source: &Path,
    executable: Option<&Path>,
    cancelled: &AtomicBool,
) -> Result<ImportedData, String> {
    if cancelled.load(Ordering::Relaxed) {
        return Err("Import cancelled.".into());
    }
    if is_mzml(source) {
        return Ok(ImportedData {
            paths: vec![source.to_path_buf()],
            workspace: None,
        });
    }
    if !source.exists() {
        return Err(format!("Dataset does not exist: {}", source.display()));
    }
    let executable = executable.ok_or(
        "Vendor imports require ProteoWizard. Choose File > Vendor import settings → Locate msconvert, or add msconvert to PATH."
    )?;
    let workspace = Arc::new(
        tempfile::Builder::new()
            .prefix("chromascope-import-")
            .tempdir()
            .map_err(|e| e.to_string())?,
    );
    let output = workspace.path().join("runs");
    fs::create_dir(&output).map_err(|e| e.to_string())?;
    let log_path = workspace.path().join("msconvert.log");
    let stdout = File::create(&log_path).map_err(|e| e.to_string())?;
    let stderr = stdout.try_clone().map_err(|e| e.to_string())?;
    let mut child = conversion_command(executable, source, &output)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|e| {
            format!(
                "Cannot start {}: {}. Check File > Vendor import settings and the required vendor runtimes.",
                executable.display(),
                e
            )
        })?;
    let status = loop {
        if cancelled.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Import cancelled.".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Cannot monitor msconvert: {}", e));
            }
        }
    };
    if !status.success() {
        return Err(format!("ProteoWizard could not import {} ({}). Vendor support depends on its installed libraries and operating system.\n{}", source.display(), status, log_tail(&log_path)));
    }
    let mut paths: Vec<_> = fs::read_dir(&output)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_mzml(p))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(format!(
            "msconvert produced no mzML runs.\n{}",
            log_tail(&log_path)
        ));
    }
    Ok(ImportedData {
        paths,
        workspace: Some(workspace),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_preserves_paths_and_has_no_processing_filters() {
        let cmd = conversion_command(
            Path::new("tools with spaces/msconvert.exe"),
            Path::new("data with spaces/sample.raw"),
            Path::new("output folder"),
        );
        let args: Vec<_> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            [
                "data with spaces/sample.raw",
                "--mzML",
                "--64",
                "--zlib",
                "--simAsSpectra",
                "--srmAsSpectra",
                "--outdir",
                "output folder"
            ]
        );
    }

    #[test]
    fn mzml_bypasses_converter_and_vendor_requires_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let mzml = dir.path().join("run.MZML");
        fs::write(&mzml, b"test").unwrap();
        let cancel = AtomicBool::new(false);
        assert_eq!(
            prepare_input(&mzml, None, &cancel).unwrap().paths,
            vec![mzml]
        );
        let vendor = dir.path().join("run.d");
        fs::create_dir(&vendor).unwrap();
        assert!(prepare_input(&vendor, None, &cancel)
            .unwrap_err()
            .contains("Locate msconvert"));
        cancel.store(true, Ordering::Relaxed);
        assert!(prepare_input(&vendor, None, &cancel)
            .unwrap_err()
            .contains("cancelled"));
    }
}
