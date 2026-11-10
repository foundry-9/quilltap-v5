# Survey C2: wardrobe / inventory / item routes / pools / carriers / providers / server smalls

Pin: v4 `624d0d25c`, read through `git show` / `git grep` only. v5 main @ `8ad1a0640`. Read-only: nothing was built and no test was run. "Measured" below means grep / read of main and of the pin.

Orders read (status header + lane record): P4.D350, P4.D356, P4.D364, P4.D367, P4.D377, P4.D378, P4.167, P4.170.

Tally: **OPEN 21 · CLOSED 6 · NO-ACTION 7 · NEEDS-RULING 2 · DEFER-TO-CATCH-UP 6.** The per-order index is at the end.

---

## A. The named items

### A1 — `provider_sdk_version_guard` reads the LIVE v4 install   [OPEN — S, harness only]

- **What it reads (measured).** `crates/quilltap-harness/tests/provider_sdk_version_guard.rs:252` takes `common::v4_root()` (`tests/common/mod.rs:439-463`): `QT_V4_CHECKOUT`, then `QT_V4_ROOT`, then `$HOME/source/quilltap-server`. Test 1 then reads `<root>/node_modules/{openai,@openrouter/sdk}/package.json` and every `<root>/plugins/dist/*/node_modules/<sdk>/package.json` + `@quilltap/plugin-utils/package.json` (`:234-248`, `:283-355`). Tests 2–4 read committed corpora only and never touch v4.
- **So the guard ALREADY honours `QT_V4_ROOT`.** In the unifier's `cargo test --workspace` (env block: `QT_V4_CHECKOUT` = `QT_V4_ROOT` = the pin) it is not among the 21 reds. It is red only where the env var is absent: **the sweep driver** (and any lane gate without the var).
- **Why the sweep misses it.** `harness/tools/recipe_sweep.py` never exports the pin: `--v4` is applied only as a TEXT rewrite of `~/source/quilltap-server` inside the header recipe (`normalize`, `:942-945`), and the run env is `dict(os.environ, CARGO_INCREMENTAL="0")` (`:1293`). This guard's header recipe (`:145-146`) is the bare `cargo test -p quilltap-harness --test provider_sdk_version_guard` — no v4 path in it, so nothing is rewritten and `v4_root()` falls to the live checkout.
- **Live install today (measured):** root `openai` 7.54.0, `@openrouter/sdk` 1.5.73; plugin dirs `openai` 7.54.0, `@anthropic-ai/sdk` 1.7.0, `@openrouter/sdk` 1.5.73, plugin-utils 2.7.4 (v4 `ac6b60c21`). The constants are the pin's floors: 7.47.0 / 1.4.0 / 2.32.0 / 1.5.34 / 2.7.3 (`:153-195`). The guard is doing its job against the wrong tree.
- **Smallest change (recommended): one header line.** Make the "Run standalone" recipe
  ```
  //!   QT_V4_CHECKOUT=~/source/quilltap-server \
  //!     cargo test -p quilltap-harness --test provider_sdk_version_guard -- --nocapture
  ```
  This is the idiom `public_schemas_vendor_guard.rs:20-22` and `qtap_schema_embed_guard.rs:14-16` already use — and those two were green in the same unification sweep in which this guard was red, which is the proof the driver's rewrite reaches the env word. Also correct the header's `:141-143` "No recipe stage" sentence. No Rust code moves.
- **Same class, same one-line cure (measure each first):** `zod_version_guard.rs:39` and `builtin_prompt_templates_guard.rs:41` use `common::v4_root()` and have a bare `cargo test` run line. They are green today only because the live checkout happens to agree (`zod` is `^4.6.7` at both; `prompts/` is unchanged `624d0d25c..HEAD`).
- **Alternative (closes the class, slightly larger):** in `run_family`, when `V4_CHECKOUT != V4_CHECKOUT_DEFAULT`, add `QT_V4_CHECKOUT` (and `QT_V4_ROOT`) to `env` unless the recipe sets it, plus a `--self-test` row. Recommend the header line now and the driver export as a rider in the same lane, since seven test files use `common::v4_root`.
- **Trap.** With the pin as root, a pin whose plugin dirs were not floor-installed (`.claude/pin-plugin-sdks.sh`) prints the guard's `SKIP:` and the driver reports `skipped` — that is the correct signal, not a regression. The catch-up round re-records at a new pin and moves the constants (`ac6b60c21`); this change is independent of that and survives it.
- **Why not "read the pin's `package.json` floors by git".** The recorders resolve INSTALLED copies; the declared caret floor is not what stamped the corpus. Keep reading installs.
- **Files:** `crates/quilltap-harness/tests/provider_sdk_version_guard.rs` (header), optionally `zod_version_guard.rs`, `builtin_prompt_templates_guard.rs`, `harness/tools/recipe_sweep.py`.

### A2 — P4.D367 item 16: the cap-2-understudy row in `primary_stream_tier3`   [OPEN — M/L; recommend OUT]

- v5 harness: `crates/quilltap-harness/tests/primary_stream_tier3_equivalence.rs:1259` and `:1544` pass `adapter: None`; `:1380-1387` passes `None` for the reroute's describer with a comment that the arrays carry no attachments. The one attachment row (`hard_error_vision_skips_understudy`) skips its understudy.
- v4: `lib/services/chat-message/provider-failover.service.ts:510`, `:854` (the pass for each candidate).
- Done = one new `hardFailover` case with an image-bearing primary array and a cap-2 vision understudy; real `FallbackDeps` (db + canned completion + transcoder) threaded into the seam; both sides' restream arrays byte-equal.
- Already pinned without it: `provider_failover`'s unit test `each_understudy_gets_the_transport_budget_for_its_own_profile`, the describer arm's unit test (`9aaed766a`), and `file_attachment_tier3`'s (E) family.
- **Why OUT:** v4 `4b97636b2` (past the pin) deletes `previousResponseId` from `primary-stream.service.ts` / `provider-failover.service.ts`, so this family's corpus and seam wiring are re-cut at the catch-up; threading describer deps now is rework there. Carry it INTO the catch-up's failover order.

### A3 — P4.D367's oracle uses the static capability mirror, not the registry   [NO-ACTION]

- `harness/oracle/cases/image-transport-budget.test.ts:16-18, 137-143`: v4's plugin registry is not initialised in jest, so `providerCanTransportImages` (`lib/llm/image-transport.ts:42-53`) answers from `staticProviderCanTransportImages`. v5's `image_transport_policy_for` (`services/image_transport_budget.rs:376-395`) always answers the baked-manifest (registry) tier.
- The predicate's registry tier has its own tier-1 differential: `image_transport_equivalence.rs` `full_init` rows over all ten real dist plugins (`harness/oracle/cases/image-transport.ts`). The budget family's two capability rows use OLLAMA / OPENAI, where both tiers agree, and the case asserts non-vacuity.
- Nothing can diverge that the leaf family does not already see. If wanted later: initialise the registry in the jest case (the `image-transport.ts` idiom). Not worth a lane.

### A4 — P4.D377 items 6 / 10 / 14 / 18

- **Item 18 — CLOSED by measurement.** The question was whether `[WardrobeLocation] Location write` reaches `wardrobe_tools` / `pascal_wardrobe_effects` / `wardrobe_wear_commit_tier2` captures. The unification's full sweep from the pin was 823 ok with only `ariel_writers_tier3`, `search_replace`, `provider_sdk_version_guard` red (`status-log.md` § the gate). All three families ran green there.
- **Item 6 — `duplicate_create_schema_refusal`   [OPEN — S/M, harness only].**
  - v5: `api/wardrobe_route_factory.rs` `duplicate` (the `parse_item_fields(&payload, false)` → `validation_error()` arm). v4: `lib/wardrobe/routes/wardrobe-route-factory.ts` `handleDuplicate`.
  - `harness/oracle/cases/wardrobe-folders-routes.test.ts:84, 162-168` runs `plants` on the MAIN copy only. The row needs an item DOCUMENT whose stored title passes the read but fails today's create schema after `nextCopyTitle` (e.g. a title at the max length, so ` (copy)` overflows). Add a mount-index plant leg (the `mountSql` idiom of `wardrobe-images-routes.test.ts`) and one row; 19 `p4d377_duplicate_*` rows already exist (`wardrobe-folders-routes.json:3598-3823`).
  - Trap: the doc store is content-addressed with a BEFORE INSERT trigger (memory `recall-lane-oracle-gotchas`); plant through the same write on both sides or via the store's own write in-case.
- **Item 10 — the two-group same-id row   [OPEN — M, harness only].**
  - v5: `api/wardrobe_route_factory.rs` `list_in`'s `?scope=group` arm over `reach::group_origin_map`. v4: `wardrobe-route-factory.ts:229-240` `listGroupTier` (`Array.from(originByMount.keys()).reverse()`).
  - Only `p4d377_list_group_tier` (one group) exists (`wardrobe-folders-routes.json:3834`). Group membership lives in the group's store (`properties.json`), not in MAIN, so the row needs either an in-case second group created through v4's real `repos.groups.create` + a same-id item written to both group stores, or the `group-wardrobe.test.ts` fixture grown. Prefer the in-case creation; a rebuilt keystone fixture re-mints ids for every family that shares it.
  - Row name from the order: `group_tier_same_id_earlier_group_wins`. Mutation proof: drop the `.rev()`.
- **Item 14 — the tier-2 doc-store rows   [OPEN — S, harness only; meeting point].**
  - The tier-1 half is landed: `mount_chunker_equivalence.rs:151-191` (`normaliseOpts` rows × `allowRoot`). The local store normaliser is `db/database_store.rs:174-179` → `StoreError::FileOp` (`:119`, `:140`, `:156`).
  - Missing: a tier-2 row driving `createDatabaseFolder` / `writeDatabaseDocument` with `.` (now the root) and `../x` (`Path traversal not allowed: ../x`). No harness test mentions that text (grep: 0 in `crates/quilltap-harness/tests`, 0 in `harness/oracle`).
  - Family: `doc_mount_folders_tier2_equivalence.rs` — shared with the documents / mount-store area (P4.D380 / P4.D381). Hand it to whichever lane owns that family this round.

### A5 — the `.qtap` kind import reports one issue where v4's ZodError lists all   [DEFER-TO-CATCH-UP]

- v5: `services/quilltap_import/characters.rs:328-341` checks `kind` alone and returns a one-issue ZodError; every other refusal falls to `serde_error_text` at `:347-352`. v4: `repos.characters.create` → `CharacterSchema.parse` (all issues).
- v4 `1dc11e947` (past the pin) splits `CharacterSchema` into `CharacterRowSchema` / `CharacterManagedFieldsSchema` and rewrites `characters.repository.ts` (+1,113 / − lines) and `lib/schemas/character.types.ts` (210 lines). A whole-character Zod twin written now is written against a schema that moves. Rides with `RAW_CHARACTER_ROW_UNVALIDATED` in the catch-up's character order.

### A6 — K6: the stale `_meta.baseline` in `uuid-remap-corpus.json`   [OPEN — S]

- `harness/oracle/cases/backup-uuid-remap.test.ts:1919` writes the LITERAL `baseline: 'f5e953a3f'`; the committed corpus (`harness/oracle/fixtures/uuid-remap-corpus.json:3`) carries it although the corpus was regenerated as late as `932d1ff12` (2026-11-08). The Rust side reads `_meta.collections` only (`backup_uuid_remap_equivalence.rs:355`), so nothing fails.
- Done = derive it (`git rev-parse --short HEAD`, the `workspace-core.ts` D330-meta precedent, `status-log.md:193749`) and regenerate corpus + NDJSON together from the pin (the corpus is hash-pinned to the NDJSON; the family is `refused` by the sweep driver by design, so it is a by-hand regen). Expected diff: the one `baseline` line + the stamped sha.
- v4 `89f259c2f` / `ebb0ca13a` / `1dc11e947` touch `uuid-remap.ts` past the pin, so the corpus is regenerated again at the catch-up; fixing the derivation now makes that regen self-correcting. Cheap rider on the carriers lane.

### A7 — K7: restore 22n-quater reads cells leniently   [OPEN — S/M]

- v5: `services/backup/restore/orchestrator.rs:2185-2260`. `os(entry, "id")` / `os(entry,"createdAt")` / `os(entry,"updatedAt")` turn a NON-STRING cell into `None`, so `LogEntriesRepository::create` (`db/log_entries.rs:171-200`) MINTS a fresh id / stamp and succeeds. `s(entry,"message")` / `s(entry,"chatId")` turn a non-string into `""`, which is refused, but with the empty-string issue rather than v4's `invalid_type`. Unknown extra keys are dropped silently.
- v4: `lib/backup/restore/restore.ts:1241-1249` — `const { id, createdAt, updatedAt, ...entryData } = entry; create(entryData, { id, createdAt, updatedAt })`: the raw values reach `LogEntrySchema`, so `id: 5` is a refusal → ONE warning `Failed to restore Log entry: <ZodError>` + WARN `{entryId, error}`.
- Done = build the entity from the RAW cells and run `zod_log_entry_issues` on it whole (the P4.161 whole-row class), so every malformed cell is v4's warning text; `entryId` in the WARN is the raw value's rendering.
- Differential: `system_restore_state.rs` (`restore_log_entries_preserve` / `_new_account` exist, `:428-438`, `:1493`) + the restore oracle case; add planted rows: numeric `id`, `null` `createdAt`, numeric `message`, an extra key. Restore census is a whitelist (memory `restore-census-whitelist`): no new line text here, but case adds move the seen / count constants.
- Also in the same block: the comment at `:2196-2199` says `llm` is `None` for degraded AND absent; `:206-207` now has `llm_logs_degraded` from `partition_state`. v4 checks `isLLMLogsDegraded()` only when rows exist. Align the gate while there (measure the absent-file arm; v4 has no absent file).
- Past the pin `restore.ts` gains +43 lines (avatar timeline, `1dc11e947`), additive — not a rewrite of this step.

### A8 — P4.D356 H9: the image-dialects recorder and NanoGPT's cold catalog GET   [CLOSED]

- `harness/oracle/providers/record-image-fixtures.mjs:824-828` answers the bug-269 catalog GET (P4.D364, `63d575c06`); the corpus was re-recorded at the pin (191 rows). The D364 lane record names it ("two stale recorder premises were also fixed").

### A9 — P4.170's eleven OPEN Tier-2 items + three Tier 3 (measured: none landed since)

Markers grepped on main: `Error toggling avatar generation` 0 hits in `crates/`; `buildToolsForProvider called` 0; `NAN_ENTRY_INDEX_READS_ZERO` still carved; `save_image_failed` unchanged; no two-chunk row; `ImageParamsLogContext` has no `character_id` / `item_id`. v4 files for all eleven are unchanged or additive `624d0d25c..HEAD` (`git diff --stat`), so none is a DEFER.

| Id | State | v5 site | v4 site @pin | Done looks like | Differential | Size |
|---|---|---|---|---|---|---|
| **B.3** | OPEN | `services/chat_avatars.rs:284-330` (`toggle_avatar_generation`; `From<DbError>` at `:67-71` leaks the DB text) | `app/api/v1/chats/[id]/actions/toggle-avatar-generation.ts:97-99` | a WRITE failure → ERROR `[Chats v1] Error toggling avatar generation {chatId, context:'avatar-generation'}` + 500 `Failed to toggle avatar generation`. The READ is v4's fallback (`Chat not found`, no outer ERROR — measured by P4.170) | `chat_cast_routes_equivalence` + an abort-trigger plant | S |
| **B.4** | OPEN | `api/wardrobe.rs:808, 915, 1038, 1062` (the four `[Avatar Preview]` lines exist) | `app/api/v1/wardrobe/preview-avatar/route.ts:104-225` | the web family opens `llm_logs: None` (`quilltap-web/tests/wardrobe_routes_equivalence.rs:270`); give `pv_*` rows a logs partition, capture the four lines, compare the `IMAGE_GENERATION` rows | that family + `wardrobe-routes*.test.ts` | S–M |
| **B.6** | OPEN | `services/wardrobe_item_image_generation.rs:245-251` | `lib/wardrobe/item-image-generation.ts:131-136` | a cyclic-composite row pinning `Outfit expansion stopped short` (`cycles`, `truncated`) | `wardrobe_item_image_generation_tier3_equivalence` | S |
| **B.7** | OPEN | `image_gen/params_builder.rs:119-128`; literals at `api/wardrobe.rs:853`, `services/wardrobe_item_image_generation.rs`, `inventory/item_image_generation.rs:264` (+ five other literal sites that gain `None`) | `preview-avatar/route.ts:129` `{context, characterId}`; `lib/wardrobe/item-image-generation.ts:301` and `lib/inventory/item-image-generation.ts:108` `{context, itemId}`; spread at `params-builder.ts:150, 273` | two optional fields rendered in v4's spread order | the item-picture tier-3 families + B.4's capture | S |
| **C.7** | OPEN | `services/image_profile_resolution.rs:119-141` returns `Option` (swallows); caller `services/tool_image_generation.rs:251` | `lib/wardrobe/tool-image-generation.ts` (`imageProfiles.findDefault` is a RETHROW `safeQuery`, `image-profiles.repository.ts:39-47`, caught → `failed`) | a failing default-profile read answers `failed`, not `no-image-profile` | `wardrobe_tools_equivalence` + a failing-read plant (must fail BOTH reads — memory `throwing-plant-must-fail-both-reads`) | S–M |
| **C.8** | OPEN | `api/chat_outfits.rs:917-926` (DEBUG logged in the `Ok(SetAll)` arm, AFTER the commit) | `app/api/v1/chats/[id]/actions/outfit.ts:163-178` (`resolveWornBundles`, BEFORE `commitEquippedOutfit` at `:205`) | the DEBUG precedes the commit's own lines, and still fires when the commit fails | `wardrobe_wear_commit_tier2_equivalence` (assert ORDER) | S |
| **C.12** | OPEN (status converged) | `services/wardrobe_item_route_steps.rs:170`; caller `api/wardrobe_route_factory.rs:1582` | `lib/wardrobe/item-route-steps.ts:31-46`; `files.findByLinkedTo` rethrow | the 500 already matches; add v4's repository ERROR + middleware `Unhandled route error` on that arm | the wardrobe routes family + an abort plant on `files` | S |
| **D.2** | OPEN | harness only | — | a `chat_gallery` save row with a nameless user seat (`''` byline) | `chat_gallery_equivalence` | S |
| **D.4** | OPEN | `api/chat_media.rs:274, 280, 362, 370` | `app/api/v1/chats/[id]/messages/[messageId]/route.ts:360-368`; `findById` fallback reads | READ failures take the not-found arm through the fallback homes; only the save 500s | `chat_gallery_equivalence` / message Save rows + a corrupt-row plant | S |
| **F.1** | NEEDS-RULING (§D) | `db/chat_wardrobe_history.rs:418-431` (`unwrap_or(0.0)`), carve in `chat_wardrobe_history_tier2_equivalence.rs` | `lib/database/repositories/chat-wardrobe-history.repository.ts` `normalizeRow` (`Number(…)` → NaN; the next settle fails → `skipped`) | per the ruling | that family | S / S–M |
| **F.6** | OPEN | `services/tool_build.rs:287` (`build_tools_for_provider`, no DEBUG) | `lib/tools/plugin-tool-builder.ts:381-382` | the DEBUG with v4's option keys | `pascal_build_tools_roster` / `tools_inventory` capture | S |
| B.11 (T3) | OPEN — recommend OUT | the almanack fixture pair lacks a designated wardrobe image profile | — | grow the builder | `almanack_*` | S–M; a shared-pair rebuild |
| G.1 (T3) | OPEN | `db/files.rs:538` `find_by_linked_to_any` (chunked; unit-tested only) | `files.repository.ts` | a 501-id two-chunk row planted by a recursive-CTE `INSERT … SELECT` on both sides | `quilltap-web/tests/wardrobe_images_routes_equivalence.rs` | S–M |
| announcement job's degraded arm (T3) | NO-ACTION | `services/aurora_notifications.rs:257` | — | P4.166's fail-soft pool covers the read; no measured divergence recorded | — | — |

Traps:
- **C.7** changes a resolver shared by THREE callers (`tool_image_generation.rs:251`, `wardrobe_item_image_generation.rs:298`, `inventory/item_image_generation.rs:178`). Add a strict sibling (`…_strict` → `Result<Option<_>, DbError>`) for the tool path, or measure v4's arm at each of the other two before changing them.
- **F.6**: a new DEBUG on the tool builder reaches every family that captures unfiltered DEBUG from a turn. Whitelisted captures hide it (memory `mutation-restore-mtime-and-capture-whitelists`); unfiltered ones go red until regenerated. Measure which families capture it before landing; `tool_build.rs` is also the Pascal area's file.
- **B.4 and B.7 must land in ONE change**: B.4's capture pins the builder line B.7 changes.
- **D.2 and D.4** share `chat_gallery_equivalence` + its oracle case: one lane.
- B.8 (the `[StoryBackground]` snake_case keys, HANDOFF(P4.166)) is not in the eleven but is still open: `services/story_background_job.rs` carries ~19 `job_id = ` / `chat_id = ` / `profile_id = ` fields. Not this area's file; flag to the planner.

### A10 — P4.167's two Tier-3 items

- **#10, D273-23: a restore onto a DEGRADED mount index   [OPEN — M, measure-and-record].** Never measured. v4 `restore.ts` imports `isMountIndexDegraded` (`:33`); its 22a `findAll` would throw. v5's restore has no mount-degraded arm at all (`orchestrator.rs` — only the logs gates at `:206`, `:1366`, `:2204`). P4.166's degraded-mount refusals have since landed, so the blocker the lane named is gone. Done = one degraded-clone row in `system_restore_state` on both sides and a record of what each does; a fix only if they differ in a way that loses data. Needs a degraded-clone rig for that harness (P4.159's plants are the precedent).
- **#11, the `js_truthy` DRY pass   [OPEN — L; recommend OUT, §E].** Measured: 16 `fn js_truthy` definitions and 58 truthy-class helpers across 53 files in `quilltap-core` / `host` / `web` (two are already `pub`: `inventory/types.rs:187`, `api/system_qtap.rs:380`). It touches every area's files.

---

## B. Other items found in the assigned orders

- **P4.D350 — v5's chat read admits a non-`CHARACTER` participant type   [OPEN — S; meeting point].** v4 `lib/schemas/chat.types.ts:742` `ParticipantTypeEnum = z.enum(['CHARACTER'])` (unchanged at HEAD); a planted `USER` seat makes v4 answer 404 `Chat not found`. v5 `db/chats_read.rs` validates only `scenarioSource` as a Zod row refusal (`:254-296`); `db/chats_participants.rs:302` filters by type instead. Reach: hand-edited rows only. Done = a `ZodRowRefusal` for a participant whose `type` is not `CHARACTER`, with v4's issue bytes, on the fallback `find_by_id` path. The carried list files this under the vault / memory group; `db/chats_read.rs` is that area's file too. The catch-up adds columns to `chats` (`348172b9c`) but does not rewrite the read.
- **P4.D356 F-a — `sidebarWidth` omitted on a NULL cell   [OPEN — S; meeting point].** `db/chat_settings.rs:1896-1903` omits the key, citing "the outer optional keeps an absent key absent". v4 `lib/schemas/settings.types.ts:767` is `z.number().min(256).max(512).default(256).optional()`, and the lane measured `safeParse(undefined)` → `256` (Zod 4 runs the inner default). Done = a NULL cell reads `256`; fix the comment at `:1508-1510` too. Differential: the settings-routes family with a `plantRaw` NULL cell. `chat_settings.rs` is also where the vault area's V12 doc-comment item sits.
- **P4.D356 H4 (settings-routes regenerated last) — CLOSED**: green in the `624d0d25c` unification sweep from the pin. **H8 — CLOSED** at the `236c04e35` unification. **H1 (the SPA Core-whisper card)** — the SPA area's standing deferral, not this survey's.
- **P4.D364 — "OPEN remainder: none"; four recorded observations, all DEFER-TO-CATCH-UP.** (a) `quilltap-web/tests/profile_bound_api_key_wire.rs:54-55` doc still says the ANTHROPIC profile uses "the `x-api-key` header" (Bearer since X2) — a comment; (b) `harness/oracle/cases/primary-stream-tier3.test.ts:113, 132, 145` cite `openai 7.23.0` (the family is green on 7.47.0 now); (c) the validate literal's key order; (d) the capability listing's raw pass-through. The six prompt-cache commits and `ac6b60c21`'s new floors (anthropic 1.4 → 1.7, openai 7.54, openrouter 1.5.73) force a full re-record and re-cite at the next pin. Fold (a) and (b) into that order. (a) alone is a one-line comment any lane touching that file may take.
- **P4.D367 item 17 (the probe's DEBUG on a failing read) — CLOSED** at unification (`3cdf31d6f`). **H6** (stale `adaptMessagesForProfile` comments) — CLOSED: `services/orchestrator.rs:3324` now names the replacement.
- **P4.D378 — nothing open.** `UNION_PENDING_P4D377_RAW_OWNER`: 0 hits on main (retired). `CONSULTED_CROSS_TIER_INTERLEAVE` (`inventory_title_lint_equivalence.rs`, `inventory_pool_order_tier2_equivalence.rs`) is a ruled both-ways carve that v4 `921fe94bb` converges on: **DEFER-TO-CATCH-UP** — retire it by measurement there, and check `unread_mounts` order then.
- **P4.D350 — the rest is NO-ACTION:** item 17's absent mutation unit test (bytes equal, 101 cases), item 26 (`mountOwnerResolver` memo, perf only), Tier 3 item 28 (unreachable arm).
- **P4.D377 item 27** (six non-store callers of `doc_mount_file_links::normalise_relative_path` on a different v4 normaliser) — recorded pre-existing divergence, unmeasured reach. NO-ACTION this round unless the documents area takes it; its files are that area's.
- **P4.167 "recorded, not ported":** the repository-level `JsonSchema` twin (v4 validates in `_create`) and three unmeasured carrier edges (a carried row's non-string `mimeType`, a non-scalar NDJSON `kind`, a non-string orphan parent id). NO-ACTION: hand-edited archives only, and `ebb0ca13a` adds a new carrier at the catch-up where the edges can be measured once.

---

## C. Drift watch (ledger §3) for this area

| v4 commit(s) past the pin | Surface | Effect on this round |
|---|---|---|
| `8dfe307f5`, `b9b9621e8`, `4b97636b2`, `f5d37e718`, `78aca159d`, `819fc9c82`, `ac6b60c21` | provider builders, recorded corpora, SDK floors, `previousResponseId` | every provider-corpus item waits: A2, the D364 notes. A1 does NOT wait (it is about which tree the guard reads). |
| `0ae0ba3a7`, `4d3b00621`, `89f259c2f`, `3db965416`, `348172b9c` | Green Room outfit selection, arrival inventory, chat create | none of this survey's OPEN items sits on `outfit_selections.rs` / `chat_create.rs` / `apply-defaults`. C.8 is the operator `set_all` route (`actions/outfit.ts`, unchanged past the pin). |
| `fee1bd63d`, `aefbdde05` | two NEW wardrobe route-factory actions (+36 lines in `wardrobe-route-factory.ts`) | additive. A4's rows and C.12 are safe; expect `wardrobe-folders-routes.json` to grow again there. |
| `921fe94bb` | Inventory pool order | the D378 carve — DEFER. |
| `0d73a4aee` | Wear again staging (SPA only) | not this area. F.1 is the server settle and is untouched. |
| `ebb0ca13a` | avatar-timeline carriers (backup / restore / `.qtap`, `uuid-remap.ts` +22) | additive. A6 / A7 are safe; the uuid-remap corpus regenerates again there. |
| `1dc11e947` | `CharacterSchema` split, `characters.repository.ts` | A5 — DEFER. |

---

## D. Rulings needed

1. **F.1 — a non-numeric `entryIndex` in `chat_wardrobe_history`.** v4 reads NaN and the next settle fails, wedging the timeline (`skipped` forever); v5 reads 0 and appends entry 1 (`NAN_ENTRY_INDEX_READS_ZERO`). Options: (A) converge — NaN-aware read, `Skipped` + v4's WARN on a non-finite max; (B) keep v5's append as a ruled divergence, DRIVE the row so the carve is measured rather than reasoned, and draft a v4 issue. **Recommend (B)**: reach is a hand-edited row, and wedging a chat's history is the worse behaviour. Either way the settle row gets driven this round.
2. **The `js_truthy` DRY pass (P4.167 #11) — in or out.** 58 helpers in 53 files. **Recommend OUT of this round** and scheduled as a solo, last-stage lane after the catch-up (which rewrites many of the same files), or dropped as cosmetic. If it must run now it cannot run beside any other server lane.

---

## E. Proposed lanes (edited source files disjoint)

**Lane C2-W — wardrobe / image server smalls** (B.3, B.4 + B.7 as one change, B.6, C.7, C.8, C.12, D.2 + D.4, F.1 per ruling, F.6). Size M–L; split F.6 off if the Pascal area owns `tool_build.rs` this round.
- Owns: `services/chat_avatars.rs`; `image_gen/params_builder.rs` + its literal sites (`api/wardrobe.rs:853`, `services/wardrobe_item_image_generation.rs`, `inventory/item_image_generation.rs`, `image_gen/image_attempt.rs`, `tools/generate_image.rs`, `api/images.rs`, `services/image_job_common.rs` — a `None` field each); `services/image_profile_resolution.rs`; `services/tool_image_generation.rs`; `api/chat_outfits.rs`; `services/wardrobe_item_route_steps.rs`; one arm of `api/wardrobe_route_factory.rs` (`:1582`); `api/chat_media.rs`; `db/chat_wardrobe_history.rs`; `services/tool_build.rs`.
- Harness: `chat_cast_routes_equivalence`, `quilltap-web/tests/wardrobe_routes_equivalence.rs`, `wardrobe_item_image_generation_tier3_equivalence`, `wardrobe_tools_equivalence`, `wardrobe_wear_commit_tier2_equivalence`, `chat_gallery_equivalence`, `chat_wardrobe_history_tier2_equivalence`, `pascal_build_tools_roster`, and their oracle cases.

**Lane C2-R — item-route differential rows, harness only** (P4.D377 items 6 and 10; G.1). Size M. No production edit expected; a red row is a finding to fix in `api/wardrobe_route_factory.rs` (coordinate with C2-W's one arm there) or `db/files.rs`.
- Owns: `harness/oracle/cases/wardrobe-folders-routes.test.ts`, `harness/oracle/fixtures/wardrobe-folders-routes.json`, `quilltap-web/tests/wardrobe_folders_routes_equivalence.rs`, `harness/oracle/cases/wardrobe-images-routes.test.ts` + `fixtures/wardrobe-images-routes.json`, `quilltap-web/tests/wardrobe_images_routes_equivalence.rs`.

**Lane C2-K — carriers** (A7 K7, A6 K6, A10 D273-23 as measure-and-record). Size M.
- Owns: `services/backup/restore/orchestrator.rs` (the 22n-quater block only), `crates/quilltap-harness/tests/system_restore_state.rs` + its oracle case, `harness/oracle/cases/backup-uuid-remap.test.ts`, `harness/oracle/fixtures/uuid-remap-corpus.json`, `backup_uuid_remap_equivalence.rs` (header only if the recipe moves).

**Lane C2-G — the v4-root guards** (A1). Size S, harness only. Can be a rider on any harness-smalls lane.
- Owns: `provider_sdk_version_guard.rs`, `zod_version_guard.rs`, `builtin_prompt_templates_guard.rs` (headers), `harness/tools/recipe_sweep.py` (the optional env export + a self-test row).

**Riders to hand to other areas' lanes** (not a lane of their own): P4.D377 item 14's tier-2 rows → the documents / mount-store lane (`doc_mount_folders_tier2_equivalence`); the participant-type refusal → whoever owns `db/chats_read.rs`; `sidebarWidth` F-a → whoever owns `db/chat_settings.rs`; B.8 → whoever owns `services/story_background_job.rs`.

### Meeting points

- `api/wardrobe_route_factory.rs` — C2-W (C.12's arm) and any finding from C2-R.
- `services/tool_build.rs` — C2-W (F.6) and the Pascal area.
- `api/wardrobe.rs` — C2-W (the preview's log context) and any wardrobe SPA-facing server change.
- `db/chat_settings.rs`, `db/chats_read.rs` — the vault / memory area.
- `services/backup/restore/orchestrator.rs` — C2-K owns one block; the files / Log area's K10 / K12 are elsewhere but its restore items, if any, meet here.
- `harness/tools/recipe_sweep.py` — C2-G and any harness-smalls lane.
- `doc_mount_folders_tier2_equivalence` — the documents area.
- `harness/oracle/fixtures/wardrobe-folders-routes.json` — a shared corpus; only C2-R writes it.

---

## F. Recommended OUT of the round

- **A2** (the cap-2-understudy row) — the family's seam is re-cut by `4b97636b2`; carry into the catch-up.
- **A5** (the `.qtap` kind import's issue list) — `1dc11e947`.
- **The D364 notes and every recorded-corpus item** — re-recorded at the next pin.
- **The D378 interleave carve** — retires by measurement at `921fe94bb`.
- **The `js_truthy` pass** — per ruling 2.
- **B.11** (the almanack pair's wardrobe profile) — a shared-pair rebuild for a log key B.7 already pins elsewhere.
- **A3** — no action.
- **P4.167's unmeasured carrier edges and the repository-level `JsonSchema` twin** — measure once with the avatar-timeline carrier.

---

## Per-order index

| Order | Header says | Measured now |
|---|---|---|
| P4.D350 | OPEN: the participant-type finding; dogfood rows | the finding OPEN (§B); rest NO-ACTION |
| P4.D356 | OPEN: H1, H4, H9 | H4 CLOSED, H9 CLOSED (A8), H1 is the SPA area's; F-a OPEN (§B) |
| P4.D364 | none | four observations, DEFER (§B) |
| P4.D367 | item 16's row OPEN | A2 OPEN, recommend OUT; item 17 CLOSED; the static-mirror note NO-ACTION (A3) |
| P4.D377 | items 6 / 10 / 14 / 18 | 18 CLOSED; 6, 10, 14 OPEN (A4); item 27 NO-ACTION |
| P4.D378 | none | none; the interleave carve DEFER |
| P4.167 | Tier 3 #10, #11 | both OPEN (A10); #11 needs the ruling |
| P4.170 | eleven Tier 2 + three Tier 3 | ten OPEN, F.1 NEEDS-RULING; G.1 OPEN, B.11 OUT, the degraded arm NO-ACTION (A9) |
