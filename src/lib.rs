//! A lossless concrete syntax tree for Starlark and the Bazel build language.
//!
//! The tree reproduces its input byte for byte, comments and all, and is
//! produced for every input including malformed ones. Errors are reported
//! alongside the tree, never instead of it.
//!
//! Scope stops at syntax. Resolving `load()`, interpreting labels, and knowing
//! what `cc_library` is are all the consumer's concern.
//!
//! ```
//! use starlark_cst::{Dialect, parse, ast::{AstNode, CallExpr, Expr}};
//!
//! let src = "cc_library(name = \"core\", srcs = [\"a.cc\"])\n";
//! let parsed = parse(src, Dialect::Bazel);
//! assert_eq!(parsed.syntax().to_string(), src);
//!
//! let call = parsed.syntax().descendants().find_map(CallExpr::cast).unwrap();
//! assert_eq!(call.callee_name().as_deref(), Some("cc_library"));
//!
//! let Some(Expr::Literal(name)) = call.arg("name") else { unreachable!() };
//! assert_eq!(name.string_value().as_deref(), Some("core"));
//! // The content range excludes the quotes: what a go-to-definition reports.
//! assert_eq!(&src[name.string_value_range().unwrap()], "core");
//! ```

pub mod ast;
pub mod dialect;
pub mod lexer;
pub mod parser;
pub mod syntax_kind;

pub use dialect::{Dialect, FileKind, classify};
pub use lexer::{Lexeme, tokenize};
pub use parser::{Parse, ParseError, parse};
pub use syntax_kind::{Starlark, SyntaxKind};

/// A node in the tree.
pub type SyntaxNode = rowan::SyntaxNode<Starlark>;
/// A token in the tree.
pub type SyntaxToken = rowan::SyntaxToken<Starlark>;
/// Either of the above.
pub type SyntaxElement = rowan::SyntaxElement<Starlark>;
