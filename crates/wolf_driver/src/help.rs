//! `wolf --help` and `wolf <command> --help` (#250).
//!
//! Until now the entire help surface was one line of twenty-three
//! pipe-separated verbs: no sentence saying what wolf is, no first
//! program, no pointer to documentation, and no per-subcommand help at
//! all. That was survivable while every reader arrived through the
//! source tree already knowing what wolf was; it stopped being
//! survivable the week `brew install wolf` and `yay -S wolf-lang`
//! started working.
//!
//! [`COMMANDS`] is the single authority: the overview renders from it,
//! `wolf <command> --help` renders one entry from it, each command's
//! own usage-error line prints from it (so the two cannot drift), and
//! the man page and the shell completions are generated from it.

use std::fmt::Write as _;

/// The repository, named in `--help` and the man page — a packaged
/// reader has no checkout to look in.
pub const HOMEPAGE: &str = "https://github.com/wolffe-lang/wolf-lang";

/// The standard library's own repository (#251): it ships separately,
/// and a packaged wolf has no way to say so except by saying so.
pub const STD_REPO: &str = "https://github.com/wolffe-lang/wolf-std";

/// One entry of the toolchain's verb table.
pub struct Cmd {
    /// The verb as typed.
    pub name: &'static str,
    /// Which block of the overview it belongs to.
    pub group: Group,
    /// One line, fitted to the overview's column.
    pub summary: &'static str,
    /// Usage lines. The first is the form; a later line that starts
    /// with the verb again is an ALTERNATIVE form (`wolf profile show`
    /// / `wolf profile merge`), and any other later line is a
    /// continuation of the one above it. All are written WITHOUT the
    /// leading `wolf `, which the renderers supply.
    pub usage: &'static [&'static str],
    /// The paragraph `wolf <name> --help` prints under the usage.
    /// Hard-wrapped as written; no runtime reflowing.
    pub about: &'static str,
}

/// The overview's blocks. A flat list of twenty-three verbs is a wall;
/// three named blocks is a table of contents.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Write a program and run it.
    Code,
    /// The package verbs (s51).
    Package,
    /// Everything that inspects rather than builds.
    Tool,
}

impl Group {
    fn heading(self) -> &'static str {
        match self {
            Group::Code => "Writing and running code:",
            Group::Package => "Packages:",
            Group::Tool => "Inspecting and reporting:",
        }
    }
}

/// Every verb `main`'s dispatch accepts, in the order the overview
/// prints them. A unit test holds this list and that dispatch to each
/// other, so a verb cannot be added to one and forgotten in the other.
pub const COMMANDS: &[Cmd] = &[
    Cmd {
        name: "build",
        group: Group::Code,
        summary: "compile an entry file to a native executable",
        usage: &[
            "build <file.lu> [-o OUT] [--emit=wir|obj|bin|llvm-ir] [--no-cache]",
            "[--verbose] [--checked] [--release] [--codegen-report]",
            "[--profile-gen[=<dir>]] [--profile=<file.wprof>] [--std-root <dir>]",
            "[--allow|--warn|--deny <W####|W##xx|warnings>] [--deny-warnings]",
            "[--error-limit=N]",
        ],
        about: "\
The entry file's directory is the package (D32): every plain `.lu` file
beside it is part of the same module. Two tiers compile the same source
— the default is the compiler's own fast backend, and `--release` goes
through LLVM. A construct the pipeline cannot yet handle is REFUSED by
name, with the deepest phase that completed; nothing is silently
interpreted instead.",
    },
    Cmd {
        name: "run",
        group: Group::Code,
        summary: "build an entry file and run it, passing on its exit code",
        usage: &[
            "run <file.lu> [-o OUT] [--emit=wir|obj|bin|llvm-ir] [--no-cache]",
            "[--verbose] [--checked] [--release] [--codegen-report]",
            "[--profile-gen[=<dir>]] [--profile=<file.wprof>] [--std-root <dir>]",
            "[--allow|--warn|--deny <W####|W##xx|warnings>] [--deny-warnings]",
            "[--error-limit=N] [prog args…]",
        ],
        about: "\
Every `wolf build` flag applies. The executable lands in the package's
`.lu-cache/bin/`, and words after the entry file are the program's own
argv. The exit code is the program's.",
    },
    Cmd {
        name: "test",
        group: Group::Code,
        summary: "discover and run `*_test.lu` files",
        usage: &["test [<dir>|<file.lu>]… [--std-root <dir>] [--deny-warnings]"],
        about: "\
No registration and no life-before-main: a file whose name ends
`_test.lu` is a test file, and each of its top-level zero-parameter
`fn test_*` is one test, run in declaration order in a fresh machine. A
test file with no `test_*` but a `main` runs black-box as one test.
Exit codes: 0 every test passed, 1 any failed or was refused, 2 a usage
or environment error.",
    },
    Cmd {
        name: "fmt",
        group: Group::Code,
        summary: "reformat source in the one canonical style",
        usage: &["fmt [--check] <file.lu|dir|->..."],
        about: "\
Files and directories (recursing into `*.lu`) are rewritten in place;
`-` formats stdin to stdout; `--check` rewrites nothing and exits
nonzero listing what is not canonical. There are no other options,
deliberately (D34): the style is fixed by the specification, not
configured per project.",
    },
    Cmd {
        name: "fix",
        group: Group::Code,
        summary: "apply the compiler's machine-applicable suggestions",
        usage: &["fix <file.lu> [--apply] [--std-root <dir>]"],
        about: "\
Dry-run by default: it lists every edit it would make and exits 1, so
nothing is rewritten behind your back. `--apply` performs them. Only
suggestions the compiler marked machine-applicable are candidates —
one that needs a human eye is reported and left alone.",
    },
    Cmd {
        name: "doc",
        group: Group::Code,
        summary: "generate the package's documentation",
        usage: &[
            "doc [--private] [--out DIR] [--check] [--open] [--json] [--coverage]",
            "[--require-docs] [--deny-warnings] [--std-root DIR] [<file.lu|dir>]",
        ],
        about: "\
Zero-config: bare `wolf doc` documents the package around the working
directory together with its resolved dependency surface, and writes
`doc/` beside it. `--check` writes nothing and exits nonzero on any
drift, which is the CI posture. A broken intra-doc link is a warning;
`--deny-warnings` makes it fail.",
    },
    Cmd {
        name: "init",
        group: Group::Package,
        summary: "promote a script to a package",
        usage: &["init --from-script <file.lu> [--dir DIR]"],
        about: "\
The script's `//!` frontmatter — dependencies, capabilities, edition —
moves into a real `wolf.pkg` verbatim, and its code moves into
`main.lu`. This is the verb the E1507 diagnostic's fix-it names.",
    },
    Cmd {
        name: "add",
        group: Group::Package,
        summary: "add a dependency to `wolf.pkg`",
        usage: &["add <alias> (--path DIR | --git URL --tag TAG) [--dir DIR]"],
        about: "\
A dependency is a path tree or a git tree at a named tag; there is no
central registry to be down. The alias is how your source spells it:
`use <alias>`.",
    },
    Cmd {
        name: "rm",
        group: Group::Package,
        summary: "remove a dependency from `wolf.pkg`",
        usage: &["rm <alias> [--dir DIR]"],
        about: "Removes the alias's entry from the manifest and the ledger.",
    },
    Cmd {
        name: "update",
        group: Group::Package,
        summary: "re-fetch pinned dependencies and refresh the ledger",
        usage: &["update [--dir DIR]"],
        about: "\
Any change to what a dependency may reach — a new capability — is shown
BEFORE the ledger is written, so a capability never arrives silently.",
    },
    Cmd {
        name: "audit",
        group: Group::Package,
        summary: "the capability tree, and what changed since `wolf.sum`",
        usage: &["audit [--ci] [--dir DIR]"],
        about: "\
Prints which capability each package in the graph reaches, and diffs
the graph's acquisitions against the witnessed ledger. `--ci` exits
nonzero when any package acquired a capability the ledger has not
seen.",
    },
    Cmd {
        name: "tree",
        group: Group::Package,
        summary: "the resolved dependency tree",
        usage: &["tree [--dir DIR]"],
        about: "The graph as resolved, one package per line, nesting by depth.",
    },
    Cmd {
        name: "why",
        group: Group::Package,
        summary: "the chain that pulls one dependency into the build",
        usage: &["why <alias> [--dir DIR]"],
        about: "\
Names who depends on `alias`, and which of them states the minimum
version that decided the resolution.",
    },
    Cmd {
        name: "vendor",
        group: Group::Package,
        summary: "mirror the resolved dependencies into `vendor/wolf/`",
        usage: &["vendor [--dir DIR]"],
        about: "\
The vendor tree IS a store, so every guarantee carries over: hashes
re-derive on use, and a tampered mirror fails exactly like a tampered
store. The next build prefers it automatically.",
    },
    Cmd {
        name: "publish",
        group: Group::Package,
        summary: "verify a package and emit its signed log record",
        usage: &["publish [--dir DIR] [--log DIR --key FILE]"],
        about: "\
Runs the manifest gates, a full resolution and the capability check,
computes the three content addresses, and emits the record. With
`--log` this is the static log's maintainer flow, which is append-only
— a published version is immutable, even for its author. Without it
the record lands in `.wolf-publish/` as a submission bundle.",
    },
    Cmd {
        name: "cache",
        group: Group::Package,
        summary: "where the script cache lives, and collecting it",
        usage: &["cache <path|gc [--dry-run] [--all]>"],
        about: "\
Nothing in the cache is ever mutated in place, so collection is its
whole lifecycle story. `path` prints the root; `gc` is the only thing
that deletes, and `--dry-run` says what it would.",
    },
    Cmd {
        name: "interface",
        group: Group::Tool,
        summary: "print every module's public interface",
        usage: &["interface [--std-root <dir>] <file.lu|dir>"],
        about: "\
Resolves the package and pretty-prints each module's `wolfi`
interface, dependencies first. Nothing is written to disk.",
    },
    Cmd {
        name: "audit-surface",
        group: Group::Tool,
        summary: "the package's complete unsafety inventory",
        usage: &["audit-surface [--std-root <dir>] <file.lu|dir>"],
        about: "\
One block per module: trusted functions with their obligations,
`unsafe` block counts, `assume` sites, re-entry doors, C imports,
inline C and assembly. Greppable and diffable on purpose — this is the
report a reviewer reads before trusting a dependency.",
    },
    Cmd {
        name: "profile",
        group: Group::Tool,
        summary: "read and merge profile-guided-optimization data",
        usage: &[
            "profile show <file.wprof>",
            "profile merge <out.wprof> <in.wprof> [in.wprof…]",
        ],
        about: "\
`wolf build --release --profile-gen` writes these; `--profile=<file>`
consumes one. `merge` combines the runs of an instrumented binary into
the single file a release build reads.",
    },
    Cmd {
        name: "c-import",
        group: Group::Tool,
        summary: "import C headers, and show what was refused",
        usage: &["c-import [options] <header.h>…"],
        about: "\
  --dump              print the artifact (the reviewable form)
  --refusals          print only what the importer refused
  -I <dir>            add an include directory (repeatable, order matters)
  -D <name>[=<value>] define a macro (repeatable)
  --cflag <flag>      pass a flag to the importer (repeatable)
  --target <triple>   import for this target (default: the host)
  --sysroot <id>      the sysroot identity to key the cache on
  --no-cache          import even if a cached artifact exists

The importer runs as a separate program (`wolf-cimport-worker`, found
beside `wolf` itself); the compiler never links a C frontend.",
    },
    Cmd {
        name: "conform-run",
        group: Group::Tool,
        summary: "observe one program and emit a conformance record",
        usage: &[
            "conform-run <file.lu> [--phase=<p>] [--seed=N] [--json]",
            "[--error-format=human|json] [--dump=regions|cfg|wir] [--zstats]",
            "[--checked] [--native] [--release] [--std-root <dir>]",
        ],
        about: "\
The differential-testing entry point: stdout carries one observation
record in the specification's format, stderr the diagnostics. This is
how the compiler and the reference interpreter are compared against
each other on the same program. Not a verb you need in order to write
wolf.",
    },
    Cmd {
        name: "lsp",
        group: Group::Tool,
        summary: "serve the Language Server Protocol over stdio",
        usage: &["lsp [--stdio]"],
        about: "\
One process, one truth: the editor sees the same resolver, the same
type checker and the same diagnostics `wolf build` does. `--stdio` is
accepted for clients that pass the conventional channel flag; it is
the only channel served.",
    },
];

/// The flags that are commands in their own right — they take no verb,
/// so they live outside [`COMMANDS`] but belong in the same overview.
const GLOBAL_FLAGS: &[(&str, &str)] = &[
    (
        "--explain E####",
        "the full explanation of one diagnostic code",
    ),
    (
        "--version",
        "this build's identity and its paired interpreter",
    ),
    (
        "--help, -h",
        "this message; `wolf <command> --help` for one command",
    ),
    ("--man", "the man page, for an installer to place"),
    (
        "--completions <shell>",
        "a completion script for bash, zsh or fish",
    ),
];

/// The first program, shown in the overview. A stranger's `--help`
/// should not send them to a website to find out how to say hello.
const FIRST_PROGRAM: &str = "\
Your first program — put it in a directory of its own:

    mkdir hello && cd hello
    cat > hello.lu <<'EOF'
    fn main() {
        print(\"hello, wolf\\n\")
    }
    EOF
    wolf run hello.lu

A DIRECTORY is a module (D32), not a file: every `.lu` file beside
`hello.lu` joins the same module, so a second scratch file with its own
`main` is an error (E0302), not a second program. Give each program its
own directory.";

/// Look up one command.
pub fn find(name: &str) -> Option<&'static Cmd> {
    COMMANDS.iter().find(|c| c.name == name)
}

/// The `usage: wolf …` block for one command, aligned across forms.
pub fn usage_text(name: &str) -> String {
    let Some(cmd) = find(name) else {
        return format!("usage: wolf {name}");
    };
    let mut out = String::new();
    for (i, form) in cmd.usage.iter().enumerate() {
        if i == 0 {
            let _ = write!(out, "usage: wolf {form}");
        } else if form.starts_with(cmd.name) {
            let _ = write!(out, "\n       wolf {form}");
        } else {
            let _ = write!(out, "\n            {form}");
        }
    }
    out
}

/// Print one command's usage to stderr and exit 2 — what every verb's
/// own argument parser does when it cannot proceed. Routing them all
/// through the table is what keeps `wolf build --help` and `wolf build`
/// with a bad flag from ever disagreeing.
pub fn usage_exit(name: &str) -> ! {
    eprintln!("{}", usage_text(name));
    eprintln!("(`wolf {name} --help` says more)");
    std::process::exit(2)
}

/// `wolf <command> --help`.
pub fn command_help(name: &str) -> String {
    let Some(cmd) = find(name) else {
        return format!("wolf {name}\n");
    };
    format!(
        "wolf {} — {}.\n\n{}\n\n{}\n",
        cmd.name,
        cmd.summary,
        usage_text(name),
        cmd.about
    )
}

/// What a word that is not a verb gets: the near-miss if there is one,
/// and where the list lives. A typo does not need the whole overview
/// reprinted at it (VOICE rule 4 — "did you mean" beats a token dump).
pub fn not_a_command(word: &str) -> String {
    let names: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
    let mut out = format!("wolf: `{word}` is not a wolf command.");
    if let Some(hit) = wolf_diag::suggest::best_match(word, &names) {
        let _ = write!(out, "\n      did you mean `wolf {hit}`?");
    }
    out.push_str("\n(`wolf --help` lists every command and shows a first program)");
    out
}

/// `wolf --help`.
pub fn overview() -> String {
    let mut out = String::new();
    out.push_str(
        "wolf — the wolf toolchain: one binary that compiles, runs, tests, formats,\n\
         documents and packages wolf programs. Wolf is a compiled systems language\n\
         whose memory lives in regions the compiler infers, so a program carries no\n\
         lifetime annotations, and whose arithmetic is checked in every profile.\n\n\
         Usage: wolf <command> [options] [file.lu]\n\n",
    );
    out.push_str(FIRST_PROGRAM);
    out.push_str("\n\n");
    let width = COMMANDS.iter().map(|c| c.name.len()).max().unwrap_or(0);
    for group in [Group::Code, Group::Package, Group::Tool] {
        let _ = writeln!(out, "{}", group.heading());
        for c in COMMANDS.iter().filter(|c| c.group == group) {
            let _ = writeln!(out, "  {:width$}  {}", c.name, c.summary);
        }
        out.push('\n');
    }
    let fwidth = GLOBAL_FLAGS.iter().map(|(f, _)| f.len()).max().unwrap_or(0);
    out.push_str("On its own:\n");
    for (flag, what) in GLOBAL_FLAGS {
        let _ = writeln!(out, "  {flag:fwidth$}  {what}");
    }
    let _ = write!(
        out,
        "\nExit codes:\n\
         \x20 0  success\n\
         \x20 1  the program did not compile, or a test or a check failed\n\
         \x20 2  a usage or environment error\n\n\
         Documentation and the specification:\n\
         \x20 {HOMEPAGE}\n\
         The standard library ships separately and no package of the compiler\n\
         carries it; point wolf at a checkout with `--std-root <dir>` or the\n\
         `WOLF_STD` environment variable:\n\
         \x20 {STD_REPO}\n"
    );
    out
}

// ------------------------------------------------------- generated files --

/// The roff man page — what an installer places in `share/man/man1`.
/// Generated rather than checked in: a hand-maintained copy of this
/// table is a copy that goes stale, which is how the compile-failure
/// footer came to name the wrong diagnostic for three releases (#249).
pub fn man_page(version: &str) -> String {
    fn esc(s: &str) -> String {
        // roff eats a leading `.` or `'`, and `\` is its escape.
        s.replace('\\', "\\e")
            .lines()
            .map(|l| {
                if l.starts_with('.') || l.starts_with('\'') {
                    format!("\\&{l}")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    let mut out = String::new();
    let _ = write!(
        out,
        ".TH WOLF 1 \"\" \"wolf {version}\" \"wolf toolchain\"\n\
         .SH NAME\n\
         wolf \\- compile, run and package wolf programs\n\
         .SH SYNOPSIS\n\
         .B wolf\n\
         .I command\n\
         [\\fIoptions\\fR] [\\fIfile.lu\\fR]\n\
         .SH DESCRIPTION\n\
         Wolf is a compiled systems language whose memory lives in regions the\n\
         compiler infers, so a program carries no lifetime annotations, and whose\n\
         arithmetic is checked in every profile.\n\
         .B wolf\n\
         is the single toolchain binary: it compiles, runs, tests, formats,\n\
         documents and packages wolf programs.\n\
         .PP\n\
         A directory is a module: every plain\n\
         .I .lu\n\
         file in it belongs to the same module, so two files in one directory may\n\
         not both define\n\
         .BR main .\n\
         Give each program its own directory.\n\
         .SH COMMANDS\n"
    );
    for c in COMMANDS {
        let _ = write!(
            out,
            ".TP\n.B wolf {}\n{}\n.PP\n.nf\n{}\n.fi\n.PP\n{}\n",
            c.name,
            esc(c.summary),
            esc(&usage_text(c.name)),
            esc(c.about)
        );
    }
    out.push_str(".SH OPTIONS\n");
    for (flag, what) in GLOBAL_FLAGS {
        let _ = write!(out, ".TP\n.B {}\n{}\n", esc(flag), esc(what));
    }
    let _ = write!(
        out,
        ".SH ENVIRONMENT\n\
         .TP\n.B WOLF_STD\n\
         The standard library tree\n\
         .RB ( use\n\
         .IR std.x\n\
         resolves to\n\
         .IR $WOLF_STD/x/ ).\n\
         The standard library ships separately, as {STD_REPO}; no package of the\n\
         compiler contains it.\n\
         .B \\-\\-std\\-root\n\
         overrides it for one invocation.\n\
         .SH EXIT STATUS\n\
         .TP\n.B 0\nSuccess.\n\
         .TP\n.B 1\nThe program did not compile, or a test or a check failed.\n\
         .TP\n.B 2\nA usage or environment error.\n\
         .SH SEE ALSO\n\
         .BR lupin (1),\n\
         the wolf reference interpreter.\n\
         .PP\n{HOMEPAGE}\n"
    );
    out
}

/// The shells `--completions` knows.
pub const SHELLS: &[&str] = &["bash", "zsh", "fish"];

/// A completion script for one shell. Verb completion only: the flag
/// surface is per-command and large, and a completion that is wrong
/// about flags is worse than one that is silent about them.
pub fn completions(shell: &str) -> Option<String> {
    let verbs: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
    let all = format!(
        "{} --explain --version --help --man --completions",
        verbs.join(" ")
    );
    Some(match shell {
        "bash" => format!(
            "# bash completion for wolf — generated by `wolf --completions bash`\n\
             _wolf() {{\n\
             \x20   local cur=\"${{COMP_WORDS[COMP_CWORD]}}\"\n\
             \x20   if [ \"$COMP_CWORD\" -eq 1 ]; then\n\
             \x20       COMPREPLY=( $(compgen -W \"{all}\" -- \"$cur\") )\n\
             \x20   else\n\
             \x20       COMPREPLY=( $(compgen -f -X '!*.lu' -- \"$cur\") $(compgen -d -- \"$cur\") )\n\
             \x20   fi\n\
             }}\n\
             complete -F _wolf wolf\n"
        ),
        "zsh" => {
            let mut s = String::from(
                "#compdef wolf\n\
                 # zsh completion for wolf — generated by `wolf --completions zsh`\n\
                 _wolf() {\n\
                 \x20   local -a cmds\n\
                 \x20   cmds=(\n",
            );
            for c in COMMANDS {
                let _ = writeln!(s, "        '{}:{}'", c.name, c.summary.replace('\'', ""));
            }
            for (flag, what) in GLOBAL_FLAGS {
                if let Some(f) = flag.split([',', ' ']).next() {
                    let _ = writeln!(s, "        '{}:{}'", f, what.replace('\'', ""));
                }
            }
            s.push_str(
                "    )\n\
                 \x20   if (( CURRENT == 2 )); then\n\
                 \x20       _describe -t commands 'wolf command' cmds\n\
                 \x20   else\n\
                 \x20       _files -g '*.lu'\n\
                 \x20   fi\n\
                 }\n\
                 _wolf \"$@\"\n",
            );
            s
        }
        "fish" => {
            let mut s = String::from(
                "# fish completion for wolf — generated by `wolf --completions fish`\n\
                 complete -c wolf -f\n",
            );
            for c in COMMANDS {
                let _ = writeln!(
                    s,
                    "complete -c wolf -n __fish_use_subcommand -a {} -d '{}'",
                    c.name,
                    c.summary.replace('\'', "")
                );
            }
            for (flag, what) in GLOBAL_FLAGS {
                if let Some(f) = flag
                    .split([',', ' '])
                    .next()
                    .and_then(|f| f.strip_prefix("--"))
                {
                    let _ = writeln!(
                        s,
                        "complete -c wolf -n __fish_use_subcommand -l {} -d '{}'",
                        f,
                        what.replace('\'', "")
                    );
                }
            }
            s.push_str("complete -c wolf -n 'not __fish_use_subcommand' -F\n");
            s
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The overview must actually answer a stranger's four questions.
    #[test]
    fn overview_says_what_wolf_is_and_how_to_start() {
        let h = overview();
        assert!(h.contains("compiled systems language"), "what wolf is");
        assert!(h.contains("wolf run hello.lu"), "a first program");
        assert!(h.contains(HOMEPAGE), "where the documentation is");
        assert!(h.contains("wolf <command> --help"), "per-command help");
        // The D32 ambush, met BEFORE it happens (#250).
        assert!(h.contains("E0302"), "the second-file rule, named");
        // The std mechanism a packaged reader cannot guess (#251).
        assert!(h.contains("WOLF_STD") && h.contains(STD_REPO));
    }

    /// Every command help is complete: a name, a usage line that starts
    /// with the verb, and prose.
    #[test]
    fn every_command_help_is_whole() {
        for c in COMMANDS {
            let h = command_help(c.name);
            assert!(h.starts_with(&format!("wolf {} — ", c.name)), "{}", c.name);
            assert!(
                h.contains(&format!("usage: wolf {}", c.name)),
                "{} usage names its own verb",
                c.name
            );
            assert!(c.about.len() > 40, "{} has real prose", c.name);
            assert!(
                !c.summary.ends_with('.'),
                "{} summary is a fragment",
                c.name
            );
            assert!(
                c.usage.first().is_some_and(|u| u.starts_with(c.name)),
                "{} names its own verb first",
                c.name
            );
        }
    }

    /// A typo gets the near-miss, not the whole overview.
    #[test]
    fn a_near_miss_is_named() {
        let m = not_a_command("biuld");
        assert!(m.contains("`biuld` is not a wolf command"));
        assert!(m.contains("did you mean `wolf build`?"), "{m}");
        assert!(m.contains("`wolf --help`"), "{m}");
        // Nothing near enough gets the pointer alone, never a wrong guess.
        let m = not_a_command("qqqqqqqq");
        assert!(!m.contains("did you mean"), "{m}");
        assert!(m.contains("`wolf --help`"), "{m}");
    }

    /// The table is a set: no verb twice.
    #[test]
    fn command_names_are_unique() {
        let mut seen: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
        let n = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), n, "no verb appears twice");
    }

    /// The man page names every verb and the environment variable.
    #[test]
    fn man_page_covers_the_table() {
        let m = man_page("0.0.0");
        assert!(m.starts_with(".TH WOLF 1"));
        for c in COMMANDS {
            assert!(m.contains(&format!(".B wolf {}\n", c.name)), "{}", c.name);
        }
        assert!(m.contains("WOLF_STD"));
        assert!(m.contains(".SH EXIT STATUS"));
    }

    /// The importer's worker name is one string in two crates; the man
    /// page and `--help` must not drift from the one that is real.
    #[test]
    fn cimport_help_names_the_real_worker() {
        let c = find("c-import").expect("c-import is a verb");
        assert!(c.about.contains(wolf_cimport::worker::WORKER_NAME));
    }

    /// Every shell we advertise produces a script that names the verbs.
    #[test]
    fn completions_cover_every_shell_and_verb() {
        for shell in SHELLS {
            let s = completions(shell).unwrap_or_else(|| panic!("{shell} is generated"));
            for c in COMMANDS {
                assert!(s.contains(c.name), "{shell} completion names {}", c.name);
            }
        }
        assert!(completions("tcsh").is_none(), "unknown shells are refused");
    }

    /// The table and `main`'s dispatch must name the same verbs. The
    /// match is not introspectable at runtime, so the test reads the
    /// source it is compiled beside — the same textual-authority trick
    /// `cargo xtask diag-catalog` uses on the code registry.
    #[test]
    fn the_table_and_the_dispatch_name_the_same_verbs() {
        // Answered by `main` directly, deliberately outside the table:
        // three are informational flags, `help` is this text itself,
        // and `bench`/`dbg` are honest not-yet stubs (D34).
        const NOT_VERBS: &[&str] = &[
            "--version",
            "--explain",
            "--help",
            "-h",
            "help",
            "--man",
            "--completions",
            "bench",
            "dbg",
        ];
        let src = include_str!("main.rs");
        let body = src
            .split_once("fn main() {")
            .expect("main exists")
            .1
            .split_once("\nfn ")
            .expect("main ends")
            .0;
        let mut dispatched: Vec<&str> = Vec::new();
        for arm in body.split("Some(").skip(1) {
            let Some((head, _)) = arm.split_once("=>") else {
                continue;
            };
            for lit in head.split('"').skip(1).step_by(2) {
                dispatched.push(lit);
            }
        }
        assert!(!dispatched.is_empty(), "the scan found the dispatch");
        for verb in &dispatched {
            assert!(
                NOT_VERBS.contains(verb) || find(verb).is_some(),
                "`wolf {verb}` is dispatched but absent from COMMANDS — \
                 add it to the table, or to NOT_VERBS if it is not a verb"
            );
        }
        for c in COMMANDS {
            assert!(
                dispatched.contains(&c.name),
                "COMMANDS lists `{}`, which `main` does not dispatch",
                c.name
            );
        }
    }

    /// A usage-error line and `--help` agree, because both read the
    /// table — the property that keeps them from drifting.
    #[test]
    fn usage_text_is_the_one_authority() {
        let u = usage_text("build");
        assert!(u.starts_with("usage: wolf build <file.lu>"));
        assert!(command_help("build").contains(&u));
        // Multi-form commands align their continuation lines.
        let p = usage_text("profile");
        assert!(p.contains("usage: wolf profile show"));
        assert!(p.contains("\n       wolf profile merge"));
    }
}
