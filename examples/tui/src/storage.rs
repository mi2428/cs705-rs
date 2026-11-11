//! Private, no-clobber artifacts. Original input files are never overwritten.

use crate::{Result, model::Snapshot};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn load(path: &Path) -> Result<Snapshot> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(nix::fcntl::OFlag::O_NONBLOCK.bits());
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err("input must be a regular ICF file".into());
    }
    let mut bytes = Vec::new();
    file.take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("ICF exceeds 32 MiB".into());
    }
    Snapshot::from_file(&bytes)
}

pub fn artifact(directory: &Path, label: &str, bytes: &[u8]) -> Result<PathBuf> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(directory)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = directory.join(format!("{stamp}-{}-{label}.icf", std::process::id()));
    let temporary = path.with_extension("tmp");
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    let result = (|| -> std::io::Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        // rename() can replace an existing destination. A same-directory hard
        // link publishes complete bytes atomically and fails rather than clobbers.
        fs::hard_link(&temporary, &path)?;
        Ok(())
    })();
    let cleanup = fs::remove_file(&temporary);
    result?;
    cleanup?;
    #[cfg(unix)]
    fs::File::open(directory)?.sync_all()?;
    Ok(path)
}

pub fn save(
    source: &Path,
    original: &Snapshot,
    candidate: &Snapshot,
    directory: &Path,
) -> Result<(Snapshot, PathBuf, PathBuf)> {
    let fresh = load(source)?;
    if original.file_bytes != fresh.file_bytes {
        return Err("input file changed since load; reload before saving".into());
    }
    let backup = artifact(
        directory,
        "before",
        fresh
            .file_bytes
            .as_deref()
            .ok_or("missing original file bytes")?,
    )?;
    let bytes = candidate.icf()?;
    let snapshot = Snapshot::from_file(&bytes)?;
    let output = artifact(directory, "edited", &bytes)?;
    Ok((snapshot, output, backup))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Editor, tests::snapshot};

    pub(crate) struct Directory(pub PathBuf);
    impl Directory {
        pub fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let p = std::env::temp_dir().join(format!("cs705-tui-{}-{stamp}", std::process::id()));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn file_save_has_real_backup_no_overwrite_and_stale_input_guard() {
        let directory = Directory::new();
        let input = directory.0.join("input.icf");
        let bytes = snapshot(5).icf().unwrap();
        fs::write(&input, &bytes).unwrap();
        let original = load(&input).unwrap();
        let mut editor = Editor::new(original.clone()).unwrap();
        editor.edit(0, 0, "New", "80").unwrap();
        let candidate = editor.candidate().unwrap();
        let (saved, output, backup) = save(&input, &original, &candidate, &directory.0).unwrap();
        assert_eq!(fs::read(&input).unwrap(), bytes);
        assert_eq!(fs::read(backup).unwrap(), bytes);
        assert_ne!(input, output);
        assert!(saved.same_memory(&candidate));
        assert!(load(&output).unwrap().same_memory(&candidate));
        fs::write(&input, b"changed").unwrap();
        assert!(save(&input, &original, &candidate, &directory.0).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o600
            );
            let fifo = directory.0.join("fifo");
            nix::unistd::mkfifo(&fifo, nix::sys::stat::Mode::S_IRUSR).unwrap();
            assert!(load(&fifo).is_err());
        }
    }

    #[test]
    fn oversized_nonregular_and_failed_output_never_replace_input() {
        let directory = Directory::new();
        assert!(load(&directory.0).is_err());
        let oversized = directory.0.join("large.icf");
        fs::File::create(&oversized)
            .unwrap()
            .set_len(32 * 1024 * 1024 + 1)
            .unwrap();
        assert!(load(&oversized).is_err());
        let input = directory.0.join("input.icf");
        let bytes = snapshot(6).icf().unwrap();
        fs::write(&input, &bytes).unwrap();
        let original = load(&input).unwrap();
        let mut editor = Editor::new(original.clone()).unwrap();
        editor.edit(0, 0, "Draft", "80").unwrap();
        let not_directory = directory.0.join("not-a-directory");
        fs::write(&not_directory, b"keep").unwrap();
        assert!(
            save(
                &input,
                &original,
                &editor.candidate().unwrap(),
                &not_directory
            )
            .is_err()
        );
        assert_eq!(fs::read(&not_directory).unwrap(), b"keep");
        assert_eq!(fs::read(&input).unwrap(), bytes);
        assert!(editor.dirty);
    }
}
