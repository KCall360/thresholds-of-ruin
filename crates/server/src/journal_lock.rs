//! Process-owned journal lease shared with the storage worker.
//! Descriptor copies inherited by a child must not extend this owner's lifetime.
use crate::{engine::storage_failure, Failure};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tor_protocol::ErrorCode;

#[derive(Debug)]
pub(crate) struct JournalLock {
    file: fs::File,
}

impl JournalLock {
    pub(crate) fn acquire(path: &Path) -> Result<(PathBuf, Arc<Self>), Failure> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|_| storage_failure())?;
        let canonical = match fs::canonicalize(path) {
            Ok(path) => path,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::canonicalize(parent)
                .map_err(|_| storage_failure())?
                .join(path.file_name().ok_or_else(storage_failure)?),
            Err(_) => return Err(storage_failure()),
        };
        let mut lock_name = canonical
            .file_name()
            .ok_or_else(storage_failure)?
            .to_os_string();
        lock_name.push(".lock");
        let file = fs::File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(canonical.with_file_name(lock_name))
            .map_err(|_| storage_failure())?;
        file.try_lock().map_err(|_| {
            Failure::new(
                ErrorCode::StorageFailure,
                "Game journal is already in use or cannot be locked",
            )
        })?;
        Ok((canonical, Arc::new(Self { file })))
    }

    #[cfg(test)]
    pub(crate) fn duplicate_descriptor(&self) -> std::io::Result<fs::File> {
        self.file.try_clone()
    }
}

impl Drop for JournalLock {
    fn drop(&mut self) {
        // Close alone may leave a lock held by a descriptor copied during fork.
        // Release it at the final Rust owner's boundary; the file still closes
        // if the explicit unlock fails.
        let _ = self.file.unlock();
    }
}
