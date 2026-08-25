//! Parser.
//!
//! Recursive descent with a Pratt loop for expressions, building a rowan green
//! tree. Recovery is by synchronising on statement boundaries: unplaceable
//! tokens are wrapped in [`SyntaxKind::ERROR`] and parsing continues, so a tree
//! is produced for every input.

use rowan::{Checkpoint, GreenNodeBuilder, TextRange, TextSize};

use crate::dialect::Dialect;
use crate::lexer::tokenize;
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
#[must_use]
pub fn parse(src: &str, dialect: Dialect) -> Parse {
    let mut spans = Vec::new();
    let mut offset = 0usize;
    for lexeme in tokenize(src, dialect) {
        let end = offset + lexeme.len as usize;
        spans.push(Span {
            kind: lexeme.kind,
            start: offset,
            end,
        });
        offset = end;
    }
    let parser = Parser {
        src,
        tokens: spans,
        pos: 0,
        builder: GreenNodeBuilder::new(),
        errors: Vec::new(),
        depth: 0,
        recursion: 0,
    };
    parser.run()
}

#[derive(Debug, Clone, Copy)]
struct Span {
    kind: SyntaxKind,
    start: usize,
    end: usize,
}

struct Parser<'a> {
    src: &'a str,
    tokens: Vec<Span>,
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<ParseError>,
    /// `(`/`[`/`{` nesting. Newlines inside brackets are trivia.
    depth: usize,
    /// Recursive-descent depth, guarded by [`MAX_RECURSION`].
    recursion: usize,
}

const STMT_RECOVERY: &[SyntaxKind] = &[SyntaxKind::NEWLINE, SyntaxKind::DEDENT, SyntaxKind::EOF];

/// Descent limit. Chosen well below where the stack actually runs out —
/// measured overflow is around 10,000 nested parentheses — because the frames
/// are large and the margin costs nothing: real Starlark does not nest deeply,
/// and generated files that do are exactly the adversarial case this guards.
const MAX_RECURSION: usize = 256;

impl Parser<'_> {
    fn run(mut self) -> Parse {
        self.builder.start_node(SyntaxKind::FILE.into());
        loop {
            match self.current() {
                SyntaxKind::EOF => break,
                SyntaxKind::NEWLINE => self.bump(),
                SyntaxKind::INDENT | SyntaxKind::DEDENT => self.bump_layout(),
                _ => self.statement(),
            }
        }
        self.flush_trivia();
        self.builder.finish_node();
        Parse {
            green: self.builder.finish(),
            errors: self.errors,
        }
    }

    // -- cursor -------------------------------------------------------------

    fn is_trivia_at(&self, i: usize) -> bool {
        let kind = self.tokens[i].kind;
        kind.is_trivia() || (kind == SyntaxKind::NEWLINE && self.depth > 0)
    }

    /// Index of the `n`th significant token at or after `pos`.
    fn significant(&self, n: usize) -> usize {
        let mut i = self.pos;
        let mut left = n;
        while i < self.tokens.len() {
            if !self.is_trivia_at(i) {
                if left == 0 {
                    return i;
                }
                left -= 1;
            }
            i += 1;
        }
        self.tokens.len().saturating_sub(1)
    }

    fn current(&self) -> SyntaxKind {
        self.tokens
            .get(self.significant(0))
            .map_or(SyntaxKind::EOF, |t| t.kind)
    }

    fn nth(&self, n: usize) -> SyntaxKind {
        self.tokens
            .get(self.significant(n))
            .map_or(SyntaxKind::EOF, |t| t.kind)
    }

    fn current_range(&self) -> TextRange {
        let span = self.tokens[self.significant(0)];
        #[allow(clippy::cast_possible_truncation)]
        TextRange::new(
            TextSize::from(span.start as u32),
            TextSize::from(span.end as u32),
        )
    }

    fn flush_trivia(&mut self) {
        while self.pos < self.tokens.len() && self.is_trivia_at(self.pos) {
            self.token_into_tree(self.pos);
            self.pos += 1;
        }
    }

    fn token_into_tree(&mut self, i: usize) {
        let span = self.tokens[i];
        if span.start < span.end {
            self.builder
                .token(span.kind.into(), &self.src[span.start..span.end]);
        }
    }

    /// Add the current significant token to the tree.
    fn bump(&mut self) {
        self.flush_trivia();
        if self.pos >= self.tokens.len() {
            return;
        }
        let kind = self.tokens[self.pos].kind;
        if kind == SyntaxKind::EOF {
            return;
        }
        match kind {
            SyntaxKind::L_PAREN | SyntaxKind::L_BRACKET | SyntaxKind::L_BRACE => self.depth += 1,
            SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::R_BRACE => {
                self.depth = self.depth.saturating_sub(1);
            }
            _ => {}
        }
        self.token_into_tree(self.pos);
        self.pos += 1;
    }

    /// Consume a zero-width layout token (`INDENT`/`DEDENT`) without adding it
    /// to the tree.
    fn bump_layout(&mut self) {
        self.flush_trivia();
        if self.pos < self.tokens.len() && self.tokens[self.pos].kind != SyntaxKind::EOF {
            self.pos += 1;
        }
    }

    fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
    }

    fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn error(&mut self, message: impl Into<String>) {
        let range = self.current_range();
        self.errors.push(ParseError {
            message: message.into(),
            range,
        });
    }

    fn expect(&mut self, kind: SyntaxKind, what: &str) {
        if !self.eat(kind) {
            self.error(format!("expected {what}"));
        }
    }

    fn start(&mut self, kind: SyntaxKind) {
        self.flush_trivia();
        self.builder.start_node(kind.into());
    }

    fn finish(&mut self) {
        self.builder.finish_node();
    }

    fn checkpoint(&mut self) -> Checkpoint {
        self.flush_trivia();
        self.builder.checkpoint()
    }

    fn wrap(&mut self, at: Checkpoint, kind: SyntaxKind) {
        self.builder.start_node_at(at, kind.into());
        self.builder.finish_node();
    }

    /// Wrap everything up to the next statement boundary in an ERROR node.
    /// This is the synchronisation point that keeps recovery local.
    fn error_to_stmt_boundary(&mut self, message: &str) {
        self.error(message);
        self.start(SyntaxKind::ERROR);
        while !STMT_RECOVERY.contains(&self.current()) {
            self.bump();
        }
        self.finish();
    }

    // -- statements ----------------------------------------------------------

    fn statement(&mut self) {
        match self.current() {
            SyntaxKind::DEF_KW => self.def_stmt(),
            SyntaxKind::IF_KW => self.if_stmt(),
            SyntaxKind::FOR_KW => self.for_stmt(),
            _ => self.simple_stmt_line(),
        }
    }

    /// `small_stmt (';' small_stmt)* NEWLINE?`
    fn simple_stmt_line(&mut self) {
        self.small_stmt();
        while self.eat(SyntaxKind::SEMI) {
            if STMT_RECOVERY.contains(&self.current()) {
                break;
            }
            self.small_stmt();
        }
        if !STMT_RECOVERY.contains(&self.current()) {
            self.error_to_stmt_boundary("expected a newline after statement");
        }
    }

    fn small_stmt(&mut self) {
        match self.current() {
            SyntaxKind::RETURN_KW => {
                self.start(SyntaxKind::RETURN_STMT);
                self.bump();
                if !STMT_RECOVERY.contains(&self.current()) && !self.at(SyntaxKind::SEMI) {
                    self.expr_list();
                }
                self.finish();
            }
            SyntaxKind::BREAK_KW => {
                self.start(SyntaxKind::BREAK_STMT);
                self.bump();
                self.finish();
            }
            SyntaxKind::CONTINUE_KW => {
                self.start(SyntaxKind::CONTINUE_STMT);
                self.bump();
                self.finish();
            }
            SyntaxKind::PASS_KW => {
                self.start(SyntaxKind::PASS_STMT);
                self.bump();
                self.finish();
            }
            SyntaxKind::LOAD_KW if self.nth(1) == SyntaxKind::L_PAREN => self.load_stmt(),
            // `type T = ...` is a type alias; any other `type` is a name.
            SyntaxKind::TYPE_KW
                if self.nth(1) == SyntaxKind::IDENT && self.nth(2) == SyntaxKind::ASSIGN =>
            {
                self.start(SyntaxKind::TYPE_ALIAS_STMT);
                self.bump();
                self.bump();
                self.bump();
                self.type_expr();
                self.finish();
            }
            _ => self.expr_or_assign(),
        }
    }

    fn load_stmt(&mut self) {
        self.start(SyntaxKind::LOAD_STMT);
        self.bump(); // load
        self.bump(); // (
        while !self.at(SyntaxKind::R_PAREN) && !self.at(SyntaxKind::EOF) {
            let before = self.significant(0);
            self.start(SyntaxKind::LOAD_ITEM);
            if self.at_name() && self.nth(1) == SyntaxKind::ASSIGN {
                self.bump();
                self.bump();
            }
            if self.at(SyntaxKind::STRING) {
                self.bump();
            } else if !self.at(SyntaxKind::COMMA) && !self.at(SyntaxKind::R_PAREN) {
                self.test();
            }
            self.finish();
            if !self.eat(SyntaxKind::COMMA) && self.significant(0) == before {
                self.error("expected `,` or `)` in load()");
                self.start(SyntaxKind::ERROR);
                self.bump();
                self.finish();
            }
        }
        self.expect(SyntaxKind::R_PAREN, "`)` to close load()");
        self.finish();
    }

    fn expr_or_assign(&mut self) {
        let cp = self.checkpoint();
        self.expr_list();
        match self.current() {
            SyntaxKind::ASSIGN => {
                self.builder
                    .start_node_at(cp, SyntaxKind::ASSIGN_STMT.into());
                while self.eat(SyntaxKind::ASSIGN) {
                    self.expr_list();
                }
                self.finish();
            }
            k if is_aug_assign(k) => {
                self.builder
                    .start_node_at(cp, SyntaxKind::ASSIGN_STMT.into());
                self.bump();
                self.expr_list();
                self.finish();
            }
            SyntaxKind::COLON => {
                // `x: int = 1`
                self.builder.start_node_at(cp, SyntaxKind::VAR_STMT.into());
                self.bump();
                self.type_expr();
                if self.eat(SyntaxKind::ASSIGN) {
                    self.expr_list();
                }
                self.finish();
            }
            _ => {
                self.builder.start_node_at(cp, SyntaxKind::EXPR_STMT.into());
                self.finish();
            }
        }
    }

    fn def_stmt(&mut self) {
        self.start(SyntaxKind::DEF_STMT);
        self.bump(); // def
        if self.at_name() {
            self.bump();
        } else {
            self.error("expected a function name");
        }
        if self.at(SyntaxKind::L_PAREN) {
            self.param_list();
        } else {
            self.error("expected `(`");
        }
        if self.eat(SyntaxKind::ARROW) {
            self.type_expr();
        }
        self.expect(SyntaxKind::COLON, "`:` after def signature");
        self.suite();
        self.finish();
    }

    fn param_list(&mut self) {
        self.start(SyntaxKind::PARAM_LIST);
        self.bump(); // (
        while !self.at(SyntaxKind::R_PAREN) && !self.at(SyntaxKind::EOF) {
            let before = self.significant(0);
            self.param();
            if !self.eat(SyntaxKind::COMMA) && self.significant(0) == before {
                self.error("expected `,` or `)` in parameters");
                self.start(SyntaxKind::ERROR);
                self.bump();
                self.finish();
            }
        }
        self.expect(SyntaxKind::R_PAREN, "`)` to close parameters");
        self.finish();
    }

    fn param(&mut self) {
        self.param_impl(true);
    }

    /// Lambda parameters cannot be annotated: their `:` ends the parameter
    /// list, so it must not be eaten as an annotation.
    fn lambda_param(&mut self) {
        self.param_impl(false);
    }

    fn param_impl(&mut self, annotations: bool) {
        self.start(SyntaxKind::PARAM);
        let _ = self.eat(SyntaxKind::STAR) || self.eat(SyntaxKind::DOUBLE_STAR);
        if self.at_name() {
            self.bump();
        }
        if annotations && self.eat(SyntaxKind::COLON) {
            self.type_expr();
        }
        if self.eat(SyntaxKind::ASSIGN) {
            self.test();
        }
        self.finish();
    }

    fn if_stmt(&mut self) {
        self.start(SyntaxKind::IF_STMT);
        self.bump(); // if
        self.test();
        self.expect(SyntaxKind::COLON, "`:` after if condition");
        self.suite();
        while self.at(SyntaxKind::ELIF_KW) {
            self.bump();
            self.test();
            self.expect(SyntaxKind::COLON, "`:` after elif condition");
            self.suite();
        }
        if self.eat(SyntaxKind::ELSE_KW) {
            self.expect(SyntaxKind::COLON, "`:` after else");
            self.suite();
        }
        self.finish();
    }

    fn for_stmt(&mut self) {
        self.start(SyntaxKind::FOR_STMT);
        self.bump(); // for
        self.target_list();
        self.expect(SyntaxKind::IN_KW, "`in` in for statement");
        self.expr_list();
        self.expect(SyntaxKind::COLON, "`:` after for clause");
        self.suite();
        self.finish();
    }

    /// An indented block, or the rest of the line for a one-liner.
    ///
    /// Nested blocks recurse through `statement`, so this is guarded on the
    /// same budget as expressions.
    fn suite(&mut self) {
        if self.recursion >= MAX_RECURSION {
            self.too_deep();
            return;
        }
        self.recursion += 1;
        self.suite_inner();
        self.recursion -= 1;
    }

    fn suite_inner(&mut self) {
        self.start(SyntaxKind::SUITE);
        if self.at(SyntaxKind::NEWLINE) {
            self.bump();
            while self.at(SyntaxKind::NEWLINE) {
                self.bump();
            }
            if self.at(SyntaxKind::INDENT) {
                self.indented_block();
            } else {
                self.error("expected an indented block");
            }
        } else if !STMT_RECOVERY.contains(&self.current()) {
            self.simple_stmt_line();
        } else {
            self.error("expected a statement");
        }
        self.finish();
    }

    /// `INDENT statement* DEDENT`. Assumes the current token is `INDENT`.
    ///
    /// A further `INDENT` means a line indented past its own block. The lexer
    /// opens a block there so the layout tokens stay balanced and leaves the
    /// diagnosis to us; swallowing it instead would pair the inner `DEDENT`
    /// with the outer block and silently end it early, dropping the rest of
    /// the body out of the enclosing statement.
    fn indented_block(&mut self) {
        self.bump_layout(); // INDENT
        loop {
            match self.current() {
                SyntaxKind::DEDENT => {
                    self.bump_layout();
                    break;
                }
                SyntaxKind::EOF => break,
                SyntaxKind::NEWLINE => self.bump(),
                SyntaxKind::INDENT => {
                    self.error("unexpected indentation");
                    self.start(SyntaxKind::SUITE);
                    self.suite_nested();
                    self.finish();
                }
                _ => self.statement(),
            }
        }
    }

    /// `indented_block` behind the recursion budget.
    fn suite_nested(&mut self) {
        if self.recursion >= MAX_RECURSION {
            self.too_deep();
            return;
        }
        self.recursion += 1;
        self.indented_block();
        self.recursion -= 1;
    }

    // -- expressions ---------------------------------------------------------

    /// Does the current token work as a plain name? Soft and conditional
    /// keywords do; they are only special in specific positions.
    fn at_name(&self) -> bool {
        matches!(
            self.current(),
            SyntaxKind::IDENT
                | SyntaxKind::TYPE_KW
                | SyntaxKind::CAST_KW
                | SyntaxKind::ISINSTANCE_KW
        )
    }

    /// `test (',' test)*`, wrapped in `TUPLE_EXPR` when there is more than one.
    fn expr_list(&mut self) {
        let cp = self.checkpoint();
        self.test();
        if self.at(SyntaxKind::COMMA) {
            self.builder
                .start_node_at(cp, SyntaxKind::TUPLE_EXPR.into());
            while self.eat(SyntaxKind::COMMA) {
                if self.stops_expr() {
                    break;
                }
                self.test();
            }
            self.finish();
        }
    }

    /// Does the current token end an expression list?
    fn stops_expr(&self) -> bool {
        matches!(
            self.current(),
            SyntaxKind::NEWLINE
                | SyntaxKind::EOF
                | SyntaxKind::DEDENT
                | SyntaxKind::SEMI
                | SyntaxKind::ASSIGN
                | SyntaxKind::COLON
                | SyntaxKind::R_PAREN
                | SyntaxKind::R_BRACKET
                | SyntaxKind::R_BRACE
                | SyntaxKind::FOR_KW
                | SyntaxKind::IN_KW
        ) || is_aug_assign(self.current())
    }

    /// Loop targets: primaries joined by commas. Parsed below comparison level
    /// so the `in` that follows is never eaten as an operator.
    fn target_list(&mut self) {
        let cp = self.checkpoint();
        if self.stops_expr() {
            self.error("expected a loop variable");
            return;
        }
        self.primary();
        if self.at(SyntaxKind::COMMA) {
            self.builder
                .start_node_at(cp, SyntaxKind::TUPLE_EXPR.into());
            while self.eat(SyntaxKind::COMMA) {
                if self.stops_expr() {
                    break;
                }
                self.primary();
            }
            self.finish();
        }
    }

    /// A type annotation. Bazel's type expressions (`int`, `list[int]`,
    /// `int | None`) are a subset of expression syntax, so reuse it.
    fn type_expr(&mut self) {
        self.start(SyntaxKind::TYPE_REF);
        self.test();
        self.finish();
    }

    /// `lambda ...` | ternary | boolean precedence tower.
    ///
    /// Guards the descent: see [`Parser::too_deep`].
    fn test(&mut self) {
        if self.recursion >= MAX_RECURSION {
            self.too_deep();
            return;
        }
        self.recursion += 1;
        self.test_inner();
        self.recursion -= 1;
    }

    /// Bail out of a descent that would otherwise exhaust the stack.
    ///
    /// A stack overflow aborts the process rather than unwinding, so
    /// `catch_unwind` cannot contain it and a language server would die with
    /// it. Everything from here to the enclosing bracket or statement boundary
    /// becomes one `ERROR` node, which keeps the round trip intact.
    fn too_deep(&mut self) {
        self.error("nested too deeply");
        self.start(SyntaxKind::ERROR);
        let before = self.significant(0);
        while !matches!(
            self.current(),
            SyntaxKind::R_PAREN | SyntaxKind::R_BRACKET | SyntaxKind::R_BRACE
        ) && !STMT_RECOVERY.contains(&self.current())
        {
            self.bump();
        }
        // Guarantee progress even when the bail lands directly on a closer.
        if self.significant(0) == before {
            self.bump();
        }
        self.finish();
    }

    fn test_inner(&mut self) {
        if self.at(SyntaxKind::LAMBDA_KW) {
            self.lambda();
            return;
        }
        let cp = self.checkpoint();
        self.or_test();
        if self.at(SyntaxKind::IF_KW) {
            self.builder.start_node_at(cp, SyntaxKind::IF_EXPR.into());
            self.bump();
            self.or_test();
            self.expect(SyntaxKind::ELSE_KW, "`else` in conditional expression");
            self.test();
            self.finish();
        }
    }

    fn lambda(&mut self) {
        self.start(SyntaxKind::LAMBDA_EXPR);
        self.bump(); // lambda
        self.start(SyntaxKind::PARAM_LIST);
        while !self.at(SyntaxKind::COLON) && !STMT_RECOVERY.contains(&self.current()) {
            let before = self.significant(0);
            self.lambda_param();
            if !self.eat(SyntaxKind::COMMA) && self.significant(0) == before {
                break;
            }
        }
        self.finish();
        self.expect(SyntaxKind::COLON, "`:` after lambda parameters");
        self.test();
        self.finish();
    }

    fn or_test(&mut self) {
        let cp = self.checkpoint();
        self.and_test();
        while self.at(SyntaxKind::OR_KW) {
            self.builder
                .start_node_at(cp, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.and_test();
            self.finish();
        }
    }

    fn and_test(&mut self) {
        let cp = self.checkpoint();
        self.not_test();
        while self.at(SyntaxKind::AND_KW) {
            self.builder
                .start_node_at(cp, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            self.not_test();
            self.finish();
        }
    }

    fn not_test(&mut self) {
        if self.at(SyntaxKind::NOT_KW) {
            self.start(SyntaxKind::UNARY_EXPR);
            self.bump();
            self.not_test();
            self.finish();
        } else {
            self.comparison();
        }
    }

    fn comparison(&mut self) {
        let cp = self.checkpoint();
        self.bit_or();
        loop {
            let is_cmp = matches!(
                self.current(),
                SyntaxKind::EQ
                    | SyntaxKind::NE
                    | SyntaxKind::LT
                    | SyntaxKind::GT
                    | SyntaxKind::LE
                    | SyntaxKind::GE
                    | SyntaxKind::IN_KW
            ) || (self.at(SyntaxKind::NOT_KW) && self.nth(1) == SyntaxKind::IN_KW);
            if !is_cmp {
                break;
            }
            self.builder
                .start_node_at(cp, SyntaxKind::BINARY_EXPR.into());
            if self.at(SyntaxKind::NOT_KW) {
                self.bump(); // not
            }
            self.bump(); // the operator / `in`
            self.bit_or();
            self.finish();
        }
    }

    fn binary_level(&mut self, next: fn(&mut Self), ops: &[SyntaxKind]) {
        let cp = self.checkpoint();
        next(self);
        while ops.contains(&self.current()) {
            self.builder
                .start_node_at(cp, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            next(self);
            self.finish();
        }
    }

    fn bit_or(&mut self) {
        self.binary_level(Self::bit_xor, &[SyntaxKind::PIPE]);
    }

    fn bit_xor(&mut self) {
        self.binary_level(Self::bit_and, &[SyntaxKind::CARET]);
    }

    fn bit_and(&mut self) {
        self.binary_level(Self::shift, &[SyntaxKind::AMP]);
    }

    fn shift(&mut self) {
        self.binary_level(Self::arith, &[SyntaxKind::SHL, SyntaxKind::SHR]);
    }

    fn arith(&mut self) {
        self.binary_level(Self::term, &[SyntaxKind::PLUS, SyntaxKind::MINUS]);
    }

    fn term(&mut self) {
        self.binary_level(
            Self::factor,
            &[
                SyntaxKind::STAR,
                SyntaxKind::SLASH,
                SyntaxKind::DOUBLE_SLASH,
                SyntaxKind::PERCENT,
            ],
        );
    }

    fn factor(&mut self) {
        if matches!(
            self.current(),
            SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::TILDE
        ) {
            self.start(SyntaxKind::UNARY_EXPR);
            self.bump();
            self.factor();
            self.finish();
        } else {
            self.primary();
        }
    }

    /// Atom plus any chain of postfix operations.
    fn primary(&mut self) {
        let cp = self.checkpoint();
        self.atom();
        loop {
            match self.current() {
                SyntaxKind::DOT => {
                    self.builder.start_node_at(cp, SyntaxKind::DOT_EXPR.into());
                    self.bump();
                    if self.at_name() || self.at(SyntaxKind::FORBIDDEN_KW) {
                        self.bump();
                    } else {
                        self.error("expected an attribute name after `.`");
                    }
                    self.finish();
                }
                SyntaxKind::L_PAREN => {
                    self.builder.start_node_at(cp, SyntaxKind::CALL_EXPR.into());
                    self.arg_list();
                    self.finish();
                }
                SyntaxKind::L_BRACKET => {
                    let kind = self.index_or_slice();
                    self.builder.start_node_at(cp, kind.into());
                    self.builder.finish_node();
                }
                _ => break,
            }
        }
    }

    fn arg_list(&mut self) {
        self.start(SyntaxKind::ARG_LIST);
        self.bump(); // (
        while !self.at(SyntaxKind::R_PAREN) && !self.at(SyntaxKind::EOF) {
            let before = self.significant(0);
            self.start(SyntaxKind::ARG);
            if self.at(SyntaxKind::STAR) || self.at(SyntaxKind::DOUBLE_STAR) {
                self.bump();
                if !self.at(SyntaxKind::COMMA) && !self.at(SyntaxKind::R_PAREN) {
                    self.test();
                }
            } else if (self.at_name() || self.at(SyntaxKind::FORBIDDEN_KW))
                && self.nth(1) == SyntaxKind::ASSIGN
            {
                self.bump();
                self.bump();
                self.test();
            } else if !self.at(SyntaxKind::COMMA) {
                let inner = self.checkpoint();
                self.test();
                if self.at(SyntaxKind::FOR_KW) {
                    // `f(x for x in y)`: not Starlark, but keep the tree sane.
                    self.builder
                        .start_node_at(inner, SyntaxKind::LIST_COMP.into());
                    self.comp_clauses();
                    self.finish();
                }
            }
            self.finish();
            if !self.eat(SyntaxKind::COMMA) && self.significant(0) == before {
                if self.at(SyntaxKind::R_PAREN) || self.at(SyntaxKind::EOF) {
                    break;
                }
                self.error("expected `,` or `)` in arguments");
                self.start(SyntaxKind::ERROR);
                self.bump();
                self.finish();
            }
        }
        self.expect(SyntaxKind::R_PAREN, "`)` to close arguments");
        self.finish();
    }

    /// Parse `[...]` after a primary; returns which node kind it turned out
    /// to be. Children are emitted; the caller wraps them.
    fn index_or_slice(&mut self) -> SyntaxKind {
        self.bump(); // [
        let mut saw_colon = false;
        while !self.at(SyntaxKind::R_BRACKET) && !self.at(SyntaxKind::EOF) {
            let before = self.significant(0);
            match self.current() {
                SyntaxKind::COLON => {
                    saw_colon = true;
                    self.bump();
                }
                SyntaxKind::COMMA => self.bump(),
                _ => self.test(),
            }
            if self.significant(0) == before {
                self.error("expected `]`");
                self.start(SyntaxKind::ERROR);
                self.bump();
                self.finish();
            }
        }
        self.expect(SyntaxKind::R_BRACKET, "`]` to close subscript");
        if saw_colon {
            SyntaxKind::SLICE_EXPR
        } else {
            SyntaxKind::INDEX_EXPR
        }
    }

    fn atom(&mut self) {
        match self.current() {
            SyntaxKind::INT
            | SyntaxKind::FLOAT
            | SyntaxKind::STRING
            | SyntaxKind::BYTES
            | SyntaxKind::ELLIPSIS => {
                self.start(SyntaxKind::LITERAL_EXPR);
                self.bump();
                self.finish();
            }
            SyntaxKind::IDENT
            | SyntaxKind::TYPE_KW
            | SyntaxKind::CAST_KW
            | SyntaxKind::ISINSTANCE_KW => {
                self.start(SyntaxKind::IDENT_EXPR);
                self.bump();
                self.finish();
            }
            SyntaxKind::FORBIDDEN_KW => {
                // Bazel rejects the word; keep the tree faithful and flag it.
                self.error("this keyword is reserved and cannot be used");
                self.start(SyntaxKind::IDENT_EXPR);
                self.bump();
                self.finish();
            }
            SyntaxKind::L_PAREN => self.paren_or_tuple(),
            SyntaxKind::L_BRACKET => self.list_or_comp(),
            SyntaxKind::L_BRACE => self.dict_or_comp(),
            _ => {
                self.error("expected an expression");
                self.start(SyntaxKind::ERROR);
                if !STMT_RECOVERY.contains(&self.current()) && !self.stops_expr() {
                    self.bump();
                }
                self.finish();
            }
        }
    }

    fn paren_or_tuple(&mut self) {
        let cp = self.checkpoint();
        self.bump(); // (
        if self.at(SyntaxKind::R_PAREN) {
            self.bump();
            self.wrap(cp, SyntaxKind::TUPLE_EXPR);
            return;
        }
        self.test();
        if self.at(SyntaxKind::FOR_KW) {
            // A generator expression: not Starlark, but produce structure.
            self.comp_clauses();
            self.expect(SyntaxKind::R_PAREN, "`)`");
            self.wrap(cp, SyntaxKind::LIST_COMP);
            return;
        }
        let mut tuple = false;
        while self.at(SyntaxKind::COMMA) {
            tuple = true;
            self.bump();
            if self.at(SyntaxKind::R_PAREN) || self.at(SyntaxKind::EOF) {
                break;
            }
            let before = self.significant(0);
            self.test();
            if self.significant(0) == before {
                break;
            }
        }
        self.expect(SyntaxKind::R_PAREN, "`)`");
        self.wrap(
            cp,
            if tuple {
                SyntaxKind::TUPLE_EXPR
            } else {
                SyntaxKind::PAREN_EXPR
            },
        );
    }

    fn list_or_comp(&mut self) {
        let cp = self.checkpoint();
        self.bump(); // [
        if self.at(SyntaxKind::R_BRACKET) {
            self.bump();
            self.wrap(cp, SyntaxKind::LIST_EXPR);
            return;
        }
        self.test();
        if self.at(SyntaxKind::FOR_KW) {
            self.comp_clauses();
            self.expect(SyntaxKind::R_BRACKET, "`]`");
            self.wrap(cp, SyntaxKind::LIST_COMP);
            return;
        }
        while self.at(SyntaxKind::COMMA) {
            self.bump();
            if self.at(SyntaxKind::R_BRACKET) || self.at(SyntaxKind::EOF) {
                break;
            }
            let before = self.significant(0);
            self.test();
            if self.significant(0) == before {
                break;
            }
        }
        self.expect(SyntaxKind::R_BRACKET, "`]`");
        self.wrap(cp, SyntaxKind::LIST_EXPR);
    }

    fn dict_or_comp(&mut self) {
        let cp = self.checkpoint();
        self.bump(); // {
        if self.at(SyntaxKind::R_BRACE) {
            self.bump();
            self.wrap(cp, SyntaxKind::DICT_EXPR);
            return;
        }
        self.dict_entry();
        if self.at(SyntaxKind::FOR_KW) {
            self.comp_clauses();
            self.expect(SyntaxKind::R_BRACE, "`}`");
            self.wrap(cp, SyntaxKind::DICT_COMP);
            return;
        }
        while self.at(SyntaxKind::COMMA) {
            self.bump();
            if self.at(SyntaxKind::R_BRACE) || self.at(SyntaxKind::EOF) {
                break;
            }
            let before = self.significant(0);
            self.dict_entry();
            if self.significant(0) == before {
                break;
            }
        }
        self.expect(SyntaxKind::R_BRACE, "`}`");
        self.wrap(cp, SyntaxKind::DICT_EXPR);
    }

    fn dict_entry(&mut self) {
        self.start(SyntaxKind::DICT_ENTRY);
        if self.eat(SyntaxKind::DOUBLE_STAR) {
            // `{**d}`: not Starlark, but keep the entry shape.
            if !self.at(SyntaxKind::COMMA) && !self.at(SyntaxKind::R_BRACE) {
                self.test();
            }
            self.finish();
            return;
        }
        self.test();
        if self.eat(SyntaxKind::COLON) {
            self.test();
        } else {
            // A set literal element. Bazel rejects it; flag, keep the tree.
            self.error("expected `:` in dict entry (set literals are not Starlark)");
        }
        self.finish();
    }

    fn comp_clauses(&mut self) {
        while matches!(self.current(), SyntaxKind::FOR_KW | SyntaxKind::IF_KW) {
            self.start(SyntaxKind::COMP_CLAUSE);
            if self.eat(SyntaxKind::FOR_KW) {
                self.target_list();
                self.expect(SyntaxKind::IN_KW, "`in` in comprehension");
                self.or_test();
            } else {
                self.bump(); // if
                self.or_test();
            }
            self.finish();
        }
    }
}

fn is_aug_assign(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::PLUS_ASSIGN
            | SyntaxKind::MINUS_ASSIGN
            | SyntaxKind::STAR_ASSIGN
            | SyntaxKind::SLASH_ASSIGN
            | SyntaxKind::DOUBLE_SLASH_ASSIGN
            | SyntaxKind::PERCENT_ASSIGN
            | SyntaxKind::AMP_ASSIGN
            | SyntaxKind::PIPE_ASSIGN
            | SyntaxKind::CARET_ASSIGN
            | SyntaxKind::SHL_ASSIGN
            | SyntaxKind::SHR_ASSIGN
    )
}
