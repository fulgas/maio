//! Keeps the four layers apart now that they share one crate (arc42 5): each layer may use only the layers above it.

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

/// The web layer drives the core: only the start-up wiring may name an adapter; handlers reach adapters through use cases.
#[test]
fn only_the_wiring_names_an_adapter() {
    let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/web");
    let mut files = Vec::new();
    rust_files(&web, &mut files);
    let broken: Vec<_> = files
        .iter()
        .filter(|f| !matches!(f.file_name().and_then(|n| n.to_str()), Some("start.rs" | "config.rs")))
        .filter(|f| fs::read_to_string(f).unwrap().contains("adapters::"))
        .map(|f| f.strip_prefix(&web).unwrap().display().to_string())
        .collect();
    assert!(broken.is_empty(), "web handlers use adapters directly:\n{}", broken.join("\n"));
}
