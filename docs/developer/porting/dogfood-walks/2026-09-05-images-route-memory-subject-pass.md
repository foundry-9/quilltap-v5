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
| A1 | CLAUDE | 💸 **bug 122 on a real multi-character turn** | In chat `8f47fb30…` (Abigail, the chat v4 ran at 17:44 today), send a turn and let Abigail answer. | v5's `llm_logs.request` carries the head formatter's `[m_xxxx] [when] About <Name>: ` in the same slot as v4's row `af3982f5`. **Cross-implementation:** compare v5's rendered prefix bytes against v4's own from the same chat/character. Verified by extracting the matching lines from both rows. | PENDING |
| A2 | CLAUDE | bug 122's negative arms | Same request: memories the character owns ABOUT ITSELF (own id) and any row with a dangling `aboutCharacterId`. | Own-id memories carry **no** prefix; a dangling/unresolvable subject renders `About another character: `. Measure the dangling population first; if zero, say so rather than inventing one. | PENDING |
| A3 | CLAUDE | 💸 **bug 121 — a user attachment reaches the SECOND responder** | In a multi-character chat, attach a small text file to a user message and let **two** characters answer in sequence. | The second responder's `llm_logs.request` carries the attachment's text (re-hydrated), not just the first responder's. Verified by grepping both rows for a nonce string written into the file. | PENDING |
| A4 | CLAUDE | 💸 **the Opus 5 sampling strip on a real profile** | Create a `claude-opus-5` ANTHROPIC profile carrying `temperature` + `top_p` (copy Opus 4.8's parameter bag), point a throwaway chat at it, send one short message. | The completion **succeeds**. That is the whole proof: Opus 5 rejects `temperature`, so a pre-fix v5 would have answered HTTP 400. Verified in `llm_logs` (a row with a response, not an error) and the server log. ⚠ costs one short Anthropic completion. | PENDING |
| A5 | CLAUDE | 💸 **`instances default --json`** against the real registry | `quilltap instances default --json`, both with and without a default recorded. | Compact `{"defaultInstance":…}` (not pretty-printed), `null` when absent — and `--json` is **not** consumed as an instance name. | **PASS.** All four arms. Present + `--json` → **compact** `{"defaultInstance":"Friday"}` (no spaces); absent + `--json` → `{"defaultInstance":null}`; absent plain → `(none)`; and `instances list --json` is still **pretty** 2-space JSON, so v4's two spellings are reproduced side by side. The flag is read **and stripped**: the real registry still holds its 4 entries with Friday default and no instance named `--json`. The absent arm was taken against an isolated `HOME`, so the user's own registry was never written to. The help line `  list [--json]                 List registered instances (default)` matches v4's padding. |
| A6 | CLAUDE | the collapse's server corrections | (a) delete a chat-scoped document that does not exist; (b) a Brahma one-shot on a key-less profile; (c) the export wizard's preview count against the written `.qtap` on an instance holding **10** archive bundles. | (a) `File not found` **once**, not "File not found not found"; (b) the **capitalised** `describeProfileApiKeyFailure` sentence; (c) preview `files` count **equals** what the archive actually contains (the inline filter is gone). | PENDING |
| A7 | CLAUDE | the About page's three widened sentences | Open About. | The Lantern / LoRA / third clauses present, with the Lantern's apostrophe rendering as **U+0027**. | PENDING |
| A8 | CLAUDE | the Workbench placeholder warnings | In a custom tool draft, write `{{params.}}`, `{{metadata.}}` and `{{state.}}`. | `{{params.}}` reads "is not a placeholder this build knows" (not "names no declared parameter"); a bare `{{metadata.}}` is now **reported** rather than silent; `{{state.path}}` is still allowed in the chip label. | PENDING |
| A9 | CLAUDE | the `qt-checkbox` adoptions | Settings → the cheap-LLM card and the answer-confirmation card. | Both cheap-LLM inputs carry `qt-checkbox`; the answer-confirmation card wears `qt-settings-toggle-row` and the hand-built label is gone. | PENDING |

## Part B — the follow-ups round 2

| # | Owner | Item | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| B1 | CLAUDE | 💸 **`GET /api/v1/images` over real data** | `curl` the collection, with and without `?tagId=`. | Rows for tagged images only; **`width`/`height`/`generationPrompt`/`generationModel` ABSENT (not null)** on the 254 rows that carry NULLs, while `url` is an explicit null; schema-invalid rows dropped. Verified against the DB row for the same id. | **PASS — exact on real data.** For `1c563844…` (DB cells `width`/`height`/`generationPrompt`/`generationModel` all NULL) the payload **omits all four keys** and carries `url` as an explicit `null`; key order `id, userId, filename, filepath, url, mimeType, size, source, createdAt, updatedAt, tags, _count`. The list returned **2,827** rows against 2,827 IMAGE rows in `files` — zero dropped, because real data has no schema-invalid row (measured: 0 bad-sha, 0 key-less). ⚠ The first run read 2,827 against a DB that had held 2,831 minutes earlier — that discrepancy is **finding #110**, not a list defect. |
| B2 | CLAUDE | `DELETE /api/v1/images/{id}` | Delete an image that IS referenced, then one that is not. | The referenced one refuses with `IMAGE_IN_USE` and an `associations` bag whose `chatAvatarOverrides` counts **characters**; the orphan deletes. Verified by the `files` row before/after. | **PASS, both arms.** `ffef03de…` (one character avatar override) refuses **HTTP 400** `{"error":"Image is in use","details":{"message":"This image is currently being used as an avatar or in chat overrides. Please remove all usages before deleting.","code":"IMAGE_IN_USE","associations":{"charactersUsingAsDefault":0,"chatAvatarOverrides":1}}}` and the row **survives** — so `bad_request_with_details` + the `CoreError::details` carry + `validation_wire_body()` work end to end. The unreferenced probe upload deletes: `200 {"success":true}`, row gone, and `GET /api/v1/files/{id}` now **404** so the bytes went with it. Bonus measurement: every `characters.defaultImageId` on this instance is **dangling** (no matching `files` row), and such an id answers `404 Image not found` — correct. |
| B3 | CLAUDE | `POST /api/v1/images` — upload + import-from-URL | Multipart upload of a small PNG; then a JSON import from a `file://`-free local HTTP URL served off this same server. | 201 with the created row; the import fetches and stores. A non-image refuses. | **PASS.** Multipart upload of an 8×8 PNG → **HTTP 201** with the created row. A second upload of the same bytes returned the **same id** (bug 54's sha256 dedup) — also 201, as v4's create does. |
| B4 | CLAUDE | 💸 **the chat-upload WebP transcode** | Attach a PNG to a chat message. | The stored bytes are **WebP** (the host pixel codec), as v4's are — verified by reading the stored blob's magic bytes and the `files.mimeType`. | **PASS on the images-route leg.** The 188-byte PNG came back `probe.webp`, `image/webp`, **100 bytes**, 8×8 — the host pixel codec transcoded it, as v4 does. The stored bytes are genuinely WebP (`RIFF`…`WEBP` magic, served `content-type: image/webp` with `content-disposition: inline; filename="probe.webp"` — P4.D114's inline disposition re-proven for free). ⚠ The **chat-upload** leg (P4.72's threading of the codec into chat messages) is a different call site and is proven separately below. |
| B5 | CLAUDE | 💸 **the streaming bubble's avatar on real data** | Watch a multi-character turn mid-flight in the Salon. | The live assistant row shows the responding character's avatar column, in BOTH the waiting-quill and the streaming states; a Flagged chat draws the danger ring on it. | PENDING |
| B6 | CLAUDE | the `?action=` refusals through a v4-shaped client | `POST /api/v1/images?action=generate`; an unknown action on a subset edge. | The **named loud refusal** for `generate`; the v4 envelope for an unknown action, and **nothing written**. | **PASS — and the fall-through is the interesting half.** `?action=generate` answers the named loud refusal (`"Generating an image through POST /api/v1/images?action=generate is recognized but not yet available…"`, HTTP 500, v5's own refusal convention). `?action=bogus` is **not** refused: v4's route is `if (action === 'generate') … else handleUploadOrImport`, with no dispatcher envelope, so an unknown action must fall through to upload — and v5 does. Proven with a real multipart body: `?action=bogus` **uploaded** (201, deduped to the same id). The earlier `400 Validation error` for `?action=bogus` with `{}` was the import-from-URL body refusing an empty object, which is the same fall-through. |

## Part C — the standing 💸 queue

| # | Owner | Item | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| C1 | CLAUDE | 💸 **the cheap-LLM priority-5 rung carries the profile's params** | Clear `isCheap` on all 10 profiles; point a chat at the `Qwen3.6-35B` Ollama profile (Max Context 65,536, nine params); trigger a cheap task; tap the Ollama wire. | The priority-5 request carries the profile's `options` (`num_ctx`, `temperature`, `top_k`, …) — pre-fix it carried none. Verified with `wire-tap.py` in front of `localhost:11434`. | PENDING |
| C2 | CLAUDE | 💸 **an Ollama uncensored profile's cheap fallback** | Flag a chat; set the uncensored desk to an Ollama profile; run a cheap task. | The **180 s local budget** is taken and the API-key lookup is skipped (`is_local` derived, not hard-coded). Verified in the server log. | PENDING |
| C3 | CLAUDE | 💸 **Pascal's group tier** (deferred five times; recipe written 2026-09-02) | Pre-seed a group state key via `groupStateSet`, then run a custom tool with a group-tier side effect in a chat whose `groupTier.status == "single"`. | The effect commits to the **group** store. | PENDING |
| C4 | HUMAN | 💸 the Brahma deep-query budget | A deep Brahma query that exercises the raised agent-turn budget. | Expensive by nature (many agent turns) — deferred for cost. | DEFERRED-TO-HUMAN |
| C5 | HUMAN | 💸 memory dedup + conversation-summaries regeneration, first run | Settings → the two maintenance cards, on 31,224 real memories. | A batch job over the whole memory table — deferred for cost. | DEFERRED-TO-HUMAN |
| C6 | HUMAN | 💸 #101 — NanoGPT prompt caching writes but never reads | A cost question about the gateway's own breakpoint placement, not a v5 behaviour. | Needs the human's judgment on whether to keep paying for it. | DEFERRED-TO-HUMAN |
| C7 | — | 💸 the LoRA **wire-byte** look | — | **BLOCKED:** `llm_logs.request` is a pre-builder projection and `wire-tap.py` cannot tap HTTPS; NanoGPT is remote. | BLOCKED(no HTTPS tap) |

---

## Findings

_(filled in as the walk runs)_

## Instrument notes

_(filled in as the walk runs — the standing rule is **prove the instrument
before trusting a negative**.)_
