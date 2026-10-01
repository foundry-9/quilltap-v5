# Survey — P4.D242: the memory-extraction prompts of `ca363178d` (part c)

**Date:** 2026-09-30 · **v4:** `ca363178d` (main HEAD; checkout clean AT it) ·
**v4 baseline:** `97b25fc53` · **v5:** `main` at `735cf568e` · read-only
measurement; nothing built or run.

`git log 97b25fc53..ca363178d -- lib/memory/cheap-llm-tasks/memory-tasks.ts`
lists ONLY `ca363178d`, so the baseline→target diff of this file is exactly
this commit's hunk set (89 lines, +84/−5).

## Spec intent (`docs/developer/features/prompt-trust-and-anti-committee.md` §4.3, §8, §15 "Phase 3")

Port from the HUNKS below, never from this prose (ledger §5.3).

- Mechanism 6: a character states a condition, the turn ends on it, and the
  extractor records "{{user}} agreed" for every observer and "I accepted" in
  the persona's own SELF memories.
- Structural fact (§8.1): by construction the user's line opens the turn and
  the next user line is not in it, so nothing the user said can be assent to
  a later proposal; the honest form is "had not yet responded".
- §8.2: the transcript heading says so — only when the turn carries a human
  line; otherwise byte-identical (it is the user message, not the cached prefix).
- §8.3: one AGREEMENTS section in BOTH extraction bodies (SELF, OTHER) between
  WHAT TO SKIP and DEDUPLICATION; spoken-assent wording on OTHER's HINGES, both
  0.90 anchors, a new OTHER 0.55 rung; an OTHER good item + a SEPARATE bad
  example (§15: separate because the existing bad example's "Correct output: []"
  doesn't apply); `FIRST_PERSON_USER_CLAUSE` gains the not-yet-responded
  sentences; TAGS `future` reserved for the speaker's own intent.
- §8.4: one sentence in `FOLD_EPISODE_PROMPT`. §8.5: no code-side consent
  filter — the parser deliberately does NOT drop invented assent (pinned by a test).

## §A — v4 hunks, in order (`lib/memory/cheap-llm-tasks/memory-tasks.ts` at `ca363178d`)

There are **two** extraction bodies, not three: `selfBodyForCap` (:269, the
SELF pass — used for AI characters AND for a user-controlled subject, which
only adds the `FIRST_PERSON_USER_CLAUSE` preamble) and `otherBodyForCap`
(:452, the OTHER pass — the user is just a subject tagged
`(the user-controlled character)` in the footer). **The ledger row's "BOTH the
USER-subject and the SELF/OTHER bodies" is wrong: there is no USER-subject
body.** v4's own doc comment (:185) says "Shared by the SELF and OTHER bodies".

### A1. NEW module constant `AGREEMENTS_INSTRUCTION_BLOCK` (:185-207, not exported)

Inserted between `EVENT_INSTRUCTION_BLOCK` and `TAGS_INSTRUCTION_BLOCK`. Source
(no `${}` interpolation, no backslash; non-ASCII = `—` U+2014 and `…` U+2026;
1,023 chars / 1,039 UTF-8 bytes of template body):

```ts
const AGREEMENTS_INSTRUCTION_BLOCK = `AGREEMENTS, PROPOSALS, AND CONDITIONS — read these strictly
- Record an agreement ONLY when the agreeing party's own words of
  assent appear in this transcript, spoken by that party: "yes,"
  "fine," "deal," "I'll do it." Nothing else counts.
- A condition, demand, rule, or proposal is recorded as what it is and
  attributed to whoever said it — "X proposed…", "X asked that…",
  "X set a condition that…" — never as accepted by anyone else.
- Silence is not assent. Neither is the exchange ending, an apology,
  self-criticism, a change of subject, "I'll think about it," or
  "I need to sit with that." When a proposal was the last word, say
  so: "…; <name> had not yet responded."
- The USER's lines came BEFORE every character line in this turn, so
  nothing the USER said can be assent to a proposal made after it.
- Keep stated limits in the text: "until breakfast," "for tonight,"
  "custody, not confiscation — back at breakfast." A memory that
  drops the limit records a different, larger thing than was said.`
```

Interpolated at exactly two sites — SELF :316 and OTHER :506 — each placed
after the `${ORIENTING_CONTEXT_SKIP_BULLET}` line + one blank line and before
`DEDUPLICATION` (one blank line after). SELF site:

```ts
- Narrative references to tool output: terminal sessions, file paths,
  exit codes, commit hashes, command names.
${ORIENTING_CONTEXT_SKIP_BULLET}

${AGREEMENTS_INSTRUCTION_BLOCK}

DEDUPLICATION
```

The OTHER site (:503-508) is the same shape after the OTHER skip list's
`- Anything implied by previously-established facts about the subject.` +
`${ORIENTING_CONTEXT_SKIP_BULLET}`. Note: the 4th bullet ("The USER's lines
came BEFORE…") is unconditional — it also reaches autonomous-room calls.

### A2. `TAGS_INSTRUCTION_BLOCK` — the `future` gloss (:216-218; old :192)

Old: `            future  — a stated intent or commitment not yet acted on`
New (source bytes — note the **escaped backticks** `\``, which evaluate to
plain `` ` `` at runtime):

```ts
            future  — a stated intent or commitment not yet acted on
                      (the speaker's own; a proposal awaiting someone
                      else's answer is \`moment\`)
```

Runtime bytes of the two added lines (22-space indent):

```
                      (the speaker's own; a proposal awaiting someone
                      else's answer is `moment`)
```

TAGS is interpolated into BOTH bodies (SELF :371, OTHER :602), so this moves
both. NOT changed: the keyword-extraction prompt's own unrelated gloss
`  future  — about an intention or plan not yet acted on` (old :723 — a
different constant; don't touch it).

### A3. SELF importance anchors (:323-333) — one rewrite

Before (old :295-304):

```
IMPORTANCE — calibrate to these anchors
  0.90  The subject made a major commitment or had a self-revelation
        that changes how they understand themselves.
  0.65  The subject formed a substantive new opinion, plan, or
        position.
  0.40  The subject expressed a fresh preference, reaction, or novel
        gesture in passing.
  0.20  The subject acted in a way consistent with established identity
        but worth a single note.
  < 0.20  Do not extract.
```

After:

```
IMPORTANCE — calibrate to these anchors
  0.90  The subject made a major commitment (one the subject spoke
        themselves) or had a self-revelation that changes how they
        understand themselves.
  0.65  The subject formed a substantive new opinion, plan, or
        position.
  0.40  The subject expressed a fresh preference, reaction, or novel
        gesture in passing.
  0.20  The subject acted in a way consistent with established identity
        but worth a single note.
  < 0.20  Do not extract.
```

Rungs: 0.90 / 0.65 / 0.40 / 0.20 / `< 0.20` — unchanged count (4 + floor).

### A4. `FIRST_PERSON_USER_CLAUSE` (:405-414) — two sentences appended

Before (old :376-381):

```ts
const FIRST_PERSON_USER_CLAUSE =
  `IMPORTANT: The SUBJECT below is a character a human is playing directly, so ` +
  `the SUBJECT's lines in the transcript may be written in the first person. ` +
  `Read every "I", "me", "my", and "myself" in the SUBJECT's own lines as ` +
  `referring to the SUBJECT — attribute those decisions, realizations, and ` +
  `actions to the SUBJECT, not to anyone else in the exchange.\n\n`
```

After:

```ts
const FIRST_PERSON_USER_CLAUSE =
  `IMPORTANT: The SUBJECT below is a character a human is playing directly, so ` +
  `the SUBJECT's lines in the transcript may be written in the first person. ` +
  `Read every "I", "me", "my", and "myself" in the SUBJECT's own lines as ` +
  `referring to the SUBJECT — attribute those decisions, realizations, and ` +
  `actions to the SUBJECT, not to anyone else in the exchange. ` +
  `The SUBJECT's lines came first in this turn and the SUBJECT has not yet ` +
  `responded to anything the characters said after them. Never record the ` +
  `SUBJECT as having accepted, agreed to, or consented to anything proposed ` +
  `in those later lines.\n\n`
```

Runtime string after (one line; the old `.\n\n` becomes `. ` + the new text +
`.\n\n`):

```
IMPORTANT: The SUBJECT below is a character a human is playing directly, so the SUBJECT's lines in the transcript may be written in the first person. Read every "I", "me", "my", and "myself" in the SUBJECT's own lines as referring to the SUBJECT — attribute those decisions, realizations, and actions to the SUBJECT, not to anyone else in the exchange. The SUBJECT's lines came first in this turn and the SUBJECT has not yet responded to anything the characters said after them. Never record the SUBJECT as having accepted, agreed to, or consented to anything proposed in those later lines.\n\n
```

Prepended only when `isUserControlled` (the SELF target slice is
`isUserControlled`), before the autonomous clause (:425-427, unchanged).

### A5. OTHER `WHAT TO PICK` item 1 (HINGES) — **NOT in the ledger row**

Before (old :434-435):

```
1. HINGES — a decision, commitment, agreement, refusal, or realignment
   formed during this exchange.
```

After (:467-469 — re-wrapped to three lines):

```
1. HINGES — a decision, commitment, agreement (spoken by the agreeing
   party — see AGREEMENTS below), refusal, or realignment formed
   during this exchange.
```

(SELF's `1. SELF-HINGES` line is unchanged.)

### A6. OTHER importance anchors (:515-527) — one rewrite + one NEW rung

Before (old :479-487):

```
IMPORTANCE — calibrate to these anchors
  0.90  An explicit new commitment or revelation that changes how the
        observer relates to the subject.
  0.60  A new substantive fact about the subject's background, plans,
        or skills.
  0.40  A new preference, trait, or novel gesture expressed in passing.
  0.20  A specific event occurred with the subject present, no new
        information.
  < 0.20  Do not extract.
```

After:

```
IMPORTANCE — calibrate to these anchors
  0.90  An explicit new commitment the subject themselves spoke, or a
        revelation, that changes how the observer relates to the
        subject. A proposal made TO the subject is not the subject's
        commitment.
  0.60  A new substantive fact about the subject's background, plans,
        or skills.
  0.55  A proposal, condition, or demand the subject stated, not yet
        answered.
  0.40  A new preference, trait, or novel gesture expressed in passing.
  0.20  A specific event occurred with the subject present, no new
        information.
  < 0.20  Do not extract.
```

Rungs now 0.90 / 0.60 / **0.55** / 0.40 / 0.20 / `< 0.20` — the 0.55 rung
sits between 0.60 and 0.40. The ledger's "three importance-anchor rewrites"
= SELF 0.90 (A3), OTHER 0.90, OTHER 0.55 (new).

### A7. OTHER good example — a third item appended (:565-575)

The list goes 2 → 3 items, order: (1) Amy four-tier cache, 0.85; (2) Charlie
agreed to defer renaming, 0.65 (unchanged — Charlie's agreement is genuine
there); (3) NEW:

```
  },
  {
    "subjectIndex": 1,
    "content": "Amy set a condition that nothing fires without the household hearing it first; Charlie had not yet responded when the exchange ended.",
    "summary": "proposed household-hears-first condition",
    "keywords": ["condition", "proposal", "household"],
    "importance": 0.55,
    "temporal": "moment",
    "scope": "narrow",
    "context": "relationships"
  }
]
```

(The `}` of item 2 gains `,` — the hunk turns `  }` into `  },` then appends.)

### A8. OTHER bad examples — a NEW separate block (:591-598)

The existing "six restatements" bad example (:578-589, "Correct output: [].")
is unchanged; a second bad-example block follows it, then one blank line, then
`${EVENT_INSTRUCTION_BLOCK}` as before:

```
All six restate facts in subject 1's ALREADY ESTABLISHED block.
Correct output: [].

EXAMPLE — bad extraction (assent invented for subject 2):
[
  { "subjectIndex": 2, "content": "Charlie agreed that nothing fires without the household hearing it first", "importance": 0.85 },
  { "subjectIndex": 2, "content": "Charlie accepted the new household rule", "importance": 0.8 }
]
Charlie said nothing after Amy's condition; recording assent invents
it. Correct output: the condition attributed to Amy, as in the third
good item above.

```

SELF body examples: **unchanged** (1 good, 1 bad; the SELF good example's
"after Charlie agreed it was the highest-leverage fix" stays).

### A9. NEW exported `ORDERED_TURN_TRANSCRIPT_HEADING` (:836-838) + `renderTurnContext` (:845-908)

```ts
/** The transcript heading when the turn carries a human line (spec §8.2). */
export const ORDERED_TURN_TRANSCRIPT_HEADING =
  "TURN TRANSCRIPT (in the order spoken — the USER's lines came first; nothing the USER says here answers anything a character says below it):"
```

(double-quoted TS string; 139 chars / 141 bytes; one `—`; contains `'`.)

The full post-commit function:

```ts
function renderTurnContext(transcript: TurnTranscript): string {
  // A user-controlled character now arrives as a slice (built from the turn
  // opener). It is the human participant *and* a memory-forming character, so
  // we list it on the USER line and render its lines once in the body labeled
  // "(the user-controlled character)" — never under the AI-character roster and
  // never duplicated as a standalone opener. AI slices keep their existing
  // labeling, so a turn with no user-controlled slice renders byte-identically
  // to before.
  const aiSlices = transcript.characterSlices.filter(s => !s.isUserControlled)
  const userSlices = transcript.characterSlices.filter(s => s.isUserControlled)
  const hasUserSlice = userSlices.length > 0

  const roster: string[] = ['PARTICIPANTS IN THIS TURN:']
  const userDisplayName = transcript.userCharacterName ?? userSlices[0]?.characterName ?? null
  if (userDisplayName) {
    roster.push(`- USER: ${userDisplayName} (the human participant)`)
  } else if (transcript.userMessage !== null) {
    roster.push('- USER: The human participant')
  }

  if (aiSlices.length === 1) {
    const slice = aiSlices[0]
    roster.push(
      `- CHARACTER: ${formatNameWithPronouns(slice.characterName, slice.characterPronouns ?? null)} (an AI character)`
    )
  } else if (aiSlices.length > 1) {
    roster.push('- CHARACTERS (AI characters in this chat):')
    for (const slice of aiSlices) {
      roster.push(
        `  * ${formatNameWithPronouns(slice.characterName, slice.characterPronouns ?? null)}`
      )
    }
  }

  const transcriptSections: string[] = []
  // Render the standalone opener only when no user slice carries it — i.e. a
  // plain human with no character. When a user slice exists, its body line
  // below is the single rendering of that text.
  if (transcript.userMessage !== null && !hasUserSlice) {
    const userLabel = transcript.userCharacterName
      ? `${transcript.userCharacterName} (the user)`
      : 'The user'
    transcriptSections.push(`${userLabel} says:\n"${transcript.userMessage}"`)
  }
  for (const slice of transcript.characterSlices) {
    const role = slice.isUserControlled ? 'the user-controlled character' : 'the character'
    const characterLabel = `${formatNameWithPronouns(slice.characterName, slice.characterPronouns ?? null)} (${role})`
    transcriptSections.push(`${characterLabel} says:\n"${slice.text}"`)
  }

  // By construction the human's line opens the turn and the next human line
  // is not in it, so nothing the human says here answers a character below.
  // Saying so is what keeps the extractor from manufacturing assent. A turn
  // with no human line keeps the plain heading, byte-identical to before.
  const heading = transcript.userMessage !== null || hasUserSlice
    ? ORDERED_TURN_TRANSCRIPT_HEADING
    : 'TURN TRANSCRIPT:'

  return `${roster.join('\n')}

${heading}

${transcriptSections.join('\n\n')}`
}
```

- Condition, confirmed from the hunk: `transcript.userMessage !== null ||
  hasUserSlice`. `hasUserSlice` is the pre-existing local at :855
  (`userSlices.length > 0`, `userSlices` = `characterSlices` filtered on
  `isUserControlled`). `''` is `!== null` → an empty user message takes the
  ORDERED arm.
- Old arm literal: `'TURN TRANSCRIPT:'`. Nothing else in the output changed:
  roster, the `\n\n` before and after the heading, the section join
  `'\n\n'`, no trailing newline — the template is identical bar
  `TURN TRANSCRIPT:` → `${heading}`.
- Edge: the roster's `- USER: …` line keys on `userCharacterName ??
  userSlices[0]?.characterName` OR `userMessage !== null` — so a transcript
  with `userMessage === null`, no user slice, but `userCharacterName` set
  renders a USER roster line AND the OLD heading. Reachability: v4's
  `buildTurnTranscript` (`lib/services/chat-message/turn-transcript.ts:141-180`)
  only builds a user slice when it also sets `userMessage`, so the
  slice-without-message arm is unreachable from the builder but legal input
  to the renderer.
- Callers: module-private; only `extractSelfMemoriesFromTurn` (:945) and
  `extractOtherMemoriesFromTurn` (:1010). Repo-wide grep of `lib/ app/
  packages/` (non-test): no other user of `renderTurnContext`,
  `ORDERED_TURN_TRANSCRIPT_HEADING`, `AGREEMENTS_INSTRUCTION_BLOCK` or
  `FOLD_EPISODE_PROMPT`. Carina's handler
  (`lib/background-jobs/handlers/carina-memory-extraction.ts:97-113`) reaches
  the renderer through `processTurnForMemory` with a synthetic transcript:
  `userMessage: question.length > 0 ? question : null`, no user slice — so
  an empty Carina question takes the OLD arm.

### A10. `FOLD_EPISODE_PROMPT` (:1141-1165) — one sentence

Inserted after the `importance` field paragraph, before the final `Return a
JSON array only…` line, as its own paragraph (blank line before and after):

```
  importance    0.20–1.00 (0.9 = a day the participants will retell for
                years; 0.5 = a pleasant but ordinary outing)

An episode records what was said and done, not what was agreed: attribute proposals and conditions to their speaker, and record an agreement only where the agreeing party's own assent appears in the window.

Return a JSON array only. No prose, no code fences. If nothing qualifies, return [].`
```

The sentence alone (one line, no wrap):

```
An episode records what was said and done, not what was agreed: attribute proposals and conditions to their speaker, and record an agreement only where the agreeing party's own assent appears in the window.
```

### A11. Anything else in the hunks

Walked all 11 hunks: A1, A2, A1-SELF-site, A3, A4, A1-OTHER-site + A5, A6,
A7, A8, A9, A10. Beyond the ledger row: **A5 (OTHER HINGES rewording)**; the
ledger's "USER-subject body" does not exist; the example changes are OTHER-only.
Nothing else in `lib/memory/**` or `lib/services/**` touches these symbols
(`--stat`: the only `lib/memory` source file is this one; the
`lib/services/*` files in the commit are parts (b)/(d) — generators and
`context-builder.service.ts`).

### A12. Tests in the commit (oracle material)

- `lib/memory/cheap-llm-tasks/__tests__/memory-consent-regression.test.ts`
  (**NEW**, 107 lines): mocks `executeCheapLLMTask`; over the
  proposal-no-reply fixture asserts the user line opens and the proposal
  closes the transcript; the OTHER user message `toContain(ORDERED_TURN_TRANSCRIPT_HEADING)`
  and `user.trimEnd().endsWith(\`${PROPOSAL_LINE}""\`)`; the stage direction
  precedes `Friday sets her mug`; Owen's SELF system `toContain('the SUBJECT
  has not yet responded')`; and **the parser does NOT filter invented assent**
  — `INVENTED_ASSENT_RESPONSE` comes back as both "Owen agreed…" / "Owen
  accepted…" (§8.5).
- `memory-extraction-tags.test.ts` (MODIFIED, +31): SELF has `WHAT TO SKIP`
  < `AGREEMENTS, PROPOSALS, AND CONDITIONS` < `DEDUPLICATION` by index, plus
  `had not yet responded`, `(one the subject spoke`; OTHER has the block,
  `agreement (spoken by the agreeing`, `A proposal made TO the subject is not
  the subject's`, `/0\.55 {2}A proposal, condition, or demand the subject
  stated, not yet\s+answered\./`, `Charlie had not yet responded when the
  exchange ended`, `recording assent invents`; TAGS contains
  `"a proposal awaiting someone\n                      else's answer is \`moment\`"`.
- `memory-user-controlled.test.ts` (MODIFIED, +33): the clause sentences;
  heading arms: `userMessage: 'tell me a story'` + one AI slice → ORDERED and
  `not.toMatch(/^TURN TRANSCRIPT:$/m)`; user slice + `userMessage` → ORDERED;
  AI slice only, no `userMessage` (greeting) → `/^TURN TRANSCRIPT:$/m` and not
  ORDERED. **v4 itself never tests the slice-without-message arm.**
- `__tests__/unit/lib/fixtures/proposal-no-reply.ts` (NEW, 167): Owen
  (user-controlled, `p-owen`), Amy, Friday; `PROPOSAL_LINE = "Then here's my
  price: nothing fires without the household hearing it first."`; a custody
  fixture (`CUSTODY_LINE`, the "breakfast" limit); built through the REAL
  `buildTurnTranscript` with `turnOpenerMessageId: 'u-1'`; `fridaySubjects`
  (Amy=1, Owen=2 `isUser`), `amySubjects`; `INVENTED_ASSENT_RESPONSE`.
- `__tests__/eval/memory-consent/` (NEW): live, skipped unless
  `MEMORY_CONSENT_EVAL_MODEL`; gates no-invented-assent and
  attributed-and-marked-unanswered at N/N, reports the limit. Not
  differential material (live model); v4's spec records **no pass rate yet**.

## §B — v5 on `main` (`735cf568e`)

`crates/quilltap-core/src/memory_tasks.rs` (1,169 lines) + the GENERATED
`crates/quilltap-core/src/memory_tasks/prompt_text.rs` (309 lines).

| v4 | v5 twin | form |
|---|---|---|
| `selfBodyForCap` | `self_body_for_cap` `memory_tasks.rs:246`, bytes in `prompt_text::SELF_BODY_BEFORE_CAP`/`_AFTER_CAP` (`prompt_text.rs:11`/`:16-147`) | raw `r####"…"####` consts, joined by `format!("{}{max_memories}{}", …)` — consts are ARGS, so `{`/`}` in the JSON examples are safe |
| `otherBodyForCap` | `other_body_for_cap` `:254`, `OTHER_BODY_*` (`prompt_text.rs:149`/`:155-309`) | same |
| `ORIENTING_CONTEXT_SKIP_BULLET`, `EVENT_INSTRUCTION_BLOCK`, `TAGS_INSTRUCTION_BLOCK` | **no Rust constants** — pre-substituted into each body by the generator (TAGS appears twice: `prompt_text.rs:127-139` SELF, `:289-301` OTHER; `future` lines `:134`, `:296`) | generated |
| `AGREEMENTS_INSTRUCTION_BLOCK` | absent; goes into both bodies (SELF before `DEDUPLICATION` `prompt_text.rs:60`; OTHER before `:203`) | via generator |
| SELF anchors | `prompt_text.rs:65-74` (0.90 at `:66`) | generated |
| OTHER HINGES / anchors | `:163-164` / `:210-218` (0.90 `:211`, 0.60 `:213`, 0.40 `:215`) | generated |
| OTHER good / bad examples | `:235-257` / `:259-270` | generated |
| `FIRST_PERSON_USER_CLAUSE` | `memory_tasks.rs:239-244` | **hand-written** `"…\` continuation string (lines start at col 0; `\"` escapes); prepended at `:276-278` |
| `renderTurnContext` | `render_turn_context` `:390-466`; literal `"{}\n\nTURN TRANSCRIPT:\n\n{}"` at `:462` | hand-written |
| `hasUserSlice` | already computed: `let has_user_slice = !user_slices.is_empty();` `:406`; `transcript.user_message: Option<String>` (`None` = null, `Some("")` present — `:372-375` doc) | the branch is local: `user_message.is_some() \|\| has_user_slice` |
| `ORDERED_TURN_TRANSCRIPT_HEADING` | absent; make it `pub const` (v4 exports it) | — |
| `FOLD_EPISODE_PROMPT` | `FOLD_EPISODE_PROMPT_BEFORE_CAP`/`_AFTER_CAP` `memory_tasks.rs:856-880`, `fold_episode_prompt()` `:882-884` (`format!` with inline const args) | **hand-split**, NOT produced by any committed generator (its doc `:851-855` says "Extracted mechanically (see … extract-memory-task-prompts.py for the sibling extraction)") — the new sentence goes in `_AFTER_CAP` between `:878` (`…ordinary outing)`) and `:880` (`Return a JSON array…`) |

Other v5 embeddings of these bytes — grep of `crates/ apps/ harness/oracle/`
for `TURN TRANSCRIPT`, `playing directly, so`, `made a major commitment`,
`An explicit new commitment`, `not yet acted on`, `You produce memory
entries`, `PARTICIPANTS IN THIS TURN`, `IMPORTANCE — calibrate`,
`consolidating a batch`: **only `memory_tasks.rs` and `prompt_text.rs`**.
`services/carina_memory_extraction.rs:166-185` builds a `TurnTranscript`
(`user_message: None` iff the question is empty — v4's rule) and calls
`process_turn_for_memory`; it embeds no prompt bytes. `quilltap-host`,
`quilltap-web`, `quilltap-cli`, `quilltap-tauri`, `apps/web/src`: none. The
in-file unit tests (`memory_tasks.rs:1013-1169`) assert no prompt bytes.
The oracle cases only match these prompts by PREFIX (`You are consolidating a
batch`, `\nCONTEXT\n(SUBJECT|OBSERVER): `, `autonomous character-to-character`)
— all unchanged.

Generator: `harness/oracle/scripts/extract-memory-task-prompts.py` (committed;
`prompt_text.rs`'s header calls it "the session script `extract_prompts.py`" —
a stale name). It reads the TS, pulls three constants by `template_after`,
substitutes `${ORIENTING_CONTEXT_SKIP_BULLET}` / `${EVENT_INSTRUCTION_BLOCK}`
/ `${TAGS_INSTRUCTION_BLOCK}`, and **aborts** on (i) any leftover `${…}` other
than the cap and (ii) any backslash in a body. At `ca363178d` it will abort
on BOTH: `${AGREEMENTS_INSTRUCTION_BLOCK}` is unsubstituted, and TAGS now
contains `\`moment\``. `harness/tools/extract-prompt-text.cjs` (P4.D212's
`FOLD_SUMMARY_PROMPT` regen; evaluates the template via `new Function`, so it
unescapes correctly) is hard-wired to `context_summary/prompt_text.rs`'s
`r#"…"#` consts and refuses any `${` — it handles neither the bodies (cap
interpolation) nor `FOLD_EPISODE_PROMPT` (`${FOLD_EPISODE_CAP}`). No
`gen-*.mjs` under `harness/oracle/tools/` covers memory tasks.

## §C — families

Every tier-3 family below replays the oracle's RECORDED
`provider|model|temperature|JSON(messages)` key through
`CannedCompletionProvider`, so a system- or user-message byte change is a
canned MISS → red until ported.

| family (harness test) | oracle case / corpus | fixtures | at `ca363178d` |
|---|---|---|---|
| `memory_tasks_equivalence` | `cases/memory-tasks-tier1.test.ts` (jest; only `executeCheapLLMTask` mocked) / `fixtures/memory-tasks-tier1.json` (18 cases, shared by both sides via `memory_tasks_equivalence.rs:214`) | none (pure); `QT_ORACLE_MEMORY_TASKS` | **MOVES** — every case that builds messages (15 of 18; #4/#10/#11 are no-call) has a new system prompt; NEW-arm user messages change |
| `memory_processor_tier3_equivalence` | `cases/memory-processor-tier3.test.ts` / `fixtures/memory-processor-tier3.json` | /tmp via `fixtures/build-memory-processor-fixture.ts` (`QT_FIXTURE_PROCESSOR_MAIN/_MOUNT`) | **MOVES** (exact key). ⚠ a STANDING red already (compressed-text byte parity — status-log :152843 et seq.); the lane must tell the canned-miss red from the standing one |
| `carina_memory_extraction_tier3_equivalence` | `cases/carina-memory-extraction-tier3.test.ts` / `fixtures/carina-memory-extraction-tier3.json` (5 cases; all questions non-empty → ORDERED arm only) | /tmp via `build-carina-memory-extraction-fixture.ts` | **MOVES** (SELF system + heading) |
| `memory_pipeline_jobs_tier3_equivalence` | `cases/memory-pipeline-jobs-tier3.test.ts` / `fixtures/memory-pipeline-jobs-tier3.json` (6 extraction + 4 summary; routes episode calls at :215) | /tmp via `build-memory-pipeline-jobs-fixture.ts`; `TZ=UTC` | **MOVES** (extraction + fold-episode) |
| `fold_episode_tier3_equivalence` | `cases/fold-episode-tier3.test.ts` / `fixtures/fold-episode-tier3.json` | /tmp via `build-fold-episode-fixture.ts` | **MOVES** (A10) |
| `context_summary_service_tier3_equivalence` | `cases/context-summary-service-tier3.test.ts` (answers the fold-episode pass at :218-225) / `context-summary-service-tier3.json` + `-ops.json` | /tmp via `build-context-summary-service-fixture.ts` | **MOVES** (A10) |
| `courier_images_routes_equivalence` | `cases/courier-images-routes.test.ts` (records the fold-episode call by exact key, :166-205) / `fixtures/courier-images-web.json` | see its header | **MOVES** (A10) — not a "memory" family; easy to miss |
| `episodic_equivalence` (`cases/episodic.ts`, imports `lib/memory/episodic` only), `memory_search_extraction` (keyword prompt untouched), `recall_replay`, `precompute`, `turn_transcript_equivalence` (builder, not renderer), `memory_gate_tier3`, `memory_watermark_tier3` | — | — | NEUTRAL |

Heading-arm coverage in `memory-tasks-tier1.json` today (classified by
`userMessage` and any `isUserControlled` slice):

- OLD arm (`userMessage` null, no user slice, no `userCharacterName`): #6
  `self-greeting-no-user-null-item`, #9 `other-autonomous-orienting`, #15
  `self-clock-narrative-in-story` — **covered**.
- ORDERED arm via `userMessage`: #0, #3, #5, #7 (`self-empty-user-message`,
  `''` — the `!== null` trap), #8, #12-14, #16, #17 — **covered**.
- ORDERED arm via user slice: #1, #2 (both also carry `userMessage`; both
  target the user-controlled slice → also exercise `FIRST_PERSON_USER_CLAUSE`).
- **Not covered:** (a) `userMessage: null` + an `isUserControlled` slice
  (ORDERED purely by `hasUserSlice`; unreachable from the builder but the
  renderer's second disjunct — a mutation `|| has_user_slice` → nothing is
  invisible today); (b) `userMessage: null` + `userCharacterName` set + no
  user slice (USER roster line + OLD heading — pins that the heading does
  NOT key on the roster's name). Add both as new `self`/`other` cases in
  `harness/oracle/fixtures/memory-tasks-tier1.json` (shape: copy #6 and set
  the slice `isUserControlled: true` / set `userCharacterName`), plus the
  proposal-no-reply shape (Owen user-controlled opener + Friday + Amy ending
  on `PROPOSAL_LINE`) as an OTHER case with `fridaySubjects` and a SELF case
  targeting Owen, and `INVENTED_ASSENT_RESPONSE` as `responseText` (the §8.5
  "parser does not filter" pin).

Note: `memory-search-extraction.test.ts:12-14` still says memory-tasks-tier1
"stays at its `7e6d13e5` vintage until round 3" — stale (P4.d14 `def4cb968`
regenerated it); harmless.

## §D — traps / premises (measured)

1. **The heading is a BRANCH** — two literals, condition `user_message.is_some()
   || has_user_slice`; both inputs already exist at `:406`/`:440`. Both arms
   and both disjuncts need corpus rows (§C gap a/b).
2. **Two bodies carry AGREEMENTS** (SELF, OTHER); the ledger's third
   "USER-subject" body does not exist. The batch prompt (`batchExtractMemories`
   :1038) and the keyword/recap prompts are untouched.
3. **Anchor numbers are text**, not floats: they live inside raw-string
   prompt bytes; nothing renders `0.55` from an `f64`. The only numeric
   interpolations are the caps (`i64`) and `FOLD_EPISODE_CAP` (`usize` → `2`),
   unchanged. The example `"importance": 0.55` is likewise raw text.
4. **Example order**: OTHER good = [Amy 0.85, Charlie 0.65, Amy-condition
   0.55]; OTHER bad = [six-restatements block] then [assent-invented block];
   SELF unchanged.
5. **The generator aborts at the target** (unsubstituted
   `${AGREEMENTS_INSTRUCTION_BLOCK}` and the `\`` escape). Extend
   `extract-memory-task-prompts.py` (add `AGREEMENTS =
   template_after('const AGREEMENTS_INSTRUCTION_BLOCK = ')` + its substitute
   BEFORE checking leftovers; unescape `\`` → `` ` `` — or evaluate the
   template as the `.cjs` tool does), regenerate, and expect the diff to be
   exactly A1×2 + A2×2 + A3 + A5 + A6 + A7 + A8. Hand-editing
   `prompt_text.rs` would violate its own GENERATED header.
6. **`FOLD_EPISODE_PROMPT` has no generator** (P4.D212's mechanism covers only
   `context_summary/prompt_text.rs`). Either extend the python script to emit
   the episode split too, or hand-insert one line + one blank line in
   `_AFTER_CAP`; the four A10-moving families prove the bytes either way.
7. **`FIRST_PERSON_USER_CLAUSE` is hand-written** with `\` continuations:
   the old `exchange.\n\n` must become `exchange. ` + the new text; keep the
   leading-space rule (Rust `\`-newline strips the next line's leading
   whitespace, so the space must END the previous line).
8. **`hasUserSlice` provenance in v5** = v4's (filter on `is_user_controlled`,
   a plain `bool` defaulting false). `Some("")` → ORDERED (case #7 pins it).
9. **The escaped backtick** is the first backslash in either body — any
   regen path that does not evaluate the template will emit a literal `\`.
10. Memory notes: `byte-exact-static-data-transcription.md` (ship the
    generator, never hand-transcribe); `closing-a-divergence-moves-the-censuses`
    does not apply (no census of these strings found).

## §E — ownership

EDITS:
- `crates/quilltap-core/src/memory_tasks.rs` (`FIRST_PERSON_USER_CLAUSE`,
  `render_turn_context` + `pub const ORDERED_TURN_TRANSCRIPT_HEADING`,
  `FOLD_EPISODE_PROMPT_AFTER_CAP`, the stale doc names).
- `crates/quilltap-core/src/memory_tasks/prompt_text.rs` (regenerated only).
- `harness/oracle/scripts/extract-memory-task-prompts.py`.
- `harness/oracle/fixtures/memory-tasks-tier1.json` (+ the new cases) and,
  if needed, `harness/oracle/cases/memory-tasks-tier1.test.ts` /
  `crates/quilltap-harness/tests/memory_tasks_equivalence.rs`.
- Regen-only (no source edit expected) for `memory_processor_tier3`,
  `carina_memory_extraction_tier3`, `memory_pipeline_jobs_tier3`,
  `fold_episode_tier3`, `context_summary_service_tier3`,
  `courier_images_routes` — their harness tests/cases only if a recipe needs
  touching.
- `docs/CHANGELOG.md`, `docs/developer/porting/status-log.md`.

MUST NOT TOUCH (measured — none hold these bytes): `generators/**`,
`services/build_context.rs`, `services/message_context.rs`, `quilltap-host`,
`quilltap-cli`, `help/`, `builtin_prompt_templates.*` (parts a/b/d/e of
`ca363178d` are other lanes).
