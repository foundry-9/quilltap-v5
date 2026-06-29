# Phase 2 on-ramp — the tier-2 DB-state oracle and its fixtures

The thing standing between "all the pure leaves are ported" and "start porting
the data layer." Phase 1 (pure functions) is verified by **tier-1** exact
equivalence. Phase 2 (repos, the writer task, per-DB partitioned apply) mutates
an encrypted SQLite database, so it needs a different proof: **tier-2 structural
DB diff** — run the same op through v4 and the Rust port against the *same*
starting database, dump the resulting state, normalize the fields that are
legitimately nondeterministic, and assert the rest is identical.

This document scopes the two builds that make that possible. Neither blocks the
other's *design*, but the cheapest path to a working harness is one thin
vertical slice that exercises both at once (see "First slice").

## Where we are

- **Phase 0:** done. Toolchain pinned; `.dbkey` pepper decryption ported;
  cipher resolved (SQLite3MC 2.3.5 / **ChaCha20**, not SQLCipher) and confirmed
  opening real Friday data; tier-1 differential harness proven.
- **Phase 1:** complete. Every pure-function leaf is ported and tier-1
  oracle-verified (crates at **0.0.18**; 30 oracle cases). See the Status
  section of [`CLAUDE.md`](../../../CLAUDE.md) for the full inventory. The one
  standing deferral is the single ICU-collation decision, punted to the
  Phase-2/3 `localeCompare` sites — nothing shipped depends on it.
- **Next:** this on-ramp, then Phase 2 proper (repo-by-repo over the real DB).

## How tier-1 works today (the pattern tier-2 extends)

`harness/oracle/cases/<name>.ts` runs under `npx tsx` from the v4 checkout,
imports the **real** `lib/` code, runs a fixed corpus, and emits one NDJSON row
per case. A Rust integration test in `crates/quilltap-harness/tests/` reads that
NDJSON (path via a `QT_ORACLE_<NAME>` env var, skip-with-warning if unset) and
diffs field-by-field. Tier-2 keeps this shape; what changes is that the "input"
is a database and the "output" is database state.

## Deliverable A — the tier-2 DB-state oracle

**Goal.** A reusable harness that, for a given repo/service op, proves the Rust
port leaves the database in the same state v4 does.

**Shape.** Mirror the tier-1 split, made stateful:

1. **v4 side (tsx).** Open a *fresh copy* of a fixture DB (RW) under the test
   pepper → run the op(s) with fixed inputs → dump the affected tables in a
   canonical form → emit as JSON. (v4's repos take a DB handle; the oracle wires
   one to the fixture copy.)
2. **Rust side (`quilltap-harness`).** Open the *same* fixture (fresh copy) →
   run the ported repo op → dump state the same way → structural-diff against
   the v4 dump.

**The canonical dump.** Per affected table: rows sorted by a stable key (the
canonical/remapped id, or a natural key), columns in schema order, values
canonicalized — BLOBs as lowercase hex, nulls explicit, no float reformatting
(embeddings are deterministic Float32 BLOBs and compare bit-exact via the
already-ported `embedding_blob` round-trip).

**The three normalization classes** (from the differential discipline in
CLAUDE.md — normalize *only* what is legitimately nondeterministic, never to
paper over a real diff):

1. **Timestamps** (`createdAt` / `updatedAt` / `fetchedAt`): wall-clock differs
   between the two runs. Prefer **injecting a fixed clock** into both sides so
   the values match outright; where v4 won't take an injected clock, replace
   with a placeholder and separately assert ordering/monotonicity.
2. **Generated UUIDs**: both sides mint different ids. Build a **first-seen-order
   remap** (id → canonical token) consistently on both sides and compare the
   remapped structure, so foreign-key *relationships* are verified without
   pinning the literal id. (v4's folder-conflict id remap, already modeled in
   `write_partition`, is the canonical example of why this matters.)
3. **LLM text**: model-dependent output. Pure repo ops have none; this is mostly
   a tier-3 concern (inject the same canned response both sides, then tier-2 on
   the writes). Note it here so the seam is named.

**Determinism levers.** The fewer fields you normalize, the stronger the test.
Inject a fixed clock and a seeded/deterministic id generator into **both** the
v4 op and the Rust op wherever the code allows; fall back to post-hoc
normalization only where it doesn't. Document, per op, which levers were
available.

**Scope of "the op."** Test repos directly first (a single `create`/`update`/
`delete` against one table). Then graduate to the **writer-task apply path** —
the `WriteBatch` partitioned across main / mount-index / llm-logs, each in its
own transaction, with the main-primary vs idempotent ordering and the
folder-conflict remap (see `api-boundary.md` Part 2). That apply path is the
real Phase-2 correctness surface, not raw inserts.

**Acceptance.** One repo round-trips green (v4 vs Rust, structural-diff) with a
written normalization spec and a harness command wired like the tier-1 cases.

## Deliverable B — fixtures (the starting DB state)

The oracle needs a database both sides open *identically*. Two ways to get one:

- **Synthetic seed (recommended for the pilot).** A small builder / SQL seed that
  materializes a fresh DB at test time, re-keyed under a **test pepper**.
  Inspectable, diff-friendly, deterministic, and **zero leak risk** — no real
  data ever involved. The encrypted-open path is already separately proven in
  Phase 0, so the pilot doesn't need real ciphertext to be meaningful.
- **Sanitized real snapshot (the "fixture sanitizer", for breadth later).** Real
  data carries shapes and edge cases synthetic seeds miss. The sanitizer takes a
  **copy** of a real instance (never the live file), deterministically scrubs
  every free-text / identifying / private field (names, character text, message
  and document content, file blobs, legacy embedding text) while **preserving
  structural shape** (row counts, the FK graph, id relationships, enum/flag
  distributions), and re-encrypts under the **test pepper**. Output is a
  committed fixture.

**Recommendation.** Build the pilot on a synthetic seed; add the sanitizer as a
second track once the diff machinery works, to widen coverage.

**Non-negotiable safety** (these are why the sanitizer is delicate):

- Operate on a **copy** only. Never point anything at live
  `~/iCloud/Quilltap/Friday`.
- The **real pepper is the master key to everything** — it must never be read
  into, embedded in, logged by, or written alongside any fixture. Fixtures are
  keyed under a throwaway **test pepper** that is safe to commit.
- The fixture must contain **nothing** that could identify the real user after
  scrubbing. When in doubt, drop the field.
- **Schema and cipher are frozen** — fixtures use the exact tables/UUID scheme
  and the ChaCha20/sqleet open sequence v4 writes (`PRAGMA key = "x'<hex>'"`,
  no `cipher=` pragma; writable path adds `foreign_keys = ON` +
  `journal_mode = TRUNCATE`).

## Kickoff decisions (settled 2026-06-28)

1. **Pilot repo:** `folders` — a pure single-table repo (`create`/`update` just
   wrap `_create`/`_update`), and with `projectId`/`parentFolderId` null a root
   general folder has zero FK parents. (`projects` was rejected: post the
   `cutover-projects-to-store-v1` cutover its `create`/`update` route through the
   document-store overlay + mount index — not low-FK.)
2. **Fixture strategy:** synthetic seed.
3. **Clock / id injection:** inject on **both** sides via v4's existing public
   API — `_create` honors `CreateOptions.{id,createdAt,updatedAt}`, `_update`
   honors an explicit `updatedAt` in the patch. No monkeypatching of
   `generateId`/`getCurrentTimestamp`. Result: the pilot dump needs **zero**
   normalization.
4. **Fixture storage:** committed plaintext seed (`folders-tier2.json`);
   encrypted DB materialized at test time (gitignored), built by v4's own
   `ensureCollection('folders', FolderSchema)` so the DDL matches production.
5. **Apply-path scope:** repos directly first (`folders.create` + `update`); the
   `WriteBatch` partitioned-apply path is the next slice.

## Running the folders tier-2 oracle

v4's native `better-sqlite3-multiple-ciphers` is built for the Node in v4's
`.nvmrc` (currently **24.13.1**) — run the oracle under that Node, from the v4
checkout (so `@/` and npm resolution land in the server tree):

```bash
N=~/.nvm/versions/node/v24.13.1/bin
cd ~/source/quilltap-server

# 1. materialize the seed-only fixture under the test pepper
QT_FIXTURE_OUT=/tmp/qt-folders-fixture.db \
  $N/npx tsx ~/source/quilltap-v5/harness/oracle/fixtures/build-folders-fixture.ts

# 2. run the create+update op sequence on a fresh copy, dump canonical state
QT_FIXTURE_FOLDERS=/tmp/qt-folders-fixture.db \
  $N/npx tsx ~/source/quilltap-v5/harness/oracle/cases/folders-tier2.ts \
  > /tmp/oracle-folders.ndjson
```

The Rust harness then copies the **same** `/tmp/qt-folders-fixture.db`, runs the
ported `folders` ops, dumps identically, and structural-diffs against
`/tmp/oracle-folders.ndjson` (env `QT_FIXTURE_FOLDERS` + `QT_ORACLE_FOLDERS`,
skip-with-warning if unset) — the same wiring as the tier-1 cases.

## First slice (the thin vertical)

Pick **one small, low-foreign-key repo** and drive it end-to-end through both
deliverables before generalizing. Good candidates: `projects`,
`folders` (ties to the already-ported folder-conflict remap), or
`connection-profiles`. Avoid `memories`/`chats` for the pilot (embeddings, FK
fan-out). The slice:

1. A synthetic seed producing a fixture with a handful of rows in the pilot
   table (and its FK parents).
2. The pilot repo's `create` + `update` ops ported to Rust against the
   `Db`/`Writer` model.
3. The tier-2 oracle: v4 and Rust each run the same op sequence on a fresh
   fixture copy, dump + normalize + diff.
4. Green diff, with the normalization spec and harness command documented.

That proves the machinery. Everything after is repo-by-repo with the harness in
place.

## Open decisions to settle at kickoff

1. **Pilot repo** for the first slice (recommend `folders` or `projects`).
2. **Fixture strategy** for the pilot — synthetic seed (recommended) vs
   sanitized-real.
3. **Clock / id injection**: can v4's repo ops take an injected clock and
   id-generator, or must those fields be normalized post-hoc? (Decides how much
   normalization the dump needs.)
4. **Fixture storage**: a plaintext seed materialized at test time, vs a
   committed test-pepper-encrypted DB file.
5. **Apply-path scope**: test repos directly first, or go straight to the
   `WriteBatch` partitioned-apply path?

## Definition of done for the on-ramp

The tier-2 harness exists and one repo round-trips green through it, with a
documented normalization spec and a reusable fixture. At that point Phase 2 is
"port the next repo and add its tier-2 case" — the same mechanical loop Phase 1
ran on tier-1.
