# SURVEY HARNESS — what the Concierge overhaul moves in the differential machinery — fresh survey 2026-09-25 on v5 main `2aed9a552`

Scope: v4 `8bd080267` (#73) → `49059fb14` (#74) → `4d370a90f` (#75) → `3b463d6b1` (#76) → `ce2f1dabf` (#77),
against the oracle baseline `b0b6656b5`. v4 was read ONLY via `git -C ~/source/quilltap-server show/grep/diff <sha>`.
Everything below was MEASURED on the tree unless marked *(predicted)*.

Method notes:
- The committed fixture DBs are encrypted (sqleet/ChaCha20, test peppers). They were read-only probed with a
  throw-away scratch binary (`scratchpad/fxprobe/`, rusqlite over `quilltap-sqlite3mc-sys`, each file tried
  against every `testPepperBase64` in `harness/oracle/fixtures/*.json` plus the `web-fixture` literal). Raw table in
  `scratchpad/fxprobe.txt`.
  - ⚠ The read-only open left `-wal`/`-shm` files beside the three `migration-vintage/*.db`, because those files are
    WAL-mode. They were deleted at once; `git status` is clean. **Any order that probes that trio should copy it
    first.**
- The vocabulary grep outputs are in `scratchpad/grep1.txt` (Rust/oracle/e2e/tools) and `scratchpad/spa.txt` (SPA src).

---

## A. The machinery map (so the orders can point at it)

### A.1 Test families (the Rust side)

| Where | Count | What it is |
|---|---|---|
| `crates/quilltap-harness/tests/*.rs` | **529** `.rs` (+ `common/`, `source_census/` dirs) | The differential families: tier-1/2/3 `*_equivalence`, the censuses, the guards, the heal families. **The family name is the file stem.** |
| `crates/quilltap-web/tests/*.rs` | **66** | Route families plus envelope/wire arms. These include `dispatch_wrong_type_census`, `web_edge_action_sites_census`, `query_param_semantics_equivalence` and `tri_state_edges_share_the_decoder`. `common/mod.rs` holds `TEST_PEPPER` (`dGVzdHBlcHBl…MDE=`), the hand-rolled 20-column `LLM_LOGS_DDL` and the `turnSkippingEnabled` ADD-COLUMN heals. `source_census/mod.rs` is the shared lexer and `request_body` walker. |
| `crates/quilltap-host/tests/*.rs` | **16** | Host boot and cadence. **`host_boot.rs`, `host_cadence.rs` and `host_llm_log_cleanup.rs` hand-write `chats` / `chat_settings` DDL containing `conciergeOverride TEXT`, `isDangerousChat INTEGER` and `dangerousContentSettings TEXT`.** `host_boot_p4d182_columns.rs` is the per-entrance boot-ensure precedent. |
| `crates/quilltap-cli/tests/*.rs` | 3 | Tier R (266/0). No Concierge vocabulary. |
| `crates/quilltap-core/src/test_support.rs` | — | `ensure_p4d171_columns` (:142) and `ensure_p4d182_columns` (:162) are the per-COPY heal idiom. 24 test files call them. |

### A.2 Oracle cases, fixtures, recipes

| Where | Count | Notes |
|---|---|---|
| `harness/oracle/cases/*.test.ts` (jest) | **217** | Real-DB and route oracles. They run through v4's jest (the P4.D32 venue). |
| `harness/oracle/cases/*.ts` (tsx, non-jest) | **278** | Tier-1 pure-function recorders. |
| `harness/oracle/cases/*.mjs` | 5 | Includes **`concierge-presentation.mjs`**, which is NOT in any grep of this survey's term list. It executes v4's `concierge-state-presentation.ts` via `git show` at a pin and writes `apps/web/src/app/chat/concierge-state-presentation.v4.json`. That is the four-state table, so #75 replaces it whole. |
| `harness/oracle/fixtures/` | 523 entries | 273 `*.json` specs, 217 `build-*.ts` builders, and the migrators (A.3). |
| `harness/oracle/lib/` | — | `tier2.ts` builds per-case DBs through v4's REAL repositories, so their schema is the pin's `generateDDL`. `p4d171-columns.ts` is the jest-side vintage heal (raw `ADD COLUMN` on the copy). |
| `harness/oracle/provision/` | — | `dump-fresh-schema.ts` is **the D23 re-dump**. It writes `QT_SCHEMA_OUT` and drives v4's REAL repos, so it emits the generateDDL surface. Also `build-provision-oracle.ts` and `verify-v5-provisioned.ts`. |
| `harness/oracle/providers/` | — | The provider-corpus recorders and `regenerate-*.sh`, which load `plugins/dist/*` bundles. #73 regenerated 24 files under v4 `plugins/dist` (+31,786/−28,320). |
| Recipes | — | **No separate recipe file.** Each family's oracle-regen recipe lives in its own `//!` header. The consuming oracle case's `/** … */` header is the fallback (`ok_restored`). |
| `harness/tools/recipe_sweep.py` | — | The sweep driver: `--list/--show/--run/--run-all/--collisions/--self-test`, with `--v4 <pinned worktree>`. `--run` deletes the family's oracle outputs first and runs with `CARGO_INCREMENTAL=0`. **Current `--list` totals: ok 485, ok_restored 80, no_oracle 11, committed_corpus 14, exempt 6, non_extractable 2** (`avatar_rolls_routes`, `generator_sse_wire`). Results are committed under `harness/tools/sweep-results/` (41 files). |

### A.3 Committed fixture DBs and the widening tool

- **109 committed `.db` files.** 106 are in `crates/quilltap-web/tests/fixtures/`, as main/mount/llmlogs pairs and
  trios. The other 3 are in `…/fixtures/migration-vintage/` (`quilltap.db`, `quilltap-mount-index.db`,
  `quilltap-llm-logs.db`). About 22 `.db.meta.json` sidecars sit beside them.
- **The folder also holds `restore-archives/*.zip`**: ten backup archives, built by `build-restore-archive-*.ts`. For
  example, `build-restore-archive-bag-keys.ts` writes `conciergeOverride` into a chat row.
- **The widening tool** is `harness/oracle/fixtures/migrate-memories-fixture-columns.ts`, the generic "fixture-vintage
  migrator", 588 lines. There are older single-purpose siblings: `migrate-fixtures-{episodic,link-group,pascal}-columns.ts`,
  `migrate-pascal-run-custom-columns.ts` and `migrate-system-data-schema.ts`.
  - **What it does:** it applies v4's own migration `ALTER … ADD COLUMN` statements verbatim, in migration order.
    Each one is guarded on column presence, so the run is idempotent. It can also run `extraSql` for v4's index
    statements.
  - **Flags and peppers:** it has a `--report-only` read-only dry run. It tries five peppers: `web-fixture`, plus four
    read from JSON specs.
  - **How to run it:** from the v4 checkout or a pinned worktree, under Node 24.
- ⚠ **The migrator CANNOT express the overhaul as it stands:**
  - It has **no DROP-COLUMN row.** Its idempotence guard is "column missing → ADD", so a drop needs an inverted guard.
  - It has **no backfill.** It has one precedent, `allowTierFallback`: that row records v4's `UPDATE` as "can never
    match" and does not transcribe it.
  - #75's and #76's backfills are **JavaScript loops**:
    - #75 iterates the chat rows and maps `(conciergeOverride, isDangerousChat)` → `(conciergeMode, SetBy, Reason)`.
    - #76's `mapLegacyConciergeSettings` builds `conciergeSettings` from `dangerousContentSettings`,
      `uncensoredImageDescriptionProfileId` and `cheapLLMSettings.imagePromptProfileId`.
  - They are not SQL an `ALTER` row can carry. The faithful widen **imports and runs v4's migration modules'
    `run()`**, or `mapLegacyConciergeSettings` directly, against each fixture: see F.1.
- **The migration-vintage trio** is built by `build-migration-vintage-fixture.ts`, which runs v4's REAL migration
  runner. So a rebuild at the target pin carries all four migrations, including the DROP, automatically. This is
  memory `migration-vintage-fixture-goes-stale`, "one command". Its consumers are `chat_activity_heal_equivalence`,
  `files_sha256_realign_heal_equivalence`, and core unit tests in `db/{chat_settings,llm_logs,chat_activity_recompute_heal}.rs`,
  `api/mount_points.rs`, `services/{builtin_mounts,delete_all}.rs` and `backup/restore/orchestrator.rs`.

### A.4 Fresh provisioning and the D23 re-dump

- `crates/quilltap-core/src/services/provisioning/fresh_schema.json` is v4's generateDDL capture, per partition.
  `mod.rs:67-75` holds the re-dump register (append-only). `chat_settings_seed.json` holds v4's seed row verbatim.
- **`provisioning_equivalence`** (harness) is the DDL proof. v5's `sqlite_master` must equal v4's **LIVE**
  generateDDL, byte-exact per partition. It also checks the seed rows, and runs cross-compat v5→v4 and v4→v5 legs
  through `verify-v5-provisioned.ts`. At the target pin it goes RED by design (D23 tripwire): see D.
- **Boot ensures** re-home v4 migrations, because v5 has no migration runner (a locked deferral). They are
  `crates/quilltap-core/src/db/*_repair.rs`, 13 files, and run from `host.rs::seed_built_ins` on setup, unlock and
  boot. The precedent for schema-absent columns is `chats_transcript_version_repair.rs` (P4.D182). Its test is
  `crates/quilltap-host/tests/host_boot_p4d182_columns.rs`, with one arm per entrance.

### A.5 SPA oracle recorders and Playwright

- **SPA recorders live OUTSIDE `src/`**, in `apps/web/oracle/` (12 files). The ones this overhaul touches:
  - `route-trail-display.ts` (#73). It records v4's `lib/chat/route-trail-display.ts` plus the three enums, which
    must now include evidence and `profileKind`. It writes `src/testing/fixtures/route-trail-display.oracle.ndjson`
    ("Expect 25 lines"), consumed by `src/app/chat/route-trail-display.oracle.spec.ts`.
  - `help-guide-capture.test.tsx` (#76). It writes `src/app/help/__fixtures__/{help-guide-tables,label-from-url-vectors}.json`,
    both of which carry `section=dangerous-content` and the slug.
  - Also moved: `harness/oracle/cases/concierge-presentation.mjs` → `src/app/chat/concierge-state-presentation.v4.json`.
- **Playwright:** `apps/web/e2e/` holds 106 entries, with `global-setup.ts` and `support/{env,fixtures,dbkey,seed-*}.ts`.
  - `global-setup.ts` **copies the committed `chat-send-main.db` fixture**, locks it with a passphrase, and launches
    the REAL axum server. That fixture carries `conciergeOverride='OFF'`, one `isDangerousChat=1` row and a
    `dangerousContentSettings.mode='AUTO_ROUTE'` settings row, so it **is a pre-#74 vintage instance**.
  - Other specs copy `characters-*`, `salon-*`, `groups-projects-*`, `pascal-run-custom-*`, `memories-*`,
    `courier-images-*` and `salon-long-*`.
  - Specs plant raw SQL through the CLI: `character-avatar-rolls-flow.spec.ts:173` guards an `ADD COLUMN`, and
    `salon-concierge-four-state-flow.spec.ts:292` / `salon-danger-avatar-flow.spec.ts:208` read
    `SELECT "conciergeOverride","isDangerousChat" FROM chats` directly.

---

## B. Every family that carries the legacy Concierge vocabulary

Key:
- **Commit:** 73 = trails/refusal; 74 = ledger; 75 = three states; 76 = settings, drop and help; 77 = retry actions.
- **Effect:** **RR** = re-record at the target; **WF** = widen the committed fixture (or the per-copy heal); **SPEC**
  = rewrite the JSON spec's retired keys; **RETIRE** = retire the pin or case, because v4 deleted the export;
  **ARM** = add a new arm; **DDL** = a hand-rolled DDL mirror to update.
- **Recipe:** always the Rust family stem. Oracle cases and fixtures list their consuming family.
- `evidence` hits in `character-optimizer-prompts.json` and the `2026-08-21-12fe3e6f-p4.54-run-lines.json` sweep
  result are **false positives**, and so is every `route_trail` string in the `sweep-results/*.json` artifacts, which
  are historic results and not moved.

### B.1 Harness families (`crates/quilltap-harness/tests/`)

| File (= recipe) | Kind | Terms carried | Committed fixture(s) / spec | Commit(s) | Predicted effect |
|---|---|---|---|---|---|
| `danger_resolver_equivalence` | harness | AUTO_ROUTE, conciergeState, dangerous_content_settings, isDangerousChat (18) | spec `danger-manual-flip.json` (+ tsx case `danger-resolver.ts`) | 75, 76 | **RETIRE+RR.** The tsx case named-imports `resolveDangerousContentSettings`, which is GONE at target; `getConciergeState` survives with new semantics. Rewrite round `resolveConciergeSettings` / `mayFailOver` / `withConciergeModeFromLegacy`. Re-record the manual-flip spec (32 legacy hits). |
| `danger_routing_equivalence` | harness | uncensoredImage/TextProfileId, `isImageModerationError`, `resolveUncensoredImageProfileForReroute` | spec `danger-routing.json` (24) | 73 | **RETIRE.** Both imported functions are DELETED at `8bd080267`. The successor family is `understudy.ts` (`resolveUncensoredText/ImageUnderstudy`) plus `image-failover.ts`, as new tier-1/tier-2 arms. |
| `danger_trigger_equivalence` | harness | conciergeOverride, isDangerousChat | `chat-scenario-main.db`, `chat-send-main.db` + jest `danger-trigger.test.ts` (AUTO_ROUTE, dangerousContentSettings) | 75, 76 | WF (chat-send is the most legacy-populated pair) + RR. The old/new state pair is re-read through `conciergeMode`. |
| `danger_gatekeeper_tier3_equivalence` | harness | finish_reason, provider_routing | spec `danger-gatekeeper.json` (AUTO_ROUTE, conciergeOverride, dangerousContentSettings, isDangerousChat) | 75, 76 | SPEC+RR. The classifier switch (`classifier-switch.ts`) and the post-commit switch are new; add an ARM for a Locked chat. |
| `danger_scan_tier2_equivalence` | harness | (via builder) | builder `build-danger-scan-fixture.ts` (conciergeOverride, dangerousContentSettings, isDangerousChat) | 75, 76 | SPEC (builder) + RR. `danger_scan.rs` reads the settings through the new resolver. |
| `salon_reads_equivalence` | harness | conciergeOverride, conciergeState, concierge_state, isDangerousChat, routeTrail, `"evidence"` (21) | `salon-main.db`, spec `salon.json`, jest `salon-reads.test.ts` (18, plus unguarded-looking `ADD COLUMN` plants at :171/176/188) | 73, 74, 75 | **RR + WF.** The chat GET drops `conciergeOverride` and adds `conciergeState/SetBy/Reason/RefusalCount`. The trail's `evidence` widens. Check the case's `ADD COLUMN` plants before widening `salon-*` (P4.111 trap). |
| `salon_mutations_equivalence` | harness | conciergeState, routeTrail | `salon-main.db`, jest `salon-mutations.test.ts` (11) | 75 | RR. The PUT/POST `conciergeState` enum becomes three values, so the old values answer 400: a new ARM, and the P4.D141 refusal arms re-recorded. |
| `salon_swipe_generate_equivalence` | harness | routeTrail, finish_reason | `salon-main.db` | 73, 77 | RR. Add an ARM for #77's `retry-uncensored`, which reuses `regenerate-swipe.service` with `profileOverride` + `routeTrail`. |
| `salon_fixture_p4d171_ensure` | harness | routeTrail | `salon-main.db` via `migrate-memories-fixture-columns.ts` | 74, 75 | WF. The ensure family is the model for a new `ensure_p4dNNN_columns`. |
| `transcript_route_equivalence` | harness | route_trail | `salon-main.db`, jest plants `transcriptVersion` (:267) | 73 | RR once the trail widens. Check the plant guard before widening. |
| `route_trail_compose_equivalence` | harness | `"evidence"`, route_trail, moderation-refusal | tsx `route-trail-compose.ts` (content_filter, finish_reason) | 73 | **RR + ARM.** `composeRouteTrail` gains evidence 2 → 5, `profileKind?`, and TOOL/Lantern trails. `RouteAttemptEvidence` in v5 (`services/route_trail.rs:103`) is a CLOSED two-variant enum. |
| `route_trail_continuation_guard` | harness guard | `"evidence"`, routeTrail, moderation-refusal (15) | (in-test DB, `PEPPER` web-fixture) | 73 | Re-assert with the new evidence values. The guard stays: the continuation still drops trails. |
| `message_finalizer_tier3_equivalence` | harness | `"evidence"`, is_dangerous_chat, routeTrail, moderation-refusal | spec `message-finalizer-tier3.json` (conciergeOverride, isDangerousChat, evidence) + builder `build-message-finalizer-fixture.ts` | 73, 74, 75 | SPEC+RR. The finalizer records the refusal ledger (#74, stated evidence only); ARM for `inferred` not counted. |
| `fallback_engine_equivalence` | harness | moderation-refusal, finish_reason | tsx `fallback-engine.ts` | 73 | RR + ARM. The `moderation-refusal` trigger now goes via `classifyRefusal`. |
| `finish_reason_equivalence` | harness | finish_reason | tsx `finish-reason.ts` | 73 | ARM. `extract-finish-reason.ts` now also reads camelCase `finishReason` and Google `promptFeedback.blockReason`. |
| `moderation_finish_reason_equivalence` | harness | finish_reason, moderation_refusal | tsx `moderation-finish-reason.ts` (imports `provider-failover.service`) | 73 | RR. `attemptUncensoredRetry` is new in `provider-failover.service`. |
| `image_dialects_equivalence` | harness | is_image_moderation_error, provider_routing | corpus `image-dialects` (providers recorder) | 73 | **REDESIGN.** v5's dialect layer keys on the keyword verdict v4 deleted (`image_dialects.rs:35`). #73's `ModerationRejectionError` typed signal means the corpus is re-recorded against the new plugin bundles and the verdict arms retire. |
| `image_generation_tier3_equivalence` | harness | dangerousContentSettings, bug 133, provider_routing | spec `image-generation.json` (AUTO_ROUTE, uncensoredImageProfileId) + builder `build-image-generation-fixture.ts` | 73, 76 | SPEC+RR. The generate path now goes through `generateImageWithConciergeFailover`. |
| `image_generate_route_equivalence` | harness | provider_routing | spec `image-generation.json` | 73, 76 | SPEC+RR. |
| `images_generate_route_equivalence` | harness | AUTO_ROUTE, dangerousContentSettings, uncensoredImageProfileId | `images-main.db` (pepper `images-collection`), spec `images-collection.json`, jest `images-generate-route.test.ts` (18) | 73, 76 | WF + SPEC + RR. The collection route's generate now takes a CONNECTION-profile understudy. |
| `story_background_job_tier3_equivalence` | harness | AUTO_ROUTE, dangerousContentSettings, dangerous_content_settings, provider_routing | spec `story-background-job.json` (35) + builder | 73, 76, 77 | SPEC+RR. The job now COMPLETES with a refusal bubble (`background-refused`), and `forceUncensored` is new. |
| `avatar_job_tier3_equivalence` | harness | provider_routing | spec `avatar-job.json` (AUTO_ROUTE, dangerousContentSettings, uncensoredImageProfileId) | 73, 76 | SPEC+RR. The avatar handler is rewritten round the chokepoint. |
| `appearance_sanitize_gate_tier3_equivalence` | harness | AUTO_ROUTE, isDangerousChat, is_dangerous_chat, bug 133 | spec `appearance-sanitize-gate.json` (39) | 75, 76 | SPEC+RR. |
| `orchestrator_tier3_equivalence` | harness | AUTO_ROUTE, conciergeOverride, dangerousContentSettings, routeTrail, uncensoredTextProfileId, provider_routing | spec `orchestrator-tier3.json` (also read by `brahma_orchestrator_tier3` / `help_chat_orchestrator_tier3`), `orch-main.db` (built) + builder `build-orchestrator-fixture.ts` | 73, 75, 76, 77 | SPEC+RR. #77 drops the synthesized `dangerFlags` for Unmoderated chats (`routedDirect`), the one independently portable hunk. |
| `primary_stream_tier3_equivalence` | harness | AUTO_ROUTE | spec `primary-stream-tier3.json` | 73, 76 | SPEC+RR. `attemptUncensoredRetry` precedes the chain. |
| `answer_confirmation_tier3_equivalence` | harness | AUTO_ROUTE, is_dangerous_chat, routeTrail | builder `build-answer-confirmation-fixture.ts` + jest case (uncensoredTextProfileId) | 76 | SPEC+RR. |
| `memory_processor_tier3_equivalence` | harness | is_dangerous_chat | spec `memory-processor-tier3.json` (AUTO_ROUTE, isDangerousChat, uncensoredTextProfileId) | 76 | SPEC+RR. Memory reads `readConciergeSettings`. |
| `compression_tier3_equivalence` | harness | isDangerousChat, is_dangerous_chat | spec `compression-tier3.json` | 76 | SPEC+RR. |
| `cheap_llm_selection_equivalence` | harness | is_dangerous_chat | tsx `cheap-llm-selection.ts` + spec `cheap-llm-selection.json` (11) | 74, 76 | SPEC+RR. `core-execution` carries `finishReason` (#74). |
| `precompute_equivalence` | harness | uncensoredTextProfileId, finish_reason | `episodic-recall-main.db` (pepper `oracle-test`), spec `precompute-cases.json` | 76 | SPEC (+ WF for `conciergeSettings`). |
| `context_summary_service_tier3_equivalence` | harness | (builder) conciergeOverride, isDangerousChat | builder `build-context-summary-service-fixture.ts`, jest (uncensoredTextProfileId) | 75, 76 | SPEC+RR. |
| `file_attachment_tier3_equivalence` | harness | uncensoredImageDescriptionProfileId, finish_reason | builders `build-file-attachment-fixture.ts` / `build-attach-file-fixture.ts` | 76 | SPEC+RR. The vision fallback moves into `conciergeSettings.uncensoredVisionProfileId`. |
| `chat_settings_tier2_equivalence` | harness | dangerous_content_settings | spec `chat-settings-tier2.json` (AUTO_ROUTE, dangerousContentSettings, imagePromptProfileId, uncensoredImageDescriptionProfileId) | 76 | **SPEC+RR.** `ChatSettingsSchema` loses both legacy keys and `CheapLLMSettings.imagePromptProfileId`, and gains `conciergeSettings` with `.default()`. |
| `settings_routes_equivalence` | harness | dangerousContentSettings, availableActions | spec `settings.json`, jest `settings-routes.test.ts` (20) | 76 | RR + ARM: **400 on any retired key.** |
| `help_tools_equivalence` | harness | — (spec: dangerousContentSettings, imagePromptProfileId, uncensoredImageDescriptionProfileId, `help_navigate`) | spec `help-tools-tier2.json` | 76 | SPEC+RR. `help_settings` gains a `concierge` category. |
| `chat_create_capstone_equivalence` | harness | conciergeState, content-filter, provider_routing | spec `chat-create-capstone.json` (19) + builder `build-chat-create-capstone.ts` | 75, 76 | SPEC+RR. The create response carries the post-flip columns, `cs_wrong_type_400` moves to the three-value enum, and the greeting's content-filter fallback is skipped on Locked (ARM). |
| `projects_routes_equivalence` | harness | conciergeOverride, conciergeState, isDangerousChat | `groups-projects-main.db`, jest `projects-routes.test.ts` | 75 | WF + RR. List payloads gain setBy/reason. |
| `characters_reads_equivalence` | harness | conciergeOverride, conciergeState, isDangerousChat, routeTrail | `characters-main.db`, jest `characters-reads.test.ts` (10) | 75 | WF + RR. |
| `chat_cast_routes_equivalence` / `inform_drop_lives_on_the_action` | harness | conciergeState | `chat-cast-main.db` (pepper `chat-cast`) | 75 | WF + RR if the payload projects the state. |
| `chat_scenario_routes_equivalence` | harness | conciergeState, routeTrail | `chat-scenario-main.db` (also lacks `routeTrail` and `transcriptVersion`; the jest case plants them at :237/240/243) | 73, 75 | WF + RR. Check the plant guards. |
| `chat_admin_routes_equivalence` | harness | routeTrail | `chat-admin-main.db`, jest case | 73, 77 | RR (the `availableActions` list). |
| `chat_continuation_tier2_equivalence` | harness | routeTrail | spec `chat-continuation-tier2.json`, built `qt-continuation-main.db` | 73 | RR. |
| `chat_export_equivalence` | harness | route_trail | `chat-dialogs-main.db` | 73 | RR. |
| `courier_images_routes_equivalence` | harness | routeTrail | `courier-images-main.db` + `lib/p4d171-columns.ts` | 73, 76 | WF + RR. |
| `brahma_console_routes_equivalence` / `brahma_orchestrator_tier3_equivalence` | harness | routeTrail | `brahma-main.db` (still lacks `transcriptVersion`) | 73, 75 | WF. |
| `system_export_equivalence` / `system_import_state` | harness | routeTrail | `system-data-main.db` (oracle cases' `ADD COLUMN` plants were guarded at P4.111) | 73, 75, 76 | RR. The export schema marks `conciergeOverride` deprecated; `.qtap` import runs `withConciergeModeFromLegacy`. |
| `system_restore_state` / `system_restore_equivalence` | harness | conciergeOverride (:1599-1606), imagePromptProfileId (:609) | restore archives + jest `system-restore.test.ts` | 75, 76 | RR + ARM. Restore translates the legacy settings and the state; `uuid-remap` remaps the four desk ids. |
| `restore_vintage_state` | harness | routeTrail, route_trail (calls `ensure_chat_messages_route_trail_column`) | migration-vintage trio | 74, 75, 76 | **Tripwire:** `no_restore_phase_names_a_column_a_migrated_table_lacks` (:258). Rebuild the vintage trio at the target. |
| `backup_uuid_remap_equivalence` | harness | — (jest `backup-uuid-remap.test.ts`: dangerousContentSettings, imagePromptProfileId, routeTrail, uncensored×3) | spec `uuid-remap-corpus.json` (12) | 73, 76 | SPEC+RR + ARM (the four desk ids in `conciergeSettings`). |
| `qtap_schema_validate_equivalence` | harness | — (spec: `"evidence"`, routeTrail) | spec `qtap-schema-validate.json` over `system-data-*` | 73, 75 | RR after re-vendoring the export schema (evidence enum widened, `conciergeOverride` deprecated). |
| `chats_messages_ops_tier2_equivalence` / `chats_messages_read_equivalence` / `chats_messages_tier2_equivalence` | harness | dangerFlags, `"evidence"`, routeTrail | specs `chats-messages-{ops,read,}-tier2.json` | 73, 77 | RR. The `chats-messages.ops` zod widens the trail; `saveToolMessages` gains `options.createdAt`. |
| `chats_read_equivalence` / `chats_tier2_equivalence` | harness | conciergeOverride, isDangerousChat | specs `chats-read-tier2.json`, `chats-tier2.json` | 75, 76 | SPEC+RR. `patchOnlyFields()` means a whole-row `_update` omits the three mode columns: an ARM. |
| `home_routes_equivalence` | harness | conciergeOverride, isDangerousChat | `home-main.db` (**populated:** `OFF`+`UNCENSORED` rows, two `isDangerousChat=1`) via `build-home-fixture.ts`, spec `home-web.json` | 75 | WF (with backfill) + RR. Home-data gains setBy/reason. |
| `in_scene_voiced_tier3_equivalence` | harness | (builder) AUTO_ROUTE, conciergeOverride, dangerousContentSettings, uncensoredTextProfileId | `in-scene-voiced-main.db` (**populated:** `UNCENSORED`, mode `AUTO_ROUTE`) | 75, 76 | WF (with backfill) + RR. |
| `almanack_tier2_equivalence` | harness | (builder) dangerousContentSettings, imagePromptProfileId, isDangerousChat | `almanack-main.db` (**populated:** `isDangerousChat=1`, mode `DETECT_ONLY`, **one row with `imagePromptProfileId` set**) | 76 | WF (with the #76 backfill, the one pair exercising the `imagePromptProfileId` move) + RR. |
| `carina_memory_extraction_tier3`, `memory_pipeline_jobs_tier3`, `enclave_step_tier3`, `courier_transport_tier3` | harness | (builders) dangerousContentSettings / AUTO_ROUTE | builders `build-{carina-memory-extraction,memory-pipeline-jobs,enclave-step,courier-transport}-fixture.ts`, spec `memory-pipeline-jobs-tier3.json` | 76 | SPEC (builders) + RR. |
| `post_office_concierge_lantern_suparna_equivalence` / `post_office_writers_tier3_equivalence` | harness | concierge_notifications | tsx `post-office-concierge-lantern-suparna.ts` | 73, 74, 75, 77 | RR + ARM. New announcement kinds: `rerouted` / `no-understudy` / `not-permitted`, `auto-flagged-refusals`, `set-moderated/-unmoderated/-locked`, `auto-unmoderated`, `background-refused`. |
| `announcer_tier3_equivalence`, `announcement_attribution_equivalence` | harness | (Concierge senders) | — | 73, 75 | Check whether they pin the Concierge writer's kinds; RR if so *(predicted)*. |
| `initial_greeting_equivalence` | harness | content-filter, content_filter | — | 75 | ARM: the greeting's content-filter fallback is skipped on Locked. |
| `help_chat_orchestrator_tier3_equivalence`, `tool_dispatch_equivalence`, `tool_definitions_equivalence`, `pseudo_tool_prompts` family (`text_block_prompt.rs` carries the `help_navigate` example) | harness | `help_navigate` bytes | specs `help-chat-orchestrator-tier3.json`, `tool-dispatch.json`, tsx `tool-definitions*.ts`, `pseudo-tool-prompts.ts` | 76 | RR (the `helpSettings` enum + description, the `helpNavigate` example URL). |
| `danger_*`-adjacent state families with no hits but committed pairs carrying `chats` | harness | — | 42 chats-bearing pairs (C.1) | 74, 75 | **WF for EVERY chats-reading family** once v5's strict `chats_read::ALL_COLUMNS` (:71) names the three mode columns (C.3). |

### B.2 Web and host tests

| File | Kind | Terms | Fixture | Commit | Effect |
|---|---|---|---|---|---|
| `quilltap-web/tests/dispatch_wrong_type_census.rs` | census | conciergeState, concierge_state (8) | — | 75, 76, 77 | See D.1. |
| `quilltap-web/tests/tri_state_edges_share_the_decoder.rs` | guard | concierge_state | — | 75 | Moves only if `ChatUpdate.concierge_state` is renamed or retyped. |
| `quilltap-web/tests/source_census/mod.rs` | shared lexer | concierge_state | — | 75 | See D.1's `strip_noise` hazard. |
| `quilltap-web/tests/query_param_semantics_equivalence.rs` | web route family | the `availableActions` of chat POST | — | 77 | **RR:** `retry-image-uncensored` is inserted after `regenerate-background`. |
| `quilltap-web/tests/web_edge_action_sites_census.rs` | census | — | — | 77 | Unmoved if the new action reads through `dispatch_required_action` (the `ALLOWED` list is `[("system_data_routes.rs",1)]`). |
| `quilltap-web/tests/messages_swipe_sse_route.rs`, `message_swipe_stream_dispatch_wire.rs` | web | swipe SSE | `salon-*` via `p4d171-columns.ts` | 77 | ARM for `retry-uncensored` (`&stream=1` SSE). |
| `quilltap-web/tests/images_edge_routes.rs`, `images_generate_dispatch_wire.rs` | web | — | spec `images-collection.json` | 73, 76 | RR. |
| `quilltap-host/tests/host_boot.rs` | host | conciergeOverride, dangerFlags, isDangerousChat, routeTrail (:35-120 hand DDL) | hand-rolled | 74, 75, 76 | **DDL** (`d23-redump-is-not-only-fresh-schema`), plus a new ensure arm. |
| `quilltap-host/tests/host_cadence.rs` | host | conciergeOverride, dangerousContentSettings (:39, :106-111, :174 INSERT) | hand-rolled | 75, 76 | DDL + INSERT rewrite. |
| `quilltap-host/tests/host_llm_log_cleanup.rs` | host | dangerousContentSettings (:44 hand DDL) | `llm-log-cleanup-main.db` (settings only, mode `OFF`) | 76 | DDL + WF. |
| `quilltap-host/tests/host_boot_p4d182_columns.rs` | host | — | — | 74 | The **template** for a `host_boot_<order>_columns.rs` (the ledger columns cannot come from the D23 dump). |

### B.3 SPA specs, SPA oracle fixtures, e2e

| File | Kind | Terms | Commit | Effect |
|---|---|---|---|---|
| `src/app/chat/concierge-state.spec.ts` (17), `concierge-mark.spec.ts` | SPA spec | conciergeOverride, conciergeState, isDangerousChat | 75 | Rewrite to three states. |
| `src/app/chat/concierge-state-presentation.spec.ts` + **`concierge-state-presentation.v4.json`** (recorder `harness/oracle/cases/concierge-presentation.mjs`) | SPA spec + SPA oracle fixture | four-state table | 75 | RR at target (three-row table). |
| `src/app/chat/{chat-view-model,conversation-header,merge-conversation-modal,message-list,message-row,streaming-message}.spec.ts` | SPA spec | conciergeOverride, isDangerousChat, routeTrail | 73, 75, 77 | Rewrite fixtures. `message-row` gains "Try uncensored" and the "Not Dangerous" blur lift. |
| `src/app/chat/sidebar/chat-section.spec.ts` (17) | SPA spec | conciergeOverride, conciergeState, isDangerousChat | 75 | Three-option select. |
| `src/app/chat/{route-trail-badge,tool-message,announcement-group,confirmation-badge,courier-bubble,group-tool-messages}.spec.ts` | SPA spec | routeTrail | 73, 77 | The `ToolMessage` "Tried:" badge, and "Try uncensored" on ToolMessage. |
| `src/app/chat/route-trail-display.oracle.spec.ts` + `src/testing/fixtures/route-trail-display.oracle.ndjson` (recorder `apps/web/oracle/route-trail-display.ts`, "Expect 25 lines") | SPA oracle fixture | the RouteAttempt enums | 73 | RR; the line count moves. |
| `src/app/quick-hide/quick-hide-consumers.spec.ts` (12), `screens/{home/home,salon/chat-card,salon/salon-list,prospero/cards/project-chats-section,characters/view/tabs/conversations-tab}.spec.ts`, `workspace/chrome/link-interceptor.spec.ts` | SPA spec | conciergeState | 75 | Fixtures gain setBy/reason. |
| `src/app/screens/new-chat/{new-chat-form,new-chat.logic}.spec.ts` | SPA spec | conciergeState | 75, 76 | Three states, `newChatsStartAs`. |
| `src/app/screens/salon/{salon-conversation,salon-impersonation-voice,salon-settings-live,salon-turn-controls}.spec.ts` | SPA spec | conciergeOverride, isDangerousChat, routeTrail | 75 | Rewrite fixtures. |
| `src/app/screens/settings/chat/{async-select-cards,connection-profiles-shared-entry}.spec.ts` | SPA spec | AUTO_ROUTE, dangerousContentSettings, imagePromptProfileId, uncensored* | 76 | Move to the new `ConciergeTabContent` five cards; `dangerous-content-settings.ts` is deleted. |
| `src/app/help/__fixtures__/{help-guide-tables,label-from-url-vectors}.json` (recorder `apps/web/oracle/help-guide-capture.test.tsx`), `help-stream.spec.ts`, `help-categories.spec.ts` | SPA oracle fixture | `dangerous-content` slug, `section=dangerous-content` | 76 | RR (slug swap + `/settings?tab=concierge`). |
| `e2e/salon-concierge-four-state-flow.spec.ts` | e2e | conciergeOverride, isDangerousChat (raw SELECT :292) | 75 | **RETIRE → three-state flow spec.** |
| `e2e/salon-danger-avatar-flow.spec.ts` | e2e | conciergeOverride, conciergeState, isDangerousChat (raw SELECT :208) | 75 | Rewrite the SELECT to `conciergeMode`. |
| `e2e/concierge-marks-flow.spec.ts` (10), `new-chat-flow.spec.ts`, `salon-streaming-avatar-flow.spec.ts` | e2e | conciergeState | 75 | Three-value payloads. |
| `e2e/settings-chat-cards-flow.spec.ts` | e2e | dangerousContentSettings, `section=dangerous-content` | 76 | Move to the Concierge tab. |
| `e2e/salon-route-trail-flow.spec.ts` | e2e | routeTrail | 73 | Add a TOOL-row trail beat. |
| `e2e/global-setup.ts` (copies `chat-send-main.db`) | e2e seed | — | 74, 75, 76 | The seed is pre-#74 vintage. It depends on the boot ensure(s) plus backfill decided in F.2. |

---

## C. The committed fixture DB pairs that bind the moving columns

### C.1 Measured (scratch probe, read-only, per `PRAGMA table_info`)

- **No committed file carries ANY post-#73 column.** `conciergeMode`, `moderationRefusalCount` and
  `chat_settings.conciergeSettings` are absent from every file.
- **Every `chats` table carries both `conciergeOverride` and `isDangerousChat`.**

**The 42 chats-bearing mains:** almanack, attach-file, autonomous, avatar-rolls, brahma, character-archive,
character-generators, characters, chat-admin, chat-cast, chat-compressed, chat-delete, chat-dialogs, chat-gallery,
chat-scenario, chat-send, conversation-summaries-regen, cost-background, courier-images, documents,
embedding-remainder, episodic-recall, files, groups-projects, headshoulders, help-chat, home, images, in-scene-voiced,
inspector, inspector-nostore, memories, pascal-run-custom, photos, post-office, salon-long, salon, subprompts,
system-data, text-replacements, wardrobe-routes, and `migration-vintage/quilltap.db`.

**The 23 pairs with a `chat_settings` table** (all carry `dangerousContentSettings` and
`uncensoredImageDescriptionProfileId`): almanack, attach-file, autonomous, chat-admin, chat-delete, chat-dialogs,
chat-scenario, chat-send, cost-background, courier-images, embedding-remainder, episodic-recall, headshoulders,
help-chat, images, in-scene-voiced, llm-log-cleanup, memories, pascal-run-custom (0 rows), salon-long, salon,
system-data, and `migration-vintage/quilltap.db` (0 rows).

**The pairs whose DATA makes a backfill non-trivial** (everything else is NULL/0 → `moderated`, with the settings
`mode='OFF'` → `enabled=false` *(per the ledger's reading of `mapLegacyConciergeSettings`)*):

| Pair | `conciergeOverride` non-NULL | `isDangerousChat=1` | settings `mode` | `imagePromptProfileId` set |
|---|---|---|---|---|
| `chat-send-main.db` (the **e2e seed**, and `danger_trigger`) | `OFF` | 1 | `AUTO_ROUTE` | 0 |
| `home-main.db` | `OFF`, `UNCENSORED` | 2 | — (no settings table) | — |
| `in-scene-voiced-main.db` | `UNCENSORED` | 0 | `AUTO_ROUTE` | 0 |
| `almanack-main.db` | — | 1 | `DETECT_ONLY` | **1** |
| `salon-main.db`, `salon-long-main.db` | — | 0 | `DETECT_ONLY` | 0 |

Other facts:
- `chat_messages.routeTrail` is present on most mains, with **0 non-NULL rows anywhere**. So no committed pair holds
  a trail, and #73's widening moves no stored cell.
- `routeTrail` is **absent** from `chat-scenario`, `chat-send`, `files`, `headshoulders`, `salon-long` and the
  migration-vintage main.
- `chats.transcriptVersion` is absent from 13: avatar-rolls, brahma, character-archive, chat-scenario, chat-send,
  files, headshoulders, help-chat, in-scene-voiced, memories, post-office, salon-long, and the vintage main.
  That is pre-existing, healed per copy by `ensure_p4d182_columns`.

### C.2 How the harness opens the pairs

- Test peppers:
  - `quilltap-web/tests/common/mod.rs:17` `TEST_PEPPER` opens most pairs.
  - The `images-*` pair uses `images-collection.json`'s pepper.
  - `chat-cast-*` uses `chat-cast.json`'s.
  - `episodic-recall-*` uses `episodic-recall.json`'s (`oracle-test`).
  - `conversation-summaries-regen-*`, `embedding-profiles-*` and `wardrobe-instructions-*` use the tier-2 pepper.
  - The migration-vintage trio uses `3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=`, the provisioning pepper. **It is
    NOT in the migrator's `TEST_PEPPERS`**, but it is rebuilt, not widened.
- The key goes in as raw hex `PRAGMA key="x'<hex>'"`, as in production.
- Families copy the pair to `/tmp/qt-*` (`qt-cc-main.db`, `orch-main.db`, …), then heal the copy (`ensure_p4d171/182_columns`)
  or boot a host over it. Host boots run the `*_repair.rs` ensures.

### C.3 Which pairs the four migrations must reach, and why

| Migration | Columns | Zod shape at target | Fatal for v4 regen on an unwidened pair? | Fatal for v5? | Pairs |
|---|---|---|---|---|---|
| `add-chat-refusal-ledger-v1` (#74) | `chats.moderationRefusalCount INTEGER NOT NULL DEFAULT 0`, `lastModerationRefusalAt TEXT DEFAULT NULL` | **absent from `ChatMetadataSchema`** (measured: no hit in `lib/schemas` at `ce2f1dabf`) | Only on a path that runs `$inc`/`getModerationRefusalLedger` (`no such column`). Whole-row writes never name it. | Only where v5's port reads or writes it (raw SQL, kept OUT of `ALL_COLUMNS`). | All 42 chats-bearing mains, **or a per-copy ensure**. The **generateDDL re-dump will NOT carry it**, so it needs a boot ensure (the P4.D182 model). |
| `add-chat-concierge-mode-v1` (#75) | `conciergeMode TEXT DEFAULT 'moderated'`, `conciergeModeSetBy`, `conciergeModeReason` (+ JS backfill) | `.nullable().optional()` on `ChatMetadataSchema` (:958-961 and :1337-1340) → the D23 dump carries them | **No** (optional; `patchOnlyFields()` omits them from whole-row `_update`). | **YES, on every chats READ.** `chats_read.rs:71` `ALL_COLUMNS` is a strict literal (`SELECT {ALL_COLUMNS} FROM chats`, :356); adding the trio fails every read on all 42 pairs. | All 42, **with the backfill** on `chat-send`/`home`/`in-scene-voiced`/`almanack` (C.1). |
| `add-concierge-settings-v1` (#76) | `chat_settings.conciergeSettings TEXT DEFAULT NULL` (+ the `mapLegacyConciergeSettings` backfill) | `.default({...})` in `ChatSettingsSchema` (`settings.types.ts:722`) | **YES.** `$set: validated` names it on every `chat_settings` update, so v4 dies `no such column: conciergeSettings` (the `composerEmoji` class). | No: `chat_settings.rs` is tolerant (`tolerant_insert` / `tolerant_select_list`). | All 23 settings-bearing pairs. Of these, `almanack-main` is the only one proving the `imagePromptProfileId` move, and `chat-send`/`in-scene-voiced` (AUTO_ROUTE) plus `almanack`/`salon`/`salon-long` (DETECT_ONLY) prove the mode map. |
| `drop-chat-concierge-override-v1` (#76) | `ALTER TABLE "chats" DROP COLUMN "conciergeOverride"` | removed from the schema; the dump will LACK it | No (an extra nullable column is invisible to v4). | Only while v5 still binds it. After the port it is harmless extra, but a **v5 read of a real post-dev.88 instance fails until the port lands** (ledger §1). | Optional for the pairs. It **must** be reflected in the vintage trio (rebuild) and in any pair a "v4 dropped it" arm wants to exercise. |

**Also moving: the re-dump of `fresh_schema.json`** *(predicted from the target Zod; measure with `dump-fresh-schema.ts`)*:
- `chats`: gains the mode trio and loses `conciergeOverride`.
- `chat_settings`: gains `conciergeSettings` and **loses `dangerousContentSettings` and
  `uncensoredImageDescriptionProfileId`**. Migrated instances KEEP both, deprecated, so fresh and migrated shapes
  diverge on `chat_settings`. The tolerant reads cope; a strict mirror would not.
- `chat_settings_seed.json` moves too: the seed gains `conciergeSettings` and drops the two keys.

**Does the existing widening tool apply v4's own statements?** Yes, for `ADD COLUMN` (verbatim, with `source:`
file:line citations). But:
- It has no row type for **DROP** or for **data backfill**.
- It holds only 5 peppers. Every probed pair opened with one of the 11 known peppers, and `pep1` (provisioning) is
  used only by the vintage trio.
- #75's and #76's backfills live in JS.

So a faithful widen needs a new migrator arm that **runs v4's migration module (or its exported mapper) against the
fixture handle**. The alternative is a documented decision to widen without backfill: C.1's five populated pairs are
then the only pairs where the post-state differs from a real upgrade.

### C.4 The rule (memory `fixture-vintage-gap-which-columns-are-fatal` + `a-nullable-optional-column-is-fatal-for-v5-not-v4`)

1. v4's `_update` writes `$set: validated`, the whole parsed entity. So a **Zod `.default()` column the fixture
   lacks kills v4's own regen** (`no such column`, bare 500). An `.optional()` one is never named, so it is harmless
   **to v4**.
2. v5 builds statements from **fixed column lists**. So **any column v5's SQL names**, optional or not, kills v5 on a
   pre-migration fixture. A family that only reads stays green, which makes the reds look arbitrary: exactly the
   writers, or here, every chats reader.
3. MANAGED_FIELDS (`characters.metadata`/`canChooseOutfit`) are never added. No v4 migration adds them.
4. The fix is to widen the committed pair through **v4's own migration statements** (migration shape wins over
   `generateDDL` where they disagree), or a per-COPY `test_support::ensure_p4dNNN_columns` when the order forbids
   touching pairs. **The ORACLE side needs the same heal**, or v4 records its own 500.
5. Adopting a column stales every pair carrying that table **the same day**. Measure with `--report-only` and grep
   oracle cases for unguarded `ADD COLUMN` plants over the pair **before** applying.

---

## D. The censuses and guards that will move

### D.1 `crates/quilltap-web/tests/dispatch_wrong_type_census.rs`

**What it counts.** It walks `Request`'s typed fields out of `api/types.rs` using its OWN `strip_noise` (:2361-2372),
which blanks only lines that START with `//` or `#[`. It then asserts:

```rust
const EXCLUDED_BY_THE_ROUTE_IDENTIFIER_RULE: usize = 449;   // :2580
assert_eq!(excluded, EXCLUDED_BY_THE_ROUTE_IDENTIFIER_RULE, "the route-identifier exclusion now drops {excluded} typed fields — re-measure …");  // :2595
```

The count is fields matching `is_route_identifier` (the `*_id`/`*_ids` heuristic). Every other typed field must have
a `Row`. The `CHAT_CREATE_TRIO` rows are driven against `ChatCreateRequest` (`concierge_state` is
`Option<Option<Value>>`, `FIXED(P4.73)`), and the probe at :2797 posts `{"conciergeState":42}` and expects accept.

**What moves it:**
- **#77.** A `Request::ChatRetryImageUncensored { chat_id, tool_message_id?, … }` and a
  `MessageRetryUncensored { chat_id, message_id, stream }` each carry `*_id` fields, and each one moves 449 by +1 per
  typed `*_id` (memory `a-new-verb-moves-the-dispatch-wrong-type-census`). Body keys carried as `Option<Option<Value>>`
  do not enter the typed set.
- **#75.** The `conciergeState` enum becomes three values. The TYPE does not change (still a raw `Value`), so the
  count is unmoved, but the `note:` text and the capstone arms re-record.
- **#76.** The settings PUT's retired-key 400 is a handler refusal, so the census is unmoved unless it adds typed
  fields.
- ⚠ **The `strip_noise` hazard (P4.115 item 5).** `ChatUpdate.concierge_state`'s MULTI-LINE `#[serde(…)]` attribute is
  exactly what hides `ChatUpdate.remove_participant_id`. `strip_noise_hides_exactly_one_typed_field_the_shared_rule_finds`
  pins that difference both ways. **If #75 rewrites that attribute (collapses it, renames the field or retypes it),
  that test reddens and 449 shifts by one**, independently of any new verb. Memory:
  `dispatch-census-strip-noise-and-multi-line-serde-attrs`. The switch to the shared rule is flagged as "the unifier's
  call", 449 → 450.

### D.2 `crates/quilltap-web/tests/web_edge_action_sites_census.rs`

- `const ALLOWED: &[(&str, usize)] = &[("system_data_routes.rs", 1)];` (:32). It counts every `"action"` string literal
  in `crates/quilltap-web/src/*.rs` except `query.rs`, after the lexer strip and the test-module brace strip.
- **Moved only if** the new chat or message action is read other than through `crate::query::dispatch_required_action`.
- **The action LIST is separate:** `CHAT_POST_ACTIONS` (`crates/quilltap-web/src/wardrobe_routes.rs:419-467`, 47
  entries). #77 inserts `retry-image-uncensored` right after `regenerate-background` (:442). That list is pinned by
  `query_param_semantics_equivalence` (web) through v4's `availableActions`, and possibly by `chat_delete_equivalence`
  and `system_jobs_routes_equivalence` (both record `availableActions` and chat action names). Re-record.
- **Message actions:** v5 has NO REST `chats/{id}/messages/{messageId}` edge (RPC-only `Message*` variants). v4's list
  starts with `override-danger-flag`, which v5 never ported, so a list-order pin is not possible without it (ledger
  trap).

### D.3 The tool-definition snapshots

- `crates/quilltap-core/src/tools/definitions/mod.rs:99` `assert_eq!(TOOL_DEFINITIONS.len(), 59);`: the count is
  unmoved.
- The bytes live in `tools/definitions/data.rs` (`help_settings` enum and description; `help_navigate` at :152 with
  `"/settings?tab=chat&section=dangerous-content"`) and are duplicated in `tools/text_block_prompt.rs`.
- Families: `tool_definitions_equivalence`, the canonical/pseudo-tool families, `help_tools_equivalence`,
  `tool_dispatch_equivalence` and `help_chat_orchestrator_tier3_equivalence` (the mocked request bodies embed the
  catalog).
- Files carrying the example URL bytes: `help-tools-tier2.json`, `help-chat-orchestrator-tier3.json`,
  `tool-dispatch.json`, and the SPA `label-from-url-vectors.json` / `help-stream.ts` / `help-stream.spec.ts` /
  `help-guide-capture.test.tsx`.

### D.4 The help tree

- `crates/quilltap-harness/tests/help_tree_embed_guard.rs:76` `const VENDORED_FILE_COUNT: usize = 129;`. It is
  unmoved, because the count stays 129: `dangerous-content.md` is deleted and `the-concierge.md` added. Its other
  literal copies are in `host_help_docs_boot.rs` (memory `a-vendored-count-is-hard-coded-in-several-crates`).
  - ⚠ It compares embedded vs DISK only, so a stale re-vendor passes it (`help-embed-guard-cannot-see-a-stale-revendor`).
- `help_tree_equivalence` (vs v4's REAL `ensureHelpDocsSynced` at the pin) is the one that moves. There are 19 files
  of difference at v4 HEAD: 15 from the overhaul and 4 from `08c49319d`.
- The `help_docs_*` / `help_*` families that read the tree re-record.

### D.5 The vendored schemas

- `qtap_schema_embed_guard.rs:31` `const VENDORED_BYTES: usize = 95_266;`. **It MOVES**, because v4's
  `public/schemas/qtap-export.schema.json` changed by +18/−4 lines across `b0b6656b5..ce2f1dabf`: the evidence enum
  widens, `profileKind` is added and `conciergeOverride` is deprecated. The v5 copy is
  `crates/quilltap-core/src/generators/qtap-export.schema.json`, and `qtap_export/schema-key-order.json` moves with
  it. The re-vendor obligation reddens the guard by design.
- `public_schemas_vendor_guard` (the `qtap-custom-tool` and `qtap-progression` schemas) has no overhaul hunk, so it is
  unmoved *(measured: only `qtap-export.schema.json` differs under `public/schemas`)*.

### D.6 The version guards

- `zod_version_guard.rs:92` `RECORDED_ZOD_VERSION = "4.6.5"` is **unmoved**. `6d0f88d65` does not bump zod (measured).
- `provider_sdk_version_guard.rs:53-59` records openai `7.20.0`, anthropic `0.115.0`, google-genai `1.52.0` and
  openrouter `1.3.11`. **RED now from the installed checkout**, because `6d0f88d65` moved openai to 7.23.0 and
  openrouter to 1.3.28. This is not the overhaul, but it is the SDK-bump regen event any #73 provider re-record rides
  on.
- #73 also bumps `@quilltap/plugin-types` 2.7.1 → 2.8.0 and regenerates every plugin `index.js`, because the finish
  reasons are now real instead of the hard-coded `'stop'`.

### D.7 How v5 proves its DDL matches v4

- `provisioning_equivalence` checks v5's `sqlite_master` against v4's **live** generateDDL at the pin, byte-exact per
  partition. **It goes RED at the target pin by design** (the D23 tripwire; the chats and chat_settings column sets
  move). The fix is re-running `dump-fresh-schema.ts` at the pin, never a hand edit, and appending the register at
  `provisioning/mod.rs:71-75`.
- Its seed-row leg moves with `chat_settings_seed.json`.
- `chat_settings_column_sites_guard.rs` (the six-sites census, P4.D179) moves when `conciergeSettings` is adopted
  (six sites) and the two legacy columns stop being bound.
- `transcript_version_isolation_guard.rs` (`COLUMN = "transcriptVersion"`, `planted == 7`) is the model for an
  **isolation guard for the two ledger columns**, which v4 keeps out of `ChatMetadataSchema`, `.qtap` exports and
  `ALL_COLUMNS` for the same reason.

### D.8 The boot-ensure precedent (P4.D182)

- `crates/quilltap-core/src/db/chats_transcript_version_repair.rs` is the "ONLY source on every instance, because
  generateDDL cannot emit it" shape, with the negative pins in `db::chats_read`'s index census and `db::chats`'s
  `ChatUpdate` guard.
- Its tests are `crates/quilltap-host/tests/host_boot_p4d182_columns.rs` (one arm per entrance: setup, unlock and
  boot) and `transcript_version_isolation_guard.rs`.
- Siblings: `chats_cycle_order_repair.rs`, `chat_messages_route_trail_repair.rs`, `chat_settings_*_repair.rs` (13
  files).
- `test_support::ensure_p4d182_columns` is the per-copy heal.

---

## E. Traps for the orders

Memory notes live in `~/.claude/projects/-Users-csebold-source-quilltap-v5/memory/`.

**(1) Widening committed fixtures through v4's own migrations**
- `fixture-vintage-gap-which-columns-are-fatal`: a `.default()` column (here `conciergeSettings`) kills v4's own regen; widen the same lane, `--report-only` first.
- `a-nullable-optional-column-is-fatal-for-v5-not-v4`: the mode trio is optional for v4 but fatal for v5's strict `ALL_COLUMNS`; `restore_vintage_state` is the free tripwire.
- `fixture-rebuild-vs-migrate-in-place`: migrate in place through v4's compareSchemas; a rebuild mints ids other files hard-code.
- `a-fixture-widen-breaks-green-readers-whose-oracle-plants-the-column`: grep cases for unguarded `ADD COLUMN` over the pair (`salon-reads.test.ts`, `chat-scenario-routes.test.ts`, `p4d171-columns.ts`).
- `the-oracle-side-needs-the-vintage-heal-too` and `an-oracle-side-that-never-heals-its-fixture-copy-is-red-at-both-pins`: heal BOTH sides; a mutual 500 compares equal.
- `a-widened-shared-column-breaks-sibling-fixtures-invisibly`: stale siblings SKIP, and a SKIP passes.
- `d23-redump-is-not-only-fresh-schema`: chase the hand-written DDL mirrors (`host_boot.rs`, `host_cadence.rs`, `host_llm_log_cleanup.rs`, the web `LLM_LOGS_DDL`).
- `migration-vintage-fixture-goes-stale`: rebuild the vintage trio at the pin. It is the only place the DROP and the JS backfills arrive for free.
- `an-oracle-built-fixture-has-no-migrations-applied`: tier-2 specs built via `initializeDatabase()` run no migration runner, so backfills never happen there.

**(2) Retiring both-directions pins when v4 converges or deletes**
- `closing-a-divergence-moves-the-censuses-that-recorded-it`: only `--workspace` shows which census.
- `retiring-a-v4-deleted-methods-oracle-row-keeps-the-fixture`: filter the retired op in the case and the test, never the committed corpus (applies to `danger-routing` / `isImageModerationError`, `resolveDangerousContentSettings`).
- `deleting-a-ts-union-member-is-not-deleting-a-serde-variant`: keep round-tripping catch-alls for retired values. Mirror case: v5's CLOSED `RouteAttemptEvidence` rejects v4's three new evidence values on a Friday row written by v4 ≥ dev.81.
- `p4.d91-bug78-bug79-convergence-lane`: a pin that does NOT fire is a finding.

**(3) Re-recording at a pin whose `node_modules` are HEAD's**
- `pinned-regen-still-uses-current-node-modules`: the pin's symlinked tree is HEAD's (openai 7.23.0, openrouter 1.3.28, plugin-types 2.8.0).
- `a-pinned-regen-cannot-prove-an-sdk-bump-the-plugin-dirs-never-installed`: the plugin dirs have their own `node_modules`.
- `regenerate-at-both-pins-and-cmp-is-not-universal`: six of thirteen families differ across same-pin regens.
- `an-encoding-drift-poisons-every-oracle-built-fixture`: stack on a substrate commit when v4 changes what the jest init creates.

**(4) A new `*_id` field moves the dispatch census**
- `a-new-verb-moves-the-dispatch-wrong-type-census`: +1 per typed `*_id`, visible only in a `--workspace` run.
- `dispatch-census-strip-noise-and-multi-line-serde-attrs`: `ChatUpdate.concierge_state`'s attribute is the load-bearing one here.

**(5) A required field on a shared struct crossing lanes**
- `a-required-field-on-a-shared-struct-crosses-lane-ownership`: e.g. a non-`Option` `concierge_mode` on `ChatRow` or `ChatSettings`, or `profile_kind` on `RouteAttempt`.

**(6) A fixture builder erroring silently / stale pass**
- `a-stale-recipe-header-makes-a-family-skip-and-print-ok` and `a-family-env-var-is-not-its-regen-var`: a missing var SKIPs green in 0.00 s.
- `sweep-recipe-stages-a-spec-the-builder-rewrites`: green having measured nothing.
- `heredoc-assert-failure-does-not-stop-the-compound-command`.
- `a-committed-fixture-builder-belongs-in-prose`.

**(7) Jest-oracle leaks**
- `jest-oracle-plugin-factory-is-mocked`: `createImageProvider` answers undefined, which matters for the typed `ModerationRejectionError` path.
- `jest-oracle-instanceof-across-resetmodules`: `ModerationRejectionError instanceof` silently never fires, so the oracle records PRE-fix behaviour. This is exactly the #73 shape.
- `jest-oracle-empty-provider-registry`, `jest-domock-survives-resetmodules`, `jest-setup-llm-logging-service-mocked` (every jest oracle writes ZERO `llm_logs` rows).
- `a-stubbed-seam-can-make-a-corpus-dimension-unreachable`: `NoApiKeys` made every Concierge reroute fail open; the understudy corpus needs keys.

---

## F. Open questions

1. **How to widen with backfill.**
   - The migrator is ADD-COLUMN-only. Should the round add an arm that imports v4's
     `migrations/scripts/add-chat-concierge-mode.ts` / `add-concierge-settings.ts` `run()` (or the exported
     `mapLegacyConciergeSettings`) and runs it on each fixture handle, and a DROP arm for `conciergeOverride`?
   - Or should it widen columns only and accept that `chat-send`, `home`, `in-scene-voiced` and `almanack` then differ
     from a real upgrade?
   - Does the lane pass v4's `getSQLiteDatabase()` seam, or reimplement the loop?
2. **The e2e seed and v5 boot backfill.** `global-setup.ts` boots v5 over a pre-#74 `chat-send-main.db` (`OFF`,
   `AUTO_ROUTE`). The mode trio and `conciergeSettings` add cleanly as boot ensures, but **a v5 boot-time data backfill
   would be a first** (every `*_repair.rs` to date is DDL-only or a heal). Decide between two options:
   - v5 ports the #75/#76 backfills as boot heals, required anyway for a Friday copy taken before v4 upgrades;
   - or the e2e seed is widened with backfill and v5 declines pre-4.10-dev.86 instances.
3. **The DROP on v5.** Does v5 ever issue `ALTER TABLE chats DROP COLUMN conciergeOverride` itself? A destructive boot
   ensure on a shared iCloud instance is new ground. Or does v5 just stop binding it and tolerate either shape?
   Fresh-provisioned v5 instances will lack it after the re-dump; migrated ones may keep it.
4. **Do fresh and migrated shapes diverge on `chat_settings`?** The re-dump will likely drop `dangerousContentSettings`
   and `uncensoredImageDescriptionProfileId`, while v4's migration keeps them. Measure with `dump-fresh-schema.ts` at
   `ce2f1dabf` before any order relies on it. Also measure whether generateDDL emits the #74 ledger columns; the
   survey's Zod grep says it cannot.
5. **Per-phase or whole.** #74's intermediate vocabulary (`flagged`, `autoSwitchAfterRefusals` as a flat setting) is
   superseded by #75/#76. Recording the oracles at `ce2f1dabf` only avoids three intermediate re-records. But
   `08c49319d` and `6d0f88d65` sit on top, so the pin choice is `ce2f1dabf` (Concierge-complete, SDKs still HEAD's)
   or HEAD.
6. **Fixture scope per order.**
   - Widen all 42 chats-bearing mains in one lane, the P4.111 precedent at the largest scale yet. It could be split by
     pepper.
   - Or add a single `ensure_<order>_columns` per-copy heal on both sides.
   - The ten-file P4.111 pass needed a guarded-plant fix, and here three known cases plant chat columns.
7. **`announcer_tier3` / `announcement_attribution` / `post_office_*`.** Do they pin the Concierge writer's KIND list?
   Pre-phase-3 transcripts carry retired kinds that must still render (the ledger's note), so a catch-all is needed.
8. **`image_dialects_equivalence`.** Retire the keyword-verdict rows, or keep them as a v5-only decline path? The
   typed signal depends on the regenerated plugin bundles. The pin's plugin dirs may not be installed (E.3).
9. **`concierge-presentation.mjs`.** Its recipe hard-codes a lane pin and reads through `git show`. **Measured:**
   `lib/services/dangerous-content/concierge-state-presentation.ts` still exists at `ce2f1dabf`, keyed
   `moderated` / `unmoderated` / `locked`, with provenance as a "note, never a colour". So it is an RR with
   `QT_V4_PIN` re-pointed, and the recorder's shape assumptions (four states) need a check.
