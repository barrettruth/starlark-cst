# starlark-cst

Lossless concrete syntax tree for Starlark and the Bazel build language.

> [!WARNING]
> The parser is not implemented yet. The public API, dialect table, and
> conformance harness are.

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
