//! Error-recovery gate.
//!
//! Every file in `corpus/` is valid by construction — Bazel accepted it — so
//! the corpus proves nothing about malformed input. An editor sees malformed
//! input on almost every keystroke, and refusing to build a tree for it is the
//! defect that makes `starlark_syntax` unusable for an LSP.
//!
//! So: mutate the corpus and require the same two properties. Mutations are
//! deterministic; a failure reproduces.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use starlark_cst::{Dialect, classify, parse};

/// Enough to be representative without making `just ci` slow.
const MAX_FILES: usize = 300;
const TRUNCATIONS: usize = 8;

fn corpus_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let mut files: Vec<_> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|p| classify(p, None).is_some())
        .collect();
    // WalkDir order is filesystem-dependent; sort so the sample is stable.
    files.sort();
    files.truncate(MAX_FILES);
    files
}

/// Largest index `<= i` that starts a UTF-8 character.
fn floor_boundary(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Prefixes at evenly spaced cut points: an editor mid-keystroke.
fn truncations(src: &str) -> Vec<String> {
    (1..=TRUNCATIONS)
        .map(|n| {
            let cut = floor_boundary(src, src.len() * n / (TRUNCATIONS + 1));
            src[..cut].to_string()
        })
        .collect()
}

/// Delete a structurally significant character: an unbalanced bracket, a
/// severed string, a stray colon.
fn deletions(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for needle in ['(', ')', '[', ']', '{', '}', '"', ':', ','] {
        if let Some(pos) = src.find(needle) {
            let mut s = String::with_capacity(src.len() - 1);
            s.push_str(&src[..pos]);
            s.push_str(&src[pos + needle.len_utf8()..]);
            out.push(s);
        }
    }
    out
}

fn mutants(src: &str) -> Vec<String> {
    let mut m = truncations(src);
    m.extend(deletions(src));
    m
}

/// Run `f`, returning the panic message if it panicked. Silences the default
/// hook so a failing run reports a list rather than thousands of backtraces.
fn catch<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = catch_unwind(AssertUnwindSafe(f));
    std::panic::set_hook(prev);
    result.map_err(|e| {
        e.downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| e.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic>".to_string())
    })
}

/// A parser that panics on malformed input takes the editor down with it.
#[test]
fn never_panics_on_malformed_input() {
    let mut failures = Vec::new();
    for path in corpus_files() {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let dialect = classify(&path, None).map_or(Dialect::Standard, |(d, _)| d);
        for (i, mutant) in mutants(&src).into_iter().enumerate() {
            if let Err(msg) = catch(|| parse(&mutant, dialect)) {
                failures.push(format!("{} mutant {i}: {msg}", path.display()));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} panic(s), first 20:\n{}",
        failures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Losslessness is not conditional on the input being valid. Whatever the
/// parser could not understand still has to be in the tree.
#[test]
fn round_trips_malformed_input() {
    let mut failures = Vec::new();
    for path in corpus_files() {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let dialect = classify(&path, None).map_or(Dialect::Standard, |(d, _)| d);
        for (i, mutant) in mutants(&src).into_iter().enumerate() {
            let Ok(parsed) = catch(|| parse(&mutant, dialect)) else {
                continue; // reported by never_panics_on_malformed_input
            };
            if parsed.syntax().to_string() != mutant {
                failures.push(format!("{} mutant {i}", path.display()));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} mutant(s) did not round-trip, first 20:\n{}",
        failures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Recovery must be more than "wrap the whole file in one ERROR node". A
/// single deleted bracket should not cost the entire file's structure.
#[test]
fn recovery_is_local() {
    let src = "\
cc_library(
    name = \"a\",
    srcs = [\"a.cc\"],
)

cc_library(
    name = \"b\",
    srcs = [\"b.cc\"],
)
";
    // Sever the first call's argument list.
    let broken = src.replacen("name = \"a\",", "name = ,", 1);

    let parsed = parse(&broken, Dialect::Bazel);
    assert_eq!(parsed.syntax().to_string(), broken, "must still round-trip");

    // The second, untouched rule has to survive as a call, not be swallowed.
    let calls = parsed
        .syntax()
        .descendants()
        .filter(|n| n.kind() == starlark_cst::SyntaxKind::CALL_EXPR)
        .count();
    assert!(
        calls >= 2,
        "expected both calls to survive recovery, found {calls}"
    );
}
