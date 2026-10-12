//! Immutable compiled content. Publication commits a complete directory before
//! updating the authoring certificate. Runtime recovery never resolves imports
//! or depends on authoring files inside that directory.
use super::*;
use std::io::Write;
use std::path::PathBuf;

pub(crate) const MAX_PACKAGE_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ARTIFACT_BYTES: u64 = 128 * 1024 * 1024;

pub(crate) fn relative_root(package: &Package) -> String {
    let identity = digest(
        format!(
            "tor-compiled-artifact-v1\0{}\0{}\0{}",
            package.certificate.model_hash,
            package.certificate.content_hash,
            package.certificate.validator
        )
        .as_bytes(),
    );
    format!(".compiled/{identity}")
}

fn io(error: std::io::Error) -> Failure {
    fail(format!("Compiled artifact: {error}"))
}

fn safe_directory(root: &Path, relative: &str) -> Result<PathBuf, Failure> {
    let canonical = root.canonicalize().map_err(io)?;
    let mut path = canonical.clone();
    for part in Path::new(relative).components() {
        let Component::Normal(part) = part else {
            return Err(fail("Invalid artifact directory"));
        };
        path.push(part);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => require(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "Artifact directory must be an ordinary directory",
            )?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir(&path).map_err(io)?
            }
            Err(error) => return Err(io(error)),
        }
    }
    require(
        path.canonicalize().map_err(io)?.starts_with(&canonical),
        "Artifact path escapes package",
    )?;
    Ok(path)
}

fn ordinary_target(path: &Path) -> Result<(), Failure> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => require(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Validation output must be an ordinary file, not a symlink",
        )?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io)?;
    file.write_all(bytes).map_err(io)?;
    file.sync_all().map_err(io)
}

fn replace_file(root: &Path, name: &str, bytes: &[u8]) -> Result<(), Failure> {
    let destination = root.join(name);
    ordinary_target(&destination)?;
    let temporary = root.join(format!(".tor-{}-{}.tmp", name, uuid::Uuid::new_v4()));
    write_new(&temporary, bytes)?;
    let result = std::fs::rename(&temporary, &destination).map_err(io);
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

/// Preflight *both* outputs before touching either. A certificate is the final
/// admission pointer; partial publication of frontend metadata fails closed.
pub(crate) fn publish(root: &Path, package: &Package) -> Result<(), Failure> {
    ordinary_target(&root.join("index.json"))?;
    ordinary_target(&root.join("validation.json"))?;
    let mut package = package.clone();
    package.validated = true;
    package.selected = package.manifest.default_character;
    let package = &package;
    let parent = safe_directory(root, ".compiled")?;
    let destination = root.join(relative_root(package));
    let staging = parent.join(format!(".pending-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&staging).map_err(io)?;
    let result = (|| {
        std::fs::create_dir(staging.join("regions")).map_err(io)?;
        let metadata = crate::storage::schema::scenario::encode_content_package(package)?;
        let mut bytes = metadata.len() as u64;
        require(
            bytes <= MAX_ARTIFACT_BYTES,
            "Compiled artifact exceeds aggregate byte limit",
        )?;
        write_new(&staging.join("package.json"), &metadata)?;
        for entry in &package.index.regions {
            let content = package.compiled_region(entry.id)?;
            bytes = bytes
                .checked_add(content.len() as u64)
                .ok_or_else(|| fail("Compiled artifact size overflow"))?;
            require(
                bytes <= MAX_ARTIFACT_BYTES,
                "Compiled artifact exceeds aggregate byte limit",
            )?;
            write_new(
                &staging.join("regions").join(format!("{}.json", entry.id)),
                content.as_bytes(),
            )?;
        }
        match std::fs::symlink_metadata(&destination) {
            Ok(metadata) => {
                require(
                    metadata.is_dir() && !metadata.file_type().is_symlink(),
                    "Compiled artifact target must be an ordinary directory",
                )?;
                // Revalidation may reuse an immutable exact artifact. Verify
                // every member before discarding the new generation.
                let existing = load(&destination)?;
                require(
                    existing.certificate == package.certificate,
                    "Compiled artifact identity collision",
                )?;
                for entry in &package.index.regions {
                    existing.compiled_region(entry.id)?;
                }
                std::fs::remove_dir_all(&staging).map_err(io)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::rename(&staging, &destination).map_err(io)?
            }
            Err(error) => return Err(io(error)),
        }
        replace_file(root, "index.json", &package.index.to_bytes()?)?;
        replace_file(
            root,
            "validation.json",
            &serde_json::to_vec_pretty(&package.certificate).map_err(|e| fail(e.to_string()))?,
        )?;
        Ok(())
    })();
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

pub(crate) fn load(root: &Path) -> Result<Package, Failure> {
    let bytes = read_limited(
        root,
        "package.json",
        crate::storage::schema::scenario::MAX_COMPILED_PACKAGE_BYTES,
    )?;
    let mut package = crate::storage::schema::scenario::decode_content_package(bytes.as_bytes())?;
    require(
        package.validated,
        "Compiled artifact has no validation evidence",
    )?;
    package.directory = Some(root.canonicalize().map_err(io)?.display().to_string());
    package.sources.set_compiled_directory(root);
    package.check_identity()?;
    package.validated = true;
    Ok(package)
}

pub(crate) fn locate(package: &Package, supplied: Option<&Package>) -> Option<PathBuf> {
    supplied
        .and_then(|p| p.directory.as_deref())
        .into_iter()
        .chain(package.directory.as_deref())
        .flat_map(|root| {
            let root = PathBuf::from(root);
            [root.join(relative_root(package)), root]
        })
        .find(|path| load(path).is_ok_and(|candidate| candidate.certificate == package.certificate))
}

/// Export compiled inputs, not materialized procedural geometry. Destination
/// must be new: this command never overwrites an existing authored package.
pub fn export(root: &Path, destination: &Path) -> Result<Certificate, Failure> {
    require(
        std::fs::symlink_metadata(destination)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        "Export destination already exists or cannot be inspected",
    )?;
    let scenario = super::load(root, 0, None, false)?;
    let package = scenario
        .package
        .as_ref()
        .ok_or_else(|| fail("Missing package"))?;
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    require(parent.is_dir(), "Export parent directory does not exist")?;
    let staging = parent.join(format!(".tor-export-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&staging).map_err(io)?;
    let result = (|| {
        std::fs::create_dir(staging.join("regions")).map_err(io)?;
        let metadata = crate::storage::schema::scenario::encode_content_package(package)?;
        let mut total = metadata.len() as u64;
        write_new(&staging.join("package.json"), &metadata)?;
        for entry in &package.index.regions {
            let content = package.compiled_region(entry.id)?;
            total = total
                .checked_add(content.len() as u64)
                .ok_or_else(|| fail("Export size overflow"))?;
            require(
                total <= MAX_ARTIFACT_BYTES,
                "Export exceeds aggregate byte limit",
            )?;
            write_new(
                &staging.join("regions").join(format!("{}.json", entry.id)),
                content.as_bytes(),
            )?;
        }
        std::fs::rename(&staging, destination).map_err(io)?;
        Ok(package.certificate.clone())
    })();
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_output_is_rejected_before_any_other_output_is_changed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.json"), "original").unwrap();
        std::fs::create_dir(dir.path().join("validation.json")).unwrap();
        let package = super::super::read_package(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room"),
        )
        .unwrap();
        assert!(publish(dir.path(), &package).is_err());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("index.json")).unwrap(),
            "original"
        );
        assert!(!dir.path().join(".compiled").exists());
    }
    #[cfg(unix)]
    #[test]
    fn symlink_outputs_and_artifact_directories_are_rejected() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        std::fs::write(external.path().join("sentinel"), "unchanged").unwrap();
        symlink(
            external.path().join("sentinel"),
            dir.path().join("validation.json"),
        )
        .unwrap();
        assert!(ordinary_target(&dir.path().join("validation.json")).is_err());
        symlink(external.path(), dir.path().join(".compiled")).unwrap();
        assert!(safe_directory(dir.path(), ".compiled").is_err());
        assert_eq!(
            std::fs::read_to_string(external.path().join("sentinel")).unwrap(),
            "unchanged"
        );
    }
}
