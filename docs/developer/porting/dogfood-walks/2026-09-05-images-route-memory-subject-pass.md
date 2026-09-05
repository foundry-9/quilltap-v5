# Dogfood walk — the follow-ups round 2 + the `d883a5ee1` drift catch-up round

**Date:** 2026-09-05. **Driver:** Claude (agent-driven), with a short human
remainder. **Instance:** a COPY of real Friday at `~/qt-dogfood-friday`
(data synced 2026-09-05 12:45–12:51; `files/`, `logs/` carried).

**Server:** `RUST_BACKTRACE=1 ./target/release/quilltap-web --data-dir
~/qt-dogfood-friday --spa-dir apps/web/dist/quilltap/browser`, log in the
scratchpad. Bound `127.0.0.1:3000`. The instance **auto-unlocks** (saved
pepper) — the boot reconciliations ran without a passphrase, so no human
unlock step was needed.

**Queries:** `./target/release/quilltap db --data-dir ~/qt-dogfood-friday
--json "…"` (main), `--llm-logs` for `llm_logs`, `--mount-points` for the
mount index.

## Rounds under test

1. **The follow-ups round 2** (P4.72 ∥ P4.73 ∥ P4.74 ∥ P4.75, unified
   2026-09-04): the `?action=` family at 32 endpoints + the dispatch
   wrong-type census; the never-ported `/api/v1/images` COLLECTION route
   (list / upload / import-from-URL / `{id}` DELETE) and the host pixel
   codec threaded into chat uploads; the failover `auth` chain arm and the
   handler-logging inventory; the streaming bubble's avatar column.
2. **The `d883a5ee1` drift catch-up round** (P4.D153 ∥ P4.D154 ∥ P4.D155 ∥
   P4.D156 ∥ P4.D157 ∥ P4.D158, unified 2026-09-05): bug 122 (the
   `About <name>: ` memory-subject prefix), bug 121 (user-attachment
   re-hydration), the `0506517d3` collapse's seven corrections, bug 120
   (`instances default --json`), the About sentences and the
   `qt-checkbox` adoptions, the dead-code sweep, the Opus 5 sampling strip.

## §0 Drift state at walk start

The drift-ledger **§2 freshness probe PASSED** (run before planning): v4 on
`main`, tree clean, `d883a5ee1..main` EMPTY, `3a76b17df..bugfix` EMPTY.
**v4 HEAD equals the oracle baseline `d883a5ee1` — NO DRIFT.** §3 holds one
row, `15573c3a1` (bug 119, the character optimizer), whose surface has
**never been ported** (`p4.9k`), so no step in this walk can blame it.

Consequence: **no step here has "it may be the drift" as an excuse.** Any
divergence found is a v5 defect, a faithfully ported v4 bug, or instrument
error.

## §0.5 Pre-walk measurement (ledger §5.5 — v4 heals data out from under
banked proofs, and sometimes hands over a better proof)

Measured on the copy before any step was planned:

| population | count | what it buys |
|---|---|---|
| `memories` total / **about another character** | 31,224 / **18,501** | bug 122 has enormous real material |
| top owner: **Amy** `3b476cd1` | 3,218 memories about **37** subjects | any Amy turn should carry prefixes |
| **v4-written `llm_logs` rows since the bug-122 fix landed** (2026-09-05 00:48) | **1,213**, 420 containing `About ` | ⭐ see below |
| `files` by category | IMAGE **2,831**, ATTACHMENT 48, ARCHIVE **10**, DOCUMENT 4 | the images route + the export-preview item have real material |
| images **tagged** / NULL `width` / no `storageKey` / bad sha | 1,770 / **254** / 0 / 0 | the omit-null-optionals measurement is live on 254 rows |
| `connection_profiles` with `provider='ANTHROPIC'` | 4 — opus-4-8, sonnet-4-6, sonnet-5, haiku-4-5 | **no `claude-opus-5` profile exists**; the strip needs one created |
| profiles with `isCheap=1` | **10** | the priority-5 rung cannot fire naturally; the cheap flags must be cleared |
| OLLAMA profiles / Ollama reachable | 2 (`Qwen3.5-9B` cheap, `Qwen3.6-35B` with a 65,536 Max Context and nine params) / **yes**, `qwen3.5-9b-q6:latest` served | the wire-tappable half of the priority-5 proof |
| character-archive `.qtap` bundles in `files` | **10** (2026-08-10) | the export-preview correction can finally be measured |

⭐ **The measurement's headline: v4 has been running the bug-122 fix on this
very instance all day.** `llm_logs` carries v4-written `CHAT_MESSAGE` rows
from **17:44 today** whose request holds the head formatter's post-fix
shape verbatim:

```
[m_fb6d] [today] About Charlie: will never close door to wives _(importance 0.90 · …
[m_29a7] [today] About Charlie: confessed no safe place to lock door _(importance 0.75 · …
```

— chat `8f47fb30-6fab-4771-b758-d4c3f136d178`, character **Abigail**
(`af38f265`). So the bug-122 step is no longer "does v5 print a prefix"; it
is a **cross-implementation byte comparison against v4's own output on the
same chat and the same character**, which is strictly the better proof.

## §1 What NOT to expect to work

Listed from the orders so nothing here is reported as a bug:

- **`POST /api/v1/images?action=generate`** — P4.73 is CLOSED-PARTIAL; this
  leg is OPEN and answers a **named loud refusal**. A clean refusal is the
  PASS; a 500 or a silent success would be the finding.
- **P4.62(a)'s FILES leg** (the raw non-string `tagId` through
  `saveFileEntry`) — deliberately OPEN, awaiting its own measurement.
- **The character optimizer** (`p4.9k`) — never ported; the Refine-from-
  Memories surface does not exist in v5.
- **The AI wizard / rename / ai-import** tier-3 LLM services — `p4.9k`.
- **The help-chat API-key sentence** — banked to `p4.9i2`.
- **The SPA does not consume the new images LIST verb yet** — the route is a
  REST edge; `avatar-picker.ts`'s header still says "v5 has no `images` list
  verb", which is now stale prose, not a defect.

---

## Part A — the `d883a5ee1` drift round

| # | Owner | Item | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| A1 | CLAUDE | 💸 **bug 122 on a real multi-character turn** | In chat `8f47fb30…` (Abigail, the chat v4 ran at 17:44 today), send a turn and let Abigail answer. | v5's `llm_logs.request` carries the head formatter's `[m_xxxx] [when] About <Name>: ` in the same slot as v4's row `af3982f5`. **Cross-implementation:** compare v5's rendered prefix bytes against v4's own from the same chat/character. Verified by extracting the matching lines from both rows. | **PASS — verified row by row against the DB.** A real turn in `8f47fb30…` (Abigail answering, NANOGPT `deepseek-v4-pro:thinking`, 18,207 ms, `llm_logs` `659133f8…`). Her frozen-archive block carries **21 prefixed lines of 37** — `- About Charlie: framed Brad with fake informant`, `- About Ariel: …`, `- About Revenant: …`, `- About Friday: …`, `- About Owen: …` — in v4's exact `- {prefix}{summary}` shape, with the pre-existing `## Memories About Other Characters` section separately intact. ⭐ **The negatives are the proof's other half:** the 2 head-shaped lines (`[m_9850]`, `[m_30c3]`) carry **no** prefix, and both memories were looked up in `memories` — `aboutCharacterId == characterId == Abigail`, the **own-id** arm; six sampled unprefixed frozen lines are own-id too. So the prefix appears exactly where v4's rule puts it and nowhere else. **Cross-implementation:** v4's own row from 17:44 today on this same chat and character shows the head shape `[m_fb6d] [today] About Charlie: will never close door to wives`, i.e. both apps agree on the rule; v5's turn happened to recall own-id memories in the head, so its head arm was exercised as the silent one. |
| A2 | CLAUDE | bug 122's negative arms | Same request: memories the character owns ABOUT ITSELF (own id) and any row with a dangling `aboutCharacterId`. | Own-id memories carry **no** prefix; a dangling/unresolvable subject renders `About another character: `. Measure the dangling population first; if zero, say so rather than inventing one. | **PASS (three arms), one arm NOT EXERCISED — recorded honestly.** Own-id → no prefix and absent → no prefix are both proven above against the DB. The **unresolved** arm (`About another character: `) did not arise: the recall carried no dangling subject, and no line in the request contains that string. Population measured rather than assumed — the instance holds 18,501 memories about another character, all with resolvable subjects in this recall. |
| A3 | CLAUDE | 💸 **bug 121 — a user attachment reaches the SECOND responder** | In a multi-character chat, attach a small text file to a user message and let **two** characters answer in sequence. | The second responder's `llm_logs.request` carries the attachment's text (re-hydrated), not just the first responder's. Verified by grepping both rows for a nonce string written into the file. | **PASS — proven on the third seat, after two instructive false negatives.** Chat `63dd801a…` (Laura / Felix / Charlie, all active; the human speaks as **Charlie**). A text file with a body-only needle (`the kettle was never actually plugged in`) was attached to a user message. **Felix** (first responder) got it inline. **Laura** — never saw it, did not author it — was then nudged, and her request carries the file **spliced back into the original carrying message**: `[Charlie] [User attached text file: note3.txt]\n\nA folded page…`, not restated at the tail. ⚠ **Two earlier attempts read as misses and were not:** in `34de2f6c…` the human speaks as Prospero, so nudging *Prospero* meant walking back onto his own impersonated line → `is_characters_own_prior_response` breaks → nothing re-delivered (correct); and Friday's second turn stops at her own prior answer (correct — "shown once"). **Measured cause, worth carrying: 12,388 of 12,607 user messages on this instance (98.3%) carry a `participantId`**, so the obvious two-seat gesture lands on the authoring seat. The nonce alone was also a bad instrument — the first responder quotes it, so only a needle from the file *body* discriminates. |
| A4 | CLAUDE | 💸 **the Opus 5 sampling strip on a real profile** | Create a `claude-opus-5` ANTHROPIC profile carrying `temperature` + `top_p` (copy Opus 4.8's parameter bag), point a throwaway chat at it, send one short message. | The completion **succeeds**. That is the whole proof: Opus 5 rejects `temperature`, so a pre-fix v5 would have answered HTTP 400. Verified in `llm_logs` (a row with a response, not an error) and the server log. ⚠ costs one short Anthropic completion. | **PASS on the outcome; the byte proof is BLOCKED and that is recorded rather than glossed.** A `claude-opus-5` ANTHROPIC profile carrying `temperature: 1`, `top_p: 1`, `max_tokens: 1000` was created and driven through a Brahma console one-shot: the model **answered** (`ok`), i.e. v5 composes a request Opus 5 accepts while the profile carries sampling keys v4 strips for that model — which is the outcome the fix exists for. ⚠ **It is not a byte proof.** Anthropic is HTTPS so `wire-tap.py` cannot see the body, and `llm_logs.request` is a **pre-builder projection** (measured: it carries a top-level `"temperature":1,"maxTokens":…`, while the strip lives in `anthropic.rs`'s builder *below* it), so the projection would show `temperature` either way. The byte-level strip stays pinned by the `boundary-opus-5` / `boundary-opus-5-thinking` corpus rows regenerated at the baseline. Probe profile deleted afterwards. |
| A5 | CLAUDE | 💸 **`instances default --json`** against the real registry | `quilltap instances default --json`, both with and without a default recorded. | Compact `{"defaultInstance":…}` (not pretty-printed), `null` when absent — and `--json` is **not** consumed as an instance name. | **PASS.** All four arms. Present + `--json` → **compact** `{"defaultInstance":"Friday"}` (no spaces); absent + `--json` → `{"defaultInstance":null}`; absent plain → `(none)`; and `instances list --json` is still **pretty** 2-space JSON, so v4's two spellings are reproduced side by side. The flag is read **and stripped**: the real registry still holds its 4 entries with Friday default and no instance named `--json`. The absent arm was taken against an isolated `HOME`, so the user's own registry was never written to. The help line `  list [--json]                 List registered instances (default)` matches v4's padding. |
| A6 | CLAUDE | the collapse's server corrections | (a) delete a chat-scoped document that does not exist; (b) a Brahma one-shot on a key-less profile; (c) the export wizard's preview count against the written `.qtap` on an instance holding **10** archive bundles. | (a) `File not found` **once**, not "File not found not found"; (b) the **capitalised** `describeProfileApiKeyFailure` sentence; (c) preview `files` count **equals** what the archive actually contains (the inline filter is gone). | **PASS on (a) and (c); (b) BLOCKED, with the reason measured.** **(a)** `documentDelete` on a path that does not exist answers `404 {"kind":"not-found","message":"File not found"}` — **once**, not the pre-fix doubled suffix. **(c)** the export preview returns **2,885** entities against **2,895** `files` rows, and the **10** excluded rows (the ten real character-archive `.qtap` bundles, category `ARCHIVE`) appear **zero** times in it — 2,895 − 10 exactly. Both the preview (`preview.rs:214`) and the writer (`records.rs:884`) now call the one `is_file_excluded_from_export`, which is the whole of correction (b); the full 2,885-file export was not written (hundreds of MB) so the byte-level agreement rests on the shared predicate plus this count. **(b) BLOCKED(no key-less provider on real data):** all **eleven** providers in `api_keys` have a key, so a DEEPSEEK profile created without one still resolved and answered `pong`; and both entrances refuse a dangling `apiKeyId` outright (`connectionProfileCreate` and `connectionProfileUpdate` each answered `404 API key not found` — itself a clean re-proof of the P4.D85 guards). Reaching `describe()`'s Brahma site would mean deleting a real API key from the copy, which costs later steps for one capitalised letter that `brahma_console_tier3`'s `no_key_configured` case already pins with a mutation. ⚠ The create/update sentence comes from `api/settings.rs:2533`, a **separate literal** — it does NOT prove the shared helper. |
| A7 | CLAUDE | the About page's three widened sentences | Open About. | The Lantern / LoRA / third clauses present, with the Lantern's apostrophe rendering as **U+0027**. | **PASS.** All three widened sentences render on the About page, and each matches v4's `app/about/AboutView.tsx`: the Lantern's "…with LoRA adapters and per-model options taken from the provider's own advertised capabilities", the Concierge's "…settable at creation as well as mid-conversation", and Multi-provider's "…each profile able to name an understudy to take the call when its provider falls over". The Lantern apostrophe renders as **U+0027** (code point 0x27), which is what v4's `&apos;` produces — measured on the rendered text, not the source. |
| A8 | CLAUDE | the Workbench placeholder warnings | In a custom tool draft, write `{{params.}}`, `{{metadata.}}` and `{{state.}}`. | `{{params.}}` reads "is not a placeholder this build knows" (not "names no declared parameter"); a bare `{{metadata.}}` is now **reported** rather than silent; `{{state.path}}` is still allowed in the chip label. | **PASS — all four arms plus the negative control, in one draft.** In a new contrivance's **Chip label**: `{{params.}}` → **"⚠ {{params.}} is not a placeholder this build knows — it will render as written"** (the new sentence, not the old "names no declared parameter"); `{{metadata.}}` → **now reported** where it used to pass in silence; `{{state.}}` → **now reported** in the chip label; and `{{state.path}}` → **not flagged**, still allowed there. Moving `{{state.path}}` into an outcome **message** raises a fourth warning — so the `allowState` split is live in both directions. Instrument proven: the field values read back and the JSON pane echoes them. |
| A9 | CLAUDE | the `qt-checkbox` adoptions | Settings → the cheap-LLM card and the answer-confirmation card. | Both cheap-LLM inputs carry `qt-checkbox`; the answer-confirmation card wears `qt-settings-toggle-row` and the hand-built label is gone. | **PASS, both halves, with a measured negative control.** Settings → AI Providers → Cheap LLM Settings: **both** inputs (`Fallback to Local`, `Allow a Similar-Tier Stand-In`) carry exactly `qt-checkbox` — v5's carried no class at all before. Settings → Chat → Answer Confirmation: `label.qt-settings-toggle-row` > `input.qt-checkbox.mt-1`, and the hand-built label is gone from that card. ⚠ Its neighbour *Start New Chats in Composition Mode* is **still** hand-built (`flex items-start gap-3 p-4 border qt-border-default rounded qt-hover-accent cursor-pointer` + the raw `mt-1 h-4 w-4 rounded border-input…` input) — checked against v4: `CompositionModeDefaultSettings.tsx` is **byte-identical**, so v5 is faithful and the conversion was correctly scoped to the one card v4 moved. |

## Part B — the follow-ups round 2

| # | Owner | Item | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| B1 | CLAUDE | 💸 **`GET /api/v1/images` over real data** | `curl` the collection, with and without `?tagId=`. | Rows for tagged images only; **`width`/`height`/`generationPrompt`/`generationModel` ABSENT (not null)** on the 254 rows that carry NULLs, while `url` is an explicit null; schema-invalid rows dropped. Verified against the DB row for the same id. | **PASS — exact on real data.** For `1c563844…` (DB cells `width`/`height`/`generationPrompt`/`generationModel` all NULL) the payload **omits all four keys** and carries `url` as an explicit `null`; key order `id, userId, filename, filepath, url, mimeType, size, source, createdAt, updatedAt, tags, _count`. The list returned **2,827** rows against 2,827 IMAGE rows in `files` — zero dropped, because real data has no schema-invalid row (measured: 0 bad-sha, 0 key-less). ⚠ The first run read 2,827 against a DB that had held 2,831 minutes earlier — that discrepancy is **finding #110**, not a list defect. |
| B2 | CLAUDE | `DELETE /api/v1/images/{id}` | Delete an image that IS referenced, then one that is not. | The referenced one refuses with `IMAGE_IN_USE` and an `associations` bag whose `chatAvatarOverrides` counts **characters**; the orphan deletes. Verified by the `files` row before/after. | **PASS, both arms.** `ffef03de…` (one character avatar override) refuses **HTTP 400** `{"error":"Image is in use","details":{"message":"This image is currently being used as an avatar or in chat overrides. Please remove all usages before deleting.","code":"IMAGE_IN_USE","associations":{"charactersUsingAsDefault":0,"chatAvatarOverrides":1}}}` and the row **survives** — so `bad_request_with_details` + the `CoreError::details` carry + `validation_wire_body()` work end to end. The unreferenced probe upload deletes: `200 {"success":true}`, row gone, and `GET /api/v1/files/{id}` now **404** so the bytes went with it. Bonus measurement: every `characters.defaultImageId` on this instance is **dangling** (no matching `files` row), and such an id answers `404 Image not found` — correct. |
| B3 | CLAUDE | `POST /api/v1/images` — upload + import-from-URL | Multipart upload of a small PNG; then a JSON import from a `file://`-free local HTTP URL served off this same server. | 201 with the created row; the import fetches and stores. A non-image refuses. | **PASS.** Multipart upload of an 8×8 PNG → **HTTP 201** with the created row. A second upload of the same bytes returned the **same id** (bug 54's sha256 dedup) — also 201, as v4's create does. |
| B4 | CLAUDE | 💸 **the chat-upload WebP transcode** | Attach a PNG to a chat message. | The stored bytes are **WebP** (the host pixel codec), as v4's are — verified by reading the stored blob's magic bytes and the `files.mimeType`. | **PASS on the images-route leg.** The 188-byte PNG came back `probe.webp`, `image/webp`, **100 bytes**, 8×8 — the host pixel codec transcoded it, as v4 does. The stored bytes are genuinely WebP (`RIFF`…`WEBP` magic, served `content-type: image/webp` with `content-disposition: inline; filename="probe.webp"` — P4.D114's inline disposition re-proven for free). ⚠ The **chat-upload** leg (P4.72's threading of the codec into chat messages) is a different call site and is proven separately below. |
| B5 | CLAUDE | 💸 **the streaming bubble's avatar on real data** | Watch a multi-character turn mid-flight in the Salon. | The live assistant row shows the responding character's avatar column, in BOTH the waiting-quill and the streaming states; a Flagged chat draws the danger ring on it. | **PASS, both states, with the ring.** Sampled the live row four times during a real multi-character turn: `<img alt="Abigail">` — the **responding** character, resolved, never the `'AI'` fallback — inside `qt-chat-desktop-avatar qt-chat-avatar-dangerous`, i.e. the P4.69 danger ring on the P4.75 column, because that chat is Flagged. Present at **t=4 ms**, in the *waiting* `Sending to Abigail… / Thinking` state, so both `@if` arms carry the column, not only the streaming one. |
| B6 | CLAUDE | the `?action=` refusals through a v4-shaped client | `POST /api/v1/images?action=generate`; an unknown action on a subset edge. | The **named loud refusal** for `generate`; the v4 envelope for an unknown action, and **nothing written**. | **PASS — and the fall-through is the interesting half.** `?action=generate` answers the named loud refusal (`"Generating an image through POST /api/v1/images?action=generate is recognized but not yet available…"`, HTTP 500, v5's own refusal convention). `?action=bogus` is **not** refused: v4's route is `if (action === 'generate') … else handleUploadOrImport`, with no dispatcher envelope, so an unknown action must fall through to upload — and v5 does. Proven with a real multipart body: `?action=bogus` **uploaded** (201, deduped to the same id). The earlier `400 Validation error` for `?action=bogus` with `{}` was the import-from-URL body refusing an empty object, which is the same fall-through. |

## Part C — the standing 💸 queue

| # | Owner | Item | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| C1 | CLAUDE | 💸 **the cheap-LLM priority-5 rung carries the profile's params** | Clear `isCheap` on all 10 profiles; point a chat at the `Qwen3.6-35B` Ollama profile (Max Context 65,536, nine params); trigger a cheap task; tap the Ollama wire. | The priority-5 request carries the profile's `options` (`num_ctx`, `temperature`, `top_k`, …) — pre-fix it carried none. Verified with `wire-tap.py` in front of `localhost:11434`. | **PASS — decisively, on a tapped wire.** Setup: all **10** `isCheap` flags cleared **and** `cheapLLMSettings` emptied (`defaultCheapProfileId` and `userDefinedProfileId` were both set — clearing the flags alone is NOT enough, priorities 1 and 2 sit above the rung), with the `Qwen3.5-9B` Ollama profile behind `wire-tap.py` on 11435. A first pass then showed cheap tasks going to **DEEPSEEK** — that was priority 1/2 still claiming them, and it is why the config had to be cleared too. With priority 5 reachable, `MEMORY_EXTRACTION` and `TITLE_GENERATION` resolved to **OLLAMA / qwen3.5-9b-q6**, and the tapped bodies are the proof: the cheap calls carry the cheap task's own sampling (`temperature 0.3, num_predict 2048, top_p 1`) **beside the profile's own** `num_ctx 40960`, `top_k 33`, `repeat_penalty 1.17` and top-level `keep_alive "7m"` — keys that exist ONLY in the profile's `parameters` bag, planted for this test precisely because the cheap task's own sampling would otherwise mask the carry. Pre-fix, both priority-5 branches set `profileParameters: None`, so none of them could appear. The main chat completions on the same wire carry the profile's `temperature 0.7 / num_predict 32768 / top_p 0.8`, giving the contrast inside one capture. **Every setting was restored afterwards** (10 cheap flags, the config bag, the profile's baseUrl and parameters). |
| C2 | CLAUDE | 💸 **an Ollama uncensored profile's cheap fallback** | Flag a chat; set the uncensored desk to an Ollama profile; run a cheap task. | The **180 s local budget** is taken and the API-key lookup is skipped (`is_local` derived, not hard-coded). Verified in the server log. | **PASS on the pick; the budget itself is not observable without a stall.** With the uncensored text desk pointed at the Ollama profile, a turn in the **Flagged** chat `8f47fb30…` sent its cheap tasks to **OLLAMA / qwen3.5-9b-q6** (four `MEMORY_EXTRACTION` rows) while the main `CHAT_MESSAGE` stayed on the character's own NANOGPT seat — so `resolve_uncensored_cheap_llm_selection` took the configured uncensored profile and it is local. Both uncensored calls appear on the tap carrying the profile's parameters. The 180 s local budget is a **deadline**: it has no wire or log surface unless a call actually stalls past it, so it stays unit-proven (the same honest limit the 2026-08-27 pass recorded for the compression ceiling). The danger settings were restored afterwards. |
| C3 | CLAUDE | 💸 **Pascal's group tier** (deferred five times; recipe written 2026-09-02) | Pre-seed a group state key via `groupStateSet`, then run a custom tool with a group-tier side effect in a chat whose `groupTier.status == "single"`. | The effect commits to the **group** store. | **PASS — the seventh attempt closes it, and the reason it kept failing is the finding.** Setup on real data: **four** chats already resolve `groupTier.status == "single"` (all on *Sebold Family*); `5c4beccd…` was seeded with `groupState.walkGroupProbe`, its chat and project tiers emptied, and a purpose-built `walk_group_probe.tool.json` written into Quilltap General with one effect `state.walkGroupProbe = 7`. **A bare manual run wrote to the CHAT tier** (`tier: "chat"`, no `previous`) even with the key sitting in the group tier — which looks like a broken group leg and is not: the same setup with the key in the **project** tier resolved `tier: "project", previous: 1`, so "write where it lives" was reached and working. The missing ingredient is an **invoking character**: re-run with `asCharacterId`, the effect resolved **`tier: "group", previous: 42, next: 7`**, the group store came back `{"walkGroupProbe": 7}`, and the chat tier stayed `{}`. ⭐ **So an operator's manual run has no group scope and its writes fall to chat — correctly — which is exactly why the obvious gesture could never reach this tier.** Probe tool and all three tiers cleaned up afterwards. |
| C4 | HUMAN | 💸 the Brahma deep-query budget | A deep Brahma query that exercises the raised agent-turn budget. | Expensive by nature (many agent turns) — deferred for cost. | DEFERRED-TO-HUMAN |
| C5 | HUMAN | 💸 memory dedup + conversation-summaries regeneration, first run | Settings → the two maintenance cards, on 31,224 real memories. | A batch job over the whole memory table — deferred for cost. | DEFERRED-TO-HUMAN |
| C6 | HUMAN | 💸 #101 — NanoGPT prompt caching writes but never reads | A cost question about the gateway's own breakpoint placement, not a v5 behaviour. | Needs the human's judgment on whether to keep paying for it. | DEFERRED-TO-HUMAN |
| C7 | — | 💸 the LoRA **wire-byte** look | — | **BLOCKED:** `llm_logs.request` is a pre-builder projection and `wire-tap.py` cannot tap HTTPS; NanoGPT is remote. | BLOCKED(no HTTPS tap) |

---

## Findings

### #110 — the daily maintenance sweep deletes the operator's images in silence (FIXED, `fda5852e`, core 0.0.796)

Found **by consequence, not by a planned step**: the copy's `files` IMAGE count
fell **2,831 → 2,827** between two measurements taken minutes apart during B1,
and nothing anywhere said why. The deleter is `collapse_stale_chat_assets`, the
ported v4 stale-chat asset collapse, run by the daily pass at
`2026-09-05T18:01:03.361Z` (the only trace was `instance_settings.
lastMaintenanceSweepAt`). **The deletion is correct and v4-faithful; the silence
was the port divergence.**

v4 emits eleven lines the port dropped — the pass's two bookends carrying the
whole summary, `runSweep`'s per-sweep `<Sweep> failed — continuing` warn at all
seven arms, `Failed to record lastMaintenanceSweepAt`, and, inside the collapse,
`Collapsed stale chat assets` (chat id / files deleted / bytes) plus
`Stale-chat asset collapse complete`. v5 had **zero** info lines and five of
seven failure arms pushed a summary key without a word — and the collapse's own
comments (`/* v4 warns + continues */`) named the warn they were dropping, which
is finding #103's exact shape.

It also escaped the P4.74 logging inventory, whose generator surveys
`lib/background-jobs/handlers/*.ts` and therefore never looked at
`scheduled-maintenance.ts` or `maintenance/*.ts` — recorded as a standing note.

Fixed with v4's sentences at v4's levels, four capture-layer tests and six
mutations. Behaviour unmoved: `maintenance_sweep_tier2_equivalence` green
against an oracle regenerated fresh at the baseline. Gate: **496 test binaries /
2,806 passed / 0 failed**, zero SKIP; clippy both feature sets; release build.

### No other v5 defect was found

Everything else on the walk either passed or was traced to an instrument error
(below) or to a v4-faithful behaviour, each measured rather than assumed.

## Instrument notes

The standing rule — **prove the instrument before trusting a negative** — earned
its keep five times today.

1. **A gate piped through `tail -60` reported 58 binaries / 146 tests** for a
   run that actually covers 496 / 2,806. That is CLAUDE.md's own named mistake,
   made anyway; the re-run captured the full log.
2. **A nonce shared with the model is not an attachment probe (A3).** The first
   responder *quoted* the control token, so every later request contained it.
   Only a needle from the file **body** that no reply repeats can discriminate
   re-hydration from repetition.
3. **The obvious bug-121 gesture lands on the authoring seat (A3).** 12,388 of
   12,607 user messages on this instance (98.3%) carry a `participantId`, so
   "attach, then nudge the other character" usually nudges the seat that *wrote*
   the message — whose own line correctly stops the walk. Two attempts read as
   misses before a three-seat chat gave a responder who neither authored nor had
   seen it.
4. **A wrong field name reads exactly like a missing file.** `documentDelete`
   takes `filePath` / `scope` / `mountPoint`; sending `mountPointId` yields
   `File not found` for a file that `GET` returns 200 for, and the file survives.
   With the correct bag both deletes succeeded. A6(a) was re-run with a correct
   bag so its 404 is unambiguous.
5. **`wire-tap.py` truncates bodies (~8 KB)**, so `messages` came back as a
   string and a naive `.get()` walk crashed; read `options` and top-level keys by
   regex instead of parsing the whole body.

Also worth carrying: **clearing every `isCheap` flag does not make the cheap
ladder fall to priority 5** — `cheapLLMSettings.defaultCheapProfileId` and
`.userDefinedProfileId` sit above it and must be cleared too (C1); and **a
manual custom-tool run has no invoking character, so it can never reach the
group tier** (C3).
