# Error Reference

This document is a consolidated catalogue of every error this MCP server can surface, the
condition that produces it, and how to resolve it. It complements the operational hints in
[CLAUDE_CODE_GUIDE.md § Troubleshooting](CLAUDE_CODE_GUIDE.md#troubleshooting), which covers
client-side setup problems, and the `isError` example in the
[README § read_footprint](../README.md#mcp-tools), which it does not repeat here.

---

## How Errors Surface

Errors reach the client through two distinct channels, and the channel determines both the
shape of the payload and how a client should react.

- **Protocol errors** are JSON-RPC 2.0 error objects. They indicate that the request itself
    could not be processed at the protocol layer: malformed JSON, a missing `jsonrpc` field, an
    unknown method, or invalid parameters. These are returned in the top-level `error` field of
    a JSON-RPC response and never reach a tool handler.
- **Tool-call results** are successful JSON-RPC responses whose `result` is a `ToolCallResult`
    carrying `isError: true`. The request was well-formed and dispatched to a tool, but the
    operation failed (for example, a component was not found or a file could not be written).
    This is the standard MCP convention so that the model can read and reason about the failure
    rather than having the call aborted at the protocol layer.

In short: a protocol error means *the server could not understand the request*; an
`isError: true` result means *the server understood the request but the operation failed*.

---

## JSON-RPC Protocol Error Codes

These codes come from the `ErrorCode` enum in `src/mcp/protocol.rs`. The numeric value is
returned in the `code` field of the JSON-RPC error object.

| Code | Name | When It Occurs |
|------|------|----------------|
| `-32700` | Parse error | The request body was not valid JSON, or the top-level value was not a JSON object. |
| `-32600` | Invalid Request | The JSON parsed but is not a valid Request object: the `jsonrpc` field is missing, is not `"2.0"`, or the `method` field is empty. |
| `-32601` | Method not found | The `method` does not exist or is not available on the server. The offending method name is included in the message. |
| `-32602` | Invalid params | The method parameters are invalid (wrong type, missing required argument, and so on). |
| `-32603` | Internal error | An unexpected internal failure occurred while handling an otherwise valid request. |
| any `i32` | Server error | A server-defined error. `ServerError(i32)` carries an arbitrary implementation-defined code. |

### Request-ID Preservation on Invalid Request

When an *Invalid Request* (`-32600`) is raised, the server makes a best effort to echo the
original request `id` back in the error response so that a strict client can correlate the
failure with its outstanding request. The `id` is recovered up front, before validation
fails.

- A valid string or integer `id` is preserved and returned.
- A non-conforming `id` (`null`, a float, an array, or an object) is **not** a valid
    JSON-RPC id and is dropped — the error response carries no `id`.
- A *Parse error* (`-32700`) never carries an `id`, because the request could not be parsed
    far enough to recover one.
- Notifications (messages with no `id`) that fail validation also carry no `id`.

---

## Configuration Errors (`ConfigError`)

These errors come from `src/error.rs` and occur while loading or validating the server
configuration at start-up. A configuration error prevents the server from starting.

| Variant | Cause | Remedy |
|---------|-------|--------|
| `ReadError` | The configuration file exists but could not be read (for example, permission denied). Wraps the underlying I/O error. | Check the file permissions and that the path is accessible to the process. |
| `ParseError` | The configuration file is not valid JSON. Wraps the underlying `serde_json` error. | Fix the JSON syntax. Note that unknown fields are rejected, so remove any stray keys. |
| `NotFound` | No configuration file was found at the expected path. | Create a `config.json` at the expected location, or point the server at an existing one. |
| `ValidationError` | The configuration parsed but failed a semantic check (see the validation rules below). | Correct the offending value as described in the message. |

### Validation Rules

`Config::validate` in `src/config/settings.rs` enforces the following. A breach of any rule
produces a `ValidationError` whose `message` names the offending field.

- **Log level.** `logging.level` must be one of `trace`, `debug`, `info`, `warn`, or
    `error` (matched case-insensitively). The default is `warn`.
- **Rate-limit burst.** `rate_limit.max_burst` must be greater than `0`; a zero burst would
    block every mutating operation. The default is `120`.
- **Rate-limit refill.** `rate_limit.refill_per_sec` must be a finite, non-negative number.
    A value of `0.0` is valid and permits a single burst with no refill. The default is
    `30.0`.

Rate limiting applies only to destructive (file-mutating) operations; read-only tools are
never rate limited.

---

## Altium File Errors (`AltiumError`)

These errors come from `src/altium/error.rs` and occur during `.PcbLib` / `.SchLib` file
operations. They are surfaced to the client as tool-call results with `isError: true`.

| Variant | Cause | Remedy |
|---------|-------|--------|
| `FileRead` | The library file could not be opened or read. Wraps the underlying I/O error. | Confirm the file exists, is readable, and lies within an allowed path. |
| `FileWrite` | The library file (or its atomic-write temporary file) could not be written. Wraps the underlying I/O error. | Confirm the target directory is writable and within an allowed path, and that there is sufficient disk space. |
| `InvalidOle` | The file is not a valid OLE compound document, or its structure is malformed. | Verify the file is a genuine Altium library and is not truncated or corrupt. |
| `MissingStream` | A required stream is absent from the OLE document. The stream name is included. | The file is incomplete or not a recognised Altium library; regenerate or replace it. |
| `ParseError` | Binary data could not be parsed at a given byte offset. Both the offset and a description are included. | The file is corrupt or uses an unexpected layout; verify it opens in Altium Designer. |
| `InvalidParameter` | A parameter value supplied to a primitive or operation is invalid. The parameter name and reason are included. | Correct the named parameter to a valid value. |
| `ComponentNotFound` | The requested component does not exist in the library. The component name is included. | Check the component name and list the library's components first. |
| `UnsupportedVersion` | The file declares a version this implementation does not support. The version string is included. | Open and re-save the library in a supported Altium Designer version. |
| `CompressionError` | Compression or decompression of stream data failed. Optionally wraps an underlying I/O error. | The stream is corrupt or uses an unexpected encoding; verify the source file. |
| `WrongFileType` | The file is the wrong kind (for example, a `.SchLib` opened as a `.PcbLib`). The expected and actual types are included. | Call the tool that matches the file type, or supply the correct file. |

---

## Tool-Call Result and Error-Context Shapes

When a tool fails, the handler returns a `ToolCallResult` (defined in `src/mcp/server.rs`).
The envelope serialises to the standard MCP shape: a `content` array of typed items plus an
`isError` flag that is present only when `true`.

```json
{
    "content": [
        {
            "type": "text",
            "text": "Component 'SOIC-99' not found in library."
        }
    ],
    "isError": true
}
```

For richer diagnostics, a handler may build the text payload from an `ErrorContext`. This
produces a structured, pretty-printed JSON document inside the `text` field, with consistent
keys across operations. Optional fields are emitted as `null` when not set.

```json
{
    "content": [
        {
            "type": "text",
            "text": "{\n  \"status\": \"error\",\n  \"operation\": \"write_pcblib\",\n  \"error\": \"Failed to write file: MyLibrary.PcbLib\",\n  \"filepath\": \"MyLibrary.PcbLib\",\n  \"component\": \"SOIC-8\",\n  \"details\": \"atomic write failed\"\n}"
        }
    ],
    "isError": true
}
```

The structured document inside `text` always carries these keys:

- **`status`** — always `"error"` for a failed call.
- **`operation`** — the operation being performed, for example `write_pcblib` or
    `delete_component`.
- **`error`** — the human-readable error message.
- **`filepath`** — the file being operated on, or `null` if not applicable.
- **`component`** — the component being processed, or `null` if not applicable.
- **`details`** — additional context about what was happening, or `null`.

---

## Path Sanitisation

Internal file paths are never disclosed in client-facing error messages. The `FileRead` and
`FileWrite` variants of `AltiumError` deliberately render only the final path component (the
file name) in their `Display` output, via `sanitise_path_for_client`. This prevents leaking
internal directory structure and atomic-write temporary paths (for example,
`…/MyLib.pcblib.tmp`) to the client.

The full path remains available in the structured error field for server-side `tracing` at
debug level — it is simply never sent to the client. When a path has no final component, the
sanitised value falls back to `<file>`.
