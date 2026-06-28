# Commit to Git

Pre-commit checklist for **quilltap-v5** (the native Rust + Angular + Tauri port).
Work through every step in order. Where a step says **block the commit**, do not
proceed until the condition is satisfied or the human waives it explicitly.

## 1. Port discipline — the equivalence test is non-negotiable

If this commit ports a unit of v4 behavior (a pure function, a repo/service op,
an enclave step) into Rust, **block the commit** unless it arrives with a
matching equivalence test against the v4 oracle. An AI-heavy port of a subtle
system cannot be verified by inspection — never accept a port without its diff.

- Confirm the oracle case imports v4's **real** `lib/` code (it must not
  reimplement the behavior it's checking).
- Confirm the test lands in the right tier: (1) **exact** for pure functions
  (1e-12 floats, exact strings), (2) **structural DB diff** for repo/service ops
  (timestamps / generated UUIDs / LLM text normalized), (3) **mocked-LLM** for
  model-dependent paths.
- Carry forward v4's *why*-comments for any invariant the port relies on.

## 2. Invariants that must not move during the port

**Block the commit** if any of these changed without an explicit, agreed reason:

- **On-disk schema.** Same tables, same UUIDs. The Rust core opens the exact DB
  file v4 writes; a schema change breaks both existing instances and the oracle
  comparison.
- **The database cipher.** It is **ChaCha20 / sqleet** (SQLite3MultipleCiphers,
  utelle, matching v4's bundled 2.3.5), **not** SQLCipher. No `cipher=` pragma.
  Reject anything reaching for `rusqlite` + `bundled-sqlcipher` (AES-only — it
  returns `NotADatabase` on every real instance).
- **The open sequence.** `PRAGMA key = "x'<hex>'"` (raw-hex, KDF skipped) is the
  first and only pragma before the first read on a read-only open. No
  `journal_mode` / `foreign_keys` on a read path. (Writable path adds
  `foreign_keys = ON` + `journal_mode = TRUNCATE` — TRUNCATE, not WAL.)
- **The single-writer model.** Only the writer task holds the RW connection; a
  channel is the only mutator. Per-database partitioned apply (main /
  mount-index / llm-logs, each its own transaction), main-primary vs idempotent
  ordering, and the folder-conflict id remap are correctness, not Node
  workarounds — keep them.

## 3. Real-instance safety

If this commit touches DB-open or write paths, confirm nothing in it points a
**writable** open at live Friday data (`~/iCloud/Quilltap/Friday`). Writes
against a real instance operate on a **copy** only. The pepper is the master key
to all data — verify it isn't committed, logged, or written anywhere that syncs.

## 4. Run the differential harness — block on failure

Regenerate the oracle output from the v4 checkout, then run the Rust diff. If any
case fails, **block the commit** and fix the port (not the test) before
continuing.

```bash
# 1. oracle output from the v4 checkout (imports real lib/ code)
cd ~/source/quilltap-server
npx tsx ~/source/quilltap-v5/harness/oracle/cases/memory-weighting.ts > /tmp/oracle-weighting.ndjson
npx tsx ~/source/quilltap-v5/harness/oracle/cases/ranking-blend.ts    > /tmp/oracle-ranking.ndjson

# 2. the Rust diff (env vars point at the NDJSON; tests skip if unset)
cd ~/source/quilltap-v5
QT_ORACLE_WEIGHTING=/tmp/oracle-weighting.ndjson \
QT_ORACLE_RANKING=/tmp/oracle-ranking.ndjson \
  cargo test -p quilltap-harness
```

Add the regenerate-and-run line for any **new** oracle case this commit
introduces. Run `cargo test -p quilltap-harness` with no env vars too, so the
harness self-test (`now_constant_matches_iso`) guards the fixed clock/date math.

## 5. Build, test, and lint the whole workspace — block on failure

Run these across the tree and fix every warning and failure you find, regardless
of whether you think you caused it:

- `cargo build --workspace`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all --check` (run `cargo fmt --all` to fix)

> Native build deps: Xcode CLT (clang) + `cmake`; the DB build compiles the
> SQLite3MC amalgamation via the `cc` crate and `buildtime_bindgen` needs Clang.
> **Never present Rust as done on the strength of inspection alone** — these
> commands actually compiling and passing is the proof, especially for crypto
> and cipher code.

## 6. Version bumps

If a crate's source changed, bump that crate's `version` in its `Cargo.toml`
(patch level unless the change is larger), and keep `Cargo.lock` — which is
committed — in step (`cargo build` will update it; stage the result). Skip this
for docs-only or scaffolding-only commits that don't touch crate source.

> Don't initiate a release. This repo's release process isn't established yet —
> set it up deliberately when the time comes, don't improvise it from a commit.

## 7. Changelog — every commit, no exceptions

Update [`docs/CHANGELOG.md`](../../docs/CHANGELOG.md) for **every** commit. A
bugfix belongs in the next release section; anything else goes in the current
`-dev` section (e.g. `5.0-dev`).

**The changelog is the exception to the Quilltap writing voice.** Write entries
terse and direct in plain American English — the steampunk / Roaring-Twenties /
Wodehouse / Lemony Snicket register we use for user-facing strings does **not**
apply here.

## 8. Docs that must stay current

- If a user-facing string was added or changed, confirm it keeps v4's
  steampunk / Wodehouse / Lemony Snicket register.
- If this commit moved the port forward, update the **Status** section of
  [`CLAUDE.md`](../../CLAUDE.md) so it reflects what's actually done.
- If the change affects the boundary, the cipher, or a phase plan, keep the
  relevant doc under `docs/developer/porting/` (`overview.md`, `phase-0.md`,
  `api-boundary.md`) in sync.

## 9. Spelling — non-negotiable

The project is **"Quilltap"** (quill + tap), **never** "Quilttap". Grep the diff
if unsure.

## 10. Commit

Don't credit yourself in the commit message. Then commit.
