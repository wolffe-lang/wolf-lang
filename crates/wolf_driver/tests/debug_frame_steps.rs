//! s283: debug-tier, frame-pointer-preserving instruction-boundary witnesses.
//! Only opt-in, nonallocated .debug_frame is promised here, not .eh_frame or
//! release-tier DWARF. Debuggers are independent instruction/unwind oracles;
//! no compiler decoder is reused. Raw output and API records remain in fixtures.
use object::{Object, ObjectSection, ObjectSymbol};
use serde_json::{Value, json};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

type Check<T> = Result<T, String>;
const SOURCE: &str = "fn inspect(a: int) -> int { a ^ 7 }\nfn middle(a: int) -> int { inspect(a) + 1 }\nfn main() -> int { middle(3) - 5 }\n";
const VARIADIC_SOURCE: &str = r#"
export fn s283_variadic(n: i32, ...) -> i64 {
    // Safety: middle supplies one i64 and the originating frame stays live.
    unsafe {
        var ap = va_start()
        let value = va_arg[i64](mut ap)
        va_end(mut ap)
        value ^ 7
    }
}
fn middle(a: int) -> int { unsafe { s283_variadic(1 as i32, a as i64) as int } }
fn main() -> int { middle(3) - 4 }
"#;
const LIMIT: usize = 2048;

fn supported(row: &str) -> bool {
    if cfg!(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64")
    )) {
        true
    } else {
        eprintln!("SKIP {row} reason=requires x86-64 Linux or arm64 macOS");
        false
    }
}

fn row(name: &str, body: impl FnOnce() -> Check<()>) {
    match body() {
        Ok(()) => println!("PASS {name}"),
        Err(e) => {
            eprintln!("FAIL {name}");
            panic!("{name}: {e}");
        }
    }
}

/// Files, rather than pipes, prevent full stdout/stderr pipes from deadlocking
/// the watchdog. No shell or process-group signalling is involved.
fn run(cmd: &mut Command, dir: &Path, label: &str, seconds: u64) -> Check<(ExitStatus, String)> {
    let stdout = dir.join(format!("{label}.stdout.txt"));
    let stderr = dir.join(format!("{label}.stderr.txt"));
    cmd.stdin(Stdio::null())
        .stdout(File::create(&stdout).map_err(|e| e.to_string())?)
        .stderr(File::create(&stderr).map_err(|e| e.to_string())?);
    let mut child = cmd.spawn().map_err(|e| format!("{cmd:?}: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            state => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{label}: timeout/wait failure ({state:?}); raw transcripts: {} and {}",
                    stdout.display(),
                    stderr.display()
                ));
            }
        }
    };
    let text = format!(
        "{}\n{}",
        diagnostic_excerpt(&stdout)?,
        diagnostic_excerpt(&stderr)?
    );
    Ok((status, text))
}

// Keep full raw artifacts, but never duplicate an unbounded verifier dump in a
// Rust panic or the lane's gauntlet log. Head and tail retain the verdict.
fn diagnostic_excerpt(path: &Path) -> Check<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let mut bytes = Vec::new();
    if size <= 65536 {
        file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    } else {
        (&mut file)
            .take(32768)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        bytes.extend_from_slice(b"\n[diagnostics truncated; full output in raw artifact]\n");
        file.seek(SeekFrom::End(-32768))
            .map_err(|e| e.to_string())?;
        file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Missing executables are real environment skips; a broken installed tool,
/// timeout, unknown flag, or compiler failure is never an environment skip.
fn tool(name: &str, dir: &Path) -> Check<bool> {
    let mut cmd = Command::new(name);
    cmd.arg("--version").stdin(Stdio::null());
    // Detect only ENOENT before using the same bounded runner for the probe.
    // PATH lookup does not execute the tool or read its implementation.
    let present = if Path::new(name).components().count() > 1 {
        Path::new(name).is_file()
    } else {
        std::env::var_os("PATH")
            .is_some_and(|paths| std::env::split_paths(&paths).any(|p| p.join(name).is_file()))
    };
    if !present {
        return Ok(false);
    }
    let label = Path::new(name).file_name().unwrap().to_string_lossy();
    let (status, text) = run(&mut cmd, dir, &format!("{label}-version"), 30)?;
    if !status.success() {
        return Err(format!("{name} --version failed: {status}\n{text}"));
    }
    Ok(true)
}

#[derive(Clone)]
struct Symbol {
    key: String,
    name: String,
    start: u64,
    end: u64,
    exact_end: bool,
}

struct Fixture {
    dir: PathBuf,
    exe: PathBuf,
    dwarf: PathBuf,
    symbols: Vec<Symbol>,
}

fn fixture(name: &str, variadic: bool) -> Check<Fixture> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("debug_frame_steps_{name}"));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let source = dir.join("probe.lu");
    let exe = dir.join("probe");
    fs::write(&source, if variadic { VARIADIC_SOURCE } else { SOURCE })
        .map_err(|e| e.to_string())?;
    let (status, text) = run(
        Command::new(env!("CARGO_BIN_EXE_wolf"))
            .args(["build", "-g", "--no-cache"])
            .arg(&source)
            .arg("-o")
            .arg(&exe),
        &dir,
        "compile",
        180,
    )?;
    if !status.success() {
        return Err(format!("wolf build -g --no-cache failed: {status}\n{text}"));
    }
    if text.contains("CFI_GAP") {
        return Err(format!("compiler reported a CFI gap:\n{text}"));
    }
    let bytes = fs::read(&exe).map_err(|e| e.to_string())?;
    let obj = object::File::parse(&*bytes).map_err(|e| e.to_string())?;
    let mut symbols = Vec::new();
    let mut selectors = vec![
        ("middle", "_Wmiddle$"),
        ("wolf-main", "_Wmain$"),
        ("main-shim", "=main"),
    ];
    if variadic {
        selectors.extend([
            ("variadic-entry", "=s283_variadic"),
            ("variadic-core", "_W.va.core."),
            ("variadic-call", "_W.va.call."),
        ]);
    } else {
        selectors.insert(0, ("inspect", "_Winspect$"));
    }
    for (key, prefix) in selectors {
        let matches: Vec<_> = obj
            .symbols()
            .filter(|s| {
                let spelling = s.name().unwrap_or("");
                let spelling = if cfg!(target_os = "macos") {
                    spelling.strip_prefix('_').unwrap_or(spelling)
                } else {
                    spelling
                };
                s.is_definition()
                    && match prefix.strip_prefix('=') {
                        Some(exact) => spelling == exact,
                        None => spelling.starts_with(prefix),
                    }
            })
            .collect();
        if matches.len() != 1 {
            return Err(format!(
                "expected one owned {prefix} symbol, got {}",
                matches.len()
            ));
        }
        let symbol = &matches[0];
        let start = symbol.address();
        let exact_end = symbol.size() != 0;
        let end = if exact_end {
            start
                .checked_add(symbol.size())
                .ok_or("symbol range overflow")?
        } else {
            // Mach-O nlist has no size. Bound by the next symbol in the same
            // section (or its end); the FDE supplies the actual code end.
            let section = obj
                .section_by_index(symbol.section_index().ok_or("no code section")?)
                .map_err(|e| e.to_string())?;
            obj.symbols()
                .filter(|s| {
                    s.is_definition()
                        && s.section_index() == symbol.section_index()
                        && s.address() > start
                })
                .map(|s| s.address())
                .min()
                .unwrap_or(section.address() + section.size())
        };
        if end <= start {
            return Err(format!("empty {key} symbol range"));
        }
        symbols.push(Symbol {
            key: key.to_owned(),
            name: symbol.name().map_err(|e| e.to_string())?.to_owned(),
            start,
            end,
            exact_end,
        });
    }
    let dwarf = if cfg!(target_os = "macos") {
        exe.with_extension("dSYM")
            .join("Contents/Resources/DWARF/probe")
    } else {
        exe.clone()
    };
    Ok(Fixture {
        dir,
        exe,
        dwarf,
        symbols,
    })
}

fn fde_ranges(text: &str) -> Check<Vec<(u64, u64)>> {
    let mut ranges = Vec::new();
    for line in text
        .lines()
        .filter(|line| line.split_whitespace().any(|w| w == "FDE"))
    {
        let pc = line
            .split_once("pc=")
            .ok_or_else(|| format!("FDE has no PC range: {line}"))?
            .1;
        let (lo, hi) = pc
            .split_once("..")
            .ok_or_else(|| format!("bad FDE range: {line}"))?;
        let hi = hi
            .trim_start_matches('.')
            .split_whitespace()
            .next()
            .ok_or("missing FDE end")?;
        let hex = |s: &str| {
            u64::from_str_radix(s.trim().trim_start_matches("0x"), 16)
                .map_err(|e| format!("invalid FDE address {s:?}: {e}"))
        };
        ranges.push((hex(lo)?, hex(hi)?));
    }
    Ok(ranges)
}

fn ledger(fixture: &Fixture, row: &str) -> Check<bool> {
    let bytes =
        fs::read(&fixture.dwarf).map_err(|e| format!("{}: {e}", fixture.dwarf.display()))?;
    let obj = object::File::parse(&*bytes).map_err(|e| e.to_string())?;
    let frame = obj
        .section_by_name(".debug_frame")
        .or_else(|| obj.section_by_name("__debug_frame"))
        .ok_or("-g artifact lacks .debug_frame")?;
    if frame.size() == 0 {
        return Err("empty .debug_frame".into());
    }
    if let object::SectionFlags::Elf { sh_flags } = frame.flags()
        && sh_flags & u64::from(object::elf::SHF_ALLOC) != 0
    {
        return Err(".debug_frame is allocated".into());
    }
    let dump = std::env::var("LLVM_DWARFDUMP").unwrap_or_else(|_| "llvm-dwarfdump".into());
    if !tool(&dump, &fixture.dir)? {
        eprintln!("SKIP {row} reason={dump} not installed; set LLVM_DWARFDUMP to the executable");
        return Ok(false);
    }
    let (status, text) = run(
        Command::new(&dump).arg("--debug-frame").arg(&fixture.dwarf),
        &fixture.dir,
        "debug-frame",
        60,
    )?;
    if !status.success() {
        return Err(format!("--debug-frame: {status}\n{text}"));
    }
    let ranges = fde_ranges(&text)?;
    for symbol in &fixture.symbols {
        let matches: Vec<_> = ranges
            .iter()
            .filter(|(lo, _)| *lo == symbol.start)
            .collect();
        if matches.len() != 1 {
            return Err(format!(
                "{}: expected one owned FDE at {:#x}, got {matches:?}",
                symbol.name, symbol.start
            ));
        }
        let (_, end) = *matches[0];
        if end <= symbol.start || end > symbol.end || (symbol.exact_end && end != symbol.end) {
            return Err(format!(
                "{}: FDE end {end:#x} disagrees with symbol {:#x}..{:#x}",
                symbol.name, symbol.start, symbol.end
            ));
        }
    }
    // Verify our producer's object, not the linked Rust runtime's foreign CUs.
    // Linked FDE ranges above are still checked against the executable symbols.
    let object = fixture.dir.join("probe.o");
    let (status, text) = run(
        Command::new(env!("CARGO_BIN_EXE_wolf"))
            .args(["build", "-g", "--no-cache", "--emit=obj"])
            .arg(fixture.dir.join("probe.lu"))
            .arg("-o")
            .arg(&object),
        &fixture.dir,
        "compile-object",
        180,
    )?;
    if !status.success() || text.contains("CFI_GAP") {
        return Err(format!("object-only debug build: {status}\n{text}"));
    }
    let (status, text) = run(
        Command::new(&dump).arg("--verify").arg(&object),
        &fixture.dir,
        "verify",
        60,
    )?;
    if !status.success() || !text.contains("No errors.") {
        return Err(format!(
            "DWARF verification did not succeed: {status}\n{text}"
        ));
    }
    Ok(true)
}

fn report(name: &str, result: Check<bool>) {
    match result {
        Ok(false) => (),
        Ok(true) => println!("PASS {name}"),
        Err(e) => row(name, || Err(e)),
    }
}

fn variadic_host(row: &str) -> bool {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        true
    } else {
        eprintln!("SKIP {row} reason=owned variadic ABI supports x86-64 Linux only, not this host");
        false
    }
}

#[test]
fn dwarf_frame_ranges_and_verifier() {
    const ROW: &str = "dwarf-frame-wolf-fde-ranges";
    if supported(ROW) {
        report(ROW, fixture("ledger", false).and_then(|f| ledger(&f, ROW)));
    }
}

#[test]
fn dwarf_frame_variadic_ranges_and_verifier() {
    const ROW: &str = "dwarf-frame-variadic-fde-ranges";
    if variadic_host(ROW) {
        report(
            ROW,
            fixture("variadic-ledger", true).and_then(|f| ledger(&f, ROW)),
        );
    }
}

// API-only observations: never parse source/command echoes. Each activation
// owns an immutable caller suffix. A nested call adds a return PC in front of
// that suffix; ret removes exactly one frame; a tail jump replaces only its
// top frame. Both Python and Rust track the root's return independently.
fn symbol_index(fixture: &Fixture, key: &str) -> Check<usize> {
    fixture
        .symbols
        .iter()
        .position(|s| s.key == key)
        .ok_or_else(|| format!("missing owned symbol {key}"))
}

fn owner(fixture: &Fixture, pc: u64) -> Option<usize> {
    fixture
        .symbols
        .iter()
        .position(|s| s.start <= pc && pc < s.end)
}

fn caller_keys(key: &str) -> &'static [&'static str] {
    match key {
        "inspect" | "variadic-entry" | "variadic-call" => &["middle", "wolf-main", "main-shim"],
        "variadic-core" => &["variadic-entry", "middle", "wolf-main", "main-shim"],
        "middle" => &["wolf-main", "main-shim"],
        "wolf-main" => &["main-shim"],
        "main-shim" => &[],
        _ => &[],
    }
}

fn action(mnemonic: &str) -> &'static str {
    match mnemonic {
        "ret" | "retq" | "retab" | "retaa" => "return",
        "call" | "callq" | "bl" | "blr" => "call",
        "jmp" | "jmpq" | "b" | "br" => "tail",
        m if m.starts_with('j')
            || m.starts_with("b.")
            || matches!(m, "cbz" | "cbnz" | "tbz" | "tbnz") =>
        {
            "branch"
        }
        _ => "linear",
    }
}

fn frame_pcs(event: &Value) -> Check<Vec<Value>> {
    let frames = event["frames"].as_array().ok_or("missing API frames")?;
    if frames.is_empty() || frames.len() >= 64 {
        return Err("incomplete/unbounded API backtrace".into());
    }
    frames
        .iter()
        .map(|f| {
            // Rebased foreign PCs can be negative; preserve and compare them too.
            let pc = &f["pc"];
            if pc.as_i64().is_none() && pc.as_u64().is_none() {
                Err("missing frame PC".into())
            } else {
                Ok(pc.clone())
            }
        })
        .collect()
}

struct Activation {
    symbol: usize,
    suffix: Vec<Value>,
}

fn validate_trace(fixture: &Fixture, root: &str, required: &[&str], trace: &str) -> Check<()> {
    let root_index = symbol_index(fixture, root)?;
    let events: Vec<Value> = trace
        .lines()
        .map(|line| {
            serde_json::from_str(line).map_err(|e| format!("invalid API record: {e}: {line}"))
        })
        .collect::<Check<_>>()?;
    let mut stack: Vec<Activation> = Vec::new();
    let mut previous = "entry";
    let mut next_pc = 0;
    let mut return_frames = Vec::new();
    let mut returned = false;
    let mut steps = 0;
    let mut counts = vec![0usize; fixture.symbols.len()];
    let mut complete = vec![false; fixture.symbols.len()];
    for (index, event) in events.iter().enumerate() {
        if event["watch"].as_str() != Some(root) {
            return Err(format!("wrong active watch: {event}"));
        }
        let kind = event["event"].as_str().ok_or("missing event kind")?;
        if kind == "done" {
            if !returned
                || index + 1 != events.len()
                || !(2..=LIMIT).contains(&steps)
                || event["steps"].as_u64() != Some(steps as u64)
            {
                return Err(format!("incomplete return/instruction count: {event}"));
            }
            for key in required {
                let i = symbol_index(fixture, key)?;
                if counts[i] == 0 || !complete[i] {
                    return Err(format!(
                        "{key}: no complete entry-through-teardown witness ({} instructions)",
                        counts[i]
                    ));
                }
            }
            return Ok(());
        }
        if returned {
            return Err("event after root return".into());
        }
        if !matches!(kind, "stop" | "foreign" | "returned") {
            return Err(format!("unexpected debugger event: {event}"));
        }
        let pc = event["pc"].as_u64().ok_or("missing stop PC")?;
        let pcs = frame_pcs(event)?;
        if pcs[0] != json!(pc) {
            return Err(format!("top frame differs from machine PC: {event}"));
        }
        let actual_owner = owner(fixture, pc);
        let actual_key = actual_owner.map(|i| fixture.symbols[i].key.as_str());
        if event["active"].as_str() != actual_key {
            return Err(format!("incorrect active function: {event}"));
        }
        if kind == "returned" {
            if previous != "return" || !stack.is_empty() || pcs != return_frames {
                return Err(format!(
                    "root did not disappear leaving its unchanged caller suffix: {event}"
                ));
            }
            returned = true;
            continue;
        }
        if kind == "foreign" {
            if previous != "call" || actual_owner.is_some() || stack.is_empty() {
                return Err(format!("foreign step-out without an owned call: {event}"));
            }
            let mut expected = vec![json!(next_pc)];
            expected.extend(stack.last().unwrap().suffix.clone());
            if pcs[1..] != expected {
                return Err(format!("foreign callee lost owned caller suffix: {event}"));
            }
            let name = event["frames"][0]["name"].as_str().unwrap_or("");
            if name
                .split('(')
                .next()
                .and_then(|name| name.rsplit("::").next())
                != Some("__wolf_rt_main_start")
            {
                return Err(format!(
                    "unapproved foreign call (not measured as wolf instructions): {event}"
                ));
            }
            previous = "foreign-return";
            continue;
        }
        let current = actual_owner
            .ok_or_else(|| format!("owned instruction escaped to foreign code: {event}"))?;
        if previous == "entry" {
            if current != root_index || pc != fixture.symbols[current].start {
                return Err("breakpoint skipped machine entry/prologue".into());
            }
            for (offset, key) in caller_keys(root).iter().enumerate() {
                let caller = pcs
                    .get(offset + 1)
                    .and_then(Value::as_u64)
                    .and_then(|pc| owner(fixture, pc));
                if caller != Some(symbol_index(fixture, key)?) {
                    return Err(format!("wrong initial consecutive caller {key}: {event}"));
                }
            }
            // At least one foreign caller beyond the complete known owned chain.
            if pcs.len() < caller_keys(root).len() + 2 {
                return Err("truncated initial caller suffix".into());
            }
            stack.push(Activation {
                symbol: current,
                suffix: pcs[1..].to_vec(),
            });
        } else if previous == "call" {
            let parent = stack.last().ok_or("call without activation")?;
            let mut expected = vec![json!(next_pc)];
            expected.extend(parent.suffix.clone());
            if pc != fixture.symbols[current].start || pcs[1..] != expected {
                return Err(format!(
                    "nested call skipped entry or changed caller suffix: {event}"
                ));
            }
            stack.push(Activation {
                symbol: current,
                suffix: pcs[1..].to_vec(),
            });
        } else {
            let active = stack.last_mut().ok_or("stop after root return")?;
            if previous == "return" && pcs != return_frames {
                return Err(format!(
                    "callee return did not restore caller chain: {event}"
                ));
            }
            if active.symbol != current {
                if previous != "tail" || pc != fixture.symbols[current].start {
                    return Err(format!("unexpected active-function transition: {event}"));
                }
                complete[active.symbol] = true;
                active.symbol = current;
            }
            if pcs[1..] != active.suffix {
                return Err(format!(
                    "caller suffix changed at an instruction boundary: {event}"
                ));
            }
            if matches!(previous, "linear" | "foreign-return") && pc != next_pc {
                return Err(format!(
                    "skipped instruction or wrong foreign-call return (wanted {next_pc:#x}): {event}"
                ));
            }
        }
        let mnemonic = event["mnemonic"]
            .as_str()
            .ok_or("missing oracle mnemonic")?;
        let size = event["size"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 15)
            .ok_or("invalid instruction size")?;
        next_pc = pc.checked_add(size).ok_or("PC overflow")?;
        previous = action(mnemonic);
        steps += 1;
        counts[current] += 1;
        if steps > LIMIT {
            return Err("instruction budget exhausted".into());
        }
        if previous == "return" {
            let active = stack.pop().ok_or("ret without activation")?;
            complete[active.symbol] = true;
            return_frames = active.suffix;
        }
    }
    Err("missing final return/done API records".into())
}

fn debugger_trace(fixture: &Fixture, root: &str, row: &str) -> Check<Option<String>> {
    let debugger = if cfg!(target_os = "macos") {
        "lldb"
    } else {
        "gdb"
    };
    if !tool(debugger, &fixture.dir)? {
        eprintln!("SKIP {row} reason={debugger} not installed");
        return Ok(None);
    }
    let trace = fixture.dir.join("api-trace.jsonl");
    match fs::remove_file(&trace) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.to_string()),
    }
    let watched = &fixture.symbols[symbol_index(fixture, root)?];
    // The unmangled C entry `main` collides with the source-level wolf main.
    // Resolve ASLR through a unique linker symbol, never a DW_AT_name lookup.
    let anchor = &fixture.symbols[symbol_index(fixture, "middle")?];
    let config = fixture.dir.join("config.json");
    fs::write(
        &config,
        json!({
            "trace": trace, "entry": watched.start, "watch": root,
            "symbol": watched.name, "limit": LIMIT,
            "anchor": anchor.start, "anchor_symbol": anchor.name,
            "owned": fixture.symbols.iter().map(|s| json!({
                "key": s.key, "start": s.start, "end": s.end,
            })).collect::<Vec<_>>(),
        })
        .to_string(),
    )
    .map_err(|e| e.to_string())?;
    let mut cmd = Command::new(debugger);
    cmd.env("S283_FRAME_CONFIG", &config);
    if debugger == "gdb" {
        let script = fixture.dir.join("steps.gdb");
        fs::write(&script, format!("set pagination off\nset confirm off\nset print thread-events off\nset backtrace past-main on\npython\n{PY_COMMON}\n{GDB}\nend\nquit\n"))
            .map_err(|e| e.to_string())?;
        cmd.args(["--nx", "--batch", "-x"])
            .arg(script)
            .arg(&fixture.exe);
    } else {
        let script = fixture.dir.join("steps.py");
        fs::write(&script, format!("{PY_COMMON}\n{LLDB}\n")).map_err(|e| e.to_string())?;
        let import = format!(
            "command script import {}",
            serde_json::to_string(&script).map_err(|e| e.to_string())?
        );
        cmd.args(["--no-lldbinit", "--batch", "-o", &import])
            .arg(&fixture.exe);
    }
    let (status, text) = run(&mut cmd, &fixture.dir, debugger, 90)?;
    let records =
        fs::read_to_string(&trace).map_err(|e| format!("no API transcript: {e}\n{text}"))?;
    let lines: Vec<_> = records.lines().collect();
    if lines.len() == 1 {
        let event: Value = serde_json::from_str(lines[0]).map_err(|e| e.to_string())?;
        if event["event"] == "launch-refused" {
            eprintln!(
                "SKIP {row} reason={} (raw transcript {})",
                event["error"],
                fixture.dir.display()
            );
            return Ok(None);
        }
    }
    if !status.success() {
        return Err(format!("{debugger}: {status}\n{text}\n{records}"));
    }
    Ok(Some(records))
}

fn run_case(root: &str, variadic: bool, name: &str, required: &[&str]) {
    let leaf = root == "inspect";
    if !(if variadic {
        variadic_host(name)
    } else {
        supported(name)
    }) {
        if leaf {
            eprintln!("SKIP dwarf-frame-three-frames reason=unsupported debugger host");
        }
        return;
    }
    let result = (|| -> Check<bool> {
        let fixture = fixture(
            &format!("{}-{root}", if variadic { "variadic" } else { "chain" }),
            variadic,
        )?;
        let Some(trace) = debugger_trace(&fixture, root, name)? else {
            return Ok(false);
        };
        validate_trace(&fixture, root, required, &trace)
            .map_err(|e| format!("{e}\nraw transcripts: {}\n{trace}", fixture.dir.display()))?;
        Ok(true)
    })();
    if leaf {
        match &result {
            Ok(true) => println!("PASS dwarf-frame-three-frames"),
            Ok(false) => eprintln!(
                "SKIP dwarf-frame-three-frames reason=debugger unavailable or launch permission refused"
            ),
            Err(_) => eprintln!("FAIL dwarf-frame-three-frames"),
        }
    }
    report(name, result);
}

#[test]
fn debugger_backtrace_at_every_instruction() {
    run_case(
        "inspect",
        false,
        "dwarf-frame-instruction-steps",
        &["inspect"],
    );
}
#[test]
fn debugger_middle_at_every_instruction() {
    run_case(
        "middle",
        false,
        "dwarf-frame-middle-steps",
        &["middle", "inspect"],
    );
}
#[test]
fn debugger_wolf_main_at_every_instruction() {
    run_case(
        "wolf-main",
        false,
        "dwarf-frame-wolf-main-steps",
        &["wolf-main", "middle", "inspect"],
    );
}
#[test]
fn debugger_main_shim_at_every_instruction() {
    run_case(
        "main-shim",
        false,
        "dwarf-frame-main-shim-steps",
        &["main-shim", "wolf-main", "middle", "inspect"],
    );
}
#[test]
fn debugger_variadic_entry_at_every_instruction() {
    run_case(
        "variadic-entry",
        true,
        "dwarf-frame-variadic-entry-steps",
        &["variadic-entry", "variadic-core"],
    );
}
#[test]
fn debugger_variadic_core_at_every_instruction() {
    run_case(
        "variadic-core",
        true,
        "dwarf-frame-variadic-core-steps",
        &["variadic-core"],
    );
}
#[test]
fn debugger_variadic_chain_at_every_instruction() {
    run_case(
        "main-shim",
        true,
        "dwarf-frame-variadic-chain-steps",
        &[
            "main-shim",
            "wolf-main",
            "middle",
            "variadic-call",
            "variadic-entry",
            "variadic-core",
        ],
    );
}

#[test]
fn frame_transcript_validation_is_fail_closed() {
    row("dwarf-frame-transcript-validation", || {
        let fixture = Fixture {
            dir: PathBuf::new(),
            exe: PathBuf::new(),
            dwarf: PathBuf::new(),
            symbols: ["inspect", "middle", "wolf-main", "main-shim"]
                .into_iter()
                .enumerate()
                .map(|(i, key)| Symbol {
                    key: key.into(),
                    name: key.into(),
                    start: (i as u64 + 1) * 100,
                    end: (i as u64 + 1) * 100 + 20,
                    exact_end: true,
                })
                .collect(),
        };
        let stop = |watch: &str, pc: u64, mnemonic: &str, pcs: &[u64]| {
            json!({
                "event": "stop", "watch": watch, "pc": pc, "mnemonic": mnemonic, "size": 4,
                "active": owner(&fixture, pc).map(|i| fixture.symbols[i].key.as_str()),
                "frames": pcs.iter().map(|pc| json!({"pc": pc})).collect::<Vec<_>>(),
            })
        };
        let returned = |watch: &str, pc: u64, pcs: &[u64]| {
            let mut event = stop(watch, pc, "unused", pcs);
            event["event"] = json!("returned");
            event
        };
        let done =
            |watch: &str, steps: u64| json!({"event": "done", "watch": watch, "steps": steps});
        let encode = |events: &[Value]| {
            events
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let events = vec![
            stop("inspect", 100, "push", &[100, 204, 304, 404, 9000]),
            stop("inspect", 104, "mov", &[104, 204, 304, 404, 9000]),
            stop("inspect", 108, "ret", &[108, 204, 304, 404, 9000]),
            returned("inspect", 204, &[204, 304, 404, 9000]),
            done("inspect", 3),
        ];
        let check =
            |events: &[Value]| validate_trace(&fixture, "inspect", &["inspect"], &encode(events));
        check(&events)?;
        let mut changed = events.clone();
        changed[1]["frames"][1]["pc"] = json!(205);
        if check(&changed).is_ok() {
            return Err("accepted changed caller PC".into());
        }
        changed = events.clone();
        changed[0]["pc"] = json!(104);
        changed[0]["frames"][0]["pc"] = json!(104);
        if check(&changed).is_ok() {
            return Err("accepted breakpoint after prologue".into());
        }
        changed = events.clone();
        changed[3] = returned("inspect", 108, &[108, 204, 304, 404, 9000]);
        if check(&changed).is_ok() {
            return Err("accepted inspect surviving ret".into());
        }
        if check(&events[..3]).is_ok()
            || validate_trace(
                &fixture,
                "inspect",
                &["inspect"],
                "(gdb) bt\n#0 inspect\n#1 middle\n#2 main\nPASS",
            )
            .is_ok()
        {
            return Err("accepted incomplete/echo-only trace".into());
        }
        let nested = vec![
            stop("middle", 200, "call", &[200, 304, 404, 9000]),
            stop("middle", 100, "push", &[100, 204, 304, 404, 9000]),
            stop("middle", 104, "ret", &[104, 204, 304, 404, 9000]),
            stop("middle", 204, "mov", &[204, 304, 404, 9000]),
            stop("middle", 208, "ret", &[208, 304, 404, 9000]),
            returned("middle", 304, &[304, 404, 9000]),
            done("middle", 5),
        ];
        validate_trace(&fixture, "middle", &["middle", "inspect"], &encode(&nested))?;
        let mut bad_nested = nested.clone();
        bad_nested[2]["frames"][1]["pc"] = json!(205);
        if validate_trace(
            &fixture,
            "middle",
            &["middle", "inspect"],
            &encode(&bad_nested),
        )
        .is_ok()
        {
            return Err("accepted temporary frame with corrupt caller suffix".into());
        }
        bad_nested = nested.clone();
        bad_nested.remove(1);
        bad_nested.remove(1);
        if validate_trace(
            &fixture,
            "middle",
            &["middle", "inspect"],
            &encode(&bad_nested),
        )
        .is_ok()
        {
            return Err("accepted nexti skipping owned callee".into());
        }
        let mut foreign = stop("main-shim", 8000, "unused", &[8000, 404, 9000]);
        foreign["event"] = json!("foreign");
        foreign["frames"][0]["name"] = json!("__wolf_rt_main_start");
        let mut skipped = vec![
            stop("main-shim", 400, "call", &[400, 9000]),
            foreign,
            stop("main-shim", 404, "ret", &[404, 9000]),
            returned("main-shim", 9000, &[9000, 9010]),
            done("main-shim", 2),
        ];
        // Include the complete foreign suffix in every snapshot, including ret.
        for event in &mut skipped[..3] {
            event["frames"]
                .as_array_mut()
                .unwrap()
                .push(json!({"pc": 9010}));
        }
        validate_trace(&fixture, "main-shim", &["main-shim"], &encode(&skipped))?;
        skipped[2]["pc"] = json!(408);
        skipped[2]["frames"][0]["pc"] = json!(408);
        if validate_trace(&fixture, "main-shim", &["main-shim"], &encode(&skipped)).is_ok() {
            return Err("accepted skipped owned instruction after foreign step-out".into());
        }
        let tail = vec![
            stop("middle", 200, "mov", &[200, 304, 404, 9000]),
            stop("middle", 204, "jmp", &[204, 304, 404, 9000]),
            stop("middle", 100, "mov", &[100, 304, 404, 9000]),
            stop("middle", 104, "ret", &[104, 304, 404, 9000]),
            returned("middle", 304, &[304, 404, 9000]),
            done("middle", 4),
        ];
        validate_trace(&fixture, "middle", &["middle", "inspect"], &encode(&tail))?;
        let mut corrupt_tail = tail.clone();
        corrupt_tail[2]["frames"][1]["pc"] = json!(308);
        if validate_trace(
            &fixture,
            "middle",
            &["middle", "inspect"],
            &encode(&corrupt_tail),
        )
        .is_ok()
        {
            return Err("accepted corrupt caller suffix across an owned tail jump".into());
        }
        // dyld can be the sole caller of the synthetic main on macOS.
        let single_caller = vec![
            stop("main-shim", 400, "mov", &[400, 9000]),
            stop("main-shim", 404, "ret", &[404, 9000]),
            returned("main-shim", 9000, &[9000]),
            done("main-shim", 2),
        ];
        validate_trace(
            &fixture,
            "main-shim",
            &["main-shim"],
            &encode(&single_caller),
        )?;
        let ranges = fde_ranges("0000 0014 0000 FDE cie=0000 pc=00000064...00000078\n")?;
        if ranges != [(100, 120)] || fde_ranges("0000 FDE no-range").is_ok() {
            return Err("FDE parser did not fail closed".into());
        }
        Ok(())
    });
}

const PY_COMMON: &str = r#"
import json, os, traceback
with open(os.environ['S283_FRAME_CONFIG']) as source:
    config = json.load(source)
trace = open(config['trace'], 'w', buffering=1)
def record(kind, **fields):
    fields['event'] = kind
    fields['watch'] = config['watch']
    trace.write(json.dumps(fields, sort_keys=True) + '\n')
    trace.flush()
def permission_refusal(message):
    message = message.lower()
    return any(s in message for s in (
        'ptrace: operation not permitted', 'ptrace: permission denied',
        'personality set failed: operation not permitted',
        'not allowed to attach to process', 'failed to get task for process'))
def owner(pc):
    matches = [s['key'] for s in config['owned'] if s['start'] <= pc < s['end']]
    if len(matches) > 1:
        raise RuntimeError('overlapping owned symbol ranges')
    return matches[0] if matches else None
def drive(api):
    depth = 1
    for step in range(config['limit']):
        state = api.snapshot(True)
        state['active'] = owner(state['pc'])
        if state['active'] is None:
            raise RuntimeError('attempted to measure foreign code as owned instructions')
        record('stop', **state)
        mnemonic = state['mnemonic']
        returning = mnemonic in ('ret', 'retq', 'retab', 'retaa')
        calling = mnemonic in ('call', 'callq', 'bl', 'blr')
        api.step()
        if returning:
            depth -= 1
            if depth == 0:
                state = api.snapshot(False)
                state['active'] = owner(state['pc'])
                record('returned', **state)
                record('done', steps=step+1)
                return
        elif calling:
            callee = api.snapshot(False)
            callee['active'] = owner(callee['pc'])
            if callee['active'] is not None:
                depth += 1
            else:
                # Explicitly unmeasured foreign interval. Entry breakpoints on
                # ALL owned functions stay armed during step-out, so a callback
                # cannot silently skip owned instructions (it fails this row).
                record('foreign', **callee)
                name = (callee['frames'][0]['name'] or '').split('(')[0].rsplit('::', 1)[-1]
                if name != '__wolf_rt_main_start':
                    raise RuntimeError('unapproved foreign step-out: ' + name)
                api.finish()
                resumed = api.snapshot(False)
                if resumed['pc'] != state['pc'] + state['size']:
                    raise RuntimeError('foreign step-out skipped return PC or hit owned callback')
    raise RuntimeError('instruction budget exhausted without root return')
"#;

const GDB: &str = r#"
import gdb
class API:
    def __init__(self, base):
        self.base = base
    def snapshot(self, disassemble):
        frame = gdb.newest_frame()
        if frame is None:
            raise RuntimeError('inferior exited before return witness')
        pc = int(frame.pc())
        frames = []
        cursor = frame
        while cursor is not None:
            if len(frames) == 64:
                raise RuntimeError('backtrace frame limit reached')
            frames.append(dict(pc=int(cursor.pc())-self.base, load_pc=int(cursor.pc()), name=cursor.name()))
            cursor = cursor.older()
        state = dict(pc=pc-self.base, load_pc=pc, frames=frames)
        if disassemble:
            instruction = frame.architecture().disassemble(pc, count=1)[0]
            state.update(mnemonic=instruction['asm'].split()[0].lower(),
                         size=instruction['length'], instruction=instruction['asm'])
        return state
    def step(self):
        gdb.newest_frame().select()
        gdb.execute('stepi', to_string=True)
    def finish(self):
        gdb.newest_frame().select()
        gdb.execute('finish', to_string=True)
def witness():
    anchor = "(char *)&'%s'" % config['anchor_symbol']
    address = '(%s + (%d))' % (anchor, config['entry'] - config['anchor'])
    bp = gdb.Breakpoint('*' + address, temporary=True, internal=True)
    try:
        gdb.execute('run', to_string=True)
    except gdb.error as error:
        if permission_refusal(str(error)):
            record('launch-refused', error=str(error))
            return
        raise
    base = int(gdb.parse_and_eval(anchor)) - config['anchor']
    guards = [gdb.Breakpoint('*%#x' % (s['start']+base), internal=True)
              for s in config['owned']]
    drive(API(base))
try:
    witness()
except Exception as error:
    record('error', error=str(error), traceback=traceback.format_exc())
finally:
    try:
        if gdb.selected_inferior().pid:
            gdb.execute('kill', to_string=True)
    finally:
        trace.close()
"#;

const LLDB: &str = r#"
import lldb
class API:
    def __init__(self, target, process, thread, base, guards):
        self.target, self.process, self.thread, self.base = target, process, thread, base
        self.guards = guards
    def snapshot(self, disassemble):
        if self.process.GetState() != lldb.eStateStopped:
            raise RuntimeError('inferior not stopped')
        frame = self.thread.GetFrameAtIndex(0)
        if not frame.IsValid():
            raise RuntimeError('missing top frame')
        count = self.thread.GetNumFrames()
        if count >= 64:
            raise RuntimeError('backtrace frame limit reached')
        frames = []
        for depth in range(count):
            caller = self.thread.GetFrameAtIndex(depth)
            frames.append(dict(pc=caller.GetPC()-self.base, load_pc=caller.GetPC(), name=caller.GetFunctionName()))
        state = dict(pc=frame.GetPC()-self.base, load_pc=frame.GetPC(), frames=frames)
        if disassemble:
            instructions = self.target.ReadInstructions(frame.GetPCAddress(), 1)
            if instructions.GetSize() != 1:
                raise RuntimeError('cannot disassemble instruction at stop')
            instruction = instructions.GetInstructionAtIndex(0)
            state.update(mnemonic=instruction.GetMnemonic(self.target).lower(),
                         size=instruction.GetByteSize(), instruction=str(instruction))
        return state
    def step(self):
        # An armed entry guard at the current PC can cause LLDB to stop twice
        # at that same instruction. Disable only that guard for this one step;
        # all guards are armed again before any foreign StepOut interval.
        pc = self.thread.GetFrameAtIndex(0).GetPC()
        current = [guard for guard in self.guards
                   if guard.GetLocationAtIndex(0).GetAddress().GetLoadAddress(self.target) == pc]
        for guard in current:
            guard.SetEnabled(False)
        try:
            self.thread.StepInstruction(False)
        finally:
            for guard in current:
                guard.SetEnabled(True)
        if self.thread.GetStopReason() not in (lldb.eStopReasonPlanComplete, lldb.eStopReasonBreakpoint):
            raise RuntimeError('unexpected single-step stop reason %s' % self.thread.GetStopReason())
    def finish(self):
        self.thread.StepOut()
        if self.thread.GetStopReason() not in (lldb.eStopReasonPlanComplete, lldb.eStopReasonBreakpoint):
            raise RuntimeError('unexpected foreign step-out stop reason %s' % self.thread.GetStopReason())
def __lldb_init_module(debugger, internal_dict):
    debugger.SetAsync(False)
    target = debugger.GetSelectedTarget()
    process = None
    try:
        address = target.ResolveFileAddress(config['entry'])
        if not address.IsValid():
            raise RuntimeError('entry file address did not resolve')
        bp = target.BreakpointCreateBySBAddress(address)
        if not bp.IsValid() or bp.GetNumLocations() != 1:
            raise RuntimeError('machine-entry breakpoint must have one location')
        error = lldb.SBError()
        process = target.Launch(lldb.SBLaunchInfo([]), error)
        if error.Fail():
            message = error.GetCString() or 'unknown launch error'
            if permission_refusal(message):
                record('launch-refused', error=message)
                return
            raise RuntimeError(message)
        base = address.GetLoadAddress(target) - config['entry']
        thread = process.GetSelectedThread()
        if not thread.IsValid() or thread.GetStopReason() != lldb.eStopReasonBreakpoint:
            raise RuntimeError('launch did not stop on entry breakpoint')
        bp.SetEnabled(False)
        guards = [target.BreakpointCreateBySBAddress(target.ResolveFileAddress(s['start']))
                  for s in config['owned']]
        if any(not guard.IsValid() or guard.GetNumLocations() != 1 for guard in guards):
            raise RuntimeError('owned callback guard did not resolve')
        drive(API(target, process, thread, base, guards))
    except Exception as error:
        record('error', error=str(error), traceback=traceback.format_exc())
    finally:
        if process is not None and process.IsValid():
            process.Kill()
        trace.close()
"#;
