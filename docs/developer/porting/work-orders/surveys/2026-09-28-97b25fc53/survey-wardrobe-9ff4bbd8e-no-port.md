# Survey — v4 `9ff4bbd8e` "Offer an outfit when importing wardrobe items from an image" (2026-09-28)

v4 `~/source/quilltap-server` at `97b25fc53` (clean); baseline `acadcc7cd`; commit is `4.10.0-dev.97`, 12 files.
Read from hunks + post-commit files only.

## HEADLINE (read this first)

**The ledger's classification `PORT (small)` is wrong: v5 has no wardrobe image-analysis port at all.**
`wardrobeAnalyzeImage` is a REFUSAL ARM (`crates/quilltap-core/src/api/wardrobe.rs:1191-1205`,
`ErrorKind::Internal`, "wardrobeAnalyzeImage is not available in this build … a P4.9f1 tier-3 deferral"),
and the SPA deliberately ships NO "Import from image" button (`apps/web/src/app/wardrobe/wardrobe-control-dialog.ts:105-107`
and `:383-384`). No v5 file holds the vision prompt bytes, `parseAnalysisResponse`, or an import-from-image
modal. There is no harness family and no oracle case for image analysis. This is a named deferral recorded in
`work-orders/p4.9f1-wardrobe-server.md:40,94,387`, `p4.9f2-wardrobe-dialog-spa.md:24,361`, `m6-screen-parity.md:541`,
the `status-log.md` M6 table (`:52444` "UNPORTED … a named v5 deferral"), and the P4.D79 NO-PORT note
(`status-log.md:3290,3563`). It has **precedent for exactly this situation**: the hair-slot round
(`status-log.md:76303,76352`) skipped v4's `image-analysis` prompt edits as "surface unported", banked as riders.

So the proper verdict is:
- **`lib/wardrobe/image-analysis.ts`, the route, the modal, and the three tests → NO-PORT (surface unported)**.
  Bank them as riders on the future image-analysis port, the same way the hair edits were banked.
- **`help/wardrobe.md` → PORT (a byte-copy).** v5 vendors `help/wardrobe.md`, which is byte-identical to the
  `acadcc7cd` copy and differs from `9ff4bbd8e` by exactly the +14 lines. `help_tree_equivalence` goes RED at a
  post-`9ff4bbd8e` pin until it is re-vendored. The file count stays at 129 because no file is added.
- README/CHANGELOG/package.json version stamps → NO-PORT.

If the human instead wants the whole import-from-image vertical ported (un-refusing an LLM-backed verb), that is
a new vertical of M–L size, not a drift row (§E). Sections A–D below are written so that future lane can use them.

---

## A. v4 hunk by hunk (post-commit line numbers at `9ff4bbd8e`)

### A.1 `lib/wardrobe/image-analysis.ts` (453 lines; md5 a104b648… vs baseline 93482ade…)

- `:39-48` — new `export interface ProposedOutfit { title: string; description: string; appropriateness: string }`.
  Its doc comment says the client turns it into a composite, so it carries no ids or types.
- `:53-59` — `ImageAnalysisResult` gains `proposedOutfit: ProposedOutfit | null`, positioned after `proposedItems`
  and before `provider`/`model`.
- `:134-172` — `SYSTEM_PROMPT` (a template literal with no interpolation). The FULL post-commit text follows, byte-copied
  from the source by `sed -n 134,172p`, including the `const … = \`` wrapper line and the closing backtick. The
  prompt bytes are the text between the backticks.

```text
const SYSTEM_PROMPT = `You are a fashion and costume analyst. Your task is to identify distinct clothing items and accessories visible in the provided image and describe each one in detail.

For each item you identify:
1. Give it a concise, evocative title (e.g., "Emerald Silk Evening Gown", "Worn Leather Ankle Boots")
2. Write a detailed description capturing texture, fit, color, material, and notable details. Use vivid, descriptive language — not clinical catalog copy.
3. Classify it into one or more slot types: "top", "bottom", "footwear", "accessories", "hair"
   - Items that span multiple slots (e.g., a dress covering top + bottom, a jumpsuit) should include all applicable types
   - If the subject wears a distinct, deliberate hairstyle (braids, an updo, an elaborate coif, a wig), emit ONE "hair" item describing the styling. Plain, loose, unstyled hair is NOT an item.
4. Suggest appropriateness tags (e.g., "formal", "casual", "combat", "intimate", "evening", "everyday") based on the visual context

Then name the ensemble the items make together — the whole look, as one outfit:
- A concise, evocative title for the outfit as a whole (e.g., "Midnight Gala Ensemble", "Rain-Soaked Detective's Kit")
- A short description of the overall look and the impression it gives, without re-describing each piece
- Appropriateness tags for the outfit as a whole

Return your analysis as a JSON object with this exact structure:
{
  "items": [
    {
      "title": "Item Title",
      "description": "Detailed description of the item...",
      "types": ["top"],
      "appropriateness": "casual, everyday"
    }
  ],
  "outfit": {
    "title": "Outfit Title",
    "description": "Overall impression of the ensemble...",
    "appropriateness": "evening, formal"
  }
}

Important rules:
- Focus ONLY on clothing, accessories, and a deliberate hairstyle if one is present. Do not describe faces, bodies, backgrounds, or other non-wearable features.
- Each distinct garment or accessory should be its own item.
- Valid types are ONLY: "top", "bottom", "footwear", "accessories", "hair"
- If you identify fewer than two items, set "outfit" to null.
- If you cannot identify any clothing items, return {"items": [], "outfit": null}
- Return ONLY the JSON object, no additional text or markdown.`
```

  What the commit changes in it: it inserts the 4-line "Then name the ensemble…" block plus a blank line before
  "Return your analysis"; the JSON example's `]` becomes `],` followed by the 5-line `"outfit": {…}` member; the
  rule `- If you cannot identify any clothing items, return {"items": []}` is REPLACED by two rules,
  `- If you identify fewer than two items, set "outfit" to null.` and
  `- If you cannot identify any clothing items, return {"items": [], "outfit": null}`.
  The prompt uses non-ASCII — (U+2014 em dash) in "not clinical catalog copy", "the whole look, as one outfit",
  and the hair line, plus straight quotes and `Detective's`, all of which must be copied as-is.
- `:174-183` — `buildUserPrompt` is UNCHANGED. It builds
  `'Analyze this image and identify all visible clothing items, accessories, and any deliberate hairstyle.'`
  and appends `\n\nAdditional guidance from the user: ${guidance}` when guidance is truthy.

```text
function buildUserPrompt(guidance?: string): string {
  let prompt =
    'Analyze this image and identify all visible clothing items, accessories, and any deliberate hairstyle.'

  if (guidance) {
    prompt += `\n\nAdditional guidance from the user: ${guidance}`
  }

  return prompt
}
```

- `:195-254` `parseAnalysisResponse(content)` now returns `{ proposedItems, proposedOutfit }`. Its rules, in order,
  are UNCHANGED except the last:
  1. `content.trim()`; if it starts with ```` ``` ````, strip `/^```(?:json)?\s*\n?/` and `/\n?```\s*$/`.
  2. `JSON.parse`; on a throw, ERROR `[Wardrobe Image Analysis] Failed to parse LLM response as JSON`
     `{contentPreview: content.substring(0,200)}`, then throw `The AI returned an invalid response. Please try again.`
  3. `!parsed || typeof !== 'object' || !('items' in parsed)` → throw
     `The AI returned an unexpected response format. Please try again.` (no log). A non-array `items` throws the same.
  4. Items are filtered: a truthy object, `title` a string, `description` a string.
  5. Map: `types` is filtered to `VALID_TYPES` (`WardrobeItemTypeEnum.options`), and an empty result becomes
     `['accessories']`. Title and description go through `String(..).trim()`. `appropriateness` is trimmed when it
     is a string, else `''`.
  6. NEW: `proposedOutfit: parseProposedOutfit(parsed.outfit, proposedItems.length)`. The count is taken AFTER
     filtering, so it counts surviving items, not raw ones.
- `:261-274` NEW `parseProposedOutfit(raw, itemCount)`. The order is load-bearing:
  1. `itemCount < 2 || !raw || typeof raw !== 'object'` → `null`, SILENT (no log). An array passes this gate
     because `typeof [] === 'object'`, and then falls to rule 3 via a missing title.
  2. `title = typeof outfit.title === 'string' ? outfit.title.trim() : ''`.
  3. `!title` → DEBUG `[Wardrobe Image Analysis] Model returned an outfit without a title; dropping it` (NO fields),
     then `null`. The count gate runs first, so a 1-item response with an untitled outfit logs NOTHING.
  4. Otherwise return `{ title, description: str ? trim : '', appropriateness: str ? trim : '' }`, keys in that order.
- `:416-434` in `runAnalyzeImageForWardrobeItems`: it destructures `{ proposedItems, proposedOutfit }`. The INFO
  `[Wardrobe Image Analysis] Analysis complete` fields are now `{itemCount, hasOutfit: proposedOutfit !== null,
  provider: profile.provider, model: profile.modelName, durationMs}`, with `hasOutfit` second. The return is
  `{ proposedItems, proposedOutfit, provider, model }`. The `llm_logs` write (`type: 'WARDROBE_IMAGE_ANALYSIS'`)
  and every other log line are unchanged.

### A.2 `app/api/v1/wardrobe/analyze-image/route.ts`

- `:3-8` doc comment only.
- `:46-51` INFO `[Wardrobe Image Analysis API] Analysis complete` `{itemCount, hasOutfit, provider, model}`.
- `:53-58` `successResponse({ proposedItems, proposedOutfit, provider, model })`, 200. `proposedOutfit` is emitted
  as JSON `null` when it is absent, never omitted.
- The schema (`:16-22`), the 14,000,000-char size gate, and the user-facing-error `badRequest` classifier
  (`No vision-capable` / `API key not found` / `The AI returned`) are all unchanged.

### A.3 `components/wardrobe/import-from-image-modal.tsx` (679 lines; +197/−?)

Types and constants: `ProposedOutfit`; `OutfitDraft extends ProposedOutfit { enabled; replace }`;
`MIN_OUTFIT_PIECES = 2`; `EMPTY_OUTFIT = { enabled:false, title:'', description:'', appropriateness:'', replace:true }`.
It imports `unionTypes` from `lib/wardrobe/composite-types.ts` (canonical slot order `top→bottom→footwear→accessories→hair`).

Flow:
1. After analysis (`:172-186`): zero items → the existing error message and back to upload. Otherwise items are
   `selected: true`, and `setOutfit(proposedOutfit && items.length >= 2 ? {...EMPTY_OUTFIT, ...proposedOutfit, enabled:true} : EMPTY_OUTFIT)`.
   The client re-applies the ≥2 gate on RAW item count. It does NOT trim the fields at seed time.
2. "Re-analyze" (`:450-455`) also resets `outfit` to `EMPTY_OUTFIT`.
3. Derived values (`:214-217`): `outfitAvailable = selectedCount >= 2`;
   `willCreateOutfit = outfitAvailable && outfit.enabled`;
   `outfitTitleMissing = willCreateOutfit && outfit.title.trim().length === 0`. Deselecting below 2 does NOT clear
   `enabled`, so reselecting restores the offer.
4. Ensemble card (`:547-635`, `opacity-50` when not `willCreateOutfit`):
   - The checkbox is `checked={willCreateOutfit}` and `disabled={!outfitAvailable}`; toggling it sets `enabled`.
   - Label: **"Also create an outfit from these pieces"**.
   - Hint: available → **`Bundles the ${selectedCount} selected items into a single outfit you can wear in one gesture.`**;
     otherwise → **"Select at least two items to bundle them into an outfit."**
   - When `willCreateOutfit`, four fields show:
     - **"Outfit title"** (`#wardrobe-image-outfit-title`, placeholder `e.g., Midnight Gala Ensemble`, no maxLength).
     - **"Appropriateness"** (`#wardrobe-image-outfit-appropriateness`, placeholder `e.g., formal, evening`, `maxLength={200}`).
     - **"Description"** (textarea `#wardrobe-image-outfit-description`, rows 2, placeholder `The overall look...`).
     - A checkbox **"Replace everything in its slots when worn"** with sub-line
       **"Off layers the pieces over whatever is already on."**, bound to `replace` (default true).
5. Footer (`:665-671`): submit label `Import ${n} Item${n!==1?'s':''}${willCreateOutfit?' + Outfit':''}`;
   `isDisabled={selectedCount === 0 || outfitTitleMissing}`.
6. `handleImport` (`:219-300`):
   a. For each selected item, SEQUENTIALLY, `POST /api/v1/characters/${characterId}/wardrobe` with
      `{title: item.title, description: item.description || null, types: item.types, appropriateness: item.appropriateness || null, isDefault:false}`.
      This body is unchanged. The piece bodies carry no `componentItemIds` and no `replace` key.
   b. `result.ok && result.data?.wardrobeItem` → push the RETURNED item (the server's id and types). Otherwise
      `console.warn('[ImportFromImageModal] Failed to create item:', title, error)`.
   c. `created.length === 0` → error toast **"Failed to import wardrobe items"**, then return. The modal STAYS OPEN,
      as before.
   d. Success toast **"1 wardrobe item imported from image"** or **`${n} wardrobe items imported from image`**.
      It fires BEFORE the outfit POST.
   e. If `willCreateOutfit && created.length >= 2`, one more POST to the same URL with body keys in this order:
      `{title: outfit.title.trim(), description: outfit.description.trim() || null, types: unionTypes(created), appropriateness: outfit.appropriateness.trim() || null, componentItemIds: created.map(c=>c.id), replace: outfit.replace, isDefault:false}`.
      The union is taken over the CREATED (server-returned) items, and only the ones that landed.
      - ok → success toast **`Outfit "${title.trim()}" assembled from ${created.length} pieces`**.
      - failure → `console.warn('[ImportFromImageModal] Failed to create outfit:', outfit.title, error)` and error
        toast **"The pieces were imported, but the outfit could not be assembled"**.
      - If `willCreateOutfit` but fewer than 2 landed → error toast
        **"Too few pieces were imported to assemble an outfit"**, with no POST.
   f. `onImported(); onClose()`. This now runs even when the outfit failed, because the pieces landed.
   g. A throw → **"Failed to import wardrobe items"**; `finally` clears `importing`.

**What the message claims that the hunks do not show:**
- "types as their slot union" is the union of the created pieces' RETURNED types. With an honest server that
  echoes them, it equals the proposal types, but the hunk reads `created`.
- "Pieces are kept if the outfit can't be made" — no rollback exists at all; that is simply why nothing deletes
  them. The piece success toast fires before the outfit attempt.
- Unmentioned: the client re-gates on raw item count ≥2; the offer can be ticked by hand when the model named no
  outfit, with import then disabled until a title is typed; there is a new "too few landed" toast; the modal now
  closes after a failed outfit.
- The `help/wardrobe.md` prose calls the button "**Import Selected**", which is a stale label (v4's own).

### A.4 Tests (oracle shapes)

- `__tests__/unit/lib/wardrobe/image-analysis.test.ts` +56, new describe "the proposed outfit". The model reply is
  mocked through `mockSendMessage` as `JSON.stringify(payload)`:
  1. Two items plus an outfit with `'  Club Night Ensemble '` → trimmed `{title:'Club Night Ensemble', description:'Sharp.', appropriateness:'evening'}`.
  2. An untitled outfit (`{description:'Nameless'}`) → null, with the items kept (2).
  3. A missing `outfit` → null.
  4. One item plus a titled outfit → null (the count gate).
  5. The system prompt contains `"outfit": {` and `set "outfit" to null`, found via `messages.find(role==='system')`.
  Not covered: a non-object outfit (string/number/array), non-string description/appropriateness → `''`, the DEBUG
  line, and count-after-filter.
- `__tests__/unit/wardrobe-image-analysis-api.test.ts` +2: the mock result and the expected JSON body both gain
  `proposedOutfit: {title:'Evening Velvet', …}` (a pass-through pin).
- `__tests__/unit/components/wardrobe/import-from-image-modal.outfit.test.tsx` +134 (new). `fetch` is routed: the
  analyze call returns ANALYSIS, and each wardrobe POST echoes `{...body, id:`item-${n}`}` with a 201.
  1. The title pre-fills `'Club Night'`. Clicking "Import 2 Items + Outfit" makes 3 POSTs, and `posted[2]` matches
     `{title, description:'Sharp and dark', appropriateness:'evening', componentItemIds:['item-1','item-2'], types:['top','footwear'], replace:true, isDefault:false}`.
     This is the ordering proof: the outfit is the THIRD POST and uses ids minted by the first two.
  2. Declining (click the offer label, then "Import 2 Items") → exactly 2 POSTs, none with `componentItemIds`.
  3. Deselecting the first piece → the offer is `disabled` and unchecked, and the label reads "Import 1 Item".
  4. `proposedOutfit: null` → the label reads "Import 2 Items"; ticking the offer shows "Import 2 Items + Outfit"
     DISABLED; typing 'My Look' in "Outfit title" enables it.

### A.5 Other files
`help/wardrobe.md` +14 (a new bullet "**Assemble an outfit** from the pieces --- see below" and a section
"#### The Whole Look, Bundled"). The README, CHANGELOG, and three package version files are stamps.

## B. v5 on main

- **Verb:** `Request::WardrobeAnalyzeImage { #[serde(flatten)] body: Value }` (`api/types.rs:2769-2774`) dispatches
  through `engine.rs:5441` to `api::wardrobe::wardrobe_analyze_image()` (`wardrobe.rs:1198`), which returns a typed
  refusal. It has no request/response types and no parse.
- **REST edge:** `quilltap-web/src/wardrobe_routes.rs:269-280` `wardrobe_analyze_image_post` (it parses the body,
  then dispatches, then `unwrap_to_http`). It is registered at `quilltap-web/src/lib.rs:479-480`.
- **Prompt bytes:** NONE anywhere in v5. `ggrep 'costume analyst'` gets zero hits. There is no generator or recorded
  constant. (The hair-slot round ported the generator hair constants but skipped this prompt.)
- **Parse:** none.
- **SPA:** no import-from-image component, and no `wardrobeAnalyzeImage` twin in `core-contract.ts`. The dialog
  documents the absent button (`wardrobe-control-dialog.ts:105-107`, `:383-384`). `llm-logs.api.ts:50` and
  `llm-inspector-entry.ts:14` do know the `WARDROBE_IMAGE_ANALYSIS` log type.
- **Guards that name the refusal:** `crates/quilltap-harness/tests/activity_span_sites_guard.rs:128-132` carries
  a row with "wardrobe_analyze_image_impl … is a REFUSAL ARM". Porting the verb flips that row. The
  `crates/quilltap-core/tests/unreported_if_blank_slots.rs:12` comment notes the skipped sibling.
- **Reusable infrastructure a future port would ride:**
  - The character wardrobe create: `Request::CharacterWardrobeCreate { character_id, item }`
    (`types.rs:693-696`) → `api/characters.rs:2668` `character_wardrobe_create`, which answers `{wardrobeItem}`.
    The TS twin is `core-contract.ts:1363-1367`, built by `wardrobe/wardrobe.api.ts:218-232` `containerCreateRequest`.
  - Composite parsing: `componentItemIds` in `api/wardrobe.rs:105,194,271` (archetype) and the vault create; the
    server-side union is `crate::wardrobe::union_types` (`wardrobe.rs:172`, used by `tools/wardrobe_create.rs:275`).
  - The SPA `unionTypes` twin: `apps/web/src/app/wardrobe/equipped-slots.ts:431`, already used by
    `wardrobe-item-editor.ts:662`.
  - A **create-pieces-then-bundle-by-returned-id precedent** exists in the SPA:
    `screens/characters/generators/wizard/save-generated.ts:70-107` reads `data.wardrobeItem.id` and feeds it to
    later composites' `componentItemIds`. Its comment records a past §3 bug where it read `item` and made every
    composite with `[]`. That is the exact trap for this flow.
  - The .qtap import `componentItemIds` remap is not needed here, because all ids are server-minted at create time.
  - Vision-profile selection helpers exist: `services/file_fallback.rs:296` and
    `files/attachment_support.rs:106` `profile_supports_mime_type`, plus the image-description profile in
    `db/chat_settings.rs`.

## C. Families and fixtures that move

- **For the NO-PORT reading:** only `help_tree_equivalence` moves (`QT_ORACLE_HELP_TREE`, the recipe is in its
  header; it regenerates the jest `help-tree-sync` case from a pinned worktree). The `help/` count is unchanged, so
  `a-vendored-count` literals do not move. Check the help-doc section/embedding families that read the tree
  (`help_section_size_equivalence`, `help_doc_sync_equivalence`) for a wardrobe.md-dependent row, because the
  section count for wardrobe.md changes (+1 H4 section). Per the memory note, only 2 of the 16 `help_*` families
  read the tree. `help_tree_embed_guard` compares embedded vs disk, so it stays green either way.
- **For a full port (future):** no family exists. It would need a NEW tier-1 family for prompt bytes +
  `parseAnalysisResponse` + `parseProposedOutfit` over v4's real module. `parseAnalysisResponse` is not exported,
  so the family would drive `analyzeImageForWardrobeItems` with `sendMessage` mocked, which makes it effectively
  tier-3: inject a canned `content` string, then compare `{proposedItems, proposedOutfit}`, the captured system and
  user messages, and the debug/info/error lines. v4's own test injects via `mockSendMessage.mockResolvedValue({content})`,
  and the oracle case would mirror that pattern under jest. A route family would add the 400 classifier plus the
  pass-through, and an llm_logs row would need tier-2. The SPA would need an import-from-image component plus a
  vitest spec mirroring the four modal cases.

## D. Traps

1. The prompt is an LLM input, so it must be byte-copied (em dashes, `Detective's`, the two-space JSON indent,
   no trailing newline before the closing backtick). v5 has no generator for it; a port should record it against
   v4's real `SYSTEM_PROMPT` (captured via the mocked `sendMessage` call) rather than transcribe it.
2. The null rules are ordered: count (post-filter) → falsy/non-object → title. The DEBUG line fires ONLY on the
   third rule, so a 1-item untitled outfit is silent, and an array `outfit` with ≥2 items logs the DEBUG line.
3. The client re-gates on RAW item count ≥2 when seeding, and on SELECTED count for availability and CREATED count
   at submit. These are three different counts.
4. The ordering proof is pieces sequential → success toast → outfit POST third, with ids from the RESPONSES
   (`save-generated.ts`'s old bug is the cautionary precedent). `types` is `unionTypes(created)` in canonical slot
   order; for the test that is `['top','footwear']`.
5. `description`/`appropriateness` become `trim() || null` on the outfit, but the pieces use `|| null` untrimmed.
   Only the outfit is trimmed.
6. The modal closes after an outfit failure but stays open when zero pieces landed.
7. Porting the verb flips the `activity_span_sites_guard.rs` refusal row and needs a census check for the new
   `Request` variant.
8. **Do not un-refuse the verb inside a drift row.** Porting it means adding a provider/vision call path, which the
   P4.9f1/f2 orders explicitly forbade that round. That needs a human ruling and its own order.
9. The help page describes a v5-absent feature. That is pre-existing: v5 already vendors v4's "Importing from an
   Image" section verbatim.

## E. Size + tier

- **NO-PORT + help re-vendor (recommended for the drift row):** XS. It is one `cp` of `help/wardrobe.md` from the
  pin, the `help_tree_equivalence` regen at the target pin, and a ledger ratification on the file list. A
  Haiku/Sonnet-tier agent can do it; it could ride any other lane.
- **Full import-from-image vertical (if ruled in):** M–L. It means ~453 v4 lines of service (profile selection,
  API key, vision `sendMessage` with an image attachment, the llm_logs write, parse), a route, a ~680-line modal as
  an Angular component, a new tier-1/3 family plus a route family plus a vitest spec, and 💸 dogfood (real vision
  spend). It needs an Opus-tier lane, planned as its own order and not as a drift catch-up.

---

## Summary

1. The ledger's `PORT (small)` for `9ff4bbd8e` is WRONG. v5's `wardrobeAnalyzeImage` is a refusal arm
   (`api/wardrobe.rs:1198`), and the SPA ships no import-from-image UI, both by P4.9f1/f2's named deferral.
2. v5 holds NO copy of the vision prompt, no `parseAnalysisResponse`, no modal, and no harness family or oracle case.
3. There is precedent: the hair-slot round skipped this same file's prompt edits as "surface unported".
4. Recommended verdict: lib + route + modal + three tests → NO-PORT, banked as riders on a future
   image-analysis vertical.
5. The only real port is `help/wardrobe.md` (+14 lines; v5's copy equals the `acadcc7cd` one). Byte-copy it and
   regen `help_tree_equivalence`; the count stays at 129.
6. v4 facts for the rider: the outfit null rules are ordered (post-filter count<2 → falsy/non-object → empty
   trimmed title with a no-field DEBUG line); the route always emits `proposedOutfit` (null, never omitted); the
   INFO line gains `hasOutfit`.
7. The client creates pieces sequentially, then the outfit POST as the Nth+1 call, with `componentItemIds` from the
   RETURNED ids and `types` from `unionTypes(created)`. There is no rollback, and a "too few landed" toast is new.
8. Reusable v5 pieces for a future port: `characterWardrobeCreate`, `union_types`/`unionTypes`, and the
   `save-generated.ts` returned-id pattern (with its past `item`-vs-`wardrobeItem` bug).
9. A surprise inside v4: its new help prose names an "Import Selected" button that the modal labels
   "Import N Items".
10. Size: the drift row is XS (Haiku/Sonnet). The full vertical is M–L (Opus, its own order, needs a human ruling
    to un-refuse an LLM verb, plus 💸 vision spend).
