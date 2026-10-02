# Survey — P4.144: the memory + harness smalls (the four `[FoldEpisodePass]` lines + the `SKIP_GATE` count, the `ALIAS_ASSIGN` widening, a v4-side `MigrationRunner` pin, an llm-logs comparand for `memory_pipeline_jobs_tier3`)

**Date:** 2026-10-02 · **v4:** f6426e196 (tree dirty by the three recorded docs paths) · **v5 main:** cb9ecf256 · **Kind:** read-only measurement — source reads of both trees, `git log -L` on v4's fold pass, and one `python3 -B` probe of the driver's `ALIAS_ASSIGN` regex (copied verbatim, plus a widened candidate) over literal strings in `/private/tmp/claude-503/p4144/probe.py`. Nothing built, nothing run against either repo.

Every path is relative to its repo root (v4 = `~/source/quilltap-server`, v5 = this repo).

## The finding in one line

Of v4's five lines only **three are reachable** (`:103` WARN, `:201` WARN, `:210` INFO; `:218` and `context-summary.ts:566` cannot fire, because `runFoldEpisodePass` has nothing left that can throw into them), and the home that can compare all three **as a true differential** is `fold_episode_tier3`, not `memory_pipeline_jobs_tier3`. That family already diffs the pass's result struct, `memoriesWritten` included, and a `Logger.prototype` patch (the `instance-settings-json-warns` precedent) gives v4's own bytes. Red-first on 3 of 4 runs once two runs are added. **The `SKIP_GATE` "count divergence" is not one.** v4's fold pass calls `createMemoryWithGate(…, { userId })`, which can only answer `SKIP_GATE` when `skipGate || skipEmbedding` is set (`memory-service.ts:396-398`), so the arm has been dead since the pass was written (`8bf3cb5f3`). The `ALIAS_ASSIGN` hole is **wider than recorded**: `W=$(git rev-parse --show-toplevel)` gets past both the backstop and the self-test's header scan. A one-regex widening closes both shapes (Tier 2, about 20 lines). The `MigrationRunner` pin would see only half of bug 176's fix, and the drift ledger already watches `migrations/index.ts`, so it stays Tier 3. The MPJ llm-logs comparand has a complete in-tree precedent (`context_summary_service_tier3`, W4.10b) but needs a `with_logging` executor per case. Tier 2.

---

## §A — (1) The `[FoldEpisodePass]` lines, `[Context Summary] Fold episode pass failed:`, and `SKIP_GATE`

### A1. v4's five lines, verbatim (`lib/memory/fold-episode-pass.ts`, `lib/chat/context-summary.ts`)

| # | site | level | message (bytes exact) | fields (order as written) | reachable? |
|---|---|---|---|---|---|
| L1 | `fold-episode-pass.ts:103-106` | `logger.warn` | `'[FoldEpisodePass] Episode extraction failed'` | `{ chatId, error: extraction.error }` | **yes**: only when `!extraction.success` (`:102`). A `[]` reply or a parse failure is `success: true` and silent (`parseFoldEpisodes` swallows a bad JSON body into `[]`, `lib/memory/cheap-llm-tasks/memory-tasks.ts:1167-1204`). `extraction.error` is `getErrorMessage(error)` (`core-execution.ts:660/675/717/741/795`) |
| L2 | `fold-episode-pass.ts:201-205` | `logger.warn` | `'[FoldEpisodePass] Failed to write episode for character'` | `{ chatId, characterId, error: perCharacterError instanceof Error ? perCharacterError.message : String(perCharacterError) }` | **yes**: any throw inside the per-participant `try` (`:132-206`): `createMemoryWithGate` (strict repo writes rethrow, `lib/database/repositories/safe-query.ts:69-70`) or `repos.memories.updateForCharacter` (a strict `safeQuery`, `memories.repository.ts:456-477`, no fallback arg). `findByCharacterAndSourceMessageIds` is a FALLBACK read (`:833-848`, `[]` fallback) and never throws. **A throw ends that character's remaining fragment linking** (the `catch` is outside the link loops) |
| L3 | `fold-episode-pass.ts:210-215` | `logger.info` | `'[FoldEpisodePass] Episode pass complete'` | `{ chatId, episodesExtracted, memoriesWritten, fragmentsLinked }` | **yes**: once per pass whose extraction was non-empty, after the episode loop, even if every write failed |
| L4 | `fold-episode-pass.ts:218-221` | `logger.warn` | `'[FoldEpisodePass] Episode pass failed (non-fatal)'` | `{ chatId, error: error instanceof Error ? error.message : String(error) }` | **no** (see A2) |
| L5 | `context-summary.ts:566` | `logger.error` (3-arg: `(message, context, error)`, `lib/logger.ts:92`) | `'[Context Summary] Fold episode pass failed:'` | `{ chatId }` + the `Error` | **no**: `runFoldEpisodePass`'s outer `try/catch` (`:67`/`:217-223`) means it never rejects, and the only other expressions in the `try` at `:552-563` are `turnsToFold.flatMap` (a local array, `:402`) and property reads on `chat` (non-null, `:338`) |

### A2. Why L4 is unreachable

The only expressions in the outer `try` that sit outside the per-character `try` are these:

- `repos.chats.findById`. This is `_findById`, a fallback `safeQuery` (`base.repository.ts:247-257`, `null` fallback), so it never throws.
- `chat.participants.filter`. Zod defaults it.
- `resolveSpeakerNames`. Its only await is inside its own `try/catch` (`lib/chat/speaker-names.ts:48-51`).
- `windowMessages.map`.
- `extractEpisodesFromFold`. The message build is pure (`memory-tasks.ts:1212-1247`), and `executeCheapLLMTask` returns failures as values.
- `resolveEpisodicAnchors`. A pure function.
- `episode.entities.map(e => e.toLowerCase())`. `parseFoldEpisodes`' `strArray` guarantees strings.

L4 fires only on a defect in a pure helper or in `getRepositories()`. A differential cannot plant that. The precedent is the "UNREACHABLE in v4" lines that P4.134 recorded and did not invent. **Port both as recorded unreachables**: a doc comment at the pass's foot and at the `context_summary.rs` call sites naming the v4 line and why. No `tracing` line, no hunk in `context_summary.rs`.

### A3. v5 today (`crates/quilltap-core/src/services/fold_episode_pass.rs`, 369 lines)

- **Zero `tracing::` lines in the file.** L1, L2 and L3 are all absent.
- L1's site: `:184-187` `match (extraction.success, extraction.result) { (true, Some(episodes)) if !episodes.is_empty() => episodes, _ => return result }`. This collapses `success:false` together with `[]`, with no line on either arm.
- L2's sites are three silent swallows:
  - `:268` `let Ok(outcome) = outcome else { continue };`. A `create_memory_with_gate` `Err` (`memory_gate.rs:240`, `Result<MemoryGateOutcome, DbError>`) is dropped. v4 warns.
  - `:335-337` and `:360-362` `let _ = db.write(… update_for_character …).await;`. v4 throws into the catch, warns, **and stops linking that character's remaining fragments**. v5 keeps linking and still increments `fragments_linked` (`:363`) after a failed write. That is a counted-but-unwritten divergence, invisible today.
  - `:285-289`: the fragment read `Err` → `continue`. v4's read is a fallback (`Error finding memories by character and source message IDs` ERROR, then `[]`, then `fragmentIds.length === 0`, then `continue`). The flow is the same; only the repository ERROR line is missing, and that belongs to the fallback-read class (see Cross-lane).
- L3's site: after the episode loop (`:366`). Absent.
- The chat read `:110` uses the STRICT `chats_read::find_by_id` behind `let Ok(Some(chat)) … else return`. v4 reads through the fallback (`Error finding entity by ID {collection:'chats', id, error}` + `null`). The twin `chats_read::find_by_id_or_none` already exists (`crates/quilltap-core/src/db/chats_read.rs:395-397`). It is a one-line swap and part of the fallback-read class.
- Callers: `crates/quilltap-core/src/services/context_summary.rs:339-351` (`FoldEpisodePassSeams`), `:456-470` (`RealContextSummarySeams`), both `let _ = run_fold_episode_pass(…)`; and the seam call `:909-927`. The seam returns `()` and the pass is infallible, so L5 has no v5 site. **No hunk is needed in `context_summary.rs`**, beyond an optional doc-comment line recording L5 as unreachable. Nothing in this round names `context_summary.rs` for another lane (P4.140 owns `orchestrator.rs`/`carina_query.rs`/`build_context.rs`).
- Field naming convention: the in-tree pin for a v4 fold line is `context_summary_service_tier3_equivalence.rs:276-289`. It asserts a `DEBUG quilltap_core::services::context_summary` prefix and camelCase fields (` chatId=…`, ` seatCount=…`). Copy it: default module target (`quilltap_core::services::fold_episode_pass`), v4's camelCase field names, and `error` rendered with `%`. For L2's DB `Err`, render through `db::fallback::error_text` (`crates/quilltap-core/src/db/fallback.rs:34-39`), which gives v4's bare message rather than `sqlite error: …`. That is the `97b25fc53` smalls-unification lesson.

### A4. `SKIP_GATE`: not a divergence

- v4 `:157` counts `INSERT | INSERT_RELATED | SKIP_GATE`. v5 `:273-278` counts `Insert | InsertRelated`. v5's `GateAction` has **no** `SkipGate` variant (`memory_gate.rs:73-82`, doc: "minus `SKIP_GATE` which is the deferred direct path").
- v4 produces `SKIP_GATE` in exactly one place: `memory-service.ts:396-398`, `if (options.skipGate || options.skipEmbedding) { … return { memory, action: 'SKIP_GATE' } }`. The fold pass passes `{ userId }` (`fold-episode-pass.ts:152`). `git log -L152,158` shows a single commit (`8bf3cb5f3`, 2026-07-21, the pass's introduction), so the options have been `{ userId }` since the pass existed. **The `SKIP_GATE` arm of `:157` is dead code in v4, and v5's count equals v4's on every reachable input.** The only other `SKIP_GATE` reader is `memory-processor.ts:338`.
- **The count is already a comparand.** `fold_episode_tier3` compares the pass's result struct field for field: the oracle writes `{kind:'result', run, result}` at `harness/oracle/cases/fold-episode-tier3.test.ts:264`, and the Rust side checks it at `crates/quilltap-harness/tests/fold_episode_tier3_equivalence.rs:395-402` (`"memoriesWritten": result.memories_written`). The recorded claim "invisible to every family (the result struct is discarded)" holds only for the MPJ and csum families, where the struct goes through `let _`.
- Deliverable: a why-comment at `fold_episode_pass.rs:273` naming v4's dead arm, `memory-service.ts:396-398` and `8bf3cb5f3`, so nobody "fixes" the count by inventing a `SkipGate` variant. No mutation proof is possible, because the arm is unreachable on both sides.

### A5. Which family can see the lines, and how

**`memory_pipeline_jobs_tier3`** (what the brief asked first). It cannot compare log lines today:

- The oracle sets `LOG_LEVEL = 'error'` (`harness/oracle/cases/memory-pipeline-jobs-tier3.test.ts:150`) and patches no `Logger` method.
- The Rust side installs no capture rig.
- Its arms: one `episode` rule (`harness/oracle/fixtures/memory-pipeline-jobs-tier3.json`, now one episode since P4.138 fix 1) serves both folds (`cs_fold`, `cs_force`). Both reach L3. No arm reaches L1 or L2, and the rule is matched by `kind` rather than per case (`:212-218`), so it cannot fail one fold and answer the other.
- It is the wrong home.

**`fold_episode_tier3`** is the right home:

1. It calls `run_fold_episode_pass` directly (`fold_episode_tier3_equivalence.rs:396`). There is no fold around it.
2. It answers **per run** (`currentRun.episodeResponse`, `fold-episode-tier3.test.ts:196-224`).
3. It already diffs `result` and the three tables.
4. The oracle imports v4's module after `jest.resetModules()` (`:150`, `:239`), so a `Logger.prototype` patch taken from `await import('@/lib/logger')` in the same generation sees the pass's `logger` instance (`lib/logger.ts:243` `export const logger = new Logger({…})`). That avoids the `jest-oracle-instanceof-across-resetmodules` trap. `jest.setup.ts` does **not** mock `@/lib/logger` (grep: none).

The precedent for capturing v4's own logger lines in an oracle is `harness/oracle/cases/instance-settings-json-warns.test.ts:79-94`. It patches `Logger.prototype.warn` after the imports and runs under `LOG_LEVEL=error`. The patch wraps the public method, so level filtering, which happens in `log()`, does not hide the call.

The proposed shape:

- **Oracle.** Patch `Logger.prototype.warn`/`info` to push `{level, message, context}` for messages starting `[FoldEpisodePass]` into a per-run sink, then emit `{kind:'logs', run, lines:[…]}` after each run. Add an optional `fail` field per run: when set, the `sendMessage` mock records the key with `fail` and **throws** it (the csum `fail` shape, `context-summary-service-tier3.test.ts:113,297-298`).
- **Rust.** Wrap each run in `quilltap_core::test_support::global_capture::capture_async` (`crates/quilltap-core/src/test_support.rs:194,306`; the csum family's `capture_lines`, `:140-151`). Filter lines containing `[FoldEpisodePass]`, parse the level, the message and the fields, and compare to the oracle's lines. Register `fail` rows through `CannedCompletionProvider::with_failure` (`crates/quilltap-core/src/model/completion.rs:570`).

**Corpus (two new runs, two new chats in `build-fold-episode-fixture.ts`):**

- `episode_fail`: `fail: "<message>"`. Fires L1 with `error` = the thrown message. The v5 executor passes `error.message` through (`cheap_llm_exec.rs:1288-1294`). v4's `getErrorMessage` does the same, but the cheap fallback-chain walk sits between them on v4 (`core-execution.ts:699-741`) and is absent on v5 for a bare `new()` executor. **Measure** that the `error` bytes agree, and record it if v4's chain rewrites the message.
- `episode_write_fail`: a non-empty reply with a marker narrative, and a fixture-builder TRIGGER `CREATE TRIGGER qt_plant_episode_write BEFORE INSERT ON memories WHEN NEW.content = '<marker>' AND NEW.characterId = '<one of two present characters>' BEGIN SELECT RAISE(ABORT, '<plant message>'); END;`. Both sides open the same `/tmp` fixture, so they see the same trigger. Expected: L2 once (that character), L3 with `memoriesWritten: 1`, and the other character's episode written. That is also the first arm where `memoriesWritten < present × episodes` on a write failure.
  - **Risk to measure:** v4's error text is the SqliteError message. v5's must go through `db::fallback::error_text`, and a rusqlite `SqliteFailure` Display may append a code. Read the first regen before pinning.
  - **Second risk:** whether either side writes the `vector_entries` row before the failing insert. The table comparand will show it, so record the measured order.
- To drive L2 through the UPDATE path instead (the `let _` swallows), a `BEFORE UPDATE OF relatedMemoryIds` trigger on a seeded fragment works the same way. Optional: it proves the "stop linking" half of v4's catch, which today makes v5's `fragments_linked` over-count.

**Red-first** (v5 `main` against the oracle after the corpus grows):

| run | v4 lines | v5 today | red? |
|---|---|---|---|
| `episode_pass` | L3 `{episodesExtracted:2, memoriesWritten:N, fragmentsLinked:M}` | none | **red** (L3 absent) |
| `no_episodes` | none (`[]` = success, silent) | none | green (the silence leg) |
| `episode_fail` | L1 | none | **red** |
| `episode_write_fail` | L2 + L3 | none (and the `result`/tables may also differ if v5 links after a failed update) | **red** |

That is 3 of 4 runs red, with one `#[test]` that fails fast. Prove the count from the oracle NDJSON (memory note `one-test-family-fail-fast-prove-the-count-from-the-oracle`): count `kind:'logs'` lines with non-empty `lines`. Expect 3.

**Mutation proofs after the port:**

- (M1) Drop the L1 arm, i.e. restore `_ => return result`: `episode_fail` reds.
- (M2) Turn `:268` back into `continue` without the warn: `episode_write_fail` reds.
- (M3) Move L3 inside the per-episode loop: `episode_pass` reds on the count (2 lines against 1).
- (M4) Log L3 on the `[]` path: `no_episodes` reds.
- (M5) Render `error` with `{:?}` or the `sqlite error:` prefix: `episode_write_fail` reds on the field.

**Neutrality set** (zero behaviour change apart from logging, plus the update-failure early exit, which no existing arm reaches): `context_summary_service_tier3_equivalence`, `memory_pipeline_jobs_tier3_equivalence`, `courier_images_routes_equivalence`.

Optional extra guard: an L3 capture pin in MPJ for `cs_fold`/`cs_force`, with the bytes copied from the hunk. It adds little once `fold_episode_tier3` holds the true differential. Tier 3.

---

## §B — (2) The `ALIAS_ASSIGN` widening (`harness/tools/recipe_sweep.py`)

### B1. Today

- The regex at `:843-848`:

  ```
  (?P<lead>^|;)(?P<sp>[ \t]*)(?P<var>V5W|WT|V5|W)=(?P<val>(?:"[^"\n]*"|'[^'\n]*'|[^\s;#])+)(?P<tail>[ \t]*(?:#[^\n]*)?)(?=$|;)
  ```

  Aliases come from `CHECKOUT_ALIASES` (`:821`). The value cannot contain unquoted whitespace, and the lookahead stops an env prefix being swallowed (`:838-842`).
- It is used once, at `:906`, in `neutralize_aliases`. That function is called from `normalize()` at `:921`, and settled values stay silent through `alias_value_is_settled` (`:863-887`).
- The docstring at `:66-74` says "ALIAS ASSIGNMENTS ARE UNFORGEABLE (P4.53) … rewrites ANY `V5W=`/`WT=`/`V5=`/`W=` assignment statement".
- Self-test coverage is at `:1879-1931`: the brahma clobber line, the announced-verbatim check, the `${V5W:-…}` reference injection, two env prefixes untouched, four non-alias assignments untouched, the `;`-joined form, and three settled shapes quiet. The committed-header scan at `:1933-1966` uses `CROSS_ALIAS_DEFAULT` (`:854-856`, `var=${from:-`) and flags `var != from`.

### B2. The probe (driver regex copied verbatim; read-only)

| line | `ALIAS_ASSIGN` today | header scan | widened |
|---|---|---|---|
| `N=… ; W=${V5W:-$(git rev-parse --show-toplevel)}` | **no match** | caught (`W`≠`V5W`) | match |
| `V5W=${V5W:-$(git rev-parse --show-toplevel)}` | **no match** | not caught (self form) | match → settled, silent |
| `W=$(git rev-parse --show-toplevel)` | **no match** | **not caught** (no `${`) | match → announced |
| ``W=`git rev-parse --show-toplevel` `` | **no match** | **not caught** | match → announced |
| `W=$(cd "$(dirname x)" && pwd)` | no match | not caught | match |
| `W=$(git rev-parse --show-toplevel) npx jest -- x` (env prefix) | no match | — | **no match** (correct) |
| `V5W=${V5W:-$(git …)} cargo test` (env prefix) | no match | — | **no match** (correct) |
| the existing 9 self-test rows (clobber, `;` form, non-aliases, prefixes, settled) | as asserted | — | **unchanged** |

The tree today has **zero** `$(…)`- or backtick-valued alias assignments: `grep 'rev-parse --show-toplevel'` over `crates/*/tests` and `harness/oracle/cases` finds 0, because P4.138 repaired the five. So the widening moves no committed recipe, and `--show` stays byte-stable for every family.

### B3. The minimal widening

Extend the `val` atom alternation with three more atoms:

- a one-level-nested command substitution: `\$\((?:[^()\n]|\([^()\n]*\))*\)`
- a backtick substitution: `` `[^`\n]*` ``
- a brace expansion that may contain a command substitution: `\$\{(?:[^{}\n]|\$\((?:[^()\n]|\([^()\n]*\))*\))*\}`

Leave `lead`/`tail` and the statement-end lookahead as they are. That keeps the env-prefix guard, which is the one way the backstop could do real harm.

Rewrite the docstring's "ANY … assignment statement" to state the value grammar it now covers, and say plainly that a deeper nesting is still outside it. This is the "carry a comment naming the sha where the prose misleads" rule.

### B4. New self-test rows (next to `:1879-1931`)

1. `normalize(["N=…", "W=${V5W:-$(git rev-parse --show-toplevel)}", "cp $W/x /tmp/y"], probe, "sa3")`: `W="<probe>"` is in the output, `rev-parse` is not, and `neutralize_aliases` reports the statement in `changed` (announced).
2. `V5W=${V5W:-$(git rev-parse --show-toplevel)}` is rewritten to `V5W="<probe>"` with `changed == []` (settled, silent).
3. `W=$(git rev-parse --show-toplevel)` and the backtick form are each rewritten and announced. This is the spelling neither guard sees today.
4. Two `$(…)` env prefixes are unchanged with `changed == []`.

**Mutation proof:** revert the regex and rows 1–3 fail. Revert only the `${…}` atom and rows 1–2 fail while row 3 still passes, which shows each atom bites. **Gate:** `python3 harness/tools/recipe_sweep.py --self-test` → `self-test: 0 failure(s)`, plus `--show` on two or three `$V5W` families diffed before and after (byte-identical).

### B5. Tier

The human's stated preference (headers repaired, backstop optional) makes this **optional**. **I recommend Tier 2**, for two reasons:

- The recorded description understates the hole. It said "one spelling short". In fact `W=$(…)` and `` W=`…` `` are invisible to **both** guards, not only the backstop. Such a header would be a live clobber, because the prepended `W="<--v5w>"` is overwritten by whatever repo the stage's cwd is in, and `--self-test` would stay green.
- The cost is about 20 lines in one file, with no crate bump, no family regen, and a self-contained proof.

Tier 3 is defensible only if the human repeats the preference.

---

## §C — (3) The v4-side `MigrationRunner` pin (bug 176)

### C1. The runner's API (`migrations/index.ts`, 278 lines)

- `export class MigrationRunner` (`:71`), `new MigrationRunner()`, `async runMigrations(): Promise<MigrationRunResult>` (`:83`). It returns `{ success, migrationsRun, migrationsSkipped, results, totalDurationMs, failed? }` (`:221-228`). **`migrationsSkipped` is a bare count with no per-id list**, so "it counts THIS migration" cannot be asserted directly. The usable assertions are: no `results[]` entry for the id, `migrationsRun` unchanged, and the table state afterwards.
- The ledger check comes before `shouldRun` (`:124-129`). `isMigrationCompleted` searches the state loaded once per run (`migrations/state.ts:150-158, 210-212`). The runner needs `waitForDatabaseReady` (`:99`), from `migrations/lib/database-utils`, and its own `migrations/lib/logger`.
- `create-help-doc-chunks-table-v1` (`migrations/scripts/create-help-doc-chunks-table.ts:28-39`) has `shouldRun = isSQLiteBackend() && !sqliteTableExists('help_doc_chunks')` and depends on `create-help-docs-table-v1`.

### C2. The cheapest harness shape is **tsx, not jest**

`harness/oracle/fixtures/build-migration-vintage-fixture.ts:83-100` already drives the real runner from nothing under `node --import tsx` from the v4 checkout: scratch `QUILLTAP_DATA_DIR`, `SQLITE_PATH`, `ENCRYPTION_MASTER_PEPPER`, then `await import('@/migrations/index')` and `new MigrationRunner().runMigrations()`, with 117 migrations recorded at P4.28 (`status-log.md:152217`). There is no `jest.setup`, so:

- the `llm-logging` / `storage-manager` stubs do not apply;
- the `resetModules`/registry trap does not apply (no registry is touched);
- the lazy help ensure does not apply (no migration script imports `help-doc-sync` or `HelpSearch`; grep over `migrations/` finds none).

**The brief's "the jest-oracle traps apply" is moot on this shape.**

A pin script would do the following:

1. Run the chain once from nothing. `create-help-doc-chunks-table-v1` runs and is stamped.
2. Apply `ALTER TABLE help_doc_chunks RENAME TO help_doc_chunks_x` through the v4 checkout's `better-sqlite3`, resolved from `process.cwd()` as in the builder's `:130-137` (the bare-import trap).
3. Run `new MigrationRunner().runMigrations()` a second time.
4. Emit `{kind:'ledgerSkip', id:'create-help-doc-chunks-table-v1', stamped:true, inResults:false, migrationsRun:0, tableExistsAfter:false}`.

A Rust test reads the row and asserts that exact shape. On a flip it fails with "v4 fixed bug 176 — retire the recorded divergence (`host_boot_hardness` cadence arm, `phase-4.md:7150`)".

Use the missing-table plant, not the bug file's renamed-column reproduction. A renamed column is invisible to every `shouldRun` (bug file, "None of these checks would notice a renamed column"), so the pin would never trip.

### C3. Why it stays Tier 3

- **It sees only half of the fix.** The bug file's fix part 1, "Check the shape at boot … the second doing most of the work" (`docs/developer/bugs/bug-176-ledger-skips-shouldrun.md`, "The fix"), lives in `instrumentation.ts`, which `register()` → `process.exit` makes undrivable. Only part 2 (re-asking `shouldRun` for structural migrations) would trip a runner pin. A pin that stays green while v4 ships the main half is a false comfort.
- **The drift process already sees it.** Any v4 fix touches `migrations/index.ts`, `instrumentation.ts` or `dedicated-db.repository.ts`, and `/driftcheck` classifies every v4 commit by file list. The ledger already carries bug 176 (`drift-ledger.md:45-48, 97`).
- **Cost:** two new files (about 80 lines of tsx and about 60 lines of Rust), a recipe header for the sweep driver, a full migration chain run twice per regen (seconds to a minute), and one more pin-required family.

**Recommendation: keep it a named Tier-3 deferral.** If the human wants it anyway, use the tsx shape above, not a jest case.

---

## §D — (4) An llm-logs comparand for `memory_pipeline_jobs_tier3`

### D1. What blocks it today

- **Oracle side.** `logLLMCall: async () => undefined` (`memory-pipeline-jobs-tier3.test.ts:253-260`) is stacked on `jest.setup.ts`'s whole-module no-op (memory note `jest-setup-llm-logging-service-mocked`). No `SQLITE_LLM_LOGS_PATH` is set, and nothing is dumped.
- **Rust side.** `DbPaths { llm_logs: None }` (`memory_pipeline_jobs_tier3_equivalence.rs:490-494`) and `CheapLlmTaskExecutor::new()` (`:499`).
- **⚠ A bare `new()` executor writes no `llm_logs` row at all** (`crates/quilltap-core/src/services/cheap_llm_exec.rs:476-509`: "A bare `CheapLlmTaskExecutor::new()` therefore has no chain, and no `llm_logs` writer either"). Production builds one executor per job through `with_logging(CheapLlmLogConfig { db, user_id, chat_id: Some(job chat), message_id: None, ctx: LogContext::none() })` (`crates/quilltap-host/src/spine.rs:3220-3235` for MEMORY_EXTRACTION, `:3268-3280` for CONTEXT_SUMMARY). **A partition alone would compare nothing.** The harness must build a `with_logging` executor per case, mirroring the spine.

### D2. The precedent, complete in tree

`context_summary_service_tier3` (W4.10b):

- **Oracle:** `SQLITE_LLM_LOGS_PATH = join(scratch, 'cs-llm-logs.db')` (`context-summary-service-tier3.test.ts:174-175`); `jest.doMock('@/lib/services/llm-logging.service', () => jest.requireActual(…))` (`:313-316`); settle, `getRawLLMLogsDatabase()`, then `SELECT * FROM llm_logs` with `id`/`createdAt`/`updatedAt` placeholdered and rows sorted by canonical JSON, emitted as `{kind:'llmlogs', columns, rows}` (`:550-578`).
- **Rust:** `with_logging` at `:808`, `common::materialize_llm_logs` (`crates/quilltap-harness/tests/common/mod.rs:72`), `common::dump_llm_logs` (`:177`), `common::oracle_llm_logs` (`:272`), `common::assert_ruled_failed_call_divergence` (`:139`) for the RULED P4.13 failure-row asymmetry.

`orchestrator_tier3` (P4.129) improves on the settle: it wraps `logLLMCall` to keep every promise and runs `await Promise.allSettled(pendingLogs)` before the dump (`orchestrator-tier3.test.ts:455-473, 894-897`). Copy that rather than the 200 ms sleep, because MPJ's extraction passes are fire-and-forget-heavy. Also copy the `sqlite_master` guard from the memory note: v4 creates `llm_logs` lazily, so a run that logs nothing has no table.

### D3. What the lane would do

**Oracle:**

- set `SQLITE_LLM_LOGS_PATH`;
- replace the `:253-260` no-op with the orchestrator's pending-drain wrapper;
- dump `llmlogs` after the case loops, guarded on `sqlite_master`.

**Rust:**

- create a fresh llm-logs partition (`materialize_llm_logs`) and set `llm_logs: Some(…)`;
- build one `with_logging` executor **per case** with `chat_id: Some(case.chat_id)`, as the spine does;
- dump and diff after the table diffs.

**Measure first:**

- The executor's `profiles_without_custom_temp` cache becomes per-case instead of per-run. v4's is module-global for the whole jest run. It is neutral here because no MPJ call is temperature-rejected, but record it.
- Attaching `with_logging` also arms the cheap **fallback chain** (`cheap_llm_exec.rs:518-530`). That is neutral if the fixture's profiles carry no fallbacks. Verify against `memory-pipeline-jobs-tier3.json`'s `connectionProfiles`.
- The `type`/`characterId`/`messageId` columns of the MEMORY_EXTRACTION rows. These are a first comparison and may surface a **real** divergence in the memory-extraction logging. If one lands in another lane's file, escalate it rather than fixing it in place.

**What it adds over P4.138's `KeyRecording`:** the key set already proves request bytes. The new coverage is the response, usage, `type`, `chatId`/`characterId` attribution, and a second consumption guard: a canned miss now writes a v5 error row v4 lacks, which is the P4.13 asymmetry, so `assert_ruled_failed_call_divergence` must see ZERO such rows here.

**Red-first:** none expected (a new comparand over presumably faithful code). The proof is mutation: (M6) drop the `chat_id` from the per-case log config and the rows red on `chatId`; (M7) a one-byte change to the fold prompt and both the key set and the llm-logs rows red.

**Tier: 2.** The precedent makes it mechanical (about 60 lines across the two files plus a regen at the pin), and it closes the last unattributed surface of the family. Downgrade it to Tier 3 if the round is long.

---

## What the recorded description got wrong

1. **"plus the `SKIP_GATE` count divergence (`memoriesWritten`)"** (P4.138 Tier 3 item 7, survey §D, the round's §S): **there is no divergence.** v4's `SKIP_GATE` arm at `fold-episode-pass.ts:157` is unreachable: the pass passes `{ userId }`, and `SKIP_GATE` needs `skipGate || skipEmbedding` (`memory-service.ts:396-398`), unchanged since `8bf3cb5f3`. v5's two-action count equals v4's on every reachable input.
2. **"invisible to every family (the result struct is discarded)"** (P4.138 survey §D): false for `fold_episode_tier3`, which diffs `memoriesWritten` field for field (`fold_episode_tier3_equivalence.rs:395-402` ← oracle `:264`). It is discarded only by the two fold seams (`context_summary.rs:345, 465`).
3. **"v4's four absent `[FoldEpisodePass]` lines" + `context-summary.ts:566`** as five lines to port: only **three** are reachable (L1, L2, L3). L4 (`:218`) and L5 (`:566`) cannot fire (§A2). Port them as recorded unreachables, not as lines.
4. **`context-summary.ts:566` is ERROR, not WARN**, and logs `(message, {chatId}, Error)` (3-arg `logger.error`, `lib/logger.ts:92`). The trailing colon is part of the bytes.
5. **"the `[FoldEpisodePass] Episode extraction failed` warn … fires on the failure arm"**: correct, but the recorded "miss vs `[]`" framing misses that a **parse failure** (`"not json at all"`) is also silent in v4. `parseFoldEpisodes` returns `[]` from its own catch (`memory-tasks.ts:1201-1203`), so csum's `fold_with_librarian_sweep` arm reaches neither L1 nor L3.
6. **The L2 port is not a log line alone.** v4's per-character catch also **stops that character's fragment linking** after a failed update. v5's `let _ =` writes keep linking and still count `fragments_linked` (`fold_episode_pass.rs:335-337, 360-363`). That is a behaviour divergence the recorded items never named.
7. **"`ALIAS_ASSIGN` … the docstring's 'unforgeable' is one spelling short"** (P4.138 Tier 3 item 8): it is short by a class. `W=$(…)` and `` W=`…` `` get past **both** the backstop and the self-test's header scan (§B2), while the recorded `W=${V5W:-$(…)}` spelling was at least caught by the header scan.
8. **"a jest case … (the jest-oracle traps: registry/resetModules, the lazy help ensure)"** (P4.135 Tier 3 item 11, survey §C.2(d)): the cheapest shape is a **tsx** script (the `build-migration-vintage-fixture.ts` precedent), where neither trap applies. "Renamed" works for `help_doc_chunks` only as a table rename, because a column rename is invisible to every `shouldRun`. And "would trip when v4 fixes 176" holds only for fix part 2, not the bug file's main part 1 (§C3).
9. **"needs the oracle to stop no-op'ing `logLLMCall` and a third partition"** (P4.138 Tier 3 item 9): a third requirement is missing. The v5 executor must be `with_logging`; a bare `new()` writes no `llm_logs` rows whatever the partition (`cheap_llm_exec.rs:476-509`).

Re-verified as stated: v4's L1/L2/L3 bytes and fields; `fold_episode_pass.rs` has zero `tracing::` lines; the MPJ oracle no-ops `logLLMCall` (`:253-260`) and the harness passes `llm_logs: None` (`:493`); `ALIAS_ASSIGN` at `:843-848`, used at `:906`, docstring `:66-74`; the five P4.138 headers are repaired (0 `rev-parse` hits in tree); `MigrationRunner` is exported from `migrations/index.ts:71` with `migrationsSkipped` in its result.

## Proposed tiered deliverables

### Tier 1 — must land

1. **L1/L2/L3 in `fold_episode_pass.rs`, at v4's levels and bytes, with fields in v4's order and camelCase names.**
   - L1 on the `!success` arm only, with the `[]` arm silent. Split the `:184-187` match into v4's two conditions.
   - L2 as ONE per-participant fallible block: `create_memory_with_gate` `Err` and either `update_for_character` `Err` → L2 with `error` = `db::fallback::error_text`, then **skip the rest of that participant** (v4's catch placement), so `fragments_linked` stops over-counting.
   - L3 after the episode loop.
   - Doc comments recording L4/L5 as unreachable (§A2) and the dead `SKIP_GATE` arm (§A4).

   **Differential:** `fold_episode_tier3_equivalence`.
   - The oracle gains a `Logger.prototype` warn/info capture → `{kind:'logs'}` per run, and a per-run `fail`.
   - The corpus gains `episode_fail` and `episode_write_fail` (two new chats and a trigger plant in `build-fold-episode-fixture.ts`).
   - The Rust side gains `global_capture::capture_async`, the line comparison and `with_failure` registration.
   - Red-first on 3 of 4 runs, counted from the NDJSON; mutations M1–M5; neutrality: `context_summary_service_tier3`, `memory_pipeline_jobs_tier3`, `courier_images_routes`.
2. **The `SKIP_GATE` ruling recorded:** no code change, the why-comment at `:273`, and a line in the lane record retiring the "divergence".

### Tier 2 — should land

3. **The `ALIAS_ASSIGN` widening** (§B3) with self-test rows 1–4 (§B4), the docstring rewritten, the mutation proof, and `--self-test` exit 0. Proof: the self-test, plus `--show` byte-stable on three families.
4. **The MPJ llm-logs comparand** (§D3): the oracle un-mocked with the pending drain and an `llmlogs` dump; the Rust side with a partition, a per-case `with_logging` executor and the dump/diff; `assert_ruled_failed_call_divergence` expecting zero failure rows; M6/M7. Regenerate at the pin.
5. **The pass's two reads moved to v4's fallback shape** (`chats_read::find_by_id_or_none` at `:110`; the fragment read through `db::fallback::find_by_filter_or_empty` or a `memories_read` twin), if the round's ownership table gives these two sites to P4.144 rather than to P4.142's census (see Cross-lane). Proof: a capture leg on a renamed-column plant, or a census entry.

### Tier 3 — loud deferrals

6. **The `MigrationRunner` ledger-skip pin**: tsx shape (§C2), deferred because it sees only fix part 2 and the drift ledger already watches the files (§C3).
7. An L3 capture pin in MPJ (`cs_fold`/`cs_force`), redundant once item 1's true differential exists.
8. v4's repository-level lines inside `updateForCharacter` (`Memory not found for update`, `Memory does not belong to character`, and the strict `Error updating memory for character` ERROR, `memories.repository.ts:456-477`). v5's `update_for_character` (`crates/quilltap-core/src/db/memories.rs:323-333`) logs none of them. This is a repository-class item and is unreachable from the fold pass with valid ids.

## Files the lane would edit

**Tier 1:**
- `crates/quilltap-core/src/services/fold_episode_pass.rs`
- `crates/quilltap-core/Cargo.toml` (version bump)
- `crates/quilltap-harness/tests/fold_episode_tier3_equivalence.rs` (EDIT: capture rig, `logs` comparand, `fail` registration, header prose + recipe; REGENERATED at the pin)
- `harness/oracle/cases/fold-episode-tier3.test.ts` (Logger patch, per-run `fail`, `logs` rows)
- `harness/oracle/fixtures/fold-episode-tier3.json` (two runs; `fail`; marker narrative)
- `harness/oracle/fixtures/build-fold-episode-fixture.ts` (two chats + the `memories` trigger plant; its `/tmp` fixture pair is REBUILT, nothing committed)
- `crates/quilltap-harness/Cargo.toml` (version bump)

**Tier 2:**
- `harness/tools/recipe_sweep.py` (regex `:843-848`, docstring `:66-74` + the `:825-842` comment, self-test rows near `:1879-1931`)
- `harness/oracle/cases/memory-pipeline-jobs-tier3.test.ts` (the `:253-260` mock, the env, the `llmlogs` dump)
- `crates/quilltap-harness/tests/memory_pipeline_jobs_tier3_equivalence.rs` (partition, per-case `with_logging`, dump/diff; REGENERATED at the pin)
- (item 5 only) `crates/quilltap-core/src/db/memories_read.rs`, if a fallback twin for `find_by_character_and_source_message_ids` is added there. Otherwise there is no edit outside `fold_episode_pass.rs`.

**Tier 3, only if taken:**
- `harness/oracle/cases/migration-runner-ledger-skip.ts` (NEW, tsx)
- `crates/quilltap-harness/tests/migration_runner_ledger_skip_equivalence.rs` (NEW)

**Docs:**
- `docs/developer/porting/status-log.md` (lane record)
- `docs/CHANGELOG.md`
- `docs/developer/porting/work-orders/p4.144-*.md` (Unification/Status at close)

**Families regenerated:** `fold_episode_tier3_equivalence` (Tier 1) and `memory_pipeline_jobs_tier3_equivalence` (Tier 2). **Families re-run for neutrality:** `context_summary_service_tier3_equivalence`, `courier_images_routes_equivalence`, `memory_pipeline_jobs_tier3_equivalence` (if item 4 is not taken). **Committed fixtures rebuilt:** none (both families build `/tmp` pairs).

## Files the lane must READ but not edit

- v4: `lib/memory/fold-episode-pass.ts`, `lib/chat/context-summary.ts:520-600`, `lib/memory/memory-service.ts:360-400`, `lib/memory/cheap-llm-tasks/memory-tasks.ts:1160-1250`, `lib/memory/cheap-llm-tasks/core-execution.ts:640-800`, `lib/database/repositories/memories.repository.ts:456-477, 833-848`, `lib/database/repositories/safe-query.ts:51-72`, `lib/database/repositories/base.repository.ts:247-257`, `lib/logger.ts:68-130, 243`, `migrations/index.ts`, `migrations/state.ts:150-240`, `docs/developer/bugs/bug-176-ledger-skips-shouldrun.md`.
- v5 core: `crates/quilltap-core/src/services/context_summary.rs:250-470, 880-930` (the seams; **no edit**), `services/memory_gate.rs:70-100, 240`, `services/cheap_llm_exec.rs:465-530, 1240-1295`, `db/fallback.rs`, `db/chats_read.rs:384-397`, `db/memories.rs:323-333`, `db/memories_read.rs:640-660`, `test_support.rs:95-135, 194-320`.
- v5 harness: `crates/quilltap-harness/tests/common/mod.rs` (`materialize_llm_logs`/`dump_llm_logs`/`oracle_llm_logs`/`assert_ruled_failed_call_divergence`), `context_summary_service_tier3_equivalence.rs:130-290, 760-1060` (capture + llm-logs precedent), `harness/oracle/cases/context-summary-service-tier3.test.ts:100-320, 545-580`, `harness/oracle/cases/orchestrator-tier3.test.ts:455-473, 890-925` (P4.140's family; pattern only), `harness/oracle/cases/instance-settings-json-warns.test.ts:60-100` (Logger patch precedent), `harness/oracle/fixtures/build-migration-vintage-fixture.ts` (tsx runner precedent), `crates/quilltap-host/src/spine.rs:3215-3285` (production executor construction), `crates/quilltap-host/tests/host_boot_hardness.rs:768-800` (the bug-176 cadence arm).
- Memory notes: `jest-setup-llm-logging-service-mocked`, `jest-oracle-instanceof-across-resetmodules`, `one-test-family-fail-fast-prove-the-count-from-the-oracle`, `a-canned-miss-in-a-no-op-run-is-invisible`, `tracing-percent-field-renders-unquoted`, `a-sorted-field-capture-cannot-see-v4-field-order`, `an-oracle-case-outside-the-v4-tree-cannot-bare-import-a-v4-dependency`, `editing-source-while-a-sweep-runs-fails-a-family-for-nothing`.

## Cross-lane adjacencies / risks

- **P4.142 (repository fallbacks / the 130-site census).** `fold_episode_pass.rs` holds two fallback-class reads: the strict chat read at `:110`, where v4 logs `Error finding entity by ID` and the twin `chats_read::find_by_id_or_none` already exists; and the fragment read at `:285-289`, where v4 logs `Error finding memories by character and source message IDs` and there is no twin. If P4.142's census converts sites file by file, it will want to touch `fold_episode_pass.rs`, which this round assigns to P4.144 alone. **The round planner must assign these two sites to one lane.** Recommendation: P4.144 converts them (it owns the file), and P4.142's census lists them as done by P4.144. Any new `memories_read` twin would be P4.142's home shape. Coordinate if P4.142 also adds one.
- **P4.140 (Carina `CHAT_MESSAGE` llm-logs; `orchestrator_tier3`).** Item 4 copies `orchestrator-tier3.test.ts`'s pending-drain wrapper, read-only. There is no shared file. Both lanes may touch `common/mod.rs`'s llm-logs helpers only by calling them; neither should edit them. Flag it if one does.
- **P4.140 (Option V).** No `context_summary.rs` edit is planned here. If Option V threads a `TimeZone` through the fold seams (`RunFoldEpisodePassInput` is built at `context_summary.rs:909-927`), adding a field to `RunFoldEpisodePassInput` would cross into `fold_episode_pass.rs`. Record that as a P4.140 → P4.144 hunk if it happens. The pass reads no zone today (clock = `createdAt` strings).
- **P4.139.** None. **P4.141.** None. **P4.143.** None. **P4.145.** None.
- **Sweep hygiene:** item 3 edits `recipe_sweep.py`, which every lane runs. Land it as its own commit, and never while a sweep is in flight (memory note `editing-source-while-a-sweep-runs…`).
- **Risk (item 1):** the `error` bytes for L1 under v4's cheap fallback-chain walk, and for L2 under rusqlite's `SqliteFailure` Display. Measure both on the first regen before pinning. Neither is predictable from source alone.

## Versions / bumps

- `quilltap-core`: 0.0.1146 → +1 (Tier 1, `fold_episode_pass.rs`; again if item 5 adds a `memories_read` twin).
- `quilltap-harness`: 0.0.1074 → +1 per commit touching its tests (Tier 1; Tier 2 item 4; Tier 3 item 6 if taken).
- `recipe_sweep.py` is no crate, so no bump. host/web/cli/tauri/SPA are unchanged.
