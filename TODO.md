# TODO — Working Notebook

> **Active effort:** Adopt infrastructure "goodies" from the sibling project
> `git-proxy-mcp` to make `altium-designer-mcp` superior.
> **Branch:** `chore/adopt-git-proxy-goodies` (off `main`).
> **Owner instruction:** keep this file current as a recovery notebook in case the
> chat context compacts. Update statuses as work lands.

## Ground rules (carry across compaction)

- British English, 4-space indent, Conventional Commits, trailer
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`.
- **NEVER** push to `main`. Commit per tier on the feature branch. No push unless asked.
- **DO NOT modify** `CHANGELOG.md` or `CODE_OF_CONDUCT.md` (off limits).
- Keep the existing npm + github-actions dependabot entries (npm covers markdownlint-cli2).
- Baseline before work: `cargo build` + `cargo test` green (69+9+... tests pass, 0 fail).
- Reference project root: `c:/Users/matej/Desktop/embedded society/git-proxy-mcp`.
- Verify after each Rust change: `cargo build`, then at the end `cargo fmt --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`.

## Source of the plan

A verified workflow comparison (49 candidates → 45 kept, 4 rejected). Rejections
(do NOT implement): manual action-SHA bumps (dependabot handles), SecurityConfig/guard-trait
abstraction (overwrite already hard-blocked), `run_with_debug_logs` (needs coverage; folded in),
progress-reporting module (sync dispatch, poor fit).

---

## Tier 1 — Quick wins (CI + config + small code) — ✅ DONE (df2fd03, 7049a10, 2d37898)

- [ ] **T1.1** Fix `release.yml` prerelease bug — never mark a prerelease `--latest`.
      Use a `RELEASE_FLAGS` array: `(--prerelease --latest=false)` if version has `-`, else `(--latest)`.
      (git-proxy release.yml ~261-272; altium block ~221-235.)
- [ ] **T1.2** Add `cargo` ecosystem to `.github/dependabot.yml` (weekly Mon, limit 5,
      single group `cargo-dependencies` patterns `["*"]`). Keep github-actions + npm.
- [ ] **T1.3** Add `timeout-minutes` to jobs missing it: ci_main quick-checks(10)/ci-success(5),
      ci_pr quick-checks(10)/ci-success(5), release validate(5)/release(10), cleanup_caches cleanup(5).
- [ ] **T1.4** Least-privilege permissions: ci_main workflow `contents: read` + job-level
      `{contents: read, actions: write}` on ci-success; release workflow `contents: read`,
      move write scopes to the release job, re-declare `contents: read` on validate+build.
- [ ] **T1.5** Concurrency groups on all 4 workflows (release: no-cancel; ci_main/ci_pr: cancel;
      ci_pr group needs `pull_request.number || github.ref` fallback; cleanup: no-cancel).
      Flip cleanup_caches `dry_run` default → true. Optionally sync cleanup-caches.js to KiB/MiB/GiB.
- [ ] **T1.6** CI cache keys: `hashFiles('**/Cargo.lock')` → `hashFiles('Cargo.toml')`
      (ci_main ~121,160; ci_pr ~116). (Cargo.lock is gitignored/untracked.)
- [ ] **T1.7** Harden release version extraction → `[package]`-scoped awk (release.yml ~52).
- [ ] **T1.8** `.gitattributes`: add `*.PcbLib binary` + `*.SchLib binary`; then `git add --renormalize .`.
- [ ] **T1.9** `tracing-subscriber` `env-filter` feature + wire `EnvFilter` in main.rs init_tracing
      (preserve -v/-q/config level via `add_directive`). Document RUST_LOG knob in CONTRIBUTING.md.
- [ ] **T1.10** Preserve JSON-RPC request id across Invalid Request parse failures
      (port `extract_request_id`; apply at invalid_request sites only, NOT the notification branch). Port tests.
- [ ] **T1 COMMIT**

## Tier 2 — High-value (coverage, tests, toolchain, security controls) — ✅ DONE (e12785d, ad591f3, e13dfa3, 112d224, 325de89, c1221fd)

- [ ] **T2.1** `codecov.yml` at root (drop build.rs ignore) + `cargo-llvm-cov` job in ci_pr + ci_main
      (`--all-features --workspace --lcov`), upload to codecov, wire into ci-success needs (skipped-tolerant),
      dependabot skip, CODECOV_TOKEN secret. Add lcov.info/*.lcov to .gitignore.
- [ ] **T2.2** Pin exact rustc in `rust-toolchain.toml` (concrete >=1.75; converge ~1.95.0 after it lints clean)
      + bump all 5 `dtolnay/rust-toolchain` pins to matching `# vX.Y.Z` + SHA + copy `check-toolchain-pin.sh`
      as first quick-checks step in both CI files.
- [ ] **T2.3** Path-leak fix: `AltiumError` Display for FileRead/FileWrite → basename only (keep PathBuf for debug!).
      Add `sanitize_path_for_client` helper; fix 4 diff `format!` sites + validate_path `.display()` builders.
      Add unit test: write error to temp path contains no parent dir string.
- [ ] **T2.4** Token-bucket rate limiting: copy git-proxy `src/security/rate_limit.rs` verbatim into new
      `src/security/`. Add `RateLimitConfig` to settings (validate). Gate ONLY mutating tools at
      handle_tools_call chokepoint; reads unmetered. Defaults higher than git-proxy's 20/5 (local I/O).
- [ ] **T2.5** Property/no-panic fuzzing: `proptest` dev-dep. PcbLib reader is PRIVATE → in-crate `#[cfg(test)]`;
      SchLib::read is pub → drivable from tests/property_tests.rs via Cursor. Seed valid-OLE-prefix mutations
      (random bytes rejected at cfb::open). Commit proptest-regressions/.
- [ ] **T2.6** Coordinate validator proptests + path-traversal/security tests in the EXISTING in-crate
      `#[cfg(test)] mod tests` (validate_path/validate_coordinate are private). Cover NaN/Inf/boundary,
      ../ traversal, symlink escape, empty-allowlist, no path leak in rejection msg.
- [ ] **T2.7** `tests/perf_tests.rs` (measure_avg/format_duration from git-proxy): save→open 100-fp lib,
      base64 of ~1MB STEP, flate2 round-trip. PcbLib::save is &mut self → recreate per iter. Generous thresholds.
- [ ] **T2.8** Python E2E harness `tests/integration/` (adapt git-proxy McpTestClient/TestRunner):
      spawn `[binary, path]` (positional config), round-trip create→read, error paths. POSIX select → run on ubuntu.
      Add integration CI job (build release, run python3).
- [ ] **T2 COMMIT**

## Tier 3 — Larger projects + docs — ✅ DONE (bb9d6fc, 7bb7afa, d5a51d2)

> **Scope notes (honest record):**
>
> - **T3.1 server.rs split:** the 1,262-line `get_tool_definitions()` was extracted to
>   `src/mcp/tool_definitions.rs` (server.rs 13.3k → ~12.0k lines) and `escape_csv_field`
>   moved to `src/util.rs`. The full per-tool **handler-body** relocation into
>   `src/mcp/tools/` was **deferred** — safely moving ~33 handlers + the shared-helper web
>   on a 12k-line file is better done as a dedicated, compiler-guided pass than via
>   line edits. Recommended follow-up.
> - **T3.4 error converter:** `ToolCallResult::from_altium` added + tested, but the ~40
>   existing `Failed to … {e}` arms were **not** mass-converted — they already emit
>   sanitised messages (the security fix lives in `AltiumError` Display, T2.3), so a
>   bulk rewrite was churn/risk without security benefit.
> - **T3.6:** empty-allowlist fail-closed done; the canonical-path cache micro-opt was
>   skipped (low value, the per-call canonicalize is fine for a local single-user tool).
> - **T3.3 audit log:** logs at the dispatch chokepoint (per-call granularity), not per
>   component, avoiding signature changes to ~33 associated fns.

- [ ] **T3.1** Split `src/mcp/server.rs` (13,282 lines) into `src/mcp/tools/` per-tool modules + `tools/shared.rs`
      (create_backup, post_write_validation_*, parse_pad/track/arc/region/text, validate_ole_name,
      validate_coordinate, escape_csv_field, draw_line, most_common). Free fns taking `&[PathBuf]`.
      Keep dispatch match + thin wrappers in server.rs. Group-by-group, compile between each.
      Follow-ons: extract per-tool `*_definition()` ctors; relocate tests next to handlers.
- [ ] **T3.2** `src/util.rs` (`pub mod util;` in lib.rs) — move `escape_csv_field`; optional sanitize_for_log.
- [ ] **T3.3** Audit log `src/security/audit.rs` (use chrono, drop git fields + hand-rolled time helpers).
      `audit_log_path: Option<PathBuf>` in LoggingConfig. NOTE several destructive fns are assoc fns (no &self)
      — pass &AuditLogger or log at &self dispatchers. Sequence AFTER path-sanitiser.
- [ ] **T3.4** Central `ToolCallResult::from_altium(operation, &err)` converter routing through sanitiser;
      replace ~12 duplicated json! arms. Add test (is_error + no absolute path).
- [ ] **T3.5** Extract transport framing free fns (read_message_line/write_message_line + strip_trailing_newline);
      port framing tests; doc max line size for inline base64 payloads.
- [ ] **T3.6** Harden `validate_path`: empty-allowlist fail-closed/warn; cache canonical allowed paths at startup;
      bypass tests (covered by T2.6); document residual TOCTOU in docs/SECURITY.md.
- [ ] **T3.7** `docs/errors.md` — JSON-RPC codes, 4 ConfigError, 10 AltiumError variants, ToolCallResult shapes.
      Register in CONTRIBUTING SSoT + CLAUDE.md Quick Reference.
- [ ] **T3.8** `docs/SECURITY.md` threat model (local-file-IO reframe) + scope banner on root SECURITY.md.
- [ ] **T3.9** CONTRIBUTING.md (security section, PR checklist path-safety + markdownlint, align clippy/fmt cmds,
      expand doc tables) + STYLE.md (SSoT pointer, Python style section). Expand config/example-config.json
      (needs `_note` serde field first). Add `__pycache__/` to .gitignore.
- [ ] **T3 COMMIT**

## Final — ✅ green

- [x] `cargo fmt --all --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
      `cargo test` (198 lib + property + perf + integration + doctests) all pass.
- [x] `markdownlint-cli2 "**/*.md"` — 0 errors.
- [x] Python E2E (`tests/integration/test_mcp_tools.py`) — 25/25.
- [x] Toolchain pinned to 1.95.0; `check-toolchain-pin.sh` passes.
- Branch `chore/adopt-git-proxy-goodies`; not pushed (awaiting review).

---

## Archived — PcbLib Writer Bug Fixes (prior effort, READY FOR TESTING)

All format bugs identified by studying AltiumSharp (C#) and pyAltiumLib (Python) have been fixed.
Ready for manual testing with Altium Designer.

### Completed Tasks

#### 1. Fix OLE Version

- [x] Changed to OLE v3 (512-byte sectors) in both PcbLib and SchLib

#### 2. Fix `FileHeader` Stream

- [x] Changed from pipe-delimited key=value to binary version string
- [x] Format: `[string_len:4 LE][string_len:1]["PCB 6.0 Binary Library File"]`
- [x] Reader updated to handle both formats (backward compatible)

#### 3. Fix `/Library/Data` Stream

- [x] Added `/Library` storage with Header and Data streams
- [x] Parameter block uses null-terminated encoding (`[block_len:4][params + \x00]`)
- [x] Reader parses `/Library/Data` for component ordering

#### 4. Fix Per-Component Streams

- [x] `/{component}/Header` — exact primitive count (removed erroneous +1)
- [x] `/{component}/Parameters` — block-prefixed with null terminator
- [x] `/{component}/Data` — binary primitives
- [x] `/{component}/WideStrings` — block-prefixed, leading pipe, empty = `[2:4]["|" + \x00]`
- [x] `/{component}/UniqueIdPrimitiveInformation` — 0-based indexing, null-terminated records

#### 5. Fix Model Data Stream

- [x] Records use leading `|` pipe character
- [x] Null terminator included in block length

#### 6. Reader Backward Compatibility

- [x] `FileHeader` reader handles both binary and pipe-delimited formats
- [x] UniqueID reader auto-detects 0-based vs 1-based indexing
- [x] Null terminators stripped from UniqueID records before parsing

### Testing Required

- [ ] Create a new PcbLib with the MCP server
- [ ] Open in Altium Designer — verify it loads without errors
- [ ] Check footprint rendering in Altium
- [ ] Test round-trip: create in MCP, open in Altium, save, open in MCP

### Key Changes Summary

| Stream | Old (Broken) | New (Correct) |
|--------|--------------|---------------|
| OLE Version | V4 (4096-byte) | V3 (512-byte) |
| `/FileHeader` | Pipe-delimited key=value | Binary version string |
| `/Library/Data` params | No null terminator | `[block_len:4][params + \x00]` |
| `/{comp}/Header` | primitive_count + 1 | Exact primitive_count |
| `/{comp}/Parameters` | Raw text | Block-prefixed with null terminator |
| `/{comp}/WideStrings` (empty) | `[0:4]` | `[2:4]["\|" + \x00]` |
| UniqueID indexing | 1-based | 0-based |
| UniqueID records | No null terminator | Null-terminated, included in block length |
| Model Data records | No leading pipe | Leading `\|`, null in block length |

### Files Modified (prior effort)

- `src/altium/pcblib/mod.rs` — FileHeader, Library/Data, Parameters, reader fixes
- `src/altium/pcblib/writer.rs` — Header count, WideStrings, UniqueID, Model Data encoders
- `src/altium/pcblib/reader.rs` — UniqueID null stripping, 0/1-based auto-detection
- `src/altium/schlib/mod.rs` — OLE v3
- `docs/PCBLIB_FORMAT.md` — Updated format documentation
