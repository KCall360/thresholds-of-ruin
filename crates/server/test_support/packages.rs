//! Shared repository package discovery for unit and integration contracts.
use std::path::{Path, PathBuf};

pub fn all_packages() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
    let mut packages: Vec<_> = [root.clone(), root.join("tests")]
        .iter()
        .flat_map(|dir| std::fs::read_dir(dir).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("scenario.toml").is_file())
        .collect();
    packages.sort();
    packages
}

#[test]
fn discovery_covers_every_repository_manifest() {
    fn visit(path: &Path, manifests: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, manifests);
            } else if path.file_name().unwrap() == "scenario.toml" {
                manifests.push(path.parent().unwrap().to_path_buf());
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
    let mut manifests = Vec::new();
    visit(&root, &mut manifests);
    manifests.sort();
    assert!(!manifests.is_empty());
    assert_eq!(
        all_packages(),
        manifests,
        "update discovery for new package roots"
    );
}
