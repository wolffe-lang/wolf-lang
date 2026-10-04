//! wolf-lang#583 (s210): units finish in parallel in ONE process — the
//! driver's `units.par_iter()` (wolf_driver main.rs, the codegen phase)
//! — so every `finish` must own its scratch directory.
//!
//! At 0.2.23 the release tier named the directory
//! `wolf-llvm-{pid}-{wall-clock nanos}` and made it with
//! `create_dir_all`. macOS's clock is microsecond-grained: two units
//! reaching `finish` in the same microsecond shared one directory, both
//! wrote `wolf.ll` and `wolf.o` there, and the first to finish deleted
//! it under the other — the ICE `read …/wolf.o: No such file or
//! directory` boreutils CI met (run 37229323186, job 111515456616). The
//! other interleaving builds one unit's object from the other's IR.
//!
//! This witness releases 16 units at a barrier, 64 rounds, and asks of
//! each object that it carries ITS unit's symbol: a shared directory
//! shows up as the ICE or as another unit's object. Linux's nanosecond
//! clock makes a collision rare there, so the trunk red is macOS CI's;
//! `scratch_dir`'s forced-collision test in the crate is the
//! deterministic half.

use std::sync::{Arc, Barrier};

use wolf_backend::{Backend, Linkage};
use wolf_codegen_llvm::LlvmBackend;

const UNITS: usize = 16;
const ROUNDS: usize = 64;

fn carries(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn parallel_finishes_never_share_a_scratch_dir() {
    if let Err(e) = LlvmBackend::new() {
        eprintln!("SKIP: {e}");
        return;
    }
    for round in 0..ROUNDS {
        let barrier = Arc::new(Barrier::new(UNITS));
        let units: Vec<_> = (0..UNITS)
            .map(|u| {
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let mut b = Box::new(LlvmBackend::new().expect("clang probed above"));
                    let name = format!("wolf_s210_r{round:02}_u{u:02}");
                    b.define_data(&name, name.as_bytes(), Linkage::Export)
                        .expect("define the unit's datum");
                    barrier.wait();
                    (name, b.finish())
                })
            })
            .collect();
        for unit in units {
            let (name, got) = unit.join().expect("the unit's thread");
            let obj = got.unwrap_or_else(|e| panic!("round {round}, {name}: {e}"));
            assert!(
                carries(&obj.bytes, name.as_bytes()),
                "round {round}: {name}'s object was built from another unit's IR"
            );
        }
    }
}
