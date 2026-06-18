# Security Design: altium-designer-mcp

This document is the **technical threat model and security-design reference** for the
MCP server. It describes what the tool defends against, what it explicitly does not,
and where each control lives in the source.

For the **vulnerability-reporting policy** (how to report a security issue, supported
versions, response timelines), see the root [SECURITY.md](../SECURITY.md).

---

## Trust Model

The MCP server is a **local binary-file-I/O tool for EDA component libraries**, not a
network service. It speaks JSON-RPC over a stdio transport to a single, co-located MCP
client (typically an AI assistant). There is no listening socket, no authentication
boundary, and no multi-tenant separation.

Given that shape, the realistic adversary is **untrusted input**, not an untrusted
caller:

- The **client is trusted** to the extent the operating-system user trusts it. The
    server runs with the user's own privileges and can touch whatever the user can.
- The **library files are untrusted input**. `.PcbLib` and `.SchLib` files are binary
    OLE compound documents that may be malformed, truncated, corrupted, or hostile.
- The **`filepath` arguments are untrusted input**. They arrive from the client and
    must be confined to configured directories before any read or write.

The security posture therefore centres on two things: **confining file access** to
configured directories and **parsing untrusted bytes without panicking or leaking
internal paths**.

---

## Threat Model

### Protected against

The following are within scope and have concrete controls behind them.

| Threat | Control | Source |
|--------|---------|--------|
| Path escape outside `allowed_paths` (`..` traversal, symlink, absolute path) | Canonicalise the target (or its parent for new files) and require it to be a prefix of a configured allowed path | `validate_path`, `src/mcp/server.rs` |
| Arbitrary overwrite of unrelated files | Same allow-list check gates every file-touching tool; writes are atomic and a timestamped backup is taken first | `validate_path` `src/mcp/server.rs`; `create_backup` `src/mcp/server.rs` |
| Internal-path disclosure in error messages | Denial message is generic; file errors render only the final path component | generic denial `src/mcp/server.rs`; `sanitise_path_for_client` `src/altium/error.rs` |
| Runaway-write / backup-thrash DoS (e.g. an AI loop) | Token-bucket rate limiter gates **mutating** tools only | gate `src/mcp/server.rs`; `is_mutating_tool` `src/mcp/server.rs` |
| Malformed / truncated / hostile `.PcbLib` / `.SchLib` input | Parsers return `Err`, never panic, on arbitrary bytes — proven by property tests | `tests/property_tests.rs` |
| Decompression bombs / oversized embedded 3D models | Each embedded model is decompressed through a bounded reader; output beyond `MAX_DECOMPRESSED_MODEL_BYTES` (256 MiB) is rejected and the model skipped, so a high-ratio zlib stream cannot exhaust memory | `decompress_model_data` / `decompress_capped` `src/altium/pcblib/reader.rs` |

A note on the parser guarantee: the no-panic property is not merely asserted, it is
exercised. The property tests both feed purely random bytes (rejected early at the OLE
container layer) and mutate a *valid* seed library so corrupted stream bytes actually
reach the Altium parsers, asserting only `Ok` or `Err` results in every case
(`tests/property_tests.rs`).

### NOT protected against / out of scope

These are honest limitations. The tool does not pretend to defend against them.

| Threat | Why it is out of scope |
|--------|------------------------|
| A compromised or malicious MCP client | The client runs with the user's privileges and drives the tool directly; confining it is the OS's job, not this tool's |
| A malicious local user with filesystem access | Anyone who can run the binary can already read and write the same files by other means |

---

## Security Controls

### Path confinement (`validate_path`)

Every file-touching tool routes its `filepath` argument through `validate_path`
(`src/mcp/server.rs`) before any I/O. The check **canonicalises** the target so
that `..` segments and symlinks are resolved to a real path; for files that do not yet
exist (new-file writes) it canonicalises the parent directory and re-appends the file
name. The resolved path is then accepted only if it is a prefix of at least one
configured allowed path (`canonical_path.starts_with(&canonical_allowed)`).

### Generic denial message

When confinement fails, the function returns a single fixed string —
`"Access denied: path is outside the configured allowed directories"`
(`src/mcp/server.rs`). It contains **no path, no allow-list contents, and no OS
error text**, so a denial cannot be used to probe the filesystem layout. The
intermediate failure messages likewise surface only the sanitised file name.

### Backups with bounded retention (`create_backup` / `MAX_BACKUPS`)

Before a destructive operation, `create_backup` (`src/mcp/server.rs`) copies the
existing file to a timestamped `filepath.YYYYMMDD_HHMMSS.bak`. New-file creation is a
no-op. Retention is bounded: `MAX_BACKUPS = 5` (`src/mcp/server.rs`) and
`cleanup_old_backups` (`src/mcp/server.rs`) prunes the oldest backups so the safety
net cannot itself become an unbounded-disk-usage DoS.

### Rate limiting (mutating tools only)

`handle_tools_call` gates execution behind a token bucket: if the requested tool is a
mutating tool **and** `rate_limiter.try_acquire()` fails, the call is rejected before
any work is done (`src/mcp/server.rs`). The mutating set is enumerated explicitly
in `is_mutating_tool` (`src/mcp/server.rs`) — read-only tools (reads, listings,
diffs, renders, validation) are never throttled. The bucket is configured by
`RateLimitConfig` (`src/config/settings.rs`): `max_burst` (default `120`) and
`refill_per_sec` (default `30.0`). Validation rejects a zero burst (which would block
every operation) and a non-finite or negative refill rate
(`Config::validate`, `src/config/settings.rs`).

### Error-path sanitisation

`AltiumError::FileRead` and `AltiumError::FileWrite` render their `Display` through
`sanitise_path_for_client` (`src/altium/error.rs`), which emits only the final path
component (falling back to `<file>`). This is deliberately important for writes: the
atomic-write temporary path (for example `…/MyLib.pcblib.tmp`) is never surfaced to the
client. The full path is retained in the structured error field for `tracing` at debug
level only. Regression tests assert that neither read nor write errors leak their
directory (`src/altium/error.rs`, `src/altium/error.rs`).

**Central sanitiser choke-point.** Beyond the per-variant `Display` sanitisation above,
every client-facing error funnels through one egress point: `ToolCallResult::error` and
`ToolCallResult::error_with_context` route their text through
`redact_absolute_paths` (`src/util.rs`), which replaces any absolute filesystem path
(Windows drive/UNC, or a Unix absolute path at the start of a token) with its final
component. This is defence in depth: even if a future tool interpolates a raw absolute
path into a message, the directory structure is not disclosed. It is deliberately
conservative — relative paths (`./Lib.PcbLib`), embedded segments, and URLs are left
untouched (see the unit tests in `src/util.rs`).

---

## Accepted Defaults and Known Limitations

### The cwd-scoped `"."` loose default

When the configuration file specifies no `allowed_paths`, the CLI substitutes
`["."]` — the current working directory — before constructing the server
(`src/main.rs`). This is the intended out-of-the-box behaviour: with no explicit
configuration, access is scoped to the directory the server was launched from, which is
broad but local. Operators who need tighter confinement should set `allowed_paths`
explicitly.

### Empty allow-list fails closed

`validate_path` returns an error for **every** path when `allowed_paths` is empty,
rather than granting whole-filesystem access (`src/mcp/server.rs`). In normal operation
this branch is unreachable, because the CLI substitutes `["."]` before constructing the
server (`src/main.rs`), so the net default is cwd-scoped. It matters for embedders that
construct `McpServer` directly: such a server denies all file access until an allow-list
is supplied.

---

## Contributor Checklist

These rules encode the project's security invariants (see `.claude/CLAUDE.md`). Treat
them as gates on any change that touches files or error messages.

### Do

- **Validate every path before I/O.** Route every `filepath` argument through
    `validate_path` before any read or write — no exceptions for "internal" helpers.
- **Use sensible defaults when config is missing.** Prefer a conservative, local
    default (the cwd-scoped `["."]`) over silently widening access.
- **Sanitise paths in client-facing errors.** Render only the final path component via
    `sanitise_path_for_client`; keep full paths to `tracing` at debug level.
- **Add path-traversal tests for any new file-touching tool.** Cover `..` escape,
    absolute-path escape, and a non-existent allowed path, mirroring
    `validate_path_rejects_traversal_outside_allowed` (`src/mcp/server.rs`).

### Don't

- **Don't write arbitrary files outside the allowed paths.** Never bypass
    `validate_path`, and never construct an output path that is not derived from a
    validated input.
- **Don't widen `allowed_paths` semantics.** Do not loosen the prefix check, do not add
    new allow-all branches, and do not relax canonicalisation.
- **Don't put absolute or internal paths in user-facing errors.** No full paths, no
    temp-file paths, no OS error text in any string returned to the client.
- **Don't push to main.** Security-relevant changes go through review on a branch.
