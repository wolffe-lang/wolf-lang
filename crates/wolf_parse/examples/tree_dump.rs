//! Debug helper: parse a file and print the full lossless tree
//! (every node and token with spans) plus diagnostics — the deep
//! sibling of `parse_dump`'s declaration-level view.
fn main() {
    let path = std::env::args().nth(1).expect("usage: tree_dump <file>");
    let bytes = std::fs::read(&path).expect("read");
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(std::path::Path::new(&path));
    let parse = wolf_parse::parse_file(file, &bytes);
    println!("{}", parse.root.dump(&bytes));
    for d in &parse.diagnostics {
        println!("{} {}..{} {}", d.code, d.span.lo, d.span.hi, d.message);
    }
}
