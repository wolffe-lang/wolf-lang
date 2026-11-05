//! Data-only Rust witnesses of runtime header layout for compiler debug tests.
//! No C ABI symbol or executable language feature is added by these constants.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderField {
    pub name: &'static str,
    pub offset: u64,
    pub size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeaderLayout {
    pub size: u64,
    pub fields: &'static [HeaderField],
}

/// Infer the real field type without constructing a value or dereferencing
/// anything. The accessor is never called, even during constant evaluation.
pub(crate) const fn field_size<T, F>(_accessor: fn(&T) -> &F) -> u64 {
    core::mem::size_of::<F>() as u64
}
