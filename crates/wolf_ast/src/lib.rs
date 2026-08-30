//! Syntax tree with lossless trivia (s08–s09).
//!
//! Contract: the lossless concrete tree (error nodes included) plus the
//! typed AST view over it. Round-trips source byte-for-byte — `wolf fmt`
//! (s11) and resilient diagnostics (D22) both depend on that property.
//!
//! # Shape
//!
//! Rowan-shaped two-layer design, owned minimally (no `rowan` dependency,
//! no interning — the resident-compiler campaign may add sharing later):
//!
//! - [`SyntaxKind`]: one flat enum for **all** token and node kinds.
//!   `wolf_parse` owns the lexer-token → `SyntaxKind` mapping.
//! - Green tree ([`GreenNode`] / [`GreenToken`] / [`Child`]): homogeneous
//!   nodes (kind + children), tokens carrying span + leading/trailing
//!   trivia spans. Direct child iteration is the cursor layer.
//! - Typed accessors ([`ast`]): `FnDecl::name()`, `Param::mode()`, … —
//!   **every accessor returns `Option`**, because any child may be
//!   missing in a broken tree.
//!
//! # Invariants ([`verify`])
//!
//! Every source token appears exactly once, in order; tree text equals
//! source text byte-for-byte; every node's span is the union of its
//! children's. Error nodes ([`SyntaxKind::ErrorNode`]) are ordinary nodes
//! holding skipped tokens — no byte is ever dropped. Zero-width
//! [`SyntaxKind::Missing`] tokens mark required-but-absent tokens without
//! disturbing losslessness.

mod ast;
mod dump;
mod green;
mod kind;
mod verify;

pub use ast::{
    Arg, ArgList, AsmExpr, AsmOperand, AssignStmt, AssumeStmt, AttrItem, Attribute, BinExpr,
    Binder, BinderParts, Block, BorrowExpr, BracketApply, BreakExpr, CallExpr, CastExpr,
    ClosureExpr, ConstDecl, ContinueExpr, DeferStmt, DynType, ElseExpr, EnumDecl, EnumDef,
    EnumVariant, ErrorRow, ErrorUnionType, ExprStmt, FieldInit, FnDecl, FnType, ForExpr,
    FreezeExpr, FromEndExpr, GenericParam, GenericParamList, IfExpr, ImplDecl, ImportCDecl,
    InBlock, InlineC, InnerAttribute, Interp, LetDecl, LiteralExpr, LoopExpr, MatchArm, MatchExpr,
    MemberExpr, Param, ParamList, ParamMode, ParenExpr, Path, PathExpr, PathType, PrefixExpr,
    PrefixType, PtrType, RangeExpr, RegionBlock, RegionValue, RetType, ReturnExpr, RowEntry,
    ScopeExpr, SelectArm, SelectExpr, SpawnExpr, StringExpr, StringLit, StructDecl, StructDef,
    StructField, StructLit, TraitDecl, TryExpr, TupleExpr, TupleType, TypeArgList, TypeBound,
    TypeDecl, UnsafeBlock, UseDecl, UseGroup, VarDecl, ViewSet, Visibility, WhenExpr, WhileExpr,
    binding_binders, is_expr_kind, is_pattern_kind, is_stmt_kind, is_type_kind,
};
pub use dump::dump_decls;
pub use green::{Child, GreenNode, GreenToken};
pub use kind::SyntaxKind;
pub use verify::verify;
