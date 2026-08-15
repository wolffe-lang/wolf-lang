//! The importer **process contract**.
//!
//! The compiler never links a C frontend. It spawns a worker and talks
//! to it over stdio. That process boundary is not an implementation
//! detail — it is the mechanism that keeps D33 and D17 both true at
//! once (see the crate docs), so the protocol is specified in a form
//! any language can implement in an afternoon:
//!
//! ```text
//! → wolf-cimport 1          the version handshake, first line, both ways
//! → import
//! → target x86_64-unknown-linux-gnu
//! → sysroot b3:9f3c…              (optional)
//! → header stdlib.h               (repeatable, order significant)
//! → include /usr/include          (repeatable, order significant)
//! → define _GNU_SOURCE=1          (repeatable)
//! → cflag -std=c17                (repeatable)
//! → end
//! ← wolf-cimport 1
//! ← ok 40213
//! ← <40213 bytes of artifact>
//! ```
//!
//! or, when the worker cannot answer at all (as opposed to refusing
//! individual declarations, which is an artifact, not an error):
//!
//! ```text
//! ← err could not find <stdlib.h> in any include path
//! ```
//!
//! Macro re-expansion rides the same channel. Macros stay *alive*: the
//! artifact records a function-like macro's tokens, and at a wolf call
//! site the worker re-expands it in the original TU's context against
//! the call-site argument types.
//!
//! ```text
//! → expand-macro
//! → key b3:9f3c…                  the import this macro belongs to
//! → macro FD_SET
//! → argtype int
//! → argtype fd_set *
//! → end
//! ← ok-tokens 7
//! ← __FDS_BIT
//! ← (
//! ← …
//! ```
//!
//! Text in, length-prefixed binary out. Line-oriented so a worker can
//! be written with `getline`, length-prefixed so the artifact never has
//! to be escaped.

use crate::cache::ImportRequest;

/// The protocol version. Both sides announce it; a mismatch is a
/// refusal, never a best-effort parse.
pub const PROTOCOL_VERSION: u32 = 1;

/// The handshake line both directions start with.
pub fn handshake() -> String {
    format!("wolf-cimport {PROTOCOL_VERSION}")
}

/// A request to a worker.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Request {
    /// Import a header set. Answered with an artifact.
    Import(ImportRequest),
    /// Re-expand a function-like macro against call-site argument
    /// types. Answered with a token sequence.
    ExpandMacro {
        /// The cache key of the import this macro came from — the
        /// worker needs the original TU context to expand correctly.
        key: String,
        name: String,
        /// The argument types as C spellings, in order.
        arg_types: Vec<String>,
    },
}

impl Request {
    /// Render the request's wire form (handshake included).
    pub fn render(&self) -> String {
        let mut s = handshake();
        s.push('\n');
        match self {
            Request::Import(r) => {
                s.push_str("import\n");
                s.push_str("target ");
                s.push_str(&r.target);
                s.push('\n');
                if let Some(sr) = &r.sysroot {
                    s.push_str("sysroot ");
                    s.push_str(sr);
                    s.push('\n');
                }
                for h in &r.headers {
                    s.push_str("header ");
                    s.push_str(h);
                    s.push('\n');
                }
                for i in &r.include_paths {
                    s.push_str("include ");
                    s.push_str(i);
                    s.push('\n');
                }
                for (k, v) in &r.defines {
                    s.push_str("define ");
                    s.push_str(k);
                    if !v.is_empty() {
                        s.push('=');
                        s.push_str(v);
                    }
                    s.push('\n');
                }
                for c in &r.cflags {
                    s.push_str("cflag ");
                    s.push_str(c);
                    s.push('\n');
                }
            }
            Request::ExpandMacro {
                key,
                name,
                arg_types,
            } => {
                s.push_str("expand-macro\n");
                s.push_str("key ");
                s.push_str(key);
                s.push('\n');
                s.push_str("macro ");
                s.push_str(name);
                s.push('\n');
                for a in arg_types {
                    s.push_str("argtype ");
                    s.push_str(a);
                    s.push('\n');
                }
            }
        }
        s.push_str("end\n");
        s
    }

    /// Parse a request from its wire form. Workers use this; so does
    /// the round-trip test that keeps the two sides honest.
    pub fn parse(text: &str) -> Result<Request, String> {
        let mut lines = text.lines();
        let hs = lines.next().ok_or("empty request")?;
        check_handshake(hs)?;
        let verb = lines.next().ok_or("request has no verb")?.trim();

        let mut req = ImportRequest::default();
        let mut key = String::new();
        let mut name = String::new();
        let mut arg_types = Vec::new();
        let mut ended = false;

        for line in lines {
            let line = line.trim_end_matches('\r');
            if line == "end" {
                ended = true;
                break;
            }
            let (k, v) = match line.split_once(' ') {
                Some((k, v)) => (k, v),
                None if line.is_empty() => continue,
                None => (line, ""),
            };
            match k {
                "target" => req.target = v.to_string(),
                "sysroot" => req.sysroot = Some(v.to_string()),
                "header" => req.headers.push(v.to_string()),
                "include" => req.include_paths.push(v.to_string()),
                "cflag" => req.cflags.push(v.to_string()),
                "define" => match v.split_once('=') {
                    Some((n, val)) => req.defines.push((n.to_string(), val.to_string())),
                    None => req.defines.push((v.to_string(), String::new())),
                },
                "key" => key = v.to_string(),
                "macro" => name = v.to_string(),
                "argtype" => arg_types.push(v.to_string()),
                other => return Err(format!("unknown request field `{other}`")),
            }
        }
        if !ended {
            return Err("request is not terminated by `end`".to_string());
        }
        match verb {
            "import" => {
                if req.headers.is_empty() {
                    return Err("import request names no headers".to_string());
                }
                if req.target.is_empty() {
                    return Err("import request names no target".to_string());
                }
                Ok(Request::Import(req))
            }
            "expand-macro" => {
                if name.is_empty() {
                    return Err("expand-macro request names no macro".to_string());
                }
                Ok(Request::ExpandMacro {
                    key,
                    name,
                    arg_types,
                })
            }
            other => Err(format!("unknown request verb `{other}`")),
        }
    }
}

/// A worker's answer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Response {
    /// A serialized artifact.
    Artifact(Vec<u8>),
    /// A macro expansion's tokens.
    Tokens(Vec<String>),
    /// The worker could not answer at all. This is **not** how a
    /// refused declaration is reported — those ride inside the
    /// artifact, so one bad declaration never costs a header.
    Err(String),
}

impl Response {
    /// Render the wire form. Binary payloads are length-prefixed, so
    /// nothing is escaped and nothing is ambiguous.
    pub fn render(&self) -> Vec<u8> {
        let mut out = handshake().into_bytes();
        out.push(b'\n');
        match self {
            Response::Artifact(bytes) => {
                out.extend_from_slice(format!("ok {}\n", bytes.len()).as_bytes());
                out.extend_from_slice(bytes);
            }
            Response::Tokens(toks) => {
                out.extend_from_slice(format!("ok-tokens {}\n", toks.len()).as_bytes());
                for t in toks {
                    out.extend_from_slice(t.as_bytes());
                    out.push(b'\n');
                }
            }
            Response::Err(msg) => {
                // A newline in an error message would desynchronize the
                // stream; the message is one line, always.
                let one = msg.replace(['\n', '\r'], " ");
                out.extend_from_slice(format!("err {one}\n").as_bytes());
            }
        }
        out
    }

    /// Parse a worker's answer.
    pub fn parse(bytes: &[u8]) -> Result<Response, String> {
        let (hs, rest) = split_line(bytes).ok_or("worker sent no handshake")?;
        check_handshake(&String::from_utf8_lossy(hs))?;
        let (head, rest) = split_line(rest).ok_or("worker sent no status line")?;
        let head = String::from_utf8_lossy(head).trim_end().to_string();

        if let Some(n) = head.strip_prefix("ok ") {
            let n: usize = n
                .trim()
                .parse()
                .map_err(|_| format!("worker sent a bad payload length `{n}`"))?;
            if rest.len() < n {
                return Err(format!(
                    "worker promised {n} bytes of artifact and sent {}",
                    rest.len()
                ));
            }
            return Ok(Response::Artifact(rest[..n].to_vec()));
        }
        if let Some(n) = head.strip_prefix("ok-tokens ") {
            let n: usize = n
                .trim()
                .parse()
                .map_err(|_| format!("worker sent a bad token count `{n}`"))?;
            let mut toks = Vec::with_capacity(n.min(4096));
            let mut cur = rest;
            for _ in 0..n {
                let (line, next) =
                    split_line(cur).ok_or("worker sent fewer tokens than promised")?;
                toks.push(String::from_utf8_lossy(line).trim_end().to_string());
                cur = next;
            }
            return Ok(Response::Tokens(toks));
        }
        if let Some(msg) = head.strip_prefix("err ") {
            return Ok(Response::Err(msg.to_string()));
        }
        if head == "err" {
            return Ok(Response::Err("(the worker gave no reason)".to_string()));
        }
        Err(format!("worker sent an unknown status line `{head}`"))
    }
}

fn check_handshake(line: &str) -> Result<(), String> {
    let line = line.trim();
    let Some(v) = line.strip_prefix("wolf-cimport ") else {
        return Err(format!(
            "expected the `wolf-cimport {PROTOCOL_VERSION}` handshake, got `{line}`"
        ));
    };
    let v: u32 = v
        .trim()
        .parse()
        .map_err(|_| format!("handshake version `{v}` is not a number"))?;
    if v != PROTOCOL_VERSION {
        return Err(format!(
            "the importer worker speaks protocol {v}; this compiler speaks {PROTOCOL_VERSION}"
        ));
    }
    Ok(())
}

/// Split off one `\n`-terminated line, returning it without the
/// newline plus the remainder. `None` at end of input.
fn split_line(b: &[u8]) -> Option<(&[u8], &[u8])> {
    let i = b.iter().position(|&c| c == b'\n')?;
    Some((&b[..i], &b[i + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn imp() -> ImportRequest {
        ImportRequest {
            headers: vec!["stdlib.h".into(), "string.h".into()],
            defines: vec![
                ("_GNU_SOURCE".into(), "1".into()),
                ("NDEBUG".into(), String::new()),
            ],
            cflags: vec!["-std=c17".into()],
            include_paths: vec!["/usr/include".into()],
            target: "x86_64-unknown-linux-gnu".into(),
            sysroot: Some("b3:9f3c".into()),
        }
    }

    #[test]
    fn import_requests_round_trip() {
        let r = Request::Import(imp());
        assert_eq!(Request::parse(&r.render()), Ok(r));
    }

    /// A bare `-Dname` and `-Dname=` are different things to the
    /// preprocessor; the wire form must keep them apart.
    #[test]
    fn a_bare_define_survives_the_wire() {
        let r = Request::Import(imp());
        let text = r.render();
        assert!(text.contains("define NDEBUG\n"));
        assert!(text.contains("define _GNU_SOURCE=1\n"));
        assert_eq!(Request::parse(&text), Ok(r));
    }

    #[test]
    fn expand_macro_requests_round_trip() {
        let r = Request::ExpandMacro {
            key: "b3:9f3c".into(),
            name: "FD_SET".into(),
            arg_types: vec!["int".into(), "fd_set *".into()],
        };
        assert_eq!(Request::parse(&r.render()), Ok(r));
    }

    #[test]
    fn artifact_responses_round_trip_with_arbitrary_bytes() {
        // Newlines and NULs in the payload must not confuse the framing
        // — that is what the length prefix is for.
        let payload: Vec<u8> = (0u8..=255).cycle().take(1000).collect();
        let r = Response::Artifact(payload);
        assert_eq!(Response::parse(&r.render()), Ok(r));
    }

    #[test]
    fn token_responses_round_trip() {
        let r = Response::Tokens(vec!["__FDS_BIT".into(), "(".into(), "d".into(), ")".into()]);
        assert_eq!(Response::parse(&r.render()), Ok(r));
    }

    #[test]
    fn error_responses_round_trip_and_stay_one_line() {
        let r = Response::Err("could not find <stdlib.h>\nin any include path".into());
        let parsed = Response::parse(&r.render()).expect("parses");
        assert_eq!(
            parsed,
            Response::Err("could not find <stdlib.h> in any include path".into())
        );
    }

    /// The handshake is the whole point of versioning a process
    /// contract: a worker from the future must be refused, loudly,
    /// rather than half-understood.
    #[test]
    fn a_protocol_mismatch_is_refused() {
        let bad = b"wolf-cimport 999\nok 0\n";
        let e = Response::parse(bad).expect_err("must refuse");
        assert!(e.contains("999"), "{e}");
        assert!(e.contains("speaks"), "{e}");

        let e = Request::parse("not a handshake\nimport\nend\n").expect_err("must refuse");
        assert!(e.contains("handshake"), "{e}");
    }

    #[test]
    fn a_truncated_payload_is_an_error() {
        let mut bytes = Response::Artifact(vec![1, 2, 3, 4]).render();
        bytes.truncate(bytes.len() - 2);
        let e = Response::parse(&bytes).expect_err("must refuse");
        assert!(e.contains("promised"), "{e}");
    }

    #[test]
    fn unterminated_requests_are_refused() {
        let e = Request::parse("wolf-cimport 1\nimport\nheader a.h\n").expect_err("must refuse");
        assert!(e.contains("end"), "{e}");
    }

    #[test]
    fn empty_imports_are_refused() {
        let e = Request::parse("wolf-cimport 1\nimport\ntarget t\nend\n").expect_err("must refuse");
        assert!(e.contains("no headers"), "{e}");
    }
}
