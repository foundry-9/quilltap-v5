# Dogfood walk — 2026-10-05 — the `52d6e7ecd` standing-informs round + the `07b8f0209` voice-mode/PDF round

**Rounds walked:** P4.D249 ∥ P4.D250 ∥ P4.146 (unified 2026-10-04) and
P4.D251 ∥ P4.D252 ∥ P4.D253 (unified 2026-10-05). Agent-driven, on a COPY of
Friday (`~/qt-dogfood-friday`, rsynced 2026-10-05 ~14:12; DB mtimes
11:49–12:23).

**Build:** main `c0dcf9f6b` (core 0.0.1208, host 0.0.181, web 0.0.219,
SPA 0.5.805), release build + `npm run build` (the human's).

**Server:**

```
RUST_BACKTRACE=1 RUST_LOG='info,quilltap::=debug,quilltap_core=debug' \
  ./target/release/quilltap-web --data-dir ~/qt-dogfood-friday \
  --spa-dir apps/web/dist/quilltap/browser
```

## §1 Drift state at walk start

The ledger's §2 probe PASSED: v4 on `main` at `07b8f0209`, tree clean,
`07b8f0209..main` and `1a2b2164c..bugfix` empty. §3 EMPTY — no step can blame
drift.

## §2 Pre-walk measurements (read-only, release CLI, before the first boot)

| # | measured | result | consequence |
|---|---|---|---|
| M1 | `chat_settings` voice columns + ledger | ONLY `impersonationVoiceMode` (= `'ask'`, 1 row); `impersonationVoiceRewrite` ABSENT; `migrations_state` has `impersonation-voice-mode-v1` completed **2026-10-05T04:23:58Z** | **v4 already migrated the live instance** (ledger §5.5). The banked "first v5 boot translates `1` → `'ask'`" proof is GONE from this copy; the row becomes the order's other arm — "on a copy v4 already migrated, an exact no-op" (A2) |
| M2 | `chat_informs` columns + population | `permanent` PRESENT; `add-chat-informs-permanent-v1` completed 2026-10-04T02:31Z; 35 rows, all `permanent = 0`, all consumed | **v4 already ALTERed it** — the "first boot appends the column" proof is gone too; A1 becomes the no-op arm. No standing informs exist — C creates the first |
| M3 | PDFs in the instance | no `files` row with `application/pdf`; no `.pdf` in the mount index (blob/json/jsonl/markdown/txt only) | E uses PDFs generated in the scratchpad (a text-bearing one and an image-only one) — no personal documents |
| M4 | backups on the copy | none (`~/qt-dogfood-friday` has `data/ files/ logs/` only) | F takes its own backup; the pre-round "old key" backup is a PLANT (the new key edited back to `impersonationVoiceRewrite: true`) |
| M5 | the Lantern (walk rule since 2026-10-02) | `storyBackgroundsSettings.enabled: true` | turned OFF on the copy (`chatSettingsUpdate`) before any test chat |

## §3 The walk

Statuses: `PENDING` → `PASS` / `FAIL(#n)` / `DEFERRED-TO-HUMAN` / `BLOCKED(reason)`.

### A — Boot ensures on a v4-migrated copy (P4.D249, P4.D251)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| A1 | CLAUDE | first boot; `sqlite_master.sql` for `chat_informs` + `chat_settings` md5'd before and after | both ensures no-op: no ALTER, no translation, no DROP; both `sql` strings byte-identical; `permanent` + mode unmoved | **PASS** — `chat_informs` sql md5 `7ac64545…` and `chat_settings` `22a1b40d…` identical before and after; mode `'ask'`, 35 rows `permanent = 0`; no ensure line in the boot log; `/health` 200 `healthy` with `structure: healthy` |
| A2 | CLAUDE | second boot (restart) | same md5s; no ensure line claiming work | **PASS** — both md5s unchanged after the restart; the boot log carries no voice/inform/ALTER/WARN line |

### B — The impersonated-line voice mode (P4.D251 server, P4.D252 SPA)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| B1 | CLAUDE | Settings → Chat → Composer card | three radios, v4's labels + descriptions; `Ask each time` checked (the migrated `'ask'`) | **PASS** — the card's "Impersonated lines in the character's own words" fieldset: `Never` / `Ask each time` / `Always restate`, each description byte-identical to v4 `components/settings/chat-settings/ImpersonationVoiceSettings.tsx:11-30` at the pin; radio `ask` checked (DOM `input[value=ask].checked`) |
| B2 | CLAUDE | the Almanack's feature line | reads `Ask each time` | **PASS** — compiled through the card ("Compile the Almanack", 43,672 B): `- **Impersonated Lines in Character Voice**: Ask each time` (v4 `render.ts:745`'s line); `Last Migration: impersonation-voice-mode-v1` (v4's, from the live instance) |
| B3 | CLAUDE | `chatSettingsUpdate` (dispatch) and the REST PUT with `impersonationVoiceMode` = `null` / `true` / `'on'`; then a body carrying ONLY the retired `impersonationVoiceRewrite: true` | each refused with the enum sentence, mode unmoved in DB; the retired-key body 200 with the mode unchanged | **PASS** (dispatch) — `null` / `true` / `"on"` each → `bad-request` `Invalid impersonationVoiceMode value (must be one of off, ask, always)` (v4 `route.ts:239`'s sentence); `{impersonationVoiceRewrite: true}` → 200 `chatSettings`, mode `ask`, the retired key absent from the reply; DB `'ask'` throughout. **The REST edge is not a v5 route** (`PUT /api/v1/settings/chat` → 405; chat settings are dispatch-only in v5's router — the order's "both transports" is proven at the handler level by `chat_settings_composer_web_routes` + `settings_routes_equivalence`) |
| B4 | CLAUDE | a two-LLM-character test chat; impersonate one seat; hover the portrait / Send button | the `ask` cue (`voiceRehearsalTitle`) on both | **PASS** — test chat `a5627f60…` (Charlie user seat; Friday + Laura on Z.AI GLM 5.3 Flash; Laura's greeting stamped `2:19 PM`), "Speak as Friday" → toast `Now speaking as Friday`; the portrait `title` = `Speaking as Friday — your draft opens for review; send it as written or have Friday restate it`, the Send button's = `Opens your draft for review — send it as written or have Friday restate it` — both v4 `SpeakingAsAvatar.tsx:37-38` byte-for-byte |
| B5 | CLAUDE | type a line as the impersonated seat, press Send | the review dialog opens on the DRAFT; **zero** new `VOICE_REWRITE` rows (`quilltap db --llm-logs`) | **PASS** — "In Friday's own words", `Would be spoken through Z.AI GLM 5.3 Flash — glm-5.3-flash`, the draft verbatim, the hint `Sending as written posts your words under Friday's name exactly as typed (Cmd/Ctrl+Enter). …`, footer Cancel / Edit original / Restate in their voice / Send as written (v4 `ImpersonationVoiceDialog.tsx:290-326`'s draft arm); `VOICE_REWRITE` count 7 before and after, newest still `04:46Z` (v4's, this morning) |
| B6 | CLAUDE | Restate in their voice | exactly ONE `VOICE_REWRITE` row; a proposal shown | **PASS** — one row `692c13a7…` `Z_AI glm-5.3-flash` at 19:21:09Z, nothing else logged; the proposal under "What Friday will say"; the footer moved to v4's review arm (Cancel / Edit original / Send as written / Regenerate / Send) |
| B7 | CLAUDE | change the picker after the proposal | proposal dropped; NO new row (waits) | **PASS** — the System-prompt picker → `Friday as Executive Assistant`: the proposal gone, the dialog back on the draft footer (Restate in their voice / Send as written); llm_logs still the one row after 6 s |
| B8 | CLAUDE | Cmd/Ctrl+Enter in the draft | the draft posted VERBATIM as one USER message (DB) | **PASS** — focus in the draft editor, Cmd+Enter: one `USER` row at 19:21:41Z under Friday's participant `7d39bf75…`, content exactly `I sit down and take the pencil. Fourteen across is HYPOTENUSE, love.`; no new `VOICE_REWRITE` |
| B9 | CLAUDE | radio → `Always restate`; the immediate DOM after the click | the new radio stays checked (no flicker back); the next impersonated send opens the dialog with ONE eager `VOICE_REWRITE` row | **PASS** — a 20 ms poll over the three radios for 3 s around a real click on `Always restate`: ONE transition (`ask` → `always` at +45 ms), never back; DB `'always'`. The Salon's cue flipped to v4's `always` wording (`Speaking as Friday — your draft goes to Friday to say in their own words first` / `Sends your draft to Friday to say in their own words first`, `SpeakingAsAvatar.tsx:31-33`); Send opened the dialog straight into `Generating in character…` / `Rehearsing…` and ONE `VOICE_REWRITE` row landed (19:23:35Z); Cancel posted nothing |
| B10 | CLAUDE | radio → `Off`; an impersonated send | no dialog; the line posts directly; no row. Restore `Ask each time` afterwards | **PASS** — `off` saved; the Salon's cue gone (`Speaking as Friday` / `Send message`); Send posted `Coffee first, then twenty-two down.` as a USER row under Friday's seat (19:24:10Z) with no dialog and no new llm_logs row; `Ask each time` restored through the radio (DB `'ask'`). (A raw `fetch` to dispatch does not reach the Salon's cached settings — the cue held until the Settings tab refetched; that is the instrument bypassing the SPA's query cache, not a finding) |

### C — Standing informs (P4.D249 server, P4.D250 SPA)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| C1 | CLAUDE | open Inform in a test chat | the "Keep it standing in this chat" checkbox; ticking it flips the guidance tail | **PASS** — "Inform the cast" (composer button) shows the box `Keep it standing in this chat` / `Every turn they take here, until you withdraw it. This chat only — it follows no one anywhere else.`; ticking it (a real click on the label) moves the guidance tail from `…once they have had their turn it is gone, like a note fed to the fire.` to `…it stays at their elbow for every turn they take in this chat, until you withdraw it.` — v4 `InformDialog.tsx:264-265,297-299` |
| C2 | CLAUDE | post a standing inform to a subset | the standing toast naming the subset; the chip's standing label + hover; DB row `permanent = 1` | **PASS** — Laura alone (Everyone/Friday unpressed), "You notice the porch railing has a loose board near the steps, and it creaks whenever anyone leans on it.": toast `A standing note for Laura, for the rest of this chat`; chip `Informing Laura on every turn in this chat`, hover `Standing in this chat until withdrawn — You notice the porch railing…`, its × titled `Withdraw this standing inform` (v4 `InformDialog.tsx:165`, `PendingInformChips.tsx:102,112,123`); DB `01b2bfa9…` `permanent = 1`, `consumedAt` NULL, record message `85c97d29…` |
| C3 | CLAUDE | two consecutive real turns | the inform in both turns' request (`llm_logs` request body), first in the block; `consumedAt` stamped once and UNMOVED on turn two | **PASS** — turn 1 (Charlie addressed Laura): Laura's reply `7a17eb65…` ("*I shift off the railing by the steps, guiltily.*…"); its `CHAT_MESSAGE` request carries the passage as its own system message right after the Identity Reminder; `consumedAt` = 19:25:45.684Z, `consumedByMessageId` = `7a17eb65…`. Friday's next turn (`a3ae6c99…`) — NOT a recipient — has no "loose board" in her request (the subset holds). Turn 2 (Nudge Laura): `0b0d2bbf…`, request carries the passage again at the same position (index 2 of 25); `consumedAt`/`consumedByMessageId` UNMOVED |
| C4 | CLAUDE | swipe/regenerate the last reply | the swipe's request carries the inform too | **PASS** — Regenerate on Laura's `0b0d2bbf…` → swipe `43059fc5…` (`swipeIndex 1`, "…hold it up like evidence. Caught. Guilty."); the swipe's build logged `[Inform] Built inform block … participant_id="e27226f4…" pending=0 reapplied=0 standing=1 passages=1`; `consumedAt` unmoved. **No `llm_logs` row for the swipe** — v4's `regenerate-swipe.service.ts` streams through `provider.streamMessage` outside the `logLLMCall` funnel, so v4 writes none either → #138 (v4-faithful), and the log line is this row's proof |
| C5 | CLAUDE | withdraw via the chip (`Withdraw this standing inform`) | `removed` = every row of the batch; chip gone; DB empty for the batch | **PASS** (run on C7's re-imported chat `e46086cc…`, after C6/C7) — the chip's × (`Withdraw this standing inform`; clicked by script — the pane was hidden): `[Chats v1] Inform cancelled … removed=1 any_consumed=true permanent=true record_deleted=false` (the batch's one row; the consumed inform keeps its Host record), the chip gone, `chat_informs` for the chat 0 |
| C6 | CLAUDE | `informCreate` with `permanent: null` on dispatch + REST | refused on both, no row | **PASS** — dispatch `chatInform {permanent: null}` → 400 `Validation error` with `details: [{expected: "boolean", code: "invalid_type", path: ["permanent"], message: "Invalid input: expected boolean, received null"}]`; REST `POST /api/v1/chats/{id}?action=inform` the same body → the same 400; the chat's inform count unmoved at 1 |
| C7 | CLAUDE | `.qtap` export of the chat with a standing inform, re-import | the imported chat's inform row keeps `permanent = 1` | **PASS** — `POST /api/v1/system/tools?action=export` (`selected`, the chat) → 141,715 B NDJSON; the `chat_inform` record carries `"permanent": true`. The original chat deleted, then `import-execute` (multipart, `skip`) → `chats 1, messages 30, chatInforms 1`, no warnings; the new chat `e46086cc…` holds the row with `permanent = 1`, `consumedAt` 19:25:45.684Z and the record pointer intact. **First attempt (`duplicate`, the original still present) lost every message and the inform** — v4's duplicate arm does the same (`import-entities.ts`: `idMaps.chats.set(chat.id, randomUUID())` but `create()` mints its own id, and each message re-inserts under its old id) → #139 (v4-faithful); its warnings also carried a v5-only `sqlite error: ` prefix → #140 |

### D — `properties.json` null preservation (P4.146, dogfood #136)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| D1 | CLAUDE | LUC Ranch: toggle Allow Any Character through the project page | the file's bytes before/after differ in ONE key; `projectList` row carries `color`/`icon`/`storyBackgroundsEnabled`/`staticBackgroundImageId` as `null` | **PASS** — LUC Ranch opened from Home's project row (`/prospero/bf90dc1f…`), the "Allow Any Character" switch flipped (script click — pane hidden; the note moved to `Every character may use the project files and shared wardrobe.`). `properties.json` (`?raw=1` on the official mount) 598 → 597 B, `diff` = ONE line (`"allowAnyCharacter": false` → `true`), its eight explicit `null`s intact; `projectList` row `color`/`icon`/`storyBackgroundsEnabled`/`staticBackgroundImageId` all `null` (present, not absent) |
| D2 | CLAUDE | create a project with no colour | file has `"color": null, "icon": null`; home dashboard row carries `color: null` | **PASS** — Home → New Project → "Create Project" with only a name (`DOGFOOD 2026-10-05 no colour`) → `f8712eab…`; its `properties.json` reads `"color": null, "icon": null` beside `allowAnyCharacter: true` and the empty roster; `systemHome`'s row `{…"color": null, "icon": null, "chatCount": 0…}` |
| D3 | CLAUDE | group editor: clear a group's colour, save | `"color": null` in the group's file | **PASS** — Constellation's editor (`/characters/groups/d07a2ade…`), the hex field `#001eff` cleared, Save Changes: the file went from `{"color": "#001eff", "icon": "🧭"}` to `{"color": null, "icon": "🧭"}`; `groupList` carries `color: null` (present) |
| D4 | CLAUDE | `.qtap` import of a group with `"color": 5` | refused `Failed to import group` | **PASS** — Celestial Engineering Team exported (`type: groups`, selected), the record re-keyed to a fresh id, renamed `DOGFOOD color five`, `color: 5`, its mount pointer dropped; `import-execute` (`skip`) → 200 with warning `Failed to import group "DOGFOOD color five": invalid type: integer \`5\`, expected a string` and WARN `Failed to import group group_id=0164c3f7…`; no `DOGFOOD%` group row. The tail after the colon is serde's text where v4's `repos.groups.create` throws a ZodError message → folded into #140 |

### E — PDF extraction through the converter seam (P4.D253, bug 177)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| E1 | CLAUDE | the AI Wizard from a text-bearing PDF (uploaded) | the seam's stderr refusal once; WARN `pdf-parse found no text, using native fallback extraction` with `size`; DEBUG `Extracted PDF content` with `size`/`chars`; the PDF's text in the wizard's request | **PASS** — `mirela-voss-dossier.pdf` (862 B, three uncompressed `Tj` lines, generated in the scratchpad) uploaded via `POST /api/v1/files` (`DOCUMENT`); `characterWizard` (`sourceType: document`, GLM 5.3 Flash, title + description; driven over dispatch — the pane was hidden): stderr `DocumentTextExtractor unavailable — refusing pdf text extraction …` once, WARN `pdf-parse found no text, using native fallback extraction size=862`, DEBUG `Extracted PDF content size=862 chars=555`; both `CHARACTER_WIZARD` requests carry `Character Reference Document:\nCaptain Mirela Voss is a retired airship navigator…`; the answer `Navigator of Kestrel Reach` with the dossier's tea and star charts |
| E2 | CLAUDE | the same with an image-only PDF | `Failed to extract PDF content (no text found)` surfaced; no LLM call | **PASS** — `scanned-page.pdf` (846 B, an 8×8 image XObject, a 17-char content stream): the refusal + WARN `… size=846`, then ERROR `[Characters v1] AI Wizard failed error=Failed to extract PDF content (no text found)`; `llm_logs` 7,639 → 7,639. The dispatch answered a generic 500 `Internal server error` — v4's `handleAiWizard` has no catch either (the throw reaches the route wrapper's 500); the SPA's streaming twin carries the sentence in its frame |
| E3 | CLAUDE | Summon From Lore (AI import) with the text PDF | `=== Source File: <name>.pdf ===` in the analysis request | **PASS** — `aiImportStream` (the dossier, memories/chats off, GLM 5.3 Flash): the refusal + WARN `size=862` + DEBUG `chars=555` at the source-context build, six `AI_IMPORT` calls, a full Mirela Voss result (navigator, peacoat, brass pocket barometer). The logged request summarizes the user message (`[source context + instruction - …]`, 116 chars), so the `=== Source File` header itself is not visible in `llm_logs`; the result is the proof the text arrived. The first-message step failed non-fatally with serde's `trailing comma at line 4 column 1` — the RECORDED divergence of `optimizer::v8_json_parse_message` (failures inside a legally-started value fall back to serde), firing live |

### F — Backup / restore carriers (last — a restore rewrites the copy)

| # | owner | gesture | expected + how verified | status |
|---|---|---|---|---|
| F1 | CLAUDE | backup with a standing inform present; restore | the inform's `permanent = 1` survives | **PASS** — a standing inform posted to Laura in `e46086cc…` (`3a28fc42…`, "…the kettle on the porch stove whistles a half-step flat."), the mode set `off`; `systemBackupCreate {compact: true}` (41 s; 55 characters / 1,048 chats / 96,589 messages / 36 informs) → the 1.25 GB archive downloaded (`chat-informs.json` carries `permanent: true`); restored (`replace`, the planted copy — F2) through the upload leg (`-T`; `--data-binary` runs curl out of memory) → `chatInforms: 36`, and `3a28fc42…` is back with `permanent = 1`, `consumedAt` NULL, the only standing row of 36. **The first restore was cut off** at 19:46 when the server's 30-minute background limit killed it (an instrument error); the server was relaunched with the 2-hour limit and the restore re-run from a fresh upload (~8 min). That restore surfaced #141 and #142 |
| F2 | CLAUDE | restore a backup whose chat settings carry the RETIRED key `impersonationVoiceRewrite: true` (a plant — M4) | mode lands `'ask'` with the translation debug line | **PASS** — the archive's `chat-settings.json` rewritten in place so its `impersonationVoiceMode: "off"` became `impersonationVoiceRewrite: true` (a pre-`07b8f0209` shape), `zip`-updated into a clone; the restore logged DEBUG `quilltap::restore: Translated the retired impersonated-line voice toggle for restore settings_id=00506655… impersonation_voice_mode=ask`, and `chat_settings.impersonationVoiceMode` = `'ask'` — distinct from both the archive's real `off` and the pre-restore `off` |

## §4 What NOT to expect

- **Real PDF text extraction.** v5's `DocumentTextExtractor` is the refusing
  default (P4.6y deferred); a text-bearing PDF reaches the regex fallback, so
  a compressed-stream PDF yields little or nothing where v4's pdf-parse reads
  it. That is the recorded divergence, not a finding. (The stderr line's
  "bookkept as extraction-failed" wording is false at this caller — already
  OPEN for the smalls round.)
- **v5 writes no `migrations_state` rows** (the deferred runner).
- The first-boot translation / ALTER arms (M1, M2) cannot run on this copy.
- WaveSpeed (ruled unsupported).

## §5 Findings

**Zero v5 defects in the two rounds' surfaces.** Five rows filed in
`dogfood-findings.md` — the two most serious OUTSIDE the rounds, surfaced by
F's restore:

- **#141 — v4 shares it; ruling FIX v5; ORDER-PENDING (high):** a `replace`
  restore orphans every character's vault — phase 6 provisions a fresh vault
  (12 files for Friday) and the archive's real one (805 links: photos,
  wardrobe, mail, summaries) lands beside it unreferenced; projects/groups
  the same. 144 stores from a 77-store archive, 41 duplicate names.
- **#142 — v4 shares it; ruling FIX v5; ORDER-PENDING:** the files phase
  resolves the Uploads store through the TARGET's pointer, restored only at
  22o; when it differs from the archive's (any fresh instance — here, a reboot
  between two restores re-minted it) every project-less file fails
  `Quilltap Uploads mount has not been provisioned` (11 on the copy).

- **#138 — v4-faithful (candidate v4 note):** a regenerate (swipe) writes no
  `llm_logs` row — v4's `regenerate-swipe.service.ts` streams outside the
  `logLLMCall` funnel.
- **#139 — v4-faithful (candidate v4 filing):** a `duplicate` `.qtap` import
  of a chat still present loses every message (re-inserted under their old
  ids) and every inform (the id map points at a phantom chat id). v5
  reproduces v4's quirk on purpose (`quilltap_import/entities.rs:13-17`).
- **#140 — port divergence (wire text), ORDER-PENDING:** import warnings carry
  v5's error tail (`sqlite error: …`, serde's `invalid type …`) where v4 has the
  bare SQLite message / the ZodError text. A standing note for the next smalls
  round.

Instrument notes (not findings): **the dogfood server must be launched with
the 2-hour background limit** (`timeout: 7200000`) — the default 30 minutes
killed it mid-restore at 19:46, leaving a half-restored copy (recovered by a
second `replace` restore); the Browser pane went hidden mid-walk, so
B9's radio and the C/D gestures from C5 onward were script-dispatched clicks
on the real controls (a hidden pane composites no frames, so coordinate and
ref clicks fail); a raw `fetch` to dispatch does not refresh the SPA's cached
chat settings (B10).
