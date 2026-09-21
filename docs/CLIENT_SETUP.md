# Connecting an AI client

How to wire `altium-designer-mcp` into every MCP-capable assistant we know of. Every
client below needs the same two facts — **where the binary is** and **where your config
file is** — and differs only in where that pair is written down.

<!-- markdownlint-disable MD013 -->

## Before you start

1. **Install the binary** somewhere permanent and note its absolute path. The release
   bundle's `README.md` covers this; from source it is `target/release/altium-designer-mcp`
   (`.exe` on Windows).
2. **Write a config file** listing the folders holding your libraries (start from the
   bundled `example-config.json`):

   ```json
   {
       "allowed_paths": ["C:\\Users\\you\\Documents\\Altium\\Libraries"]
   }
   ```

   The server refuses to touch anything outside `allowed_paths`.
3. **Check the binary runs** before involving any client:

   ```bash
   altium-designer-mcp --version
   ```

The server speaks MCP over **stdio**. Point it at your config file with its single
positional argument — or skip the file entirely and grant folders directly with
`--allow <DIR>` (repeatable; every other setting then takes its default). Throughout
this page:

| Placeholder | Linux / macOS example | Windows example (as written in JSON) |
|-------------|-----------------------|--------------------------------------|
| `<binary>` | `/usr/local/bin/altium-designer-mcp` | `C:\\Users\\you\\AppData\\Local\\Programs\\altium-designer-mcp\\altium-designer-mcp.exe` |
| `<config>` | `/home/you/.altium-designer-mcp/config.json` | `C:\\Users\\you\\.altium-designer-mcp\\config.json` |

Always use **absolute paths**. In JSON on Windows, every backslash is doubled (`\\`);
in YAML and TOML a backslash inside double quotes also needs doubling, or use forward
slashes (`C:/Users/you/...`), which Windows accepts everywhere.

### The standard block

Most clients read the same `mcpServers` shape. This is the block referred to below:

```json
{
    "mcpServers": {
        "altium": {
            "command": "<binary>",
            "args": ["<config>"]
        }
    }
}
```

If the client's file already has an `mcpServers` object, add the `"altium"` entry inside
it rather than pasting a second object.

## Quick reference

| Client | Where the config lives | Shape |
|--------|------------------------|-------|
| [Claude Code](#claude-code) | `claude mcp add`, or `.mcp.json` in the project | standard block |
| [Claude Desktop](#claude-desktop) | `claude_desktop_config.json` (Settings → Developer → Edit Config) | standard block |
| [Google Antigravity](#google-antigravity) | `~/.gemini/config/mcp_config.json` or `.agents/mcp_config.json` | standard block |
| [Cursor](#cursor) | `~/.cursor/mcp.json` or `.cursor/mcp.json` | standard block |
| [VS Code (Copilot)](#vs-code-github-copilot) | `.vscode/mcp.json` or user `mcp.json` | `servers` key |
| [GitHub Copilot CLI](#github-copilot-cli) | `~/.copilot/mcp-config.json` or `/mcp add` | standard block + `type: local` |
| [Windsurf](#windsurf) | `~/.codeium/windsurf/mcp_config.json` | standard block |
| [Cline](#cline) | MCP Servers panel → Configure, or `~/.cline/mcp.json` (CLI) | standard block |
| [Roo Code](#roo-code) | `mcp_settings.json` (global) or `.roo/mcp.json` | standard block |
| [Kiro](#kiro) | `~/.kiro/settings/mcp.json` or `.kiro/settings/mcp.json` | standard block |
| [JetBrains AI Assistant](#jetbrains-ai-assistant) | Settings → Tools → AI Assistant → MCP, "As JSON" | standard block |
| [Zed](#zed) | `settings.json` → `context_servers` | own key |
| [Gemini CLI](#gemini-cli) | `gemini mcp add`, or `~/.gemini/settings.json` | standard block |
| [OpenAI Codex CLI](#openai-codex-cli) | `codex mcp add`, or `~/.codex/config.toml` | TOML |
| [Continue](#continue) | `~/.continue/config.yaml` or `.continue/mcpServers/*.yaml` | YAML |
| [Goose](#goose) | `~/.config/goose/config.yaml` | YAML (`extensions`) |
| [OpenCode](#opencode) | `opencode.json` | `mcp` key, command as array |
| [Anything else](#any-other-mcp-client) | its stdio server settings | `command` + `args` |

## Claude Code

The CLI does the editing for you. Everything after `--` is the command that starts the
server:

```bash
claude mcp add altium -- /usr/local/bin/altium-designer-mcp /home/you/.altium-designer-mcp/config.json
```

Windows (PowerShell):

```powershell
claude mcp add altium -- "$env:LOCALAPPDATA\Programs\altium-designer-mcp\altium-designer-mcp.exe" "$env:USERPROFILE\.altium-designer-mcp\config.json"
```

Add `--scope user` before the name to make it available in every project (the default,
*local*, is this project only on this machine). To share with a team instead, commit the
standard block as `.mcp.json` at the project root — Claude Code asks each person to
approve it on first use.

Check: `claude mcp list` shows `altium` with `✔ Connected`; inside a session, `/mcp` lists
its 34 tools. See [`USAGE.md`](USAGE.md) for worked examples.

## Claude Desktop

macOS and Windows only (there is no Linux build of Claude Desktop).

**One-click extension (recommended):** download `altium-designer-mcp.mcpb` from the
[latest release](https://github.com/embedded-society/altium-designer-mcp/releases/latest)
— on an older Claude Desktop build that does not accept it, take the identical bundle
under the format's old name, `altium-designer-mcp.dxt`. Then Settings → **Extensions** →
**Advanced settings** → **Install Extension…**, pick the file, and choose your library
folders when prompted. That folder list is the `allowed_paths` security boundary, so
keep it narrow. No binary to place, no config file, no JSON: one bundle carries the
server for macOS, Windows and Linux, and the picked folders reach it as `--allow`
grants. (The extension route follows the pattern of
[coffeenmusic/altium-mcp](https://github.com/coffeenmusic/altium-mcp).)

**Manual config** — wiring the binary by hand instead:

1. Claude menu → **Settings…** → **Developer** → **Edit Config**. This opens (or creates)
   - macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
   - Windows: `%APPDATA%\Claude\claude_desktop_config.json`
2. Paste the standard block (Windows paths with doubled backslashes).
3. Quit Claude Desktop completely and start it again.

Check: the **+** ("Add files, connectors, and more") button below the message box →
**Connectors** lists `altium`.

If it does not appear, the logs say why: `~/Library/Logs/Claude/mcp-server-altium.log`
(macOS) or `%APPDATA%\Claude\logs\mcp-server-altium.log` (Windows) carry the server's
stderr, and `mcp.log` beside them the connection attempts.

## Google Antigravity

Settings (bottom left) → **Customizations** → **Installed MCP Servers** → **Add MCP**, or
edit the file directly:

- Linux / macOS: `~/.gemini/config/mcp_config.json`
- Windows: `%USERPROFILE%\.gemini\config\mcp_config.json`
- One project only: `.agents/mcp_config.json` in the workspace root

Paste the standard block (`"disabled": true` parks an entry without deleting it). The file
is strict JSON (no comments). Reload Antigravity after editing it; the MCP store / server
list must show `altium` enabled. The official
[Antigravity MCP docs](https://antigravity.google/docs/mcp) carry the current UI.

## Cursor

Global: `~/.cursor/mcp.json` (every project). Project: `.cursor/mcp.json` in the project
root. Both take the standard block with one addition — Cursor's docs mark
`"type": "stdio"` as required for a local server:

```json
{
    "mcpServers": {
        "altium": {
            "type": "stdio",
            "command": "<binary>",
            "args": ["<config>"]
        }
    }
}
```

Check: Cursor Settings → **MCP** shows `altium` with a green dot and its tool list.

## VS Code (GitHub Copilot)

Workspace: `.vscode/mcp.json`. User-wide: run **MCP: Open User Configuration** from the
Command Palette. VS Code uses a `servers` key rather than `mcpServers`:

```json
{
    "servers": {
        "altium": {
            "type": "stdio",
            "command": "<binary>",
            "args": ["<config>"]
        }
    }
}
```

**MCP: Add Server** in the Command Palette walks through the same fields. The tools
appear in Copilot Chat's **Agent** mode under the tools picker.

## GitHub Copilot CLI

Start `copilot`, enter `/mcp add`, and fill in the form: name `altium`, type
**Local/STDIO**, the command *including its argument* (`<binary> <config>`), no
environment variables, tools `*` — or `copilot mcp add` from a terminal does the same.
Or edit `~/.copilot/mcp-config.json`:

```json
{
    "mcpServers": {
        "altium": {
            "type": "local",
            "command": "<binary>",
            "args": ["<config>"],
            "env": {},
            "tools": ["*"]
        }
    }
}
```

A `.mcp.json` or `.github/mcp.json` in the repository is also read, after you confirm
folder trust on first launch.

## Windsurf

Cascade panel → **MCPs** icon, or Settings → Cascade → **MCP Servers**; the file is
`~/.codeium/windsurf/mcp_config.json` and takes the standard block. Cascade caps the
total at 100 tools across all servers — this server's 34 fit comfortably.

## Cline

In the IDE extension: **MCP Servers** icon in the toolbar → **Configure** tab →
**Configure MCP Servers**, which opens the settings JSON. Cline's CLI reads
`~/.cline/mcp.json`. Either way, the standard block, optionally with Cline's extras:

```json
{
    "mcpServers": {
        "altium": {
            "command": "<binary>",
            "args": ["<config>"],
            "disabled": false,
            "autoApprove": []
        }
    }
}
```

## Roo Code

Settings icon in the Roo Code pane → **Edit Global MCP** (`mcp_settings.json`) or
**Edit Project MCP** (`.roo/mcp.json`). Standard block; Roo's `alwaysAllow` array lists
tools that skip the approval prompt — leave it empty for a file-writing server until you
trust the workflow. A project entry overrides a global one of the same name.

## Kiro

User level `~/.kiro/settings/mcp.json`, workspace level `.kiro/settings/mcp.json`
(Command Palette → **Kiro: Open workspace MCP config (JSON)**); workspace wins. Standard
block, with Kiro's optional `disabled` / `autoApprove` fields. Kiro CLI (formerly Amazon
Q Developer CLI — migrated automatically in November 2025) reads the same files.

## JetBrains AI Assistant

**Settings → Tools → AI Assistant → Model Context Protocol (MCP)** → **+** → either fill
in the stdio form (command `<binary>`, argument `<config>`) or choose **As JSON** and paste
the standard block. Pick *Global* or *Project* as the server level. (IntelliJ, PyCharm,
CLion, Rider and the rest share this dialog.)

## Zed

Settings → AI → **MCP Servers** → **Add Server** → **Add Local Server**, or run
`zed: open settings file` and add:

```json
{
    "context_servers": {
        "altium": {
            "command": "<binary>",
            "args": ["<config>"],
            "env": {}
        }
    }
}
```

## Gemini CLI

```bash
gemini mcp add --scope user altium /usr/local/bin/altium-designer-mcp /home/you/.altium-designer-mcp/config.json
```

(omit `--scope user` for a project-only entry). Or add the standard block to
`~/.gemini/settings.json` (user) or `.gemini/settings.json` (project). `gemini mcp list`
confirms the connection.

## OpenAI Codex CLI

```bash
codex mcp add altium -- /usr/local/bin/altium-designer-mcp /home/you/.altium-designer-mcp/config.json
```

Or in `~/.codex/config.toml` (or a trusted project's `.codex/config.toml`):

```toml
[mcp_servers.altium]
command = "/usr/local/bin/altium-designer-mcp"
args = ["/home/you/.altium-designer-mcp/config.json"]
```

On Windows, write the paths with forward slashes or doubled backslashes inside the
quotes. `codex mcp list` shows the result.

## Continue

Continue's configuration is YAML. Either in `~/.continue/config.yaml`:

```yaml
mcpServers:
  - name: altium
    type: stdio
    command: /usr/local/bin/altium-designer-mcp
    args:
      - /home/you/.altium-designer-mcp/config.json
```

or as its own file under `.continue/mcpServers/` in the workspace — that folder also
accepts a copied `mcpServers` JSON block from another client.

## Goose

`~/.config/goose/config.yaml` (Linux / macOS) or `%APPDATA%\Block\goose\config\config.yaml`
(Windows), under `extensions`:

```yaml
extensions:
  altium:
    type: stdio
    name: altium
    enabled: true
    cmd: /usr/local/bin/altium-designer-mcp
    args: ["/home/you/.altium-designer-mcp/config.json"]
    envs: {}
    env_keys: []
    timeout: 300
```

Restart Goose after editing; `goose configure` → **Add Extension** → **Command-line
Extension** offers the same through a menu.

## OpenCode

`opencode.json` in the workspace. The command is a **single array** holding the binary
and its argument:

```json
{
    "$schema": "https://opencode.ai/config.json",
    "mcp": {
        "altium": {
            "type": "local",
            "command": ["<binary>", "<config>"],
            "enabled": true
        }
    }
}
```

## Any other MCP client

If it supports **local (stdio) servers**, it will ask for a command and arguments:
command `<binary>`, one argument `<config>`, no environment variables. The standard block
is the de-facto interchange format — many clients import it directly.

## HTTP transport

Every client above starts the server itself and talks to it over stdio. A client that
prefers a URL — or several clients sharing one server — can use the MCP Streamable HTTP
transport instead:

```bash
altium-designer-mcp --allow ~/Altium/Libraries --http 127.0.0.1:8080
```

The endpoint is `http://127.0.0.1:8080/mcp`. For example, in Claude Code:

```bash
claude mcp add --transport http altium http://127.0.0.1:8080/mcp
```

and in VS Code's `mcp.json`:

```json
{ "servers": { "altium": { "type": "http", "url": "http://127.0.0.1:8080/mcp" } } }
```

What the transport does to stay safe on a machine where it writes files:

- **Loopback by default.** A non-loopback address (`0.0.0.0:8080`, a LAN address) is
  refused unless a bearer token is set in the `ALTIUM_DESIGNER_MCP_HTTP_TOKEN`
  environment variable; every request must then send `Authorization: Bearer <token>`
  (Claude Code: `--header "Authorization: Bearer <token>"`; VS Code: a `headers`
  entry). The token is never taken from the command line, which other users of the
  machine can list. Put a TLS reverse proxy in front of anything that leaves the
  machine: the server itself speaks plain HTTP.
- **Browser origins.** A request carrying an `Origin` header is refused unless the
  origin is local (`localhost`, `127.0.0.1`, `[::1]`, any port) or listed with
  `--http-allow-origin`. That is what stops a web page from reaching the server through
  DNS rebinding.
- **Sessions and limits.** `initialize` opens a session (`Mcp-Session-Id`); `DELETE
  /mcp` ends it. Messages are capped at 32 MiB, handled one at a time so two clients
  never write the same library at once, and every session draws on the one rate
  limiter in the config. The server sends no event stream, so `GET /mcp` answers 405.

### Web-only assistants

claude.ai in the browser and ChatGPT connect only to *public* HTTPS MCP servers, and
they authenticate with OAuth or not at all. The HTTP transport authenticates with a
static bearer token and does not implement OAuth, so it cannot yet be offered to them
safely: exposing it to the internet without a token would let anyone who finds the URL
write your libraries. Use one of the desktop, CLI or IDE clients above until OAuth
lands.

## Troubleshooting

| Symptom | Check |
|---------|-------|
| Server missing from the client's list | The config file is valid JSON/YAML/TOML (a trailing comma is the usual culprit); paths are absolute; the client was fully restarted. |
| "Failed to connect" / spawn error | Run `<binary> <config>` in a terminal: it should sit silently waiting for input (press Ctrl+C to stop). If it prints an error, fix that first — a typo in `allowed_paths`, or a missing config file. |
| Windows: path errors | Every `\` in JSON must be `\\`; or use forward slashes. `%APPDATA%`-style variables are not expanded inside JSON — write the real path. |
| Windows SmartScreen / macOS Gatekeeper blocks the first run | The binaries are not code-signed. Windows: **More info → Run anyway**. macOS: right-click → **Open** once, or `xattr -d com.apple.quarantine <binary>`. The release's signed provenance attestation is the stronger check — see the release notes. |
| Tools are listed but every call fails with a path error | The library file is outside `allowed_paths`. Add its folder to the config (or, for the Claude Desktop extension, to its **Library folders** setting) and restart the client. |
| Claude Desktop shows nothing | Read `mcp-server-altium.log` (locations above) — it holds the server's own error output. |
