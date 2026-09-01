//! The unified [`SyntaxKind`] enum: every token kind and every node kind
//! in one flat namespace (rowan-shaped). `wolf_parse` owns the lexer
//! `TokenKind` → `SyntaxKind` mapping; this crate only names the kinds.

/// One kind for every token and node in the syntax tree.
///
/// Token kinds come first (through [`SyntaxKind::Missing`]), node kinds
/// after — [`SyntaxKind::is_token`] / [`SyntaxKind::is_node`] classify.
/// `Tombstone` is a parser-internal placeholder for abandoned markers and
/// never appears in a finished tree (the verifier rejects it).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
#[repr(u16)]
#[rustfmt::skip]
pub enum SyntaxKind {
    // ------------------------------------------------------ token kinds --
    /// Parser-internal: an abandoned start marker. Never in trees.
    Tombstone,

    Ident,
    /// `_`, the wildcard.
    Underscore,
    Int,
    Float,
    /// `'a'` — a `char` literal (s121, `[gram.lex.char]`).
    Char,

    // The 50 reserved keywords `[gram.inv.kw]`, one kind each.
    AsKw, AsmKw, AssumeKw, BorrowKw, BreakKw, ComptimeKw, ConstKw, ContinueKw,
    CopyKw, DeferKw, DistinctKw, DynKw, ElseKw, EnumKw, ErrdeferKw, ExportKw,
    ExternKw, FalseKw, FnKw, ForKw, FreezeKw, HandleKw, IfKw, ImplKw, ImportKw,
    InKw, LetKw, LoopKw, MatchKw, MoveKw, MutKw, ProcKw, PubKw, RegionKw,
    ReturnKw, ScopeKw, SelectKw, SharedKw, SpawnKw, StructKw, TakeKw, TraitKw,
    TrueKw, TypeKw, UnsafeKw, UseKw, VarKw, WeakKw, WhenKw, WhileKw,

    /// Contextual `self` receiver: lexes as `Ident`; the parser
    /// reclassifies the kind in receiver position (text is unchanged —
    /// the tree stays lossless).
    SelfKw,

    // Punctuation, mirroring `wolf_lex::Punct`.
    LParen, RParen, LBracket, RBracket, LBrace, RBrace,
    Comma, Dot, DotDot, DotDotEq, Colon, At, Arrow, FatArrow,
    Eq, EqEq, NotEq, Lt, Gt, LtEq, GtEq, Spaceship,
    Plus, Minus, Star, Slash, Percent,
    Amp, AmpAmp, Pipe, PipePipe, Caret, Not, Question, Shl, Shr,
    PlusEq, MinusEq, StarEq, SlashEq, PercentEq,
    AmpEq, PipeEq, CaretEq, ShlEq, ShrEq,

    /// `#[`, the attribute opener.
    PoundBracket,
    /// `#![`, the file-wide (inner) attribute opener.
    PoundBangBracket,
    /// Statement terminator (inserted newline or explicit `;`).
    Term,

    // String episode tokens (payloads dropped; derivable from source).
    StrBegin, StrFragment, InterpOpen, InterpClose, FormatSpecBegin, StrEnd,

    /// Bytes that form no token (lexer error recovery).
    ErrorToken,
    /// Zero-width completion marker; owns end-of-file trivia.
    Eof,
    /// Zero-width marker inserted where a required token was missing.
    Missing,

    // ------------------------------------------------------- node kinds --
    SourceFile,
    /// Skipped tokens from panic-mode recovery. An ordinary node — no
    /// byte is ever dropped.
    ErrorNode,

    // Items.
    FnDecl, UseDecl, ImportCDecl, TypeDecl, StructDecl, EnumDecl,
    TraitDecl, ImplDecl, LetDecl, VarDecl, ConstDecl,

    // Item pieces.
    Attribute, AttrItem, AttrInput, Visibility, Path, UseGroup,
    /// `#![attr, …]` — the file-wide attribute (`[gram.attr.index]`);
    /// a direct child of `SourceFile`, position-enforced by the parser.
    InnerAttribute,
    GenericParamList, GenericParam, TypeBound,
    ParamList, Param, ViewSet, RetType,
    StructDef, EnumDef, StructField, EnumVariant,
    ErrorRow, RowEntry,
    /// One whole string episode (`StrBegin … StrEnd`) in a declaration
    /// position (`extern "c"`, `import c "hdr"`, attribute literals).
    StringLit,

    // Types `[gram.type]`.
    PathType, TypeArgList,
    /// A `[…]` argument that is expression-shaped (const generics):
    /// parked as a raw token group, opened by sema (D29).
    TypeArgPending,
    ErrorUnionType, PrefixType, PtrType, DynType, TupleType, FnType,
    /// The keyword `type` used as a type.
    TypeType,
    /// The keyword `region` used as a type (X4).
    RegionType,

    // Patterns `[gram.pat]`.
    WildcardPat, LiteralPat, IdentPat, PathPat, TuplePat, OrPat, BindingPat,
    /// `path '{' field_pat … '}'` (`[gram.pat.struct]`, s129 #179).
    StructPat,
    /// One `IDENT (':' pattern)?` of a struct pattern. The shorthand
    /// holds a single [`SyntaxKind::IdentPat`] child (the binding IS
    /// the field name); the explicit form holds the field-name token,
    /// `:`, and the sub-pattern node.
    FieldPat,
    /// The `..` rest marker closing a struct pattern's field list —
    /// its own node (not a pattern kind: it binds nothing and may not
    /// nest) so the formatter's list machinery sees it as a member.
    RestPat,

    /// One `pattern (':' type)? '=' expr` member of a comma-grouped
    /// `let`/`var` (D63). Present only when the declaration carries
    /// two or more binders; a single-binder declaration keeps its
    /// flat shape.
    Binder,

    // Statements `[gram.expr.block]` (s09).
    /// `expr TERM` (or a trailing block expression — no `Term` child).
    ExprStmt,
    /// `place assign_op expr TERM` — statement-only `[gram.expr.assign]`.
    AssignStmt,
    /// `defer expr TERM` / `errdefer expr TERM` (keyword child decides).
    DeferStmt,
    /// `assume noalias expr (',' expr)+ TERM` `[gram.expr.unsafe]`.
    AssumeStmt,

    // Expressions `[gram.expr]` (s09).
    /// A single identifier in expression position (dots build
    /// `MemberExpr` chains — member position is keyword-transparent).
    PathExpr,
    /// `INT | FLOAT | true | false`.
    LiteralExpr,
    /// One whole string episode in expression position; interpolations
    /// are parsed as `Interp` children `[gram.lex.str]`.
    StringExpr,
    /// `{ expr format_spec? }` inside a string.
    Interp,
    /// The `: …` format spec inside an interpolation.
    FormatSpec,
    /// `( expr )` grouping.
    ParenExpr,
    /// `( expr, … )` tuple construction (incl. `(a,)`).
    TupleExpr,
    /// `{ stmt* expr? }` — blocks are expressions `[gram.expr.block]`.
    Block,
    /// `! - & &mut * move copy shared` prefix operators (tier 3).
    PrefixExpr,
    /// One binary operator application, tiers 5–13 `[gram.expr.prec]`.
    BinExpr,
    /// `expr as type` (tier 4).
    CastExpr,
    /// `a..b`, `a..=b`, `..b`, `a..` (tier 14).
    RangeExpr,
    /// `^n` — end-relative range endpoint (D25).
    FromEndExpr,
    /// Postfix `?` (max binding power, D30).
    TryExpr,
    /// `callee(args)`.
    CallExpr,
    /// `expr[args]` — ONE postfix form covering indexing and generic
    /// application; sema resolves which (D29, `[gram.amb.brackets]`).
    BracketApply,
    /// The delimited argument list of a call or bracket apply.
    ArgList,
    /// One argument: `('mut' | 'take')? expr` or a type-only form.
    Arg,
    /// `expr . member` (member: IDENT, INT, or any keyword).
    MemberExpr,
    /// `path { field_init, … }` `[gram.expr.primary]`.
    StructLit,
    /// `IDENT (':' expr)?` in a struct literal.
    FieldInit,
    // Control flow as expressions `[gram.expr.flow]`.
    IfExpr, MatchExpr, MatchArm,
    /// The `if expr` guard of a match arm.
    MatchGuard,
    ForExpr, WhileExpr, LoopExpr,
    /// `fn(a, b) expr` / `fn(a, b) { … }` `[gram.expr.closure]`.
    ClosureExpr,
    ReturnExpr, BreakExpr, ContinueExpr,
    /// `expr else …` — the defaulting operator (tier 15, D30).
    ElseExpr,
    // Regions `[gram.expr.region]` — sugar vs value forms are distinct.
    /// `region name? (':' strategy)? { … }` block sugar (X4).
    RegionBlock,
    /// `region(strategy?)` first-class value form (X4).
    RegionValue,
    /// `rc` / `pool(T)`.
    RegionStrategy,
    /// `cap ':' expr` — the creation-time byte budget
    /// (`[mem.region.cap.1]`, D68/#187): parenthesized after the
    /// sugar form's name (`region r(cap: N) { … }`), last in the
    /// value form's parens (`region(cap: N)` / `region(rc, cap: N)`).
    RegionCap,
    /// `in expr { … }`.
    InBlock,
    /// `freeze expr`.
    FreezeExpr,
    // Concurrency surface `[gram.expr.conc]`.
    ScopeExpr, SelectExpr, SelectArm, WhenExpr, SpawnExpr,
    // Unsafe tier `[gram.expr.unsafe]`.
    UnsafeBlock,
    /// `unsafe c capture_list? { … }` — inline C.
    InlineC,
    /// The opaque brace-balanced token body of an `unsafe c` block.
    InlineCBody,
    /// `[a, b]` capture list of an inline-C block.
    CaptureList,
    AsmExpr, AsmOperand,
    /// `borrow expr from expr`.
    BorrowExpr,
}

impl SyntaxKind {
    /// Is this a token kind (leaf)? `Tombstone` is neither.
    pub fn is_token(self) -> bool {
        self > SyntaxKind::Tombstone && self <= SyntaxKind::Missing
    }

    /// Is this a node kind (interior)?
    pub fn is_node(self) -> bool {
        self >= SyntaxKind::SourceFile
    }

    /// Is this one of the item (declaration) node kinds?
    pub fn is_item(self) -> bool {
        matches!(
            self,
            SyntaxKind::FnDecl
                | SyntaxKind::UseDecl
                | SyntaxKind::ImportCDecl
                | SyntaxKind::TypeDecl
                | SyntaxKind::StructDecl
                | SyntaxKind::EnumDecl
                | SyntaxKind::TraitDecl
                | SyntaxKind::ImplDecl
                | SyntaxKind::LetDecl
                | SyntaxKind::VarDecl
                | SyntaxKind::ConstDecl
        )
    }
}

#[cfg(test)]
mod tests {
    use super::SyntaxKind;

    #[test]
    fn token_node_partition() {
        assert!(!SyntaxKind::Tombstone.is_token());
        assert!(!SyntaxKind::Tombstone.is_node());
        assert!(SyntaxKind::Ident.is_token());
        assert!(SyntaxKind::Missing.is_token());
        assert!(!SyntaxKind::Missing.is_node());
        assert!(SyntaxKind::SourceFile.is_node());
        assert!(!SyntaxKind::SourceFile.is_token());
        assert!(SyntaxKind::BorrowExpr.is_node());
        assert!(SyntaxKind::Block.is_node());
        assert!(SyntaxKind::FnDecl.is_item());
        assert!(!SyntaxKind::Attribute.is_item());
    }
}
