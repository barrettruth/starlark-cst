//! A lossless concrete syntax tree for Starlark and the Bazel build language.
//!
//! The tree reproduces its input byte for byte, comments and all, and is
//! produced for every input including malformed ones. Errors are reported
//! alongside the tree, never instead of it.
//!
//! Scope stops at syntax. Resolving `load()`, interpreting labels, and knowing
//! what `cc_library` is are all the consumer's concern.
//!
//! ```no_run
//! use starlark_cst::{Dialect, parse};
//!
//! let src = "cc_library(\n    name = \"a\",  # keep\n)\n";
//! let parsed = parse(src, Dialect::Bazel);
//! assert_eq!(parsed.syntax().to_string(), src);
//! ```

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
