//! Altium Library Package (`.LibPkg`) project-file generation.
//!
//! A `.LibPkg` is an INI-style project file that groups source library
//! documents (`.SchLib`, `.PcbLib`) so Altium can compile them into an
//! Integrated Library (`.IntLib`). Altium backfills every optional project
//! setting the first time the project is opened, so only the `[Design]`
//! version and one `[Document{N}]` section per member document are required.
//!
//! ```text
//! [Design]
//! Version=1.0
//!
//! [Document1]
//! DocumentPath=MyLib.SchLib
//!
//! [Document2]
//! DocumentPath=MyLib.PcbLib
//! ```
//!
//! Member documents are referenced by their path **relative to the `.LibPkg`
//! file**. This module only generates the project source; compiling it to a
//! binary `.IntLib` is an operation performed inside Altium Designer.

use std::path::Path;

/// Computes the path of `document` relative to the directory containing
/// `libpkg_path`, using Windows-style `\` separators (Altium's convention).
///
/// A document in the same directory becomes a bare file name; a document with
/// no shared root (a bare name, or a different drive) is emitted unchanged.
#[must_use]
pub fn relative_to_libpkg(libpkg_path: &Path, document: &str) -> String {
    let base = libpkg_path.parent().unwrap_or_else(|| Path::new(""));
    relative_path(&base.to_string_lossy(), document)
}

/// Builds the full text of a `.LibPkg` referencing `documents`.
#[must_use]
pub fn build_libpkg(libpkg_path: &Path, documents: &[String]) -> String {
    use std::fmt::Write as _;
    let mut out = String::from("[Design]\r\nVersion=1.0\r\n");
    for (i, doc) in documents.iter().enumerate() {
        let rel = relative_to_libpkg(libpkg_path, doc);
        let _ = write!(out, "\r\n[Document{}]\r\nDocumentPath={rel}\r\n", i + 1);
    }
    out
}

/// Lexically relativizes `target` against the directory `base`.
///
/// Both are treated as **Windows-style** paths (Altium's convention) regardless
/// of the host OS: we split on `\` or `/` ourselves rather than via
/// [`std::path::Path::components`], which only treats `\` as a separator on
/// Windows and would mis-parse `C:\…` paths on Linux/macOS. Segment comparison
/// is case-insensitive, matching Windows path semantics.
fn relative_path(base: &str, target: &str) -> String {
    let split = |p: &str| -> Vec<String> {
        p.split(['\\', '/'])
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect()
    };
    let b = split(base);
    let t = split(target);

    let mut shared = 0;
    while shared < b.len() && shared < t.len() && b[shared].eq_ignore_ascii_case(&t[shared]) {
        shared += 1;
    }

    // No shared root — a bare file name or a different drive. Emit as given
    // (normalised to backslashes) rather than inventing a bogus `..` chain.
    if shared == 0 {
        return target.replace('/', "\\");
    }

    let mut parts: Vec<String> = vec!["..".to_string(); b.len() - shared];
    parts.extend(t[shared..].iter().cloned());
    parts.join("\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_directory_uses_bare_filename() {
        let pkg = Path::new(r"C:\lib\TestLib.LibPkg");
        assert_eq!(
            relative_to_libpkg(pkg, r"C:\lib\TestLib.SchLib"),
            "TestLib.SchLib"
        );
    }

    #[test]
    fn subdirectory_is_relative() {
        let pkg = Path::new(r"C:\lib\TestLib.LibPkg");
        assert_eq!(
            relative_to_libpkg(pkg, r"C:\lib\sub\Part.PcbLib"),
            "sub\\Part.PcbLib"
        );
    }

    #[test]
    fn parent_directory_walks_up() {
        let pkg = Path::new(r"C:\lib\pkg\TestLib.LibPkg");
        assert_eq!(
            relative_to_libpkg(pkg, r"C:\lib\Part.SchLib"),
            "..\\Part.SchLib"
        );
    }

    #[test]
    fn bare_filename_passes_through() {
        let pkg = Path::new(r"C:\lib\TestLib.LibPkg");
        assert_eq!(relative_to_libpkg(pkg, "TestLib.SchLib"), "TestLib.SchLib");
    }

    #[test]
    fn build_has_design_and_documents() {
        let pkg = Path::new(r"C:\lib\TestLib.LibPkg");
        let docs = vec![
            r"C:\lib\TestLib.SchLib".to_string(),
            r"C:\lib\TestLib.PcbLib".to_string(),
        ];
        let text = build_libpkg(pkg, &docs);
        assert!(text.starts_with("[Design]\r\nVersion=1.0"));
        assert!(text.contains("[Document1]\r\nDocumentPath=TestLib.SchLib"));
        assert!(text.contains("[Document2]\r\nDocumentPath=TestLib.PcbLib"));
    }
}
