//! The canonical textual printer. QBE-grade terseness; deterministic
//! by construction: blocks are numbered in reverse postorder from the
//! entry, values in definition order along that layout, and facts are
//! printed sorted. `print → parse → print` reaches a byte-identical
//! fixpoint (tested), which is what makes dumps diff cleanly and gives
//! D8 content-hashing a stable input. Grammar: `wolf_wir/text.md`.

use crate::entity::EntityRef;
use crate::facts::{DerefSize, FactData, Just};
use crate::ir::{Aux, Block, Function, Inst, Module, SigId, Value};
use crate::ops::Opcode;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

/// Canonical numbering for one function: blocks in RPO (unreachable
/// blocks appended in index order), values in definition order.
pub(crate) struct Canon {
    /// Blocks in canonical (print) order.
    pub blocks: Vec<Block>,
    pub block_num: HashMap<Block, u32>,
    pub value_num: HashMap<Value, u32>,
}

impl Canon {
    pub fn block(&self, b: Block) -> String {
        match self.block_num.get(&b) {
            Some(n) => format!("b{n}"),
            None => format!("b?{b:?}"),
        }
    }

    pub fn value(&self, v: Value) -> String {
        match self.value_num.get(&v) {
            Some(n) => format!("%{n}"),
            None => format!("%?{}", v.as_u32()),
        }
    }
}

/// The successor edges of a block, in terminator operand order.
pub(crate) fn successors(f: &Function, b: Block) -> Vec<Block> {
    let Some(&last) = f.blocks[b].insts.last() else {
        return Vec::new();
    };
    match f.insts[last].aux {
        Aux::Jump(bc) => vec![bc.block],
        Aux::Br(t, e) => vec![t.block, e.block],
        _ => Vec::new(),
    }
}

/// The order a consumer must WALK a function's blocks in: reverse
/// postorder from the entry, then any remaining layout block in layout
/// order (s74, #72).
///
/// `Function::layout` is a *printing* order — entry first, nothing more
/// (see its doc). Lowering appends blocks as it creates them, and a
/// construct that creates a joining block before the block that feeds it
/// (a `select` arm inside a loop: the latch exists before the arm body
/// does) leaves a use ahead of its def in layout order. That is legal
/// WIR — the verifier checks *dominance*, which the CFG satisfies — so
/// any backend translating in a single forward pass must ask for this
/// order instead of assuming the layout is one. RPO is exactly the
/// guarantee such a pass needs: every reachable definition is visited
/// before every reachable use of it.
///
/// The reachable prefix matches the canonical print numbering, so a
/// dump and a codegen walk see one order.
pub fn block_order(f: &Function) -> Vec<Block> {
    let mut order = reachable_rpo(f);
    let seen: HashSet<Block> = order.iter().copied().collect();
    for &b in &f.layout {
        if !seen.contains(&b) {
            order.push(b);
        }
    }
    order
}

/// Reverse postorder over the blocks reachable from the entry.
/// Successors are visited in reverse so the RPO lists then-targets
/// before else-targets (natural reading order for diamonds and loops).
fn reachable_rpo(f: &Function) -> Vec<Block> {
    let mut post: Vec<Block> = Vec::new();
    let mut seen: HashSet<Block> = HashSet::new();
    let Some(entry) = f.entry() else {
        return post;
    };
    // Iterative DFS: (block, next successor index to visit).
    let mut stack: Vec<(Block, usize)> = vec![(entry, 0)];
    seen.insert(entry);
    while let Some((b, i)) = stack.last().copied() {
        let mut succs = successors(f, b);
        succs.reverse();
        if i < succs.len() {
            stack.last_mut().expect("nonempty").1 += 1;
            let s = succs[i];
            if f.blocks.contains(s) && seen.insert(s) {
                stack.push((s, 0));
            }
        } else {
            post.push(b);
            stack.pop();
        }
    }
    post.reverse();
    post
}

pub(crate) fn canonicalize(f: &Function) -> Canon {
    let mut post = reachable_rpo(f);
    let seen: HashSet<Block> = post.iter().copied().collect();
    // Unreachable blocks (and blocks outside the layout) print too —
    // the printer must not lose code on unverified modules. Appended
    // in entity-index order.
    for b in f.blocks.keys() {
        if !seen.contains(&b) {
            post.push(b);
        }
    }
    let mut block_num = HashMap::new();
    for (n, &b) in post.iter().enumerate() {
        block_num.insert(b, n as u32);
    }
    // Values: definition order along the canonical block order.
    let mut value_num = HashMap::new();
    let mut next = 0u32;
    for &b in &post {
        for v in f.vpool.get(f.blocks[b].params) {
            value_num.entry(v).or_insert_with(|| {
                let n = next;
                next += 1;
                n
            });
        }
        for &inst in &f.blocks[b].insts {
            for v in f.vpool.get(f.insts[inst].results) {
                value_num.entry(v).or_insert_with(|| {
                    let n = next;
                    next += 1;
                    n
                });
            }
        }
    }
    // Any value not defined along the layout (raw-API orphans) still
    // needs a stable name.
    for v in f.values.keys() {
        value_num.entry(v).or_insert_with(|| {
            let n = next;
            next += 1;
            n
        });
    }
    Canon {
        blocks: post,
        block_num,
        value_num,
    }
}

/// `(mut ptr, i64, mem.r0) -> f64` — shared by `fn` headers and `decl`s.
pub(crate) fn render_sig(m: &Module, sig: SigId) -> String {
    let data = &m.sigs[sig];
    let params: Vec<String> = data
        .params
        .iter()
        .map(|p| match p.mode.keyword() {
            Some(kw) => format!("{kw} {}", m.types.display(p.ty)),
            None => m.types.display(p.ty),
        })
        .collect();
    let mut s = format!("({})", params.join(", "));
    if !data.results.is_empty() {
        let res: Vec<String> = data.results.iter().map(|&t| m.types.display(t)).collect();
        write!(s, " -> {}", res.join(", ")).unwrap();
    }
    s
}

fn render_args(canon: &Canon, args: &[Value]) -> String {
    args.iter()
        .map(|&v| canon.value(v))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_edge(canon: &Canon, f: &Function, bc: crate::ir::BlockCall) -> String {
    let args = f.vpool.get(bc.args);
    if args.is_empty() {
        canon.block(bc.block)
    } else {
        format!("{}({})", canon.block(bc.block), render_args(canon, &args))
    }
}

/// Render one instruction (no indentation, no trailing newline).
pub(crate) fn render_inst(m: &Module, f: &Function, canon: &Canon, inst: Inst) -> String {
    let data = &f.insts[inst];
    let args = f.vpool.get(data.args);
    let results = f.vpool.get(data.results);
    let mut s = String::new();
    if !results.is_empty() {
        // The memory family (token-minting) and reserved ops carry
        // explicit result types; everything else derives result types.
        let rendered: Vec<String> = results
            .iter()
            .map(|&v| {
                if data.op.explicit_results() {
                    format!("{}: {}", canon.value(v), m.types.display(f.value_ty(v)))
                } else {
                    canon.value(v)
                }
            })
            .collect();
        write!(s, "{} = ", rendered.join(", ")).unwrap();
    }
    s.push_str(data.op.base_mnemonic());
    // Mnemonic suffix.
    match data.op {
        Opcode::Iconst
        | Opcode::Fconst
        | Opcode::Sext
        | Opcode::Zext
        | Opcode::Itrunc
        | Opcode::Load => {
            if let Some(&r) = results.first() {
                write!(s, ".{}", m.types.display(f.value_ty(r))).unwrap();
            }
        }
        Opcode::Store => {
            if let Some(&a) = args.first() {
                write!(s, ".{}", m.types.display(f.value_ty(a))).unwrap();
            }
        }
        Opcode::Icmp => {
            if let Aux::IntCc(cc) = data.aux {
                write!(s, ".{}", cc.mnemonic()).unwrap();
            }
        }
        Opcode::Fcmp => {
            if let Aux::FloatCc(cc) = data.aux {
                write!(s, ".{}", cc.mnemonic()).unwrap();
            }
        }
        Opcode::Trap => {
            // `Assert` is the default kind and prints as bare `trap`
            // (the pre-s28 form); other kinds carry a dotted suffix.
            if let Aux::Trap(k) = data.aux
                && k != crate::ops::TrapKind::Assert
            {
                write!(s, ".{}", k.mnemonic()).unwrap();
            }
        }
        _ => {}
    }
    // Operands.
    match data.aux {
        // `iconst N`, and `region.foreign ROLE` (s80): a bare immediate
        // with no operands to render beside it.
        Aux::Int(n) if matches!(data.op, Opcode::Iconst | Opcode::RegionForeign) => {
            write!(s, " {n}").unwrap();
        }
        Aux::FloatBits(bits) => {
            let is_f32 = results.first().is_some_and(|&r| {
                matches!(m.types.get(f.value_ty(r)), crate::types::TypeData::F32)
            });
            if is_f32 {
                write!(s, " 0x{:08x}", bits as u32).unwrap();
            } else {
                write!(s, " 0x{bits:016x}").unwrap();
            }
        }
        Aux::Bool(b) => {
            write!(s, " {b}").unwrap();
        }
        Aux::Scale(scale) => {
            write!(s, " {}, {scale}", render_args(canon, &args)).unwrap();
        }
        Aux::Int(k) => {
            // agg.get field index.
            write!(s, " {}, {k}", render_args(canon, &args)).unwrap();
        }
        Aux::Callee(ef) => {
            write!(
                s,
                " @{}({})",
                f.ext_funcs[ef].name,
                render_args(canon, &args)
            )
            .unwrap();
        }
        Aux::Data(idx) => {
            let name = m
                .data
                .get(idx as usize)
                .map(|d| d.name.as_str())
                .unwrap_or("?");
            write!(s, " @{name}").unwrap();
        }
        // `call.ind %f(%args…) : (…) -> …` — the sig rendered INLINE
        // by content (s97 target 7a): the D8 hash input is this
        // printed form, and a raw `sigN` here would be its first
        // non-content byte.
        Aux::Sig(sig) => {
            write!(
                s,
                " {}({}) : {}",
                canon.value(args[0]),
                render_args(canon, &args[1..]),
                render_sig(m, sig)
            )
            .unwrap();
        }
        Aux::Jump(bc) => {
            write!(s, " {}", render_edge(canon, f, bc)).unwrap();
        }
        Aux::Br(t, e) => {
            write!(
                s,
                " {}, {}, {}",
                render_args(canon, &args),
                render_edge(canon, f, t),
                render_edge(canon, f, e)
            )
            .unwrap();
        }
        Aux::None | Aux::IntCc(_) | Aux::FloatCc(_) | Aux::Trap(_) => {
            if !args.is_empty() {
                write!(s, " {}", render_args(canon, &args)).unwrap();
            }
        }
    }
    s
}

/// Render one fact line (no indentation).
pub(crate) fn render_fact(canon: &Canon, fact: &FactData) -> String {
    let mut s = String::from("fact ");
    match fact.kind {
        crate::facts::FactKind::Noalias(a, b) => {
            write!(s, "noalias {} {}", canon.value(a), canon.value(b)).unwrap();
        }
        crate::facts::FactKind::Deref(v, size) => {
            write!(s, "deref {} ", canon.value(v)).unwrap();
            match size {
                DerefSize::Const(n) => write!(s, "{n}").unwrap(),
                DerefSize::Scaled { elem, count } => {
                    write!(s, "{elem}x{}", canon.value(count)).unwrap();
                }
            }
        }
        crate::facts::FactKind::Range(v, lo, hi) => {
            write!(s, "range {} {lo}..={hi}", canon.value(v)).unwrap();
        }
        crate::facts::FactKind::Region(v, r) => {
            write!(s, "region {} {r}", canon.value(v)).unwrap();
        }
        crate::facts::FactKind::Frozen(v) => {
            write!(s, "frozen {}", canon.value(v)).unwrap();
        }
    }
    match fact.just {
        Just::Theorem(t) => write!(s, " : {}", t.tag()).unwrap(),
        Just::DefOp => s.push_str(" : op"),
        Just::Op(v) => write!(s, " : op {}", canon.value(v)).unwrap(),
    }
    s
}

/// Render one function in canonical form.
pub(crate) fn print_function(m: &Module, f: &Function) -> String {
    let canon = canonicalize(f);
    let mut out = String::new();
    let export = if f.export { "export " } else { "" };
    writeln!(out, "{export}fn @{}{} {{", f.name, render_sig(m, f.sig)).unwrap();
    // Facts, sorted by rendered line — deterministic and diff-stable.
    let mut fact_lines: Vec<String> = f.facts.values().map(|fd| render_fact(&canon, fd)).collect();
    fact_lines.sort();
    for line in fact_lines {
        writeln!(out, "  {line}").unwrap();
    }
    for &b in &canon.blocks {
        let params = f.vpool.get(f.blocks[b].params);
        if params.is_empty() {
            writeln!(out, "{}:", canon.block(b)).unwrap();
        } else {
            let ps: Vec<String> = params
                .iter()
                .map(|&v| format!("{}: {}", canon.value(v), m.types.display(f.value_ty(v))))
                .collect();
            writeln!(out, "{}({}):", canon.block(b), ps.join(", ")).unwrap();
        }
        for &inst in &f.blocks[b].insts {
            writeln!(out, "  {}", render_inst(m, f, &canon, inst)).unwrap();
        }
    }
    out.push_str("}\n");
    out
}

/// Escape data bytes for the canonical `data @name = "…"` form:
/// printable ASCII verbatim, `\"` `\\` `\n` `\t` `\r`, and `\xNN` for
/// everything else (non-ASCII bytes stay bytes — the data section is
/// bytes, not text).
pub(crate) fn escape_bytes(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() + 2);
    for &b in bytes {
        match b {
            b'"' => s.push_str("\\\""),
            b'\\' => s.push_str("\\\\"),
            b'\n' => s.push_str("\\n"),
            b'\t' => s.push_str("\\t"),
            b'\r' => s.push_str("\\r"),
            0x20..=0x7e => s.push(b as char),
            _ => {
                write!(s, "\\x{b:02x}").unwrap();
            }
        }
    }
    s
}

/// Print a module in canonical form: `decl`s (every declared or called
/// name, sorted), then `data` declarations in definition order, then
/// functions in definition order, blank-line separated.
pub fn print_module(m: &Module) -> String {
    let funcs: Vec<_> = m.funcs.keys().collect();
    print_selected(m, &funcs)
}

/// [`print_module`] restricted to a subset of functions: the sorted
/// `decl` union of the subset's callees, the data declarations the
/// subset references (in definition order), and the functions in the
/// given order. The whole-module print is `print_selected(all)`. This
/// is the per-module cache-key input of the s31 driver: the printed
/// text IS the codegen input, so hashing it keys objects soundly (D8).
pub fn print_selected(m: &Module, funcs: &[crate::ir::FuncId]) -> String {
    let mut decls: BTreeMap<String, SigId> = BTreeMap::new();
    for (name, sig) in &m.decls {
        decls.entry(name.clone()).or_insert(*sig);
    }
    let mut used_data: HashSet<u32> = HashSet::new();
    for &fid in funcs {
        let f = &m.funcs[fid];
        for ef in f.ext_funcs.values() {
            decls.entry(ef.name.clone()).or_insert(ef.sig);
        }
        for inst in f.insts.values() {
            if let Aux::Data(idx) = inst.aux {
                used_data.insert(idx);
            }
        }
    }
    let mut out = String::new();
    for (name, sig) in &decls {
        writeln!(out, "decl @{name}{}", render_sig(m, *sig)).unwrap();
    }
    let mut any_head = !decls.is_empty();
    for (i, d) in m.data.iter().enumerate() {
        if !used_data.contains(&(i as u32)) {
            continue;
        }
        writeln!(out, "data @{} = \"{}\"", d.name, escape_bytes(&d.bytes)).unwrap();
        any_head = true;
    }
    let mut first = !any_head;
    for &fid in funcs {
        if !first {
            out.push('\n');
        }
        first = false;
        out.push_str(&print_function(m, &m.funcs[fid]));
    }
    out
}
