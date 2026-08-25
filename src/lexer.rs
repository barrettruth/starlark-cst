//! Tokeniser.
//!
//! Emits a flat token stream covering the whole input with no gaps: the
//! concatenation of every token's text equals the source. Trivia is emitted,
//! not skipped. `INDENT`/`DEDENT` are synthesised and have zero width.

use crate::dialect::Dialect;
use crate::syntax_kind::SyntaxKind;

/// A token as a kind plus a byte length. Offsets are recovered by scanning,
/// which keeps the struct small enough to stay in cache on large BUILD files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lexeme {
    pub kind: SyntaxKind,
    pub len: u32,
}

/// Tokenise `src`.
///
/// # Contract
///
/// - `tokens.iter().map(|t| t.len).sum::<u32>() as usize == src.len()`
/// - `INDENT` and `DEDENT` have `len == 0`
/// - the final token is `EOF` with `len == 0`
/// - never panics, never returns `Err`; unclassifiable bytes become
///   [`SyntaxKind::ERROR_TOKEN`]
///
/// # Panics
///
/// Unimplemented.
#[must_use]
pub fn tokenize(src: &str, dialect: Dialect) -> Vec<Lexeme> {
    let _ = (src, dialect);
    todo!("see AGENTS.md; tests/lexer.rs pins the contract")
}
