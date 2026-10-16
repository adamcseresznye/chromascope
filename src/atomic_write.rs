//! Crash-safe file saves.
//!
//! Every file the user keeps (sessions, methods, batches, exports, presets, settings) is written
//! to a temporary file in the same directory and then renamed over the target. A crash, a full
//! disk or a sync-client conflict part-way through leaves the previous file intact instead of a
//! truncated one.
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Writes `bytes` to `path` atomically, replacing an existing file.
///
/// The temporary file is created next to `path` (same volume, so the rename is atomic) with the
/// process's default permissions; an existing target keeps its permissions. The temporary file is
/// removed when any step fails.
pub fn write_atomic(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    let path = path.as_ref();
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let temp = temp_path(dir, name);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes.as_ref())?;
        file.sync_all()?;
        drop(file);
        if let Ok(meta) = fs::metadata(path) {
            // Best effort: keep the mode of the file being replaced.
            let _ = fs::set_permissions(&temp, meta.permissions());
        }
        rename_over(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn temp_path(dir: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let mut temp_name = std::ffi::OsString::from(".");
    temp_name.push(name);
    temp_name.push(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
    dir.join(temp_name)
}

/// Renames `from` over `to`. Windows refuses with "Access is denied" while an indexer, antivirus
/// scanner or sync client briefly holds the target open, so retry a few times before giving up.
fn rename_over(from: &Path, to: &Path) -> io::Result<()> {
    let mut last = None;
    for attempt in 0..8u64 {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && cfg!(windows) => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(25 * (attempt + 1)));
            }
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("rename failed")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leftovers(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn creates_and_replaces_without_leaving_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.chromascope");
        write_atomic(&path, b"first").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"first");
        write_atomic(&path, "second, longer").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second, longer");
        write_atomic(&path, b"x").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"x");
        assert!(leftovers(dir.path()).is_empty());
    }

    #[test]
    fn failed_save_keeps_the_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        // Target is a directory: the final rename must fail and nothing may be damaged.
        let target = dir.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep.txt"), b"keep").unwrap();
        assert!(write_atomic(&target, b"data").is_err());
        assert_eq!(fs::read(target.join("keep.txt")).unwrap(), b"keep");
        assert!(leftovers(dir.path()).is_empty());
        // Missing parent directory is an error, not a silent success.
        assert!(write_atomic(dir.path().join("nope").join("f"), b"data").is_err());
    }

    #[test]
    fn rejects_paths_without_a_file_name() {
        assert!(write_atomic(Path::new(""), b"data").is_err());
        assert!(write_atomic(Path::new(".."), b"data").is_err());
    }
}
