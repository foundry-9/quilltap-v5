# Survey — the Novel template (server + SPA), composer punctuation warnings + prose mode, and Salon diagrams (P4.D351 ∥ P4.D352 ∥ P4.D353)

Read-only planning survey, **2026-10-29**, for the `236c04e35` catch-up round.
v4 = `~/source/quilltap-server`, read ONLY through `git show 236c04e35:<path>`,
`git show <sha> -- <path>` and the detached pin worktree
`/tmp/qt-v4-pin-setupphase-7789ebed6` (code-identical to `236c04e35`).
**The pin is `236c04e35`** (`4.12.0-dev.0`); the oracle baseline v5 has ported is
`322a5488e`; v5 = `main` at **`f6cdceedd`** (core 0.0.1379, host 0.0.217, web
0.0.251, SPA 0.5.907). Every v4 line number below is at the pin unless it is a
`@@ -N` hunk anchor from the commit's own diff; every v5 line number is at
`f6cdceedd`. Nothing is taken from commit prose — every claim is from a shipped
hunk or a measured v5 line. The ledger rows were read first
(`drift-ledger.md:562` `778b97b2b`, `:576` `1dd0a0c01`, `:600` `2b2b190b6`, `:582`
`6588972d5`, `:585` `9faed0c7e`, `:588` `8bf856050`); where a hunk disagrees
with a row this survey says so (§0 item 8, §1.1.9, §5.1).

**Commits in scope (six):**

| sha | subject | this survey's half |
|---|---|---|
| `778b97b2b` | Feature: Novel roleplay template | whole, MINUS the two columns + the seed (keystone P4.D341) |
| `1dd0a0c01` | Feature: punctuation warnings in the Salon composer | whole (server key → P4.D351 *or P4.D356, see §7*; engine + UI → P4.D352) |
| `2b2b190b6` | Composer prose mode: narration/OOC preview and a no-dialogue warning | whole (SPA, P4.D352) |
| `6588972d5` | Feature: diagrams in the Salon (iteration 1) | CLIENT half (P4.D353); the SERVER half is P4.D349's — §6.3 names the wire contract |
| `9faed0c7e` | Feature: diagram lightbox and Copy PNG | whole (P4.D353) |
| `8bf856050` | Chore: prune diagram trace logging (#112) | the post-prune shape is what P4.D353 ports (NO-PORT? as a row; nothing separate to do) |

**Lanes informed:** P4.D351, P4.D352, P4.D353 — and, by file overlap, P4.D341
(keystone), P4.D345/P4.D346 (chat-settings types), P4.D349 (the diagram wire),
P4.D356 (`api/settings.rs`), P4.D357 (`_variables.css` / `_chat.css`), P4.D358
(composer / message-row a11y), P4.D359 (name chips in the renderer), P4.D360
(`help/diagrams.md`, the vendored `.qtap` schema).

## 0. Headline findings (written last)

1. **The Novel renderer lands in the SPA's TS port of v4's SERVER renderer, not
   its client.** v5 renders every message client-side through
   `apps/web/src/app/chat/render/markdown-renderer.ts` (a port of
   `lib/services/markdown-renderer.service.ts` + `markdown-postprocess.ts`) and
   `roleplay-rendering.ts`; v4's `MessageContent.tsx` hunks (`segmentsToReactNodes`,
   `processChildren`) have NO v5 counterpart. The port is `markdown-postprocess.ts`'s
   `applyRoleplayPatterns` open-tag stack + `stripUnmarkedSpans` and
   `roleplay-rendering.ts`'s nested `tokenizeWith` / `unmarked` scope / pre-parse
   `strip`+`literal` — verified by RE-CAPTURING `markdown-fixtures.json` from the
   pin with new Novel cases (`apps/web/tooling/capture-markdown-fixtures.mts`,
   the P4.30 / P4.D74 recipe). There is NO Rust renderer to touch.
2. **A v5-only bug-in-waiting the ledger missed:** `narrowRenderingPatterns`
   (`roleplay-rendering.ts:51-61`) coerces every `scope` that is not `'line'` to
   `'inline'` at the wire boundary — so the Novel template's `unmarked` /
   `literal` / `strip` patterns would be DESTROYED before the renderer sees them,
   and `compileRenderingPatterns` (`:120-127`) would compile `[\s\S]` as an inline
   rule claiming every character. P4.D352's first red is here, not in the renderer.
3. **`isQuoteSensitiveRoleplayConfig` must learn `mentionsGlyph`**
   (`chat/render/typography.ts:126`): Novel's dialogue pattern carries `"` AND an
   `rpBody` group, so today's v5 would silently switch `displayQuotes` OFF for every
   Novel chat; v4 exempts a pattern that also names the curly forms. Pin it with
   a captured `quotes-on-novel-dialogue` fixture (the `quotes-*` family already
   exists, 16 rows).
4. **`punctuationWarnings` is a READ-time materialization, not just a PUT arm.**
   v4's `ChatSettingsSchema` per-key default fills `punctuationWarnings: true` on
   every read of a stored three-key bag; v5's read is a RAW passthrough
   (`db/chat_settings.rs:1725-1731` via `SMART_TYPOGRAPHY_DEFAULT_JSON`, three
   keys), so `settings_routes_equivalence` GET rows go red on any pre-round row
   until the read fills it (the `wardrobeImageSettings.generateFromTools`
   precedent at `:445-495`). The column DDL default is UNCHANGED (three keys) —
   `SMART_TYPOGRAPHY_DEFAULT_JSON` (`chat_settings_composer_repair.rs:48`) must
   stay three keys and a SECOND four-key constant is needed for the schema /
   repository default.
5. **`chat_settings_seed.json` MOVES at this pin** (v4's repository default row,
   `chat-settings.repository.ts:113-118` at the pin, now carries four keys) — the
   keystone's `dump-fresh-schema.ts` re-dump (`QT_SEED_OUT`) captures it; P4.D351
   is wave 1 for exactly this reason. Append the register line in
   `provisioning/mod.rs:254-275`.
6. **Three lanes want `api/settings.rs` + `db/chat_settings.rs`** (P4.D351's
   `punctuationWarnings`, P4.D356's `coreWhisper` arm + unknown-key 400 +
   `7789ebed6` pins, P4.D345's `customToolIntervention` PUT arm). Recommend
   moving the `punctuationWarnings` SERVER key to P4.D356 (§7.2); the SPA half
   stays P4.D352's and the wire key is a one-line contract.
7. **The punctuation-lint engine is importable with zero deps** — `engine.ts`
   and `prose-mode.ts` import nothing (the eslint `no-restricted-imports` fence
   `eslint.config.mjs:124-129`), so a tsx recorder can drive v4's REAL engine over
   both vector files plus a generated corpus and emit a `.v4.json` the vitest spec
   replays (`concierge-state.oracle.spec.ts` + `harness/oracle/cases/
   concierge-chat-override.mjs` pattern); the byte-copied vectors
   (`typography-vectors.json` precedent, P4.D74 unit 1) are the floor.
8. **Ledger corrections:** (a) `778b97b2b`'s row says the DDL shows the two
   columns "APPENDED after `updatedAt` while Zod places them before `createdAt`"
   — confirmed at the pin (`DDL.md:1544-1550`): the keystone's live re-dump
   decides; `narrationDelimiters`' own line does NOT move (`TEXT DEFAULT '"*"'`
   at `:1545`, unchanged). (b) The row's "v5 intersection: `db/wardrobe.rs`"-class
   claim is absent here, but it DOES miss `services/backup/collect.rs:257`,
   `restore/orchestrator.rs:1099-1103`, `quilltap_import/entities.rs:325`, and
   `qtap_export/schema-key-order.json:256` — four carriers that read/write
   `narrationDelimiters` and must follow the codec + nullability. (c) The
   `1dd0a0c01` row says "the column DEFAULT text is UNCHANGED … every read / wire
   carries it" — correct, but it did not say v5's read is raw (item 4).
   (d) `6588972d5`'s row calls `hasDiagramFence` a fast-path gate; in v5 there is no
   `renderedHtml` fast path, so the walker's only v5 job is to FIND the closed
   diagram fences (and their sources) for the slot pass (§6.3).
9. **The diagram dependency ruling (§10.1):** `mermaid@12.2.0` is 122 MB unpacked
   with 23 runtime deps (`d3`, `cytoscape`, `elkjs`, `katex ^0.18` — v5 pins
   `katex 0.18.4`, compatible), `@hpcc-js/wasm-graphviz@1.30.0` 2.0 MB / zero deps
   (wasm inlined — no `.wasm` fetch, no MIME question; `static_serve.rs:34`
   already maps `application/wasm` anyway), `svg2roughjs@3.2.3` 1.9 MB
   (`roughjs`, `svg-pathdata`, `tinycolor2`), `dompurify@3.4.17` 1.6 MB. All four
   load by dynamic `import()` in v4 (`render-mermaid.ts:16`,
   `render-graphviz.ts:18,60`) — Angular's esbuild builder emits them as lazy
   chunks, OUTSIDE the `initial` budget (`angular.json:39-44`, 1.5 MB warn / 3 MB
   error). Tauri's CSP is `null` (`tauri.conf.json:19`) and `quilltap-web` sets
   no page CSP, so `WebAssembly.instantiate` needs no `'wasm-unsafe-eval'`.
10. **Four shared SPA files need a named owner:** `salon-conversation.ts` (P4.D352
    + P4.D344 + P4.D358 + P4.D359), `message-row.ts` / `message-content.ts`
    (P4.D353 + P4.D358 + P4.D359), `chat-settings.types.ts` (P4.D352 + P4.D346),
    `_variables.css` (P4.D352 + P4.D353 + P4.D357 + P4.D359). §7.4 lays out the
    anchors and the HANDOFF shape. The `core-contract.ts` fence convention
    (one `// === P4.Dnnn … ===` block per lane, unifier merges) holds.

## 1. Per commit, the shipped hunks

### 1.1 `778b97b2b` — the Novel roleplay template (34 files, +1,458 / −211)

#### 1.1.1 `lib/chat/novel-template.ts` (NEW, 119 lines) — the seed row's bytes (KEYSTONE consumes; P4.D351 cites)

- `NOVEL_DELIMITERS` (`:26-46`): three `wrap` entries in this order —
  `{kind:'wrap', name:'Dialogue', buttonName:'Dia', delimiters:'"', style:'qt-chat-dialogue', matchCurlyQuotes:true, closeOptional:true}`,
  `{kind:'wrap', name:'Emphasis', buttonName:'Emph', delimiters:'*', style:'qt-roleplay-plain', hideDelimiter:true, addOns:{bold:true, italic:true, reverse:false, underline:'none', border:'none', font:''}}`,
  `{kind:'wrap', name:'Out of Character', buttonName:'OOC', delimiters:['((','))'], style:'qt-chat-ooc'}`.
- `NOVEL_UNMARKED_STYLE` (`:49-52`): `{style:'qt-roleplay-plain', addOns:{bold:false, italic:true, reverse:false, underline:'none', border:'none', font:''}}`.
- `NOVEL_SYSTEM_PROMPT` (`:59-91`) — MODEL-VISIBLE, byte-exact. Begins
  `[SYSTEM INSTRUCTION: NOVEL FORMATTING]\nWrite every reply the way a published novel is typeset. This replaces any roleplay formatting habits you may have.\n\n1. NARRATION IS PLAIN PROSE. …` through rule 7 and
  `EXAMPLE OF A CORRECTLY FORMATTED REPLY:` with the Marguerite / Theo passage,
  ending `So he had read it. Of course he had.` (no trailing newline). The
  template literal contains `“...”` as JS escapes in rule 2 — the STORED
  bytes are the curly glyphs. The seed dump (`dump-builtin-templates.ts`) carries
  it verbatim; nobody transcribes it.
- `NOVEL_TEMPLATE` (`:94-119`): `userId:null, name:'Novel', description:'Prose as a novel prints it: plain narration, "dialogue" in quotes, *emphasis* in single asterisks, and ((out of character)) in double parentheses.', systemPrompt, isBuiltIn:true, tags:[], delimiters:NOVEL_DELIMITERS, renderingPatterns: generateTemplateRenderingPatterns({delimiters, narrationDelimiters:null, unmarkedStyle, markdownAsterisks:false}), dialogueDetection:{openingChars:['"','“'], closingChars:['"','”'], className:'qt-chat-dialogue'}, narrationDelimiters:null, unmarkedStyle:NOVEL_UNMARKED_STYLE, markdownAsterisks:false`.
  The seeded `renderingPatterns` are therefore, in order: `{pattern:'\\*{2,}', className:'', scope:'strip'}`, `{pattern:'\\*', className:'', scope:'literal'}`, the dialogue wrap `["“](?<rpBody>[^"”]+)(?:["”]|$)` (source form — see 1.1.3), the hidden emphasis wrap `(?<!\*)\*(?<rpBody>[^*]+)\*(?!\*)` with `hideDelimiters:true` and `className:'qt-roleplay-plain qt-rp-bold qt-rp-italic'`, the OOC wrap `\(\((?<rpBody>[^)]+)\)\)`, then `{pattern:'[\\s\\S]', className:'qt-roleplay-plain qt-rp-italic', scope:'unmarked'}`. **Do not hand-derive these — the dump emits them.**

#### 1.1.2 `lib/schemas/template.types.ts` — the Zod shapes (hunks `@@ -88`, `-180`, `-189`, `-198`, `-236`, `-254`)

- `WrapDelimiterSchema` gains `matchCurlyQuotes: z.boolean().optional()` and
  `closeOptional: z.boolean().optional()` AFTER `delimiters` (schema order ⇒
  stored key order on the INSERT path).
- `RenderingPatternSchema.className` loosens `z.string().min(1)` → `z.string()`;
  `scope` → `z.enum(['inline','line','unmarked','literal','strip']).optional()`;
  the object gains `.refine(p => p.scope==='literal' || p.scope==='strip' || p.className.length > 0, {message:'className is required for span-styling patterns', path:['className']})`.
- NEW `UnmarkedStyleSchema = z.object({style: z.string().min(1).max(50), addOns: DelimiterAddOnsSchema.optional()})`.
- `RoleplayTemplateSchema`: `narrationDelimiters: NarrationDelimitersSchema.nullable().default('*')`;
  NEW `unmarkedStyle: UnmarkedStyleSchema.nullable().optional()` and
  `markdownAsterisks: z.boolean().default(true)` BEFORE `createdAt` (Zod order
  ⇒ the route-read / export key order places them after `narrationDelimiters`).

#### 1.1.3 `lib/chat/annotations.ts` — pattern generation (hunks `@@ -7`, `-102`, `-165`, `-207`)

- `composeClassName(styled: {style; addOns?})` now takes any styled thing (the
  unmarked style too).
- NEW `TemplateRenderingOptions {unmarkedStyle?, markdownAsterisks?}`; NEW
  `LITERAL_ASTERISK_PATTERNS = [{pattern:'\\*{2,}', className:'', scope:'strip'}, {pattern:'\\*', className:'', scope:'literal'}]`.
- `generateRenderingPatterns(delimiters, narrationDelimiters?: NarrationDelimiters | null, options = {})`:
  when `options.markdownAsterisks === false` the two literal patterns are
  PREPENDED (copies); a `null` narration generates NO narration pattern (the
  `if (narrationDelimiters)` arm is unchanged, so `null` simply skips); when
  `options.unmarkedStyle?.style` is truthy an `{pattern:'[\\s\\S]', className: composeClassName(unmarkedStyle), scope:'unmarked'}` is APPENDED last.
- NEW `generateTemplateRenderingPatterns(template: {delimiters?, narrationDelimiters?, unmarkedStyle?, markdownAsterisks?})` = the one call the routes and the seed use.
- `buildDelimiterPattern`'s `wrap` arm passes `{matchCurlyQuotes, closeOptional}`
  to `buildWrapPattern(prefix, suffix, opts)`. NEW helpers `CURLY_QUOTE_FORMS`
  (`'"' → {open:'“', close:'”'}`, `"'" → {open:'‘', close:'’'}`),
  `STRAIGHT_FORM_OF`, `uEscape(ch)` (`\\u` + 4-hex lower-case), `quoteTolerant(delim, side)`
  (each quote glyph → `[<escaped straight><\uXXXX curly>]`, others `escapeRegex`;
  returns the raw chars for the exclusion class).
- `buildWrapPattern`: a NEW early arm when `prefix && suffix && (matchCurlyQuotes || closeOptional)`:
  `exclude` = the de-duplicated close chars each rendered as `\uXXXX` when
  `> 0x7f`, `\-` for `-`, else `escapeRegex`; `closing = closeOptional ? '(?:'+close.source+'|$)' : close.source`;
  `linkExclusion = suffix === ']' && !closeOptional ? '(?!\\()' : ''`;
  result `{regex: open.source + '(?<rpBody>[^'+exclude+']+)' + closing + linkExclusion, scope:'inline'}`
  (NB: for `"` this is `["“](?<rpBody>[^"”]+)(?:["”]|$)` — the
  same-open/close `(?<!…)` guard of the plain path is NOT applied here). The three
  legacy arms are unchanged.

#### 1.1.4 `lib/chat/roleplay-rendering.ts` — the shared renderer core (hunks `@@ -63`, `-80`, `-98`, `-125`, `-315`, `-379`, `-420`, `-453`)

- `Segment` gains `children?: Segment[]`; `CompiledRule.scope` becomes
  `'inline' | 'line' | 'unmarked'`.
- `compileRenderingPatterns` FILTERS OUT `literal` and `strip` (they never become
  rules), maps `scope: 'line' → 'line'`, `'unmarked' → 'unmarked'`, else `'inline'`.
- NEW `unmarkedClassFor(rules)` = `rules.find(r => r.scope === 'unmarked')?.className || undefined`.
- `MAX_INLINE_NESTING = 3`; NEW `TokenizeOptions {unmarkedClassName?}`;
  `tokenizeInline(text, rules, options = {})` → `tokenizeWith(text, inlineRules, options.unmarkedClassName, 0)`.
  `tokenizeWith`: `plain(t)` tags unclaimed runs with the unmarked class; a
  ZERO-LENGTH match is skipped (`if (match[0].length === 0) continue`); the
  earliest match records the RULE; after a claim, when `depth < 3`, the OTHER
  inline rules re-tokenize the claimed display text with NO unmarked class, and
  `children` is attached only when some inner segment got a class.
- `segmentsToHtml` recurses into `children`.
- `escapeMarkdownInBrackets`: splits `strip` / `literal` / inline (`!p.scope || p.scope === 'inline'`);
  the early-return adds `stripPatterns.length === 0 && literalPatterns.length === 0`;
  per non-code part: `applyPreParse(part, stripPatterns, () => '')` FIRST, the
  existing escapes, then `applyPreParse(result, literalPatterns, m => m.replace(MARKDOWN_PUNCT, '\\$1'))` LAST.
  NEW constants: `MARKDOWN_PUNCT = /([!-/:-@[-`{-~])/g`,
  `THEMATIC_BREAK = /^ {0,3}([*_-])(?:[ \t]*\1){2,}[ \t]*$/`,
  `LEADING_BULLET = /^([ \t]*[*+-][ \t]+)/`,
  `PROTECTED_INLINE = /(`[^`\n]*`|\$\$[\s\S]*?\$\$)/g`.
  `applyPreParse` compiles each pattern as `new RegExp('(?<!\\\\)(?:'+p.pattern+')', flags-without-g + 'g')`
  (a malformed pattern is skipped, never throws), splits on `PROTECTED_INLINE`
  (odd chunks untouched), then per line: a thematic break is left whole, a
  leading bullet is split off and re-attached, the body gets each regex applied.

#### 1.1.5 `lib/services/markdown-postprocess.ts` — the server post-pass (hunks `@@ -15`, `-32`, `-68`, `-77`, `-118`, `-127`) — **the v5 renderer's actual twin**

- `applyRoleplayPatterns(html, compiledRules)`: computes `unmarkedClassName = unmarkedClassFor(compiledRules)`
  and keeps an `openTags` stack; on every tag part (after the code/pre counting)
  `if (unmarkedClassName) trackOpenTags(openTags, lowerPart)`; a text part is
  tokenized with `{unmarkedClassName: directlyInBlock ? unmarkedClassName : undefined}`
  where `directlyInBlock = openTags.length > 0 && UNMARKED_BLOCK_TAGS.has(top)`.
  `UNMARKED_BLOCK_TAGS = {p, li, blockquote, h1..h6}`; `VOID_TAGS = {br, hr, img, input, wbr, col, area, base, embed, link, meta, source, track}`;
  `trackOpenTags` parses `/^<(\/?)([a-z][a-z0-9-]*)/`, a closing tag truncates the
  stack at the LAST same-name entry, a void or `/>`-ended tag is not pushed.
- NEW `stripUnmarkedSpans(html, cls)` — removes `<span class="<cls>">…</span>`
  (plain-text bodies only, lazy match, global) — called in `applyLineScopedClasses`
  on a block a LINE rule claims, BEFORE the hide-prefix slice.
- NB v5's `applyRoleplayPatterns` (`markdown-renderer.ts:90-131`) also carries a
  `katexDepth` guard v4's server post-pass does not have (a v5 addition from the
  math port); the open-tag stack must be threaded around it without disturbing it.

#### 1.1.6 `lib/markdown/typography.ts` (hunks `@@ -89`, `-112`, `-123`)

NEW `mentionsGlyph(source, glyph)` = `source.includes(glyph) || source.toLowerCase().includes('\\u' + hex4)`;
`isQuoteSensitiveRoleplayConfig`: a pattern that `includes(quote)` is SKIPPED
when `CURLY_FORMS[quote].every(c => mentionsGlyph(pattern.pattern, c))`.

#### 1.1.7 `lib/chat/template-prompt-hint.ts` (hunks `@@ -20`, `-50`) — MODEL-VISIBLE starter text

`generateFormattingPromptHint(delimiters, narrationDelimiters?: NarrationDelimiters | null, options: {markdownAsterisks?} = {})`.
Three new bullets, byte-exact:
- `narrationDelimiters === null` ⇒ `- Narration and action: plain prose with no marks around it (e.g. She stepped closer.). Never wrap narration in asterisks or brackets.`
- a `wrap` with notes: `matchCurlyQuotes` ⇒ note `straight or curly quotation marks are both fine`; `closeOptional` ⇒ note `` `when it runs over several paragraphs, leave ${suffix} off every paragraph but the last` ``; tail = `' ' + notes.map(n => n[0].toUpperCase() + n.slice(1)).join('; ') + '.'` appended after `(e.g. …).`
- `options.markdownAsterisks === false` ⇒ `- Never use double asterisks (**like this**); they have no meaning here and will not display as bold.` (appended AFTER the delimiter bullets, before the empty check).

#### 1.1.8 `lib/database/repositories/roleplay-template-narration-codec.ts` (NEW, 50 lines) + the repository (hunks `@@ -11`, `-64`, `-101`, `-116`, `-162`, `-179`, `-323`) — KEYSTONE's; the semantics P4.D351's carriers must honour

- `encodeNarrationDelimiters(v) = JSON.stringify(v)` — `'*'` STORES as `"*"` (three bytes), a pair as `["[","]"]`, `null` as `null` (the JSON text `null`, i.e. the column is the four-byte string `null`, NOT SQL NULL).
- `decodeNarrationDelimiters(stored)`: SQL `null` → `null`; an array pair → itself; a non-string / empty → `'*'`; JSON-parsable text → `null` / string / pair; non-JSON text → itself (legacy raw `*` / `+`).
- The repository overrides `toPersistedRow`, `_findById`, `_findAll`, `findByFilter`, `findOneByFilter` (decode), the seeder's INSERT (`this.encoded(validated)`) and UPDATE `$set` (`this.encoded(updateData)`, now also `$set`ting `unmarkedStyle` and `markdownAsterisks`), and `update()` always carries an ENCODED narration (the patch's, else the stored one) and returns a DECODED row.
- The two legacy built-ins gain `unmarkedStyle: null, markdownAsterisks: true`; `NOVEL_TEMPLATE` is the third element.

#### 1.1.9 The migration + prettify (KEYSTONE's; recorded here for §3)

`migrations/scripts/add-roleplay-template-unmarked-fields-v1.ts` (94 lines):
`dependsOn: ['add-narration-delimiters-field-v1']`, `introducedInVersion: '4.11.0'`,
`shouldRun` = SQLite && table exists && (either column missing); `run` =
`addColumnIfMissing('roleplay_templates','unmarkedStyle','TEXT')` then
`addColumnIfMissing('roleplay_templates','markdownAsterisks','INTEGER DEFAULT 1')`.
Prettify label `'Setting aside a shelf for the unadorned novel'`
(`lib/startup/prettify.ts:308`). Log lines in §4.

**Ledger correction:** the row's parenthetical "(verify whether
`narrationDelimiters`' own line moves now that it is `.nullable().default('*')`)"
— measured: `DDL.md:1545` is `"narrationDelimiters" TEXT DEFAULT '"*"'`, unmoved.

#### 1.1.10 The routes (`app/api/v1/roleplay-templates/route.ts` `@@ -12`, `-77`, `-106`, `-124`; `[id]/route.ts` `@@ -12`, `-27`, `-51`, `-122`)

POST:
- the narration guard becomes `if (narrationDelimiters === undefined || narrationDelimiters === '') return errorResponse('Narration delimiters are required (null for unmarked prose)', 400)`; an explicit `null` passes (the string / array arms unchanged; note the old `!narrationDelimiters` truthiness guard is GONE, so `0`/`false` now fall to the final `must be a string or [string, string] array` arm).
- NEW `UnmarkedStyleSchema.nullable().optional().safeParse(body.unmarkedStyle)` → 400 `` `Invalid unmarked style: ${issues[0]?.message ?? 'malformed'}` ``; NEW `if (body.markdownAsterisks !== undefined && typeof body.markdownAsterisks !== 'boolean') return errorResponse('markdownAsterisks must be a boolean', 400)`; `unmarkedStyle = unmarkedResult.data ?? null`; `markdownAsterisks = body.markdownAsterisks ?? true`.
- regen whenever `renderingPatterns` is empty (the `templateDelimiters.length > 0 || narrationDelimiters` condition is GONE): `generateTemplateRenderingPatterns({delimiters, narrationDelimiters, unmarkedStyle, markdownAsterisks})`.
- NEW DEBUG `Roleplay template patterns resolved for create` `{delimiterCount, patternCount, unmarkedNarration: narrationDelimiters === null, markdownAsterisks}` (fires on EVERY create, not only on regen).
- `create({… narrationDelimiters, unmarkedStyle, markdownAsterisks})` — the 201 echo therefore carries both new keys (after `narrationDelimiters`, before `createdAt`).

GET-[id]: regen whenever `!template.renderingPatterns || length === 0`
(the delimiters/narration condition GONE) via `generateTemplateRenderingPatterns(template)`.

PUT: `updateRoleplayTemplateSchema` gains `narrationDelimiters: z.union([...]).nullable().optional()`, `unmarkedStyle: UnmarkedStyleSchema.nullable().optional()`, `markdownAsterisks: z.boolean().optional()`.
`derivesPatterns = delimiters !== undefined || narrationDelimiters !== undefined || unmarkedStyle !== undefined || markdownAsterisks !== undefined`;
when `derivesPatterns && !validatedData.renderingPatterns` → regen over
`{delimiters: new ?? existing, narrationDelimiters: new !== undefined ? new : existing, unmarkedStyle: likewise, markdownAsterisks: new ?? existing}` + NEW DEBUG
`Roleplay template patterns regenerated for update` `{templateId, patternCount}`.
(The two old arms — "delimiters changed" / "narration only" — collapse into this one.)

#### 1.1.11 The SPA (`components/settings/roleplay-templates/*`, `FormattingToolbar.tsx`, `MessageContent.tsx`, `SalonView.tsx`, `ChatComposer.tsx`)

- `types.ts`: `RoleplayTemplate.narrationDelimiters: string | [string,string] | null`, `unmarkedStyle?: UnmarkedStyle | null`, `markdownAsterisks?: boolean`; `DelimiterFormEntry` + `matchCurlyQuotes: boolean`, `closeOptional: boolean`; `TemplateFormData.narrationDelimiterMode: 'single'|'pair'|'none'`, + `unmarkedEnabled: boolean`, `unmarkedStyle: string`, `unmarkedAddOns: DelimiterAddOns`, `markdownAsterisks: boolean`; `EMPTY_DELIMITER` + the two falses; `INITIAL_FORM_DATA` + `unmarkedEnabled:false, unmarkedStyle:'qt-roleplay-plain', unmarkedAddOns:{...EMPTY_ADD_ONS}, markdownAsterisks:true`.
- `hooks/useRoleplayTemplates.ts`: `delimiterToFormEntry` reads `d.matchCurlyQuotes ?? false` / `d.closeOptional ?? false` (wrap only; linePrefix/tagPrefix entries get `false`); `formEntriesToDelimiters` spreads `{matchCurlyQuotes:true}` / `{closeOptional:true}` ONLY when ticked (an unticked box emits NO key); NEW `narrationFromForm` (`'none'` → `null`), `unmarkedFromForm` (`!unmarkedEnabled` → `null`; `style = trim() || 'qt-roleplay-plain'`; `addOns` only when `hasAnyAddOn`), `templateToFormData(template, name)` (shared by edit + copy-as-new; `narrationDelimiterMode = delimiters === null ? 'none' : isPair ? 'pair' : 'single'`; `narrationOpen/Close` fall back to `'*'`; `unmarkedEnabled = unmarked !== null`; `markdownAsterisks = template.markdownAsterisks ?? true`); the save payload gains `unmarkedStyle: unmarkedFromForm(formData)` and `markdownAsterisks: formData.markdownAsterisks`; `appendFormattingHint` passes `{markdownAsterisks: prev.markdownAsterisks}` and `narrationFromForm(prev)`.
- `index.tsx` (+263/−): `STYLE_OPTIONS` gains FIRST `{value:'qt-roleplay-plain', label:'Plain (no chip — dress it with flourishes)'}`; NEW shared `AddOnsEditor` (the flourish checkboxes + three selects, lifted from the per-delimiter block); the Draft button's `disabled` = `delimiters.length === 0 && narrationDelimiterMode !== 'none' && !narrationOpen.trim()`; the narration help text becomes `How narration and action text is marked in this template's formatting — or, as in a proper novel, not marked at all.`; a THIRD radio `None — plain prose` whose `onChange` sets `narrationDelimiterMode:'none'` AND `unmarkedEnabled:true`; in `none` mode the input row shows `Narration carries no marks; style it under <em>Unmarked Text</em> below.`; a NEW `Unmarked Text` section (label `Unmarked Text`; help `Whatever no delimiter lays claim to. Give it a style of its own and it is dressed accordingly — italic narration, say, for a template whose narration wears no marks.`; checkbox `Style unmarked text`; when on: a `Style (CSS class)` text input with datalist `qt-style-options-unmarked`, placeholder `qt-roleplay-plain`, `maxLength=50`, + `AddOnsEditor`; then the checkbox `Markdown reads asterisks as bold and italic. Untick it to hand a single * to your own delimiters (an emphasis mark, perhaps); doubled ** then mean nothing at all and are quietly dropped.` with `*` and `**` in `<code>`); per `wrap` delimiter two checkboxes `Straight quotes also match curly ones (" and “…” alike)` and `May run unclosed to the end of a paragraph (speech carried over several)`; the Save `disabled` predicate becomes `saving || !name.trim() || !systemPrompt.trim() || (mode !== 'none' && (!narrationOpen.trim() || (mode === 'pair' && !narrationClose.trim())))`.
- `FormattingToolbar.tsx` (`@@ -39`, `-55`, `-482`): `RoleplayTemplateWithDelimiters.markdownAsterisks?: boolean`; `narrationDelimiters?: NarrationDelimiters | null`; `MARKDOWN_FORMATS.filter(f => !(f.type === 'bold' && template?.markdownAsterisks === false))`.
- `MessageContent.tsx` (`@@ -21`, `-158`, `-168`, `-179`, `-323`): React-side nesting + `unmarkedClassFor` — NO v5 counterpart (headline 1).
- `SalonView.tsx:204` / `ChatComposer.tsx:131`: the narration state / prop type widens to `| null`.
- `help/roleplay-templates.md` (+25) / `help/roleplay-templates-settings.md` (+38): P4.D360's bytes (vendor); the SPA copy above is the lane's.
- `public/schemas/qtap-export.schema.json` (`@@ -1197`, `-1220`): `TemplateDelimiter` + `matchCurlyQuotes` / `closeOptional` (booleans, `wrap only`); `narrationDelimiters.anyOf` + `{type:'null'}`; NEW `unmarkedStyle` (`anyOf` object `{style (1..50), addOns $ref DelimiterAddOns}` / null); NEW `markdownAsterisks` boolean. The v5 copy at `crates/quilltap-core/src/generators/qtap-export.schema.json` is P4.D360's byte copy.
- `docs/developer/DDL.md` (`@@ -1544`, `-1564`): the two column lines + the prose note (vendor).

### 1.2 `1dd0a0c01` — punctuation warnings (24 files, +1,356 / −11)

#### 1.2.1 Server: ONE key, THREE sites

- `lib/schemas/settings.types.ts:686-687`: `SmartTypographySettingsSchema` gains `punctuationWarnings: z.boolean().default(true)` (fourth key, declaration order ⇒ parse output order `displayQuotes, dashes, ellipsis, punctuationWarnings`).
- `settings.types.ts` `ChatSettingsSchema` (`@@ -854`): the object default `.default({displayQuotes:false, dashes:true, ellipsis:true, punctuationWarnings:true})`.
- `lib/database/repositories/chat-settings.repository.ts` (`@@ -236`, at the pin `:113-118` after `7789ebed6`'s refactor): the repository create default gains `punctuationWarnings: true`.
- The column DDL default is UNCHANGED (`DDL.md:1145`): `'{"displayQuotes":false,"dashes":true,"ellipsis":true}'` — the hunk only extends the comment.
- No route change: `app/api/v1/settings/chat/route.ts` already parses the bag through `SmartTypographySettingsSchema.parse` (`:13` import; the key rides the schema).

#### 1.2.2 `lib/punctuation-lint/engine.ts` (NEW, 344 lines at this commit; 472 at the pin after `2b2b190b6`) — **imports nothing**

At the pin: types `PunctuationRule` (nine identifiers incl. `'no-dialogue'`),
`PunctuationIssue {block, start, end, rule}`, `LintCaret {block, offset}`,
`PunctuationLintOptions {caret?, proseMode?}`, `ProseKind`, `ProseSpan`;
`PUNCTUATION_RULE_MESSAGES` (nine user-facing strings — the exact bytes are in
the pin file `lib/punctuation-lint/engine.ts:98-114`; copy, don't retype — e.g.
`'nested-double-quote': 'A double quote inside dialogue that is already open. Quoting someone inside speech? Use single quotes.'`, …,
`'no-dialogue': 'No dialogue yet — not a single opening quote in the whole draft. Meant to say this aloud? Put the speech in double quotes.'`);
the rule internals `OPENING_CONTEXT`, `CLOSING_CONTEXT`, `NEUTRAL_CONTEXT`,
`SCENE_BREAK = /^\s*(?:\*\s*){3,}$/`, `toParagraphs` (split on `/\n[ \t]*\n/g`;
a `null` block is a barrier), `quoteShape`, `lintQuotes` (returns `issues`,
`unclosedOpener`, `opensWithQuote`, `dialogue` runs), `lintAsterisks` (one stack
per run length; the `2 * 3` spaces-both-sides skip), `lintOoc`, `oocSpans`,
`classifyParagraph`, exported `classifyProse(blocks)` and
`lintPunctuation(blocks, options)` (sorted by block then offset; the prose-mode
`no-dialogue` pass has NO caret exemption). Behaviour is defined by
`fixtures/punctuation-vectors.json` (object `{description, cases[]}`, each
`{name, blocks, caret?, expect[]}`) and `fixtures/prose-vectors.json`
(`{description, lint[], classify[]}`) — both say "quilltap-v5 copies this
byte-for-byte".

#### 1.2.3 The client (adapter + settings + CSS)

- `components/chat/lexical/plugins/PunctuationLintPlugin.tsx` (329 → 476 lines at the pin): CSS Custom Highlight API adapter — `$collectBlocks` (one string per block element; inline `code` text masked to `'x'.repeat(n)`; a code block → `null`; line breaks → `\n`; inline elements recurse; decorators → `'￼'`; an empty paragraph still yields a block), `$caretFor`, `domPoint` / `rangeFor`, `paint(editor, warnings, proseMode)`, a module-level `contributions` registry so several composers share one `Highlight` per name, `publishRanges` sets `highlight.priority`, the hover explanation via the root's `title` attribute through `caretPositionFromPoint`. Reads `chatSettings?.smartTypographySettings?.punctuationWarnings ?? true` through `useChatSettingsQuery`. Three `console.debug('[punctuation-lint] …')` lines (§4).
- `LexicalComposerWrapper.tsx:181-183`: `<PunctuationLintPlugin />` mounted LAST ("Read-only: paints warnings, never edits, so its order is immaterial.").
- `components/settings/chat-settings/SmartTypographySettings.tsx` (`@@ -35`, `-164`): `const punctuationWarnings = current.punctuationWarnings ?? true`; a NEW `SettingsToggleRow` heading `Warn about punctuation that looks wrong` with the body (byte-exact, inline `qt-code-inline` spans around `"`, `*`, `((`, `))`): `While you type in the Salon composer, marks that will probably confuse the formatting are tinted and underlined: a " opened inside dialogue that is already open (quote someone with single quotes there), a closing quote with nothing to close, a quote stranded between spaces or jammed between words, dialogue that never closes, and an *, (( or )) with no partner. Hover a mark to see why. Nothing is ever corrected for you, and nothing you are still in the middle of typing is flagged.`; `onUpdate({punctuationWarnings: value})`.
- `components/settings/chat-settings/types.ts:192-193, 199`: `punctuationWarnings?: boolean`; `DEFAULT_SMART_TYPOGRAPHY_SETTINGS.punctuationWarnings = true`.
- `app/styles/qt-components/_chat.css` (`@@ -1035`): `::highlight(qt-punctuation-warning) { background-color: var(--qt-punctuation-warning-bg); text-decoration: underline wavy var(--qt-punctuation-warning-underline); text-decoration-thickness: 2px; }` inserted before `Lexical rich text node styles`.
- `_variables.css` (`@@ -99`): `--qt-punctuation-warning-bg: color-mix(in srgb, var(--color-warning) 35%, transparent); --qt-punctuation-warning-underline: var(--color-warning);` after `--qt-status-danger-border`.
- `eslint.config.mjs:124-129`: the `no-restricted-imports` fence now covers `lib/punctuation-lint/**/*.ts` ("the engine travels to v5, the Lexical adapter does not").
- `help/chat-settings-composer.md` (+13): vendor.

### 1.3 `2b2b190b6` — prose mode (21 files, +787 / −51)

- `engine.ts` (+128): `proseMode` option, the `no-dialogue` rule + message, `lintQuotes` returns `dialogue: [number, number][]`, NEW `oocSpans`, `classifyParagraph`, exported `classifyProse`; the prose-mode pass in `lintPunctuation` (quoted in 1.2.2 — the pin file is the union).
- NEW `lib/punctuation-lint/prose-mode.ts` (28 lines): `ProseTemplateShape {narrationDelimiters?: unknown; delimiters?: readonly {kind?, delimiters?}[] | null}`; `isProseTemplate(t) = !!t && t.narrationDelimiters === null && (t.delimiters ?? []).some(d => (d.kind ?? 'wrap') === 'wrap' && d.delimiters === '"')`.
- `PunctuationLintPlugin.tsx`: four highlight names + priorities `qt-punctuation-warning: 3, qt-punctuation-no-dialogue: 2, qt-composer-ooc: 1, qt-composer-narration: 0`; `paint` routes `no-dialogue` issues to the no-dialogue name (and lists them AFTER the specific ones for hover), and when `proseMode` adds `classifyProse` spans under the two preview names; the preview IGNORES the warnings toggle (`if ((!warnings && !proseMode) …)` is the only gate).
- Threading: `SalonView.tsx` `composerProseMode` state set to `isProseTemplate(template)` in the template fetch (and `false` on the three reset arms), passed as `proseMode` → `ChatComposer` (`@@ -75`, `-179`, `-599`) → `LexicalComposerWrapper` (`@@ -87`, `-109`, `-180`) → `<PunctuationLintPlugin proseMode={proseMode} />`.
- `_chat.css` (`@@ -1046`): `::highlight(qt-punctuation-no-dialogue) {background-color: var(--qt-punctuation-no-dialogue-bg); text-decoration: underline dotted var(--qt-punctuation-no-dialogue-underline); text-decoration-thickness: 2px;}`, `::highlight(qt-composer-narration) {color: var(--qt-composer-narration-fg); background-color: var(--qt-composer-narration-bg);}`, `::highlight(qt-composer-ooc) {color: var(--qt-composer-ooc-fg); background-color: var(--qt-composer-ooc-bg);}`.
- `_variables.css` (`@@ -102`): `--qt-punctuation-no-dialogue-bg: color-mix(in srgb, var(--color-warning) 14%, transparent); --qt-punctuation-no-dialogue-underline: var(--color-warning); --qt-composer-narration-fg: color-mix(in srgb, var(--color-foreground) 65%, var(--color-primary)); --qt-composer-narration-bg: color-mix(in srgb, var(--color-primary) 7%, transparent); --qt-composer-ooc-fg: var(--qt-rp-ooc-color); --qt-composer-ooc-bg: var(--qt-rp-ooc-bg);`.
- Tests: `__tests__/unit/lib/punctuation-lint/prose-mode.test.ts` (27), `PunctuationLintPlugin.test.tsx` (+63), `engine.test.ts` (+40) — transcribe case-for-case.
- `help/chat-settings-composer.md` (+7), `help/roleplay-templates.md` (+2): vendor.

### 1.4 `6588972d5` — diagrams, the CLIENT half (the SERVER half: §6.3; P4.D349)

#### 1.4.1 `lib/markdown/diagrams.ts` (NEW, 105 lines) — pure, no imports

`DIAGRAM_LANGUAGES = {mermaid:'mermaid', dot:'graphviz', graphviz:'graphviz', svg:'svg'}`;
`DIAGRAM_STYLES = ['handDrawn','clean']`; `DEFAULT_DIAGRAM_STYLE = 'handDrawn'`;
`diagramStyleOrDefault(v)` (lenient); `diagramEngineFor(language)` (trim + lower-case,
own-property lookup); `OPENING_FENCE = /^[\s>]*(`{3,}|~{3,})(.*)$/`;
`hasDiagramFence(markdown)` — line walker, a closed fence of a diagram language
returns true; an unterminated fence never counts; a backtick fence whose info
contains a backtick is skipped; the closing regex is
`^[\s>]*\<char>{<len>,}\s*$`.

#### 1.4.2 `components/chat/diagrams/*` at the pin (post-`8bf856050`)

- `types.ts`: `RenderedDiagram {svg: string}`.
- `render-queue.ts` (39): `enqueueRender(task)` (one promise chain; a failure never jams it), `nextRenderId(prefix)` = `` `${prefix}-${Date.now().toString(36)}-${serial}` ``, `seedFromSource(source)` (FNV-1a 32-bit, `(hash >>> 1) || 1`).
- `sanitize-svg.ts` (91): a PRIVATE `DOMPurify(window)` instance with an `uponSanitizeAttribute` hook (`href`/`xlink:href` kept only when they start with `#`; any attribute value with a non-`#` `url(...)` dropped); `sanitize(svg, {USE_PROFILES:{svg:true, svgFilters:true}, FORBID_TAGS:['script','foreignObject','style','image','iframe','object','embed'], ADD_TAGS:['use'], ALLOW_DATA_ATTR:false, RETURN_DOM_FRAGMENT:true})`; `sanitizeSvgElement` returns the root only when EXACTLY one element/non-blank text node survives and it is `svg` in the SVG namespace; `sanitizeSvg` serializes it; `looksLikeSvgRoot` strips BOM / `<?xml?>` / comments / DOCTYPE then tests `/^<svg[\s>]/i`.
- `diagram-theme.ts` (114): `DiagramPalette` (12 colours + `darkMode` + `fontFamily` + `handFontFamily`); `FALLBACK_LIGHT` hexes; `toHex` through a 1×1 canvas (`willReadFrequently`); `luminance`; `readDiagramPalette(host)` reads `--color-background/foreground/card/primary/primary-foreground/secondary/secondary-foreground/muted/muted-foreground/accent/accent-foreground/border`, `--qt-diagram-font` (default `ui-sans-serif, system-ui, sans-serif`), `--qt-diagram-hand-font` (default `"Segoe Print", "Bradley Hand", "Chalkboard SE", "Marker Felt", cursive`); `darkMode = luminance(background) < 0.5`.
- `render-mermaid.ts` (106): lazy `import('mermaid')`; `configFor` = `{startOnLoad:false, securityLevel:'strict', htmlLabels:false, flowchart:{htmlLabels:false}, theme:'base', look: handDrawn ? 'handDrawn' : 'classic', handDrawnSeed: seed, fontFamily, themeVariables:{darkMode, background: card, fontFamily, primaryColor: secondary, primaryTextColor: secondaryForeground, primaryBorderColor: primary, secondaryColor: accent, secondaryTextColor: accentForeground, secondaryBorderColor: border, tertiaryColor: muted, tertiaryTextColor: foreground, tertiaryBorderColor: border, mainBkg: secondary, nodeBorder: primary, clusterBkg: muted, clusterBorder: border, lineColor: mutedForeground, textColor: foreground, titleColor: foreground, edgeLabelBackground: card, noteBkgColor: accent, noteTextColor: accentForeground, noteBorderColor: border, actorBkg: secondary, actorBorder: primary, actorTextColor: secondaryForeground, signalColor: foreground, signalTextColor: foreground, labelBoxBkgColor: secondary, labelTextColor: foreground}}`; `renderMermaid` enqueues: `initialize(config)` → `parse(source)` (rejects with Mermaid's own message) → `render(id, source)` → in `finally` remove `#<id>` and `#d<id>` from `<body>`.
- `render-graphviz.ts` (117): lazy `import('@hpcc-js/wasm-graphviz')` → `Graphviz.load()`; `graphviz.dot(source, 'svg')` (throws Graphviz's message); `sanitizeSvgElement(raw)` (DOT `URL`/`href` become `<a xlink:href>`); `themeGraphvizDefaults` (black strokes/fills and unfilled `text` → `currentColor`; the first `g.graph > polygon` with a white fill → `none`); `sketch` via lazy `import('svg2roughjs')` on an offscreen stage appended INSIDE `host` (`position:absolute;left:-100000px;…`) with `new Svg2Roughjs(output, OutputType.SVG, {roughness:1})`, `seed`, `fontFamily = palette.handFontFamily`, `backgroundColor = null`; a sketch failure falls back to the clean markup with `console.debug('[diagrams] Hand-drawn Graphviz sketch failed; showing the clean drawing', {error})`.
- `render-svg.ts` (41): `looksLikeSvgRoot` else throws `'An svg block must start with an <svg> element'`; sanitize else `'Nothing drawable survived sanitizing this SVG'`; `fitToContainer` (guarantee a `viewBox` from numeric width/height; drop `width`/`height`; append `width:<w>px;` or `width:100%;` + `max-width:100%;height:auto;` to `style`).
- `DiagramBlock.tsx` (287): the dispatcher — `RENDERERS` map; a 64-entry LRU `renderCache` keyed `cacheKeyFor(engine, style, themeKey, source)` (`svg|<source>` for raw SVG; theme key = `${activeThemeId ?? 'default'}|${resolvedColorMode ?? 'light'}|${tokensId}`); the render runs in a `requestAnimationFrame` ("one frame's grace" after a theme switch); `BLOT_NOTE = 'The draughtsman’s pen has blotted — the diagram below could not be drawn.'`; the error state: `<div class="qt-diagram-error" data-diagram-engine>` with `<p class="qt-diagram-error-note" title={error}>` + `<details class="qt-diagram-error-details"><summary>What the {Graphviz|Mermaid|SVG} engine said</summary><pre>{error}</pre></details>` + the code block; the drawn state: `<div class="qt-diagram" data-diagram-engine data-diagram-style={engine==='svg' ? undefined : style}>` with a `role="toolbar" aria-label="Diagram actions"` of five `qt-diagram-toolbar-button`s — `View source`/`Hide source` (`aria-pressed`, icon `code`), `Enlarge` (icon `expand`, title `Enlarge — zoom, and copy as PNG`), `Copy source` / `Copied` (icons `copy`/`check`), `Copy SVG` / `Copied`, `Download SVG` (icon `download`, file `${engine}-diagram.svg`) — then the canvas `<div class="qt-diagram-canvas" role="button" tabIndex=0 aria-label="${kind} — open to zoom and copy" title="Click to enlarge">` (`kind` = `Drawing` for svg else `Diagram`; a click with a non-collapsed non-empty selection does NOT open; Enter / Space open), the placeholder `<div class="qt-diagram-placeholder" role="status" aria-live="polite">Sketching the diagram…</div>`, the optional source code block, and the offscreen `qt-diagram-stage`. `streaming` ⇒ render the code block only. Log line on failure (§4).
- `CodeBlockWithCopy.tsx` (99): the copy-button code block, LIFTED out of `MessageContent.tsx` unchanged (v5 has no component twin — its renderer emits `<pre><code>` and the copy button is CSS-only; see §2.4).
- `diagram-lightbox-host.tsx` (59): one lightbox at a time; its OWN React root on a `div.qt-diagram-lightbox-root` appended to `<body>` (outlives a virtualized row); `closeDiagramLightbox` removes the container, calls `restore()`, unmounts on a microtask; `openDiagramLightbox({svg, label, trigger})` captures the scroll anchor first.
- `scroll-anchor.ts` (65): `findScrollContainer`, `captureScrollAnchor(anchor)` → restores the anchor's offset in its container (or the container's `scrollTop` if the row was unmounted), `anchor.focus({preventScroll:true})`, re-applies on the next frame, `console.debug('[diagrams] Restored transcript position after lightbox', {anchored, savedTop})`.
- `lightbox-geometry.ts` (123, pure): `MIN_ZOOM 0.1`, `MAX_ZOOM 8`, `ZOOM_STEP 1.25`, `FIT_PADDING 24`, `naturalSvgSize` (absolute width/height in `px|pt|pc|in|cm|mm`, the `style` width, the viewBox, aspect fill-in, last resort 300×150), `clampScale`, `fitScale`, `centeredView`, `fitView`, `zoomTo(view, scale, focus)`, `wheelZoomFactor(deltaY, deltaMode, ctrlKey)` (`k = ctrlKey ? 0.01 : 0.0015`, `exp(-px*k)`).
- `export-png.ts` (428): `PNG_EXPORT_SCALE = 2`, `MAX_CANVAS_SIDE 16384`, `MAX_CANVAS_AREA 16_777_216`, `DiagramExportError(reason: 'tainted'|'too-large'|'load-failed')`, `parseFontFamilies`, `findFontFaces` (walks `document.styleSheets` incl. grouping rules; prefers `woff2`), `fontAsDataUrl` (cached per page; `console.debug('[diagrams] Could not embed a font for PNG export', {url, error})` on failure), `foreignObjectAsText` (centred `<text>`/`<tspan>` in the label's computed font/size/weight/style/colour), `hasSwitchFallback`, `replaceForeignObjects`, `safeExportScale`, `prepareSvgForRaster` (clone, replace FOs, size in device pixels, `xmlns`, copy the root's computed `color`/`font-family`/`font-size`, embed `@font-face` data URLs in a `<style>`, `console.debug('[diagrams] Prepared diagram for PNG export', {width, height, scale, replacedForeignObjects, embeddedFamilies})`), `rasterizePreparedSvg` (a `data:image/svg+xml;charset=utf-8,` URL — "the CSP's img-src admits data: only"; `img.decode()`; canvas fill with `background` unless `transparent`; `toBlob` null → `'too-large'`; a thrown SecurityError → `'tainted'`), `diagramToPngBlob`.
- `DiagramLightbox.tsx` (428): `role="dialog" aria-modal="true" aria-label={label} tabIndex=-1`; the bar `role="toolbar" aria-label="Diagram view"` with Zoom out (`−`, icon `zoom-out`, title `Zoom out (−)`), the live `{percent}%` (`aria-live="polite"`, `aria-label="Zoom N%"`), Zoom in (`zoom-in`, `Zoom in (+)`), `Fit` (icon `compress`, `aria-pressed={fitted}`, title `Fit the whole drawing to the window`), `100%` (title `Natural size (0)`, pressed when `!fitted && |scale−1| < 0.005`), the `Transparent` checkbox (title `Copy without a background`; remembered for the page's life), `Copy PNG` / `Copying…` / `Copied` (icon `image`/`check`, title `` `Copy as a PNG at ${PNG_EXPORT_SCALE}× resolution` ``), Close (icon `close`, title `Close (Esc)`, `aria-label="Close"`); the viewport (pointer drag pans; two pointers pinch about their midpoint and follow it; a non-moved press that began on the backdrop closes; double-click toggles Fit / 100%); wheel + Safari `gesturestart/change/end` bound by hand (`passive:false`); keys (captured on `window`, capture phase): Escape closes (`stopImmediatePropagation`), `+`/`=`, `-`/`_`, `0`, Tab trap; opens at Fit; `resize` re-fits while fitted; the sheet `qt-diagram-lightbox-sheet` (`data-transparent`) sized `natural × scale` at `(x, y)`; the hint `Wheel or pinch to zoom · drag to pan · Esc to return`; toasts: info `` `A large drawing — copied at ${scale.toFixed(1)}× rather than ${PNG_EXPORT_SCALE}×.` ``, errors `The browser would not let this drawing be copied as a picture.` / `This drawing is too large to copy as a picture.` / `The drawing could not be turned into a picture.` / `The picture could not be put on the clipboard.`; `console.error('Failed to copy diagram as PNG', {error})`.
- `lib/clipboard-utils.ts:111-133`: `copyPngBlobToClipboard(png: Promise<Blob>)` — `navigator.clipboard.write([new ClipboardItem({'image/png': png})])` (a PROMISED blob, Safari's gesture rule); on failure `await png` first (so a picture failure is reported as such), then the Electron bridge, else rethrow.

#### 1.4.3 Where the renderer draws (v4 client; v5 twins in §2.4 / §6.3)

- `MessageContent.tsx:74` `diagrams?: {style: DiagramStyle; streaming?: boolean}`; `:334-365` the `pre` override: `diagramEngine = diagramStyle ? diagramEngineFor(language) : null` → `<DiagramBlock engine language source={codeString.replace(/\n$/, '')} style streaming />` else `<CodeBlockWithCopy>`; the components memo deps gain `diagramStyle, diagramsStreaming` (`:483`). **Only the Salon passes `diagrams`** — every other surface keeps code blocks.
- `LazyMessageContent.tsx`: `diagramStyle?: DiagramStyle` (default `'clean'`); `hasDiagram = !!renderedHtml && hasDiagramFence(content)` routes OFF the pre-rendered fast path; passes `diagrams={{style: diagramStyle}}`; memo compares `diagramStyle`.
- `MessageRow.tsx:207-212`: `diagramStyle = useMemo(() => resolveMessageDiagramStyle(message, participantData))`; passed to the regenerating/settled `LazyMessageContent`s (`diagramStyle`) and to the regenerating `MessageContent` as `diagrams={{style, streaming:true}}`.
- `message-row/helpers.ts:14-28` `resolveMessageDiagramStyle(message, participants)`: `systemSender` ⇒ `'clean'`; no `participantId` ⇒ `'clean'`; else the participant's `character.diagramStyle` through `diagramStyleOrDefault`, or `'clean'` when the seat has no character.
- `StreamingMessage.tsx:16` `STREAMING_DIAGRAMS = {style:'clean', streaming:true}` on every streaming `MessageContent`.
- `useParticipants.ts:133` carries `diagramStyle: p.character.diagramStyle` field-by-field; `salon/[id]/types.ts:191` `CharacterData.diagramStyle?`; `ParticipantCard.tsx:46` `ParticipantData.character.diagramStyle?`.
- Aurora: `components/characters/CanDrawDiagramsToggle.tsx` (93): `qt-card` `data-testid="can-draw-diagrams"`; checkbox label `Draws diagrams`, hint (`id="can-draw-diagrams-hint"`) `This character may sketch flowcharts, family trees, maps and the like right into the conversation. The toggle only grants the pen — to have them reach for it often, say so in their prompt.`; a `<fieldset>` `Drawing style` (disabled while unchecked / saving / disabled, value KEPT) with radios `Hand-drawn — sketched in ink, a touch wobbly` and `Clean — ruled lines, as the engine drew them` (`name="diagram-style"`). `app/aurora/shared/save-diagram-settings.ts` (35): one `PUT /api/v1/characters/{id}` with `{canDrawDiagrams?}` or `{diagramStyle?}`; success toasts `The pen is theirs — this character may now draw diagrams` / `The pen is put away — this character will draw no more diagrams` / `Their diagrams will be drawn clean and ruled` / `Their diagrams will be sketched by hand`; failure `Failed to update diagram settings`. Mounted on the Details tab of BOTH `CharacterEditView.tsx:239-264` (under `CharacterBasicInfo`, "Saved on change, not with the form", optimistic + re-fetch on failure, `console.error('Failed to save diagram settings', {error})`) and `CharacterDetailView.tsx:225-249`.
- CSS: `_content.css` `@@ -656` (+120: `.qt-diagram`, `-toolbar`, `-toolbar-button` (+hover/pressed/focus-visible), `-canvas` (`overflow-x:auto`), `-canvas > svg`, `-placeholder` (`min-height: var(--qt-diagram-placeholder-height)`), `-stage`, `-error`, `-error-note`, `-error-details` (+summary, pre)) and `9faed0c7e` `@@ -776` (+145: `.qt-diagram-canvas[role="button"]` `cursor: zoom-in`, the `.qt-diagram-lightbox*` family: `position:fixed; inset:0; z-index:100; backdrop-filter: blur(4px)`, bar, group, button, zoom readout, check, viewport, sheet (+`[data-transparent]` checkerboard), hint); `_variables.css` `@@ -600` (+17: `--qt-diagram-radius/bg/fg/border/muted-fg/toolbar-fg/toolbar-hover-bg/error-fg`, `--qt-diagram-placeholder-height: 8rem`, `--qt-diagram-font: var(--font-sans)`, `--qt-diagram-hand-font: "Segoe Print", "Bradley Hand", "Chalkboard SE", "Marker Felt", cursive`) and `9faed0c7e` `@@ -616` (+11: `--qt-diagram-lightbox-backdrop/fg/muted-fg/button-bg/button-hover-bg/pressed-border` mapped to the `--qt-overlay-*` tokens, `--qt-diagram-lightbox-shadow: 0 10px 40px rgb(0 0 0 / 0.45)`, `-check-a: #ffffff`, `-check-b: #d4d4d8`).
- `package.json:60, 88, 94, 116`: `@hpcc-js/wasm-graphviz ^1.30.0`, `dompurify ^3.4.17`, `mermaid ^12.2.0`, `svg2roughjs ^3.2.3` (lockfile: 12.2.0 / 1.30.0 / 3.4.17 / 3.2.3).

### 1.5 `9faed0c7e` — the lightbox + Copy PNG (the files are described in 1.4.2; its own diff adds, in `DiagramBlock.tsx`: the `canvasRef`, `handleOpen` / `handleCanvasClick` / `handleCanvasKeyDown`, the `Enlarge` button, and turns the canvas from `role="img" aria-label="Diagram|Drawing"` into the `role="button"` above).

### 1.6 `8bf856050` — four `console.debug('[diagrams] …')` lines REMOVED: `Rendering diagram` (DiagramBlock), `Copying diagram as PNG` (lightbox), `Closed diagram lightbox` / `Opened diagram lightbox` (host). The pin keeps five `[diagrams]` debug lines (§4). Nothing for v5 to do beyond porting the pin's shape.

## 2. The v5 surface map (current shape at `f6cdceedd`)

### 2.1 The roleplay-template server

| v4 file | v5 counterpart | v5's CURRENT shape |
|---|---|---|
| `lib/schemas/template.types.ts` (Zod) | `crates/quilltap-core/src/db/roleplay_templates.rs` — `RenderingPattern :88-99` (`scope: Option<String>` — ALREADY carries any string; `class_name: String` — an empty string serializes fine), `DelimiterAddOns :122-137`, `StringOrPair :143-159`, `TemplateDelimiter :169-231` (`Wrap` has NO `matchCurlyQuotes` / `closeOptional` — serde would silently DROP them on round-trip, so the Novel seed's INSERT path would store a different byte string than v4's), `RtCreate :280-298` (`narration_delimiters: StringOrPair` — NOT nullable; no `unmarked_style` / `markdown_asterisks`), `RtUpdate :311-321` | KEYSTONE P4.D341 widens all of these (§7.1 contract). |
| `roleplay-template-narration-codec.ts` | `narration_storage` `:263-273` (bare `*` stored as plain TEXT, a pair as JSON) + `marshal_rt_row` `:593-597` (RAW passthrough) | KEYSTONE replaces with the JSON codec (encode on `create`/`update`/`seed_update` + `find_full_json_by_id`/`find_all_for_user` decode). |
| `roleplay-templates.repository.ts` seed | `services/builtin_templates.rs :38-63` (`narration_delimiters: StringOrPair`), `:90-107` (insert-or-update), `:146-166` (`to_seed_update` writes `narration_delimiters: self.narration_delimiters.storage()` — the bare form); `builtin_templates.json` — TWO rows (`Standard`, `Quilltap RP`), keys `name, description, systemPrompt, delimiters, renderingPatterns, dialogueDetection, narrationDelimiters` (no `unmarkedStyle` / `markdownAsterisks`) | KEYSTONE re-dumps via `harness/oracle/provision/dump-builtin-templates.ts` from the pin (Node 24) → three rows, encoded narration, the two new keys; `seed_update` `:508-526` gains the two columns. |
| `lib/chat/annotations.ts` | `services/annotations.rs` — `Scope :41-44` (`Inline`/`Line` only), `compose_class_name(&TemplateDelimiter) :132-136`, `build_wrap_pattern(prefix, suffix) :140-179` (no opts; the `]` link exclusion at `:171`), `build_delimiter_pattern :183-223`, `generate_rendering_patterns(delimiters, Option<&StringOrPair>) :228-278` | P4.D351: a `TemplateRenderingOptions`, the literal prepend, the unmarked append, `generate_template_rendering_patterns`, the quote-tolerant / close-optional wrap arm. Tier-1 exact against `harness/oracle/cases/annotations-rendering-patterns.ts` (`annotations_rendering_patterns_equivalence.rs`, `OracleRow.narration_delimiters: Option<StringOrPair>` `:25-29` — already nullable in the row type, good). |
| `lib/chat/template-prompt-hint.ts` | `template_prompt_hint.rs :68-134` (its own `TemplateDelimiter`/`NarrationDelimiters` enums; narration `Option<&NarrationDelimiters>` where `None` means "absent" — v4 now distinguishes `undefined` from `null`) | P4.D351: a three-state narration (`Absent` / `Null` / value) + `options.markdownAsterisks` + the `matchCurlyQuotes`/`closeOptional` fields on `Wrap`. Tier-1 against `cases/template-prompt-hint.ts` (`template_prompt_hint_equivalence.rs`). **Note:** no v5 dispatch verb calls this function (grep: the only consumer is the SPA's deferral note, `template-form-modal.ts:42-45`) — it is a verified leaf with no wire. |
| `app/api/v1/roleplay-templates/route.ts` + `[id]/route.ts` | `api/roleplay_templates.rs` — `apply_read_time_regen :65-92` (the `patterns_empty && (has_delims \|\| has_narration)` condition v4 dropped), create `:140-296` (narration required via `js_truthy` `:166-167` → `"Narration delimiters are required"`; regen guard `:210-213`; the 201 echo `:264-295` lacks the two keys), update `:301-448` (the two regen arms `:397-429`; `build_update_patch :507-556`) | P4.D351 rewrites per §1.1.10. The Zod issue text for `Invalid unmarked style: …` reuses `api/zod_issues.rs:1991 zod_error_message` / the `ZodIssue` helpers (`api/settings.rs:593-640`). |
| `lib/markdown/typography.ts` | **NO Rust counterpart** (the typographer lives in the SPA: `apps/web/src/app/chat/render/typography.ts:120-144`) | P4.D352. |
| `roleplay-rendering.ts` / `markdown-postprocess.ts` | **NO Rust counterpart** (v5 dropped `renderedHtml`; the port is `apps/web/src/app/chat/render/{roleplay-rendering,markdown-renderer}.ts`) | P4.D352. |
| the `.qtap` export / import / backup / restore of templates | `services/qtap_export/records.rs:438-461` + `preview.rs:108-117` + `entities.rs:89` + `mod.rs:279,444` (all read through `find_all_for_user` / `find_full_json_by_id` — decoded once the keystone lands); `qtap_export/schema-key-order.json:245-258` (`roleplay_template` key list — needs `unmarkedStyle`, `markdownAsterisks` after `narrationDelimiters`); `quilltap_import/entities.rs:315-330` (`narration_delimiters: StringOrPair` with `d_star` default `:327-330`; `:492-506` the create) ; `backup/collect.rs:257` `("narrationDelimiters", F::StrOpt)` (raw column — v4's backup reads raw rows too, so the JSON-encoded text is what BOTH carry after the seed rewrite; the two new columns must be added to the list: `unmarkedStyle` as the JSON-text/NULL class the file uses for `dialogueDetection`, `markdownAsterisks` as `F::Bool`/`F::BoolDefault` — check the enum at the top of `collect.rs`); `backup/restore/orchestrator.rs:1085-1110` (`narration_delimiters: de_opt(...).unwrap_or(Single("*"))` — must become the three-state value and carry the two new fields) | P4.D351 (the carriers), consuming the keystone's `RtCreate`. The v4 restore for a template goes through the repository (`toPersistedRow` encodes) — v4's archive JSON carries the DECODED value (`"*"` / `null`), so v5's restore must ENCODE on write (the keystone's `create` does it). |
| `lib/startup/prettify.ts` | none (v5 surfaces no migration labels — `status-log.md:3448`) | NO-PORT (standing). |
| `migrations/scripts/*` | a boot ensure (KEYSTONE; §3) | — |
| `lib/schemas/settings.types.ts` `SmartTypographySettingsSchema` | `db/chat_settings.rs:389-421` (`SmartTypographySettings` — THREE fields; `Default` `:413-421` three keys), the row struct `:963-968` (`#[serde(default)]`), create `:1148`, update `:1296-1300`, read `:1725-1731` (`parse_json_or_default(…, SMART_TYPOGRAPHY_DEFAULT_JSON)` — RAW; no per-key fill), the seed `:1936-1953` (`chat_settings_seed.json`), tests `:2745` (asserts the three-key bag); `db/chat_settings_composer_repair.rs:48-49` (`SMART_TYPOGRAPHY_DEFAULT_JSON`, three keys — doubles as the DDL default `:60-63`, pinned equal by `the_ddl_default_matches_the_json_const` `:166-200`); `api/settings.rs:699-720` (`zod_smart_typography_settings` — three `zod_bool`s), the PUT arm `:1159-1170`; the GET mint `:308-328` (`update_for_user_returning_id(…, &[], now)` → the seed row) | the `punctuationWarnings` key (P4.D351 as briefed, or P4.D356 — §7.2): a fourth `zod_bool(o, "punctuationWarnings", true, …)`; a fourth struct field `#[serde(default = "smart_typography_default_true")]`; a NEW four-key constant for the schema/repository default (the DDL constant stays three keys); the READ fills the absent key (v4's per-key default) — the `generateFromTools` precedent at `:445-495`; `chat_settings_seed.json` moves (keystone's dump). |

### 2.2 The composer SPA (P4.D352)

| v4 | v5 | current shape |
|---|---|---|
| `lib/punctuation-lint/engine.ts`, `prose-mode.ts`, `fixtures/*.json` | NEW `apps/web/src/app/punctuation-lint/{engine.ts, prose-mode.ts, engine.spec.ts, prose-mode.spec.ts, fixtures/punctuation-vectors.json, fixtures/prose-vectors.json}` — the `smart-typography/{engine.ts, engine.spec.ts, fixtures/typography-vectors.json}` layout (P4.D74 unit 1, `status-log.md:70649`) | nothing exists. |
| `PunctuationLintPlugin.tsx` | NEW `apps/web/src/app/editor/punctuation-lint-plugin.ts` (+ `.spec.ts`), composed in `editor/rich-editor.ts:302-400 buildPlugins()` next to `smartTypographyPlugin(() => this.smartTypography())` `:368`; a NEW `rich-editor.ts` input `punctuationLint = input<PunctuationLintInputs | null>(null)` read live (the `smartTypography` input idiom `:96-102`); the code guard reuses `editor/code-context.ts` (`code_block` node + `code` mark) | `rich-editor.ts` already carries a `DecorationSet` plugin (`placeholderPlugin :410-427`) — the ProseMirror-idiomatic adapter is **inline `Decoration`s** (§8.2 for the Custom-Highlight-API alternative). |
| `useChatSettingsQuery` → `punctuationWarnings` | `smart-typography/settings.ts` — `displayQuotes :44`, `typing :47`, `StoredBag :103-107`, `readDisplayQuotes :114-116`, `readTypingOptions :119-122` | add `punctuationWarnings = signal(readPunctuationWarnings(seed))` (default `true`) + the cache-event read (`:68-82`). |
| `SmartTypographySettings.tsx` | `screens/settings/chat/smart-typography-settings.ts` (card; `bag :128-130`, the three computeds `:133-144`, `onUpdate :162-172` merging `DEFAULT_SMART_TYPOGRAPHY_SETTINGS` — so the whole FOUR-key bag lands on PUT once the default grows) + `smart-typography-settings.spec.ts` | add the fourth toggle row with v4's copy. |
| `chat-settings/types.ts` | `screens/settings/chat/chat-settings.types.ts:284-297` | add `punctuationWarnings?: boolean` + the default — **shared with P4.D346** (`839575d89` adds `customToolIntervention` to the same v4 file; §7.4). |
| `SalonView.tsx` `composerProseMode` | `screens/salon/salon-conversation.ts` — the template effect `:1066-1101` (`roleplayDelimiters :1278`, `roleplayNarrationDelimiters :1279`, the reset `:1072-1078`), the composer bindings `:517-518` | add a `composerProseMode` signal set from `isProseTemplate(template)` in the SAME effect (+ `markdownAsterisks`), bind `[proseMode]` / `[markdownAsterisks]` on `<qt-chat-composer>`. **Shared file** (§7.4). |
| `ChatComposer.tsx` `proseMode` | `chat/chat-composer.ts` — `narrationDelimiters` input `:692` (already `NarrationDelimiters \| null`), toolbar binding `:285-294`, `<qt-rich-editor>` `:504-514` (`[smartTypography]="smartTypography()"` `:514`; `smartTypography = inject(SmartTypographySettings).typing` `:586`) | add inputs `proseMode = input(false)`, `markdownAsterisks = input(true)`; bind `[punctuationLint]` on the editor from `{enabled: settings.punctuationWarnings, proseMode}` (memoised in a `computed` — NG0100 note in `react-closure-reads-in-signal-ports`); bind `[markdownAsterisks]` on the toolbar. |
| `FormattingToolbar.tsx` Bold withhold | `editor/formatting-toolbar.ts` — `MARKDOWN_BUTTONS :48-60`, `buttons = MARKDOWN_BUTTONS :286`, inputs `:252-259` | add `markdownAsterisks = input(true)` and a `computed` that filters `type === 'bold'` when `false`. `documents/document-pane.ts:63,185` passes NO narration and no template → unaffected. |
| `lib/chat/roleplay-rendering.ts` | `chat/render/roleplay-rendering.ts` — `RenderingPattern.scope :22` (`'inline' \| 'line'`), `WireRenderingPattern :36-42`, `narrowRenderingPatterns :51-61` (**coerces non-`'line'` to `'inline'`** — headline 2), `Segment :103-106`, `CompiledRule :109-114`, `compileRenderingPatterns :120-127`, `tokenizeInline :133-171`, `segmentsToHtml :262-268`, `escapeMarkdownInBrackets :299-352` (`MARKDOWN_CHARS :271`) | port §1.1.4 whole; widen the wire type to the five scopes. |
| `lib/services/markdown-postprocess.ts` | `chat/render/markdown-renderer.ts` — `applyRoleplayPatterns :90-131` (with the v5-only `katexDepth` guard), `applyDialogueDetection :155-173`, `applyLineScopedClasses :177-202`, `applyWrapBlockClasses :204-230`, `renderMarkdownToHtml :297-357` | port §1.1.5 (the open-tag stack + `stripUnmarkedSpans`). |
| `lib/markdown/typography.ts` | `chat/render/typography.ts` — `STRAIGHT_QUOTES :74`, `CURLY_FORMS :77-80`, `isQuoteSensitiveRoleplayConfig :120-144` (`pattern.pattern.includes(quote)` `:126`), `shouldCurlQuotes :151-159` | add `mentionsGlyph` + the exemption. |
| `__fixtures__` + capture | `chat/render/__fixtures__/markdown-fixtures.json` (67 fixtures; `template-*` ×11, `quotes-*` ×16), `markdown-renderer.spec.ts` (replays; asserts ≥ 11 option-bearing rows `:49-56`), `typography.spec.ts` (hand-written `isQuoteSensitive` rows `:61-133`), `apps/web/tooling/capture-markdown-fixtures.mts` (drives v4's REAL `renderMarkdownToHtml` from `QT_V4_ROOT` `:45-52`) | add the Novel cases (§5.3) and re-capture at the pin. |
| `components/settings/roleplay-templates/*` | `screens/settings/templates/template-form.ts` (`NarrationForm :80-84` two modes; `narrationToForm :87-93`; `formToNarration :96-98`; `DelimiterFormEntry :35-55` no curly/close; `delimiterToFormEntry :104-131`; `formEntryToDelimiter :156-188`; `TemplateFormData :211-217`; `emptyTemplateForm :220-228`; `templateToForm :231-242`; `isTemplateFormValid :245-252`; `formToTemplateBody :255-270`), `template-form-modal.ts` (the two radios `:113-175`; signals `:216-218`; the body `:289-292`; the Draft-button DEFERRAL note `:42-45`), `template-delimiter-editor.ts` (`STYLE_OPTIONS :17-32` — no Plain; the `wrap` case `:142-187`; the flourishes block `:262-331` inline, not shared), `template-preview-modal.ts`, `roleplay-templates-card.ts` (built-ins list `:80-101` — Novel appears automatically), `templates.api.ts` (no change), + the four specs | port §1.1.11; extract the flourish editor into a component (v4's `AddOnsEditor`) so the Unmarked Text section can reuse it. |
| `components/settings/roleplay-templates/types.ts` (the DTO) | `core/core-contract.ts` — `RenderingPattern :5399-5405` (`scope?: string`), `NarrationDelimiters :5415`, `RoleplayTemplateDto :5418-5434` (`narrationDelimiters: NarrationDelimiters` `:5431`), `RoleplayTemplateCreateBag :5437-5446`, `RoleplayTemplateUpdateBag :5448-5457`, `TemplateDelimiter :5366-` (`wrap` arm), `TemplateDelimiterAddOns :5352` | a `// === P4.D352 … ===` fence: `narrationDelimiters: NarrationDelimiters \| null` on the DTO + both bags, `unmarkedStyle?: UnmarkedStyle \| null`, `markdownAsterisks?: boolean`, NEW `UnmarkedStyle {style: string; addOns?: TemplateDelimiterAddOns}`, `matchCurlyQuotes?` / `closeOptional?` on the `wrap` arm. |
| `_chat.css` / `_variables.css` | `apps/web/src/styles/qt-components/_chat.css:1043` (the `Lexical rich text node styles` anchor exists verbatim), `_variables.css:100-103` (`--qt-status-danger-border` / `ALERT TOKENS`), `:418-419` (`--qt-rp-ooc-color/-bg` exist) | append the six tokens after `:100` and the four rules before `:1043` — but as CLASS rules if the adapter uses decorations (§8.2). **Shared with P4.D357 / P4.D359** (§7.4). |

### 2.3 Diagrams (P4.D353) — v5 has NOTHING (grep `canDrawDiagrams|diagramStyle|DIAGRAM` over `apps/web/src` and `crates/quilltap-core/src`: zero hits)

| v4 | v5 home (all NEW unless noted) | notes |
|---|---|---|
| `lib/markdown/diagrams.ts` | `apps/web/src/app/chat/diagrams/diagram-fences.ts` | pure; the walker's v5 job is to list the closed diagram fences IN ORDER with their sources (§6.3). |
| `components/chat/diagrams/*.ts` (pure) | `chat/diagrams/{render-queue, sanitize-svg, diagram-theme, render-mermaid, render-graphviz, render-svg, lightbox-geometry, scroll-anchor, export-png}.ts` | near-verbatim TS; `diagram-theme.ts`'s theme key — v5 has no `ThemeContext`; the cache key uses the active theme id + colour mode from v5's theme service (locate: `grep -rn "activeThemeId" apps/web/src/app --include=*.ts -l`). |
| `DiagramBlock.tsx` | `chat/diagrams/diagram-block.ts` (an Angular component) + `chat/diagrams/diagram-slots.ts` (the HTML post-pass + hydration) | §6.3 for the mounting design. |
| `CodeBlockWithCopy.tsx` | **none** — v5's code blocks are the renderer's `<pre><code>` with the CSS copy affordance (`_content.css:1057-1105` `.qt-copy-button`, `.qt-code-block-container .qt-copy-button`); grep `qt-code-block-copy` in `apps/web/src/app` to find whether a copy button is wired at all (the streaming / error fallbacks render the SAME `<pre><code>` the renderer already emits) | the "degrade to the code block" arms keep the renderer's own code block — no component needed. |
| `diagram-lightbox-host.tsx` + `DiagramLightbox.tsx` | `chat/diagrams/diagram-lightbox.ts` (component) + `diagram-lightbox.service.ts` (one open at a time; body portal — the `images/full-screen-image-viewer.ts` reparent idiom and `ui/alert-modal.service.ts` precedent) | the shared frame (`full-screen-image-viewer.ts`) has no zoom/pan — a sibling, not a reuse. |
| `lib/clipboard-utils.ts` `copyPngBlobToClipboard` | `core/clipboard-utils.ts` (`copyImageToClipboard :81-96`; the Electron arm is a recorded non-goal `:15-21`) | add the promised-`ClipboardItem` function, browser arm only. |
| `MessageContent.tsx` `diagrams` prop | `chat/message-content.ts` (inputs `:29-51`; `html` computed `:71-80`; `[innerHTML]` `:21-25`) | add `diagrams = input<{style; streaming?} \| null>(null)` + the hydration (§6.3). **Shared with P4.D359** (`9a88f862f` client). |
| `LazyMessageContent.tsx` | none (no fast path in v5 — `message-content.ts:56-61` records it) | the `hasDiagram` gate has no twin; only the `diagramStyle` pass-through. |
| `MessageRow.tsx` + `message-row/helpers.ts` | `chat/message-row.ts` (`qt-message-content` ×4 `:197, 236-240, 263-271, 277-281`; inputs `:539-540`; `chat().participants` `:802-803`) + NEW `chat/diagrams/message-diagram-style.ts` (`resolveMessageDiagramStyle`) | **shared with P4.D358 (eed72d60c, 10e09b7c3 — one-line a11y hunks) and P4.D359 (9a88f862f)**. |
| `StreamingMessage.tsx` | `chat/streaming-message.ts` (`qt-message-content` `:92-99`; inputs `:165-184`) | `STREAMING_DIAGRAMS`. |
| `useParticipants.ts` / `salon/types.ts` / `ParticipantCard.tsx` (`diagramStyle` carried) | `core/core-contract.ts` `DetailCharacter :3362-` (the chat-participant character; `archivedAt :3375`) — add `diagramStyle?: 'handDrawn' \| 'clean'`; v5 has no field-by-field participant copy (the DTO is used as-is) | the wire field is P4.D349's (§6.3). |
| `CanDrawDiagramsToggle.tsx` + `save-diagram-settings.ts` | NEW `screens/characters/can-draw-diagrams-card.ts` modelled on `screens/characters/choose-outfit-card.ts` (`characterUpdate` dispatch + invalidate; toasts `:70-80`); mounted in `screens/characters/edit/character-edit.ts` (the outfit card at `:169-179`) and `view/character-detail.ts` (`:246`) | v4 mounts on the DETAILS tab (both views), v5's outfit card lives on the Wardrobe tab — the diagram card goes on Details as v4 does. |
| `CharacterEditView` / `CharacterDetailView` types | `core-contract.ts` `CharacterListItem :3608-` (`canChooseOutfit :3632`) and the character-page DTO (`canChooseOutfit :3692`, `archivedAt :3700`) | add `canDrawDiagrams?: boolean; diagramStyle?: 'handDrawn' \| 'clean'` in the P4.D353 fence. |
| `_content.css`, `_variables.css` | `apps/web/src/styles/qt-components/_content.css` (anchor `HIGHLIGHT.JS` `:671` — v4 inserts the DIAGRAMS block immediately before it), `_variables.css:585-588` (`--qt-code-font` / `LINK VARIABLES` — v4 inserts the diagram variables between them) and `:638` (`--qt-overlay-btn-hover-bg` exists for the lightbox mappings) | append-only blocks. |
| `package.json` | `apps/web/package.json` (`dependencies :17-52`; no mermaid/graphviz/dompurify/svg2roughjs) + `package-lock.json` | RULING (§10.1). |
| the `theme-storybook` mirror | NO-PORT (v5 has no `packages/theme-storybook` twin — the standing ruling, e.g. `2026-10-20-setupphase-interrupts-raise-hand-stop.md` §1.17) | — |
| `help/diagrams.md` (NEW 117 + 15), `help/character-editing.md`, `help/quick-hide.md`, `docs/developer/features/complete/diagrams.md` | P4.D360 (vendor) | — |

### 2.4 One more v5 fact P4.D353 must plan around

`chat/render/render-cache.ts:57-77` memoises `renderMarkdownToHtml` by
`content + ref-ids(renderingPatterns, dialogueDetection) + displayQuotes + imagesHidden`.
If the diagram slot pass runs INSIDE `renderMarkdownToHtml` the flag must join
the key (one more bit, the `imagesHidden` precedent `:74-77`); if it runs as a
post-pass in `message-content.ts` over the cached HTML (recommended, §6.3) the
cache is untouched.

## 3. Schema / D23 shapes (KEYSTONE's; recorded for completeness)

- **Migration DDL** (`add-roleplay-template-unmarked-fields-v1`): `ALTER TABLE roleplay_templates ADD COLUMN unmarkedStyle TEXT`; `ALTER TABLE roleplay_templates ADD COLUMN markdownAsterisks INTEGER DEFAULT 1` (via `addColumnIfMissing` — unquoted identifiers, as every v4 migration writes them).
- **generateDDL's fresh rendering at the pin** (`docs/developer/DDL.md:1534-1550`): `"unmarkedStyle" TEXT,` and `"markdownAsterisks" INTEGER DEFAULT 1` are the LAST two lines, AFTER `"updatedAt" TEXT NOT NULL` — i.e. generateDDL renders them appended (DDL.md is generated, not hand-edited, in v4's regime). The Zod schema places them BEFORE `createdAt`. **Two shapes, both appended** — the fresh rendering and the migration agree on position (last), so the re-dump should show them last; the keystone verifies against the LIVE `generateDDL` (`provisioning_equivalence` with `QT_FRESH_SCHEMA_LIVE`). `narrationDelimiters`' line is UNMOVED (`TEXT DEFAULT '"*"'`).
- Indexes: none new (`idx_roleplay_templates_createdAt` / `_userId` unchanged); `dependsOn: ['add-narration-delimiters-field-v1']`; prettify label `Setting aside a shelf for the unadorned novel`.
- **A data rewrite rides the SEED, not the migration:** on every boot the seeder `$set`s an ENCODED `narrationDelimiters` on the two legacy built-ins (`*` → `"*"`, `["[","]"]` → `["[","]"]` unchanged bytes) and INSERTs `Novel`. User templates are NOT rewritten until their next `update()` (which always writes the encoded form) — `decodeNarrationDelimiters` tolerates the legacy raw text forever.
- **Stamping recommendation:** the migration is column-gated (`shouldRun` = either column missing), so v4 would NOT re-run it on a v5-ensured instance — **do not stamp** (the ping-pong rule says stamp only what v4 would otherwise re-run); mirror it in `restore_vintage_state`'s boot-align list (memory note `vintage-restore-mirror-boot-list`).
- `chat_settings`: NO column change for `punctuationWarnings`; the `chat_settings_seed.json` row's `smartTypographySettings` cell becomes `{"displayQuotes":false,"dashes":true,"ellipsis":true,"punctuationWarnings":true}` (v4's repository default, `chat-settings.repository.ts:113-118` at the pin) — a SEED-ONLY move the keystone's dump carries; append to the register in `provisioning/mod.rs:254-275`.

## 4. Log lines (v4 text + keys + level → v5 home)

| commit | level | text | keys | v5 home |
|---|---|---|---|---|
| `778b97b2b` | INFO | `Added unmarked-text columns to roleplay_templates table` | `context: 'migration.add-roleplay-template-unmarked-fields', columnsAdded, durationMs` | KEYSTONE's boot ensure (`db/chat_settings_4_11_repair.rs:109-120` `tracing::info!` precedent) — capture-pin |
| `778b97b2b` | ERROR | `Failed to add unmarked-text columns to roleplay_templates` | `context, error` | same |
| `778b97b2b` | DEBUG | `Roleplay template patterns resolved for create` | `delimiterCount, patternCount, unmarkedNarration, markdownAsterisks` | NEW in `api/roleplay_templates.rs` create (after regen, before the write) — capture-pin in `roleplay_templates_routes_equivalence` |
| `778b97b2b` | DEBUG | `Roleplay template patterns regenerated for update` | `templateId, patternCount` | NEW in the update arm (only when `derivesPatterns && !renderingPatterns`) |
| `778b97b2b` (pre-existing, observed) | INFO | `Built-in roleplay template seeded` | `templateId, name` | v5's `builtin_templates.rs` logs NOTHING today (grep) — a pre-existing gap, record it; the keystone may close it while in the file |
| `1dd0a0c01` | console.debug | `[punctuation-lint] CSS Custom Highlight API unavailable; warnings disabled` | — | NEW `editor/punctuation-lint-plugin.ts` (only if the Custom-Highlight adapter is chosen; moot with decorations) |
| `1dd0a0c01` | console.debug | `[punctuation-lint] mounted` | `{warnings, proseMode}` | NEW plugin (on install / input change) |
| `1dd0a0c01` | console.debug | `` `[punctuation-lint] ${n} warning(s)` `` | — | NEW plugin — "one line when the number of warnings changes — never per keystroke" |
| `6588972d5` (pin) | console.debug | `[diagrams] Diagram could not be drawn` | `{engine, error}` | NEW `diagram-block.ts` |
| `6588972d5` (pin) | console.debug | `[diagrams] Hand-drawn Graphviz sketch failed; showing the clean drawing` | `{error}` | NEW `render-graphviz.ts` |
| `6588972d5` | console.error | `Failed to copy diagram source` / `Failed to copy diagram SVG` / `Failed to copy code block` | `{error}` | NEW `diagram-block.ts` (the third is `CodeBlockWithCopy`'s — no twin) |
| `6588972d5` | console.error | `Failed to save diagram settings` | `{error}` | NEW `can-draw-diagrams-card.ts` |
| `6588972d5` (server) | DEBUG | `[VaultOverlay] Writing diagram settings to properties.json` | (see P4.D349's survey) | P4.D349 |
| `9faed0c7e` | console.debug | `[diagrams] Restored transcript position after lightbox` | `{anchored, savedTop}` | NEW `scroll-anchor.ts` |
| `9faed0c7e` | console.debug | `[diagrams] Could not embed a font for PNG export` | `{url, error}` | NEW `export-png.ts` |
| `9faed0c7e` | console.debug | `[diagrams] Prepared diagram for PNG export` | `{width, height, scale, replacedForeignObjects, embeddedFamilies}` | NEW `export-png.ts` |
| `9faed0c7e` | console.error | `Failed to copy diagram as PNG` | `{error}` | NEW `diagram-lightbox.ts` |
| `8bf856050` | — | REMOVED `[diagrams] Rendering diagram`, `[diagrams] Copying diagram as PNG`, `[diagrams] Opened diagram lightbox`, `[diagrams] Closed diagram lightbox` | — | do not port |

## 5. Harness / oracle impact

### 5.1 Families that exercise the touched v4 code (none IMPORT-DEAD at the pin — verified by name)

| family (`crates/quilltap-harness/tests`) | oracle case | effect at the pin | who |
|---|---|---|---|
| `annotations_rendering_patterns_equivalence.rs` (`QT_ORACLE_ANNOTATIONS`) | `cases/annotations-rendering-patterns.ts` (imports `generateRenderingPatterns` — still exported) | REDDENS only once the corpus grows: today's rows pass unchanged (no options → no new patterns). Grow it: `null` narration, `unmarkedStyle` with/without addOns, `markdownAsterisks:false`, a `"` wrap with `matchCurlyQuotes`, with `closeOptional`, both, a `]`-suffixed wrap with `closeOptional` (the link-exclusion drop), a `'` wrap curly-tolerant, the Novel delimiter set whole. Add `generateTemplateRenderingPatterns` rows. | P4.D351 |
| `template_prompt_hint_equivalence.rs` (`QT_ORACLE_TEMPLATE_PROMPT_HINT`) | `cases/template-prompt-hint.ts` (2-arg call — still valid) | grows: `null` narration, the two note tails (singly and both), `{markdownAsterisks:false}`; the row shape must distinguish `undefined` from `null` (`narrationDelimiters: null` vs key absent). | P4.D351 |
| `roleplay_templates_routes_equivalence.rs` (`QT_ORACLE_ROLEPLAY_ROUTES`) over the committed groups-projects fixture (`crates/quilltap-web/tests/fixtures`; `harness/oracle/fixtures/groups-projects.json`) | `cases/roleplay-templates-routes.test.ts` rows `:189-369` (`list`, `create_happy`, `create_narration_array`, `create_name_required`, `create_sysprompt_required`, `create_narration_required`, `create_dup`, `get_user`, `get_regen`, `get_builtin`, `get_missing`, `update_happy`, `update_builtin_403`, `update_dup`, `update_regen_delims`, `update_regen_narration`, `update_null_description`, `update_missing_required`, `delete_happy`, `delete_builtin_403`, `delete_missing`) | REDDENS: `create_narration_required`'s 400 text changes; `list`/`get_*` echo the new keys + the DECODED narration (the fixture's built-in rows carry the legacy raw `*` — decoded identically; good); `get_regen` regenerates under the new unconditional rule; `create_*` echoes carry `unmarkedStyle: null, markdownAsterisks: true`. The committed fixture DB has NO `unmarkedStyle`/`markdownAsterisks` columns — the harness's `fresh_db` must run the boot ensure (or the fixture pair is REBUILT; `rebuilt-fixture-pair-vintage` note). Add rows: `create_null_narration` (201), `create_narration_empty_string` (400 new text), `create_unmarked_style_bad` (`{style:''}` → `Invalid unmarked style: Too small: …` — capture the exact Zod 4 text), `create_markdown_asterisks_not_bool` (400), `create_novel_like` (the full Novel bag — the regen bytes), `update_regen_unmarked_only`, `update_regen_markdown_only`, `update_narration_null`, `get_builtin_novel` (needs the seeder to have run on the fixture copy — it does on boot). | P4.D351 (+ the keystone for the columns) |
| `roleplay_templates_tier2_equivalence.rs` (`QT_ORACLE_ROLEPLAY_TEMPLATES` + `QT_FIXTURE_ROLEPLAY_TEMPLATES`) | `cases/roleplay-templates-tier2.ts` + `fixtures/build-roleplay-templates-fixture.ts` ("narrationDelimiters held to a plain string across the corpus") | REDDENS at the data layer: v4's `create` now stores `"*"` where v5 stores `*`; the fixture builder must be re-run at the pin (its `roleplay_templates` DDL gains the two columns) and the spec grows `null` narration + the two fields. | KEYSTONE |
| `builtin_templates_equivalence.rs` (`QT_ORACLE_BUILTIN_TEMPLATES`, three `QT_STATE`s) | `cases/builtin-templates.ts` + `provision/dump-builtin-templates.ts` | REDDENS on all three states (third row; encoded narration; two new `$set` columns). | KEYSTONE |
| `provisioning_equivalence.rs` | `provision/dump-fresh-schema.ts` (`QT_SEED_OUT`) | REDDENS: the roleplay_templates DDL (2 lines) + the seed row's `smartTypographySettings` cell + the keystone's other moves. | KEYSTONE |
| `settings_routes_equivalence.rs` (`QT_ORACLE_SETTINGS_ROUTES`) | `cases/settings-routes.test.ts` rows `:551-648` (`smartTypographySettings` full / partial / `{}` / `null` / `{dashes:'yes'}` / `'on'` / the D73 bank) | REDDENS: every echo carries `punctuationWarnings: true` (the partial-bag materialization) and the GET of a stored three-key row reads four keys. Add: `{punctuationWarnings:false}` alone, `{punctuationWarnings:'no'}` (one Zod issue). | P4.D351 / P4.D356 |
| `chat_settings_tier2_equivalence.rs` (`:137`, `:208` `SmartTypographySettings`) | `cases/chat-settings-tier2.ts` (grep) | REDDENS where the spec's create/update rows compare the stored bag (v4 now stores four keys on create). | same |
| `system_restore_state.rs`, `backup_uuid_remap_equivalence.rs`, `characters_create/slim_tier2`, `chat_create_capstone`, `create_wrap_lines`, `migration_index_backfill`, `participant_resolver_tier2` (reads `find_system_prompt_by_id` only) | `system-export/import/import-execute/restore.test.ts`, … | MOSTLY GREEN (they read templates by id or compare whole dumps whose template rows the keystone's columns widen); the export / import / restore families need rows with `narrationDelimiters: null` + `unmarkedStyle` to prove the carriers. | P4.D351 (rows), keystone (widening) |
| `markdown-fixtures.json` (vitest, not cargo) | `apps/web/tooling/capture-markdown-fixtures.mts` | the existing 67 rows should be byte-stable (verify: re-capture at the pin and diff — the nesting change alters output ONLY where two inline rules overlap; the `template-custom-patterns-*` rows have single rules). | P4.D352 |

**IMPORT-DEAD census for this area:** `cases/annotations-rendering-patterns.ts`,
`cases/template-prompt-hint.ts`, `cases/builtin-templates.ts`,
`cases/roleplay-templates-tier2.ts`, `cases/roleplay-templates-routes.test.ts`,
`cases/settings-routes.test.ts`, `provision/dump-builtin-templates.ts` — every
import they make still resolves at the pin (checked the exports of
`lib/chat/annotations.ts`, `lib/chat/template-prompt-hint.ts`,
`lib/database/repositories/roleplay-templates.repository.ts`,
`lib/schemas/template.types.ts`). None GARBLE: the only positional change
(`generateFormattingPromptHint`'s third `options` argument) is additive.

### 5.2 The punctuation engine's recipe (new family, SPA side)

- The engine `imports nothing` (verified: the pin's `engine.ts` has no `import`
  line; `prose-mode.ts` likewise), so a plain `npx tsx` leaf case from the v4
  checkout can `import { lintPunctuation, classifyProse } from '@/lib/punctuation-lint/engine'`
  and `isProseTemplate` from `prose-mode` — exactly the `template-prompt-hint.ts`
  leaf shape. Record `apps/web/src/app/punctuation-lint/engine.v4.json`
  (the `concierge-state.oracle.spec.ts` + `harness/oracle/cases/concierge-chat-override.mjs`
  pattern, P4.D229) over: every row of both vector files (sanity — they must
  equal the fixtures' `expect`), PLUS a generated corpus (every rule's edge:
  caret at each offset of a short draft, curly/straight mixes, `6"`, `2 * 3`,
  `* * *`, nested `((`, a `null` block barrier, blank-line paragraph splits,
  prose mode with/without dialogue, `classifyProse` on each).
- The floor is the byte-copy of `punctuation-vectors.json` + `prose-vectors.json`
  (the `typography-vectors.json` rule: "copied from v4 BYTE-FOR-BYTE … if the port
  disagrees with a row, the port is what gets fixed").
- Closest existing recipes, by name: **the typography family**
  (`smart-typography/engine.spec.ts` ↔ `fixtures/typography-vectors.json`, P4.D74
  unit 1) for the corpus copy; **the renderer vectors family**
  (`chat/render/markdown-renderer.spec.ts` ↔ `__fixtures__/markdown-fixtures.json`
  ↔ `tooling/capture-markdown-fixtures.mts`) for the executed-v4 capture.
  Neither is in the cargo sweep (`harness/tools/recipe_sweep.py`); both are SPA
  vitest gates.

### 5.3 The renderer vectors to add (P4.D352; captured from v4's REAL renderer at the pin)

Each with its `-reset` twin where the P4.30 convention applies:
`template-novel-dialogue-straight`, `-dialogue-curly`, `-dialogue-mixed`,
`-dialogue-unclosed-paragraph` (closeOptional), `-emphasis-inside-dialogue`
(nesting → `children`), `-double-asterisks-stripped`, `-single-asterisk-literal`,
`-scene-break-kept` (`* * *` → `<hr>`), `-bullet-kept` (`* item`),
`-inline-code-untouched`, `-math-untouched`, `-ooc`, `-unmarked-narration-italic`
(the `qt-roleplay-plain qt-rp-italic` span on direct `<p>` text, NOT inside the
dialogue span), `-unmarked-in-li-and-blockquote`, `-line-rule-strips-unmarked`
(a `linePrefix` rule under an unmarked template → `stripUnmarkedSpans`),
`-wrap-block-hidden-under-unmarked`, and the typography pair
`quotes-on-novel-dialogue` (displayQuotes ON + Novel → curled, because
`mentionsGlyph` exempts the pattern) / `quotes-on-novel-dialogue-reset`. Use the
SEEDED Novel patterns (take them from the keystone's `builtin_templates.json`,
never hand-derived).

## 6. SPA: components, specs, wire

### 6.1 Component map (P4.D352)

| v4 component | v5 file | spec(s) |
|---|---|---|
| `PunctuationLintPlugin` | `editor/punctuation-lint-plugin.ts` | NEW `punctuation-lint-plugin.spec.ts` (transcribe `PunctuationLintPlugin.test.tsx` 139+63 lines case-for-case: paints on load, repaints per update, caret exemption, code block masked, inline code masked, the setting off → nothing, prose mode's four names/priorities, hover explanation, unmount clears) |
| `SmartTypographySettings` row | `screens/settings/chat/smart-typography-settings.ts` | `smart-typography-settings.spec.ts` (+ the toggle row; `onUpdate` sends the four-key bag) |
| `useChatSettingsQuery` read | `smart-typography/settings.ts` | `settings.spec.ts` |
| `RoleplayTemplatesTab` modal | `screens/settings/templates/template-form-modal.ts`, `template-form.ts`, `template-delimiter-editor.ts`, NEW `template-add-ons-editor.ts` | `template-form.spec.ts` (narration `none` ↔ `null`; `unmarkedFromForm`; `matchCurlyQuotes` emitted only when true), `template-form-modal.spec.ts`, `roleplay-templates-card.spec.ts` (Novel listed as built-in) |
| `FormattingToolbar` | `editor/formatting-toolbar.ts` | `formatting-toolbar.spec.ts` (Bold withheld) |
| `MessageContent` / server renderer | `chat/render/{roleplay-rendering,markdown-renderer,typography}.ts` | `markdown-renderer.spec.ts` (the captured vectors), `typography.spec.ts` (`mentionsGlyph`) |
| `SalonView` threading | `screens/salon/salon-conversation.ts`, `chat/chat-composer.ts` | `salon-conversation.spec.ts` (prose mode follows the template; a Novel template flips it, a deleted template resets), `chat-composer.spec.ts` |
| Playwright | `e2e/salon-roleplay-template-flow.spec.ts` (`:46` P4.30, `:186` the p4.9l narration-button beat — extend with a Novel chat: unmarked narration italic, `**` gone, dialogue upright, Bold absent from the toolbar), `e2e/smart-typography-flow.spec.ts` (`:30` — add "a nested quote lights up and the hover explains it" + "the toggle off paints nothing"), `e2e/settings-chat-cards-flow.spec.ts` (`:87` — the new toggle persists) | — |

### 6.2 Component map (P4.D353)

| v4 | v5 | spec |
|---|---|---|
| `lib/markdown/diagrams.ts` | `chat/diagrams/diagram-fences.ts` | transcribe `__tests__/unit/lib/markdown/diagrams.test.ts` (112 lines) |
| `sanitize-svg.ts` | same name | transcribe `sanitize-svg.test.ts` (97) — DOMPurify runs under jsdom |
| `render-svg.ts` | same | `render-svg.test.ts` (30) |
| `DiagramBlock` | `diagram-block.ts` | transcribe `DiagramBlock.test.tsx` (160+37) with the renderers MOCKED (v4 does); placeholder / error / toolbar / cache / streaming / click-opens / selection-does-not |
| `DiagramLightbox` + host | `diagram-lightbox.ts` + `diagram-lightbox.service.ts` | `DiagramLightbox.test.tsx` (113), `lightbox-geometry.test.ts` (83), `scroll-anchor.test.ts` (84), `export-png.test.ts` (158 — canvas is stubbed in v4's jsdom suite; copy the stubs) |
| `CanDrawDiagramsToggle` | `screens/characters/can-draw-diagrams-card.ts` | NEW spec (`choose-outfit-card.spec.ts` as the model) |
| `MessageRow` / helpers | `chat/message-row.ts` + `chat/diagrams/message-diagram-style.ts` | transcribe `message-row-diagram-style.test.ts` (37): systemSender → clean, no participant → clean, a user-controlled character's style, absent style → handDrawn |
| `LazyMessageContent.diagrams.test.tsx` (50) | `message-content.spec.ts` (new) | only the "Salon passes diagrams, others don't" arm survives |
| Playwright | NEW `e2e/diagrams-flow.spec.ts` on its own port (the P4.D330 pattern): (1) a seeded assistant message with a closed ```` ```mermaid ```` `graph TD; A-->B` renders an `svg` inside `.qt-diagram-canvas` (real mermaid in Chromium), (2) a ```` ```dot ```` renders (real wasm Graphviz; `data-diagram-style="handDrawn"` for the character), (3) a bad fence shows the blot note + `<details>` + the code block, (4) click → `role="dialog"` lightbox, `+` zooms the readout, Escape returns and the row's `scrollTop` is unchanged, (5) Copy PNG with `context.grantPermissions(['clipboard-write'])` → `Copied`, (6) streaming: while the mock LLM streams a fence it is a code block, after `done` the drawing appears, (7) Aurora: the Details-tab card toggles, the style radios disable while off, reload persists (needs P4.D349 — gate on `P4D349_SERVER_LANDED`), (8) a non-Salon surface (Brahma console) keeps the code block. 💸 none — the mock LLM. |

### 6.3 The wire contracts

**(a) `punctuationWarnings`** — `chat_settings.smartTypographySettings` JSON key
`punctuationWarnings: boolean`, default `true`, FOURTH in the bag's key order;
PUT accepts a partial bag and materializes it; GET always carries it. SPA:
`chat-settings.types.ts` `SmartTypographySettings.punctuationWarnings?: boolean`
+ `DEFAULT_SMART_TYPOGRAPHY_SETTINGS.punctuationWarnings: true`. Built by the
server lane, consumed by P4.D352 — the SPA reads `?? true` so it is correct on an
old server too.

**(b) the template fields** — on `RoleplayTemplateDto` (list / get / 201 / PUT
echo), both bags, the `.qtap` record and the backup row:
`narrationDelimiters: string | [string, string] | null` (the DECODED value on
every read; `null` is a JSON null, not an absent key),
`unmarkedStyle: {style: string; addOns?: DelimiterAddOns} | null` (absent key on
route reads when SQL NULL? — **measure**: v4's `.nullable().optional()` + the
SQLite hydrate of NULL → `undefined` → the key is OMITTED on reads but PRESENT as
`null` on the create/update echoes, exactly the `description` / `dialogueDetection`
asymmetry `core-contract.ts:5422-5430` already documents; the routes family pins
it), `markdownAsterisks: boolean` (always present; `INTEGER DEFAULT 1` reads
`true`), and on a `wrap` delimiter `matchCurlyQuotes?: true`, `closeOptional?: true`
(present only when set — v4's form emits them only when ticked, the schema
`.optional()` omits them). `renderingPatterns[].scope ∈ {inline, line, unmarked,
literal, strip}` and `className` may be `''` for the last two. Rust names (§7.1).

**(c) the diagram gate (P4.D349 builds, P4.D353 consumes)** — vault
`properties.json` keys `canDrawDiagrams: boolean` (absent → `false`) and
`diagramStyle: 'handDrawn' | 'clean'` (absent/unknown → `'handDrawn'`); on the
wire: the characters LIST DTO carries both (`canDrawDiagrams: character.canDrawDiagrams ?? false`,
`diagramStyle: diagramStyleOrDefault(…)`, placed after `canChooseOutfit` — v4
`get.ts:80-81`); the character GET-by-id (overlay) carries both via the schema
defaults; the chat GET's participant `character` and the chat-enrichment detail
carry `diagramStyle` ONLY, placed after `archivedAt` (`helpers.ts:69`,
`chat-enrichment.service.ts:346, 375`); the character PUT accepts
`canDrawDiagrams?: boolean` and `diagramStyle?: 'handDrawn'|'clean'` (a bad
`diagramStyle` is a Zod 400). v5 homes P4.D349 edits: `vault_overlay.rs`
(reader + the six-key writers — the LIVE-DATA erasure), `services/character_enrichment.rs:126-130`
(the list DTO, `canChooseOutfit` idiom) and `:177-178` (`archivedAt` on the
participant — `diagramStyle` goes right after), `api/characters.rs:2225-2255`
(the PUT whitelist), `db/characters_read.rs:236`, `system_prompt.rs:528`. **The
SPA's consumption is defensive** (`?? 'handDrawn'`, `?? false`) so P4.D353 can
land first; the Aurora card's e2e beat is gated on P4.D349.

**(d) the slot contract inside P4.D353 (the design the lane should take):**
`renderMarkdownToHtml` stays UNTOUCHED (it belongs to P4.D352 this round). In
`message-content.ts`, when `diagrams` is set and `!diagrams.streaming`, run
`listClosedDiagramFences(content)` (the ported walker returning
`{engine, language, source}[]` in document order); if non-empty, walk the
rendered HTML's `<pre><code class="…language-<lang>…">` elements in order,
match the Nth diagram-language block to the Nth listed fence (both walks skip
fences nested inside other fences — a nested one is text inside a `<pre>`, not
a `<pre>` of its own; rehype-highlight keeps the `language-<lang>` class on the
`<code>` and may add `hljs` + token spans for `dot`, which is why the SOURCE
comes from the walker, never from the HTML), and replace the `<pre>` with
`<div class="qt-diagram-slot" data-slot-index="N"></div>`; then, in an
`afterRenderEffect`, mount a `DiagramBlock` component per slot (
`ViewContainerRef.createComponent` + `slot.replaceWith(ref.location.nativeElement)`,
the body-reparent idiom of `full-screen-image-viewer.ts`), destroying them when
the content/options change. The `hasDiagramFence` closed-fence rule is what
keeps a still-streaming fence as code (the streaming message also passes
`streaming: true`, as v4 does). Measure the alternative (a remark plugin via
`data.hName`/`hProperties` on the mdast `code` node inside `renderMarkdownToHtml`,
gated by an option and joining the render-cache key) only if the post-pass
proves fragile — it costs a shared file.

## 7. The lane split, by FILE OWNERSHIP

### 7.1 Confirmed with amendments

**KEYSTONE P4.D341 (wave 0) owns, for this area:** `crates/quilltap-core/src/db/roleplay_templates.rs`
(whole), `services/builtin_templates.rs` + `builtin_templates.json`,
`services/provisioning/fresh_schema.json` + `chat_settings_seed.json` + the
register in `provisioning/mod.rs`, the boot ensure (a new `db/roleplay_templates_unmarked_repair.rs`
or the keystone's ensure module) + `restore_vintage_state.rs`'s boot-align,
`harness/oracle/cases/builtin-templates.ts`, `cases/roleplay-templates-tier2.ts`,
`fixtures/build-roleplay-templates-fixture.ts`, `provision/dump-builtin-templates.ts`
(re-run), the two tier-2 families. **Named shared contract `RoleplayTemplateColumns-v2`** (keystone builds, P4.D351 consumes):

```rust
// db/roleplay_templates.rs
pub enum TemplateDelimiter { Wrap { name, button_name, style, hide_delimiter: Option<bool>,
    add_ons: Option<DelimiterAddOns>, delimiters: StringOrPair,
    #[serde(rename="matchCurlyQuotes", skip_serializing_if="Option::is_none", default)] match_curly_quotes: Option<bool>,
    #[serde(rename="closeOptional", skip_serializing_if="Option::is_none", default)] close_optional: Option<bool> }, LinePrefix{..}, TagPrefix{..} }
#[derive(Serialize, Deserialize)] #[serde(rename_all="camelCase")]
pub struct UnmarkedStyle { pub style: String, #[serde(skip_serializing_if="Option::is_none", default)] pub add_ons: Option<DelimiterAddOns> }
pub struct RtCreate { …, pub narration_delimiters: Option<StringOrPair> /* None = unmarked prose */,
    pub unmarked_style: Option<UnmarkedStyle>, pub markdown_asterisks: bool }
pub struct RtUpdate { …, pub narration_delimiters: Option<Option<StringOrPair>> /* Some(None) = set null */,
    pub unmarked_style: Option<Option<UnmarkedStyle>>, pub markdown_asterisks: Option<bool>, .. }
pub fn encode_narration_delimiters(v: Option<&StringOrPair>) -> String;      // JSON text
pub fn decode_narration_delimiters(stored: Option<&str>) -> Option<StringOrPair>; // v4 codec, legacy-tolerant
// marshal_rt_row: "narrationDelimiters" decoded (JSON null when None), "unmarkedStyle" OMITTED on SQL NULL,
// "markdownAsterisks" Bool (NULL cell → true); RT_COLUMNS gains the two columns LAST.
pub struct SeedUpdate { …, pub narration_delimiters: String /* ENCODED */, pub unmarked_style_json: Option<String>, pub markdown_asterisks: bool }
```

**P4.D351 (wave 1, server) edits:** `api/roleplay_templates.rs`,
`services/annotations.rs`, `template_prompt_hint.rs`,
`services/backup/collect.rs` (two column entries), `services/backup/restore/orchestrator.rs:1085-1110`,
`services/quilltap_import/entities.rs:315-330, 492-506`,
`services/qtap_export/schema-key-order.json:245-258`; harness:
`cases/annotations-rendering-patterns.ts`, `cases/template-prompt-hint.ts`,
`cases/roleplay-templates-routes.test.ts`, the export/import/restore oracle
rows (`system-export/import/import-execute/restore.test.ts` — rows only), and
their `.rs` families. **Plus, as briefed,** the `punctuationWarnings` server key
in `db/chat_settings.rs`, `db/chat_settings_composer_repair.rs` (a second
constant), `api/settings.rs`, `cases/settings-routes.test.ts`, `chat-settings-tier2`
— **see 7.2 for the recommended move.**

**P4.D352 (wave 0, SPA) edits / creates:** NEW `apps/web/src/app/punctuation-lint/**`,
NEW `editor/punctuation-lint-plugin.ts` (+spec), `editor/rich-editor.ts`,
`editor/formatting-toolbar.ts` (+spec), `smart-typography/settings.ts` (+spec),
`screens/settings/chat/smart-typography-settings.ts` (+spec),
`screens/settings/chat/chat-settings.types.ts` (SHARED — 7.4), `chat/chat-composer.ts`
(+spec), `screens/salon/salon-conversation.ts` (SHARED), `chat/render/roleplay-rendering.ts`,
`chat/render/markdown-renderer.ts`, `chat/render/typography.ts`, their three specs,
`chat/render/__fixtures__/markdown-fixtures.json`, `tooling/capture-markdown-fixtures.mts`,
`screens/settings/templates/{template-form.ts, template-form-modal.ts, template-delimiter-editor.ts, template-preview-modal.ts}` + NEW `template-add-ons-editor.ts` + the four specs,
`core/core-contract.ts` (its own fence), `styles/qt-components/_variables.css`
+ `_chat.css` (SHARED), `e2e/salon-roleplay-template-flow.spec.ts`,
`e2e/smart-typography-flow.spec.ts`, `e2e/settings-chat-cards-flow.spec.ts`.
Must NOT touch any crate, `help/`, `chat/message-row.ts`, `chat/message-content.ts`.

**P4.D353 (wave 0, SPA) edits / creates:** NEW `apps/web/src/app/chat/diagrams/**`
(fifteen files + specs), `core/clipboard-utils.ts`, `chat/message-content.ts`
(SHARED), `chat/message-row.ts` (SHARED), `chat/streaming-message.ts`, NEW
`screens/characters/can-draw-diagrams-card.ts` (+spec), `screens/characters/edit/character-edit.ts`,
`screens/characters/view/character-detail.ts`, `core/core-contract.ts` (its own
fence), `styles/qt-components/_content.css`, `_variables.css` (SHARED),
`apps/web/package.json` + `package-lock.json`, NEW `e2e/diagrams-flow.spec.ts`.
Must NOT touch `chat/render/**` (P4.D352's), `salon-conversation.ts`, any crate.

### 7.2 Amendment: move the `punctuationWarnings` SERVER key to P4.D356

Proof of the collision: `api/settings.rs` and `db/chat_settings.rs` are edited by
P4.D356 (`3b8b6f42b`'s `coreWhisper` PUT arm + the unknown-key 400 land in
`chat_settings_update :349-` and the ACCEPTED-keys list; `7789ebed6`'s #120 pins
sit in `chat_settings.rs`'s read/mint path `:308-328` / the repair modules),
by P4.D345 (`839575d89` adds the `customToolIntervention` PUT arm to the same
`route.ts` → `api/settings.rs:1150-1170` neighbourhood and `chat-settings.repository.ts`'s
default row), and by P4.D351 for ONE `zod_bool` + ONE struct field + the read
fill. Three lanes, two files, adjacent hunks. The key is a one-line wire
contract (6.3 a) whose SPA consumer is defensive. **Recommend:** P4.D356 owns the
server key (it already re-measures the `smartTypographySettings` rows of
`settings_routes_equivalence` for #120), P4.D351 is left pure-template, and
P4.D345's `customToolIntervention` PUT arm is delivered to P4.D356 as a HANDOFF
hunk or P4.D345 stacks on P4.D356's Tier-1 commit. If the planner keeps the key
in P4.D351, make P4.D351 stack on P4.D356 (both wave 1, serial).

### 7.3 Stacking

- P4.D351 branches from the keystone's Tier-1 commit (the `RtCreate` shape, the
  codec, the seed, the seed-row move) — as the table says.
- P4.D352 and P4.D353 cut from `main` (wave 0): neither needs a crate; both
  read the wire defensively. P4.D353's Aurora e2e beat and the diagram-style
  propagation beat gate on `P4D349_SERVER_LANDED`.
- P4.D352's Novel template SPA beats that need the SEEDED Novel row gate on
  `P4D341_LANDED` (the e2e fixture pair carries only Standard / Quilltap RP
  today; a Novel chat can be CREATED through the form in the beat instead —
  prefer that, no gate).

### 7.4 Shared files and their meeting points

| file | lanes | anchor / shape |
|---|---|---|
| `apps/web/src/app/core/core-contract.ts` | P4.D344, P4.D346, P4.D349?, P4.D352, P4.D353, P4.D359 | the standing fence convention: each lane appends ONE `// === P4.Dnnn … ===` block; the unifier merges (`phase-4.md:964`, P4.D330 `:456` precedent). P4.D352's block: the template DTO/bag fields; P4.D353's: `canDrawDiagrams`/`diagramStyle` on `CharacterListItem`, the character-page DTO, `DetailCharacter`. |
| `screens/salon/salon-conversation.ts` | P4.D352 (3 lines in the template effect `:1066-1101` + 2 bindings `:517-518`), P4.D344 (storyteller controls — `b15fe9404` SalonView +41), P4.D358 (`10e09b7c3` +8), P4.D359 (`9a88f862f` +5) | give the file to P4.D344 (the largest hunk) and have P4.D352 / P4.D358 / P4.D359 write `HANDOFF:` hunks with exact anchors; OR accept the four small disjoint hunks and let git merge — they sit in different regions (template effect vs. participants vs. header). The planner decides; the survey's measured anchors for P4.D352 are above. |
| `chat/message-row.ts`, `chat/message-content.ts` | P4.D353 (new input + hydration + the style resolve), P4.D359 (`9a88f862f` chips — in v5 these likely become an HTML post-pass or a hover component in the same two files), P4.D358 (one `aria-label` each in `MessageRow.tsx`) | P4.D353 owns both files; P4.D359 and P4.D358 deliver `HANDOFF:` hunks. P4.D359's chips, if they post-process the rendered HTML, should take the `imagesHidden` slot in `renderMarkdownToHtml` (P4.D352's file!) — flag to that survey. |
| `screens/settings/chat/chat-settings.types.ts` | P4.D352 (`punctuationWarnings`), P4.D346 (`customToolIntervention`) | two additive lines in different interfaces (`SmartTypographySettings :284-291` vs. the chat-settings DTO) — git merges; name it anyway. |
| `chat/chat-composer.ts` | P4.D352 (two inputs, two bindings), P4.D358 (`eed72d60c` +1 accessible name) | P4.D352 owns; P4.D358 HANDOFF (one attribute). |
| `editor/rich-editor.ts` | P4.D352 (the plugin + input), P4.D358 (`68340008b` names the composer — v5's `ariaLabel` input `:84` already exists; its hunk is probably in `chat-composer.ts`'s binding) | P4.D352 owns. |
| `styles/qt-components/_variables.css` | P4.D352 (6 tokens after `:100`), P4.D353 (17 + 11 tokens between `:585` and `:588`), P4.D357 (`58120a7fe` +10/−3, `cce44e098` +2/−1 — EDITS to existing lines), P4.D359 (`99463fc56` +15 Brahma tokens) | append-only blocks at distinct anchors merge cleanly; P4.D357's edits to EXISTING lines are the only conflict risk — P4.D357 should list its touched line ranges in its order so the others avoid adjacency. |
| `styles/qt-components/_chat.css` | P4.D352 (four rules before `:1043`), P4.D357 (`cce44e098`, `86cc25482` — edits), P4.D359 (`9a88f862f` +20 chips) | same rule. |
| `styles/qt-components/_content.css` | P4.D353 only | — |
| `screens/settings/chat/smart-typography-settings.ts` | P4.D352, P4.D358 (`eed72d60c` +1) | P4.D352 owns; HANDOFF. |
| `screens/settings/templates/template-form-modal.ts` / `template-delimiter-editor.ts` | P4.D352, P4.D358 (`dff76ebef` +37 dialog semantics, `eed72d60c` +57 accessible names on `index.tsx`) | **the biggest a11y overlap in this area** — P4.D352 owns both files; P4.D358's dialog/labels hunks become HANDOFFs, or P4.D358 stacks on P4.D352's Tier-1 commit for these two files. Flag to the WCAG survey. |
| `screens/characters/edit/character-edit.ts`, `view/character-detail.ts` | P4.D353 (mount the card), P4.D349? (if it also ports `humanoid` / `pronounGuidance` SPA — `b27e5089a` touches `DescriptionsTab.tsx`, `eb90d9ed8` touches `CharacterBasicInfo.tsx`; v5 twins are sub-components, not these two files — measure), P4.D358 (`eed72d60c` touches `CharacterBasicInfo.tsx`, `ProfilesTab.tsx`) | P4.D353's hunk is one `<qt-can-draw-diagrams-card>` line per file under the basic-info section; low risk. |
| `apps/web/package.json` / `package-lock.json` | P4.D353 only (this round) | the RULING gates it. |

## 8. Tiering

### 8.1 P4.D351

- **Tier 1 (must land):** the routes per §1.1.10 with every message byte-exact and both DEBUG lines captured; `annotations.rs` whole (all five pattern shapes, tier-1 exact over the grown corpus); `template_prompt_hint.rs` three-state narration + options (tier-1 exact); the four carriers (export key order, import, backup collect, restore) with oracle rows carrying `null` narration + `unmarkedStyle` + `markdownAsterisks:false` through `.qtap` export → import and backup → restore both ways; the routes family green from the pin.
- **Tier 2 (should land):** the `Built-in roleplay template seeded` INFO line (pre-existing gap, if the keystone leaves it); `preview.rs` / `records.rs` key-order pins for the new keys; a `provisioning`-adjacent test that the Novel row's `renderingPatterns` round-trip through `RenderingPattern` (`class_name: ""`, `scope: "strip"`) byte-equal.
- **Tier 3 (loud refusals, never silent):** none needed — every arm has a v4 twin. (If the planner keeps `punctuationWarnings` here: Tier 1 the struct field + PUT arm + read fill + the two new constants; the DDL constant must NOT change.)

### 8.2 P4.D352

- **Tier 1:** the engine + `prose-mode.ts` byte-copied with both vector files + the transcribed `engine.test.ts` / `prose-mode.test.ts` (and the recorded `.v4.json` corpus); the ProseMirror adapter (`punctuationLint` input; decorations — see below); the settings toggle + the `smart-typography/settings.ts` signal; the renderer (§1.1.4–1.1.6) with the re-captured corpus incl. §5.3's rows and `narrowRenderingPatterns` widened; the template editor whole (§1.1.11) + `core-contract` fence; the toolbar's Bold withhold; prose mode threaded from the Salon's template fetch; the three e2e extensions.
- **Tier 2:** a TS port of `generateFormattingPromptHint` + the Draft button (the standing deferral `template-form-modal.ts:42-45`; the existing tsx oracle `cases/template-prompt-hint.ts` can feed a `.v4.json` for a vitest twin) — **needs the planner to lift the deferral explicitly**; `template-preview-modal.ts` showing `None — plain prose` for a null narration and the two wrap flags.
- **Adapter divergence to record (not a ruling):** v4 paints with the CSS Custom Highlight API so Lexical's model is untouched; ProseMirror's `Decoration.inline` is the view layer (the document, undo history and markdown export are equally untouched — `rich-editor.ts:410-427` already runs a `DecorationSet` plugin), works in jsdom (the Highlight API does not), gives the hover `title` for free, and avoids the WebKit/Chromium support gap. The four `::highlight(name)` rules become `.name` class rules with the SAME tokens. If the lane prefers fidelity, the Highlight API is available in the Tauri WebKit webview (Safari 17.2+) and Chromium 105+ — then keep v4's `CSS.highlights` registry and `highlightsSupported()` bail (and its debug line).
- **Tier 3:** none.

### 8.3 P4.D353

- **Tier 1 (if the ruling is "port with the four deps"):** `diagram-fences.ts`, `sanitize-svg.ts`, `render-queue.ts`, `diagram-theme.ts`, the three renderers, `diagram-block.ts` with the slot pass in `message-content.ts`, the style resolve in `message-row.ts` + `streaming-message.ts`, the CSS, the `core-contract` fence, the Aurora card, `e2e/diagrams-flow.spec.ts` beats 1–3, 6, 8.
- **Tier 2:** the lightbox + Copy PNG (`9faed0c7e` whole) + beats 4–5; the theme-change re-render (v4's `themeKey` in the cache key — v5's theme service must expose an id/mode signal).
- **Tier 3 (loud):** if the ruling is "defer": a `qt-diagram-deferred` note above the code block — `The draughtsman has not yet been engaged — this diagram is shown as its source for now.` (a v5-authored line in the house voice, recorded as a divergence) so a Novel-era chat never shows an unexplained fence; the Aurora card still lands (the flag is P4.D349's and the model-visible note depends on it) with its hint unchanged.
- If the ruling is "mermaid only": the `dot`/`graphviz`/`svg` engines take the Tier-3 note; `sanitize-svg.ts` still lands (mermaid's own output is not sanitized, but `render-svg`'s is) — then `dompurify` is still a dep; so "mermaid only" saves exactly `@hpcc-js/wasm-graphviz` + `svg2roughjs` (3.9 MB unpacked, lazy).

## 9. Fixtures

- **KEYSTONE delivers:** `builtin_templates.json` (3 rows, re-dumped), `fresh_schema.json`, `chat_settings_seed.json` (+ the register line), the rebuilt `roleplay-templates` tier-2 fixture (`QT_FIXTURE_OUT`) and `builtin-templates` oracle, and — because the committed groups-projects fixture pair (`crates/quilltap-web/tests/fixtures/*-main.db`, built by `harness/oracle/fixtures/build-*-fixture.ts`) lacks the two columns — either the boot ensure runs in `fresh_db` (preferred: proves the ensure) or the pairs are rebuilt (`rebuilt-fixture-pair-vintage` note: a rebuild drops migration-only columns later widens added — diff `sqlite_master` first).
- **P4.D351 delivers:** the grown NDJSONs for `annotations`, `template-prompt-hint`, `roleplay-templates-routes`, the export/import/restore rows; regenerates dependent oracles via the sweep driver (`--families annotations_rendering_patterns,template_prompt_hint,roleplay_templates_routes,…`; `--v5w ~/source/quilltap-v5`, the spaceless path — memory note).
- **P4.D352 delivers:** `punctuation-lint/fixtures/{punctuation-vectors,prose-vectors}.json` (byte copies; a spec guards `cases.length` as `engine.spec.ts:44-48` does), `punctuation-lint/engine.v4.json` (recorded), the re-captured `markdown-fixtures.json` (67 → ~90; the P4.30 note at `markdown-renderer.spec.ts:27-37`).
- **P4.D353 delivers:** nothing committed from v4 (the diagram tests are mocked/stubbed); the e2e seed needs a message row with a closed mermaid fence + a `dot` fence + a bad fence in a Salon fixture pair — add rows to the Salon pair's seeder, never a new pair (`own-instance-e2e-fixture-choice` note).

## 10. Rulings owed to the human

1. **The four client dependencies (P4.D353)** — `mermaid@12.2.0` (122 MB unpacked, 23 deps incl. `d3`, `cytoscape`, `elkjs`, `katex ^0.18.0` — compatible with v5's pinned `0.18.4`; lazy-loaded so the initial bundle is unaffected, but `npm ci` and the lockfile grow by hundreds of packages), `@hpcc-js/wasm-graphviz@1.30.0` (2.0 MB, zero deps, wasm inlined), `svg2roughjs@3.2.3` (1.9 MB; `roughjs`, `svg-pathdata`, `tinycolor2`), `dompurify@3.4.17` (1.6 MB; also a transitive dep of mermaid). Options, measured: **(A) port whole** — v4 parity, four deps, Angular lazy chunks (verify once in the lane that esbuild splits `import('mermaid')` and that mermaid's internal `import()`s resolve under the Angular builder — a known-good combination, but the first `ng build` is the proof); Tauri: CSP `null`, no `'wasm-unsafe-eval'` needed; the SPA served by `quilltap-web` has no CSP header; **(B) mermaid only** — saves 3.9 MB of lazy weight and two deps, loses `dot`/`graphviz`/`svg` fences (shown as code under a loud note) and the hand-drawn Graphviz; **(C) defer whole** — a loud placeholder note over the code block, the Aurora card still lands (P4.D349's flag), the model-visible note then promises pictures the Salon cannot draw — the one option that makes v5 LIE to the model, so the note should be withheld (which is P4.D349's `system_prompt.rs` arm — a cross-lane consequence). Recommendation: A.
2. **`punctuationWarnings` server-key ownership** (P4.D351 as briefed vs. P4.D356 — §7.2).
3. **The adapter shape for the lint** (decorations vs. the Custom Highlight API — §8.2; an adapter divergence the human should see, not strictly a ruling).
4. **Lift the "Draft formatting instructions" deferral?** (P4.D352 Tier 2 — a TS twin of a function the Rust side already ports and the SPA never calls.)
5. **The `qt-roleplay-plain` style** — v4 ships NO CSS for it in any touched stylesheet (grep over `app/styles` at the pin and v5's `styles/`: zero hits) — "Plain — no chip, no colour of its own" is by design; the lane adds no rule. Confirm, so nobody "fixes" it.
6. **`narrowRenderingPatterns`** — the five-scope widening is a v5 bug fix the ledger did not record; no divergence, but the human should know P4.D352's first red is a v5 defect, not drift.

## 11. Dogfood rows to queue

- The Friday copy's first boot after the keystone: `roleplay_templates` gains the two columns, `Standard`'s `narrationDelimiters` cell reads `"*"` (JSON), `Quilltap RP`'s stays `["[","]"]`, a third built-in `Novel` appears in Settings → Templates; a user template keeps its raw `*` until edited.
- Create a Novel chat: narration italic and unmarked, `"dialogue"` upright with curly quotes from the model, `*emphasis*` bold-italic with the marks hidden, `**shouting**` shown plain, `((aside))` in the OOC livery, a `* * *` scene break as a rule, emphasis INSIDE dialogue styled, a multi-paragraph speech with unclosed paragraphs styled to each paragraph's end. 💸 one real turn (model-visible `NOVEL_SYSTEM_PROMPT`).
- Settings → Chat → Smart typography: the new toggle persists across reload; with `displayQuotes` ON a Novel chat still curls (the `mentionsGlyph` exemption).
- The composer under Novel: a nested `"` lights up (hover explains), an unclosed `(( ` lights up only after the caret leaves the paragraph, a draft with no dialogue tints every narrated run, narration/OOC previews paint, the toggle off removes the warnings but keeps the preview, Bold is absent from the toolbar.
- Template editor: copy Novel as new → the form shows `None — plain prose`, `Style unmarked text` ticked with `qt-roleplay-plain` + Italic, the asterisks box unticked, the Dialogue delimiter's two wrap boxes ticked; save → the stored patterns equal the built-in's.
- `.qtap` export of the Novel copy → import into a fresh instance → identical bytes (`narrationDelimiters: null`, `unmarkedStyle`, `markdownAsterisks: false`); a full backup → `replace` restore keeps them.
- Diagrams (ruling A): a character with Draws diagrams ON gets the `[FORMATTING: DIAGRAMS]` note (P4.D349) and, asked for a flowchart, returns a ```` ```mermaid ```` fence that renders hand-drawn; switch the style to Clean → re-render ruled; a ```` ```dot ```` graph renders; the lightbox zooms, Copy PNG pastes into the composer; Escape returns the transcript to its place; a theme switch re-draws in the new palette; quick-hide Salon Images leaves diagrams visible. 💸 two real turns.

## 12. Measurement record (what was read, for the lanes' Survey-verified starting points)

- v4: `git show 778b97b2b` (34 files), `1dd0a0c01` (24), `2b2b190b6` (21), `6588972d5` (68), `9faed0c7e` (25), `8bf856050` (4); `git log 322a5488e..236c04e35 -- <each file this area owns>` to find every other commit touching them (§7.4); the pin worktree for `engine.ts`, `prose-mode.ts`, `PunctuationLintPlugin.tsx`, the eleven `components/chat/diagrams/*` files, `lib/markdown/diagrams.ts`, `CodeBlockWithCopy.tsx`, `chat-settings.repository.ts:100-125`, `DDL.md:1534-1550`; `npm view` for the four packages.
- v5 (`f6cdceedd`): every `path:line` in §2 was read, not inferred; `grep` censuses: `narrationDelimiters` (12 core files), `smartTypography` (5 core + 2 web-crate-adjacent + 11 SPA), `canDrawDiagrams|diagramStyle` (zero), the renderer twins (`tokenizeInline` etc. — SPA only), `qt-roleplay-plain` (zero), `activeThemeId` (not measured — the lane's first grep), `Decoration` precedents (`rich-editor.ts` only), the e2e list (125 specs; the three named), `angular.json` budgets, `tauri.conf.json` CSP, `static_serve.rs:34`.
