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

fn first_type(node: &GreenNode) -> Option<&GreenNode> {
    node.nodes().find(|n| is_type_kind(n.kind))
}

fn first_pattern(node: &GreenNode) -> Option<&GreenNode> {
    node.nodes().find(|n| is_pattern_kind(n.kind))
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

    /// The fenced `{…}` body (opened in s09). `None` for bodyless forms.
    pub fn body(self) -> Option<&'a GreenNode> {
        self.0.child_node(SyntaxKind::BlockPending)
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

macro_rules! binding_accessors {
    ($name:ident) => {
        impl<'a> $name<'a> {
            pub fn pattern(self) -> Option<&'a GreenNode> {
                first_pattern(self.0)
            }

            /// The `: T` ascription, if any.
            pub fn ty(self) -> Option<&'a GreenNode> {
                first_type(self.0)
            }

            /// The fenced `= …` initializer (opened in s09).
            pub fn init(self) -> Option<&'a GreenNode> {
                self.0.child_node(SyntaxKind::ExprPending)
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
        self.0.child_node(SyntaxKind::ExprPending)
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
