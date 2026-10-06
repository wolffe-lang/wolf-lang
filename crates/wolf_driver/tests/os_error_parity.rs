//! s200 (wolf-lang#407, `[os.fs.error]`): `os_error_text` is answered by
//! two copies of one dozen lines — the runtime's (`wolf_rt::fs`, native and
//! release) and the checked machine's (`wolf_mem::ubcheck`), because D15
//! keeps the runtime out of the compiler's graph. This holds them equal on
//! every code a host is likely to hand back, so the two tiers cannot part
//! on a reason's words.

/// `os_error_text` is one table on the two sides of D15's wall: the
/// runtime's (native, release) and the checked machine's mirror.
#[test]
fn os_error_text_is_one_table() {
    for code in -3..=200i64 {
        assert_eq!(
            wolf_rt::fs::os_error_text(code),
            wolf_mem::ubcheck::host_error_text(code),
            "os_error_text({code})"
        );
    }
    assert_eq!(wolf_rt::fs::os_error_text(0), "");
    assert!(!wolf_rt::fs::os_error_text(2).is_empty());
    #[cfg(unix)]
    assert_eq!(wolf_rt::fs::os_error_text(2), "No such file or directory");
}
