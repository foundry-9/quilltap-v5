# Survey — P4.138: the harness smalls lane (g + i)

**Date:** 2026-10-01 · **v4:** `f6426e196` (clean) · **v5 `main`:** `6d44cfae2` ·
**Kind:** read-only measurement — nothing built or run except
`recipe_sweep.py --self-test / --show / --list` (all read-only) and a python
probe of the driver's own regexes over literal strings.

Two harness-only items, one lane:

- **(g)** `memory_pipeline_jobs_tier3` is blind to the fold-episode prompt's
  bytes (the P4.D242 finding; the memory note
  `a-canned-miss-in-a-no-op-run-is-invisible` already carries it as its
  second instance).
- **(i)** `recipe_sweep.py --self-test` is RED on main — five headers spell
  the P4.53 clobber (`W=${V5W:-…}`).

## The finding in one line

(g) is a **comparand** gap, not a canned-key gap: the MPJ oracle keys every
call exactly, v5's canned provider refuses a miss with an `Err`, but the
fold-episode pass folds `success:false` and `[]` into the SAME `return
result` with no persisted trace — fix it harness-side with a recording
wrapper that asserts every oracle `canned` key was hit (fix 2, recommended —
no fixture change, no regen, covers every rule at once), optionally
strengthened by a non-empty episode reply (fix 1, which needs NO new
embedding entries because this family's embeddings are record-and-replay);
(i) is five `//!` headers that copied one 2026-09-18 header's
`W=${V5W:-$(git rev-parse --show-toplevel)}` spelling — safe at run time only
because the driver prepends `V5W="…"`, and **invisible to the driver's
`ALIAS_ASSIGN` backstop** (the value has spaces), so the fix is the sanctioned
literal `V5W=${V5W:-$HOME/source/quilltap-v5}` + `$V5W/…`, header text only,
stages unchanged.

---

## §A — (g): how `memory_pipeline_jobs_tier3` cans, records, replays, and where the blindness lives

### A1. The fixture's completion rules are MATCHERS, keyed by `kind` (+ `namePrefix`), not by exact key

`harness/oracle/fixtures/memory-pipeline-jobs-tier3.json` `:156`
`"completionRules": [` — seven rules:

| line | `kind` | `namePrefix` | `response` | usage p/c/t |
|---|---|---|---|---|
| :158-159 | `self` | `Ada` | `[{"content": "Ada greeted a traveler warmly and repaired his pocket watch.", "summary": "warm greeting and a watch repair", "keywords": ["traveler", "watch"], "importance": 0.6, "temporal": "moment", "scope": "narrow", "context": "information"}]` | 140/26/166 |
| :164-165 | `self` | `Bram` | `[]` | 120/2/122 |
| :170-171 | `self` | `Quill` | `[]` | 118/2/120 |
| :176-177 | `other` | `""` | `[]` | 150/2/152 |
| :182 | `fold` | — | `Active threads: the correspondents keep exchanging terse notes.\n\nResolved decisions: none.\n\nEmotional state: businesslike calm.\n\nOpen questions: none.\n\nTimeline:\n- 2023-06-06: a long exchange of short notes.` | 400/48/448 |
| **:187** | **`episode`** | — | **`[]`** | 300/2/302 |
| :192 | `title` | — | `A Ledger of Short Notes` | 90/8/98 |

Cases: `:197` `meCases` (six MEMORY_EXTRACTION), `:205` `csCases` (four
CONTEXT_SUMMARY: `cs_fold` on chat `d1000006` [24 messages,
`2023-06-06T10:00:00.000Z` → `10:23:00.000Z`, one participant `a6…01` →
characterId `cd000000-0000-4000-8000-0000000000dd`], `cs_below_gate` and
`cs_force` [`forceRegenerate: true`] both on `d1000007` [2 messages,
`2023-06-07T10:00:00.000Z`/`10:00:05.000Z`, participant → `ce…ee`],
`cs_missing_chat`). The three provisioned characters are Ada `ca…0a`, Bram
`cb…0b`, Quill `cc…0c` (`:characters`); **the two CS chats' characters
`cd…dd`/`ce…ee` are deliberately NOT provisioned** (the builder's own
comment, `harness/oracle/fixtures/build-memory-pipeline-jobs-fixture.ts:14-15`:
"NONEXISTENT characterIds so the fold's vault mirror/refresh no-op — the
ctx-summary family's unprovisioned-character idiom").

### A2. The jest side: classify by prompt prefix, record by EXACT key, keep the first

`harness/oracle/cases/memory-pipeline-jobs-tier3.test.ts`:

- Header `:16-20`: the mock "RECORDS the exact `provider|model|temperature|messages`
  canned key it answered (`kind:"canned"` rows) … The Rust side replays those
  exact entries through `CannedCompletionProvider` — a prompt/selection/
  temperature divergence surfaces as a canned-miss."
- `:21-24`: embeddings are **RECORD-and-replay**: "any text embeds to the fixed
  unit vector e0 (dim 8) and the text is recorded (`kind:"cannedEmbedding"`
  rows)". Implementation `:185-193` (`embeddingRecorded.add(text)`; vector
  `EMBED_VECTOR = [1, 0, 0, 0, 0, 0, 0, 0]` `:122`).
- `:26`: `logLLMCall` → no-op ("the llm-logs partition is not part of this family").
- The `sendMessage` mock `:202-240`: classification `:212-218` —
  `system.startsWith('You are updating an existing summary')` → `fold`;
  `system.startsWith('You are consolidating a batch')` → **`episode`**;
  `system.startsWith('Generate a literary title')` → `title`; else the
  CONTEXT footer `/\nCONTEXT\n(SUBJECT|OBSERVER): ([^\n]*)\n/` `:220` →
  `self`/`other` with `label.startsWith(r.namePrefix ?? '')` `:224-226`.
  `:228` `if (!rule) throw new Error('no completion rule for call');`
  `:229` `` const key = `${provider}|${params.model}|${params.temperature ?? '-'}|${JSON.stringify(messages)}`; ``
  `:230-238` first-write-wins (`if (!cannedRecorded.has(key))`).
- The fold-episode call reaches this mock through v4's REAL
  `handleContextSummary` → `generateContextSummary` → `runFoldEpisodePass`
  (`lib/chat/context-summary.ts:552-566`, inside its own try/catch) →
  `extractEpisodesFromFold` (`lib/memory/cheap-llm-tasks/memory-tasks.ts:1213-1252`,
  messages = `[{system: FOLD_EPISODE_PROMPT}, {user: "${clockLine}\n\nDATED TURNS:\n\n${rendered}"}]`
  :1237-1240, then `executeCheapLLMTask(…, parseFoldEpisodes, 'fold-episode-extraction', chatId)`).
  P4.D242 counted **2** episode-sentence occurrences in a fresh NDJSON
  ("memory-pipeline-jobs 10/9/2") = the `cs_fold` and `cs_force` folds; the
  two windows differ, so the two keys differ (the
  `identical-canned-keys-share-the-first-recorded-answer` trap does not bite
  here — see §D).
- Written per run `:341-345`: every `canned` row and every `cannedEmbedding`
  row; `:356-364` six table dumps (`memories` by `content`, `vector_indices` by
  `id`, `vector_entries` by `embedding`, `chat_messages` by `content`, `chats`
  by `id`, `background_jobs` by `type`).

### A3. The Rust side: exact-key replay; a miss is an `Err`, never an answer

`crates/quilltap-harness/tests/memory_pipeline_jobs_tier3_equivalence.rs`:

- `:398-423` "Replay EXACTLY the oracle-recorded entries — an input the
  oracle never sent surfaces as a canned-miss, not an answer." —
  `CannedCompletionProvider::with_response(provider, model, temperature,
  &messages, response, Some(usage))` per `canned` row; `:425-428` embeddings
  via `CannedEmbeddingProvider::with_vector(text, vec)`.
- `:475-494` the CS loop calls `handle_context_summary(&db, &completion,
  &embedding, &executor, &spec.user_id, &payload)` and asserts only the
  thrown-error string (`assert_case_error` `:449-453`); `:497-` dumps + diffs
  the six tables in the `TABLES` spec (`:160-215`; `memories`: `order_by:
  "content"`, `id_columns: ["id","sourceMessageId"]`, `id_array_columns:
  ["relatedMemoryIds"]`, `ts_columns: [createdAt, updatedAt, lastReinforcedAt,
  lastAccessedAt]`; `chat_messages`: `text_id_columns: ["debugMemoryLogs"]`).
- The key: `crates/quilltap-core/src/model/completion.rs:427-448`
  `canned_key_from_parts` → `format!("{provider}|{model}|{temp}|{messages_json}")`
  (temperature `None` → `"-"`, messages projected to `[{role, content}]`) —
  byte-identical to the jest key; `canned_completion_key` is `pub` (`:389`).
- The miss: `completion.rs:584-621` `impl CompletionProvider for
  CannedCompletionProvider` — `responses.get(&key)` `None` →
  `Err(CompletionError::new(format!("no canned completion registered for call
  (provider {provider}, model {}, temp {:?}, {} message(s), first {} chars)", …)))`
  (`:613-619`; the why-comment `:606-612`: "temp", not "temperature", so the
  executor never mistakes a miss for a temperature rejection).
  `CannedCompletionProvider` is `#[derive(Clone, Default)]` over two
  `HashMap`s (`:485-489`) — **no interior mutability, no hit counter**.

### A4. Where the `Err` is swallowed — the exact collapse

1. The executor turns the `Err` into `success:false`:
   `crates/quilltap-core/src/services/cheap_llm_exec.rs:1248-1257` logs
   `[CheapLLM] Task failed` (target `quilltap::cheap_llm`, `error = %error.message`),
   walks the fallback chain (`:1266-1280`; none in this fixture), then `:1288-1294`
   `CheapLlmTaskResult { success: false, result: None, error: Some(error.message), timed_out, usage: None }`.
2. The episode pass collapses both arms:
   `crates/quilltap-core/src/services/fold_episode_pass.rs:168-181`
   ```rust
   let extraction = executor.execute(completion, selection, messages, parse_fold_episodes,
       None, None, None, Some("fold-episode-extraction"), CheapLlmTaskOptions::default()).await;
   let episodes = match (extraction.success, extraction.result) {
       (true, Some(episodes)) if !episodes.is_empty() => episodes,
       _ => return result,
   };
   ```
   `(false, None)` [a miss] and `(true, Some(vec![]))` [the `[]` reply] both hit
   `_ => return result` — `result` is `FoldEpisodePassResult::default()`
   (`:103`), nothing written, nothing logged.
3. The caller discards even that result:
   `crates/quilltap-core/src/services/context_summary.rs:456-470`
   (`RealContextSummarySeams`) — "Best-effort — the pass swallows its own
   failures." `let _ = run_fold_episode_pass(…)`; the same at `:339-351`
   (`FoldEpisodePassSeams`).
4. **v4 is shaped the same but WARNS on the failure arm** —
   `lib/memory/fold-episode-pass.ts:100-109`:
   ```ts
   if (!extraction.success || !extraction.result || extraction.result.length === 0) {
     if (!extraction.success) {
       logger.warn('[FoldEpisodePass] Episode extraction failed', { chatId, error: extraction.error })
     }
     return result
   }
   ```
   v4 has four `[FoldEpisodePass]` lines (`:103` warn, `:201` warn `Failed to
   write episode for character`, `:210` info `Episode pass complete`, `:218`
   warn `Episode pass failed (non-fatal)`); **`fold_episode_pass.rs` has zero
   `tracing::` lines** (measured: `ggrep -n "tracing::"` → none). An absent-lines
   aside, recorded in §D — it is why no capture-pin "third fix" exists for a
   harness-only lane.

So: a changed prompt → a different key → the oracle's recorded key is never hit
→ v5's call misses → `Err` → `success:false` → `return result` → the same
zero rows as the oracle's `[]` reply wrote. **M7 survived by construction.**

### A5. Why the extraction rules' `[]` are NOT in the same blind class

`memory-pipeline-jobs-tier3.json:164-177` also answers `[]` for Bram, Quill and
the OTHER pass, but a miss THERE leaks into a persisted column:
`crates/quilltap-core/src/services/memory_processor.rs:1111-1129` pushes
`"[Memory] OTHER extraction {failed|was LOST to a timeout} ({observer} → {n} subject(s)): {error}"`
onto `state.debug_logs` (the SELF arm `:1029-1033`: `"[Memory] SELF extraction failed for {}: {}"`),
and `crates/quilltap-core/src/services/memory_extraction_job.rs:353` writes
`{ "debugMemoryLogs": result.debug_logs }` onto the chat message — a
`chat_messages` column the comparand diffs (`text_id_columns: ["debugMemoryLogs"]`).
That is why P4.D242 saw the family red "at index 0" on the extraction calls at
the target while the episode sentence slipped. Only the episode pass has no
trace of any kind.

### A6. The three sibling families that DO see the episode prompt

| family | how it answers the episode call | why a miss reddens |
|---|---|---|
| `fold_episode_tier3` | `harness/oracle/fixtures/fold-episode-tier3.json:123-` run `episode_pass` with `"episodeResponse": "```json\n[{\"narrative\": \"On July 14th, Aria and Bram visited Lighthouse Point and bought the brass sextant from the chandler by the jetty. They stayed until the tide turned.\", \"summary\": \"visited lighthouse point\", \"when\": \"2026-07-14\", \"narrativeTime\": \"the third day ashore\", \"entities\": [\"Lighthouse Point\", \"  \", \"the chandler\", 7], \"participants\": [\"Aria\", \"Bram\"], \"importance\": 0.85}, {\"narrative\": \"Later the same afternoon they watched the gulls quarrel over the jetty rail.\", \"summary\": \"   \", \"importance\": 4}, {\"narrative\": \"A third episode beyond the cap.\", \"summary\": \"dropped by the cap\"}]\n```"` (`:131`), usage 420/90/510; the jest mock answers `currentRun.episodeResponse` (`fold-episode-tier3.test.ts:196-224`). Its second run `no_episodes` answers `[]` — the first instance of the trap. | a miss writes zero episodes where the oracle wrote two → `memories` diverges. Embeddings there are EXACT-key (`cannedEmbeddings` `:205-218`, keyed by the full embed text `"visited lighthouse point\n\nOn July 14th, … tide turned.\n(when: 2026-07-14 · story time: the third day ashore · place: Lighthouse Point, the chandler)"` → `[1,0,0,0]`; a miss throws `EmbeddingError("no canned embedding registered for input (N chars)")` `:174-179`). |
| `context_summary_service_tier3` | `harness/oracle/fixtures/context-summary-service-ops.json` rules keyed by `op` + `kind: "fold-episode"`: `fold_regular` → `[{"narrative":"On March 1st, Vaulted A and the crew finished the Blackwater run and split the take in the back room of the Copper Lantern.","summary":"blackwater run take split","when":"2026-03-01","entities":["Blackwater","Copper Lantern","Vaulted A"],"participants":["Vaulted A","User"],"importance":0.7}]` (`:28`), `check_gate_fires_fold` → two episodes (`:118`), `fold_with_librarian_sweep` → `"not json at all"` (`:73`), the rest `[]`. Classifier `context-summary-service-tier3.test.ts:220-228` on `'You are consolidating a batch of roleplay conversation turns into EPISODE records'` (the P4.36 comment: v4 runs the pass on EVERY fold, incl. a `check` op's internal fold). Embedding = a fixed unit vector for EVERY text (`:31`). | non-empty replies on folds whose chats reference **unprovisioned** characters (`:344-346`: "the check-op chats reference unprovisioned characters") still wrote `memories` rows — P4.D242's red was "`memories rows diverge (the fold-episode pass's writes)`". **This is the precedent that an episode row inserts without a character row** (the `memories` DDL, `crates/quilltap-core/src/services/provisioning/fresh_schema.json` `/main[21]`: `"characterId" TEXT NOT NULL` with no `FOREIGN KEY`/`REFERENCES` clause — grep over the DDL string returns none; v4's `createMemoryWithGate` `lib/memory/memory-service.ts:371` looks a character up only inside `applyNamePresenceCheck` `:153-154`, i.e. only for a non-self `aboutCharacterId`, which the pass never sets). |
| `courier_images_routes` | `harness/oracle/cases/courier-images-routes.test.ts:91` `const EPISODE_RESPONSE = '[]';` `:92` usage 99/3/102; classification `:192-194`. **Also `[]`** — yet M7 reddened it (`["resolve_cadence_tables"]`). | because its comparand carries the request BYTES: `crates/quilltap-harness/tests/courier_images_routes_equivalence.rs:18-24` diffs the fold + episode `SUMMARIZATION` llm_logs rows over the committed empty `courier-images-llmlogs.db`, `dump_llm_logs_stable` `:385-395` selecting `qt_text(request)`. That route is closed to MPJ without an oracle change: the MPJ jest no-ops `logLLMCall` (`:26`) and ships no llm-logs partition (`DbPaths { llm_logs: None }` `:434`). |

### A7. The recipe and its `TZ=UTC` pin

`python3 harness/tools/recipe_sweep.py --show memory_pipeline_jobs_tier3_equivalence`
(normalized; the driver prepends `V5="/Users/csebold/source/quilltap-v5"`):

```
# ---- regen (from the .rs header)
set -euo pipefail
V5="/Users/csebold/source/quilltap-v5"
N=~/.nvm/versions/node/v24.13.1/bin ; V5="/Users/csebold/source/quilltap-v5"
cd ~/source/quilltap-server
QT_FIXTURE_OUT=/tmp/qt-mpj-main.db QT_FIXTURE_MOUNT_OUT=/tmp/qt-mpj-mount.db \
$N/npx tsx $V5/harness/oracle/fixtures/build-memory-pipeline-jobs-fixture.ts
mkdir -p /tmp/qt-mpj-oracle/cases /tmp/qt-mpj-oracle/fixtures
cp $V5/harness/oracle/cases/memory-pipeline-jobs-tier3.test.ts /tmp/qt-mpj-oracle/cases/
cp $V5/harness/oracle/fixtures/memory-pipeline-jobs-tier3.json /tmp/qt-mpj-oracle/fixtures/
TZ=UTC QT_FIXTURE_MPJ_MAIN=/tmp/qt-mpj-main.db QT_FIXTURE_MPJ_MOUNT=/tmp/qt-mpj-mount.db \
QT_ORACLE_OUT=/tmp/oracle-memory-pipeline-jobs.ndjson \
$N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots /tmp/qt-mpj-oracle/cases -- memory-pipeline-jobs-tier3
# ---- run
set -euo pipefail
TZ=UTC QT_ORACLE_MPJ=/tmp/oracle-memory-pipeline-jobs.ndjson \
QT_FIXTURE_MPJ_MAIN=/tmp/qt-mpj-main.db QT_FIXTURE_MPJ_MOUNT=/tmp/qt-mpj-mount.db \
cargo test -p quilltap-harness --test memory_pipeline_jobs_tier3_equivalence
```

The pin is on BOTH stages (`.rs` header `:27` and `:30-31`: "TZ pinned to match
the oracle's date math"; the `.ts` header `:44-45`: "the fold's [YYYY-MM-DD]
prefixes and the episodic date math are zone-sensitive"). P4.D242's gate
measured the family "green with and without `TZ=UTC`" on today's corpus; fix 1
adds a `when` phrase to that date math, so the pin stays and the reply's
`when` should be ABSOLUTE (§B1) — the driver's `normalize()` "Never touches TZ
pins" (`recipe_sweep.py:910-912`).

---

## §B — (g): the two candidate fixes, with exact shapes

### B1. Fix 1 — a non-empty canned episode reply in `memory-pipeline-jobs-tier3.json`

**Shape.** Replace `:187-190`'s `"response": "[]"` with one episode. What a row
needs, from v5's parser `crates/quilltap-core/src/memory_tasks.rs:888-963`
(`parse_fold_episodes`, v4 `parseFoldEpisodes`): `narrative` (string, required
non-empty after trim — an empty one is skipped `:928-934`), `summary` (string;
blank → `utf16_truncate(narrative, 60)` `:935-939`), optional `when`
(`narrativeTime` likewise), `entities`/`participants` (strings only, blank
dropped, cap 8 `:905-917`), `importance` (number clamped `0.2..=1.0`, else
`0.6` `:954-961`), cap `FOLD_EPISODE_CAP = 2` (`:870`). Proposed:

```json
{
  "kind": "episode",
  "response": "[{\"narrative\": \"The correspondents traded a dozen short notes across one morning and agreed to keep the ledger terse.\", \"summary\": \"a ledger of terse notes\", \"when\": \"2023-06-06\", \"entities\": [\"the ledger\"], \"participants\": [], \"importance\": 0.5}]",
  "usage": { "promptTokens": 300, "completionTokens": 48, "totalTokens": 348 }
}
```

Design points, each measured:

- **One rule serves BOTH folds** (`cs_fold` and `cs_force` — the `episode`
  rule is matched by `kind` alone, `:216`), so the reply writes one episode in
  `d1000006` (character `cd…dd`) and one in `d1000007` (`ce…ee`). The two
  canned keys differ (different `DATED TURNS`), the two memories differ by
  `characterId`/`chatId`/`sourceMessageId`, and `memories` orders by `content`
  — make the narrative distinct from every existing content (`Ada greeted a
  traveler …`, the whisper/system rows) so the sort is unambiguous.
- **`when` absolute, not relative.** `fold_episode_pass.rs:197-200` resolves
  `when` through `resolve_when_phrase(Some(w), &clock.now_iso)` and falls back
  to the window's first stamped message; an absolute date is zone-free under
  the family's `TZ=UTC` pin, a relative phrase ("yesterday") is zone math. Keep
  `narrativeTime` out (both chats are `realtime`). Avoid a date INSIDE the
  narrative text, or v4's `applyEpisodicFallbackAnchors`
  (`memory-service.ts:384-387`) and its v5 twin become part of the proof —
  fine, but then it is two things.
- **No new embedding entries are needed** — this family's embeddings are
  RECORD-and-replay (`:21-24`, `:185-193`): the oracle records whatever text v4
  embeds (`"{summary}\n\n{narrative}\n(when: …)"`, the shape
  `fold-episode-tier3.json:206` shows) and v5 must produce the identical text
  or its `CannedEmbeddingProvider` misses (`model/embedding.rs:190` `no canned
  embedding registered for input (N chars)`) → `create_memory_with_gate`
  answers `SKIP_EMBEDDING_FAILED` → no row → the `memories` diff reddens.
  The "which embedding entries" half of the question dissolves.
- **Tables that gain rows, all inside the comparand:** `memories` (+2,
  `kind: 'episodic'`, `source: 'AUTO'`, keywords
  `[…entities lowercased, "past", "scope: narrow", "history"]`,
  `witnessedContext: 'user_present'`, `sourceMessageId` = the window's last
  stamped message, per `fold_episode_pass.rs:224-260`), `vector_entries` (+2)
  and `vector_indices` (+ the two characters' indices). `chat_messages`,
  `chats`, `background_jobs` unmoved (the per-character housekeeping enqueue
  `memory-service.ts:61-75` is count-gated and stays below cap). The
  `memories` `TABLES` spec already remaps `id`/`sourceMessageId`/
  `relatedMemoryIds` and the four timestamp columns; `occurredAt` /
  `sourceMessageTimestamp` are deterministic (`2023-06-06T…`) and compare raw.
- **A tie risk to know in advance:** `vector_entries` orders by `embedding`
  and every vector in this family is e0, so with three equal keys the order is
  insertion order on both sides (SQLite scan order vs JS stable sort over the
  rowid-ordered `SELECT *`) — identical as long as both sides write in the same
  case order, which they do. If the family reddens on `vector_entries` ONLY
  after fix 1, it is this tie, not the port.
- **Unprovisioned characters are fine** — §A6's csum precedent + the FK-less
  DDL. The pass's `present_participants` filter (`fold_episode_pass.rs:119-129`)
  needs `type == "CHARACTER"`, a present status and a non-empty `characterId`;
  the builder seeds `type: 'CHARACTER'` (`build-memory-pipeline-jobs-fixture.ts:203`)
  and no status (→ the default), so both CS chats qualify.
- **Red-first proof:** regen at the pin, then run with the pre-P4.D242 core
  file whole (`git show <pre>:crates/quilltap-core/src/memory_tasks/prompt_text.rs`
  into place, restore by backup) — the memory note's rule, since a single-line
  mutation can pass for the wrong reason.

**Cost:** one fixture edit + one oracle regen (the committed `cannedEmbedding`
and `canned` rows are NOT committed — the NDJSON is a `/tmp` artifact — so only
the fixture changes), and the family's recipe already regenerates per run.

### B2. Fix 2 — "every recorded canned row was consumed", harness-side

**Precedent.** No family asserts it today (grep of
`crates/quilltap-harness/tests` for `unconsumed` / `unmatched canned` / `unused
canned` → none). Two shapes exist that it would be built from:

- `crates/quilltap-harness/tests/ai_import_tier3_equivalence.rs:286-310` —
  `struct ScriptedProvider { script, next: Mutex<usize>, calls: Mutex<Vec<Value>> }`
  whose `send_message` pushes `json!({ "provider", "baseUrl", "model",
  "temperature", "maxTokens", "profileParameters", "messages": […] })` per call
  (`:302-310`); `:771` `let calls = completion.calls.lock().unwrap().clone();`
  then diffed against the oracle row's `calls` (`:899`, `:939`) — "every
  recorded model call (the source context and the step prompts as bytes)"
  (`:10-11`).
- `crates/quilltap-harness/tests/pascal_custom_tools_execution_equivalence.rs:8`
  / `:419` — `FixedBytes::consumed()` equality: "every byte consumed".

**Shape.** In `memory_pipeline_jobs_tier3_equivalence.rs`, a test-local
wrapper around the canned provider that records the key of every call and
asserts set-equality against the oracle's `canned` rows:

```rust
struct KeyRecording<C> { inner: C, hit: Mutex<HashSet<String>> }
impl<C: CompletionProvider> CompletionProvider for KeyRecording<C> {
    fn send_message(&self, provider: &str, base_url: Option<&str>, params: &CompletionParams)
        -> impl Future<Output = Result<CompletionResponse, CompletionError>> + Send {
        self.hit.lock().unwrap().insert(canned_completion_key_with_attachments(
            provider, &params.model, params.temperature, &params.messages, &params.attachments));
        self.inner.send_message(provider, base_url, params)
    }
}
// after the two case loops, before the table diff:
let want: BTreeSet<String> = oracle_canned.iter().map(|r| canned_completion_key(&r.provider, &r.model, r.temperature, &messages_of(r))).collect();
let got: BTreeSet<String> = recording.hit.lock().unwrap().iter().cloned().collect();
assert_eq!(got, want, "canned keys hit ≠ keys the oracle recorded — a prompt/selection divergence the tables could not see");
```

Measured facts the shape rests on: `CompletionProvider`'s other three methods
default onto `send_message` (`completion.rs:272-330`; `CannedCompletionProvider`
itself implements only `send_message` `:584`), so one method suffices;
`canned_completion_key` and `canned_completion_key_with_attachments` are `pub`
(`:389`, `:459`); the oracle NDJSON **already carries the per-call key
list** — every `canned` row's `provider`/`model`/`temperature`/`messages`
(`:231-238`) is the key's four parts, so **no oracle change is needed**. The
oracle dedups by key (`:230`), v5 may hit a key twice; set semantics absorb it.
Diagnostics: on failure print `want − got` (keys never hit = v5 built a
different prompt) and `got − want` (keys v5 sent that v4 never did — today an
invisible swallowed `Err`).

**Cost:** one harness test edit, no fixture change, no regen (the existing
NDJSON suffices), no core change. **Red-first proof:** the same whole-file
pre-port restore as B1 — with the pre-P4.D242 `prompt_text.rs` the two episode
keys land in `want − got`.

### B3. Recommendation: **fix 2 first, fix 1 as the optional second step**

- Fix 2 is a **general** guard: it covers every rule at once (every `[]` the
  fixture answers today and any `[]` a future case adds), and it covers the
  reverse direction (`got − want`) that neither the tables nor fix 1 can see.
  Fix 1 covers the episode rule only and only while that rule stays non-empty.
- Fix 2 touches nothing committed but one `.rs`; fix 1 moves a committed
  fixture every reader regenerates from (§D).
- Fix 1 still earns its place as the "writes" proof — the memory note's own
  rule is "put the arm in a run whose canned answer produces writes", and a
  non-empty episode on real `CONTEXT_SUMMARY` handlers proves the whole
  episode→gate→vector path under this family's seams (today proven only by
  the two sibling families). It is cheap (one fixture rule, no embeddings);
  do it in the same lane AFTER fix 2 is green, as its own commit.
- NOT recommended: an llm-logs comparand (the courier route) — it needs the
  oracle to stop no-op'ing `logLLMCall` and a third partition on both sides;
  and NOT available to a harness-only lane: a capture pin on v4's
  `[FoldEpisodePass] Episode extraction failed` warn — v5 does not emit it
  (§A4 item 4; an absent-lines candidate for a core smalls order).

---

## §C — (i): `recipe_sweep.py --self-test` is RED on main

### C1. The failure, verbatim

`python3 harness/tools/recipe_sweep.py --self-test` at `6d44cfae2`, exit 1,
exactly ONE failure (the full output is three lines):

```
SELF-TEST FAIL: a header defaults one checkout alias from another (the P4.53 clobber spelling — write `V5W=${V5W:-$HOME/source/quilltap-v5}` and reference `$V5W/…`): ['doc_opacity_equivalence: W=${V5W:-…}', 'instance_settings_json_warns_equivalence: W=${V5W:-…}', 'scenario_builder_mount_pool_equivalence: W=${V5W:-…}', 'scenario_builder_tier3_equivalence: W=${V5W:-…}', 'scenario_builder_routes_equivalence: W=${V5W:-…}']
[selftest_alias] alias assignment neutralized to --v5w (/probe/worktree): W=${V5W:-$HOME/source/quilltap-v5}
self-test: 1 failure(s)
```

The second line is the self-test's OWN probe (`recipe_sweep.py:1888-1894`
feeds the literal clobber through `normalize()` and expects the notice), not a
header. **No other self-test check fails.**

### C2. The rule, in the driver's words

`harness/tools/recipe_sweep.py:66-74` (module docstring):

> ALIAS ASSIGNMENTS ARE UNFORGEABLE (P4.53). `normalize()` rewrites ANY
> `V5W=`/`WT=`/`V5=`/`W=` assignment statement to `--v5w` and says so once per
> family, so a header cannot decide which checkout it is tested against. Five
> case headers had written `W=${V5W:-$HOME/source/quilltap-v5}`, which overwrote
> the driver's injected alias with MAIN's path — the family staged its case file
> and its fixtures from main during a worktree sweep and exited 0 regardless. The
> committed headers are repaired to the sanctioned self-referential convention
> (`V5W=${V5W:-…}` + `$V5W/…`); this rewrite is the backstop that makes the class
> unrepeatable. See `ALIAS_ASSIGN`.

The sanctioned spelling, `:59-63`: "V5W the v5 checkout (headers default it
with `V5W=${V5W:-$HOME/source/quilltap-v5}` so they are copy-paste-safe; the
driver overrides it to `--v5w` …)". The two regexes: `ALIAS_ASSIGN` `:843-848`
(an assignment STATEMENT — the value is `(?:"[^"\n]*"|'[^'\n]*'|[^\s;#])+`,
i.e. **no unquoted whitespace**, with a lookahead to end-of-statement so an
env PREFIX is never swallowed, `:838-842`) and `CROSS_ALIAS_DEFAULT` `:854-856`
(`var=${from:-`; the self-test flags `var != from`). `alias_value_is_settled`
`:859-885`: the self-referential form is "settled" (rewritten silently — "38
families are written this way"); the cross form "is the P4.53 clobber, and it
is what the notice is for". The committed-header check itself: `:1942-1966`,
scanning every `family_files(v5w)` `.rs` doc header AND every
`harness/oracle/cases/*.ts`/`*.tsx` header (`rs_doc_header`/`ts_doc_header`).

### C3. The five headers (file:line), and what each `$W/` reference becomes

All five spell `W=${V5W:-$(git rev-parse --show-toplevel)}` — a variant of the
P4.53 clobber (a DIFFERENT default from the docstring's, but the same
cross-alias shape). The `.rs` headers are what the driver regenerates from
("`# ---- regen (from the .rs header)`" for all five).

| family | assignment | every `$W` reference in the header |
|---|---|---|
| `doc_opacity_equivalence` | `crates/quilltap-harness/tests/doc_opacity_equivalence.rs:54` | `:57` `cp $W/harness/oracle/cases/doc-opacity.test.ts …`, `:58` `cp $W/harness/oracle/fixtures/doc-opacity.json …`, `:62` `$N/node --import tsx $W/harness/oracle/fixtures/build-doc-opacity-fixture.ts` |
| `instance_settings_json_warns_equivalence` | `crates/quilltap-harness/tests/instance_settings_json_warns_equivalence.rs:20` | `:23` `cp $W/harness/oracle/cases/instance-settings-json-warns.test.ts …`, `:24` `cp $W/harness/oracle/fixtures/instance-settings-json-warns.json …` |
| `scenario_builder_mount_pool_equivalence` | `crates/quilltap-harness/tests/scenario_builder_mount_pool_equivalence.rs:73` | `:76`, `:77` (two `cp`), `:81` `… tsx $W/harness/oracle/fixtures/build-doc-opacity-fixture.ts` |
| `scenario_builder_tier3_equivalence` | `crates/quilltap-harness/tests/scenario_builder_tier3_equivalence.rs:45` | `:48`, `:49` (two `cp`), `:53` (the fixture builder) |
| `scenario_builder_routes_equivalence` | `crates/quilltap-web/tests/scenario_builder_routes_equivalence.rs:73` (a **quilltap-web** family; its run stage is `cargo test -p quilltap-web --test scenario_builder_routes_equivalence -- --nocapture`) | `:76`, `:77` (two `cp`), **`:79` `QT_FIXTURE_SBR_MAIN=$W/crates/quilltap-web/tests/fixtures/chat-send-main.db \`**, **`:80` `QT_FIXTURE_SBR_MOUNT=$W/crates/quilltap-web/tests/fixtures/chat-send-mount.db \`** — two COMMITTED fixtures named through the alias in env prefixes, which the shield policy also rewrites (`shield_fixture_envs` `:952-974` replaces `$V5W`/`$V5`/`$W` before copying the `.db` to `/tmp/qt-recipe-shield-<family>/`). |

Rewrite rule, mechanical: the assignment line becomes
`N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}`
and every `$W/` becomes `$V5W/` (and `cd`/`cp`/`rm`/`STAGE` lines carry no
other alias). No `cd` line references `$W` in any of the five (each is
`cd ~/source/quilltap-server`, which the driver points at `--v4`,
`:914-920`). The precedent spelling to copy, already in the tree:
`crates/quilltap-harness/tests/context_summary_service_tier3_equivalence.rs:95`
`N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}`.

**One header's full regen block** (`doc_opacity_equivalence.rs:49-67`, the
`//!` margin stripped) and its rewrite:

```
# as committed
N=~/.nvm/versions/node/v24.13.1/bin ; W=${V5W:-$(git rev-parse --show-toplevel)}
STAGE=/tmp/qt-oracle-stage-doc-opacity
rm -rf $STAGE && mkdir -p $STAGE/harness/oracle/cases $STAGE/harness/oracle/fixtures
cp $W/harness/oracle/cases/doc-opacity.test.ts $STAGE/harness/oracle/cases/
cp $W/harness/oracle/fixtures/doc-opacity.json $STAGE/harness/oracle/fixtures/
cd ~/source/quilltap-server        # or a worktree pinned at the baseline
rm -f /tmp/qt-dopa-main.db /tmp/qt-dopa-mount.db
QT_FIXTURE_DOPA_MAIN=/tmp/qt-dopa-main.db QT_FIXTURE_DOPA_MOUNT=/tmp/qt-dopa-mount.db \
$N/node --import tsx $W/harness/oracle/fixtures/build-doc-opacity-fixture.ts
QT_FIXTURE_DOPA_MAIN=/tmp/qt-dopa-main.db QT_FIXTURE_DOPA_MOUNT=/tmp/qt-dopa-mount.db \
QT_ORACLE_OUT=/tmp/oracle-doc-opacity.ndjson \
$N/npx jest --silent --watchman=false --testTimeout=240000 \
--roots "$PWD" --roots "$STAGE/harness/oracle/cases" -- "doc-opacity\.test\.ts$"

# after the fix (four substitutions: the assignment + three `$W/` → `$V5W/`)
N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
…
cp $V5W/harness/oracle/cases/doc-opacity.test.ts $STAGE/harness/oracle/cases/
cp $V5W/harness/oracle/fixtures/doc-opacity.json $STAGE/harness/oracle/fixtures/
…
$N/node --import tsx $V5W/harness/oracle/fixtures/build-doc-opacity-fixture.ts
```

The header's own prose two lines above (`:50-52`) already says "The sweep
driver is the sanctioned path … it supplies the checkout, so this header never
names one" — the committed-recipe rule, honoured in prose, broken in the
spelling.

### C4. The driver already rewrites at run time — so the fix is header text only, and the stages do not move

`--show` for each of the five prepends BOTH aliases and keeps the statement
verbatim; e.g. `doc_opacity_equivalence`'s first four normalized lines:

```
set -euo pipefail
V5W="/Users/csebold/source/quilltap-v5"
W="/Users/csebold/source/quilltap-v5"
N=~/.nvm/versions/node/v24.13.1/bin ; W=${V5W:-$(git rev-parse --show-toplevel)}
```

(the same four for `instance_settings_json_warns_equivalence`,
`scenario_builder_mount_pool_equivalence`, `scenario_builder_tier3_equivalence`,
`scenario_builder_routes_equivalence`, each followed by its own `STAGE=…-<family>`
line and the stages quoted in C3's table; the full normalized regen+run blocks
are what `--show <family>` prints, byte-stable for the lane to diff before/after).
At run time `V5W` is injected first (`normalize()` `:935-941` prepends
`var="<--v5w>"` for every alias the text references, matching the
`${V5W:-…}` brace form), so the header's `:-` default never fires and `W`
resolves to the driver's checkout. **Correct today by the prepend alone.**

**A second finding the lane must respect — the backstop does NOT see this
spelling.** Probing the driver's own regexes (read-only python over the
literal line):

```
line = "N=~/.nvm/versions/node/v24.13.1/bin ; W=${V5W:-$(git rev-parse --show-toplevel)}"
ALIAS_ASSIGN matches:         []                      ← the value has spaces; `val` stops at whitespace
CROSS_ALIAS_DEFAULT matches:  [('W', 'V5W')]           ← the self-test sees it
neutralize_aliases(line) ->   <line unchanged>, changed=[]
sanctioned "V5W=${V5W:-$HOME/source/quilltap-v5}" -> V5W="/probe/worktree", changed=[]   (settled, silent)
"V5W=${V5W:-$(git rev-parse --show-toplevel)}"     -> <unchanged>; CROSS: [('V5W','V5W')] (passes the self-test, still outside the backstop)
```

So a minimal rename `W`→`V5W` that KEEPS `$(git rev-parse --show-toplevel)`
would turn the self-test green while leaving the statement un-neutralized
(safe only by the prepend, exactly today's situation). The order must
prescribe the **sanctioned literal** `V5W=${V5W:-$HOME/source/quilltap-v5}`,
which `ALIAS_ASSIGN` matches and rewrites (quietly, as "settled"). A hand
runner also loses nothing: `$HOME/source/quilltap-v5` is the checkout, whereas
`$(git rev-parse --show-toplevel)` evaluated BEFORE the `cd ~/source/quilltap-server`
line gives whichever repo the human happened to be standing in.

After the fix, `--show` for each family prints the same stages with the
assignment line now `N=… ; V5W="/Users/csebold/source/quilltap-v5"` and the
`W="…"` prepend gone (no `$W` left to trigger it) — the lane diffs the two
`--show` outputs per family and expects exactly those substitutions.

### C5. The `.ts` case headers, and when this landed

The five `.ts` cases spell the placeholder form, not the cross form:
`harness/oracle/cases/doc-opacity.test.ts:42`,
`instance-settings-json-warns.test.ts:26`, `scenario-builder-mount-pool.test.ts:34`,
`scenario-builder-tier3.test.ts:50`, `scenario-builder-routes.test.ts:33` — each
`N=~/.nvm/versions/node/v24.13.1/bin ; W=<this worktree>` with `$W/` in the `cp`
lines (and `:39-40` of the routes case the same two `QT_FIXTURE_SBR_*=$W/…`
prefixes). `PLACEHOLDER_WORKTREE` (`recipe_sweep.py:213-215`,
`<[^<>]*(?:worktree|checkout|repo-root|this tree|v5)[^<>]*>`) rewrites the
placeholder, and `CROSS_ALIAS_DEFAULT` does not match it — they PASS the
self-test and are prose-only (the driver regenerates from the `.rs` headers;
`restored_from` a `.ts` header applies only when the `.rs` header is elided,
which none of these are). Aligning them to `V5W=${V5W:-…}` + `$V5W/` is
optional consistency; the order can take it in the same sed pass or leave it.

Provenance (`git log -S`): the self-test rule landed `5f631d70e` (2026-08-20,
"fix(harness): normalize() neutralizes any checkout-alias assignment (P4.53)");
the five headers landed AFTER it — `doc_opacity_equivalence.rs` in `0cf80af6b`
(2026-09-18, P4.D200), then copied forward into
`scenario_builder_mount_pool_equivalence.rs` (`e754b1788`),
`scenario_builder_tier3_equivalence.rs` (`1d13694cc`),
`scenario_builder_routes_equivalence.rs` (`0c2bbc319`, all 2026-09-23, P4.D217 —
note the three scenario headers also reuse the doc-opacity FIXTURE BUILDER,
`build-doc-opacity-fixture.ts`) and `instance_settings_json_warns_equivalence.rs`
(`0e4a00dd9`, 2026-09-24, P4.113). The self-test has therefore been red since
2026-09-18 — no gate in that span ran `--self-test` (CLAUDE.md's baseline bullet
names it as a guard; the round records' gates list fmt/clippy/sweep/Tier R,
never it). A one-line gate addition is a candidate rider (§E).

---

## §D — Traps and the regen list

- **The committed-recipe rule.** A committed header never names a `/tmp` pin
  or a checkout: `recipe_sweep.py:914-920` — "A lane whose baseline is behind
  v4 HEAD passes its pinned worktree here [`--v4`]; the recipes themselves
  never name a pin, by policy." The five headers' own prose honours it
  (`doc_opacity_equivalence.rs:50-52`). Fix (i) keeps `cd ~/source/quilltap-server`
  as the committed v4 path (the driver's `--v4` rewrite target, `:917-918`);
  the lane's pin lives on the command line only.
- **`editing-source-while-a-sweep-runs-fails-a-family-for-nothing`** (memory
  note): the driver compiles per family; an edit mid-sweep reddens whichever
  family was building with an `error[E…]` that is not a differential red.
  Fix (g)'s harness edit lands BEFORE the lane's regens, and the five
  families of (i) are re-run by name AFTER the header sed — never while a
  sweep owns the tree.
- **Fixtures these items touch, and every reader (re-run each by name
  through the driver):**
  - `harness/oracle/fixtures/memory-pipeline-jobs-tier3.json` (fix 1 only) —
    readers: `crates/quilltap-harness/tests/memory_pipeline_jobs_tier3_equivalence.rs`,
    `harness/oracle/cases/memory-pipeline-jobs-tier3.test.ts`,
    `harness/oracle/fixtures/build-memory-pipeline-jobs-fixture.ts` (the builder
    reads `chats`/`characters`, not `completionRules`, so the `/tmp` DB pair
    is unchanged by fix 1 — but the recipe rebuilds it per run regardless).
    One family: `memory_pipeline_jobs_tier3_equivalence`.
  - Fix 2 touches NO fixture — only the `.rs` test.
  - (i) touches NO fixture — five `.rs` headers (+ optionally five `.ts`
    headers). Families to re-run by name: `doc_opacity_equivalence`,
    `instance_settings_json_warns_equivalence`,
    `scenario_builder_mount_pool_equivalence`, `scenario_builder_tier3_equivalence`,
    `scenario_builder_routes_equivalence` — and `--self-test` (expected
    `self-test: 0 failure(s)`, exit 0) and `--show <family>` ×5 diffed
    against the pre-fix output.
  - The three sibling families (`fold_episode_tier3_equivalence`,
    `context_summary_service_tier3_equivalence`, `courier_images_routes_equivalence`)
    are NOT touched; they are the lane's neutrality set if fix 1's reply is
    taken (zero source change expected → they stay green).
- **`identical-canned-keys-share-the-first-recorded-answer`** (memory note):
  the MPJ oracle keeps the FIRST answer per key (`:230`). Fix 1's one
  `episode` rule answers both folds, but their keys differ (different
  `DATED TURNS` windows: 24 vs 2 messages), so each fold records its own row
  with the same reply — not the trap. The trap WOULD bite if a future case
  folded a byte-identical window; the fix-2 set-equality assert is what
  would then show one key where two were expected. Per the note, count the
  `canned` rows whose system prompt starts `You are consolidating a batch` in
  the fresh NDJSON: expect **2**.
- **`a-canned-miss-in-a-no-op-run-is-invisible`** (memory note, both
  instances): the red-first proof for (g) is the WHOLE pre-port file restored
  (`git show <pre-P4.D242>:crates/quilltap-core/src/memory_tasks/prompt_text.rs`),
  not a one-line mutation — and fix 2's `want − got` must list exactly the
  two episode keys.
- **The `--self-test` probe prints a notice line even when green** —
  `[selftest_alias] alias assignment neutralized to --v5w (/probe/worktree): W=${V5W:-$HOME/source/quilltap-v5}`
  is the self-test's own probe asserting the notice fires (`:1888-1898`); it
  is not a header and will still print after the fix. The pass criterion is
  `self-test: 0 failure(s)` / exit 0.
- **`--list` prints only problem families** (today: `avatar_rolls_routes` and
  `generator_sse_wire` as `elided_or_missing_regen`, both pre-existing and
  unrelated; totals `ok: 500, ok_restored: 80, no_oracle: 17,
  committed_corpus: 15, exempt: 6, non_extractable: 2`), so registration of
  the nine families here is proven by `--show`, not `--list`.
- **Absent v4 lines (aside, out of lane scope):** v5's
  `fold_episode_pass.rs` carries none of v4's four `[FoldEpisodePass]` lines
  (`fold-episode-pass.ts:103/201/210/218`), nor `context-summary.ts:566`'s
  `[Context Summary] Fold episode pass failed:`. A core smalls candidate; once
  the warn exists, a capture pin (silence leg on a hit) becomes a third guard.
  Also noted in passing: v4 counts `SKIP_GATE` in `memoriesWritten`
  (`fold-episode-pass.ts:157`) where v5 counts only `Insert | InsertRelated`
  (`fold_episode_pass.rs:273-276`) — invisible to every family (the result
  struct is discarded), recorded here only.
- **(i) is not a sweep hazard today**, only a self-test one: the prepend
  makes the five recipes stage from the driver's `--v5w` in a worktree sweep.
  The lane should still re-run the five by name in its worktree to show the
  rewritten headers stage identically (compare the `STAGE` copies' md5 to the
  worktree files, the P4.53 symptom).

---

## §E — What the order should say

**Tier 1 (must):**
1. (g) fix 2: the `KeyRecording` wrapper in
   `memory_pipeline_jobs_tier3_equivalence.rs`, set-equality of hit keys vs the
   oracle's `canned` rows with both differences printed; red-first against the
   whole pre-P4.D242 `prompt_text.rs` (exactly the two episode keys in
   `want − got`), green on main; no fixture change, no regen needed (regen
   anyway at the pin for the record).
2. (i) the five `.rs` headers rewritten to the sanctioned literal
   `V5W=${V5W:-$HOME/source/quilltap-v5}` with every `$W/` → `$V5W/` (incl.
   the routes family's two `QT_FIXTURE_SBR_*=$W/…` prefixes); `--self-test`
   exits 0; `--show` ×5 diffed (only the assignment and `$W`→`$V5W` move; the
   `W="…"` prepend disappears); the five families re-run by name through the
   driver in the worktree. The order must say explicitly: NOT
   `V5W=${V5W:-$(git rev-parse --show-toplevel)}` (passes the self-test, stays
   outside `ALIAS_ASSIGN`).

**Tier 2 (should):**
3. (g) fix 1: the non-empty `episode` rule (§B1's shape — absolute `when`,
   distinct narrative, no embeddings), regen, red-first by the same whole-file
   restore, the `vector_entries` tie noted; the three sibling families as the
   neutrality set.
4. The five `.ts` case headers aligned to the same spelling (prose-only;
   optional, same sed).
5. A `--self-test` line added to the lane-gate / unify checklist (the rule has
   been red since 2026-09-18 unnoticed).

**Tier 3 (may / defer loudly):**
6. The `[FoldEpisodePass]` absent lines and the `SKIP_GATE` count — a core
   smalls order, by pointer.
7. A driver-side widening of `ALIAS_ASSIGN` to see a `$(…)`-valued assignment
   (so the backstop covers the spelling the self-test catches) — only if the
   human wants the backstop closed rather than the headers repaired; the
   docstring's "unforgeable" claim is otherwise one spelling short.
