# Phase 3 kickoff — services / engine

> Start-of-phase plan for the third porting phase. Read alongside
> [`overview.md`](./overview.md) (the map), [`api-boundary.md`](./api-boundary.md)
> (the boundary + single-writer + enclave `step()` decisions this phase
> implements), and the CLAUDE.md Status section (the per-unit ledger). Phase 2 is
> complete; this doc scopes what Phase 3 builds, in what order, and how each unit
> is verified.

## Where we are (entering Phase 3)

- **Phase 0:** done. Toolchain, cipher-correct DB open, tier-1 harness.
- **Phase 1:** done. Every pure-function leaf ported, tier-1 exact-verified.
- **Phase 2:** done. **Every v4 repository** round-trips green through the tier-2
  harness (main DB + the mount-index and llm-logs sibling DBs, incl. the
  `characters` and `chats` capstones and `memories`). The partitioned write
  applier (`write_apply`) + `__finalizeFile` + post-commit effects are ported and
  trace-verified. The `upsert*` back-fill and the fixture sanitizer are done. All
  Phase-2 deferred *seams* are closed.

Phase 2 gave us a trusted data layer: repos that marshal byte-identically to v4,
and an apply path that sequences partitioned transactions correctly. Phase 3
builds the **services** that sit on top of that layer — the first code that makes
*decisions* rather than just persisting rows — and it does so leaning on Phase 2
being trusted, so any failure localizes to the new service, not the store.

## The one methodology change: tier-3 enters

Phases 1–2 were verified by tier-1 (exact) and tier-2 (structural DB diff). Phase
3's services call models (LLMs for chat/extraction, an embedding provider for the
memory gate), so they need the third tier named in the differential discipline:

> **Tier-3 (mocked-LLM):** inject the *same canned model response* on both the v4
> and Rust sides, then fall back to a tier-2 structural diff on the resulting
> writes. The model is stubbed identically, so any divergence is in *our*
> orchestration, not the model.

Tier-3 does not replace tier-2 — it feeds it. A service's model call is the only
nondeterministic input; pin it on both sides and the rest of the service becomes
a deterministic function whose DB effect we already know how to diff.

## Three things Phase 3 needs that don't exist yet

Phase 2 left three foundations unbuilt because nothing consumed them. They are the
first Phase-3 work, in this order — each is a prerequisite for the one after.

### Unit 0 — the writer-task runtime (`Db` / `Writer` actor + `ReadPool`) — ✅ DONE

**Status: ported and green** (`quilltap-core::db::runtime`). `Db` is the
`Clone + Send + Sync` handle every service holds: a per-partition read pool plus a
`tokio::mpsc::Sender` that is the only mutator. A dedicated OS thread owns the
`WriterSet` (main + optional mount-index / llm-logs RW `Writer`s) and drains the
channel serially via `blocking_recv`, so batch-apply is naturally serial. A write
is a type-erased `FnOnce(&mut WriterSet)` closure carrying its own `oneshot` reply
— services call the same typed repositories, but only ever on the writer thread
(no `{method, args}` reflection; `write_apply` stays available for the multi-DB
job path, invoked *inside* a closure). Reads go direct to a pooled read-only
connection (`PRAGMA key` first-and-only pragma per the CLAUDE.md read-path rule).
`Db::write` (async) / `write_blocking` (for the plain-`#[test]` harness) /
`read_main` / `read_mount_index` / `read_llm_logs`. Verified by four self-tests:
100 concurrent writers serialize with **no lost updates** (a read-modify-write
increment reaching the writer count), read-after-awaited-write sees committed
state, `write_blocking` commits, and a sibling-partition read on a main-only
instance is a clean typed error. `tokio` was added (`sync` in the lib — the writer
is a plain OS thread, no scheduler pulled in; `macros`/`rt-multi-thread` dev-only).

The one **Phase-2-scoped** item still open. Phase 2 built `Writer` (the
type-ownership shape) and `write_apply` (the apply logic, trace-verified). What is
*not* built is the runtime shell from [`api-boundary.md`](./api-boundary.md)
Part 2 that makes "the channel is the only mutator" a live invariant:

```rust
pub struct Writer { conn: Connection }              // ✅ exists (owns RW conn)
pub struct ReadPool { /* pool of readonly conns */ } // ⬜ build
pub struct Db {                                      // ⬜ build
    reads:  ReadPool,
    writes: mpsc::Sender<WriteBatch>,                //   the only way to mutate
}
```

- A dedicated `tokio` task **owns** the `Writer`, receives `WriteBatch` values
  over the `mpsc` channel, and drives them through the already-verified
  `write_apply`. Batch-apply is naturally serial (one task, one channel) — exactly
  the property v4's folder-conflict remap and main-primary ordering assume.
- `ReadPool` hands out readonly SQLite3MC connections (three per partition:
  main / mount-index / llm-logs) so reads go direct and never contend with the
  writer.
- `Db` is `Clone` and is what every service holds. Reads = direct pool calls;
  writes = `send(WriteBatch)`.

**Why it's Unit 0, not a Phase-2 retrofit:** it has no consumer until a service
issues a write. Building it in isolation is untestable theater; building it *with*
the memory gate (Unit 1) exercises the full path — a service issues a `WriteBatch`
→ the channel → the writer task → `write_apply` → committed rows — end to end.
Verified by the memory-gate tier-2 diff, plus a focused test that concurrent
writers serialize (no interleaved partitions) and reads see committed state.

### Unit 0.5 — the tier-3 mocked-LLM harness scaffold — ✅ DONE

**Status: complete.** The model-boundary core is ported and green
(`quilltap-core::model`), and the v4-oracle-side canned injection was exercised
end-to-end by Unit 1's memory-gate differential (see below): the oracle mocks
`generateEmbeddingForUser` to the same canned vectors the Rust
`CannedEmbeddingProvider` injects, then a tier-2 structural diff over the writes.
The mechanism note is below (the model-boundary core detail is preserved).
`model::embedding` defines `EmbeddingProvider` (the tier-3 seam — an async
`generate_embedding_for_user` mirroring v4's `generateEmbeddingForUser`, with
`EmbeddingResult` / `EmbeddingError` / `EmbeddingPriority`) plus
`CannedEmbeddingProvider`, a deterministic responder keyed by exact input text
(fixed vector, explicit failures for `SKIP_EMBEDDING_FAILED`, unregistered input
is a surfaced error — never a silent answer). The boundary is async
(`-> impl Future + Send`) and consumers take a **generic** `P: EmbeddingProvider`
(not a trait object), so no boxing and the future stays `Send`. The completion
half joins as `model::completion` when chat orchestration lands. Verified by three
self-tests. **Remaining (lands with Unit 1's differential):** the v4-oracle-side
canned injection — stubbing `generateEmbeddingForUser` to return the *same* vector
the Rust test injects — is exercised by the memory-gate tier-3 case, not in
isolation (per this unit's own acceptance: "one model-dependent service round-trips
green with a canned embedding injected identically on both sides").

Net-new harness capability; gates every model-dependent unit after it. Design:

- A **model boundary trait** in `quilltap-core` that every model call goes
  through — embeddings and completions both. Production wires the real provider
  (Phase-3 later / Phase-4); tests wire a **canned responder** keyed by call
  input, returning a fixed vector / fixed completion.
- The v4 side is stubbed to match. v4 already has the seam: `generateEmbedding*`
  and the LLM clients are injectable/mockable (the cheap-LLM-tasks tests and the
  `write-apply` jest oracle already mock module singletons — reuse that shape).
  The oracle case injects the **same** canned response the Rust test injects.
- Everything downstream of the model call is then deterministic → the existing
  tier-2 canonical-dump + normalization machinery diffs the writes.

Acceptance: one model-dependent service (the memory gate) round-trips green with a
canned embedding injected identically on both sides.

### Unit 1 — first service: the memory gate — ✅ DONE

**Status: ported and green** (`quilltap-core::services::memory_gate`), the first
tier-3 → tier-2 differential. See the detail section below; the port + its
verification are summarized in the CLAUDE.md Status section. The reusable oracle
mechanism (running v4's REAL data layer under jest with only the model call
pinned) is recorded in `[[document-store-oracle-gotchas]]`'s sibling memory
`[[jest-real-db-oracle]]`.

The recommended first real service (self-contained, high-leverage, reuses ported
leaves). See its own section below.

> **First-service choice is revisitable.** The memory gate is the recommendation
> because it is the most contained decision service. If chat orchestration turns
> out to be the priority, it can lead instead — but it is deeper (turn manager,
> streaming, the `Event` channel) and is better attempted once Units 0/0.5 are
> shaken out on something smaller.

## Unit 1 in detail — the memory gate

**v4 source:** `lib/memory/memory-gate.ts` (~623 lines). The pre-write
similarity check that turned the memory system from append-only into
append-or-reinforce.

**What it does.** Given a candidate memory, it:

1. Generates an embedding for the candidate text (the model call — the tier-3
   seam; `generateEmbeddingForUser`, with v4's one-retry-on-failure).
2. Queries the character's vector store for the top-K nearest existing memories
   (cosine similarity; `GATE_TOP_K = 5`).
3. Makes a **three-tier (really five-outcome) decision** by similarity band:

   | Outcome | Band | Effect |
   |---|---|---|
   | `SKIP_NEAR_DUPLICATE` | `>= NEAR_DUPLICATE_THRESHOLD` | absorbed into the existing memory; **no new row** |
   | `REINFORCE` | `>= MERGE_THRESHOLD` | boost the existing memory (reinforced-importance), no new row |
   | `INSERT_RELATED` | `>= RELATED_THRESHOLD` | insert the candidate **and** link the related memories |
   | `INSERT` | below `RELATED_THRESHOLD` | fresh memory |
   | `SKIP_EMBEDDING_FAILED` | — | embedding unavailable after retry; skip |

> ⚠️ **Port the constants, not the doc comment.** The file's header comment says
> "REINFORCE >= 0.80 / INSERT_RELATED 0.70–0.80" — **stale.** The authoritative
> exported constants are `NEAR_DUPLICATE_THRESHOLD = 0.90`, `MERGE_THRESHOLD =
> 0.85`, `RELATED_THRESHOLD = 0.70`. This is the same class of trap as the
> SQLCipher-vs-ChaCha20 comment: identifiers/comments lie, behavior is truth.
> Port the constants verbatim and let the differential prove the bands.

**Why it's the right first service.**

- **Contained.** One input (a candidate), one model call, one bounded read
  (top-K), one gated write. No turn management, no streaming, no `Event` channel.
- **Dependencies already ported.** It reuses Phase-1 leaves — the embedding vector
  math (cosine similarity, L2 normalization, Float32↔LE-BLOB), `extractNovelDetails`,
  memory weighting + reinforced-importance — and Phase-2 repos (`memories`,
  `vector_indices`). Almost nothing new below it; the port is the *orchestration*.
- **Exercises all three foundations at once.** It issues writes (Unit 0), it calls
  the embedding model (Unit 0.5), and its result is a `memories` / `vector_indices`
  DB effect (tier-2). One green differential validates the whole new stack.

**Verification plan (tier-3 → tier-2).** Inject the same canned embedding vector
for the candidate on both sides; seed a fixture with a handful of existing
memories + their vectors chosen to land the candidate in each band; run the gate;
diff the resulting `memories` + `vector_indices` state with the minted-values
remap normalization (the gate mints ids/timestamps on an INSERT). One corpus row
per outcome (`INSERT`, `INSERT_RELATED`, `REINFORCE`, `SKIP_NEAR_DUPLICATE`,
`SKIP_EMBEDDING_FAILED`), plus a boundary-exact row at each threshold.

**Likely follow-on units in the memory family** (same pattern, once the gate is
green): `memory-processor.ts`, `memory-service.ts`, and `housekeeping.ts` (the
`MEMORY_HOUSEKEEPING` job — the original reason for v4's child-process writer, now
a `tokio` blocking-pool task per Unit 0). These are heavier and model-dependent;
they follow naturally once the gate proves the tier-3 → writer-task path.

## The boundary contract in Phase 3 (build incrementally, don't front-load)

[`api-boundary.md`](./api-boundary.md) Part 1 locks the `Request`/`Response`/
`Event` contract as *the* line. Phase 3 does **not** need to enumerate every
variant up front — that would be speculative. Instead:

- Introduce `Request`/`Response`/`Event` as **thin enums that grow one variant per
  service** as services land. The memory gate is an internal service (no direct
  user Request), so it may not add a variant at all — it's called by the
  extraction path. Add variants when a user-meaningful operation actually needs
  one.
- Keep the **rule** from day one even while the enums are small: no business logic
  above the line; streaming only ever on `Event`. Getting the *discipline* right
  early is what's expensive to retrofit — not the variant count.
- The **axum HTTP shim** (Part 1) is worth standing up early *for CI*: it lets
  end-to-end tests drive `dispatch` with no webview. Defer the Tauri and uniffi
  adapters (Phase 4).

## The enclave `step()` seam (later in Phase 3)

Autonomous rooms ([`api-boundary.md`](./api-boundary.md) Part 3) are a Phase-3
service, but **not** an early one — they sit on top of chat orchestration and the
turn manager (turn-state machine, next-speaker selection — the pure leaves are
already ported in Phase 1). When the enclave engine is built, it must be the
`step()` + persisted `RunState` state machine from the design doc — one committed
turn per transition, cadence injected by a per-host driver, never a wall-clock
loop. Flagged here so no earlier unit accidentally bakes in an always-on-host
assumption.

## Deferrals carried in from Phase 2 (close during Phase 3)

These could not close in Phase 2 because each depends on a Phase-3 subsystem.
Track them to closure as their subsystem lands:

1. **`chats.delete`'s participant-vault summary sweep** — reached the
   vault-summary bridge. **✅ CLOSED (2026-07-07, W4.6b)**: ported the bridge
   (`services::conversation_summary_vault_bridge` —
   `write_conversation_summary_to_vaults` / `remove_conversation_summaries_from_vaults`)
   and composed `delete_conversation_with_vault_sweep` (participants captured before
   the row delete + the `syncVaults` skip). Differential-verified by
   `vault_summary_mirror_tier2_equivalence` (mirror + rename-in-place + skip + the
   delete sweep, five mount-index tables). **This was the last Phase-2 deferral.**
2. **The General / project wardrobe archetype tiers** — **✅ CLOSED (2026-07-03,
   wave 4 / W4.0)**: `db::archetype_wardrobe` + the seeded read overlay + the
   `WardrobeLocation` write generalization + the public READ trio + the
   transfers service, all differential-verified (see the CLAUDE.md W4.0 entry).
   The one residual is the deprecated `findByCharacterIdRaw` (reads the
   pre-cutover `wardrobe_items` table; no consumer yet).
3. **`background_jobs.markCompleted`'s dotted `payload.result` merge** — a forward
   v5-only capability (v4-on-SQLite throws `no such column`). **✅ CLOSED (W4.8,
   2026-07-06):** `services::job_runner`'s completion path now calls
   `markCompleted(id, Some(result))`, wiring the pure `merge_result_into_payload`
   through the runner; the end-to-end runner self-test proves the merged
   `payload.result` lands after a real handler dispatch.

## Unit order (summary)

0. Writer-task runtime (`Db`/`Writer` actor + `ReadPool`) — the last Phase-2 item. **✅ done** (`db::runtime`).
0.5. Tier-3 mocked-LLM harness scaffold (model boundary trait + canned responder,
   both sides). **✅ done** (`model::embedding`); the v4-oracle-side injection was
   exercised by Unit 1's differential.
1. **Memory gate** — first real service; validates 0 + 0.5 + tier-2 together.
   **✅ done + green** (`services::memory_gate`).
2. Memory family follow-ons (`memory-processor`, `memory-service`, `housekeeping`). **✅ done** (all sub-units below):
   - Deletion chokepoint (`deleteMemoryWithUnlink` / `deleteMemoriesWithUnlinkBatch`)
     — **✅ done + green** (`db::memories::delete_with_unlink` / `delete_many_with_unlink`,
     tsx real-DB differential). The leaf every cascade path deletes through.
   - Cascade-delete family (`deleteMemoryWithVector` + the three
     `deleteMemoriesBy*WithVectors`) — **✅ done + green**
     (`services::memory_service`, tsx real-DB differential
     `memory_cascade_tier2_equivalence` over `memories` + `vector_indices` +
     `vector_entries`; `CharacterVectorStore::remove_vector` added).
   - Housekeeping (`runHousekeeping` / `getHousekeepingPreview` /
     `needsHousekeeping`) — **✅ done + green** (`services::housekeeping`, tsx
     real-DB differential `memory_housekeeping_tier2_equivalence`: per-op
     results + three table dumps; retention / stored-vector merge / cap
     passes, protection gate, dry run).
   - The **completion half of the model boundary** (`model::completion`) —
     **✅ done** (self-tests, like Unit 0.5): `CompletionProvider` at v4's
     `provider.sendMessage` seam + `CannedCompletionProvider` keyed by the exact
     call input (provider | model | temperature | messages JSON). The
     oracle-side canned injection lands with the memory-processor differential.
   - The **memory-extraction pure leaves** (`memory_tasks`) — **✅ done + green**
     (`memory_tasks_equivalence`, tier-1): the SELF/OTHER prompt builders
     (generated `prompt_text` bodies, preambles, orienting footer), the shared
     turn-context renderer, the message builders, and the response parsers
     (fence stripping, targeting-tag validation, caps, JS coercion semantics).
     Jest oracle: v4's REAL extractors with only `executeCheapLLMTask` mocked.
   - The **memory processor** (`processTurnForMemory`) — **✅ done + green**
     (`services::memory_processor` + `cheap_llm` + `services::cheap_llm_exec`,
     `memory_processor_tier3_equivalence`): the first tier-3 differential to
     pin BOTH model boundaries — the completion by recorded exact call key
     (oracle-recorded `provider|model|temperature|messages` entries replayed
     through `CannedCompletionProvider`, so prompt/selection divergence is a
     canned-miss), the embedding by exact text. Result objects (debug logs
     byte-for-byte) + the three memory tables diffed. Also **closed the gate's
     `applyNamePresenceCheck` deferral** (the cross-character lookup now rides
     `characters_read::find_by_id` + the Phase-1 `resolve_about_character_id`).
     Tracked host-side deferrals: API-key acquisition, the fire-and-forget
     `logLLMCall` llm-logs write, and the temperature-fallback path is
     self-test-covered only (the corpus stays on the 0.3 path).
   - The gate's **watermark auto-housekeeping check**
     (`maybeEnqueueHousekeeping`) — **✅ done + green**
     (`memory_watermark_tier3_equivalence`): `services::queue_service` (the
     enqueue + dedupe slice; `ensureProcessorRunning` now CLOSED in W4.8 — the
     enqueue fires the runner's process-global wake hook), `services::housekeeping_outcome_cache` (process-global
     like v4's module-global Map), a scoped
     `chat_settings::find_auto_housekeeping_settings_by_user_id` read, and the
     gate wiring after INSERT / INSERT_RELATED. The memory family is now fully
     ported.
   - Next: chat orchestration (Unit 3 below).
3. Chat orchestration (turn manager + streaming on the `Event` channel). —
   **the core engine is DONE; wave 4 (the adjacent subsystems) is in
   progress.** Decomposed and tracked in
   [`chat-orchestration.md`](./chat-orchestration.md). Waves 1–3 are ported and
   green: the pure-leaf units, the system-prompt builder, the stateful
   turn-orchestration core, the streaming model boundary, the model-calling
   services (compression, context summary, knowledge injector, participant /
   user-identity resolvers, primary stream + recovery + failover, carina
   runner), the message finalizer, the `buildContext` capstone, the
   `processMessage` spine + `executeTurnChain` (the first end-to-end tier-3
   differential), and the `buildMessageContext` wrapper. Of wave 4: **W4.0
   (wardrobe drift batch), the entire W4.1 tool subsystem (a–g), W4.2
   (danger orchestration), and W4.4a parts 1–3 are DONE.** The endgame was
   re-planned 2026-07-06: every remaining batch — W4.2u, W4.3, W4.4a4,
   W4.4b, W4.5, W4.6a/b, W4.7a–f, W4.8 (the job runner, promoted to its own
   batch), W4.9a/b (image generation + photo trio) — has an agent-ready
   work order in [`work-orders/`](./work-orders/), with the batch table +
   execution rounds in [`chat-orchestration.md`](./chat-orchestration.md).
4. Enclave engine (`step()` + `RunState` + driver seam) — **DONE
   (2026-07-08, Round 5, branch `u4-enclave`)**. All four sub-units ported
   and differential-green: U4.1 milestones (+ the Phase-1 budget-port
   currency regen, zero drift), U4.2 cron (croner 10.0.1 hand-rolled — the
   Rust croner crate rejected; 124-row × 2-tz tier-1), U4.3
   announce/lifecycle (38-op tier-2, the cron seam closed at integration),
   U4.4 `step()` + schedule tick + the runner dispatch rows (the 19-call
   tier-3 capstone driving v4's REAL `handleAutonomousRoomTurn` /
   `…ScheduleTick`). Decomposed in [`enclave-engine.md`](./enclave-engine.md)
   (see its revised decision #3 — the turn writes DIRECTLY through `Db`,
   the `write_apply` routing superseded); work order
   [`work-orders/u4-enclave-engine.md`](./work-orders/u4-enclave-engine.md).

Each unit ships with its differential (tier-2, or tier-3 → tier-2 for
model-dependent ones), the same accept-nothing-unverified discipline as Phases 1–2.

## How to resume in a fresh session

Open with: *"Continuing the quilltap-v5 native port. Read CLAUDE.md,
docs/developer/porting/overview.md, and docs/developer/porting/chat-orchestration.md.
Phase 3 is in its planned endgame — pick the next work order from
docs/developer/porting/work-orders/ per the execution-rounds table (then the
enclave engine, Unit 4)."* The tier-1/tier-2 harness run commands are in
[`phase-0.md`](./phase-0.md) and [`phase-2-onramp.md`](./phase-2-onramp.md);
each tier-3 differential's exact oracle recipe is in its harness test header
(`crates/quilltap-harness/tests/*_equivalence.rs`).
