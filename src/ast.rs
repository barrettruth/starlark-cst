//! Typed accessors over the untyped tree.
//!
//! Every type here is a newtype around a [`SyntaxNode`] of one kind, so casting
//! is a `kind()` check and carries no allocation. The tree is unchanged; this
//! only spares callers from matching on [`SyntaxKind`] by hand and getting the
//! child order subtly wrong.
//!
//! ```
//! use starlark_cst::{Dialect, parse, ast::{AstNode, CallExpr}};
//!
//! let parsed = parse("cc_library(name = \"a\")\n", Dialect::Bazel);
//! let call = parsed.syntax().descendants().find_map(CallExpr::cast).unwrap();
//! assert_eq!(call.callee_name().as_deref(), Some("cc_library"));
//!
//! let arg = call.args().next().unwrap();
//! assert_eq!(arg.name().as_deref(), Some("name"));
//! ```

use crate::lexer::string_content_range;
use crate::syntax_kind::SyntaxKind;
use crate::{SyntaxNode, SyntaxToken};
use rowan::TextRange;

/// A node of one known kind.
pub trait AstNode: Sized {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(node: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;

    /// The source text this node spans.
    fn text(&self) -> String {
        self.syntax().text().to_string()
    }

    /// Where this node sits in the file.
    fn range(&self) -> TextRange {
        self.syntax().text_range()
    }
}

fn child<N: AstNode>(parent: &SyntaxNode) -> Option<N> {
    parent.children().find_map(N::cast)
}

fn children<N: AstNode>(parent: &SyntaxNode) -> impl Iterator<Item = N> + use<N> {
    parent.children().filter_map(N::cast)
}

fn nth_child<N: AstNode>(parent: &SyntaxNode, n: usize) -> Option<N> {
    parent.children().filter_map(N::cast).nth(n)
}

fn token(parent: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    parent
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == kind)
}

/// The first non-trivia token, whatever its kind. Operators live directly on
/// the node rather than in a child, so this is how they are read.
fn first_token_matching(
    parent: &SyntaxNode,
    pred: impl Fn(SyntaxKind) -> bool,
) -> Option<SyntaxToken> {
    parent
        .children_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| !t.kind().is_trivia() && pred(t.kind()))
}

macro_rules! ast_node {
    ($(#[$m:meta])* $name:ident, $kind:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == SyntaxKind::$kind
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                Self::can_cast(node.kind()).then_some(Self(node))
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

ast_node!(/// The whole file.
    File, FILE);
ast_node!(DefStmt, DEF_STMT);
ast_node!(IfStmt, IF_STMT);
ast_node!(ForStmt, FOR_STMT);
ast_node!(ReturnStmt, RETURN_STMT);
ast_node!(BreakStmt, BREAK_STMT);
ast_node!(ContinueStmt, CONTINUE_STMT);
ast_node!(PassStmt, PASS_STMT);
ast_node!(LoadStmt, LOAD_STMT);
ast_node!(AssignStmt, ASSIGN_STMT);
ast_node!(ExprStmt, EXPR_STMT);
ast_node!(VarStmt, VAR_STMT);
ast_node!(TypeAliasStmt, TYPE_ALIAS_STMT);
ast_node!(Suite, SUITE);

ast_node!(LiteralExpr, LITERAL_EXPR);
ast_node!(IdentExpr, IDENT_EXPR);
ast_node!(UnaryExpr, UNARY_EXPR);
ast_node!(BinaryExpr, BINARY_EXPR);
ast_node!(LambdaExpr, LAMBDA_EXPR);
ast_node!(IfExpr, IF_EXPR);
ast_node!(CallExpr, CALL_EXPR);
ast_node!(DotExpr, DOT_EXPR);
ast_node!(IndexExpr, INDEX_EXPR);
ast_node!(SliceExpr, SLICE_EXPR);
ast_node!(ListExpr, LIST_EXPR);
ast_node!(TupleExpr, TUPLE_EXPR);
ast_node!(DictExpr, DICT_EXPR);
ast_node!(ListComp, LIST_COMP);
ast_node!(DictComp, DICT_COMP);
ast_node!(ParenExpr, PAREN_EXPR);
ast_node!(CastExpr, CAST_EXPR);
ast_node!(IsinstanceExpr, ISINSTANCE_EXPR);

ast_node!(TypeRef, TYPE_REF);
ast_node!(TypeApplication, TYPE_APPLICATION);
ast_node!(TypeUnion, TYPE_UNION);

ast_node!(ParamList, PARAM_LIST);
ast_node!(Param, PARAM);
ast_node!(ArgList, ARG_LIST);
ast_node!(Arg, ARG);
ast_node!(LoadItem, LOAD_ITEM);
ast_node!(DictEntry, DICT_ENTRY);
ast_node!(CompClause, COMP_CLAUSE);
ast_node!(/// Tokens the parser could not place.
    Error, ERROR);

macro_rules! ast_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident($ty:ty)),* $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant($ty)),*
        }

        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                $(<$ty>::can_cast(kind))||*
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                $(if <$ty>::can_cast(node.kind()) {
                    return <$ty>::cast(node).map(Self::$variant);
                })*
                None
            }
            fn syntax(&self) -> &SyntaxNode {
                match self {
                    $(Self::$variant(inner) => inner.syntax()),*
                }
            }
        }
    };
}

ast_enum!(/// Any expression.
Expr {
    Literal(LiteralExpr),
    Ident(IdentExpr),
    Unary(UnaryExpr),
    Binary(BinaryExpr),
    Lambda(LambdaExpr),
    If(IfExpr),
    Call(CallExpr),
    Dot(DotExpr),
    Index(IndexExpr),
    Slice(SliceExpr),
    List(ListExpr),
    Tuple(TupleExpr),
    Dict(DictExpr),
    ListComp(ListComp),
    DictComp(DictComp),
    Paren(ParenExpr),
    Cast(CastExpr),
    Isinstance(IsinstanceExpr),
});

ast_enum!(/// Any statement.
Stmt {
    Def(DefStmt),
    If(IfStmt),
    For(ForStmt),
    Return(ReturnStmt),
    Break(BreakStmt),
    Continue(ContinueStmt),
    Pass(PassStmt),
    Load(LoadStmt),
    Assign(AssignStmt),
    Expr(ExprStmt),
    Var(VarStmt),
    TypeAlias(TypeAliasStmt),
});

impl File {
    /// Top-level statements, in source order.
    pub fn stmts(&self) -> impl Iterator<Item = Stmt> + use<> {
        children(&self.0)
    }

    /// Every `load()` in the file.
    pub fn loads(&self) -> impl Iterator<Item = LoadStmt> + use<> {
        children(&self.0)
    }
}

impl CallExpr {
    /// The called expression: an `IdentExpr` for `cc_library(...)`, a
    /// `DotExpr` for `ctx.actions.run(...)`.
    #[must_use]
    pub fn callee(&self) -> Option<Expr> {
        child(&self.0)
    }

    /// The callee spelled out, for the common cases only: `cc_library` and
    /// `ctx.actions.run`. `None` when the callee is computed.
    #[must_use]
    pub fn callee_name(&self) -> Option<String> {
        match self.callee()? {
            Expr::Ident(ident) => ident.name(),
            Expr::Dot(dot) => Some(dot.dotted_name()),
            _ => None,
        }
    }

    #[must_use]
    pub fn arg_list(&self) -> Option<ArgList> {
        child(&self.0)
    }

    /// Arguments in source order, positional and keyword alike.
    pub fn args(&self) -> impl Iterator<Item = Arg> + use<> {
        self.arg_list()
            .into_iter()
            .flat_map(|list| children::<Arg>(list.syntax()).collect::<Vec<_>>())
    }

    /// The value of keyword argument `name`, e.g. the `srcs` of a rule call.
    #[must_use]
    pub fn arg(&self, name: &str) -> Option<Expr> {
        self.args()
            .find(|a| a.name().as_deref() == Some(name))?
            .value()
    }
}

impl ArgList {
    pub fn args(&self) -> impl Iterator<Item = Arg> + use<> {
        children(&self.0)
    }
}

impl Arg {
    /// The keyword, for `name = "a"`. `None` for a positional argument.
    ///
    /// The keyword is a token rather than a child node, so it is only present
    /// when an `=` follows it — `**kwargs` has neither.
    #[must_use]
    pub fn name(&self) -> Option<String> {
        token(&self.0, SyntaxKind::ASSIGN)?;
        Some(token(&self.0, SyntaxKind::IDENT)?.text().to_string())
    }

    #[must_use]
    pub fn value(&self) -> Option<Expr> {
        child(&self.0)
    }

    /// `*args`.
    #[must_use]
    pub fn is_splat(&self) -> bool {
        token(&self.0, SyntaxKind::STAR).is_some()
    }

    /// `**kwargs`.
    #[must_use]
    pub fn is_kwargs(&self) -> bool {
        token(&self.0, SyntaxKind::DOUBLE_STAR).is_some()
    }
}

impl LoadStmt {
    pub fn items(&self) -> impl Iterator<Item = LoadItem> + use<> {
        children(&self.0)
    }

    /// The first item, which is the module being loaded from.
    #[must_use]
    pub fn module(&self) -> Option<LoadItem> {
        child(&self.0)
    }

    /// The symbols, i.e. every item after the module.
    pub fn symbols(&self) -> impl Iterator<Item = LoadItem> + use<> {
        children::<LoadItem>(&self.0).skip(1)
    }
}

impl LoadItem {
    /// The local name a symbol is bound to, for `alias = "original"`.
    #[must_use]
    pub fn alias(&self) -> Option<String> {
        token(&self.0, SyntaxKind::ASSIGN)?;
        Some(token(&self.0, SyntaxKind::IDENT)?.text().to_string())
    }

    /// The string token, whether this is the module, a symbol, or the
    /// right-hand side of an alias.
    #[must_use]
    pub fn string(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::STRING)
    }

    /// The string's content, quotes and prefix removed.
    #[must_use]
    pub fn value(&self) -> Option<String> {
        string_value(&self.string()?)
    }

    /// Where the content sits in the file, excluding the quotes. This is the
    /// range to report for a go-to-definition on a load path.
    #[must_use]
    pub fn value_range(&self) -> Option<TextRange> {
        string_value_range(&self.string()?)
    }
}

impl DefStmt {
    #[must_use]
    pub fn name(&self) -> Option<String> {
        Some(token(&self.0, SyntaxKind::IDENT)?.text().to_string())
    }

    #[must_use]
    pub fn name_token(&self) -> Option<SyntaxToken> {
        token(&self.0, SyntaxKind::IDENT)
    }

    #[must_use]
    pub fn param_list(&self) -> Option<ParamList> {
        child(&self.0)
    }

    pub fn params(&self) -> impl Iterator<Item = Param> + use<> {
        self.param_list()
            .into_iter()
            .flat_map(|list| children::<Param>(list.syntax()).collect::<Vec<_>>())
    }

    /// The `-> T` annotation.
    #[must_use]
    pub fn return_type(&self) -> Option<TypeRef> {
        child(&self.0)
    }

    #[must_use]
    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }
}

impl ParamList {
    pub fn params(&self) -> impl Iterator<Item = Param> + use<> {
        children(&self.0)
    }
}

impl Param {
    #[must_use]
    pub fn name(&self) -> Option<String> {
        Some(token(&self.0, SyntaxKind::IDENT)?.text().to_string())
    }

    /// The `: T` annotation.
    #[must_use]
    pub fn ty(&self) -> Option<TypeRef> {
        child(&self.0)
    }

    /// The `= expr` default.
    #[must_use]
    pub fn default(&self) -> Option<Expr> {
        child(&self.0)
    }

    /// `*args`.
    #[must_use]
    pub fn is_splat(&self) -> bool {
        token(&self.0, SyntaxKind::STAR).is_some()
    }

    /// `**kwargs`.
    #[must_use]
    pub fn is_kwargs(&self) -> bool {
        token(&self.0, SyntaxKind::DOUBLE_STAR).is_some()
    }
}

impl Suite {
    pub fn stmts(&self) -> impl Iterator<Item = Stmt> + use<> {
        children(&self.0)
    }
}

impl AssignStmt {
    #[must_use]
    pub fn lhs(&self) -> Option<Expr> {
        nth_child(&self.0, 0)
    }

    #[must_use]
    pub fn rhs(&self) -> Option<Expr> {
        nth_child(&self.0, 1)
    }

    /// The operator token: `=` for a plain assignment, `+=` and friends
    /// otherwise.
    #[must_use]
    pub fn op(&self) -> Option<SyntaxToken> {
        first_token_matching(&self.0, is_assign_op)
    }

    /// Whether this is an augmented assignment such as `x += 1`.
    #[must_use]
    pub fn is_augmented(&self) -> bool {
        self.op().is_some_and(|t| t.kind() != SyntaxKind::ASSIGN)
    }
}

impl VarStmt {
    #[must_use]
    pub fn lhs(&self) -> Option<Expr> {
        nth_child(&self.0, 0)
    }

    #[must_use]
    pub fn ty(&self) -> Option<TypeRef> {
        child(&self.0)
    }

    #[must_use]
    pub fn rhs(&self) -> Option<Expr> {
        let ty_end = self.ty()?.range().end();
        children::<Expr>(&self.0).find(|e| e.range().start() >= ty_end)
    }
}

impl ExprStmt {
    #[must_use]
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl ReturnStmt {
    #[must_use]
    pub fn value(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl IfStmt {
    #[must_use]
    pub fn condition(&self) -> Option<Expr> {
        child(&self.0)
    }

    /// Bodies in source order: the `if` block first, then each `elif`, then
    /// `else` if present.
    pub fn bodies(&self) -> impl Iterator<Item = Suite> + use<> {
        children(&self.0)
    }
}

impl ForStmt {
    /// The loop variables.
    #[must_use]
    pub fn targets(&self) -> Option<Expr> {
        nth_child(&self.0, 0)
    }

    #[must_use]
    pub fn iterable(&self) -> Option<Expr> {
        nth_child(&self.0, 1)
    }

    #[must_use]
    pub fn body(&self) -> Option<Suite> {
        child(&self.0)
    }
}

impl IdentExpr {
    #[must_use]
    pub fn name(&self) -> Option<String> {
        Some(self.name_token()?.text().to_string())
    }

    #[must_use]
    pub fn name_token(&self) -> Option<SyntaxToken> {
        first_token_matching(&self.0, |k| {
            matches!(
                k,
                SyntaxKind::IDENT
                    | SyntaxKind::TYPE_KW
                    | SyntaxKind::CAST_KW
                    | SyntaxKind::ISINSTANCE_KW
            )
        })
    }
}

impl DotExpr {
    #[must_use]
    pub fn base(&self) -> Option<Expr> {
        child(&self.0)
    }

    /// The field after the dot.
    #[must_use]
    pub fn field(&self) -> Option<String> {
        Some(token(&self.0, SyntaxKind::IDENT)?.text().to_string())
    }

    /// The whole dotted path with no whitespace, e.g. `ctx.actions.run`.
    #[must_use]
    pub fn dotted_name(&self) -> String {
        let mut parts = Vec::new();
        let mut current = Some(Expr::Dot(self.clone()));
        while let Some(expr) = current {
            match expr {
                Expr::Dot(dot) => {
                    if let Some(field) = dot.field() {
                        parts.push(field);
                    }
                    current = dot.base();
                }
                Expr::Ident(ident) => {
                    if let Some(name) = ident.name() {
                        parts.push(name);
                    }
                    current = None;
                }
                _ => current = None,
            }
        }
        parts.reverse();
        parts.join(".")
    }
}

impl IndexExpr {
    #[must_use]
    pub fn base(&self) -> Option<Expr> {
        nth_child(&self.0, 0)
    }

    #[must_use]
    pub fn index(&self) -> Option<Expr> {
        nth_child(&self.0, 1)
    }
}

impl SliceExpr {
    #[must_use]
    pub fn base(&self) -> Option<Expr> {
        nth_child(&self.0, 0)
    }

    /// `start`, `stop` and `step`, each `None` when omitted.
    ///
    /// The parts are positional in the tree, so `a[:2]` and `a[2:]` are
    /// indistinguishable by index alone. They are told apart here by where each
    /// expression falls relative to the colons.
    #[must_use]
    pub fn parts(&self) -> (Option<Expr>, Option<Expr>, Option<Expr>) {
        let colons: Vec<_> = self
            .0
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .filter(|t| t.kind() == SyntaxKind::COLON)
            .map(|t| t.text_range().start())
            .collect();
        let Some(&first) = colons.first() else {
            return (None, None, None);
        };
        let second = colons.get(1).copied();

        let mut slots: [Option<Expr>; 3] = [None, None, None];
        for expr in children::<Expr>(&self.0).skip(1) {
            let at = expr.range().start();
            let slot = if at < first {
                0
            } else if second.is_none_or(|s| at < s) {
                1
            } else {
                2
            };
            slots[slot] = Some(expr);
        }
        let [lower, upper, stride] = slots;
        (lower, upper, stride)
    }
}

impl UnaryExpr {
    #[must_use]
    pub fn op(&self) -> Option<SyntaxToken> {
        first_token_matching(&self.0, |k| {
            matches!(
                k,
                SyntaxKind::MINUS | SyntaxKind::PLUS | SyntaxKind::TILDE | SyntaxKind::NOT_KW
            )
        })
    }

    #[must_use]
    pub fn operand(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl BinaryExpr {
    #[must_use]
    pub fn lhs(&self) -> Option<Expr> {
        nth_child(&self.0, 0)
    }

    #[must_use]
    pub fn rhs(&self) -> Option<Expr> {
        nth_child(&self.0, 1)
    }

    #[must_use]
    pub fn op(&self) -> Option<SyntaxToken> {
        let lhs_end = self.lhs().map(|e| e.range().end());
        self.0
            .children_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .find(|t| {
                !t.kind().is_trivia() && lhs_end.is_none_or(|end| t.text_range().start() >= end)
            })
    }
}

impl IfExpr {
    /// The value taken when the condition holds.
    #[must_use]
    pub fn then_branch(&self) -> Option<Expr> {
        nth_child(&self.0, 0)
    }

    #[must_use]
    pub fn condition(&self) -> Option<Expr> {
        nth_child(&self.0, 1)
    }

    #[must_use]
    pub fn else_branch(&self) -> Option<Expr> {
        nth_child(&self.0, 2)
    }
}

impl ListExpr {
    pub fn elements(&self) -> impl Iterator<Item = Expr> + use<> {
        children(&self.0)
    }
}

impl TupleExpr {
    pub fn elements(&self) -> impl Iterator<Item = Expr> + use<> {
        children(&self.0)
    }
}

impl ParenExpr {
    #[must_use]
    pub fn inner(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl DictExpr {
    pub fn entries(&self) -> impl Iterator<Item = DictEntry> + use<> {
        children(&self.0)
    }
}

impl DictEntry {
    #[must_use]
    pub fn key(&self) -> Option<Expr> {
        nth_child(&self.0, 0)
    }

    #[must_use]
    pub fn value(&self) -> Option<Expr> {
        nth_child(&self.0, 1)
    }
}

impl ListComp {
    #[must_use]
    pub fn element(&self) -> Option<Expr> {
        child(&self.0)
    }

    pub fn clauses(&self) -> impl Iterator<Item = CompClause> + use<> {
        children(&self.0)
    }
}

impl DictComp {
    #[must_use]
    pub fn entry(&self) -> Option<DictEntry> {
        child(&self.0)
    }

    pub fn clauses(&self) -> impl Iterator<Item = CompClause> + use<> {
        children(&self.0)
    }
}

impl CompClause {
    /// `for x in y` rather than `if cond`.
    #[must_use]
    pub fn is_for(&self) -> bool {
        token(&self.0, SyntaxKind::FOR_KW).is_some()
    }

    /// The loop variables of a `for` clause.
    #[must_use]
    pub fn targets(&self) -> Option<Expr> {
        self.is_for().then(|| nth_child(&self.0, 0))?
    }

    /// The iterable of a `for` clause, or the test of an `if` clause.
    #[must_use]
    pub fn expr(&self) -> Option<Expr> {
        if self.is_for() {
            nth_child(&self.0, 1)
        } else {
            child(&self.0)
        }
    }
}

impl LambdaExpr {
    pub fn params(&self) -> impl Iterator<Item = Param> + use<> {
        child::<ParamList>(&self.0)
            .into_iter()
            .flat_map(|list| children::<Param>(list.syntax()).collect::<Vec<_>>())
    }

    #[must_use]
    pub fn body(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl TypeAliasStmt {
    #[must_use]
    pub fn name(&self) -> Option<String> {
        Some(token(&self.0, SyntaxKind::IDENT)?.text().to_string())
    }

    #[must_use]
    pub fn value(&self) -> Option<TypeRef> {
        child(&self.0)
    }
}

impl TypeRef {
    #[must_use]
    pub fn expr(&self) -> Option<Expr> {
        child(&self.0)
    }
}

impl LiteralExpr {
    /// The literal's token: `STRING`, `BYTES`, `INT` or `FLOAT`.
    #[must_use]
    pub fn token(&self) -> Option<SyntaxToken> {
        first_token_matching(&self.0, |k| {
            matches!(
                k,
                SyntaxKind::STRING | SyntaxKind::BYTES | SyntaxKind::INT | SyntaxKind::FLOAT
            )
        })
    }

    #[must_use]
    pub fn is_string(&self) -> bool {
        self.token()
            .is_some_and(|t| matches!(t.kind(), SyntaxKind::STRING | SyntaxKind::BYTES))
    }

    /// A string literal's content, quotes and prefix removed. `None` for
    /// non-strings.
    ///
    /// Escapes are left as written: `"a\nb"` yields the four characters
    /// `a`, `\`, `n`, `b`. Interpreting them is the consumer's business,
    /// and a consumer resolving labels wants the raw bytes anyway.
    #[must_use]
    pub fn string_value(&self) -> Option<String> {
        let token = self.token()?;
        self.is_string().then(|| string_value(&token))?
    }

    /// Where a string literal's content sits in the file, excluding quotes and
    /// prefix.
    ///
    /// This is the range to report for a go-to-definition or a document link on
    /// something written inside a string, which in Bazel is where labels live.
    #[must_use]
    pub fn string_value_range(&self) -> Option<TextRange> {
        let token = self.token()?;
        self.is_string().then(|| string_value_range(&token))?
    }
}

fn string_value(token: &SyntaxToken) -> Option<String> {
    let text = token.text();
    let (start, end) = string_content_range(text)?;
    Some(text[start..end].to_string())
}

fn string_value_range(token: &SyntaxToken) -> Option<TextRange> {
    let (start, end) = string_content_range(token.text())?;
    let base = token.text_range().start();
    #[allow(clippy::cast_possible_truncation)]
    Some(TextRange::new(
        base + rowan::TextSize::from(start as u32),
        base + rowan::TextSize::from(end as u32),
    ))
}

fn is_assign_op(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::ASSIGN
            | SyntaxKind::PLUS_ASSIGN
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
