# Survey: v4 `97b25fc53`, "Drape undressed characters in story backgrounds instead of re-dressing them"

Date: 2026-09-28. Surveyor: read-only agent. v4 `~/source/quilltap-server` at `main` = `97b25fc53` (clean); oracle baseline `acadcc7cd`.
v5 `/Users/csebold/source/quilltap-v5` on `main` (`ef29a058e`). All v4 line numbers below are **post-commit, at `97b25fc53`**.

17 files, `4.10.0-dev.99 → -dev.100`. Code: `lib/memory/cheap-llm-tasks/{types,index,image-scene-tasks}.ts`, `lib/image-gen/appearance-resolution.ts`, `lib/background-jobs/handlers/story-background.ts`. Docs: `help/story-backgrounds.md` (one paragraph extended), `help/the-concierge.md` (+2 lines, one new paragraph), CHANGELOG, README badge. Tests: one new file, one grown, three one-line mock additions.

**The ledger's reading checks out against the hunks, with three additions.** (1) The parse-failure fallback returns the input array unchanged. That covers both the non-array arm (`:1045`) and the `catch` arm (`:1054`), so the `undressed` key is absent there in *both* modes. (2) v5 has **none** of v4's three post-rule-3 log lines in `sanitize_appearances_if_needed`. That includes the line this commit edits. This gap predates the commit (§B.3). (3) The merge's new `textChanged` compare is **unchanged text-wise** from the old condition. The only behavioural change is the `undressed` arm.

---

## A. v4, hunk by hunk

### A.1 `lib/memory/cheap-llm-tasks/types.ts` (appended at `:262–280`), verbatim

```ts
/**
 * How the Concierge's appearance pass treats an explicit appearance bound for a
 * moderated image provider. `redress` swaps explicit states for neutral
 * clothing — for callers with no concealment guidance downstream. `conceal`
 * keeps the character's state honest and flags an undressed one, for the
 * story-background crafter, whose concealed guidance drapes them instead.
 */
export type AppearanceSanitizeMode = 'redress' | 'conceal'

/**
 * One character's appearance after the Concierge's sanitization pass
 */
export interface SanitizedAppearance {
  characterId: string
  appearanceText: string
  /** `conceal` mode only: the character is undressed in this scene */
  undressed?: boolean
}
```

`index.ts` re-exports both types (type block, `:22–23`) and `CONCEALMENT_MARKER` (value block, `:90`, after `sanitizeAppearance`).

### A.2 `image-scene-tasks.ts`: the marker (`:260–266`)

```ts
export const CONCEALMENT_MARKER = 'Undressed in this scene; depict with cinematic concealment.'
```

The marker is a **single-quoted** string literal (not a template), 59 UTF-16 units, with no escapes. It is declared *before* `STORY_BACKGROUND_CONCEALED_INTIMACY` (`:274`), so there is no temporal-dead-zone issue.

### A.3 The moderated intimacy block after the change (`STORY_BACKGROUND_CONCEALED_INTIMACY`, `:274–287`, FULL)

```
DEPICTING INTIMATE OR UNCLOTHED STATES:
When the scene context or a character description implies someone is undressed, partially clothed, or in an intimate state, do NOT render explicit nudity. Instead, use cinematic concealment to preserve the narrative truth while keeping the image renderable. The figure is still nude in the story — the camera is just polite about it. Choose whichever technique fits the scene most naturally, and you may combine them:
- Drapery: a sheet, blanket, towel, or robe carelessly arranged, falling just where it needs to fall; a quilt pulled up to the collarbone; a towel knotted at the chest
- Composition & framing: crop from the shoulders up, shoot from behind, use a side silhouette, or frame so the figure is partly off-canvas
- Foreground occlusion: furniture, a bedpost, hanging plants, a doorframe, billowing curtains, steam, mist, smoke, snowfall
- Lighting: backlighting to reduce the figure to silhouette; low-key candlelight or firelight casting shadow across the body; dim ambient light with only selective highlights
- Pose: limbs drawn up, body turned away, side profile, knees folded, hair falling forward over the shoulders
- Environment: water (bath, pool, sea, hot spring) at a discreet level; tall grass; deep snow; a steamed-over mirror
- Implied context: tousled bedding, discarded clothing on the floor, a robe thrown over a chair — let the surroundings tell the story while the figure itself is shown with tasteful concealment

The intent of the scene MUST come through. Use evocative mood language — "intimate", "tender", "vulnerable", "private moment", "morning after", "candlelit", "bare-shouldered", "barefoot", "freshly bathed" — to preserve the meaning. Do NOT erase the narrative by substituting fully-clothed alternatives ("wearing pajamas", "in casual clothes"); that destroys the scene. The reader of the final image should understand exactly what is happening; only the explicit anatomy is held back.

PER-CHARACTER REQUIREMENT:
Apply this to EACH character on their own, not just the ones you notice. A character must be depicted with at least one concealment technique above when their description ends with "${CONCEALMENT_MARKER}", or when it describes them as nude, naked, undressed, topless, bare-chested, partially undressed, or wearing nothing. Never pass a bare descriptor such as "topless" or "nude" through verbatim, and never dress such a character in clothes the story did not give them.
```

What the hunk changes:
- Before the commit the template closed with a backtick directly after `…held back.`.
- The hunk adds `\n\nPER-CHARACTER REQUIREMENT:\n<one paragraph>`.
- **`${CONCEALMENT_MARKER}` is the only interpolation in the block.** It sits inside ASCII double quotes: `…description ends with "${CONCEALMENT_MARKER}", or when…`. So the resolved text reads `ends with "Undressed in this scene; depict with cinematic concealment.", or when it describes…`. The marker's own final `.` falls inside the quotes, before the closing `"`.
- There is no trailing newline.

Resolved lengths, measured with node on the post-commit source and the marker substituted:
- Block: **2505** UTF-16 units (was 1968; v5's doc comment says 1968).
- Assembled concealed prompt (head/intimacy/tail/example/closing joined by `\n\n`): **5651** (was 5114).
- Candid assembly: unchanged.
- `APPEARANCE_SANITIZATION_PROMPT`: unchanged at 999.

The candid block (`:295`) is unchanged and carries neither the requirement nor the marker. v4's new test pins that.

### A.4 `APPEARANCE_CONCEALMENT_PROMPT` (`:412–433`, new, FULL, 1490 UTF-16 units, no interpolation)

```
You are a content safety filter for image generation prompts. You will receive character appearance descriptions that have been flagged as potentially explicit or inappropriate for a standard image generation provider. A later step will compose the image and knows how to handle undressed figures tastefully — drapery, framing, shadow, pose.

Your task is to rewrite ONLY the explicit parts, and to report whether each character is undressed.

GUIDELINES:
- Remove explicit anatomical or sexual detail. Keep hair color, eye color, body type, and other non-explicit physical traits unchanged
- Do NOT invent clothing. If the character is nude, topless, partially undressed, or wearing only something minimal, say so in plain neutral words ("unclothed", "bare-shouldered", "wrapped only in a towel") — never substitute pajamas, casual clothes, loungewear, or any garment the input does not give them
- Keep whatever clothing the character does have, described neutrally ("lingerie" → "a silk slip")
- Set "undressed" to true when the character is nude, topless, partially undressed, or wearing only a towel, sheet, or similar covering; otherwise false
- Keep descriptions concise and suitable for image generation
- Do NOT add new details that weren't implied by the original

You will receive a JSON array of objects with characterId and appearanceText.
Respond with a JSON array of objects with characterId, the rewritten appearanceText, and undressed (boolean).

JSON only - no other text.
```

⚠ The first sentence is **identical** to `APPEARANCE_SANITIZATION_PROMPT`'s: `You are a content safety filter for image generation prompts.`. All three v5 oracle mocks branch on `system.startsWith(...)` of exactly that sentence, so conceal-mode calls **still land in the sanitize branch**. To tell the two modes apart, a mock must use a second probe such as `system.includes('Do NOT invent clothing')` or `'report whether each character is undressed'`.

### A.5 `sanitizeAppearance` (`:1012–1060`)

```ts
export async function sanitizeAppearance(
  appearances: Array<{ characterId: string; appearanceText: string }>,
  selection: CheapLLMSelection,
  userId: string,
  chatId?: string,
  mode: AppearanceSanitizeMode = 'redress'
): Promise<CheapLLMTaskResult<SanitizedAppearance[]>> {
  logger.debug('[CheapLLM] Sanitizing appearances', {
    context: 'cheap-llm-tasks.sanitize-appearance',
    chatId,
    mode,
    count: appearances.length,
  })
  const messages: LLMMessage[] = [
    { role: 'system', content: mode === 'conceal' ? APPEARANCE_CONCEALMENT_PROMPT : APPEARANCE_SANITIZATION_PROMPT },
    { role: 'user', content: JSON.stringify(appearances) },
  ]
  return executeCheapLLMTask(selection, messages, userId,
    (content: string): SanitizedAppearance[] => {
      try {
        const cleanContent = stripCodeFences(content)
        const parsed = JSON.parse(cleanContent)
        if (!Array.isArray(parsed)) {
          return appearances // Return originals if parsing fails
        }
        return parsed.map((item: Record<string, unknown>) => ({
          characterId: String(item.characterId || ''),
          appearanceText: String(item.appearanceText || ''),
          ...(mode === 'conceal' ? { undressed: item.undressed === true } : {}),
        }))
      } catch {
        return appearances // Return originals on error
      }
    },
    'sanitize-appearance',
    chatId
  )
}
```

- **The DEBUG line fires first.** It is logged before the messages are built and before `executeCheapLLMTask`, so it fires on every call, including one whose LLM call later fails. Fields in order: `context`, `chatId`, `mode`, `count`. `chatId` is `undefined` when absent, so the key drops.
- **Prompt pick:** strict `mode === 'conceal'`; anything else gets the redress prompt.
- **Conceal-arm parse:** the item carries `undressed` only in conceal mode, and it is strict `=== true`. So `"true"`, `1` or an absent key all give `false`, and the key is **present** with value `false`. In redress mode the key is **absent** even if the model answered `undressed: true`; v4's new test pins that.
- **Fallback (non-array or JSON error):** returns the *input* `appearances`, so objects carry only `{characterId, appearanceText}` and have **no `undressed` key in either mode**. The user message stays `JSON.stringify(appearances)` in key order `characterId, appearanceText`, unchanged.

### A.6 `lib/image-gen/appearance-resolution.ts`

- `ResolvedCharacterAppearance` gains `needsConcealment?: boolean` (`:62–66`, after `wasSanitized`). It is only ever *set* to `true` or left absent; nothing writes `false`.
- The signature (`:391–400`) gains a trailing `mode: AppearanceSanitizeMode = 'redress'` after `chatId?: string`. The JSDoc (`:384–388`) says only a caller whose crafter carries concealment guidance may pass `conceal`.
- Rules 1–4 are **unchanged**. The existing rule-5 INFO line (`:465–470`) gains `mode` as the **last** field: `'[AppearanceResolution] Sanitizing dangerous appearance descriptions'` with `{context: 'image-gen.appearance-resolution', chatId, characterCount: appearances.length, mode}`.
- `toSanitize` is unchanged (`` `${physical}. ${clothing}`.trim() ``). The call becomes `sanitizeAppearance(toSanitize, cheapLLMSelection, userId, chatId, mode)` (`:477–483`).
- The failure WARN `'[AppearanceResolution] Sanitization failed, passing through original'` (`:485–492`) is unchanged.
- The reshaped merge (`:494–514`), verbatim:

```ts
  return appearances.map(appearance => {
    const sanitized = sanitizeResult.result!.find(
      s => s.characterId === appearance.characterId
    )
    if (!sanitized) return appearance
    const textChanged = sanitized.appearanceText !== `${appearance.physicalDescription}. ${appearance.clothingDescription}`.trim()
    if (!textChanged && !sanitized.undressed) return appearance
    return {
      ...appearance,
      ...(textChanged
        ? {
            // Use sanitized text as both physical + clothing combined
            physicalDescription: sanitized.appearanceText,
            clothingDescription: '',
            wasSanitized: true,
          }
        : {}),
      ...(sanitized.undressed ? { needsConcealment: true } : {}),
    }
  })
```

What the merge now does:
- No match, or no change and not undressed: the **same object** comes back.
- Unchanged text but `undressed`: the clothing is **preserved** and `wasSanitized` stays `false`, with `needsConcealment: true`. This is v4 test case 3.
- Changed text and `undressed`: all three sanitized fields plus `needsConcealment: true`.
- Undressed truthiness is a JS truthy test, but the parser only ever yields a boolean.

### A.7 `lib/background-jobs/handlers/story-background.ts`

- **The call (`:450–464`):** `sanitizeAppearancesIfNeeded(resolvedAppearances, conciergePolicy, isDangerousChat, uncensoredImageTarget, cheapLLMSelection, job.userId, payload.chatId, 'conceal')`. `'conceal'` is argument index **7**, with a three-line why-comment.
- **The description builder (`:474–507`):**

```ts
    if (resolved) {
      const descParts = [genderPrefix + resolved.physicalDescription];
      if (resolved.clothingDescription) {
        descParts.push(`Wearing: ${resolved.clothingDescription}`);
      }
      if (resolved.needsConcealment) {
        descParts.push(CONCEALMENT_MARKER);
        logger.debug('[StoryBackground] Character flagged for cinematic concealment', {
          context: 'background-jobs.story-background',
          jobId: job.id,
          characterId: char!.id,
        });
      }
      return { name: char!.name, description: descParts.join('. ') };
    }
```

Part order is `prefix+physical`, then `Wearing: …` (only if clothing is non-empty), then `MARKER`, all joined by `'. '`. The marker is therefore always the **last** part, which makes the crafter prompt's "ends with" claim true.
- When text changed, clothing is `''`, so the result is `<prefix><sanitizedText>. Undressed in this scene; depict with cinematic concealment.`.
- When text is unchanged, the result is `<prefix><physical>. Wearing: <clothing>. Undressed in this scene; …`.
- **No de-duplication of a trailing period.** A sanitized text ending in `.` yields `..` before the marker.
- The fallback (no resolved appearance) branch is unchanged and never carries the marker.

### A.8 v4's four tests (the oracle shapes)

- **`image-scene-concealment.test.ts` (new, +97).** It mocks `core-execution`, reads `calls[0][1][0].content` (the system prompt) and drives `calls[0][3]` (the parser).
  - Crafter bound for a moderated target: the prompt contains `PER-CHARACTER REQUIREMENT` and the marker.
  - Crafter bound for an uncensored target: the prompt contains neither.
  - `sanitizeAppearance` default mode: the prompt contains `"wearing nothing" → "wearing casual clothes"`. A parser answer carrying `undressed: true` comes back **without** it (`toEqual`, which cannot tell an absent key from an undefined one; the code omits it).
  - Conceal mode (`'chat-1','conceal'`): the prompt contains `Do NOT invent clothing` and not `wearing casual clothes`. `[{…undressed:true},{…no key}]` parses to `undressed: true` and `undressed: false`.
- **`appearance-resolution.test.ts` (+64).**
  - The existing redress case now passes `'redress'` explicitly and adds `needsConcealment` toBeUndefined.
  - New: in conceal mode, `mockSanitizeAppearance.mock.calls[0][4] === 'conceal'`, and changed text with `undressed:true` gives the sanitized fields plus `needsConcealment: true`.
  - New: conceal mode with an unchanged text echo (`'A young woman with red hair. Blue jeans and t-shirt'`) plus `undressed:true` gives `wasSanitized false`, clothing preserved, `needsConcealment true`.
- **`story-background-uncensored-target.test.ts` (+34).**
  - `mockSanitizeAppearances.mock.calls[0][7] === 'conceal'`.
  - The crafter input's `characters[0].description` matches `/unclothed, wearing pearls\. Undressed in this scene; depict with cinematic concealment\.$/` and does not match `/Wearing:/`.
- **`-backfill-absent` and `-sha256` tests:** one line each, adding `CONCEALMENT_MARKER` to their module mock. This is mock plumbing only.

### A.9 What the commit message does NOT change (verified in the hunks)

- `lib/tools/handlers/image-generation-handler.ts:1011` is **untouched**, so `generate_image` keeps the redress default. The commit message's claim is true.
- `lib/tools/handlers/image-generation-handler.ts:1011` and `story-background.ts:450` are the only two v4 callers of `sanitizeAppearancesIfNeeded`.
- No schema, no migration, no route, no SPA, no DB write shape. `needsConcealment` is never persisted.
- The candid intimacy block and both example blocks are unchanged.
- None of the six intervening commits (`acadcc7cd..97b25fc53^`) touches `story-background.ts`, `lib/image-gen/`, `lib/memory/cheap-llm-tasks/` or the image-gen handler. Checked with `git log` on those paths; only `c3eefa752` (prompt templates/wizard) and `f7f3d7bf0` touch nearby `lib/` areas. **So a `97b25fc53` oracle measures exactly this commit** on these paths.

---

## B. v5 on main

### B.1 `crates/quilltap-core/src/services/image_scene_tasks.rs`

`sanitize_appearance` is at `:501–546`:

```rust
pub async fn sanitize_appearance<C: CompletionProvider>(
    executor: &CheapLlmTaskExecutor, completion: &C, appearances: &[AppearanceText],
    selection: &CheapLlmSelection, _user_id: &str, _chat_id: Option<&str>,
) -> CheapLlmTaskResult<Vec<AppearanceText>> {
    // Wire struct {characterId, appearanceText} -> serde_json::to_string
    let messages = vec![
        CompletionMessage::system(prompt_text::APPEARANCE_SANITIZATION_PROMPT),
        CompletionMessage::user(user_content),
    ];
    let originals = appearances.to_vec();
    executor.execute(completion, selection, messages,
        move |content: &str| parse_sanitize_result(content, &originals),
        None, None, None, Some("sanitize-appearance"), CheapLlmTaskOptions::default()).await
}
```

- The parser `parse_sanitize_result` (`:550–563`) does `strip_code_fences`, then `from_str` (Err → originals), then `as_array` (None → originals), then maps items with `js_string_or_empty` (`:863`, JS-falsy → `""`).
- **There is no mode, no `undressed`, and no tracing anywhere in the file** (0 `tracing::` hits). `_chat_id` is unused.
- The return type is `Vec<AppearanceText>` (`:492–497`, `{character_id, appearance_text}`, `PartialEq`).
- The story prompt is assembled at `:692` (`build_story_background_prompt`, a five-piece `"\n\n"` join).
- Unit tests that **will move**:
  - `concealed_assembly_matches_the_pre_split_constant_length` (`:892`) asserts `encode_utf16().count() == 5114`. The new value is **5651**; the join-equality half stays valid.
  - `sanitize_parse_falls_back_to_originals` (`:988`) stays valid but needs mode arms.

### B.2 The generated `image_scene_tasks/prompt_text.rs` and its generator

`prompt_text.rs` holds these consts:
- `IMAGE_PROMPT_CRAFTING_PROMPT`
- `APPEARANCE_RESOLUTION_PROMPT`
- `APPEARANCE_SANITIZATION_PROMPT` (999)
- `SCENE_CONTEXT_DERIVATION_PROMPT`
- `STORY_BACKGROUND_PROMPT_HEAD`
- `STORY_BACKGROUND_CONCEALED_INTIMACY` (doc comment says 1968)
- `STORY_BACKGROUND_CANDID_INTIMACY`
- `STORY_BACKGROUND_PROMPT_TAIL`
- `STORY_BACKGROUND_CONCEALED_EXAMPLE`
- `STORY_BACKGROUND_CANDID_EXAMPLE`
- `STORY_BACKGROUND_PROMPT_CLOSING`

There is **no `APPEARANCE_CONCEALMENT_PROMPT` and no `CONCEALMENT_MARKER`.** The file's header says "Extracted mechanically … do not hand-edit. Regenerate with `harness/oracle/cases/gen-image-scene-prompts.mjs`".

The generator header (`:16–22`) is run as:

```
node ~/source/quilltap-v5/harness/oracle/cases/gen-image-scene-prompts.mjs \
  ~/source/quilltap-server \
  ~/source/quilltap-v5/crates/quilltap-core/src/services/image_scene_tasks/prompt_text.rs
```

It reads the **working tree** of whatever v4 path is given (not a sha), so it must run against a checkout at `97b25fc53`. v4 main is at that commit now. Header line 16 claims: "Every template literal is plain (no backtick, no `${...}` interpolation)". That claim becomes false.

Generator lines 30–70, verbatim:

```js
    30	}
    31	
    32	const src = fs.readFileSync(
    33	  `${V4}/lib/memory/cheap-llm-tasks/image-scene-tasks.ts`,
    34	  'utf8',
    35	);
    36	
    37	
    38	function extract(name) {
    39	  const start = src.indexOf(`const ${name} = \``);
    40	  if (start < 0) throw new Error(`${name} not found`);
    41	  const bodyStart = src.indexOf('`', start) + 1;
    42	  const bodyEnd = src.indexOf('`', bodyStart);
    43	  const body = src.slice(bodyStart, bodyEnd);
    44	  if (body.includes('`') || body.includes('${')) {
    45	    throw new Error(`${name} contains a backtick or interpolation — not raw-embeddable`);
    46	  }
    47	  return body;
    48	}
    49	
    50	// Pick a raw-string hash fence that does not collide with the body.
    51	function fence(body) {
    52	  let hashes = '';
    53	  while (body.includes(`"${hashes}`) || body.includes(`${hashes}"`)) hashes += '#';
    54	  // r#"..."# style — the closing is `"` + hashes.
    55	  return hashes.length ? '#'.repeat(hashes.length + 1) : '';
    56	}
    57	
    58	function emit(name, doc, body) {
    59	  const h = fence(body);
    60	  const open = h ? `r${h}"` : 'r"';
    61	  const close = h ? `"${h}` : '"';
    62	  return `/// ${doc}\npub const ${name}: &str = ${open}${body}${close};\n`;
    63	}
    64	
    65	const craft = extract('IMAGE_PROMPT_CRAFTING_PROMPT');
    66	const appres = extract('APPEARANCE_RESOLUTION_PROMPT');
    67	const appsan = extract('APPEARANCE_SANITIZATION_PROMPT');
    68	const scene = extract('SCENE_CONTEXT_DERIVATION_PROMPT');
    69	
    70	// [decd8ef9] The story-background prompt's seven pieces, in `buildStory-
```

(The `cat -n` numbering: line 30 is the `}` closing the usage check, and `src` is read at `:33–36`.)

At `97b25fc53` the guard at `:44` **throws on `STORY_BACKGROUND_CONCEALED_INTIMACY`**. `extract` is fine for `APPEARANCE_CONCEALMENT_PROMPT` (plain body), but nothing extracts it yet. `extract` also cannot find `CONCEALMENT_MARKER`, because it searches for `` const NAME = ` `` and the marker is single-quoted.

### B.3 `crates/quilltap-core/src/services/appearance_resolution.rs`

- `ResolvedCharacterAppearance` (`:61–72`, `#[derive(Clone, Debug, PartialEq)]`) has `character_id`, `character_name`, `physical_description`, `physical_description_name`, `clothing_description`, `clothing_source: String`, `was_sanitized: bool`. There is no concealment field.
- Struct literals appear in `appearance_resolution.rs` and the gate harness test only (6 hits, 2 files). They must gain the field, or the struct needs `Default`.
- `sanitize_appearances_if_needed` (`:350–481`) takes `(db, executor, moderation, completion, appearances: Vec<_>, concierge_policy, is_dangerous_chat, routes_dangerous_to_uncensored, selection, user_id, chat_id: Option<&str>)`, 11 arguments, with `#[allow(clippy::too_many_arguments)]` already present.
- Rule 1 logs its DEBUG (`:374`). **That is the only log line in the function.**
- ⚠ **Pre-existing absent v4 lines:**
  - The rule-3 INFO `Appearance text classified as dangerous` (`{context, chatId, score, categories, routesDangerousToUncensored}`).
  - The rule-5 INFO `Sanitizing dangerous appearance descriptions`. **This is the one the commit edits.**
  - The failure WARN `Sanitization failed, passing through original` (`{context, chatId, error}`).
  - A repo-wide grep finds none of the three strings.
- v4's classify `catch` ERROR `Appearance classification failed, passing through` has no v5 arm, because v5's `classify_content` returns a value, not a `Result`. The order should measure whether v4's `classifyContent` can throw at all before calling it a divergence.
- The merge at `:457–480`:

```rust
            match sanitized {
                Some(s) if s.appearance_text != combined => ResolvedCharacterAppearance {
                    physical_description: s.appearance_text.clone(),
                    clothing_description: String::new(),
                    was_sanitized: true,
                    ..appearance
                },
                _ => appearance,
            }
```

`combined` = `js_trim(format!("{}. {}", physical, clothing))`, which is the right twin of v4's `.trim()`. The `.find` is first-match, like v4. The "failure → originals" gate is `sanitize_result.result.filter(|_| sanitize_result.success)`.

### B.4 `crates/quilltap-core/src/services/story_background_job.rs`

- The call (`:527–549`) passes `…, uncensored_image_target, &cheap_selection, user_id, Some(&payload.chat_id)`. **v4's `catch` WARN `Appearance sanitization failed, using unsanitized` is unreachable in v5** because the gate is infallible. That was already true and this commit does not change it.
- The description builder (`:552–583`):

```rust
            let description = match resolved {
                Some(r) => {
                    let mut parts = vec![format!("{gender_prefix}{}", r.physical_description)];
                    if !r.clothing_description.is_empty() {
                        parts.push(format!("Wearing: {}", r.clothing_description));
                    }
                    parts.join(". ")
                }
                None => { /* mediumPrompt || shortPrompt || name fallback */ }
            };
```

- `job_id: &str` is in scope (`:216`, used by other logs as `job_id = job_id`). The log target convention is `target: "quilltap::story_background"`, `context = "background-jobs.story-background"` (`:362–365`).
- `char_id` is computed at `:560`, so the DEBUG can carry it.

### B.5 `crates/quilltap-core/src/tools/generate_image.rs:2053`, the second caller

It calls `sanitize_appearances_if_needed(db, deps.executor, deps.moderation, deps.completion, resolution.appearances, &concierge_policy, db_ctx.is_dangerous_chat, routes_dangerous_to_uncensored, selection, &ctx.user_id, ctx.chat_id.as_deref())`. It must pass `AppearanceSanitizeMode::Redress` explicitly, since Rust has no default arguments.

The downstream expander does not read `needs_concealment`, and must not start to. With `redress` the field can never be set, which is faithful to v4.

### B.6 The families

**`appearance_sanitize_gate_tier3_equivalence`** (Rust `crates/quilltap-harness/tests/…rs`, 408 lines; oracle `harness/oracle/cases/appearance-sanitize-gate.test.ts`; corpus `harness/oracle/fixtures/appearance-sanitize-gate.json`, 30 cases)
- It is **DB-free on the oracle side**. The Rust side uses a scratch `Db` with only `llm_logs`, which is not compared.
- Recipe:

```
N=~/.nvm/versions/node/v24.13.1/bin ; V5W=<worktree>
cd ~/source/quilltap-server
TMPO=/tmp/qt-appearance-gate-oracle; rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
cp $V5W/harness/oracle/cases/appearance-sanitize-gate.test.ts "$TMPO/cases/"
cp $V5W/harness/oracle/fixtures/appearance-sanitize-gate.json "$TMPO/fixtures/"
QT_ORACLE_OUT=/tmp/oracle-appearance-sanitize-gate.ndjson \
  $N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots "$TMPO/cases" -- "appearance-sanitize-gate.test"
# Rust
QT_ORACLE_APPEARANCE_GATE=/tmp/oracle-appearance-sanitize-gate.ndjson cargo test -p quilltap-harness --test appearance_sanitize_gate_tier3_equivalence
```

- **How a canned reply is injected.**
  - The oracle `jest.doMock('@/lib/llm')` replaces `createLLMProvider` with a `sendMessage` stub.
  - Classify calls (`user.startsWith('Classify the following content:')`) answer `spec.classifications[c.classification]`.
  - Sanitize calls (`system.startsWith('You are a content safety filter for image generation prompts.')`) answer by case flags: `sanitizeJunk` gives `{not:'an array'}`, `sanitizeEchoes` gives the input echoed, and otherwise `spec.sanitizedText[characterId]`.
  - Each unique `provider|model|temp|JSON(messages)` is recorded as `kind:"canned"`.
  - The Rust side loads every `canned` row into `CannedCompletionProvider::with_response`, which is exact-key replay, so **Rust gets whatever the oracle answered, provided Rust sends byte-identical messages.**
- **To pose a conceal-mode `undressed: true` reply:**
  1. Add `mode?: 'redress'|'conceal'` and an `undressed?: Record<charId, unknown>` answer map to `CaseSpec`.
  2. In the sanitize branch, include `undressed: <map value>` in each item when the case sets it.
  3. Pass `c.mode` as the 8th argument.
  4. Add `needsConcealment: a.needsConcealment` to the result projection (`JSON.stringify` drops `undefined`, so the key is present only when `true`).
  5. Rust: add `mode` to `CaseSpec`, pass it through, and add `#[serde(default)] needs_concealment: Option<bool>` to `OracleAppearance`, asserting `g.needs_concealment == e.needs_concealment.unwrap_or(false)`. Also assert that the oracle key is never `Some(false)`, as a guard.
  6. Keep the per-case TOKEN rule. The classification cache is process-global.
  - The coverage floors at the bottom (≥8 zero-call, ≥6 one-call, ≥4 two-call, one sanitized) should gain "≥1 row with `needsConcealment`" and "≥1 row with `needsConcealment && !wasSanitized`".

**`story_background_job_tier3_equivalence`** (Rust 1065 lines; oracle `harness/oracle/cases/story-background-job.test.ts`, 564 lines; corpus `harness/oracle/fixtures/story-background-job.json` with 23 `chats` keyed by label; fixture builder `harness/oracle/fixtures/build-story-background-job-fixture.ts`)
- Recipe:

```
N=~/.nvm/versions/node/v24.13.1/bin ; WT=<worktree>
cd ~/source/quilltap-server
QT_FIXTURE_STORY_MAIN=/tmp/qt-story-main.db QT_FIXTURE_STORY_MOUNT=/tmp/qt-story-mount.db \
  $N/node --import tsx $WT/harness/oracle/fixtures/build-story-background-job-fixture.ts
TMPO=/tmp/qt-story-oracle; rm -rf "$TMPO"; mkdir -p "$TMPO/cases" "$TMPO/fixtures"
cp $WT/harness/oracle/cases/story-background-job.test.ts "$TMPO/cases/"
cp $WT/harness/oracle/fixtures/story-background-job.json "$TMPO/fixtures/"
QT_FIXTURE_STORY_MAIN=… QT_FIXTURE_STORY_MOUNT=… QT_ORACLE_OUT=/tmp/oracle-story-background-job.ndjson \
  $N/npx jest --silent --watchman=false --testTimeout=120000 --roots "$PWD" --roots "$TMPO/cases" -- "story-background-job.test"
# Rust
QT_ORACLE_STORY=/tmp/oracle-story-background-job.ndjson QT_FIXTURE_STORY_MAIN=/tmp/qt-story-main.db QT_FIXTURE_STORY_MOUNT=/tmp/qt-story-mount.db \
  cargo test -p quilltap-harness --test story_background_job_tier3_equivalence
```

- **Canned injection** follows the same recording pattern.
  - The `doMock` sits inside `for (const [label, chat] of Object.entries(spec.chats))` (`:172`), so the loop's `label` is in closure alongside the `STORYCASE:` marker-derived `caseLabel`. The sanitize call's user message carries no `STORYCASE` marker, so branch on the loop `label`.
  - The classify branch always answers dangerous (0.91).
  - The sanitize branch (`:264–277`) answers every item with `'a woman with copper hair, in a high-necked woollen dress'` and no `undressed`.
  - The crafter branch keys on the system message's candid probe (`'The target image provider accepts adult content'`), so it is unaffected by the new moderated text.
- The Rust side replays the recorded `canned`, `cannedImage` and `cannedImageFailure` rows. It diffs the per-op result, five mount-index tables plus `files`, the chat and project story-image columns, the Lantern body and both `llm_logs` projections.
- **To pose `undressed:true`:**
  1. Add a chat such as `conceal_marks_undressed` (preScreen on, moderated, `uncensoredImageTarget` false).
  2. In the sanitize branch, when `label === 'conceal_marks_undressed'`, answer with `undressed: true`.
  3. Pose one text **ending in `.`** to pin the double period, and one text-unchanged echo with `undressed:true` to pin `Wearing: … . MARKER`.
  - The marker then reaches the craft call's **user** message (the characters block). The recorded craft key carries it, so a v5 description-builder divergence is a canned miss. Both `llm_logs` projections carry it too.

**`image_generation_tier3_equivalence`** (Rust 1338 lines; oracle `harness/oracle/cases/image-generation.test.ts`; corpus `image-generation.json` + the builder's `.meta.json`; fixture `harness/oracle/fixtures/build-image-generation-fixture.ts`)
- The recipe is in the header (`:52–70`). Stage at `/tmp/qt-imggen-oracle`, build the fixture with `node --import tsx build-image-generation-fixture.ts`, then run `npx jest … --forceExit … -- "cases/image-generation\\.test\\.ts$"`. Rust needs `QT_ORACLE_IMGGEN`, `QT_FIXTURE_IMGGEN_MAIN` and `QT_FIXTURE_IMGGEN_MOUNT`.
- The oracle **does** reach the sanitize branch (`:359`, answering `'a woman with silver hair, in a high-necked woollen dress'`) on its Concierge-on cases (5 `conciergeSettings` bags in the corpus). It always goes through **redress**, so the key bytes must not move.
- This is the neutrality family. Also neutral: `image_generate_route_equivalence`, which drives `execute_image_generation_tool` too.

---

## C. Families and fixtures that move

| Family | At `97b25fc53` against unported main | Why | Real-DB fixture? |
|---|---|---|---|
| `story_background_job_tier3_equivalence` | **RED (designed)**. Every concealed-variant case's craft key moves (the system message gains the requirement paragraph). Every sanitize-reaching case's key moves (conceal prompt). The `llm_logs` projections move. | prompt text plus mode | **YES**: the two-DB fixture from the builder, which uses v4's `generateDDL` plus repositories at **CWD**. |
| `appearance_sanitize_gate_tier3_equivalence` | Green for existing rows (they call with no mode, so redress, unchanged). **Grows** the conceal rows. | new arms | No (DB-free oracle; Rust scratch `llm_logs` only). |
| `image_generation_tier3_equivalence` | **Must stay GREEN with zero v5 change.** This is the neutrality proof: redress prompt, redress keys. | none | **YES**: the imggen two-DB fixture (`generateDDL` at CWD). |
| `image_generate_route_equivalence` | Should stay green (redress). | none | check its recipe |
| `help_tree_equivalence` (+ the help embed guard) | RED until `help/story-backgrounds.md` and `help/the-concierge.md` are byte-copied. The file count is **unchanged** (edits only, no new file). | help text | no |
| core unit `concealed_assembly_matches_the_pre_split_constant_length` | RED (5114 → 5651) | prompt text | — |

⚠ **The split-pin rule applies to both real-DB families.** Both builders import `@/lib/database/schema-translator` `generateDDL` and `ChatMetadataSchema` from the v4 checkout they run in. At `97b25fc53`, `ChatMetadataSchema` no longer has `renderedMarkdown` (`f7f3d7bf0`). A fixture built there lacks the column v5 main binds, so v5 cannot read it.
- **Build both fixtures from a pinned `acadcc7cd` worktree** of `quilltap-server`, using the ledger's pinned-worktree regen recipe. Then **run the jest oracle from the `97b25fc53` checkout** against those `/tmp` fixture DBs.
- The sweep driver runs builder and oracle in one v4 dir, so this is **not one `--run` invocation**. It is a two-step recipe; record it in the family header.
- **Measure first:** v4 `97b25fc53`'s real DB stack opens an `acadcc7cd`-vintage DB carrying `renderedMarkdown`. Confirm it does not run the `drop-chat-rendered-markdown-v1` migration on the oracle's per-case copy, or that nothing compared reads it. The story family compares only `storyBackgroundImageId`/`lastBackgroundGeneratedAt` on `chats`, so an extra column should be harmless. Confirm it.
- Neither fixture is committed; both go to `/tmp/qt-story-*.db` and `/tmp/qt-imggen-*.db`. There is **no committed pair to widen or narrow.** The recorded `/tmp/qt-imggen-*` staging collision still applies: never run imggen regens concurrently.

---

## D. Traps

1. **The generator must learn to inline the ONE named constant.** Proposed exact change to `gen-image-scene-prompts.mjs`:

```js
// [97b25fc53] ONE named single-quoted constant is interpolated into ONE template
// body (STORY_BACKGROUND_CONCEALED_INTIMACY). Resolve it here, emit it as its own
// pub const, and keep the `${` guard for everything else.
function extractSingleQuoted(name) {
  const m = src.match(new RegExp(`export const ${name} = '([^'\\\\\\n]*)'\\n`));
  if (!m) throw new Error(`${name} not found (expected a plain single-quoted literal)`);
  if (m[1].includes('`') || m[1].includes('${')) throw new Error(`${name} not raw-embeddable`);
  return m[1];
}
const NAMED = { CONCEALMENT_MARKER: extractSingleQuoted('CONCEALMENT_MARKER') };

function extract(name, allow = []) {
  const start = src.indexOf(`const ${name} = \``);
  if (start < 0) throw new Error(`${name} not found`);
  const bodyStart = src.indexOf('`', start) + 1;
  const bodyEnd = src.indexOf('`', bodyStart);
  let body = src.slice(bodyStart, bodyEnd);
  for (const k of allow) {
    const tok = '${' + k + '}';
    const n = body.split(tok).length - 1;
    if (n !== 1) throw new Error(`${name}: expected exactly one ${tok}, found ${n}`);
    body = body.split(tok).join(NAMED[k]);
  }
  if (body.includes('`') || body.includes('${')) {
    throw new Error(`${name} contains a backtick or an unlisted interpolation — not raw-embeddable`);
  }
  return body;
}
```

   The rest of the change:
   - The `storyParts` entry for `STORY_BACKGROUND_CONCEALED_INTIMACY` gets `allow: ['CONCEALMENT_MARKER']`, threaded into the `.map` call.
   - Add `const appcon = extract('APPEARANCE_CONCEALMENT_PROMPT')`.
   - Emit `pub const CONCEALMENT_MARKER` (59 UTF-16 units) and `APPEARANCE_CONCEALMENT_PROMPT` (1490 UTF-16 units), with the doc comment noting the resolved marker.
   - Add `appcon=` and `marker=` to the stderr summary line.
   - Fix header line 16's "no `${...}`" claim to name the one resolved interpolation.
   - Expected emitted lengths: intimacy **2505**, assembled concealed **5651**.
   - A Rust unit pin should assert `STORY_BACKGROUND_CONCEALED_INTIMACY.matches(CONCEALMENT_MARKER).count() == 1`, and that it contains `format!("ends with \"{CONCEALMENT_MARKER}\", or when")`. That makes the inline-versus-const agreement self-checking; Rust `concat!` cannot join `const &str`s.
   - The story builder imports `prompt_text::CONCEALMENT_MARKER`. Never re-type it.
2. **Marker placement and join.**
   - The marker is the last part, joined with `". "`. That is `Wearing:` then marker when clothing survives, or the sanitized text then marker when text changed.
   - **Do not trim or de-duplicate a trailing period.** `"….. Undressed …"` is v4's output. Pin it with a posed reply ending in `.`.
   - The fallback (unresolved) branch never carries the marker.
   - The DEBUG line fires once per flagged character with fields `context`, `jobId`, `characterId`.
3. **The mode-dependent `undressed` key.** Model it as `undressed: Option<bool>`.
   - Conceal mode gives `Some(item.get("undressed") == Some(&Value::Bool(true)))`. That is strict: `"true"` and `1` give `Some(false)`.
   - Redress mode gives `None`, even when the reply carries `undressed:true`.
   - The fallback gives `None` in **both** modes, because the originals come back unchanged.
   - The merge tests `undressed == Some(true)`. `needs_concealment` can be a plain `bool` on `ResolvedCharacterAppearance`, since v4 only ever writes `true` or leaves it absent. Mirror it as `Option<bool>` only if a projection needs key presence.
   - Unchanged text with undressed must return the clothing **intact** and `was_sanitized: false`. The existing `match` guard `Some(s) if s.appearance_text != combined` must be rewritten into v4's three-way shape, not extended.
4. **The DEBUG fires before the LLM call, even on failure.** Emit `[CheapLLM] Sanitizing appearances` at the top of `sanitize_appearance`, before building messages. Fields: `context="cheap-llm-tasks.sanitize-appearance"`, `chat_id`, `mode`, `count`. Pin it with a capture-layer test on a completion that errors (canned miss), so the line appears and the result is a failure.
   - Field **order** cannot be seen by a sorted-field capture (the memory note). Write it in v4's order anyway.
   - Also restore v4's rule-5 INFO with `mode` last, plus its two sibling absent lines (rule-3 INFO, the failure WARN). These are pre-existing absences, but the commit edits one of them. Scope that into this order explicitly, or name it as a follow-up.
5. **The redress path's bytes must not move.**
   - `APPEARANCE_SANITIZATION_PROMPT` stays 999.
   - The user message JSON is unchanged.
   - `generate_image.rs:2053` passes `Redress`.
   - Neutrality is proven by `image_generation_tier3_equivalence` (and `image_generate_route_equivalence`) **green at the `97b25fc53` oracle both before and after the port**. Only story and gate move.
   - Remember the recorded caveat that some families differ across two regens at the same pin (minted ids). The neutrality proof is "green", not "cmp-identical NDJSON".
6. **Mock branching:** both sanitizer prompts share their first sentence (§A.4). A mock that must answer differently per mode needs a second probe. Every conceal row also needs its own TOKEN (the process-global classification cache).
7. **Help re-vendor:** byte-copy the two pages. The count stays the same (the vendored-count literals do not move). Run `help_tree_equivalence` at the target pin, not only the embed guard.
8. **Rust signature churn:** add `mode: AppearanceSanitizeMode` as the trailing parameter of both functions, with `#[derive(Default)]` = `Redress` and `as_str()` returning `"redress"`/`"conceal"`. There are three call sites of `sanitize_appearances_if_needed` (story, generate_image, gate test) and one internal caller of `sanitize_appearance`. Update the struct literals in the gate test.

---

## E. Size and tier

**Size: small to medium, one lane.**
- Core: about 120–180 lines across `image_scene_tasks.rs`, `appearance_resolution.rs`, `story_background_job.rs`, a one-argument edit in `generate_image.rs`, and the regenerated `prompt_text.rs`.
- Generator: about 30 lines.
- Oracles: three small edits (the gate's mode, the undressed map and projection; story's new chat or chats plus a sanitize-branch arm; imggen none).
- Corpus: the gate gains about 8 rows: conceal changed+undressed, conceal unchanged+undressed, conceal not-undressed, conceal `"true"`-string, conceal junk fallback, redress with a stray `undressed:true`, conceal dangerous-but-routes-uncensored (no call), and the double-period text.
- Story gains 1–2 chats.
- Rust family edits, 3–4 capture pins, 2 help pages, and the version bumps (core, harness).

The split-pin regen for the two real-DB families is the fiddliest step.

**Recommended tier: Sonnet-class** implementer, working from an order that carries §D.1's generator diff and the split-pin recipe verbatim. Escalate to Opus only if the `97b25fc53` real-DB oracle refuses the `acadcc7cd`-vintage fixture (§C "measure first").

---

## Summary (10 lines)

1. The ledger's reading is **correct**. v4 adds `redress`/`conceal` sanitize modes, a 59-char `CONCEALMENT_MARKER`, a new 1490-unit conceal prompt, and a PER-CHARACTER REQUIREMENT paragraph in the moderated intimacy block. Story backgrounds pass `'conceal'`; generate_image is truly untouched.
2. The moderated block grows 1968 → **2505** units and the assembled concealed prompt 5114 → **5651**. The core unit pin at `image_scene_tasks.rs:894` moves.
3. The generator's `${` guard (`:44`) will throw. The proposed fix inlines exactly one named single-quoted constant (exactly-once check), emits `CONCEALMENT_MARKER` and `APPEARANCE_CONCEALMENT_PROMPT` as their own `pub const`s, and adds a Rust pin that the inline matches the const.
4. The `undressed` key depends on mode. It is strict `=== true` in conceal mode, absent in redress mode, and absent on the parse-failure fallback in both modes. `needsConcealment` is only ever `true` or absent.
5. The merge is now three-way. Unchanged text with undressed keeps the clothing and `wasSanitized:false`, so v5's `match` guard needs rewriting, not extending.
6. The marker always comes last, joined by `". "`, with no period de-duplication, so `..` is v4-faithful. The DEBUG line fires before the LLM call even when it fails.
7. **SURPRISE:** v5's `sanitize_appearances_if_needed` has none of v4's three post-rule-3 log lines, including the rule-5 INFO this commit edits, and `image_scene_tasks.rs` has no tracing at all. These are pre-existing absences; scope them in explicitly or name them as a follow-up.
8. **SURPRISE:** both sanitizer prompts open with the same sentence, so all three oracle mocks still route conceal calls to the redress branch until they gain a second probe.
9. Moving families: story (designed RED; real-DB fixture), the gate (grows; DB-free), help_tree (two pages). The neutrality proof is image_generation and image_generate_route staying green unchanged. Both real-DB fixtures must be built from an `acadcc7cd` worktree with the oracle run at `97b25fc53`: a two-step recipe the sweep's `--run` cannot do in one go. Measure that the new v4 opens the old-vintage fixture cleanly.
10. Size: one small-to-medium lane, Sonnet-class with this survey's generator diff and split-pin recipe in the order; escalate only if the cross-vintage oracle open fails.
