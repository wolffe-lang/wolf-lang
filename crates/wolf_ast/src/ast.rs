//! Typed accessors over the green tree. Every accessor returns `Option`
//! — broken trees are ordinary trees (D22), so any child may be missing
//! and downstream phases are written against that honesty from day one.

use crate::green::{GreenNode, GreenToken};
use crate::kind::SyntaxKind;

/// Parameter passing mode (D10 Tier 0). The default `read` mode has no
/// keyword — its absence *is* the syntax — so accessors return
/// `Option<ParamMode>` with `None` meaning `read`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParamMode {
    Mut,
    Take,
}

/// Is `kind` one of the type node kinds (`[gram.type]`)?
pub fn is_type_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::PathType
            | SyntaxKind::ErrorUnionType
            | SyntaxKind::PrefixType
            | SyntaxKind::PtrType
            | SyntaxKind::DynType
            | SyntaxKind::TupleType
            | SyntaxKind::FnType
            | SyntaxKind::TypeType
            | SyntaxKind::RegionType
    )
}

/// Is `kind` one of the pattern node kinds (`[gram.pat]`)?
pub fn is_pattern_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::WildcardPat
            | SyntaxKind::LiteralPat
            | SyntaxKind::IdentPat
            | SyntaxKind::PathPat
            | SyntaxKind::TuplePat
            | SyntaxKind::OrPat
            | SyntaxKind::BindingPat
    )
}

/// Is `kind` one of the expression node kinds (`[gram.expr]`)?
pub fn is_expr_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::PathExpr
            | SyntaxKind::LiteralExpr
            | SyntaxKind::StringExpr
            | SyntaxKind::ParenExpr
            | SyntaxKind::TupleExpr
            | SyntaxKind::Block
            | SyntaxKind::PrefixExpr
            | SyntaxKind::BinExpr
            | SyntaxKind::CastExpr
            | SyntaxKind::RangeExpr
            | SyntaxKind::FromEndExpr
            | SyntaxKind::TryExpr
            | SyntaxKind::CallExpr
            | SyntaxKind::BracketApply
            | SyntaxKind::MemberExpr
            | SyntaxKind::StructLit
            | SyntaxKind::IfExpr
            | SyntaxKind::MatchExpr
            | SyntaxKind::ForExpr
            | SyntaxKind::WhileExpr
            | SyntaxKind::LoopExpr
            | SyntaxKind::ClosureExpr
            | SyntaxKind::ReturnExpr
            | SyntaxKind::BreakExpr
            | SyntaxKind::ContinueExpr
            | SyntaxKind::ElseExpr
            | SyntaxKind::RegionBlock
            | SyntaxKind::RegionValue
            | SyntaxKind::InBlock
            | SyntaxKind::FreezeExpr
            | SyntaxKind::ScopeExpr
            | SyntaxKind::SelectExpr
            | SyntaxKind::WhenExpr
            | SyntaxKind::SpawnExpr
            | SyntaxKind::UnsafeBlock
            | SyntaxKind::InlineC
            | SyntaxKind::AsmExpr
            | SyntaxKind::BorrowExpr
    )
}

/// Is `kind` one of the statement node kinds (`[gram.expr.block]`)?
/// Item kinds also occur in statement position (nested declarations).
pub fn is_stmt_kind(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::ExprStmt
            | SyntaxKind::AssignStmt
            | SyntaxKind::DeferStmt
            | SyntaxKind::AssumeStmt
    ) || kind.is_item()
}

fn first_type(node: &GreenNode) -> Option<&GreenNode> {
    node.nodes().find(|n| is_type_kind(n.kind))
}

fn first_pattern(node: &GreenNode) -> Option<&GreenNode> {
    node.nodes().find(|n| is_pattern_kind(n.kind))
}

fn first_expr(node: &GreenNode) -> Option<&GreenNode> {
    node.nodes().find(|n| is_expr_kind(n.kind))
}

fn nth_expr(node: &GreenNode, n: usize) -> Option<&GreenNode> {
    node.nodes().filter(|c| is_expr_kind(c.kind)).nth(n)
}

macro_rules! ast_node {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug)]
        pub struct $name<'a>(&'a GreenNode);

        impl<'a> $name<'a> {
            pub fn cast(node: &'a GreenNode) -> Option<Self> {
                (node.kind == SyntaxKind::$name).then_some(Self(node))
            }

            pub fn syntax(self) -> &'a GreenNode {
                self.0
            }
        }
    };
}

macro_rules! common_item_accessors {
    ($name:ident) => {
        impl<'a> $name<'a> {
            /// The `attribute*` prefixing this item.
            pub fn attributes(self) -> impl Iterator<Item = Attribute<'a>> {
                self.0.nodes().filter_map(Attribute::cast)
            }

            /// The `pub` / `pub(pkg)` visibility, if any.
            pub fn visibility(self) -> Option<Visibility<'a>> {
                self.0.nodes().find_map(Visibility::cast)
            }
        }
    };
}

// ---------------------------------------------------------------- items --

ast_node!(
    /// `fn` item `[gram.item.fn]`.
    FnDecl
);
common_item_accessors!(FnDecl);

impl<'a> FnDecl<'a> {
    /// The function name — the first direct `Ident` token (qualifier
    /// strings and generic names live inside child nodes).
    pub fn name(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }

    pub fn generics(self) -> Option<GenericParamList<'a>> {
        self.0.nodes().find_map(GenericParamList::cast)
    }

    pub fn params(self) -> Option<ParamList<'a>> {
        self.0.nodes().find_map(ParamList::cast)
    }

    pub fn ret_ty(self) -> Option<RetType<'a>> {
        self.0.nodes().find_map(RetType::cast)
    }

    /// The `{…}` body block. `None` for bodyless (`extern`) forms.
    pub fn body(self) -> Option<Block<'a>> {
        self.0.nodes().find_map(Block::cast)
    }

    pub fn is_comptime(self) -> bool {
        self.0.child_token(SyntaxKind::ComptimeKw).is_some()
    }

    pub fn is_export(self) -> bool {
        self.0.child_token(SyntaxKind::ExportKw).is_some()
    }

    /// The ABI string of an `extern "…"` qualifier.
    pub fn extern_abi(self) -> Option<StringLit<'a>> {
        self.0.child_token(SyntaxKind::ExternKw)?;
        self.0.nodes().find_map(StringLit::cast)
    }
}

ast_node!(
    /// `use` item `[gram.item.use]`.
    UseDecl
);
common_item_accessors!(UseDecl);

impl<'a> UseDecl<'a> {
    pub fn path(self) -> Option<Path<'a>> {
        self.0.nodes().find_map(Path::cast)
    }

    /// The `.{a, b}` group, if any.
    pub fn group(self) -> Option<UseGroup<'a>> {
        self.0.nodes().find_map(UseGroup::cast)
    }

    /// The `as IDENT` alias, if any: the `Ident` token following `as`.
    pub fn alias(self) -> Option<&'a GreenToken> {
        let mut seen_as = false;
        for t in self.0.tokens() {
            if seen_as && t.kind == SyntaxKind::Ident {
                return Some(t);
            }
            if t.kind == SyntaxKind::AsKw {
                seen_as = true;
            }
        }
        None
    }
}

ast_node!(
    /// `import c "header"` item `[gram.item.use]`.
    ImportCDecl
);
common_item_accessors!(ImportCDecl);

impl<'a> ImportCDecl<'a> {
    /// The header-name string literal.
    pub fn header(self) -> Option<StringLit<'a>> {
        self.0.nodes().find_map(StringLit::cast)
    }
}

ast_node!(
    /// `type X = …` item `[gram.item.type]`.
    TypeDecl
);
common_item_accessors!(TypeDecl);

impl<'a> TypeDecl<'a> {
    pub fn name(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }

    pub fn generics(self) -> Option<GenericParamList<'a>> {
        self.0.nodes().find_map(GenericParamList::cast)
    }

    /// The right-hand side: a `StructDef`, `EnumDef`, or type node.
    pub fn def(self) -> Option<&'a GreenNode> {
        self.0.nodes().find(|n| {
            matches!(n.kind, SyntaxKind::StructDef | SyntaxKind::EnumDef) || is_type_kind(n.kind)
        })
    }
}

ast_node!(
    /// `struct Name { … }` item sugar `[gram.item.type]`.
    StructDecl
);
common_item_accessors!(StructDecl);

impl<'a> StructDecl<'a> {
    pub fn name(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }

    pub fn generics(self) -> Option<GenericParamList<'a>> {
        self.0.nodes().find_map(GenericParamList::cast)
    }

    pub fn fields(self) -> impl Iterator<Item = StructField<'a>> {
        self.0.nodes().filter_map(StructField::cast)
    }
}

ast_node!(
    /// `enum Name { … }` item sugar `[gram.item.type]`.
    EnumDecl
);
common_item_accessors!(EnumDecl);

impl<'a> EnumDecl<'a> {
    pub fn name(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }

    pub fn generics(self) -> Option<GenericParamList<'a>> {
        self.0.nodes().find_map(GenericParamList::cast)
    }

    pub fn variants(self) -> impl Iterator<Item = EnumVariant<'a>> {
        self.0.nodes().filter_map(EnumVariant::cast)
    }
}

ast_node!(
    /// `trait` item `[gram.item.trait]`.
    TraitDecl
);
common_item_accessors!(TraitDecl);

impl<'a> TraitDecl<'a> {
    pub fn name(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }

    pub fn generics(self) -> Option<GenericParamList<'a>> {
        self.0.nodes().find_map(GenericParamList::cast)
    }

    /// Member items, parsed re-entrantly by the item parser.
    pub fn members(self) -> impl Iterator<Item = &'a GreenNode> {
        self.0.nodes().filter(|n| n.kind.is_item())
    }
}

ast_node!(
    /// `impl` item `[gram.item.trait]`.
    ImplDecl
);
common_item_accessors!(ImplDecl);

impl<'a> ImplDecl<'a> {
    pub fn generics(self) -> Option<GenericParamList<'a>> {
        self.0.nodes().find_map(GenericParamList::cast)
    }

    /// The trait (or inherent-impl target) path.
    pub fn trait_path(self) -> Option<Path<'a>> {
        self.0.nodes().find_map(Path::cast)
    }

    /// The `for T` self type, if any.
    pub fn self_ty(self) -> Option<&'a GreenNode> {
        first_type(self.0)
    }

    /// Member items, parsed re-entrantly by the item parser.
    pub fn members(self) -> impl Iterator<Item = &'a GreenNode> {
        self.0.nodes().filter(|n| n.kind.is_item())
    }
}

ast_node!(
    /// `let` binding `[gram.item.let]`.
    LetDecl
);
common_item_accessors!(LetDecl);

ast_node!(
    /// `var` binding `[gram.item.let]`.
    VarDecl
);
common_item_accessors!(VarDecl);

ast_node!(
    /// One `pattern (':' type)? '=' expr` member of a comma-grouped
    /// `let`/`var` (D63, `[gram.item.let]`). Present only when the
    /// declaration carries two or more binders.
    Binder
);

impl<'a> Binder<'a> {
    pub fn pattern(self) -> Option<&'a GreenNode> {
        first_pattern(self.0)
    }

    /// The `: T` ascription, if any.
    pub fn ty(self) -> Option<&'a GreenNode> {
        first_type(self.0)
    }

    /// The `= …` initializer expression.
    pub fn init(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }
}

/// One binder of a `let`/`var`, shape-agnostic: `node` is the
/// `Binder` node when the declaration is a comma group, or the whole
/// declaration when it is flat. Semantics of a group are the sequence
/// of single bindings, left to right (D63).
#[derive(Clone, Copy, Debug)]
pub struct BinderParts<'a> {
    pub node: &'a GreenNode,
    pub pattern: Option<&'a GreenNode>,
    pub ty: Option<&'a GreenNode>,
    pub init: Option<&'a GreenNode>,
}

/// Every binder of a `let`/`var` declaration node, in source order —
/// the kind-agnostic spelling of [`LetDecl::binders`]/
/// [`VarDecl::binders`] for callers holding a matched `GreenNode`.
pub fn binding_binders(node: &GreenNode) -> Vec<BinderParts<'_>> {
    debug_assert!(matches!(
        node.kind,
        SyntaxKind::LetDecl | SyntaxKind::VarDecl
    ));
    binder_parts(node)
}

fn binder_parts(node: &GreenNode) -> Vec<BinderParts<'_>> {
    let groups: Vec<&GreenNode> = node
        .nodes()
        .filter(|n| n.kind == SyntaxKind::Binder)
        .collect();
    if groups.is_empty() {
        vec![BinderParts {
            node,
            pattern: first_pattern(node),
            ty: first_type(node),
            init: first_expr(node),
        }]
    } else {
        groups
            .into_iter()
            .map(|b| BinderParts {
                node: b,
                pattern: first_pattern(b),
                ty: first_type(b),
                init: first_expr(b),
            })
            .collect()
    }
}

macro_rules! binding_accessors {
    ($name:ident) => {
        impl<'a> $name<'a> {
            /// The first (or only) binder's pattern. A comma group has
            /// more — walk `binders()` to see every one.
            pub fn pattern(self) -> Option<&'a GreenNode> {
                self.binders().first().and_then(|b| b.pattern)
            }

            /// The first (or only) binder's `: T` ascription, if any.
            pub fn ty(self) -> Option<&'a GreenNode> {
                self.binders().first().and_then(|b| b.ty)
            }

            /// The first (or only) binder's `= …` initializer.
            pub fn init(self) -> Option<&'a GreenNode> {
                self.binders().first().and_then(|b| b.init)
            }

            /// Every binder in source order (D63): the single flat
            /// binder, or each `Binder` member of a comma group.
            pub fn binders(self) -> Vec<BinderParts<'a>> {
                binder_parts(self.0)
            }
        }
    };
}

binding_accessors!(LetDecl);
binding_accessors!(VarDecl);

ast_node!(
    /// `const` item `[gram.item.let]`.
    ConstDecl
);
common_item_accessors!(ConstDecl);

impl<'a> ConstDecl<'a> {
    pub fn name(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }

    pub fn ty(self) -> Option<&'a GreenNode> {
        first_type(self.0)
    }

    pub fn init(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }
}

// ---------------------------------------------------------- item pieces --

ast_node!(
    /// `#[attr, …]` `[gram.item.attr]`.
    Attribute
);

impl<'a> Attribute<'a> {
    pub fn items(self) -> impl Iterator<Item = AttrItem<'a>> {
        self.0.nodes().filter_map(AttrItem::cast)
    }
}

ast_node!(
    /// `#![attr, …]` — the file-wide attribute (`[gram.attr.index]`);
    /// a direct child of the source file, before any item.
    InnerAttribute
);

impl<'a> InnerAttribute<'a> {
    pub fn items(self) -> impl Iterator<Item = AttrItem<'a>> {
        self.0.nodes().filter_map(AttrItem::cast)
    }
}

ast_node!(
    /// One `path attr_input?` inside an attribute.
    AttrItem
);

impl<'a> AttrItem<'a> {
    pub fn path(self) -> Option<Path<'a>> {
        self.0.nodes().find_map(Path::cast)
    }

    pub fn input(self) -> Option<&'a GreenNode> {
        self.0.child_node(SyntaxKind::AttrInput)
    }
}

ast_node!(
    /// `pub` / `pub(pkg)` `[gram.item.unit]`.
    Visibility
);

impl<'a> Visibility<'a> {
    /// Is this the package-scoped `pub(pkg)` form?
    pub fn is_pkg(self) -> bool {
        self.0.child_token(SyntaxKind::LParen).is_some()
    }
}

ast_node!(
    /// `IDENT ('.' IDENT)*`.
    Path
);

impl<'a> Path<'a> {
    pub fn segments(self) -> impl Iterator<Item = &'a GreenToken> {
        self.0.tokens().filter(|t| t.kind == SyntaxKind::Ident)
    }
}

ast_node!(
    /// The `{a, b}` of a `use` group.
    UseGroup
);

impl<'a> UseGroup<'a> {
    pub fn names(self) -> impl Iterator<Item = &'a GreenToken> {
        self.0.tokens().filter(|t| t.kind == SyntaxKind::Ident)
    }
}

ast_node!(
    /// `[T, U: Bound, N: type]` `[gram.item.fn]`.
    GenericParamList
);

impl<'a> GenericParamList<'a> {
    pub fn params(self) -> impl Iterator<Item = GenericParam<'a>> {
        self.0.nodes().filter_map(GenericParam::cast)
    }
}

ast_node!(
    /// One generic parameter.
    GenericParam
);

impl<'a> GenericParam<'a> {
    pub fn name(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }

    /// The `: Bound (+ Bound)*` constraint, if any.
    pub fn bound(self) -> Option<TypeBound<'a>> {
        self.0.nodes().find_map(TypeBound::cast)
    }

    /// Is this the `IDENT : type` comptime-type-parameter form?
    pub fn is_type_param(self) -> bool {
        self.0.child_token(SyntaxKind::TypeKw).is_some()
    }
}

ast_node!(
    /// `path ('+' path)*`.
    TypeBound
);

impl<'a> TypeBound<'a> {
    pub fn paths(self) -> impl Iterator<Item = Path<'a>> {
        self.0.nodes().filter_map(Path::cast)
    }
}

ast_node!(
    /// `(param, …)`.
    ParamList
);

impl<'a> ParamList<'a> {
    pub fn params(self) -> impl Iterator<Item = Param<'a>> {
        self.0.nodes().filter_map(Param::cast)
    }
}

ast_node!(
    /// One parameter: `mode? IDENT : type` or `mode? self view_set?`.
    Param
);

impl<'a> Param<'a> {
    /// `mut` / `take`; `None` is the default `read` mode.
    pub fn mode(self) -> Option<ParamMode> {
        self.0.tokens().find_map(|t| match t.kind {
            SyntaxKind::MutKw => Some(ParamMode::Mut),
            SyntaxKind::TakeKw => Some(ParamMode::Take),
            _ => None,
        })
    }

    /// Is this a `self` receiver parameter?
    pub fn is_self(self) -> bool {
        self.0.child_token(SyntaxKind::SelfKw).is_some()
    }

    pub fn name(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }

    pub fn ty(self) -> Option<&'a GreenNode> {
        first_type(self.0)
    }

    /// The `.{a, b}` view set on a `self` receiver.
    pub fn view_set(self) -> Option<ViewSet<'a>> {
        self.0.nodes().find_map(ViewSet::cast)
    }
}

ast_node!(
    /// `.{a, b}` — field-granular exclusivity on a receiver.
    ViewSet
);

impl<'a> ViewSet<'a> {
    pub fn fields(self) -> impl Iterator<Item = &'a GreenToken> {
        self.0.tokens().filter(|t| t.kind == SyntaxKind::Ident)
    }
}

ast_node!(
    /// `-> type ('!' error_row)?`.
    RetType
);

impl<'a> RetType<'a> {
    pub fn ty(self) -> Option<&'a GreenNode> {
        first_type(self.0)
    }

    /// The explicit `! {row}` after the type, if any.
    pub fn error_row(self) -> Option<ErrorRow<'a>> {
        self.0.nodes().find_map(ErrorRow::cast)
    }

    /// Every `! {row}` tail, in order (#34: `-> T ! {a} ! {b}` parses
    /// with one row child per tail; more than one is a nested union,
    /// which sema refuses by name until the spec rules its meaning).
    pub fn error_rows(self) -> impl Iterator<Item = ErrorRow<'a>> {
        self.0.nodes().filter_map(ErrorRow::cast)
    }
}

ast_node!(
    /// Anonymous `struct { … }` in a `type` RHS.
    StructDef
);

impl<'a> StructDef<'a> {
    pub fn fields(self) -> impl Iterator<Item = StructField<'a>> {
        self.0.nodes().filter_map(StructField::cast)
    }
}

ast_node!(
    /// Anonymous `enum { … }` in a `type` RHS.
    EnumDef
);

impl<'a> EnumDef<'a> {
    pub fn variants(self) -> impl Iterator<Item = EnumVariant<'a>> {
        self.0.nodes().filter_map(EnumVariant::cast)
    }
}

ast_node!(
    /// `attribute* visibility? IDENT : type`.
    StructField
);
common_item_accessors!(StructField);

impl<'a> StructField<'a> {
    pub fn name(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }

    pub fn ty(self) -> Option<&'a GreenNode> {
        first_type(self.0)
    }
}

ast_node!(
    /// `IDENT ('(' type, … ')')?`.
    EnumVariant
);

impl<'a> EnumVariant<'a> {
    pub fn name(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }

    /// Payload types, in order (empty for payload-less variants).
    pub fn payload(self) -> impl Iterator<Item = &'a GreenNode> {
        self.0.nodes().filter(|n| is_type_kind(n.kind))
    }
}

ast_node!(
    /// `{ path(payload)?, …, ..? }` — an explicit error row (D30).
    ErrorRow
);

impl<'a> ErrorRow<'a> {
    pub fn entries(self) -> impl Iterator<Item = RowEntry<'a>> {
        self.0.nodes().filter_map(RowEntry::cast)
    }

    /// Does the row end with the `..` open marker?
    pub fn is_open(self) -> bool {
        self.0.child_token(SyntaxKind::DotDot).is_some()
    }
}

ast_node!(
    /// One row entry: `path ('(' type, … ')')?`.
    RowEntry
);

impl<'a> RowEntry<'a> {
    pub fn path(self) -> Option<Path<'a>> {
        self.0.nodes().find_map(Path::cast)
    }

    pub fn payload(self) -> impl Iterator<Item = &'a GreenNode> {
        self.0.nodes().filter(|n| is_type_kind(n.kind))
    }
}

ast_node!(
    /// One whole string episode in a declaration position.
    StringLit
);

// ---------------------------------------------------------------- types --

ast_node!(
    /// `path type_args?`.
    PathType
);

impl<'a> PathType<'a> {
    pub fn path(self) -> Option<Path<'a>> {
        self.0.nodes().find_map(Path::cast)
    }

    pub fn args(self) -> Option<TypeArgList<'a>> {
        self.0.nodes().find_map(TypeArgList::cast)
    }
}

ast_node!(
    /// `[arg, …]` type application.
    TypeArgList
);

impl<'a> TypeArgList<'a> {
    /// Arguments: type nodes, or `TypeArgPending` groups for
    /// expression-shaped const-generic arguments.
    pub fn args(self) -> impl Iterator<Item = &'a GreenNode> {
        self.0
            .nodes()
            .filter(|n| is_type_kind(n.kind) || n.kind == SyntaxKind::TypeArgPending)
    }
}

ast_node!(
    /// `!T` — error union with inferred row (D30).
    ErrorUnionType
);

impl<'a> ErrorUnionType<'a> {
    pub fn ty(self) -> Option<&'a GreenNode> {
        first_type(self.0)
    }
}

ast_node!(
    /// `shared T` / `handle T` / `weak T` / `distinct T`.
    PrefixType
);

impl<'a> PrefixType<'a> {
    /// The prefix keyword token.
    pub fn prefix(self) -> Option<&'a GreenToken> {
        self.0.tokens().find(|t| {
            matches!(
                t.kind,
                SyntaxKind::SharedKw
                    | SyntaxKind::HandleKw
                    | SyntaxKind::WeakKw
                    | SyntaxKind::DistinctKw
            )
        })
    }

    pub fn ty(self) -> Option<&'a GreenNode> {
        first_type(self.0)
    }
}

ast_node!(
    /// `*T` — raw pointer, unsafe tier.
    PtrType
);

impl<'a> PtrType<'a> {
    pub fn ty(self) -> Option<&'a GreenNode> {
        first_type(self.0)
    }
}

ast_node!(
    /// `dyn Path`.
    DynType
);

impl<'a> DynType<'a> {
    pub fn path(self) -> Option<Path<'a>> {
        self.0.nodes().find_map(Path::cast)
    }
}

ast_node!(
    /// `(T, U)` (also `(T)` grouping).
    TupleType
);

impl<'a> TupleType<'a> {
    pub fn elems(self) -> impl Iterator<Item = &'a GreenNode> {
        self.0.nodes().filter(|n| is_type_kind(n.kind))
    }
}

ast_node!(
    /// `fn(T, U) -> R`.
    FnType
);

impl<'a> FnType<'a> {
    pub fn params(self) -> impl Iterator<Item = &'a GreenNode> {
        self.0.nodes().filter(|n| is_type_kind(n.kind))
    }

    pub fn ret(self) -> Option<RetType<'a>> {
        self.0.nodes().find_map(RetType::cast)
    }
}

// ----------------------------------------------- statements (s09) --------

ast_node!(
    /// `{ stmt* expr? }` — blocks are expressions `[gram.expr.block]`.
    Block
);

impl<'a> Block<'a> {
    /// Statements, in order (item declarations included).
    pub fn statements(self) -> impl Iterator<Item = &'a GreenNode> {
        self.0.nodes().filter(|n| is_stmt_kind(n.kind))
    }

    /// The block's value: the final statement's expression, when the
    /// final statement is an [`ExprStmt`] (blocks evaluate to their
    /// final expression — `[gram.expr.block]`). The terminator tokens
    /// only decide where statements *split*; a newline before `}` does
    /// not demote the value.
    pub fn trailing_expr(self) -> Option<&'a GreenNode> {
        ExprStmt::cast(self.statements().last()?)?.expr()
    }
}

ast_node!(
    /// `expr TERM?`.
    ExprStmt
);

impl<'a> ExprStmt<'a> {
    pub fn expr(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }

    /// The `Term` token, absent for a trailing block expression.
    pub fn terminator(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Term)
    }
}

ast_node!(
    /// `place assign_op expr TERM` `[gram.expr.assign]`.
    AssignStmt
);

impl<'a> AssignStmt<'a> {
    /// The place expression (validity is sema's check, not grammar's).
    pub fn place(self) -> Option<&'a GreenNode> {
        nth_expr(self.0, 0)
    }

    /// The assignment (or compound-assignment) operator token.
    pub fn op(self) -> Option<&'a GreenToken> {
        self.0.tokens().find(|t| {
            matches!(
                t.kind,
                SyntaxKind::Eq
                    | SyntaxKind::PlusEq
                    | SyntaxKind::MinusEq
                    | SyntaxKind::StarEq
                    | SyntaxKind::SlashEq
                    | SyntaxKind::PercentEq
                    | SyntaxKind::AmpEq
                    | SyntaxKind::PipeEq
                    | SyntaxKind::CaretEq
                    | SyntaxKind::ShlEq
                    | SyntaxKind::ShrEq
            )
        })
    }

    pub fn value(self) -> Option<&'a GreenNode> {
        nth_expr(self.0, 1)
    }
}

ast_node!(
    /// `defer expr` / `errdefer expr`.
    DeferStmt
);

impl<'a> DeferStmt<'a> {
    /// Is this the `errdefer` (error-path-only) form?
    pub fn is_errdefer(self) -> bool {
        self.0.child_token(SyntaxKind::ErrdeferKw).is_some()
    }

    pub fn expr(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }
}

ast_node!(
    /// `assume noalias expr (',' expr)+`.
    AssumeStmt
);

impl<'a> AssumeStmt<'a> {
    pub fn exprs(self) -> impl Iterator<Item = &'a GreenNode> {
        self.0.nodes().filter(|n| is_expr_kind(n.kind))
    }
}

// ---------------------------------------------- expressions (s09) --------

ast_node!(
    /// A single identifier in expression position.
    PathExpr
);

impl<'a> PathExpr<'a> {
    pub fn ident(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }
}

ast_node!(
    /// `INT | FLOAT | true | false`.
    LiteralExpr
);

ast_node!(
    /// One whole string episode in expression position.
    StringExpr
);

impl<'a> StringExpr<'a> {
    pub fn interps(self) -> impl Iterator<Item = Interp<'a>> {
        self.0.nodes().filter_map(Interp::cast)
    }
}

ast_node!(
    /// `{ expr format_spec? }` inside a string.
    Interp
);

impl<'a> Interp<'a> {
    pub fn expr(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }

    pub fn format_spec(self) -> Option<&'a GreenNode> {
        self.0.child_node(SyntaxKind::FormatSpec)
    }
}

ast_node!(
    /// `( expr )`.
    ParenExpr
);

impl<'a> ParenExpr<'a> {
    pub fn expr(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }

    /// The X1 receiver-mode marker (`(mut p)` / `(take p)`), if any.
    /// A moded paren is legal only in method-receiver position
    /// ([gram.expr.primary]); the parser enforces the position.
    pub fn mode(self) -> Option<ParamMode> {
        self.0.tokens().find_map(|t| match t.kind {
            SyntaxKind::MutKw => Some(ParamMode::Mut),
            SyntaxKind::TakeKw => Some(ParamMode::Take),
            _ => None,
        })
    }
}

ast_node!(
    /// `( expr, … )`.
    TupleExpr
);

impl<'a> TupleExpr<'a> {
    pub fn elems(self) -> impl Iterator<Item = &'a GreenNode> {
        self.0.nodes().filter(|n| is_expr_kind(n.kind))
    }
}

ast_node!(
    /// A tier-3 prefix operator application.
    PrefixExpr
);

impl<'a> PrefixExpr<'a> {
    /// The operator token (`! - & * move copy shared`; `&mut` carries a
    /// `MutKw` token too).
    pub fn op(self) -> Option<&'a GreenToken> {
        self.0.tokens().next()
    }

    pub fn operand(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }
}

ast_node!(
    /// One binary operator application.
    BinExpr
);

impl<'a> BinExpr<'a> {
    pub fn lhs(self) -> Option<&'a GreenNode> {
        nth_expr(self.0, 0)
    }

    /// The operator token between the operands.
    pub fn op(self) -> Option<&'a GreenToken> {
        self.0.tokens().find(|t| t.kind != SyntaxKind::Missing)
    }

    pub fn rhs(self) -> Option<&'a GreenNode> {
        nth_expr(self.0, 1)
    }
}

ast_node!(
    /// `expr as type`.
    CastExpr
);

impl<'a> CastExpr<'a> {
    pub fn expr(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }

    pub fn ty(self) -> Option<&'a GreenNode> {
        first_type(self.0)
    }
}

ast_node!(
    /// `a..b` / `a..=b` / `..b` / `a..`.
    RangeExpr
);

impl<'a> RangeExpr<'a> {
    /// Is this the inclusive (`..=`) form?
    pub fn is_inclusive(self) -> bool {
        self.0.child_token(SyntaxKind::DotDotEq).is_some()
    }

    /// The endpoints in order; `FromEndExpr` marks `^n` forms.
    pub fn endpoints(self) -> impl Iterator<Item = &'a GreenNode> {
        self.0
            .nodes()
            .filter(|n| is_expr_kind(n.kind) || n.kind == SyntaxKind::FromEndExpr)
    }
}

ast_node!(
    /// `^n` end-relative endpoint (D25).
    FromEndExpr
);

impl<'a> FromEndExpr<'a> {
    pub fn expr(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }
}

ast_node!(
    /// Postfix `?`.
    TryExpr
);

impl<'a> TryExpr<'a> {
    pub fn expr(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }
}

ast_node!(
    /// `callee(args)`.
    CallExpr
);

impl<'a> CallExpr<'a> {
    pub fn callee(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }

    pub fn args(self) -> Option<ArgList<'a>> {
        self.0.nodes().find_map(ArgList::cast)
    }
}

ast_node!(
    /// `expr[args]` — indexing *and* generic application (D29).
    BracketApply
);

impl<'a> BracketApply<'a> {
    pub fn callee(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }

    pub fn args(self) -> Option<ArgList<'a>> {
        self.0.nodes().find_map(ArgList::cast)
    }
}

ast_node!(
    /// The delimited argument list of a call or bracket apply.
    ArgList
);

impl<'a> ArgList<'a> {
    pub fn args(self) -> impl Iterator<Item = Arg<'a>> {
        self.0.nodes().filter_map(Arg::cast)
    }
}

ast_node!(
    /// One argument: `('mut' | 'take')? expr` or a type-only form.
    Arg
);

impl<'a> Arg<'a> {
    /// The X1 call-site mode marker, if any.
    pub fn mode(self) -> Option<ParamMode> {
        self.0.tokens().find_map(|t| match t.kind {
            SyntaxKind::MutKw => Some(ParamMode::Mut),
            SyntaxKind::TakeKw => Some(ParamMode::Take),
            _ => None,
        })
    }

    /// The argument value: an expression, or a type node for the forms
    /// only types can spell (`handle Node`, bare `region`).
    pub fn value(self) -> Option<&'a GreenNode> {
        self.0
            .nodes()
            .find(|n| is_expr_kind(n.kind) || is_type_kind(n.kind))
    }
}

ast_node!(
    /// `expr . member`.
    MemberExpr
);

impl<'a> MemberExpr<'a> {
    pub fn base(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }

    /// The member token after the dot (IDENT, INT, or a keyword —
    /// member position is keyword-transparent).
    pub fn member(self) -> Option<&'a GreenToken> {
        let mut seen_dot = false;
        for t in self.0.tokens() {
            if seen_dot {
                return Some(t);
            }
            if t.kind == SyntaxKind::Dot {
                seen_dot = true;
            }
        }
        None
    }
}

ast_node!(
    /// `path { field_init, … }`.
    StructLit
);

impl<'a> StructLit<'a> {
    /// The path expression before the brace.
    pub fn path(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }

    pub fn fields(self) -> impl Iterator<Item = FieldInit<'a>> {
        self.0.nodes().filter_map(FieldInit::cast)
    }
}

ast_node!(
    /// `IDENT (':' expr)?` in a struct literal.
    FieldInit
);

impl<'a> FieldInit<'a> {
    pub fn name(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }

    pub fn value(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }
}

ast_node!(
    /// `if expr block (else (if_expr | block))?`.
    IfExpr
);

impl<'a> IfExpr<'a> {
    pub fn condition(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }

    pub fn then_block(self) -> Option<Block<'a>> {
        self.0.nodes().find_map(Block::cast)
    }

    /// The else continuation: a nested [`IfExpr`] or the else [`Block`].
    pub fn else_branch(self) -> Option<&'a GreenNode> {
        let else_kw = self.0.child_token(SyntaxKind::ElseKw)?;
        self.0
            .nodes()
            .find(|n| n.span.lo >= else_kw.span.hi)
            .filter(|n| matches!(n.kind, SyntaxKind::IfExpr | SyntaxKind::Block))
    }
}

ast_node!(
    /// `match expr { arm* }`.
    MatchExpr
);

impl<'a> MatchExpr<'a> {
    pub fn scrutinee(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }

    pub fn arms(self) -> impl Iterator<Item = MatchArm<'a>> {
        self.0.nodes().filter_map(MatchArm::cast)
    }
}

ast_node!(
    /// `pattern (if expr)? => (expr | block)`.
    MatchArm
);

impl<'a> MatchArm<'a> {
    pub fn pattern(self) -> Option<&'a GreenNode> {
        first_pattern(self.0)
    }

    pub fn guard(self) -> Option<&'a GreenNode> {
        self.0.child_node(SyntaxKind::MatchGuard)
    }

    pub fn body(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }
}

ast_node!(
    /// `for pattern in expr block`.
    ForExpr
);

impl<'a> ForExpr<'a> {
    pub fn pattern(self) -> Option<&'a GreenNode> {
        first_pattern(self.0)
    }

    pub fn iterable(self) -> Option<&'a GreenNode> {
        self.0
            .nodes()
            .find(|n| is_expr_kind(n.kind) && n.kind != SyntaxKind::Block)
    }

    pub fn body(self) -> Option<Block<'a>> {
        self.0.nodes().find_map(Block::cast)
    }
}

ast_node!(
    /// `while expr block`.
    WhileExpr
);

impl<'a> WhileExpr<'a> {
    pub fn condition(self) -> Option<&'a GreenNode> {
        self.0
            .nodes()
            .find(|n| is_expr_kind(n.kind) && n.kind != SyntaxKind::Block)
    }

    pub fn body(self) -> Option<Block<'a>> {
        self.0.nodes().find_map(Block::cast)
    }
}

ast_node!(
    /// `loop block`.
    LoopExpr
);

impl<'a> LoopExpr<'a> {
    pub fn body(self) -> Option<Block<'a>> {
        self.0.nodes().find_map(Block::cast)
    }
}

ast_node!(
    /// `fn(a, b) expr` / `fn(a, b) { … }`.
    ClosureExpr
);

impl<'a> ClosureExpr<'a> {
    pub fn params(self) -> Option<ParamList<'a>> {
        self.0.nodes().find_map(ParamList::cast)
    }

    /// The body: a [`Block`] or a single maximal expression.
    pub fn body(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }
}

ast_node!(
    /// `return expr?`.
    ReturnExpr
);

impl<'a> ReturnExpr<'a> {
    pub fn value(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }
}

ast_node!(
    /// `break expr?`.
    BreakExpr
);

impl<'a> BreakExpr<'a> {
    pub fn value(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }
}

ast_node!(
    /// `continue`.
    ContinueExpr
);

ast_node!(
    /// `expr else (block | |pat| body | expr)` — defaulting (D30).
    ElseExpr
);

impl<'a> ElseExpr<'a> {
    pub fn scrutinized(self) -> Option<&'a GreenNode> {
        nth_expr(self.0, 0)
    }

    /// The `|pat|` handler pattern, if any.
    pub fn handler_pattern(self) -> Option<&'a GreenNode> {
        first_pattern(self.0)
    }

    pub fn fallback(self) -> Option<&'a GreenNode> {
        nth_expr(self.0, 1)
    }
}

ast_node!(
    /// `region name? (':' strategy)? { … }` — block sugar (X4).
    RegionBlock
);

impl<'a> RegionBlock<'a> {
    pub fn name(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }

    pub fn strategy(self) -> Option<&'a GreenNode> {
        self.0.child_node(SyntaxKind::RegionStrategy)
    }

    pub fn body(self) -> Option<Block<'a>> {
        self.0.nodes().find_map(Block::cast)
    }
}

ast_node!(
    /// `region(strategy?)` — first-class value form (X4).
    RegionValue
);

impl<'a> RegionValue<'a> {
    pub fn strategy(self) -> Option<&'a GreenNode> {
        self.0.child_node(SyntaxKind::RegionStrategy)
    }
}

ast_node!(
    /// `in expr { … }`.
    InBlock
);

impl<'a> InBlock<'a> {
    pub fn region(self) -> Option<&'a GreenNode> {
        self.0
            .nodes()
            .find(|n| is_expr_kind(n.kind) && n.kind != SyntaxKind::Block)
    }

    pub fn body(self) -> Option<Block<'a>> {
        self.0.nodes().find_map(Block::cast)
    }
}

ast_node!(
    /// `freeze expr`.
    FreezeExpr
);

impl<'a> FreezeExpr<'a> {
    pub fn expr(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }
}

ast_node!(
    /// `scope name? { … }`.
    ScopeExpr
);

impl<'a> ScopeExpr<'a> {
    pub fn name(self) -> Option<&'a GreenToken> {
        self.0.child_token(SyntaxKind::Ident)
    }

    pub fn body(self) -> Option<Block<'a>> {
        self.0.nodes().find_map(Block::cast)
    }
}

ast_node!(
    /// `select { arm* }`.
    SelectExpr
);

impl<'a> SelectExpr<'a> {
    pub fn arms(self) -> impl Iterator<Item = SelectArm<'a>> {
        self.0.nodes().filter_map(SelectArm::cast)
    }
}

ast_node!(
    /// `pattern from expr => body` / `timeout(expr) => body`.
    SelectArm
);

impl<'a> SelectArm<'a> {
    /// Is this a `timeout(…)` arm?
    pub fn is_timeout(self) -> bool {
        first_pattern(self.0).is_none()
    }

    pub fn pattern(self) -> Option<&'a GreenNode> {
        first_pattern(self.0)
    }

    pub fn body(self) -> Option<&'a GreenNode> {
        self.0.nodes().filter(|n| is_expr_kind(n.kind)).last()
    }
}

ast_node!(
    /// `when (a, b, …) { … }` — multi-acquire.
    WhenExpr
);

impl<'a> WhenExpr<'a> {
    pub fn operands(self) -> impl Iterator<Item = &'a GreenNode> {
        self.0
            .nodes()
            .filter(|n| is_expr_kind(n.kind) && n.kind != SyntaxKind::Block)
    }

    pub fn body(self) -> Option<Block<'a>> {
        self.0.nodes().find_map(Block::cast)
    }
}

ast_node!(
    /// `spawn proc path(args)`.
    SpawnExpr
);

impl<'a> SpawnExpr<'a> {
    pub fn proc_path(self) -> Option<Path<'a>> {
        self.0.nodes().find_map(Path::cast)
    }

    pub fn args(self) -> Option<ArgList<'a>> {
        self.0.nodes().find_map(ArgList::cast)
    }
}

ast_node!(
    /// `unsafe { … }`.
    UnsafeBlock
);

impl<'a> UnsafeBlock<'a> {
    pub fn body(self) -> Option<Block<'a>> {
        self.0.nodes().find_map(Block::cast)
    }
}

ast_node!(
    /// `unsafe c capture_list? { … }` — inline C.
    InlineC
);

impl<'a> InlineC<'a> {
    pub fn captures(self) -> Option<&'a GreenNode> {
        self.0.child_node(SyntaxKind::CaptureList)
    }

    /// The opaque brace-balanced C body.
    pub fn body(self) -> Option<&'a GreenNode> {
        self.0.child_node(SyntaxKind::InlineCBody)
    }
}

ast_node!(
    /// `asm { STRING, operand* }`.
    AsmExpr
);

impl<'a> AsmExpr<'a> {
    /// The template string.
    pub fn template(self) -> Option<&'a GreenNode> {
        self.0.child_node(SyntaxKind::StringExpr)
    }

    pub fn operands(self) -> impl Iterator<Item = AsmOperand<'a>> {
        self.0.nodes().filter_map(AsmOperand::cast)
    }
}

ast_node!(
    /// `(IDENT '=')? dir '(' constraint ')' expr`.
    AsmOperand
);

impl<'a> AsmOperand<'a> {
    pub fn expr(self) -> Option<&'a GreenNode> {
        first_expr(self.0)
    }
}

ast_node!(
    /// `borrow expr from expr`.
    BorrowExpr
);

impl<'a> BorrowExpr<'a> {
    pub fn borrowed(self) -> Option<&'a GreenNode> {
        nth_expr(self.0, 0)
    }

    pub fn source(self) -> Option<&'a GreenNode> {
        nth_expr(self.0, 1)
    }
}
