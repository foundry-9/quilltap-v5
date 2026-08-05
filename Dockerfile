# Quilltap — the first-class HTTP deployment (Phase-4 D1/D21, dev-grade).
#
#   docker build -t quilltap .
#   docker run -p 3000:3000 -v quilltap-data:/app/quilltap quilltap
#
# Three stages, two of them concurrent:
#
#   build — the Rust binaries (`quilltap-web` + the `quilltap` CLI). The
#     SQLite3MC amalgamation C compile is the slow layer; BuildKit cache
#     mounts keep target/ + the cargo registry warm across builds, and the
#     pinned `quilltap-sqlite3mc-sys` version means the 12 MB C compile only
#     ever happens once per cache.
#   spa — `ng build`, the Angular dist. Shares no layer with `build`, so
#     BuildKit runs the two at the same time.
#   runtime — a slim image running `quilltap-web` on 0.0.0.0, serving that
#     dist, with the conventional volume mount `/app/quilltap` as the data
#     dir and the CLI on PATH for `docker exec`.
#
# Dev-grade by decree (Phase-4 deliverable 6): no non-root user, no
# healthcheck, no multi-arch, no size-golfing, and emphatically no release
# (D21) — nothing here is published, signed, or tagged.

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
    cargo build --release -p quilltap-web -p quilltap-cli \
    && mkdir -p /out \
    && cp target/release/quilltap-web /out/quilltap-web \
    && cp target/release/quilltap /out/quilltap

# --------------------------------------------------------------------------
# SPA stage — the Angular dist (P4.5+). Independent of the Rust stage, so
# BuildKit runs the two concurrently.
# --------------------------------------------------------------------------
FROM node:24-bookworm AS spa

WORKDIR /spa

# Lockfile first: `npm ci` is the slow layer and only needs to re-run when a
# dependency actually moves, not on every source edit.
COPY apps/web/package.json apps/web/package-lock.json ./
RUN --mount=type=cache,target=/root/.npm npm ci

# The build inputs: angular.json + the tsconfigs + postcss + src/ + public/.
# (`tsconfig.app.json` includes `src/**/*.ts` only; e2e/ never enters the
# context — see .dockerignore.)
COPY apps/web/ ./

# Emits dist/quilltap/browser — the same path tauri.conf.json names as
# frontendDist, so desktop and server ship the identical bundle.
RUN npm run build

# --------------------------------------------------------------------------
# Runtime stage
# --------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# `ca-certificates` first (its postinst is the last thing in the image that
# could want perl), THEN purge `perl-base`. Debian marks it Essential, so the
# purge needs --force-remove-essential; nothing in this image is perl, and it
# carries a set of critical/high CVEs with no fix in Debian 12 — the same purge
# v4 makes in its own image (`f31598c0`). Verified on the resulting layer: perl
# is gone, /etc/ssl/certs/ca-certificates.crt survives intact (285 entries), and
# /usr/share/zoneinfo is untouched — the two things the runtime actually needs
# (TLS roots for provider calls, tzdb for QUILLTAP_TIMEZONE). Note this makes
# `apt-get install` inside a running container unreliable; that is acceptable
# for a dev-grade image and is what v4 accepts too.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && dpkg --purge --force-remove-essential perl-base \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /out/quilltap-web /usr/local/bin/quilltap-web

# The `quilltap` CLI (D12), so `docker exec -it <c> quilltap db --tables`
# reaches the running instance. Inside a live container the CLI is subject to
# the single-writer rule: its write paths refuse against a running server.
# That refusal is the design, not a defect — see docs/developer/running.md.
COPY --from=build /out/quilltap /usr/local/bin/quilltap

# The Angular dist, in the FHS share location beside the binary. Without it
# the server falls back to the two embedded placeholder pages — which is what
# every image built before this one did.
COPY --from=spa /spa/dist/quilltap/browser /usr/local/share/quilltap/spa

# The conventional container data dir (paths.rs resolves it automatically in
# a container; the env var makes it explicit for spawned terminals too).
ENV QUILLTAP_DATA_DIR=/app/quilltap

# Where the dist above lives. An env var rather than an ENTRYPOINT flag so
# `docker run … --spa-dir /elsewhere` can still override it (the flag is the
# first link of the chain in `quilltap_web::spa`) — the same reason the data
# dir is an env var. Note the layout would resolve without this: the binary
# is in /usr/local/bin, so the chain's ../share/quilltap/spa candidate finds
# it. Stating it keeps the container's behavior legible rather than clever.
ENV QUILLTAP_SPA_DIR=/usr/local/share/quilltap/spa

VOLUME /app/quilltap
EXPOSE 3000

# No TZ default is set here on purpose: an unset zone means UTC, and inventing
# one for the operator would be worse than the honest default. Pass
# `-e QUILLTAP_TIMEZONE=America/Chicago` (or `-e TZ=`) and `quilltap-web`'s
# `main` reconciles the pair before anything reads the clock — see
# `resolve_process_timezone` in crates/quilltap-web/src/main.rs and the
# timezone section of docs/developer/running.md. There is no entrypoint script
# to hang that on (v4 uses one); the binary is the entrypoint, so the binary
# does it. Zone lookup needs a tzdb on disk — the debian:bookworm-slim base
# ships `tzdata`, so do not swap for a distroless/scratch base without
# carrying /usr/share/zoneinfo along.
#
# The container binds all interfaces (D2 — the container boundary is the trust
# boundary; put a proxy in front for more).
ENTRYPOINT ["quilltap-web", "--host", "0.0.0.0", "--port", "3000"]
