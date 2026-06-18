//! Append-only audit logging of destructive operations.
//!
//! When an audit log path is configured, every mutating tool call (write,
//! delete, rename, copy, merge, batch update, restore, …) is recorded as one
//! JSON line: timestamp, operation, outcome, and the sanitised file name. This
//! gives a durable, greppable record of irreversible-in-effect changes to a
//! user's libraries.
//!
//! Logging is best-effort: a failure to write the audit log is reported via
//! `tracing` but never fails the underlying operation. Paths are recorded as
//! file names only (already sanitised by the caller), never full paths.

use std::io::Write;
use std::path::PathBuf;

use serde::Serialize;

/// Outcome of an audited operation.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditOutcome {
    /// The operation completed successfully.
    Success,
    /// The operation returned an error result.
    Error,
    /// The operation was denied (e.g. rate limited or path rejected).
    Denied,
}

/// A single audit log entry.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    /// RFC 3339 timestamp of when the event was created.
    pub timestamp: String,
    /// The tool/operation name (e.g. `write_pcblib`, `delete_component`).
    pub operation: String,
    /// The outcome of the operation.
    pub outcome: AuditOutcome,
    /// The sanitised file name the operation targeted, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filepath: Option<String>,
    /// Optional additional detail.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl AuditEvent {
    /// Creates an event stamped with the current local time.
    #[must_use]
    pub fn new(
        operation: impl Into<String>,
        outcome: AuditOutcome,
        filepath: Option<String>,
    ) -> Self {
        Self {
            timestamp: chrono::Local::now().to_rfc3339(),
            operation: operation.into(),
            outcome,
            filepath,
            details: None,
        }
    }

    /// Attaches an additional detail string.
    #[must_use]
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

/// Append-only JSON-lines audit logger.
#[derive(Debug, Clone)]
pub struct AuditLogger {
    path: PathBuf,
}

impl AuditLogger {
    /// Creates a logger that appends to `path`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Appends one event as a single JSON line.
    ///
    /// Best-effort: serialisation or I/O failures are logged via `tracing` and
    /// swallowed so auditing can never break an operation.
    pub fn record(&self, event: &AuditEvent) {
        let line = match serde_json::to_string(event) {
            Ok(line) => line,
            Err(e) => {
                tracing::warn!(error = %e, "failed to serialise audit event");
                return;
            }
        };

        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            Ok(mut file) => {
                if let Err(e) = writeln!(file, "{line}") {
                    tracing::warn!(error = %e, "failed to append audit log entry");
                }
            }
            Err(e) => tracing::warn!(error = %e, "failed to open audit log"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_append_as_json_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("audit.log");
        let logger = AuditLogger::new(&path);

        logger.record(&AuditEvent::new(
            "write_pcblib",
            AuditOutcome::Success,
            Some("Lib.PcbLib".to_string()),
        ));
        logger.record(
            &AuditEvent::new("delete_component", AuditOutcome::Error, None)
                .with_details("component not found"),
        );

        let content = std::fs::read_to_string(&path).expect("read audit log");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["operation"], "write_pcblib");
        assert_eq!(first["outcome"], "success");
        assert_eq!(first["filepath"], "Lib.PcbLib");
        assert!(first.get("timestamp").is_some());

        let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(second["operation"], "delete_component");
        assert_eq!(second["outcome"], "error");
        assert_eq!(second["details"], "component not found");
        // filepath is omitted when None.
        assert!(second.get("filepath").is_none());
    }

    #[test]
    fn outcome_serialises_lowercase() {
        assert_eq!(
            serde_json::to_string(&AuditOutcome::Denied).unwrap(),
            "\"denied\""
        );
    }
}
