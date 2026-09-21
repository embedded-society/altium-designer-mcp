# TODO — Working Notebook

Outstanding work only — shipped items are deleted, not struck through (git log is the
record). The specialised worklists stay the single source of truth for their areas:

| Area | Worklist |
|------|----------|
| Golden-fixture enrichment and verified negatives | `scripts/samples/COVERAGE.md` |

## B. After v1.0.0

- [ ] **OAuth for the HTTP transport**, so claude.ai in the browser and ChatGPT can
      connect: they reach only public HTTPS servers and authenticate with OAuth or not at
      all, while `--http` takes a static bearer token (`docs/CLIENT_SETUP.md` § Web-only
      assistants). The MCP authorization specification makes the server an OAuth resource
      server that validates tokens from an external authorization server; the open
      questions are which authorization server a single user runs and how the server is
      hosted with TLS, both the maintainer's call before any code.
- [ ] **Windows code signing through SignPath Foundation** — free for open-source
      projects, HSM-held key, signs from GitHub Actions; decided 2026-09-02 over the paid
      routes. Ready on the repository side: the [code signing
      policy](docs/CODE_SIGNING_POLICY.md) their terms require (attribution line, roles,
      privacy statement), linked from the README and every release's notes; the Windows
      version resource with product name and version, checked in the `build` job;
      organisation-wide two-factor authentication. Left, in order: the maintainer applies
      at signpath.org; after approval, add SignPath's GitHub action to the repository's
      action allow-list and its API token as a secret; configure the artifact (product
      name and version restrictions); sign in the `build` job before packaging so the
      attestation covers the signed binary; then drop the SmartScreen caveat and the
      "application pending" note from the docs and release notes.
- [ ] **Sign the `.mcpb` bundle too** (`mcpb sign` / `mcpb verify`) once a certificate
      exists — after checking what Claude Desktop shows for a signed side-loaded bundle.
- [ ] **macOS notarisation** (Apple Developer Program, $99/year) only when macOS downloads
      justify it. As of 2026-09-13, v1.0.1 counted 1 macOS download against 67 for Windows
      and 42 for the Claude Desktop bundle, and later releases show the same pattern.
      Until then the docs' right-click → Open note stands.

## C. Outreach

- [ ] **Claude Connectors Directory** — submit the latest release's `.mcpb` (it carries
      the tool annotations, icon and privacy policy the directory requires) through
      Anthropic's [desktop extension form](https://clau.de/desktop-extention-submission).
      On hold until the maintainer says go; needs a human with the account, plus the
      documentation URL, privacy policy URL and icon.
- [ ] **OpenAI / ChatGPT** — its directory takes remote (HTTPS) servers only, so it waits
      for OAuth on the HTTP transport (§ B); Codex CLI users are covered already.
- [ ] **Upstream: forward server instructions through `mcp-proxy`.** Glama runs every
      stdio server behind punkpeye/mcp-proxy, which builds its server from the backend's
      name, version and capabilities but never passes `instructions` on, so Glama shows
      "no instructions" for this server and every other stdio server. The fix is one
      line where `src/bin/mcp-proxy.ts` constructs its `Server`
      (`instructions: client.getInstructions()`), offered as an issue plus a PR. Parked by
      the maintainer on 2026-09-07; raise again when asked.

## D. Maintenance & waiting

- [ ] **Waiting on others.**
    - #67: if bingran names their AI client, answer with its section of
      `docs/CLIENT_SETUP.md`.
- [ ] **Golden-fixture enrichment backlog**, detailed with its procedure in
      `scripts/samples/COVERAGE.md` § Remaining enrichment backlog:
    - Hand-authored evidence only, since AD24 scripting cannot produce it: a via longer
      than the 321-byte template (an older Altium's), and a pad power-plane connection
      (@67-85) other than the default: AD24's Thermal Relief box writes the
      polygon-connect override instead (`manual/thermal_relief.PcbLib`). A footprint
      link with `IntegratedModel`/`DatabaseModel` waits on finding the Altium action
      that writes the flags: the UI's Add Footprint does not
      (`manual/footprint_link.SchLib`).

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
