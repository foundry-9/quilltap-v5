# Survey — P4.151: harness / test-craft smalls (2026-10-05)

Measured read-only on v5 `main` `6c3a3c635` and v4 `07b8f0209` (clean). No
cargo / npm / jest / tsx was run; every count below is a `grep`/`sed` count.
"NOT MEASURED" marks what only a run can settle.

Every item is **test or fixture work**. Core hunks are predicted for **none of
them**: two items need no source edit at all, and the one place a core edit
looked necessary (A1) has a harness-only shape (§A1.5).

---

## A1. No oracle row runs v4's real `ChatSettingsSchema` over a stored `'maybe'`

1. **Item** (`work-orders/p4.d251-…md:5`, Unification): "no oracle row runs
   v4's real `ChatSettingsSchema` over a stored `'maybe'` (the unit test's Zod
   bytes are a measured literal — a `repository-zod-messages.ts` row)". The
   same item is in `status-log.md` at the `07b8f0209` unification record
   (~165963, "Recorded, not fixed (the follow-ups smalls round)").
2. **v5 today:**
   - Production: `crates/quilltap-core/src/db/chat_settings.rs:1538-1572`
     (`find_by_user_id`). When the `impersonationVoiceMode` cell is a string
     that `ImpersonationVoiceMode::parse` rejects, it builds ONE
     `ZodIssue::invalid_value(&ImpersonationVoiceMode::VALUES, ["impersonationVoiceMode"])`
     and logs `ERROR quilltap::db Data validation failed collection=chat_settings error=<zod>`.
     It then answers `None` through `db::fallback::find_one_by_filter_or_none`,
     which logs `Error finding entity by filter` with the same bytes.
     `VALUES` is at `:841` (`["off", "ask", "always"]`).
   - The literal: the unit test
     `find_by_user_id_voice_mode_null_reads_off_and_an_unknown_value_drops_the_row`
     at `chat_settings.rs:2223-2290`. The Zod string is hard-coded at `:2277`
     (`[\n  {\n    "code": "invalid_value",\n    "values": [ "off","ask","always" ] …, "message": "Invalid option: expected one of \"off\"|\"ask\"|\"always\""`).
     Nothing compares it to v4.
   - The family the item names: `harness/oracle/cases/repository-zod-messages.ts`
     (184 lines; a pure tsx oracle, no fixture) ↔
     `crates/quilltap-harness/tests/repository_zod_messages_equivalence.rs`
     (155 lines). It currently drives three schemas: `group`,
     `groupDocMountLink` and `chatMetadataBase`. The row tuple is
     `[id, schema, row]`, built with `patch(BASE, p)`. `ABSENT` means
     undefined, and `{$float32:n}` means a `Float32Array`. The Rust side
     `match`es on `schema` (`:94-123`) and calls a v5 twin. Its floors are
     `rows >= 51` and `chat_messages == 6`.
3. **v4 at `07b8f0209`:** `lib/schemas/settings.types.ts:608-764`
   `ChatSettingsSchema`. The field is at `:702`:
   `impersonationVoiceMode: ImpersonationVoiceModeEnum.default('off')`, where
   the enum is `z.enum(['off','ask','always'])` (`:335`). The required
   non-defaulted keys are `id`/`userId` (`UUIDSchema`) and
   `createdAt`/`updatedAt` (`TimestampSchema`). `tagStyles` defaults to `{}`
   (`common.types.ts:92`), and every other key defaults or is optional. So a
   minimal valid row is `{id, userId, createdAt, updatedAt}`. The path is
   REACHABLE in v4 (the P4.D251 measurement: `findByUserId` answers `null` on
   a stored `'maybe'`). The NULL cell reads as `undefined`, so `.default('off')`
   applies.
4. **Divergence:** NONE known. This is a proof gap: the bytes are a literal
   nobody re-derives from v4.
5. **Predicted hunks: harness only.** Do NOT add a core helper. Drive v5's
   REAL `quilltap_core::db::chat_settings::find_by_user_id` (pub, `:1243`):
   - The Rust side gets a new arm `"chatSettings"`.
   - Build an in-memory `chat_settings` from the D23 dump in
     `crates/quilltap-core/src/services/provisioning/fresh_schema.json:10`,
     not from a hand DDL.
   - Insert the row and plant the mode with a raw UPDATE, as the unit test does.
   - Call `find_by_user_id` under `quilltap_core::test_support::captured_with`
     (already used by 53 harness files).
   - Take the `error=` tail of the `Data validation failed` line as v5's
     `message`. `None` plus zero lines means "ok".
   - Compare to v4's `r.error.message`, and assert the second
     (`Error finding entity by filter`) line carries the same bytes.

   The oracle gets one more schema key:
   `chatSettings: ChatSettingsSchema` in `SCHEMAS`, with a base
   `SETTINGS = {id, userId, createdAt: TS, updatedAt: TS}`. Rows:
   `settings-valid` (mode ABSENT → ok), `settings-mode-ask`,
   `settings-mode-always` (ok), `settings-mode-maybe`, `settings-mode-empty`
   (`''`), `settings-mode-case` (`'Off'`), `settings-mode-numeric-text`
   (`'1'` — a TEXT-affinity column stores a bound `1` as `'1'`).

   Optional, and unreachable by any writer: `settings-mode-float32`
   (`{$float32: 2}`). v5's check is `as_str()`-gated (`:1551`), so a BLOB cell
   would pass v5 and fail v4. Include it only as a both-ways pinned divergence,
   or leave it out and say so.

   The Rust floors move: `rows >= 51` becomes about 58, and a
   `settings_messages` count is added. Blast radius: two files.
6. **Proof:** tier-1 exact (message + issues) through the existing family.
   Red-first: today there is no row, so the proof is mutational. Change
   `ImpersonationVoiceMode::VALUES` order or `ZodIssue::invalid_value`'s
   message, and the new rows go RED. The unit-test literal reddens with them,
   which is the point: the literal is now oracle-backed. Closest recipe: the
   `chatMetadataBase` arm (`:107-123`).
7. **Fixtures:** none (pure tsx oracle). `fresh_schema.json` is read, not
   written.
8. **Risk:** low.
   - `find_by_user_id` reads many columns. Insert the same column set as the
     unit test (`:2253-2264`), or the hydrator may fail before the check
     (NOT MEASURED which columns it requires).
   - The capture rig is thread-scoped (memory note
     `a-process-global-test-seam-must-be-thread-scoped`).
   - Do NOT touch `db/chat_settings.rs`.

## A2. The voice-mode ensure corpus drives only INTEGER/NULL legacy cells

1. **Item** (`p4.d251-…md:5`): "the ensure corpus drives only INTEGER/NULL
   legacy cells (`cell_to_json`'s TEXT/REAL arms unexercised against v4)".
2. **v5 today:**
   - `crates/quilltap-core/src/db/chat_settings_impersonation_voice_mode_repair.rs:157-165`
     `cell_to_json` maps:
     - `Null | Blob(_)` → `Null`;
     - `Integer(i)` → number;
     - `Real(f)` → number;
     - `Text(s)` → string.

     It feeds `services::impersonation_voice_legacy::impersonation_voice_mode_from_legacy`
     (`:48-60`: `Bool(b)` → b; `Number` → `as_f64()==Some(1.0)`; else false).
   - The family is `crates/quilltap-harness/tests/chat_settings_voice_mode_ensure_equivalence.rs`
     (250 lines; modes a/b/c). Its oracle is
     `harness/oracle/cases/chat-settings-voice-mode-ensure.ts` (197 lines).
     The oracle BUILDS the per-mode base files into `QT_FIXTURE_OUT_DIR`
     (`voice-mode-<a|b|c>.db`; NOT committed).
   - Mode (a) seeds `s1..s4` = `1, 0, null, 1` (`:142-147`).
   - The comparands are `PRAGMA table_info`, `sqlite_master.sql` and
     `(id, impersonationVoiceMode)`. The Rust `expected("a")` pins
     `rows_backfilled: 4` (`:121-127`) and cross-checks v4's `translated N`.
   - The fixture migrator (`migrate-memories-fixture-columns.ts`) is NOT
     involved: this family builds its own files.
3. **v4:** `migrations/scripts/impersonation-voice-mode.ts:76-90` selects
   candidates and calls `impersonationVoiceModeFromLegacy(row.impersonationVoiceRewrite)`.
   That function (`lib/chat/impersonation-voice-legacy.ts:28-32`) returns
   `value === true || value === 1 ? 'ask' : 'off'`. better-sqlite3 hands the
   cell over as a number, string, Buffer or null.
4. **Divergence:** NONE predicted. `cell_to_json` looks right for all four
   storage classes. The gap is that a mutation of the TEXT, REAL or BLOB arm
   survives today.
5. **Predicted hunks:** NONE in the repair module (`cell_to_json` needs no
   change). Two files move:
   - The oracle case: mode (a) grows rows.
   - The Rust test: `expected("a").rows_backfilled` goes from 4 to 4+N.

   ⚠ **The trap is affinity.** The old column is `INTEGER DEFAULT 0`
   (`ADD_OLD`, case `:65`), so SQLite's INTEGER affinity coerces numeric text:
   - `'1'` and `'1.0'` are stored as INTEGER 1 (→ `'ask'`, the INTEGER arm);
   - a bound JS `1.0` is stored as INTEGER;
   - only non-numeric text stays TEXT (`'true'`, `'yes'`, `'ask'`);
   - only non-integral reals stay REAL (`1.5`, `0.5`).

   So the new rows must be chosen to land in each class:
   - `s5` = `'true'` (TEXT → `'off'`);
   - `s6` = `1.5` (REAL → `'off'`);
   - `s7` = `Buffer.from([1])` (BLOB → `'off'`);
   - `s8` = `'1'` (affinity → INTEGER 1 → `'ask'`, documents the coercion).

   Make the storage class a **comparand**, or the rows can silently fall into
   the INTEGER arm. The oracle emits
   `SELECT id, typeof("impersonationVoiceRewrite")` from the BASE file before
   migrating. The Rust side asserts the same `typeof` set on its copy before
   calling the ensure.
6. **Proof:** tier-2 structural, per mode. Red-first mutations, each RED only
   on the new rows and green on today's corpus (that is the
   "could-not-fail" proof):
   - (i) `Sql::Text(_) => Value::from(1)` reddens `s5`;
   - (ii) `Sql::Real(f) => Value::from(f.trunc())` reddens `s6`;
   - (iii) `Sql::Blob(_) => Value::from(1)` reddens `s7`.
7. **Fixtures:** none committed. The only reader of the case output is this
   one family.
8. **Risk:**
   - The `'1'` row's expectation depends on SQLite's numeric-text conversion
     rules. Measure it (the `typeof` comparand does). Whether `' 1'` (leading
     space) converts is NOT MEASURED; leave it out.
   - Mode (c) stays untouched.

## B1. The tier-2 standing corpus cannot see the posting-order leg among standing rows

1. **Item** (`p4.d249-…md:5`): "the tier-2 standing corpus seeds its
   standing rows in `createdAt` order, so `by_delivery_order`'s posting-order
   leg AMONG standing rows is observable only in the unit test".
2. **v5 today:**
   - The comparators: `crates/quilltap-core/src/db/chat_informs.rs:287-300`
     `by_posting_order` (createdAt ms, then `locale_compare(id)`; NaN →
     `Equal`) and `:310-321` `by_delivery_order` (`permanent` first, then
     posting).
   - The read: `find_pending_for_participant` (`:338-356`), whose SQL has no
     ORDER BY, so rows arrive in rowid (insertion) order before `sort_by`.
   - The corpus is `harness/oracle/fixtures/chat-informs-tier2.json`
     (`seed` 17 rows, `ops` 27, `standingChatId` `33333333-…`). Its standing
     rows C (`…c1`/`…c2` @ 00:02), D (`…d1` @ 00:03) and E (`…e1` @ 00:04)
     are seeded in array order, which is also createdAt order. So an
     implementation that skipped the sort among standing rows would pass ops
     18 and 20.
   - The fixture is built per regen by
     `harness/oracle/fixtures/build-chat-informs-fixture.ts`, which inserts in
     `spec.seed` order through v4's real `ChatInformsRepository.create`.
3. **v4:** `lib/database/repositories/chat-informs.repository.ts:94`
   `rows.filter(isInformInForce).sort(byDeliveryOrder)`; `:139` the batch
   read sorts `byPostingOrder`.
4. **Divergence:** NONE known; this is a proof gap.
5. **Predicted hunks:** the JSON spec only (plus the family's floors).
   - Append to `seed`, AFTER E, two standing rows for p1 in the standing
     chat:
     - **F** with `createdAt` `00:01:30` (earlier than C, inserted last), so
       insertion order ≠ posting order;
     - **G** with the SAME `createdAt` as D but an id that sorts BEFORE `…d1`
       (e.g. `5a000000-…-0000000000d0`), so the id tiebreak is exercised
       against insertion order.
   - Update `_commentStanding`.
   - The ops that read the standing chat move: 18, 20, 23–25, and the
     final-state dump. Re-check `chat_informs_tier2_equivalence.rs`'s
     row-count expectations (`:493` comment about "two more", `:522`).
6. **Proof:** tier-2 (`chat_informs_tier2_equivalence.rs` ↔
   `cases/chat-informs-tier2.ts`). Red-first mutation: make
   `by_delivery_order` return `Equal` when both rows are permanent, so
   standing rows keep insertion order. Op 18 goes RED on F/G; it is green on
   today's corpus.
7. **Fixtures:** `chat-informs-tier2.json` has FOUR readers:
   - `crates/quilltap-harness/tests/chat_informs_tier2_equivalence.rs`;
   - `chat_informs_routes_equivalence.rs` (reads only the `routes` block and
     the built fixture; its room is a different chat, `3f1c9f4a-…`);
   - `chat_informs_permanent_ensure_equivalence.rs` (reads only
     `testPepperBase64`, `:31-41`);
   - plus `cases/chat-informs-tier2.ts`, `cases/chat-informs-permanent-ensure.ts`
     and `fixtures/build-chat-informs-fixture.ts`.

   Re-run all three Rust families from the pin. The built `.db` is
   `/tmp`-only (nothing committed).
8. **Risk:** do not touch the P4.D205 chat (`11111111-…`). Its ops read
   unchanged only while the standing rows stay in their own chat.

## B2. The swipe-dedup case is tier-1 only

1. **Item** (`p4.d249-…md:5`): "the swipe-dedup case (a standing row whose
   first delivery is the swiped message) is tier-1 only".
2. **v5 today:**
   - `crates/quilltap-core/src/services/inform_block.rs:171-179`
     `merge_for_swipe` dedups standing ids out of `reapplied`.
   - Tier 1 is `inform_block_equivalence.rs` ↔ `cases/inform-block.ts` +
     `fixtures/inform-block.json`.
   - The tier-3 home is `crates/quilltap-harness/tests/regenerate_swipe_tier3_equivalence.rs`
     ↔ `harness/oracle/cases/regenerate-swipe-tier3.test.ts` +
     `harness/oracle/fixtures/regenerate-swipe-tier3.json` (`informs`, 7 rows),
     built per regen by `fixtures/build-regenerate-swipe-fixture.ts`
     (`:92-93,309-314`; `/tmp` .db, not committed).
   - Today's standing row `aa0c0007` is consumed by `aa0f0005` (OUTSIDE the
     group), so the dedup filter is never exercised. The family's request
     assertions are at `:396-423`, and its row-count floor
     (`rows.len() == 7`) is at `:693`.
3. **v4:** `lib/chat/context/inform-block.ts` `mergeForSwipe` (the standing
   rows first, then the re-applied rows minus the standing ids).
4. **Divergence:** NONE known.
5. **Predicted hunks:** the JSON spec plus the Rust test's floors.
   - Add `aa0c0008`: `permanent: true`, chat `aa000006-…` (the ungrouped
     target), seat `aa0a0006-…`, `consumedByMessageId: "aa060001-…-000000000601"`,
     which is the swiped message itself. The row is then in BOTH the
     consumed-by-messages set and the seat's in-force set.
   - Add a positive assertion (`:396-423` style): the standing text appears
     EXACTLY once in that call's request, ahead of
     `"The only line this target read."`.
   - Update `:693` from 7 to 8.
6. **Proof:** tier-3. The canned stream is keyed on the request, so a
   duplicated passage misses v4's canned key. Red-first mutation: drop the
   `seen` filter in `merge_for_swipe`, and the new call goes RED. It is green
   today because the filter is vacuous on the current rows.
7. **Fixtures:** `regenerate-swipe-tier3.json` has readers
   `regenerate_swipe_tier3_equivalence.rs`, `cases/regenerate-swipe-tier3.test.ts`
   and `fixtures/build-regenerate-swipe-fixture.ts` (no other). The recipe
   carries libtest flags (`--test-threads=1`); the sweep driver's splice fix
   from the 52d6e7ecd round applies.
8. **Risk:** the case's oracle also dumps `chat_informs` (`:434-436`), so the
   new row joins the "a swipe never writes" comparand. That is free coverage.

## B3. The ensure differential is one-mode

1. **Item** (`p4.d249-…md:5`): "the ensure differential is one-mode
   (baseline shape only; the absent-table / already-migrated / generateDDL
   arms are unit-pinned, not oracle-compared)".
2. **v5 today:**
   - `crates/quilltap-core/src/db/chat_informs_permanent_repair.rs:61-75`
     has two gates: the table exists, and the column is absent. The ALTER is
     `PERMANENT_DECL`. Unit tests are at `:144-230`.
   - The family is `chat_informs_permanent_ensure_equivalence.rs` (135
     lines) ↔ `cases/chat-informs-permanent-ensure.ts` (131 lines). It is one
     NDJSON line from one base file (`QT_FIXTURE_OUT`), with the generateDDL
     `permanent` line removed.
3. **v4:** `migrations/scripts/add-chat-informs-permanent.ts:31-41`
   `shouldRun`: SQLite backend; the table exists; `!columnExists('permanent')`.
   `:48` is `addColumnIfMissing(…, 'INTEGER NOT NULL DEFAULT 0')`.
4. **Divergence:** NONE known.
5. **Predicted hunks:** NONE in the repair module (it returns `()`, and the
   comparands are the DB state). Re-shape the case and the test into the
   **multi-mode template** that P4.D251 already built: one NDJSON line per
   mode, base files in `QT_FIXTURE_OUT_DIR`, as in
   `chat-settings-voice-mode-ensure.ts:126-190` /
   `chat_settings_voice_mode_ensure_equivalence.rs:147-250`. The modes:
   - (a) the baseline (today's);
   - (b) no `chat_informs` table, so v4 reports `not needed` and both
     `sqlite_master` lack it;
   - (c) already migrated: the (a) file after one v4 run, run again — v4
     `not needed`, and table_info/sql/rows unchanged;
   - (d) a generateDDL-current table (schema-order nullable `permanent`
     `INTEGER DEFAULT 0`) — `not needed`, left alone.

   The env vars change: `QT_FIXTURE_INFORM_ENSURE` becomes `…_DIR`. Update
   the header recipe and the sweep-driver header. The driver parses recipe
   headers (the `--self-test` guard), so expect to touch only the header.
6. **Proof:** tier-2 per mode. Red-first mutations:
   - drop the table gate → (b) errors on ALTER;
   - drop the column gate → (c)/(d) error with "duplicate column";
   - change `PERMANENT_DECL` → (a) differs.

   Today only the third reddens the family.
7. **Fixtures:** none committed. The case also reads `chat-informs-tier2.json`
   for the pepper; see B1 for its other readers.
8. **Risk:** v4's `runV4Migrations` reports a migration that did not run;
   pin that wording per mode, as the voice-mode family does (`:174-186`).

## B4. The in-force census is narrow and finds its test module by `split`

1. **Item** (`p4.d249-…md:5`): "the in-force census test is narrow
   (`.is_none()`/`.is_some()` needles only — a SQL `IS NULL` would pass it)
   and finds the test module by `split("#[cfg(test)]")`".
2. **v5 today:** `crates/quilltap-core/src/db/chat_informs.rs:728-750`
   `the_in_force_predicate_is_never_open_coded`. The file has one
   `#[cfg(test)]` (`:673`). The test:
   - scans only its own file;
   - uses two needles, `consumed_at.is_none()` (count 1) and
     `consumed_at.is_some()` (count 0);
   - is a substring count over raw text, so comments and literals count.

   The open-coded reads elsewhere in core (measured):
   - `services/inform_block.rs:224` `r.consumed_at.is_none()`, the
     "never delivered" `row_ids` filter (v4 `inform-block.ts:137`
     `!r.consumedAt`);
   - `api/chat_informs.rs:540` `r.consumed_at.is_some()`, the cancel's
     "ever delivered" (v4 `app/api/v1/chats/[id]/actions/inform.ts:219`).

   Both are CORRECT open checks: the review verified them, and they mirror
   v4. No production SQL names `consumedAt IS [NOT] NULL` (grep: 0).
3. **v4:** `lib/schemas/chat-inform.types.ts:83-84` `isInformInForce`, used
   at `chat-informs.repository.ts:94,139,277`.
4. **Divergence:** NONE. The census is just too narrow to catch a future
   one.
5. **Predicted hunks:**
   - Move the census to a NEW harness test,
     `crates/quilltap-harness/tests/chat_informs_in_force_census.rs`. Use
     `mod source_census;` (`tests/source_census/mod.rs`:
     `core_src_root`, `rust_sources`, `production_zone` (the lexer-based test
     stripper — the replacement for `split`), `code_only`, `string_literals`,
     `contains_word`).
   - It runs over ALL of core (optionally `quilltap-web`/`quilltap-host`
     `src` too). It checks:
     - **(i) code:** in `code_only(production_zone(src))`, count
       `consumed_at.is_none()`, `consumed_at.is_some()`, `consumed_at == None`,
       `consumed_at != None`, `consumed_at.is_none_or(`,
       `consumed_at.is_some_and(`. Allowlist with reasons: the home
       (`db/chat_informs.rs`, `is_none` 1) and the two sites above. The
       list IS the set of files.
     - **(ii) SQL:** in `string_literals(production_zone(src))`, any literal
       containing `chat_informs` (or `consumedAt`) together with `IS NULL` /
       `IS NOT NULL` (`contains_word`). Allowed: 0.
   - Delete the in-file test from `db/chat_informs.rs`'s test module. That
     is the module's TEST zone only, in the proposed ownership.
6. **Proof:** source census. Red-first mutations:
   - add a `WHERE consumedAt IS NULL` to `find_pending_for_participant`'s
     SQL → (ii) RED (the old test is green on this: the item's own point);
   - open-code `r.consumed_at.is_none()` in `find_pending_batches` → (i) RED;
   - move the home's needle into a comment → the old test miscounts; the new
     one does not.
7. **Fixtures:** none.
8. **Risk:** the memory notes `a-source-census-needs-a-lexer-and-must-keep-literals`
   and `a-new-harness-test-can-trip-a-source-census` apply. A new harness
   file is itself read by no census (they scan core `src`), so this is safe.

## B5. `primary_stream_tier3` / `system_import_state` carry no red-first count

1. **Item** (`p4.d249-…md:5`): "regenerated at the pin but the lane record
   carries no red-first count for them (the unified sweep ran both green from
   the pin)".
2. **This is a record gap only**; there is no code or test to change. The
   unified sweep at the `52d6e7ecd` unification (~165105) ran both `ok`, and
   every later sweep has too. Reconstructing a red-first count now would mean
   running both families at `e5c6bd0c0` against today's tree, which no longer
   matches the lane's tree, so it proves nothing.
3. **Proposal:** close it by a one-line recorded note in the P4.151 lane
   record ("record gap acknowledged; not reconstructable; both green at every
   sweep since"). No family work.

## C. The withdrawn `google-incomplete-tail` `.wire` row + `assert_catch_lines` widening

1. **Item** (`p4.141-…md:3`, OPEN): "the withdrawn `google-incomplete-tail`
   `.wire` row (wants `streaming_composer_equivalence::assert_catch_lines`
   widened first — bytes in the lane record)". The lane record is
   `status-log.md` `## P4.141` (160004). Unit 7 is at 160400-160424 and §S
   handoff (4) at 160482.
2. **v5 today:**
   - The decoder throw IS ported: `crates/quilltap-core/src/model/decoders/google_parts.rs:363`
     `DecodeError::new("Incomplete JSON segment at the end")`, with unit pins
     `:394`.
   - Google's catch line is ported: `model/plugin_catch_log.rs:196-208`
     (`Error streaming from Google Gemini API`,
     `context=GoogleProvider.streamMessage model=… error=…`), unit-pinned at
     `:567-576`.
   - `crates/quilltap-harness/tests/streaming_composer_equivalence.rs:382-408`
     `assert_catch_lines` filters captured lines by the SUBSTRING
     `"API error in streamMessage"`. It renders v4's lines as
     `ERROR quilltap::model::streaming_provider {message} context={ctx.context} baseUrl={ctx.baseUrl} error={e.error}`,
     using `.unwrap()` on `ctx.baseUrl`, so a Google row would PANIC rather
     than fail.
   - It is called from `run_decoder_fixture` (`:440ff`, the `captured_with`
     at about `:474`). **A second "could not fail" lives here:** Google's
     line does not contain the filter substring, so the composer family's
     silence leg on the six existing Google rows cannot see a spurious Google
     catch line. All six `google.recorded.ndjson` rows have an empty
     `pluginErrorLog` (measured).
   - The corpus: `harness/oracle/fixtures/streams/google_parts/{cases.json, google.recorded.ndjson, 6 .wire}`.
     It is recorded by `harness/oracle/providers/regenerate-stream-fixtures.sh:48`
     (`run google qtap-plugin-google google_parts`) through
     `record-stream-fixtures.mjs` from `$V4/plugins/dist/qtap-plugin-google`
     (the REAL genai).
   - The "two Google carve-out rows" in the brief are the
     text-http-errors corpus's three content-type approximations × 2 modes
     (lane record: "540 of 546 … the 6 others are the three pinned Google
     content-type approximations"). They live in
     `text_http_errors_equivalence.rs`, not in the stream corpus. The stream
     corpus has NO Google erroring row today.
   - The model renderer to copy is `text_http_errors_equivalence.rs:978-1004`
     `render_value` + `render_v4_line` (context keys in v4's order — harness
     `serde_json` has `preserve_order`, `Cargo.toml:28` — then the
     third-argument `error`).
3. **v4:** the recorded row (lane record, verbatim, recorded at
   `f6426e196`):
   `{"decoder":"google_parts","provider":"google","case":"google-incomplete-tail","error":"Incomplete JSON segment at the end","chunks":[{"content":"Hello","done":false}],"thrown":{"message":"Incomplete JSON segment at the end","name":"Error"},"refusal":{"refused":false},"trigger":"provider-error","pluginErrorLog":[{"plugin":"qtap-plugin-google","message":"Error streaming from Google Gemini API","context":{"context":"GoogleProvider.streamMessage","model":"gemini-2.5-flash","error":"Incomplete JSON segment at the end"},"error":null}]}`.
   ⚠ **The `.wire` bytes are NOT verbatim in the record:** the record gives
   `data: {…"Hello"…}\n\n` + an undelimited `{"candidates":…}`. The lane must
   rebuild them, modelled on `google-basic.wire`'s first event, and
   RE-RECORD at the current baseline. Do not paste the f6426e196 row.
4. **Divergence:** NONE expected (the decoder throw and the line are both
   ported). This is a proof gap.
5. **Predicted hunks:** harness + fixtures only.
   - In `streaming_composer_equivalence.rs`, rewrite `assert_catch_lines`:
     - select v5 lines by level + target
       (`starts_with("ERROR quilltap::model::streaming_provider ")`), not by
       substring;
     - render each `pluginErrorLog` entry generically (message, every
       `context` key in order via `render_value`, then ` error={e}` when
       `error` is a string);
     - no `unwrap` on `baseUrl`.

     The existing openai-SDK rows must stay byte-identical; they render
     `context=… baseUrl=… error=…` the same way.
   - A new `google-incomplete-tail.wire` + `cases.json` entry
     (`model: "gemini-2.5-flash"`), and a re-recorded `google.recorded.ndjson`
     (6 → 7 rows).
   - Check `stream_decoders_equivalence.rs` (`:430,440,704`) for any
     per-decoder row-count floor.
6. **Proof:** tier-1 wire (`stream_decoders_equivalence` +
   `streaming_composer_equivalence`, both `google_parts`). Red-first:
   - M9 from the lane: disable the tail check at `google_parts.rs:363`.
     Both families go RED on the new row.
   - Mutate `plugin_catch_log`'s Google stream message → the composer goes
     RED. Before the widening it could not.
   - Add a stray Google catch emit on a clean row → the widened silence leg
     goes RED.
7. **Fixtures:** `fixtures/streams/google_parts/*` is read by
   `stream_decoders_equivalence.rs` and `streaming_composer_equivalence.rs`.
   `provider_sdk_version_guard.rs:49` only cites the genai version in a
   comment. Regenerate ONLY the google line of the script, from the pinned
   v4 (`V4=<pin>`). Running the whole script would re-record every decoder.
   Check the other `*.recorded.ndjson` are `cmp`-identical if it is run
   whole.
8. **Risk:** the widened filter now sees EVERY streaming_provider ERROR line,
   so any existing row where v5 logs an unrelated ERROR would surface. That
   is the intended blindness fix, but the first run may find something. Treat
   a new red as a finding, not a test bug. Owned with C: the
   `streaming_composer_equivalence.rs` family (no longer "literals-only";
   that was §R.10(n) of P4.141).

## D. A PDF `document` case in the wizard / ai-import tier-3 corpora

1. **Item** (`p4.d253-…md:5` + Tier 2 item 7 at `:228-231`): "Tier 2 item 7
   NOT LANDED (measured unstageable: the tier-3 oracles read only the
   committed `character-generators-*` pair and none carries a PDF; a PDF
   `document` case needs that pair rebuilt)".
2. **v5 today:**
   - **The committed pair:**
     `crates/quilltap-web/tests/fixtures/character-generators-main.db`
     (237,568 B, mtime Sep 29 — widened by the vintage migrator) and
     `…-mount.db` (192,512 B, Sep 7).
   - **The builder:** `harness/oracle/fixtures/build-character-generators-fixture.ts`
     from the spec `harness/oracle/fixtures/character-generators.json`.
     `files` has 4 entries with pinned ids `f0000002-…-0000000000{11,12,13,14}`.
     `FileSpec` supports `textBody` / `png` / `hexBody` (`:78-79,140-141`), so
     a PDF can ride `hexBody` with `mimeType: "application/pdf"` and no
     builder code change. A `pdfBase64` key would be cleaner but is optional.
   - ⚠ The builder header says the vault-side mount-point / link / blob ids
     are MINTED and baked, so a rebuild REMINTS them.
   - **EVERY reader of the pair** (grep of the filename):
     - harness: `character_wizard_tier3_equivalence.rs`,
       `ai_import_tier3_equivalence.rs`, `character_optimizer_tier3_equivalence.rs`;
     - web: `crates/quilltap-web/tests/common/mod.rs:431-460`
       `materialize_generators_instance`, used by
       `generators_wizard_routes.rs` and `characters_generators_routes.rs`;
     - oracle: `cases/character-wizard-tier3.test.ts`,
       `cases/ai-import-tier3.test.ts`, `cases/character-optimizer-tier3.test.ts`
       (+ `fixtures/character-optimizer-tier3.json`);
     - tooling: `fixtures/migrate-memories-fixture-columns.ts:227` (a recipe
       line) and `build-headshoulders-fixture.ts:9` (a comment only).
     - No Playwright reader (grep of `apps/web`: 0).
   - **Case specs:** `fixtures/character-wizard-tier3.json:159,176,193`
     (`documentId` 11 / 14 / 12) and `fixtures/ai-import-tier3.json:25-26,44`
     (`sourceFileIds`).
   - **v5's PDF arm:** `crates/quilltap-core/src/generators/file_content.rs:334`
     `extract_pdf_content`. It reads the seam via `pdf_text_extractor()`
     (`:291-295`): a THREAD-LOCAL `SCRIPTED_TEXT_EXTRACTOR`
     (`ScriptedTextExtractorGuard`, `:280-300`), else
     `default_text_extractor()` = `RefusingTextExtractor`
     (`services/mount_index/converters.rs:34-44`, which answers `''`). On
     `''` it uses `extract_pdf_text_fallback` (`:225`).
   - The jest oracle UN-mocks the storage manager
     (`character-wizard-tier3.test.ts:141-146`), so real blob bytes DO reach
     `extractFileContent`.
3. **v4:** `lib/services/file-content-extractor.ts:165-210`. It goes through
   `convertPdfBufferToText` (`lib/mount-index/converters/pdf-converter.ts`,
   real `pdf-parse`). On empty it logs WARN
   `pdf-parse found no text, using native fallback extraction` and uses the
   fallback. If the fallback is also empty, the failure is
   `Failed to extract PDF content (no text found)`. Otherwise DEBUG
   `Extracted PDF content`. `jest.config` does not map `pdf-parse`, so
   whether the REAL pdf-parse (pdfjs-dist, ESM) loads inside the jest oracle
   is NOT MEASURED.
4. **Divergence:** the brief says "v5 refuses, so the case pins the known
   divergence". That is only half right. v5 refuses at the converter, then
   runs the SAME native fallback v4 runs on an empty converter answer. So:
   - **(a) parity design (recommended):** the oracle `jest.doMock`s
     `@/lib/mount-index/converters/pdf-converter` `convertPdfBufferToText` per
     case, and the Rust side arms `ScriptedTextExtractorGuard` with the same
     script. This is the P4.D253 tier-1 idiom. Two rows:
     - "converter answers text": the parsed text reaches the prompt;
     - "converter answers `''`": the fallback runs on both sides, with the
       WARN line.
     Both are parity.
   - **(b) one optional "real converter" row:** v4's real pdf-parse vs v5's
     refusing default. This is a DIVERGENCE row (parsed text vs fallback text
     or failure), pinned both ways. Include it only if pdf-parse loads under
     jest; measure first.
5. **Predicted hunks:** fixtures + oracle cases + harness only; no core.
   - `character-generators.json` gains `f0000002-…-000000000015`
     `uploads/lore.pdf`. Use a small real PDF with an uncompressed `BT … Tj`
     text stream, so `extract_pdf_text_fallback` can read it for the `''`
     row.
   - Rebuild the pair.
   - New cases in `character-wizard-tier3.json` and `ai-import-tier3.json`.
   - The two oracle cases gain a per-case `pdfConverter` script.
   - The two Rust families arm the guard per case.
   - ⚠ Thread-local seam: the families must call the generator on the
     arming thread. A multi-thread tokio runtime that hops threads would
     silently use the refusing default. NOT MEASURED which runtime each
     family uses; check before relying on it.
6. **Proof:** tier-3. Red-first mutations:
   - ignore the scripted extractor (always the refusing default) → the
     "converter answers text" row goes RED;
   - fallback-first → RED.
7. **Fixtures:** the pair has NINE readers (list above). After the rebuild,
   re-run all three harness families + both web route tests + the optimizer
   oracle by name. The vault ids REMINT, so check that no reader hard-codes
   a minted id. NOT MEASURED; grep the three `.json` specs and the two web
   tests for any `b0000002`-less mount UUIDs.
   - **The alternative to a rebuild:** an additive in-place insert (one
     `files` row + one mount blob, like the vintage migrator's in-place
     widen). This keeps the minted ids but must reproduce v4's exact
     blob/`files` bytes. Recommend the rebuild: the builder is v4's own
     repositories at the pin.
8. **Risk / ruling:** one lane must own the pair (no sibling may regenerate
   it). Confirm the vintage: rebuilding at `07b8f0209` gives current-vintage
   columns, which is fine. The pair is in `crates/quilltap-web/tests/fixtures/`,
   a web-crate path; the lane owns only those two `.db` files. **Ruling
   wanted:** is design (a)-only acceptable, with the real-pdf-parse row
   dropped if jest cannot load it?

## E. `host_zone_sites_census` cannot see a zone re-derived from a NAME (harness side)

1. **Item** (`p4.140-…md:3`, OPEN): "the census hunts ambient reads and
   `TimeZone::UTC` but not a display surface re-deriving the zone via
   `TimeZone::get(server_tz)`". (`ProductionSpineFactory::new`'s UTC default
   is P4.150's.)
2. **v5 today:** `crates/quilltap-harness/tests/host_zone_sites_census.rs`
   (811 lines) has these tables:
   - `CENSUS` (`system_display_zone(` callers, `:64`);
   - `HOST_SITES` (14 rows, `:75-160`);
   - `AMBIENT_READ_ALLOWED` (`:165`, the CLI `docs_cmd.rs`
     `TimeZone::system(`);
   - `AMBIENT_READ_NEEDLES` (`:176`: `system_display_zone(`,
     `TimeZone::system(`, `try_system(`, `Zoned::now(`);
   - `VALUE_SITES` (9 rows, `:190`);
   - `NO_AMBIENT_READ_DIRS` (web/tauri/cli, `:253`);
   - `UTC_ALLOWED` (6 files, `:260`);
   - `UTC_ARITHMETIC_ALLOWED` (`:299`).

   The host loop pins `spine.rs` at exactly 2 production `TimeZone::UTC`
   (`:515-524`; one is `ProductionSpineFactory::new`'s default at
   `spine.rs:3699`). There is NO `TimeZone::get(` needle.

   **Measured production `TimeZone::get(` sites** (before each file's first
   `#[cfg(test)]`):
   - core `enclave/cron.rs:97` (cron NAME);
   - core `enclave/step.rs:261` (`last_local_midnight_iso`);
   - core `chat_timestamp.rs:226` (`zone_offset_seconds`, the story zone);
   - core `day_references.rs:313` (`resolve_day_reference`);
   - core `db/llm_logs.rs:1297` (retention cutoff);
   - core `progressions/engine.rs:749` (`format_instant_en_us` — **a display
     surface from a NAME**, the P4.140 Tier 3 item 17 residue);
   - core `services/memory_recap/distill.rs:150` (`local_date_stamp`);
   - host `spine.rs:2425` (`js_local_offset_minutes`).

   That is 7 core files and 1 host file, one each. All the `America/Chicago`
   hits are in test modules. `quilltap-web/src/main.rs:405/411` are inside
   the `:277` test module.
3. **v4:** n/a (a v5 wiring census).
4. **Divergence:** none today. All eight are NAME-calendar or recorded
   residue sites, and a new one would go unseen.
5. **Predicted hunks:** this one file.
   - A `ZONE_BY_NAME_ALLOWED: &[(&str /*path from repo root*/, usize, &str /*why*/)]`
     with the eight rows. Each reason names its NAME source (cron / story
     zone / distill calendar / retention / progressions residue / JS offset).
   - A loop over core `all_core_files()` plus the host crate's sources
     (`rust_sources(repo.join("crates/quilltap-host/src"))`) and the
     `NO_AMBIENT_READ_DIRS`. It counts `TimeZone::get(` in
     `code_only(production_zone(src))` (catches `jiff::tz::TimeZone::get(`
     and `tz::TimeZone::get(` alike), and asserts per file. The list IS the
     set.
   - Optionally also hunt `TimeZone::posix(` / `TimeZone::fixed(` (production
     count 0 today; `host_zone.rs:109` is test).
6. **Proof:** source census. Red-first: add a production
   `TimeZone::get(&cfg.tz)` in any display formatter (P4.140's M4 shape) →
   RED. Today it is green.
7. **Fixtures:** none.
8. **Risk / cross-lane:** ⚠ **P4.150 owns `ProductionSpineFactory::new`.**
   If it removes the UTC default, `host_injection_points_carry_the_host_zone`'s
   `spine.rs` `TimeZone::UTC` count (2 → 1) and the `quilltap-web/src/lib.rs`
   `HOST_SITES` row's reason move. Either P4.150 owns those hunks in this
   file, or this lane takes them as a rider stacked after P4.150 (memory note
   `stack-a-rider-order-on-an-unrun-siblings-branch`). Ruling needed on who
   owns the file. Do not let both lanes edit it in parallel.

## F. Other open "test that could not fail" items in the two rounds' records

Searched `status-log.md` 164644–166016 (the `52d6e7ecd` and `07b8f0209` lane +
unification records) for "could not / cannot / vacuous / recorded, not fixed".
Everything the reviews found was fixed at unification (07b8f0209 review items
1–7; 52d6e7ecd items 1–7). The STILL-OPEN test-craft residue:

| # | Item | Source | Harness-only? | Proposal |
|---|---|---|---|---|
| F1 | The composer's Google silence leg is blind (substring filter) | found by this survey (§C.2) | yes | folded into C |
| F2 | text-http-errors 2xx rows never assert `transport_kind == None` | `p4.141-…md:3` OPEN; `text_http_errors_equivalence.rs:1166` `Posed::Ok2xx(_) => {}` | yes | assert `e.transport_kind.is_none()` on every Ok2xx row that errs (the 7-provider shape guard). Red-first: build the shape-guard error with `TransportError::http`-kind → RED |
| F3 | `PosedTransport::execute` ignores `policy` (send-side timeout budget unproven) | `p4.141-…md:3`; `text_http_errors_equivalence.rs:565-569` `_policy` | yes | record `policy.timeout` on `saw`; assert it equals the row's `requestTimeoutMs` on send-mode hang rows (the stream arm already does, `:590-592`). Red-first: pass a default policy on the send path → RED |
| F4 | `GroupsRepository::create` has no differential; the projects routes `check` compares bodies only | 52d6e7ecd record ~165065 (P4.146) | no — `db/groups.rs`/`db/projects.rs` territory | OUT of this lane (named for the planner) |
| F5 | the restore accepts malformed `permanent` with no pin | 52d6e7ecd record ~165058 | no — `services/backup/**` | OUT (backup/restore lane, P4.147 survey) |
| F6 | v4's `Error deleting entity` has no v5 analog | same | no — `db/fallback.rs` | OUT |
| F7 | the P4.D250 greeting wait could resolve early (fails loudly) | same, ~165071 | no — `apps/web/e2e` | OUT |
| F8 | `RefusingTextExtractor`'s stderr wording false at the PDF caller | `p4.d253-…md:5` | no — `services/mount_index/converters.rs` | OUT (smalls, not test craft); note D's case would print it |

Phantoms: none found. Every in-scope item above was verified open on `main`
`6c3a3c635`.

---

## §Ownership proposal

**Edits (harness / oracle / fixtures):**
- `crates/quilltap-harness/tests/repository_zod_messages_equivalence.rs`, `harness/oracle/cases/repository-zod-messages.ts` (A1)
- `crates/quilltap-harness/tests/chat_settings_voice_mode_ensure_equivalence.rs`, `harness/oracle/cases/chat-settings-voice-mode-ensure.ts` (A2)
- `harness/oracle/fixtures/chat-informs-tier2.json`, `crates/quilltap-harness/tests/chat_informs_tier2_equivalence.rs` (B1)
- `harness/oracle/fixtures/regenerate-swipe-tier3.json`, `crates/quilltap-harness/tests/regenerate_swipe_tier3_equivalence.rs` (B2)
- `crates/quilltap-harness/tests/chat_informs_permanent_ensure_equivalence.rs`, `harness/oracle/cases/chat-informs-permanent-ensure.ts` (B3)
- NEW `crates/quilltap-harness/tests/chat_informs_in_force_census.rs` (B4)
- `crates/quilltap-harness/tests/streaming_composer_equivalence.rs`, `harness/oracle/fixtures/streams/google_parts/{cases.json, google.recorded.ndjson, google-incomplete-tail.wire}` (C)
- `harness/oracle/fixtures/character-generators.json`, `crates/quilltap-web/tests/fixtures/character-generators-{main,mount}.db` (the two files only), `harness/oracle/fixtures/{character-wizard-tier3,ai-import-tier3}.json`, `harness/oracle/cases/{character-wizard-tier3,ai-import-tier3}.test.ts`, `crates/quilltap-harness/tests/{character_wizard_tier3,ai_import_tier3}_equivalence.rs` (D)
- `crates/quilltap-harness/tests/host_zone_sites_census.rs` (E — coordinate with P4.150)
- `crates/quilltap-harness/tests/text_http_errors_equivalence.rs` (F2/F3)

**Core:** `crates/quilltap-core/src/db/chat_informs.rs`, its `#[cfg(test)]`
module only (delete the moved census). **No other core hunk:**
- `db/chat_settings.rs`: NONE (A1 drives the real function);
- `db/chat_settings_impersonation_voice_mode_repair.rs`: NONE (`cell_to_json`
  is right; only rows are added);
- `db/chat_informs_permanent_repair.rs`: NONE.

**Re-run only (readers of a moved fixture):**
- `chat_informs_routes_equivalence.rs`
- `character_optimizer_tier3_equivalence.rs` + `cases/character-optimizer-tier3.test.ts`
- `crates/quilltap-web/tests/{generators_wizard_routes,characters_generators_routes}.rs`
- `stream_decoders_equivalence.rs`

**Must NOT touch:**
- `db/fallback.rs`, `db/projects.rs`, `db/groups.rs`
- `services/backup/**`, `services/quilltap_import/**`
- `services/mount_index/**` (F8)
- `generators/file_content.rs` (D uses its existing guard)
- `model/**`
- `crates/quilltap-host/**` (incl. `spine.rs` — P4.150)
- `apps/web/**`
- `harness/oracle/providers/*` (C uses the script, does not edit it)

## §Open questions

1. **E:** P4.150 vs P4.151 ownership of `host_zone_sites_census.rs` — one
   owner, or a stacked rider after P4.150?
2. **D:** accept the scripted-converter parity design (a) as the whole proof,
   dropping the real-pdf-parse divergence row if `pdf-parse` will not load
   under jest (NOT MEASURED)?
3. **D:** rebuild the `character-generators-*` pair (remints vault ids; nine
   readers re-run) vs an in-place additive insert? This survey recommends
   the rebuild.
4. **A1:** include the unreachable BLOB-in-mode-column row as a both-ways
   pinned divergence (v5 passes it, v4 drops the row), or omit it as
   writer-unreachable?
5. **B5:** is a recorded acknowledgement enough to close it? This survey says
   yes; it cannot be reconstructed.
