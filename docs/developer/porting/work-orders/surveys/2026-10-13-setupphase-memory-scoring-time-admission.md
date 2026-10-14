# Survey — memory SCORING, TIME, the CANDIDATE, ADMISSION and the ONE COMBINE across `4a38222e6` → `64743ee13`

Read-only planning survey, **2026-10-13**, for the `64743ee13` memory-refactor
+ scenarios catch-up round. v4 = `~/source/quilltap-server` read ONLY through
`git show 64743ee13:<path>` (the **PIN**, `4.10.0-dev.175`), `git diff
4a38222e6 64743ee13 -- <paths>` / `git show <sha> -- <paths>` (the span) and
`git show 4a38222e6:<path>` (the **baseline**, the shape v5 ported — marked
`(pre)`). **Every v4 line number without `(pre)` is AT THE PIN.** v5 = `main`
at **`a7c6f0d03`**. Nothing here is taken from commit prose (ledger §5.3);
the ledger's §1 (`drift-ledger.md:17-553`) and its §3 rows for `c2795733f`
(`:600`), `e0ac7493a` (`:601`), `8af764514` (`:603`), `124c8aca3` (`:595`),
`93c5af6ad` (`:605`), `4ae0c4ca6` (`:607`) were read first and are built on,
not restated; where the hunks disagree with them it is said explicitly
(§0.1).

**Scope:** `lib/memory/scoring/*`, `lib/memory/time/*`, `lib/memory/candidate.ts`,
`lib/memory/reinforcement-footnotes.ts`, `lib/memory/admission/*` (whole),
the admission thresholds in `lib/memory/settings-defaults.ts`,
`lib/embedding/cosine-scale.ts`, the anchor line's zone in
`lib/memory/episodic.ts`, and every module v4 DELETED that those replace
(`memory-weighting.ts`, `reinforced-importance.ts`, `day-references.ts`,
`episodic-anchors.ts`, `memory-gate.ts`, `memory-merge.ts`, the gate half of
`memory-service.ts`). **Out of scope, named only as seams:** the index store /
vector retirement (Phase B), the recall pipeline (Phase E — it CONSUMES
`scoring/rank.ts` and `time/clock.ts`), the extraction passes (Phase D — they
CALL admission), maintenance (Phase F — it calls `foldInto` / `applyFold` /
`withAdmissionLock` / `protection`), the memories routes (the API lane — the
POST `action` wire), the migrations themselves (the schema keystone — they
import this slice's formula and calendar).

---

## 0. The span for this slice, in one paragraph

`124c8aca3` (Phase 0, bug **200** — not 199; 199 is the trailing-only recall)
widened the gate's top-K search past orphan vectors; that loop survives
almost verbatim in `admission/neighbours.ts`. `c2795733f` (bugs 203–226)
rewrote the reinforced-importance formula to `log2(max(1, count))` (bug 216),
created `memory-calendar.ts` (bug 212 — ONE zone for event time), made merges
write deltas (bug 226) and manual creates MANUAL-only (bug 218, with a
`promoteOnMatch` that Phase C deleted again). `e0ac7493a` (Phase A) moved
scoring into `lib/memory/scoring/*` — DELETING the floored
`calculateEffectiveWeight` and `rankMemoriesByWeight` (orderings move to
**salience**), dropping the per-turn recall floor .5 → .3 and every
`minImportance` filter onto reinforced importance — moved time into
`lib/memory/time/*` (one rule table both directions; the WRITE side stamps the
window START on the memory calendar; read windows close 1 ms early), and
introduced the Zod `MemoryCandidate` (bugs 230–234; 231/234's delta link and
footnote writes are the ones this slice reads). `af48f6cc2` (Phase B) added
`admission/update-memory.ts` (the hand edit). `8af764514` (Phase C) DELETED
`memory-gate.ts` + `memory-merge.ts` for `lib/memory/admission/*`: the same
thresholds, a NEW decide (subject / kind / source eligibility over EVERY close
neighbour, excluded neighbours become links, cold promote-and-reinforce, AUTO
never promoted to MANUAL), a per-character lock + idempotency ledger, and ONE
combine (`foldInto` / `applyFold`) for reinforce, merge and dedup; the POST
`/memories` `action` wire became the lowercase kind. `ac6348c5f` (Phase D)
added the rate ledger, the dry-run book and `backfill`; `93c5af6ad` added bug
237's mixed-dimension fallback; `584d28164` (Phase E) added
`scoring/rank.ts`; `4ae0c4ca6` added `ADMISSION_THRESHOLDS_TFIDF` +
`isLexicalEmbeddingProvider` + `lexicalText`; `104e986c7` (Phase F) deleted
`admission/after-insert.ts` (the watermark now `maintenance/triggers`) and
exported `withAdmissionLock`. `b045dd986` / `48790fb73` / `9ec4fa2e9` /
`64743ee13` do not touch this slice's modules.

### 0.1 Where the hunks disagree with the ledger (say-so list)

1. **Not every `minImportance` filter reads `reinforcedImportance ??
   importance`** (ledger `:601`). The two SQL filters in the repository
   (`findByCharacterIdPaginated`'s `minImportance`, `findByImportance`)
   compare the **`reinforcedImportance` COLUMN** (`e0ac7493a`
   `memories.repository.ts` hunks `@@ -206` / `@@ -288`): a row whose column
   is NULL is EXCLUDED there, where the in-memory `filterImportance` would
   fall back to `importance`. Port both shapes as v4 has them (§3 S8).
2. **The TF-IDF thresholds ride on the admission REQUEST, not the
   candidate** (ledger `:607` "carried on the candidate's
   `embeddingProvider`"): `AdmitPreparedRequest.embeddingProvider`
   (`admit.ts:99`) is the provider string `generateEmbeddingForUser` returned
   for the candidate's own vector; `MemoryCandidate` has no such field.
3. **The "Phase-0 gate orphan widen" is bug 200**, not 199 (the brief's
   wording). Bug 199 is `buildContext`'s trailing-only list — not this slice.
4. **The watermark after an insert is `maybeEnqueueMaintenance`**, not "the
   consolidation watermark at admission" (ledger `:603`, true at `8af764514`
   but rewritten by `104e986c7`, `admit.ts:406-412` at the pin).
5. **The idempotency key exists only for admissions made inside a JOB**
   (`admit.ts:227-232` — minted only when `isJobChild()`; the API admission
   carries none). The ledger's "a failed job no longer rolls admissions back"
   is the motivation; the key's scope matters for v5 (§9 O3).

---

## 1. v4 AT THE PIN — every module in the slice

### 1.1 `lib/memory/scoring/` (7 files, 518 lines)

**`index.ts` (62)** — barrel: `reinforcedImportance`, `calculateReinforcedImportance`,
`REINFORCED_IMPORTANCE_STEP`; `salience`, `sortBySalience`, `referenceTimeMs`,
`SALIENCE_HALF_LIFE_DAYS`, `SalienceBreakdown`, `SalienceInput`; `protection`,
`calculateProtectionScore`, `PROTECTION_THRESHOLD`, `DEFAULT_PROTECTION_CONFIG`,
`ProtectionBreakdown`, `ProtectionInput`, `ProtectionScoreConfig`,
`ProtectionScoreResult`; `relevance`, `computeRankingBlend`,
`RANKING_RELEVANCE_WEIGHT`, `RANKING_PRIORITY_WEIGHT`,
`DEFAULT_MIN_COSINE_NEURAL`, `DEFAULT_MIN_COSINE_TFIDF`,
`defaultMinCosineForProvider`; `RECALL_MIN_IMPORTANCE`, `TOOL_MIN_IMPORTANCE`,
`IMPORTANCE_HIGH`, `IMPORTANCE_LOW`, `filterImportance`, `importanceBand`,
`ImportanceBand`; `rankForRecall`, `rankByBlend`, `reserveBackgroundSlots`,
`byBlendedAfter`, `RankInput`. Header (`:9-10`): "Every worth-ordering in the
memory system goes through `salience` or `protection`; nothing else
re-derives a worth score."

**`reinforced-importance.ts` (29)**
```ts
export const REINFORCED_IMPORTANCE_STEP = 0.05                    // :12
export function reinforcedImportance(importance: number, count: number): number {
  const n = Number.isFinite(count) ? Math.max(1, count) : 1       // :24
  return Math.min(1.0, importance + Math.log2(n) * REINFORCED_IMPORTANCE_STEP)
}
export const calculateReinforcedImportance = reinforcedImportance  // :29
```
(pre `lib/memory/reinforced-importance.ts:12-14`: `Math.min(1.0, base +
Math.log2(reinforcementCount + 1) * 0.05)` — no `max(1,…)`, no finiteness
guard.) Count 1 now equals `importance` (a fresh row's stored value);
count 0 / negative / NaN read as 1. Consumers at the pin:
`memories.repository.ts` (`incrementReinforcement`), `admission/apply.ts`
(`insertRow` at count 1, `raiseImportance`), `admission/update-memory.ts`,
`consolidation-plan.ts:313`, `maintenance/fold-near-duplicates.ts`,
`ai-import.service.ts:583`, the job child's proxy, and migration
`recompute-reinforced-importance-v1` (`migrations/scripts/…:28,84-87`:
`calculateReinforcedImportance(row.importance ?? 0.5, row.reinforcementCount ?? 1)`,
written only when `|stored − value| > 1e-9` or stored is NULL).

**`salience.ts` (66)**
- `SALIENCE_HALF_LIFE_DAYS = 30` (`:18`), `DAY_MS = 86_400_000`.
- `SalienceInput = Pick<Memory,'importance'|'createdAt'> & Partial<Pick<Memory,'reinforcedImportance'|'lastReinforcedAt'>>`.
- `SalienceBreakdown { salience, base, decay, daysSince }`.
- `referenceTimeMs(memory)` (`:44-48`) — `max(Date(createdAt), lastReinforcedAt ? Date(lastReinforcedAt) : 0)` (moved byte-identical from pre `memory-weighting.ts:49-55`).
- `salience(memory, clock = {}) → SalienceBreakdown` (`:51-57`): `base = reinforcedImportance ?? importance`; `daysSince = max(0, (now − ref)/DAY_MS)`; `decay = 0.5^(daysSince/30)`; `salience = base × decay`. **== the old `rawWeight`** (pre `memory-weighting.ts:89`); there is NO floor.
- `sortBySalience(memories, clock = {})` (`:60-66`): one `nowMs` for the whole sort; key = salience desc, ties by original index (stable); returns a new array.

**`protection.ts` (191)**
- `PROTECTION_THRESHOLD = 0.5` (`:18`).
- `ProtectionScoreConfig` / `DEFAULT_PROTECTION_CONFIG` (`:76-87`) — IDENTICAL values to pre (`contentHalfLifeDays 30, contentFloor 0.10, maxReinforcementBonus 0.25, reinforcementCoeff 0.08, maxGraphDegreeBonus 0.10, graphDegreeCoeff 0.025, recentAccessBonus 0.10, recentAccessWindowDays 90, maxContentContribution 0.40, episodicBonus 0.10`).
- `calculateProtectionScore(memory, config, now)` (`:112-166`) — BYTE-IDENTICAL body to pre `memory-weighting.ts:257-311` (input type narrowed to `ProtectionInput`). ⚠ Its reinforcement bonus STILL reads `Math.log2(reinforcementCount + 1)` (`:131`) — the bug-216 change is to reinforced IMPORTANCE only; do not "fix" this one.
- NEW `ProtectionBreakdown extends ProtectionScoreResult { protected; protectedBySource }`.
- NEW `protection(memory, clock = {}, config = DEFAULT) → ProtectionBreakdown` (`:181-191`): `source === 'MANUAL' || 'CONSOLIDATED'` → `{ ...result, score: 1, protected: true, protectedBySource: true }`; else `protected = score >= 0.5`. Consumers: `maintenance/demote.ts`, `maintenance/fold-near-duplicates.ts`, `recall/deliver.ts`, `retire.ts`.

**`relevance.ts` (40)**
- `relevance(cosine) = cosine` (`:16`).
- `RANKING_RELEVANCE_WEIGHT = 0.75`, `RANKING_PRIORITY_WEIGHT = 0.25` (`:20-21`, unchanged).
- `computeRankingBlend(cosine, salienceValue) = 0.75 × relevance(cosine) + 0.25 × salience` (`:24-26`) — same arithmetic as pre (second arg renamed `rawWeight` → `salienceValue`).
- `DEFAULT_MIN_COSINE_NEURAL = 0.30`, `DEFAULT_MIN_COSINE_TFIDF = 0.10` (`:34-35`).
- `defaultMinCosineForProvider(p) = isLexicalEmbeddingProvider(p) ? 0.10 : 0.30` (`:38-40`; pre compared `provider === 'BUILTIN'` — same set today).

**`importance.ts` (36)**
```ts
export const RECALL_MIN_IMPORTANCE = 0.3   // every prompt path: head, proactive, inter-character, Carina, voice-rewrite, replay
export const TOOL_MIN_IMPORTANCE = 0       // the search tool and the Memories API
export const IMPORTANCE_HIGH = 0.7
export const IMPORTANCE_LOW = 0.3
export type ImportanceBand = 'high' | 'medium' | 'low'
export function filterImportance(m) { return m.reinforcedImportance ?? m.importance }
export function importanceBand(v) { if (v >= 0.7) return 'high'; if (v < 0.3) return 'low'; return 'medium' }
```
(The medium/low boundary was **0.4** at every pre site — the card, the
editor, `search-scriptorium-handler.ts`.)

**`rank.ts` (94, NEW in Phase E `584d28164`)** — the recall ranking the recall
lane consumes:
- `RankInput { memory, score, rawScore, salience, usedEmbedding }`.
- `byBlendedAfter(a,b) = (b.recallAdjustment?.blendedAfter ?? 0) − (a… ?? 0)`.
- `rankForRecall(candidates, ctx: RecallContext, bestCosine, extraFired = [])` (`:41-66`): per candidate `blendedBefore = computeRankingBlend(c.score, c.salience)`; `adj = combineRecallMultipliers(c.memory, ctx, { cosine: c.rawScore, bestCosine })` (from `recall/tags.ts`); `adj.exclude` → dropped; `recallAdjustment { multiplier, fired: [...adj.fired, ...extraFired], blendedBefore, blendedAfter: blendedBefore × multiplier }`; sort `byBlendedAfter` (Array.sort — stable).
- `rankByBlend(candidates)` (`:69-73`): `{memory, score, usedEmbedding, salience}` sorted by blend desc (stable).
- `reserveBackgroundSlots(ranked, background, headSize, fraction)` (`:82-94`): `reserve = floor(headSize × fraction)`; `taken = min(reserve, background.length)`; 0 → `ranked` unchanged; else `head = [...ranked.slice(0, headSize − taken), ...background.slice(0, taken)].sort(byBlendedAfter)`, `tail = [...ranked.slice(headSize − taken), ...background.slice(taken)].sort(byBlendedAfter)`, `[...head, ...tail]`.
- Imports `combineRecallMultipliers` / `RecallContext` from `../recall/tags` and `RecallCandidate` from `../recall/types` — so in v5 this file sits with the recall lane's types (§7).

### 1.2 `lib/memory/time/` (7 source files, 798 lines; 5 test files)

**`index.ts` (47)** — barrel (I/O-free; `calendar-refresh` deliberately NOT
re-exported): `clockFor`, `memoryClockFor`, `resolveEventTime`,
`resolveWhenPhrase`, `resolveReferenceWindow`, `resolveDayReference`,
`normaliseTimeRange`, `MemoryClock`, `TimeWindow`, `DayReferenceResolution`;
`DAY_REFERENCE_TABLE`, `DAY_NUMBER_WORDS`, `MAX_TABLE_DAYS_AGO`,
`matchDayReferenceRule`, `DayReferenceRule`, `RuleWindow`;
`getMemoryTimeZone`, `isValidTimeZone`, `zonedParts`, `zonedDateStamp`,
`zonedDayBoundsIso`, `startOfZonedDayMs`, `startOfZonedWeekMs`,
`zonedTimeOnDayMs`, `zonedTimeToUtcMs`; `formatMemoryAge`.

**`calendar.ts` (209; = c2795733f's `memory-calendar.ts`, renamed 100%)**
- `INSTANCE_TIMEZONE_TTL_MS = 5 × 60 × 1000` (`:29`); module cache `cachedInstanceTimeZone` / `cachedAtMs`.
- `isValidTimeZone(tz)` (`:35-43`): non-empty string AND `new Intl.DateTimeFormat('en-US', { timeZone: tz })` does not throw.
- `systemTimeZone()` (`:45-53`): `Intl.DateTimeFormat().resolvedOptions().timeZone` if valid, else `'UTC'`.
- **`getMemoryTimeZone(instanceTimeZone?)` (`:60-66`) — the resolution chain:** valid explicit arg → valid cached instance zone → valid `process.env.QUILLTAP_TIMEZONE` → `systemTimeZone()`.
- `isMemoryTimeZoneStale(nowMs)` (`cachedAtMs === 0 || now − cachedAtMs >= TTL`), `setCachedInstanceTimeZone(tz, nowMs)` (invalid → null), `resetMemoryTimeZoneCacheForTests()`.
- `ZonedParts { year, month (0-based), day, hour, minute, second, weekday (0=Sun) }`; `zonedParts(ms, tz)` (`:120-132`) via a cached `Intl.DateTimeFormat('en-US', { timeZone, hourCycle: 'h23', year:'numeric', month/day/hour/minute/second:'2-digit', weekday:'short' })`, `hour % 24`, weekday from `{Sun:0…Sat:6}` (`?? 0`).
- `offsetMs(ms, tz)` (`:135-139`): `Date.UTC(parts…) − floor(ms/1000)×1000` (second-truncated).
- **`zonedTimeToUtcMs(year, month, day, hour, tz)` (`:146-158`) — the two-pass algorithm a port must reproduce literally:** `guess = Date.UTC(y, m, d, h, 0, 0, 0)`; `first = offsetMs(guess)`; `candidate = guess − first`; `second = offsetMs(candidate)`; `return second === first ? candidate : guess − second`. Day/month overflow is normalised the `Date.UTC` way (`day: 0` = previous month's last day; callers do `p.day + offsetDays`).
- `startOfZonedDayMs(ms, tz, offsetDays = 0)`, `zonedTimeOnDayMs(ms, tz, offsetDays, hour)`, `startOfZonedWeekMs(ms, tz, weekOffset = 0)` (Monday: `daysSinceMonday = (weekday + 6) % 7`), `zonedDateStamp(ms, tz)` (`YYYY-MM-DD`, zero-padded).
- `zonedDayBoundsIso(date, tz)` (`:191-207`): `/^(\d{4})-(\d{2})-(\d{2})$/` + a real-calendar check via `Date.UTC` round-trip (`2026-02-31` → null); `{ from: ISO(local midnight), to: ISO(next local midnight − 1) }`.
- `export { DAY_MS as MEMORY_CALENDAR_DAY_MS }`.

**`calendar-refresh.ts` (41)** — `refreshMemoryTimeZone(nowMs = Date.now()): Promise<string>`:
not stale → `getMemoryTimeZone()`; else `getRepositories().chatSettings.findAll()`,
`configured = all.find(s => isValidTimeZone(s?.timezone))?.timezone ?? null`,
`setCachedInstanceTimeZone(configured, nowMs)`, then:
- DEBUG `[MemoryCalendar] Instance timezone refreshed` `{ instanceTimeZone: configured, effective: getMemoryTimeZone() }`
- on throw, DEBUG `[MemoryCalendar] Could not read the instance timezone; using fallback` `{ effective, error }` (never throws).

**`rules.ts` (134)**
- `RuleWindow { fromMs, endMs (EXCLUSIVE), pastPointing }`; `DayReferenceRule { name, pattern, window(match, anchorMs, tz) }`.
- `DAY_NUMBER_WORDS` one…fourteen (1–14); `MAX_TABLE_DAYS_AGO = 14`.
- **`DAY_REFERENCE_TABLE` (`:56-114`), in this ORDER** (the read scan's longest-first order):

  | # | name | pattern | window (`day(o)` = local midnight `o` days from anchor's local day) | past |
  |---|---|---|---|---|
  | 1 | day before yesterday | `day before yesterday` | `[day(−2), day(−1))` | ✓ |
  | 2 | last night | `last night` | `[18:00 local day −1, 06:00 local day 0)` | ✓ |
  | 3 | today | `earlier today\|this morning\|this afternoon\|this evening\|tonight\|today` | `[day(0), anchor+1)` | ✓ |
  | 4 | yesterday | `yesterday` | `[day(−1), day(0))` | ✓ |
  | 5 | N days ago | `` `(\d{1,2}\|one\|…\|fourteen) days? ago` `` | n∉[1,14] → null; `[day(−n), day(−n+1))` | ✓ |
  | 6 | this week | `this week` | `[Mon(0), anchor+1)` | ✓ |
  | 7 | last week | `last week` | `[Mon(−1), Mon(0))` | ✓ |
  | 8 | next week | `next week` | `[Mon(1), Mon(2))` | ✗ |
  | 9 | tomorrow | `tomorrow` | `[day(1), day(2))` | ✗ |
- `matchDayReferenceRule(phrase)` — first rule whose `^(?:pattern)$` /i matches.
- `DAY_REFERENCE_SCAN_PATTERN = /\b(?:(?:p1)|(?:p2)|…)\b/gi` (`:131-134`).

**`clock.ts` (275)**
- `MemoryClock { timeZone, nowMs, timelineMode? }`; `TimeWindow { from, to }` (closed ISO); `DayReferenceResolution { timeRange, pastPointing, matched }`.
- `clockFor(chat, instance = {}, nowMs = Date.now())` → `{ timeZone: getMemoryTimeZone(instance.timeZone ?? null), nowMs, timelineMode: chat?.timelineMode ?? 'realtime' }`; `memoryClockFor(chat, nowMs)` = `clockFor(chat, { timeZone: await refreshMemoryTimeZone(nowMs) }, nowMs)`.
- `toWindow(w) = { from: ISO(fromMs), to: ISO(endMs − 1) }` — **every window closes 1 ms before the next boundary.**
- Write tables: `MONTH_NAMES` (january…december → 0–11), `UNIT_DAYS` (`day(s) 1, week(s) 7, month(s) 30, year(s) 365`), `WORD_NUMBERS` (`a/an 1`, one…**fourteen**, `couple 2`, `few 3`), `WEEKDAYS`.
- **`resolveEventTime(phrase, anchorMs, clock: {timeZone}) → { occurredAt: string|null, window: TimeWindow|null }` (`:126-203`)**, in order:
  1. `raw = phrase?.trim()`; empty or non-finite anchor → none. `lower = raw.toLowerCase().replace(/\s+/g, ' ')`.
  2. **Table first**: `matchDayReferenceRule(lower.replace(/^the\s+/, ''))`; past window → `{ occurredAt: ISO(w.fromMs), window }` (**the window START**); a future window → none.
  3. ISO `/^(\d{4})-(\d{2})-(\d{2})(t[\d:.]+(z|[+-]\d{2}:?\d{2})?)?$/` on `lower`: `Date.parse(raw)` non-finite → none; with a time part → `ISO(parsed)`, window null; date-only → `zonedDayBoundsIso(lower.slice(0,10), tz)` → `{ occurredAt: bounds.from, window: bounds }`, invalid calendar date → none.
  4. Named dates (`monthFirst` `/^(?:on\s+)?([a-z]+)\s+(\d{1,2})(?:st|nd|rd|th)?(?:,?\s+(\d{4}))?$/`, `dayFirst` `/^(?:on\s+)?(\d{1,2})(?:st|nd|rd|th)?\s+(?:of\s+)?([a-z]+)(?:,?\s+(\d{4}))?$/`; day 1–31): year = given ?? anchor's ZONED year; `zonedTimeToUtcMs(y, m, d, 0, tz)`; yearless and after the anchor → year − 1; window null.
  5. `/^(earlier|just now|now|this night)$/` → the anchor instant.
  6. `agoMatch` `/^(?:about\s+|around\s+|some\s+)?(?:a\s+(?=couple|few))?([a-z]+|\d+)\s+(?:of\s+)?(days?|weeks?|months?|years?)\s+(?:ago|back|earlier|before)$/` → `n` (digits or `WORD_NUMBERS`), `0 < n < 10000` → `daysBack(n × UNIT_DAYS)` (= `ISO(startOfZonedDayMs(anchor, tz, −days))`).
  7. `last month` → `daysBack(30)`; `last year` → 365; `the other day` → 2.
  8. `last <weekday>` → `daysBack(((anchorZonedWeekday − target + 7) % 7) || 7)`.
  9. `last (spring|summer|fall|autumn|winter)` → `daysBack(182)`.
  10. else none.
- `resolveWhenPhrase(phrase, anchorIso, timeZone = getMemoryTimeZone())` = `resolveEventTime(phrase, Date.parse(anchorIso), { timeZone }).occurredAt`.
- **`resolveReferenceWindow(text, clock: {timeZone, nowMs})` (`:229-242`)**: empty text / non-finite now → null; scans a fresh copy of the scan pattern; each match's phrase is lower-cased + whitespace-collapsed, re-matched with `matchDayReferenceRule`, `window(...)` at `clock.nowMs`; a non-null window replaces `latest` (an unresolvable "97 days ago" never displaces an earlier one); zero-length-match guard.
- `resolveDayReference(text, now: Date, timeZone = getMemoryTimeZone())` — the `(text, Date, zone)` form.
- **`normaliseTimeRange(range, clock: {timeZone})` (`:259-274`)**: both trimmed bounds required and both must match `/^\d{4}-\d{2}-\d{2}/`; a length-10 bound widens through `zonedDayBoundsIso` (`from` → `.from`, `to` → `.to`, invalid date → null → whole result null); longer bounds pass through; `Date.parse` both finite and `from ≤ to`, else null.

**`anchors.ts` (68; = pre `episodic-anchors.ts` + a zone)** — `resolveEpisodicAnchors<F>(input: { when, referenceIso, fallbackIso: F, timelineMode, narrativeTime?, timeZone? })`:
`occurredAt = fallbackIso`; if `when && referenceIso`, `resolved = timeZone ? resolveWhenPhrase(when, referenceIso, timeZone) : resolveWhenPhrase(when, referenceIso)`, non-null wins; `narrativeTime = timelineMode === 'narrative' ? (explicit ?? (when || null)) : explicit` (`explicit = input.narrativeTime ?? null`). Sole caller: `extraction/run.ts:308` (`timeZone: ctx.clock.timeZone`).

**`age-label.ts` (24)** — `formatMemoryAge(memory, now = new Date())` = `formatRelativeDays(eventReferenceTimeMs(memory.occurredAt, referenceTimeMs(memory)), now)` — byte-identical behaviour to pre `formatRelativeAge` (`memory-weighting.ts:164-167`). Consumers: `recall/render.ts`, `recall/recap.ts`, `lib/format-time.ts` (doc ref).

**`lib/memory/episodic.ts` at the pin (94; was 239 pre)** — `resolveWhenPhrase`,
`isoDateFromMs`, `MONTH_NAMES`, `UNIT_MS`, `WORD_NUMBERS`, `buildMemoryEmbeddingText`
REMOVED (the embedding text moved to `index-store/embedding-text.ts`, Phase B).
`buildMemoryAnchorLine(view, timeZone?)` (`:49-78`) now renders the date as
`zonedDateStamp(Date.parse(occurredAt), timeZone ?? getMemoryTimeZone())` when
parseable, else `occurredAt.slice(0,10)`; kept only if `/^\d{4}-\d{2}-\d{2}$/`
(pre: always `slice(0,10)` — the UTC date). Labels `when` / `story time` /
`place`, ≤ 4 entities, ` · ` join. `eventReferenceTimeMs` unchanged.

### 1.3 `lib/memory/candidate.ts` (280, NEW in Phase A)

- `TargetingTagsSchema = z.object({ temporal: z.enum(['past','moment','present','future']), scope: z.enum(['narrow','wide']), context: z.enum(['philosophy','relationships','history','banter','mannerisms','trivia','information']) })`; `DEFAULT_TARGETING` = `present` / `wide` / `information` (from `recall/tags`).
- `normaliseTargeting(raw, fallback = {})` — each axis: `raw.trim().toLowerCase()` in the closed set → it, else `fallback.x`, else default.
- `targetingKeywords(t) = [t.temporal, \`scope: ${t.scope}\`, t.context]`.
- `splitTargetingKeywords(keywords)` → `{ free, targeting }` (`scope:` prefix → scope; a temporal word → temporal; a context word → context; else trimmed non-empty → free; last of each axis wins; non-strings skipped). Consumer: `consolidation-plan.ts:206`.
- **`coerceImportance(raw) = typeof raw === 'number' && isFinite(raw) ? min(1, max(0, raw)) : 0.5`** — "the one importance default"; a model's 0 stays 0 (pre writers did `importance || 0.5`).
- `MEMORY_ENTITY_CAP = 12` (Phase C).
- `CandidatePassEnum = ['self-turn','other-turn','other-fold','episode-fold','carina','manual','consolidation','import']` (`consolidation` and `import` are enumerated but no producer builds them at the pin).
- **`MemoryCandidateSchema` (`:150-179`)**:
  ```ts
  content: z.string().min(1), summary: z.string().min(1),
  keywords: z.array(z.string()).default([]),
  targeting: TargetingTagsSchema.nullable(),
  importance: z.number().min(0).max(1),
  kind: MemoryKindEnum, when: z.string().nullable(),
  entities: z.array(z.string()).max(MEMORY_ENTITY_CAP).default([]),
  tags: z.array(z.string()).default([]),
  subject: z.object({ holderId: Id, aboutId: Id.nullable() }),          // Id = z.string().min(1)
  provenance: z.object({ source: MemorySourceEnum, witnessedContext: WitnessedContextEnum.nullable(),
    chatId: Id.nullable(), projectId: Id.nullable(), sourceMessageId: Id.nullable(),
    anchorTimeMs: z.number().finite(), pass: CandidatePassEnum }),
  ```
- `MemoryDraftLike` — the parser's output view (the OLD type is now named `MemoryDraft`, in `extraction/parse.ts`).
- `anchorTimeMsFrom(isoCandidates, fallbackMs = Date.now())` — first `Date.parse`-finite entry, else fallback.
- **`candidateFromDraft(draft, subject, provenance) → MemoryCandidate | null`** (`:219-241`): `content = draft.content?.trim() ? draft.content : draft.summary?.trim() ? draft.summary : ''` (untrimmed value kept); `summary = draft.summary?.trim() ? draft.summary : content`; `safeParse({ content, summary, keywords ?? [], targeting: normaliseTargeting(draft.targeting), importance: coerceImportance(draft.importance), kind: draft.kind === 'episodic' ? 'episodic' : 'semantic', when: draft.when ?? null, entities: (draft.entities ?? []).slice(0, 12), tags: [], subject, provenance })`; failure → null (the caller skips the one draft).
- `ResolvedCandidateTime { occurredAt, narrativeTime, sourceMessageTimestamp? }`.
- **`toAdmissionRow(candidate, resolved) → AdmissionRow`** (`:255-280`): `keywords = targeting ? [...keywords, ...targetingKeywords(targeting)] : [...keywords]` (tags AFTER the free words); `sourceMessageId: … ?? undefined`; every other field copied.

### 1.4 `lib/memory/reinforcement-footnotes.ts` (60, NEW in Phase A; Phase C added `cap`)

- `MAX_REINFORCEMENT_FOOTNOTES = 8`; `FOOTNOTE_PREFIX = '[+] '`.
- `countReinforcementFootnotes(content)` — lines of `content.split('\n')` starting with `'[+] '` (unchanged from pre `memory-gate.ts:426-432`).
- **`appendCappedFootnotes(content, details, cap = 8)`** (`:41-60`): `present` = the set of existing `[+] ` LINES; `room = max(0, cap − count)`; walk details: stop at `room`, **skip a detail whose `[+] detail` line is already present** (bug 231/234 — NEW vs pre, which took `details.slice(0, room)`), dedupe within the call; nothing appended → content unchanged.
- Used by the repository's atomic `appendReinforcementFootnotes` (applied INSIDE the write transaction against the live row) and by `foldInto` (planning against the snapshot).

### 1.5 Admission thresholds (`lib/memory/settings-defaults.ts`, 103) + `lib/embedding/cosine-scale.ts` (23)

```ts
export interface AdmissionThresholds { readonly nearDuplicate; readonly reinforce; readonly related; readonly dateGuardDays; readonly footnoteCap }
export const ADMISSION_THRESHOLDS = Object.freeze({ nearDuplicate: 0.90, reinforce: 0.85, related: 0.70, dateGuardDays: 7, footnoteCap: MAX_REINFORCEMENT_FOOTNOTES })   // :45-51
export const ADMISSION_THRESHOLDS_TFIDF = Object.freeze({ ...ADMISSION_THRESHOLDS, nearDuplicate: 0.80, reinforce: 0.40, related: 0.20 })  // :61-66
export function admissionThresholdsForProvider(p) { return isLexicalEmbeddingProvider(p) ? ADMISSION_THRESHOLDS_TFIDF : ADMISSION_THRESHOLDS }  // :72-74
export function resolveClusterThreshold(settings, provider, override?)  // override ?? (lexical ? tfidfClusterThreshold : clusterThreshold) — consolidation's
MEMORY_SETTING_DEFAULTS.admission / .admissionTfidf                    // the settings facade exposes both
```
`cosine-scale.ts`: `LEXICAL_EMBEDDING_PROVIDERS = new Set(['BUILTIN'])`;
`isLexicalEmbeddingProvider(p) = !!p && set.has(p)` — the ONE predicate for the
recall floor, the R1 gate, admission and consolidation.

### 1.6 `lib/memory/admission/` (15 files, 2,251 lines)

#### `types.ts` (135) — the wire-level shapes

- `AdmissionRow` — `{ characterId (holder); content; summary; keywords?; tags?; importance?; aboutCharacterId?: string|null; chatId?; projectId?; source?: 'AUTO'|'MANUAL'|'CONSOLIDATED'; sourceMessageId?; sourceMessageTimestamp?: string; witnessedContext?: 'user_present'|'autonomous_room'|'manual'|null; occurredAt?; narrativeTime?; entities?; kind?: 'semantic'|'episodic' }`.
- **`AdmissionAction`** (`:65-71`):
  ```ts
  | { kind: 'insert' }
  | { kind: 'insert-linked'; linkTo: string[]; reason: 'band' | 'date-guard' | 'mismatch' }
  | { kind: 'reinforce'; into: string }
  | { kind: 'absorb'; into: string; redirectedFrom?: string }
  | { kind: 'promote-and-reinforce'; into: string; withFootnotes: boolean }
  | { kind: 'reject'; reason: 'embedding-failed' | 'empty' }
  ```
  `AdmissionActionKind = AdmissionAction['kind']`.
- `AdmissionNeighbour { memory; cosine; via?: string; danglingDigest?: boolean }`.
- `NeighbourExclusion = 'subject' | 'kind' | 'manual-over-auto' | 'date-guard'`.
- `NeighbourAssessment { id; cosine; via?; verdict: 'below'|'band'|'excluded'|'eligible'; excludedBy? }`.
- `AdmissionDecision { action; assessments }`.
- `AdmissionTrace { thresholds; assessments; embeddingAttempts; orphansIgnored; searchedK; notes: string[] }`.
- `AdmissionOutcome { action; memory: Memory|null (never carries its embedding); novelDetails: string[]; linkedIds: string[]; trace }`.

#### `decide.ts` (147) — `decideAdmission` (PURE)

- `AdmissionCandidateView { holderId; aboutId; kind; source; occurredAt }`.
- `occasionsAreDistinct(a, b, dateGuardDays)` (`:52-62`): both truthy, both `Date.parse`-finite, `|a − b| > dateGuardDays × 86_400_000` (pre took no third argument; `DATE_GUARD_DAYS = 7` was a constant).
- `subjectOf(holderId, aboutId) = aboutId || holderId` (a self memory reads as its holder; `''` is falsy too).
- `exclusionFor(candidate, n, t)` (`:69-82`), FIRST hit wins, in order: subject differs → `'subject'`; `(m.kind ?? 'semantic') !== (candidate.kind ?? 'semantic')` → `'kind'`; `candidate.source === 'MANUAL' && (m.source ?? 'MANUAL') === 'AUTO'` → `'manual-over-auto'`; `occasionsAreDistinct(candidate.occurredAt, m.occurredAt, t.dateGuardDays)` → `'date-guard'`.
- **`decideAdmission({ candidate, neighbours, thresholds })` (`:88-147`)**:
  1. `[...neighbours].sort((a,b) => b.cosine − a.cosine)` (stable).
  2. For each: `cosine < related` → `below`; `< reinforce` → `band` + push to `linkTo` (deduped); else `exclusionFor` → `excluded` + `exclusions.add` + push to `linkTo`; else `eligible` and the FIRST eligible becomes `target` (later eligibles are assessed but not taken).
  3. With a target `m`: `near = cosine >= nearDuplicate`; `coldUnsuperseded = m.tier === 'cold' && (!m.supersededById || danglingDigest === true)` → `promote-and-reinforce { into, withFootnotes: !near }`; else `target.via` → `absorb { into, redirectedFrom: via }`; else `near` → `absorb { into }`; else `reinforce { into }`.
  4. No target, `linkTo` non-empty → `insert-linked { linkTo, reason }` with `reason = exclusions.has('date-guard') ? 'date-guard' : exclusions.size > 0 ? 'mismatch' : 'band'`.
  5. Else `insert`.

#### `neighbours.ts` (257) — `gatherNeighbours` (reads only; parent)

- `ADMISSION_TOP_K = 5`; `ADMISSION_MAX_SEARCH_K = 20`; `MAX_DIGEST_HOPS = 3`; `FALLBACK_MAX_WORDS = 6`; `FALLBACK_MIN_WORD_LENGTH = 4`. Child logger `module: 'memory:admission:neighbours'`.
- `GatheredNeighbours { neighbours; orphanIds; searchedK; reembedded? }`; `NeighbourProbe { text; userId }` (admission passes `text: \`${row.summary}\n${row.content}\``).
- `unscorableRowCount(characterId, queryDims)` (`:72-90`): `memoryIndex.shape` → `{size, matching, dominantDimensions}`; empty or all-matching → 0; else `memoryIndex.reload(characterId)` and re-measure; `unscorable > 0` → WARN `Admission index cannot compare some rows with the candidate; matching them by text` `{ characterId, queryDimensions, dominantDimensions, unscorable, size }`; else DEBUG `Admission index was stale; reloaded it from the rows` `{ characterId, queryDimensions }`.
- `fallbackWords(text)`: lower-case, split on `/[^\p{L}\p{N}]+/u`, length ≥ 4, a Set (first-seen order), then `.sort((a,b) => b.length − a.length)` (stable), first 6.
- `unindexedNeighbourHits(characterId, embedding, probe, skip)` (`:109-163`): `comparable` = index entries of the query's length; for each fallback word `repos.memories.searchByContent(characterId, word)` (rows whose content/summary LIKE the word), skip comparable / skipped ids, count word overlaps; top 5 by overlap (stable); each re-embedded with `embeddingTextFor(memory, tz)` + `lexicalText` at `priority: 'background'`; length-mismatch skipped; failure → DEBUG `Could not re-embed an unindexed neighbour; skipping it` `{ characterId, memoryId, error }`; then DEBUG `Scored unindexed admission neighbours by re-embedding` `{ characterId, candidates: overlap.size, scored }`.
- `resolveDigest(memory)` (`:170-188`): not cold or no `supersededById` → as-is; follow up to 3 hops via `findByIds([supersededById])`; a missing digest → DEBUG `Superseding digest missing; the cold row stands for itself` `{ memoryId, supersededById }` and `{ memory: current, danglingDigest: true }` (+ `via: memory.id` when ≥ 1 hop was taken); else `{ memory: digest, via: memory.id }` (`via` is ALWAYS the ORIGINAL member's id).
- **`gatherNeighbours(characterId, embedding, { exclude?, probe? })` (`:194-257`)**:
  1. `unscorable = unscorableRowCount(...)`.
  2. `search(limit)` = `memoryIndex.search(characterId, embedding, { limit, hotOnly: false })` (**BOTH tiers**), `exhausted = found.length < limit` (measured BEFORE the `exclude` filter), hits minus `exclude`.
  3. Widen loop (bug 200): `k = 5`; repeat `findByIds(hit ids)`, `live = hits with a row`; break when `live >= 5 || exhausted || k >= 20`; else `k = min(20, k + (5 − live))`.
  4. `orphanIds` = hits without a row → WARN `Vector index holds entries with no memory row; ignoring them` `{ characterId, orphanIds, searchedK: k }` (pre-Phase-C text carried the `[MemoryGate] ` prefix and a `[Gate] Ignored N orphan vector(s)` debugInfo line — both gone).
  5. `scored` = live hits; if `unscorable > 0 && probe` → merge `unindexedNeighbourHits` (skip = scored ∪ exclude) and re-sort by score desc.
  6. For `scored.slice(0, 5)`: `resolveDigest`; collapse by resolved id — a later hit replaces an earlier only when its score is HIGHER (`prior.cosine >= hit.score` → skip); Map insertion order kept; then `.sort(cosine desc)`.
  7. DEBUG `Gathered admission neighbours` `{ characterId, neighbours, redirected, orphans, searchedK: k, reembedded }`.

#### `admit.ts` (602) — the entry point and the parent half

- Constants: `EMBEDDING_RETRY_DELAY_MS = 500`; `ADMISSION_LEDGER_TTL_MS = 24h`; `ADMISSION_LEDGER_MAX = 5000`. Child logger `module: 'memory:admission'`.
- `AdmitOptions { userId; dryRun?; backfill?; dryRunBook? }`; `AdmitPreparedRequest { row; embedding; embeddingProvider?; userId; embeddingAttempts; notes; idempotencyKey?; dryRun?; backfill? }`.
- `rejected(reason, notes, attempts)` — trace carries **`ADMISSION_THRESHOLDS` (the NEURAL set) always**, `searchedK 0`.
- `embedRow(row, tz, opts, notes)` (`:138-166`): text `embeddingTextFor(row, tz)`, call `{ priority: 'background', lexicalText: lexicalEmbeddingTextFor(row) }`; attempt 1 fails → note `embedding attempt 1 failed (<msg>)`, sleep 500 ms, attempt 2; fails → note `embedding failed after retry (<msg>)` + WARN `Memory not admitted: embedding failed after retry` `{ characterId, userId, error }` → `{ embedding: null, provider: null, attempts: 2 }`. `<msg>` = `EmbeddingError.message` else `String(e)`.
- **`admissionContentKey(row)`** (`:169-182`) = `sha1(JSON.stringify([characterId, aboutCharacterId ?? null, kind ?? 'semantic', source ?? 'MANUAL', summary, content, sourceMessageId ?? null, occurredAt ?? null]))` hex.
- `admitCandidate(candidate, resolved, opts)` = `admitRow(toAdmissionRow(candidate, resolved), opts)`.
- **`admitRow(row, opts)`** (`:197-241`), inside `trackActivity('memory', …)`:
  1. `!row.content?.trim() || !row.summary?.trim()` → DEBUG `Memory not admitted: empty text` `{ characterId }` → `reject { empty }` (attempts 0).
  2. `tz = await refreshMemoryTimeZone()`; `applyNamePresenceCheck(row, notes)`; `applyEpisodicFallbackAnchors(prepared, tz)`.
  3. `embedRow` → null → `reject { embedding-failed }`.
  4. Dispatch: dry run in the parent → `decideOnly(request, opts.dryRunBook)`; in the job child → mint `idempotencyKey = nextAdmissionKey(admissionContentKey(prepared))` (not for a dry run) and `callHost('admitMemory', request)`; else `admitPrepared(request)`.
- Parent guards: `characterLocks` (`processSingleton('memory.admission.characterLocks')`) — a promise chain per character (`withCharacterLock`; exported as **`withAdmissionLock`** for maintenance's near-duplicate fold); `admissionLedger` (`processSingleton('memory.admission.ledger')`) — `recallAdmission(key, now)` (expired → delete, null), `recordAdmission(key, outcome, now)` (insert, then evict from the oldest while `size > 5000` OR the entry is older than the TTL); `resetAdmissionStateForTests()`. `patchChildIndex` — the job child's cache patch (Node plumbing).
- **`admitPrepared(request)`** (`:324-428`):
  - In the job child → `throw new Error('admitPrepared runs in the parent; the job child admits through host-RPC (admitMemory)')`.
  - `dryRun` → `decideOnly(request)` (no lock).
  - Under `withCharacterLock(characterId)`: a keyed repeat → DEBUG `Admission already made by an earlier attempt of this job; nothing written` `{ characterId, action, memoryId }`, return the stored outcome with note `already admitted by an earlier attempt of this job`.
  - `thresholds = admissionThresholdsForProvider(request.embeddingProvider)`.
  - Up to two attempts: `gatherNeighbours(..., { probe })` → `decideAdmission({ candidate: { holderId: characterId, aboutId: row.aboutCharacterId, kind: row.kind, source: row.source, occurredAt: row.occurredAt }, neighbours, thresholds })` → trace → `applyAction(...)`. `'vanished'` on attempt 0 → note `target retired before the write; deciding again` + DEBUG `Admission target vanished; deciding again` `{ characterId, action }`; on attempt 1 → note `target retired again; inserted instead` + WARN `Admission target vanished twice; inserting the candidate` `{ characterId }` + force `insert`.
  - After: `outcome.memory` → `publishRealtime('memories')`; inserted ∧ `source === 'AUTO'` ∧ `!backfill` → `recordAdmissionInsert(characterId)`; child cache patch; **inserted → fire-and-forget `maintenance/triggers.maybeEnqueueMaintenance([characterId])`** (failure DEBUG `Maintenance watermark check skipped after admission` `{ characterId, error }`); keyed → record.
  - DEBUG **`Memory admitted`** `{ characterId, action, memoryId, neighbours, bestCosine, novelDetails, linked, keyed, thresholdScale: thresholds === ADMISSION_THRESHOLDS ? 'neural' : 'tfidf' }`.
- **`decideOnly(request, book?)`** (`:435-484`): gather with `exclude: book.retiring` when a book; book neighbours appended (`bookNeighbours`), re-sorted by cosine desc and sliced to 5; decide; the target row (for absorb / reinforce / promote) without its embedding; an insert records into the book; DEBUG `Admission decided (dry run; nothing written)` `{ characterId, action, targetId, neighbours }`; `linkedIds = insert-linked ? [...linkTo] : []`; note `dry run: decided, nothing written`.
- **`applyAction(action, row, embedding, neighbours, userId, notes)`** (`:508-602`):
  - `reject` → nothing.
  - `insert` → `insertRow(row, embedding)`; index entry hot.
  - `insert-linked` → `insertRow` then `linkBothWays(characterId, id, linkTo)`; returns the POST-LINK row (`relatedMemoryIds: linkedIds`, bug 26).
  - `absorb` / `reinforce` / `promote-and-reinforce` → target from the gathered map (absent → `throw new Error(\`Admission target ${into} is not among the gathered neighbours\`)`); promote first: `memoryIndex.setTier(characterId, [id], 'hot', { supersededById: null })`, `0` moved → `'vanished'`, note `promoted cold row <id> to hot`; `withFootnotes = reinforce || (promote && action.withFootnotes)`:
    - with footnotes → `plan = foldInto(target, [{ content: row.content, occurredAt, narrativeTime, entities, reinforcementCount: 1, lastReinforcedAt: new Date().toISOString() }], FOLD_CAPS)`; `memory = applyFold(target, plan, { userId })`; `novelDetails = memory ? plan.footnotes : []`;
    - count-only → `repos.memories.incrementReinforcement(characterId, id, nowIso, 1)` merged onto the target.
    - `memory` null → DEBUG `Admission target vanished before the write` `{ memoryId }` → `'vanished'`.
    - `row.source === 'MANUAL'` → `raiseImportance(memory, row.importance)`.
    - index entry when promoted or the vector changed.

#### `apply.ts` (226) — the writes (parent only). Child logger `module: 'memory:admission:apply'`.

- `withoutEmbedding(memory)`.
- **`insertRow(row, embedding)`** (`:38-72`): `importance = row.importance ?? 0.5`; `repos.memories.create({ characterId, content, summary, keywords ?? [], tags ?? [], importance, aboutCharacterId: row.aboutCharacterId || null, chatId: row.chatId || null, projectId ?? null, source: row.source || 'MANUAL', sourceMessageId: row.sourceMessageId || null, witnessedContext ?? null, occurredAt ?? null, narrativeTime ?? null, entities ?? [], kind ?? 'semantic', reinforcementCount: 1, relatedMemoryIds: [], reinforcedImportance: calculateReinforcedImportance(importance, 1), embedding }, sourceMessageTimestamp ? { createdAt, updatedAt } : undefined)` — the vector rides in the create (invariant 4); `memoryIndex.indexInserted(characterId, [{ id, embedding, tier: 'hot' }])`; DEBUG `New memory persisted` `{ characterId, memoryId, source }`.
- `reembedRow(memory, userId)` (`:79-99`): `embeddingTextFor(memory, await refreshMemoryTimeZone())` + `lexicalText`, `priority: 'background'`; `memoryIndex.upsert(characterId, [{ id, embedding, tier }])`; DEBUG `Re-embedded memory` `{ memoryId }`; failure WARN `Failed to re-embed memory; it keeps its old vector` `{ memoryId, error }` → null.
- **`applyFold(survivor, plan, { userId? })`** (`:117-206`): in the job child → `throw new Error('applyFold writes in the parent only; the job child cannot read its buffered writes back')`. Order — **each write is a delta against the row at write time; the first one that lands is the commit point**:
  1. `plan.footnotes.length > 0` → `appendReinforcementFootnotes(characterId, id, plan.footnotes)`;
  2. `Object.keys(plan.patch).length > 0` → `updateForCharacter(characterId, id, plan.patch)`;
  3. `plan.addLinks.length > 0` → `addRelatedMemoryIds(characterId, id, plan.addLinks)`;
  4. `plan.reinforcementDelta > 0` → `incrementReinforcement(characterId, id, plan.reinforcedAt ?? nowIso, plan.reinforcementDelta)`;
  a miss BEFORE anything landed → WARN `Fold target is gone; nothing folded` `{ survivorId, characterId, step }` → return null; a miss AFTER → WARN `Fold step missed after the fold landed; the fold stands` `{ … step }` and continue (`step` ∈ `footnotes` / `anchors` / `links` / `reinforcement`);
  5. `plan.reembed && landed` → with `userId` `reembedRow` (current gets the new vector); without → WARN `Survivor changed but no userId to re-embed with` `{ survivorId }`;
  6. DEBUG `Folded into survivor` `{ survivorId, characterId, footnotes, anchors: Object.keys(patch), links, reinforcementDelta, reembedded }`.
- **`raiseImportance(memory, requested)`** (`:214-226`): `requested === undefined || requested <= memory.importance` → unchanged; else `updateForCharacter({ importance: requested, reinforcedImportance: calculateReinforcedImportance(requested, reinforcementCount ?? 1) })`; null → WARN `Failed to raise the importance of a row a manual memory folded into` `{ memoryId }`; DEBUG `Raised importance for a manual write` `{ memoryId, importance }`. (`promoteMatchedMemory` / `promoteOnMatch` — bug 218's AUTO→MANUAL promotion — are GONE: a MANUAL candidate is never folded into an AUTO row; the decider links it.)

#### `fold.ts` (171) — `foldInto`, THE ONE COMBINE (PURE)

- `FoldCaps { footnoteCap; entityCap }`; `FOLD_CAPS = { footnoteCap: 8, entityCap: 12 }`.
- `FoldSource { id?; content; occurredAt?; narrativeTime?; entities?; relatedMemoryIds?; reinforcementCount?; lastReinforcedAt?; createdAt? }`; `FoldSurvivor = Pick<Memory, 'id'|'content'|'occurredAt'|'narrativeTime'|'entities'|'relatedMemoryIds'|'lastReinforcedAt'>`.
- `FoldPlan { survivorId; footnotes; content; addLinks; patch: Partial<{occurredAt, narrativeTime, entities}>; reinforcementDelta; reinforcedAt; atCap; reembed }`.
- `earliest(values)` — first value with the smallest `Date.parse` (falsy / unparseable skipped; strict `<`).
- `latest(values)` — `values.filter(non-empty string).sort().pop() ?? null` — **UTF-16 code-unit lexicographic max, not a date compare** (v5's `memory_merge.rs` already reproduces it).
- **`foldInto(survivor, absorbed, caps)`** (`:106-171`):
  - `atCap = countReinforcementFootnotes(survivor.content) >= footnoteCap`.
  - Footnotes: for each source, `novel = extractNovelDetails(source.content, content)` against the GROWING content; `appendCappedFootnotes(content, novel, footnoteCap)`; collect `appended`.
  - Links: `group = {survivor.id} ∪ source ids`; `addLinks` = sources' `relatedMemoryIds` in order, minus group, minus the survivor's existing links, deduped (**no removals ever**).
  - `occurredAt = earliest([survivor.occurredAt, ...sources])`, patched when it differs.
  - `narrativeTime` filled only when the survivor has none: first source value with a non-blank trim.
  - Entities: survivor's first; append sources' entities skipping non-string / blank / case-insensitive duplicates, stopping at `entityCap`; patched only when the list grew.
  - `reinforcementDelta = Σ max(1, source.reinforcementCount ?? 1)`.
  - `reinforcedAt = latest([survivor.lastReinforcedAt, ...sources.map(s => s.lastReinforcedAt ?? s.createdAt)])`.
  - **`reembed = content !== survivor.content || Object.keys(patch).length > 0`** (an anchor change re-embeds even at the footnote cap — bug 204; pre skipped the re-embed at the cap).

#### `novel-details.ts` (118) — `extractNovelDetails`, moved **byte-identical** from pre `memory-gate.ts:942-1046` (verified by diff; `STOP_WORDS` identical).

#### `prepare.ts` (106) — the safety nets (run wherever admission is called, before embedding)

- `FALLBACK_ANCHOR_MAX_ENTITIES = 6`.
- `applyNamePresenceCheck(row, notes = [])` (`:30-69`): no proposal or self → as-is; `source && source !== 'AUTO'` → as-is; reads both characters; `resolveAboutCharacterId({ holderCharacterId, holderCharacter: {name, aliases} | null, proposedAboutCharacterId, proposedAboutCharacter: {name, aliases, controlledBy} | null, text: \`${summary || ''}\n${content || ''}\` })`; `flipped` → note `subject <id> not named in the text; recorded as a self memory` + DEBUG `[Admission] Name-presence check collapsed the subject to the holder` `{ holderCharacterId, proposedAboutCharacterId }` → `aboutCharacterId: holder`; throw → WARN `[Admission] Name-presence check failed; using proposed aboutCharacterId unchanged` `{ holderCharacterId, proposedAboutCharacterId, error }` (pre: `[Memory] …`, and NO debug line on a flip).
- `applyEpisodicFallbackAnchors(row, timeZone)` (`:72-106`): non-AUTO (truthy source ≠ AUTO) → as-is; needs entities (`!entities || length 0`) and/or `!occurredAt`; `details = extractNovelDetails(content || '', '')`; occurredAt: `anchorIso = sourceMessageTimestamp ?? nowIso`, first detail with `resolveWhenPhrase(detail, anchorIso, timeZone)` non-null; entities: details matching `/^[A-Z]/` without `/\d/`, first 6, only when non-empty; any throw → row unchanged.

#### `links.ts` (27) — `linkBothWays(characterId, memoryId, linkTo)`: `targets = linkTo.filter(id !== memoryId)`; for each target `addRelatedMemoryIds(characterId, target, [memoryId])`; then `written = targets.length ? addRelatedMemoryIds(characterId, memoryId, targets) : []`; DEBUG `[Admission] Linked memory to neighbours both ways` `{ characterId, memoryId, linked: targets.length }`; returns `written ?? targets`. Callers: `admit.ts` insert-linked, `extraction/passes/episode-fold.ts:110` (fragment links).

#### `describe.ts` (51) — `describeAdmission(outcome, label, summary)` → `[Memory] <head> — <summary>[ · <neighbours>]`:

| action | head |
|---|---|
| insert | `Created ${label}` |
| insert-linked | `Created ${label} (linked to ${linkedIds.length \|\| linkTo.length} related, ${reason})` |
| reinforce | `REINFORCED ${label}: ${id} (count ${memory?.reinforcementCount ?? '?'}, +${novelDetails.length} details)` |
| promote-and-reinforce | `REINFORCED ${label}: ${id} (promoted from cold, count ${count ?? '?'})` |
| absorb | `SKIPPED near-duplicate ${label}: absorbed into ${id} (similarity ${best ? best.cosine.toFixed(3) : 'n/a'}${redirectedFrom ? \`, via ${redirectedFrom}\` : ''})` |
| reject embedding-failed | `SKIPPED ${label} — embedding generation failed: ${trace.notes.at(-1) ?? 'unknown'}` |
| reject empty | `SKIPPED ${label} — no usable text` |

`id = memory?.id ?? (into || 'unknown')`; neighbours = non-`below` assessments as `${id.slice(0,8)} ${cosine.toFixed(3)} ${verdict | 'excluded:'+excludedBy}${via ? ' via '+via.slice(0,8) : ''}` joined `; `.

#### `outcome.ts` (33) — `admissionInserted` (insert | insert-linked), `admissionReinforced` (reinforce | absorb | promote-and-reinforce), `isNearDuplicateDecision(decision, thresholds)` (has `into` and the target's assessment cosine ≥ `nearDuplicate`; a digest redirect in the reinforce band is NOT one).

#### `rate-ledger.ts` (66, Phase D) — `RATE_LEDGER_WINDOW_MS = 1h`; per-character insert times (`processSingleton('memory.admission.rateLedger')`); `recordAdmissionInsert(characterId, at)` (parent only; prune then push); `countRecentAdmissionsLocal(characterId, sinceMs, now)`; `countRecentAdmissions` (host-RPC from the child; failure WARN `[Memory] Could not read the admission rate ledger; counting none` `{ characterId, error }` → 0); `resetRateLedgerForTests`. In memory only: "a restart forgets the last hour, which only ever errs towards admitting". Consumer: `extraction/rate-limit.ts`.

#### `dry-run-book.ts` (71, Phase D review) — `DryRunBook { retiring: Set; inserted: Map<characterId, {memory, embedding}[]> }`; `createDryRunBook(retiring)`; `bookNeighbours(book, characterId, embedding)` (same-length vectors, cosine, desc); `recordBookInsert(book, row, embedding)` (id `dry-run:<characterId>:<n>`, the row's fields with `source ?? 'AUTO'`, `tier 'hot'`, count 1). Consumer: `extraction/dry-run.ts:114`.

#### `update-memory.ts` (195, Phase B) — the hand edit (`PUT /memories/[id]`, search-replace). `MemoryEditPatch { content?, summary?, keywords?, tags?, importance?, aboutCharacterId?, chatId?, relatedMemoryIds? }`; `UpdateMemoryResult = ok | not-found | invalid-related {invalidIds} | embedding-failed {message}`. Order: find (`not-found`); importance change → `reinforcedImportance` recomputed (DEBUG `Recomputed reinforcedImportance for edited importance`); `relatedMemoryIds` validated per id with `findByIdForCharacter` (DEBUG `Refusing memory edit: related ids missing or belong to another character` → `invalid-related`), added/removed computed; summary/content change → embed FIRST (no `priority` — interactive), failure WARN `Memory edit refused: the edited text could not be embedded` and message `Could not embed the edited memory: <msg>`; write text+vector in ONE `memoryIndex.upsert(..., alongside: update)` (DEBUG `Edited memory re-embedded`) else `updateForCharacter`; link deltas both ways (DEBUG `Mirrored edited memory links onto neighbours`); `invalidateFrozenArchive`; `publishRealtime('memories')`; a hot digest → `mirrorCommonplaceForSubjects` (DEBUG `Refreshed Commonplace mirror after digest edit`); DEBUG `Memory edited` `{ characterId, memoryId, fields }`. Child logger `module: 'memory:update-memory'`.

#### `index.ts` (46) — barrel (the public surface listed in §1.6).

### 1.7 Every log line the slice emits at the pin (verbatim)

| site | level | message | fields |
|---|---|---|---|
| `time/calendar-refresh.ts:30` | debug | `[MemoryCalendar] Instance timezone refreshed` | instanceTimeZone, effective |
| `time/calendar-refresh.ts:35` | debug | `[MemoryCalendar] Could not read the instance timezone; using fallback` | effective, error |
| `admission/admit.ts:158` | warn | `Memory not admitted: embedding failed after retry` | characterId, userId, error |
| `admit.ts:203` | debug | `Memory not admitted: empty text` | characterId |
| `admit.ts:309` | debug | `Could not patch the job child's memory index cache` | characterId, error (NO-PORT — child) |
| `admit.ts:338` | debug | `Admission already made by an earlier attempt of this job; nothing written` | characterId, action, memoryId |
| `admit.ts:387` | debug | `Admission target vanished; deciding again` | characterId, action |
| `admit.ts:390` | warn | `Admission target vanished twice; inserting the candidate` | characterId |
| `admit.ts:411` | debug | `Maintenance watermark check skipped after admission` | characterId, error |
| `admit.ts:415` | debug | `Memory admitted` | characterId, action, memoryId, neighbours, bestCosine, novelDetails, linked, keyed, thresholdScale |
| `admit.ts:464` | debug | `Admission decided (dry run; nothing written)` | characterId, action, targetId, neighbours |
| `admit.ts:588` | debug | `Admission target vanished before the write` | memoryId |
| `neighbours.ts:79` | warn | `Admission index cannot compare some rows with the candidate; matching them by text` | characterId, queryDimensions, dominantDimensions, unscorable, size |
| `neighbours.ts:87` | debug | `Admission index was stale; reloaded it from the rows` | characterId, queryDimensions |
| `neighbours.ts:150` | debug | `Could not re-embed an unindexed neighbour; skipping it` | characterId, memoryId, error |
| `neighbours.ts:157` | debug | `Scored unindexed admission neighbours by re-embedding` | characterId, candidates, scored |
| `neighbours.ts:177` | debug | `Superseding digest missing; the cold row stands for itself` | memoryId, supersededById |
| `neighbours.ts:223` | warn | `Vector index holds entries with no memory row; ignoring them` | characterId, orphanIds, searchedK |
| `neighbours.ts:248` | debug | `Gathered admission neighbours` | characterId, neighbours, redirected, orphans, searchedK, reembedded |
| `apply.ts:70` | debug | `New memory persisted` | characterId, memoryId, source |
| `apply.ts:90` | debug | `Re-embedded memory` | memoryId |
| `apply.ts:93` | warn | `Failed to re-embed memory; it keeps its old vector` | memoryId, error |
| `apply.ts:131` | warn | `Fold target is gone; nothing folded` | survivorId, characterId, step |
| `apply.ts:134` | warn | `Fold step missed after the fold landed; the fold stands` | survivorId, characterId, step |
| `apply.ts:192` | warn | `Survivor changed but no userId to re-embed with` | survivorId |
| `apply.ts:196` | debug | `Folded into survivor` | survivorId, characterId, footnotes, anchors, links, reinforcementDelta, reembedded |
| `apply.ts:221` | warn | `Failed to raise the importance of a row a manual memory folded into` | memoryId |
| `apply.ts:224` | debug | `Raised importance for a manual write` | memoryId, importance |
| `prepare.ts:53` | debug | `[Admission] Name-presence check collapsed the subject to the holder` | holderCharacterId, proposedAboutCharacterId |
| `prepare.ts:62` | warn | `[Admission] Name-presence check failed; using proposed aboutCharacterId unchanged` | holderCharacterId, proposedAboutCharacterId, error |
| `links.ts:25` | debug | `[Admission] Linked memory to neighbours both ways` | characterId, memoryId, linked |
| `rate-ledger.ts:55` | warn | `[Memory] Could not read the admission rate ledger; counting none` | characterId, error |
| `update-memory.ts` | debug/warn | `Recomputed reinforcedImportance for edited importance` · `Refusing memory edit: related ids missing or belong to another character` · `Memory edit refused: the edited text could not be embedded` · `Edited memory re-embedded` · `Mirrored edited memory links onto neighbours` · `Refreshed Commonplace mirror after digest edit` · `Memory edited` | (see §1.6) |
| repository `incrementReinforcement` | warn / debug / error | `Memory not found for reinforcement` `{memoryId, characterId}` · `Memory reinforcement counted` `{memoryId, characterId, by, reinforcementCount, reinforcedImportance, lastReinforcedAt}` · safeQuery `Error reinforcing memory` | |
| repository `appendReinforcementFootnotes` | warn / debug | `Memory not found for reinforcement footnotes` · `Reinforcement footnotes appended` `{memoryId, characterId, offered, appended}` · `Error appending reinforcement footnotes` | |
| repository link deltas | warn / debug | `Memory not found for link update` `{memoryId, characterId, op}` · `Memory links updated` `{memoryId, characterId, op, requested, changed, size}` · `Error updating memory links` | |

Every line in `admit.ts` / `neighbours.ts` / `apply.ts` / `update-memory.ts`
is emitted on a **child logger** carrying `module: 'memory:admission'` /
`'memory:admission:neighbours'` / `'memory:admission:apply'` /
`'memory:update-memory'` (v5 precedent: a `module = "…"` tracing field —
`db/wardrobe_wear_stats.rs:569`, `tools/send_mail.rs:106`). The oracle's log
capture must see child-logger bindings (memory note "child-logger bindings
invisible to the capture"). **Gone from the pin entirely:** every
`[MemoryGate] …`, `[MemoryMerge] …` logger line and every `[Gate] …`
debugInfo string (§2).

### 1.8 Who calls admission at the pin (the seams the sibling lanes own)

| caller | what it calls | lane |
|---|---|---|
| `lib/memory/extraction/run.ts:316-351` `admitDraft` — every pass (self-turn, other-turn, other-fold, episode-fold, carina via `selfTurnPass('carina')`) | `resolveEpisodicAnchors` → `candidateFromDraft` (`source: 'AUTO'`, `witnessedContext: autonomous_room / user_present`, chat / project / sourceMessageId / `anchorTimeMsFrom`) → `admitCandidate(candidate, { occurredAt, narrativeTime, sourceMessageTimestamp }, { userId, dryRun?, backfill?, dryRunBook? })`; one candidate's throw → WARN `[Memory] Admission failed for one candidate; continuing`; `admissionInserted` → `ctx.rateLimit.record`; `describeAdmission` → `result.debugLogs` (`[Memory] DRY-RUN … would ${kind}` in a dry run) | extraction (Phase D) |
| `extraction/passes/episode-fold.ts:110` | `linkBothWays` (fragments → episode), `admissionInserted` | extraction |
| `extraction/dry-run.ts:56,114` | `toAdmissionRow`, `createDryRunBook` | extraction |
| `extraction/rate-limit.ts:16` | `countRecentAdmissions`, `RATE_LEDGER_WINDOW_MS` | extraction |
| `app/api/v1/memories/route.ts:506-627` `handleCreateMemory` | `MemoryCandidateSchema.safeParse` (MANUAL, `witnessedContext 'manual'`, `targeting: null`, `when: null`, `anchorTimeMs: Date.now()`, `pass: 'manual'`) → `admitCandidate(candidate, { occurredAt: body.occurredAt ?? null, narrativeTime: null }, { userId })`; no memory → `empty` 400 `Memory content and summary must not be blank.` / `embedding-failed` 502 `The memory was not saved because its text could not be embedded. Check the configured embedding profile.` / else 500 `The memory was not saved.`; **`{ memory, action: outcome.action.kind }`, 201 when inserted else 200** | API |
| `app/api/v1/memories/[id]/route.ts:100`, `lib/search-replace/search-replace-service.ts:218` | `updateMemory` | API / search-replace |
| `lib/memory/maintenance/fold-near-duplicates.ts:113,165,183,290` | `occasionsAreDistinct`, `withAdmissionLock`, `foldInto`, `applyFold`, `FOLD_CAPS`; `maintenance/run.ts:276` `admissionThresholdsForProvider` | maintenance (Phase F) |
| `lib/memory/anchor-gate-probe.ts:121-191` | `admissionThresholdsForProvider`, `decideAdmission`, `admissionReinforced`, `isNearDuplicateDecision` | anchor probe / CLI |
| `lib/background-jobs/host/host-rpc-dispatcher.ts:133` | `admitPrepared` (host-RPC `admitMemory`) | NO-PORT (child plumbing) |
| **NOT admission** (raw `memories.create`): `lib/import/quilltap-import/import-entities.ts:587` (comment `:455`: "Deliberately uses the raw repository rather than admission … none of which is acceptable for an import"), `lib/backup/restore/restore.ts:361`, `lib/memory/consolidation.ts:484` (digests, vector in the create) | — | carriers / consolidation |

---

## 2. What v4 DELETED in the span that v5 still carries

| v4 module / export (pre line) | deleted by | v5 twin on `a7c6f0d03` |
|---|---|---|
| `lib/memory/memory-weighting.ts` (334): `MemoryWeightingConfig`, `DEFAULT_WEIGHTING_CONFIG {30, 0.70, 0.05}` `:26`, `EffectiveWeightResult`, `calculateEffectiveWeight` `:75-101`, `rankMemoriesByWeight` `:317-334`, `formatRelativeAge` `:164`, `referenceTimeMs` (moved), ranking blend + cosine floors (moved), protection (moved) | `e0ac7493a` | `crates/quilltap-core/src/memory_weighting.rs` (228): `WeightingConfig` `:17-22`, `DEFAULT_WEIGHTING_CONFIG` `:24-28`, `EffectiveWeight` `:83-90`, `calculate_effective_weight` `:92-113`, `format_relative_age` `:223-228`, `compute_ranking_blend` `:193-195`, `default_min_cosine_for_provider` `:203-209`, protection `:30-182`; `generators/optimizer.rs:924-935` `rank_memories_by_weight` (the < 0.05 filter) |
| `lib/memory/reinforced-importance.ts` (14) `log2(count + 1)` | `e0ac7493a` (formula changed by `c2795733f`) | `memory_gate.rs:42-47` `calculate_reinforced_importance` |
| `lib/memory/day-references.ts` (221): server-LOCAL `resolveDayReference(text, now)` | `e0ac7493a` | `day_references.rs` (551) — `resolve_day_reference(text, now_ms, tz)` `:307` (zone by argument; jiff civil math `:117-170`) |
| `lib/memory/episodic-anchors.ts` (59) `resolveEpisodicAnchors` | `e0ac7493a` (→ `time/anchors.ts`) | inlined: `services/memory_processor.rs:769-796` `resolve_candidate_anchors`; `services/fold_episode_pass.rs:271` |
| `lib/memory/episodic.ts` `resolveWhenPhrase` `:126-219`, `isoDateFromMs` `:106-108`, `UNIT_MS` etc. | `e0ac7493a` (→ `time/clock.ts`) | `episodic.rs:129-370` (`month_number`, `unit_ms`, `word_number`, `iso_date_from_ms` `:177`, `resolve_when_phrase` `:231`) |
| `c2795733f`'s `lib/memory/memory-calendar.ts` + `-refresh.ts` | renamed into `time/` by `e0ac7493a` | **no twin** (v5 never absorbed `c2795733f`) |
| `lib/memory/memory-gate.ts` (1,046): `NEAR_DUPLICATE_THRESHOLD/MERGE_THRESHOLD/RELATED_THRESHOLD` `:44-50`, `MAX_REINFORCEMENT_FOOTNOTES` `:58`, `GATE_TOP_K` `:61`, `DATE_GUARD_DAYS` `:71`, `occasionsAreDistinct` `:74`, `GateDecision/GateResult/MemoryGateOutcome` `:89-121`, `runMemoryGate` `:144`, `resolveColdForAbsorb` / `promoteColdMemory` / `redirectColdLink` `:186-258`, `runMemoryGateInner` `:260-419`, `countReinforcementFootnotes` / `appendCappedFootnotes` `:426-447`, `patchMemory` `:457`, `reembedMemory` `:476`, `countReinforcement` `:528`, `absorbNearDuplicate` `:558`, `reinforceMemory` `:589-685`, `linkRelatedMemories` `:694-727`, the delete chokepoints `:770-930`, `extractNovelDetails` `:969` | `8af764514` (chokepoints → Phase B `retire.ts`) | `memory_gate.rs` (369, the pure half: `DATE_GUARD_DAYS` `:16`, `occasions_are_distinct` `:21-40`, the formula `:45-47`, `MAX_REINFORCEMENT_FOOTNOTES` `:53`, `count_reinforcement_footnotes` `:59-64`, `append_capped_footnotes` `:70-82`, `STOP_WORDS` `:87-222`, `extract_novel_details` `:233-316`); `services/memory_gate.rs` (2,562): thresholds `:69-75`, `GateAction` `:80-87`, `MemoryGateOutcome` `:91-111`, `CreateMemoryOptions` `:115-143`, `apply_episodic_fallback_anchors` `:147-208`, `MemoryServiceOptions` `:210-214`, `create_memory_with_gate` `:238-361`, `run_memory_gate(_inner)` `:373-574`, cold handling `:575-715`, `generate_with_retry` `:716-750`, `create_memory_direct_with_embedding` `:752-836`, `Counted` / `count_reinforcement` / `absorb_near_duplicate` `:837-933`, `patch_memory` `:934-957`, `reembed_memory` `:978-1065`, `apply_memory_merge` `:1081-1160`, `reinforce_memory` `:1161-1304`, `link_related_memories` `:1306-1364`, `apply_name_presence_check` `:1365-1448`, `maybe_enqueue_housekeeping` `:1450-1552`, tests `:1586-2562`; `db/memories.rs:1052` / `:1141` (`delete_with_unlink` / `delete_many_with_unlink` — the retire lane's) |
| `lib/memory/memory-merge.ts` (176) `planMemoryMerge` / `applyMemoryMerge` / `MemoryMergePlan` | `8af764514` | `memory_merge.rs` (307) `plan_memory_merge` `:136-255`, `MemoryMergePlan` + `to_mem_update` `:35-73`; `services/memory_gate.rs:1081-1160` `apply_memory_merge`; caller `services/memory_dedup.rs:49-51,401` |
| `memory-service.ts` gate half: `CreateMemoryOptions`, `MemoryServiceOptions.skipGate/skipEmbedding/embeddingProfileId`, `createMemoryWithEmbedding` `:359`, `createMemoryWithGate` `:373-497`, `applyNamePresenceCheck` `:142-181`, `applyEpisodicFallbackAnchors`, `memoryRowFromOptions`, `createMemoryDirect` `:540`, `maybeEnqueueHousekeeping` | `8af764514` (+ Phase E deletes the file) | `services/memory_gate.rs` (above); callers `api/memories.rs:39-41,536`, `services/memory_processor.rs:75-76,855`, `services/fold_episode_pass.rs:70-71,337` |
| `c2795733f`'s `promoteMatchedMemory` / `promoteOnMatch` (bug 218) | `8af764514` | **no twin** — v5 never had it (no-op) |
| `admission/after-insert.ts` (Phase C's `maybeEnqueueHousekeeping` move) | `104e986c7` | no twin (v5 has the pre shape at `services/memory_gate.rs:1450-1552`) |
| The Phase-1 oracle cases `memory-weighting.ts`, `ranking-blend.ts` | (import-dead) | `harness/oracle/cases/memory-weighting.ts`, `ranking-blend.ts` + `crates/quilltap-harness/tests/memory_weighting_equivalence.rs`, `ranking_blend_equivalence.rs`, `quilltap-harness/src/lib.rs:4-190` (`corpus()` of `MemoryInputs`) |

---

## 3. Behaviour deltas a port must hit (numbered)

### Scoring

- **S1 — the reinforced-importance formula.** `min(1, i + log2(max(1, n)) × 0.05)`, non-finite `n` → 1 (`scoring/reinforced-importance.ts:23-26`). v5 `memory_gate.rs:45-47` computes `log2(n + 1)`. Every v5 caller moves with it: `db/memories.rs:784` (increment), `consolidation_plan.rs:665` (digest — the consolidation lane), `memory_merge.rs:224` (dies with the merge). A fresh row's stored value (`importance`) is unchanged (`insertRow` passes count 1; pre `memoryRowFromOptions` stored `importance` directly). ⚠ `calculate_protection_score`'s `log2(count + 1)` (`memory_weighting.rs:143`) does NOT change (v4 `protection.ts:131`).
- **S2 — `calculate_effective_weight` / `WeightingConfig` / `DEFAULT_WEIGHTING_CONFIG` / `EffectiveWeight` / `rank_memories_by_weight` DIE; `salience` replaces them at every ordering site.** v5 sites that read the FLOORED weight (each must move to salience — the recall lane owns most): `memory_injector.rs:315-316, 692-693, 869-870, 894, 1080-1081` (whole file dies with Phase E), `services/frozen_archive.rs:69, 431-436` (archive fill → `sortBySalience`), `generators/optimizer.rs:55, 924-935, 1835` (→ `sortBySalience`, **the `< 0.05` filter is dropped**, ties stable by input index), `services/housekeeping.rs:45-47, 429-430` (dies with Phase F), `services/memory_service.rs:73-75, 1039-1048, 1409-1433, 1568-1577` (recall — Phase E rewrites), `services/first_message_context.rs:247-256` (sorts by RAW `importance`; v4 now sorts by salience, `first-message-context.ts:221-224` — ties: `Array.sort` stable on the map's insertion order). The `effectiveWeight` fields on the wire become `salience`: `tools/search.rs:393` (`search_scriptorium` result metadata `effectiveWeight` → `salience`, and `importance` → `filterImportance(memory)`), `tools/self_inventory.rs:1247, 2648` (the line `weight X` → `salience X`, unfloored), `services/build_context.rs:1113, 4069` (`debugMemories[].effectiveWeight` → `salience`), `services/carina_query.rs:1362`, `services/announcer/voice_rewrite_core.rs:86`.
- **S3 — `salience(memory, clock)` + `sort_by_salience`** exactly as §1.1 (`base`, `decay`, `daysSince`, stable ties). `salience == raw_weight` numerically (v5 `memory_weighting.rs:102`).
- **S4 — `protection(memory, clock, config)`** — the NEW wrapper: MANUAL / CONSOLIDATED → `score 1, protected, protectedBySource`; else `protected = score >= 0.5`. `calculate_protection_score` unchanged (v5 `memory_weighting.rs:125-182` stays).
- **S5 — `compute_ranking_blend(cosine, salience)`** unchanged arithmetic; `default_min_cosine_for_provider` reads `is_lexical_embedding_provider` (v5 `memory_weighting.rs:203-209` compares `Some("BUILTIN")` — same set; port the predicate so admission / recall / consolidation share it).
- **S6 — the importance constants + `filter_importance` + `importance_band`** (`RECALL_MIN_IMPORTANCE 0.3`, `TOOL_MIN_IMPORTANCE 0`, `IMPORTANCE_HIGH 0.7`, `IMPORTANCE_LOW 0.3`). Band sites at 0.4 today: `tools/search.rs:984-991` (`search_scriptorium`'s label — v4 bands `result.metadata.importance`, which is now `filterImportance(memory)`), the SPA `apps/web/src/app/memory/memory-format.ts:4-27` (card / editor — v4 bands RAW `memory.importance` there).
- **S7 — the per-turn recall floor .5 → .3.** v5 `services/orchestrator.rs:4594-4595` (`// v4 buildMessageContext passes minMemoryImportance: 0.5` → `RECALL_MIN_IMPORTANCE`); fold the two local consts `services/carina_query.rs:80` (`CARINA_MEMORY_MIN_IMPORTANCE = 0.3`) and `services/announcer/voice_rewrite_core.rs:54` (`RECALL_MIN_IMPORTANCE = 0.3`) onto the one constant; `pre_compute.rs:404`, `recall_replay.rs:562,606` already pass 0.3 literally.
- **S8 — every `minImportance` filter on reinforced importance.** In-memory (`reinforcedImportance ?? importance`): v5 `services/memory_service.rs:1056-1061, 1385-1390, 1502-1503` (raw `importance`, `unwrap_or(0.0)`), `api/memories.rs:198` (the character-memories listing filter). SQL (the COLUMN, NULL excluded — §0.1 item 1): `db/memories_read.rs:534-536` (`AND importance >= ?` → `AND reinforcedImportance >= ?`), `:620-630` `find_by_importance` (`importance >= ?2` → `reinforcedImportance >= ?2`). Recap bands (bug 215, `findRecentByImportanceTier`) are the recall lane's.
- **S9 — `TOOL_MIN_IMPORTANCE` as the default** for the search tool (`tools/search.rs:234-237`, already `unwrap_or(0.0)`) and the Memories API search (`api/memories.rs:854-874`) — `minImportance ?? 0` (pre: undefined → no filter; identical result except it now always filters on `filterImportance`).
- **S10 — `rank_for_recall` / `rank_by_blend` / `reserve_background_slots` / `by_blended_after`** (§1.1 `rank.ts`) — provided to the recall lane; v5's inline equivalents are `services/memory_service.rs:1133-1150, 1230-1240, 1587-1590` (R6 at the `[Memory] Background reservation (R6)` site).
- **S11 — `format_relative_age` → `format_memory_age`** (time module), byte-identical output (v5 `memory_weighting.rs:223-228`; consumers `memory_injector.rs:718,932,1131`, `services/memory_recap/mod.rs:24,573`).
- **S12 — the scoring chokepoint census** (`__tests__/unit/lib/memory/scoring/scoring-chokepoint.test.ts`): no source outside the scoring module may name `effectiveWeight` / `calculateEffectiveWeight` / `rankMemoriesByWeight` / `memory-weighting` / `memory/reinforced-importance`. Port as a v5 source census (`effective_weight`, `calculate_effective_weight`, `rank_memories_by_weight`, `memory_weighting`) over `crates/*/src` + `apps/web/src`.

### Time

- **T1 — the memory calendar zone.** ONE zone for event time: the first valid `chat_settings.timezone` → `QUILLTAP_TIMEZONE` → the system zone (`calendar.ts:60-66`, `calendar-refresh.ts:24-41`; a 5-minute TTL cache). v5 today: the distill scan uses `server_tz` (the HOST calendar name, `services/memory_recap/distill.rs:204-208`, `build_context.rs:2518`, `pre_compute.rs:285`, `recall_replay.rs:345`) — never the Salon `timezone`; every write-side stamp is UTC (`episodic.rs:177` `iso_date_from_ms`). The host's `server_tz` already folds `QUILLTAP_TIMEZONE` / `TZ` (`host_zone.rs:61-62`), so v5's chain is `first valid chat_settings.timezone` → `server_tz` (§7.3, §9 O5). Validity = an IANA name the runtime accepts (v4: `Intl.DateTimeFormat`; v5: `jiff::tz::TimeZone::get` — §9 O6).
- **T2 — the zoned arithmetic.** `zoned_parts`, `offset_ms` (second-truncated), **`zoned_time_to_utc_ms`'s two-pass guess** (§1.2) with `Date.UTC`-style overflow normalisation of `(year, month, day + offset, hour)`, `start_of_zoned_day_ms`, `zoned_time_on_day_ms`, `start_of_zoned_week_ms` (Monday), `zoned_date_stamp`, `zoned_day_bounds_iso` (real-calendar check). v5's `day_references.rs:117-170` uses jiff civil arithmetic (`local_date` / `start_of_local_day` / `local_time_on_day` / `start_of_local_week`) — its DST-gap disambiguation is jiff's, not v4's two-pass (§9 O7). Rewrite onto the ported helpers.
- **T3 — the read side's windows close 1 ms early.** `to = ISO(endMs − 1)` for every table rule: yesterday `[day−1, day0 − 1ms]` (pre `to = day0`), day before yesterday, last night `→ 05:59:59.999 local` (pre `06:00`), N days ago, last week, next week, tomorrow; today / this week keep `to = now`. Scan order is the table order (§1.2), whitespace collapsed in `matched`. v5 `day_references.rs:163-307`.
- **T4 — the WRITE side stamps the window START on the memory calendar** (`resolve_event_time`, §1.2 steps 1–10). Per-phrase moves vs v5 `episodic.rs:231-367` (the pre shape): `today` / `tonight` / `this morning|afternoon|evening` / `earlier today` → **local midnight** (was the anchor instant); `this night` / `earlier` / `just now` / `now` → anchor (unchanged); `yesterday` → local midnight − 1 day (was UTC midnight of `anchor − 24h`); `last night` → **18:00 local the day before** (was UTC midnight of `anchor − 24h`); `day before yesterday` → local midnight − 2 (was NULL); `N days ago` 1–14 → local midnight − N, > 14 → `daysBack(n)` local (was UTC date of `anchor − n days`); `this week` → **Monday 00:00 local** (was NULL); `last week` → previous Monday 00:00 local (was UTC midnight `anchor − 7d`); a leading `the ` is stripped for the table match; ISO date-only → local midnight via `zoned_day_bounds_iso`, an impossible date (`2026-02-31`) → NULL (was the RAW `2026-02-31T00:00:00.000Z` — `episodic.rs:26-29` documents that quirk); named dates → local midnight, the year roll-back compared against the zoned candidate; number words `eleven…fourteen` (was up to `ten`); `a couple / few … earlier|before` (was `ago|back` only); whitespace runs collapse before matching; weekday from the ZONED anchor (was `getUTCDay`); `last month/year`, `the other day`, seasons → local midnights. `resolve_when_phrase(phrase, anchor_iso, tz)` gains the zone.
- **T5 — `resolve_episodic_anchors(input)` with `time_zone`** (`time/anchors.ts`) as ONE function (v5 inlines it: `services/memory_processor.rs:769-796`, `services/fold_episode_pass.rs:271`) — the extraction lane calls it.
- **T6 — `normalise_time_range(range, clock)`** — date-only bounds → LOCAL day bounds, invalid calendar date → null, both bounds required (§1.2). v5 `services/memory_recap/distill.rs:477-500` widens to UTC days (`T00:00:00.000Z` / `T23:59:59.999Z`). The recall lane calls it.
- **T7 — `search_scriptorium` `since` / `until`** — date-only → `zoned_day_bounds_iso(…).from/.to`, falling back to the old UTC strings when the date is invalid (`search-scriptorium-handler.ts:106-112`); the zone is read only when either bound is present. v5 `tools/search.rs:246-258`.
- **T8 — the anchor line's date on the memory calendar.** `build_memory_anchor_line(view, time_zone)` renders `zoned_date_stamp(Date.parse(occurredAt), tz)` (v5 `episodic.rs:75-112` takes `js_slice10` — the UTC date). Every embedding of a memory row flows through it (`embeddingTextFor(row, tz)` — the index-store lane owns the text builder; this slice provides the stamp).
- **T9 — `MemoryClock` / `clock_for` / `memory_clock_for`** (`clock.ts:34-84`) — the recall / extraction lanes' clock value (`timelineMode` defaulting `'realtime'`).

### The candidate

- **C1 — `MemoryCandidate` (the Zod schema) is a NEW type.** v5's `memory_tasks.rs:149` `MemoryCandidate` is v4's OLD type (now `MemoryDraft`): content/summary optional, `importance` a raw JSON number defaulting 0.5 only when absent (`:730-733`, `importance_f64` `unwrap_or(0.5)` `:171-173`). Port `candidate_from_draft` (blank-text fallbacks, `coerce_importance` clamp [0,1] — a 0 stays 0 —, `kind` coercion, entities ≤ 12, `tags: []`, null on schema refusal), `to_admission_row` (targeting tags AFTER the free keywords), `normalise_targeting`, `targeting_keywords`, `split_targeting_keywords`, `anchor_time_ms_from`, `MEMORY_ENTITY_CAP`, `CandidatePass` (8 values). The rename collision is a §9 point (O2).

### Admission

- **A1 — ONE entry point replaces `create_memory_with_gate`** (`services/memory_gate.rs:238-361`). Order: empty-text reject BEFORE embedding (content OR summary blank after trim → `reject{empty}`, no embedding call) → memory zone → name presence → fallback anchors (zone-aware `resolve_when_phrase`) → embed (`embeddingTextFor(row, tz)`, `priority background`, `lexicalText`, one retry after 500 ms — v5 `generate_with_retry` `:716-750` omits the sleep as a recorded host-timing choice, §9 O9) → decide under the per-character lock → write → publish → rate ledger → maintenance watermark → ledger → `Memory admitted`. v5 has no `empty` reject (`create_memory_with_gate` embeds whatever text it gets).
- **A2 — the decide** (§1.6 `decide.ts`) replaces v5's best-match-only band logic (`services/memory_gate.rs:446-573`): **every** neighbour ≥ `reinforce` is checked; NEW exclusions `subject` / `kind` / `manual-over-auto`; the date guard now one of four exclusions over EVERY close neighbour (v5 checks only `results[0]`, `:471-489`) and reads the HYDRATED row (the digest after redirection; v5 reads the member's own `occurredAt`); an excluded neighbour becomes a link target and the loop continues to the next eligible one; `insert-linked` links band + excluded rows together with `reason` (v5's `InsertRelated` carries either the band rows OR the single date-guarded best); the near / reinforce choice comes from the FIRST ELIGIBLE neighbour (not `results[0]`).
- **A3 — the cold tier.** A cold row nothing supersedes (or whose digest chain dangles) → `promote-and-reinforce` with footnotes when `cosine < nearDuplicate` (v5 `resolve_cold_for_absorb` `:605-650` promotes then SKIPs / REINFORCEs; a DANGLING multi-hop intermediate is NOT promoted in v5 — it stops on the last cold-superseded row). A member reached through a digest → `absorb { redirectedFrom }` in EITHER band (v5: count-only `SkipNearDuplicate` in both — same writes). A cold row in the related band is linked, never promoted (v5: same via `redirect_cold_link`). Digest resolution now runs for every top-5 neighbour BEFORE deciding, collapsing members of one cluster onto the digest at the best cosine (v5 collapses only the band list, `:532-556`).
- **A4 — the neighbour search (bug 200 + bug 237).** Both tiers (`hotOnly: false`), the widen loop to `k ≤ 20` past orphans with the WARN above (v5 `:441-446` searches `GATE_TOP_K` once and silently drops orphans via `best_mem.as_ref()` — the pre-fix shape, ledger `:595`); the `exclude` set (dry run); the dimension check + one reload + the word-match / re-embed fallback (bug 237 — v5 has none; a dimension-mismatched store simply scores nothing). `memories_read::search_by_content` (`db/memories_read.rs:599`) is the word-match read.
- **A5 — the per-character lock.** Search → decide → write is serialised per character (two concurrent admissions make one row + a reinforcement, test "serialises concurrent admissions for one character"). v5: none — `run_memory_gate_inner` reads on the pool and writes through `Db::write` with awaits between (§9 O3).
- **A6 — the idempotency ledger** (24 h / 5,000, keyed `jobId:contentKey:n`, only inside a job) — §9 O3.
- **A7 — the vanished target.** Decide once more; a second vanish force-inserts (WARN). v5: `count_reinforcement` returning `None` WARNs `[MemoryGate] Failed to update memory during reinforcement` and returns the stale row (`:1268-1290`).
- **A8 — THE ONE COMBINE (`fold_into` + `apply_fold`) replaces `reinforce_memory` (`:1161-1304`), `absorb_near_duplicate` (`:907-933`) for the fold path, and `plan_memory_merge` / `apply_memory_merge` (`memory_merge.rs:136-255`, `services/memory_gate.rs:1081-1160`).** Moves: `occurredAt` → the EARLIEST (v5 reinforce fills only a NULL `occurredAt`, `:1202-1220`); `narrativeTime` fill now for dedup too; entities: case-insensitive union ≤ 12, skipping blank / non-string (v5 reinforce `[...existing, ...fresh].slice(0,12)`; merge did not union entities at all); links by `addLinks` DELTA, never removing (v5 merge writes a whole `relatedMemoryIds` array minus `exclude_ids`); count by `reinforcementDelta = Σ max(1, count ?? 1)` through `increment_reinforcement(by)` (v5 merge writes an ABSOLUTE `reinforcementCount` + `reinforcedImportance` via `MemUpdate`, `memory_merge.rs:51-58,218-228` — bug 226); `lastReinforcedAt` = the latest observation, never rewound by the repository; re-embed whenever content OR any anchor key changed, **including at the footnote cap** (v5 reinforce skips the re-embed at the cap, `:1292-1298`); WRITE ORDER footnotes → anchor patch → links → count, the first landed write is the commit point (v5 reinforce: count FIRST, then one content+anchor patch, `:1268-1293`); footnotes written by the repository's atomic `append_reinforcement_footnotes` with the "already present" skip (v5 writes the whole new content in a patch).
- **A9 — `append_capped_footnotes(content, details, cap)`** skips a detail whose `[+] ` line is already present and dedupes within the call (v5 `memory_gate.rs:70-82` takes a prefix of `details`; no `cap` parameter).
- **A10 — MANUAL folds raise importance** (`raise_importance`) when a hand-written candidate reinforces / is absorbed into a MANUAL row or a digest; AUTO is never promoted to MANUAL (v5: a MANUAL create can REINFORCE an AUTO row today — no subject / source check).
- **A11 — `insert_row`** writes the vector IN the create (invariant 4) and caches it hot (v5 `create_memory_direct_with_embedding` `:752-836` writes the row then the `vector_entries` store — the index lane re-homes the vector onto `memories.embedding`); `aboutCharacterId || null`, `chatId || null`, `source || 'MANUAL'`, `sourceMessageId || null`, `reinforcedImportance = formula(importance, 1)`, created/updated from `sourceMessageTimestamp` when given.
- **A12 — `embeddingProfileId` is gone from admission** (always the user's default profile, `generateEmbeddingForUser(text, userId, undefined, …)`; v5 `MemoryServiceOptions.embedding_profile_id` `:210-214` dies).
- **A13 — provider-scaled thresholds.** `admission_thresholds_for_provider(embedding_provider)` — `BUILTIN` (lexical) → `0.80 / 0.40 / 0.20`; neural → `0.90 / 0.85 / 0.70`; date guard 7 and footnote cap 8 shared; the provider is the one that embedded the candidate (`EmbeddingResult.provider`); `rejected` traces carry the NEURAL set. v5's constants `services/memory_gate.rs:69-75` and `anchor_gate_probe.rs:68,198-212,455-456,555-556` are neural-only.
- **A14 — every log line** of §1.7 (with the `module` field) replaces every `[MemoryGate]` / `[MemoryMerge]` line in v5 (`services/memory_gate.rs:425-430, 525-528, 568-571, 912-923, 1003-1058, 1092-1138, 1184-1189, 1270-1285`); the name-presence WARN prefix `[Memory]` → `[Admission]` and a NEW flip DEBUG.
- **A15 — `describe_admission`** (the extraction debug-log line, §1.6) — consumed by the extraction lane; replaces v5's per-gate-action debug strings in `services/memory_processor.rs` `write_candidate`.
- **A16 — the realtime hint.** `publishRealtime('memories')` once per admission that has a row (insert, insert-linked, reinforce, absorb, promote) — a NEW publish site (pre had none on create; v5 none). It needs a `realtime_publish_sites_guard` census row (`crates/quilltap-harness/tests/realtime_publish_sites_guard.rs:74-104`) and a `realtime::publish_sites` behavioural pin.
- **A17 — the rate ledger and the maintenance watermark after an insert.** AUTO, non-backfill inserts are recorded (`record_admission_insert`); EVERY insert (manual included, backfill included) fires `maybe_enqueue_maintenance([characterId])` fire-and-forget (v5 fires `maybe_enqueue_housekeeping` on insert, `services/memory_gate.rs:336, 350` — Phase F lane replaces the target).
- **A18 — `update_memory`** (§1.6) replaces v5 `api/memories.rs:599-663` `memory_update` (no re-embed, raw `update_for_character`) and the search-replace path (`db/memories.rs:948` `replace_in_memories`, v4 removed `replaceInMemories` — bug 233). Embed first / write text + vector in one update / link deltas both ways / archive invalidation / publish / digest mirror.
- **A19 — the dry run** (`decide_only` + the `DryRunBook`) — writes nothing, `exclude: retiring`, the book's would-be inserts as extra neighbours (top-5 after merge). v5's dry run never reaches the gate (`services/memory_processor.rs` collects candidates).
- **A20 — `occasions_are_distinct(a, b, date_guard_days)`** takes the guard as an argument (v5 `memory_gate.rs:21-40` reads the constant).

---

## 4. v5 on `main` today — per file in the slice

| file | lines | what it is | fate at the pin |
|---|---|---|---|
| `crates/quilltap-core/src/memory_weighting.rs` | 228 | Phase-1 port of `memory-weighting.ts`: weighting config + `calculate_effective_weight` (`:17-113`), protection (`:30-182`), blend + floors (`:188-209`), `format_relative_age` (`:223-228`), `MemoryInputs` (`:66-81`) | **dies / re-homes** as the scoring module: `:17-28`, `:83-113` DIE; protection keeps; blend / floors move; age label moves to time; `MemoryInputs` → a `SalienceInput`/`ProtectionInput` view |
| `crates/quilltap-core/src/memory_gate.rs` | 369 | the pure gate half: date guard `:12-40`, formula `:42-47`, footnotes `:49-82`, novel details `:84-316`, tests `:318-369` | split: `:42-47` → scoring (formula moves, S1); `:49-82` → `reinforcement_footnotes` (A9); `:84-316` → `admission::novel_details` (byte-identical); `:12-40` → `admission::decide` (3-arg, A20) |
| `crates/quilltap-core/src/services/memory_gate.rs` | 2,562 | the stateful gate (`:238-1552`) + tests `:1586-2562` | **dies whole** for `services::admission` (A1–A19); the watermark `:1450-1552` belongs to Phase F's `maybe_enqueue_maintenance`; the name-presence `:1365-1448` and fallback anchors `:147-208` move to `admission::prepare` |
| `crates/quilltap-core/src/memory_merge.rs` | 307 | `plan_memory_merge` | **dies** for `admission::fold::fold_into` (keep its `earliest_occurred_at` `:103-122` and the UTF-16 `max_by` `:207-210` idioms) |
| `crates/quilltap-core/src/services/memory_dedup.rs` | 1,117 | the dedup tool (calls `apply_memory_merge` `:401`) | dies with Phase F (maintenance fold calls `fold_into` / `apply_fold`) |
| `crates/quilltap-core/src/day_references.rs` | 551 | `resolve_day_reference(text, now_ms, tz)` + jiff civil math | **rewritten** as `memory_time::{rules, clock}` (T2, T3) |
| `crates/quilltap-core/src/episodic.rs` | 545 | anchor line `:75-112`, embedding text `:114-126`, `resolve_when_phrase` `:129-367`, event clocks `:370-386`, `js_date_parse_ms` `:402` | `:129-367` → `memory_time::clock::resolve_event_time` (T4); `:75-112` gains the zone (T8); `:114-126` → the index lane's `embedding_text_for` (+ `lexical_embedding_text_for`); event clocks + `js_date_parse_ms` stay |
| `crates/quilltap-core/src/db/memories.rs` | — | `increment_reinforcement` `:721-799` (no `by`, `lastReinforcedAt = at` unconditionally) | gains `by` (positive integer, else throw-equivalent), never-rewind `lastReinforcedAt`, the DEBUG `by` + `lastReinforcedAt` fields; NEW `add_related_memory_ids` / `remove_related_memory_ids` / `append_reinforcement_footnotes` (the keystone / repository lane — §7.1) |
| `crates/quilltap-core/src/db/memories_read.rs` | — | `:534-536`, `:620-630` raw-importance SQL filters; `search_by_content` `:599` | S8 |
| `crates/quilltap-core/src/api/memories.rs` | — | `memory_create` `:490-555` (always `{memory}` 201, client `source` honoured, `skipGate` ignored); `memory_update` `:599-663`; search `:854-874` | the API lane: `{ memory, action }` + 201/200 + the reject mapping (§7.4) |
| `crates/quilltap-core/src/services/memory_processor.rs` | 1,667 | `resolve_candidate_anchors` `:769-796`; `write_candidate` → `create_memory_with_gate` `:855` | the extraction lane re-homes onto `admit_candidate` |
| `crates/quilltap-core/src/services/fold_episode_pass.rs` | — | `create_memory_with_gate` `:337`; `resolve_when_phrase` `:271` | extraction lane |
| `crates/quilltap-core/src/services/anchor_gate_probe.rs` | 736 | MERGE / NEAR thresholds `:68, 198-212, 455-456, 555-556` | re-aimed onto `decide_admission` + provider thresholds (probe owner) |
| `crates/quilltap-core/src/memory_tasks.rs` | — | old-shape `MemoryCandidate` `:149-174`, importance parse `:730-733` | renamed draft (extraction lane, §9 O2) |
| `crates/quilltap-core/src/generators/optimizer.rs` | — | `rank_memories_by_weight` `:924-935`, call `:1835`, test `:2243` | → `sort_by_salience` (S2) |
| `crates/quilltap-core/src/services/frozen_archive.rs` | 1,123 | `calculate_effective_weight` fill `:431-436`; `memory_inputs` `:447` | recall lane (→ `sort_by_salience`) |
| `crates/quilltap-core/src/services/first_message_context.rs` | — | importance sort `:247-256` | → salience (S2) |
| `crates/quilltap-core/src/consolidation_plan.rs` | — | formula call `:665` | moves with S1 (consolidation lane) |
| `crates/quilltap-core/src/tools/search.rs`, `tools/self_inventory.rs` | — | S2 / S6 / S7 / T7 sites | tools / recall lane |

---

## 5. Harness

### 5.1 Families in this slice — fate at the pin

| family (Rust test ↔ oracle) | tier | at the pin | why |
|---|---|---|---|
| `memory_weighting_equivalence.rs` (191) ↔ `cases/memory-weighting.ts` (134) | 1 | **IMPORT-DEAD** | imports `calculateEffectiveWeight`, `calculateProtectionScore`, `formatRelativeAge`, `DEFAULT_WEIGHTING_CONFIG`, `DEFAULT_PROTECTION_CONFIG` from `@/lib/memory/memory-weighting` |
| `ranking_blend_equivalence.rs` (135) ↔ `cases/ranking-blend.ts` (76) | 1 | **IMPORT-DEAD** | same module; the VALUES survive (blend, floors, age) |
| `memory_name_helpers_equivalence.rs` (126) ↔ `cases/memory-name-helpers.ts` (76) | 1 | **IMPORT-DEAD** (and RED once re-pointed) | `calculateReinforcedImportance` from `@/lib/memory/memory-gate`; rows `one` / `power-of-two` / `saturates-cap` / `cap-at-7` / `exact-16` / `hundred` change value (`zero-count` / `zero-base` do not); the name-helper rows (`format-utils`, `about-character-resolution`) are unaffected |
| `extract_novel_details_equivalence.rs` (50) ↔ `cases/extract-novel-details.ts` (51) | 1 | **IMPORT-DEAD, value-GREEN** | `@/lib/memory/memory-gate` → `@/lib/memory/admission/novel-details` (byte-identical function) |
| `memory_merge_plan_equivalence.rs` (122) ↔ `cases/memory-merge-plan.ts` (213) | 1 | **IMPORT-DEAD** | `planMemoryMerge` (`memory-merge`) + footnote helpers (`memory-gate`) |
| `day_references_equivalence.rs` (162) ↔ `cases/day-references.ts` (63) + `fixtures/day-references.json` | 1 | **IMPORT-DEAD** + RED | `@/lib/memory/day-references`; `to` moves 1 ms; the zone source |
| `episodic_equivalence.rs` (308) ↔ `cases/episodic.ts` (245) | 1 | **IMPORT-DEAD** (missing exports) + RED | `resolveWhenPhrase` and `buildMemoryEmbeddingText` are no longer exported by `@/lib/memory/episodic`; `buildMemoryAnchorLine` gains the zone |
| `memory_gate_tier3_equivalence.rs` (360) ↔ `cases/memory-gate-tier3.test.ts` (250) + `fixtures/build-memory-gate-fixture.ts` + `memory-gate-tier3.json` | 3 | **IMPORT-DEAD** | `createMemoryWithGate` from `@/lib/memory/memory-service` (deleted); the builder dies on `repos.vectorIndices` (Phase B) |
| `memory_watermark_tier3_equivalence.rs` (276) ↔ `cases/memory-watermark-tier3.test.ts` (213) | 3 | **IMPORT-DEAD** | `createMemoryWithGate` + `@/lib/memory/housekeeping-outcome-cache` |
| `anchor_gate_probe_tier3_equivalence.rs` (476) ↔ `cases/anchor-gate-probe-tier3.test.ts` (264) | 3 | RED | module survives; rows gain `anchoredAction` / `anchorFreeAction`, the provider scale, `lexicalProfile` |
| `first_message_context_equivalence.rs` (357) ↔ `cases/first-message-context.test.ts` (166) | 2/3 | RED | salience ordering (S2) |
| `memory_processor_tier3`, `fold_episode_tier3`, `fold_other_pass_tier3`, `carina-memory-extraction-tier3` | 3 | IMPORT-DEAD (`memory-processor`, `fold-*-pass` deleted) | the extraction lane's — they exercised the gate through the writers |
| `memory_dedup_equivalence.rs` ↔ `cases/memory-dedup.test.ts` | 2 | IMPORT-DEAD (`lib/tools/memory-dedup` deleted, Phase F) | maintenance lane |
| non-slice cases that `jest.doMock('@/lib/memory/memory-gate', …requireActual…)`: `cases/archive-reencrypt-tier2.test.ts:115`, `cases/character-archive-tier2.test.ts:220`, `cases/memory-delete-tier2.ts:93` | 2 | **IMPORT-DEAD** — a non-virtual `doMock` of a missing module throws `Cannot find module` | the carriers / retire lanes must drop the mock |
| `repository_zod_messages` (`cases/repository-zod-messages.ts:346-347`) | 1 | GREEN | only the column NAME `reinforcedImportance` |

### 5.2 What they become (proposed corpus)

1. **`memory_scoring_equivalence`** (tier 1; NEW case `cases/memory-scoring.ts` importing `@/lib/memory/scoring` + `@/lib/memory/time/age-label`): the Phase-1 corpus (`quilltap-harness/src/lib.rs` `corpus()`, the 17 ids of `cases/memory-weighting.ts:50-93`) re-run as `{ salience: {salience, base, decay, daysSince}, protection: {score…, daysSinceRefTime}, protectionWrapper: {protected, protectedBySource, score}, age: formatMemoryAge }`; PLUS rows for `source: 'MANUAL' | 'CONSOLIDATED'` (score 1), a future-dated row (`daysSince` 0), `reinforcedImportance` absent / present, `sortBySalience` ties; `reinforcedImportance(i, n)` for `n ∈ {NaN, −1, 0, 1, 2, 3, 7, 15, 64, 100}` × `i ∈ {0, 0.3, 0.5, 0.9, 0.97}`; `filterImportance` (null / absent / present), `importanceBand` at `0.29999, 0.3, 0.69999, 0.7`; the constants. Replaces `memory_weighting` + `memory_name_helpers`' reinforced half (keep the name-helper rows in their own re-pointed case).
2. **`ranking_blend`** re-pointed to `@/lib/memory/scoring` (`computeRankingBlend(c, salience)`, `defaultMinCosineForProvider`) + `@/lib/memory/time` (`formatMemoryAge`) — values unchanged; ADD `rankByBlend` / `reserveBackgroundSlots` rows (the recall lane may own `rankForRecall`, which needs `recall/tags`).
3. **`memory_time_equivalence`** (tier 1; replaces `day_references` + `episodic`'s `resolveWhenPhrase` half): drive `resolveEventTime`, `resolveReferenceWindow`, `normaliseTimeRange`, `zonedDayBoundsIso`, `zonedTimeToUtcMs`, `startOfZonedWeekMs`, `buildMemoryAnchorLine(view, tz)` with the zone passed EXPLICITLY (no `TZ` legs needed any more — the functions take the zone; still run one leg under `TZ=UTC` because `Date.parse` of a zone-less datetime is process-local, `episodic.ts:14-21`). Zones: `UTC`, `America/Chicago`, `Asia/Kolkata` (+5:30), `Pacific/Chatham` (+12:45 / DST), `America/Santiago` (midnight DST gap), `Europe/London`, `Australia/Lord_Howe` (30-min DST). Anchors around both 2026 DST transitions and 23:30 / 00:30 local. Every past phrase of `time/__tests__/rule-table.test.ts` both directions ("write lands inside read"), every write-only rule, the ISO-impossible date, the `the ` strip, whitespace runs, `eleven…fourteen`, `a few weeks earlier`.
4. **`admission_decide_equivalence`** (tier 1, PURE; NEW case importing `@/lib/memory/admission/decide` + `outcome` + `describe`): hand-built neighbour lists — every row of `lib/memory/__tests__/admission-decide.test.ts` (20 cases: the bands, subject / kind / source, the date guard over every neighbour, digests and the cold tier, the trace), both threshold sets (`ADMISSION_THRESHOLDS_TFIDF` at 0.45 / 0.29 / 0.08 — `provider-thresholds.test.ts`), ties (stable order), `admissionInserted` / `admissionReinforced` / `isNearDuplicateDecision`, and `describeAdmission` for every kind (the `toFixed(3)` and `slice(0, 8)` renders).
5. **`admission_fold_equivalence`** (tier 1, PURE; replaces `memory_merge_plan`): `foldInto` over the merge-plan corpus + `admission-fold.test.ts`'s nine rows (the growing-content judge, the cap with and without anchor change, entity union case-insensitive survivor-first ≤ 12, narrativeTime fill, `latest` over `lastReinforcedAt ?? createdAt` incl. a non-ISO lexicographic case, an identical source → count only, non-string entity skip); `appendCappedFootnotes(content, details, cap)` incl. a detail already present; `countReinforcementFootnotes` (`\r\n`, `[+]x`).
6. **`memory_candidate_equivalence`** (tier 1): `candidateFromDraft` (blank content / summary fallbacks, whitespace-only → null, importance `-0.2 / 0 / 0.3 / 1.7 / NaN / '0.5' / undefined`, entities 13 → 12, kind coercion), `MemoryCandidateSchema` refusals (empty ids, `anchorTimeMs: Infinity`, an unknown pass), `toAdmissionRow` (keyword order, `targeting: null`), `normaliseTargeting` / `splitTargetingKeywords` / `anchorTimeMsFrom` — mirrors `memory-candidate.test.ts`.
7. **`extract_novel_details`** — re-point the import only (stays GREEN by value).
8. **`admission_tier3_equivalence`** (tier 3 — mocked embeddings, then tier-2 on the writes; replaces `memory_gate_tier3` + `memory_watermark_tier3`): drive `admitCandidate` through v4's REAL module in the parent (no job child: `QUILLTAP_JOB_CHILD` unset) with a canned `generateEmbeddingForUser` returning `{ embedding, provider }`; a fixture whose memories carry `memories.embedding` (Phase B — no `vector_entries`). Rows: empty → reject (no embedding call); embedding fails twice → reject, nothing written; insert into an empty book; insert-linked (band + excluded) with both-way link deltas; reinforce with footnote + delta + re-embed; absorb (count only); MANUAL over AUTO → linked; MANUAL into MANUAL raises importance (never lowers); cold promote-and-reinforce (`tier`, `supersededById` NULL); a member → its digest (`absorb` + `redirectedFrom`); a dangling multi-hop chain; orphans at rank 1 and filling the whole top-K (the widen); the TF-IDF scale (`provider: 'BUILTIN'`) vs neural at the same cosine; the name-presence flip; the fallback anchors from text (zone-aware); `sourceMessageTimestamp` stamping; the realtime hint; the rate ledger. Compare the `memories` table (tier-2 normalised: minted ids remap, `updatedAt` / `lastReinforcedAt` masked where wall-clock) and the captured log lines (with the `module` binding — memory note on child loggers). Copy the recipe of `memory_gate_tier3_equivalence.rs` (canned provider, `CannedEmbeddingProvider` keyed by exact text — `model/embedding.rs:98-140`) and the builder shape of `build-memory-gate-fixture.ts` minus `repos.vectorIndices`.
9. **`update_memory_tier3`** (tier 3): the nine `update-memory.test.ts` rows (re-embed on text change only, embed-first refusal writes nothing, `reinforcedImportance` recompute, related validation → `invalid-related`, link deltas both ways, digest mirror) — likely the API lane's, which owns the PUT.
10. **Source censuses:** the scoring chokepoint (S12); `services/memory_gate.rs` / `memory_merge.rs` absent; no `[MemoryGate]` / `[MemoryMerge]` / `[Gate]` string left in `crates/`.

The closest recipes: tier-1 → `extract_novel_details_equivalence.rs` (simplest NDJSON row compare); tier-3 → `memory_gate_tier3_equivalence.rs` + `harness/oracle/cases/memory-gate-tier3.test.ts:146-200` (the cipher driver path + `initializeDatabase` + dynamic import pattern).

---

## 6. v4 jest tests at the pin covering the slice (mirror these)

| file | ≈ cases |
|---|---|
| `lib/memory/__tests__/admission-decide.test.ts` | 20 (+ an `it.each` band table) |
| `lib/memory/__tests__/admission-neighbours.test.ts` | 14 |
| `lib/memory/__tests__/gate-date-guard.test.ts` | 7 (`occasionsAreDistinct` half; the retro half is recall's) |
| `__tests__/unit/lib/memory/admission-admit.test.ts` | 18 (incl. "serialises concurrent admissions", "answers a repeated job-attempt key", "decides again when the target is retired", "two near-identical candidates make one row with count 2 (bug 209)", the TF-IDF scale pair) |
| `__tests__/unit/lib/memory/admission-fold.test.ts` | 16 (`foldInto` 9, `applyFold` 7 incl. the commit point and bug 226) |
| `__tests__/unit/lib/memory/admission-helpers.test.ts` | 21 |
| `__tests__/unit/lib/memory/admission-dry-run.test.ts` | 7 |
| `__tests__/unit/lib/memory/update-memory.test.ts` | 9 |
| `__tests__/unit/lib/memory/memory-candidate.test.ts` | 13 |
| `__tests__/unit/lib/memory/provider-thresholds.test.ts` | 6 |
| `__tests__/unit/lib/memory/scoring/scoring-formulas.test.ts` | 15 (+ `it.each` tables) |
| `__tests__/unit/lib/memory/scoring/protection-and-age.test.ts` | 22 |
| `__tests__/unit/lib/memory/scoring/scoring-chokepoint.test.ts` | 1 (the census) |
| `__tests__/unit/lib/memory/ranking-blend.test.ts` | 13 |
| `lib/memory/time/__tests__/rule-table.test.ts` | 10 (+ anchors × phrases `it.each`) |
| `lib/memory/time/__tests__/calendar.test.ts` | 11 (incl. the 25-hour day, Nov 1 2026) |
| `lib/memory/time/__tests__/day-references.test.ts` | 18 |
| `lib/memory/time/__tests__/anchors.test.ts` | 5 |
| `lib/memory/time/__tests__/calendar-refresh.test.ts` | 3 |
| `__tests__/unit/lib/memory/memory-anchor-zone.test.ts` | 2 |
| `__tests__/unit/api/memories-manual-create.test.ts` | 13 (API lane — the `action` wire) |
| `__tests__/unit/lib/database/migration/recompute-reinforced-importance-v1.test.ts` / `rezone-legacy-memory-occurred-at-v1.test.ts` | 3 / 8 (the keystone's ensures — they import this slice's formula and calendar) |
| `__tests__/unit/lib/background-jobs/admission-host-rpc.test.ts` | 5 — NO-PORT (child plumbing) |
| helpers `__tests__/helpers/admission-mock.ts`, `memory-admission-fake.ts` | — the fakes other suites mock admission with |

---

## 7. Cross-lane meeting points + PROPOSED Rust spellings

### 7.1 What this slice NEEDS (from the keystone / repository / index / embedding lanes)

- **Repository (`db::memories`)** — the delta writes (bugs 226 / 231 / 234), each scoped by `(character_id, memory_id)`, atomic inside one SAVEPOINT, returning the row's new state or `None` (not found → v4's WARN):
  - `increment_reinforcement(character_id, memory_id, at, by: u32) -> Result<Option<Reinforcement { reinforcement_count, reinforced_importance, last_reinforced_at }>>` — `by` must be a positive integer (v4 throws `Memory reinforcement: delta must be a positive integer (got N)`), `lastReinforcedAt = max(stored, at)` by STRING compare (`row.lastReinforcedAt > at`), DEBUG fields gain `by` + `lastReinforcedAt` (today `db/memories.rs:721-799`).
  - `append_reinforcement_footnotes(character_id, memory_id, details: &[String]) -> Result<Option<(String /*content*/, Vec<String> /*appended*/)>>` (uses `reinforcement_footnotes::append_capped_footnotes` with the default cap; writes only when something appended).
  - `add_related_memory_ids` / `remove_related_memory_ids(character_id, memory_id, ids) -> Result<Option<Vec<String>>>` (lenient JSON parse — malformed reads as `[]`; skip self / present; `changed` flag in the DEBUG).
  - `update_for_character` accepting `reinforced_importance` (exists, `:558`), `find_by_ids`, `find_by_id_for_character`, `memories_read::search_by_content` (exists, `db/memories_read.rs:599`).
  - `create` taking the embedding IN the row (Phase B's `memories.embedding`).
- **Index store (Phase B lane)** — a `memory_index` facade: `search(character_id, &[f32], SearchOpts { limit, hot_only: bool, exclude })`, `shape(character_id, query_dims) -> { size, dominant_dimensions: Option<usize>, matching }`, `entries(character_id)`, `reload(character_id)`, `index_inserted(character_id, &[IndexEntry])`, `upsert(character_id, &[IndexUpsert { id, embedding, tier: Option, alongside: Option<MemUpdate> }]) -> Vec<Option<Value>>`, `set_tier(character_id, ids, tier, extra { superseded_by_id: Option<Option<String>>, consolidated_at }) -> usize`; plus `embedding_text_for(row, tz)` / `lexical_embedding_text_for(row)` (`index-store/embedding-text.ts`).
- **Embedding provider** — `EmbeddingProvider::generate_embedding_for_user` (`model/embedding.rs:67-78`) needs a `lexical_text: Option<&str>` (the TF-IDF lane's `4ae0c4ca6` seam); `EmbeddingResult.provider` (exists, `:22`) feeds `admission_thresholds_for_provider`.
- **Maintenance (Phase F lane)** — `maintenance::triggers::maybe_enqueue_maintenance(db, &[character_id])` (fire-and-forget after an insert; never fails the admission).
- **Realtime** — `publish_realtime(RealtimeTopic::Memories, None)` (exists) + the census row (A16).
- **The keystone's ensures** import `scoring::reinforced_importance` (`recompute-reinforced-importance-v1`) and `memory_time::calendar::{get_memory_time_zone, is_valid_time_zone, zoned_day_bounds_iso}` (`rezone-legacy-memory-occurred-at-v1`, whose zone is "the first valid `chat_settings.timezone` (SQL `SELECT timezone FROM chat_settings WHERE timezone IS NOT NULL`) then `getMemoryTimeZone`'s fallback") — this lane should land those three functions FIRST (or the keystone ports them and this lane imports them; one home either way).

### 7.2 What this slice PROVIDES

- To **recall (Phase E)**: `memory_scoring::{salience, sort_by_salience, reference_time_ms, protection, compute_ranking_blend, relevance, default_min_cosine_for_provider, filter_importance, importance_band, RECALL_MIN_IMPORTANCE, TOOL_MIN_IMPORTANCE, rank_for_recall, rank_by_blend, reserve_background_slots, by_blended_after}` (rank.rs needs the recall lane's `RecallContext` / `RecallCandidate` / `combine_recall_multipliers` — either rank lives in the recall module, or the recall lane exports those types first; §9 O8); `memory_time::{MemoryClock, clock_for, resolve_reference_window, normalise_time_range, format_memory_age}`.
- To **extraction (Phase D)**: `memory_candidate::{MemoryCandidate, candidate_from_draft, to_admission_row, anchor_time_ms_from, coerce_importance, normalise_targeting, CandidatePass}`, `memory_time::anchors::resolve_episodic_anchors`, `services::admission::{admit_candidate, AdmitOptions, describe_admission, admission_inserted, link_both_ways, DryRunBook, create_dry_run_book, count_recent_admissions, RATE_LEDGER_WINDOW_MS}`.
- To **maintenance (Phase F)**: `admission::fold::{fold_into, FoldSource, FoldPlan, FOLD_CAPS}`, `services::admission::apply::apply_fold`, `services::admission::with_admission_lock`, `admission::decide::occasions_are_distinct`, `admission::thresholds::admission_thresholds_for_provider`, `memory_scoring::protection`.
- To **consolidation**: `memory_scoring::reinforced_importance`, `memory_candidate::{normalise_targeting, split_targeting_keywords, targeting_keywords, DEFAULT_TARGETING}`, `admission::thresholds::resolve_cluster_threshold` (or the settings lane hosts it).
- To **the API lane**: `admit_candidate` + `MemoryCandidate` (the manual create), `update_memory` (PUT, search-replace), the `AdmissionActionKind` wire string.
- To **the anchor probe**: `decide_admission`, `admission_reinforced`, `is_near_duplicate_decision`, the provider thresholds.

### 7.3 PROPOSED spellings (C1)

Module homes follow v5's pure-at-root / stateful-in-`services` precedent
(`memory_gate.rs` vs `services/memory_gate.rs`):

```rust
// crates/quilltap-core/src/memory_scoring/{mod,reinforced_importance,salience,protection,relevance,importance}.rs
pub const REINFORCED_IMPORTANCE_STEP: f64 = 0.05;
pub fn reinforced_importance(importance: f64, count: f64) -> f64;          // + `pub use … as calculate_reinforced_importance` for the v4 alias
pub const SALIENCE_HALF_LIFE_DAYS: f64 = 30.0;
pub struct SalienceInput { importance: f64, reinforced_importance: Option<f64>, created_at_ms: f64, last_reinforced_at_ms: Option<f64> }
pub struct SalienceBreakdown { pub salience: f64, pub base: f64, pub decay: f64, pub days_since: f64 }
pub fn reference_time_ms(created_at_ms: f64, last_reinforced_at_ms: Option<f64>) -> f64;
pub fn salience(m: &SalienceInput, now_ms: f64) -> SalienceBreakdown;
pub fn sort_by_salience<T>(items: Vec<T>, view: impl Fn(&T) -> SalienceInput, now_ms: f64) -> Vec<T>;  // stable
pub const PROTECTION_THRESHOLD: f64 = 0.5;
pub struct ProtectionConfig { … }  pub const DEFAULT_PROTECTION_CONFIG: ProtectionConfig;   // moved from memory_weighting.rs:30-62
pub fn calculate_protection_score(m: &ProtectionInput, cfg: &ProtectionConfig, now_ms: f64) -> ProtectionScore;
pub struct ProtectionBreakdown { pub score: ProtectionScore, pub protected: bool, pub protected_by_source: bool }
pub fn protection(m: &ProtectionInput, now_ms: f64, cfg: &ProtectionConfig) -> ProtectionBreakdown;
pub const RANKING_RELEVANCE_WEIGHT: f64 = 0.75;  pub const RANKING_PRIORITY_WEIGHT: f64 = 0.25;
pub fn relevance(cosine: f64) -> f64;
pub fn compute_ranking_blend(cosine: f64, salience: f64) -> f64;
pub const DEFAULT_MIN_COSINE_NEURAL: f64 = 0.30;  pub const DEFAULT_MIN_COSINE_TFIDF: f64 = 0.10;
pub fn default_min_cosine_for_provider(provider: Option<&str>) -> f64;
pub const RECALL_MIN_IMPORTANCE: f64 = 0.3;  pub const TOOL_MIN_IMPORTANCE: f64 = 0.0;
pub const IMPORTANCE_HIGH: f64 = 0.7;        pub const IMPORTANCE_LOW: f64 = 0.3;
pub enum ImportanceBand { High, Medium, Low }   // as_str "high" | "medium" | "low"
pub fn filter_importance(importance: f64, reinforced_importance: Option<f64>) -> f64;
pub fn importance_band(value: f64) -> ImportanceBand;
// crates/quilltap-core/src/embedding_scale.rs  (v4 lib/embedding/cosine-scale.ts)
pub fn is_lexical_embedding_provider(provider: Option<&str>) -> bool;   // {"BUILTIN"}

// crates/quilltap-core/src/memory_time/{mod,calendar,rules,clock,anchors,age_label}.rs
pub fn is_valid_time_zone(tz: Option<&str>) -> bool;
pub fn memory_time_zone(instance_tz: Option<&str>, host_fallback: &str) -> String;   // getMemoryTimeZone minus the env/system read (host passes server_tz)
pub fn zoned_parts(ms: f64, tz: &str) -> ZonedParts;
pub fn zoned_time_to_utc_ms(year: i64, month0: i64, day: i64, hour: i64, tz: &str) -> f64;  // v4's two-pass, Date.UTC overflow
pub fn start_of_zoned_day_ms(ms: f64, tz: &str, offset_days: i64) -> f64;
pub fn zoned_time_on_day_ms(ms: f64, tz: &str, offset_days: i64, hour: i64) -> f64;
pub fn start_of_zoned_week_ms(ms: f64, tz: &str, week_offset: i64) -> f64;
pub fn zoned_date_stamp(ms: f64, tz: &str) -> String;
pub fn zoned_day_bounds_iso(date: &str, tz: &str) -> Option<TimeWindow>;
pub struct RuleWindow { pub from_ms: f64, pub end_ms: f64, pub past_pointing: bool }
pub static DAY_REFERENCE_TABLE: [DayReferenceRule; 9];  pub const MAX_TABLE_DAYS_AGO: i64 = 14;
pub fn match_day_reference_rule(phrase: &str) -> Option<(&'static DayReferenceRule, Captures)>;
pub struct MemoryClock { pub time_zone: String, pub now_ms: f64, pub timeline_mode: TimelineMode }
pub fn clock_for(timeline_mode: Option<&str>, memory_tz: &str, now_ms: f64) -> MemoryClock;
pub struct TimeWindow { pub from: String, pub to: String }
pub struct EventTime { pub occurred_at: Option<String>, pub window: Option<TimeWindow> }
pub fn resolve_event_time(phrase: Option<&str>, anchor_ms: f64, time_zone: &str) -> EventTime;
pub fn resolve_when_phrase(phrase: Option<&str>, anchor_iso: &str, time_zone: &str) -> Option<String>;
pub struct DayReferenceResolution { pub time_range: TimeWindow, pub past_pointing: bool, pub matched: String }
pub fn resolve_reference_window(text: &str, time_zone: &str, now_ms: f64) -> Option<DayReferenceResolution>;
pub fn normalise_time_range(from: Option<&str>, to: Option<&str>, time_zone: &str) -> Option<TimeWindow>;
pub fn resolve_episodic_anchors(input: &EpisodicAnchorInput) -> EpisodicAnchors;   // anchors.rs
pub fn format_memory_age(created_at_ms: f64, last_reinforced_at_ms: Option<f64>, occurred_at: Option<&str>, now_ms: f64) -> String;
// services/memory_time_zone.rs (stateful): instance read + v4's 5-min TTL cache, never fails
pub fn refresh_memory_time_zone(db: &Db, host_fallback: &str, now_ms: f64) -> String;  // DEBUG [MemoryCalendar] lines

// crates/quilltap-core/src/memory_candidate.rs
pub struct TargetingTags { pub temporal: Temporal, pub scope: Scope, pub context: Context }
pub enum CandidatePass { SelfTurn, OtherTurn, OtherFold, EpisodeFold, Carina, Manual, Consolidation, Import }  // serde kebab-case
pub struct MemoryCandidate { content, summary, keywords: Vec<String>, targeting: Option<TargetingTags>, importance: f64,
  kind: MemoryKind, when: Option<String>, entities: Vec<String>, tags: Vec<String>,
  subject: CandidateSubject { holder_id, about_id: Option<String> },
  provenance: CandidateProvenance { source, witnessed_context: Option<…>, chat_id, project_id, source_message_id, anchor_time_ms: f64, pass: CandidatePass } }
pub fn parse_memory_candidate(v: &Value) -> Result<MemoryCandidate, ZodIssues>;   // MemoryCandidateSchema.safeParse twin (defaults: keywords/entities/tags [])
pub fn candidate_from_draft(draft: &MemoryDraftView, subject: CandidateSubject, provenance: CandidateProvenance) -> Option<MemoryCandidate>;
pub fn to_admission_row(c: &MemoryCandidate, resolved: &ResolvedCandidateTime) -> AdmissionRow;
pub fn coerce_importance(raw: Option<&Value>) -> f64;  pub const MEMORY_ENTITY_CAP: usize = 12;

// crates/quilltap-core/src/reinforcement_footnotes.rs
pub const MAX_REINFORCEMENT_FOOTNOTES: usize = 8;
pub fn count_reinforcement_footnotes(content: &str) -> usize;
pub fn append_capped_footnotes(content: &str, details: &[String], cap: usize) -> (String, Vec<String>);

// crates/quilltap-core/src/admission/{mod,types,thresholds,decide,fold,novel_details,outcome,describe}.rs   (PURE)
pub struct AdmissionThresholds { pub near_duplicate: f64, pub reinforce: f64, pub related: f64, pub date_guard_days: f64, pub footnote_cap: usize }
pub const ADMISSION_THRESHOLDS: AdmissionThresholds;        // 0.90 / 0.85 / 0.70 / 7 / 8
pub const ADMISSION_THRESHOLDS_TFIDF: AdmissionThresholds;  // 0.80 / 0.40 / 0.20 / 7 / 8
pub const ADMISSION_TOP_K: usize = 5;
pub fn admission_thresholds_for_provider(provider: Option<&str>) -> &'static AdmissionThresholds;
#[derive(Serialize)] #[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AdmissionAction {
    Insert,
    #[serde(rename_all = "camelCase")] InsertLinked { link_to: Vec<String>, reason: LinkReason },   // "band" | "date-guard" | "mismatch"
    Reinforce { into: String },
    #[serde(rename_all = "camelCase")] Absorb { into: String, #[serde(skip_serializing_if = "Option::is_none")] redirected_from: Option<String> },
    #[serde(rename_all = "camelCase")] PromoteAndReinforce { into: String, with_footnotes: bool },
    Reject { reason: RejectReason },                                                                 // "embedding-failed" | "empty"
}
pub enum AdmissionActionKind { Insert, InsertLinked, Reinforce, Absorb, PromoteAndReinforce, Reject }  // as_str → "insert" | "insert-linked" | "reinforce" | "absorb" | "promote-and-reinforce" | "reject"
pub enum NeighbourExclusion { Subject, Kind, ManualOverAuto, DateGuard }                             // "subject" | "kind" | "manual-over-auto" | "date-guard"
pub enum Verdict { Below, Band, Excluded, Eligible }
pub struct AdmissionNeighbour { pub memory: Value, pub cosine: f64, pub via: Option<String>, pub dangling_digest: bool }
pub struct NeighbourAssessment { pub id: String, pub cosine: f64, pub via: Option<String>, pub verdict: Verdict, pub excluded_by: Option<NeighbourExclusion> }
pub struct AdmissionDecision { pub action: AdmissionAction, pub assessments: Vec<NeighbourAssessment> }
pub struct AdmissionCandidateView<'a> { pub holder_id: &'a str, pub about_id: Option<&'a str>, pub kind: Option<&'a str>, pub source: Option<&'a str>, pub occurred_at: Option<&'a str> }
pub fn occasions_are_distinct(a: Option<&str>, b: Option<&str>, date_guard_days: f64) -> bool;
pub fn decide_admission(candidate: &AdmissionCandidateView, neighbours: &[AdmissionNeighbour], t: &AdmissionThresholds) -> AdmissionDecision;
pub struct FoldCaps { pub footnote_cap: usize, pub entity_cap: usize }  pub const FOLD_CAPS: FoldCaps;
pub struct FoldSource { id, content, occurred_at, narrative_time, entities, related_memory_ids, reinforcement_count: Option<f64>, last_reinforced_at, created_at }
pub struct FoldPlan { survivor_id, footnotes, content, add_links, patch: FoldPatch { occurred_at, narrative_time, entities }, reinforcement_delta: u32, reinforced_at: Option<String>, at_cap: bool, reembed: bool }
pub fn fold_into(survivor: &Value, absorbed: &[FoldSource], caps: &FoldCaps) -> FoldPlan;
pub fn extract_novel_details(candidate: &str, existing: &str) -> Vec<String>;      // moved verbatim from memory_gate.rs:233
pub fn admission_inserted(a: &AdmissionAction) -> bool;  pub fn admission_reinforced(a: &AdmissionAction) -> bool;
pub fn is_near_duplicate_decision(d: &AdmissionDecision, t: &AdmissionThresholds) -> bool;
pub fn describe_admission(o: &AdmissionOutcome, label: &str, summary: &str) -> String;
pub struct AdmissionRow { … }  pub struct AdmissionTrace { … }  pub struct AdmissionOutcome { action, memory: Option<Value>, novel_details, linked_ids, trace }

// crates/quilltap-core/src/services/admission/{mod,admit,neighbours,apply,prepare,links,dry_run_book,rate_ledger,update_memory}.rs   (STATEFUL)
pub struct AdmitOptions<'a> { pub user_id: &'a str, pub dry_run: bool, pub backfill: bool, pub dry_run_book: Option<&'a mut DryRunBook>, pub idempotency_key: Option<String>, pub host_zone_fallback: &'a str }
pub async fn admit_candidate<P: EmbeddingProvider>(db: &Db, provider: &P, c: &MemoryCandidate, resolved: &ResolvedCandidateTime, opts: AdmitOptions<'_>) -> Result<AdmissionOutcome, DbError>;
pub async fn admit_row<P: EmbeddingProvider>(db: &Db, provider: &P, row: AdmissionRow, opts: AdmitOptions<'_>) -> Result<AdmissionOutcome, DbError>;
pub async fn with_admission_lock<T>(character_id: &str, f: impl Future<Output = T>) -> T;   // per-character tokio Mutex map
pub fn admission_content_key(row: &AdmissionRow) -> String;                                  // sha1 over the JSON array
pub async fn gather_neighbours<P>(db, provider, character_id, embedding: &[f32], exclude: Option<&HashSet<String>>, probe: Option<NeighbourProbe>) -> Result<GatheredNeighbours, DbError>;
pub async fn apply_fold<P>(db, provider: Option<&P>, survivor: &Value, plan: &FoldPlan, user_id: Option<&str>) -> Result<Option<Value>, DbError>;
pub async fn update_memory<P>(db, provider, character_id, memory_id, patch: MemoryEditPatch, user_id) -> Result<UpdateMemoryResult, DbError>;
pub fn record_admission_insert(character_id: &str, at_ms: f64);  pub fn count_recent_admissions(character_id: &str, since_ms: f64, now_ms: f64) -> usize;  pub const RATE_LEDGER_WINDOW_MS: f64 = 3_600_000.0;
pub async fn link_both_ways(db, character_id, memory_id, link_to: &[String]) -> Result<Vec<String>, DbError>;
```

⚠ **Crate-API risk:** serde's per-variant `#[serde(rename_all = "camelCase")]`
inside a `#[serde(tag = "kind", rename_all = "kebab-case")]` enum renames the
FIELDS while the outer `rename_all` renames the VARIANT tags — correct on the
serde version in `Cargo.lock`, but pin the JSON with a byte test (memory note
"serde_json Map == ignores key order" — compare `to_string`). The field ORDER
on the wire is v4's object-literal order (`kind` first, then the variant's
fields in declaration order).

### 7.4 Wire changes (`api/types.rs`)

- `Request::MemoryCreate { memory }` (`api/types.rs:1663-1665`) — doc to read "`admitCandidate`; 201 `{memory, action}` on insert / insert-linked, else 200". The **`Response`** must carry the status split: `Response::Memory(Value)` (`:4562`) has no status today, so either a new `Response::MemoryCreated { body, inserted: bool }` (the web edge maps `inserted` → 201 / 200) or the API lane's established status mechanism. Body `{ "memory": <row without embedding>, "action": "<kind>" }`. The reject mapping (bug 236): `empty` → `Response::error(ErrorKind::BadRequest, "Memory content and summary must not be blank.")`; `embedding-failed` → 502 `The memory was not saved because its text could not be embedded. Check the configured embedding profile.` (v5 today: 500 `Failed to generate embedding for memory — no row was created. Check the configured embedding profile.`, `api/memories.rs:541-545`); any other memory-less outcome → 500 `The memory was not saved.` The body's `source` / `skipGate` are IGNORED (DEBUG `[Memories API] Ignoring client-supplied source/skipGate on manual create`); `projectId` / `kind` / `occurredAt` / `entities` accepted; `Character` / `Project` / `Chat` 404s. (API lane — named here so it lands on the same `AdmissionActionKind::as_str`.)
- `Request::MemoryUpdate` (`:1667-1672`) — "re-embeds on a text change; 502 when the edit cannot be embedded; 400 on invalid `relatedMemoryIds`" (API lane, via `update_memory`).
- The SPA contract `apps/web/src/app/core/core-contract.ts:5936` `MemoryCreateRequest` — the response type gains `action`.

---

## 8. SPA / help / docs / CLI touch points

- **SPA:** `apps/web/src/app/memory/memory-format.ts:4-27` — the 0.4 Medium boundary → `importance_band` at 0.3 (v4 `components/memory/memory-card.tsx:52-59`, `memory-editor.tsx:100-101` band RAW `memory.importance`); the create toast `Already on file — the existing memory has been reinforced instead` on a 200 (v4 `memory-editor.tsx` span hunk; `apps/web/src/app/memory/memory.api.ts:80`); the editor no longer sends `source: 'MANUAL'`.
- **help/:** `characters.md`, `episodic-memory.md` (Phase C), `memory-recall-relevance.md`, `chat-settings.md`, `cli-memories.md` (Phase A / bugs) — the help lane re-vendors the whole tree at the pin.
- **docs/v4/:** `features/complete/memory-system-refactor.md` (§3.2 the candidate, §3.4 scoring, §3.6 admission, locked decisions 6 and 10), `features/builtin-tfidf-memory-tuning.md`, `bugs/fixed/bug-200…`, `-212`, `-216`, `-218`, `-226`, `-231`, `-234`, `-236`, `-237`.
- **v5 CLAUDE.md** — "Running the differential harness" (the example commands run `memory-weighting.ts` / `ranking-blend.ts`, both IMPORT-DEAD at the pin) must be re-pointed when this lane replaces them.
- **CLI:** `quilltap anchor-probe` (`crates/quilltap-cli/src/anchor_probe_cmd.rs:385-398`) — v4 relabelled `reinforce ≥ X` → `would fold`, `near-dup ≥ X` → `near-dup absorbed`, `Crossed only without anchors` → `Fold only without anchors`, the legend `= would fold into an older row only without the anchor line (reinforce ≥ X)`, and the `lexicalProfile` note (`4ae0c4ca6`) — the probe owner's.

---

## 9. Open points for the order (observations / questions — NOT rulings)

- **O1 — Splitting.** Scoring + time + candidate are pure tier-1 and can land as one keystone-adjacent lane FIRST (the keystone's two data-rewrite ensures import the formula and the calendar; recall and extraction import scoring / time / candidate). Admission (A1–A20) is the larger stateful lane and depends on the index-store facade (Phase B) and the repository delta writes; `update_memory` could ride with the API lane. Suggest: lane **S/T/C** (pure) ∥ lane **ADMISSION** (after the index facade), with `fold_into` in the admission lane because maintenance consumes it.
- **O2 — The `MemoryCandidate` name collision.** v5's `memory_tasks.rs:149` `MemoryCandidate` is v4's OLD type (now `MemoryDraft`). Rename it to `MemoryDraft` in the extraction lane (one source census) and give the new Zod type the v4 name — or spell the new one `AdmissionCandidate`. A planner ruling, because two lanes touch the name.
- **O3 — The lock and the idempotency ledger under v5's runtime.** v4 needs the per-character lock because search → decide → write spans awaits; v5's admission spans awaits too (embedding, read pool, `Db::write`), and the job runner retries (`services/job_runner.rs` `max_attempts 3`) with immediate (non-batched) writes — so a retried extraction job re-admits and DOUBLE-REINFORCES exactly as v4's ledger exists to prevent. Port both (a `tokio::sync::Mutex` map; a job-scoped occurrence counter — e.g. a `tokio::task_local!` `JobScope { job_id, occurrences }` set by the runner around a handler — minting `jobId:contentKey:n`)? Or rule the ledger a Node-batching artifact? The semantics are observable (test "answers a repeated job-attempt key from the ledger and writes nothing").
- **O4 — `trackActivity('memory')` + `publishRealtime` placement.** v5 `track_activity(ActivityKind::Memory, …)` exists; the NEW `memories` publish site at admission needs a census row and a behavioural pin (A16) — and the API create route must NOT add a second one.
- **O5 — The memory calendar's fallback.** v4: instance `chat_settings.timezone` → `QUILLTAP_TIMEZONE` → system zone. v5's core never reads env / the process zone (`host_zone.rs` census). Proposal: the host passes `server_tz` (already `QUILLTAP_TIMEZONE`/`TZ`-reconciled) as `host_fallback`; core reads the instance zone from `chat_settings`. Under a POSIX `TZ` rule `server_tz` is `"UTC"` (the recorded residue) where v4 reads the system zone — record or rule. Also: keep v4's 5-minute TTL cache (a process-global) or read fresh per admission (one cheap SELECT)? The TTL is observable only across a Salon timezone change within 5 minutes.
- **O6 — Zone validity.** v4 `isValidTimeZone` = "`Intl.DateTimeFormat` accepts it" (ICU: case-insensitive, legacy links like `US/Central`, `EST5EDT`); v5 would use `jiff::tz::TimeZone::get` (tzdb names; case-insensitive lookup). A stored value accepted by one and refused by the other picks a different calendar. Measure against the bundled tzdb before ruling.
- **O7 — DST gaps / overlaps.** v4's `zonedTimeToUtcMs` two-pass result in a gap (e.g. `America/Santiago` midnight) and an overlap differs from jiff's "compatible" disambiguation; v5's `day_references.rs` uses jiff civil math today. Port the two-pass algorithm literally (with a second-truncated offset) and pin it with gap / overlap rows in the tier-1 corpus (§5.2 item 3).
- **O8 — Where `rank.rs` lives.** It imports `recall/tags` (`combineRecallMultipliers`, `RecallContext`) and `recall/types` (`RecallCandidate`); v5's twins are `recall_tags.rs` + `services/memory_service.rs`. Either the recall lane owns `rank_for_recall` (recommended — it is a recall stage) or the scoring lane waits on the recall lane's types.
- **O9 — The 500 ms embedding retry delay.** v5's gate deliberately omits it (`services/memory_gate.rs:36-38`, "a host-timing concern with no DB-state effect"). Keep the recorded omission or add a `tokio::time::sleep` (admission is now async in the parent anyway)? No differential can see it; a provider rate limit can.
- **O10 — The SQL `reinforcedImportance` column filters** (§0.1 item 1): v4's two SQL filters EXCLUDE a NULL column where the in-memory filters fall back. After `recompute-reinforced-importance-v1` no row is NULL on a migrated instance, but a v5-written row via an old import path could be. Port faithfully (recommended) and note it.
- **O11 — Tier-3 loud refusals / deferrals.** Candidates: the job-child plumbing (`admitPrepared`'s child refusal, `patchChildIndex`, host-RPC `admitMemory` / `countRecentAdmissions`, `applyFold`'s child refusal, the IPC `memory-index-patch`) — NO-PORT (v5 has no child), recorded, not a refusal. `CandidatePass::{Consolidation, Import}` have no producer at the pin — port the enum values (Zod would refuse unknown passes) without inventing producers. The `consolidation` / `import` passes are YAGNI-safe.
- **O12 — `insertRow`'s `aboutCharacterId || null` after a name-presence flip** stores the HOLDER's id (not NULL) — v4's shape since pre; the decide reads both as the same subject (`subjectOf`). Keep; a census of readers that treat "about = holder" vs "about = NULL" differently may be owed.
- **O13 — The anchor-line entity cap vs `reembed`.** `foldInto` re-embeds when `entities` grew even if the new entity is the 5th+ (the anchor line shows ≤ 4) — a provider call that changes nothing. Faithful port (recommended); recorded so a reviewer does not "fix" it.
- **O14 — Live-data hazard (4).** Until this lane lands, every v5 reinforce writes `log2(count + 1)` onto rows v4's migration rewrote to `log2(max(1, count))` (ledger §1 hazard 4). The recompute ensure (keystone) and S1 must land in the SAME round, and the ensure must be gated once-only over `migrations_state` (the brief's ruling) — or a v5 build that ran S1 without the ensure would leave pre-migration rows on the old scale.
- **O15 — Size.** v5 deletes ~3,200 lines (`services/memory_gate.rs` 2,562 + `memory_merge.rs` 307 + `memory_weighting.rs` parts + `day_references.rs` 551 rewrite) and adds ~2,300 (v4's admission is 2,251 TS lines; scoring 518; time 798; candidate 280). The admission tests alone (§6: ~100 jest cases) are a lane's worth; the tier-3 family (§5.2 item 8) needs a fixture with `memories.embedding` — i.e. after Phase B's builder rebuild.
