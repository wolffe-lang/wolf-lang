//! The assembly roster (kw05, spec/04 `[abi.asm.roster]`): the routines
//! a root manifest's `asm` sources define, read from their text.
//!
//! A listed source is audited like `#[trusted]` code: what wolf may call
//! through a bodyless `extern "c" fn` on the freestanding target is what
//! the listed files export, and the export is the `.globl` (or
//! `.global`) directive — the one statement that makes a label visible
//! to the link at all. Reading the directives from the text, rather than
//! the assembled object, keeps the roster the same on every host and
//! lets the checked machine name a call into assembly without an
//! assembler. A routine made global some other way (a macro, a
//! preprocessor include) is not on the roster, and the refusal says so.

/// The names `.globl`/`.global` make visible in assembly source `text`,
/// in order of appearance, each once. Comments (`/* … */`, `//`, and
/// `#` at the start of a statement, which is also where a preprocessor
/// line starts) are skipped; statements split at newlines and `;`.
pub fn globals(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for stmt in statements(&strip_block_comments(text)) {
        let s = stmt.trim();
        let rest = if let Some(r) = s.strip_prefix(".globl") {
            r
        } else if let Some(r) = s.strip_prefix(".global") {
            r
        } else {
            continue;
        };
        // `.globl` must be followed by whitespace (`.globalfoo` is not it).
        if !rest.starts_with([' ', '\t']) {
            continue;
        }
        // A name ends at whitespace: what follows on the line (an x86
        // `# comment`) is not part of it.
        for piece in rest.split(',') {
            let name = piece.split_whitespace().next().unwrap_or("");
            if name.starts_with('#') {
                break;
            }
            if !name.is_empty() && !out.iter().any(|n| n == name) {
                out.push(name.to_string());
            }
        }
    }
    out
}

fn strip_block_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        match rest[i + 2..].find("*/") {
            Some(j) => {
                // Keep line structure: a comment spanning lines still
                // ends the statement it interrupted.
                let body = &rest[i + 2..i + 2 + j];
                out.extend(body.chars().filter(|&c| c == '\n'));
                rest = &rest[i + 2 + j + 2..];
            }
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn statements(text: &str) -> impl Iterator<Item = &str> {
    text.lines().flat_map(|line| {
        let line = match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        };
        line.split(';').map(|s| {
            let t = s.trim_start();
            if t.starts_with('#') { "" } else { t }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::globals;

    #[test]
    fn reads_globl_and_global_with_comments_and_lists() {
        let src = "/* io.S: .globl not_me */\n\
                   \t.text\n\
                   \t.globl pax_outb\n\
                   pax_outb: mov %esi, %eax // .globl nor_me\n\
                   # .globl a_comment\n\
                   #define X 1\n\
                   .global pax_exit, wolf_trap ; .globl  pax_big\n\
                   .globalfoo bar\n\
                   .globl kw_big # .globl trailing\n\
                   .globl pax_outb\n";
        assert_eq!(
            globals(src),
            ["pax_outb", "pax_exit", "wolf_trap", "pax_big", "kw_big"]
        );
    }

    #[test]
    fn an_unterminated_block_comment_hides_the_rest() {
        assert_eq!(globals(".globl a\n/* .globl b\n.globl c\n"), ["a"]);
        assert!(globals("").is_empty());
    }
}
