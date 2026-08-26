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

## Supported Dialects

bazel-language-servers supports all major (and pre) Bazel 9 dialects:

- `BUILD`
- `BUILD.bazel`
- `*.bzl`
- `MODULE.bazel`
- `*.MODULE.bazel`
- `REPO.bazel`
- `VENDOR.bazel`
- `WORKSPACE`
- `WORKSPACE.bazel`
- `WORKSPACE.bzlmod`
- `*.BUILD`
- `*.BUILD.bazel`
- `*.cquery`
- `*.query.bzl`
- `*.scl`
- `tools/build_rules/prelude_bazel`.

See [CONTRIBUTING.md](CONTRIBUTING.md).
