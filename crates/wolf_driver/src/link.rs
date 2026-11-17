//! Opt-in cc-shaped linker seam. The platform's default link remains in
//! main::link_staged; a selected tool is invoked once and never falls back.
use std::path::{Path, PathBuf};

pub fn selection(cli: Option<&Path>, env: Option<std::ffi::OsString>) -> Option<PathBuf> {
    cli.map(Path::to_path_buf)
        .or_else(|| env.filter(|value| !value.is_empty()).map(PathBuf::from))
}

/// ligature, the linker this seam was built for, cannot order input sections
/// yet (ligature#38). A selection whose executable is named `ligature` gets
/// the cc-shaped line below unchanged and objects in today's layout; any other
/// selected driver receives the stable-layout arguments.
pub fn is_ligature(path: &Path) -> bool {
    path.file_name() == Some(std::ffi::OsStr::new("ligature"))
}

pub fn supported() -> Result<(), String> {
    if cfg!(windows) {
        Err("selected linker: hosted Windows is unsupported by the cc-shaped ELF seam; use the existing WOLF_LINKER COFF path".into())
    } else if !cfg!(target_os = "linux") {
        Err("selected linker: only native Linux ELF targets are supported; the platform link path is unchanged".into())
    } else {
        Ok(())
    }
}

pub fn selected(
    linker: &Path,
    paths: &[PathBuf],
    out: &Path,
    rt: &Path,
    verbose: bool,
    trap_hook: bool,
    layout_args: &[std::ffi::OsString],
) -> Result<(), String> {
    let mut command = std::process::Command::new(linker);
    command.args(layout_args);
    command.arg("-o").arg(out).args(paths).arg(rt);
    command.args(["-lpthread", "-ldl", "-lm", "-Wl,--gc-sections"]);
    if trap_hook {
        command.args([
            "-Wl,--undefined=__wolf_rt_trap_at",
            "-Wl,--defsym=wolf_trap=__wolf_rt_trap_at",
        ]);
    }
    // A no-output success cannot validate a prior executable.
    match std::fs::remove_file(out) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("selected linker: remove {}: {e}", out.display())),
    }
    let status = crate::status::run_tool(&mut command)
        .map_err(|e| format!("selected linker: cannot run {command:?}: {e}"))?;
    if !status.success() {
        return Err(format!(
            "selected linker: {command:?} failed linking {} ({status})",
            out.display()
        ));
    }
    use object::{Object, ObjectSegment};
    let bytes =
        std::fs::read(out).map_err(|e| format!("selected linker: read {}: {e}", out.display()))?;
    let arch = if cfg!(target_arch = "x86_64") {
        object::Architecture::X86_64
    } else {
        object::Architecture::Aarch64
    };
    let valid = object::File::parse(&*bytes).is_ok_and(|file| {
        file.format() == object::BinaryFormat::Elf
            && file.is_64()
            && file.is_little_endian()
            && file.architecture() == arch
            && matches!(
                file.kind(),
                object::ObjectKind::Executable | object::ObjectKind::Dynamic
            )
            && file.entry() != 0
            && file.segments().any(|segment| {
                matches!(segment.flags(), object::SegmentFlags::Elf { p_flags } if p_flags & 1 != 0)
                    && file
                        .entry()
                        .checked_sub(segment.address())
                        .is_some_and(|offset| {
                            segment.data().is_ok_and(|data| offset < data.len() as u64)
                        })
            })
    });
    if !valid {
        return Err(format!(
            "selected linker: {command:?} did not produce an ELF64 little-endian {arch:?} executable"
        ));
    }
    if verbose {
        eprintln!("wolf build: linked {} with {command:?}", out.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ligature_is_recognized_by_its_executable_name_only() {
        assert!(is_ligature(Path::new("ligature")));
        assert!(is_ligature(Path::new("tools with spaces/ligature")));
        assert!(!is_ligature(Path::new("cc")));
        assert!(!is_ligature(Path::new("ligature-wrapper")));
        assert!(!is_ligature(Path::new("ligature/cc")));
    }

    #[test]
    fn linker_cli_over_environment_over_default() {
        assert_eq!(
            selection(Some(Path::new("cli tool")), Some("env tool".into())),
            Some("cli tool".into())
        );
        assert_eq!(
            selection(None, Some("env tool".into())),
            Some("env tool".into())
        );
        assert_eq!(selection(None, Some("".into())), None);
        assert_eq!(selection(None, None), None);
    }
}
