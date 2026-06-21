//! Small cross-cutting helpers with no dependency on server state.
//!
//! Lifting pure helpers here makes them directly unit-testable rather than
//! reachable only through the `McpServer` impl.

use std::sync::OnceLock;

use regex::Regex;

/// Redacts absolute filesystem paths in a client-facing string, replacing each
/// with its final component (basename).
///
/// This is a defence-in-depth choke-point: even if a future code path
/// interpolates a raw absolute path into an error message, the internal
/// directory structure is not disclosed to the client. It is intentionally
/// conservative to avoid false positives:
///
/// - **Windows** drive-absolute (`C:\…`, `C:/…`) and UNC (`\\server\…`) paths
///   are always redacted (relative paths never contain a drive letter).
/// - **Unix** absolute paths (`/a/b/…`, two or more components) are redacted
///   only at the start of the string or after whitespace, so relative paths
///   (`./Lib.PcbLib`), embedded segments (`a/b`), and URLs (`https://h/p`) are
///   left untouched.
#[must_use]
#[allow(clippy::missing_panics_doc)] // the regexes are constant literals; new() cannot fail
pub fn redact_absolute_paths(message: &str) -> String {
    fn basename(path: &str) -> String {
        path.rsplit(['/', '\\'])
            .find(|seg| !seg.is_empty())
            .unwrap_or("<path>")
            .to_string()
    }

    static WINDOWS: OnceLock<Regex> = OnceLock::new();
    static UNIX: OnceLock<Regex> = OnceLock::new();

    // Group 1 is a leading boundary (start, whitespace, quote, paren, `=`) so a
    // drive letter preceded by other letters — e.g. the `s:` in `https://` — is
    // not mistaken for `C:\`.
    let windows = WINDOWS
        .get_or_init(|| Regex::new(r#"(^|[\s"'(=])((?:[A-Za-z]:[\\/]|\\\\)[^\s"'<>|]*)"#).unwrap());
    let unix = UNIX.get_or_init(|| Regex::new(r"(^|\s)(/[^\s/]+(?:/[^\s/]+)+)").unwrap());

    let redact = |caps: &regex::Captures| format!("{}{}", &caps[1], basename(&caps[2]));
    let step1 = windows.replace_all(message, &redact);
    let step2 = unix.replace_all(&step1, &redact);
    step2.into_owned()
}

/// Escapes a field value for RFC 4180 compliant CSV output.
///
/// If the field contains commas, double quotes, or newlines, it is wrapped in
/// double quotes with any internal quotes doubled.
#[must_use]
pub fn escape_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        // Wrap in quotes, escaping any internal quotes by doubling them.
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Generates an 8-character uppercase A–Z identifier for Altium `UniqueID`
/// fields (library `FileHeader`, schematic records, etc.).
///
/// Altium only requires the id to be 8 letters; uniqueness across a session is
/// achieved by mixing the wall clock with a process-wide counter.
#[must_use]
pub fn generate_unique_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let time_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());

    // Combine time with an incrementing counter for uniqueness.
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let seed = time_seed.wrapping_add(u128::from(counter).wrapping_mul(0x9E37_79B9_7F4A_7C15));

    let chars: Vec<char> = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".chars().collect();
    let mut id = String::with_capacity(8);
    let mut n = seed;
    for _ in 0..8 {
        #[allow(clippy::cast_possible_truncation)]
        let idx = (n % 26) as usize;
        id.push(chars[idx]);
        n = n.wrapping_mul(1_103_515_245).wrapping_add(12345);
    }
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_unique_id_is_eight_uppercase_letters() {
        let id = generate_unique_id();
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_uppercase()));
        // Successive calls differ (counter advances).
        assert_ne!(generate_unique_id(), generate_unique_id());
    }

    #[test]
    fn plain_field_is_unchanged() {
        assert_eq!(escape_csv_field("RESC0402"), "RESC0402");
        assert_eq!(escape_csv_field(""), "");
    }

    #[test]
    fn field_with_comma_is_quoted() {
        assert_eq!(escape_csv_field("a,b"), "\"a,b\"");
    }

    #[test]
    fn field_with_quote_is_doubled_and_wrapped() {
        assert_eq!(escape_csv_field("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn field_with_newline_is_quoted() {
        assert_eq!(escape_csv_field("a\nb"), "\"a\nb\"");
        assert_eq!(escape_csv_field("a\r\nb"), "\"a\r\nb\"");
    }

    #[test]
    fn redact_windows_drive_path() {
        assert_eq!(
            redact_absolute_paths("Failed to write file: C:\\Users\\me\\proj\\Lib.pcblib.tmp"),
            "Failed to write file: Lib.pcblib.tmp"
        );
        assert_eq!(
            redact_absolute_paths("at C:/Users/me/Lib.PcbLib here"),
            "at Lib.PcbLib here"
        );
    }

    #[test]
    fn redact_unix_absolute_path() {
        assert_eq!(
            redact_absolute_paths("read /home/user/secret/Parts.SchLib failed"),
            "read Parts.SchLib failed"
        );
        // At the very start of the string.
        assert_eq!(redact_absolute_paths("/a/b/c.step"), "c.step");
    }

    #[test]
    fn redact_handles_multiple_and_mixed() {
        assert_eq!(
            redact_absolute_paths("at /a/b and C:\\x\\y.PcbLib"),
            "at b and y.PcbLib"
        );
    }

    #[test]
    fn redact_leaves_relative_paths_and_plain_text_untouched() {
        // Relative paths the client supplied must be preserved.
        assert_eq!(
            redact_absolute_paths("Component not found in './MyLib.PcbLib'"),
            "Component not found in './MyLib.PcbLib'"
        );
        assert_eq!(
            redact_absolute_paths("Missing required parameter: filepath"),
            "Missing required parameter: filepath"
        );
        // A single-segment root path is not a directory disclosure.
        assert_eq!(redact_absolute_paths("see /etc"), "see /etc");
    }

    #[test]
    fn redact_leaves_urls_untouched() {
        let msg = "See https://example.com/docs/path for details";
        assert_eq!(redact_absolute_paths(msg), msg);
    }
}
