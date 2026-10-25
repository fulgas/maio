//! No `unwrap()` outside tests: a failure is handled or reported, and a poisoned lock is taken back, not a panic.
//! Tests and the in-memory fakes (`testing.rs`) may unwrap: there a panic is the failing test.

use std::{fs, path::Path};

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") && path.file_name().is_some_and(|n| n != "testing.rs") {
            out.push(path);
        }
    }
}

#[test]
fn nothing_outside_tests_unwraps() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut found = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        let code = text.split("#[cfg(test)]").next().unwrap_or_default();
        for (n, line) in code.lines().enumerate() {
            if line.contains(".unwrap()") && !line.trim_start().starts_with("//") {
                found.push(format!("{}:{}", file.strip_prefix(&src).unwrap().display(), n + 1));
            }
        }
    }
    assert!(found.is_empty(), "unwrap() outside tests: {found:?}");
}
