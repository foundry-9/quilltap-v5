# Survey E — Inventory, the Log, the terminal, smalls / vendor orders, SPA / e2e test debt

Survey of v5 `main` @ `4e43b714e`, against v4 at the pin `322a5488e`. The survey was read-only: no builds or tests ran.
Orders covered: P4.D278, D287, D303, D313, D323, D324, D325, D326, D328 (R-6 excluded), D330, D331, D337, D338, D339, D340. Also covered: P4.D334 HANDOFF (9), P4.D336's `isMultiCharacterRoom`, and the `83334b9a5` round's "SPA lows".

**Counts:**

Counted by report heading:

| Status | Count |
|---|---|
| OPEN | 23 distinct (24 headings; D328-T3-7 is folded into D324-T3-33) |
| CLOSED | 16 headings, covering about 27 individual order items |
| NEEDS-RULING | 2 |
| NO-ACTION (v4-faithful, filing candidate, standing class, or carried to a future order) | 27 headings |

Filing candidates (v4 GitHub-issue material; nothing has been drafted):
- the SVG allow-list gaps (D324);
- `copy_inventory_items_to_folder`'s N projections (D324 T3-39);
- R-R, the help chat's slate (D328);
- `withArticle` / "an unicorn horn" (D330 T3-31);
- the `.max(4000)` / `.slice(0, 4000)` disagreement on astral text (D331 T3-34);
- the `handleOpenDocument` new-file residue (D338 T3-25);
- the Scriptorium's generic 500 for a refused title (D340 T3-4);
- `help/character-gallery.md:51` "with its generation prompt" (D323 T3-13);
- "Log line" in help vs "Enter a log line" in the UI (D334 (9)).

---

## P4.D278 — the `01a83539d` vendor order

### D278-11 — stale v4 line citations in `salon-conversation.ts`   [OPEN]
- Source: P4.D278 Tier 3 item 11; the unification header lists it as OPEN for "follow-ups smalls".
- v5 site: `apps/web/src/app/screens/salon/salon-conversation.ts:3535` cites v4 `setPauseState` at `useChatControls.ts:222-228`. `:3551` cites `togglePause` at `:230-238`. (The order measured `:3441` before the file grew.)
- v4 site @322a5488e: `app/salon/[id]/hooks/useChatControls.ts:216-220` (`setPauseState`) and `:223-231` (`togglePause`).
- Fix shape: change the two comment citations to `:216-220` and `:223-231`. It is worth a grep of the file's other `useChatControls:` cites at `:243`, `:715`, `:1355`, `:1451`, `:1470`, `:2942`, `:3030` and `:3068` against the pin in the same pass.
- Differential: none (comments only).
- Files a lane would own: `apps/web/src/app/screens/salon/salon-conversation.ts` (comments only).
- Size: S

### D278-10 — R-E, tightening the bug-139 spec   [NEEDS-RULING]
- Source: P4.D278 Tier 3 item 10 and R-E ("the human's call").
- v5 site: `apps/web/src/app/screens/salon/salon-turn-controls.spec.ts:1500-1535` pins the dispatch order (`chatUpdate` → `chatTurnAction` → `chatSend`) and `isPaused: false`. It does not hold the persist unresolved.
- v4 site @322a5488e: `__tests__/unit/app/salon/hooks/all-llm-pause-actions.test.ts` ("continueAllLLMRoom (bug 139)") holds the pause persist pending and asserts that no turn is requested before it resolves. The code is `app/salon/[id]/hooks/all-llm-pause-actions.ts`.
- Fix shape (if ruled IN): stub `chatUpdate` with a deferred promise, call `onAllLLMContinue()`, assert there is no `chatTurnAction` call, resolve the promise, then assert the order. One new `it(...)`.
- Differential: unit-only (an SPA spec mirrors v4's jest shape).
- Files a lane would own: `salon-turn-controls.spec.ts`.
- Size: S

### D278-12 / D278-13 — post-pin commits; the export schema and CLI templates   [CLOSED]
- Item 12 was absorbed by the `4a38222e6` wardrobe round. Item 13 was P4.D272's and P4.D269's, and both landed at the `01a83539d` unify.

## P4.D287 — the `4a38222e6` vendor order

### D287-6 / D287-7 — `help/scenario-browser.md` and the post-pin pages; bug 229 and `4ce64fa1e`   [CLOSED]
- `help/scenario-browser.md` exists, vendored by P4.D303. P4.D303 ratified `4ce64fa1e` as NO-PORT (the picker fix renumbered to bug 229). The unification header says "OPEN: nothing".

## P4.D303 — the `83334b9a5` vendor order

### D303-9 — `9ec4fa2e9`'s `memories status` "within the cap" line   [NO-ACTION — carried to the CLI `memories` order]
- v5 site: the `quilltap` CLI has `memories` only in `SUBCOMMANDS` (`crates/quilltap-cli/src/main.rs:62`). It dispatches to `not_yet_available` (`:265`), so there is no surface for the line yet. The SPA twin exists (`screens/settings/memory/memory-maintenance-ledger.ts:147`).
- v4 site: `packages/quilltap/lib/memories-commands.js:1323`.
- Carry the line by name to whichever order ports the CLI `memories` family. It is not a smalls item.

### D303-10 — bug 243's live confirmation; the TF-IDF refit WARN   [NO-ACTION]
- This is P4.D292's, and it is a dogfood row. There is no record of it in `dogfood-findings.md` yet, so it stays in the dogfood queue.

### D303-11 — the `write_apply.rs` invalidation table   [CLOSED]
- RESOLVED at the unify by P4.D288's union table, per the header.

### D303-12 — the help-guide re-capture, the export schema, `V4_APP_VERSION`   [CLOSED]
- P4.D300 and P4.D294 landed. `V4_APP_VERSION` is now `"4.11.0-dev.53"` (`crates/quilltap-harness/tests/ai_import_tier3_equivalence.rs:126`).

## P4.D313 — the `25b8fab6a` vendor order

### D313-12 / D313-13 / D313-15 — the HANDOFFs to D308, D309, D305 and D306   [CLOSED]
- The header records all three as byte-equal at unify.

### D313-14a — the CLI captured bytes ("Memory Gate" → "suppress reinforcement at admission")   [CLOSED]
- v5: `crates/quilltap-cli/src/help/main_help.txt:19` and `completion/zsh.template:46`. `fish.template:64` equals the pin byte for byte.

### D313-14b — `extractMigrationId` reads both id forms   [NO-ACTION — carried]
- There is no v5 `migrations status`: `migrations` is listed in `SUBCOMMANDS` but answers `not_yet_available` (`main.rs:265`). Carried by name to the order that ports the CLI `migrations` verb.

### D313-16 — the test-only `is_moderation_refusal`   [OPEN — optional YAGNI]
- Source: P4.D313 Tier 3 item 16 (R-6), "optional follow-up for a smalls round".
- v5 site: `crates/quilltap-core/src/services/dangerous_content/refusal.rs:381` is a `pub fn`. Its only callers are tests: `model/image_dialects.rs:2508-2535` (inside `#[cfg(test)]`, which starts at `:2331`) and `quilltap-harness/tests/refusal_classify_equivalence.rs:47,209`.
- v4 site @322a5488e: deleted in `7417e9a1b`. The oracle re-implements its body through the real classifier (`harness/oracle/cases/refusal-classify.test.ts:223-237`).
- Fix shape: delete the fn. In both test sites, re-aim to `classify_refusal(RefusalInput{error: Some(e), ..Default::default()}).refused`. Alternatively, mark it `#[cfg(test)]`, but the harness is a separate crate and could not see it then.
- Differential: `refusal_classify_equivalence` must stay green (re-run by name).
- Files a lane would own: `refusal.rs`, `image_dialects.rs` (its test module), `refusal_classify_equivalence.rs`.
- Size: S

### D313-17 — `DATABASE_ENCRYPTION.md`'s SQLCipher error   [NO-ACTION]
- The mirror stays as v4 wrote it. CLAUDE.md carries the correction.

### D313-18 / D313-19 — `V4_APP_VERSION` stamp; dependency guards   [CLOSED]
- The stamp is `4.11.0-dev.53`. The guards were green against the pin at the `322a5488e` unify; `provider_sdk_version_guard` is red only against the LIVE checkout, which is a known class.

## P4.D323 — the `e8e17b2d9` vendor order

### D323-11 — vendoring the two Inventory spec docs   [CLOSED]
- `docs/v4/developer/features/complete/inventory.md` and `inventory-tools-and-pascal.md` both exist. P4.D339 reports "`docs/v4/` 502 (zero deferrals)".

### D323-OPEN — stale `at v4 01a83539d` assert message in `host_help_docs_boot.rs`   [CLOSED]
- `crates/quilltap-host/tests/host_help_docs_boot.rs:127-131` now reads `"the vendored tree at v4 322a5488e"`. P4.D339 fixed it.

### D323-12 / D323-14 — the public schemas; `V4_APP_VERSION`   [CLOSED]

### D323-13 — R-W: `help/character-gallery.md:51`'s "with its generation prompt"   [NO-ACTION — v4 filing candidate]
- The line is still present at `help/character-gallery.md:51` and also at `:36`, with identical bytes at the pin. Per the gallery survey §8, no prompt reaches v4's modal. File it with v4; v5 keeps v4's bytes.

### D323-15 / D323-16 — the storybook CSS; unmirrored v4 files   [NO-ACTION]

## P4.D324 — the Inventory keystone

### D324-NOTE-svg — the SVG allow-list gaps v4 shares   [NO-ACTION — v4 GitHub-issue filing candidate (facts only)]
- Source: the P4.D324 header's §3 NOTE and the round record's "Recorded OPEN (notes)".
- v5 site: `crates/quilltap-core/src/inventory/svg.rs:63` (`HREF_ATTRIBUTES = ["href","xlink:href"]`) and `:93-114` (`keep_attribute`). This is a line-for-line port; the §3 reader verified it.
- v4 site @322a5488e: `lib/inventory/svg.ts:40` (`HREF_ATTRIBUTES = new Set(['href', 'xlink:href'])`) and `:44-49` (`keepAttribute`). The relevant logic:
  - `compact = value.replace(/[\s\u0000-\u001f]+/g,'').toLowerCase()`;
  - `if (HREF_ATTRIBUTES.has(lower)) return value.trim().startsWith('#')`;
  - otherwise, every `url(` must be `url(#…)`.
- Facts:
  1. **Namespaced `href`.** Only the literal names `href` and `xlink:href` get the fragment-only rule. `<svg xmlns:x="http://www.w3.org/1999/xlink"><use x:href="https://host/s.svg#a"/>` keeps `x:href`. It contains no `javascript:`, `data:`, `vbscript:` or `url(`, so it passes.
  2. **Entity-encoded `url(`.** The value is checked before XML entity decoding, so `fill="u&#114;l(https://host/x)"` (or `&#x75;rl(`) passes the `url(`-fragment rule and decodes to an external `url(...)` at render. `escape_amps` (`svg.rs:116`, v4's `/&(?![a-z0-9#]+;)/gi`) keeps well-formed entities as-is. The same bypass would apply to `j&#97;vascript:`, but `href` is already fragment-only and `on*` is dropped.
  3. **Mitigation in both apps.** Glyphs render only as an `<img src="data:image/svg+xml;utf8,…">`, which blocks scripts and external fetches in the img context. v5: `apps/web/src/app/inventory/inventory-glyph.ts:12`.
- Fix shape: none in v5 (it stays faithful). If v4 fixes it: match any attribute whose local name is `href`, and decode `&#…;` / `&#x…;` / named entities before the `compact` checks. Port that over `inventory_svg_equivalence`-class rows.
- Size: — (filing)

### D324-NOTE-probe — `chats_read::select_list` runs a per-read `pragma_table_info` probe   [OPEN]
- Source: the P4.D324 header's §3 NOTE.
- v5 site: `crates/quilltap-core/src/db/chats_read.rs:116-130`. Every `chats` read runs `SELECT 1 FROM pragma_table_info('chats') WHERE name='equippedInventory'` and falls back to `NULL` when the column is absent.
- v4 site @322a5488e: `SELECT *` (no probe).
- Fix shape: switch to `conn.prepare_cached(...)`, which uses the per-connection statement cache, or cache the answer per connection. Keep the fallback; it serves fixture opens before the boot ensure. Alternatively, drop the fallback and require `ensure_inventory_columns_on` in the fixtures, but that is wider.
- Differential: unit-only (performance; behaviour is unchanged). Re-run `chats_*_tier2` by name.
- Files a lane would own: `db/chats_read.rs`.
- Size: S

### D324-NOTE-merge — the merge's degraded-mount-index early return skips v4's arrival WARN   [OPEN]
- Source: the P4.D324 header's §3 NOTE.
- v5 site: `crates/quilltap-core/src/services/chat_merge.rs:459-462`. `let Some(mount_w) = w.mount_index() else { return Ok(Vec::new()); };` skips the identity-stack compile, the outfit selection, and `apply_inventory_defaults_holding_lock` (`:504-517`) silently.
- v4 site @322a5488e: `lib/inventory/apply-defaults.ts:98-104`. The per-character `try { withChatInventoryLock(...arriveUnderLock) } catch → logger.warn('[InventoryArrival] Failed to apply arrival defaults for a character', …)`, with `applyInventoryDefaults`' outer WARN at `:72`. (The `apply-chat-merge.ts:265-267` call is the site.)
- Fix shape: on `None`, emit v4's per-character WARN (`chatId`, `characterId`, `source: 'merge'`, error, `context: 'inventory'`) before returning. Measure first whether v4's pool load actually throws on an unreadable mount index; P4.D326 measured `loadInventoryPool` as fail-soft, so the WARN may be unreachable in v4. If it is unreachable, record NEGATIVE instead.
- Differential: `chat_merge`-family tier-2 (`chat_merge_*_equivalence`) with a degraded-mount plant, or unit-only if NEGATIVE.
- Files a lane would own: `services/chat_merge.rs`.
- Size: S

### D324-T3-33 (+ D328-T3-7) — arrival and reset refresh the avatar directly, so `[InventoryEffects] Inventory change effects fired` never fires on arrival   [OPEN]
- Source: P4.D324 Tier 3 item 33 (the §S.5 HANDOFF: "the unifier re-points the call"), and P4.D328 Tier 3 item 7 (R-5 alias re-point). Neither re-point happened at the union.
- v5 site: `crates/quilltap-core/src/inventory/apply_defaults.rs:288-305`. `refresh_avatars_after_arrival` calls `outfit_change_effects::refresh_avatar_for_outfit(…, INVENTORY_CHANGE_AVATAR_CONTEXT)` directly; its callers are `:354` (arrival) and `:415` (reset-to-defaults), plus `services/chat_merge.rs:521`. The doc comment says "The DEBUG `[InventoryEffects]` line around it is P4.D328's". Separately, `services/inventory_change_effects.rs:400` (`notify_inventory_changed`) still calls the `refresh_avatar_for_outfit` alias rather than `refresh_avatar_for_appearance`.
- v4 site @322a5488e: arrival goes `apply-defaults.ts:59` → `equipItemsHoldingLock(..., { provenance:'operator', announce:false })` → `equip.ts:380` → `notifyInventoryChanged`. That runs `equip-effects.ts:114-160`, which calls `refreshAvatarForAppearance(…, 'inventory-change')` and then logs DEBUG `'[InventoryEffects] Inventory change effects fired'` `{chatId, characterId, announced:false, provenance:'operator', equipped, unequipped, context}`.
- Fix shape: replace `refresh_avatars_after_arrival`'s body with `notify_inventory_changed(db, chat_id, character_id, change, &InventoryChangeContext{user_id, provenance: Operator, announce: false, pending: None}, None)` per non-empty change. In `notify_inventory_changed`, call `refresh_avatar_for_appearance`. Confirm that v4's reset path also goes through `equip*` → notify.
- Differential: `inventory_keystone_tier2_equivalence` currently FILTERS OUT `[InventoryEffects]` lines (`:136`, `:470`). Lift that filter for the arrival / reset / merge ops and regenerate from the pin; the oracle `cases/inventory-keystone-tier2.ts` already runs the real effects. Note: the line is emitted after an awaited refresh. If it runs on the writer thread, use `global_capture` (memory note `capture-rig-writer-thread`).
- Files a lane would own: `inventory/apply_defaults.rs`, `services/inventory_change_effects.rs` (one call), `quilltap-harness/tests/inventory_keystone_tier2_equivalence.rs`.
- Size: S–M

### D324-T3-31 / D324-T3-32 / D324-T3-37 / D324-T3-41 — migration lines; the promise-chain map; `analyze-image` / sync `generate`; `d88c1ce39`   [NO-ACTION]
- All are recorded NO-PORT or by-construction.

### D324-T3-34 / D324-T3-35 / D324-T3-36 — the describer, the title lint, the image phrase, routes, carriers and tools   [CLOSED]
- P4.D328, D340, D326, D325 and D327 all landed at the union.

### D324-T3-38 — the `$like` wildcard in `find_chat_ids_carrying_inventory_id`   [NO-ACTION — v4-faithful]

### D324-T3-39 — `copy_inventory_items_to_folder` makes N projections   [NO-ACTION — v4 filing candidate]

### D324-T3-40 — the duplicate rule compares projected names case-insensitively   [NO-ACTION — v4-faithful]

## P4.D325 — Inventory routes

### D325-26 — the 502 over dispatch / Tauri stays a 500   [NO-ACTION — the standing P4.D263 class]
- v5 site: `crates/quilltap-web/src/dispatch.rs:19-33` (`status_for`, `ErrorKind::Internal` → 500). `ErrorKind` (`crates/quilltap-core/src/api/types.rs:5937`) has no 502 kind. The REST edges restore 502 in two places:
  - `quilltap-web/src/inventory_images_routes.rs:80-93` (`generate` only; pinned at `:408-416`);
  - `wardrobe_images_routes.rs:112`.
- v4 site @322a5488e: the images route answers a non-refusal provider failure with 502.
- Observability: the SPA's `CoreDispatchError` carries no HTTP status (D330 T3-35, measured), and Tauri IPC carries none either. Only a raw `POST /api/v1/dispatch` client sees 500 instead of 502.
- Fix shape (if ever wanted): add `ErrorKind::BadGateway` and map it to 502 in `status_for`. Core's provider-failure arms would return it, and the REST special cases would retire. That is a cross-cutting change, not a smalls item.
- Size: — (would be M)

### D325-R2c — `wardrobe_routes::unwrap_to_http` answers 400 for a details-bearing 409   [NEEDS-RULING]
- Source: the P4.D325 lane record, item 1(c). The doc comment calls it "a follow-ups-round question".
- v5 site: `crates/quilltap-web/src/wardrobe_routes.rs:44-54`. Any `details` bag answers 400. The divergence is pinned by `inventory_routes.rs:581-590` (`a_details_bearing_409_stays_409_here_and_not_in_unwrap_to_http`). Today no wardrobe route emits a details-bearing non-400, so the bug is latent.
- Fix shape: make `inventory_routes::render` the shared renderer (status from the error kind; `details` merged), and retire `unwrap_to_http`'s 400 arm. Alternatively, keep both and document.
- Differential: the `wardrobe_routes_equivalence` + `inventory_routes_equivalence` route families, regenerated by name.
- Files a lane would own: `quilltap-web/src/wardrobe_routes.rs`, `inventory_routes.rs`.
- Size: S–M

### D325-notdone — the images family's `direct` generate 201 / 422 / 502 rows; capture of save-to-store's success INFO and the `Generate body is not valid JSON` DEBUG   [OPEN]
- Source: P4.D325 lane record, "Not done".
- v5 site: `crates/quilltap-web/tests/inventory_item_images_routes_equivalence.rs` with `harness/oracle/cases/inventory-item-images-routes.test.ts`. The pair has no image profile, and the served host logs off-thread.
- Fix shape: plant an image profile on the IIIR pair, mock the provider below `generateInventoryItemImage` on v4's side and a scripted image seam on v5's, then add the three rows. Pin the INFO and DEBUG through `global_capture` in a `direct` row.
- Differential: `inventory_item_images_routes_equivalence` (`QT_ORACLE_IIIR`, `QT_FIXTURE_IIIR_MAIN/MOUNT`).
- Size: M

### Other P4.D325 Tier 3 items
- **D325-22 — effects wiring:** CLOSED (wire 8).
- **D325-23 — transfers error shapes:** NO-ACTION. `TRANSFER_ERROR_SHAPES_P4D328` was measured EMPTY: the arms are unreachable on the pair.
- **D325-24 — chat POST gate order:** NO-ACTION (standing `CHAT_GATE_ORDER`).
- **D325-25 — 201 → 200 over dispatch:** NO-ACTION (standing).
- **D325-27 — middleware WARNs:** CLOSED (measured as already emitted).
- **D325-28 — Tauri unit pin:** NO-ACTION (holds by construction).
- **D325-29 — `?projectId` / `?chatId`:** NO-ACTION.
- **D325-30 — help / docs:** CLOSED (P4.D339).

## P4.D326 — the Inventory image side and bug 133

### D326-T2-1 — capture pins for the reroute DEBUG `[DangerousContent] Image profile is already uncensored; not rerouting`   [OPEN]
- Source: the P4.D326 header: "item 1 NOT landed (measured — no family captures the wrapper's lines)".
- v5 site: `crates/quilltap-core/src/services/dangerous_content/provider_routing.rs:310-325`, inside `resolve_image_provider_for_dangerous_content` (`:281`). The DEBUG carries `profile_id` and `profile_name`.
- v4 site @322a5488e: `lib/services/dangerous-content/provider-routing.service.ts:176`, gated by `understudy.ts:98-106` `isUncensoredImageProfile` (`isDangerousCompatible === true` or `id === desk.imageProfileId`).
- What WOULD capture it: **`danger_routing_equivalence`** already drives `resolve_image_provider_for_dangerous_content` directly under `captured_with` and compares `v5_lines` with `v4_lines` byte for byte (`crates/quilltap-harness/tests/danger_routing_equivalence.rs:402-426`, target filter `quilltap::dangerous_content_routing` at `:243-253`). Its corpus (`harness/oracle/fixtures/danger-routing.json`) has two relevant rows:
  - `img-configured-is-the-original-excluded` (original = desk `e0…a2`). This row should already take the new arm at the pin, so the desk leg is probably pinned already. CONFIRM by grepping the regenerated oracle for `already uncensored`.
  - Nothing poses an **own-compatible original**: `e0…a3` "Img Compatible A" and `e0…a5` have `isDangerousCompatible: true` but are never an `originalProfileId`.
- Fix shape:
  1. Add two image cases: `img-own-compatible-stays-put` (original `e0…a3`, Concierge on, Moderated, no desk), and a Locked / off-duty twin to prove the policy gate answers FIRST. These are the silence legs.
  2. Regenerate the oracle `harness/oracle/cases/danger-routing.test.ts`.
  3. That is cheaper than growing the three tier-3 families. The order's "silence leg (a non-compatible original logs the understudy lines instead)" is already present in `img-configured` and `img-scan-compatible`.
- Differential: `danger_routing_equivalence` (`QT_ORACLE_DANGER_ROUTING` + its fixture builder `harness/oracle/fixtures/build-danger-routing-fixture.ts`).
- Files a lane would own: `harness/oracle/fixtures/danger-routing.json`. The test file only if the case schema needs a field.
- Size: S

### D326-T3-1 … D326-T3-6   [NO-ACTION]
- All are recorded as NEGATIVE pins, collapses, or other owners' items, and all were handled at the union.

## P4.D328 — Inventory tools (R-6 excluded)

### D328-RR — the help chat's slate offers the nine `inventory_*` tools   [NO-ACTION — v4 filing candidate]
- v5 site: `crates/quilltap-core/src/services/help_chat/orchestrator.rs:768-772` (`can_manage_inventory: true`, faithful, with the comment).
- v4 site @322a5488e: `lib/services/help-chat/orchestrator.service.ts:237`. `buildTools` gets 11 positionals, so `canManageInventory` is undefined, which means ON. Every call answers `'The inventory tools need a character to act for.'` (`lib/chat/tool-executor.ts:798`).
- Filing text facts: the help chat has no acting character, so the nine tools are offered but always refuse. That wastes tokens and invites failed calls.

### D328-T3-1 / D328-T3-2 — the `RUN_CUSTOM_PREAMBLE` red; sibling catalog carves   [CLOSED]
- `tools/run_custom.rs:113` now carries "…or what they carry, server-side…" (equal to `data.rs:282`). `SIBLING_KEYS_UNTIL_UNION` is gone.

### D328-T3-7 — re-pointing the avatar-refresh rename   [OPEN — folded into D324-T3-33]

### D328-T3-3 / D328-T3-4 / D328-T3-5 / D328-T3-8   [NO-ACTION]
- These are `child-repositories-proxy`, `turnContainersOf` / `byIdCache`, the drain order (R-6, other survey), and the receiver search.

## P4.D330 — the Inventory SPA

### D330-g — `inventory-flow` beat (g) is skipped because the e2e mock LLM has no tool-calling arm   [OPEN]
- Source: the P4.D330 header's OPEN list and the round record (§S.9).
- v5 sites:
  - `apps/web/e2e/inventory-flow.spec.ts:319-330`: `test.skip(true, 'needs a tool-calling arm…')`. The body only does `goto` plus `expect(...).toContainText('Picked up ')`, and **that assertion is not discriminating**. Beat (i) (`:332-341`) plants an `inventory_pick_up` TOOL row in the SAME chat (`SOLO_VOYAGE_CHAT_ID`, planted at `:135`), so (g) would pass on the planted row. It needs its own chat or a distinct object name, plus a real send.
  - The mock is `apps/web/e2e/support/mock-llm.ts` (170 lines, shared by 35 `startMockLlm(` call sites). It speaks OpenAI chat-completions SSE: content deltas, then `finish_reason: 'stop'`. It reads the body only for `stream`, and it has no `tool_calls` arm. `workspace-help-ask-flow.spec.ts:24-31` records the same gap for `help_navigate`.
  - The server side is ready: `crates/quilltap-core/src/model/decoders/chat_completions_sse.rs:19,104,269,397-416` accumulates streamed `delta.tool_calls[].index` and decodes them for OPENAI_COMPATIBLE.
  - `inventory-flow` runs its own mock on port 45346 (`:62`, `:139`) with its own instance on 4348.
- Fix shape (size the arm):
  1. Add `MockLlmOptions.toolCall?: { name: string; arguments: Record<string, unknown> }`, fully opt-in so the other 34 users are unaffected.
  2. When a STREAMING request's body has a non-empty `tools` array and no message with `role: 'tool'` (and optionally the named tool is among `tools[].function.name`), emit one chunk `{choices:[{index:0, delta:{role:'assistant', tool_calls:[{index:0, id:'call_mock_1', type:'function', function:{name, arguments: JSON.stringify(args)}}]}, finish_reason:null}]}`, then `{choices:[{index:0, delta:{}, finish_reason:'tool_calls'}], usage}` and `[DONE]`.
  3. On the follow-up turn (the body contains a `role:'tool'` message), stream the normal text reply.
  4. Non-streaming cheap calls keep today's behaviour.
  5. Then rewrite (g):
     - seed an archetype in the character's reach (a name distinct from (i)'s "unicorn horn");
     - set `canManageInventory` on the seat's character;
     - send a message in a FRESH chat (or a second planted chat);
     - assert the `.qt-chat-wardrobe-notice` "Picked up <distinct name>" and the Pascal line.
  6. Check whether the fixture's OPENAI_COMPATIBLE model reports `toolUse` (provider manifest `provider_manifest/mod.rs:155`). If not, the turn uses text-block tools, and the arm would instead emit the pseudo-tool text marker in `content`.
- Differential: e2e only (the Rust tool path is proven by `inventory_tools_equivalence`).
- Files a lane would own: `apps/web/e2e/support/mock-llm.ts`, `apps/web/e2e/inventory-flow.spec.ts`. It could also re-activate the fifth `workspace-help-ask-flow` beat.
- Size: M (about 60 lines in the mock plus a beat rewrite; run by file alone).

### D330-meta — the `workspace-core-fixtures.json` `_meta.baseline` literal is stale   [OPEN]
- Source: the P4.D330 and P4.D334 headers.
- v5 site:
  - `harness/oracle/cases/workspace-core.ts:629-632` hard-codes `_meta: { …, baseline: 'e646f58b' }`;
  - the emitted `apps/web/src/app/workspace/core/__fixtures__/workspace-core-fixtures.json:2-5` carries the same value;
  - but the corpus was last regenerated at the pin `322a5488e` by `8c1ed46ed` (P4.D330's inventory workspace tab).
- Fix shape: set the literal to `'322a5488e'`, or better, derive it (`git rev-parse --short HEAD` in the v4 cwd, or an env `QT_V4_BASELINE`) so it cannot go stale. Regenerate from the pin; only `_meta` should change (`git diff` shows exactly one line).
- Differential: the SPA corpus spec that reads the fixture (unchanged).
- Files a lane would own: `harness/oracle/cases/workspace-core.ts`, `apps/web/src/app/workspace/core/__fixtures__/workspace-core-fixtures.json`.
- Size: S

### D330-T3-31 — the `withArticle` pair ("an unicorn horn")   [NO-ACTION — v4 filing candidate]
- The client says "an", the server says "a". It is pinned in `inventory-flow.spec.ts:340`.

### D330-T3-28 / T3-29 / T3-30 / T3-32 / T3-35 / T3-36 / T3-37   [NO-ACTION]
- These are recorded classes.

### D330-T3-33 / D330-T3-34 — the `confirm()` collapse; the HANDOFFs   [CLOSED]
- Item 24 landed. The HANDOFFs closed at §S.9.

## P4.D331 — the Log

### D331-T3-35 — the CLI `db schema` domain row `'Log entries': ['log_entries']` is not recorded in a comment   [OPEN]
- Source: P4.D331 Tier 3 item 35. The lane record says "not edited".
- v5 site: `crates/quilltap-cli/src/db_cmd.rs:24-27` (`DB_VERBS`, `"schema"` refused). There is no domain-table comment.
- v4 site @322a5488e: `packages/quilltap/lib/db-commands.js:34` (`'Log entries': ['log_entries']`, from `e4a60ad9e`).
- Fix shape: add a comment beside `"schema"` that names v4's domain map row, for the lane that ships the verb. No Tier R movement.
- Differential: none.
- Files a lane would own: `crates/quilltap-cli/src/db_cmd.rs`.
- Size: S (trivial)

### D331-T3-28 — the help page's "One quirk" sentence (`CHILD_BUFFER_VISIBILITY`) as a standing note   [OPEN]
- Source: P4.D331 Tier 3 item 28. The unifier was to add it to `dogfood-findings.md`'s standing notes.
- v5 site: `docs/developer/porting/dogfood-findings.md` (grepped: no "One quirk" or `CHILD_BUFFER` entry). The vendored `help/the-log.md` stays as v4 wrote it.
- Fix shape: one standing-note paragraph: "help/the-log.md 'One quirk' describes v4's job-child buffering; v5 has no child (a same-turn `log_read` sees a Pascal `log` effect at once — `CHILD_BUFFER_VISIBILITY`); a help-text divergence candidate."
- Files a lane would own: `docs/developer/porting/dogfood-findings.md`.
- Size: S

### D331-T3-34 — `.max(4000)` vs `.slice(0, 4000)` on astral text   [NO-ACTION — v4 filing candidate]
- Measured (status-log ~191288): `create-4000-astral` is accepted at 8,000 UTF-16 units and the door clips it to 2,000 emoji.

### D331-T3-36 / D331-T3-37 — the `log: 'The Log'` category group; the help / DDL HANDOFF   [CLOSED]
- `apps/web/src/app/chat/tools/tool-settings.ts:81` (`log: 'The Log'`). Help is byte-equal (P4.D339).

### D331-T3-26 / T3-27 / T3-29 … T3-33, T3-38; the DDL `"id" TEXT PRIMARY KEY` inaccuracy   [NO-ACTION / CLOSED]
- These are moot, recorded, or handled at the union. The DDL inaccuracy is vendored as v4 wrote it.

## P4.D334 HANDOFF (9) — help-sentence byte-compare (P4.D339's OPEN)

### D334-H9 — `help/pascals-workbench.md` sentences against the UI   [CLOSED (measured) + NO-ACTION for the one mismatch]
- Measured on main. Each sentence below is present in the help and in the SPA source:

| Sentence | SPA source |
|---|---|
| "Tests on one line must all hold; any line that holds opens the door." | `screens/custom-tools/gate-section.ts` |
| "passed on line" | `proving-bench.ts` |
| "(would attempt)" | `proving-bench.ts` |
| "From an object" | `builder-form.ts` |
| "equip it at once" | `inventory-effect-fields.ts` |
| "Insert key…" | `aggregate-key-field.ts` |

- "warns when a prompt …" is prose only, as the order said.
- The KNOWN mismatch is v4-shared. The help says the "*Log line*" kind (`help/pascals-workbench.md:34,90`; `help/the-log.md:37`). The select option reads "Enter a log line" in both v5 (`apps/web/src/app/screens/custom-tools/side-effects-section.ts:208`) and v4 (`components/custom-tools/SideEffectsSection.tsx:220`). The field label is "Log line" in both (v5 `:248,258`; v4 `:268,277`). v5 is faithful; this is a v4 filing candidate at most.
- Fix shape: none. Mark HANDOFF (9) closed in the D334 and D339 headers.

## P4.D336 — interrupts SPA

### D336-multichar — `isMultiCharacterRoom` tests `p.character` over raw participants   [OPEN]
- Source: the round record's "Recorded OPEN (notes)" ("cosmetic").
- v5 site: `apps/web/src/app/screens/salon/salon-conversation.ts:2724-2737`. The filter runs over `chat().participants` with no `type === 'CHARACTER'` filter, and the LLM leg tests `p.character` (the hydrated object). Its consumer is `stop()` at `:4304-4312` (pause-on-stop). The wire type `EnrichedParticipantSummary` (`core/core-contract.ts:2992`) has `type: 'CHARACTER'` and `character: … | null`, but no `characterId`.
- v4 site @322a5488e: `app/salon/[id]/hooks/useParticipants.ts:34-39`:
  - `participantsAsBase = chatParticipants.filter(p => p.type === 'CHARACTER' && (p.characterId || p.character?.id))`;
  - then `:64-68` `isMultiChar` is false on an empty list, else `isMultiCharacterChat(participantsAsBase)`;
  - `lib/chat/turn-manager/utils.ts:176-184` tests `p.characterId && p.controlledBy === 'llm'`;
  - `controlledBy` defaults to `'llm'` when absent (`useParticipants.ts:44`).
- Divergence cases:
  - (a) a seat whose character failed to hydrate (deleted character, `character: null`) still counts as LLM in v4 (it has a `characterId`) but not in v5;
  - (b) a seat with absent `controlledBy` counts as LLM in v4 (via the default) but not in v5.
- Fix shape: build the same base list, `seats.filter(p => p.type === 'CHARACTER' && (charIdOf(p)))` with `controlledBy ?? 'llm'`. Test `charIdOf(p)` for the LLM leg. If the wire has no `characterId`, add it to the contract (MEASURE whether the server's enriched participant emits it) or use `p.character?.id` and record (a). Also move it next to `isSingleCharacterChat` as one `participantsAsBase` computed.
- Differential: unit-only. Add `salon-turn-controls.spec.ts` rows for a null-character LLM seat and an absent `controlledBy`, transcribed from v4's `utils.ts`.
- Files a lane would own: `salon-conversation.ts`; possibly `core/core-contract.ts`.
- Size: S

## P4.D337 — bug 255, the terminal

### D337-T3-24 … D337-T3-32   [NO-ACTION]
- `SCREEN_RESIZE_NO_REFLOW` is ruled (R-D).
- Async parse; migration lines; the snapshot.
- The real captured transcript is a 💸 human-supplied copy.
- No new e2e.
- The final flush's `chat-update` nudge order is a recorded v5 difference.
- The lone-surrogate cut; the standing `ariel_writers_tier3` cell.
- The union's 500 × 200 resize clamp is a RULED divergence.
- Header NOTE: v4 `85c002822` (bugs 267 / 268) rewrites this surface. That belongs to the drift catch-up, not the smalls round.

## P4.D338 — four smalls

### D338-20 — a real-cascade toast beat   [OPEN]
- Source: P4.D338 Tier 2 item 20 (not landed). The source-text guard `crates/quilltap-harness/tests/toast_stacking_guard.rs` pins `.qt-toast-container { z-index: 1000 }` above every other z-index.
- Fix shape: create `apps/web/e2e/toast-stacking-flow.spec.ts` (own instance on 4352, mock 45350 per §R.9). Open a modal whose action toasts without an LLM call, then assert that `document.elementFromPoint(toastCentre)` lies inside `.qt-toast-container`. Candidates are any settings modal save, or the photo save dialog's "saved" toast. Run it by file alone.
- Differential: e2e only.
- Files a lane would own: NEW `apps/web/e2e/toast-stacking-flow.spec.ts`.
- Size: S–M

### D338-22 — a NULL-`storeType` plant   [OPEN]
- Source: P4.D338 Tier 2 item 22 (R-7 MEASURE, not landed). The committed photos pair has no NULL row, so the mutation was an equivalent mutant.
- v5 site: `crates/quilltap-core/src/photos/user_gallery_service.rs:95-99` (`store_type != Some("character")`; NULL is kept). Siblings are `photo_link_summary.rs:69` and `character_gallery_service.rs:74`.
- v4 site @322a5488e: `lib/photos/user-gallery-service.ts:55` (`mountPoint.storeType !== 'character'`). The open question is whether v4's `findEnabled` / `findById` Zod parse (`.default()` applies only to `undefined`) DROPS or THROWS on a NULL `storeType`. If it drops, v4 never lists that store, and v5 would list it.
- Fix shape:
  1. In `cases/photos-routes.test.ts` (or the photos family's case), plant one enabled store with `storeType` NULL and a `photos/` link on a case-private copy, on both sides.
  2. Record v4's behaviour.
  3. Match v5 (filter or keep), or name a both-ways table.
- Differential: `photos_routes_equivalence` (the photos family; `harness/oracle/cases/photos-routes*.test.ts`).
- Files a lane would own: that oracle case and its Rust family, and `photos/user_gallery_service.rs` if behaviour moves.
- Size: S–M

### D338-librarian — `librarian_notifications.rs` logs `mountPoint` as the string `"null"`   [OPEN]
- Source: the P4.D338 header NOTE.
- v5 site: `crates/quilltap-core/src/services/librarian_notifications.rs:116-133`, specifically `:128` `mountPoint = mount_point.unwrap_or("null")`. `None` is recorded as the string `"null"`; `Some("")` is recorded as `""`.
- v4 site @322a5488e: `lib/services/librarian-notifications/writer.ts:219-225`, `logger.debug('[LibrarianNotification] No store name for an announced path; printing the bare path', { context, mountPoint: mountPoint ?? null, path })`. That is JSON `null` for undefined / null, `''` for empty, and `'self'` / `'SELF'` as given.
- Fix shape: emit the field as JSON, either `mountPointJson = %serde_json::to_string(&mount_point).unwrap()` (the harness's `…Json` suffix convention) or an absent field for `None`, whichever the file-log envelope maps to v4's `null`.
- Differential: no family captures this DEBUG today (grep: zero hits in the harness). Add capture to `crates/quilltap-harness/tests/post_office_librarian_equivalence.rs`, which has `ds-self` (`:48,69`) but no `None` row, and to its oracle case, with a `ds-null` row and a `ds-self` row.
- Files a lane would own: `services/librarian_notifications.rs`, `post_office_librarian_equivalence.rs`, and its oracle case and fixture.
- Size: S

### D338-T3-24 — `/photos` embeds the query before learning the gallery is empty   [OPEN (pre-existing; recorded)]
- v5 site: `crates/quilltap-core/src/api/photos.rs:183-206`. `needs_query_embedding(q)` triggers the embedding call before `list_user_gallery` runs (`photos/user_gallery_service.rs:176-177`).
- v4 site @322a5488e: `lib/photos/user-gallery-service.ts:325-326` returns `{entries:[],total:0,hasMore:false}` when `allPhotoLinks.length === 0`, BEFORE `generateEmbeddingForUser` (`:349` region).
- Effect: an empty gallery plus a query spends one embedding call, and logs or fails on a broken provider, where v4 does neither.
- Fix shape: after album resolution, run a cheap mount-index read for "any `photos/` link in the scoped mount points"; when there are none, skip the embedding and answer the empty page. Keep `Listing user gallery` where v4 logs it.
- Differential: the photos routes family, with an `empty_gallery_with_query` row and a failing-embedding seam. v4 makes no call, and the row must show no embedding.
- Files a lane would own: `api/photos.rs`, `photos/user_gallery_service.rs`.
- Size: S

### D338-T3-25 — the `handleOpenDocument` new-file residue (R-13)   [NO-ACTION — v4 filing candidate]

### D338-T3-23 / T3-26 / T3-27 / T3-28   [NO-ACTION / CLOSED]

## P4.D339 — `help/` 135, `docs/v4/`, theme

### D339-drift — `help/ariel.md` and `system-backup-restore.md` move past the baseline (`c50cdddc6`, `85c002822`, `cc5f2347c`)   [NO-ACTION]
- This belongs to the 46-commit drift catch-up, not the smalls round.

### D339-T3-15 — the Madman's Box icon set and loader   [NO-ACTION]
- The theme set is "inert by ruling" per the header.

### D339-T3-14 / T3-16 … T3-20   [NO-ACTION / CLOSED]

## P4.D340 — the Inventory title lint

### D340-name — fixture row `doc-keyed-naming` is a REJECTION in v4   [OPEN]
- Source: the P4.D340 header NOTE.
- v5 site: `harness/oracle/fixtures/inventory-title-lint.json:2389-2399`. It writes `Inventory/tape.json` titled "Friday's Yellow Tape Measure", a title Friday's instance already carries (`:203`, `:316`), so v4 refuses it as a shadow or collision. It is hand-authored: no generator names it, and grep found no other reference in the harness or crates.
- Fix shape: rename it to `doc-keyed-title-already-carried-refused` (or similar). Grep the Rust family and the NDJSON consumers for the old name (none found) and regenerate the oracle so the row id moves both sides. Optionally add a true accepting `doc-keyed-naming` twin with a fresh title.
- Differential: `inventory_title_lint_equivalence` (`QT_ORACLE_INV_TITLE_LINT`, `QT_FIXTURE_ITL_MAIN/MOUNT`).
- Files a lane would own: `harness/oracle/fixtures/inventory-title-lint.json`.
- Size: S

### D340-T2-2 — `cases/inventory-title-lint-surfaces.test.ts` (route-level differential rows)   [OPEN]
- Source: P4.D340 Tier 2 item 2, "NOT RUN" (status-log ~191835, 191852). The R-8 surfaces are pinned only at the mapping layer in Rust.
- Fix shape: write the jest case over the ITL pair. Write a shadowing `Inventory/*.json` through v4's REAL route handler and doc-tool handlers, stubbing v4's census per case with `jest.doMock('@/lib/pascal/tool-gate-keys', …)` and un-mocking after. Read the result with the same Rust binary under `QT_ORACLE_INV_TITLE_LINT_SURFACES`.
- Differential: NEW rows in `inventory_title_lint_equivalence`.
- Files a lane would own: NEW `harness/oracle/cases/inventory-title-lint-surfaces.test.ts`, the Rust family.
- Size: M

### D340-shas — fold `INVENTORY_ROUND_SHAS` into one `322a5488e`-round marker census   [OPEN]
- Source: the P4.D340 lane record's HANDOFF → unifier, not done.
- v5 site: `crates/quilltap-harness/tests/no_wardrobe_lane_remove_markers_left.rs:68,148` and `no_memory_lane_remove_markers_left.rs:68` each carry `const INVENTORY_ROUND_SHAS: [&str; 1] = ["7263105c6"]`. Also, `LANE-REMOVE (` (with a space) escapes both guards' `LANE-REMOVE(` match. All 22 remaining `LANE-REMOVE` mentions in source are historical prose.
- Fix shape: write one `no_inventory_round_lane_remove_markers_left` guard, or widen the match to `LANE-REMOVE\s*\(`, and delete the two fenced constants.
- Files a lane would own: the two guard files.
- Size: S

### D340-keyorder — integer-like gate keys: V8 `Object.keys` order beyond the census   [OPEN — measure]
- Source: the round record's wire 18. The census reorders: `crates/quilltap-core/src/pascal/tool_gate_keys.rs:175-189`. The record says "the same ordering gap may exist wherever `ToolGateGroup` keys iterate (OPEN)".
- v5 sites to measure: the iterations of `ToolGateGroup` (`pascal/custom_tool_types.rs:837` onward) in `pascal/tool_gate.rs` (evaluation order, i.e. which failing reason is reported first) and `pascal/tool_vocabulary.rs`.
- v4 site @322a5488e: every `Object.entries(gate.metadata ?? {})` / `Object.keys(group[subject])` walk puts integer-like keys first, ascending.
- Fix shape: add a corpus row with a gate `{ "lamp": …, "42": … }` to the definition and execution corpora. Where the order is observable (first-failing reason, the describer's text), apply one shared `v8_own_keys_order` helper; the census already has the logic, so lift it to a shared fn.
- Differential: `tool_definitions_equivalence` / `pascal_*_execution` corpora (the 710-row definition corpus), with a new integer-like-key row.
- Size: S–M

### D340-T2-1 — the scratch union preview   [CLOSED (superseded by the real union)]

### D340-T3-1 … T3-5   [NO-ACTION]
- T3-1 is closed at wire 18. T3-4 (the Scriptorium's 500 for a refused title) is a v4 filing candidate.

---

## The `83334b9a5` round's "SPA lows" (phase-4.md:7958; status-log ~184932; the orders' headers)

The phrase "the SPA lows" refers to the low or cosmetic OPEN items in the P4.D300 and P4.D302 headers. None of the files has changed since 2026-10-15 (`git log` on each), so all four are still as recorded.

### SPA-low-1 — `createMemory` returns a non-optional `action` it admits may be absent   [OPEN]
- v5 site: `apps/web/src/app/memory/memory.api.ts:93-102` (`action: data['action'] as AdmissionActionKind`), `core/core-contract.ts:6557-6560` (`MemoryCreateReply.action: AdmissionActionKind`, required), and the consumer `memory/memory-editor.ts:182-190`, whose comment says "An absent action is a pre-round server".
- Fix shape: make it `action?: AdmissionActionKind` in the contract (or `AdmissionActionKind | undefined` in the api), and read it as `data['action'] as AdmissionActionKind | undefined`. The editor already handles `undefined`.
- Differential: unit-only (the existing `memory-editor.spec.ts` toast rows).
- Size: S

### SPA-low-2 — `MemorySettingsDocument.config` is exported as a `WritableSignal` with no external writer   [OPEN]
- v5 site: `apps/web/src/app/screens/settings/memory/memory-settings-document.ts:23` (interface `config: WritableSignal<T>`), set internally at `:61` and `:74`. No consumer calls `doc.config.set` (`memory-extraction-grain-card.ts:181,233` set its OWN `this.config`).
- Fix shape: expose it as `Signal<T>` (`config.asReadonly()`) on the interface.
- Size: S

### SPA-low-3 — the provenance `@if` block's leading collapsed space   [OPEN (cosmetic)]
- v5 site: `apps/web/src/app/chat/sidebar/chat-scenario-control.ts:126-131`. The `@if (provenance(); as p) {` block's leading whitespace collapses into a rendered leading space before "From:". The e2e had to switch to a substring locator because of it.
- v4 site @322a5488e: `ChatScenarioControl` `:411-428` (per the comment).
- Fix shape: put `From:` immediately after the `<div …>` with no leading whitespace (or trim it), then restore `getByText(/^From:/)` in the spec.
- Size: S

### SPA-low-4 — the "four jsdom files transcribed" wording   [NO-ACTION]
- This is a lane-record claim in P4.D302 (the helper cases are oracle-RECORDED, 113 rows). It is a doc correction only, made in the P4.D302 header if at all.

Also in the same "Recorded, not fixed" list are the D301 toast duration and the duplicated `STREAM_FAILURE_ERROR` literal (P4.D301 header). These are SPA / server smalls, but they are not part of this slice's orders.
