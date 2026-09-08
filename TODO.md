# TODO — Working Notebook

Outstanding work only — shipped items are deleted, not struck through (git log is the
record). The specialised worklists stay the single source of truth for their areas:

| Area | Worklist |
|------|----------|
| Golden-fixture enrichment and verified negatives | `scripts/samples/COVERAGE.md` |

## B. After v1.0.0

- [ ] **Streamable HTTP transport** (v1.1.0) alongside stdio, so web-only assistants
      (claude.ai in the browser, ChatGPT) can connect as a remote server — today they
      cannot (`docs/CLIENT_SETUP.md` § Web-only assistants). Deliberately after 1.0.
- [ ] **Windows code signing through SignPath Foundation** — free for open-source
      projects, HSM-held key, signs from GitHub Actions. Decided 2026-09-02 over the paid
      routes (Azure Artifact Signing ~$10/month on a paid subscription, commercial OV/EV
      $200–700/year): the publisher line reads "SignPath Foundation", which is fine, and
      SmartScreen reputation then builds under an established identity. Steps: write the
      short code-signing policy page their terms require (roles, MFA, credit), apply, add
      their action to the repository's action allow-list, sign in the `build` job before
      packaging so the attestation covers the signed binary, and drop the SmartScreen
      caveat from the docs and release notes.
- [ ] **Sign the `.mcpb` bundle too** (`mcpb sign` / `mcpb verify`) once a certificate
      exists — after checking what Claude Desktop shows for a signed side-loaded bundle.
- [ ] **macOS notarisation** (Apple Developer Program, $99/year) only when macOS downloads
      justify it — 3 of the 81 v0.2.0 downloads today. Until then the docs' right-click →
      Open note stands.

## E. v2.0 candidates (breaking)

Glama's Server Coherence review (2026-09-08, grade A overall) marked two things that
only a major version can change, since the tool interface follows semantic versioning:

- [ ] **Consolidate the read-side tools.** Tool Count scored 2/5: 34 tools "feels heavy
      for agent selection", with `read_pcblib`/`read_schlib`, `get_component`,
      `list_components`, `search_components` and `export_library` named as granular
      variants of one operation, and `update_component`/`update_pad`/`update_primitive`
      likewise. Sketch: one `read_library` (type from the extension) with `component`,
      `query`, `details` and `format` arguments, one `update` with a `target` argument;
      keep the old names as deprecated aliases for one minor release first.
- [ ] **Regularise the verb_noun names.** Naming Consistency scored 4/5 over
      `component_exists` (noun-verb), `batch_update`, `bulk_rename` and the fused
      `read_pcblib`/`read_schlib`; renames go in the same major bump as the
      consolidation.

## C. Outreach

- [ ] **Claude Connectors Directory** — submit the v1.0.1 `.mcpb` (it carries the
      tool annotations, icon and privacy policy the directory requires) through
      Anthropic's [desktop extension form](https://clau.de/desktop-extention-submission).
      On hold until the maintainer says go; needs a human with the account, plus the
      documentation URL, privacy policy URL and icon.
- [ ] **OpenAI / ChatGPT** — its directory takes remote (HTTPS) servers only, so it waits
      for the v1.1.0 Streamable HTTP transport; Codex CLI users are covered already.

## D. Maintenance & waiting

- [ ] **Drop the `cfb` git pin** (`[patch.crates-io]`, rev `8c1ec76`) as soon as rust-cfb
      publishes a release newer than v0.14.0 — check
      [rust-cfb releases](https://github.com/mdsteele/rust-cfb/releases) at session start.
