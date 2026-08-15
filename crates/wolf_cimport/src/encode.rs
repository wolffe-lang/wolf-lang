//! The versioned binary serialization of an [`Artifact`].
//!
//! Hand-rolled, little-endian, length-prefixed. No serialization
//! framework: the format is an interface that outlives the crate that
//! writes it (c15's frontend must emit these bytes), so it is spelled
//! out rather than derived from whatever a macro decided this year.
//!
//! Every read is bounds-checked and every tag is validated — this
//! decodes bytes out of a shared cache directory and out of a worker
//! process, neither of which is more trustworthy than a file on disk.
//! A malformed artifact is [`DecodeError`], never a panic.

use crate::artifact::{
    Artifact, ConstValue, Decl, DeclKind, FORMAT_VERSION, Linkage, MacroDef, MacroKind,
    ShimRequest, SourceLoc,
};
use crate::ctype::{
    CType, CTypeArena, CTypeId, Endian, EnumDef, EnumId, Field, IntSpelling, Qual, Record,
    RecordId, RecordKind, TargetInfo,
};
use crate::refuse::{Demotion, Refusal, Status};

/// `WOLFCIMP` — the artifact magic.
pub const MAGIC: &[u8; 8] = b"WOLFCIMP";

/// What went wrong reading an artifact.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DecodeError {
    /// Not an artifact at all.
    BadMagic,
    /// A format version this compiler does not read. Carries what was
    /// found and what we speak, because the fix (re-import, or upgrade)
    /// depends on which way the mismatch goes.
    Version { found: u32, expected: u32 },
    /// Ran off the end.
    Truncated,
    /// A tag byte or string that is not in the format's vocabulary.
    BadTag(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::BadMagic => f.write_str("not a c-import artifact (bad magic)"),
            DecodeError::Version { found, expected } => write!(
                f,
                "c-import artifact is format {found}, this compiler reads {expected}"
            ),
            DecodeError::Truncated => f.write_str("c-import artifact ends mid-record"),
            DecodeError::BadTag(t) => write!(f, "c-import artifact has an unknown tag `{t}`"),
        }
    }
}

// ------------------------------------------------------------ writer ----

#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i128(&mut self, v: i128) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.0.extend_from_slice(&v.to_bits().to_le_bytes());
    }
    fn bool(&mut self, v: bool) {
        self.u8(v as u8);
    }
    fn bytes(&mut self, b: &[u8]) {
        self.u32(b.len() as u32);
        self.0.extend_from_slice(b);
    }
    fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
    fn strs(&mut self, v: &[String]) {
        self.u32(v.len() as u32);
        for s in v {
            self.str(s);
        }
    }
    fn opt_u64(&mut self, v: Option<u64>) {
        match v {
            Some(n) => {
                self.bool(true);
                self.u64(n);
            }
            None => self.bool(false),
        }
    }
}

// ------------------------------------------------------------ reader ----

struct R<'a> {
    b: &'a [u8],
    p: usize,
}

type D<T> = Result<T, DecodeError>;

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> D<&'a [u8]> {
        let end = self.p.checked_add(n).ok_or(DecodeError::Truncated)?;
        let s = self.b.get(self.p..end).ok_or(DecodeError::Truncated)?;
        self.p = end;
        Ok(s)
    }
    fn u8(&mut self) -> D<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> D<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| DecodeError::Truncated)?,
        ))
    }
    fn u32(&mut self) -> D<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| DecodeError::Truncated)?,
        ))
    }
    fn u64(&mut self) -> D<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| DecodeError::Truncated)?,
        ))
    }
    fn i128(&mut self) -> D<i128> {
        Ok(i128::from_le_bytes(
            self.take(16)?
                .try_into()
                .map_err(|_| DecodeError::Truncated)?,
        ))
    }
    fn f64(&mut self) -> D<f64> {
        Ok(f64::from_bits(self.u64()?))
    }
    fn bool(&mut self) -> D<bool> {
        Ok(self.u8()? != 0)
    }
    fn bytes(&mut self) -> D<Vec<u8>> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
    fn str(&mut self) -> D<String> {
        let b = self.bytes()?;
        String::from_utf8(b).map_err(|_| DecodeError::BadTag("non-utf8 string".into()))
    }
    fn strs(&mut self) -> D<Vec<String>> {
        let n = self.u32()? as usize;
        // A length is not a licence to allocate: cap against the bytes
        // that could possibly remain (a 4-byte string is the smallest).
        if n > self.b.len() - self.p.min(self.b.len()) {
            return Err(DecodeError::Truncated);
        }
        (0..n).map(|_| self.str()).collect()
    }
    fn opt_u64(&mut self) -> D<Option<u64>> {
        Ok(if self.bool()? {
            Some(self.u64()?)
        } else {
            None
        })
    }
    /// A count that will drive a loop of records at least `min` bytes
    /// wide — refuse absurd lengths before allocating.
    fn count(&mut self, min: usize) -> D<usize> {
        let n = self.u32()? as usize;
        let left = self.b.len().saturating_sub(self.p);
        if min > 0 && n.saturating_mul(min) > left {
            return Err(DecodeError::Truncated);
        }
        Ok(n)
    }
}

// ------------------------------------------------------------- parts ----

fn put_status(w: &mut W, s: &Status) {
    match s {
        Status::Ok => w.u8(0),
        Status::Refused { demotion, refusal } => {
            w.u8(1);
            w.str(demotion.tag());
            w.str(refusal.tag());
            w.str(refusal.payload());
        }
    }
}

fn get_status(r: &mut R<'_>) -> D<Status> {
    Ok(match r.u8()? {
        0 => Status::Ok,
        1 => {
            let dtag = r.str()?;
            let demotion = match dtag.as_str() {
                "opaque" => Demotion::Opaque,
                "extern-only" => Demotion::ExternOnly,
                "error-on-use" => Demotion::ErrorOnUse,
                _ => return Err(DecodeError::BadTag(dtag)),
            };
            let rtag = r.str()?;
            let payload = r.str()?;
            let refusal =
                Refusal::from_tag(&rtag, &payload).ok_or(DecodeError::BadTag(rtag.clone()))?;
            Status::Refused { demotion, refusal }
        }
        other => return Err(DecodeError::BadTag(format!("status {other}"))),
    })
}

fn put_loc(w: &mut W, l: &SourceLoc) {
    w.u32(l.file);
    w.u32(l.line);
    w.u32(l.col);
}

fn get_loc(r: &mut R<'_>) -> D<SourceLoc> {
    Ok(SourceLoc {
        file: r.u32()?,
        line: r.u32()?,
        col: r.u32()?,
    })
}

fn put_qual(w: &mut W, q: Qual) {
    w.u8((q.is_const as u8) | ((q.is_volatile as u8) << 1) | ((q.is_restrict as u8) << 2));
}

fn get_qual(r: &mut R<'_>) -> D<Qual> {
    let b = r.u8()?;
    Ok(Qual {
        is_const: b & 1 != 0,
        is_volatile: b & 2 != 0,
        is_restrict: b & 4 != 0,
    })
}

fn put_ctype(w: &mut W, t: &CType) {
    match t {
        CType::Void => w.u8(0),
        CType::Bool => w.u8(1),
        CType::Int {
            bits,
            signed,
            spelling,
        } => {
            w.u8(2);
            w.u16(*bits);
            w.bool(*signed);
            w.str(spelling.tag());
        }
        CType::Float { bits } => {
            w.u8(3);
            w.u16(*bits);
        }
        CType::Ptr { pointee, qual } => {
            w.u8(4);
            w.u32(pointee.0);
            put_qual(w, *qual);
        }
        CType::Array { elem, len } => {
            w.u8(5);
            w.u32(elem.0);
            w.opt_u64(*len);
        }
        CType::Func {
            ret,
            params,
            variadic,
        } => {
            w.u8(6);
            w.u32(ret.0);
            w.u32(params.len() as u32);
            for p in params {
                w.u32(p.0);
            }
            w.bool(*variadic);
        }
        CType::Record(id) => {
            w.u8(7);
            w.u32(id.0);
        }
        CType::Enum(id) => {
            w.u8(8);
            w.u32(id.0);
        }
        CType::Refused(tag) => {
            w.u8(9);
            w.str(tag);
        }
    }
}

fn get_ctype(r: &mut R<'_>) -> D<CType> {
    Ok(match r.u8()? {
        0 => CType::Void,
        1 => CType::Bool,
        2 => {
            let bits = r.u16()?;
            let signed = r.bool()?;
            let s = r.str()?;
            CType::Int {
                bits,
                signed,
                spelling: IntSpelling::from_tag(&s).ok_or(DecodeError::BadTag(s))?,
            }
        }
        3 => CType::Float { bits: r.u16()? },
        4 => CType::Ptr {
            pointee: CTypeId(r.u32()?),
            qual: get_qual(r)?,
        },
        5 => CType::Array {
            elem: CTypeId(r.u32()?),
            len: r.opt_u64()?,
        },
        6 => {
            let ret = CTypeId(r.u32()?);
            let n = r.count(4)?;
            let params = (0..n)
                .map(|_| Ok(CTypeId(r.u32()?)))
                .collect::<D<Vec<_>>>()?;
            CType::Func {
                ret,
                params,
                variadic: r.bool()?,
            }
        }
        7 => CType::Record(RecordId(r.u32()?)),
        8 => CType::Enum(EnumId(r.u32()?)),
        9 => CType::Refused(r.str()?),
        other => return Err(DecodeError::BadTag(format!("ctype {other}"))),
    })
}

// ------------------------------------------------------------ public ----

/// Serialize an artifact. The bytes start with [`MAGIC`] and the
/// format version, so a stale cache entry is detected rather than
/// misread.
pub fn encode(a: &Artifact) -> Vec<u8> {
    let mut w = W::default();
    w.0.extend_from_slice(MAGIC);
    w.u32(a.format_version);
    w.str(&a.importer);

    // target
    let t = &a.target;
    w.str(&t.triple);
    for v in [
        t.pointer_bits,
        t.pointer_align_bits,
        t.short_bits,
        t.int_bits,
        t.long_bits,
        t.long_long_bits,
        t.wchar_bits,
        t.size_t_bits,
    ] {
        w.u16(v);
    }
    w.bool(t.char_signed);
    w.bool(t.wchar_signed);
    w.u8(match t.endian {
        Endian::Little => 0,
        Endian::Big => 1,
    });

    w.strs(&a.headers);
    w.strs(&a.files);

    // types
    w.u32(a.types.len() as u32);
    for (_, ty) in a.types.iter() {
        put_ctype(&mut w, ty);
    }

    // records
    w.u32(a.records.len() as u32);
    for rec in &a.records {
        w.str(&rec.name);
        w.u8(match rec.kind {
            RecordKind::Struct => 0,
            RecordKind::Union => 1,
        });
        w.opt_u64(rec.size_bytes);
        w.opt_u64(rec.align_bytes.map(u64::from));
        w.bool(rec.opaque);
        w.u32(rec.fields.len() as u32);
        for f in &rec.fields {
            w.str(&f.name);
            w.u32(f.ty.0);
            w.u64(f.offset_bits);
            w.opt_u64(f.bit_width.map(u64::from));
        }
    }

    // enums
    w.u32(a.enums.len() as u32);
    for e in &a.enums {
        w.str(&e.name);
        w.u32(e.underlying.0);
        w.u32(e.constants.len() as u32);
        for (n, v) in &e.constants {
            w.str(n);
            w.i128(*v);
        }
    }

    // decls
    w.u32(a.decls.len() as u32);
    for d in &a.decls {
        w.str(&d.name);
        w.str(&d.wolf_name);
        match &d.kind {
            DeclKind::Func { ty, inline_only } => {
                w.u8(0);
                w.u32(ty.0);
                w.bool(*inline_only);
            }
            DeclKind::Object { ty } => {
                w.u8(1);
                w.u32(ty.0);
            }
            DeclKind::Typedef { ty } => {
                w.u8(2);
                w.u32(ty.0);
            }
            DeclKind::Tag { ty } => {
                w.u8(3);
                w.u32(ty.0);
            }
            DeclKind::EnumConst { ty, value } => {
                w.u8(4);
                w.u32(ty.0);
                w.i128(*value);
            }
        }
        w.str(d.linkage.tag());
        put_loc(&mut w, &d.loc);
        put_status(&mut w, &d.status);
    }

    // macros
    w.u32(a.macros.len() as u32);
    for m in &a.macros {
        w.str(&m.name);
        match &m.kind {
            MacroKind::Object { value, tokens } => {
                w.u8(0);
                match value {
                    None => w.u8(0),
                    Some(ConstValue::Int { value, ty }) => {
                        w.u8(1);
                        w.i128(*value);
                        w.u32(ty.0);
                    }
                    Some(ConstValue::Float { value, ty }) => {
                        w.u8(2);
                        w.f64(*value);
                        w.u32(ty.0);
                    }
                    Some(ConstValue::Str(b)) => {
                        w.u8(3);
                        w.bytes(b);
                    }
                    Some(ConstValue::Char { value, ty }) => {
                        w.u8(4);
                        w.u64(*value as u64);
                        w.u32(ty.0);
                    }
                }
                w.strs(tokens);
            }
            MacroKind::Function {
                params,
                variadic,
                tokens,
            } => {
                w.u8(1);
                w.strs(params);
                w.bool(*variadic);
                w.strs(tokens);
            }
        }
        put_loc(&mut w, &m.loc);
        put_status(&mut w, &m.status);
    }

    // shims
    w.u32(a.shims.len() as u32);
    for s in &a.shims {
        w.str(&s.function);
        w.str(&s.source);
    }

    w.0
}

/// Deserialize an artifact. Refuses anything that is not exactly the
/// format version this compiler speaks — a cache entry from a different
/// version is re-imported, never guessed at.
pub fn decode(bytes: &[u8]) -> Result<Artifact, DecodeError> {
    let mut r = R { b: bytes, p: 0 };
    if r.take(8)? != MAGIC {
        return Err(DecodeError::BadMagic);
    }
    let format_version = r.u32()?;
    if format_version != FORMAT_VERSION {
        return Err(DecodeError::Version {
            found: format_version,
            expected: FORMAT_VERSION,
        });
    }
    let importer = r.str()?;

    let triple = r.str()?;
    let pointer_bits = r.u16()?;
    let pointer_align_bits = r.u16()?;
    let short_bits = r.u16()?;
    let int_bits = r.u16()?;
    let long_bits = r.u16()?;
    let long_long_bits = r.u16()?;
    let wchar_bits = r.u16()?;
    let size_t_bits = r.u16()?;
    let char_signed = r.bool()?;
    let wchar_signed = r.bool()?;
    let endian = match r.u8()? {
        0 => Endian::Little,
        1 => Endian::Big,
        other => return Err(DecodeError::BadTag(format!("endian {other}"))),
    };
    let target = TargetInfo {
        triple,
        pointer_bits,
        pointer_align_bits,
        char_signed,
        short_bits,
        int_bits,
        long_bits,
        long_long_bits,
        wchar_bits,
        wchar_signed,
        size_t_bits,
        endian,
    };

    let headers = r.strs()?;
    let files = r.strs()?;

    let n = r.count(1)?;
    let mut types = CTypeArena::new();
    for _ in 0..n {
        let t = get_ctype(&mut r)?;
        types.push_raw(t);
    }

    let n = r.count(8)?;
    let mut records = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.str()?;
        let kind = match r.u8()? {
            0 => RecordKind::Struct,
            1 => RecordKind::Union,
            other => return Err(DecodeError::BadTag(format!("record kind {other}"))),
        };
        let size_bytes = r.opt_u64()?;
        let align_bytes = r.opt_u64()?.map(|v| v as u32);
        let opaque = r.bool()?;
        let nf = r.count(8)?;
        let mut fields = Vec::with_capacity(nf);
        for _ in 0..nf {
            fields.push(Field {
                name: r.str()?,
                ty: CTypeId(r.u32()?),
                offset_bits: r.u64()?,
                bit_width: r.opt_u64()?.map(|v| v as u32),
            });
        }
        records.push(Record {
            name,
            kind,
            size_bytes,
            align_bytes,
            fields,
            opaque,
        });
    }

    let n = r.count(8)?;
    let mut enums = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.str()?;
        let underlying = CTypeId(r.u32()?);
        let nc = r.count(8)?;
        let mut constants = Vec::with_capacity(nc);
        for _ in 0..nc {
            constants.push((r.str()?, r.i128()?));
        }
        enums.push(EnumDef {
            name,
            underlying,
            constants,
        });
    }

    let n = r.count(16)?;
    let mut decls = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.str()?;
        let wolf_name = r.str()?;
        let kind = match r.u8()? {
            0 => DeclKind::Func {
                ty: CTypeId(r.u32()?),
                inline_only: r.bool()?,
            },
            1 => DeclKind::Object {
                ty: CTypeId(r.u32()?),
            },
            2 => DeclKind::Typedef {
                ty: CTypeId(r.u32()?),
            },
            3 => DeclKind::Tag {
                ty: CTypeId(r.u32()?),
            },
            4 => DeclKind::EnumConst {
                ty: CTypeId(r.u32()?),
                value: r.i128()?,
            },
            other => return Err(DecodeError::BadTag(format!("decl kind {other}"))),
        };
        let ltag = r.str()?;
        let linkage = Linkage::from_tag(&ltag).ok_or(DecodeError::BadTag(ltag))?;
        decls.push(Decl {
            name,
            wolf_name,
            kind,
            linkage,
            loc: get_loc(&mut r)?,
            status: get_status(&mut r)?,
        });
    }

    let n = r.count(16)?;
    let mut macros = Vec::with_capacity(n);
    for _ in 0..n {
        let name = r.str()?;
        let kind = match r.u8()? {
            0 => {
                let value = match r.u8()? {
                    0 => None,
                    1 => Some(ConstValue::Int {
                        value: r.i128()?,
                        ty: CTypeId(r.u32()?),
                    }),
                    2 => Some(ConstValue::Float {
                        value: r.f64()?,
                        ty: CTypeId(r.u32()?),
                    }),
                    3 => Some(ConstValue::Str(r.bytes()?)),
                    4 => Some(ConstValue::Char {
                        value: r.u64()? as i64,
                        ty: CTypeId(r.u32()?),
                    }),
                    other => return Err(DecodeError::BadTag(format!("const value {other}"))),
                };
                MacroKind::Object {
                    value,
                    tokens: r.strs()?,
                }
            }
            1 => MacroKind::Function {
                params: r.strs()?,
                variadic: r.bool()?,
                tokens: r.strs()?,
            },
            other => return Err(DecodeError::BadTag(format!("macro kind {other}"))),
        };
        macros.push(MacroDef {
            name,
            kind,
            loc: get_loc(&mut r)?,
            status: get_status(&mut r)?,
        });
    }

    let n = r.count(8)?;
    let mut shims = Vec::with_capacity(n);
    for _ in 0..n {
        shims.push(ShimRequest {
            function: r.str()?,
            source: r.str()?,
        });
    }

    Ok(Artifact {
        format_version,
        importer,
        target,
        headers,
        files,
        types,
        records,
        enums,
        decls,
        macros,
        shims,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit;

    #[test]
    fn round_trips_a_full_artifact() {
        let a = testkit::sample_artifact();
        let bytes = encode(&a);
        let back = decode(&bytes).expect("decodes");
        assert_eq!(a, back);
    }

    #[test]
    fn encoding_is_byte_deterministic() {
        let a = testkit::sample_artifact();
        assert_eq!(encode(&a), encode(&a));
        // And a second, independently built artifact agrees — the
        // conformance suite compares bytes across workers.
        let b = testkit::sample_artifact();
        assert_eq!(encode(&a), encode(&b));
    }

    #[test]
    fn a_stale_format_version_is_refused_not_misread() {
        let a = testkit::sample_artifact();
        let mut bytes = encode(&a);
        bytes[8] = 99; // format_version low byte
        assert_eq!(
            decode(&bytes),
            Err(DecodeError::Version {
                found: 99,
                expected: FORMAT_VERSION
            })
        );
    }

    #[test]
    fn foreign_bytes_are_refused() {
        assert_eq!(decode(b"not an artifact"), Err(DecodeError::BadMagic));
        assert_eq!(decode(b""), Err(DecodeError::Truncated));
    }

    /// The decoder reads a shared cache directory and a child process's
    /// stdout. Every truncation must be an error, never a panic and
    /// never a gigabyte allocation.
    #[test]
    fn every_truncation_is_an_error_not_a_panic() {
        let bytes = encode(&testkit::sample_artifact());
        for cut in 0..bytes.len() {
            let r = decode(&bytes[..cut]);
            assert!(r.is_err(), "prefix of length {cut} decoded as valid");
        }
    }

    /// A corrupted length field must not be believed.
    #[test]
    fn absurd_lengths_do_not_allocate() {
        let a = testkit::sample_artifact();
        let mut bytes = encode(&a);
        // Smash every byte in turn to a large value and require that
        // decoding either errors or produces something — never hangs
        // or aborts. (Cheap fuzz; the real fuzzer is s49's.)
        for i in 0..bytes.len().min(400) {
            let save = bytes[i];
            bytes[i] = 0xff;
            let _ = decode(&bytes);
            bytes[i] = save;
        }
    }
}
