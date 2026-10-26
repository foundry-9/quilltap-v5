# SPA test fixtures

Committed oracle output the SPA's own differentials replay. These are **copies**
of harness oracle output, never hand-edited: if one looks wrong, regenerate it
and investigate the diff — do not patch the fixture.

## `pascal-custom-tool-definition.oracle.ndjson`

_Refreshed 2026-09-09 at v4 `25f534c0b` (the progressions round's unification,
from a pinned worktree): the Pascal lane (P4.D169) grew the shared generator by
40 `definition` + 21 `gate` rows for the `progress` family, and the round's
widened refusal sentences moved twelve pre-existing rows; 301 → 362 rows
(10 `title` + 300 `definition` + 52 `gate`). The SPA lane (P4.D170) had
re-recorded the un-extended generator at the same pin; this copy supersedes it._

_Refreshed 2026-09-05 at v4 `d883a5ee1` (Zod 4.5.4): thirteen rows moved — the
unrecognized-key shapes, now continuable inside unions/refines — and two
astral-title rows added for Zod's code-point length windows (v4 `6e1a64ea6`);
299 → 301 rows._

**362 rows** — 10 `title` + 300 `definition` + 52 `gate`. Drives
`app/pascal/custom-tool-types.corpus.spec.ts`, which replays
every row through the SPA's hand-ported schema module and byte-compares the
verdict, the parsed data (`JSON.stringify`), the unknown-key report, and the
full `formatDefinitionIssues` rejection sentence against v4's REAL Zod output.

The `gate` kind arrived with the `231be14c` round: each row is one
`evaluateToolGate` verdict against one fact sheet, serialized, so `withheldBy`'s
ABSENCE on an available verdict is part of the comparison. The SPA replays them
through its own `tool-gate.ts` — the same rows the Rust half diffs — which is
what proves the two client-safe ports agree with v4 and with each other.

The sentence is compared rather than regex-matched because it is payload:
`loadToolsFromMount` stores it as a load error's `reason`, and both the chat
roster route and the Workbench library route return it verbatim — so a browser
that phrased it differently would be disagreeing with the server about the same
file.

- **Provenance:** v4 `c4d4b0de`, regenerated 2026-08-01 at the `c4d4b0de`
  drift round's unification (P4.D35's §C extension added 63 definition rows —
  7 chipLabel, 21 accepted effects, 32 rejected effects, 3 shape-order — from
  the clean checkout; no pin needed). Byte-identical to the copy the Rust
  `pascal_custom_tool_definition_equivalence` differential consumed at the
  same commit, with that differential re-run green over this exact output.
  The `231be14c` corpus was 236 rows (10 title + 195 definition + 31 gate). The `7e6d13e5` corpus
  was 175 rows (10 title + 165 definition; 58 accept / 107 reject); the
  `6864bf0e` availability-gate drift added **30 definition rows** (the
  `availableWhen`/`withheldWhen` accept/reject arms, the both-clauses rejection,
  the literal-operand rejections, and three rows covering the pre-existing
  `z.record` sites nothing had ever reached) plus the **31 new `gate` rows**.
- **Owner of the generator:** `harness/oracle/cases/pascal-custom-tool-definition.ts`
  (lane AY's tree — this is a consumer copy).
- **Regenerate** (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):

  ```bash
  cd ~/source/quilltap-server            # must be at the pinned v4 baseline
  npx tsx <V5>/harness/oracle/cases/pascal-custom-tool-definition.ts \
    > <V5>/apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson
  ```

  If the v4 tree is dirty, generate from a pinned detached worktree at the
  baseline instead. Expect 362 lines (10 `title` + 300 `definition` + 52 `gate` since the `25f534c0b` round); a shorter file means the generator errored
  and left the old one in place.

## `pascal-progress.oracle.ndjson`

**71 rows** — 38 `definition` + 16 `parseEffectTarget` + 10 `classifyPlaceholder`
+ 7 `evaluateToolGate`. Drives `app/pascal/progress-arms.oracle.spec.ts`: the
`progress` family's arms as the SPA's Pascal twins see them — both gate subjects
with the empty/absent combinations, the four bad-key shapes, `when.progress`,
all ten effect-target arms (the reserved-key pair with `metadata.progressionsNotes`
as the near-miss control), the two new placeholders inside an expression, the
raw `parseEffectTarget` results compared as whole serialized objects, the
`classifyPlaceholder` refs, and gate verdicts against a sheet the spec DERIVES
with `flattenProgressions` and byte-compares to v4's before the gate sees it.

Recorded by the P4.D170 lane from v4's REAL `lib/pascal/*` at `25f534c0b` before
the Pascal lane had extended the shared generator; since the `25f534c0b` round's
unification the shared corpus above carries the same family (its 40 new
definition + 21 gate rows), so this file is the overlapping half — kept because
its `parseEffectTarget` / `classifyPlaceholder` / derived-sheet comparands are
shapes the shared corpus does not record. Folding it into the shared generator
is a named follow-up (`apps/web/oracle/pascal-progress.recorder.ts` header).

- **Provenance:** v4 `25f534c0b`, recorded 2026-09-08 at the P4.D170 lane from
  a detached worktree pinned there, under `zod` 4.5.4.
- **Owner of the generator:** `apps/web/oracle/pascal-progress.recorder.ts`
  (outside `src/` — it imports v4's `@/lib/…`).
- **Regenerate:** the `progressions-schema.oracle.ndjson` recipe below with
  `pascal-progress.recorder.ts` in place of the schema recorder, into
  `src/testing/fixtures/pascal-progress.oracle.ndjson`. Expect 71 lines.

## `progressions-schema.oracle.ndjson`

**71 rows** — 62 `progression` + 9 `progressions`. Drives
`app/progressions/schema.oracle.spec.ts`, which replays every row through the
SPA's hand-ported `progressions/schema.ts` and byte-compares the verdict, the
whole joined rejection sentence (paths and their order included) and, on an
accept, `JSON.stringify` of the parsed data — so the declaration key order and
the omitted optionals are pinned too.

The SPA has no `zod` (measured 2026-09-08: absent from `apps/web/package.json`,
imported nowhere), so that module reimplements the slice of Zod 4.5.4 v4's
`ProgressionSchema` uses. The sentence is compared rather than regex-matched
because it is payload: the progression editor modal renders
`${issue.path.join('.') || 'This entry'}: ${issue.message}` straight to the
author.

- **Provenance:** v4 `25f534c0b` (the character-progressions commit
  `0587d1e96` plus bug 126), recorded 2026-09-08 at the P4.D170 lane from a
  detached worktree pinned there, under `zod` 4.5.4.
- **Owner of the generator:** `apps/web/oracle/progressions-schema.recorder.ts`
  (this lane's own; it lives outside `src/` because it imports v4's `@/lib/…`
  and would not compile in the SPA's tsconfig).
- **Regenerate** (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):

  ```bash
  PIN=/tmp/qt-v4-pin-p4d170-25f534c0b
  git -C ~/source/quilltap-server worktree add --detach "$PIN" 25f534c0b
  ln -sfn ~/source/quilltap-server/node_modules "$PIN/node_modules"
  cp <V5>/apps/web/oracle/progressions-schema.recorder.ts "$PIN/"
  cd "$PIN" && npx tsx progressions-schema.recorder.ts \
    > <V5>/apps/web/src/testing/fixtures/progressions-schema.oracle.ndjson
  ```

  Expect 71 lines; a shorter file means the recorder errored and the redirect
  already truncated the old one.

## `carina-parser.ndjson`

**83 rows** — 49 `parseCarinaQuery` rows (`content` + `out`) and 34
`isCarinaInvocableName` rows (`name` + `out` + `parsedName`). Drives
`app/chat/carina-parser.oracle.spec.ts`.

- **Provenance:** the 49 parse rows were recorded at `f4ad2c8d1` (P4.D181);
  P4.D224 re-recorded the file at v4 `b0b6656b5` from a pinned worktree with the
  34 name rows appended — the first 49 lines are byte-identical to the old file
  (`cmp` on the prefix), because v4 `3376b3dfa` rebuilt `LINE_RE` from
  `NAME_SOURCE` into the same pattern.
- **Recorder:** `apps/web/oracle/carina-parser.recorder.ts` (its header has the
  pinned-worktree invocation). Expect 83 lines.


## `mention-typeahead.ndjson`

**116 rows** — 1 `constants`, 39 `trigger`, 27 `rank`, 9 `candidates`, 34
`classify`, 6 `canKeep`. Drives `app/chat/mentions/mention-typeahead.oracle.spec.ts`:
v4's `lib/mentions/mention-typeahead.ts` (the composer's `@` typeahead, `3376b3dfa`)
over v4's own test vectors plus a wider corpus.

- **Provenance:** recorded by P4.D224 at v4 `b0b6656b5` from a pinned worktree.
- **Recorder:** `apps/web/oracle/mention-typeahead.recorder.ts` (its header has
  the invocation). Expect 116 lines.

## `wear-display.ndjson`

**112 rows** — 32 `formatWornRelative` + 38 `formatWornWhen` + 42
`formatWearLine`, each `{fn, id, args, out}`. Drives
`app/wardrobe/wear-display.oracle.spec.ts`, which replays every row through the
SPA's `wardrobe/wear-display.ts` and byte-compares the phrase (with a row-count
guard so a truncated file cannot pass vacuously). Bug 183 (`ed8b15b50`): the
"last week" / "last month" rungs of the shared day ladder become "a week ago" /
"a month ago" after the wear line's own "last"; the corpus walks every rung and
both sides of each rung edge, the empty / unparseable inputs, and the wear line
for counts 0 / 1 / 2 / 8.

- **Provenance:** v4 `01a83539d` (`4.10.0-dev.143`), recorded 2026-10-09 by the
  P4.D277 lane from a detached pin worktree (`/tmp/qt-v4-pin-<order>-01a83539d`)
  against v4's REAL `lib/wardrobe/wear-display.ts` (which imports the REAL
  `lib/format-time.ts` ladder). A consumer copy — never hand-edited.
- **Owner of the recorder:** `apps/web/oracle/wear-display.recorder.ts`.
- **Regenerate** (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`; the full
  recipe, with the pin variable, is the recorder's header):

  ```bash
  cp <V5>/apps/web/oracle/wear-display.recorder.ts "$PIN/"
  rm -f <V5>/apps/web/src/testing/fixtures/wear-display.ndjson
  cd "$PIN" && npx tsx wear-display.recorder.ts \
    > <V5>/apps/web/src/testing/fixtures/wear-display.ndjson
  ```

  Expect 112 lines, with `grep -c 'a week ago'` and `grep -c 'a month ago'`
  both > 0; a shorter file means the recorder errored and the redirect already
  truncated the old one.

## `memory-maintenance-text.ndjson`

_Recorded 2026-10-14 at v4 `83334b9a5` by P4.D300 (Tier 2 item 21), from the
lane's pinned worktree._

**29 rows** — 7 constants (`MEMORY_SETTING_DEFAULTS.{maintenance,
consolidation, extractionLimits}`, each its schema's REAL `.parse({})`;
`MEMORY_CAP_BOUNDS`; `IMPORTANCE_HIGH` / `IMPORTANCE_LOW`;
`MAINTENANCE_PHASES`; the report's `PHASE_LABELS`) + 10 `importanceBand` edges
(0 … 0.29, 0.2999, 0.3, 0.6999, 0.7 … 1) + 12 `summaryLine` arms (both skips,
each part alone, the skipped-consolidation arms incl. an empty reason, big
demote numbers with no `toLocaleString`, every part). Recorded by
`apps/web/oracle/memory-maintenance-text.recorder.ts` through `npx tsx` from the
pin — v4's two client components (`memory-maintenance-ledger.tsx`,
`maintenance-report.tsx`) load under `tsx`, so `summaryLine` and `PHASE_LABELS`
are v4's own, not reimplemented. Drives
`app/screens/settings/memory/memory-maintenance-text.oracle.spec.ts` (objects
compared through `JSON.stringify` — key order is part of the value).


## The `25b8fab6a` refresh — `pascal-custom-tool-definition.oracle.ndjson` + `carina-parser.ndjson` (P4.D307)

_Recorded 2026-10-16 at v4 `25b8fab6a` (`4.11.0-dev.10`) by P4.D307 (the
Pascal SPA lane), from the lane's pinned worktree
(`/tmp/qt-v4-pin-p4d307-25b8fab6a`), Node 24.13.1, the pin's resolved `zod`
**4.6.7**._

**`pascal-custom-tool-definition.oracle.ndjson` — 362 → 425 rows** (10 `title`
+ 363 `definition` + 52 `gate`), sha-256 `633c837f33bd59fa4be0bda3d00001cca6c39a71680fb57594378bf7a1398f3f`.
**Superseded at the round's unification (2026-10-17):** the generator's three
lane blocks were merged BY ID and the corpus regenerated ONCE from the
`25b8fab6a` pin for both consumers — **449 rows** (10 `title` + 387
`definition` + 52 `gate`), sha-256
`48765f04b4132131f044758e116afb59fb5338000aeacfe86f0c82abf4780427`. Three ids
this block recorded over a different input than the server lanes' kept the
server input; this block's input survives under a suffixed id
(`-wrong-types`, `-bogus-key`, `-literal-value`).
v4 `362426c8f` (`triggers`) and `f0db8cbb8` (the write | wardrobe effect
union) grew the format; the shared generator
`harness/oracle/cases/pascal-custom-tool-definition.ts` gained ONE fenced block,
`// ---- P4.D307 rows (the SPA twin's; MERGE BY ID with P4.D305 §4.1 / P4.D306
§4.1 at unification) ----`, of 63 definition rows under the two server surveys'
row names (20 `triggers-*`, 30 `effects-wardrobe-*`, the effect-union /
shape-order / known-key rows, and nine Zod 4.6.7 length-check rows). Two OLDER
rows moved by design: `effect-target-missing` / `effect-value-missing` now read
`a write needs both target and value` (the write halves became optional). No
older row moved from the Zod 4.6.5 → 4.6.7 bump. Main's unmodified generator
re-run at the pin: 362 rows, those two rows the only diff. **The unifier's one
merged regen supersedes this copy** (P4.D305 and P4.D306 grow the same
generator on their branches; a same-id row with different bytes is a finding).

**`carina-parser.ndjson` — 83 → 144 rows**, sha-256 `87a2e489679d7744cb19fcafeaaedf466bc343d848e3c87957933b9d34a3b86a`. v4 extracted
`parseCarinaQuery`'s loop body into `parseQueryLine` and added
`stripCarinaQueryLines`; the recorder appends 61 rows of a new `{ id, kind:
'strip', content, out }` kind AFTER the 34 name rows — twelve hand-written
vectors plus the strip of every parse vector. The 83 older rows (no `kind` key)
re-record BYTE-IDENTICAL, which is the proof the extraction changed no parse.

Recipes (canonical; stage under a lane-private dir, `rm -f` the target first):

```bash
PIN=/tmp/qt-v4-pin-<order>-25b8fab6a          # git worktree add --detach "$PIN" 25b8fab6a
V5W=${V5W:-$HOME/source/quilltap-v5}
cd "$PIN" && TZ=UTC npx tsx "$V5W/harness/oracle/cases/pascal-custom-tool-definition.ts" \
  > "$V5W/apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson"
cp "$V5W/apps/web/oracle/carina-parser.recorder.ts" "$PIN/" && cd "$PIN" && \
  npx tsx carina-parser.recorder.ts > "$V5W/apps/web/src/testing/fixtures/carina-parser.ndjson"
```

Expect ≥ 425 and exactly 144 lines; `grep -c '"id":"triggers-'` ≥ 20,
`grep -c '"id":"effects-wardrobe-'` ≥ 18, `grep -c '"kind":"strip"'` = 61.


## `slash-commands.ndjson`

**186 rows** — 1 `constants`, 54 `trigger`, 107 `rank`, 24 `build`. Drives
`app/chat/slash-commands/slash-commands.oracle.spec.ts`: v4's `lib/slash-commands/`
(the composer's `/` slash commands, `4877fc2da`) over v4's own test vectors
(`lib/slash-commands/__tests__/slash-commands.test.ts`) plus a wider corpus —
every opener context (incl. a soft `\n` and an image leaf U+FFFC), the alphabet
and 64/65 length edges, uppercase folding, each `WORD_SPLIT` separator and a run,
kind-order and label ties, `sensitivity: 'base'`, the limit at 0 / 12 / 13 and
the bare-slash no-limit rule, the `/coin` action-before-custom tie, and the
registry's skip rules for built-ins (`available` / `userInvocable` undefined,
empty and duplicate ids, every `parameters` shape) and customs (`asCharacterId`
absent / `null` / a string, duplicate keys, an empty name). `build` rows are
compared through `JSON.stringify` (key order is part of the value).

- **Provenance:** recorded 2026-10-16 by P4.D308 at v4 `25b8fab6a` (the round
  pin, `4.11.0-dev.10`) from a pinned worktree, Node 24.13.1 —
  `localeCompare` / `toLocaleLowerCase` are Node's ICU on both sides.
- **Recorder:** `apps/web/oracle/slash-commands.recorder.ts` (its header has
  the invocation). Expect 186 lines; a second run reproduces the file byte
  for byte (`cmp`).


## P4.D320 — the Pascal SPA pair (`wearable-ref`, `wardrobe-vantage`, `announcement-sender-name`, `pascal-character`)

Four tier-1 corpora recorded 2026-10-19 by P4.D320 at the round pin v4
`e8e17b2d9` (`4.11.0-dev.26`, Zod 4.6.7) from a lane-unique detached worktree,
Node 24.13.1, each through v4's REAL module. Every recorder lives under
`apps/web/oracle/` with its invocation in its header (`cp` it into the pin,
`npx tsx` it there, redirect into this directory, `rm -f` the output first);
a second run reproduces each file byte for byte (`cmp`, measured).

| Corpus | Rows | sha256 | v4 code | Consumer |
|---|---|---|---|---|
| `wearable-ref.oracle.ndjson` | 53 — 31 `resolveWearableRef`, 10 `matchWearableTitle`, 12 `isTemplatedWearableRef` | `9a2de740d1b2a0eb9af00d519911eec30376edcff8addf2e6d823e4075c87d34` | `lib/wardrobe/wearable-ref.ts` (`5cb0e0190`) | `app/wardrobe/wearable-ref.oracle.spec.ts` |
| `wardrobe-vantage.oracle.ndjson` | 40 — 20 `defaultWardrobeVantage`, 20 `wardrobeReachLabel` | `2f2db54ec162bb5ac785eaef33ee38e9dfd31f56195f6c0833d75d347827c3ad` | `lib/pascal/tool-draft.ts` + `lib/wardrobe/wardrobe-container.ts` (`5cb0e0190`) | `app/pascal/wardrobe-vantage.oracle.spec.ts` |
| `announcement-sender-name.oracle.ndjson` | 32 — 20 announcer rows, 12 Staff-fallback rows | `d9f75b6056ff19ad69314329ba4106c11f252438d2830220642281d5f167894b` | `app/salon/[id]/components/system-message-labels.ts` (`a05c6f755`) | `app/chat/system-message-labels.oracle.spec.ts` |
| `pascal-character.oracle.ndjson` | 18 — 12 `definition`, 6 `placeholder` | `0c1f6c56ad5bd1092be6247116be8b060020c0177651d47bdedce9fe8fa2c4e8` | `lib/pascal/custom-tool.types.ts` + `lib/pascal/placeholders.ts` (`a05c6f755`) | `app/pascal/character-arms.oracle.spec.ts` |

- **The shared definition corpus (R-F).** `pascal-custom-tool-definition.oracle.ndjson`
  was re-recorded from the same pin through `main`'s generator
  (`harness/oracle/cases/pascal-custom-tool-definition.ts`, `TZ=UTC`): 449 rows,
  byte-identical to the committed copy, sha256
  `48765f04b4132131f044758e116afb59fb5338000aeacfe86f0c82abf4780427` — nothing
  moved at that lane's close. **Superseded at the `e8e17b2d9` round's
  unification (2026-10-19):** the generator (grown by P4.D319's fenced
  `announceAsCharacter` + `{{character}}` block) was regenerated ONCE from the
  `e8e17b2d9` pin for both consumers — **458 rows**, sha256
  `888f32ad231cc104ceb91d1d07cff8777a7bde33ed5a64a981c6048b515f639e`, the sha
  P4.D319 recorded, so the server family and this copy agree (R-F) — nothing
  moved, so the copy was not re-committed. The unifier's one regen of the
  generator P4.D319 grows supersedes this copy.
- **`pascal-character` retires at union.** Its definition row ids are the ones
  P4.D319's grown shared generator carries (`announce-as-character-*`,
  `effects-value-character-concat`, `effects-wardrobe-character-ref`); once the
  unifier's regen carries them, a same-id row whose bytes differ between the two
  files is a finding, and retiring this recorder is the unifier's call (the
  `pascal-progress` precedent). Measured against P4.D319's lane branch
  (`claude/p4.d319-pascal-server-workbench-wardrobe-lens-2efa2e`, its grown
  generator run from the same pin): the four `announce-as-character-true` /
  `-false` / `-string` / `-null` rows are byte-identical; two of its ids pose a
  DIFFERENT input than this lane first did, so this lane's rows ride suffixed
  ids (`unknown-keys-announce-as-character-known-persist`,
  `announce-as-character-issue-order-reversed`) and both inputs stay pinned.

## The `322a5488e` refresh — `pascal-custom-tool-definition.oracle.ndjson` (P4.D333)

_Recorded 2026-10-26 at v4 `322a5488e` ("Fix: custom-tool LLM consult can
narrate a verdict the table already reached (bug 257)") by P4.D333 (the
custom-tool grammar twins lane), from a v4 worktree pinned at that sha, Node
24.13.1, the pin's resolved `zod` **4.6.7**._

**`pascal-custom-tool-definition.oracle.ndjson` — 458 → 537 rows** (10 `title`
+ 457 `definition` + 70 `gate`), md5 `e15a19fd1f20e33aba1a089f4795bcb7`,
sha-256 `73b2075ef4bfd80414addad1eb3af94e39d0e74c288d40ac011312d3c74b8803`.
v4 `a53024d3c` (any-of gates, outcome `when`s and effect `when`s; record keys
rendered by the key schema's own reason) and `322a5488e` (bug 257 — an
outcome's `brief`, `{{outcome.*}}` in the consult prompt) moved the format;
the shared generator `harness/oracle/cases/pascal-custom-tool-definition.ts`
grew the `anyof-*`, `brief-*` and `heldGroup` gate rows, and the older
`Invalid key in record` rows moved to their key schemas' sentences by design.

This is the ONE regen of the round (R-F): sibling lanes that grow the same
generator merge their rows BY ID, and the unifier re-runs the recipe once over
the merged generator (a same-id row whose bytes differ is a finding). The SPA
copy is byte-equal to the server family's.

Recipe (canonical; from a v4 worktree pinned at `322a5488e`, `rm -f` the
target first):

```bash
cd ~/source/quilltap-server && TZ=UTC npx tsx "$V5W/harness/oracle/cases/pascal-custom-tool-definition.ts" > /tmp/oracle-pascal-definition.ndjson
cp /tmp/oracle-pascal-definition.ndjson "$V5W/apps/web/src/testing/fixtures/pascal-custom-tool-definition.oracle.ndjson"
```

(`~/source/quilltap-server` stands for the PINNED worktree — `git worktree add
--detach <dir> 322a5488e` — never the live checkout.) Expect exactly 537
lines; `grep -c '"kind":"gate"'` = 70. The consumer is
`app/pascal/custom-tool-types.corpus.spec.ts`, green whole with no parse-row
carve; its draft round-trip leg skips the any-of gate rows by name
(`DRAFT_ROUNDTRIP_PENDING_P4D334`) until P4.D334 widens the draft layer.

**`pascal-progress.oracle.ndjson` — re-recorded alongside (71 rows, 12 moved).**
Same recorder (`apps/web/oracle/pascal-progress.recorder.ts`, unchanged), run
at the same `322a5488e` pin: twelve gate / `when` rows move to the pin's
sentences (the any-of union's `availableWhen: …` hoisting, the four-subject
"must test …" texts). Its only consumer is `progress-arms.oracle.spec.ts`.
