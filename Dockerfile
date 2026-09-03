# syntax=docker/dockerfile:1

# Runs altium-designer-mcp in a container: the same `--locked` release build
# the published binaries come from, on a minimal Debian base. Altium itself
# never runs here — the container reads and writes library files under
# /libraries, the only path it may touch, so mount your library folder there:
#
#     docker build -t altium-designer-mcp .
#     docker run -i --rm -v /path/to/libraries:/libraries altium-designer-mcp
#
# See README.md § Installation. Both stages use the same Debian release so the
# binary links against the glibc it will run on.

# ---- builder ---------------------------------------------------------------
# Pinned to the repository's Rust toolchain (rust-toolchain.toml); the tag is
# held to it by .github/scripts/check-toolchain-pin.sh.
FROM rust:1.98.0-slim-bookworm AS builder

# git + CA certificates: Cargo.toml patches `cfb` to a git revision, which
# cargo fetches over HTTPS.
RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

# --locked: the committed Cargo.lock, exactly as CI and the release build use it.
RUN cargo build --locked --release --bin altium-designer-mcp

# ---- runtime ---------------------------------------------------------------
FROM debian:bookworm-slim

LABEL org.opencontainers.image.title="altium-designer-mcp" \
      org.opencontainers.image.description="MCP server for AI-assisted Altium Designer component library management" \
      org.opencontainers.image.source="https://github.com/embedded-society/altium-designer-mcp" \
      org.opencontainers.image.licenses="GPL-3.0-or-later"

# An unprivileged user owning the one folder the server is allowed to use.
RUN useradd --system --create-home --uid 10001 mcp \
    && mkdir -p /libraries \
    && chown mcp:mcp /libraries

COPY --from=builder /src/target/release/altium-designer-mcp /usr/local/bin/altium-designer-mcp

USER mcp
VOLUME ["/libraries"]

# MCP over stdio; the mounted folder is the whole allow-list.
ENTRYPOINT ["altium-designer-mcp"]
CMD ["--allow", "/libraries"]
