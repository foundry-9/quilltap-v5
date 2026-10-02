# Survey — P4.D244: v4 bug 174, the vault image's server-relative `url` (`f6426e196`)

**Date:** 2026-10-01 · **v4:** `f6426e196` (clean) · **v5 `main`:** `6d44cfae2`
**Kind:** read-only measurement. Nothing was built or run. Every path below is
relative to its repo root (v4 = `~/source/quilltap-server`, v5 = this repo).
Line numbers are the POST-commit files at `f6426e196` unless marked "pre-fix"
(read from `git show f6426e196 -- <path>`).

**The finding in one line.** v5 has both halves of bug 174 verbatim by faithful
port and nothing pins either: the loader (`services/chat_files.rs:258`, `:345`)
puts the server-relative `/api/v1/mount-points/…` path in `"url"` beside the
bytes, and the Z.AI / NanoGPT builders (`request_builder/chat_completions.rs:120`,
`:191`) send `url` before `data` — so on v5 today every document-store image
reaches Z.AI and NanoGPT as an unfetchable path, exactly as v4 did before this
commit. v4's fix is two subtractions and one guard; three
`file_attachment_tier3_equivalence` arms and four `request_builder_equivalence`
rows go RED at the pin on unported main.

---

## §A v4 (the oracle)

### A1 The commit

`f6426e196` — "Fix bug 174: send vault image bytes, not a server path, to Z.AI
and NanoGPT" (2026-10-01 15:59, `4.10.0-dev.106`), one commit past the baseline
`ca363178d`. 18 files, +329/−22. Code: `lib/chat-files-v2.ts` (−8/+6, two
hunks), the Z.AI and NanoGPT `provider.ts` AND their built `index.js` (+8/−1
each, the same hunk four times). Tests: one new file, two new cases. The rest
is stamps and docs (A5). `help/` did NOT move (`git diff --stat ca363178d
f6426e196 -- help/` is empty). The bug doc:
`docs/developer/bugs/fixed/bug-174-vault-image-relative-url-to-provider.md`
(115 lines, "v5 status: Not assessed"; provenance "Original to v4 … arrived
with `45dcf97dc` (2026-05-15 …)").

### A2 The loader — `lib/chat-files-v2.ts` `loadMountFileAsAttachment` (`:561-675`)

Two hunks, both inside this one function; nothing else in the file moved.

**Native-text branch** (pre-fix `:587-599` → post `:586-600`). Pre-fix:

```ts
const url = `/api/v1/mount-points/${mountLink.mountPointId}/files/${encodeURI(mountLink.relativePath)}`;
return { id: mountLink.id, filepath: url, filename: mountLink.originalFileName ?? mountLink.fileName,
         mimeType: textMime, size: buffer.length, data: buffer.toString('base64'), url };
```

Post-fix (`:590-598`): the comment `// \`filepath\` only, never \`url\`: see the
note on the blob branch below.` then the same object with the template
INLINED into `filepath` and **no `url` key**. Key order, pre: `id, filepath,
filename, mimeType, size, data, url`; post: `id, filepath, filename, mimeType,
size, data`. Only the last key is gone; `filepath` keeps the identical string.

**Blob branch** (pre-fix `:664-676` → post `:663-674`). Pre-fix built `const url
= …/blobs/…` then `{ id, filepath: url, filename, mimeType: outputMimeType,
size: buffer.length, data: buffer.toString('base64'), url }`. Post-fix carries
a four-line why-comment (`:663-666`):

> The server path goes in `filepath` and never in `url`. `FileAttachment.url`
> means "a URL the provider can fetch", and a plugin that honours it sends it
> in place of the bytes — so a server-relative path there reached Z.AI and
> NanoGPT as an unfetchable image and the turn died with a 400 (bug 174).

and returns (`:667-674`) `id, filepath, filename, mimeType, size, data` — the
template inlined at `:669`, no `url`.

**Does any OTHER v4 attachment loader set `url`?** No. Measured:

| site | shape | `url`? |
|---|---|---|
| `lib/chat-files-v2.ts:703-710` — the legacy `files`-table branch of `loadChatFilesForLLM` | `{ id, filepath: getFileApiPath(id), filename, mimeType, size, data }` | never had one (the bug doc's "Uploaded chat files … set no `url`") |
| `lib/chat-files-v2.ts:237-246`, `:298-307`, `:310-319`, `:438-447` | upload RESULTS (`ChatUploadedFile`: `sha256`, `width`, `height`), not provider attachments | no |
| `lib/chat/file-attachment-fallback.ts` | reads `fileAttachment.data` (`:925`, `:935`); `grep -n '\burl\b'` → zero hits | no |
| the Lantern walk | `context-builder.service.ts:258` `collectLanternImageFileIdsForCharacter` collects IDs; the bytes come from `loadChatFilesForLLM` (`:186`, `:450`) — so a vault-hosted Lantern background WAS a bug-174 input (the bug doc lists "a Lantern background") and is fixed by the same hunk | via the loader |
| `lib/chat/message-attachment-adapter.ts:148` | `filepath: attachment.filepath ?? \`/api/v1/files/${attachment.id}\`` | no |
| `grep -n 'url:' lib/chat-files-v2.ts` at the pin | only the two comments (`:590`, `:663`) | — |

`FileAttachment` (`packages/plugin-types/src/providers/common.ts:13-30`):
`filepath?` is documented "Path to the file on disk (internal use)" (`:16-17`);
`url?` is "URL to fetch the file (alternative to data)" (`:26-27`). The commit
did NOT touch this file (the bug doc's fix step 3, "Consider documenting …", was
not done).

**Lookalikes that are NOT this bug** (a lane must leave them alone):
`app/api/v1/chats/[id]/files/route.ts:138-155` `chatFilePayload` sets
`url: entry.filepath` on every file the route hands back ("`url` is always the
same value as `filepath`; both are kept because clients read both",
`:135-136`) and the mount branches build `/api/v1/mount-points/…` into it
(`:320`, `:428`); `:517`/`:520` read `attachment.url` off a
`MountAttachmentEntry` (`lib/photos/chat-gallery.ts:118-126`, its `url` is
"The API URL that serves the bytes"); `lib/chat/transcript-projection.ts:172-177`
builds the same path into `filepath`. These are LISTING shapes for the SPA, a
different type from `FileAttachment`, and the commit touched none of them —
which is why the bug doc's "a grep … found no host reader" of `.url` is true
of `FileAttachment` even though `route.ts:517` exists.

### A3 The two plugins

**Z.AI** `plugins/dist/qtap-plugin-z-ai/provider.ts` (1.1.31 → 1.1.32).
`attachmentToImageUrl` post-fix (`:156-160`), with the doc-comment `:149-155`:

```ts
private attachmentToImageUrl(attachment: FileAttachment): string | null {
  if (attachment.data) return `data:${attachment.mimeType};base64,${attachment.data}`;
  if (attachment.url && /^https?:\/\//i.test(attachment.url)) return attachment.url;
  return null;
}
```

Pre-fix the two `if`s were the other way round and the second had no regex:
`if (attachment.url) return attachment.url; if (attachment.data) return
\`data:…\`;`. The comment (identical on both plugins and in both `index.js`):
"The bytes win whenever there are any. A URL is used only when it is one the
provider can actually fetch: an absolute http(s) address. Preferring `url` sent
the host's server-relative path for a vault image straight to the provider,
which answered "messages[0].content[0].file must contain at least one of
file_id, file_url, or file_data" and failed the turn (bug 174)."

The CALLER did not change. `buildUserContent` (`:104-147`): no attachments →
`msg.content` (`:111-113`); else `parts`, text part first when `msg.content`
is truthy (`:116-118`); per attachment the MIME gate (`:121-127`, `Unsupported
file type: ${mime}. Z.AI supports: ${list}`), then (`:129-136`):

```ts
const url = this.attachmentToImageUrl(attachment);
if (!url) { failed.push({ id: attachment.id, error: 'Attachment missing data or URL' }); continue; }
parts.push({ type: 'image_url', image_url: { url } }); sent.push(attachment.id);
```

and `if (parts.length === 0) parts.push({ type: 'text', text: '' })`
(`:142-144`). So a relative `url` with no bytes now takes the SAME failure arm
a bytes-less, url-less attachment always took: the string `'Attachment missing
data or URL'` (`:133`) is unchanged, the part is not pushed, the id is not in
`sent`.

**NanoGPT** `plugins/dist/qtap-plugin-nanogpt/provider.ts` (1.2.8 → 1.2.9).
`attachmentToImageUrl` `:109-113` (byte-identical body to Z.AI's); caller
`buildUserContent` `:133-160`; MIME gate `:145-151` (`Unsupported file type:
${mime}. NanoGPT forwards images only (${list}).`); the no-url arm `:152-156`
with `error: 'Attachment missing data or URL'` (`:154`); the empty-parts
fallback below `:160`. Unchanged.

The regex is `/^https?:\/\//i` — anchored, ASCII letters only, the `i` flag.
(JS canonicalization in non-`u` mode never maps a code unit ≥ 128 onto one
< 128, so `eq_ignore_ascii_case` on the 7/8-byte prefix is exact; see D6.)

### A4 Every other plugin's attachment → image handling at the pin (all UNTOUCHED)

| plugin | site | rule | reads `url`? |
|---|---|---|---|
| OpenRouter | `provider.ts:101-114` `buildMessageContent`: filter `SUPPORTED_IMAGE_MIME_TYPES.includes(a.mimeType) && (a.data \|\| a.url)` (`:103`); `const url = img.url ?? \`data:…\`` (`:110`) | **`url` first, NULLISH** | yes — still prefers `url`, no absolute-URL guard |
| OpenRouter | `:121-144` `collectAttachmentResults`: `a.data \|\| a.url` → `sent`; else `'Image attachment missing data and url'` (`:133`); non-image → `OpenRouter ${mime} attachments are not yet implemented` (`:138`) | | yes |
| OpenRouter | `:146-152` `hasImageAttachments` (the vision trigger) | `a.data \|\| a.url` | yes |
| OpenAI | `:148` `if (!attachment.data)`; `:158` `image_url: \`data:…\`` | data only | no |
| Anthropic | `:284` `if (!attachment.data)`; `:299`, `:326` `data: attachment.data` | data only | no |
| Google | `:492` `if (!attachment.data)`; `:507` | data only | no |
| Grok | `:157` `if (!attachment.data)`; `:169` `image_url: \`data:…\`` | data only | no |
| DeepSeek | `:210`, `:310` `collectAttachmentFailures` — drops every attachment (`'DeepSeek models do not accept file attachments…'`, `:117`) | none | no |
| Ollama | `:77-85` `collectAttachmentFailures` (`'Ollama file attachment support not yet implemented…'`) | none | no |
| openai-compatible (the OAC base) | `grep -n attachment provider.ts` → zero hits; `NanoGPTProvider extends OpenAICompatibleProvider` (`nanogpt/provider.ts:90`) and overrides the content build itself | none | no |

So after this commit a mount attachment reaches OpenRouter with no `url` key
(the loader's half) and OpenRouter's `??` falls to `data:` — on BOTH sides,
once v5 ports the loader. OpenRouter's `url`-first rule stays as is; the bug
doc's "Who it bites" names only Z.AI and NanoGPT.

### A5 The new tests (oracle material)

**`__tests__/unit/plugins/image-attachment-url-preference.test.ts`** (110
lines, new). Mocks `openai` (`:18-27`) so `chat.completions.create` is a
`jest.fn` resolving `FAKE_COMPLETION` (`:31-34`); `send()` (`:63-76`) calls the
REAL `ZAIProvider` / `NanoGPTProvider` `sendMessage({ model, messages:
[{ role: 'user', content: 'What is this?', attachments: [attachment] }] },
'test-key')` and reads back the LAST message's `image_url` parts plus
`response.attachmentResults`. Constants: `VAULT_PATH =
'/api/v1/mount-points/701e03fd-9bed-4b75-b126-f25e5498eaba/blobs/photos/2026-10-01T20-19-26.593Z-finally-laura.webp'`
(`:36-37`), `WEBP = { id: 'attachment-1', filename: 'finally-laura.webp',
mimeType: 'image/webp', size: 2048, data: 'UklGRg==' }` (`:39-45`). Run
`describe.each` over `['Z.AI', glm-5.3-flash]` and `['NanoGPT',
some/vision-model]` (`:58-61`):

| `it` (`:79-109`) | input | expectation |
|---|---|---|
| sends a data: URI when an attachment carries both data and a relative url | `{ ...WEBP, url: VAULT_PATH }` | `imageUrls == ['data:image/webp;base64,UklGRg==']`; `sent == ['attachment-1']` |
| prefers the bytes even over an absolute url | `{ ...WEBP, url: 'https://example.com/a.webp' }` | `imageUrls == ['data:image/webp;base64,UklGRg==']` |
| forwards an absolute http(s) url when there are no bytes | `{ ...WEBP, data: undefined, url: 'https://example.com/a.webp' }` | `imageUrls == ['https://example.com/a.webp']`; `sent == ['attachment-1']` |
| refuses a relative url with no bytes rather than sending it | `{ ...WEBP, data: undefined, url: VAULT_PATH }` | `imageUrls == []`; `sent == []`; `failed[0].error` matches `/missing data or URL/i` |

**`__tests__/unit/lib/chat-files-v2-mount-document.test.ts`** (+39): two cases
appended to the bug-38 `describe` (`:69-106`). The helper `reposWithDocument`
(`:31-50`) mocks `getRepositories` with a link `{ id: 'link-1', fileId:
'file-1', mountPointId: 'mp-1', relativePath: 'Notes/field.md', fileName:
'field.md', originalFileName: null }`, no blob, a document with content
`'# Field Notes\n\nThe zeppelin listed to starboard.'`.

| `it` | expectation |
|---|---|
| bug 174 — sets no url on a document attachment, only filepath | `loadChatFilesForLLM(['link-1'])` → `attachment.url` undefined; `filepath == '/api/v1/mount-points/mp-1/files/Notes/field.md'` |
| bug 174 — sets no url on a blob (vault image) attachment, only filepath | link `link-2`/`file-2`/`photos/laura.webp`; blob `{ id: 'blob-2', storedMimeType: 'image/webp' }`, `readData` → `Buffer.from('RIFF0000WEBP')` → `url` undefined; `filepath == '/api/v1/mount-points/mp-1/blobs/photos/laura.webp'`; `data == bytes.toString('base64')`; `mimeType == 'image/webp'` |

### A6 The non-code files

| file | change |
|---|---|
| `README.md:11` | the "This Version" badge `4.10.0--dev.105` → `4.10.0--dev.106` (the only hunk) |
| `package.json`, `packages/quilltap/package.json`, `package-lock.json` (2 lines) | `4.10.0-dev.105` → `4.10.0-dev.106` |
| `plugins/dist/qtap-plugin-z-ai/{manifest.json,package.json}` | `1.1.31` → `1.1.32` |
| `plugins/dist/qtap-plugin-nanogpt/{manifest.json,package.json}` | `1.2.8` → `1.2.9` |
| `docs/CHANGELOG.md` | +14: a new `#### Fix bug 174: vault images sent to Z.AI and NanoGPT as a server path` block FIRST under `### 4.10-dev`, above "Scene note on chained multi-character turns" |
| `docs/developer/bugs.md` | `Last Updated` 2026-09-25 → 2026-10-01; the Status paragraph `1–173` → `1–174` with the bug's sentence prepended; one new index row `\| 174 \| … \| 2026-10-01 \| 2026-10-01 \| Medium \| … \| \`lib/chat-files-v2.ts\` +2 plugins \| Not assessed \|` (`:1164`) |
| `docs/developer/bugs/fixed/bug-174-vault-image-relative-url-to-provider.md` | new (115 lines) |

### A7 v4's own named follow-up

The bug doc's FIXED paragraph ends: "Failed calls are still not written to
`llm_logs`; that is left as a follow-up." — and "Why it survived" records
"Failed calls are not written to `llm_logs`, so the bad request body was never
visible." NOT in this commit; not a bug-174 port item.

### A8 What v4 records for a refused attachment (the comparand)

Per plugin (A3): `attachmentResults = { sent: string[], failed: { id, error }[] }`
with `error: 'Attachment missing data or URL'`, no `image_url` part, the text
part alone (or `[{ type: 'text', text: '' }]` when there is no text either).
Downstream the ledger reaches `lib/chat/file-attachment-fallback.ts:95-103`
`verifyImageReachedModel` (`reason: \`the provider reported the attachment as
not sent: ${mine.error || 'no reason given'}\``), `primary-stream.service.ts:259`/`:318`
and `native-tool-loop.service.ts:372`/`:442` (`streaming.attachmentResults =
chunk.attachmentResults || null`), `provider-failover.service.ts:697`, and is
persisted by `message-finalizer.service.ts:88`/`:460`. None of that moved.

---

## §B v5 on `main` (`6d44cfae2`)

### B1 The loader twin — `crates/quilltap-core/src/services/chat_files.rs`

`load_mount_file_as_attachment` (`:187-347`), a `Value` bag built with `json!`.

**Native-text branch** (`:223-263`): `let url = format!("/api/v1/mount-points/{}/files/{}", …)`
(`:241-245`), then (`:251-259`)

```rust
json!({ "id": …, "filepath": url, "filename": …, "mimeType": text_mime, "size": size, "data": data, "url": url })
```

— `"filepath": url` at `:253`, `"url": url` at `:258`.

**Blob branch** (`:329-346`): `let url = format!("/api/v1/mount-points/{}/blobs/{}", …)`
(`:330-334`); `"filepath": url` at `:340`, `"url": url` at `:345`.

Key order on both: `id, filepath, filename, mimeType, size, data, url` —
v4's PRE-fix order exactly, so the port is "delete the last key" on each
branch (and, optionally, rename the binding `url` → `filepath` to match v4's
inlining; cosmetic). The legacy `files` branch (`:386-395`) has no `url`, as
in v4.

**No other v5 site puts a `url` key on a provider attachment.** Measured
(`grep -rn '"url"' crates/quilltap-core/src/services/`): the two above;
`chat_avatars.rs:104`, `:247` (`"url": Value::Null` on the chat gallery /
avatar listing rows — a LISTING shape); `help_chat/mod.rs:71` (help URLs).
`file_fallback.rs`, `message_context.rs`, `message_attachment_adapter.rs`,
`lantern_notifications.rs`, `chat_media.rs`: zero `"url"` hits. The listing
twins of A2's lookalikes: `api/chat_media.rs:1006-1013` (`"filepath":
attachment.url`, `"url": attachment.url` off
`photos::chat_gallery::resolve_message_attachment_entries_db`) and
`:2457-2460`, `:2541-2544` (the attach-mount-file response's `url: filepath`
pair) — all v4-faithful and untouched by the commit.

**The bag travels whole.** All three callers forward the loaded `Value`
unchanged to the builders: `load_and_process_files` `:502-503` →
`attachments_to_send` (`:530`); `load_lantern_images` `:562` → `keep.push(fa)`
(`:598`) → `LanternLoad.attachments` (`:602-605`); `load_user_attachments`
`:623` → `UserAttachmentFile { attachment: fa, … }` (`:657-658`). Each reads
only `data` for the fallback (`:517`, `:588`, `:649`). So the `url` key the
loader sets today reaches `zai_user_content` / `nanogpt_user_content` on a real
Salon turn — v5 is bitten end to end, not just in the builder.

`chat_files.rs` has NO `#[cfg(test)] mod tests` (grep: none), so nothing pins
the bag's keys at the unit level; the pins are the harness families (§C).

### B2 Every v5 consumer of an attachment's `url`

All in `crates/quilltap-core/src/model/request_builder/chat_completions.rs`;
the helpers in `mod.rs`: `att_str` (`:269-271`, JS-truthy — `Some` iff
present, a string, non-empty), `att_id` (`:275-280`), `att_fail` (`:283-288`,
`results.failed.push(StreamAttachmentFailure { id, error })`).

**Z.AI** `zai_user_content` (`:94-137`), the arm at `:119-129`:

```rust
// v4 `attachmentToImageUrl`: `attachment.url` first, else the data URL.
let url = match att_str(a, "url") {
    Some(u) => u.to_string(),
    None => match att_str(a, "data") {
        Some(d) => format!("data:{mime};base64,{d}"),
        None => { att_fail(results, a, "Attachment missing data or URL"); continue; }
    },
};
parts.push(json!({ "type": "image_url", "image_url": { "url": url } }));
results.sent.push(att_id(a));
```

MIME gate `:108-118` (`Unsupported file type: {mime}. Z.AI supports: {list}`);
empty-parts fallback `:133-135`. The doc-comment `:69-93` carries v4's bug-104
why-comment and must gain the bug-174 one.

**NanoGPT** `nanogpt_user_content` (`:163-210`), the arm at `:188-200`
(identical body; the comment `:188-190` "`attachment.url` first, else the data
URL, else nothing — a row missing BOTH is the one failure the MIME gate cannot
pre-empt"); failure string `:196`; MIME gate `:177-186`; empty-parts `:206-208`.

**OpenRouter** (`:212-307`) — three reads, all v4-faithful and UNTOUCHED by
the commit: the filter `att_str(a, "data").is_some() || att_str(a, "url").is_some()`
(`:226`); the part `match img.get("url").and_then(Value::as_str) { Some(u) =>
u, None => format!("data:…") }` (`:241-247`, the doc-comment `:213-215` names
it NULLISH — a present-but-empty `url` ships verbatim, "carried faithfully");
`openrouter_collect_attachment_results` `:267-270` (`"Image attachment missing
data and url"`); `openrouter_has_formattable_images` `:304`. Leave all three.

**Every other builder reads `data` only**: `anthropic.rs:246` (`let Some(data)
= att_str(a, "data") else { att_fail(…, "File data not loaded") }`),
`responses_api.rs:83-84` (same string), `google.rs:372-373` (same). `tool_wire/
text_parsers.rs:174` is a tool-ARGUMENT named `url`, unrelated.
`crates/quilltap-host`: zero attachment-`url` reads (`grep` over `src`, after
excluding `base_url`/`image_url`/webhook strings).

The builder module's own tests (`#[cfg(test)]` from `:1765`) contain NO
attachment case — `"attachments"`, `att-img`, `attachment_results` do not
appear after that line. The only pins on the url/data preference are the
recorded corpus rows (§C2).

### B3 The failure-path shape (to match A8 byte-for-byte)

`StreamAttachmentResults { sent: Vec<String>, failed: Vec<StreamAttachmentFailure> }`,
`StreamAttachmentFailure { id: String, error: String }` (`model/stream.rs:262-274`,
`Serialize`); the harness compares it as JSON against the row's
`attachmentResults` (`request_builder_equivalence.rs:753-757`). The string
`"Attachment missing data or URL"` already sits on the right arm at `:125` and
`:196`; the port adds a case that reaches it (a relative `url`, no `data`) and
keeps `sent` empty for it.

The port of A3 in `att_str` terms: `if let Some(d) = att_str(a, "data") {
data: URI } else if let Some(u) = att_str(a, "url") && is_absolute_http(u) {
u } else { att_fail(…) }` — `att_str`'s non-empty filter already matches v4's
`attachment.url &&` truthiness; only the prefix test is new.

---

## §C Families

### C1 `file_attachment_tier3_equivalence` — RED at the pin (predicted)

Oracle `harness/oracle/cases/file-attachment-tier3.test.ts`; fixture
`harness/oracle/fixtures/build-file-attachment-fixture.ts` (ONE mount link,
an image blob: `spec.mount.relativePath` / `storedMimeType` `:301-306`, plus
`extraMounts` `:317-322`; `meta.mountLinkId` `:333`). Three arms call the REAL
`loadChatFilesForLLM` on a mount link and emit the WHOLE attachment array as
the comparand:

| label | site | call |
|---|---|---|
| `lcffl_mount` | `:524-527` | `loadChatFilesForLLM([meta.mountLinkId], { provider: 'DEEPSEEK' })` → `result: attachments` |
| `lcffl_shrink_mount` | `:575` | `ids: [extraMountLinkIds.shrinkMount]`, `provider: 'NANOGPT'` |
| `lcffl_shrink_no_autoresize_mount` | `:578` | same ids, `autoResize: false` |

The Rust side asserts the whole object (`crates/quilltap-harness/tests/
file_attachment_tier3_equivalence.rs:861-866` `assert_eq!(&got, want,
"lcffl_mount diverged")`; the shrink arms at `:872-` likewise). At
`f6426e196` v4's rows lose their `url` key; v5 still emits it → three reds,
all on the LOADER half. The `lap_*` (`:348-358`) and `fb_*` (`:385-415`) arms
use `fileKeys` → legacy `files` rows → no `url` → neutral.
`lcffl_shrink_ladders` (`:590-600`) compares `{ bytes, calls }` only →
neutral. **The native-text branch is NOT exercised by any arm** (no `native`/
`document`/`.md` in the case; the fixture has no document mount) — the lane
should add one, with v4's `link-1`/`Notes/field.md` jest shape as the model,
so the second hunk gets a differential too.

Recipe (`python3 harness/tools/recipe_sweep.py --show file_attachment_tier3_equivalence`):
build the fixture with `QT_FIXTURE_FILE_ATTACH_MAIN=/tmp/qt-fa-main.db
QT_FIXTURE_FILE_ATTACH_MOUNT=/tmp/qt-fa-mount.db $N/node --import tsx
…/build-file-attachment-fixture.ts` from the v4 checkout, copy the case +
`fixtures/file-attachment.json` + `lib/shrink-script.ts` into
`/tmp/qt-oracle-run`, `jest … -- file-attachment-tier3` with
`QT_ORACLE_OUT=/tmp/oracle-file-attachment.ndjson`; run
`QT_ORACLE_FILE_ATTACHMENT=… cargo test -p quilltap-harness --test
file_attachment_tier3_equivalence -- --nocapture`. Run it at `ca363178d`
(green) and `f6426e196` (red-first on the three arms) — the pin-both-ways
proof.

### C2 `request_builder_equivalence` — 4 rows RED after a re-record (predicted)

A `committed_corpus` family (`--show` prints an empty regen):
`harness/oracle/fixtures/request-envelopes/request-envelopes.recorded.ndjson`,
385 rows, recorded by `harness/oracle/providers/record-request-envelopes.mjs`
through `regenerate-request-envelopes.sh` (`V4=… V5=… bash
regenerate-request-envelopes.sh`; the recorder imports each plugin's
`provider.ts` from the plugin dir, `:52-59`, so it sees the fix; both modes,
stream-first). The attachment vectors (`:201-261`): `IMG_ATT` (data, no url),
`IMG_NO_DATA` (`att-nodata-1`, neither), `IMG_ATT_URL` (`:252-258`,
`att-img-url-1`, `url: 'https://cdn.example.invalid/remote.png'`, no data),
`IMG_ATT_URL_AND_DATA` (`:260`, `att-img-both-1`, both). The comment at
`:259` — "Both present — the precedence pin: `url` wins and the data is never
encoded." — is the pre-fix rule and goes stale.

Rows carrying a `url` today (8 of 385):

| provider / case (× stream, send) | recorder line | recorded `image_url.url` | at `f6426e196` |
|---|---|---|---|
| `z-ai` / `image-attachment-url-wins` | `:375` (`glm-4.6v`) | `https://cdn.example.invalid/both.png`; `sent: ['att-img-both-1']` | **MOVES** → `data:image/png;base64,…` (A5 case 2) |
| `nanogpt` / `image-attachment-url-wins` | `:618` | same | **MOVES** |
| `nanogpt` / `image-attachment-url` | `:616` | `https://cdn.example.invalid/remote.png` | unchanged (absolute, no bytes — forwarded; A5 case 3) |
| `openrouter` / `image-attachment-url-wins` | `:446` | `https://cdn.example.invalid/both.png` | unchanged (plugin untouched) |

Z.AI has NO url-only vector (`:375` is the only `url` line in its block), and
no provider has a RELATIVE-url vector — v4's two refusal shapes (A5 cases 1
and 4) are unrecorded. The lane should add `IMG_ATT_REL_URL_AND_DATA` (data +
`VAULT_PATH`) and `IMG_ATT_REL_URL` (`VAULT_PATH` only) and record them for
z-ai and nanogpt (2 vectors × 2 providers × 2 modes = +8 rows → 393): the
first sends `data:`, the second records `attachmentResults.failed[0].error ==
'Attachment missing data or URL'`, `sent: []`, and a parts array of the text
part alone. The count floor `rows >= 360` (`request_builder_equivalence.rs:772`)
and the `named_rows == 18` pin (`:773-776`) are unaffected; the `385` in the
comment at `:768` should move with the count. ⚠ The re-record must run with
the plugin dirs AT the pin and their `node_modules` installed (the recorder
resolves the SDK from the plugin dir); the ledger records HEAD's SDKs equal
the recorded ones (openai 7.23.0).

### C3 Predicted NEUTRAL (run to prove)

| family | why neutral |
|---|---|
| `orchestrator_tier3_equivalence` | uses the REAL `loadChatFilesForLLM` (`orchestrator-tier3.test.ts:495`) but every attachment id in `orchestrator-tier3.json` is a `files` row (11 in the `files` spec, incl. the 8 `fsmBytesFill` WebPs, `:503-510` → `fsmByFileId`) except `d3ff0001-…f1` on the Lantern message at `:841`, which has no `files` row and no mount link → skipped on both sides (`:500-502`). No mount attachment → no `url`. Recipe via `--show orchestrator_tier3_equivalence` (`TZ=UTC`, `QT_FIXTURE_ORCH_*`). |
| `enclave_step_tier3`, `regenerate_swipe_tier3`, `salon_swipe_generate` | `jest.doMock('@/lib/chat-files-v2', … loadChatFilesForLLM: async () => [])` (`:317-319`, `:237-239`, `:229-231`) |
| `query_param_semantics` | mocks the module whole (`:603`) |
| `photo_tools`, `fallback_engine` | import only `file-attachment-fallback` (`verifyImageReachedModel`), untouched |
| `files_routes_equivalence`, `chat_gallery_equivalence`, the attach-mount-file route family (`attach-file.json`) | the listing/attach `url` comes from `chat-gallery.ts` / `chatFilePayload`, untouched (A2 lookalikes) |
| `request_builder_equivalence`'s anthropic / openai / grok / deepseek / ollama / openai-compatible rows; `request_builder_google_wire_equivalence` | `data`-only or attachment-dropping builders |

The sweep driver lists 620 families; the three relevant names are
`file_attachment_tier3_equivalence`, `orchestrator_tier3_equivalence`,
`request_builder_equivalence` (the ledger's shorthand `file_attachment_tier3`
/ `request_envelopes` are not driver names — `--show` rejects them).

---

## §D Traps

1. **`docs/v4/` LAGS the pin on exactly the commit's doc files.**
   `docs/v4/CHANGELOG.md`'s first `4.10-dev` block is still "Scene note on
   chained multi-character turns"; `docs/v4/developer/bugs.md:8` still reads
   `Bugs **1–173**` with no row 174; `docs/v4/developer/bugs/fixed/` holds
   173 files (through `bug-173-raw-sql-buffer-output.md`) against v4's 174.
   Mirror the three. The root `README.md` badge has NO mirror (`docs/v4/`
   carries only `packages-quilltap-README.md`, and `packages/quilltap/README.md`
   did not move) — NO-PORT.
2. **`help/` did not move** (empty diff) — no re-vendor, the 129-file count
   stands, `help_tree_equivalence` neutral.
3. **Plugin versions are recorded nowhere in v5.** The generated manifests
   (`crates/quilltap-core/src/provider_manifest/manifests/{nanogpt,z-ai}.json`)
   have no `version` key (grep count 0); `1.1.31`/`1.2.8` appear only in the
   P4.D232 survey prose. NO-PORT for the four stamps; nothing to bump.
4. **The SPA never reads a provider attachment's `url`.** `MessageAttachment`
   (`apps/web/src/app/core/core-contract.ts:2997-3003`) is `id, filename,
   filepath, mimeType, sha256?` — no `url`; the `url` fields that exist
   (`chat-files.api.ts:20`, the `ChatFilesEntry` doc at `core-contract.ts`
   ~`:6765`) are the files LISTING's, fed by the untouched route. No SPA hunk,
   no Playwright change; v4 shipped no client change either.
5. **The bug doc's "no host reader" grep.** `app/api/v1/chats/[id]/files/route.ts:517`,
   `:520` DO read `attachment.url` — off `MountAttachmentEntry`, not
   `FileAttachment`. A lane grepping `.url` will find them (and v5's
   `api/chat_media.rs:1006-1013` twin); they are not bug-174 sites.
6. **The regex.** `/^https?:\/\//i` is anchored and ASCII; in JS's non-`u`
   mode a code unit ≥ 128 never canonicalizes onto one < 128, so
   `s.as_bytes()[..7].eq_ignore_ascii_case(b"http://") ||
   …[..8].eq_ignore_ascii_case(b"https://")` is exact — no `regex` crate
   needed (the memory note's "case folding" axis does not bite here). Pin
   `HTTP://`, `Https://` accepted and `qtap://`, `/api/v1/…`, `data:` refused.
7. **v4 fixed BOTH halves; port both, and prove each by its own family.** The
   loader half is what fixes real turns (B1: the bag travels whole); after it
   alone, Z.AI/NanoGPT would already receive `data:` for every mount
   attachment and C1 goes green. The builder half is reachable only by a
   hand-built bag carrying `url` — the corpus rows (C2) and v4's jest file
   (A5) are its only witnesses, so without the C2 vectors the builder change
   would ship unpinned.
8. **`att_str` is already JS-truthy** (`""` = absent), matching v4's
   `attachment.url &&`; do not add a second emptiness check.
9. **Key order survives.** v4 removed the LAST key; dropping `"url"` from the
   two `json!` bags keeps `id, filepath, filename, mimeType, size, data`. The
   `let url =` binding name is now a misnomer (v4 inlined the template into
   `filepath`) — renaming is optional and must not change the string.
10. **OpenRouter is a deliberate non-change.** Its `img.url ?? data:` and the
    `data || url` filters (B2) match v4 at the pin; a lane that "fixes" them
    reddens `openrouter/image-attachment-url-wins` (C2) against v4's own
    bytes.
11. **Regen rule:** the ledger's §2 says PIN REQUIRED at `ca363178d` for every
    other family; this round's families regenerate at `f6426e196` as the
    target (the ledger's §3 row names exactly these families). Run C1 at BOTH
    pins; C2 is re-recorded once at the target.
12. **v4's follow-up (A7)** is not a port item; carry it as a candidate v4
    note ("failed calls write no `llm_logs` row"), not an order line.

---

## What the order should say (candidates)

**Tier 2 (the loader, real-DB differential):**
- `services/chat_files.rs:258` and `:345`: delete the `"url"` key on both
  branches; carry v4's `:663-666` why-comment on the blob branch and the
  `:590` pointer on the native-text branch.
- `file_attachment_tier3_equivalence` regenerated at `f6426e196`: red-first on
  `lcffl_mount`, `lcffl_shrink_mount`, `lcffl_shrink_no_autoresize_mount`;
  green after; green at `ca363178d` before the change (pinned both ways).
- NEW arm for the native-text branch (a `.md` document mount in the fixture;
  v4's `link-1`/`Notes/field.md` shape) — the second hunk's only possible
  witness.

**Tier 1/wire (the builders, recorded corpus):**
- `chat_completions.rs:119-129` and `:188-200`: bytes first, then `url` only
  when `is_absolute_http(url)`, else `att_fail(…, "Attachment missing data or
  URL")`; a shared `fn is_absolute_http(&str) -> bool` with the ASCII
  prefix test; carry the bug-174 comment on both.
- `record-request-envelopes.mjs`: retire the `:259` "url wins" comment; add
  `IMG_ATT_REL_URL_AND_DATA` and `IMG_ATT_REL_URL` (the `VAULT_PATH` bytes
  from A5); record them for z-ai and nanogpt in both modes; re-record the
  corpus at the pin from the plugin dirs; update the `385` prose at
  `request_builder_equivalence.rs:768`.
- `request_builder_equivalence` red-first on the four `url-wins` rows at the
  new corpus, green after; the two new refusal rows comparing
  `attachmentResults` byte-for-byte; the openrouter rows unchanged (pin the
  non-change by name).
- A unit twin of v4's four `it`s per provider in `chat_completions.rs`'s test
  module (the module has no attachment test today).

**Tier 3 / neutrality:** run `orchestrator_tier3_equivalence` at the target
(no mount attachment in its corpus — expect ok); the three swipe/enclave
families and `query_param_semantics` are neutral by construction (mocked
loader) — list, don't run.

**NO-PORT / mirror:** the four version stamps, the lock, the README badge;
MIRROR `docs/CHANGELOG.md`, `docs/developer/bugs.md`, the new
`bugs/fixed/bug-174-…md` into `docs/v4/`. No `help/`, no SPA, no Playwright.

**💸 dogfood:** a vault photo on a Z.AI vision profile (the bug's own
signature: the 400 gone, the `data:` body logged before the HTTP call — a bad
key makes it free), the same on NanoGPT, and an uploaded (non-vault) image on
both to prove the legacy branch unmoved.
