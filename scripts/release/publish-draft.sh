#!/usr/bin/env bash
# Publishes a reviewed draft (docs/RELEASING.md step 9). Refuses unless
# review-draft.sh passed for this exact draft with these exact asset digests,
# publishes as the latest release (a pre-release tag with a suffix is marked
# --prerelease instead), then waits for the MCP Registry to list the version,
# re-dispatching registry-publish.yml once if its run failed. Every write is
# guarded, so a retried call cannot publish twice. Ends with "PUBLISH DONE" or
# "PUBLISH STOP: <reason>".
#
#   bash scripts/release/publish-draft.sh vX.Y.Z
set -u

TAG="${1:-}"
[ -n "$TAG" ] || { echo "usage: $0 vX.Y.Z"; exit 2; }
VERSION="${TAG#v}"

stop() { echo "PUBLISH STOP: $*"; exit 1; }
ts() { date -u +%H:%M:%SZ; }

REPO_DIR="$(git rev-parse --show-toplevel 2>/dev/null)" || stop "not inside the repository"
REPO="$(gh repo view --json nameWithOwner --jq .nameWithOwner)" || stop "gh cannot see the repository"
R="repos/$REPO"
MARKER="${TMPDIR:-/tmp}/altium-release-review-$TAG.txt"
cd "$REPO_DIR" || stop "cannot enter the repository"

# The review must have passed for exactly what is in the draft now.
[ -s "$MARKER" ] || stop "no review marker for $TAG; run scripts/release/review-draft.sh $TAG first"
mid="$(head -1 "$MARKER" | sed 's/^id=//')"
rel="$(gh api "$R/releases/$mid" --jq '"\(.tag_name) draft=\(.draft)"' 2>&1)" || stop "cannot read release $mid ($rel)"
case "$rel" in
    "$TAG draft=true") ;;
    "$TAG draft=false") echo "$(ts) release $mid is already public; skipping to verification" ;;
    *) stop "release $mid is not the reviewed $TAG draft ($rel)" ;;
esac

if [ "$rel" = "$TAG draft=true" ]; then
    CHECK="$(mktemp -d)"
    gh release download "$TAG" --repo "$REPO" --dir "$CHECK" --clobber >/dev/null 2>&1 || stop "cannot re-download the draft assets for the digest check"
    now="$(cd "$CHECK" && sha256sum SHA256SUMS.txt altium-designer-mcp-linux-x86_64.tar.gz altium-designer-mcp-macos-aarch64.tar.gz altium-designer-mcp-windows-x86_64.zip altium-designer-mcp.dxt altium-designer-mcp.mcpb | sort)"
    [ "$now" = "$(tail -n +2 "$MARKER")" ] || stop "draft assets changed since the review"
    echo "$(ts) draft $mid matches the reviewed digests"

    if [[ "$VERSION" == *-* ]]; then
        FLAGS=(--prerelease --latest=false)
    else
        FLAGS=(--latest)
    fi
    # Guarded retries: the state is re-read before every attempt.
    for _ in 1 2 3 4; do
        cur="$(gh api "$R/releases/$mid" --jq '.draft' 2>/dev/null)"
        [ "$cur" = "false" ] && break
        gh release edit "$TAG" --repo "$REPO" --draft=false "${FLAGS[@]}" >/dev/null 2>&1
        sleep 10
    done
fi
pub="$(gh api "$R/releases/$mid" --jq '.draft' 2>/dev/null)"
[ "$pub" = "false" ] || stop "release did not become public (draft=${pub:-unreadable})"
latest="$(gh api "$R/releases/latest" --jq .tag_name 2>/dev/null)"
echo "$(ts) published; latest release is ${latest:-unreadable}"
if [[ "$VERSION" != *-* ]] && [ "$latest" != "$TAG" ]; then
    stop "published, but the latest release is ${latest:-unreadable}, not $TAG"
fi

# The registry: publishing triggers registry-publish.yml.
# A timed-out poll is just a poll that found nothing; the loop retries it.
listed() { curl -s --max-time 20 "https://registry.modelcontextprotocol.io/v0/servers?search=altium-designer-mcp" | grep -q "\"version\":\"$VERSION\""; }
ok=no
for _ in $(seq 1 20); do
    if listed; then ok=yes; break; fi
    sleep 20
done
if [ "$ok" = no ]; then
    echo "$(ts) the registry does not list $VERSION yet; checking the registry workflow"
    last="$(gh run list --workflow registry-publish.yml --limit 1 --json status,conclusion --jq '.[0] | "\(.status) \(.conclusion // "")"' 2>/dev/null)"
    echo "  last registry-publish run: ${last:-none}"
    if [ "$last" = "completed failure" ]; then
        gh workflow run registry-publish.yml -f tag="$TAG" >/dev/null 2>&1 && echo "$(ts) registry publish re-dispatched for $TAG"
        for _ in $(seq 1 20); do
            if listed; then ok=yes; break; fi
            sleep 20
        done
    fi
fi
[ "$ok" = yes ] || stop "published, but the registry does not list $VERSION yet (check registry-publish.yml)"
echo "$(ts) the registry lists $VERSION"
echo "PUBLISH DONE $TAG — announce it (docs/RELEASING.md step 10)"
