# Survey — `f7f3d7bf0`, half 2: the `renderedMarkdown` DROP + on-demand render + status + readers + client

Dated 2026-09-28. Read-only survey. v4 at `97b25fc53` (clean), baseline `acadcc7cd`, commit `f7f3d7bf0`
(`4.10.0-dev.96`). v5 `main` at `ef29a058e`. Out of scope, covered by the sibling survey: the cold-tier / warm-embeddings
half (collapse-stale-chat-caches, retention-constants, scheduled-maintenance, cold-chunk-reembed,
reconcile-embedding-dimensions, embedding-reindex, conversation-chunks.repository, settings.types, the
data-retention route + DataRetentionSettings.tsx).

---

## A. v4, hunk by hunk (post-commit line numbers at `f7f3d7bf0`)

### A.1 `migrations/scripts/drop-chat-rendered-markdown.ts` (NEW, 111 lines) + `migrations/scripts/index.ts` (+6)

- `:34` `MIGRATION_ID = 'drop-chat-rendered-markdown-v1'`
- `:38` description `'Drop chats.renderedMarkdown; transcripts are rendered on demand'`
- `:39` `introducedInVersion: '4.10.0'`
- `:40` **`dependsOn: ['add-rendered-markdown-field-v1']`**. It does NOT depend on `drop-chat-concierge-override-v1`.
  The only "after the Concierge drop" ordering is its **array position**: `index.ts:318-322` adds the import after
  `dropChatConciergeOverrideMigration`, and `:837-838` (the `migrations` array) plus `:1241-1242` (the named export
  block) append it LAST, after `dropChatConciergeOverrideMigration`.
- `shouldRun()` `:42-50`: `false` unless SQLite backend; `false` unless `sqliteTableExists('chats')`; then
  `return sqliteColumnExists('chats', 'renderedMarkdown')`. So it is idempotent, with no marker row of its own.
- `run()` `:52-110`:
  - `:57-62` a count query run first:
    `SELECT COUNT(*) AS n, COALESCE(SUM(LENGTH("renderedMarkdown")), 0) AS bytes FROM "chats" WHERE "renderedMarkdown" IS NOT NULL`
  - `:64-68` `logger.debug('Dropping the stored renderedMarkdown column from chats', { context: 'migration.drop-chat-rendered-markdown', renderedChats: n, bytes })`
  - `:70` **`db.exec('ALTER TABLE "chats" DROP COLUMN "renderedMarkdown"')`**. It has no guard, no backfill and no
    table rebuild. The header `:15-17` says no index, trigger or view names the column.
  - `:73` `mb = (bytes / 1048576).toFixed(1)`. `mb` appears only in the result `message`, never in the log.
  - `:74-79` `logger.info('Dropped the stored renderedMarkdown column from chats', { context, renderedChats: n, bytes, durationMs })`
  - `:81-90` success result: `itemsAffected: n`, message
    `` `Dropped chats.renderedMarkdown (${n} stored transcript${n === 1 ? '' : 's'}, ${mb} MB). ` + `Run 'npx quilltap db optimize' to shrink the file.` ``
  - `:91-109` catch: `logger.error('Failed to drop the renderedMarkdown column', { context, error })`, then
    `success: false, itemsAffected: 0, message: 'Failed to drop chats.renderedMarkdown', error`.
- Header `:17-19`: an old `.qtap` or backup that still carries the field is "stripped on import", because the chat
  schema no longer declares it.
- ⚠ **Not touched:** `migrations/scripts/sqlite-initial-schema.ts:125` still creates `"renderedMarkdown" TEXT DEFAULT NULL`
  in the hand-written base `chats` table. A v4 instance born from `SQLITE_TABLES` therefore gets the column and relies
  on the migration to drop it. The `generateDDL` surface (v5's D23 source) will not have it.

### A.2 `lib/schemas/chat.types.ts` (−6)

- The field `renderedMarkdown: z.string().nullable().optional()` and its doc comment
  `/** Scriptorium: deterministic Markdown rendering of the full conversation */` are removed from BOTH
  `ChatMetadataSchema` (was `:988-989`) and `ChatMetadataBaseSchema` (was `:1367-1368`). The field sat between
  `sceneState` and `equippedOutfit`.

### A.3 `lib/scriptorium/render-chat.ts` (NEW, 68 lines) — `renderChatConversation`

- `:38-40` signature: `renderChatConversation(chat: Pick<ChatMetadata, 'id'|'title'|'createdAt'|'updatedAt'|'participants'>): Promise<RenderedConversation | null>`
- `:31` logger `createServiceLogger('Scriptorium:RenderChat')`
- `:44` `events = await repos.chats.getMessages(chat.id)`
- `:45-48` `if (events.length === 0) { logger.debug('No events to render', { chatId }); return null; }`
- `:50` `speakerNames = await resolveSpeakerNames(chat)`. This is bug 161's ONE resolver (`lib/chat/speaker-names.ts`):
  - It walks every participant.
  - It skips seats with no `characterId`, and ids it has already named.
  - It calls `characters.findByIdRaw`, and sets the name only when `character?.name` is truthy.
  - It swallows throws.
  - It gives **NO `'User'` fallback**.
- `:52-57` `renderConversationMarkdown(events, chat.participants, speakerNames, { conversationId: chat.id, title: chat.title, createdAt: chat.createdAt, lastUpdatedAt: chat.updatedAt })`.
  It does call `lib/scriptorium/markdown-renderer.ts`, which changed by exactly one line: the `characterNames`
  param type at `:223` became `ReadonlyMap<string,string>` (it was `Map`). No behaviour moved.
- `:59-65` `logger.debug('Rendered conversation', { chatId, events: events.length, interchanges: result.interchanges.length, markdownLength: result.markdown.length, durationMs })`
- It returns `RenderedConversation` (`{ markdown, interchanges[] }`), the renderer's own output shape, unchanged.

**Is the on-demand render byte-identical to the old stored column? NO, for three reasons.**

1. **The wall clock.** `markdown-renderer.ts:364` does `nowText = formatDateTime(new Date().toISOString())`, and
   `:380` emits `` `Current time: ${nowText}. You are reading history, not in active conversation.` `` in the header.
   Every render carries the wall clock of its own moment. The stored column held the render-job moment; a live render
   holds the read moment. v5's own `embedding_remainder_equivalence.rs:39` records this line; it compares only because
   the clock is frozen on both sides.
2. **`lastUpdatedAt`** comes from `chat.updatedAt` at read time, not at render time. A stored column could also be
   stale against later messages.
3. **The speaker map changed.** The OLD handler (`acadcc7cd:conversation-render.ts:33-46`) did three things the new
   path does not:
   - It used `characters.findById`. That is the overlay read; `name` is not a vault-managed field, so the name matches.
   - It set **`'User'` for a `controlledBy === 'user'` seat with no resolvable character**. The new path leaves such a
     seat unmapped, and the renderer's fallback (`markdown-renderer.ts:263-270`) then gives `'User'` only for
     `role === 'USER'` and **`'Assistant'` for an ASSISTANT-role message from that seat**. So names differ on that edge.
   - It let a character-read throw propagate. The new resolver swallows it.

   The test pins the change: `conversation-render.test.ts:140-141` now reads
   `expect(characterNames.has('participant-2')).toBe(false)`, with the comment
   "A seat with no character is left for the renderer's own 'User' label".

v4 has no test proving byte-identity, and it cannot have one because of the clock. The help page's claim
("identically every time", `help/scriptorium.md:11`) is false on the `Current time:` line.

### A.4 `lib/scriptorium/status.ts` (NEW, 26 lines) + `__tests__/unit/lib/scriptorium/status.test.ts` (NEW, 18 lines)

- `:16` `export type ScriptoriumStatus = 'none' | 'rendered' | 'embedded'`; `:18-21` `interface ChunkCounts { total: number; embedded: number }`
- `:23-26` the whole rule:
  ```ts
  export function deriveScriptoriumStatus(counts: ChunkCounts | undefined): ScriptoriumStatus {
    if (!counts || counts.total === 0) return 'none';
    return counts.embedded >= counts.total ? 'embedded' : 'rendered';
  }
  ```
- Test rows: `undefined`→none; `{0,0}`→none; `{14,12}`→rendered; `{3,0}`→rendered; `{24,24}`→embedded.
- **The semantics moved, and the commit is silent on it.**
  - Old: `hasRenderedMarkdown ? (chunks>0 && embedded>=total ? embedded : rendered) : none`.
  - A chat with chunks but a NULL column was `none` before and is now `rendered`/`embedded`. That is every chat the old
    stale sweep NULLed.
  - A chat with the column but zero chunks was `rendered` before and is now `none`.

### A.5 `lib/background-jobs/handlers/conversation-render.ts` (+14/−36)

- Header `:5-6`: "…storing its interchange chunks for embedding. The Markdown itself is not persisted; see render-chat.ts."
- `:12` imports `renderChatConversation` instead of `renderConversationMarkdown`.
- `:24-32` `findById` + the WARN `'[ConversationRender] Chat not found, skipping'` are unchanged.
- `:37-44` NEW: `result = await renderChatConversation(chat)`; `if (!result)` gives the NEW debug line
  `'[ConversationRender] Chat has no events, nothing to render', { jobId, chatId }` and returns. The old path returned
  silently.
- **Removed:** the private name map (the old steps 2+3) and **step 5 `repos.chats.update(chatId, { renderedMarkdown })`**.
  The job no longer writes the chat row at all; the test at `:143-144` asserts `mockUpdateChat).not.toHaveBeenCalled()`.
- Steps 3-4 are unchanged (chunk upsert loop `:46-56`, the default-profile-only embed enqueue `:58-98`), as is the INFO
  `'[ConversationRender] Conversation rendered successfully' { jobId, chatId, interchangeCount, markdownLength, durationMs }` `:101-107`.
  It does not re-chunk differently and does not re-embed differently.
- The events read moved inside `renderChatConversation`, which now runs AFTER the name resolution. Old order:
  findById, then N character reads, then `getMessages`. New order: findById, then `getMessages`, then N `findByIdRaw`.

### A.6 `lib/services/chat-enrichment.service.ts` (+12/−21)

- `:31` imports `deriveScriptoriumStatus, type ScriptoriumStatus`.
- `:56-58` doc: `conversationChunkCounts` is now "The sole input to `scriptoriumStatus`".
- `:237-238` `EnrichedChatSummary.scriptoriumStatus: ScriptoriumStatus` (was the inline union). The **key is unchanged**.
- `enrichChatForList` `:591-597`:
  - With preload: the preloaded map (unchanged).
  - **Without preload: ALWAYS** `(await repos.conversationChunks.countByChatIds([chat.id])).get(chat.id)`. Before, it
    ran `findByChatId` + filter, and only when `hasRenderedMarkdown`.
  - Then `scriptoriumStatus = deriveScriptoriumStatus(chunkStats)`.
- `enrichChatsForList` `:663-676`: `renderedChatIds` is deleted, and `countByChatIds(chatIds)` now runs over **every**
  chat id (`chatIds.length > 0 ? … : empty Map`).
- `countByChatIds` (unchanged, repo `:78-108`) is a `safeQuery` with an empty-Map fallback:
  `SUM(CASE WHEN embedding IS NOT NULL …)`, with `Number(r.embedded ?? 0)`.

### A.7 `app/api/v1/chats/[id]/handlers/get.ts` (+6/−4) — comment only

- `:256-261` the re-warm comment is reworded ("Un-embedded chunk re-warm: … a leftover from before chunk embeddings
  were kept warm unconditionally …"). **No code change, no key change.** The chat GET never read the column.
  `...chat` spreads exist only in the ST export (`:151`) and the create response (`route.ts:1520`); once the schema
  strips the field, the key simply stops appearing there.

### A.8 `app/api/v1/characters/[id]/handlers/get.ts` (+7/−13)

- `:26` imports `deriveScriptoriumStatus`.
- `:161-164` NEW: ONE grouped read per page, `chunkCounts = await repos.conversationChunks.countByChatIds(paginatedChats.map(({chat}) => chat.id))`.
  It runs before the `Promise.all` enrichment.
- The per-chat `hasRenderedMarkdown` + `findByChatId` block is removed (was `:172-180`).
- `:224` `scriptoriumStatus: deriveScriptoriumStatus(chunkCounts.get(chat.id))`. The key is unchanged.

### A.9 `lib/tools/handlers/read-conversation-handler.ts` (+9/−5)

- `:18` imports `renderChatConversation`.
- `:99-107`, after the unchanged findById (`'Conversation not found.'`) and participation guard:
  `rendered = await renderChatConversation(chat); if (!rendered || rendered.interchanges.length === 0) return { success: false, error: 'Conversation has no messages to read yet.' }`
  - The **error string changes** (was `'Conversation has not been rendered yet.'`).
  - The **gate changes**: it was a falsy column; it is now null or zero interchanges. A render with a header and no
    interchanges was a success before and is a failure now.
- `:109` `markdown = rendered.markdown`; everything downstream (strip/merge, slicing, counts) is unchanged.
- Test (`read-conversation-handler.test.ts`):
  - Mocks `@/lib/scriptorium/render-chat`, with a `rendered(markdown)` helper that sets one interchange per
    `## Interchange` header.
  - NEW case `'renders the transcript live rather than reading a stored copy'`: it asserts the mock is called with the
    chat object and the result is `{ success: true, markdown, messageCount: 1, interchangeCount: 1 }`.
  - The not-rendered case becomes `'returns a no-messages error when the chat has nothing to read'`, using
    `rendered('')` and expecting the new string.

### A.10 `lib/tools/handlers/upsert-annotation-handler.ts` (+10/−7)

- `:17` imports `renderChatConversation`.
- `:83-95`, after findById (unchanged `'Chat not found.'`):
  `rendered = await renderChatConversation(chat); messageMatches = rendered?.markdown.match(/^### Message \d+/gm); messageCount = …length ?? 0; if (messageCount === 0) return { success:false, message_index, error: 'Conversation has no messages to annotate yet.' }`
  - The **error string changes** (was `'Conversation has not been rendered yet.'`).
  - The **edge changes**: a column with zero `### Message` headers used to fall through to the out-of-range error
    (`message_index >= 0`). It now returns the no-messages error.
- The out-of-range branch that follows is unchanged. No test file for this handler is in the commit.

### A.11 `lib/startup/reconcile-conversation-rendering.ts` (+19/−50) — only arm A is this half's

- Arm (A) `:87-96`: **`c."renderedMarkdown" IS NULL` → `NOT EXISTS (SELECT 1 FROM "conversation_chunks" cc0 WHERE cc0."chatId" = c."id")`**
  AND the unchanged USER/ASSISTANT `chat_messages` EXISTS. Arms (B)/(C) are unchanged.
- (Sibling half) the stale gate is removed: the `isStale` / `resolveStaleChatDays` / `retentionCutoff` imports, the
  loop's staleness `try`, the WARN `'Staleness check failed during reconciliation; skipping chat'`, and the
  `skippedStale` field on the result and on the completion INFO.
- NEW test `'keys arm (A) on messages with no conversation_chunks rows at all'` asserts the SQL `not.toContain('renderedMarkdown')`,
  contains `'NOT EXISTS'`, `'"conversation_chunks" cc0'` and `` `m."role" IN ('USER', 'ASSISTANT')` ``.
- Its ARM-A consequence: a chat with messages, chunks and a NULL column no longer matches arm A. A chat with a column
  and no chunks now does.

### A.12 `components/chat/ChatCard.tsx` (+89/−78)

- `:26` imports `Tooltip` from `@/components/ui/Tooltip`; `:29` imports `type ScriptoriumStatus`.
- `ChatCardData.scriptoriumStatus?: ScriptoriumStatus` `:92-93`. Its doc now reads
  "none = no chunks yet, rendered = chunks awaiting embeddings, embedded = fully indexed". **The DTO field set is
  unchanged**: the card reads `messageCount`, `memoryCount`, `scriptoriumStatus`, `isAutonomous`, `conciergeState`, …
  exactly as before. The list response still carries `scriptoriumStatus` (now chunk-derived, A.6).
- **NEW tooltip strings** (`:98-103`, `SCRIPTORIUM_TOOLTIPS`). These are REWORDED, not just moved:
  - embedded: `'Scriptorium: transcribed and indexed, every word findable — click to re-render'` (was `'Scriptorium: Rendered and embedded — click to re-render'`)
  - rendered: `'Scriptorium: transcribed, the indexing still under way — click to re-render'` (was `'Scriptorium: Rendered but not fully embedded — click to re-render'`)
  - none: `'Scriptorium: not yet transcribed — click to render and index'` (was `'Scriptorium: Not yet rendered — click to render'`)
- Every `title=` on the card is replaced by a wrapping `<Tooltip content=…>`. The contents are unchanged:
  `"Messages"`; `"Memories — click to delete and re-extract"`; the Scriptorium table;
  `"Autonomous character-to-character room"`; `"Copy link to this chat"`;
  `'Delete chat'` / `'Remove from project'`.
- NEW aria-labels:
  - memory button: `` `${chat.memoryCount} memories — delete and re-extract` ``
  - scriptorium button: `SCRIPTORIUM_TOOLTIPS[status]`
  - action button: `'Delete chat'` / `'Remove from project'` (it had only a title before)
  - copy-link keeps its existing aria-label.
- Classes are unchanged.

### A.13 Small files

- `lib/startup/prettify.ts:202` NEW label: `'drop-chat-rendered-markdown-v1': 'Clearing the duplicate transcripts from the shelves…'`.
- `instrumentation.ts:929-931` is a comment-only rewording of the dimension-reconcile note (sibling half's subject).
- `docs/developer/DDL.md`:
  - `:564` deletes the `"renderedMarkdown" TEXT DEFAULT NULL` line from `chats`.
  - `:774` NEW paragraph: chunks are the only stored form; the Markdown is rendered on demand; status is derived from
    chunks; the column was dropped by `drop-chat-rendered-markdown-v1`.
  - `:785` `conversation_chunks.embedding` row: "NULL only while an embed is pending or has failed … Never cleared by
    the stale-chat sweep (4.10+)."
  - `:1598` the `dataRetention` bullet: `renderedMarkdown` leaves the collapse list, and the "not touched (4.10+)"
    sentence is added.
- `docs/developer/API.md` (the data-retention GET prose) belongs to the sibling half.

### A.14 Help pages (one line per changed paragraph)

- `help/chats.md:431`: the Semantic Search paragraph gains two sentences. Untouched conversations stay findable, and
  the card's document badge links `scriptorium.md#the-scriptorium-badge`.
- `help/chat-settings.md:141`: the Data Retention paragraph drops "semantic-search embeddings" from the tidied list and
  adds "semantic-search embeddings are always kept warm regardless of this window".
- `help/data-retention.md`:
  - The intro paragraph (`:9`) drops embeddings, and a new paragraph (`:11`) says embeddings are kept warm.
  - The "Conversation embeddings" bullet is deleted, and "conversation embeddings" is added to the "Never touched" line.
  - The section `## Search while a chat is cold` and its two paragraphs are replaced by `## Search stays warm, always`
    plus one paragraph.
- `help/embedding-profiles.md:183`: the "Every startup audits the ledger" bullet's last sentence changes. Dormant chats
  are now treated the same as busy ones.
- `help/scriptorium.md`:
  - `:11` "How It Works": the full manuscript is not kept and is written out afresh.
  - `:25` NEW paragraph: chunks are kept for every conversation.
  - `:27-35` NEW section `### The Scriptorium Badge`, with Green/Amber/Red bullets and the click behaviour.
  - `:73` the `read_conversation` paragraph: "written out fresh … even a long-quiet chat is always legible".

  ⚠ The `help/` tree must be re-vendored (count unchanged; files byte-copied).

### A.15 What the commit message claims that the hunks do NOT do, or leave unsaid

1. **"Byte-identical" is not claimed, and would be false.** The help's "identically every time" is contradicted by
   the renderer's `Current time:` header line (A.3).
2. **The render's speaker map changed** (the `'User'` fallback for character-less user seats is gone, and the reads are
   raw). The message does not say so.
3. **Two tool error strings changed, and one gate moved.** `read_conversation` now fails on zero interchanges;
   `upsert_annotation` returns the no-messages error where the out-of-range error used to fire. The message does not
   say so.
4. **The status semantics inverted on two edges** (A.4). The message says only "derived from chunks alone".
5. **The chat GET is comment-only.** The message lists it among the readers, but it never read the column.
6. **The migration's ordering is array-only.** `dependsOn` is `add-rendered-markdown-field-v1`, not the Concierge drop.
7. **The ChatCard tooltip COPY was reworded** and aria-labels were added. The message mentions only the mechanism
   ("in-app Tooltip").
8. **`sqlite-initial-schema.ts` still creates the column.**
9. **The migration's `mb` figure is only in the result message, not the log.**

---

## B. v5 on `main` — every `renderedMarkdown` site

`ggrep` finds **42 Rust hits** (the order said 37; the extra five are `rendered_markdown` snake-case), **0 in the SPA**,
and 11 oracle-side files.

### B.1 Production Rust

| file:line | what it does |
|---|---|
| `crates/quilltap-core/src/db/chats_read.rs:93` | **BIND/READ** — `ALL_COLUMNS` (the strict explicit SELECT list; its doc says "All 102 columns"). `renderedMarkdown` sits between `sceneState` and `equippedOutfit`. |
| `db/chats_read.rs:297` | **READ** — `put_opt_string(&mut obj, "renderedMarkdown", row.get(62)?)`. Index 62; every later index (63…) shifts down by one on removal. |
| `db/chats.rs:388` | `ChatCreate.rendered_markdown: Option<String>` (`#[serde(default)]`). ⚠ `services/quilltap_import/entities.rs:772` deserializes a whole `.qtap` chat object into `ChatCreate`, so today a v4 bundle's `renderedMarkdown` is **written on import**. v4 now strips it. Removing the field makes serde ignore the key (no `deny_unknown_fields` on the struct). |
| `db/chats.rs:788-793` | `ChatUpdate.rendered_markdown` (the P4.6BM block), whose only writer is the render job. |
| `db/chats.rs:876` + `:956` | **WRITE** — the INSERT column list plus the `data.rendered_markdown` param. The placeholder is **`?59`** (`sceneState ?58`, then `renderedMarkdown ?59`, then `equippedOutfit ?60` … `?98`, with `?99`-`?102` spliced). Removing it means renumbering `?60`…`?98` down by one, **exactly the P4.D227 treatment** of `conciergeOverride` (was `?58`; the comment at `:845-857` records it: "every placeholder above 58 was renumbered down by one mechanically, and the create arm proved it"; `chats_tier2_equivalence`'s create arm is the catch). |
| `db/chats.rs:1361-1362` | **WRITE** — `set_col!("renderedMarkdown", …)` in `update`. |
| `services/conversation_render_job.rs:6,24,89,207` | **WRITE** — step 5 persists the markdown with `updated_at: None` (`:196-208`). It also carries the old private name map (`:135-167`, the overlay `characters_read::find_by_id` plus the `'User'` fallback). **All of it becomes `resolve_speaker_names`.** v5 already has the resolver at `services/speaker_names.rs:82`, and `SpeakerNames::entries()` returns exactly the `&[(String,String)]` that `render_conversation_markdown` (`services/conversation_markdown.rs:406`) takes. ⚠ The docstring `:36-38` claims "isDefault OR, failing that, the FIRST row", but the code `:246-257` is default-only (stale doc). ⚠ v5 does NOT emit v4's INFO `'[ConversationRender] Conversation rendered successfully'`, which was already absent before this commit. The new DEBUG `'…Chat has no events, nothing to render'` is also absent. |
| `services/conversation_render_reconcile.rs:10,41,108` | **READ (SQL)** — arm (A) `c."renderedMarkdown" IS NULL`. It becomes `NOT EXISTS (… conversation_chunks cc0 …)`. The in-file test DDL `:336` and INSERT `:365` name the column. |
| `services/collapse_stale_chat_caches.rs:9,31,79,107,110` | **WRITE (SQL)** — `UPDATE chats SET compressionCache = NULL, renderedMarkdown = NULL, … AND (… OR renderedMarkdown IS NOT NULL …)`. This is the sibling half's file, but the SQL **FAILS with `no such column`** on a dropped instance, so the two halves must land together. |
| `services/chat_enrichment.rs:72,725-743,876-890` | **READ** — `has_rendered` (non-empty string) gates the no-preload fallback and the status. `rendered_chat_ids` restricts the batched `count_by_chat_ids`. The port: `derive_scriptorium_status(Option<(i64,i64)>)`; the fallback always counts; the batch runs over all `chat_ids`. |
| `api/characters.rs:624-641` | **READ** — the per-chat `has_rendered` (`!is_null()`, which ⚠ differs from v4's `!!` on `''`; moot once removed) plus `count_stats_by_chat_id`. v4 now makes ONE `countByChatIds(page ids)` per page. |
| `tools/read_conversation.rs:23,208-219` | **READ** — the column gate plus the `'Conversation has not been rendered yet.'` string. It becomes a live render: v5 needs `get_messages` + `resolve_speaker_names` + `render_conversation_markdown(…, now_iso)`, with a **clock injected into the tool path**, and the gate becomes `interchanges.is_empty()` with the new string. |
| `tools/annotations.rs:278-292` | **READ** — the column gate + string, then `count_message_headers(rendered)`. It becomes a live render with a `message_count == 0` gate and the new string. |
| `services/maintenance.rs:221` | Doc comment on the stale gate (sibling half). |
| `realtime/publish_sites.rs:1435` | Doc comment in a test (the collapse sweep list). |
| `quilltap-host/src/spine.rs:1678` | Comment on the post-turn render trigger ("keeps `renderedMarkdown` … current"). |
| `enclave/lifecycle.rs:1019`, `enclave/step.rs:1527` | Test-only hand DDL `chats` column lists. They can stay; with E.2 treatment they model the migrated shape. |
| `services/provisioning/fresh_schema.json` | The D23 dump (`"renderedMarkdown" TEXT` in `chats`). |
| `services/qtap_export/schema-key-order.json` | Export key order, `/chat` list index 64 of 103. ⚠ **It still lists `conciergeOverride`** (1 hit), so P4.D227 did NOT regenerate it. The table is non-lossy (`key_order.rs:14-17`), so this is harmless, but stale. |

### B.2 Tests

| file:line | what it does |
|---|---|
| `crates/quilltap-host/tests/host_boot.rs:125`, `host_cadence.rs:116` | Hand DDL `"renderedMarkdown TEXT"` (the `conciergeOverride` line is already commented DROPPED at P4.D227). |
| `crates/quilltap-harness/tests/collapse_stale_chat_caches_tier2_equivalence.rs:176,184` | The chats projection SELECTs `renderedMarkdown`. It must drop the column (sibling half's family). |
| `crates/quilltap-harness/tests/embedding_remainder_equivalence.rs:39` | Doc: the column compares byte-exact under a frozen clock. That comparand disappears. |

### B.3 The E.2 treatment to prescribe (cite)

- `db/chats_read.rs:64-76` (the doc on `ALL_COLUMNS`): `conciergeOverride` "LEFT the list … An explicit list, so an
  instance that still HAS the column … opens exactly like one without it — v5 never drops the column itself."
- The test `db/chats_read.rs:697-746`, `a_chat_reads_the_same_with_or_without_the_dropped_override_column`:
  - It builds the re-dumped fresh DDL and asserts the column is absent.
  - A second DB gets `ALTER TABLE ADD COLUMN` plus a value.
  - `find_by_id` must be equal on both DBs, the dropped key must be `is_none()`, and a neighbour must read correctly.
  - A `renderedMarkdown` twin is the exact prescription, with the neighbour pinned as `equippedOutfit`/`sceneState`.
- The alignment census (`chats_read.rs` "pinned against the D23 dump by this module's `alignment_census` tests") will
  trip until the index table shifts.
- INSERT renumbering, per `db/chats.rs:845-857`.
- The ledger §1 (`drift-ledger.md:76-90`) already expects "the same tolerate-both-shapes treatment".

### B.4 v5's existing status derivation

- Two inline copies (`chat_enrichment.rs:734-743` and `api/characters.rs:634-642`), with **no shared helper**. The port
  should add ONE `derive_scriptorium_status` (e.g. `scriptorium/status.rs` or in `services/chat_enrichment.rs`) and use
  it at both sites.
- The chunk counters are `db/conversation_chunks.rs:228` (`count_stats_by_chat_id`) and `:260` (`count_by_chat_ids`,
  chunked by `SQLITE_VARIABLE_CHUNK_SIZE`, a v5 scale measure).
- ⚠ v4's `countByChatIds` is a `safeQuery` (an error gives an empty Map, hence `none`). v5 propagates `Err`.
  This is pre-existing; it becomes reachable on the character GET now.

### B.5 SPA

- `apps/web/src/app/ui/scriptorium-badge.ts`, the shared badge for BOTH cards (`screens/salon/chat-card.ts:96-100` and
  `screens/characters/view/tabs/character-conversation-card.ts:92-95`):
  - It uses `[title]="badgeTitle()"` with the three OLD strings (`:55-64`).
  - The port: wrap the badge in `<qt-tooltip>` (`apps/web/src/app/ui/tooltip.ts`, v4's `Tooltip` port; its docstring
    says the wrapped control should carry no `title`), switch to the NEW strings, and set `aria-label` to the same
    string.
  - Its spec is `ui/scriptorium-badge.spec.ts`.
- Other `title=` sites on the two cards:
  - salon `chat-card.ts`: `:57` Remove from project, `:82` Messages, `:90` Memories, `:115` Autonomous,
    `:124` Copy link, `:166` Delete chat.
  - `character-conversation-card.ts`: `:78` Messages, `:86` Memories, `:142` Delete chat.

  ⚠ v5's memory badge is a static `"Memories"` span, not a click-to-reextract button (a recorded deferral at
  `chat-card.ts:28-30`). So v4's new memory aria-label has no v5 home unless that deferral is lifted.
- ⚠ **The e2e selector breaks.** `apps/web/e2e/m4-salon.spec.ts:92` finds the badge by
  `button[title^="Scriptorium:"]`. Moving to the Tooltip removes the `title`, so re-key it on `aria-label^=`.
- DTO: `core/core-contract.ts:2974,3717` has `scriptoriumStatus: 'none' | 'rendered' | 'embedded'`. The key and the
  union are unchanged; there is no DTO change.

### B.6 Committed real-DB pairs carrying `chats.renderedMarkdown`

- 49 `crates/quilltap-web/tests/fixtures/*-main.db` are tracked (109 `.db` files in all). All are encrypted with test
  peppers, so they were not opened.
- The column has been in `generateDDL` since `add-rendered-markdown-field-v1`, and in the hand base schema. Every
  chats-bearing pair therefore carries it, which is **P4.D227's list: the 41 chats-bearing pairs plus
  `llm-log-cleanup-main` = 42** (the `status-log.md` record near line 152682).
- It is measurable exactly with the migrator's `--report-only` `--module` run: `shouldRun()` reports `sqliteColumnExists`.
- The per-run builders that write the column through v4's repos:
  `build-embedding-remainder-fixture.ts:144,433`, `build-retention-caches-fixture.ts:178` (raw UPDATE),
  `build-salon-fixture.ts:138,484-485`, `build-tool-dispatch-fixture.ts:45,210`, `build-scriptorium-tools-fixture.ts:8`
  (spec chats passed to `create`).
- The JSON specs with the key: `chats-read-tier2.json:118`, `embedding-remainder.json` (13), `salon.json:247`,
  `scriptorium-tools.json` (6, incl. two `$comment`s), `tool-dispatch.json:25,36` (+ a `$comment` at `:100`).

---

## C. Families, fixtures and recipes that move

1. **`chats_tier2_equivalence`** (the create arm proves the INSERT renumber) and **`chats_read_equivalence`**
   (`chats-read-tier2.json:118` sets the column).
   - At the target, v4's read strips it (the schema no longer declares it), even though the fixture copy keeps the
     column: **no v4 migration runs in the oracles** (see D.3).
   - So v5 must not surface it either, which is exactly the E.2 removal from `ALL_COLUMNS`.
   - Regenerate both at the target. Red-first: at the baseline v5 emits the key and v4 at the target does not.
2. **`scriptorium_tools_equivalence`** (`harness/oracle/cases/scriptorium-tools.ts`, `build-scriptorium-tools-fixture.ts`,
   `scriptorium-tools.json`). **This is the biggest rework.**
   - The fixture seeds chats with a stored `renderedMarkdown` and **no `chat_messages` rows and no characters**.
   - At the target, v4 renders live, so every read and upsert returns the new no-messages errors.
   - The spec must gain real message rows (and character rows or names) that render to the intended headers.
   - The oracle must **freeze `Date`** (it does not today: grep finds no clock handling in the case), and v5 must
     inject the same `now_iso`.
   - Expected outputs change wholesale: the live render has v4's real header and `Current time:` line, not the
     hand-written `# Conversation` strings.
3. **`tool_dispatch_equivalence`** (`cases/tool-dispatch.ts`, `build-tool-dispatch-fixture.ts`, `tool-dispatch.json`):
   the `read_conversation` success row (`:61`) and the "not rendered" failure row (`:100`, whose string changes). Same
   clock and messages need as above.
4. **`embedding_remainder_equivalence`** (`cases/embedding-remainder.test.ts`, `build-embedding-remainder-fixture.ts`,
   `embedding-remainder.json` with 13 column values):
   - The render phase's `chats.renderedMarkdown` comparand disappears, since the job no longer writes the chat.
   - The reconcile's arm-A scenarios (`unrendered`, `empty`, `system-only`, the "stale text from an older render" row)
     re-key on chunk absence, so outcomes can move (see A.11).
   - It also covers the `speaker` map change (a `controlledBy:'user'` seat with no character, if the corpus has one;
     add an arm if it does not).
   - The sibling half's stale-gate removal also lands here.
5. **`collapse_stale_chat_caches_tier2_equivalence`** + `cases/collapse-stale-chat-caches-tier2.ts:80` +
   `build-retention-caches-fixture.ts:11,110,178`: the projection and the fixture UPDATE name the column. This is the
   sibling half's family; it must co-land.
6. **Salon families** reading `salon.json:247` (`salon_reads`, `salon_mutations`, `salon_skip`,
   `salon_swipe_generate`, `transcript_route` — the consumer list in B.6 grep). Only a projection key disappears. Re-run
   them; expect green once `ALL_COLUMNS` drops the key.
7. **The character GET / chat-list families** (the `characters` routes and chat-list enrichment families carrying
   `scriptoriumStatus`): the status is re-derived from chunks. Check that each corpus has a chunks-but-NULL-column and a
   column-but-no-chunks row. Add them if they are missing (they are red-first under the new rule).
8. **`provisioning_equivalence`** — the D23 re-dump.
9. **Host tests** `host_boot.rs`/`host_cadence.rs`: the hand DDL can keep the column (it models the migrated shape) or
   drop it.
10. **`conversation_markdown_equivalence`** (tier-1): the renderer is unchanged apart from a type change, so no movement
    is expected. Re-run it at the target as a neutrality proof.
11. **SPA**: `scriptorium-badge.spec.ts`, `e2e/m4-salon.spec.ts:92`, plus a new tooltip-string spec. Record v4's
    `SCRIPTORIUM_TOOLTIPS` bytes.
12. **`help_tree_equivalence`** after re-vendoring the 5 pages.
13. **The export key order**: regenerate `schema-key-order.json` with `harness/oracle/fixtures/dump-export-key-order.ts`
    at the target. That drops `renderedMarkdown` AND the stale `conciergeOverride`. Check whichever export family pins
    it, and the qtap-schema byte count guards if v4's vendored qtap JSON schema moved (it is not in this commit's file
    list, so it probably did not).

**D23 re-dump procedure** (`harness/oracle/provision/dump-fresh-schema.ts:28-32`, applied at P4.D226 and P4.D227 "run
FROM the pin"):

```
cd <pinned v4 worktree at f7f3d7bf0>          # never the live checkout
QT_SCHEMA_OUT=/tmp/qt-fresh-schema.json [QT_SEED_OUT=…] $N/npx tsx $W/harness/oracle/provision/dump-fresh-schema.ts
```

Copy the output to `crates/quilltap-core/src/services/provisioning/fresh_schema.json`, record the delta in
`provisioning/mod.rs`'s dated notes (`:95-125` is the pattern), and run `provisioning_equivalence` red-first then green.
The expected delta is exactly one line: `chats` loses `"renderedMarkdown" TEXT`. The seed should be unmoved (`cmp` it).

---

## D. Traps and premises to measure

1. **Fixture NARROWING through the real module.** It works as-is.
   - The migrator `harness/oracle/fixtures/migrate-memories-fixture-columns.ts` has the `--module <file>#<export>` arm
     (`:283-306` doc, `:548-578` `runModules`, `:586-603` argv). It runs v4's REAL `shouldRun()`/`run()` through
     `harness/oracle/lib/v4-migrations.ts`, whose header (`:5-9`) names "#76's DROP" as a supported shape.
   - P4.D227 already narrowed 42 pairs this way with `drop-chat-concierge-override`.
   - Recipe: from a pinned `f7f3d7bf0` worktree, run
     `--module migrations/scripts/drop-chat-rendered-markdown.ts#dropChatRenderedMarkdownMigration [--report-only] <every chats-bearing *-main.db>`.
   - Traps carried from P4.D227:
     - A read-write open re-encrypts page 1, so `git restore` the pairs the module left logically unchanged.
       Here, though, every chats-bearing pair SHOULD change.
     - `--report-only` runs on a scratch copy.
     - Journal mode is restored by the lib.
     - The migration-vintage trio was REBUILT at the pin.
   - `ALTER TABLE DROP COLUMN` needs SQLite ≥ 3.35; the sqleet binding bundles 3.53.2.
2. **P4.D233 F1 applies with full force.** A real-DB fixture built at a pin ≥ `f7f3d7bf0` has no
   `chats.renderedMarkdown`, and `main`'s strict `ALL_COLUMNS` fails on it (`no such column`).
   - Build per-run fixtures at the baseline pin `acadcc7cd` and run the oracles at the target, **until the E.2 removal
     lands**. After that, v5 reads both shapes.
   - Stack the lane's first commit as the E.2 read-side tolerance (the substrate) so every later family can use
     target-built fixtures.
   - The same applies to the Friday copy: a copy past `-dev.96` breaks v5's chat reads today (ledger §1).
3. **v4's jest / oracle DB init does NOT run migrations.** v4 runs migrations only from `instrumentation.ts`, via
   `migrations/index.ts`; `lib/` has no caller. `schema-translator.ts:527-548` `generateAlterStatements` only ADDs
   columns and WARNs on removed ones, and has no runtime caller.
   - So an oracle at `f7f3d7bf0` against a baseline-built fixture copy **keeps the column on disk**, and v4's reads
     silently strip it (the Zod schema no longer declares it).
   - It will NOT silently drop the column. It also means an oracle-side raw SQL naming the column still works at the
     target, so the collapse oracle's projection would not fail by itself. The v5 side must still stop naming it.
4. **The on-demand render is NOT byte-identical to the stored column** (A.3): the `Current time:` wall-clock line,
   `lastUpdatedAt` at read time, and the speaker-map change.
   - The ledger's trap line ("the on-demand render must match the old stored bytes exactly") is a **false premise**.
     The right proof is a frozen-clock differential of the live render against v4's live render, not a comparison
     against stored bytes.
   - v4's own reconcile test proves nothing about bytes (it asserts SQL substrings).
5. **Clock plumbing.** `read_conversation`/`upsert_annotation` in v5 now need `now_iso`. Check how the tool executor
   carries a clock; the render job already takes one (`conversation_render_job.rs:89-96`).
   - This is a new required parameter across the tool dispatch path (see the memory note on required fields crossing
     lane ownership).
6. **The status inversion edges** (A.4), dogfood-visible. On Friday, every chat the old sweep NULLed flips from red to
   amber/green on first boot, before the sibling half's re-embed drains.
7. **The `.qtap` import currently WRITES the key.** Once `rendered_markdown` leaves `ChatCreate`, the key is ignored,
   matching v4's strip. Pin it with an import arm carrying the key, and assert that the created row lacks it or has NULL.
8. **`sqlite-initial-schema.ts` still has the column.** It is irrelevant to v5, which provisions from `generateDDL`,
   but it explains why a v4 migration-accumulated instance shows the drop in its migration log.
9. **Co-landing.** `collapse_stale_chat_caches.rs`'s UPDATE and the reconcile's arm A both name the column in raw SQL.
   If this half removes the column from the fresh schema and the sibling half has not rewritten the collapse SQL, the
   sweep errors on fresh instances. Land both halves in one round, with one owner for `collapse_stale_chat_caches.rs`.
10. **The stale `schema-key-order.json`** carries `conciergeOverride` from before P4.D227. Regenerate once, with both keys.
11. **`api/characters.rs` → one grouped `count_by_chat_ids` per page.** Mind v4's `safeQuery` fallback versus v5's `Err`.
12. **The v5 render-job docstring** (`:36-38`, "isDefault OR first row") is stale versus its code. Fix it while
    rewriting the file.

---

## E. Size and tier

- **Core:** ~12 files.
  - `db/chats.rs` (INSERT renumber, two struct fields, update arm) and `db/chats_read.rs` (list, index shift, E.2 twin
    test, census).
  - A new shared `render_chat_conversation` (reusing `speaker_names` + `conversation_markdown`) plus a shared
    `derive_scriptorium_status`.
  - `conversation_render_job.rs`, `conversation_render_reconcile.rs` (arm A), `chat_enrichment.rs`, `api/characters.rs`,
    `tools/read_conversation.rs`, `tools/annotations.rs` (clock plumbing), doc comments.
- **Artifacts:** the D23 re-dump + key-order regen, a 42-pair narrowing, and the 5 help pages.
- **Families:** 4 reworked (scriptorium-tools and tool-dispatch are the heavy ones: new message corpora + a frozen clock),
  about 8 re-run.
- **SPA:** the badge + tooltip, 2 specs, 1 e2e selector.
- **Estimate:** one large lane (~1.5–2 days agent time), or two lanes. **A (Opus):** core + E.2 substrate + fixtures +
  families, stacked on a substrate commit. **B (Sonnet):** SPA badge/tooltip + help re-vendor.
- The sibling cold-tier half should be the SAME round, with the collapse file co-owned by lane A.
- **Recommended tier: the most capable model (Opus) for lane A.** It needs strict-SELECT renumbering, a DROP-direction
  fixture migration, and live-render differentials with clock injection. A cheaper agent is fine for lane B.

---

## Summary

1. The migration `drop-chat-rendered-markdown-v1` is a plain `ALTER TABLE "chats" DROP COLUMN "renderedMarkdown"`,
   gated by `sqliteColumnExists`. **It depends on `add-rendered-markdown-field-v1`; it is ordered after the Concierge
   drop only by array position.**
2. `renderChatConversation` = `getMessages` → `resolveSpeakerNames` (bug 161's resolver, raw reads, no `'User'`
   fallback) → the unchanged `renderConversationMarkdown`. It returns null for zero events.
3. **Surprise: the live render can never byte-match the stored column.** The header carries a wall-clock `Current time:`
   line, so the ledger's "must match stored bytes" trap is a false premise. The proof has to be a frozen-clock
   live-versus-live render.
4. **Surprise: the speaker map changed silently.** A user seat with no character now labels its ASSISTANT-role lines
   'Assistant' instead of 'User'.
5. `deriveScriptoriumStatus` works from chunks alone and inverts two edges: a NULL column with chunks is no longer
   'none', and a column with no chunks is now 'none'.
6. Two tool error strings changed ('…no messages to read yet.' / '…no messages to annotate yet.'), and both gates moved.
7. v5: 42 Rust hits. The E.2 precedent is `chats_read.rs:64-76,697-746` plus the INSERT renumber `chats.rs:845-857`
   (renderedMarkdown is `?59`, read index 62). The `.qtap` import currently writes the key via `ChatCreate`.
8. The heaviest movers are the `scriptorium_tools` and `tool_dispatch` fixtures. They seed stored markdown and NO
   messages, so at the target every read fails until they get real message corpora and a frozen `Date`.
9. The narrowing works through the existing `--module` arm (P4.D227 did the same to 42 pairs). v4 oracles run no
   migrations, so a target oracle keeps the column on its copy and strips it only on read. F1 holds until v5's E.2
   removal lands, so make that the substrate commit.
10. SPA: the badge strings are reworded (not just moved to the Tooltip). `e2e/m4-salon.spec.ts:92` selects by
    `title^="Scriptorium:"` and breaks. `schema-key-order.json` is also stale on `conciergeOverride` from P4.D227.
