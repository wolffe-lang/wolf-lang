//! A deliberately narrow, fail-closed proof over GNU objdump -d
//! --no-show-raw-insn output. Invariant: **no SSE instruction executes
//! when the incoming AL is zero**. A plain register-to-register copy
//! may preserve incoming AL in another register's low byte. Loads,
//! arithmetic and unknown clobbers never establish that provenance.
//!
//! This is a machine-code regression gate, not a general x86 verifier.
//! It accepts one contiguous AL-guarded XMM save block per function;
//! unfamiliar prologue shapes fail rather than weakening the proof.

use std::collections::{BTreeMap, BTreeSet};

struct Instruction {
    address: u64,
    mnemonic: String,
    operands: String,
    /// Kept until a bounded unreachable-suffix proof, never ignored.
    invalid: bool,
}

#[derive(Clone, Copy)]
struct Register {
    family: usize,
    width: u8,
    low: bool,
}

fn register(text: &str) -> Option<Register> {
    let text = text.trim().strip_prefix('%')?;
    let names = [
        ["rax", "eax", "ax", "al", "ah"],
        ["rbx", "ebx", "bx", "bl", "bh"],
        ["rcx", "ecx", "cx", "cl", "ch"],
        ["rdx", "edx", "dx", "dl", "dh"],
        ["rsi", "esi", "si", "sil", ""],
        ["rdi", "edi", "di", "dil", ""],
        ["rbp", "ebp", "bp", "bpl", ""],
        ["rsp", "esp", "sp", "spl", ""],
    ];
    for (family, aliases) in names.iter().enumerate() {
        for (part, name) in aliases.iter().enumerate() {
            if !name.is_empty() && text == *name {
                return Some(Register {
                    family,
                    width: [64, 32, 16, 8, 8][part],
                    low: part != 4,
                });
            }
        }
    }
    for family in 8..16 {
        for (suffix, width) in [("", 64), ("d", 32), ("w", 16), ("b", 8)] {
            if text == format!("r{family}{suffix}") {
                return Some(Register {
                    family,
                    width,
                    low: true,
                });
            }
        }
    }
    None
}

fn operands(text: &str) -> Vec<&str> {
    let mut depth = 0;
    let mut begin = 0;
    let mut parts = Vec::new();
    for (index, ch) in text.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(text[begin..index].trim());
                begin = index + 1;
            }
            _ => {}
        }
    }
    if !text[begin..].trim().is_empty() {
        parts.push(text[begin..].trim());
    }
    parts
}

fn vector_state(instruction: &Instruction) -> bool {
    let op = instruction.mnemonic.as_str();
    instruction.operands.contains("%xmm")
        || instruction.operands.contains("%ymm")
        || instruction.operands.contains("%zmm")
        || instruction.operands.contains("%mm")
        || instruction.operands.contains("%st")
        || op.starts_with('f')
        || instruction.operands.contains("%k")
        || op.starts_with('v')
        || op.contains("mxcsr")
        || op.starts_with("fxsave")
        || op.starts_with("fxrstor")
        || op.starts_with("xsave")
        || op.starts_with("xrstor")
        || op.starts_with("tile")
        || matches!(
            op,
            "emms"
                | "femms"
                | "xsetbv"
                | "xgetbv"
                | "ldtilecfg"
                | "sttilecfg"
                | "fsave"
                | "fnsave"
                | "frstor"
                | "fnstenv"
                | "fldenv"
                | "finit"
                | "fninit"
        )
}

fn ordinary_alu(op: &str) -> bool {
    [
        "add", "sub", "and", "or", "xor", "lea", "inc", "dec", "neg", "not", "shl", "shr", "sar",
        "sal", "rol", "ror",
    ]
    .iter()
    .any(|base| {
        op == *base
            || ["b", "w", "l", "q"]
                .iter()
                .any(|suffix| op == format!("{base}{suffix}"))
    })
}

/// The only operation that can mint an alias is an ordinary same-width
/// register move from a previously proved LOW-byte alias. Everything
/// else either preserves known-unwritten registers or clears knowledge.
fn transfer(instruction: &Instruction, aliases: &mut [bool; 16]) -> Result<(), String> {
    let op = instruction.mnemonic.as_str();
    let args = operands(&instruction.operands);
    if op.starts_with('j') || op.starts_with("loop") || op.starts_with("ret") {
        return Err("control transfer before the save guard prevents a linear entry proof".into());
    }
    match op {
        "mov" | "movb" | "movw" | "movl" | "movq" | "movabs" => {
            if args.len() != 2 {
                aliases.fill(false);
            } else if let Some(dst) = register(args[1]) {
                let origin = register(args[0]).is_some_and(|src| {
                    src.low && dst.low && src.width == dst.width && aliases[src.family]
                });
                aliases[dst.family] = origin;
            } else if !args[1].contains('(') {
                // A non-GPR destination not recognized as memory.
                aliases.fill(false);
            }
        }
        "push" | "pushq" | "pushw" => aliases[7] = false,
        "pop" | "popq" | "popw" => {
            aliases[7] = false;
            if let Some(dst) = args.first().and_then(|a| register(a)) {
                aliases[dst.family] = false;
            } else {
                aliases.fill(false);
            }
        }
        "nop" | "nopl" | "nopw" | "endbr64" | "cmp" | "cmpb" | "cmpw" | "cmpl" | "cmpq"
        | "test" | "testb" | "testw" | "testl" | "testq" => {}
        _ if ordinary_alu(op) => {
            if let Some(dst) = args.last().and_then(|a| register(a)) {
                aliases[dst.family] = false;
            } else if !args.last().is_some_and(|a| a.contains('(')) {
                aliases.fill(false);
            }
        }
        // Calls and implicit/multiple-register writes (mul/div/xchg,
        // cpuid, string ops, etc.) deliberately invalidate ALL aliases.
        _ => aliases.fill(false),
    }
    Ok(())
}

fn branch_target(instruction: &Instruction) -> Option<u64> {
    let first = instruction.operands.split_whitespace().next()?;
    u64::from_str_radix(first.trim_start_matches("0x"), 16).ok()
}

/// Find a bounded trailing literal pool, not arbitrary dead code.
/// An invalid marker must follow a RET/UD2 boundary; every instruction
/// before that boundary remains subject to the ordinary SSE proof.
fn executable_prefix(code: &[Instruction]) -> Result<usize, String> {
    let Some(first_bad) = code.iter().position(|i| i.invalid) else {
        return Ok(code.len());
    };
    let boundary = code[..first_bad]
        .iter()
        .rposition(|i| matches!(i.mnemonic.as_str(), "ret" | "retq" | "ud2"))
        .ok_or("malformed bytes have no preceding RET/UD2 suffix boundary")?;
    Ok(boundary + 1)
}

/// A deliberately bounded non-control vocabulary for the literal-pool
/// proof. Unknown instructions are not assumed to fall through. This
/// list never grants an AL alias or exempts an executable SSE operation.
fn known_fallthrough(i: &Instruction) -> bool {
    let op = i.mnemonic.as_str();
    let integer = [
        "mov", "movabs", "push", "pop", "cmp", "test", "mul", "imul", "div", "idiv", "adc", "sbb",
        "xchg",
    ];
    ordinary_alu(op)
        || integer.iter().any(|base| op == *base || ["b", "w", "l", "q"].iter().any(|suffix| op == format!("{base}{suffix}")))
        || ["e", "ne", "z", "nz", "a", "ae", "b", "be", "g", "ge", "l", "le", "s", "ns", "o", "no", "p", "np", "c", "nc"]
            .iter().any(|cc| op == format!("set{cc}") || op == format!("cmov{cc}") || op == format!("cmov{cc}q") || op == format!("cmov{cc}l"))
        || matches!(op, "movsbl" | "movsbq" | "movswl" | "movswq" | "movslq" | "movzbl" | "movzbq" | "movzwl" | "movzwq"
            | "nop" | "nopl" | "nopw" | "endbr64" | "leave" | "leaveq" | "cbtw" | "cwtl" | "cltq" | "cwtd" | "cltd" | "cqto"
            | "cbw" | "cwde" | "cdqe" | "cwd" | "cdq" | "cqo" | "ret" | "retq" | "ud2")
        // Executable vector operations are still checked, and only
        // guarded XMM stores survive check_function below.
        || vector_state(i)
}

fn check_function(code: &[Instruction]) -> Result<usize, String> {
    let vector: Vec<_> = code
        .iter()
        .enumerate()
        .filter(|(_, i)| vector_state(i))
        .collect();
    if vector.is_empty() {
        return Ok(0);
    }
    let first = vector[0].0;
    let last = vector.last().expect("nonempty").0;
    if first < 2 || vector.len() > 8 || last - first + 1 != vector.len() {
        return Err(
            "vector state is not a single prologue save block of at most eight stores".into(),
        );
    }
    let mut saved = BTreeSet::new();
    for (_, instruction) in &vector {
        if !matches!(
            instruction.mnemonic.as_str(),
            "movaps" | "movups" | "movdqa" | "movdqu"
        ) {
            return Err(format!(
                "forbidden vector/state operation: {} {}",
                instruction.mnemonic, instruction.operands
            ));
        }
        let args = operands(&instruction.operands);
        if args.len() != 2 {
            return Err("invalid XMM save operands".into());
        }
        let Some(register) = args[0]
            .strip_prefix("%xmm")
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|&n| n < 8)
        else {
            return Err("save block contains a non-argument XMM source or a load".into());
        };
        if !saved.insert(register) || !(args[1].ends_with("(%rsp)") || args[1].ends_with("(%rbp)"))
        {
            return Err("save must write each argument XMM at most once to its stack frame".into());
        }
    }
    let test = &code[first - 2];
    let jump = &code[first - 1];
    let args = operands(&test.operands);
    if !matches!(test.mnemonic.as_str(), "test" | "testb") || args.len() != 2 || args[0] != args[1]
    {
        return Err("save guard is not TEST of the same low-byte register".into());
    }
    let Some(tested) = register(args[0]).filter(|r| r.low && r.width == 8) else {
        return Err("save guard must test a LOW-8 register, not a high byte or wider alias".into());
    };
    let mut aliases = [false; 16];
    aliases[0] = true; // Only entry RAX's low byte is incoming AL.
    for instruction in &code[..first - 2] {
        transfer(instruction, &mut aliases)?;
    }
    if !aliases[tested.family] {
        return Err(
            "guarded byte has no unbroken register-copy provenance from incoming AL".into(),
        );
    }
    if !matches!(jump.mnemonic.as_str(), "je" | "jz") {
        return Err("guard must bypass the saves on zero".into());
    }
    let after = code
        .get(last + 1)
        .ok_or("save block has no following instruction")?
        .address;
    if branch_target(jump) != Some(after) {
        return Err("zero-AL branch does not skip exactly the complete save block".into());
    }
    // A later branch must not reenter the prologue after overwriting
    // its proven aliases, nor jump directly into the protected stores.
    for instruction in code {
        let op = instruction.mnemonic.as_str();
        if op.starts_with('j') || op.starts_with("loop") || op.starts_with("call") {
            if let Some(target) = branch_target(instruction) {
                if target >= code[0].address && target <= code[last].address {
                    return Err("control transfer can reenter the save prologue".into());
                }
            } else if !op.starts_with("call") {
                return Err(
                    "indirect jump prevents proving that the save block cannot be reentered".into(),
                );
            }
        }
    }
    Ok(vector.len())
}

/// Prove the invariant for every disassembled function and return the
/// total number of guarded XMM saves. Zero is valid for a genuinely
/// SSE-free object. Missing/empty disassembly is an error, not a pass.
pub fn check(text: &str) -> Result<usize, String> {
    let mut functions: BTreeMap<String, Vec<Instruction>> = BTreeMap::new();
    let mut current = None;
    let mut scope = 0usize;
    let mut scopes = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        // Archive members and sections reuse addresses. Only direct
        // control transfers in the same disassembled section can be
        // compared numerically; never mix two members' offset zero.
        if line.starts_with("Disassembly of section ") {
            scope += 1;
            current = None;
            continue;
        }
        if let Some((address, name)) = line.split_once(" <")
            && u64::from_str_radix(address, 16).is_ok()
            && let Some(name) = name.strip_suffix(">:")
        {
            if functions.contains_key(name) {
                return Err(format!("duplicate function label {name}"));
            }
            functions.insert(name.to_string(), Vec::new());
            scopes.insert(name.to_string(), scope);
            current = Some(name.to_string());
            continue;
        }
        let Some((address, rest)) = line.split_once(':') else {
            continue;
        };
        let Ok(address) = u64::from_str_radix(address, 16) else {
            continue;
        };
        let Some(name) = &current else {
            return Err("instruction outside a function".into());
        };
        let rest = rest.split('#').next().unwrap_or("").trim();
        if rest.is_empty() {
            continue;
        }
        let (mut mnemonic, mut operands) =
            rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        // GNU objdump prints instruction prefixes as separate words.
        // Strip them so e.g. `rex.W xsave` cannot hide a state op.
        while matches!(
            mnemonic,
            "data16"
                | "addr32"
                | "cs"
                | "ds"
                | "es"
                | "fs"
                | "gs"
                | "ss"
                | "lock"
                | "rep"
                | "repz"
                | "repnz"
                | "repe"
                | "repne"
        ) || mnemonic.starts_with("rex")
        {
            (mnemonic, operands) = operands
                .trim()
                .split_once(char::is_whitespace)
                .unwrap_or((operands.trim(), ""));
            if mnemonic.is_empty() {
                return Err("instruction prefix without an operation".into());
            }
        }
        let invalid = matches!(mnemonic, "(bad)" | ".byte");
        if !invalid
            && !mnemonic
                .chars()
                .all(|ch| ch.is_ascii_alphabetic() || ch.is_ascii_digit() || ch == '.')
        {
            return Err(format!("unrecognized instruction syntax: {line}"));
        }
        // Reject raw-byte columns: this gate requires the no-raw-insn
        // spelling, otherwise bytes could hide the actual mnemonic.
        if mnemonic.len() == 2 && mnemonic.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err("objdump input must use --no-show-raw-insn".into());
        }
        let code = functions.get_mut(name).expect("label inserted");
        if code.last().is_some_and(|i| i.address >= address) {
            return Err(format!("non-increasing instruction addresses in {name}"));
        }
        code.push(Instruction {
            address,
            mnemonic: mnemonic.to_ascii_lowercase(),
            operands: operands.trim().to_string(),
            invalid,
        });
    }
    if functions.is_empty() || functions.values().any(Vec::is_empty) {
        return Err("missing or empty function disassembly".into());
    }
    let mut cuts = BTreeMap::new();
    let mut pools = Vec::new();
    for (name, code) in &functions {
        let cut = executable_prefix(code).map_err(|why| format!("{name}: {why}"))?;
        cuts.insert(name.clone(), cut);
        if cut != code.len() {
            // Bound the removed interval by the next symbol in this
            // section. Without one, conservatively reject every forward
            // control target at or beyond the pool's start.
            let next = functions
                .iter()
                .filter(|(other, other_code)| {
                    scopes[*other] == scopes[name]
                        && other_code[0].address > code.last().unwrap().address
                })
                .map(|(_, other_code)| other_code[0].address)
                .min();
            pools.push((scopes[name], code[cut].address, next, name.clone()));
        }
    }
    // No edge from ANY retained function in the same object/section
    // may enter a trimmed suffix. This catches cross-function direct
    // branches too. Indirect jumps and unknown control flow make this
    // bounded proof unavailable, rather than guessing their targets.
    for (name, code) in &functions {
        let local_pools: Vec<_> = pools.iter().filter(|(s, ..)| *s == scopes[name]).collect();
        if local_pools.is_empty() {
            continue;
        }
        for instruction in &code[..cuts[name]] {
            let op = instruction.mnemonic.as_str();
            if op.starts_with('j') || op.starts_with("loop") || matches!(op, "call" | "callq") {
                if let Some(target) = branch_target(instruction) {
                    for (_, start, end, pool_name) in &local_pools {
                        if target >= *start && end.is_none_or(|end| target < end) {
                            return Err(format!(
                                "{name}: direct control target enters removed literal pool of {pool_name}"
                            ));
                        }
                    }
                } else if !matches!(op, "call" | "callq") {
                    return Err(format!(
                        "{name}: indirect jump prevents an unreachable literal-pool proof"
                    ));
                }
            } else if !known_fallthrough(instruction) {
                return Err(format!(
                    "{name}: unknown control-flow effects in literal-pool proof: {op}"
                ));
            }
        }
    }
    let mut saves = 0;
    for (name, code) in functions {
        saves += check_function(&code[..cuts[&name]]).map_err(|why| format!("{name}: {why}"))?;
    }
    Ok(saves)
}

#[cfg(test)]
mod tests {
    use super::check;

    fn fixture(prefix: &[&str], guard: &str) -> String {
        let mut lines: Vec<String> = prefix.iter().map(|s| s.to_string()).collect();
        lines.push(guard.into());
        lines.push(format!("je {:x}", lines.len() + 2));
        lines.push("movaps %xmm0,0x20(%rsp)".into());
        lines.push("ret".into());
        let mut text = "0000000000000000 <probe>:\n".to_string();
        for (address, line) in lines.iter().enumerate() {
            text.push_str(&format!(" {address:x}: {line}\n"));
        }
        text
    }

    #[test]
    fn literal_and_copied_incoming_al_are_proven() {
        assert_eq!(check(&fixture(&[], "test %al,%al")), Ok(1));
        assert_eq!(
            check(&fixture(
                &["mov %eax,%esi", "mov %rdi,%rax"],
                "test %sil,%sil"
            )),
            Ok(1)
        );
        assert_eq!(
            check(&fixture(
                &["mov %al,%cl", "mov %cl,%r8b", "xor %eax,%eax"],
                "test %r8b,%r8b"
            )),
            Ok(1)
        );
    }

    #[test]
    fn unknown_and_clobbered_aliases_are_rejected() {
        for prefix in [
            vec![],
            vec!["mov %eax,%esi", "xor %esi,%esi"],
            vec!["mov %eax,%esi", "mov $0,%sil"],
            vec!["mov %eax,%esi", "mov $0,%si"],
            vec!["mov %eax,%esi", "mov (%rsp),%esi"],
            vec!["mov %eax,%esi", "add $0,%esi"],
            vec!["mov %eax,%esi", "call 100 <other>"],
            vec!["mov %eax,%esi", "cpuid"],
            vec!["mov %eax,%esi", "xchg %esi,%edi"],
            vec!["mov %eax,%esi", "jmp 2"],
        ] {
            assert!(
                check(&fixture(&prefix, "test %sil,%sil")).is_err(),
                "{prefix:?}"
            );
        }
        assert!(check(&fixture(&["mov %eax,%esi"], "test %esi,%esi")).is_err());
        assert!(check(&fixture(&[], "test %ah,%ah")).is_err());
        assert!(check(&fixture(&["mov %ah,%cl"], "test %cl,%cl")).is_err());
        assert!(
            check(&fixture(
                &["mov %eax,%esi", "mov %esi,%edi"],
                "test %sil,%dil"
            ))
            .is_err()
        );
    }

    #[test]
    fn vector_state_without_register_operands_cannot_hide() {
        for op in [
            "vzeroupper",
            "ldmxcsr 0x20(%rsp)",
            "stmxcsr 0x20(%rsp)",
            "fxsave (%rsp)",
            "rex.W xsave (%rsp)",
            "xsave64 (%rsp)",
            "xrstor (%rsp)",
            "emms",
        ] {
            let text = format!("00000000 <plain>:\n 0: {op}\n 4: ret\n");
            assert!(check(&text).is_err(), "{op}");
        }
    }

    #[test]
    fn bypass_reentry_and_missing_guard_are_rejected() {
        let good = fixture(&[], "test %al,%al");
        assert!(check(&good.replace("je 3", "je 2")).is_err());
        assert!(check(&good.replace("je 3", "jne 3")).is_err());
        assert!(check(&good.replace("test %al,%al", "nop")).is_err());
        assert!(check(&good.replace("3: ret", "3: jmp 2")).is_err());
        assert!(check(&good.replace("3: ret", "3: jmp 0")).is_err());
        assert!(check(&good.replace("3: ret", "3: jmp *%rax")).is_err());
    }

    #[test]
    fn unreachable_max_integer_literal_pools_are_trimmed() {
        // UINT64_MAX: eight 0xff bytes do not decode as instructions.
        let mut unsigned = "00000000 <unsigned_pool>:\n 0: ret\n".to_string();
        for address in 1..=8 {
            unsigned.push_str(&format!(" {address:x}: (bad)\n"));
        }
        assert_eq!(check(&unsigned), Ok(0));
        // INT64_MAX, after padding and a nonreturning call/trap tail.
        let signed = "00000000 <signed_pool>:\n 0: call *%rax\n 2: ud2\n 4: add %al,(%rax)\n 6: (bad)\n 7: (bad)\n 8: (bad)\n 9: (bad)\n a: (bad)\n b: (bad)\n c: (bad)\n d: .byte 0x7f\n";
        assert_eq!(check(signed), Ok(0));
        let guarded =
            fixture(&[], "test %al,%al") + " 4: add %al,(%rax)\n 6: (bad)\n 7: .byte 0x7f\n";
        assert_eq!(check(&guarded), Ok(1), "AL proof survives pool trimming");
    }

    #[test]
    fn reachable_or_targeted_bad_bytes_are_never_trimmed() {
        for text in [
            "00000000 <bad>:\n 0: (bad)\n 1: ret\n",
            "00000000 <bad>:\n 0: .byte 0xff\n 1: ret\n",
            "00000000 <bad>:\n 0: jne 4\n 1: ret\n 2: add %al,(%rax)\n 4: (bad)\n",
            "00000000 <bad>:\n 0: jmp 2\n 1: ud2\n 2: add %al,(%rax)\n 4: (bad)\n",
            "00000000 <bad>:\n 0: call 4\n 1: ret\n 2: add %al,(%rax)\n 4: (bad)\n",
            "00000000 <bad>:\n 0: jmp *%rax\n 1: ret\n 2: (bad)\n",
            "00000000 <bad>:\n 0: xbegin 4\n 1: ret\n 2: (bad)\n",
            "00000000 <bad>:\n 0: iretq\n 1: ret\n 2: (bad)\n",
            "00000000 <bad>:\n 0: vzeroupper\n 1: ret\n 2: (bad)\n",
        ] {
            assert!(check(text).is_err(), "{text}");
        }
        let other_function_enters_pool =
            "00000000 <pool>:\n 0: ret\n 1: (bad)\n00000020 <other>:\n 20: jmp 1\n 22: ret\n";
        assert!(check(other_function_enters_pool).is_err());
    }

    #[test]
    fn archive_sections_do_not_share_literal_pool_addresses() {
        let text = "Disassembly of section .text:\n00000000 <pool>:\n 0: ret\n 1: (bad)\nDisassembly of section .text:\n00000000 <other>:\n 0: jmp 1\n 1: ret\n";
        assert_eq!(check(text), Ok(0));
    }

    #[test]
    fn function_boundaries_reset_provenance_and_empty_input_fails() {
        let first = fixture(&["mov %eax,%esi"], "test %sil,%sil");
        let second = fixture(&[], "test %sil,%sil").replace("<probe>", "<other>");
        assert!(check(&(first + &second)).is_err());
        assert_eq!(
            check("00000000 <plain>:\n 0: mov %rdi,%rax\n 3: ret\n"),
            Ok(0)
        );
        assert!(check("").is_err());
        assert!(check("00000000 <plain>:\n").is_err());
    }
}
