//! The observation record's file index (`[proto.record.diag]`, s181,
//! wolf-lang#437).
//!
//! A span is a byte range in ONE file. When any diagnostic's span lies
//! outside the entry file, the record carries a top-level `files` array
//! and every diagnostic an integer `file` indexing it; otherwise both
//! keys are absent and the record is byte-identical to one written
//! before the clause. `files` holds package-relative paths (relative to
//! the entry file's directory, `/`-separated, no leading `./`): the
//! entry at index 0, then each other file a diagnostic lies in, once,
//! in order of first appearance. A file under the std root is `std/`
//! followed by its path there.
//!
//! The paths come from the package root and never from the cwd, and
//! the key set from where the spans ARE and never from how the package
//! was staged — which is what lets lupin, staging the same package its
//! own way, emit the identical array.

use std::path::{Component, Path, PathBuf};

/// The index the record will carry: `files`, and one `file` per
/// diagnostic in record order. `None` when every span is in the entry.
#[derive(Debug, PartialEq, Eq)]
pub struct FileIndex {
    pub files: Vec<String>,
    pub per_diag: Vec<u64>,
}

/// `loaded` is the run's index→path table (SourceMap intern order, as
/// the loader spelled each path); `diag_files` is each diagnostic's
/// span file, as an index into it.
pub fn file_index(
    entry: &Path,
    loaded: &[String],
    diag_files: &[usize],
    std_root: Option<&Path>,
) -> Option<FileIndex> {
    let root = entry.parent().unwrap_or(Path::new("."));
    let entry_key = key(entry);
    let is_entry = |i: usize| loaded.get(i).is_none_or(|p| key(Path::new(p)) == entry_key);
    if diag_files.iter().all(|&i| is_entry(i)) {
        return None;
    }
    let entry_name = relative(entry, root, std_root);
    let mut files = vec![entry_name];
    let mut seen: Vec<(usize, u64)> = Vec::new();
    let mut per_diag = Vec::with_capacity(diag_files.len());
    for &i in diag_files {
        if is_entry(i) {
            per_diag.push(0);
            continue;
        }
        let idx = match seen.iter().find(|(j, _)| *j == i) {
            Some(&(_, idx)) => idx,
            None => {
                files.push(relative(Path::new(&loaded[i]), root, std_root));
                let idx = (files.len() - 1) as u64;
                seen.push((i, idx));
                idx
            }
        };
        per_diag.push(idx);
    }
    Some(FileIndex { files, per_diag })
}

/// A comparison key for "is this the same file": canonical when the
/// file exists, otherwise absolute and lexically normalized — always
/// absolute, so a canonical root and a lexical file still compare.
fn key(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| {
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default().join(p)
        };
        normalize(&abs)
    })
}

/// Lexical normalization: drops `.` components, folds `x/..`.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn slashed(p: &Path) -> String {
    p.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            Component::ParentDir => Some("..".to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// The record's spelling of one file: package-relative, or `std/…`
/// under the std root, or — for a file under neither, which no corpus
/// program produces today — the loader's own spelling, `/`-separated.
fn relative(p: &Path, root: &Path, std_root: Option<&Path>) -> String {
    let file = key(p);
    if let Ok(rel) = file.strip_prefix(key(root)) {
        return slashed(rel);
    }
    if let Some(std) = std_root
        && let Ok(rel) = file.strip_prefix(key(std))
    {
        return format!("std/{}", slashed(rel));
    }
    p.display().to_string().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn every_span_in_the_entry_means_no_index() {
        let loaded = s(&["./main.lu", "./geometry/shapes.lu"]);
        assert_eq!(
            file_index(Path::new("./main.lu"), &loaded, &[0, 0], None),
            None
        );
        assert_eq!(file_index(Path::new("./main.lu"), &loaded, &[], None), None);
    }

    #[test]
    fn a_sibling_span_indexes_package_relative_paths() {
        let loaded = s(&["./main.lu", "./geometry/shapes.lu", "./other/o.lu"]);
        let got = file_index(Path::new("./main.lu"), &loaded, &[2, 0, 1, 2], None).unwrap();
        assert_eq!(
            got.files,
            s(&["main.lu", "other/o.lu", "geometry/shapes.lu"])
        );
        assert_eq!(got.per_diag, vec![1, 0, 2, 1]);
    }

    #[test]
    fn the_cwd_prefix_is_not_part_of_the_path() {
        let loaded = s(&["pkg/main.lu", "pkg/geometry/shapes.lu"]);
        let got = file_index(Path::new("pkg/main.lu"), &loaded, &[1], None).unwrap();
        assert_eq!(got.files, s(&["main.lu", "geometry/shapes.lu"]));
        assert_eq!(got.per_diag, vec![1]);
    }

    #[test]
    fn the_entry_is_recognized_in_any_spelling() {
        let loaded = s(&["pkg/./main.lu", "pkg/geometry/../geometry/shapes.lu"]);
        let got = file_index(Path::new("pkg/main.lu"), &loaded, &[0, 1], None).unwrap();
        assert_eq!(got.files, s(&["main.lu", "geometry/shapes.lu"]));
        assert_eq!(got.per_diag, vec![0, 1]);
    }

    #[test]
    fn a_std_file_is_named_under_std() {
        let loaded = s(&["/w/pkg/main.lu", "/opt/std/fmt/float.lu"]);
        let got = file_index(
            Path::new("/w/pkg/main.lu"),
            &loaded,
            &[1],
            Some(Path::new("/opt/std")),
        )
        .unwrap();
        assert_eq!(got.files, s(&["main.lu", "std/fmt/float.lu"]));
    }
}
