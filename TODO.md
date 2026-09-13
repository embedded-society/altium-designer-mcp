# TODO — Working Notebook

Outstanding work only — shipped items are deleted, not struck through (git log is the
record). The specialised worklists stay the single source of truth for their areas:

| Area | Worklist |
|------|----------|
| Golden-fixture enrichment and verified negatives | `scripts/samples/COVERAGE.md` |

## B. After v1.0.0

- [ ] **Polish three tool descriptions Glama's re-review marked down** (after v1.0.3,
      2026-09-13; the letter grade stayed A, the tool average fell to 4.34). Non-breaking,
      ships with the next patch release, same approach as the five rewritten for 1.0.2:
    - `update_component` (3.7, now the lowest of 34): Usage Guidelines 3 and Behaviour 3.
      State that the component must already exist, that the footprint/symbol object
      replaces it wholesale, that a different `name` in the object renames it and a clash
      is refused, that its position is kept, and what `dry_run` reports. Name the
      alternatives: `write_pcblib`/`write_schlib` for a whole library, `batch_update` for
      many components, `update_pad`/`update_primitive` for one primitive,
      `rename_component` for a rename alone.
    - `read_pcblib` (5.0 → 4.6): say when to prefer `get_component` or
      `search_components`, explain `compact`, and break the single dense paragraph up.
    - `read_schlib` (4.5 → 4.4): the same routing, and add what the schema alone does not
      say rather than restating `component_name`/`limit`/`offset`.
    - Then regenerate `docs/TOOLS.md`, add the CHANGELOG entry, and after the release run
      Sync Server plus Build & Release on Glama's admin page so it re-scores.
- [ ] **Make the release job survive GitHub 5xx errors.** During a GitHub incident on
      2026-09-13, `gh release create` with all six assets failed three times: once before
      the draft existed, once after five uploads, leaving a partial draft, and once on a
      re-run. Create the draft without assets first, then upload each asset with retries
      (`gh release upload --clobber`), so a transient error neither aborts the job nor
      leaves a partial draft, and let a re-run reuse a draft that already exists.
- [ ] **Document the recovery and keep the release review in the repository.** Add to
      `docs/RELEASING.md` § If something is wrong: a partial draft left by a GitHub error
      is deleted by its id while the tag stays, the failed job is re-run, and the run's
      artefacts expire 7 days after the tag push. Commit the draft-review and publish
      scripts used for v1.0.3 under `scripts/release/` and point step 8 at them: hard
      pass/fail checks for the checksums, all six attestations, identical bundles, the
      manifest and binary versions, the MCP handshake and the release notes against the
      CHANGELOG section.
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
      for the v1.1.0 Streamable HTTP transport; Codex CLI users are covered already.
- [ ] **Upstream: forward server instructions through `mcp-proxy`.** Glama runs every
      stdio server behind punkpeye/mcp-proxy, which builds its server from the backend's
      name, version and capabilities but never passes `instructions` on, so Glama shows
      "no instructions" for this server and every other stdio server. The fix is one
      line where `src/bin/mcp-proxy.ts` constructs its `Server`
      (`instructions: client.getInstructions()`), offered as an issue plus a PR. Parked by
      the maintainer on 2026-09-07; raise again when asked.

## D. Maintenance & waiting

- [ ] **Waiting on others.**
    - #507: Kylinghu's retest of v1.0.3, running the six mutation paths against a copy
      of their Altium Designer 21 library. Pass: close the issue. Fail: the `olefile`
      storage listing before and after the write is the input for the next fix.
    - #67: if bingran names their AI client, answer with its section of
      `docs/CLIENT_SETUP.md`.
- [ ] **Golden-fixture enrichment backlog**, detailed with its procedure in
      `scripts/samples/COVERAGE.md` § Remaining enrichment backlog:
    - PcbLib region hole contour: probe a two-contour `TGeometricPolygon` through
      `PCBGeometricPolygonFactory`; the same probe settles whether a region `NET` exists.
    - Hand-authored evidence only, since AD24 scripting cannot produce it: a symbol
      footprint link with `IntegratedModel`/`DatabaseModel` as a golden, text beyond
      U+00FF, a via longer than the 321-byte template, and pad thermal relief or
      power-plane connection.
- [ ] **Per-library ANSI code page.** An Altium file authored on a non-1252 locale writes
      PATTERN, `Library/Data` and `SectionKeys` in that locale's code page (issue #507's AD21
      library: GBK bytes `A3 A8` for `（`) while the CFB storage name carries the true UTF-16.
      The reader decodes those bytes as Windows-1252, so such a name shows as `£¨` in JSON and
      must be addressed that way; the file itself stays intact since the same bytes go back and
      the storage name is preserved. The clean fix detects the code page (the storage name is
      the oracle: the encoding whose bytes of it equal PATTERN's), records it on the library,
      and decodes/encodes every text field through it. Needs `encoding_rs` labels for GBK,
      Big5, Shift_JIS, EUC-KR and the 125x pages, and a fixture from a non-1252 Altium.
- [ ] **Drop the `cfb` git pin** (`[patch.crates-io]`, rev `8c1ec76`) as soon as rust-cfb
      publishes a release newer than v0.14.0 — check
      [rust-cfb releases](https://github.com/mdsteele/rust-cfb/releases) at session start.

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
