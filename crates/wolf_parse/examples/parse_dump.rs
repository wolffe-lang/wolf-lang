//! Debug helper: parse a file and print the decl dump + diagnostics.
fn main() {
    let path = std::env::args().nth(1).expect("usage: dump <file>");
    let bytes = std::fs::read(&path).expect("read");
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(std::path::Path::new(&path));
    let parse = wolf_parse::parse_file(file, &bytes);
    match wolf_ast::verify(&parse.root, &bytes) {
        Ok(()) => eprintln!("verify: ok"),
        Err(e) => eprintln!("verify: FAIL {e}"),
    }
    println!("{}", wolf_ast::dump_decls(&parse.root, &bytes));
    for d in &parse.diagnostics {
        println!(
            "{} [{:?}] {}..{} {}",
            d.code,
            d.severity,
            d.span().lo,
            d.span().hi,
            d.message
        );
    }
}
