//! Atomic file writes (#126 DEF8, roast defer-bucket): write to a same-directory
//! temp file, fsync it, then rename it over the target.
//!
//! `std::fs::write` truncates the target in place, so a crash, full disk, or
//! `SIGKILL` mid-write leaves a half-written `pins.yaml` or manifest — a torn
//! write of governance data that a later run reads as corrupt (or, worse, as a
//! silently different pin set). `std::fs::rename` is atomic on the same volume on
//! both Unix (`rename(2)`) and Windows (`MoveFileExW` with
//! `MOVEFILE_REPLACE_EXISTING`), so a concurrent or subsequent reader observes
//! either the whole old file or the whole new one, never a partial. The temp file
//! lives in the target's own directory so the rename never crosses a filesystem
//! boundary (a cross-device rename is not atomic and would fail).
//!
//! Scope: this prevents a *torn* write. Full crash-durability of the rename
//! metadata itself would additionally fsync the parent directory; that is left
//! out (Unix-only, and beyond DEF8's "temp-file + fsync + atomic rename").

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Write `bytes` to `path` atomically: a reader never observes a partial write.
///
/// On success the target is replaced in a single step. On any error the target
/// is left exactly as it was and the temp file is best-effort removed, so a
/// failed write leaves no residue in the directory.
pub(crate) fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;

    // A unique sidecar temp in the SAME directory, so the rename is same-volume
    // (hence atomic). The pid + a nanosecond stamp keep two concurrent processes
    // from colliding on the temp; the leading dot keeps it out of `*.md` globs.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(file_name);
    tmp_name.push(format!(".tmp-{}-{stamp}", std::process::id()));
    let tmp = dir.join(tmp_name);

    // Write, flush to the OS, fsync to disk, then rename over the target. Any
    // early error removes the temp so no half-written sidecar is left behind.
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?; // the bytes are durably on disk before the rename swaps them in
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root(label: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("mdatron-atomic-{label}-{stamp}"));
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn writes_a_new_file_with_exact_content() {
        let root = tmp_root("new");
        let path = root.join("pins.yaml");
        write(&path, b"alpha bytes").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"alpha bytes");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn overwrites_an_existing_file_completely() {
        // The re-pin case: a shorter payload must fully replace a longer one, not
        // truncate-in-place and leave a tail.
        let root = tmp_root("overwrite");
        let path = root.join("pins.yaml");
        fs::write(&path, b"old-and-considerably-longer-content").unwrap();
        write(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_successful_write_leaves_no_temp_residue() {
        let root = tmp_root("residue");
        let path = root.join("manifest.yaml");
        write(&path, b"x").unwrap();
        let leftovers: Vec<String> = fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "manifest.yaml")
            .collect();
        assert!(
            leftovers.is_empty(),
            "a successful write leaves only the target; found {leftovers:?}"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
