//! Keeps the four layers apart now that they share one crate (SPEC 3): each layer may use only the layers above it.

use std::{fs, path::Path};

/// Each layer and the layers it must not use.
const RULES: &[(&str, &[&str])] = &[
    ("domain", &["usecases", "adapters", "web", "ui"]),
    ("usecases", &["adapters", "web", "ui"]),
    ("adapters", &["web", "ui"]),
    // The web app runs in the browser too: it reaches the server only through the API.
    ("ui", &["usecases", "adapters", "web"]),
];

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn each_layer_uses_only_the_layers_above_it() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut broken = Vec::new();
    for (layer, banned) in RULES {
        let mut files = Vec::new();
        rust_files(&src.join(layer), &mut files);
        for file in files {
            let text = fs::read_to_string(&file).unwrap();
            for other in *banned {
                if text.contains(&format!("crate::{other}")) {
                    broken.push(format!("{} uses {other}", file.strip_prefix(&src).unwrap().display()));
                }
            }
        }
    }
    assert!(broken.is_empty(), "layer rule broken:\n{}", broken.join("\n"));
}
