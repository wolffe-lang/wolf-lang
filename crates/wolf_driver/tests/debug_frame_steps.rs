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
const LIMIT: usize = 512;

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
        fs::read_to_string(stdout).map_err(|e| e.to_string())?,
        fs::read_to_string(stderr).map_err(|e| e.to_string())?
    );
    Ok((status, text))
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

fn fixture(name: &str) -> Check<Fixture> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("debug_frame_steps_{name}"));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let source = dir.join("probe.lu");
    let exe = dir.join("probe");
    fs::write(&source, SOURCE).map_err(|e| e.to_string())?;
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
    for name in ["inspect", "middle", "main"] {
        let prefix = format!("_W{name}$");
        let matches: Vec<_> = obj
            .symbols()
            .filter(|s| {
                let spelling = s.name().unwrap_or("");
                let spelling = if cfg!(target_os = "macos") {
                    spelling.strip_prefix('_').unwrap_or(spelling)
                } else {
                    spelling
                };
                s.is_definition() && spelling.starts_with(&prefix)
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
            return Err(format!("empty {name} symbol range"));
        }
        symbols.push(Symbol {
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

#[test]
fn dwarf_frame_ranges_and_verifier() {
    const ROW: &str = "dwarf-frame-wolf-fde-ranges";
    if !supported(ROW) {
        return;
    }
    let mut skipped = false;
    let result = (|| -> Check<()> {
        let fixture = fixture("ledger")?;
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
            eprintln!(
                "SKIP {ROW} reason={dump} not installed; set LLVM_DWARFDUMP to the executable"
            );
            skipped = true;
            return Ok(());
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
        let (status, text) = run(
            Command::new(&dump).arg("--verify").arg(&fixture.dwarf),
            &fixture.dir,
            "verify",
            60,
        )?;
        if !status.success() || !text.contains("No errors.") {
            return Err(format!(
                "DWARF verification did not succeed: {status}\n{text}"
            ));
        }
        Ok(())
    })();
    if !skipped {
        row(ROW, || result);
    }
}

// The sidecar contains only debugger API observations, never command/source
// echoes. PCs are rebased to file addresses so Rust independently checks the
// first THREE consecutive frames, not a filtered subsequence of any backtrace.
fn validate_trace(fixture: &Fixture, trace: &str) -> Check<()> {
    let events: Vec<Value> = trace
        .lines()
        .map(|line| {
            serde_json::from_str(line).map_err(|e| format!("invalid API record: {e}: {line}"))
        })
        .collect::<Check<_>>()?;
    let mut baseline = None;
    let mut steps = 0;
    let mut previous_ret = false;
    let mut returned = false;
    let mut previous_pc = None;
    for (index, event) in events.iter().enumerate() {
        let kind = event["event"].as_str().ok_or("missing event kind")?;
        match kind {
            "stop" => {
                if returned {
                    return Err("stop after returned record".into());
                }
                let pc = event["pc"].as_u64().ok_or("missing stop PC")?;
                let frames = event["frames"].as_array().ok_or("missing frames")?;
                let expected = if previous_ret {
                    &fixture.symbols[1..]
                } else {
                    &fixture.symbols[..]
                };
                // Foreign frames may have sentinel PCs below the image slide;
                // retain them in the raw trace, but judge only the owned prefix.
                let pcs: Vec<u64> = frames
                    .iter()
                    .take(expected.len())
                    .map(|f| {
                        f["pc"]
                            .as_u64()
                            .ok_or_else(|| "missing owned frame PC".into())
                    })
                    .collect::<Check<_>>()?;
                if pcs.len() < expected.len() || pcs.first() != Some(&pc) {
                    return Err(format!("incomplete backtrace: {event}"));
                }
                for (actual, symbol) in pcs.iter().zip(expected) {
                    if *actual < symbol.start || *actual >= symbol.end {
                        return Err(format!(
                            "PC {actual:#x} not in {} {:#x}..{:#x}: {event}",
                            symbol.name, symbol.start, symbol.end
                        ));
                    }
                }
                if let Some((middle, main)) = baseline {
                    let actual = if previous_ret {
                        (pcs[0], pcs[1])
                    } else {
                        (pcs[1], pcs[2])
                    };
                    if actual != (middle, main) {
                        return Err(format!("caller PCs changed: {event}"));
                    }
                } else {
                    if pc != fixture.symbols[0].start {
                        return Err("breakpoint skipped machine entry/prologue".into());
                    }
                    baseline = Some((pcs[1], pcs[2]));
                }
                if previous_ret {
                    returned = true;
                } else {
                    if previous_pc == Some(pc) {
                        return Err("single-step made no progress".into());
                    }
                    previous_pc = Some(pc);
                    let mnemonic = event["mnemonic"]
                        .as_str()
                        .ok_or("missing instruction mnemonic")?;
                    previous_ret = matches!(mnemonic, "ret" | "retq" | "retab" | "retaa");
                    steps += 1;
                }
            }
            "done" if returned && index + 1 == events.len() => {
                if !(3..=LIMIT).contains(&steps) || event["steps"].as_u64() != Some(steps as u64) {
                    return Err(format!("invalid instruction count {steps}: {event}"));
                }
                return Ok(());
            }
            _ => return Err(format!("unexpected/incomplete debugger API event: {event}")),
        }
    }
    Err("missing final return/done API records".into())
}

#[test]
fn debugger_backtrace_at_every_instruction() {
    const ROW: &str = "dwarf-frame-instruction-steps";
    if !supported(ROW) {
        return;
    }
    let result = (|| -> Check<bool> {
        let fixture = fixture("steps")?;
        let debugger = if cfg!(target_os = "macos") {
            "lldb"
        } else {
            "gdb"
        };
        if !tool(debugger, &fixture.dir)? {
            eprintln!("SKIP {ROW} reason={debugger} not installed");
            eprintln!("SKIP dwarf-frame-three-frames reason={debugger} not installed");
            return Ok(false);
        }
        let trace = fixture.dir.join("api-trace.jsonl");
        match fs::remove_file(&trace) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.to_string()),
        }
        let config = fixture.dir.join("config.json");
        fs::write(
            &config,
            json!({
                "trace": trace, "entry": fixture.symbols[0].start,
                "symbol": fixture.symbols[0].name, "limit": LIMIT,
            })
            .to_string(),
        )
        .map_err(|e| e.to_string())?;
        let mut cmd = Command::new(debugger);
        cmd.env("S283_FRAME_CONFIG", &config);
        if debugger == "gdb" {
            let script = fixture.dir.join("steps.gdb");
            fs::write(&script, format!("set pagination off\nset confirm off\nset print thread-events off\npython\n{PY_COMMON}\n{GDB}\nend\nquit\n"))
                .map_err(|e| e.to_string())?;
            cmd.args(["--nx", "--batch", "-x"])
                .arg(script)
                .arg(&fixture.exe);
        } else {
            let script = fixture.dir.join("steps.py");
            fs::write(&script, format!("{PY_COMMON}\n{LLDB}\n")).map_err(|e| e.to_string())?;
            // JSON quoting is also suitable for LLDB's command path argument.
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
        // Only a structured launch error, before any stop, can skip. A broken
        // unwind, absent Python support, or stepping error cannot use this path.
        let lines: Vec<_> = records.lines().collect();
        if lines.len() == 1 {
            let event: Value = serde_json::from_str(lines[0]).map_err(|e| e.to_string())?;
            if event["event"] == "launch-refused" {
                eprintln!(
                    "SKIP {ROW} reason={} (raw transcript {})",
                    event["error"],
                    fixture.dir.display()
                );
                eprintln!(
                    "SKIP dwarf-frame-three-frames reason=debugger launch permission refused"
                );
                return Ok(false);
            }
        }
        if !status.success() {
            return Err(format!("{debugger}: {status}\n{text}\n{records}"));
        }
        validate_trace(&fixture, &records).map_err(|e| {
            format!(
                "{e}\nraw transcripts: {}\n{text}\n{records}",
                fixture.dir.display()
            )
        })?;
        Ok(true)
    })();
    match result {
        Ok(false) => (),
        Ok(true) => {
            println!("PASS dwarf-frame-three-frames");
            println!("PASS {ROW}");
        }
        Err(e) => {
            eprintln!("FAIL dwarf-frame-three-frames");
            row(ROW, || Err(e));
        }
    }
}

#[test]
fn frame_transcript_validation_is_fail_closed() {
    row("dwarf-frame-transcript-validation", || {
        let fixture = Fixture {
            dir: PathBuf::new(),
            exe: PathBuf::new(),
            dwarf: PathBuf::new(),
            symbols: [100, 200, 300]
                .into_iter()
                .map(|start| Symbol {
                    name: format!("owned-{start}"),
                    start,
                    end: start + 20,
                    exact_end: true,
                })
                .collect(),
        };
        let stop = |pc, mnemonic: &str, pcs: &[u64]| {
            json!({
                "event": "stop", "pc": pc, "mnemonic": mnemonic,
                "frames": pcs.iter().map(|pc| json!({"pc": pc})).collect::<Vec<_>>(),
            })
        };
        let events = vec![
            stop(100, "push", &[100, 204, 304]),
            stop(104, "mov", &[104, 204, 304]),
            stop(108, "ret", &[108, 204, 304]),
            stop(204, "add", &[204, 304]),
            json!({"event": "done", "steps": 3}),
        ];
        let encode = |events: &[Value]| {
            events
                .iter()
                .map(Value::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        };
        validate_trace(&fixture, &encode(&events))?;
        let mut changed = events.clone();
        changed[1]["frames"][1]["pc"] = json!(205);
        if validate_trace(&fixture, &encode(&changed)).is_ok() {
            return Err("accepted a caller PC change inside the same function".into());
        }
        changed = events.clone();
        changed[0]["pc"] = json!(104);
        changed[0]["frames"][0]["pc"] = json!(104);
        if validate_trace(&fixture, &encode(&changed)).is_ok() {
            return Err("accepted a breakpoint after the prologue".into());
        }
        changed = events.clone();
        changed[3] = stop(108, "ret", &[108, 204, 304]);
        if validate_trace(&fixture, &encode(&changed)).is_ok() {
            return Err("accepted inspect still present after ret".into());
        }
        if validate_trace(&fixture, &encode(&events[..3])).is_ok()
            || validate_trace(&fixture, "(gdb) bt\n#0 inspect\n#1 middle\n#2 main\nPASS").is_ok()
        {
            return Err("accepted an incomplete/echo-only transcript".into());
        }
        let ranges = fde_ranges("0000 0014 0000 FDE cie=0000 pc=00000064...00000078\n")?;
        if ranges != [(100, 120)] || fde_ranges("0000 FDE no-range").is_ok() {
            return Err("FDE range parser did not fail closed".into());
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
    trace.write(json.dumps(fields, sort_keys=True) + '\n')
    trace.flush()
def permission_refusal(message):
    message = message.lower()
    return any(s in message for s in (
        'ptrace: operation not permitted',
        'ptrace: permission denied',
        'personality set failed: operation not permitted',
        'not allowed to attach to process',
        'failed to get task for process'))
def is_return(mnemonic):
    return mnemonic in ('ret', 'retq', 'retab', 'retaa')
"#;

const GDB: &str = r#"
import gdb
def witness():
    # A quoted linker symbol resolves to its first machine byte, unlike a
    # source/name breakpoint which commonly skips the prologue.
    address = "&'%s'" % config['symbol']
    bp = gdb.Breakpoint('*' + address, temporary=True, internal=True)
    try:
        gdb.execute('run', to_string=True)
    except gdb.error as error:
        if permission_refusal(str(error)):
            record('launch-refused', error=str(error))
            return
        raise
    base = int(gdb.parse_and_eval(address)) - config['entry']
    returned = False
    for step in range(config['limit'] + 1):
        frame = gdb.newest_frame()
        if frame is None:
            raise RuntimeError('inferior exited before return witness')
        pc = int(frame.pc())
        frames = []
        cursor = frame
        for depth in range(32):
            if cursor is None:
                break
            frames.append(dict(pc=int(cursor.pc()) - base, name=cursor.name()))
            cursor = cursor.older()
        instruction = frame.architecture().disassemble(pc, count=1)[0]
        mnemonic = instruction['asm'].split()[0].lower()
        record('stop', pc=pc-base, frames=frames, mnemonic=mnemonic,
               instruction=instruction['asm'])
        if returned:
            record('done', steps=step)
            return
        if step == config['limit']:
            raise RuntimeError('instruction budget exhausted without return')
        returned = is_return(mnemonic)
        # stepi, never nexti: every executed instruction, including ret.
        gdb.execute('stepi', to_string=True)
    raise RuntimeError('unreachable')
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
        launch = lldb.SBLaunchInfo([])
        error = lldb.SBError()
        process = target.Launch(launch, error)
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
        returned = False
        for step in range(config['limit'] + 1):
            if process.GetState() != lldb.eStateStopped:
                raise RuntimeError('inferior not stopped')
            frame = thread.GetFrameAtIndex(0)
            if not frame.IsValid():
                raise RuntimeError('missing top frame')
            pc = frame.GetPC()
            frames = []
            for depth in range(min(thread.GetNumFrames(), 32)):
                caller = thread.GetFrameAtIndex(depth)
                frames.append(dict(pc=caller.GetPC()-base, name=caller.GetFunctionName()))
            instructions = target.ReadInstructions(frame.GetPCAddress(), 1)
            if instructions.GetSize() != 1:
                raise RuntimeError('cannot disassemble instruction at stop')
            instruction = instructions.GetInstructionAtIndex(0)
            mnemonic = instruction.GetMnemonic(target).lower()
            record('stop', pc=pc-base, frames=frames, mnemonic=mnemonic,
                   instruction=str(instruction))
            if returned:
                record('done', steps=step)
                return
            if step == config['limit']:
                raise RuntimeError('instruction budget exhausted without return')
            returned = is_return(mnemonic)
            thread.StepInstruction(False)
            if thread.GetStopReason() != lldb.eStopReasonPlanComplete:
                raise RuntimeError('single-step stopped for unexpected reason %s' % thread.GetStopReason())
        raise RuntimeError('unreachable')
    except Exception as error:
        record('error', error=str(error), traceback=traceback.format_exc())
    finally:
        if process is not None and process.IsValid():
            process.Kill()
        trace.close()
"#;
