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

use starlark_cst::{Dialect, SyntaxKind, classify, parse};

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
                continue;
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

/// An over-indented line must be reported, and must not end the block it sits
/// in. Swallowing the stray `INDENT` pairs the following `DEDENT` with the
/// outer block, which silently drops the rest of the body to the top level —
/// a wrong tree with no diagnostic to hint at it.
#[test]
fn over_indentation_is_reported_and_contained() {
    let src = "def f():\n    a = 1\n        b = 2\n    c = 3\n";
    let parsed = parse(src, Dialect::Bazel);

    assert_eq!(parsed.syntax().to_string(), src);
    assert!(
        !parsed.ok(),
        "inconsistent indentation must produce a diagnostic"
    );

    let root = parsed.syntax();
    let top: Vec<_> = root.children().map(|n| n.kind()).collect();
    assert_eq!(
        top,
        vec![starlark_cst::SyntaxKind::DEF_STMT],
        "`c = 3` belongs to the function body, not the file"
    );

    let def = root.first_child().unwrap();
    assert_eq!(
        usize::from(def.text_range().end()),
        src.len(),
        "the def must span the whole body"
    );
}

/// Deeply nested input must not exhaust the stack.
///
/// This one cannot be expressed as an assertion: a stack overflow aborts the
/// process instead of unwinding, so `catch_unwind` does not see it and the test
/// binary dies outright. Reaching the end of this function is the assertion.
#[test]
fn deep_nesting_does_not_overflow() {
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        for n in [1_000usize, 100_000] {
            let src = format!(
                "x = {}1{}\n",
                open.to_string().repeat(n),
                close.to_string().repeat(n)
            );
            let parsed = parse(&src, Dialect::Bazel);
            assert_eq!(
                parsed.syntax().to_string(),
                src,
                "{open} x{n} must round-trip"
            );
        }
        let src = format!("x = {}1\n", open.to_string().repeat(50_000));
        let parsed = parse(&src, Dialect::Bazel);
        assert_eq!(
            parsed.syntax().to_string(),
            src,
            "unclosed {open} must round-trip"
        );
    }

    let mut src = String::new();
    for i in 0..2_000 {
        src.push_str(&"    ".repeat(i));
        src.push_str("if x:\n");
    }
    src.push_str(&"    ".repeat(2_000));
    src.push_str("pass\n");
    assert_eq!(parse(&src, Dialect::Bazel).syntax().to_string(), src);
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
    let broken = src.replacen("name = \"a\",", "name = ,", 1);

    let parsed = parse(&broken, Dialect::Bazel);
    assert_eq!(parsed.syntax().to_string(), broken, "must still round-trip");

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

/// An empty slot in an argument list is a syntax error, as it already is in a
/// list or a dict. Recovery keeps the comma in the tree, so the round trip
/// holds, and produces no `ARG` — a node spanning nothing is one a consumer
/// cannot report a position for.
#[test]
fn an_empty_argument_slot_is_rejected() {
    for src in ["foo(a, , b)\n", "foo(, a)\n", "foo(,)\n"] {
        let parsed = parse(src, Dialect::Bazel);
        assert_eq!(parsed.syntax().to_string(), src, "round trip for {src:?}");

        let errors = parsed.errors();
        assert_eq!(errors.len(), 1, "{src:?} produced {errors:?}");
        assert_eq!(errors[0].message, "expected an expression");
        assert_eq!(
            &src[errors[0].range.start().into()..errors[0].range.end().into()],
            ",",
            "the error anchors on the comma that opens the empty slot"
        );

        assert!(
            !parsed
                .syntax()
                .descendants()
                .any(|node| node.kind() == SyntaxKind::ARG && node.text_range().is_empty()),
            "{src:?} built an ARG spanning nothing"
        );
    }

    // A single trailing comma before `)` stays legal.
    let trailing = parse("foo(a,)\n", Dialect::Bazel);
    assert!(trailing.errors().is_empty(), "{:?}", trailing.errors());
}

/// A diagnostic with an empty range has no characters to underline, and
/// clients disagree about what to draw for one. An unclosed delimiter is the
/// state a build file spends most of its editing life in, so this is the
/// commonest diagnostic the crate produces.
#[test]
fn errors_at_end_of_input_have_somewhere_to_point() {
    for src in [
        "foo(\n",
        "foo(a\n",
        "x = [\n",
        "x = {\n",
        "def f(:\n    pass\n",
    ] {
        let parsed = parse(src, Dialect::Bazel);
        assert_eq!(parsed.syntax().to_string(), src, "round trip for {src:?}");
        for error in parsed.errors() {
            assert!(
                error.range.start() < error.range.end(),
                "empty range for {src:?}: {error:?}"
            );
        }
    }

    // An unclosed delimiter anchors on the delimiter, not on where the parser
    // noticed: it points at the `(` that needs closing.
    for (src, opener) in [
        ("foo(\n", "("),
        ("foo(a\n", "("),
        ("x = [\n", "["),
        ("x = {\n", "{"),
        ("def f(:\n    pass\n", "("),
    ] {
        let parsed = parse(src, Dialect::Bazel);
        let closing = parsed
            .errors()
            .iter()
            .find(|error| {
                error.message.starts_with("expected `)`")
                    || error.message.starts_with("expected `]`")
                    || error.message.starts_with("expected `}`")
            })
            .unwrap_or_else(|| panic!("{src:?} reported no unclosed delimiter"));
        assert_eq!(
            &src[closing.range.start().into()..closing.range.end().into()],
            opener,
            "{src:?} anchored {closing:?} somewhere other than the opener"
        );
    }
}

/// A lexical error is a fact about the bytes that no later pass can recover:
/// a truncated literal is an ordinary `STRING` token by the time the parser
/// sees it. Reporting has to happen in the lexer, and reporting it must not
/// disturb the token stream — the tree an editor gets for `load("@rules_`
/// mid-keystroke has to be the same shape as the closed one.
#[test]
fn lexical_errors_are_reported() {
    let cases = [
        ("x = \"hello\ny = 1\n", "unclosed string literal"),
        ("x = \"hello", "unclosed string literal"),
        ("x = \"\"\"hello\n", "unclosed string literal"),
        ("x = r'ab\n", "unclosed string literal"),
        ("def f():\n\tpass\n", "tab characters are not allowed"),
        ("x = ?\n", "invalid character"),
    ];
    for (src, expected) in cases {
        let parsed = parse(src, Dialect::Bazel);
        assert_eq!(parsed.syntax().to_string(), src, "{src:?} must round-trip");
        assert!(
            parsed.errors().iter().any(|e| e.message.contains(expected)),
            "{src:?} reported {:?}, wanted {expected:?}",
            parsed.errors()
        );
    }
}

/// The cases next door to each lexical error, which must stay silent.
#[test]
fn well_formed_input_stays_silent() {
    for src in [
        "x = \"hello\"\n",
        "x = \"\"\n",
        "x = \"\"\"a\"\"\"\n",
        "x = r'ab'\n",
        // A tab is only wrong as indentation. Bazel accepts it elsewhere, and
        // whitespace on a blank line indents nothing.
        "x =\t1\n",
        "x = [\n\t1,\n]\n",
        "def f():\n    pass\n\t\n",
    ] {
        let parsed = parse(src, Dialect::Bazel);
        assert!(
            parsed.errors().is_empty(),
            "{src:?} reported {:?}",
            parsed.errors()
        );
    }
}

/// One bad byte is one diagnostic, and the lexer's is the informative one.
#[test]
fn an_invalid_character_is_not_reported_twice() {
    let parsed = parse("x = ?\n", Dialect::Bazel);
    assert_eq!(
        parsed.errors().len(),
        1,
        "expected one diagnostic, got {:?}",
        parsed.errors()
    );
}

/// Lexical and syntactic errors arrive as one list in source order, so a
/// consumer publishing diagnostics does not have to sort them, and
/// `errors()[0]` is the first thing wrong with the file.
#[test]
fn errors_are_in_source_order() {
    let src = "def f():\n\tx = \"oops\n\treturn ?\n";
    let parsed = parse(src, Dialect::Bazel);
    assert_eq!(parsed.syntax().to_string(), src, "must still round-trip");

    let starts: Vec<u32> = parsed
        .errors()
        .iter()
        .map(|e| e.range.start().into())
        .collect();
    let mut sorted = starts.clone();
    sorted.sort_unstable();
    assert_eq!(starts, sorted, "{:?}", parsed.errors());

    for error in parsed.errors() {
        assert!(
            !error.range.is_empty(),
            "empty range: {error:?} in {:?}",
            parsed.errors()
        );
    }
}
