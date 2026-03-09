//! Fixture-based tests: for each `*.nrql` in tests/fixtures, parse and compare to matching `*.json` expected AST.

use nom_nrql::{parse_nrql, Query};
use std::fs;
use std::path::Path;

fn fixture_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn run_all_nrql_fixtures() {
    let root = fixture_dir();
    if !root.exists() {
        panic!("Fixture root {} does not exist", root.display());
    }
    walk_dir(&root);
}

fn walk_dir(dir: &Path) {
    for entry in fs::read_dir(dir).expect("read_dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().map(|n| n == "errors").unwrap_or(false) {
                continue;
            }
            walk_dir(&path);
            continue;
        }
        if path.extension().map(|e| e == "nrql").unwrap_or(false) {
            run_fixture(&path);
        }
    }
}

fn run_fixture(nrql_path: &Path) {
    let nrql = fs::read_to_string(nrql_path).expect("read nrql");
    let nrql = nrql.trim_end();

    let json_path = nrql_path.with_extension("json");
    if !json_path.exists() {
        panic!(
            "No expected JSON for {} at {}",
            nrql_path.display(),
            json_path.display()
        );
    }

    let parsed = parse_nrql(nrql).unwrap_or_else(|e| {
        panic!("Parse failed for {}: {}", nrql_path.display(), e);
    });

    let json_str = fs::read_to_string(&json_path).expect("read json");
    let expected: Query = serde_json::from_str(&json_str).unwrap_or_else(|e| {
        panic!(
            "Invalid expected JSON for {}: {}",
            json_path.display(),
            e
        );
    });

    assert_eq!(
        parsed, expected,
        "Fixture {}: parsed AST != expected JSON",
        nrql_path.display()
    );
}

/// Error fixtures: *.nrql with optional *.err (expected error substring) or expect parse to fail.
#[test]
fn run_error_fixtures() {
    let errors_dir = fixture_dir();
    let errors_dir = errors_dir.join("errors");
    if !errors_dir.exists() {
        return;
    }
    for entry in fs::read_dir(&errors_dir).expect("read_dir errors") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.is_file() && path.extension().map(|e| e == "nrql").unwrap_or(false) {
            let nrql = fs::read_to_string(&path).expect("read nrql");
            let nrql = nrql.trim_end();
            let err = parse_nrql(nrql);
            assert!(err.is_err(), "Expected parse error for {}: got Ok(_)", path.display());
            let err_path = path.with_extension("err");
            if err_path.exists() {
                let expected_sub = fs::read_to_string(&err_path).expect("read err").trim().to_string();
                let got = err.unwrap_err();
                assert!(
                    got.message.contains(&expected_sub),
                    "Error fixture {}: expected message containing {:?}, got: {}",
                    path.display(),
                    expected_sub,
                    got.message
                );
            }
        }
    }
}
