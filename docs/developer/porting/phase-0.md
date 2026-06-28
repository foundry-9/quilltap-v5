# Native Port — Phase 0: Scaffolding & the Differential Harness

> Status: **substantially complete (2026-06-27).** Companion to
> [`api-boundary.md`](./api-boundary.md) and [`overview.md`](./overview.md).
> Phase 0 built *no product features* — it built the monorepo skeleton, the Rust
> toolchain, the cipher-correct DB-open path, and the differential test harness
> that every later phase depends on. The remaining items (fixture sanitizer,
> tier-2 DB-state oracle) are the on-ramp to Phase 2, not blockers for Phase 1.

## Why Phase 0 exists

The port is AI-heavy, and the existing TypeScript system is subtle (single-writer
invariant, per-DB partitioned apply, host-RPC ordering bridges, the memory
protection blend). Neither a human nor an agent can reliably *eyeball* a port of
that for correctness. So before porting anything, we built the thing that makes
correctness **mechanically checkable**: a harness that runs an operation through
the old TS implementation (the **oracle**) and the new Rust core, and asserts
they produced the same result.

Phase 0's *hard, risk-bearing* parts are done: the monorepo builds, the Rust core
opens and reads a real (copied) encrypted instance with the correct cipher, and
the harness runs operations through both TS and Rust and asserts equivalence
(two pure-function cases proven). What remains is lower-risk Phase-2 on-ramp work.

---

## ✅ The cipher question — RESOLVED & VERIFIED

> ### ⚠️ The cipher is **sqleet / ChaCha20-Poly1305, NOT SQLCipher**

Everything in v4 is *named* "sqlcipher" (`ENCRYPTION_MASTER_PEPPER`,
`sqlcipherKey`), and `docs/v4/.../DATABASE_ENCRYPTION.md` *wrongly* claims
SQLCipher — but v4 sets **no `cipher=` pragma**, so it uses the default cipher of
`better-sqlite3-multiple-ciphers` (the binding for the C project
**SQLite3MultipleCiphers**, utelle): **sqleet = ChaCha20-Poly1305**. Confirmed
empirically — `PRAGMA cipher` on Friday → `chacha20`.

- **`rusqlite` + `bundled-sqlcipher` is the WRONG dependency.** Stock SQLCipher
  is AES-only; it returns `NotADatabase` on every real Quilltap DB regardless of
  `cipher_compatibility`. (The throwaway `sqlcipher-probe` crate exists only to
  demonstrate this dead end.)
- **The real DB layer links SQLite3MultipleCiphers**, opened with its default
  (sqleet) cipher — no `cipher=` pragma needed, exactly like the app.

**Resolution (verified 2026-06-27):** the installed
`better-sqlite3-multiple-ciphers@12.11.1` bundles **SQLite3 Multiple Ciphers
2.3.5** (SQLite 3.53.2 in the matching amalgamation; npm `12.x` tracks
`better-sqlite3`'s version, *not* the C library's — the C version is the one that
matters). The `sqlite3mc-probe` crate compiles the SQLite3MC **2.3.5
amalgamation** via `build.rs`/`cc`, links rusqlite against it
(`default-features = false` + `buildtime_bindgen`, no bundled SQLite), and opened
a copy of Friday read-only: **cipher = chacha20, 37 tables, 33 characters,
20 320 memories.** The real DB layer adopts this same amalgamation build.

### Opening a database (must match v4 byte-for-byte)

`lib/database/meta.ts` (v4) keys the DB like this:

```ts
const keyHex = Buffer.from(pepper, 'base64').toString('hex');
db.pragma(`key = "x'${keyHex}'"`);     // raw-hex key form — KDF skipped
db.pragma('foreign_keys = ON');
db.pragma('journal_mode = TRUNCATE');   // NOT WAL — cloud-sync safety
```

1. **Raw-hex key.** `x'...'` tells the cipher to use the bytes directly, skipping
   its KDF (we already derived via PBKDF2 when unwrapping `.dbkey`). Pass the same
   base64→hex bytes — **not** a passphrase.
2. **Key first, key only on a read path.** On a read-only open, set *only* the
   key, then verify with `SELECT 1`. Do **not** mutate `journal_mode` /
   `foreign_keys` — doing so on an existing encrypted file forces header writes
   that race the cipher context and surface as `NotADatabase`. (Mirror v4's
   `db-helpers.js openEncryptedDb`.) The writable path adds `foreign_keys = ON`
   + `journal_mode = TRUNCATE`.

> **Two different ciphers — never conflate.** The `.dbkey` *file* wraps the pepper
> with **AES-256-GCM + PBKDF2** (ported & verified in `quilltap-core::dbkey`).
> The *databases* are **ChaCha20**. Both facts are load-bearing.

---

## Rust build environment (the steps actually taken)

macOS dev box; Linux/Docker notes inline.

### P1 — Rust toolchain via rustup ✅

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
# zsh: add `. "$HOME/.cargo/env"` to ~/.zshrc (rustup may fail to edit a stray
# ~/.tcshrc — harmless; that error doesn't block the install).
rustc --version && cargo --version
```

Pinned via committed `rust-toolchain.toml` (channel **1.96.0**):

```toml
[toolchain]
channel = "1.96.0"
components = ["rustfmt", "clippy", "rust-src"]
profile = "default"
```

> A `rust-toolchain.toml` is an **override file**: an invalid `channel` (e.g. a
> placeholder like `1.XX.0`) makes *every* `cargo` command in the tree fail until
> fixed. Use the real version.

### P2 — Native build deps ✅

The SQLite3MC amalgamation is compiled by the `cc` crate; `buildtime_bindgen`
needs Clang.

```bash
# macOS
xcode-select --install      # clang
brew install cmake
# Debian/Ubuntu
apt-get update && apt-get install -y build-essential cmake
```

### P3 — Cargo tooling (optional) — deferred

`cargo-nextest` / `cargo-watch` / `cargo-deny` recommended for the eventual CI
lane; not yet installed.

### P4 — Prove the *correct* cipher library opens a real DB ✅

The original P4 (compile `bundled-sqlcipher`, check `PRAGMA cipher_version`) was
done first via `sqlcipher-probe` — and it *passed*, which was a **red herring**:
it matched SQLCipher to itself on a synthetic file and never touched a real
sqleet DB. The real proof is `sqlite3mc-probe`: it compiles the SQLite3MC 2.3.5
amalgamation and opens a **copy of Friday**, reading real rows. That is the test
that actually validates the data-layer foundation. Keep this lesson: *version
matching is not cipher matching — only a real-instance open proves it.*

### P5 — Node side unchanged ✅

The oracle is the *current* v4 app, run with `npx tsx` from the `quilltap-server`
checkout. Phase 0 changed nothing about how v4 builds or runs.

---

## Monorepo layout (as built)

```
quilltap-v5/
├── Cargo.toml                    # [workspace] members = crates/*
├── rust-toolchain.toml           # pinned 1.96.0
├── CLAUDE.md                     # standing rules (loaded every turn)
├── crates/
│   ├── quilltap-core/            # the engine (lib). Modules: dbkey,
│   │                             #   memory_weighting. Repos/services/boundary
│   │                             #   land in later phases.
│   ├── quilltap-harness/         # differential tests vs the v4 oracle.
│   ├── sqlcipher-probe/          # THROWAWAY — proves bundled-sqlcipher is wrong.
│   └── sqlite3mc-probe/          # real-cipher probe; build.rs compiles the
│                                 #   SQLite3MC amalgamation (vendor/*.c,*.h committed).
│   (future) quilltap-cli, quilltap-tauri
├── harness/oracle/               # Node/tsx bridge driving v4's real lib/ code.
│   └── cases/{memory-weighting,ranking-blend}.ts
├── docs/
│   ├── developer/porting/        # THIS doc + api-boundary.md + overview.md
│   └── v4/                       # mirror of v4 server docs (reference only).
│   (future) apps/web/            # Angular 21 SPA.
```

Design choices that earned their place:

- **`quilltap-core` is a library, not a binary.** The future CLI, Tauri shell,
  and the harness all link it — which *forces* the transport-agnostic boundary
  (the core can't assume it's inside Tauri when the harness links it Tauri-less).
- **The oracle wraps v4, it does not fork it.** `harness/oracle/cases/*.ts`
  import the **real** `lib/memory/...` code from the `quilltap-server` checkout
  (run via `npx tsx` from there so `@/` resolves). If the oracle drifts from the
  shipping app, the harness lies — so it must call the same code the app calls.
- **The probe crates are scaffolding.** Once the real DB layer adopts the
  amalgamation build, fold `sqlite3mc-probe`'s open-real-DB logic into
  `quilltap-harness` and delete both probes.

---

## The differential harness (the heart of Phase 0) — WORKING

```
   fixed corpus / copied real instance  →  ORACLE (v4 TS)   →  result
                                        →  PORT (Rust core)  →  result
                                                  ↓
                                        ASSERT EQUIVALENT
              (tier-1 exact; tier-2 structural DB diff with normalization;
               tier-3 mocked-LLM then tier-2 on the writes)
```

Three equivalence tiers:

1. **Exact** — pure functions. Same inputs → identical outputs (1e-12 for floats,
   exact for strings). **Two cases proven:** `memory_weighting`
   (calculateEffectiveWeight, calculateProtectionScore) and `ranking-blend`
   (computeRankingBlend, defaultMinCosineForProvider, formatRelativeAge).
2. **Structural DB** — repo/service ops. Diff affected tables; **normalize**
   legitimately-nondeterministic fields (timestamps; generated UUIDs via a remap,
   like v4's folder-conflict remap; LLM text). *Not yet built — Phase-2 on-ramp.*
3. **Mocked-LLM** — model-dependent paths. Inject the same canned response both
   sides, then tier-2 on the writes. *Not yet built.*

> The normalizer (tier 2) is where bugs hide both ways — too loose passes broken
> ports, too strict fails on legitimate nondeterminism. Treat it as
> security-critical: review every rule by hand; on a failure, first ask "real
> divergence or normalizer gap?"

### Running it

```bash
# 1. generate oracle output from the v4 checkout
cd ~/source/quilltap-server
npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-weighting.ts > /tmp/oracle-weighting.ndjson
npx tsx ~/source/quilltap-v5/harness/oracle/cases/ranking-blend.ts    > /tmp/oracle-ranking.ndjson

# 2. run the Rust diff (env vars point at the NDJSON; tests skip cleanly if unset)
cd ~/source/quilltap-v5
QT_ORACLE_WEIGHTING=/tmp/oracle-weighting.ndjson \
QT_ORACLE_RANKING=/tmp/oracle-ranking.ndjson \
  cargo test -p quilltap-harness
```

`cargo test -p quilltap-harness` with no env vars runs the self-test
(`now_constant_matches_iso`), which guards the harness's own fixed clock/date
math against drift. (It has already caught one real bug — a 5-day-off `NOW_MS`
constant — before it could masquerade as a port error.)

---

## Definition of done

- [x] `rustup` toolchain pinned via `rust-toolchain.toml`; `cargo build` works.
- [x] A probe proves the **correct** cipher library compiles and opens a real DB
      (`sqlite3mc-probe` → Friday, cipher=chacha20). *(The original
      `bundled-sqlcipher` P4 passed but was a red herring — see P4 above.)*
- [x] `quilltap-core` opens a **copied real instance** read-only via the raw-hex
      key form and reads real rows (37 tables, 33 chars, 20 320 memories).
- [x] `.dbkey` pepper decryption ported & verified (`quilltap-core::dbkey`).
- [x] Workspace skeleton: `crates/{quilltap-core, quilltap-harness,
      sqlcipher-probe, sqlite3mc-probe}`, `harness/oracle/`.
- [x] Differential harness runs operations through both sides and asserts
      equivalence — two pure-function cases green.
- [ ] Fixture sanitizer produces anonymized real-instance snapshots. *(Phase-2 on-ramp.)*
- [ ] Tier-2 DB-state oracle: oracle bridge captures resulting DB state, harness
      does the normalized structural diff. *(Phase-2 on-ramp.)*
- [ ] CI lane (fmt/clippy/test against a committed fixture). *(Deferred.)*
- [ ] `crates/quilltap-cli` + `apps/web` scaffolds. *(Phase-4 setup; not required
      to start Phase 1.)*

**Phase 1 (pure-function ports) can begin now** — pure functions need only the
tier-1 harness, which is proven. The unchecked boxes above are Phase-2 on-ramp /
later-phase setup, not Phase-1 blockers.

---

## How to drive the agent

- **Toolchain + cipher/DB-open validation: do interactively, not bulk-delegated.**
  Load-bearing; failure here is silent. *(Done.)*
- **Fixture sanitizer and oracle bridge are good delegable units** once their
  protocol is specified — well-bounded, individually testable.
- **The tier-2 normalizer is NOT delegable** — review every rule yourself.
- **Never accept Rust that hasn't been compiled and run.** The sandbox has no
  Rust; the compile-and-test loop on the user's machine is the proof. Crypto and
  cipher code especially: "looks right" is not enough.
