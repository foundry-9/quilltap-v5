# SPA test fixtures

Committed oracle output the SPA's own differentials replay. These are **copies**
of harness oracle output, never hand-edited: if one looks wrong, regenerate it
and investigate the diff — do not patch the fixture.

## `pascal-custom-tool-definition.oracle.ndjson`

_Refreshed 2026-11-03 at the `236c04e35` pin (P4.D345 — the round's ONE
regen, R-F): 31 `definition` rows for v4 `839575d89`'s operator intervention
(the `intervention` block — the 18th known top-level key — and `intervene` on a
parameter and on an effect; 15 accepted, 16 refused), in the fenced
`=== P4.D345 rows ===` block after the P4.D329 rows. The 716 pre-existing rows
re-recorded byte-identical at the pin. 716 → **747 rows** (10 `title` + 638
`definition` [224 accept / 414 reject] + 99 `gate`); md5
`99a010307e9c4602e7786d6cfb5db473`, sha256
`4ab4d8fd8ab5c7523a2d5e4d75a2d6933e7107187a9730d6a33e268898167f14`. One row
(`parameter-intervene-unknown-beside`) records that v4's parameter schema is a
plain `z.object` — an unknown key beside a parameter is STRIPPED, not refused
(the SPA twin's parameter parse must agree — P4.D346)._

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

_Refreshed 2026-11-07 at the round pin `624d0d25c` (`4.13.0-dev.8`) by P4.D385
for v4 `db6190b44`'s `deus` composer action: `ALL_ACTIONS` gained `'deus'` in
v4's test's position, plus ten new rows (`build: action deus alone` and nine
`rank: deus …` rows over its keywords and a `de…` tie). 186 → **196 rows**. The
31 pre-existing rows that moved are EXACTLY the ones whose INPUT carries the
new action (`constants`, `build: v4 all actions`, `build: mixed`, every
`rank: mixed …` and `rank: all actions bare`): with the `action:deus` command
stripped from input and output each equals its old bytes, except `rank: mixed
"d" limit 3`, where `deus` takes one of the three slots (`library` drops out)
— measured, no other v4 drift._

**196 rows** — 1 `constants`, 54 `trigger`, 116 `rank`, 25 `build`. Drives
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
  the invocation). Expect 196 lines; a second run reproduces the file byte
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
carve; its draft round-trip leg runs the any-of gate rows group by group
(the `DRAFT_ROUNDTRIP_PENDING_P4D334` carve was lifted at the `322a5488e`
unification, once P4.D334's draft layer landed).

**`pascal-progress.oracle.ndjson` — re-recorded alongside (71 rows, 12 moved).**
Same recorder (`apps/web/oracle/pascal-progress.recorder.ts`, unchanged), run
at the same `322a5488e` pin: twelve gate / `when` rows move to the pin's
sentences (the any-of union's `availableWhen: …` hoisting, the four-subject
"must test …" texts). Its only consumer is `progress-arms.oracle.spec.ts`.

## P4.D338 — `scenario-sort.oracle.ndjson`

The scenario picker's one comparator (`1d0a31132`, pinned at `322a5488e`). Recorded
by `apps/web/oracle/scenario-sort.recorder.ts` against v4's REAL
`lib/scenarios/sort.ts`, over the same grid the Rust family
(`harness/oracle/cases/scenario-pure.ts`) replays.

| Fixture | Rows | sha256 | v4 source | Spec |
|---|---|---|---|---|
| `scenario-sort.oracle.ndjson` | 61 — 30 `scenarioSortKey`, 24 `compareScenarios` (sign), 7 `sortScenarios` (index order) | `e511d04d67172ecaac2e1ec0fbc2cf5ae4ca54adcaa0c085a01b2680c9aa3ae6` | `lib/scenarios/sort.ts` (`1d0a31132`) | `app/scenario/scenario-sort.oracle.spec.ts` |

Regenerate (the recorder header has the full recipe): copy the recorder into a pin
worktree at `322a5488e`, run it with `LANG=en_US.UTF-8` under Node 24 (v4's
`localeCompare(…, undefined, …)` is the HOST default locale), `rm` the copy. The
spec compares the host's `localeCompare` to the recording, so run `npm test` under
an en-US host as well.



## P4.D330 — the Inventory SPA (`inventory-display`, `inventory-refusal`, `inventory-action-summary`)

Three tier-1 corpora recorded 2026-10-26 by P4.D330 at the round pin v4
`322a5488e` (`4.11.0-dev.53`) from a lane-unique detached worktree
(`/tmp/qt-v4-pin-p4d330-322a5488e`), Node 24.13.1, `TZ=UTC`, each through v4's
REAL module. Every recorder lives under `apps/web/oracle/` with its invocation
in its header (`cp` it into the pin as `zz-p4d330-<name>.recorder.ts`, `npx tsx`
it there, redirect into this directory, `rm -f` the output first); a second run
reproduces each file byte for byte (`cmp`, measured).

| Corpus | Rows | sha256 | v4 code | Consumer |
|---|---|---|---|---|
| `inventory-display.oracle.ndjson` | 67 — 14 `inventoryDisplayName`, 14 `inventoryLabel`, 5 `isKin`, 6 `isEquippable`, 10 `kitState`, 7 `inventoryOriginLabel`, 11 `containerOfOrigin` | `22d38e39ebac3912fc852455d2bcec5d9eb2374d63746418d79f1e36e629c21a` | `lib/inventory/resolve.ts` + `lib/inventory/container.ts` | `app/inventory/inventory-display.oracle.spec.ts` |
| `inventory-refusal.oracle.ndjson` | 24 | `167b9f356b10108e50df9aad8c9dd85bf5d0ae9edf654507954bc1d40cdcb971` | `components/inventory/hooks/useInventoryItemActions.ts` `describeInventoryRefusal` + `lib/hooks/use-inventory.ts` `InventoryRequestError` | `app/inventory/inventory-refusal.oracle.spec.ts` |
| `inventory-action-summary.oracle.ndjson` | 58 — 19 `withArticle`, 6 `inventoryRowLabel`, 33 `buildInventoryActionSummary` | `9cc04322501b21136ed5e785fa4d3b39985c6ef9331c6b429e7f157db455f618` | `components/chat/ToolMessage.tsx:175-245` (module-private — through an exporting copy) | `app/chat/inventory-action-summary.oracle.spec.ts` |

- **The `ToolMessage` export patch.** The three functions are module-private
  in v4. The recorder imports a copy of v4's file placed BESIDE the original
  inside the pin (`components/chat/zz-p4d330-ToolMessage.export.tsx`, so its
  relative imports resolve from the pin's tree) with exactly one appended line,
  `export { withArticle, inventoryRowLabel, buildInventoryActionSummary };` —
  `diff` against the original shows only that line. The copy is deleted with
  the pin.
- **The recorded pair (R-6).** Two rows read `an unicorn horn` — v4's client
  rule (`/^[aeiou]/i`); the server's Pascal line spells it `a unicorn horn`.
- **A `refusal` row's input** is rebuilt on the SPA side as the
  `CoreDispatchError` the transports throw (`code` + `details`, R-L), where v4
  threw `InventoryRequestError(message, 400, code, details)`.
- `kitState`'s resolver travels as a plain map `{ id: member }`, wrapped into
  a lookup on both sides.



## P4.D334 — the Workbench's draft model and Inventory evaluators (pin `322a5488e`)

Two tier-1 corpora recorded 2026-10-26 by P4.D334 at the round pin v4
`322a5488e` (`4.11.0-dev.53`) from a lane-unique detached worktree, Node 24.13.1,
each through v4's REAL modules. Each recorder lives under `apps/web/oracle/` with
its invocation in its header (`cp` it into the pin, `npx tsx` it there, redirect
into this directory, `rm -f` the output first); a second run reproduces each file
byte for byte (`cmp`, measured). Every row carries `needs` — the sibling lane
whose code the row's truth reads (`P4D333`: the list rule, `compareOrdered`,
`splitInventoryReference`, the parse; `P4D330`: `kitState`). Both siblings
landed at the `322a5488e` unification, which deleted the gate module and its
stand-ins; every row now replays live.

| Corpus | Rows | sha256 | v4 code | Consumer |
|---|---|---|---|---|
| `tool-draft-grammar.oracle.ndjson` | 274 (re-recorded at the `236c04e35` pin by P4.D346) — 10 `groupsOf`, 182 `draft`, 5 `rename`, 5 `find`, 17 `definition`, 55 `direct`; by `needs`: 212 live, 62 `P4D333` | `3b8aa25a3e7dece768151886f9b7468dfedcea194f81708633d0f22dd0528306` | `lib/pascal/tool-draft.ts` (`a53024d3c`, `322a5488e`, `e1872c94f`, `e4a60ad9e`, `839575d89`; pin `236c04e35`) | `app/pascal/tool-draft-grammar.oracle.spec.ts` |
| `pascal-inventory-twins.oracle.ndjson` | 636 — 46 `parseAggregateKey`, 26 `isListAggregate`, 26 `aggregateKeyEndsInFold`, 90 `inventoryAggregateHolds`, 10 `parseInventoryOperand`, 31 `resolveInventoryValue`, 30 `inventoryStateHolds`, 36 `inventoryCountHolds`, 9 `gateNamesInventory`, 12 `mockInventoryContext`, 320 `resolveInventoryRef`; by `needs`: 536 live, 80 `P4D333`, 20 `P4D330` | `0407da9b23816519fbdbec8ad345ea69dd6f95d3c521738309bd666347e31e5f` | `lib/pascal/{inventory-aggregate,inventory-subjects,bench-inventory}.ts` + `lib/inventory/refs.ts` | `app/pascal/inventory-twins.oracle.spec.ts` |

- **The draft inputs are draft JSON, ids normalised.** Every `nextDraftId` value
  is mapped to `<prefix>#<n>` by first appearance (draft first, then the
  outputs) before it is written, so v5 feeds the same object to
  `definitionFromDraft` / `validateDraft` without going through the parse and
  the `draft` rows run live on a branch that lacks P4.D333's twin.
- **The shared definition corpus is NOT touched.**
  `pascal-custom-tool-definition.oracle.ndjson` is P4.D333's and was neither read
  nor regenerated by this lane (R-F): `git diff main --` on it is empty. Its
  `DRAFT_ROUNDTRIP_PENDING_P4D334` carve (the draft round trip over array clauses
  and the five gate records) was lifted at the unification on this lane's code; the
  same law is proved here over hand-written gates in `tool-draft.spec.ts`.


## P4.D336 — raise a hand / stop (no fixture)

This lane vendors no NDJSON: v4 shipped no test for the composer's buttons, the
hook's `stopStreaming` / `toggleRaisedHand`, or the frame reader, so the corpora
are the v5 specs (composer, reducer, salon vertical) written from the `2b5e8ee24`
hunks. The one v4 case, `ParticipantCard.test.tsx`'s "shows the Speaking button,
not a Stop button, while this participant is generating", is transcribed name for
name. `e2e/salon-interrupt-flow.spec.ts` carries a spec-local sequenced mock
(`createServer`; `/models`, then SSE word deltas `plan.delayMs` apart, the reply
taken from `plan.replies[callIndex]`); `e2e/support/mock-llm.ts` is untouched.


## P4.D344 — Storyteller mode SPA (no fixture)

This lane vendors no NDJSON. v4 shipped ONE client test for Storyteller mode,
`__tests__/unit/components/chat/storyteller-board-arrange.test.ts` (four
`arrangeBids` cases), transcribed name for name into
`chat/storyteller-board.pure.spec.ts`. Every other surface — the board, the
Turn mode radiogroup, the turn-mode hooks, the frame fold, the Inspector filter
— is a v5 spec written from the `b15fe9404` / `946a50a04` hunks at the pin
`236c04e35` with v4's strings byte for byte, against wire contract W1 stubbed.

`e2e/salon-storyteller-flow.spec.ts` carries a spec-local BID-AWARE mock
(`createServer`; `/models`; on `/chat/completions` a message carrying both
`[Turn note from the Salon — not spoken by any character]` and `answer only
with your bid` streams the four bid lines, one carrying `The storyteller chose
you to go ahead` streams `plan.turnReply`, anything else `plan.reply`; SSE word
deltas `plan.delayMs` apart). `e2e/support/mock-llm.ts` is untouched.

## P4.D346 — the Pascal intervention prose (`intervention-summary`) and the `tool-draft-grammar` re-record

Recorded at the round's pin `236c04e35` from a lane-unique detached v4 worktree
with its OWN `npm ci --offline` (zod 4.6.7) — never the live checkout, which is
past the pin. Each recorder carries its invocation in its header (`cp` it into
the pin, `npx tsx` it there, redirect into this directory, `rm -f` the output
first).

- `intervention-summary.oracle.ndjson` drives v4's REAL
  `getInterventionSummary` (the "adjusted" badge's tooltip, `839575d89`) over
  v4's own twelve test cases and twelve edges, and v4's REAL `describeSpan`
  (`lib/pascal/intervention/spans.ts`, `8ae8a127c` — the hold dialog's span
  hints) over fourteen spans. Every row is live (no `needs`).
- `tool-draft-grammar.oracle.ndjson` was RE-RECORDED WHOLE (P4.D346 R-8):
  `839575d89`'s `newDraft()` seeds `intervention`, so all 244 earlier rows'
  draft inputs / parsed drafts moved by that key. Stripping the `intervention`
  key from both the `322a5488e` copy and the new one leaves ZERO differences
  over the 244 earlier ids; the thirty P4.D346 rows ride a fenced block at the
  recorder's end.

| Corpus | Rows | sha256 | v4 code | Consumer |
|---|---|---|---|---|
| `intervention-summary.oracle.ndjson` | 38 — 24 `getInterventionSummary`, 14 `describeSpan` | `52d90e1aa20edb639d5a698bddbe43aed179b1f83171ca2105b0d53eaafbc7e9` | `app/salon/[id]/components/system-message-labels.ts` (`839575d89`) + `lib/pascal/intervention/spans.ts` (`8ae8a127c`); pin `236c04e35` | `app/chat/system-message-labels.intervention.oracle.spec.ts` |

## P4.D353 — diagrams (no fixture)

This lane vendors no NDJSON: v4 proved the diagram feature with a jsdom suite only,
so the corpora are its ten test files transcribed case for case into vitest specs
(`diagrams.test.ts` 112 lines, `sanitize-svg` 97, `render-svg` 30, `DiagramBlock` 197,
`DiagramLightbox` 113, `lightbox-geometry` 83, `scroll-anchor` 84, `export-png` 158,
`LazyMessageContent.diagrams` 50 — the arms that survive v5's lack of a fast path —
and `message-row-diagram-style` 37), the renderers stood in through the
`DIAGRAM_RENDERERS` / `DIAGRAM_PNG_COPY` tokens as v4 mocks the modules.
`e2e/diagrams-flow.spec.ts` seeds four Aria rows into Solo Voyage on its own copy of
the salon pair (a mermaid flowchart, a dot graph, a mermaid fence that does not parse,
a reasoning-only mermaid fence) and streams one through `startMockLlm`. R-B: (A).
||||||| parent of 45c7f57c6 (feat(spa): the pronoun presets and wardrobe pronoun-guidance twins, recorded from v4 (v4 eb90d9ed8))


## P4.D359 — `pronoun-guidance.oracle.ndjson` (the wardrobe pronoun guidance)

Recorded 2026-11-03 at the pin `236c04e35` (a detached worktree, root
`npm ci --offline`) through v4's REAL `lib/wardrobe/pronoun-guidance.ts` +
`lib/characters/pronoun-presets.ts` (`eb90d9ed8`). Their import graph is v4's
own `lib/` only (the `Pronouns` / `PronounGuidance` imports are type-only and
erased by tsx; no `zod` is loaded).

| Corpus | Rows | sha256 | v4 code | Consumer |
|---|---|---|---|---|
| `pronoun-guidance.oracle.ndjson` | 131 — 22 `normalize`, 13 `subjectOf`, 6 `label`, 70 `fits`, 11 `filter`, 9 `format` | `f0450eab48d73d7a8d5a1767da8403006b929ad1c957b4e3dd9f87b81386b7a9` | `lib/wardrobe/pronoun-guidance.ts` + `lib/characters/pronoun-presets.ts` (`eb90d9ed8`) | `app/wardrobe/pronoun-guidance.corpus.spec.ts` |

- **Row shape:** `{kind, args, expected}` — `args` is the argument list as JSON
  (`"__undefined__"` for an `undefined` argument, `{"$set": [...]}` for a
  `Set`), `expected` the `JSON.stringify`'d result.
- **Regenerate** (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):

  ```bash
  cd <pin> && QT_V4_ROOT=$PWD PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
    npx tsx <V5>/apps/web/tooling/record-pronoun-guidance.mts \
    > <V5>/apps/web/src/testing/fixtures/pronoun-guidance.oracle.ndjson
  ```
- **Cross-check (free):** P4.D349's Rust `pronoun_guidance` family records its own
  oracle from the same two modules; the unifier may diff the `normalize` rows.

## P4.D359 — `mention-markers.oracle.ndjson` (the Salon's name chips)

Recorded 2026-11-03 at the pin `236c04e35` through v4's REAL
`lib/chat/mention-markers.ts` + `lib/chat/name-mentions.ts` (`9a88f862f`), whose
import graph is v4's own `lib/` only (`lib/utils/regex.ts`).

| Corpus | Rows | sha256 | v4 code | Consumer |
|---|---|---|---|---|
| `mention-markers.oracle.ndjson` | 59 — 27 `matcher`, 19 `rehype`, 7 `split`, 6 `assign` | `1cad4bd4764e60bc07c688a928787146ed83dfc256171e23c0b2e8f5cfd7536e` | `lib/chat/mention-markers.ts` + `lib/chat/name-mentions.ts` (`9a88f862f`) | `app/chat/absent-mentions/mention-markers.corpus.spec.ts` |

- **Row shape:** `{kind, input, expected}`, `expected` the `JSON.stringify`'d
  output (`matcher` → `{usable, first, last}`; `rehype` → the hast tree after
  v4's plugin; `split` → the runs; `assign` → per-run mentions, `undefined` as
  `null`).
- **Regenerate** (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):

  ```bash
  cd <pin> && QT_V4_ROOT=$PWD PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
    npx tsx <V5>/apps/web/tooling/record-mention-markers.mts \
    > <V5>/apps/web/src/testing/fixtures/mention-markers.oracle.ndjson
  ```
- **The post-render pass is not in the corpus.** `markMentionsInElement`
  (`MENTION_MARKERS_POST_RENDER`, ruling R-2) is v5's own walk over the
  rendered DOM; `mention-markers.spec.ts` proves it on v4's `MentionChip.test.tsx`
  shapes through v5's real renderer.
- **Cross-check (free):** P4.D347's Rust `name_mentions_equivalence` records its
  own oracle from the same `name-mentions.ts`; the unifier may diff the `matcher`
  rows.

## P4.D384 — `breadcrumb-density.oracle.ndjson` (the compact header breadcrumb)

Recorded 2026-11-07 at the pin `624d0d25c` (`/tmp/qt-v4-pin-p4d384-624d0d25c`,
a detached worktree with its OWN `npm ci --offline --ignore-scripts` — never a
symlink to the live checkout's `node_modules`) through v4's REAL
`components/chat/ChatHeaderBreadcrumb.tsx` (`3c41cf3da`). tsx loads the
`.tsx` module's whole import graph (`Icon`, `Tooltip`, `CopyChatIdButton`,
React) without a DOM, so no jest capture was needed.

| Corpus | Rows | sha256 | v4 code | Consumer |
|---|---|---|---|---|
| `breadcrumb-density.oracle.ndjson` | 78 — 52 `abbreviate`, 26 `density` | `f3335df83f830e7ec821383a079b5e0f65e8ecc5975c9eaeb251c001499febd1` | `abbreviateName` / `chooseDensity` in `components/chat/ChatHeaderBreadcrumb.tsx` (`3c41cf3da`) | `app/chat/breadcrumb-density.oracle.spec.ts` |

- **Row shapes:** `{id, kind: 'abbreviate', name, out}` and `{id, kind:
  'density', available, widths: {full, abbrev, avatar}, out}`; a non-finite
  number is spelled `"NaN"` / `"Infinity"` / `"-Infinity"` (the spec decodes
  with `Number`).
- **Corpus:** v4's own ten jest assertions (`__tests__/unit/components/chat/
  ChatHeaderBreadcrumb.test.tsx`) plus astral letters and emoji, combining
  marks, the 4- vs 5-code-point boundary, NBSP / tab / newline / U+3000
  separators (and U+200B, which is not `\s`), all-minor and mixed-case minor
  words, leading / trailing minors, the four-initial cap, a growing upper-case
  (`ß` → `SS`), empty / whitespace; `chooseDensity` at every `<=` boundary,
  zero / `NaN` / infinite / negative widths and `available`.
- **Regenerate** (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):

  ```bash
  PIN='/tmp/qt-v4-pin-p4d384-624d0d25c'
  ls -ld "$PIN" && git -C "$PIN" rev-parse HEAD
  F=<V5>/apps/web/src/testing/fixtures/breadcrumb-density.oracle.ndjson
  rm -f "$F"
  cp <V5>/apps/web/oracle/breadcrumb-density.recorder.ts "$PIN/"
  cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
    npx tsx breadcrumb-density.recorder.ts > "$F"
  wc -l < "$F"   # 78
  ```

## P4.D384 — `message-navigation.oracle.ndjson` (the `?msg=` reveal href reader)

Recorded 2026-11-07 at the pin `624d0d25c` (`/tmp/qt-v4-pin-p4d384-624d0d25c`, a
detached worktree with its OWN `npm ci`, never a live `node_modules` symlink)
through v4's REAL `lib/chat/message-navigation.ts` (`4f5047241`). Its import
graph is empty: the module is self-contained.

| Corpus | Rows | sha256 | v4 code | Consumer |
|---|---|---|---|---|
| `message-navigation.oracle.ndjson` | 42 — 41 `out`, 1 `threw` | `deed6ab424e0283edded566997ecee8819e71111c6c0db5f1a0ff57fbd4090db` | `lib/chat/message-navigation.ts` `revealRequestFromSalonHref` (`4f5047241`) | `app/chat/message-navigation.oracle.spec.ts` |

- **Row shape:** `{id, href, out}` (`out` = `{chatId, messageId}` or `null`) or
  `{id, href, threw}` — a malformed `%` escape in the chat id makes v4's
  `decodeURIComponent` throw `URIError: URI malformed`, uncaught; the twin
  matches.
- **Vectors:** v4's own test hrefs (`message-navigation.test.ts:80-94`) + `new`
  (with a slash, upper case, percent-encoded — which decodes to `new` AFTER the
  check), trailing / doubled slashes, a `#hash` either side of the query,
  `%2F` / `%20` / multi-byte / malformed ids, empty / bare / repeated / `+`
  `msg`s, non-salon, relative and absolute hrefs, the empty string.
- **Regenerate** (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`):

  ```bash
  PIN='/tmp/qt-v4-pin-p4d384-624d0d25c'
  ls -ld "$PIN" && git -C "$PIN" rev-parse HEAD   # 624d0d25c…
  F=<V5>/apps/web/src/testing/fixtures/message-navigation.oracle.ndjson
  rm -f "$F"
  cp <V5>/apps/web/oracle/message-navigation.recorder.ts "$PIN/"
  cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
    npx tsx message-navigation.recorder.ts > "$F"
  wc -l < "$F"                 # 42
  grep -c '"threw"' "$F"       # 1
  ```
- **The storage / event pair is not in the corpus** (it needs `sessionStorage`
  and `window`): `app/chat/message-navigation.spec.ts` replays v4's own
  `message-navigation.test.ts:45-77` cases plus the storage-failure arms.

## P4.D384 — `conversation-search.oracle.ndjson` ("find in this chat")

Recorded 2026-11-07 at the pin `624d0d25c` (`/tmp/qt-v4-pin-p4d384-624d0d25c`,
its own `npm ci`) through v4's REAL `lib/chat/conversation-search.ts`
(`4f5047241`), a pure module with no imports.

| Corpus | Rows | sha256 | v4 code | Consumer |
|---|---|---|---|---|
| `conversation-search.oracle.ndjson` | 94 — 1 `constants`, 24 `text`, 6 `escape`, 19 `pattern`, 44 `find` | `8fde42c0a2861b9d60dab239a75694476c65fac436e3f384b12523c500a28494` | `lib/chat/conversation-search.ts` (`4f5047241`) | `app/chat/conversation-search.oracle.spec.ts` |

- **Row shape:** `{id, kind, …inputs, out}`; the spec compares
  `JSON.stringify` bytes. `find` rows carry the messages, query, limit and a
  `speaker` MODE (`absent` / `null` / `byId` / `announcer`) the spec rebuilds
  as the same `speakerOf` callback; `pattern` rows record `{source, flags,
  probeHit}` or `null`.
- **Corpus:** v4's own `conversation-search.test.ts` cases plus every
  `toSearchableText` pass, TOOL / empty / `systemSender` rows, `limit` under
  the total and `0`, metacharacters, 1 vs 2 trimmed characters, `u`-flag case
  folding (`İ` folds to nothing but itself; `ß` ↔ `ẞ` but NOT `SS`; the Kelvin
  sign ↔ `k`; `ſ` ↔ `S`), positions 0 / end / 40 / 41 / past 140, overlapping
  `aa` in `aaaa` (counted non-overlapping: 2), and astral edges.
- **Measured (not fixed):** the snippet slices UTF-16 code units, so an astral
  character straddling the window splits — v4 emits a lone LOW surrogate after
  the leading `…` (`find: astral characters straddling the snippet start`,
  message 2) and a lone HIGH surrogate before the trailing `…` (`… snippet
  end`, message 2). v5 reproduces the split byte for byte.
- **Regenerate** (Node 24 at `~/.nvm/versions/node/v24.13.1/bin`; the
  recorder's header has the same recipe):

  ```bash
  PIN='/tmp/qt-v4-pin-p4d384-624d0d25c'
  ls -ld "$PIN" && git -C "$PIN" rev-parse HEAD     # 624d0d25c…
  F=<V5>/apps/web/src/testing/fixtures/conversation-search.oracle.ndjson
  rm -f "$F"
  cp <V5>/apps/web/oracle/conversation-search.recorder.ts "$PIN/"
  cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH \
    npx tsx conversation-search.recorder.ts > "$F"
  rm -f "$PIN/conversation-search.recorder.ts"
  wc -l < "$F"                                   # 94
  ```
- **Not in the corpus:** the DOM paint (`chat/search-highlight.ts`, v4
  `useSearchHighlight`) and the search surfaces (`search/conversation-matches.ts`,
  the bar, the dialog) — component specs over v4's markup.

## P4.D384 — `stream-frames.oracle.ndjson` (the Salon's kind-tagged frames)

Recorded by `apps/web/oracle/stream-frames.recorder.ts` against v4's REAL
`parseFrame` / `doneLeavesNoReply` (`lib/chat/stream-frames.ts`, `604a082c4`)
at the pin **`624d0d25c`** (`4.13.0-dev.8`, `zod` 4.6.7), from the lane-unique
pin `/tmp/qt-v4-pin-p4d384-624d0d25c` with its OWN `npm ci --offline`. Replayed
IN FILE ORDER through one fresh module by `core/stream-frames.oracle.spec.ts`
(the once-per-kind WARN Set is module-global, so order is part of the corpus).

**179 rows:** `null` 11, `valid` 66, `drop` 46 (each refused row also carries
`issues`, v4's first three Zod issue texts), `golden` 51 (every frame of v4's
six golden streams through v4's own `loadFrameStream`; `core/__fixtures__/
frame-trace.ts`'s `goldenStream()` reads them for the reducer specs),
`doneLeavesNoReply` 5. The spec asserts every count.

```bash
PIN='/tmp/qt-v4-pin-p4d384-624d0d25c'
F=<V5>/apps/web/src/testing/fixtures/stream-frames.oracle.ndjson
rm -f "$F"; cp <V5>/apps/web/oracle/stream-frames.recorder.ts "$PIN/"
cd "$PIN" && PATH=$HOME/.nvm/versions/node/v24.13.1/bin:$PATH npx tsx stream-frames.recorder.ts > "$F"
wc -l < "$F"   # 179
```


## `established-fact.oracle.ndjson`

**207 rows** — 72 `sender … × kind …` (every `systemSender` × `systemKind`
cell with `isEstablishedFact` absent) + 135 `flag … × sender … × kind …` (the
five other fact flags — true / false / null / 1 / `'true'` — over every sender
with the kind absent / null / `established-fact`). Drives
`app/chat/established-fact.oracle.spec.ts`: v4's four fact predicates
(`lib/chat/established-fact.ts`, `db6190b44`), `getSystemKindDisplayLabel` /
`getAnnouncementAccentClasses` / `getAnnouncementImportance`
(`system-message-labels.ts`) and `isAlwaysExpandedAnnouncement` /
`isCollapsedAnnouncement` (`announcement-render-items.ts`, `671c59a53`), each
row one message shape with every function's answer.

- **Provenance:** recorded 2026-11-07 by P4.D385 at the round pin `624d0d25c`
  (`4.13.0-dev.8`) from a lane-unique detached worktree with its own `npm ci
  --offline`, Node 24.13.1, through v4's REAL modules (the import idiom).
- **Recorder:** `apps/web/oracle/established-fact.recorder.ts` (its header has
  the invocation). Expect 207 lines; a second run reproduces the file byte for
  byte (`cmp` — measured).

## `markdown-button-name.oracle.ndjson`

**31 rows** — 1 `formats` (v4's `MARKDOWN_FORMATS` label / tooltip / type in
order, plus the sliced declaration's bytes) + 30 `name` (v4's eleven buttons,
then the rule's edges: containment either case, digit-only, non-alphanumeric /
empty / astral / accented labels, an empty tooltip, a dotted capital İ).
Drives `app/editor/formatting-toolbar.oracle.spec.ts`: v4's
`markdownButtonName` (`components/chat/FormattingToolbar.tsx:128-139`,
`df8d4d70a`), the Markdown buttons' accessible names.

- **Provenance:** recorded 2026-11-08 by P4.D385 at the round pin `624d0d25c`
  (`4.13.0-dev.8`), Node 24.13.1. v4 does NOT export the function, so the
  recorder reads the pin's own `FormattingToolbar.tsx`, slices exactly the one
  declaration, transpiles it with the pin's TypeScript 6.0.3 and evaluates it
  (the SLICE idiom, ruling PL-11).
- **Recorder:** `apps/web/oracle/markdown-button-name.recorder.ts`. Expect 31
  lines; a second run reproduces the file byte for byte (`cmp` — measured).
