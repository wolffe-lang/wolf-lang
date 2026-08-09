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

    // s09 fences: complete, lossless raw-token groups.
    /// A `{…}` body held as raw tokens; s09 opens it.
    BlockPending,
    /// An initializer expression (`= …` up to TERM) held as raw tokens;
    /// s09 opens it.
    ExprPending,
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
        assert!(SyntaxKind::ExprPending.is_node());
        assert!(SyntaxKind::FnDecl.is_item());
        assert!(!SyntaxKind::Attribute.is_item());
    }
}
