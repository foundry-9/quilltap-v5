# Survey — search, recall and Brahma: 4.12.1's window-first dated search (`6121f59b9` / `afec0af10`), the Staff-free global message search (`902c6efe9`, the behaviour + its index-rebuild ensure), `4f5047241`'s `?msg=` deep links (server half), and the Brahma lookup shortcuts + the inline-`@Brahma` profile (`c539c82e0`)

Read-only planning survey, **2026-11-04**. v4 pin **`624d0d25c`** (v4 `main`
HEAD, "ci: restore the 8 GB heap for the coverage run", `4.13.0-dev.8`).
Oracle baseline **`236c04e35`**. v5 `main` **`ff1651177`** (core 0.0.1427,
host 0.0.223, web 0.0.256, cli 0.0.38, harness 0.0.1110, SPA 0.5.971). Every v4
read below is `git show 624d0d25c:<path>`, `git diff 236c04e35 624d0d25c -- <path>`
or one commit's hunks. The v4 working tree (which has unrelated uncommitted
optimizer edits) was never read. Nothing was built or run, and nothing was
written in either repo except this file. Port facts come from SHIPPED HUNKS,
never commit prose. v4 line numbers are **at the pin**. v5 line numbers are at
**`ff1651177`**.

**Commits in scope:**

| sha | subject | this survey's slice |
|---|---|---|
| `6121f59b9` (+ merge `afec0af10`) | release: 4.12.1 | the whole `lib/` + `help/` content, EXCEPT `lib/tools/native-tool-prompt.ts` (§1.1.10: `96156566b`'s lane owns that file) and the CI / CHANGELOG / version stamps |
| `902c6efe9` | search: keep Staff messages and announcement bubbles out of global search | whole. The behaviour is here. The migration is shared with the schema survey (§7, ruling R-1) |
| `4f5047241` | search: look in the open Salon chat first; ?msg= links scroll to the message | the server half, which **does not exist** (§1.3). The SPA files are listed for the SPA lane in §6 |
| `c539c82e0` | Feature: Brahma lookup shortcuts and an engine setting for @Brahma | whole server side. The SPA pickers are listed for the SPA lane in §6 |

Ledger rows read: `drift-ledger.md:486` (`4f5047241`), `:487` (`902c6efe9`),
`:529` (`c539c82e0`), `:539` (`96156566b`, for the file collision),
`:544` (`6121f59b9`) and `:545` (`afec0af10`); §1 `:17-310`, including the
LIVE-DATA bullets at `:296-307`; and §5. Where a hunk disagrees with a row, the
correction is marked **⚠ ledger**.

Shape follows `surveys/2026-10-29-setupphase-keystone-memory.md`.

---

## Headline findings (written last)

1. **⚠ ledger (`:544`) — the "honest empty" text is not confined to dated searches.** The production caller always passes `opts`: `tool-executor.ts:1025` calls `formatSearchScriptoriumResults(result.results, { query: result.query, windowCensus: result.windowCensus })`, and it is the only production caller at the pin (`git grep`). So **every** empty `search` now returns ``Nothing matched "<query>". An empty search is not proof that nothing happened: try other words, or pass since/until if you are asking about a particular time.`` (`search-scriptorium-handler.ts:668-…`). The undated case says this too. `No relevant results found.` survives only for a caller that passes no `opts`, and production has none. Today v5's `tools/search.rs:979-981` and `tools/executor.rs:1169-1173` give the old text in every case.
2. **⚠ ledger (`:544`) — `minScore ??` is NOT a divergence to carry; it is v4 converging onto v5.** v5's `db/conversation_search.rs:64` is `options.min_score.unwrap_or(0.3)`, which is already `??` semantics (an explicit `Some(0.0)` sticks). v4's `conversation-search.ts:77` moved from `||` to `??`. Nothing changes in v5 until a caller passes `Some(0.0)`, which the narrow-window search arm now does (`handler:363`).
3. **⚠ ledger (`:486`) — `4f5047241` has NO server half.** It ships no `app/api/` hunk. `?msg=` links are already emitted server-side at the baseline: `app/api/v1/ui/search/route.ts:269` (`url: /salon/${chatId}?msg=${messageId}`) is untouched in the span, and v5 already mirrors it at `crates/quilltap-core/src/api/ui_search.rs:606`. Everything in the commit is SPA, the client module `lib/chat/conversation-search.ts`, `lib/chat/message-navigation.ts` (client-only sessionStorage plus a window event) and `help/search.md`. v5's SPA records the deferral by name at `apps/web/src/app/search/search.logic.ts:20-21`. The row should read **PORT-NEW (SPA) only**.
4. **Every `search` row in `search_tools_equivalence` reddens on the log line alone.** The handler's per-call INFO `Search scriptorium completed` gains `defaultedSources` (always present, between `scope` and `totalFound`) and, on dated calls, `windowCensus: {memoryCount, conversations}` (`handler:550-560`). Since P4.114, every search case compares that line (`harness/oracle/cases/search-tools-search.test.ts:354-357`). The window rows (`:127-137`) move further: there is a new `windowCensus` key in the output JSON, `lastMessageAt`-first chat spans, the window-scoped conversation search and the relaxed floor.
5. **The window pass in the TEXT strategy filters `excludeIds` and cold rows itself** (`gather.ts:329-334`), and it sorts rows newest-first with `Date.parse(b.occurredAt ?? b.createdAt) - Date.parse(a…)` (`:333`). v5's `gather_text` doc (`services/recall/gather.rs:15-17`) says "NO exclude / cold filter applies here — hydrate does it". The new arm is the exception and must carry v4's filters. **The sort is a NaN comparator**: an unparseable `occurredAt` / `createdAt` gives `NaN`, so it needs `js_array_sort_by` (the V8 short-array-sort trap in the memory notes), not `sort_by`. The text map's guard also moves: `probes.length > 0 ? Math.max(...) : 0` (`gather.ts:≈320`, `:347`), where v5 folds to `f64::NEG_INFINITY` (`gather.rs:557-561`).
6. **`findIdsInEventWindow` compares ISO strings LEXICALLY in SQL** (`memories.repository.ts:1073-1102`: `COALESCE(occurredAt, createdAt) >= ? AND … <= ?`), while `score.ts`'s `inWindowAt` uses `Date.parse` (`score.ts:≈76-80`). v4's own comment says "keep the two in step". The two agree only when every stored value is a `…Z` ISO string. A date-only or offset `occurredAt` (`2025-12-01`, `…+05:00`) is read differently by the SQL gather and the JS score window. Carry the SQL verbatim (ruling R-3) and measure Friday's `occurredAt` shapes on the dogfood copy.
7. **The tool path's floor relaxation also reaches `lookup_memories`.** `score.ts:82`: `relaxFloor = !opts.recallContext && window && to - from <= 14 days`. `brahma-lookup-handler.ts:321-329` calls `searchMemories(… window, includeCold: true …)` with no recall context, so the Brahma lane's `lookup_memories` differential is byte-equal only on a tree that also carries the SEARCH lane's gather/score changes. That is a union red by construction (contract C-2).
8. **`902c6efe9` makes the FTS trigger text a cross-version byte contract.** `staleChatMessageFtsTriggers` (`chat-message-fts.ts:184-…`) compares each trigger's `sqlite_master.sql` against v4's expected text (`IF NOT EXISTS ` stripped) **byte for byte**. `ensureChatMessageFtsSchema` (`:203-213`) then DROPs and recreates any trigger that differs. v4's boot reconcile calls that ensure on every boot (`lib/startup/reconcile-chat-message-fts.ts:78`). So any whitespace drift in v5's trigger text from v4's template literal makes **each side drop and recreate the other's triggers on every boot**, a new ping-pong class. Rebuilds do not follow, because the counts agree. `chat_message_fts_equivalence` already compares `sqlite_master` text (`harness/oracle/cases/chat-message-fts.test.ts:245-253`) and will pin the bytes. The ledger's §1 rebuild ping-pong (`:296-302`) retires once the eligibility SQL is ported.
9. **⚠ ledger (`:529`) — the Brahma settings ERASURE is measured, not hypothetical.** v5's `set_brahma_console_settings` writes a fresh `json!({ "maxAgentTurns": validated })` (`db/instance_settings.rs:510-520`). `brahma_console_settings_update` rebuilds the merge as `{maxAgentTurns}` alone (`api/settings.rs:3268-3306`), and the request decoder carries only that key (`api/types.rs:4179-4182`, `:4899-4902`). **Any v5 PUT drops a v4-written `inlineConnectionProfileId`.** The GET also answers only `{maxAgentTurns}`. And the read diverges from v4 even without a write: a stored blob whose `inlineConnectionProfileId` is not a UUID fails v4's whole-object parse, so v4 WARNs and answers the DEFAULT `maxAgentTurns` (50). v5's `parse_brahma_console_settings` (`:462-478`) ignores the key and keeps the stored budget.
10. **The project vault-key ERASURE is confirmed from source.** v5's `ProjectProperties` (`db/projects.rs:78-170`) has no `brahmaConnectionProfileId`, and `property_keys()` (`:175-197`) omits it. Serde drops the unknown key on the read-modify-write, so every v5 project write erases it. v4's schema order puts it **between `answerConfirmationOverride` and `storyBackgroundsEnabled`** (`project.types.ts:109`; `PROJECT_STORE_MANAGED_FIELDS:205`). It follows the #136 three-state `double_option` pattern exactly.
11. **One tool-catalog regen covers the whole span.** With `--full-history --no-merges`, the only commits past the baseline that touch the snapshot or any `lib/tools/*-tool.ts` are `c539c82e0` (four new entries), `6121f59b9` (three `searchScriptorium` strings) and `7bdb45873`. `7bdb45873` is the 4.12.0 squash on `bugfix` and duplicates `c539c82e0`'s content. The `searchScriptoriumBrahma` and `…Scenario` variants do not move (the snapshot hunks are `:2110` and `:2127` only). So a single `CATALOG:` stacking commit at the pin, regenerated via `harness/oracle/tools/gen-tool-catalog.mjs`, belongs in this area (ruling R-4). The oracle case `harness/oracle/cases/tool-definitions.ts` hand-lists `ALL_TOOLS` (`:79`, `:165`), so it must gain the four imports and keys in v4's order: after `runSql` and before `searchScriptorium` (snapshot test `:154-158`).
12. **Every helper the four lookups need is already ported.** `searchMemories` with `embedding_memo` is at `services/recall/search.rs:219-247`. The rest are:
    - `resolve_speaker_names` / `speaker_label` (`services/speaker_names.rs:82`, `:117`);
    - `is_character_authored_message` (`chat_activity.rs:51`);
    - `characters_read::find_all_raw` (`:338`);
    - `memories_read::count_by_character_id` (`:1226`);
    - `filter_importance` (`memory_scoring/importance.rs:47`);
    - `memory_archived_label` (`tools/search.rs:916`);
    - `zoned_day_bounds_iso` (`memory_time/calendar.rs:267`);
    - `refresh_memory_time_zone` (`services/memory_time_zone.rs:94`).

    The lookups are new composition, not new primitives.
13. **The lane cut is four server lanes plus SPA and vendor.**
    - **S1 FTS** (wave 0): all of `902c6efe9`, plus `c539c82e0`'s `participantId` hit-shape hunk, because both edit `db/chats_search.rs`.
    - **S2 SEARCH**: `6121f59b9`, branched from `CATALOG`.
    - **S3 BRAHMA-TOOLS**: its FIRST commit is `CATALOG`; then the four lookups, `run_sql`'s budget and the SQL prompt.
    - **S4 INLINE-PROFILE**: the settings key, the project vault key, the remap, the import and the one-shot resolution.

    The collisions with other areas are `tools/search.rs` (`0c47f4539`, the mount-index survey's `reach.ts` refactor), `services/tool_build.rs` and `tools/native_tool_prompt.rs` (`96156566b`, the agent-mode lane), `tools/executor.rs` (several areas), and `host.rs` with `restore_vintage_state` (the schema survey's boot-ensure list). The full table is in §7.

---

## 1. Per commit, the shipped hunks

### 1.1 `6121f59b9` — 4.12.1, window-first dated search (prod `lib/` 14 files)

`afec0af10` is the merge. `git diff 6121f59b9 624d0d25c` over 4.12.1's `lib/` files is the one file `lib/tools/native-tool-prompt.ts` (re-shaped by `96156566b`), so the pin's content is 4.12.1 whole. Measured, and it agrees with the ledger.

#### 1.1.1 Repository — `lib/database/repositories/memories.repository.ts:1063-1102`
- NEW `findIdsInEventWindow(characterId, {from,to}, {hotOnly})`, as `safeQuery`.
  - SQL: `SELECT id FROM memories WHERE characterId = ? AND COALESCE(occurredAt, createdAt) >= ? AND COALESCE(occurredAt, createdAt) <= ?`. When `hotOnly`, the template appends `\n              AND ${HOT_TIER_SQL}` (whitespace inside the template is irrelevant to results).
  - DEBUG `Found memories in event window` `{characterId, from, to, hotOnly, count}`.
  - Fallback `[]` with the error text `Error finding memories in event window` `{characterId, from, to}`.
- **v5:** NEW `memories_read::find_ids_in_event_window`, beside `HOT_TIER_SQL` (`db/memories_read.rs:105`). It is a fallback read, so a SQL error logs v4's safeQuery line and answers `[]` (the P4.149 / P4.163 fallback class).

#### 1.1.2 Index — `lib/memory/index-store/memory-index.ts:80`, `:147-153`
- `MemoryIndexSearchOptions.includeIds?: ReadonlySet<string>`. The filter becomes `(!include || include.has(id)) && (!exclude || !exclude.has(id)) && (!hotOnly || isHotVector)`, and `filter` is engaged when `hotOnly || exclude || include`.
- **v5:** `services/memory_index.rs:63-70` `IndexSearchOptions` gains `include: Option<&HashSet<String>>`, and `search` (`:87-102`) gains the third clause and the third engage condition.

#### 1.1.3 Gather — `lib/memory/recall/gather.ts:176-200` (vector), `:276` (trace), `:318-357` (text)

**Vector strategy.** The window pass runs AFTER the probe merge and BEFORE the literal / anchor hits:
- It reads `ids = findIdsInEventWindow(characterId, query.window, {hotOnly: !includeCold})` and sets `windowIds = ids.length`.
- `inWindow` is `ids` minus `excluded`. When it is non-empty, the pass rebuilds `byId` from `hits` in order and runs one `memoryIndex.search(… {...indexSearch, includeIds: inWindow})`.
- A new id is appended `{via: 'window'}` and counts toward `windowAdded`. An existing id with a better score keeps its `via` and takes the new score.
- DEBUG `[Memory] Window-first gather` `{characterId, window, inWindow: ids.length, windowAdded}` is logged whenever a window is present, including when the set is empty.
- The trace detail gains `window` and `windowAdded` right after `probeAdded`, only when a window is present.

**Text strategy.**
- The existing map takes the `probes.length > 0` guard.
- When a window is present, the pass reads `findIdsInEventWindow(… {hotOnly: !opts.includeCold})` and filters out ids already in `pool` and in `opts.excludeIds`. `windowIds` is the FILTERED count.
- It runs `findByIds`, then `.filter(includeCold || tier !== 'cold')`, then sorts by event time descending (the NaN comparator, Headline 5), then `.slice(0, limit * 3)`.
- Each row is pushed as `{via: 'window', score: maxTextScore or 0, memory}`, and `windowAdded` counts them all.
- The trace key order is `strategy, reason, probes, [window, windowAdded], anchors, pool`.

**v5:** `services/recall/gather.rs:214-…` (the vector arm, at the probe merge `:300-350`; the detail at `:351-362`) and `gather_text` (`:512-588`). `HitSource::Window` goes into `services/recall/types.rs:18-24`, between `Probe` and `Anchor` (it serializes as `"window"`).

#### 1.1.4 Score — `lib/memory/recall/score.ts:27-39`, `:76-90`, `:130`
- NEW `export const TOOL_WINDOW_FLOOR_MAX_DAYS = 14` and `isNarrowToolWindow(window)`, which is `parseWindow(window) && to - from <= 14 * DAY_MS`.
- `score()` hoists `window = parseWindow(opts.window)` and `inWindowAt(memory)`, which uses `Date.parse(occurredAt ?? createdAt)`, finite and inclusive.
- `relaxFloor = !opts.recallContext && window && to - from <= 14 days`. A floored row passes when `relaxFloor && inWindowAt`.
- The trace detail gains `floorRelaxed: true` after `floored`, only when the floor is relaxed.
- **v5:** `services/recall/score.rs:73-77` (`parse_window`), `:113` (the floor filter), `:149-172` (the window staging re-uses `in_window_at`), and `:183-197` (the detail). Export `is_narrow_tool_window` for `tools/search.rs`.

#### 1.1.5 Assemble — `lib/memory/recall/assemble.ts:53`, `:618-648`
- In the retrospective mini-recap, when `query.window && filtered.length < RETRO_MINI_RECAP_MAX_ENTRIES`, the pass calls `findChatsInWindow({characterId, userId: plan.userId, window, excludeChatId: plan.chat.id})`.
- Each chat that is not already listed, standing or matched is pushed as `{conversationId, conversationTitle: title || 'Untitled', relativePath: null, score: 0, firstMessageAt: chat.createdAt, lastMessageAt: chat.lastMessageAt ?? null}` until the cap is reached.
- The trace detail becomes `{matched, listed, unsummarized, signature, reusedQuery}`. `unsummarized` is ALWAYS present, and is 0 without a window.
- `conversations.ts:41`: `relativePath: string | null`.
- **v5:** `services/recall/assemble.rs:965-1052`, with the detail at `:1041-1045`. `services/memory_recap/mod.rs:98` `relative_path: String` becomes `Option<String>`. Check every reader of the field. `first_message_at`'s doc says "finite under `Date.parse`", but v4 pushes the raw `createdAt` here. The renderer must accept it as given.

#### 1.1.6 NEW `lib/scriptorium/chat-window.ts` (96 lines)
- `chatSpanOf` is `[Date.parse(createdAt), max(start, first finite of lastMessageAt, updatedAt)]`. It is null when `createdAt` is unparseable. The order is `lastMessageAt` → `updatedAt` → `createdAt`.
- `chatOverlapsWindow` is inclusive, and false when a window bound is NaN.
- The sort is newest first by `span.end`, with a `?? 0` fallback (a well-formed comparator).
- `findChatsInWindow({characterId?, userId, window, excludeChatId?})` reads `chats.findByCharacterId(characterId)` when a character is given, else `chats.findByUserId(userId)`.
  - A throw logs WARN `Could not read chats for a time window` `{characterId, userId, error}` and returns `[]`.
  - It always logs DEBUG `Chats in time window` `{characterId, from, to, candidates, inWindow}`.
  - **No `chatType` filter.** Help, Brahma and autonomous chats count.
- **v5:** NEW `crates/quilltap-core/src/services/chat_window.rs`, over `chats_read::find_by_character_id` (`:666`) / `find_by_user_id` (`:613`). It must parse with `js_date_parse_ms`, not a strict ISO parse (the memory notes' `iso_to_ms` trap). The existing per-hit `updatedAt` span in `tools/search.rs:436-480` is DELETED; that is v4's `spanByChat` block, which is gone.

#### 1.1.7 Conversation search — `lib/scriptorium/conversation-search.ts:61`, `:77`, `:87-95`
- `chatIds?` restricts `characterChatIds` before scoring, with DEBUG `Conversation search restricted to window chats` `{requested, inScope}`.
- `minScore ?? 0.3`. See Headline 2.
- **v5:** `db/conversation_search.rs:42-90` `ConversationSearchOptions` gains `chat_ids: Option<HashSet<String>>`.

#### 1.1.8 The search handler — `lib/tools/handlers/search-scriptorium-handler.ts`
- `:40-45`: `SearchSource`, `ALL_SOURCES`, `CENSUS_MAX_CONVERSATIONS = 10`.
- `:70`: context `chatId?`.
- `:131-139` **dated default sources**. `defaultedSources = !parsed.sources && (since || until) && !context.mountPool`. When it holds, `sources = operatorSurface ? ['conversations'] : ['memories','conversations']`. Otherwise `sources` is the parsed value or `ALL_SOURCES`.
- `:156-165`: `narrowWindow = isNarrowToolWindow(timeWindow)`. `windowChats = timeWindow && (searchMemories || searchConversations) ? findChatsInWindow({characterId: operatorWide ? undefined : characterId, userId, window}) : null`. This runs ONCE, before any source, and does not exclude the current chat.
- `:259-264` **memory census**. Inside the owned-character branch, when a window is present: `windowMemoryCount = findIdsInEventWindow(characterId, window, {hotOnly: false}).length`, then DEBUG `Memories in search window` `{characterId, count, narrowWindow}`.
- `:340-372` **conversations**.
  - When `windowChatIds` is present but empty, the search is SKIPPED: DEBUG `No conversations in search window; chunk search skipped` `{characterId}`, and no embedding call is made.
  - Otherwise the search runs with `chatIds: windowChatIds` (when present) and `minScore: narrowWindow ? 0 : 0.3`, then DEBUG `Window conversation search` `{windowChats, hits}`.
  - The post-hoc span filter is DELETED.
- `:541-560`: `windowCensus = windowChats ? buildWindowCensus(…) : undefined`. The INFO gains `defaultedSources` and, when there is a census, `windowCensus: {memoryCount, conversations: listed + more}`. The output adds `...(windowCensus ? {windowCensus} : {})` after `query`.
- `:598-630` `buildWindowCensus`.
  - It filters out `excludeChatId` and keeps the first 10.
  - Each entry is `{id, title || 'Untitled', firstMessageAt: createdAt, lastMessageAt ?? null, date}`, where `date` is `zonedDateStamp(createdAt, tz)` when the memory zone is set. Otherwise it is the UTC `toISOString().slice(0,10)`, or the raw `iso.slice(0,10)` when the date does not parse.
  - The census adds `moreConversations`.
- `:≈645-700` `censusDate`, `censusPeriod`, `censusConversationLines` and `formatEmptySearch`.
  - Periods read `on X`, `between X and Y`, `between X and today` or `up to Y|today`.
  - The six empty texts are all verbatim in the hunk.
- `:705-…` `formatSearchScriptoriumResults(results, opts?)`.
  - With no results it returns `formatEmptySearch(opts?.query, opts?.windowCensus)`.
  - Otherwise it returns the body, plus a census footer when the census lists conversations: `Conversations you took part in during this period:` (or `Conversations during this period:` when `memoryCount === null`), the lines, then `Read any of them with read_conversation.`
- **v5:** `tools/search.rs`.
  - `SearchContext` gains `chat_id` (the executor fills it from `ToolExecutionContext.chat_id`, `services/tool_execution.rs:258`).
  - The sources default moves at `:220`, the window at `:282-300`, and the conversation arm at `:420-480`.
  - The INFO is at `:616-629` and the formatter at `:979-…`.
- **v5 executor:** `tools/executor.rs:1143-1185` `run_search` passes `opts` and adds `windowCensus` to the result JSON after `query`.

#### 1.1.9 Tool definition — `lib/tools/search-scriptorium-tool.ts:20-75`, `:129-166`
- Three `.describe` strings move: `query` (`:22-24`), `sources` (`:27-29`) and `since` (`:62`). `until` is unchanged. The new interface is `SearchWindowCensus`, and the output gains `windowCensus?`. Only the main variant moves; the Brahma and Scenario variants do not (snapshot hunks `:2110`, `:2127`).
- **v5:** the `CATALOG` regen (§3) moves `crates/quilltap-core/src/tools/definitions/data.rs` key `searchScriptorium`.

#### 1.1.10 Rule 6 — two prompts
- `simple-json-prompt.ts:266`. The old tail `If you find nothing, say you don't recall — never invent specifics.` becomes `If the search lists conversations from that period, read the likeliest one with \`read_conversation\` before answering. Only if it finds nothing and lists no conversation from that period, say you don't recall — never invent dates, places, or details.` **v5:** `tools/simple_json_prompt.rs:186` (`SUFFIX`) → **S2**.
- `native-tool-prompt.ts:34`. The same sentence now sits inside `96156566b`'s exported `NATIVE_TOOL_RULES` (`:24-34`, under the two `PREAMBLES` at `:36`). **v5:** `tools/native_tool_prompt.rs:5-24` is re-shaped wholesale by `96156566b`. **Owner: the agent-mode lane** (ruling R-5). It takes the PIN's file, which carries 4.12.1's sentence, and S2 does not touch it.

#### 1.1.11 Help: `help/episodic-memory.md` (4 lines), `help/memory-recall-relevance.md` (2) → vendor lane.

### 1.2 `902c6efe9` — Staff rows out of global message search (prod 7 files)

- `lib/database/backends/sqlite/chat-message-fts.ts:101-107`: `chatMessageFtsEligibilitySql(alias)` appends `` ` AND ${p}"systemSender" IS NULL AND ${p}"customAnnouncer" IS NULL` ``.
- `:126-137`: the INSERT trigger's `WHEN` is now `${chatMessageFtsEligibilitySql('new')}`. The bytes are `new."type" = 'message' AND new."role" IN ('USER','ASSISTANT') AND new."content" IS NOT NULL AND new."systemSender" IS NULL AND new."customAnnouncer" IS NULL`. The trigger comment lines change, but the comment is outside the SQL string.
- `:≈165-200`: NEW `expectedTriggerSql()` (the statements' trigger texts with `CREATE TRIGGER IF NOT EXISTS ` → `CREATE TRIGGER `) and `staleChatMessageFtsTriggers(db)`. The latter is `SELECT name, sql FROM sqlite_master WHERE type = 'trigger' AND name IN (?,?,?)`, then the names whose `sql !==` the expected text. A MISSING trigger is not "stale"; `missingChatMessageFtsObjects` covers that case.
- `:203-213`: `ensureChatMessageFtsSchema` runs the stale check first. On a stale trigger it logs DEBUG `Replacing outdated chat message FTS triggers` `{stale}` and runs `DROP TRIGGER IF EXISTS "<name>"` for each, then the five `IF NOT EXISTS` statements as before.
- `:246`, `:299`: the counts and the rebuild already use the eligibility function, so they move automatically.
- `lib/database/repositories/chats-search.ops.ts:89`: the FTS arm adds `AND ${chatMessageFtsEligibilitySql('mm')}` after `MATCH ?`. The LIKE arm (`:113`) already used the function. The legacy non-SQLite loop (`:274`) uses `isCharacterAuthoredMessage(msg)`; v5 has no such arm, so that is NO-PORT.
- `migrations/scripts/reindex-chat-message-fts-character-only.ts` (98 lines, NEW).
  - `id` is `reindex-chat-message-fts-character-only-v1`. `dependsOn` is `[create-chat-message-fts-v1, add-system-sender-field-v1, add-custom-announcer-field-v1]`.
  - `shouldRun` is: SQLite, AND `chat_messages` exists, AND (missing objects OR stale triggers OR eligible ≠ indexed).
  - `run` captures `stale` and `before` (the indexed count), calls `ensure`, then logs INFO `Rebuilding chat message search index without Staff messages` `{context: 'migrations.reindex-chat-message-fts-character-only', replacedTriggers, indexedBefore}`.
  - It then runs `rebuildChatMessageFtsIndex(db, reportProgress(done, all, 'messages'))`, computes `dropped = max(0, before - indexed)`, and logs INFO `Chat message search index rebuilt` `{context, scanned, indexed, total, dropped}`.
  - The result message is `Removed N Staff message(s) from the search index` or `No Staff messages were in the search index`.
- `migrations/scripts/index.ts:463`, `:956`: registered LAST, after `stripAnthropicSamplingParametersMigration`.
- `create-chat-message-fts.ts:50`: `dependsOn` gains the two field migrations. This is ordering only; in v5 the columns are in `fresh_schema.json` and the reconcile runs at PHASE 3.65 (`quilltap-host/src/host.rs:1699-1730`).
- `lib/startup/prettify.ts`: a label. `docs/developer/DDL.md` and `help/search.md` go to the vendor lane.
- `lib/startup/reconcile-chat-message-fts.ts` has NO prod hunk, but it calls `ensureChatMessageFtsSchema` (`:78`). So **v4's every-boot reconcile now replaces stale triggers too**, and v5's `reconcile_chat_message_fts` (`db/chat_message_fts_reconcile.rs:92-160`) inherits that through `ensure_chat_message_fts_schema` (`db/chat_message_fts.rs:175-180`).
- **v5 surfaces:**
  - `db/chat_message_fts.rs:118-125` (the eligibility function), `:143-144` (the hand-written trigger `WHEN`, which must become the function's bytes), `:175` (ensure + stale), and a NEW `stale_chat_message_fts_triggers`.
  - `db/chats_search.rs:111-127` (the FTS SQL).
  - A NEW boot ensure for the migration, STAMPED (R-1). It goes in `host.rs` BEFORE the PHASE 3.65 reconcile at `:1727`, and in `restore_vintage_state`'s boot-align list (memory note "Vintage restore mirror-boot list").

### 1.3 `4f5047241` — the server half: NONE (Headline 3)

The 27 files are the SPA, `lib/chat/conversation-search.ts` (a client module), `lib/chat/message-navigation.ts` (client), CSS, `help/search.md` (+32 / −?) and `docs/FEATURES.md`. There is nothing for a server lane. §6 lists the SPA work.

### 1.4 `c539c82e0` — Brahma lookup shortcuts + the inline-`@Brahma` profile (prod 26 files)

#### 1.4.1 Four NEW operator-only tools
- Definitions:
  - `lib/tools/lookup-character-tool.ts`: `{character: string min 1}`.
  - `lookup-memories-tool.ts`: `query`, plus optional `character`, `about`, `since`, `until`, and `limit` (`llmNumber` int 1..30, `.default(10).optional()`).
  - `lookup-messages-tool.ts`: `query`, plus optional `character`, `speaker`, `since`, `until`, `limit`.
  - `read-messages-tool.ts`: `conversation_id`, plus optional `around_message_id`, `before` / `after` (0..10, default 3), `offset` (int, default 0) and `count` (1..20, default 10).
  - Each declares effect `'read'`.
- Handler: `lib/tools/handlers/brahma-lookup-handler.ts` (548 lines, cited by line in the full file at the pin).
  - The budgets (`:59-68`) are `CORE_FIELD_CHARS` 3000, `PHYSICAL_CHARS` 1500, `MEMORY_CHARS` 700, `MESSAGE_CHARS` 2500, `SNIPPET_CHARS` 320, `RECENT_CONVERSATIONS` 8, `MAX_HOLDERS` 60 and `MESSAGE_OVERFETCH` 200.
  - `clip` (`:74-79`) appends `… [truncated, N chars in all]`. Lengths are UTF-16.
  - `invalid` (`:81-87`) answers `Invalid <tool> input: <path> — <message>.`. These are the first Zod issue's bytes, so v5 needs the Zod issue message text for these four schemas (the `api/zod_issues.rs` home).
  - `resolveCharacterRef` (`:101-134`) tries an id or exact name over `findAllRaw`, then exact name or alias over hydrated `findAll`, then substring. It prefers live characters over archived ones and returns `ambiguous` with up to 10 candidates.
  - The texts are `unresolved` (`:137-147`) and `resolveWindow` (`:153-163`): date-only bounds go through `zonedDayBoundsIso`, or `T00:00:00.000Z` / `T23:59:59.999Z`; open bounds are `0000-01-01…` / `9999-12-31…`.
  - `conversationRef` (`:183-188`) answers `{id, title, createdAt, chatType}` or `{id, title: null, note: 'conversation no longer exists'}`.
  - `snippetAround` (`:191-207`).
- `lookup_character` (`:213-276`).
  - It filters chats to `salon` / `autonomous`, and sorts by `updatedAt` descending with `localeCompare`. That is the ICU-collation class: carry v5's en-US collator, as in P4.D279's `localeCompare` finding.
  - `first` is the minimum `createdAt` by string `<=`. `physicalDescription` is `fullDescription ?? completePrompt ?? longPrompt ?? mediumPrompt ?? shortPrompt`.
  - It returns the fixed `hint` sentence.
- `lookup_memories` (`:282-374`).
  - The holders are one named character, or up to 60 live characters with `countByCharacterId > 0` in `findAllRaw` order.
  - It shares ONE `embeddingMemo`. It calls `searchMemories` per holder with `{literalPhrase: true, aboutCharacterId, window, includeCold: true}`, sorts the merged hits by score descending, and takes the first `limit`.
  - **No access stamp** is written: it never calls `stampAccessed`.
- `lookup_messages` (`:380-476`).
  - It reads chats (by character, or `findAll`) filtered to salon / autonomous, and optionally to a `speaker` seated character.
  - With no chats it answers the `No conversations match those filters.` note.
  - It calls `searchMessagesGlobal(ids, query, postFiltered ? 200 : limit)`, then filters the window by string comparison on `createdAt` and filters the speaker by seat, then slices.
  - The labels come from `resolveSpeakerNames` / `speakerLabel`. With no hits it adds the `No matches. Try …` note.
- `read_messages` (`:482-548`).
  - It returns the `isCharacterAuthoredMessage` lines, with an around-window or an offset / count slice (a negative offset counts from the end).
  - The `cast` is `Array.from(new Set(names.values()))`. The not-found texts are verbatim.
- Every tool logs a `… completed` / `resolved` DEBUG and a `… failed` WARN, and returns `` `<tool> failed: ${message}` `` on a throw.
- Dispatch: `lib/chat/tool-executor.ts:273-282` (`BRAHMA_LOOKUP_HANDLERS`), `:314` (`BUILT_IN_TOOLS` gains the four names after `run_sql`), `:1069-1086`. The gate is `` `${name} is only available in the Brahma Console.` `` with `result: null`; the success result is the handler's object whole.
- Slate: `lib/tools/plugin-tool-builder.ts:530-538`. Under `options.sqlAccess`, the four push BEFORE `runSqlToolDefinition`.
- Effects: `lib/tools/effects.ts:164-171` (the map order follows `run_sql`).
- **v5:**
  - NEW `tools/brahma_lookup.rs`, registered in `tools/mod.rs`.
  - `tools/executor.rs`: the `BUILT_IN` list at `:127` and `:263-265`, the dispatch at `:678`, and the gate copied from `run_run_sql` at `:1305-1321`.
  - `tools/effects.rs:249`, `:331`.
  - `services/tool_build.rs:357`: four `push_key`s before `"runSql"`.

#### 1.4.2 `run_sql` budget — `lib/tools/handlers/run-sql-handler.ts:50-58`, `:233-277`, `:376-399`; `run-sql-tool.ts:95-102`

- `RESULT_CHAR_BUDGET = 24_000` and `LONG_CELL_CHARS = 600`. `fitToBudget` works as follows:
  - It measures `JSON.stringify(rows).length`, which is **UTF-16 units of JS-formatted JSON**. v5 must measure through its JS-stringify twin, not `serde_json::to_string`: serde formats `1.0` and JS formats `1`, which changes the count at the boundary.
  - It clips every string cell longer than 600 code units to `` `${slice(0,600)}… [${len} chars; select substr(${key}, …) to read more]` ``.
  - It drops rows to `max(1, floor(n * 0.75))` while the result is over budget.
  - It writes the note `Result exceeded the size budget: <parts>. Select fewer or narrower columns, aggregate, or add a LIMIT.`
- `columns` now comes from `sliced[0]` before sanitizing. The keys are identical, so there is no byte change.
- `truncated` is now `rawRows > maxRows || droppedRows > 0`. A DEBUG `run_sql result shortened to fit the size budget` `{userId, database, clippedCells, droppedRows}` is logged, and the output adds `note` last, only when present.
- **v5:** `tools/run_sql.rs:65-100` (the `RunSqlResult::Success` serializer is a FIXED 6-field struct; it needs `note: Option<String>` serialized last when `Some`), and `:320-335`.

#### 1.4.3 The Brahma SQL prompt — `lib/brahma-console/brahma-sql-prompt.ts:18-…`, `:78`
- A new leading section, `## Reach for the lookup shortcuts first`, of 5 paragraphs. The `### Chats and messages` paragraph gains two sentences: the no-`LIKE` rule and the `participantId`-is-a-seat rule.
- **v5:** `services/brahma_console/prompt_text.rs:15` (`BRAHMA_SQL_PROMPT`; the doc says 8231 UTF-16 units). It is GENERATED: regenerate with `harness/oracle/cases/gen-brahma-prompts.mjs`. The base brief is unchanged.

#### 1.4.4 The global message-search hit — `chats-search.ops.ts:24-41`, `:60`, `:259`, `:282`
- `participantId` is selected (`m."participantId" AS participantId`) and added to every hit **between `role` and `createdAt`**, defaulting to `?? null`.
- The `/api/v1/ui/search` route (`route.ts:256-281`) maps fields explicitly, so its wire is unchanged.
- **v5:** `db/chats_search.rs:380-397` (`run_search`'s row map) and `defer_text_decode`'s SELECT (`:≈85-100`).

#### 1.4.5 Inline `@Brahma` profile resolution
- NEW `lib/services/brahma-console/inline-profile.ts` (110 lines). `resolveInlineBrahmaConnectionProfile(repos, userId, chatId)` resolves in this order:
  - **Project:** `chats.findById` → `projectId`. A throw logs WARN `Could not read chat for inline Brahma profile resolution; skipping project override`. Then `projects.findById` → `brahmaConnectionProfileId` → `findConfiguredProfile`. A throw logs WARN `Could not read project …`.
  - **Global:** `getBrahmaConsoleSettings().inlineConnectionProfileId`. A throw logs WARN `Could not read Brahma Console settings; skipping global inline profile`.
  - **Default:** `connections.findDefault(userId)`.
  - Logs at each step: DEBUG `Inline Brahma using project|global|default connection profile`. A stale or foreign id logs WARN `Inline Brahma connection profile no longer exists; falling through` `{source, profileId, …context}`.
- `one-shot.service.ts:45`, `:72-74`: `runBrahmaQuery` uses it in place of `resolveBrahmaConnectionProfile(…, null)`.
- `settings.types.ts:383-392`: `BrahmaConsoleSettingsSchema.inlineConnectionProfileId: UUIDSchema.nullable().optional()`. The route `app/api/v1/settings/brahma-console/route.ts:27-30` adds `inlineConnectionProfileId: … ?? null` to `logSummary`.
- `project.types.ts:99-109`, `:205`: `brahmaConnectionProfileId: UUIDSchema.nullable().optional()`, a store-managed field. The project PUT schema `app/api/v1/projects/[id]/schemas.ts:23` is `z.string().uuid().nullable().optional()`.
- Carriers:
  - `lib/backup/restore/uuid-remap.ts:93-112`: the `brahmaConsole` instance-setting JSON remap, with DEBUG `Remapped inline Brahma connection profile in instance settings` `{from, to}` and WARN `brahmaConsole instance setting is not valid JSON; leaving it as is`.
  - `:566`: the project `remapFields` gains `brahmaConnectionProfileId`.
  - `:798`: the instance-settings arm.
  - `lib/import/quilltap-import/reconcile.ts:524-531`: the project remap through `idMaps.connectionProfiles`.
  - `public/schemas/qtap-export.schema.json:1661` (vendor).
- **v5 surfaces:**
  - `services/brahma_console/mod.rs:90-108` (`resolve_brahma_connection_profile`) and `:193`. The new resolution lives in NEW `services/brahma_console/inline_profile.rs`.
  - `db/instance_settings.rs:440-520` (Headline 9) and `api/settings.rs:3268-3306` (the GET is at `:≈3246`).
  - `api/types.rs:4179-4182`, `:4899-4902` (the decoder carries one key today).
  - `db/projects.rs:78-170`, `:175-197` (Headline 10).
  - `api/projects.rs:490` (the field-rule table) and `api/zod_issues.rs:997` (`nullable_uuid`).
  - `services/qtap_export/schema-key-order.json` (`project`, `:296` area).
  - `services/backup/uuid_remap.rs:52` (`MOUNT_POINT_SETTING_KEYS`), `:283`, `:501-…` (the instance-settings `each`).
  - `services/quilltap_import/reconcile.rs:618-…`.

---

## 2. Which earlier orders ported what these commits move

| v5 surface | ported by |
|---|---|
| `services/recall/*` (gather / score / assemble / search / types) | P4.D291 (the recall pipeline), on P4.D267 (retuning, hot search), P4.D268 (archive, delivered-only stamps), P4.D342 (the inner circle) |
| `tools/search.rs` window / `since` / `until` / conversation span | P4.d13 (episodic retrieval tools), P4.114 (log lines on every case), P4.D216 (the pre-built pool), P4.D268 (archived labels) |
| `services/memory_index.rs`, the TF-IDF / vector store | P4.D292 (refit + backfill), P4.D288 (the keystone) |
| `db/conversation_search.rs` | P4.d13 |
| `services/memory_recap/mod.rs` `VaultConversationMatch` | P4.d13 (public since then) |
| `db/chat_message_fts.rs`, `chat_message_fts_reconcile.rs`, `chats_search.rs`, `host.rs:1699-1730` | P4.D204 (FTS5 message search); P4.105 (the update trigger) |
| `api/ui_search.rs` (`?msg=` URL) | P4.6-era global search; P4.D204 |
| Brahma one-shot `services/brahma_console/*`, `carina_query.rs` | W4.5b; P4.9I1A (server); P4.79; P4.89; P4.165 (Stop) |
| Brahma settings GET / PUT | P4.D57 |
| `tools/run_sql.rs` | W4.1d5; P4.D331 (the `log_entries` description) |
| `tools/definitions/data.rs` (catalog) | W4.1b.3; last regen P4.D350 `CATALOG` `8ab640259` |
| `tools/effects.rs` | P4.D343 (declared effects) |
| `db/projects.rs` three-state keys; `zod_project_properties_issues` | P4.146 (#136); P4.148 |
| `services/backup/uuid_remap.rs`; `quilltap_import/reconcile.rs` | P4.D273 / P4.167 / earlier carrier rounds |

---

## 3. The `CATALOG` regen (one stacking commit, in S3's first commit)

Regen recipe (`harness/oracle/tools/gen-tool-catalog.mjs:15-23`). Run it from the **pinned worktree at `624d0d25c`** (drift-ledger §5.1), never from the live v4 checkout:
1. Add the four imports and keys to `harness/oracle/cases/tool-definitions.ts` in v4's `ALL_TOOLS` order: `runSql`, `lookupCharacter`, `lookupMemories`, `lookupMessages`, `readMessages`, `searchScriptorium` (pin snapshot test `:154-158`).
2. `npx tsx …/tool-definitions.ts > ndjson`.
3. `node gen-tool-catalog.mjs ndjson crates/quilltap-core/src/tools/definitions/data.rs`.

Expected moves: **five entries.** Four are NEW. `searchScriptorium` takes three description strings. The header count goes from 76 to 80. Measure the count after the regen; do not assume it. The commit subject starts with `CATALOG:` (the P4.D350 precedent: `p4.d350-…md:85-106`).

## 4. Schema / D23

None in this area. `902c6efe9`'s migration touches no generateDDL object: the FTS objects are filtered out of the dump by construction (`host.rs:1715-1720`), so `fresh_schema.json` does not move. `c539c82e0` adds two JSON keys, one in an instance-settings value and one in a project `properties.json`. Neither is a column.

## 5. Harness / oracle impact

These are v4 drift reds at the next pin, not v5 bugs.

### 5.1 S1 FTS
- **`chat_message_fts_equivalence`** (`cases/chat-message-fts.test.ts`). The `sqlite_master` trigger text moves (`:245-253` compares it) and the eligibility counts move. Grow it with a planted stale trigger (the stale replace + DEBUG) and with Staff rows (`systemSender` set; `customAnnouncer` set).
- **`chat_message_fts_reconcile_equivalence`**: Staff rows now count as ineligible.
- **`chats_search_equivalence`** (`cases/chats-search.ts`). Every hit gains `participantId` in key position 5 (the `c539c82e0` hunk, owned by S1). Rows with Staff messages drop.
- **`ui_search_equivalence`**: rows with Staff messages drop; the wire keys are unchanged.
- **`chat_message_update_fts_tier2_equivalence`**: re-measure.
- **NEW ensure family**, `reindex_chat_message_fts_ensure_equivalence`. Closest recipe: `memories_superseded_by_index_ensure_equivalence` / `chat_informs_permanent` (v4's REAL migration under the narrowed `MigrationRunner`; memory note "Oracle runner + SKIP traps"). Add the new id to `zero_migrations_rerun_equivalence`'s `run_chain` and to the `migrations_state` plants.

### 5.2 S2 SEARCH
- **`search_tools_equivalence`** (`cases/search-tools-search.test.ts`):
  - every search row, through the INFO's `defaultedSources` (Headline 4);
  - the window rows `:127-137`, through `windowCensus` in `resultJson`, the gather window pass and the floor relaxation (`search_window_dateonly` is a 31-day window, so NOT narrow);
  - the conversation rows `:135-136`, through `lastMessageAt` spans and the window-scoped search, and the skip with no embedding call on `_out`.

  The case formats WITHOUT `opts` (`:353`), so its `formatted` comparand keeps `No relevant results found.`. **Grow it**: format the way the executor does (`{query, windowCensus}`), plus rows for each of the six empty texts and the footer. The P4.D216 executor row (`:317-328`) goes through the real executor and moves wherever it formats.
- **`recall_assemble_tier3_equivalence`** (`cases/recall-assemble-tier3.test.ts:141-162`, the `timeRange` rows): the gather trace gains `window` / `windowAdded`, the retrospective trace gains `unsummarized` (on EVERY retrospective row, as 0 or more), and unsummarized chats are listed.
- **Re-measure; they move wherever a window or the text strategy is in play:** `recall_query_tier3_equivalence`, `recall_probe_fixture_equivalence`, `semantic_search_tuning_tier3_equivalence`, `recall_replay_equivalence` (a replay without a recall context and with a ≤ 14-day window relaxes the floor), `recall_deliver_equivalence`, and `memories_routes_equivalence` (the memories search route, if it windows).
- **`pseudo_tool_prompts_equivalence`** (`cases/pseudo-tool-prompts.ts`): the simple-JSON rule 6.
- **NEW tier-1 / tier-2 family** for `chat-window.ts`: `chatSpanOf` / `chatOverlapsWindow` over NaN, absent and reversed spans, plus `findChatsInWindow` over a builder fixture. Closest recipe: `recent_conversations_block_equivalence` / `speaker_names_equivalence` (`cases/speaker-names.ts`). Plant chats with `lastMessageAt` NULL and `updatedAt` later.
- **NEW index row** for `memory_index_tier2_equivalence`: `includeIds` composed with `hotOnly` / `exclude`.
- **`tool_definitions_equivalence`**: red on any tree without `CATALOG`; green on S2 / S3, which branch from it.
- **Moved by OTHER areas in the same files.** `recall_tags_equivalence` moves with `ba6c8fe0f` (prompt-trust, `services/recall/context.rs` / `recall_tags.rs`), not here. The native rule 6 rides `96156566b`'s lane, through `native_tool_loop_tier3`, `orchestrator_tier3` and `build_context_tier3`.

### 5.3 S3 BRAHMA-TOOLS
- **`brahma_console_tier3_equivalence`** / **`brahma_orchestrator_tier3_equivalence`**. The `sqlAccess` slate gains four tools in the request bodies. The system prompt's SQL section moves through `prompt_text.rs`. **`carina_query_tier3_equivalence`** moves the same way for the inline `@Brahma` slate.
- **`tool_build_equivalence`**: the `sqlAccess` slates.
- **`state_sql_tools_equivalence`** (`cases/state-sql-tools.test.ts`, the `run_sql` handler). `truncated` / `note` move only on rows over 24,000 characters; grow it with a wide-row plant (cells over 600 units, rows over budget, a surrogate pair at the 600 boundary).
- **`tool_executor_ceiling_equivalence`** and the P4.D343 effect census: the four names.
- **NEW family `brahma_lookup_tools_equivalence`.** Closest recipe: `state_sql_tools_equivalence`, extended over a salon builder pair with memories, speakers and aliases (an ambiguous name, an archived namesake, an alias-only match, `read_messages` around / offset / negative offset, a Staff line skipped). ⚠ `lookup_memories` rows are union-red without S2 (C-2). `lookup_messages` rows need S1's `participantId` (C-1).

### 5.4 S4 INLINE-PROFILE
- **`settings_routes_equivalence`** (`("brahmaConsole","PUT")` at `:262-270`). Grow it with: a body carrying `inlineConnectionProfileId`; a non-UUID id (400); explicit `null`; a partial body preserving a stored id; and a stored invalid id on GET (v4 falls back to `{maxAgentTurns: 50}`).
- **`projects_routes_equivalence`** / **`projects_tier2_equivalence`**: the key in schema order, null preservation, and the PUT uuid gate.
- **`backup_uuid_remap_equivalence`**: the project field, and the `brahmaConsole` JSON (valid / invalid JSON / non-string id).
- **`qtap_import_equivalence`**: the remap through `connectionProfiles`.
- **`carina_query_tier3_equivalence`** / **`brahma_console_tier3_equivalence`**: NEW rows for project, global and default resolution, and for a stale id falling through (each WARN).

### 5.5 Import-dead census (this area)

No v4 export was deleted in these four commits. `resolveBrahmaConnectionProfile` is still exported (`orchestrator.service.ts`), and only `one-shot.service.ts` stopped importing it. The `chats-search.ops.ts` return type changed from an inline type to `GlobalMessageSearchHit`, which is type-only. Re-grep at setup; the recipe is in the memory note "Setupphase IMPORT-DEAD census".

## 6. SPA (for the SPA lane — files + wire)

- **`c539c82e0`:**
  - The Settings → Chat → Brahma Console picker (`components/settings/chat-settings/BrahmaConsoleSettings.tsx` +138, and NEW `components/brahma-console/InlineBrahmaProfileSelect.tsx` 65) goes to `apps/web/src/app/screens/settings/chat/brahma-console-settings.ts` and `.api.ts`.
  - The project Model Behavior card (`app/prospero/[id]/components/ModelBehaviorCard.tsx` +20, `useProjectDetail.ts` +23, `types.ts`, `ProjectDetailView.tsx`) goes to `apps/web/src/app/screens/prospero/cards/project-model-behavior-card.ts`.
  - Types go to `apps/web/src/app/core/core-contract.ts`.
  - The wire: the GET / PUT gain `inlineConnectionProfileId`; the project PUT gains `brahmaConnectionProfileId`.
- **`4f5047241`:**
  - The header search bar's "This conversation" group, with `components/search/search-bar.tsx` +92, `search-dialog.tsx` +52, `search-results.tsx` and NEW `conversation-matches.tsx`. These go to `apps/web/src/app/search/search-bar.ts`, `search-dialog.ts`, `search-results.ts`, `search.logic.ts:20-21` (retire the named `?msg=` deferral) and a NEW conversation-matches component.
  - The client matcher, NEW `lib/chat/conversation-search.ts` (134): markdown-stripped, case-insensitive, newest first, TOOL rows skipped.
  - The Salon reveal and highlight (`useSearchHighlight` NEW 87, `useAutoScroll`'s `isInitialScrollDone`, `SalonView.tsx` +186, `page.tsx`) go to `apps/web/src/app/screens/salon/salon-conversation.ts`. It uses the CSS Custom Highlight API (`app/globals.css` +14).
  - The page-toolbar slot (`page-toolbar-provider.tsx`, `tab-toolbar.tsx`) goes to `apps/web/src/app/layout/page-toolbar.ts`.
  - The `?msg=` readers (`WorkspaceLinkInterceptor`, `useWorkspaceNavigate`, `WorkspaceIntent`) go to the workspace link interception (`p4.9j`).
  - `message-navigation.ts`'s `requestMessageReveal` / `takePendingMessageReveal` (sessionStorage key `pendingMessageReveal`, event `quilltap:reveal-message`).
  - No server dependency.

## 7. The lane split, by FILE OWNERSHIP

| lane | wave | commits | owns (v5) | oracle / harness |
|---|---|---|---|---|
| **S1 FTS** | 0, from `main` | `902c6efe9` whole + `c539c82e0` §1.4.4 | `db/chat_message_fts.rs`, `db/chat_message_fts_reconcile.rs`, `db/chats_search.rs`, NEW `db/chat_message_fts_character_only_repair.rs` (the stamped ensure), the `host.rs` insertion before `:1727` + `restore_vintage_state`'s list (**shared with the schema survey's boot-ensure lane — one insertion point, R-1**) | `cases/chat-message-fts.test.ts`, `chat-message-fts-reconcile.test.ts`, `chats-search.ts`, `ui-search.test.ts`, NEW ensure case; their `tests/*.rs` |
| **S3 BRAHMA-TOOLS** | 0, from `main`; FIRST commit `CATALOG:` | `c539c82e0` §1.4.1–1.4.3 + `6121f59b9` §1.1.9 (the catalog only) | `tools/definitions/data.rs` (CATALOG), NEW `tools/brahma_lookup.rs`, `tools/mod.rs`, `tools/executor.rs` (the four arms + `BUILT_IN` lists ONLY), `tools/effects.rs`, `services/tool_build.rs` (the `sql_access` arm ONLY), `tools/run_sql.rs`, `services/brahma_console/prompt_text.rs` (regen), `api/zod_issues.rs` (four lookup schemas' issue texts — a NEW section) | `cases/tool-definitions.ts`, `gen-brahma-prompts.mjs` re-run, `state-sql-tools.test.ts`, NEW `brahma-lookup-tools.test.ts` |
| **S2 SEARCH** | 1, from `CATALOG` | `6121f59b9` (minus native prompt, minus catalog) | `db/memories_read.rs` (one new fn), `services/memory_index.rs`, `services/recall/{gather,score,types,assemble}.rs` (assemble: the retrospective block only), `services/memory_recap/mod.rs` (`relative_path`), NEW `services/chat_window.rs` + `services/mod.rs` line, `db/conversation_search.rs`, `tools/search.rs`, `tools/executor.rs` (`run_search` ONLY), `tools/simple_json_prompt.rs` | `cases/search-tools-search.test.ts`, `recall-assemble-tier3.test.ts`, NEW `chat-window` case, `memory_index_tier2` row, `pseudo-tool-prompts.ts` |
| **S4 INLINE-PROFILE** | 0, from `main` | `c539c82e0` §1.4.5 | NEW `services/brahma_console/inline_profile.rs`, `services/brahma_console/mod.rs` (`run_brahma_query`'s step 1 ONLY), `db/instance_settings.rs` (the Brahma block `:430-525`), `api/settings.rs` (the Brahma GET / PUT), `api/types.rs` (the Brahma variant + decoder), `db/projects.rs`, `api/projects.rs` (the field-rule row), `api/zod_issues.rs` (`zod_project_properties_issues` ONLY), `services/qtap_export/schema-key-order.json` (project), `services/backup/uuid_remap.rs`, `services/quilltap_import/reconcile.rs` | `settings_routes`, `projects_*`, `backup_uuid_remap`, `qtap_import`, `carina_query_tier3` / `brahma_console_tier3` rows |
| SPA | any | §6 | §6 files | Playwright beats (gated until S4 lands for the pickers) |
| vendor | any | help + docs | `help/episodic-memory.md`, `memory-recall-relevance.md`, `search.md`, `brahma-console.md`, `carina.md`, `project-settings.md`; `docs/v4/…/DDL.md`; `public/schemas/qtap-export.schema.json` re-vendor | — |

**Collisions with other areas (flag to the planner):**
- `tools/native_tool_prompt.rs`: **the agent-mode lane (`96156566b`) owns it** and lands the PIN's `NATIVE_TOOL_RULES`, including 4.12.1's rule 6. S2 does not touch it (R-5).
- `services/tool_build.rs`: `96156566b` makes `submit_final_response` research-only. S3 touches only the `sql_access` arm (`:357`). The hunks are distinct; cherry-pick by path.
- `tools/search.rs`: `0c47f4539` (the mount-index survey's `reach.ts` refactor) rewrites the knowledge/document tier walk (`build_knowledge_tiers`, `:760-…`, and the pool region). S2 rewrites the sources default, the window, the conversation arm, the INFO and the formatter. Either sequence them (S2 first, recommended, because its arms are larger) or cut by function and record the split in both orders.
- `tools/executor.rs`: S2 (`run_search`), S3 (the four arms, `BUILT_IN`) and possibly other areas. Region-disjoint; commit with `-- paths` (memory note: shared git index).
- `services/brahma_console/orchestrator.rs` and `services/agent_loop/one_shot_loop.rs`: `96156566b` and `604a082c4` edit them. S3 and S4 do not touch them.
- `api/zod_issues.rs`: S3 (a new lookup section) and S4 (the project function); possibly the character-kind area (`kind`). Section-disjoint.
- `api/projects.rs`: S4 (one row in the field table) and the mount-index area (`c17033f04` / `3d39c303c` / `0c47f4539` edit `projects/[id]/mount-points` / `scenarios` routes). Check whether those live in the same v5 file.
- `services/qtap_export/schema-key-order.json`, `uuid_remap.rs` and `reconcile.rs`: S4, plus any other vault-key area (character kind). Key-disjoint.
- `host.rs` and `restore_vintage_state`: S1, plus the schema survey's ensures. One ordered list; R-1.

### Named shared contracts
- **C-1 (S1 → S3):** `ChatSearchRepository::search_messages_global` hits are `{messageId, content, chatId, role, participantId, createdAt}` (`participantId` is `null` when absent). S3's `lookup_messages` differential is union-red until S1 lands.
- **C-2 (S2 → S3):** `search_memories` with a window and no recall context runs the window pass and the ≤ 14-day floor relaxation. S3's `lookup_memories` rows are union-red until S2 lands. S3 must not edit `services/recall/*`.
- **C-3 (S2 → S3/S4):** `services/chat_window.rs` is S2's. No other lane re-implements chat spans.
- **C-4 (agent-mode lane):** `tools/native_tool_prompt.rs` carries 4.12.1's rule-6 sentence verbatim (`native-tool-prompt.ts:34` at the pin).

## 8. Tiering
- Tier 1: `fitToBudget`, `chatSpanOf` / `chatOverlapsWindow`, `isNarrowToolWindow`, the census formatters (six empty texts and the footer), `snippetAround`, `clip`.
- Tier 2: `findIdsInEventWindow`, the index `includeIds`, `findChatsInWindow`, the FTS ensure / stale replace / rebuild, `search_messages_global`, the four lookups, inline-profile resolution, the settings / project / remap / import carriers.
- Tier 3: the Brahma one-shot slate and prompt, the inline `@Brahma` profile in `carina_query`, recall assemble with a window.

## 9. Fixtures

There is no new D23 pair. Plants:
- Staff rows (`systemSender` / `customAnnouncer`) in the FTS and search corpora.
- A pre-`902c6efe9` trigger text for the stale replace.
- Memories with `occurredAt` NULL / Z / date-only / offset (R-3).
- Chats with `lastMessageAt` NULL and `updatedAt` bumped.
- A project `properties.json` carrying `brahmaConnectionProfileId` (UUID / null / absent).
- `instance_settings.brahmaConsole` = `{maxAgentTurns, inlineConnectionProfileId}` / an invalid id / bad JSON.

Recipe headers must pass `--v5w ~/source/quilltap-v5` (memory note: the spaced path breaks every regen).

## 10. Rulings owed to the human

- **R-1 — Who owns `902c6efe9`'s migration, and is it stamped? Recommend: S1 owns the ensure WITH the behaviour, STAMPED.**
  - The ensure's `shouldRun` IS the behaviour (`staleChatMessageFtsTriggers` + the new eligibility), so splitting them across lanes creates a same-file union obligation.
  - It must be stamped so v4's runner never re-runs it on a v5-touched instance (the ping-pong class; memory note "retired-ensure-arm ping-pong").
  - It runs before the PHASE 3.65 reconcile. The schema survey's lane only lists the id in the shared boot-ensure order.
  - Alternative: the schema lane takes all of `902c6efe9`.
- **R-2 — The Brahma settings blob. Recommend: port v4's schema whole.** Parse to `{maxAgentTurns, inlineConnectionProfileId?}` with v4's UUID check; strip unknown keys as Zod does; on a refused stored blob WARN and answer the default (v4's read). The PUT merges the parsed current value with the body, and the GET answers the parsed object.
  - This removes the measured erasure (Headline 9).
  - Alternative: keep the minimal schema and preserve unknown keys. That diverges from v4 on stripping and on the invalid-id fallback.
  - Side note: v5 has never emitted v4's `[Settings v1] Brahma console settings fetched / updated (instance-wide)` lines (`instance-setting-handlers.ts:67`, `:89`; no v5 hit). This is a pre-existing gap. Recommend recording it, not fixing it here.
- **R-3 — The lexical window in SQL against `Date.parse` in JS (Headline 6). Recommend: carry v4 verbatim**, and on the dogfood copy count `occurredAt` / `createdAt` values that are not `YYYY-MM-DDTHH:MM:SS.sssZ`. If any exist, draft a v4 issue for the human's go-ahead (the filing rule: ask first).
- **R-4 — Who regenerates `CATALOG`. Recommend: S3's first commit**, with S2 branching from it (the P4.D350 precedent). The alternative is a separate wave-0 micro-lane.
- **R-5 — `tools/native_tool_prompt.rs`. Recommend: the agent-mode lane (`96156566b`) owns it** and takes the pin's file, which already carries 4.12.1's rule 6 (C-4). The ledger row `:539` already says "take `main`'s file".
- **R-6 — The four lookups' read classes. Recommend: v4's.** The handlers catch every throw into `<tool> failed: <msg>`, so v5 propagates `?` up to that one catch per tool. They do not swallow per read, except `loadChats`, which DEBUG-logs and skips each failed chat (`:166-181`).

## 11. Dogfood rows

These run on a Friday COPY once v5 has caught up (memory note "Dogfood waits for catch-up").
- A dated `search` ("what happened last Tuesday") returns the census footer, and an empty dated search returns the honest text naming the period's conversations.
- An undated empty search now says `Nothing matched "…"`.
- On a ≤ 14-day window, a loosely matching in-window memory is returned.
- The retrospective mini-recap lists an unsummarized chat from the window.
- The first boot of the post-port v5 on a v4-migrated copy: no `out of step; rebuilding` WARN, and no trigger replace (the bytes are equal). Then a v4 boot afterwards does not replace triggers either.
- Global search no longer returns Commonplace whispers or Host announcements.
- The Brahma Console answers "how old is X" through `lookup_character` / `lookup_memories` / `lookup_messages` / `read_messages`. A wide `run_sql` comes back budgeted with the note.
- Inline `@Brahma` in a project chat with a project profile uses it. A deleted profile falls through with the WARN.
- A v5 PUT of the Brahma settings keeps `inlineConnectionProfileId`, and a v5 project edit keeps `brahmaConnectionProfileId`.
- A `.qtap` round trip and a `new-account` restore remap both ids.
