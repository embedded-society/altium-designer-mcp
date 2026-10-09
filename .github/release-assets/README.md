# altium-designer-mcp @VERSION@

An MCP server that gives AI assistants file I/O and primitive-placement tools for Altium Designer
`.PcbLib` (footprint) and `.SchLib` (symbol) libraries. The assistant works out the geometry; this
server writes the files.

You do **not** need Rust or a build toolchain — the binary in this archive is prebuilt. The full
project, issue tracker and contribution guide live on GitHub:

<https://github.com/embedded-society/altium-designer-mcp>

## What's in this bundle

| File | Purpose |
|------|---------|
| `altium-designer-mcp` (`.exe` on Windows) | The MCP server binary. |
| `example-config.json` | Starting configuration — set `allowed_paths` to your library folders. |
| `docs/CLIENT_SETUP.md` | Setup for every MCP client: Claude Code, Claude Desktop, Google Antigravity, Cursor, VS Code, Windsurf, Cline, Roo Code, Kiro, JetBrains, Zed, Gemini CLI, Codex CLI, Continue, Goose, OpenCode and more, plus troubleshooting. |
| `docs/USAGE.md` | What to ask for once connected: example workflows, prompts and tips, identical for every client. |
| `docs/AGENT_GUIDE.md` | Invariants an AI assistant must follow (units, pin geometry, sandbox). |
| `docs/TOOLS.md` | Reference for every MCP tool. |
| `CHANGELOG.md` | Release history. |
| `LICENCE` | GNU General Public License v3.0. |

The two setup guides were written for people building from source, so ignore their "clone and build"
step — you already have the binary. Everything after it applies.

---

## Step 1 — Install the binary

Unpack this archive and move the binary somewhere permanent. It is a single self-contained
executable; there is nothing to install and no runtime to add.

**Windows**

```powershell
New-Item -ItemType Directory -Force "$env:LOCALAPPDATA\Programs\altium-designer-mcp"
Copy-Item altium-designer-mcp.exe "$env:LOCALAPPDATA\Programs\altium-designer-mcp\"
```

**Linux / macOS**

```bash
sudo install -m 755 altium-designer-mcp /usr/local/bin/altium-designer-mcp
```

On Linux the binary needs glibc 2.34 or newer: Ubuntu 22.04, Debian 12, RHEL 9 (and its free
rebuilds, AlmaLinux and Rocky Linux 9) and later (`ldd --version` shows yours).

On macOS the binary is not code-signed, so Gatekeeper blocks the first run: right-click it in Finder
and choose **Open** once, or run `xattr -d com.apple.quarantine altium-designer-mcp`.

Check it works:

```bash
altium-designer-mcp --version
```

## Step 2 — Create your configuration

Copy `example-config.json` to the default location, or keep it anywhere and pass the path as an
argument.

- **Windows:** `%USERPROFILE%\.altium-designer-mcp\config.json`
- **Linux / macOS:** `~/.altium-designer-mcp/config.json`

Edit `allowed_paths` to list the folders holding your Altium libraries:

```json
{
    "allowed_paths": [
        "C:\\Users\\you\\Documents\\Altium\\Libraries"
    ]
}
```

The server refuses to read or write anything outside those folders. That confinement is its main
safety property, so keep the list as narrow as the work allows — a single project's library folder
rather than your whole home directory.

No file at all is also fine: `altium-designer-mcp --allow <folder>` (repeatable) grants folders on
the command line and takes defaults for everything else.

## Step 3 — Connect your AI assistant

Every client below speaks the same protocol; they differ only in where the configuration lives.
Replace the paths with your own.

### Claude Code

```bash
claude mcp add altium -- /usr/local/bin/altium-designer-mcp ~/.altium-designer-mcp/config.json
```

On Windows PowerShell:

```powershell
claude mcp add altium -- "$env:LOCALAPPDATA\Programs\altium-designer-mcp\altium-designer-mcp.exe" "$env:USERPROFILE\.altium-designer-mcp\config.json"
```

Then run `claude`, and check it is loaded with `/mcp`. See `docs/USAGE.md` for worked
examples.

### Claude Desktop

Skip this archive entirely if you like: the release also ships a one-click extension,
`altium-designer-mcp.mcpb` (identical twin: `altium-designer-mcp.dxt` for older builds).
Settings → **Extensions** → **Advanced settings** → **Install Extension…**, pick the
file, choose your library folders — done. To wire this archive's binary by hand instead:

Edit `claude_desktop_config.json` — **Settings → Developer → Edit Config** opens it, or find it at
`%APPDATA%\Claude\` (Windows) or `~/Library/Application Support/Claude/` (macOS):

```json
{
    "mcpServers": {
        "altium": {
            "command": "C:\\Users\\you\\AppData\\Local\\Programs\\altium-designer-mcp\\altium-designer-mcp.exe",
            "args": ["C:\\Users\\you\\.altium-designer-mcp\\config.json"]
        }
    }
}
```

Restart Claude Desktop afterwards. On Windows, note the doubled backslashes — JSON requires them.

### Google Antigravity

Settings → Customizations → Add MCP, or edit the MCP config file directly using the same
`mcpServers` block as above. See `docs/CLIENT_SETUP.md` § Google Antigravity.

### Cursor

Create `~/.cursor/mcp.json` for all projects, or `.cursor/mcp.json` inside one project, using the
same `mcpServers` block as Claude Desktop.

### VS Code (Copilot agent mode)

Create `.mcp.json` in the workspace, or use **MCP: Add Server** from the command palette. VS Code
uses the same `command` / `args` fields, under a `servers` key rather than `mcpServers`.

### Windsurf, Cline, Roo Code, Kiro, JetBrains, Zed, Gemini CLI, Codex CLI, Continue, Goose, OpenCode…

`docs/CLIENT_SETUP.md` has a verified section for each — where its config file lives and the exact
snippet — plus a troubleshooting table.

### Any other MCP client

The `mcpServers` block above is the de-facto standard schema. If your client supports MCP over
stdio, give it the binary path as the command and the config-file path as the single argument (or
`--allow` followed by your library folders instead); check its documentation for where its
configuration file lives.

## Step 4 — Try it

Ask your assistant something like:

> Create a PcbLib at `<your library folder>` with an 0402 resistor footprint, then show me an ASCII
> preview of it.

`docs/TOOLS.md` lists every tool it can call. `docs/AGENT_GUIDE.md` is worth pasting into a project
brief — it tells the assistant the unit and geometry conventions it must respect.

---

## Verifying this download

This archive ships with a signed build-provenance attestation. To confirm it came from the project's
own release workflow and has not been tampered with (needs the [GitHub CLI](https://cli.github.com/)):

```bash
gh attestation verify <this-archive> --repo embedded-society/altium-designer-mcp
```

To confirm the download is intact, using the `SHA256SUMS.txt` published beside it on the release page:

```bash
sha256sum --check --ignore-missing SHA256SUMS.txt
```

The binaries are not code-signed, so Windows SmartScreen and macOS Gatekeeper warn on first run. The
attestation above is the stronger check either way.

## A word of warning

Generated footprints and symbols are a starting point, not a finished part. **Always check a
generated library against the manufacturer's datasheet and land-pattern drawing before committing to
fabrication.** An AI can misread a dimension table exactly the way a tired engineer can.

## Found something odd? Please tell us

This is an open project, and the fastest way it improves is people reporting what happened to them in
real work. If a footprint came out wrong, a library would not open in Altium, a tool did something
surprising, or the setup steps above did not match what you saw — that is worth an issue, even a
short one. You do not need a diagnosis or a minimal reproduction; the part number and what you
expected is plenty to start with.

Especially valuable:

- **A footprint or symbol that Altium rejects, or that looks wrong once placed.** Attach the
  generated library if you can share it.
- **Datasheet edge cases** — odd land patterns, unusual pad stacks, thermal pads, slot holes,
  anything where the geometry is not a simple rectangle grid.
- **Anything in the setup above that did not work on your machine or with your AI client.**

Pull requests are just as welcome, from a typo fix upward. The contributing guide on GitHub covers the
build and test workflow, and maintainers review everything that comes in.

- Issues: <https://github.com/embedded-society/altium-designer-mcp/issues>
- Source and contributing guide: <https://github.com/embedded-society/altium-designer-mcp>

If it saved you some time, a ⭐ on the repository helps other engineers find it.

Thank you for using altium-designer-mcp! 🙏
