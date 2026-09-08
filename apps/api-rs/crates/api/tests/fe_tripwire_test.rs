//! FE-evidence tripwire: every `fe_evidence` entry in parity-inventory.json
//! must point to a real service file that still defines the method and a URL
//! matching the endpoint path. If the FE drops a caller, CI fails loudly.
mod common;

use common::{endpoints, fe_urls_in_file, load_inventory, repo_root, segments_match, wildcard_segments};
use std::path::PathBuf;

#[test]
fn fe_evidence_files_exist() {
    let inv = load_inventory();
    let mut checked = 0;
    for (_domain, ep) in endpoints(&inv) {
        let Some(ev) = ep["fe_evidence"].as_array() else { continue };
        for e in ev {
            let file: PathBuf = repo_root().join(e["service"].as_str().expect("service string"));
            assert!(file.exists(), "FE service file missing: {}", file.display());
            checked += 1;
        }
    }
    assert!(checked >= 4, "expected seeded FE evidence entries, got {checked}");
}

#[test]
fn fe_evidence_methods_exist() {
    let inv = load_inventory();
    for (_domain, ep) in endpoints(&inv) {
        let Some(ev) = ep["fe_evidence"].as_array() else { continue };
        for e in ev {
            let file: PathBuf = repo_root().join(e["service"].as_str().expect("service string"));
            let src = std::fs::read_to_string(&file).unwrap();
            let method = e["method"].as_str().expect("method string");
            assert!(src.contains(method), "{}: method {method} not found", file.display());
        }
    }
}

#[test]
fn fe_evidence_urls_match_inventory() {
    let inv = load_inventory();
    let mut failures = Vec::new();
    for (_domain, ep) in endpoints(&inv) {
        let Some(ev) = ep["fe_evidence"].as_array() else { continue };
        let matrix_path = ep["path"].as_str().expect("path string");
        let matrix_segs = wildcard_segments(matrix_path, false);
        for e in ev {
            let file: PathBuf = repo_root().join(e["service"].as_str().expect("service string"));
            let urls = fe_urls_in_file(&file);
            let matched = urls.iter().any(|u| {
                let fe_segs = wildcard_segments(u, true);
                segments_match(&fe_segs, &matrix_segs)
            });
            if !matched {
                failures.push(format!(
                    "{} ({}): no URL matches {}",
                    file.display(),
                    e["method"].as_str().expect("method string"),
                    matrix_path
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "FE evidence no longer matches inventory:\n{}",
        failures.join("\n")
    );
}
