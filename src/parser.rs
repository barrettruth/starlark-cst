//! Parser.
//!
//! Recursive descent with a Pratt loop for expressions, building a rowan green
//! tree. Recovery is by synchronising on statement boundaries: unplaceable
//! tokens are wrapped in [`SyntaxKind::ERROR`] and parsing continues, so a tree
//! is produced for every input.

use crate::dialect::Dialect;
use crate::syntax_kind::{Starlark, SyntaxKind};

/// A diagnostic, reported alongside the tree rather than instead of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub range: rowan::TextRange,
}

/// The result of parsing: always a tree, plus whatever went wrong.
#[derive(Debug, Clone)]
pub struct Parse {
    green: rowan::GreenNode,
    errors: Vec<ParseError>,
}

impl Parse {
    #[must_use]
    pub fn syntax(&self) -> rowan::SyntaxNode<Starlark> {
        rowan::SyntaxNode::new_root(self.green.clone())
    }

    #[must_use]
    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }

    #[must_use]
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Construct directly. Exists so the corpus harness can be written and run
    /// before the parser is.
    #[must_use]
    pub fn new(green: rowan::GreenNode, errors: Vec<ParseError>) -> Self {
        Self { green, errors }
    }
}

/// Parse `src`.
///
/// # Contract
///
/// - `parse(src, d).syntax().to_string() == src`, byte for byte, for every
///   input, valid or not. This is the round-trip gate in `tests/corpus.rs`.
/// - never panics on any input
/// - the root node is [`SyntaxKind::FILE`]
///
/// # Panics
///
/// Unimplemented.
#[must_use]
pub fn parse(src: &str, dialect: Dialect) -> Parse {
    let _ = (src, dialect, SyntaxKind::FILE);
    todo!("see AGENTS.md; tests/corpus.rs pins the contract")
}
