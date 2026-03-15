# Clara Cerebrum — Multi-stage Docker build
#
# Build context: Development/ (parent of clara-cerebrum/ and lildaemon/)
# Usage: docker compose -f clara-cerebrum/docker/docker-compose.yml build
#
# Stages:
#   builder      — compiles all Rust binaries + SWI-Prolog + CLIPS from source
#   clara-api    — runtime image for the main API server
#   prolog-mcp   — runtime image for the Prolog MCP adapter (HTTP mode)
#   clips-mcp    — runtime image for the CLIPS MCP adapter (HTTP mode)
#   frontdesk    — runtime image for the front desk PoC demo

# ── Stage 1: Builder ──────────────────────────────────────────────────────────
FROM rust:1.82-bookworm AS builder

# Build deps for:
#   - CLIPS: cc crate compiles C source from clara-clips/clips-src/core/
#   - SWI-Prolog: CMake build from clara-prolog/swipl-src/ (SWIPL_PACKAGES=OFF)
#   - General: libssl for reqwest, pkg-config
RUN apt-get update && apt-get install -y \
    cmake \
    ninja-build \
    build-essential \
    libssl-dev \
    pkg-config \
    libgmp-dev \
    zlib1g-dev \
    libpcre2-dev \
    python3 \
    git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY clara-cerebrum/ .

# Build all four Rust binaries in one pass.
# SWI-Prolog compiles from swipl-src/ via build.rs (~30-45 min first run).
# CLIPS compiles from clips-src/core/ via cc crate (~1 min).
RUN cargo build --release \
    -p clara-api \
    -p prolog-mcp-adapter \
    -p clips-mcp-adapter \
    -p clara-frontdesk-poc

# Locate the SWI-Prolog shared library and home directory produced by the build.
# build.rs puts them under target/release/build/clara-prolog-*/out/swipl-build/
# Copy to known paths so runtime stages can COPY --from=builder.
# Uses swipl-build/src directory search (avoids fragile pipe-to-dirname pattern)
# and libswipl.so* glob to handle versioned .so names (libswipl.so.9.x.x etc).
RUN set -e && \
    SWIPL_SRC=$(find /build/target/release/build -path "*/swipl-build/src" -type d | head -1) && \
    SWIPL_HOME=$(find /build/target/release/build -path "*/swipl-build/home" -type d | head -1) && \
    if [ -z "$SWIPL_SRC" ] || [ -z "$SWIPL_HOME" ]; then \
      echo "ERROR: SWI-Prolog build artifacts not found. Build tree:" && \
      find /build/target/release/build -maxdepth 3 -type d && \
      exit 1; \
    fi && \
    mkdir -p /swipl/lib /swipl/home && \
    cp "$SWIPL_SRC"/libswipl.so* /swipl/lib/ && \
    cp -r "$SWIPL_HOME/." /swipl/home/ && \
    echo "swipl src:  $SWIPL_SRC" && \
    echo "swipl home: $SWIPL_HOME" && \
    ls -la /swipl/lib/


# ── Stage 2: clara-api runtime ────────────────────────────────────────────────
FROM debian:bookworm-slim AS clara-api

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libgmp10 \
    zlib1g \
    libpcre2-8-0 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /build/target/release/clara-api /usr/local/bin/
COPY --from=builder /swipl/lib/                     /usr/local/lib/
COPY --from=builder /swipl/home                     /opt/swipl/home
# Copy config into /app/config/ so the binary finds it at config/default.toml
COPY clara-cerebrum/config                           /app/config/

RUN ldconfig

# Models volume-mounted at runtime (860 MB — not baked into image).
# SWI_HOME_DIR overrides the compile-time constant baked in by build.rs.
ENV SWI_HOME_DIR=/opt/swipl/home
ENV LD_LIBRARY_PATH=/usr/local/lib
ENV RUST_LOG=info

EXPOSE 8080 9090
CMD ["clara-api"]


# ── Stage 3: prolog-mcp runtime ───────────────────────────────────────────────
FROM debian:bookworm-slim AS prolog-mcp

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libgmp10 \
    zlib1g \
    libpcre2-8-0 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/prolog-mcp-adapter /usr/local/bin/
COPY --from=builder /swipl/lib/                              /usr/local/lib/
COPY --from=builder /swipl/home                              /opt/swipl/home

RUN ldconfig

ENV SWI_HOME_DIR=/opt/swipl/home
ENV LD_LIBRARY_PATH=/usr/local/lib
ENV REST_API_URL=http://clara-api:8080
ENV TRANSPORT=http
ENV HTTP_PORT=1968
ENV RUST_LOG=info

EXPOSE 1968
CMD ["prolog-mcp-adapter"]


# ── Stage 4: clips-mcp runtime ────────────────────────────────────────────────
FROM debian:bookworm-slim AS clips-mcp

# CLIPS is statically linked — no extra runtime libs needed
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/clips-mcp-adapter /usr/local/bin/

ENV REST_API_URL=http://clara-api:8080
ENV TRANSPORT=http
ENV HTTP_PORT=1951
ENV RUST_LOG=info

EXPOSE 1951
CMD ["clips-mcp-adapter"]


# ── Stage 5: frontdesk runtime ────────────────────────────────────────────────
FROM debian:bookworm-slim AS frontdesk

# CLIPS statically linked; no SWI-Prolog dep in frontdesk binary
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/clara-frontdesk        /usr/local/bin/
COPY clara-cerebrum/clara-frontdesk-poc/static                    /app/static
COPY clara-cerebrum/clara-frontdesk-poc/roost                     /app/roost
COPY clara-cerebrum/clara-frontdesk-poc/config/city_of_dis.toml  /etc/frontdesk/city_of_dis.toml

ENV FRONTDESK_CONFIG=/etc/frontdesk/city_of_dis.toml
ENV RUST_LOG=info

EXPOSE 8088
CMD ["clara-frontdesk"]
