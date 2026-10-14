# Survey — the memory SPA (Phase G's ONE Maintenance card + dialog), the chat-stream fixes (bugs 241 / 242), the `help/` + `docs/v4/` re-vendor, and the NO-PORT evidence

Read-only planning survey, **2026-10-13**. v4 = `~/source/quilltap-server`
read ONLY through `git show 64743ee13:<path>` (**the PIN**, `4.10.0-dev.175`)
and `git diff 4a38222e6 64743ee13 -- <paths>` / `git show <sha> -- <paths>`
(the span). **The oracle baseline v5 has ported is `4a38222e6`**; v5 = `main`
at **`a7c6f0d03`**. **Every v4 line number below is AT THE PIN** unless marked
`(pre)` (= `4a38222e6`). Measured: no file in this slice moved between
`01a83539d` (the pin P4.D276 ported the memory SPA at) and `4a38222e6`
(`git log 01a83539d..4a38222e6 -- components/memory components/tools
components/settings/tabs/MemorySearchTabContent.tsx app/salon/[id]/hooks/
useSSEStreaming.ts` is empty), so `(pre)` IS the shape v5's SPA carries.
Nothing here is taken from commit prose (ledger §5.3): every claim is from a
shipped hunk. The ledger's §1 (lines 17–553) and §3 rows `93c5af6ad` (`:605`),
`b045dd986` (`:609`), `48790fb73` (`:610`), `9ec4fa2e9` (`:611`), `64743ee13`
(`:612`), `4ce64fa1e` (`:599`) were read first and are built on, not restated;
where a hunk disagrees with a row this survey says so (§9, points C1–C4).

**Scope.** (A) the SPA side of the memory programme: Settings → Memory
(`MemorySearchTabContent.tsx`), the Maintenance card + its ledger + the
consolidation particulars, the Maintenance dialog + report, the extraction
limits on the Extraction Grain card, `useMemorySettingsDocument`, the memory
list / card / editor, the wipe wording, the realtime refresh of the ledger,
and the SPA-side wire contract. (B) the chat stream: bug 242 (`inChain`) and
bug 241 (`SubmissionLedger` → the error frame → composer restore; the
buffered reader; early close) — **both halves**, server frame and SPA.
(C) `help/` (131 → 131, different files) and the `docs/v4/` mirror. (D) the
NO-PORT evidence for `4ce64fa1e`, `9ec4fa2e9`, bug 235, and the transport
halves of bug 241. **Out of scope** (other surveys): every server-side
maintenance engine (`lib/memory/maintenance/*`, the job, the scheduler, the
ledger's seeding), the route handlers' bodies beyond the wire shapes the SPA
reads, the schema keystone / D23, admission, extraction, recall, the
carriers, the scenarios SPA (`545e299bb`'s `ScenarioBrowserDialog` etc.), the
CLI `memories` family.

---

## 0. The span for this slice, in one paragraph

Fifteen of the nineteen commits touch this slice (most only through `help/`). **SPA:** `c2795733f` (bug
210: the housekeeping card's `N counted · M protected`, the editor drops
`source`), `e0ac7493a` (the housekeeping card + dialog move onto
`?action=maintenance-config`, the cap bounds 100–100,000, the importance band
`.4 → .3` via `importanceBand`), `8af764514` (the editor's 200-vs-201
"Already on file" toast), `124c8aca3` (bug 201: the wipe / re-extract
wording + `sparedCount`), `4ae0c4ca6` (`tfidfClusterThreshold` on the
consolidation card), `104e986c7` (Phase F: one `MEMORY_MAINTENANCE` job;
the dedup card's system-tools actions DELETED, the job topics), then
**`b045dd986` (Phase G) deletes all five P4.D276-era cards / dialogs**
(`memory-housekeeping-card.tsx`, `memory-consolidation-card.tsx`,
`memory-consolidation-report-dialog.tsx`, `memory-dedup-card.tsx`,
`components/memory/housekeeping-dialog.tsx`) for ONE Maintenance card
(`memory-maintenance-card.tsx` 383 + `memory-maintenance-ledger.tsx` 102 +
`memory-consolidation-particulars.tsx` 124 + `hooks/useMemorySettingsDocument.ts`
84) and ONE dialog (`components/memory/maintenance-dialog.tsx` 259 +
`maintenance-report.tsx` 271), moves the extraction rate limits onto the
Extraction Grain card (`memory-extraction-limits-section.tsx` 97), maps the
three retired `section=` anchors onto `memory-maintenance`, and fixes **bug
242** in the stream reader; `48790fb73` relabels three report / ledger lines
("Clusters worth folding" over `clustersQualified`, "Exempt" / "exempt",
"Within the cap" muted). **Stream:** `93c5af6ad` (bug 241: the server's
`SubmissionLedger` + `ErrorEventExtras`; the Salon's `SalonSendError`,
`restoreUnpersistedSubmission`, `splitSseBuffer` reader,
`StreamClosedEarlyError`, the 8 s toast; `persistDraft` threaded through
`SalonView` / `useImpersonationVoice`), `b045dd986` (bug 242).
**help/docs:** 25 `help/` paths across fifteen commits (§8.3), 73 `docs/` paths
(§8.4). **NO-PORT:** `4ce64fa1e` (a merge whose own resolution is the
picker fix renumbered 199 → 229), `9ec4fa2e9` (docs + one CLI status line),
bug 235 (`processSingleton`, a Next.js per-bundle defect).

---

## 1. v4 AT THE PIN — every module in the slice

### 1.1 `components/settings/tabs/MemorySearchTabContent.tsx` (64 lines; touched `104e986c7`, `b045dd986`)

- `:18-22` `RETIRED_SECTIONS: Record<string,string>` =
  `{'memory-housekeeping': 'memory-maintenance', 'memory-consolidation':
  'memory-maintenance', 'memory-deduplication': 'memory-maintenance'}`.
- `:26-27` `activeSection = requestedSection ? (RETIRED_SECTIONS[requestedSection] ?? requestedSection) : null`.
- **Seven** `CollapsibleCard`s, in order (`:34-60`), title / description /
  `sectionId` verbatim:
  1. `Embedding Profiles` / `Configure embedding models for semantic memory` / `embedding-profiles`
  2. `Repair Missing Embeddings` / `Generate embeddings for legacy memories that predate the embedding-aware gate` / `memory-backfill`
  3. **`Maintenance`** / **`Fold repetitions, consolidate into digests, keep each character under the cap, and let old archive notes go`** / **`memory-maintenance`** (NEW, `:42-44`)
  4. `Extraction Grain` / `Choose how often a character's observations of other characters are noted` / `memory-extraction-grain`
  5. `Recall Relevance` / `Keep project-specific memories from wandering into unrelated chats` / `memory-recall`
  6. `Regenerate Memories` / `Wipe and rebuild every chat-linked memory using the current extraction pipeline` / `memory-regenerate`
  7. `Regenerate Conversation Summaries` / `Re-mirror every summarised chat into its characters' vaults for relevant-conversation recall` / `conversation-summaries-regenerate`
- GONE (pre `:33-38`, `:49-51`): `Memory Housekeeping` (`memory-housekeeping`),
  `Consolidation` (`memory-consolidation`), `Memory Deduplication`
  (`memory-deduplication`).

### 1.2 `components/tools/memory-maintenance-card.tsx` (383 lines, NEW `b045dd986`, relabelled `48790fb73`)

URLs `:22-24`: `MAINTENANCE_URL = '/api/v1/memories?action=maintenance-config'`,
`CONSOLIDATION_URL = …?action=consolidation-config`,
`CHARACTER_MEMORY_COUNTS_URL = …?action=character-memory-counts`.
`CAP_MIN = MEMORY_CAP_BOUNDS.min` (100), `CAP_MAX = .max` (100,000) `:26-27`.

- **`useCharacterMemoryCounts()`** `:46-61` (exported; reused by the
  ledger): query key `queryKeys.memories.characterMemoryCounts`; a failed
  read leaves the list EMPTY (no error); each row
  `{id, name, demotable: c.demotable ?? c.memoryCount ?? 0, protected: c.protected ?? 0}`.
- **`PhaseSwitches`** `:87-181` — `busy = maintenance.saving || consolidation.saving`.
  - `Enable automatic maintenance` (`m.enabled`, disabled `busy`), hint
    `A daily round, and a visit whenever a character nears the cap. Off by default — read the limits below first.`
    → `maintenance.save({ enabled: !m.enabled })`.
  - indented `pl-7`:
    - `Fold near-duplicates` (`m.phases.foldNearDuplicates`, disabled `busy || !m.enabled`), hint `Notes that say the same thing about the same occasion are folded into the sturdiest of them, stray details kept as footnotes. No model is consulted.`
    - **`Consolidate`** (`c.enabled` — the CONSOLIDATION document; disabled
      ONLY by `busy`, never by `!m.enabled`) → `consolidation.save({ enabled: !c.enabled })`,
      hint `Once a day, and whenever a character’s unconsidered notes pass the watermark, a model folds clusters of related notes into digests and the originals go to the archive. Has its own switch, as it costs model calls.` (curly `’`).
    - `Demote over the cap` (`m.phases.demote`), hint `Past the cap, the least protected notes — old, unreinforced, unlinked, unconsulted — go to the archive first. Nothing is destroyed.`
    - `Retire old archive notes` (`m.phases.retire`), hint `Superseded archive notes older than the retention window below are let go for good.`
    - toggles post `{ phases: { ...m.phases, [key]: !m.phases[key] } }` — the
      WHOLE three-key object (`:99-100`).
    - **Archive retention** input `#maintenance-retention` (`:156-173`):
      `type=number min=1 step=1 placeholder="never"`, draft until blur,
      label `Archive retention`, suffix `days (blank keeps them forever)`.
      `commitRetention` `:102-116`: blank → save `{coldRetentionDays: null}`
      only when currently non-null; non-integer or `< 1` →
      `consolidation.setError('Archive retention must be a whole number of days, or blank for never')`;
      unchanged → no save. Saved through the CONSOLIDATION document.
    - footnote `:174-177`: `The Commonplace files in each vault are refreshed after every consolidation. Hand-written notes and digests are never folded, demoted or let go by any of this.`
- **`CapSettings`** `:183-313`: `#perCharacterCap` (`min=CAP_MIN max=CAP_MAX
  step=100`, draft until blur — the PR-98 review fix, comment `:192-194`),
  suffix `extracted notes per character`. `handleCapBlur` `:198-208`:
  `Math.floor(Number(trim))`; blank / non-finite / out of range →
  ``setError(`The per-character cap must be between ${CAP_MIN.toLocaleString()} and ${CAP_MAX.toLocaleString()}`)``;
  equal → no save; else `save({ perCharacterCap: value })`.
  Overrides (only when `characters.length > 0`): toggle button
  `{▾|▸} Per-character overrides ({n} active)`; help
  `Set a different cap for individual characters. Leave blank to fall back to the global cap (` + `{cap.toLocaleString()}` + `).`;
  each row: name + `({demotable.toLocaleString()} counted` + (protected > 0 ?
  ` · {protected.toLocaleString()} exempt` : ``) + `)`, class
  `qt-text-destructive` when `demotable > (override ?? cap)` else
  `qt-text-muted`; input `aria-label="Cap override for {name}"`,
  placeholder = the global cap. `handleOverrideBlur` `:210-234`: blank removes
  (no save when absent); bad →
  ``setError(`Override for character ${characterId.slice(0, 8)} must be between …`)``;
  equal → no save; else `save({ perCharacterCapOverrides: nextOverrides })`
  (the WHOLE map).
- **`MemoryMaintenanceCard`** `:315-383`: two `useMemorySettingsDocument`s
  (`maintenance` — key `queryKeys.memories.maintenanceConfig`, defaults
  `MEMORY_SETTING_DEFAULTS.maintenance`, noun `'maintenance settings'`;
  `consolidation` — key `…consolidationConfig`, noun `'consolidation settings'`).
  Loading (any of the two docs or the counts) →
  `Loading maintenance settings…` (`&hellip;`). `error = maintenance.error ?? consolidation.error`
  rendered once at the bottom (`qt-text-destructive`). Sections in order:
  intro paragraph `:339-344` (verbatim:
  `One round of the household staff tends each character’s memories, in a fixed order: repetitions are folded together, related notes are consolidated into digests, anything over the cap goes to the archive, and superseded archive notes past their retention are let go. Archived notes are not lost: they stay on the shelf, still guard against repeats, and return to active duty if the world asks for them again.`);
  `h3 Automatic rounds` + PhaseSwitches; `h3 The cap` +
  `Only extracted notes in active service count. Maintenance calls when a character reaches nine-tenths of it.` + CapSettings;
  `MemoryConsolidationParticulars`; `h3 Run now` + button
  `Run maintenance…` (`qt-button qt-button-secondary`) + `One character or every one, any phases, with a dry run first.`;
  `h3 The ledger` + `MemoryMaintenanceLedger`; the dialog when open
  (no `characterId` — the picker variant).

### 1.3 `components/tools/memory-maintenance-ledger.tsx` (102 lines, NEW `b045dd986`, `48790fb73`)

- `STATUS_URL = '/api/v1/memories?action=maintenance-status'` `:17`; query key
  `queryKeys.memories.maintenanceStatus`; header comment: "the `memories`
  realtime topic refreshes this read after a run commits" `:13-14`.
- `LedgerStatus` `:19-24` = `{characterId; lastRun: Partial<Record<MaintenancePhase,{at: string; reason: MaintenanceReason}>>; lastSummary: MaintenanceSummary|null; lastDemotion: {at; demoted; excess; effective: boolean}|null}`.
- `REASON_TEXT` `:26-30`: `scheduled → 'daily round'`, `watermark → 'nearing the cap'`, `manual → 'by hand'`.
- `when(iso)` `:32-35`: invalid date → the raw string; else `toLocaleString()`.
- **`summaryLine(summary)`** `:38-50` (exported): `skippedReason` →
  `'skipped: the character is archived'` (archived) else
  `'skipped: character not found'`; else parts joined `' · '`:
  `{fold.folded} folded`; consolidation → `consolidation skipped ({skippedReason})`
  or `{digestsCreated + digestsUpdated} digests, {membersSuperseded} superseded`;
  `{demote.demoted} archived ({demote.demotable}/{demote.cap} counted)`
  (NO `toLocaleString` here); `{retire.retired} let go`; none →
  `nothing to do`.
- Render `:58-101`: loading `Opening the ledger…`; error
  `readErrorText(error, 'Failed to read the maintenance ledger')` destructive;
  entries filtered `e.lastSummary || Object.keys(e.lastRun).length > 0`;
  empty → `The ledger is blank: no round has been recorded since the house last opened its doors.`;
  per entry a `qt-card`: name (from the counts list) else `characterId.slice(0, 8)`;
  `last {rehearsal|run} {when(finishedAt)} ({REASON_TEXT[reason]})`;
  `summaryLine`; a 2-col list over `MAINTENANCE_PHASES` (all FIVE incl.
  mirror) `PHASE_LABELS[phase]` → `{when(at)} · {REASON_TEXT}` or `never`;
  **`48790fb73`**: when `lastDemotion && !lastDemotion.effective`:
  `excess === 0` → class `qt-text-muted`,
  `Within the cap at the last demotion; automatic demotion looks again in an hour.`;
  else `qt-text-warning`,
  `The last demotion moved too little of the excess to be worth repeating at once; automatic demotion rests for an hour after such a round.`

### 1.4 `components/tools/memory-consolidation-particulars.tsx` (124 lines, NEW `b045dd986`)

A collapsed disclosure `{▾|▸} Consolidation particulars` (`aria-expanded`,
`:64-71`). `NUMBER_FIELDS` `:26-34` (label / help / min / max / step / integer):

| key | label | help | min | max | step | int |
|---|---|---|---|---|---|---|
| `clusterThreshold` | `Cluster threshold` | `How alike two notes must be (0 to 1, by cosine) to be folded together.` | 0 | 1 | 0.01 | no |
| `tfidfClusterThreshold` (NEW `4ae0c4ca6`) | `Cluster threshold (Built-in)` | `The same measure when the default embedding profile is the Built-in TF-IDF one, which reckons likeness far more sparingly.` | 0 | 1 | 0.01 | no |
| `minClusterSize` | `Smallest cluster` | `Fewest notes worth a digest. Falls to two when one of them is quite old.` | 2 | — | 1 | yes |
| `maxClusterSize` | `Largest cluster` | `Most notes put before the model in one sitting.` | 2 | — | 1 | yes |
| `matureAfterDays` | `Mature after (days)` | `Notes younger than this are left to settle.` | 0 | — | 1 | no |
| `maxClustersPerRun` | `Clusters per run` | `Bounds the cost of a run; a backlog drains over several.` | 1 | — | 1 | yes |
| `watermark` | `Watermark` | `Unconsidered active notes at which a character is queued automatically.` | 1 | — | 1 | yes |

`coldRetentionDays` is NOT here any more (it moved to the phase switches,
§1.2). `commitNumber` `:42-57`: bad →
``setError(`${label} must be ${integer ? 'a whole number' : 'a number'} ${max !== undefined ? `between ${min} and ${max}` : `of at least ${min}`}`)``;
unchanged → no save. Profile select `#consolidation-profile` `:75-96`:
`The cheap LLM (default)` (value `''` → `null`), a missing selection
`(a profile that has since left the building)`, options
`{name} ({provider}{modelName ? ' • '+modelName : ''})`; help
`A capable model does this better than the cheap one, and is recommended for the first, large backfill.`
Input ids `consolidation-${key}`.

### 1.5 `components/tools/hooks/useMemorySettingsDocument.ts` (84 lines, NEW `b045dd986`)

`MemorySettingsDocument<T> = {config, setConfig, loading, saving, error, setError, save(patch): Promise<boolean>}`.
`useMemorySettingsDocument({queryKey, url, defaults, noun})`: GET via
`apiFetch<{settings?: Partial<T>}>`; seeds `{...defaults, ...loaded.settings}`;
a load error → `readErrorText(loadError, `Failed to load ${noun}`)`; `save`
POSTs the patch, sets `{...defaults, ...data.settings}`, invalidates the key,
toasts `` `${Noun} saved` `` (first letter upper-cased: `Maintenance settings saved`,
`Consolidation settings saved`, `Extraction limits saved`); failure →
`writeErrorText(err, `Failed to save ${noun}`)` → error line + error toast,
returns `false`. Header: "The server merges each patch over the stored
value (bug 244), so a section sends only what it changed."

### 1.6 `components/memory/maintenance-dialog.tsx` (259 lines, NEW `b045dd986`)

`MAINTAIN_URL = '/api/v1/memories?action=maintain'`; `ALL = '__all__'`;
`PICKABLE_PHASES = MAINTENANCE_PHASES` minus `'mirror'` (`:27`).
`PHASE_HINTS` `:29-34` verbatim:
- `fold-near-duplicates`: `Fold notes that say the same thing about the same occasion into the sturdiest of them. No model is consulted.`
- `consolidate`: `Ask a model to fold clusters of related notes into digests; the originals go to the archive. Costs model calls.`
- `demote`: `Send the least protected notes to the archive while a character holds more than the cap.`
- `retire`: `Let go of superseded archive notes older than the retention window (when one is set).`

Props `:43-50`: `characterId?` (fixed; hides the picker), `characterName?`,
`onClose`, `onQueued?`. State: `target` (default `characterId ?? ''`),
`phases` (all four ticked), `capDraft ''`, `running: 'dry'|'real'|null`,
`result: {kind:'one',report}|{kind:'many',reports}`, `error`. The counts query
runs only when `!characterId` (`enabled`). Derived `:70-77`: `everyone =
target === ALL`; `chosen = PICKABLE_PHASES ∩ phases` (in phase order);
`dryRunBlocked = everyone && phases.has('consolidate')`; `cap` = blank →
undefined else `Math.floor(Number(capDraft))`; `capInvalid` (non-finite or
outside 100..100,000); `ready = target !== '' && chosen.length > 0 &&
!capInvalid && running === null`. Toggling a phase / changing the target /
the cap clears `result`.

**Request body** `:89-94` (exact, key order):
`{ ...(everyone ? {} : { characterId: target }), phases: chosen, dryRun, ...(cap !== undefined && phases.has('demote') ? { settings: { cap } } : {}) }`.

`run(dryRun)` `:96-126`: dry → `data.report` → `{kind:'one'}` else
`{kind:'many', reports: data.reports ?? []}`; real →
`queued = data.jobIds?.length ?? 0`; success toast
everyone ? `` `The staff have been sent round to ${queued} character${queued === 1 ? '' : 's'}; the work proceeds below stairs.` ``
: `The staff have been sent round; the work proceeds below stairs.`;
invalidate `maintenanceStatus`; `onQueued?.()` THEN `onClose()`. Failure:
`writeErrorText(err, dryRun ? 'The rehearsal failed' : 'Failed to queue maintenance')`
→ error line + error toast.

Render (portal to `document.body`, overlay `qt-dialog-overlay p-4 z-[100]`,
`qt-dialog max-w-3xl max-h-[90vh]`): title
`Memory Maintenance` + (`characterName` ? ` — {name}` : ``); close button
`aria-label="Close"`; description
`Rehearse first, if you please: a dry run shows what each phase would do and writes not a word. The real run is queued in the background and goes ahead whatever the switches on the settings card say.`
Picker `#maintenance-target` labelled `Character`: `Choose a character…`
(`''`), `Every character` (`__all__`), then each
`{name} ({(demotable ?? memoryCount ?? 0).toLocaleString()} counted)`.
Fieldset `Phases` (label `PHASE_LABELS[p]` + hint). When demote ticked:
`#maintenance-cap` `Cap for this run`, `placeholder="the configured cap"`,
`step=100`; hint capInvalid ?
`` `Between ${min.toLocaleString()} and ${max.toLocaleString()}, if you please.` ``
: `Leave blank to use each character’s own cap (its override, else the global one).`
When `dryRunBlocked`: warning
`Consolidation calls a model, so it can be rehearsed for one character at a time.`
Body: error; `Rehearsing…`; the report (one → `MaintenanceReportView`, many
→ `MaintenanceReportsTable`); idle → `Choose the phases, then rehearse or send for the staff.`
Footer: `Close`; `Dry run` / `Rehearsing…` (disabled `!ready || dryRunBlocked`);
`Run for real` / `Queuing…` (`qt-button-primary`, disabled `!ready`).

### 1.7 `components/memory/maintenance-report.tsx` (271 lines, NEW `b045dd986`, `48790fb73`)

- `PHASE_LABELS` `:12-18` (exported): `fold-near-duplicates → 'Fold near-duplicates'`,
  `consolidate → 'Consolidate'`, `demote → 'Demote over the cap'`,
  `retire → 'Retire old archive notes'`, `mirror → 'Refresh the Commonplace files'`.
- `SKIP_TEXT` `:20-27`: `not-requested → 'not asked for'`,
  `maintenance-disabled → 'automatic maintenance is off'`,
  `consolidation-disabled → 'consolidation is off'`,
  `phase-disabled → 'switched off above'`,
  `no-retention-window → 'no archive retention window is set'`,
  `nothing-touched → 'nothing changed that it mirrors'`.
- `REPORT_SKIP_TEXT` `:29-32`: `character-not-found → 'The character could not be found.'`,
  `archived → 'The character rests in the archive; nothing was done.'`.
- `CONSOLIDATION_SKIP_TEXT` `:34-36`: `no-llm → 'No model is available to do the consolidating. Name a connection profile, or configure the cheap LLM.'`
- `Stat` card `:38-45`; `bucketLabel` `:47-56` (`About themselves` /
  `About {subjectName}` / `About no one in particular`).
- `ClusterBlock` `:58-118` — the old consolidation report's cluster card
  (`Cluster {i+1} · {bucket} · {clusterKind}`, status, error, `Notes to be folded ({n})`,
  standalone `(kept on the shelf as it is)`, `Proposed digest{s}`,
  `{revises an existing digest|new digest} · {kind} · importance {pct}% · from {n} note{s}`,
  `Contradictions noticed ({n})`, `A newer note displaces an older one.`,
  `({older8} superseded by {newer8})`).
- `ConsolidationSection` `:120-147`: `proposed = digestsCreated + digestsUpdated || Σ cluster.digests.length`;
  stats grid **`Notes considered` (`candidates`) · `Clusters worth folding`
  (`clustersQualified` — `48790fb73`; was `Clusters found` / `clustersFound`) ·
  `Clusters attempted` · `Digests proposed`**; the tail line pieces
  `{clustersDeferred} further clusters wait for another run. `,
  `{immatureSkipped} notes are too young to fold. `,
  `{noEmbeddingSkipped} notes lack embeddings. `,
  `The time budget ran out; the rest will keep. `, then `{(durationMs/1000).toFixed(1)}s.`;
  no clusters and not skipped → `Not a single cluster worth folding. The shelves are in good order.`
- **`MaintenanceReportView({report})`** `:150-235` (exported):
  `skippedReason` → the warning sentence only. Else the phase ledger list:
  each `PHASE_LABELS[phase]` (muted unless `ran`) and
  `ran` ? `` `${dryRun ? 'rehearsed' : 'ran'} · ${(durationMs/1000).toFixed(1)}s` ``
  : `` `skipped — ${SKIP_TEXT[skippedBecause ?? 'not-requested']}` ``.
  Then per phase that has a block: **fold** (`Notes compared` = eligible,
  `Sets found` = `groups.length`, `Would fold|Folded`; tail
  `{deferred} notes wait for a later comparison. `, `{groupsDeferred} sets wait for the next run. `,
  `{groupsFailed} sets could not be folded and were left as they were.`);
  **consolidation** (the section above); **demote** (`Cap`, `Counted`,
  **`Exempt`** (`48790fb73`; was `Protected`) over `demote.protected`,
  `Would archive|Archived` — all `toLocaleString()`; tail
  `{excess} counted notes over the cap; the least protected go to the archive first.`
  or `Within the cap. Nothing need go to the archive.` + ` Hand-written notes and digests are never counted and never moved.`);
  **retire** (`No superseded archive note is older than {N} days.` /
  `{n} superseded archive note{s} older than {N} days {would be|were} let go.`);
  **mirror** (only when `subjects.length > 0`):
  `{n} subject file{s} {would be refreshed | refreshed once the run is committed | refreshed}.`
- **`MaintenanceReportsTable({reports})`** `:238-271` (exported):
  rows = reports without `skippedReason`; none →
  `No character has any memories to tend.`; columns `Character`, `Would fold`,
  `Counted / cap` (title `Counted notes against the cap`; red when excess > 0),
  `Would archive`, `Would let go`; missing block → `—`; name `?? 'Unnamed'`.

### 1.8 `components/tools/memory-extraction-limits-section.tsx` (97 lines, NEW `b045dd986`) + the grain card

`LIMITS_URL = '/api/v1/memories?action=extraction-limits-config'`; key
`queryKeys.memories.extractionLimitsConfig`; defaults
`MEMORY_SETTING_DEFAULTS.extractionLimits` (`{enabled:false, maxPerHour:20,
softStartFraction:0.7, softFloor:0.7}`), noun `'extraction limits'`.
`h3 Rate limits`; checkbox `Limit how quickly a character takes notes` +
`A guard against a long, busy evening filling the shelves. Off by default.`;
three fields (disabled `saving || !config.enabled`), ids `extraction-limits-${key}`:
`maxPerHour` `Notes per hour` / `The most new notes one character may take in an hour.` 1..10,000 step 1 int;
`softStartFraction` `Begin choosing at` / `The share of the hourly limit (0 to 1) past which only weightier notes are kept.` 0..1 step .05;
`softFloor` `Importance floor` / `Past that point, the least importance a new note must carry (0 to 1).` 0..1 step .05.
Bad value → `` `${label} must be ${int ? 'a whole number' : 'a number'} between ${min} and ${max}` ``.
Loading → `Loading extraction limits…`. Its OWN error line.
`memory-extraction-grain-card.tsx:189` mounts it inside
`<div className="pt-4 border-t qt-border-default">` after the grain card's
own error line (`+5` lines, the card otherwise unchanged).

### 1.9 The memory list / card / editor

- `components/memory/memory-list.tsx`: the `Cleanup` button becomes
  `Maintenance`, title `Fold repetitions, consolidate, and keep this character under the cap`
  (`:256-261`); `<MaintenanceDialog characterId={characterId} onClose=… onQueued={handleMaintenanceQueued}/>`
  (`:392-399`) — **no `characterName`**, so the title is bare
  `Memory Maintenance`; `handleMaintenanceQueued` `:232-235` re-reads page 0
  (does not close — the dialog closes itself). Tier filter / digest viewer
  unchanged in the span.
- `components/memory/memory-card.tsx:52-59` and
  `memory-editor.tsx:100-101`: `importanceBand(importance)` from
  `lib/memory/scoring/importance.ts` — `IMPORTANCE_HIGH = 0.7`,
  `IMPORTANCE_LOW = 0.3`; `>= 0.7 → high`, `< 0.3 → low`, else medium
  (`e0ac7493a`; was `>= 0.4 → Medium`). Card reads `memory.importance`
  (not reinforced).
- `memory-editor.tsx` (`c2795733f`, `8af764514`): the payload no longer
  carries `source` (create AND edit); `fetchJson<{memory?, action?}>`;
  `merged = !isEditing && result.status === 200` (`:88`) → toast
  `Already on file — the existing memory has been reinforced instead` (`:90`)
  else `Memory created` / `Memory updated`. The server answers 201 only when
  `admissionInserted(outcome)` (`action.kind ∈ {insert, insert-linked}`,
  `lib/memory/admission/outcome.ts:11-13`), 200 with `{memory, action}`
  otherwise (`app/api/v1/memories/route.ts:636-637`).

### 1.10 The wipe / re-extract wording (`124c8aca3`, bug 201)

- `app/salon/[id]/hooks/useMemoryActions.ts:42` confirm:
  `` `Delete the memories extracted from this chat (${count} on record)? Hand-written entries, archived entries and consolidated digests stay on the shelf. This action cannot be undone.` ``;
  `:55-58`: `spared = typeof data.sparedCount === 'number' ? data.sparedCount : 0`;
  count set to `spared`; toast spared > 0 ?
  `` `Deleted ${deletedCount} memories; kept ${spared} hand-written, archived or digest entries` ``
  : `` `Deleted ${deletedCount} memories` ``.
- `useMemoryActions.ts:121`, `SalonListView.tsx`, `character-conversations-tab.tsx:150`
  re-extract confirm: `This will delete the memories extracted from this chat and re-extract them from the conversation. Hand-written entries, archived entries and consolidated digests are kept. Are you sure?`

### 1.11 Client query keys + realtime (`lib/query/keys.ts`, `lib/realtime/*`)

`queryKeys.memories`: `housekeepingConfig` GONE; NEW
`maintenanceConfig: ['memories','maintenance-config']`,
`extractionLimitsConfig: ['memories','extraction-limits-config']`,
`maintenanceStatus: ['memories','maintenance-status']` (doc: "the `memories`
topic refreshes it after a run"). `topic-map.ts:102-106`: `memories` →
scoped `[chatCount(id)]`, **unscoped `[queryKeys.memories.all]`** (covers
the ledger). `job-topics.ts`: `INTER_CHARACTER_MEMORY`, `MEMORY_HOUSEKEEPING`,
`MEMORY_CONSOLIDATION` cases GONE; `MEMORY_MAINTENANCE` →
`[{topic:'memories'}, {topic:'mountPoints'}]` (no dry-run carve: a dry run
never enqueues). `lib/background-jobs/activity-kinds.ts`: `MEMORY_MAINTENANCE: 'memory'`
replaces three rows; the `Mem` chip title UNCHANGED
(`Memory work (extraction, regeneration, consolidation, housekeeping)`).

### 1.12 The wire the SPA reads (route `app/api/v1/memories/route.ts`, 1,388 lines at the pin)

Action table (header `:10-33`). GET: `maintenance-status`, `embeddings`,
`maintenance-config`, `extraction-limits-config`, `recall-config`,
`consolidation-config`, `extraction-mode-config`, `backfill-embeddings`,
`character-memory-counts`, `regenerate-all` (`:199-210`). POST: `search`,
`maintain`, `repair`, … (`:254-270`). **Retired** (pre `:12-14`, `:18`, `:23`,
`:26-27`, `:35`): `housekeep` (GET + POST), `housekeep-sweep`,
`housekeeping-config` (GET + POST), `consolidate`, `extraction-concurrency`
(GET + POST) → v4's `dispatchAction` answers the unknown-action 400
(`lib/api/middleware/actions.ts:182`). System tools: `memory-dedup-preview` /
`memory-dedup` DELETED (`app/api/v1/system/tools/route.ts`, −81).

- `maintainSchema` `:129-141`: `{characterId?: uuid, phases?: enum(MAINTENANCE_PHASES)[] min 1, dryRun?: bool, settings?: {consolidation?: patchSchemaOf(MemoryConsolidationSettingsSchema), maxClustersPerRun?: int 1..1000, cap?: int 100..100000}}`.
  `handleMaintain` `:736-810`: `phases → withImpliedPhases` (consolidate
  brings mirror); unknown character → 404 `Character`; dry run that may call
  the model (`!phases || phases.includes('consolidate')`) without a
  characterId → 400 `A dry run that includes consolidation names one character (characterId)`;
  dry-run clusters `min(settings.maxClustersPerRun ?? 10, 40)`, budget
  120,000 ms; reply `{success, dryRun: true, report}` (one) or
  `{success, dryRun: true, reports}` (all non-archived characters); real →
  `{success, dryRun: false, jobId: jobIds[0] ?? null, jobIds}`; failures
  500 `Maintenance dry run failed` / `Failed to enqueue maintenance`.
- `handleMaintenanceStatus` `:812-825`: optional `characterId` (404 when
  unknown) → `{success, ledger: LedgerStatus[]}` (`lib/memory/maintenance/ledger.ts:250-283`;
  `effective` computed at read; ISO `at`s).
- `handleRepair` `:827-848`: `{characterId?, dryRun?}` → `{success, report: MemoryGraphRepairReport}`
  (`lib/memory/repair.ts:25-45`); 500 `Memory graph repair failed`. **No
  SPA caller at the pin** (`git grep 'action=repair' 64743ee13 -- components app/salon` is empty — CLI only).
- `maintenance-config` GET `:855-861` → `{success, settings: MemoryMaintenanceSettings}`;
  POST `:863-891` validates `{enabled?, phases?: patch, perCharacterCap?, perCharacterCapOverrides?}`,
  MERGES `phases` over the stored toggles, → `{success, settings}`.
- `extraction-limits-config` POST `:909-935` (patch + merge), GET
  `:901-907` → `{success, settings}`.
- `character-memory-counts` `:1022-1052` → `{success, characters: [{id, name, memoryCount (hot+cold), hotTotal, demotable, protected, cold}]}`
  sorted `demotable desc, memoryCount desc` (was `memoryCount desc`).
- Settings schemas (`lib/schemas/settings.types.ts` at the pin):
  `MEMORY_MAINTENANCE_CAP_MIN = 100`, `_MAX = 100_000`;
  `MemoryMaintenanceSettingsSchema = {enabled: false, perCharacterCap: 2000, perCharacterCapOverrides: {}, phases: {foldNearDuplicates: true, demote: true, retire: true}}`;
  `MemoryConsolidationSettingsSchema` gains `tfidfClusterThreshold` (0.30)
  BETWEEN `clusterThreshold` and `minClusterSize`;
  `MemoryExtractionLimitsSchema.maxPerHour` `.max(10_000)`.
- Types (`lib/memory/maintenance/types.ts`, 184 lines, client-safe):
  `MAINTENANCE_PHASES = ['fold-near-duplicates','consolidate','demote','retire','mirror']`,
  `MaintenanceReason`, `PhaseSkipReason`, `PhaseOutcome`, `FoldPhaseReport`,
  `DemotePhaseReport`, `RetirePhaseReport`, `MirrorPhaseReport`,
  `MaintenanceReport`, `MaintenanceSummary` — transcribed in §7.4.
- `ConsolidationStats` (`lib/memory/consolidation.ts:148-177`) GAINS
  `demotedCandidates` (after `candidates`), LOSES `mirrorFilesWritten`;
  `ConsolidationReport.skippedReason` NARROWS to `'no-llm'` (the
  character-level skips moved up to `MaintenanceReport`).

### 1.13 Chat stream — bug 242 (`b045dd986`)

`app/salon/[id]/hooks/useSSEStreaming.ts:756-762` (1,449 lines at the pin):
``if ((data.chainDepth || 0) > 0) inChain = true`` (was `inChain = true` on
any `turnStart`). The depth-0 event still drives `onTurnStart` (the bubble's
avatar / name). Consequence in v4: a single-character turn's `done` goes to
`onDone` (`:1061-1123` — `fetchChat`, `notifyQueueChange`, the empty-response
toast, the indicator resets, an assistant bubble even for empty
`fullContent`) instead of `onIntermediateDone` (`:1124-1146` — returns on
`emptyResponse || !fullContent`). The first turn of a multi-character chain
also goes to `onDone` again; depth ≥ 1 turns still take the intermediate
path; `chainComplete` still ends the chain. The server keeps emitting
`turnStart` at `chainDepth: 0` before every first turn
(`orchestrator.service.ts:410`).

### 1.14 Chat stream — bug 241 (`93c5af6ad`), both halves

**Server.** `lib/services/chat-message/orchestrator.service.ts`:
- `interface SubmissionLedger { userMessagePersisted: boolean; pendingToolResultsPersisted: boolean }` `:195-198`,
  created `{false, false}` per `handleSendMessage` request `:210`, passed to
  `processMessage` AND to every chained `processMessage` (`:239` — chained
  turns are continue-mode and never flip it).
- Flip sites: the fair-rotation pause returns after saving the user message
  → `ledger.userMessagePersisted = true` + DEBUG
  `[Orchestrator] User message persisted (fair-rotation pause)` `{chatId}`
  (`:377-381`); after the pending tool results' TOOL rows →
  `ledger.pendingToolResultsPersisted = true` + DEBUG
  `[Orchestrator] Pending tool results persisted` `{chatId, count}`
  (`:752-756`); after `repos.chats.addMessage(chatId, userMessage)` →
  `ledger.userMessagePersisted = true` + DEBUG
  `[Orchestrator] User message persisted` `{chatId, userMessageId}` (`:827-829`).
- `handleStreamError(error, controller, encoder, {chatId, ledger})`
  `:2180-2222`: ERROR `Streaming error` `{chatId, userMessagePersisted, pendingToolResultsPersisted}` (+ error);
  `errorMessage`/`errorType`: ZodError → `Validation error: {msg} at {path}` /
  `'validation'` (empty issues → `Response validation failed`); Error →
  `error.message` / `error.name`; else `'Unknown error'` / `'unknown'`;
  DEBUG `[Orchestrator] Emitting error frame` `{chatId, errorType, userMessagePersisted, pendingToolResultsPersisted}`;
  frame `encodeErrorEvent(encoder, 'Failed to generate response', errorType, errorMessage, {userMessagePersisted, pendingToolResultsPersisted})`.
- `streaming.service.ts:709-733`: `ErrorEventExtras {userMessagePersisted?, pendingToolResultsPersisted?}`;
  the frame bytes are `` `data: ${JSON.stringify({ error, errorType, details, ...extras })}\n\n` ``
  — key order `error, errorType, details, userMessagePersisted, pendingToolResultsPersisted`;
  without extras still the three-key frame (test
  `__tests__/unit/lib/services/chat-message/streaming.service.test.ts`
  `carries the persistence flags when given (bug 241)` / `stays a three-field frame when no extras are passed`).

**Client.** `useSSEStreaming.ts`:
- `SSEEvent.userMessagePersisted?` / `pendingToolResultsPersisted?` `:59-66`.
- `class SalonSendError extends Error` `:191-205` (`name = 'SalonSendError'`,
  the two `boolean | undefined` fields).
- `STREAM_CLOSED_EARLY_MESSAGE = 'The connection closed before the reply was finished. Please try again.'` `:207`;
  `class StreamClosedEarlyError` `:215-220`.
- `SUBMISSION_RESTORED_NOTE = 'Your message is back in the composer.'` `:223`;
  `sendFailureToastText(message, restored)` `:226-231`: not restored → message;
  else `trimEnd`, append `.` unless it ends in `[.!?…]`, then `' ' + NOTE`.
- `SEND_FAILURE_TOAST_MS = 8000` `:234`.
- `readSSEStream`: `turnClosed` `:650` (set on `done` `:731` and
  `chainComplete` `:790`, reset on `turnStart` `:761`); an error frame throws
  `SalonSendError(errorMsg, {flags when boolean})` `:695-698`; lines carried
  across reads through `splitSseBuffer(buffer, decoder.decode(value, {stream:true}))`
  `:805-812` (`components/agent-stream/parse-agent-stream.ts:63-67`: split on
  `\n`, the last piece is the carried rest); the decoder tail flushed `:815-819`;
  `!turnClosed` at EOF → `setResponseStatus(null)` + `throw new StreamClosedEarlyError()` `:821-824`.
- An HTTP refusal before any stream →
  `new SalonSendError(errorMessage, {userMessagePersisted: false, pendingToolResultsPersisted: false})` `:1049`.
- `restoreUnpersistedSubmission` `:855-908`: `userRowMissing` = the frame's
  flag when it said, else "the temp user bubble still stands after the
  post-error read"; `toolRowsMissing` likewise over the temp TOOL bubbles;
  neither → false; **a non-empty composer → false (never clobber new
  words)**; else restore input (`setInput` + `inputRef.setMarkdown` +
  `persistDraft`), attachments, pending tool results; returns whether
  anything went back.
- Catch `:1186-1226`: `displayMessage` (`'Unknown error'` / `'TypeError'` →
  `Connection lost. Please try again.`); resets; **`await fetchChat()`
  BEFORE the restore and the toast**; then
  `showErrorToast(sendFailureToastText(displayMessage || 'Failed to send message', restored), SEND_FAILURE_TOAST_MS)`
  — ONE toast. The `finally` then clears provisional bubbles.
- `persistDraft?: (text) => void` threaded: `sendMessage`'s new last arg,
  `SendMessageArgs` (`useImpersonationVoice.ts:110-111`, `:125`, `:306`),
  `SalonView.tsx:1753`, `:1779`.

---

## 2. What v4 DELETED in the span that v5 still carries

| v4 deleted (pre) | v5 twin on `a7c6f0d03` |
|---|---|
| `components/tools/memory-housekeeping-card.tsx` (290) | `apps/web/src/app/screens/settings/memory/memory-housekeeping-card.ts` (279) + `.spec.ts` (180) |
| `components/tools/memory-consolidation-card.tsx` (301) | `screens/settings/memory/memory-consolidation-card.ts` (388) + `.spec.ts` (503) |
| `components/tools/memory-consolidation-report-dialog.tsx` (178) | `screens/settings/memory/memory-consolidation-report-dialog.ts` (275) + `.spec.ts` (347) |
| `components/tools/memory-dedup-card.tsx` (367) | `screens/settings/memory/memory-dedup-card.ts` (294) + `.spec.ts` (163) |
| `components/memory/housekeeping-dialog.tsx` (288) | `apps/web/src/app/memory/housekeeping-dialog.ts` (284) + `.spec.ts` (337) |
| `queryKeys.memories.housekeepingConfig` | `memory/memory.api.ts:36` `memoryKeys.housekeepingConfig` |
| the housekeeping / consolidate / sweep fetchers | `memory.api.ts:99-174` (`fetchHousekeepingPreview`, `runHousekeeping`, `fetchHousekeepingConfig`, `setHousekeepingConfig`, `runHousekeepingSweep`), `:250-267` (`consolidateMemories`) |
| the system-tools dedup actions | `core/core-client.ts:375-388` (`memoryDedupPreview`, `memoryDedupRun`); contract `core-contract.ts:2649-2650` (union), `:8614-8623` `DedupPreviewDto` + its `DedupCharacterResult` above it, `MemoryDedupPreviewRequest` / `MemoryDedupRunRequest` (~`:8727-8737`) |
| the retired verbs on the wire | `core-contract.ts:5971-6065` (Housekeeping region: `HousekeepingDetail`, `HousekeepingPreview`, `HousekeepingResult`, `MemoryHousekeepOptions`, `HousekeepingConfig`, the five request interfaces), `:6137-6143` (`MemoryExtractionConcurrency{Get,Set}Request`), `:6312-6324` (`MemoryConsolidateRequest`), union members `:6343-6347`, `:6358-6359`, `:6368`; Rust `api/types.rs:1689-1712` (`MemoryHousekeepPreview` / `MemoryHousekeep` / `MemoryHousekeepSweep` / `MemoryHousekeepingConfig{Get,Set}`), `:1725-1729` (`MemoryExtractionConcurrency{Get,Set}`), `:2876-2880` (`MemoryConsolidate`), `:1286-1295` (`MemoryDedup{Preview,Run}`) — server lane's to retire, SPA's to stop sending |
| `ConsolidationStats.mirrorFilesWritten`; `ConsolidationReport.skippedReason` `'character-not-found' \| 'archived'` | `core-contract.ts` `ConsolidationStats` (`mirrorFilesWritten`), `ConsolidationReport.skippedReason` (the three-member union) in the P4.D276 appendix `:6166-6325` |
| `help/memory-housekeeping.md`, `help/memory-consolidation.md` | `help/memory-housekeeping.md`, `help/memory-consolidation.md` (byte-equal to pre) |
| the help category slug `memory-housekeeping` | `apps/web/src/app/help/help-categories.ts:93`; `help/__fixtures__/help-guide-tables.json:77` |
| `INTER_CHARACTER_MEMORY` / `MEMORY_HOUSEKEEPING` / `MEMORY_CONSOLIDATION` job topics + activity rows | Rust only (no SPA job-type map: `apps/web/src/app/layout/activity-kinds.ts` holds only the chips) — server lane |

---

## 3. The behaviour deltas a port must hit (numbered; v4 site at the pin → v5 site on `a7c6f0d03`)

**SPA — Settings → Memory**

1. **Seven cards, not nine.** `MemorySearchTabContent.tsx:33-61` →
   `screens/settings/memory/memory-tab.ts:48-130`: delete the Housekeeping
   (`:68-75`), Consolidation (`:77-84`) and Deduplication (`:104-111`)
   cards; insert `Maintenance` / its description / `memory-maintenance`
   after `memory-backfill`; drop the three imports (`:9-12`); fix the
   doc comment (`:16-25`, "nine cards").
2. **Retired anchors open the new card.** `:18-22`, `:27` →
   `memory-tab.ts:142` `section` computed: map through
   `{memory-housekeeping|memory-consolidation|memory-deduplication → memory-maintenance}`
   before every `forceOpen` comparison.
3. **The Maintenance card** (§1.2) — NEW
   `screens/settings/memory/memory-maintenance-card.ts`: two settings
   documents (maintenance + consolidation), the phase switches with their
   exact disable rules (fold / demote / retire disabled by `!m.enabled`;
   Consolidate NOT), the whole-`phases` patch, the retention input on the
   CONSOLIDATION document with its null-on-blank / error sentence, the cap
   + overrides (draft-until-blur, whole-map patch, `exempt`), the
   particulars (§1.4 — `tfidfClusterThreshold` added, retention removed),
   `Run maintenance…`, the ledger. The v5 cards being deleted already hold
   reusable pieces: the profile option label
   (`memory-consolidation-card.ts:276`), the R-E "re-sync a control after a
   failed save" handling (`:60-76` comment), the override list
   (`memory-housekeeping-card.ts:62-120`).
4. **`useMemorySettingsDocument`** (§1.5) → a v5 helper (signal-based
   class or `inject`able factory) under `screens/settings/memory/`:
   seeded `{...defaults, ...settings}`, one patch per save, toast
   `` `${Noun} saved` ``, the read/write error texts through the existing
   `api-error-text.ts:22-35` `readErrorText` / `writeErrorText`.
5. **The ledger** (§1.3) — NEW `memory-maintenance-ledger.ts` +
   `summaryLine` pure function; the `48790fb73` muted "Within the cap" arm.
6. **The ledger must refresh on the `memories` topic.** v4's unscoped
   `memories` hint invalidates `queryKeys.memories.all` (`topic-map.ts:106`),
   which covers `maintenance-status`. v5's `core/realtime-topic-map.ts:136-144`
   narrows the unscoped hint to `memoryKeys.chatCounts()` (the recorded
   P4.D276 R-A / P4.D177 §C.4 divergence) — **so the ledger would never
   refresh after a run**. Add `memoryKeys.maintenanceStatus()` (and arguably
   `characterCounts()`) to the unscoped arm (§9 point S2).
7. **The Maintenance dialog** (§1.6) — NEW, replacing
   `memory/housekeeping-dialog.ts` (284): the picker variant (settings card)
   and the fixed-character variant (memory list); the exact request body and
   key order; `dryRunBlocked`; the cap-for-this-run only with demote; the
   two toasts; `queued` → invalidate the status key → `(queued)` →
   `(close)`. v5's modal idiom: `qt-modal` (`housekeeping-dialog.ts:169`) or
   the body-reparent idiom P4.D276 used for the digest viewer.
8. **The report view + table** (§1.7) — NEW `memory/maintenance-report.ts`
   (`PHASE_LABELS`, the three skip tables, the phase list, the four blocks,
   `ClusterBlock` reused from `memory-consolidation-report-dialog.ts`'s
   cluster rendering, the users-wide table); the `48790fb73` labels
   **`Clusters worth folding` over `clustersQualified`** and **`Exempt`**.
9. **The extraction limits on the Extraction Grain card** (§1.8) — NEW
   `memory-extraction-limits-section.ts`, mounted in
   `memory-extraction-grain-card.ts` after its error line (`:133-135`,
   inside the loaded block, in a `pt-4 border-t qt-border-default` div).
   **No v5 SPA reader of `memoryExtractionLimits*` exists today** (the
   variants exist, `core-contract.ts:6124-6129`, typed `config:
   Record<string, unknown>` — tighten).
10. **The memory list's button** (§1.9) → `memory/memory-list.ts:69-78`
    (`Cleanup` / `Clean up old and low-importance memories` → `Maintenance`
    / `Fold repetitions, consolidate, and keep this character under the cap`),
    `:206-212` (the dialog, no character name), `:260` (`showHousekeeping`),
    `:407-410` (`onHousekeepingComplete` → a queued handler re-reading the
    list from offset 0), `:20`/`:55` (imports), the doc comment `:43-46`.
11. **Importance band `.4 → .3`.** `importanceBand` → `memory/memory-format.ts:13-29`
    (`importanceLabel`, `importanceColorClass` — change `>= 0.4` to
    `>= 0.3`; the doc lines `:4`, `:13`, `:21`). Specs
    `memory-format.spec.ts:41` and `memory-card.spec.ts:59` move.
12. **The editor stops sending `source`** and **the reinforced toast**.
    `memory/memory-editor.ts:171-188`: drop `source: 'MANUAL'` (`:177`;
    and `MemoryWriteBag.source` at `core-contract.ts:5904-5911`); after a
    create, `merged = action` is not `insert` / `insert-linked` (v5's
    dispatch has no HTTP status — the reply body must carry `action`, §7.4)
    → `Already on file — the existing memory has been reinforced instead`.
    v5's `memoryCreate` reply today is `{memory}` only (`api/memories.rs:409`
    and the create path from `:494`) — the server lane must add `action`.
13. **The wipe / re-extract wording** (§1.10) →
    `screens/salon/memory-sidebar.api.ts:95` (confirm), `:100` (toast; read
    `sparedCount`), `screens/salon/memory-reextract.api.ts:30` (the one
    shared constant). **Owner question:** this is bug 201's SPA half
    (`124c8aca3`) — whichever lane ports the chat wipe's `sparedCount`
    should carry it (§9 point S6).
14. **The wire types** (§7.4): the maintenance verbs, the widened
    character counts (`demotable` / `protected` / `hotTotal` / `cold`, sort
    by demotable), `tfidfClusterThreshold`, the narrowed
    `ConsolidationReport`, retire the dead verbs from `MemoryRequest`
    (`core-contract.ts:6333-6369`) and `CoreRequest` (`:2649-2650`), and
    the dedup client methods (`core-client.ts:375-388`).
15. **The help category** `lib/help-guide/categories.ts` (`memory-housekeeping`
    → `memory-maintenance`) → `apps/web/src/app/help/help-categories.ts:93`
    + re-capture `help/__fixtures__/help-guide-tables.json` through the
    recorder `apps/web/oracle/help-guide-capture.test.tsx` at the pin (the
    only moved byte is that slug; the README forbids hand edits).

**Chat stream**

16. **Bug 242 — `inChain` only for `chainDepth > 0`.**
    `useSSEStreaming.ts:760` → `apps/web/src/app/core/chat-stream.reducer.ts:295-306`:
    `inChain: true` becomes `inChain: (frame.chainDepth ?? 0) > 0 ? true : s.inChain`
    (v4 never clears on depth 0; it only declines to set). v5's server
    emits the depth-0 event unconditionally
    (`crates/quilltap-core/src/services/orchestrator.rs:1178-1183`, the
    `chain_depth: 0` `turnStart` before every first turn; chained ones at
    `:4313-4320`) — so v5's reducer is in the PRE-fix state on every turn.
    **Measured effect in v5 (by reading, not run):** v5's end-of-turn work
    does NOT ride `inChain` — the refetch / `notifyQueueChange` / stranded-
    notice clear / provisional sweep all run at the dispatch reconcile point
    (`screens/salon/salon-conversation.ts:3737-3790`), and the empty-response
    toast rides `finalDone.emptyResponse` transitions
    (`salon-conversation.ts:3947-3952`), ungated by `inChain`. What `inChain`
    still decides in v5: `reduceDone`'s `emitBubble = prev.inChain ?
    content.trim().length > 0 : true` (`chat-stream.reducer.ts:400`) and the
    bubble's `intermediate` flag (`:413`, no reader outside the reducer). So
    v5 SHARES the flag bug, with a narrower symptom: a single-character
    turn whose `done` carries a `messageId` and empty / whitespace content
    (a tool-only turn) emits NO live bubble until the reconcile refetch,
    where v4 post-fix appends one at once. The fix is one line; the
    v4-visible symptoms (no refetch, no toast) were never v5's.
17. **Bug 241, server: the ledger and the frame's two flags.** v5's
    persistence sites: the fair-rotation pause
    (`orchestrator.rs:1131-1144` — `maybe_pause_for_user_seat_turn` writes
    the user message at `:940` before returning `Some`), the pending tool
    results (`:1712-1739`, the write at `:1736`), the user message
    (`:1828`). Each flips a per-request ledger AFTER its `db.write(...)`
    succeeds, with v4's three DEBUG lines (§1.14) under `quilltap::chat` (or
    the orchestrator's existing target). The ledger is created per
    `run_send` (`crates/quilltap-host/src/spine.rs:1452`) and passed to the
    initial AND chained `ProcessMessageInput`s (`make_chain_input`,
    `:1680-1708` — chained turns are continue-mode and never flip it).
    Both error shells read it: the initial-turn failure (`spine.rs:1617-1649`)
    and the chain failure (`:1720-1742`). `ChatErrorPayload`
    (`crates/quilltap-core/src/api/types.rs:5623-5630`) gains
    `user_message_persisted: Option<bool>` + `pending_tool_results_persisted:
    Option<bool>` serialized AFTER `details` as `userMessagePersisted`,
    `pendingToolResultsPersisted` (`skip_serializing_if = Option::is_none` so
    any other `chat_error` keeps the three-key frame — §9 point S8).
18. **Bug 241, server log lines.** v4's ERROR `Streaming error` now carries
    `{chatId, userMessagePersisted, pendingToolResultsPersisted}` and a DEBUG
    `[Orchestrator] Emitting error frame` `{chatId, errorType, …}` precedes
    the frame. v5's ERROR `Chat send failed while streaming the initial
    turn` (`spine.rs:1622-1627`) / `Turn chain failed after the initial turn`
    (`:1725-1730`) are v5's own (P4.18) — add the two fields, add the DEBUG.
    `errorType`: v4 sends `error.name` (`'Error'` for plain errors,
    `'validation'` for Zod); v5 hard-codes `"Error"` (`spine.rs:1633`,
    `:1736`) — recorded, unchanged by the span.
19. **Bug 241, the SPA's knowledge of the flags.** The SPA's catch
    (`salon-conversation.ts:3724-3735`) reads only the dispatch rejection.
    The `chat_error` frame rides the global `/api/events` stream and races
    the dispatch reply (`spine.rs` sends the event, THEN returns `Err`), so
    the flags must ride the **dispatch rejection** to be reliable: a new
    `CoreError` rider (`types.rs:5150`, the `already_saved` /
    `associations` precedent) → `CoreDispatchError` (`core-contract.ts:4917-4932`)
    gains them. Every `Err` out of `run_send` carries the ledger (a refusal
    before `process_message` — chat not found, a bad bag — carries
    `{false, false}`, v4's `:1049` arm). §7.3 has the shape; §9 point S7.
20. **Bug 241, the SPA restore.** `salon-conversation.ts:3566-3584`
    (`postComposedMessage` clears the composer via `clearAfterSend` and
    snapshots `pendingToolResults`) + `runTurn` `:3601-3802`: on a failed
    send (not `continueMode`) — after the reconcile read
    (`invalidateQueries` `:3746` + `settleTranscriptAfterTurn` `:3759`,
    BEFORE `clearProvisionalMessages` `:3787`) — decide
    `userRowMissing = rider ? !rider.userMessagePersisted : tempBubbleStillStanding`
    (the temp id is `temp-user-${Date.now()}`, `:4241`) and
    `toolRowsMissing = pending.length > 0 && (rider ? !rider.pendingToolResultsPersisted : <no temp tool bubbles in v5 — see §9 S9>)`;
    when the composer is EMPTY, restore text (+ draft) and attachments to the
    composer and the chips to `pendingToolResults` (`:3112`); then ONE
    toast `sendFailureToastText(display, restored)` for 8,000 ms
    (`ToastService.showError(message, duration?)`,
    `ui/toast.service.ts:78`). The toast must MOVE from the catch
    (`:3730`) to after the reconcile (v4 toasts after `fetchChat`). The
    composer (`chat/chat-composer.ts:1026-1032` `clearAfterSend`) needs a
    snapshot/restore pair: the tray is `UploadedChatFile[]` (`url`
    included, `chat/chat-files.api.ts:15-21`), which `ComposerSend`
    (`:79-88`) does not carry — so `clearAfterSend()` should hand back
    `{text, files}` (or the Salon holds the tray snapshot), and a
    `restoreAfterFailedSend(snapshot): boolean` refuses when the editor or
    source textarea is non-empty, sets the editor markdown + the source
    text + the `text` gate, re-adds the files, and writes the draft
    immediately (`persistDraft`, `:833-841`, bypassing the 800 ms debounce).
21. **The display text.** v4 toasts `` `${error}: ${details}` `` (e.g.
    `Failed to generate response: No API key configured for this connection profile.`
    + the note). v5's catch shows the bare `CoreError.message`
    (`e.to_string()` — the details only). Decide which the SPA shows (§9
    point S10); the restored-note composition is v4's either way.
22. **A possible pre-existing double toast** (observation, unmeasured):
    when the `chat_error` frame reaches the reducer before the dispatch
    rejects, `reportStreamTransitions` toasts `after.error`
    (`salon-conversation.ts:3953-3955`, the reducer's
    `${error}: ${details}`, `chat-stream.reducer.ts:235-239`) AND the catch
    toasts the bare message (`:3730`). v4's test pins exactly ONE toast
    (`toHaveBeenCalledTimes(1)`). The port should leave one (§9 S10).

**help / docs**

23. `help/` re-vendored WHOLE at the pin: 21 modified, 2 deleted, 2 added
    (§8.3); count stays **131** (`help_tree_embed_guard.rs:92`
    `VENDORED_FILE_COUNT`, `host_help_docs_boot.rs:112` literal — both
    stay `131`, their history comments gain the pin's line).
24. `docs/v4/` mirror to the pin (§8.4): 24 modified-or-renamed paths
    present in the mirror at PRE bytes (measured md5-equal), 49 additions.

---

## 4. v5 on `main` (`a7c6f0d03`) — per file in the slice

| File | Lines | What it does | Fate |
|---|---|---|---|
| `apps/web/src/app/screens/settings/memory/memory-tab.ts` | 149 | the nine cards on `section=` anchors | edit `:6-14` imports, `:16-25` doc, `:48-130` cards, `:142` section mapping |
| `…/memory-housekeeping-card.ts` (+spec 180) | 279 | per-user housekeeping config + overrides | DELETE (reuse the override list into the Maintenance card) |
| `…/memory-consolidation-card.ts` (+spec 503) | 388 | consolidation settings, Consolidate now | DELETE (reuse number-field / profile-select / R-E re-sync into the particulars + phase switches) |
| `…/memory-consolidation-report-dialog.ts` (+spec 347) | 275 | the report dialog; `SKIPPED_TEXT` `:20-23`; `Clusters found` `:239` | DELETE (reuse the cluster block into `maintenance-report.ts`) |
| `…/memory-dedup-card.ts` (+spec 163) | 294 | dedup preview/run over `core.memoryDedup*` | DELETE |
| `…/memory-extraction-grain-card.ts` (+spec 275) | 234 | the grain card | edit: mount the limits section after `:133-135` |
| `…/api-error-text.ts` | 35 | `readErrorText` / `writeErrorText` | reuse unchanged |
| `…/memory-cards-realtime.spec.ts` | 208 | backfill / regenerate / summaries hint specs | unchanged (no housekeeping card in it) |
| `apps/web/src/app/memory/housekeeping-dialog.ts` (+spec 337) | 284 | per-character Cleanup dialog | DELETE |
| `…/memory/memory-list.ts` (+spec 389) | 439 | the list + Cleanup button | edit (Δ10) |
| `…/memory/memory-card.ts` (+spec 306) | 209 | the card | no edit (band via `memory-format.ts`); spec `:59` title + values move |
| `…/memory/memory-editor.ts` (+spec 192) | 196 | create / edit | edit `:171-188` (Δ12); spec `:105`, `:175` move |
| `…/memory/memory-format.ts` (+spec 96) | 93 | bands, keywords, dedupe, `getErrorMessage` | edit `:4-29` (Δ11) |
| `…/memory/memory.api.ts` (+spec 116) | 267 | keys + dispatch helpers | edit: delete `:36` + `:99-174` housekeeping + `:250-267` consolidate; add maintenance / status / limits / repair? helpers + keys (`maintenanceConfig`, `maintenanceStatus`, `extractionLimitsConfig`) |
| `…/memory/memory-digest-viewer.ts`, `memory-badge.ts` | 68 / 46 | | unchanged |
| `apps/web/src/app/core/core-contract.ts` | 8,994 | the wire | edit the MEMORY region `:5850-6369` (+ `:2649-2650`, `:4917-4932`, `:4980-5060` ChatStreamFrame error keys, `:8614-8737` dedup DTOs) |
| `apps/web/src/app/core/core-client.ts` | — | `memoryDedupPreview` / `memoryDedupRun` `:375-388` | DELETE those two |
| `apps/web/src/app/core/realtime-topic-map.ts` | — | `memories` → `chatKeys.all` + chat-count keys `:136-144` | edit (Δ6) |
| `apps/web/src/app/core/chat-stream.reducer.ts` (+spec 350, 24 cases) | 493 | the pure SSE fold | edit `:295-306` (Δ16; `inChain: true` at `:299`); the error frame record `:235-239` may grow the flags |
| `apps/web/src/app/core/__fixtures__/frame-trace.ts` | — | hand-authored traces; `singleTurnTrace` `:19-43` has NO depth-0 `turnStart` although v5's server emits one | add a real-shape single-turn trace (depth-0 `turnStart` first) and a chain whose first turn is depth 0 |
| `apps/web/src/app/screens/salon/salon-conversation.ts` | 4,617 | the Salon spine (`runTurn` `:3601-3802`, `postComposedMessage` `:3566-3584`, `reportStreamTransitions` `:3897-…`, `settleTranscriptAfterTurn` `:4052-4058`, `clearProvisionalMessages` `:4193-4200`, `makeTempUserMessage` `:4222-…`) | edit (Δ19–22) |
| `apps/web/src/app/chat/chat-composer.ts` | — | `clearAfterSend` `:1026-1032`, draft `:806-852`, tray `:709`, `addAttachedFile` `:1075` | add snapshot/restore (Δ20) |
| `apps/web/src/app/screens/salon/memory-sidebar.api.ts`, `memory-reextract.api.ts` | — | wipe confirm/toast `:95`/`:100`; re-extract sentence `:30` | edit (Δ13, owner TBD) |
| `apps/web/src/app/help/help-categories.ts` (+`__fixtures__/help-guide-tables.json`) | — | `:93` | edit + re-capture (Δ15) |
| `crates/quilltap-core/src/services/orchestrator.rs` | — | `process_message` `:990-`; persistence sites `:940`, `:1131-1144`, `:1712-1739`, `:1828` | edit (Δ17) |
| `crates/quilltap-host/src/spine.rs` | — | `run_send` `:1452-`; error shells `:1617-1649`, `:1720-1742` | edit (Δ17–19) |
| `crates/quilltap-core/src/api/types.rs` | — | `ChatErrorPayload` `:5623-5630`; `CoreError` `:5150-` | edit (Δ17, Δ19) |
| `help/**` (131) | — | vendored tree, md5-equal to `4a38222e6` (measured: 0 diffs) | re-vendor (§8.3) |
| `docs/v4/**` | — | mirror, the 24 span-touched paths md5-equal to pre (measured) | re-vendor (§8.4) |

---

## 5. Harness, oracles, fixtures

**The help families (tier 1 / tier 2), all vendor-driven, none IMPORT-DEAD
at the pin** (`lib/help/**` is untouched in the span; only
`lib/help-guide/categories.ts` moved, one line):

- `crates/quilltap-harness/tests/help_tree_equivalence.rs` (oracle
  `harness/oracle/cases/help-tree-sync.test.ts`, `ensureHelpDocsSynced` over
  the pin's tree): **RED at the pin** until `help/` is re-vendored (walk
  order, docs, chunks all move); GREEN after. Regen from the pinned worktree
  (`--v4 <pin>`; recipe in the test header `:19-31`).
- `help_section_size_equivalence.rs` (oracle `cases/help-section-size.ts`,
  `js-tiktoken` in the PIN's `node_modules`): RED until re-vendored; the new
  161-line `memory-maintenance.md` is the section-size risk (v4's own
  `help-doc-size.test.ts` is green at the pin by construction).
- `help_tree_embed_guard.rs:92` — `VENDORED_FILE_COUNT` stays **131**; the
  doc block `:85-91` gains the pin's line ("131 at `64743ee13` (131 − 2 + 2)").
  Predicted green after the re-vendor (the embedded table follows the disk).
- `crates/quilltap-host/tests/host_help_docs_boot.rs:109-112` — literal 131
  stays; comment gains the line.
- **SPA tier 1:** `apps/web/oracle/help-guide-capture.test.tsx` re-captured
  at the pin → `help-guide-tables.json` (one slug moves, `:77`);
  `help-guide-vectors.json`, `label-from-url-vectors.json`,
  `param-routes-vectors.json`, `welcome-card.json` predicted byte-identical
  (the README records the last re-capture at `acadcc7cd`; add this one).
  `help-categories.spec.ts` (reads the fixture) goes RED on the fixture
  until `help-categories.ts:93` moves.

**The chat stream:**

- `crates/quilltap-harness/tests/orchestrator_tier3_equivalence.rs`
  (tier 3; oracle `harness/oracle/cases/orchestrator-tier3.test.ts`, v4's
  REAL `handleSendMessage`): it DROPS every `error` frame from the compared
  trace (`:1250-1262`) and compares only the error frames' `details`
  against `process_message`'s `Err` (`:1415-1430`). At the pin every
  recorded error frame gains `userMessagePersisted` /
  `pendingToolResultsPersisted` — invisible to the current comparison.
  **Rows to ADD:** compare the two flags per failing call against the v5
  ledger (the requires-gate arms already fail before the user write →
  `{false,false}`; ADD a failing call AFTER the user write — e.g. a provider
  that throws on stream after the save → `{true, false}`, and one with
  pending tool results → `{true, true}`). ⚠ **Predicted IMPORT-DEAD at the
  pin, independent of this slice:** the oracle `jest.doMock`s
  `@/lib/memory/memory-recap` (`orchestrator-tier3.test.ts:547-549`), a
  path deleted at the pin (moved to `lib/memory/recall/recap.ts`); jest
  refuses a non-virtual mock of a missing module — verify at the pin; the
  recall lane owns the re-point.
- No v5 differential pins `ChatErrorPayload`'s bytes against v4's
  `encodeErrorEvent` (v4's `streaming.service.test.ts` has the two new
  cases). Closest existing shape: `chat_events.rs:987-993` (the
  `turnStart` byte pin) — add a serde unit pin for the five-key and
  three-key frames.
- The SPA reducer: `chat-stream.reducer.spec.ts` (24 cases) — ADD v4's two
  `useSSEStreaming-single-turn-done.test.tsx` cases (§6) as fold cases
  (depth-0 `turnStart` → `done` with content → `inChain` false, a
  non-intermediate bubble; with `emptyResponse` → `finalDone.emptyResponse`
  set) + one asserting a depth-1 `turnStart` still sets `inChain`, + the
  empty-content depth-0 `done` emitting a bubble.

**Memory SPA:** no Rust harness family is the SPA's. The server families
that pin the wire the SPA reads — `memory_consolidation_routes_equivalence.rs`
(`PARTIAL_DEFAULTS_CLOBBER` `:355-396`, which TRIPS at the pin by design —
bug 244), the memories route families (`housekeep*`,
`housekeeping-config`, `extraction-concurrency`, `consolidate` retire) —
are the server-API lane's; their response bodies are what §7.4 must match.

**Closest recipes to copy:** for a help re-vendor, the P4.D278 order
(`work-orders/p4.d278-vendor-help-131-docs-v4-noport-ratifications.md`
Tier 1 items 1–4) — re-vendor whole, the three count pins, the two regens
at the pin, the docs mirror, the md5 census; for the SPA, P4.D276's order
(Ownership / R-E / R-F / §S.9 gating idiom).

---

## 6. v4 jest tests at the pin covering the slice (mirror these)

| Test | Cases | What to mirror |
|---|---|---|
| `__tests__/unit/components/tools/memory-maintenance-card.test.tsx` (139) | 4 | one phase toggle posts `{phases:{foldNearDuplicates:true, demote:false, retire:true}}`; `^consolidate` posts `{enabled:true}` to the consolidation doc; blank retention → `{coldRetentionDays:null}` (input shows `90` first); typed cap on blur → `{perCharacterCap:3000}` and the input keeps 3000; the ledger renders `100 archived (2100/2000 counted)` and the name |
| `__tests__/unit/components/memory/maintenance-dialog.test.tsx` (103) | 3 | fixed character, consolidate unticked → body `{characterId, phases:['fold-near-duplicates','demote','retire'], dryRun:true}`, `Would archive`, `no archive retention window is set`; cap 500 → `settings:{cap:500}`; `__all__` disables Dry run, Run for real posts `{phases:[…all four], dryRun:false}`, toast contains `2 characters`, `onQueued` + `onClose` |
| `__tests__/unit/components/memory/memory-list.test.tsx` (179) | 5 | `opens the maintenance dialog for this character` (replaces the housekeeping case) |
| `__tests__/unit/hooks/useSSEStreaming-single-turn-done.test.tsx` (144) | 2 | `runs the end-of-turn work after a depth-0 turnStart and done`; `shows the empty-response toast after a depth-0 turnStart` |
| `__tests__/unit/hooks/useSSEStreaming-send-failure.test.tsx` (336) | 11 | pre-persist frame → ONE toast containing the details + the note, 8,000 ms, restored; split frame (transport — §9 S11); persisted → no note, not restored; HTTP refusal → restored; new words in the composer → not restored; EOF without terminal → `STREAM_CLOSED_EARLY_MESSAGE` (transport); read shows no row → restored; read reconciled → not restored; two split-read parsing cases (transport); `sendFailureToastText` (three asserts) |
| `__tests__/unit/lib/services/chat-message/streaming.service.test.ts` (566, 32) | +2 | the five-key frame with flags; the three-key frame without |

v4 shipped no test for `memory-maintenance-ledger.tsx`'s `summaryLine`,
`maintenance-report.tsx`, `memory-consolidation-particulars.tsx`,
`memory-extraction-limits-section.tsx` or `useMemorySettingsDocument` —
the lane writes its own (red-first against unported `main`).

---

## 7. Cross-lane meeting points + the proposed SPA-side wire contract

### 7.1 What this slice NEEDS from the server keystone / API lane

- Request variants (Rust `api/types.rs` + engine arms) for:
  `maintain`, `maintenance-status`, `maintenance-config` GET/POST, and the
  existing `extraction-limits-config` (moved to instance settings by
  `e0ac7493a`), `consolidation-config` (with `tfidfClusterThreshold`),
  `character-memory-counts` (widened). `repair` is CLI-only in v4 (§1.12);
  the SPA does not need it — the API lane may still port it.
- The memory-create reply's `action` (Δ12).
- The `MEMORY_MAINTENANCE` job publishing `memories` + `mountPoints` on
  commit (v4 `job-topics.ts`) — the ledger's refresh depends on it (Δ6).
- The chat spine's ledger + rider (Δ17–19) — if the chat-spine server half
  is a different lane, the SPA half depends on the rider's spelling.

### 7.2 PROPOSED Rust spellings (the server lane decides; listed so the SPA can be written against them)

```rust
// crates/quilltap-core/src/api/types.rs — Request (camelCase `type` tags)
MemoryMaintain { options: serde_json::Value },          // the RAW maintain bag (the MemoryHousekeep / MemoryConsolidate precedent)
MemoryMaintenanceStatus { #[serde(default)] character_id: Option<String> },
MemoryMaintenanceConfigGet,
MemoryMaintenanceConfigSet { config: serde_json::Value },
MemoryRepair { options: serde_json::Value },            // optional for the SPA
// retire: MemoryHousekeepPreview, MemoryHousekeep, MemoryHousekeepSweep,
//         MemoryHousekeepingConfigGet/Set, MemoryConsolidate,
//         MemoryExtractionConcurrencyGet/Set, MemoryDedupPreview/Run
```

Responses ride the existing memories-family envelope `Response::Memory(Value)`
— **not** `Response::MemoryMaintenance(Value)` (`types.rs:4548-4551`), which
already names the P4.9H2A dedup / summaries family; reusing that variant name
for the new verbs would be a misleading collision (§9 S3).

```rust
// the chat-send ledger (crates/quilltap-core/src/services/orchestrator.rs)
#[derive(Debug, Default, Clone, Copy)]
pub struct SubmissionLedger {
    pub user_message_persisted: bool,
    pub pending_tool_results_persisted: bool,
}
// threaded as `&std::cell::Cell<SubmissionLedger>` or `&mut SubmissionLedger`
// on ProcessMessageInput / OrchestratorDeps — the lane picks; process_message
// returns Err after a flip, so the ledger must live OUTSIDE the Result.

// ChatErrorPayload (types.rs:5623) — append, field order = v4 key order
#[serde(rename = "userMessagePersisted", skip_serializing_if = "Option::is_none")]
pub user_message_persisted: Option<bool>,
#[serde(rename = "pendingToolResultsPersisted", skip_serializing_if = "Option::is_none")]
pub pending_tool_results_persisted: Option<bool>,

// CoreError (types.rs:5150) — a rider on the dispatch rejection
#[serde(skip_serializing_if = "Option::is_none")]
pub submission: Option<SubmissionLedgerDto>,   // {userMessagePersisted, pendingToolResultsPersisted}
```

### 7.3 The SPA-side error rider

```ts
// core-contract.ts — CoreError gains
submission?: { userMessagePersisted: boolean; pendingToolResultsPersisted: boolean };
// CoreDispatchError gains
readonly submission?: { userMessagePersisted: boolean; pendingToolResultsPersisted: boolean };
// ChatStreamFrame gains (error frame only)
userMessagePersisted?: boolean;
pendingToolResultsPersisted?: boolean;
```

### 7.4 PROPOSED SPA wire types — to be pinned VERBATIM against the server API lane

Transcribed from the pin (`lib/memory/maintenance/types.ts`,
`lib/memory/maintenance/ledger.ts:250-256`, `lib/memory/consolidation.ts:101-192`,
`lib/memory/repair.ts:25-45`, `lib/memory/retire.ts:455-462`,
`lib/schemas/settings.types.ts`, `route.ts`). Declaration order = wire order.

```ts
// ---- vocabulary -------------------------------------------------------------
export const MAINTENANCE_PHASES = ['fold-near-duplicates', 'consolidate', 'demote', 'retire', 'mirror'] as const;
export type MaintenancePhase = (typeof MAINTENANCE_PHASES)[number];
export type MaintenanceReason = 'scheduled' | 'watermark' | 'manual';
export type PhaseStatus = 'ran' | 'skipped';
export type PhaseSkipReason =
  | 'not-requested' | 'maintenance-disabled' | 'consolidation-disabled'
  | 'phase-disabled' | 'no-retention-window' | 'nothing-touched';

export interface PhaseOutcome {
  phase: MaintenancePhase;
  status: PhaseStatus;
  skippedBecause?: PhaseSkipReason;
  durationMs: number;
}
export interface NearDuplicateGroup { survivorId: string; absorbedIds: string[]; minCosine: number; }
export interface FoldPhaseReport {
  groupsDeferred: number; eligible: number; deferred: number;
  groups: NearDuplicateGroup[]; folded: number; groupsFailed: number;
}
export interface DemotePhaseReport {
  cap: number; hotTotal: number; demotable: number; protected: number; excess: number;
  demoted: number; demotedIds: string[]; highestScoreDemoted: number | null;
}
export interface RetirePhaseReport { retentionDays: number; retired: number; retiredIds: string[]; }
export interface MirrorPhaseReport { subjects: string[]; filesWritten: number; deferredToCommit: boolean; }

export interface MaintenanceReport {
  characterId: string;
  characterName: string | null;
  reason: MaintenanceReason;
  dryRun: boolean;
  startedAt: string;
  finishedAt: string;
  durationMs: number;
  skippedReason?: 'character-not-found' | 'archived';
  phases: PhaseOutcome[];
  fold?: FoldPhaseReport;
  consolidation?: ConsolidationReport;   // the P4.D276 type, amended below
  demote?: DemotePhaseReport;
  retire?: RetirePhaseReport;
  mirror?: MirrorPhaseReport;
}

export interface MaintenanceSummary {
  characterId: string;
  reason: MaintenanceReason;
  dryRun: boolean;
  finishedAt: string;
  durationMs: number;
  skippedReason?: 'character-not-found' | 'archived';
  phases: PhaseOutcome[];
  fold?: { eligible: number; groups: number; folded: number; groupsFailed: number; groupsDeferred: number };
  consolidation?: {
    digestsCreated: number; digestsUpdated: number; membersSuperseded: number;
    clustersFailed: number; clustersDeferred: number; skippedReason?: string;
  };
  demote?: Omit<DemotePhaseReport, 'demotedIds'>;
  retire?: { retentionDays: number; retired: number };
  mirror?: { subjects: number; filesWritten: number; deferredToCommit: boolean };
}

/** GET ?action=maintenance-status → { success, ledger: MaintenanceLedgerStatus[] } */
export interface MaintenanceLedgerStatus {
  characterId: string;
  lastRun: Partial<Record<MaintenancePhase, { at: string; reason: MaintenanceReason }>>;
  lastSummary: MaintenanceSummary | null;
  lastDemotion: { at: string; demoted: number; excess: number; effective: boolean } | null;
}

// ---- settings documents (instance-wide; GET → {success, settings}; POST patch → {success, settings}) ----
export interface MemoryMaintenancePhaseToggles { foldNearDuplicates: boolean; demote: boolean; retire: boolean; }
export interface MemoryMaintenanceConfig {
  enabled: boolean;                               // default false
  perCharacterCap: number;                        // int 100..100000, default 2000
  perCharacterCapOverrides: Record<string, number>; // each int 100..100000
  phases: MemoryMaintenancePhaseToggles;          // default all true
}
export interface MemoryExtractionLimitsConfig {
  enabled: boolean;            // default false
  maxPerHour: number;          // int 1..10000, default 20
  softStartFraction: number;   // 0..1, default 0.7
  softFloor: number;           // 0..1, default 0.7
}
// MemoryConsolidationConfig (P4.D276) GAINS, between clusterThreshold and minClusterSize:
//   tfidfClusterThreshold: number;  // 0..1, default 0.30
export const MEMORY_CAP_BOUNDS = { min: 100, max: 100_000 } as const;

// ---- maintain ----------------------------------------------------------------
export interface MemoryMaintainOptions {
  characterId?: string;                           // omitted = every character the user owns
  phases?: Exclude<MaintenancePhase, never>[];    // min 1; the server adds 'mirror' when 'consolidate' is named
  dryRun?: boolean;
  settings?: {
    consolidation?: Partial<MemoryConsolidationConfig>;
    maxClustersPerRun?: number;                   // int 1..1000 (dry runs clamp to 40, default 10)
    cap?: number;                                 // int 100..100000
  };
}
/** POST ?action=maintain */
export type MemoryMaintainReply =
  | { success: true; dryRun: true; report: MaintenanceReport }
  | { success: true; dryRun: true; reports: MaintenanceReport[] }
  | { success: true; dryRun: false; jobId: string | null; jobIds: string[] };

// ---- character counts (widened; sorted demotable desc, memoryCount desc) -----
export interface CharacterMemoryCount {
  id: string;
  name: string;
  memoryCount: number;   // hot + cold
  hotTotal?: number;
  demotable?: number;    // hot AUTO — what the cap measures (bug 210)
  protected?: number;    // hot MANUAL + digests — "exempt"
  cold?: number;
}

// ---- repair (CLI-only in v4; include if the API lane ports it) ----------------
export interface MemoryGraphRepairReport {
  dryRun: boolean;
  characterId: string | null;
  found: {
    related: { links: number; rows: number };
    consolidatedFrom: { links: number; digests: number };
    supersededBy: number;
    missingEmbeddings: number;
  };
  repaired?: {
    relatedRowsTouched: number; digestsTouched: number; digestsRetired: number;
    membersCut: number; charactersAffected: number;
    embeddingJobsQueued: number;
    embeddingSkippedReason?: 'no-embedding-profile';
  };
}

// ---- ConsolidationReport amendments (P4.D276 appendix) ------------------------
// ConsolidationStats: ADD `demotedCandidates: number` after `candidates`;
//                     DROP `mirrorFilesWritten`.
// ConsolidationReport.skippedReason: narrow to `'no-llm'`.

// ---- memory create reply ------------------------------------------------------
export type AdmissionActionKind = 'insert' | 'insert-linked' | 'reinforce' | 'absorb' | 'promote-and-reinforce';
export interface MemoryCreateReply { memory: MemoryDto; action: AdmissionActionKind; } // v4 201 iff insert|insert-linked
// MemoryWriteBag: DROP `source`.
```

Request interfaces (SPA, names binding at name level with the Rust
variants): `{type:'memoryMaintain'; options: MemoryMaintainOptions}`,
`{type:'memoryMaintenanceStatus'; characterId?: string}`,
`{type:'memoryMaintenanceConfigGet'}`,
`{type:'memoryMaintenanceConfigSet'; config: Partial<MemoryMaintenanceConfig>}`
(note `phases` may be partial on the wire — v4 merges — but the card sends
the whole object), `memoryExtractionLimitsSet.config:
Partial<MemoryExtractionLimitsConfig>` (tightened), optional
`{type:'memoryRepair'; options: {characterId?: string; dryRun?: boolean}}`.

### 7.5 What this slice PROVIDES to siblings

The `apps/web/**` memory + Salon surfaces, the `help/**` + `docs/v4/**`
trees, the help-guide fixture. Shared file with any other SPA lane:
`core/core-contract.ts` — carve by region (the MEMORY region `:5850-6369`,
the chat-stream frame `:4980-5060`, `CoreDispatchError` `:4917-4932`, the
dedup DTOs `:8600-8740`). The scenarios SPA (`545e299bb`) touches other
regions.

---

## 8. SPA / help / docs / CLI touch points

### 8.1 Playwright specs that move (`apps/web/e2e/`)

| Spec | Today | At the pin |
|---|---|---|
| `settings-memory-consolidation-flow.spec.ts` (450) | LIVE anchors for `section=memory-consolidation` / `memory-extraction-grain` (`:223-246`); GATED Consolidation save / dry run / `Consolidate for real` (`:278-357`), `Cleanup archives … Archive N Memories` (`:359-411`), digest viewer (`:413-`) | rewrite: `section=memory-maintenance` + each retired anchor force-opening the Maintenance card; the switches + cap + particulars saving; `Run maintenance…` dry run (one character) + `Run for real` (queue); the memory list's `Maintenance` dialog replacing `Cleanup`; the Extraction Grain's Rate limits; the digest viewer beat survives |
| `settings-memory-flow.spec.ts` (186) | `memory deduplication: preview renders …` `:144-165` (skipped behind `P49H2A_MAINTENANCE_LANDED`) | DELETE the beat (the card is gone); doc `:13`, `:21` |
| `settings-flow.spec.ts:834-850` | asserts headings `Memory Housekeeping`, `Consolidation`, `Memory Deduplication` | → `Maintenance` (and the six others) |
| `salon-memories-realtime-flow.spec.ts` (`:18-20`, `:42`, `:71-92`) | fires `memoryHousekeepSweep` (a `MEMORY_HOUSEKEEPING` job) to prove the pushed `memories` hint | re-aim at `memoryMaintain` real run (all characters, the fixture's default disabled switches → a manual run still runs every asked phase — check it stays a no-op on the fixture's counts: cap 2000, retention null) or another `memories` publisher |
| (new) a Salon send-failure beat | — | a profile without an API key → the composer gets the text back with the note (bug 241's live shape); and a single-character turn emitting a live bubble (bug 242) — optional, the vitest covers both |

### 8.2 Vitest specs that move

DELETE with their components: `memory-housekeeping-card.spec.ts`,
`memory-consolidation-card.spec.ts`, `memory-consolidation-report-dialog.spec.ts`,
`memory-dedup-card.spec.ts`, `memory/housekeeping-dialog.spec.ts`.
EDIT: `memory-tab.spec.ts` (`:13-14` nine → seven, the retired-anchor
mapping), `memory-list.spec.ts` (the dialog button), `memory-card.spec.ts:59`,
`memory-format.spec.ts:41-55`, `memory-editor.spec.ts:105-133`, `:175-`
(no `source`; reinforced toast), `memory.api.spec.ts:52-116` (housekeeping /
consolidate helpers gone; maintenance helpers new),
`memory-extraction-grain-card.spec.ts` (the section mounts),
`help-categories.spec.ts` (fixture), `chat-stream.reducer.spec.ts`,
`salon-conversation.spec.ts` (send-failure restore; ~`:2795` "a turn ending
by the ERROR arm" neighbours), `salon-impersonation-voice.spec.ts:270-334`
(the `clearAfterSend` spies — a snapshot return value must not break them),
`chat-composer.spec.ts` (restore), possibly `memory-sidebar.api.spec.ts`
(wipe wording). NEW: maintenance card / ledger (`summaryLine`) / dialog /
report / particulars / limits section / settings-document helper.

### 8.3 `help/` — exact diff `4a38222e6..64743ee13` (25 paths; 131 → 131)

Vendored at `<repo>/help/` (v5, 131 files, md5-equal to `4a38222e6` —
measured), embedded by `crates/quilltap-host/build.rs` →
`help_content::EMBEDDED_HELP` (`src/help_content.rs`), read by
`files_store::embedded_help_source_files` for the boot reconcile
(`HelpDocReconcileGate::ensure`) and `EMBEDDING_REINDEX_ALL` (p4.9i2).

| Path | Status | Commits |
|---|---|---|
| `help/memory-maintenance.md` | **A** (161 lines, `url: /settings?tab=memory&section=memory-maintenance`) | `b045dd986`, `48790fb73` |
| `help/scenario-browser.md` | **A** (52 lines) | `545e299bb` (the scenarios lane's surface) |
| `help/memory-consolidation.md` | **D** | (touched `c2795733f`, `af48f6cc2`, `ac6348c5f`, `4ae0c4ca6`, `104e986c7`; deleted `b045dd986`) |
| `help/memory-housekeeping.md` | **D** | (touched `c2795733f`, `e0ac7493a`, `ac6348c5f`, `104e986c7`; deleted `b045dd986`) |
| `help/characters.md` | M | `c2795733f`, `8af764514`, `b045dd986` |
| `help/chat-message-actions.md` | M | `e0ac7493a` |
| `help/chat-participants.md` | M | `124c8aca3`, `ac6348c5f` |
| `help/chat-settings.md` | M | `c2795733f` |
| `help/chats.md` | M | `545e299bb`, `93c5af6ad` (bug 241's paragraph "Should a send come to grief …") |
| `help/cli-memories.md` | M | `c2795733f`, `104e986c7`, `b045dd986`, `48790fb73` |
| `help/conversation-summary-regenerate.md` | M | `b045dd986` |
| `help/data-retention.md` | M | `b045dd986` |
| `help/embedding-profiles.md` | M | `af48f6cc2`, `93c5af6ad`, `4ae0c4ca6`, `b045dd986` |
| `help/episodic-memory.md` | M | `c2795733f`, `e0ac7493a`, `8af764514`, `584d28164`, `b045dd986` |
| `help/general-scenarios.md` | M | `545e299bb` |
| `help/groups.md` | M | `545e299bb` |
| `help/memory-playing-a-character.md` | M | `b045dd986` |
| `help/memory-recall-relevance.md` | M | `e0ac7493a`, `584d28164`, `104e986c7`, `b045dd986` |
| `help/memory-regenerate.md` | M | `124c8aca3`, `c2795733f`, `e0ac7493a`, `ac6348c5f`, `b045dd986` |
| `help/post-office.md` | M | `69d38dd60` |
| `help/project-scenarios.md` | M | `545e299bb` |
| `help/settings.md` | M | `104e986c7`, `b045dd986` |
| `help/system-import-export.md` | M | `8050958be`, `93c5af6ad` |
| `help/system-tools.md` | M | `124c8aca3`, `104e986c7`, `b045dd986` |
| `help/the-almanack.md` | M | `b045dd986` |

No v5 surface outside `help/`, `docs/`, the help-guide fixture and the
retiring SPA cards names the two deleted slugs (`rg memory-housekeeping|memory-consolidation`
outside them hits only comments, oracle cases, the e2e specs of §8.1, and
`services/llm_logging.rs:435` — a TASK-TYPE string, not the help slug).
No spaced filename in the help span.

### 8.4 `docs/v4/` mirror — every `docs/` path the span touches (73: 18 M, 6 R, 49 A) → v5 path

Mirror rule: `docs/<p>` → `docs/v4/<p>`; `packages/quilltap/README.md` →
`docs/v4/packages-quilltap-README.md` (moved by `c2795733f`, `8af764514`,
`ac6348c5f`, `93c5af6ad`, `584d28164`, `104e986c7`, `b045dd986`; v5 mirror
= pre, measured). The span carries **no spaced docs filename** (the one
spaced mirror file, `docs/v4/developer/features/complete/Wardrobe UX Overhaul.md`,
is untouched — memory note `v4-path-quoting-traps.md` still applies to any
whole-tree loop). Measured: every M/R source below exists in the mirror at
PRE bytes.

- **Modified (18), mirror present:** `CHANGELOG.md`; `developer/API.md`,
  `BACKGROUND_JOBS_CHILD.md`, `CLI.md`, `DATABASE_ABSTRACTION.md`, `DDL.md`,
  `DEAD-CODE-REPORT.md`, `DEVELOPMENT.md`, `SYSTEM_FLOWCHARTS.md`,
  `bugs.md`; `developer/features/ROADMAP.md`,
  `features/complete/memory_management.md`,
  `features/complete/wardrobe-folders.md`, `features/native-port-phase-0.md`,
  `features/prompt-trust-and-anti-committee.md`,
  `features/recall-multiplier-retuning.md`,
  `features/recall-probe-set-runbook.md`, `features/scenario-builder.md`.
- **Renamed into `features/complete/` (6)** — the old mirror path exists,
  the new does not; the re-vendor MOVES them (`git mv` in the mirror, then
  overwrite with the pin's bytes — R077–R098, so the content moved too):
  `commonplace-relevance-fix.md`, `episodic-recall-overhaul.md`,
  `memory-consolidation-and-tiers.md`, `memory-recall-and-housekeeping-fixes.md`,
  `memory-system-refactor.md`, `scenarios-from-anywhere.md`.
- **Added (49):** `developer/features/builtin-tfidf-memory-tuning.md`; and
  `developer/bugs/fixed/bug-199-*` … `bug-246-*` (48 files: 199–202,
  203–226, 227, 228, 229 (the picker, renumbered), 230–246 —
  `bug-242-single-turn-skips-ondone.md`, `bug-241-send-failure-loses-text.md`,
  `bug-235-two-vector-caches.md` among them). Note the intermediate
  `docs/developer/bugs/bug-2xx-*.md` (open) files created and moved within
  the span net out — only `fixed/` lands.
- Untouched in the span: `docs/releases/**`, `public/schemas/**` bar
  `qtap-export.schema.json` (the carriers lane's re-vendor, not this slice).

### 8.5 CLI

v4's CLI changes in the span (`packages/quilltap/lib/memories-commands.js`
— `memories maintain`, `validate --repair`, `db memories --tier`,
`memories status`'s ledger lines, `--instance` port resolution in
`lock-helpers.js`) have **no v5 home today**: `quilltap memories …` hits
`not_yet_available` (`crates/quilltap-cli/src/main.rs:49-64` lists
`memories` in `SUBCOMMANDS`; `:250-258` dispatches only `db`, `sync`,
`docs`, `instances`, `completion`, `recall-replay`, `anchor-probe`); `quilltap
db memories` hits the db verb refusal (`db_cmd.rs:26-38` lists it;
`:286-296` dispatches only `characters` / `optimize`, the rest answer
`db subcommand '{other}' is recognized but not yet available in this build of the quilltap CLI.`).
The CLI `help/cli-memories.md` page is vendored regardless.

---

## 9. Open points for the order (observations / questions — not rulings)

**SPA**

- **S1. Where the memory SPA lives in the round.** The SPA half (§3 Δ1–15)
  depends on the server lane's verbs (§7.1). The P4.D276 idiom fits: author
  against the proposed types, gate the live Playwright beats behind a
  `…_SERVER_LANDED = false` constant, the unifier flips them (§S.9).
- **S2. The ledger refresh (Δ6).** v5's unscoped `memories` hint maps to
  `memoryKeys.chatCounts()` only (a recorded divergence). The Maintenance
  ledger (and the cap overrides' counts) will not refresh after a queued run
  unless the unscoped arm adds `memoryKeys.maintenanceStatus()` (+
  `characterCounts()`), or widens to `memoryKeys.all` as v4 does. Planner /
  human: narrow-add or widen.
- **S3. Variant naming.** `Response::MemoryMaintenance` already exists for
  the P4.9H2A dedup / summaries family (`types.rs:4548-4551`); the new verbs
  should not reuse it (or the old one is renamed when the dedup verbs
  retire). Server-lane call.
- **S4. `repair` in the SPA.** v4 has no SPA caller; listed in §7.4 only so
  the API lane and the SPA agree if it is ported. YAGNI says the SPA does
  not add one.
- **S5. The extraction limits are a NEW SPA surface** (no v5 reader; v4
  pre had none either). Small, but it needs the server's instance-settings
  move (`e0ac7493a`) to be real; until then a v5 build reading the
  per-user column is a LIVE-DATA hazard (ledger §1 (1)).
- **S6. Bug 201's SPA wording (Δ13)** sits with whichever lane ports the
  chat wipe's `sparedCount` (`124c8aca3`); flag it in that order as a
  rider, or give it to the SPA lane with a HANDOFF on the reply key.
- **S7. The `CHARACTER_MEMORY_COUNTS` sort** moves to `demotable desc`;
  the dialog's option order follows it. Cosmetic, but a spec pinning option
  order moves.

**Stream**

- **S8. The rider's channel (Δ19).** The flags must ride the dispatch
  rejection (`CoreError`) to be race-free; the frame keeps them for v4 byte
  parity and the tier-3 harness. Ruling wanted: a `submission` object
  rider (proposed) vs two flat fields; and whether every `chat_error`
  emitter (the two spine shells; the help-chat loop's mid-stream
  `ChatEvent::Error` is a different variant and stays three-key) carries
  them.
- **S9. Temp TOOL bubbles.** v4 pushes optimistic TOOL bubbles for the
  pending results and falls back to "still standing?" for them; v5's
  `runTurn` pushes only the user bubble (`:3669-3677`). With the rider
  always present on a server-side failure, the fallback is needed only for
  a dropped connection — then v5 cannot tell whether the TOOL rows landed.
  Options: assume "not restored" for tool chips on a dropped connection, or
  read the transcript for TOOL rows newer than the send. Recorded
  divergence candidate.
- **S10. The toast's text and count (Δ21–22).** v4: ONE toast,
  `` `${error}: ${details}` `` + the note, 8 s. v5 today: possibly two
  (the frame's `after.error` via `reportStreamTransitions` AND the catch),
  the catch's carrying only `details`. Measure on a live server (a profile
  with no key) before choosing; then keep exactly one, after the
  reconcile.
- **S11. The transport halves of bug 241 — NO-PORT candidates.** (a) The
  split-frame buffer (`splitSseBuffer`): v5's HTTP client reads events
  through ONE browser `EventSource` (`core/core-transport.ts:70-160`),
  which buffers SSE frames by spec; the Tauri shell delivers whole events
  over IPC — a split frame cannot reach the reducer. (b) The early-close
  error (`StreamClosedEarlyError`): v5's turn terminal is the
  `dispatch(chatSend)` reply, not the event stream's EOF
  (`salon-conversation.ts:3682-3735`); a dropped dispatch already lands in
  the catch as `Connection lost. Please try again.` (`:3727-3730`, v4's
  same mapping, still at the pin `:1189-1191`). What DOES port from (b) is
  the read-fallback restore on that dropped-connection arm. Ratify (a) and
  the error half of (b) as NO-PORT with this evidence; port the restore.
- **S12. Bug 242 in v5 (Δ16)** — shared in the flag, with a narrower
  symptom (no live bubble for an empty-content single-character `done`);
  the dogfood measurement the ledger asks for: a tool-only reply in a
  single-character chat — does a live bubble appear before the refetch?
  The fix is one line regardless; v4's other symptoms cannot occur in v5
  (end-of-turn work is at the dispatch reconcile). Also: `frame-trace.ts`'s
  hand-authored `singleTurnTrace` omits the depth-0 `turnStart` the real
  server emits — the fixture misled every reducer spec; re-author it.
- **S13. Pre-existing nuance, not in the span:** v4's intermediate path
  skips on `!fullContent` (whitespace survives); v5's on
  `content.trim().length === 0` (`chat-stream.reducer.ts:400`). Record or
  align while there.
- **S14. The tier-3 orchestrator oracle** is predicted IMPORT-DEAD at the
  pin through its `@/lib/memory/memory-recap` doMock (§5) — whoever re-points
  it should add bug 241's flag rows in the same regen.

**NO-PORT evidence (for the unifier's ratification)**

- **N1. `4ce64fa1e` (merge).** Parents `97f0d8cb8` and `8050958be`.
  `git diff --stat 8050958be 4ce64fa1e` lists exactly the ten paths of
  `git show --stat 97f0d8cb8`, with identical line counts, except
  `bugs/fixed/bug-199-folder-picker-closes-dialog.md` →
  `bug-229-folder-picker-closes-dialog.md`; `git show --cc 4ce64fa1e`'s own
  resolution touches only the README badge (`dev.155`), the bug number in
  two code comments (`WardrobeBrowser.tsx`, `useWardrobeItemActions.ts`),
  one test title, `bugs.md`, `CHANGELOG.md`. v5 already uses **229**
  everywhere (`apps/web/src/app/wardrobe/wardrobe-item-actions.ts:7,15,107`,
  `wardrobe-control-dialog.ts:105,793,1178`, `wardrobe-browser.spec.ts:15,400,421`,
  `wardrobe-control-dialog.spec.ts:1783`) — P4.D285 ported `97f0d8cb8`
  already under the merged number. Nothing to port; the bug file lands
  with the docs mirror (§8.4).
- **N2. `9ec4fa2e9`.** Ten files: README, CHANGELOG, `bugs.md`, two OPEN bug
  files (245 / 246 — fixed by `64743ee13` in the same span, so only their
  `fixed/` copies land), the refactor spec's acceptance-walk notes, three
  version stamps, and ONE code hunk —
  `packages/quilltap/lib/memories-commands.js:1319-1326`
  (`renderMaintenance`): `excess === 0` prints
  `last demotion:       within the cap` (+ `; automatic demotion looks again in an hour`
  when ineffective) instead of `{demoted} of {excess} over the cap …`.
  v5 has no `memories` CLI family (§8.5 — `main.rs:250-258`,
  `not_yet_available` `:102-108`), so the line has no home; ratify NO-PORT
  for this round, and the order that ports `memories status` carries it
  (it mirrors the SPA ledger's `48790fb73` arm).
- **N3. Bug 235 (`processSingleton`, `93c5af6ad`).** v4's defect: Next.js
  evaluates a module once per bundle, so a module-level `const cache = new
  Map()` is one cache PER COPY (`lib/utils/process-singleton.ts:1-17`); the
  fix keeps eleven pieces of state on `globalThis` under `Symbol.for`
  keys (the vector-store manager, the frozen-archive cache, admission
  locks + ledger, the rate ledger, the maintenance ledger + scheduler, the
  refit debounce / forced set / chain, the interactive counter —
  `git grep 'processSingleton(' 64743ee13 -- lib`). v5 is ONE compiled
  Rust binary: a `static` is one per process by construction — e.g.
  `crates/quilltap-core/src/services/frozen_archive.rs:214`
  (`static CACHE: OnceLock<Mutex<FrozenArchiveCache>>`),
  `services/housekeeping_outcome_cache.rs:35-36`,
  `services/consolidation_triggers.rs:68,74`; and the bug's headline object
  has no v5 counterpart — v5's vector store is not cached at all, it is
  loaded per call as an owned snapshot (`db/vector_store.rs:7-17`), and the
  whole module retires under the 2026-10-13 vector ruling. The refit
  debounce is host-timing (`services/embedding_refit_job.rs:7-10`). There
  is no forked child (the job runner is in-process), so the "dispatcher's
  post-commit reload refreshes only its own copy" path cannot occur.
  Ratify NO-PORT for 235; the NEW state those keys hold (the maintenance
  ledger / scheduler) is the server lanes' to port as ordinary Rust
  statics or host-owned state.
- **N4. Bug 241's transport halves** — S11 above.

**Sizing**

- **Z1.** Three separable lanes fall out of this slice: (i) **memory SPA**
  (Δ1–15; ~5 deleted components + specs, ~7 new ones, the contract region,
  four Playwright specs) — the largest; (ii) **chat-stream bugs 241/242**
  (Δ16–22: one Rust ledger through `orchestrator.rs` + `spine.rs` +
  `types.rs`, the reducer line, the Salon restore + composer
  snapshot/restore, the toast) — medium, touches the busiest SPA file
  (`salon-conversation.ts`) and the busiest Rust file (`orchestrator.rs`),
  so it should not share a wave with another lane editing either;
  (iii) **help / docs re-vendor + NO-PORT record** (Δ23–24, N1–N4, the
  help-guide fixture) — mechanical, late in the unify order (it carries
  `help/memory-maintenance.md` whose `url:` targets lane (i)'s anchor, and
  `help/scenario-browser.md` whose surface is the scenarios lane's).
- **Z2. Tier-3 loud-refusal candidates:** none needed in the SPA; on the
  wire, the retired verbs should answer v4's unknown-action shape (a
  deserialize refusal in v5) rather than a stale handler.

**Ledger corrections (where the hunks disagree with the §3 rows)**

- **C1.** `b045dd986`'s row lists the SPA new files as
  "`memory-maintenance-card.tsx` 383, `-ledger.tsx`,
  `memory-consolidation-particulars.tsx` + `maintenance-dialog.tsx` /
  `-report.tsx`"; the hunks also ADD `components/tools/hooks/useMemorySettingsDocument.ts`
  (84) and `memory-extraction-limits-section.tsx` (97) and modify
  `MemorySearchTabContent.tsx`, `memory-extraction-grain-card.tsx`,
  `memory-list.tsx`, `lib/query/keys.ts`, `lib/help-guide/categories.ts`
  — the row names the last two only implicitly.
- **C2.** The rows' citation of v5's reducer (`chat-stream.reducer.ts:296`
  / `:296-299`) is RIGHT on `a7c6f0d03` (`if (frame.turnStart)` at `:296`,
  `inChain: true` at `:299`, the branch through `:306`). But the ledger's
  framing ("whether v5 shares it is a dogfood measurement") can be partly
  settled by reading: v5's end-of-turn work does not ride `inChain` (Δ16),
  so only the empty-content live-bubble symptom remains to measure.
- **C3.** The `93c5af6ad` row places bug 241's v5 landing at "the Salon
  spine's error frame, `api/salon.rs`"; `api/salon.rs` is the Salon LIST
  surface (no `chat_error` / `ChatSend` in it); the error frame v5 emits is in
  `crates/quilltap-host/src/spine.rs:1617-1649` / `:1720-1742`
  (`Event::chat_error` with `ChatErrorPayload`, `api/types.rs:5623`), and
  the persistence sites are in `quilltap-core/src/services/orchestrator.rs`.
- **C4.** The §1 "Help / docs" paragraph's list for `b045dd986` is right
  (12 edited + 1 A + 2 D); the cumulative 25-path table (§8.3) adds
  `chat-message-actions`, `chat-participants`, `chat-settings`, `chats`,
  `general-scenarios`, `groups`, `post-office`, `project-scenarios`,
  `system-import-export` from the earlier commits — `help/scenario-browser.md`
  is the only NEW page besides `memory-maintenance.md`.
