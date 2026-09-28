# Survey — v4 `04d6c9d52` "Give characters joining a running chat their outfit avatar" (4.10.0-dev.99)

Dated 2026-09-28. Read-only survey. v4 at `97b25fc53` (clean); oracle baseline `acadcc7cd`; v5 `main` at `ef29a058e`.
All v4 line numbers are post-commit at `04d6c9d52`.

## A. v4, hunk by hunk

Files: `app/api/v1/chats/[id]/actions/participants.ts` (+55), `__tests__/unit/app/api/v1/chats/[id]/actions/participants-join-avatar.test.ts` (+175, new), `…/participants-impersonation.test.ts` (+4), `help/chat-settings-ai-services.md` (+3/-2), `docs/CHANGELOG.md` (+10), and version stamps (README, package.json, package-lock, packages/quilltap/package.json). **The only production hunk is `participants.ts`.** The ledger's reading is correct, with the caveats below.

### A.1 Import (`:33`)
`import { triggerAvatarGenerationIfEnabled } from '@/lib/wardrobe/avatar-generation';`

### A.2 The new function, `refreshAvatarForArrivingCharacter` (`:251-284`), verbatim shape
```ts
async function refreshAvatarForArrivingCharacter(chatId, characterId, userId, repos): Promise<void> {
  try {
    const equippedSlots = await repos.chats.getEquippedOutfitForCharacter(chatId, characterId);
    if (!equippedSlots) {
      logger.debug('[Chats v1] No equipped outfit for arriving character, avatar left as-is', { chatId, characterId });
      return;
    }
    await triggerAvatarGenerationIfEnabled(repos, {
      userId, chatId, characterId,
      callerContext: '[Chats v1] participant-join',
    });
    logger.debug('[Chats v1] Avatar refresh requested for arriving character', { chatId, characterId });
  } catch (error) {
    logger.warn('[Chats v1] Failed to request avatar refresh for arriving character', {
      chatId, characterId,
      error: error instanceof Error ? error.message : String(error),
    });
  }
}
```
- Three log lines: DEBUG skip (fields `chatId, characterId`), DEBUG requested (`chatId, characterId`), WARN failed (`chatId, characterId, error`). Field order as written.
- Params passed to the trigger: exactly `{userId, chatId, characterId, callerContext}`. No `imageProfileIdOverride`, no `equippedSlotsOverride`, no `force`. So the job payload carries no `force` key, and the configuration cache serves it.
- One try/catch covers both the read and the trigger. The function returns `void`, so it can never fail the join.

### A.3 Call sites
**Reactivate path** (`:380-384`), inside `if (removedParticipant) { … }`, which itself sits inside `if (validatedData.type === 'CHARACTER' && validatedData.characterId)`.
Order in the branch:
1. `updateParticipant`
2. `serverError` if `!updatedChat`
3. find `reactivatedParticipant`
4. enrich
5. `characters.findById`
6. INFO `[Chats v1] Participant reactivated`
7. `postHostAddAnnouncement` + `compileIdentityStackForParticipant` (only if character && participant)
8. the outfit re-apply, gated `validatedData.outfitSelection && reactivatedParticipant?.characterId`
9. **NEW:**
```ts
      // Whether or not the outfit was re-applied, the returning character's
      // avatar should match what they now have on.
      if (reactivatedParticipant?.characterId) {
        await refreshAvatarForArrivingCharacter(chatId, reactivatedParticipant.characterId, user.id, repos);
      }
```
10. `return NextResponse.json({participant, chat: updatedChat}, {status: 200})`.

The condition is just `reactivatedParticipant?.characterId`. It is **not** gated on `outfitSelection`, and it does **not** require the character row to exist. It is awaited.

**Add path** (`:450-460`). It runs after `handleAddParticipant`, enrich, `characters.findById`, INFO `Participant added`, the add announcement, the identity-stack compile and the join-scenario announcement. It sits inside the existing `if (validatedData.characterId) {` block, immediately after `await applyOutfitForAddedParticipant(...)`:
```ts
    await refreshAvatarForArrivingCharacter(chatId, validatedData.characterId, user.id, repos);
```
It is awaited, runs before the 201 return, and comes strictly AFTER the dress (the test pins `invocationCallOrder`). `characterId` is `z.uuid()` and required by `addParticipantSchema`, so the guard is always true here.

The **chat-PUT bag** (`processChatUpdates` `addParticipant`) is NOT touched. It still never dresses the character and never requests an avatar.

### A.4 `getEquippedOutfitForCharacter` (`lib/database/repositories/chats.repository.ts:797`)
- It returns `state[characterId]` from `getEquippedOutfit` (`:772`), whose entries are normalized to all five slots. It returns `null` when the chat is absent, `equippedOutfit` is null, or the character has no key.
- Both functions are **fallback-mode `safeQuery`** (fallback `null`). On a DB error they log ERROR `Failed to get equipped outfit[ for character]` and return `null`. They rethrow only inside a strict scope (`strictRepositoryFailuresActive`, bug 79).
- **"No equipped slots" is `!equippedSlots`, a null/absent-key test only.** An entry whose slots are all empty (mode `none` writes `{top:[],bottom:[],footwear:[],accessories:[],hair:[]}`) is an object, so it is truthy and the trigger FIRES.

### A.5 `triggerAvatarGenerationIfEnabled` (`lib/wardrobe/avatar-generation.ts:140-164`)
Gates, in order:
1. `repos.chats.findById(chatId)`. If `!chat?.avatarGenerationEnabled`, return silently.
2. If `chat.chatType === 'autonomous'`, return silently.
3. `await triggerAvatarGeneration(repos, params)`, discarding its result.
4. Catch → WARN `Failed to enqueue avatar generation after outfit change` `{context: callerContext, chatId, characterId, error}`.

`triggerAvatarGeneration` (`:51-132`):
- Reads the chat again. If missing, returns `{queued:false, reason:'chat-not-found'}`.
- Resolves the profile: override (WARN `Avatar generation override profile not found, falling back` `{context, chatId, imageProfileIdOverride}`) → `chat.imageProfileId` → `imageProfiles.findAll().find(isDefault)`. If none resolves, returns `no-image-profile`.
- Calls `enqueueCharacterAvatarGeneration(userId, {chatId, characterId, imageProfileId, …equippedSlotsOverride?, …force?})`.
- Its own catch → WARN `Failed to enqueue avatar generation` `{context: callerContext, chatId, characterId, error}` and returns `{queued:false, reason:'error'}`.
- **It never throws.**

`enqueueCharacterAvatarGeneration` (`lib/background-jobs/queue-service.ts:1185`):
- Dedups on a PENDING/PROCESSING `CHARACTER_AVATAR_GENERATION` job for the same chat + character. On a hit it logs INFO `[CharacterAvatar] Reusing existing pending job` `{context:'background-jobs.queue', chatId, characterId, existingJobId}`.
- Otherwise it calls `enqueueJob` (priority `?? 0`, maxAttempts 3) and logs INFO `[CharacterAvatar] Avatar generation job enqueued` `{context, chatId, characterId, jobId}`.

`callerContext` is used **only** as the `context` field of those three WARN lines. It is not written into the job.

### A.6 The tests (oracle shapes)
`participants-join-avatar.test.ts` mocks `triggerAvatarGenerationIfEnabled`, `applyOutfitSelections`, `handleAddParticipant` and the repos. It has four cases:
1. **Fresh add** → 201. `applyOutfitSelections` was called. The trigger was called once, with `objectContaining({userId, chatId, characterId: charB})`. Dress happens before trigger (by `invocationCallOrder`).
2. **Reactivate of a `removed` seat with no `outfitSelection`** → 200. `applyOutfitSelections` was NOT called, and the trigger WAS called (`chatId, characterId`).
3. **Equipped returns `null`** → the trigger was NOT called.
4. **The trigger throws** (mocked) → still 201.

`participants-impersonation.test.ts` gains only a `jest.mock('@/lib/wardrobe/avatar-generation', …)` stub, so the new import is inert in that file. Not a behaviour case.

The tests do not assert `callerContext`, any log line, or anything about the real gates or the cache. They assert only the call and the non-failure.

### A.7 What the message claims that the hunks do NOT do
- "gated on the chat's avatar-generation setting" and "the configuration cache answers first … rebound, not regenerated" are claims about **pre-existing** code (the trigger's gate; the job handler's `7fbf8a55b` cache). This commit adds neither. It relies on both.
- "the same way chat-open does": chat-open (`app/api/v1/chats/route.ts:617`, `callerContext '[Chats v1] chat-open'`) triggers only for the opening cast after the opening-outfit whisper. The join path posts **no** outfit whisper. Only the avatar leg is shared.
- The "Avatar refresh requested" DEBUG fires **whenever the equipped read is non-null**, even when the trigger then did nothing (avatar generation off, autonomous room, no image profile, dedup hit). "Requested" does not mean "queued".
- The WARN catch is **unreachable in production outside a strict scope.** The read is a fallback `safeQuery` and the trigger never throws. Only the mocked test reaches it.
- The trigger ignores `controlledBy`: a **user-controlled** joiner also gets a job (the help text for chat-open says "LLM-controlled").

## B. v5 on main

### B.1 `crates/quilltap-core/src/api/chat_cast.rs`
- `chat_add_participant(db, user_id, chat_id, data, outfit_runner)` at **:135-267**. The fresh-add branch runs handle_add → enrich → character read → add announcement → `compile_stack_best_effort` → join-scenario announcement, then `apply_outfit_for_added_participant(...)` at **:256-264** (unconditional, since `character_id` is a required `String`), then `ok(...)` at :266.
  - **New call goes after :264.**
- `reactivate_participant(db, user_id, chat_id, chat, removed, data, outfit_runner)` at **:271-384**. It runs the patch write → re-read (:320) → find `reactivated` (:326) → enrich → character read (:337-346) → announcement + compile if both (:348-365) → outfit re-apply only `if data.outfit_selection.is_some()` (:369-381) → `ok(...)` at :383.
  - **New call goes between :381 and :383**, gated `reactivated.as_ref().and_then(|p| s(p,"characterId"))` and NOT on `outfit_selection`.
- **userId source:** `api/engine.rs:1367` passes `SINGLE_USER_ID`, and the verb already threads `user_id` through to both branches. No boundary change is needed.
- Pre-existing and not this commit: v5 emits neither of v4's INFO lines, `[Chats v1] Participant added` nor `[Chats v1] Participant reactivated`. That is a possible rider.

### B.2 `services/chat_participants.rs`
- `apply_outfit_for_added_participant` at **:1351**. It is best-effort and swallows its own failures.
- Pre-existing divergence: its failure line is `tracing::warn!` (:1397), while v4 logs `logger.error` with the same text. A rider candidate, not this commit.
- It returns `()`, so the new helper cannot learn whether the dress succeeded. v4 cannot either; it re-reads.
- **Home for the new helper:** alongside `apply_outfit_for_added_participant` in `services/chat_participants.rs`, or private in `chat_cast.rs`. v4 keeps it file-private next to the apply helper.

### B.3 `services/avatar_generation.rs` (170 lines)
- `pub async fn trigger_avatar_generation_if_enabled(db: &Db, params: &AvatarGenerationParams)` at **:151**.
- Gates match v4's order: chat read (Err → silent return), `avatarGenerationEnabled != Some(true)` → return, `chatType == "autonomous"` → return, then `let _ = trigger_avatar_generation(...)`.
- **`AvatarGenerationParams` (:26) has NO `caller_context` field.** Its fields are `user_id, chat_id, character_id, image_profile_id_override, equipped_slots_override, force`, all public, with no `Default`.
- Construction sites:
  - `api/chat_outfits.rs:740` (outfit-equip)
  - `api/chat_outfits.rs:880` (regenerate-avatar)
  - `services/chat_create.rs:2383` (chat-open)
  - harness `image_generation_tier3_equivalence.rs:1141`
- **v5 has NONE of v4's avatar-trigger log lines.** All of these are absent:
  - WARN `Avatar generation override profile not found, falling back` (comment only, :101)
  - WARN `Failed to enqueue avatar generation`
  - WARN `Failed to enqueue avatar generation after outfit change` (v5 swallows silently at :155)
  - INFO `[CharacterAvatar] Reusing existing pending job` and `[CharacterAvatar] Avatar generation job enqueued` (`services/queue_service.rs:578-638`, silent)
  - Grep for any of those strings across `crates/` → zero hits.
- This is pre-existing. The new call site's `'[Chats v1] participant-join'` context string has **no v5 consumer** unless a rider adds `caller_context` plus those lines. Adding a required field touches the four sites above (see the memory note "a required field on a shared struct crosses lane ownership").

### B.4 The equipped-outfit read
- `db/chats_outfits.rs:124` `ChatOutfitsRepository::get_equipped_outfit_for_character(chat_id, character_id) -> Result<Option<Value>, DbError>`, over `get_equipped_outfit` (:112, normalized).
- **Its `Err` propagates, where v4 falls back to `null` after an ERROR line.** A faithful port maps `Err` to the "no outfit" arm, logging ERROR `Failed to get equipped outfit for character`, or at least never lets it reach the WARN arm.
- It is used by `chat_create.rs:2326`, `character_avatar_job.rs:280`, the wardrobe tools, `build_context.rs`, `outfit_selections.rs` and `story_background_job.rs`.
- The chat-open precedent `chat_create.rs::post_opening_outfit_and_avatar` (:2311-2394) already returns early on `None`, then calls the trigger with `SINGLE_USER_ID` and `force: false`. That is the template.

### B.5 The avatar configuration cache
- `services/character_avatar_job.rs::handle_character_avatar_generation` (:172).
- Step 7b (:325-413): `avatar_cache::derive_avatar_cache_keys` (:358) → `avatar_cache::lookup_cached_avatar` (:368). On a HIT it calls `bind_avatar_to_chat` and logs INFO `[CharacterAvatar] Reused cached avatar for this configuration` (:413), with no provider call. `force` bypasses the cache.
- The job handler is unchanged by this commit.

### B.6 Families
| Family | Recipe (header) | Relevance |
|---|---|---|
| `crates/quilltap-harness/tests/chat_cast_routes_equivalence.rs` ↔ `harness/oracle/cases/chat-cast-routes.test.ts` | `QT_ORACLE_CHAT_CAST=/tmp/oracle-chat-cast.ndjson`; oracle `QT_FIXTURE_CAST_MAIN/MOUNT=…/crates/quilltap-web/tests/fixtures/chat-cast-{main,mount}.db QT_ORACLE_OUT=/tmp/oracle-chat-cast.ndjson TZ=UTC npx jest … -- chat-cast-routes` (staged to `/tmp/qt-cast-oracle`), fixture spec `harness/oracle/fixtures/chat-cast.json`, builder `build-chat-cast-fixture.ts` | **The home.** It drives the real add-participant route. It has 12 add cases: fresh add on `CHAT_MAIN`; six on `CHAT_QUIET` (explicit fields, history access, outfit none/manual/default ×2, user-controlled, stale profile); and two reactivate cases on `CHAT_MAIN`. **`castTables` dumps only `chat` + `messages` for add cases, NOT `jobs`**, so the new enqueue is invisible there today. `readJobs()` / `dump_jobs` (:382) exist but are only used by the two toggle cases. |
| `outfit_llm_choose_tier3_equivalence.rs` ↔ `chat-dialogs-llm-choose-tier3.test.ts` | `QT_ORACLE_LLM_CHOOSE=/tmp/oracle-llm-choose-tier3.ndjson`; fixture `crates/quilltap-web/tests/fixtures/chat-dialogs-{main,mount}.db` | It drives real add-participant with `llm_choose`. The `chat-dialogs` builder sets no `avatarGenerationEnabled`, so the trigger returns at its first gate and no job is written. It does not dump jobs. **Neutral**, but it re-runs at the target, since v4's route now makes one extra chat read. |
| `chats_participants_tier2_equivalence.rs` | repo-level ops | Unaffected (no route). |
| `avatar_job_tier3_equivalence.rs` ↔ `avatar-job.json` | `QT_ORACLE_AVATAR`, `QT_FIXTURE_AVATAR_MAIN/MOUNT` | The handler only. It already has 8 `cache_*` cases (`cache_hit_second_run`, `cache_force_rerolls`, …). Unchanged by this commit. It is the existing proof that an unforced job rebinds. |
| `apps/web/e2e/salon-cast-flow.spec.ts` | Playwright | It adds and removes participants. Check whether its chat has `avatarGenerationEnabled`. If so, a job row is now enqueued (the job pump would then run it against the e2e image stub). Probably inert. |

There is **no** tier-3 family for the trigger, and none is needed: the trigger path has no model call. The move is tier 2.

## C. Families and fixtures that move
- **`chat_cast_routes_equivalence`: MOVES in substance but is BLIND today.**
  - At `04d6c9d52`, every add case on `CHAT_QUIET` (`avatarGenerationEnabled: true`, and the fixture's single image profile is `isDefault: true`) will enqueue a `CHARACTER_AVATAR_GENERATION` job in v4. That includes `add_outfit_none` (empty slots are truthy) and `add_user_controlled`.
  - Because add cases dump no `jobs`, the family stays green on unported main. It is **not a red-first family as-is.**
  - The lane must widen the add dump with `jobs` on both sides; the Rust mirror is `dump_jobs` (:382). Then it goes RED on unported v5 for exactly the `CHAT_QUIET` adds.
  - `CHAT_MAIN` adds and reactivates stay job-free (generation disabled). The fixture seeds no `equippedOutfit`, so a reactivate without an outfit hits the "no equipped outfit" DEBUG arm on real data.
  - New arms needed:
    - (a) reactivate in a generation-enabled chat **with no outfitSelection but a prior equipped entry**. This is the "fires even when not re-applied" arm.
    - (b) reactivate in an enabled chat with no prior entry → no job.
    - (c) an autonomous room → no job.
    - (d) a dedup case (a pending job already exists → no second row).
    - (e) enabled + no image profile. Optional; it needs the default profile removed.
  - Build these by **in-case repository seeding** (the `seededUpdateCase` precedent: `setParticipantStatus`/`updateParticipant` to `removed`, `setEquippedOutfit`, `chats.update({chatType:'autonomous'})` or `{avatarGenerationEnabled:true}`, a pre-enqueued job). That way the committed fixture need not be rebuilt.
- **The fixture rule (`f7f3d7bf0` drops `chats.renderedMarkdown`):**
  - Any rebuild of `chat-cast-{main,mount}.db` (or `chat-dialogs-*`) must run the builder at the **`acadcc7cd`** pinned worktree, because a fixture built at ≥ `f7f3d7bf0` has no `renderedMarkdown` and v5 main cannot read it. Only the ORACLE runs at `04d6c9d52`.
  - Affected if rebuilt: `chat_cast_routes_equivalence` (chat-cast pair; builder `harness/oracle/fixtures/build-chat-cast-fixture.ts`) and `outfit_llm_choose_tier3_equivalence` (chat-dialogs pair). Avoid rebuilding both if in-case seeding suffices; that is recommended.
- **Measure before attributing:** the oracle at `04d6c9d52` also carries `f7f3d7bf0`/`9ff4bbd8e`/`c3eefa752`/`12c336fad`/`39bc98ffc`, all unported. `f7f3d7bf0` changes message/transcript rendering, and the cast family dumps `messages` (Host announcements). Regenerate at `acadcc7cd` first as a control (green). Then regenerate at `04d6c9d52` and attribute every delta. Any non-job delta belongs to another ledger row, not this lane. If `f7f3d7bf0` moves the `messages` dump, the order must pin to a stack or a waiver.

## D. Traps
1. **The reactivate arm fires with NO outfit re-apply.** Gate on `characterId`, not on `outfit_selection`. It still does nothing unless an equipped entry survives from before the removal (removal does not clear `equippedOutfit`; verify the v5 remove path matches). The v4 test's case 2 is exactly this.
2. **"No equipped slots" is null/absent only.** Mode `none` (all-empty slots) and `default` for a character with an empty vault (an entry is still written) both FIRE. Do not port it as "any slot non-empty".
3. **The `Err` shape.** v5's read returns `Result`. v4's is a fallback `safeQuery` (ERROR line, then `null`, then the DEBUG skip line). Do not route `Err` into the WARN arm and claim v4 parity. The WARN is reachable in v4 only under a strict scope or a mock. Pin what is reachable, and record the WARN as a mock-only arm (the P4.D219/P4.113 lesson).
4. **"Requested" DEBUG is not "queued".** It logs after the trigger returns regardless of gates. Pin it on a generation-DISABLED chat too (the `CHAT_MAIN` add), where it still fires and no job is written.
5. **Cache hit = rebind, zero generations.** A proof that the arrival costs nothing must show the enqueued job carries **no `force` key**, and that running it on a pre-cached configuration writes no new `files` row and makes no provider call. `avatar_job_tier3`'s `cache_hit_second_run` already proves the handler half. The lane only needs the payload pin (the `jobs` dump shows `payload` without `force`), plus optionally a v5-only composed test (join → drain one job with a canned image provider that panics if called → assert the `characterAvatars` rebind). There is no new oracle work for the handler.
6. **The new enqueue on the add path moves every family that counts jobs after an add** into a generation-enabled chat: `chat_cast` (once widened), possibly `salon-cast-flow` e2e, and any wire test that counts `background_jobs` after `chatAddParticipant`. The dedup (pending job per chat + character) means a second add or reactivate of the same character in a tick reuses the row.
7. **userId.** The v5 cast verb carries `user_id` (`SINGLE_USER_ID` from `engine.rs:1367`). Pass it through; do not hard-code a new constant.
8. **Autonomous rooms and generation-disabled chats** are skipped inside the existing trigger. Do not re-check them in the helper, because v4 does not (and the "requested" DEBUG still fires).
9. **Order.** The avatar comes after the dress on the add path, and after the optional re-dress on reactivate, both before the response. v5's `apply_outfit_for_added_participant` is awaited, so placement is enough. Pin the order the way v4 pins it: job row present and `equippedOutfit` written, in a single post-state.
10. **`callerContext` has no v5 home.** Either leave it as a recorded divergence (none of v4's context-carrying WARNs exist in v5), or take a rider that adds `caller_context: &'static str` to `AvatarGenerationParams` plus the five absent lines. That touches four construction sites; the rider is separable.
11. **The chat-PUT bag's add is unchanged in v4.** Do not add the refresh to `api::salon::chat_update`. `ENTRANCE_PAIRS` already drops `equippedOutfit`; if the lane puts `jobs` into the paired dumps, drop `jobs` from the pair comparison as well.

## E. Size, tier, rider duty
**Size: SMALL.** It is about 40 lines of core: one helper plus two call sites, mirroring `post_opening_outfit_and_avatar`'s early-return-then-trigger shape. On the harness side it is about 150–250 lines: `jobs` added to the add dumps on both sides, 4–5 in-case-seeded arms, capture pins for the two DEBUG lines (and the Err→skip leg), and a no-`force` payload pin. **Recommended tier: Sonnet** for the lane, with an explicit order covering traps 1–5 and 11. Escalate to Opus only if the `caller_context` + missing-log rider (trap 10) is folded in, since it crosses four construction sites and the queue-service log lines.

### Rider — help pages (`~/source/quilltap-server/help` @ `97b25fc53` vs v5 `help/`)
Both trees have 129 files; there are no adds or removes. 13 files differ. For every one, v5's md5 equals v4's `acadcc7cd` copy, so the delta is purely the drift range.

| file | v4 HEAD md5 | v5 md5 | owning commit(s) |
|---|---|---|---|
| chat-settings-ai-services.md | e78239a84b1d1d31a3557d803c495679 | adf14a196bad5c89bb27f1794a5845fc | **04d6c9d52 (this lane)** |
| character-optimizer.md | 38d7af422ef2dbdcd58afc5f6f673ba0 | 86284776389ebe9a4e89e5b85fd1c1a3 | c3eefa752 |
| character-system-prompts.md | a2a612af3d2d3868f506be2ccf317aca | cb6fa815ee9c56fe6db02361c51193ae | c3eefa752 |
| prompts.md | ac27eb0b91aa8bb7996541f8fe2fd734 | c9361222523089d54981adb7157f7029 | c3eefa752 |
| chat-settings.md | ad920495d06a39c27d20176ad891e557 | ff22cf1a84c21379a8a175c6d035d138 | f7f3d7bf0 |
| chats.md | 64a08e8003f40064cea4bc67af2d8571 | cf743179047be8c14a6f906f596f087d | f7f3d7bf0 |
| data-retention.md | 47f556da8bfc4607aed5abe46e14da9a | b4f80dbd64b93f7356d59f8822433221 | f7f3d7bf0 |
| embedding-profiles.md | 9101de913fd1536553e7efe5ea328495 | c92aeaaa464b899ec06cee78970d682e | f7f3d7bf0 |
| scriptorium.md | b72a0289229a1a39bbef482f0ef1250e | 76cca6eeefb7518e6f05e9db7197330c | f7f3d7bf0 |
| post-office.md | 8ef482446e867d3bfc3bcf7ee8495418 | 4320d67ebeb1b7bd32dbbc4a2c8b904f | 39bc98ffc, 12c336fad |
| wardrobe.md | 0cfac20487a3daa111118143eb4e50e4 | dc424ddc07094abe95a081207eba11cc | 9ff4bbd8e |
| story-backgrounds.md | 6b6c25c1cb426f1f35b3d2c8006ecdb4 | 9399cf1760152fc7993482c6d4cf614e | 97b25fc53 |
| the-concierge.md | 6b2935820ba412aed5431d4680e6bf1b | 0b7282972f1cccc3ff0eb778c6f6443e | 97b25fc53 |

This lane's own page is `chat-settings-ai-services.md`. Byte-copy it at `04d6c9d52` (or at HEAD, since no later commit touches it) and re-run `help_tree_equivalence` at that pin. The other 12 belong to their own rows. If one lane takes the whole re-vendor, `help_tree_equivalence` must run at `97b25fc53`, and the embedded-count literals stay at 129 (no adds).

### Rider — `docs/v4` mirror (v4 `docs/` moved in `acadcc7cd..97b25fc53`)
- `docs/CHANGELOG.md`: touched by all 7 commits. The mirror `docs/v4/CHANGELOG.md` matches NEITHER `acadcc7cd` nor HEAD, nor any of the last 40 CHANGELOG revisions, so it is older or hand-diverged. Refresh it at HEAD.
- `docs/developer/API.md`: `f7f3d7bf0`, `12c336fad`, `39bc98ffc`. The mirror is at the baseline.
- `docs/developer/DDL.md`: `f7f3d7bf0`. The mirror is at the baseline.
- `docs/developer/features/complete/db-size-reduction-spec.md`: `f7f3d7bf0`. The mirror is at the baseline.
- This commit (`04d6c9d52`) touches only `docs/CHANGELOG.md` under `docs/`.

## Summary (10 lines)
1. The only production hunk is `participants.ts`: a file-private `refreshAvatarForArrivingCharacter` (null-check read → trigger with `callerContext '[Chats v1] participant-join'` → DEBUG, catch → WARN), called after the dress on add and unconditionally on `characterId` on reactivate, both awaited.
2. "No equipped slots" is `!equippedSlots`, null or absent key only. All-empty slots (mode `none`) still fire. The fixture seeds no `equippedOutfit`, so a reactivate without an outfit hits the skip on real data.
3. The WARN catch is unreachable in v4 production: the read is a fallback `safeQuery` and the trigger never throws. Only the mocked test reaches it. The "requested" DEBUG fires even when the gates then skip.
4. v5 insertion points: `chat_cast.rs` after :264 (add) and between :381/:383 (reactivate). `user_id` is already threaded (`SINGLE_USER_ID`). `chat_create.rs:2311-2394` is the template.
5. v5's `get_equipped_outfit_for_character` returns `Result`; map `Err` to the v4 fallback, not the WARN.
6. `chat_cast_routes_equivalence` is the home, but it is BLIND: add cases dump no `jobs`. Widening it makes the `CHAT_QUIET` adds (generation on, default image profile) red-first. The new arms should be in-case-seeded so no fixture rebuild (and no `renderedMarkdown` pin trap) is needed.
7. Regenerate at `acadcc7cd` as a control before `04d6c9d52`. `f7f3d7bf0` may move the cast family's `messages` dump independently.
8. The cache/rebind half is already proven by `avatar_job_tier3` (8 `cache_*` cases). The lane needs only a no-`force` payload pin.
9. Size SMALL, Sonnet tier. Rider: `help/chat-settings-ai-services.md` byte-copy (13 help files differ in all, each owned by its commit; 129 = 129). The docs/v4 mirror needs CHANGELOG (stale beyond the baseline), API.md, DDL.md and db-size-reduction-spec.md.
10. **SURPRISE:** v5 has none of v4's avatar-trigger or enqueue log lines (no `caller_context` at all). Worse, v5's wardrobe TOOLS (`tools/wardrobe_create.rs:369`, "image subsystem seam (out of scope)") never call `trigger_avatar_generation_if_enabled`, where v4's `notifyWardrobeChanged` does for create/wear/take_off/archive. A character's own mid-chat wardrobe change never refreshes their avatar in v5. That is a pre-existing gap worth its own order.
