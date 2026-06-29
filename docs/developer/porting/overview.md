# The Quilltap Native Port — Overview & Roadmap

> Start here. This is the map for porting Quilltap from the v4 Next.js/React app
> (the `quilltap-server` repo, mirrored under `docs/v4/`) to the native Rust +
> Angular + Tauri stack in this repo. Read alongside CLAUDE.md (the standing
> rules, loaded every turn).

## The one idea that governs everything

**v4 is the oracle.** It defines correct behavior. An AI-heavy port of a subtle
system cannot be verified by reading it — so every ported unit arrives with a
**differential equivalence test** that runs the same inputs through the real v4
code and the new Rust code and asserts they match. No port is accepted without
one. The harness that makes this mechanical is the centerpiece of Phase 0 and is
already working ([`phase-0.md`](./phase-0.md)).

## The stack (decided June 2026)

- **Core:** Rust — portable engine (`quilltap-core`): data layer, memory
  subsystem, job orchestration, the single-writer invariant.
- **DB cipher:** SQLite3MultipleCiphers (sqleet/**ChaCha20**, not SQLCipher).
- **Front end:** Angular 21+ (zoneless/signals/standalone) SPA in a Tauri 2
  webview. *(Not React.)*
- **Shell:** Tauri 2 — desktop now; iOS/Android later via Tauri-mobile or native
  shells over the same core via `uniffi`.
- **CLI:** a `quilltap` binary linking the core (v4's `npx quilltap` is its oracle).

## Phase roadmap (leaf-to-root, pure-to-stateful)

| Phase | What | Equivalence tier | Status |
|------|------|------------------|--------|
| **0** | Scaffolding, toolchain, cipher-correct DB open, differential harness | tier-1 proven | **substantially done** |
| **1** | Pure functions (scoring, sizing, remaps, budget math) | tier-1 exact | **done** |
| **2** | Data layer: repos, the writer-task model, per-DB partitioned apply | tier-2 structural DB diff | in progress — fifteen repos round-trip green (`folders`, `tags`, `text_replacement_rules`, `prompt_templates`, `conversation_annotations`, `provider_models`, `help_docs`, `roleplay_templates`, `image_profiles`, `connection_profiles`, `plugin_config`, `embedding_profiles`, `terminal_sessions`, `character_plugin_data`, `tfidf_vocabulary`) ([`phase-2-onramp.md`](./phase-2-onramp.md)); repo-by-repo |
| **3** | Services / engine: memory gate, chat orchestration, enclave `step()` | tier-2 + tier-3 mocked-LLM | not started |
| **4** | Transports (Tauri/uniffi/axum) + Angular UI | end-to-end | not started |

Each phase leans on the one below being trusted, so failures localize.

## Documents in this directory

- [`phase-0.md`](./phase-0.md) — scaffolding, the Rust build-environment steps,
  the **cipher finding** (the highest-risk fact in the port), and the harness.
- [`api-boundary.md`](./api-boundary.md) — the transport-agnostic Core API, the
  single-writer-as-ownership model, and the enclave `step()` seam. Implemented in
  Phases 3–4 but **locked in now** because it's expensive to retrofit.
- [`phase-2-onramp.md`](./phase-2-onramp.md) — the tier-2 DB-state oracle and its
  fixtures: the build that unblocks Phase 2 once the Phase-1 leaves are done.
- This overview.

## Current status (update as it moves)

Phase 0's hard, risk-bearing parts are done and verified on real data: toolchain
pinned (1.96.0), monorepo skeleton, `.dbkey` pepper decryption ported, cipher
resolved (SQLite3MC 2.3.5 / ChaCha20) and confirmed opening Friday (37 tables,
33 characters, 20 320 memories), and the differential harness proven across two
pure-function cases (numeric + string).

**Phase 1 is now complete** — every pure-function leaf is ported and tier-1
oracle-verified (crates at 0.0.18, 30 oracle cases). The full inventory lives in
the CLAUDE.md Status section.

**Phase-2 on-ramp: done.** The tier-2 DB-state oracle exists and the `folders`
repo round-trips green through it (v4 vs the Rust `quilltap-core::db` layer,
structural-diff, zero normalization). The machinery — cipher-correct writable
open, single-writer model, canonical dump, the TS oracle + harness diff — is in
place, so **Phase 2 proper is now the same mechanical loop, repo by repo**:
port the next repo, add its tier-2 case. See [`phase-2-onramp.md`](./phase-2-onramp.md).

**Phase 2 proper has started.** Fourteen repos past the pilot now round-trip
green, ported in parallel batches (agents draft each repo's own new files; the
shared `db/mod.rs` wiring + verification are serialized afterward) — two batches
of three, then a batch of five plain-base single-table repos (`plugin_config`
[UserOwned + open-JSON `config` + optional bool], `embedding_profiles` [Taggable +
nullable-REAL numbers + enum], `terminal_sessions` [nullable strings + nullable-
REAL `exitCode`], `character_plugin_data` [first open-JSON _value_ column,
`z.unknown()`], and `tfidf_vocabulary` [first repo overriding the base
`create`/`update` — `updatedAt` minted unconditionally, so placeholder-normalized;
first plain-string JSON-text columns]). The first
batch — `conversation_annotations` (a REAL-affinity unbounded-int column
`messageIndex` + a nullable UUID column), `provider_models` (two nullable REAL
number columns + boolean-default + enum TEXT columns), and `help_docs` (the
**first tier-2 BLOB column**, a Float32 embedding compared bit-exact as hex, with
a text-only update proven to leave the BLOB untouched). The second batch —
`roleplay_templates` (the **first array-of-objects JSON column**,
`renderingPatterns`, typed serde structs in schema order, plus a nullable
JSON-object column), `image_profiles` (the **Taggable lineage** — `userId` + a
JSON `tags` array — plus the first **open/arbitrary-JSON `parameters` column**),
and `connection_profiles` (the widest surface to date, ~29 columns). The open
`parameters` column carries a tracked deferred seam: multi-key objects would
diverge on key order (`serde_json::Value` sorts vs v4's insertion-order
`JSON.stringify`), so the corpora constrain it to `{}`/single-key.

Of the earlier repos, the second, `tags` (`create` + `update` + `delete`), widened the tier-2
marshaling surface past `folders`' all-strings shape: a boolean column
(`quickHide` → INTEGER 0/1), a nullable JSON-object column (`visualStyle` →
compact JSON in schema field order), and the `nameLower` derivation, plus the
`delete` op. The third, `text_replacement_rules`, is the first repo with
**conflict detection** — and so the first to need a repo-level *read*:
`create`/`update` scan existing rows and reject a duplicate
`(fromText, caseSensitive)` pair (`TrrError::Conflict`, v4's
`TextReplacementRuleConflictError`). It adds a real INTEGER number column
(`sortOrder`) and two boolean columns, and brought the canonical dump's
`js_number_to_json` refinement (integer-valued REAL → JSON integer, matching JS
`JSON.stringify`). The fourth, `prompt_templates`, banks the **first JSON array
column** (`tags` → compact JSON text) plus several nullable string columns, and
adds the **built-in read-only guard** (a read-then-guard that *suppresses*
`update`/`delete` on a built-in row, returning not-modified rather than
throwing). The on-ramp's
**generated-UUID remap + timestamp-placeholder normalization** is also built and
green (`folders_remap_tier2_equivalence`): a parent + child created with nothing
pinned, reconciled by a first-seen id remap in natural-key order (verifying the
FK relationship without literal ids) plus timestamp placeholdering — the
normalization form for repos/ops that can't take injected ids/clocks.

The **partitioned write applier** (`quilltap-core::write_apply`, the writer-task
apply path from v4's `applyWritesUnsafe`) is ported and green
(`write_apply_equivalence`): per-partition transactions, main-primary vs
idempotent ordering + failure policy, and the concurrent-folder-create reconcile.
Because the apply path is orchestration (row writes delegate to repos), it's a
tier-1-style trace differential against v4's real applier, driven through an
injected `ApplyHost` seam.

## How to resume in a fresh session

Open with: *"Continuing the quilltap-v5 native port. Read CLAUDE.md and
docs/developer/porting/overview.md. Phase 1 is done; start the Phase-2 on-ramp
per docs/developer/porting/phase-2-onramp.md."* The harness run commands are in
[`phase-0.md`](./phase-0.md).
