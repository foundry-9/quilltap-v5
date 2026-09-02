# Quilltap Changelog

Newest first. Each entry is one commit: the header carries the commit date
and the commit subject; the line beneath it records the crate versions that
commit bumped (or notes a docs-only change). Entries up to 2026-08-18 were
restructured mechanically from git history and also carry the short commit
hash; new entries omit the hash (it doesn't exist when the entry is
written — see `.claude/commands/commit.md` §7 for the format). Older months
are archived under `docs/changelog/`.

Archived months: [July 2026 (days 16–end)](changelog/2026-07b.md), [July 2026 (days 1–15)](changelog/2026-07a.md), [June 2026](changelog/2026-06.md).

## September 2026

#### 2026-09-02 — chore(harness): drop the superseded background-mode row masker (P4.D145)

_Versions: harness 0.0.634._

`mask_background_mode_row` was left behind when the P4.D146 tripwire was
restructured to key its mask off the affected file ids rather than per-row
content. Dead since that rewrite; `mask_background_mode_table` is the live one.
Caught by the gate's own `dead_code` warning, which `clippy -D warnings` would
have failed on.

#### 2026-09-02 — docs(porting): the P4.D145 lane record + the DDL.md mirror refresh

_Docs-only; no version bumps._

The bug-114 lane record appended to `status-log.md`: what landed unit by unit,
the two refuted order premises (the provisioning call reddens
`provisioning_equivalence`; the pre-planted-duplicate routes arm is unreachable
once the index exists), the measured-absent §C sibling-drift risk on the image
tier-3 families and the measured-present one on the restore family, the §10
read-only Friday-copy measurement, the NO-COUNTERPART rows, every fixture
changed with the siblings re-run, the mutation proofs, and the regen recipes.

`docs/v4/developer/DDL.md` refreshed from the pin. The diff is 50 insertions,
not the six this commit added — the mirror was also stale for six earlier
absorbed rounds. `70505745a` (P4.D146's row, an ancestor of this lane's pin)
does not touch the file, so no sibling-lane content rode along. v4's CLAUDE.md
chokepoint line has no mirror row to update: `docs/v4/` mirrors the `docs/` tree
only.

#### 2026-09-02 — fix(restore): drop a pre-collapse backup's duplicate folder rows quietly (v4 bug 114, P4.D145 unit 5)

_Versions: core 0.0.741, harness 0.0.633._

Ports v4 `a5df98b3f`'s restore arm. Restore keeps `create` — ids must be
preserved, which the chokepoint cannot do — and gains one branch ahead of the
standard warning: a backup taken before `collapse-duplicate-folders-v1` ran can
carry many rows for one `(userId, projectId, path)`, the unique index rejects
the extras, and the first one restored is the survivor. No warning, no skipped
counter, `foldersRestored` simply not incremented, which is why `warn_row!`
cannot express it and this one site is written out.

A NEW committed archive, `restore-archive-duplicate-folders.zip`, carries six
folder rows where all eleven existing archives carry exactly one (measured, so
the arm was structurally invisible — the P4.D31 lesson). Three survive (the
first `/notes`, a different path, and the same path inside a project — a
`projectId`-blind index would drop that one), and three are dropped: two UNIQUE
and one PRIMARY KEY, because v4's predicate names the whole constraint family.
Measured against v4's real restore: `foldersRestored` 3, warnings unchanged.
Reverting the arm reddens the case's warnings comparison.

Both sides materialize the index on the fresh target before restoring — v4 by
running its REAL migration, v5 by calling the boot ensure — because a
generateDDL target is pre-index by construction and the arm would otherwise be
silently unreachable. It also models reality: both apps boot before anyone
restores into them.

**A named tripwire rides with it.** This lane's pin (`a5df98b3f`) has P4.D146's
`70505745a` as an ancestor, so v4's restore writes a project's
`backgroundDisplayMode` as the coerced `theme` while v5, which has not ported
that commit, writes the stored `project`. Measured: 46 differences, every one of
them that value or something derived from it (the content, its two lengths, its
sha). `BACKGROUND_MODE_PENDING_P4D146` masks exactly those, keyed off the file
ids of the documents whose two sides disagree — after asserting the disagreement
is still v5 `project` vs v4 `theme`, and refusing to mask anything else. It
fails loudly if it ever masks nothing, so it retires itself when P4.D146 lands.

Riding the fix: the four comparison branches (plain diff, phase-order residual,
#58 orphan assertion) now read one masked view of each side instead of three
different ones — the first two took the raw rows, where a carve-out silently did
not apply.

The order's §10 measurement, run read-only on the dogfood copy via a new
`p4d145_folder_population` example: **607 rows describing 24 identities, 583
duplicates, index absent** — v4's own 2026-09-02 number to the row. Every
duplicate is under `/story-backgrounds/` or `/character-avatars/`; no
hand-created folder duplicated.

#### 2026-09-02 — refactor(files): every folder writer goes through ensure_by_path (v4 bug 114, P4.D145 unit 4)

_Versions: core 0.0.740, harness 0.0.632, web 0.0.102._

The five v5 folder-create sites v4 `a5df98b3f` converted now write through the
chokepoint: both image handlers' legacy-folder ensure (and with them the two
private `find_folder_by_path` copies v5 carried, deleted), the folders route's
recursive parent chain, `files_folder_create`'s create branch, and the .qtap
importer. The idempotent 200 `alreadyExists: true` arm and the importer's reuse
branch both stay — they are reporting, not the uniqueness guarantee. Because the
chokepoint returns the persisted row, `files_folder_create`'s post-create
re-read and its "folder vanished after create" arm are gone; the wire shape and
statuses are unchanged.

Three v4 sites have no v5 counterpart and are recorded rather than converted:
the file-storage watcher (v5 ships none), the forked child's buffered-write
override (v5's job runner is in-process; the routing half is pinned by
`write_partition_equivalence`'s `folders.ensureByPath` classify row), and the
migration-runner progress label.

**Measured: the cutover is invisible to every differential.** `create` and
`ensure_by_path` differ only under a race, or when the read and the index
disagree — neither reachable from a sequential op list — so reverting a call
site to `create` is green across `qtap_import_equivalence`,
`files_routes_equivalence` and both image tier-3 families. The census in
`folders_chokepoint_wiring_guard` (renamed and widened from unit 3's boot guard)
therefore holds all five sites by name, mutation-proven, alongside the boot
wiring and the no-counterpart rows.

`folders_remap_tier2_equivalence` gains the four `ensureByPath` arms over v4's
REAL repository — existing returns the same row without inserting, absent
creates, an ABSENT `projectId` key normalizes to SQL NULL on both the read and
the write, and the constraint arm re-raises with nothing written. Its fixture
gains v4's unique index (generateDDL cannot express a COALESCE index, so the
fixture was pre-index by construction and that arm unreachable) plus a seeded
row whose `projectId` is the empty string — the one shape where `findByPath` and
the index disagree, which is what makes the conflict reachable with no race. It
is seeded with raw SQL because v4's `FolderSchema.projectId` is UUID-validated
and refuses `''` (measured at the pin).

`files-main.db` / `files-mount.db` were regenerated at the pin to carry the same
index — the post-bug-114 vintage every real instance will be in — and
`files_routes_equivalence` gains a same-path-inside-a-project create arm proving
the general row does not shadow it. The three sibling families that read those
fixtures were re-run green by name.

#### 2026-09-02 — feat(db): collapse duplicate folder rows and add the unique path index at boot (v4 bug 114, P4.D145 unit 3)

_Versions: core 0.0.739, harness 0.0.631, host 0.0.92._

Ports v4 `a5df98b3f`'s `collapse-duplicate-folders-v1` migration as a v5 boot
repair. Groups `folders` rows by `(userId, COALESCE(projectId, ''), path)` with
the oldest surviving (`ORDER BY createdAt ASC, id ASC`), repoints every
`parentFolderId` naming a discarded row at its group's survivor, deletes the
rest, and creates the unique index. Wired into the boot repair chain beside the
P4.D140 fence, on the main partition.

The guard is the INDEX, not the `migrations_state` ledger — v4's own
`shouldRun()` is `!indexExists()` and never reads the ledger, so the index is
the cross-app once-only marker in both directions. v5 therefore writes no ledger
row, and the differential asserts that as a measured divergence: v4's runner
does write one (informational for this migration), while the created index is
proven byte-identical, which is what actually makes a later v4 boot skip.

New `folders_collapse_heal_equivalence` drives v4's REAL migration plus its REAL
ledger write over ten shared scenarios — v4's own five test cases, four widened
arms (a clean instance, an empty table, the createdAt tie-break, a second user),
and the real instance's measured shape (607 rows describing 24 folders, v4's
2026-09-02 measurement). The diff covers the whole post-pass table, the index
SQL byte-for-byte, the `MigrationResult` message, the forced second run, and
post-pass insert probes covering both what the index now rejects (including the
coalesced-NULL arm) and the different-project sibling it must still allow. Five
mutation proofs, each reddening exactly one arm.

**An order premise was refuted by measurement.** The order proposed calling the
ensure from `services/provisioning` as well. v4's `generateDDL` cannot express a
COALESCE index, so `provisioning_equivalence` compares v5's provisioned
`sqlite_master` against a v4 fresh dump that does not carry it — creating it
there reddens that family on `schema mismatch in partition main` (measured at
the pin, then reverted). It is also unnecessary: `Host::assemble` runs the boot
chain on every open, including the first one after Setup. A new
`folders_index_boot_wiring_guard` holds both halves — the boot call must exist,
provisioning must not create the index — since no differential can see a
deleted call site.

#### 2026-09-02 — feat(db): FoldersRepository::ensure_by_path, the find-or-create chokepoint (v4 bug 114, P4.D145 unit 2)

_Versions: core 0.0.738._

Ports v4 `a5df98b3f`'s `FoldersRepository.ensureByPath` — the only sanctioned
way to bring a folder row into being for a path that may already have one. Reads
by path, returns an existing row, otherwise creates; a unique-constraint
violation re-reads and resolves to the winning row, a conflict that resolves to
nothing re-raises the original error rather than answering with a folder that
does not exist, and a non-constraint failure propagates untouched.

`create` gains a private `create_returning` twin so the chokepoint can hand back
the persisted `FolderRow` (v4's `create` resolves to the created `Folder`)
without a second query or a second insert path.

Nine unit tests carry v4's six-case spy suite onto a REAL `folders` table.
Two arms needed a v5-specific mechanism because a repository cannot be spied:
the lost-race arm drives a `before_insert` seam that plants the winner in the
one instant a competing writer could commit in, and the non-constraint arm
becomes an error-IDENTITY assertion (the seam reshapes the table so the INSERT
and a hypothetical recovery re-read fail with different SQLite sentences),
standing in for v4's "findByPath called ONCE". Measured on the way: with the
index present, dropping the read-first branch still converges — so the
existing-row case is pinned WITHOUT the index, where the early return is the
only thing that can prevent a duplicate.

The header records what v5 never had: v4's amplifier was a soft-failing read
(`findByPath` returning its `safeQuery` null), and v5's `find_by_path`
validates nothing and propagates every error but `QueryReturnedNoRows`. The
duplicates v5 meets are v4-written.

#### 2026-09-02 — feat(db): the SQLite unique-constraint predicate (v4 bug 114, P4.D145 unit 1)

_Versions: core 0.0.737._

The port of v4 `a5df98b3f`'s new `lib/database/sqlite-errors.ts`. A new
`db::sqlite_errors` module answers whether a failure is a SQLite constraint
violation — the structured driver code first (rusqlite folds every
`SQLITE_CONSTRAINT_*` extended code onto `ErrorCode::ConstraintViolation`, so
the set matches v4's `code.startsWith('SQLITE_CONSTRAINT')` exactly), then the
`/UNIQUE constraint failed/i` message a wrapped or re-thrown error carries.
Seven unit tests pin it against REAL driver errors from a table carrying the
`(userId, COALESCE(projectId, ''), path)` index bug 114 adds, including the
coalesced-NULL arm and a primary-key violation.

v4 also had its background-job write applier stop keeping a second copy of the
predicate and re-export the shared one. v5 cannot re-export outright — the
applier half classifies a replayed JSON error shape, not a live
`rusqlite::Error` — so the one sentence they genuinely share, the message test,
now reads from `db::sqlite_errors::message_names_unique_constraint`.

The `write-partition` oracle case gains v4's two folder classify rows
(`folders.create` and the new `folders.ensureByPath`), driving v4's REAL
`classifyWriteTarget`: both answer `main`, so v5's default-to-Main routing
already covers the chokepoint's non-conforming method name — the assertion v4
added to its own suite, landed as a differential row instead.
#### 2026-09-02 — docs(porting): the P4.D146 lane gate record

_Docs-only; no version bumps._

The lane's verification gate appended to the P4.D146 record: the workspace
numbers, the by-name sweep-driver run from the `70505745a` pin, the
changed-bytes greps, and the cross-lane `/tmp` fixture collision the first full
run exposed (the sibling P4.D145 lane rebuilt the shared
`/tmp/qt-story-{main,mount}.db` from its own worktree's builder, so this lane
was diffing a five-mount-point fixture against a six-mount-point oracle).

#### 2026-09-02 — fix(prospero): drop the two retired background modes from the project card (P4.D146 unit 4)

_Versions: SPA 0.5.617._

v4 `70505745a`, the client half. "Project-generated background" and "Static
uploaded image" leave the Story Backgrounds select along with their hint
sentences and their `modeLabels` toast entries; the two `backgroundDisplayMode`
unions in `core-contract.ts` narrow to `'latest_chat' | 'theme'`. The server's
update schema now refuses both values, so a stale option would have been a
control that could only 400.

Specs: a new pin on the select's exact two options and labels, and one on each
surviving hint sentence. The two existing specs that drove a retired value moved
to a surviving one. An ACTIVATE-AT-UNIFY e2e beat in
`workspace-project-backdrop-flow` asserts the same two options in a real
browser; it is authored gated because this lane does not own Playwright for the
round.

#### 2026-09-02 — fix(projects): narrow the background display mode to latest_chat | theme (P4.D146 unit 3)

_Versions: core 0.0.739, harness 0.0.632._

v4 `70505745a`. Two of the four project background display modes never worked:
"Project-generated background" read a field only the Latest chat path ever
wrote (there is no project-background generator), and "Static uploaded image"
read a field nothing writes, with no upload control and no acceptance in the
update schema. The enum is now `latest_chat | theme`, and the background GET's
two resolution branches are deleted outright.

Projects stored in a retired mode coerce to `theme` rather than failing.
`normalize_background_display_mode` lands beside `ProjectProperties` and is
applied inside `ProjectEntity::parse_properties` — the one chokepoint the
overlay read, the write overlay's read-modify-write, and `write_managed_fields`
on create all pass through — so a pre-4.9 `.qtap` import or backup restore also
lands on a valid value with no change to the restore orchestrator. The absent
key is still left to the schema default; an explicit `null` still refuses, as
it does in v4 (measured, not assumed: v4's `.default()` short-circuits only on
`undefined`, so `null` reaches the preprocess and then fails the enum). The
update schema refuses both retired values outright — the coercion is for values
already on disk, not a licence to write a new one.

New tier-1 family `project_background_display_mode_equivalence` over v4's real
module, one row per assertion in v4's own test plus the shapes it does not
state. The projects tier-2 corpus gains a planted-retired-mode project for the
read side; `projects_routes_equivalence` gains the two refusals and a surviving
mode; the restore-state family already carried a `project`-bearing archive and
proves the create path coerces.

#### 2026-09-02 — fix(images): exclude absent participants from the story-background back-fill (P4.D146 unit 2)

_Versions: core 0.0.738._

v4 `70505745a`, step 9b. The back-fill scans the finished prompt for named
workspace characters and appends a canonical appearance so the provider does
not invent one. Its candidate pool was "every user character not in
`payload.characterIds`" — so the moment unit 1 stopped putting absent
participants in the payload, they landed squarely in that pool instead: a
crafter that picked the name out of the transcript would have been handed a
portrait, restoring by the side door the figure the filter had just removed.
The exclusion set is now the payload UNION the chat's not-present participants.
A character absent here can still be enumerated when genuinely unaffiliated
with the chat, which is what the scan is for.

The `/tmp`-built story-background fixture gains a third character (Bram) and a
`backfill_absent` chat: Fern present and carrying the payload, Bram an absent
participant carrying nothing, Zelda unaffiliated. The crafted prompt names all
three, so the recorded image key says which the scan appended — Zelda only.

#### 2026-09-02 — fix(images): keep absent characters out of story-background enqueues (P4.D146 unit 1)

_Versions: core 0.0.737, harness 0.0.631._

v4 `70505745a`, both enqueue sites. The story-background prompt crafter is
told to place every enumerated character as a figure in the frame, so a
participant marked Absent — or soft-removed from the chat — was being painted
back into a room they had walked out of. Both sites now filter on
`isParticipantPresent`: `active` and `silent` are present (silent characters
are standing there, just not speaking), `absent` and `removed` are not. With
nobody present, the auto-trigger enqueues nothing and the manual
`?action=regenerate-background` route answers the reworded 400 (`No characters
present in chat to generate background for.`).

The str-to-status step the JSON-reading sites need lands once in
`chat_predicates` as `participant_status_from_str` +
`json_participant_is_present`, matching v4's Zod `.default('active')` for an
absent key and reading an unrecognised status as not-present.

The committed `cost-background-{main,mount}.db` fixture pair was widened for
this: it seeded every participant `active`, so neither differential could
discriminate the filter at all. It now carries two more characters, a chat
with one participant per status, and a chat where everyone has left.
`cost_background_routes_equivalence` gains two arms and
`title_update_tier3_equivalence` two cases, all four measured against v4's
real code at the `70505745a` pin.
#### 2026-09-02 — docs(concierge): the P4.D143 lane gate record

_Versions: harness 0.0.636._

Records the lane's gate in `status-log.md` and fixes the one thing clippy caught
in it: `clippy::doc_lazy_continuation` on the §D tripwire's doc comment, where a
paragraph followed a list item without a blank line.

Gate: fmt clean; clippy both feature sets exit 0; `cargo test --workspace` 481
binaries / 2,668 passed / 0 failed / 1 ignored with the lane's env block, zero
SKIP; the nine families by name through the sweep driver at the `c43d3b1b4` pin,
9 ok. `provisioning_equivalence` is 3/3 at that pin, which measures what
P4.D145's survey predicted: v4's new folders UNIQUE index does not reach the
fresh-instance surface, because `generateDDL` cannot emit its `COALESCE(...)`.

#### 2026-09-02 — test(concierge): v4's own trigger corpus becomes a differential

_Versions: core 0.0.742, harness 0.0.635._

The new `danger_trigger_equivalence` drives v4's REAL
`triggerChatDangerClassification` over mocked repos (the DB-free
route-guard-oracle idiom — no fixture, three `jest.doMock`s) and diffs the
enqueue calls against v5's, which are read back out of `background_jobs`. v4's
own `chat-danger-trigger.test.ts` is the corpus case for case, plus the two
operator arms `c43d3b1b4` added and an empty-string context summary.

Until now the gate chain was only ever inferred from the tier-3 spine dumps,
which exercise one or two gates per op; here each gate has its own case.
Reverting the on-duty guard reddens exactly the two operator arms whose label is
`false` — the third, whose preserved label is `true`, is caught by the sticky
check instead, which is the corpus showing its gates are independent.

Two of v4's observables are recorded and NOT compared, because they have no v5
counterpart: `chatSettingsLookedUp` and the `settings_lookup_throws` case both
depend on v4 resolving the danger mode inside the function, where v5's callers
compute it first. `trigger_chat_danger_classification` becomes `pub` so the
family can drive it.

#### 2026-09-02 — feat(concierge): the Quick-hide probe, on the uncensored route

_Versions: core 0.0.741, harness 0.0.634, web 0.0.102._

§H of the round's shared contract. v5 never had v4's `GET /api/v1/chats?action=
has-dangerous` at all, and `c43d3b1b4` re-based it off the raw `isDangerousChat`
onto the uncensored route: the "Dangerous Chats" toggle hides Flagged (the
Concierge's verdict) and Uncensored (the operator's), so the affordance appears
on exactly that set and not on every chat carrying a preserved label.

Landed as the `chatsHasDangerous` verb answering `{hasDangerous: boolean}`, plus
a new `chats_routes.rs` serving v4's whole collection GET dispatcher: the action,
v4's exact unknown-action 400, and the no-action list leg delegated to the
existing `ListChats` verb (v4 serves the list at that URL, so refusing it would
have been an invention).

`salon_reads_equivalence` gains four arms over v4's REAL GET dispatcher — none,
vouched-only (a preserved TRUE label that must NOT count), flagged, uncensored —
plus v4's recorded 400 bytes; the new `chats_collection_route` wire test proves
the edge is registered, unwraps its response variant, and builds the same
sentence. Reverting the probe to `isDangerousChat === true` reddens the vouched
and uncensored arms; asking `shouldShowDangerStyling` instead reddens the
uncensored one.

P4.D144 consumes the verb as the third arm of `hasQuickHideFeatures`.

#### 2026-09-02 — feat(concierge): the character conversations and project chats rows carry the derived state

_Versions: core 0.0.740, harness 0.0.633._

The last two of v4 `c43d3b1b4`'s list payloads. `characters/{id}?action=chats`
and `projects/{id}?action=chats` drop `isDangerousChat` for `conciergeState` +
`dangerCategories` in the same slot; both envelopes (`{chats, total}` and
`{chats, pagination}`) are unchanged.

Both oracle cases gained per-case Concierge mutations rather than a fixture
regen: characters gets a `setConcierge` UPDATE in the `setImpersonation` idiom
(three new cases over the single seeded chat — the fixture is read by eight
families, and none of them re-runs on this account), projects reuses the
`list_chats_activity_fallback` mutation pattern for two more cases.

`projects_routes_equivalence` also gains `BACKGROUND_MODE_PENDING_P4D146`, a
named §D tripwire. This lane's pin has `70505745a` as an ancestor, and that
commit — P4.D146's row — retires the `'project'` background display mode, so
five of this family's cases move against a v5 that has not ported it. The
divergence was measured first and is masked by name in exactly its measured
shape (`backgroundDisplayMode` / `displayMode` `"project"` → `"theme"` in four
project-read payloads, plus `backgroundUrl` url → null on `get-background`); any
other pair falls through and reddens the family. The unifier deletes the
function once P4.D146 is on the same branch and the oracle is regenerated at the
new baseline.

#### 2026-09-02 — feat(concierge): the Salon list and home dashboard carry the derived state, not the raw label

_Versions: core 0.0.739, harness 0.0.632._

Ports the first two of v4 `c43d3b1b4`'s five list payloads. `EnrichedChatSummary`
drops BOTH `isDangerousChat` and `conciergeOverride` for `conciergeState`
(derived through `getConciergeState`) plus `dangerCategories`, in the same slot;
`RecentChat` drops `isDangerousChat` for the same pair as a straight
pass-through from the summary — no second derivation, no second `?? []`. Key
order is otherwise untouched, which is what keeps the two key-order pins green.

The single-chat GET is deliberately unchanged: v4's detail view still needs the
raw trio for the sidebar control, and `salon_reads_equivalence` now asserts that
in its own right.

`salon-reads` gained a per-case `setConcierge` mutation (the `setImpersonation`
idiom) so the committed three-chat fixture can be painted into three distinct
states without a regen: Vouched over a TRUE label, Uncensored over a FALSE one,
Flagged with categories — the label set the wrong way round on both operator
rows, so a payload that leaked `isDangerousChat` would be visibly wrong rather
than accidentally right. The home fixture builder learned `conciergeOverride`
and `dangerCategories`, `home-web.json` seeds all four states, and the committed
`home-{main,mount}.db` pair was rebuilt from the pin.

#### 2026-09-02 — fix(concierge): stop an Uncensored chat enqueueing a doomed classification every turn

_Versions: core 0.0.738._

Ports the trigger half of v4 `c43d3b1b4`. `trigger_chat_danger_classification`
now asks `is_classifier_on_duty` immediately after the chat read and before the
sticky-label check: once the operator has spoken — Vouched Safe or Uncensored —
the classifier is off the case and the handler would discard the job at its own
guard anyway.

v5 measurably had the bug, proven red-first in two families. Vouched Safe was
fine by accident in production (the resolver collapses it to OFF before the
call), but Uncensored resolves to AUTO_ROUTE on purpose and its preserved label
is usually false, so every turn enqueued a `CHAT_DANGER_CLASSIFICATION` job that
was immediately thrown away — the behaviour the 2026-08-27 dogfood pass saw as
"completed six times in four minutes".

`message-finalizer-tier3` gains two chats and two calls (an `UNCENSORED` and an
`OFF` chat, each with a context summary and the label `false`), and
`orchestrator-tier3` gains `danger_uncensored_no_enqueue` — the end-to-end arm,
where the REAL resolver hands the finalizer `dangerMode: AUTO_ROUTE` and the new
guard is the only thing that stops the enqueue. Reverting the guard puts a
`CHAT_DANGER_CLASSIFICATION` row for that chat in both dumps.

#### 2026-09-02 — feat(concierge): name the uncensored row once, in a state-only predicate

_Versions: core 0.0.737, harness 0.0.631._

Ports the predicate half of v4 `c43d3b1b4` (PR #46). `concierge_state_uses_
uncensored_route(state)` is new — the state-only twin of `should_use_uncensored_
route`, for callers that already hold a derived `ConciergeState` (the chat-list
payloads this lane goes on to change carry `conciergeState`, not the raw
`isDangerousChat` / `conciergeOverride` pair) and would otherwise have to
fabricate a chat-like to ask the question. It is now THE one place naming which
states take the uncensored route; `should_use_uncensored_route` delegates to it.

No behaviour change: the disjunction moved, it did not change. The differential
proves that directly — inlining it back leaves `danger_resolver_equivalence`
green, so the new function is pinned by its own arms rather than by the old
ones. `harness/oracle/cases/danger-resolver.ts` gained v4's `it.each(TABLE)`
agreement claim on every override row (`stateUsesUncensoredRoute`, driven
through `getConciergeState(chat)`) plus four `stateRoute` rows that drive the
twin on each literal state with no chat anywhere, and the Rust family asserts
both. A shape guard fails the run if a stale oracle carries fewer than the four
`stateRoute` rows.

#### 2026-09-02 — docs(orders): the `6d2a50382` drift catch-up round — five work orders (P4.D143–P4.D147)

_Docs-only; no version bumps._

`/setupphase` over the ledger's six unprocessed rows. Five orders written
under `docs/developer/porting/work-orders/`, all six ledger rows marked
`ORDERED(...)`, and a round section added to `phase-4.md`. The Concierge
list-marks commit (`c43d3b1b4`) splits server (P4.D143 — the derived
`conciergeState`/`dangerCategories` list pair, the predicate delegation,
the per-turn enqueue guard, the `has-dangerous` probe) from SPA (P4.D144 —
the presentation table as the one string home, `ConciergeMark` over the
Tooltip, `shouldHideChat`, the CSS) under a binding payload contract; bug
114 (P4.D145), the absent-participants/background-mode commit (P4.D146)
and bug 113 (P4.D147) are one lane each. Three ledger rows gained
ordering-survey corrections: bug 114 is NOT a D23 re-dump row (v4's
`generateDDL` cannot emit an expression index — the index arrives via a
boot ensure, index-presence-guarded, no ledger row), v5's story-background
enqueue twin lives in `image_profile_resolution.rs`, and v5 has no folder
picker at all (bug 113 becomes a build, not a latch fix). Shared contracts
§A–§H are byte-identical across the five orders; P4.D147 owns Playwright.

#### 2026-09-02 — docs(drift): six commits past the baseline — bugs 113/114 and the Concierge list marks

_Docs-only; no version bumps._

`/driftcheck` re-run after the same-day check that recorded one commit: v4 has
landed five more. §1 rewritten (main HEAD `6d2a50382`, v4 4.9.0-dev.113; bugfix
tip `3a76b17df` unmoved; checkout on main and clean; verdict DRIFT PENDING — 6
commits; regen rule stays PIN REQUIRED at `4622411fd`), and five rows appended
to §3.

Four PORT rows, no convergences — v4's bugs 113 and 114 are both v4's own
findings, not this port's filings coming back. `c43d3b1b4` (PR #46) derives the
chat-list mark and Quick-hide from the four-state Concierge status instead of
the raw `isDangerousChat` label, landing on P4.D141, P4.64/P4.65's list
payloads, the quick-hide vertical, the P4.D132 Tooltip primitive and the qt-*
class family, and fixes a per-turn `CHAT_DANGER_CLASSIFICATION` enqueue on
Uncensored chats that the handler discarded. `a5df98b3f` (bug 114) is a D23
re-dump row: a new unique index on `folders (userId, COALESCE(projectId,''),
path)` plus a collapse migration, an `ensureByPath` chokepoint over the six
hand-rolled `findByPath` → `create` sites v5 inherited, and a restore arm.
`a00e18f0d` (bug 113) is the client-only folder-picker latch. `70505745a`
carries forward unprocessed. `f3351d54f` (the plan doc) and `6d2a50382` (a
version bump) are NO-PORT candidates awaiting ratification.

#### 2026-09-02 — docs(dogfood): the round-2 + P4.D138-follow-up walk record

_Docs-only; no version bumps._

Walk doc, findings rows (#108 FIXED, #109 RECORDED), status-log record and the
CLAUDE.md status bullet for the 2026-09-02 pass on the Friday copy. 20 CLAUDE
rows terminal: 18 PASS, one PARTIAL→FAIL(#109), one not attempted with reason.
The round's whole 💸 queue is discharged, including the live HuggingFace LoRA
query and the bug-112 boot recompute in both arms (with a free cross-app proof:
v4 had already written the migration ledger row, so v5's completed-check
honoured it). Four instrument errors are recorded alongside the results.

#### 2026-09-02 — fix(images): the Provider select shows the profile's own provider (dogfood #108)

_Versions: SPA 0.5.616; no crate touched._

Found on the Friday copy during the round-2 dogfood walk: editing a real
NanoGPT image profile showed **Provider: OpenAI** while the same dialog showed
the NanoGPT API key, the `flux-lora` model and a NanoGPT options panel. On that
instance it affected **11 of 14** profiles — every one not on `OPENAI` — and it
is deterministic, not a cold-paint race.

The select's rows come from an `@for` over the async provider list while the
value was bound `[value]="provider()"`; Angular applies the property binding
before the option views exist, so the assignment matches nothing and the browser
settles on row 0, and the binding never re-runs because `provider()` never
changed. v4's React controlled select re-applies `value` on the render that
fills the list, so v4 was never affected. The same file already used the
post-render `afterRenderEffect` assignment for the Model and Size selects, with
a comment describing this exact hazard; the Provider select was missed.

Display-only — `provider()` held the real value throughout, proven by a live
round trip that wrote `NANOGPT` back with the `size` sibling key intact.

Fixed with a third `afterRenderEffect` keyed on `providers()`. Pins: four specs
(a non-first row with the option list asserted present first so "not row 0"
cannot pass vacuously, a middle row, row 0 itself, and a user pick still
winning), mutation-proven — restoring the naive binding reds exactly the two
non-first-row arms. The live `settings-image-lora-flow` beat, which already
re-opens a NanoGPT profile's editor after a full reload, gained the assertion
that was missing.

#### 2026-09-02 — docs(drift): record 70505745a — absent characters out of story backgrounds

_Docs-only; no version bumps._

The `/dogfood` freshness probe found the ledger stale: v4 landed one commit
past the baseline. `70505745a` "fix(images): keep absent characters out of
story backgrounds" (v4 `4.9.0-dev.110`) is a **PORT** row on two
already-ported families. (a) Both story-background enqueue sites now filter
participants on `isParticipantPresent` — an Absent or soft-removed character
was being painted into the frame — and the prompt back-fill's candidate pool
excludes them too, so a crafter that picked the name out of the transcript
can no longer restore the figure by the side door; the empty case answers
"No characters **present** in chat…". (b) The project background display
enum narrows to `latest_chat | theme`: `'project'` and `'static'` never
worked (one read a field only the latest-chat path writes, the other a field
nothing writes), and stored rows coerce to `'theme'` through a new
`normalizeBackgroundDisplayMode` preprocessor rather than failing the
`.parse` every project read performs. Not a convergence — v4's bugs doc is
untouched. Ledger §1 rewritten (verdict DRIFT PENDING — 1 commit; **regen
rule flips to PIN REQUIRED** at `4622411fd`) and the row appended to §3 as
UNPROCESSED.

#### 2026-09-01 — chore(unify): the P4.D138 follow-up — the LoRA train's server units 5–7

_Versions: core 0.0.736, harness 0.0.630, host 0.0.91, SPA 0.5.615; web/cli/tauri unchanged._

The resumed LoRA-train lane unified onto main and P4.D138 CLOSES whole; the
drift ledger's §3 is empty and the baseline stays `4622411fd`. Landed: bug
110's family-first `apply_loras` (the `image-dialects` corpus re-recorded at
the tip — exactly the two predicted rows moved) and bug 111's error-level
request log plus v4's debug line, both capture-pinned; the `list-models`
`loraSupport` map, the `options-schema` action and the NanoGPT detailed-
catalog cache (the unit-1 narrowing retired at source; the round-2
`LORA_SUPPORT_PENDING_P4D138_UNIT6` tripwire fired as designed and is
deleted; the two SPA LoRA beats live after their first run corrected the
model to a declaring family and fixed three gestures); the HuggingFace lookup
+ `lora-metadata` action behind an engine gate with the host transport, over
a 57-row differential carrying the canned wire per row. The §3 review found
nothing blocking and five fidelity items were fixed on the unify branch: the
bug-111 line no longer fires on the malformed-2xx arm v4 excludes, both log
lines report the posted model (`hidream` default), the `new URL()` stand-in
gained the WHATWG arms its doc called unreachable (dot segments, backslash,
host percent-decoding, port validation — six new corpus rows, v4 agreeing),
the host transport decides status before a body read, and the over-cap beat
asserts its flag unconditionally. Gate: nine affected families regenerated at
the baseline 9/9 zero SKIP; fmt/clippy both feature sets; release build;
**479 test binaries / 2,665 passed / 0 failed / 1 ignored — exit 0** with the lane-scoped env block (the eight affected families' recipe vars plus the HuggingFace family; the untouched families' oracle vars deliberately withheld — their /tmp oracles were retired at the round-2 cleanup hours earlier and they were proven at that gate on main; a first run with the stale block failed `brahma_console_routes` on a missing file, the recorded "deleted-path reads like a regression" trap; cargo captures a passing test's SKIP line, so their silence is the capture, not a claim — the affected families' positive proof is the by-name sweep above); ng test 373 files / 5,782; ng build clean; full Playwright
**258 passed / 3 failed / 1 skipped** in the full run (the skip is the standing store-probe park; the two LoRA beats LIVE and green) — the three reds are `salon-documents-flow` ×2 and the `workspace-flow` terminal pop-out, Document-Mode/terminal surfaces this lane never touches, the same three the lane record classified, green twice earlier today in this session's full runs and **18/18 green re-run in isolation** — the standing full-suite intermittent class, recorded, not this lane.

#### 2026-09-01 — feat(image): the HuggingFace LoRA lookup and the lora-metadata action (P4.D138 unit 7)

_Versions: core 0.0.735, harness 0.0.629, host 0.0.90._

P4.D138 unit 7 — v4 `2ece98c90`, the LoRA train's third commit and the order's
last unit. It closes P4.D138.

Two modules, split the way v4 splits them: `huggingface_repo_id` is pure and
dependency-free because the editor decides whether a source is askable-about
before offering a Query button, and that decision runs in the browser;
`huggingface_lookup` is the one place that asks HuggingFace, and it renders no
compatibility verdict — a false "this will not work" on an adapter that works is
worse than the silence it replaced.

The lookup gets its own transport seam rather than the shared `WireTransport`:
v4 splits timeout from network on the thrown error's NAME, and the shared seam
collapses a throw to a message. `LoraMetadataTransport` carries a
`ThrownError { name, message }`, and the host maps reqwest's `is_timeout()` onto
it. v4's ten-second `AbortSignal.timeout` becomes the transport's per-request
timeout.

The new `huggingface_lora_lookup_equivalence` drives v4's real modules with
`global.fetch` canned: 32 repo-id + 25 lookup rows, each network row carrying
the canned wire WITH it and recording the request v4 made, so v5's URL and
`Authorization` header are comparands. `image_profiles_routes_equivalence` grows
58 → 69 with v4's four guards in order, the source that never reaches the
network (proven by a transport that panics if reached), and the success/declined
pair — a declined lookup answering 200 with `ok:false`. Ten mutations, each
reddening a named arm.

One corpus arm was found vacuous by reading v4's answer rather than by a red:
`lora_metadata_null_body` measured the missing-source guard, because the shared
mock resolves `body ?? {}` and `null` arrived as `{}`. A verbatim-resolving mock
now delivers it.

Recorded divergence: v4's `detail` on a non-JSON body is V8's own `SyntaxError`
wording, which no Rust parser reproduces. Only that string is exempt, and both
spellings are asserted so the exemption cannot widen unnoticed.

The verb, handler, engine arm and the host's `ReqwestLoraMetadata` are wired
live.

#### 2026-09-01 — feat(image): the list-models LoRA map, the options-schema action, and the NanoGPT catalog cache (P4.D138 unit 6)

_Versions: core 0.0.734, harness 0.0.628, SPA 0.5.615._

P4.D138 unit 6 — v4 `84f33ce94`'s READ side, the half units 1–4 declared but
never served.

`GET ?action=list-models` now answers `loraSupport`, a `Record<modelId,
ImageLoraSupport>` between `source` and the conditional `fetchError`; a model
resolving nothing is ABSENT from the map, which is the editor's signal to offer
no LoRA rows at all. It resolves against the RAW provider string, as v4 does, so
a `GOOGLE_IMAGEN` request resolves against `GOOGLE_IMAGEN` even though its models
came from `GOOGLE`.

The new `imageProfileOptionsSchema` verb serves v4's `?action=options-schema` —
the SPA has dispatched it since P4.D139 and has been 400ing silently into its
legacy panel. Its provider gate is the plain registry lookup, not
`createImageProvider`, so a text-only provider answers a null schema where
`list-models` would refuse it; both payload fields are `null`, never a zero-cap
object.

NanoGPT is the only provider declaring the hook and its schema is per-model, so
this also lands v4's module-global detailed-catalog cache (60-minute TTL): the
synchronous schema hook gets no API key and cannot fetch, so the keyed model
listing fills the cache and the hook reads it. v5 keeps `parse_models_page`
sans-IO, so the write lands in `RealImageProvider::available_models` — the IO
layer, where v4 puts it too. A cold cache falls back to the provider-wide size
list.

That cache also retires the `lora_data_for` narrowing recorded in unit 1: a live
`lora`-tagged model outside the static dialect table now earns capability without
a dialect, with v4's four skips arm for arm.

`image_profiles_routes_equivalence` grows 44 → 58 cases. The unit ran RED FIRST
through the tripwire the previous round installed: `strip_pending_lora_support`
asserted v5 answered no `loraSupport` key, and fired the moment it did. The
constant and the helper are now deleted and the four masked arms compare whole.

P4.D139's `P4D138_LORA_SERVER_LANDED` e2e gate is flipped, and the two beats it
activates caught three gesture defects on their first live run, none of them a
product defect: `LORA_MODEL` was a flagship id (`flux-2-dev`) that declares no
LoRA support, so the beat would have activated onto a section that never
appears; Create is gated on an API key the beat never picked, and the fixture
seeds no NanoGPT key, so global setup now seeds one; and the post-reload
`maybeUnlock` waited on the Salon's landmark from a Settings route, so the
landmark is a parameter now. Both beats pass.

#### 2026-09-01 — fix(image): the NanoGPT LoRA dialect restructure + the failed-request log line (P4.D138 unit 5)

_Versions: core 0.0.733, harness 0.0.627._

P4.D138 unit 5 — v4's `648d5c8aa` (bugs 110 and 111), one v4 commit and so one
v5 commit.

Bug 110 restructures `model::nanogpt_loras::apply_loras`: the LoRA family is
resolved FIRST, the empty-list early return is gone, the weights/url key writes
are gated on `kept` being non-empty, the `lora_preset` attachment moves OUTSIDE
that guard, and a known family now reports `Some(dialect)` even when it
contributes zero keys. Both of v4's asymmetry comments are carried so the shape
is not "consistency"-fixed back: the HuggingFace credential is gated on there
being weights to authenticate, while the preset stands alone because it names a
server-side preset that needs no adapter. The `image-dialects` corpus was
re-recorded from the tip pin; exactly two rows move
(`lora_preset_without_adapters` and `lora_preset_no_loras_key` gain
`lora_preset`), and `lora_weights_token_without_weights` stays `{}` as v4 keeps
it.

Bug 111 logs the composed request at ERROR `NanoGPT image request failed` with
`{context, model, size, n, loraDialect, loraKeys, loraDropped, passthroughKeys,
error}` — key NAMES only, never LoRA values — and rethrows unchanged. It covers
the transport throw and the non-2xx gate, not the `Invalid response from NanoGPT
Images API` arm, which v4 raises after its own try/catch. To carry the dialect
facts to the log site without firing `apply_loras`'s warnings twice,
`build_nanogpt` now returns its `{passthrough_keys, applied}` pair through a new
`build_image_request_with_extras`; `build_image_request` delegates to it. v4's
DEBUG `Posting NanoGPT image request` line lands on every request alongside it.

Bug 111 is log-only and differential-invisible, so it is pinned by a capturing
`tracing` layer (the `title_update_tier3` idiom, thread-scoped) asserting the
ERROR line's field names on the failure branch, the DEBUG line on every request,
and silence on the success branch. Six mutations, each reddening a named arm.

#### 2026-09-01 — chore(unify): the drift catch-up round 2 of 2 — P4.D138 ∥ P4.D139 ∥ P4.D140 ∥ P4.D141 ∥ P4.D142 ∥ P4.66

_Versions: core 0.0.732, harness 0.0.626, host 0.0.89, web 0.0.101, SPA 0.5.614; cli/tauri unchanged._

Five of six orders unified onto main and P4.D138 left OPEN at units 5–7; the
oracle baseline moves `7fb668263` → `4622411fd` (v4 HEAD, zero commits past
it) with the LoRA train's three rows kept PARTIAL in the drift ledger. Landed:
the LoRA train's whole client half + server units 1–4 (matchers, the `loras`
write guard with the Zod envelope and `CoreError.details`, the params builder
+ five-site consolidation, the NanoGPT dialects at the commit-1 pin), bug
112's `lastMessageAt` redefinition whole (the chokepoint, both write sites,
six readers, restore, the boot recompute heal, the SPA flips), the Concierge
four-state whole (predicate family, resolver, flips + sentences, the
`conciergeState` PUT arm, the SPA control/badge/twin, the live walk),
`qt-range` + finding #107 + the host-class guard, and finding #106's
optimistic-bubble reconcile with the suite's first mid-turn beat. Wires: the
sidebar `conciergeOverride` binding (spec-pinned), §C measured a NO-OP with
evidence (message-bubble danger styling was never ported — a candidate), the
three P4.D140 masks deleted, the pending-`loraSupport` measured strip. The §3
review (five parallel readers) fixed six groups on the unify branch — three
would have shipped: the sidebar select's permanent optimistic latch (v4
derives from props; a refetch/auto-flip/other tab can now win), the
optimistic-bubble echo scoped across two clocks (now an id snapshot of the
rows on screen at send time), and a harness family the Concierge kind rename
had silently broken; plus v4's `safeQuery` fallback at both new
last-played-read sites, the orientation-inserted `size` key slot (corpus
row red-first), the qt-class guard's one-line-header regex + its ordered
self-test, the modal's providerKey dependency, and small doc/byte repairs.
The gate caught one more: v4's `overflow-hidden` markdown frame clips its own
toolbar pickers (a v4 filing candidate; v5 keeps the frame without it,
recorded). Gate: the 36-family sweep from the `4622411fd` pin 36/36 zero SKIP
(+ the two repaired families); fmt/clippy both feature sets; release build;
**477 test binaries / 2,655 passed / 0 failed / 1 ignored, ZERO `SKIP:` lines — exit 0** (the first full run stopped fail-fast at binary 26 on `avatar_job_tier3`, the second at `image_generation_tier3` — the key-mirror catch and the `/tmp/qt-imggen-*` pair collision recorded above; `image_generate_route_equivalence` shares that pair AND its env-var names with the tier-3 family, so its oracle var was withheld from the block and it ran GREEN by name against its own snapshot under `/tmp/unify-r2/route/`); ng test 373 files / 5,782; ng build clean; full Playwright
**259 passed / 0 failed / 3 skipped** (the standing store-probe park + the two D138-gated LoRA beats; the suite grew 256 → 262 with the two LoRA beats, the four-state walk, the two mid-turn bubble beats and their siblings — the first run went 257/2/3 on the two gate catches above, both repaired and re-run whole).

#### 2026-09-01 — feat(nanogpt): the LoRA wire dialects + the passthrough allow-list (P4.D138 unit 4)

_Versions: core 0.0.723._

The NanoGPT half of v4 `84f33ce94`, ported at that commit's exact state (the
train is D-stacked, so bug 110's fix arrives in its own commit next).

`model::nanogpt_loras` gains the wire half of v4's
`plugins/dist/qtap-plugin-nanogpt/image-loras.ts`: `apply_loras` (three
dialects — indexed `lora_url_N`/`lora_scale_N`, a single
`lora_weights`/`lora_scale` plus an optional `hf_api_token`, and
`lora_url`/`lora_strength` plus an optional `lora_preset`), the
`NANOGPT_PASSTHROUGH_KEYS` allow-list with its blank-skipping applier, and the
`NANOGPT_LORA_SCOPED_KEYS` pair the host deliberately keeps OFF that list so a
credential is never broadcast to whatever model a profile happens to name.

Capping happens twice by design — host-side in `cap_loras` and again here with
a different sentence — because a model whose family the static table does not
know resolves its capability from the live catalog's `lora` tag alone, and then
there is no cap and no spelling.

`build_nanogpt` grows the flat model-specific controls the same channel
carries (`guidance_scale`, `num_inference_steps`, `negative_prompt` beside the
existing `seed`), then the passthrough bag, then the LoRA keys, in v4's
insertion order. `supported_image_models("NANOGPT")` gains the ten LoRA family
ids (v4's `STATIC_IMAGE_MODEL_IDS` is now generated from the dialect table),
and the model-listing request asks for `?detailed=true`.

The `image-dialects` recorded corpus grows the dialect rows, the passthrough
allow-list arm and bug 110's own shapes, recorded against a worktree pinned at
`84f33ce94` so this commit's rows are the pre-fix behaviour.

#### 2026-09-01 — feat(images): one params builder for every image call site (P4.D138 unit 3)

_Versions: core 0.0.722, harness 0.0.619, host 0.0.88._

`quilltap_core::image_gen::params_builder` ports v4's
`lib/image-gen/params-builder.ts` and takes over from the four v5 call sites
that used to assemble image requests independently. v4's own framing applies
here unchanged: three of those sites read exactly ONE key off the profile
(`quality`), so anything configured on a profile worked in the Salon and
vanished for avatars, story backgrounds and the wardrobe preview. v5 inherited
that drift verbatim — `services::image_job_common::build_job_gen_params`
hard-coded it and `quality_from_parameters` WAS the bug — and both are deleted
here. Those paths now gain the profile's `negativePrompt`, `seed`,
`guidanceScale`, `steps`, its residual options bag and its LoRA list, and the
wardrobe preview resolves portrait through the provider's own mechanism instead
of the hardcoded 1024x1792 that only OpenAI ever accepted.

`ImageGenParams` grows `responseFormat`, `loras` and `profileParameters`, and
`n` / `seed` / `steps` become JS numbers (`f64` rendered through
`js_number_to_json`, so an integral value still prints as `1`). `to_key_value`
is reordered to v4's builder INSERTION order — `JSON.stringify` emits insertion
order, and the canned image key in three tier-3 families is built from it on
both sides.

The injected plugin-registry seam widens with it: `orientation_data_for`
becomes `declarations_for`, returning v4's whole `ImageDeclarations`
(one model list carrying both `orientationSupport` and `loraSupport`, plus both
provider-level defaults) rather than the orientation half.
`image_gen_data::image_declarations_for` merges v5's two compiled tables back
into that single-list shape.

The `generate_image` path also gains v4's step 5c: the effective profile's LoRA
trigger phrases are folded into the prompt crafter's `styleTriggerPhrase` seam
before expansion, so an adapter's magic word is woven into the crafted prompt
rather than bolted on afterwards — and the builder still appends whatever the
crafter failed to say.

`POST /api/v1/images?action=generate` is the one v4 call site with no v5 twin
(re-measured, not assumed).

`image_gen_leaves_equivalence` grows 25 params-builder rows against v4's real
`buildImageGenParams`, driven from each row's own recorded profile, overrides
and declarations.

#### 2026-09-01 — feat(images): the write-side LoRA guard + the Zod refusal envelope (P4.D138 unit 2)

_Versions: core 0.0.721, harness 0.0.618, host 0.0.87, web 0.0.101._

`quilltap_core::image_gen::lora_validation` ports v4's
`lib/image-gen/lora-validation.ts`: the reserved `parameters.loras` key is
validated before anything is written, so a malformed adapter list is a 400
with nothing stored rather than a profile that saves cleanly and fails at
generation time. There is deliberately **no cap check** on the write path — an
over-cap list is kept by the absence of a guard, so narrowing the model and
widening it again loses nothing.

The refusal is v4's Zod envelope, not a bespoke sentence:
`{error: 'Validation error', details: [...]}` at 400. `CoreError` therefore
gains a `details` carry (boxed, skipped when absent — the `entity` /
`associations` pattern), `Response::validation_error(...)` builds it, and the
dispatch transport merges `CoreError::validation_wire_body()` into the body the
way the store-unavailable 503 already merges its own. The issue objects are
reproduced key-for-key against Zod 4 — `invalid_type` puts `expected` first,
the size issues put `origin` first, array indices ride the `path` as numbers —
and every arm was measured against v4's real handler rather than guessed.

The guard sits between the "Parameters must be an object" check (whose refusal
it must not steal) and the `apiKeyId` lookup (whose 404 it outranks) on create,
and inside the `parameters !== undefined` arm on update, each with its own warn
sentence. `image_profiles_routes_equivalence` grows sixteen arms comparing the
WHOLE refusal body, plus two storage dumps proving the over-cap list is stored
intact and a refused update writes nothing.

#### 2026-09-01 — feat(images): the model matchers + the LoRA support resolver (P4.D138 unit 1)

_Versions: core 0.0.720, harness 0.0.617._

The first unit of the LoRA train's server half (v4 `84f33ce94`). Two new pure
modules and one new data table, all differential-verified against v4's real
`lib/` code.

`quilltap_core::model_matchers` is v4's `lib/plugins/model-matchers.ts`:
`model_matches_pattern` (exact id, then a `*` glob, then a plain family
prefix) and `field_applies_to_model` (an absent/empty list, or an unknown
model, resolves toward showing the field). The glob arm reproduces v4's
`new RegExp` faithfully on two axes the corpus now pins: the literal parts go
through `regex::escape` (a superset of v4's own escape set, so
`gpt-image-125` still fails `gpt-image-1.5*`), and the `*` join is spelled as
an explicit negated class rather than `.*`, because JS's `.` excludes CR and
U+2028/U+2029 where Rust's excludes only LF.

`quilltap_core::image_gen::lora_support` is v4's `lib/image-gen/lora-support.ts`:
`resolve_lora_support` (per-model `loraSupport` through the same `match_model`
the orientation resolver walks, then the provider-level declaration, then
none), `resolve_lora_scale_bounds`, `read_loras_from_parameters` (the read-side
re-check of an opaque bag — non-object entries, blank sources and out-of-range
scales dropped and named in the log), `cap_loras` (a `None` support strips, an
over-cap list keeps the leading entries and names what fell off) and the
trigger-phrase pair. `ModelInfo` gains `lora_support` and `match_model` becomes
`pub` — one matcher, two capabilities, exactly as v4's comment says.

`image_gen_data::lora_data_for` is the compiled declaration table (NanoGPT
only: six flagships with no support, plus one entry per LoRA family generated
from the dialect table), and `model::nanogpt_loras` carries that dialect table
(ten families, three dialects, longest-prefix matching) as compiled data —
v5 reimplements the NanoGPT plugin natively, so its `image-loras.ts` lands
beside the wire builder that will consume it.

`image_gen_leaves_equivalence` grows from 18 to 101 rows over the same jest
oracle, driving v4's REAL matcher and LoRA functions; every new row carries its
own input, so nothing is transcribed into Rust. Six mutations were applied and
each reddened exactly one arm (the `.`-class, the cap floor, the scale range,
the phrase-dedupe fold, the resolution order, and the source trim).
#### 2026-09-01 — fix(tests): the two reduced chat_messages DDLs carry customAnnouncer

_Versions: core 0.0.726._

Caught by the full workspace gate, not by any differential. The
`conversation_render_reconcile` and `embedding_dimension_reconcile` test modules
hand-roll a reduced `chat_messages` table listing exactly the columns the OLD
played-message predicate needed. P4.D140 widened that predicate to read
`customAnnouncer`, so the staleness gate's query started erroring against those
tables and both modules misjudged every chat — seven failures, none of them
visible to a family. Both DDLs gain the column (the real schema has always had
it) with a note at each site saying why a column nothing in the module names is
load-bearing.

#### 2026-09-01 — fix(spa): chat cards show when a character last spoke (v4 735d9408c, bug 112)

_Versions: SPA 0.5.601._

The client half of P4.D140. A new `chat/chat-activity.ts` transcribes v4's
`chatActivityAt` — the one export the client uses — and four display sites take
it: the Salon chat card (which read `updatedAt` ONLY, under a comment claiming
the Salon transform omits `lastMessageAt`; it does not), the merge-conversation
modal, the home recent-chat row, and the character Conversations card (whose
`||` becomes v4's `??`). Prospero's chats section reuses `<qt-chat-card>`
directly, so it inherits the fix with no local edit — v4 needed a transform edit
there and v5 does not.

Two DTOs gain `createdAt` in v4's slot: the home `RecentChat` (which the row's
fallback needs) and `BrahmaPastChat` (carried for wire fidelity — the launcher
renders no date at all).

Four spec pins, each mutation-proven: the helper's `??`-not-`||` semantics, and
a never-spoken-in chat dating by `createdAt` on the Salon card, the home row and
the character card.

Also repaired: the archived-character e2e seed stamped a wall-clock
`lastMessageAt` on a chat whose only message was old, so the new boot recompute
would walk the date back and sink the chat out of the virtualized render window
— breaking the courier beats' `openChatWith`. The message is now stamped RECENT
too, with `systemSender`/`customAnnouncer` forced NULL rather than inherited
from whatever row `LIMIT 1` returned.

#### 2026-09-01 — feat(boot): recompute existing chats' last-activity dates once (v4 735d9408c, bug 112)

_Versions: core 0.0.725, harness 0.0.619, host 0.0.87._

Bug 112's data pass, re-homed from v4's migration runner into the boot repair
chain in the P4.D97 shape. `recompute_chat_last_message_at` rewrites
`lastMessageAt` for every existing chat under the shipped predicate — the date
walks backwards off a Staff announcement, and a chat where only the Staff ever
spoke clears to NULL, where readers fall back to `createdAt`. `updatedAt` is
never written. One transaction. On the real instance v4 measured 608 of 871
chats mis-dated.

The drift query uses `IS NOT`, not `<>`, so a NULL on either side counts as a
difference — the chats that must be CLEARED are exactly the rows going to NULL,
and `<>` would never find them.

Once-only through v4's own `migrations_state` ledger, in both directions. A boot
that finds NO drift writes NO ledger row and simply re-checks next time, exactly
as v4's `shouldRun()` gate makes its runner behave: a v5 stamp on a clean boot
would make a later v4 boot skip a migration it never ran.

The new `chat_activity_heal_equivalence` family drives v4's REAL migration plus
its REAL ledger write over two scenarios — a mixed instance carrying every shape
from v4's own integration suite, and a clean one. It measures the `''`-
systemSender seam rather than guessing at it: the SQL mirror reads `IS NULL`, so
that chat clears, even though the in-memory predicate would have counted it.
Four mutation proofs, plus three unit tests. v4's prettify label is a recorded
non-port (no v5 runner UI).

#### 2026-09-01 — fix(restore): re-derive a restored chat's last-activity date from its transcript (v4 735d9408c)

_Versions: core 0.0.724._

`add_message` stamps `lastMessageAt` with the wall clock, so replaying a
transcript dated every restored chat to the instant of the restore — and that
column is what every list sorts and displays by, so an entire history landed in
one flat heap at the top. Restore now re-derives the column from the transcript
it just wrote, through the one predicate that defines it, in its own try
immediately after the per-chat message loop. Failure degrades to v4's exact
warning sentence; `updatedAt` is preserved by omission; NULL is a legitimate
answer, where readers fall back to `createdAt`.

Pre-existing, and previously masked because `updatedAt` was flattened by the
same replay.

Recorded as a no-counterpart: v4's `ai-import.service.ts` `assembleQtapExport`
twin (whose last-row read becomes a filter through the predicate, with the empty
case moving from `now` to `null`) has no v5 surface to port into — v5 ships no
AI-import wizard.

#### 2026-09-01 — fix(settings): chat settings carry allowCheapFallback, as v4's schema default does

_Versions: core 0.0.723._

A P4.D135 remainder, found by P4.D140's oracle regens and confirmed
pre-existing by two-pin attribution (the identical divergence appears against a
worktree pinned at the baseline `7fb668263`, and nothing P4.D140 touches is on
that path).

v4 `65f5021c8` appended `allowCheapFallback: z.boolean().default(false)` to the
end of `CheapLLMSettingsSchema`. A Zod `.default()` is always present after a
parse, so v4 writes the key on every create AND fills it in on every read —
including for a stored bag that predates it. v5's `CheapLlmSettings` had no such
field, so v5 wrote three-key bags, and `find_by_user_id` returned pre-4.9 bags
verbatim, three-keyed.

Both halves fixed: the field lands at the end of the struct with
`#[serde(default)]` (v4's schema position, so the serialized key order matches),
and the read fills the key in when a stored bag lacks it. That closed two
standing reds — `salon_reads_equivalence [settings]` and ten cases of
`system_restore_state`, all of them the one missing key — and left
`provisioning_equivalence`, `chat_settings_tier2`, `settings_routes`,
`chat_settings_composer_web_routes` and `system_restore_equivalence` green.
Two mutation proofs, one per half.

#### 2026-09-01 — fix(chats): every chat list dates by activity, not by the row changing (v4 735d9408c, bug 112)

_Versions: core 0.0.722, harness 0.0.618._

All six server readers routed through the chokepoint. The Salon-list and home
sort (one home, `sort_chats_for_list`), the projects chat list and the Brahma
console list all take `by_chat_activity_desc`; `RecentChat` gains `createdAt`
between `title` and `updatedAt` so the home client's fallback can reach it; the
Brahma console's enriched row gains `createdAt` in v4's exact slot; and
self-inventory's prompt-visible activity line drops its `updatedAt` middle
fallback.

The characters `?action=chats` route loses its hand-rolled re-derivation
entirely — its own independent copy of the same bug, taking the max
`type === 'message'` createdAt over the whole transcript and counting every
Staff announcement. Activity is now the stored `lastMessageAt`, and the ISO
round-trip through `new Date(ms).toISOString()` disappears with the block, as
v4's does.

Six mutation proofs, each verified applied. Two exposed corpus blind spots and
both were closed rather than noted: the projects list could not tell the two
fallbacks apart (both its chats were created the same day), so the oracle case
and the Rust side gained a matching `list_chats_activity_fallback` mutation that
pushes the never-spoken-in chat's `updatedAt` past the other's activity; and the
self-inventory fixture's one chat had `updatedAt == createdAt`, so the builder
now touches it to a later pinned value, which makes `latestActivityAt`
discriminate.

Also corrected: the `mutate_null_last_message` header comment in the home-routes
oracle case, which said the null case "falls back to updatedAt" — the sentence
this commit makes false.

#### 2026-09-01 — fix(chats): lastMessageAt moves only when a character spoke (v4 735d9408c, bug 112)

_Versions: core 0.0.721._

The write side of P4.D140. `update_chat_metadata` splits the one gate v5 had
into two: `updatedAt` still moves for any actual message row, `lastMessageAt`
only when the batch carried character-authored content. Both still take the
same single minted `now`, since a character-authored event is necessarily an
actual one.

`delete_messages_by_ids` now recomputes `lastMessageAt` from what survives, so
deleting the newest character message walks the date backwards instead of
leaving it pointing at a row that no longer exists; it can go to NULL, and
`updatedAt` is still deliberately not bumped. `get_last_played_message_at`
takes the shared `CHARACTER_AUTHORED_MESSAGE_FILTER`, so staleness and display
now agree by construction.

Red-first: regenerating `chats_messages_tier2` at the pin turned the existing
kitchen-sink `systemSender: "host"` case red — v5 stamped `lastMessageAt`
where v4 leaves it NULL — and the fix turned it green. Both corpora then grew
the arms v4's own new test file pins: a Staff-only batch, a mixed batch, an
announcement bubble and a raw TOOL row on the add side; on the delete side a
chat whose date walks back PAST a Staff row (with a newer tool row and
announcement bubble present, so the old narrow filter would pick the wrong
one) and a chat that clears to NULL. Three mutations, each reddening the arm
it should. The two maintenance families were re-measured rather than assumed:
their fixture's played messages are plain ASSISTANT rows, so the widened
predicate does not flip their staleness.

#### 2026-09-01 — feat(chat-activity): the character-authored chokepoint (v4 735d9408c, bug 112)

_Versions: core 0.0.720, harness 0.0.617._

The first unit of P4.D140. `crates/quilltap-core/src/chat_activity.rs` ports
v4's new `lib/chat/chat-activity.ts` whole: `is_character_authored_message`
(role USER/ASSISTANT, no `systemSender`, no `customAnnouncer` — whispers count
by omission), the SQL mirror `CHARACTER_AUTHORED_MESSAGE_FILTER`,
`chat_activity_at` (`lastMessageAt ?? createdAt`, never `updatedAt`),
`chat_activity_time` (NaN clamped to 0 so comparators stay total) and
`by_chat_activity_desc`.

Both of v4's spellings are mirrored rather than unified: the in-memory
predicate tests JS truthiness (an empty-string `systemSender` reads as absent),
the SQL mirror tests `IS NULL` (it does not). v4 ships the pair knowingly; the
new `chat_activity_equivalence` family measures the seam against v4's real
module instead of guessing at it.

The differential drives v4's real exports over its own test table plus the
edges that table leaves unstated (the `''` sender, an empty announcer object,
a lowercase role, the nullish empty-string win, unparseable timestamps sorting
as 0). The SQL-mirror arm translates v4's `QueryFilter` object into a WHERE
fragment mechanically and compares, so nothing is transcribed by hand. Five
mutations, each reddening exactly the arm it should.

Nothing calls the module yet — the write gates, readers, restore and the boot
heal follow in their own units.
#### 2026-09-01 — docs(p4.d141): the lane's verification gate record and the m6 placement row

_Docs-only change._

Records P4.D141's in-lane gate: 475 test binaries / 2,637 passed / 0 failed / 1
ignored with the lane's 14-variable env block and zero SKIP lines; the eight
affected families 8/8 ok through the sanctioned sweep driver from the
`60e3c4a0a` pin, with the changed-bytes greps; clippy clean on both feature sets;
SPA 367 files / 5,523 passed and a clean build; no Playwright (P4.66 owns the
port, and this lane's beat is authored gated). Also lists the twenty mutations,
each verified applied and reverted.

The m6 parity row for the Agent-mode badge is corrected: it no longer leads the
Chat section, because v4's placement — directly after the Concierge control — is
restored now that the four-state control occupies that slot.

#### 2026-09-01 — feat(salon): the Concierge four-state control, the single-pill header badge and the client predicate twin (v4 `60e3c4a0a`)

_Versions: SPA 0.5.601._

The SPA half of the four-state control. NEW `chat/concierge-state.ts` is the
client twin of v4's `chat-override.ts` — the same four purpose-named predicates,
transcribed 1:1, with a parity spec that is v4's own `chat-override.test.ts`
table row for row. v4 imports one module into both its server and its
components; v5's Rust core is the differential-proven authority, so the twin gets
the spec instead.

The header badge is rewritten as ONE pill derived from `getConciergeState`,
fixing a pre-existing divergence: v5 rendered two independent `@if` pills, so a
chat that was both off-duty and flagged showed BOTH where v4's ternary shows one.
Monitored now renders no badge at all — "the pill means something other than the
default is set". Restoring the two-pill render reddens two of the four new specs.

`_chat.css` parameterizes `.qt-danger-badge` over `--qt-concierge-badge-color`
and adds the `-muted` and `-info` recolors, with v4's comments: a recolor, not
four rules.

The sidebar control is built from scratch in its marked slot, retiring a
six-round-old named deferral: v4's two optgroups in v4's order, the four helper
sentences byte for byte, the four state icons with their tints, the four success
toasts, and the PUT wiring with v4's error path. `conciergeState` goes on the
wire as a SIBLING of the `chat` bag — sent as a bag key it would be stripped by
`updateChatSchema` and silently do nothing. `ChatSidebar` gains the
`conciergeOverride` input beside its existing `isDangerousChat` (v4's own prop
shape) and its participant cards now paint from `shouldShowDangerStyling`.

⚠ One cross-lane wire remains, recorded loudly rather than defaulted silently:
`conciergeOverride` originates in `screens/salon/salon-conversation.ts`, which
P4.66 owns, so one binding line is left for the unifier. Until it lands the
control can display only Monitored/Flagged; writing an operator state works end to
end. The four-state e2e walk is authored and gated on that wire.

⚠ Commit-boundary note: five of these SPA files —
`chat/concierge-state.ts` and its spec, `chat/conversation-header.ts`,
`core/core-contract.ts` and `styles/qt-components/_chat.css` — were swept into
the PRECEDING commit (the server `conciergeState` arm) by a `git add -A` while
this work was already on disk. The content is this change's; only the boundary is
wrong, and it is recorded here rather than rewritten.

Riding along: a sibling spec that destructured the section's `<select>` list
POSITIONALLY is now label-scoped. It broke the moment the Concierge control took
v4's slot at the head of the panel — and before that it would have driven the
wrong control silently.

#### 2026-09-01 — test(concierge): the uncensored route reaches the spine, and one corpus that looked like coverage does not (v4 `60e3c4a0a`)

_Versions: harness 0.0.620._

Adds v4's own motivating regression to the story-background corpus:
`uncensored_override_candid` — the operator asserts a chat spicy
(`conciergeOverride: 'UNCENSORED'`) while the classifier label is `false` and the
GLOBAL Concierge mode is `OFF`, and the image prompt must still go out candid and
bound for the uncensored profile. Before the four-state port that corner was
unreachable. The discriminator is byte-visible: the `IMAGE_PROMPT_CRAFTING`
request is 5,487 characters where the concealed sibling is 6,347. Dropping the
Uncensored arm from `should_use_uncensored_route` reddens it.

Recorded, and the more useful half: **`precompute_equivalence` is structurally
blind to that predicate.** Its `dangerous-chat-reroute-runs` case reads as
coverage of the uncensored cheap-LLM swap, but both sides pass `allProfiles: []`,
so the resolver has nothing to swap to and the emitted row never carries the
selection. Measured — forcing the predicate to return `false` unconditionally
leaves the family green, that case included. The family header now says so, and
making it discriminating (seeding an uncensored profile and threading
`allProfiles` through both sides) is deferred loudly rather than done here.

Also measured and recorded: the resolver's forced `AUTO_ROUTE` is NOT observable
from the story-background path, which never gates on `mode`. That arm's coverage
lives in `danger_resolver_equivalence` section 1, where its own mutation reddens.

#### 2026-09-01 — feat(salon): the chat PUT's conciergeState arm closes v5's long-named deferral (v4 `60e3c4a0a`)

_Versions: core 0.0.721, harness 0.0.619._

v5 had no production caller of `apply_concierge_flip` — only the harness drove
it, and `api/salon.rs` named `conciergeState` a deferral. That closes here. The
PUT's `conciergeState` key (a SIBLING of `chat`, not a bag field) dispatches
through `apply_concierge_flip` with the real Concierge announcer, in v4's exact
position: after the participant add, before the remove, re-reading the chat only
when the flip actually changed something.

Guard order follows v4's handler, which parses the whole body after the 404 and
before `processChatUpdates` runs: an out-of-domain `conciergeState` refuses the
entire request with the `chat` bag UNWRITTEN. Measured on v4: the refusal is
`.parse` uncaught, so the middleware answers 400 `{error: 'Validation error',
details: [...]}`; v5 answers the flat sentence, the `details` issue array being
the standing project-wide deferral.

The field is typed `Option<Option<Value>>` with `double_option`. `Option<Value>`
alone collapses JSON `null` to key-absent, and since v4's `.optional()` is not
`.nullish()`, an explicit null is a ZodError — the collapse would have turned a
refusal into a silent success. A serde-boundary unit test pins all five shapes
(absent, a valid value, the retired `'off'` spelling, null, and a wrong type),
because `salon_mutations_equivalence` calls the handler directly and cannot see
the boundary at all.

The differential grows nine cases over v4's REAL PUT route: the four accepted
values (three writing the stored pair and posting a bubble, one a no-op), three
refusals, both families in one request, and — the arm that matters most — an
invalid state alongside a valid `chat` bag, proving nothing is written. Five
mutations were verified applied: accepting `'off'`, moving the validation after
the bag write, treating null as absent, skipping the flip, and dropping
`double_option`. Each reddened exactly the arms it should.

Two harness fixes rode along: minted Concierge-bubble ids are placeholdered and
sorted last (they differ between implementations by construction), and the
`lastMessageAt` column is masked behind `LAST_MESSAGE_AT_PENDING_P4D140` after a
measurement — bug 112 shows up here in two shapes, a system bubble v4 no longer
bumps for and a delete v4 now recomputes backwards from, and both leave v4's
stamp earlier than v5's.

#### 2026-09-01 — test(concierge): the two classifier gates prove the Uncensored skip (v4 `60e3c4a0a`)

_Versions: harness 0.0.618._

Grows the two danger-gate fixtures with an operator-Uncensored chat, so the
`is_classifier_on_duty` change landed in the previous commit is actually
measured. `danger-scan` gains chat `cb` — never classified, salon, with a
resolvable profile, so it would be enqueued but for the gate — and
`danger-gatekeeper` gains `skip-uncensored`, deliberately carrying the
`dangerous-llm` token so a reverted gate would flag it and post a Concierge
bubble rather than fail quietly.

Both corpora were BLIND before this: rebuilding the pre-change fixtures and
reverting each gate to the raw `conciergeOverride === 'OFF'` test left both
families GREEN. On the new corpora the same two mutations redden **exactly**
their own family and leave the sibling green.

Recorded, a refuted order premise: `danger_routing_equivalence` was named as the
home for the resolver's two operator arms, but that family drives
`provider_routing` and is fed `DangerousContentSettings` directly — the resolver
is invisible to it. Those arms are covered in `danger_resolver_equivalence`
section 1 instead (six new resolve rows, mutation-proven), and
`danger_routing_equivalence` is re-run unchanged and green.

#### 2026-09-01 — feat(concierge): the four-state per-chat control's predicate family, resolver and flips (v4 `60e3c4a0a`)

_Versions: core 0.0.720, harness 0.0.617._

Ports the substrate of v4's four-state Concierge control. `conciergeOverride`
widens from `NULL | 'OFF'` to `NULL | 'OFF' | 'UNCENSORED'`, and the tri-state
Safe/Flagged/Off-duty becomes a 2x2 — rows are the route (ordinary vs
uncensored), columns are the provenance (the classifier vs the operator):
Monitored, Flagged, Vouched Safe, and the previously unreachable corner,
Uncensored.

`chat_override.rs` is reshaped: `ConciergeState` gains `Vouched`/`Uncensored`
and renames `Safe` to `Monitored`, and the two overloaded predicates
(`is_concierge_off_duty`, `is_chat_active_dangerous`) are DELETED rather than
re-pointed — as v4 did, so every call site is forced to state which question it
is asking. Three purpose-named predicates replace them:
`should_use_uncensored_route` (flagged or uncensored), `should_show_danger_styling`
(flagged only — an uncensored chat takes the same routes by the operator's own
hand and is deliberately not painted as a hazard), and `is_classifier_on_duty`
(monitored or flagged, and true for a `None` chat).

Every call site is threaded: the eight shared-helper sites
(`orchestrator.rs`, `pre_compute.rs`, `story_background_job.rs`,
`title_update_job.rs`, `memory_extraction_job.rs`, `carina_memory_extraction.rs`,
`pascal/llm_consult.rs`, `tools/generate_image.rs`), plus `context_summary.rs`,
whose private copy of the old predicate is deleted in favour of the shared
helper. The two raw `conciergeOverride === 'OFF'` classifier gates
(`danger_scan.rs`, `dangerous_content/gatekeeper_job.rs`) now go through
`is_classifier_on_duty`, so an Uncensored chat is never reclassified out from
under the operator.

The resolver gains `chat-uncensored`: it spreads the global settings (so the
configured uncensored profile IDs ride through) and forces `AUTO_ROUTE` with
threshold 1.0 and every scan off. Forcing AUTO_ROUTE even under a global `OFF`
is deliberate — asking for uncensored routing on one chat should not first
require flipping a global switch. `chat-off-duty` is renamed `chat-vouched` and
`OFF_DUTY_DANGEROUS_CONTENT_SETTINGS` becomes
`vouched_safe_dangerous_content_settings`. Branch order is exempt → uncensored →
vouched → global → default; exempt-beats-uncensored is test-pinned.

`manual_flip.rs` handles the fourth state. `manual-off-duty`/`manual-on-duty`
become `manual-vouched`/`manual-resumed` (the resumed scope WIDENS: it fires for
Monitored requested from Vouched *or* Uncensored), and `manual-uncensored` is
new. Both operator arms write only the override column, preserving
`isDangerousChat` underneath so returning to Monitored/Flagged picks up where
the classifier left off. The Concierge writer grows to five kinds: the vouched
persona copy is reworded, `manual-resumed` keeps the old "returns to his post"
line under its new name, `manual-uncensored` is new, and the vouched advisory
gains "Ordinary providers still apply."

Differentials, all regenerated from a worktree pinned at v4 `60e3c4a0a`:
`danger_resolver_equivalence` section 1 grows to 30 rows (v4's own
`chat-override.test.ts` truth table asking all three predicates, plus the
resolver's uncensored arms incl. AUTO_ROUTE-under-global-OFF and
exempt-beats-uncensored); section 2 grows from 7 to 16 ops — all 12 ordered
four-state transitions plus the four no-ops, with v4's
`expect(update).not.toHaveProperty('isDangerousChat')` reproduced as a
column-level assert over the dumped rows. The five manual sentences are diffed
byte-for-byte through the un-mocked writer (the U+2019 in `operator’s` included).
Seven mutations were verified applied and each reddened the family.

One cross-lane divergence is recorded, not fixed: v4 bug 112 (`735d9408c`,
P4.D140's lane) is an ancestor of this lane's pin, so at the pin v4 no longer
moves `lastMessageAt` for a system-authored row while v5 still does. The fix
lives in `db/chats_messages.rs`, which P4.D140 owns. The column is masked in
`danger_resolver_equivalence` and `danger_gatekeeper_tier3_equivalence` behind
`LAST_MESSAGE_AT_PENDING_P4D140`, but only after a measurement asserts the
divergence has exactly the shape bug 112 predicts — and the measurement reddens
the moment P4.D140 lands, which is the signal to drop the mask.
#### 2026-09-01 — test(e2e): the LoRA editor beats, gated ACTIVATE-AT-UNIFY (P4.D139 unit 8)

_Versions: SPA 0.5.609._

Two beats in a new `settings-image-lora-flow.spec.ts`, riding the shared
global-setup server and self-cleaning: the round trip (a LoRA-capable NanoGPT
model offers the editor with its capacity sentence, an adapter is added and
saved, and a full reload re-opens the editor with it intact) and the over-cap
flag (adding to the cap then narrowing the model keeps every row).

Both gated behind `P4D138_LORA_SERVER_LANDED = false`. A NAMED constant, not a
capability probe, for two reasons: a probe cannot tell a model that
legitimately declares no support — most of them, which is the whole point of
§A's absent-not-empty rule — from a server that has not learned to serve
support at all; and an unknown field on a dispatch verb is silently ignored, so
a `loras` list would round-trip as nothing and the reload assertion would fail
for a reason that says nothing about this lane.

No Playwright run in this lane (P4.66 owns port 4319). The beats were
parse-checked with `playwright test --list`, which starts no server, runs no
`globalSetup`, and binds no port: both register.

#### 2026-09-01 — feat(spa): the image-profile editor asks the plugin what to render (P4.D139 unit 7)

_Versions: SPA 0.5.608._

v4 `84f33ce94`'s `ImageProfileForm` hunks plus `2ece98c90`'s `hfToken` prop,
ported into `image-profile-modal.ts`: the `optionsSchema`/`loraSupport`/
`catalogVersion` state, the options-schema fetch effect keyed on
`[normalized provider, model, catalogVersion]` with v4's cancelled flag, the
render swap that puts the shared model-aware panel in front of the legacy
arms, and the LoRA editor beneath both.

The semantics that make it safe are all here: a failed fetch CLEARS both
rather than leaving a stale schema on screen; a provider change clears them
eagerly, before any answer lands; `catalogVersion` bumps when a model fetch
answers `source: 'provider'`, so a plugin that builds its schema from a
key-gated catalog is asked again once that catalog exists;
`handleSetParameter` deletes the key on `undefined` or `''`; `handleLorasChange`
deletes `parameters.loras` when the list empties; `currentLoras` reads only an
array; and `hfToken` is the bag's `hf_api_token` only when it is a string.

v5's parameters live in a JSON textarea rather than an object, so every
structured write round-trips through the bag exactly as `setSize` has since
P4.D102 — the legacy arm IS the textarea, which is v5's own invention;
v4's `default:` case renders nothing. Recorded in the class doc.

Nine mutations redden the spec. Two needed the cases rewritten to be measured
at all: the failed-fetch clear was masked by the eager provider-change clear
until the case was rebuilt around a MODEL change, and the cancelled flag was
unpinned until a case raced two answers by holding the first open.

#### 2026-09-01 — feat(spa): the LoRA list editor (P4.D139 unit 6)

_Versions: SPA 0.5.607._

v4's `LoraListEditor.tsx` (`84f33ce94` + `2ece98c90`'s query half) ported as
`screens/settings/images/lora-list-editor.ts`, with every string verbatim: the
heading, the composed `sourceHint` sentence, the empty state, the over-cap
warning, the Query button's two titles, both help paragraphs, the strength
label, and the footer tally. A null `support` hides the editor entirely.

The mechanics that make position-keyed rows safe are ported with their reasons:
editing a Source DISCARDS that row's answer (a stale fact beside a new address
is worse than no fact) while a scale or trigger edit does not; removing a row
RE-INDEXES the answers below it; an emptied trigger phrase becomes `undefined`
rather than `''`; and a thrown or non-ok request collapses into the same
`network` panel as a failed lookup.

The slider carries the literal `qt-range w-full` per Shared contract §B and
ships no slider CSS of its own — P4.D142 defines the class, and the name is
inert-until-both rather than broken.

⚠ The wire is STUBBED: `imageProfileLoraMetadata` is P4.D138's, so the query
cases drive a scripted `CoreClient` — answering with the RECORDED shapes from
unit 5's oracle, so the editor is exercised on what the server will send.

Ten mutations redden the spec, including both re-index spellings. The swap
mutation initially reddened only one case: a page-wide text assertion cannot
tell WHICH row an answer sits under, which is the exact bug the re-index
prevents. The three re-index cases now scope every assertion to a row.

#### 2026-09-01 — feat(spa): the LoRA query-result panel (P4.D139 unit 5)

_Versions: SPA 0.5.606._

v4's `LoraQueryResult.tsx` (new at `2ece98c90`, 187 lines) ported as
`screens/settings/images/lora-query-result.ts`. Renders what HuggingFace
declares about a repository and passes NO judgement on whether it will work —
the module doc carries v4's reason, that a false "this will not work" on an
adapter that works is worse than the silence it replaces.

Everything is here: the seven `failureCopy` sentences (including
`rate-limited`'s U+2019 in `moment’s`), the three `kindCopy` sentences, the
Trained on / Nature / Pipeline / Weights (0-1-many arms) / Gated / Standing
rows, the trigger-phrase row with its `Use it` button versus
`— already in place.` on TRIMMED equality, the closing sentence, and the
failure panel with its `HuggingFace — {repoId}` heading and
`Try the page yourself` link. External links carry
`target="_blank" rel="noopener noreferrer"`.

v4's JSX copy that carries meaning is lifted into exported helpers so the spec
pins its bytes directly rather than through a whitespace-collapsed
`textContent` — a spelling difference, not a semantic one; the helpers are 1:1
with v4's own module-level `failureCopy`/`kindCopy`.

The result objects the spec renders are NOT hand-written. New oracle case
`harness/oracle/cases/lora-lookup-shapes.test.ts` drives v4's REAL
`lookupHuggingFaceLora` with `fetch` mocked over v4's own payload fixtures at
the pin, and records the ten resulting shapes. That caught something a written
fixture would have missed: v4's `ambiguous-weights` payload derives
`isLora: true` WITH `isAdapter: false`, which is what makes the ORDER of
`kindCopy`'s two ifs testable at all. Five mutations redden the spec.

#### 2026-09-01 — test(spa): transcribe v4's repo-id table, which does exist (P4.D139 unit 3 correction)

_Versions: SPA 0.5.605._

Unit 3 recorded that "v4 ships no unit test for this module". Wrong: v4's four
repo-id cases live inside `__tests__/unit/image-gen/huggingface-lookup.test.ts`
— the module is re-exported from the lookup, so its tests sit with the
re-exporter rather than the source, and a search beside the module finds
nothing. They are now transcribed 1:1 alongside the recorded vectors, and the
oracle case's and lane record's claims are corrected.

The recording still earns its place: v4's four cases cover the happy shapes and
eight refusals, none of the machinery the corpus reaches.

#### 2026-09-01 — feat(spa): the LoRA / options-schema DTOs and client API (P4.D139 unit 4)

_Versions: SPA 0.5.604._

The Shared contract §A shapes land in `core-contract.ts`'s §D region:
`ImageLoraSpec`, `ImageLoraSupport`, `HuggingFaceLookupFailure`,
`HuggingFaceLoraFacts`, `HuggingFaceLookupResult`, and the two new request
variants `imageProfileOptionsSchema` + `imageProfileLoraMetadata`. Field names
are §A's verbatim; the doc comments carry v4's reasons — why the lookup renders
no compatibility verdict, why an over-cap list is kept rather than deleted, and
why the token rides the POST body rather than a query string.

`image-profiles.api.ts` gains `fetchImageOptionsSchema` and
`queryLoraMetadata`, and `ImageModelListing` gains `loraSupport` between
`source` and `fetchError`. The map is read through a defensive helper: a
server that has not landed it yet reads as "no model declares support", which
is precisely what the map's own absent-not-empty rule means, so the editor
degrades to offering no LoRA rows rather than to a crash.

⚠ **Cross-lane:** the two dispatch verb NAMES are this lane's, since §D gives
`core-contract.ts` to P4.D139 — P4.D138's server arms must match
`imageProfileOptionsSchema` / `imageProfileLoraMetadata`, and the unifier
diffs the contract name-for-name.

#### 2026-09-01 — feat(spa): the client HuggingFace repo-id twin (P4.D139 unit 3)

_Versions: SPA 0.5.603._

v4's `lib/image-gen/huggingface-repo-id.ts` (new at `2ece98c90`, split out of
the lookup precisely so the browser can decide whether to offer a Query
button) transcribed into the SPA as
`screens/settings/images/huggingface-repo-id.ts`. Pure and dependency-free on
both sides. Nothing consumes it yet — the LoRA editor is unit 6.

v4 ships NO unit test for this module, so the whole differential is a 49-row
recording of v4's REAL functions from a worktree pinned at `2ece98c90` (new
oracle case `harness/oracle/cases/huggingface-repo-id.test.ts`). The corpus
asks the questions a hand-written table would have invented answers for: the
hostname regex's three arms (`hf.huggingface.co` accepted,
`nothuggingface.co` and `huggingface.co.evil.example` refused, and a
fully-qualified trailing dot refused), the first-two-segments rule INCLUDING
its quirk (a `/models/owner/name` URL yields `models/owner`, which v5
reproduces rather than fixes), the leading-alphanumeric anchor on each
segment, and what the `^https?://` gate does not treat as a URL.

Five mutations redden it: `includes('huggingface.co')` for the hostname test
(3), a single character class per segment (4), last-two segments instead of
first-two (3), dropping `filter(Boolean)` (15), and dropping the trim (2).

#### 2026-09-01 — feat(spa): the shared options renderer honours `appliesToModels` (P4.D139 unit 2)

_Versions: SPA 0.5.602._

v4 `84f33ce94` retired `appliesToModels`'s reserved status: `shouldRenderField`
now consults `fieldAppliesToModel` FIRST, before `showIf`. Ported into
`provider-options-panel.ts` with v4's rationale comment verbatim — once a
plugin has named the models, an unnamed one is a deliberate no. The
"Reserved … Intentionally not consumed" doc on
`ProviderOptionField.appliesToModels` is replaced by v4's live semantics text.

The gate is live for the LLM side too: this panel serves the connection-profile
editor (P4.D84) as well as the image-profile editor the LoRA train points at
it, so the nine new cases are written against an LLM-side schema on purpose.
No v5 provider manifest declares a matcher list today, so nothing observable
moves until one does.

Written RED first: against the pre-gate renderer the block ran 4 failed / 5
passed, the four being exactly the gate-dependent cases. Three mutations
redden it again — moving the gate below `showIf` (3 red, incl. the
order-specific case), reading `modelName` untracked so a model swap leaves a
stale gate (1 red), and removing the gate entirely (4 red, reproducing the
red-first split exactly).

#### 2026-09-01 — feat(spa): the client model-matcher twin for `appliesToModels` (P4.D139 unit 1)

_Versions: SPA 0.5.601._

v4's `lib/plugins/model-matchers.ts` (new at `84f33ce94`) transcribed into
the SPA as `screens/settings/providers/model-matchers.ts`:
`modelMatchesPattern` (empty-pattern-never, exact, `*` glob with
regex-escaped literals anchored `^…$`, then plain prefix) and
`fieldAppliesToModel` (absent/empty list or unknown model resolve toward
showing). Names match P4.D138's Rust twin. Nothing consumes it yet — the
renderer gate is unit 2.

Two-part differential: v4's own eleven-expectation unit table transcribed
1:1, plus a 37+12-case recorded differential against v4's REAL functions
run from a worktree pinned at `2ece98c90` (new oracle case
`harness/oracle/cases/model-matchers.test.ts`, vectors committed at
`__fixtures__/model-matchers-vectors.json`). The recording reaches what the
transcription cannot: the `^…$` anchors, the escape class's actual
membership, and the guard order that makes an empty pattern match nothing
even though every string starts with it. Mutation-proven — dropping the
anchors, removing the empty-pattern guard, narrowing the escape class, and
inverting the unknown-model arm each redden the corpus and not the
transcription. One mutation (WIDENING the escape class to cover `-` and
`/`) stays green and is recorded as behaviour-neutral rather than a
coverage gap: `\-` and `\/` mean themselves outside a character class.
#### 2026-09-01 — feat(lint): guard every Angular component host's qt-* class, not just the four utility families

_Versions: SPA 0.5.602; core/harness/host/web/cli/tauri unchanged._

Extends `check-qt-classes.mjs` (P4.D142, point 4): the guard already refused
an undefined `qt-bg-`/`qt-text-`/`qt-border-`/`qt-shadow-` name or a
hand-written variant, but it deliberately never policed bare component
classes, on the theory that most are theme hooks meant to have no app-side
rule. That theory doesn't hold for a component's OWN host class — an
unstyled Angular custom element defaults to `display: inline`, and an
unruled host class has now shipped as a live bug three times (dogfood #97,
the Almanack's `qt-entity-tabs`, and #107, fixed in the prior commit). The
guard now also scans every `@Component`'s `host: { class: '…' }` (and
`[class.qt-…]` conditional bindings) and requires any bare `qt-*` token
found there to resolve to a CSS rule, red-first proven by stripping
`.qt-markdown-field`'s rule and confirming the guard names it. Landed at
this narrower scope rather than the fuller "every host needs an explicit
display" invariant the finding proposed: building the wider form surfaced
roughly a dozen pre-existing hosts with no class, no style, and no
bare-element rule, each needing its own visual judgment call — recorded as
a named follow-up rather than guessed at.

#### 2026-09-01 — fix(theme): give sliders a real qt-range class, and fix finding #107's toolbar overflow

_Versions: SPA 0.5.601; core/harness/host/web/cli/tauri unchanged._

Ported v4 `5f56f7a7d` (P4.D142): the `.qt-range` class (token-driven via
`--qt-range-accent`/`--qt-range-focus-ring`, natively rendered on purpose —
`accent-color` paints both the filled track and the thumb, which
`appearance: none` would lose in Chrome/Safari) and its adoption across all
twelve v5 range-input hosts, replacing five ad-hoc idioms (a text-field
style on both talkativeness sliders, `appearance-none` with no replacement
track on two memory sliders, and six unaccented sliders). Also fixed
dogfood finding #107: `qt-markdown-field`'s Angular host had no CSS rule at
all (the third instance of the unstyled-custom-element-host family, after
#97 and the Almanack's `qt-entity-tabs`), so the formatting toolbar's
centred row hung out equally on both sides of the New Chat scenario field
and every other markdown form field. Fixed with one rule giving the host
v4's exact frame (`MarkdownLexicalEditor.tsx:194-206`) plus `display:
block`. One build-forced deviation from the literal frame classes: three of
them (`qt-border-default`, `qt-bg-card`, `qt-shadow-sm`) are plain classes,
not Tailwind `@utility` declarations, so `@apply` refused them — inlined as
the equivalent raw properties instead, with the computed style unchanged.
#### 2026-09-01 — fix(salon): reconcile the optimistic user bubble against a mid-turn refetch (dogfood #106)

_Versions: SPA 0.5.601; no crate touched._

The user's own message could render twice during a multi-character turn.
v4 holds the whole transcript in one array that a refetch replaces wholesale,
so a mid-turn refetch can never duplicate anything; v5's optimistic bubble
lives in a separate signal appended at render, which was latent until the
realtime work started refetching the chat mid-turn. `displayMessages` now
drops the optimistic bubble the moment a matching persisted row exists
(same author, same content, not older than the send) instead of always
appending it. Added the suite's first mid-turn observation beat
(`salon-optimistic-bubble-reconcile.spec.ts`), which injects a realtime hint
at the wire (the same handler a real server frame calls) rather than racing
a background job's scheduling against the mock's reply timing, and proves
the fix by sampling the rendered count across the refetch window rather than
a single racy assertion. Red-first captured and restored before landing.

#### 2026-09-01 — docs(setupphase): the round-2 drift catch-up work orders — P4.D138 ∥ P4.D139 ∥ P4.D140 ∥ P4.D141 ∥ P4.D142 ∥ P4.66

_Docs-only; no version bumps._

Six agent-ready work orders for the pre-planned round 2 (drift-ledger §3's
eight rows, all now ORDERED): the three-commit LoRA train split
server/SPA (P4.D138 D-stacks `84f33ce94` → `648d5c8aa` → `2ece98c90`
server-side incl. the five-call-site params consolidation and the
HuggingFace lookup; P4.D139 carries the client half incl. the
`appliesToModels` gate), bug 112's `lastMessageAt` redefinition whole
(P4.D140, with the recompute boot heal in the P4.D97 ledger shape), the
Concierge four-state whole (P4.D141, closing the long-named
`conciergeState` PUT deferral), `qt-range` + dogfood finding #107's
inline-host fix and guard (P4.D142), and finding #106's optimistic-bubble
reconcile with the suite's first mid-turn observation beat (P4.66, the
round's one Playwright slot). Five shared contracts (§A–§E) pinned
verbatim across all six orders; `4622411fd` assigned to the round's
`/unify` for NO-PORT ratification with its evidence recorded. Regen rule:
PIN REQUIRED, per-lane pins at each target commit.

#### 2026-09-01 — chore(unify): the drift catch-up round 1 of 2 — P4.D134 ∥ (P4.D135→P4.D136) ∥ P4.D137

_Versions: core 0.0.719, harness 0.0.616, host 0.0.86, cli 0.0.17, SPA 0.5.600; web/tauri unchanged._

All four orders unified onto main; the oracle baseline moves `b121ac77f` →
`7fb668263` and the round's eight-row drift prefix is cleared (the Lima/WSL2
retirement, provider/model fallback chains, bugs 106/107, bugs 108/109, two
NO-PORT ratifications, the About Discord rider; eight commits remain as the
pre-planned round 2). The §3 unification review (four parallel reviewers
against v4's real source at the pins) found zero blocking findings and fixed
four groups on the unify branch: the `[CheapLLM] Task failed` warn now fires
BEFORE the fallback chain as v4's catch does (a rescued task still counts — the
counter bug 107 was measured from; capture-layer pinned, mutation-proven), the
failing-over toast fires on a message change within the stage (the second
stand-in's name is news; four new unit specs on a previously spec-less branch),
two corpus blind spots closed (three classifier ladder-order rows; the doc-text
guard-placement ops moved off `.yaml` — a supported text format — onto `.png`,
with insert gaining its own mutation-proven placement op), and four stale
comments/names corrected. Gate: family sweep 23 recipes + uuid-remap's replay
leg fresh from the `7fb668263` pin, zero unexplained SKIP; Tier R 214/0 against
v4's real launcher; 475 test binaries / 2,632 / 0 with the round's env block,
zero SKIP; fmt/clippy both feature sets; release build; ng test 5,477/0; ng
build clean; full Playwright 256 passed / 0 failed / 1 skipped (the standing
store-probe park; the suite grew 255 → 256 with the live understudy
round-trip beat).

## August 2026

#### 2026-08-31 — docs(status): record the P4.D134 lane gate

_Docs-only change._

The lane's verification gate: fmt clean, clippy clean on both feature sets, the
full workspace test green at 473 binaries / 2,586 tests / 0 failed with zero
`SKIP:` lines, and all four touched families confirmed to have run by name over
oracles regenerated from a worktree pinned at `1560bd43b`. SPA: 366 files /
5,459 tests and a clean build. No Playwright — the sibling branch owns port
4319 this round. Changed-bytes greps and the "no fixtures moved" statement are
recorded with it.

#### 2026-08-31 — style(rewrite): blank-line the gateway-order list (clippy doc_lazy_continuation)

_Versions: core 0.0.704._

The paragraph after the three-item gateway-resolution list in
`provider_manifest/rewrite.rs`'s header read as an unindented continuation of
item 3, which `clippy::doc_lazy_continuation` rejects under `-D warnings`. A
blank line separates them. Comment-only.

#### 2026-08-31 — docs(lima-retirement): the completeness census, the NO-PORT ratifications, and the help bank

_Docs-only change._

Closes P4.D134's tier-2 and tier-3 deliverables.

The grep census over `crates/*/src`, `crates/*/tests`, `apps/web/src` and
`harness/oracle` finds zero production code paths still consulting a Lima or
WSL2 signal. Every surviving hit is enumerated and classified: comments citing
the sha, the retired-vocabulary lock tolerance, the deletion pins that set
`LIMA_CONTAINER` on purpose, and v4's own `isVMEnvironment` name (which survives
with a new meaning). One stale comment was repaired —
`mount-points-routes.test.ts` still cited `LIMA_CONTAINER` as a containerized
probe.

Two help rows go to the `p4.9i2` bank: `chat-settings.md`'s timezone paragraph
and `the-almanack.md`'s runtime-type list.

Both adjacent NO-PORT? rows are ratified with file lists rather than subject
lines: `7819afb1d` is six files with zero `lib/`/`app/` hunks and no
`packages/quilltap` production hunk (a jest mock-factory fix in v4's own test,
plus README, changelog and version bumps), and `3c3432ae9` is one file,
`docs/releases/4.9.0.md`. `1560bd43b`'s own NO-PORT remainder is ratified by
file list too, including `packages/plugin-utils`, whose host-rewrite copy was
diffed against the lib copy at the pin and agrees.

#### 2026-08-31 — refactor(lock): retire the lima and wsl2 environments (v4 1560bd43b)

_Versions: host 0.0.85, cli 0.0.17._

The instance-lock half of v4's Lima/WSL2 retirement. `EnvironmentType` drops
`Lima` and `Wsl2`; `detect_environment_type()` loses the `LIMA_CONTAINER` and
`WSL_DISTRO_NAME` branches that used to run ahead of the Docker probe;
`resolve_runtime_mode()` loses the `vm` mode; `is_lima_environment()` is
deleted. The cross-host heartbeat arm narrows to `environment === 'docker'`,
the three-way containerized label collapses to the literal `Docker container`,
and the same-host `env_label` cascade drops its two VM rows.

The CLI's `is_vm_environment()` mirrors v4's `VM_ENVIRONMENTS` set, now the
single `docker` entry.

`EnvironmentType` gains an `Other(String)` READ shape. This is not an invented
value: v4's type is a bare TypeScript union over a JSON field its
`readLockFile` never validates (only pid, hostname and history are
shape-checked), so a lock written by a pre-4.9 v4 inside a Lima VM still parses
there and simply fails every `=== 'docker'` test. Without the catch-all v5
would classify that lock as `corrupt` where v4 reads it as a stale
different-host lock. A unit test pins the whole chain — parse, `as_str`
round-trip, not-a-container, `Stale` despite a one-second-old heartbeat,
`local server` label, and that no probe ever mints one — and two mutations
(counting `Other` as a container; collapsing it to `Local`) each redden it.

Tier R gained two cases and ran red-first against the pinned v4 launcher: with
the old three-value predicate, `lock status retired lima env` and `lock clean
retired lima env` produced 3 failures (two stdout, one exit code) out of 214;
after the port, 214/0.
#### 2026-08-31 — refactor(runtime): retire the vm runtime modes and the Lima gateway strategies (v4 1560bd43b)

_Versions: core 0.0.703, host 0.0.84._

The remainder of v4's Lima/WSL2 retirement.

`self_inventory`: `RUNTIME_MODE_LABELS` drops the `vm` -> "VM (Lima/WSL2)" and
`electron-vm` -> "Electron + VM" rows, which are prompt-visible bytes. No
differential can see this — every `self_inventory_equivalence` row runs
`local-dev` — so a unit test pins all five surviving labels plus v4's
`?? mode` fallthrough for the two deleted ones and for anything else. A
mutation restoring the `vm` row reddens it.

Almanack `runtimeType` drops `'lima'` from its union. Measured before porting:
v5's emitter has only ever answered `docker` or `node`, and the tier-2
differential splices the whole runtime-environment block, so this is pure
doc-comment convergence — recorded as such rather than claimed as a fix.

`provider_manifest::rewrite`: v4 collapsed five gateway strategies to two
(`QUILLTAP_HOST_IP`, then `host.docker.internal` in Docker) and redefined
`isVMEnvironment()` as `isDockerEnvironment() || QUILLTAP_HOST_IP is set`, which
is how a self-managed VM opts in. v5's port of this module is the pure URL
rewrite with the gateway injected, so the collapse lands as the module contract:
v4's why-comment is carried in full, including why `/proc/net/route` was
actively wrong for Docker.

That header also records a measured pre-existing gap, loudly: nothing outside
`quilltap-core` calls `with_localhost_gateway`, so the injected gateway is
`None` on every production path and v5 has never rewritten a localhost URL.
Porting the two surviving strategies would be new wire behavior, not a
retirement, so this deletion lane names it as a follow-up instead of closing it.
#### 2026-08-31 — refactor(about): the three back ends, no VM (v4 1560bd43b + 7fb668263)

_Versions: SPA 0.5.598._

The About page's half of the Lima/WSL2 retirement, byte-for-byte from v4's
post-`1560bd43b` `AboutView.tsx`: the "runs as a native desktop application"
paragraph now names macOS, Windows and Linux with Docker as the locked door;
the Native-desktop-app and Docker-runtime feature bullets describe the shell's
three back ends (Direct, Docker, Remote) instead of the old VM/Docker toggle;
the "macOS VM: Lima / VZ" and "Windows VM: WSL2" tech-stack rows are deleted;
and the infrastructure acknowledgment reads "Electron, Docker".

Riding along: v4 `7fb668263`'s one ported hunk, the updated Discord invite.
The existing link spec pinned the old invite and went red first.

A new spec asserts both halves — that none of Lima / WSL2 / WSL / VZ / macOS VM
/ Windows VM appears anywhere on the rendered page, and that each replacement
sentence is present verbatim.

Two v4 hunks in the same commit are NO-PORTs with evidence: `footer-wrapper.tsx`
(v5 ships no footer component and no `BackendMode` badge) and
`instance-lock-gate.tsx` (v5's startup screen never ported the environment-label
cascade).
#### 2026-08-31 — refactor(data-dir): drop the Lima probe and the isVM wire key (v4 1560bd43b)

_Versions: core 0.0.702, harness 0.0.604, SPA 0.5.597._

v4 `1560bd43b` retired the managed Lima (macOS) and WSL2 (Windows) VM modes.
The data-directory half: `isLimaEnvironment()` is deleted along with the
Lima-first branch in `getPlatform()` (a `LIMA_CONTAINER=true` process carrying
its exported rootfs's Docker markers now reports as `docker`, deliberately) and
the Lima disjunct in `getHostDataDir()`. `DataDirEnv` loses its
`lima_container` field — v4 reads the variable nowhere. `isContainerized()` in
the mount-index base-path probe drops the same disjunct.

`GET /api/v1/system/data-dir` no longer answers `isVM`: the key is gone from the
response builder, from the key-order pin, and from the SPA's `DataDirInfo` wire
type and spec mock.

`data_dir_paths_equivalence` regenerated from a worktree pinned at `1560bd43b`
and re-run red-first: the two Lima cases diverged on `platform`, `path`,
`sourceDescription` and `hostPath` before the port and are green after. Both
were reshaped into deletion pins — `platform_lima_flag_inert` and
`host_path_lima_flag_inert` set `LIMA_CONTAINER=true` on the v4 side and record
that it now changes nothing — and the corpus-coverage guard names them.
#### 2026-08-31 — fix(wardrobe): the outfit consult's two bounds inverted with the raised cheap-LLM ceiling

_Versions: core 0.0.712, harness 0.0.612._

The unified gate's catch. P4.D42 bounded the outfit consult twice, and on a
REMOTE profile the inner bound used to be the tighter one: the cheap-LLM attempt
gave up at 45 s, inside the 60 s `OUTFIT_LLM_TIMEOUT_MS` phase ceiling. Raising
the shared background tier to 90 s (bug 107) inverts that — and v4 inverts it
too, because `applyOutfitSelections` calls `chooseLLMOutfit` through
`withTimeout(…, OUTFIT_LLM_TIMEOUT_MS)` and never reaches for the options bag, so
both sides take the `background` default.

Unlike the memory recap — which v4 deliberately declares `interactive` precisely
to keep its phase ceiling above its own legs — v4 left this one to invert. So v5
reproduces the inversion rather than protecting against it, and the paused-clock
test now pins 60 s with the reasoning written down. The local twin (180 s attempt
vs the same 60 s ceiling) is unaffected.

Also here: three clippy allowances the new parameters earned
(`send_to_provider`, `resolve_provider_for_dangerous_content`, and the
title-update case tuple factored into a `type` alias), and the two new
constant assertions moved into `const { … }` blocks — which is the stronger form
anyway, matching the phase-ceiling pin in `build_context`.

#### 2026-08-31 — fix(jobs): a cheap-LLM pass lost to a timeout fails its job (v4 a1d88aa3a, bug 107)

_Versions: core 0.0.711, harness 0.0.611._

The half of bug 107 that makes the other half measurable. A cheap task that
times out returns an unsuccessful result, and every job handler treated that the
same way it treats a refusal: log a warning, return, and be marked COMPLETED. So
the memory that was never extracted and the scene state that was never derived
looked, from every counter the operator has, exactly like work that finished.

Five of v4's six handlers take the guard: title-update (before the cursor write,
so a timeout defers the rename instead of skipping it), context-summary,
story-background, memory-extraction and carina-memory-extraction. The sixth,
scene-state-tracking, has no v5 handler to attach it to — its wrapper is a
pre-existing tracked deferral, recorded loudly in the module header.
`TurnMemoryProcessingResult.passes_lost_to_timeout` carries the multi-pass count,
since a per-character pass fails soft and a turn can lose half its extraction and
still return `success: true`, and `SummaryGenerationResult.timed_out` carries the
fold's.

Two differentials grew a timing-out arm. `title_update_tier3` gains
`provider_times_out` — the throw comparand carries v4's exact
`CheapLLMTaskLostError` message. `memory_processor_tier3` gains
`self_pass_times_out`, where one SELF pass and one OTHER pass time out on a
profile with its own model: `passesLostToTimeout: 2`, `success: true`, both SELF
debug lines and the rewritten OTHER sentence. The completion rules can now throw,
which also made the RULED failed-call log divergence reachable from that family
for the first time — subtracted by name with an exact-count pin that fired the
moment the second timing-out rule was added. Three mutations, each reddening one
arm.

Recorded on the way: v4's retry is atomic because the job child's writes are
discarded on a throw, and v5's writer applies as it goes — so v5's re-run repeats
the passes that succeeded. The outcome is the same because the repeat is
idempotent (v4's own handler-audit table says so), which is why v5 needs no
buffering here.

#### 2026-08-31 — fix(cheap-llm): set the budgets from the measured curve and retry a timeout once (v4 a1d88aa3a, bug 107)

_Versions: core 0.0.710, harness 0.0.610._

The cheap-LLM ceilings were round numbers sitting inside the distribution they
were meant to bound. Across 1,971 completed non-compression calls on a live
instance, not one had ever exceeded 40,000 ms against a 40,000 ms provider
budget, and three task types peaked within 600 ms of the wall — a censored
distribution, where the maxima were the budget rather than the work.

The shared tier moves 45 s → **90 s** and compression 75 s → **120 s**, both set
clear of the measured p99s. The ceiling now follows *who is waiting*, not only
which task it is: a `CheapLlmLatencyClass` threads from the call site, and the
memory recap and the cache-miss inline compression declare themselves
interactive and keep the tighter numbers. That split is also what stops the
raise inverting `MEMORY_RECAP_PHASE_TIMEOUT_MS` — the const block pinning that
ordering failed to compile the moment the constant moved, which is the pin doing
its job.

A timed-out attempt now gets one more go at a fresh socket. Timeouts only: a 401
or a refusal would fail identically. Never on the interactive path, where the
operator is already waiting out the budget. `CheapLlmTaskResult.timed_out`
separates "this pass never happened" from "this pass disappointed me", and
`throw_if_lost_to_timeout` turns the first into a failed job.

v4's two test suites are transcribed whole (the deadline additions and the new
`cheap-llm-timeout-retry` suite), plus a background stalling-socket proof that
the retry is a *second budget* rather than a second call inside the first, a
wiring pin that the recap's interactive declaration reaches the provider's
budget (40 000 ms, not 85 000), and a forwarding pin that
`apply_context_compression` hands `options.latency` down. Five mutations, each
reddening the right arm.

Note the two `compressMemories` legs v4 also marks interactive have no v5 twin
yet — build-context phase 2 is a pre-existing tracked deferral — so the
`compress-memories` / `compress-system-prompt` override rows are ported and
ready but unreached in production.

#### 2026-08-31 — fix(chat): re-decide the attachment question when the model changes underneath the array (v4 a1d88aa3a, bug 106)

_Versions: core 0.0.709, harness 0.0.609._

The other two halves of bug 106. `services/message_attachment_adapter` ports
v4's new `lib/chat/message-attachment-adapter.ts`: the uncensored reroute now
re-runs `process_file_attachment_fallback` against the profile actually being
called, so an image a text-only substitute cannot read becomes its description
and the retry proceeds instead of dying at the gateway with *400 does not
support image inputs*. v4's same-array-reference contract — a profile that can
take the bytes gets the array back untouched, no describer spent — becomes a
`None` return, so it is a type fact rather than a comment. The rerouted
profile's raw row rides up through `RouteResult`/`DangerousProviderRouteResult`,
because v4's route result IS the whole `ConnectionProfile` and v5's four-field
`EffectiveProfile` cannot answer the attachment question.

And `needsVision` now reads what the message array carries rather than what the
user uploaded, at both call sites. An image the primary could not take was
already replaced by its description upstream, and a chain that still called the
turn vision-bearing skipped understudies perfectly able to answer it.
`ProcessedFiles::has_image_attachment` and `RunPrimaryStreamOptions::needs_vision`
are retired with it — the value is computed where it is used.

Differentials: `file_attachment_tier3` grows an (E) family driving v4's real
`adaptMessagesForProfile` and `collectAttachmentMimeTypes` over 11 + 3 cases
(the same-reference contract, the describe/inline/unsupported branches, the
keep-and-drop partition, the foreign-bag passthrough, multi-message and
multi-attachment ordering, MIME de-duplication), with its own fixture image so
the describe is a real call. `primary_stream_tier3` gains
`hard_error_vision_skips_understudy`, where `attachedFiles` is empty and the
array carries an image: the two spellings of `needsVision` disagree on exactly
that case. Seven mutations, each reddening one arm.

One recorded narrowing: v4's `delete next.attachments` distinguishes an absent
key from an empty list and `StreamMessage::User.attachments` cannot. Nothing
observes the difference — every request builder reads the list with JS
truthiness — and the differential collapses the two on both sides.

#### 2026-08-31 — fix(danger): order the uncensored scan by what the turn is carrying (v4 a1d88aa3a, bug 106)

_Versions: core 0.0.708, harness 0.0.608._

The uncensored reroute swaps the model but inherits the message array the
original profile's call was built against, bytes and all. On a turn carrying an
image, a vision-capable primary had correctly embedded the raw bytes and a
text-only substitute got them: 400, then the chain stopped and the character
said nothing. v5 measurably had the bug — the differential's new
`mime-image-prefers-carrier` case hands an image turn to a text-only OLLAMA
profile before this change.

`resolve_provider_for_dangerous_content` gains v4's fifth parameter, the MIME
types this turn's message array is carrying, and partitions its scan by
`profile_can_carry_turn` before walking. **Ordered, not filtered**: filtering
outright would trade a degraded-but-delivered turn for no reroute at all when
the only uncensored route on an instance happens to be text-only, so the
non-carriers follow behind the carriers rather than being dropped. The
`DangerousContentRouter` seam carries the parameter through; v4's three other
call sites (both danger-orchestrator legs, chat create) take the `[]` default
and so do v5's. `collect_attachment_mime_types` arrives with the new
`services/message_attachment_adapter` module and feeds the empty-response
reroute's call.

`danger_routing_equivalence` grows eight cases and three profiles: the image and
non-image carrier preferences, the `every`-not-`some` multi-MIME arm, the
nobody-carries fall-through that proves this is a preference, an explicit
configured profile honoured despite the payload, and the OFF-mode arm. A carrier
with no API key sits first among the carriers, so the same row also pins that
the ordering happens before the key loop. The oracle now initializes the
provider registry with the ten real dist plugins — load-bearing, since the carry
test reaches `providerCanTransportImages`, which prefers the registry and would
otherwise answer the static mirror.

#### 2026-08-31 — fix(attachments): one predicate for "can this profile receive this?" (v4 a1d88aa3a, bug 106)

_Versions: core 0.0.707._

v4 had three independent spellings of the question "can this profile actually
receive an attachment of this MIME type?" — the dangerous-content router asked
it not at all, the describe-fallback and the fallback chain asked it
differently — and that drift is what produced v4's bugs 91, 97 and 104.
`a1d88aa3a` collapsed them into one `profileCanReceiveAttachment`; this commit
does the same in v5.

`profile_can_receive_attachment` lands in `files/image_transport.rs` over a new
`AttachmentProfileView` (v4's structural `{provider, supportsImageUpload?,
baseUrl?}` parameter; `baseUrl` is accepted-but-unused in v4 and so absent
here), and the model-half question moves to
`files/attachment_support::profile_supports_mime_type` as its single home.
`services/file_fallback::profile_supports_mime_type` is now a thin `&Value`
delegate, `needs_fallback_processing` is the predicate's negation, and the
fallback chain's `can_receive_this_turns_images` calls it with v4's own
`image/jpeg` probe.

Truth-table neutral, so the log line is the only observable movement:
`[Attachment] Profile claims image support but its plugin cannot transport
images` becomes `[Attachment] Plugin cannot transport images`, gains
`supportsImageUpload`, and now also fires when the operator's tick is off (the
early return that used to pre-empt it belongs to the predicate). Pinned by an
exhaustive negation test over provider x flag x MIME and by a capture-layer
test on the new sentence; both mutation-proven. `file_attachment_tier3`,
`attachment_anchor`, `fallback_engine` and `image_transport` all stay green
against oracles regenerated at the `a1d88aa3a` pin.

#### 2026-08-31 — feat(chat): the fallback chain on image description, the fourth call site (P4.D135 unit 6)

_Versions: core 0.0.706._

The describer's three escapes, in v4's order (`65f5021c8`): the primary's own
fallback chain, THEN the configured uncensored describer, then that profile's own
chain run dangerous. The chain comes first because it is cheaper to be right
about — a describer that is rate-limited or misconfigured is not a content
problem, and spending the uncensored profile on it wastes the one escape that can
actually answer a refusal. `processingMetadata` gains `fallbackAttemptTrail`:
who was asked and how each one failed, in order.

`needsVision: true` is the load-bearing flag on this path. A stand-in must both
accept image uploads and have a plugin that actually puts the bytes on the wire;
a describer that silently drops the image would answer from the prompt alone and
invent a picture, which is worse than failing.

The fixture's primary describer now names an understudy, so the chain WALKS and
the chain-before-uncensored order is observable — the trail alone was not enough,
and a mutation moving the chain after the uncensored escape survived until it
did. Pointing the understudy at the local Ollama profile does NOT work, and the
reason is itself a pin: `staticProviderCanTransportImages('OLLAMA')` is false, so
the vision gate drops it and the chain is empty again.

#### 2026-08-31 — feat(spa): the understudy picker, the cheap-LLM stand-in toggle, and the failing-over toast (P4.D135 unit 5)

_Versions: SPA 0.5.597._

The client half of the fallback chains (v4 `65f5021c8`). The profile modal gains
its Fallback card — the understudy dropdown (this profile and every Courier
filtered out; a cycle is deliberately offered, because chains never recurse and
one simply stops) and the tier-pick toggle with its Model Class nudge. The
soft key warning is a warning, never a block: keys arrive later, and a provider
that takes none at all is ready as it stands.

The Cheap LLM card gains "Allow a Similar-Tier Stand-In", the one off-profile
switch, with v4's copy carried over.

The Salon's rescue toast now fires on `failing-over` as well as `retrying` —
both are moments where the reply the user gets is not the one they configured.
No reducer change was needed and that was MEASURED, not assumed: `ResponseStatus
.stage` is a free string and the reducer carries the whole status object
through, so the new stage arrives intact. The recorded once-per-entry divergence
applies to both stages and bites harder on this one (a chain walking two
stand-ins toasts once where v4 toasts twice) — recorded, not fixed.

Nine new specs (the picker's filters, the pool default, the key warning in three
positions, the Model Class nudge) and six on the form's load/build round-trip
including the Courier body, which forces both fields off whatever the form
holds. Plus this branch's Playwright beat: the understudy chosen, saved, read
back off a reopened modal, then cleared and read back again — the one thing no
unit spec can reach, because it proves the choice survived the wire.

⚠ The picker binds `[selected]` on the option, not `[value]` on the select. The
first version bound the select and the spec caught it: an Angular select cannot
mirror React's controlled value, and every sibling select in this modal already
binds the same way.

#### 2026-08-31 — feat(memory): the cheap-LLM fallback chain, and the switch that governs the profile-less routes (P4.D135 unit 4)

_Versions: core 0.0.705, harness 0.0.607._

`services/cheap_llm_fallback.rs` — v4's `lib/memory/cheap-llm-tasks/fallback.ts`.
The cheap path speaks a different currency from the Salon (a `CheapLlmSelection`,
not a connection profile), so this converts between the two rather than growing
a second engine; the two paths drifting apart is the trap the feature was warned
about. A failed cheap task now walks the route's chain, re-issuing against each
stand-in with a FRESH deadline — the previous route spent its budget without
answering, and charging the understudy for that would guarantee it fails too.

The `allowCheapFallback` half landed with the schema in unit 1; this is what
reads it. A route with no profile behind it — a pure-local Ollama pick, a
provider-cheapest synthesis — has nothing to hang a chain on, so it is governed
by that one off-profile switch and draws a stand-in from the user's `isCheap`
profiles.

v4 reaches its ambient `getRepositories()`; v5 needs an explicit handle, and
`CheapLlmLogConfig` already carries the `Db` and the user id, so `with_logging`
supplies both and none of the 60-odd call sites change. ⚠ `CheapLlmTaskExecutor
::new()` therefore has no chain — the two production sites that use it (image
prompt expansion, one enclave leg) keep the pre-4.10 behaviour. A named gap.

New family `cheap_llm_fallback_equivalence` (7 cases, five mutation proofs) over
the settings fixture, comparing SELECTIONS — the conversion is where the two
currencies meet. It had to walk around two vacuity traps, both measured rather
than reasoned: a profile-less route stands in a synthetic failed profile with no
model class, and `tierMatches` reads unknown-vs-known as a non-match, so a
*classified* cheap profile is never drafted and the switch's two positions both
answered `[]`; and v4's `jest.setup.ts` mocks the whole DB stack while
`safeQuery` swallows the failure, so every seeding write quietly no-ops and the
case goes green having measured a world it never built.

#### 2026-08-31 — feat(chat): the fallback chain on the Salon spine, both entrances (P4.D135 unit 3)

_Versions: core 0.0.704, harness 0.0.606._

The chain reaches the turn. `run_primary_stream`'s catch-all — which rethrew
unconditionally — now walks the profile's chain before it preserves and
rethrows; the empty-response recovery gains its THIRD step, after the
same-provider retry and the uncensored reroute; and both stream through the new
`failing-over` SSE stage.

`FallbackChainRepos` (`services/fallback_repos.rs`) is the shared read seam: the
engine's two lookups plus the one a walk needs on top, an understudy's API key,
because v4 records a resolution failure as an `auth` attempt rather than
silently skipping the candidate. It holds a `Db` and does its own `read_main`
per question — a borrowed connection cannot be held across an await, and this is
what lets the engine stay synchronous and driveable from an in-memory `Vec`.

**Three things the differential caught that inspection had not:**

1. **`65f5021c8` also added `characterId` to `restreamInto`'s call.** Every
   failover leg's `CHAT_MESSAGE` row now carries the character where it used to
   carry NULL — invisible in the results and the event traces, caught by the
   `llm_logs` dump.
2. **v5 was forwarding two params v4 never sends.** `restreamInto` builds its
   `streamMessage` call by hand and names neither `previousResponseId` (v4's
   comment: handing an OpenAI chaining token to a different account is
   meaningless at best) nor, on the empty-response legs, `stop`. v5 cloned the
   primary's whole `StreamParams`, so it carried both — a pre-existing
   divergence the tier-3 corpus cannot see, because its canned key is
   `provider|model|temperature|messages`. `stop` is now an explicit argument
   (`None` = v4's empty-response shape, the primary's sequences on the chain
   legs) and the pin is a recording provider.
3. **The buffer reset survived its first mutation.** No case dirtied the buffers
   before a swap, so deleting `reset_streaming_buffers_for_swap` went green. The
   corpus gained a primary that emits REASONING and then dies —
   `hasStartedStreaming` stays false, so the chain runs — with the reasoning
   buffers as comparands. v4 resets to the empty string, not to absent, and the
   port now does too.

Corpus: `primary-stream-tier3` grows three profiles + three api keys (the chain
reads REAL rows) and six cases — the understudy answering, an exhausted chain
whose summary reaches the rethrown error, the mid-stream skip, a non-eligible
token limit, the empty-response third recovery, and the dirty-buffer swap. Seven
mutation proofs. `turn_orchestrator`'s `ChainConfig` needed no change: v4 deleted
`maxRetries`/`retryDelayMs` in this commit and v5 never ported them.

#### 2026-08-31 — feat(llm): the pure fallback engine, tier-1 against v4's real modules (P4.D135 unit 2)

_Versions: core 0.0.703, harness 0.0.605._

`quilltap_core::llm_fallback` — v4's `lib/llm/fallback/` whole: the trigger
classifier, the chain builder, the tier picker, `recordAttempt` and
`summarizeFallbackAttempts`. Pure; no call site wired yet.

Two shape decisions the port had to make and the corpus then settled:

**The error input.** v4 classifies an `unknown` by walking `instanceof`, then
`error.name`, then `error.message`. v5 has no error-class hierarchy at the
stream seam — `StreamError` carries a message and nothing else — so
`FallbackError` names those three inputs explicitly and a caller supplies
whichever it holds. The oracle emits the OBSERVED `(name, message)` pair rather
than a constructor label, so both classifiers work from the same two strings.
`record_attempt` takes `Option<&str>`, because v4's
`String(error ?? 'unknown error')` arm is reachable only from a `throw null` —
which an `Error` with an empty message must NOT be confused with, and
`String(error ?? '')` alone renders the two identically.

**The repo seam.** v4's `buildFallbackChain` is async because its reads are;
v5's are synchronous over a borrowed connection, so `FallbackRepos` is a
synchronous trait and the chain is built inside one read and walked afterwards.

The differential (`fallback_engine_equivalence`, 155 cases) drives v4's real
modules over v4's own two test files' shapes plus the arms they do not reach.
It does two things v4's tests deliberately do not:

- **it initializes the provider registry with the ten real dist plugins.** v4's
  tests mock `providerCanTransportImages`; an UNINITIALIZED registry is worse
  than a mock, because it silently changes the verdict — `getConfigRequirements`
  returns undefined, `requiresApiKey` defaults to `true` "for safety", and a
  keyless Ollama candidate is skipped for want of a key it never needed. That
  divergence is exactly what the first run reported, and initializing is what
  makes both sides answer the same production question;
- **both capability answers are their own comparands** (`transport` rows for
  `providerCanTransportImages`, `apiKeyCapability` rows for the
  `acceptsApiKey`/`requiresApiKey` pair), so a registry-vs-mirror disagreement
  fails by name instead of surfacing as a mysteriously wrong pick.

Eleven mutation proofs: each of the five classifier non-triggers (typed
token/content, message-matched token/content, tool-unsupported, ZodError,
unattributed 4xx), the named understudy's vision filter, the danger filter it
deliberately does NOT have, the picker's different-provider preference, the
unknown-vs-known tier rule, the "no tier replacement qualified" tail, and a
chain that recurses one level.

`LOG_LEVEL=error` is load-bearing in the recipe: the engine logs through v4's
real logger, which writes JSON to stdout and would interleave log records into
the NDJSON. The reader refuses such a line by name rather than dying on a
missing field.

#### 2026-08-31 — feat(db): the connection-profile fallback columns, their CRUD, and both id-remap paths (P4.D135 unit 1)

_Versions: core 0.0.702, harness 0.0.604, host 0.0.84._

The substrate half of v4's provider/model fallback chains (`65f5021c8`): the two
`connection_profiles` columns end to end, before the engine that reads them.

`fresh_schema.json` is re-dumped from the pin's live `generateDDL`. It moved in
two places, only one of which the work order predicted: `connection_profiles`
gains `"fallbackProfileId" TEXT` + `"allowTierFallback" INTEGER DEFAULT 0`, and
`chat_settings`'s `cheapLLMSettings` DEFAULT grows `"allowCheapFallback":false`.
The column POSITION is not where the order said either — v4's hand-written
`sqlite-initial-schema.ts` inserts the pair after `multiCharacterPrefill`, but
generateDDL (the shape a fresh instance actually gets) places it after
`modelClass`, which is the Zod declaration order. The re-dumped
`schema-key-order.json` agrees, and the export key-order guard now pins the slot
by name.

Unusually, the two DDL shapes AGREE on the defaults here (no DEFAULT on the TEXT
column, `DEFAULT 0` on the INTEGER one), so omitting a column and writing its
default land the same cell — which is why the create names both unconditionally
instead of carrying `multiCharacterPrefill`'s three-state omission. The boot
ensure (`db::connection_profiles_fallback_repair`) reproduces v4's migration
`add-profile-fallback-fields-v1` including its per-column guard, so a
half-migrated table heals on the next boot.

Also landed: the create/update route gates byte-for-byte (the self-reference and
Courier refusals, the non-boolean 400s, and v4's guard ORDER — the create checks
`allowTierFallback` before it checks that the profile has a name at all); the
cleared-null echo for `fallbackProfileId`; the delete cascade, which releases the
deleted profile from every other profile that named it — **and stamps their
`updatedAt`, because v4 releases through `updateMany`, which always folds the
timestamp into its `$set`**; `seedLegacyConnectionProfileFields`'s two new seeds
plus its self-reference strip (whose gate is JS-truthy, so an empty string is
neither compared nor cleared); `fallbackProfileId` in the backup remapper's
scalar list; and its remap in the `.qtap` reconcile pass, where a forward
reference — a profile naming an understudy that lands later in the bundle — is
the whole reason the pass exists.

`allowCheapFallback` lands as far as the settings substrate: the seed row, the
`CheapLLMSettingsSchema` normalizer (appended LAST, as v4 appended it to the
schema), and the chat-settings route's boolean guard, which unlike its two enum
neighbours is not truthiness-gated and so refuses an explicit `null`.

Differentials: `provisioning_equivalence` (the D23 tripwire fired on both moved
defaults, as designed), `connection_profiles_tier2_equivalence` (corpus 11 → 18
ops, incl. the two-namer delete cascade with a movement-asserting normalizer for
the wall-clock release stamp), `connection_profile_legacy_fields_equivalence`
(306 → 414 cases, a separate `fallback` block so every pre-existing row stays
byte-identical), `settings_routes_equivalence` (26 → 43 connection-profile arms
+ three `allowCheapFallback` arms; the fixture gains a Courier profile and seeds
one profile naming another), `backup_uuid_remap_equivalence` (a new
`fallback_understudy_links` corpus case), and `restore_vintage_state` (the
committed migration-vintage instance replayed at the pin, plus a new tripwire
that names a stale fixture directly instead of leaving it to surface as a pile
of `has no column named` warnings).

Two committed fixtures needed the new INSERT: the migration-vintage instance was
re-replayed from the pin (it exists to move when v4's migrations move), and
`web_search_runner_wire`'s pre-4.10 `chat-admin-main.db` seed now runs the boot
ensure first, the same repaired-at-boot idiom the vintage family uses for
`linkGroupId`.

The `.qtap` reconcile remap is pinned at the UNIT tier: `system_import_state`'s
connection-profile leg has been vacuous since v4 `aa464abf` (the committed
`system-data-main.db` predates `multiCharacterPrefill`, so both sides fail
identically and the arms stay green on matching failures), and the two new
columns land in the same hole. Widening that fixture is cross-lane; the gap is
named in both files.
#### 2026-08-31 — fix(doc-tools): name the missing argument, and match past the file's punctuation (v4 bugs 108, 109)

_Versions: core 0.0.703, harness 0.0.605._

The enablement half of v4 `487ae16b1`, and bug 108 whole.

**Bug 108 — the call, not the file.** The tool dispatcher deliberately hands a
handler its raw arguments (that is what lets a `qtap://` URI stand in for
scope/mount/path), so a `doc_str_replace` that simply omitted `find` reached the
matcher as an absent needle, matched nothing, and was answered "Text not found
in file… use the exact text from your most recent read". A model reading that
re-reads and repeats the identical malformed call. v5 measurably had this shape;
the differential ran red on it before the fix.

`handle_str_replace` now checks `find` and `replace` **before the file is
opened** — before path resolution and the write-permission check, so a
write-blocked file, an unknown mount and a non-text extension all still answer
the argument sentence. `find` must be a non-empty string, and the message says
`empty` or `missing` accordingly. `replace` is guarded by type, not truthiness:
`''` is a legitimate deletion, and without the type test an omitted `replace`
silently deleted the found span. `handle_insert_text` gains the same guards for
`position` and `content`; an array `position` still falls through to v4's own
"Position must specify before, after, or at", being truthy and `typeof 'object'`.

**Bug 109 — enablement.** The typographic fold is turned on for
`doc_str_replace`, for `doc_insert_text`'s anchor, and for `doc_grep`'s
**literal** path. Grep folds unconditionally — there is no uniqueness contract
to protect and a character searching for `Veyra-5's` should find the sentence.
The regex path is deliberately untouched: there the caller is spelling the
pattern themselves.

A typographic-tier match splices the replacement over the **original** span, so
the caller dictates the new bytes of the passage it named and cannot reach
beyond it. The success message says when a match needed the fold, and an
ambiguity the fold produced is reported differently from one the bytes produced.

`doc_text_equivalence` grew 20 ops (43 → 46 with the ordering trio) and
`doc_enum_equivalence` grew the grep trio that makes the fold's scope
measurable: the literal query finds the curly file, the regex and un-normalized
queries do not. Both families regenerated from the `487ae16b1` pin; nine
mutations, each reddening exactly the arm it should.

Quilltap's own typography did not move.

#### 2026-08-31 — feat(doc-edit): fold typographic spellings for matching (v4 bug 109, part 1)

_Versions: core 0.0.702, harness 0.0.604._

Ports v4's `lib/doc-edit/typographic-folding.ts` (new at `487ae16b1`) and the
rebuilt `lib/doc-edit/diacritics.ts` that composes with it. Models write curly
punctuation of their own accord and Quilltap stores it faithfully; a later turn
retypes the sentence with a straight apostrophe and the byte-exact match fails.

The new `doc_edit::typographic_folding` module holds the 25-entry fold table —
the quote family onto `'` and `"`, the dash family onto `-`, `…` onto `...`, and
the non-breaking/wide spaces onto U+0020 — transcribed in v4's source order.
Zero-width characters and guillemets are deliberately excluded.

`DiacriticsMatchOptions` gains `fold_typography` (default false, so every
existing caller keeps byte-exact semantics), and `find_unique_match` now returns
the `MatchTier` that answered: it runs **exact first and folds only on a total
miss**, so a file carrying both spellings still resolves to the one the caller
typed rather than going ambiguous. More than one exact match is an answer, not a
miss, and never consults the fold.

The searched string and its position map are now built from ONE per-character
function, so a length-changing fold (`…` → `...`, one character to three) maps
back to the original span correctly. This closes the whole-string/per-unit seam
the module header used to document.

Enablement at the three tool sites is a separate change; nothing folds yet.

`doc_edit_leaves_equivalence` grew the bug-109 corpus: the fold table compared
entry-for-entry and in order, 10 fold cases, 19 match cases, and v4's replay
shape as an executable assertion — five typographic failures resolve, twenty-five
genuinely stale find texts still miss. 167 rows against v4's real `lib/doc-edit`
at the `487ae16b1` pin.

#### 2026-08-31 — docs(orders): write the drift catch-up round 1 of 2 — P4.D134 ∥ (P4.D135 → P4.D136 stacked) ∥ P4.D137

_Docs-only change._

Four work orders covering the contiguous drift prefix `1560bd43b`..
`7fb668263` (eight of the fifteen pending commits; the baseline moves to
`7fb668263` when the round unifies). P4.D134: the Lima/WSL2 retirement
across the six ported surfaces (host env/lock, CLI, data-dir wire with the
`isVM` key deletion, host-rewrite collapse, almanack/self-inventory, SPA
About/footer/profile) plus the `7fb668263` About Discord-link rider and
the `7819afb1d`/`3c3432ae9` NO-PORT ratifications. P4.D135: the
provider/model fallback chains whole — the D23 re-dump for the two
mid-table `connection_profiles` columns, the pure fallback engine, the
four integration sites, both id-remap paths, the delete-nulls cascade,
and the SPA mirrors. P4.D136 (stacked on D135's branch): bugs 106/107 —
the reroute's message-array re-decision + the
`profileCanReceiveAttachment` consolidation, and the cheap-LLM budget
rewrite with the latency-class split (the superseded 75 s dogfood C4 row
retired per ledger §5.5). P4.D137: doc-edit bugs 108/109 — the argument
guards and the typographic-folding two-tier matcher. The remaining seven
rows (the three-commit LoRA train, `qt-range`, bug 112, the Concierge
four-state, `e41fcb12e`) are round 2 by design: they stack on this
round's output or collide with its lanes (`tools/self_inventory.rs`, the
spine quartet, the job handlers). Ledger §3 rows marked ORDERED.

#### 2026-08-31 — docs(drift): record four more v4 commits — bug 112, the Concierge four-state, the HuggingFace LoRA lookup

_Docs-only change._

A full `/driftcheck`, one day after the last. v4 `main` is now fifteen
commits past the `b121ac77f` oracle baseline — four new since 2026-08-30.
`bugfix` is still unmoved at `3a76b17df`; the checkout is clean on `main`;
the regen rule stays PIN REQUIRED.

`735d9408c` (bug 112, v4-filed) is PORT and lands on surfaces this port just
rebuilt: `lastMessageAt` now moves only when a character — the user or an
LLM — posts content, via a new chokepoint predicate
(`isCharacterAuthoredMessage` + its SQL mirror), with every chat-list sort,
both write sites, deletion recompute, restore/import re-derivation, and a
recompute migration behind it. That hits v5's `db/chats_messages.rs`, the
P4.65 `ChatListPreloaded` Salon-list batching, P4.64's `services/home.rs`,
the characters/projects/brahma routes, self-inventory, and the SPA chat
cards.

`60e3c4a0a` is PORT: the per-chat Concierge control grows to four states
(`conciergeOverride` admits `'UNCENSORED'`; `'OFF'` relabeled Vouched Safe).
No DDL change — the migration is ledger-only — but the whole
chat-override/manual-flip/resolver family v5 ported reshapes
(`isChatActiveDangerous` → `shouldUseUncensoredRoute`, new flip kinds, new
Concierge notice sentences) across ~14 ported call sites, plus the Salon
sidebar control.

`2ece98c90` is PORT-NEW, the LoRA train's third commit (D-stack with
`84f33ce94` → `648d5c8aa`): a HuggingFace lookup for LoRA sources — the
repo's second mocked non-LLM external HTTP provider after Serper — behind
`POST /api/v1/image-profiles?action=lora-metadata`, deliberately rendering
no compatibility verdict. `e41fcb12e` is a docs/help-only NO-PORT? candidate
whose help hunks bank to `p4.9i2`.

#### 2026-08-30 — docs(drift): record five more v4 commits — LoRA adapters, doc-tool matching, qt-range

_Docs-only change._

A full `/driftcheck`, one day after the last. v4 `main` is now eleven commits
past the `b121ac77f` oracle baseline — five new since 2026-08-29. `bugfix` is
still unmoved at `3a76b17df`; the checkout is clean on `main`; the regen rule
stays PIN REQUIRED. The backlog is now large enough to need more than one
catch-up round.

`487ae16b1` (bugs 108/109) is PORT and the one v4 says this port inherits:
`doc_str_replace` handlers now guard their arguments before opening the file
(`replace` by `typeof`, since `''` is a legitimate deletion), and a new
typographic-folding pass runs after an exact miss so a document's curly
punctuation no longer defeats an edit the model retyped straight. It rebuilds
`lib/doc-edit/diacritics.ts`, which v5 ported file-for-file.

`84f33ce94` is PORT-NEW: LoRA adapters stored under a reserved key in the
existing `parameters` bag, per-model provider option resolution through the
same `matchModel` as orientation, and `appliesToModels` promoted from reserved
to honoured. It also consolidates five image-parameter call sites that had
drifted apart — three of them read only `quality` off the profile, so profile
settings worked for `generate_image` and vanished for avatars, story
backgrounds, the images route and the wardrobe preview. Worth measuring v5 for
the same drift. `648d5c8aa` (bugs 110/111) stacks on it.

`5f56f7a7d` defines `qt-range`, a class v4 referenced and never had — the
inert-name family's third instance, and v5 has the same ten range-input hosts
with no such class. `7fb668263` is a Discord link that reaches the ported About
mirror.

#### 2026-08-29 — docs(drift): record four new v4 commits — fallback chains and bugs 106/107

_Docs-only change._

A full `/driftcheck`. v4 `main` is now six commits past the `b121ac77f` oracle
baseline — four of them new since the 2026-08-27 check. `bugfix` is unmoved at
`3a76b17df`; the checkout is clean on `main`; the regen rule stays PIN REQUIRED.

Two of the four are real ports on heavily-ported surfaces. `65f5021c8`
(provider/model fallback chains) is PORT-NEW: two `connection_profiles` columns
inserted mid-table by `generateDDL` and appended by the migration, a new pure
`lib/llm/fallback/` engine, a new `failing-over` SSE stage, four integration
sites across the chat spine and the cheap-LLM path, `allowCheapFallback` on
`CheapLLMSettings`, and `fallbackProfileId` remapping in both id-rewriting
paths. `a1d88aa3a` (bugs 106/107) is PORT: bug 106's message-array
re-decision — which v4's own bug row says any port swapping the model mid-turn
inherits — plus the consolidation of three "can this profile receive this
attachment?" spellings into one predicate, and bug 107's cheap-LLM budget
rewrite. The other two are docs-only NO-PORT candidates.

Bug 107 supersedes the 75 s compression budget this port landed at P4.D127 and
the dogfood pass closed as C4 PARTIAL two days ago: the background ceiling
moves to 120 s, the shared tier to 90 s, and a new latency class keeps the
interactive legs at their old values. Recorded under the ledger's §5.5
proof-expiry rule — re-measure, don't carry the old numbers forward.

CLAUDE.md's baseline bullet had restated the drift count in defiance of its own
"never restate it here" rule and gone stale within hours; the restatement is
removed rather than refreshed.

#### 2026-08-29 — docs(dogfood): record finding #107 — the Markdown toolbar overflows its column

_Docs-only change._

Reported from the New Chat dialog's Starting Scenario field: the formatting
toolbar's buttons extend past the writing column on both sides. Recorded rather
than fixed — the run was closing.

The both-sides symmetry is the diagnostic. A block overflow spills right only;
equal overhang means justify-content: center on a flex row wider than its
container. The CSS is a faithful port — v5's .qt-formatting-toolbar is
byte-identical to v4's, flex items-center justify-center gap-2 with no wrap and
no max-width. The divergence is the enclosing box: v5 interposes
<qt-markdown-field>, whose host class has no rule anywhere in
apps/web/src/styles/, so it renders at display: inline and constrains nothing.
v4 has no such wrapper.

This is the third instance of the family, after finding #97 (qt-tab-view) and
the Almanack walk's qt-entity-tabs, across 20 non-spec call sites. The standing
note now proposes closing the class rather than the instance: assert that every
component whose host declares a qt- class has a matching CSS rule, in the
check-qt-classes idiom. The likely fix is a block display on the host, but it
wants verification across a sample of the 20 sites since some may depend on the
current shrink-to-content behaviour.

#### 2026-08-29 — docs(dogfood): close C4 partial — the 75 s compression budget, with two measurement corrections

_Docs-only change._

Closes the walk's last non-deferred row with a deliberately bounded claim,
taking it to 21 PASS / 1 deferred and sixteen live proofs discharged.

Proven live: production selects the 75 s branch. Three v5-written
CONTEXT_COMPRESSION calls (30,080 / 26,633 / 25,459 ms) ran against the remote
NANOGPT cheap LLM, which is the arm where cheap_llm_deadline_for returns the
override rather than the local 175 s or the shared 40 s default.

Not provable by gesture, and recorded as such: the 40-75 s discriminating band
(below 40 s both budgets succeed, and the band is provider-latency luck at 18
of 397 historical calls) and the "[CheapLLM] Task failed" warn, which needs a
call over 75 s when the maximum ever observed across 400 real calls is 67.7 s.
Both are unit-proven in cheap_llm_exec.rs.

Two corrections banked as a memory note, both of which sent the human after
the wrong thing before being measured. Compression fires on context pressure
(compressible_tokens > max_available * 0.50), not conversation length — the
first target's characters sat on 1,024,000-token windows, ten times over the
bar, so no number of turns could have fired it; the profile is character-level,
there being no chat-level or participant-level connection profile for salon
turns. And duration does not track prompt size: 13,013 ms at 287 KB against
30,080 ms at 242 KB, with prompt sizes clustering regardless of chat volume.

#### 2026-08-29 — docs(dogfood): record finding #106 — the duplicate optimistic user bubble

_Docs-only change._

Reported from a real multi-character turn on the Friday copy: the user's own
message renders twice for most of the turn — in its chronological place and
again at the transcript foot — collapsing to one when the turn ends. Recorded
rather than fixed, at the human's direction; the fix is lane-sized.

v4 pushes the optimistic bubble into the message array itself, so any refetch
that replaces the array removes it and v4 structurally cannot show both. v5
holds it in a separate signal appended unconditionally at render and clears it
only at the turn-end reconcile point. That divergence was latent until
P4.D123-D125 began publishing scoped chat hints on per-turn job completion, so
the chat is now refetched mid-turn and the persisted user row arrives while the
optimistic bubble is still standing. CHAT_DANGER_CLASSIFICATION alone completed
six times in four minutes during the reporting session.

Recorded with it as a standing note: the entire Playwright suite is green
through this defect, because every beat asserts the transcript after the turn
completes and the bug exists only during it. No beat observes the mid-turn
window, which is how a regression on the SPA's most-used screen survived a full
round and a 22-row dogfood walk over the same component. The owning lane should
treat that gesture as a deliverable in its own right.

#### 2026-08-28 — docs(dogfood): close C5 — bug 104's glm-5.3 vision send proven on real data

_Docs-only change._

A 1.8 MB `image/jpeg` attached to a chat on the existing `Z.AI GLM 5.3 Flash`
profile was described correctly, taking the walk to 20 PASS / 2 deferred and
fifteen live proofs discharged.

The server-side chain rules out the describe-fallback: the user message at
`04:53:52`-minus-30s carries the file, the only completion in the window is
`Z_AI`/`glm-5.3-flash`/`CHAT_MESSAGE` at 25,821 ms, and there are zero
`IMAGE_DESCRIPTION` rows after a call 31 minutes earlier. So a model whose id
carries no `v` read the image directly — the case v4's plugin dropped before
1.1.24, and the reason bug 104's fix deleted Z.AI's private vision-model list
outright.

Recorded with it: `llm_logs.request` cannot evidence a vision send either way.
It is a pre-builder projection with message content flattened to strings, so a
search for `image_url` correctly returns zero whether or not the image reached
the wire.

#### 2026-08-28 — docs(dogfood): close A9 — `instances restore-key` proven with the real pepper

_Docs-only change._

The one row the agent-driven pass reserved for the human ran on 2026-08-28
and passed, taking the walk to 19 PASS / 3 deferred. Run with the server down
and the instance lock released, and deliberately without `--force`, so the
proof arm the agent's run had to skip actually executed: all three partitions
answered `opens with this pepper` before anything was written, the `.dbkey`
was rewritten at mode 0600, and a subsequent `quilltap db` read back 42
characters. No `.bak` rotation line, correctly — the previous file had been
moved aside rather than overwritten.

Recorded alongside it: the command refuses while the instance lock is held,
so it must run with the server stopped (the agent initially got that ordering
wrong), and passing the pepper as an inline environment prefix lands it in
shell history — the exposure the CLI's own help cites as its reason for never
accepting it as a flag.

#### 2026-08-27 — docs(dogfood): the P4.D131-round pass — 18 PASS, zero v5 defects, thirteen 💸 items discharged

_Docs-only change._

A 22-row walk over the Friday copy covering the P4.D131 round in full plus
the backlog from three earlier rounds. Eighteen PASS (one partial, stated),
four deferred to the human, **no v5 defects found**.

Discharged: the Salon chat list at real scale (779 chats, 4.1 MB, 1.34 s
against the 8.6–12.2 s P4.64 measured pre-batching) and `systemHome`
(0.31 s against 8.8 s); the whole tooltip vertical, plus three branches the
plan never listed — `focusin` opens at 13 ms against the 200 ms hover dwell,
`focusout` closes, outside-pointerdown dismisses a pinned bubble; the net-NEW
ConfirmationBadge over a measured population of 5,736 real confirmations; the
`try_decrypt` IV-length guard end-to-end through the CLI with no pepper; all
four realtime items including pushed invalidation proven by discriminator and
the terminal WS origin gate correct on all eight arms against a real PTY; the
two hover fills; the About strings; all three completion templates
byte-identical to v4's real launcher plus a live TAB; and P4.D130's outfit
pull-down and garments-only slot pickers.

Four observations that looked like defects were each chased to a root cause
and none was real — two were v5 being correctly v4-faithful, two were
instrument error. Adds three memory notes and three standing notes to
`dogfood-findings.md`; discharges that file's 7.5 s `systemHome` note.

#### 2026-08-27 — docs(porting): drift check — v4 retires Lima/WSL2 (2 commits past `b121ac77f`)

_Docs-only change._

The `/dogfood` freshness probe came back stale; this is the full `/driftcheck`
that followed. v4 `main` is two commits past the oracle baseline and the
checkout is clean on `main`.

`1560bd43b` (refactor(runtime): drop Lima and WSL2 support, Docker is now the
sandbox) is a real **PORT** row touching six already-ported v5 surfaces: the
instance-lock `EnvironmentType` and its user-visible lock sentences (host
`env.rs`/`lock.rs` plus the CLI's `is_vm_environment`), `lib/paths.ts` and the
`/api/v1/system/data-dir` response — v4 **deletes** the `isVM` key, which the
`data_dir_paths_equivalence` family compares and the SPA Profile screen reads —
the host-rewrite gateway cascade (five strategies to two, with
`isVMEnvironment()` changing meaning to `isDocker || QUILLTAP_HOST_IP`), the
bug-56 base-path availability check, the Almanack `runtimeType` union, and the
`self_inventory` runtime-mode union with its two prompt-visible labels. The
About prose, footer `BackendMode`, CLI lock helpers, and two help pages ride
along; `lima/`, the rootfs build script, CI, Docker and the unported plugin
packages are the NO-PORT remainder.

`7819afb1d` (fix(ci): the restore-key suite couldn't find the SQLCipher binding
on CI) is a **NO-PORT?** candidate — jest mock plumbing, README, changelog and
version bumps, zero `lib/`/`app/` hunks — for the very surface P4.D133 ported.

The regen rule flips back to **PIN REQUIRED**: every oracle regen runs from a
worktree pinned at `b121ac77f` until a catch-up round moves the baseline.

#### 2026-08-27 — docs(porting): the P4.D131 ∥ P4.D132 ∥ P4.D133 ∥ P4.65 round unification — baseline → b121ac77f

_Versions: core 0.0.701, harness 0.0.603, cli 0.0.16, web 0.0.100, SPA 0.5.596._

All four lanes unified on `unify/p4d131-round`; the oracle baseline moves
`aec86a613` → `b121ac77f` and the four-commit drift debt is cleared. The
bug-105 divergence arm retired on a measured FULL convergence; the Tooltip
primitive + nine-button adoption + the net-new ConfirmationBadge landed
with two live beats; `instances restore-key` landed whole (Tier R 188 →
212, red-first); the Salon chat-list gained v4's `ChatListPreloaded`
batching, payload-proven byte-identical on the Friday copy at ~5.7×.

The §3 review found no blocking findings inside any lane; the unified
Playwright gate then caught the round's would-have-shipped defect — the
widened salon fixture's broken-vault chat sorted FIRST and its broken
character became the archive seeder's copy template, breaking seven
beats — repaired fixture-side (Ridge Reunion pinned oldest, the seeder
tie-breaks by id) with zero product code. Also fixed on the unify
branch: the `try_decrypt` IV-length panic, the fixture sort-key
fragility (pinned `lastMessageAt`, loud builder throw), three
v4-fidelity gaps on the action bar (Delete danger chrome, swipe
disabled utilities, the counter's `2/3` bytes), and a stale comment.

Gate: 473 test binaries / 2,585 / 0 with the round's env block; Tier R
212/0 from the pin; ten family regens fresh at their pins, zero SKIP;
clippy both feature sets; release build; ng 366 files / 5,458; full
Playwright 255 passed / 0 failed / 1 skipped (the standing store-probe
park). Round record: `status-log.md`.

#### 2026-08-27 — harness(import): retire the bug-105 divergence arm — v4 converged at 679e450e3 (P4.D131)

_Versions: core 0.0.699, harness 0.0.603._

v4 fixed its bug 105 (one malformed connection profile aborted a whole
`.qtap` import) at `679e450e3`, exactly as this port's filing described:
the legacy-field seeding call moved inside the per-item try, and the
helper now type-tests the provider instead of relying on `??`. A fresh
oracle regen from a worktree pinned at `b121ac77f` measured FULL
convergence — v4 now answers `success: true` with exactly one warning
naming `Bug 105 Connection`, imports the `Bug 105 Survivor` image
profile, and writes no connection profile — byte-for-byte what v5's leg
of the `execute_bug105_seed_abort` case asserted all along (v5 never had
the bug, per the standing 2026-08-03 backup/restore/import ruling).

The divergence machinery is retired: `classify_bug105_seed_abort`, its
blanked comparand, and the `main.image_profiles` table-skip threaded
through `compare_execute`/`normalize_side` are deleted (the `skip`
parameter is removed outright rather than kept as an always-empty
capability). The case stays in the corpus as a plain-equality regression
guard — 37 cases, state-compared like every other — and is
mutation-proven in both the warning-sentence and survivor-write
directions. The `profiles.rs` divergence doc block is rewritten as a
convergence record. No v5 production behavior changed; the core bump is
doc-comment-only. The one v4-side line this commit does not port —
`help/system-import-export.md`'s new sentence — banks to `p4.9i2`.
#### 2026-08-27 — docs(porting): the P4.D133 lane record — restore-key landed whole

_Docs-only change._

The lane record for P4.D133 appended to the status log: all Tier 1 and Tier 2
deliverables landed (red-first completion/help copy, the core seams, the verb
end-to-end, 24 Tier R arms taking the differential 188 → 212 / 0, the
archive-note arm, the coverage-guard mutation proof). Deferred loud: the
real-pepper recovery walk on a Friday copy (💸 dogfood queue) and the NO-PORT
remainder for the unifier's ratification. Lane gate: fmt clean, clippy both
feature sets, 473 test binaries / 2,565 / 0 with the differential confirmed
run.

#### 2026-08-27 — port(cli): the instances restore-key verb with its Tier R arms

_Versions: cli 0.0.16._

The `quilltap instances restore-key <name>` verb (alias `rebuild-key`) ported
whole from v4 `b121ac77f`'s `dbkey-restore.js`, as its own module: pepper from
`ENCRYPTION_MASTER_PEPPER` or the hidden prompt (never a flag), the 44-char
warning, the three-database proof step before anything is written (the
refusal is unwaivable while an encrypted database exists; `--force` only
covers a fresh or still-plaintext instance), the write lock with a Drop guard
as v4's `finally` (errors surface through the instances handler's
`Error: <msg>` + exit 1, not `db --write`'s bare print), the same-pepper
rewrap vs the different-pepper WARNING+confirm, the passphrase precedence
chain, the timestamped backup, the unknown-field-preserving write, read-back
verification with restore-on-mismatch, the registry stored-passphrase update,
and the four-line ARCHIVE-bundles note under v4's exact predicate. Every
user-facing string byte-exact against v4's real launcher.

Tier R grew 188 → 212: 22 output-diffed arms (happy path, alias, wrong
pepper — the cross-engine `file is not a database` byte risk verified — the
non-TTY pepper refusal, both `--data-dir` spellings with the registry-scan
name recovery, unprovable declined/accepted/forced, the plaintext note, the
stale-keyfile pair, lock contention on a live sleeper, instB set/clear with
the archive note) plus two state-compared blocks: the new-passphrase re-wrap
(registry bytes identical, written key ORDER identical, both sides'
files unwrap under the new passphrase and refuse the old — the file can never
byte-match, fresh salt/IV per wrap) and the planted-`minServerVersion` carry.
`CaseOpts` gained `normalize_bak` (the backup stamp is the one run-time
truth) and the child env scrubs `ENCRYPTION_MASTER_PEPPER`. Pure pieces
unit-pinned: header classification, the backup-stamp shape, the
`passphrase_changed` truth table, the node path helpers.

#### 2026-08-27 — port(core): the .dbkey restore-key seams — raw read, tryDecrypt, carry-preserving write

_Versions: core 0.0.699._

`quilltap-core::dbkey` gains the three public seams v4's new CLI
`packages/quilltap/lib/dbkey.js` needs (v4 `b121ac77f`), with no duplicated
crypto: `read_dbkey_raw` (v4 `readDbKeyFile` — strips the legacy
`hasPassphrase` flag and rewrites the file), `try_decrypt_pepper` (v4
`tryDecryptDbKey` — any failure is None), and `save_dbkey_preserving` (v4
`preserveExtraFields` + `writeDbKeyFile` — the ten fresh wrapper fields first,
carried extras appended in the existing file's order; deliberately a different
key-order shape from `rewrap_dbkey_json`'s in-place server re-wrap, because
the two v4 sites build the object differently). The `rewrap_dbkey_json` doc
comment's recorded v4-drop divergence is rescoped to v4's SERVER re-wrap
explicitly — v4's new CLI restore path DOES preserve, so the old wording
over-claimed. Four new unit pins: extras survive and land appended (order
asserted both ways), the `hasPassphrase` strip rewrites the file, the
try-decrypt edges, and the fresh-write arm.

#### 2026-08-27 — port(cli): the instances restore-key completion templates + help text, red-first

_Versions: cli 0.0.15._

The P4.D133 lane's first unit: v4 `b121ac77f`'s completion-template and help
changes byte-copied from a pinned worktree, red-first. Against the pinned v4
with v5 unchanged, the Tier R differential went red on exactly the four
predicted cases (`instances help` + `completion bash|zsh|fish`; 188 cases, 4
failures — the P4.D118/P4.D128 signature); after the copy it is 188/0. The
three shell templates are `cmp`-identical to v4's; `instances_help.txt` was
captured from v4's running launcher (`quilltap instances --help`) rather than
extracted from the source literal, which would have left `\\` unrendered. The
P4.D128 coverage guard now enforces the five new flags; a mutation dropping
`--no-passphrase` from the fish template reddens it (proven, restored).
`--force` cannot discriminate template-wide because `docs` already carries it —
v4's own guard coarseness, recorded in the lane record.
#### 2026-08-27 — port(chat-list): the ChatListPreloaded batching threaded through the list enrichment (P4.65)

_Versions: core 0.0.700, harness 0.0.603, web 0.0.99._

The Salon chat list stops paying 8.6–12.2 s at real scale: v4's
`ChatListPreloaded` maps are now built in `enrich_chats_for_list` (one
collection pass, `characters.findByIds` first seeding the avatar-id set,
then the five remaining batched reads in v4's exact order) and threaded
through `enrich_chat_for_list` / `enrichParticipantSummary` /
`getCharacterSummary` with v4's preload-preferred/fallback shape. The
`_for_list` vault-only twins are retired — the preloaded avatar branch now
carries v4's real two-step (the vault-link map, then the story-background
files map). Payload identity proven two ways: the `salon_reads_equivalence`
family over a pin-fresh oracle (8 cases + a 30-object key-order pin), and
the real-scale leg on the Friday copy — the `listChats` dispatch payload
byte-identical before and after (4,104,806 bytes, md5
`1ef288a15da550c0625ec74a8bc4e557`, `cmp` clean) with enrichment at
12,984/8,256 ms → 2,227/1,451 ms. One named behavior CONVERGENCE: a
participant whose character vault is unavailable now answers
`character: null` (v4's batched drop) where v5's per-row read answered a
StoreUnavailable error — pinned by the widened fixture's broken-vault
character. The committed salon fixture DBs regenerated from the pinned v4
worktree with a third chat (cross-row dedup, distinct sort keys),
character-level tags, a vault-link avatar hit, and the broken vault; the
new `list_exclude_character_tag` case pins the participant `_allTagIds`
arm; the three sibling salon families and `home_routes_equivalence` re-ran
green over pin-fresh oracles through the sweep driver. Eight source
mutations (reverse sort, each preload map dropped, a silent-absorb
fallback) each reddened the family.

#### 2026-08-27 — port(db): the four batched read paths for the chat-list preload (P4.65)

_Versions: core 0.0.699._

The substrate for v4's `ChatListPreloaded` batching: `files::find_by_ids`,
the generic store-backed `find_by_ids`/`find_by_ids_raw` (hydrated form
drops unavailable-store rows, per v4) with `projects::find_by_ids` on top,
`doc_mount_file_links::find_by_ids_with_content` (first-occurrence id
dedup, per v4), and `conversation_chunks::count_by_chat_ids` (the GROUP BY
twin of `count_stats_by_chat_id`). Every new `IN (…)` — plus the existing
`memories_read::count_by_chat_ids`, which gains its first production
caller — is chunked at the 900-id budget (`chunk::chunk_array`; v4 does
not chunk these reads — a scale-safety measure invisible in output, after
P4.D126's 40,000-id "too many SQL variables" failure). Twenty unit tests
beside the twins, including a past-the-32,766-variable-ceiling chunking
proof per site; un-chunking all five sites reddened exactly those five
proofs.

#### 2026-08-27 — docs(porting): the drift catch-up + chat-list-batching round ordered — P4.D131 ∥ P4.D132 ∥ P4.D133 ∥ P4.65
#### 2026-08-27 — port(spa): the tooltip live beats + the committed emit recorder

_Versions: SPA 0.5.595._

The P4.D132 closing commit. A new `salon-tooltips-flow.spec.ts` walks the
ported surface live: hovering an action-bar button grows the body-portalled
bubble after the 200 ms dwell with v4's copy (and the icons row carries no
`title` attribute anywhere), and the seeded AMENDED confirmation badge pins
its structured note on click (`data-pinned`), survives the pointer leaving,
and dismisses on Escape. The verdict is seeded as an UPDATE onto the
existing tool-flow assistant row rather than a new message — Solo Voyage is
shared by ~20 specs and a new bottom bubble would move the chat's last row
under all of them. The parity-table emission recorder is committed as
`harness/oracle/cases/tooltip-strings.test.tsx` (the `text-transforms`
precedent: renders v4's REAL MessageActionBar/ConfirmationBadge/Tooltip
under v4's own jest; the committed copy's emission re-proven byte-identical
from the pinned worktree; note the `.tsx` twist — the /tmp mirror dir needs
its own `node_modules` symlink for `react/jsx-runtime`), and the three v5
parity-spec headers point at it. One downstream gesture fixed: the
destructive re-attribute beat clicked its button by the old accessible
name, which unit 2's copy fix changed.

#### 2026-08-27 — port(spa): drop the dead desktop-actions CSS (v4 1b0ce9eba)

_Versions: SPA 0.5.594._

The v5 share of v4 `1b0ce9eba` (deletion-only — v5 never ported the
always-hidden `MessageDesktopActions` component itself): the three dead
transcribed `display: none !important` rules leave `_chat.css`
(`.qt-chat-desktop-hover-actions`, `.qt-chat-message-desktop-actions`, and
`.qt-chat-desktop-timestamp`, all grep-confirmed template-unused), leaving
the icon action bar's `display: flex !important` in place. The MessageRow
copy-choice docblock and the message-row spec's test name — both of which
cited the now-deleted `MessageDesktopActions.tsx:73` — are rewritten as
history naming the deleting sha, and the MessageActionBar cite moves to the
post-commit `:178`.

#### 2026-08-27 — port(spa): the answer-confirmation badge — a real pinnable button

_Versions: SPA 0.5.593._

The ConfirmationBadge lands in v5 for the first time (only its CSS had ever
been transcribed): v4 `ConfirmationBadge.tsx` in its post-`0bd841394` form —
a real `type="button"` that takes keyboard focus, wrapped in a pinnable /
interactive `qt-tooltip` gated on `hasDetail` (notes or original present),
with the four verdict states (vouched ✓ / amended ✎ / stood-by ! /
unvetted —), the structured bubble (title, summary, "What looked off",
"Originally written", the pin hint), and the `spoken` aria-label join — all
strings byte-for-byte from the emitted table. Mounted in the action bar
after the LLM-logs entry (v4 puts it before Resend, which v5 lacks). The
`_chat.css` base rule gains v4's button-reset + transition and the new
hover/focus-visible/state/user-bubble rules in v4's order; the section
banner no longer claims the title holds the notes. Data plumbing: the
stream→bubble mapper now carries `confirmationOriginalContent` (the fifth
family field it used to drop — absent on every live frame, v4-faithfully,
since `confirmationResult` never carries the pre-revision text; the reducer
leg was re-measured against v4's `applyConfirmationResult` and is already
faithful). Eleven new specs: the six mirrors of v4's badge tests, the
emitted state-tuple and bubble-structure pins, and the mapper-thread pin.
Mutation-proven: the checked gate, the revised branch, the pin gate, and
the mapper line each redden the right specs.

#### 2026-08-27 — port(spa): the action bar adopts qt-tooltip on all nine buttons

_Versions: SPA 0.5.592._

The v5 half of v4 `0bd841394`'s MessageActionBar adoption: every `title=`
attribute in the salon message action bar is gone, each of v5's nine buttons
is wrapped in `qt-tooltip` with v4's content string, and each keeps its
explicit `aria-label` (a tooltip is not an accessible name). The
re-attribute copy takes v4's new wording ("Re-attribute to **a** different
participant" — v5 had carried the old string), and the Save-image content
keeps v4's conditional plural against a fixed aria-label. The three v4
buttons v5 never had (Collapse-this-message, View source/View rendered,
Resend) are pre-existing gaps recorded in the lane record, not this
commit's scope; v5's Delete-after-LLM-logs button order is likewise
recorded, not churned. New parity specs pin the (content, aria-label)
pairs for both roles against the table emitted from v4's REAL component at
the pinned worktree, plus a no-`title`-anywhere pin; the LLM-logs
copy-choice spec now reads the bubble. Mutation-proven: the old
re-attribute wording and a reintroduced `title` each redden the specs.

#### 2026-08-27 — port(spa): the Tooltip primitive — Quilltap draws its own tooltips

_Versions: SPA 0.5.591._

The v5 half of v4 `0bd841394`'s foundation: `app/ui/tooltip.ts`, an Angular
port of v4's new `components/ui/Tooltip.tsx`. The component host is the
`qt-tooltip-anchor` (v4's wrapper span); the bubble renders under `@if` and an
`afterRenderEffect` reparents it onto `document.body` (v4's `createPortal`),
with the reparented node removed by hand on close — Angular tears the embedded
view down against the container it created the nodes in, so a moved node
otherwise outlives its view. Faithful contract: 200 ms dwell / immediate open
on `focusin` (React's delegated `onFocus`), 120 ms close grace, top/bottom
flip + viewport clamp with v4's margins (8 px viewport, 6 px anchor gap),
rAF-coalesced scroll/resize follow with capture-phase scroll, Escape closes,
`pinnable` click-to-pin with outside-pointerdown dismissal, `interactive`
lets the pointer enter the bubble, identity-stable coords, `aria-hidden`
bubble with the accessible name staying on the trigger. The `_surfaces.css`
`.qt-tooltip` rule is rewritten to v4's post-commit form (fixed, `z-[70]`,
max-width, border color-mix, popover shadow, `pre-line`, pointer-events
gating) and the new `qt-tooltip-anchor`/`-body`/`-title`/`-section`/
`-section-label`/`-quote`/`-hint` family lands in v4's order. Ten specs: the
five mirrors of v4's `tooltip.test.tsx` Tooltip block plus five
emitted-constant pins (constants emitted from v4's real source at the pinned
worktree — see the spec header's regen recipe). Mutation-proven: the flip
inversion, the broken focus-immediate, and an ANCHOR_GAP nudge each redden
the right specs; the closeSoon inner pinned-guard mutation survives because
every pinning path also clears the timer — v4's own defensive redundancy,
recorded rather than vacuously pinned.

_Docs-only change._

Four work orders for the next round, planned from fresh v4 surveys (the
freshness probe passed; the ledger's four UNPROCESSED rows are marked
ORDERED in the same commit). P4.D131 retires the bug-105 divergence arm to
a plain equality now that v4 converged at `679e450e3` — measurement-led per
ledger §5.4, zero production-source change expected. P4.D132 ports v4's
Tooltip primitive (`0bd841394`) with its action-bar adoption and the
answer-confirmation badge — which the survey found v5 never ported at all
(only its CSS was transcribed, and the stream→bubble mapper drops
`confirmationOriginalContent`) — plus the `1b0ce9eba` deletion rider.
P4.D133 ports the CLI `instances restore-key` verb (`b121ac77f`) Tier R
red-first; the ledger's flagged human decision is resolved in the order —
the write is sandbox-provable through the existing `reset_live` fixture
mechanism, and only the real-pepper recovery walk stays human-only,
banked 💸. P4.65 ports v4's `ChatListPreloaded` batching into the Salon
chat list (P4.64's measured 8.6–12.2 s deferral), payload-identity
disciplined, with the drop-vs-503 unavailable-vault convergence named and
pinned rather than landed silently. CLAUDE.md's stale drift restatement
(three commits / `1b0ce9eba`) was trimmed to defer to the ledger, which
already recorded four.

#### 2026-08-27 — docs(drift): v4 adds a CLI `.dbkey` rebuild — DRIFT PENDING at four commits

_Docs-only change._

A full drift check from the main checkout. v4 main moved once more, to
`b121ac77f` — `quilltap instances restore-key <name>` (alias `rebuild-key`),
which rebuilds a lost or passphrase-locked `.dbkey` from the pepper with the
server down: pepper from `ENCRYPTION_MASTER_PEPPER` or a hidden prompt and
never a flag, proved against every encrypted database on disk before anything
is written, lock-gated, the old key file backed up, and unknown fields
(`minServerVersion`) carried across. Classified PORT-NEW and the largest of
the four pending commits.

Its intersections are delineated in the ledger row: v5's ported
`instances_cmd.rs` verb family and the Tier R help-text differential; the
byte-copied completion templates in all three shells (a red-first item in the
bug-101 / P4.D128 idiom, five new flags plus the verb lists); and
`quilltap-core::dbkey`, which already has the whole write side and P4.46's
unknown-field preservation — with the caveat that the recorded v4-drop
divergence concerns v4's *server* re-wrap, untouched here. The
`db-helpers.loadDbKey` / `instances.verifyPassphrase` consolidation into the
new `packages/quilltap/lib/dbkey.js` was verified behavior-preserving from
the hunks, so it owes v5 nothing. Flagged for the human: the write itself has
no sandbox-safe live proof.

Also re-measured this check: `bugfix` unmoved at `3a76b17df` (by content, per
§4 step 2), `release` at `8736d7042` and fully contained in main, still no
4.9.0 squash, and the checkout is now CLEAN — the docs-only dirt recorded at
the last probe was the in-flight work that became `b121ac77f`. Verdict:
DRIFT PENDING — 4 unprocessed portable commits; regen rule unchanged at PIN
REQUIRED, `aec86a613`.

#### 2026-08-27 — port(round): the P4.D130 ∥ P4.62 ∥ P4.63 ∥ P4.64 unification — the outfit pull-down, the collapse pockets closed whole, the home dashboard 22× faster

_Versions: core 0.0.698, harness 0.0.602, web 0.0.98, SPA 0.5.590._

All four orders closed; the oracle baseline moves `8872d7efc` →
`aec86a613`. The `aec86a613` outfit pull-down landed whole in the SPA
(composed-outfit pool split with an ICU-collation recorded-vector corpus,
the capture-phase-Escape pull-down, garments-only slot pickers, a live
dissolution beat) plus both carried wardrobe e2e debts — the missing
`instance_settings` materialization (create-scope beat LIVE) and the
duplicate-"Quilltap General" root cause (the courier seeding, NOT the
provisioner; reconciled by what each store holds). P4.62 adjudicated the
last three wrong-type-collapse pockets site by site (13+7+1 — zero census
rows remain unadjudicated; two new DB-free-over-real-HTTP families; the
whole system/unlock body gate and the per-action malformed-body 500s
restored to v4's bytes). P4.63 closed the four harness follow-ups (the
bug-105 divergence arm — which v4 then fixed HOURS later, so the arm's
scheduled convergence trip is already booked; the attach-mount-file red
diagnosed to bug-91 corpus vintage and re-lit, canned calls 0 → 4; the
deadline-warn assert bound to its exact line; both blob censuses
comment-aware). P4.64 profiled the 7.5 s dashboard, refuted the standing
hypothesis (97% was the enrichment fan-out — a dropped-preload port
defect, not the findAll loads), and landed the payload-identical
sort-then-slice fix: byte-equal at real scale, 8.8 s → 0.39 s; the Salon
list's matching cost is the named next candidate (`ChatListPreloaded`
batching). The §3 review found no blocking findings. Gate: 473 test
binaries / 2,557 / 0 with five pin-fresh families zero SKIP; clippy both
feature sets; release build; ng 364 files / 5,435; full Playwright
**253 passed / 0 failed / 1 skipped (5.8 m)** — the suite grew with the pull-down beat and the un-parked create-scope half; the one skip is the component-transfer beat re-parked on its REAL blocker (the missing `projects`/`groups` tables — named, P4.D130). v4 drifted three times during the round
(`679e450e3` convergence, `0bd841394` tooltips, `1b0ce9eba` cleanup) —
all recorded in the drift ledger, every regen pinned; the catch-up is the
next round's top candidate.


#### 2026-08-27 — docs(drift): the mid-unify probe — v4 moved twice more and the checkout is dirty

_Docs-only change._

The four-lane round's unification probe failed against the morning's
check: v4 main picked up `679e450e3` (a CONVERGENCE — v4 fixing bug 105,
this port's own filing; it lands the very divergence P4.63's new
`system_import_state` oracle arm pins, so that arm flips by design at the
round that absorbs it) and `0bd841394` (PORT-NEW — a body-portalled
`Tooltip.tsx` adopted by the message action bar's eleven buttons and the
now-pinnable answer-confirmation badge), and the checkout went dirty in
`app/salon/` with in-progress work continuing the same surface. Ledger §1
and §3 updated; regen rule stays PIN REQUIRED.
#### 2026-08-27 — fix(web): adjudicate the three wrong-type-collapse pockets — reportId's 404, the zod concurrency envelope, the mount-write schema

_Versions: harness 0.0.599, web 0.0.98._

P4.62 closes the census pockets P4.60 deferred: all thirteen
`and_then(Value::as_*)` sites in `system_data_routes.rs`, the seven closure-form
sites in `files_routes.rs`, and the one in `llm_logs_routes.rs`. Each was read
against v4's real route first, then measured. Sixteen are FAITHFUL — v4 itself
collapses those shapes, either through a coercion (`typeof x === 'number' ? x :
0.80`) or by refusing every non-matching shape with one sentence. Five were
divergent and are fixed.

`?action=capabilities-report-delete`: v4's `if (!reportId)` is JS falsiness, so
a truthy non-string (`true`, `123`, `{}`, `[]`) passes the gate and then fails
the `f.id === reportId` lookup — v4 answers **404 Report not found**, where the
collapse answered 400.

`?action=job-concurrency`: v4 answers `validationError(...)`, the two-key
`{error:'Validation error', details:[…zod issues]}` envelope. The edge answered
an invented flat sentence with no `details`; it now reproduces Zod 4's issue
objects arm for arm, and validates before the pump check the way v4 does.

The `capabilities-report-generate` progressId gate accepted UUIDs Zod refuses:
`Uuid::parse_str` ignores the RFC version and variant nibbles that Zod 4's
`z.uuid()` enforces. It runs Zod's own regex now.

The mount-file PUT's JSON leg gets v4's `writeBodySchema` whole: it had invented
a `content is required` sentence and then silently accepted an unknown
`encoding`, a negative or fractional `expected_mtime`, and a string `force` —
all of which v4 refuses. The upload leg's `tags` part now splits v4's three
outcomes (unparseable → 400, truthy non-array → the `.map` TypeError's 500,
falsy → no tags). The `?action=link` leg refused an empty `fileId` too late and
answered its 400 ahead of v4's chat-404.

Neighbours found by reading and fixed with them: each tools action's malformed-
body answer (a 500 with the leg's own sentence, except `job-concurrency`, whose
`.catch(() => ({}))` makes it a 400), and `system/unlock`'s missing-action
sentence plus its `Request body must be a JSON object` gate, which was absent
entirely — a body of `42` rode through to a passphrase change.

The link fix's first shape rewrote only a `File not found` answer, and the
pre-existing `files_write_routes` beat caught it: where the file lookup errors
rather than resolving nothing, the 400 was lost as a 500. When the fileId is
invalid v4 never reaches its lookup, so the edge now passes the chat-404 through
and answers 400 for everything else.

Two new differentials over real HTTP against v4's real handlers:
`system_body_guards_equivalence` (55 route arms + 15 progressId arms) and
`files_body_guards_equivalence` (36 arms), both from oracles pinned at
`8872d7efc`. Fifteen mutations, each reddening exactly one family. The census
guard's counts and prose now carry every verdict; nothing in it is deferred.

One escalation, recorded not fixed: a wrong-typed `tagId` (`[{"tagId": 5}]`) is
carried by v4 into `linkedTo` as the raw value, where v5 drops it. Closing it
needs `Request::FileUpload.tags` widened past `Vec<String>` in
`quilltap-core/src/api/types.rs`, outside this lane's ownership.
#### 2026-08-27 — docs(porting): P4.63's gate numbers, measured at the committed state

_Docs-only change._

The lane record's gate section, filled in from the run at the committed
versions: clippy exit 0 on both feature sets, `cargo test --workspace` 471
binaries / 2,555 passed / 0 failed, and both owned families confirmed to have
run inside the suite rather than skipped.

#### 2026-08-27 — test(harness): the bug-105 oracle divergence arm — v4 aborts a whole import, v5 names the item

_Versions: harness 0.0.601._

`system_import_state` gains `execute_bug105_seed_abort`, the oracle-side
tripwire P4.D126 named as a follow-up. v4 `e000d6bfc` reads
`(seeded.provider ?? '').toUpperCase()` at the top of
`importConnectionProfiles`' loop body, outside the per-item try, so a
non-string `provider` throws past the loop and aborts the entire import; v5
names the item and carries on, under the standing 2026-08-03 import ruling.

The payload is one malformed connection profile followed by one sound IMAGE
profile — `executeImport` runs tags, connection profiles, then image
profiles, so reaching the image profile at all is what "continued" means.
v4 answers `success: false`, every count zero, one `Import failed: …`
sentence, and not a row written anywhere; v5 answers `success: true`, one
named warning, `imageProfiles: 1`, and exactly `Bug 105 Survivor` added.
Both legs are asserted, then the result body is blanked and
`main.image_profiles` subtracted from the comparands — inside the
normalization walk, so the minted-id labels stay aligned. Every other table
in all three partitions stays a plain equality. Additive: nothing in the
existing corpus moved.

Mutation-proven both ways: a v4-shaped guard in v5's importer reddens both
v5 legs, and an oracle edited to show v4 adopting the fix reddens the v4 leg
with its retirement instruction. That day is already scheduled — v4
committed the fix as `679e450e3` hours later — so the arm will trip at the
next baseline move past it, by design.

Recorded, not fixed: the committed `system-data-main.db` predates v4 4.9's
`connection_profiles.multiCharacterPrefill`, so every connection-profile
import in this family fails on both sides with a "no such column" error and
the arms stay green on matching failures. That import has measured nothing
since v4 `aa464abf`. Widening the fixture is cross-lane, so it is a
follow-up.

#### 2026-08-27 — fix(harness): describe through a transporting provider — the attach-mount-file corpus went dark at bug 91

_Versions: harness 0.0.600._

`attach_mount_file_equivalence` has been red at both pins since v4
`a14a1811`, failing its corpus-shape assert with "the oracle yields zero
canned vision calls". The cause is corpus vintage, not the port: the
fixture's four describer profiles were on `OPENAI_COMPATIBLE`, and bug 91
put `providerCanTransportImages()` in front of every
`describeImageWithProfile` attempt — the OpenAI-compatible plugin's shared
base declares no attachment support, so v4 began refusing before the
provider seam. The oracle recorded no vision calls,
`doc_mount_blobs.description` came back empty, and the `IMAGE_DESCRIPTION`
rows stopped being written.

v5 ports the same predicate (P4.D106) and refuses identically, which is why
the state diff could not see it: both engines went dark together and
compared equal. Only the canned-call count noticed.

The profiles move to `OPENAI`, a provider both transport tiers agree on —
v4's static mirror, which is what answers under jest where the plugin
registry is never initialized, and v5's baked manifest registry.
`OPENROUTER` was deliberately avoided (bug 97's two sources disagreed
there). The three `attach-file-*.db` files and the `.meta.json` sidecar were
rebuilt from the pinned baseline worktree; the family is their only
consumer. 13/13 cases green, four canned vision calls recorded.

Two additions so it cannot go dark quietly again: the canned-count assert
now names bug 91 and the both-tiers rule, and a new per-case pin asserts each
vision rung actually LOGGED its call — the only side of the diff that can
tell, since two engines writing nothing compare equal.

#### 2026-08-27 — test(harness): narrow the blob-registry exemption to per-site and make both censuses comment-aware

_Versions: harness 0.0.599._

Two precision gaps in `embedding_blob_binding_guard`, both of which made the
guard looser than it read.

`REGISTRY_ALLOWED` was a whole-FILE skip of `db/help_docs.rs`, so a real
`register_blob_columns()` could have grown inside the one module whose header
explains why no such mechanism may exist. It is now a per-site census —
`(path, needle, expected COMMENT count, why)` — and a CODE hit is refused
everywhere, help_docs.rs included: a comment may quote the grep, nothing may
call it.

Both censuses were bare `text.matches(needle).count()`, so a mention in a doc
comment counted as a call site — an encode could be deleted while a comment
kept the census green. A new `count_hits` splits CODE from COMMENT hits
(`/* … */` nesting counted; a `//` earlier on the line demotes the hit) and is
itself pinned by a seven-case table including nested blocks and a multi-byte
line. Its one failure direction is deliberate and documented: it under-counts
CODE, which reddens rather than passes.

Mutation-proven with the vacuity measured, not argued: a real
`fn register_blob_columns(…)` in help_docs.rs and a `memories.rs` whose two
encodes are respelled `float32_to_blob (v)` beside a comment naming
`float32_to_blob(` both pass the old guard and redden the new one; a second
prose mention of `register_blob` is refused by the per-site count.

#### 2026-08-27 — test(cheap-llm): bind the abandonment-warn assert to its own line and exact target

_Versions: core 0.0.697._

`a_fired_deadline_warns_and_writes_the_ruled_error_row` asserted the level and
target with `captured.contains("WARN quilltap::cheap_llm")`, which is a prefix
match — any sibling target (`quilltap::cheap_llm_exec`, …) satisfied it. The
field asserts were looser still: they ran against the whole capture, and this
test drives three events on `quilltap::cheap_llm` (the abandonment WARN,
`Cheap-LLM call failed`, `[CheapLLM] Task failed`), all carrying
`provider=`/`model=`/`character_id=`.

The target is now matched as a whole token — the trailing space is what ends
it — on the line that also carries the abandonment message, and every field is
asserted on that line. Measured rather than argued: retargeting the
abandonment warn to `quilltap::cheap_llm_exec` leaves the old assert GREEN and
reddens the new one. Test-only; no production behavior changed.
#### 2026-08-27 — perf(home): slice the recent chats before enriching them, not after

_Versions: core 0.0.697._

The landing dashboard took about nine seconds to build on a real instance
(773 conversations). Measuring it per step showed that ninety-seven percent
of that was one thing: the dashboard enriched every one of those 773
conversations — resolving each participant's character through the vault,
which costs eleven database reads and nine document parses per lookup,
roughly two thousand lookups in all — and then displayed the twelve most
recent and discarded the rest.

The order of those two operations is now reversed. The sort that picks the
twelve reads only raw conversation fields, so nothing the enrichment
produces can affect which twelve are chosen; sorting and slicing first and
enriching only the survivors therefore yields exactly the same twelve rows
in exactly the same order. On the real instance the dashboard payload came
back byte-for-byte identical (52,841 bytes, same checksum) while the time
fell from 8.87 s to 0.39 s — twenty-two times faster. The equivalence test
against the reference app is unchanged and green; its fixture carries
fourteen conversations against a twelve-row slice, so it does exercise the
reordering.

The comparator itself moved into a named function so the two places that
now depend on the same ordering cannot drift apart. Nothing else changed:
the reference app still enriches everything and slices afterwards, and the
one thing this can no longer do is fail the whole dashboard because of an
error raised by a conversation nobody was going to see — a case these reads
do not produce, since every read made for a discarded conversation is also
made for the twelve that are kept.
#### 2026-08-27 — test(wardrobe): the pull-down walked live, and the two carried e2e-fixture debts closed

_Versions: SPA 0.5.590._

A live beat for `aec86a613`'s whole contract: two garments in different
slots bundled into one composite, then the Top picker offers the garment and
not the composite, the pull-down lists it as `Top, Footwear`, and wearing it
there dissolves it across both slot rows with no bundle card and no
composite chip. Run RED first against a bundle built with the
`selectGarments` filter reverted.

Both carried wardrobe e2e-fixture debts are addressed with it, and neither
needed a committed fixture byte.

The "Shared — everywhere" create-scope beat parked because
`characters-main.db` carries no `instance_settings` table at all, so
`ensure_builtin_mounts` hit v4's own `sqliteTableExists` guard and skipped
provisioning entirely — the instance had no Quilltap General to write into.
`beforeAll` now materializes that empty table (the salon instance's own
precedent) and v5's real provisioning path mints the built-in stores at boot;
that beat's write half is live. Widening the committed pair instead would have
invalidated six harness families, the `quilltap-web` test venue and two e2e
specs; nothing was invalidated.

Lifting that park exposed a second, independent, pre-existing blocker on the
component-transfer beat, so it stays parked — but on the truth now.
`characters-main.db` has no `projects` and no `groups` either,
`enumerate_destinations` reads both, and one missing table fails the whole
verb, so `wardrobeTransferDestinations` 500s and the Move dialog's destination
select renders with no options at all. Measured server-side with no browser,
with and without the General store: identical failure. Materializing those two
tables does fix the verb, and then two beats written against the broken fetch
start failing — one asserts a character-destination count of zero that becomes
four once destinations actually load. That is its own scoped job; the beat now
probes the destinations verb instead of the General store and says why.

The duplicate "Quilltap General" P4.D122 recorded is not
`builtin_mounts.rs`. Measured on an isolated instance, provision-or-adopt
mints on the first boot and adopts on the second, leaving one store. The
cause is `seedCourierImagesFixture`, which copies the courier fixture's
whole `doc_mount_points` table — including its own Quilltap General and
Quilltap Uploads — while copying mount-partition tables only, so the
`instance_settings` pointers never arrive; provision-or-adopt is idempotent
by the pointer, not by name, so boot minted rivals. `global-setup.ts` now
reconciles each seeded built-in store by what it holds: unreferenced ones
are dropped and boot mints its own, referenced ones get the pointer so boot
adopts them. Measured: the courier General is referenced by nothing;
Uploads holds the ingested courier image, and dropping it would orphan that
image for the boot reaper.

#### 2026-08-27 — port(wardrobe): the pull-down goes live; the slot pickers list garments only

_Versions: SPA 0.5.589._

The composer mounts the `Wear an outfit…` pull-down above the bundle cards and
slot rows, and the per-slot `+` pickers now run their candidates through
`selectGarments` first. A three-slot ensemble used to appear once per slot it
covered, pushing the garments actually meant for the slot down the list; it now
appears exactly once, in the pull-down. Single-slot composites are listed there
too — it is their only route on. A multi-slot leaf (a dress typed
`["top","bottom"]`) is not a composite and stays in the slot pickers. The dead
`· composite` suffix is gone with them.

No new equip path: `(wear)` emits the composer's existing `addToSlot` output, so
the flag-driven rule the slot pickers already ran applies unchanged — an
additive bundle layers, one marked `replace` sweeps its slots first, and either
way it dissolves into components as it lands.

The slot row still receives `allItems` whole; only the candidates are filtered.
That is load-bearing and had no test until now: `groupEquippedSlots` promotes a
composite to a bundle card only at two-or-more occupied slots, so a *one*-slot
composite stays in the slot row and renders as a chip whose title comes from the
same list the picker draws from. Narrowing at the composer instead — the obvious
simplification — leaves every other spec green and makes that chip read
"unknown"; a new composer spec was written red against exactly that mutation.

#### 2026-08-27 — port(wardrobe): the "Wear an outfit…" pull-down component

_Versions: SPA 0.5.588._

An Angular port of v4's new `components/wardrobe/outfit-quick-pick.tsx`
(`aec86a613`, 138 lines): a full-width toggle with a rotating chevron over a
`role="listbox"` panel that lists every composite in the pool, title-sorted,
each row naming the slots it claims and whether it replaces them. Nothing hosts
it yet — the composer wiring follows.

Two details are load-bearing and are pinned by spec. The Escape handler is a
real capture-phase `document` listener that calls `stopPropagation`, so closing
the menu does not dismiss the enclosing wardrobe dialog with it — a template
`(document:keydown.escape)` binding is a bubble listener and would; a spec-level
dialog stub proves the difference. And where v4 returns `null` for a
composite-free pool, an Angular host element cannot leave the DOM, so it takes
the `hidden` attribute instead, which also drops it out of the composer's
`space-y-2` sibling chain and keeps the first bundle card's spacing v4's.

#### 2026-08-27 — port(wardrobe): the composed-outfit pool split, with v4's collation recorded

_Versions: SPA 0.5.587._

A client twin of v4's new `lib/wardrobe/composed-outfits.ts` (`aec86a613`):
`selectComposedOutfits` (composites, title-sorted) and `selectGarments` (the
complement, caller's order), both built on the existing client `isBundle` so
the outfit-vs-garment rule stays single-sourced. Nothing consumes them yet —
the composer and slot-row wiring follow.

Two equivalence checks ship with it. v4's own 69-line
`composed-outfits.test.ts` is transcribed 1:1 (same case names, same fixture
items, same expectations). Beyond that, a new oracle case
(`harness/oracle/cases/composed-outfits.test.ts`) drives v4's REAL module over
a nine-case corpus from a worktree pinned at `aec86a613` and records the result
as committed vectors, because the sort is `localeCompare` and no transcribed
case asks an ICU question. It matters: replacing `localeCompare` with a
code-unit compare leaves all seven transcribed cases green and reddens three
recorded ones.

#### 2026-08-27 — docs(orders): the four-lane round — the `aec86a613` pull-down drift, the collapse pockets, the harness follow-ups, the systemHome profile

_Docs-only change._

Four work orders for the next round, all lanes fully disjoint (the binding
ownership table is identical in each): **P4.D130**
(`p4.d130-outfit-quick-pick-spa.md`) ports the `aec86a613` outfit
pull-down into the SPA composer — the round's one drift commit, marked
ORDERED in the drift ledger — and carries the two wardrobe e2e-fixture
debts (the Quilltap General store widening that un-parks the
component-transfer beat, and the duplicate-General-store diagnosis);
**P4.62** (`p4.62-wrong-type-collapse-pockets.md`) adjudicates P4.60's
three deferred census pockets (`system_data_routes.rs` 13 sites,
`files_routes.rs` 5 caller-input, `llm_logs_routes.rs` 1); **P4.63**
(`p4.63-harness-differential-followups.md`) discharges the 4.9.0-push
round's four named harness follow-ups (the bug-105 oracle-side divergence
tripwire, the `attach_mount_file_equivalence` pre-existing red, the
deadline-warn prefix assert, the embedding-blob guard notes); **P4.64**
(`p4.64-systemhome-profile.md`) profiles the 7.5 s landing dashboard
measure-first, payload-identical by rule. Regen rule for every lane: PIN
REQUIRED (P4.D130's own family at `aec86a613`; everything else at the
`8872d7efc` baseline).

#### 2026-08-27 — docs(drift): record the `aec86a613` wardrobe outfit pull-down — DRIFT PENDING, pin required

_Docs-only change._

A `/driftcheck` run against v4. `main` has moved two commits past the
`8872d7efc` baseline: `b6c6d7793` (docs-only — this port's own bug-105
filing, carried as a NO-PORT? row for explicit ratification) and
`aec86a613`, a genuine PORT-NEW.

`aec86a613` adds a `Wear an outfit…` pull-down above the composer's slot
rows and removes composites from the per-slot pickers. Client-only: no
server verb, no schema, no wire change. Five pieces — a new pure
`lib/wardrobe/composed-outfits.ts` (`selectComposedOutfits` /
`selectGarments`, both built on the existing `isBundle`), a new
`outfit-quick-pick.tsx` component, the composer mounting it above the
bundle cards, `equipped-slot-row.tsx` filtering its candidates through
`selectGarments` and dropping the now-dead `· composite` suffix, plus
help/changelog/version riders. It lands on two already-ported SPA
components (`apps/web/src/app/wardrobe/outfit-composer.ts` and
`equipped-slot-row.ts`, P4.9f2 unit 3), and v5 measurably carries the
pre-fix shape at `equipped-slot-row.ts:100` and `:144`.

`bugfix` is unmoved at the inert `3a76b17df` fork marker; the checkout is
clean on `main`; v4 is at `4.9.0-dev.89` with no release squash yet. The
regen rule flips to **PIN REQUIRED** — build a detached worktree at
`8872d7efc` (drift-ledger §5.1) for every oracle regen until the baseline
moves.

#### 2026-08-27 — docs(porting): the 4.9.0-push round unification — baseline → 8872d7efc, drift debt cleared

_Versions at round close: core 0.0.696, harness 0.0.598, web 0.0.97, cli 0.0.14, SPA 0.5.586 (bumped across the round's commits; this commit is docs-only)._

The four-lane 4.9.0-push drift catch-up (P4.D126 memory/backup, P4.D127
provider/cheap-LLM, P4.D128 client/CLI, P4.D129 dedup-neutrality +
ratifications) unified onto main with the review-fixes and wires commits.
The oracle baseline moves `f3892158d` → `8872d7efc`; all fourteen drift
rows are absorbed or NO-PORT-ratified (drift-ledger §6). Headlines: the
`dcab791c2` dedup sweep proven neutral everywhere EXCEPT the measured
title-cleaner second-trim, which landed at the wires red-first; v4 bug
105 filed upstream (the bug-103 fix's own regression); the finding-#47
web-edge tripwire retired to a plain equality; the SQLite variable-limit
chunking, bug 103's seeding, bug 104's vision-list drop, the compression
budget, the completion flags, and the About strings all landed red-first
with their differentials.

Gate: fmt/clippy both feature sets/release build clean; the 15-family
pinned sweep 15/15 zero SKIP; `cargo test --workspace` 471 binaries /
2,554 / 0 with the round's env block; SPA lint (937 classes resolving) /
test (361 files, 5,399) / build clean; full Playwright 252 passed / 0
failed / 1 skipped (the standing store-probe park). Round record:
`docs/developer/porting/status-log.md`.

#### 2026-08-27 — chore(harness): refresh the uuid-remap corpus at the 4.9.0 baseline

_No crate versions bumped._

The `backup_uuid_remap_equivalence` deliberate-write corpus, regenerated at
the `8872d7efc` pin as the P4.D129 neutrality gap's closure: a
baseline-vs-target sandwich proved corpus AND oracle byte-identical across
the whole fourteen-commit drift block (so `dcab791c2` is neutral on this
surface), and the seven-line delta against the committed file is
pre-existing staleness — the `composerEmoji`/`composerUnicode`/
`smartTypographySettings` default keys v4 grew at 4.8.2, after the corpus's
last regeneration (P4.D62). Corpus and oracle move together per the
family's hash pin.

#### 2026-08-27 — port(wires): the dcab791c2 title second-trim, the #47 envelope retirement, the splice self-test pin

_No crate versions bumped (accumulated in the sibling review-fixes commit)._

The unification wires — the cross-lane obligations no single lane owned:

- **The `dcab791c2` non-neutral hunk lands in v5** (P4.D129's escalation 1):
  v4's dedup sweep collapsed the four inline title-generation cleaners onto
  `cleanTitle`, which trims a SECOND time after stripping the wrapping quote
  pair. v5's `clean_title` and `clean_generated_title`
  (`services/context_summary/tasks.rs`) carried the pre-sweep spelling —
  padding tucked inside quotes survived into the stored title, and the length
  cap measured before the trim. Both now second-trim, red-first at unit tier
  (two new arms failed against the pre-fix cleaners), and the
  `chat_regenerate_title_tier3` family gains the
  `regen_title_quoted_padded_inside` case on both sides so the oracle pins it
  end-to-end. `normalize_title`'s doc records the convergence.
- **The finding-#47 web-edge tripwire retired to a plain equality**
  (P4.D129's escalation 4): v4 adopted the corrupt-vault 503 refusal at
  `13ddc5ee`; the characters-update corpus arm was retired then, but
  `store_unavailable_envelope`'s twin was missed because the family SKIPs
  without its env var, and it fired at the P4.D129 neutrality sweep. Arm 2
  now compares both sides as equals against the recorded oracle answer
  (status, error, body-key shape).
- **The `--nocapture` splice gains its regression pin** (P4.D129's own review
  standard applied to it): the splice is extracted to a testable
  `splice_nocapture` helper and `--self-test` grows four arms — flat,
  continued (the P4.D129 bug shape asserted not to recur mid-continuation),
  and already-spliced.

#### 2026-08-27 — fix(review): the 4.9.0-push §3 review fixes — the hover pair, swapped fixture comments, stale cross-references

_Versions: core 0.0.696, harness 0.0.598, web 0.0.97, SPA 0.5.586._
(The web bump belongs to the sibling wires commit's `store_unavailable_envelope`
retirement; all four bumps ride here so the pair accumulates once.)

The unification review's findings across the four 4.9.0-push lanes, none
blocking:

- The two destructive-confirm buttons v4 styles with
  `hover:qt-bg-destructive/90` (`FileDeleteConfirmation.tsx:50`,
  `OrphanCleanupModal.tsx:60`) lacked the hover step in v5 — the same
  pre-existing hover-gap class P4.D128 closed at `file-preview-modal.ts:106`,
  found on the census's two unrecorded rows. Both closed.
- `restore_vintage_state.rs`: the "Both Predate" / "Carried Both" row comments
  were swapped (values were correct — they match the red-first table and the
  archive builder); an assert message carried a flattened multi-line literal's
  embedded spaces.
- `chat_completions.rs`: the NanoGPT twin's doc-comment still claimed the
  no-vision-arm shape as "the difference from `zai_user_content`" — no longer
  a difference since bug 104's port; reworded to record the convergence.
- Stale cross-references settled: `salon-conversation.ts`'s notice comments
  now name `useToolExecutionStatus.ts` (the v4 `487ae57fe` extraction),
  `folder_utils.rs` records v4's own deletion of `joinFolderPath`
  (`561466cfe`), and the `data-dir-paths` recipe pair drops its finished-round
  "PINNED worktree — the checkout is dirty" prose.

#### 2026-08-27 — docs(porting): the P4.D126 lane record — deferrals, out-of-scope families, and the gate

_Docs-only change._

Closes the P4.D126 lane record in `status-log.md`: the tier-3 deferrals (the
five help/docs prose rows banked to `p4.9i2` by name; the 💸 pre-4.9-archive
live proof queued for the next dogfood pass), the three out-of-§A families the
import-site change reached and this lane therefore re-ran, and the lane gate.

#### 2026-08-27 — fix(backup): seed the profile columns an older archive predates (bug 103)

_Versions: core 0.0.691, harness 0.0.594._

Ports v4 `e000d6bfc` (P4.D126 unit 3). Restore and `.qtap` import both rebuild
a connection profile from whatever the archive held, so a column the archive is
older than got no answer at all and the table DEFAULT decided a setting nobody
chose. New `services::connection_profile_legacy_fields` seeds both columns for
both paths: `supportsImageUpload` from the frozen historic provider map
(matched case-insensitively), `multiCharacterPrefill` as an explicit `null` —
the "never chosen" tri-state. A key the archive did carry is never touched, a
stored `false` and a stored `null` included. Both call sites debug-log when
seeding fires.

`CpCreate.multi_character_prefill` becomes `Option<Option<bool>>` so an
explicit NULL is expressible: omitting the column and writing NULL land the
same cell on a fresh (generateDDL) instance but not on a migrated one, where
`DEFAULT 1` turned the `[Name]` prefill on for every profile in a pre-4.9
backup, Anthropic included. Measured before the fix against the rebuilt
migration-vintage fixture: five of six profiles landed
`multiCharacterPrefill = 1` and two lost their vision flag. The import site's
private provider set retires into the shared module, which also fixes its
case-sensitive match.

Three things came out of the port. v5's restore defaulted `courierDeltaMode` to
`false` where v4's schema defaults it `true` — invisible until an archive
omitted the key. The committed `migration-vintage` fixture was rebuilt at the
pin: it predated v4's own `multiCharacterPrefill` migration, so its column set
no longer matched what its suite claims. And v4's fix reads
`(seeded.provider ?? '').toUpperCase()` outside the per-item try, so one
non-string `provider` now aborts a whole v4 import; v5 does not reproduce that,
under the standing backup/restore/import/export ruling.

New fixture `restore-archive-legacy-profiles.zip` (with its builder) carries the
six shapes; new tier-1 family `connection_profile_legacy_fields_equivalence`
drives v4's real helper over 306 cases.

#### 2026-08-26 — fix(memory): chunk batch memory deletion under the SQLite variable limit

_Versions: core 0.0.690._

Ports v4 `805ef12bf` (P4.D126 unit 2). Both batch-deletion sites built one
`IN (…)` list with one bind variable per id, so a batch past
`SQLITE_MAX_VARIABLE_NUMBER` failed the whole statement instead of deleting.
Measured before the fix on a 40,000-id batch: both
`MemoriesRepository::bulk_delete` and the doomed-id resolve inside
`delete_many_with_unlink` answered `too many SQL variables`. This bites
full-wipe restores and large character cascades on instances with tens of
thousands of memories.

New `quilltap_core::chunk` mirrors v4's `lib/utils/chunk.ts`: the
`SQLITE_VARIABLE_CHUNK_SIZE = 900` budget (safely under both the 999 of older
builds and the 32766 of current ones) and an order-preserving `chunk_array`
that refuses a zero size with v4's sentence. Both sites loop it — the
repository summing `deletedCount` per chunk, the resolve accumulating into the
same by-character map — so the grouping and the neighbour scrub are unchanged.

Pinned by v4's own shapes: the helper's order/empty/exact-multiple/2,000-id
cases, and site tests that put real rows in three different chunks of a
40,000-id batch (and either side of the first 900-boundary in a 2,000-id one).
Mutation-proven three ways: raising the budget over the ceiling, and
short-circuiting each site's loop after its first chunk.

#### 2026-08-26 — refactor(backup): route the full-wipe memory deletion through the memory-gate chokepoint

_Versions: core 0.0.689, harness 0.0.593._

Ports v4 `914b59e13` (P4.D126 unit 1). `delete_user_data` deleted memories
with a per-row `MemoriesRepository::delete` loop — the last direct bypass of
the deletion chokepoint. It now collects every doomed id across the user's
characters into one list and makes a single
`delete_many_with_unlink` call. v4's why-comment is carried: with the whole
corpus in one doomed set the neighbour scrub is a no-op (every candidate is
itself doomed), so the batch degrades to per-character bulk deletes instead
of N per-row statements.

The tier-2 count-map differential is blind to the routing by design — both
shapes delete exactly the same rows — so the pin is behavioural and lives
beside the family: `delete_all_routes_memory_deletion_through_the_chokepoint`
seeds a memory on a character the user does not own (so it survives the
wipe) whose `relatedMemoryIds` points at a doomed row, and asserts the edge
is scrubbed. That only happens through the chokepoint. Mutation-proven:
restoring the per-row loop leaves the edge dangling and the test fails.
#### 2026-08-27 — chore(logging): drop the per-publish realtime coalesce trace

_Versions: core 0.0.691._

Ports v4 `21f573039`, one deleted line. The `Realtime publish coalesced` debug
print fired on every publish that landed inside the 250 ms debounce window, and
job-status transitions pump that path: an `EMBEDDING_REINDEX_ALL` sweep of 1,000
jobs emitted one "queued", 999 "coalesced" and one flush line. The flush line
already reports the same absorbed total once per window, and with logs rolling
every 2-3 MB the per-absorb copy only evicted real diagnostics.

The `coalesced` counter itself stays — the flush line reads it. Log-only on both
halves, so no differential can see it: the pin is a capturing tracing layer
asserting silence at the publish site, exactly one surviving "queued" line, and
the flush line's exact `coalesced=11` after twelve publishes. That last value is
what separates "delete the trace" from "delete the counter with it"; both
mutations redden it.

#### 2026-08-27 — perf(cheap-llm): give compression its own budget, and log cheap-task failures

_Versions: core 0.0.690._

Ports v4 `8872d7efc`. Every cheap LLM task shared one 45 s deadline, 40 s of it
handed to the provider. Compression does not fit that shape: it carries the whole
conversation history, so it sends the largest prompt of any cheap task and sits at
the slow end of the distribution as a matter of course rather than as a stall.
Measured over three days on a live instance, compression supplied 13 of the 34
cheap calls that finished within five seconds of the provider budget — more than
any other task type — with a mean around 2.5x the cheap-task mean.

The three compression task types now get 75 s through a new
`CHEAP_LLM_TASK_TIMEOUT_OVERRIDES_MS` table; every other task keeps 45 s, and
local providers keep their 180 s regardless of task. The local check comes first,
so a per-task override can never shrink the local budget.
`cheap_llm_deadline_for` and `provider_budget_for` both take the task type now,
and it is threaded through `send_to_provider`'s `request_timeout_ms` and
`send_with_deadline` — so a remote compression attempt hands the provider 70 s and
abandons at 75 s, and its timeout message says `75000ms`.

A failed cheap-LLM task now warns with the task type, provider, model, chat and
character. The deadline path already logged when our own timer fired, but a
provider giving up on its own budget arrived as an ordinary provider error, and
the plugin's log line names the provider without naming the task — so a timed-out
extraction pass was invisible to a server-log grep.

v4's four test groups are mirrored (the three 75 s budgets, six named
non-compression tasks still on 45 s, the unknown/absent fallback, the local
exemption), plus real-path arms driving a stalling provider at 60 s and 100 s and
a capturing-layer pin on the warn. Five mutations — an override-key typo, the
local check reordered after the override, the warn deleted, and the task type
dropped at each of the two threading sites — each redden exactly the arms that
name them.

#### 2026-08-26 — fix(z-ai): drop the builder's private vision list so GLM 5.3 receives images (bug 104)

_Versions: core 0.0.689, harness 0.0.593._

Ports v4 `964ffb959` (Z.AI plugin 1.1.24). The Z.AI request builder kept its
own list of which GLM models read pictures, matching only ids with a `v`
immediately after the generation number (`glm-4.6v`, `glm-5v`). Z.AI's 5.3
line reads images without one, so `glm-5.3-flash` failed the regex and every
attachment was dropped before the wire with "Selected Z.AI model does not
support image input" — while the connection profile's `supportsImageUpload`
flag had already asserted the opposite and suppressed the describe-fallback.
The character never saw the picture, and the attachment-failure warning fired
on every turn that followed.

`is_zai_vision_model` and the refusal branch of `zai_user_content` are gone,
along with the now-unused model argument; the MIME check and the missing-data
check are the only remaining ways an attachment can fail. This restores bug
91's rule — whether the model reads images is the host's question, answered by
`supportsImageUpload`; whether the transport can send them is the builder's,
answered by the MIME list — and matches the shape NanoGPT took at P4.D106.

The proof is the committed request-envelopes corpus, regenerated from a v4
worktree pinned at `8872d7efc`: the two `image-attachment-non-vision` rows
flipped red-first (v5 produced the text-only body v4 no longer produces), a
new `image-attachment-glm-5-3` pair records the model that named the bug, and
all 339 other rows are byte-identical.
#### 2026-08-26 — docs(porting): the P4.D128 lane gate record

_Docs-only change._

The lane's gate on its final tree, and its three loud deferrals. `cargo fmt`
clean; clippy clean on both feature sets; `cargo test --workspace` 469 binaries
/ 2,517 passed / 0 failed; `completion_behavior` 7/7; Tier R by name against
the `8872d7efc` pin **188 cases, 0 failures**; `check-qt-classes` 937; `npm
test` 361 files / 5,399 / 0; `npm run build` clean.

Deferred loud: v4's `help/cli-completion.md` + `packages/quilltap/README.md`
prose to the `p4.9i2` bank; `packages/theme-storybook` recorded NO-PORT (no v5
analog); three 💸 dogfood rows named — a live `docs docker-mounts --format
<TAB>` in all three shells, the About bullet on screen, and the two new solid
hover fills on a real hover.

#### 2026-08-26 — feat(about): the 4.9.0 provider list and the Live interface bullet (v4 `8440b6391`)

_Versions: SPA 0.5.585._

P4.D128 unit 3 — the one code hunk of v4's 4.9.0 documentation-freshness sweep
(the commit's docs remainder is P4.D129's ratification, per the round's Shared
contract §D).

The Multi-provider bullet gains DeepSeek, Z.AI and NanoGPT — all three shipped
in v5 too — and a new **Live interface** bullet slots between "LLM tools" and
"Database protection", in v4's position.

v4's JSX writes the copy with HTML entities (`&mdash;`, `&ldquo;`/`&rdquo;`);
v5's about page stores plain strings, so the RENDERED characters are carried:
em-dashes and curly quotes, byte-for-byte what v4's browser shows.

The bullet says "socket" while v5 pushes the same invalidation hints over SSE —
the `f3892158d` round's locked mechanism divergence, already on the record.
The register is user-facing copy and it is v4's, verbatim; nothing is reworded.

Both strings and the bullet's position are spec-pinned (the about-page spec
idiom from the P4.D68 release-freshness mirror). Mutation-proven three ways:
reverting the provider sentence, deleting the bullet, and moving it after
"Database protection" each redden the new test and nothing else.

#### 2026-08-26 — test(cli): mirror v4's token-level completion coverage guard

_Versions: cli 0.0.14._

P4.D128 unit 2, tier 2. v4's `completion-coverage.test.js` grew two guards at
`57e7b1bc2` — the check that would have caught the four missing flags before
they shipped. Mirrored into `crates/quilltap-cli/tests/completion_behavior.rs`,
where both halves are in-crate and the guard needs no fixture.

- **`completions_offer_every_flag_the_help_text_advertises`** — every long flag
  named in a subcommand's own `--help` must be offered by all three templates.
  fish spells a flag `-l 'name'` (already an exact quoted token); bash and zsh
  are matched with a trailing-boundary rule, since `--max` is a prefix of
  `--max-nodes` and a plain substring test passes for a flag that is not there.
- **`bash_knows_which_docs_flags_take_a_value`** — bash cannot infer which flags
  swallow the next word, so its `vf_*` lists are compared, as whole
  space-delimited tokens, against the flags zsh declares with a `:value:` spec.
- **`help_sources_cover_every_dispatched_subcommand`** — v4 asserts its help-source
  map covers all twelve subcommands; v5 dispatches five and answers
  `not_yet_available` for the rest, so the v5 form parses `SUBCOMMANDS` out of
  `main.rs`, computes which have a real dispatch arm, and requires the map to
  equal exactly that set. Implementing another subcommand now fails this file
  until its help lands here too. (It earned its keep immediately: its first run
  caught a bug in this test's own `SUBCOMMANDS` parser.)

**Red-proven against the pre-fix templates**, and it names precisely what v4
named: `docs: bash template is missing ["--format"]`, `docs: zsh template is
missing ["--format"]`, `docs: fish template is missing ["--base64", "--format",
"--uri"]`. Three further mutations, each reddening exactly one test: dropping
`--format` from `vf_docs` alone (`do not list … ["--format"]`), dropping
`--max` from `vf_docs` while `--max-nodes` stays (`["--max"]` — v4's
review-bot finding, proving the token-set rather than substring comparison),
and dropping `'--max[…]'` from zsh's `docs_opts` while `--max-nodes` stays
(`zsh template is missing ["--max"]` — the same finding on the template scan).

#### 2026-08-26 — fix(cli): complete the four flags `--help` already documents (v4 `57e7b1bc2`)

_Versions: cli 0.0.13._

P4.D128 unit 2. v4's 4.9.0 release-checklist item 12 audited `--help` against
the three completion templates and found four documented flags tab-completion
never offered. v5 byte-copies those templates, so it had all four gaps.

- **bash** (+9/−2): `--format` appended to `vf_docs`, the value-flag list —
  without it `--format json <TAB>` reads `json` as the verb, bug 101's exact
  failure mode; a new `--format)` case completing `args json`; and `--format`
  appended to the `docs_flags` line-continuation block.
- **zsh** (+1): `'--format[For docker-mounts: output shape]:format:(args json)'`
  in `_quilltap_docs`.
- **fish** (+3): `docs --uri`, `docs --base64`, and the docker-mounts
  `--format` line gated on `__quilltap_using_subverb`.

All three files were byte-identical to v4's pre-fix templates and are now
byte-identical to v4's post-fix templates (`diff` against a worktree pinned at
`8872d7efc`, empty).

**Tier R, red-first.** Against the pinned v4 launcher the differential was
**188 cases, 3 failures** — `completion bash`, `completion zsh`,
`completion fish`, exactly the flipping set and nothing else. After the copy:
188 cases, 0 failures.

v4's `help/cli-completion.md` and `packages/quilltap/README.md` prose rewrites
ride the `p4.9i2` help-docs bank; no v5 analog is touched here.

#### 2026-08-26 — refactor(themes): the release-window qt-* utilities sweep (v4 `97d0b8f8e`)

_Versions: SPA 0.5.584._

P4.D128 unit 1. v4's 4.9.0 release-checklist item 7 converted 20 Tailwind class
sites across 15 components to semantic `qt-*` utilities and added the two solid
hover fills its stylesheet had been missing. Ported onto v5's twins.

**The two utilities.** `.hover\:qt-bg-primary` and `.hover\:qt-bg-success` —
the solid-fill siblings of `.hover\:qt-bg-destructive`, which v5 already had.
Tailwind generates no variants for classes declared in `@layer utilities`, so
an unwritten hover form is inert; both landed in v4's sibling positions with
v4's `var(--color-*)` bodies, and both blocks now match v4 byte-for-byte.

**The sweep, per site** (v4 site → v5 twin): 18 PORTED, 1 ALREADY-QT, 1
NO-SITE. `AuroraView.tsx:581` → `characters/list/character-card.ts`;
`CharacterHeader.tsx:279` → `characters/view/character-header.ts`;
`ProjectDetailHeader.tsx:81` → `prospero/cards/project-header.ts`;
`DeleteProjectDialog.tsx:35` → `prospero/project-delete-dialog.ts`;
`ChatCard.tsx:268` → `ui/scriptorium-badge.ts` (v5 extracted that ternary into
its own component); `FileDeleteConfirmation.tsx:50` →
`files/file-delete-confirmation.ts`; `OrphanCleanupModal.tsx:60` →
`files/orphan-cleanup-dialog.ts`; `DeletedImagePlaceholder.tsx:70` →
`images/deleted-image-placeholder.ts`; `GalleryImage.tsx` ×4 →
`characters/view/tabs/gallery-tab.ts`; `ImageMetadata.tsx:78` →
`images/image-metadata.ts`; `image-gallery.tsx` ×2 → `images/image-gallery.ts`;
`TaskFilters.tsx:140` → `settings/system/tasks-queue-card.ts`. Both
`memory-recall-card.tsx` checkboxes → `mt-1 qt-checkbox` (v5's class is defined
identically to v4's).

**ALREADY-QT:** `CharacterHeader.tsx:139`, the avatar placeholder v4 moved off
`bg-gray-300 dark:bg-slate-700` — v5's port had already written `qt-bg-muted`
there, so v4 converged onto v5 and nothing moved.

**NO-SITE:** `ChatCard.tsx:362`, the `actionType === 'delete'` branch. v5's
chat card implements only the `remove` overlay (recorded in its own docstring),
so the delete-action variant has no twin to convert.

**One visible shift carried:** `image-gallery.tsx:176`, the overlay's
`bg-black` + `bg-opacity-0/50` pair → `qt-bg-overlay-medium`, dropping the
redundant opacity pair (the element's own `opacity-0 group-hover:opacity-100`
fade already does that work). This was the SPA's only `bg-opacity` site.

**One pre-existing v5 gap closed on the way:** v4's `FilePreviewActions.tsx:108`
converts `hover:bg-destructive` to the qt- form on a delete button whose v5
twin (`files/file-preview-modal.ts`) carried no hover treatment at all — both
`hover:qt-bg-destructive` and `hover:qt-text-on-destructive` restored, and both
resolve in v5's stylesheet.

`check-qt-classes` reports 937 classes defined with every guarded reference
resolving (v4's post-change count is 920; the numbers differ by design and it
is resolution, not the count, that is asserted). Mutation-proven: deleting
either new utility makes the guard exit 1 and name the inert class and its
call site (`hover:qt-bg-primary` → `gallery-tab.ts:198`; `hover:qt-bg-success`
→ `gallery-tab.ts:183`). v4's `packages/theme-storybook` mirror has no v5
analog — recorded, not ported.
#### 2026-08-27 — fix(harness): repair nine sweep recipes and the driver's --nocapture splice

_Versions: harness 0.0.594._

Fallout from P4.D129's bulk neutrality sweep: nine families failed for
reasons with nothing to do with v4, and one of them was a defect in the
sweep driver itself. All nine are green after the repair.

The driver injects `-- --nocapture` into a family's run stage so a test
that SKIPs (missing env var) says so out loud instead of masquerading as a
green proof. It did that with `re.sub(r"(cargo test[^\n]*)$", …,
flags=re.M)`, and `$` under `re.M` is end of LINE — so a `cargo test`
spelled across backslash continuations took the flags after its first line,
which ends the continuation, hands cargo a bare `--nocapture` it rejects,
and runs the orphaned `--test <family>` line as a separate command. The
family then reports `run_failed` for a reason the recipe never caused. Now
the substitution consumes the continuation lines first, so the flags land
at the end of the whole command; proven on both the continued and the flat
shape, and `--self-test` still reports zero failures. This mattered beyond
`templates_equivalence`: any continued-recipe family had been running
without the SKIP-masquerade guard the injection exists to provide.

The eight recipe repairs: `data_dir_paths_equivalence` and
`profile_routes_equivalence` hard-coded `cd /tmp/qt-v4-baseline`, a lane pin
from a finished round — the driver's `--v4` rewrite only substitutes
`cd ~/source/quilltap-server`, so they escaped the pin and died on a missing
directory (five references swept, including two the sweep never reached).
`mount_case_moves_equivalence` and `mount_case_resolution_equivalence`
carried a literal `<W>` placeholder that bash read as a redirect from a file
named `W`. `memory_dedup_equivalence` had a parenthetical inside its `cd`
line. `mount_read_equivalence` and `mount_md_convert_equivalence` used
`.../fixture.db` ellipses and a bare `node --import tsx`, which ran from the
v5 worktree and died on `Cannot find package 'tsx'`. And
`roleplay_templates_tier2_equivalence`'s regen stage set the Rust test's
`QT_FIXTURE_ROLEPLAY_TEMPLATES` where the TypeScript oracle case reads
`QT_FIXTURE_RT`.

#### 2026-08-27 — chore(ratify): the 4.9.0 knip/coverage riders — a blob-registry guard and one vestigial wardrobe twin

_Versions: core 0.0.689, harness 0.0.593._

The two riders that came with P4.D129's ratification of v4's `487ae57fe`
(regression-test coverage) and `561466cfe` (the knip sweep). Neither v4
commit is portable; both left a question about v5 that wanted an answer.

`487ae57fe` pins v4's registerBlobColumns re-assert trap with a new
`help-doc-chunks.repository` test: the registration is keyed to the backend,
not the repository instance, so a cached "already registered" flag leaves a
fresh backend without blob handling and the write path then persists an
index-keyed JSON object where a BLOB belongs — which is how v4's "legacy
JSON-text embeddings" were minted in the first place. v5 cannot reproduce
that bug: it abandoned v4's document mapper, so there is no registry to
forget, no cached flag, and no `JSON.stringify` fallback a Float32 vector
could land in. `db/help_docs.rs` and `db/help_doc_chunks.rs` already record
that finding at length, ending with the instruction not to add a
registration mechanism in order to have something to register.

A prose instruction is not a pin, so the new
`embedding_blob_binding_guard` makes the claim executable in three arms:
nothing under `crates/` may name a blob-column registration mechanism (the
sole allowed hit is the doc comment quoting the grep); each of the six
modules owning a table with an `embedding` column still carries a
`float32_to_blob(` call site; and `float32_to_blob` has exactly one
definition. The needle carries its paren deliberately — a module quietly
switching to `float32_to_blob_raw`, the headerless legacy encoder, would
satisfy a substring match while writing the wrong bytes. All three arms
mutation-proven with compiling mutations, each reddening exactly one arm.

`561466cfe` deleted eleven unused v4 exports, four of them P4.D71-era
wardrobe helpers. Checked against v5: `GROUP_WARDROBE_FOLDER` and
`PROJECT_WARDROBE_FOLDER` have no v5 constant twins;
`noSharedWardrobeTiers` maps to `SharedWardrobeTiers::none()`, which is
load-bearing at sixteen call sites and stays; and
`resolve_shared_wardrobe_tiers_for_project` had never acquired a call site
in v5 either, so it is removed here — dead in both trees — together with
the import it alone kept alive. A `pub` item in a library crate is
invisible to `dead_code`, which is why this class needs a v4 deletion to
surface it at all.

#### 2026-08-26 — docs(porting): four work orders for the 4.9.0-push catch-up round — all fourteen drift rows ordered

_Docs-only change._

The `/setupphase` planning pass over the drift ledger's fourteen-commit
4.9.0 block. Four parallel lanes, ownership disjoint, meeting only at the
binding Shared contract (§A family ownership, §B pins at the `8872d7efc`
target baseline, §C sweep serialization, §D the AboutView split):

- `p4.d126-memory-wipe-batch-bug103.md` — the full-wipe memory deletion
  routed through the unlink-batch chokepoint (`914b59e13`), the SQLite
  bind-variable chunking v5 measurably lacks at both `db/memories.rs`
  sites (`805ef12bf`), and bug 103's shared legacy profile-column seeding
  for restore + `.qtap` import (`e000d6bfc`) — all red-first.
- `p4.d127-provider-cheapllm-drift.md` — bug 104's Z.AI private
  vision-list deletion (v5 measurably has it in
  `chat_completions.rs`), the per-task cheap-LLM budget table (75 s
  compression) + the failed-task warn, and the realtime coalesce-trace
  drop with a capturing-layer silence pin. Retires the expired 💸 Z.AI
  refusal-sentence proof per the ledger's §1 note.
- `p4.d128-qt-utilities-cli-about.md` — the two missing solid hover
  qt-* utilities + the 20-site class sweep with its two deliberate
  visual shifts, the four never-completed CLI flags (Tier R red-first +
  v4's token-level coverage guard mirrored), and the About page's
  ten-provider sentence + new "Live interface" bullet.
- `p4.d129-dedup-neutrality-ratify.md` — the `dcab791c2` 155-file dedup
  sweep ratified by NEUTRALITY PROOF (bulk sweep at the post-drift pin,
  sibling families excluded; the two named non-neutral hunks measured),
  the five remaining NO-PORT? ratifications, and the two riders (the
  `help_doc_chunks` blob-column pin, the knip vestigial-twin check).
  Runs after the other three lanes close.

The drift ledger's §3 dispositions move to `ORDERED(<lane>)` for all
fourteen rows in this commit.

#### 2026-08-26 — docs(porting): the 4.9.0-push drift check — fourteen commits classified, eight PORT

_Docs-only change._

A full `/driftcheck` over the fourteen commits v4 landed past the
`f3892158d` baseline — its whole 4.9.0 release push, all on 2026-08-26 —
replacing the previous entry's honest count-only record. Every row is
classified from its shipped hunks: **eight PORT**, six **NO-PORT?**.

Three of the PORT rows are v4 bugs this port measurably reproduces, each
confirmed by reading v5 source rather than inferred from the commit
message. Bug 103 (restore lets the table DEFAULT answer for a column the
archive predates) is live in `restore/orchestrator.rs:275-300`, on both
`supportsImageUpload` and `multiCharacterPrefill` — and v5's `.qtap`
import already seeds half of it, so the two paths disagree exactly as
v4's did. Bug 104 (the Z.AI plugin's private vision list, which
`glm-5.3-flash` outgrew) is transcribed at `chat_completions.rs:54-57`
with its refusal sentence at `:106`. The SQLite variable-limit ceiling
`805ef12bf` fixes is present at two sites in `db/memories.rs` — the
`bulk_delete` `IN (…)` at `:336` and the doomed-set resolve at `:607` —
and bites full-wipe restores at Friday's memory count.

The largest row is not a bug at all: `dcab791c2` is a 155-file,
~2,800-line dedup sweep claiming byte-neutrality across routes, hooks,
tool handlers, the data layer, and the provider wire. It is the
P4.D31/P4.D44 class, and it ratifies by regenerating the affected
families at the pin and proving them identical — not by reading. v4's own
diff underlines why: the OpenAI-compatible builder spreads `stream` /
`stream_options` mid-literal specifically to keep those keys between
`stop` and `user` so the serialized body stays byte-identical.

Also recorded: `bugfix` was re-measured and is an inert `4.8.4` fork
marker with nothing unabsorbed; the regen rule stays **PIN REQUIRED**;
and one banked live proof expires under §5.5 — the standing "Z.AI refusal
sentence" dogfood item, which bug 104 deletes the path for. CLAUDE.md's
baseline paragraph no longer restates a drift count that can go stale; it
points at the ledger's §1 and says so.

#### 2026-08-26 — docs(porting): v4 moved eleven more commits during the unification gate — recorded, /driftcheck owed

_Docs-only change._

The round's closing look at the v4 checkout found HEAD at `964ffb959` —
eleven commits past the `561466cfe` the mid-round record captured, all
landed while the unification gate ran (the 4.9.0 release push: bugs
103/104, release notes, dedup/refactor sweeps — including `21f573039`,
which drops the per-publish realtime coalesce trace inside the very code
this round ported). The ledger's §1 now records the count and shas with
the verdict marked ELEVEN UNCLASSIFIED; a full /driftcheck is owed before
the next /setupphase. The regen rule stays pin-required at `f3892158d`.


#### 2026-08-26 — fix(realtime): the collection POST's enqueue publishes its jobs hint — the activated beat's first-run catch

_Versions: core 0.0.688, harness 0.0.592 (unchanged), SPA 0.5.583 (unchanged)._

The unification-activated hint beat failed its FIRST live run and led to a
real cross-lane fidelity gap: v4's `POST /api/v1/system/jobs` enqueues
through `enqueueJob` — a realtime publish site — but v5's `jobs_enqueue`
writes the row at the API layer, bypassing `queue_service::enqueue_job`, so
the collection POST emitted no hint. A fourth enqueue site neither lane's
survey table carried. Fixed at the API layer; pinned by a capturing test
(publish on success, silence on the refusal arms), a new census row in
`realtime_publish_sites_guard`, and a mutation proof reddening both. The
beat's own failure was a fixture-precondition gesture defect
(`memoryBackfillStart` needs a default embedding profile the committed
fixture deliberately lacks); it now drives the collection POST — which
makes the beat the new site's live wire proof — with a residue-free delete.


#### 2026-08-26 — test(core): serialize the wrapped-path span writers with the registry's exact-count tests

_Versions: core 0.0.687._

The unified workspace gate's one red — `a_fired_deadline_warns_and_writes_
the_ruled_error_row`, green in isolation and in eight consecutive
full-binary re-runs — exposed a reasoned race while being diagnosed: the
activity registry's exact-count tests hold `ActivityTestGuard` over the
process-global counters, but thirteen tests in `cheap_llm_exec` and
`embedding_provider` now drive REAL spans through the round's wrapped
paths without it, bumping the same statics concurrently. Each takes the
guard now (test-only; no production code moved). The observed red itself
did not reproduce (0-for-8) and is recorded in the round record as the
pre-existing capture-under-load class, per the P4.40 precedent for an
honestly-unreproduced intermittent.


#### 2026-08-26 — feat(e2e): the f3892158d-round unification wires — the live hint beat goes active

_Versions: SPA 0.5.583._

The cross-lane proofs no single lane could run. The §Shared contract diffed
name-for-name across sides: the six topics (`RealtimeTopic::as_str` vs the
client `REALTIME_TOPICS`/topic map), the five kind ids in order
(`ActivityCounts::to_json` vs the client `ACTIVITY_KINDS`), and the jobs
response key order (server insertion vs the client reader) — all identical;
the hint wire bytes are pinned server-side and the client discriminates on
exactly the contract rule (`topic` + `v`). `P4D124_HINTS_LANDED` flipped
true: the page-toolbar beat that drives a REAL `jobs` hint off the live
event stream is active, running for the first time in the unified gate.

#### 2026-08-26 — fix(spa,core,tools): the f3892158d-round §3 review findings, and the ng-run spec-build-failure hang

_Versions: core 0.0.686, harness 0.0.592, SPA 0.5.582._

The unification review's three blocking findings, fixed with red-first pins:
`formatRelativeDate` had grown a `year:` key v4's tail branch does not have
(cross-contaminated from `formatChatListDate` during the hoist into
`shared/format-date.ts`; a prior-year case now pins the exact v4 option set),
`formatChatListDate` rendered the short weekday where v4 renders `'long'` —
with the lane's spec pinning the divergent value — and the two memory
regenerate cards read the realtime channel gate inside the function-form
`refetchInterval`, where the signal read is untracked, so a mid-drain channel
drop could never re-arm the fallback poll (hoisted to the reactive options
factory, the badges' pattern; the spec family gained the up→down direction,
one resume case per card, mutation-proven). Minors fixed alongside: the
autonomous reconcile published its hint even when the pause-patch write
failed (now gated, pinned by an UPDATE-trigger failed-write leg), the
span-sites guard asserts exactly-one-wrap-per-file (was presence-only), a
broken rustdoc link, the registry header's garbled spawn-propagation
sentence, `fallbackPoll`'s overclaimed consumer list, and the two raw
`['systemAutonomousRooms']` spellings now import `AUTONOMOUS_ROOMS_KEY`.

Riding the round because the gate hit it twice: `tools/ng-run.mjs`'s `test`
marker now treats `Application bundle generation failed.` as terminal — a
spec that failed to BUILD never reaches vitest, so the wrapper's
vitest-summary markers never fired and every spec build failure became a
30-minute silent hang. Proven by reliving the failure: the same broken spec
now exits 1 in ~10 s. Full record: `status-log.md` → "The `f3892158d`-round
§3 unification review".

#### 2026-08-26 — docs(porting): record the mid-round v4 drift — 487ae57fe and 561466cfe, both NO-PORT candidates

_Docs-only change._

(Entry added retroactively with the review-findings commit — the ledger
update landed at the start of unification without one.) v4 landed two
release-checklist commits while the round's lanes ran: `487ae57fe` (nine
regression-test files; the only lib/app hunks are a stated behaviour-neutral
hook extraction on the already-ported bug-77 notice surface) and `561466cfe`
(a knip dead-code sweep + a byte-identical HAIR-guidance dedup). Recorded as
UNPROCESSED NO-PORT? rows in the drift ledger with ratification notes.

#### 2026-08-26 — test(chat): discriminate hints from chat frames in the send smoke

_Versions: web 0.0.96._

`chat_send_smoke` asserted that EVERY frame on the event stream is chat-scoped.
That premise stopped holding the moment the turn's own activity spans and
post-turn enqueues began publishing invalidation hints, which are deliberately
unscoped. The trace now partitions the stream the way a client does — a frame is
a hint iff it carries both `topic` and `v` — then asserts hints carry no chat
scope and every remaining frame does.

Caught by the workspace gate, not by inspection.

#### 2026-08-26 — test(realtime): the hint end to end, and the SSE exposure survey

_Versions: web 0.0.95._

A wire test drives an enqueue through the real route and reads the resulting
hint off `GET /api/events`, pinning the three things the core's capture tests
cannot see: that the host actually arms the bus at boot (nothing else fails if
it does not — every publish just becomes the documented no-op, silently), that
the event reaches the SSE stream, and that the bytes are the contract's
`{"v":1,"topic":"jobs","at":<ms>}` with no scope tag and no `id`.

The SSE exposure survey rides in it as an assertion rather than prose: v4's
origin worry is WebSocket-specific, but hints ride an EventSource here, which
is CORS-governed. This router installs no CORS layer at all, so the stream
carries no `Access-Control-Allow-Origin` and a cross-origin reader is blocked.
The test now fails if a permissive layer ever appears.

#### 2026-08-26 — feat(terminal): the WebSocket same-origin gate

_Versions: web 0.0.94._

Ports the one leg of v4 `f3892158d`'s `upgrade-auth.ts` that applies here.
Browsers do not apply CORS to WebSocket upgrades, so v5's terminal socket — which
had no origin check at all — could be opened by a page on any origin. A
mismatched `Origin` now closes with 1008, after the session-exists check, which
is where v4's gate sits.

v4's other two checks do not port: v5 has no session auth by design (D2), and a
locked instance is already answered 503 before the upgrade.

New tier-1 differential `terminal_ws_origin_equivalence` drives v4's real
`authenticateUpgrade` over 19 (origin, host) pairs with its session and locked
legs mocked away, comparing verdicts and refusal sentences. It immediately
caught a real divergence: v4's `if (!origin)` is a truthiness test, so an EMPTY
`Origin` header is allowed rather than refused as unparseable. A live wire test
covers the plumbing — cross-origin refused, a different port refused, missing
and same origin accepted, and the session check still running first.

#### 2026-08-26 — feat(realtime): publish invalidation hints from every chokepoint

_Versions: core 0.0.685, harness 0.0.591, host 0.0.83._

Wires v4 `f3892158d`'s publish points: the enqueue funnels, a cancel that
actually took, the claim transition, completion (plus the entity hints its type
and payload name) and failure, both edges of an activity span, the post-commit
write-batch hook, and all seven autonomous run-state transitions. The host arms
the bus at boot with the engine's sender and a spawner.

v4 has three queue-service publishes; v5 needs five sites for them, because v4's
one `enqueueJob` is two functions here plus a render enqueue that mints its row
inside the caller's transaction. v4's three `markFailed` publish arms collapse
into v5's one, since there is no child and the apply is in-process.

A hint is not DB state, so no differential can see any of this. Sixteen capture
tests drive the real entry points with a subscriber on the broadcast channel;
they arm a thread-scoped bus, because a globally-armed one collects hints from
every concurrent test and makes a publish from a plain `#[test]` thread panic
for want of a reactor. Fourteen mutation proofs. The rollback publish inside
`begin_autonomous_run` cannot be isolated behaviourally — it always coalesces
with the start patch's — so a census holds the count instead.

#### 2026-08-26 — feat(realtime): the job-type and write-batch topic computation

_Versions: core 0.0.684, harness 0.0.590._

Ports v4 `f3892158d`'s pure `job-topics.ts`: which entity topics a finished job
announces, and which a committed write batch does. An id that cannot be read
still yields a collection-wide hint — coarser than ideal, never wrong — and the
batch legs dedupe by `topic:id`, order-preserving.

New tier-1 differential `realtime_topics_equivalence` drives v4's real module
over 73 cases. The corpus lives once, in the oracle: each row carries its input
alongside v4's output, and the Rust side reads that input back, so the two
sides cannot drift. The work order's expectation that the write-batch leg would
need a paired corpus is refuted — v5's buffered writes are v4's `{method,
args}` verbatim, because the Phase-2 partition port kept that representation
deliberately.

Five mutation proofs.

#### 2026-08-26 — feat(realtime): the invalidation hint and its coalescing bus

_Versions: core 0.0.683._

Ports v4 `f3892158d`'s realtime bus onto v5's own transport. A hint says which
slice of server state changed, never what it changed to:
`{"v":1,"topic":"…"[,"id":"…"],"at":…}`. `publish_realtime` collapses repeats
per `topic`/`topic:id` behind a 250 ms trailing-edge debounce and emits one
hint, stamped at flush time.

**The one mechanism divergence:** v4 adds a second WebSocket at
`/api/v1/system/realtime/stream`. v5 does not, and will not — hints ride the
existing `Event` channel (SSE in `quilltap-web`, the pump in `quilltap-tauri`),
because the locked transport-agnostic boundary says streaming only ever happens
there. v4's WS-protocol legs (the ping/pong message, the stream path, the
per-socket fan-out bookkeeping) have no twin: the broadcast channel owns
delivery.

The bus is armed by the composition root with the engine's sender AND a
spawner, because a trailing-edge debounce needs a timer and this core
deliberately has no tokio scheduler — the same rule the job runner states.
Publishing before arming is a silent no-op, which is the shape v4's job-child
guard takes here.

Thirteen unit tests, four mutation proofs.

#### 2026-08-26 — feat(jobs): activity spans at every v5 twin of v4's instrumented paths

_Versions: core 0.0.682, harness 0.0.589._

Wires v4 `664cfca84`'s eight applicable span sites: the job runner attributes
each handler to its own chip kind without adding a count, and `track_activity`
wraps the cheap-LLM executor (kind computed from the task type), the memory
gate, the Concierge gatekeeper, the real embedding provider, the image
generation tool, the vision describe-fallback, and the avatar preview. v4's
why-comments come with them.

Two of v4's sites have no v5 surface: the child-IPC mirror (v5's runner is
in-process) and `POST /api/v1/images?action=generate` (v5 serves only
`/api/v1/images/{id}`; its Generate Image path goes through image-profiles into
the already-wrapped tool). The character wizard and the wardrobe image analyzer
are likewise unported or refusal arms.

No differential can see any of this — an in-flight counter never touches a row.
New `activity_span_sites_guard` holds all ten rows mechanically, asserting the
absent ones stay absent; three sites are additionally driven for real, observing
the attribution set from inside an injected stub, which is a thread-local and so
immune to the global counters other tests in the binary move. Nine mutations
run, one per site plus the no-surface arm.

#### 2026-08-26 — fix(jobs): the jobs verb answers activeByKind, and activeByType becomes opt-in

_Versions: core 0.0.681, harness 0.0.588, web 0.0.93._

`GET /api/v1/system/jobs` now always answers `activeByKind` and `startedByKind`
alongside `stats` and `processor`, and withholds `activeByType` unless
`includeByType=true` — or `includeJobs=true`, which implies it (v4's
`param === 'true' || includeJobs`; that widening lives inside `jobs_list` so the
differential can prove it). The per-type breakdown reads every active row, which
is why v4 `664cfca84` made it opt-in; the by-kind snapshot is what the toolbar
polls.

`system_jobs_collection_equivalence` grows 8 cases to 11 and now pins the key
ORDER of every GET, so a leaked unconditional `activeByType` cannot hide behind
a reordering. It was regenerated red-first: the four pre-existing GET cases
failed before the port. New `system_jobs_web_routes` drives the real query
string over a live server, because the differential can only assume how the edge
decodes `?includeByType=1`.

#### 2026-08-26 — feat(jobs): active counts by activity kind, merged with in-flight work

_Versions: core 0.0.680._

Adds `BackgroundJobsRepository::get_active_counts_by_kind` (v4
`getActiveCountsByKind`) and `queue_service::get_activity_snapshot` (v4
`getActivitySnapshot`): PENDING+PROCESSING job rows folded through the kind
table, merged with whatever the activity registry currently has in flight, plus
the registry's monotonic started totals.

v4 runs one indexed `COUNT(*)` per kind; v5 gets the same totals from one
aggregating GROUP-BY pass mapped through the table — the same deliberate
divergence `get_stats` already carries, which v4 has now converged onto from the
other direction (its `getStats` became per-status `COUNT(*)`s this commit, so
that side is a no-op for v5).

Three unit tests, two mutation proofs. The merge itself is invisible to any DB
diff — in-flight counters never touch a row — so it is pinned here rather than
in the jobs differential.

#### 2026-08-26 — feat(jobs): the in-flight activity registry

_Versions: core 0.0.679._

Ports v4 `664cfca84`'s activity registry: the other half of the toolbar chips,
counting work that never becomes a `background_jobs` row. `track_activity`
counts a kind for the whole span including failures, re-entrant by kind so a
shared chokepoint can be wrapped without inflating the chip when a job of the
same kind calls it; `run_attributed_to_job` attributes a handler without adding
a count; `begin_activity` returns an idempotent guard; the monotonic
`startedByKind` totals gate on a 250 ms threshold so a cache hit never makes a
chip flicker.

Two deliberate v5 shapes. v4's child-IPC mirror does not port — v5's job runner
is in-process, so `local` is the whole truth and there is no crash mirror to
zero. And attribution is a hand-rolled poll-scoped thread-local rather than
`tokio::task_local!`, because that lives behind tokio's `rt` feature and the
default core build has no scheduler; the guard also ends on `Drop`, which v4
does not need because JavaScript futures cannot be cancelled.

Eighteen unit tests mirror v4's own suite. Five mutations were run; the first
pass caught a vacuous idempotence case — the floor at zero absorbs a missing
latch, in v4's test as much as ours — so two cases that actually see it were
added.

#### 2026-08-26 — feat(jobs): the activity-kind tables behind the toolbar chips

_Versions: core 0.0.678, harness 0.0.587._

Ports the two static tables v4 `664cfca84` introduced. `JOB_TYPE_ACTIVITY`
maps every one of the 23 background-job types to one of five activity kinds
(memory, embedding, summary, danger, image) or to an explicit `None`; v4 gets
its totality from `Record<BackgroundJobType, …>`, and since v5's job types are
strings, a unit test asserts the table's key set equals the enqueue gate's list
in both directions. `TASK_TYPE_ACTIVITY` maps the 22 cheap-LLM task types, with
v4's fall-back-to-summary rule for anything absent.

New differential `activity_tables_equivalence` diffs both tables — entry order
included — plus `ACTIVITY_KINDS` and v4's real `BackgroundJobTypeEnum.options`,
against v4's exports. `ACTIVITY_CHIPS` is client-only display metadata and does
not port here.
#### 2026-08-26 — fix(e2e): a healthy live channel parks the chips' fallback poll, so the beat must take the channel down

_Versions: SPA 0.5.581._

The two reworked queue-chip beats failed on their first live run against the
real Playwright server, and correctly: since P4.D125 the chips return `false`
from `refetchInterval` while the live channel is up, and that server's SSE
stream is healthy. A beat that intercepts `/api/v1/system/jobs` and then waits
for the chips to notice a changed body waits forever.

Both beats now abort `GET /api/events` for their page, which puts the transport
in `reconnecting` — the dropped-connection state the heartbeat exists for — and
unroute it at the end. Nothing was weakened: the beats test the FALLBACK, and
they are now the thing that removes the channel rather than assuming it was
never there.

Full suite: 251 passed, 0 failed, 2 skipped (the gated `jobs`-hint beat awaiting
P4.D124, and the standing P4.D112 store-probe park).

#### 2026-08-26 — feat(spa): the realtime hub over the existing event stream, the reworked queue chips, and the polling-site migrations

_Versions: SPA 0.5.580._

The bulk of P4.D125 — the client half of v4 `664cfca84` + `f3892158d`.

**The hub.** `core/realtime.service.ts` is v5's twin of v4's
`lib/realtime/client.ts` + `hooks/useRealtime.ts` + `RealtimeProvider`, folded
into one root service because v5 already owns the connection those three shared.
v4 opens a second WebSocket; v5 does not — hints ride the EXISTING event channel
`CoreClient` already owns (SSE `GET /api/events` in HTTP mode,
`quilltap://event` in the Tauri shell), which is the locked transport-agnostic
boundary meeting v4's own "one socket per tab". The ping keepalive and the
hand-rolled 1 s → 30 s jittered backoff are WS-protocol legs with nothing to do
here and are recorded NO-PORT per leg; everything observable carries — a
`connected` status the fallback gating reads, `{topic, v}` frame discrimination
on the shared stream, unknown-topic tolerance, and the catch-up sweep on every
(re)connect, SSE reopen-after-error, and `quilltap://resync`.

**The topic map** (`core/realtime-topic-map.ts`) targets v5's actual per-feature
key consts, row by row, with each divergence from v4's targets recorded beside
it. `mountPoints` is recognised but resolves to nothing: v5 has no
document-store query key at all.

**The chips** are a TanStack query on the new `systemJobsKeys.all`, reading
`activeByKind`/`startedByKind` (never `activeByType`), with v4's adaptive
heartbeat moved into `refetchInterval`'s function form (1.5 s busy / 8 s idle,
gated on the channel) and the `startedByKind` pulse — first read is a delta base,
a decrease is a server restart, an advance pulses for 1.2 s. `ACTIVITY_CHIPS` is
transcribed from v4 including the `image` → `qt-queue-badge-story` quirk, and
`.qt-queue-badge-pulse` + `@keyframes qt-queue-badge-blip` land in
`_content.css`. `notifyQueueChange()` stays — v4 keeps it too, as the instant
same-tab kick — but now invalidates the jobs key instead of driving a bespoke
re-poll; v5's own `NavigationEnd` stop-and-refire is gone with the hand-rolled
poller.

**Migrated sites**, each keeping its original cadence as a gated fallback: the
tasks queue (with v4's "Auto-refresh (5s)" → "Fallback polling (5s)" relabel and
its tooltip), the three memory housekeeping cards, the autonomous badges and
management list, the Salon's story-background sweep and active watch (whose
change callback moves to the shared transition effect), the merge picker's ages,
and the character conversation card's day-boundary rollover.

Parity specs throughout, twenty-seven mutation proofs, and the toolbar e2e beats
reworked onto the new shape plus a gated `jobs`-hint beat awaiting the P4.D124
server half.

#### 2026-08-26 — refactor(spa): the chat query keys get a const, so the realtime topic map can name them

_Versions: SPA 0.5.579._

`chat/chat-keys.ts` (P4.D125 unit 2). v5 has no central key module; keys live
beside their feature. The chat family had no const at all — only the raw
spellings `['chats']` and `['chat', id]` typed out at every call site, the Salon
alone carrying twenty-seven. The realtime topic map has to name both, and a
table quoting a spelling nobody else imports is a drift waiting to happen.

Swept, with the spellings unchanged: `screens/salon/salon-conversation.ts` (27
sites), `chat/merge-conversation-modal.ts` (2), `screens/salon/salon-list.ts`
(1), `workspace/core/tab-refetch.ts` (the `CHAT_LISTS` const). `detail` takes a
nullable id because several Salon handlers pass `this.chatId()` straight
through, and `['chat', null]` is the key those sites already produced.

No behavior change; the guard spec pins the two spellings, the singular/plural
split the tab-refetch prefix rule leans on, and the prefix relationship to
`['chat', id, 'background' | 'outfit-summary' | 'cost']`.

#### 2026-08-26 — feat(spa): the shared clock, and the relative formatters take a `nowMs`

_Versions: SPA 0.5.578._

The client half of v4 `f3892158d`'s Phase 0 (P4.D125 unit 1). `NowService`
(`shared/now.service.ts`) is the Angular twin of v4's `hooks/useNow.ts`: one
`setTimeout` chain per granularity however many consumers subscribe, ticks
aligned just past each boundary (`granularity - (now % granularity) + 1` ms, and
local midnight at day granularity) so every "4m ago" on screen flips together,
sub-minute tickers parked while the tab is hidden and resynced on the way back,
and an `enabled` flag that neither subscribes nor advances when false.

`formatRelativeDate` and `formatChatListDate` move from the two feature files
they were transcribed into (`tasks-queue.api.ts`, `character-conversation-card.ts`)
to `shared/format-date.ts` — v5's home for `lib/format-time.ts` — and both gain
v4's optional `nowMs` parameter. `formatRelativeAge` lands beside them, byte-
faithful to v4's version; it has no v5 consumer yet, because v5's startup screen
has never carried v4's per-step event list.

Parity specs against v4's own `__tests__/unit/hooks/useNow.test.tsx` semantics
plus the formatters' branch boundaries; seven mutation proofs (the boundary
epsilon, local-midnight alignment, the disabled path, the hidden-tab pause,
`Math.round` vs floor in `formatRelativeAge`, and both `nowMs` threads).

#### 2026-08-26 — docs(porting): the `f3892158d` drift catch-up round ordered — three work orders across two lanes

_Docs-only change._

`/setupphase` for the two-commit drift block (`664cfca84` jobs/activity,
`f3892158d` realtime). Three orders: `p4.d123-jobs-activity-server.md` (the
total kind map, the in-flight activity registry, the ten span sites, the
`activeByKind`/`startedByKind` jobs verb) and `p4.d124-realtime-server.md`
(the invalidation bus, the topic computation with its tier-1 differential,
the publish points, the terminal same-origin gate) run STACKED as one server
lane; `p4.d125-activity-realtime-spa.md` (the chips rework, the realtime hub,
the topic map, the shared clock, the poller migrations) runs in parallel.
The round's settled transport decision, binding in all three §Shared
contracts: the invalidation hints ride v5's EXISTING Event channel (the
engine broadcast → SSE `/api/events` → the Tauri pump) — no second
WebSocket, per the locked transport-agnostic-boundary invariant. Both
drift-ledger rows marked ORDERED; regen rule stays pin-required at
`b220999da` (lane regens for moved families pin at `f3892158d`).

#### 2026-08-26 — docs(porting): v4 drifted two commits — the jobs-activity rework and the realtime subsystem

_Docs-only change._

`/driftcheck` against the `b220999da` oracle baseline: v4 `main` is **two
commits ahead**, both landed 2026-08-26, both on already-ported surfaces.
`bugfix` is unmoved at `3a76b17df` with no unabsorbed content, and the
checkout is clean on `main`.

`664cfca84` ("toolbar chips count whole operations") reworks activity
accounting rather than the chips alone: a total `JOB_TYPE_ACTIVITY` map, a new
in-flight activity registry counting non-job work (inline image paths, the
Concierge classifier, inline embeddings, the memory gate, cheap-LLM tasks, four
vision call sites), re-entrant counting by kind, indexed `COUNT(*)` stats
queries, a heartbeat poll — and a wire change: `GET /api/v1/system/jobs` always
returns `activeByKind`/`startedByKind` while `activeByType`, the key v5's jobs
verb and SPA badges both read today, becomes opt-in behind `?includeByType=true`.

`f3892158d` ("push interface updates over a WebSocket") adds a whole
`lib/realtime/**` subsystem: a multiplexed socket carrying ~40-byte
invalidation hints with a 250 ms per-topic debounce, publish points inside
already-ported queue/dispatcher/autonomous-room code, polling demoted to a
socket-health-gated fallback across a dozen client sites, shared WS upgrade auth
that also replaces the terminal handler's cookie test, and a `useNow` ticker
with `nowMs` threaded through `lib/format-time.ts`.

The ledger's §1 flips to **DRIFT PENDING — 2 commits** and the regen rule to
**PIN REQUIRED**: every oracle regeneration now runs from a lane-unique
detached worktree pinned at `b220999da` until a catch-up round moves the
baseline. Neither commit is a convergence — `docs/developer/bugs.md` is
unchanged since the baseline.

#### 2026-08-26 — docs(porting): the systemHome dashboard cost, recorded as a candidate for a coding phase

_Docs-only change._

The 2026-08-26 dogfood pass measured `systemHome` — the one dispatch behind the
landing dashboard — at a steady **7.50 s / 7.70 s** on back-to-back warm calls
against the real Friday copy (859 chats, 32 live characters, 8 projects, 45
vaults). Correct payload, no panic, and no v4 comparison was run, so it is
explicitly not filed as a divergence.

Recorded as **phase-4.md candidate 2** with a starting point read off the
handler: `services::home::get_home_data` loads all chats, all projects, all
characters and all files — projects and characters both through the mount-index
overlay — before trimming in memory to 12 recent chats, 8 projects and a few
characters. That is a hypothesis, not a profile; profiling the four loads is
step one, and v4 composes the same `findAll` shape, so the target is the cost of
producing the payload rather than what the dashboard shows. A pointer note also
lands in `dogfood-findings.md`'s standing notes.

Candidate 1 is marked RAN with what the pass discharged and what it left owed
(Pascal's group tier, now characterized as needing a single-group chat, plus the
three human cost calls), and candidate 3's duplicate-store collision is narrowed
to the committed e2e fixture — the real instance has no duplicate store names at
all.

#### 2026-08-26 — docs(dogfood): the b220999d-round pass — 41 rows, 37 PASS, finding #105 fixed

_Docs-only change._

The walk record for the 2026-08-26 dogfood pass over the `b220999d` round
(per-tier dressing instructions, archive-instead-of-delete, the Documents search
chip) plus the carried `8f910137` queue, and its findings/status-log/CLAUDE.md
rows.

v4 had run the dressing-instructions feature on this instance hours before the
copy was taken, so v5 read v4's own `Wardrobe/instructions.md` bytes back
byte-identically and the cascade reached a real "Let character choose" turn
carrying them. Nine standing live-proof items discharged, including two of
Pascal's three remaining side-effect write paths. The one defect (#105) was
fixed and committed separately as `599f6be9`.

#### 2026-08-26 — fix(search): a Documents result opened from inside a chat threw NG0201 and did nothing

_Versions: SPA 0.5.577._

Dogfood finding #105, on the `b220999d` round's own new surface. Clicking a
Documents search result with a Salon focused should split the document into
that conversation; instead it threw `NG0201` and the dialog just sat there.

`OpenDocumentFromSearch` is `providedIn: 'root'`, so its `inject(Injector)` is
the root injector — which never sees `salon-conversation.ts`'s component
`providers: [… DocumentApi]`. The lane had already hit NG0201 at render time
and moved the lookup to a lazy `injector.get(DocumentApi)`; that relocated the
crash from render to click without fixing it.

`DocumentApi` is a stateless wrapper over the root `CoreClient`, so the fix
builds our own instance in the root injection context
(`runInInjectionContext`), memoized. It is deliberately not registered
globally: `document-picker.ts` injects `DocumentApi` `{optional: true}` and
relies on it being absent outside a chat to fall back to
`StandaloneDocumentApi`.

Guards: three TestBed specs that resolve the service the way the app does (the
existing hand-built harness stubs the injector with one that always answers,
which is exactly why this was invisible), and a third e2e beat that clicks the
card with a Salon focused — the gesture neither existing beat makes.

#### 2026-08-26 — docs(porting): the b220999d-round unification — all four orders land whole; the baseline moves

_Docs-only change (the round's code landed in the preceding commits; final
versions: core 0.0.677, harness 0.0.586, web 0.0.92, SPA 0.5.576)._

The `b220999d` drift catch-up round unifies: the per-tier dressing
instructions and archive-instead-of-delete features whole (server + SPA, the
stacked P4.D119→P4.D120 lane and P4.D121), and the Documents-search vertical
(P4.D122). The oracle baseline moves `8f910137` → `b220999da` and the drift
debt is cleared; the two docs-only feature specs are NO-PORT-ratified. The
§3 review's three would-have-shipped findings (the instructions handlers'
guard order, the scenario `archived: null` silent-keep, the REST edges'
unknown-action fallthrough) were fixed red-first on the unify branch. Gate:
the 31-family regen+run sweep from the pinned worktree 31/31 ok zero SKIP
with changed-bytes greps; `cargo test --workspace` 461 binaries / 2,426 / 0
with the round's 60-variable env block and zero SKIP lines; clippy clean on
both feature sets; release build clean; SPA `npm test` 351 files / 5,292 /
0 and `npm run build` clean; full Playwright 249 passed / 0 failed / 1
skipped (the standing store-probe park) with the round's five beats live —
the three activated beats' first live runs caught three gesture defects,
all repaired spec-side. Round record: `status-log.md`; the next candidates:
`phase-4.md`.

#### 2026-08-26 — feat(spa): the b220999d-round unification wires

_Versions: SPA 0.5.576._

The cross-lane obligations no single lane could discharge. The P4.D122
`PENDING_CROSS_LANE` hand-off: `DocumentModeController` now listens for
`quilltap:document-opened` (v4 `useDocumentMode.ts:725-742`) and, for its own
chat, reconciles the open-document set and focuses the new row only after the
reconcile resolves and only when the event names a `chatDocumentId` — three
specs pin the match, the ignore, and the focus gate; an external open into an
already-split Salon no longer waits for a reload. The scenario mutator's
interim relist divergence is RETIRED to v4's shape: the server's scenario
Update/Rename/Delete verbs carry `includeArchived` (P4.D120 threaded it to
their fresh-list returns), so the SPA now sends the flag on the mutate verbs
and applies each response directly — create stays flagless, faithfully
reproducing v4's body-not-param refresh quirk — with the spec arms rewritten
to pin the new shape and the six contract request types gaining the field.
The three gated e2e beats (`P4D120_SERVER_LANDED` ×2,
`P4D119_INSTRUCTIONS_LANDED`) are flipped live now that the server lane is on
the branch.

#### 2026-08-26 — fix(api,search,spa): the b220999d-round §3 review findings

_Versions: core 0.0.677, harness 0.0.586, web 0.0.92, SPA 0.5.575 (the
lane-accumulation recount + this fix; base was core 0.0.665, harness 0.0.577,
web 0.0.87, SPA 0.5.566)._

The unification review's findings, fixed on the unify branch. The three
scoped `?action=instructions` SET handlers parsed the body BEFORE the 404
existence check where v4 gates existence first — a missing character/group/
project with an invalid body answered 400 where v4 answers 404 (corpus-blind;
three new `*_post_missing_and_invalid_404` corpus cases pin it). A scenario
bag's explicit `archived: null` was silently treated as omitted (200 +
preserve) where v4's `z.boolean().optional()` refuses — fixed with Zod 4's
measured sentence (`Invalid input: expected boolean, received null`) on the
file-backed scopes, and with a doubled Option on the two character-scenario
dispatch verbs (flat `Validation error`, parse-before-404 as v4 orders it
there); the sibling name/description/isDefault arms' older null-tolerance is
a recorded pre-existing lead, not this round's field. The three registered
wardrobe REST edges fell through on an unknown `?action=` — `POST
?action=bogus` could CREATE an archetype — and now answer v4's dispatcher
envelope (`Unknown action: …` + `availableActions`), with the
present-but-empty action staying falsy as v4's truthiness gate has it;
wire-tested including the nothing-was-created leg. Search lane: the
enabled-stores read's error is now logged (v4's `safeQuery` sentence), two
stale ambiguity-arm comments in the ui-search corpus were rewritten to match
the healed-name reality, and the open-from-search pathname read strips URL
fragments v4's `usePathname` never carries. SPA lane: the B7 seed-guard
quirks are now spec-pinned (project/general guarded, the character seed
deliberately UNGUARDED — three mutations each redden exactly one spec), the
Show-archived flip no longer double-fetches the group union (its comment was
also wrong), and the salon beat's restore gesture scopes its checkbox by the
"Show archived" label instead of matching any checkbox.

#### 2026-08-26 — docs(porting): the P4.D119 + P4.D120 stacked lane's gate record

_No crate versions bumped._

The gate for the stacked dressing-instructions + archive-entries lane, run once
after both orders: fmt clean, clippy clean on both feature sets, and
`cargo test --workspace` at 461 test binaries / 2,393 tests / 0 failed with all
twenty of the round's families positively confirmed to have run rather than
skipped. Every regenerated oracle grepped for the bytes the change added.

#### 2026-08-26 — feat(almanack,wardrobe): the scenario archived column and the archive pin suites (v4 `d25dacc1`)

_Versions: core 0.0.672, harness 0.0.584, web 0.0.91._

The Almanack's Scriptorium table gains `| Tier | Scenarios | Archived |`, and
the fixture gains one archived scenario so the column is measurable; the
`*No scenarios*` empty state still tests `count === 0` only, as v4 leaves it.
With the P4.D119 seed the same fixture now proves both halves of v4's
asymmetry in one run: the character tier's garment count is 2 (the
instructions file excluded) while its archived count is 2 (the same file,
because its body says `archived: true` and the sibling LIKE-count got no
exclusion).

The pin suites v4 wrote as unit tests land where v5 can actually reach them.
"Archived garments never audition" becomes four end-to-end tier-3 rows: each
tier's garments vanish from the RECORDED PROMPT when archived, and with every
tier archived the consult never happens at all. The opposite-direction rule —
a garment archived mid-chat stays worn — is pinned in the source, because no
oracle case reaches the title-resolving read: `archived_wearer_read_guard`
holds the pool's two excluding reads and the equipped set's including read
apart, and flipping either fails. The `?includeArchived` spelling rule gets its
own unit arms at the two REST edges that speak URLs. The export projection is
pinned where a loss could occur — a scenario's `archived` rides inside the
scenarios array and a wardrobe item is not templated at all.

#### 2026-08-26 — feat(wardrobe): `archived` → `archivedAt` on all four item routes (v4 `d25dacc1`)

_Versions: core 0.0.671, harness 0.0.583, web 0.0.90._

`archived_patch` is the one place the API's boolean becomes the item's
timestamp: archiving is idempotent (re-archiving keeps the ORIGINAL stamp),
restoring clears it, and an already-in-state request returns `None` so the
caller can skip a pointless vault rewrite. All four item PUTs route through it —
the character and General routes off their already-loaded item, the project and
group routes off an extra O(folder) read taken ONLY when `archived` is in the
body, which is exactly why the two new 404 sentences (`Project wardrobe item not
found` / `Group wardrobe item not found`) are unreachable without it.

The collection GETs honour `?includeArchived`: General, the character route and
its `?scope=group` arm, and — the behavior change — project and group, whose
hard-coded `true` reads are replaced so the filter is server-side.

Found on the way and fixed in scope: v5's character item PUT accepted a
present-but-non-boolean `archived` where v4's Zod parse refuses with the flat
`Validation error`. That route validates NOTHING else either — a pre-existing
gap wider than this lane, banked at the source with its named remedy.

Four corpora grow: `wardrobe-routes` (+10, the General half), `group-wardrobe`
(+7, incl. the new 404 and a pre-archive step so the flag can discriminate),
`projects-routes` (+4) and `characters-mutations` (+3). `archivedAt` is
CLASSIFIED rather than blanked — a fresh stamp differs between runs, but
stamped-vs-null is the whole point. Nine mutations redden them.

Also fixed: the group-wardrobe oracle's mock request lacked `nextUrl`, which
P4.D119's `withActionDispatch` wrapper now reads — every collection case was
answering 500.

#### 2026-08-26 — feat(scenarios): archive entries instead of deleting them — the scenario half (v4 `d25dacc1`)

_Versions: core 0.0.670, harness 0.0.582, web 0.0.89._

`archived: true` frontmatter across all four scenario scopes. The chokepoint
(`db::scenarios`) gains the `"true"`-string coercion v4 reads `isDefault` with,
`is_scenario_content_archived` (so a PUT that doesn't mention `archived`
preserves it through the whole-file rewrite), the list filter, the rule that an
archived scenario can never win default resolution — nor be named as the
warning's winner — even when it IS listed, and an emitter that omits the key
entirely for an active scenario. `resolve_scenario_body` deliberately ignores
the flag: archiving hides a scenario from the menus, it does not break the chats
that already chose it.

`includeArchived` reaches the seven scenario list verbs, the item-level
mutations' fresh-list returns, and the four wardrobe list verbs; the two
hard-coded `true` reads (project and group wardrobe) are replaced, so the
filtering is server-side and a picker that never passes the parameter is safe by
construction. v4's collection-POST quirk is reproduced: the three file-backed
creates refresh their list from the BODY's `archived`, not the query param.

Character scenarios: the `absent-when-false` spread on read, the tri-state on
add/update (a restore genuinely echoes `archived: false` — measured, not
reasoned about), the GET response filter (the array itself must keep archived
entries or the projection sweep deletes their files), and **v4's `description`
round-trip fix, which v5 measurably needed too** — it was parsed on read and
never written back, so the next projection silently dropped it.

`ScenarioEntry` gains `archived` and the system-prompt builder's implicit
scenario becomes `first_active_scenario_content`. The Almanack's Scriptorium
table splits scenario counts into total and archived.

The scenarios-routes corpus grows 41 → 64 cases (writes pinned through the file
BYTES, frontmatter key order included); six mutations redden it. The
vault-character-write corpus gains a described scenario, an archived one, and
the FAILURE shape — a pre-filtered array and the file is swept. Character
scenarios are pinned at two tiers because a vault-linked character's DB column
is empty: the arrays family sees the file bytes, the subresources family sees
the returned object.

#### 2026-08-26 — feat(wardrobe): the four `?action=instructions` surfaces as dispatch verbs (v4 `b86bb1a5`)

_Versions: core 0.0.669, harness 0.0.581, web 0.0.88._

Eight verbs — `characterWardrobeInstructions{Get,Set}`,
`groupWardrobeInstructions{Get,Set}`, `projectWardrobeInstructions{Get,Set}`,
`wardrobeInstructions{Get,Set}` — with v4's guard orders, sentences and status
codes: the character GET is deliberately NOT tombstoned (an archived character
answers 200 while the SET 409s), a vault-less character clears as a 200 no-op
and refuses a write with 500, an unprovisioned Quilltap General does the same,
and the group/project GETs ensure the store but NOT the `Wardrobe/` folder
(only their POSTs do). `instructions` is v4's `z.string().nullable()` — required
but nullable — so it rides a `double_option` and an absent key is the flat
`Validation error` 400.

REST edges are extended only where a consumer already speaks the URL: the
General GET and POST, and the character GET. The character POST and the
group/project collection routes have no edges in v5 and ride
`POST /api/dispatch`, recorded per surface.

New `wardrobe_instructions_routes_equivalence` drives v4's four real route
handlers through the real middleware over 43 shared cases, comparing status,
body and a whole-table dump of the mount index; six mutations redden it. A
`quilltap-web` wire test proves the three registered URLs resolve, decode the
tri-state, and do not swallow the collection reads.

The Almanack's `count_links_in_folder` gains v4's `exclude_relative_path`
parameter, passed for the `items` figure only — the sibling `archived`
LIKE-count keeps counting an instructions file whose body says `archived: true`,
exactly as v4 ships it. The almanack fixture gains that file; the family is
regenerated once, with P4.D120's scenario column.

#### 2026-08-26 — feat(wardrobe): dressing instructions reach the outfit-selection prompt (v4 `b86bb1a5`)

_Versions: core 0.0.668, harness 0.0.580._

`OUTFIT_SELECTION_PROMPT` gains its fourth bullet, unconditionally — every
`llm_choose` consult now carries it. `build_outfit_messages` gains the
`Dressing Instructions` block, byte-exact, LAST in the note chain (after the
scenario note, immediately before `Available Wardrobe Items:`); null, empty and
whitespace-only all produce a user message byte-identical to the pre-commit one.

v4 resolves the cascade in one place; v5 split that entrance in two, so
`choose_llm_outfit` takes the resolution as a closure invoked exactly where v4
resolves — after `wardrobe_start`, once the character, the non-empty pool and
the profile guard all hold. Both entrances (`resolve_llm_choose` for chat
create, `run_llm_choose_via_db` for add-participant and merge) pass one. v4's
redundant second `sharedWardrobeTiersForCharacter` call and its
outer-`projectMountPointIds` quirk are carried; its two `.catch()` arms have no
analogue in v5's infallible resolvers and are recorded rather than invented.

`outfit_llm_choose_tier3_equivalence` grows four cases with a per-case seeded
`Wardrobe/instructions.md` (the committed fixture is untouched): the note on
both entrances, a general-tier file reaching a character with none — which is
what proves the production path invokes the CASCADE rather than a vault read —
and the blank-file arm. Dropping the bullet or nulling the runner's resolver
reddens it. The note guard's blank arm is unreachable from production (the
cascade already answers `None` for a blank file), so it is pinned the way v4
pins it: a direct unit call. A new `outfit_instructions_wiring_guard` walks the
source, because no differential reaches the create entrance's consult.

#### 2026-08-26 — feat(wardrobe): the projection preserve list and the shared reader's instructions skip (v4 `b86bb1a5`)

_Versions: core 0.0.667, harness 0.0.579._

`project_array_into_vault_folder` gains `preserve_file_names` (v4's
`opts.preserveFileNames`): the preserved names are lowercased, the `seen` set is
SEEDED with them so a garment titled "Instructions" disambiguates onto
`Instructions-1.md`, and the sweep skips a file whose last path segment matches.
Only `project_vault_wardrobe` passes it; the four managed-fields call sites
(Prompts and Scenarios, in both the create and update writers) pass an empty
list, so their behavior is byte-identical to before.

`read_character_vault_wardrobe` filters `instructions.md` (any casing) BEFORE the
emptiness branch, so a folder holding only the instructions file still falls
through to the legacy `wardrobe.json` branch.

`vault_wardrobe_write_equivalence` grows a seeded, deliberately mis-cased
`Wardrobe/Instructions.MD` and a fourth op whose item is TITLED "Instructions";
four mutations redden it. `vault_wardrobe_read_equivalence` grows a fourth store
holding only the instructions file, plus a mis-cased instructions file carrying
VALID garment frontmatter in the existing folder vault — without which the
case-insensitive skip is invisible in the item list. The four non-passing call
sites are proven neutral by regenerating `vault_character_write_equivalence`.

#### 2026-08-26 — feat(wardrobe): the per-tier dressing-instructions module (v4 `b86bb1a5`)

_Versions: core 0.0.666, harness 0.0.578._

The new `quilltap-core::wardrobe_instructions` module ports v4's
`lib/wardrobe/wardrobe-instructions.ts`: the `Wardrobe/instructions.md`
constants and the case-insensitive filename predicate, the four-tier cascade
(`resolve_wardrobe_instructions` — character vault, then group, then project,
then Quilltap General; first non-blank file wins), and the per-container
read/write helpers. Every quirk is carried: the General mount id is read
unconditionally and first, the character tier is skipped on JS truthiness, the
group and project tiers are deduped then sorted by UTF-16 code unit, a file that
trims empty CONTINUES to the next tier, the write path trims and ensures the
folder while the clear path deletes and ensures nothing, and only a NOT_FOUND
delete failure is swallowed.

`read_vault_text_file` gains a `_conn` sibling so the cascade probes up to four
tiers inside one mount-index read.

New differential `wardrobe_instructions_tier2_equivalence` drives v4's real
resolver and helpers over a new committed `wardrobe-instructions-{main,mount}.db`
fixture pair: 15 resolve/read cases and 8 ordered write ops with the five
mount-index tables dumped after each. v4's own unit suite pins the probe ORDER
through a mocked reader, which a real-DB oracle cannot see, so the
dedupe-then-sort is made observable instead — two mounts in one tier both carry a
file and the sort decides which content comes back. Six mutations proven to
redden; a seventh (dropping the folder ensure) provably does not, because the
write primitive find-or-creates folder segments itself — recorded at the source.
#### 2026-08-26 — test(search): a live walk over the Documents chip and its standalone open

_Versions: SPA 0.5.568._

`workspace-search-documents-flow.spec.ts` seeds a document into a real
database-backed store through the standalone verbs, then walks the toolbar
search bar → "See all results →" → the six filter chips in `ALL_SEARCH_TYPES`
order → narrow to Documents → the result card (Document badge, `store · path`
subtitle, the standalone deep link as its href) → click → a
`document-standalone` tab opens in place with no chat told.

Both beats are ACTIVE and green on their first full run.

#### 2026-08-26 — feat(search): the Documents chip and the open-from-search choreography

_Versions: SPA 0.5.567._

The SPA half of v4 `b220999d`. `search.types.ts` gains the sixth `SearchType`,
the shared `ALL_SEARCH_TYPES` constant (which the dialog's chips now read
instead of a local copy), `DocumentSearchResultItem`, and the three map entries.
The dialog's placeholder and empty-state line gain `documents` in v4's exact
positions.

`SearchResults` gains the documents row: the store-name `·` path subtitle, the
Document badge, a conditional Vault badge for a character-vault document, and
the standalone deep link as its href. Its click routes through the new
`OpenDocumentFromSearch` service and emits `resultClick` only when the service
actually handled the click — so a middle-click or ⌘-click falls through to the
browser, opens the silent standalone link in a new tab, and leaves the search UI
open. The other five cards keep v5's existing unconditional intercept; their v4
counterparts have no passthrough either.

Two new client files under `documents/`: `open-document-in-chat.ts` (the
three-step in-chat choreography — open the row, open the workspace tab,
announce) and `open-document-from-search.ts` (`resolveActiveSalon`,
`isModifiedClick`, and the service that picks between the in-chat open, a silent
standalone tab, and plain navigation). Three v5-source mutations each redden
exactly one parity spec.

#### 2026-08-26 — feat(search): the document text-search engine and the uiSearch documents branch

_Versions: core 0.0.669, harness 0.0.578._

Ports v4 `b220999d`'s `lib/mount-index/document-text-search.ts` whole, plus the
sixth `documents` type on `GET /api/v1/ui/search`.

The engine merges the two scans one-result-per-document: name/path hits go in
first and shadow a content hit on the same link; `matchPriority` is 0 only when
the whole lowercased file name equals the whole query, 1 for any other name or
path hit, 2 for a content hit; results sort by priority then recency, and
`totalCount` is the merged size — bounded by the 200-row scan cap, knowingly.
Archived characters' vaults are excluded, and the lookup FAILS CLOSED: if the
archived set can't be resolved, every store with `storeType === 'character'` is
dropped while ordinary stores still search (a NULL `storeType` survives the
strict compare, as in v4). Snippets lead by one THIRD of the remaining window,
trim before they ellipsize, and take their heading prefix with an em dash.

The route pushes documents between memories and tags, with v4's exact key order
and the `/workspace?open=document-standalone&…` deep link. `VALID_TYPES` is now
v4's `ALL_SEARCH_TYPES` — six entries, reordered.

`ui_search_equivalence` grows 23 → 28 cases over a fixture extended with five
document stores, a seventh archived character, and twelve pinned documents; its
corpus-shape gate is re-baselined and gains five non-vacuity assertions. Seven
v5-source mutations each redden exactly the expected cases.

#### 2026-08-26 — feat(search): the two document-store scans behind the Documents chip

_Versions: core 0.0.668._

Ports v4 `b220999d`'s two repository queries. `docMountFileLinks
.searchByNameOrPath` becomes `DocMountFileLinksRepository::search_by_name_or_path`
(the same LIKE pattern bound TWICE, once per arm, so a folder-only hit still
lands); `docMountChunks.searchContent` becomes
`DocMountChunksRepository::search_content` (`GROUP BY c.linkId` +
`MIN(c.chunkIndex)` on SQLite's bare-column rule, so the text returned is the
lowest MATCHING chunk's — and the scope filter is the chunk's own denormalized
`mountPointId`, not the link's). Both are scoped to the new
`EDITABLE_TEXT_FILE_TYPES`, both escape LIKE metacharacters, and both swallow
failures into `[]` after a log, as v4's `safeQuery` fallback does.

`DocMountPointsRepository::find_enabled_for_search` is a new scoped read that
carries `storeType` (as `Option`, since the column is nullable and v4's
fail-closed sweep uses a strict `=== 'character'` compare) without widening the
doc-edit `DmpRow`.

Eleven unit tests cover the SQL-level quirks no route-level differential can
discriminate; three v5-source mutations (MIN→MAX, dropping the second LIKE bind,
widening the escape set) each redden exactly one of them.

#### 2026-08-26 — refactor(doc-edit): extract docStoreAuthority and add the bare store-ref resolver

_Versions: core 0.0.667._

Ports v4 `b220999d`'s `qtap-uri.ts` / `uri-producers.ts` refactor.
`doc_store_authority(name, id, name_is_ambiguous)` is now the one place that
picks a document store's addressable reference — the name, or the UUID when the
name is ambiguous or collides with a reserved authority (`self`/`project`/
`general`). `format_doc_store_uri` delegates to it; the emitted `qtap://` bytes
are unchanged (`qtap_uri_equivalence` green over an oracle regenerated at the
pin, and v4's own oracle output is byte-identical between `8f910137` and
`b220999d`).

New `DocStoreRefResolver` (v4 `buildDocStoreRefResolver`): a precomputed
ambiguity set plus a synchronous `ref_for_mount`, with the empty-name guard and
deliberately **no** self-vault shorthand — `self` only means anything inside a
character's own prompt, and these references are handed to operator surfaces.
The ambiguity precompute is now a shared `collect_ambiguous_store_names`.

v4's commit also fixed a bug in that extraction (a self-vault throw used to
empty the ambiguity set). v5 never had it — `DocStoreUriResolver::build` has
always computed the two independently — so only the new resolver is ported; the
no-port is recorded at the source.

#### 2026-08-26 — feat(search): the LIKE-escape helper for user-supplied substring search

_Versions: core 0.0.666._

Ports v4 `lib/database/repositories/like-escape.ts` (new at `b220999d`) as
`db::like_escape`: `LIKE_ESCAPE_CHAR`, `escape_like_literal` (escapes exactly
`\\`, `%`, `_`, each with one backslash), and `like_contains_pattern` (lower-cases
inside the helper, then wraps `%…%`). Callers pair it with
`WHERE LOWER(col) LIKE ? ESCAPE '\\'` — SQLite's bare `LIKE` folds ASCII only.

v4's five unit cases ported one for one, plus a sixth pinning the escape set as
exactly those three characters.
#### 2026-08-26 — docs(porting): the P4.D121 lane record

_Docs-only change._

The lane record for the `b86bb1a5` + `d25dacc1` client halves: what landed per
tier, the four recorded mechanism divergences (the eight verbs for
`?action=instructions`, the mutate-response relist, `canArchive` for v4's
optional prop, the New-Chat checkbox gate), the Tier-3 no-op with its survey
evidence, the group-optgroup gap resolution, the gate numbers, and the four
items owed at unification.

#### 2026-08-26 — test(e2e): three gated beats for the archive walk and the dressing-instructions round trip

_Versions: SPA 0.5.572._

`salon-scenario-flow` gains the archive walk: archive a project scenario in the
ScenariosManager → it is gone from the Salon picker (the FETCH, not a filter, is
what hides it) → "Show archived" reveals it suffixed `(archived)` and still
selectable → restore it in the manager, where the archived row is badged and its
default radio disabled.

`wardrobe-flow` gains two: archive a garment from the kebab → it vanishes because
the fetch omitted it → the checkbox reveals it badged → "Restore from archive"
brings it back; and the dressing-instructions round trip (collapsed with
`None on file` → expand, type, save → `On file` → reopen the dialog and the file
came back from the server → a blank save clears it).

All three are authored GATED (ACTIVATE-AT-UNIFY by named constant):
`P4D120_SERVER_LANDED` for the archive write + `includeArchived` read, and
`P4D119_INSTRUCTIONS_LANDED` for the eight instructions verbs. Until those land a
dispatch verb silently ignores the unknown field, so an ungated beat would fail
for a reason that says nothing about this lane. The suite lists 248 tests in 66
files with all three registered.

#### 2026-08-26 — feat(wardrobe): the Dressing Instructions section, and archive/restore with server-side hiding

_Versions: SPA 0.5.571._

Both wardrobe halves of the round.

**Dressing Instructions** (v4 `b86bb1a5`). A collapsible section that edits the
browsed container's optional `Wardrobe/instructions.md` — the second-person
guidance consulted when a character chooses their own opening outfit. Mounted in
the wardrobe dialog between the container selector and the item grid, and on the
Aurora Wardrobe tab under "Open wardrobe for …". Collapsed by default;
`Consulting…` / `On file` / `None on file`; the dirty rule compares TRIMMED draft
against UNTRIMMED stored (sound because the server stores trimmed); a non-blank
save sends the UNTRIMMED draft and a blank one sends `null`; the echo is adopted,
so the field goes clean against what persisted. Toasts and strings are v4's.

Two recorded mechanism divergences: the eight verbs replace v4's
`?action=instructions`; and v4's remount key is a Lexical workaround v5 does not
need (`RichEditor` absorbs an external `value` change), so v4's composite key is
transcribed for the container-switch re-seed it also performs, not for the async
load.

**Archive/restore** (v4 `d25dacc1`). The dialog gains "Show archived" — threaded
into BOTH loaders, so the flip is a new fetch on every tier — and
`handleToggleArchived`, routed over the same two arms `toggleItemDefault` uses.
**v5's own client-side `if (i.archivedAt) return false` is DELETED**: keeping it
would be a second place for the rule to drift from the server's. The row gains the
lowercase `archived` badge and an `Archive` / `Restore from archive` kebab entry
behind BOTH `canManage` and a `canArchive` gate — Angular outputs are always
present, so v4's optional `onToggleArchived` prop becomes an explicit boolean
input defaulting to false, and the outfit composer (which does the same job the
outfit-selection LLM does) keeps offering nothing.

The container resolution pool ALWAYS asks for archived archetypes, whatever the
caller asked: a composite may bundle one, and an unresolvable component would
render as a gap. The project wardrobe card gains the checkbox and INLINE
Archive/Restore buttons (that surface has no kebab), and its mutator gains
`showArchived` / `setShowArchived` / `setItemArchived`.

#### 2026-08-26 — feat(new-chat): Show archived, the keep-the-selection exception, and the group optgroup v4 finally connected

_Versions: SPA 0.5.570._

The New Chat half of v4 `d25dacc1`.

`showArchivedScenarios` joins the reference-data load: the flag rides
`scenarioList`, `projectScenarioList` and `groupScenariosUnion`, and flipping it
re-runs both the batched load and the participant union — v4 has the flag in its
fetch effect's deps, and v5's group union lives outside `load()`, so both are
re-run to reach the same four lists. The three loaded lists carry `archived`.

The default-seed guards gain `&& !s.archived` on the PROJECT and GENERAL lookups
only. The character seed is UNGUARDED — v4-faithful and REPRODUCED, not fixed
(the commit prose says otherwise; the shipped hunks are the spec). A character
whose default scenario has since been archived still auto-selects it, and it
still renders, because the form's keep-the-selection-visible exception holds it
in the list, suffixed.

`NewChatForm` splits `allCharacterScenarios` (unfiltered) from what the dropdown
offers (filtered, with the currently-selected id ALWAYS kept, so an archived pick
made a moment ago does not blank the select out from under the user). The
selected-scenario preview is looked up against the UNFILTERED list, so a chat
already pointing at an archived scenario keeps previewing it.

**The group optgroup gap is closed.** v4's form declared a `groupScenarios` prop
and — since `44a8137e` — handed it to the shared picker, but neither caller ever
passed it, so the Group Scenarios optgroup never appeared; v5 had ported that
always-broken shape faithfully, with the reason in its class header. `d25dacc1`
connected it, so the tier is now wired here too — which also makes the group arms
of the preset preview and the selection chain reachable for the first time.

One recorded mechanism divergence: v4 renders the checkbox only when its
change-callback prop is supplied, so `NewChatModal` stays a pure consumer. v5
never ported that modal, and this form reads `NewChatState` directly rather than
taking scenario props, so there is no caller that could withhold the setter — the
checkbox always renders, which is what v4's page (the one caller v5 has) does.

#### 2026-08-26 — feat(scenarios): archive and restore across the managers and the character edit form

_Versions: SPA 0.5.569._

The scenario management half of v4 `d25dacc1`.

`ScenarioMutator` gains `showArchived` / `setShowArchived` / `setScenarioArchived`,
and the list op takes the flag. Flipping the checkbox is a NEW request, never a
client-side pass — the server is the single source of truth for what is hidden,
so a surface that never asks is safe by construction. `setScenarioArchived`
re-sends the row's own fields with the flag and drops an `isDefault` claim on the
way in, since an archived scenario can never win default resolution and a dead
`isDefault: true` would sit in the file. A path no longer in the list answers
v4's `Scenario not found in current list`.

⚠ One recorded mechanism divergence: v4 threads `?includeArchived=true` onto the
MUTATE urls too, so a PUT answers a freshly listed set that still contains the row
it just changed. The Shared contract puts `includeArchived` on the LIST verbs
only, and a dispatch verb silently ignores an unknown field — so with the toggle
on, the mutate body is discarded and the list is re-read through the one verb that
honours the flag. Same final list, no intermediate paint. Pinned in
`scenarios.api.spec.ts` in both directions.

`ScenariosManager` gains the checkbox in a `justify-between` row with
`+ New scenario` and a no-confirm `handleToggleArchived` (archiving destroys
nothing). `ScenarioRow` goes three → four actions in BOTH layouts, badges an
archived row, and disables the default radio with v4's
`Archived scenarios cannot be the default`. New spec file, transcribed from v4's
own `ScenarioRow.test.tsx`.

The character edit form's scenario array gains per-row Archive / Restore against
LOCAL form data, plus v4's archiving help paragraph. Restoring DELETES the key
rather than writing `archived: false`; every mutation spreads the existing object,
so `description` and anything else this editor never renders survives a save — the
rebuilt-bag trap v4 retyped its local interface to avoid.

#### 2026-08-26 — feat(scenario): the archived marker in the shared picker, and Show archived in the Salon sidebar

_Versions: SPA 0.5.568._

v4 `d25dacc1`'s picker half. `ARCHIVED_OPTION_SUFFIX = ' (archived)'` lands beside
`CUSTOM_SCENARIO_VALUE`, and all four option shapes gain `archived?: boolean`. A
native `<option>` cannot hold a badge element, so the marker is text — appended
after the default marker and before the ` — description`, giving v4's
`Tavern (project default) (archived) — a cozy inn`. Archived entries only reach
the component when the surface asked the server for them, and when they do they
stay selectable: archiving hides an entry from the default view, it does not
forbid one a human has deliberately gone looking for.

`ChatScenarioControl` gains "Show archived". The flip re-fetches all four tiers
rather than filtering what is loaded, and `includeArchived` is part of every
TanStack key (`scenarioKeys.general(flag)` and friends), so the archived-free
answer and the archived-inclusive one cannot overwrite each other. The scenario
fetchers thread the flag; `toScenarioOption` narrows the wire value with v4's
`=== true`, so a server that predates the field reads as active rather than
leaking `undefined` into the label's truthiness test.

#### 2026-08-26 — feat(spa): the P4.D121 contract surface — eight dressing-instructions verbs, the archive fields, and the field hint

_Versions: SPA 0.5.567._

The client-side contract for v4 `b86bb1a5` + `d25dacc1`, ahead of the surfaces
that consume it.

`core-contract.ts` gains the eight `*WardrobeInstructions{Get,Set}` verbs and
their `{ instructions: string | null }` body (Shared contract A1/A2). v4
expresses these as `?action=instructions` on the four wardrobe collection
routes; v5 has no URLs, so the four containers x two directions become eight
verbs — recorded as a mechanism divergence in the type's doc comment.
`instructions` is REQUIRED and nullable on every SET, matching v4's
`z.string().nullable()`.

The archive fields (Shared contract B1-B3): `includeArchived?: boolean` on the
nine list verbs; `archived?: boolean` on `ScenarioCreateBag` and (tri-state, with
its rule spelled out) `ScenarioUpdateBag`; `archived: boolean` — always present —
on `ScenarioDto`; and `description?` + `archived?` on the character-scenario
shape, the second only ever `true` (omission is what "active" means in the vault
file). `description` is on the canonical shape rather than a listing projection
because the character edit form round-trips whole scenario objects.

`prompt-field-hints.ts` gains `wardrobeInstructions`, transcribed byte-for-byte
from `git show b86bb1a5:components/prompt-fields/field-hints.ts` (curly quotes,
curly apostrophes and the em dash included) into v4's slot between
`groupInstructions` and `roleplayTemplatePrompt`. The parity spec's independent
second transcription and its key-count and typographic-apostrophe guards move
with it.

#### 2026-08-25 — docs(porting): the `b220999d`-round work orders — four lanes over the five-commit drift

_Docs-only change._

`/setupphase` over the drift ledger's five UNPROCESSED rows (probe passed:
v4 clean on `main` at `b220999d`, `bugfix` unmoved). Four work orders
committed, each with a fresh hunk-level v4 survey dated 2026-08-25:
`p4.d119-wardrobe-instructions-server.md` (the per-tier dressing
instructions server half — the cascade module, `preserve_file_names`, the
reader skip, the outfit-prompt thread at BOTH of v5's `llm_choose`
entrances, the four `?action=instructions` surfaces as dispatch verbs) and
`p4.d120-archive-entries-server.md` (the archive feature server half —
scenario `archived` frontmatter across all four scopes with the
default-suppression rule, the wardrobe `archivedPatch` semantics,
`includeArchived` end-to-end, the Green Room pins, and v4's
`buildScenarioFile` description-round-trip fix, a bug v5 replicates
verbatim today) run STACKED in one lane in v4's own prerequisite order;
`p4.d121-instructions-archive-spa.md` carries both features' client halves
against a two-part pinned contract; `p4.d122-documents-search-vertical.md`
carries the Documents-search feature whole (the LIKE-based text-search
engine with its fail-closed archived-vault exclusion, the two repo scans,
the `uiSearch` sixth type + chip reorder, the ref resolver, and the SPA
chip/card/open-choreography — flagging the two v5-specific hazards the
survey found: `DmpRow` carries no `storeType`, and the results component's
click handler would break v4's modified-click passthrough). Ledger §3 rows
marked ORDERED; a planning block appended to `phase-4.md`. The surveys also
recorded upstream-filing candidates (v4's startup-migration dedupe hole,
the three unconverted `scenarios[0]` sites, the unguarded archived-default
seed) for the lanes to carry into their records.

#### 2026-08-26 — docs(porting): drift check — five v4 commits, three features on just-ported surfaces

_Docs-only change._

`/driftcheck` against the `8f9101370` baseline. v4 `main` has moved five
commits (all dated 2026-08-25); `bugfix` is unmoved at `3a76b17df` with no
unabsorbed content; the checkout is clean on `main`.

Three features, two docs-only specs. `b86bb1a58` adds a per-tier
`Wardrobe/instructions.md` read when a character dresses themselves —
landing on the tri-tier cascade (P4.D39), the vault projection sweep (which
gains a `preserveFileNames` exemption), the four wardrobe collection routes
(P4.D112/P4.D113), the Almanack garment counts, and the prompt-field hints
table. `d25dacc1d` makes scenarios and wardrobe items archivable across all
four scopes — 84 files, landing squarely on the scenario feature unified the
day before (P4.D115/P4.D116) and on the Green Room candidate pool, with
server-side filtering replacing two client-side passes. `b220999da` adds a
`documents` type to the global search: a new document-text-search module
over the doc-mount repos, plus a behavior change on the ported `uiSearch`
verb (P4.9P) whose ordered type list also reorders the existing chips and
result groups. The two docs commits are the specs for the latter two.

No SQL DDL moved, so no D23 re-dump is implied — the one `schema.ts` hunk is
the vault overlay's TS constant re-export. No bug-doc changes in the range,
so no convergence rows.

**The regen rule flips to PIN REQUIRED**: v4 HEAD is past the baseline, so
every oracle regeneration now runs from a lane-unique detached worktree
pinned at `8f9101370` (§5.1).

#### 2026-08-25 — test(salon): the scenario walk's first live run — three gesture repairs

_Versions: SPA 0.5.566._

The `salon-scenario-flow` beat was authored gated (its lane could never run
it) and failed its FIRST live run at unification on three spec-side defects,
each fixed with no product assertion weakened: `waitForHealth` accepted only
`res.ok`, but a fresh fixture server boots LOCKED and answers 423 (the
sibling own-server specs' spelling adopted, plus a server-side `unlock`
dispatch before the API seeding); the seeding sent the RESPONSE tag
(`chats`) as a request verb where the request verb is `listChats`; and the
revision-body assertion used an unscoped `getByText` that (correctly)
matched both the Host bubble and the picker's preset preview — now scoped to
the transcript. The walk then passed whole: seeded scene → picker opens on
it → preset change → revision bubble → reload opens on the preset → no-op →
clear → the cleared sentence.

#### 2026-08-25 — docs(porting): the 8f910137-round unification — P4.D115–P4.D118 land whole; the baseline moves

_Docs-only change._

The four-lane `8f910137` drift catch-up round unifies: the scenario-change
feature end-to-end (server verb + SPA control + the activated
`salon-scenario-flow` walk), the client-fixes pair (bugs 100/102 — the qt-*
sheet made real + the `check-qt-classes` guard; bug 99 — the gallery download
+ the modal's escape from the workspace stacking trap), and bug 101's
completion templates with the bash-driving guard. The oracle baseline moves
`f6a10055` → `8f910137` and the drift debt is cleared (`8f910137` itself
NO-PORT-RATIFIED: CI + tests-only).

Gate: fmt/clippy (both feature sets)/release clean; oracles regenerated fresh
at the new baseline (50 + 19 rows, changed-bytes verified); the differentials
by name zero SKIP (`chat_scenario_routes_equivalence` 50 cases, the capstone
neutrality, Tier R 188/0 vs v4's real launcher, `completion_behavior` 4/0);
`cargo test --workspace` 456 binaries / 2,376 / 0 with the delta reconciled
exactly; SPA 347 files / 5,196 / 0 with the qt-class guard green over the
union (934 classes); `npm run build` clean; full Playwright 244 passed / 0 failed / 1 skipped
(the pre-existing wardrobe store-probe park; the suite grew 242 → 245 with
the round's three beats).

The §3 review caught one log-only fidelity gap (fixed, see the entry below);
the wires applied the cross-lane qt rewrite, activated the walk, and diffed
the shared contract name-for-name. Final versions: core 0.0.665, harness
0.0.577, cli 0.0.12, SPA 0.5.566.

#### 2026-08-25 — fix(chat): the scenario verb's `source` log label follows JS truthiness

_Versions: core 0.0.665._

The §3 unification review's catch on the `8f910137` round: `source_label` (the
`source` field of the verb's `Scenario changed` info log) tested `is_some()`
where v4's ternary cascade tests JS truthiness, so an empty-string path — which
`z.string().max(500).nullish()` admits — would have been labelled by its tier
where v4 logs `custom`. Log-only (no differential can see the field), so it is
pinned by a new unit test (`source_label_treats_empty_strings_as_falsy`).

#### 2026-08-25 — chore(porting): the 8f910137-round unification wires

_Versions: SPA 0.5.565._

The cross-lane hand-offs no single lane could do: P4.D117's
`qt-text-tertiary` → `qt-text-secondary` rewrite applied in the three
P4.D116-owned `new-chat` files and the guard's `PENDING_CROSS_LANE_SITES`
tripwire block deleted (the guard now reports 934 defined classes with every
guarded reference resolving, over BOTH SPA lanes' files); P4.D116's
`salon-scenario-flow` e2e beat activated (`P4D115_SERVER_LANDED` → true) now
the `chatSetScenario` verb is on the branch; and the §Shared contract diffed
name-for-name across sides — the SPA's `ChatSetScenarioRequest` fields match
the Rust variant's wire spellings exactly, and the differential already decodes
the literal `{"type":"chatSetScenario", …}` wire shape through the `Request`
enum.

#### 2026-08-25 — feat(chat): the scene can be changed without leaving the conversation (server)

_Versions: core 0.0.664, harness 0.0.577._

Ports the server half of v4 `44a8137e`: the `chatSetScenario` dispatch verb
(v4 `POST /api/v1/chats/[id]?action=scenario`), the chat-GET `scenarioText`
projection, and the `ChatUpdate.scenario_text` setter the verb needs.

The verb rewrites `chat.scenarioText`, recompiles every participant's identity
stack (best-effort, as v4 does — the compiler has a read-through fallback), and
posts the Host's revision announcement. Re-picking the scene already in force
is a no-op: no write, no recompile, no announcement, and `changed: false`. A
character `scenarioId` is resolved against whichever active seat owns it, and a
character whose vault will not open is skipped rather than failing the change.

The guard order is the COMPOSITE v4 exposes at the URL, not the handler's own:
`handlePost` 404s on a missing chat before it dispatches, so an invalid body
against a missing chat is a 404 and not a 400. The six body fields are raw
`Value`s in the dispatch variant so a wrong-typed field reaches v4's flat
`Validation error` instead of the transport's own decode 400.

`ChatUpdate` gains `scenario_text`, and the dispatch `chatUpdate` bag stays
closed around it — v4 keeps `scenarioText` off `updateChatSchema` on purpose,
because a bare field update would skip the recompile and the announcement. Both
halves are pinned: a `chatUpdate` carrying the key leaves the column untouched,
and a source census fails if the setter gains a second construction site.

New family `chat_scenario_routes_equivalence` (50 cases over the new committed
`chat-scenario-{main,mount}.db` fixture): the four preset tiers, the precedence
chain, every fail-soft fall-through, the clear/no-op/refusal arms, and three
`export_*` cases proving the revision notice survives into a Markdown
transcript — red-first before `scenario-change` joined the exported kinds.

#### 2026-08-25 — feat(host): the Host announces a scene revised mid-conversation

_Versions: core 0.0.663._

Ports v4 `44a8137e`'s `postHostScenarioRevisionAnnouncement` and the new
`scenario-change` host kind. The four strings are byte-exact; a blank scene
takes the cleared pair ("The Host draws the previous scene aside…") rather than
returning early, which is where this writer differs from the chat-start
`scenario` announcement it deliberately does not resemble — the opening notice
still stands earlier in the transcript, so the revision has to read as
superseding it rather than contradicting it.

`scenario-change` also joins `HOST_LINK_KINDS` in the Markdown transcript
export, with v4's rewritten why-comment: the header prints whatever scene is in
force at export time, so without the revision notices a reader would see the
story relocate with nothing to mark the move.

The branch that picks the cleared pair is split into a pure helper so it is
testable; the blank-but-present arm is v4 code the `?action=scenario` verb
cannot reach (`combine_scenario_text` trims), pinned by unit test in both
directions. A cross-module test asserts the kind the writer stamps is one the
transcript keeps — a mismatch would drop every revision notice from an export
in silence.

#### 2026-08-25 — feat(chat): the scenario precedence chain moves into its own resolver

_Versions: core 0.0.662._

Ports v4 `44a8137e`'s `lib/chat/scenario-selection.ts` extraction. The
four-tier precedence chain (character `scenarioId` > project path > group path
> general path, free text layered beneath whatever resolves) leaves
`chat_create.rs` for the new `services::scenario_selection`, so the in-chat
`?action=scenario` verb landing next resolves a selection exactly the way the
New Chat dialog does. Every tier still fails soft; the warnings v5's inline
chain never emitted are now carried, tagged with the caller's log prefix.

The extraction closes a latent divergence on the way: v4 guards each tier with
JS truthiness, so an empty-string pointer (`scenarioId: ''`, an empty
`projectId`, an empty resolved body) is treated as absent and the chain falls
through. v5's inline chain treated `Some("")` as present. The resolver models
`presetBody` as a plain `String` whose emptiness is the falsy test, matching
v4 arm for arm.

Neutrality proven by regenerating `chat_create_capstone` from a v4 worktree
pinned at `44a8137e` (which carries v4's own extraction) and re-running it
green; the family's `two_char_scenario` case is the live create-path scenario
arm. The resolver's own four-tier differential arrives with the verb.
#### 2026-08-25 — docs(porting): the P4.D116 lane gate and unifier notes

_Versions: SPA 0.5.561._

Records the lane's verification gate (fmt, both clippy feature sets, the
workspace tests, the SPA suite and build, the full Playwright run), what did not
land and why, and the two notes the unifier needs: the five files that sit
outside the Ownership table's three directories but are required by the order's
own deliverables, and this lane's claim on e2e port 4330.

#### 2026-08-25 — test(salon): the scenario-change kind at both render sites, and the gated picker walk (P4.D116 units 5-6)

_Versions: SPA 0.5.560._

v4 `44a8137e` added the `scenario-change` announcement kind and NO table entry
for it — no display override, no host importance row — so both apps answer
through their fall-through arms: the label de-hyphenates to "scenario change"
and the dot lands on the host tier's `'*'`, medium. Verified rather than
assumed, and pinned at both of v5's Staff render sites (the P4.D36 whisper-tag
pair): the chip that `chat-view-model` builds and `announcement-group` draws,
and the header bar `message-row` draws for the carve-out senders. A row with no
`systemKind` column is pinned too — the revision wording is deliberately unlike
the chat-start "The Host sets the scene", so content inference correctly
declines to call it `scenario`.

The e2e beat lands gated on `P4D115_SERVER_LANDED`: a chat is seeded with a
scene that matches no preset, the picker opens on Custom holding that text (the
GET projection), a project scenario is picked and the Host announces the
revision, re-picking the same scene announces nothing, and an empty Custom box
clears it. It runs a dedicated server on port 4330 over a copy of the salon
fixture; the project and its `Scenarios/` entry are created through the API,
since a scenario file lives in a document store and cannot be planted with SQL.

#### 2026-08-25 — feat(salon): the scene can be changed without leaving the conversation (P4.D116 units 3-4)

_Versions: SPA 0.5.559._

The client half of v4 `44a8137e`'s in-chat picker. The Chat drawer gains a
Scenario control offering the same four tiers the New Chat dialog does —
project, general, group, and (when a single LLM character is present) character
— plus a Custom option that reveals a free-text box. Saving dispatches the new
`chatSetScenario` verb; an empty custom scenario clears the scene, and the
server's own message is what the toast says, the `changed: false` no-op arm
("Scenario unchanged") included.

The control opens on the scene actually in force. `ChatDetail` gains
`scenarioText` — v4's chat GET never projected it, which is why its picker
could only ever open on "Custom…", even immediately after a save — and the
seed is derived rather than copied into state, so it settles as the option
tiers finish loading without fighting a choice the user has since made.

The tier lists get their own per-scope query keys (v4's `queryKeys.scenarios`)
and reuse the existing list verbs; the group tier is keyed on the comma-joined,
sorted LLM-character ids. The sidebar derives that cast from the raw
`controlledBy` column rather than `isUserDrivenSeat`, which is v4's actual
spelling and the right one: an impersonated LLM character is still an LLM
character whose group memberships belong in the picker.

#### 2026-08-25 — refactor(new-chat): the scenario dropdown moves onto the shared component (P4.D116 unit 2)

_Versions: SPA 0.5.558._

Mirrors v4 `44a8137e`'s New Chat half: `new-chat.types.ts` re-exports the option
shapes and tokens from `app/scenario/scenario.types` instead of declaring its
own, the form renders `<qt-scenario-select>` in place of its inline `<select>`,
and the token-to-patch helper becomes the composition of the shared parser with
a new selection-taking `scenarioSelectionPatch` — which is exactly what v4's
refactor made it.

The refactor is equivalence-preserving and the existing new-chat specs are
green UNCHANGED (additions only). One arm genuinely moved and is pinned: a
malformed group token (`group:` with no second colon) used to fall out of the
group branch into the character arm and be stored whole as a `scenarioId`; the
shared parser returns custom from inside that branch, so it now clears every
source. Unreachable from the rendered dropdown either way.

The character tier now carries its ` — description` suffix, which v4 has always
rendered and v5's inline copy had dropped. The group optgroup stays absent: v4
declares the prop and hands it to the shared component, but neither of its two
callers ever passes it, so v4 renders no group optgroup either.

#### 2026-08-25 — feat(scenario): the scenario dropdown becomes a shared component (P4.D116 unit 1)

_Versions: SPA 0.5.557._

Ports the client half of v4 `44a8137e`'s first move: the scenario option
shapes, the `<option value>` tokens, and the dropdown itself extracted into
`apps/web/src/app/scenario/` so the New Chat dialog and the Salon sidebar's
new in-chat picker render the same control. Transcribed 1:1 from v4's new
`components/scenario/types.ts` and `components/scenario/ScenarioSelect.tsx`.

Three Angular-port notes are pinned by spec. Each option's label is computed
as one string and interpolated once, because v4 concatenates three adjacent
JSX expressions with nothing between them and separate template lines would
collapse to a stray space. The host carries `block`, so `.qt-select`'s
`w-full` has a real box to fill. And the selected row is assigned in an
`afterRenderEffect` rather than through `[value]`/`[selected]`: React assigns
`select.value` after the children mount, so a selection naming no rendered
option leaves the control blank instead of snapping to row 0 - reachable here
whenever a tier's list refetches without the row the current selection names.
#### 2026-08-25 — docs(porting): the P4.D117 lane record — deferrals and the gate

_Docs-only change._

The lane's loud deferrals (the `help/character-gallery.md` bullet to the `p4.9i2`
bank, the pre-existing `qt-image-gallery` host gap, the five `qt-text-tertiary`
sites in P4.D116's directory that the guard holds under a self-retiring tripwire,
and `photo-gallery-modal`'s un-portaled overlay which matches v4) and the gate
numbers.

Also recorded: the port-4319 collision the order warned about, which happened.
Playwright's global setup neither kills a squatter nor fails on the bind — it
waits for the port to answer, and a sibling lane's server answers, so the run
proceeds silently against the other lane's build and reads as ordinary failures.

#### 2026-08-25 — fix(images): the image detail modal escapes the workspace stacking trap (bug 99, part 2)

_Versions: SPA 0.5.559._

The stacking half of v4 `8018c487`, and v5 measurably had it. `.qt-workspace`
carries `isolation: isolate`, which makes it a stacking context, so the modal's
`z-[60]` was resolved inside the workspace rather than against the page and the
sticky `.qt-page-toolbar` (`z-30`, painted by an ancestor context) covered the
whole strip its Download/Copy/Save/Close cluster occupies. Nothing was clipped or
mispositioned — the controls laid out exactly where they belonged and were simply
painted over, so Escape still closed the modal and the picture was merely
unsaveable.

Measured before the fix by the new e2e beat: `document.elementFromPoint()` at the
Download control's centre returned the toolbar's queue-status badges, while
Playwright's `toBeVisible()` passed on the same controls in the same failing run.
No non-browser layer can see this — jsdom runs no compositing.

The modal now reparents its host to `document.body`, the v5 equivalent of v4's
`createPortal`, and the idiom `search-dialog.ts` established for bug 40. Not
z-index escalation: v4 rejects that and the workspace isolation is load-bearing.

The reparent runs in `afterNextRender`, not the constructor. Every host mounts
this modal under an `@if`, and an embedded view's root nodes are attached only
after the view is created, so a constructor-time reparent is silently undone by
the insertion that follows — which is what the full suite caught. That discovery
also exposed two assertions in sibling specs that queried the modal inside the
fixture's own subtree, where a portaled modal can never be; both now query the
document.

#### 2026-08-25 — fix(images): a character's photo gallery can download a picture again (bug 99, part 1)

_Versions: SPA 0.5.558._

The download half of v4 `8018c487`. `af1bc479` (P4.D114) gave the hover Download
button to every other image grid and missed the embedded character gallery, so a
photo in a character's album could only be saved by opening the detail view —
which, under a shell with no right-click Save Image, meant not at all.

The Photo Gallery tab's tile overlay gains v4's Download button between Set as
avatar and Delete, gated on `!isMissingImage` exactly as v4 gates it. The handler
is v4's `useGalleryData.handleDownloadImage` verbatim: the src is `image.url`
when set and the filepath with a leading `/` forced on otherwise, a non-ok fetch
throws `Failed to fetch image (${status})`, and the blob goes to
`triggerBlobDownload` under the entry's own `fileName`. The failure toast reads
the fixed `Failed to download image`; the thrown message reaches only the console
line, with v4's exact three-key bag.

The tile → `ImageData` mapping was factored out of `selectedImage` so the
download handler and the detail modal read the same projection, as v4's hook and
modal read the same `GalleryImage`.

Four specs pin it: the control's presence and `aria-label`, its absence on a tile
whose bytes failed to load, the downloaded filename's source (the entry's
`fileName`, not the blob path's basename), and the fixed toast sentence beside
the real logged one.

#### 2026-08-25 — fix(themes): hover and opacity qt-* classes actually style something (bugs 100, 102)

_Versions: SPA 0.5.557._

The v5 port of v4 `309aaa97`. v5 ported the `qt-*` utility sheet at P4.D34 and
inherited both halves of the defect with it: measured here at **69 class names
over 364 call sites**, all resolving to no CSS rule at all.

Two mechanisms. Tailwind v4 generates variants only for utilities it owns, and a
class declared inside `@layer utilities` is not one of those — so `hover:qt-bg-muted`
was never "qt-bg-muted, on hover", it was a class name nobody defined. Opacity
modifiers fail the same way: `qt-bg-muted/50` is not qt-bg-muted at half strength.
`_utilities.css` gains v4's +490 lines — the missing opacity steps for the muted,
card, primary, destructive, success, warning, info and secondary tokens, the border
and text steps to match, `qt-bg-secondary` / `qt-bg-input`, the `qt-text-on-*`
family (bug 100's `qt-text-*-foreground` names were the Tailwind spelling with a
`qt-` bolted on), and a STATE VARIANTS section with every state form written out
and escaped by hand. The sheet is now byte-identical to v4's, modulo the two
comment blocks v5 already carried.

Thirty-seven component files had invented a name that was never vocabulary
(`qt-text-error`, `qt-text-sm`, `qt-surface-alt` and friends); those moved onto
the class that already existed rather than getting a definition, following v4's
rewrite table exactly.

The durable half is `apps/web/scripts/check-qt-classes.mjs`, run by `npm run lint`
and ahead of the unit suite by `npm test`. It holds every `qt-bg-*` / `qt-text-*` /
`qt-border-*` / `qt-shadow-*` reference, and every variant-prefixed `qt-*`
reference whatever its family, against the selectors the stylesheets actually
define, and fails on one that resolves to nothing. A class that does not exist is
indistinguishable from one that inherits at every automated layer short of a real
browser, which is how this survived three separate findings.

Two v5-only adaptations: class strings live in Angular inline templates inside
`.ts` files and in `.html` files, and an Angular component selector is a
`qt-`-prefixed token too — `qt-text-replacement-settings` collides head-on with
the `qt-text-` family — so the selectors are read out of the source itself rather
than kept in an allowlist that would rot.
#### 2026-08-25 — test(cli): the shipped completion templates are driven under a real shell

_Versions: cli 0.0.12._

P4.D118 unit 2 — the v5-side mirror of v4's `completion-behavior.test.js`
(new at `6afacb18`, extended at `8f910137`; both vintages' cases are carried).
Tier R proves the templates are v4's bytes; this proves the bytes actually do
something.

`crates/quilltap-cli/tests/completion_behavior.rs` writes a stub `quilltap` to a
temp dir — v4's own `makeStubBin` shape, answering `instances list --names-only`
with `StubInstance` and `docs list --names-only` with `Stub Store\nOther Store`
— sources the template under a real `bash` with `COMP_LINE`, `COMP_WORDS` and
`COMP_CWORD` set as at a prompt, and reads `COMPREPLY` back. The bytes under
test are the shipped bytes: the test `include_str!`s the same paths
`src/completion_cmd.rs` emits from, so a template that drifted from what
`quilltap completion bash` prints cannot exist.

Thirteen bash cases: the verb list survives no flags, `--instance Friday`,
`-i Friday`, `--limit 5`, `--json`, and flags on both sides; `db --limit 5`
still offers `characters` and `db characters --instance Friday` still offers
`status`; `memories -i` offers `ls` and withholds instance names, because `-i`
is `--ignore-case` there. Store names carrying a space come back
`printf '%q'`-escaped as `Stub\ Store` on `--mount`, on the `docs ls`
positional, and on the destination store of `docs move Src a.md` — and are
withheld from `docs find`, which takes a query. Four zsh cases, structural as
v4's are (its completion system only runs inside a widget): no `(( CURRENT ==`
word-index test survives, both `(-)`-prefixed top-level positional specs are
present, at least six `1: :->` positional dispatchers exist, and `zsh -n`
accepts the template. Each shell is probed first and skips with a loud named
message rather than passing silently — v4 added that guard for zsh at
`8f910137` because GitHub's ubuntu runners ship without the shell.

Red-proven: swapping the pre-fix templates back in fails three of the four
tests with exactly bug 101's symptoms. The fourth,
`zsh_template_is_syntactically_valid`, passes on both vintages, correctly — the
old zsh template parsed fine, it was semantically wrong.

#### 2026-08-25 — fix(cli): shell completion survives flags on the line (v4 bug 101)

_Versions: cli 0.0.11._

P4.D118 unit 1 — the v4 `6afacb18` drift. All three completion templates
(`bash`, `zsh`, `fish`) re-copied at v4's post-fix bytes. v4's emitter reads its
template file and writes it through untouched, and so does ours, so the shipped
bytes ARE v4's file: the port is a byte copy and the differential is Tier R
against v4's real launcher.

**v5 measurably had bug 101's bash half.** Driven under a real bash against a
stub `quilltap` (v4's own `makeStubBin` shape), the pre-copy templates answered
`quilltap docs --limit 5 <TAB>` with the docs *flag* list instead of the verb
list, chopped `Stub Store` into `Stub` and `Store` on `--mount`, offered no
store names at all on a store-taking positional such as `docs ls`, and read
`memories -i` as `--instance` (offering instance names) rather than
`--ignore-case`.

What the new bytes bring: the bash scanner tracks which flags take a value *per
subcommand*, so a flag's value can no longer be mistaken for the subcommand's
verb — which also settles `-o` (the global `--open` vs themes' `--output`) and
`memories -i`; `_quilltap_lines_compreply` fills `COMPREPLY` from
newline-separated candidates with `printf '%q'` escaping, so store and instance
names carrying spaces survive; `_quilltap_docs_positional` knows which docs
positionals name a store and which name a local path (`move|copy|link` want one
at both 2 and 4; `export`'s third arg is a local directory); and the live
lookups reuse the `-i`/`-d`/`--passphrase` already on the line so they query the
instance being addressed. The zsh template hands every subcommand's options and
positionals to one `_arguments -C` call and branches on the parsed state, with
`(-)` on the top-level positionals keeping a flag typed after the subcommand
with that subcommand. fish, which never had the bug, gains store names on
`--mount`.

Tier R red-first at a v4 worktree pinned to `6afacb18`: 188 cases, **3
failures — exactly `completion bash`, `completion zsh`, `completion fish`**,
each a stdout difference. After the copy: 188 cases, 0 failures.

#### 2026-08-25 — docs(porting): the four work orders for the `8f910137` drift catch-up round (P4.D115–P4.D118)

_Docs-only change._

The next round is the drift catch-up over the five v4 commits past the
`f6a10055` baseline, planned as four parallel lanes:

- **P4.D115** (`work-orders/p4.d115-scenario-change-server.md`) — the
  `44a8137e` scenario-change feature, server half: the scenario-selection
  resolver extracted to its own module (chat-create refactored onto it,
  neutrality-proven), the `chatSetScenario` verb with v4's guard order and
  no-op semantics, the chat-GET `scenarioText` projection, the Host
  scenario-revision announcement strings byte-exact, and the transcript
  export's `scenario-change` carry.
- **P4.D116** (`work-orders/p4.d116-scenario-change-spa.md`) — the client
  half: the shared ScenarioSelect extraction with the New-Chat form
  refactored onto it, the in-chat ChatScenarioControl in the Chat drawer,
  the sidebar/SalonView threading, the scenario query keys, and a gated e2e
  beat. Meets P4.D115 only at the pinned shared contract.
- **P4.D117** (`work-orders/p4.d117-qt-classes-gallery-download.md`) — the
  client-fixes pair: v4 bugs 100/102 (the qt-* utility sheet's missing
  opacity steps, hand-written state variants, and `qt-text-on-*` family,
  plus the call-site sweep and a ported `check-qt-classes` build guard —
  v5 confirmed to carry 20+ files of inert names) and bug 99 (the
  embedded-gallery download + the image-detail modal freed from the
  workspace stacking trap, measured before ported).
- **P4.D118** (`work-orders/p4.d118-cli-completion-bug101.md`) — v4 bug
  101: the three shell-completion templates re-byte-copied (flag-tolerant
  verb lookup, space-safe candidates), proven through Tier R red-first plus
  a v5-side bash-driving behavioral guard mirroring v4's new test; also
  gathers the NO-PORT evidence for the CI-only `8f910137`.

The drift ledger's five §3 rows move to ORDERED. The regen rule stands:
pin `f6a10055` (or the lane's own later pin where an order says so) for
every regen until the round moves the baseline.

#### 2026-08-25 — docs(porting): the v4 drift ledger and /driftcheck; the porting commands probe instead of re-checking

_Docs-only change._

Drift tracking moves into the repo. `docs/developer/porting/drift-ledger.md`
is now the single record of where v4 stands relative to the oracle baseline:
the current state and the regen rule in force (§1), a four-command read-only
freshness probe consumers run (§2), the per-commit drift table with a
disposition lifecycle — UNPROCESSED → ORDERED → ABSORBED/NO-PORT-RATIFIED
(§3), the full check method (§4), and the standing recipes and traps formerly
kept only in session memory: the pinned-worktree regen recipe with all three
symlink classes, the silent-stale-pass discipline, commit-prose-misdescribes-
the-diff, convergence-measure-don't-assume, and expiring live proofs (§5).

A new `/driftcheck` command runs the check and updates the ledger. The other
porting commands (`/setupphase`, `/carryout`, `/unify`, `/dogfood`) no longer
check drift themselves: they run the ledger's §2 probe and read what the last
`/driftcheck` recorded. Lanes STOP on a probe failure and never write the
ledger; `/unify` owns moving the baseline and retiring absorbed rows.
CLAUDE.md's baseline bullet now points at the ledger instead of carrying the
method. (The ledger file itself rode into the previous commit by accident of
a shared working tree; this commit carries the commands and the CLAUDE.md
pointer.)

The seeding check found v4 main FIVE commits past `f6a10055` — three more
than the round record knew: bug 101's completion-template rewrite, bugs
100/102's qt-* utility repair, and a NO-PORT-candidate CI fix, alongside the
two already recorded. All five sit UNPROCESSED in the ledger; the regen rule
is pin-required.

#### 2026-08-25 — test(images): pin the moderation reroute to the reconstructed message (finding #104)

_Versions: core 0.0.661._

Finding #104's fix turned out to restore a dead feature, not just improve an
error string. The Concierge decides whether to retry image generation on the
uncensored profile by keyword-matching the error message
(`is_image_moderation_error`). While every non-2xx from an SDK-backed provider
collapsed into `Invalid response from <name> Images API`, nothing matched, the
reroute never fired, and AUTO_ROUTE image generation was dead for OPENAI, GROK,
Z_AI and NANOGPT.

Measured live on one chat before and after: the job went FAILED with a single
GROK attempt, then COMPLETED with two — GROK refusing with
`400 "Generated image rejected by content moderation."` and NanoGPT/chroma
answering `Generated 1 image(s) (Concierge reroute)`. v4 was never affected
because its SDK throws that message.

Adds `a_moderation_400_still_reads_as_a_moderation_error_downstream`, which runs
the reconstructed message through `is_image_moderation_error` and keeps the
pre-fix sentence as the counter-example, so re-wording the reconstruction can
never silently switch the reroute off again. Mutation-proven.

#### 2026-08-25 — docs(porting): the E6/E7/G6 follow-up rows and finding #104's record

_Docs-only change._

The walk doc's three deferred rows resolved (avatar-preview download both arms,
Generate Image download, the candid story-background arm measured at 4,255
UTF-16 units) and the finding #104 row with its commit hash.

#### 2026-08-25 — fix(images): a provider's own error reaches the operator (dogfood finding #104)

_Versions: core 0.0.660._

Every non-2xx from an SDK-backed image provider was collapsing into the generic
`Invalid response from <name> Images API`. v4 generates through the OpenAI SDK
for OPENAI, GROK, Z_AI and NANOGPT, and the SDK throws an `APIError` on any
non-2xx carrying the API's own message — its `Invalid response` sentence is
reserved for a 2xx with a malformed body. v5 fetches the wire itself and handed
the response to the parser whatever the status; a 400 body has no `data` key, so
every failure read as a malformed body.

Found by dogfooding: a real story-background generation failed with
`Invalid response from Grok Images API`, the same opaque sentence an earlier
pass had recorded as an unexplained open question. Replaying the exact prompt
against the API returned `400 {"error":"Generated image rejected by content
moderation."}` — the reason, discarded.

The status gate now sits in `generate_image` for the four SDK providers, and
`openai_sdk_error` implements the SDK's full three-way message rule instead of
only one arm: a string `error` is JSON-stringified (quotes included, which is
exactly Grok's shape), an object error yields its plain message, and a non-JSON
body falls back to the raw text. All four rules were measured against the real
SDK through a stub server rather than transcribed.

GOOGLE and OPENROUTER are raw-`fetch` in v4 and keep their own sentences. A
mutation widening the gate to every provider — silently replacing those — stayed
green until `sdk_and_raw_fetch_providers_keep_their_own_non_2xx_sentences` was
added to pin the split in both directions.

#### 2026-08-25 — docs(porting): the f6a10055-round dogfood pass — 41 rows, finding #103, #98 closed

_Docs-only change._

The walk doc for the `f6a10055` wardrobe-containers dogfood pass, its record in
`status-log.md`, the finding #103 row (with its commit hash and live proof),
finding #98 marked CLOSED, and the CLAUDE.md status bullet.

41 rows: 34 PASS, one finding found and fixed on main (#103, `795ca3c5`), one
blocked on a pre-existing gap (`qt-image-gallery` has no v5 host), three
deferred, four left to the human. Two standing live-proof items discharged:
web search running off the configured `SERPER` key with no environment
variable, and the `[Title Update]` log lines in a real `combined.log`.

#### 2026-08-25 — fix(vault): a dropped component reference says so (dogfood finding #103)

_Versions: core 0.0.659._

The vault wardrobe reader drops two kinds of bad `componentItems:` entry — a
ref that matches nothing in the container, and a ref that would form a cycle.
Both drops are v4-faithful and stay. What was missing is that v4 logs a warning
at each one (`vault-overlay/parsers.ts:435` and `:453`) and v5 did neither, so
the drop was completely silent.

That silence is what makes the consequence invisible. The drop happens at read
time, and the next write to the container re-emits every sibling wardrobe file
from the read state — so a reference that went unresolvable is erased from disk
on the next unrelated write. Found by dogfooding: moving one component out of a
project took the parent outfit's reference to it from 7 to 6, and moving the
component back did not bring the reference back, with nothing logged at any
point.

`resolve_and_check_component_items` now takes `character_id` and
`mount_point_id` — which is exactly why v4's own signature takes them — and
emits v4's two messages verbatim with the same fields (snake_cased, per the
port's logging convention). Three tests in the capturing-layer idiom pin the
drop warning's fields, the cycle warning, and the silence leg on a healthy
vault; each is mutation-proven against a separate mutation.

#### 2026-08-25 — chore(unify): the f6a10055 wardrobe-containers drift round lands whole (P4.D112 ∥ P4.D113 ∥ P4.D114)

_Versions: quilltap-core 0.0.658, quilltap-harness 0.0.576, quilltap-web 0.0.87, SPA 0.5.556._

All three lanes of the `f6a10055` drift catch-up unified: the group
wardrobe CRUD + component-carrying transfers + the slug-collision vault
fix (server), the container-browser dialog with the pinned editor and the
transfer component prompts (SPA), and the gallery downloads + bug-98
create schema + blob `Content-Disposition` (both). The unification wires:
the §2 contract (five `groupWardrobe*` verbs, the widened
`wardrobeTransferApply`) folded into `core-contract.ts` with the casts
retired and the name-for-name wire diff clean against `api/types.rs`;
`P4D112_TRANSFER_COMPONENTS_LANDED` flipped live (the beat self-parks on
the committed fixture's missing General store — a recorded follow-up).
The §3 review found no blocking findings. The oracle baseline moves to
`f6a10055`; the unified gate's regens ran from a pinned worktree because
the v4 checkout's tree was dirty at unification. Gate numbers in the
round record (`status-log.md`).

#### 2026-08-25 — feat(groups): the group wardrobe gets its own CRUD verbs (P4.D112)

_Versions: quilltap-core 0.0.658, quilltap-harness 0.0.576._

Ports the server half of v4 `d7263f39`'s new group wardrobe API
(`/api/v1/groups/[id]/wardrobe[/itemId]` — the group tier previously had
list-only plumbing). Five dispatch verbs — `groupWardrobeList` / `Create` /
`Get` / `Update` / `Delete` — mirror the project-wardrobe five over the
shared mount-scoped writers (group and project items share the same
mount-folder storage). The collection verbs ensure the group's official
store and its `Wardrobe/` folder; the item verbs resolve the store only,
exactly as v4's routes split it. Error bodies match v4's: 404 `Group` /
404 `Group wardrobe item`, the flat 400 `Validation error` for schema
failures on create/update (Zod-faithful field checks incl. the slot-type
enum and UTF-16 lengths), and the component-cycle 400 with the writer's
own sentence. Delete clears equipped references warn-and-proceed first.
Served dispatch-only per the project-tier precedent — the v4 REST URLs get
no quilltap-web edge. A new `group_wardrobe_routes_equivalence` family
drives v4's REAL route files (jest real-DB oracle over the
wardrobe-transfers fixture pair, whose group store the cases reuse) across
15 cases with the seven-table remap diff — the error arms double as
nothing-was-written proofs — regenerated at the `f6a10055` pin and
runnable through the sweep driver.

_Versions: quilltap-core 0.0.657, quilltap-harness 0.0.575._

Ports the server half of v4 `d7263f39` + `f6a10055` on the transfers route.
The transfer body gains an explicit `source: {scope, id}` container (used
when the dialog browses a shared container directly — no character probing)
alongside the legacy `sourceCharacterId`, and `components: move|copy|none`
for composites: the transitive closure of the outfit's same-container
components travels with it (shared-tier pieces stay put), all-or-nothing,
with copy-minted ids rewritten into every travelling `componentItemIds`.
Any planned id already taken at the destination refuses the whole transfer
before anything is written, with v4's title-carrying sentence. Components
land before the outfit; a move deletes the travelling components and then
the item. A post-write verification reads the outfit back and reports
planned references that did not survive the round-trip
(`unresolvedComponentIds`); the response now always carries
`componentsTransferred`. The schema port adds both refine sentences and the
`source`/`components` tri-states (absent / null / value) with Zod-exact
messages. The transfers tier-2 family now drives the shared parse layer
with key-presence-faithful bodies over 18 scenarios (composite fixtures
seeded through v4's real repos; literal id compares where the UUID
normalizer would be blind; a `__destinations` GET row pins the roster) and
the wardrobe-routes web family gains five tri-state cases — all against
oracles regenerated at the `f6a10055` pin, with the read-back and
collision arms mutation-proven.

#### 2026-08-25 — fix(wardrobe): an ambiguous title slug is assigned to nobody (P4.D112)

_Versions: quilltap-core 0.0.656._

Ports v4 `f6a10055`'s `buildSlugByItemIdMap` fix. The vault wardrobe writer
used to hand a colliding title slug to the first item in write order while
the reader resolved slugs in filename order, so two same-titled items in one
container could silently rewire a composite's components on the next read.
`build_slug_by_item_id_map` is now two-pass: a slug borne by more than one
item is assigned to nobody and every reference to a collider is written as
the exact UUID. Unit tests mirror v4's new `wardrobe-slug-map.test.ts`; the
`vault-wardrobe-emit` corpus gains a differently-spelled-collision case and
the `vault-wardrobe-write` fixture a collision-returns op, both proven
red-first against the old code and green against oracles regenerated at the
`f6a10055` pin.
#### 2026-08-25 — test(workspace): follow the wardrobe selector's rename (P4.D113)

_Versions: SPA 0.5.555._

The workspace tab beat asserted the wardrobe body renders by looking for
`#wardrobe-char-select`. That control is now the container selector
`#wardrobe-container-select` — a one-line follow-on of the rename, caught by
the full Playwright suite.

#### 2026-08-25 — test(wardrobe): browser beats for the container browser and the component prompt (P4.D113)

_Versions: SPA 0.5.554._

Three beats in the wardrobe walk. The container selector beat browses Quilltap
General from the new top menu and proves the shared-container view: its own
contents only, the shared-wardrobe note, the right-hand outfit column standing
aside, and the editor pinned with a destination note instead of the "Add to"
scope selector. The composite beat builds an outfit and opens both transfer
dialogs: Move offers three component choices defaulting to move, Copy offers
two and never offers a move — v4 makes the illegal combination unreachable
rather than surfacing it as an error — and the item's own home is dropped from
the destination list.

The write half of the container beat is self-activating rather than skipped by
choice: the committed `characters-*` fixture pair has no `instance_settings`
table, so the boot ensure skips and the instance has no Quilltap General store
to write into. The beat probes the instance and switches the create/edit
round-trip on when a store exists. The components-actually-travel beat is
gated on P4.D112's server half.

#### 2026-08-25 — feat(wardrobe): the dialog browses and edits every wardrobe container (P4.D113)

_Versions: SPA 0.5.553._

The wardrobe dialog's top menu now lists every place a wardrobe item or outfit
can live — each character, Quilltap General, each project, and each group —
matching the Move/Copy destination roster. Browsing a shared container gives
its items the full kebab plus in-place creation; in a character's merged view,
items from other tiers stay Move/Copy-only as before. A shared container has
no character to dress, so the equip buttons and the right-hand outfit column
step aside.

The item editor is pinned to the browsed container: creates POST into it and
edits PUT back to it. This fixes a latent bug v5 shared with v4 — an edit to
any shared item targeted Quilltap General regardless of which store the item
actually lived in, so editing a project or group garment silently forked a
General copy. The "Add to" scope selector is replaced by a destination note,
the default-outfit helper text gains a group arm, and component candidates
become the container's own items plus the General archetypes.

The transfer dialog hides the item's known home from the destination list and,
for a composite outfit, prompts for its components: moving offers move / copy /
leave, copying offers copy / don't. Copy-plus-move is unreachable from the UI
and refused by the server besides. The explicit `source` container rides the
request when the home tier is known exactly, and `sourceCharacterId` is now an
absent key rather than a null when there is no selected character.

The wardrobe avatar preview download goes through the shared download util
instead of a hidden anchor click, matching v4's `af1bc479`.

#### 2026-08-25 — feat(wardrobe): a row's kebab follows the container it is being browsed in (P4.D113)

_Versions: SPA 0.5.552._

The wardrobe row's `isShared = !item.characterId` rule becomes an optional
`canManage` predicate. An item that lives in the container being browsed gets
the full kebab (Edit, star, Duplicate, Move, Copy, Delete); an item merged in
from a shared tier elsewhere keeps Move and Copy and wears the `· shared`
badge. Without a predicate the row falls back to v4's character-view rule
exactly, so the character view is unchanged. Nested composite components
inherit the predicate rather than re-deriving it from `characterId`.

#### 2026-08-25 — feat(wardrobe): every container reachable over its own verb, and Duplicate keeps the Portrait Cue (P4.D113)

_Versions: SPA 0.5.551._

v4's `wardrobeCollectionUrl` / `wardrobeItemUrl` build four families of REST
URL; v5 dispatches verbs, so the same four-way routing lands as
`containerListRequest` / `containerCreateRequest` / `containerUpdateRequest` /
`containerDeleteRequest` in `wardrobe.api.ts`. A container missing its id
throws by name rather than silently addressing the wrong tier.

`loadWardrobeContainerItems` ports v4's new `use-wardrobe-container-items`
hook: one shared container's own contents with no tier merging, plus the
General archetypes as a resolution pool so a composite row can still show
components it borrowed from General. A failed container read empties both
lists, as v4's catch does — a half-loaded container would offer edits into a
place we could not read.

`toggleItemDefault` and `deleteWardrobeItem` gain the browsed container as an
argument and pick v4's two arms from it; browsing a group, a shared item's
star now targets the group rather than Quilltap General.
`duplicateWardrobeItem` targets any container and carries `imagePrompt` — the
Portrait Cue used to be dropped on every duplicate, so a copied garment came
back describing nothing.

The five `groupWardrobe*` verbs and the two `wardrobeTransferApply` body
additions (`source`, `components`) are mirrored locally as a §2 block with a
dispatch cast, in the idiom §1 used before it was folded; lane P4.D112
delivers them in `api/types.rs` and the unifier folds them into
`core-contract.ts`.

#### 2026-08-25 — feat(wardrobe): the container module the dialog, editor, and transfer prompt will share (P4.D113)

_Versions: SPA 0.5.550._

A 1:1 transcription of v4's new `lib/wardrobe/wardrobe-container.ts`
(`d7263f39`): the four container scopes, `GENERAL_CONTAINER`, and the
encode / decode / same helpers the wardrobe dialog's new top selector, the
pinned item editor, and the transfer dialog all read. The scope spellings are
shared with the transfers wire (`source.scope`), so every arm — the unknown
scope, the non-general-needs-an-id rule, the `split(':', 2)` truncation — is
pinned by a parity spec rather than the happy path alone.

Recorded mechanism divergence: v4's module also exports
`wardrobeCollectionUrl` / `wardrobeItemUrl`, which build REST URLs. v5's
mutations ride dispatch verbs, so that routing lands in `wardrobe.api.ts` as a
verb router in a later unit of this lane; the types and the encoding stay
v4's verbatim.
#### 2026-08-25 — test(e2e): the download buttons produce real browser downloads (P4.D114)

_Versions: no crate versions bumped (the SPA rides un-bumped per the round's ownership split; the unifier recounts)._

Three live beats for v4 `af1bc479`. `photos-flow` gains two: the detail
modal's four-button footer row plus a Download click that yields a real
Playwright `download` event named after the stored file, and a photo with no
bytes behind it taking v4's failure arm (a live 404 from the blobs route → the
`Failed to download photo` toast, and the busy latch released). The
Scriptorium walk's existing upload beat gains a Download step between describe
and delete, asserting the stored name.

`seed-photos-fixture` now gives its first entry real bytes in `doc_mount_blobs`
(the table is created lazily on first write, so a store that never held a blob
has none) and deliberately leaves the second without, so the two arms are both
reachable. The stored MIME matches the stored `.webp` name on purpose: a
Content-Type that disagrees with the download name invites Chromium to rewrite
the extension, which would make the assertion about the browser rather than
about the port.

#### 2026-08-25 — fix(home): a failed New Project shows v4's sentence, not the server's (P4.D114)

_Versions: no crate versions bumped (the SPA rides un-bumped per the round's ownership split; the unifier recounts)._

The client half of v4 `c93ec7ff`. v4 gave `QuickActionsRow`'s own create
handler the two toasts its raw `fetch` never had, with the same sentences
Prospero's `useProjects` hook already used — so both v4 hosts now agree.

v5 reaches both hosts through ONE shared `qt-project-create-dialog`, which
already toasted `Project created successfully!`. Its failure arm surfaced the
transport's own message, which v4 never does: `useProjects` throws a fixed
`Failed to create project` without reading the response body. That mattered
more once bug 98's schema landed, since a refused create now answers
`Validation error` — a sentence a v4 user never sees. The dialog now shows
v4's fixed sentence in both hosts.

#### 2026-08-25 — feat(images): every gallery can download the picture on display (P4.D114)

_Versions: no crate versions bumped (the SPA rides un-bumped per the round's ownership split; the unifier recounts)._

Port of v4 `af1bc479`'s client half. My Photos' detail modal gains v4's
three-button footer row — Download (busy label `Downloading…`, error toast
`Failed to download photo`), Copy (`Image copied to clipboard` /
`Failed to copy image to clipboard`), Close. The Scriptorium file table's
detail row gains a Download button beside Open bytes, named after the STORED
file rather than the blob's `originalFileName`. The Generate Image page's
hand-rolled anchor click converges onto `core/download-utils`, gaining v4's
new `res.ok` guard on the way — a 404 body used to be saved as a file. The
(still unhosted) `qt-image-gallery` mirror gains its hover download in v4's
bottom-left corner.

New: `core/clipboard-utils.ts`, a transcription of v4's `lib/clipboard-utils`
browser arm (Clipboard API first, PNG conversion through an offscreen canvas,
loaded from a data URL because the CSP allows `data:` and not `blob:`). v4's
Electron IPC fallback has no v5 counterpart and is recorded as a non-goal in
the module header, the same class as `download-utils`' native save arm.

The specs drive the REAL utils and intercept the anchor click and the
`ClipboardItem` write, so the saved filename and the copied MIME are measured
at the browser boundary rather than at a mock.

#### 2026-08-25 — fix(projects): create validates exactly as v4's schema does (bug 98, P4.D114)

_Versions: no crate versions bumped (core + harness ride un-bumped per the round's ownership split; the unifier recounts)._

Port of v4 `c93ec7ff`. v4's `createProjectSchema` moved out of `route.ts` into
`app/api/v1/projects/schemas.ts` and its four presentational fields became
`.nullable().optional()`: the create dialogs send `description || null` for a
blank field, and the old plain `.optional()` refused the whole project over it.

v5's create was hand-rolled and validated only that `name` was non-blank, so it
never had bug 98 — but it also enforced none of the length, hex-colour,
UUID-array or type rules v4 has always enforced, and it REFUSED a
whitespace-only name that v4 accepts (`.min(1)` runs on the raw string; there
is no `.trim()` in this schema). It now runs a `PROJECT_CREATE_SCHEMA` table in
the `PROJECT_UPDATE_SCHEMA` idiom, and every refusal answers v4's flat
`Validation error` — including a non-object body, where v5 used to answer its
own invented sentence.

`projects_routes_equivalence` gains eighteen arms, one per row of the
old-vs-new measurement table taken from v4's real schema. Thirteen of them were
red before the port.

#### 2026-08-25 — feat(mount-points): the blob endpoint names the bytes it serves (P4.D114)

_Versions: web 0.0.87._

Port of v4 `af1bc479`'s server half. `GET /api/v1/mount-points/{id}/blobs/{*path}`
now sends `Content-Disposition: inline` with the STORED basename on both
response arms, through the existing `quilltap_core::content_disposition`
helper. The name is `relativePath.split('/').pop()`, not `originalFileName`:
images are transcoded to WebP on upload, so the original name's extension can
mismatch the bytes the endpoint actually serves. `originalFileName` (blob arm)
and the literal `document` (documents-fallback arm) are the falsy fallbacks v4
chains behind it.

`binary_routes` gains the pins: a nested blob path where the stored basename
and `originalFileName` disagree, a non-ASCII basename whose header was
compared byte-for-byte against v4's real `buildContentDisposition`, and the
documents-fallback arm. The routing-unreachable `'document'` default is pinned
as a unit test on the name helper instead.

#### 2026-08-25 — docs(porting): the f6a10055 wardrobe-containers drift round is planned (P4.D112 ∥ P4.D113 ∥ P4.D114)

Docs-only. v4 moved four commits past `0ba942b1` — the gallery-downloads +
bug-98 pair already flagged at the last unification, plus the two-commit
wardrobe-containers feature (a new group wardrobe CRUD API, component-
carrying transfers, the slug-collision vault fix v5 shares). Three work
orders committed under `docs/developer/porting/work-orders/` with a binding
server↔SPA contract and a disjoint-ownership table; phase-4.md gains the
planning note. Pin `f6a10055` for every regen until the round unifies.

#### 2026-08-25 — chore(unify): the no-drift maintenance round lands whole (P4.59 ∥ P4.60 ∥ P4.61)

_Versions: quilltap-core 0.0.655, quilltap-harness 0.0.574, quilltap-host 0.0.82, quilltap-web 0.0.86, SPA 0.5.549._

All three lanes of the 2026-08-24 round unified on `unify/p459-round` and
fast-forwarded to main. Dogfood #98 closes (the configured Serper search
provider end-to-end — registration behind v4's site-plugins gate, per-call
keys from `api_keys`, the providers listing's search row, the API-keys
modal's invented type filter removed); the wrong-type-collapse adjudication
completes (14 divergences fixed, 6 faithful verdicts recorded, the
executable census guard); the title-update handler's five portable log
lines land byte-faithfully (two proven NO-PORTs), and the `docs/v4/` mirror
is refreshed at the baseline. The §3 review found no blocking findings.
Gate: 13/13 families regenerated fresh from a worktree pinned at
`0ba942b1`, zero SKIP; 453 test binaries / 2,338 passed / 0 failed; clippy
both feature sets; release build; ng 341 files / 5,072; full Playwright
237/237 zero skips. ⚠ v4 drifted two commits mid-round (`af1bc479` +
`c93ec7ff`) — the catch-up is the next round's top candidate.

#### 2026-08-24 — refactor(brahma): the create-body Zod arm reads as what it is (P4.60 tidy)

_Versions: core 0.0.651._

`brahma_console_create`'s `connectionProfileId` arm was written with a guard
(`None | Some(Value::Null) if !matches!(…)`) that behaved correctly and read as
if it did not. Absent is the only case that falls back to the default, which is
now what the code says. The two Zod helpers take `&Value` rather than
`Option<&Value>`, since every caller had one in hand. Behavior-neutral; the
brahma routes differential is green over the pinned oracle either way.

#### 2026-08-24 — test(web): the collapse census gains the closure spelling, and the adjudication table lands (P4.60 unit 7)

_Versions: harness 0.0.569._

P4.57's survey needle was `and_then(Value::as_`, which cannot see the same
collapse written `and_then(|v| v.as_str())`. `web_edge_body_parse_guard` now
walks both, so P4.60's tier-2 enumeration is executable rather than a paragraph
— it is what found `files_routes.rs`'s five caller-input reads.

The full adjudication table (every enumerated key, its v4 route and schema, and
its verdict) is in the lane record: fourteen DIVERGENT-FIXED, six FAITHFUL, one
CONFIRMED, and no deliberate divergences.

#### 2026-08-24 — fix(qtap): the import legs' exportData guard is JS falsiness, and a non-JSON body is v4's 500 (P4.60 unit 6)

_Versions: harness 0.0.568, web 0.0.85._

The order listed the `.qtap` import legs as confirm-only. The `data_key_absent`
comment does still describe the code — but measuring the neighbouring guards
against v4's real route rather than reading them found two divergences.

v4's `if (!body.exportData)` is JS falsiness, so `0`, `''` and `false` are
missing exactly as `null` is; v5 excluded only `null`, so a falsy body fell
through to the manifest check and answered the wrong sentence. And v4's
`await req.json()` sits inside the handler's `try`, so a malformed body's
rejection escapes to the outer catch as a **500** with the leg's own sentence —
as does a body that is literally `null`, via the TypeError from
`null.exportData`. A body that is `42` or `"nope"` does not, because JS reads a
missing property off a scalar as `undefined`.

New `qtap_import_guards_equivalence` (24 arms, real HTTP against a served
instance) pins all of it. `files_write_routes`' 101 MB ceiling test now asserts
that 500 — the status was never its point, but it is asserted so a future change
has to be deliberate.

#### 2026-08-24 — fix(embedding-profiles): the reindex scope keeps its absent/null split and v4's String() coercion (P4.60 unit 5)

_Versions: core 0.0.650, harness 0.0.567, web 0.0.84._

The reindex edge coerced a non-string `scope` with `Value::to_string()`, which
is the JSON text; v4 interpolates `String(body.scope)`, so an object reads
`[object Object]` and an array reads its comma-joined elements. And v4's guard
is `body.scope !== undefined`, a distinction the edge's `Option<String>` could
not carry — an explicit `null` must reach the refusal while an absent key
defaults to `all`.

`scope` now rides the verb raw, and the handler uses `to_js_string`.
`embedding_profiles_routes_equivalence` gains seven arms (37 → 44), among them
`scope: ["mismatched-dim"]` — which v4 refuses with a sentence naming a valid
scope, because the comparison is against the array. The family also gained the
consumed-case guard it lacked.

#### 2026-08-24 — fix(restore): the uploadId and mode guards run in v4's order, on the raw values (P4.60 unit 4)

_Versions: core 0.0.649, harness 0.0.566, web 0.0.83._

`POST /api/v1/system/restore` reads `uploadId` and `mode` with no Zod at all —
v4 destructures and guards them by hand, so `if (!uploadId)` is JS falsiness and
a truthy wrong-typed value passes it and reaches `UUID_REGEX.test(uploadId)`,
which `String()`-coerces. The two failures answer different sentences;
`and_then(Value::as_str)` collapsed both into `uploadId is required`.

The three fields now ride the verbs raw and the core arms guard them in v4's
measured order — uploadId, then mode, then the upload lookup. That order used to
differ between the two entrances: the REST edge checked `uploadId` first, the
dispatch arm checked `mode` first.

New `system_restore_guards_equivalence` (17 arms) drives v4's real route
handlers. It needs no provisioned instance on either side, because every arm
stops inside the guards. `compact` and `keepArchivedCharacterBundles` are
recorded FAITHFUL — both of v4's checks are strict comparisons against a
literal, which no collapse can change.

#### 2026-08-24 — fix(brahma): the console bodies are validated in v4's order, after the 404 gate (P4.60 unit 3)

_Versions: core 0.0.648, harness 0.0.565, web 0.0.82._

The Brahma Console edge read `content`, `fileIds`, `title` and
`connectionProfileId` with `and_then(Value::as_str)` / `as_array` and answered
its own sentences. v4 parses each schema **uncaught**, so a wrong-typed value is
the flat 400 `Validation error`, a non-uuid `fileIds` entry is refused rather
than emptied, and — for send, rename and set-model — the parse runs AFTER
`verifyBrahmaChat`, so a bad body on a chat that is not a Brahma console is a
404.

The four body fields now ride the dispatch verbs as raw JSON values (the
`recall-replay` precedent) and the core arms validate them in v4's order;
`brahma_send_prepare` keeps the gate and the schema together so they cannot
drift apart. The SPA is unaffected — it omits absent keys and sends non-empty
strings — so no client change was needed.

`brahma_console_routes_equivalence` gains seventeen arms (17 → 34), including
three that carry a body which would ALSO have failed, on a chat that 404s. The
family also gains the case-count guard it lacked, so an oracle case forgotten on
the Rust side can no longer pass silently.

#### 2026-08-24 — fix(characters): the photos JSON body is parsed against v4's saveByIdSchema (P4.60 unit 2)

_Versions: core 0.0.647, harness 0.0.564, web 0.0.81._

`POST /api/v1/characters/{id}/photos` with a JSON body read `fileId`, `linkId`,
`caption` and `tags` with `and_then(Value::as_str)` / `as_array`. A wrong-typed
`caption: 5` or `tags: "airship"` was silently dropped and the photo saved with
a 201; v4 `safeParse`s and answers 400 with the joined issue sentences.

`api::characters::parse_photo_save_by_id_body` ports the schema, including the
refine's measured quirk: an `invalid_type` issue anywhere — one bad element of
`tags` included — suppresses the refinement, while a `too_small` issue does not,
so `{fileId: ''}` answers two sentences and `{fileId: null}` answers one.

`characters_mutations_equivalence` gains eleven arms driving v4's real photos
route, each diffing the joined sentence AND the photos/ link dump, so a refusal
proves it wrote nothing.

#### 2026-08-24 — fix(custom-tools): the run body is parsed against v4's runSchema, not collapsed key-by-key (P4.60 unit 1)

_Versions: core 0.0.646, harness 0.0.563, web 0.0.80._

`POST /api/v1/chats/{id}/custom-tools?action=run` read its four body keys with
`and_then(Value::as_str)` / `as_bool` / `as_object`, which turned a
present-but-wrong-typed value into "the caller didn't say". v4 calls
`runSchema.parse` uncaught, so `tool: 123`, `parameters: "nope"`,
`private: null` and `asCharacterId: 42` are all a flat 400 `Validation error`
there; v5 ran the tool.

The schema is now ported as `quilltap_core::api::custom_tools::parse_run_body`,
which the edge calls. A second divergence the collapse had hidden: v4 reads
`asCharacterId` through a truthiness gate at all four of its sites, so an empty
string means "nobody named" — v5 answered `No character participant with id  is
in this chat`, and on the effect path would have written to a character's sheet
where v4 writes to nobody's. `chat_custom_tool_run` normalizes it, so the
dispatch entrance agrees with the edge.

`pascal_custom_tools_route_equivalence` gains twelve arms (24 → 36) driving the
real parser, and its POST leg now goes through it rather than re-reading the
keys itself. New `web_edge_body_parse_guard` holds the whole
`quilltap-web/src/*_routes.rs` surface to a per-file census of the collapsing
idiom and pins that the fixed edge still routes through its parser.
#### 2026-08-24 — fix(spa): the API-keys surface offers the search provider (dogfood #98)

_Versions: SPA 0.5.549._

v4's Add-New-API-Key modal filters the provider list on
`providerAcceptsApiKey(p.configRequirements)` and nothing else. v5 had added
`p.type === 'llm'`, which — once the search row exists — makes a Serper key
uncreatable and so leaves the configured search path unreachable from the UI.
That is dogfood #98's remaining half. The filter is now v4's, and both
directions are spec-pinned: the search provider IS offered (sorted in by display
name, as in v4), and a keyless provider is still excluded, so the filter was not
simply deleted.

`ProviderInfo.capabilities` is optional, because the search row carries no
capability bag at all — and that absence is load-bearing: it is how v4's own
profile editor keeps search providers out of the LLM picker
(`p.capabilities?.chat`). Two specs pin that direction, one of them proving the
filter is the capability rather than the type by excluding a non-chat LLM
provider. Three call sites that read the bag directly were made optional-safe.

The e2e beats become the registration proof over the real binary. The settings
walk asserts the Serper option and creates a Serper key; the salon web-search
beat now runs the CONFIGURED path — `SERPER_API_KEY` is no longer set at launch,
global setup seeds the `api_keys` row instead, so a green run proves the row is
what reached the wire.

#### 2026-08-24 — test(search): the tier-3 web-search oracle drives v4's REAL registry, plugin and key predicate

_Versions: harness 0.0.566._

The `web_search_tool` oracle used to mock `searchProviderRegistry` with a
hand-built object whose `executeSearch` returned canned output, so on the
provider path v4's own plugin — its request, its error sentences, its formatter
— was never in the loop. Now the oracle initializes the REAL registry the way
v4's boot does, with the REAL built `qtap-plugin-search-serper` bundle, over a
mocked `fetch`. Only the repository read is mocked, and it answers with a
realistic multi-row list so v4's own
`find(k => k.provider === 'SERPER' && k.isActive)` decides.

This side goes through the production `DbSearchApiKeys` over a real provisioned
instance whose `api_keys` rows are written by the real repository — one
instance, one user per row-set, so the read is user-scoped as production's is.
New arms: an inactive-only row, another provider's row, an inactive row skipped
for a later active one, two active rows where the first wins, the knowledge-graph
unshift through the plugin's own mapping, the plugin's network-error catch, and
the two precedence arms — a registered provider short-circuits the env fallback
(the tell is the plugin's 401 sentence, not the fallback's), and a registered
but keyless provider refuses rather than falling back. 17 cases to 26.

**One arm was vacuous on its first pass and a mutation proved it.** Which key
the lookup chose is invisible in the tool's output — it travels as a request
header, and the canned transport's key is method + url + body — so a mutation
taking the LAST active row passed green. Both sides now echo the `X-API-KEY`
header into the result title, and that mutation reds. Also mutation-proven:
`serper_registered` forced false; `isActive` dropped from the predicate; the
registered path preferring the env key.

The registry keeps its state on `globalThis` and survives `jest.resetModules()`,
so each case deletes that key first — without it a case that must see an empty
registry inherits the previous case's provider.

#### 2026-08-24 — fix(search): the registered arm's real wire — the plugin's User-Agent and its `validateApiKey` probe

_Versions: core 0.0.648, harness 0.0.565, host 0.0.82._

Two wire facts that only matter now that the plugin path is live, and that the
differential could not see while it was dark.

The Serper plugin sends `User-Agent: getQuilltapUserAgent()`; v4's legacy
env-var fallback, built by hand in the main-app handler, sends no such header.
`build_serper_request` takes the user agent as an argument so the byte follows
the arm rather than a global default, and `RealWebSearchProvider` carries the
host's `Quilltap/<version>`. The recorder now captures request headers, so
`web_search_wire_equivalence` compares them (names folded, the version-bearing
UA and the key folded to placeholders — the `provider_header_common`
precedent). A unit test pins the WIRING the differential cannot see: which arm
asks for the header, proven with a recording transport.

The plugin's second fetch site — `validateApiKey`, a fixed `{q: 'test', num: 1}`
POST answering `response.ok` — is reachable from the API-keys screen's Test
button through v4's `searchProviderRegistry.validateApiKey`, and reachable in
v5 now that a Serper key can exist. `WireConnectionValidator` gains the SERPER
arm; without it the catch-all answered a silent `{valid: false}` for every
Serper key. Five recorded validate rows drive v5's real validator over a canned
transport.

Mutation-proven: dropping the UA push reds the search headers; disabling the
SERPER arm reds `validate_ok`; a `num: 5` probe body reds `validate_ok body`;
sending the UA on both arms reds the wiring pin.

#### 2026-08-24 — feat(search): registration goes live — the providers listing gains v4's `type: 'search'` row

_Versions: core 0.0.647, harness 0.0.564, host 0.0.81, web 0.0.80._

The host now computes v4's registration once, at spine build, from the
site-plugins gate, and threads the SAME answer into both consumers: the
`search_web` runner's `serper_registered` flag and the new
`EngineAssembly.search_providers` the providers listing serves. v4's
`isWebSearchConfigured()` is `registry.isSearchConfigured() || SERPER_API_KEY`,
and both terms are now live — the provider is built when either holds, so
`web_search.is_some()` is that `||` term for term. `DbSearchApiKeys`, wired
inert since P4.42, is load-bearing from here: with the provider registered the
per-call key comes from the user's `api_keys` row, and a missing row surfaces
as v4's `No API key configured for Serper Web Search…` sentence instead of a
silent refusal.

`provider_list` gains the search row in v4's spread position (after the ten LLM
rows) with its materially different shape: no `capabilities` — which is exactly
how v4's own profile editor keeps it out of the LLM picker (`p.capabilities?.chat`
on an absent bag) — no `optionsSchema`, no `thinkingTurnRule`, and a hand-built
three-key `configRequirements`. The "no search-provider manifest is ported"
comment retires.

`providers_listing_equivalence` grew the row, red-first (oracle 11 vs got 10),
and gained a whole-row byte compare with key ORDER included, since `Value`
equality under `preserve_order` catches a wrong key but is blind to a wrong
position — and the search row's whole value is which keys it omits and where
its three-key bag sits. Mutation-proven: dropping the append reds the count;
moving `apiKeyLabel` before `requiresBaseUrl` reds the SERPER row bytes.

**That new assert immediately caught a harness bug of its own.** The family's
`normalize` dropped `icon` with `Map::remove`, which under `preserve_order` is
IndexMap's SWAP-remove — it moved `thinkingTurnRule` from last into `icon`'s
slot and manufactured a key-order difference in every LLM row. Now
`shift_remove`. Nothing shipped was wrong; the old order-independent compare
simply could not see it.

#### 2026-08-24 — feat(search): the native Serper search-provider manifest + v4's site-plugins gate

_Versions: core 0.0.646, harness 0.0.563._

The first half of dogfood finding #98. v4 registers exactly one search
provider — the bundled `qtap-plugin-search-serper` dist plugin,
`enabledByDefault: true` — and everything the host reads off that registry is
data: the plugin's `metadata` and its three-key `config`. So the search half
takes the shape the LLM half took in W4.7a: a generated declarative manifest
(`provider_manifest/search.rs` + `manifests/search_serper.json`) plus the
already-compiled implementation in `tools::web_search`. No plugin runtime is
introduced; that stays deferred.

Registration is gated exactly as v4 gates it. v4's manifest loader drops a
plugin whose name fails `isSitePluginEnabled` before `enabledByDefault` is
even read, so `SITE_PLUGINS_DISABLED=qtap-plugin-search-serper` yields an
instance with no search provider. That predicate is ported whole as
`is_site_plugin_enabled`, pure with both env values injected (the core reads
no environment), and pinned by the new `site_plugins_equivalence` differential
over v4's real `lib/plugins/site-plugins.ts` — 19 cases covering unset, empty,
whitespace-only, `all` in three casings, comma lists with stray spaces and
empty segments, and the disabled-wins overlap. Mutation-proven: dropping the
case-insensitive `all` reds `enabled_all_upper`; dropping the disabled check
reds `disabled_serper`.

Recorded divergence: v5's ten LLM providers are native and are NOT gated by
`SITE_PLUGINS_*` — they have no plugin name to gate on, and the loader that
consults the gate is part of the un-ported plugin runtime. Only the Serper arm
of the gate is faithful, which is the arm an operator can exercise today.

The manifest generator learned the search shape in the same commit (the
standing rule for this file); all ten LLM manifests regenerated byte-identical.
#### 2026-08-24 — fix(title-update): the handler says what it did (P4.61)

_Versions: core 0.0.646, harness 0.0.563._

v5's `TITLE_UPDATE` handler carried one of v4's eight log lines. Five of the
remaining seven now land byte-faithfully, so `combined.log` shows a failed
cheap-LLM call, the decision to rename, the written title, and both
story-background queue outcomes: `[Title Update] Failed for chat <id>: <error>`,
`[Title Update] Chat <id> - needsNewTitle: true, reason: <reason>`,
`[Title Update] Updated title for chat <id> to: "<title>"`,
`[Title Update] Queued story background generation`, and
`[Title Update] Failed to queue story background generation`. The last two live
in `image_profile_resolution.rs`, where v5 split v4's
`queueStoryBackgroundIfEnabled` — the enqueue result is no longer discarded, so
v4's `isNew` gate and its catch arm both have somewhere to land.

The other two v4 sites are NO-PORTs with evidence, recorded in the source: `:89`
(`No cheap LLM available`) sits behind `if (!cheapLLMSelection)`, and
`getCheapLLMProvider` is typed non-nullable with no `return null` on any path;
`:185` (`Failed to create system event:`) sits in a `catch` whose two callees
each wrap their whole body and resolve rather than reject.

A log line writes no row, so the differential cannot see any of this. Each line
is pinned instead with the capturing tracing layer P4.D110 established, over the
real `handle_title_update` on the committed fixture, asserting both presence on
its own branch and silence on the siblings — including v4's `isNew` dedupe arm
(a second run must stay quiet) and the enqueue's failure arm (the fixture copy
loses its `background_jobs` table so the enqueue really errors). Six mutation
proofs, one per line plus the escaped-gate spelling, each reddening only its own
arm. `title_update_tier3` regenerated fresh at `0ba942b1`: 17/17 green, so the
lane moved no state.

#### 2026-08-24 — docs(v4): refresh the reference mirror at `0ba942b1` (P4.61)

_Docs-only change._

`docs/v4/` had drifted from the v4 checkout it mirrors. Refreshed mechanically
by rsync from `~/source/quilltap-server/docs/` at the round's baseline
(`.DS_Store` excluded; no `--delete`, so the mirror's selective `docs/v4/help/`
survives), plus `docs/v4/help/database-protection.md` from v4's `help/`.

19 files modified, 97 added. The work order's "~8 differing files" estimate was
stale in both directions: `developer/API.md` was already byte-identical, while
18 other files had drifted and 83 `bugs/fixed/` rows, four releases
(4.8.1–4.8.4), both `CUSTOM_TOOL_SPEC*.json` files, `bugfix-sessions/`, and
seven feature docs were missing entirely. `diff -rq` is clean afterwards. The
mirror is reference-only — no v5 prose was edited to match it.

#### 2026-08-24 — docs(porting): the no-drift maintenance round ordered (P4.59 ∥ P4.60 ∥ P4.61)

_Docs-only; no version bumps._

Three work orders for the first zero-drift round in weeks, all from the
banked queues: P4.59 (dogfood #98 — the configured-path Serper search
provider: `serper_registered` real, keys from `api_keys`, the providers
listing's `type: 'search'` entry, the SPA API-keys surface), P4.60 (the
P4.57-banked wrong-type-collapse edge adjudication across eleven
enumerated route sites), and P4.61 (the seven missing `[Title Update]`
log lines + the stale `docs/v4/` mirror refresh). The baseline stays
`0ba942b1`; the lanes meet nowhere. `p4.9i2` (help/HelpChat) is named the
recommended next dedicated round.

#### 2026-08-24 — fix(almanack): the report measures free memory instead of reporting a hardcoded zero (dogfood #94)

_Versions: host 0.0.80._

`almanack_services.rs` hardcoded `free_memory_bytes: 0.0`, so the Almanack's
System Information read `Free Memory: 0 B` on a 48 GB machine — which reads as
"this box is out of memory", not as "not measured". The module header argued
the zero on the grounds that no dependency-free portable read exists. That
premise was false by the file's own technique, which already shells out to
`sysctl hw.memsize` and parses `/proc/meminfo`.

`free_memory_bytes()` now mirrors `total_memory_bytes()`'s two `cfg` arms. The
macOS arm turns on a detail worth stating: free memory is `Pages free` **plus**
`Pages speculative`, times the page size `vm_stat` states in its own header.
`vm_stat` prints `free_count - speculative_count` on its "Pages free" line and
reports the speculative pages separately, while libuv's `uv_get_free_memory`
reads the raw `free_count` — so the two lines have to be added back together to
match what v4's `os.freemem()` reports. Measured against Node four times on the
same host: `free` alone is off by roughly 236 MB, `free + speculative` tracks
`os.freemem()` to within the drift between two samples. The Linux arm reads
`MemFree`, the key libuv reads before falling back to `sysinfo()`, through a
parser now shared with `MemTotal` so the two cannot drift apart in parsing. A
platform that genuinely cannot be read still answers zero.

The renderer is untouched: it is byte-pinned against v4's, which always emits a
number, so computing the value keeps the line both faithful and true.

Five tests. The parsers are pinned against real captured output rather than a
hand-written shape, the page size is proven read rather than assumed, the
unparseable arms still fall back to the honest zero, and a live arm asserts
`0 < free <= total`. The fifth reads the value back through `runtime_facts()`,
and exists because the mutation pass caught its absence: reverting the struct
literal to `0.0` left every other test green, since they all called the
function directly.

#### 2026-08-24 — docs(dogfood): the vision-round pass — 16 PASS, no v5 defects, eight live proofs discharged

_Docs-only change._

The `a14a1811` vision round and the `0ba942b1` drift round met real data for
the first time: 19 rows, 16 PASS, one BLOCKED with its premise corrected, three
deferred. No v5 defects, and zero panics in roughly two hours against the real
800 MB instance.

The `describe_image` vision tier ran in production for the first time — all
three tiers proven on real images, with the `vision-call` arm a genuine
6,996 ms GROK call whose description persisted onto the file row, then
re-proven free as `stored-description`. The NanoGPT vision send is proven both
on the wire (an `image_url` content part carrying a 3,000-character `data:`
URL) and in the answer (`zai-org/glm-4.6v` reading a purpose-drawn PNG
exactly). Also live: the bug-91 describe-fallback on a non-transporting Ollama
seat, the bug-97 OpenRouter convergence, the attachment-failure toast, bug 93's
moderation sentence in both arms, and bug 96's near-miss title key.

The last two were driven by purpose-written provider stubs — one that answers
with an empty stream carrying a chosen `finish_reason`, one that returns a
canned verdict under a misspelled key — so the refusal path was exercised
without composing anything a provider would have to refuse. Those stubs and a
structural wire tap that walks message content parts (the existing
`wire-tap.py` collapses `messages` to a count) are promoted to
`harness/tools/`.

Two rows recorded, neither a v5 defect: NanoGPT prompt caching writes a cache
every turn and never reads one though the system blocks are byte-identical
(raised for the human as a cost question — the wire itself is correct and
identical to v4's), and a plain regenerate re-sends no attachments because v4
does not either, which corrects the setup the whisper-tailed-regenerate item
was written against.

#### 2026-08-23 — unify: the `0ba942b1` drift round (P4.D110 ∥ P4.D111 ∥ P4.58)

_Versions: core 0.0.645, harness 0.0.562._

All three lanes unified same-day; the oracle baseline moves to `0ba942b1`
and the drift debt is cleared (bugs 96 + 97 absorbed whole). The
title-verdict parser lands with the checkpoint-burned warn (red-first,
10 → 17 tier-3 cases, six mutation proofs); the bug-97 OpenRouter vision
convergence retires every pin to plain equalities (red-first per family,
nine sibling manifests byte-identical); the photo-tools and settings-routes
corpus blind spots close with zero v5 source change and nine mutation
proofs. The §3 review read the whole combined diff against v4's real code:
**no blocking findings** (the recorded divergences are cosmetic tracing
field-name conventions). The one wire was the version recount — two lanes
bumped core off the same base and the auto-merge kept one, the playbook's
standing trap. Gate: 7/7 pinned sweep zero SKIP with changed bytes grepped
per family; 449 test binaries / 2,320 / 0; clippy both feature sets;
release build; ng 341 files / 5,068; full Playwright 237/237 zero skips.
Banked: v5's title-update handler carries 1 of v4's 8 log lines — a small
handler-logging order.

#### 2026-08-23 — fix(titles): a misspelled key stops silencing the auto-titler (bug 96)

_Versions: core 0.0.644, harness 0.0.560._

Ports v4 `3c041e46` whole. A cheap model answered `needsNewTitle: true` with the
title under `suggestTitle` — two letters short of the `suggestedTitle` the prompt
asks for. Reading the canonical key alone yielded nothing, the handler read that
as a decline, the checkpoint cursor advanced, and no story background ever
generated (it queues only off a successful rename).

The parser moves into a new `services::context_summary::title_verdict` module —
v5 already shared one parse site where v4 carried two copies, so this is the body
change plus a per-site task label. It reads the canonical key first, then a short
near-miss list (`suggestTitle`, `newTitle`, `proposedTitle`, `title`), then a
case- and separator-folding second pass that catches `suggested_title` and
friends; the canonical key wins when a model emits several, an explicit `null`
falls through instead of stopping the walk, and the normalizer trims a second
time after unwrapping the quotes. The reason field now requires a string whose
trim is truthy. Four warn arms carry v4's exact sentences and `context` values
into `combined.log`: response was not JSON, response JSON was not an object,
title arrived under a non-canonical key, and a rename requested with no usable
title. The job handler warns `checkpoint burned` on that last case.

Cursor advancement is unchanged: all three no-rename outcomes still advance, as
v4's shipped fix leaves them. The fix shrinks the unreadable set and makes the
residue loud.

The `title_update_tier3` differential grew seven cases driving the recovery
through the real handler, so a recovered title is measured as a write — the
renamed chat row and the story-background job's scene context. Five were red
against the pre-fix parser. The `checkpoint burned` warn writes no row, so it is
pinned by a capturing-subscriber wiring test over the real handler.
#### 2026-08-23 — fix(images): OpenRouter transports images again, and the describer guard names NanoGPT (v4 bug 97)

_Versions: core 0.0.644._

The convergence half of this port's own upstream filing. P4.D106 measured a
contradiction inside v4 and reproduced it faithfully: OpenRouter's plugin
registry entry declared `supportsAttachments: false` while the client-safe
static map listed its four image types, so v4 production — which has the
registry up — answered false for OPENROUTER, routed every OpenRouter vision
profile to the describe-fallback, and refused an OpenRouter describer in the
same sentence that recommended OpenRouter. Filed as v4 bug 97; fixed upstream
at `0ba942b1`, where plugin 1.0.59 declares `supportsAttachments: true` and
imports its MIME list from the provider module that does the sending.

v5 converges. The ten provider manifests were regenerated from the pinned v4
worktree: only `openrouter.json` moved (`supportsAttachments` true, the four
image MIME types, v4's new description and notes bytes), the other nine came
back byte-identical. `provider_can_transport_images` therefore answers true for
OPENROUTER, and its unit test moves that provider into the transporting set.
Both stale narratives — the `image_transport` module header's "a v4 bug to file
upstream" and the `attachment_support` header's OpenRouter example — are
rewritten as converged notes.

Two riders. The describe-fallback's transport-guard sentence gains NanoGPT
between OpenRouter and Z.AI, byte-exact to v4's new literal, with v4's
keep-in-step comment carried over; NanoGPT has transported images since plugin
1.1.0, and the omission pushed operators away from a working choice. And the
`moderation_finish_reason` docblock's note about v4's "(bug 94)" mis-numbering
is retired — v4 corrected its own docblock at `7a6716b5`, so the note was now
wrong about v4.

Proven red-first, per family, over oracles regenerated fresh at the pin:
`image_transport_equivalence` red on the OPENROUTER `full_init` row,
`provider_registry_equivalence` red on the `attachmentSupport` block bytes,
`file_attachment_tier3_equivalence` red on `fb_ollama_describer_guard`'s
sentence — then all three green with no oracle re-run.
#### 2026-08-23 — docs(dogfood): finding #95 closes — P4.D93 absorbed v4 bug 82

_Docs-only change._

P4.58 item 5. Row #95's status cell still read "FILED as v4 bug 82 … this row
retires when the port absorbs the fix". v4 fixed it at `9125f492` and the P4.D93
lane absorbed exactly that fix on 2026-08-19 (the leading-system-message fold,
landed in the Ollama and OpenAI-Compatible builders only so hosted requests stay
byte-identical; the request-envelope corpus went 257 → 263 with every older row
unchanged). The cell now says so.

#### 2026-08-23 — test(settings): the supportsImageUpload create-time seed default reaches the oracle

_Versions: harness 0.0.561._

P4.58 item 3. The a14a1811 §C1 static-map rows flipped NANOGPT's and Z_AI's
`PROVIDER_ATTACHMENT_CAPABILITIES` entries from `[]` to the four image types,
which changes what a connection-profile CREATE stores for an OMITTED
`supportsImageUpload`: false becomes true. `settings_routes_equivalence` stayed
green through that flip because its corpus carried no NANOGPT or Z_AI create at
all, let alone one omitting the field — the flip was pinned only by the map
rows' own unit tests.

Four new create rows, each with `after: 'connProfiles'` so the persisted value
is diffed as well as the response: NANOGPT and Z_AI with the field omitted
(both resolve TRUE), DEEPSEEK with the field omitted as the contrast (it stayed
`[]`, so FALSE), and NANOGPT with an explicit `false` (the client-sent boolean
wins over the map). The `connection_profiles >= 22` stale-oracle floor moves to
26; the family runs 141 → 145 cases.

One nuance in the order was measured and corrected: v4's route calls
`providerSupportsMimeType`, which is `supportsMimeType` from
`lib/llm/attachment-support.ts` — the CLIENT-SAFE hardcoded map, whose `baseUrl`
parameter is accepted and never read. It is not the registry-aware
`providerCanTransportImages`, so jest's uninitialized provider registry cannot
change the answer and these rows pin exactly the table v5's
`files::attachment_support::supports_mime_type` reads. The case comment says so.

Mutation-proven red-first, each reverted from a file backup: emptying the
NANOGPT map row reds `cp_create_image_default_nanogpt`; emptying only Z_AI's
reds `cp_create_image_default_zai`; giving DEEPSEEK the image types reds
`cp_create_image_default_deepseek` (with the three pre-existing DEEPSEEK creates
temporarily skipped, since they cover the false direction incidentally and the
loop aborts at the first mismatch); and making the create ignore the client
boolean reds `cp_create_image_explicit_false_nanogpt`.

#### 2026-08-23 — test(photo-tools): the NULL-dimension and whitespace-description arms reach the oracle

_Versions: harness 0.0.560._

P4.58 item 1+2. The photo-tools differential gained five ops and the fixture
spec gained two images, closing the two corpus blind spots the P4.D108 lane
recorded. No v5 source changed.

The **key-omission arm**: v4 builds the `describe_image` success row and the
`attach_image` descriptor with `width: entry.width ?? undefined`, so an absent
dimension OMITS the key under `JSON.stringify`. The corpus could not reach that
— its only candidate row carried a PRESENT `width: 0`, and `??` keeps a present
zero. The fixture spec now makes `width`/`height` optional (both together, or
neither — the builder refuses a half-specified pair) and the builder patches
them only when the spec names them, so `ingestImageBuffer`'s NULL dimensions
survive (sharp cannot read the corpus's synthetic bytes). A new `dimensionless`
image, baked into vault A, drives both sites: `describe_dimensionless` and
`attach_dimensionless`. Both compare EXACTLY (no UUID normalization), so key
presence is a live comparand.

The **truthiness-then-trim quirk**, pinned at both ends. v4's already-described
precheck and the describe tier-1 gate both read `description && trim().length >
0`, so a whitespace-only stored value is truthy but must not satisfy either: the
new `blankdesc` image carries `" \t \n  "` and drives `describe_whitespace_stored`
(falls through to the generation-prompt tier) and `autodescribe_whitespace_stored`
(the module proceeds and overwrites). At the describer end, the new `whitespace`
module mock answers a whitespace-only vision result — truthy, so it passes v4's
`!result.imageDescription` guard, and the subsequent `.trim()` then persists an
EMPTY string: `autodescribe_whitespace_result` diffs `files.description = ''`,
the blank link's `description`/`extractedText` = `''` with the empty-string
sha256, and zero chunks. Both apps agree.

Mutation-proven red-first (each reverted from a file backup): unconditionally
inserting `width`/`height` in `describe_respond` reds `describe_dimensionless`;
dropping the descriptor's `skip_serializing_if` reds `attach_dimensionless`;
dropping the trim from the describe tier-1 gate reds
`describe_whitespace_stored`; dropping it from the already-described precheck
reds `autodescribe_whitespace_stored`; and making the vision-result guard
trim-based reds `autodescribe_whitespace_result`.

The order's suggested fixture-side mutation (make the builder always insert the
keys) was measured and rejected: it changes BOTH sides through the same
regeneration and stays green, so it proves nothing.

#### 2026-08-23 — docs(porting): the `0ba942b1` drift-round work orders (P4.D110 ∥ P4.D111 ∥ P4.58)

_Docs-only; no version bumps._

The next round planned against a fresh drift check: v4 main is THREE commits
past the `a14a1811` baseline — `3c041e46` (bug 96, the auto-titler's
misspelled-key silence: a behavior change on a ported surface), `7a6716b5`
(this port's own bug-97 filing; docs + a comment-only lib line), and the new
`0ba942b1` (v4 fixing bug 97 — the pre-announced convergence). The `bugfix`
branch gained only the tests-only `009c49b2` — nothing to port. Three
work orders committed, fully disjoint, all pinned to the new `0ba942b1`
round baseline:

- **P4.D110** (`p4.d110-title-verdict.md`) — the title-verdict parser
  (near-miss keys + fold pass + double-trim + four byte-exact warn arms) and
  the handler's checkpoint-burned warn; v5 has ONE parse site already, so
  the port is the body upgrade, the 16-case unit mirror, and the
  `title_update_tier3` family extended red-first. The survey settled a
  commit-prose trap: v4's shipped fix does NOT change cursor advancement.
- **P4.D111** (`p4.d111-bug97-convergence.md`) — the ten-site convergence
  checklist: the OpenRouter manifest regen (mechanical — attachment fields
  are not augmented), the `image_transport.rs` assert flip + two stale
  header narratives, the guard sentence's `NanoGPT, ` entry, the
  `moderation_finish_reason.rs` mis-number note retired, three families
  red-first at the pin, the help paragraph banked to `p4.9i2`. The SPA
  attachment table needs NO change (it was correct throughout).
- **P4.58** (`p4.58-corpus-maintenance-smalls.md`) — the a14a1811 round's
  recorded corpus blind spots: photo-tools width/height-NULL omission +
  whitespace-only-description arms (spec/builder widening + fixture rebuild
  + mirrored ops), the settings-routes NANOGPT/Z_AI
  create-with-omitted-`supportsImageUpload` rows + DEEPSEEK contrast,
  mutation-proven; plus the stale dogfood #95 status cell. Zero expected v5
  source change.

#### 2026-08-23 — docs(porting): the round's two upstream findings filed as v4 bug 97

_Docs-only; no version bumps._

The a14a1811-round record's two "TO FILE UPSTREAM" items are discharged,
filed directly into v4's bug record (v4 commit `7a6716b5`, pushed): the
OpenRouter registry/static-map transport contradiction is **v4 bug 97**
(`bugs/bug-97-openrouter-registry-denies-vision.md`, with the fix spec —
flip the plugin's stale pre-vision `attachmentSupport` declaration to what
`provider.ts` has implemented since bug 45, comment-tied to its MIME list,
plus a registry-initialised test so jest reads the production branch), and
the `moderation-finish-reason.ts` docblock's "(bug 94)" mis-number was
corrected to bug 93 in the same commit. v5-side records updated to match:
CLAUDE.md's status bullet + baseline paragraph (v4 HEAD is now TWO commits
past the pin — bug 96 plus this NO-PORT-class filing), phase-4.md's
candidate 3, and the status-log follow-up.

#### 2026-08-23 — unify: the `a14a1811` vision round (P4.D106 ∥ P4.D107 ∥ P4.D108 ∥ P4.D109 ∥ P4.57)

_Versions: core 0.0.643, harness 0.0.559, host 0.0.79, web 0.0.79, SPA 0.5.548._

All five lanes unified; the oracle baseline moves to `a14a1811` and v4 bugs
91–95 are absorbed whole: the image-transport predicate + moderation
finish reasons + the three-tier attachment anchor (server), NanoGPT plugin
1.1.0's `image_url` wire + truthful ledger, the `describe_image` looking
verb end-to-end (with the production vision-tier wiring landed as the
unification wire), the attachment-failure warning toast + the client
attachment table's convergence, and tri-state decode-once across all three
settings verbs. The §3 review's headline catch — the vision tier was
structurally unreachable (driver wired, bytes store not) — plus the
restream ledger carry, raw error propagation in auto-describe, and seven
smaller findings, all fixed pre-merge. v4 drifted one commit mid-round
(`3c041e46`, bug 96 — the next catch-up); every regen ran from the pinned
worktree. Gate: the 24-family pinned sweep 24/24 zero SKIP; clippy both
feature sets; 449 test binaries / 2,299 / 0; release build; ng 341 files /
5,068; full Playwright 237/237 zero skips. Round record: `status-log.md`.

#### 2026-08-23 — fix(unify): the a14a1811-round unification wires + the §3 review findings

_Versions: core 0.0.643, harness 0.0.559, host 0.0.79._

The cross-lane wires: the `P4D107_NANOGPT_MANIFEST_LANDED` tripwire flipped
to plain equality (D107's manifest is on the branch), and P4.D108's recorded
follow-up landed whole — `describe_image`'s vision tier is now REACHABLE in
production: `OrchestratorDeps` + the spine's `tool_runner()` thread the
host's `HostImageDescribeRunner` AND the production photo-bytes store into
every tool runner (the §3 review caught that the driver half alone still
starved the tier on `no-bytes` — the runner's `NotConfiguredBytes` default
had zero production overrides), pinned by wiring probes + a mutation-proven
composition test.

The §3 review findings, all fixed here: `restream_into` never carried the
retry's attachment ledger (v4 overwrites `state.attachmentResults` on every
done — bug 94's new reader made the stale value user-visible; pin test
added); the auto-describe module relabeled DB failures as skip reasons
where v4 propagates raw (the persist arm blamed a vision call that had
SUCCEEDED — now `Result` with the raw text surfacing as the tool error);
the empty-response warn payload gained v4's `dangerMode`; the NANOGPT rows
joined all three request-envelope coverage floors; the id-set predicate
extracted + pinned against its named silent mutation (lowercase `"user"`);
`SamplingCapture` forwards the anchored entry point; `describe_image`
joined the doc-edit stray-call guard; a doc-comment splice in
`doc_mount_file_links` repaired; the `id-field-dropped` oracle case renamed
to its post-a14a1811 truth + a force-include-with-id row pinning the
deliberately-preserved id-drop quirk; two stale comments corrected.

#### 2026-08-23 — fix(model): the wire layer honors the attachment anchor instead of re-stamping last-user (bug 95, the stamp)

_Versions: core 0.0.633, host 0.0.78._

The downstream-stamp measurement the order required, and the one real
re-anchor it found: the streaming spine and the google merge are faithful
(per-message placement flows through; google merges per user-run exactly
as v4's plugin does), but the non-streaming regenerate funnel flattened
attachments into `CompletionParams.attachments` and the wire layer
re-stamped them onto the LAST user message — undoing the anchor whenever
a staff whisper trails the human's turn. Fixed with a defaulted
`send_message_with_anchor` trait method (canned/test providers ignore it;
their keys deliberately never see placement), the anchor threaded through
`execute_completion_with_anchor` into `request_input_from_params` (mapped
across the tool-role drop, last-user kept as the floor), the host's
`WireCompletionProvider` override, and the regenerate funnel computing
the carrier's position. Every pre-existing caller is byte-identical
(`CompletionParams` untouched — extending it would have edited a sibling
lane's file). Pinned by the updated last-user test, a new
anchored-placement unit test, and a new wire-byte pin through the full
composition on Z.AI (the anchored message's `image_url` part vs the
trailing whisper's plain string); neutralizing the anchored slot reddens
both.

#### 2026-08-23 — fix(chat): attachments anchor to the human's turn, not the last user-role message (bug 95)

_Versions: core 0.0.632, harness 0.0.555._

Ports v4 `a14a1811`'s bug-95 fix: staff whispers format as role=user, so
"the last user-role message" stopped meaning the human's turn — on a
regenerate the image landed on a connection-profile-change bubble, and
after a tool call attachments dropped silently. The message selector's
main-loop copy now carries the source row id (the force-include arm still
omits it, as v4's does); `ContextMessage.metadata` gains `isUserTurn` and
the history/new-user pushes stamp their metadata; and the new
`select_attachment_anchor_index` (this turn's user input → the last
genuine human turn by id, captured pre-normalization with the
`!systemSender` clause → the old rule as a floor → -1 with a warn)
replaces last-user-wins for BOTH the Lantern prefix and the attachment
splice, with the final construction extracted and unit-pinned. Pinned by
the new tier-1 `attachment_anchor_equivalence` family (v4's six shapes by
name + 12 adversarial rows, each of the three scans droppable only redly)
and seven affected families regenerated fresh at the pin — the
message-selector comparator was id-blind and now compares it (red-first
proven), and both build-context metadata stamps are mutation-proven live.

#### 2026-08-23 — fix(files): images route to the describer when the plugin can't send them (bug 91, the wirings)

_Versions: core 0.0.631, harness 0.0.554._

Wires the transport predicate into the three fallback sites, mirroring v4
`a14a1811`: `needs_fallback_processing` gains the image-only second arm
(model reads it but the plugin can't send it → describe-fallback, with
v4's info log), the auto-pick describer filter excludes non-transporting
providers, and a configured describer whose plugin cannot transport
answers `unsupported` with v4's guard sentence byte-exact — before any
model call is made. `file_attachment_tier3` regenerated at the pin with
six new ops (the Ollama-vision route, the non-image control, the
describer guard with the send-never-made assert at the mock level, the
auto-pick exclusion) over a fixture whose uncensored describer is
corrected to a transporting provider as v4's test was — Z.AI rather than
v4's OpenRouter pick, since OpenRouter only transports in the tier v4's
tests see (the unit-2 upstream finding). Three mutations proven red.

#### 2026-08-23 — feat(files): the image-transport predicate pair (bug 91, the predicate half)

_Versions: core 0.0.630, harness 0.0.553._

Ports v4 `a14a1811`'s two-tier "can this plugin actually send an image?"
predicate: the new `files/image_transport.rs::provider_can_transport_images`
(manifest tier → static tier → unknown-is-true) and
`attachment_support.rs::static_provider_can_transport_images` (keys off the
types list after the known-provider guard). The static map gains the §C1
rows — NANOGPT and Z_AI with the four image types, DEEPSEEK known-empty —
which also moves the settings route's `supportsImageUpload` create default
for those providers exactly as v4's shared map does. The registry-vs-static
collapse in v5 is measured and recorded in the module header: manifests are
baked, so the manifest tier answers for every known provider and the static
tier only for names the manifest set lacks. Pinned by the new tier-1
`image_transport_equivalence` family driving v4's REAL code in BOTH
configurations (registry uninitialized → static fallback, and initialized
with all ten real dist plugins → production truth; 51 rows). The NANOGPT
registry row rides an ACTIVATE-AT-UNIFY gate (`P4D107_NANOGPT_MANIFEST_
LANDED`) that self-arms when P4.D107's manifest regen lands. Two findings
recorded for upstream: v4's OpenRouter plugin registry entry declares
`supportsAttachments: false` while its static map and working vision path
say otherwise (production routes OpenRouter vision profiles to the
describe-fallback), and v4's own unit tests only ever see the static tier.
Not yet wired — the three fallback-site wirings are the next unit.

#### 2026-08-23 — feat(llm): provider moderation refusals named, not blamed on the weather (bug 93)

_Versions: core 0.0.629, harness 0.0.552._

Ports v4 `a14a1811`'s bug-93 fix end-to-end. The new
`quilltap-core::moderation_finish_reason` module carries the ten-literal set
(incl. the hyphenated `content-filter` the v4 docblock doesn't call out),
`is_moderation_finish_reason` (trim + lowercase set membership, no substring
guessing), and `describe_moderation_refusal` with v4's sentence byte-exact.
`get_empty_response_reason` gains the moderation first branch ahead of its
five pre-existing sentences (byte-unchanged) plus the uncensored-retry
suffix; the orchestrator's empty-response arm extracts the finish reason
from the raw response, threads provider/model, and its warn payload gains
`finishReason` + `moderationRefusal`. Pinned by the new tier-1
`moderation_finish_reason_equivalence` family (35 module rows + 12
`getEmptyResponseReason` matrix rows over v4's REAL code; dropping a
literal, neutralizing the first branch, and un-threading the orchestrator
each proven red) and a new `moderation_refusal` op in the orchestrator
tier-3 corpus — v4's real `processMessage` answers the moderation sentence
in the done frame where the "known issue" copy used to appear, and v5's
spine reproduces it. v4's module docblock mis-numbers itself "bug 94"; the
port records the discrepancy and keeps the code. Also mirrors v4's
bug-88…95 docs + `bugs.md` index into `docs/v4` and banks the two help-file
sections to the `p4.9i2` row.
#### 2026-08-23 — docs(porting): the P4.D107 verification gate record

_Docs-only change._

The lane's gate appended to `status-log.md`: fmt and both clippy feature
sets clean; `cargo test --workspace` 446 binaries / 2,270 tests / 0
failed with the lane's oracle env block, the three families confirmed to
have RUN by name; the sweep driver 3/3 ok over oracles regenerated fresh
against v4 at `a14a1811`; the corpus greps and the nine-sibling
byte-identity check.

#### 2026-08-23 — docs(v4): mirror the plugin-author attachment contract

_Docs-only change._

`docs/v4/developer/PROVIDER_PLUGIN_DEVELOPMENT.md` refreshed from v4
`a14a1811`: the new attachment-contract section for plugin authors — the
two questions the host asks before an image reaches a plugin, the rule
that declaring `supportsAttachments: false` is a respectable answer while
declaring `true` and dropping the bytes is not, the honest-ledger
requirement, and the guidance against a router plugin keeping its own
vision-model list.

The NanoGPT plugin README has no mirror here (`docs/v4/` mirrors v4's
`docs/` tree only), and the bug 91–95 write-ups are left for the sibling
lanes so no two lanes write the same mirror file.

Audited tree-wide: the retired "NanoGPT chat requests are text-only in
Quilltap" sentence survives nowhere in v5 outside porting records.

#### 2026-08-23 — feat(nanogpt): serialise image_url and report the real attachment ledger

_Versions: core 0.0.630, harness 0.0.552._

v4 `a14a1811` (bug 91, NanoGPT plugin 1.1.0) stops dropping image
attachments. `build_nanogpt_body` no longer calls `collect_drop_failures`
with the blanket "text-only" sentence; the new `nanogpt_user_content`
shapes user content exactly as v4's `buildUserContent` does: a plain
string when the message carries no attachments, otherwise a content-part
array — the text part only when the content is non-empty, one
`image_url` part per forwarded image (`url` first, else the `data:` URI),
`Unsupported file type: <mime>. NanoGPT forwards images only (<list>).`
for a MIME NanoGPT does not accept, `Attachment missing data or URL` for
a row carrying neither, and a single empty text part as the floor when
every attachment failed and there was no text.

The ledger the caller reports is now what the builder actually put on the
wire — advertised and executed can no longer disagree. Both compositions
read the same `BuiltRequest.attachment_results` from the one
`build_request`, as v4 uses one `buildBody` for both modes.

No vision-model list, deliberately, carrying v4's reasoning: NanoGPT
fronts hundreds of upstream models and such a list would be stale within
the week; the host has already decided by the time a request is built.

Envelope corpus 321 → 341. The two pre-existing NanoGPT
`image-attachment` rows flip; every other pre-existing row is
byte-identical. Eight new NanoGPT rows cover the data-URI, remote-`url`,
`url`-wins-over-data, unsupported-MIME, missing-data, mixed-ledger,
empty-content, all-failed-floor and attachment-on-a-non-user-message
arms, each recorded from v4's real dist plugin in both modes; seven
mutation proofs.

Mutation testing also found a pre-existing blind spot in two SIBLING
providers: no corpus bag had ever carried a `url`, so Z.AI's and
OpenRouter's `url`-before-data precedence stayed green under a swapped
spelling. Both gained a `image-attachment-url-wins` row (corpus only — no
source change), and both swaps now go red.

#### 2026-08-23 — feat(nanogpt): the manifest attachment flip (plugin 1.1.0)

_Versions: core 0.0.629._

NanoGPT's plugin declares image attachment support as of v4 `a14a1811`
(plugin 1.1.0), and the generated provider manifest follows it:
`supportsAttachments` true, the four image MIME types, the new description,
and the `notes` field explaining the host-decides posture (NanoGPT fronts
hundreds of models and deliberately keeps no vision-model list of its own).

Regenerated through `harness/oracle/providers/gen-provider-manifests.mjs`
against the v4 checkout at the pin — never hand-edited. The generator's
augmentation table needed no extension: `notes` was already read off the
plugin's `attachmentSupport` object. The nine sibling manifests are
byte-identical.

The family that pins this block is `provider_registry_equivalence` (its
`attachmentSupport` row), not `providers_listing_equivalence` — the
listing oracle carries no attachment data at all. Proven red-first against
the pre-flip manifest, then green.
#### 2026-08-23 — docs(porting): the P4.D108 lane-closed record

_Docs-only change._

The lane-closing record in the status log: six commits, the gate of
record (446 binaries / 2,274 tests / 0 failed; the seven families 7/7
fresh at `a14a1811` through the sweep driver, zero SKIP), and the loud
deferrals (the dogfood live proof, the OrchestratorDeps/spine vision-tier
wiring, the chat_files upload-time no-op, the shared bugs.md index for
the unifier).

#### 2026-08-23 — fix(tools): the tools-inventory count tripwire moves 40 → 41 (P4.D108 unit 6)

_Versions: core 0.0.634._

The lane's final full workspace gate caught the one red the targeted runs
had missed: `every_schema_key_resolves` (the tools-inventory module's own
unit test) pins `BUILT_IN_TOOLS.len()` and still said 40 after unit 4
added the describe_image row. The tripwire fired exactly as designed —
moved to 41 with the bug-92 provenance comment; `SCHEMA_KEYS` correctly
stays 37 (the photo-tool no-schema quirk). Workspace re-run green.

#### 2026-08-23 — feat(librarian): the describe_image upload-announcement rewrites (P4.D108 unit 5)

_Versions: core 0.0.633._

The five Librarian sentence rewrites byte-exact from v4 `a14a1811`:
`attach_id_hint` ("call describe_image with it to be told what it
depicts…"), `build_upload_content` single + plural (the "if your eyes
read pictures you are looking at it already" pair ending "neither of
those shows it to you"), and `build_upload_opaque_content` single +
plural (the "so a vision-capable model can see it already" pair ending
"neither one shows it to you" — the two spellings differ deliberately;
each byte-copied). The librarian family regenerated fresh at the pin
went red-first against the old strings and green after; a verb-order
mutation (attach before describe) proven red-then-restored. The Lantern
`build_content` doc comment ports v4's comment-only change, with the
no-output-moved claim proven by the concierge-lantern-suparna family
running green against a pin-fresh oracle with zero v5 string changes.
Also: the bug-92 fix doc mirrored into `docs/v4/developer/bugs/fixed/`,
and the `help/keep-image-tools.md` rewrite (retitled H1 + the
describe_image subsection) banked verbatim into `m6-screen-parity.md`
row 11 for the `p4.9i2` help family.

#### 2026-08-23 — feat(tools): register describe_image at every layer (P4.D108 unit 4)

_Versions: core 0.0.632._

The registration sweep: `DOC_EDIT_TOOL_NAMES` + the executor's two
recognized-name lists + `PHOTO_TOOLS` + the dispatch arm +
`run_describe_image` (precheck on the writer thread → the real
auto-describe module composed from the runner's own `file_bytes` /
`photo_side_effects` seams and the new injected `ImageDescribeDriver`
[`with_image_describe`; default `None` answers v4's `describe-failed`
shape, so tiers 1/2 stay fully live] → the after-vision serve); the
`describeImage` slate push immediately after `attachImage` inside the
document-editing gate (comment 18+3 → 18+4); the 41st
`tools_inventory` row with the measured quirk carried (v4 at `a14a1811`
registers describe_image ONLY as a BUILT_IN_TOOLS row — no schema-map
entry, no availability arm). Red-first at fresh `a14a1811` oracles:
`tool_build` (the slate lacked the key), `tools_inventory` (40 vs 41),
and `tool_dispatch` (the new unresolvable-uuid describe op) all failed,
then went green. The dispatch fixture builder gained an
`ensureCollection('files', …)` — v4-side jest creates the table lazily
on first repo touch, the Rust side opens the raw fixture. `tool_wire`:
measured surface unmoved (the recorded slice never carried photo
tools); the committed corpus untouched, family green. The three prompt
renderings (native / simple-json / text-block): the order's premise
REFUTED by measurement — neither v4 nor v5 renders any photo tool
there and v4's commit touched none of them; no edits. The live-turn
(`OrchestratorDeps`) and spine `tool_runner()` wiring of the driver is
the recorded follow-up — both files are this lane's must-NOT boundary.

#### 2026-08-23 — feat(tools): the describe_image handler + the photo-tools family widening (P4.D108 unit 3)

_Versions: core 0.0.631, harness 0.0.552._

`tools/photo.rs` gains v4's `handleDescribeImage` split at the vision seam:
`resolve_describable_file_entry` (direct image-v2 id → album link id →
sha256 sisters, IMAGE-preferring; album membership deliberately NOT
required), `handle_describe_image_precheck` (the two free tiers — stored
description, then generation revisedPrompt || prompt — plus the No-image
and not-an-image sentences byte-exact), and
`handle_describe_image_after_vision` (serve the vision result, the
already-described race re-read, or the could-not-describe sentence naming
the skip reason). The attach_image no-kept-image error takes v4's rewritten
sentence naming both verbs. The photo-tools family grows 14 → 29 ops: nine
describe_image handler ops (the auto-describe module canned at v4's own
jest-mock level, incl. a race op whose mock writes the winner's description
first) and six module-level ops driving the REAL
`auto_describe_chat_image_attachment` with `generateImageDescription`
mocked at the §C2 seam, three of them dumping the module's three sink
tables (files / links / chunks) in the shared-id remap form. A completeness
assert now pins oracle rows == mirrored ops. Fixture: four new spec images
(described / promptonly / blank / plaintext) through the same builder;
oracle + fixture regenerated fresh at `a14a1811`. Mutation-proven four
ways: tier reorder → describe_stored red; direct-id arm dropped →
red; race re-read dropped → describe_race red; kept-link skip removed →
autodescribe_kept red.

#### 2026-08-23 — feat(photos): port the auto-describe-attachment pipeline (P4.D108 unit 2)

_Versions: core 0.0.630._

`photos/auto_describe_attachment.rs` ports v4's
`lib/photos/auto-describe-attachment.ts` whole: the four pre-bytes skip
arms in v4's order (`not-found` / `not-image` / `no-sha` /
`already-described`), the bytes read behind the existing `FileBytesStore`
seam (a failure is v4's warn + `no-bytes`), the vision call behind the §C2
`ImageDescribeDriver` (the host's driver runs
`generate_image_description` whole — profile resolution, refusals, the
uncensored retry, `logLLMCall`), and the three-sink persist pass: the
`files.description` write, every blank hard link updated (per-mount
`description` + `extractedText` + the real chunk pass; kept-image links
with markdown are left untouched), and the per-mount embedding enqueue
through the recorded `SaveImageSideEffects` seam (the photo-trio
precedent — v4's differential jest-mocks the enqueue identically). The
links repo gained the dedicated `apply_auto_description` update mirroring
v4's `docMountFileLinks.update` call. Five unit tests over a provisioned
fresh two-partition instance pin the skip vocabulary (incl. the
whitespace-only-description trim quirk carried from v4), the
three-sink success path, the second-run `already-described`
short-circuit with zero vision calls, and the kept-link skip. The
module's differential (driving v4's real module with
`generateImageDescription` mocked at the §C2 level) lands with the
photo-tools family widening in unit 3.

#### 2026-08-23 — feat(tools): the describe_image catalog entry + the attach/keep/list copy rewrites (P4.D108 unit 1)

_Versions: core 0.0.629._

The tool-definition catalog regenerated at the `a14a1811` pin via
`gen-tool-catalog.mjs`: 57 → 58 entries with the new `describeImage`
definition (v4 bug 92 — the looking verb) in v4 `ALL_TOOLS` order after
`deleteAnnotation`, plus the byte-exact description rewrites v4 shipped
alongside it (`attach_image` is now explicitly a display verb that "does
NOT let you see the picture", `keep_image` is "filing, not looking", and
`list_images` names the `describe_image` arm). Both tool-definitions
oracle cases gained the new import; the count tripwire in
`definitions/mod.rs` moved 57 → 58. Red-first: the family failed on the
57 != 58 size mismatch against the fresh oracle before the regen, green
after. The handler, registration, and Librarian rewrites follow in this
lane's later units.
#### 2026-08-23 — refactor(api): decode the settings tri-state once (taboo, brahma-console, data-retention)

_Versions: core 0.0.629, harness 0.0.552, web 0.0.79._

Three settings PUT surfaces — the Taboo list, the Brahma Console turn budget,
and the data-retention window — each derived the absent / explicit-`null` /
value tri-state three separate times: once at the web edge, once in the engine's
dispatch arm (rebuilding a JSON bag), and once in the handler (re-reading that
bag). Every re-derivation is a chance to collapse two of the three states, which
is what the Taboo review caught on one verb and what P4.56 found still live on
another.

There is now exactly one decoder per verb, in `api/types.rs`
(`taboo_update_request`, `brahma_console_update_request`,
`data_retention_update_request`), and all three callers use it: the web edge
(now a single shared `settings_update_put` for all three routes), the dispatch
layer (the engine arms pass the decoded tri-state straight through), and the
route differential. The handlers take `Option<Option<Value>>` directly instead
of a rebuilt bag.

No observable behavior changes. All 138 driven rows of
`settings_routes_equivalence` are byte-identical before and after against an
oracle regenerated at `a14a1811`, and the explicit-`null` 400 on each of the
three surfaces was mutation-proven still red-capable. A new unit test pins each
decoder against its literal wire key and tag — a typo there would silently turn
every body into the keep-current arm at the edge, the dispatch layer and the
differential at once. The live web-edge test grew from one surface to all three,
each over the full tri-state; the Taboo edge had never been walked live at all.
#### 2026-08-23 — docs(porting): the P4.D109 lane record — Tier 2 reads, deferral, gate

_No crate versions bumped._

Closes the P4.D109 lane record with its Tier 2 verification reads (bug 93's
moderation sentence already has a client reader through the existing
`emptyResponseReason` path; `describe_image` renders through the tool card's
name fallback, as every other photo tool does in both apps), the loud Tier 3
deferral of the live proof to the dogfood queue, and the verification gate.

#### 2026-08-23 — test(e2e): walk the dropped-attachment warning live (bug 94)

_Versions: SPA 0.5.548._

A new Playwright beat sends in Group Expedition against the mock LLM and asserts
v4's plural sentence in the real toast stack — the `(and N more)` suffix before
the colon, and only the first plugin error.

One thing is faked: the bytes of one `done` frame, rewritten by an init script
that wraps `EventSource.prototype.onmessage` before app boot. Everything
downstream is real — the live EventSource, the transport parse, the reducer
carry, the Salon's toast door, the toast stack in the DOM. Provoking a genuine
ledger is not reachable from the e2e's mock OpenAI-compatible endpoint: after
this round's bug-91 fix the host routes an untransportable image to the
describer instead of handing it to a plugin that drops it, so a real failure
needs a per-attachment fault inside a plugin that does send images. The live
proof on real data rides the P4.D106/D107 dogfood walk.

Mutation-proven: with the door's `length > 0` guard broken, the beat's 44 polls
resolve to zero warnings and it fails.

#### 2026-08-23 — fix(settings): the client attachment table learns NanoGPT, DeepSeek and Z.AI (bug 91)

_Versions: SPA 0.5.547._

v5's transcription of v4's static client capability table carried a doc comment
recording its own staleness: three shipped providers had no row, so all three
fell through the unknown-provider branch to "no attachments", and a new profile
on any of them started with the vision box unticked. The note said the fix
belonged upstream. v4 `a14a1811` is that fix, so the rows land here with it —
NANOGPT and Z_AI with the four image types, DEEPSEEK explicitly empty — and the
staleness paragraph is rewritten to record the convergence.

The seed moves with the table: a new NanoGPT or Z.AI profile now ticks the
vision box, DeepSeek does not, and an endpoint the table has never heard of is
unchanged. v5's row shape stores only `types` (the one member its two readers
consult), so v4's per-row `description`/`notes` prose stays out, as it has since
P4.21.

#### 2026-08-23 — fix(salon): warn when the provider dropped an attachment (bug 94)

_Versions: SPA 0.5.546._

The ledger now has a reader. `reportStreamTransitions` — the Salon's single
door from reducer transitions to toasts — raises v4's warning when a done frame
carries failed attachments, with v4's message construction transcribed: the
singular/plural arms, the `(and N more)` suffix before the colon, the
first-error-only rule, and the `unknown reason` fallback for a plugin that
reported a failure without saying why.

v4 reads the ledger inside `if (data.done)`, before its chain branch, so it
warns once per done EVENT — intermediate dones included. Riding transitions
instead of events, the ledger object's identity is that key: a new done brings
its own object, while the Courier's `pendingExternalTurn` patch spreads the
previous one forward and a later `chainComplete` leaves `finalDone` alone.
Six specs pin the arms; inverting the identity comparison reddens five of them.

Nothing was added on the Brahma console, which shares the reducer: v4's fix is
Salon-only (`useSSEStreaming.ts` is the Salon hook) and v5's Brahma consumer
reads only `state.error`, so the carry exposes no toast there.

#### 2026-08-23 — fix(salon): carry the done frame's attachment ledger through the reducer (bug 94)

_Versions: SPA 0.5.545._

v4's `SSEEvent` gained `attachmentResults` at `a14a1811` so the Salon could
warn about attachments a provider plugin never put on the wire. v5 reads its
frames through a pure reducer, so the field needs two homes before any render
site can see it: `ChatStreamFrame.attachmentResults` on the contract (typed as
the round's frozen `{ sent?, failed?: [{ id, error }] } | null` shape) and
`FinalDoneInfo.attachmentResults` off the fold.

The carry preserves the frame's own object reference rather than copying it —
the render site keys "warn once per done" off identity, and the Courier's
`pendingExternalTurn` patch spreads the previous ledger forward unchanged, so
identity is exactly what separates a new done from a re-render. Pinned at the
reducer level (a render-site spec cannot see a dropped carry), mutation-proven
red first.

#### 2026-08-23 — docs(porting): the a14a1811 vision-round work orders (five lanes)

_Docs-only; no version bumps._

The next round planned against a moved oracle baseline: v4 HEAD is
`a14a1811` ("characters can look at images, and images reach vision models",
bugs 91–95), three commits past `f8973813` — the other two dispositioned
NO-PORT at planning (`65f3476e` already ruled at the f8973813 round;
`718c9ada` is pure CI/bundler, the Turbopack revert, zero lib content).
Five work orders committed: P4.D106 (the image-transport predicate +
moderation finish reasons + the three-tier attachment anchor, server),
P4.D107 (NanoGPT plugin 1.1.0 — `image_url` serialization + the real
attachment ledger + the manifest flip), P4.D108 (the `describe_image`
looking verb end-to-end, incl. the auto-describe module v5 never ported),
P4.D109 (the attachment-failure toast + the client attachment table's
stale-note retirement, SPA), and P4.57 (tri-state decode-once for
taboo/brahma-console — the P4.56 recorded lead, a no-behavior-change
consolidation). Three binding shared contracts pinned across the sibling
orders: the a14a1811 attachment-capability values (three homes), the
describer seam (file_fallback's frozen public fns), and the done-frame
`attachmentResults` wire shape (chat_events.rs frozen for the round).

#### 2026-08-23 — fix(logging): the measured duration on the streaming chat log (dogfood #100)

_Versions: core 0.0.628, harness 0.0.551._

The 2026-08-22 dogfood pass found every streamed `CHAT_MESSAGE` row logging
`durationMs = 0`, against 6,115 v4-written rows on the same instance where not
one zero appears. Three sites hard-coded the value — the streaming chat write
and both image-description writes — each behind a comment calling it a tracked
follow-up because a measured stream clock could not be diffed. That blocker had
already been lifted by `normalize_duration_ms`, which collapses any non-NULL
duration to a presence marker on both sides; the comments never caught up.

`StreamLogCtx` now carries `started_at_ms`, stamped where v4 takes its own
`startTime` — once per `streamMessage` entry, so the primary attempt, the
tool-unsupported retry, and each failover leg time only their own call.
`describe_image_with_profile` takes v4's `describeStart` at the top and both its
log sites subtract from it. Since no differential can tell a hard-coded zero
from a measured one once the column is normalized, the guard is a source census
in the `db_error_key_guard` idiom, mutation-proven red-first. Verified live: the
next streamed turn logged 9,601 ms.

#### 2026-08-22 — unify: the `f8973813` NanoGPT-caching + settings-wire round (P4.D105 ∥ P4.56)

_Versions: core 0.0.627, harness 0.0.550, web 0.0.78._

Both lanes unified; the oracle baseline stays `f8973813` (v4's one newer
commit `65f3476e` is CI/release infra + a comment-only lib edit +
standalone-tarball native linking v5 doesn't have — NO-PORT with evidence;
every gate regen ran from a pinned worktree). P4.D105: NanoGPT prompt
caching whole — the Prompt Caching options group through the manifest
generator, the `promptCaching` body key behind the strict `=== true` gate
with the TTL collapse, both-dialect cache-usage normalization with the
`??`-precedence pin measured on v4 and cache reads excluded from
prompt/total on both the non-streaming response and the streaming final
chunk (which also gains unconditional `rawProviderUsage`), plus the
`response_parse_equivalence` run line. P4.56: the B2 data-retention
present-`null` collapse fixed red-first via `double_option` behind the
harness serde-path rewire, the new `GET/PUT /api/v1/settings/data-retention`
edge (which uncovered and fixed the `BrahmaConsole` success-arm 500 standing
since P4.D57, and two leaked-DbError sentences), the groups cleared-null pin
(zero source change), `settings_wire_actions` self-containment, the
float-literal store fix, and the shared `apiKeyId`/`baseUrl` classify
readers. The §3 review found no blocking findings. Unification wire: the
NanoGPT spec-fixture transcription gained the Prompt Caching group with two
showIf render specs. Gate: 12/12 families fresh at the pin; 445 test
binaries / 2,269 / 0; clippy both feature sets; release build; ng 341 files
/ 5,056; full Playwright green (numbers in the round record).

#### 2026-08-22 — docs(porting): the P4.D105 v4 mirror refresh and lane dispositions

_Docs-only change._

Refresh `docs/v4/CHANGELOG.md` from the pinned `f8973813` worktree — the
only one of the commit's four doc files that lives inside the mirror. It
was two rounds stale, so this also brings in the `a6870c5a`
person-consistency entry.

Record in `status-log.md`: the two `help/connection-profiles.md` NanoGPT
bullets banked verbatim for `p4.9i2` (they live at v4's top-level `help/`,
outside the mirror); the spot-check of v4's audit claims (reasoning-effort
values, the `delta.reasoning` / `reasoning_content` precedence,
`stream_options.include_usage`, tool-call delta accumulation — all three
already hold in v5, nothing to port); cache-read pricing verified as a
NO-PORT (v4 has no NanoGPT pricing row at the pin); the downstream
`cacheUsage` / `rawProviderUsage` plumbing verified generic rather than
re-plumbed; and two deferrals — the live caching smoke to the dogfood
queue, and the SPA's hand-transcribed NanoGPT schema fixture, which this
lane may not touch.

#### 2026-08-22 — feat(nanogpt): cache usage and rawProviderUsage on the streaming final chunk

_Versions: core 0.0.623, harness 0.0.545._

The streaming half of v4 `f8973813`. `Flavor::NanoGpt` now derives cache
usage through the SAME `nanogpt_cache_usage` the non-streaming arm calls —
v4 has one `extractCacheUsage` serving both, and two copies here would be
two places to drift — and applies the same cache-read exclusion to the
final chunk's `promptTokens`/`totalTokens`. `build_usage` gained a `written`
counter alongside `cached`; NanoGPT is the only flavor of the five whose
gateway reports cache writes, so the other four pass `None` and their
recorded bytes are unchanged.

NanoGPT also joins Z.AI in emitting `rawProviderUsage` on the final chunk:
v4's `(usage ?? null)`, so the KEY is always present — an explicit `null`
when the stream carried no usage frame, not an absent field.

The `chat_completions_sse` corpus grows 16 → 22 cases (six NanoGPT wires:
the Anthropic dialect, write-only, the OpenAI dialect, a present-zero read,
the clamp, and a stream with no usage frame at all). All five sibling
providers' recordings are byte-identical; the five pre-existing NanoGPT
recordings change only by gaining `rawProviderUsage`. Both consumers —
`stream_decoders_equivalence` (three chunkings) and
`streaming_composer_equivalence` — are green, and the decoder differential
gains six coverage arms plus a per-case assert that v4's own recorded
`promptTokens` is `max(0, prompt_tokens - cacheRead)`.

#### 2026-08-22 — feat(nanogpt): both cache-usage dialects on the non-streaming response

_Versions: core 0.0.622, harness 0.0.544._

Port v4's new `extractCacheUsage` into the `ChatFlavor::NanoGpt` arm of
`response_parse`. It reads BOTH dialects the gateway emits — Anthropic-style
`cache_read_input_tokens` / `cache_creation_input_tokens` and OpenAI-style
`prompt_tokens_details.cached_tokens` — with v4's `??` precedence, so a
present-but-zero `cache_read_input_tokens` does not fall through to the
OpenAI key. No cacheUsage at all when neither counter is positive; the read
and write keys are independently conditional.

The house rule then applies at the call site: cache reads come out of
`promptTokens` and `totalTokens` (clamped at zero), `completionTokens`
untouched. Before this, a NanoGPT turn billed every cached input token at
full price.

Response-bodies corpus 46 → 52 rows (six NanoGPT cases recorded from v4's
real `sendMessage` with the SDK mocked below it); every pre-existing row is
byte-identical. The differential gains the missing `NANOGPT` family-presence
guard, four cache coverage arms, and a per-row assert that v4's recorded
`promptTokens` really is `max(0, prompt_tokens - cacheRead)`.

`response_parse_equivalence` also gets its self-contained run line — the
last `nothing_to_run` envelope row (`recipe_sweep.py --self-test` clean, the
family runs attributably by name).

#### 2026-08-22 — feat(nanogpt): the Prompt Caching profile options and the `promptCaching` body key

_Versions: core 0.0.621, harness 0.0.543._

Regenerate the ten built-in provider manifests against v4 `f8973813`
(NanoGPT plugin 1.0.3). Only `nanogpt.json` moves: its `optionsSchema`
gains v4's new "Prompt Caching" group — the `enablePromptCaching` boolean
(default `false`) and the `cacheTTL` enum (`5m` / `1h`, default `5m`,
gated by `showIf: { field: 'enablePromptCaching', equals: true }`) — with
the group and field help text byte-copied from the plugin. The nine
sibling manifests are byte-identical.

No Rust change: `optionsSchema` is carried as an opaque value, and the
SPA's provider-options panel already renders booleans and `showIf`-gated
enums generically (P4.D84). `providers_listing_equivalence` (which pins
`optionsSchema` byte-for-byte, field order included) and
`provider_registry_equivalence` both re-run green over oracles
regenerated fresh at the pin.

`build_nanogpt_body` then emits v4's body-level helper:
`promptCaching: { enabled: true, ttl }` between the `user` key and the
allow-listed profile params, under a STRICT `enablePromptCaching === true`
gate (a truthy `1` does not arm it) and v4's TTL collapse (only the
literal `'1h'` buys the hour). Both option keys are consumed and stay off
`NANOGPT_PROFILE_ALLOWLIST`, so neither reaches the wire verbatim —
asserted by a unit test over the list and by a corpus row carrying both.

The request-envelope corpus grows 307 → 321 rows (seven NanoGPT cases in
both modes, recorded from v4's real `buildRequestBody`); every
pre-existing row is byte-identical. The differential gains four
coverage-shape asserts (both TTLs, the caching-off arm, the
consumed-keys arm) read off v4's recorded body, so a corpus that lost
the vectors cannot pass green.
#### 2026-08-22 — docs(porting): the P4.56 lane gate record

_Docs-only change._

The settings-wire remainder lane's verification gate: the six-family regen+run
sweep from a pinned `f8973813` worktree (`{'ok': 6}`, zero SKIP), the nine new
arms grepped present in the fresh NDJSONs, `cargo test --workspace` at 445
binaries / 2,267 passed / 0 failed with zero SKIP lines, clippy on both feature
sets, the release build, and `recipe_sweep.py --self-test` at 0 failures. Also
records the Tier-3 dispositions: the SPA data-retention card needs no follow-up
(it cannot send a `null`), and the taboo / brahma-console siblings still carry
the bag-shaped wire this lane's edge demonstrates the alternative to.

#### 2026-08-22 — refactor(settings): one reader for the profile `apiKeyId` / `baseUrl` semantics

_Versions: core 0.0.624._

P4.55 fixed the missing-`else` sub-family at three sites and left the
triplication as a named cleanup. The JS SEMANTICS now live once —
`classify_api_key_id` and `classify_base_url` in `api/settings.rs` — and the
three profile-update handlers (connection, image, embedding) keep only what
genuinely differs: which patch struct, which lookup, which fixed 500 sentence.
v4's reasoning (`findApiKeyById` on a non-string, `baseUrl || null` as JS
falsiness) and the recorded terminal-but-after-side-effects divergence are
stated once instead of three times.

Behavior-neutral by construction and by measurement: all three families are
green with no NDJSON movement, and a mutation that turns either reader's refusal
arm into a clear reddens all three — so the shared reader is genuinely live at
every site, not left beside dead code.

#### 2026-08-22 — fix(memories): store integer-valued floats the JS way

_Versions: core 0.0.623, harness 0.0.547._

Zod's `.int()` is `Number.isInteger`, so `{"perCharacterCap": 200.0}` PASSES
validation; JS then has no float/int distinction and `JSON.stringify` writes the
stored cell as `200`. serde_json keeps the float, so v5's two memories-config
setters stored and echoed `200.0`. Both merged bags now go through the shared
`normalize_js_numbers` walk before they are written or echoed — genuine
fractions (`softStartFraction: 0.7`) are untouched, `1.0` collapses to `1`
exactly as `JSON.stringify` writes it.

Pinned by `housekeeping_config_set_int_float_literal` and
`extraction_limits_set_int_float_literal`, each comparing the echo AND the
re-read stored bag. Both arms deliberately bypass the family's `norm` helper:
its `canon_numbers` normalizer collapses integer-valued floats on both sides,
which is precisely the difference being measured — a new float-sensitive
`norm_float_exact` compares them instead. Measured RED first (`1.0` vs `1` on
all four comparands), green after.

#### 2026-08-22 — test(harness): `settings_wire_actions` builds the fixture it reads

_Versions: harness 0.0.546._

The family reads `/tmp/qt-settings-fixture.db` and its recipe never built it —
only `settings_routes_equivalence`'s regen invokes the builder. P4.54 measured
the consequence: with `/tmp` cleaned the family does not SKIP, it FAILS every
case, so it was green only because a sibling happened to run first. Its header
now carries the fixture build as its own regen stage (the same idempotent
builder invocation, spelled with the load-bearing `V5W=${V5W:-…}` form).

Proven from a genuinely clean slate: `/tmp/qt-settings-fixture.db` deleted, then
`recipe_sweep.py --run settings_wire_actions` rebuilds it and passes 5/5.
`--self-test` stays at 0 failures — this family was never one of the two the
self-test structurally requires to remain in the `nothing_to_run` debt list (it
had a run line all along; what it lacked was a regen).

#### 2026-08-22 — test(groups): pin the cleared-null echo on the groups side

_Versions: harness 0.0.545._

The owed groups-side counterpart to P4.55's `update_clear_description` on
projects. `description` is store-resident for a group (a markdown file, not a
slim column), so the DB patch is empty and both sides take the store-backed
update's no-DB-work branch — v4 re-reads through `applyOverlayOne(_findById(id))`,
v5 re-reads unconditionally. The fresh oracle records what v4 actually answers:
`"description": null`, present in the echo rather than omitted. v5 matches.
Zero source change, as predicted — one `store_backed.rs`, two `StoreEntity`
impls, so groups inherited the projects verdict by construction and now has its
own pin.

#### 2026-08-22 — feat(web): the data-retention REST edge (and the brahma-console edge that never worked)

_Versions: core 0.0.622, web 0.0.78._

`quilltap-web` served no data-retention route at all: v4's
`GET/PUT /api/v1/settings/data-retention` had no v5 counterpart, so the setting
was reachable only over `/api/dispatch`. The pair now exists, modeled on the
taboo edge but decoding the PUT body into `Request::DataRetentionSettingsUpdate`
through serde rather than hand-building the variant — the absent /
explicit-`null` / value tri-state is resolved by exactly one piece of code, and
a re-implementation at the edge is what made the Taboo differential blind.

Found while extending `unwrap_to_http`'s success arm: `CoreResponse::BrahmaConsole`
had been missing from that hand-maintained variant list since P4.D57, so BOTH
brahma-console edges answered 500 `Unexpected core response` on every SUCCESS.
Only the error path worked, because errors leave through `CoreResponse::Error` —
which is why the settings differential (which drives the handler) could never
see it. Fixed, and both settings pairs are now asserted on their success path by
the new `data_retention_web_routes` live wire test.

Also: the two data-retention handlers leaked the raw `DbError` text where v4's
catch answers its own fixed sentence (`Failed to fetch…` / `Failed to update…`).
Harmless while nothing served the route; the first thing an operator would have
seen now that something does.

The wire test is mutation-proven in both directions — reverting the
`double_option` field makes the explicit-`null` PUT answer 200 instead of 400,
and removing the `BrahmaConsole` arm makes its GET answer 500 instead of 200.

#### 2026-08-22 — fix(settings): an explicit `null` staleChatDays is a 400, not a silent keep

_Versions: core 0.0.621, harness 0.0.544._

`Request::DataRetentionSettingsUpdate` carried `#[serde(default)]
stale_chat_days: Option<Value>`, so serde mapped an explicit `null` to `None`
indistinguishably from an absent key; the dispatch arm then built `{}` and the
handler kept the stored value at 200. v4's Zod `.default(30)` fires only for
`undefined`, so `{"staleChatDays": null}` is a 400 `Validation error` there.
Ironically this variant's own doc comment was cited as the precedent for the
Taboo fix while still carrying the bug.

The field is now the `double_option` tri-state and the dispatch arm has the
three-arm match its siblings have: absent keeps the stored value,
`Some(Some(v))` rides raw to the handler's Zod-faithful parse, and `Some(None)`
carries the explicit `null` through to that same parse, which refuses it. Two
arms pin it — `dr_put_null` (v4's exact 400 body) and
`dr_put_null_writes_nothing` (seeded at 120, `after` refetch proves the refusal
wrote nothing). Both were measured RED against the pre-fix code through the
rewired serde path (`{"staleChatDays":30}` where v4 answers
`{"error":"Validation error"}`). Family floor 15; 141 cases matched.

The Angular card is unaffected: `data-retention-settings.ts` only ever sends a
finite in-range integer, so no client surface can now see a 400 it did not see
before.

#### 2026-08-22 — test(settings): the data-retention differential rides the `Request` serde path

_Versions: harness 0.0.543._

The settings-routes family's `dataRetention` PUT leg handed the oracle's raw
body straight to `settings::data_retention_settings_update`, bypassing the
`Request` enum's serde entirely. Every arm therefore proved the handler and
nothing about the wire — a present-`null` arm added against that leg would have
passed green while the real wire (dispatch, and the REST edge this lane adds)
collapsed `null` to key-absent. The leg now decodes the body into
`Request::DataRetentionSettingsUpdate` exactly as the wire does (schema key
retained, tagged `type` inserted, serde deciding) and maps the result to the
handler's bag the way `engine.rs` does.

Four new arms, and the seeding the family never had: `seedDataRetention` writes
a non-default `instance_settings['dataRetention']` through v4's real setter (and
the ported one) before the case runs, so "kept the current value" is
distinguishable from "reset to the schema default 30" — `dr_get_seeded`,
`dr_put_empty_merge_seeded`, `dr_put_string_body` (v4's non-object spread), and
`dr_put_invalid_writes_nothing`. `dr_put_valid` and the three seeded PUTs carry
the family's new `after: 'dataRetention'` refetch, so the persisted effect is a
comparand rather than an inference. Family floor: 13 rows. 139 cases matched.

#### 2026-08-22 — docs(porting): work orders for the `f8973813` round (P4.D105 ∥ P4.56)

_Docs-only change._

Plan the next round against the new v4 baseline `f8973813` (one commit of
drift: NanoGPT prompt caching, plugin 1.0.3; bugfix unmoved). Two disjoint
lanes: P4.D105 absorbs the drift (the two Prompt Caching profile options
through the manifest generator, the `promptCaching` body key, both-dialect
cache-usage normalization with the cache-read exclusion, plus the
`response_parse_equivalence` run-line debt), and P4.56 closes the P4.55
named remainders (the B2 data-retention present-`null` collapse with the
harness edge-mapping rewire landing first, the missing `quilltap-web`
data-retention REST edge, the groups-side cleared-null pin, the memories
float-literal nit, the shared `apiKeyId`/`baseUrl` helper, and
`settings_wire_actions` self-containment).

#### 2026-08-22 — unify: the `a6870c5a` prompts-trio round (P4.D103 ∥ P4.D104 ∥ P4.55)

_Versions: core 0.0.620, harness 0.0.542, SPA 0.5.544._

All three lanes unified; the oracle baseline moves `4cb1035e` → `a6870c5a`
and the drift debt is cleared. Server: the standing-instructions section
(project + group `instructions`) lands in the cacheable system prompt
between the Taboo section and the tool instructions (cache structure
version 3 → 4), threaded to the live turn, Carina, and self-inventory,
with the Prospero project-context whisper's duplicate section dropped;
bug 88's second-person tool reinforcement replaces the pronoun lookup;
the identity stack moves to person-consistent wording under the new
version-stamped `compiledIdentityStacks` envelope, with v4's golden table
byte-copied (v5's computed hash equals v4's registered golden — a free
cross-implementation proof); the group verbs gain `instructions` plus
both v4 validators, ported whole. SPA: the shared prompt-field label and
the byte-identical twelve-key hints table, the seven-surface migration
sweep converging the drifted create/edit copy, the Group Instructions
editor, and the round-trip beat activated at unification (its first live
run green). Maintenance: the `c8a3cf77` merge-verb silent-keep lead is
closed — the two memories config verbs validate first (they had been
persisting garbage), the autonomous-rooms settings patch refuses invalid
fields, projects update validates through v4's schema, the three
missing-`else` `apiKeyId`/`baseUrl` sites are fixed, and the store-backed
cleared-null echo measured NOT divergent. The §3 review's ten findings
were fixed on the unify branch (previous entry). Gate: the 43-family
regen+run sweep 43/43 ok zero SKIP over oracles fresh at `a6870c5a`;
`cargo test --workspace` 444 binaries / 2,266 / 0 with the 75-variable
env block; clippy both feature sets; release build; `npm test` 341 files
/ 5,054; `npm run build`; full Playwright 236/236 zero skips.

#### 2026-08-22 — fix(unify): the a6870c5a-round §3 review findings

_Versions: core 0.0.620, harness 0.0.542, SPA 0.5.544._

Ten §3 unification-review findings fixed on the unify branch before the
gate. The two that would have shipped: `group_update` parsed the body
BEFORE the existence check, answering 400 where v4's find-first order
answers 404 for a missing group with a garbage patch (fixed to v4's
order; pinned by the new `update_missing_group_invalid_body_404` arm) —
and the autonomous-rooms `title` max counted Unicode scalars where Zod
measures UTF-16 code units, silently widening the limit for astral text
(fixed; pinned by the new `update_invalid_title_astral` arm). Also:
the settings-routes `connection_profiles` stale-oracle floor raised
19 → 22 for P4.55's three new arms (the row-driven family would have
passed green with them absent); the gated group-instructions beat's
clear-save leg moved from `waitForRequest` to `waitForResponse` (the
twice-deflaked navigate-aborts-the-save class, fixed before its first
live run); the recorded-only `create_non_string_instructions_400` row
gained a status assert; the identity-compiler family gained a
sentinel-presence tripwire (fixture-agreeing-with-itself guard); the
three profile `baseUrl` comments now record the v4 terminal-failure
sequencing divergence instead of overclaiming "nothing written";
details-tab regained v4's six `htmlFor` attributes; two doc-string
corrections (the hints spec's key count, a v4 line cite).

#### 2026-08-22 — port(groups): `instructions` on the group verbs + both v4 validators, whole

_Versions: core 0.0.615, harness 0.0.537._

`groupCreate` gains an optional `instructions` (`string | null`) and persists
`instructions || null`; `groupUpdate` accepts it too. Per the round's shared
contract with P4.D104.

Both v4 schemas are ported whole rather than just the new field, because the
drift commit lands on the validators and a partial port would be worse than
none. That closes two pre-existing gaps: `group_create` hand-checked a TRIMMED
non-empty name where v4's `.min(1)` runs on the raw string (so `"   "` is a
valid name in v4 and was rejected by v5), and answered `Name is required` where
v4's middleware answers the flat `Validation error` — that sentence only ever
appears inside the `details` array. And `group_update` was a RAW passthrough
patch map with no validation at all (its doc comment claimed
`updateGroupSchema.parse`, which was stale), so it wrote unknown keys v4's
non-strict `z.object` strips. The `details` issue array remains the standing
project-wide deferral.

Fourteen new `groups_routes_equivalence` arms: create with/without, the
`|| null` empty-string normalization, the whitespace name, the empty name 400,
the cap boundary in both directions, a non-string field (refused at v5's typed
wire, which is where v5's equivalent of the ZodError lives), update
set/clear/empty-string, the update cap 400, the `name: null` 400 (`optional()`
but not `nullable()`), a bad colour, and the unknown-key strip probe. Four
mutations proven red-first.

MEASURED, and it refutes the work order's premise: a PUT with `instructions:
""` does reach the store, but `instructions` lives in a markdown file and the
overlay reader is `content === '' ? null : content`, so the round trip answers
`null` either way. The case is renamed to say what it measures.

#### 2026-08-22 — port(prospero): drop the project-instructions whisper section

_Versions: core 0.0.614, harness 0.0.536._

All five of v4 `8f868109`'s removals in the Prospero project-context writer:
`instructions` leaves `ProsperoProjectContext`, the loader stops reading it (v4's
WHY comment carried in its place), the `**Project instructions:**` block leaves
`appendProjectBodySection` along with its separator condition and return
expression, and the term leaves `projectHasContent`. The section is now injected
into the cacheable system prompt every turn, so re-whispering it would only
duplicate it in context.

The `instr-only-no-general` corpus case keeps its name as the retired-section
tripwire: a project whose only former content was instructions now has NO
content, so the whisper must come back empty. Proven load-bearing — restoring
v5's old block reddens that row with the whole whisper on the left and `""` on
the right. `post_office_prospero` and `post_office_writers_tier3` regenerated at
`a6870c5a` and grepped: the literal `**Project instructions:**` appears nowhere
in either NDJSON, or anywhere in the tree.

#### 2026-08-22 — port(prompts): Carina one-off queries carry the standing instructions

_Versions: core 0.0.613._

Mirrors v4 `8f868109`'s Carina insertion: after the scenario, before the "who
is consulting you" card, so the prompt still reads identity → world → who's
asking → what you remember. The template context is HAND-BUILT as
`{char: answerer.name, user: "User"}` — the literal string, never a resolved
user character — which is v4's own shape on this path.

The group leg follows the ANSWERER's memberships, not the chat's roster. The
carina fixture now puts the chat in an instructed project (so all 17 rows carry
the project section) and gives ONE character an instructed group, so a case
answered by anyone else carries the project section alone — which is what makes
the answerer-not-chat rule measurable. Three mutations proven red-first: the
`{{user}}` key resolved from the asker, the membership leg keyed off the chat,
and the section moved after the reference-query card.

#### 2026-08-22 — port(prompts): the standing-instructions slot in the cacheable system prompt

_Versions: core 0.0.612, harness 0.0.535._

`BuildSystemPromptOptions` gains `standing_instructions`, pushed between the
Taboo section and the tool instructions and — unlike Taboo — run through
`process_template`, so a project or group prompt can address `{{char}}`.
Absent, empty, and whitespace-only all build a byte-identical prompt to the
pre-feature layout. `PROMPT_CACHE_STRUCTURE_VERSION` 3 → 4, with v4's history
comment carried; the two sibling wording commits deliberately did not bump.

Threaded at v4's two passing call sites: `build_context` resolves before the
synchronous builder (the same shape as the Taboo read above it), and
`self_inventory`'s `build_prompt_section` includes the full section because it
is substantive conduct guidance a character should be able to introspect. The
two NON-passing sites are verified and commented rather than assumed — the
character-voiced announcer imports the shared builder but never passes the
option, and the greeting head is a separate function entirely.

`system_prompt_equivalence` gains eight standing-instructions rows (absent /
empty / whitespace byte-identity, the position between Taboo and tools, the
`{{char}}`-resolution discriminator, roleplay + tools, and an untrimmed
section pushed verbatim) plus a shape floor so a stale oracle cannot pass by
not carrying them. Three mutations proven red-first.

`build_context_tier3` gains the spine proof: the fixture bakes an instructed
project and three groups whose membership insert order fights the name sort,
and a new op puts the chat in the project. `self_inventory`'s fixture project
and group gain `instructions`. Both threading sites mutation-proven red-first.

#### 2026-08-22 — port(prompts): the version-stamped `compiledIdentityStacks` envelope

_Versions: core 0.0.611, harness 0.0.534._

Ports the compiler half of v4 `a6870c5a`. `chats.compiledIdentityStacks`
becomes `{version, stacks}` keyed to `IDENTITY_STACK_BUILDER_VERSION`: every
write is stamped, reads require STRICT equality (older, newer, and the legacy
bare map all read as "nothing cached", so the read-through path rebuilds with
current wording — newer matters on a downgrade, where a rolled-back build must
not consume stacks a later build wrote), a stale map is DISCARDED on merge
rather than blended into and re-stamped, the drop path CLEARS a stale map
instead of rewriting it back, and an empty map still writes `null` (the null
column is the "nothing cached" state and needs no version). No migration.

`identity_compiler_equivalence` gains eight `participant` rows over eight new
pre-seeded chats — current / legacy-bare / older / newer stamps on the merge
path, and all four drop-path shapes. The seeded stacks are sentinel strings no
builder emits, so a port that merges into or rewrites a stale map shows the
sentinel rather than being inferred. Five mutations proven red-first.

`chat_admin_routes_equivalence`'s minted-id normalizer moves one level down —
only the inner `stacks` keys are minted, so `version` is now a full comparand.
`chat_cast_routes_equivalence` needed nothing (it tokenizes every object key
generically). Both regenerated at `a6870c5a` and confirmed to carry the
envelope.

#### 2026-08-22 — fix(prompts): second-person tool reinforcement + identity-stack person consistency (v4 bug 88)

_Versions: core 0.0.610, harness 0.0.533._

Ports v4 `346e855f` (bug 88) and the server half of `a6870c5a` — the two
wording commits — together, since the goldens they move are the same two.

Bug 88: the last block of the assembled prompt drops its pronoun lookup
entirely and reads `When you use workspace tools, you CALL them — you do not
merely describe calling them.` v5 measurably HAD the bug: a character with no
pronouns recorded ended its prompt on `they CALLS them — they does not`, and
v5's own unit test pinned the third-person string. That assertion is flipped
and widened — a character WITH pronouns and one WITHOUT must now produce the
same final block.

Person consistency: `build_identity_stack`'s aliases, pronouns, and
physical-appearance blocks move to second person (the appearance block also
loses its markdown bullet, and keeps a second-person WRAPPER only — the body
stays noun phrases, shared with the image pipelines); manifesto, personality,
and example dialogues gain referent-fixing wrapper lines. The outward-facing
builders (public identity card, other-participants info, identity
reinforcement) stay third person — their referent is someone else.

New `IDENTITY_STACK_BUILDER_VERSION = 2`, colocated with the builder as v4
colocates it, plus v4's `IDENTITY_STACK_GOLDENS` table byte-copied as a v5
unit test that binds the constant to a hash of the builder's output in both
directions. v5 reproduces v4's registered hash `1408705ab29bb3ba` exactly — a
free cross-implementation check. The compiler that reads the stamp lands next.

`system_prompt_equivalence` regenerated at `a6870c5a`: 65 rows green and both
cache-determinism goldens move to v4's new values (`937ea8197a65d022` /
`bc37032e92411263`) with the transition recorded inline. The regenerated
NDJSON was grepped for all seven changed byte shapes (present) and all four
retired ones (absent).

#### 2026-08-22 — port(prompts): the standing-instructions module (project + group `instructions`)

_Versions: core 0.0.609, harness 0.0.532._

Ports v4 `lib/chat/context/standing-instructions.ts` (`8f868109`) as
`quilltap_core::standing_instructions`: `resolve_standing_instructions`
(project by `chat.projectId`, groups by the RESPONDING character's
memberships — never the chat), `render_standing_instructions_section`
(byte-exact `[STANDING INSTRUCTIONS]` preamble, `## Project Instructions —
<name>` / `## Group Instructions — <name>` headings, `\n\n` block joins),
and the `resolve_standing_instructions_section` one-shot. Nothing is
injected yet — the builder slot and the call sites land in the next units.

Group sources sort by `localeCompare(name)` then `localeCompare(instructions)`
— v4's doc comment says the tie-break is "(then id)", but its code tie-breaks
on `instructions`; the CODE is what is ported. Empty/whitespace instructions
contribute nothing at both layers, so an instance that never touches the
feature builds a byte-identical prompt (the Taboo contract). Every lookup
fails soft per v4's three try/catch sites.

New differential `standing_instructions_equivalence` (14 resolve + 8 render
rows) over a new `/tmp`-built fixture pair whose membership INSERT order
deliberately fights every sort. Five mutations proven red-first: byte order
instead of ICU (`apple` vs `Banana`), a reversed name sort, the dropped
`instructions` tie-break (two groups both named `Mirror`), the dropped
resolver trim, and groups-before-project.
#### 2026-08-22 — docs(porting): the P4.55 verification gate record

_Docs-only change._

The lane's gate, for the unifier: fmt clean, clippy clean on both feature sets,
443 test binaries / 2,261 passed / 0 failed with the eight-variable oracle
env block, the six families re-run by name with zero SKIP and all 26 new arms
green and present in their freshly regenerated NDJSONs, and a clean release
build. No committed fixture changed, so no sibling oracle is invalidated.

#### 2026-08-22 — docs(porting): P4.55 closes — the Tier 3 deferrals recorded by name

_Docs-only change._

The work order's status header flips to CLOSED (Tier 1 and Tier 2 both landed
whole), and the two items the lane deliberately did not take get their own
phase-4 section: the data-retention present-`null` state collapse, which is
confirmed divergent but whose fix lives in files another lane owned and whose
harness leg bypasses the wire it would need to pin; and the groups-side
cleared-null pin, which inherits the projects-side verdict by construction but
rides the next round.

#### 2026-08-22 — fix(profiles): a non-string apiKeyId or baseUrl no longer vanishes on update

_Versions: core 0.0.612, harness 0.0.535._

All three profile-update handlers (connection, image, embedding) read `apiKeyId`
as `if null … else if as_str …` with no else, so a present non-string was
silently dropped and the PUT answered 200. Measured against v4: it falls into
`findApiKeyById(<non-string>)`, which answers null for a number (no id is
spelled that way) and null for an object or array (the driver's binder throws
and `safeQuery` swallows it), so both cases are a 404 `API key not found`.

The sibling `baseUrl` read is JS falsiness — `baseUrl || null` — not a string
check. v5's `as_str()` filter collapsed every non-string to null, so a truthy
non-string silently cleared the column. Measured against v4: it assigns the
value verbatim, the row validation rejects it, and the route answers its fixed
500 with nothing written. Both reads now match, with the falsy arms (`""`,
`null`, `false`, `0`) still clearing as before.

Nine new arms across the settings-routes, image-profiles-routes and
embedding-profiles-routes families, each proven red against the old behavior.
The embedding-profiles recipe is repaired on the way: its regen wrote one
NDJSON path and its run line read another, and the .test.ts header carried a
parenthetical that is a bash syntax error, so the family could not be
regenerated through the sweep driver at all. The stale comment claiming the
embedding-profile PUT echo drops cleared nulls is corrected — it echoes them
explicitly, and has since the P4.9H2A review.

#### 2026-08-22 — fix(projects): validate the update body through v4's updateProjectSchema

_Versions: core 0.0.611, harness 0.0.534._

`project_update` passed the raw request body straight to the repository with no
validation: `{"allowAnyCharacter": "yes"}`, a 101-character `name`, an explicit
`null` on the non-nullable `backgroundDisplayMode` — all accepted, answered 200.
v4 runs `updateProjectSchema.parse(body)` and hands the repository the parsed
data, so an invalid field is a 400 `Validation error` and an unknown key is
stripped.

All fourteen fields ported constraint by constraint, in schema-declaration
order, with the `.nullable()` column carried per field so a cleared
`description` still clears while a null `backgroundDisplayMode` refuses. The
existence check runs first, as in v4, so a missing project still answers 404
even for a garbage patch. Five new arms in the projects-routes differential;
the three invalid ones proven red against the old passthrough.

The same commit settles the P4.D85 cleared-null question on the store-backed
side: v4's `update` answers `_update`'s in-memory merge overlaid, v5 re-reads,
and the new `update_clear_description` arm measures the two echoes agreeing.
No `store_backed.rs` change was needed.

#### 2026-08-22 — fix(autonomous-rooms): refuse an invalid update-settings patch instead of dropping the field

_Versions: core 0.0.610, harness 0.0.533._

`parse_settings_patch` was deliberately lenient: a present-but-wrong-typed or
out-of-range field was silently dropped and the request answered 200. A bogus
`runVisibility`, a negative or fractional cap, a 400-character `title` all
vanished without a word — and a valid field riding alongside an invalid one
still landed. v4 runs `updateSettingsSchema.parse` before the service call and
answers 400 `Validation error`, writing nothing.

The parser is now fallible across all ten fields, each constraint taken from
v4's schema: `.int().positive()` on the four integer caps, `.positive()` alone
on the spend cap, the three-member `runVisibility` enum, `title` at 300 UTF-16
units, `scheduleCron` at 120, and both booleans strict. `title` is `.optional()`
rather than `.nullish()`, so a present `null` is a refusal, not a clear. Six new
arms in the autonomous-rooms differential including a writes-nothing composite
over a room with stored caps; all seven comparands proven red against the old
leniency.

#### 2026-08-22 — fix(memories): validate the housekeeping + extraction-limits config bodies before writing

_Versions: core 0.0.609, harness 0.0.532._

The `memoryHousekeepingConfigSet` and `memoryExtractionLimitsSet` verbs merged
whatever the body carried into the stored settings bag and persisted it: a
`"yes"` for a boolean, a negative `perCharacterCap`, a string where a number
belongs — all merged, written, answered 200. v4 `safeParse`s
`housekeepingConfigSchema` / `extractionLimitsConfigSchema` before it reads
anything and answers 400 `Validation error`, writing nothing.

Both verbs now validate first, on the template the recall verb already used.
The bespoke non-object sentences ("Invalid housekeeping config body" /
"Invalid extraction limits body") are gone — a non-object body is Zod's own
root-level `invalid_type` and takes the same `Validation error` path. Six new
arms in the memories-config differential (two invalid + one writes-nothing
composite comparing the stored bag per verb), all eight comparands proven red
against the old lenient behavior.
#### 2026-08-22 — fix(prompts): the shared prompt-field label host is a block

_Versions: SPA 0.5.543._

An Angular custom-element host is `display: inline` by default, and the React
component this ports has no host element at all. In the appearance tab the
header is a DIRECT child of a `space-y-4` stack, so an inline host silently
loses its gap — `margin-top` does not apply to an inline box. `host: { class:
'block' }` fixes it everywhere at once; `display: contents` would read like
v4's zero-host render but makes `space-y-*`'s `> * + *` margin land on a
boxless element and vanish. Same class of defect as dogfood finding #97's
`qt-tab-view`. Spec-pinned and mutation-proven.

#### 2026-08-22 — feat(groups): the Group Instructions editor in the group detail view

_Versions: SPA 0.5.542._

P4.D104 unit 3 — v4 `8f868109`'s client half. The routed group editor gains
a Group Instructions markdown field, third in the form between Description
and Color exactly where v4 puts it, at v4's `minHeight="14rem"`, headed by
the shared prompt-field label over `PROMPT_FIELD_HINTS.groupInstructions`
with `optional`. Load reads `g.instructions || ''`; save sends
`instructions || null` — the server's update path is a validated
passthrough, so an emptied editor would otherwise persist `""`, and the
client is what normalizes, as v4's does.

The TS contract mirrors the Shared contract: `instructions` on
`GroupCreateRequest`, on `GroupUpdatePatch`, and on the group row DTO. The
row DTO declares it optional, exactly as v4's own shared `Group` type does —
one type serves both the list read and the detail read, and only the detail
read is contractually obliged to carry it.

The group CREATE dialog gains nothing: v4 did not touch its own.

Specs cover seeding from the loaded group, the header text drawn from the
hints table, the field's position in the form, saving an edited body, the
cleared-to-null normalization, and a group that carries no instructions at
all. Both the null normalization and the field placement were
mutation-proven. A gated Playwright beat (type → save → reload → clear →
assert the wire carries null) waits on `P4D103_SERVER_LANDED`, which the
unifier flips once the server half lands.

#### 2026-08-22 — feat(prompts): migrate every prompt editor onto the shared field label

_Versions: SPA 0.5.541._

P4.D104 unit 2 — v4's `a6870c5a` sweep mirrored across every v5 prompt
editor. Migrated: the character edit Details tab (seven fields), the
character create page (eight, incl. the singular Scenario), the
system-prompt modal's Content field (`required` plus the Markdown /
placeholder suffix), the roleplay-template modal's LLM Prompt (`required`
plus the placeholder suffix), the project Settings card's Project
Instructions, and the appearance tab, which gains ONE physicalDescription
note above all five prompt variants rather than a helper per field. The
create page's disabled "Import Template" control moves into the label row's
projection slot, v4's `actions`.

This CONVERGES the character create and edit forms, which is the stated
reason v4 wrote the component: the two had drifted apart in their
hand-rolled hint copy, and v5 had transcribed both drifted versions
faithfully. Every replaced string is deleted, not shadowed.

The scenarios block keeps its custom header — v4 deliberately left that one
hand-rolled, since the "+ Add Scenario" control and the array editor below
are not a labelled field — and gains only what the commit gave it: "the
stage, never the actor" folded into the helper and a `Written as:` line from
the shared scenario example. Fixed on the way: v5's helper had dropped v4's
"Stored in the vault's Scenarios/ folder." clause.

Pinned by a new rendered-DOM spec that asserts every migrated header is
drawn from the hints table, that create and edit render byte-identical
headers for their seven shared fields, and that none of the eleven retired
sentences survives anywhere in the rendered output. Mutation-proven:
restoring one hand-rolled helper reddened both the field assertion and the
convergence assertion.

Deferred loud, unchanged: the roleplay-template modal's "Draft formatting
instructions" button (v4 puts it in the label row's actions slot; v5 has
never had the control), and v5's "Import Template" stays disabled.

#### 2026-08-22 — feat(prompts): the shared prompt-field label + single-sourced field hints

_Versions: SPA 0.5.540._

P4.D104 unit 1. Ports v4's `components/prompt-fields/` pair (commit
`a6870c5a`): `qt-prompt-field-label`, the header-only shared component for
prompt-bearing fields (label line, helper paragraph, `Written as: <em>`
worked example, the `" (Optional)"` suffix and destructive `*`, an
`ng-content` slot standing in for v4's `actions` ReactNode), and
`PROMPT_FIELD_HINTS`, the eleven-key hint table transcribed byte-for-byte
from v4 — typographic apostrophes included.

The label component renders the header only and never wraps the editor, so
it drops above any existing input without touching its state wiring. The
whole label line is one computed string because Angular's default
whitespace collapsing would otherwise insert a space where v4's JSX
concatenates.

The hints table is pinned by a v4-client-oracle parity spec whose
expectation rows were emitted from v4's real module, so a drift edit on
either side of any of the 33 strings goes red; a mutation on one helper was
run red-then-green. No call site migrates yet — that is unit 2.

#### 2026-08-22 — docs(porting): work orders for the prompts-trio drift round (P4.D103 ∥ P4.D104 ∥ P4.55)

_Docs-only change._

Three work orders for the next round, planned against v4 `a6870c5a` (the
prompts trio — the drift the `4cb1035e` unification predicted).
`p4.d103-prompts-trio-server.md`: the standing-instructions module and its
system-prompt slot (project/group `instructions` between the Taboo section
and tool instructions, `PROMPT_CACHE_STRUCTURE_VERSION` 3 → 4), the bug-88
second-person tool reinforcement, the identity-stack person-consistency
wording, and the `compiledIdentityStacks` version-stamped envelope with the
golden-table guard — plus the groups verbs' `instructions` acceptance with
v4's validators ported whole. `p4.d104-prompts-trio-spa.md`: the
group-instructions editor, the shared prompt-field label component + the
transcribed field-hints table, the migration sweep across every v5 prompt
editor, and the gated round-trip beat. `p4.55-merge-verb-silent-keep.md`:
the `c8a3cf77` LEAD resolved to a measured site table — two unfixed
memories merge-verbs that persist garbage, the autonomous-rooms
settings patch's self-documented leniency, the projects update's missing
schema parse, the three missing-`else` `apiKeyId` sites, and the
store_backed cleared-null echo question — each fix red-first with
invalid-payload oracle arms. The data-retention present-null divergence is
recorded but deferred by file ownership. Shared contract (the group
`instructions` wire field + the `P4D103_SERVER_LANDED` beat gate) pinned
verbatim in both drift orders.

#### 2026-08-22 — unify: the `4cb1035e` image + NanoGPT drift round (P4.D100 → P4.D101 stacked ∥ P4.D102)

_Versions: core 0.0.608, harness 0.0.531, host 0.0.77, SPA 0.5.539._

All three lanes unified; the oracle baseline moves `12fe3e6f` → `4cb1035e`.
The honest image `list-models` verb replaces its refusal end to end (live
per-provider model discovery for all five image plugins, source/fetchError
labels, cache-only-live); the image-download seam lands and Z.AI image
generation becomes real (v5 measurably had v4's URL-only zero-byte bug); any
`gemini*` id now routes to generateContent; and NanoGPT arrives whole as the
tenth bundled provider — chat with the flat `reasoning_effort` allowlist and
the dual `delta.reasoning`/`reasoning_content` dialect plus bug 87's
prose-echo guard, images over the shared download seam, embeddings with the
catalogue pinned against v4's real plugin, and a thinking-turn rule through
the P4.D97 machinery. SPA: the Fetch Models control with v4's exact label
strings, the Z.AI/NanoGPT provider entries and Default Size panels, the
NanoGPT embedding surface and badge. A real v4 bug found to file upstream:
OpenRouter image discovery reads wire keys its own SDK's zod strips, so every
keyed listing throws (v5 reproduces, convergence-tripwired). Unification
wires: both gated beats flipped live; the unified gate caught and fixed a
cross-lane oracle blind spot (the routes case's plugin list) and a
first-live-run beat gesture defect (redesigned around an offline list-order
discriminator, mutation-proven). Gate: ten families fresh at the `4cb1035e`
pin zero SKIP; manifests regenerated byte-identical; 443 test binaries /
2,261 / 0; clippy both feature sets; release build; ng 338 files / 5,016;
full Playwright 235/235 zero skips. Round record: `status-log.md`.

#### 2026-08-22 — docs(porting): the P4.D101 tier-2 docs — mirror refresh + the help-doc bank

_Docs-only change._

The `docs/v4` mirror is refreshed for what the three NanoGPT commits actually
touched and v5 actually mirrors: `CHANGELOG.md`, `developer/bugs.md`, and the new
`developer/bugs/fixed/bug-87-nanogpt-reasoning-echo.md`.

The four help docs the order listed are banked for `p4.9i2` rather than ported.
They live at v4's top-level `help/*.md`, not under `docs/`, so they sit outside
the mirror entirely and no refresh applies to them; and v5 has no ported
help-doc content surface to port them into — the P4.D77 `help_doc_chunks`
substrate landed, but the Guide client half is the standing `p4.9i2` bank. Their
names and NanoGPT hit counts are recorded so that lane can find them.

The pricing measurement the order asked for is recorded with the unit-2 census:
v4 has no NanoGPT pricing row anywhere in `lib/`, so the pricing fetcher and its
fallback table are NO-PORT, and the manifest ships `"pricing": {}` — which the
generator emitted off the plugin without being told to.

#### 2026-08-22 — port(providers): the NanoGPT image arms over P4.D100's seam (v4 `781fc420`, P4.D101)

_Versions: core 0.0.608, harness 0.0.531._

Generation, discovery, orientation and auth for NanoGPT's OpenAI-compatible
images route, built on the download seam P4.D100 landed.

`response_format: "b64_json"` is PINNED on every request, carrying v4's why —
"NanoGPT defaults to b64_json already; pin it so a future default change
upstream cannot silently hand us URLs." Unlike the OpenAI builder there is no
`gpt-image` exemption, which the corpus proves with a `gpt-image-1.5` row. `size`
rides verbatim and only when supplied — v4 casts it without validating, so none
of the OpenAI path's size coercion applies; `seed` rides only when set; an absent
model becomes `hidream`, NanoGPT's own server-side default made explicit.

P4.D100's `download_zai_images` is generalized to `download_url_images`: both
plugins carry the identical loop and differ only in their two error sentences, so
the provider now selects the wording and nothing else. Z.AI's bytes are unchanged,
which its own corpus rows keep proving.

Discovery is a raw fetch to `/image-models`, so a non-ok status is NanoGPT's own
`NanoGPT image-model listing failed: HTTP <status>` rather than an SDK error. The
filter is the capability FLAG and strictly `=== true` — the listing also carries
edit-only and upscale-only entries — and the curated six are unioned in and
sorted, so the arm needs no empty-throw.

Corpus: image-dialects 65 → 82 rows, all 65 of P4.D100's byte-identical. Three
mutation proofs: dropping the b64 pin reddens; swapping in Z.AI's download
sentence reddens with the exact wording diff; and relaxing the capability filter
from `=== true` to truthy reddens on the planted `image_generation: 1` row.

Two harness generalizations were needed and both were P4.D100 shapes rather than
new ones: the `model` column is now optional (the `default_model` case
deliberately sends none), and the Z.AI-named download leg is provider-parameterized.

⚠ The regen script's concat list is separate from its run list, so adding a `run`
line alone produced a green regen with NanoGPT silently absent. Both are updated.

#### 2026-08-22 — port(providers): the NanoGPT embeddings arms (v4 `781fc420`, P4.D101)

_Versions: core 0.0.607, harness 0.0.530._

NANOGPT joins the embedding-profile provider list with no code change — the list
is manifest-driven off `capabilities.embeddings`, so the manifest landing was
enough; a unit assertion pins that it is really there and that BUILTIN stays
last.

The wire is the OpenAI-compatible `/embeddings` route with two real differences:
the base url defaults to NanoGPT's gateway, and the request carries a
`User-Agent` on both the single and batch routes where v4's single-embedding
OpenAI path omits it entirely. The runtime dispatch gains its `NANOGPT` branch,
returning the REQUEST model like the OpenAI path does.

**The differential caught a defect inspection would not have.** The first
implementation built NanoGPT's error sentence by wrapping `openai_error_message`,
which already carries its own prefix — producing `NanoGPT embedding failed:
OpenAI embedding failed: Invalid API key`. Fixed at the root: the bare
`error.error?.message || statusText` extraction is now a shared
`embedding_error_detail`, and neither provider's prefix can absorb the other's.

The seven-model catalogue is transcribed from `models.ts`, and — better than the
existing pattern — it is PINNED: `embedding_wire_equivalence` gained a
`catalogue` row driving v4's real `plugin.getEmbeddingModels()`. Extending the
same row to the other four providers was free, so OPENAI / OLLAMA / OPENROUTER /
BUILTIN are now byte-pinned against v4 too, where they had only been asserted by
count. All four were already correct. Mutation-proven: a one-word description
change or a dropped model row reddens the catalogue arm by name.

The `fetch-models` refusal note widens to name NanoGPT and record what the live
verb will owe — `GET /api/v1/embedding-models` with fallback-not-throw on a
non-ok status, an empty list, or a thrown fetch, which is the deliberate
opposite of NanoGPT's image listing.

Corpus: embedding-wire 12 → 23 rows.

#### 2026-08-22 — port(providers): the NanoGPT reasoning dialects + bug 87's echo guard (v4 `d5830439` + `4cb1035e`, P4.D101)

_Versions: core 0.0.606, harness 0.0.529._

`Flavor::NanoGpt` reads `delta.reasoning` with `delta.reasoning_content` as the
legacy fallback — the `??` precedence, which neither `OpenRouterRaw` (reasoning
only) nor the SDK flavors (reasoning_content only) have. Its terminal
`raw_response` is the OpenAI-SDK shape and still carries the accumulated run
under the LEGACY `reasoning_content` key: v4's `d5830439` changed which field is
READ off the wire and deliberately left the synthesized key alone. NanoGPT emits
a usage object even with no usage frame, derives no cache usage, and never sets
`raw_provider_usage`.

v4 bug 87's prose-echo guard is ported as decoder state. NanoGPT's gateway
sometimes re-emits the whole answer down the reasoning channel after the content
stream ends, which would repeat the reply inside a thinking fold. A reasoning run
is HELD while it is still a verbatim prefix of the prose streamed so far;
divergence commits it in full as a single chunk, and a run still mirroring the
prose at stream end is discarded from the live chunks, the final chunk, and the
`raw_response`. The guard only arms while nothing has been committed and prose
has already started, so ordinary pre-content reasoning never touches it. The
stream-end arm yields no live chunk, matching v4.

The non-streaming twin drops a run exactly equal to `msg.content ?? ''`. Exact
equality, not a prefix — a near-miss is real reasoning and is kept, which the
corpus pins.

`is_openai_sdk` and a new shape predicate were collapsed into one
`has_sdk_raw_response`, since both call sites wanted the same three flavors.

Five stream fixtures (both dialects, tool-fragment assembly, the split-chunk
echo, the diverging run), nine response bodies (both dialects, the `??`
precedence with both fields present, both echo channels, the near-miss, tool-call
normalization, no-usage), and the thinking-turn corpus gains NanoGPT's real rule
— the corpus's FIRST multi-value enabled list, and the shape where the empty
string is in neither list so "(model default)" falls through to the model's
`thinksByDefault` habit. Corpora: streams 11 → 16 cases, response bodies 37 → 46
rows, thinking-turn → 1,560 rows; all pre-existing rows byte-identical.

Four mutation proofs, each reddening by case name: disarming the streaming guard
(`nanogpt-echo-dropped` 5 chunks vs 3), dropping the main-endpoint field
(`nanogpt-reasoning-main` 3 vs 5), disarming the non-streaming guard (both
`reasoning-echo-dropped` rows), and reversing the `??` precedence
(`reasoning-both-precedence`).

#### 2026-08-22 — port(providers): the NanoGPT chat wire + the switch-table census (v4 `781fc420`, P4.D101)

_Versions: core 0.0.605, harness 0.0.528._

`build_nanogpt_body` is v4's `buildBody` verbatim — one body function for both
modes, so the streaming and non-streaming wires cannot drift. Defaults 0.7 /
4096 / 1; `stream_options.include_usage` on the streaming path only; `stop`;
`tools` with `tool_choice` defaulting to `"auto"` whenever tools are present;
both `response_format` kinds; the cache key on `user` (NOT DeepSeek's
`user_id`) and only when non-empty. The five-key profile allow-list is FLAT:
NanoGPT does not override `normalizeProfileParam`, so `reasoning_effort` is a
top-level key rather than folded into `chat_template_kwargs` the way the
OpenAI-Compatible endpoint folds it. Reasoning is never echoed back, and leading
system messages are NOT folded — NanoGPT is a hosted gateway with its own
`formatMessages`, and folding would change the bytes it receives.

`ChatFlavor::NanoGpt` carries the non-streaming reasoning read: `message.reasoning`
with `message.reasoning_content` as the legacy fallback — a precedence no other
flavor has — minus v4 bug 87's gateway echo, where a reasoning run exactly equal
to the content is dropped. Tool calls go through `normalize_oac_tool_calls`,
confirmed line-by-line to be the OAC base's `normalizeToolCalls` that v4's
NanoGPT provider actually calls, not DeepSeek's `extract_openai_tool_calls`.

The models-list fetch unions the manifest's `fallbackModels` in and removes its
`imageGenerationModels`, then sorts — one home for the curated lists rather than
a second hardcoded copy. Measured on the way: the plugin's try/catch fallback is
unreachable for a wire failure, because the OAC base's own `getAvailableModels`
swallows every transport error and returns `[]`; the union produces the same
sorted catalogue the catch would have.

The corpus grows 269 → 307 (19 cases × both modes) and all 269 pre-existing rows
are byte-identical. Mutation-proven: folding `reasoning_effort` into
`chat_template_kwargs` reddens `request_builder_equivalence` with the exact
flat-vs-folded diff.

**The switch-table census refutes four of the order's named sites.** `NANOGPT`
appears in v4's entire `lib/` tree exactly ONCE — the embedding-profile enum. It
is in none of `DEFAULT_CONTEXT_BY_PROVIDER`, `LEGACY_RECOMMENDED_CHEAP_MODELS`,
`PROVIDER_NAME_SUPPORT`, or `PROVIDER_ATTACHMENT_CAPABILITIES`, and `Provider` is
`z.string()` so no type forces an entry. Those are v4's pre-plugin fallbacks; a
plugin-era provider is served by the registry, which v5 consults first. Adding
rows would have been a v5-invented divergence, so a new guard test pins the
absence and proves the registry still answers 131072.

#### 2026-08-22 — port(providers): the NanoGPT manifest through the generator (v4 `781fc420` + `d5830439`, P4.D101)

_Versions: core 0.0.604, harness 0.0.527._

NanoGPT joins the built-in provider registry as a tenth manifest, generated
rather than hand-written. The generator's `PROVIDERS` table gains the
`qtap-plugin-nanogpt` row with its wire augmentation (base URL
`https://nano-gpt.com/api/v1`, `/chat/completions` + `/models`, bearer auth,
the chat-completions SSE decoder, no request transform); everything else —
capabilities, config requirements, the `reasoning_effort` options schema, the
`thinkingTurnRule`, the ten catalogue rows, the `:thinking` thinking facts,
message format, cheap models, chars-per-token, and the 131072 default context
window — is read off the built plugin, so the committed `nanogpt.json` is a
transcription that cannot rot away from v4.

`resolveImageGenerationModels` needed a real extension to reach NanoGPT's six
image ids. Grok and Z.AI declare `supportedModels` inside `image-provider.ts`,
where the existing reader looks; NanoGPT imports the list from `models.ts`, so
the reader now follows a same-dir relative import when the const is not
module-local, and still exits loudly and by name on any shape it does not
recognize. Hand-copying the six ids into the augmentation table would have been
the `imageGenerationModels` rot of P4.39 and the google-auth rot of P4.D78 for a
third time.

The manifest is APPENDED to `BUILT_IN_MANIFEST_JSON` rather than slotted
alphabetically. Both `provider_registry_equivalence` (the `names` row) and
`providers_listing_equivalence` (a positional zip) compare registration order,
so appending leaves all nine pre-existing rows byte-identical on both sides; the
two oracle cases' `PLUGIN_DIRS` lists carry the same append. The nine committed
manifests regenerate byte-for-byte at the pin, which is the proof the generator
change is additive.

`providers_listing_equivalence`'s exactly-two-thinking-rules shape guard moves
to three (deepseek + ollama + nanogpt) — a designed tripwire firing as designed,
not a weakened assertion — and the options-schema floor moves 8 → 9. The
NanoGPT row is mutation-proven compared: perturbing one `enumValues` label in
the committed manifest reddens the listing differential by name.

#### 2026-08-22 — port(images): the honest `list-models` verb (v4 `ca22ec45`, P4.D100)

_Versions: core 0.0.603, harness 0.0.526, host 0.0.77._

`imageProfileListModels` was a loud typed refusal; it is now v4's honest Fetch
Models action. `source` is `provider` ONLY when the provider's API was actually
queried and answered, otherwise `builtin` (the plugin's curated list) with the
live-fetch reason in `fetchError` — omitted, never null, when there is none.
The flow is v4's exactly: a falsy `provider` is 400 `Provider is required`; an
unavailable one is 400 `Provider {provider} is not available`; a dangling
`apiKeyId` is 404; and every other failure collapses into the outer catch's 500
`Failed to fetch models`. The legacy `GOOGLE_IMAGEN` alias resolves to GOOGLE's
plugin for the list, while the response and the cache key echo the RAW provider
string.

Only genuinely live-fetched lists are cached in `provider_models` (capability
`image`, `displayName` equal to the id) — a built-in list would masquerade as
provider-confirmed on later reads. The write is best-effort, warned and
swallowed exactly as v4's is, so a cache-layer problem can never turn a
successful fetch into an error page.

The discovery crosses a new `image_discovery` engine seam, wired LIVE in
`quilltap-host` over the W4.7f provider (it needs only the HTTP client and the
image-download seam, so it is built independently of the spine bundle). An
unassembled seam answers a loud not-assembled refusal — the
`image_generation` precedent — rather than silently serving a keyless list,
which is the very thing this drift set out to stop.

The routes differential grows seven arms over the v4 oracle, with the
`provider_models` table as a measured comparand on four of them. Two oracle
repairs were needed to make them honest: `jest.setup.ts` globally mocks
`@/lib/llm/plugin-factory`, so `createImageProvider` answered `undefined` for
every provider and the action 500'd on an `undefined.supportedModels` read (the
same class as the empty-provider-registry trap; un-mocking moved no other
recorded row); and the committed fixture predates `provider_models`, which v4
creates lazily and v5's boot ensure creates at startup, so the oracle now emits
v4's own `CREATE TABLE` text and the harness applies it — otherwise "nothing
cached" would be true on the v5 side for the wrong reason and the
cache-only-live rule would be vacuously green. A named stale-oracle guard fails
loudly on a pre-P4.D100 regen. Five mutations proven red: caching a built-in
list, dropping `fetchError`, caching under the wrong capability, echoing the
resolved rather than the raw provider, and losing the built-in fallback.

The P4.D33 bank note is retired with a record of what the measurement found
beyond it; it still stands for the `p4.9h` embedding-profiles surface.
`docs/v4/developer/API.md` is refreshed from the pin.

#### 2026-08-22 — port(images): keyed model discovery + the Z.AI image-download seam (v4 `ca22ec45`, P4.D100)

_Versions: core 0.0.602, harness 0.0.525, host 0.0.76._

### The five providers' keyed model discovery

v4's `ca22ec45` gave (or hardened) a keyed `getAvailableModels(apiKey?)` on all
five image plugins — openai, google, grok, z-ai, and openrouter. The contract
is uniform in one respect and deliberately asymmetric in two: no key returns
the plugin's curated static list and makes NO request at all; a live failure
throws on every provider; an empty result throws on openai / google / grok /
openrouter but cannot arise on z-ai, whose union with two static ids makes
empty unreachable.

Ported into `model/image_dialects.rs` as the same three-part split the generate
dialects use — `build_models_request`, `parse_models_page`, `finalize_models` —
composed by `RealImageProvider` over the injected `WireTransport` through a new
`ImageModelDiscovery` trait (plus an object-safe `ErasedImageDiscovery` for the
dispatch engine). `supported_image_models` transcribes the five plugins'
`supportedModels`, which are NOT the manifests' `imageGenerationModels`: google
orders imagen first and openrouter's entries differ outright, and the route
reads the plugin's list.

Endpoints, auth headers, filters, paging, dedup, sort and every error sentence
are byte-faithful: OpenAI's `/v1/models` with `/^(dall-e|gpt-image)/`; Google's
`pageSize=1000` `x-goog-api-key` page loop with the imagen-`predict` /
gemini-with-"image"-`generateContent` pair; xAI's dedicated
`/v1/image-generation-models` accepting both the `models` and `data` top-level
keys with aliases riding along as selectable ids; Z.AI's
`/^(cogview|glm-image)/i` — the exact complement of its chat filter — unioned
with the two documented ids; and OpenRouter's paged `models.list()`. The two
OpenAI-SDK providers' thrown messages (`{status} {error.message}`, falling back
to the raw body) are reconstructed because the host wire hands back a status
where v4's SDK threw; both shapes were measured against the real SDK.

**A v4 bug, faithfully reproduced and pinned.** OpenRouter's discovery reads
three WIRE key names off an object `@openrouter/sdk`'s zod has already
rewritten. `Model$inboundSchema` is a `z.object`, so it STRIPS model-level
`output_modalities` and `supported_generation_methods` outright, and remaps the
architecture's genuine `output_modalities` to `outputModalities` (plural) where
v4 reads `outputModality` (singular). All three acceptance arms therefore read
`undefined`, and at `d5830439` every keyed OpenRouter Fetch Models call throws
"OpenRouter listed no image-output models for this API key". Measured at the
pin with a schema-valid payload carrying all four signals; the port reproduces
the SDK projection so v5 answers identically, and the
`openrouter/models_live_every_signal` corpus row is the tripwire that fires
when v4 fixes the read. Same class as the P4.D33 bank note and dogfood #24.

The committed image-dialects corpus grows 24 `kind:'models'` rows recorded from
v4's REAL plugin methods at the pin, carrying every request (method, URL, and
full header set) and each page's canned answer. The differential replays them
through the whole composed path over a canned transport keyed on the exact
request signature, header-subset-asserts what v5 builds, and additionally
exercises `finalize_models` in isolation. The corpus floor now asserts SHAPE —
a no-key row and a keyed row per provider, plus eleven named contract arms —
rather than a hand count. Mutation-proven: dropping the SDK projection, grok's
dedup, openai's sort, google's `image`-in-id conjunct, grok's endpoint, and
google's auth header each go red.

One blinded comparand is named rather than papered over: OpenRouter alone
neither sorts nor dedupes, and the corpus cannot see it while v4's own bug
empties every list, so that rule is pinned by a direct unit test instead.

### The image-download seam and Z.AI URL→base64

Z.AI's Images API answers with URLs (valid roughly 30 days) while every
Quilltap consumer — the chat handler, the avatar and story-background jobs,
`tools::generate_image` — reads only the base64 `data`. v5 measurably had the
resulting bug: `parse_zai` kept both fields, and the consumer decoded
`img.data`, so a URL-only Z.AI row produced zero bytes. v4's `ca22ec45`
downloads each URL inside the provider; this ports that, in the same place.

The download crosses a NEW narrow seam, `model::image_bytes::ImageBytesFetch`,
rather than a widened `WireTransport`: the wire seam's response carries a
`String` body and no headers, and an image download needs raw bytes plus the
`content-type`. Widening the wire for one caller would have touched every
dialect. `RealImageProvider` gained a second type parameter for it, defaulting
to a `NoImageBytesFetch` that fails loudly by name; every host construction
site that can reach a Z.AI profile — the avatar job, the story-background job,
the generate-image tool runner, and the avatar-preview renderer — is wired to
the new `ReqwestImageBytes`, a bare GET with no headers of ours (v4 issues
`fetch(img.url)` with no init object at all, and the URL is a signed link the
provider just handed us).

v4's per-image rules are carried exactly: keep an existing `b64_json` and make
NO request; otherwise download, treat a non-2xx as `Failed to download Z.AI
image: HTTP {status}`, default the mime type to `image/png` and let the
response's `content-type` override it only when it starts with `image/`,
truncated at the first `;`; and reject an entry carrying neither field with
`Z.AI image entry carried neither base64 data nor a URL`.

The recorder now scripts a distinct binary answer for a provider's follow-up
request, so the corpus's z-ai rows drive the whole composed `generate_image`
against v4's real plugin: the URL-only conversion, an entry with both fields
(no download), a non-`image/` content type, a 404 download, and an entry with
neither field. The differential asserts v4's download is a bare GET with an
empty header set, and a row with no scripted download that attempts one fails
loudly. Six mutations proven red, one per rule. The pre-existing
`zai_keeps_url_and_b64` parse pin is untouched — the conversion sits above it.

#### 2026-08-22 — port(images): route any `gemini*` id to generateContent (v4 `ca22ec45`, P4.D100)

_Versions: core 0.0.600._

v4's `ca22ec45` widened `isGeminiImageModel` so ANY id beginning `gemini`
routes to the Gemini `:generateContent` endpoint, with the original exact /
prefixed / substring arms over `GEMINI_IMAGE_MODELS` preserved behind it.
Live-fetched ids the honest Fetch Models list now surfaces (e.g.
`gemini-2.0-flash-preview-image-generation`) must not fall through to the
Imagen `:predict` endpoint, which serves only `imagen-*` models. The
predicate drives both the build and the parse side of the Google dialect.

Pinned by a new `google/gemini_live_fetched_id` row in the committed
image-dialects corpus, recorded from v4's REAL `GoogleImagenProvider` at
the `d5830439` pin, plus a direct unit test over the predicate. Red-first
proven: with the pre-widening predicate the row builds
`…/gemini-2.0-flash-preview-image-generation:predict` where v4 builds
`:generateContent`.

The z-ai `url_only` corpus row is deliberately held at its pre-drift
recording in this commit — v4's Z.AI URL→base64 download (the same drift
commit) lands with the bytes seam in a later unit of this lane, and the
recorder needs a distinct download response before that row can be
regenerated honestly.
#### 2026-08-22 — port(images): the Z.AI and NanoGPT size panels, the NanoGPT options verify-spec, and two gated beats (v4 `ca22ec45` + `781fc420` + `d5830439`, P4.D102)

_Versions: SPA 0.5.539._

The image-profile modal's structured parameters editor gains its first two
cases: v4's Z.AI and NanoGPT Default Size selects, with the eight and seven
sizes, their labels, and both footnotes transcribed verbatim. Providers v4 has
no size case for keep v5's JSON textarea stand-in.

The size change spreads the existing bag rather than rebuilding it, so a
parameter the panel does not render survives being edited — the one behaviour a
structured editor most easily breaks, and mutation-proven here. The select
assigns post-render, so an off-list stored size leaves it blank as React does.

NanoGPT's options group is verified, not reimplemented: a spec drives the
existing schema-driven panel with the contract's schema and asserts the group,
the label, and all seven enum values in order, confirming v4's position that no
bespoke editor code was needed. The thinking-turn client twin gains v4's
partition arms — every non-blank value classified by exactly one side, the blank
by neither, and the blank deferring to the model's own habit.

Two e2e beats land gated on the sibling server lanes (ACTIVATE-AT-UNIFY): the
Fetch Models control's keyless-then-keyed walk, and NanoGPT arriving in the
image picker from the live registry. Both are written so a fallback render fails
them rather than quietly passing.

#### 2026-08-22 — port(embeddings): the NanoGPT embedding-provider surface (v4 `781fc420`, P4.D102)

_Versions: SPA 0.5.538._

NANOGPT joins the embedding-provider union, the metadata map (display name,
requires-a-key, description — verbatim), the badge-class map, and the
needs-an-API-key list, so a NanoGPT embedding profile without a key now shows
the missing-key badge. The `qt-badge-provider-nanogpt` CSS rule lands with it,
including v4's quirk that `--qt-badge-primary-border` has no definition at all —
the same shape as ollama's secondary border, and deliberately left unpainted.

Two order items were refuted by measurement and NOT landed. v4 has no NanoGPT
row in the connection-profile fallback provider list at this baseline — its list
is the same seven v5 already carries, and none of the round's three drift
commits touch that file — so adding one would have been a v5-invented
divergence. And NanoGPT's client attachment fall-through is achieved by ABSENCE:
v4's static table has no row for NANOGPT (nor for Z_AI or DEEPSEEK), so the
correct port is no entry, with the known-stale note updated to name it.

#### 2026-08-22 — port(images): the honest Fetch Models flow in the image-profile modal (v4 `ca22ec45`, P4.D102)

_Versions: SPA 0.5.537._

The Model row is now v4's real control instead of a `defaultModels` listing.
`imageProfileListModels` is wired through `image-profiles.api.ts` (the contract
type already existed), the modal auto-loads on provider/key change, the Fetch
Models button carries v4's disabled states and both title strings, and the
source label beneath reproduces v4's four sentences character-for-character —
the provider tally with its singular/plural, and the three-way built-in
ternary.

Faithful details worth naming: a hard request failure falls back to the
registry's `defaultModels` WITHOUT a `fetchError`, so it reads as the plain
built-in sentence rather than the "Couldn't fetch" one, matching v4's fallback
branches; the provider is normalized before the call, since the server echoes
the raw string it was sent; and any `source` that is not exactly `provider`
reads as built-in.

The Model select assigns its value post-render rather than binding it, so an
off-list name leaves the control blank as React does instead of snapping to row
zero — reachable whenever a provider has no default models.

Eight pins were mutation-tested. Two initially survived and are now covered: a
stale source label during a re-fetch (the first fetch cannot show it, so only a
second one discriminates), and the non-`provider` source coercion.

The `Validate` button stays deferred — v4 did not move it this round.

#### 2026-08-22 — port(images): the Z.AI and NanoGPT image-provider entries (v4 `ca22ec45` + `781fc420`, P4.D102)

_Versions: SPA 0.5.536._

`FALLBACK_PROVIDERS` gains v4's Z.AI and NanoGPT rows — labels, default model
lists and `apiKeyProvider` transcribed verbatim — so the image-profile modal
offers both providers in v4's order when the `list-providers` fetch fails.
`PROVIDER_DEFAULTS` gains their icon rows (`ZAI` / success, `NGPT` / primary);
without them both fell through to the generic three-character abbreviation and
rendered as `Z_A` and `NAN`.

v4's matching `PROVIDER_BADGES` rows have no v5 home — `ProviderBadge` is a
standing named deferral — so they are recorded verbatim in that deferral note
rather than dropped, for whichever lane lands the badge surface.

#### 2026-08-22 — docs(porting): work orders for the `d5830439` drift round (P4.D100 → P4.D101 stacked ∥ P4.D102)

_Docs-only change._

The next round planned against new v4 baseline `d5830439` (three commits
of drift: `ca22ec45` honest image Fetch Models + Z.AI image generation
made real; `781fc420` the NanoGPT bundled provider; `d5830439` NanoGPT
thinking options). Three orders written with fresh two-sided surveys:
P4.D100 (server — the image list-models verb, five providers' keyed
model discovery, the bytes/download seam + Z.AI URL→base64, the gemini
routing widening), P4.D101 (server, stacked on D100 — NanoGPT end to end:
manifest through the generator, chat wire + the dual reasoning dialect,
images, embeddings, switch-table census, thinking options), P4.D102 (SPA
— the whole client half, contracts pinned, beats gated). Deliberately
left out: the merge-verb silent-keep sweep (collides again with this
round's settings/provider case files), the maintenance trio
(`response_parse_equivalence` is where NanoGPT's rows land), and the owed
💸 dogfood queue (a `/dogfood` pass right after unification). Also fixed:
P4.12's stale `STATUS: OPEN` header (closed-by-P4.13 since 2026-07-23;
findings #22/#25 long confirmed gone).

#### 2026-08-22 — unify: the `12fe3e6f` thinking-turn drift round (P4.D97 ∥ P4.D98 ∥ P4.D99 ∥ P4.54)

_Versions: core 0.0.599, harness 0.0.523, host 0.0.75, SPA 0.5.535._

All four lanes unified; the oracle baseline moves `b8449b3e` → `12fe3e6f`.
v4's bugs 84/85/86 are absorbed whole — bug 84 (the tool-error sentence)
and bug 85 (the DeepSeek thinking-prefill 400) were this port's own
dogfood filings coming back fixed. Server: the thinking-turn evaluator +
registry join, the manifest substrate's first per-model facts +
`thinkingTurnRule`, the prefill `runsThinkingTurn` threading, the
model-aware DeepSeek strip, and the retire-prefill heal keyed on v4's own
`migrations_state` ledger. SPA: the browser evaluator twin, the profile
editor's three thinking-turn behaviors, and bug 84's two-layer client fix
(the reducer carried the sibling `error` nowhere before). Maintenance: run
lines for 32 of the 39 `nothing_to_run` families.

Unification wires: `P4D97_THINKING_WIRE_LANDED` flipped true (the
thinking-turn e2e beat activated) and the contract diffed name-for-name
across sides. The §3 review found no blocking findings; one documented
mechanism divergence recorded (the editor's stored-null correction is a
fired-once latch where v4's effect can re-fire — the order sanctioned the
once-only spelling).

Gate: fmt clean; clippy both feature sets; release build; 443 test
binaries / 2,253 tests / 0 failed with the round's env block; the nine
affected families fresh at a PINNED `12fe3e6f` worktree through the sweep
driver, zero SKIP, changed-bytes verified in every regenerated NDJSON;
SPA 334 files / 4,970 tests + production build; full Playwright 233/233
zero skips (the suite grew with the activated beat). ⚠ v4 moved DURING the round (`ca22ec45`,
image-provider Fetch Models + Z.AI image generation) — pin `12fe3e6f`
for every regen until that catch-up runs.

#### 2026-08-21 — docs(porting): P4.54 closes — the run-line classification, executed and recorded

_No crate versions bumped._

The successor artifact to P4.53's inventory:
`harness/tools/sweep-results/2026-08-21-12fe3e6f-p4.54-run-lines.json`. It is the
sweep driver's own results record (same shape as P4.53's) for the 29 families
this lane gave run lines, with a `p4_54_classification` block folded in carrying
all 35 in-scope rows plus the four P4.D97 owns this round. Every run-line row
carries its family's verbatim `test result:` line, because a bare exit code is
not evidence that anything ran — that is what the driver's SKIP guard exists to
say.

The vacuous-green debt goes **39 → 10**. The residual is exactly the six
wire/seam contract pins ruled correctly-headerless plus P4.D97's four envelope
families, whose run lines ride a later maintenance pass.

Recorded as new debt rather than fixed here:
`p4_6ay_workbench_wire_contract` is a sixth contract pin of the identical class
— its own header cites the `p4_6ar_wire_contract` precedent by name — that the
driver's `EXEMPT_FAMILIES` constant omits, so its refusal sentence names the
wrong reason (`no_oracle` instead of the exemption). The repair is one line of
driver logic, which this lane's Ownership forbids. A note for whoever takes it:
`--self-test`'s two end-to-end arms pick a real exempt family and a real
non-exempt `no_oracle` family out of the LIVE debt list, so the class cannot be
retired to zero until the self-test gets synthetic families of its own.

Neither the driver nor any fixture, corpus or test assertion was touched.

#### 2026-08-21 — test(harness): scoped run lines for the harness and CLI `nothing_to_run` families

_No crate versions bumped._

P4.54 item 1, second half — the ten remaining in-scope families outside
`quilltap-web`.

Eight are committed-corpus differentials (`image_dialects_equivalence`,
`moderation_wire_equivalence`, `restore_vintage_state`,
`stream_decoders_equivalence`, `streaming_composer_equivalence`,
`tool_wire_call_site`, `tool_wire_equivalence`, `web_search_wire_equivalence`).
Their by-hand RECORDING stage stays non-runnable by the driver's own design —
`stages_to_run` excludes a committed-corpus regen so a sweep can never clobber
bytes checked into the repo — and the new line adds only the cargo half.
Verified after the edit that all eight still classify as `committed_corpus`: a
run line carrying a `QT_ORACLE*=` variable would have flipped them out of that
class and re-armed the recording stage, so none does.

`web_search_runner_wire` is the P4.42 wiring proof: no oracle, no env var, a
bare scoped line.

`cli_differential` is the one family in the set that can run vacuously. It is
env-gated on `QT_V4_CHECKOUT` and prints `skipping CLI differential: …` when it
is absent, which the driver's `SKIP_PROSE` detector catches and reports as
`skipped`, exit 3. Its run line therefore carries the gate itself —
`QT_V4_CHECKOUT=~/source/quilltap-server QT_NODE=$N/node` — which is what makes
the line self-contained rather than dependent on the operator's shell:
`normalize()` rewrites the v4 path to the driver's `--v4` pin and prepends the
Node 24 `N=` assignment.

#### 2026-08-21 — test(web): scoped run lines for the nineteen `nothing_to_run` web arms

_No crate versions bumped._

P4.54 item 1, first half. Nineteen `quilltap-web` integration arms extracted to
zero stages, so `recipe_sweep.py --run <fam>` refused them as `nothing_to_run`
rather than printing a free green. Each header gains the sanctioned indented
block — `//! Run:` at the prose margin, `cargo test -p quilltap-web --test
<family>` indented past it — which satisfies both P4.45 rules at once: the
INDENTATION rule (a command sits two spaces past the marker, prose at one) and
the ATTRIBUTION rule (`--test <family>`, so a SKIP in that run can be attributed
to this family rather than a stranger).

None of the nineteen reads a `QT_FIXTURE_*` variable — measured, not assumed:
they resolve their committed fixtures through `CARGO_MANIFEST_DIR` — so the bare
scoped line is genuinely self-contained and the silent-skip hazard does not
apply to this set.

Four of them (`change_passphrase_archive_sweep`, `characters_action_route`,
`characters_wardrobe_route`, `file_content_missing_404`) already carried a
`Run:` line with the command in backticks, written at the PROSE margin, so the
driver had always read it as a sentence. Those are rewritten into the indented
form rather than duplicated.
#### 2026-08-22 — docs(porting): finding #99 is fixed — v4 at `d9c98cf2`, v5 in P4.D99

_Docs-only change._

Row #99 moves from "NO v5 CHANGE — FILED UPSTREAM as v4 Bug 84" to FIXED,
recording both halves: v4's single resolver at the render site, and v5's
two-layer version (the reducer had to carry the sibling `error` before the
vertical could read it, so a render-site-only fix would have been inert). It
also carries v4's own correction of the filing — one `imageProfileId` feeds
both the tool slate and the executor, so the recorded "offered-but-refused"
repro is not reachable that way, though the frame shape the defect turns on
is identical either way — and notes the live look owed to the dogfood queue.

The standing-notes paragraphs the finding earned (the dead-observer lesson,
and "a field the server carries on purpose is worth grepping for on the
client") are left untouched: they are lessons, not state.

The Tier-2 `docs/v4/` mirror refresh is a measured no-op. `d9c98cf2` touched
`docs/developer/bugs.md`, the bug-84 filing doc, and
`help/image-generation-profiles.md`; the mirror carries none of the three
(`docs/v4/developer/bugs/` holds only `fixed/`, and `docs/v4/help/` holds
only `database-protection.md`), so there is nothing to refresh.

#### 2026-08-22 — fix(salon): render the failing tool's own sentence in the notice and the toast (bug 84)

_Versions: SPA 0.5.531._

Both `generate_image` failure surfaces now resolve through
`resolveToolResultErrorText`, so a refusal that names its own remedy —
`Image generation is not enabled for this chat` — reaches the user instead of
`Failed to generate image` / `Image generation failed: Unknown error`. The
generic strings stay, byte-identical to v4's, as the fallback for a frame
that carries nothing worth showing.

The one-level-too-deep `(call.result ?? {}).error` read is gone; the success
branch's cast narrows to `{ images?: unknown[] }`.

The deliberate faithful-reproduction pin is retired: the spec that asserted
the generic strings as all a failure could ever say now asserts the resolved
sentence with its prefix stripped, and keeps the generic arm as the fallback.
A third case pins the WHOLE path — real frames through `reduceChatFrame`,
then the reporter, then the notice — because the existing notice specs drive
the reporter directly through a cast, which is exactly how a reducer-level
drop stayed invisible. Reverting either layer alone turns that case red
(both mutations run).

#### 2026-08-22 — port(salon): carry the tool-result error sentence through the reducer (bug 84)

_Versions: SPA 0.5.530._

`PendingToolCall` gains `errorText`, and `applyToolResult` stores the frame's
sibling `error` on the matching call. It is carried RAW — the executor's own
`Error: ` prefix intact — so the reducer stays pure and the render site
resolves it, mirroring v4's own separation.

v5 is a two-layer fix where v4 is one: v4's hook receives raw frames, while
v5 splits into a pure reducer plus a vertical reporter, and the reducer used
to drop the sibling `error` before the vertical ever saw it. Placing the
resolver at the render sites alone would be inert — the data never gets
there.

New reducer coverage feeds real frames through `reduceChatFrame`: a failing
`generate_image` result carries its sentence onto the call, and a success
frame leaves `errorText` undefined. Dropping the carry turns the first red
(mutation-proven).

#### 2026-08-22 — port(salon): the tool-result error-sentence resolver, v4's client twin (bug 84)

_Versions: SPA 0.5.529._

Lands `resolveToolResultErrorText` as a standalone client twin of v4
`app/salon/[id]/hooks/useSSEStreaming.ts` (v4 `d9c98cf2`). The SSE
`toolResult` frame carries a failing tool's human-readable text in `error`,
a sibling of `result`, because `result` itself is null on failure; the
resolver prefers that sibling, keeps the nested `result.error` read as a
fallback, strips the executor's own leading `Error: ` prefix, and returns
`undefined` for anything empty so a caller's generic string still fires.

The spec is transcribed case for case from v4's own regression test
`__tests__/unit/hooks/useSSEStreaming-tool-error.test.ts`: the real failure
shape, the prefix strip, the unwrapped sentence, the nested fallback,
sibling-wins precedence, and every empty case.

No caller reads it yet — the reducer carry and the two render sites follow.
#### 2026-08-21 — test(e2e): the gated thinking-turn prefill beat (P4.D98 tier 2, ACTIVATE-AT-UNIFY)

_Versions: SPA 0.5.532._

One gated beat in the provider-options flow: a DeepSeek profile that opts
into thinking through the schema panel un-seeds the multi-character prefill
box on model pick, warns when re-ticked, and stands the warning down on an
explicit thinking-off — gated `P4D97_THINKING_WIRE_LANDED = false` until the
server half (P4.D97) serves the rule. The beat drives the profile-choice arm;
the model-facts arm cannot run keylessly (v4 refuses a keyless model fetch
for a key-requiring provider) and is pinned at the component tier instead.

#### 2026-08-21 — feat(spa): the profile editor's three thinking-turn behaviors (v4 bug 85, P4.D98 unit 3)

_Versions: SPA 0.5.531._

The connection-profile editor now answers "will this profile run a thinking
turn?" and seeds the multi-character prefill box accordingly (client half of
v4 `97d2fcb5`): a model pick on a NEW profile re-seeds from the model's
static facts (a seed, never a clamp); a stored row that never chose (null)
is corrected ONCE when the model list lands, through both fetch sites, with
a fired-once latch so it can never fight the checkbox; and ticking the box
on a thinking profile draws v4's warning paragraph byte-for-byte. The
`fetchedModelsWithInfo` plumbing carries the wire's `modelsWithInfo` rows
the editor needs. All behaviors degrade to the provider-rule-only seed when
the wire omits the rule and facts (the pre-P4.D97 state), spec-pinned; the
correction wirings and the latch are mutation-proven red-first.

#### 2026-08-21 — feat(spa): defaultMultiCharacterPrefill learns runsThinkingTurn (v4 bug 85, P4.D98 unit 2)

_Versions: SPA 0.5.530._

The client twin of v4 `lib/llm/multi-character-prefill.ts` gains the
`runsThinkingTurn = false` second parameter from `97d2fcb5`: a profile that
will run a thinking turn seeds the prefill box off on ANY provider, before
the provider rule is consulted. Doc comments carried (including the "Resist
adding a provider here" warning); the parity spec grows v4's three new
cases 1:1. The `profileUsesNamePrefill` resolution half stays server-side
(P4.D97).

#### 2026-08-21 — feat(spa): the evaluateThinkingTurn browser twin + the thinking-turn contract fields (v4 bug 85, P4.D98 unit 1)

_Versions: SPA 0.5.529._

Ports the client half of v4 `97d2fcb5`'s shared evaluator:
`settings/providers/thinking-turn.ts` answers "will this profile run a
thinking turn?" from the provider's declared `thinkingTurnRule`, the profile's
`parameters`, and the selected model's static facts — explicit choice wins
(absent/null/`''` all read as unset), else `thinksByDefault`, else no. Parity
spec transcribed 1:1 from v4's `thinking-turn.test.ts` (10 cases). The wire
contract mirror gains `ThinkingTurnRule`, `ProviderInfo.thinkingTurnRule`,
and the two `ModelInfo` facts (`supportsThinking` / `thinksByDefault`); the
server half that serves them is P4.D97's, and everything here degrades to the
provider-rule-only answer while the fields are absent, exactly as v4's client
does.
#### 2026-08-21 — test(harness): sweep-runnable run lines for the three envelope families (P4.D97 rider)

_Versions: harness 0.0.523._

`request_builder_equivalence`, `request_builder_google_equivalence`, and
`request_builder_google_wire_equivalence` were `nothing_to_run` refusals
under the sweep driver — committed-corpus families whose recording is a
deliberate by-hand step and whose headers carried no `cargo test` run line
(three rows of the P4.53-measured vacuous-green debt; P4.54 owns the OTHER
families' run lines this round and excludes these three by name). Each
header gains the scoped run line; all three now run end-to-end by name
through the driver, and `--self-test` stays at 0 failures.

#### 2026-08-21 — docs(porting): the P4.D97 mirror refresh + dispositions (v4 bugs 84–86 docs)

_Docs-only change._

The `docs/v4` mirror refreshed at the `12fe3e6f` pin: `developer/API.md`,
`developer/PROVIDER_PLUGIN_DEVELOPMENT.md`, `CHANGELOG.md`, and — new to the
mirror — `developer/bugs.md` (whose table carries the two docs-only commits'
rows: `e04405a5` filed bug 85, `c0984bdf` filed bug 84) plus the two fixed
filings under `developer/bugs/fixed/`. The v4 root README's model-table
delta has no mirror slot (the mirror never carried the root README or plugin
READMEs) — recorded, not invented. Dispositions: `lib/startup/prettify.ts`'s
migration pretty-label is NO-PORT (v5 surfaces no migration labels anywhere
— the P4.D79/D63/D73 precedent); package/lock + plugin-types version churn
is NO-PORT (the type additions were ported as the manifest substrate). Loud
deferrals recorded in the lane record: the two help-file deltas → the
`p4.9i2` bank, and the two 💸 live proofs → the dogfood queue.

#### 2026-08-21 — feat(db): the retire-prefill-on-thinking-profiles boot heal over v4's migration ledger (P4.D97 unit 6)

_Versions: core 0.0.599, harness 0.0.522, host 0.0.75._

v4's `retire-prefill-on-thinking-profiles-v1` data migration (`97d2fcb5`),
re-homed as a boot heal — with a NEW once-only mechanism for this repo: the
pass is data-only (no column absence to key off, the P4.D79-class guard
does not transfer), and re-running it would clobber a user's re-ticked
`true`. The guard is v4's OWN `migrations_state` ledger, interoperated in
both directions: the heal skips when the row
`retire-prefill-on-thinking-profiles-v1` exists (v4 migrated first), and
after healing writes the row with v4's exact `migrations/state.ts` shapes
(lazy CREATE of both tables under the migrations_state absence check, the
row's five columns, the two `migrations_metadata` upserts) so v4's runner
skips thereafter (`isMigrationCompleted`). `quilltapVersion` records which
app stamped the row — v4 writes its package version (`4.9.0-dev.43` at the
pin), v5 stamps quilltap-core's; the id is the key. Table/column-absent
skips stamp NOTHING (v4's `shouldRun` false arm — retried next boot).
Fresh instances match v4's observable outcome: no `migrations_state` at
provisioning (D23 — nothing invented), the row appearing on the first boot
exactly as v4's runner stamps every migration on its first run.

The heal transcribes v4's FROZEN rule tables + selection + per-row
evaluation verbatim ("a migration describes the world it ran in" — the
frozen copies deliberately do NOT follow the manifests), runs after
`ensure_connection_profiles_prefill_column` in `seed_built_ins` (the column
must exist first on a pre-4.9 instance), and logs the examined/cleared
counts. New differential `thinking_prefill_heal_equivalence`
(`QT_ORACLE_THINKING_HEAL`): both sides build the same migration-vintage
table from the committed `thinking-prefill-heal.json` spec (v4's own
integration-test matrix plus modelName-NULL / parameters-NULL /
string-"true" / `''`-option rows — 17 rows, 13 examined, 6 cleared); the
oracle drives v4's REAL migration `run()` + `recordCompletedMigration()`
and proves `isMigrationCompleted` skips a re-run; the diff covers every
profile row, the ledger row (ts/version normalized), the metadata upserts,
and the result message byte-exact. The second-boot no-op and the
re-ticked-true survival are pinned in both the module tests and the family.

#### 2026-08-21 — fix(deepseek): decide the thinking strip from the model, not the request body (v4 bug 86, P4.D97 unit 5)

_Versions: core 0.0.598, harness 0.0.521._

v4 `12fe3e6f` ported: `strip_thinking_incompatible`'s predicate becomes
`deepseek_will_run_thinking_turn(body)` — the profile's explicit
`thinking.type` enabled/disabled first, then the model's own habit from the
frozen catalogue copy by exact-id match (`deepseek-v4-flash` /
`deepseek-v4-pro`, both `thinksByDefault`). Reading only the body was the
bug: a default-state profile on a V4 model was judged not to be thinking,
and temperature/top_p/frequency_penalty/presence_penalty went out on a
request that ignores them. The frozen copy is pinned against the committed
`deepseek.json` manifest's `models` field by a unit test (the two homes
cannot rot apart — v4 carries the same duplication plugin-side), and the
predicate's truth table (explicit both ways, habit by exact id,
uncatalogued/no-model/unrecognized-shape fall-throughs) is unit-pinned.

The request-envelopes corpus regenerated at the pin with three new DeepSeek
rows (both modes; 263 → 269): `thinking-default-v4-model` (the strip fires —
temperature/top_p shed from base, the two penalties never land),
`thinking-model-default-empty` ("(model default)" — the shared applier skips
`''`, so the habit decides), `thinking-disabled-v4-model` (everything
stays). All 263 pre-existing rows byte-identical — the corpus's other
DeepSeek rows ride `deepseek-chat`, an UNCATALOGUED id, so they are the
keeps-the-params leg (the order's expectation that `profile-params` /
`profile-params-skips` would shed params was WRONG about the corpus's model
id — measured, not assumed). The pre-fix red is on record: the commit-C
workspace gate ran the old predicate against the regenerated corpus and
failed at `thinking-default-v4-model[stream]` with the params present.

#### 2026-08-21 — feat(llm): thread runsThinkingTurn through the prefill default (v4 bug 85, P4.D97 unit 4)

_Versions: core 0.0.597, harness 0.0.520._

The prefill second-arg from v4 `97d2fcb5`: all three
`multi_character_prefill` functions gain `runs_thinking_turn` (checked FIRST
in the default; a stored boolean still outranks it — the tri-state is
intact), and the three call sites thread the registry join: the create
route's absent-field default now resolves
`profile_runs_thinking_turn(provider, modelName, parameters)` (the PUT arm
verified untouched, as v4 left it), and both `use_prefill` producers
(orchestrator + regenerate-swipe) join over the profile row. The prefill
oracle corpus doubled with the runsThinkingTurn axis (288 rows, full-product
shape-asserted; deleting the thinking arm reds the family). Four new
settings-routes create arms (absent-field DeepSeek-V4 default now false;
explicit thinking-disabled keeps true; Ollama enable_thinking true; a stored
true surviving) — regenerating them exposed an ORACLE HARNESS GAP: the jest
environment's provider registry was EMPTY, so v4's join silently answered
the old provider default; the case now registers the two declaring dist
plugins exactly as production startup does (a rule-less plugin and an absent
one evaluate identically, so the other arms are unaffected). Family at 132
cases; the create join mutation-proven. `orchestrator_tier3` regenerated
fresh at the pin: no movement (its profiles carry stored booleans), as the
order predicted; the producer-site wiring matches v4's own coverage level
(v4 tests the functions, not the context-builder call) — the live DeepSeek
repro rides the dogfood queue. The `connection_profiles_tier2` question
resolved by inspection: that family drives the REPOSITORY, which never
resolves defaults, so no op can observe the route default there.

#### 2026-08-21 — feat(api): serve thinkingTurnRule + model thinking facts on the wire (v4 bug 85, P4.D97 unit 3)

_Versions: core 0.0.596, harness 0.0.519._

The two wire serializations from v4 `97d2fcb5`. `provider_list` (v4
`GET /api/v1/providers`) serves `thinkingTurnRule` per provider — `?? null`,
positioned after `optionsSchema`, key order carried by the typed struct's
field order. `model_fetch` (v4 `POST /api/v1/models`) spreads
`supportsThinking` / `thinksByDefault` onto each `modelsWithInfo` echo row per
exact-id match against the manifest's model catalogue, keys omitted when the
catalogue has no entry — the merge lives at the route as v4's does, so it
covers every fetcher. Measured at the pin: the GET leg is untouched in v4 (the
cache write carries no thinking facts), so `model_list` stays as it was —
pinned by a new wire-actions test asserting the cache read stays fact-blind.
`providers_listing_equivalence` regenerated at the pin now compares
`thinkingTurnRule` byte-for-byte (key order included, the optionsSchema
precedent) with an exactly-two-rules shape assertion; removing the serializer
line reds the family (mutation-proven).

#### 2026-08-21 — feat(llm): port the thinking-turn evaluator + manifest substrate (v4 bug 85, P4.D97 units 1–2)

_Versions: core 0.0.595, harness 0.0.518._

The pure evaluator from v4's `lib/llm/thinking-turn.ts` (`97d2fcb5`):
`evaluate_thinking_turn(rule, parameters, model)` answers "will this profile
run a thinking turn?" — an explicit rule-keyed choice in the profile's
parameters wins (unset = absent/null/empty string; `disabledValues` checked
before `enabledValues`), else the model's `thinksByDefault` habit, else false.
The `ThinkingTurnRule` and `ThinkingModelFacts` types land in the
provider-manifest substrate (v5's analog of `@quilltap/plugin-types`), carried
as opaque JSON scalars so the wire re-serializes them byte-for-byte; value
matching uses JS `===` semantics (numbers compare as f64, never across
types).

The manifest substrate grows two optional fields (`schemaVersion` stays 1 —
additive optionals): `thinkingTurnRule` after `optionsSchema`, and `models`
(fact-bearing model-catalogue entries) after `fallbackModels`. The generator
learned both and all nine manifests were regenerated at the `12fe3e6f` pin —
only deepseek (rule + two V4 model entries + the bug-86 helpText rewrite,
which rides the same regen) and ollama (rule only) moved, byte-reviewed. The
generator emits the fields only where the plugin declares them, so the other
seven manifests stay byte-identical; a fact-less model entry is observably
identical to no entry on every consumer, so only fact-bearing entries are
emitted. Registry accessors `thinking_turn_rule` / `model_thinking_facts`
plus the join `profile_runs_thinking_turn(registry, provider, model, params)`
(v4's `providerRegistry.profileRunsThinkingTurn` — JS-falsy guards, exact-id
model lookup) round out the substrate.

New tier-1 differential `thinking_turn_equivalence`
(`QT_ORACLE_THINKING_TURN`, 1,134 cases over rules x parameter shapes x model
facts), with the disabled-before-enabled order and the empty-string-is-unset
arm both mutation-proven (the latter needed `''` planted inside a rule's own
disabled list — a set-but-unmatched value falls through identically
otherwise). The prefill threading, wire serializations, strip predicate, and
the heal arrive in the following units.

#### 2026-08-21 — docs(porting): plan the `12fe3e6f` drift round — four work orders (P4.D97 ∥ P4.D98 ∥ P4.D99 ∥ P4.54)

_Docs-only change._

v4 moved five commits past `b8449b3e`, all dated today: bug 84 (the
tool-result error sentence — this port's own finding-#99 filing, now fixed
v4-side), bug 85 (prefill-hostility scoped to thinking models via a new pure
`thinking-turn` evaluator, a declarative per-plugin `thinkingTurnRule`,
per-model `supportsThinking`/`thinksByDefault` facts, and a data migration
retiring the stored prefill on thinking DeepSeek/Ollama rows), and bug 86
(DeepSeek's thinking-incompatible param strip now decided from the model,
not the request body). The bugfix branch is quiet. Four orders written
against the new `12fe3e6f` baseline: P4.D97 (server half — evaluator,
manifest substrate growth including the first per-model facts, registry
join, both wire serializations, the bug-86 predicate, and the once-only
heal interoperating with v4's `migrations_state` ledger), P4.D98 (the
profile-editor client half — client evaluator twin, model-change re-seed,
stored-null correction, the warning paragraph), P4.D99 (bug 84's two-layer
client fix — the reducer currently drops the sibling `error` before the
vertical sees it), and P4.54 (run lines for the 35 in-scope
`nothing_to_run` families from P4.53's committed inventory).

#### 2026-08-21 — docs(porting): the anti-chorus dogfood pass — 18 pass, two findings, nine live proofs discharged

_Docs-only change._

The `c8a3cf77` and `b8449b3e` rounds met real data. Both multi-character anchor
routes proven at the byte level on a purpose-built three-character chat; the
direct-address rewrite behaves in production (a third-person mention no longer
arms the caution, a vocative does), and a character passed with the skip
sentinel unprompted. Per-turn conversation summaries mutation-proven ON vs OFF
over the persisted whispers. The vision send, the P4.50 log line, the bug-76
key heal, the tool-change splice-once, whispered announcements, the roleplay
quote delimiter, and the failed-import warnings all discharged.

Two v4-heuristic observations recorded, both v4-faithful: `and` is a vocative
lead-in, so a roll-call recap ending `X and Y.` re-arms the caution the
anti-chorus fix withholds; and the caution can never see the message that just
addressed the responder, because the user message is persisted after the
eligibility read in both apps.

Finding #98: the Serper key configured through v4's Settings → API Keys is
invisible to v5, which reads only `SERPER_API_KEY` — the search-provider plugin
registry is the standing P4.42 deferral, so web search is dark on a real
instance. No v5 code changed; the refusal path itself is faithful.

#### 2026-08-21 — fix(documents): Document Mode fills its workspace tab, so source mode is more than three lines

_Versions: SPA 0.5.528._

Dogfood finding #97. Opening a store document through the rail's Document Mode
entry and switching to markdown source rendered the textarea 77px tall inside a
788px pane — three visible lines of a 52,000-character document, with 700px of
empty pane below it. The WYSIWYG branch masked it: ProseMirror grows to its own
content and the pane scrolls, so nothing looked wrong until the toggle.

`StandaloneDocumentView` declared `host: { class: 'flex flex-col flex-1 …' }`,
but the workspace mounts it inside `qt-tab-view` — an Angular custom element
with no host styling, i.e. `display: inline`. `flex-1` in a non-flex parent is
inert, so the view collapsed to content height and every `h-full` beneath it
measured against that. v4 has no such element: its `TabView` renders context
providers only, so `DocumentPane`'s `flex flex-col h-full` root is a direct
child of `.qt-tab-pane` and measures the grid cell. The host class is now
`h-full`, which resolves against `.qt-tab-pane` the same way — and the same way
the Salon's `block h-full` host already did. Re-measured on the same document:
77px to 612px.

`standalone-document-view` was the only `flex-1` host mounted directly by the
tab registry; the other five all sit inside real flex parents.

Pinned by a spec asserting the rendered host class carries `h-full` and not
`flex-1` (jsdom computes no layout, so a height assertion there would be
vacuous), and by extending the existing p4.9l2 standalone-toolbar beat to
*measure* the textarea against its pane. That beat already toggled source mode
and asserted the markdown bytes, and passed throughout — which is why the bug
shipped.

#### 2026-08-21 — docs(porting): the case-folding divergence ratified

_Versions: core 0.0.594._

The human ratified P4.D96's recorded case-folding divergence (Rust Unicode
simple folding vs v4's ECMAScript canonicalize — over-detection only, on a
handful of exotic code points). The skip_signal doc comment, the order
status header, and the CLAUDE.md round bullet now carry the ratification;
a ruling record is appended to the status log. Comment-only source change.

#### 2026-08-21 — port(unify): the b8449b3e round lands — the anti-chorus drift, the memories fixture vintage, the sweep-driver follow-ups

_Versions: core 0.0.593, harness 0.0.517, SPA 0.5.527._

The three-lane `b8449b3e` round unifies onto main and the oracle baseline
moves to `b8449b3e`. P4.D96 ports v4's anti-chorus commit whole (the
direct-address `isRecentlyAddressed` rewrite in core and the SPA twin, the
grown turn-skip note, the turn-anchor restructure with the
GROUP_SCENE_DISCIPLINE block, a new tier-1 turn-anchor family; the
case-folding divergence is recorded and awaits ratification). P4.52 widens
the committed memories fixture pair to the current schema vintage and
retires the housekeeping ruled vintage row to a plain equality. P4.53 makes
sweep-recipe checkout aliases unforgeable, turns empty-stage runs into a
named refusal (39 families measured), and repairs the five clobbering
headers. Unification work: version accumulation (harness took six lane
bumps), a mid-pick Cargo.lock repair, and the round docs. Gate: fmt, clippy
both feature sets, release build, driver self-test 0 failures, the seven
affected families regenerated fresh at a pinned b8449b3e worktree (zero
SKIP), cargo test --workspace 441 binaries / 2,237 / 0, SPA 332 files /
4,936 / 0 with a clean build, and the full Playwright suite green.

#### 2026-08-21 — docs(porting): record the P4.D96 gate

_Docs-only change._

The lane's verification gate, all green: fmt; clippy plain and with
`quilltap-core/native-transport`; `cargo test --workspace` at 441 binaries /
2,237 passed / 0 failed with both lane oracle env vars set; both lane families
by name through the sweep driver against the `b8449b3e` pin, zero SKIP; the
fourteen-family spine batch at 14 ok; SPA 332 files / 4,936 tests and a clean
build. Playwright was not run — no e2e spec changed.

#### 2026-08-21 — docs(porting): the P4.D96 spine regen, the b8449b3e NO-PORT, and two corrections

_Docs-only change._

Records unit 4 (fourteen spine families regenerated at the `b8449b3e` pin
through the sweep driver — all green, exit 0, zero SKIP) with the coverage
measurement that green alone does not give: `orchestrator_tier3` is the one
family carrying all three changed byte sequences through the production spine
(56 discipline blocks, 25 turn notes, 49 identity instructions),
`regenerate_swipe_tier3` and `salon_swipe_generate` carry the discipline block
only, and the other eleven carry none. No committed corpus in the tree carries
the old bytes, so no fixture moved.

Records unit 5: v4 `b8449b3e` (the jest `--no-sparkplug` guard for bug 83) is
NO-PORT — v5 has no jest — and our own zone-legged `jest-zone-globalsetup.cjs`
chains v4's `globalSetup` rather than replacing it, so the guard still arms for
the two families that pass it. The regen venue gains from the commit; nothing is
bypassed.

Corrects two claims in this round's earlier entries, which the unit-4
measurement disproved: the turn-skip note and the turn anchor each had no
*direct* differential before this round, but both were covered transitively by
`orchestrator_tier3`. Also refreshes the `docs/v4` mirror of the nothing-to-add
spec, and records the deferrals: the two help docs to the `p4.9i2` bank, and a
live group-scene walk to the dogfood queue.

#### 2026-08-21 — feat(salon): the client skip-signal twin follows the direct-address rewrite

_Versions: SPA 0.5.527._

The Salon's client-side `isRecentlyAddressed` — which guards the human Skip
button and the turn banner — takes v4 `e22f7b36`'s rewrite as a near-verbatim
transcription: the two vocative constants and `buildDirectAddressRegex`, with
`escapeRegex` replacing the mention scan as the module's regex import. The
parity spec grows v4's own eight new cases 1:1, plus the no-usable-name null
return and its whisper-still-wins twin, which the server family also pins.

`findMentionedCharacterIds` stays in the client helpers with no consumer, as the
faithful mirror of v4's still-shipping `mentioned-characters.ts` (whose Rust
twin keeps its own consumer in `services::off_scene`); its doc comment now says
so.

#### 2026-08-21 — feat(salon): a group-scene discipline block rides every multi-character turn

_Versions: core 0.0.593, harness 0.0.513._

Ports the anchor half of v4 `e22f7b36`. `applyMultiCharacterTurnAnchor` is
restructured: it finds the system message first, builds a list of system
additions (the prose route contributes its identity instruction first; the
prefill route contributes none), always appends the new byte-exact
`GROUP_SCENE_DISCIPLINE` block, joins the list with a blank line and appends it
to the system message when one exists, and only then — prefill route — pushes
the assistant `[Name]` message. Two behavior deltas: the prefill route now
edits the system message too (it used to return before reading one), and a
prose turn appends two blocks where a prefill turn appends one.

The discipline block is the anti-chorus content rule set: no recap openings, no
agree-then-add, no reusing another character's coined phrases, no restating the
plan, speak only to change something, vary length. The identity anchor keeps a
turn attributed to one character but says nothing about its content, and with
the previous turns as the strongest style examples in context, models converge
into exactly the chorus the block forbids.

New tier-1 differential `multi_character_turn_anchor_equivalence` over v4's real
exports — no oracle drove this function *directly* before; the spine covered it
transitively through `orchestrator_tier3`. Both routes × system message
present/absent, system-message placement (not first; two systems, the first
wins), empty system content, an empty message array, and four interpolated-name
shapes, each row diffed as the full post-call message array, plus the constant
on its own. Mutation-proven on the join separator, the prefill-route system
append, the prose identity instruction, the additions order, and one byte of
the constant.

#### 2026-08-21 — feat(salon): the turn-skip signal requires direct address, and the note calls restating a pass

_Versions: core 0.0.592, harness 0.0.512._

Ports the turn-skip half of v4 `e22f7b36`. `isRecentlyAddressed` no longer
fires on any name mention: it now requires a direct address — the character's
name or an alias in a vocative position, an `@`-mention, or a targeted whisper.
A chorus-prone group scene opens every turn with a roll-call recap naming most
of the cast, so the mention-based signal marked everyone addressed forever, the
"answer rather than pass" caution fired for every character on every turn, and
nobody ever passed. The Turn note's base text gains a closing paragraph (a
reply that mostly restates or endorses what has been said is not substantive —
pass), and the caution is reworded to "directly addressed since they last
spoke".

The regex is built from the character's trimmed name and aliases, longest
first, escaped. Three JS-fidelity decisions were made by measurement rather
than assumption, each pinned by its own oracle vector: JS `\s` is written out
(Rust's `\s` excludes U+FEFF and includes U+0085, both reachable from message
content); JS's `m`-flag `^`/`$` honour CR, U+2028 and U+2029 where Rust's
`(?m)` anchors on `\n` alone, so those three are matched by consuming them
next to the token, which is equivalent for the existence of a match; and
Unicode simple case folding is a recorded divergence from ECMAScript
Canonicalize on a handful of exotic code points (U+212A, U+1E9E, U+017F),
erring toward "addressed", as `crate::mentioned_characters` already does.

The skip-signal differential grows v4's own eight new suite shapes 1:1 plus the
fidelity vectors, and gains a `turnSkipNote` kind driving v4's real
`buildTurnSkipInstruction` — the note bytes had no *direct* differential before
this round (every `build_context_tier3` op passes `turnSkip: None`); the spine
carried them only transitively, through 25 `orchestrator_tier3` rows.
#### 2026-08-20 — test(harness): widen the memories fixture pair to v4's schema vintage; retire the ruled vintage row (P4.52)

_Versions: harness 0.0.512._

The committed `memories-{main,mount}.db` pair predated seven columns v4 has
added since it was baked, and v4's `BaseRepository._update` writes
`$set: validated` — the whole validated entity — so every schema field with a
Zod default is named in the UPDATE and a column the fixture lacks is fatal.
That is why `chatSettings.updateForUser` died on `no such column:
composerEmoji` and why `housekeeping_config_set` has been pinned in
`memories_routes_equivalence` as a RULED VINTAGE ROW (v4's fixture-artifact
500 asserted alongside v5's 200) since the `c8a3cf77` unification.

Measured the gap table by table against v4's live `generateDDL` at
`b8449b3e` (the dump matches the committed `fresh_schema.json` exactly, so no
D23 drift rides along): three tables in the main partition, zero in the mount
partition. New script
`harness/oracle/fixtures/migrate-memories-fixture-columns.ts` applies v4's own
migration ALTERs, verbatim and idempotently, for `characters.archivedAt` /
`archiveFileId` / `archivedAvatarFileId`,
`connection_profiles.multiCharacterPrefill`, and `chat_settings.composerEmoji`
/ `composerUnicode` / `smartTypographySettings` — the migration shape a real
instance carries, per the `migrate-fixtures-pascal-columns` precedent. Every
pre-existing cell in both partitions is byte-identical after the widening
(pre/post row dumps of all 21 tables compared); the only delta is the seven
new columns reading v4's defaults.

Two `generateDDL` columns are deliberately still absent:
`characters.metadata` and `canChooseOutfit` are MANAGED_FIELDS that v4
`delete`s from every DB row on create and update, no v4 migration adds them,
and a real migrated instance does not carry them either.

With the fixture widened, v4 answers 200 and the tripwire fired exactly as
designed; the ruled row retires to a plain `check_body`. Mutation-proven:
dropping `composerEmoji` from a scratch copy puts v4 back on the bare 500 and
reddens the family with the same "expected an error arm" refusal the ruling
documented.
#### 2026-08-20 — fix(harness): normalize() neutralizes any checkout-alias assignment (P4.53)

_Versions: harness 0.0.514._

Repairing the five clobbering headers made the committed recipes read true; it
did not make the class unrepeatable, because the next header written
`W=${V5W:-$HOME/source/quilltap-v5}` brings the bug straight back and nothing
in the driver would notice. So the assignment no longer decides anything:
`normalize()` rewrites every `V5W=` / `WT=` / `V5=` / `W=` assignment statement
to the driver's `--v5w`, and announces the rewrite once per family — a silent
one would hide exactly the rot it neutralizes.

The notice is quiet for a rewrite that changes nothing: a value that already IS
the checkout, and the SELF-referential `V5W=${V5W:-...}` (the sanctioned
convention — the driver injects `V5W`, so the `:-` fallback never fires and the
rewrite substitutes the value it would have produced anyway). 173 families'
normalized scripts change under this rewrite and every one of them is settled,
so the notice fires zero times across the tree today. The CROSS-referential
form is not settled, and that is the whole point: the one line that fires is
the one worth reading.

The match shape is deliberately narrow. `V5W=/some/path cargo test ...` is an
ENV PREFIX, not an assignment, and rewriting it would swallow the command, so
the statement must end after its value (or its trailing `#` comment). A
`;`-joined assignment is caught; a non-alias variable, and one that merely ends
in an alias name, is not. A header that already assigns the checkout under test
is rewritten to the identical value and is not announced.

The alias INJECTION regex is widened in the same pass: it matched `$VAR` and
`${VAR}` but never `${VAR:-...}`, which is the other half of why the clobber
survived — `V5W` was never injected, so the `:-` default fired.

`--self-test` gains the mutation pins: the live defect verbatim (rewritten, and
main's path gone from the whole script), the announcement, the reference form,
two env prefixes that must survive untouched, the `;` form, the no-op silence —
and a durable regression pin that reads every `.rs` and `.ts` header in the tree
and refuses any that defaults one checkout alias from a DIFFERENT one. The
self-referential `V5W=${V5W:-...}` is the sanctioned convention and stays
allowed; the cross form is the clobber, and the backstop above hides it from
every other symptom.

#### 2026-08-20 — fix(harness): the sweep driver refuses a family with nothing to run (P4.53)

_Versions: harness 0.0.513._

`--run <family>` on a family whose recipe extracts to EMPTY stages printed
`OK: <family> recipe ran end-to-end` and exited 0 having executed nothing at
all — the driver's own uniform on the vacuous green it exists to prevent.
Every `exempt` compile-time pin and every `no_oracle` integration arm answered
that way, as did every `committed_corpus` family whose only stage is the
by-hand recording a sweep deliberately skips: 39 families in total, each of
which a gate script could name and collect a green line from for free.

`nothing_to_run` is now a refusal in the class the P4.51 unknown-family error
established — raised BEFORE any stage (nothing below it runs, not even the
stale-oracle deletion), exiting 2, and naming which class the family is in and
what to do about it. `--run-all` records the status on the row, so a batch
containing one is not green, and its results artifact gains a `nothing_to_run`
key enumerating the whole set over the entire checkout rather than just the
batch — the debt was previously recorded nowhere.

`stages_to_run` is now the single source of truth for "will anything execute?",
shared by the refusal and the execution loop, so the two cannot drift apart.
`cmd_run` also distinguishes REFUSED from FAILED: a refusal is the driver
declining to pretend, and calling it a failure sends the operator hunting for a
broken recipe.

None of the 39 is hard recipe rot — the exempt pins and committed corpora have
nothing to regenerate by design, and the no_oracle arms build their state in
process. Roughly 34 of them could gain an explicit scoped `cargo test --test
<family>` run line so `--run` can prove them; that inventory is the next
maintenance pass's, deferred by the work order.

#### 2026-08-20 — fix(harness): five oracle-case headers stop clobbering the driver's injected checkout alias (P4.53)

_Versions: harness 0.0.512._

Five case headers opened their regen recipe with
`W=${V5W:-$HOME/source/quilltap-v5}` and referenced `$W/...`. That reads as a
courtesy default and is a live clobber. The sweep driver injects the checkout
under test by PREPENDING `W="<--v5w>"` (it does so because `$W/` appears in the
body), and the header's own assignment then overwrites it — with
`${V5W:-...}` rather than `$V5W`, because the driver's prepend regex matches
`$VAR` and `${VAR}` but never `${VAR:-...}`, so `V5W` was never injected and the
`:-` default fired.

Measured on `brahma-console-routes.test.ts`, the only header where the alias is
live (its `.rs` family restores its regen from this header): a sweep run from a
lane worktree staged that family's case file AND both `QT_FIXTURE_BRAHMA_*`
databases from MAIN, then exited 0. A marker planted in the worktree's copy of
the case never reached the staged `/tmp` mirror. The other four are dormant
today only because their `.rs` headers are authoritative; they are the same
landmine and are repaired with it.

All five now use the sanctioned convention — `V5W=${V5W:-$HOME/source/quilltap-v5}`
with `$V5W/...` references — so the `:-` default finds the value the driver
injected. Header lines only; no case bodies changed, and the classifier's
report over all 412 families is byte-identical before and after.

#### 2026-08-20 — docs(porting): plan the b8449b3e round — the anti-chorus drift catch-up + two maintenance lanes

_Docs-only change._

Work orders for the next round, planned against v4 `b8449b3e` (two commits
past the `c8a3cf77` baseline; drift-checked at planning — `bugfix` carries
nothing new). P4.D96 ports the `e22f7b36` anti-chorus commit (the
direct-address `isRecentlyAddressed` rewrite in core + the SPA twin, the
turn-skip note bytes, the turn-anchor restructure with the
GROUP_SCENE_DISCIPLINE block, plus a new tier-1 turn-anchor oracle family)
and dispositions `b8449b3e` (jest Sparkplug, bug 83) as NO-PORT. P4.52
widens the committed memories-{main,mount}.db fixture pair to the current
schema vintage and retires the housekeeping_config_set ruled vintage row.
P4.53 closes the three recorded sweep-driver follow-ups (the live `W=`
header self-clobber, the `nothing_to_run` refusal, the `normalize()` alias
neutralization). The three lanes meet nowhere; the baseline moves to
`b8449b3e` at unification. The merge-verb silent-keep sweep is deliberately
deferred a round (its case-file footprint collides with both maintenance
lanes).

#### 2026-08-20 — port(unify): the c8a3cf77 round lands — per-turn summaries, the document-pane toolbar, the sweep smalls

_Versions: core 0.0.591, harness 0.0.511, SPA 0.5.526._

All three lanes unified: P4.D95 (the whole `870a57fa` drift — the per-turn
conversation-summary cadence riding the turn's one embedding), P4.9L2 (the
Document-Mode pane's formatting toolbar, closing m6 row 14b), and P4.51 (the
two `W=` recipe headers + the sweep driver's unknown-family refusal). The
oracle baseline moves to `c8a3cf77`; v4 moved again mid-round (`e22f7b36`,
anti-chorus discipline — the next round's drift), so every gate regen ran
from a detached worktree pinned at `c8a3cf77`.

The §3 review fixed one would-have-shipped divergence: an invalid
recall-config value (wrong enum, non-boolean) now answers v4's 400
"Validation error" instead of silently keeping the stored value — pinned by
three new oracle arms including a writes-nothing composite. The gate's first
by-name family run then caught two more: `housekeeping_config_set` has been
a silent standing red since v4 4.8.2 (a fixture-vintage artifact — v4's
whole-row UPDATE dies on the pre-4.8.2 committed fixture; now a ruled
vintage row with a repair tripwire, the fixture widening being a named
maintenance item), and the composite arm's `storedAfter` was being dropped
by the oracle runner's record shaper (now passed through).

Gate: seven families by name over fresh pinned oracles, zero SKIP; 440 test
binaries / 2,236 / 0; clippy both feature sets; release build; ng 332 files
/ 4,929; full Playwright 232/232 zero skips (the suite grew by the round's
three beats). Both shared version files accumulated (the identical-bump trap
hit harness AND the SPA).

#### 2026-08-20 — feat(settings): the per-turn conversation-summary toggle on the Recall Relevance card (P4.D95)

_Versions: SPA 0.5.523._

The client half of the per-turn conversation-summary cadence. `RecallConfig`
grows `perTurnConversationSummaries`, and Settings → Memory → Recall Relevance
grows v4's third checkbox ("Consult past conversations every turn") with its
body copy carried byte-for-byte. Only the toggled field travels on save — the
server merges over what it stores. Two specs cover it, and a live e2e beat
toggles it on, reloads the page to prove the value came back from the instance
settings row rather than the card's local echo, checks the sibling toggle is
untouched, and toggles it back off.

The new help-doc section v4 wrote alongside this
(`help/memory-recall-relevance.md`) is banked to `p4.9i2` with the rest of the
help family; the bank note is on `m6-screen-parity.md` row 11.
#### 2026-08-20 — feat(memory): per-turn conversation summaries riding the turn's one embedding (P4.D95)

_Versions: core 0.0.591, harness 0.0.510._

Ports v4 `870a57fa`. A new instance-wide setting,
`memoryRecall.perTurnConversationSummaries` (Settings → Memory → Recall
Relevance, off by default, no chat/project/character override), re-runs the
relevant-past-conversations search over the responding character's vault
`Conversation Summaries/` folder on every turn and folds the result into the
consolidated Commonplace Book whisper's `relevantConversations` part. Until now
that list refreshed on three cadences only — the chat-start / character-join
recap, each summary fold, and retrospective turns — so between folds it stood
still while the conversation moved on.

It costs no extra embedding call. `search_memories_semantic` now reports the
vector it embedded through a capture out-param (fired the moment the embedding
lands — before the dimension guard, never for the extra probes, never on the
text-search fallback), and `search_vault_conversation_summaries` accepts it as
`precomputed_embedding`. The proactive pre-compute path threads its vector
through `proactive_recall_task` → the orchestrator → `build_context`, so the
reuse holds on both memory paths; with no vector available the cadence sits the
turn out rather than embedding on its own.

Dedup: the per-turn list filters against the standing fold-posted
`relevant-conversations` whisper (read backwards, stopping at the first match —
the fold refresh sweeps a target's prior whispers when it posts a fresh one),
the retrospective mini-recap now filters against both lists, and the cadence
stands down entirely on the turn the recap runs. The whisper target scope is
computed once and shared, so a list can never be filtered against one scope and
whispered to another. The four list-length ramp constants moved next to the
search so both cadences read the same numbers.

Also here: `get_memory_recall_settings` returns a struct rather than a
`(scope_policy, expand_related)` tuple (a tuple that grows a third field
silently re-orders every destructuring that reads it), and the recall-config
GET/POST verbs carry the third field with v4's byte-exact merge and response
bodies.

Two venue repairs ride along, neither of them a port. `memories-config.test.ts`
now mocks the background-job processor host: under jest the fork of
`child-entry.ts` dies instantly and its crash-and-respawn loop disturbed
whichever case happened to be writing. Each case also gets its own
`QUILLTAP_DATA_DIR`, because one shared `data/quilltap.lock` raced a case's
close against the next case's open. The `memories_routes_equivalence` recipe
header, which produced only one of the two NDJSONs the test needs, now carries
both regen stages and both env vars.

Not fixed here, diagnosed and documented in the family header:
`housekeeping_config_set` is a pre-existing red. v4's `chat_settings` UPDATE
names every column, and the committed `memories-*.db` fixture predates the three
columns v4 added in the 4.8.2/4.8.3 round, so it dies on `no such column:
composerEmoji`. Reproduced against an unmodified oracle case at `c8a3cf77`. The
repair is a fixture-vintage one and belongs to a maintenance order — that `.db`
pair is also read by two e2e specs.
#### 2026-08-20 — test(e2e): walk the document toolbar in both of its hosts

_Versions: SPA 0.5.525._

`salon-documents-flow` gains a beat that seeds a roleplay template carrying one
`wrap` delimiter, hangs it on the chat through `chatUpdate` (reading the chat
back, since a dispatch verb answers 200 on an ignored field), opens a document,
and asserts the pane's toolbar carries exactly ONE delimiter button with v4's
`getDelimiterTooltip` string — no synthesized "Nar", which the composer shows
for the same template. It then presses the delimiter on a selected line, reads
the document's bytes through the toolbar's own source toggle, and presses a
markdown button against the raw textarea. The chat is put back on no template
and the template deleted.

`workspace-document-standalone-flow` gains the mirror: the chat-less pane's
toolbar is present with the markdown inventory and both pickers, carries NO
delimiter buttons, and its Bold reaches the saved bytes. It deletes the document
it created.

Row 14b of `m6-screen-parity.md` is struck, and the `GAP (named, P4.9L)`
paragraph in `document-pane.ts` was retired when the mount landed. One further
divergence the survey turned up is recorded, not fixed: v4's source mode shows
the whole file and hides the frontmatter table, where v5 shows the body and
keeps the table — the saved bytes are unaffected, but a transform at offset 0
lands in a different place.

#### 2026-08-20 — feat(salon): thread the chat's roleplay delimiters into the document pane

_Versions: SPA 0.5.524._

`salon-conversation.ts` passes the template delimiters it already fetches into
`qt-document-pane`, so a Salon-hosted document shows the template's delimiter
buttons — v4's `SalonView.tsx:1577/1630/1847` passing `chat?.roleplayTemplateId`
down through `DocumentPaneBinding`, in v5's resolved-delimiters spelling (the
toolbar takes the resolved entries rather than fetching the row a second time —
the P4.9L mechanism divergence).

`standalone-document-view.ts` deliberately passes nothing, matching v4's
`StandaloneDocumentView.tsx:381`, which mounts `DocumentPane` with no
`roleplayTemplateId`: the chat-less pane's toolbar shows the markdown buttons
and no delimiter rail. The omission is now named at the mount site.

#### 2026-08-20 — feat(documents): the Document-Mode pane wears v4's formatting toolbar

_Versions: SPA 0.5.523._

The pane (`documents/document-pane.ts`) mounts `qt-formatting-toolbar` in a
`.qt-doc-toolbar` row above the editor, where v4's `DocumentPane` mounts its own
`FormattingToolbar` (`DocumentPane.tsx:323-355, :686-693`): the markdown
buttons, the indent controls, the code-block toggle, both character pickers, the
source toggle, and the active roleplay template's delimiter buttons. v4's
`DocToolbar` passes no `narrationDelimiters`, so this toolbar shows no
synthesized "Nar" button — the binding is deliberately absent, and the omission
is pinned against the two recorded v4 vectors.

Wiring: format buttons through `formatCommand`, delimiters through
`applyDelimiterCommand`, both falling to their source-mode transforms when the
raw textarea is showing; pickers insert into the editor, or at the textarea
caret in source mode. Source-mode transforms run through the pane's own
`onEditorInput` seam, so a markdown file's frontmatter block is still recombined
onto the saved bytes.

The toolbar's source toggle and the pane's header button now share one
`showSource` signal through one `toggleSourceMode()` handler, which flushes on
the way out as v4's does (`DocumentPane.tsx:487-494`; v4's `onFlushSave` is its
`onBlur` at both call sites). The header control itself has no v4 counterpart —
it was added when this pane had no toolbar — and is recorded as a divergence.

`document-pane.toolbar.spec.ts` (12 cases) pins the mount shape, the no-Nar
shape against the recorded corpus, the shared signal from both controls, the
flush direction, and both routing branches. Six mutations red.
#### 2026-08-20 — docs(porting): the P4.51 lane gate — both riders discharged, three follow-ups recorded

_Docs-only change._

The lane's gate numbers (fmt, clippy both feature sets, the harness self-tests,
the full workspace suite at 440 / 2,236 / 0, the spelling guard, the driver
self-test, and both carina families end-to-end through the driver from the
worktree), the drift check at `c8a3cf77`, and the three things the lane
deliberately left open: the fifth `.ts` header whose family really does restore
from it (`brahma_console_routes_equivalence`), the driver's empty-stage vacuous
`OK`, and the `normalize()` hardening that would make the header class
unforgeable.

#### 2026-08-20 — fix(harness): the sweep driver refuses an unknown family name loudly, before any stage runs (P4.51 unit 2)

_No crate versions bumped._

`recipe_sweep.py` used to leave an unrecognized family name with a bare
`unknown family: x` on stderr and exit 1 — the same code an ordinary recipe
failure carries, so a typo in a gate script could not be told apart from a real
red. Inside `--run-all` it was worse: the death came AFTER the family banner had
printed, so the results artifact carried no record for the name it died on, and
an unknown `--exclude` name was silently ignored, running the family you meant
to skip.

An unknown family is now an operator error in the driver's own refusal class:
every family name — `--show`, `--run`, and both of `--run-all`'s
`--families`/`--exclude` lists — is validated BEFORE any stage executes, and an
unrecognized one exits 2 naming the family, the closest known spellings
(`difflib`), and the checkout that was scanned (a wrong `--v5w` being the other
way to get here). `--self-test` grew four end-to-end arms that drive the driver
as a subprocess plus two unit arms; both a swallowed exit code and a removed
validation redden them.

#### 2026-08-20 — fix(harness): the two carina recipe headers stop clobbering the sweep driver's worktree injection (P4.51 unit 1)

_Versions: harness 0.0.510._

`carina_query_tier3_equivalence` and `carina_memory_extraction_tier3_equivalence`
declared their v5 checkout as `W=${V5W:-$HOME/source/quilltap-v5}`. The sweep
driver injects the checkout it was asked to test by PREPENDING `W="<path>"` to
the script, so the header's own assignment ran second and overwrote it — with
`$HOME/source/quilltap-v5`, i.e. main — every time the driver ran either family
from a worktree. The run still went green, against main's case file and main's
fixture builder: the failure mode is invisible. Both headers now use the
documented immune spelling (`V5W=${V5W:-…}`, where the injected `V5W` wins the
`:-` default) and reference `$V5W`; nothing else in either header moved.

Proven by running both families end-to-end through the driver from the lane
worktree (fresh fixtures, fresh oracles, both diffs green), and by a
both-directions provenance probe: a marker appended to the worktree's copy of
`carina-query-tier3.test.ts` is ABSENT from the staged `/tmp` mirror under the
old spelling and PRESENT under the new one.

#### 2026-08-20 — docs(porting): plan the c8a3cf77 round — P4.D95 ∥ P4.9L2 ∥ P4.51

_Docs-only change._

Drift-checked v4 at planning: two commits past the `9125f492` baseline —
`870a57fa` (per-turn conversation summaries with embedded vector reuse, a
behavior change on the ported memory/context spine; no schema change) and the
version-only `c8a3cf77` (NO-PORT). Three work orders written, all disjoint:
P4.D95 (the whole drift — the new `memoryRecall.perTurnConversationSummaries`
setting end-to-end, the `captureQueryEmbedding` hook, `precomputedEmbedding`
on the vault summary search, the proactive vector thread, and the
build-context per-turn cadence with its dedup/stand-down rules; six harness
families named), P4.9L2 (the DocumentPane formatting toolbar — the m6 §4
row-14b named gap, SPA-only, two live beats), and P4.51 (the two `W=`
self-clobbering carina recipe headers + the sweep driver's
exit-0-on-unknown-family wart). Round-plan paragraph added to phase-4.md; the
oracle baseline moves to `c8a3cf77` at unification.

#### 2026-08-19 — port(unify): P4.50 lands — the DbError::Key catch-all split (finding #96 FIXED)

_Versions: core 0.0.590, harness 0.0.509, host 0.0.74, web 0.0.77._

The solo stacked lane unified onto main; the oracle baseline stays `9125f492`.
`DbError::Internal(String)` (bare-message Display) replaces the catch-all use
of `DbError::Key` at 243 of its 246 construction sites — the two genuine
key-derivation wraps and the Display arm keep the prefix, held there by an
executable census guard (`db_error_key_guard`). No observable byte moved: no
`From<DbError>` shim matched `Key` explicitly, so the new variant inherits
every mapping, and `db_error_response` still answers `ErrorKind::Internal`.
The `system_restore_state` leaked-prefix mask is retired, so restore warnings
now byte-compare against v4's whole sentences. The §3 review audited the
migration mechanically (every hunk a pure variant rename; the string-literal
multiset moved by exactly one literal, the retired prefix-strip helper whose
rendered bytes are identical) and found no blocking issues.

Gate: fmt/clippy clean both feature sets; release build; 440 test binaries /
2,236 / 0 with the restore oracle regenerated fresh at the pin; both moved
families by name zero SKIP; SPA 331 files / 4,915 / 0; full Playwright
229/229 zero skips. Deferred loud: the three per-domain taxonomy candidates
named not built; the live combined.log look on a real failed turn joins the
dogfood queue.

#### 2026-08-19 — fix(core): a failed provider call stops claiming key derivation (P4.50, finding #96)

_Versions: core 0.0.590, harness 0.0.509, host 0.0.74, web 0.0.77._

`DbError::Key`'s `Display` prepends `"key derivation failed: "`, which is a
claim about the cause of the failure. The variant was also the crate's
general-purpose message carrier: of 246 construction sites workspace-wide,
exactly two derived a key. Every other one printed a cipher-flavoured lie in
front of its real sentence — including the one an operator meets after a
failed turn, which read `key derivation failed: primary stream failed:
HTTP 500 …` in `combined.log`.

Adds `DbError::Internal(String)`, whose `Display` is the bare message (the
shape `StoreUnavailable` already uses), and migrates all 243 non-key
construction sites onto it. The two genuine sites — `runtime::Db::open` and
`Writer::open_writable`, both wrapping `dbkey::pepper_b64_to_key_hex` — keep
`Key`, so the prefix keeps its diagnostic value instead of becoming noise.

Nothing observable moves on any v4-pinned surface: no `impl From<DbError>`
shim matched `Key` explicitly, so `Internal` inherits every catch-all arm,
and `db_error_response` maps it to `ErrorKind::Internal` exactly as before.
The one place the prefix reached a v4-comparable string was the restore
warning — `system_restore_state` carried a `LEAKED_PREFIX` strip so the rest
of the sentence could be compared. That strip is retired: those warnings now
byte-compare against v4's whole sentence, which is strictly stronger.

A new harness guard (`db_error_key_guard`) walks `crates/**/*.rs` and holds
every `DbError::Key(` occurrence against a census, so the catch-all cannot
silently regrow; the allow-list is the census. Two unit pins cover both
variants' `Display` bytes and two more cover the api-body surface.

#### 2026-08-19 — port(unify): the 9125f492 drift round lands — bugs 81/82 + the Lantern uncensored target (P4.D93 ∥ P4.D94)

_Versions: core 0.0.589, harness 0.0.508, host 0.0.73, SPA 0.5.522._

Both drift lanes unified onto main; the oracle baseline moves to `9125f492`
and the drift debt is cleared. P4.D93 absorbs v4's fix for the two bugs this
port filed from its own 2026-08-19 dogfood walk: a provider may now accept an
API key without requiring one (`acceptsApiKey` on the manifest substrate, the
predicate pair with one fallback home, the `resolve_connection_profile_api_key`
gate+lookup composite at both Brahma sites with a dangling id refusing loudly
even where the key is optional, and the settings SPA offering the
OpenAI-Compatible key unstarred and optional), and local endpoints fold their
leading system-message run at request-build time (Ollama + OAC builders only;
DeepSeek's three blocks survive on the wire as the recorded regression guard;
request-envelopes corpus 257 → 263 rows, all pre-existing rows byte-identical).
The spine measurement answered that v5 never had bug 81's server-spine half —
the host key scan is capability-blind — and that reading is pinned. P4.D94
absorbs the Lantern change: the story-background crafter selects candid vs
concealment intimacy guidance per call (the prompt split into seven generated
constants with the concealed path proven byte-identical at 5114 UTF-16 units),
the flag threads through the empty-response retry unchanged, and a post-hoc
moderation reroute re-crafts the prompt candidly for its target through a new
seam on the shared reroute machinery (the avatar handler passes the no-op; its
family regenerated green as the guard), with five new dangerous-chat fixture
cases and seven red mutation proofs across the lane.

Gate: fmt/clippy clean both feature sets; release build; 439 test binaries /
2,231 / 0 with the round's env block over oracles regenerated fresh at the
pin; the eight moved families re-run by name, zero SKIP; SPA 331 files /
4,915 / 0; full Playwright 229/229 zero skips. The §3 unification review read
the whole combined diff and found no blocking issues; the round record in
`status-log.md` carries the details, including the unifier's own
caught-and-amended conflict-marker incident.

#### 2026-08-19 — docs(porting): the P4.D93 lane gate record

_No crate versions bumped._

Gate numbers for the bug-81/82 lane: 439 test binaries / 2,227 passed / 0
failed with the lane's env block, clippy clean in both feature configurations,
the six moved differential families re-run by name over oracles regenerated
fresh at `9125f492` with zero skips, SPA 331 files / 4,915 tests, and the full
Playwright suite 229 passed / 0 failed / 0 skipped with the bug-81 walk live.

#### 2026-08-19 — test(e2e): walk the optional OpenAI-Compatible key, and repair two self-clobbering recipe headers

_Versions: harness 0.0.506, SPA 0.5.522._

The bug-81 live walk joins the existing settings wizard beat rather than
standing alone: it needs exactly what that beat has just produced, a saved
OpenAI-Compatible profile to attach a key to. Open Add API Key — the provider
list now offers OpenAI-Compatible and still omits Ollama — create a key of that
provider, edit the wizard's profile, find the key field present with an
unstarred label and the `None — the endpoint needs no key` placeholder, attach
the key, save, and re-open to see it held.

Running the Brahma one-shot family through `recipe_sweep.py --run` went red for
a reason unrelated to the port: both Brahma headers opened with
`W=${V5W:-$HOME/source/quilltap-v5}`, and the driver injects the worktree path
as `W` before that line, so with `V5W` unset the header clobbered the injection
and rebuilt the fixture from main's spec file. Converted to the documented
`V5W` spelling, where the injected value survives the `:-` default. Two more
families carry the same broken spelling and are recorded for a maintenance pass.

#### 2026-08-19 — fix(settings): the OpenAI-Compatible key field, offered and optional (v4 bug 81, SPA)

_Versions: SPA 0.5.521._

The client half of v4 `9125f492`. A new `api-key-support.ts` holds the two
predicates as a pure module with no other imports, the v5 twin of v4's
`lib/llm/api-key-support.ts`, so the Add-New-API-Key filter, the profile form's
key field, and the form's outbound gate all read the same question through the
same function rather than each testing `requiresApiKey` by hand.

The Add-New-API-Key provider list is filtered on whether a provider *may* hold a
key; OpenAI-Compatible now appears in it and Ollama still does not. The profile
modal shows the key field whenever the provider takes one, starred only when it
demands one, with the placeholder option reading `None — the endpoint needs no
key` in the optional case. `outboundApiKeyId` — the one gate every outbound site
reads — now refuses on "accepts no key" rather than "requires none", which is
what actually let the attached key leave the form.

The Fetch Models gate and the Connect guard stay on `requiresApiKey`
deliberately, as v4's do: a provider that merely accepts a key must not be
blocked for lacking one. The wizard and the embedding-profile modal are
untouched — their question is requires-shaped and v4 left them alone.

Seven specs mirror v4's added cases: its one api-key-modal case and its five
`profile-modal-optional-api-key` cases, plus the starred-label counterpart.

#### 2026-08-19 — fix(providers): fold the leading system run for local endpoints (v4 bug 82)

_Versions: core 0.0.587._

Ports v4 `9125f492`'s `collapseLeadingSystemMessages`. The context builder
deliberately emits the head of a turn as up to three consecutive `system`
messages so a cache breakpoint on the first survives churn in the others. A
hosted provider accepts that; a local runtime applies the model's own chat
template, and the Qwen family — plus several Llama- and Gemma-derived templates
— raises on any system message after index 0. The opening greeting sends one
block and worked; every turn after it was refused with a 500.

The repair is at request-build time, leaving the context assembly and its
caching design untouched. The fold takes the maximal leading run of `system`
messages, drops empty contents before joining the rest with a blank line, keeps
the first block's other keys, and hands back an array of fewer than two leading
system messages unchanged and un-reallocated.

v5 has no provider subclassing, so v4's `acceptsRepeatedSystemMessages` property
has no counterpart: v4's one override is on the local-endpoint OpenAI-Compatible
plugin, which is exactly v5's `OpenAiCompatible` kind, and the hosted subclasses
have their own builder functions here. Both v5 folds are therefore
unconditional, applied at the two sites v4 folds at, and every hosted builder is
byte-identical on the wire because it never calls the fold at all.

The request-envelope corpus gained six rows recorded from v4's real plugins: a
three-leading-system turn for Ollama and for OpenAI-Compatible on both the
streaming and non-streaming paths, and the same turn for DeepSeek — the
regression guard, with all three system messages still on the wire. The 257
pre-existing rows are byte-identical. Request prefix hashes were re-checked at
the pin rather than assumed unaffected, and are unchanged.

#### 2026-08-19 — fix(providers): forward the key a profile attached, and refuse a dangling one (v4 bug 81, server)

_Versions: core 0.0.586, host 0.0.73._

Ports v4's `resolveConnectionProfileApiKey`. Four v4 paths gated the key
*lookup* on `requiresApiKey`, so an OpenAI-Compatible profile's attached key
would never have reached the wire even once the form could hold it. The v5 twin
lives in `services::api_key_service` beside the two capability predicates, which
moved there from their two duplicate homes (`api::settings` and the Brahma
console) — one v4 function had become three copies.

The order of its three gates is load-bearing: a provider that accepts no key is
never looked up, so a stale row cannot fail its turn; a missing `apiKeyId`
refuses only where a key is required; and a present `apiKeyId` is always
followed, so a dangling one refuses even where the key is optional — the user
attached it on purpose, and going out unauthenticated instead is the
silent-wrong-answer kind of failure.

Both Brahma sites route through it, keeping their byte-different sentences (the
one-shot service's are lower-case, the orchestrator's are not — a pre-existing
v4 asymmetry). The one-shot tier-3 corpus gained three arms recorded from v4's
real code: a dangling key on an accepting provider refuses, an accepting
provider with no key proceeds, and a keyless provider ignores a dangling stale
id. The orchestrator's are composition-level tests over its committed fixture.

The chat-message spine needed no port: v5 resolves keys host-side through a
provider scan with no capability gate on it, so a stored OAC key has always been
forwarded, and the manifest's `auth` scheme is what keeps a keyless endpoint
bare. That reading is now pinned and recorded on the seam.

#### 2026-08-19 — feat(providers): a provider may accept an API key without requiring one (v4 bug 81, substrate)

_Versions: core 0.0.585, SPA 0.5.520._

`ProviderConfigRequirements` gains an optional `acceptsApiKey`, ported from v4
`9125f492`. `requiresApiKey` was answering two questions — "must this provider
hold a key?" and "may it?" — which are the same question for a wholly hosted or
wholly local provider and genuinely different for OpenAI-Compatible, whose one
plugin serves an unauthenticated llama.cpp on localhost and a hosted endpoint
behind a bearer token.

`ConfigRequirements::accepts_api_key` is the single home for the fallback rule:
omitted means "the same answer as `requiresApiKey`", so the eight manifests that
do not declare it keep exactly their present behavior. The manifest generator
extracts the field from the plugin config only when the plugin declares it,
mirroring v4's own `manifest.json`; regenerating all nine manifests changes
exactly one line, in `openai_compatible.json`.

`provider_list()` emits `acceptsApiKey` into `configRequirements` immediately
after `requiresApiKey` and only when the manifest carries it — v4's route passes
`plugin.config` through whole, so the key is present exactly where the plugin
declares it. The providers-listing oracle now spreads that whole config object
instead of hand-picking six fields: the hand-picked comparand was blind to any
config key v4 adds, which is exactly how `acceptsApiKey` would have passed green.
#### 2026-08-19 — port(lantern): a moderation reroute re-crafts the story prompt candidly

_Versions: core 0.0.586, harness 0.0.507._

v4 `decd8ef9`, second half. When an image provider post-hoc rejects a generated
image for content moderation, the Concierge reroutes to the configured
uncensored image profile — and was resending the prompt verbatim, cinematic
concealment and all, to a provider that never asked for it. It now re-crafts
candidly for the reroute target first, re-running the step-9b
character-enumeration pass over the replacement, and is best-effort: any failure
keeps the prompt already in hand so the reroute still produces an image.

v4 does this inline in the story handler's catch. v5 shares the reroute
machinery between the story and avatar handlers, so it lands as a seam: a
`RerouteRecraft` trait on `image_job_common`, invoked with the RESOLVED target's
provider once a reroute exists and before the profile/orientation resolution —
exactly where v4's block sits. The story handler passes `CandidRecraft`, guarded
on `!uncensored_image_target` (an already-candid prompt is left alone), crafting
through v4's `uncensored ?? cheap` selection into a context carrying the
target's provider and `uncensored_image_target: true`, re-running the
enumeration pass over the hoisted non-participant list, and logging v4's info
and warn sentences with its fields. The avatar handler passes
`NoRerouteRecraft`: v4's avatar path is unchanged. All three downstream
consumers moved onto the re-crafted base prompt — the reroute prompt-hint concat
and both `llm_logs` request projections.

v5's crafter returns a result rather than throwing, so v4's two best-effort arms
(a soft empty result and a thrown error) collapse into one `None` return.

Three tier-3 cases cover it, over an image profile whose `blocked-model` throws
a moderation error: the re-craft (two craft calls, flags false then true, the
reroute prompt carrying the candid text AND the re-run enumeration), the
re-craft failing (the reroute reuses the concealed prompt and still produces an
image), and an already-candid prompt (ONE craft call, no re-craft). Four
mutations proven red: skipping the enumeration re-run, ignoring the re-craft at
the prompt site or in the log projections, and dropping the already-candid
guard. `avatar_job_tier3_equivalence` was regenerated fresh at the pin and re-run
by name — no seam leak.

#### 2026-08-19 — port(lantern): the story-background crafter picks its intimacy guidance per call

_Versions: core 0.0.585, harness 0.0.506._

v4 `decd8ef9`, first half. The story-background crafter's "DEPICTING INTIMATE
OR UNCLOTHED STATES" section translates narrative nudity into cinematic
concealment (drapery, silhouette, foreground occlusion). That exists to clear
image-provider moderation, but it was baked into the system-prompt constant and
applied unconditionally — so a dangerous-marked chat with a Concierge
uncensored image profile got accurate appearance text (the appearance sanitizer
already steps aside for exactly that case) fed into a crafter that then draped a
sheet over it, to clear moderation the target provider does not perform.

The prompt is now assembled per call. `gen-image-scene-prompts.mjs` learned
v4's seven-constant shape (a shared head and tail, two intimacy blocks, two
worked examples, the closing line) and `prompt_text.rs` was regenerated from it;
`build_story_background_prompt(uncensored_image_target)` performs v4's
five-element `"\n\n"` join. The concealed (default) path is byte-identical to
the pre-split constant — checked against the previously generated constant's
exact bytes, and pinned at its old 5114 UTF-16 code units. Four Rust unit tests
mirror v4's five prompt-content cases, assertion substrings verbatim.

`StoryBackgroundPromptContext` gains `uncensored_image_target` (the
`generate_image` crafter and the avatar builder take no flag, as in v4), and the
story-background handler computes `is_dangerous_chat &&
has_uncensored_image_provider` — the same pair the appearance sanitizer reads,
so the two layers agree. The empty-response retry carries the flag unchanged:
swapping in the uncensored TEXT profile does not change the IMAGE target.

The tier-3 corpus grew two cases: a dangerous chat WITH an uncensored image
profile (candid) and a dangerous chat WITHOUT one (still concealed — the `&&`
pin). A chat-settings row is per-user and the corpus has one user, so a case can
carry its own `dangerousContentSettings` bag, which both sides apply to their
fresh copy before the run. The oracle's completion mock now branches on the
candid marker in the system message, so the selected variant reaches the image
key and both `llm_logs` projections. Three mutations proven red: dropping either
conjunct of the signal, and hard-coding the retry's flag to `true`.

#### 2026-08-19 — docs(porting): plan the `9125f492` drift round — orders P4.D93, P4.D94, P4.50

_Docs-only change._

v4 shipped three commits past the `c6ff8051` baseline, two of them fixing the
bugs this port filed from its own 2026-08-19 dogfood walk. The round is
planned as two parallel drift lanes plus one stacked maintenance lane, each
with a fresh survey (both sides, dated, file:line):

- **P4.D93** (`docs/developer/porting/work-orders/p4.d93-oac-api-key-and-system-fold.md`)
  — v4 `9125f492`, bugs 81/82: the `acceptsApiKey` manifest flag and
  predicate pair, the shared connection-profile key resolver at the two
  Brahma sites (the help-chat leg banked to `p4.9i2`; the chat spine's
  divergent key seam measured, not assumed), the leading-system-message
  fold in the Ollama and OpenAI-Compatible request builders only (with a
  DeepSeek no-fold regression row in the request-envelopes corpus), and
  the settings SPA half.
- **P4.D94** (`.../p4.d94-lantern-uncensored-target.md`) — v4 `decd8ef9`:
  the story-background crafter's candid-vs-concealment intimacy selection
  through the prompt-text generator (concealed path pinned byte-identical),
  the handler flag + retry carry, and the moderation-reroute candid
  re-craft as a story-only hook on the shared reroute machinery, with the
  story fixture grown its first dangerous-chat coverage.
- **P4.50** (`.../p4.50-db-error-kind-split.md`) — dogfood finding #96:
  split the `DbError::Key` catch-all (246 construction sites) to a
  bare-message variant with a per-site census and a regrowth guard; runs
  stacked after the drift lanes unify because it touches their files.

Also fixed in passing: `phase-4.md`'s standing regen note was stale at
`979652a9` (the `c6ff8051` unification updated the candidate list but
missed that paragraph), and the phase plan now records the round.

#### 2026-08-19 — build(spa): `npm run build` / `npm test` actually return the shell

_SPA 0.5.519._

`ng build` and `ng test` complete their work, print their summary, and then
never exit, so anything chained behind one silently never starts — the failure
mode `.claude/commands` and the memory notes have been working around for
months. Measured this time rather than worked around: after the completion
line `process.getActiveResourcesInfo()` reports exactly
`["PipeWrap","PipeWrap","ProcessWrap"]` and nothing else — the
`esbuild --service=... --ping` child and its two stdio pipes, still ref'd — and
killing that one child makes `ng build` exit instantly with code 0. It is not
esbuild's bug (its shim unrefs the service child at spawn and re-refs it only
during an in-flight operation); `@angular/build` 21.x defers `result.dispose()`
into a generator `finally` on the non-watch path. Fixed upstream in
`@angular/build` **22.0.4**, which disposes eagerly before yielding, and never
backported — 21.2.21's `src/` tree is byte-identical to 21.2.19's, and taking
22.x means Angular 21 -> 22 plus TypeScript 5.9 -> 6.0. That upgrade is tabled
as its own lane.

New `apps/web/tools/ng-run.mjs` (Node, zero dependencies, so it works in the
Docker image too) runs ng as a detached child, streams its output through
untouched, waits for the command's terminal marker — Angular's own
`Application bundle generation complete./failed.`, or vitest's `Duration` line
plus the `Test Files` verdict — then reclaims the shell and exits with the real
status, killing the process group so the esbuild grandchild goes with it. All
four paths measured: successful build -> 0, broken build -> 1, passing tests ->
0, failing tests -> 1, zero stray processes each time. `--watch` and `serve`
pass straight through untouched. `npm run build` and `npm test` now route
through it (`build:raw` / `test:raw` remain as escape hatches), which also
removes a latent hang from the Dockerfile's `RUN npm run build`. `/dogfood`,
`/unify`, `/carryout`, the SPA README and `docs/developer/running.md` all point
at the npm scripts and say plainly never to invoke `ng` directly.

#### 2026-08-19 — docs(dogfood): the /dogfood skill drives the walk itself, pausing only for the human build + unlock

_Docs-only change._

The `/dogfood` command is restructured around a new division of labor: Claude
executes the walk through the Browser pane instead of scripting it for the
human. The research phase now emits a living checklist document under
`docs/developer/porting/dogfood-walks/` with per-step owner
(`CLAUDE`/`HUMAN`), status, gesture, and verification method; the skill then
pauses with verbatim build (`cargo build --release`, `ng build`) and rsync
instructions for the `~/qt-dogfood-friday` refresh, launches `quilltap-web`
itself after confirmation (passphrase unlock stays the human's step), and
drives every `CLAUDE`-owned test — deferring only expensive or
judgment-bound steps to a short human remainder at the end. Per-step result
recording, the diagnose-against-v4 fix loop, and the close-out trail
(findings rows, status-log record, 💸 queue update) carry over from the old
form.

#### 2026-08-19 — docs(porting): the c6ff8051 round record — unified, baseline moved, both lanes closed

_Docs only._

The `c6ff8051` drift round (P4.D91 ∥ P4.D92) unified onto main: v4's
bugs-78/79 convergence absorbed (pin retirements, the five named import
warnings, the preflight refusal, the equipped-slot coercion point) and the
bug-80 project story background landed through the workspace backdrop with
a live e2e beat. The unification review added the JS template-literal
interpolation fix. Gate: clippy both feature sets; release build; 439 test
binaries / 2,208 / 0 with the round's env block; the four affected families
regenerated fresh from a PINNED `c6ff8051` worktree (the v4 checkout moved
to the `release` branch mid-unify — the first, unpinned regen went red and
was discarded); ng test 331 files / 4,908; ng build clean; full Playwright
229/229 zero skips. The oracle baseline moves to `c6ff8051`.

#### 2026-08-19 — fix(import): interpolate a malformed item's name the way v4's template literal does

_Versions: core 0.0.584._

The five named import warnings quote the item's `name` read off the raw
payload, and the arm fires exactly when the item failed to parse — so the
field can be any JSON shape, or absent. v4 renders it through a JS template
literal (`undefined`, `null`, `7`, `a,b`, `[object Object]`); the port read
it as a string and rendered anything else as empty. The five warning arms
now share one helper with the JS interpolation semantics, pinned by a unit
test. The create-path name fallbacks are untouched — they feed written
data, not a sentence. Caught at the unification review.

#### 2026-08-19 — fix(import): name the five silent per-item failures and the preflight refusal (bug 79)

_Versions: core 0.0.583, harness 0.0.505._

Ports v4 `275cd7bc`'s second half. Five import arms — tags, roleplay
templates, and the three profile kinds — logged a per-item failure and
dropped the item without putting anything in the result's `warnings`, so a
user whose items vanished had no way to know. They now push v4's exact
`Failed to import <kind> "<name>": <error>` sentence, from both the
per-item catch and the typed-deserialization refusal that stands in for
v4's create-time validation. The preserve-ids preflight, which aborts the
whole import, likewise answered `success: false` with an empty warnings
array; it now pushes `Import refused before anything was written:
<message>`, so a refused import always says why.

v4's mechanism for the other half of its fix — an AsyncLocalStorage scope
that suspends the repository layer's degrade-to-fallback behavior for the
duration of an import — has no counterpart here and needs none: this port's
reads are typed results that propagate by construction, which P4.48 already
made the import honor at every site.

Two new differential arms carry the change. `execute_named_item_failures`
imports five deliberately malformed items, one per newly-named arm, since
no committed archive can express an item that fails to import — a real
export imports. `execute_preserve_ids_unvalidatable_row_refuses` plants a
destination row that SQLite returns happily and v4's schema rejects, which
is the one plant that reaches v4's strict scope: v4 refuses naming the
validation failure, this port refuses naming the collision it can still
see, and neither writes a row.

The P4.48 `preview_planted_unreadable_tags_table` divergence was
re-measured and its recorded mechanism corrected. It is not `safeQuery`
swallowing a read error: v4's `ensureCollection` rebuilds a dropped table on
first repository access, so the read succeeds against a table v4 has just
created. The oracle now emits whether the table came back, and the arm
asserts the asymmetry on both engines, so the claim is a comparand rather
than a comment.

#### 2026-08-19 — fix(wardrobe): normalize equipped slots on the way out of the column (bug 78)

_Versions: core 0.0.582, harness 0.0.504._

Ports v4 `275cd7bc`'s first half. `chats.equippedOutfit` is unconstrained
JSON and slots were added over time, so a chat row written before the hair
slot carries four keys. `get_equipped_outfit` now maps every character's
entry through the slot normalizer, which makes it the single coercion point
out of the column: the chat-outfit API, the previous-chat outfit carry, and
`set_equipped_outfit`'s read-modify-write all see five slots, and a write
that re-reads the state repairs its siblings' bags on the way through.
`wardrobe_create`'s hand-rolled read of the column moved to the repository
for the same reason.

This port was never exposed to the crash v4 fixed — its slot reader
defaulted a missing key to empty from the start, which is what the retired
`legacy_four_key_equipped` divergence pin recorded. That pin fired on the
first regeneration, as designed, and is now a plain equality: v4 completes
the avatar job and writes the avatar, as this port always did. New unit
tests diff the normalizer against v4's `normalizeEquippedSlots` case for
case, including the malformed-bag salvage and the non-object shapes.
#### 2026-08-18 — fix(prospero): keep v4's legacy per-view background layer on the routed project page

_Versions: SPA 0.5.518._

v4's fix for bug 80 added the backdrop report without removing the older
per-view CSS variable, because the project page still renders outside the
workspace on its own route, where the `::before` layer is the only thing that
paints. Quilltap-v5 serves that route too whenever the workspace-tabs flag is
off, so it carries the variable as well: set when a background resolves,
absent when none does, so the `:not([style*=…])::before` rule keeps the layer
hidden.

Inside the workspace this is inert by design — the layer is suppressed there
in favor of the single arbitrated backdrop, exactly as in v4.

#### 2026-08-18 — test(e2e): walk a project's story background onto the workspace backdrop

_Versions: SPA 0.5.517._

A live browser beat for v4 bug 80's fix. It deep-links straight onto a
project — the case v4's two competing reporters used to lose — sets "Latest
chat background", and checks that the arbitrated backdrop paints the image
resolved from the project's own chat, over the real dispatch and the real
byte route. It then confirms the per-view background layer really is
suppressed inside the workspace, so the pixels came from the reporter and
not from a surviving `::before`. Finally it switches the project to "Theme"
and asserts the backdrop is absent — Quilltap-v5's honest shape, since it has
no subsystem image to fall back to.

The display mode is driven through the UI rather than seeded in SQL. The
first run of this beat found out why: `backgroundDisplayMode` is a
document-store overlay property, so the `projects` column is shadowed by the
project's `properties.json` and a direct table UPDATE is invisible to every
reader. Driving the select exercises the real write path and leaves the
shared test instance in the mode it shipped with.

#### 2026-08-18 — fix(prospero): a project's story background reaches the workspace backdrop (v4 bug 80)

_Versions: SPA 0.5.516._

A project set to "Latest chat background" (or "Project background", or a
static upload) showed nothing. The setting saved and the server returned the
right image — the page simply never painted it.

The tabbed workspace replaced each view's own background layer with one
arbitrated backdrop that views must report to, and suppressed the per-view
`::before` layer inside `.qt-workspace`. The project detail was never
converted, so its background reached the screen by neither route. It now
reports the resolved image to the backdrop registry under its own tab id and
clears the entry when the view goes away, with v4's passive-poll gate
(poll only when the mode is not "Theme"; the fetch itself always runs,
because the server is what resolves "Theme" to no image).

Two deliberate differences from v4's fix, both recorded in the component.
v4 falls back to the theme's Prospero subsystem image for "Theme" mode;
Quilltap-v5 has no subsystem-background machinery at all, so that mode
reports nothing and the backdrop is absent — the standing divergence, whose
newest instance this is. And v4 had to move the projects list's own
subsystem reporter into a shell that unmounts while a detail is shown,
because two live reporters raced over one tab key; the v5 list reports
nothing, so the detail is already the only reporter and the deep-link case
cannot lose.

#### 2026-08-18 — fix(prospero): read a project's resolved story background (bug 80, part 1)

_Versions: SPA 0.5.515._

`ProjectBackgroundDto` described a body the server does not send. The core
verb `project_background_get` answers v4 `handleGetBackground`'s bare
`{backgroundUrl, displayMode, sourceChatId?}`, and `backgroundUrl` is
already the id-keyed byte route (`/api/v1/files/{id}`) — the client type
declared `{url, sourceChatId}` instead, so `fetchProjectBackground` read a
key that is never present and resolved every project's background to null.
Nothing consumed it yet, which is why it went unnoticed.

The DTO now matches the wire, the resolver maps all three fields, and
`projectKeys` gains a `background(id)` sibling of the detail key (spelled in
this file's own `['projects', <kind>, id]` idiom, so the `projectKeys.all`
prefix still covers it). This is the read half of bug 80's port; the
reporter that puts the value on screen follows.

#### 2026-08-19 — docs(changelog): restructure into per-commit headers, split by month

_Docs-only change._

Restructured this changelog. The single flat "Recent Changes" body — 1,479
paragraphs, ~19,400 lines, recording nearly every commit since Phase 1 with
no headers at all — is now one H4 section per originating commit, derived
mechanically from `git blame`: each header carries the short commit hash,
the commit date, and the commit subject, and the line beneath it records
the crate versions that commit bumped (or notes a docs-only change). The
file is split by month, with June and July 2026 archived under
`docs/changelog/` (July in two halves; it alone held 895 entries). No
paragraph text was altered: an order-insensitive line diff against the old
file came back identical. Going forward, new entries use the same header
shape but without the hash — a commit's hash doesn't exist when its entry
is written, and amending would invalidate it — per the amended
`.claude/commands/commit.md` §7 and `unify.md` §6.

#### `6334b193` — 2026-08-18 — docs(readme): a front page that says what this repository is

_Docs-only change._

Planned the `c6ff8051` drift catch-up round and wrote its two work orders.
v4 moved three commits past the `979652a9` baseline: a docs-only commit
filing bugs 78/79 (the port's own findings), the fix for both (v4 converging
onto behavior v5 already pinned — the equipped-slot normalization and strict
import reads, plus new named import warnings v5 must adopt), and the bug-80
fix (a project's story background reaching the workspace backdrop — a
display half v5 never ported). P4.D91 (`work-orders/
p4.d91-import-wardrobe-convergence-server.md`) owns the crates side:
retiring the fired convergence pins, the five import warning arms plus the
preflight warning byte-for-byte, and the healed-reader sweep. P4.D92
(`work-orders/p4.d92-project-backdrop-spa.md`) owns the SPA side: the
project background query, the detail-tab backdrop reporter in v4's fixed
one-reporter shape, and a live e2e beat, with the theme-mode subsystem
fallback as a measured tier-2. The bugfix branch gained only the 4.8.4
version marker (no port).

Gave the README a real front page. It had been a short banner carrying v4's
badges — v4's GitHub release, v4's Docker Hub tag, v4's npm package — none of
which this repository produces. Those three are replaced by a port-status
badge, a stack badge, and one badge that names v4 as the version you can
actually install, and the page now says in its first paragraph what it is:
the unfinished native port, with the Quilltap you can run today living in
quilltap-server.

The rest states the terms. v5 is feature-identical and API-identical to v4 at
release, an existing instance directory opens as it stands with no export or
conversion step, and this repository becomes the project when v4 makes its
last release. Two exceptions are called out rather than left to be discovered
later: v4's plugins will not run in v5 and no compatibility shim is planned,
because a Rust binary has no Node process to load an npm package into — a
replacement extension system is being designed and it will not be JavaScript
— and the v3/v4 virtual-machine appliance is retired, with a locked-down
Docker deployment as the sandbox and the reasoning given in full. The page
also explains how each ported unit is checked against v4's running code
rather than against a description of it, names the four ways in at first
release (macOS, Windows, Linux, Docker) and mobile as unpromised but
deliberately unblocked, credits the libraries the new stack is built on, and
points readers at quilltap.ai, the Folio, and the porting docs. It answers
"should you use this today" with "no."

#### `07b87e84` — 2026-08-18 — docs(porting): rule the product version scheme and order PB1

_Docs-only change._

Settled how v5 will be versioned and scheduled the work. The first real
release is 5.0.0; until then the product version is the semver prerelease
5.0.0-dev.N, one canonical string in the workspace manifest with every
platform form derived from it. Today no such version exists: the About
badge says so outright, a single build answers with four different numbers
depending on whether you reach it through the server, the desktop shell or
the CLI, and this changelog has no version anchors. The change is ordered
as PB1, the first pre-beta work order, to run when parity work is winding
down and before any build reaches an outside tester. Nothing ships yet;
signing, publishing, the updater, multi-arch Docker and cross-platform CI
stay deferred.

Removed a stale version badge from the README. It showed a v4 version
number and linked to a root package.json this repo does not have.

#### `674ee84a` — 2026-08-18 — docs(porting): the 979652a9 round record — unified, baseline moved, all five orders closed

_Docs-only change._

Unified the 979652a9 drift round: five parallel lanes, all closed. The
wardrobe gains its fifth slot — hair, holding a hairdo rather than hair —
threaded through every tool, prompt, avatar branch, import, export, and
dialog, with an empty hair slot deliberately saying nothing anywhere (empty
means unstyled, never bald). Importing a .qtap with composite outfits now
remaps their component references, so imported outfits arrive whole instead
of hollow. An API key no longer follows a connection profile onto another
provider, and an already-poisoned profile heals on its next ordinary save.
The image-generation notice above the composer now owns its own lifetime: it
appears when generation starts, reports the outcome, dismisses itself after
six seconds, and carries a close button — no route out of a turn can strand
it. Workspace tabs refresh their data when you navigate back to them, with
live surfaces and unsaved editors deliberately left alone. And the server
now writes combined.log and error.log into the instance's logs directory
with rotation and a startup sweep for iCloud and Finder conflict files, so
warnings survive the terminal.

#### `e253c4db` — 2026-08-18 — fix(import): remap composite componentItemIds on .qtap import (P4.D87, v4 Bug 75 / 40d507cc)

_Versions: core 0.0.581, harness 0.0.503._

Fixed .qtap character import leaving composite outfits hollow (v4 Bug 75
ported; v5 had the same defect). Import re-mints wardrobe item ids, but
composite componentItemIds kept referencing the export's original ids.
The importer now pre-assigns every new id, creates leaf items before the
composites that bundle them, remaps the references, and drops a dangling
reference with a warning instead of leaving it pointing at nothing.
Proven by a new committed composite-chain .qtap fixture whose items are
read back through both sides' real vault readers and compared by
relationship.

#### `b76c4729` — 2026-08-18 — feat(wardrobe): port the hair slot's server half (P4.D87, v4 4423ad10)

_Versions: core 0.0.580, harness 0.0.502._

Ported the server half of v4's hair wardrobe slot (P4.D87, v4 4423ad10).
Wardrobe slots now number five — top, bottom, footwear, accessories, hair
— with hair holding a hairdo, not hair: an empty hair slot means unstyled,
never bald, so no report, prompt, or tool result ever mentions hair when
the slot is blank (two per-slot dumps that printed a literal
"hair: (empty)" line are fixed at all three v5 sites). Nudity semantics
compute over clothing slots only; a hair-only pick still counts as "chose
nothing to wear"; avatars carry the hairdo on both the dressed and
bare-top branches (the bare-top guard keyed on accessories alone and
would have silently dropped it); undressing keeps the hairstyle in scene
state. The ten duplicated slot lists collapse onto one registry
(WARDROBE_SLOT_META) with per-slot label/clothing/report-when-empty
metadata; three deliberately frozen legacy lists stay at four. The outfit
hash gains the hair key unconditionally (each chat re-derives its cached
clothing summary once). No schema or migration change: absent hair keys
read as empty. Thirty-one differential families regenerated fresh from a
pinned 979652a9 v4 worktree, all green, plus a new outfit_hash_equivalence
tier-1 family. Found and pinned upstream: v4 itself crashes avatar
generation on any pre-hair chat row (raw four-key equipped state reaches
the five-slot resolver with no default) — v5 tolerates the legacy shape
by design, pinned both directions with a convergence tripwire.
Quilltap now writes its logs to files again, the way the Node version
did: `logs/combined.log` holds every record and `logs/error.log` holds the
errors, both as one JSON object per line, rotating into numbered backups
once a file passes its size limit. On startup the log directory is also
swept of the debris a synced folder collects — iCloud conflict copies like
`combined 2.log`, Finder duplicates, and leftovers from an older rotation
naming — while terminal transcripts and the launcher's own stdout/stderr
files are left strictly alone. Until now the native build wrote only to the
terminal it was launched from, so a warning worth acting on was gone the
moment the window scrolled. Found by dogfooding (finding #93).

#### `9c8414a5` — 2026-08-18 — feat(logging): wire the file transport into the log surface, defaulting to both

_Versions: web 0.0.75, tauri 0.0.7._

Both destinations are on by default now. `LOG_OUTPUT` still chooses
between `console`, `file` and `both`, and `LOG_FILE_PATH`,
`LOG_FILE_MAX_SIZE` and `LOG_FILE_MAX_FILES` work as they always did; a
value that cannot be read falls back to the default and says so in the log
rather than refusing to start. A setup that asks for files but has nowhere
to put them keeps writing to the terminal instead of going quiet.

#### `718e747d` — 2026-08-18 — docs(logging): rule the CLI's twin by measurement — reader, not writer

_Versions: cli 0.0.10._

The `quilltap` command-line tool still logs only to the terminal, as it
always has — it reads the log files rather than writing them, and a
short-lived command must not rotate a log file out from under a running
server.
Added a browser walk for the hair wardrobe slot — create a hairdo, see it
badged, wear it in the Hair slot, and find it again after reopening the
dialog. It stays inert until the server accepts the fifth slot.

#### `265277c6` — 2026-08-18 — test(wardrobe): parity specs for the slot registry, grouping, and the preview

_Versions: SPA 0.5.507._

Added tests covering the new hair wardrobe slot: the slot registry's rows,
the item editor's grouping, the rose badge on a chosen hairdo, and an
outfit payload saved before hair existed still rendering.

#### `918c3f24` — 2026-08-18 — fix(profiles): an api key no longer follows a profile onto another provider (v4 bug 76)

_Versions: SPA 0.5.509._

Added the "hair" wardrobe slot to the app. It holds a hairdo — braids, an
updo, marcel waves, a wig — not hair itself; colour, length, and texture
stay in the physical description. Hair appears wherever the other four
slots do: the wardrobe dialog's slot filters and equipped rows, the item
editor's types and component groups, the project wardrobe, the
"same as last conversation" preview, and the Green Room's outfit preview,
all wearing a rose badge. The app now reads one slot registry instead of
five copies of the slot list, and an outfit saved before the hair slot
existed still opens, with hair empty.
Fixed an API key following a connection profile onto a provider that
cannot use it. Switching a profile from, say, Anthropic to Ollama left the
stored key in place while the API Key control disappeared, and the save
was then refused with "API key provider does not match profile provider" —
naming a field the dialog no longer showed, with no gesture anywhere that
cleared it. Switching between two hosted providers was worse still: the
control read blank while the wire carried the old provider's key. A
profile now sends only a key its current provider could actually display,
and clears the column otherwise, so a profile already saved that way —
or imported that way — heals on its next ordinary save. Found by
dogfooding (finding #90).

#### `e644a08a` — 2026-08-18 — fix(salon): give the tool-execution notice its own lifetime (v4 bug 77)

_Versions: SPA 0.5.510._

Fixed the "Generating image..." notice above the composer never going
away. It had exactly one teardown, at the end of one of the several routes
a turn can finish by, so any other ending — a tool chain's intermediate
turn, continue mode, either error arm — left it pinned above the composer
for the rest of the session, with no close control to escape it. The
notice now owns its own lifetime: it stays up while the image is still
generating, reports the outcome for six seconds and then dismisses itself,
is dropped if the turn ends without ever producing a result, clears at
once when you stop a turn, and carries a close button of its own.
Two end-to-end walks now cover the returning-to-a-tab refresh: leaving
the chat list and coming back re-reads it, and returning to the
Scriptorium re-lists its stores without the page blinking through its
loading state.

#### `3d41a51f` — 2026-08-18 — feat(screens): views that fetch outside the query cache refresh on return

_Versions: SPA 0.5.513._

The workspace tabs that load their data outside the query cache now
refresh on return too: My Photos, Scenarios, the Scriptorium (list and
store detail), Generate Image's character list, and a character's stats,
conversations and memories. Each refresh happens in place — the page you
came back to stays on screen while fresh data arrives, instead of
blinking through its loading state.

#### `afff539e` — 2026-08-18 — feat(workspace): refresh a tab's cached reads when it is navigated back to

_Versions: SPA 0.5.512._

Returning to a workspace tab now refreshes what it shows. Because tabs
stay mounted, a tab you came back to still displayed whatever it had
loaded when you left it. Each tab kind now declares which cached reads
go stale on re-activation — Home refreshes the dashboard plus chats,
projects and characters; Characters, the chat list, Projects, Files,
Generate Image, Pascal's tools, your profile and Settings each refresh
their own. Live surfaces (a conversation, a terminal, Document Mode,
the Brahma console) and editors holding unsaved work are deliberately
left alone, and the chat sweep never touches a conversation's own data.

#### `8ecf2d73` — 2026-08-18 — feat(workspace): a tab subtree knows whether it is visible

_Versions: SPA 0.5.511._

Workspace tabs now know whether they are the tab you are actually looking
at. Each mounted tab's subtree gets a visibility signal, and a new
`onTabActivated` hook runs a callback on every hidden-to-visible
transition — never on the first activation, never when a tab is hidden,
and never outside the workspace. Nothing uses it yet; the refresh it
enables lands next.

#### `f1471616` — 2026-08-18 — fix(providers): google's model list, on the field name the wire actually sends

_Versions: core 0.0.579._

Fixed Fetch Models returning nothing for Google. Two faults compounded:
the model list was filtered on a field name the Google SDK invents when it
reshapes the response, which the API itself never sends, so every model was
discarded; and the fallback list Quilltap keeps for exactly this case was
missing, so the empty result reached the screen instead of being covered.
Both are restored, including the distinction that Google falls back on an
empty result as well as on an outright failure, where Anthropic falls back
only on failure. Found by dogfooding (finding #91).

#### `ca27521c` — 2026-08-18 — fix(build): the amalgamation path is resolved at build time, not baked in

_No crate versions bumped._

Fixed a release build failing in the main checkout after a lane worktree
was deleted. The SQLite3MC build script resolved its vendor directory
with a compile-time path, and the compiled build script is cached
indefinitely because that crate's version is deliberately pinned — so a
worktree sharing the same target directory could leave its own path
baked in, pointing at a directory that no longer existed. The path is now
read at build time.

#### `cfd202e0` — 2026-08-17 — fix(providers): the models list carries the provider's declared auth headers

_Versions: core 0.0.578._

Fixed Fetch Models returning a fixed list of eleven Claude models for
Anthropic instead of the models your key can actually reach. The models
request was sent without the `anthropic-version` header Anthropic
requires, so the API rejected it and the code fell back to its built-in
list without reporting anything. Providers declare fixed headers like
that in their manifests, and the two hand-built wire calls — the models
list and the generic connection probe — now send them. Found by
dogfooding (finding #89).

#### `1355c23c` — 2026-08-17 — docs(porting): the d123658d round record — baseline moves to d123658d, both lanes closed

_Docs-only change._

Unified the d123658d connection-profile-editor round (P4.D85 server ∥
P4.D86 SPA). The oracle baseline moves to d123658d and the drift debt is
cleared; v4's one newer commit (9c01fa99, sample-prompt content) is
classified no-port. The unification review caught one cross-lane
staleness before merge — the SPA's enriched-tag type documented a
narrowing its sibling lane had closed in the same round — fixed with the
type widened to the full tag row. The gated profile-tag e2e beat was
activated and passed its first live run. Final versions: core 0.0.577,
harness 0.0.501, SPA 0.5.505; host/web/cli/tauri unchanged.

#### `fdd1b8ef` — 2026-08-17 — docs(porting): the P4.D86 lane record — closed, nothing deferred

_Docs-only change._

Closed work order P4.D86, the SPA half of the `d123658d` round. Docs only; no
code changed.

#### `905bfdbf` — 2026-08-17 — feat(spa): the banked attachment-support line, and v4's own verification walk

_Versions: SPA 0.5.502._

The connection-profile editor names what a provider accepts as attachments
again, under the provider dropdown, matching v4.

#### `ea258b9f` — 2026-08-17 — feat(spa): connection-profile tags, in their fixed form (v4 bug 74, client)

_Versions: SPA 0.5.501._

Connection profiles can be tagged. The editor's tag box adds and removes tags as
you go, and the tags now show their names on the profile card instead of drawing
as empty pills. Ports the client half of v4's bug-74 fix, where tagging a profile
had never worked at all.

#### `193c371e` — 2026-08-17 — fix(spa): a hidden base URL no longer follows the profile (v4 bug 73)

_Versions: SPA 0.5.500._

Fixed a base URL following a connection profile onto a provider that neither
shows nor takes one. Selecting Ollama fills in localhost:11434; switching to a
hosted provider hid the field but kept the value, and every connection test,
model fetch and save still sent it, so the profile could not connect with
nothing on screen to explain why and no way to clear it. The value now stays in
the form (switching back restores it) but never reaches the wire, and a save
always sends the field so an already-broken profile heals the next time you save
it. A provider the app has not loaded keeps its stored URL. Ports v4's bug-73
fix.

#### `05716b08` — 2026-08-17 — fix(spa): a cleared numeric provider option keeps its own draft (v4 bug 72)

_Versions: SPA 0.5.499._

Fixed a numeric provider option in the connection-profile editor putting the
schema default straight back when you clear it, so the next digit appended to it
and a wrong value was stored (clear 300, type 5, get 3005). The box now keeps its
own draft while you edit, and an unset option shows its default as a placeholder
rather than a value, so "leave blank for the default" is a state you can see
yourself reach. Ports v4's bug-72 fix.

#### `17bd67ba` — 2026-08-17 — port(profiles): connection-profile tags — the shared flat resolver and the three verbs (v4 Bug 74, d123658d)

_Versions: core 0.0.576, harness 0.0.500._

Connection profiles can be tagged. The three actions v4 grew in its bug-74 fix
— read a profile's tags, add one, remove one — are now verbs, and the two tag
shapes that had drifted apart in v4 each have one owner here: the flat shape the
tag editor reads, shared by the character and connection-profile answers, and
the enveloped shape the list endpoints send. The enveloped one now carries the
whole tag record, as v4's does; it had been narrowed to an id and a name, which
nothing could catch because no profile in the test corpus had ever had a tag.

#### `ac25de4a` — 2026-08-17 — fix(profiles): a cleared field comes back explicitly empty, as v4 sends it

_Versions: core 0.0.577, harness 0.0.501._

Clearing a connection profile's base URL, model class, max context, or API key
now answers with that field explicitly empty, as v4 does, instead of leaving it
out of the reply altogether. A client reading the saved profile straight back
off the response could not tell a cleared field from one the server had
declined to send. Found by giving a test profile a stored base URL — with every
column already empty, the two behaved identically and the difference could not
be seen.

#### `d6f1f4fc` — 2026-08-17 — docs(porting): work orders for the d123658d connection-profile-editor drift round (P4.D85 ∥ P4.D86)

_Docs-only change._

Planned the `d123658d` connection-profile-editor drift round and committed its
two work orders: P4.D85 (server — the profile tag verbs, the GET action gate,
and the `resolveEditorTags` convergence from v4's bug-74 fix) and P4.D86 (SPA —
the bug-72 number-field draft machinery, the bug-73 `outboundBaseUrl`
chokepoint with the always-send save body, and the profile tag surface). v4's
`d123658d` fixes bugs 72 and 73, which this port's own 2026-08-16 dogfood walk
found and filed, plus bug 74 (profile tagging had never worked). The sibling
commit `d81ccc17` is docs-only, no port. Docs only; no code changed.

#### `56196c2c` — 2026-08-16 — docs(dogfood): the 93ed8abf walk's coverage through Part B — three live proofs discharged

_Docs-only change._

Recorded the 93ed8abf round's dogfood coverage through Part B. Three of the
round's four owed live proofs are discharged on real Friday data: Max Tokens
and Top P reaching a local wire along with the keep-alive sentinels, the
per-profile request timeout firing on a cold model, and the provider-options
panel driven on real profiles. OpenAI-compatible tool calling against a local
llama-server is still owed.

#### `3b7cdbcf` — 2026-08-16 — tools(dogfood): a byte-faithful wire tap for local providers

_No crate versions bumped._

Added `harness/tools/wire-tap.py`, a byte-faithful TCP tap for reading what a
local provider actually receives. Neither the LLM Inspector nor the logs can
answer that — the log stores a summary of the request, and the IO layer traces
nothing — so a claim that the profile's parameters reach the wire had no way to
be checked by hand. Point a connection profile's base URL at the tap and every
request body prints while the bytes relay untouched. Dev tooling only; nothing
in the app changed.

#### `c753a62a` — 2026-08-16 — docs(dogfood): findings #87 and #88 — two faithfully ported v4 bugs, filed upstream

_Docs-only change._

Recorded two dogfood findings from the 93ed8abf round's walk, both diagnosed as
faithfully ported v4 bugs and filed upstream rather than fixed here. Clearing a
numeric provider option puts the schema default straight back with the caret
after it, so the next digit appends to it and a wrong value reaches the server.
And a base URL picked up from Ollama or an OpenAI-compatible endpoint survives a
switch to a provider that hides the field, breaking every connection test and
saving onto the profile row. Both were measured by driving v4's own components,
not by reading them. No product code changed.

#### `6024652b` — 2026-08-16 — docs(porting): the 93ed8abf drift-round record — baseline moves to 93ed8abf, all three lanes closed

_Docs-only change._

Unified the 93ed8abf drift catch-up round: the context budget honors the
profile's Max Context end-to-end (single-sourced window resolution, the unified
safe-input-limit formula the builder and validator both read, tool schemas and
spliced system messages measured and reserved before the context is built, and
the tool-change notice now actually reaches the model — a pre-existing deferral
closed), the profile's sampling knobs reach every completion path through one
resolver (five call sites including Carina; Top P gained a seat in the
non-streaming params; absent Max Tokens stopped meaning zero), the
profile-parameters passthrough covers Ollama and OpenAI-compatible endpoints
(sampler options, keep-alive, thinking effort, request timeout; OAC gains tool
calling on both paths), each provider's connection-profile options schema is
served and rendered by a schema-driven panel in the profile editor (replacing
the hardcoded Enable Thinking row), and the tool-use seed hint plus the vision
re-seed landed. The oracle baseline moves to 93ed8abf; the drift debt is
cleared. Gate: 437 test binaries / 2,147 / 0 with the round's env block, 26
families regenerated fresh at the pin with zero skips, clippy both feature
sets, release build, ng test 4,792 / 0, full Playwright 222 / 0 / 0 with the
options round-trip beat live on first activation. Versions: core 0.0.575,
harness 0.0.499, host 0.0.72, SPA 0.5.498.

#### `9e841c4f` — 2026-08-16 — unify(review): the §3 findings — the kwargs array arm, OAC non-streaming tool calls, the client-facing pre-send validation, the reroute budget profile, the provider estimator rate

_Versions: core 0.0.575, harness 0.0.499._

The 93ed8abf unification review's fixes. A profile storing its OpenAI-compatible
chat_template_kwargs as a JSON-array string now reaches the wire as the parsed
array, matching the reference (objects only, before). The OpenAI-compatible
non-streaming path parses tool calls back with the reference's own filter. The
pre-send context validation now tells the client — an unconditional validating
status and a warning status on overage, the user's only signal a payload may be
rejected — instead of logging only. A danger-rerouted turn budgets its context
to the rerouted profile's Max Context (it read the original's), and the
turn-extras reservation uses the provider's own chars-per-token rate (Google is
3.8, not the flat 3.5). Doc placement fixed on the transport policy composers;
the sampling and timeout harness assertions gained their missing symmetric
directions.

#### `75b88d4e` — 2026-08-16 — port(providers): serve each provider's connection-profile options schema — P4.D83 unit 7

_Versions: core 0.0.574, harness 0.0.497._

The providers listing now carries each provider's connection-profile options
schema — the fields, labels, help text and enum choices a profile editor draws
for that provider. It was always null, so the panel had nothing to render;
eight of the nine built-in providers declare one, and the ninth (Google) still
answers null because it declares none.

#### `c044678d` — 2026-08-16 — port(ollama): the profile's request timeout on both call shapes — v4 d89babc4, P4.D83 unit 6

_Versions: core 0.0.573, harness 0.0.496, host 0.0.72._

An Ollama connection profile can set its own Request Timeout. A turn was
bounded by the shared five-minute default with nothing in the UI to change it,
and loading a large model off disk plus reading a long prompt both happen
before the first token — a big model on a busy machine could cross the ceiling
and leave no reply at all. The profile's `request_timeout_seconds` now sets
that budget on both the streaming first-token wait and the whole non-streaming
request. Blank, absent or unparseable falls through to 300 seconds, so nothing
changes for a profile that never touches it, and a caller that has already
decided what the work is worth waiting for still wins.

#### `b32513cd` — 2026-08-16 — port(oac): tools on both paths, the profile allow-list, the template-kwargs fold — v4 93ed8abf, P4.D83 unit 5

_Versions: core 0.0.572, harness 0.0.495._

An OpenAI-compatible endpoint can now call tools, and reads the profile's
settings. It sent no tools on either path and never looked at the parameters
blob, so a local llama-server with function calling could not be used for tool
work and reasoning effort was unreachable. Tools and tool_choice now go on the
body and tool calls are parsed back on both paths, with streamed argument
fragments accumulated by index. The provider forwards its own allow-list —
top_k, min_p, the three penalties, seed, cache_prompt — and folds reasoning
effort into chat_template_kwargs, which is how llama-server reaches a
template's arguments; a flat key parses and is never seen by the template. The
tool-use capability stays off: it seeds the checkbox on a new profile and never
disables it.

#### `e3ca1596` — 2026-08-16 — port(ollama): the profile's sampler options, keep_alive and thinking effort — v4 93ed8abf, P4.D83 unit 4

_Versions: core 0.0.571, harness 0.0.494._

Ollama now sends the sampler settings a connection profile saves. It read two
keys — the context window and the thinking switch — beside a hardcoded options
literal, and dropped every other one in silence, so no local model could run at
its publisher's recommended settings. The options table now carries top_k,
min_p, the three penalties, the seed and the mirostat trio; Keep Model Loaded
rides the top level (sending nothing at all by default, leaving OLLAMA_KEEP_ALIVE
in charge, and sending its two sentinels as numbers because the server refuses
them as duration strings); and Thinking Effort folds into the thinking field as
a level. The three control keys the provider reads for itself never reach the
wire.

#### `31868bb4` — 2026-08-16 — port(providers): one profile-parameter applier with a per-key hook — v4 93ed8abf, P4.D83 unit 3

_Versions: core 0.0.570, harness 0.0.493._

Z.AI no longer sends a reasoning effort to a model that ignores it. A profile
setting Reasoning Effort had it forwarded to every GLM, including the older
models where the field is not honored; it is now sent only to the models that
support it, matching what the rest of the stack already did. The
profile-parameter passthrough itself became one shared mechanism with a
per-key hook, so a provider can reshape a stored value or send it under a
different key without hand-rolling the copy loop.

#### `aa1b28a9` — 2026-08-15 — port(llm): the profile's Max Tokens and Top P reach the model — v4 d89babc4, P4.D83 unit 2

_Versions: core 0.0.569, harness 0.0.492._

A connection profile's Max Tokens and Top P now reach the model. Both were
stored and displayed and never sent: the Salon's main path, the greeting
(including its uncensored retry) and Carina's reference query all read them
under camelCase names the profile editor does not write, so two of the three
sampling knobs came out empty on every turn and the provider fell back to its
own defaults. Regenerate/swipe read the snake_case names but never read Top P
at all, so the original reply and a regeneration of it disagreed. All five
sites now go through the one resolver, and the image-description fallback
sends Top P as well — the non-streaming path had nowhere to carry one. A
profile that names no Max Tokens now leaves the field off the request, where
regenerate and the image description used to send a literal zero.

#### `2dd6e3ef` — 2026-08-15 — port(llm): one reader for a profile's sampling knobs — v4 d89babc4, P4.D83 unit 1

_Versions: core 0.0.568, harness 0.0.491._

A connection profile's sampling knobs now have one reader. `resolve_sampling_params`
maps a profile's free-form `parameters` blob to the three per-request fields —
canonical snake_case (`temperature` / `max_tokens` / `top_p`) first, camelCase
tolerated for a hand-edited or imported blob, absent knobs left unset so nothing
is invented. Strings go through the same number coercion the rest of the port
uses, and a key that is present but unusable falls through to the other
spelling rather than ending the search.

#### `465e3d54` — 2026-08-15 — port(chat): reserve the turn's extras before building context; tell the model its tools changed — v4 f933ba9c, P4.D82 unit 4

_Versions: core 0.0.567, harness 0.0.490._

The turn now reserves room for what it adds after the context is built, and
tells the model when its tool roster changed. A chat whose tool settings the
operator edited set a flag that nothing ever consumed or cleared, so the
model was never told and the flag stayed set forever; that turn now carries
the tool-change notice and clears it. The pre-send payload check measures
the messages plus the tool schemas against the same ceiling the builder
packed to.

#### `f1d50ed6` — 2026-08-15 — port(tokens): count the tool schemas, and build the turn extras in one place — v4 f933ba9c, P4.D82 units 2-3

_Versions: core 0.0.566, harness 0.0.489._

Tool schemas now count toward the context budget. `count_tool_schema_tokens`
measures the serialized slate (plus per-tool framing), and a new
`turn_extras` module builds and measures everything a turn adds after the
context is built — the tool schemas, the agent-mode instructions, and the
tool-change notice — in one place, so the room reserved for them and the
text that fills it cannot drift apart.

#### `8ab6ff1e` — 2026-08-15 — port(context): the context budget honors the profile's Max Context — v4 f933ba9c (bug 70), P4.D82 unit 1

_Versions: core 0.0.565, harness 0.0.488._

The context budget now honors the connection profile's Max Context (v4 bug
70). A profile pointed at a model no lookup table knows — any `hf.co/...`
Ollama tag, any custom OpenAI-compatible endpoint — was budgeted at the
8192-token provider default while the compression trigger, which does read
the profile, worked from the real window; history was trimmed to fit the
small figure on every turn. `resolve_context_window` is now the single
source of the window (the profile wins, the name lookup is the fallback, a
zero or negative setting falls through), and the allocation, the safe-input
limit, `calculate_max_available` and the self-inventory last-turn section
all route through it. The builder and the pre-send validation now share one
ceiling (`safe_input_limit` = window less response reserve less a 10%
estimator margin), which `ContextBudget` carries alongside the margin
itself; the builder used to pack 10% past the line that then warned about
it. The context builder also accepts `reserved_outgoing_tokens` — room held
back for what the caller adds after the context is built — and says so by
name when the fixed payload leaves no room for conversation history at all.
Documented the providers listing's options-schema field on the SPA contract now
that the profile editor consumes it.

#### `0aece250` — 2026-08-15 — test(spa): the provider-options e2e walk, two beats live (P4.D84 unit 4)

_Versions: SPA 0.5.497._

Added an end-to-end walk for the connection-profile editor's provider-driven
surfaces: the tool-use hint and the vision re-seed run live, and the
schema-driven options round trip is written and waiting on the wire half of
this round.

#### `e52d6a4d` — 2026-08-15 — port(spa): the tool-use seed hint and the vision re-seed (P4.D84 unit 3)

_Versions: SPA 0.5.496._

The connection-profile editor now explains itself when a provider does not
advertise tool support: the box still starts off, but a note under it says an
endpoint that really does speak native function calling can be switched on
regardless. Changing the provider on a new profile also re-seeds the vision
checkbox from that provider's attachment support, which it previously left
alone.

#### `b89d9281` — 2026-08-15 — port(spa): cut the profile modal over to the options panel (P4.D84 unit 2)

_Versions: SPA 0.5.495._

Cut the connection-profile editor over to the schema-driven options panel.
The hardcoded Ollama "Enable Thinking" row is gone; Ollama's own schema draws
it along with thinking effort, keep-alive, the request timeout, and the whole
sampling group, and every other provider's options appear the same way.
Clearing a numeric option now removes the key instead of leaving a hole,
OpenRouter's "Use Custom Model ID" switches the model box to free text, and an
old profile's nested OpenRouter data-collection preference is translated to the
flat zero-data-retention flag the panel shows.

#### `ade78dcb` — 2026-08-15 — port(spa): the schema-driven provider-options renderer (P4.D84 unit 1)

_Versions: SPA 0.5.494._

Added the schema-driven provider-options renderer to the connection-profile
editor (`qt-provider-options-panel`), a port of v4's `ProviderOptionsPanel`.
It draws boolean, enum, multi-enum, string, and number fields from the schema
each provider plugin declares, honors `showIf` guards and group headings, and
writes each key into the profile's parameters bag one at a time so keys with
no control still survive a save. Not yet wired into the modal.

#### `5df3938e` — 2026-08-15 — docs(porting): the 93ed8abf drift-round work orders — P4.D82 (bug-70 context budget), P4.D83 (profile-params wire, stacked), P4.D84 (SPA options panel)

_Docs-only change._

Planned the 93ed8abf drift catch-up round and committed its three work
orders: P4.D82 (bug 70 — the context budget honors the profile's Max
Context, with the new turn-extras tool-schema token accounting), P4.D83
(the profile-parameters wire — resolveSamplingParams at all four call
sites, the Ollama options/keep_alive/thinking-effort/request-timeout
table, OPENAI_COMPATIBLE tool calling, and optionsSchema carried onto
the provider manifests; stacked on P4.D82), and P4.D84 (the SPA
schema-driven provider-options panel replacing the hardcoded Enable
Thinking row, plus the tool-use seed hint and the supportsImageUpload
re-seed). Docs only; no crate or SPA source touched.

#### `a6cd94f5` — 2026-08-15 — docs(porting): the aa464abf drift-round record — baseline moves to aa464abf, all five lanes closed

_Docs-only change._

Unified the aa464abf drift catch-up round: the whole Ollama-thinking wire
(inline think-block stream parsing on both channels, the think and num_ctx
request fields with a one-shot retry when a model refuses the think
parameter, the toolUse capability flip), the per-profile multi-character
[Name] prefill column end to end with greeting reasoning persisted onto the
first message, the profileParams consolidation (which also fixed three
pre-existing v5 defects: the Salon primary stream sent no per-model
parameters at all, the Carina answer read temperature from a nonexistent
key, and the profile editor dropped every non-sampling parameters key on
save), the archivedAt chat-GET enrichment with the archived-badge beat
flipped live, the archive rehydrate self-heal for digest rows damaged by
v4's pre-4.9 watcher, and the import preflight's read-error propagation
(a ruled divergence — v4 swallows those errors — with both-directions
tripwires). The oracle baseline moves to aa464abf; v4's bug-70 commit is
the queued next drift. The unification review found no blocking findings;
the gate repaired three harness recipes. Full gate green: 435 test
binaries / 2,125 tests, the 28 affected differential families regenerated
fresh at the pinned baseline, ng test 4,741, and the full Playwright suite
(numbers in the round record). Versions: core 0.0.564, harness 0.0.487,
host 0.0.71, SPA 0.5.493.

#### `e83b133e` — 2026-08-15 — docs(porting): the P4.D78 lane gate and the pinned-oracle re-proof

_No crate versions bumped._

Re-proved every P4.D78 oracle against a v4 worktree pinned at aa464abf after
the v4 checkout went dirty mid-lane with unrelated WIP: all eight (the two new
oracles, the registry and listing oracles, the generated manifests, the ollama
stream recording, and the request-envelope and response-body corpora) are
byte-identical. Docs and a recipe-header correction only.

#### `a62831fd` — 2026-08-15 — feat(model): the Ollama retry-without-think + the toolUse manifest flip (P4.D78 units 5-6)

_No crate versions bumped._

Ollama declares tool use, and the retry-without-think lands (P4.D78 units
5-6, v4 d9c5a1c7): a non-ok Ollama response whose body mentions "think" now
re-sends once with the parameter deleted, on both the streaming and the
non-streaming path, proven by fake-transport quartets and a new tier-3 arm
driving v4's real plugin with fetch mocked below it. The regenerated
manifests flip `OLLAMA.capabilities.toolUse` to true.

Fixed the provider-manifest generator's stale google auth entry: P4.47
corrected the committed manifest to the `x-goog-api-key` header but not the
generator's augmentation table, so the next regen would have silently
reverted it (the P4.39 class of rot).

#### `ae46c8f9` — 2026-08-15 — feat(decoders): route Ollama reasoning through the stream decoder (P4.D78 unit 4)

_No crate versions bumped._

The Ollama stream decoder now routes reasoning (P4.D78 unit 4, v4
d9c5a1c7): native `message.thinking` deltas and inline `<think>` interiors
land on one cumulative `reasoningContent`, reasoning-only chunks carry empty
content, the parser's flush releases its tail as a content chunk before the
terminal one, and the terminal `rawResponse` content is think-free. Seven new
recorded stream vectors; every pre-existing vector byte-identical. The
composer differential's stale ollama byte-chunking exclusion is retired — all
three chunkings now run there too.

#### `0c8cdddd` — 2026-08-15 — feat(model): route both Ollama reasoning channels on the non-streaming parse (P4.D78 unit 3)

_No crate versions bumped._

The non-streaming Ollama parse now routes both reasoning channels (P4.D78
unit 3, v4 d9c5a1c7): `parse_ollama` splits `message.content` through the
think parser and concatenates the native `message.thinking` ahead of the
inline reasoning, attaching `reasoningContent` only when the result is
non-empty. Seven new Ollama cases in the response-body corpus; every
pre-existing row byte-identical.

#### `517b1b3c` — 2026-08-15 — feat(model): Ollama think + options.num_ctx on the wire (P4.D78 unit 2)

_No crate versions bumped._

Ollama requests now carry the thinking switch and the context window
(P4.D78 unit 2, v4 d9c5a1c7): `build_ollama_body` emits a top-level `think`
on every body (false when the profile's `enable_thinking` is off) and
`options.num_ctx` when the profile bag coerces to a finite positive number.
The request-envelope corpus regenerated at the aa464abf pin — 14 new Ollama
cases in both modes, every non-Ollama vector byte-identical.

#### `680e507d` — 2026-08-15 — feat(model): the Ollama inline-<think> stream parser (P4.D78 unit 1)

_No crate versions bumped._

Ported the Ollama inline-`<think>` stream parser (P4.D78 unit 1, v4
d9c5a1c7): `ThinkTagStreamParser` plus the one-shot `extract_think_blocks`,
carrying the partial-tag holdback, the swallowed-opening-tag rule and its
emitted-visible cutoff, the flush semantics, and the JS-whitespace sanitize.
New tier-1 differential `ollama_think_parser_equivalence` drives v4's real
think-parser.ts over a committed 339-case table that enumerates every split
point of both tags.
Closed the connection-profile parameters key-order seam on the update path
(P4.D79 tier 2). The corpus already pinned a non-sorted multi-key bag on
create; the replace path now has its own arm, because the module's
"constrained to {} or a single key" note stopped describing reality when the
SPA began writing enable_thinking beside temperature. Insertion order
round-trips both ways. The module doc was swept for the new column and the
retired constraint. Versions: core 0.0.557, harness 0.0.479.

#### `3760c342` — 2026-08-15 — feat(chat): capture the greeting's reasoning and persist it

_Versions: core 0.0.556, harness 0.0.478._

Greeting generation now captures a thinking model's reasoning and persists it
on the opening message (P4.D79 unit 6, v4 23af7146). Providers emit
reasoningContent cumulatively — the full thinking-so-far on every chunk — so
it is accumulated by assignment, not concatenation, and an empty chunk does
not clear what came before; the value is trimmed alongside the content and
carried through all four greeting attempts and the Concierge reroute. It is
display-only, and a scripted first message stores NULL because it never
touched a model. The greeting differential grew a reasoning-carrying case
(plus one that pins the empty-chunk rule) and the chat-create capstone now
diffs the persisted value. Versions: core 0.0.556, harness 0.0.478.

#### `42af3ad0` — 2026-08-15 — feat(export): carry multiCharacterPrefill and regenerate the key-order table

_Versions: core 0.0.555._

Carried multiCharacterPrefill through the .qtap export (P4.D79 unit 5, v4
23af7146). schema-key-order.json was regenerated from v4's live schemas rather
than hand-appended, so the key lands in its schema slot after pseudoToolMode
instead of silently at the end of every exported profile record; a unit test
pins the slot. The connection-profile net read also gained per-column
tolerance: a table that predates the column selects a literal NULL instead of
failing outright, which is how pre-migration instances and every fixture built
before the drift stay readable. Versions: core 0.0.555.

#### `d0b2d6f7` — 2026-08-15 — feat(api): multiCharacterPrefill through the connection-profile routes

_Versions: core 0.0.554, harness 0.0.477._

Wired multiCharacterPrefill through the connection-profile create and update
routes (P4.D79 unit 4, v4 23af7146). Create resolves the provider default when
the client omits the field and stores it, so a create never writes the
tri-state NULL; both routes 400 with v4's exact sentence on a present
non-boolean, an explicit null included. The repo's create OMITS the column
when the value is absent rather than writing NULL — measured against v4, whose
INSERT names only the columns the parsed document carries, so on a fresh
instance the omission lands NULL and on a migrated one the DEFAULT 1 lands 1;
writing NULL would have passed the differential and diverged on upgraded
instances. The restore path carries the field; the .qtap import carry is
STOPPED by the round's ownership tripwire, with the two-line edit recorded at
the site. Versions: core 0.0.554, harness 0.0.477.

#### `f40a10ea` — 2026-08-15 — feat(services): the multi-character turn anchor becomes per-profile

_Versions: core 0.0.553, harness 0.0.476._

Made the multi-character [Name] turn anchor per-profile (P4.D79 unit 3, v4
23af7146). The hardcoded "Anthropic gets prose, everyone else gets the
prefill" branch is replaced by applyMultiCharacterTurnAnchor over the
profile's own multiCharacterPrefill choice, resolved through the tri-state
resolver; the prose sentence is byte-unchanged. The connection-profile net
read carries the new column (a NULL reads as an absent key, matching v4's
SQLite hydration). The orchestrator tier-3 corpus gains two cases that invert
the old provider rule — an OpenRouter profile with the prefill off takes the
prose route, an Anthropic profile with it on takes the prefill — and the
danger-reroute case pinned that the anchor resolves from the EFFECTIVE
profile, not the original. Versions: core 0.0.553, harness 0.0.476.

#### `3facb590` — 2026-08-15 — feat(core): consolidate profileParams and port the Ollama num_ctx injection

_Versions: core 0.0.552, harness 0.0.475._

Consolidated every profile-parameters construction site onto the shared
profileParams helper and ported its Ollama num_ctx injection (P4.D79 unit 7,
v4 d9c5a1c7). The helper gains the injection (Max Context becomes
options.num_ctx for Ollama profiles that do not already pin it); the eight v4
call sites' v5 twins now all route through it, which fixes three measured
divergences on the way (regenerate-swipe forwarded a non-object parameters
cell verbatim, the image-description fallback and the greeting both dropped
an array bag). A new 900-case tier-1 differential compares the result both
structurally and as literal JSON text, so key order is a comparand.

Two larger gaps surfaced while converting, both pre-existing and both fixed:
the Salon's primary stream had no modelParams twin at all, so a profile's
temperature, maxTokens, topP and the whole parameters bag were silently
dropped on every turn; and the Carina answer read its temperature from a
top-level profile key that does not exist. The orchestrator tier-3 corpus
gave its Primary profile a real parameters bag and the oracle now records
the modelParams reaching the wire, which is what made both visible.
Versions: core 0.0.552, harness 0.0.475.

#### `40172c98` — 2026-08-15 — feat(services): the multi-character [Name] prefill resolver

_Versions: core 0.0.551, harness 0.0.474._

Ported the multi-character prefill resolver (P4.D79 unit 2, v4 23af7146's
lib/llm/multi-character-prefill.ts): the provider default (off for Anthropic,
on everywhere else) and the tri-state resolution, where a stored null means
"never chosen" and falls back to the default — so an Anthropic profile
imported from a pre-4.9 bundle cannot come back with the prefill on. A new
144-case tier-1 differential over providers by stored state, mutation-proven.
Versions: core 0.0.551, harness 0.0.474.

#### `e30acf4e` — 2026-08-15 — feat(db): the multiCharacterPrefill column — D23 re-dump + boot ensure

_Versions: core 0.0.550, host 0.0.71._

Added the connection_profiles.multiCharacterPrefill column (P4.D79 unit 1,
v4 23af7146): the D23 fresh-schema re-dump at the aa464abf pin, plus a boot
ensure that gives an existing instance v4's migration shape. The two v4
shapes differ here — generateDDL emits a bare INTEGER (the Zod field has no
default), the migration emits INTEGER DEFAULT 1 and backfills Anthropic
profiles off. The ensure's guard is at the column level, not per statement,
because v4's backfill runs exactly once: re-running it every boot would
clobber a user's explicit choice on an Anthropic profile. Versions: core
0.0.550, host 0.0.71.
Closed work order P4.D80. Docs only.

#### `6d4285ca` — 2026-08-15 — port(P4.D80): bug-66 archivedAt enrichment + bug-69 rehydrate self-heal

_Versions: core 0.0.550, harness 0.0.474._

Ported the server half of v4's aa464abf. The chat GET's character enrichment
now carries archivedAt on both of its return paths, so the Salon sidebar can
badge an archived seat on a fresh load instead of only after a participants
action (v4 bug 66 — the bug this port filed upstream, now fixed on both
sides). Rehydrating a character archive also self-heals a bundle row whose
recorded digest was overwritten with the digest of its encrypted bytes: when
the recorded digest is provably the digest of the file as stored, the bundle
is intact and the record is repaired with a warning; any other mismatch is
still refused as corrupt (v4 bug 69, reachable on instances a pre-4.9 v4
damaged). The character-archive differential grows to 20 cases and gains a
digest-classification comparand, both mutation-proven.
Re-ran the neighbouring import, restore and archive differentials against
freshly regenerated oracles to confirm the import read-failure change moved
no green-path byte. Six of seven green; the one red is v4 drift on a
connection-profile column another lane is porting, not a regression here.

#### `4a2d4e85` — 2026-08-15 — test(import): pin the read-failure behavior with three planted differential arms

_Versions: harness 0.0.474._

Pinned the import read-failure behavior with three differential arms that
plant a failure in the database itself and compare against v4's real import
and preview. Two legs, measured rather than assumed: an unavailable document
store sinks both engines identically (the refusal message compared byte for
byte, and the preflight proven to write nothing across all three
partitions), while an unreadable table is swallowed by v4 and refused by v5
— recorded as a deliberate divergence with tripwires in both directions.

#### `943b9d27` — 2026-08-15 — fix(import): a failed existence read refuses the import instead of reading "id free"

_Versions: core 0.0.550._

The `.qtap` import no longer mistakes an unreadable database for an empty
one. Every existence check in the preserve-ids preflight, the per-item
importers, and the import preview used to swallow a read failure and report
the id as free; a failed read now refuses the import (or sinks the preview)
instead of going on to attempt id-carrying inserts into a database it could
not read. Twenty-three sites in all — the escalation had counted ten,
missing the multi-line ones. A fresh survey of v4 refuted the premise that
v4 propagates here: v4 swallows the same read errors, so this is a
deliberate divergence under the import/restore ruling, except on the
unavailable-document-store leg, where v4 genuinely throws and v5 was simply
wrong.
Recorded the P4.D81 lane's deferrals and gate: the provider option-schema
renderer stays unported, a live Ollama thinking proof is owed, and the archived
badge beat's gate is the unifier's to flip.

#### `c8f6d2c5` — 2026-08-15 — docs(composer): v4 converged on the source-view send, and the greeting fold pin (P4.D81 unit 5)

_Versions: SPA 0.5.492._

Recorded that v4 adopted Quilltap-v5's source-view send behavior in 4.9: a send
made with the raw-Markdown view open ships what the writer can see. No behavior
change here — comments and test notes only, plus a spec pinning that Send lights
for text typed only in source view.

#### `12cff6b7` — 2026-08-15 — test(archive): flip the archived-seat badge beat, and pin the pass-through (P4.D81 unit 4)

_Versions: SPA 0.5.491._

Turned the archived-seat sidebar beat around: it now expects both the Absent
and Archived badges on a fresh load, gated until the server projection that
makes that possible lands beside it.

#### `ba3e410e` — 2026-08-15 — feat(settings): Ollama's Enable Thinking, and the parameters bag that leaked (P4.D81 unit 3)

_Versions: SPA 0.5.490._

Added an Ollama "Enable Thinking" option to the connection-profile editor,
writing `enable_thinking` into the profile's parameters. Editing a profile no
longer drops the parameter keys the editor shows no control for — `num_ctx`,
OpenRouter's provider preferences, and the rest now survive a save.

#### `c18d919d` — 2026-08-15 — feat(settings): the multi-character prefill checkbox (P4.D81 unit 2)

_Versions: SPA 0.5.489._

Added the multi-character prefill checkbox to the connection-profile editor:
"Announce the speaker in multi-character scenes ([Name] prefill)", seeded from
the stored value or, when a profile has never chosen, from the provider
default. Switching provider re-seeds it on a new profile and leaves a saved
one alone. Ticking it on Anthropic warns that recent models reject a request
handed over mid-turn. The value ships on create and update, including the
Courier's, which renders the same assembled context.

#### `5c79c004` — 2026-08-15 — feat(settings): the multi-character prefill default, client twin (P4.D81 unit 1)

_Versions: SPA 0.5.488._

Ported v4's multi-character prefill default into the SPA
(`defaultMultiCharacterPrefill`): off for Anthropic, on for every other
provider, on when the provider is absent. It seeds the connection-profile
editor's new checkbox; the stored value's resolution stays server-side.

#### `c16f69ac` — 2026-08-15 — docs(porting): the aa464abf drift-round work orders — five lanes planned

_Docs-only change._

Planned the aa464abf drift catch-up round and committed five work orders:
P4.D78 (the Ollama-thinking provider wire — think-tag stream parsing, the
think/num_ctx request fields with the retry-without-think, the toolUse
manifest flip), P4.D79 (the multiCharacterPrefill column through the D23
re-dump and boot ensure, the per-profile turn anchor, greeting reasoning
capture, and the profileParams consolidation that also fixes three measured
v5 divergences), P4.D80 (the bug-66 archivedAt enrichment and the bug-69
rehydrate self-heal arm, with the round's three no-port dispositions),
P4.D81 (the SPA riders: the prefill and Enable Thinking checkboxes, the
archived-badge beat flip, the bug-67 convergence records), and P4.48 (the
escalated import-preflight read-error propagation at ten sites). Docs only;
the oracle baseline moves to aa464abf at the round's unification.

#### `0dad9719` — 2026-08-14 — docs(porting): v4 bugs 66 and 67 filed — the round's two v4-side findings discharged

_Docs-only change._

Filed the round's two v4-side findings upstream as v4 bugs 66 and 67 (the
archived-seat badge that cannot light on a fresh load, and the raw-source
view's send discarding source edits) — a docs-only commit in the v4
checkout, left unpushed beside the in-flight Ollama work.

#### `f6cd55d6` — 2026-08-14 — docs(porting): the help-drift round record — baseline moves to 24633026, all four lanes closed

_Docs-only change._

Unified the help-drift round: the server half of v4's section-level help
search (the help_doc_chunks table, chunking, the sync's re-slice and upgrade
backfill, the embedding job's chunk pass, the reindex/reapply riders, and
best-section ranking in help_search), the archive-family coverage remainder,
three maintenance smalls (the settings Zod arms, the google api-key header
fix, the sweep driver's staging-dependency class), and the composer
formatting toolbar with v4's composer layout. The oracle baseline moves to
24633026. The unification review fixed two wire-status divergences, a
disabled-state gap, and a mis-claimed editor divergence before merge; the
gate repaired two sweep-driver defects and restored a briefly-clobbered
recorded corpus. Full gate green: 431 test binaries / 2,082 tests, the
round's differentials regenerated fresh at the pinned baseline, ng test
4,711, and the full Playwright suite (numbers in the round record).
Versions: core 0.0.549, harness 0.0.473, host 0.0.70, SPA 0.5.487.

#### `cf1ec06f` — 2026-08-14 — fix(harness): restore the sweep-clobbered google-wire corpus; the driver never runs a committed corpus's recording stage

_Versions: harness 0.0.473._

Restored the recorded google-wire corpus after the unification gate's sweep
clobbered it: running the family through the sweep executed its RECORDING
script against the pinned v4 worktree, where the google plugin's runtime
deps are absent, so all eighteen recordings refused and the committed bytes
were overwritten with refusal rows (then swept into a commit by a broad
add). The sweep driver now never runs a committed-corpus family's recording
stage — recording is a deliberate by-hand act — and warns loudly whenever
any family's stages leave tracked fixture bytes modified.

#### `e633a113` — 2026-08-14 — fix(review): the help-drift round's §3 findings — settings error statuses, the composer disabled gate, the list-divergence pin, and four comment corrections

_Versions: core 0.0.549, harness 0.0.472, SPA 0.5.487._

Fixed two wire-status divergences the unification review's new error-status
assert caught in the settings routes: a validation failure whose message
carries no "Invalid" (a bare threshold out of range) now answers 500 exactly
as v4's catch does, and creating or renaming a connection profile onto an
existing name now answers 409 Conflict, not 400. Bodies were already
identical; only the statuses moved. The settings differential now asserts
the recorded status on every error row.

Fixed the Generate Image composer button staying clickable while the
composer is disabled (v4 disables it), and recorded — with a pinning spec —
the deliberate divergence that a narration/OOC button pressed inside a list
rewrites only the caret's item where v4 flattens the whole list into one
text run. Corrected two help-chunk code comments that misstated v4's
safe-query error semantics (the behavior was faithful; the stated mechanism
was inverted), refreshed the stale "no reindex-all handler" note in the
help-doc sync header, and recorded the reindex clear-pair's transaction
shape beside the backfill's existing note.

#### `3d3e60a1` — 2026-08-14 — fix(harness): the help-sync-guards recipe stages its own fixture (the P4.47 handoff wire)

_Versions: harness 0.0.471._

The help-sync guards differential's regen recipe now stages its own fixture
into a family-specific /tmp path instead of silently reading the sync family's
— the staging-dependency defect the new sweep-driver class flags, flagged by
the P4.47 lane and repaired at unification per the round's shared contract.

#### `b35ecea1` — 2026-08-14 — docs(help): record the backfill's transaction-shape divergence, and close the P4.D77 lane record

_Versions: core 0.0.546._

Recorded, in the code, why the section backfill's slicing loop is a single
transaction where v4's is not: a failure part-way through rolls the whole pass
back here and the next boot retries it, where v4 keeps what it wrote and waits
for a reindex. Comments only.

#### `b2494b62` — 2026-08-14 — test(help): pin that help sections keep no bookkeeping of their own, and bank the unported Guide search (P4.D77 tier 2, v4 24633026)

_Versions: core 0.0.545, harness 0.0.464._

Pinned the rule that section embeddings keep no bookkeeping of their own: the
dimension reconcile and the embedding-status rows count help documents, never
sections. Tests only.

#### `b1f8c92f` — 2026-08-14 — feat(help-search): rank a help page by its best section, and lead the result with it (P4.D77 unit 6, v4 24633026)

_Versions: core 0.0.544, harness 0.0.463._

`help_search` now ranks a help page by its best matching section as well as by
the page as a whole, and leads the result with that section rather than with the
first thousand characters of the file — which, on a long settings page, is a
table of contents and a preamble and never the answer. A page whose sections
have not been embedded yet still ranks exactly as it did.

#### `acea31dc` — 2026-08-14 — feat(embedding): embed help sections in the HELP_DOC job, and clear/re-fit them with their document (P4.D77 units 4-5, v4 24633026)

_Versions: core 0.0.543, harness 0.0.462._

Help sections are now embedded. The same job that embeds a help document fills
in its section vectors, skipping any that already have one, and a section whose
embedding call fails is logged and passed over rather than failing the whole
document. A full reindex clears section vectors alongside the document ones,
and re-fitting an embedding profile re-fits the sections too. Search does not
use them yet.

#### `075f809d` — 2026-08-14 — feat(help): slice help documents into section chunks on sync, and backfill upgraded instances (P4.D77 units 2-3, v4 24633026)

_Versions: core 0.0.542, harness 0.0.461._

Help documents are now sliced into sections for search. Each slice keeps the
nearest heading above it, and the slices are deliberately smaller than the
Scriptorium's — a settings page covers a dozen unrelated subsystems, and one
chunk that swallows four of them defeats the purpose. Nothing embeds or
searches them yet.

#### `07b2957f` — 2026-08-14 — test(import): the archive-bundle sweep and the preserveIds duplicate corner (P4.D65 item 5b/5c)

_Versions: harness 0.0.467._

Added the `help_doc_chunks` table — the storage for section-level help search.
Fresh instances get it from the re-dumped schema; existing instances get it on
the next boot. Nothing reads or writes it yet; the slicing, embedding, and
search follow.
Added tests for wiping archived-character bundles. Deleting all data spares
archived bundles by default; the explicit request to destroy them too was
untested, so nothing checked that the option was read at all. Also pinned how
imports behave when told to preserve incoming ids — an id that already exists
refuses the whole import before any duplicate-handling runs, which is what makes
the outcome predictable. Tests only; no shipped behavior changed.

#### `27ca83f2` — 2026-08-14 — test(archive): the banked archived-character refusals and the setParticipantStatus wrapper (P4.D65 item 5d/5e)

_Versions: harness 0.0.466._

Closed the last untested character-archive guards. Sending or listing mail as an
archived character, sending mail to one, asking an archived character what it
can reach, and picking one to answer a turn all refuse — and every one of those
refusals now has a test that fails if it is removed. Previously the guards were
in place but no test fixture contained an archived character, so nothing checked
them. Tests only; no shipped behavior changed.

#### `770de89a` — 2026-08-14 — test(archive): the four corpus-undriven arms (P4.D65 item 6) — and a stale oracle mock that was starving v4's import

_Versions: harness 0.0.465._

Closed the four corpus-undriven arms the character-archive review had recorded
as owed. The archive test fixture gained a default embedding profile, a
per-chat avatar-override face, and a standalone avatar thumbnail, and the
differential gained four cases: an archive refused because the instance
passphrase has not been entered, a rehydrate of a bundle sealed under an
earlier passphrase, an incomplete prune, and a rehydrate of a tombstone left by
the older archive revision. Regenerating the comparison also found that the
test harness had been stubbing out the default embedding profile, so the
reference implementation had been queueing no embedding work for restored
memories at all; with that removed, both implementations queue the same nine
jobs. Tests only; no shipped behavior changed.
Fixed Google Gemini requests sending the API key in the URL instead of the
`X-Goog-Api-Key` header. Google accepts both, so nothing was visibly broken, but
keys in URLs are the shape that ends up in proxy and server logs — and Quilltap's
own image-generation requests were already using the header, so the two paths
disagreed. Found by newly asserting the request headers the Google wire test had
been recording and ignoring.

#### `53737074` — 2026-08-14 — fix(harness): the sweep driver's staging-dependency class, made honest (P4.47 C)

_Versions: harness 0.0.469._

Developer tooling: the harness recipe sweep now detects a test family whose
oracle-regeneration recipe reads a scratch file no stage of that recipe builds —
the failure mode that leaves a family working on one machine and dead on another.
Four families that leaned on a sibling's scratch build now stage their own inputs
and were re-proven to run from nothing.

#### `f2d3cda2` — 2026-08-14 — fix(settings): the three sibling Zod arms answer v4's real 400 bodies (P4.47 A)

_Versions: core 0.0.547, harness 0.0.468._

Fixed three chat-settings fields answering an invented error message instead of
the one the server actually produces. Saving an out-of-range or wrong-typed
value under Answer Confirmation, Cheap LLM or Dangerous Content used to come
back as a single flat sentence; it now returns the same detailed, per-field
report v4 returns, listing every offending key at once. Dangerous Content also
now checks that the two uncensored-profile ids are real UUIDs, and the cheap-LLM
bag is checked at the point v4 checks it, so a request with more than one bad
field reports the same one v4 reports.
Documented a known gap: the Document Mode pane still has no formatting toolbar,
where v4 gives it the same one the composer has.

#### `7e9051c7` — 2026-08-14 — feat(salon): v4's composer layout, and retire the dogfood #75 band-aid (P4.9L units 4-5)

_Versions: SPA 0.5.485._

Fixed the Salon composer's layout (dogfood finding #75). The message-level tools
are back in the compact two-column block beside the message box rather than
strung out in one long row, so the box keeps the width it needs and the
"Type a message…" placeholder no longer clips. The interim fix that wrapped the
whole tool row below the box is gone with it.

#### `54f7f331` — 2026-08-14 — feat(salon): mount the formatting toolbar in the composer (P4.9L unit 3)

_Versions: SPA 0.5.484._

The Salon composer has its formatting toolbar. Switch a chat to composition mode
and the bold/italic/heading/list/quote/code buttons, the indent controls, the
emoji and symbol pickers, the roleplay template's delimiter buttons, and the
"Edit markdown source" toggle all appear above the message box — the same
toolbar the form fields have had, with the delimiters v4 shows only in a chat.

#### `313feede` — 2026-08-14 — feat(composer): the toolbar's roleplay-delimiter section (P4.9L unit 2)

_Versions: SPA 0.5.483._

The formatting toolbar now shows a roleplay template's delimiter buttons — the
narration button first, then the template's own, minus any that mark text the
same way narration does. Form fields, which have no template, are unchanged.

#### `3f522662` — 2026-08-14 — feat(composer): port v4's roleplay-delimiter toolbar transforms (P4.9L unit 1)

_Versions: SPA 0.5.482._

Ported v4's roleplay-delimiter toolbar transforms — the half of the composer
formatting toolbar the shared form-field toolbar never had. The narration button
a template synthesizes from its narration characters, the tooltip strings, the
line-prefix and tag-prefix transforms, and the rich-editor command that applies
them all now match v4 byte for byte, against vectors recorded from v4's own
toolbar, transforms and command handler.

#### `88adbbee` — 2026-08-14 — docs(porting): the 24633026 round's work orders — P4.D77 help-chunks drift, P4.47 maintenance smalls, P4.9L composer toolbar; P4.D65 remainder assigned

_Docs-only change._

Planned the next porting round against the new v4 baseline `24633026`
(section-level help embeddings and Guide content search). Wrote three new
work orders — the P4.D77 help-doc-chunks drift catch-up (server), the
P4.47 maintenance smalls (settings Zod-collapse arms, google-wire header
asserts, the sweep driver's staging-dependency class), and the P4.9L
composer formatting toolbar (SPA) — and assigned the open P4.D65
remainder (items 5–6) to the round. Docs only; no code changes.

#### `2c748fbc` — 2026-08-14 — docs(dogfood): the 2026-08-14 walk record — 38 steps, findings #79-#86, the wardrobe-tier and passphrase proofs discharged

_Docs-only change._

Dogfooded the 4.8.2/4.8.3 round against a copy of a real instance: 38 steps
across Setup, the instance lock, smart typography, both composer typeaheads,
the group wardrobe tiers, and the passphrase chain. Two defects found and
fixed (see the entries below); everything else behaved as v4 does. The
passphrase walk confirmed that changing it rewrites every archived-character
bundle — both archives rehydrated afterwards from bundles sealed under the old
passphrase.

#### `0477623e` — 2026-08-14 — fix(spa): typed backslashes doubled on the wire, and the typeahead arrows died under the mouse (dogfood #84, #85)

_Versions: SPA 0.5.481._

Fixed the arrow keys appearing to do nothing in the composer's emoji and symbol
menus (dogfood finding #85). They always worked with the mouse away from the
menu; with the pointer resting on it — where it usually is, since the menu opens
at the caret you just clicked — every keypress was undone by the row list being
rebuilt underneath the cursor. The rows are now updated in place when only the
highlight moved.

Fixed typed backslashes doubling on their way out of the editor (dogfood finding
#84). `$\alpha$` written in the composer reached the message as `$\\alpha$` —
broken LaTeX for KaTeX and a doubled backslash for any model reading the
transcript — and the same went for Windows paths and any other literal
backslash. The markdown serializer escaped the backslash where v4's never does.
Text written by a character was never affected; it does not pass through the
editor.

#### `f8924e60` — 2026-08-14 — fix(spa): the paused notice said something pause does not do (dogfood #83)

_Versions: SPA 0.5.480._

Corrected the paused-chat notice in the Salon (dogfood finding #83). It claimed
"the next character won't speak until you resume", which is not what pause does
in either app: pause stops the auto-chain between characters, and a message you
send is still answered once by whoever's turn it is. The notice is a v5-only
affordance (v4 shows none), so the wording was v5's alone — and it read one
ordinary reply as a broken pause. The engine behavior was already v4-faithful
and is unchanged.

#### `4ace3b2a` — 2026-08-14 — fix(spa): Enter leaves a fenced code block (dogfood #82)

_Versions: SPA 0.5.479._

Fixed a fenced code block being a one-way door in the chat composer (dogfood
finding #82). Typing ``` opens a code block and nothing in the markdown dialect
closes one, so every subsequent Enter only added another line to it — a writer
who fenced a snippet mid-message could never get back out to prose. v4 has an
escape v5 never got: Enter on a blank trailing line trims that line away and
opens a paragraph after the block. Ported with v4's conditions one for one, and
scoped to the composer, the only host v4 gives it to.

#### `5b25c54f` — 2026-08-14 — docs: the 4.8.2/4.8.3-round unification record — baseline → 48396682, all seven orders closed, the two review finds, gate numbers; order status headers; phase-4 candidates; CLAUDE.md round bullet + baseline (old paragraph archived)

_No crate versions bumped._

Unified the 4.8.2/4.8.3 drift catch-up + lock-order round (P4.D71 ∥ P4.D72 ∥
P4.D73 ∥ P4.D74 ∥ P4.D75 ∥ P4.46 ∥ P4.D76 — all seven closed) onto main; the
oracle baseline moves to `48396682` and the drift debt is cleared (v4 HEAD
`11553944` = 4.8.4 is tests+docs only, NO-PORT). Landed: the group wardrobe
tiers + bundle dissolution on both sides (dogfood finding #78 closed — v4's
Bug-61 fix ported with a deterministic race beat), the three composer features
whole (smart typography, `:` emoji + `\` Unicode typeaheads and pickers over
v4's code-identical engines and byte-copied corpora/datasets; bugs 62 + 63
fixed), the three chat_settings columns (D23 re-dump + boot ensure + Zod-exact
PUT arms), the P4.D68 open-before-lock escalation discharged (lock before any
partition open on boot/unlock/setup) with setup hardening and .dbkey
unknown-field preservation, and the SDK wire re-check (neutral outside the
self-dating version markers). The unification review caught and fixed two
would-have-shipped bugs: the empty typeahead menu swallowed Enter/Tab/arrows
(v4 falls through), and first-run Setup failed on a not-yet-created data dir
(the lock reorder outran the dir creation; every test had masked it). Gate:
430 test binaries / 2,067 / 0 with the round's env block; the 25 affected
oracle families regenerated fresh; ng test 319 files / 4,614 / 0; full
Playwright 212/212 zero skips. Versions: core 0.0.540, harness 0.0.460, host
0.0.69, web 0.0.72, SPA 0.5.478.

#### `9fdb4b14` — 2026-08-13 — test(wardrobe): prove the chat-start group tier where it actually runs (P4.D71 unit 5)

_Versions: core 0.0.534, harness 0.0.457._

Closed a coverage hole in the group wardrobe tier (P4.D71 unit 5, tests and
fixtures only — no behavior change). The chat-start pool's own group read and
the per-character tier in default-outfit resolution had shipped with no
differential able to see them: deleting either left every family green. The
chat-dialogs fixture gained a group holding two characters, with a default the
group supplies outright, a copy that shadows a Quilltap General item under a
different slot, and a personal opt-out of a group default — which together make
the whole precedence chain (character > group > project > general) visible in
the stored outfit rather than only in prompt text. Four v4 merge-rule unit
tests ported alongside.

#### `84418b46` — 2026-08-13 — feat(wardrobe): the group-scope read + transfers out of a group (P4.D71 units 3-4, v4 8600c83f)

_Versions: core 0.0.533, harness 0.0.456, web 0.0.71._

Finished the group wardrobe tier's two remaining surfaces (P4.D71 units 3–4).
`GET /api/v1/characters/{id}/wardrobe?scope=group` now serves the group tier —
the shared items in the `Wardrobe/` folder of every store belonging to a group
the character is a member of — as a standalone read for the client-side tier
merge; the dispatch verb `characterWardrobeList` takes the same optional
`scope`. The REST path itself is new to v5: the SPA reads this resource over
the dispatch channel, so the documented URL had never been served and would
have answered 404 to any client following the API reference. Wardrobe transfers
can also take an item back OUT of a group now: the source lookup scans the
source character's group stores between the project store and Quilltap General,
and a group source deletes by mount point like a project source does. Without
that, an item moved into a group was stuck there.

#### `01076672` — 2026-08-13 — feat(wardrobe): the group tier + bundle dissolution, server side (P4.D71 units 1-2, v4 8600c83f + 61574563)

_Versions: core 0.0.532, harness 0.0.455._

Ported the group wardrobe tier and bundle dissolution into the Rust core
(P4.D71 units 1–2; v4 `8600c83f` + `61574563`). Wardrobe items living in a
group store's `Wardrobe/` folder are now visible and resolvable everywhere
the other shared tiers are, with precedence character > group > project >
general — `wardrobe_list`, `wardrobe_read`/`_wear`/`_update`/`_archive`,
the chat outfit route's six item-resolving modes, chat-start default
resolution and the cheap LLM's pick, the outfit summary's cast-wide union,
the equipped reads in chat creation, the Aurora outfit whisper, story
backgrounds, image generation, buildContext's outfit cache, and the avatar
prompt. Group stores follow the character, never a co-participant; the
chat outfit summary is the one documented exception and reads the union of
the participants' memberships. Two v4 fixes ride along: `wardrobe_create`
now keys component resolution on the RECIPIENT of a gift, and its
equip-now path passes tiers at all (a new bundle's shared components did
not resolve before). Separately, wearing a resolvable bundled outfit now
dissolves it into its leaf garments at write time on every wear path
(wear / replace / add_to_slot, default outfits, the cheap LLM's chat-start
pick), so equipped state holds garments rather than one opaque card over
empty slot rows. Dissolution is recursive and fail-safe: a bundle whose
components cannot be resolved is stored whole exactly as before. No schema
or export-shape change; no migration.
P4.D73 unit 3 — the chat-settings PUT arms for the three 4.8.2 keys. Two
boolean arms at v4's schema-ordered positions after `composerSpellcheck`, with
its exact `Invalid composerEmoji value (must be boolean)` sentences, and a
route-level `SmartTypographySettingsSchema.parse` that defaults each absent key
on a partial bag. The reject bodies are v4's whole `ZodError.message` —
`JSON.stringify(issues, null, 2)`, which v4's route lets escape to
`getErrorMessage` and its `.includes('Invalid')` test turns into a 400 — so the
Zod issue shape is ported byte-for-byte rather than collapsed to a summary
sentence. `settings_routes_equivalence` gained a ten-case `composer_settings`
family (GET defaults, full/partial/empty bags, both wrong-typed booleans, a
null bag, a nested wrong type, a non-object, and the create branch) with its own
stale-oracle count guard; 72 cases match. A new `quilltap-web` wire test drives
the three keys through `POST /api/dispatch` over an instance whose columns were
dropped before boot, pinning the boot ensure and the raw-bag wire — an explicit
`null` must reach the handler as present-and-invalid, not read as an absent key.

#### `50ca5f0b` — 2026-08-13 — feat(db): the chat_settings composer/typography boot ensure (P4.D73 unit 2)

_Versions: core 0.0.536, host 0.0.67._

P4.D73 unit 2 — the `chat_settings` composer/typography boot ensure. v4
adds the three 4.8.2 columns through its migration runner; v5's runner is a
locked deferral, so they are re-homed as a boot repair over the main partition
(the P4.d7 / P4.D41 / P4.D63 precedents), each `ALTER TABLE ... ADD COLUMN`
carrying v4's exact type and DEFAULT clause and guarded by its own
column-presence check. Load-bearing rather than cosmetic: measured on an
un-ensured instance, the read tolerates (the settings screen renders with the
Zod defaults) but the PUT answers `500 sqlite error: no such column:
composerEmoji`, since `update_for_user`'s update branch is a plain `SET` —
the same class as the third Friday dogfood sighting. A fresh instance already
has the columns from the re-dump. v4's three migration pretty labels have no
v5 analog: recorded NO-PORT.

#### `0871733b` — 2026-08-13 — feat(settings): the D23 re-dump + v4 4.8.2's three chat_settings columns through the data layer (P4.D73 unit 1)

_Versions: core 0.0.535, harness 0.0.458._

P4.D73 unit 1 — adopted v4 4.8.2's three new `chat_settings` columns
through the data layer and the D23 fresh-schema re-dump. `fresh_schema.json`
and `chat_settings_seed.json` re-dumped from v4's live `generateDDL` at
`48396682` (31 -> 34 seed columns; the diff is exactly `composerEmoji INTEGER
DEFAULT 1`, `composerUnicode INTEGER DEFAULT 1`, `smartTypographySettings TEXT
DEFAULT '{"displayQuotes":false,"dashes":true,"ellipsis":true}'`).
`schema-key-order.json` regenerated and proven byte-identical (no exported
entity moved). All six `chat_settings.rs` sites carry the three columns, with
the new `SmartTypographySettings` struct in v4 Zod declaration order; the read
defaults each absent column to its Zod default rather than erroring or emitting
null, and the restore-facing `ChatSettingsCreate` deserialize defaults them so
a pre-4.8.2 archive still restores. The tier-2 corpus grew the three cells on
both write paths plus a surviving create op -- the create arm's cell values
were previously unobservable, because the only created row was deleted by a
later op in the same sequence.
Recorded the dispositions for v4's two 4.8.3 lifecycle fixes (docs only, no
code): bug 64 (first-run setup wedging every DB connection) is a NO-PORT — v5
caches no database handle above `Db`, lock/unlock rebuilds the world, and there
is no post-setup plaintext-to-cipher conversion to sequence because v5
provisions encrypted from byte zero — and bug 65 (the inert version guard) is a
NO-PORT because v5 has neither a version guard nor a migration runner. Both
records carry the evidence, and both real defects the survey turned up on the
way are fixed above. Left deliberately unfixed and flagged for a ruling:
`instance_settings.highest_app_version` is read by the Almanack but never
written by v5, so a v5-provisioned instance renders that premise null and
carries no downgrade tripwire — writing v5's own version into v4's semver guard
would be meaningless at best and lock-out-inducing at worst.

#### `bce6d432` — 2026-08-13 — fix(dbkey): a passphrase change preserves .dbkey fields v5 does not model (P4.46 tier 2)

_Versions: core 0.0.539, harness 0.0.460._

A passphrase change no longer strips fields Quilltap v5 does not model from
`quilltap.dbkey`. v4's version guard writes `minServerVersion` into the key file
by read-modify-write so an older binary is refused before the database is opened
at all; v5's re-wrap was a full replace, so the first passphrase change on a
v4-authored instance removed the floor. The re-wrap is now read-modify-write at
the JSON level — the ten modelled fields are replaced in place, everything else
survives, and the file keeps its key order. This is a deliberate divergence from
v4, measured from v4's own code in the cross-compat oracle: v4's
`changePassphrase` drops the field too, but v4 rewrites it every startup and v5
never would.

#### `f327baf9` — 2026-08-13 — feat(host,core): lock the instance before ANY partition opens — boot, unlock, and first-run setup (P4.46, the P4.D68 escalation)

_Versions: core 0.0.538, SPA 0.5.455._

The instance lock now covers every partition open — boot, unlock, AND
first-run setup (P4.46, the standing P4.D68 escalation). v4 acquires its
single-instance lock inside `connect()`, ahead of `new Database`; v5 acquired
it at the head of the host assembler instead, which is after `Db::open` has
opened all three partitions writable and, on first-run setup, after the whole
DDL replay and the baseline seed have been written. Against a contended
instance that meant writing to databases another live process believes it holds
exclusively — the class v4's bug-58 fix closes. Acquisition moves to a new
`EngineAssembler::pre_open` seam the engine calls at all three entrances
(re-entrant per PID, so setup's own claim does not block its later open);
release stays exactly at shutdown and the heartbeat / lock-lost semantics are
unchanged. Proven by a new contended-start test that plants a live foreign lock
and asserts the refusal arrives with the partitions byte-for-byte untouched and
nothing created; mutation-checked by reverting the ordering (all three go red).

First-run setup no longer eats the display-once encryption key, and a failed
setup can no longer brick the instance on retry. A late failure — the key
written, the instance provisioned, but the databases would not open — now
returns the pepper anyway with `requiresRestart` (v4 bug 64's contract: the key
is displayed exactly once, so it is never withheld behind an error), and the
setup wizard renders the key plus a restart notice instead of a bare error. A
second setup against a data directory that already carries `quilltap.dbkey` or
any partition refuses by name instead of minting a new pepper over the old one
(v5 hardening — v4's conversion-shaped setup has no analog); provisioning
enforces the same precondition its doc comment already claimed.

#### `ce1412b8` — 2026-08-13 — P4.D76: the 4.8.2 SDK refresh moved v4's provider wire not at all

_No crate versions bumped._

Fixed an ordering-dependent break in the SPA test suite: a clipboard stub was
installed as a non-writable property, so whichever spec next touched
`navigator.clipboard` in the same worker threw. Test-only.
Re-checked the provider wire against v4's 4.8.2 dependency refresh (P4.D76):
openai 7.2.0 → 7.4.0 and `@openrouter/sdk` 1.2.2 → 1.2.32 moved v4's outgoing
requests not at all. All four recorded corpora were regenerated against the
new SDKs (proven installed: the plugin and root lock files match their
node_modules exactly, 188 + 1,023 packages, zero mismatches): every request
body, url, method, input, and the one recorded refusal are byte-identical,
and the only differences are the self-dating version markers the corpora
started recording in P4.44 — `x-stainless-package-version` on the 80
openai-family rows and the speakeasy user-agent on OpenRouter's 13 SDK-path
rows. Anthropic's rows are byte-identical whole (its SDK did not move), as
are the google-request and response-body corpora. The six consuming
differentials pass by name against the fresh corpora, including the
OpenRouter pricing family with the real 1.2.32 SDK in the oracle loop — the
SDK's key remap and its 500-row page loop both survived the bump. The
provider manifests regenerate byte-identical against the 14 rebuilt plugins.
The google-wire corpus also gained the request headers it should have gotten
in P4.44 (that lane regenerated only its sibling); the bodies are unchanged
and nothing asserts those headers yet. No engine code changed.
P4.D72 unit 5 — the round's smaller wardrobe riders. The character editor's
"Aurora's Core whisper" card now stacks: label and description span the card
with the full-width dropdown beneath, instead of a two-column row where the
select took nearly the whole card and wrapped the label into a sliver (v4 4.8.2
`fe63547a`). The Salon sidebar's `.qtap` chat export anchor-clicks through the
download helper instead of setting `window.location.href` — the same fix v4
4.8.2 made for Electron, and v5 had the same exposure in the Tauri `qtap://`
webview, where the old line navigated the app window onto the API route rather
than downloading. Dogfood finding #78 is CLOSED (v4 fixed Bug 61; v5 ported the
fix), the wardrobe e2e's now-stale comment block is rewritten, and the walk
gains a beat that holds `chatOutfitGet` open across a Wear click and asserts
the edit survives the seed and reaches one `set_all` with both items — red
against the pre-fix code, green after.

#### `bd803042` — 2026-08-13 — port(spa): the group wardrobe tier in the loader (v4 4.8.2 `8600c83f`)

_Versions: SPA 0.5.458._

P4.D72 unit 4 — the SPA wardrobe loader reads a fourth tier: the shared
wardrobe of every group the character belongs to (v4 4.8.2 `8600c83f`, where
items moved into a group were invisible to everyone because nothing read them
back). `characterWardrobeList` gains an optional `scope: "group"` field per the
round's shared contract, the loader fires four parallel tier reads instead of
three, and the merge precedence becomes personal > group > project > general —
so a group's livery shadows a project's copy while a personal copy shadows
both, including the isDefault-false personal copy a character uses to opt out
of a shared default. Group items arrive with a null characterId like every
other shared tier, so they are wear-only with no extra labeling. The group read
fails soft, so the SPA still works against a server whose group arm has not
landed.

#### `2166296b` — 2026-08-13 — port(spa): client bundle dissolution (v4 4.8.2 `61574563`)

_Versions: SPA 0.5.457._

P4.D72 unit 3 — ported v4 4.8.2's client-side bundle dissolution into the SPA:
putting a bundled outfit on now stores its PARTS, never the bundle's own id, so
the wardrobe no longer shows one "Man in Black" card above four slot rows that
all read Empty. New `dissolve-bundles.ts` (dissolveBundleToLeaves /
layLeavesIntoSlots / dissolveBundlesInSlots) over a new `expand-composites.ts`,
with `wearItemIntoSlots` / `replaceItemIntoSlots` / the new `addItemToSlot` /
`computeDisplacedSlots` / `buildDefaultOutfit` widened to take the item lookup.
Threaded through the dialog's five wear sites and the outfit store's three
optimistic paths. Fail-safe throughout: no lookup, or a bundle whose parts do
not resolve, stores it whole exactly as before, and read-time expansion still
covers outfits equipped before the change. `replace` clears the union of the
slots the bundle designates and the slots its pieces land in. v4's 347-line
dissolve suite ported case-for-case (minus its persisted-repo describe, which
is P4.D71's) plus the default-outfit case; mutation-proven at both levels —
inverting the dissolve guard fails 16, dropping the lookup at the call sites
fails 4 (dialog) and 1 (store). The server stays authoritative for persisted
slots; this is the optimistic mirror.

#### `d8678abc` — 2026-08-13 — port(spa): the bug-61 staged replay in the wardrobe dialog (v4 4.8.2 `07d4ccce`)

_Versions: SPA 0.5.456._

P4.D72 unit 2 — wired v4 4.8.2's Bug 61 fix into the SPA wardrobe dialog: a
Wear clicked before the worn snapshot arrives is no longer lost. The gesture
(not its result) is recorded in a per-character pending-mutator queue, the
seeding effect drains that queue and rebases it onto the true worn slots
instead of cloning them, and the Done flush classifies through
`classifyStagedOutfits` — a character staged against no baseline is now put to
the operator ("Word of what {names} is presently wearing never reached us…")
instead of being silently counted as clean and closed as if saved. Declining
keeps the dialog open so a late snapshot can still seed and save. v4's race
suite is ported case-for-case (4 tests, held-open `chatOutfitGet`, asserting
ONE `set_all` carrying both the already-worn shirt and the just-clicked hat)
and mutation-proven: restoring the three pre-fix parts fails three of four.
Closes dogfood finding #78 in the SPA.

#### `8feb94b6` — 2026-08-13 — port(spa): the staged-live-outfits pure module (v4 4.8.2 `07d4ccce`, bug 61)

_No crate versions bumped._

P4.D72 unit 1 — ported v4 4.8.2's new `lib/wardrobe/staged-live-outfits.ts`
into the SPA as `app/wardrobe/staged-live-outfits.ts`: `rebaseStagedSlots`
(replays gestures staged before the worn snapshot arrived onto the real
slots) and `classifyStagedOutfits` (separates "nothing changed" from "we
never learned what clean was" — the Bug 61 silent loss). `equippedSlotsEqual`
moved from `equipped-slots.ts` to the new module, matching v4's own move out
of the dialog. v4's unit suite ported case-for-case (11 tests) and
mutation-proven: reverting either helper to its pre-fix shape fails four
cases. No behavior change yet — the dialog wiring is unit 2.
Closed P4.D74 (the 4.8.2 smart-typography + renderer drift, SPA): the
rule engine, render-time quote curling, type-time dashes and ellipsis,
bugs 62 and 63, the katex pin, the settings card, and a live Playwright
walk. Gate: ng test 304 files / 4,283 tests, ng build clean, full
Playwright 206 passed / 1 gated skip.

#### `06dd13bb` — 2026-08-13 — fix(spa): the settings service must not write signals while being constructed (P4.D74 unit 8)

_Versions: SPA 0.5.467._

Fixed a render-breaking regression in the smart-typography settings
service (P4.D74 unit 8): it wrote its signals while being constructed,
which in a zoneless app happens inside the parent template's reactive
context — Angular threw NG0600, the render unwound, and the Salon chat
list came up empty after navigating back out of a chat. The initial
values are seeded at field level now and every later write is untracked.
Caught by the full Playwright suite, which is the only place it was
visible.

#### `0994c21e` — 2026-08-13 — test(spa): the smart-typography live walk + v4's spec mirrored (P4.D74 unit 7)

_Versions: SPA 0.5.466._

Walked smart typography live (P4.D74 unit 7): four Playwright beats —
the dash ladder and the ellipsis typed into the real composer, the
one-Backspace revert and its one-keystroke window, the code-fence bail,
and the settings card's try-it box. The display-quotes beat waits on the
settings column and is gated by a named constant. v4's feature spec is
mirrored under `docs/v4/`; its help-doc section is banked for `p4.9i2`,
which is where v5's help surface still lives.

#### `5f026785` — 2026-08-13 — feat(spa): the Smart Typography settings card (P4.D74 unit 6)

_Versions: SPA 0.5.465._

Added the Smart Typography settings card (P4.D74 unit 6), between Text
Replacement and Token Display in Settings → Chat: the curly-quote
display toggle, the two type-time rules, and a try-it box that runs the
same engine the composer does, so the preview cannot disagree with the
real thing. Each save sends the whole bag, so flipping one rule cannot
drop another.

#### `c49df581` — 2026-08-13 — port(spa): smart typography Part B — type-time dashes and ellipsis (P4.D74 unit 5)

_Versions: SPA 0.5.464._

Ported smart typography Part B into the composer and the Document Mode
editor (P4.D74 unit 5): typing `--` gives an en dash, `---` an em dash
and `...` an ellipsis, over the same engine v4 uses, with v4's revert
semantics — one Backspace puts the literal characters back, one
Cmd/Ctrl+Z undoes the substitution and nothing else, and any other key
closes the window. Nothing fires in code blocks, inline code, source-mode
editors or during IME composition, and pasted text is left alone. Each
rule follows its own toggle, live.

#### `c268189a` — 2026-08-13 — fix(spa): bug 63 — typing aids no longer rewrite code (P4.D74 unit 4)

_Versions: SPA 0.5.463._

Fixed bug 63 in the composer (P4.D74 unit 4): text replacements fired
inside fenced code blocks and inline code runs, because a code block is
a textblock like any other and the inline code mark was never consulted.
Both typing aids now bail through one shared `code-context` helper —
which also covers the case where the code mark is armed but nothing has
been typed yet — so their bail lists cannot drift apart again.

#### `49751f9c` — 2026-08-13 — port(spa): smart typography Part A — render-time quote curling (P4.D74 unit 3)

_Versions: SPA 0.5.462._

Ported smart typography Part A into the SPA renderer (P4.D74 unit 3):
`remark-smartypants` curls straight quotes at render time when
`smartTypographySettings.displayQuotes` is on, at v4's exact position in
the pipeline and behind v4's two cached processors. Stored content is
never modified. Code, math and link targets are excluded structurally,
and a roleplay template that claims a quote character as a delimiter —
or a dialogue detection naming a straight quote without its curly forms
— suppresses curling for the whole render. The renderer reads the
setting itself, as v4 does, so every message surface agrees; the render
memo now keys on it so a toggle repaints what is already on screen. The
v4-captured parity corpus grows 51 → 67 vectors.

#### `562fdcee` — 2026-08-13 — fix(spa): bug 62 — curly-quoted dialogue is highlighted again; katex → 0.18.4; corpus recaptured (P4.D74 unit 2)

_Versions: SPA 0.5.461._

Fixed bug 62 in the SPA renderer and recaptured the v4 parity corpus
(P4.D74 unit 2): the default dialogue pattern and the paragraph-level
detection chars now carry the curly double quotes as v4 does since
`c7892132` (in `\\u` escape form, deliberately), the `katex` pin moved to
v4's resolved 0.18.4, and `markdown-fixtures.json` was recaptured from
v4's real renderer at `48396682`. Exactly one vector's bytes moved —
curly-quoted dialogue is highlighted now, where before it was not.

#### `9ec51bbe` — 2026-08-13 — port(spa): the smart-typography rule engine twin + v4's fixture corpus (P4.D74 unit 1)

_Versions: SPA 0.5.460._

Ported the smart-typography rule engine into the SPA (P4.D74 unit 1):
v4's stdlib-only `lib/smart-typography/engine.ts` copied byte-identical
below its docblock, its 15-vector fixture corpus copied byte-for-byte,
and v4's engine test suite ported case-for-case to vitest (23 tests).
The engine has no callers yet — the renderer, the composer plugin and
the settings card follow in later units of the same order.
P4.D75 gate: SPA unit suite 310 files / 4,420 tests green, `ng build` clean with
the two character datasets served as static assets and absent from every JS
bundle, and the lane's four e2e beats green against the real server.

#### `84538e45` — 2026-08-13 — test(e2e): four live beats for the character-insertion feature, and the v4 docs mirror (P4.D75 unit 6)

_Versions: SPA 0.5.475._

Added four end-to-end beats for the character-insertion feature (`:smile:` in
the live composer, the menu's Enter/Escape, `\to ` keeping its space, and a
toolbar picker inserting into a markdown field), and mirrored v4's two feature
specs into `docs/v4/`.

#### `8519d42e` — 2026-08-13 — port(spa): the two Composer settings toggles for the emoji and symbol shortcuts (P4.D75 unit 5)

_Versions: SPA 0.5.474._

Added the two Composer settings toggles for the emoji and symbol shortcuts
(`composerEmoji` / `composerUnicode`, both on by default). They govern the
automatic `:` / `\` triggers only; the toolbar buttons are never gated.

#### `789a0267` — 2026-08-13 — port(spa): the emoji and symbol pickers — the shared panel, both popovers, the two toolbar buttons (P4.D75 unit 4)

_Versions: SPA 0.5.473._

Added the emoji (☺) and symbol (Ω) pickers to the editor's formatting toolbar:
a shared browsable panel with search, a recently-used row, and a grid grouped by
emoji category or Unicode block, over the same match engine the typeaheads use.
Neither button is gated by the composer settings — those govern the automatic
`:` / `\` triggers only. In raw-source mode the pick lands at the textarea
caret, where v4 loses it.

#### `dc6f747c` — 2026-08-13 — port(spa): mount the typeaheads in the composer and Document Mode, gated by the two chat settings (P4.D75 unit 3)

_Versions: SPA 0.5.472._

Mounted the composer emoji/Unicode typeaheads in the two hosts v4 mounts them
in — the Salon composer and the Document-Mode editor — gated by the
`composerEmoji` / `composerUnicode` chat settings (default on, read from the
shared settings query). Form-field editors do not get them. The pickers' insert
path landed on the editor handle.

#### `1466d436` — 2026-08-13 — port(spa): the char-typeahead adapter — the caret-anchored menu ProseMirror had no precedent for (P4.D75 unit 2)

_Versions: SPA 0.5.471._

Ported the composer's `:` emoji and `\` Unicode typeahead into the SPA's
ProseMirror editor (v4's `CharTypeaheadPlugin`): the menu-free commit
keystrokes (`:smile:`, `\to `), the caret-anchored menu with its flip/align
geometry, the code-block / inline-code / math-span / IME bails, the lazy
dataset fetch, and v4's one-undo contract. v4's two plugin suites are ported
case-for-case (56 cases) over a real editor and the real dataset, plus 11 cases
for the menu surface. Not yet mounted in a host — that is the next unit.

#### `ebfb104c` — 2026-08-13 — port(spa): the char-insert engine twin — v4's `lib/char-insert/` verbatim, its four corpora and both datasets byte-copied (P4.D75 unit 1)

_Versions: SPA 0.5.470._

Ported v4's character-insertion engine (`lib/char-insert/`) into the SPA at
`apps/web/src/app/editor/char-insert/` — the shared logic behind the composer's
`:emoji` and `\unicode` typeaheads and both toolbar pickers. Code-identical to
v4's (doc comments aside); the four behavior corpora and both dataset assets
are byte-identical copies, and v4's four unit suites are replayed case-for-case
(174 cases). The datasets are served from `public/` at v4's paths and fetched
lazily, so they never enter the bundle.

#### `595fb678` — 2026-08-13 — docs: trim CLAUDE.md under the per-turn size limit — archive the 2026-07-10→07-30 round bullets and the superseded oracle-baseline chain to claude-md-status-history.md (verbatim, diff-verified); CHANGELOG

_Docs-only change._

Trimmed CLAUDE.md back under its per-turn size limit (202KB → 73KB, docs
only, no code): the round bullets from 2026-07-10 through the `5cc76688`
round (2026-07-30) and the whole superseded oracle-baseline history chain
moved verbatim (diff-verified) to
`docs/developer/porting/claude-md-status-history.md`, replaced in
CLAUDE.md by a compressed digest bullet and a pointer. Rounds from P4.D29
(2026-07-30) onward and the current `03154b72` baseline paragraph stay in
place. Second application of the 2026-07-10 precedent (the unit journal's
move to `status-log.md`); going forward, displaced baseline paragraphs
and aged-out round bullets migrate to the archive instead of chaining.

#### `cb0c4aab` — 2026-08-13 — docs: plan the 4.8.2/4.8.3 drift catch-up round — seven work orders (P4.D71–P4.D76 ∥ P4.46), round record, CHANGELOG

_Docs-only change._

Planned the 4.8.2/4.8.3 drift catch-up round and wrote seven work orders
(docs only, no code): v4 main moved `03154b72` → `48396682` with releases
4.8.2 + 4.8.3 — the group/shared wardrobe tiers + bundle dissolution +
v4's fix for Bug 61 (the staged-edit race this port filed), the three
composer features (smart typography, emoji, Unicode) with their three new
`chat_settings` columns, the bug-64/65 lifecycle fixes, and an SDK
refresh. Orders: P4.D71 (wardrobe server) ∥ P4.D72 (wardrobe SPA, closes
dogfood finding #78) ∥ P4.D73 (settings columns + D23 re-dump) ∥ P4.D74
(smart-typography/renderer SPA + bugs 62/63 + katex) ∥ P4.D75
(char-insert SPA) ∥ P4.46 (lock-before-open reshape + setup hardening —
the standing P4.D68 escalation, widened by survey to unlock and Setup) ∥
P4.D76 (provider SDK wire re-check). Round record in the status log; the
oracle baseline moves to `48396682` when the round unifies.

#### `601371d0` — 2026-08-12 — docs: the 4.8.1-drift round record — baseline → 03154b72, all three orders closed, the open-before-lock escalation ordered next, gate numbers

_Docs-only change._

Unified the `03154b72` 4.8.1-release drift catch-up round (P4.D68 ∥
P4.D69 ∥ P4.D70, plus the parallel wardrobe-flow deflake that was sitting
uncommitted in its worktree) onto main — the oracle baseline moves to
`03154b72` and the drift debt is cleared. v4 released 4.8.0 and 4.8.1 and
now develops on two branches (main + bugfix); drift-checks widen to both.
Landed: the bug-60 one-file dbkey port (the phantom
`quilltap-llm-logs.dbkey` write shed; the cross-compat oracle grown to
both directions with mutation-proven cross-side tripwires), the bug-59
measured convergence + fail-closed seed pin, the bug-58 NO-PORT with the
writable-open lock enumeration — whose one escalation (v5 boots the
databases writable BEFORE taking the instance lock) is now the top named
next-round candidate — the repo-wide spelling sweep wired into the
workspace gate, the `db characters` shell completions (Tier R red-first →
188/0), the standalone streaming indicator + About release-freshness
mirror, and the wardrobe `set_all` beat deflaked (the lost-edit race
measured at 3 ms, kept v4-faithful, filed upstream as v4 Bug 61). The §3
review found no blocking defects. Gate: 426 test binaries / 2,017 tests /
0 failed with the round's env block (families by name, zero SKIP, fresh
at the pin); clippy both feature sets; release build; ng test 298 files /
4,142 / 0; ng build clean; full Playwright 202/202 zero skips (5.1 m).
Versions: core 0.0.531, harness 0.0.454, cli 0.0.9, SPA 0.5.454.

#### `f5e2ac7b` — 2026-08-12 — docs(about): mirror v4's 4.8.0 release-freshness sweep of the feature list (P4.D70 unit 2)

_Versions: SPA 0.5.453._

P4.D70 unit 2: mirrored v4 `5fdd7bed`'s About release-freshness sweep into
the SPA's Key Features list — the Docker bullet notes the filesystem
document stores bound through to the container, a new "The Workspace"
bullet lands before Aurora, Aurora gains encrypted-bundle archiving,
Pascal is rewritten around four state registers plus user-authored custom
tools and the Workbench, and the Foundry gains the Almanack. Every named
feature is ported in v5, so the new text is truthful here. Pinned by spec
(static template prose has no other guard against rot); v5 keeps reporting
its own version, not v4's.

#### `aefbcc56` — 2026-08-12 — fix(salon): give the streaming thinking indicator room above a tool block (P4.D70 unit 1)

_Versions: SPA 0.5.452._

P4.D70 unit 1: re-ported v4 `fed5b5da` — the streaming "still working" quill
now takes a line of its own (with the tool block's own top spacing) when the
trailing prose segment is empty and a tool batch precedes it, instead of
rendering inline where the quill's feather crowds the block above it. Inline
placement mid-prose is unchanged. v4 splits the live prose into an
interleaved parts array to ask that question; v5 renders one prose blob
followed by the batches, so the predicate reads the same batch offsets
directly. Both arms pinned by spec, mutation-checked.

#### `7881e5f0` — 2026-08-12 — fix(cli): teach shell completions the db-characters sub-subverbs (P4.D69, v4 db195fba)

_Versions: cli 0.0.9._

P4.D69 (the 4.8.1 CLI shell-completions drift): mirrored v4 `db195fba`'s
`db characters` completion arms into all three shell templates
(bash/fish/zsh) in `quilltap-cli` — the five sub-subverbs
(status/archives/archive/rehydrate/export) and their flags, including
zsh's early `return` after the subverb `_describe`. Byte-copied from a
worktree pinned at v4 `03154b72`; Tier R flipped exactly the three
`completion <shell>` cases red before the fix (188 cases / 3 failures)
and runs green after (188 / 0). quilltap-cli 0.0.8 → 0.0.9.

#### `4f8778a2` — 2026-08-12 — docs(v4): mirror the 4.8.1 bug write-ups + dbkey doc corrections; record the baseline-move neutrality proof (P4.D68 unit 5)

_Docs-only change._

P4.D68 unit 5 — the baseline-move neutrality proof + the docs mirror.
The two seed-transitive oracle families (`reset_builtins_equivalence`,
`seed_avatars_equivalence` — the only cases importing v4's
`seed-initial-data`) regenerated and re-ran green at the `03154b72` pin
through the sweep driver, proving the bug-59 drift happy-path-neutral;
the oracle baseline moves to `03154b72` at unification. Mirrored into
`docs/v4/`: the three 4.8.1 bug write-ups (a new `bugs/fixed` mirror
subtree), the bug-60 corrections to DATABASE_ENCRYPTION / DDL /
BACKUP-RESTORE / DEPLOYMENT, and `help/database-protection.md` (a new
`help/` mirror subtree), with that help doc joining the `p4.9i2` bank
by name.

#### `516586da` — 2026-08-12 — feat(tooling): the repo-wide Quilltap spelling sweep, wired into the workspace gate (P4.D68 unit 4)

_Versions: harness 0.0.454._

P4.D68 unit 4 — mechanical spelling enforcement. The v5 analog of v4
4.8.1's repo-wide checker: `harness/tools/check_spelling.py` fails on
any case-insensitive quilt-based misspelling of "Quilltap" in tracked
text files outside a reasoned allowlist (rule-stating docs, the `docs/v4`
mirror, a correctly-spelled line-exception marker), and a new
`quilltap-harness` test (`spelling_guard`) runs it under
`cargo test --workspace`. The first sweep found five closed work orders
quoting the misspelling to state the rule — allowlisted, text untouched.

#### `d5872b69` — 2026-08-12 — fix(seed): pin the bug-59 fail-closed gate; record the bug-58 lock enumeration + escalation (P4.D68 units 2-3)

_Versions: core 0.0.531._

P4.D68 units 2–3 — the bug-59 and bug-58 dispositions, measured. Bug 59
(v4 4.8.1: a failed read must not trigger first-startup seeding) is a
structural convergence: v5's sample-content gate already fails closed
(`character_count` returns `Result`; the `Err` arm warns and returns
without seeding) and the embedding-profile seed runs only inside fresh
provisioning — no code change; a new regression pin
(`failed_gate_probe_seeds_nothing`) ports the intent of v4's
probe-throws test (a populated-but-unreadable database seeds nothing).
Bug 58 (migrations bypass the instance lock) is NO-PORT — v5 has no
migration runner; the lock-coverage enumeration confirmed every v5
writable entrance sits behind the instance lock (host/web/tauri boot,
the spine's in-process writers, the CLI's no-override write lock, the
CLI archive verbs through the running server), and v5's lock semantics
match v4's contract (re-entrant same-PID, same-host dead-PID reap, VM
heartbeat freshness). ONE escalation recorded in the order header: the
boot path opens the databases writable (journal-mode header writes)
BEFORE the lock is acquired, where v4 locks before opening — needs its
own small order.

#### `aa55e100` — 2026-08-12 — fix(dbkey): one pepper, one file — change_passphrase sheds the phantom quilltap-llm-logs.dbkey (v4 4.8.1 bug 60, P4.D68 unit 1)

_Versions: core 0.0.530, harness 0.0.453._

P4.D68 unit 1 — the bug-60 port: `change_passphrase` writes one `.dbkey`
file. v4 4.8.1 removed the vestigial `quilltap-llm-logs.dbkey` write (the
remnant of a per-database-key design that was never built; nothing ever
read it, and it could hold a stale wrapping), and v5 follows: the second
write in `quilltap-core::dbkey::change_passphrase` is gone, a pre-existing
stale file on disk is left untouched (v4 parity — no deletion), and the
doc comment now carries v4's one-pepper-one-file reasoning. Proven at the
new `03154b72` pin by the extended dbkey cross-compat differential, now
BOTH directions: v4's REAL `changePassphrase` (driven fresh in the pinned
worktree via the new `QT_DBKEY_V4_OUT` leg of
`verify-dbkey-crosscompat.ts`) writes one file that v5's reader unlocks
(new `reads_v4_changed_passphrase_dbkey`), and v5's rewrap writes one
file that v4's real code unlocks — with the one-file outcome asserted on
both sides of the fence and mutation-proven (re-adding the second write
turns four Rust arms and the oracle assertion red). The
`archive_reencrypt_tier2_equivalence` family regenerated fresh at the pin
(its comparands never observe the data-dir file set — recorded; the
one-file arm lives in the dbkey vehicle) and the
`change_passphrase_archive_sweep` wire test re-ran green.

#### `32bde325` — 2026-08-12 — docs: work orders for the 03154b72 4.8.1-release drift catch-up round (P4.D68 ∥ P4.D69 ∥ P4.D70)

_Docs-only change._

Planned the `03154b72` 4.8.1-release drift catch-up round and committed
its three work orders (docs-only; no code moved). v4 released 4.8.0 and
4.8.1 and its main moved eight commits past `de9f70bf`; the effective
lib/app drift is v4's bugs 58–60 (instance lock, fail-closed seeding, the
phantom `quilltap-llm-logs.dbkey` — which v5's `change_passphrase`
reproduces faithfully and must now shed), the `db characters` shell
completions, and two client fixes (the standalone streaming indicator
above a tool block; About release-freshness). Orders:
`work-orders/p4.d68-dbkey-onefile-seed-lock-drift.md` (server; owns the
baseline move to `03154b72`), `p4.d69-cli-completions-archive-drift.md`,
`p4.d70-streaming-indicator-about-spa-drift.md`. Operational hazard
recorded in all three: the v4 checkout now sits on the `bugfix` branch
(4.8.2-bugfix.0), so every oracle regen this round pins a detached
worktree at `03154b72`, and future drift-checks must watch both v4
branches.

#### `e8e90e12` — 2026-08-11 — docs: the P4.D65-finish + sweep-rot round record — baseline → de9f70bf, P4.D63 + P4.45 closed, gate numbers

_Docs-only change._

Unified the P4.D65-finish + sweep-rot round (P4.D65-resumed ∥ P4.45) onto
main — the oracle baseline moves to `de9f70bf` and the Bug-57 drift debt is
cleared (v4 converged onto this port's twice-linked-blob rehydrate dedupe;
the divergence pins retired to plain equalities, with the fixture-driven
equality arm mutation-proven). P4.D63 and P4.45 are CLOSED; P4.D65 stays
open on items 5–6 only (the banked round-1 tier-2 arms and the owed corpus
arms). The unification review fixed two things before merge: the
re-encryption sweep's upload-failure reason now carries v4's `uploadRaw`
wrapper sentence (it leaked the bare backend error into a string the
settings UI surfaces verbatim — pinned by a failing-upload unit test), and
the archive-holder lookup's error arm now answers v4's fixed
`Failed to delete file` 500 instead of raw database error text. Gate: 425
test binaries / 2,013 tests / 0 failed with the round's env block; the 20
affected differential families regenerated fresh at `de9f70bf` and run by
name through the repaired sweep driver, zero skips; clippy both feature
sets; release build; ng test 298 files / 4,138; full Playwright 202/202
zero skips. Versions: core 0.0.529, harness 0.0.452, host 0.0.66, web
0.0.70; cli, tauri, SPA unchanged.

#### `746a7cb1` — 2026-08-11 — fix(harness): the green proof over all 39 repaired families, and the four broken recipes it found (P4.45 unit 4)

_Versions: harness 0.0.451._

Proved all 39 repaired differential families runnable end-to-end through the
sweep driver — regenerate the oracle from v4, then run the diff, one clean
invocation each — and committed the per-family results. The run exposed four
more broken recipes and fixed them: one that annotated a command line with a
parenthesis (bash died on it), one that expected a temp-built fixture to
"already exist" and so was dead the first time the temp directory was cleaned,
and two whose regeneration step was silently dropped because it invoked its
tool by full path, which the extractor did not recognize as a command. A fifth
suspected class — a recipe reading a fixture no step of it builds — is recorded
with its candidates rather than half-detected. Developer tooling only.

#### `300708f3` — 2026-08-11 — fix(harness): the sweep driver's stale-oracle deletion covers jest families too (P4.45 unit 3b)

_Versions: harness 0.0.450._

Closed a hole in the sweep driver's own stale-oracle guard: it deleted a
family's previous oracle only when the recipe wrote it through a shell
redirect, which is the tsx convention. Every jest-based family writes through
`QT_ORACLE_OUT=`, so its old oracle survived — and a regen that quietly
produced nothing would let the run pass against a previous round's file. That
covers the majority of the database-level and mocked-LLM families.

#### `de19bafe` — 2026-08-11 — fix(harness): the sweep driver refuses an unattributable run line (P4.45 unit 3)

_Versions: harness 0.0.449._

Made the sweep driver refuse an unattributable recipe outright: a family whose
run line does not name its own test binary is now reported as broken and will
not execute, so the skip-masquerade cannot be reintroduced silently. A
positional test-name filter does not count as a scope — it matches across
every binary in the crate. Mutation-proven by reverting a repaired header in
place and watching the refusal fire.

#### `d19d915d` — 2026-08-11 — fix(harness): scope every differential recipe's cargo-test line to its own binary (P4.45 unit 2)

_Versions: harness 0.0.448._

Scoped every differential family's `cargo test` recipe line to its own test
binary. Thirty-two families' run lines read `cargo test -p quilltap-harness`
with no `--test <family>`, so following the recipe compiled and ran EVERY
harness test binary with only one family's oracle env var set: every sibling
family found its own env var missing, printed its skip notice and passed. A
run like that proves nothing about the family whose recipe it is, and the
sweep driver's fail-on-skip guard could not tell whose skip it was seeing —
which is why three consecutive rounds had to re-run these families by hand.
Developer tooling only — no test body, fixture or oracle case changed.

#### `674a27c4` — 2026-08-11 — fix(harness): the sweep driver reads recipe lines by INDENTATION, not by first word (P4.45 unit 1)

_No crate versions bumped._

Fixed the sweep driver's prose-leak class at its root: a differential
family's regeneration recipe is now recognized by INDENTATION (a command
sits two spaces past `//!` or `*`; prose sits at the marker's margin)
rather than by guessing from the line's first word. Twenty-one doc
sentences opening with a command word were being extracted as shell, six of
them into regen scripts that would die on a bash syntax error. The one
header that used a markdown code fence instead of indentation
(`memory_weighting_equivalence`) was converted to the tree's convention.
Developer tooling only — no shipped behavior changed.

#### `012d0301` — 2026-08-11 — feat(archive): the passphrase-change re-encryption sweep, the held-bundle delete guard, and the export archive keys

_Versions: core 0.0.528, harness 0.0.447, host 0.0.66, web 0.0.70._

Finished P4.D65's resume list. Changing the instance passphrase now
re-encrypts the archive library with it: the sweep runs as phase two of
`POST /api/v1/system/unlock?action=change-passphrase`, and the response
carries an `archives` summary naming any bundle left holding the old
passphrase. A failed sweep does not fail the passphrase change. Deleting a
held archive bundle is refused (`ARCHIVE_BUNDLE_HELD`, naming the character
whose only copy it is) unless `force=true`. Archived characters no longer
appear in the export picker. Exported character records carry the three
archive columns in v4's schema slot — the committed key-order table had
been stale since the columns landed, so the keys were being appended at the
end of every character record. Two real defects were caught by the new
differentials: the re-encryption sweep would have panicked the moment it
was called from the async dispatch arm (it blocked on the writer channel),
and the export key order above. New: a six-case archive re-encryption
differential over v4's real sweep (planted plaintext / foreign-passphrase /
missing-bytes bundles, plus one the archive service really wrote), a
live web-edge test that archives, changes the passphrase and rehydrates,
and five new arms on the archive differential. v4's Bug-57 fix
(`de9f70bf`) converged onto this port's twice-linked-blob dedupe, so the
divergence markers retire and the fixture grew the shape as a
plain-equality arm; the oracle baseline moves to `de9f70bf`.

#### `6917fc9b` — 2026-08-11 — docs: plan the P4.D65-finish + sweep-rot round (the de9f70bf Bug-57 convergence folds into P4.D65; P4.45 written)

_Docs-only change._

Planned the P4.D65-finish + sweep-rot round (docs only). v4 fixed its Bug
57 at `de9f70bf` — converging onto this port's twice-linked-blob rehydrate
dedupe — so the drift folds into P4.D65's resumed lane as a Round-3
addendum (pin retirement, a plain-equality fixture arm, import-graph
regens, baseline move to `de9f70bf`; zero v5 source change needed —
verified at planning). A new parallel maintenance order, P4.45, repairs
the sweep-driver recipe rot: the six turn families' unscoped run lines
(the SKIP-masquerade), the `diff`-prose classifier trap, self-test growth,
and the 30-file census. Round-planned record in the status log.

#### `5ecaff36` — 2026-08-11 — fix(cli): §3 review — propagate swallowed SQL errors, the falsy default sub, the Tier R wire-parity assertion; round docs

_Versions: cli 0.0.8._

Unified round 2 of the character-archive catch-up (P4.D65 ∥ P4.D66 ∥
P4.D67) onto main — the oracle baseline moves to `ed8934f1` and the Bug-56
drift debt is cleared. The archive service is LIVE end-to-end: archive
packs a character into an encrypted bundle, verifies it, commits the
tombstone and prunes the vault in place; rehydrate brings them back at
their original ids; both dispatch verbs and the SPA's four action beats
run live (the archive e2e spec is 10/10). The CLI gained the whole
`db characters` family (status / archives / archive / rehydrate / export
with offline bundle decrypt), Tier R 136 → 188 cases against v4's real
launcher. The Bug-56 base-path-availability module landed with byte-exact
diagnosis sentences, the folder-create 409, and the store-create warning
rewrite. The unification review caught and fixed, before merge: the
round's cross-lane blind spot (no lane served the CLI's
`POST /api/v1/characters/{id}?action=` URL on v5's server — a thin REST
edge now delegates into the dispatch arms, pinned by a live web-edge
test); a missing character answering 500 where v4's route answers 404;
four CLI sites swallowing SQL errors v4 propagates; the archive
differential's blindness to `background_jobs`; and one deliberate
divergence with the v4-side fix queued — v4 cannot rehydrate a character
whose vault links the same bytes twice (per-link blob export duplication ×
an undeduped preflight list), while v5's preflight now dedupes carried
blob ids. Gate: 423 test binaries / 2,010 tests / 0 failed; the round's
differentials fresh at `ed8934f1`; clippy both feature sets; release
build; ng test 4,138; full Playwright green with the four archive action
beats active. P4.D66 and P4.D67 close; P4.D65 stays OPEN at its resume
list (re-encrypt wire, files-delete guard, export filter, banked arms).
Versions: core 0.0.526, harness 0.0.446, cli 0.0.8, web 0.0.69,
SPA 0.5.451.

#### `c0d0cd97` — 2026-08-11 — feat(archive): the character-archive service + the two dispatch verbs (P4.D65 unit 1)

_Versions: core 0.0.523, harness 0.0.444._

Ported the character-archive service (P4.D65 unit 1): `archiveCharacter`
and `rehydrateCharacter` now work end to end, and the two dispatch verbs
`characterArchive` / `characterRehydrate` no longer refuse. Archiving
packs the character into an encrypted `.qtap` bundle, verifies it by
decrypting the bytes that will actually be persisted, commits the
tombstone, and prunes the vault in place — keeping the ten managed
documents, the wardrobe and the avatar links, so an archived character is
still a readable page and old messages keep their faces. Rehydrating
restores the pruned material at its original ids, un-flags the tombstone,
brings absent chat seats back, and re-chunks the vault. New committed
`character-archive-{main,mount}.db` fixture family and an eight-case
tier-2 differential against v4's real archive service, diffing the
returned result, the DECRYPTED bundle, and every table in both
partitions. (Ciphertext is never compared — a fresh salt and IV per
bundle make it nondeterministic on both sides.)
P4.D67 closed — the `ed8934f1` (Bug 56) base-path-availability drift
catch-up is complete and clears the drift debt; the oracle baseline
moves to `ed8934f1` at unification and the `d553f72a` mount-points
regen pin retires.

#### `63e773da` — 2026-08-11 — docs(running): the Docker bind-mount property for filesystem stores (P4.D67 tier 2)

_Docs-only change._

P4.D67 tier 2 — documented the Docker bind-mount property in
`docs/developer/running.md`: filesystem and Obsidian document stores are
invisible inside a container unless bound in at creation, the failure is
quiet (the folder listing comes from the cached mount index, so only
byte-touching operations notice), and the Bug-56 409 diagnosis is the
runtime symptom. Notes the same-path-both-sides requirement, the
non-root user, Docker's fabricated bind sources, and that v5 has no
equivalent of v4's bind planner (it banks with the standing
`quilltap docs` CLI deferral). Docs only.

#### `4bf6ea35` — 2026-08-11 — port(mount-index): wire the base-path check into all three consumers (P4.D67 unit 2)

_Versions: core 0.0.524._

P4.D67 unit 2 — wired the base-path-availability check into the three v4
consumers (`ed8934f1`, Bug 56). Creating a folder in a filesystem store
whose own root is unreachable now answers 409 with the diagnosis instead
of letting the recursive mkdir walk up to the topmost missing ancestor
and fabricate the entire chain; `verify_base_path` is deleted, and
store creation's warning is the diagnosis plus "The store was created,
but scanning will fail until the path is reachable." — so a store on a
real directory now creates with no warning at all, where v5 previously
warned unconditionally. Both mount differentials gained planted
reachability arms (missing / denied / not-a-directory / available) and
were regenerated at `ed8934f1`: mount-points-routes 15 → 19 cases,
mount-ops 39 → 42.

#### `543ae5e2` — 2026-08-11 — port(mount-index): the base-path-availability module (P4.D67 unit 1)

_No crate versions bumped._

P4.D67 unit 1 — ported v4 `ed8934f1`'s new
`lib/mount-index/base-path-availability.ts` as
`services/mount_index/base_path_availability.rs`: the never-failing
`check_base_path_availability` (missing / denied / not-a-directory), the
`assert_base_path_available` refusal carrying the diagnosis, and the
byte-exact operator-facing sentences including the containerized
variants (pinned by unit tests — neither test environment is a
container). No caller yet; the wire lands with the route arms.
Docs — P4.D66's order status header closed out: what landed (the whole
`db characters` family, Tier 1 and Tier 2 both), the one deferral that is not
from this family (`docs docker-mounts`), and the two places the lane
deliberately departs from the order's text (v4's launcher has its own
passphrase ladder; the archive/rehydrate arms need no ACTIVATE-AT-UNIFY gate
because one stub answers both CLIs). Docs only; no version bumps.

#### `1a547c3c` — 2026-08-11 — cli(characters): port the whole `db characters` family (v4 ed8934f1)

_Versions: cli 0.0.7._

CLI — ported the whole `quilltap db characters` family (v4 `ed8934f1`):
`status`, `archives`, `archive`, `rehydrate`, `export`. This is the first
`db` verb v5 ships at all, so it also lands v4's verb-path entrance
(resolve → instance hint → dbkey unlock → dispatch) and its error contract
(`Error: <message>`; exit 2 on an ambiguous name). `status` carries the full
vault-readiness report including the divergence pass; `archives` lists the
shelf with loose bundles; `export` decrypts an archived character's bundle
offline through core's archive crypto — the only way to reach packed-away
mail, photographs and summaries without rehydrating — and proxies live
characters to the server. `archive`/`rehydrate` proxy the same v4 URLs.
Two v4 quirks ported deliberately and pinned: `characters --json status`
runs the default sub with JSON off, and `db --json characters` reports an
unknown subcommand. The Tier R differential goes 136 → 188 cases, 0
failures, over a new 17-character fixture with eight distinct archived-export
arms; the new arms are mutation-proven and carry a permanent coverage guard.

#### `592fcbea` — 2026-08-11 — cli(drift): re-capture the db/docs help + completion templates at v4 ed8934f1

_Versions: cli 0.0.6._

CLI — absorbed v4's `ed8934f1` text drift into `quilltap-cli`. `db --help`
picked up the fourteen `characters archives|archive|rehydrate|export` lines
from the round-1 archive commit; `docs --help` and all three shell-completion
templates picked up `docs docker-mounts` from the Bug-56 commit. All five
files re-captured byte-for-byte from v4's real launcher and templates. The
`docs docker-mounts` verb itself is recognized and refuses loudly by name (its
bind planner is unported); `--format` is parsed so that flag reaches the
refusal. The Tier R CLI differential goes 136 cases / 7 failures → 0 at the
new baseline.

#### `6bdb9e9c` — 2026-08-11 — Docs: plan the character-archive round-2 + Bug-56 drift round (P4.D65 ∥ P4.D66 ∥ P4.D67)

_Docs-only change._

Planned the character-archive round 2 + the `ed8934f1` (Bug 56)
drift-catch-up round: three work orders committed under
`docs/developer/porting/work-orders/` — P4.D65 (the archive service,
verbs, re-encrypt wire, export carry, and the round-1-banked oracle
arms; closes P4.D63 on completion), P4.D66 (the CLI `db characters`
family, Tier R), and P4.D67 (the Bug-56 base-path-availability port
slice, which clears the drift debt and moves the oracle baseline to
`ed8934f1` at unification). Round record in `status-log.md` ("Round
planned — character-archive ROUND 2 + the `ed8934f1` Bug-56 drift
catch-up"). Docs only; no version bumps.

#### `02b8ce57` — 2026-08-11 — Docs: the character-archive round-1 record, order status headers, CHANGELOG, phase-4 + CLAUDE.md baseline move to d553f72a

_Docs-only change._

Unified round 1 of the character-archive drift catch-up (P4.D62 ∥ P4.D63
∥ P4.D64) onto main — the oracle baseline moves to `d553f72a`. Server:
the whole `.qtap` preserveIds substrate (vault-carrying character
exports with carried row ids, the 16-kind refuse-on-collision preflight
with the rehydrate-only skip-if-present mode, the Bug-52 avatar remap,
Bug-54 sha256 dedup, Bug-55 typed missing-content 404s), the three
archive columns (D23 re-dump + boot ensure + read tolerance), the write
guard + the `archived=` list chokepoint + every turn/tool/mail refusal
arm, the archive bundle crypto (byte-exact format, 17-arm tier-1
differential) with the engine-held runtime passphrase cache, the
wipe/restore spare-bundle options, and the two round-2 verbs defined
refusal-armed. SPA: the whole client surface — roster toggle/badges/
sort, the read-only tombstone page with both dialogs, group and seat
badges, and the four settings surfaces — with six tombstone-read e2e
beats live over a seeded archived character and four action beats gated
for round 2. The unification review fixed six findings before merge
(the headline: the one-default embedding rule had leaked into help-doc
sync, where v4 keeps the first-profile fallback), and the beats' first
live runs surfaced a v4-side bug now recorded for filing upstream (the
archived-seat sidebar badge cannot light on a fresh load in v4 — the
chat GET's enrichment never got `archivedAt`). The archive service
itself, the API actions, and the CLI subcommands are round 2; the
`ed8934f1` (Bug 56) drift catch-up is owed. Gate: 421 test binaries /
1,997 / 0; 25 oracle families fresh at the `d553f72a` pin by name; clippy
both feature sets; release build; ng test 4,138 / 0; full Playwright 198
passed / 4 gated skips / 0 failed. Versions: core 0.0.522, harness
0.0.443, web 0.0.68, host 0.0.65, SPA 0.5.450.

#### `a78e8f90` — 2026-08-10 — Widen system-data-* so the archive substrate's arms are measured (P4.D62)

_Versions: core 0.0.521, harness 0.0.442._

Widened the shared test fixture so the archive substrate's new behavior is
actually measured rather than merely present: a nested vault folder, a
document shared by two characters' vaults, a real portrait blob with the
avatar pointers that exercise the remap, and archive-category files. No
shipped behavior changed, but one real port bug surfaced with it — the export
PREVIEW was excluding archive bundles where v4 still lists them.

#### `89d86972` — 2026-08-10 — Answer 404 when a file row outlives its bytes (P4.D62, v4 bug 55)

_Versions: core 0.0.520, host 0.0.64, web 0.0.67._

Serving a file whose bytes have gone missing now answers 404 instead of 500
(v4 bug 55). A `files` row can outlive its content — a deleted mount point, a
storage key with nothing at that path — and answering a server error on every
render invited retries that could never work while burying genuine storage
faults. Both file routes now tell the two apart; every other failure still
500s, unchanged.

#### `677f1ea5` — 2026-08-10 — feat(archive): the character-archive schema, guards, chokepoint and crypto (P4.D63 units 1-6, 8-11)

_Versions: core 0.0.522, harness 0.0.443, host 0.0.65, web 0.0.68._

Ported the `.qtap` export/import substrate the character archive is built
on (v4 `01e481f6` + `d553f72a`, work order P4.D62). A `characters` export now
carries each character's whole vault — mount point, folders, documents and
blobs — so a cross-instance import no longer lands a faceless, mail-less
character; export records carry their source row ids; character-archive
bundles and anything under `/archives` are excluded from exports alongside
backups; and the export preview reports the vault weight a bundle will add.
The import side gained the `preserveIds` path: a bundle can be restored at
its original ids, refusing the whole import on any collision, or — for
rehydration only — skipping ids that already exist inside the target
character's own vault. Content rows and blobs settle that by content hash
first, so a conversation summary shared with a group chat no longer blocks a
rehydrate (v4 bug 54). Imported characters are repointed at the vault their
bundle carried and the placeholder vault is discarded, and avatar pointers
are remapped through it — a dangling default image is cleared and an
unresolvable per-chat override is dropped, each with a warning (v4 bug 52).
Imported memories are now embedded under the default profile whatever its
provider, instead of being left unembedded under the built-in one. Folder
parents on an ordinary import resolve by path rather than keeping a source
id that never existed here.
Landed P4.D63 units 1-6 (the character-archive schema, guards, chokepoint and
crypto) against v4 `d553f72a`. Adopted the three new `characters` columns via a
D23 re-dump of v4's live generateDDL — `archivedAt`, `archiveFileId`,
`archivedAvatarFileId` — with a boot repair pass that adds them to any existing
instance and pragma-guarded read tolerance so a pre-drift database still opens.
An archived character is now a tombstone: the repository refuses every write to
one except the single-key unarchive, the wardrobe write path refuses rather than
silently falling back to the legacy table, and the read overlay deliberately does
NOT short-circuit (an archived character hydrates exactly like a live one, since
archiving prunes the vault in place). Added the `archived=` list filter as the
single chokepoint — every picker and roster excludes tombstones by default, with
`include`/`only` to opt in — plus `archivedAt` on list items, chat participants
and group members, refusals for adding an archived character to a group or
roster, and a refusal for exporting one. The turn selection drops an archived
seat even if its status somehow stayed active, and the participant resolver,
Carina probe, character resolver, self-inventory, both mail tools, both mail chat
actions and the doc-edit self-vault resolver all refuse or skip archived
characters. Conversation summaries are no longer written into an archived
character's vault. Ported the archive bundle crypto byte-for-byte (PBKDF2-SHA256
at 600k iterations into AES-256-GCM, under the instance passphrase and never the
database pepper, so a bundle stays readable after a restore onto a new instance)
with its four typed errors, and added the runtime passphrase cache the engine had
never had, deposited at all four `.dbkey` chokepoints and cleared on lock.

Also landed the rest of P4.D63's server surface. Deleting all data and
restoring in replace mode now spare archived-character bundles by default —
the character rows go, so what survives is a loose importable bundle, and only
an explicit `keepArchivedCharacterBundles: false` wipes them too; the delete
summary reports how many were on hand and whether they were kept. Defined the
`characterArchive` and `characterRehydrate` verbs, which refuse loudly by name
until the archive service lands in round 2, and added an exact-match
`category` filter to the files list. Tightened the embedding rule at all five
sites that pick a profile: only a profile actually marked default counts, with
no fallback to an arbitrary one, so chunks wait rather than embedding into a
different vector space than everything else. **Still owed under this order:**
the passphrase-change re-encryption sweep is ported but not yet wired to the
change-passphrase response, so a passphrase change does not yet rewrite
archive bundles — see the work order's resume list.
P4.D64 unit 6: end-to-end beats for the archive, plus the fixture that feeds
them. Global setup seeds a tombstoned character (Marchpane) with a group and
a conversation that hold it, so the read surfaces — the roster toggle,
badges, the read-only page, the group's can-speak line, the archived seat —
have something real to walk. The seeder probes for the archive column and
writes nothing without it, keeping it inert until the schema lands. Six read
beats activate at this round's unification; four action beats stay gated for
round 2.

#### `eff6ce32` — 2026-08-10 — feat(archive): the settings surfaces account for archive bundles (P4.D64 unit 5)

_Versions: SPA 0.5.449._

P4.D64 unit 5: the settings surfaces account for archive bundles. The
Encryption Passphrase card counts the bundles sealed under the current
passphrase and warns that each will be rewritten, then reports what the
change actually did — all rewritten, or the ones left behind named with
their reasons. Delete All Data reports the bundles on hand and offers to
leave them on the shelf (ticked by default), with a note on the completion
screen when any survive. The export wizard, on reaching its options step for
a characters export, asks what the vaults add to the trunk and says so —
advisory only, so a failed read simply says nothing. A replace-mode restore
can spare the bundles from the wipe that precedes it.

#### `f95f8436` — 2026-08-10 — feat(archive): archived members and seats say so (P4.D64 unit 4)

_Versions: SPA 0.5.448._

P4.D64 unit 4: archived characters are visible where they still belong. A
group whose roster includes an archived member now reads "3 members / 2 can
speak (1 archived)" and badges that member's row; an ordinary group's
subtitle is unchanged. In a conversation's cast, an archived seat carries an
"Archived" badge alongside "Absent" — both can show, since an archived
character is normally absent too. Every character picker was verified to
inherit the server's exclude-by-default, with a spec as the tripwire.

#### `cc3933e9` — 2026-08-10 — feat(archive): the archived character's page, read-only, with both dialogs (P4.D64 unit 3)

_Versions: SPA 0.5.447._

P4.D64 unit 3: an archived character's page is readable but inert. A banner
above the header explains what was packed away and what was kept; every tab
still renders, inside a disabled fieldset, and the Edit Character door hides
itself (a disabled fieldset cannot inert a link). The header forks: an
archived character offers one Rehydrate button in place of the live action
cluster, and a live one gains Archive at the end of it. Two new dialogs —
the archive confirmation, which itemizes what goes and what stays, and the
post-rehydrate bundle disposal, whose destructive arm is deliberately the
secondary button. All six toasts carry v4's sentences. The archive and
rehydrate actions answer P4.D63's not-yet-available refusal until round 2.

#### `694c5586` — 2026-08-10 — feat(archive): the roster shows and sorts the archive (P4.D64 unit 2)

_Versions: SPA 0.5.446._

P4.D64 unit 2: the character roster shows the archive. A "Show Archived"
toggle leads the toolbar (folder icon, v4 labels and tooltips both ways);
its two states fetch and cache separately, and every character mutation now
refreshes both. Archived characters sort to the very end of the shelf ahead
of every other ordering rule, wear an "Archived" badge dated to when they
were put away, lose the favorite / Carina / control toggles, and trade Chat
and both export actions for one inert "Resting in the archive" note —
Delete stays. Sort rule 0 and the distinct-cache-key refetch were
mutation-proven.

#### `1bf6159c` — 2026-08-10 — feat(archive): mirror the character-archive client contract + data layer (P4.D64 unit 1)

_Versions: SPA 0.5.445._

P4.D64 unit 1: mirrored the character-archive client contract (v4
`d553f72a`) and added the archive data layer. `core-contract.ts` gains the
`archived` list filter, `archivedAt` on the character list/detail DTOs plus
`archiveFileId`, `archivedAt` on group members and chat-participant
characters, `keepArchivedCharacterBundles` on delete-data and
restore-execute, the files-list `category` filter, the two new
`characterArchive`/`characterRehydrate` verbs, and the
`ArchiveReencryptSummary` / `ExportVaultPreview` shapes; the stale "v4 has
no export-preview route" comment is reconciled. `characters.api.ts` gains
`archiveCharacter`, `rehydrateCharacter`, `deleteArchiveBundle`,
`countArchiveBundles`, the archived list filter, and distinct cache keys per
archived-filter state. The two verbs answer P4.D63's not-yet-available
refusal this round; round 2 fills them.

#### `e85adb7c` — 2026-08-10 — Docs: plan the character-archive drift catch-up, round 1 of 2 (P4.D62 ∥ P4.D63 ∥ P4.D64)

_Docs-only change._

Planned the character-archive drift catch-up (v4 `f6eac168` →
`d553f72a`): round 1 of 2, three work orders committed —
`p4.d62-export-import-archive-substrate.md` (export fidelity, preserveIds,
Bugs 52/54/55), `p4.d63-archive-schema-guards-crypto.md` (the D23
three-column re-dump, write guards, the `archived=` chokepoint, archive
crypto + re-encrypt, wipe/restore options), and `p4.d64-archive-spa.md`
(the whole client surface, action beats gated). The archive service, API
actions, and CLI are round 2 (scope recorded in the status log). Docs
only; no version bumps.

#### `2d38535b` — 2026-08-08 — Docs: the P4.D60∥P4.D61∥P4.44 round record, order status headers, CHANGELOG entry, phase-4 + CLAUDE.md baseline move to f6eac168

_Docs-only change._

Unified the `f6eac168` drift catch-up round (P4.D60 ∥ P4.D61 ∥ P4.44) onto
main — all three orders closed; the oracle baseline moves to `f6eac168`
(v4 Bugs 47-51, filed from this port's own dogfood walk). Server: the
fair-rotation first-responder pause (a fresh user send now pauses for
another user-driven seat instead of forcing the sole LLM to answer every
human turn), the Brahma budget-exhaustion salvage in both paths (a run
that spends its budget now always answers and signals completion), the
chat GET projects `impersonatingParticipantIds`/`activeTypingParticipantId`
(a reload restores impersonation state), and the five-copy
impersonating-ids reader consolidation. SPA: impersonating hands the seat
the current turn (client turn override, cleared on send), the speaking-as
follows the current user-driven turn (latch-keyed), and the persisted
speaking-as seeds once instead of clobbering live state. P4.44: the
conversation-chunks upsert create arm pinned, eager per-delete/overwrite
thumbnail cleanup over StorageBackend (bug 43 tier 2 closed), and the
provider request-header pin (corpus regenerated byte-identical, 8-provider
coverage floor). The unification review fixed two spec defects before
merge: a false-green seed-once parity spec (TanStack structural sharing
kept the stub reference; now mutation-proven both directions) and the
reload beat's gesture + over-claiming assertion (the turn-follow
legitimately supersedes the seed; the beat now pins the deterministic
payoffs). Gate: 419 test binaries / 1,978 / 0; the round's seven
differentials by name zero SKIP over fresh `f6eac168` oracles; clippy both
feature sets; release build; ng test 296 files / 4,065; ng build clean;
full Playwright 192/192 zero skips. Versions: core 0.0.518, harness
0.0.440, SPA 0.5.444.

#### `059fb611` — 2026-08-08 — refactor(chat): unify the five impersonating-ids readers (P4.D60 rider)

_Versions: core 0.0.517._

Refactor: consolidated the five duplicate `impersonatingParticipantIds` JSON
extractions to one shared `db::chats_impersonation::read_impersonating` reader
(the P4.D56 §3 style note). Pure de-duplication, no behavior change.

#### `386568b9` — 2026-08-08 — port(salon): chat GET projects impersonation overlay (bug 51 server, unit 4)

_Versions: core 0.0.516, harness 0.0.437._

Fixed (v4 Bug 51, server half): the chat GET now projects
`impersonatingParticipantIds` and `activeTypingParticipantId`, so a reload (or a
mid-session server restart) restores the impersonation overlay instead of
snapping every seat back to LLM-controlled. Both keys are always present (`[]` /
`null` when unset), matching the mutation replies.

#### `b2b77330` — 2026-08-08 — port(brahma): budget-exhaustion salvage in both paths (bug 47, server unit 3)

_Versions: core 0.0.515, harness 0.0.436._

Fixed (v4 Bug 47): the Brahma Console no longer hangs silently when its turn
budget runs out. The forced final turn runs no tools, so a model that answered
it with another tool call left an empty response — saving no message and
signaling no completion. Both Brahma paths now salvage an explanatory answer
folding in the last tool result: the streaming orchestrator always finalizes
(message + done event) even with no tool data; the one-shot @Brahma path
salvages when tool data exists and otherwise falls through to its existing empty
-response failure.

#### `98ce0eb1` — 2026-08-08 — port(salon): fair-rotation pause guard in the chat spine (bug 50, server unit 2)

_Versions: core 0.0.514._

Fixed (v4 Bug 50): in a room where the human drives two or more seats alongside
a single LLM, the LLM answered every human turn. The chat spine now runs a
fair-rotation pause guard before resolving a responder — when the rotation's
next speaker after a human post is another seat the human drives, it persists
the message and pauses for that seat (emitting the existing `user_turn`
chain-complete frame) instead of forcing the sole LLM to speak out of turn.
Single-user-seat rooms and whisper/nudge/continue turns are untouched.

#### `5ae85165` — 2026-08-08 — test(salon): impersonation take-the-turn + reload e2e beats, and AllLLMPause opener coverage (P4.D61)

_Versions: SPA 0.5.444._

Added `select_next_speaker_after_user_message` (v4 Bug 50 fair rotation): the
pure helper that projects the turn rotation one step past a user's just-typed,
unpersisted message, so a multi-seat room's first responder honors the full
roster instead of an LLM-only shortlist. Tier-1 differential extended
(`select-speaker` gains eight `select-after` cases against v4's real helper).
Tests (P4.D61): extended the impersonation e2e walk to assert Bug 48's
take-the-turn (after Speak-as the user-turn banner names the impersonated seat),
and added a reload-restores-impersonation beat gated ACTIVATE-AT-UNIFY behind
`P4D60_CHAT_GET_PROJECTION_LANDED` (it needs the sibling server lane's chat-GET
projection). The P4.D54 AllLLMPause live-opener e2e beat stays deferred (the
committed fixture has no all-LLM chat and the pause threshold needs real LLM
turns) — its opener → take-over → Bug-48 handoff is instead covered
deterministically at the unit level. SPA 0.5.444.

#### `4f2ffa61` — 2026-08-08 — fix(salon): the composer speaking-as follows the current user-driven turn (v4 Bug 49)

_Versions: SPA 0.5.443._

Fixed (v4 Bug 49; P4.D61): the composer's speaking-as now follows the current
user-driven turn — when the rotation lands on a seat the human drives (their own
character or one they are impersonating) and that seat changes, the speaking-as
defaults to it, so on the impersonated character's own turn you speak as them
without a manual switch. It is a latch keyed on the turn seat: a deliberate
same-turn Speaker choice still sticks, and a non-user/absent next speaker clears
it. Client-only (forwarded per send as `speakingAsParticipantId`; no per-turn
persistence). SPA 0.5.443.

#### `dfc5a3b5` — 2026-08-08 — fix(salon): impersonating hands the character the current turn (v4 Bug 48)

_Versions: SPA 0.5.442._

Fixed (v4 Bug 48; P4.D61): impersonating a character now hands them the current
turn — unless an LLM is mid-generation — so the composer's user-turn banner
names them and a typed message lands in turn. v5's turn is server-authoritative
and auto-refreshed, so this is a client turn override that layers above the
server-queried turn and is cleared when a message is sent (matching v4, which
recomputes the turn from history once a message is sent). Both impersonate entry
points (the sidebar and the AllLLMPause take-over) inherit it. SPA 0.5.442.

#### `d3a73428` — 2026-08-08 — test(harness): pin the provider request headers (P4.44 item 3)

_Versions: harness 0.0.440._

Fixed (v4 Bug 51, client half; P4.D61): the impersonation overlay
(`impersonatingParticipantIds` / `activeTypingParticipantId`) is now held as a
client-local mirror that the chat record only SEEDS, never overrides live. The
impersonating list re-seeds from the record only when non-empty (transitions,
including → empty, are owned by the mutation replies), and the persisted
speaking-as is seeded once while still unset — so once the sibling server lane
(P4.D60) projects those fields on the chat GET, a refetch no longer resurrects a
just-stopped impersonation nor snaps the composer back to the stale persisted
seat after each turn. The sidebar's active-typing indicator reads the same local
source (v4 feeds `impersonation.activeTypingParticipantId`, not the record). SPA
0.5.441.
P4.44 item 3 — pinned the provider request HEADERS (the P4.D55 deferral: the
vision/transport path's headers were unpinned by any family). The request-
envelope recorder now captures the outbound headers, and
`request_builder_equivalence` compares them at the post-`apply_auth` point (v5's
real `transport_headers` + `apply_auth` via `execute_completion`), a subset check
over the headers v5 models (User-Agent, HTTP-Referer/X-Title, content-type, auth,
anthropic-version), normalizing the version-bearing User-Agent and the auth
secret. The corpus widening is additive (every pre-existing method/url/body
byte-identical). One documented OpenRouter-only divergence: v4's `@openrouter/sdk`
send path overrides the User-Agent and omits X-Title, where v5's single reqwest
transport keeps both. The abort/timeout-arming half stays a loud deferral (it is
wall-clock, unobservable in the fetch-args corpus, and proven unit-tier in
`model::transport`). quilltap-harness 0.0.437.

#### `bf6d1f2f` — 2026-08-08 — feat(files): eager per-delete/overwrite thumbnail cleanup (P4.44 item 2)

_Versions: core 0.0.518, harness 0.0.439._

P4.44 item 2 — eager per-delete/overwrite thumbnail cleanup (closes the bug 43
tier-2 deferral). Deleting or overwriting a library file now eagerly removes its
cached thumbnails (`_thumbnails/{fileId}_{120,150,300}.webp`) over the host's
disk `StorageBackend`, matching v4's `cleanupThumbnails` — best-effort,
DB-invisible, never failing the delete/upload, and gated on
`canGenerateThumbnail(mimeType)`. `file_delete`/`file_upload` take an optional
backend, wired at the engine from `qtap_file_storage()`; a diskless host has no
thumbnails to reap. The daily orphan-thumbnail sweep stays the reaper for strays
that leave by any other route. The chat-file routes run no cleanup in v4, so
their twins stay unwired. quilltap-core 0.0.513, quilltap-harness 0.0.436 (the
`files_routes_equivalence` call sites pass the new optional backend).

#### `b4a05f40` — 2026-08-08 — test(harness): pin the conversation-chunks upsert CREATE arm (P4.44 item 1)

_Versions: harness 0.0.438._

P4.44 item 1 — pinned the conversation-chunks `upsert` CREATE arm. The tier-2
family only ever exercised the update arm (every corpus upsert hit an existing
`(chatId, interchangeIndex)` row); two new upserts on unseeded pairs now take
the create arm (with and without a supplied embedding). The harness placeholders
the create arm's minted id/createdAt/updatedAt and re-sorts the dump by
`(chatId, interchangeIndex)` so the comparison is independent of v4's random
minted id. No production behavior change (test + fixture only). quilltap-harness
0.0.435.

#### `1356dd06` — 2026-08-08 — Docs: plan the f6eac168 drift catch-up round — work orders P4.D60 ∥ P4.D61 ∥ P4.44

_Docs-only change._

Planned the `f6eac168` drift catch-up round (v4 Bugs 47-51) and committed
three work orders: P4.D60 (server — the fair-rotation first-responder pause,
the Brahma budget-exhaustion salvage, and the chat-GET impersonation
projection, plus the `impersonating_ids` consolidation rider), P4.D61 (SPA —
impersonate-takes-the-turn, the speaking-as turn-follow, seed-once
reconciliation, plus the AllLLMPause live-opener beat rider), and P4.44
(three pinning follow-ups: the conversation-chunks upsert create arm,
per-delete thumbnail cleanup over StorageBackend, and the vision-path
headers pin). v4 `f521fc0c` classified docs-only NO-PORT. Docs only; no
version bumps.

#### `20881bc5` — 2026-08-08 — fix(salon): let the human speak as their own character while impersonating (dogfood #77)

_Versions: SPA 0.5.440._

Fixed (dogfood #77): while impersonating a character, the human could not switch
back to speaking as their own character — the Speaking-As selector stayed hidden
and every typed message went to the impersonated seat. v5's `controlledCharacters`
listed only genuine user seats, where v4 includes impersonated seats too, so the
selector never reached its two-seat threshold. It now includes impersonated seats
(matching v4), and `onSelectSpeaker` applies the server reply to the local
speaking-as/impersonation mirrors so a pick persists (the chat GET projects
neither field). SPA 0.5.440.

#### `442bfed5` — 2026-08-08 — fix(salon): apply activeTypingParticipantId from the impersonate reply (dogfood #76)

_Versions: SPA 0.5.439._

Fixed (dogfood #76): while impersonating a character, the composer speaking-as
portrait kept showing your own character (not the impersonated one) and a
just-sent message was optimistically attributed to your character before the
server corrected it to the impersonated seat. The chat GET projects no
`activeTypingParticipantId` (v4-faithful), and v5 had no local mirror for it —
so the speaking-as resolution fell back to the owner seat while impersonating.
Added the `activeTypingLocal` mirror, applied from the impersonate/stop replies
like v4, and folded it into the active-speaker resolution. SPA 0.5.439.

#### `937ea82f` — 2026-08-08 — fix(salon): give the composer editor room; wrap the toolbar below (dogfood #75)

_Versions: SPA 0.5.438._

Fixed (dogfood #75, interim): the Salon composer editor collapsed to its width
floor and the "Type a message…" placeholder clipped to "Type a", because v5's
one-row gutter cluster (a p4.9l shortcut) plus the speaking-as avatar crammed
into the composer's max-w-4xl cap. As a band-aid pending the p4.9l composer-
toolbar port, the action cluster now takes a full-width row and wraps below the
editor+avatar line, so the editor keeps the dominant width. The proper 2-column
layout is routed to p4.9l. SPA 0.5.438.

#### `0b373829` — 2026-08-08 — fix(brahma): make the Console header sticky (dogfood #74)

_Versions: SPA 0.5.437._

Fixed (dogfood #74): the Brahma Console header (model picker + New conversation)
scrolled off the top of the workspace tab with the transcript instead of staying
put. The message-list component host defaulted to `display: block` with no flex
sizing — in v4's React DOM the `flex-1 overflow-y-auto` root is the node itself,
but Angular wraps it in the host element, so the inner scroll container had no
bounded parent and the whole tab scrolled. Gave the host `flex flex-col flex-1
min-h-0` and the inner scroller `min-h-0`; the transcript now scrolls within the
list and the header is sticky. SPA 0.5.437.

#### `ac5f457d` — 2026-08-08 — Docs: the P4.D57∥D58∥D59 round record, order status headers, CHANGELOG entry, phase-4 + CLAUDE.md baseline move to 1bed814f

_Docs-only change._

Unified the `1bed814f` drift catch-up round (P4.D57 ∥ P4.D58 ∥ P4.D59) onto
main — all three orders closed; the oracle baseline moves to `1bed814f` and
the drift debt is cleared. The Brahma Console turn budget is live end-to-end
(server accessors/resolver/verbs/REST edge + the Settings → Chat card), the
salon impersonation reconcile closes dogfood findings #71/#72 (banner gate,
optimistic-bubble attribution, the speaking-as composer portrait), and the
About-backdrop change is a recorded no-port. Unification wires: the two
brahma-console request variants folded into the SPA contract name-for-name
and the gated settings e2e beat activated. Gate: fmt/clippy both feature
sets/release build clean; full workspace tests with the round's oracles
regenerated fresh at `1bed814f` (the three families re-run by name, zero
SKIP); ng test 296 files / 4,046; ng build clean; full Playwright green zero
skips. Final versions: core 0.0.512, harness 0.0.434, web 0.0.66, SPA
0.5.436.

#### `9e344d20` — 2026-08-07 — P4.D57 unit 6: regenerate the two Brahma tier-3 differentials at the new cap

_Versions: harness 0.0.434._

P4.D57 (Brahma Console turn budget, server): regenerated the two Brahma tier-3
differentials at the new baseline — the agent-mode system prompt now records the
default-50 turn cap (was 25), and both the one-shot and orchestrator ports
reproduce it byte-for-byte. harness 0.0.434.

#### `5403bdad` — 2026-08-07 — P4.D57 unit 5: settings-routes differential for the brahma-console budget

_Versions: harness 0.0.433._

P4.D57 (Brahma Console turn budget, server): extended the settings-routes
differential with 12 brahma-console GET/PUT cases (default, seeded, valid,
boundaries, empty-merge, and the four 400 arms + null + non-object body), driven
against the reference route and green over an oracle regenerated at `1bed814f`.
harness 0.0.433.

#### `59c6d890` — 2026-08-07 — P4.D57 unit 4: REST edge GET/PUT /api/v1/settings/brahma-console

_Versions: web 0.0.66._

P4.D57 (Brahma Console turn budget, server): added the REST edge
`GET / PUT /api/v1/settings/brahma-console` (byte-faithful to the reference
route: GET returns `{maxAgentTurns}`, PUT merges over current, validates,
persists, echoes; a malformed body 500s, an invalid value 400s). web 0.0.66.

#### `5cde24b4` — 2026-08-07 — P4.D57 unit 3: brahma-console settings dispatch verbs + handler

_Versions: core 0.0.512._

P4.D57 (Brahma Console turn budget, server): added the dispatch surface the
settings card consumes — `brahmaConsoleSettings` (GET, `{maxAgentTurns}`) and
`brahmaConsoleSettingsUpdate` (PUT, merge-over-current, validate, echo the
stored value; 400 on out-of-range/non-integer/null). core 0.0.512.

#### `04cc1e18` — 2026-08-07 — P4.D57 unit 2: both Brahma paths read the operator-set turn budget

_Versions: core 0.0.511._

P4.D57 (Brahma Console turn budget, server): both Brahma paths — the streaming
orchestrator and the one-shot `@Brahma` — now read the operator-set agent-turn
budget through a shared resolver (`resolve_brahma_max_agent_turns`) instead of a
hardcoded 25. A deep ledger investigation can now run to the configured budget
(default 50) before it is forced to answer; the duplicate/stale-query guard is
unchanged and still short-circuits a stuck loop. core 0.0.511.

#### `30365398` — 2026-08-07 — P4.D57 unit 1: brahmaConsole instance-setting accessors

_Versions: core 0.0.510._

P4.D57 (Brahma Console turn budget, server): added the
`instance_settings['brahmaConsole']` accessors to the Rust core — the
agent-turn budget (default 50, bounds 5–200) both Brahma paths read. Read
falls back to the default on a missing/unparseable/out-of-range value; write
validates and refuses out-of-range without storing. Portable by default
(rides `.qtap` export and full backups). core 0.0.510.

#### `9c0d4638` — 2026-08-07 — P4.D58 unit 5: assert the speaking-as portrait in the impersonation e2e beat

_No crate versions bumped._

Extended the impersonation end-to-end test to check that the "speaking as"
composer portrait appears and names the character in play. (Test-only.)

#### `d5e3e0e1` — 2026-08-07 — P4.D58 unit 4: the SpeakingAsAvatar composer cue (bug 46b)

_Versions: SPA 0.5.436._

Added a "speaking as" portrait to the Salon composer: a persistent full-height
picture of the character your typed message will be attributed to, seated just
left of the send controls. It brightens when the floor is yours and dims while a
reply is streaming, and it hides on the narrowest screens. Resolved the same way
the server attributes a message, so it always names whom you're actually speaking
as — including a character you're impersonating. SPA 0.5.436.

#### `32fb9e4f` — 2026-08-07 — P4.D58 units 2-3: reconcile Salon impersonation attribution to the overlay

_Versions: SPA 0.5.435._

Reconciled the Salon's impersonation attribution to the reference app's updated
client. The composer turn banner now announces an impersonated seat's own turn
(offering "type as them" and Skip), where before it stayed silent because the
seat keeps its LLM control column under the impersonation overlay. And a
just-sent message's optimistic bubble is now attributed to the same seat the
server will resolve it onto, so it no longer flickers to the wrong author on
refetch. SPA 0.5.435.

#### `d4792c50` — 2026-08-07 — P4.D58 unit 1: client impersonation-overlay participant filters (turn-order.ts)

_Versions: SPA 0.5.434._

Added client-side impersonation-overlay participant filters (`isUserDrivenSeat`,
`findUserParticipant`, `findActiveUserParticipant`) to the chat turn-order
module — the browser mirror of the reference app's turn-manager utilities and
the differential-proven engine filters. These resolve who the human is driving a
seat as, honoring the impersonation overlay (an impersonated seat keeps its LLM
control column). Groundwork for the composer turn banner and message-attribution
reconciliation. SPA 0.5.434.

#### `8d4f30a5` — 2026-08-07 — feat(settings): add the Brahma Console budget card to Settings → Chat (P4.D59)

_No crate versions bumped._

Added the Brahma Console budget card to Settings → Chat (between Data
Retention and Autonomous Rooms). One number field caps how many tool-use
turns the Console — and every one-shot @Brahma consultation — may take
before it must answer, adjustable 5–200 (default 50). It commits on blur
or Enter, reverts an out-of-range entry without nagging, and skips the
round-trip when unchanged. This is the SPA half; the setting itself is
served by the sibling server change. The About workspace-backdrop
reference-app change is a deliberate no-port: this app ships no About
background image, so the bug it fixes cannot occur here.

#### `1b5ab7ef` — 2026-08-07 — Write P4.D57/D58/D59 work orders: the 1bed814f drift catch-up round

_Docs-only change._

Planned the next porting round: a drift catch-up on three reference-app
changes (work orders P4.D57/P4.D58/P4.D59). It ports the Brahma Console
turn-budget instance setting, reconciles the impersonated-seat message
attribution and turn banner (with a new "speaking as" composer portrait),
and dispositions the About workspace-backdrop change. Work orders only;
no behavior change yet.

#### `3435055c` — 2026-08-07 — Revert dogfood #70: v5's turn banner matches v4's (keys on the bare controlledBy)

_Versions: SPA 0.5.433._

Reverted the previous change to the impersonated-seat turn banner. The
reference app does not announce an impersonated seat's turn in the
composer banner either (it keys on the seat's control column, which the
impersonation overlay leaves unchanged), so lighting the banner up for
impersonated seats was a divergence. The turn banner is back to matching
the reference behavior. The underlying rough edge — that while
impersonating, the banner and the message attribution can disagree with
no on-screen cue — is a shared reference-app issue and will be addressed
there first. SPA 0.5.433.

#### `0a646da8` — 2026-08-07 — Dogfood #70: surface the type-as-them/Skip banner on an impersonated seat's turn

_Versions: SPA 0.5.432._

Dogfood fix: an impersonated character's paused turn now shows the
"type as them, or skip" prompt and its Skip button. Since impersonation
became a pure overlay, the seat keeps its LLM control column while the
server correctly reports the turn as the user's; the Salon was still
deciding whose turn it was from the bare column, so an impersonated
seat's turn surfaced neither the prompt nor the Skip control. The turn
banner now consults the impersonation overlay the same way the engine
does. SPA 0.5.432.

#### `b2885484` — 2026-08-07 — Close the P4.D56 Bug 44 overlay round: round record, CHANGELOG, order header CLOSED, phase-4 candidates, CLAUDE.md baseline move to 62c63dc3

_Docs-only change._

Unified the Bug 44 impersonation-overlay round (single lane): the
reference-app baseline moves to 4.8.0-dev.178 and the drift debt is
cleared. Impersonation is a pure overlay end to end — starting or
stopping never writes the seat's control column or recompiles identity
stacks, the turn-resolution gates consult the overlay, and the
participant card's Stop-Impersonate button is back in solo-shaped
casts, proven by the re-gestured end-to-end walk driving Stop through
the card itself. Gate: 419 test binaries with 1,956 tests and no
failures, all twenty-four affected differential families regenerated
fresh against the reference app and green, the Angular suite at 4,015
tests, and the full end-to-end suite at 189 passing with zero skips.
Versions: core 0.0.509, harness 0.0.432, SPA 0.5.431.

#### `c158d2e3` — 2026-08-07 — Re-gesture the impersonation e2e beat to the Bug 44 overlay

_Versions: SPA 0.5.431._

Re-gestured the salon impersonation walk to the Bug 44 overlay: it now
checks that the seat's control column does not flip, that the
Stop-Impersonate button stays on the participant card, and that stopping
through that button returns the Speak-as affordance. No client component
changed — the healed Stop button falls out of the server no longer
flipping the column.

#### `3ae4708d` — 2026-08-07 — Port Bug 44: impersonation overlays the seat instead of mutating controlledBy

_Versions: core 0.0.509, harness 0.0.432._

Impersonation is now a pure overlay instead of mutating a participant's
control column (Bug 44, ported from the reference app). Starting to
speak as a character no longer writes controlledBy to user or recompiles
identity stacks, and stopping no longer writes it back to llm; recording
the seat in impersonatingParticipantIds is the whole change. A shared
is_user_driven_seat helper is consulted at the turn-resolution gates
(attribution and who-responds); owner-seat readers keep reading the
column, which restores the Stop-Impersonate button on the participant
card with no client change. The stop flow's new-profile arm is now a
plain connection-profile reassignment. Twelve differential families were
regenerated against the reference app and re-run; the neutrality set is
unchanged.

#### `25348fe3` — 2026-08-07 — Plan the Bug 44 impersonation-overlay drift round: the P4.D56 order (single lane), the round-planned record, the drift classified (62c63dc3 = the round; cc0bbebf/3fa36825 NO-PORT)

_Docs-only change._

Planned the Bug 44 impersonation-overlay drift catch-up (work order
P4.D56, single lane): the reference app has now implemented the ruled
correction — impersonation becomes a pure overlay and the seat's
controlledBy column is never written — so this port absorbs it as the
pre-announced drift round. The order carries the site-by-site survey
(the change-list and, just as binding, the keep-list of owner-seat
readers that stay on the column), the twelve moving differential
families plus the neutrality set, and the e2e re-gesture that returns
the Stop-Impersonate button to the participant card.

#### `94fdc646` — 2026-08-06 — Point the #39 records at v4 Bug 44 (catalogued upstream, v4 3fa36825, docs-only; baseline unchanged)

_Docs-only change._

The #39 mechanism correction is now catalogued upstream as v4 Bug 44
(reference-app docs commit `3fa36825`): the full account of what is
wrong with the mutate-and-restore impersonation mechanism and the
ruled overlay fix, specced at the exact sites. This app's records
point at it; the oracle baseline is unchanged (docs-only upstream).

#### `b2ca2781` — 2026-08-06 — Record the #39 impersonation-mechanism ruling: the overlay design stands; v4's bug-27 mutate-and-restore is ruled a mistake, correction queued v4-first

_Docs-only change._

The impersonation-mechanism ruling (finding #39) is recorded: the
overlay design stands, and the reference app's recent impersonation
fix — which rewrites who controls a character's seat and restores it
afterward — is ruled a mistake to be corrected there first, since it
is the oracle. Until that correction lands, this app faithfully
matches the shipped behavior; the ruling, its reasoning, and the two
gate sites the correction touches are recorded in the porting docs.

#### `47a2851a` — 2026-08-06 — Close the f4955e0e convergence round: round record, CHANGELOG, CLAUDE.md baseline move, the stop-affordance v4-side note

_Docs-only change._

Unified the `f4955e0e` found-bugs convergence round — six lanes, all
closed, the oracle baseline moved to v4 4.8.0-dev.175. The reference
app's coordinated bug sweep (every catalogued v4 bug 1-43 is now fixed
there, much of it adopting fixes this port made first) is absorbed
whole: roughly twenty-five deliberate-divergence pins across seven
differential families retired into plain equalities, and the four
genuinely new pieces landed. Long conversation interchanges now
sub-chunk at a 24,000-character budget so they can actually embed and
be searched (with the boot pass re-rendering previously stranded
chats once); the all-LLM pause finally has its dialog, opening
automatically with take-over buttons for the cast; OpenRouter sends
images (a dedicated non-streaming vision path, plus the capability map
now saying so); Grok accepts text attachments; almost-base64 text
attachments arrive as their actual bytes; Ollama streaming no longer
drops content split across network reads; the chat record projects
five more fields so the sidebar's controlled selects survive a reload;
staff-signed announcements reach the model with their author's name;
impersonation genuinely hands the seat over (and back); tool cards
initiated by the operator wear the operator's face; self-targeted
whispers read "whispered to you"; deleted chats sweep their
annotations; orphaned thumbnails get a daily reaper; corrupt
character-vault edits are refused in both apps now; and Memory
Deduplication + Regenerate Conversation Summaries went live in
Settings, closing the embedding-profiles order whole. Gate: 419 Rust
test binaries / 1,951 tests, every round differential fresh at the
new baseline with zero skips, ng 294 files / 4,015, full Playwright
189/189 zero skips. Versions: core 0.0.508, harness 0.0.431, host
0.0.63, web 0.0.65, SPA 0.5.430.

#### `01097dab` — 2026-08-06 — P4.D51: stale-comment sweep + bug-43 tier-2 loud deferral

_Versions: core 0.0.494._

P4.D51: stale "queued v4-side" / "deliberate divergence" prose in the
store-delete, import, delete-all, and character-vault comments is corrected
now that the reference app has converged (bugs 8/9/10/11). The per-delete
eager thumbnail cleanup (bug 43 tier 2) is deferred with a loud note — the
landed daily orphan-thumbnail sweep covers those strays until a storage
backend is threaded through the file delete/upload paths.

#### `4bb2703b` — 2026-08-06 — P4.D51 bug 38: attach native-text documents instead of 404ing

_Versions: core 0.0.493, harness 0.0.420._

P4.D51 (bug 38): a native-text document (a `.md`/`.txt`/`.json` written into a
document store, held with no image blob) can now be attached to a chat and
reaches the model as text, instead of failing with "Mount-point file blob not
found". The attach path, the file list, and the send-time attachment loader
all fall back to the document row for these files, serving their text with the
`/files/` route.

#### `817fe88a` — 2026-08-06 — P4.D51 bug 43: orphaned-thumbnail sweep + the maintenance-summary reshape

_Versions: core 0.0.492, harness 0.0.419, host 0.0.62._

P4.D51 (bug 43): the daily maintenance pass now sweeps orphaned thumbnails —
`_thumbnails/{fileId}_{size}.webp` cache entries whose source file left by a
route that never cleaned them up (a restore, a delete-all, an out-of-app
edit) — deleting the strays and skipping unparseable names. Adds a `list`
capability to the storage backend (the local-disk backend enumerates the
directory; backends that cannot list report zero). The maintenance summary is
reshaped to the reference app's order and shape (the store-children reaper now
runs before the orphaned-files sweep, and reports `{links, folders,
documents}`; the thumbnail sweep reports `{scanned, deleted, unparseable}`).

#### `8e116d1a` — 2026-08-06 — P4.D51 bug 9: retire the store-delete cascade-leak divergences

_Versions: harness 0.0.418._

P4.D51 (bug 9): the reference app now deletes a document store's children
in one transaction, leaking no orphaned document bodies or group links —
the cascade fix this port made first. The store-delete differential's
both-directions leak divergences are retired to plain equalities; the
boot orphan-reaper arm stays (a fix still unique to this port).

#### `783e5178` — 2026-08-06 — P4.D51 bug 12: reshape the restore dedup carve-out for v4's partial convergence

_Versions: harness 0.0.417._

P4.D51 (bug 12): the reference app now preserves an archived document
store's link IDs across a second-generation restore — the carried-file
dedup this port made first. Its convergence is partial: the reference app
kept its earlier file-restore phase order, so it still loses the "restored"
folder on one archive shape and, on a store file larger than 3 MB, invents
a phantom duplicate copy the archive never contained. Both remain deliberate
divergences where this port is ahead. The restore differential's dedup
carve-out is reshaped: the gen-2 archive is now a plain equality, and the
remaining phase-order divergences are pinned in both directions with a
self-retiring tripwire.

#### `79438835` — 2026-08-06 — P4.D51 bug 10 (delete-all half): retire the conversation_annotations divergence

_Versions: core 0.0.491, harness 0.0.416._

P4.D51 (bug 10, delete-all half): the reference app now clears
`conversation_annotations` on "delete all my data" — a privacy fix this
port made first. The delete-data differential's both-directions divergence
carve-out is retired; the annotation count is compared as a plain equality
like every other table.

#### `d89a5619` — 2026-08-06 — P4.D51 bug 11: retire the import store-identity + folder-clear divergences

_Versions: core 0.0.490, harness 0.0.415._

P4.D51 (bug 11): the reference app now recognizes an imported `.qtap`
archive's document store by its ID (not its display name), preserves that
ID on create, and clears folders on overwrite — all fixes this port made
first. The import-execute differential's both-directions divergence
carve-outs (store identity, folder clear) are retired to plain equalities.
One table stays carved out with a self-retiring tripwire: the per-chat
`conversation_annotations` sweep on chat delete is a sibling lane's (P4.D53)
change, so the two converge at unification.

#### `010c8b37` — 2026-08-06 — P4.D51 bug 15: retire the reindexLinkGroupSiblings divergence carve-out

_Versions: harness 0.0.414._

P4.D51 (bug 15): the reference app now re-chunks hard-linked file siblings
when one location is rewritten, where before its sibling-reindex pass was
dead code and the other locations served stale search chunks. This port
was already correct, so the both-directions divergence carve-out in the
mount-file-links differential is retired — the chunk table is now a plain
row-for-row equality.

#### `3c3aeef1` — 2026-08-06 — P4.D51 bug 13: guard gc_orphaned_file_row against a blobless mount index

_Versions: core 0.0.489._

P4.D51 (bug 13): garbage-collecting an orphaned mount-index file row no
longer crashes on an index that never held a blob (a document-only,
restored, or hand-built store). The blob and document payload deletes are
now guarded behind a table-existence check, so the second write to a path
succeeds instead of failing with "no such table: doc_mount_blobs".

#### `e54e3192` — 2026-08-06 — P4.D51 bug 18: widen the help-sync prune guard against blank-content wipes

_Versions: core 0.0.488, harness 0.0.413._

P4.D51 (bug 18): the help-doc sync no longer wipes the in-app Guide when
the only Markdown on disk is blank. A `help/` directory whose files are
all whitespace-only walks non-empty but produces no usable content;
previously that pruned every existing row. It now refuses the destructive
prune when nothing on disk parsed to usable content while the table is
populated, matching the reference app, and leaves the rows for the next
healthy sync.

#### `8ac1bb25` — 2026-08-06 — P4.D51 bug 8: converge the corrupt-properties.json character-vault refusal

_Versions: core 0.0.487, harness 0.0.412._

P4.D51 (bug 8): a character whose `properties.json` is present but
corrupt now fails a save loudly instead of clobbering the six vault-only
fields — a fix this port made first (finding #47), and the reference app
has since adopted it, so the two now refuse identically. The refusal
message matches the reference app's character-vault wording.
Corrected a stale scope note on the conversation-chunks repository doc
(it described `upsert` and its siblings as unimplemented, though the
Scriptorium slices landed them). Comment cleanup only.

#### `ae69b883` — 2026-08-06 — Count failed embedding rows in the Almanack + retire converged pins

_No crate versions bumped._

The Almanack's embedding-pipeline census now counts failed embedding
rows instead of a status value nothing writes (v4 Bug 19). The "Failed
rows" line — renamed from "Permanently failed rows" — now reflects the
real FAILED count for the current profile. The cast-size histogram and
the wardrobe-permission counts, which this port had fixed ahead of the
reference app, are now matched by the reference app too, so their
differential divergence pins are retired to plain comparisons (v4 Bugs
20 and 21).

#### `70efff36` — 2026-08-06 — Preserve the gate's memory links in the fold pass (bug 26)

_No crate versions bumped._

The fold-episode pass no longer discards the memory links its own gate
just created (v4 Bug 26). When a consolidated episode is written as a new
memory linked to a related memory, folding in the per-turn fragment
links used to overwrite the whole related-memory list, dropping the
gate's link. It now preserves the gate's links and adds the fragments on
top; a plain insert still starts empty.

#### `e7e3e1c4` — 2026-08-06 — Reconcile arm C (bug 17) + read mount points from the mount DB (bug 16)

_No crate versions bumped._

The startup reconcile now re-renders a conversation whose only stuck
chunk is over the per-chunk budget but under the transport cap (v4 Bug
17 arm C) — exactly the oversize chunk that could never embed before
sub-chunking, and that the FAILED-status skip otherwise leaves stranded.
Re-rendering splits it into embeddable in-context chunks; the pass stays
self-limiting and stale-gated.

The boot embedding-dimension reconcile's mount-chunk count now reads the
mount-index database where `doc_mount_points` actually lives (v4 Bug 16).
It had guarded on the main database, where that table does not exist, so
the count was dead — non-conforming chunks on an enabled document mount
were never counted toward a mismatched-dimension reindex. The
embedding-remainder differential fixture was regenerated to seed the
arm-C window chunk, and the old dead-code tripwire is retired.

#### `4ae88dd8` — 2026-08-06 — Null a conversation chunk's stale embedding on content change (bug 17)

_No crate versions bumped._

A conversation-chunk re-render now nulls a chunk's embedding when the
text at that interchange position changed (v4 Bug 17). Before, a
re-render preserved the stored vector unconditionally, so when an
oversize interchange split into sub-chunks — shifting every later chunk
to new content at an existing index — those chunks kept the previous
occupant's vector and were never re-embedded. The chunk upsert now nulls
a stale vector (the re-embed enqueue gate is "no embedding"), while a
caller that supplies a fresh embedding still wins. Unchanged text keeps
its vector. The conversation-chunks differential grew a three-op upsert
family (supplied-embedding overwrite / content-change null / identical-
content preserve), mutation-proven.

#### `0b785afc` — 2026-08-06 — Port bug 17's interchange sub-chunking into the Scriptorium renderer

_No crate versions bumped._

The Scriptorium conversation renderer now sub-chunks an oversize
interchange (v4 Bug 17). An interchange whose rendered text exceeds a
24,000-character budget is split into sequential in-context chunks — at
message boundaries first, then within a single very long message at
natural boundaries (paragraph, sentence, whitespace, and a hard cut only
when a single token itself overflows) — so a long conversation turn no
longer produces one chunk the embedding model can never accept and that
stays unsearchable forever. Anything under budget is unchanged, byte for
byte. All budget arithmetic counts UTF-16 code units, matching the
reference app. The conversation-markdown differential corpus grew four
over-budget cases (message-boundary split, a single-message boundary
walk, a split interchange 0 carrying the metadata header, and astral
content for the UTF-16 proof), all byte-exact against the reference
renderer.
Fixed a cluster of chat-state and impersonation bugs (v4 bugs 22, 23, 24, 27,
36, 37). "Speak as an AI character" now attributes correctly: starting it flips
the chosen character to user-controlled (and hands it back to the LLM on stop),
so the operator's next message lands under the right speaker and the badges are
truthful. A `controlledBy` change to a participant no longer short-circuits the
rest of the update, and removing an impersonating participant returns the chat
as it stands after cleanup (no more stale "still impersonating" state until a
refetch). The chat view now remembers saved values for the timeline mode,
lantern-image alerts, show-thinking, and answer-confirmation selects across a
reload, exposes the all-LLM pause count, and reports whether a connection
profile permits tool use (so the tool-settings dialog can warn when it doesn't).

#### `2bc03dfe` — 2026-08-06 — P4.D53 unit 3 — bug 28: attribute staff-signed announcements (finding #54)

_No crate versions bumped._

Staff-signed announcements now carry their author's name into the model's
context (v4 bug 28). An Insert-Announcement line signed as the Host (or any
Staff voice) used to reach every character as an anonymous block of prose, so
the model had to guess who spoke; it now arrives tagged with the resolved staff
name — and the tag lands on the opaque body too, so an opaque-anywhere chat
stays attributed. Ordinary Staff whispers (image notices, tool bubbles) are
left alone, since they name themselves in their own prose.

#### `60b96a40` — 2026-08-06 — P4.D53 unit 2 — bug 10: sweep per-chat annotations on chat delete

_No crate versions bumped._

Deleting a conversation now also sweeps its per-message annotations (v4 bug
10). Annotations sat on no cascade, so a deleted chat left its annotation rows
behind — harmless day to day, but a later restore of a migrated instance could
collide on the unique constraint. The sweep is scoped to the deleted chat.

#### `5507bd3b` — 2026-08-06 — P4.D55 bugs 31/33/34: OpenRouter vision send, Grok text/PDF, base64 round-trip

_No crate versions bumped._

Widened the `Content-Disposition` `filename*` escaping to cover the full RFC
8187 stray set — `' ( ) * !` are now all percent-encoded, matching the
reference app (v4 bug 41). v5 previously escaped only the apostrophe; v4 has
since caught up to the full set, so the two now agree and the deliberate
divergence is retired. Filenames with parentheses, asterisks, or exclamation
marks beside non-ASCII characters download with their real names intact.
OpenRouter non-streaming image sends now reach the model (v4 bug 31): a
completion carrying an image routes around the SDK — whose client-side
validation silently rejected image content — to a direct chat-completions
request, so regenerate and continuation legs with a picture no longer send
nothing. Grok text and PDF attachments are handled (v4 bug 33): text files
embed inline and PDFs get an honest "requires the Files API" message instead
of a blanket "unsupported file type" rejection. Text attachments that are
plain text (not base64) now ship verbatim instead of garbled, on both Grok
and Anthropic (v4 bug 34).

#### `b6fb5ab4` — 2026-08-06 — P4.D55 bug 35: the Ollama stream splitter buffers across reads

_No crate versions bumped._

The Ollama streaming decoder now buffers across network reads (v4 bug 35):
a JSON object split across two reads reassembles instead of being silently
dropped, a multi-byte UTF-8 character split across reads is no longer
corrupted, and a CRLF line terminator parses. The decoder is now
push-boundary-insensitive and joins the full three-chunking stream-decoder
equivalence.

#### `dfb18622` — 2026-08-06 — Land the P4.9H2A maintenance remainder (P4.43): memory-dedup + conversation-summaries regeneration

_Versions: web 0.0.64._

OpenRouter now reports its four image MIME types (JPEG, PNG, GIF, WebP) in
the client-side attachment-capability map instead of "unsupported" (v4 bug
32), so a new OpenRouter connection profile that omits the flag defaults to
image upload enabled — the client vision gate onto the now-working
non-streaming vision path.
Landed the memory-maintenance remainder (P4.43): the Settings → Commonplace
Book "Memory Deduplication" and "Regenerate Conversation Summaries" cards now
work. Analyze Memories finds semantically duplicate memories across all
characters (grouped by embedding width, clustered by cosine similarity), keeps
the best of each cluster, folds unique details from the discards in as
footnotes, and — on Run — removes the duplicates; preview costs nothing and
changes nothing. Regenerate Conversation Summaries re-mirrors every summarized
chat's summary into its participant vaults (the files the Commonplace Book reads
before a character's turn), deduping to one run at a time. Both are live over
the dispatch verbs and v4's `/api/v1/system/tools` and
`/api/v1/system/conversation-summaries` URLs.
P4.D54 (Salon + UI-polish SPA drift, `f4955e0e` round — in progress):
- Bug 29: a composer/Run-Tool result card no longer wears the last
  speaker's face. The standalone-tool avatar helper is now
  `resolveToolRowAttributionMessage`, which heads an `initiatedBy: 'user'`
  row as the operator; character-initiated rows keep the positional
  borrow.
- Bug 30: a private user-initiated run reads "whispered to you" instead
  of "whispered to unknown" — the whisper label resolves the operator's
  own userId to "you" (both the bubble label and the Staff header tag).
- Bug 22: the four controlled chat-sidebar selects (Story's Clock,
  Lantern image announcements, Thinking visibility, Answer Confirmation)
  now survive a reload instead of snapping back to their defaults — the
  server projects the columns (P4.D53) and the sidebar seeds from them.
  The `core-contract.ts` mirror gains `showThinking`,
  `answerConfirmationOverride` and `allLLMPauseTurnCount`; the three
  write-only-select divergence records are retired.
- Bug 36: the LLM Tool Settings dialog now shows its "tools disabled by
  connection profile" warning when an LLM participant's profile has tool
  use turned off — the projected `connectionProfile.allowToolUse` (P4.D53)
  makes v4's long-dead-code condition computable, and the binding fires it.
- Bug 42: toasts slide up and fade in on appearance (a 0.3s entry
  animation with a reduced-motion guard), matching v4's now-live
  `slideInUp`.
- Bugs 39/40: two v5-first fixes v4 has now converged on — the
  `.qt-text-danger` alias and the search-dialog body portal — so their
  "deliberate v5 divergence" comments are retired to convergence notes
  (no behavior change).
- Bug 37: an all-LLM room that auto-pauses (at 3, 6, 12, 24… turns) now
  explains itself with the All-LLM Pause dialog — continue, stop, or take
  control of a character — instead of stalling silently. The opener keys
  off the projected `isPaused`/`allLLMPauseTurnCount` (P4.D53) and rides
  the existing chain-complete refetch, so it also surfaces on loading an
  already-paused room.
- Tier 2: an e2e beat proving a Story's-Clock choice survives a reload
  is authored, gated behind `PROJECTION_ROUNDTRIP_SERVER_LANDED` until
  P4.D53's projection lands. The all-LLM-pause live-opener beat is
  deferred loud (needs a seeded paused all-LLM fixture).

#### `8cb86df1` — 2026-08-06 — Plan the f4955e0e found-bugs convergence round: six work orders + the round record

_Docs-only change._

Planned the `f4955e0e` found-bugs convergence round: six committed
work orders (P4.D51 guards/backup/mount convergence, P4.D52
scriptorium/memory/almanack, P4.D53 chat-API/attribution server,
P4.D54 Salon/UI-polish SPA, P4.D55 provider attachments/streaming,
P4.43 memory-maintenance remainder) absorbing v4's eleven-commit
"bugs 8–43" batch — at the new baseline every catalogued v4 bug is
fixed, much of it v4 adopting fixes this port made first, so the
round is largely convergence-pin retirement plus four genuine ports
(interchange sub-chunking, the AllLLMPauseModal, the OpenRouter
vision path, the thumbnail sweep). Docs only; no code changed.

#### `90ed1743` — 2026-08-06 — Close the fallback + wire + embedding-profiles round: docs, headers, round record

_Docs-only change._

Unified the fallback + wire + embedding-profiles round — four lanes, all
landed. OpenAI conversations no longer wedge when the provider forgets a
prior response: a failed chained request now retries once with the full
conversation, matching the reference app (finding #69). Web search is
connected end to end: with a Serper key set, the search tool actually
runs on every surface (chats, autonomous rooms, Carina, the Brahma
Console, Run Tool), and what the tool picker advertises is now derived
from the same source that executes, so they can never disagree.
Embedding profiles are manageable from Settings → Commonplace Book:
create, edit, delete, set default (with the update rules that queue the
right re-embedding work — full reindex, vocabulary refit, or the local
Matryoshka re-apply, which now has a live job handler taking a backup
before any rewrite). The Memory Deduplication and Regenerate
Conversation Summaries cards are built and answer an honest
not-yet-available message until their server halves land. Chat cards
grew the three-state Scriptorium badge with click-to-re-render. The
unification review fixed one wire defect before it shipped (an
explicit-null profile update dropped keys from the echo) and aligned
every error sentence with the reference app. Gate: 417 test binaries /
1,931 tests / 0 failed with all round differentials fresh at the pinned
baseline; SPA tests 292 files / 3,956; full Playwright 185 passed + 2
gated skips (the one flake is the documented wardrobe intermittent,
green in isolation). Versions: core 0.0.486, harness 0.0.411, host
0.0.61, web 0.0.63, SPA 0.5.422.

#### `520226a5` — 2026-08-06 — Restore the OpenAI conversation-chaining fallback (P4.41, finding #69)

_Versions: core 0.0.483._

Restored the OpenAI conversation-chaining fallback (finding #69). When a
multi-turn OpenAI chat sends a prior-response reference the server can no
longer find — routine, because both apps send unstored responses — the
streaming provider now retries the turn once with the full conversation
instead of failing it. Before this, such a turn errored and left the chat
wedged on the same dead reference. Matches the reference app's behavior;
the retry is a second request only on failure. Version: core 0.0.483.

#### `e5991a00` — 2026-08-06 — Pin the chaining fallback's retry bytes to a non-chained build (P4.41)

_Versions: harness 0.0.409._

Added a wire-byte regression test pinning the chaining fallback's retry
request to a from-scratch non-chained build of the same call (no
prior-response reference, the full conversation). Version: harness
0.0.409.

#### `2e687f9c` — 2026-08-06 — Add the tier-3 chaining-fallback differential (P4.41, finding #69)

_Versions: harness 0.0.410._

Added a differential test proving the chaining fallback recovers a turn
exactly as the reference app does: both run their real code (the
reference app's provider with its SDK made to fail once then succeed; our
streaming provider with the transport made to do the same) and the
recovered stream and retry pattern are compared. Version: harness
0.0.410.
Defined the memory-deduplication and conversation-summary-regeneration
maintenance actions in the request dispatcher; they currently return a
clear "not yet available" response pending their implementation.

#### `80a9b55b` — 2026-08-06 — P4.9H2A unit 4: EMBEDDING_REAPPLY_PROFILE handler + reapply differential

_Versions: core 0.0.485, harness 0.0.411, host 0.0.61._

Implemented the embedding re-apply job: when a profile's Matryoshka
dimensions are narrowed, the stored vectors are re-sliced and
renormalized in place (with a safety backup of each database taken
first) instead of re-embedding from scratch. The narrow arm of the
profile-edit trigger now does real work.

#### `5241a38a` — 2026-08-06 — P4.9H2A unit 3: embedding-profiles REST edges (quilltap-web)

_Versions: web 0.0.63._

Exposed the embedding-profiles management API at its standard web
addresses (in addition to the shared request endpoint), so the settings
screens can reach it by URL.

#### `e8d30e91` — 2026-08-06 — P4.9H2A unit 5 (routes): embedding-profiles routes differential + fixture family

_Versions: harness 0.0.410._

Added a differential test and committed test fixtures covering the
embedding-profiles management API against the reference app, including
the re-embed trigger matrix. Test-only.

#### `15930e05` — 2026-08-06 — P4.9H2A unit 2: embedding-profiles management verbs + handler + PUT matrix

_Versions: core 0.0.484._

Added the embedding-profiles management API: listing profiles (with
their API key, tags, and — for the built-in model — vocabulary and
embedding-progress stats), viewing, creating, updating, and deleting a
profile, and the manual re-embed / vocabulary-refit / re-apply actions.
Editing the default profile's model, provider, or dimensions now
re-embeds the whole corpus; narrowing a Matryoshka profile's dimensions
re-applies locally instead. Available embedding providers and their
models are listed for the settings screens. Wired into the request
dispatcher; the browser screens land in the companion change.

#### `ba9cdf2a` — 2026-08-06 — P4.42: wire the web-search provider into the running engine

_Versions: harness 0.0.409._

Extended the embedding-profiles data layer with the query helpers the
management surface needs: looking a profile up by name (for the
duplicate-name check when creating or renaming), clearing the default
flag across a user's profiles, and clearing a profile's optional
numeric or key fields back to empty. Internal groundwork for the
embedding-profiles management screens; no user-visible change yet.
Began wiring the web search tool (P4.42). Added an optional endpoint-URL
override to the Serper web-search provider so a test can point it at an
in-process mock; by default the request is unchanged and goes to the
real Serper endpoint byte-for-byte. No behavior change on its own — the
provider is still not connected to the running engine yet. Versions:
core 0.0.483, host 0.0.61.
Connected the web search tool to the running engine (P4.42). The
`search_web` tool was fully built and verified but never wired in, so
it refused at runtime even with a search key set, while the tools list
advertised it as available. Now, when `SERPER_API_KEY` is set, the host
builds the Serper provider once and threads it into every tool-running
surface — the in-chat turn, the character/ask-carina and Brahma Console
engines, and the operator Run Tool modal — so searches actually run
(one Serper call each). The tools list's "web search configured" flag
now derives from the very same provider, so it can never again claim
the tool is available while the tool refuses. The plugin-registry path
(a search plugin supplying the key) stays deferred; the environment-key
path is what this connects. Also added an optional endpoint-URL override
(default: the real Serper endpoint, byte-for-byte unchanged) so a test
can point the request at an in-process mock. A new wiring test proves a
runner built the production way actually runs a search (and refuses with
today's message when unconfigured), and pins the advertised-vs-executed
consistency both ways. An end-to-end browser beat runs a search through
a mock Serper server and shows the result card. Versions: core 0.0.483,
host 0.0.61, harness 0.0.409, SPA 0.5.417.
Added the P4.9H2B e2e beats: a live Salon walk clicking the Scriptorium
badge to enqueue a conversation render (asserting the dispatch + toast,
not job completion), plus the gated Commonplace Book management walk
(profile create → rename → delete on a non-default BUILTIN profile,
dedup preview zero-state, summaries enqueue) that activates once the
P4.9H2A server verbs land. SPA 0.5.422.

#### `d7df1f9e` — 2026-08-06 — P4.9H2B unit 5: the p4.9o Scriptorium-badge rider

_Versions: SPA 0.5.421._

Made the chat-card Scriptorium badge clickable to queue an on-demand
conversation render (P4.9H2B rider, p4.9o). The three-state badge
(not-rendered / rendered / rendered-and-embedded) is now a shared
component on both the Salon list card and the character Conversations
card; clicking it POSTs the render-conversation action, toasts, and
wakes the toolbar queue badges. The Salon card previously showed no
badge at all; the character card's was static. SPA 0.5.421.

#### `d3e49ac7` — 2026-08-06 — P4.9H2B unit 4: the Regenerate Conversation Summaries card

_Versions: SPA 0.5.420._

Ported the Regenerate Conversation Summaries card into the Settings →
Memory tab (P4.9H2B), last in v4's card order on the
`conversation-summaries-regenerate` deep link: a single-click enqueue
that re-mirrors every summarised chat into its participants' vaults, an
in-flight line that polls every 5s while a run drains and stops at zero,
and swallowed status-read failures (the button still works). This
completes the Commonplace Book tab — every card the reference app ships
now renders. SPA 0.5.420.

#### `eb70f139` — 2026-08-06 — P4.9H2B unit 3: the Memory Deduplication card

_Versions: SPA 0.5.419._

Ported the Memory Deduplication card into the Settings → Memory tab
(P4.9H2B), between Recall Relevance and Regenerate Memories on the
`memory-deduplication` deep link: the similarity-threshold slider
(0.70–0.95, default 0.80) and the Analyze → preview → Run dialog with
the per-character table, the totals, the zero-removable Run disable, and
the exact success toast. Preview failures show inline only, never as a
toast. SPA 0.5.419.

#### `b6ab160e` — 2026-08-06 — P4.9H2B unit 2: the Embedding Profiles card

_Versions: SPA 0.5.418._

Ported the Embedding Profiles card into the Settings → Memory tab
(P4.9H2B), first in v4's card order on the `embedding-profiles` deep
link: the profile list with provider and missing-key badges and
embedded-status counts, the four default-gated maintenance actions
(refit / re-embed everything / Matryoshka re-apply / re-embed
mismatched) with their two-step confirms and inline "View Tasks Queue"
success banners, and the create/edit modal with the BUILTIN model pin,
the validity gate, and the "Re-embed Everything?" follow-up on a save
that newly sets the default. The Matryoshka truncation inputs are
deliberately absent, matching the reference app (an API-only matrix
there). SPA 0.5.418.

#### `ae114b56` — 2026-08-06 — P4.9H2B unit 1: the §1 embedding-profiles + maintenance wire mirror

_No crate versions bumped._

Added the client-side wire contract for the embedding-profiles
management surface (P4.9H2B, §1): the eleven profile verbs (list / get /
create / update / delete / refit / reindex / reapply / list-providers /
list-models / fetch-models) and the four memory-maintenance verbs
(dedup preview + run, conversation-summaries status + regenerate), as
CoreClient methods over `dispatchData` plus the request/DTO types.
Server half is the parallel P4.9H2A lane. SPA 0.5.417.

#### `f160203c` — 2026-08-06 — Plan the fallback + wire + embedding-profiles round: four work orders

_Docs-only change._

Planned the next porting round and committed four work orders (docs
only, no code change): restore the OpenAI conversation-chaining
fallback that finding #69 showed was dropped in the port (P4.41),
connect the already-ported Serper web search to the running engine so
the search tool stops advertising while refusing (P4.42), and port the
embedding-profiles management surface — profile CRUD with the update
trigger matrix, the profile re-apply job, memory deduplication, and
conversation-summary regeneration — as a server lane and an SPA lane
(P4.9H2A/P4.9H2B, the SPA lane also carrying the chat-card Scriptorium
badge). Round record in the status log; no version bumps.

#### `972e4a25` — 2026-08-06 — Dogfood #66/#67/#68: fix two Almanack ledger-collector bugs on real Friday data

_Versions: core 0.0.482, harness 0.0.408._

Fixed two defects in the system report ("The Almanack"), found while
reviewing the report against real data. The "Cast sizes" table now
groups conversations by their number of participants instead of listing
one row per conversation. The character summary's "may dress themselves"
and "may create outfits" counts now reflect the effective permission — a
character left at its default is permitted, and only an explicit opt-out
is excluded — where before they counted only characters explicitly
switched on and so read zero on most instances. Both changes match how
the reference app behaves at runtime rather than how its report counted;
they are recorded as intentional differences that revert automatically
if the reference app adopts the same fix. Versions: core 0.0.482,
harness 0.0.408.

#### `8633cbd1` — 2026-08-06 — Close the Taboo + maintenance round: round record, order headers, phase plan, baseline move

_No crate versions bumped._

Unified the Taboo + maintenance round — three lanes, all closed. The
system report ("The Almanack") is now fully live: its collectors are
verified against the reference implementation by a new 72-check
equivalence suite over a committed four-database fixture family, the
host wiring makes the report reachable in production, timestamps in the
plain space-separated database vintage render correctly, and the
end-to-end walk (compile, per-phase progress, viewer, download, delete)
runs on every test pass. The reference app's newest feature, Taboo, is
absorbed whole: an instance-wide list of phrases no character may
utter, normalized on save, rendered into the cacheable prefix of every
character's system prompt (verified byte-for-byte including both cache
goldens), served over new settings endpoints, and edited from a new
Settings → Chat card. The maintenance sweep repaired the two
differential oracles that could no longer regenerate (a corpus defect,
not a port bug — and the same defect turned out to explain a
long-standing red in a third suite), fixed a flaky tracing test at its
real cause, hardened two intermittent end-to-end checks, and taught the
recipe sweep driver to regenerate safely while the reference app is
mid-drift. The unification review caught and fixed a real bug before it
shipped — an explicit null sent to the new Taboo endpoint would
silently keep the stored list where the reference app rejects it — plus
five report-fidelity nits (user scoping, error wording, number shape, a
dropped id-format gate, an unknown-provider skip) and three rotted
regeneration recipes. Gate: 414 Rust test binaries / 1,911 tests / 0
failed with eleven equivalence families re-proven fresh; SPA 3,926
tests / 0; full Playwright 183 passed / 0 failed / 0 skipped. Versions:
core 0.0.481, harness 0.0.407, host 0.0.60, web 0.0.62, SPA 0.5.416.

#### `efdc58fa` — 2026-08-06 — P4.37: activate the Almanack e2e walk; fix the inline-host click bug it caught

_Versions: SPA 0.5.413._

The system-report card's end-to-end test walk is active: compile,
per-phase progress, viewer, download, delete. Activating it caught two
real issues fixed here: the report card's and the settings tab
container's custom elements had no display rule, so their inline boxes
did not enclose their content and automated clicks on the compile button
were read as landing on the tab container (the same class of bug as the
chat sidebar's Add Character footer); and the test's viewer assertion
was ambiguous once a real report rendered, since the dialog's
description repeats the report title.

#### `4aec0307` — 2026-08-05 — P4.37: wire the Almanack host seam - the report is live in production

_Versions: host 0.0.60._

The Almanack is now reachable in production: the server supplies the
report pipeline its database paths, backups directory, honest runtime
facts (host version, platform, uname, total memory, process uptime,
timezone), the passphrase-protection flag, the application version, a
clock, and the disk storage layer. Until now the four report actions
answered a clear "not assembled" refusal even though the collectors were
fully built. The runtime section describes the actual host process — a
documented difference from the reference app's Node facts.

#### `487f7fc4` — 2026-08-05 — P4.37: parse the space-form date vintage in the Almanack's formatters

_Versions: core 0.0.478, harness 0.0.405._

The system report's date columns now render the plain space-separated
timestamps older database rows carry ("2026-08-05 09:07:03") instead of
"N/A". The reference app's JavaScript date parser has always accepted
that form; our strict ISO parser did not, so any report column fed such a
stamp went blank. The acceptance is scoped to the report's two formatters
only, and a new render-oracle case pins the behavior against the
reference app byte-for-byte.

#### `3adcf77f` — 2026-08-05 — P4.37 unit 12: the almanack-* fixture family + the tier-2 differential

_Versions: core 0.0.477, harness 0.0.404._

The new system report ("The Almanack") is now verified against the
reference app end to end: a new committed fixture family exercises every
report section non-trivially over all three databases, and a differential
drives the reference app's real collectors and route handlers against
ours — comparing the report data, the rendered markdown byte-for-byte,
the four report actions with their download link, the per-phase progress
frames, and the rows both apps persist when a report is filed. Both
attribution arms are proven (a modern call log with the per-profile
columns and a legacy one without). One fix fell out: the report's
library entry no longer invents a description on the stored copy the
reference app leaves blank. Two knowingly accepted differences are
pinned so any drift trips the test: our provider list omits the built-in
TF-IDF entry (no plugin loader), and JSON parse-failure wording follows
the JSON engine.

#### `bda0d2f0` — 2026-08-05 — Port the Taboo list into the system prompt (P4.D50 unit 1)

_Versions: core 0.0.481, harness 0.0.407._

Ported the collectors, orchestrator and API surface of the new system
report ("The Almanack"): the seven-phase census across all three
databases, the four report actions (generate / list / get / delete) with
the download link, and per-phase progress on the live event stream.
NOT YET VERIFIED against the reference app and NOT YET REACHABLE: the
report's equivalence test is still to be built, and the server answers a
clear "not assembled" until the host supplies its paths and version.
Characters can now be told which phrases they may never utter. An
instance-wide Taboo list is stored in instance settings and rendered
into the universal, cache-stable portion of every character's system
prompt on conversational turns, between the math-notation note and the
per-turn tool instructions. Saving normalizes the list — each entry
trimmed, blanks dropped, case-insensitive duplicates removed keeping
the first — while deliberately preserving the order you arranged, so
the cached prompt prefix only shifts when you actually edit the list.
An empty list renders nothing at all, so an instance that never touches
the feature produces a byte-identical prompt to one built before it
existed. The prompt-structure cache version moved from 2 to 3
accordingly. The list travels: it exports with instance settings and
rides along in full backups. Introspection (the self-inventory tool)
deliberately omits the section, matching the reference app. The list is
read and written over `/api/v1/settings/taboo`, which merges a partial
body over the stored value so an incomplete request can never wipe it,
and is edited from a new Taboo card on Settings → Chat, between
Dangerous Content and Data Retention. The card sends the whole list on
every add and remove and renders back exactly what the server stored,
so the normalization is visible where you made the edit. The Taboo help
page from the reference app is not carried over — this build has no help
surface to show it on.
Fixed a check in the oracle-recipe sweep tool that was flagging
correct recipes as broken. It warned whenever a recipe pointed the
test runner at a directory ending in the standard layout — which the
correct, staged form also ends in — so sixteen already-correct
comparisons were reported as debt and the tool refused to run them
from a working branch. All sixteen now run there, proven by running
them. Nine recipes that really were in the old form were converted,
and two doc comments that opened a sentence with a command word (and
so were being executed as shell) were reworded. No product code
changed.

#### `8e51f3da` — 2026-08-05 — Work the recipe-rot tail down; pin the sweep at a v4 worktree (P4.40 unit 4)

_No crate versions bumped._

Repaired the last broken oracle-regeneration recipes and gave the
sweep driver the one option a catch-up round needs: a way to point
every recipe at a pinned copy of the reference app, so a sweep run
while the reference app is ahead of us cannot quietly bake in changes
we have not absorbed yet. Re-running the known-broken set from that
pin found fifteen of sixteen already healthy. The sixteenth — a
comparison two earlier rounds had argued over and left red — turned
out to have exactly the same cause as the two oracles repaired
earlier today, and is now green. No product code changed.

#### `97340a71` — 2026-08-05 — Deflake the terminal-typing and rename-toast e2e beats (P4.40 unit 3)

_No crate versions bumped._

Hardened two end-to-end checks that could fail on a busy machine
without anything being wrong with the app. The terminal check typed
its "exit" the moment the terminal appeared, but the connection
underneath opens a moment later and silently discards anything sent
before it does — so on a slow run the command was never sent and the
check waited out its clock for a shell that had not heard it; it now
waits for the shell's own first output first. The rename check looked
for a "Chat renamed" notice by text alone, and when an earlier notice
had not yet faded there were two on screen and the lookup became
ambiguous; it now waits for the save round trip and matches the
success notice specifically. Both failures were reproduced on demand
before being fixed, and neither check asserts less than it did.

#### `bac876f0` — 2026-08-05 — Deflake failed_job_emits_a_tracing_event (P4.40 unit 2)

_No crate versions bumped._

Fixed a test that could report the job runner had stopped logging
failures when it had not. The check installed its log capture only for
its own thread, but the logging library decides once, process-wide,
whether a given log statement is worth evaluating — and a sibling test
running first, with no capture installed, could switch that statement
off for everyone. Under a parallel run the check failed roughly two
times in three. The capture is now installed process-wide with
per-thread buffers, which is what makes the statement reachable no
matter which test ran first. Nothing it proves changed: the failure
event must still be a warning on the same log target, and the job row
must still end up marked failed.

#### `467c038a` — 2026-08-05 — Repair the two unregenerable tier-3 oracles (P4.40 unit 1)

_No crate versions bumped._

Repaired two differential test oracles that could no longer be
regenerated. Both drive the reference app's real memory and
conversation-summary code, and both had started failing outright
because their test data used placeholder connection-profile names
where the reference app now requires real identifiers — so every
call-log row it tried to write was rejected, silently, and the
comparison had nothing to read. The test data now uses well-formed
identifiers and both comparisons run again, with a note in each
recording why. No product code changed.

#### `2a0c0b81` — 2026-08-05 — Plan the Taboo + maintenance round (P4.37-resumed ∥ P4.D50 ∥ P4.40)

_Docs-only change._

Planned the next porting round and committed its work orders — three
lanes. The first resumes the held Almanack server work from its
preserved branch: the report collectors get the equivalence test they
were held back for, the host wiring that makes the report reachable in
production, and a date-format arm for the space-form timestamps real
instances carry. The second absorbs the reference app's newest
feature, Taboo — an instance-wide list of phrases no character may
utter, stored in instance settings, normalized on save (trimmed,
deduplicated case-insensitively, order preserved), rendered into the
cacheable prefix of every character's system prompt, and edited from a
new Settings → Chat card. The third is a maintenance sweep: repairing
the two differential oracles that can no longer regenerate, curing a
flaky tracing test and three intermittent end-to-end beats, and
working down the remaining broken regeneration recipes. Documentation
only — the implementation is the two new work orders plus the resume
assignment added to the Almanack order.

#### `a892b153` — 2026-08-05 — Measure real durationMs on both image-path llm_logs sites

_Versions: core 0.0.475._

Image-generation calls now record how long the provider actually took.
Two internal call-log sites (the avatar/story-background job path and
the in-chat image tool) were writing a duration of zero on every image
call, which made those rows read as unmeasured in the system report's
latency figures; both now measure the real wall clock around the
provider call, on the primary attempt and the Concierge reroute alike,
matching the reference app. Unit tests pin that the logged duration
actually brackets the provider call.

#### `4b23e1ee` — 2026-08-05 — Close the f7f1a956 round: round record, order headers, phase plan, baseline move

_Docs-only change._

Unified the `f7f1a956` Almanack round (partially): the reference app's
system-report rewrite is absorbed on every already-ported surface, and
the new report's foundations are in. The LLM call log gains the two
per-profile attribution columns (fresh schema follows the reference;
existing pre-migration instances stay readable, and the write refuses
exactly as the reference does), call sites now record which profile
served a call and how long it took, and a token-accounting bug is
fixed that had made autonomous-room daily token budgets a silent no-op
— rooms with a daily budget now actually pause or end when they spend
it. Restores remap the new columns. The Almanack's byte-exact markdown
renderer, its phase manifest, and a progress "phase" event landed with
a seven-case differential; the full settings card, report viewer, and
a shared segmented progress bar (also adopted by the Proving Bench and
search-importance meters) are live in the SPA — but the server-side
report collectors were deliberately held back pending their
equivalence test, so the card lists no editions yet. The provider
manifest generator was repaired (regenerating manifests no longer
drops the image-model lists) and perl was purged from the Docker
image. The unification review caught a stale test-schema copy that was
silently costing log coverage, restored the report viewer's heading
typography, and documented one layout divergence. Gate: 413 Rust test
binaries / 1,867 tests / 0 failed with twelve differential families
re-proven over oracles pinned at the reference baseline; SPA 3,914
tests / 0; Playwright 180 passed with only the deliberately gated
Almanack beat skipped. Versions: core 0.0.474, harness 0.0.403, host
0.0.59, web 0.0.61, tauri 0.0.6, SPA 0.5.412.

#### `36a4e485` — 2026-08-05 — P4.D49 units 7+9: the blast-radius sweep, two stale DDLs, and the inspector coverage

_Versions: core 0.0.472, harness 0.0.402, host 0.0.59, web 0.0.61, tauri 0.0.6._

The LLM Inspector shows which profile served each logged call, on
instances whose log database carries the new columns; older rows and
older instances read exactly as before.

#### `3b076024` — 2026-08-05 — P4.D49 unit 8: `f7f1a956` is NO-PORT, and its jest-TZ clobber is defused

_Versions: harness 0.0.401._

Harness only: the reference app now pins its test timezone to UTC before
forking test workers, which silently overrode the timezone our two
zone-legged oracle regenerations pass on the command line. Both now apply
their zone from a hook that runs early enough, prove it took, and record
it so a future override fails loudly instead of quietly re-recording the
wrong leg.

#### `9e3ca4f7` — 2026-08-05 — P4.D49 unit 6: the llm-logs profile ids join the new-account UUID remap

_Versions: core 0.0.471._

Restoring a backup into a new account now carries LLM-log profile
attribution across: a log row's connection-profile and image-profile ids
are remapped alongside the profiles themselves, so restored rows keep
naming a live profile instead of one that no longer exists.

#### `af4ecf8b` — 2026-08-05 — P4.D49 units 4-5: the token-usage un-zero and the autonomous daily budget it revives

_Versions: core 0.0.470._

Autonomous rooms' daily token budget now works. The query behind it
filtered on a condition SQL can never satisfy, so the spend it summed was
always zero and a configured daily budget never bound on anything; the
reference app found and fixed this and v5 had reproduced it faithfully.
A room with a daily token budget will now grant its grace turn and then
pause at instance-local midnight rollover, as it was always meant to.
Rooms without one are unaffected.

#### `5d4e361d` — 2026-08-05 — P4.D49 units 2-3: the llm_logs profile columns through the write spine and the six call sites

_Versions: core 0.0.469, harness 0.0.400._

LLM call logs now record which profile served the call. Every text call
made through a connection profile stamps its id; every image call stamps
its image profile id, including Concierge reroutes. The shared cheap-LLM
path and the content gatekeeper now also measure how long the provider
took — both had been writing no duration at all, which left every
latency average hollow. Logs written before this change read exactly as
before, and an instance whose log database predates the new columns
still opens and reads.

#### `0a26dadc` — 2026-08-05 — P4.D49 unit 1: the D23 re-dump for the llm-logs profile columns

_Versions: core 0.0.468._

Adopted the reference app's new LLM-log schema: a freshly provisioned
instance now creates the call log with the two profile-attribution
columns (`connectionProfileId`, `imageProfileId`), re-dumped from the
reference app's live schema generator rather than hand-edited. No
indexes changed.
Generalized the progress side-channel so any long operation can narrate
named phases, not just chat creation. Existing progress messages are
unchanged on the wire.

#### `f9816ad5` — 2026-08-05 — P4.37 units 1-2: the Almanack's pure core + the tier-1 render differential

_Versions: core 0.0.473, harness 0.0.403._

Added the pure core of the new system report ("The Almanack"): the
seven-phase manifest, the report data model, and the markdown renderer,
all byte-identical to the reference app. The renderer is verified against
the reference app's real code over seven inputs covering every branch,
including empty sections, an unreachable document store, and locale
number and date formatting. No user-visible surface yet — the collectors
and the report API follow.
Added browser tests for the Almanack card and marked the parity
checklist row done. The card and its deep link are covered now; the
full compile-and-view walk is written and waits on the server half.

#### `b2cb084f` — 2026-08-05 — Port the Almanack card and its report viewer onto the Providers tab

_Versions: SPA 0.5.411._

Added the Almanack card to the AI Providers settings tab, with its
report viewer. Compiling a report shows a phase-by-phase progress bar
while it runs and opens the finished report when it's done; previous
editions can be viewed, downloaded, or deleted. The card answers the
same `?section=capabilities-report` deep link older bookmarks use.

#### `cf060b98` — 2026-08-05 — Move the Proving Bench and search-importance meters onto qt-progress

_Versions: SPA 0.5.410._

Moved the two existing meters — the Proving Bench outcome shares and the
search results' importance bar — onto the shared progress styles. Both
were painted with hardcoded colors no theme could reach; they now draw
from the same variables as every other bar in the app.

#### `b8df4641` — 2026-08-05 — Add the shared segmented progress bar and the qt-progress CSS family

_Versions: SPA 0.5.409._

Added the shared progress bar and its themeable `qt-progress` style
family. The bar shows one segment per phase of a long operation, sized
by how long each phase usually takes; the running segment stops at 90%
so a slow phase reads as still working rather than finished and stuck.

#### `44ee9c5d` — 2026-08-05 — Purge perl-base from the Docker runtime image

_No crate versions bumped._

Mirrored the Almanack's client contract into the web app: the four
dispatch verbs (generate, list, get, delete), the new `phase` progress
frame on the shared event stream, and the seven-phase manifest whose
labels and timing weights must match the server's exactly.
Purged `perl-base` from the Docker image. Debian's slim base ships it as
an Essential package carrying a set of critical and high CVEs with no fix
available, and nothing in the image is perl — the runtime is two compiled
Rust binaries. The TLS root bundle and the timezone database, the two
things the server actually needs from the base image, are untouched;
verified by building the image and walking a fresh container through
setup, the seeded home dashboard, a restart, the in-container CLI, and
`QUILLTAP_TIMEZONE=America/Chicago` resolving. Installing packages inside
a running container is no longer reliable, which is an acceptable trade
for a dev-grade image.

#### `3be673f5` — 2026-08-05 — Teach gen-provider-manifests.mjs the imageGenerationModels field

_No crate versions bumped._

Repaired the provider-manifest generator so regenerating the nine
built-in provider manifests no longer silently drops each provider's
image-generation model list. The list had been added to the committed
manifests by hand and never taught to the generator, so a regen deleted
it from five of them — google, grok, openai, openrouter, z_ai — and the
only symptom would have been an empty model dropdown when configuring an
image profile. The generator now reads the list off each built plugin
where the plugin exposes it, and out of the plugin's own image-provider
source for the two that do not; an unrecognized source shape stops the
run with a named error instead of emitting a stale manifest, and nothing
is written until all nine build. Regenerating now reproduces all nine
committed manifests byte for byte.

#### `1585456f` — 2026-08-05 — Plan the f7f1a956 Almanack round: four work orders + the drift classification

_Docs-only change._

Planned the `f7f1a956` Almanack round and committed its four work
orders: the reference app rewrote its system capabilities report into
The Almanack, adding per-profile attribution columns to the LLM call
log and fixing a token-accounting bug that had made autonomous-room
daily token budgets a no-op. One order absorbs those changes on the
already-ported surfaces (schema, call-site logging, the budget fix,
backup remapping), two port the new Almanack report itself (server
collectors/renderer/API, and the settings card with a new shared
progress-bar component), and a fourth repairs the provider-manifest
generator so regenerating manifests no longer silently drops the
image-model lists. Planning docs only; no code changed.

#### `9d62dd51` — 2026-08-05 — Close the 7189a968 round: round record, order headers, phase plan, baseline move

_Docs-only change._

Unified the `7189a968` round: the reference app's import/export overhaul
is fully absorbed. Exports no longer carry memory embeddings (99.7% of a
real 791 MB archive; the importer re-embeds what it inserts, one queued
job per memory), the export picker reaches all fifteen types (files with
their bytes, prompt templates, provider models, plugin settings with
password-key redaction, instance settings minus the instance-local
keys), a long-standing ordering bug that silently dropped every group's
linked document stores on import is fixed, and an opt-in compact backup
leaves the search indexes behind and rebuilds them after restore. The
import preview now explains per-item notes ("secrets withheld…"), and
the backup dialog gained the compact checkbox. Separately: the reference
app's Anthropic SDK jump was proven byte-neutral on the wire; the Docker
container now honors QUILLTAP_TIMEZONE/TZ so scheduled rooms and
same-day recall stop running on UTC; and a stale harness mock that had
been mis-reporting a context-summary divergence was retired. Gate: 412
Rust test binaries / 1,854 tests / 0 failed with twelve differential
families re-proven over oracles pinned at the reference baseline; SPA
281 files / 3,870 / 0; full Playwright green with the round's gated beat
live (numbers in the round record). Versions: core 0.0.467, harness
0.0.399, web 0.0.60, SPA 0.5.407.

#### `881dc3a0` — 2026-08-05 — Port compact backup + the restore tail (P4.D46 u5); pin the drifted v4

_Versions: core 0.0.467, harness 0.0.399, web 0.0.60._

Compact backup lands end to end (P4.D46 unit 5, reference `7189a968`):
opt-in via `compact: true` on the backup-create call (body optional, a
malformed one treated as absent, only literal true engages), memory
embeddings nulled at their schema slot, the six derived embedding
collections omitted from the archive outright — absent is what shrinks
it — and `manifest.compact` stamped only when true, so a full backup's
manifest stays byte-identical. Restore gained the matching tail: a
compact archive enqueues a full re-index before the embedding reconcile
(so the reconcile's dedupe sees it — proven by the oracle, whose
reconcile reports no second enqueue), and every restore now ends with
the dimension reconcile, its outcome riding the summary as
`embeddingReconcile` with the reference app's warning sentences. A new
committed compact archive, built by the reference app's real
createBackup, drives the new differential arms; the eleven existing
restore cases regenerated green with the tail, and the two compact
cases extend the ruled replay-dedupe divergence by name. Found on the
way: the reference app's restore-oracle environment had its vector
store globally mocked, which made the whole reconcile throw into its
catch the moment a restored corpus carried real vectors — the oracle
now uses the real module, same class as the embedding-service mock.

#### `9666907d` — 2026-08-05 — Port the 7189a968 export/import server surface (P4.D46 u2-u4, u6 tri-state)

_Versions: core 0.0.466, harness 0.0.398, web 0.0.59._

The five new export types land server-side (P4.D46 unit 4, reference
`7189a968`): files (folder tree, metadata, and bytes as chunked base64
through the same counted-arrivals reassembly the document-store blobs
use), prompt templates (built-ins never travel), the provider-model
catalogue (a regenerable cache, exportable for air-gapped instances),
plugin configs (every password-typed manifest key redacted, the whole
config withheld when the manifest can't be resolved — v5 carries a
static transcription of the bundled manifests, generator included), and
instance settings (minus the five non-portable keys). The entity
listing and both previews cover all fifteen types, and the importers
land with all four conflict strategies: file bytes are written back
through the same storage bridges the backup restore uses (storage keys
never transfer; post-bridge mime/size win; dangling links dropped so
cascade-delete can't be fooled), prompt templates dedupe by name,
provider models upsert by provider and model id, plugin configs merge
so a redacted key can't clobber a local secret, and instance settings
overwrite unconditionally. The plugin-config upsert gains the reference
app's tri-state `enabled` flag, passed by restore and import both, so a
plugin the user had switched off doesn't come back on. Differentials:
the export family grew to 57 cases, the import-read family to 25, and
the import-execute state family to 23 — including a four-strategy
files matrix whose first run caught two real gaps (the mount-stats
refresh v5's bridge leaves to callers, and the upload-failure message
wrapper) and one oracle artifact (v4's fire-and-forget stats refresh
caught mid-air by the state dump; the oracle now settles pending tails
before dumping, which is also what keeps them from poisoning the next
case).

Embeddings no longer travel in `.qtap` exports, matching the reference
app's `7189a968` overhaul (P4.D46 units 2–3). The writer strips the
field at both memory emit sites, the NDJSON reassembler drops it from
older archives that still carry it, and the legacy import path drops it
again before validation. Every memory an import creates now gets its own
`EMBEDDING_GENERATE` job queued after the reconcile — previously nothing
re-embedded and semantic search stayed broken until the next restart —
with the reference app's exact bail-out warnings when no default
embedding profile is configured or the default is the built-in TF-IDF
one. Document stores also import before the group→store link step; they
ran dead last, so in a mixed archive every group's linked stores were
silently dropped (proven by mutation: moving the step back turns the
differential red on `group_doc_mount_links`). The entity listing gains
`groups` and `document-stores` and the import preview gains the
`documentStores` array, both from the same reference commit. The
import-execute oracle also stopped lying: the jest setup file's global
embedding-service mock had `getDefaultEmbeddingProfile` pinned to null,
so the oracle claimed the reference app never enqueued — the
`doMock(requireActual)` antidote plus a processor-wake stub fixed it,
and the job rows the state diff now compares are real on both sides.

#### `0c6b9ee3` — 2026-08-05 — Widen the system-data fixture for the export/import drift port (P4.D46 u1)

_No crate versions bumped._

Widened the shared `system-data-*` test fixture for the export/import
drift port (P4.D46 unit 1): a second database store hard-linked to the
group, the Quilltap Uploads mount with its `userUploadsMountPointId`
pointer, two files with real bytes stored through the reference app's
uploads bridge (one project-less text file, one project-bound binary
over the 3 MiB chunk size), an embedding-bearing memory, a built-in
prompt template, a switched-off bundled-plugin config, and four more
instance-settings rows. No production code changed; every consuming
differential regenerates over the rebuilt DBs as the round's units land.
The Export Data picker now offers all fifteen kinds of data the writer
can produce, not seven. Prompt templates, projects, character groups,
document stores, files and folders, provider models, plugin settings,
and instance settings were all exportable and none of them could be
reached from the screen. The list is now exhaustive by construction —
a kind without a label no longer compiles — and the picker's order and
wording match the reference app exactly. None of the eight new kinds
needs an extra wizard step: each is a two-choice, four-step flow, and
the names the server composes are shown exactly as they arrive.

#### `0ff2a759` — 2026-08-05 — Show the import preview's per-item notes and new sections

_Versions: SPA 0.5.405._

The import preview lists those new kinds too, and now carries the
server's per-item notes: a file whose contents did not travel says so
before you import it, and a plugin's withheld secrets are named so you
know what you will have to type back in.

#### `045c95e7` — 2026-08-05 — Offer a compact backup, off by default

_Versions: SPA 0.5.406._

Create Backup offers a compact archive, off by default. It leaves the
search indexes behind — a restore rebuilds them — for a considerably
smaller file. Full fidelity stays the default on purpose: a backup
usually returns to the same instance, where those indexes are still
good, and rebuilding costs time and money at the worst possible moment.

#### `c26a0c93` — 2026-08-05 — Honor QUILLTAP_TIMEZONE / TZ in the container (P4.D48 unit 2)

_No crate versions bumped._

The browser walk for all of the above is written and waits on the
server half of the same change; it starts running the moment that lands.
Fixed the timezone in Docker. A container has no timezone, so it ran on
UTC — and that was not just cosmetic, because rooms that wake on a
schedule, the daily token allowance that turns over at midnight, and
"today" for same-day recall all read the clock directly. A room set for
7am fired at 2am. Worse, setting `QUILLTAP_TIMEZONE`, the one variable
the docs mentioned, fixed only the printed timestamps and left the
schedules on UTC, so it looked solved. Now either `QUILLTAP_TIMEZONE` or
`TZ` sets the whole process, whichever you supply fills in the other, and
`QUILLTAP_TIMEZONE` wins if the two disagree — matching the reference
app. The value must be an IANA name (`America/Chicago`, or `UTC`); an
abbreviation like `CDT` is refused with a warning instead of being
forwarded and silently falling back to UTC. `docs/developer/running.md`
documents it.

#### `83315ca5` — 2026-08-05 — Prove the be2c9cbb Anthropic-SDK jump wire-neutral (P4.D48 unit 1)

_No crate versions bumped._

Checked that the reference app's Anthropic SDK jump — 27 minor versions,
0.88 to 0.115 — did not change a single byte on the wire, and it did not.
All four recorded corpora that drive the Anthropic plugin (request
envelopes, response bodies, tool wire, streaming frames) were re-recorded
against the new SDK and compared byte for byte against the committed
copies: identical, including the streaming event types the upgrade put at
risk. The seven differentials that consume them pass unchanged. Nothing
to fix, but the check is the point: the last time an SDK was upgraded on
a claim of "no wire change," the proving run turned up two real bugs.
The same differential now also compares what that pass writes — the dated
episode memories it consolidates out of a folded stretch of conversation,
and the links it makes between an episode and the individual turns it came
from — rather than only checking that both apps asked the model the same
question. Doing so turned up a second stale mock: the test harness had
been standing in for the vector store with one that always returned no
matches, which meant the reference app's duplicate check never actually
ran. With that removed, both sides agree.

#### `346acc17` — 2026-08-05 — Un-mock the fold-episode pass in the context-summary oracle

_No crate versions bumped._

Retired a stale mock in the context-summary differential (test-only; no
shipped behavior changed). The fold-time episode pass — which turns a
just-folded stretch of conversation into dated episode memories — runs in
the reference app on every fold, and it runs here too. But the test's
canned model had no answer for the episode prompt, so on both sides the
call died as "unrecognized prompt" and the pass was suppressed in all but
name; the resulting row-count mismatch had been sitting as an unexplained
red. The canned model now answers it, so the pass runs to completion on
every fold in the corpus and both sides are compared on what it does.

#### `de624c15` — 2026-08-05 — Plan the 7189a968 round: four work orders for the export/import drift, the SDK wire check, and the stale-mock retirement

_Docs-only change._

Planned the next porting round (docs only). The reference app shipped
seven commits, headlined by an import/export overhaul on already-ported
surface: embeddings no longer travel in exports, five new export types
plus an exhaustive type listing, and an opt-in compact backup. Four work
orders cover it — the server port (`p4.d46`), its SPA half (`p4.d47`),
a wire-neutrality check on the reference app's Anthropic SDK jump plus
container-timezone handling (`p4.d48`), and retiring a stale harness
mock on the context-summary differential (`p4.36`). Also corrected two
stale status cells on dogfood findings #58/#59, which had stayed "open"
after their fixes landed.

#### `bb935688` — 2026-08-04 — Let a real export be imported, and say so when a preview fails

_Versions: web 0.0.58, SPA 0.5.402._

Large exports can be imported again. A `.qtap` over 100 MB — a real
character library runs to hundreds of megabytes once vault images are
included — was refused at the edge before any of the import code ran,
with an unhelpful "Payload Too Large". The ceiling now matches the
reference app's own, which is 10 GB and always was; the wrong one of its
two configured limits had been copied. Note that a very large import is
now possible, not cheap: the whole archive is held in memory while it is
read, so a multi-gigabyte file will cost multi-gigabyte memory until the
import path learns to stream.

The import wizard also stops going blank when a preview fails. It
recorded the reason and drew nothing — an empty "Step 2 of 5" with
working buttons and no explanation, in both apps. It now shows what went
wrong, a deliberate departure from the reference app in the
backup/restore/import/export family, where a lost explanation is worst
precisely when something has already gone wrong.

#### `eb045d97` — 2026-08-04 — Run P4.34's phase-2 handoff at unification and adjudicate the round's one red

_Docs-only change._

Unified the 7fe9fe40 drift-catch-up round onto the main line: four
parallel lanes, all closed, and the oracle baseline moves to the
reference app's head — no drift debt remains. The New Chat form gains
its Roleplay Template dropdown: pre-selected with what the chat would
have gotten anyway (project default, then your global default, then No
Template), hidden when no templates exist, and an explicit choice —
including a deliberate "No Template" — now beats both defaults at
creation, with an unresolvable template refused outright. Staff
announcements and the tool-execution rules stop teaching models
asterisk-delimited narration, which also retires a long-stale wording
gap in the native tool prompt. Importing a document store now matches
by the store's identity rather than its display name (renames can no
longer redirect an overwrite onto an unrelated store), imports preserve
archive store ids so re-imports recognize their own stores, and an
overwrite claims the whole store, folders included. The differential
harness's recipe rot is repaired rather than re-measured — most of the
"unrunnable" recipes turned out to be a measurement venue artifact, the
sweep driver gained a self-test and a batch mode whose results survive
the round, and the one remaining red is precisely diagnosed (a stale
oracle mock, not a port bug) and queued as its own small order. Gate
results, versions, and the unification review's findings are in the
round record.

#### `19b4279c` — 2026-08-04 — Repair the build-context fixture builder, its TZ pin, and its seed

_Versions: harness 0.0.396._

Repaired the differential harness's build-context fixture builder, which
had stopped building at all: the blob table that the reference app's
orphan collector now requires was created after character provisioning
rather than before it, and provisioning is itself what first reaches the
collector. Its regeneration recipe also gained the timezone pin it always
needed — without it the recall signature ring baked the host machine's
offset into the generated oracle — and now stages its case and corpus in
a scratch mirror so the recipe runs from a worktree checkout as well as
the main one. The seeded Commonplace Book message in that corpus was
updated to the new announcement wording, so no fixture teaches the old
shape.

#### `1531d941` — 2026-08-04 — Stop teaching models asterisk narration (the 7fe9fe40 re-port)

_Versions: core 0.0.463._

Staff announcements and the tool-execution rules no longer teach models
asterisk-delimited narration. Under a roleplay template that uses a
different narration delimiter, the asterisks in those strings taught the
wrong format outright. Thirteen strings across the Aurora wardrobe
announcements, the Aurora core whisper, the seven Commonplace Book
sections, and the Suparna mail whisper (including the blank-letter
placeholder) are now plain declarative lines: the wording is unchanged,
only the delimiters are gone, and a trailing dash becomes a colon where
the line introduces content. The native tool prompt's first rule now
illustrates itself in unquoted prose instead of quoted asterisk spans;
that block goes into the system prompt on every tool-enabled turn, so it
was the one unconditional source. This also retires a stale wording gap
in that same rule, left over from an earlier upstream change.
A browser walk covers the new template picker end to end: seed a
template, open New Chat, confirm the pre-selection is what the chat would
have gotten, pick the template, create, and read the created chat back to
confirm it carries the id that was on screen.

#### `32df594e` — 2026-08-04 — Pick a roleplay template when creating a chat

_Versions: SPA 0.5.400._

The New Chat form gained a Roleplay Template dropdown, beneath Play As.
Previously the template was decided silently at creation and could only
be seen or changed afterward from the chat's sidebar. The dropdown is
pre-selected with what the chat would have gotten anyway — project
default, then your global default, then No Template — with that option
marked "(default)", and it is hidden entirely when no templates exist.
Adding a character or switching projects re-seeds the default only until
you pick one by hand. The form sends the value it displayed, and omits it
entirely when the reads it depends on failed, so a read error can never
masquerade as a deliberate "no template".

#### `a334b095` — 2026-08-04 — Accept an explicit roleplay template when a chat is created

_No crate versions bumped._

Chat creation now accepts an explicit roleplay template. The create
request carries a tri-state `roleplayTemplateId`: present — including an
explicit `null`, which means "no template" — beats both the project
default and the user's global default; omitted keeps the old default
chain; an id that resolves to nothing is rejected with "Roleplay template
not found". The chat-creation differential's fixture gained three
roleplay templates and both defaults, and five new cases cover the five
resolution arms. The fixture change also gave every pre-existing case a
real template to carry, and that exposed a hole in the differential
itself: the id-normalizing diff could not tell one template id from
another, so two of the new arms passed even with the resolution
deliberately broken. The template id is now compared literally.
Audited every place the code could reach a document store by its display
name, confirming that nothing resolves a character's vault that way. The
one remaining name lookup is the startup repair that re-adopts a vault
whose link was lost, where there is no ID left to look up; it is now
labeled as such in place.

#### `342e8939` — 2026-08-04 — P4.33 arm 2: a document store's identity on import is its ID

_Versions: core 0.0.464, harness 0.0.397._

Import now identifies a document store by its ID rather than its display
name. Overwriting or skipping matches the store the archive actually came
from, so renaming a store — on either side — can no longer redirect an
overwrite onto an unrelated store that happens to wear the name, and an
archive from elsewhere that merely shares a name is created alongside
instead of claiming yours. For that to work, importing a store now keeps
the archive's store ID rather than minting a new one, so re-importing
the same archive updates the store it created the first time instead of
multiplying it; the display name is still made unique when it collides,
and "import as duplicate" still mints a fresh ID. This is a deliberate
difference from the reference app, which still matches by name.

#### `2f7a0be8` — 2026-08-04 — P4.33 arm 1: the import overwrite-clear claims the folders too

_Versions: host 0.0.58._

Fixed a test-suite race in the maintenance-sweep cadence check: it waited for
the terminal-session cleanup and then read the sweep timestamp, which is
written separately just afterward, so a heavily loaded full-suite run could
read it too early. Test-only; no product behavior changed.

Overwriting a document store on import now clears its folders too.
Previously an overwrite replaced every file in the store but left the old
folder tree standing, so folders the archive no longer mentioned became
permanent empty husks, and re-importing the same archive failed to
restore its own folders at all — every one collided with a survivor. The
store is now emptied completely before the archive's tree is written
back, which is what makes an import a faithful round trip. A real export
always carries the store's whole folder tree, scaffolding included, so
nothing is lost by clearing first. This is a deliberate difference from
the reference app, which still keeps the husks.
Recorded the post-fix autonomous-rooms sweep result alongside the
pre-fix one, so the committed evidence shows both states.

#### `3a97952a` — 2026-08-04 — Repair the rotten oracle recipes and pin the ruled failed-call divergence

_No crate versions bumped._

Repaired the differential harness's rotten oracle-regeneration recipes.
Re-measuring the families a previous sweep had written off found that most
of them were never broken — they only fail when the sweep runs from an
agent worktree, which the reference app's test runner ignores. The ones
that were genuinely broken had four distinct causes, each fixed: a recipe
that used a scratch directory it never created, one that copied into a
directory it never made, one whose complete recipe the driver had been
dropping on the floor, and a fixture builder that created a table the
reference app needs *after* the step that needs it. Nine more recipes now
stage their files outside the worktree so where you run them stops
mattering, two stopped pointing at temporary directories that no longer
exist, and four stopped running the entire test suite when they meant to
run one test.

The context-compression differential is green again: it was permanently red
only because v5 deliberately records a log row for failed cheap model calls
where the reference app records nothing, and that ruled difference is now
pinned explicitly — asserted in both directions, so it fails loudly if the
two ever agree again instead of hiding it.

Also fixed the autonomous-rooms oracle, which had been silently spawning a
child process that cannot start under the test runner; its crashes were
intermittently emptying the database mid-run.

#### `52112051` — 2026-08-04 — Fix the sweep driver's four defects and give it a durable batch mode

_No crate versions bumped._

Fixed the differential harness's recipe-sweep driver, which had been
misreporting working recipes as broken. Doc prose that happens to start
with a shell word no longer leaks into the generated script; the
skip detector no longer mistakes an environment variable name ending in
"_SKIP" for a skipped test; recipes naming a temporary reference-app pin
directory (which never survives the round that made it) are now flagged;
and recipes that hand the test runner a path inside an agent worktree —
where the reference app's test runner ignores everything — are flagged
and refused with an explanation instead of failing mysteriously. The
driver also gained a batch mode that writes its results to a file after
every family, so a sweep's per-family findings stop dying in temporary
storage, plus its own self-tests.

#### `f699ffae` — 2026-08-04 — Plan the 7fe9fe40 round: four work orders (P4.D44 ∥ P4.D45 ∥ P4.33 ∥ P4.34)

_Docs-only change._

Planned the next porting round and committed its work orders — four
lanes. Two absorb the reference app's newest drift: the New Chat form
gains its roleplay-template dropdown (pick the template at creation,
with the default chain pre-selected and an explicit "No Template"
honored), and the staff announcement strings plus the tool-execution
rules stop teaching models asterisk-delimited narration (which also
retires a long-stale wording gap in the native-tool prompt). The third
lane is the previously ruled import-overwrite repair, now slotted into
the round. The fourth is a maintenance pass over the differential
harness's regeneration recipes: the sweep driver's known defects, the
mechanically-unrunnable recipe repairs, pins for the deliberately
divergent error-row families, and a durable results artifact so sweep
classifications stop dying with the round. Documentation only — the
implementation is the four work orders.

#### `27219d98` — 2026-08-04 — Rule the import-overwrite escalation and order it as P4.33

_Docs-only change._

Ruled and ordered the import-overwrite repair the last round escalated:
overwriting a document store on import now means overwriting all of it,
folders included; a store's identity for import matching is its ID rather
than its display name (renames can no longer redirect an overwrite onto
an unrelated store, and imports will preserve archive store IDs so
re-imports recognize their own stores); and an audit ensures every
character-vault reference in the code resolves by ID. Documentation only
— the implementation is the new P4.33 work order.

#### `f3aa2d8f` — 2026-08-04 — Close the gate's two late catches: a doc-lint and the convert beat's weak toast locator

_Versions: core 0.0.462, SPA 0.5.399._

Unified the 49769ec4 drift catch-up and store-delete round onto the main
line: four parallel lanes, all closed. Every model request is now bounded
— a provider that accepts a call and never answers is abandoned in
seconds-to-minutes instead of wedging a turn for ten silent minutes
(45s/180s per cheap-task attempt, a 60s ceiling on the memory recap, a
300s default at the transport, and streaming carefully bounded only until
the first byte so a long answer is never cut off mid-generation). The
custom-tool run dialog gained presets: save the current parameter values
under a name into the character's vault, load them back from a dropdown,
reset to defaults — ordinary vault files, visible in the Scriptorium,
riding backup and export. Deleting a document store now takes all of the
store's rows with it in one transaction (the reference app leaks
documents and group links there — deliberate, pinned divergences), and a
startup pass reaps the orphans existing databases have already
accumulated, including the 43 links and 118 folders measured on the real
instance. The document-editing test oracle now runs the reference app's
real chunk-on-write, so the two long-red equivalence checks are green and
edited documents are positively proven chunked for search. The
unification review caught four real defects before they shipped — the
worst a fail-soft gate that would have silently disabled the orphan
repair forever on text-only databases. Gate: 412 Rust test binaries /
1,848 tests / 0 failed with fourteen oracle families regenerated from a
pinned reference checkout and re-run by name; Angular tests 278 files /
3,829 green; the full browser suite 177 passed / 0 failed / 0 skipped
with the new preset walk live.

#### `6a79f0ee` — 2026-08-04 — Repair the six doc-edit families' regeneration recipes (P4.32)

_No crate versions bumped._

Rewrote the regeneration recipes carried in all six document-editing
equivalence checks. The old ones pointed the reference app's test runner
at a path it silently ignores, so a regeneration matched nothing, left
the previous output sitting on disk, and let the check pass against a
stale comparison — which is how the chunk-on-write difference stayed
hidden. Each recipe now stages its case where the runner can see it,
names its target exactly, and was executed start to finish to prove it
works. Documentation inside the tests only.

#### `2e7c7cf0` — 2026-08-04 — Un-mock the doc-edit oracle reindex and prove the chunk pass (P4.32)

_No crate versions bumped._

Repaired the two long-red document-editing equivalence checks. The test
oracle had been silencing the reference app's own chunk-on-write step
along with a separate, still-unported trigger, so the reference looked
like it never indexed an edited document while the port did — a
difference in the test rig, not in either app. The oracle now runs the
real indexing code and seams only the piece the port genuinely omits,
and the checks additionally prove, on named files, that an edited
document really is chunked for search on both sides. All six
document-editing families' regeneration recipes were rewritten to
commands that actually run, since the broken ones are why this sat
unnoticed. Test-harness only; no application behavior changed.

#### `8a52de57` — 2026-08-04 — Rule the doc-edit oracle un-mock and order it as P4.32

_Docs-only change._

Ruled and ordered the repair for the two long-red document-editing
equivalence checks: the test oracle had been silencing the reference
app's own chunk-on-write step, making the reference look like it skipped
indexing when it doesn't. The oracle now runs the real indexing code, so
the comparison also proves edited documents get chunked for search. The
repair runs as a fourth lane alongside the round already in flight.
Checked the new request ceilings against the reference app across
seventy-five comparison suites, and confirmed the change moves the bytes
sent to model providers not at all. The check found one defect in the new
work — two test harnesses ran on a clock that had been switched off — and
a backlog of comparison suites that can no longer be re-run at all, which
is recorded for its own repair. No product behavior changed here.

#### `5c138336` — 2026-08-04 — Put a 60s phase ceiling on the memory recap (P4.D42 unit 3)

_Versions: core 0.0.455._

Put a one-minute ceiling on the whole memory-recap step of a turn. The
recap makes two network calls in a row, each already deadlined; this is
the backstop that keeps a turn from sitting on "Recalling…" no matter
which leg misbehaves. It sits above the per-call budget on purpose — a
recap that is merely slow in two places is still working — and when it
does fire the turn simply continues without the recap, which is optional
flavour rather than something the reply depends on.

#### `6e1a77de` — 2026-08-04 — Give every cheap-LLM attempt a deadline (P4.D42 unit 2)

_Versions: core 0.0.454._

Gave every background model call a deadline. Memory recaps, titling,
compression and extraction now abandon a provider that goes silent after
45 seconds (three minutes for a local model, where slow is not the same
as stalled), and each attempt gets its own fresh deadline. The provider
is handed a hard budget five seconds inside that one, so a stalled
request is closed at the socket rather than left running. An abandoned
call now says so in the log, with the provider, model, task and elapsed
time — the whole reason the original ten-minute stall was so hard to
find is that it said nothing at all.

#### `ccde0e93` — 2026-08-04 — Close dogfood #58's root cause: the store-delete cascade + the orphan reaper

_Versions: web 0.0.57._

Put a ceiling on every provider request. A model call that a provider
accepts and then never answers used to run all the way to the ten-minute
SDK default; without a caller-supplied budget it now fails after five
minutes, and a caller can hand down a shorter budget for one attempt,
which is never retried past. Streaming is bounded differently on
purpose: the ceiling there covers only how long a provider may take to
*start* answering, so a long reply is never cut off mid-sentence.
Deleting a document store now takes its contents with it, in one
all-or-nothing step: the files, folders, links, chunks, document bodies,
image data and the group and project links that pointed at it. Before
this, several of those were left behind with nothing referencing them —
invisible, unreachable, and impossible to clean up — which is what made
restoring a backup report dozens of raw database errors. A file that two
stores share is still spared: only the last store to let go of it takes
the contents down. Startup now also sweeps up the leftovers a database
has already accumulated, and the daily maintenance pass does the same
for servers that run for weeks without restarting.

#### `e4b71a7e` — 2026-08-04 — Measure and pin the import overwrite-clear folder gap, and escalate it

_Versions: harness 0.0.391._

Recorded, with a test that will notice if it ever changes: importing an
archive over an existing document store replaces every file but leaves
the old folders standing, so folders the archive no longer contains
survive forever. The obvious repair -- clear the folders too -- turned
out to delete a character vault's own scaffolding (Outfits, Prompts,
Scenarios, Wardrobe), so the behavior is unchanged pending a decision
about which folders an overwrite may claim.
Added an end-to-end walk for run presets against a real server and a
real vault — save a preset, change the values, deal the preset back,
reset to defaults — and mirrored the reference app's design note for
the feature.

#### `3b8aaecf` — 2026-08-04 — P4.D43 unit 3: the run dialog's Presets section

_Versions: SPA 0.5.396._

The custom-tool run dialog can now keep named presets. Set a tool's
parameters the way you like them, give the arrangement a name, and it
is filed in the running character's vault as an ordinary JSON document
— visible in the Scriptorium, hand-editable, and carried along by
backup and export. A dropdown deals a saved preset back into the form,
and a Reset button returns to the tool's declared defaults. Loading is
forgiving on purpose: a preset saved against an older version of a tool
fills in whatever still applies and leaves the rest alone, rather than
refusing to load. The section only appears where it can do something —
a tool that takes parameters, run as a character with a vault.

#### `095df46e` — 2026-08-04 — P4.D43 unit 2: the tool-presets naming contract, ported verbatim

_No crate versions bumped._

Ported the naming rules for custom-tool presets: a preset is a plain
JSON file in the character's vault, named for the tool and the preset
together, and the name is locked to lowercase letters, digits, dashes
and underscores as it is typed. The reference app's own tests for those
rules came across case for case.

#### `8836356b` — 2026-08-04 — P4.D43 unit 1: the roster listing carries the running character's vault

_No crate versions bumped._

The reference app's custom-tool run presets now work here too, first
half: the composer's run dialog is told which vault belongs to the
character a tool would run as, which is what decides whether the preset
controls appear at all. A character whose vault could not be read says
so plainly rather than going missing, so the dialog can hide the
controls instead of guessing.

#### `9d7c2a34` — 2026-08-04 — Plan the 49769ec4 drift catch-up + store-delete round (P4.D42 ∥ P4.D43 ∥ P4.31)

_Docs-only change._

Planned the next porting round and committed its three work orders: a
re-port of the reference app's new provider-request bounding (so a
stalled model call can no longer wedge a turn for ten minutes), a
re-port of the new custom-tool run presets (named parameter hands saved
into the character's vault), and a repair for the long-standing bug
where deleting a document store left its files, folders, and links
orphaned in the index — plus a startup pass that cleans up the orphans
existing databases have already accumulated. Documentation only; the
work itself runs in the round's three parallel lanes.

#### `57524682` — 2026-08-03 — Record the round: docs, order headers, phase plan, and the baseline move

_Docs-only change._

Unified the hard-link-groups drift and restore-remainder round onto the
main line: four parallel lanes, all closed. Hard links between document
stores are now real in v5 exactly as the reference app just made them —
a file linked into two places stays one file when either side is edited,
copies stay independent, orphaned content left behind by old edits is
collected on every write and swept once at startup, links survive export
and import as links, and the command-line file listing counts deliberate
links rather than coincidentally identical bytes. Along the way the port
found the reference app's own sibling-reindex half of that feature is
dead code (its fix is queued upstream) plus a crash it can hit on old
databases. The backup family closed out the latest walk's findings:
wiping data now clears conversation annotations, restoring an archive
with orphaned rows skips them with plain-English sentences instead of
fifty raw database errors, a backup that cannot collect a file now says
so at backup time, and background jobs hold still while a restore or
delete-all is rewriting the shelves. A new migration-vintage test
fixture — the reference app's real migration chain replayed from
nothing — makes these vintage-shaped faults provable for the first
time. The remaining screens owed notification messages got them
(ninety-two sentences, byte-for-byte), and every conversation now
renders with its own roleplay template's patterns instead of the
defaults. The unification review caught and fixed a twin-write-path gap
that would have left hard-linked siblings stale on the main editing
surfaces, sealed the round's deliberate two-vintage oracle seam, and
pinned every new deliberate divergence in both directions.

#### `9283008b` — 2026-08-03 — Dump the link group, and regenerate what the collection moves

_Versions: harness 0.0.386._

Editing a document in a store no longer leaves its previous contents behind
for good. Every edit stores the new version under a new entry and repoints the
file at it; the entry it left was never collected, so a long-lived instance
accumulated dozens of abandoned document and image bodies. They are now
collected as they are abandoned, and any backlog is cleared at startup.

#### `3536fee3` — 2026-08-03 — Count deliberate links in the CLI, not identical bytes

_Versions: cli 0.0.5._

The command line no longer reports a file as linked into three dozen places
when nothing was linked at all. Its links column counted every file that
happened to store identical bytes — an empty file, a shared boilerplate header
— because that is all a shared store row means. It now counts links actually
made, and lists only those under each file. An instance carried forward from
an older version reports one link per file rather than refusing the listing.

#### `8cea7fcb` — 2026-08-03 — Carry hard-link groups through export and import

_Versions: core 0.0.449, harness 0.0.385._

Backups and exports now carry hard links, so a file linked into two stores
comes back linked rather than as two documents that drift apart on the next
edit. Importing the same archive twice cannot merge the two copies into one
file: the link is re-made from scratch each time, never reused by name.

#### `045c5dfd` — 2026-08-03 — Bind a hard-link group when linking, never when copying

_Versions: core 0.0.448, harness 0.0.384._

Linking a file into a second store and copying it there are now different
things, as they read. A link makes one file with two paths: edit either and
both change. A copy is its own file from the moment it is written, even though
it shares storage with the original until then.

#### `1c7aad74` — 2026-08-03 — Keep hard-linked document-store files linked on write

_Versions: core 0.0.447, harness 0.0.383._

A file hard-linked into a second document store now stays one file. Editing
either side used to fork them apart silently: the write moved the edited
location onto fresh contents while the other kept the old ones, so the second
place served the previous revision indefinitely — to the file browser, to
search, and to characters. A write through any member of a deliberate link now
moves every member, and rebuilds the search index for each. Unlinking the
second-to-last member leaves an ordinary independent file behind. Contents
nothing points at any more are collected as they are abandoned, not left to
accumulate.

#### `8d263517` — 2026-08-03 — Record deliberate hard links in the document-store schema

_Versions: core 0.0.446._

Started making hard links between document stores real. A fresh instance now
carries the field that records a deliberate link, and an instance carried
forward from an older version gains it on startup. Startup also clears out
stored file contents that nothing points at any more — every edit to a
document in a store leaves its previous contents behind, and until now they
were never collected, so a long-lived instance accumulated dozens of them.
Backup and Restore now raise the same notifications the original does — a
confirmation when a backup downloads or a restore finishes, and the failure
message as a notification as well as on the dialog.

#### `8635012d` — 2026-08-03 — Measure the vintage gap instead of reasoning about it

_Versions: harness 0.0.384._

Checked, against a database built the way real ones are built, that
restoring a backup never asks for a column the database in front of it does not
have. It does not — and the check found that the difference between a fresh
database and a carried-forward one is wider than assumed: forty columns across
four tables. One case is still not handled, and is recorded with a test that
fails as soon as it is.

#### `a215870a` — 2026-08-03 — Hold the pump still while the shelves are being emptied

_Versions: core 0.0.448._

Background jobs are now held still while a restore or a delete-all runs.
They used to keep claiming work and writing to tables the operation was in the
middle of emptying and refilling. They start again afterwards however the
operation ends, including when it fails — unless you had stopped them yourself,
in which case they stay stopped.

#### `d81e5546` — 2026-08-03 — Refuse to insert a row whose parent the backup never carried

_Versions: core 0.0.447, SPA 0.5.381._

Restoring a backup no longer fails on document-store rows whose store is
gone. A store deleted at some point in the past can leave its file links and
folders behind; the backup copies them out faithfully, and putting them back
used to fail with a bare database error — once per row, naming a filename and
nothing else. Restore now checks that a row's parent is in the backup before
trying, and says which rows it skipped and why. Nothing about how backups are
written has changed, so archives you already have benefit too.

Creating a backup now tells you when it could not read one of your files.
Those files were skipped in silence, and the only sign was an error at restore
time, possibly months later. The names now come back with the backup, and the
skips are written to the server log as well.

#### `b6539324` — 2026-08-03 — Sweep the margin notes away with everything else

_Versions: core 0.0.446, harness 0.0.383._

Deleting all your data, and restoring a backup over the top of an
existing library, now also clear the notes characters leave on individual
messages. That table was never cleared, so the old notes stayed behind and a
restore could fail trying to put the backed-up ones back — on databases carried
forward from older versions, once per note. The original leaves the same rows
behind; this version does not.
A hands-on walk now covers the whole path end to end: a template with its
own narration marks is made, hung on a conversation, and the line already on
screen re-dresses itself without a reload — then goes plain again when the
template is taken away.

#### `9fdae5b5` — 2026-08-03 — Render each conversation with its own roleplay template

_Versions: SPA 0.5.382._

Conversations now render their messages using their own roleplay template's
patterns. Until now nothing ever read the template a conversation was set to,
so every message everywhere was drawn with the built-in marks — a template
that dressed narration or dialogue differently had no effect on anything you
could see. Its patterns now reach settled messages, the reply being typed as
it streams, a character's shown reasoning, and an opened announcement alike,
and they follow a template swapped mid-conversation without a reload. A
template that has been deleted, or that supplies no patterns of its own,
quietly falls back to the built-in marks, as the original does.

#### `23a427dd` — 2026-08-03 — Teach the render parity corpus about roleplay templates

_Versions: SPA 0.5.381._

Widened the test that holds message rendering to the original's, so it now
also covers conversations whose roleplay template supplies its own patterns
rather than the built-in ones. Eleven new cases were captured from the
original app itself, covering custom patterns, custom dialogue marks, and
both of the ways a template can fall back to the defaults. Every one of the
forty existing cases came back unchanged. No visible behavior changed here;
this is the measuring stick the next change is checked against.
Added an automated test that walks three real actions end to end — saving
a character, allowing any character into a project, and creating a
folder in Files — and checks that each one shows its confirmation
message.

#### `2433acb0` — 2026-08-03 — Close out Tier 1: API key associations + tag-create toasts (P4.29 unit 12)

_Versions: SPA 0.5.389._

Gave the API key dialog a confirmation message for each connection
profile it automatically links to a newly created key, matching the
original. Gave the character tag editor an error message when creating a
brand-new tag fails.

#### `f7abb286` — 2026-08-03 — Give New Chat its v4 toasts, retiring the inline banner (P4.29 unit 10)

_Versions: SPA 0.5.388._

Gave the New Chat screen confirmation and error messages for every
outcome: loading its data, every validation refusal (no character picked,
an autonomous room needing two LLM characters and no user, a missing
connection profile, no LLM-controlled character at all), and both
creating a chat and creating an autonomous room. An inline banner v5 had
added for all of these is gone, matching the original, which shows every
one of them as a passing message only.

#### `d8006093` — 2026-08-03 — Give the general Files page its v4 toasts (P4.29 unit 9)

_Versions: SPA 0.5.387._

Gave the general Files page confirmation and error messages for every
action: loading the list, deleting a file (both the plain and the
"delete anyway" paths), syncing the filesystem, and both orphan-cleanup
actions. All of these had shown a plain browser alert box on failure and
nothing at all on success. Also added a confirmation for downloading a
file, and for creating a folder (including trying to move it to another
project) -- an inline error banner v5 had added for the create-folder
and move-to-project dialogs is gone, matching the original.

#### `484f9d52` — 2026-08-03 — Give the project detail screen its full toast set (P4.29 unit 7)

_Versions: SPA 0.5.386._

Gave the project detail page confirmation and error messages across all
of its settings: the header save, Allow Any Character, removing a
character from the roster, agent mode, answer confirmation, the default
roleplay template, avatar generation, Lantern image announcements, the
story-background display mode, the default image profile, and removing a
chat from the project. None of these showed anything before. An inline
error banner v5 had added for four of these actions is gone, matching
the original, which shows none of them inline.

#### `30624987` — 2026-08-03 — Give the group editor its save/add/remove-member toasts (P4.29 unit 5)

_Versions: SPA 0.5.385._

Gave the group editor confirmation and error messages for saving a
group's details and for adding or removing a member, matching the
original -- which shows none of these inline, only as a passing message.
An inline error banner v5 had added for these four actions is gone.

#### `8b0943f0` — 2026-08-03 — Give template replace/restore and header toggles their toasts (P4.29 unit 4)

_Versions: SPA 0.5.384._

Gave the character detail page confirmation and error messages for
converting between character and NPC, and for the favorite/Carina/
controlled-by toggles in its header, plus messages for the Details tab's
four name-to-template and template-to-name replacement buttons.

#### `67f2b206` — 2026-08-03 — Port DescriptionsTab's physical-description toasts (P4.29 unit 3)

_Versions: SPA 0.5.383._

Gave the character editor's Appearance tab a Clear button for the physical
description, matching the original, and confirmation and error messages
for saving or clearing it — including refusing to save without a name.

#### `a98dfe4f` — 2026-08-03 — Give character save and avatar changes their v4 toasts (P4.29 unit 2)

_Versions: SPA 0.5.382._

Gave the character editor confirmation and error messages for saving a
character and for setting or clearing its avatar, matching the original.
The avatar picker's inline error banner is gone now that failures raise a
message the same way the original always did.

#### `8cb4a16e` — 2026-08-03 — Give the Characters roster its v4 notification toasts (P4.29 unit 1)

_Versions: SPA 0.5.381._

Gave the Characters page confirmation and error messages for actions that
previously finished silently: deleting a character (naming how many chats,
images, and memories went with it), toggling the favorite star, toggling
whether Carina will answer for a character, toggling who controls a
character, importing from SillyTavern, and resetting the built-in
characters. Two dialogs also lost inline error banners the original never
had, now that the same failures raise a toast instead.

#### `ceeec560` — 2026-08-03 — Plan the hard-link-groups drift + restore-remainder round (four orders)

_Docs-only change._

Planned the next porting round (documentation only): four parallel work
orders. The reference app moved one commit — making hard links between
document stores real, so a file linked into two places stays one file —
and one order absorbs all of it, including the cleanup of content rows
that older writes left behind. The other three cover the backup/restore
faults the latest hands-on walk surfaced (annotations that survive a
wipe, links and files missing from archives, jobs running mid-restore)
together with tolerance for databases carried forward from older
versions; the remaining screens still owed their notification messages;
and making chat messages render with the conversation's own template
patterns instead of the defaults.

#### `08284bba` — 2026-08-03 — Let settings writes tolerate a column the database lacks

_Versions: core 0.0.445._

Fixed a fault that could leave Quilltap unusable, showing an error on every
screen with no way to recover from inside the app. Settings are stored in a
table that, on a database carried forward from an older version, can be
missing a column this version knows about. Reading such a table already
coped; writing to it did not. Because Quilltap creates a settings row
whenever it finds none — which is exactly the state a restore leaves behind
if it could not put the old one back — the app would try that write on every
page load and fail. Writes now leave out columns the database in front of
them does not have, which is what the original does too. No database is
altered in the process.

#### `3f5dc700` — 2026-08-03 — Port v4's list-indent pre-pass, reversing the (a)-edge ruling

_Versions: SPA 0.5.380._

Stopped the editor from destroying nested lists in documents written
elsewhere. A list whose sub-items were indented a little less than the
parent's text begins — common in hand-written and Obsidian markdown, and
easy to produce under a numbered item like `20.`, whose text starts four
columns in — was read as two separate lists rather than one nested one.
Opening such a document and saving it wrote the flattened version back, so
the nesting was lost for good rather than merely displayed wrongly. In one
real case the sub-points stopped belonging to their numbered item
altogether. Indentation is now resolved from the shape of the document, as
the original does, and a document still saves at whatever indent width it
was written with. Found by scanning real documents: thirty instances across
five files.

#### `fdaa42d0` — 2026-08-03 — Drop the groups section's unused error-alert import

_Versions: SPA 0.5.379._

Removed an unused import from the Groups section of the Characters page. It
had been declaring an error-alert component it never displayed, which the
compiler flagged on every build. Nothing on the page changes: the original
also shows no error banner there, so a groups list that fails to load reads
as an empty one in both.

#### `66bc0605` — 2026-08-02 — Close the stray space before the may-write sentence's full stop (#52)

_Versions: SPA 0.5.378._

Closed a stray gap before the full stop in the run dialog's note about what a
tool may write.

#### `1bef3aad` — 2026-08-02 — Stop DOM events from impersonating output payloads (dogfood #51)

_Versions: SPA 0.5.377._

Fixed two ways the app confused a browser event for a piece of data. Running
a custom tool left its written-out parameters filled with a machine
placeholder the next time the tool was opened, because closing a field was
being mistaken for a change to what it held. The same mistake, found while
chasing the first, meant that **copying selected text out of a message
replaced what you copied with the word "undefined"** and claimed the message
had been copied — so anything you had lined up on the clipboard was lost.
Copying from a conversation now does what it says, and tool parameters keep
what you typed.

#### `8307136e` — 2026-08-02 — Restore the Project library button to the document picker (dogfood #50)

_Versions: SPA 0.5.376._

Restored the Project library button to the Open Document window. A
conversation that belongs to a project could not reach that project's own
files from the document picker at all — and turning on "Look everywhere"
did not help, because every project's store appeared there except the one
the conversation lives in. The store was being deliberately set aside for
a button that had never been built. It is there now, opens the project's
files, and stays put when the "Look everywhere" switch is toggled.

#### `bc9f51ab` — 2026-08-02 — Record dogfood finding #49: a side-effect counter cannot bootstrap

_Docs-only change._

Recorded a limitation of custom-tool side effects, found while trying them
on real data: a counter cannot start itself. An effect that adds one to a
stored value is skipped whenever that value does not exist yet, and the
only thing that would create it is the effect being skipped. The guard
forms an author reaches for are not available either — the expression
language has arithmetic and text joining, but no logical operators and no
defaulting, and a condition cannot ask whether a stored value is present.
Seed the value once in the state editor and the increment works from then
on. This matches the original app exactly, so the repair belongs there
first; it is queued with the other post-release improvements.

#### `18646d82` — 2026-08-02 — Rule the P4.D40 list-indent edge: the divergence stands, with a committed check

_Versions: SPA 0.5.375._

Ruled the one place the rewritten editor deliberately disagrees with the
original app, and committed the check that keeps the ruling honest. A list
item indented two spaces under a numbered parent is read as a sub-list by
the old editor and as a separate list by the new one, which follows the
Markdown standard its parser implements. The new behavior stands: the old
app's rule was a workaround for a since-fixed bug of its own, it never
writes that shape itself, and chasing it would mean rewriting every
document on open. Because the disagreement costs the nesting on the next
save rather than only on screen, the ruling is conditional on evidence — a
scanner now ships that finds the shape in a body of documents. It reports
nothing across the documents stored as plain files; the documents held in
the encrypted stores need the passphrase and are queued for a hands-on
pass, and anything it finds there reopens the decision with the repair
already identified.

#### `14d6655c` — 2026-08-01 — Fix the two e2e beats the merged branch broke, and the docs for the round

_No crate versions bumped._

Unified the round that catches this project up with two days of changes to
the app it is a rewrite of — ten commits, four of them landing on parts
already rewritten, absorbed across six parallel workstreams.

Custom tools can now record side effects. A tool may carry a short list of
conditional writes that run after the roll: into the scene's persistent
state — chat, project, group, or the general store, chosen by finding
where the value already lives — or onto the rolling character's own record
sheet. A written value can be a literal or a small expression (arithmetic,
joined text, references to the roll and its parameters), evaluated by a
closed parser with no identifiers and no function calls. A bad expression
is refused when the tool is loaded; one that fails at roll time skips that
single write and never sinks the roll. Tools can also carry a per-run chip
label, so a roll can name itself in the transcript, and a result whose
message opens with a list or a heading now renders properly instead of
being crushed onto one line. The workbench gained a card for authoring all
of this and shows what a trial run would write without writing it.

Announcements can now be whispered. The Insert Announcement dialog has a
"Who hears it" section; check nobody and the announcement is public as
before, check one or more characters and only they receive it in their
context. The collapsed chip says who it went to. Two related fixes came
with it: an announcement posted as a named character now tells the model
who spoke — previously it arrived anonymous, and a character in testing
attributed a private aside to entirely the wrong member of the staff — and
one high-volume class of internal whisper stopped ignoring the All
Whispers toggle. Whisper labels in all six bundled themes were too faint to
read and now meet the accessibility contrast bar in both light and dark.

Characters are dressed from all three wardrobes at the start of a chat.
Previously only a character's own wardrobe was read, so shared and
project-wide garments were invisible: default outfits living in a shared
wardrobe never got worn, an outfit assembled from shared pieces resolved to
nothing, and a character whose wardrobe was entirely shared was never even
offered to the model. Opening a chat with several characters now consults
for them at the same time rather than one after another (one stalled
provider used to hold up an entire cast), a stalled request gives up after
a minute and falls back to defaults, and "wearing nothing on purpose" is
now distinguishable from "the request failed".

The editor stopped reflowing nested lists. A document indented with four
spaces, three spaces, or tabs now comes back out the way it went in
instead of being rewritten to two on the first edit. Tab and Shift-Tab
indent and outdent list items, with matching toolbar buttons that also
work in raw-source mode, and Tab still moves focus everywhere else.

The review before merging caught a dependency block that a conflict
resolution had silently deleted — the code that needed it was landing
without it — and six recorded instructions for regenerating comparison data
that could no longer be followed, three of them pointing at scratch
directories that no longer exist. All were repaired; none was a fault in
the ported code. Gate: formatting, both lint configurations, a release
build, 409 test binaries and 1,798 tests with every one of the round's 42
comparison suites confirmed to have actually run, 3,639 unit tests across
268 files, a production bundle, and the full end-to-end suite — 172 of
172, no skips.

#### `1125cac3` — 2026-08-01 — Rebuild the fixture and prove where the writes land (P4.D35 unit 6)

_Versions: core 0.0.439, harness 0.0.377._

Rebuilt the custom-tool test instance so the side effects can actually be
measured rather than described. It now has a project alongside its conversation,
group and shared stores, and two tools that write: one whose single roll touches
all four stores plus the rolling character's own sheet, and one that keeps its
odds secret while still writing. Both test harnesses now read every store back
after a run and compare it to the reference app's, so "the write landed in the
right place" is a measurement, not a claim about a record. The Workbench's dry
run does the same in reverse: every bench case checks that nothing was written
at all, which is the only way to prove a preview stays a preview. A deliberate
break that silently skipped one store — leaving the record still claiming
success — was caught by exactly that check.

#### `051f6946` — 2026-08-01 — Wire the effects through both entrances (P4.D35 unit 5)

_Versions: core 0.0.438, harness 0.0.376._

Connected the custom-tool side effects to the two places a tool can be rolled —
a character reaching for it and the operator reaching for it from the composer.
The writes land before the result is announced, so if the announcement fails the
writes still stand; and an operator roll made as nobody edits nobody's character
sheet. The result bubble now puts the roll's name on its own line with the
message as a separate paragraph below, so an outcome that opens with a list, a
heading, a quotation or a code block renders as what its author wrote instead of
running into the heading. Messages posted before this change keep their old
one-line shape — a record is not rewritten. A tool that writes now says which
places it writes to, in the tool listing the model reads and in the panel a
person reads, without disclosing what it writes or when. Tools that keep their
odds secret keep their consequences secret too.

#### `4f68c41c` — 2026-08-01 — Port the side-effect applier (P4.D35 unit 4)

_Versions: core 0.0.437._

Added the part that actually performs a custom tool's side effects. A write
lands in the store where its key already lives — the conversation first, then
the project, then the group, then the shared store — and a brand-new key goes
to the conversation, the most local place with the least reach. Each store is
written at most once no matter how many effects touch it, in a fixed order, and
each write is caught on its own: if one store refuses, only its own effects are
dropped and the rest still land. A run nobody made writes to nobody's character
sheet. The user-only underscore keys are refused a second time here, behind the
refusal that already happens when the file is read. Nothing here can fail a
roll that already happened.

#### `5a80c6f5` — 2026-08-01 — Resolve chipLabel and effects in the execution core (P4.D35 unit 3)

_Versions: core 0.0.436, harness 0.0.375._

Taught a custom-tool roll to work out its chip label and its side effects.
The label is rendered once, after the outcome is chosen, so it can quote
anything the outcome message can — including the answer of a mid-roll consult.
The effects are worked out but not yet written anywhere: each one records
either the value it would write and where, or the reason it was passed over —
a condition that did not hold, a formula that could not be worked out, a
reference that resolved to nothing. Nothing sinks a roll. Passed-over effects
keep their place in the list rather than being dropped, so the audit trail
shows what the tool declared, not just what fired. The table audit ignores
both, as it should: an audit that wrote ten thousand times would not be an
audit. Verified by 12 new differential cases and three deliberate breakages.

#### `702a5c70` — 2026-08-01 — Port the effects + chipLabel definition schema (P4.D35 unit 2)

_Versions: core 0.0.435._

Taught the custom-tool file format to declare side effects and a per-run chip
label. A tool may now carry an `effects` list — up to sixteen conditional
writes, each naming where it writes and what — and an optional label template
for the result chip. Both are checked when the file is read, not when someone
rolls: a target that names nowhere writable, a formula that does not parse, a
reference to a parameter the tool never declared, and the underscore-guarded
state keys that belong to the user alone are all refused up front, in the
reference app's exact words. The check that walks an outcome row's subjects is
now shared with effect conditions rather than copied. Verified by growing the
format's differential corpus by 63 cases, with three deliberate breakages
proving the new key positions and the order rules are really being compared.

#### `a36e2413` — 2026-08-01 — Port the effect-expression grammar (P4.D35 unit 1)

_Versions: core 0.0.434, harness 0.0.374._

Ported the expression grammar that a custom tool's side effects will use to
compute what they write: arithmetic, string joining, parentheses, literals,
and the same `{{...}}` placeholders an outcome message already understands.
There are no identifiers and no function calls, so there is nothing to call
and nothing to reach beyond the roll's own values. A typo in a formula is
reported when the tool file is read, not when someone rolls. Every error
sentence is reproduced word for word against the reference app, including
the character positions it reports, which count the way text is counted in a
browser rather than in bytes. Verified with a new 125-case differential and
three deliberate breakages to prove it catches them.
Walked the new tool-authoring controls in a real browser: filling in a
chip label, adding a side effect, watching the editor object to a bare
phrase where a quoted one was meant, and confirming that the exact file
the editor would save carries both new pieces. Two further walks — the
test bench's dry-run display, and a labelled roll's chip in the
conversation — are written and waiting on the matching server work, each
held behind a named switch rather than a guess about what the server can
do. Also confirmed that the recorded reference results the browser is
checked against are unchanged by this release of the reference app, so
the existing checks still hold.

#### `4684ca88` — 2026-08-01 — Say what a tool may write, and label each run by its own chip label

_Versions: SPA 0.5.362._

Told the reader what a tool may change, and named each run by its own
label. The Run Tool dialog now lists the places a tool may write when it
runs — kept as a separate sentence from the list of things it quotes,
since consulting a number and changing it are different claims — and shows
that panel even for a tool that quotes nothing at all but does change
something. In the conversation, a roll's chip is now labelled by the
run's own rendered label when it has one, falling back to the tool's title
and then its name. Rosters from an older server, which say nothing about
writes, are read as writing nothing rather than as an error.

#### `f2d11272` — 2026-08-01 — Add the chip label and Side Effects card to the Workbench

_Versions: SPA 0.5.361._

Gave the tool workbench the two controls the new format needs: a chip-label
field beside the title, and a Side Effects card between the form and the
outcome table where a roll's consequences are written down. A condition too
rich for the card to draw is shown as a read-only badge rather than a
control, so it cannot be flattened by accident. The test bench's miniature
result now matches the real one — the heading stands over its own paragraph,
so an outcome that opens with a list or a quote reads properly — and it
lists what each effect would write, alongside a plain statement that the
bench computes those effects and never applies them.

#### `19c938e4` — 2026-08-01 — Carry chip labels and side effects through the draft layer

_Versions: SPA 0.5.360._

Extended the tool builder's working model to hold a chip label and a list
of side effects, and to hand them back unchanged when the file is saved
again. A condition richer than the form can draw — the kind a person writes
by hand in the raw file — is carried through untouched behind a read-only
badge rather than quietly dropped. Both new fields are audited as they are
typed: an unknown placeholder in the label is a gentle warning, while a
target that names nowhere writable, or a formula that will not parse, is a
blocking error said beside the row that caused it.

#### `7b743e85` — 2026-08-01 — Add chipLabel and effects to the browser schema twin

_Versions: SPA 0.5.359._

Taught the browser's copy of the custom-tool format about the two new
things a tool may declare: a per-run label for its result chip, and a list
of side effects — small writes a roll records once the dice have settled.
The browser now accepts, refuses, and explains exactly what the server
does for the same file, including the trap that catches every author once:
a bare phrase where a quoted one was meant. Checked against eighty-one
verdicts recorded from the reference app, compared down to the wording and
the order of the keys.

#### `b64d5745` — 2026-08-01 — Name announcement speakers in LLM context (P4.D37 unit 2)

_No crate versions bumped._

Taught the browser to read the new side-effect expression language for
itself. Custom tools can now carry small formulas — arithmetic, text
joining, parentheses, and references to the run's own numbers — and the
editor has to judge them the moment they are typed rather than waiting for
the server to refuse the file. The browser's copy of that reader reproduces
the reference app's wording exactly, down to the sentence an author sees
when they forget to quote a phrase, because the same complaint is also
raised on the server about the same file and the two must not disagree.
Verified against ninety-six results recorded from the reference app itself,
plus its own test suite ported case for case.
Announcements now tell the model who is speaking. When an announcement is
posted as a named off-scene character or under a free-text name, that name
was painted on the bubble but never reached the model, so the announcement
arrived as anonymous prose and the model guessed the speaker — badly, and
then carried the mistake into the scene. Each such announcement is now
prefixed with its speaker's name in the model's transcript only; nothing
stored, exported, or displayed changes, and an announcer whose name cannot
be resolved passes through unnamed rather than being given an invented one.
Re-running a turn does not stack the tag. The same pass also closes the
last gap where a private aside could reach a character it was not
addressed to: the single-character transcript builder applied its privacy
check to tool results alone, and now applies it to every kind of message.
Verified against the reference app's real code with a new twenty-six-case
comparison plus eleven regenerated end-to-end comparisons.

#### `3b51a0ba` — 2026-08-01 — Whisper an ad-hoc announcement to named participants (P4.D37 unit 1)

_Versions: host 0.0.55._

Manual announcements can now be whispered to specific people in a
conversation instead of always being spoken to the room. The composer's
audience is re-checked on the server against who is actually in the scene
right now, so an announcement can never be addressed to someone who has
left or to a stranger from another conversation — that is refused outright,
because the alternative is filing a note nobody could ever be shown.
Naming nobody posts publicly exactly as before. When the announcement is
rewritten in a character's own voice, the character is now told privately
who is listening and given those names in place of the room's roster, so a
private aside is not pitched like a declaration to a crowd. The rehearsal
itself stays forgiving: a stale name there simply drops out of the audience
rather than failing the preview. Verified against the reference app's real
code across fifty-six compared cases, with deliberate breakages proving
each new check can actually fail.
The Insert Announcement dialog can now whisper. A new "Who hears it" section
lists the chat's current participants with a checkbox each — leave every box
unchecked and the announcement posts publicly exactly as before, or check one
or more names and it becomes a whisper only those participants' contexts
include. The collapsed chip and the composer both say who a whisper went to,
and changing who hears it invalidates any in-character rewrite already on
screen, since a private aside reads differently than a public proclamation.

#### `9c1e5135` — 2026-08-01 — Show the whisper audience on both announcement render sites

_No crate versions bumped._

A whispered announcement's collapsed chip and its full-row Staff header now
say who it went to, the same "to Alice, Bob" tag either way, and the chip
wears the whisper's border color so a private aside is distinguishable from
a public proclamation before it's ever expanded.

#### `f4e14624` — 2026-08-01 — Port the overheard-whisper dim (tier 2)

_No crate versions bumped._

Turning on "All Whispers" now visibly dims the whispers that weren't meant
for the operator, instead of showing every one of them at full strength.
The dim keeps the whisper's border and label legible, so the operator can
still tell it's a whisper — just not one addressed to them.

#### `6ec31319` — 2026-08-01 — Re-port whisper-visibility.ts: kind-narrowing + operator-authored announcements

_No crate versions bumped._

The operator now keeps seeing their own private asides no matter who they're
signed as, and Prospero's busiest whisper — telling one character which group
shelves it may read — finally stays hidden behind the All Whispers toggle
instead of leaking into the main flow. The visibility rule that decides which
whispers the operator sees even with the toggle off is now keyed on the exact
kind of whisper (a private dice roll, a private tool run and its errors)
rather than on the sender alone, and a whisper posted through Insert
Announcement is now recognized as the operator's own writing and never
hidden or dimmed, whichever Staff member or invented name it's signed as. A
legacy row with no recorded kind keeps its old behavior rather than
disappearing from a view the operator is used to.

#### `f4ff60d4` — 2026-08-01 — Prove the tri-tier dressing on the create path, composite and all

_No crate versions bumped._

The "whispered to X" label on a whisper bubble, and the matching audience tag
on a collapsed announcement chip, were unreadable in light mode in five of six
bundled themes (Art Deco measured a near-invisible 1.07:1 contrast) and failed
the accessibility bar in two themes in dark mode as well. All six bundled
themes now clear the WCAG AA 4.5:1 contrast requirement for this label in both
light and dark mode, matching the reference app's fix.
Proved the whole thing end to end on the path that creates a chat, against the
reference app. Two characters open a project chat wearing the shared shirt, the
project's sash, and — for the one who owns clothes of her own — her jacket
layered on top in the right order. And when the model picks a shared outfit set
whose pieces also live in the shared collection, the pieces now appear: the
creation dialog lists the coat and the boots by name where before it showed an
empty outfit, because nothing had gone looking for parts the wearer doesn't own.

#### `bc742826` — 2026-08-01 — Measure the merged-pool consult and the deliberate-nudity contract

_No crate versions bumped._

Proved the outfit-choosing model now sees the shared wardrobe, against the
reference app. A character who owns no clothes is now asked at all — before,
the empty candidate list meant the model was skipped entirely and they were
dressed in nothing — and a character with a wardrobe of their own can pick a
shared garment without it being thrown away. The whole conversation sent to the
model is compared word for word, including the new paragraph asking it to say
when nakedness is the point, and the three ways an empty answer can arrive
(flagged, unflagged, and flagged with the wrong kind of value) each land where
they should.

#### `3f461207` — 2026-08-01 — Give chat-cast a shared wardrobe tier to measure against

_No crate versions bumped._

Proved the tri-tier dressing against the reference app on the add-a-character
path: a character who owns no clothes at all now joins wearing the shared
collection, shared and personal defaults layer in the order they were created,
and a character who keeps their own unmarked copy of a shared garment goes
without it. The test fixture had no shared wardrobe in it before, so none of
this was measurable there.

#### `740bf69f` — 2026-08-01 — Sort layered defaults and say what a default garment promises

_No crate versions bumped._

Layered default outfits now appear in the same order in the composer's preview
and in the chat that opens, and the "default" checkbox says what it will
actually do. Marking a garment default in a project store or in the shared
collection puts it on every character who can reach it, which is a much larger
promise than "part of this character's default outfit" — so the label now
depends on where the item is headed.

#### `08269619` — 2026-08-01 — Dress characters from all three tiers at chat start

_No crate versions bumped._

Characters are now dressed from all three wardrobe tiers when a chat opens.
Until now the two paths that dress everyone at the start — creating a chat and
adding someone to one — looked only in that character's own vault, so a
character whose clothes all live in a project's shared store or in Quilltap
General opened the scene wearing nothing, and the model that picks outfits was
never even asked. Defaults from different tiers now layer together in the same
slot, oldest first, and a character can still opt out of a shared default by
keeping their own copy of it unmarked.

Three problems from the first live run are fixed with it. Everyone's outfit is
now decided at the same time rather than one after another — a chat that took
two and a half minutes to open was waiting on a single slow reply — while the
saving still happens one character at a time, because it has to. A stalled
provider is given one minute and then the character wears their usual clothes.
And "naked on purpose" is no longer indistinguishable from "the model gave up":
the model is asked to say which it means, and only a plainly stated choice
counts.

#### `348a43fc` — 2026-08-01 — Hydrate a shared composite's components before unpacking it

_No crate versions bumped._

Fixed an outfit made of shared parts resolving to nothing. A composite garment
— a livery, a dress uniform, anything that bundles other pieces — used to be
unpacked using only the wearer's own wardrobe, so when the bundle lived in a
project store or Quilltap General its pieces were invisible and the whole
outfit quietly came out empty. The pieces are now fetched a level at a time,
one lookup per level, as deep as the unpacking itself will go. Listing a
character's wardrobe now asks the shared pool for its answer instead of
repeating the merge by hand.

#### `c72b9562` — 2026-08-01 — Add toolbar and source-mode list indent/outdent buttons

_No crate versions bumped._

Gave the wardrobe one place to answer "what can this character actually wear?".
The answer folds the shared tiers — Quilltap General plus any of the project's
own stores — underneath the character's own vault, so a character's private
copy of a house garment hides the shared one, and an archived copy simply
disappears, letting the shared item show through again. The merge deliberately
happens on the full lists and only then drops archived items, because a
character's opt-out from a shared default is itself an item, and filtering
first would throw it away before it could do its job. The equip path, which
needs archived garments so it can still name what someone is wearing, does not
use this and says so.
Added list indent/outdent buttons to the editor's formatting toolbar and its
raw-Markdown source mode (P4.D40, tier 1 unit 6, closing the sub-list
indentation feature). The two new buttons carry the reference app's exact
arrow glyphs, titles, and keyboard-shortcut hints, sit between the list/
blockquote buttons and the code-block toggle exactly where the reference
app places them, and dispatch the same sink/lift commands Tab already
reaches in rich-text mode. In source mode they read the raw text's own
indentation width — never a fixed default — and shift only the selected
list lines by it, matching the reference app's toolbar behavior line for
line. The button-sizing CSS rule (arrow glyphs at a slightly larger size to
match the text-label buttons) is copied over unchanged.

#### `3162067c` — 2026-08-01 — Add Tab/Shift-Tab list indent/outdent, confined to list items

_No crate versions bumped._

Gave the editor Tab/Shift+Tab list indent and outdent, confined to list items
(P4.D40, tier 1 unit 3) — Markdown has no way to represent an indented
paragraph, so pressing Tab anywhere else still moves focus like a normal
textbox. A ProseMirror caret always knows its full ancestor chain, so unlike
the reference app the check needed no special case for an ambiguous caret
position; it is simply true whenever either end of the selection sits inside
a list item. Tab nests the item under its previous sibling and Shift+Tab
lifts it back out, both dispatched through the standard ProseMirror list
commands, and — matching the reference app — pressing Tab on an item that
has nothing to nest under (the first item in a list) still counts as
handled rather than falling through to focus-move. Verified with real DOM
keydown events dispatched at the live editor component, including a
4-space document proving the indent still preserves that document's own
nesting width rather than the two-space default. Also fixes a latent gap
this uncovered: jsdom implements `Element.getClientRects`/
`getBoundingClientRect` but not `Range`'s, so any editor command that asks
the browser to scroll a change into view (already true of the pre-existing
Shift+Enter line break, just never previously exercised by a test) threw
under the test environment; a small shared stub answers both the way a
real browser's Range would.

#### `41933c12` — 2026-08-01 — Wire the unit-preserving post-pass into the markdown bridge

_No crate versions bumped._

Wired list-indentation unit memory into the editor's Markdown bridge
(P4.D40, tier 1 unit 2). Loading a document now records the nesting width it
was written with, and every save re-indents to that same width instead of
rewriting the whole list to a fixed two spaces — a two-space file stays
two-space, a four-space file stays four-space, a single edit no longer
reflows every nested line. The round-trip gate grew five entries covering
four-space and tab-nested bullets, nested and wide ordered lists, and
three-space bullets, plus two editor-integration specs proving the unit
survives an in-editor Tab/toolbar indent, not just a plain parse-then-save.
One divergence from the reference app is recorded and pinned rather than
silently accepted: a two-column list item nested under a numbered parent
(`1. a` / `  - b`) nests in the reference app but parses as two separate
lists here, because Markdown's own CommonMark rules require at least three
columns to continue a numbered item — the reference app's own output never
produces this shape, so only hand-written input can hit it, and a human
ruling is requested before building anything more invasive.

#### `3ccd3ebd` — 2026-08-01 — Port v4's list-indentation pure functions with a tier-1 differential

_No crate versions bumped._

Ported the reference app's list-indentation math for the editor's Markdown
bridge (P4.D40, tier 1 unit 1). The reference app resolves how deeply a
Markdown list item is nested from the document's own structure, not by
assuming every file uses two-space indentation, and re-indents its output to
match whichever spacing the file was written with instead of rewriting the
whole thing to one fixed width. New `editor/list-indentation.ts` ports the
pure detect/apply/source-line-shift functions verbatim; a new tier-1
differential drives the reference app's real module directly and byte-compares
every case against the port (24 cases, zero reimplemented). Not yet wired into
the live editor — that's the next commit.

#### `8d783e05` — 2026-08-01 — Plan the c4d4b0de drift catch-up round: six work orders (P4.D35-P4.D40)

_Docs-only change._

Planned the next porting round: the reference app moved ten commits in two
days, and six work orders now cover absorbing all of it — the Pascal
custom-tool side-effects feature (server and Workbench halves), whispered
manual announcements with speaker attribution in model context (server and
Salon halves), dressing characters from all three wardrobe tiers at chat
start, and the editor's sub-list indentation contract — plus theme
contrast corrections and two commits dispositioned as not applicable to
this codebase with evidence recorded. Planning surveys ran against both
codebases first, so each order carries verified file-and-line starting
points, the shared wire contract, and the exact oracle families it must
regenerate. Documents only; no application code changed.

#### `d65b4804` — 2026-08-01 — Close the deflake follow-up: the pgrep rule, the round record, and the docs

_Docs-only change._

Unified that deflake onto the main line. The review before merging checked
each of the change's stated reasons against the actual application code
rather than taking them on trust; all of them held, including the finding
that one race the tests now step around is a faithful copy of how the old
app behaves and so was correctly left alone rather than "fixed" into a
difference. One correction was made: a new comment described a field as
being sent as empty when the feature is off, when in fact the field is
omitted entirely and is a decimal number when on — which is why the tests
match it loosely on purpose. Left uncorrected, the next person to tidy
that match would have reintroduced the very flake this work removed.
Gate: formatting, both lint configurations, a release build, the full
workspace test run, the unit suite (3,210), a production bundle, and the
complete end-to-end suite — 168 of 168, no skips.

