#!/usr/bin/env bash
# Verifies that every `dtolnay/rust-toolchain@<sha> # vX.Y.Z [(annotation)]`
# pin in .github/workflows/ uses the same version comment, and that the
# version matches the channel field in rust-toolchain.toml. Any trailing
# annotation (e.g. " (latest stable)") is ignored — only the X.Y.Z token
# is matched. Exits non-zero on mismatch.
#
# Resolves paths relative to the repository root regardless of the
# caller's CWD (so this works whether invoked from CI's `bash
# .github/scripts/check-toolchain-pin.sh` or directly).

set -euo pipefail

# Stable LC_ALL so `sort -u` orders bytes consistently across runners.
export LC_ALL=C

# cd to the repo root (script lives at <root>/.github/scripts/...).
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd "${script_dir}/../.." && pwd)
cd "${repo_root}"

toml_channel=$(awk -F'"' '/^[[:space:]]*channel[[:space:]]*=/ {print $2; exit}' rust-toolchain.toml)

if [[ -z "$toml_channel" ]]; then
    echo "ERROR: could not extract channel from rust-toolchain.toml" >&2
    exit 1
fi

mapfile -t pin_versions < <(
    grep -h 'uses:[[:space:]]*dtolnay/rust-toolchain@' .github/workflows/*.yml \
        | sed -nE 's|.*#[[:space:]]+v?([0-9]+\.[0-9]+\.[0-9]+).*|\1|p' \
        | sort -u
)

if [[ ${#pin_versions[@]} -eq 0 ]]; then
    echo "ERROR: no dtolnay/rust-toolchain pins found in .github/workflows/" >&2
    exit 1
fi

if [[ ${#pin_versions[@]} -ne 1 ]]; then
    echo "ERROR: dtolnay/rust-toolchain pins disagree across workflows: ${pin_versions[*]}" >&2
    exit 1
fi

pin_version=${pin_versions[0]}

if [[ "$pin_version" != "$toml_channel" ]]; then
    echo "ERROR: rust-toolchain.toml channel ($toml_channel) does not match action pin (v$pin_version)" >&2
    echo "Bump both together when updating Rust." >&2
    exit 1
fi

echo "OK: Rust pinned to $pin_version (rust-toolchain.toml + dtolnay/rust-toolchain action)"
