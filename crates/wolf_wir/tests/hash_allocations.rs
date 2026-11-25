//! The native cache-key hash must not copy and grow its whole input for padding.
//! Count this thread's allocation requests around the real shared SHA routine.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use wolf_wir::sha256_hex;

#[derive(Clone, Copy, Default)]
struct Counts {
    calls: usize,
    requested: usize,
    largest: usize,
}

thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static COUNTS: Cell<Counts> = const { Cell::new(Counts {
        calls: 0, requested: 0, largest: 0,
    }) };
}

fn request(size: usize) {
    // Const thread-local Cells need no heap allocation. Ignore calls during TLS
    // destruction, and do not count allocations from the test harness's threads.
    let _ = ENABLED.try_with(|enabled| {
        if enabled.get() {
            let _ = COUNTS.try_with(|counts| {
                let mut value = counts.get();
                value.calls = value.calls.saturating_add(1);
                value.requested = value.requested.saturating_add(size);
                value.largest = value.largest.max(size);
                counts.set(value);
            });
        }
    });
}

struct Counted;

// SAFETY: every operation forwards its original pointer/layout to System; the
// counters neither allocate nor alter the allocator's result or ownership.
unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        request(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        request(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        request(size);
        unsafe { System.realloc(ptr, layout, size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counted = Counted;

struct Measurement;

impl Measurement {
    fn start() -> Self {
        ENABLED.with(|enabled| assert!(!enabled.get(), "no nested measurement"));
        COUNTS.with(|counts| counts.set(Counts::default()));
        ENABLED.with(|enabled| enabled.set(true));
        Self
    }

    fn finish(self) -> Counts {
        let counts = COUNTS.with(Cell::get);
        drop(self);
        counts
    }
}

impl Drop for Measurement {
    fn drop(&mut self) {
        ENABLED.with(|enabled| enabled.set(false));
    }
}

#[test]
fn sha256_padding_boundaries_preserve_exact_bytes() {
    // Python hashlib over bytes(i % 256 for i in range(length)). Include both
    // sides of the 56-byte padding boundary and exact/full block boundaries.
    for (length, expected) in [
        (
            0,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            1,
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
        ),
        (
            55,
            "463eb28e72f82e0a96c0a4cc53690c571281131f672aa229e0d45ae59b598b59",
        ),
        (
            56,
            "da2ae4d6b36748f2a318f23e7ab1dfdf45acdc9d049bd80e59de82a60895f562",
        ),
        (
            57,
            "2fe741af801cc238602ac0ec6a7b0c3a8a87c7fc7d7f02a3fe03d1c12eac4d8f",
        ),
        (
            63,
            "29af2686fd53374a36b0846694cc342177e428d1647515f078784d69cdb9e488",
        ),
        (
            64,
            "fdeab9acf3710362bd2658cdc9a29e8f9c757fcf9811603a8c447cd1d9151108",
        ),
        (
            65,
            "4bfd2c8b6f1eec7a2afeb48b934ee4b2694182027e6d0fc075074f2fabb31781",
        ),
        (
            119,
            "da18797ed7c3a777f0847f429724a2d8cd5138e6ed2895c3fa1a6d39d18f7ec6",
        ),
        (
            120,
            "f52b23db1fbb6ded89ef42a23ce0c8922c45f25c50b568a93bf1c075420bbb7c",
        ),
        (
            127,
            "92ca0fa6651ee2f97b884b7246a562fa71250fedefe5ebf270d31c546bfea976",
        ),
        (
            128,
            "471fb943aa23c511f6f72f8d1652d9c880cfa392ad80503120547703e56a2be5",
        ),
        (
            129,
            "5099c6a56203f9687f7d33f4bfdf576d31dc91f6b695ecea38b2770c87631135",
        ),
        (
            1024,
            "785b0751fc2c53dc14a4ce3d800e69ef9ce1009eb327ccf458afe09c242c26c9",
        ),
    ] {
        let data: Vec<u8> = (0..length).map(|i| (i % 256) as u8).collect();
        assert_eq!(sha256_hex(&data), expected, "input length {length}");
    }
}

#[test]
fn sha256_scratch_allocation_is_bounded() {
    // A small input reaches the exact to_vec -> padding push growth path that
    // failed in native cache-key construction. No large-memory fixture or RSS
    // sampling is needed. Input construction and reporting are outside the scope.
    let data: Vec<u8> = (0..65_536).map(|i| (i % 256) as u8).collect();
    let measurement = Measurement::start();
    let digest = sha256_hex(std::hint::black_box(&data));
    let counts = measurement.finish();
    // Independently generated with Python hashlib; padding must not change the
    // existing digest or the driver's exact cache key.
    assert_eq!(
        digest,
        "7daca2095d0438260fa849183dfc67faa459fdf4936e1bc91eec6b281b27e4c2"
    );
    assert!(
        counts.calls > 0 && counts.requested >= 64,
        "counter is active"
    );
    if counts.largest > 1024 || counts.requested > 4096 || counts.calls > 64 {
        eprintln!("FAIL native-key-sha256-allocation");
        panic!(
            "native-key-sha256-allocation: calls={} requested_bytes={} largest_request={} (budgets 64/4096/1024)",
            counts.calls, counts.requested, counts.largest
        );
    }
    println!(
        "PASS native-key-sha256-allocation calls={} requested_bytes={} largest_request={}",
        counts.calls, counts.requested, counts.largest
    );
}
