//! The lexer's two gates: every operator lexes back to itself (maximal
//! munch), and every corpus file is covered byte for byte.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use expect_test::expect;
use starlark_cst::{Dialect, Lexeme, SyntaxKind, classify, tokenize};

/// Every operator token, paired with its spelling. Order mirrors
/// `SyntaxKind`; if a variant is added there, `operator_table_is_exhaustive`
/// fails until it is added here.
const OPERATORS: &[(SyntaxKind, &str)] = {
    use SyntaxKind as K;
    &[
        (K::PLUS, "+"),
        (K::MINUS, "-"),
        (K::STAR, "*"),
        (K::DOUBLE_STAR, "**"),
        (K::SLASH, "/"),
        (K::DOUBLE_SLASH, "//"),
        (K::PERCENT, "%"),
        (K::AMP, "&"),
        (K::PIPE, "|"),
        (K::CARET, "^"),
        (K::TILDE, "~"),
        (K::LT, "<"),
        (K::GT, ">"),
        (K::LE, "<="),
        (K::GE, ">="),
        (K::EQ, "=="),
        (K::NE, "!="),
        (K::SHL, "<<"),
        (K::SHR, ">>"),
        (K::ASSIGN, "="),
        (K::PLUS_ASSIGN, "+="),
        (K::MINUS_ASSIGN, "-="),
        (K::STAR_ASSIGN, "*="),
        (K::SLASH_ASSIGN, "/="),
        (K::DOUBLE_SLASH_ASSIGN, "//="),
        (K::PERCENT_ASSIGN, "%="),
        (K::AMP_ASSIGN, "&="),
        (K::PIPE_ASSIGN, "|="),
        (K::CARET_ASSIGN, "^="),
        (K::SHL_ASSIGN, "<<="),
        (K::SHR_ASSIGN, ">>="),
        (K::DOT, "."),
        (K::COMMA, ","),
        (K::SEMI, ";"),
        (K::COLON, ":"),
        (K::ARROW, "->"),
        (K::ELLIPSIS, "..."),
        (K::L_PAREN, "("),
        (K::R_PAREN, ")"),
        (K::L_BRACKET, "["),
        (K::R_BRACKET, "]"),
        (K::L_BRACE, "{"),
        (K::R_BRACE, "}"),
    ]
};

#[test]
fn operator_table_is_exhaustive() {
    let mut expected: Vec<SyntaxKind> = Vec::new();
    let mut k = SyntaxKind::PLUS as u16;
    while k <= SyntaxKind::R_BRACE as u16 {
        expected.push(
            *OPERATORS
                .iter()
                .map(|(kind, _)| kind)
                .find(|kind| **kind as u16 == k)
                .unwrap_or_else(|| panic!("operator kind {k} missing from OPERATORS")),
        );
        k += 1;
    }
    assert_eq!(expected.len(), OPERATORS.len());
}

fn significant(tokens: &[Lexeme]) -> Vec<Lexeme> {
    tokens
        .iter()
        .copied()
        .filter(|t| {
            !t.kind.is_trivia()
                && !matches!(
                    t.kind,
                    SyntaxKind::INDENT | SyntaxKind::DEDENT | SyntaxKind::EOF
                )
        })
        .collect()
}

/// Every operator, lexed alone, comes back as exactly one token of its own
/// kind spanning its whole spelling. `//=` as `/`+`/`+`=` fails here.
#[test]
fn every_operator_lexes_to_itself() {
    for &(kind, text) in OPERATORS {
        let tokens = significant(&tokenize(text, Dialect::Bazel));
        assert_eq!(tokens.len(), 1, "{text:?} lexed as {tokens:?}");
        assert_eq!(tokens[0].kind, kind, "{text:?} lexed as {tokens:?}");
        assert_eq!(tokens[0].len as usize, text.len(), "{text:?}");
    }
}

/// Every ordered pair of operators, separated by a space, lexes back to that
/// pair. This is maximal munch exercised exhaustively: `/ /` must stay two
/// slashes while `//` is one token, for all 43×43 combinations.
#[test]
fn every_operator_pair_lexes_to_the_pair() {
    for &(a_kind, a) in OPERATORS {
        for &(b_kind, b) in OPERATORS {
            let src = format!("{a} {b}");
            let tokens = significant(&tokenize(&src, Dialect::Bazel));
            let kinds: Vec<SyntaxKind> = tokens.iter().map(|t| t.kind).collect();
            assert_eq!(kinds, [a_kind, b_kind], "{src:?} lexed as {tokens:?}");
        }
    }
}

/// The known failure mode, spelled out: the `/` family adjacent with no
/// space, where a greedy-but-wrong lexer splits or over-merges.
#[test]
fn maximal_munch_slash_family() {
    use SyntaxKind as K;
    let cases: &[(&str, &[SyntaxKind])] = &[
        ("/", &[K::SLASH]),
        ("//", &[K::DOUBLE_SLASH]),
        ("/=", &[K::SLASH_ASSIGN]),
        ("//=", &[K::DOUBLE_SLASH_ASSIGN]),
        ("///", &[K::DOUBLE_SLASH, K::SLASH]),
        ("///=", &[K::DOUBLE_SLASH, K::SLASH_ASSIGN]),
        ("////", &[K::DOUBLE_SLASH, K::DOUBLE_SLASH]),
        ("//==", &[K::DOUBLE_SLASH_ASSIGN, K::ASSIGN]),
        ("/==", &[K::SLASH_ASSIGN, K::ASSIGN]),
        ("//=/", &[K::DOUBLE_SLASH_ASSIGN, K::SLASH]),
    ];
    for &(src, expected) in cases {
        let kinds: Vec<SyntaxKind> = significant(&tokenize(src, Dialect::Bazel))
            .iter()
            .map(|t| t.kind)
            .collect();
        assert_eq!(kinds, expected, "{src:?}");
    }
}

fn corpus_files() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .map(walkdir::DirEntry::into_path)
        .filter(|p| classify(p, None).is_some())
        .collect()
}

/// The coverage invariant: for every corpus file, the token lengths sum to
/// the source length, the final token is a zero-width `EOF`, and every
/// `INDENT`/`DEDENT` is zero-width.
#[test]
fn corpus_is_covered_byte_for_byte() {
    let files = corpus_files();
    assert!(files.len() >= 500, "corpus has only {} files", files.len());
    let mut failures = Vec::new();
    for path in files {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let dialect = classify(&path, None).map_or(Dialect::Standard, |(d, _)| d);
        let tokens = tokenize(&src, dialect);
        let total: usize = tokens.iter().map(|t| t.len as usize).sum();
        let eof_last = tokens
            .last()
            .is_some_and(|t| t.kind == SyntaxKind::EOF && t.len == 0);
        let layout_zero = tokens
            .iter()
            .filter(|t| matches!(t.kind, SyntaxKind::INDENT | SyntaxKind::DEDENT))
            .all(|t| t.len == 0);
        if total != src.len() || !eof_last || !layout_zero {
            failures.push((path, total, src.len(), eof_last, layout_zero));
        }
    }
    assert!(
        failures.is_empty(),
        "{} file(s) violate the coverage invariant: {failures:#?}",
        failures.len()
    );
}

/// Layout is suppressed inside brackets, including on the line that closes
/// them. Measuring indentation there emits a spurious `INDENT` whose `DEDENT`
/// then closes an enclosing block that was never open — which shows up far
/// away, as a round-trip failure in an unrelated file.
#[test]
fn brackets_suppress_layout_through_the_closing_line() {
    let layout = |src: &str| {
        tokenize(src, Dialect::Bazel)
            .into_iter()
            .filter(|t| matches!(t.kind, SyntaxKind::INDENT | SyntaxKind::DEDENT))
            .count()
    };

    assert_eq!(layout("x = [\n    1,\n] if c else []\n"), 0);
    assert_eq!(layout("f(\n    a,\n        b,\n)\n"), 0);
    assert_eq!(layout("x = {\n    'a': 1,\n}\ny = 2\n"), 0);
    assert_eq!(layout("x = [[\n    1,\n]]\n"), 0);

    assert_eq!(layout("def f():\n    pass\n"), 2);
}

fn render(src: &str, dialect: Dialect) -> String {
    let mut offset = 0usize;
    let mut out = String::new();
    for token in tokenize(src, dialect) {
        let end = offset + token.len as usize;
        let _ = writeln!(out, "{:?} {:?}", token.kind, &src[offset..end]);
        offset = end;
    }
    out
}

#[test]
fn snapshot_def_with_block() {
    let source = "def f(x):\n    return x // 2  # halve\n";
    expect![[r##"
        DEF_KW "def"
        WHITESPACE " "
        IDENT "f"
        L_PAREN "("
        IDENT "x"
        R_PAREN ")"
        COLON ":"
        NEWLINE "\n"
        WHITESPACE "    "
        INDENT ""
        RETURN_KW "return"
        WHITESPACE " "
        IDENT "x"
        WHITESPACE " "
        DOUBLE_SLASH "//"
        WHITESPACE " "
        INT "2"
        WHITESPACE "  "
        COMMENT "# halve"
        NEWLINE "\n"
        DEDENT ""
        EOF ""
    "##]]
    .assert_eq(&render(source, Dialect::Bazel));
}

#[test]
fn snapshot_strings_and_layout() {
    let source = "x = [\n    'a',\n    r\"\"\"b\"\"\",\n    b'c',\n]\ny = \\\n    1\n";
    expect![[r#"
        IDENT "x"
        WHITESPACE " "
        ASSIGN "="
        WHITESPACE " "
        L_BRACKET "["
        NEWLINE "\n"
        WHITESPACE "    "
        STRING "'a'"
        COMMA ","
        NEWLINE "\n"
        WHITESPACE "    "
        STRING "r\"\"\"b\"\"\""
        COMMA ","
        NEWLINE "\n"
        WHITESPACE "    "
        BYTES "b'c'"
        COMMA ","
        NEWLINE "\n"
        R_BRACKET "]"
        NEWLINE "\n"
        IDENT "y"
        WHITESPACE " "
        ASSIGN "="
        WHITESPACE " "
        LINE_CONTINUATION "\\\n"
        WHITESPACE "    "
        INT "1"
        NEWLINE "\n"
        EOF ""
    "#]]
    .assert_eq(&render(source, Dialect::Bazel));
}
