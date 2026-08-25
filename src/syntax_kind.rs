//! The token and node alphabet.
//!
//! Ordering is contractual: every token precedes [`SyntaxKind::EOF`], and every
//! node follows it. `Language::kind_from_raw` relies on the discriminants being
//! contiguous from zero.

// SCREAMING_CASE variants are the rowan/rust-analyzer convention for a syntax
// alphabet, and keep the enum legible next to the grammar it encodes.
#![allow(non_camel_case_types, clippy::upper_case_acronyms)]

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // -- trivia. Retained in the tree; this is what "lossless" costs.
    WHITESPACE = 0,
    COMMENT,
    /// `#: ...`, the Sphinx-style doc comment Bazel 9 introduced.
    DOC_COMMENT,
    /// A `\` at end of line.
    LINE_CONTINUATION,

    // -- layout
    NEWLINE,
    INDENT,
    DEDENT,

    // -- literals
    INT,
    FLOAT,
    STRING,
    BYTES,

    IDENT,

    // -- keywords
    AND_KW,
    BREAK_KW,
    CONTINUE_KW,
    DEF_KW,
    ELIF_KW,
    ELSE_KW,
    FOR_KW,
    IF_KW,
    IN_KW,
    LAMBDA_KW,
    LOAD_KW,
    NOT_KW,
    OR_KW,
    PASS_KW,
    RETURN_KW,
    /// Soft keyword: only a keyword when it opens a type alias.
    TYPE_KW,
    /// Conditional keyword, gated on `Dialect::has_type_keywords`.
    CAST_KW,
    /// Conditional keyword, gated on `Dialect::has_type_keywords`.
    ISINSTANCE_KW,

    /// Reserved by Bazel and rejected: `while`, `with`, `match`, `try`, `class`,
    /// `import`, `assert`, `async`, `await`, `del`, `except`, `finally`,
    /// `from`, `global`, `is`, `nonlocal`, `raise`, `yield`. Lexed so the tree
    /// stays faithful and the consumer can produce Bazel's own error text.
    FORBIDDEN_KW,

    // -- punctuation
    PLUS,
    MINUS,
    STAR,
    DOUBLE_STAR,
    SLASH,
    DOUBLE_SLASH,
    PERCENT,
    AMP,
    PIPE,
    CARET,
    TILDE,
    LT,
    GT,
    LE,
    GE,
    EQ,
    NE,
    SHL,
    SHR,
    ASSIGN,
    PLUS_ASSIGN,
    MINUS_ASSIGN,
    STAR_ASSIGN,
    SLASH_ASSIGN,
    DOUBLE_SLASH_ASSIGN,
    PERCENT_ASSIGN,
    AMP_ASSIGN,
    PIPE_ASSIGN,
    CARET_ASSIGN,
    SHL_ASSIGN,
    SHR_ASSIGN,
    DOT,
    COMMA,
    SEMI,
    COLON,
    ARROW,
    ELLIPSIS,
    L_PAREN,
    R_PAREN,
    L_BRACKET,
    R_BRACKET,
    L_BRACE,
    R_BRACE,

    /// Any byte sequence the lexer could not classify.
    ERROR_TOKEN,

    EOF,

    // -- nodes
    FILE,

    // statements
    DEF_STMT,
    IF_STMT,
    FOR_STMT,
    RETURN_STMT,
    BREAK_STMT,
    CONTINUE_STMT,
    PASS_STMT,
    LOAD_STMT,
    ASSIGN_STMT,
    EXPR_STMT,
    /// `x: int = 1`
    VAR_STMT,
    /// `type T = list[int]`
    TYPE_ALIAS_STMT,
    SUITE,

    // expressions
    LITERAL_EXPR,
    IDENT_EXPR,
    UNARY_EXPR,
    BINARY_EXPR,
    LAMBDA_EXPR,
    IF_EXPR,
    CALL_EXPR,
    DOT_EXPR,
    INDEX_EXPR,
    SLICE_EXPR,
    LIST_EXPR,
    TUPLE_EXPR,
    DICT_EXPR,
    LIST_COMP,
    DICT_COMP,
    PAREN_EXPR,
    /// `cast(T, x)`
    CAST_EXPR,
    /// `isinstance(x, T)`
    ISINSTANCE_EXPR,

    // type syntax
    TYPE_REF,
    /// `list[int]`
    TYPE_APPLICATION,
    /// `int | None`
    TYPE_UNION,

    // fragments
    PARAM_LIST,
    PARAM,
    ARG_LIST,
    ARG,
    LOAD_ITEM,
    DICT_ENTRY,
    COMP_CLAUSE,

    /// Recovery node. Holds tokens the parser could not place.
    ERROR,

    #[doc(hidden)]
    __LAST,
}

impl SyntaxKind {
    #[must_use]
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            Self::WHITESPACE | Self::COMMENT | Self::DOC_COMMENT | Self::LINE_CONTINUATION
        )
    }

    #[must_use]
    pub fn is_token(self) -> bool {
        self <= Self::EOF
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

/// rowan's binding for this alphabet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Starlark {}

impl rowan::Language for Starlark {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        ALL[raw.0 as usize]
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

/// Every variant in discriminant order. Kept in sync by `all_kinds_in_order`.
const ALL: &[SyntaxKind] = {
    use SyntaxKind as K;
    &[
        K::WHITESPACE,
        K::COMMENT,
        K::DOC_COMMENT,
        K::LINE_CONTINUATION,
        K::NEWLINE,
        K::INDENT,
        K::DEDENT,
        K::INT,
        K::FLOAT,
        K::STRING,
        K::BYTES,
        K::IDENT,
        K::AND_KW,
        K::BREAK_KW,
        K::CONTINUE_KW,
        K::DEF_KW,
        K::ELIF_KW,
        K::ELSE_KW,
        K::FOR_KW,
        K::IF_KW,
        K::IN_KW,
        K::LAMBDA_KW,
        K::LOAD_KW,
        K::NOT_KW,
        K::OR_KW,
        K::PASS_KW,
        K::RETURN_KW,
        K::TYPE_KW,
        K::CAST_KW,
        K::ISINSTANCE_KW,
        K::FORBIDDEN_KW,
        K::PLUS,
        K::MINUS,
        K::STAR,
        K::DOUBLE_STAR,
        K::SLASH,
        K::DOUBLE_SLASH,
        K::PERCENT,
        K::AMP,
        K::PIPE,
        K::CARET,
        K::TILDE,
        K::LT,
        K::GT,
        K::LE,
        K::GE,
        K::EQ,
        K::NE,
        K::SHL,
        K::SHR,
        K::ASSIGN,
        K::PLUS_ASSIGN,
        K::MINUS_ASSIGN,
        K::STAR_ASSIGN,
        K::SLASH_ASSIGN,
        K::DOUBLE_SLASH_ASSIGN,
        K::PERCENT_ASSIGN,
        K::AMP_ASSIGN,
        K::PIPE_ASSIGN,
        K::CARET_ASSIGN,
        K::SHL_ASSIGN,
        K::SHR_ASSIGN,
        K::DOT,
        K::COMMA,
        K::SEMI,
        K::COLON,
        K::ARROW,
        K::ELLIPSIS,
        K::L_PAREN,
        K::R_PAREN,
        K::L_BRACKET,
        K::R_BRACKET,
        K::L_BRACE,
        K::R_BRACE,
        K::ERROR_TOKEN,
        K::EOF,
        K::FILE,
        K::DEF_STMT,
        K::IF_STMT,
        K::FOR_STMT,
        K::RETURN_STMT,
        K::BREAK_STMT,
        K::CONTINUE_STMT,
        K::PASS_STMT,
        K::LOAD_STMT,
        K::ASSIGN_STMT,
        K::EXPR_STMT,
        K::VAR_STMT,
        K::TYPE_ALIAS_STMT,
        K::SUITE,
        K::LITERAL_EXPR,
        K::IDENT_EXPR,
        K::UNARY_EXPR,
        K::BINARY_EXPR,
        K::LAMBDA_EXPR,
        K::IF_EXPR,
        K::CALL_EXPR,
        K::DOT_EXPR,
        K::INDEX_EXPR,
        K::SLICE_EXPR,
        K::LIST_EXPR,
        K::TUPLE_EXPR,
        K::DICT_EXPR,
        K::LIST_COMP,
        K::DICT_COMP,
        K::PAREN_EXPR,
        K::CAST_EXPR,
        K::ISINSTANCE_EXPR,
        K::TYPE_REF,
        K::TYPE_APPLICATION,
        K::TYPE_UNION,
        K::PARAM_LIST,
        K::PARAM,
        K::ARG_LIST,
        K::ARG,
        K::LOAD_ITEM,
        K::DICT_ENTRY,
        K::COMP_CLAUSE,
        K::ERROR,
        K::__LAST,
    ]
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_kinds_in_order() {
        for (i, kind) in ALL.iter().enumerate() {
            assert_eq!(
                *kind as usize, i,
                "ALL is out of order at index {i}: {kind:?}"
            );
        }
        assert_eq!(ALL.len(), SyntaxKind::__LAST as usize + 1);
    }

    #[test]
    fn tokens_precede_nodes() {
        assert!(SyntaxKind::EOF.is_token());
        assert!(!SyntaxKind::FILE.is_token());
        assert!(SyntaxKind::COMMENT.is_trivia());
        assert!(!SyntaxKind::IDENT.is_trivia());
    }
}
