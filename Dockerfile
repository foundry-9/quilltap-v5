# Quilltap — the first-class HTTP deployment (Phase-4 D1/D21, dev-grade).
#
#   docker build -t quilltap .
#   docker run -p 3000:3000 -v quilltap-data:/app/quilltap quilltap
#
# Multi-stage: the Rust build (the SQLite3MC amalgamation C compile is the
# slow layer — BuildKit cache mounts keep target/ + the cargo registry warm
# across builds, and the pinned `quilltap-sqlite3mc-sys` version means the
# 12 MB C compile only ever happens once per cache), then a slim runtime
# image running the `quilltap-web` binary on 0.0.0.0 with the conventional
# volume mount `/app/quilltap` as the data dir.

# --------------------------------------------------------------------------
# Build stage
# --------------------------------------------------------------------------
FROM rust:bookworm AS build

# Native build deps: clang (buildtime_bindgen) + cmake (the CLAUDE.md set).
RUN apt-get update \
    && apt-get install -y --no-install-recommends clang cmake \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src

# The pinned toolchain file first (a layer that almost never changes; rustup
# installs the pinned channel once).
COPY rust-toolchain.toml ./

# The workspace manifests + the PINNED sys crate (vendored amalgamation)
# before the fast-moving sources — these layers cache across app-code edits.
COPY Cargo.toml Cargo.lock ./
COPY crates/quilltap-sqlite3mc-sys ./crates/quilltap-sqlite3mc-sys

# The committed sample-content seed assets. `quilltap-core`'s seed_assets.rs
# reaches OUT of crates/ to include_str!/include_bytes! these at compile time
# (the P4.4u4 seed), so the workspace does not compile without them. ~140 KB
# of committed binaries that almost never change — a slow-changing layer, so
# it belongs above COPY crates.
COPY assets ./assets

# The rest of the tree (docs/harness/apps excluded via .dockerignore).
COPY crates ./crates

# BuildKit cache mounts keep the amalgamation object + dependency rlibs warm;
# the binary is copied OUT of the cache mount so the runtime stage can see it.
RUN --mount=type=cache,target=/src/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release -p quilltap-web \
    && mkdir -p /out \
    && cp target/release/quilltap-web /out/quilltap-web

# --------------------------------------------------------------------------
# Runtime stage
# --------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /out/quilltap-web /usr/local/bin/quilltap-web

# The conventional container data dir (paths.rs resolves it automatically in
# a container; the env var makes it explicit for spawned terminals too).
ENV QUILLTAP_DATA_DIR=/app/quilltap
VOLUME /app/quilltap
EXPOSE 3000

# The container binds all interfaces (D2 — the container boundary is the trust
# boundary; put a proxy in front for more).
ENTRYPOINT ["quilltap-web", "--host", "0.0.0.0", "--port", "3000"]
