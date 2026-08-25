//! The gate.
//!
//! Every file under `corpus/` must parse with zero errors and round-trip byte
//! for byte. Populate the corpus with `just corpus`; only the hand-written
//! cases are committed.

use std::path::{Path, PathBuf};

use starlark_cst::{Dialect, classify, parse};

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

fn corpus_files() -> Vec<PathBuf> {
    walkdir::WalkDir::new(corpus_root())
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|p| classify(p, None).is_some())
        .collect()
}

/// Concatenating the tree's tokens must reproduce the source exactly. A parser
/// that drops a comment or a blank line fails here, which is the entire point.
#[test]
fn round_trips_byte_for_byte() {
    let mut failures = Vec::new();
    for path in corpus_files() {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let dialect = classify(&path, None).map_or(Dialect::Standard, |(d, _)| d);
        let round_tripped = parse(&src, dialect).syntax().to_string();
        if round_tripped != src {
            failures.push(path);
        }
    }
    assert!(
        failures.is_empty(),
        "{} file(s) did not round-trip: {failures:#?}",
        failures.len()
    );
}

/// Real-world Bazel files are, by construction, accepted by Bazel. Any error
/// here is a bug in this crate, not in the corpus.
#[test]
fn corpus_parses_without_errors() {
    let mut failures = Vec::new();
    for path in corpus_files() {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let dialect = classify(&path, None).map_or(Dialect::Standard, |(d, _)| d);
        let parsed = parse(&src, dialect);
        if !parsed.ok() {
            failures.push((path, parsed.errors().to_vec()));
        }
    }
    assert!(
        failures.is_empty(),
        "{} file(s) failed to parse: {failures:#?}",
        failures.len()
    );
}

/// The corpus is worthless if it is empty, and `just corpus` is easy to forget.
#[test]
fn corpus_is_populated() {
    let n = corpus_files().len();
    assert!(n >= 500, "corpus has only {n} files; run `just corpus`");
}
