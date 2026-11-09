//! The prelude census (s289, wolf-lang#1083 and #708): every ambient name,
//! used as a value (`let f = NAME`), called (`NAME()`) and as a member's
//! receiver (`NAME.member()`), and none may answer "a std/prelude stub
//! without a signature" — the refusal that read as the compiler's limit
//! wherever a name had no typing behind it, whether the program's mistake
//! or a real gap. Each class answers in its own words instead:
//!
//! - a prelude type (`List`, `Scope`, `range`, …) is a `type`-kinded value,
//!   as a builtin type is, and calling one bare is E0406 naming it;
//! - a builtin function or intrinsic used as a value is declined by name
//!   ("call it by name"): lupin runs those, so it is the compiler's limit;
//! - `min` is typed by its own clause (`[type.min]`, the third change):
//!   called with no operands it is E0402, and as a value it is a builtin;
//! - the free `zip` has no meaning (`[type.min.zip]`): every use is
//!   declined with that reason;
//! - a provisional stand-in nothing defines is E0301.
//!
//! At the base 431 of the 687 uses answered with the stub.

use wolf_sema::prelude::{self, Kind};
use wolf_sema::{
    AliasTable, MemoryLoader, Typecheck, resolve_package_with, typecheck_package_with,
};

const STUB: &str = "a std/prelude stub without a signature";
/// A builtin function or intrinsic used as a value.
const BUILTIN_AS_VALUE: &str = "a builtin function used as a value (call it by name)";
/// The free `zip` (`[type.min.zip]`).
const FREE_ZIP: &str = "the prelude's free `zip`, which no clause gives a meaning \
                        (`std.list`'s method `xs.zip(ys)` is the spelling)";

/// The program's resolve diagnostics' codes, and its typecheck when it
/// resolves.
fn run(src: &str) -> (Vec<String>, Option<Typecheck>) {
    let mut ml = MemoryLoader::new("census");
    ml.add_file(&[], "main.lu", src);
    let res = resolve_package_with(&mut ml, &AliasTable::default(), true).expect("root loads");
    let codes: Vec<String> = res
        .diagnostics
        .iter()
        .filter(|d| d.severity == wolf_diag::Severity::Error)
        .map(|d| d.code.as_str().to_string())
        .collect();
    if !codes.is_empty() {
        return (codes, None);
    }
    (codes, Some(typecheck_package_with(&res.package, true)))
}

#[derive(Debug, PartialEq)]
enum Answer {
    Resolve(Vec<String>),
    NotYet(&'static str),
    Errors(Vec<String>),
    Clean,
}

fn answer(src: &str) -> Answer {
    let (resolve, tc) = run(src);
    let Some(tc) = tc else {
        return Answer::Resolve(resolve);
    };
    if let Some(n) = tc.not_yet.first() {
        return Answer::NotYet(n.construct);
    }
    let errors: Vec<String> = tc
        .diagnostics
        .iter()
        .filter(|d| d.severity == wolf_diag::Severity::Error)
        .map(|d| d.code.as_str().to_string())
        .collect();
    if errors.is_empty() {
        Answer::Clean
    } else {
        Answer::Errors(errors)
    }
}

const USES: [(&str, &str); 3] = [
    ("value", "fn main() -> int {\n    let f = NAME\n    0\n}\n"),
    ("call", "fn main() -> int {\n    NAME()\n    0\n}\n"),
    (
        "member",
        "fn main() -> int {\n    NAME.member()\n    0\n}\n",
    ),
];

/// Every ambient name, three ways: none answers with the stub.
#[test]
fn no_ambient_name_answers_with_the_stub() {
    let mut stubs = Vec::new();
    let mut uses = 0;
    for row in prelude::names() {
        for (use_, tmpl) in USES {
            uses += 1;
            if answer(&tmpl.replace("NAME", row.name)) == Answer::NotYet(STUB) {
                stubs.push(format!("{} ({use_})", row.name));
            }
        }
    }
    assert!(uses >= 687, "the census walked {uses} uses");
    assert!(
        stubs.is_empty(),
        "{} of {uses} uses answer {STUB:?}: {}",
        stubs.len(),
        stubs.join(", ")
    );
}

/// The words each class answers with, held so a name added later cannot
/// fall back to a refusal that blames nothing.
#[test]
fn each_class_answers_in_its_own_words() {
    for row in prelude::names() {
        let name = row.name;
        let value = answer(&USES[0].1.replace("NAME", name));
        let call = answer(&USES[1].1.replace("NAME", name));
        if prelude::PRELUDE_PROVISIONAL.contains(&name) {
            for (use_, tmpl) in USES {
                assert_eq!(
                    answer(&tmpl.replace("NAME", name)),
                    Answer::Errors(vec!["E0301".into()]),
                    "the stand-in {name} ({use_})"
                );
            }
            continue;
        }
        if name == "min" {
            assert_eq!(
                call,
                Answer::Errors(vec!["E0402".into()]),
                "`min` called with no operands"
            );
            assert_eq!(value, Answer::NotYet(BUILTIN_AS_VALUE), "`min` as a value");
            continue;
        }
        if name == "zip" {
            for (use_, tmpl) in USES {
                assert_eq!(
                    answer(&tmpl.replace("NAME", name)),
                    Answer::NotYet(FREE_ZIP),
                    "the free zip ({use_})"
                );
            }
            continue;
        }
        match row.kind {
            Kind::Type => {
                assert!(
                    !matches!(value, Answer::NotYet(_)),
                    "the type {name} as a value: {value:?}"
                );
                if !matches!(name, "Mutex" | "VaList") {
                    assert_eq!(
                        call,
                        Answer::Errors(vec!["E0406".into()]),
                        "the type {name}, called bare"
                    );
                }
            }
            Kind::Function | Kind::Intrinsic if !matches!(name, "print" | "print_raw") => {
                assert_eq!(
                    value,
                    Answer::NotYet(BUILTIN_AS_VALUE),
                    "the builtin {name} as a value"
                );
            }
            _ => {}
        }
    }
}
