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

    // Group 1 is a leading boundary (start, whitespace, quote, paren, `=`, `:`) so a
    // drive letter preceded by other letters — e.g. the `s:` in `https://` — is not
    // mistaken for `C:\`, while a path glued straight after a colon (`detail:C:\…`)
    // is still redacted. The false-positive guard holds: in `https://` the drive
    // candidate `s:/` is preceded by `p` (a letter, not in the class), so it never
    // matches.
    // The path body allows spaces, because `C:\Program Files\…`,
    // `C:\Users\First Last\…` and `OneDrive - Company\…` are ordinary Windows
    // paths. It terminates instead on characters that cannot appear in a Windows
    // path (`" < > | ? *`) or that end a path in prose: a *second* colon (the
    // drive's is already consumed by the prefix, so a later one means
    // `…\x.PcbLib: permission denied`), a comma, a semicolon, brackets, an
    // apostrophe, or a newline.
    //
    // The bias is deliberate. Over-matching a trailing word is harmless —
    // `basename` keeps it inside the final segment, so "Failed to read
    // x.PcbLib now" still reads correctly — whereas under-matching discloses
    // directories.
    //
    // The prefix alternation spells out all four shapes, longest first. The
    // verbatim forms are needed because `std::fs::canonicalize` returns them on
    // Windows; `?` stays excluded from the body, where it cannot occur in a real
    // file name, so the prefix absorbs it instead.
    //
    // Separators are matched as `\{1,2}` because this runs over an
    // already-serialised JSON body (see `ToolCallResult::error`), where every
    // Windows separator arrives escaped as `\\`.
    let windows = WINDOWS.get_or_init(|| {
        Regex::new(
            r#"(^|[\s"'(=:])((?:\\{2,4}[?.]\\{1,2}UNC\\{1,2}|\\{2,4}[?.]\\{1,2}[A-Za-z]:[\\/]{1,2}|\\{2,4}|[A-Za-z]:[\\/]{1,2})[^"'<>|?*:,;()\r\n]*)"#,
        )
        .unwrap()
    });
    // Unix paths take the same treatment, for `/home/me/My Libraries/x.PcbLib`.
    // The leading segment must still be space-free so ordinary prose starting
    // with a slash-word is not swallowed, and at least one separator is required.
    //
    // The boundary class matches the Windows one rather than plain `\s`: every
    // tool response is JSON, so a path is normally reached as `"filepath":
    // "/home/…"`, preceded by a quote and never by whitespace. URLs stay safe
    // regardless — in `https://h/p` the first `/` is followed by another `/`,
    // which the segment body rejects, and the second `/` is preceded by `/`,
    // which is not a boundary character.
    let unix = UNIX.get_or_init(|| {
        Regex::new(r#"(^|[\s"'(=:])(/[^\s/"'<>|:,;()\r\n]+(?:/[^/"'<>|:,;()\r\n]+)+)"#).unwrap()
    let mut has_non_underscore = false;
    });

    let redact = |caps: &regex::Captures| {
        // Trim trailing whitespace the greedy body may have taken with it, so a
        // path at the end of a sentence does not keep a dangling space.
        format!("{}{}", &caps[1], basename(caps[2].trim_end()))
                if c != '_' {
                    has_non_underscore = true;
                }
    };
    let step1 = windows.replace_all(message, &redact);
    let step2 = unix.replace_all(&step1, &redact);
    step2.into_owned()
}
    if cleaned.is_empty() || !has_non_underscore {
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

/// Characters Windows forbids in file names (also the set `write_pcblib` /
/// `write_schlib` reject in component names). Shared so every producer of an
/// on-disk name applies the same rule.
    const ALPHABET: &[u8; 26] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub const FILE_NAME_INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Sanitises a single file-name component derived from untrusted data (e.g. a
/// model name read out of a library) so it is safe to join onto a directory.
///
/// Replaces [`FILE_NAME_INVALID_CHARS`] and ASCII control characters with `_`
/// — notably `:`, which on NTFS would otherwise write an alternate data
/// stream (`foo:bar.step`) — and trims trailing dots/spaces (invalid in
/// Windows names). Returns `None` when nothing usable remains.
pub fn sanitise_file_name(name: &str) -> Option<String> {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if FILE_NAME_INVALID_CHARS.contains(&c) || c.is_control() {
        id.push(ALPHABET[idx] as char);
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim_end_matches(['.', ' ']);
    // The all-underscores check must run on the *trimmed* string: an input like
    // "<>." maps to "__." and trims to "__", which is unusable even though the
    // dot was not an underscore. Tracking "saw a non-underscore" during the map
    // pass would wrongly accept it.
    if cleaned.bytes().all(|b| b == b'_') {
        None
    } else {
        Some(cleaned.to_string())
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
    const ALPHABET: &[u8; 26] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";

    let time_seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());

    // Combine time with an incrementing counter for uniqueness.
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let seed = time_seed.wrapping_add(u128::from(counter).wrapping_mul(0x9E37_79B9_7F4A_7C15));

    let mut id = String::with_capacity(8);
    let mut n = seed;
    for _ in 0..8 {
        #[allow(clippy::cast_possible_truncation)]
        let idx = (n % 26) as usize;
        id.push(ALPHABET[idx] as char);
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
    fn sanitise_file_name_replaces_windows_invalid_chars() {
        // The NTFS alternate-data-stream vector: `foo:bar.step` must not keep
        // the colon (writing it would create a hidden stream on `foo`).
        assert_eq!(
            sanitise_file_name("foo:bar.step").as_deref(),
            Some("foo_bar.step")
        );
        assert_eq!(
            sanitise_file_name("a<b>c\"d/e\\f|g?h*i.step").as_deref(),
            Some("a_b_c_d_e_f_g_h_i.step")
        );
        // Control characters are replaced too; trailing dots/spaces trimmed.
        assert_eq!(
            sanitise_file_name("mo\u{7}del.step. ").as_deref(),
            Some("mo_del.step")
        );
        // A clean name passes through untouched.
        assert_eq!(
            sanitise_file_name("RESC1005X04L.step").as_deref(),
            Some("RESC1005X04L.step")
        );
    }

    #[test]
    fn sanitise_file_name_rejects_unusable_names() {
        assert_eq!(sanitise_file_name(""), None);
        assert_eq!(sanitise_file_name("..."), None);
        assert_eq!(sanitise_file_name("   "), None);
        assert_eq!(sanitise_file_name("::"), None, "nothing but replacements");
        // The trailing dot/space must not rescue an all-underscores name: the
        // usability check runs on the trimmed string, so "<>." (mapped to
        // "__.", trimmed to "__") is unusable even though '.' != '_'.
        assert_eq!(sanitise_file_name("<>."), None, "trimmed to underscores");
        assert_eq!(sanitise_file_name("__ ."), None, "trimmed to underscores");
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
        // A drive path glued straight after a colon (no space) must still redact.
        assert_eq!(
            redact_absolute_paths("detail:C:\\Users\\me\\secret\\Lib.PcbLib"),
            "detail:Lib.PcbLib"
        );
        // The `https://` guard still holds — no drive-letter false positive.
        assert_eq!(
            redact_absolute_paths("see https://example.com/x"),
            "see https://example.com/x"
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
    fn redact_paths_containing_spaces() {
        // Regression for #306. Spaces are ordinary in Windows paths, so a
        // path body that stops at the first space leaves the rest of the
        // directory tree — and potentially the account name — in the message.
        // This is the common case, not an edge one.
        assert_eq!(
            redact_absolute_paths(
                "Failed to read C:\\Users\\me\\Documents\\embedded society\\proj\\Corrupt.PcbLib"
            ),
            "Failed to read Corrupt.PcbLib"
        );
        assert_eq!(
            redact_absolute_paths("Failed to read C:\\Program Files\\Altium\\Lib.PcbLib"),
            "Failed to read Lib.PcbLib"
        );
        // UNC share with spaces.
        assert_eq!(
            redact_absolute_paths("at \\\\file server\\Team Libs\\Parts.SchLib"),
            "at Parts.SchLib"
        );
        // Unix paths with spaces leaked the same way.
        assert_eq!(
            redact_absolute_paths("at /home/me/My Libraries/Parts.PcbLib"),
            "at Parts.PcbLib"
        );
    }

    #[test]
    fn redact_windows_verbatim_prefixed_paths() {
        // `std::fs::canonicalize` returns `\\?\C:\…` on Windows, so this is the
        // shape the server handles after resolving a path, not an exotic input.
        // The `?` must be absorbed by the prefix, since the body excludes it.
        assert_eq!(
            redact_absolute_paths("Failed to read \\\\?\\C:\\Users\\me\\proj\\Corrupt.PcbLib"),
            "Failed to read Corrupt.PcbLib"
        );
        // Device namespace and verbatim UNC take the same route.
        assert_eq!(
            redact_absolute_paths("at \\\\?\\UNC\\server\\Team Libs\\Parts.SchLib"),
            "at Parts.SchLib"
        );
        assert_eq!(
            redact_absolute_paths("at \\\\.\\C:\\libs\\X.PcbLib"),
            "at X.PcbLib"
        );
    }

    #[test]
    fn redact_json_escaped_windows_paths() {
        // The real input shape: `ToolCallResult::error` redacts an already
        // serialised JSON body, so every Windows separator arrives doubled.
        // A pattern matching only single separators left the path essentially
        // intact, which is how a full path reached clients on Windows.
        let json =
            r#"{"status": "error", "filepath": "\\\\?\\C:\\Users\\me\\proj\\Corrupt.PcbLib"}"#;
        assert_eq!(
            redact_absolute_paths(json),
            r#"{"status": "error", "filepath": "Corrupt.PcbLib"}"#
        );
        let plain_drive = r#"{"filepath": "C:\\Users\\me\\embedded society\\X.PcbLib"}"#;
        assert_eq!(
            redact_absolute_paths(plain_drive),
            r#"{"filepath": "X.PcbLib"}"#
        );
    }

    #[test]
    fn redact_quoted_paths_as_they_appear_in_json_responses() {
        // Every tool response is JSON, so this is how a path is actually reached
        // in practice — preceded by a quote, never by whitespace, so a
        // whitespace-only boundary would let the entire absolute path through
        // on Linux and macOS.
        assert_eq!(
            redact_absolute_paths(r#"{"filepath": "/home/me/work/proj/.tmp/Corrupt.PcbLib"}"#),
            r#"{"filepath": "Corrupt.PcbLib"}"#
        );
        assert_eq!(
            redact_absolute_paths("Failed to read '/home/me/libs/X.PcbLib'"),
            "Failed to read 'X.PcbLib'"
        );
        assert_eq!(
            redact_absolute_paths("(/home/me/libs/X.PcbLib)"),
            "(X.PcbLib)"
        );
    }

    #[test]
    fn redact_stops_at_trailing_prose_after_a_path() {
        // A later colon ends the path: the drive's own colon is consumed by the
        // prefix, so the next one is punctuation rather than part of the name.
        assert_eq!(
            redact_absolute_paths("Failed to read C:\\a b\\x.PcbLib: permission denied"),
            "Failed to read x.PcbLib: permission denied"
        );
        // A comma likewise, so a path inside a list does not swallow the rest.
        assert_eq!(
            redact_absolute_paths("tried C:\\a b\\x.PcbLib, then gave up"),
            "tried x.PcbLib, then gave up"
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
