#!/usr/bin/env bash
# Reviews a release (docs/RELEASING.md step 8) with hard checks and no
# publishing. Ends with "REVIEW PASS <tag>" and leaves the marker the publish
# script requires, or "REVIEW FAIL: <reason>".
#
#   bash scripts/release/review-draft.sh vX.Y.Z
#
# Checks: the release exists with exactly the six expected assets, every line
# of SHA256SUMS.txt, a build-provenance attestation for each of the six files,
# identical .mcpb and .dxt, the bundle manifest's version and its three
# binaries, the archives' contents, this platform's binary (--version and an
# MCP initialize + tools/list handshake with the tagged source's tool count),
# and the release notes against the CHANGELOG section of the tag.
set -u

TAG="${1:-}"
[ -n "$TAG" ] || { echo "usage: $0 vX.Y.Z"; exit 2; }
VERSION="${TAG#v}"

fail() { echo "REVIEW FAIL: $*"; rm -f "$MARKER"; exit 1; }
retry() { local n=$1; shift; local i; for _ in $(seq 1 "$n"); do "$@" && return 0; sleep 20; done; return 1; }

REPO_DIR="$(git rev-parse --show-toplevel 2>/dev/null)" || { echo "REVIEW FAIL: not inside the repository"; exit 1; }
REPO="$(gh repo view --json nameWithOwner --jq .nameWithOwner)" || { echo "REVIEW FAIL: gh cannot see the repository"; exit 1; }
MARKER="${TMPDIR:-/tmp}/altium-release-review-$TAG.txt"
WORK="$(mktemp -d)"
# The first python that actually runs: on Windows, `python3` may be the Store
# stub that only prints an install hint.
PY=""
for candidate in python3 python; do
    if "$candidate" -c 'import sys' >/dev/null 2>&1; then PY="$candidate"; break; fi
done
[ -n "$PY" ] || fail "python is needed for the handshake and notes checks"
WANT="SHA256SUMS.txt altium-designer-mcp-linux-x86_64.tar.gz altium-designer-mcp-macos-aarch64.tar.gz altium-designer-mcp-windows-x86_64.zip altium-designer-mcp.dxt altium-designer-mcp.mcpb"

rm -f "$MARKER"
cd "$REPO_DIR" || fail "cannot enter the repository"

# The release, its state, and its assets — every one must be fully uploaded.
info="$(gh api "repos/$REPO/releases?per_page=20" --jq "[.[] | select(.tag_name==\"$TAG\")] | if length == 1 then .[0] | \"\\(.id) \\(.draft) \" + ([.assets[] | select(.state==\"uploaded\") | .name] | sort | join(\",\")) else \"count=\\(length)\" end")" || fail "cannot list releases"
id="$(echo "$info" | cut -d' ' -f1)"
draft="$(echo "$info" | cut -d' ' -f2)"
names="$(echo "$info" | cut -d' ' -f3)"
case "$draft" in
    true) echo "draft release id=$id" ;;
    false) echo "published release id=$id (reviewing it after the fact)" ;;
    *) fail "no single release for $TAG ($info)" ;;
esac
[ "$names" = "$(echo "$WANT" | tr ' ' '\n' | sort | paste -sd, -)" ] || fail "assets are not exactly the six expected: $names"
echo "the six expected assets are uploaded"

cd "$WORK" || fail "cannot enter $WORK"
retry 3 gh release download "$TAG" --repo "$REPO" --dir . --clobber >/dev/null 2>&1 || fail "download failed"

# Checksums: every archive listed once, every line OK.
[ "$(wc -l < SHA256SUMS.txt)" = "5" ] || fail "SHA256SUMS.txt does not list exactly five files"
sha256sum -c --strict SHA256SUMS.txt || fail "checksum mismatch"
echo "checksums ok"

# Provenance: every asset, the checksum file included. A verify can fail on
# a transient API error, so each is retried before it counts.
for f in $WANT; do
    retry 3 gh attestation verify "$f" --repo "$REPO" >/dev/null 2>&1 || fail "attestation does not verify for $f"
done
echo "attestations ok for all six assets"

cmp -s altium-designer-mcp.mcpb altium-designer-mcp.dxt || fail ".mcpb and .dxt differ"
echo ".mcpb and .dxt identical"

mkdir -p bundle && (cd bundle && unzip -q ../altium-designer-mcp.mcpb) || fail "bundle does not unzip"
mv="$("$PY" -c "import json; print(json.load(open('bundle/manifest.json'))['version'])")" || fail "bundle manifest unreadable"
[ "$mv" = "$VERSION" ] || fail "bundle manifest version is $mv, not $VERSION"
for b in server/darwin/altium-designer-mcp server/linux/altium-designer-mcp server/win32/altium-designer-mcp.exe; do
    [ -s "bundle/$b" ] || fail "bundle lacks $b"
done
echo "bundle manifest $mv with three binaries"

for a in altium-designer-mcp-linux-x86_64.tar.gz altium-designer-mcp-macos-aarch64.tar.gz; do
    tar -tzf "$a" | grep -qx "altium-designer-mcp" || fail "$a lacks the binary"
    tar -tzf "$a" | grep -qx "docs/TOOLS.md" || fail "$a lacks docs/TOOLS.md"
done
mkdir -p win && (cd win && unzip -q ../altium-designer-mcp-windows-x86_64.zip) || fail "windows archive does not unzip"
[ -s win/altium-designer-mcp.exe ] || fail "windows archive lacks the binary"
echo "archives carry the binary and docs"

# This platform's binary: version, then a real MCP handshake.
case "$(uname -s)" in
    Linux) mkdir -p run && tar -xzf altium-designer-mcp-linux-x86_64.tar.gz -C run && BIN=run/altium-designer-mcp ;;
    Darwin) mkdir -p run && tar -xzf altium-designer-mcp-macos-aarch64.tar.gz -C run && BIN=run/altium-designer-mcp ;;
    *) BIN=win/altium-designer-mcp.exe ;;
esac
v="$("$BIN" --version 2>&1)"
[ "$v" = "altium-designer-mcp $VERSION" ] || fail "binary reports '$v'"
tools_expected="$(cd "$REPO_DIR" && git show "$TAG:src/mcp/tool_definitions.rs" | grep -c 'name: "[a-z_]*".to_string()')" || fail "cannot count the tagged source's tools"
mkdir -p libs
hs="$(printf '%s\n%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"review","version":"0"}}}' \
    '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
    | "$BIN" --allow ./libs 2>/dev/null | "$PY" -c '
import json, sys
ver = tools = None
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    d = json.loads(line)
    if d.get("id") == 1:
        ver = d["result"]["serverInfo"]["version"]
    if d.get("id") == 2:
        tools = len(d["result"]["tools"])
print(ver, tools)
')"
[ "$hs" = "$VERSION $tools_expected" ] || fail "handshake reported '$hs' (want '$VERSION $tools_expected')"
echo "binary $v; handshake ok, $tools_expected tools"

# Release notes: the CHANGELOG section for this version, then the fixed
# install-and-verify block the workflow appends after a `---`.
gh release view "$TAG" --repo "$REPO" --json body --jq .body > notes-body.md 2>/dev/null || fail "cannot read the release notes"
(cd "$REPO_DIR" && git show "$TAG:CHANGELOG.md") | awk -v h="## [$VERSION]" 'index($0, h) == 1 {f=1; next} /^## \[/ {f=0} f' > notes-section.md
notes="$("$PY" - notes-body.md notes-section.md <<'PY'
import sys
body = open(sys.argv[1], encoding="utf-8").read().replace("\r", "")
section = open(sys.argv[2], encoding="utf-8").read().replace("\r", "")
marker = "\n---\n\n## Claude Desktop one-click install\n"
at = body.find(marker)
if at < 0:
    print("the appended install block is missing")
    sys.exit()
if "## Verifying this download" not in body[at:]:
    print("the appended verification section is missing")
    sys.exit()

def norm(text):
    lines = [line.rstrip() for line in text.split("\n")]
    while lines and not lines[0]:
        lines.pop(0)
    while lines and not lines[-1]:
        lines.pop()
    return lines

if not norm(section):
    print("the CHANGELOG section is empty")
elif norm(body[:at]) != norm(section):
    print("the notes differ from the CHANGELOG section")
else:
    print("MATCH")
PY
)"
[ "$notes" = "MATCH" ] || fail "release notes: $notes"
echo "release notes equal the CHANGELOG $VERSION section, followed by the install and verification block"

{ echo "id=$id"; sha256sum $WANT | sort; } > "$MARKER"
echo "REVIEW PASS $TAG (marker: $MARKER)"
