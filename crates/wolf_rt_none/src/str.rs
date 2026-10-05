//! The process root and the interpolation buffer (kw12).
//!
//! The root region is where an allocation lands when no `region` is
//! open (`[mem.region.create.3]`). Hosted, it is a bump arena over
//! malloc'd chunks that never frees; here each root grant is its own
//! block from the hook, and it is never freed either — the same
//! meaning, with the kernel's allocator seeing every block.
//!
//! The strbuf is the hosted lowering's contract (`strbuf_new` → appends
//! → `strbuf_finish`, which copies the built bytes into the ambient
//! region and writes `{ptr, len}` through the caller's 16-byte slot):
//! a growable byte buffer from the hook, handed back to the hook at
//! `finish`. The holes render through [`crate::fmt`], the hosted
//! renderer's twin.

use crate::fmt::{Val, render_packed};
use crate::native::{grain, hook_alloc, hook_free};

/// Bump-allocate `size` bytes (16-aligned) in the process root. Never
/// freed; zero-size asks get a distinct pointer.
pub(crate) fn ambient_alloc(size: usize) -> *mut u8 {
    hook_alloc(grain(size))
}

/// The build buffer behind a strbuf handle.
#[repr(C)]
struct StrBuf {
    ptr: *mut u8,
    len: usize,
    cap: usize,
}

const HDR: usize = (core::mem::size_of::<StrBuf>() + 15) & !15;
/// The first buffer's size: most interpolations are a line.
const FIRST: usize = 64;

impl StrBuf {
    fn push(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let need = self.len + bytes.len();
        if need > self.cap {
            let ncap = grain(need.max(self.cap * 2).max(FIRST));
            let n = hook_alloc(ncap);
            if self.len > 0 {
                // SAFETY: both blocks hold at least `len` bytes.
                unsafe { core::ptr::copy_nonoverlapping(self.ptr, n, self.len) };
            }
            if self.cap > 0 {
                // SAFETY: the old block came from hook_alloc(cap).
                unsafe { hook_free(self.ptr, self.cap) };
            }
            self.ptr = n;
            self.cap = ncap;
        }
        // SAFETY: `cap >= len + bytes.len()`.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), self.ptr.add(self.len), bytes.len());
        }
        self.len = need;
    }
}

/// The buffer behind `handle`.
///
/// # Safety
///
/// `handle` from [`__wolf_rt_strbuf_new`], unfinished.
unsafe fn buf<'a>(handle: i64) -> &'a mut StrBuf {
    unsafe { &mut *(handle as *mut StrBuf) }
}

/// Rebuild the bytes of a `{ptr, len}` pair.
///
/// # Safety
///
/// `ptr` addresses `len` readable bytes (every wolf `str` does).
unsafe fn view<'a>(ptr: i64, len: i64) -> &'a [u8] {
    if ptr == 0 || len <= 0 {
        return &[];
    }
    unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) }
}

/// `strbuf.new` — a fresh interpolation buffer.
#[unsafe(no_mangle)]
pub extern "C" fn __wolf_rt_strbuf_new() -> i64 {
    let h = hook_alloc(HDR).cast::<StrBuf>();
    // SAFETY: a fresh 16-aligned block of HDR bytes.
    unsafe {
        h.write(StrBuf {
            ptr: core::ptr::null_mut(),
            len: 0,
            cap: 0,
        });
    }
    h as i64
}

/// Append a str segment under the packed `spec` (0 = raw bytes).
///
/// # Safety
///
/// `handle` from [`__wolf_rt_strbuf_new`], unfinished; `ptr`/`len` a
/// valid str pair.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_strbuf_str(handle: i64, ptr: i64, len: i64, spec: i64) {
    let s = unsafe { view(ptr, len) };
    let b = unsafe { buf(handle) };
    if spec == 0 {
        b.push(s);
    } else {
        render_packed(Val::Str(s), spec, &mut |x| b.push(x));
    }
}

/// Append an integer hole (bit 14 of the spec: unsigned).
///
/// # Safety
///
/// `handle` from [`__wolf_rt_strbuf_new`], unfinished.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_strbuf_i64(handle: i64, v: i64, spec: i64) {
    let b = unsafe { buf(handle) };
    render_packed(Val::Int(v), spec, &mut |x| b.push(x));
}

/// Append a `true`/`false` hole (the bool crosses as one byte).
///
/// # Safety
///
/// `handle` from [`__wolf_rt_strbuf_new`], unfinished.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_strbuf_bool(handle: i64, v: i8, spec: i64) {
    let b = unsafe { buf(handle) };
    render_packed(Val::Bool(v != 0), spec, &mut |x| b.push(x));
}

/// Append a `char` hole: the character's UTF-8 encoding (U+FFFD for a
/// value that is not a scalar); a spec applies the str surface.
///
/// # Safety
///
/// `handle` from [`__wolf_rt_strbuf_new`], unfinished.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_strbuf_char(handle: i64, v: i64, spec: i64) {
    let c = u32::try_from(v)
        .ok()
        .and_then(char::from_u32)
        .unwrap_or('\u{FFFD}');
    let mut tmp = [0u8; 4];
    let enc = c.encode_utf8(&mut tmp).as_bytes();
    let b = unsafe { buf(handle) };
    if spec == 0 {
        b.push(enc);
    } else {
        render_packed(Val::Str(enc), spec, &mut |x| b.push(x));
    }
}

/// Copy the built bytes into the ambient region, write the `{ptr,
/// len}` pair through `out`, and hand the buffer back to the hook.
///
/// # Safety
///
/// `handle` from [`__wolf_rt_strbuf_new`] — dead after this call;
/// `out` addresses 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __wolf_rt_strbuf_finish(handle: i64, out: i64) {
    let h = handle as *mut StrBuf;
    unsafe {
        let StrBuf { ptr, len, cap } = h.read();
        let p = crate::list::alloc_in(crate::native::ambient_region(), len);
        if len > 0 {
            core::ptr::copy_nonoverlapping(ptr, p, len);
        }
        if cap > 0 {
            hook_free(ptr, cap);
        }
        hook_free(h.cast(), HDR);
        let o = out as *mut i64;
        o.write(p as i64);
        o.add(1).write(len as i64);
    }
}
