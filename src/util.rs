//! Small cross-cutting helpers with no dependency on server state.
//!
//! Lifting pure helpers here makes them directly unit-testable rather than
//! reachable only through the `McpServer` impl.

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
