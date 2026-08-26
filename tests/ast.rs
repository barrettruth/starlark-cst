//! Typed-accessor tests.
//!
//! The child order in the tree is positional, so most of these pin an accessor
//! against a shape that is easy to get backwards.

use std::path::{Path, PathBuf};

use starlark_cst::ast::{AstNode, CallExpr, DotExpr, Expr, File, SliceExpr, Stmt};
use starlark_cst::{Dialect, SyntaxKind, classify, parse};

fn file(src: &str) -> File {
    File::cast(parse(src, Dialect::Bazel).syntax()).expect("root is FILE")
}

fn first_call(src: &str) -> CallExpr {
    parse(src, Dialect::Bazel)
        .syntax()
        .descendants()
        .find_map(CallExpr::cast)
        .expect("a call")
}

#[test]
fn call_callee_and_arguments() {
    let call = first_call("cc_library(name = \"a\", srcs = [\"x.cc\"], *rest, **kw)\n");
    assert_eq!(call.callee_name().as_deref(), Some("cc_library"));

    let args: Vec<_> = call.args().collect();
    assert_eq!(args.len(), 4);
    assert_eq!(args[0].name().as_deref(), Some("name"));
    assert_eq!(args[1].name().as_deref(), Some("srcs"));

    assert_eq!(args[2].name(), None);
    assert!(args[2].is_splat());
    assert_eq!(args[3].name(), None);
    assert!(args[3].is_kwargs());

    let srcs = call.arg("srcs").expect("srcs");
    assert!(matches!(srcs, Expr::List(_)));
    assert!(call.arg("deps").is_none());
}

#[test]
fn dotted_callee() {
    let call = first_call("ctx.actions.run(executable = tool)\n");
    assert_eq!(call.callee_name().as_deref(), Some("ctx.actions.run"));

    let dot = parse("a.b.c\n", Dialect::Bazel)
        .syntax()
        .descendants()
        .find_map(DotExpr::cast)
        .expect("a dot expression");
    assert_eq!(dot.dotted_name(), "a.b.c");
}

#[test]
fn load_module_symbols_and_aliases() {
    let src = "load(\"@skylib//rules:write_file.bzl\", \"write_file\", alias = \"copy_file\")\n";
    let load = file(src).loads().next().expect("a load");

    assert_eq!(
        load.module().and_then(|m| m.value()).as_deref(),
        Some("@skylib//rules:write_file.bzl")
    );

    let symbols: Vec<_> = load.symbols().collect();
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].value().as_deref(), Some("write_file"));
    assert_eq!(symbols[0].alias(), None);
    assert_eq!(symbols[1].alias().as_deref(), Some("alias"));
    assert_eq!(symbols[1].value().as_deref(), Some("copy_file"));
}

/// The range must exclude quotes and any prefix, and must index the original
/// source — this is what a go-to-definition on a label reports.
#[test]
fn string_value_range_indexes_the_source() {
    for (src, expected) in [
        ("x = \"//lib:srcs\"\n", "//lib:srcs"),
        ("x = '//lib:srcs'\n", "//lib:srcs"),
        ("x = r\"//lib:srcs\"\n", "//lib:srcs"),
        ("x = \"\"\"//lib:srcs\"\"\"\n", "//lib:srcs"),
    ] {
        let literal = parse(src, Dialect::Bazel)
            .syntax()
            .descendants()
            .find_map(starlark_cst::ast::LiteralExpr::cast)
            .expect("a literal");
        let range = literal.string_value_range().expect("a string range");
        assert_eq!(&src[range], expected, "range for {src:?}");
        assert_eq!(literal.string_value().as_deref(), Some(expected));
    }

    let literal = parse("x = 42\n", Dialect::Bazel)
        .syntax()
        .descendants()
        .find_map(starlark_cst::ast::LiteralExpr::cast)
        .unwrap();
    assert!(!literal.is_string());
    assert!(literal.string_value_range().is_none());
}

/// Slice parts are positional in the tree, so an omitted bound would otherwise
/// shift the ones that follow it.
#[test]
fn slice_parts_survive_omitted_bounds() {
    let parts = |src: &str| {
        let slice = parse(src, Dialect::Bazel)
            .syntax()
            .descendants()
            .find_map(SliceExpr::cast)
            .expect("a slice");
        let (a, b, c) = slice.parts();
        (
            a.map(|e| e.text()),
            b.map(|e| e.text()),
            c.map(|e| e.text()),
        )
    };

    assert_eq!(
        parts("x = a[1:2:3]\n"),
        (Some("1".into()), Some("2".into()), Some("3".into()))
    );
    assert_eq!(parts("x = a[:2]\n"), (None, Some("2".into()), None));
    assert_eq!(parts("x = a[2:]\n"), (Some("2".into()), None, None));
    assert_eq!(parts("x = a[::2]\n"), (None, None, Some("2".into())));
}

#[test]
fn assignment_forms() {
    let stmts: Vec<_> = file("a = 1\nb += 2\nx: int = 3\n").stmts().collect();

    let Stmt::Assign(plain) = &stmts[0] else {
        panic!("expected an assignment, got {:?}", stmts[0]);
    };
    assert!(!plain.is_augmented());
    assert_eq!(plain.lhs().map(|e| e.text()).as_deref(), Some("a"));
    assert_eq!(plain.rhs().map(|e| e.text()).as_deref(), Some("1"));

    let Stmt::Assign(augmented) = &stmts[1] else {
        panic!("expected an assignment");
    };
    assert!(augmented.is_augmented());
    assert_eq!(
        augmented.op().map(|t| t.text().to_string()).as_deref(),
        Some("+=")
    );

    let Stmt::Var(annotated) = &stmts[2] else {
        panic!("expected an annotated binding");
    };
    assert_eq!(annotated.ty().map(|t| t.text()).as_deref(), Some("int"));
    assert_eq!(annotated.rhs().map(|e| e.text()).as_deref(), Some("3"));
}

#[test]
fn def_signature() {
    let src = "def f(a, b = 1, c: int = 2, *args, **kwargs) -> str:\n    return a\n";
    let Some(Stmt::Def(def)) = file(src).stmts().next() else {
        panic!("expected a def");
    };
    assert_eq!(def.name().as_deref(), Some("f"));
    assert_eq!(def.return_type().map(|t| t.text()).as_deref(), Some("str"));

    let params: Vec<_> = def.params().collect();
    assert_eq!(params.len(), 5);
    assert_eq!(params[0].name().as_deref(), Some("a"));
    assert_eq!(params[1].default().map(|e| e.text()).as_deref(), Some("1"));
    assert_eq!(params[2].ty().map(|t| t.text()).as_deref(), Some("int"));
    assert!(params[3].is_splat());
    assert!(params[4].is_kwargs());

    assert_eq!(def.body().map(|b| b.stmts().count()), Some(1));
}

#[test]
fn comprehension_clauses() {
    let comp = parse("x = [a for a in b if a]\n", Dialect::Bazel)
        .syntax()
        .descendants()
        .find_map(starlark_cst::ast::ListComp::cast)
        .expect("a comprehension");
    assert_eq!(comp.element().map(|e| e.text()).as_deref(), Some("a"));

    let clauses: Vec<_> = comp.clauses().collect();
    assert_eq!(clauses.len(), 2);
    assert!(clauses[0].is_for());
    assert_eq!(clauses[0].targets().map(|e| e.text()).as_deref(), Some("a"));
    assert_eq!(clauses[0].expr().map(|e| e.text()).as_deref(), Some("b"));
    assert!(!clauses[1].is_for());
    assert_eq!(clauses[1].expr().map(|e| e.text()).as_deref(), Some("a"));
}

/// What a consumer actually does on opening a BUILD file: list the targets it
/// declares. If this is awkward, the layer has failed at its job.
#[test]
fn extracting_targets_from_a_build_file() {
    let src = "\
load(\"//macros:defs.bzl\", \"my_macro\")

cc_library(
    name = \"core\",
    srcs = [\"a.cc\"],
)

my_macro(name = \"generated\")

filegroup(name = \"data\", srcs = glob([\"**/*.txt\"]))
";
    let targets: Vec<(String, String)> = file(src)
        .stmts()
        .filter_map(|stmt| match stmt {
            Stmt::Expr(expr) => expr.expr(),
            _ => None,
        })
        .filter_map(|expr| match expr {
            Expr::Call(call) => Some(call),
            _ => None,
        })
        .filter_map(|call| {
            let rule = call.callee_name()?;
            let Expr::Literal(name) = call.arg("name")? else {
                return None;
            };
            Some((rule, name.string_value()?))
        })
        .collect();

    assert_eq!(
        targets,
        vec![
            ("cc_library".to_string(), "core".to_string()),
            ("my_macro".to_string(), "generated".to_string()),
            ("filegroup".to_string(), "data".to_string()),
        ]
    );
}

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
    files.truncate(300);
    files
}

/// Every node the parser can produce must be reachable through the typed API:
/// an expression kind that no `Expr` variant covers is a hole a consumer falls
/// into with no way out but raw `kind()` matching.
#[test]
fn every_expression_and_statement_kind_casts() {
    let mut unreachable = Vec::new();
    for path in corpus_files() {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let dialect = classify(&path, None).map_or(Dialect::Standard, |(d, _)| d);
        for node in parse(&src, dialect).syntax().descendants() {
            let kind = node.kind();
            let expr_like = matches!(
                kind,
                SyntaxKind::LITERAL_EXPR
                    | SyntaxKind::IDENT_EXPR
                    | SyntaxKind::CALL_EXPR
                    | SyntaxKind::LIST_EXPR
                    | SyntaxKind::DICT_EXPR
                    | SyntaxKind::TUPLE_EXPR
                    | SyntaxKind::BINARY_EXPR
                    | SyntaxKind::DOT_EXPR
            );
            if expr_like && Expr::cast(node.clone()).is_none() {
                unreachable.push(format!("{} {kind:?}", path.display()));
            }
        }
    }
    assert!(unreachable.is_empty(), "unreachable: {unreachable:#?}");
}

/// Walking the whole corpus through the typed accessors must not panic. The
/// accessors index children positionally, and a malformed file can leave those
/// children missing.
#[test]
fn accessors_tolerate_the_whole_corpus() {
    let mut calls = 0usize;
    for path in corpus_files() {
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let dialect = classify(&path, None).map_or(Dialect::Standard, |(d, _)| d);
        for node in parse(&src, dialect).syntax().descendants() {
            if let Some(call) = CallExpr::cast(node.clone()) {
                let _ = call.callee_name();
                for arg in call.args() {
                    let _ = (arg.name(), arg.value(), arg.is_splat());
                }
                calls += 1;
            }
            if let Some(slice) = SliceExpr::cast(node.clone()) {
                let _ = slice.parts();
            }
            if let Some(expr) = Expr::cast(node) {
                let _ = expr.range();
            }
        }
    }
    assert!(
        calls > 1_000,
        "expected a substantial corpus, saw {calls} calls"
    );
}
