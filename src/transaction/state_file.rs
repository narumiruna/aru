//! Private coordination files must never follow injected links or trust foreign files.
use super::*;
use std::io::Read;

const LIMIT: u64 = 16 * 1024 * 1024;

pub(super) fn validate(path: &Path) -> Result<()> {
    match path.symlink_metadata() {
        Ok(metadata) => validate_metadata(path, &metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(AruError::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn validate_metadata(path: &Path, metadata: &std::fs::Metadata) -> Result<()> {
    let mut safe = metadata.is_file() && !metadata.file_type().is_symlink();
    #[cfg(unix)]
    {
        safe &= owned_private_file(metadata, unsafe { libc::geteuid() });
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        safe &= metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            == 0;
    }
    if !safe {
        return Err(AruError::msg(format!(
            "unsafe transaction state file {}; preserve it for manual review",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn owned_private_file(metadata: &std::fs::Metadata, uid: libc::uid_t) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.uid() == uid && metadata.nlink() == 1 && metadata.mode() & 0o022 == 0
}

fn open(path: &Path, create: bool) -> Result<File> {
    validate(path)?;
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(create)
        .create(create)
        .truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path).at(path)?;
    validate_metadata(path, &file.metadata().at(path)?)?;
    Ok(file)
}

pub(super) fn read(path: &Path) -> Result<Option<String>> {
    match path.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        result => {
            result.at(path)?;
        }
    }
    let file = open(path, false)?;
    // Bound both allocation and reads, including files that grow after stat.
    let mut text = String::new();
    file.take(LIMIT + 1).read_to_string(&mut text).at(path)?;
    if text.len() as u64 > LIMIT {
        return Err(AruError::msg("transaction state exceeds the 16 MiB limit"));
    }
    Ok(Some(text))
}

pub(super) fn write_atomic(path: &Path, body: &str) -> Result<()> {
    if body.len() as u64 > LIMIT {
        return Err(AruError::msg("transaction state exceeds the 16 MiB limit"));
    }
    let temporary = path.with_extension("toml.tmp");
    validate(path)?;
    let mut file = open(&temporary, true)?;
    file.set_len(0).at(&temporary)?;
    file.write_all(body.as_bytes()).at(&temporary)?;
    file.sync_all().at(&temporary)?;
    drop(file);
    std::fs::rename(&temporary, path).at(path)?;
    sync_parent(path)
}

#[cfg(all(test, unix))]
#[path = "state_file_tests.rs"]
mod tests;
