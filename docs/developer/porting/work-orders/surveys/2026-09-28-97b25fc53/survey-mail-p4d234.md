# Survey — v4 `39bc98ffc` (read_mail + list_email→list_mail) and `12c336fad` (discard_mail)

Date: 2026-09-28. Read-only survey. v4 checkout `~/source/quilltap-server` at `97b25fc53` (clean);
oracle baseline `acadcc7cd`. All v4 line numbers are **post-commit at `12c336fad`** unless marked.
Classified from the hunks. The commit messages were checked against them; see A.12.

Commit order after the baseline: `acadcc7cd` → `39bc98ffc` (-dev.94) → `12c336fad` (-dev.95) →
**`f7f3d7bf0`** (the `chats.renderedMarkdown` DROP) → … → `97b25fc53`. So **`12c336fad` is the last
pin before the column drop.** Pin every fixture builder and oracle regen at `12c336fad` (C.3).

The ledger rows (drift-ledger.md:149–150) are **accurate**, with one clarification: the catalog count
ends at 61, which is 59 − listEmail + listMail + readMail + discardMail. Neither ledger row mentions
four things, and all four are traps: stored `disabledTools` ids (D.1), the protected-document bypass
in discard_mail (D.6), the orchestrator tools-at-wire family (C.1), and the absent v4 log lines (D.8).

---

## A. v4, hunk by hunk

### A.1 `lib/post-office/mailbox.ts` (the parser, plus discardLetter)

New imports: `deleteDatabaseDocumentIfExists` from `@/lib/mount-index/database-store` (line 29).
The logger stays `createServiceLogger('PostOffice:Mailbox')` (line 39), which the logging layer
renders as `{ service: 'PostOffice:Mailbox', module: 'service' }`.

**`export function letterFileName(path: string): string`** (lines 94–97)
```ts
const prefix = `${MAIL_FOLDER}/`;           // "Mail/"
return path.toLowerCase().startsWith(prefix.toLowerCase()) ? path.slice(prefix.length) : path;
```
It strips ONE leading `Mail/` (case-insensitive). Anything else comes back unchanged.

**`export function resolveMailPath(ref: string): string | null`** (lines 106–113). The order of
checks is load-bearing:
1. `ref.trim()` — this is JS trim, so port it with `jsstr::js_trim`, not `str::trim`.
2. `.replace(/^qtap:\/\/self\//i, '')` — anchored, case-insensitive, one occurrence.
3. `.replace(/^\/+/, '')` — strips leading slashes. **This runs AFTER the qtap strip.** So
   `/qtap://self/Mail/x` keeps its URI, then contains `/`, and resolves to **null**, while
   `qtap://self//Mail/x` resolves to `Mail/x.md`.
4. If `name.toLowerCase().startsWith('mail/')`, strip exactly 5 chars. This happens once: `Mail/Mail/x`
   becomes `Mail/x`, which still contains `/`, so it resolves to null.
5. `if (!name || name.includes('/') || name.includes('\\') || name === '.' || name === '..') return null;`
   Only exact `.` and `..` are refused. `...` and `.md` pass.
6. `if (!name.toLowerCase().endsWith('.md')) name = \`${name}.md\`` — so `x.MD` stays `x.MD`.
7. `return \`Mail/${name}\``

v4's tests (mailbox.test.ts): `111-from-ariadne.md` and `111-from-ariadne` both give
`Mail/111-from-ariadne.md`; the `Mail/…` and `qtap://self/Mail/…` forms resolve to the same path;
`Notes/secret.md`, `../secret.md`, `Mail/../secret.md`, `..` and `'   '` all give null.

**`export async function discardLetter(vaultId: string, path: string): Promise<boolean>`**
(lines 294–298, added in c2):
```ts
const deleted = await deleteDatabaseDocumentIfExists(vaultId, path);
logger.debug('discardLetter', { vaultId, path, deleted });
return deleted;
```
The module doc comment (lines 9–16) is rewritten. It now says everything is safe from the forked
child, and that discardLetter is replayed whole on the parent.

These pre-existing functions are unchanged: `readLetter` (null on NOT_FOUND),
`markAlerted` (warns `'markAlerted: letter no longer present'` `{vaultId, path}` on NOT_FOUND),
`listMailbox` and `collectUnalertedMail`.

The chokepoint these reach, in `lib/mount-index/database-store.ts` (unchanged, logger
`MountIndex:DatabaseStore`):
- `deleteDatabaseDocument` (176–191) runs `normaliseRelativePath`, then `findByMountPointAndPath`. If
  there is no link it returns false. Otherwise it calls `docMountFileLinks.deleteWithGC(link.id)`, then
  `emitDocumentDeleted` (the watcher at `watcher.ts:114` schedules a store re-embed), and returns true.
- `deleteDatabaseDocumentIfExists` (200–213) wraps it. It catches `DatabaseStoreError` NOT_FOUND,
  logs debug `'Document already absent on delete'` `{mountPointId, relativePath}`, and returns false.
  **This arm is unreachable from discard.** deleteDatabaseDocument never throws NOT_FOUND, and a path
  resolveMailPath returns can never trip `normaliseRelativePath`'s `..` throw.

### A.2 `lib/post-office/instructions.ts` (rewritten; final state lines 1–50)

The `formatSelfUri` import is gone at the end of c2. The file imports
`letterFileName, type DeliveredLetterSummary` from `./mailbox`.
```ts
export interface LetterActionOptions { includeRead?: boolean; }
export function formatLetterActions(letter: { path: string; from: string }, options: LetterActionOptions = {}): string
  // name = letterFileName(path)
  // if (options.includeRead !== false) "   • Read it again: read_mail({ letter: \"${name}\" })"
  // "   • Answer it: send_mail({ character: \"${from}\", message: \"…your reply…\", in_reply_to: \"${name}\" })"
  // "   • Discard it: discard_mail({ letter: \"${name}\" })"          // c1 had doc_delete_file({ uri: formatSelfUri(path) })
  // joined "\n"
export function formatLetterDate(sentAt: string): string   // unchanged: formatDateTime(long) || 'an unrecorded hour'
export function formatLetterHeading(letter, index): string
  // `${index}. From ${from} — ${date}${announced}\n   Letter: ${letterFileName(path)}`
  //   (was `\n   ${formatSelfUri(path)}`); announced = ' (already announced)' | ' (newly arrived)'
```
The indent is three spaces followed by `•`. The strings use a real `…` (U+2026) and `—` (U+2014).

### A.3 `lib/post-office/deliver.ts` (c1 only)
- The import `MAIL_FOLDER` becomes `resolveMailPath`.
- `composeAndDeliverLetter`, line 63:
  `const inReplyTo = params.inReplyTo ? (resolveMailPath(params.inReplyTo) ?? params.inReplyTo) : null;`
  (was `params.inReplyTo ?? null`). This normalized value is what gets stored in the recipient
  letter's `inReplyTo` frontmatter. An unresolvable ref stays raw, but it then fails the lookup and
  returns `reply-not-found`, so a raw value never lands in a letter.
- `resolveReplyInSenderMailbox` (100–107) becomes `const path = resolveMailPath(inReplyTo); if (!path) return null; return readLetter(senderVaultId, path);`.
  It was: strip leading `/`, require a case-insensitive `Mail/` prefix, then read the raw
  `normalized`. **Behaviour deltas:** a bare name now works (it was `reply-not-found`). `.md` is now
  appended. `Mail/sub/x.md` is now refused (it used to be read). A leading-slash `/Mail/x.md` is
  stored as `Mail/x.md` (it used to be stored raw with the slash). The reply-not-found debug line
  `'Reply target not in sender mailbox'` `{senderVaultId, inReplyTo}` now logs the **resolved**
  inReplyTo.
- Both consumers are affected: the `send_mail` tool and the Salon "Compose Mail" chat action
  (`app/api/v1/chats/[id]/actions/send-mail.ts:63`, `inReplyTo: validated.inReplyToPath ?? null`).
  Neither file changed.

### A.4 `lib/services/suparna-notifications/writer.ts`
- The import becomes `letterFileName, type DeliveredLetterSummary` from mailbox; the `formatSelfUri`
  import is dropped.
- `buildSuparnaMailWhisper` (46–58) is unchanged in code, but its text moves through
  `formatLetterActions`. It now reads "Read it again: read_mail…", "in_reply_to: "<name>"" and
  "Discard it: discard_mail…".
- `buildSuparnaMailLLMContext` (65–80):
  - The head line is
    `Letter from ${from}, delivered ${formatLetterDate(sentAt)} (letter: ${letterFileName(path)}):`.
    It was `(${formatSelfUri(path)})`.
  - The howto line is verbatim:
    `You can read any letter again with read_mail({ letter: "<its file name>" }), answer it with send_mail (set in_reply_to to its file name), or discard it with discard_mail({ letter: "<its file name>" }).`
  - c1's middle state had the discard clause as `doc_delete_file({ uri: "qtap://self/Mail/<its file name>" })`.

### A.5 Tool definitions (v5 stores these as byte-exact `JSON.stringify({name, description, parameters})`)

The field order inside `parameters` comes from zod `toJSONSchema`. The generator decides the final
bytes; the predictions below follow the catalog's existing pattern (`type, minLength, description`).

**`lib/tools/list-mail-tool.ts`** (renamed from list-email-tool.ts). The schema is `z.object({})`, the
name is `list_mail`, and the description is **unchanged** (verbatim, with the curly apostrophe U+2019):
`List the letters waiting in your own mailbox, newest first, with the exact way to read, answer, or discard each. Takes no arguments — it always lists your postbox and no one else’s.`
Predicted:
`{"name":"list_mail","description":"<above>","parameters":{"type":"object","properties":{},"additionalProperties":false}}`
The output type is `ListMailToolOutput { success; listing; count; error? }`.

**`lib/tools/read-mail-tool.ts`** (NEW, 48 lines):
- Schema: `letter: z.string().min(1).describe(…)`.
- The `letter` description, verbatim:
  `The letter's file name, exactly as list_mail or Suparṇā named it (e.g. "1718370000000-from-ariadne.md"). Just the name — no folder or path; the Post Office knows where your postbox is.`
- The tool description, verbatim:
  `Read a letter from your own mailbox, by its file name. Only ever reads your own postbox; use list_mail to see what is waiting.`
- Predicted:
  `{"name":"read_mail","description":"…","parameters":{"type":"object","properties":{"letter":{"type":"string","minLength":1,"description":"…"}},"required":["letter"],"additionalProperties":false}}`
- `ReadMailToolOutput { success: boolean; text: string; path?: string; error?: string }`.
- `validateReadMailInput` runs `safeParse`. The object is non-strict, so extra keys are allowed.

**`lib/tools/discard-mail-tool.ts`** (NEW, 47 lines):
- The `letter` description is **byte-identical** to read_mail's.
- The tool description, verbatim:
  `Throw away a letter from your own mailbox, by its file name. This cannot be undone. Only ever touches your own postbox; use list_mail to see what is there.`
- `DiscardMailToolOutput { success: boolean; message: string; path?: string; error?: string }`.

**`lib/tools/send-mail-tool.ts`** (c1) changes two strings:
- The `in_reply_to` description becomes
  `Optional. The file name of a letter in YOUR OWN postbox that you are answering (as list_mail or Suparṇā names it, e.g. "1718370000000-from-ariadne.md"). When supplied, your reply is prefaced with a quoted copy of that original letter.`
- The tool description's tail becomes `…Any character may write to any other; read your own letters with read_mail and answer them with send_mail's in_reply_to.`
  It was `…reading and answering are done with doc_read_file and send_mail's in_reply_to.`

**Snapshot** (`lib/tools/__tests__/tool-definitions-snapshot.test.ts` + `.snap`). `ALL_TOOLS` gains
`listMail` (replacing `listEmail`), `readMail` and `discardMail`, in the order
`keepImage, listMail, readMail, discardMail, listImages, sendMail, …`. That order is the key order
v5's data.rs must follow. The snapshot asserts only the derived **parameters** per key:
- `listMail` is the empty object.
- `readMail` and `discardMail` are `{additionalProperties:false, properties:{letter:{description, minLength:1, type:string}}, required:['letter'], type:object}`.
- `sendMail`'s `in_reply_to` description moves.
- jest's snapshot sorts keys, so it proves the values but not the byte order.

### A.6 Handlers

**`lib/tools/handlers/list-mail-handler.ts`** (renamed).
- Logger `logger.child({ module: 'list-mail-handler' })` (it was `list-email-handler`).
- Functions: `executeListMailTool(input, ctx: ListMailToolContext{userId, chatId, characterId?})` and
  `formatListMailResults`.
- The header string at line 66, final:
  `(Each letter is named by its file name — hand that to read_mail, discard_mail, or send_mail's in_reply_to.)`
  At baseline it read `(Each letter shows its qtap://self/… URI; the "self" authority always addresses your own vault.)`.
  c1's version was `…hand that to read_mail, or to send_mail's in_reply_to.)`.
- The catch fallback and ERROR line are renamed: `'Unexpected error in list_mail handler'` and
  `moduleLogger.error('list_mail handler threw unexpectedly', { chatId }, error)`.
- The other strings are unchanged: `'Your postbox stands empty.'`, the four fail strings, and
  `The Post Office stumbled and couldn't sort your post — ${msg}`.

**`lib/tools/handlers/read-mail-handler.ts`** (NEW, 103 lines).
- Logger `logger.child({ module: 'read-mail-handler' })`.
- `fail(m)` returns `{ success:false, text:m, error:m }`.

Order of checks in `executeReadMailTool(input, ctx: ReadMailToolContext{userId, chatId, characterId?})`:
1. The parse fails: `The Post Office needs the letter's file name to fetch it.`
2. There is no `characterId`: `Only a character keeps a postbox, and no character holds this one.`
3. `resolveMailPath(letter)` gives null: `Name the letter by its file name alone — the Post Office will not rummage outside your postbox.`
   **This check comes BEFORE the character lookup.**
4. `characters.findByIdRaw` finds nothing: `The Post Office cannot find your postbox; your character seems to have gone astray.`
5. `me.archivedAt` is set: `That character is archived; rehydrate it to continue.`
6. `ensureCharacterVault(me)` runs, then `readLetter(vault, path)` returns null. It logs
   `moduleLogger.debug('read_mail: no such letter', { chatId, characterId: me.id, path })` and fails
   with `No letter named "${letterFileName(path)}" rests in your postbox. list_mail will show you what does.`
7. If `!letter.frontmatter.alerted`, it calls `markAlerted(vault, path)`.
8. It logs `moduleLogger.debug('read_mail: letter read', { chatId, characterId: me.id, path, markedAlerted: !letter.frontmatter.alerted })`.
9. The success text joins these three parts with `'\n\n'`:
   `A letter from ${from}, posted ${formatLetterDate(sentAt)}:`, then
   `letter.body.trim() || '(the letter is blank)'`, then
   `formatLetterActions({ path, from }, { includeRead: false })`.
   It returns `{ success:true, text, path }`.
10. The catch path logs `moduleLogger.error('read_mail handler threw unexpectedly', { chatId }, err)`
    and fails with `The Post Office stumbled and couldn't fetch your letter — ${msg}`. When the thrown
    value is not an Error, `msg` falls back to `'Unexpected error in read_mail handler'`.

`formatReadMailResults(o)` returns `o.success ? o.text : o.error || o.text`.

**`lib/tools/handlers/discard-mail-handler.ts`** (NEW, 91 lines).
- Logger `module: 'discard-mail-handler'`.
- `fail(m)` returns `{ success:false, message:m, error:m }`.

Order of checks:
1. The parse fails: `The Post Office needs the letter's file name to know which one to discard.`
2. There is no `characterId`, the ref does not resolve, the character is missing, or it is archived:
   the same strings as read_mail, in the same order.
3. `ensureCharacterVault`, then `deleted = await discardLetter(vault, path)`, then
   `name = letterFileName(path)`. The name is computed after the delete, whatever its outcome.
4. `!deleted`: debug `'discard_mail: no such letter'` `{chatId, characterId: me.id, path}`, then fails
   with `No letter named "${name}" rests in your postbox. list_mail will show you what does.`
5. Success: `moduleLogger.info('discard_mail: letter discarded', { chatId, characterId: me.id, path })`.
   **This is INFO, not debug.** It returns
   `{ success:true, message: \`The letter "${name}" has been consigned to the wastepaper basket.\`, path }`.
6. The catch path logs ERROR `'discard_mail handler threw unexpectedly'` `{chatId}` and fails with
   `The Post Office stumbled and the letter stays where it was — ${msg}`. The fallback `msg` is
   `'Unexpected error in discard_mail handler'`.

`formatDiscardMailResults(o)` returns `o.success ? o.message : o.error || o.message`.

### A.7 `lib/chat/tool-executor.ts`
- The `BUILT_IN_TOOLS` Set (lines 303–306) is `'send_mail','list_mail','read_mail','discard_mail'`.
- Dispatch arms, each a lazy `import()` called with `{ userId, chatId, characterId }`:
  - `list_mail` (1217–1231) returns `{ toolName:'list_mail', success, result:{ formattedText, count }, error: success?undefined:error }`.
  - `read_mail` (1233–1247) returns `{ toolName:'read_mail', success, result:{ formattedText, path: out.path }, error }`.
    `path` is `undefined` on failure, so **the key is ABSENT** in JSON.
  - `discard_mail` (1249–1263) has the same shape, with `path`.
- There are **no alias arms.** An LLM that still calls `list_email` falls through to the non-built-in
  (plugin / unknown) path.

### A.8 `lib/tools/plugin-tool-builder.ts`
- Imports at lines 103–106.
- The workspace block pushes the definitions at lines 467–470, in the order `send_mail, list_mail,
  read_mail, discard_mail`. They sit before `readConversation`, and only when `includeWorkspaceTools`
  is not false.
- The comments are updated. Brahma (`includeWorkspaceTools:false`) gets none of the four.
  `lib/brahma-console/__tests__/brahma-console.test.ts` adds
  `expect(names).not.toContain('list_mail'|'read_mail'|'discard_mail')`.

### A.9 `lib/tools/index.ts`
The re-export blocks sit at line 59 (List Mail), 73 (Read Mail) and 87 (Discard Mail). v5 has no
counterpart.

### A.10 `lib/tools/destructive-tools.ts` (c2), after the change
```ts
export const DESTRUCTIVE_TOOL_NAMES: ReadonlySet<string> = new Set<string>([
  'doc_delete_file',
  'doc_delete_folder',
  // Post Office — deletes a letter through the same GC chokepoint as doc_delete_file.
  'discard_mail',
]);
```
It has one reader, `lib/services/chat-message/orchestrator.service.ts:1042–1061`. For
`chat.chatType === 'autonomous'`, the destructive tools are filtered out unless
`policy !== 'always_refuse' && chat.runDestructiveToolsAllowed === 1`. When anything is removed it
logs INFO `'Autonomous room: destructive tools filtered from per-turn list'`
`{chatId, policy, allowedAtRoom, removed}`. The list has no confirmation or approval semantics
anywhere else.

### A.11 `app/api/v1/tools/route.ts` (the `GET /api/v1/tools` inventory)
- Imports at lines 43–46 (the `@/lib/tools` barrel).
- `BUILT_IN_TOOL_SCHEMAS` (72–…) gains `list_mail: listMailToolDefinition`,
  `read_mail: readMailToolDefinition` and `discard_mail: discardMailToolDefinition`, right after
  `send_mail`. It is used only when `includeSchemas`, to attach `parameters`.
- `BUILT_IN_TOOLS` (117–…) rows, in order after `send_mail`:
  - `{ id:'list_mail', name:'List Mail', description:'List the letters in your own mailbox, with the exact way to read, answer, or discard each', source:'built-in', category:'utility' }`
  - `{ id:'read_mail', name:'Read Mail', description:'Read a letter from your own mailbox by its file name', …'utility' }`
  - `{ id:'discard_mail', name:'Discard Mail', description:'Throw away a letter from your own mailbox by its file name', …'utility' }`
- Result: 41 → 43 rows, 37 → 39 schema ids. `send_mail`'s schema parameters also move (A.5).
- `GET` (454) maps `BUILT_IN_TOOLS`. Nothing else in the route changes.

### A.12 Other files
- The CHANGELOG entries.
- `help/post-office.md`, "Reading, answering, and discarding", lines 51–61: rewritten in both
  commits. It is vendored in v5, so it must be byte-copied at `12c336fad`.
- Docs only, not ported: `docs/developer/API.md`, `README.md`, `.claude/commands/update-documentation.md`.
- Version stamps: `package.json`, `packages/quilltap/package.json` and `package-lock.json` go
  `4.10.0-dev.93` → `.94` → `.95`. Any oracle that stamps an `appVersion` moves.
- Tests: `send-mail-handler.test.ts` gets a bare-name reply arm, where `readLetter` is called with
  `'Mail/old-from-bertie.md'` and `deliverLetter` receives `inReplyTo` `'Mail/old-from-bertie.md'`.
  There is also `writer.test.ts`, the new `read-mail-handler.test.ts` (119 lines) and
  `discard-mail-handler.test.ts` (90 lines).
- **No migration touches tool names.** `migrations/` holds only `index.ts` and `README.md`, and there
  are no `list_email` hits in lib/app/migrations.

### A.13 Claims in the commit messages versus the hunks
- **c1: "Discarding still goes through doc_delete_file."** True at c1; c2 supersedes it.
- **c2: "the same deleteWithGC chokepoint doc_delete_file uses."** True only for the link or GC
  step. `doc_delete_file` (`file-management-handlers.ts:541–552`) also runs
  **`assertCharacterMayWrite(resolved, context)`**, the protected-document check, before
  `deleteDatabaseDocument`. It also distinguishes "folder, not file" and posts a **Librarian delete
  announcement**. **discard_mail does none of these.** A letter link with
  `allowCharacterWrite=false` is deleted anyway. The CHANGELOG admits only the missing announcement.
  See D.6.
- **c2: "buffered and replayed on the parent from the job child."** No hunk touches the buffering. It
  describes the existing `deleteWithGC` IPC path, which is a Node workaround. v5 needs nothing beyond
  running the delete inside a writer closure.
- **c1: "Reading an unannounced letter marks it announced."** True, but it is a content WRITE
  (`writeDatabaseDocument`, which re-indexes the store) done inside a "read" tool.
- **c1: "send_mail's in_reply_to accepts the file name and stores the Mail/ path."** True, but the
  work happens in `deliver.ts`; the send-mail handler is unchanged. The Salon Compose-Mail action
  inherits it too, which the message does not mention.

---

## B. v5 on main — where each surface lands

Line numbers are v5 main at `ef29a058e`.

| v4 surface | v5 file:line | Change |
|---|---|---|
| `list-mail-tool.ts` + handler | `crates/quilltap-core/src/tools/list_email.rs` (156 lines) | Rename to `list_mail.rs`: `ListMailOutput`, `execute_list_mail`, `format_list_mail_results`. Header at `:126` becomes the c2 string. Module doc `:1-7`. Serialize name `:35`. v5 has **no** ERROR line at all (the v4 `list_mail handler threw unexpectedly` is absent; pre-existing). |
| read-mail handler | NEW `tools/read_mail.rs` | Model it on list_email.rs. Both connections: `find_by_id_raw` (main), `ensure_character_vault` (`db/character_vault.rs:246`, main+mount), `read_letter`, and `mark_alerted` (mount write). Output `{success, text, path?, error?}` with a fixed serialize order. |
| discard-mail handler | NEW `tools/discard_mail.rs` | Output `{success, message, path?, error?}`. Delete through `database_store::delete_database_document` (`db/database_store.rs:254` → `DocMountFileLinksRepository::delete_database_document` `db/doc_mount_file_links.rs:1312`, which calls `delete_with_gc` `:1526`). This is the same chokepoint `doc_delete_file` uses (`tools/doc_edit/file_management.rs:605`). **v5 has no `…IfExists` twin**, and none is needed (A.1: the arm is unreachable). Use a comment, or add a twin only if a census demands v4's function shape. |
| `tools/mod.rs` | `:14` `pub mod list_email;` | Rename it, and add `read_mail` and `discard_mail`. |
| tool-executor | `tools/executor.rs` `:62` (use list), `:151` (`BUILT_IN_TOOLS`), `:214` (`PORTED_TOOLS`), `:585` (dispatch arm), `:1460-1478` (`run_list_email`, `wardrobe_write` closure, `json!({formattedText, count})`) | Rename, then add `read_mail` and `discard_mail` to all four places. `run_*` returns `result: {formattedText, path}` with **`path` OMITTED on failure**. Note that `run_send_mail` (`:1438`) inserts `json!(out.path)`, which gives `null`, not absent (D.9). |
| definitions catalog | `tools/definitions/data.rs` `:12` ("59 entries"), `:175-178` (listEmail), `:185-187` (sendMail JSON) | Generated. Regenerate with `harness/oracle/tools/gen-tool-catalog.mjs` from `harness/oracle/cases/tool-definitions.ts` at pin `12c336fad`. The order is `keepImage, listMail, readMail, discardMail, listImages, sendMail`. |
| catalog count | `tools/definitions/mod.rs:91-99` `assert_eq!(TOOL_DEFINITIONS.len(), 59)` | Becomes **61**, with a history comment. |
| plugin-tool-builder workspace set | `services/tool_build.rs:301-302` | `push_key(…"listEmail")` becomes `listMail`. Add `readMail` and `discardMail` after it, in v4's order. |
| DESTRUCTIVE_TOOL_NAMES | `services/tool_build.rs:676` `&["doc_delete_file","doc_delete_folder"]` | Append `"discard_mail"` with v4's comment. Reader: `services/orchestrator.rs:2074-2085` (the autonomous filter via `destructive_allowed` and `filter_destructive_tools`). Unit pin `tool_build.rs:884-900` (`destructive_filter`): add a workspace-on slate asserting discard_mail is removed and list_mail/read_mail are kept. The v4 INFO line `'Autonomous room: destructive tools filtered…'` is **absent in v5** (pre-existing). It now fires on every workspace slate in an autonomous room (D.8). |
| Brahma | `services/brahma_console/mod.rs:248`, `orchestrator.rs:403` (`include_workspace_tools: false`) | Neutral by construction. v5 has no twin of v4's `not.toContain` pins. Add a tool_build unit assertion. The `tool_build` corpus row `brahma_no_workspace` covers it on the wire. |
| Other workspace-on surfaces | `services/carina_query.rs:554`, `services/help_chat/orchestrator.rs:771` (`include_workspace_tools: true`) | Carina answerers and the help chat gain read_mail/discard_mail automatically. See C for neutrality. |
| tools inventory route | `services/tools_inventory.rs` `:2` (header "41-entry"), `:6`, `:51` (row), `:92` ("37 ids"), `:99` (`("list_email","listEmail")`), `:392-396` (asserts 37/41) | Rename the row to `list_mail`/"List Mail". Insert the `read_mail` and `discard_mail` rows (A.11 strings) after it. Add SCHEMA_KEYS `("read_mail","readMail")` and `("discard_mail","discardMail")`. Counts become 39 and 43. |
| mailbox parser | `post_office/mailbox.rs` (after `MAIL_FOLDER` `:34`; `mark_alerted` `:299-322`) | Add `letter_file_name`, `resolve_mail_path` (A.1 order; `js_trim`; `to_lowercase` compares; `regex`-free is fine) and `discard_letter` (debug `discardLetter` `{vaultId, path, deleted}`). Module doc `:1-16` names list_email; update it. v5's `mark_alerted` lacks v4's NOT_FOUND warn `'markAlerted: letter no longer present'` (pre-existing; read_mail makes it reachable in a new race). |
| instructions | `post_office/instructions.rs:12-50` | `format_letter_actions(path, from)` gains an `include_read: bool` (or an options struct) and names by file name. The heading becomes `Letter: <name>`. The `format_self_uri` import goes. |
| deliver | `post_office/deliver.rs:59-98` and `:100-117` | Store `resolve_mail_path(r).unwrap_or(r)` as `in_reply_to`. It currently passes the raw value to `deliver_letter` at `:94`. `resolve_reply_in_sender_mailbox` is rewritten over `resolve_mail_path`. **Shared** with the Salon compose route (`api/chat_post_office.rs:981-1049`). |
| Suparṇā LLM context | `services/suparna_mail.rs:36-54` | Head `(letter: <name>)` plus the new howto string, verbatim. |
| Suparṇā whisper | `services/suparna_notifications.rs:55-83` (and doc `:25`) | No code change. It moves through `format_letter_actions`. |
| comments naming the tool | `post_office/mod.rs:3`, `post_office/mailbox.rs:11`, `subprompts/storage.rs:145` ("`tools::list_email` idiom"), `api/chat_post_office.rs:1100` ("list-email tool's refusal") | Comment-only renames. `subprompts/storage.rs` has no behavioural reference. |
| help page | `help/post-office.md` (repo root) | Byte-copy at `12c336fad`. The file count is unchanged (no new help file), so the vendored-count literals do not move. |
| SPA | `apps/web/src` | **Confirmed: zero hits** for `list_email`, `send_mail`, `list_mail`, `read_mail` or `discard_mail`, including e2e. The tools inventory is rendered dynamically. `salon-conversation.ts:215-223` `DOC_RELOAD_TOOLS` names `doc_delete_file`, but v4's SalonView was untouched, so do **not** add discard_mail. `autonomous-room-card.ts:249` copy is unchanged in v4 (`AutonomousRoomCard.tsx:277` still names only the doc tools). **No SPA work.** |
| Opacity covenant (not changed) | `tools/doc_edit/shared.rs:559-591` (`build_read_resolution_context`, `hide_character_vaults: true`), `doc_edit/path_resolver.rs:441`, `:587-596`, `:646` | Untouched. read_mail and discard_mail must **not** route through it (D.5). |

Harness and oracle files that name list_email (all confirmed by grep):
- **`crates/quilltap-harness/tests/mail_carina_tools_equivalence.rs`** (`:1`, `:5`, `:49` import, `:352-355`).
  - Recipe (header `:14-37`): build the fixture to `/tmp/qt-mail-{main,mount}.db` via
    `node --import tsx harness/oracle/fixtures/build-mail-carina-tools-fixture.ts` from the v4 checkout.
  - Then `TZ=UTC … QT_ORACLE_OUT=/tmp/oracle-mail-tools.ndjson npx jest … -- "mail-tools\.test\.ts$"`,
    staged under `/tmp/qt-mail-carina-tools-stage`.
  - Env vars: `QT_ORACLE_MAIL`, `QT_ORACLE_CARINA`, `QT_FIXTURE_TMP_MAIN`, `QT_FIXTURE_TMP_MOUNT`.
- **`harness/oracle/cases/mail-tools.test.ts`** (`:4`, `:191-199`): imports
  `@/lib/tools/handlers/list-email-handler`. **It crashes at the target pin** until it is repointed.
  There are 12 scenarios (`:73-126`).
- **`harness/oracle/fixtures/build-mail-carina-tools-fixture.ts`** (`:2`, a comment). It seeds through
  v4's real `deliverLetter` and `initializeDatabase` (`:97`) plus `ensureCollection('characters')` and
  hand-run mount-index DDL.
- **`harness/oracle/cases/tool-definitions.ts`** (`:54`, `:122`) and
  **`tool-definitions-canonical.ts`** (`:51`, `:119`): both import `@/lib/tools/list-email-tool`, so
  **both crash at the target** until edited.
  - They feed `crates/quilltap-harness/tests/tool_definitions_equivalence.rs`. Recipe `:10-18`:
    `npx tsx …/tool-definitions.ts > /tmp/oracle-tool-definitions.ndjson`, then the same for
    `-canonical.ts`.
  - Env vars: `QT_ORACLE_TOOL_DEFINITIONS`, `QT_ORACLE_TOOL_DEFINITIONS_CANONICAL`.
  - The same ndjson feeds gen-tool-catalog.mjs.

---

## C. Families and fixtures that move

### C.1 Harness families (predicted moves; measure each)

| Family (test file) | Oracle case | Why it moves | Fixture |
|---|---|---|---|
| `mail_carina_tools_equivalence` | `mail-tools.test.ts` (+ `carina-tool.test.ts`, unchanged) | Rename; `send_and_list` and `list_single` listing text; **grow**: read_mail and discard_mail arms. `reply` stays neutral because it uses the `Mail/…` path, and the content is identical. | Built to /tmp from `mail-carina-tools.json` (**not committed**) |
| `tool_definitions_equivalence` | `tool-definitions.ts`, `tool-definitions-canonical.ts` | Catalog 59 → 61; the sendMail JSON moves | none |
| `tools_inventory_equivalence` | `chat-dialogs-tools.test.ts` (`QT_ORACLE_TOOLS_INVENTORY`) | 41 → 43 rows; 37 → 39 schemas; the sendMail parameters move | committed `crates/quilltap-web/tests/fixtures/chat-dialogs-{main,mount}.db` (read only, not rebuilt) |
| `tool_build_equivalence` | `tool-build.test.ts` (`QT_ORACLE_TOOL_BUILD`) | Every `includeWorkspaceTools` slate: the listEmail name plus two new defs; the `destructive_autonomous_*` rows (`tool-build.json:30-33`) drop discard_mail. The `brahma_no_workspace` and p4d216 rows stay neutral. | Built to `/tmp/qt-tool-build.db` via `build-tool-build-fixture.ts` (only `plugin_configs` ensured) |
| `orchestrator_tier3_equivalence` | `orchestrator-tier3.test.ts` (`QT_ORACLE_ORCHESTRATOR`, TZ=UTC) | Records **`tools` at the wire** per canned stream (`orchestrator-tier3.test.ts:204,339-370`) through the REAL buildTools. Every native character turn's slate moves. The text-block cases (o1-mini) render the tools into the prompt, so their canned keys move. | Built to `/tmp/qt-orch-{main,mount}.db` via `build-orchestrator-fixture.ts`. **Reads `chats`, so it MUST be pinned.** |
| `context_feeders_leaves_equivalence` | `context-feeders-leaves.ts` (`QT_ORACLE_CONTEXT_FEEDERS_LEAVES`, TZ=UTC) | `buildSuparnaMailLLMContext` head plus the howto line | none (tier-1) |
| `post_office_concierge_lantern_suparna_equivalence` | `post-office-concierge-lantern-suparna.ts` (`QT_ORACLE_PO_CLS`, TZ=UTC) | `buildSuparnaMailWhisper` action lines | none (tier-1) |
| `help_tree_equivalence` | help-tree jest (`QT_ORACLE_HELP_TREE`) | `help/post-office.md` bytes. Also re-run whichever `help_*` families read the tree content (memory: only 2 of 16 do). | none |
| **NEW** tier-1 `mail_path_equivalence` (suggested) | new `harness/oracle/cases/mail-path.ts` over v4's real `resolveMailPath`/`letterFileName` | Pure parser; the corpus in D.10 | none |
| `post_office_routes_equivalence` | `post-office-routes.test.ts` (`QT_ORACLE_POST_OFFICE`) | **Predicted NEUTRAL.** `send_mail_reply` passes `meta.seededLetterPath` (a `Mail/…` path) and not-found uses `Mail/1700000000000-from-nobody.md`. **Grow**: a bare-name `inReplyToPath` arm and a `/Mail/…` leading-slash arm, both of which now store `Mail/…`. | build-post-office-fixture.ts → pin |
| `post_office_writers_tier3_equivalence` | `post-office-writers-tier3.ts` | NEUTRAL: `postSuparnaMailWhisper` gets pre-built `content` from the spec (`:236-241`) | — |
| `brahma_console_*`, `brahma_orchestrator_tier3`, `scenario_builder_tier3` | — | NEUTRAL: `includeWorkspaceTools: false` | — |
| `help_chat_orchestrator_tier3`, `enclave_step_tier3`, `carina_query_tier3` | — | Predicted NEUTRAL: the canned keys are messages-only and the recorded `slate` is messages (help-chat `:20`). Carina's oracle stubs `buildTools` to `[]` (`carina-query-tier3.test.ts:209`). **Measure** enclave and help-chat, in case either runs in text-block tool mode (then the tools would render into the prompt text). | — |
| `primary_stream_tier3` | — | NEUTRAL: `toolCount` comes from the harness-supplied params (`:411`) | — |

### C.2 Oracle cases to edit
`mail-tools.test.ts` (repoint plus new scenarios), `tool-definitions.ts`, `tool-definitions-canonical.ts`,
and the new `mail-path.ts`. After editing they cannot run at the `acadcc7cd` pin unless they are
written to shape imports by what the tree exports, which is the P4.D216 idiom (`tool_build_equivalence`
header `:19-22`). Decide whether both-pin runs are wanted for the neutrality proof.

Fixture specs to grow:
- `mail-carina-tools.json`: an already-alerted letter, a letter with a mixed-case name, and a letter
  **hard-linked** into a second path or vault for the discard GC arm (D.7).
- `post-office-web.json`: optional.

### C.3 Committed fixture pairs and the `renderedMarkdown` pin
- **The mail fixture is NOT committed.** `build-mail-carina-tools-fixture.ts` writes
  `/tmp/qt-mail-{main,mount}.db` fresh from **v4's own `initializeDatabase()`** (`:97`) at whatever
  checkout runs it. It explicitly ensures only `characters` plus the six `doc_mount_*` tables and
  `doc_mount_blobs`.
- Whether a `chats` table exists depends on what v4's `initializeDatabase` creates. **UNMEASURED**:
  the memory rule "measure what the jest DB init creates" applies, and I could not run it read-only.
- The v5 mail handlers never read `chats`, so a column-less `chats` would probably be harmless for
  this family. But **build it in a pinned worktree at `12c336fad`** regardless. A build at v4 HEAD
  (`97b25fc53`, which is ≥ `f7f3d7bf0`) writes a v4-4.10-dev.96+ schema.
- **The orchestrator fixture** (`build-orchestrator-fixture.ts` → `/tmp/qt-orch-*.db`) reads chats
  heavily. **A build at any pin ≥ `f7f3d7bf0` lacks `chats.renderedMarkdown` and is unreadable or red
  for v5 main.** It MUST be built at `12c336fad`, in a pinned worktree per the drift-ledger recipe.
  The same applies to `build-tool-build-fixture.ts` and `build-post-office-fixture.ts`, and to every
  sweep family re-run this round.
- The committed `crates/quilltap-web/tests/fixtures/chat-dialogs-{main,mount}.db` (tools inventory)
  is consumed, not rebuilt, so the column question does not arise. It is encrypted; its
  `renderedMarkdown` presence was not checked.

---

## D. Traps and premises to measure

1. **Stored names are not migrated in v4, so v5 must not migrate them either.**
   - `chats.disabledTools` / project `disabledTools` (`lib/schemas/chat.types.ts:910,1293`, applied at
     `orchestrator.service.ts:953-1011`) are arrays of tool ids. A user who disabled `list_email` now
     silently gets `list_mail` enabled. This is v4-faithful, so do NOT remap.
   - Historic TOOL messages and llm_logs rows still carry `toolName: "list_email"` as data. Nothing
     reads them by name.
   - A model echoing the old name gets the unknown-tool path. There is no alias (CHANGELOG: "The old
     name is not aliased").
   - Consider a v4 bug filing only if the human wants one. There is no port action.
2. **Who reads DESTRUCTIVE_TOOL_NAMES in v5.**
   - Readers: `services/orchestrator.rs:2074-2085` (the autonomous-room filter only) and
     `tool_build_equivalence.rs:272-273` (the corpus mirror).
   - The list has no confirmation or approval semantics, so the ledger's phrase "gates confirmation /
     autonomy behaviour" overstates it: it gates autonomy only.
   - Measure whether the enclave or autonomous step path in v5 builds its slate through the same
     orchestrator branch, so that autonomous rooms really lose discard_mail.
3. **The `{text}` / `{message}` failure-key asymmetry.** list_mail uses `listing`, read_mail uses
   `text`, discard_mail and send_mail use `message`. Each serializes in declaration order
   `success, <key>, [path], [error]`: list is `success, listing, count, error?`; read is
   `success, text, path?, error?`; discard is `success, message, path?, error?`. On failure, `path` is
   absent and `error` equals the message.
4. **The announced-flag write path.** `markAlerted` does `readDatabaseDocument`, then
   `updateFrontmatterInContent(content, {alerted:true})`, then `writeDatabaseDocument`.
   - This is a **content rewrite** of the mount-index document: `doc_mount_documents` content, the
     file row's sha/size via the content-addressed rewrite plus `gcOrphanedFileRow`, and a post-write
     reindex (chunks).
   - There is no main-DB write and no dedicated column. The flag lives only in the letter's YAML
     frontmatter.
   - The v5 twin exists (`post_office/mailbox.rs:299-322`, calling `update_frontmatter_in_content`
     and `write_database_document` on the **mount writer**). So read_mail must run inside a
     both-connections writer closure (`wardrobe_write`, as `run_list_email` does), not a read.
   - The tier-2 comparand should dump the letter content after the read, to prove
     `alerted: true`, and the mount rows.
5. **The opacity covenant bypass is by design.**
   - v4's covenant is `lib/doc-edit/path-resolver.ts:127` (`hideCharacterVaults?`), `:359`
     (`vaultsVisible = !hideCharacterVaults`), `:447-451` (the `self` token refused when hidden), and
     `:499/:509` (the `vaultsHidden:` messages). It is set from
     `lib/tools/handlers/doc-edit/shared.ts:170` (`actingCharacterIsOpaqueToVaults`).
   - read_mail and discard_mail never touch the doc-tool resolver. They call `ensureCharacterVault`
     (`lib/mount-index/character-vault.ts:143`) on the caller's raw row
     (`read-mail-handler.ts:64`, `discard-mail-handler.ts:64`). Confinement comes only from
     `resolveMailPath` plus "your own vault id".
   - v5's covenant is `tools/doc_edit/shared.rs:559-591`, `doc_edit/path_resolver.rs:441,587-596,646`.
     Port the handlers over `db::character_vault::ensure_character_vault` (`:246`) exactly as
     `list_email.rs:90-104` does. **Do not route through `build_read_resolution_context`, and do not
     "fix" the hole.**
   - A differential arm with an OPAQUE character (no `systemTransparency`) reading and discarding its
     own letter is the proof. A matching `doc_read_file qtap://self/Mail/…` refusal on the same
     character is the contrast.
6. **Protected-document bypass (a v4 property, not in the ledger).**
   - `doc_delete_file` runs `assertCharacterMayWrite` (`file-management-handlers.ts:541`; v5
     `file_management.rs:592`) before deleting. `discard_mail` does not.
   - So a `Mail/` letter whose link has `allowCharacterWrite = 0` (or `allowCharacterRead = 0`) is
     discarded anyway, and read_mail reads a `characterRead:false` letter.
   - This is v4-faithful. Pin it with an arm and record it; consider an upstream filing if the human
     wishes.
7. **The GC chokepoint.**
   - v5 `delete_database_document` → `delete_with_gc` (`doc_mount_file_links.rs:1312-1333,1526`)
     matches by `LOWER(relativePath) = LOWER(?)`. v4 uses `findByMountPointAndPath`. Measure
     case-sensitivity parity with a mixed-case letter name read or discarded by a lower-case name.
     `resolveMailPath` rebuilds the prefix as `Mail/` but keeps the name's case, and read, mark and
     delete all use the resolved path. A case-insensitive lookup plus a case-sensitive write could
     create a second link on `markAlerted`, so measure both sides.
   - Hard-link arm: a letter hard-linked elsewhere loses only this link; the file row survives. A
     group of one is dissolved.
   - `emitDocumentDeleted` → the watcher re-embed is v5's standing no-op seam (`database_store.rs:252`).
   - discard posts **no Librarian announcement**. Do not reuse the doc-edit handler, which attaches
     one (`file_management.rs:617-631`).
8. **Absent v4 log lines** (capture-pin candidates; each is v4's exact level, message and fields).
   - New in this round:
     - `read_mail: no such letter` (debug)
     - `read_mail: letter read` (debug, `markedAlerted`)
     - `read_mail handler threw unexpectedly` (error)
     - `discard_mail: no such letter` (debug)
     - `discard_mail: letter discarded` (**info**)
     - `discard_mail handler threw unexpectedly` (error)
     - `discardLetter` (debug, service `PostOffice:Mailbox`)
   - Pre-existing and absent in v5:
     - `list_mail handler threw unexpectedly`
     - send_mail's error line
     - `markAlerted: letter no longer present` (warn)
     - `Reply target not in sender mailbox` (debug)
     - `Autonomous room: destructive tools filtered from per-turn list` (info, now reachable on every
       autonomous workspace slate because discard_mail is always present)
   - Logger identity: `logger.child({ module: '<x>-handler' })` versus `createServiceLogger` (the
     latter gives `{service, module:'service'}`). v5 has no single idiom (`search.rs` uses
     `context = …`; `llm_image_budget.rs` uses `module = …`). Choose per the capture-rig convention
     and pin it.
9. **The executor `path` key.** v4's `result: { formattedText, path: out.path }` drops `path` on
   failure. v5 `run_send_mail` (`executor.rs:1438-1441`) inserts `json!(out.path)`, which gives
   **`null`**, while its comment says "dropped". Measure whether any comparand (tool-message content,
   llm_logs) sees it before copying the pattern into read_mail and discard_mail. It may be a latent
   send_mail divergence.
10. **The resolveMailPath corpus** (tier-1). Include:
    - Whitespace: JS `trim` covers NBSP, U+FEFF and U+2028, so use `js_trim`.
    - `QTAP://SELF/mail/x`, `qtap://self//Mail/x` (gives `Mail/x.md`), `/qtap://self/Mail/x` (gives
      null — the order trap), `//Mail/x`.
    - `Mail/`, `Mail`, `mail/X.MD`, `x.md.md`, `...`, `.md`, `.`, `..`, `a\\b`, `Mail/Mail/x`, `''`,
      `'   '`.
    - Non-ASCII names (`İ`, emoji).
    - `letterFileName` on a non-`Mail/` path, on `MAIL/x`, and on `Mail/` alone.
    - The prefix compare stays ASCII-safe: no non-ASCII character lowercases into `mail/` (`İ` becomes
      `i̇`, two code units), so `slice(5)` is a safe byte index.
11. **The in_reply_to storage change is shared.** `compose_and_deliver_letter` serves both
    `send_mail` and the Salon compose route (`api/chat_post_office.rs:1032-1049`), so both move
    together. v4's truthiness guard (`params.inReplyTo ?`) is covered by the `min(1)` / `min1`
    validators on both paths.
12. **Validation.** `letter: z.string().min(1)` counts UTF-16 units. `'   '` passes Zod and then fails
    `resolveMailPath`, which gives the "rummage" message, not the parse message. A non-object input or
    a missing, non-string or empty `letter` gives the parse message. Extra keys are accepted.
13. **Tool-slate order matters for canonicalized wires only where they are not re-sorted.**
    `tool_build_equivalence` compares arrays sorted by name, but the orchestrator's tools-at-wire may
    not be sorted. Push in v4's exact order: send, list, read, discard, then readConversation.
14. **Version stamp.** `-dev.95` at the pin. Check for any `V4_APP_VERSION` / `appVersion` constants
    in families re-run at `12c336fad` (e.g. `ai_import_tier3`).

---

## E. Size estimate and agent tier

**v5 production files: about 17 touched, 2 new.**
- New: `tools/read_mail.rs`, `tools/discard_mail.rs`.
- Renamed and edited: `tools/list_email.rs` → `list_mail.rs`.
- Edited: `tools/mod.rs`, `tools/executor.rs`, `tools/definitions/data.rs` (generated),
  `tools/definitions/mod.rs`, `post_office/{mod,mailbox,deliver,instructions}.rs`,
  `services/suparna_mail.rs`, `services/suparna_notifications.rs` (doc only),
  `services/tool_build.rs` (push + destructive + unit pin), `services/tools_inventory.rs`.
- Comment-only: `subprompts/storage.rs`, `api/chat_post_office.rs`.
- Also `help/post-office.md` (a byte copy) and the CHANGELOG.
- Roughly 450–600 Rust lines including unit tests. No SPA work.

**Harness and oracle.**
- 3 oracle cases edited, 1 new tier-1 case plus its test file, and 1–2 fixture specs grown.
- About 8 families re-recorded: mail_carina_tools (grown), tool_definitions, tools_inventory,
  tool_build, orchestrator_tier3, context_feeders_leaves, po_concierge_lantern_suparna, help_tree
  (plus the help families that read the tree).
- About 5 to measure as neutral: post_office_routes (grow optional), post_office_writers_tier3,
  help_chat/enclave/carina tier-3, brahma_*, scenario_builder.
- All regens at a pinned `12c336fad` worktree. The fixture builders are pinned for the
  `renderedMarkdown` reason.

**Recommended tier: one lane, a mid-to-high tier agent (Sonnet-class is enough for the mechanical
rename and registration).** The judgment calls still need the most capable model, either to write the
order or to review the lane:
- the covenant bypass (D.5)
- the protected-document bypass (D.6)
- the case-sensitivity measurement (D.7)
- the `path: null` premise (D.9)
- the resolveMailPath order trap (A.1 step 3)

The two commits should land as one lane (c2 edits c1's strings). An optional split is possible:
- (a) the rename, the catalog, the inventory and the tool_build slate, which is mechanical;
- (b) read_mail, discard_mail, the parser and the Suparṇā text, which is behavioural.

This is not worth two lanes given the shared files.
