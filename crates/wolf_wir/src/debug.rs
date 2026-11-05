//! Source-type debug descriptions, independent of canonical WIR value shapes.
//! Sizes and offsets describe the particular storage view (native values,
//! packed container elements, or raw C pointees), not a language ABI promise.

use std::sync::Arc;

pub type DebugType = Arc<DebugTypeData>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DebugTypeData {
    pub name: String,
    /// Distinguish equal-shaped nominals from different source modules.
    pub identity: Option<(u32, String)>,
    pub size: u64,
    pub kind: DebugKind,
}

impl DebugTypeData {
    pub fn new(name: impl Into<String>, size: u64, kind: DebugKind) -> DebugType {
        Arc::new(Self {
            name: name.into(),
            identity: None,
            size,
            kind,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DebugEncoding {
    Signed,
    Unsigned,
    Float,
    Boolean,
    UnsignedChar,
    Utf,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DebugKind {
    Base(DebugEncoding),
    Pointer(DebugType),
    Reference(DebugType),
    Struct(Vec<DebugMember>),
    Enum(Vec<(String, i64)>),
    Variants(Vec<DebugVariant>),
    /// Named incomplete type; never pretend to know runtime-private fields.
    Opaque,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DebugMember {
    pub name: String,
    pub offset: u64,
    pub ty: DebugType,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DebugDiscriminant {
    Value(i64),
    /// Resolved after the module has interned all error tags. Debug metadata
    /// must not change the order in which executable code interns those tags.
    ErrorTag(String),
    Default,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DebugVariant {
    pub name: String,
    pub discriminant: DebugDiscriminant,
    pub members: Vec<DebugMember>,
}
