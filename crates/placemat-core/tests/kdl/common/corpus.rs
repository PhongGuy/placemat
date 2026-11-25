//! The vendored KDL v2 compliance corpus (kdl 6.7.1, Apache-2.0).
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kdl-compliance")
}

pub fn kdl_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "kdl"))
        .collect();
    files.sort();
    files
}

pub fn inputs() -> Vec<(String, String)> {
    kdl_files(&root().join("input"))
        .into_iter()
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read_to_string(&p).unwrap(),
            )
        })
        .collect()
}
