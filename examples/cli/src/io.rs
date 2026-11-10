//! Bounded byte I/O and atomic publication. Payload and diagnostics never mix.
//!
//! File publication: create_new sibling -> write_all -> sync_all -> link/rename.
//! A hard link provides atomic no-clobber publication without an exists race.
//! There is no fallback to truncating the destination when publication fails.
//! Stdout is a stream: write or flush failure can leave an emitted prefix.
//! File paths come from explicit CLI selections, never from payload labels.
//! Unlike the native ICF reader's EOF merge, input acquisition rejects read
//! errors before parser entry. Accepting parser prefixes does not waive I/O errors.

use crate::{AppError, Result, args::Write};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write as _},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const INPUT_LIMIT: u64 = 32 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Stdin is one owned stream, not a repeatable file for independent inputs.
pub fn single_stdin<'a>(inputs: impl IntoIterator<Item = &'a str>) -> Result<()> {
    if inputs
        .into_iter()
        .filter(|path| *path == "-")
        .take(2)
        .count()
        > 1
    {
        return Err(AppError::invalid("stdin may be consumed by only one input"));
    }
    Ok(())
}

fn regular_file(path: &str) -> Result<fs::File> {
    let before = fs::metadata(path)?;
    if !before.is_file() {
        return Err(AppError::io("input must be a regular file or stdin"));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // A path swap to a FIFO must not wait for a peer before fstat.
        options.custom_flags(nix::fcntl::OFlag::O_NONBLOCK.bits());
    }
    let file = options.open(path)?;
    let opened = file.metadata()?;
    if !opened.is_file() {
        return Err(AppError::io("input must be a regular file or stdin"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (before.dev(), before.ino()) != (opened.dev(), opened.ino()) {
            return Err(AppError::io("input changed while acquiring the file"));
        }
    }
    Ok(file)
}

pub fn read(path: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    if path == "-" {
        std::io::stdin()
            .lock()
            .take(INPUT_LIMIT + 1)
            .read_to_end(&mut bytes)?;
    } else {
        regular_file(path)?
            .take(INPUT_LIMIT + 1)
            .read_to_end(&mut bytes)?;
    }
    if bytes.len() as u64 > INPUT_LIMIT {
        return Err(AppError::invalid("input exceeds the 32 MiB limit"));
    }
    Ok(bytes)
}

pub fn json_bytes(value: &Value) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|_| AppError::invalid("JSON serialization failed"))?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn stdout(bytes: &[u8]) -> Result<()> {
    let mut out = std::io::stdout().lock();
    out.write_all(bytes)?;
    out.flush()?;
    Ok(())
}

pub fn report(value: &Value) -> Result<()> {
    let mut out = std::io::stderr().lock();
    out.write_all(&json_bytes(value)?)?;
    out.flush()?;
    Ok(())
}

/// Encode arbitrary stored bytes without UTF-8 replacement or terminal escapes.
pub fn bytes_value(bytes: &[u8]) -> Value {
    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    let text = std::str::from_utf8(bytes).ok();
    json!({"hex": hex, "utf8": text})
}

pub fn decode_hex(text: &str) -> Result<Vec<u8>> {
    if !text.is_ascii() || !text.len().is_multiple_of(2) {
        return Err(AppError::invalid(
            "hex value requires an even number of ASCII digits",
        ));
    }
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| {
            let a = (p[0] as char).to_digit(16);
            let b = (p[1] as char).to_digit(16);
            a.zip(b)
                .map(|(a, b)| (a * 16 + b) as u8)
                .ok_or_else(|| AppError::invalid("invalid hexadecimal digit"))
        })
        .collect()
}

pub fn publish(bytes: &[u8], write: &Write) -> Result<()> {
    if write.dry_run {
        return report(&json!({"dry_run": true, "output_bytes": bytes.len(), "published": false}));
    }
    let output = write
        .output
        .as_deref()
        .ok_or_else(|| AppError::invalid("output is required"))?;
    if output == "-" {
        return stdout(bytes);
    }
    atomic(Path::new(output), bytes, write.overwrite)
}

fn destination_permissions(path: &Path, overwrite: bool) -> Result<Option<fs::Permissions>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !overwrite {
                return Err(AppError::io("output exists; use --overwrite to replace it"));
            }
            if !metadata.is_file() {
                return Err(AppError::io(
                    "output replacement requires a regular file, not a symlink",
                ));
            }
            Ok(Some(metadata.permissions()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Catch known output conflicts before an explicit device operation. Publication
/// repeats these checks and still uses atomic no-clobber; preflight is not a lock.
pub fn preflight(write: &Write) -> Result<()> {
    if write.dry_run {
        return Ok(());
    }
    let output = write
        .output
        .as_deref()
        .ok_or_else(|| AppError::invalid("output is required"))?;
    if output == "-" {
        return Ok(());
    }
    let path = Path::new(output);
    destination_permissions(path, write.overwrite)?;
    if path.file_name().is_none() {
        return Err(AppError::io("output needs a file name"));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !fs::metadata(parent)?.is_dir() {
        return Err(AppError::io("output parent must be a directory"));
    }
    Ok(())
}

fn atomic(path: &Path, bytes: &[u8], overwrite: bool) -> Result<()> {
    let existing = destination_permissions(path, overwrite)?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if path.file_name().is_none() {
        return Err(AppError::io("output needs a file name"));
    }
    // A valid destination may already use the filesystem's full name budget.
    // The sibling name stays bounded independently of the destination name.
    let temporary_path = parent.join(format!(
        ".cs705-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary_path)?;
    // Ownership begins only after create_new succeeds. A colliding file belongs
    // to somebody else and must not be removed by an error-path guard.
    let temporary = Temporary(temporary_path);
    file.write_all(bytes)?;
    if let Some(permissions) = existing {
        file.set_permissions(permissions)?;
    }
    file.sync_all()?;
    drop(file);
    if overwrite {
        fs::rename(&temporary.0, path)?;
    } else {
        fs::hard_link(&temporary.0, path)?;
    }
    Ok(())
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_stdin_consumers_are_rejected_as_one_set() {
        assert!(single_stdin(["a", "-", "b"]).is_ok());
        assert!(single_stdin(["-", "-"]).is_err());
        assert!(single_stdin(["a", "-", "b", "-"]).is_err());
        assert!(single_stdin(["-", "-", "-"]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn input_acquisition_keeps_nonblocking_until_fd_type_validation() {
        use std::os::fd::AsRawFd;
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output");
        fs::create_dir_all(&parent).unwrap();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = parent.join(format!("input-{}-{unique}", std::process::id()));
        let mut owned = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        let cleanup = Temporary(path);
        owned.write_all(b"regular input").unwrap();
        drop(owned);
        let file = regular_file(cleanup.0.to_str().unwrap()).unwrap();
        let flags = nix::fcntl::fcntl(file.as_raw_fd(), nix::fcntl::FcntlArg::F_GETFL).unwrap();
        assert_ne!(flags & nix::fcntl::OFlag::O_NONBLOCK.bits(), 0);
        assert_eq!(read(cleanup.0.to_str().unwrap()).unwrap(), b"regular input");
    }

    #[test]
    fn colliding_temporary_file_is_not_owned_or_deleted() {
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-output");
        fs::create_dir_all(&parent).unwrap();
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = parent.join(format!("collision-{}-{unique}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let _cleanup = Cleanup(root.clone());
        let output = root.join("output");
        let sequence = NEXT.load(Ordering::Relaxed);
        let collision = root.join(format!(".cs705-{}-{sequence}.tmp", std::process::id()));
        fs::write(&collision, b"unrelated existing file").unwrap();
        assert!(atomic(&output, b"new payload", false).is_err());
        assert_eq!(fs::read(&collision).unwrap(), b"unrelated existing file");
        assert!(!output.exists());
    }
}
