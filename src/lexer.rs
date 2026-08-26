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

/// A lexical diagnostic: a fact about the bytes that no later pass can
/// recover.
///
/// The token stream is unaffected by one of these — the token is emitted
/// exactly as it would have been, so the round trip and the tree shape are the
/// same whether or not the literal was closed. That matters most for the case
/// an editor hits constantly: `load("@rules_` is an unclosed string on almost
/// every keystroke, and the consumer still needs the `STRING` token in its
/// usual place to offer a completion inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    /// Byte offsets into the source, always non-empty.
    pub start: usize,
    pub end: usize,
}

/// The token stream, and what was wrong with the bytes behind it.
#[derive(Debug, Clone, Default)]
pub struct Lexed {
    pub tokens: Vec<Lexeme>,
    pub errors: Vec<LexError>,
}

/// Tokenise `src`.
///
/// # Contract
///
/// - `tokens.iter().map(|t| t.len).sum::<u32>() as usize == src.len()`
/// - `INDENT` and `DEDENT` have `len == 0`
/// - the final token is `EOF` with `len == 0`
/// - never panics, never returns `Err`; unclassifiable bytes become
///   [`SyntaxKind::ERROR_TOKEN`] spanning a whole UTF-8 character, so every
///   token boundary is a character boundary and the sum above stays exact
/// - `errors` is in source order, and every range is non-empty
#[must_use]
pub fn tokenize(src: &str, dialect: Dialect) -> Lexed {
    Lexer::new(src, dialect).run()
}

/// The byte range of a string literal's content: the text between the quotes,
/// after the prefix. Labels live inside string literals, so the consumer needs
/// exactly this. Returns `None` if `text` is not a string literal.
#[must_use]
pub fn string_content_range(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() && matches!(bytes[i], b'r' | b'R' | b'b' | b'B') {
        i += 1;
        if i > 2 {
            return None;
        }
    }
    let quote = *bytes.get(i)?;
    if quote != b'\'' && quote != b'"' {
        return None;
    }
    let triple = bytes.len() >= i + 3 && bytes[i + 1] == quote && bytes[i + 2] == quote;
    let q = if triple { 3 } else { 1 };
    let start = i + q;
    let end = if bytes.len() >= start + q && bytes[bytes.len() - q..].iter().all(|&b| b == quote) {
        bytes.len() - q
    } else {
        bytes.len()
    };
    Some((start, end.max(start)))
}

const TAB_STOP: usize = 8;

/// Operators of three bytes, matched before the two-byte table.
///
/// Maximal munch: longest match first, always. `//=` has to be tried before
/// `//`, which has to be tried before `/`, and a table consulted in the wrong
/// order mislexes silently — the failure appears three layers up as a
/// round-trip mismatch, which is why `tests/lexer.rs` asserts every operator
/// lexes back to itself.
const THREE: &[(&[u8], SyntaxKind)] = &[
    (b"//=", SyntaxKind::DOUBLE_SLASH_ASSIGN),
    (b"<<=", SyntaxKind::SHL_ASSIGN),
    (b">>=", SyntaxKind::SHR_ASSIGN),
];
const TWO: &[(&[u8], SyntaxKind)] = &[
    (b"**", SyntaxKind::DOUBLE_STAR),
    (b"//", SyntaxKind::DOUBLE_SLASH),
    (b"==", SyntaxKind::EQ),
    (b"!=", SyntaxKind::NE),
    (b"<=", SyntaxKind::LE),
    (b">=", SyntaxKind::GE),
    (b"<<", SyntaxKind::SHL),
    (b">>", SyntaxKind::SHR),
    (b"->", SyntaxKind::ARROW),
    (b"+=", SyntaxKind::PLUS_ASSIGN),
    (b"-=", SyntaxKind::MINUS_ASSIGN),
    (b"*=", SyntaxKind::STAR_ASSIGN),
    (b"/=", SyntaxKind::SLASH_ASSIGN),
    (b"%=", SyntaxKind::PERCENT_ASSIGN),
    (b"&=", SyntaxKind::AMP_ASSIGN),
    (b"|=", SyntaxKind::PIPE_ASSIGN),
    (b"^=", SyntaxKind::CARET_ASSIGN),
];

struct Lexer<'a> {
    bytes: &'a [u8],
    src: &'a str,
    pos: usize,
    dialect: Dialect,
    out: Vec<Lexeme>,
    errors: Vec<LexError>,
    /// Indentation columns of enclosing blocks. Never empty; `[0]` at top level.
    indents: Vec<usize>,
    /// `(`/`[`/`{` nesting depth. Layout is suppressed inside brackets.
    depth: usize,
    /// A logical line is about to begin; measure indentation first.
    at_line_start: bool,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str, dialect: Dialect) -> Self {
        Self {
            bytes: src.as_bytes(),
            src,
            pos: 0,
            dialect,
            out: Vec::new(),
            errors: Vec::new(),
            indents: vec![0],
            depth: 0,
            at_line_start: true,
        }
    }

    fn run(mut self) -> Lexed {
        while self.pos < self.bytes.len() {
            if self.at_line_start {
                if self.depth == 0 {
                    self.line_start();
                } else {
                    self.at_line_start = false;
                    self.token();
                }
            } else {
                self.token();
            }
        }
        while self.indents.len() > 1 {
            self.indents.pop();
            self.push(SyntaxKind::DEDENT, 0);
        }
        self.push(SyntaxKind::EOF, 0);
        Lexed {
            tokens: self.out,
            errors: self.errors,
        }
    }

    /// Record a lexical fact. Called only where `start < end`, so every range
    /// a consumer sees is something an editor can underline.
    fn error(&mut self, message: impl Into<String>, start: usize, end: usize) {
        self.errors.push(LexError {
            message: message.into(),
            start,
            end,
        });
    }

    /// Consume one unclassifiable character as an `ERROR_TOKEN`, and say why.
    ///
    /// The token kind already records that the byte belongs to no token, but
    /// the message is reported here rather than left to the parser because a
    /// byte being unlexable is true regardless of where the parser was: left
    /// to the parser, an `ERROR_TOKEN` swallowed by a recovery loop passes
    /// without a word.
    fn error_token(&mut self) {
        let start = self.pos;
        let ch = self.src[start..].chars().next();
        let len = ch.map_or(1, char::len_utf8);
        self.pos += len;
        self.push(SyntaxKind::ERROR_TOKEN, len);
        if let Some(ch) = ch {
            self.error(format!("invalid character `{ch}`"), start, self.pos);
        }
    }

    fn push(&mut self, kind: SyntaxKind, len: usize) {
        #[allow(clippy::cast_possible_truncation)]
        self.out.push(Lexeme {
            kind,
            len: len as u32,
        });
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    /// Measure the indentation of a fresh logical line and synthesise
    /// `INDENT`/`DEDENT`. Blank and comment-only lines leave the stack alone.
    /// Measure a line's indentation and emit the `INDENT`/`DEDENT` it implies.
    ///
    /// A blank or comment-only line carries no block structure and is skipped,
    /// so a comment at column zero inside a body does not close it.
    ///
    /// A column matching no enclosing block is an indentation error, and the
    /// answer is to open a block anyway: `INDENT` and `DEDENT` must stay
    /// balanced for the parser to recover at all, so the lexer keeps the
    /// bookkeeping honest and leaves the diagnosis to the parser.
    ///
    /// A tab is reported once for the line rather than once per tab, and only
    /// on a line that carries code: Bazel rejects tab indentation, but the
    /// whitespace on a blank line indents nothing, and trailing whitespace is
    /// common enough that flagging it would bury the real ones.
    fn line_start(&mut self) {
        let start = self.pos;
        let mut col = 0usize;
        let mut tabbed = false;
        loop {
            match self.peek() {
                Some(b' ') => col += 1,
                Some(b'\t') => {
                    col += TAB_STOP - col % TAB_STOP;
                    tabbed = true;
                }
                _ => break,
            }
            self.pos += 1;
        }
        if self.pos > start {
            self.push(SyntaxKind::WHITESPACE, self.pos - start);
        }
        self.at_line_start = false;
        if matches!(self.peek(), None | Some(b'\n' | b'\r' | b'#')) {
            return;
        }
        if tabbed {
            self.error(
                "tab characters are not allowed for indentation",
                start,
                self.pos,
            );
        }
        let top = *self.indents.last().unwrap_or(&0);
        if col > top {
            self.indents.push(col);
            self.push(SyntaxKind::INDENT, 0);
        } else if col < top {
            while self.indents.len() > 1 && *self.indents.last().unwrap_or(&0) > col {
                self.indents.pop();
                self.push(SyntaxKind::DEDENT, 0);
            }
            if *self.indents.last().unwrap_or(&0) < col {
                self.indents.push(col);
                self.push(SyntaxKind::INDENT, 0);
            }
        }
    }

    fn token(&mut self) {
        let Some(b) = self.peek() else { return };
        match b {
            b'\n' => {
                self.pos += 1;
                self.push(SyntaxKind::NEWLINE, 1);
                self.at_line_start = true;
            }
            b'\r' => {
                let len = if self.peek_at(1) == Some(b'\n') { 2 } else { 1 };
                self.pos += len;
                self.push(SyntaxKind::NEWLINE, len);
                self.at_line_start = true;
            }
            b' ' | b'\t' => {
                let start = self.pos;
                while matches!(self.peek(), Some(b' ' | b'\t')) {
                    self.pos += 1;
                }
                self.push(SyntaxKind::WHITESPACE, self.pos - start);
            }
            b'#' => self.comment(),
            b'\\' => self.backslash(),
            b'\'' | b'"' => self.string(0, false),
            b'0'..=b'9' => self.number(),
            b'.' => {
                if self.peek_at(1) == Some(b'.') && self.peek_at(2) == Some(b'.') {
                    self.pos += 3;
                    self.push(SyntaxKind::ELLIPSIS, 3);
                } else if self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
                    self.number();
                } else {
                    self.pos += 1;
                    self.push(SyntaxKind::DOT, 1);
                }
            }
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => self.ident_or_string(),
            _ => self.operator(),
        }
    }

    fn comment(&mut self) {
        let start = self.pos;
        while !matches!(self.peek(), None | Some(b'\n' | b'\r')) {
            self.pos += 1;
        }
        let kind = if self.bytes[start..self.pos].starts_with(b"#:") {
            SyntaxKind::DOC_COMMENT
        } else {
            SyntaxKind::COMMENT
        };
        self.push(kind, self.pos - start);
    }

    /// A `\` outside a string: a line continuation when at end of line,
    /// otherwise a stray byte. The continuation token swallows the newline so
    /// the next physical line is not a fresh logical line.
    fn backslash(&mut self) {
        match self.peek_at(1) {
            Some(b'\n') => {
                self.pos += 2;
                self.push(SyntaxKind::LINE_CONTINUATION, 2);
            }
            Some(b'\r') => {
                let len = if self.peek_at(2) == Some(b'\n') { 3 } else { 2 };
                self.pos += len;
                self.push(SyntaxKind::LINE_CONTINUATION, len);
            }
            _ => self.error_token(),
        }
    }

    /// `prefix_len` bytes of `r`/`b` prefix have already been accepted and
    /// `self.pos` sits on the opening quote.
    /// Consume a string literal, from its prefix to its closing quote.
    ///
    /// An escape consumes the next byte in raw strings too: in a raw string
    /// `\"` is two characters of *content* and does not terminate the literal,
    /// even though the backslash is not an escape in the value.
    fn string(&mut self, prefix_len: usize, is_bytes: bool) {
        let start = self.pos - prefix_len;
        let quote = self.bytes[self.pos];
        self.pos += 1;
        let triple = self.peek() == Some(quote) && self.peek_at(1) == Some(quote);
        if triple {
            self.pos += 2;
        }
        let open_end = self.pos;
        let mut closed = false;
        loop {
            match self.peek() {
                None => break,
                Some(b'\\') => {
                    self.pos += 1;
                    if self.peek() == Some(b'\r') && self.peek_at(1) == Some(b'\n') {
                        self.pos += 2;
                    } else if self.peek().is_some() {
                        self.pos += 1;
                    }
                }
                Some(b) if b == quote => {
                    if triple {
                        if self.peek_at(1) == Some(quote) && self.peek_at(2) == Some(quote) {
                            self.pos += 3;
                            closed = true;
                            break;
                        }
                        self.pos += 1;
                    } else {
                        self.pos += 1;
                        closed = true;
                        break;
                    }
                }
                Some(b'\n' | b'\r') if !triple => break,
                Some(_) => self.pos += 1,
            }
        }
        if !closed {
            // On the opening quote, not on the truncation point: the quote is
            // what is missing a partner, and it is where the fix goes. Same
            // anchor the parser uses for an unclosed bracket.
            self.error("unclosed string literal", start, open_end);
        }
        let kind = if is_bytes {
            SyntaxKind::BYTES
        } else {
            SyntaxKind::STRING
        };
        self.push(kind, self.pos - start);
    }

    fn number(&mut self) {
        let start = self.pos;
        let mut is_float = false;
        if self.peek() == Some(b'0')
            && matches!(
                self.peek_at(1),
                Some(b'x' | b'X' | b'o' | b'O' | b'b' | b'B')
            )
            && self.peek_at(2).is_some_and(|c| c.is_ascii_alphanumeric())
        {
            self.pos += 2;
            while self.peek().is_some_and(|c| c.is_ascii_alphanumeric()) {
                self.pos += 1;
            }
            self.push(SyntaxKind::INT, self.pos - start);
            return;
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') && self.peek_at(1) != Some(b'.') {
            is_float = true;
            self.pos += 1;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let mut i = 1;
            if matches!(self.peek_at(i), Some(b'+' | b'-')) {
                i += 1;
            }
            if self.peek_at(i).is_some_and(|c| c.is_ascii_digit()) {
                is_float = true;
                self.pos += i;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.pos += 1;
                }
            }
        }
        let kind = if is_float {
            SyntaxKind::FLOAT
        } else {
            SyntaxKind::INT
        };
        self.push(kind, self.pos - start);
    }

    fn ident_or_string(&mut self) {
        let mut prefix = 0;
        let mut is_bytes = false;
        let mut seen_r = false;
        while prefix < 2 {
            match self.peek_at(prefix) {
                Some(b'r' | b'R') if !seen_r => seen_r = true,
                Some(b'b' | b'B') if !is_bytes => is_bytes = true,
                _ => break,
            }
            prefix += 1;
        }
        if prefix > 0 && matches!(self.peek_at(prefix), Some(b'\'' | b'"')) {
            self.pos += prefix;
            self.string(prefix, is_bytes);
            return;
        }

        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            self.pos += 1;
        }
        let text = &self.src[start..self.pos];
        let kind = self.keyword(text).unwrap_or(SyntaxKind::IDENT);
        self.push(kind, self.pos - start);
    }

    /// The keyword a word is, if any.
    ///
    /// The forbidden set is transcribed from Bazel's `Lexer.java`. `match` is
    /// deliberately absent: it is a soft keyword in Python only, and real BUILD
    /// files use it as an ordinary name.
    fn keyword(&self, text: &str) -> Option<SyntaxKind> {
        use SyntaxKind as K;
        let kind = match text {
            "and" => K::AND_KW,
            "break" => K::BREAK_KW,
            "continue" => K::CONTINUE_KW,
            "def" => K::DEF_KW,
            "elif" => K::ELIF_KW,
            "else" => K::ELSE_KW,
            "for" => K::FOR_KW,
            "if" => K::IF_KW,
            "in" => K::IN_KW,
            "lambda" => K::LAMBDA_KW,
            "load" => K::LOAD_KW,
            "not" => K::NOT_KW,
            "or" => K::OR_KW,
            "pass" => K::PASS_KW,
            "return" => K::RETURN_KW,
            "type" if self.dialect.allows_type_syntax() => K::TYPE_KW,
            "cast" if self.dialect.has_type_keywords() => K::CAST_KW,
            "isinstance" if self.dialect.has_type_keywords() => K::ISINSTANCE_KW,
            "while" | "with" | "try" | "class" | "import" | "assert" | "async" | "await"
            | "del" | "except" | "finally" | "from" | "global" | "is" | "nonlocal" | "raise"
            | "yield" => K::FORBIDDEN_KW,
            _ => return None,
        };
        Some(kind)
    }

    fn operator(&mut self) {
        use SyntaxKind as K;
        let rest = &self.bytes[self.pos..];
        for (text, kind) in THREE {
            if rest.starts_with(text) {
                self.pos += 3;
                self.push(*kind, 3);
                return;
            }
        }
        for (text, kind) in TWO {
            if rest.starts_with(text) {
                self.pos += 2;
                self.push(*kind, 2);
                return;
            }
        }
        let kind = match rest[0] {
            b'+' => K::PLUS,
            b'-' => K::MINUS,
            b'*' => K::STAR,
            b'/' => K::SLASH,
            b'%' => K::PERCENT,
            b'&' => K::AMP,
            b'|' => K::PIPE,
            b'^' => K::CARET,
            b'~' => K::TILDE,
            b'<' => K::LT,
            b'>' => K::GT,
            b'=' => K::ASSIGN,
            b',' => K::COMMA,
            b';' => K::SEMI,
            b':' => K::COLON,
            b'(' | b'[' | b'{' => {
                self.depth += 1;
                match rest[0] {
                    b'(' => K::L_PAREN,
                    b'[' => K::L_BRACKET,
                    _ => K::L_BRACE,
                }
            }
            b')' | b']' | b'}' => {
                self.depth = self.depth.saturating_sub(1);
                match rest[0] {
                    b')' => K::R_PAREN,
                    b']' => K::R_BRACKET,
                    _ => K::R_BRACE,
                }
            }
            _ => {
                self.error_token();
                return;
            }
        };
        self.pos += 1;
        self.push(kind, 1);
    }
}
