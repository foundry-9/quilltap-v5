# Survey D — wardrobe programme, outfit history, item images, gallery, degraded-mount-index class

Main @ `4e43b714e` (2026-10-27). v4 read at `322a5488e` only. Paths are repo-relative to
`/Volumes/External 1/source_overflow/quilltap-v5` (v5) or the v4 pin.

**Tally: 42 OPEN · 5 CLOSED · 3 NEEDS-RULING · 19 NO-ACTION** (several NO-ACTION rows bundle multiple Tier-3 numbers) (NO-ACTION = recorded NO-PORT / deferral to an
unported vertical / human filing / v4-faithful).

Suggested lane split (see §A for #156): **Lane W1** = #156 entity-route chokepoint (core, M–L);
**Lane W2** = wardrobe fail-soft reads on a degraded mount + announcement job (core+harness, M);
**Lane W3** = wardrobe/route/log smalls (core+web+harness, M); **Lane W4** = SPA/e2e smalls (S–M).

---

## §A — Dogfood #156 / #157 and the degraded-mount-index class

### 156 — `with_both_conns` answers 500 `image job requires the mount-index database` on a degraded mount index   [OPEN]
- Source: `dogfood-findings.md:15` (row) + `:862` (standing order); P4.D275 T3 #20, P4.D281 OPEN ("two equip-adjacent routes"), P4.D315 OPEN (GET history — §A.2).
- v5 site: `crates/quilltap-core/src/services/image_job_common.rs:445-462` — `writers.mount_index().ok_or_else(|| DbError::Internal("image job requires the mount-index database"))`; `api/types.rs:5765-5775` `db_error_response` maps only `StoreUnavailable` → 503, every other `DbError` → `Internal` with the raw text.
- **Re-count on main today: 88 call sites** (was 115 at the finding; refactors folded some), plus `scenarios.rs:420 run()` is itself a family chokepoint behind N scenario routes. Command:
  `grep -rn 'with_both_conns(' crates/quilltap-core/src --include=*.rs | grep -v 'fn with_both_conns' | grep -v '//'` → 88.
  Full file:line + enclosing-fn list: `the appendix below`.
- **Classification against v4 @322a5488e.** v4 has no up-front refusal: every mount-index repo acquires through `requireMountIndexDb()` (`lib/database/backends/sqlite/mount-index-guard.ts:18-20`, throws `Mount index database is in degraded mode`) INSIDE `withRawDb`, and the overlay reads (`docMountDocuments.findManyByMountPointsAndPath`, `doc-mount-documents.repository.ts:142-167`) are FALLBACK reads → `[]`. So an entity load sees "keystone missing" → `CharacterVaultUnavailableError` (`vault-overlay/read-overlay.ts:160`) / `Project|GroupStoreUnavailableError` → middleware 503 (`lib/api/middleware/context.ts:176-205`) + ERROR `[METHOD url] … unavailable`; anything that WRITES the mount throws the guard sentence → middleware 500 `Internal server error` + ERROR `[METHOD url] Unhandled route error` (`context.ts:206-207`).

  | Class | v5 sites (count) | v4 answer at the analogous site | Fix |
  |---|---|---|---|
  | **C1 entity-first, project** | `projects.rs` 572 update, 633 delete, 745/799 character add/remove, 961/999 chat add/remove (`handleAddChat` `app/api/v1/projects/[id]/actions/chats.ts:137` = the measured case), 1054/1079 state, 1115 tool settings, 1296 aesthetic, 1363/1402 mount link/unlink, 1816/1859 file add/remove (14) | `repos.projects.findById` → overlay → 503 `Project document store unavailable` `{error, projectId}` | gate → `DbError::StoreUnavailable{project}` |
  | **C1 group** | `groups.rs` 452, 480, 508, 539, 591, 618, 795, 915, 945 (9) | 503 `Group document store unavailable` | same, group |
  | **C1 character** | `characters.rs` 315, 924, 950, 974, 1014, 1067, 1133, 1182, 1496, 1691, 2153, 2278, 2412, 2440, 2475, 2503, 2539, 2572, 2604, 2632, 2664 (21) | `repos.characters.findById` → 503 `Character vault unavailable` `{error, characterId}` for a vaulted character (every post-cutover character); a vault-less row proceeds to a mount write → 500 | gate → `StoreUnavailable{character}` when the raw row has `characterDocumentMountPointId`, else the C3 500 |
  | **C1 tier factories** | `wardrobe_route_factory.rs` 732, 822, 851, 979, 1251, 1405, 1508, 1536 (8); `inventory_route_factory.rs` 613, 935, 1133, 1198, 1243, 1280 (6); `scenarios.rs` 424 (`run`, all scenario routes), 776, 818 (3) | the location resolve loads the owner (character/project/group) → 503 per entity; **General tier: MEASURE** (v4 `resolveWardrobeLocation('general')` reads `docMountPoints` — likely fallback → `{wardrobeItems: []}` / write 500 `Quilltap General store is not provisioned yet`) | gate by scope; General measured |
  | **C2 fail-soft wardrobe reads** | `chat_outfits.rs` 1010 `put_on_or_fit`, 1198 `chat_wardrobe_rollback` (2) | v4 `loadWearablePool` (`lib/wardrobe/pool.ts:257-319`) fails soft per tier → EMPTY pool → equip answers the not-found 404 via `reason`; rollback resolves against an empty pool (skipped items) | pool over `Option<&Connection>` (§A.2) |
  | **C3 creates / cross-entity** | `projects.rs` 264/302 create, `groups.rs` 302/337 create, `characters.rs` 1292/1347/1423 create/quick/import, `wardrobe.rs` 396 transfer apply, `inventory.rs` 91 transfer apply, `wardrobe.rs` 702/775/983 preview-avatar (13) | the official-store / vault provisioning (or the destination write) throws the guard → 500 `Internal server error` + `Unhandled route error`. **Preview-avatar + transfers: MEASURE** (v4 may 503 first on the character load) | neutral `PartitionUnavailable` → the edge's generic 500 body |
  | **C4 jobs** | `story_background_job.rs` 294/457/890/1107/1131, `character_avatar_job.rs` 215/279/381/785/923, `headshoulders_backfill_job.rs` 181/304 (12) | handler throws the guard sentence → job FAILED/retry with that `error` text | neutral refusal only (text = guard sentence via `fallback::error_text`) |
  | **C5 already fail-soft** | `appearance_resolution.rs:353 with_carrying` (1) | v4 `withCarrying` fails soft (`Could not add carried objects…`) | none (v5 already logs; text follows the neutral refusal) |

  Total 14+9+21+17+2+13+12+1 = 88 (wardrobe_route_factory/inventory/scenarios = 17 in C1-tier).
- Fix shape (split): **(1) ONE chokepoint change** — `with_both_conns` refuses with `DbError::PartitionUnavailable(WriteDbTarget::MountIndex)` (neutral; `fallback::error_text` renders v4's guard sentence), which fixes C3/C4/C5 text in one edit, and the generic 500 arm answers v4's `Internal server error` (check each family's existing 500 body). **(2) ONE entity gate** — a sibling `with_both_conns_for(db, EntityRef{label,id}, f)` (or a pre-gate `require_store(db, EntityRef)`) that, when the mount writer is absent, resolves the entity through the READ path's degraded overlay (the model is the chat-PUT project gate in `api/salon.rs`, which already answers 503 on this exact clone — `db/fallback.rs:255-262` names it) and returns `DbError::StoreUnavailable`; C1's 72 sites swap the call name (mechanical). **(3) Per-site** only for C2 (§A.2). Don't do 88 hand-edits.
- Differential: extend `crates/quilltap-harness/tests/degraded_sibling_open_equivalence.rs` (P4.159's degraded-clone rig) with one route row per family (project/group/character/tier factory/General/create/job), plus each family's route test (`projects_routes_equivalence`, `groups_routes_*`, `characters_mutations_equivalence`, `wardrobe_routes_equivalence` [web], `inventory_routes_*`, `scenarios_routes_equivalence`) gaining a degraded row; byte-compare status + body + the middleware ERROR (`CoreError::log_store_unavailable`, `api/types.rs:5809`).
- Files a lane would own: `services/image_job_common.rs`, `api/types.rs` (db_error_response), `api/{projects,groups,characters,scenarios,wardrobe_route_factory,inventory_route_factory,wardrobe,inventory}.rs` (call-name swaps), `db/fallback.rs` (if the gate lives there), the harness families above.
- Size: **L** (one lane; mechanical swaps after a S keystone).

### 156.a — GET `?action=wardrobe-history` 500s on a degraded mount   [OPEN]
- Source: P4.D315 Unification OPEN; phase-4.md NEXT 2.
- v5 site: `crates/quilltap-core/src/api/chat_outfits.rs:286-389` — `db.read_mount_index(|mount| load_wearable_pool(…))` (`:303`) → `Err(PartitionUnavailable)` → ERROR `[Chats v1] Error reading wardrobe history` + 500 `Failed to read wardrobe history`.
- v4 site: `app/api/v1/chats/[id]/actions/outfit.ts:306-364` — `loadWearablePool` fails soft per tier (`lib/wardrobe/pool.ts:257-319`, WARNs `[WearablePool] Failed to read the character wardrobe; using shared tiers alone` / `… group wardrobe tier; skipping it`) → **200** with every slot `{title:null, missing:true}`.
- Fix shape: give the pool loader a degraded arm (`load_wearable_pool` over `Option<&Connection>` or a `load_wearable_pool_degraded(main, character_id, opts)` that returns an empty pool with v4's per-tier WARNs carrying the guard sentence); the GET calls it when `read_mount_index` answers `PartitionUnavailable`. Same helper serves 156 C2, the announcement job (§B.2) and R-10 (156.b).
- Differential: `chat_wardrobe_history_tier2_equivalence.rs` / `chat_export_equivalence.rs` (the GET's cases, R-V) + a degraded row; v4 side needs the degraded-mode rig P4.159 used.
- Files: `api/chat_outfits.rs`, `wardrobe/wearable_pool.rs` (or wherever `load_wearable_pool` lives), harness family.
- Size: M (shared helper) — put in Lane W2.

### 156.b — R-10: the Workbench seated preview 500s on a degraded mount (route-level byte compare owed)   [OPEN]
- Source: P4.D319 lane record (status-log ~189185, ~189257), P4.D316/phase-4 NEXT 2 ("R-10's route-level status").
- v5 site: `crates/quilltap-core/src/api/custom_tools.rs:1077-1125` `resolve_metadata` — `db.read_mount_index(…)` → `Err` → `internal(e)` 500 BEFORE the lens (`pascal/bench_wardrobe.rs:142-180`, whose WARN arm is pinned only at the lens).
- v4 site: `app/api/v1/custom-tools/route.ts:176-177,228-233` — the overlay's fallback reads return `[]`, so a VAULTED seated character raises `CharacterVaultUnavailableError` → **422** with its message; a vault-less one proceeds → 200 with no `wardrobeResolution` key (the order's R-10 premise).
- Fix shape: on `PartitionUnavailable`, run `apply_document_store_overlay_one` against an empty doc set (→ `OverlayOneError::Unavailable` → the existing `unprocessable(u.message())` arm) — i.e. mirror v4's fallback, not a 500. Then byte-compare both arms.
- Differential: `crates/quilltap-harness/tests/pascal_workbench_route_equivalence.rs` (+ a degraded plant) / `pascal_bench_wardrobe_equivalence.rs`.
- Files: `api/custom_tools.rs`, the harness family.
- Size: S–M.

### 156.c — P4.D324 NOTE: the merge's degraded early return skips v4's per-character arrival WARN   [OPEN]
- Source: P4.D324 status header NOTEs.
- v5 site: `crates/quilltap-core/src/services/chat_merge.rs:458-463` — `let Some(mount_w) = w.mount_index() else { return Ok(Vec::new()); };` skips identity compile, the outfit apply AND the inventory arrival silently.
- v4 site: `lib/chat/apply-chat-merge.ts` (~228-270): compile in try/catch → WARN `[ChatMerge] Failed to compile identity stack for merged participant`; outfit in try/catch → ERROR `[ChatMerge] Failed to apply outfit for merged participant {targetChatId, characterId, mode, error}`; `applyInventoryDefaults` → per-character WARN `[InventoryArrival] Failed to apply arrival defaults for a character {chatId, characterId, source, context:'inventory', error}` (`lib/inventory/apply-defaults.ts:100-106`); then INFO `[ChatMerge] Merged character into chat`.
- Fix shape: in the degraded arm emit the lines v4 would (each with the guard sentence) instead of `Ok(Vec::new())`; measure which of the three v4 actually reaches (the identity compile may fail soft earlier). Note the `let _ =` at `:465` and `:496-497` — confirm the callee logs v4's lines on the non-degraded failure path too.
- Differential: `chat_merge_*` family (grep `crates/quilltap-harness/tests/*merge*`) + degraded row; or unit-only capture (degraded v4 rig cost).
- Files: `services/chat_merge.rs`.
- Size: S.

### 157 — `Failed to log LLM call` on a degraded llm-logs partition: v5 keys + v5 error text   [OPEN]
- Source: `dogfood-findings.md:14`, `:863`.
- v5 site: `crates/quilltap-core/src/services/llm_logging.rs:351-358` — `tracing::error!(log_type, provider, error = %error, "Failed to log LLM call")`; `error` Display = `partition not available: llmLogs`.
- v4 site: `lib/services/llm-logging.service.ts:238-244` — `logger.error('Failed to log LLM call', { userId, type, provider, model, error: <message string> })`; guard sentence `LLM logs database is in degraded mode` (`lib/database/backends/sqlite/llm-logs-guard.ts:21`).
- **Lines to emit (exactly one line, two defects):**
  1. ERROR `Failed to log LLM call` with context keys `userId`, `type`, `provider`, `model`, `error` (in that order).
  2. `error` = `crate::db::fallback::error_text(&e)` (renders `LLM logs database is in degraded mode` for `PartitionUnavailable(LlmLogs)`, bare message otherwise).
  ⚠ v4 passes `error` as a CONTEXT string, not winston's error arg; v5's file layer (`crates/quilltap-web/src/log_file.rs:1140-1158`) turns a field named `error` into the envelope `error` object — use `errorJson = %serde_json::to_string(&text)` so it lands at `context.error` (the `…Json` suffix rule, `log_file.rs:1134-1136`).
- Fix shape: rename keys (`log_type`→`type`, add `userId`, `model`), error via `error_text`, `errorJson` field.
- Differential: unit capture in `llm_logging.rs` tests (the posed degraded arm); existing `degraded_sibling_open_equivalence` llm-logs arm if it captures lines.
- Files: `services/llm_logging.rs`.
- Size: S.

### 156.d — P4.D324 NOTE: `chats_read::select_list` probes `pragma_table_info` on every read   [OPEN]
- Source: P4.D324 status header NOTE.
- v5 site: `crates/quilltap-core/src/db/chats_read.rs:116-129` — `prepare("SELECT 1 FROM pragma_table_info('chats') WHERE name = 'equippedInventory'")` per list read.
- v4: n/a (perf; v4 has the column via migration).
- Fix shape: cache per connection — the column can only appear, never disappear, so cache only the positive answer (e.g. a `thread_local!`/`OnceLock` keyed by `conn.handle()` pointer, or `prepare_cached` at minimum). Keep the absent arm for fixtures that predate the column.
- Differential: unit-only (no behaviour change; the existing `chats_read` tests for the absent column stay green).
- Files: `db/chats_read.rs`.
- Size: S.

---

## §B — P4.D282 / P4.D283 (outfit pipeline, image side)

### B.1 — P4.D282 behavioural turn-end-drain row   [OPEN — mostly closed; residual line pin]
- Source: P4.D282 Unification OPEN; phase-4 NEXT 3.
- v5 site: the drain is `services/outfit_change_effects.rs:350-353` (`orchestrator-turn-end`), called once from `services/orchestrator.rs:3685`. **Mostly CLOSED by P4.D316 Tier 2 item 25:** `crates/quilltap-harness/tests/orchestrator_tier3_equivalence.rs:67-79, 3558-3667` drives a canned turn with three wardrobe calls and compares `background_jobs` whole (ONE `WARDROBE_OUTFIT_ANNOUNCEMENT`) + `chat_wardrobe_history`. The source census `outfit_change_effects_tier3_equivalence.rs:847` remains.
- Residual: the turn-level family does not capture the `orchestrator-turn-end` DEBUG (only the effects-module family does, `:773-781`).
- Fix shape: add the DEBUG to the orchestrator family's captured lines for `wardrobe_wear_then_take_off_one_turn`, then retire the source census.
- Differential: `orchestrator_tier3_equivalence.rs` + `harness/oracle/cases/orchestrator*.ts`.
- Files: harness only. Size: S.

### B.2 — `WARDROBE_OUTFIT_ANNOUNCEMENT` job: no differential + two missing v4 lines   [OPEN]
- Source: P4.D282 BANKED (confirmed real).
- v5 site: `crates/quilltap-core/src/services/aurora_notifications.rs:257-331` `handle_wardrobe_outfit_announcement` — no INFO at start; a non-object slot shape returns silently (`:298-300`); per-element validation skipped (`Slots::from_value` lenient); a degraded mount returns `Ok(None)` silently (`:282-284`) where v4 proceeds with a fail-soft pool.
- v4 site: `lib/background-jobs/handlers/wardrobe-announcement.ts:33-38` INFO `[WardrobeAnnouncement] Starting announcement {context:'background-jobs.wardrobe-announcement', jobId, chatId, characterId}`; `:52-63` `EquippedSlotsSchema.safeParse` failure → WARN `[WardrobeAnnouncement] equippedOutfit slot shape failed validation, skipping {context, chatId, characterId, issues: first 3}`.
- Fix shape: add both lines (`issuesJson` for the array); port `EquippedSlotsSchema` validation (UUID elements) or reuse an existing Rust twin; take `jobId` (handler signature needs the job); degraded arm through §A.2's fail-soft pool.
- Differential: NEW tier-3 family `wardrobe_announcement_job_tier3_equivalence.rs` + `harness/oracle/cases/wardrobe-announcement-job.ts` (v4's real handler over a fixture: posted whisper row, the two lines, malformed-slot + missing-chat + no-slots rows). Also covers `build_context.rs:1194 resolve_live_clothing` if planned in the same corpus (P4.D282 BANKED names it).
- Files: `services/aurora_notifications.rs`, `services/job_runner.rs` (payload/jobId plumbing), new harness family.
- Size: M.

### B.3 — Toggle-avatar route's outer catch   [OPEN]
- Source: P4.D282 Unification OPEN.
- v5 site: `crates/quilltap-core/src/services/chat_avatars.rs:284-330` — every `?` on `read_main`/`write` maps through `impl From<DbError> for AvatarError` (`:67-71`) → 500 with the raw DB text; no ERROR line. Route: `api/chat_cast.rs:772-779`.
- v4 site: `app/api/v1/chats/[id]/actions/toggle-avatar-generation.ts:27,97-100` — outer try/catch → ERROR `[Chats v1] Error toggling avatar generation {chatId, context:'avatar-generation'}` (+error) and 500 `Failed to toggle avatar generation`.
- Fix shape: wrap the pre-sweep body; any `DbError` → that ERROR + `AvatarError::internal("Failed to toggle avatar generation")`. Measure v4's `chats.findById` (fallback? → `Chat not found` 500 instead).
- Differential: `chat_cast_routes_equivalence.rs` (toggle rows) + a failing-write plant (abort trigger, the `wardrobe_wear_commit_tier2` `plantSql` precedent).
- Files: `services/chat_avatars.rs`. Size: S.

### B.4 — P4.D283: `[Avatar Preview]` lines + preview `IMAGE_GENERATION` `llm_logs` rows unpinned   [OPEN]
- Source: P4.D283 Unification OPEN.
- v5 site: `crates/quilltap-core/src/api/wardrobe.rs:818, 918, 1041, 1065` (the four lines exist); the web routes family opens `llm_logs: None`, so neither lines nor rows are compared.
- v4 site: `app/api/v1/wardrobe/preview-avatar/route.ts:104, 143, 207, 224-225`.
- Fix shape: give the preview rows an llm-logs partition and capture the four lines; compare `llm_logs` rows (normalised).
- Differential: `crates/quilltap-web/tests/wardrobe_routes_equivalence.rs` `pv_*` rows + `harness/oracle/cases/wardrobe-routes*.test.ts`.
- Files: harness/web tests only. Size: S–M.

### B.5 — P4.D283: two silent `.ok().flatten()` chat-settings reads   [OPEN]
- Source: P4.D283 Unification OPEN.
- v5 sites: `crates/quilltap-core/src/api/wardrobe.rs:823-827`; `crates/quilltap-core/src/services/wardrobe_item_image_generation.rs:282-284`.
- v4 site: `lib/database/repositories/chat-settings.repository.ts:39-46` — `safeQuery(…, 'Error finding chat settings by user ID', {userId}, null)` (fallback with ERROR).
- Fix shape: extract ONE fallback home `chat_settings::find_by_user_id_or_none(main, user_id)` (logging v4's ERROR as `wardrobe_tool_images_enabled` does at `db/chat_settings.rs:1724-1737`) and use it at both sites; also fold the three inline copies (`chat_settings.rs:1724`, `services/headshoulders_backfill_job.rs:223-236`, `services/extraction/context.rs:414-425`) — DRY.
- Differential: unit capture (posed failing read); no oracle row needed (line text pinned at `chat_settings.rs:2708`).
- Files: `db/chat_settings.rs`, the five callers. Size: S.

### B.6 — P4.D283: `Outfit expansion stopped short` untested   [OPEN]
- v5: `services/wardrobe_item_image_generation.rs:245-251` logs `cyclesJson` (the file layer renders it as `cycles` — the shape is likely right); v4 `lib/wardrobe/item-image-generation.ts:131-136` `{context, itemId, cycles, truncated}`.
- Fix shape: a cyclic-composite row in the item-picture family pinning the line (file-envelope shape).
- Differential: `wardrobe_item_image_generation_tier3_equivalence.rs`. Files: harness. Size: S.

### B.7 — P4.D283 / P4.D263: `ImageParamsLogContext` lacks `characterId` (preview) and `itemId` (item picture)   [OPEN]
- v5: `crates/quilltap-core/src/image_gen/params_builder.rs:119-128` (no `character_id` / `item_id`); preview builder `api/wardrobe.rs:856`.
- v4: `lib/image-gen/…params-builder` `logContext` carries `characterId` / `itemId` (re-read at the pin for the exact key set + order).
- Fix shape: add the two optional fields, render in v4's order, set at the preview and item-picture call sites.
- Differential: `lora_log_anchor_guard.rs` / the item-picture tier-3 family's captured builder line. Files: `image_gen/params_builder.rs`, callers. Size: S.

### B.8 — P4.D283: `[StoryBackground]` lines log snake_case keys   [OPEN]
- v5: `crates/quilltap-core/src/services/story_background_job.rs` — 38 snake_case field lines (e.g. 383-384, 416-418, 431, 545, 549, 577, 622-624, 656-657, 735-742, 769-783, 815, 909, 980-983, 1002-1006, 1024-1028, 1623).
- v4: `lib/background-jobs/handlers/story-background*.ts` — camelCase (`jobId`, `chatId`, `profileId`, …).
- Fix shape: rename to v4's keys line by line against the pin; pin a representative subset.
- Differential: `story_background_job_tier3_equivalence.rs` line capture. Files: that job + family. Size: S–M.

### B.9 — P4.D283 T3 #11 `analyze-image`; P4.D286 T3 #12 import-from-image link flow; P4.D261 T3 #24 `keepPhotograph`   [NO-ACTION]
- Standing deferral R-L (the unported `analyze-image` vertical). Confirmed still absent (`grep keepPhotograph apps/web/src` → 0).

### B.10 — P4.D283 T3 #12 `scene-state-tracking.ts:214`'s pick   [NO-ACTION] — job handler unported.

### B.11 — P4.D283 T3 #13 (P4.D263 carried: almanack `.db` pair's designated wardrobe profile)   [OPEN]
- The `itemId` half is B.7. The almanack fixture pair still lacks a designated wardrobe image profile (no `wardrobeImageSettings` profile row driven). Fix: grow the almanack builder; Differential: `almanack_*_equivalence`. Size: S–M (fixture rebuild — mind the shared-fixture vintage rule).

---

## §C — P4.D280 / P4.D279 / P4.D281 (tools, keystone, route factory)

### C.1 — `run_wardrobe_tool` ignores `written` when a stage is stashed   [OPEN]
- Source: P4.D280 Unification OPEN.
- v5 site: `crates/quilltap-core/src/tools/executor.rs:1772-1793` — the closure stashes the stage, then `if let Some(stage) = stash.take()` proceeds to `finish_wardrobe_tool` (effects, picture, success) even when `written` is a failure (the commit/`db.write` returned `Err` after the closure ran, `:2176-2195` → `fail("wardrobe", e)`). **Same bug in the inventory twin** `executor.rs:1857-1880` (`run_inventory_tool`).
- v4 site: `lib/chat/tool-executor.ts` (~725-807) — a thrown write never reaches the effects.
- Fix shape: `if !written.success { return written; }` before taking the stash (drop the stage) in both runners; add a unit/tier-2 row with a commit-failing plant (abort trigger on the vault doc table).
- Differential: `wardrobe_tools_equivalence.rs` (+ `plantSql` abort row) / `tools_inventory` family.
- Files: `tools/executor.rs`. Size: S.

### C.2 — 53-op (now 58-op) corpus gaps   [OPEN]
- Source: P4.D280 Unification OPEN.
- v5: `harness/oracle/fixtures/wardrobe-tools.json` (58 ops, listed by `ops[i].label`). Missing: `wardrobe_take_off` remove of a not-found title; `wardrobe_wear` not-found (`Ghost Cloak`) and archived refusal (e.g. wear `Red Hat` after op 32 `archive_own`); `wardrobe_update` / `wardrobe_archive` with NO location (the vault-less participant the fixture already carries, `QT_WT_TOOLS_CORPUS=1`).
- Fix shape: append 5 ops at the END (keeps prior op indices), regen PINNED.
- Differential: `crates/quilltap-harness/tests/wardrobe_tools_equivalence.rs` + `harness/oracle/cases/wardrobe-tools.ts`.
- Files: fixture JSON + recipe. Size: S.

### C.3 — `[ItemMutations] Updated wardrobe item` `fields` key order   [NEEDS-RULING]
- Source: P4.D279 Unification OPEN.
- v5 site: `crates/quilltap-core/src/services/wardrobe_item_mutations.rs:341-353` `ITEM_PATCH_KEY_ORDER` (title, types, componentItemIds, description, imagePrompt, appropriateness, isDefault, replace, imageFileId, migratedFromClothingRecordId, folder) + `patch_keys` `:360-388`; logged `:451-460`.
- v4 site: `lib/wardrobe/item-mutations.ts:149-180` — `fields: Object.keys(next)` where `next = {...patch minus archived}` (caller's insertion order), `folder` deleted if same, `archivedAt` appended, `types` appended when recomputed but not restated. Callers order differently: **route** = Zod `updateWardrobeSchema` shape order (`lib/schemas/wardrobe.types.ts:155-190`: title, description, imagePrompt, types, appropriateness, isDefault, componentItemIds, replace, folder, imageFileId) via `wardrobe-route-factory.ts:495-506`; **tool** = `lib/tools/handlers/wardrobe-update-handler.ts:99-107` (title, description, imagePrompt, appropriateness, types, isDefault, replace, componentItemIds). **v5's fixed order matches NEITHER.**
- Options: (A) **converge** — `ItemPatch` carries its key order: two `const` arrays (`ROUTE_PATCH_ORDER`, `TOOL_PATCH_ORDER`) chosen by the caller (an `order: &'static [&str]` field or a `PatchOrigin` enum); `patch_keys` filters that order instead of `ITEM_PATCH_KEY_ORDER`. ~20 lines + two pins. (B) **ruled divergence** `ITEM_MUTATIONS_FIELDS_ORDER` — DEBUG-only, no wire impact; record both-ways carves in the route + tool families.
- **Recommendation: (A) converge** (the standing default; cheap; both callers' families already capture v4's lines per op — `wardrobe_tools_equivalence` R-H by tag, the route families by line).
- Differential: `wardrobe_tools_equivalence.rs` (op 20/23/24/25 update rows) + `crates/quilltap-web/tests/wardrobe_routes_equivalence.rs` `update_*` rows (unmask the `fields` key).
- Files: `services/wardrobe_item_mutations.rs`, `tools/wardrobe_update.rs` (caller), `api/wardrobe_route_factory.rs:1405` region. Size: S.

### C.4 — `vault_character_write` managed-field projections log the mount id as `characterId`   [OPEN]
- Source: P4.D279 Unification OPEN.
- v5 site: `crates/quilltap-core/src/db/vault_character_write.rs:346-357` and `:366-377` pass `mount_point_id` as `scope_id` (the `characterId` of the stale-delete WARN, `db/vault_wardrobe_write.rs:262-267`). `vault_character_update.rs:269/291` already passes `character_id` (correct).
- v4 site: `lib/database/repositories/vault-overlay/managed-fields.ts:302-311, 314-323` pass `character.id`; WARN `vault-projection.ts:155-161` `{characterId, mountPointId, relativePath, docId, error}`.
- Fix shape: pass `character.id`. (Also note the WARN lacks v4's `docId` — recorded at `vault_wardrobe_write.rs:255-258`.)
- Differential: unit capture (posed delete failure) — `vault_wardrobe_write_equivalence` if it drives the stale-delete arm. Files: `db/vault_character_write.rs`. Size: S.

### C.5 — Prompts / Scenarios vault reads sorted by code unit   [CLOSED]
- `crates/quilltap-core/src/db/vault_read_overlay.rs:400-408` (prompts) uses `crate::collation::locale_compare`; scenarios go through `parse_scenario_files_sorted` → `scenarios::sort::sort_scenarios` (`:218-225, 419-423`) — closed by the scenarios round (P4.D295).

### C.6 — P4.D279 T3 #19 (`store-file.ts` reservation, `legacy-wardrobe-rows.ts`, stub tidy) / #20   [NO-ACTION] — NO-PORT / "not in v4".

### C.7 — P4.D280 T3 #13 / P4.D262 OPEN: `maybe_queue_wardrobe_tool_image` answers `not-enabled` / `no-image-profile` on a READ failure where v4 answers `failed`   [OPEN]
- v5 site: `crates/quilltap-core/src/services/tool_image_generation.rs:186` (`maybe_queue_wardrobe_tool_image`).
- v4: `lib/tools/…/wardrobe-image` queue helper — a thrown read → `failed`.
- Fix shape: distinguish `Err` from `Ok(None)` on the settings / profile reads; `Err` → `failed`. Differential: `wardrobe_tools_equivalence` picture scenario + a failing-read plant. Size: S.

### C.8 — P4.D280 T3 #14 / P4.D282 T3 #12 / P4.D262 OPEN: `set_all` uncredited-claims line logged after the commit (v4 before)   [OPEN]
- v5: the `set_all` arm of `api/chat_outfits.rs` (`put_on_or_fit`, ~1010) / `commit_equipped_outfit`; v4: `lib/wardrobe/commit-equipped-outfit` + `wardrobe-wear.repository.ts` (`log` child, `:59`).
- Fix shape: move the line ahead of the write; pin order in `wardrobe_wear_commit_tier2_equivalence.rs` (already captures per-op lines — assert ORDER). Lane must locate the exact line first (survey did not resolve the message text). Size: S.

### C.9 — P4.D262 OPEN rider: `generate_image: null` accepted as absent   [OPEN]
- v4's Zod `generate_image: z.boolean().optional()` refuses `null`. Fix in the wardrobe create/update tool parsers; add a `wardrobe_tools` op. Size: S.

### C.10 — P4.D281: the General item PUT edge parses the body before the 404   [OPEN]
- v5 site: `crates/quilltap-web/src/wardrobe_routes.rs:357-370` `wardrobe_item_put` — `parse_body(&body, "Internal server error")` before `dispatch_core(WardrobeUpdate)`.
- v4: `lib/wardrobe/routes/wardrobe-route-factory.ts:490-495` — `findItem` (404) BEFORE `updateWardrobeSchema.parse(await req.json())`.
- Fix shape: pass the raw body string to core (as the other tiers do) or dispatch an existence probe first; a bad-JSON PUT on a missing id → 404.
- Differential: `crates/quilltap-web/tests/wardrobe_routes_equivalence.rs` + a `missing_id_bad_body` row. Size: S.

### C.11 — P4.D281: two `[WardrobeTransfers v1]` ERROR lines never logged   [OPEN]
- v5 site: `crates/quilltap-core/src/api/wardrobe.rs` (only `:423` read-back ERROR and `:442` INFO exist).
- v4 site: `app/api/v1/wardrobe/transfers/route.ts:204` ERROR `[WardrobeTransfers v1] Failed to list destinations`, `:424` ERROR `[WardrobeTransfers v1] Failed to transfer item` (with error).
- Fix shape: add both at the GET / POST catch arms with v4's fields; pin via capture.
- Differential: `wardrobe_transfers_tier2_equivalence.rs` (failing plant) or unit capture. Size: S.

### C.12 — P4.D281: `image_choice_error`'s read failure   [OPEN — measure; status likely converged]
- v5 site: `crates/quilltap-core/src/services/wardrobe_item_route_steps.rs:170-187` (`Read` → `Err`), caller `api/wardrobe_route_factory.rs:1413-1420` → `internal_server_error()`.
- v4 site: `lib/wardrobe/item-route-steps.ts:31-46` rethrows; `assertItemImageChoice` (`lib/wardrobe/item-images.ts:158-168`) → `files.findByLinkedTo` is a **rethrow** `safeQuery` (`files.repository.ts:104-112`, no fallback) → ERROR `Error finding files linked to entity {collection, entityId}` → middleware 500 `Internal server error` + `Unhandled route error`.
- Fix shape: the status/body already match; add v4's two ERROR lines (repository + middleware) on that arm. Differential: route family + abort plant on `files`. Size: S.

### C.13 — P4.D281 T3 #11 REST edges for project / group wardrobe   [NO-ACTION] — R-F named deferral (dispatch-only).
### C.14 — P4.D281 T3 #12 (P4.D256 T3 #19 REST item-GET edges; the four PUTs' duplicated `Internal server error` arms)   [OPEN — measure]
- The factory (`api/wardrobe_route_factory.rs`) replaced the four PUTs with ONE, which likely erased the duplicated arms; the REST item-GET edges remain R-F. Lane: grep `crates/quilltap-web/src/wardrobe_routes.rs` for per-tier PUT arms and record CLOSED/OPEN. Size: S (record-only if closed).

### C.15 — P4.D282 T3 #11 `outfit_llm_choose` archived-part filing (R-K)   [NO-ACTION] — the human's v4 filing.

---

## §D — P4.D275 / P4.D277 (save-to-store, viewer)

### D.1 — Save lines: `errorMessage` (two new doors) vs `message =` (gallery door) vs v4 `message`   [OPEN]
- Source: P4.D275 Unification OPEN + T3 #22.
- v5 sites: `crates/quilltap-core/src/api/chat_media.rs:357` (`errorMessage`), `crates/quilltap-core/src/api/wardrobe_item_images.rs:784` (`errorMessage`), `chat_media.rs:569-574` (`message = %err.message` — clobbers the file envelope's `message`). Gallery door keys are snake_case: `chat_media.rs:470, 484, 496, 527-534, 557-564, 569-573, 580` (`chat_id`, `file_id`, `id_kind`, `mount_point_id`, `attribution_role`, `relative_path`, `link_id`).
- v4 site: `app/api/v1/chats/[id]/actions/save-image.ts:63-115` — `{chatId, fileId, galleryTotal}`, `{chatId, fileId, idKind, source, mountPointId, attributionRole}`, `{chatId, fileId, source, mountPointId, relativePath, linkId}`, `{chatId, code, message}`, ERROR `{chatId}`+error.
- Fix shape: the file layer ALREADY supports a context key named `message` via `messageJson = %serde_json::to_string(&err.message)` (`crates/quilltap-web/src/log_file.rs:1134-1136`) — use it at all three doors (no layer change needed); camelCase the gallery door's keys; update #18's v4-side-only key pin to compare v5's rendered envelope.
- Differential: `chat_gallery_equivalence.rs` + the save-to-store rows in `crates/quilltap-web/tests/wardrobe_images_routes_equivalence.rs`; envelope check through `log_file` rendering.
- Files: `api/chat_media.rs`, `api/wardrobe_item_images.rs`. Size: S.

### D.2 — Salon doors' `''` user-name byline converged by reasoning, not a `chat_gallery` row   [OPEN]
- Fix shape: add a `chat_gallery` save row with a nameless user seat. Differential: `chat_gallery_equivalence.rs`. Size: S.

### D.3 — `ENABLED_STORE_ZOD_DROP` + `malformed_store_row` carve rewrites v4's counts by string replace   [NEEDS-RULING → recommend converge]
- Source: P4.D275 Unification OPEN + T3 #23.
- v5 site: `db/doc_mount_points.rs` `find_enabled` keeps a `mountType='bogus'` row as `document-store`; carve `crates/quilltap-harness/tests/photo_album_options_equivalence.rs:20, 189-196`.
- v4: `findEnabled` Zod-drops the row after `Data validation failed` + `Safe validation failed` (status-log ~176629).
- Fix shape: port the drop (validate `mountType` enum per row, v4's two lines) → retire the carve and its string-replace. If ruled leave-alone, replace the string-replace with a structural comparand. Size: S–M.

### D.4 — `save_image_failed` widens the 500 to read failures   [OPEN]
- v5 site: `crates/quilltap-core/src/api/chat_media.rs:370-378` (+ its callers): a READ failure of the message/file answers 500 `Failed to save image`.
- v4: `findById` is a FALLBACK read → `null` → the not-found arm (repository-fallback ledger class).
- Fix shape: route the reads through the fallback homes (`fallback::find_by_id_or_none`); only the save itself 500s. Differential: `chat_gallery_equivalence` / message Save rows with a corrupt-row plant. Size: S.

### D.5 — P4.D275 T3 #20 = #156 (`find_home`'s `mount_conn(ws)?`)   → covered by §A (C1 tier class; `api/wardrobe_item_images.rs:81-88` `mount_conn` answers `PartitionUnavailable` — include in Lane W1's entity gate).
### D.6 — P4.D275 T3 #21 `chat_photo_albums` onto `PhotoAlbumOption`   [NO-ACTION] (YAGNI, v4 unchanged).
### D.7 — P4.D275 T3 #24 `help/wardrobe.md`   [CLOSED] — re-vendored whole by P4.D278/P4.D323/P4.D339 (`host_help_docs_boot.rs:131` → 135 at `322a5488e`).

### D.8 — P4.D277: `salon-image-detail-flow.spec.ts:128` R-B re-pin has no owner (its one beat skips on the shared fixture)   [OPEN]
- v5: `apps/web/e2e/salon-image-detail-flow.spec.ts:127-131` (re-pin present). The beat never runs.
- Fix shape: seed a gallery fixture the beat can reach (or point it at the salon pair + row copy — memory `own-instance-e2e-fixture-choice`) so the locator is exercised. Size: S–M. (P4.D286 T3 #13 is the same item.)

### D.9 — P4.D277: save dialog's document-Escape listener does not stop propagation   [NO-ACTION] — v4 `BaseModal` closes on Escape too (convergence, `save-image-dialog.ts:86-97`); T3 #21 same class.
### D.10 — P4.D277 Tier 2 #14 (chat Save beat presses no Escape)   [NO-ACTION] — dogfood row.
### D.11 — P4.D277 T3 #17: the album-load failure line `Failed to load photo albums (${res.status})`   [OPEN]
- v5: `apps/web/src/app/images/save-image-dialog.ts:83` (comment: no counterpart). v4: `components/…/SaveImageDialog.tsx:134`.
- Fix shape: needs an HTTP status in the dispatch error envelope (memory `spa-component-harness-gotchas`: dispatch errors carry no status) — either surface `status` on the client error, or render the sentence without the status (a recorded divergence). Size: M if the envelope grows; S otherwise. Possibly NEEDS-RULING on the status half.
### D.12 — P4.D277 T3 #18, #19, #20   [NO-ACTION] (convention stands; module-private; dogfood row).

---

## §E — P4.D285 / P4.D286 (SPA)

### E.1 — Prospero card mounts the wardrobe browser while collapsed (+1 folders read per project page)   [OPEN]
- v5 site: `apps/web/src/app/screens/prospero/cards/project-wardrobe-card.ts:35-50` — `<qt-wardrobe-browser>` projected into `<qt-collapsible-card>` unconditionally (content projection instantiates it).
- v4: the card's children mount only when open.
- Fix shape: track the card's open state (`(openChange)` / `defaultOpen`) and wrap the browser + instructions section in `@if (open())`. Vitest: assert no folders read until opened.
- Files: that card (+ spec). Size: S.

### E.2 — P4.D285: bug 229's click arm cannot fail in v5   [NO-ACTION] — parity only.
### E.3 — P4.D285 Tier 2 #8 (`mergeWearableTiers` if a spec needs it; `avatar-generation-pane.ts` ownership move)   [NO-ACTION] — not needed; recorded.
### E.4 — P4.D285 T3 #10 = P4.D261 T3 #23 (avatar-picker `orientation:'portrait'` + Shape note), #24 (`keepPhotograph`), #25 (new-chat `wornBundleIds` + `buildDefaultOutfitWithCredit` seeding — `apps/web/src/app/screens/new-chat/outfit-selector.ts` has neither), #26; the checkbox-stays-ticked pattern   [NO-ACTION] — each waits on an unported leg (ImageUploadDialog / analyze-image / Compose-manual). #25 is portable today if the planner wants it: S–M SPA (v4 `outfit-selector.tsx`).
### E.5 — P4.D285 T3 #11   [NO-ACTION].
### E.6 — P4.D286 T3 #12 / #13   [NO-ACTION] / = D.8.

### E.7 — jsdom `scrollIntoView` stub for the six pre-existing vitest unhandled errors   [OPEN]
- Source: e8e17b2d9 unification record (status-log ~190676-190678).
- v5 site: `apps/web/src/app/ui/collapsible-card.ts:108-112` calls `this.host.nativeElement.scrollIntoView(…)` in a `requestAnimationFrame`; jsdom lacks it → six unhandled errors under `apps/web/src/app/screens/settings/memory/memory-tab.spec.ts`.
- Fix shape: either guard as siblings do (`typeof el.scrollIntoView === 'function'`, cf. `workspace/chrome/tab-strip.ts:97`, `scenario/scenario-browser-dialog.ts:451`) or stub in `memory-tab.spec.ts` (`chat-tab.spec.ts:59` precedent: `if (proto && !proto.scrollIntoView) proto.scrollIntoView = () => undefined`). Recommend the production guard (one line, matches two siblings, kills the class for every future spec). No global setup file exists (`angular.json` test builder has none).
- Differential: unit-only (`npm test` reports 0 unhandled errors). Size: S.

---

## §F — Outfit history (P4.D315 / P4.D316 / P4.D318)

### F.1 — Non-numeric `entryIndex` settle consequence (recorded, undriven)   [NEEDS-RULING]
- v5 site: `crates/quilltap-core/src/db/chat_wardrobe_history.rs:405-440` `normalize_row` (`entry_index: number_cell(..).unwrap_or(0.0) as i64` → `"abc"` reads 0); `settle` `:544`. Carve `NAN_ENTRY_INDEX_READS_ZERO` in `crates/quilltap-harness/tests/chat_wardrobe_history_tier2_equivalence.rs:50-62, 388, 427`.
- v4: `lib/database/repositories/chat-wardrobe-history.repository.ts` `normalizeRow` `Number(…)` → NaN; next settle computes `NaN + 1` → INSERT fails → `skipped`. v5 appends entry 1.
- Options: (A) drive the row (a tier-2 settle over the planted `"abc"` timeline) and converge — represent the read as `Option<i64>`/NaN-aware and make `settle` answer v4's `Skipped{reason}` for a non-finite max index (+ v4's WARN); (B) drive it and rule the divergence (reach = hand-edited archive only; v4's behaviour is itself a bug — a v4 filing candidate).
- Recommendation: (A) drive first (cheap), then (B) ruled + file v4 — wedging a timeline forever is worse than v5's append. Size: S (drive) / S–M (converge).
- Differential: `chat_wardrobe_history_tier2_equivalence.rs` + its oracle case.

### F.2 — P4.D315: the seed's BLOB cell unplanted   [OPEN]
- Fix: plant a BLOB `equippedOutfit` cell in the ensure family's corpus. Differential: `chat_wardrobe_history_ensure_equivalence.rs`. Size: S.

### F.3 — P4.D315 T3 #24 (seed non-fatal, R-2), #25 (NULL-`updatedAt` seed, R-3), #26 (one-row arm drops actor/tool, R-7 — v4 filing candidate), #27 (`Raw query failed` NO-PORT), #28 (child-proxy NO-PORT), #29 (unreachable throws), #30 (dependents)   [NO-ACTION] — all recorded classes; #26 is the human's filing.

### F.4 — P4.D316 Tier 2 #30: `history_dresser` definition in `build-pascal-run-custom-fixture.ts`   [OPEN]
- v5: absent (`grep -rn history_dresser crates harness` → 0). Builder: `harness/oracle/fixtures/build-pascal-run-custom-fixture.ts`; pair `pascal-run-custom.json`.
- Fix shape: add ONE definition (a `wardrobe` effect dressing the rolling character) by id, then case rows in the three entrances (`pascal-run-custom-handler.test.ts`, `pascal-custom-tools-route.test.ts`, `pascal-trigger-runner.test.ts`) asserting the outside-a-turn `pascal` + `actorToolName` settle row in `chat_wardrobe_history`. ⚠ rebuilding the shared pair re-mints ids and touches 7 harness families (`pascal_run_custom_handler`, `_edit_custom_tool`, `_definition_reader`, `_run_custom`, `_trigger_runner`, `_custom_tools_route`, `_build_tools_roster`) — prefer an `extend-pascal-run-custom-*.ts` extender (precedent: `extend-pascal-run-custom-triggers.ts`).
- Differential: those three families. Size: M.

### F.5 — P4.D316 T3 #31 (Brahma never flushes), #32 (`Failed to settle` unreachable in v4), #33 (NO-PORT), #36 (HANDOFFs, landed), #37 (Google builder needs nothing)   [NO-ACTION]

### F.6 — P4.D316 T3 #34: v4's `buildToolsForProvider called` DEBUG never ported   [OPEN]
- v5: `crates/quilltap-core/src/tools/tool_build.rs` has no `tracing::debug`. v4: `lib/tools/plugin-tool-builder.ts:381-382` (options incl. the two container keys).
- Fix shape: add the DEBUG with v4's options keys; capture in `pascal_build_tools_roster.rs` / `tools_inventory`. Size: S.

### F.7 — P4.D316 T3 #35: malformed-JSON ERROR on the rollback REST edge   [OPEN]
- v4 logs `[Chats v1] Error rolling back wardrobe` on a bad body; v5's edge answers the 500 without it (the `equip` arm's shape too). Fix: log at the edge (or move parse into core). Differential: `wardrobe_routes_equivalence` (web) rollback rows. Size: S.

### F.8 — P4.D316: missing-chat + invalid-body gate order (v4 404 / v5 400 — the standing `chat_item_post` class)   [OPEN]
- Fix shape: chat existence check before body validation on the rollback POST (and the class's other members — name them in the lane). Size: S–M (class-wide).

### F.9 — P4.D318 beat A: unwritten Aurora-announcement assertion   [OPEN]
- v5: `apps/web/e2e/wardrobe-flow.spec.ts:1360-1421` — comment at `:1418-1420` says the announcement is "the unifier's to MEASURE"; no assertion.
- Fix shape: after Done, wait on the Salon transcript for Aurora's `outfit-change` whisper (debounced job — needs the job runner pumping in the e2e server and a bounded wait; assert the brogues summary text). 💸 none (no LLM). Size: S–M.

### F.10 — P4.D318 R-L v4 filing; T3 #19–#25   [NO-ACTION] — human's filing; recorded divergences / v4-faithful.

---

## §G — Gallery (P4.D321) and riders (P4.D322)

### G.1 — P4.D321 Tier 2 #22: two-chunk (`>500` ids) `repo` row   [OPEN]
- v5: `crates/quilltap-core/src/db/files.rs:538` `find_by_linked_to_any` (chunked; v5-only unit test). Family: `crates/quilltap-web/tests/wardrobe_images_routes_equivalence.rs` + `harness/oracle/cases/wardrobe-images-routes.test.ts` (+ `fixtures/wardrobe-images-routes.json`, has `repo_duplicate_link`).
- Fix shape: plant 501 pool ids via a recursive-CTE `INSERT … SELECT` in `mountSql` on both sides; one row comparing the response. Only if the builder re-mint doesn't perturb it (else stays T3 #25). Size: S–M.

### G.2 — P4.D321 Tier 2 #23: e2e group plate (`Group · <name>` caption)   [OPEN]
- v5: `apps/web/e2e/character-wardrobe-pictures-flow.spec.ts` (no `Group ·` assertion). Fix: create a group with Aria + a group wardrobe item with a picture via dispatch in-spec, assert the third plate. Size: S–M.

### G.3 — P4.D321: `isPending` vs v4 `isLoading` on a paused query   [OPEN — cosmetic]  Size: S (one-word swap in the section).
### G.4 — P4.D321 "generation prompt" help filing; T3 #25–#30   [NO-ACTION] (human's filing; recorded).

### G.5 — P4.D322: stale "ONLY consumers" sentence in the older archive extender   [OPEN]
- v5 site: `harness/oracle/fixtures/extend-character-archive-profile-and-avatars.ts:52-54` claims the pair's ONLY consumers are `character_archive_tier2_equivalence` + its case. Truth (per `extend-character-archive-inventory.ts:47-51`): THREE — `character_archive_tier2_equivalence`, `archive_reencrypt_tier2_equivalence` (+ `archive-reencrypt-tier2.test.ts`), `crates/quilltap-web/tests/change_passphrase_archive_sweep.rs`.
- Fix: rewrite the sentence. Docs-only. Size: S.

### G.6 — stale `host_help_docs_boot` assert text   [CLOSED]
- `crates/quilltap-host/tests/host_help_docs_boot.rs:127-131` now `assert_eq!(expected, 135, "the vendored tree at v4 322a5488e")` (P4.D339, `ea17982cd`).

### G.7 — P4.D322 T3 #15 / #16   [CLOSED/NO-ACTION] — conditional deferrals that did not trigger (both riders landed).
### G.8 — P4.D322 T3 #17 (bug 253), #18 (rest of `4fbdad186`), #19 (doc rewording)   [CLOSED] — absorbed by the `322a5488e` round (P4.D324: `inventory/prompt_section.rs:3`, `build_context.rs:2960-2968`; `chats_read.rs:107` `equippedInventory`; help re-vendored at 135).
### G.9 — P4.D322 T3 #20 (`context-management.test.ts` `it` not transcribed)   [NO-ACTION].

---

## §H — Proposed lanes (for the planner)

- **W1 (#156 keystone, L):** §A 156 — neutral `PartitionUnavailable` in `with_both_conns` + the entity gate + the 72 C1 swaps + `wardrobe_item_images.rs:81` `mount_conn`; degraded-clone rows per family.
- **W2 (degraded wardrobe reads + announcement job, M):** 156.a, 156.b, 156.c, B.2 (shared fail-soft pool helper), B.1 residual.
- **W3 (server smalls, M):** C.1, C.2, C.3 (after ruling), C.4, C.7, C.8, C.9, C.10, C.11, C.12, C.14, B.3, B.5, B.6, B.7, B.8, D.1, D.2, D.3, D.4, F.1, F.2, F.6, F.7, F.8, #157, 156.d — split by file ownership (e.g. W3a tools/executor + wardrobe tools; W3b routes/web edges; W3c logging-key smalls).
- **W4 (SPA/e2e + harness fixtures, M):** E.1, E.7, D.8, D.11, F.9, G.2, G.3, F.4, G.1, G.5.
- **Rulings owed before W3 starts:** C.3 (`fields` order — recommend converge), D.3 (`ENABLED_STORE_ZOD_DROP` — recommend converge), F.1 (NaN `entryIndex` — recommend drive, then rule + file v4); D.11's status half if the envelope must grow.


## Appendix — the 88 `with_both_conns(` call sites (file:line + enclosing fn, main @ `4e43b714e`)

```
groups.rs:302 group_create
groups.rs:337 group_create
groups.rs:452 group_update
groups.rs:480 group_delete
groups.rs:508 group_member_add
groups.rs:539 group_member_remove
groups.rs:591 group_mount_point_link
groups.rs:618 group_mount_point_unlink
groups.rs:795 group_scenarios_union
groups.rs:915 group_state_set
groups.rs:945 group_state_reset
wardrobe_route_factory.rs:732 list_store_items
wardrobe_route_factory.rs:822 instructions_get_store
wardrobe_route_factory.rs:851 instructions_set
wardrobe_route_factory.rs:979 folder_write
wardrobe_route_factory.rs:1251 create
wardrobe_route_factory.rs:1405 item_update
wardrobe_route_factory.rs:1508 item_delete
wardrobe_route_factory.rs:1536 item_delete
wardrobe.rs:396 wardrobe_transfer_apply_with_codec
wardrobe.rs:702 run_wardrobe_preview_avatar
wardrobe.rs:775 run_wardrobe_preview_avatar
wardrobe.rs:983 run_wardrobe_preview_avatar
chat_outfits.rs:1010 put_on_or_fit
chat_outfits.rs:1198 chat_wardrobe_rollback
characters.rs:315 character_depiction_guidelines_update
characters.rs:924 character_favorite
characters.rs:950 character_toggle_controlled_by
characters.rs:974 character_toggle_carina
characters.rs:1014 character_set_default_partner
characters.rs:1067 character_avatar
characters.rs:1133 character_add_tag
characters.rs:1182 character_remove_tag
characters.rs:1292 character_create
characters.rs:1347 character_quick_create
characters.rs:1423 character_import
characters.rs:1496 character_photo_remove
characters.rs:1691 character_photo_save_by_id
characters.rs:2153 character_delete
characters.rs:2278 character_update
characters.rs:2412 character_prompt_create
characters.rs:2440 character_prompt_update
characters.rs:2475 character_prompt_delete
characters.rs:2503 character_prompt_set_default
characters.rs:2539 character_scenario_create
characters.rs:2572 character_scenario_update
characters.rs:2604 character_scenario_delete
characters.rs:2632 character_plugin_data_upsert
characters.rs:2664 character_plugin_data_delete
scenarios.rs:424 run
scenarios.rs:776 scenario_everywhere
scenarios.rs:818 scenario_read_by_ref
inventory.rs:91 inventory_transfer_apply_with_codec
projects.rs:264 project_create
projects.rs:302 project_create
projects.rs:572 project_update
projects.rs:633 project_delete
projects.rs:745 project_character_add
projects.rs:799 project_character_remove
projects.rs:961 project_chat_add
projects.rs:999 project_chat_remove
projects.rs:1054 project_state_set
projects.rs:1079 project_state_reset
projects.rs:1115 project_tool_settings_update
projects.rs:1296 project_aesthetic_set
projects.rs:1363 project_mount_point_link
projects.rs:1402 project_mount_point_unlink
projects.rs:1816 project_file_add
projects.rs:1859 project_file_remove
inventory_route_factory.rs:613 collection_write
inventory_route_factory.rs:935 duplicate
inventory_route_factory.rs:1133 item_update
inventory_route_factory.rs:1198 item_clear_override
inventory_route_factory.rs:1243 item_detach
inventory_route_factory.rs:1280 item_delete
headshoulders_backfill_job.rs:181 handle_headshoulders_backfill
headshoulders_backfill_job.rs:304 handle_headshoulders_backfill
story_background_job.rs:294 handle_story_background_generation
story_background_job.rs:457 handle_story_background_generation
story_background_job.rs:890 handle_story_background_generation
story_background_job.rs:1107 handle_story_background_generation
story_background_job.rs:1131 handle_story_background_generation
appearance_resolution.rs:353 with_carrying
character_avatar_job.rs:215 handle_character_avatar_generation
character_avatar_job.rs:279 handle_character_avatar_generation
character_avatar_job.rs:381 handle_character_avatar_generation
character_avatar_job.rs:785 handle_character_avatar_generation
character_avatar_job.rs:923 bind_avatar_to_chat
```
