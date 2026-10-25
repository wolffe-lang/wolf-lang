//! wolf-lang#925 (s266): `test_cpus`, the one place a gate's pinned rows
//! get their CPUs. Its parsing and rendering are `taskset -c`'s list
//! syntax; the allocation is `WOLF_TEST_CPUS` or this process's own.

mod test_cpus;

use test_cpus::{parse_list, render, take};

#[test]
fn lists_parse_as_taskset_reads_them() {
    assert_eq!(parse_list("4-7").unwrap(), vec![4, 5, 6, 7]);
    assert_eq!(parse_list("6,8-10").unwrap(), vec![6, 8, 9, 10]);
    assert_eq!(parse_list("0-15:4").unwrap(), vec![0, 4, 8, 12]);
    assert_eq!(parse_list(" 3,1,3 ").unwrap(), vec![1, 3]);
    for bad in ["", "a", "7-4", "1-3:0", "1,,2", "-1", "4-", "9999"] {
        assert!(parse_list(bad).is_err(), "{bad:?} parsed");
    }
}

#[test]
fn a_set_renders_as_its_shortest_list() {
    assert_eq!(render(&[4, 5, 6, 7]), "4-7");
    assert_eq!(render(&[4, 6]), "4,6");
    assert_eq!(render(&[0, 1, 3, 8, 9, 10]), "0-1,3,8-10");
    assert_eq!(render(&[5]), "5");
    for list in ["4-7", "6,8-10", "0-1,3,8-10"] {
        assert_eq!(render(&parse_list(list).unwrap()), list);
    }
}

#[test]
fn a_request_takes_the_allocations_first_cpus_or_says_why_not() {
    let block = parse_list("6-11").unwrap();
    assert_eq!(take(&block, 1).unwrap(), "6");
    assert_eq!(take(&block, 4).unwrap(), "6-9");
    let gappy = parse_list("4,6,8,10").unwrap();
    assert_eq!(take(&gappy, 4).unwrap(), "4,6,8,10");
    let e = take(&parse_list("4-5").unwrap(), 4).unwrap_err();
    assert!(
        e.contains("4 CPU(s) asked of an allocation of 2 (4-5)"),
        "{e}"
    );
    assert!(take(&block, 0).is_err());
}

#[test]
fn the_allocation_is_one_this_process_may_use() {
    let alloc = test_cpus::allocation();
    assert!(
        !alloc.is_empty(),
        "an empty allocation from {}",
        test_cpus::source()
    );
    eprintln!(
        "s266: allocation {} from {}",
        render(&alloc),
        test_cpus::source()
    );
    if cfg!(target_os = "linux") {
        let pin = test_cpus::pin(1, "the_allocation_is_one_this_process_may_use");
        if let Some(pin) = pin {
            let out = pin
                .command("grep")
                .args(["Cpus_allowed_list", "/proc/self/status"])
                .output()
                .expect("taskset runs");
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(out.status.success(), "{}: {text}", pin.label());
            assert_eq!(
                text.split_whitespace().nth(1),
                Some(pin.list.as_str()),
                "{}: {text}",
                pin.label()
            );
        }
    }
}
