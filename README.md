# starlark-cst

Lossless concrete syntax tree for Starlark and the Bazel build language.

```rust
use starlark_cst::{Dialect, parse, ast::{AstNode, CallExpr, Expr}};

let parsed = parse("cc_library(name = \"core\", srcs = [\"a.cc\"])\n", Dialect::Bazel);
let call = parsed.syntax().descendants().find_map(CallExpr::cast).unwrap();

assert_eq!(call.callee_name().as_deref(), Some("cc_library"));
let Some(Expr::Literal(name)) = call.arg("name") else { unreachable!() };
assert_eq!(name.string_value().as_deref(), Some("core"));
// The content range excludes the quotes: what a go-to-definition reports.
assert_eq!(name.string_value_range().unwrap(), (19..23).into());
```

## Why

Every existing Starlark parser in Rust loses something an editor needs.

| parser                    | problem                                                                  |
| ------------------------- | ------------------------------------------------------------------------ |
| `starlark_syntax` (Meta)  | discards comments from the token stream; returns `Err` on the first error |
| `tree-sitter-starlark`    | 215 lines wrapping the Python grammar; no `load` node; stale since 2024-12 |
| buildifier (`bazel-contrib/buildtools`) | lossless and correct, but Go                                |

This crate keeps every byte, produces a tree for malformed input, and covers
Bazel 9 type-annotation syntax.

## Scope

Syntax only. No name resolution, no `load()` following, no labels, no build
graph, no formatting. Those belong to the consumer — see
[`bazel-language-server`](https://github.com/barrettruth/bazel-language-server).

## Dialects

`BUILD`, `BUILD.bazel`, `*.bzl`, `MODULE.bazel`, `*.MODULE.bazel`, `REPO.bazel`,
`VENDOR.bazel`, `WORKSPACE`, `WORKSPACE.bazel`, `WORKSPACE.bzlmod`, `*.BUILD`,
`*.BUILD.bazel`, `*.cquery`, `*.query.bzl`, `*.scl`, and
`tools/build_rules/prelude_bazel`.

## Guarantees

For every input, valid or not:

```rust
parse(src, dialect).syntax().to_string() == src
```

Enforced against ~1,000 real-world files from bazel, rules_cc, rules_python,
rules_go, bazel-skylib, buildtools, rules_java and apple_support.

## Development

```sh
nix develop      # rust, buildifier, bazelisk, just
just corpus      # harvest the conformance corpus
just ci          # format, lint, test
```

## License

MIT
