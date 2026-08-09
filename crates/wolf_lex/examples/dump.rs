//! Debug helper: `cargo run -p wolf_lex --example dump -- <file>` prints
//! the token+trivia stream for a file (or stdin with `-`).

use std::io::Read;

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "-".into());
    let bytes = if arg == "-" {
        let mut b = Vec::new();
        std::io::stdin().read_to_end(&mut b).expect("read stdin");
        b
    } else {
        std::fs::read(&arg).expect("read file")
    };
    let mut sm = wolf_span::SourceMap::new();
    let file = sm.intern(std::path::Path::new(&arg));
    let lexed = wolf_lex::lex(file, &bytes);
    print!("{}", lexed.dump(&bytes));
    assert_eq!(lexed.reassemble(&bytes), bytes, "LOSSLESS VIOLATION");
}
