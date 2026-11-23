//! `[mem.region.escape]`: the calls whose `str` answer is a new allocation
//! in the current region, named.
//!
//! A `str` is `Copy`, so a `str` result never made `ret_heap`, and a host
//! builtin's row (`str ! {…}`) hides it from "is the result `str`" as well:
//! the memory tier sees these allocations only by name. The lists below are
//! the names, read by `lower.rs`'s `is_materializing_str_call` and nowhere
//! else, and `tests/region_site_list.rs` holds them against the runtime's
//! source both ways (wolf-lang#1179): every runtime entry point that copies a
//! `str` into the current region must be named here, and every name here
//! must be such an entry point or a named exception.

/// Host builtins whose `str` answer the runtime builds in the current
/// region. A call to one is a site: its answer returned out of the region
/// it was asked in is E1010. A name joins the list no later than the commit
/// that routes its bytes to the region (wolf-lang#374's order): without the
/// site, moving the bytes turns a leak into a read of freed memory.
///
/// - `str_from_utf8` (wolf-lang#191's other half): the accepted bytes,
///   copied, borrowing nothing from the list.
/// - `os_error_text` (`[os.fs.error]`): the host's message. Its runtime
///   copies into the process root today, so listing it refuses a program
///   that would be safe; it is the guard's one named exception.
/// - `net_read` (wolf-lang#374, the net half): what arrived.
/// - `os_cwd` (wolf-lang#834): the working directory's name.
/// - `env_arg0` (`[os.env.argv]`): the invocation name.
pub const HOST_STR_SITES: &[&str] = &[
    "str_from_utf8",
    "os_error_text",
    "net_read",
    "os_cwd",
    "env_arg0",
];

/// Methods on a `str` receiver that build fresh bytes in the current
/// region, as `+` does (s160, wolf-lang#321). The view methods
/// (`[mem.str.view]`) allocate nothing and are not here.
pub const STR_METHOD_SITES: &[&str] = &["upper", "lower", "repeat", "replace"];
