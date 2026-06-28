# Native Port — Phase 0: Scaffolding & the Differential Harness

> Status: exploratory plan for the next-generation native Quilltap
> (`foundry-9/quilltap`). Companion to
> [`native-core-api-boundary.md`](./native-core-api-boundary.md). Phase 0 builds
> *no product features* — it builds the monorepo skeleton, the Rust toolchain,
> and the differential test harness that every later phase depends on. Do this
> phase carefully and by hand (with agent assistance); do not bulk-delegate it.

## Why Phase 0 exists

The port is AI-heavy, and the existing TypeScript system is subtle (single-writer
invariant, per-DB partitioned apply, host-RPC ordering bridges, the memory
protection blend). Neither a human nor an agent can reliably *eyeball* a port of
that for correctness. So before porting anything, we build the thing that makes
correctness **mechanically checkable**: a harness that runs an operation through
the old TS implementation (the **oracle**) and the new Rust core against the same
real database, and asserts they produced the same result.

Phase 0 is done when: the monorepo builds end-to-end, a Rust core can open and
read a real (copied) encrypted Quilltap instance, and the harness can run one
trivial operation through both TS and Rust and assert equivalence.

---

## Hard constraint discovered during planning: the SQLCipher key path

`lib/database/meta.ts` opens the DB like this:

```ts
const keyHex = Buffer.from(sqlcipherKey, 'base64').toString('hex');
db.pragma(`key = "x'${keyHex}'"`);     // raw-hex key form
db.pragma('foreign_keys = ON');
db.pragma('journal_mode = TRUNCATE');   // NOT WAL — cloud-sync safety
```

Two things the Rust core **must** replicate exactly or it cannot open existing
databases (and the failure looks like "file is not a database", which misleads):

1. **Raw-key form.** The `x'...'` hex form tells the cipher to use the bytes as
   the key *directly, skipping the KDF*. The Rust side must do the same — pass
   the same base64→hex bytes via `PRAGMA key = "x'...'"`, **not** a passphrase
   (which would run PBKDF2 and derive a different key).
2. **Same pragmas, same order.** `key` first, before any other statement. For a
   read-only open, set **only** the key — do **not** issue `journal_mode` /
   `foreign_keys` afterward. (Mutating `journal_mode` on an existing encrypted
   file forces header writes that race the cipher context and surface as
   `NotADatabase`. The app's own `db-helpers.js openEncryptedDb` opens
   read-only, sets only the key, and verifies with a bare `SELECT 1` — mirror
   that.) The full app open adds `foreign_keys = ON` + `journal_mode = TRUNCATE`
   for its read-write connection; those belong on the writable path, not a probe.

> ### ⚠️ THE BIG ONE: the cipher is **sqleet / ChaCha20-Poly1305, NOT SQLCipher**
>
> Discovered the hard way during Phase 0 (2026-06-27). Everything in the
> codebase is *named* "sqlcipher" (`ENCRYPTION_MASTER_PEPPER`, `sqlcipherKey`,
> etc.), but the app sets **no explicit `cipher=` pragma**, so it uses the
> **default cipher of `better-sqlite3-multiple-ciphers`**, which is the Rust/Node
> binding for the C project **SQLite3MultipleCiphers** (utelle). That library's
> default scheme is **sqleet = ChaCha20-Poly1305 (AES is *not* used)**.
>
> Consequences:
>
> - **`rusqlite` with `bundled-sqlcipher` is the WRONG dependency.** Stock
>   SQLCipher is AES-only; it physically cannot read a ChaCha20/sqleet database
>   and returns `NotADatabase` no matter what `cipher_compatibility` you set.
>   The Phase-0 probe's matching "SQLCipher 4.6.1" version line was a red herring
>   — it was matching SQLCipher *to itself* on a synthetic file, never touching a
>   real sqleet database until the real-instance open failed.
> - **The Rust core must link the same C library the Node app uses:**
>   **SQLite3MultipleCiphers**, opened with its default (sqleet) cipher — no
>   `cipher=` pragma needed, exactly like the app. Route: point `libsqlite3-sys`
>   at a SQLite3MultipleCiphers amalgamation instead of the SQLCipher source, or
>   use a crate that already wraps it. **Verify by opening a real-instance copy**
>   (the only test that exercises the actual cipher), not by version-matching.
> - Pin the exact SQLite3MultipleCiphers version to match what the installed
>   `better-sqlite3-multiple-ciphers` bundles, so page format and KDF defaults
>   agree byte-for-byte.
>
> This is the single highest-risk item in the port and it is **not** closed by
> "the pepper decrypted and the file opened" — it is closed only by reading real
> rows out of a real database with the correct cipher library.
>
> **✅ RESOLVED & VERIFIED (2026-06-27).** The installed
> `better-sqlite3-multiple-ciphers@12.11.1` bundles **SQLite3 Multiple Ciphers
> 2.3.5** (on SQLite 3.53.2 in the matching amalgamation; npm `12.x` tracks
> `better-sqlite3`'s version, *not* the C library's — the C version is the one
> that matters). The `sqlite3mc-probe` crate compiles the SQLite3MC **2.3.5
> amalgamation** via `build.rs`/`cc`, links rusqlite against it
> (`default-features = false` + `buildtime_bindgen`, no bundled SQLite), and
> opened a copy of Friday read-only: **cipher = chacha20, 37 tables, 33
> characters, 20 320 memories.** No `cipher=` pragma needed — sqleet is the
> default. The real DB layer adopts this same amalgamation build.

---

## Prerequisites: setting up the Rust build environment

These are the exact, ordered steps. They assume macOS (your dev box) with
Homebrew already present; Linux/Docker notes follow each where they differ.

### P1 — Install the Rust toolchain via rustup

```bash
# Install rustup (the toolchain manager — do NOT use Homebrew's `rust`,
# it pins one version and fights cross-compilation later).
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

rustc --version && cargo --version   # sanity check
```

Pin the toolchain so every machine and CI agent builds identically. Commit a
`rust-toolchain.toml` at the repo root:

```toml
[toolchain]
channel = "1.XX.0"          # pin an explicit stable; bump deliberately
components = ["rustfmt", "clippy", "rust-src"]
profile = "default"
```

> `rust-src` is needed later for some mobile/`uniffi` workflows; harmless now.

### P2 — Native build dependencies for `bundled-sqlcipher`

`rusqlite`'s `bundled-sqlcipher-vendored-openssl` feature compiles SQLCipher
**and** OpenSSL from vendored source, which minimizes host dependencies — but it
still needs a C toolchain and a few build tools:

```bash
# macOS
xcode-select --install      # C/C++ toolchain (clang), if not already present
brew install cmake          # OpenSSL-from-source build driver
# (perl + make are provided by the Xcode CLT)
```

```bash
# Debian/Ubuntu (Docker build stage / Linux dev)
apt-get update && apt-get install -y build-essential cmake perl
```

Using `*-vendored-openssl` means you do **not** need a system OpenSSL or to set
`OPENSSL_DIR`. If you later switch to non-vendored OpenSSL for smaller builds,
add `pkg-config` and `libssl-dev` and revisit this.

### P3 — Helpful cargo tooling (optional but recommended)

```bash
cargo install cargo-nextest      # faster, better test runner — used by the harness
cargo install cargo-watch        # re-run tests on change during porting
cargo install cargo-deny         # license + advisory gate for CI
```

### P4 — Confirm the sandbox/CI can build a SQLCipher-linked binary

Before trusting the toolchain, prove the hardest dependency compiles. Create a
throwaway crate that depends on `rusqlite = { features = ["bundled-sqlcipher-vendored-openssl"] }`,
open an in-memory DB, run `PRAGMA cipher_version;`, and assert it returns a
non-empty SQLCipher version string. If this builds and runs, the rest of the
data layer will. (This is the first real harness fixture — keep it.)

### P5 — Node side unchanged

The oracle is the *current* app. Keep Node `>=24` and the existing
`npm`/Next.js/Jest setup exactly as-is in `package.json`. Phase 0 adds Rust
*alongside* it; it changes nothing about how the TS app builds or runs.

---

## Monorepo layout

You asked for frontend + backend + CLI at least. Recommended shape — a Cargo
workspace for the Rust crates, npm workspaces for the JS, coexisting at the root:

```
quilltap/                         # the next-gen repo
├── Cargo.toml                    # [workspace] — Rust members
├── rust-toolchain.toml           # pinned toolchain (P1)
├── package.json                  # npm workspaces root (frontend, oracle shims)
├── crates/
│   ├── quilltap-core/            # the engine: data layer, services, enclave step()
│   │   └── (later: the QuilltapCore trait + Request/Response/Event)
│   ├── quilltap-cli/             # `quilltap` CLI — first real consumer of the core
│   ├── quilltap-tauri/           # Tauri 2 shell (desktop now, mobile later)
│   │   └── src-tauri/
│   └── quilltap-harness/         # Phase-0 differential harness (test-only)
├── apps/
│   └── web/                      # Angular 21 SPA (frontend) — scaffolded, not built out
├── oracle/                       # thin Node bridge that drives the CURRENT TS app
│   └── (exposes "run operation X, return resulting DB state" over stdio/HTTP)
└── fixtures/                     # anonymized real-instance snapshots (git-LFS or external)
```

Notes on the choices:

- **`quilltap-core` is a library, not a binary.** The CLI, the Tauri shell, and
  the harness all depend on it. This *forces* the transport-agnostic boundary
  from day one — the core can't accidentally assume it's inside Tauri, because
  the CLI and harness link the same crate without Tauri present.
- **`quilltap-cli` is the first consumer** — port a feature, expose it in the CLI,
  diff the CLI against `npx quilltap` (which already exists and is your CLI
  oracle). The CLI is the cheapest possible transport to stand up.
- **`oracle/` wraps the existing app, it does not fork it.** It imports the real
  `lib/` repositories/services from the current codebase (path-referenced, or the
  current repo vendored as a submodule) so the oracle is *literally* today's
  behavior, not a reimplementation. If the oracle drifts from the shipping app,
  the whole harness lies — so it must call the same code the app calls.
- **`fixtures/` are real instance snapshots, anonymized.** Not synthetic. The
  subtle bugs live in real data shapes (a 20k-memory character, a wedged
  autonomous room, a deconverted doc store). Build a sanitizer that copies a real
  instance and scrubs PII while preserving structure, row counts, and edge cases.

---

## The differential harness (the heart of Phase 0)

### What it does

```
                ┌────────────────────────────────────────────┐
                │  fixture: copy of a real encrypted instance  │
                │  (same DB file handed to BOTH sides)         │
                └───────────────┬───────────────┬─────────────┘
                                │               │
          ┌─────────────────────▼──┐      ┌─────▼──────────────────────┐
          │ ORACLE (current TS app) │      │ PORT (quilltap-core, Rust) │
          │ run operation X         │      │ run operation X            │
          └─────────────┬───────────┘      └───────────┬────────────────┘
                        │                              │
                resulting DB state              resulting DB state
                        └──────────────┬───────────────┘
                                       ▼
                          ASSERT EQUIVALENT
                  (row-level diff over affected tables,
                   normalized for legitimately-nondeterministic
                   fields: timestamps, generated UUIDs, LLM text)
```

### How it asserts equivalence (the part that needs judgment)

Three equivalence tiers, because not everything is deterministic:

1. **Exact value equivalence** — pure functions (memory protection score, budget
   math). Same inputs → byte-identical outputs. No normalization. Phase 1's bread
   and butter.
2. **Structural DB equivalence** — repository / service operations. After running
   op X, diff the affected tables. **Normalize** the fields that are *allowed* to
   differ: `createdAt`/`updatedAt` timestamps, freshly-generated UUIDs (compare
   *shape and references*, not the literal id — a remap table, exactly like the
   folder-conflict remap the code already does), and any LLM-returned text.
   Everything else must match exactly.
3. **Mocked-LLM equivalence** — anything that calls a model (memory extraction,
   enclave turns, titling). Inject the *same canned LLM response* into both sides,
   then fall back to tier-2 structural equivalence on the resulting writes. You're
   testing the plumbing around the model, not the model.

The normalization rules are where bugs hide both ways — too loose and you pass
broken ports, too strict and you fail on legitimate nondeterminism. Treat the
normalizer as security-critical code: review it by hand, and when a diff fails,
the first question is always "is this a real divergence or a normalizer gap?"

### Harness deliverables for Phase 0

- A fixture sanitizer (`fixtures/sanitize.ts` or a CLI subcommand) that produces
  scrubbed instance snapshots from a real one.
- The Node oracle bridge (`oracle/`) exposing "run op X against this DB, return
  affected-table state" over a stdio/JSON protocol.
- The Rust harness crate (`quilltap-harness`) that runs the same op against
  `quilltap-core` and performs the tiered diff.
- One end-to-end proof: a trivial read op (e.g. `list characters`) run through
  both, asserted equivalent, green in CI.

---

## CI for Phase 0

Add a CI lane that, on every PR touching `crates/`:

1. Restores the pinned toolchain (`rust-toolchain.toml`).
2. `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo deny check`.
3. Builds `quilltap-core` with `bundled-sqlcipher-vendored-openssl`.
4. Runs `cargo nextest run` including the differential harness against a small
   committed fixture.
5. (Later phases) runs the growing oracle-equivalence suite.

Keep the existing TS CI untouched and running in parallel — both systems are
live during the port.

---

## Definition of done for Phase 0

- [ ] `rustup` toolchain pinned via `rust-toolchain.toml`; `cargo build` works on
      your Mac and in the Linux CI/Docker stage.
- [ ] The P4 throwaway crate proves `bundled-sqlcipher-vendored-openssl` compiles
      and `PRAGMA cipher_version` returns a version.
- [ ] `quilltap-core` opens a **copied real instance** read-only using the
      **raw-hex key form** and the TRUNCATE journal mode, and reads one table
      correctly. (This validates the hard constraint above end-to-end.)
- [ ] Monorepo skeleton exists: `crates/{quilltap-core,quilltap-cli,quilltap-tauri,quilltap-harness}`,
      `apps/web` (Angular scaffold), `oracle/`, `fixtures/`.
- [ ] Fixture sanitizer produces at least one scrubbed snapshot.
- [ ] Oracle bridge runs one operation through the current TS app and returns DB
      state.
- [ ] Differential harness runs one operation through both sides and asserts
      equivalence, green in CI.

When all boxes are checked, Phase 1 (pure-function ports) can begin — and every
function ported after this point arrives with an oracle-equivalence test, because
the machinery to write that test now exists.

---

## How to drive the agent through Phase 0 specifically

- **Steps P1–P5 and the SQLCipher-key validation: do these with me interactively,
  not via bulk delegation.** They're load-bearing and failure here is silent.
- **The fixture sanitizer and oracle bridge are good delegable units** once the
  protocol between them is specified — well-bounded, individually testable.
- **The normalizer is not delegable** — review every normalization rule yourself;
  it's the one piece whose bugs let broken ports pass.
- Never accept Rust I haven't actually compiled and run in the sandbox; the
  compile-and-test loop is what keeps an AI-driven port honest.
