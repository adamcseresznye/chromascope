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
        rename_with_retry(&temp, path)
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

/// Renames `from` to `to` (a file or a directory), retrying while Windows reports a transient lock.
///
/// Windows refuses with "Access is denied" or a sharing violation while an indexer, antivirus
/// scanner or sync client briefly holds the source or target open, typically right after the
/// files were created. The delivery tests that move a freshly exported bundle hit this about one
/// run in three on a developer machine. Other errors, and every error on other platforms, are
/// returned at once.
pub fn rename_with_retry(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    let mut last = None;
    for attempt in 0..12u64 {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if is_transient_lock(&e) => {
                last = Some(e);
                std::thread::sleep(std::time::Duration::from_millis(25 * (attempt + 1)));
            }
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| io::Error::other("rename failed")))
}

fn is_transient_lock(error: &io::Error) -> bool {
    // ERROR_SHARING_VIOLATION (32) and ERROR_LOCK_VIOLATION (33) have no stable ErrorKind.
    cfg!(windows)
        && (error.kind() == io::ErrorKind::PermissionDenied
            || matches!(error.raw_os_error(), Some(32 | 33)))
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
    fn rename_with_retry_waits_for_a_directory_that_is_briefly_locked() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle");
        fs::create_dir(&bundle).unwrap();
        let file = bundle.join("report.json");
        fs::write(&file, b"{}").unwrap();
        // An open handle without delete sharing blocks a directory rename on Windows, like a
        // virus scanner that has just opened a new file. It is released while we retry.
        let mut open = OpenOptions::new();
        open.read(true);
        #[cfg(windows)]
        {
            // Rust opens files with FILE_SHARE_DELETE by default; a scanner does not.
            use std::os::windows::fs::OpenOptionsExt;
            open.share_mode(1); // FILE_SHARE_READ only
        }
        let handle = open.open(&file).unwrap();
        #[cfg(windows)]
        assert!(
            fs::rename(&bundle, dir.path().join("probe")).is_err(),
            "the open handle must block a plain rename, or this test proves nothing"
        );
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(120));
            drop(handle);
        });
        let moved = dir.path().join("moved");
        rename_with_retry(&bundle, &moved).unwrap();
        release.join().unwrap();
        assert!(moved.join("report.json").exists());
        assert!(!bundle.exists());
        // A missing source is a real error and must not be retried into success.
        assert!(rename_with_retry(&bundle, dir.path().join("again")).is_err());
    }

    #[test]
    fn rejects_paths_without_a_file_name() {
        assert!(write_atomic(Path::new(""), b"data").is_err());
        assert!(write_atomic(Path::new(".."), b"data").is_err());
    }
}
