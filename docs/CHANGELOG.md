# Quilltap Changelog

## Recent Changes

P4.d7 tier 2: a migration-vintage regression proves the boot hook
(ensure_mount_index_tables) repairs a mount-index built with the legacy
case-sensitive indexes and planted folder/link/store-name collisions — the
colliding rows get suffixed and the unique NOCASE indexes replace the legacy
ones. Closes the P4.d7 lane (the 0a0419f5 case-insensitive mount-namespace
re-port).

P4.d7 unit 5: case-insensitive store-name uniqueness (v4 0a0419f5).
next_unique_mount_point_name matches case-insensitively and trimmed;
ensure_character_vault suffixes ` (N)` so same-named characters get distinct
vaults; and the mount-points create/rename routes return 409 with a verbatim
message ("A document store named ... already exists. Names are matched without
regard to case ...") when a name clashes with a peer's in any casing (the
rename check excludes the store itself). The mount_points_routes differential
gains create-clash / rename-clash / rename-case-only-self cases, and the
mount_case_repair differential gains a naming-leaf check, both against v4's real
code. The importDocumentStores dedup has no v5 counterpart (v5's import subset
excludes mount points) — a documented absence.

P4.d7 unit 4: case-only renames + the copy guard (v4 0a0419f5). copyFile's
same-path guard moves BEFORE the force-overwrite branch and compares
case-insensitively, so force-copying a file onto a case-variant of its own path
no longer deletes the source; moveFile / moveFolder skip the destination-exists
check for a same-mount case-only rename. In the database store,
moveDatabaseDocument allows a case-only document rename, and moveDatabaseFolder
allows the source itself at the destination and canonicalises the destination
under the parent's stored casing (descendants + links prefix-rewrite from the
source's stored path). The doc-edit move-file handler gains the same case-only
skip. New mount_case_moves differential proves all of it against v4's real
copyFile / moveFile / moveDatabaseDocument / moveDatabaseFolder.

P4.d7 unit 3: case-insensitive, case-preserving folder + link resolution
(v4 0a0419f5). ensure_link_folder_id and ensure_folder_path walk folder
segments with an exact-match-then-NOCASE query and continue under the folder's
STORED casing, returning the canonical directory; the database-store link
upserts (link_document_content, link_blob_content) probe the existing row
case-insensitively on the canonical path and update it IN PLACE keeping its
stored casing; link_filesystem_file matches case-insensitively but ADOPTS the
scanned casing (the filesystem is the source of truth); doc_mount_folders
find_by_mount_point_and_path gains the exact-then-lowercased-scan fallback.
Differentials: doc_mount_file_links_tier2 gains two case-variant ops, and a new
mount_case_resolution differential proves the blob (case-preserving) and
filesystem (case-adopting) sites against v4's real repo.

P4.d7 unit 2: the mount-index case-collision repair pass
(db/mount_index_case_repair.rs, porting v4's mount-index-case-repair.ts). At
boot it renames the newer of any case-colliding folder siblings, file links, or
document-store names with a ` (N)` suffix (subtrees and links repaired along),
and trust-or-recreates a genuine unique COLLATE NOCASE index — replacing the
legacy case-sensitive one or a tampered same-named stand-in, and catching
non-ASCII collisions the ASCII-only NOCASE index tolerates. Wired into the boot
hook services::builtin_mounts::ensure_mount_index_tables (v4's three per-repo
init call sites collapse to v5's single once-per-startup hook). A no-op on a
fresh instance; existing pre-0a0419f5 instances are migrated here. New tier-2
differential (mount_case_repair_equivalence, 7 planted-collision cases) against
v4's real repair over a real in-memory SQLite.

P4.d7 unit 1: re-dump fresh_schema.json from v4 d68638b4. The two mount-index
unique indexes become COLLATE NOCASE under new names
(idx_doc_mount_folders_mp_parent_name_nocase,
idx_doc_mount_file_links_mp_path_nocase), and the characters table gains a
metadata TEXT column (the per-character fact sheet). Fresh instances get all
three from provisioning replay; the metadata column is vault-managed and stays
inert in the DB (character.metadata hydrates from the vault's metadata.json,
which lane p4.6az owns). Provisioning, builtin-mounts, and builtin-templates
equivalence regenerated at d68638b4 and green.
P4.6az lane gate fix (lane AZ): renumber the `vault_character_write` module-doc
write list (the interleaved `1b.` metadata step became a plain `2.`, shifting the
rest) to satisfy clippy's `doc_lazy_continuation` / `doc_overindented_list_items`
lints. Doc-only.

P4.6az Tier-2/Tier-3 deferrals (lane AZ): the lazy-backfill wiring (seeding
`metadata.json` into already-linked/adopted vaults) is deferred to the unifier —
its two hook sites are inside lane D7's shared region of `character_vault.rs`, so
the two one-line hooks are documented verbatim for the unifier to place. Until
wired, behavior is unaffected (the read path hydrates `{}` regardless); only file
discoverability for a pre-feature adopted vault waits. Named Tier-3 absences: the
startup backfill sweep (v5 has no such subsystem) and `qtap-export.schema.json`
(still no counterpart; the gap grew).

P4.6az unit 5 (lane AZ): the PUT arm + reader enumeration + qtap-import
threading. `character_update`'s whitelist now NAMES `metadata` (Zod strips
unknown keys, so an unnamed field would be silently dropped before the write
overlay); `metadata` joins `MANAGED_FIELDS` (routes a metadata-only PUT to the
vault, strips it from every slim write). The hydrated read echoes (`character_get`,
`merge_update_echo`) carry it automatically from unit 2; qtap-import threads it
through `CharacterVaultWriteInput` (deserializing the whole character), so a
round-trip no longer drops the fact sheet. The ST export is unchanged (v4 never
put metadata on ST cards). Extended `characters_update_tier2` (two metadata ops:
set + whole-object replace) and `characters_mutations` (a metadata PUT proving a
named key survives + the echo carries it while an unknown key is stripped).

P4.6az unit 4 (lane AZ): seed `metadata.json` in the character-vault scaffold
and add `ensure_character_metadata_file`. The scaffold now seeds an empty `{}`
fact sheet (for discoverability — the file manager is the only editing surface),
alongside `properties.json` / `physical-prompts.json`; the new
existence-check-only ensure fn (checks the file EXISTS, never parses — a
fat-fingered sheet is never "healed" into an empty one) is the seed the deferred
lazy backfill reaches for. Regenerated every scaffold-touched differential at
d68638b4 (scaffold, create, provision, adopt, arrays, physical, summary-mirror)
plus the new `ensure_character_metadata_file` family (3 states: absent → seeds
`{}` + true; valid → untouched; invalid → untouched, not healed).

P4.6az unit 3 (lane AZ): the guarded write projection + whole-object patch
routing for `metadata.json`. `CharacterVaultWriteInput` gains an optional
`metadata`, and the create-time projection writes `metadata.json` ONLY when
`metadata != null` — the anti-clobber invariant: metadata has no DB column, so
a raw row's absent metadata must not overwrite a real fact sheet with `{}`. The
update write-overlay routes a `metadata` patch as a whole-object REPLACE (not a
key-merge — one field owns one file, so a merge would make deleting a key
impossible), stripping it from the DB patch. New `metadata_vault_roundtrip`
differential (7 ops: replace-drops-omitted-keys, null/empty → `{}`,
absent-untouched, all-value-types, replace-overwrites-unparseable) + extended
`vault_character_write` with a metadata op.

P4.6az unit 2 (lane AZ): hydrate `character.metadata` from the vault's
`metadata.json`. Added `metadata.json` (second, beside the `properties.json`
keystone) to `SINGLE_FILE_OVERLAY_PATHS`, and `hydrate_one` now yields at least
`{}` for every vault-linked character — an absent or unparseable file both
hydrate `{}`, never hollowing the character. This also lifts the
`character_stats` health denominator (`characterFilesTotal`) 8 → 9, tracking
v4's own drift. Regenerated the read-echo differentials at d68638b4 (vault
read overlay + the four characters read/action/subresource families).

P4.6az unit 1 (lane AZ, d68638b4 drift round): port `parse_vault_metadata`,
the fail-soft parser for a character vault's `metadata.json` fact sheet.
Returns the JSON object verbatim, or null for anything that is not an object
(invalid JSON, a top-level array, a bare scalar) — the caller hydrates `{}`.
Unlike properties.json, metadata is not a keystone: a malformed file never
hollows the character. Extended the tier-1 vault-json-parsers differential
with 10 metadata cases (24 → 34).
P4.6ay units 5 + 6 (the Pascal writer + Prospero's custom-tool-error). New
services/pascal_writer.rs: build_pascal_result_content (the `🎲 **{title}** —
{message}` body, opaque == content — with the flourishes gone there is no persona
to strip) and post_pascal_result (the synthetic ASSISTANT message, systemSender
'pascal' / systemKind 'custom-tool-result', empty whisper target → null, errors →
None). services/prospero_notifications.rs gains build_custom_tool_error_content /
_opaque_content, the reason normalization (trim, strip trailing `[.\s]`, fall back
to 'the table would not deal'), and post_prospero_custom_tool_error (systemKind
'custom-tool-error', whisper to the caller alone) — failures are authored by
Prospero, never Pascal. PascalMetaIn gains the d68638b4 metadataTested field (in
v4 schema order, between outcomeIndex and invokedBy). New tier-1 differential
pascal_writers_equivalence (8 pascal bodies + 10 error bodies) against v4's real
builders.

P4.6ay unit 2 (Pascal custom-tool discovery / roster). New pascal/roster.rs:
is_root_tool_file (a nested Tools/sub/x.tool.json is rejected — definitions are a
flat root-level convention), load_definitions (the read → parse → validate → dedup
core; a broken file becomes an error entry, never a throw; a same-mount duplicate
name is rejected), ordered_mounts (same-tier ties break by mount id
lexicographically), resolve_roster_from_pool (nearest tier wins; a `disabled`
tombstone suppresses a name at its tier and every farther one; MAX_ROSTER_SIZE
drop list), and the live wrappers resolve_custom_tool_roster / load_tools_from_mount
(database + filesystem/obsidian stores). No caching, ever — the roster is
re-resolved per call. New tier-2 differential pascal_roster_equivalence: 20
scenarios driven through v4's real resolveCustomToolRoster with the pool + store
mocked (v4's own discovery-test template), replayed through the v5 core. The
malformed-JSON reason is compared by prefix (a serde-vs-V8 parser-message seam);
every other reason byte-exact.

P4.6ay unit 11 (the d68638b4 metadata re-port). Port the character-metadata
subject into the custom-tool definition format and execution core. WhenObject
gains an optional `metadata` record: keys are any non-empty string (the user's
own vocabulary in metadata.json, not the identifier grammar `params` takes),
values are the same comparators as `params`. Load-time validation is
deliberately shallow — only that a `$param` operand resolves to a declared
parameter — since a metadata key names something on a character the file has
never seen. Execution adds fail-soft metadata matching (an absent, non-primitive,
or wrong-typed key declines rather than throws; a `$param` operand still throws
if unresolved), records only the winning row's tested primitive keys as
`metadataTested`, and renders the `{{metadata.key}}` template family (a missing
or non-primitive key leaves the placeholder verbatim). `execute_custom_tool`
takes a `metadata` override. The run_custom tool description grew v4's metadata
sentence. Differentials extended and re-greened against fresh d68638b4 oracles:
pascal definition (105 defs + 10 titles), pascal execution (146 rows),
tool_definitions (58 tools byte-exact).
P4.6ba unit 3 (SPA): the composer custom-tools popup + run flow. A bespoke
composer toolbar button (wand) opens an anchored, upward popup (the
speaker-selector idiom) that resolves Pascal's roster fresh on every open and
gates its own visibility on that roster (a runnable tool OR a broken file). Each
tool expands into the standalone `qt-custom-tool-params-form` (declared-type
coercion, reusable by P4.6bb's bench) plus a "Roll privately" toggle; running
dispatches `chatCustomToolRun` with coerced params, closes, and refetches the
chat. Odds/outcome tables are never shown; broken-file badges render the loader's
verbatim reason. The Workbench entries (open-on/new/repair) are omitted (P4.6bb).

P4.6ba unit 2 (SPA): the Pascal bubble. A Pascal roll outcome is carved out of
the announcement-chip collapse (`isAnnouncementChip`) and renders as its own
full message row with a static header bar (dot · Pascal · tool title · time) —
`toolTitle ?? tool` for legacy rows — above the normal markdown body; the
character author header/avatar is suppressed since Pascal has no participant.

P4.6ba unit 1 (SPA): the Pascal in-chat wire mirror + stream + labels. Added
`pascalMeta` to the message DTO, `'pascal'` to the systemSender union, the
`pascalResult` stream frame field, and the `chatCustomToolsList` /
`chatCustomToolRun` dispatch verbs with their `CustomToolListing` /
`CustomToolLoadError` / roster + run data shapes (§4, consumed structurally).
The stream reducer surfaces a mid-turn `pascalResult` deduped by id like
carina/host. System-message labels name Pascal, add the custom-tool-result /
-error kinds, and label a roll outcome by its tool title (`toolTitle ?? tool`).

Plan the d68638b4 drift catch-up round (docs only). v4 moved ten commits past
the e3593f75 baseline and the predicted tripwire fired: the custom-tools +
character-metadata feature landed, alongside the case-insensitive mount
namespace fix and Pascal's Workbench. Four work orders committed: p4.d7 (the
NOCASE namespace re-port: renamed COLLATE NOCASE indexes via a fresh-schema
re-dump, the boot repair pass, case-preserving ops, the 409 name arms), the
P4.6ay re-baseline addendum (resume at the new unit 11 — the metadata delta to
the landed units plus the changed run_custom description — through unit 12,
the Workbench server surface), p4.6az (the metadata.json vault surface:
fail-soft hydration, the guarded write projection, whole-object patch, the
scaffold seed), and p4.6ba (the in-chat Pascal SPA plus the All-Whispers
toggle re-port). The Workbench SPA is deliberately deferred to P4.6bb next
round. Drift docs mirrored under docs/v4; round record in the status log;
CLAUDE.md's in-flight warning resolved. No code changes, no version bumps.

Unify the P4.d5 + P4.6ay resumed lanes. P4.d5 (dice modifier, lenient numbers,
the spine RNG call sites, the 58-tool catalog) is complete and closed; P4.6ay
lands its first two Pascal units (the custom-tool definition format and the
execution core) with units 2 and 4-9 still open on the order. The run_custom
catalog entry ships ahead of its handler: verified inert (keyed lookups only,
nothing offers it to a model), which unblocks the Pascal lane's byte-identity
test when it resumes. All nine affected oracle families were regenerated from
a v4 worktree pinned at the e3593f75 baseline after the live v4 checkout went
dirty mid-unification with in-flight feature work; the two new v4 feature docs
are mirrored under docs/v4/developer/features/. Gate: fmt, clippy on both
feature sets, release build, 332 suites / 1392 tests / 0 failed with the nine
differentials re-run by name (zero skips), ng test 1247, ng build, and the
full Playwright suite 65/65. Versions: core 0.0.246, harness 0.0.219.

P4.6ay unit 3: the Pascal custom-tool execution core
(quilltap-core::pascal::custom_tools). resolve_params, coerce_param (Pascal's
OWN coercion, deliberately not the tool layer's llmNumber), clamp,
resolve_roll_field, crypto_uniform (6 bytes / 2^48, through the shared dice
module's RandomBytes seam), roll_range (multiply then offset then round;
min === max short-circuits without drawing), matches_when with the value / roll
/ params subjects and $param operands, format_value, render_template, and
execute_custom_tool (the dice branch's raw === value === total, and the
visibility override).

Also quilltap-core::pascal::js_value: the JS coercions the core runs on —
Number(), String(), JSON.stringify(), Number::toString, and toPrecision. These
are general JS primitives that belong in jsnum.rs; they sit under pascal/ for
now because jsnum.rs is in no lane's Ownership table this round while a sibling
lane ports llmNumber next door. Lifting them is a named follow-up.

The differential injects the byte source on both sides: the jest oracle mocks
crypto.randomBytes over a scripted pool and reports the cursor; the Rust side
replays the same pool through FixedBytes and asserts consumed() matches. That
pins what inspection cannot — a degenerate range draws 0 bytes where a real one
draws 6, and the dice path's rejection sampling draws exactly what v4 draws.
117 rows green, including formatValue over 1e21/1e-7/2.675, Math.round's
halves-toward-positive-infinity, and Number()/String() on strings, booleans,
arrays, and objects.


P4.6ay unit 1: the Pascal custom-tool definition format
(quilltap-core::pascal::custom_tool_types). The constants, the schema tree with
a33ac8b8's strictness (nested objects strict, top level tolerant so v2 keys
stay reserved), the load-time rules (the mandatory trailing catch-all, the
rejected earlier catch-all, $param reference and operand-type checks),
display_title, format_definition_issues, and collect_unknown_keys.

The differential pins the FULL rejection sentence, Zod's own built-ins
included, not just the accept/reject verdict: loadToolsFromMount stores that
sentence as a load error's reason and the custom-tools GET route returns it
verbatim, so it is payload the route differential has to match anyway. That
forced a faithful port of three Zod rules — issues from checks are
"continuable" while type errors abort, checks are skipped once an aborting
issue exists, and a union with exactly one non-aborted branch hoists that
branch instead of wrapping it. 102 corpus rows green against v4's real schema.

Recorded divergence (JSON layer, not the schema): a definition containing an
overflow literal such as {"gt": 1e999} parses to Infinity in v4 and is rejected
by the schema's finite() check, while serde_json rejects it at the parse, so v5
rejects the same file at read_tool_file. Both refuse it; only the reason string
differs. finite() is carried faithfully regardless.

P4.d5 fix: the tool catalog's completeness count follows the run_custom entry
(57 to 58), and the lookup test pins run_custom's camelCase key against its
snake_case function name. The full-workspace gate caught this; the unit-3
differential could not, since it diffs the catalog against v4 rather than
against a hardcoded count.

P4.d5 tier 2: leniency coverage for the web_search tool. The search_web
differential now drives a quoted maxResults through the real handler and proves
the converted number reaches the outgoing request as a number rather than a
quoted string, plus that `true` is still refused rather than silently becoming
1. The order's worry that the v5 case pinned v4's pre-drift "no coercion under
Zod" assertion was checked and is unfounded: it never mirrored that assertion.

P4.d5 unit 5: the two chat-spine RNG call sites. Typing "2d20+5" in a Salon —
or a character writing it in a reply — rolled a plain 2d20 and dropped the +5,
then persisted a TOOL row whose arguments didn't record the modifier at all.
Both auto-detect sites (the user-message path in the orchestrator and the
response path in the message finalizer) now pass the detected modifier into the
roll and into the saved row. The modifier is always recorded, zero included, so
even a coin flip's row carries `modifier: 0` — matching what v4 writes.
Verified by the regenerated orchestrator and finalizer end-to-end differentials,
with a new case rolling 3d6+2 through the whole spine.

P4.d5 unit 4: the prose dice scanner honors modifiers. Typing "roll 3d6+2" in
a Salon detected a 3d6 and dropped the +2 on the floor. The detector no longer
carries its own dice regex; it drives the shared scanner, so the modifier comes
through and the 2-1000 sides / 1-100 rolls bounds are enforced in one place —
skipping out-of-bounds notation rather than clamping it, which means "3d6+1001"
now yields no roll at all rather than a quiet 3d6. Spacing still disambiguates:
"2d6-1" carries its -1, while "2d6 - 1 apple" stays a plain 2d6 near an
unrelated subtraction. Coin and bottle detection are untouched, as is the
detection order. Verified by the regenerated 64-row differential.

P4.d5 unit 3: the rng modifier. The tool now understands `3d6+2` — a flat
amount added to the dice total — and reports it: every successful dice roll's
output carries `modifier` and `total` alongside `sum`, where an unmodified roll
reads `modifier: 0, total: sum`. That changes the serialized output of every
dice case, not only modified ones. The rolled line gains the arithmetic
("Rolled 3d6+2: [4, 2, 6] + 2 = **14** total"); the unmodified wording is
byte-for-byte what it always was. Quoted numbers now reach the roller, so
{"type": "6"} rolls a real d6 instead of returning "Unknown RNG type", and
{"modifier": "2"} adds two rather than concatenating a digit. The published rng
tool definition gains `modifier` so the model is told it exists, and the
catalogue picks up ten tools' key-order changes from v4's llmNumber refactor.
The run_custom definition joins the catalogue for the Pascal lane; it is a
keyed lookup, so it offers the model nothing until that lane's handler lands.
Verified by a 37-case differential (up from 14) asserting output JSON, the
formatted string, and exact random-byte consumption, plus 58 byte-exact tool
definitions.

P4.d5 unit 2 (rider): image generation now honors a quoted `count`. A
model that asked for `{"count": "3"}` got one image instead of three.

P4.d5 unit 2: lenient numbers (`llmNumber`) across the tool surface. Models
routinely quote their numbers — `{"type": "6"}` rather than `{"type": 6}` —
and v5 rejected every one of them, so the roll never happened and the
character was told their sensible request was invalid. The new
`tools::llm_number` seam converts a numeric-looking string before
validation, and nothing else: `true`, `null`, and `[]` are still refused
rather than quietly becoming 1 and 0, because a rejected call beats a wrong
one. Bounds still apply after conversion. The conversion is JS `Number()`,
not `parseInt` and not Rust's float parser — so `"0x10"` is 16, `"5px"` is
refused, and `"inf"` is refused even though Rust's parser would take it.
Wired into every guarded field's validator AND its read: v4's Zod parse
replaces the value, so a site that validated leniently and then read the raw
argument would have silently used its default instead. Verified by a new
73-row differential against v4's real `llmNumber`.

The D24 blocker is resolved and the drift round is re-planned (2026-07-17).
The fix went into v4 itself, per the ruling: quilltap-server e3593f75
(4.8.0-dev.62) makes all 57 tool validators return safeParse's parsed data,
every handler call site read that parse, and the doc-edit dispatcher route its
26 cases through the validators with a raw-input fallback — so llmNumber's
quoted-number leniency now actually reaches the handlers. Drift-checked: one
commit past a33ac8b8, scoped entirely to the tool-input family, published
tool-definition bytes unchanged. The two open work orders carry addenda
re-pinning the round baseline to e3593f75: P4.d5 resumes at unit 2 (its held
unit-2 branch code is now correct as written) and P4.6ay resumes at unit 1
(only unit 4's validator shape changed). The previously undeclared
executor.rs image_generation.count collision is now a declared shared region
in both Ownership tables. D24's record in phase-4.md carries the resolution.

The P4.d5 ∥ P4.d6 ∥ P4.6ay round PARTIALLY UNIFIED on main (2026-07-17).
P4.d6 CLOSED; P4.d5 and P4.6ay stay OPEN, both for good reasons recorded in
their status headers.

Landed: the whole help-docs sync drift (the slug, the read-once sync, the
guarded prune, the divergence trigger and embedding enqueue); v4 4.8.0's two
new columns (chat_messages.pascalMeta, chat_settings.customTools) adopted per
D23 by re-dumping fresh_schema.json from v4's live generateDDL; and the shared
dice module (pascal::dice), whose primitives were moved rather than rewritten
so the byte-stream differential survived intact.

The unification wire: v5's help_settings tool is a second, independent reader
of the chat settings bag and was missing customTools, which v4 added to
help-settings-handler.ts in 61ec90bd. Lane B predicted the red differential on
exactly that key and named it as lane C's; lane C's column landed everywhere
except there. Only both lanes on one branch could show it.

Held back deliberately. The custom-tools quoted-number work stopped on a
finding: v4's tool validators are boolean type guards that discard the parsed
data, so the handler reads the raw string and llmNumber never takes effect —
{"type":"6"} still fails, and {"modifier":"2"} makes a d6 roll report a total
of "42". The ruling is to fix v4 first and then port the fixed behavior, so
the rng modifier, the prose detector, and the lenient-number seam stay on
their branch. The run_custom tool definition is withheld with them: publishing
it without its handler would offer the model a tool that answers "Unknown
tool".

Also recorded: v5 opening a v4 instance older than 4.8.0 cannot read or write
messages at all, rather than merely lacking the new columns. Migrate a dogfood
copy to 4.8.0 before pointing v5 at it.

Gate: fmt clean; clippy -D warnings clean on both feature sets; release build
clean; cargo test --workspace 329 suites / 1376 tests / 0 failed, with every
affected oracle regenerated fresh from v4 at a33ac8b8 and each new differential
re-run by name with zero skips; ng test 128 files / 1247; ng build clean; full
Playwright 65/65, zero skips. Final versions: core 0.0.238, harness 0.0.213,
host 0.0.19, web 0.0.25, quilltap-tauri 0.0.3, SPA 0.5.134.

P4.d5 unit 1: the shared dice module (`quilltap-core::pascal::dice`), the
port of v4's new `lib/pascal/dice.ts`. One parser, one roller, one source
of randomness for the rng tool, the prose scanner, and (next round)
Pascal's custom tools. Adds `NdS±M` notation: two deliberately different
regexes (the prose scanner forbids whitespace around the sign, so
"2d6 - 1 apple" stays a plain 2d6; the strict parser allows it), bounds
that skip rather than clamp, and roll/format helpers that carry the
modifier v4's old prose regex dropped on the floor. The roller primitives
(`secure_random_int` / `roll_dice` / `flip_coin`) and the injected byte
seam MOVED here unchanged, mirroring v4's own move — `tools::rng`
re-exports the seam, so both spellings resolve. Verified by a new 75-row
tier-1 differential against v4's real `dice.ts`, including byte-stream
parity per roll.
P4.d6 close-out: recorded why v4's second help-docs bug — the unregistered
embedding column that silently dropped every help doc from listings — is not
a v5 bug and cannot become one. v5 has no blob-column registry to forget to
populate: each repository writes explicit SQL and converts embeddings at the
binding site, so the JSON-text shape v4 was accidentally minting has nowhere
to come from. The note now sits in the help-docs repository header, where a
porter reading v4's fix will meet it. The read-side recovery for genuinely old
rows stays; v4's every-boot repair is refused as inapplicable rather than
deferred.

P4.d6 unit 4: help docs added to the repo after the first sync now actually
reach the database (v4 6c59b1ca). The sync only ran when help_docs was
completely empty, and that is the only trigger outside a full embedding
reindex, so a doc shipped later stayed invisible in the Guide forever —
eleven of them, in v4's case. It now also syncs when the files on disk and
the rows in the database disagree, in either direction: a file with no row,
or a row whose file is gone. Both directions come out of one directory scan,
which reads no file contents, and the deleted direction is what makes the
prune reachable at all — a deletion alone would otherwise never start a sync.

Newly synced docs are also queued for embedding now (v4 551f090b), so a new
doc becomes searchable instead of merely listed. The sync itself stays
deliberately queue-free: a full reindex re-embeds everything and batch-inserts
its jobs, so queueing there would race it.

Verified against v4's real ensureHelpDocsSynced across five scenarios — an
empty table, disk and database in agreement, a doc added, a doc deleted, and
no embedding profile configured — diffing both the resulting documents and the
queued jobs.

P4.d6 unit 3: help docs whose Markdown file has been deleted are now pruned
from the database along with their embedding status, instead of lingering in
the Guide forever (v4 551f090b). The sync result reports a `deleted` count.

The prune is guarded so it can only ever remove rows the walk proves absent:
a missing help/ directory, or one with no Markdown at all, syncs nothing and
prunes nothing. One boundary is worth stating plainly, because v4's comment
and v4's code disagree and the port follows the code: a help/ containing only
empty Markdown files walks non-empty, so the guard does not fire, and every
row is pruned — the table empties. v4 does exactly this, confirmed against its
real code rather than assumed, and a three-scenario differential pins all
three outcomes so the boundary cannot drift unnoticed.

P4.d6 unit 2: the help-doc sync now reads its table once and indexes by
path, replacing a findByPath per file — roughly 115 queries on every sync
(v4 551f090b). The prune landing next needs every row anyway, so the one
read serves both. Behavior is unchanged, which is the point: the existing
sync differential stays green untouched.

P4.d6 unit 1: ported the help-document slug (v4's `helpDocSlug`), the
path-derived identifier used outside the database, where the primary key is
a UUID that changes whenever a doc is re-created. v4 promoted it out of the
sync module in d6e74145; v5 had skipped it as dead code, which it was at the
old baseline and no longer is, so that rationale is replaced rather than
appended to. The port maps UTF-16 code units, not chars: v4's character regex
carries no `/u` flag, so one non-BMP character is two dashes, not one — a
40-case tier-1 differential against v4's real module pins it. The slug's
consumers stay unported and are named as such: v4's help-search handler drops
`slug` from its output, so adding the field to v5's tool-wire shape would
diverge from v4 rather than match it.
P4.6ay unit 10: adopt v4 4.8.0's two new columns — `chat_messages.pascalMeta`
(the persisted custom-tool roll record) and `chat_settings.customTools` (the
feature toggle, default on). `fresh_schema.json` re-dumped from v4's live
generateDDL at a33ac8b8; the chat-settings seed, the settings read/write paths,
the message write/read marshaling, and the salon read pass-through all carry the
new columns. A pre-4.8.0 instance lacks `customTools`; the read now supplies
v4's Zod default (true) instead of erroring. The committed test fixtures are
brought forward by v4's own migration SQL (new script:
`harness/oracle/fixtures/migrate-fixtures-pascal-columns.ts`). The migration
runner itself is not ported. Verified by the provisioning differential against a
freshly regenerated schema, the v4-reads-v5 cross-compat check, and the
chat-settings + chats-messages write/read/ops differentials.

The P4.d5 ∥ P4.d6 ∥ P4.6ay round PLANNED (2026-07-16, work orders only —
no code). A drift check found v4 moved 8 commits past the oracle baseline
(02865bdb → a33ac8b8): 106 files, ~8,000 insertions, two schema
migrations. The predicted Pascal drift landed as code. Already-ported v5
code is wrong until this round lands: the dice modifier is dropped, so
"3d6+2" rolls as 3d6 and persists the wrong number; every dice roll's
output JSON now differs (v4 emits modifier/total on all of them); 18
tools reject numeric arguments a model quoted, which v4 now accepts; and
the help-doc sync is missing its divergence trigger, prune, and embedding
enqueue. Three lanes: P4.d5 (the shared dice module, the rng modifier,
lenient numbers across 28 fields, the two chat-spine call sites), P4.d6
(the help-doc sync drift plus the slug promotion), and P4.6ay (the whole
Pascal custom-pseudo-tools server surface — the definition format, roster
resolution, the eval-free execution core, the run_custom tool, the Pascal
writer, and the route). The Pascal SPA is deliberately held for the next
round.

New locked decision D23 (phase-4.md): when v4's schema moves, v5 adopts
the columns by re-dumping fresh_schema.json from v4's live generateDDL;
the migration runner stays deferred. v4 added chat_messages.pascalMeta and
chat_settings.customTools in 4.8.0, so "the schema does not change during
the port" — which assumed a stationary v4 — becomes "v5 never changes the
schema unilaterally; it follows v4's, and only via a re-dump."

Two v4 changes that look like drift and are not, recorded so they are not
rediscovered: v4's new "always show messages with a systemSender" rule is
client-only, and v5 never ported the whisper toggle, so its context
shaping is unaffected; and v4's embedding blob-registration bug cannot
occur in v5, which has no column registry at all and writes embeddings
through one typed conversion at each binding site. v5 needs no
repair-text-embeddings port as a result.

The P4.6aw ∥ P4.6ax ∥ P4.8 round UNIFIED on main (2026-07-16) — all
three orders CLOSED. Rust riders: the two byte-identical cost-estimator
seams consolidated into one trait/default/host impl (behavior-frozen;
title + carina tier-3 differentials green over fresh oracles, zero
skips); the stale "serde_json sorts keys" rationale retired across 15
files; the depiction-guidelines editor now proactively warns when a
character has no document vault (both appearance tabs, v4's exact
warning). Editor riders: __bold__ boldens on type; qt-markdown-field
gains v4's source-mode toggle (default ON, toolbar buttons operate on
the raw textarea via transforms proven against a 32-row oracle driving
v4's real FormattingToolbar); GFM tables round-trip through the rich
editor with v4's exact lossy semantics (19/20 recorded vectors
byte-match; the 20th pins a pre-existing dialect-wide block-separation
divergence bidirectionally). And the M6 screen-parity review landed
docs/developer/porting/m6-screen-parity.md: every v4 screen and
screen-grade dialog verdict-ed with citations, a 16-item p4.9 backlog,
and the v4 retirement criteria. v4 drifted one docs-only commit
(34746bed, a feature spec — Pascal the Croupier); oracle baseline stays
02865bdb. Gate: fmt/clippy both feature sets/release build clean; cargo
test --workspace 325 suites / 1357 tests / 0 failed (four oracles
regenerated fresh; both lane-B vector files identical to fresh runs);
ng test 128 files / 1247; ng build clean; full Playwright 65/65,
zero skips, run alone. Final versions: core 0.0.232, harness 0.0.209,
host 0.0.19, web 0.0.25, quilltap-tauri 0.0.3, SPA 0.5.134.

P4.8 (the M6 screen-parity review) produced
docs/developer/porting/m6-screen-parity.md: the complete v4-vs-v5
screen-parity checklist, the screen-grade dialog inventory, the
deferral cross-reference, a 16-item prioritized backlog, and the v4
retirement criteria. Every row carries a verdict (PARITY /
DIVERGENCE-DOCUMENTED / MISSING / WON'T-PORT) with citations on both
sides. Four findings corrected the planning seed: v4's tabbed
workspace is its DEFAULT shell, not an experiment (the feature flag
defaults ON and 15 routes redirect into it), which makes it the
largest parity gap and a human decision rather than a mechanical one;
the seed's single LLM-log row is two distinct v4 surfaces (the salon
Inspector panel, ported 1:1, and LLMLogViewerModal, unported); the
per-chat Core Whisper override is a gap no source had recorded; and
three v5 docstrings understate what has landed. Rendered the two
verdicts delegated to this review: the boxed ChatCostSummary variant
and detailed=true are WON'T-PORT (both verified dead in v4), and the
redirect-only aliases plus the /foundry/* deep-links are WON'T-PORT.
Documentation only — no code, fixture, or config changes; no version
bumps.

The rich editor now understands GFM tables, matching v4: typed or pasted
pipe tables become real tables, and they are written back the way v4
writes them -- cells padded to the column width, and the separator always
left-aligned, since neither editor stores per-column alignment. Table
text is literal, so `**bold**` in a cell stays as typed. There is no
insert-table button, because v4 has none. Known gap: tables render
without borders in the editor pending a stylesheet rule. SPA 0.5.133.

Every markdown form field — character prose, memories, scenarios, and the
rest — now has an "Edit markdown source" button in its toolbar, matching
v4, where the toggle is on by default. It swaps the rich editor for a raw
markdown textarea and back, and the formatting buttons keep working on the
raw text while you are in source mode. Switching back does not disturb
what the field reports to the form, exactly as in v4. SPA 0.5.132.

Ported the source-mode text transforms — what a formatting-toolbar button
does to raw markdown text (the groundwork for the editor's source-mode
toggle). Faithful to v4 including two behaviors worth naming: the
heading/list/blockquote buttons ADD their prefix unconditionally rather
than toggling it off (a second H1 click gives "# # title"), and an
ordered list writes "1. " on every line rather than counting up. Proven
byte-for-byte against 32 vectors recorded from v4's real toolbar. SPA
0.5.131.

Typing `__bold__` in the rich editor now boldens on the closing
underscore, matching v4 (whose transformer set includes Lexical's
BOLD_UNDERSCORE). The text normalizes to `**bold**` on serialization —
v4 does the same, because its export dedups bold to the first matching
transformer. As in v4, the rule refuses to fire inside a word
(`a__b__` stays literal), and a lone `*` remains literal roleplay
narration. SPA 0.5.130.

P4.6aw item 3: the depiction-guidelines editor now warns up front when a
character has no document vault, instead of failing after you type. A
vault-less character has nowhere to store depiction guidelines, and until
now v5 let you write the text and press Save before surfacing the server's
refusal ("Character has no document vault to store depiction guidelines").
Matching v4, both appearance tabs (character edit and character view) now
replace the editor and its Save with a warning explaining that the
character must be saved once to provision its vault — and no guidelines
fetch fires for a character that has none. The shared
aesthetic-editor-field's docstring, which claimed the depiction-guidelines
field was unported, is corrected: the field landed as inline markdown
fields on those two tabs, which is where the arm now lives; the component's
own disabledHint input stays unported (no v5 consumer would pass it). SPA
0.5.130.

P4.6aw item 2: retired the stale "serde_json sorts keys" rationale across
quilltap-core (comment-only; no code changed). The claim was true of
serde_json's default BTreeMap-backed Value, but the crate enables the
preserve_order feature, so Value::Object is an IndexMap that keeps
insertion order — the same order v4's JSON.stringify emits. Every comment
citing key-sorting as the REASON for a decision was therefore arguing from
a false premise. Swept 15 files: the typed-struct sites keep their
load-bearing guidance (schema field order stays the convention — the
struct is what declares that order) with only the false justification
corrected, and five sites that described an open-JSON multi-key key-order
divergence as a TRACKED DEFERRED SEAM (seam #5: connection_profiles
parameters, character_plugin_data data, image_profiles parameters,
background_jobs payload, plugin_config config + its merge) now record that
preserve_order closed it — those corpora stay narrow by choice, not by
constraint. Deliberate sorts (cache_prefix_hashes stableStringify,
embedding_blob, core_whisper) are untouched and still correct;
canonicalize's explicit sortKeysDeep is now documented as load-bearing
precisely BECAUSE preserve_order means Value won't sort on its own. core
0.0.232.

P4.6aw item 1: consolidated the two byte-identical cost-estimator seams.
MessageCostEstimator and CarinaCostEstimator were separate traits with
identical signatures wrapping the same v4 function (estimateMessageCost),
each with its own no-cost default and its own host pricing impl whose
bodies were byte-for-byte identical. They grew apart because their units
were ported at different times. Now one trait (MessageCostEstimator, in
services::cost_estimation), one default (NoMessageCost), one host impl
(PricingMessageCost) serving both the TITLE_GENERATION and
MEMORY_EXTRACTION events. Behavior-frozen refactor: no logic changed, and
the title-update and carina tier-3 differentials both stay green over
fresh 02865bdb oracles. core 0.0.231, host 0.0.19, harness 0.0.209.

Planned the P4.6aw ∥ P4.6ax ∥ P4.8 riders + M6-review round (docs
only): three work orders committed under
docs/developer/porting/work-orders/ — P4.6aw (Rust riders: the
cost-estimator consolidation, the stale "serde_json sorts keys"
comment sweep, the depiction-guidelines no-vault hint), P4.6ax
(editor riders: __bold__ on-type, the form-field source-mode toggle
with the text-transforms oracle, the GFM table transformer), and
P4.8 (the M6 screen-parity review producing m6-screen-parity.md).
Planning surveys re-scoped two pool items: roleplayTemplateId
toolbar awareness is a future Salon slice (the v5 composer has no
toolbar), and the boxed ChatCostSummary variant is dead in v4 (zero
callers — a WON'T-PORT verdict for the M6 checklist). No code
changes; no version bumps.

Fixed: the thumbnail-size slider in the Salon's Chat Photos gallery had
no visible effect. The slider updated the size and requested a
correctly-sized thumbnail from the server, but the rendered box stayed
at 80px: the port sized the image via HTML width/height attributes,
which CSS always overrides, and the shared .qt-chat-attachment-image
class hard-codes width/height 5rem. Now matches v4, which sizes the
button container with an inline style and lets the image fill it — all
six sizes (80-200px) render. The class keeps its fixed 80px box for its
other two consumers (message-row attachments, save-image dialog). SPA
0.5.129.

The P4.6au ∥ P4.6av ∥ P4.7c round UNIFIED on main (2026-07-16) — all
three orders CLOSED. The homepage exists end-to-end: the `systemHome`
dispatch verb + `GET /api/v1/system/home` (v4's `getHomeData` over the
ported repos/enrichment, with the new base-sensitivity collator
option, the committed `home-{main,mount}.db` fixture family, and the
14-case `home_routes_equivalence` differential against v4's real
service + route handler at 02865bdb) feeds the new Home dashboard at
`/` (welcome greeting, the five-action quick row, the recent-chats /
projects / characters grid — the root redirect-to-salon retired).
And the Tauri shell is one-origin: the window ships on
qtap://localhost/ (the qtap handler serves the embedded dist and
delegates /api/* into the reused router), closing dogfood finding
#12's cause — server-relative image URLs now resolve; apiUrl() is
identity on a qtap-origin page. Unification wires: the systemHome
request folded into the SPA CoreRequest union (the lane's cast
removed) and diffed name-for-name against the Rust wire; the home
Playwright beat ACTIVATED over the live verb; SPA version union
0.5.127+0.5.127 → 0.5.128. Gate: fmt/clippy both feature sets/release
build clean; cargo test --workspace 325 suites / 1357 tests / 0
failed (home_routes_equivalence 14/14 by name over a FRESH oracle);
ng test 1172 (127 files); ng build clean (zero __TAURI_INTERNALS__ in
the main bundle); full Playwright 65/65, zero skips, home beats
active. Remaining acceptance: the combined human M5 + finding-#12
walk (staged in the P4.7c order header). Final versions: core
0.0.230, harness 0.0.208, host 0.0.18, web 0.0.25, quilltap-tauri
0.0.3, SPA 0.5.128.

P4.7c (lane C, one-origin): the Tauri window now loads the SPA off
qtap://localhost/ instead of tauri://localhost, fixing dogfood finding
#12 (every image broken under the Tauri shell against real data).
The qtap protocol handler serves the embedded frontendDist for
non-API GET/HEAD paths (Tauri's own asset resolver, index fallback
included) and keeps delegating /api/* and /health into the reused
quilltap-web router — so server-supplied relative URLs (avatar
filepaths, story backgrounds, /api/v1/files/... links inside
pre-rendered bodies) resolve through the same origin as the page.
The wire format is untouched (no server-side absolutizing). Spike
gating checks all green in the real webview: pushState/router
navigation, localStorage + relaunch persistence, isTauri + invoke +
event attach, relative fetches and img requests arriving at the
protocol, deep-link index fallback, devtools call. ipc_contract
grows a one-origin case (SPA index, hashed asset, deep-link
fallback, un-shadowed /health, and a seeded-image
/api/v1/files/{id} byte round-trip through handle_qtap_request).
quilltap-tauri 0.0.2 → 0.0.3.
P4.6av (lane B of the homepage + Tauri one-origin round): the Home
dashboard at `/` (SPA 0.5.127). New `screens/home/` family porting v4's
`components/homepage/*`: welcome greeting, the quick-actions row (Start
a Chat, Start Autonomous Room, Continue Last with the no-recent-chats
disabled state, New Project reusing the Prospero create dialog; v4's
Generate Image action omitted — `/generate-image` is an unported
screen, banked for M6 parity), and the three-column grid — Recent Chats
(story-background strip over the avatar stack, dangerous-chat marker),
Active Projects (color-tinted folder chip, relative activity time,
new-chat-in-project action), Characters (ResizeObserver fit-to-content
grid, provider badge from connectionProfileList, whole-card click with
the finding-#4 inner-control guard; the Chat button navigates to
`/salon/new?characterId=` instead of v4's NewChatModal — documented
divergence). Fed by ONE `systemHome` fetch against the round's §1
contract (lane A provides the verb; the request type folds into
CoreRequest at unification). Root route `''` now mounts the screen
(previously redirect-to-salon; the `'**'` wildcard still redirects);
the shell brand button points Home at `/`. New Playwright home walk
(probe-guarded record-and-fallback, ACTIVATE-AT-UNIFY once lane A's
verb merges) + 16 existing specs' `/`-entries moved to the Salon
(literal gotos, salon-scroll's template-literal root, setup-flow's
Home-nav click). 20 new unit specs pin the transcribed v4 logic
(formatMessageTime, profileDisplayName, characterNames, the click
guard, quick-action states, empty arms).
P4.6au: the home-dashboard REST edge + fixture family + differential —
`GET /api/v1/system/home` (v4's successResponse is the raw payload;
internal failure answers v4's exact 500 {error} body), the committed
`home-{main,mount}.db` family + checked-in generator
(build-home-fixture.ts — real-repo staging: 2 users, 28 characters,
16 chats, 14 projects, 5 files, a pinned photo mount with one
vault-link avatar), the jest-real-DB oracle driving v4's REAL
exported getHomeData AND the REAL route handler, and
`home_routes_equivalence`: 14 cases green (route envelope ×2, the
displayName ladder + the scoped-vs-unscoped split ×6, raw-SQL
mutation cases ×6 replayed identically on both sides), a key-order
assertion across the richest payload (88 objects), and the always-on
§1 wire-shape contract test. quilltap-web 0.0.25.

P4.6au: the home-dashboard composition ported — v4
`lib/services/home-data.service.ts` (getHomeData, 224 lines) as
`quilltap-core::services::home` plus the new `systemHome` dispatch
verb (`Request::SystemHome`, no params; `Response::SystemHome`; the
engine arm supplies the single-user scope). Carries the parallel
fetches, the help-chat filter (home shows salon/legacy-null only —
autonomous never, unlike the Salon list), the enriched recent-chats
slice (cap 12) + lastChatId, the three-source project-activity sort
(project updatedAt vs latest chat lastMessageAt vs latest file
updatedAt; cap 12), the character grid (npc/user-controlled filtered;
favorites → chat count desc → base-sensitivity name; cap 24; counts
over ALL chats incl. help/autonomous), and the wire-visible
omit-vs-null DTO splits (participant defaultImageId/url omitted when
falsy; character/defaultImage/storyBackgroundUrl explicit null;
project description/color/icon present-vs-absent pass-throughs).
`FilesRepository::find_all` added (v4's un-overridden base findAll —
unscoped, unlike the user-scoped chats/characters reads). On internal
failure the verb answers v4's fixed serverError message. Differential
lands with the REST edge (next entry). quilltap-core 0.0.230.

P4.6au: base-sensitivity collation option (`locale_compare_base` —
en-US, primary strength) added to quilltap-core's ICU4X collation
module, matching JS `localeCompare(b, undefined, {sensitivity:
'base'})`: case AND accents compare equal, ties fall to the caller's
stable sort. First consumer is the homepage character sort (v4
`home-data.service.ts:197`). Probed against Node 24 (full ICU) and
pinned by new unit tests (the sorted-vector order, the pairwise
signs, the tie pairs, numeric:false). quilltap-core 0.0.229.

P4.7c (lane C, SPA side): apiUrl() reconciled for one-origin — under
Tauri it now returns the path unchanged when the page itself is on
the qtap origin (qtap: protocol or qtap.localhost host), keeping the
qtap-origin prefix only for the cross-origin devUrl dev loop.
Signature unchanged; browser behavior byte-identical. New
api-url.one-origin.spec.ts pins the identity arm over a
qtap.localhost jsdom page URL; the existing resolver and transport
specs unchanged and green. SPA 0.5.126 → 0.5.127.

The P4.6au ∥ P4.6av ∥ P4.7c round planned (2026-07-16): three work
orders committed under docs/developer/porting/work-orders/ — the
homepage server lane (the systemHome dispatch verb + GET
/api/v1/system/home over a new home-{main,mount}.db fixture family,
differentialed against v4's real home-data.service.ts), the homepage
SPA lane (the Home screen at `/` — welcome + quick actions + the
recent-chats/projects/characters grid — replacing the redirect-to-
salon root route), and the Tauri one-origin lane closing dogfood
finding #12 (spike serving the SPA off qtap://localhost/ so
server-relative image URLs resolve; render-seam apiUrl fallback if
the WKWebView custom-scheme spike goes RED). Oracle drift-checked:
v4 HEAD unchanged at 02865bdb. Docs only — no version bumps.

Debug-profile fix for dogfood-observed first-load slowness: the
SQLite3MC amalgamation (the ChaCha20 page cipher) now compiles at
opt-level 2 in dev builds ([profile.dev.package.quilltap-sqlite3mc-sys]
in the workspace Cargo.toml — the same precedent as the existing
sha2/pbkdf2/aes overrides). At -O0 every cold page read of a
real-sized instance decrypted at unoptimized speed, making the first
open of any list page in a debug artifact (the M5 debug bundle, the
e2e servers) visibly slow against the Friday copy. One-time
amalgamation recompile when first applied (~30 s on this machine),
cached as usual after; release artifacts unaffected. Verified: the
quilltap-web contract suite opens the real encrypted fixtures green on
the rebuilt library; both Tauri bundles (debug + release) relinked.
Build-config only — no crate code, no version bumps.

Dogfood finding #12 recorded (Tauri vs the Friday copy): every image
renders broken under the Tauri shell — server-supplied RELATIVE URLs
(avatarUrl/filepath/backgroundUrl DTO fields, links inside
server-rendered bodies, + the inline blob rewrite in
markdown-renderer.ts:98) resolve against the webview's
tauri://localhost dist origin and 404. The wire format is v4-faithful
(do not absolutize server-side). Promoted to the next order with two
candidate fixes (one-origin qtap-served SPA — spike first — vs
render-seam apiUrl normalization). Docs only.

P4.7a ∥ P4.7b round UNIFIED on main (2026-07-16) — both orders CLOSED;
P4.7 (the Tauri 2 desktop shell) is LANDED. crates/quilltap-tauri
0.0.2 (tauri 2.11.5): boot via the shared quilltap-web helpers, §1
invoke dispatch/health, §2 quilltap://event pump with Green-Room
backlog replay, §3 the qtap custom protocol delegating into the reused
router, §4 terminal paired IPC over Channel, the 6-test tier-4 IPC
contract suite ∥ the SPA D14 seam: CoreTransport split (HTTP frozen
byte-for-byte), the Tauri transport + isTauri() bootstrap selection
(IPC modules in one lazy chunk), the apiUrl resolver at every raw
REST/byte site, the TerminalStreamTransport seam + Tauri pipe.
Unification wires: the §1–§4 contract diffed name-for-name (no folds
needed); the debug bundle rebuilt over a real ng build; the M5 walk
instance staged at ~/qt-m5-instance and the app boot-smoked headless.
Gate: fmt/clippy both feature sets/release build clean; cargo test
--workspace 324 suites / 1353 tests / 0 failed (ipc_contract 6/6 by
name); ng test 1150; ng build clean (main bundle Tauri-free); full
Playwright 63/63 zero skips. The human M5 walk (unlock → salon → send
in the desktop app) is the round's one remaining acceptance step.
Versions: core 0.0.228, harness 0.0.208, host 0.0.18, web 0.0.24,
quilltap-tauri 0.0.2, SPA 0.5.126.

P4.7b unit 4 (lane B, tier 2): the §4 terminal stream transport. The
WS lifecycle in TerminalSessionService extracted byte-for-byte behind
a stream-transport seam (open/send/message-callback/close with a
transient-close classification); ping cadence, reconnect/backoff, and
state transitions stay in the service, now transport-agnostic. The
Tauri pipe implements terminal_attach (Channel callback carrying the
frozen WsServerMessage union verbatim) / terminal_send /
terminal_detach; an attach failure routes into the same reconnect
path as a WS 1006/1011 close. 21 new specs (stubbed-WS mapping,
mockIPC round trips with frames driven through the captured Channel,
fake-pipe service cadence/backoff). Live pairing with lane A's shell
lands at the M5 unification walk. ng test 125 files / 1150 tests.
SPA 0.5.126.

P4.7b unit 3 (lane B): the §3 origin resolver. New `apiUrl(path)` —
identity in the browser, `qtap://localhost` (macOS/Linux) or
`http://qtap.localhost` (Windows) prepended inside the Tauri shell —
adopted at every raw REST/byte site in the closed seam inventory: the
image byte-route builders (`fileUrl`/`thumbnailUrl`), the scriptorium
mount item/blob builders + multipart upload, the file-manager
`?action=write-file` POST, the chat-file multipart POST, the
characters photo/import/reset-builtins/PNG-export routes, and the
four terminal REST fetches. Paths unchanged; 8 new specs cover both
origins, the Windows rule, and the builder sites. ng test 122 files /
1129 tests. SPA 0.5.125.

P4.7b unit 2 (lane B): the Tauri CoreClient transport + bootstrap
selection. `TauriCoreTransport` implements the §1/§2 IPC contract:
`invoke('dispatch', {request})` resolving the envelope verbatim
(rejection → the same synthetic-internal envelope as an HTTP network
failure), `invoke('health')` interpreted by the shared interpreter,
and `listen('quilltap://event')` BEFORE `invoke('events_attach')` so
the Green-Room backlog replay is never dropped; `quilltap://resync`
bumps the resync counter. Selection at bootstrap via `isTauri()`
(`createCoreTransport`); the IPC modules load through a lazy gateway
(`tauri-api.ts`) — the browser main bundle carries no Tauri IPC code.
New dependency `@tauri-apps/api` ^2.11.1. 14 new specs (mockIPC with
shouldMockEvents over the real invoke/listen, incl. a health
branch-parity sweep against the HTTP transport and the
backlog-while-connecting proof). ng test 121 files / 1121 tests.
SPA 0.5.124.

P4.7b unit 1 (lane B): the CoreClient transport split. The three raw
HTTP touchpoints (dispatch POST, health GET, the EventSource stream)
extracted verbatim into an internal `CoreTransport` boundary
(`core-transport.ts`, `HttpCoreTransport`); `CoreClient` keeps its
public API and delegates. The health interpreter (`interpretHealth`)
and the SSE skip rules (`parseEventData`) are now shared exports so
the coming Tauri transport cannot fork them. No behavior change:
ng test 119 files / 1107 tests green, ng build clean. SPA 0.5.123.

P4.7a lane CLOSED (lane A of the P4.7 Tauri round; unification + the M5
walk pending). All tier-1 deliverables landed (crate scaffold, boot, §1
dispatch/health, §2 event pump, §3 qtap protocol, the 6-test IPC contract
suite, the macOS debug bundle at target/debug/bundle/macos/Quilltap.app)
plus tier 2 (§4 terminal paired IPC; the dev loop documented in the crate
docs). Tier-3 deferrals stand as ordered: native niceties (menus/tray/
window-state/deep links), updater/signing/release bundles, uniffi/mobile,
Last-Event-ID-style replay. Versions: tauri 2.11.5, tauri-build 2.6.3,
wry 0.55.1, tao 0.35.3, tauri-cli 2.11.4.

P4.7a unit 4 (lane A): §4 — the terminal stream over paired IPC (tier 2
LANDED). `terminal_attach`/`terminal_send`/`terminal_detach` over
`tauri::ipc::Channel` carrying the frozen WsServerMessage union verbatim;
attach semantics mirror the frozen WS route exactly (unknown session →
the session_not_found exit frame; refused subscribe → the "Failed to
subscribe" rejection; otherwise ring-buffer output + meta replay then live
frames); send mirrors the WS client arm (input/resize on the manager,
ping → pong on the paired channel, malformed swallowed). One new contract
case over the chat-send fixture: spawn via the §3 protocol, attach replay,
ping→pong, input→echoed output, unknown-session exit frame, detach +
DELETE. quilltap-tauri 0.0.1 → 0.0.2.

P4.7a units 2–3 (lane A): the `quilltap-tauri` crate lands — the Tauri 2
desktop shell (tauri 2.11.5 / tauri-build 2.6.3 / wry 0.55.1). Boot mirrors
the HTTP binary via the shared quilltap-web helpers (`--data-dir`/
`--instance` accepted; Host cadence untouched); §1 `dispatch`/`health`
commands reuse `dispatch_body`/`health_parts` (dispatch always resolves the
envelope — IPC carries no HTTP status; health returns `{status, body}`); §2
`events_attach` + `quilltap://event`/`quilltap://resync` over
`subscribe_with_backlog` (backlog-before-live preserved; Lagged → resync);
§3 the `qtap` custom protocol delegates the full http::Request into the
reused router (tower oneshot, buffered body, permissive CORS + preflight).
Verified by the tier-4 #2 IPC contract suite (5 tests mirroring
quilltap-web/tests/contract.rs case-for-case: health vocabulary, dispatch
round trip, malformed → BadRequest, locked vault + setup flow, boot-failure
arm, the ordered §2 event trace incl. Green-Room backlog replay on
re-attach, protocol GET/preflight, plus a get_ipc_response wiring proof).
quilltap-tauri 0.0.1 (new).

P4.7a unit 1 (lane A, the Tauri shell): extracted the transport-agnostic
cores from quilltap-web so the Tauri IPC surface can reuse them verbatim —
`dispatch_body` (request bytes → HTTP status + envelope Value, all three
arms including the Locked setup-body merge), `health_parts` (HTTP status +
the /health JSON body), and `subscribe_with_backlog` (the D6
subscribe-then-snapshot event ordering rule). The HTTP handlers now wrap
these; behavior unchanged (all 16 quilltap-web suites green). Also moved
the binary's base-dir resolution and production HostConfig assembly into
the lib (`resolve_instance_base_dir`, `production_host_config`) so the
Tauri shell boots with the identical recipe. quilltap-web 0.0.23 → 0.0.24.

P4.7 round PLANNED (2026-07-15): two work orders committed for the
Tauri 2 desktop shell — P4.7a (`crates/quilltap-tauri`: invoke
dispatch/health, the global event pump, the `qtap` custom protocol
delegating into the reused quilltap-web router, terminal paired IPC,
the tier-4 IPC contract suite) ∥ P4.7b (the SPA D14 seam made real:
the CoreClient transport split + Tauri IPC implementation, the origin
resolver over the closed raw-REST inventory, the terminal stream
transport seam; browser path frozen, full Playwright as the proof).
Binding IPC contract (§1–§4) reproduced verbatim in both orders;
milestone M5 (the Tauri app runs the same SPA against the same core)
lands at unification with a human-run walk. Docs only — no code, no
version bumps.

P4.6ar ∥ P4.6as ∥ P4.6at round UNIFIED on main (2026-07-15) — all
three orders CLOSED, and with them the P4.6ao-round LLM-Inspector,
Default-Aesthetics-card, and minHeight-residual deferrals. Landed: the
llm-logs read surface (eight repo reads, the llmLogsList/llmLogGet/
llmLogDelete verbs + REST edges; v4's ?standalone=true carried
broken-but-exact, the garbage-limit NaN quirk via hand-rolled js_min,
no ownership check on the item routes — all faithful) + the
systemImageAestheticsGet/Set pair over DRY'd services::aesthetics
helpers, with two fresh-oracle differentials (27-case llm-logs incl. a
wire key-order assertion, 13-case aesthetics incl. the
unprovisioned-store arms) over the new four-file inspector-* fixture
family ∥ the whole LLM-Inspector SPA vertical (slide-over panel with
the role-only-while-open divergence from v4's phantom modal,
entry/panel components, toolbar button + Cmd+Shift+L, per-message cpu
icon, reconcile-point log refresh, a live seeded-partition e2e walk) ∥
the shared aesthetic-editor-field extraction (correcting textarea-era
drift in the project field) + the Default Aesthetics Images-tab card +
sixteen minHeight bindings at the P4.6al-adopted sites. Unification
gate: fmt/clippy both feature sets/release build clean; cargo test
--workspace 320 suites / 1347 tests; ng test 1107; full Playwright
63/63 zero skips, all three new beats live. Final versions: core
0.0.228, harness 0.0.208, host 0.0.18, web 0.0.23, SPA 0.5.122.

P4.6ar/as/at unification wires (2026-07-15): the three SPA-local
request types folded into the CoreRequest union name-for-name against
types.rs (LlmLogsListRequest, SystemImageAestheticsGet/SetRequest; the
api modules re-export and drop their casts; llmLogGet/llmLogDelete
deliberately get no union member — no v4 client calls them either), a
new p4_6ar_wire_contract harness guard pinning the §1/§2 serde shapes
(the P4.6ao precedent), and both ACTIVATE-AT-UNIFY beats gone LIVE:
the LLM-Inspector walk now reads the real llmLogsList verb over the
partition rows global-setup seeds (route mock deleted), and the
Default Aesthetics beat drives the real systemImageAestheticsGet/Set
(fulfil-mock reduced to a record-and-fallback observer) and grew the
reload round-trip assertion the mock could not prove. SPA version
accumulated to 0.5.122 (lane B +5, lane C +4); harness 0.0.208.

P4.6at unit 4 (2026-07-15): the Default Aesthetics e2e beat. Deep-links
/settings?tab=images&section=default-aesthetics, asserts both fields load, types
into the Default Image Aesthetic editor, saves, and pins the dispatch payload.
The two aesthetics verbs are route-mocked until the sibling lane's handlers land
(tagged ACTIVATE-AT-UNIFY); everything else on the page dispatches live.
SPA 0.5.117.

P4.6at unit 3 (2026-07-15): the minHeight residual. The sixteen form fields the
P4.6al editor adoption left without v4's minHeight now bind it: the memory
editor's content field (10rem), the eight New Character prose fields
(6/8/8/8/8/6/12/8rem) and the seven Details-tab prose fields
(6/8/8/8/6/12/8rem). Every value re-verified against v4 by aria-label rather
than position; three specs pin them per host. Closes the residual gap named in
the P4.6aq record. SPA 0.5.116.

P4.6at unit 2 (2026-07-15): the Default Aesthetics card. v4's third Images-tab
card now exists in v5 (closing a named P4.6ao/ap/aq-round deferral): two shared
aesthetic-editor-fields over the systemImageAestheticsGet/Set verbs, under a
"Default Aesthetics" collapsible with v4's copy and its ?section= deep link.
The Default Image Aesthetic covers scenes and backgrounds; the Default Character
Aesthetic covers how people and outfits are depicted. Saving a field empty
deletes the stored file and restores the fallback. The request types are local
to the lane's api module and cast at dispatch until the unifier folds them into
CoreRequest. SPA 0.5.115.

P4.6at unit 1 (2026-07-15): the shared aesthetic-editor-field. v4 has one
AestheticEditorField serving three surfaces; v5 had a prospero-only copy.
Extracted it to ui/aesthetic-editor-field.ts, taking injected load/save
callbacks plus a query key in place of v4's loadUrl/saveUrl (no URLs over
the dispatch boundary), and re-pointed project-aesthetic-field at it. The
extraction is a faithful re-port, so it also corrects textarea-era drift in
the project field: the Save button is now primary and dirty-gated
(disabled until an edit, "Saving…" in flight), with v4's "Saved" span and
its shared load/save error span. Two project specs adapted: an untouched
Save is unreachable under v4's dirty gate, so the load-never-re-normalizes
guard now asserts the Save button stays disabled after a load — the same
bug, caught one step earlier. SPA 0.5.114.

P4.6as unit 5 (2026-07-15): the courier spec's chat-discovery guard now
waits for its selector instead of sampling isVisible() the instant the
message list appears. The sample was a race, so whether the lightbox beat
RAN was timing-dependent — it skipped in the full suite (reading as
"nothing to test") while passing in isolation. Isolated by a control run
with the new spec excluded. 2s ceiling, paid only on chats that genuinely
lack the selector. SPA 0.5.117 → 0.5.118.

P4.6as unit 4 (2026-07-15): the slide-over declares its dialog role only
while open. v4 hard-codes role="dialog" + aria-modal="true" on an
always-mounted panel, so every Salon page permanently contained what
announced itself as an open modal, with its controls tab-reachable
off-screen. v5 keeps the markup and the data-open transitions but
declares the role only while open and marks the closed panel inert.
Caught by the full Playwright run: the phantom dialog broke the
Edit-Enclave beat (getByRole('dialog') strict-mode violation) and the
courier lightbox beat (which asserts no aria-modal dialog remains after
Escape). Both green again. SPA 0.5.116 → 0.5.117.

P4.6as unit 3 (2026-07-15): the LLM-Inspector e2e beat. New
apps/web/e2e/llm-inspector-flow.spec.ts — two walks over the locked
fixture instance: the toolbar open → three entries oldest-first with
their badges → expand → request/response/usage tabs → filter → the
Cmd+Shift+L close, and the per-message cpu icon opening the panel
scrolled with the right entry highlighted. Both tagged
ACTIVATE-AT-UNIFY: llmLogsList is lane A's verb, so the dispatch is
route-mocked with the Shared contract §1 envelope verbatim.
global-setup.ts seeds three llm_logs rows on Solo Voyage (the committed
salon-llm-logs.db has the table but no rows) through the CLI's
--llm-logs flag, which targets the llm-logs partition; the rows carry
SINGLE_USER_ID directly since the userId rewrite loop only reaches the
main db. SPA 0.5.115 → 0.5.116.

P4.6as unit 2 (2026-07-15): the LLM-Inspector host wiring. The
conversation header gained the Inspector button (code glyph, v4's
"LLM Inspector (Cmd+Shift+L)" title, active-state class swap, placed
BEFORE the cost summary per v4's toolbar effect) gated on
llmLoggingSettings.enabled !== false — DEFAULT TRUE. message-row
gained the per-message cpu entry for assistant messages with logs,
carrying MessageActionBar's title ("View LLM request/response logs")
since v5's single bar is that bar; message-list threads
messagesWithLogs + the callback. The salon host owns the llmLogsList
query (enabled on the CANONICAL message count), the panel mount, the
open/toggle/close state with v4's clear-on-open-only rule, and the
Cmd+Shift+L shortcut (attached only while the gate is true, uppercase
L). Tier 2 landed: the post-turn refreshLogs hook. 39 new specs;
ng test 968 → 1094. SPA 0.5.114 → 0.5.115.

P4.6as unit 1 (2026-07-15): the LLM-Inspector components. New
apps/web/src/app/ui/slide-over-panel.ts (a reusable right-edge
slide-over: always-mounted scrim + panel driven by data-open, focus
save/restore, Escape, scrim click, focus trap), chat/llm-logs.api.ts
(the llmLogsList request/DTOs + messagesWithLogs derivation; the
request type is LOCAL and cast at the call site per the round's
ownership rule), chat/llm-inspector-entry.ts (collapsed summary row +
lazy request/response/usage tabs, the twelve-of-nineteen badge tables
carried verbatim, the 500-char truncation, both backward-compat
fallback chains), and chat/llm-inspector-panel.ts (chronological
reverse, the seven filter groups, entry count, refresh, the three
empty states in v4's priority order, scroll-to-message + highlight).
102 new specs, each pinned to its v4 file:line. SPA 0.5.113 → 0.5.114.

P4.6ar unit 4 (lane A): a wire key-order assertion for the llm-logs
differential, plus a corrected seam note. `llm_logs_routes_equivalence`'s body
diff sorts keys on both sides, which left the schema-field-order marshaling
unproven; `check_key_order` now diffs the raw key SEQUENCE of every object
against the oracle's `JSON.stringify` bytes, on `item_get_found` (7 objects)
and `list_recent_default` (65). Swapping two `LlmLogRow` fields fails it while
every body diff still passes.

The finding behind it: `db/llm_logs.rs`'s Phase-2 header claims
`serde_json::Value` sorts object keys. It does not in this crate — quilltap-core
builds serde_json with `preserve_order` (the locked decision that closed the
open-JSON key-order seam), confirmed by probe. Comments repeating the stale
claim are corrected; the obsolete Phase-2 constraint is flagged in the
status-log as a follow-up rather than rewritten here. Versions: core 0.0.228,
harness 0.0.207.

P4.6ar unit 3 (lane A): the `system/image-aesthetics` GET/PUT pair — the
server side of the Images tab's two Default Aesthetic editors.
`SystemImageAestheticsGet`/`Set` + `Response::SystemAesthetic`, and
`GET`/`PUT /api/v1/system/image-aesthetics` at the web edge. The single-tier
read/write pair the project aesthetic verbs already carried is factored into
`services::aesthetics` (`aesthetic_filename_for_kind` / `read_aesthetic_file` /
`write_aesthetic_file`) and shared by both surfaces; only the mount they
resolve differs (the project's official store vs the Quilltap General
singleton). The project pair now calls the shared helpers — behavior
unchanged, and its five aesthetic cases stay green against a freshly
regenerated oracle.

v4 arms carried: an invalid or absent `kind` is a 400 on both verbs; a GET
with no Quilltap General store SUCCEEDS with `{content: ''}` while the PUT
refuses with a 500; and empty, whitespace-only, malformed-body, and
non-string-`content` PUTs all resolve to `''` and DELETE the file (restoring
the fallback). New differential `image_aesthetics_routes_equivalence`: 13
cases (the order asked for ≥ 8), each mutating case re-reading the store so
the effect is diffed and not just the ack. Versions: core 0.0.227, harness
0.0.206.

P4.6ar units 1–2 (lane A): the llm-logs read surface — the LLM Inspector's
whole server side. `db::llm_logs` gains the eight repo reads v4's
`/api/v1/llm-logs` routes call (`findById` / `findByMessageId` /
`findByChatId` / `findAllForChat` / `findByCharacterId` / `findStandalone` /
`findByType` / `findRecent`), marshaling full rows through a new `LlmLogRow`
in `LLMLogSchema` field order (a NULL column is ABSENT from the body, matching
v4's null→undefined hydrate; `durationMs` renders as a JS number).
`api::llm_logs` ports the two route handlers behind `LlmLogsList` /
`LlmLogGet` / `LlmLogDelete` + `Response::LlmLog`, and `quilltap-web` serves
`GET /api/v1/llm-logs` and `GET`/`DELETE /api/v1/llm-logs/{id}` (envelope
unwrapped; `?type=` maps onto the contract's `logType`).

Four v4 behaviors carried deliberately, each pinned by a differential case:
a garbage `limit` disables the slice entirely and rides the wire as `null`
(`parseInt` NaN → `Math.min` NaN → `length > NaN` false — Rust's `f64::min`
would have silently repaired it); `total` is the FETCHED page's size, not the
collection's, on any repo-limited branch; the item routes have NO ownership
check (any log, any user); and `?standalone=true` can never return a row —
v4's `$eq: null` lowers to `col = NULL`, which is UNKNOWN for every row, the
same family of bug v4's own comment documents for `$ne: null`.

New differential `llm_logs_routes_equivalence`: 27 cases over fresh
`02865bdb` oracles — every list branch incl. precedence and both
includeMessages defaults, the clamp above cap, garbage/zero/negative limits,
garbage offset, offset and offset+limit, both notFound arms, the item
GET/DELETE found+missing arms, and the cross-user read+delete that prove the
missing ownership check. The two delete cases re-read afterwards, so the DB
effect is diffed and not just the ack. Versions: core 0.0.226, web 0.0.23,
harness 0.0.205.

P4.6ar unit 0 (lane A): the new committed `inspector-*` fixture family +
its checked-in generator (`harness/oracle/fixtures/build-inspector-fixture.ts`
over `inspector-web.json`). Four files, all baked through v4's real repos:
`inspector-main.db` (two users, one character, one chat + three messages,
the Quilltap General store's `instance_settings.generalMountPointId`
pointer), `inspector-mount.db` (the character's minted vault + the General
store carrying `lantern-aesthetics.md` and deliberately no
`aurora-aesthetics.md`), `inspector-llm.db` (the llm-logs partition — 14
rows across all twelve LLM-Inspector badge types: message-linked,
chat-linked, character-linked, standalone, one error response, rows with
and without usage/cacheUsage/durationMs, and one row owned by a second
user), and `inspector-nostore-main.db` (a byte-copy of the main DB with
the general-store pointer deleted, so the unprovisioned-store arms stage
without either side mutating its copy). Every log row carries a distinct
createdAt — v4's translated sort is `ORDER BY "createdAt" DESC` with no
secondary key. Fixture data only; no crate source touched.

P4.6ar ∥ P4.6as ∥ P4.6at round planned (2026-07-15): three work orders
committed under docs/developer/porting/work-orders/ — the llm-logs +
system-aesthetics server lane (the eight llm-logs repo reads, the
llmLogsList/llmLogGet/llmLogDelete verbs + REST edges, the
systemImageAestheticsGet/Set pair, a new inspector fixture family with
two route differentials), the LLM-Inspector SPA lane (slide-over panel,
inspector entries, toolbar button + Cmd+Shift+L, per-message log icon),
and the Default-Aesthetics-card + minHeight-residual lane (the shared
aesthetic-editor-field extraction, the third Images-tab card, sixteen
recorded minHeight bindings). Shared contracts §1-§2 pinned verbatim
across all three; docs only, no version bumps.

P4.6ao ∥ P4.6ap ∥ P4.6aq round UNIFIED on main (2026-07-15) — all three
orders CLOSED, and with them the P4.6an token/cost deferral, the
P4.6ak/P4.6am background-generation deferral, and the P4.6al item-6
form-field deferral. Landed: the chatGetCost verb (raw un-enveloped
body) + the regenerate-background un-refusal + the TITLE_UPDATE job
handler (closing a live loud-failure — context_summary had been
enqueuing title jobs that died unhandled, which also kept automatic
background generation from ever firing) over a new committed
cost-background fixture family with three fresh-oracle differentials
(13-case routes, 10-case tier-3 title-update + a runner-registration
e2e, the §1/§2 wire-contract pin) ∥ the per-message token badge +
compact chat-totals header summary + the Story Backgrounds Images-tab
card + the Regenerate Background header entry with both polls (active
5s×36, passive 30s gated by enabled — display stays unconditional) ∥
the qt-markdown-field minHeight input + eleven field adoptions across
ten sites with the async-load absorb-once gating fix at three hosts.
Unification wires: the §1/§2 request types folded into the CoreRequest
union; the two ACTIVATE-AT-UNIFY beats made live (the totals summary
reads the real verb; the regenerate entry drives the real edge);
image_profiles joined the e2e userId rewrite. Gate: fmt/clippy (both
feature sets)/release build clean; cargo test --workspace 317 suites /
1341 tests / 0 failed with both new differentials regenerated fresh at
02865bdb and run by name; ng test 968; ng build clean; full Playwright
60/60 zero skips, all four new beats LIVE. Final versions: core
0.0.225, harness 0.0.204, host 0.0.18, web 0.0.22, SPA 0.5.113.

P4.6ao/ap/aq unification fix: the e2e userId-rewrite loop gains
image_profiles (user-scoped; the regenerate-background resolver checks
ownership, so an un-rewritten profile was invisible and the live
regenerate beat hit the "No image profile available" arm), and
global-setup makes the fixture's "Mock Images" profile resolvable
(isDefault + the fixture's api-key id). Full Playwright 60/60 zero
skips with all four token/cost + background beats LIVE.

P4.6ao/ap/aq unification wires: ChatGetCostRequest +
ChatRegenerateBackgroundRequest folded into the CoreRequest union
(name-for-name against types.rs), the api-module casts dropped, and
the two ACTIVATE-AT-UNIFY beats in salon-token-cost-flow.spec.ts made
LIVE (route mocks deleted — the totals summary reads the real
chatGetCost verb; the regenerate entry drives the real un-refused
edge). SPA lockfile version fields synced to the accumulated 0.5.113.

P4.6aq unit 5: the e2e walks follow the swapped fields. The scenarios
walk drove #scenario-body and the settings walk drove the template
dialog's first textarea; both are ProseMirror contenteditables now, so
they take real key events on .qt-rich-editor-content (the P4.6ag idiom).
Also fixes a latent timing bug the run exposed in the characters walk:
its first beat waited for the roster with the default 5s after clicking
Unlock, while the sibling helper in the same file already allowed 10s for
the identical wait — that beat pays a cold PBKDF2 unlock on a debug
build. Unrelated to the field swaps (unlock is server-side; this lane
changed no Rust).

P4.6aq unit 4: the character appearance fields take qt-markdown-field —
the six image/physical prompts on the character-edit tab (v4 minHeight
4/4/6/8/10/10rem) and the depiction-guidelines editors on both the edit
and view tabs (8rem, v4's AestheticEditorField value). The edit tab's
guidelines field is not in the work order's table, but its v4 counterpart
is the same AestheticEditorField as the view tab's, so it is swapped too.

Both blocks on the edit tab now gate their editors on their query, with
seeding moved into queryFn — v4 gates the same way (DescriptionsTab
early-returns while loading), and without it a load would surface as an
edit and let an untouched Save rewrite stored prompts in normalized
bytes. Specs cover seeding, per-field edit payloads, and untouched-save
byte-exactness for all seven fields.

P4.6aq unit 3: qt-markdown-field swapped in at the remaining four
single-field sites — the project wardrobe item description (v4 minHeight
10rem), the project instructions card (14rem), the project aesthetic
field (8rem), and the new-chat starting scenario (6rem). This closes the
last two recorded "plain textarea" divergences in the Prospero cards.

The aesthetic field also gains v4's loading gate: v4 never emits on a
load (its mount-time parse is tagged external-sync and skipped by the
change listener), and v5's equivalent absorb-once seam swallows exactly
one emit, so the content must be in hand before the editor mounts. Its
seeding moved into queryFn to guarantee that. Without the gate, opening
the card and pressing Save would silently rewrite the stored file in
normalized bytes (__bold__ to **bold**, trailing newline dropped); two
specs now pin that, and both fail if the gate is removed.

P4.6aq unit 2: qt-markdown-field swapped in at the first four form-field
sites — the character-edit scenario rows (v4 minHeight 6rem), the
scenarios editor modal body (12rem), the roleplay-template LLM prompt
(14rem), and the character system-prompt content (12rem). Each carries
v4's minHeight and its remountKey as recordKey. The scenario modal's
v4 counterpart passes no minHeight, so it takes v4's own 12rem default;
ours passes that value explicitly. Specs drive the editors through the
imperative handle and assert the hosts' existing dispatch payloads carry
the markdown byte-identical, including an untouched-save round-trip.

P4.6aq unit 1: the minHeight input on qt-markdown-field (v4
MarkdownLexicalEditor's input of the same name), bound as a min-height
on the qt-rich-editor element inside the field's own template — no
rich-editor change, no new stylesheet. It defaults to unset rather than
v4's 12rem, so the P4.6al-adopted sites (which pass nothing) render
unchanged; every site ported from here on passes v4's effective value
explicitly, including the sites where v4 falls through to its own
default. Three unit specs.
P4.6ap unit 7: fix the token-badge beat's in-suite failure. The beat asserted
an absolute badge count across the chat (1), which held in isolation but not
in-suite: m4-salon.spec.ts sends a live turn into the same Solo Voyage chat
through the mock LLM, and that reply arrives with real token counts, so it
grows a badge of its own (Expected 1, Received 2). The assertion was wrong,
not the port — and the failure incidentally confirms the finalizer stores
actuals and the badge picks them up on a live turn. Now scoped to the message
row, with the gate's other arm (a null-count message stays bare) asserted
per-row too, so neither claim depends on suite history. Full Playwright 60/60,
zero skips. SPA 0.5.108.

P4.6ap unit 6 (tier 2): four live Playwright beats for the token/cost +
story-background surfaces (salon-token-cost-flow.spec.ts). Two run LIVE
against the real server: the per-message token badge following
showPerMessageTokens through the real chatSettingsUpdate dispatch, and the
Story Backgrounds card round-tripping through a reload. Two are
ACTIVATE-AT-UNIFY, route-mocked with the Shared-contract bodies verbatim
until lane A lands: the chat-totals summary (chatGetCost) and the regenerate
entry (chatRegenerateBackground, scoped to the enqueue — the e2e host has no
image-provider key). global-setup seeds the chat-row cost aggregates the
totals beat will read live at unification; the badge beat needs no seed at
all — the fixture already carries a message with 8/4 tokens. SPA 0.5.107.

P4.6ap unit 5: the Regenerate Background entry + both story-background polls
(Salon SPA lane). Ports v4's regenerate handler (useChatControls.ts:397-416)
and useStoryBackground's two polls: the passive 30s refetchInterval, gated by
storyBackgroundsSettings.enabled, and the active 5s/36-poll regeneration
watch that stops when the resolved background moves. Display stays
unconditional — the flag gates polling only. The regenerate button relocates
from v4's unported ChatSidebar palette to the conversation-header cluster
(the Edit-Enclave idiom) and uses the sparkles glyph, since v4's image glyph
already means "View chat photos" in an icon-only cluster. Server messages
(both success arms and the three badRequest strings) surface verbatim through
the scriptorium flash idiom. The dispatch uses the EXISTING
chatRegenerateBackground wire name; route-mocked until lane A un-refuses it.
SPA 0.5.106.

P4.6ap unit 4: the Story Backgrounds settings card (Salon SPA lane). Ports
v4 components/settings/chat-settings/StoryBackgroundsSettings.tsx into the
IMAGES tab — where v4 mounts it, despite the file living in v4's
chat-settings/ directory — over the shared ChatSettingsCard substrate, so it
joins the one deduped settings GET. Both controls PUT the whole
storyBackgroundsSettings bag with one key replaced (a partial nested patch
would drop the sibling key; the server replaces the JSON column wholesale),
and the async profile select binds [selected] per option. The Images tab
gains v4's space-y-4 card wrapper, which a single-card tab never needed.
Deferred: v4's third Images card, Default Aesthetics. SPA 0.5.105.

P4.6ap unit 3: the chat-totals header summary (Salon SPA lane). Ports v4
components/chat/ChatCostSummary.tsx (COMPACT variant — v4's Salon is the
only caller and never asks for the other one) into the conversation header,
over the round's Shared-contract §1 chatGetCost verb that lane A provides;
the request/DTO types live in a lane-owned chat/chat-cost.api.ts and the
verb is route-mocked until unification. Gated on
tokenDisplaySettings.showChatTotals from the already-shared settings query,
refresh-keyed on the CANONICAL server message count (not the optimistic
display list, which would fetch totals mid-stream for an unwritten turn).
Carries v4's suppression rules — hidden (no fetch at all), loading, zero
totals, and a failed fetch all render nothing — plus the openrouter-estimate
and unavailable price-source markers. SPA 0.5.104.

P4.6ap unit 2: the per-message token badge (Salon SPA lane). Ports v4
components/chat/TokenBadge.tsx to apps/web/src/app/chat/token-badge.ts and
mounts it in the message-row timestamp row behind v4's gate
(showPerMessageTokens && (promptTokens || completionTokens)), threading
tokenDisplaySettings from the already-shared chatSettings query — no new
request. The timestamp row now uses v4's qt-chat-message-action-timestamp
markup, which activates the user-bubble color rule v5 had ported but never
mounted. Two v4 dead paths are ported AS dead with why-comments rather than
invented: showPerMessageCost (no cost field exists on the Message type, and
v4's gate reads the tokens flag only) and showSystemEvents (no renderer
anywhere in v4). SPA 0.5.103.

P4.6ap unit 1: the token/cost display formatting leaf (Salon SPA lane).
Ports v4 lib/utils/format-tokens.ts (formatTokenCount, formatCostForDisplay)
to apps/web/src/app/chat/format-tokens.ts. The 37-case spec table was
generated by driving v4's real function over a band-edge corpus and pinned
verbatim, so it is a tier-1 equivalence proof rather than a guess: it pins
v4's quirks (999999 renders "1000.0K" not "1.0M" — v4 bands on the raw
value; a non-zero sub-rounding cost renders "$0.0000", never "Free").
SPA 0.5.102.
P4.6ao unit 3 (tier 1): the TITLE_UPDATE job handler. KNOWN_JOB_TYPES listed
TITLE_UPDATE but nothing was registered, so the jobs context_summary enqueues
at every title checkpoint died on the runner's loud fallback -- which also
meant the automatic story-background trigger never fired. Ports v4's
handleTitleUpdate (all four checkpoint-cursor arms, the throwing reads, the
uncensored rerouting for dangerous chats, the TITLE_GENERATION spend event,
and the non-help-chat background kick over the already-ported gate) and its two
cheap-LLM tasks, considerTitleUpdate and considerHelpChatTitleUpdate, plus
their two byte-extracted system prompts. Registered in core and in the host
spine, with the pricing cascade wired to the new MessageCostEstimator seam.

New title_update_tier3_equivalence differential: 10 mocked-LLM cases over a
fresh 02865bdb oracle, diffing the chat row, the system events, and the job
rows. The canned reply is keyed by the system prompt on both sides, so a wrong
evaluator prompt fails the case instead of silently answering the other arm.
Plus a runner-registration E2E (enqueue, claim, dispatch, COMPLETED) -- the
guard for the actual bug, which a differential over an unregistered handler
would have passed.

Also adds a small p4_6ao_wire_contract test pinning the round's shared-contract
request shapes, since the SPA lane develops against a route mock and only meets
these verbs live at unification.

P4.6ao unit 2 (tier 1): the regenerate-background un-refusal. The
`chatRegenerateBackground` dispatch no longer answers a typed refusal — it
runs v4's handleRegenerateBackground: the three badRequest arms (story
backgrounds disabled / no image profile resolvable / no characters in the
chat), the image-profile resolution, and the enqueue with its chat-level
dedupe (a second call reuses the pending job, returning the same jobId with
the already-in-progress message). Edge only — the generation job was already
ported and is registered live in the host.

Fixes a latent bug in the shared enqueue that the new job-row diff caught:
enqueue_story_background_generation OMITTED `projectId` from the payload when
the chat had no project, but both v4 call sites build the literal with
`projectId: chat.projectId ?? null`, so the key is never absent. It is now
always written, null when there is no project. Inert for the job handler
(which reads it with as_str, where null and absent agree) and correct for the
stored row.

cost_background_routes_equivalence 7 -> 13 cases against a fresh 02865bdb
oracle; the regenerate cases diff the background_jobs rows as well as the
response body. The oracle mocks the job processor off — enqueueJob kicks it,
and it was claiming the freshly-queued row and flipping it PENDING ->
PROCESSING mid-case.

P4.6ao unit 1 (tier 1): the chatGetCost verb. Ports v4's
getChatCostBreakdown (stored chat-row aggregates, the legacy
cost-without-priceSource inference) and getDetailedChatCostBreakdown
(per-message and per-system-event itemization) as
services::cost_estimation, behind a new Request::ChatGetCost /
Response::ChatCost pair and the chats GET REST edge
(?action=cost[&detailed=true]) — which answers the breakdown RAW, with
no successResponse envelope, exactly as v4 does. New committed fixture
family cost-background-{main,mount}.db plus its checked-in generator,
and a new cost_background_routes_equivalence differential (7 cases) over
a fresh 02865bdb oracle.

P4.6ao ∥ P4.6ap ∥ P4.6aq round planned (2026-07-15): three work orders
committed under docs/developer/porting/work-orders/ — the token/cost +
background-generation server lane (the chatGetCost verb, the
regenerate-background un-refusal, the TITLE_UPDATE handler), the Salon
SPA lane (per-message token badge, chat-totals header summary, Story
Backgrounds card, regenerate entry + polls), and the form-field
adoptions rider (the minHeight input + ten qt-markdown-field swaps).
Docs only; v4 baseline 02865bdb re-verified, no drift.

P4.6an round UNIFIED on main (2026-07-15) — the order CLOSED, and the
last two P4.6ad deferrals close with it. One lane, nine commits: the
eleven remaining Chat-tab settings cards in v4's full 16-card order
(shared ChatSettingsCard substrate, one deduped settings GET, the tab
placeholder retired), the live croner cron next-run preview in the
shared autonomous room card, the composer spellcheck rider, and the
dangerousContentSettings Zod-faithful parse (the one server gap —
settings_routes_equivalence 19 → 32 cases against a fresh 02865bdb
oracle). Unification gate: fmt/clippy (both feature sets)/release
build clean; cargo test --workspace 314 suites / 1327 tests / 0
failed with the settings differential regenerated fresh and run by
name (32/32); ng test 846; ng build clean; full Playwright 56/56,
zero skips. Final versions: core 0.0.222, harness 0.0.201, host
0.0.17, web 0.0.21, SPA 0.5.101. Still deferred loud: the Salon
token/cost display rendering (a Salon slice with its own order).

P4.6an unit 9 (tier 2): four live Playwright beats over the fitted-out Chat
tab — the full sixteen-card order renders with the placeholder gone;
Auto-Scroll round-trips a scalar through the real server and survives a
reload; the Dangerous Content mode round-trips a nested BAG and its sibling
keys survive; and the cron preview computes in the browser as you type
(valid → "Next run:", garbage → the invalid arm, blank → nothing). Full
Playwright 52 → 56, zero skips.

P4.6an unit 8 (tier 2): the Composer card's setting is now LIVE — the
`qt-rich-editor` contenteditable binds `spellcheck` to
`chat_settings.composerSpellcheck ?? true`, matching v4's Lexical composer.
The salon threads the setting from the settings row it already reads; the
document pane and the form fields take the default. The attribute is written
explicitly ("true"/"false") because a contenteditable inherits spellcheck,
and the view is nudged when the setting changes, since ProseMirror only
recomputes its attributes on a view update and the setting arrives async.

P4.6an unit 7: the Settings -> Chat tab is fully fitted out. All sixteen v4
cards are mounted in v4's exact order (Composer and Auto-Scroll between
Composition Mode and Text Replacement; the nine engine-facing cards between
Text Replacement and Data Retention), each with v4's title, description, and
`sectionId` deep link. The "not yet fitted out" placeholder that enumerated
the eleven missing cards is removed. A tab spec pins the order and the
sectionIds against `ChatTabContent.tsx`, since v4's order is neither
alphabetical nor thematic and is easy to tidy by accident.

P4.6an unit 6: the last two Chat-tab cards land — Image Description (the
primary + uncensored-fallback vision pickers, each a bare nullable scalar
over the vision-capable connection profiles) and Dangerous Content (the
largest of the eleven: mode/threshold/scan toggles/uncensored routing/display
mode/custom prompt, plus the image-prompt-expansion picker that writes the
cheap-LLM bag rather than the danger bag). Both use `[selected]`-per-option
over async-loaded profile lists per the binding dogfood-#6 rule, with the
late-options regression spec confirmed to fail under a `[value]` binding.
All eleven cards now exist; mounting them is the next unit.

P4.6an unit 5: the Agent Mode and Context Compression Chat-tab cards land.
Agent Mode ports v4's two separate handlers over `agentModeSettings` (the
default-enabled toggle and the max-turns select, which writes a number, not
the select's string). Context Compression ports v4's full slider drag/commit
protocol — the three sliders track a local value while dragging and write
once on release, never on every `input` — including the window/interval
cross-validation, where raising the sliding window past the project-context
re-injection interval pushes the interval up with it in a single PUT (a "0"
= never interval is exempt).

P4.6an unit 4: the three nested-bag Chat-tab cards land — Token Display
(four visibility toggles over `tokenDisplaySettings`), Memory Cascade (the
two cascade-action selects over `memoryCascadePreferences`, with v4's
asymmetry kept: the swipe select filters out "Ask every time"), and
Thinking / Reasoning (`thinkingDisplay`, display-only, with "start
collapsed" gated on "show thinking"). Each PUTs the whole merged bag —
the server replaces the column wholesale, so a partial nested patch would
drop the sibling keys. The Salon token/cost RENDERING remains a named
deferral; the card stores the setting faithfully regardless.

P4.6an unit 3: the four scalar-toggle Chat-tab cards land — Composer
(`composerSpellcheck`), Auto-Scroll (`autoScrollOnResponseComplete`),
Automation (`autoDetectRng`), and Answer Confirmation
(`answerConfirmationSettings`) — each a v4-faithful port with v4's copy
verbatim and v4's default-when-unset. Adds the cards' shared substrate: the
ported option tables/defaults from v4's `types.ts`, the `ChatSettingsCard`
base (the v5 answer to v4's `useChatSettings` provider — one shared query,
a save that seeds the cache from the PUT response), and a local
`qt-settings-card` shell. Not yet mounted in the tab; the tab re-order is
its own unit.

P4.6an unit 2: the autonomous cron next-run preview is LIVE, closing the
first of the two P4.6ad deferrals. The shared Autonomous Room card (New
Chat, Edit Enclave, Settings defaults) now previews the next fire time as
you type, via v4's own `croner` dependency at v4's exact range (^10.0.1).
v4's `tryCronNextRun` ported verbatim with its three arms: "Next run:
<local time>", "Parses, but never fires from now.", and "Invalid cron:
<croner's message>". The shape-only five-field check (`isCronShapeValid`)
it replaces is retired. The server's hand-rolled `enclave::cron` is
untouched — v4 previews in the browser against the local `Date`, and the
two agree by construction.

P4.6an unit 1 (the order's server-gap contingency, NOT expected to
fire): `dangerousContentSettings` now goes through a hand-rolled
Zod-faithful parse instead of a serde struct round-trip. The struct
path dropped an explicit `null` on the three `.nullable().optional()`
fields (`uncensoredTextProfileId`, `uncensoredImageProfileId`,
`customClassificationPrompt`) where v4's Zod keeps it, rejected a
partial bag where v4 materializes defaults, and re-emitted an integral
`threshold` as `1.0` where v4 writes `1`. All three are reachable from
the Dangerous Content card this round ports. Proven by two new
`settings_routes_equivalence` cases (`s_put_danger_nulls`,
`s_put_danger_partial`) over a fresh `02865bdb` oracle: 32/32, and all
three defects confirmed to fail the diff before the fix.

Planned the P4.6an round (one lane): the eleven remaining Chat-tab
settings cards + the autonomous cron next-run preview, closing the
last two P4.6ad deferrals. Work order committed at
`docs/developer/porting/work-orders/p4.6an-chat-tab-cards-cron-preview.md`;
survey-verified SPA-only (the server settings parse already covers
every card key); v4 baseline `02865bdb` drift-checked at planning.
Docs only.

Dogfood pass 2026-07-15 wrap-up recorded in the findings log's standing
notes: verified composer marks/paste/character-edit round-trips, chat
backgrounds (#9), chained-response render (#7), and the /files listing;
the next pass starts at text replacements, composition mode, drafts,
delete-with-associations, composer attach, and image generation. Docs
only.

Dogfood findings #10 and #11 recorded as NOT-A-BUG (v4-faithful,
oracle-verified): the composer-vs-message-renderer dialect mismatch
(`==highlight==` literal, `*word*` italic in sent messages — v4's own
renderer output is byte-identical) and the /files page scope + absent
upload control (v4's general files page behaves the same). Docs only.

Dogfood log bookkeeping: mark findings #7 (chained-response render),
#8 (composition mode), and #9 (chat backgrounds) FIXED and the
finding-#6 select audit CLOSED in
docs/developer/porting/dogfood-findings.md, matching the unified
P4.6ak∥al∥am round record. Docs only.

Unify P4.6ak∥al∥am (the D17 editor follow-ons + salon dogfood round):
ALL THREE orders CLOSED, and dogfood findings #7, #8, #9 and the
standing finding-#6 select audit CLOSE with them. The
text-replacement-rules surface + the chat story-background resolver
(server, differential-verified) ∥ strikethrough/highlight marks +
emphasis-on-type input rules + the shared qt-markdown-field (adopted
in the memory editor + character edit/new fields) + composition mode +
draft persistence + the text-replacement plugin and settings card ∥
the chained-response streaming render + chat background display + the
last dynamic-options select fix. Unification wires: the six new
dispatch types folded into the CoreRequest union (contract
cross-checked name-for-name against types.rs — no divergences); the
salon binds composition mode (persisted via chatUpdate
{documentEditingMode}) and the live text-replacement rules; the
background e2e beat went LIVE over a seeded story background; three
new live composer beats (composition mode incl. persisted-flag
reload, drafts, a live rule firing). Gate catches (fixed in the
gate-fix commit): the e2e instance predates the
text_replacement_rules migration table (materialized, the folders
precedent); the background files row needed sha256/source/linkedTo/
tags; the storage backend roots at <instance>/files, not
<instance>/data/files. Gate: 314 Rust suites / 1327 tests / 0 failed
with the two round differentials regenerated FRESH at `02865bdb` and
run by name (routes 15/15, tier-2 green, no SKIPs); clippy both
feature sets; ng test 764; ng build clean; full Playwright 52/52 with
zero skips. Final versions: core 0.0.221, harness 0.0.200, web
0.0.21, host 0.0.17, SPA 0.5.93.

P4.6am tier 2 (lane C) — the story-background e2e beat.
`salon-background-flow.spec.ts` walks unlock → open a salon chat →
`--story-background-url` lands on `.qt-chat-layout` from the resolved
id-keyed byte route → the `::before` layer stops being `display:none`
(the CSS actually draws the backdrop). The live `chatGetBackground`
dispatch is lane A's (wired at unification), so in-lane the beat
route-mocks the resolver + the file byte route — a green in-lane mock
beats a never-firing guard (the P4.6aj precedent); the mock is dropped
at unification. The chained-render (#7) e2e is NOT live-walkable (no
multi-responder LLM in the e2e host, per the order) — #7 is covered by
the `message-list` component specs + the reducer trace specs.

P4.6am unit 3 (lane C) — the last select-audit site (dogfood finding
#6, CLOSES the standing audit). The reverse-`{{user}}` dialog select in
`characters/view/tabs/details-tab.ts` bound the selection with `[value]`
on the `<select>` over dynamic computed options (`otherUserControlled()`
— a computed over the async `userControlledCharacters` input), so a
value bound before the options rendered left the native control blank.
Converted to per-option `[selected]` (the established finding-#6
pattern) + a regression spec that resolves the options AFTER first
render and asserts the stored selection still displays. The two
remaining `[value]` sites are confirmed proven-safe (STATIC options,
document-only): `characters/edit/details-tab.ts:165` (pronoun preset,
lane B's file) and `settings/providers/profile-modal.ts:489`
(modelClass). No dynamic-options `[value]` sites remain.

P4.6am unit 2 (lane C) — chat background images (dogfood finding #9).
The Salon now displays a chat's story background. The
`.qt-chat-layout::before` layer (opacity 0.45, fixed/cover,
hide-when-absent) was already ported byte-for-byte in `_chat.css`; the
missing piece was the data wiring. A small `story-background.api.ts`
resolves `chatGetBackground` (the §1 dispatch verb, mocked in-lane,
live at unification) into the CSS `url(...)` value — preferring the
returned `fileId` through the store-backed byte route
(`/api/v1/files/{id}`) over v4's path string. `salon-conversation.ts`
fetches it once per chat open (no 30s poll — that gates the unported
regeneration subsystem, not display) and binds it as
`--story-background-url` on the layout root, so the ported CSS draws
it. The `chatGetBackground` types land in the shared `core-contract.ts`
§2 append block; the request is bridged with a cast until the unifier
folds it into the `CoreRequest` union. New `story-background.api.spec`
+ two salon-conversation specs prove the resolver and the applied
style var.

P4.6am unit 1 (lane C) — chained-response streaming render (dogfood
finding #7). A chained character's finished reply is now visible the
instant its turn ends, instead of being held back until the whole
chain completes and the canonical refetch lands. The SSE reducer
already folded each intermediate done / carina answer / host
announcement into `state.messages`; the gap was render-side. The
message list now renders those accumulated finished bubbles (assistant
+ carina + host) below the canonical flow and above the live
in-progress bubble, deduped by id against the canonical list so the
reconcile handoff never doubles a row, through the same
MessageRow/AnnouncementGroup path so the bubbles look identical before
and after the refetch. Skipped turns render nothing (their Host note
still surfaces as a chip). New `buildStreamRenderItems` pure helper +
`message-list.spec.ts` drive the multi-turn chain (per-turn
visibility, dedup handoff, skip, carina) through the reducer and the
rendered DOM.

P4.6al lane: fixed the new-character spec to drive the Identity field
through the qt-markdown-field editor handle (the item-3 adoption removed
the `#identity` textarea id). Full `ng test` green (749).

P4.6al lane (D17 editor follow-ons, tier 2 item 5 — text replacement):
the composer's word-boundary autocorrect (v4 `TextReplacementPlugin`)
and its settings CRUD card. A new `editor/text-replacement.ts` ports
the compiled-rules helper (`compileRules`/`findReplacement`) and a
ProseMirror plugin with v4's exact trigger semantics — collapsed caret
at the end of the word, the v4 trigger-char set (newline excluded), IME
skipped, replace + trigger char in one undo. The plugin is composer-only
and inert unless the host passes compiled rules; the composer forwards
them gated by `textReplacementsEnabled`, both as inputs the salon wires
at unification. The Settings → Chat "Text Replacement" card (v4's
`TextReplacementSettings`) provides the master toggle, add-rule form,
per-row edit/toggle/delete, and the "Try it" preview, over new
dispatch verbs (`textReplacementsList`/`Create`/`Update`/`Delete`/
`BulkReplace`) added to core-contract's P4.6al block and a client api
file. Specs: text-replacement +9, the settings card +6, composer
gating +1.

P4.6al lane (D17 editor follow-ons, tier 2 item 4 — composer draft
persistence): the chat composer now saves and restores an unsent draft
per chat (v4 `useDraftPersistence` + `ComposerSyncPlugin`). Keyed
`quilltap-draft-${chatId}` in localStorage, restored once on mount
into the editor's initial value, saved on an 800ms debounce (blank
text removes the key), and cleared immediately on a successful send.
No expiry. Specs +4.

P4.6al lane (D17 editor follow-ons, tier 1 item 3 — the shared
form-field editor): a new `qt-markdown-field` (v4's
`MarkdownLexicalEditor`, "Designed for forms") pairs the composer
dialect editor with a ported `qt-formatting-toolbar` (bold / italic /
H1–H6 / lists / blockquote + a code-block toggle — v4's
`MARKDOWN_FORMATS` inventory; strikethrough/highlight remain
type-to-format marks, not toolbar buttons, matching v4). The field
swallows the editor's initial absorb-once normalization so it emits
only on genuine edits — the exact contract of the textarea it
replaces. Adopted in the memory-editor content field, the character
edit Details tab (all seven markdown fields), and the new-character
form (the same seven plus scenario). RichEditor gained a `runCommand`
handle and an `inCodeBlock` signal for the toolbar. The
`roleplayTemplateId`-aware toolbar delimiters are a named deferral
(no client-side template plumbing yet). Specs: qt-markdown-field +6,
plus the memory/character hosts kept green.

P4.6al lane (D17 editor follow-ons, tier 1 item 2 — composition mode,
dogfood #8): the composer now honors v4's `documentEditingMode`. The
rich editor gained a `submitOnModEnter` input (Cmd+Enter on Mac /
Ctrl+Enter elsewhere submits, plain Enter inserts a paragraph — v4
KeyboardPlugin's exact `isMac` branch); the chat composer gained a
`compositionMode` input, a `compositionModeChange` output, and a
toolbar toggle button (v4's two titles + active state), binding the
editor `[submitOnEnter]="!compositionMode"` /
`[submitOnModEnter]="compositionMode"`. The Settings → Chat tab gained
the "Composition Mode" card (v4's first) with the "Start New Chats in
Composition Mode" toggle, saved through the existing chat-settings
dispatch as `compositionModeDefault`. The salon binding
(documentEditingMode ↔ the composer) is a unification wire. Specs:
rich-editor +4 (submit modes), composer +1 (toggle), the new settings
card +3.

P4.6al lane (D17 editor follow-ons, tier 1 item 1): added
strikethrough (`~~`) and highlight (`==`) marks to the composer
markdown dialect and emphasis-on-type input rules. Strikethrough rides
markdown-it's own built-in `~~` rule; highlight is a hand-rolled `==`
inline rule modeled byte-for-byte on that strikethrough rule
(markdown-it ships no `==`). Both serialize with the same literal-tilde
protection v4's `preserve*` flags apply. The on-type input rules
convert `_italic_`, `**bold**`, `` `code` ``, `~~strike~~`, and
`==highlight==` when the closing delimiter is typed (CommonMark
flanking); single `*narration*` never auto-formats, and intra-word
underscores stay literal. The byte-round-trip gate grew 8 mark/edge
entries and the input-rule keymap spec grew 9; both green.

P4.6ak lane A (server): a loud typed refusal for the chat
`regenerate-background` action (`ChatRegenerateBackground`). v4's
story-background GENERATION subsystem (image-profile prompt build, the
30s poll loop) is a tier-3 deferral; the dispatch now answers a
recognized `not_available` refusal so the SPA gets a typed response,
not an unknown-action fallback. Version: core 0.0.221.

P4.6ak lane A (server): the text-replacement-rules surface + the chat
story-background resolver, both differential-verified against v4's real
route handlers. The Phase-2 `text_replacement_rules` repo gained
`list` (ordered sortOrder then createdAt), `bulk_replace` (the
transactional full-list replace), and a `find_by_id`/row projection;
five dispatch verbs (`textReplacementsList` / `textReplacementCreate` /
`textReplacementUpdate` / `textReplacementDelete` /
`textReplacementsBulkReplace`) over them, plus `chatGetBackground` (the
story-background resolver, all three arms + the chat-not-found 404).
REST edges at `/api/v1/settings/text-replacements`(`/:id`) and
`/api/v1/chats/:id?action=get-background` unwrap the dispatch envelope
to v4's raw route bodies. New committed fixture
(`text-replacements-{main,mount}.db`) + generator; a 15-case
`text_replacements_routes_equivalence` differential over a fresh
`02865bdb` oracle + a live web-edge integration test. Ported v4
quirks: `sortOrder` reads tolerant of REAL affinity (v4's `z.number()`
column), and the 404 message carries v4's doubled "not found not
found". Versions: core 0.0.220, harness 0.0.200, web 0.0.21.

Plan the D17 editor follow-ons + salon dogfood round — three
agent-ready work orders covering the P4.6ag tier-3 editor deferrals
and dogfood findings #7/#8/#9. Drift-checked first: v4 HEAD is still
`02865bdb` (no drift). The three lanes: P4.6ak (server — the unported
`text_replacement_rules` surface with five dispatch verbs + REST
edges, and `chatGetBackground` for the story-background resolver, both
with jest real-DB differentials over a new committed fixture), P4.6al
(SPA — strikethrough/highlight marks + emphasis-on-type input rules
extending the byte-round-trip gate, composition mode composer-side
[finding #8; the server storage is already ported], the shared
qt-markdown-field adopted in the memory/character-field textareas,
draft persistence, and the text-replacement plugin + settings card),
and P4.6am (SPA — finding #7's chained-response streaming render,
finding #9's chat background display over the new verb, and the last
finding-#6 select-audit site, which closes that standing audit). The
composer/salon seam (composition-mode + chatId bindings in
salon-conversation) is pinned as a unification wire; core-contract.ts
is shared via delimited append blocks. Docs-only.

Dogfood findings #7–#9 logged (docs only, no code changes): #7 chained
characters' finished responses don't render until the chain ends (the
streaming overlay never renders the reducer's accumulated intermediate
messages — port divergence, fix deferred to the next Salon slice); #8
the composition-mode toggle (Enter-inserts-newline) is an unported v4
feature, now a named deferral; #9 chat background images aren't
rendered by the Salon, now a named deferral.

Unify P4.6ah∥ai∥aj∥d4 (the "finish P4.6ae + catch up from v4" round):
all four orders CLOSED, and P4.6ae + P4.6ab (tier 2) close with them.
The files write + maintenance server remainder (chat-file upload +
link, general upload REST leg, the itemized FILE_HAS_ASSOCIATIONS
envelope + dissociate, three maintenance verbs) ∥ the
imageProfileGenerate un-refusal over the new
EngineAssembly.image_generation seam, wired live in the host ∥ the SPA
delete-associations close-out (dissociate-only — no v4 client sends
force) ∥ the 02865bdb skip-signal trailing-sentinel re-port. **The
oracle baseline is now v4 HEAD `02865bdb`.** Wires: contract diffed
name-for-name (no divergences); the P4.6af guarded files data beat
self-activated; a composer-attach live-leg beat added; the REST-edge
envelope leak fixed (see the gate-fix entry). Gate: 312 Rust suites /
1324 tests / 0 failed with the three round oracles fresh at
`02865bdb` and their differentials run by name (files-routes 41/41,
image-generate 4/4, skip-signal 106 rows); clippy both feature sets;
ng test 698; ng build clean; full Playwright 48/48 with zero skips.
Final versions: core 0.0.219, harness 0.0.199, host 0.0.17, web
0.0.20, SPA 0.5.83.

Unify gate fix P4.6ah∥aj: the files-family REST legs returned the
dispatch envelope (`{type, data}`) instead of v4's raw route bodies —
`core_response_to_http` now unwraps `Files`/`ChatMedia` payloads like
the existing `MountFile` arm, so `POST /api/v1/chats/{id}/files`
answers the LOCKED SPA client's `{file}` / `{duplicate,…}` shapes and
the upload/delete legs answer `{data: FileEntry}` / `{success: true}`
verbatim. (The bug slipped both lanes: the differential diffs at the
CoreRequest layer and the web-edge test's link/delete assertions sat
behind a fixture-dependent guard that never fired.) Also: the e2e
instance gained the `folders` table (schema materialization in
global-setup — the salon fixture predates the files family; the
self-activated files data beat reads it), and the composer-attach wire
beat discovers a GENERAL chat via the API (the fixture's project chats
have no linked document store, so their upload branch fails
v4-faithfully). Full Playwright 48/48, zero skips — the P4.6af files
data beat is ACTIVE.

Unify wire P4.6ah∥ai∥aj∥d4: accumulate the multi-lane version bumps
(core 0.0.219, harness 0.0.199 — three lanes each bumped from the same
base; host 0.0.17, web 0.0.20, SPA 0.5.83 stand), drop two stray
committed SQLite `-journal` fixture artifacts, and add the
composer-file-attach e2e beat over the now-live chat-file upload leg
(`POST /api/v1/chats/{id}/files`) — the one cross-lane proof neither
lane A (server, no SPA) nor lane C (SPA, no server leg in-worktree)
could run alone. The P4.6af guarded general-files data beat needs no
edit: its runtime probe covers the upload REST leg and self-activates
now that lane A landed it.

P4.6ah (files write + maintenance server lane) — complete the OPEN
P4.6ae server remainder. The chat-file upload leg (`uploadChatFile`
ported into `services/chat_files.rs` over the `file_storage.rs` write
seams: project dup-detect + skip/replace/keepBoth resolutions +
non-project sha-dedup; the `chat_media.rs` refusal body replaced) with
its web multipart route + the JSON `action=link` leg. The general
upload REST leg (`fileUpload` variant + `saveFileEntry`: sanitize →
sha256 → overwrite-reuse → project-store vs Quilltap-Uploads-mount
branch → 201 create / 200 overwrite) behind
`POST /api/v1/files?action=upload`. The itemized `FILE_HAS_ASSOCIATIONS`
delete envelope: `CoreError` gains an additive optional `associations`
field (+ `Response::error_with_associations`), the un-forced
linked-file delete emits the itemized `{characters, messages}` payload,
and the `dissociate=true` arm strips message attachments + character
default-image/avatar-override refs before deleting. The three
maintenance verbs: `filesGenerateThumbnails` (owned+resizable filter),
`filesCleanupStale` (mount-blob in-DB existence + enumerated disk-key fs
leg), `filesCleanupOrphans` (rescue/duplicate/unique partition + move
relocation). Differential: `files_routes_equivalence` extended to 41
cases (the 25 P4.6ae reads/moves/folders + 16 new write/maintenance/
delete cases) over the extended `files-{main,mount}.db` fixture, all
green against a fresh `02865bdb` oracle. Enumerated fs/codec legs
(dispatch-layer degradations): thumbnail generation (host codec, the
on-demand byte-GET route unaffected), cleanup-stale disk-key existence
(host backend), chat-image auto-describe (fire-and-forget host seam),
`filesSync` (unported reconciliation subsystem). Bumps core 0.0.217,
web 0.0.20, harness 0.0.197.

P4.6ai (lane B): un-refuse `imageProfileGenerate` end-to-end. Added a
new `EngineAssembly.image_generation` seam (the courier_resolve idiom —
struct field, `None` default, ready-state mirror, `ready_generate_image`
gate), threaded `prompt`/`chatId`/`count` through the engine arm, and
replaced the `api/image_profiles.rs` `not_available("generate")` refusal
with a handler that runs the already-ported W4.9a generation runner
(`execute_image_generation_tool`) via the seam and shapes v4's
`{success, data, expandedPrompt, metadata}` envelope (the 404 gate +
the `badRequest(result.error || 'Image generation failed')` arm). Wired
the seam LIVE in `quilltap-host` (a per-run `HostImageGenerationRunner`
over the W4.7f `Real*Provider`s — the avatar/story job-handler idiom, so
`now_ms` + the cheap-LLM log context are per-request); spine-less
assemblies keep `image_generation: None` → the loud not-assembled
refusal. `imageProfileValidateKey` / `imageProfileListModels` stay
refusal-armed. Differential: a new `image_generate_route_equivalence`
diff drives the ported handler with the image provider canned on both
sides and byte-matches v4's real `[id]/route` generate envelope across
four cases (happy+chat, no-chat, `count`>1 → 2 images, profile 404).
Bumps core 0.0.217, host 0.0.17, harness 0.0.197.

P4.d4: re-port the v4 `02865bdb` skip-signal drift — `detectSkipSentinel`
now strips a trailing lone sentinel line from an otherwise real turn.
Weak models sometimes narrate a genuine turn and then tack `[NOTHING TO
ADD]` on the end; the narration is kept, but the dangling sentinel must
not survive into display, persistence, or memory. Restructured
`detect_skip_sentinel` to mirror v4: the sentinel-first arm is unchanged
(bare → skip; sentinel + prose → cleaned); when the first non-empty line
is prose, it now walks to the LAST non-empty line and, if that line is a
lone sentinel, drops it and keeps the prose above. Bare sentinels and
mid-line mentions of the phrase are unaffected. Seven new `detect` oracle
rows + four Rust unit tests; the tier-1 differential is 106 rows green
over a fresh `02865bdb` oracle. Byte-exact leaves (`is_sentinel_line`,
`js_trim`, `utf16_len`) untouched. Moves the oracle baseline to
`02865bdb`.

P4.6aj (lane C): the files SPA delete-associations close-out. Fresh
SPA survey found the itemized dialog + the two-stage delete flow
already landed and v4-faithful (P4.6af): v4's FileBrowser offers ONE
action — "Delete Anyway" → `?dissociate=true`. No v4 client UI sends
`force` (the `force` query param exists server-side but no front-end
uses it), so the order's "force + dissociate two-button" mandate was
corrected to v4's dissociate-only reality (reduced v4-faithful scope,
user-approved — the force button was NOT invented). Tier 1: the
missing dedicated `file-delete-confirmation.spec.ts` (renders the
itemized characters + messages; hides an empty section; emits
confirm/cancel; disables + relabels while deleting) plus the "clean
delete (no associations) skips the dialog" case in
`files-browser.spec.ts` — `ng test` 698. Tier 2: a route-mocked
delete-associations e2e beat in `general-files-flow.spec.ts` (bare
delete → itemized dialog → "Delete Anyway" → dissociate resend → the
file drops from the refetched list), mocked because v4's
`serializeFileEntry` omits `linkedTo` so the list can't reveal a
linked file for a self-activating fixture beat — full Playwright 46
passed + 1 skip. Composer-attach + generate live paths verified at the
unit level (`chat-composer.spec.ts`, `generate-image-dialog.spec.ts`);
end-to-end activation over lanes A/B lands at unification.

Plan the "finish P4.6ae + catch up from v4" round — four agent-ready
work orders decomposing the P4.6ae OPEN server remainder plus the one
v4 drift commit. Drift-checked first: v4 advanced one commit past
baseline `dd0d9ff5` to `02865bdb` ("strip a trailing 'nothing to add'
line from an otherwise real turn"), an isolated `detectSkipSentinel`
behavior change touching none of the files-family/image-gen surface.
The four lanes: P4.6ah (the chat + general upload legs, `action=link`,
the maintenance verbs, and the itemized `FILE_HAS_ASSOCIATIONS`
envelope + `dissociate` arm), P4.6ai (the `imageProfileGenerate`
un-refusal + a new `EngineAssembly.image_generation` host seam),
P4.6aj (the SPA delete-associations dialog + composer/generate
live-path verification), and P4.d4 (the `02865bdb` skip-signal
re-port, which moves the oracle baseline). Lanes A+B share
`types.rs`/`engine.rs` via delimited blocks (the two-core-dispatch-
writer rule); lane C owns the SPA client; lane D is file-disjoint.
Docs-only.

Unify the P4.6ae ∥ P4.6af ∥ P4.6ag files-family + editor round onto
main. P4.6af CLOSED (the /files general-files SPA vertical + the two
salon autonomous riders); P4.6ag CLOSED — D17 DECIDED: ProseMirror
ADOPTED (gate GREEN), the qt-rich-editor ships in the Document Mode
pane and the chat composer; P4.6ae stays OPEN (partial): the nine
general-files dispatch verbs landed with the 25-case differential, but
the P4.6ab tier-2 close-out (chatFileUpload, imageProfileGenerate),
the upload REST leg, thumbnails/cleanup, and the itemized
FILE_HAS_ASSOCIATIONS envelope remain. Gate: 310 Rust suites / 1318
tests (files differential regenerated fresh at dd0d9ff5), clippy both
feature sets, ng test 691, ng build clean, Playwright 45 passed / 1
guarded skip. Final versions: core 0.0.216, harness 0.0.196, web
0.0.19, host 0.0.16, SPA 0.5.82.

P4.6ae∥af∥ag unification wire: the general-files e2e data beat's
self-activation guard now also covers the `?action=upload` REST leg
(P4.6ae unit 4, still OPEN) — with lane A's dispatch verbs live but the
upload leg unported, the beat would have failed on a 404 seed instead of
skipping; it now skips cleanly and self-activates when the leg lands.
Contract reconciled name-for-name across `core-contract.ts` (lane B) and
the `types.rs` P4.6ae block (lane A) — no divergences. Also settles the
terminal-flow walk's chip-count baseline (the virtualized list mounts
asynchronously; the extended documents walk grew the shared chat's
history enough that a too-early snapshot read 0 and the stale chip then
mounted with the post-spawn refetch — the baseline now waits for two
agreeing reads a beat apart). SPA 0.5.82.

P4.6ag tier 2 (unit 6): live e2e beats for the rich editor. The
salon-documents-flow walk edits markdown in the ProseMirror pane (not a
textarea), plus two new beats — a dialect round-trip (type a `#` heading via
the input rule + literal `*`/`_` roleplay punctuation, save, toggle to raw
source, assert the exact v4 dialect bytes) and a composer send-bytes beat
(type `_softly_` + a literal-`*` narration line, send, assert the outgoing
chatSend content). The m4-salon composer interaction is updated to drive the
contenteditable (the composer swap). Full doc-flow + m4 walks pass live
against the real server. SPA 0.5.77.

P4.6ag tier 2 (unit 5): markdown input shortcuts + the formatting command
set for the rich editor (v4 FormattingCommandPlugin + Lexical
MarkdownShortcut, within the transformer scope). Type-as-you-go rules for
`# ` headings, `> ` blockquote, `- `/`1. ` lists, and ``` code fences;
keybindings for bold (Mod-b), underscore italic (Mod-i), inline code, list
Enter/indent/outdent, headings, and blockquote. No inline-emphasis input
rule — a typed `*narration*` stays literal (dialect quirk #6). No toolbar
(v5 has none today). SPA 0.5.76.

P4.6ag tier 1 (unit 4): the Salon composer adopts the rich editor. The
message box is now `qt-rich-editor` in chat mode (Enter sends, Shift+Enter
a line break); the send reads the markdown from the editor handle at submit
time (v4's decoupled ComposerSyncPlugin posture), so a user-typed
`*narration*` survives literal. `hasContent` send-gating, paste-image
upload (+ the duplicate-conflict resolver), and the `ComposerSend` payload
are unchanged; `salon-conversation` is untouched. SPA 0.5.75.

P4.6ag tier 1 (unit 3): Document Mode adopts the rich editor for markdown
files. `USES_RICH_MARKDOWN_EDITOR` flips true, so `.md`/`.markdown` files
edit in `qt-rich-editor` (frontmatter split + body-only editing + rawBlock
recombine unchanged); everything else stays a plain textarea. A header
source-toggle (v4 `showSource`) drops any markdown file back to a raw
textarea. The save/mtime-conflict path is untouched. The first
re-serialization after a load is absorbed as baseline (v4
`computeAbsorbNext`, now live) — specced in document-mode.spec. SPA 0.5.74.

P4.6ag tier 1 (unit 2): the `qt-rich-editor` component — a ProseMirror
view hosted imperatively over the dialect bridge, with markdown in via
`value` and out via an imperative handle (`focus`/`getMarkdown`/
`setMarkdown`/`prependText`, mirroring v4's `ComposerEditorHandle`).
History, base keymap, an empty-doc placeholder, a paste-image passthrough,
and a `submitOnEnter` mode (Enter emits submit, Shift+Enter a line break —
v4 KeyboardPlugin chat mode). External `value` changes reload the document
and emit the normalized markdown once (the absorb-once seam). SPA 0.5.73.

P4.6ag tier 0 (the D17 gate): the committed markdown round-trip test
that decides whether ProseMirror can replace v4's Lexical editor. A new
`apps/web/src/app/editor/markdown-dialect.ts` configures a ProseMirror
schema/parser/serializer for v4's composer dialect (underscore italic,
`**` bold, single `*`/`_`/backtick/`~` literal and unescaped, headings,
blockquotes, ordered/unordered/check lists, fenced code, soft line
breaks as `\n`) and `markdown-round-trip.spec.ts` asserts
`serialize(parse(x)) === x` over a 28-entry corpus (each entry traced to
a v4 transformer or preserve flag). The gate runs GREEN, so adoption
proceeds. SPA 0.5.72.

P4.6af unit 7: the e2e walks. NEW `salon-autonomous-entry.spec.ts`
(LIVE — 3 beats over the shared Salon server: a seeded cron room is
hidden by default with the hint shown, the "Show Autonomous Rooms"
toggle reveals it, the "New Autonomous Room" action links to
`/salon/new?autonomous=1`, and the header Edit-Enclave button opens the
frozen modal and round-trips a title save). NEW
`general-files-flow.spec.ts` (the /files render beat is LIVE; the seed
→ browse → preview data beat probe-guards on lane A's files-family
variants and self-activates at unification). Renamed from the order's
`files-flow.spec.ts` so it sorts AFTER foundation ('files' would sort
before 'foundation' and pre-empt the gate walk). SPA 0.5.75.

P4.6af unit 5: the salon autonomous riders. (a) The conversation header
gains an Edit-Enclave button gated on `chatType === 'autonomous'` (v4's
ChatSidebar "Organize" entry — label/tooltip verbatim; PLACEMENT
DIVERGENCE: v5 has no chat sidebar, so it rides the header's right
cluster), wired in salon-conversation to the existing
`qt-edit-enclave-modal`; the chat refetches on save. (b) The salon list
gains a "Show Autonomous Rooms" toggle (persisted to the shared
`quilltap.quickHide.includeAutonomousRooms` localStorage key; the flag
rides the query key so flipping it refetches), a hidden-rooms hint (the
cheap `listAutonomousRooms` probe fires only when excluding), and a "New
Autonomous Room" action → `/salon/new?autonomous=1`. Effective include =
the user's visibility default OR the toggle. SPA 0.5.74.

P4.6af unit 2: the general Files page. NEW `/files` screen (wired into
the shell's Files nav) ports v4's legacy-mode FileBrowser: folder
breadcrumb + go-up + subfolders derived from BOTH the DB folder rows
AND file-path prefixes, grid and list views with client-side sort
(default name-asc; the server list is createdAt-desc, so the client
re-sorts), a file preview lightbox (image / plain-text-with-copy /
pdf-download / metadata-fallback renderers, ←/→ + Esc nav), and the
Create-Folder / Move-to-Project / associations-aware Delete /
Orphan-Cleanup dialogs. The two-stage delete surfaces the
FILE_HAS_ASSOCIATIONS envelope as a dissociate confirmation; the sync
button renders lane A's loud `filesSync` refusal faithfully; a
fire-and-forget thumbnail batch fires on list load. NO upload
affordance (v4 parity). Deferred loud: markdown/syntax-highlight/
wikilink text preview + pdf.js rendering (lane C's dependency
territory), the rich FolderPicker, drag relocation. SPA 0.5.73.

P4.6af unit 1: the general files-family wire contract. lane B (the
`core-contract.ts` / `core-client.ts` owner) authors the files-family
Request variants (`filesList`, `fileMove`, `filePromote`,
`fileDelete`, `filesGenerateThumbnails`, `filesCleanupStale`,
`filesCleanupOrphans`, `filesFoldersList`, `filesFolderCreate`,
`filesFolderRename`, `filesFolderDelete`, `filesSync`, `fileUpload`)
plus the `FileEntry` / `FolderEntry` / `FileAssociations` DTOs, and
the CoreClient read helpers (`filesList` / `filesFoldersList` /
`filesGenerateThumbnails`). Names transcribed verbatim from the
p4.6af Shared contract; the server side (p4.6ae) pins the response
envelopes at unification. SPA 0.5.72.

P4.6ae units 2+3+5 (files-family server): the general files dispatch
surface goes live over `/api/dispatch` — `filesList`, `fileMove`,
`filePromote`, `fileDelete`, `filesFoldersList`, `filesFolderCreate`,
`filesFolderRename`, `filesFolderDelete`, and the loud `filesSync`
refusal. A new `api/files.rs` ports v4's `app/api/v1/files/**` route
logic (both file-response shapes; the read-marshaling null-drop vs
mutation null-echo distinction; the empty general-folders list; the
`{...existing,...patch}` move/promote merge). Proven by the new
`files_routes_equivalence` differential (25 cases) over the committed
`files-{main,mount}.db` fixture, driving v4's real handlers. Deferred
loud: the `FILE_HAS_ASSOCIATIONS` itemized-associations wire payload
(the shared `CoreError` has no field and widening it touches out-of-lane
host code — the computation is ported + verified directly) and the
`dissociate=true` delete arm. Versions: core 0.0.216, harness 0.0.196.

P4.6ae unit 1 (files-family server): the db-layer + folder-path leaves
the general files surface needs. `db/files.rs` gains the `FileFull`
projection and `find_full_by_id` / `find_by_user_id` /
`find_general_files` / `find_by_project_id` / `find_by_filename_in_scope`
/ `find_by_filename_in_project` reads plus the move/promote update that
can set `projectId = NULL`; `db/folders.rs` gains the `FolderRow`
projection and `find_by_user_id` / `find_by_path` / `update_path_prefix`
/ `has_children` / `delete`; `folder_utils.rs` completes the hierarchy +
validation half (`get_folder_depth`, `get_parent_path`, `get_folder_name`,
`validate_folder_path`). Scaffolding — the route differential arrives with
the dispatch surface. Version: core 0.0.215.

Unify the P4.d3 db-size-reduction drift re-port onto main; the oracle
baseline is now v4 dd0d9ff5. Embedding blobs are read header-aware
(legacy Float32 + int8 + f16) and written int8-quantized
byte-identically to v4 (~4x smaller); the daily maintenance sweep
collapses stale chats' regenerable caches and cold-tiers their chunk
embeddings (window set by the new Data Retention setting); opening a
cold chat re-enqueues its chunk embeddings; the Settings Chat tab
gains the Data Retention card. 15 differentials regenerated fresh at
dd0d9ff5 (two NEW); full gate green (309 Rust suites, ng 622,
Playwright with the retention beat active). Versions: core 0.0.214,
harness 0.0.195, SPA 0.5.71. Still deferred loud: the
EMBEDDING_GENERATE execution handler, EMBEDDING_REAPPLY_PROFILE, the
backup-service leg, db optimize parity.

P4.d3 unit 6: Settings → Chat gains a Data Retention card — set how
many days (1–3650, default 30) an inactive chat keeps its regenerable
working data before the nightly sweep tidies it. The window autosaves
and reads back live.

P4.d3 unit 5: a new instance-wide Data Retention setting exposes the
stale-chat window (staleChatDays, default 30, bounded 1–3650) over a
GET/PUT dispatch surface, with validation of out-of-range or
wrong-typed input. Every stale-gated maintenance sweep reads this one
value.

P4.d3 unit 4: opening a chat whose embeddings were cold-tiered now
transparently re-warms it — cold conversation-chunks are re-enqueued
for embedding through the standard pipeline (debounced per chat,
deduped per chunk), so semantic search recovers on its own without any
manual step. Enqueue-only; loading the chat is never slowed or broken.

P4.d3 unit 3: the daily maintenance sweep now collapses stale chats'
regenerable data — compression/render caches, raw provider payloads,
model thinking traces, and pre-rendered HTML — and cold-tiers their
conversation-chunk embeddings (keeping the text for keyword search),
all guarded and idempotent. The stale-chat window is resolved through
the new Data Retention setting (default 30 days) so every stale-gated
sweep agrees on "stale". Nothing that matters — messages, memories,
opaque content — is ever touched.

P4.d3 unit 2: the embedding differential suite is re-baselined to the
quantized on-disk format — every equivalence test that stores an
embedding now checks the compact int8 blob against v4. Test-only.

P4.d3 unit 1: the embedding BLOB codec now reads and writes v4's
self-describing quantized format. Stored embeddings shrink ~4× (int8
symmetric quantization with a per-vector scale; float16 fallback
supported), and the header-aware reader keeps legacy raw-Float32 blobs
readable, so an instance can hold both formats during and after v4's
quantization migration. Writes match v4 byte-for-byte.

Drift-amend the files-family + editor round to four lanes: v4 moved
to dd0d9ff5 (4.8.0-dev.52, DB size reduction — int8-quantized
embedding blobs, stale-chat cache collapse, cold-tier chunk
embeddings, dataRetention). New work order P4.d3 re-ports the drift
(the header-aware quantized codec is load-bearing: v5 cannot read
post-migration blobs without it) and owns the affected-differential
regen batch; all four orders re-pinned to baseline dd0d9ff5;
CLAUDE.md carries the banked-drift note and the back-up-Friday
caution. Docs only.

Plan the files-family + editor round: three work orders committed
(P4.6ae the files-family server surface + the P4.6ab tier-2 close-out,
P4.6af the general Files SPA + the salon autonomous riders, P4.6ag the
D17 ProseMirror editor decision lane). Docs only — four fresh v4
surveys at baseline 6a8a77aa inform the orders; no code changes.

Unify the P4.6ab/P4.6ac/P4.6ad round + the two terminal-probe branches
onto main. The courier + chat-images dispatch surface (resolve/cancel
external turn, save-image, photo-albums, add-tool-result, chat-files
list/delete) over the new committed courier-images fixture; the whole
courier + images Salon SPA (Courier bubble, thumbnails + lightbox,
markdown store-image rewrite, SaveImageDialog, PhotoGalleryModal, the
generate dialog, composer attach + conflict flow); the full
autonomous-rooms vertical (seven dispatch verbs over the frozen
enclave lifecycle, the Settings Chat tab's two autonomous cards,
EditEnclaveModal, the New-Chat toggle, shell run-state badges); and
the live terminal liveness probe (chat GET no longer falsely retires
live PTY sessions). Unification wires: the host's ChatSpine now backs
courier resolve (thread-bridge driver) and save-image bytes
(ProductionFileBytes); imageProfileGenerate's params reconciled to the
Shared-contract shape (still refusal-armed — the un-refusal and the
chat-file multipart upload leg are P4.6ab tier 2, OPEN); the courier
e2e beats activated by seeding lane A's fixture chats into the shared
walk instance (pinned ids remapped — the two fixture families collide
— and vault mounts copied along); the e2e instance gained its missing
llm-logs partition (a committed empty fixture db) so autonomous turns
can log. Gate: 307 Rust suites / 1294 tests green incl. three
fresh-oracle differentials by name (courier-images 15, autonomous 24,
salon-reads), clippy -D warnings both feature sets, ng test 618,
full Playwright 38/38 with every new beat ACTIVE. Versions: core
0.0.210, harness 0.0.191, web 0.0.19, host 0.0.16, SPA 0.5.70.

P4.6ab (lane A, courier + chat-images server surface): the Salon's
courier and image dispatch verbs are now differential-proven against
v4's real route handlers. New `api::chat_media` module wraps the
already-ported services: resolve/cancel external turn (courier),
save-image (behind the injected file-bytes seam), photo-albums,
add-tool-result (the Prospero-authored generate_image recorder), and
the chat-files list/delete. New `courier-images-{main,mount}.db`
fixture family + the `courier_images_routes_equivalence` differential
(15 checks green over a fresh v4 `6a8a77aa` oracle). Two new
EngineAssembly seams (courier-resolve driver + save-image bytes),
`None` until the P4.6ac unification wire (loud refusal / EMPTY_bytes
fallback meanwhile). OPEN under the order (loud deferrals): the
chat-file multipart upload leg and the imageProfileGenerate un-refusal.
Survey correction: `blobMountPointId` is a dead prop in v4 (no route
emits it) — the additive chat-GET echo is a no-op and was NOT added.
P4.6ad (SPA half): the autonomous-rooms vertical goes live in the
Angular SPA. The Settings → Chat tab is fitted out (replacing its
placeholder) with the two v4 autonomous CollapsibleCards — "Autonomous
Rooms" (the user defaults, autosave-on-blur into
`chat_settings.autonomousRoomSettings`) and "Scheduled Autonomous Rooms"
(the management list: 5s poll, Start/Pause/Resume/Stop/Edit with
optimistic run-state patch) — deep-linkable via
`?tab=chat&section=autonomous-rooms`; the other 13 Chat-tab cards are
named as a loud deferral. A shared `qt-autonomous-room-card` editor
(cron, freshness, the four budget caps, "Count only the dear tokens",
visibility, destructive-clamp) backs both the Edit-Enclave modal
(ms⇄human round-trip, `clampedDestructive` surfaced) and the New-Chat
autonomous toggle. New-Chat now enables autonomous mode (state slice +
Reality-Injection⇄editor swap + the user-controlled incompatibility
note), maps the exact v4 create payload (hours×3_600_000,
minutes×60_000, caps only when >0, `budgetExcludeCacheHits` always), and
navigates to the settings management list on success. Toolbar run-state
badges (5s poll + 1s client tick, tokens→turns→time readout, inline
play/pause) mount in the shell. The contract gained a lane-C
`AutonomousRoomRequest`/`autonomousRoom` block + seven CoreClient
methods. Live e2e (`settings-autonomous-flow.spec.ts`): the tab renders,
the New-Chat toggle swaps in the editor, and a dispatch-seeded cron room
walks list→start→pause→resume→edit(clear a cap)→stop. Loud deferrals:
the live next-run cron preview before save (validates shape only; the
server computes scheduleNextRunAt on save), and the Salon in-chat
Edit-Enclave entry + salon-list toggle (lane B). Bumps the SPA 0.5.62.

P4.6ad (server half): the autonomous-rooms dispatch surface. New
`api/autonomous_rooms.rs` wraps the frozen `enclave::lifecycle`
run-control core behind seven Request variants — `systemAutonomousRooms`
(the user-scoped listing, sorted running→idle→paused→budgetExhausted→
stopped→error then updatedAt desc, with the projectName join),
`chatAutonomousRoomStatus`, and the `Start`/`Pause`/`Stop`/`Resume`/
`UpdateSettings` verbs. Preserves v4's envelope quirks: start/resume
distinguish chat-not-found (404) while pause/stop answer every failure
400; update-settings guards the chat first then routes invalid-cron to
400; the update caps are nullish (explicit null clears, absent leaves);
`clampedDestructive` echoes the user's `always_refuse` ceiling. The
cron seam resolves the host local zone via jiff. Verified by the new
`autonomous_rooms_routes_equivalence` differential (24 cases over a new
committed `autonomous-{main,mount}.db` fixture: listing sort/ownership,
status defaults, start/resume enqueue tier-2, pause/stop/update
structural row diffs, invalid-cron/non-autonomous/missing arms, the
destructive clamp both ways). No `ChatCreateRequest` change needed —
it already carries every autonomous field (verified). Bumps
quilltap-core 0.0.208, quilltap-harness 0.0.189.
P4.6ac (Salon SPA, lane B): the generate-image dialog. A composer
sparkles button opens GenerateImageDialog (chat mode): a prompt with
`{{Character}}` quick-inserts for the chat's characters, generating
against the chat's image profile via `imageProfileGenerate` and
recording the result via `chatAddToolResult`. It degrades loudly (v4
voice) when no image profile is configured or while lane A's generate
arm is still refusal-armed. The `imageProfileGenerate` contract variant
was reshaped to the Shared-contract params. The
StandaloneGenerateImageDialog + ImageProfilePicker (explicit-profile
path) and auto-attaching generated images to the next message are
deferred (loud). SPA 0.5.66.

P4.6ac (Salon SPA, lane B): the composer attach affordance and the
courier/images e2e walk. The composer gained an attach button + hidden
file input + paste-image handler that upload to the chat-files
multipart leg (`POST /api/v1/chats/{id}/files`), showing attached-file
chips (removable) and the duplicate-conflict resolver (Replace / Keep
Both / Skip); the attached file ids ride the send (`chatSend.fileIds`).
Added a probe- and fixture-guarded Playwright walk
(`salon-courier-images-flow.spec.ts`) covering the courier bubble
(render → cancel settles) and the thumbnail → lightbox flow; it
discovers its fixture chats by content and skips until lane A's
dispatch + fixture merge. SPA 0.5.65.

P4.6ac (Salon SPA, lane B): the save-to-album dialog and the in-chat
photo gallery. The message action bar gained a Save button (shown when
a message carries image attachments) that opens SaveImageDialog — an
album picker grouped by kind (character/project/document-store/general)
over `chatPhotoAlbums`, with an optional caption, saving via
`messageSaveImage`. The conversation header gained a gallery button that
opens PhotoGalleryModal (chat mode): a thumbnail grid of the chat's
image files (`chatFilesList`) with a size control; clicking a thumbnail
opens the shared ImageModal lightbox. `AlbumOption` corrected to v4's
shape. SPA 0.5.64.

P4.6ac (Salon SPA, lane B): the Courier bubble and the in-chat image
lightbox. A pending manual/clipboard turn (`pendingExternalPrompt`)
now renders a Courier bubble in the message row — copy the delta or
full-context prompt, download referenced attachments, paste the reply
back to settle (`messageResolveExternalTurn`) or cancel
(`messageCancelExternalTurn`), skipping the normal action bar/danger
chrome. Image attachments render 80px thumbnails (the id-keyed
`/api/v1/files/{id}?action=thumbnail` route) that open an ImageModal
lightbox: save to a character's gallery (`characterPhotoSaveById`),
download, copy, delete (`chatFileDelete`). The courier/images request
variants were authored into `core-contract.ts` (lane A pins their
response envelopes). SPA 0.5.63.

P4.6ac (Salon SPA, lane B): ported the markdown store-image rewrite.
Rendering a message with a `blobMountPointId` now rewrites relative
image refs (`![](images/x.webp)`) to the chat's mount-point blob route,
matching v4's client `MessageContent` img override; absolute,
protocol-relative, `data:`, and `/`-rooted srcs pass through. The
rewrite runs last on the finished HTML (the roleplay post-processors
leave `<img>` tags untouched). `MessageContent` gained a
`blobMountPointId` input and the render cache keys on it. Dormant until
a producer emits the field (as in v4). SPA 0.5.62.

Planned the P4.6ab/P4.6ac/P4.6ad round (docs only): three work orders
written — the courier + chat-images server surface (P4.6ab), the
courier + images Salon SPA (P4.6ac), and the autonomous-rooms
vertical (P4.6ad, server marshaling + SPA in one lane). Shared
contract pins the new dispatch variant names (courier pair,
save-image/photo-albums, add-tool-result, chat-files family,
imageProfileGenerate un-refusal, the seven autonomous-room verbs);
lanes A and C are the round's two core-dispatch writers. Files-family
and D17 ProseMirror surveys banked in phase-4.md for the next round.
No code changes; no version bumps.

Chat GET now reconciles terminal sessions with the real live-PTY probe
(the P4.2-era stubbed `is_live = |_| false` deferral is closed). New
`TerminalLivenessProbe` trait rides `EngineAssembly::terminal_probe`
(the memory_embedding / mount_refresh seam idiom); the host's
TerminalManager implements it over its live-session map, matching v4's
`ptyManager.get` in lib/terminal/reconcile.ts. Fixes the live-server
bug where every chat load falsely retired live PTY sessions (exitedAt
minted, exitCode NULL) and posted a spurious Ariel "terminal closed"
chip ~30ms after every spawn — which also broke the session picker and
re-attach (the DB lied about liveness). Unwired assemblies (read-only
embedders, no terminal subsystem) keep the empty-map behavior, which is
v4 parity. The terminal e2e walk grew real beats: a closed-chip count
guard against the spurious chip, kill-then-re-attach through the
session picker (the surviving session is only listable with the real
probe), and a typed `exit` for a REAL session-closed announcement;
salon-documents-flow now genuinely ends its shell before its unwind
kill. Verified: salon-reads differential green against a fresh v4
oracle (probe-less parity), new host PTY probe test, full Playwright.
Versions: core 0.0.208, host 0.0.16, harness 0.0.189, SPA 0.5.62.
(Note: the unmerged sibling branch claude/admiring-shtern-894679 /
417566c also bumps SPA to 0.5.62 with a spec-only in-suite fix; this
branch's spec rewrite incorporates that commit's opened-chip
count-baseline gesture, so unification can subsume it.)

Fix the terminal-flow in-suite e2e failure (the follow-up flagged in
the 6a8a77aa re-port gate): the spec's "terminal opened" chip gesture
raced the post-spawn refetch — with stale chips left in the shared
fixture chat by salon-documents-flow, `.last()` resolved instantly to
the stale chip and the expanded embed bound the WRONG session
(trace-proven: embed WS vs pane WS on different session ids). The spec
now snapshots the pre-spawn chip count and waits for it to grow before
clicking. The embed's same-session detection was correct. The
diagnosis surfaced two real server-side findings, recorded in the
status log and ordered as follow-ups, not fixed here: (1) chat_get
runs the terminal reconcile with the stubbed `is_live = |_| false`
probe (the tracked deferral), so on the live server every chat load
falsely marks live PTY sessions exited and posts a spurious Ariel
"session-closed" announcement ~30ms after every spawn; (2) the pane's
kill sends SIGTERM, which an interactive zsh ignores (v4 sends the
same signal — parity, banked). Full Playwright 33/33 in-suite.
SPA 0.5.62.

Drift re-port (v4 6a8a77aa): nudge is now a persisted Host
announcement, matching v4. New writer helpers (buildNudgeContent /
buildNudgeOpaqueContent / postHostNudgeAnnouncement) in
services::host_notifications; the orchestrator posts the announcement
once per summon (guarded by continue-mode + the nudge flag) and
surfaces it live on the hostAnnouncement frame; the SPA labels the
chip "invited to speak" at medium importance. v4's parallel removal of
the ephemeral-message subsystem needed no v5 change (never ported).
Verified: the post-office-host tier-1 differential extended with the
nudge builders (byte-exact) and the orchestrator tier-3 differential
regenerated at 6a8a77aa (the corpus's continue+nudge call now
exercises the announcement end-to-end); post-office-writers and
context-feeders oracles regenerated fresh, all green. Oracle baseline
rebased a7b1398d → 6a8a77aa (no other v4 commits). Playwright 32/33:
the one failure (terminal-flow, in-suite only) reproduces identically
at the pre-change HEAD and is tracked as its own follow-up. Versions:
core 0.0.207, harness 0.0.188, SPA 0.5.61.

P4.6z/P4.6aa unification: the Scriptorium-SPA round is UNIFIED on
main — both orders CLOSED, D18 DECIDED (ngx-explorer spike green on
its gating checks but rejected for adoption — no move/copy verb, a
second theming engine; the bespoke qt-file-manager shipped over the
ported v4 adapter helpers). On main: the /scriptorium +
/scriptorium/:id vertical (grid, five dialogs, DirectoryPicker,
FileTable), the systemBrowseDirectory dispatch variant with its route
differential, the qt-file-manager widget behind the store detail's
"New file manager (beta)" toggle, and the dogfood-#6 select-audit (2
conversions, 7 proven safe). Full gate at unification: fmt/clippy
both feature sets clean, 305 Rust suites 0 failed, browse-directory
differential fresh-green from v4 a7b1398d, release build, ng test 546,
ng build, full Playwright 33/33 with the file-manager walk ACTIVE.
Deferred loud: the /files files-family page (server surface unported),
FilePreview, workspace-tab drill, cross-mount move/copy UI, drag
relocation. v4 drift noted for the next round: 6a8a77aa (nudge → a
persisted Host announcement) needs a drift re-port at baseline rebase.
Versions: core 0.0.206, web 0.0.18, harness 0.0.187, host 0.0.15,
SPA 0.5.60.

P4.6z/P4.6aa unification gate fixes (e2e only): the file-manager walk's
spec renamed file-manager-flow → scriptorium-file-manager-flow — the
old name sorted BEFORE foundation.spec ('i' < 'o') and the activated
probe's unlock deterministically broke foundation's locked-screen
start (misread as machine-load flake in the lane gates); the store
seeding moved from beforeAll to after the unlock (a dispatch against
the locked vault refuses, so the beforeAll seed mis-skipped the walk
in isolation); and the copy-folder refusal beat now pastes INSIDE the
copied folder — a paste in place is the widget's silent no-op guard
(the unit spec pastes into a different folder too), which masked the
refusal. Full Playwright 33/33 with the file-manager walk ACTIVE.
SPA 0.5.60.

P4.6z/P4.6aa unification wire: the store-detail Indexed-Files section
gains the "New file manager (beta)" / "Classic view" toggle rendering
the deferred qt-file-manager over the server-derived capability bag
(the classic FileTable stays the default) — the contract-§4 seam
neither lane could land in-lane; and MountCapabilities is deduped to
core-contract's identically-shaped MountPointCapabilities (the
sanctioned flip, shape unchanged). SPA 0.5.59.

P4.6z unit 3 (lane A): the live Scriptorium e2e walk
(`e2e/scriptorium-flow.spec.ts`) over the shared Salon server — a database
store (create → upload a blob → expand → edit description → delete file →
delete store), a filesystem store (create on a temp dir → scan → the markdown
file indexes), and the convert refusal (the live-guarded verb answers the typed
refusal, surfaced in the flash banner). The old Salon fixture predates
`embedding_profiles`; the scan's embedding enqueue reads that table, so
`global-setup.ts` now materializes an empty `embedding_profiles`
(`CREATE TABLE IF NOT EXISTS`, the terminal_sessions/chat_documents precedent —
schema materialization, not a fixture regen) so the enqueue skips gracefully.

P4.6z unit 2 (lane A): the Scriptorium SPA vertical. New `/scriptorium`
document-stores screen (card grid + create/edit/delete/convert/deconvert
dialogs + a server-backed DirectoryPicker over `systemBrowseDirectory` + scan
with spinner) and `/scriptorium/:id` store detail (header, four info cards,
scan-error panel, include/exclude pattern cards, and the classic FileTable with
sort/filter, expandable blob detail, description editing, per-file delete, and
database-mount multipart upload). The nav's Scriptorium item is now live. The
store list mirrors v4's patch-not-refetch shape (create prepends, update
replaces, delete filters, scan/convert/deconvert re-GET the one store and
splice it). Convert/deconvert fire the live-guarded server verbs and surface
the typed refusal loudly. Reads dispatch through `CoreClient`; the detail's
file-manager toggle is a unification wire (ships classic FileTable only
in-lane). `core-contract.ts` gains the mount-file action verbs
(scan/convert/deconvert/file-delete/file-describe/blob-list) + the
`systemBrowseDirectory` request and result types. 13 unit specs.

P4.6z unit 1 (lane A): the `systemBrowseDirectory` server rider — a new
`api::system::browse_directory` dispatch handler porting v4's
`GET /api/v1/system/browse-directory` (the DirectoryPicker's host-filesystem
browser). DB-free; rides `/api/dispatch` (no new REST route). Home-dir default,
`path.resolve`/dirname/join posix normalization, dirent dir-only filtering,
hidden-dir skip, and localeCompare (ICU4X en-US) sort all match v4. Verified by
a new route differential (`browse_directory_equivalence`, 5 cases byte-exact
over the committed `browse-fs-tree/` fixture) plus Rust unit tests for the
home-default and permission-denied arms.
P4.6aa lane B (file-manager, gate): full gate run. ng test 535 green (76
files), ng build clean, cargo test --workspace untouched-green (no Rust
changed). Full Playwright: the probe-guarded file-manager spec correctly
reports SKIPPED in-lane. Added an afterAll cleanup to the e2e spec
(mountPointDelete the seeded store) so it never contaminates the shared
fixture DB. Note for the unifier: under this machine's heavy build/disk
load the full suite showed non-deterministic flakes in the server-
lifecycle specs (foundation / setup-flow / terminal-flow — a different
subset each run); all pass in isolation (3/3 green), all orthogonal to
this lane's changes.

P4.6aa lane B (file-manager, unit 5): the <select [value]> audit rider
(dogfood-#6). Audited every <select [value]> in the enumerated files.
Static/synchronous-option selects (coreWhisper, pronoun preset, transport,
pseudoToolMode, modelClass=MODEL_CLASSES const, the model-selector
component, the reverse-user computed, and the cheap-llm/model-selection
selects already on [selected]) are safe and left as-is. Converted the two
genuinely-risky ones — async query options bound via [value] on the
<select> — to [selected]-per-option: the api-key-modal Provider select
(async providerList) and the new-character Default Connection Profile
select (async connectionProfileList). Regression specs added for both
(api-key-modal.spec.ts new; a [selected] case in new-character.spec.ts).

P4.6aa lane B (file-manager, unit 4): the probe-guarded e2e
(file-manager-flow.spec.ts). A live browser walk of qt-file-manager over
the shared Salon server: seed a database store via the frozen-live
mountPointCreate dispatch, reach its detail screen, flip the "New file
manager (beta)" toggle, then create folder → upload → rename → move into
the folder → delete → the copy-folder refusal beat (assert the steampunk
prompt, no request). The detail screen + toggle belong to the sibling lane
(P4.6z) + the unifier wire, so a runtime probe skips the walk in-lane
(reports SKIPPED) and self-activates once they land (the P4.6x precedent).

P4.6aa lane B (file-manager, unit 3): the qt-file-manager component
(bespoke, D18). Selector qt-file-manager, standalone + lazy-loadable, per
the pinned component contract: mountPointId / capabilities / mountType
inputs, no outputs (self-contained load/reload). A navigable folder tree +
listing over mountFilesList (the document-picker navigation idiom), with
rename (inline), create-folder (inline), delete, upload (multipart when
canWrite), open/download (raw item route), and move/copy within the mount
via a cut/copy -> "paste here" clipboard — every affordance gated by the
server-derived capabilities bag. The two backend gaps (copy-folder,
cross-mount folder move) surface v4's steampunk refusals, never a request.
Loading ("Summoning the file manager…") / empty / error states in v4
voice. Styling is native qt-* (no second theming engine — the bespoke
payoff). 10 component cases green (zoneless TestBed over a fake CoreClient).

P4.6aa lane B (file-manager, unit 2): the wiring core. Ported v4's
createSvarAdapter forward as a framework-free FileManagerAdapter (method
surface instead of SVAR's api.on interception): the serialized mutation
chain (a flurry of gestures can't interleave), capability gating before
any request (a read-only mount refuses and reverts the optimistic
change), reload-to-reconcile after every mutating op, error translation
to the steampunk verdict (dispatch code then kind, read defensively), and
the post-copy reindex for cross-storage byte-copies (onIndexing +
mountReindex/mountEmbed). The concrete transport (CoreFileManagerTransport)
rides CoreClient.dispatch for JSON verbs and the multipart
?action=write-file REST leg for uploads; the file-op verbs aren't in the
SPA CoreRequest union (lane A's core-contract.ts), so they go through
dispatch with one localized cast + a defensive envelope read. loadMountTree
turns mountFilesList into flat tree nodes. 14 unit cases green.

P4.6aa lane B (file-manager, unit 1): the D18 ngx-explorer spike ran
GREEN on all three named gating checks (renders under Angular 21
zoneless; standalone-from-standalone interop — the lib is standalone in
5.0.2, not NgModule as the doc said; a mock IDataService adapter drove a
live listing + createDir mutation), but adoption was REJECTED in favour
of the bespoke fallback: its IDataService has no move/copy verb (a
Tier-1 must-land), and wrapping it reintroduces a second theming engine
(the svar-theme-bridge cost D18 set out to avoid), a numeric-id↔path
map, and a per-directory listing model at odds with our whole-mount
mountFilesList. ngx-explorer was uninstalled (package.json/lock clean).
Ported the v4 SVAR adapter helpers forward as pure spec'd TS under
apps/web/src/app/files/: node-id (relative-path arithmetic, verbatim),
listing-to-tree (over the mountFilesList envelope), event-wire-map (the
gesture→wire translation, re-targeted from v4's REST routes to the P4.6v
dispatch verbs, both backend-gap refusal arms preserved), error-
translation (the steampunk code table verbatim; fallback re-targeted
from HTTP status to the dispatch ErrorKind), reindex-after-copy
(verbatim). 37 unit cases green (derived from the v4 sources, which
carried no unit tests).

P4.6z/P4.6aa round setup: work orders written for the Scriptorium SPA
round (D18) after a fresh v4 survey at a7b1398d (no oracle drift).
Lane A (p4.6z-scriptorium-spa.md): the /scriptorium stores +
/scriptorium/:id detail + FileTable vertical over the frozen
mount-file surface, plus the one missing server variant
(systemBrowseDirectory, the DirectoryPicker's browse route) with a
route differential over a committed fs-tree fixture. Lane B
(p4.6aa-file-manager-component.md): the D18 ngx-explorer spike
(bespoke fallback) + the v4 SVAR adapter-helper ports + the
qt-file-manager component, integrated via a toggle wire pasted at
unification; carries the dogfood-#6 select-audit rider. Shared
contract + ownership sections verified byte-identical across both
orders. Docs only — no version bumps.

P4.6y unification: the mount-file-ops remainder round is UNIFIED on
main. The single lane branched from main HEAD, so the round was a
clean fast-forward (no cherry-pick conflicts). P4.6y CLOSED, P4.6v
CLOSED, D7 CLOSED, EngineAssembly.mount_refresh WIRED LIVE. The full
gate ran fresh at unification: fmt --check; clippy -D warnings on both
feature sets; release build (Cargo.lock in sync); cargo test
--workspace green (296 suite runs, 0 failed); all eight oracles
regenerated FRESH from v4 a7b1398d and their differentials re-run
green by name (chunker 69, md-convert 28, read 18, index 14, ops 39,
write 22, refresh 1, documents-routes 24); ng test 470 (66 files); ng
build clean; the FULL Playwright suite 29/29 — the two Document Mode
beats now exercise the live refresh path (a document-store write
chunks + refreshes stats in the background). Standing deferrals (loud,
named): the production pdf/docx DocumentTextExtractor + WebP codec,
conversion.ts behind the refusal-armed convert/deconvert verbs, the
chokidar-equivalent fs watcher + db-store-event emitter chain, the
quilltap docs CLI subcommands. Next: the Scriptorium SPA (D18).
Versions: core 0.0.205, web 0.0.17, harness 0.0.187, host 0.0.15,
SPA 0.5.50.

P4.6y unit K: D7 CLOSED — the api/mount_points.rs refusal note deleted
(the action verbs + semantic-search all live in api::mount_files now);
P4.6v marked CLOSED (completed by P4.6y) and the closure noted in the
P4.6p header; the P4.6y order carries the full close-out record
(contract pins + standing deferrals). Lane gate green: fmt --check;
clippy -D warnings on both feature sets; cargo test --workspace (304
suites, 0 failed); all eight differentials regenerated FRESH from v4
a7b1398d and re-run green by name (chunker 69, md-convert 28, read 18,
index 14, ops 39, write 22, refresh 1, documents-routes 24); release
build clean. Unifier flag: run the FULL Playwright suite (the seam wire
changes live document-write behavior).

P4.6y unit J: the EngineAssembly.mount_refresh seam is WIRED LIVE (the
P4.6w deferral closed). refresh.rs gains run_refresh (mount lookup + fs
abs-path resolution + the whole-mount form) and DbMountRefreshScheduler —
the production MountRefreshScheduler: fire-and-forget with the writer
channel as the scheduler (a spawned OS thread enqueues its own write job,
so it runs after the triggering write commits and never re-enters the
busy writer). host.rs replaces the P4.6w mount_refresh: None block with
the live wiring; None assemblies keep the loud skip. New refresh-parity
differential (mount-refresh oracle over the documents fixture, refresh
chain UNMOCKED + drained vs v5 driving the PRODUCTION scheduler and
polling the chunk) — green; the P4.6w documents-routes differential
regenerated fresh and re-run green after the wire. core 0.0.205, host
0.0.15, harness 0.0.187.

P4.6y unit H: the quilltap-web edge legs. mount_file_get gains the
filesystem-mount branch of the raw byte read (boundary-escape guarded,
X-File-Sha256); new v4-shaped routes: PUT
/api/v1/mount-points/{id}/files/{path} (JSON + multipart ingest onto
mountFileWrite), POST /api/v1/mount-points/{id}?action=write-file
(multipart, onto mountFileWriteRaw; other actions answer a loud pointer
to /api/dispatch), and POST /api/v1/mount-points/{id}/blobs (multipart
upload onto mountBlobUpload, 201). doc_mount_blobs gains v4's lazy
table-init (hand-written DDL verbatim) so runtime-minted stores accept
their first blob — called from link_blob_content. New live-server
integration test drives all four legs plus the escape refusal and a
blob byte round-trip. core 0.0.204, web 0.0.17.

P4.6y unit I: mountConvert/mountDeconvert land refusal-armed — the
variants exist, v4's capability guards run live (already-database /
not-database / mid-conversion quiesce / empty targetPath), and the
conversion machinery itself answers a loud typed refusal naming P4.6y
(conversion.ts is a full future unit). Pinned by a dedicated harness
test over the mounts fixture. core 0.0.203, harness 0.0.186.

P4.6y units B+G: the storeMountFile ingest pipeline + the blob routes.
store_file.rs ports all three ingest branches (fs disk write with the
optimistic-mtime CONFLICT; database native-text into doc_mount_documents
with the expectedMtime guard, the inline chunk pass, and the post-write
refresh chain; database blob with WebP transcode + pdf/docx extraction
through seams). blob_transcode.rs adds the WebpTranscoder seam (the
refusing default takes v4's store-original fallback arm loudly — the
production codec is a named deferral, and a real encoder could never be
byte-identical to sharp anyway); refresh.rs is the shared refresh chain
(v4 scheduleDocumentStoreRefresh == storeMountFile's fire-and-forget),
run synchronously under the single-writer model. New variants:
mountFileWrite (the ingest PUT; multipart legs ride base64 +
originalMimeType/FileName), mountFileWriteRaw (the byte-preserving
?action=write-file, behavior-keyed apart), mountBlobUpload (201 at the
edge), mountBlobsList/Delete/Update (the documents-table fallback and
the full 21-column joined blob view preserved). New mount-write
differential: 22 cases green — including the drained fire-and-forget
parity (chunks + PENDING jobs + refreshed stats after a native-text
write) and both mtime-conflict arms. core 0.0.202, harness 0.0.185.

P4.6y units C+D: the whole mount-file mutation surface. file_ops.rs
completes v4's file-ops.ts (copyFile/moveFile/linkFile/writeFile/
deleteFile — the four strategies db-link/fs-link/rename/byte-copy, the
sha256 end-to-end verify pair, hardLinkDbToDb, updateLinkLocation,
writeDestBytes, deleteAtSource/Dest, writeFsFileBytes) and folder_ops.rs
ports folder-ops.ts (deleteFolder refusing non-empty, moveFolder
same-mount with the fs link-prefix rewrite). Eight new dispatch variants:
mountFileMove/Copy/Link/Delete, mountFolderDelete/Move, mountFileUpdate
(item-route PATCH — rename-first, blob-only descriptions), and
mountFolderCreate (isPathSafe-guarded). v4's per-handler catch split
reproduced (file verbs code only FileOpError; folder verbs also code
DatabaseStoreError). New mount-ops differential: 39 cases over the jest
real-DB oracle — every strategy arm, the error {error, code} envelopes
byte-exact (including v4's copy_same_path → DEST_EXISTS ordering quirk),
and the eight-table dumps under the shared normalization (extracted to
tests/mount_common/). core 0.0.201, harness 0.0.184.

P4.6y unit E: reindex + scoped embedding enqueue + semantic search.
services/mount_index/reindex.rs ports reindexLinks (synchronous
in-request, deliberately — the empty-extraction and catch bookkeeping
arms exact) and enqueueEmbeddingJobsScoped (the {jobs, queued, skipped}
summary with the config-missing messages verbatim). New dispatch
variants mountReindex / mountEmbed / mountSemanticSearch (the search
parses v4's semanticSearchSchema in-handler, embeds through the
engine's memory_embedding provider, and searches with
includeBlocked:true). search_document_chunks gains the projectId
scope resolution and now reproduces v4's JS falsy-|| defaults
(limit:0 → 10, minScore:0 → 0.3 — threshold 0 really searches at 0.3,
ported broken-but-exact). CoreError gains an optional `code` field
(absent unless set) carrying the binding {error, code} file-op union —
EMBEDDING_FAILED / EMBEDDING_DIMENSION_MISMATCH land on the search
arms. The mount-index differential now covers all 14 cases (scan +
reindex + embed + search, incl. real builtin TF-IDF scores rounded at
1e-6) — green; the oracle un-mocks jest.setup's canned embedding
service and pins the processor-off recipe. core 0.0.200, harness
0.0.183.

P4.6y unit F: the scanner + converters + embedding-scheduler ports and
the mountScan variant. services/mount_index gains converters.rs (the
markdown/txt converters with JS-regex-faithful syntax stripping — the two
backreference patterns hand-rolled — plus the DocumentTextExtractor seam,
whose refusing default routes pdf/docx through v4's empty-text bookkeeping
arms, loudly), reindex_file.rs (the doc-edit reindexSingleFile port),
scanner.rs (walk/processMountFile/removeMountFile/updateMountPointTotals/
scanMountPoint/rescanDatabaseMountPoint/verifyBasePath/
createFilesystemFolder), and embedding_scheduler.rs (the MOUNT_CHUNK
EMBEDDING_GENERATE enqueue with the embed:false erase policy). Repo
extensions (additive): linkFilesystemFile + updatePolicyFlags + the widened
LinkUpdate patch on doc_mount_file_links; chunk row reads +
clearEmbeddingsByLinkId; updateScanStatus/updateLastScanned/refreshStats on
doc_mount_points; blob updateDescription/updateExtractedText;
MountServiceInfo widened. New differentials, both green: mount-md-convert
(28-case tier-1 exact over v4's real convertMarkdownToText) and mount-index
(the jest real-DB route oracle — 4 scan cases with the full eight-table
dump diffed under one shared normalization; reindex/embed/search rows
already emitted for the next unit). The committed fs tree gains a
syntax-heavy styled.md (embed:false frontmatter), blank.md, and an excluded
scratch.tmp; mounts-main.db gains the background_jobs table; mount-read
regenerated + green. core 0.0.199, harness 0.0.182.

P4.6y unit A: extend the committed mounts fixture family for the
mutation/indexing differentials — the MAIN db gains the BUILTIN TF-IDF
embedding profile (default) + a fitted tfidf_vocabularies row over the
chunk corpus; MP_DB gains a garbage-PDF blob (docs/report.pdf, the
extraction-state substrate: conversionStatus 'pending' /
extractionStatus 'none') and three pinned chunks (two with REAL builtin
TF-IDF embeddings via v4's generateEmbeddingForUser, one NULL-embedding
enqueue target). mount-read oracle regenerated (18 cases) and
mount_read_equivalence re-run green. quilltap-web 0.0.16 (the fixture
bytes live in its tests/fixtures).

Write the P4.6y work order (docs only): the single-lane resumption order
for the P4.6v remainder — the mount-index mutation + indexing surface
(store-file/file-ops/folder-ops, reindex/embed/scan, semantic search,
blobs, the multipart + raw-read fs web-edge legs, convert/deconvert
refusal arms), wiring the EngineAssembly.mount_refresh seam live, and
closing D7. The P4.6v order stays the survey of record with a pointer to
P4.6y; v4 baseline re-verified unmoved at a7b1398d.

The P4.6v ∥ P4.6w ∥ P4.6x round unified: Document Mode end-to-end (the
operator-doc-actions server surface + the SPA pane/picker vertical) and
the first slice of the Scriptorium server (the mount-index pure leaves
+ the READ/LIST keystone). P4.6w and P4.6x are CLOSED; P4.6v stays OPEN
with units 4-9 (write/ops/scan/blobs/convert + reindex/embed — D7 not
yet closed, and the EngineAssembly.mount_refresh seam stays unwired
until those services land). D17's Document-Mode Lexical spike came back
RED: markdown documents ship in the byte-exact textarea; ProseMirror is
the named next editor decision. Full gate green: fmt/clippy (both
feature sets)/dev+release builds clean; all four round oracles
regenerated fresh from v4 a7b1398d (69+18+16+24 cases) and their
differentials green by name; cargo test --workspace 298 suites, 0
failed; ng test 470 (66 files); ng build clean; the full Playwright
suite 29/29 with the two document beats newly ACTIVATED (blank →
edit → flush-save → reload-persist → rename → Librarian chip → close,
plus document+terminal stacked panes). Versions: core 0.0.198, harness
0.0.181, host 0.0.14, web 0.0.15, SPA 0.5.50.

Fix the P4.6x document beats at first live run (four gate fallouts,
all port-divergence/gesture class): (1) the pane's flush-on-blur save never
fired in a real browser — the editor container listened for `blur`,
which does not bubble; v4's React `onBlur` works because React
delegates blur as `focusout`. The container now listens for `focusout`
(a unit spec pins the wiring). (2) The new e2e spec file was named
`document-flow.spec.ts`, which sorts before `foundation.spec.ts` and
unlocked the shared server before foundation walked the locked gate —
renamed to `salon-documents-flow.spec.ts` (every shared-server spec
must sort after foundation; the constraint is now documented in the
spec header). (3) The reload-persistence check asserted the shell entry
after `page.reload()`, but the reload lands back on the chat page — the
gesture now waits for the chat body. (4) Shared-chat residue: the
both-panes beat now unwinds the panes it opens (server-persisted state)
and terminal-flow's announcement-chip locators are newest-match — the
documents beat's own Ariel chips stay in the shared chat's history.
SPA 0.5.50.

P4.6vwx unification wire: the `EngineAssembly.mount_refresh` seam stays
UNWIRED (None + loud skip) — the order planned to wire it to lane A's
reindex/embed services at unification, but P4.6v's units 4-9 (which
deliver those services) remain open; the host.rs comment now records
the real condition. Host 0.0.14.

P4.6v (lane A) unit 3: the mount-index READ + LIST surface. Ports v4's
`readMountFile` / `readMountFileBytes` (`read-file.ts`, all storage
shapes — fs bytes, database documents, database blobs, with line-window
pagination), the files-list route body (the full
`DocMountFileLinkWithContent` set + the `doc_mount_folders` ∪ on-disk
folder merge via `listFilesystemFolders`/`matchesPattern`), and
`resolveFsAbsolute` (the boundary-escape guard, unit-pinned). New:
`services/mount_index/{read_file,list,file_ops}.rs`,
`api/mount_files.rs`, the `mountFilesList` + `mountFileRead` dispatch
variants (reachable via `/api/dispatch`; lane C's DocumentPicker
consumes `mountFilesList`), a `find_service_info_by_id` /
`find_links_with_content_json_by_mount_point_id` repo pair, and the
committed `mounts-{main,mount}.db` fixture + `mounts-fs-tree/`.
Differential-proven exact against v4's real code (18-case tsx oracle
`mount-read.ts` over per-side fixture + fs-tree copies).

P4.6v (lane A, the mount-index file-ops server) unit 1: the tier-1 pure
leaves of v4's `lib/mount-index/` — the chunker (`chunkDocument` /
`estimateTokens`), the path utilities (`normaliseRelativePath` /
`detectNativeText` / `mimeForExtension`, ported with a faithful
`path.posix.normalize` / `path.posix.extname`), the `FileOpError` type,
and the `fileOpStatus` HTTP-status mapper. Landed under
`quilltap-core::services::mount_index`; the web edge now shares the one
`mime_for_extension` port (the duplicate in `files_routes.rs` removed).
Differential-proven exact against v4's real code (a 69-row tsx oracle,
`harness/oracle/cases/mount-chunker.ts`).
P4.6w (Document Mode server, lane B): the qtap-target byte route
(`GET /api/v1/chats/{id}/qtap-target`) — resolves a `{filePath, scope,
mountPoint}` through the operator override (the same chat access rules as
Document Mode) and streams the raw bytes (text docs + blobs), for the global
qtap image viewer on non-Salon surfaces. A dedicated byte route (D4 —
binary is a real URL, never enum dispatch), reusing the differential-proven
`documents::resolve_operator_doc_path` + the `mount_file_get` byte read. The
18 dispatch variants need no router change — they flow through the generic
`/api/dispatch`.

P4.6w (Document Mode server, lane B): the `api::documents` dispatch — the
11 chat-scoped + 7 standalone Document Mode variants (active/open/recent
document lists, accessible-stores in both modes, open/close/read/resolve/
write/rename/delete, plus the standalone stores/recent/open/read/write/
rename/delete). Wired into `api::types` (the `Request` variants + a
`Response::Document`) and `api::engine` (dispatch + the
`MountRefreshScheduler` seam on `EngineAssembly`, defaulting to `None`).
`refreshDocumentMode` recompute, the recents dedupe (current-chat wins), the
Librarian open/save/rename/delete announcements, the effectiveScope fallback,
and the move-sync sweeps are all differential-proven — a 24-case route
differential over a new committed `documents-{main,mount}.db` fixture drives
v4's REAL chat-scoped + standalone handlers and diffs response bodies +
Librarian message text + the chat_documents/documentMode state byte-for-byte.
Fixture-build gotcha closed: materialize `chat_messages` (via `getMessageCount`)
so the Librarian post has a table to append to.

P4.6w (Document Mode server, lane B): the `documents` core module — the
chat-agnostic operator-doc-actions port (`STANDALONE_CHAT_ID`,
`MAX_RECENT_DOCUMENTS`, `resolveOperatorDocPath`, `resolvedPathExists`,
`classifyResolvedTarget`, `pickUntitledDocumentPath`, `openDocumentFile`,
`writeDocumentFile`, `computeRenameTarget`, `renameDocumentFile`,
`deleteDocumentFile`, `listAllEnabledStores`) plus the `MountRefreshScheduler`
seam (defaults to a loud unwired skip; the machinery is wired at unification).
Filesystem/`general` scopes surface the host-fs seam (database-backed corpus
only, matching the doc-edit surface). The mtime-conflict guard is reproduced in
`writeDocumentFile` so the 409 arm stays faithful without touching
`database_store`. `computeRenameTarget` is proven byte-exact against v4 (pure
tier-1 oracle); the stateful functions are proven with the route surface.

P4.6w (Document Mode server, lane B): extend the `chat_documents` repo
with the six Document Mode queries/sweeps — `find_active_for_chat`
(earliest-opened active), `find_recent_for_chat`, `find_recent_across_chats`
(newest-first), `rename_file_path_in_store` / `rename_folder_path_in_store`
(the best-effort move-sync sweeps, scope + normalized-null mount matched),
and `delete_by_chat_id` (cascade) — plus a full-row `ChatDocumentFull`
projection. Rust unit tests cover each; the v4-oracle differential lands
with the route surface.

Work orders for the P4.6v ∥ P4.6w ∥ P4.6x round (the Document Mode +
Scriptorium-server round), docs only. Lane A
(p4.6v-mount-index-file-ops-server.md): the mount-index file-ops
server surface — v4's lib/mount-index service layer (chunker,
file-ops strategies, store-file, read-file, reindex/embed, scanner)
under ~20 mount-file dispatch variants plus the multipart/raw web
legs, closing the standing D7 Scriptorium refusal; convert/deconvert
refusal-armed, pdf/docx extraction and the fs watcher behind named
seams; new committed mounts fixture + fs tree. Lane B
(p4.6w-document-mode-server.md): the Document Mode server surface —
the operator-doc-actions core with STANDALONE_CHAT_ID, 11 chat-scoped
+ 7 standalone document variants, chat_documents repo extensions, the
qtap-target byte route, and the MountRefreshScheduler seam wired at
unification; new committed documents fixture. Lane C
(p4.6x-document-mode-spa.md): the Document Mode SPA vertical — the
pane in the P4.6u split scaffolding, the useDocumentMode state store,
the Document Picker (consuming lane A's mountFilesList), autosave +
409 reload, tool-result reloads, and the D17 Lexical spike with a
loud textarea fallback; standalone/workspace-tab surface deferred.
Shared contract + ownership blocks verified byte-identical across the
three orders; the Scriptorium SPA (D18 ngx-explorer spike) is
deliberately next round over lane A's then-frozen surface.

The P4.6s ∥ P4.6t ∥ P4.6u round unified: the Commonplace Book (memories
server + Memory SPA) and the Salon terminal pane. Unification wires:
EngineAssembly/SpineBundle gained memory_embedding and the production
host now threads the spine's ApiEmbeddingProvider into ReadyEngine, so
memoryCreate/memorySearch run LIVE in the real server (lane A's named
seam closed); the A-to-B contract diffed clean name-for-name (all 29
memory variants); the B+C core-contract single-author blocks merged
without drift (one stray concat marker dropped); SPA version
accumulated to 0.5.43. Full gate green: fmt/clippy (both feature
sets)/release build clean; the two memories oracles regenerated fresh
from v4 a7b1398d (routes 24 + config 17 = 41 cases) and the
memories_routes_equivalence differential green by name; cargo test
--workspace 294 suites / 1,251 tests / 0 failed; ng test 411 (60
files); ng build clean; Playwright 27/27 including the newly-activated
P4.6t memory beats (the create/edit/delete walk exercises the live
embedding wire end-to-end) and the P4.6u terminal walk. Gate fallout
(gesture/materialization class only): the Memories tab click needed
nav-scoped locators (a conversation card's count glyph shares the
name), and the memory beat's fixture userId rewrite gained the
embedding/settings tables so the baked default BUILTIN profile follows
the session user. Versions: core 0.0.193, harness 0.0.177, host
0.0.12, web 0.0.13, SPA 0.5.43.

P4.6u (lane C) — the LIVE terminal-flow e2e + the fixes it surfaced. The
`e2e/terminal-flow.spec.ts` walk (unlock → open a chat → open the pane →
spawn a real PTY → `echo quilltap` renders → expand the "terminal opened"
chip to see the in-pane embed note → kill → "terminal closed" chip)
runs green over the real WebSocket. Fixes it forced: (1) `qt-terminal`
now STATICALLY imports `@xterm/xterm` (a runtime `import()` of xterm
5.x's UMD build breaks esbuild interop — the named export isn't a
constructor); the module stays lazy via the Salon route chunk. (2) The
inline embed moved from `message-row` to the announcement-group's
EXPANDED chip — v5 collapses every Staff announcement (incl. Ariel
`session-opened`) into a chip, and v4 shows the embed only on an expanded
announcement. (3) `global-setup` materializes the `terminal_sessions`
table (the frozen Salon fixture predates terminal support) and the PTY's
`files`/`logs` dirs before launch — fixture-schema materialization, not a
regen. (4) `SplitLayout` gained a stretch host class — the Angular host
element between `.qt-chat-main` and `.qt-doc-split-layout` broke the
`h-full` cascade and regressed the message-list virtualization (dogfood
#3a redux).

P4.6u (lane C) — the inline terminal embed + the pop-out route. Added
`terminal-embed.ts` (v4 `TerminalEmbed`: a collapsible inline surface,
collapse persisted to localStorage per session, pop-out / kill controls,
a "showing in the pane" note when the session is the active pane one, and
the `quilltap:terminal-exited` dispatch on PTY exit). Message rows render
it for Ariel session-opened announcements matched by the
`<!-- terminalSessionId:UUID -->` marker. Added the full-page pop-out
route `/salon/:id/terminal/:sessionId` (`terminal-popout.ts`, v4's
pop-out page).

P4.6u (lane C) — the terminal pane wired into the Salon. Added
`terminal-pane.ts` (v4 `TerminalPane`: header focus-toggle / hide / kill
with a two-click confirm + the xterm body) and `terminal-session-picker.ts`
(v4 `TerminalSessionPicker`, over `qt-modal`). The Salon conversation now
provides `TerminalModeController`, wraps its chat + terminal pane in
`qt-split-layout`, hydrates the pane state on chat load, refetches on the
`quilltap:chat-update` / `quilltap:terminal-exited` DOM events, and binds
Cmd/Ctrl+Shift+T (toggle) + Escape (exit focus). The composer gained an
"Open terminal" button (hidden while the pane is up). The lane-C
core-contract block documents the terminal protocol home + merges the
pane-state read fields onto `ChatDetail`.

P4.6u (lane C) — the terminal mode controller + split layout. Added
`terminal-api.ts` (the REST wrapper over the frozen `/api/v1/terminals*`
routes + pane-state persistence via `chatUpdate`), `terminal-mode.ts`
(the injectable per-conversation `TerminalModeController` — v4
`useTerminalMode`: smart open/attach/spawn/hide/kill/focus, hydrate from
the chat with a dead-session fallback), and the generic split
scaffolding `split-layout.ts` + `right-pane-vertical-split.ts` (v4
`SplitLayout` / `RightPaneVerticalSplit`, `TemplateRef`-slotted,
draggable dividers with keyboard support; the vertical split is ported
for Document Mode's future top pane though only the terminal mounts it
now). Pure-logic specs cover the [20, 80] clamps, the focus toggle,
`isLiveSession`, and the controller's open/hydrate/kill decision tree.

P4.6u (lane C) — the Salon terminal foundation. Added `@xterm/xterm`
5.5.0 + `@xterm/addon-fit` 0.10.0 (the only deps this round) and
`apps/web/src/app/terminal/`: the WebSocket protocol types pinned from
the frozen Rust source (`quilltap_host::terminal::protocol`), the
`<!-- terminalSessionId:UUID -->` marker extraction, the ref-counted
`TerminalSessionService` (one WS per session id: ping/pong keepalive,
client-side replay buffer, resize, reconnect on 1006/1011), and the
`qt-terminal` xterm surface (lazy-imported). Unit specs cover the
marker regex, the output fan-out/replay, and the server-frame → state
mapping.
P4.6t lane B, unit 4 — the fixture-guarded e2e beats. A
`characters-flow.spec.ts` describe (P4.6t) boots its own locked server
over lane A's NEW `memories-main.db` and walks: open a character's
Memories tab → the count-bearing header renders over the fixture →
create a memory → edit it → delete it (memoryCreate/Update/Delete). A
`settings-flow.spec.ts` describe asserts the four Commonplace Book cards
render on the Memory tab (and the deferred dedup card does NOT) and that
a Recall Relevance toggle round-trips through the server across a reload
(memoryRecallConfigSet/Get). Both describes SKIP while `memories-main.db`
is absent (this worktree) and auto-activate at unification;
`playwright --list` discovers all three (26 total). SPA → 0.5.38.

P4.6t lane B, unit 3 — the Settings → Memory tab (the Commonplace Book
cards). The Memory tab now renders v4's `MemorySearchTabContent`
CollapsibleCards (titles/descriptions + `?section=` deep-link ids ported
verbatim): Repair Missing Embeddings (`memoryBackfillProgress` polled
every 4s + `memoryBackfillStart`), Memory Housekeeping (config +
character counts, the enable toggle / per-character cap / collapsible
per-character overrides / merge-similar, all merge-patch via
`memoryHousekeepingConfigSet`, + a `memoryHousekeepSweep` run-now),
Recall Relevance (`memoryRecallConfigGet/Set`: the down-weight|exclude
scope policy + expand-related toggle), and Regenerate Memories (the
destructive wipe-and-rebuild behind an inline confirm, with a
`memoryRegenerateAllStatus` line that polls every 5s only while a sweep
is in flight). The `<select>` binds `[selected]` per option (dogfood #6).
Deferred loudly — rendered as NOTHING, no dead cards: the Embedding
Profiles sub-tab, the Memory Deduplication card (server unported), and
the Regenerate Conversation Summaries card. Unit specs (mocked
CoreClient) per card. SPA → 0.5.37.

P4.6t lane B, unit 2 — the per-character memory Cleanup (housekeeping)
dialog. The Memories tab's Cleanup button (shown when the list is
non-empty) opens v4's `housekeeping-dialog.tsx` over `qt-modal`: the
options (max unprotected memories, max age in months, min-importance
deletion threshold slider, merge-similar), a 300ms-debounced preview
(`memoryHousekeepPreview`) rendering the Keep / Delete / Merge stat tiles
and a changes list (kept rows omitted), and a Run
(`memoryHousekeep`, `dryRun:false`) that deletes and refetches. Run is
disabled while loading or when the preview shows nothing to do. Preview
errors surface in an alert. Unit spec (mocked CoreClient) covers the
debounced preview, the changed-details filter, the disabled-when-idle
guard, the dryRun:false run + complete emit, and the error path.
SPA → 0.5.36.

P4.6t lane B, unit 1 — the character Memories (Commonplace Book)
vertical. The `memories-tab.ts` placeholder is closed: the per-character
Memories tab now renders v4's `MemoryList` — a debounced search box,
sort-by / sort-order / source filters, an infinite-scroll grid of memory
cards (`MEMORIES_PER_PAGE = 30`, id-deduped across pages to survive
offset instability during a regenerate sweep), the create/edit editor
(plain textarea for content — the Lexical editor is a D17 deferral;
always sends `source:'MANUAL'`, never edits tags), and inline
delete-with-confirm. The memory card renders v4's Low/Medium/High
importance buckets, AUTO/MANUAL badge, keyword chips, read-only tag
badges, expandable content (>150 chars), and an AUTO Source link (deep
message anchoring deferred — navigates to the chat). Lane B authors the
`core-contract.ts` memory block (all P4.6t Shared-contract Request
variants + read DTOs, folded into `CoreRequest`) and the shared
`memory/` data layer + pure-logic helpers. Unit specs for the pure logic
(bucket thresholds, keywords join/split, page-dedupe transcribed from
v4), card, editor, and list (mocked CoreClient). SPA → 0.5.35.
P4.6s memories server, part 5 (embedding status + backfill; tier-2 close):
`memoryEmbeddingStatus` (coverage % + configured profile name via a new
`find_default_id_name` finder) and `memoryBackfillStart` (batch-enqueue
EMBEDDING_GENERATE for memories missing an embedding). The three heavy arms
stay LOUD refusals (the `not_available` idiom) — `memoryGenerateEmbeddings` /
`memoryRebuildIndex` (the `generateMissingEmbeddings` / `rebuildVectorIndex`
services are unported) and `chatQueueMemories` (`resolveCheapLLMProfileId` + the
batch extraction enqueue are unported). 42 differential cases.

P4.6s memories server, part 4 (regenerate + backfill status): `memoryBackfillProgress`
(count-without-embedding + in-flight EMBEDDING_GENERATE MEMORY jobs),
`memoryRegenerateAllStatus` (fan-out/wipe/extraction job counts), and
`memoryRegenerateAll` (wipe in-flight jobs, resolve the standard + dangerous-
compatible cheap profiles, enqueue one deduped fan-out). New additive enqueuers
`enqueue_memory_regenerate_all` (userId-deduped) + `enqueue_embedding_generate`.
Tier 1 of the memories surface is complete; 39 differential cases.

P4.6s memories server, part 3 (housekeeping + configs): `memoryHousekeepPreview`
(GET envelope), `memoryHousekeep` (POST dryRun/run — details only on dryRun),
`memoryHousekeepSweep` (job enqueue), `memoryHousekeepingConfigGet/Set` (per-user
chat_settings, default injection + merge-patch), and the three instance-wide
config pairs — `memoryRecallConfigGet/Set`, `memoryExtractionLimitsGet/Set`,
`memoryExtractionConcurrencyGet/Set` (new additive `instance_settings` getters/
setters). The oracle splits into `memories-routes` + `memories-config` to stay
under the jest cross-case-contamination threshold; the fixture gains the
`instance_settings` table. `memories_routes_equivalence` covers 36 cases total.

P4.6s memories server, part 2 (writes + search): `memoryCreate` (the
gate-driven create — INSERT, the near-duplicate absorb, 201 `{memory}`,
SKIP_EMBEDDING_FAILED → server error), `memoryUpdate` (no re-embed),
`memoryDelete` (the relatedMemoryIds unlink scrub), `memoryDeleteByChat`,
and `memorySearch` — the builtin TF-IDF semantic search running LIVE. The
embedding arms take an injected `EmbeddingProvider` (the engine holds an
`ErasedEmbeddingProvider` seam, refused until host-wired); the differential
drives them with the fixture's builtin profile. Seven more cases in
`memories_routes_equivalence` (24 total), incl. the create/delete structural
dumps and the rounded-score search match.

P4.6s memories server, part 1 (reads + the fixture): the new committed
`memories-{main,mount}.db` fixture (3 characters, 51 memories on one,
builtin TF-IDF embeddings via v4's real path, a swipe group, tagged +
related pairs) and the first five dispatch arms — `memoryList` (both the
paginated and the legacy in-memory paths, tagDetails, search/minImportance/
source filters), `memoryGet` (tagDetails + access-time bump), `memoryCountByChat`,
`memoryByMessage` (swipe-group expansion + the trimmed shape), and
`memoryCharacterCounts` (count-desc). Proven by `memories_routes_equivalence`
against v4's real route handlers (17 read cases, byte-for-byte incl. the
embedding index-keyed object).

Work orders for the P4.6s ∥ P4.6t ∥ P4.6u round (the Commonplace Book
+ terminal-pane round), docs only. Lane A (p4.6s-memories-server.md):
the memories dispatch surface — the collection endpoint's ~20 action
verbs, the item CRUD, and chat queue-memories over the fully-ported
memory engine, with a new committed memories fixture and a
memories_routes_equivalence differential; extract-memories-dry-run,
memory-dedup, embedding-profiles management, and
conversation-summaries stay deferred with no variants. Lane B
(p4.6t-memory-spa.md): the Memory SPA vertical — the per-character
Memories tab (list/card/editor/housekeeping dialog) and the Settings
Memory tab (backfill/housekeeping/recall/regenerate cards), owning the
core-contract memory block. Lane C (p4.6u-salon-terminal-pane.md): the
Salon terminal pane — xterm.js surface, WebSocket session client over
the existing quilltap-web terminal routes, the split-pane scaffolding
Document Mode will reuse, message-embed markers, and a live
terminal-flow e2e walk. Shared contract pinned once per block with a
single named author per the P4.6pqr lesson. Drift check at planning
time: v4 HEAD still a7b1398d.

The P4.6p ∥ P4.6q ∥ P4.6r round gate + close-out. Full gate green:
fmt/clippy (both feature sets)/release build clean; the seven round
oracles regenerated fresh from v4 `a7b1398d` (annotations 25,
roleplay-templates 21, image-profiles 18, mount-points 13, groups 14,
projects 39, scenarios 41) and every differential re-run green by
name; cargo test --workspace 293 suites / 1,250 tests / 0 failed;
ng test 328; ng build clean; Playwright 23/23 including the
newly-live new-chat walk and the P4.6r settings/picker beats. Gate
fixes (gesture/assertion class only, no product bugs): the Templates
beat asserted visibility on the zero-box qt-template-form-modal HOST
(→ ARIA dialog role) and hit strict-mode against the Default Template
selector card (→ div.qt-card element-type scoping, and the
delete-confirm filter could not require the Delete button it had just
replaced); the projects picker beat's post-reload unlock helper
expected the Projects LIST heading on a DETAIL page (→ ready-signal
override, the settings-flow idiom). Orders P4.6p / P4.6q / P4.6r
CLOSED — closing the three P4.6l listing-surface picker gaps with
them. Versions: core 0.0.188, harness 0.0.172, web 0.0.13, SPA 0.5.34.

Unification wires for the P4.6p ∥ P4.6q ∥ P4.6r round. The B↔C
core-contract listing-surface appendix diverged (lane B folded the
variants into the CoreRequest union with …Bag bags; lane C shipped a
localized `as unknown as CoreRequest` cast seam over …Input names) —
reconciled to lane B's union fold: lane C's templates/image-profiles
api layers renamed to the …Bag types and the cast seams dropped (the
requests now typecheck through the union). Contract diffed
name-for-name against the Rust variants: all 16 live variants match;
the three refusal-armed image-profile action interfaces reconciled to
the Rust shapes (opaque payload / provider+apiKeyId — the appendix had
guessed prompt/profileId); `sortByUserCharacter` annotated as
read-by-neither-server (v4 parity). RoleplayTemplateDto
description/dialogueDetection widened to `| null` (the create/update
echoes carry null per lane A's oracle; route reads omit) and the
image-profile create bag accepts explicit null apiKeyId/baseUrl (v4's
`|| null` coercion). FALLBACK_PROVIDERS literals gained the required
`legacyNames`. Verified `defaultRoleplayTemplateId` is plumbed through
chat_settings create/update (lane C's flag). SPA 0.5.33.

P4.6r lane C, part 3 — the reset-builtins rider + e2e beats. "Reset
Built-in Characters" on the roster is now live: a confirm dialog (v4
copy) over the WEB-EDGE `POST /api/v1/characters?action=reset-builtins`
route (live since P4.4u4), dispatched via `fetch`, with a result banner.
Two fixture-guarded Playwright beats authored for activation at
unification: the project Model-Behavior template picker seeds options +
persists a selection (projects-flow), and the Templates create→edit→
delete + Images card listing (a new settings-flow describe over lane A's
extended fixture). SPA 0.5.25. Lane C complete on its branch (awaits
unification, which wires the `CoreRequest` union + drops the localized
dispatch cast).

P4.6r lane C, part 2 — the three disabled default-* pickers go live. The
project Model-Behavior roleplay-template picker, the project
Image-Generation image-profile picker, and the character Defaults-tab
image-profile picker now fetch the P4.6p listings and bind their existing
`defaultRoleplayTemplateId` / `defaultImageProfileId` fields, joining the
per-field immediate-save flow (catch + surface errors). Each uses
`[selected]`-per-option to survive async option loading (the dogfood-#6
regression), covered by a picker spec that seeds options after first
render. The two now-stale "disabled affordance" project-card specs flip to
assert the enabled state. SPA 0.5.24.

P4.6r lane C, part 1 — the Templates & Prompts and Images settings tabs
(SPA-only, tier-4). The two placeholder tabs now render v4's management
surfaces: the Roleplay Templates manager (built-in read-only grid with
Preview + Copy-as-New; My Templates create/edit/delete-with-confirm; the
full Formatting Delimiters editor — wrap/linePrefix/tagPrefix + flourishes;
narration single/pair; the global Default Template selector over
chatSettings; duplicate-name 409 + built-in-guard 403 surfaced verbatim)
and the Image Profiles card (default/uncensored badges; create/edit via a
provider-select + filtered API-key-select + JSON parameters form;
delete-with-confirm; isDefault). The listing-surface DTOs + Request
interfaces landed as the byte-identical B↔C core-contract appendix block;
the SPA dispatches them through a localized cast until lane A wires the
`CoreRequest` union at unification. Deferred loudly: the image-profile
Validate / list-models (their variants are refusal-armed), the structured
per-provider parameters editor (a JSON textarea stands in), and the
"Draft formatting instructions" template helper. SPA 0.5.23.

P4.6q (New-Chat SPA, lane B) — the Salon-list rider + the e2e beat. Added
the "New Chat" affordance to the Salon-list header (the empty-state link
and the project links now resolve to `/salon/new`). Authored
`e2e/new-chat-flow.spec.ts` (unlock → New Chat → pick a character → the
profile auto-seeds → Create → the Green Room narrates → land on the
created conversation with the streamed greeting), fixture-guarded for the
salon fixture and discovered by `playwright test --list` (activated at the
P4.6p/q/r unification). Normalized the new-chat modules with Prettier.
SPA 0.5.29.

P4.6q (New-Chat SPA, lane B) — the `/salon/new` route + page. Ported v4's
`app/salon/new/page.tsx` as `qt-new-chat-page`: reads `?projectId=` /
`?characterId=` / `?autonomous=1`, composes the picker + form + submit
spine + the Green Room dialog, and navigates to the created chat. Added
the `salon/new` route (before `salon/:id`, which previously swallowed it
as `id="new"`). `?autonomous=1` surfaces a loud not-yet-available notice
and proceeds as an ordinary chat. SPA 0.5.28.

P4.6q (New-Chat SPA, lane B) — the form body + shared children. Ported
v4's `NewChatForm` as `qt-new-chat-form`: the in-place Play-As select
(with duplicate-name disambiguation), the self-fetching image-profile
picker (`imageProfileList`, lane-A live variant, `[selected]`-per-option),
the scenario dropdown (project / general / character sources, prefix
tokens, precedence, read-only preview, layered free-text notes via a
plain textarea), the outfit selector (default / llm_choose / none;
manual + previous_chat loudly disabled/omitted), the compact
"Reality Injection Mode" timestamp card, the avatar-generation toggle,
and the project row (picker / read-only). Autonomous mode is a
disabled-with-title deferral; the group optgroup stays absent (dead UI
in v4's /salon/new). Component spec transcribes v4's scenario-layering +
Play-As-listing assertions. SPA 0.5.27.

P4.6q (New-Chat SPA, lane B) — the Green Room (creation-progress dialog).
Ported v4's `CreationProgressProvider` + `ChatCreationProgressModal` over
the ONE global event stream: `GreenRoomStore` subscribes to
`CoreClient.events$`, filters frames scope-tagged with the submit's
`progressId`, and folds them through a pure reducer (`applyGreenRoomFrame`
— status/log with a 100-cap, the wardrobe-start/result upsert, done →
"The players are ready.", error → "Something went awry."). The dialog is
blocking and non-dismissable while creation runs; only the error state
offers Close. `qt-outfit-slots-preview` renders the decided four-slot
outfit. No bespoke SSE route (D3/D6) — the server buffers/replays.
SPA 0.5.26.

P4.6q (New-Chat SPA, lane B) — the character picker panel. Ported v4's
`CharacterPickerPanel` as `qt-new-chat-picker`: the searchable, v4-sorted
roster (favorites > user-controlled > chat count > name > title) on the
left; the selected cast with the "Speaks First" badge and per-character
connection-profile + system-prompt selects on the right. The profile
select's "Play As (User)" option flips the entry to the human in place;
selecting/removing a character resets the chosen character scenario only.
Component spec + the reused pure logic cover the wiring. SPA 0.5.25.

P4.6q (New-Chat SPA, lane B) — the new-chat state service + pure logic.
Ported v4's `useNewChat` as an Angular signals object (`NewChatState`):
the batched reference-data load (characters / connection profiles /
general + project scenarios / seed character + default partner / the
participant-union group scenarios), the seeding precedence (project
default > general default; the seed character's default partner
auto-joins as the user persona; `projectGet` loads the defaults the
list omits), the single-LLM default propagation, and the submit spine
(open the Green Room before the dispatch; the dispatch resolving is the
authoritative ready signal). The load-bearing decisions live in pure
helpers (`generateTitle`, `applyPlayAs`, `applyProfileChange`,
`scenarioSelectPatch`, `buildCreateRequest`, `sortRoster`,
`seedSelectedCharacter`) with a Vitest spec transcribing v4's Play-As +
scenario-layering + payload assertions. The group participant-union is
fetched faithfully but never rendered (dead UI in v4's `/salon/new`).
SPA 0.5.24.

P4.6q (New-Chat SPA, lane B) — core-contract.ts re-pins. Replaced the
provisional `ChatCreateRequest` with the real flattened v4 `POST
/api/v1/chats` body (participants, scenario-source precedence fields,
timestampConfig, outfitSelections, progressId, the carried autonomous
fields) and re-pinned `ChatCreateDto` to the live `{ chat: { id, … } }`
echo. Replaced the `CreationProgressFrame` sketch with the real shape
transcribed from `services/creation_progress.rs` (the `kind`-tagged
Green-Room frame folded flat into `ScopedEvent`, plus `OutfitPreviewSlots`
/ `OutfitPreviewEntry`). Appended the BINDING listing-surface block
(roleplay-template / image-profile / mount-point request variants + DTOs,
byte-identical with lane C) and folded it into `CoreRequest`. SPA 0.5.23.

P4.6p unit 4 (lane A): the global mount-points dispatch surface — the
five variants (list / get / create / patch / delete-cascade) as
`api::mount_points`, composed over the ported `db::doc_mount_points`
repo plus the new `find_all_full_json` + the two embedded-count reads
(the cheap LIST GROUP-BY `IS NOT NULL` and the expensive GET-[id]
`IS NOT NULL AND length > 0`) and the pure `deriveMountCapabilities`.
Pinned quirks: the LIST is `{mountPoints}` (createdAt DESC, no
capabilities); GET-[id] adds `embeddedChunkCount` + `capabilities`;
create returns the in-memory validated mount (nulls present) +
optional `warning` (the `verifyBasePath` seam is injected — a
non-database mount always warns, and the differential drives a
nonexistent path so v4 agrees); the PATCH handler's single try/catch
means a bad body 500s (not 400) and the echo omits count/capabilities;
DELETE runs the exact ordered cascade (chunks → files [+ orphan GC] →
documents → blobs → folders → project-links → the point). The twelve
action verbs + semantic-search get NO variants this round (D7).
Differential `mount_points_routes_equivalence` (13 cases, incl. the
character-scaffold folder dumps and the full cascade table dump)
against v4's real route handlers.

P4.6p unit 3 (lane A): the image-profiles dispatch surface — the five
CRUD variants (list incl. `?sortByCharacter=` / create / get / update /
delete) + `imageProviderList` as `api::image_profiles`, composed over the
ported repo plus new reads (`find_by_user_id`, `find_id_by_user_and_name`,
`unset_all_defaults`, and the nullable-clearing `IpUpdate` tri-state for
apiKeyId/baseUrl), the api-key + tag enrichment, and the manifest
Registry. The three LLM/IO-coupled actions (generate / validate-key /
list-models) are loud typed refusal arms. Added an `imageGenerationModels`
field to the five image-capable provider manifests (byte-exact
transcription of v4's plugin model lists) so `list-providers`'
`defaultModels` matches. Differential `image_profiles_routes_equivalence`
(18 cases, incl. the default-first + createdAt-DESC sort, the
sortByCharacter matching-tag re-sort with v4's default-last tie-break, the
apiKeyId-null-clears / ''→null-baseUrl / isDefault-unsets-others side
effects, list-providers, and every validation arm) against v4's real
route handlers. The provider probe's reject path is unit-tested directly
(the v4 registry probe is a no-op in the jest sandbox); its accept path is
covered end-to-end by the create happy-path.

P4.6p unit 2 (lane A): the roleplay-templates dispatch surface — the
five variants (list / create / get / update / delete) as
`api::roleplay_templates`, composed over the ported repo + new
full-JSON reads (`find_full_json_by_id`, `find_all_for_user`,
`find_id_by_user_and_name`) and the pure `generate_rendering_patterns`.
Extended `ErrorKind` with `Forbidden` (403) and `Conflict` (409) — v4's
`responses.ts` vocabulary the built-in guards and duplicate-name arms
need — and mapped both in the `quilltap-web` transport. Differential
`roleplay_templates_routes_equivalence` (21 cases) against v4's real
route handlers. Pinned v4 quirks: the LIST is a bare JSON array
(built-in-first, then localeCompare); `narrationDelimiters` reads back
as a raw string (not a registered JSON column); GET auto-regenerates
empty rendering patterns non-persisted; and the PUT `updateData` always
overwrites name/description/systemPrompt, so a partial body omitting
name or systemPrompt 400s on the full-schema re-validate and every
update drops `description` unless a string is supplied.

P4.6p fixture extension (lane A): extended the shared groups-projects
fixture with the listing-surfaces rows — the two built-in roleplay
templates (via v4's real seeder) + two user templates, four tags, a
tagged character (DIANA), three image profiles, and a dedicated
"Indexed Store" mount carrying one embedded chunk. All additive and
invisible to the existing groups/projects/scenarios reads;
regenerated those three oracles and re-verified their differentials
green (14/39/41 cases) to confirm zero perturbation.

P4.6p unit 1 (lane A): ported `generateRenderingPatterns` (v4
`lib/chat/annotations.ts`) as the pure `services::annotations`
module — the rendering-pattern auto-generation the roleplay-template
routes use when a template carries no explicit `renderingPatterns`
(all three delimiter kinds, add-on class composition, the
same-open/close lookaround, the `]`-suffix markdown-link exclusion,
the kind-tagged dedupe key, narration append). Tier-1 EXACT
differential (`annotations_rendering_patterns_equivalence`, 25 cases)
against v4's real code.

Work orders for the P4.6p ∥ P4.6q ∥ P4.6r round (docs only). Three
fresh surveys at v4 `a7b1398d` (drift check clean); three lanes: P4.6p
— the listing-surfaces server round (roleplay templates + image
profiles + global mount points, closing the three P4.6l picker gaps);
P4.6q — the New-Chat SPA vertical (/salon/new + the Green Room; the
server side is already live, so the lane is SPA-only); P4.6r — the
Templates & Images settings tabs + picker enablement + the
reset-builtins button rider. Round plan appended to phase-4.md. Survey
finding recorded: the D16 server-side markdown renderer resolved to
omission (v4 renders at GET-time; v5's locked divergence renders
client-side) — no core port needed.

The P4.6n ∥ P4.6o ∥ P4.4u4 round gate + close-out. Full gate green:
fmt/clippy (both feature sets)/release build clean; the six round
oracles regenerated fresh from v4 `a7b1398d` and every differential
green by name; cargo test --workspace 289 suites / 1,243 tests / 0
failed; ng test 256; ng build clean; Playwright 19/19 including the
newly-activated scenarios + wardrobe walks. Gate fixes (assertion/
gesture class only): the web contract setup-flow test now asserts the
v4-parity seeded fresh boot (2 characters, 42 memories, 5 mount
stores) after the seed default-ON flip; the projects-flow walk scopes
its header Edit (the P4.6o wardrobe rows added same-named buttons);
the scenarios walk gained a rowAction helper for the container-query
kebab. Orders P4.6n / P4.6o / P4.4u4 CLOSED — closing P4.6k, P4.6l,
and P4.4u3's family-3 deferral with them. No refusal arms remain in
the groups/projects/scenarios surface. Versions: web 0.0.12, SPA
0.5.22.

Unification wires for the P4.6n ∥ P4.6o ∥ P4.4u4 round. The A↔B scenario
contract diffed name-for-name and field-level: 19 request variants + the
opaque scenario bag identical on both sides (nested bag, newFilename;
dispatchData is response-tag-agnostic) — no reconciliation needed this
round. The reset-builtins wire landed at the web edge, not as a core
dispatch arm: `POST /api/v1/characters?action=reset-builtins`
(quilltap-web characters routes) calls the differential-proven
`quilltap_import::reset::reset_builtins` inside one Db::write with the
host image codec — codec-needing legs live at the edge (the P4.6m
precedent), and core has no codec seam. Route-level roundtrip test added
(fresh import round + delete-and-re-mint round; post-reset ids DIFFER
from preserved per v4's create-mints quirk the differential pinned).
`HostConfig::seed_sample_content` flipped to default ON (v4 parity);
the builtin-seeds host test opts out (it pins the 3 built-in mount
stores; the seed adds the two character vaults on top). Lane B's
fixture-guarded scenarios/wardrobe e2e beats auto-activate over lane
A's committed groups-projects fixture. Versions: web 0.0.11, host
0.0.12.


P4.6o (lane B, SPA) — the Scenarios + Wardrobe SPA remainder. Re-pinned
the SPA scenario contract (`core-contract.ts`) to v4's Zod-schema shape:
`groupScenario*`/`projectScenario*` create/update now ride a nested
`scenario` bag (`{filename, name?, description?, isDefault?, body}` on
create; no `filename` on update), rename takes `newFilename`, and the
`ScenarioDto` gains `filename`/`rawIsDefault`/`body`/timestamps
(superseding the `{name, content, isDefault}` sketch). Added the six
net-new general (instance-wide) `scenario*` request variants and a
`WardrobeItemDto` + slot-type. (Lane A makes the matching Rust change;
reconciled at unification.)

Built the scope-agnostic `qt-scenarios-manager` family (manager + row +
editor modal + a `ScenarioMutator` service interface with project- and
general-scoped factories over `CoreClient`). The manager makes no
dispatches itself — the scope lives in the mutator. Delete confirms,
rename prompts on the FILENAME, set-default re-sends update with
`isDefault: true` (no dedicated verb); the editor ships a plain textarea
(the established Lexical divergence). Added a `closeOnBackdrop` input to
the shared Modal (default true) for the no-click-outside editor.

Built the `qt-project-wardrobe-manager` (self-contained inline draft
form + rows) + a project-scoped wardrobe mutator over the
`projectWardrobe*` verbs. Blank optional strings ride as `null` (v4
`handleSave`); the composite picker excludes the item being edited; the
slot-type floor keeps at least one slot. Unit-tested against a mock
mutator: draft round-trip, blanks→null payload, self-exclusion, the slot
floor, badge states, delete confirm (7 specs).

Wired the two managers into the Prospero project detail: the Scenarios
and Wardrobe cards replace their loud-disabled placeholders (closing the
P4.6l remainder), and the general `/scenarios` page renders the manager
at page scope behind the now-enabled `scenarios` nav item (route
registered). When the general mount is unprovisioned the list is empty
and mutations surface the server refusal — matching v4.

Authored the Playwright beats (activated at unification over lane A's
fixture): `scenarios-flow.spec.ts` (project card create → `.md` suffix →
edit → set default → rename → delete; general page create + list) and a
wardrobe beat in `projects-flow.spec.ts` (create → badges → delete).
Fixture-guarded skip until the groups-projects fixture lands.

P4.4u4 unit 3 (lane C, tier 2): reset_builtins as a service. Ported v4's
`handleResetBuiltins` to `quilltap-core::services::quilltap_import::reset::
reset_builtins` — cascade-delete the built-in characters (Lorian, Riya),
re-import the committed seed with the seed-id → preserved-id remap
(`replace_mapped_ids_recursively` + `find_builtin_character_ids`), then
reseed the built-in avatars. Composes the already-differentially-proven
`execute_cascade_delete` (P4.6i), `execute_import` (unit 1), and
`seed_avatars` (unit 2). Reproduces v4's quirk that `create` mints fresh
ids, so the "preserved" ids are not actually preserved (postResetIds
differ). Proven by `reset_builtins_equivalence` (tier-2: v4's real
`handleResetBuiltins` driven through the collection route over a
pre-seeded instance vs the service — the result shape, the post-state
counts, and the normalized post-reset characters/memories rows). The
qtap-import fixture builder now also materializes the cascade-touched
tables (chats/files/character_plugin_data/vector_indices/vector_entries).
The dispatch arm (the Request variant + api/characters.rs arm) is deferred
to unification (lane A owns api/types.rs this round).

P4.4u4 unit 2 (lane C): the startup sample-content seed wire. Ported v4's
`seedInitialData` gated tail (`quilltap-core::services::quilltap_import::
seed`): the zero-characters gate, `seedFromImports` (import the committed
`.qtap`), and `seedAvatars` (match each seed avatar to its character by
name, idempotency check, delete-then-insert vault write, `defaultImageId`
update) + `reseedAvatarsForCharacters`. Every layer swallows and collects
its errors (seeding never blocks boot). The seed assets are embedded into
the binary (`seed_assets`). Wired into the host `assemble` step behind a
new `HostConfig::seed_sample_content` flag (default OFF this lane — the
default-on flip + the fresh-boot e2e fixture updates land at unification).
Corrected a stale survey note: BOTH seed characters carry an avatar file
at v4 `a7b1398d` (Lorian.webp + Riya.webp) — committed both; seeding
produces two avatars, not one. Proven by `seed_avatars_equivalence`
(tier-2: v4's real `reseedAvatarsForCharacters` vs the port — both WebP
blobs diffed sha-exact since WebP is stored as-is, plus the idempotency
no-op) and a host fresh-boot smoke (`host_sample_content_seed`: first boot
seeds Lorian + Riya + 42 memories + 2 avatars + 8 wardrobe .md; second
boot short-circuits on the gate).

P4.4u4 unit 1 (lane C): ported the seed subset of the quilltap-import
service (`quilltap-core::services::quilltap_import`). The legacy
monolithic-JSON parse + the `format`/`version` hard pins, the subset
`execute_import` (characters with the legacy `scenario` → `scenarios`
migration + per-character vault-backed wardrobe, then remap-only
memories, then the character reconcile loop), `conflictStrategy: 'skip'`
(id-then-name existence check), the shared id-map, and the
counts/warnings result shape. Everything outside the subset is a hard
typed refusal (unsupported entity kinds enumerated, NDJSON sniff,
non-skip strategies, non-empty pluginData) — a deliberate divergence
from v4, which would import them. Committed the byte-identical
`assets/first-startup/{imports/lorian-and-riya.qtap,avatars/Lorian.webp}`
sample-content assets. Proven by the `qtap_import_equivalence` tier-2
differential (v4's real `executeImport` over the committed `.qtap` vs
the port: characters + wardrobe-vault + memories + the vault structure,
plus the name-match skip branch on a 2nd import). Dispatch/seed wiring
lands in later units.

P4.6n unit 5: made the project `list-files` two-branch + add-file/remove-file
arms live. Branch A (a linked primary Scriptorium store present) lists that
store's `doc_mount_files`; branch B (no store) lists the legacy `files` rows
scoped to the project, deriving each file's effective folderPath and omitting
null description/width/height (the read marshaling drops them). Added the
`mime::detect_mime_type`-backed `mimeForMountFile` and
`folder_utils::resolve_effective_folder_path` uses, a `files` listing read,
and a NULL-clearing project-link update. Proven by the `projects_routes`
differential extended with the two-branch list (Iota store-backed / Lambda
legacy / Kappa empty), add-file (+ file-row dump), add-file-missing(404), and
remove-file. **This closes the P4.6n surface — no tier-3 refusal arms
remain.**

P4.6n unit 4: added the general (instance-wide "Quilltap General")
scenarios family — six NEW dispatch variants (`scenarioList`/`Create`/
`Get`/`Update`/`Rename`/`Delete`) + `api::scenarios` general handlers that
resolve the singleton mount from `instance_settings.generalMountPointId`,
with the pre-provision race arms (GET → `{mountPointId:null, …}`; mutations
→ 400/404 "Quilltap General mount has not been provisioned yet"). Proven by
the `scenarios_routes` differential extended with 13 general cases,
including the default-conflict warning (two scenarios both marking
isDefault → the alphabetically-first wins, the other is demoted-in-response)
and the three race arms (the pointer deleted on both differential sides).

P4.6n unit 3: made the projects scenarios surface live — the six
`projectScenario*` dispatch arms, mirroring the groups family over the
shared `api::scenarios` CRUD but ensuring ONLY `Scenarios/` (no Knowledge)
and resolving the project's official store (overlay find for the collection
routes' name; the RAW FK for the item routes). Proven by the
`scenarios_routes` differential extended with 12 projects cases (Iota's
opening[default]/climax scenarios).

P4.6n unit 2: made the groups scenarios surface live — the six
`groupScenario*` dispatch arms + the `groupScenariosUnion` participant
aggregation, composing the shared mount-scoped scenario CRUD (new
`api::scenarios` module: validate-bag, filename sanitisation, the
collision guard, write + set-default, the fresh re-list) over each
group's official store (ensuring both `Scenarios/` and `Knowledge/`).
The union re-resolves every requested character through the user-scoped
lookup before the membership table (the security invariant), skips
zero-scenario groups, and sorts by group name (ICU4X en-US). Added the
`Response::Scenario` variant. Proven by the new `scenarios_routes`
differential (18 groups cases: reads/create/update/rename/delete + the
error arms + the union's multi-group/sort/skip/ownership-gate).

P4.6n: extended the committed groups-projects test fixture for the
scenarios surface — added groups Beacon (member Aria, one scenario;
sorts before Gamma) and Zephyr (member Aria, zero scenarios) to exercise
the participant-union's sort + zero-scenario skip, and the singleton
"Quilltap General" mount + `instance_settings.generalMountPointId` + two
general scenarios that both mark isDefault (the default-conflict warning).
Gamma/Delta/the projects are untouched; the existing `groups_routes` (14)
and `projects_routes` (33) differentials re-verify green against the
regenerated `.db` files.

P4.6n unit 1: ported the scenarios-common service surface into
`quilltap-core::db::scenarios` — `parseScenarioDoc`,
`listScenariosInFolder` (ICU4X-collated sort + the alphabetically-first
default-conflict resolution + warning), `readScenarioByPath`,
`setScenarioDefaultInFolder` (sequenced no-transaction multi-write),
`buildScenarioFileContent`, and `resolveScenarioPath` (nested-path
rejection). Added the supporting mount-index reads (a `lastModified`
field on `VaultFolderDoc` + a full single-doc read
`find_with_link_by_mount_point_and_path`) and the `mime::detect_mime_type`
+ `folder_utils::{normalize_folder_path, derive_folder_path_from_storage_key,
resolve_effective_folder_path}` helper ports the list-files legacy branch
needs. Pure leaves unit-tested; the composed surface is proven by the
`scenarios_routes_equivalence` differential landing with the route arms.

Planned the P4.6n ∥ P4.6o ∥ P4.4u4 round and committed the three work
orders (docs only). P4.6n closes the P4.6k server remainder: the
scenario contract re-pinned at planning time from v4's Zod schemas
(create `{filename, name?, description?, isDefault?, body}`, identical
across the groups/projects/general families; update drops filename;
rename `{newFilename}`), the scenarios-common service surface, the 13
refusal-armed scenario arms + the participant-union, the net-new
general (instance-wide) scenarios family, and the list-files
two-branch + file add/remove. P4.6o closes the P4.6l SPA remainder:
the scope-agnostic ScenariosManager (project card + the general
/scenarios page behind the disabled nav item) and the Wardrobe card +
ProjectWardrobeManager. P4.4u4 closes the P4.4u3 family-3 deferral:
the quilltap-import seed subset (.qtap is plain JSON, not an archive;
characters + wardrobe + scenario-migration + memories, skip strategy),
the startup seeding wire with the zero-characters gate and avatar
seeding, and reset_builtins as a tier-2 service (dispatch wire at
unification). Round plan appended to phase-4.md; drift check clean at
`a7b1398d`.


Unified the P4.6k ∥ P4.6l ∥ P4.6m groups+projects+multipart round onto
main. Lane A landed the groups + projects (Prospero) dispatch surface
(CRUD/members/roster/chats/state/tool-settings/mount-points + wardrobe +
background/aesthetic, proven by the groups-routes [14] and projects-routes
[33] differentials, over a new committed groups-projects fixture); lane B
landed the Groups + Prospero SPA verticals (groups section + editor, the
/prospero list + card-grid detail, Projects nav) plus the characters
upload/PNG riders and the finding-#6 select audit; lane C closed ALL
three byte-shaped characters deferrals (multipart photo upload,
photo-save-fileid both storage modes, ST PNG export/import) with the
hand-rolled PNG codec proven byte-exact (st-png tier-1) and the reusable
quilltap-web multipart helper. Unification wires: the nested
group/project update-bag reconciliation, the .qt-page-container dialog
z-trap fix, and six live e2e beats. Gate: clippy clean both feature
sets, five fresh oracles at `a7b1398d` green by name, 283 workspace
suites / 1221 tests, ng test 237, ng build clean, Playwright 16/16.
Still refusal-armed: scenarios + participant-union and list-files
(P4.6k), the Scenarios/Wardrobe cards (P4.6l). Versions: core 0.0.176,
harness 0.0.161, web 0.0.10, SPA 0.5.16.


Unification wires for the P4.6k ∥ P4.6l ∥ P4.6m round (live-seam fixes
found by the first real-server walks). Contract reconciliation: the SPA's
`groupUpdate`/`projectUpdate`/`projectCreate` senders now ride the nested
`group`/`project` bags lane A pinned and differential-proved (the flat
form the order sketched was never live); `core-contract.ts` and the unit
tests updated with them. A real layering bug fixed: `.qt-page-container >
*`'s z-1 stacking context trapped any dialog opened from an early page
child beneath later siblings (the groups Create dialog was unclickable) —
a `:has(.qt-dialog-overlay)` exception raises the hosting child to the
dialog layer. New live e2e beats: the characters gallery multipart upload
and the SillyTavern PNG-export download (asserting the embedded card —
the container is v4-faithful to the avatar bytes, not necessarily PNG);
the groups/projects walks activated over lane A's fixture with
fixture-tolerant setup (absent `tags` table, un-scoped store-backed rows)
and post-commit-4 strict-mode locator scoping. Full Playwright suite
16/16 green. SPA 0.5.16.


P4.6k (lane A) unit 4 — project wardrobe CRUD. list / get / create / update /
delete over the project store's `Wardrobe/` folder (PROJECT_WARDROBE_FOLDER =
CHARACTER_WARDROBE_FOLDER), reusing the P4.6f vault-write functions
(create/update/delete_project_wardrobe_item, read_project_wardrobe). Create
mints id + ISO timestamps in the route (blanked in the differential); delete
runs removeEquippedItemFromAllChats warn-and-proceed. Update re-reads through
the overlay so the echo carries the full null-inclusive item shape. Proven by
projects_routes_equivalence (now 35 cases).

P4.6k (lane A) unit 5 (partial) — project background + aesthetic editor. The
`get-background` resolver (URL by `backgroundDisplayMode`: theme/static/project/
latest_chat, BARE envelope) and the lantern/aurora aesthetic get/set (get returns
the RAW store-file content; set writes, and an empty/whitespace body DELETES the
file to restore the fallback). Proven by `projects_routes_equivalence` (now 30
cases incl. write+readback). The `list-files` two-branch DTO remains a loud
`not_available` deferral (it needs the net-new `mimeForMountFile` /
`resolveEffectiveFolderPath` helper ports).

P4.6k (lane A) unit 2 — the Projects server surface at the core boundary.
Landed the projects CRUD (list with the faithful O(n²) `_count`, create
with default injection, get with enriched roster + `_count`, update,
delete that nulls chats/files `projectId` but leaves `projectDocMountLinks`
dangling), roster (hand-rolled add/remove per v4's route quirk, list),
chats (paginated list with the `lastMessageAt ?? updatedAt` sort fallback
+ enriched participants/tags/storyBackground, add/remove), state
(get/set/reset), tool-settings, and mount-points (list/link/unlink) — all
differential ports proven by `projects_routes_equivalence` (21 cases,
reads + mutations with table dumps). Added `project_doc_mount_links::
{unlink,link_returning}`. list-files/background/aesthetic/scenarios/
wardrobe still answer the loud `not_available` refusal until their units.

P4.6k (lane A) unit 1 — the Groups server surface at the core boundary.
New `api::groups` dispatch module + the pinned Groups/Projects `Request`
variants (the full Shared contract) + engine dispatch arms. Landed groups
CRUD, members (add/remove/list), and mount-points (list/link/unlink), each
a differential port of v4's real route handlers. Added a committed
groups-projects fixture (built via real v4 repos + store-write helpers:
2 groups, 3 projects, characters, scenarios, wardrobe, aesthetics, chats,
legacy + store-backed files, a dangling mount-link) and the
`groups_routes_equivalence` differential (reads + mutations, table dumps
for the delete/member/mount-link side effects). Repo additions:
`group_character_members::{add_member,remove_member,delete_by_group_id}`,
`group_doc_mount_links::{unlink,delete_by_group_id,link_returning}`,
`doc_mount_points::find_full_json_by_id`. Projects + scenarios variants
answer the loud `not_available` refusal until their units land.
Retired the closed characters deferral refusals and unified the duplicated
photo-link-summary (P4.6m unit 5). The dispatch `export-png` /
`photo-save-fileid` `not_available` arms now point at the live quilltap-web
REST routes (PNG export streams binary; the fileId photo save reads
host-stored bytes — both need the transport the dispatch channel can't
carry), and the import-png doc note reflects the new multipart route. The
`api::salon` message-attachment resolver now calls the shared
`photos::photo_link_summary::get_photo_link_summary_by_sha256` instead of its
byte-identical private copy (one implementation, all callers).

Added the SillyTavern multipart import route + the main-avatar vault write
(P4.6m unit 4). `POST /api/v1/characters?action=import` accepts a `.png` or
`.json` ST card (multipart): it creates the character through the ported
import spine and, for a PNG, lands the bytes as the imported avatar via the
new `write_main_avatar_to_vault` (v4 `writeCharacterAvatarToVault({kind:
'main'})` — the delete-then-insert at `images/avatar.webp`, WebP transcode
via the injected host codec) and sets `defaultImageId`. Avatar failure is
non-fatal (character kept). Closes the `import-png` deferral. Proofs: a
route-level integration test (PNG create + avatar + defaultImageId, verified
end-to-end by re-exporting the character; the JSON leg; the error arms) and a
tier-2 differential (`character-avatar-write-tier2`) driving v4's real
`writeCharacterAvatarToVault` — the link row's stable fields + the replaced-
count + the blob's decoded metadata (16×16 WebP) diffed exactly, the WebP
bytes/sha the declared codec seam.

Added the SillyTavern PNG-export route (P4.6m unit 3):
`GET /api/v1/characters/{id}?action=export&format=png` streams the ST card
embedded in a PNG `tEXt` chunk (the avatar bytes — vault link or legacy file
— as the container, or the generated placeholder), `Content-Disposition:
attachment`. The `format=json` leg (pretty ST card) and the error arms (404
unknown character, 400 non-export action) ride the same route. Closes the
`export-png` deferral. Route-level integration test: the real-avatar embed,
the placeholder round-trip, JSON, and the arms.

Gave quilltap-web its first multipart machinery + the photo-upload route
(P4.6m unit 2). New `multipart` module (a browser-`FormData`-shaped helper:
whole-body buffering, string-or-file fields, `get`/`getAll`) and
`POST /api/v1/characters/{id}/photos` with all three v4 legs — multipart
upload, JSON `linkId`, and JSON `fileId` (the two-mode `downloadFile`:
`mount-blob:` → the DB blob, else the disk backend) — behind the ported
`save_to_character_gallery` / `save_link_to_character_gallery` write spine
and v4's content-type dispatch + error mapping (404/400-keyword/500). Thin
edge code (the `files_routes` precedent). Proven by a route-level
integration test (all three legs in both storage modes + every error arm,
real HTTP bodies against the characters fixture) and a tier-2 differential
(`character-photo-upload-tier2`) driving v4's real `saveToCharacterGallery`
over the upload filename→path branches, the dedup refusal, and the two
400-keyword arms.

Ported the SillyTavern PNG codec (P4.6m unit 1, tier-1). Added
`create_st_character_png` / `parse_st_character_png` / the solid-colour
placeholder generator + CRC32 to `quilltap-core`'s `sillytavern` module —
hand-rolled PNG `tEXt`-chunk arithmetic with no image library, matching v4.
Byte-exact against the v4 oracle on the real-avatar encode leg and every
decode case (chara/ccv2/bare-data/malformed); the placeholder leg is
compared at the decoded level (identical IHDR + tEXt chunks and inflated
pixels — v4 zlib-compresses the IDAT, the port emits stored DEFLATE blocks,
the one declared seam). New oracle `harness/oracle/cases/st-png.ts` +
`st_png_equivalence` differential.
P4.6l (lane B, in progress) — the project-detail tier-2 cards. The **Files**
card lists the first 10 files (image thumbnails, name, size, category; a plain
lightbox on click; "Browse All Files" disabled — the FileBrowser family + file
upload defer). The **Image Generation** card ships the Avatar Generation,
Announce Lantern Images, and Story-Background immediate selects plus the two
aesthetic textareas (byte-exact round-trip via `projectAestheticGet/Set`); the
Default Image Profile select is a disabled affordance (no image-profiles listing
surface this round). Both wired into the detail card grid.

**Scenarios + Wardrobe defer LOUDLY** (disabled "not yet available" cards, not
silent omissions): the scenario dispatch body fields (v4 filename/body/
newFilename vs the pinned name/content) aren't reconciled by lane A yet, and the
project wardrobe inline form (360 ln) is banked — both are OPEN tier-2 remaining
for a follow-up slice. SPA 0.5.15.

P4.6l (lane B, in progress) — the characters riders + the `<select [value]>`
audit (dogfood finding #6 class). The Photo Gallery "Upload Photo" button is now
live: it multipart-POSTs to lane C's `POST /api/v1/characters/{id}/photos` web
route (failures surface the v4 400 keyword message). A second export button on
each roster card downloads the SillyTavern PNG card via
`GET ?action=export&format=png` (JSON export unchanged). Both call the byte-leg
web routes by `fetch` (dispatch can't carry bytes) — mocked in unit tests, live
at unification against lane C. The ST **import** PNG leg was already wired.

The `[value]` audit converted the five genuinely-risky async-options selects to
per-option `[selected]` (saved value + async options = the finding-#6 trap):
`cheap-llm-card` ×2, `profile-modal` provider + apiKeyId, `model-selection-step`
provider. Proven safe as-is (recorded, no change): the pronoun-preset select
(static options), the new-character connection-profile select (empty initial
value), the reverse-user dialog select (options from already-loaded data), the
api-key-modal provider select (create-only, starts empty), and the profile-modal
modelClass select (static options). SPA 0.5.14.

P4.6l (lane B, in progress) — the Projects (Prospero) vertical in the SPA, tier
1. The Projects nav item is enabled (`/prospero`); the list (grid/card/create
dialog/delete-with-confirm) and the routed detail (`/prospero/:id`) land. The
detail is a dense card grid with per-card expansion memory (all expanded on the
first visit, collapsed after — localStorage `quilltap_project_visited_{id}`):
Header (inline title/description edit + Save, New Chat link), Scriptorium
(linked stores + unlink, reusing the groups stores card), Characters ("Allow Any
Character" immediate toggle + roster grid with hover-remove; no add picker),
Model Behavior (Agent Mode + Answer Confirmation immediate selects), Settings
(instructions textarea + explicit Save + a Project State JSON editor modal), and
the full-width chats section (paginated, page size 20, the shared ChatCard in a
new removable mode — remove DISASSOCIATES). Every immediate-save select/toggle
catches and surfaces failures with v4's fallback microcopy.

Loud deferrals (no ported listing surface this round): the Default Roleplay
Template select and the Default Tool Settings row are disabled affordances with
v4-register tooltips; the project Scriptorium link-store picker is likewise
disabled (list + unlink live). Recorded divergence: Project Instructions use a
plain textarea, not v4's Lexical editor (bytes round-trip exactly). `ng test`
36 files / 229 tests green; `ng build` clean; `projects-flow` e2e beats skip
until lane A's fixture lands. SPA 0.5.13.

P4.6l (lane B, in progress) — the Groups vertical in the Angular SPA. The
Characters page now carries a Groups section above the roster (grid + card +
the toolbar Create Group dialog) and a routed group editor at
`/characters/groups/:id` (v5 path idiom; v4 used `/aurora/groups/[id]`). The
editor is an explicit-Save form (name/description/color/icon — no autosave)
over two collapsed-by-default cards: Members (list + X-remove + an Add-Member
`<select>` that binds `[selected]` per option, the finding-#6 discipline) and
"The Scriptorium" (linked stores list + unlink; the Link-store picker is a
disabled affordance since the global mount-points listing is not a ported
dispatch surface this round). All 18 group Request variants + 40 project
variants added to `core-contract.ts`; the group editor route registered.
Delete is immediate with no confirm (v4 behavior). Coded against a mocked
CoreClient (lane A pins the server side); the live `groups-flow` e2e beats
skip until lane A's fixture lands at unification. SPA 0.5.12.

Scoped the next porting round: three agent-ready work orders for the
P4.6k ∥ P4.6l ∥ P4.6m parallel round —
`docs/developer/porting/work-orders/p4.6k-groups-projects-server.md`
(the full groups + projects/Prospero dispatch backfill over the
Phase-2-ported repos, ~40 pinned Request variants, jest real-DB
differentials, a committed groups-projects fixture),
`p4.6l-groups-projects-spa.md` (the groups section + editor on the
Characters page, the `/prospero` list + card-grid detail, the Projects
nav item, plus the characters upload/ST-PNG affordances and the
dogfood-#6 `<select [value]>` audit as riders), and
`p4.6m-multipart-binary-routes.md` (quilltap-web's first multipart
machinery + the three v4-shaped routes closing the photo-upload,
photo-save-fileid, and SillyTavern-PNG deferrals, with the hand-rolled
PNG tEXt codec as a tier-1 byte-exact port). Fresh v4 surveys informed
all three; oracle baseline unchanged (`a7b1398d`).

Unified the P4.6i ∥ P4.6j characters-remainder round onto main. All eight
characters `not_available` arms are live and differential-proven (delete
cascade + cascade-preview, per-character chats, photo gallery list/save-by-
linkId/remove, ST import/export JSON), and the SPA's Conversations tab,
delete flow, gallery, and ST import/Export-JSON ride them. Unification
wires: the gallery contract reconciled to the pinned
`{entries,total,hasMore}` envelope (gallery tab + avatar picker on
`linkId`/`blobUrl`) and the three live `characters-flow` e2e beats
activated with their gestures fixed. Gate: clippy clean both feature sets,
fresh characters oracles at `a7b1398d` (24 + 22 cases) with both
differentials green by name, 275 workspace test suites green, 206 SPA unit
tests, `ng build` clean, the full 10-test Playwright suite green. Orders
P4.6f/g/i/j all CLOSED; remaining deferrals are enumerated loud refusals
(ST PNG, photo multipart upload, photo-save-fileid, the tier-3 LLM
services, the wardrobe dialog vertical). Versions: core 0.0.172, harness
0.0.157, SPA 0.5.8.

Scoped the characters-remainder round: two agent-ready work orders that close
the OPEN slice-5 remainder of P4.6f/P4.6g —
`docs/developer/porting/work-orders/p4.6i-characters-remainder-server.md`
(delete cascade + cascade-preview, per-character chats, the photo gallery
JSON legs, ST import/export JSON — with the `deleteMemoriesWithUnlinkBatch`
and `character-gallery-service` ports and their differentials) and
`p4.6j-characters-remainder-spa.md` (the Conversations tab, the live
delete/gallery/import flows, the ST-export action). The parent P4.6f/P4.6g
status headers now point at them. Oracle baseline unchanged (`a7b1398d`).

### 5.0-dev

P4.6x (lane C, in progress) — the Document Mode SPA state store + the
core-contract document block. The `useDocumentMode` port lands as an
Angular signals store (`DocumentModeController`) with the open-document
set, focus, dirty tracking, 30s autosave + flush, the 409-conflict
reload (no retry), Librarian-announcement append, and the shared
`dividerPosition` ownership (moved off the terminal controller, matching
v4). Pure ports: the qtap:// URI producer, the frontmatter/word-count
helpers, and the Myers unified-diff for autosave notifications. The
dispatch client (`DocumentApi`) reads lane B's document family + lane A's
`mountFilesList` defensively off the envelope. Contract: the single-author
document block + `mountFilesList` request variants, `documentMode` merged
onto `ChatDetail`. SPA 0.5.44.

P4.6x unit 7 — the Document Mode e2e beats (fixture-guarded). A live Salon
walk: open a chat → open the picker → new blank document → the pane renders
→ edit → flush-save → the edit survives a reload → rename (Librarian chip)
→ close (pane collapses, Open-document button returns), plus a both-panes
beat (document + terminal stacked). Guarded by a runtime capability probe
that skips the describe when the shared server lacks the document dispatch
(the in-lane signature) and auto-activates once lane B's arms land at
unification; `chat_documents` is materialized pre-launch in global-setup.
The full Playwright suite is green (27 passed, 2 guarded-skip) — the
terminal walk confirms the dividerPosition ownership move didn't regress.
SPA 0.5.49.

P4.6x unit 5 — tool-result reload wiring. A persistent Salon subscription
watches the live stream for the LLM's document tools (v4 `onToolResult`):
`doc_open/close_document` + the `doc_write/move/delete_file/folder` family
reload the open set from the server; `doc_focus` routes to the pane that
owns the target document. On turn end, every open document is re-read
(dirty panes skipped) so an unsurfaced LLM edit still lands. SPA 0.5.48.

P4.6x unit 3 — Document Mode split integration. The `DocumentModeController`
+ `DocumentPane` ride the frozen `SplitLayout` / `RightPaneVerticalSplit`
`documentContent` slot; a combined mode (focus/split/normal across the two
panes) drives the layout, and the document + terminal stack vertically when
both are open. `dividerPosition` ownership moved off the terminal controller
onto Document Mode (matching v4); the terminal keeps `rightPaneVerticalSplit`.
A composer "Open document" button opens the picker; a selection opens/creates
the doc; Librarian announcements refetch the chat so the collapsed chip
appears. SPA 0.5.47.

P4.6x unit 4 — the `qt-document-picker` modal (v4 `DocumentPickerModal`,
chat-scoped): the source step (new blank, recents, the four store
accordions + look-everywhere) and the browse step (a mount point's folder
tree over lane A's `mountFilesList`, breadcrumbs, folder navigation, and
"new document here"). Deferred loudly: the project/general FileBrowser path
(no listing endpoint consumed this round) and the in-picker new-folder
control (needs `mountFolderCreate`). SPA 0.5.46.

P4.6x unit 2 — the `qt-document-pane` component (v4 `DocumentPane`):
click-to-rename title, focus/split toggle, delete (confirm), close, the
qtap:// URL row with copy, the frontmatter "Document Info" table (markdown
only), the status bar (Markdown/Plain text · word count · Saved/Unsaved/
Saving · AI-editing), and the byte-exact textarea editor. Markdown files
split the frontmatter into the table and edit the body only, recombining
`rawBlock + body` on change so on-disk bytes stay faithful. SPA 0.5.45.

P4.6x — the D17 Lexical spike for the Document Mode markdown editor is
RED. The sanctioned vanilla scope (lexical + @lexical/rich-text +
@lexical/markdown) round-trips headings/text-formats but throws on any
list, code fence, or table, and v4's non-lossy markdown needs its custom
preservation bridge — a safe port would mean half-porting a second editor.
Markdown ships in the byte-exact textarea too; ProseMirror stays the named
next-round decision. Recorded on phase-4.md's D17 line.

Dogfood finding #6 root cause FIXED (code in `ab985d4`): the Default
Settings tab's saves were succeeding all along — the profile/partner
selects never displayed the stored value because a select-level `[value]`
binding fires before the async-loaded options render, silently resetting
to "" (Angular re-fires nothing when the options arrive; React
re-renders). The profile/partner/prompt/scenario selects now bind
`[selected]` per option, with regression tests that deliver the options
after first render. Verified live against the Friday copy (stored profile
+ partner display; an edit round-trips). ~8 more `[value]`+dynamic-options
sites are listed for audit in dogfood-findings' standing notes. SPA 0.5.11.

Dogfood finding #6 (Friday smoke, partial): the Default Settings tab
appeared to reject edits on real data. Confirmed port divergence fixed: v4
surfaces every failed defaults save via an error toast; v5's autosave had
try/finally with no catch, so a server-side rejection silently reverted the
control with nothing shown. The tab now renders a `qt-alert-error` with the
server's message (v4's per-control fallback microcopy otherwise), plus unit
tests for both paths. The underlying real-data rejection is still to be
identified from the now-visible error (saves verified working end-to-end
against the fixture instance). SPA 0.5.10.

Dogfood finding #5 (Friday smoke): the System Prompts view tab rendered a
prompt containing the character's name as scattered fragments with huge
gaps. Cause: v5 had inlined v4's `TemplateDisplay` markup into the tab
template inside a `<pre>` element, and Angular preserves template whitespace
inside `<pre>` — every highlight segment rendered wrapped in the template's
own newlines and indentation. Fix: port v4's shared `TemplateDisplay` as
`qt-template-display` (compiled outside any `<pre>`, so default whitespace
stripping applies) and use it from both the System Prompts and Details tabs
(deduplicating the inlined copies). New unit test pins the rendered
`<pre><code>` text byte-exact to the prompt content. SPA 0.5.9.

P4.6i/j unification wire — the gallery contract reconciled to lane A's pinned
`{ entries, total, hasMore }` envelope (each entry `{linkId, mountPointId,
relativePath, fileName, blobUrl, mimeType, sha256, fileSizeBytes, keptAt,
caption, tags, linkSummary}`): the SPA `CharacterPhoto` type now IS that entry;
`fetchCharacterPhotos` drops the legacy `photos`/`images` fallbacks; the
gallery tab tracks/removes by `linkId` and renders from `blobUrl`; the avatar
picker (a latent pre-unification consumer of the old shape) selects the
`linkId` — which is what `characterAvatar {imageId}` stores for vault photos —
and renders from `blobUrl`. The three P4.6j `characters-flow` e2e beats
(Conversations → Salon link, cascade-delete a throwaway, gallery list +
remove) are activated (`test.fixme` dropped) with their gestures fixed for
the live walk: unlock-state-tolerant entry (the file's beats share one
server), the throwaway card clicked by its `h2` title (a quick-create has no
description, so `p.line-clamp-3` is empty and unclickable), the dialog
confirm scoped to `qt-character-delete-dialog` (the edit view's danger-zone
button keeps the same accname under the overlay), and gallery tiles counted
by their delete affordance (a bare `img` count catches the header avatar).
SPA 0.5.8.

P4.6j unit 4 — ST import verified + Export (JSON) action, and the live e2e beats
(SPA). The SillyTavern import dialog already reads a JSON file client-side and
dispatches `characterImport {payload}` (PNG rides the deferred multipart web
route) — verified with new specs (parse → dispatch → refresh; malformed-file
error). Replaced the roster's `window.open` export with a dispatch-based
Export-JSON: `characterExport {format:'json'}` returns the ST card, downloaded
client-side as `<name>.json` via a Blob. Added the three live characters-flow
e2e beats (Conversations→Salon link, delete-via-cascade-dialog, gallery
list→remove) as `test.fixme`, activated at unification over lane A's fixture.
SPA 0.5.7.

P4.6j unit 3 — the photo gallery, verified against the finalized envelope (SPA).
v4's `/photos` returns `{ entries }` where each entry's `id` is the vault
`doc_mount_file_links.id` (the linkId), plus `caption` / `tags`. `fetchCharacterPhotos`
now reads `entries` first (legacy `photos`/`images` kept as a fallback until lane
A pins the bytes at unification); the gallery tile renders the caption (as the
image `alt`/`title` and a bottom overlay) and remove uses `linkId ?? id`, so an
entry that carries only `id` still deletes correctly. Upload stays the deferred
multipart web route (disabled control + inline note). SPA 0.5.6.

P4.6j unit 2 — the delete + cascade-preview entry point (SPA). The existing
`character-delete-dialog` is byte-faithful to v4 (it renders title +
messageCount per exclusive chat and the total image count — v4 does not render
per-chat `lastMessageAt` or the three separate image counts), so it is
unchanged. Added a "Delete Character" affordance to the character EDIT view's
danger zone (next to Rename/Replace): it opens the cascade dialog, dispatches
`characterDelete {cascadeChats, cascadeImages}`, drops the roster cache, and
navigates to `/characters` (the character's own pages no longer exist).
Divergence: v4 deletes only from the roster `AuroraView`; this detail/edit
entry point is an additive SPA affordance the work order requests. SPA 0.5.5.

P4.6j unit 1 — the character Conversations tab (SPA). Replaced the empty-state
placeholder with the real per-character chat list over the `characterChats`
dispatch: a debounced search box, offset pagination (v4 `CHATS_PER_PAGE = 10`,
infinite-scroll sentinel plus a "Load more" fallback), and a display-only chat
card (title, message/memory badges, a static scriptorium badge, the dangerous
marker, relative date, preview text, project + tags) that links into
`/salon/:id`. New contract types `CharacterChatSummary` / `CharacterChatsResult`
and a `fetchCharacterChats` api helper. Ported v4's `formatChatListDate` and the
`getCharacterChatPreview` quirk (preview is the oldest of the recent three)
verbatim. Divergence: the story-background thumbnail renders when present (v4's
`ChatCard` hides it here behind `showAvatars=false`); the v4 per-card
delete/re-extract/re-render and refresh-archive actions hit routes outside this
vertical's contract and are omitted. SPA 0.5.4.
P4.6i (characters server remainder, lane A): ported the character
cascade-delete preview + executor (`services::cascade_delete`). Preview
(`CharacterCascadePreview`) composes the exclusive-chat / exclusive-image /
exclusive-chat-image finders + memory count over the RAW character row
(broken-vault-safe). Delete (`CharacterDelete`, `findByIdRaw` ownership) runs
the destructive fan-out: exclusive chats + their images, exclusive character
images (vault-links via the gallery remove, legacy files via the GC-safe file
delete), memories via the unlink-batch chokepoint, the vector index, plugin
data, and the slim row. Wired both dispatch arms — the last two of the eight
characters `not_available` refusals are now live. The legacy-`files`
exclusive-image branch and `findExclusiveImagesForChats` are ported faithfully
but not corpus-exercised (the fixture avatar is a vault-link with no chat
attachments); `deleteFileCompletely`'s host byte reclaim is a host seam.
Differentials: `characters_reads_equivalence` +cascade_preview;
`characters_mutations_equivalence` +character_delete_cascade (the
`{success,deletedChats,deletedImages,deletedMemories}` body AND the full
cascade-touched multi-table dump across both DBs — characters / chats /
messages / memories / plugin data / vault links / files / blobs). Green at
a7b1398d. This CLOSES the P4.6f server remainder except the enumerated tier-3
refusals.

P4.6i (characters server remainder, lane A): ported the character photo
gallery SAVE-by-id JSON leg (`save_to_character_gallery` +
`save_link_to_character_gallery`). The `linkId` leg reads bytes from the
source vault link's mount-blob and hard-links a copy into the character's
`photos/` folder (deduped by sha256; kept-image markdown sidecar with a
character attribution) — fully DB-resolvable and LIVE. The `fileId` leg reads
bytes via the host file store the characters dispatch doesn't wire, so it
stays a loud `not_available("photo-save-fileid")` deferral (alongside the
multipart upload web-route deferral). Wired the `CharacterPhotoSaveById`
dispatch arm. Differential: `characters_mutations_equivalence` +photo_save_link
— under a frozen clock (matching keptAt injected both sides) the return value
AND the written `photos/` link row (relativePath / fileId / mime / kept-image
markdown) diff byte-exact.

P4.6i (characters server remainder, lane A): ported the character photo
gallery LIST + REMOVE JSON legs (`photos::character_gallery_service::
{list_character_gallery, remove_from_character_gallery}` + the shared
`photo_link_summary::get_photo_link_summary_by_sha256`). List surfaces the
vault `photos/` folder plus the historic `images/avatar.webp` +
`images/history/*` portraits, most-recent first, each entry carrying its
mount-blob URL / caption / tags / reverse-index link summary. Remove clears
the character's `defaultImageId`/`avatarOverrides` pointers, then reclaims the
link (and its file + blob when it was the last reference) through the GC-safe
`deleteWithGC` chokepoint (now reports `fileGC`). Wired the `CharacterPhotoList`
/ `CharacterPhotoRemove` dispatch arms (save-by-id stays a loud refusal until
the save unit lands). Differentials: `characters_reads_equivalence` +photo_list;
`characters_mutations_equivalence` +photo_remove_avatar (diffing the
`{deleted,fileGC}` body AND the mount-index GC-table dump — the removed link /
reclaimed file+blob / nulled `defaultImageId`). Both oracles now un-mock
`character-vault-bridge` (jest.setup stubs it) so the vault resolves against
the real DB.

P4.6i (characters server remainder, lane A): ported the SillyTavern
character-card JSON legs (`services::sillytavern::{export_st_character,
import_st_character}`). Export (`CharacterExport` format=json) turns the
overlaid character into a `chara_card_v2` card the SPA downloads. Import
(`CharacterImport`, JSON body) unwraps the card, creates the character
directly through the repo (so `sillyTavernData` lands in the slim column and
no create schema runs), and echoes the slim create shape. The PNG legs
(export/import) stay deferred to the quilltap-web multipart route (loud
`export-png` refusal). Differentials: `characters_reads_equivalence` gains
an `export_json` case; `characters_mutations_equivalence` gains an
`st_import_card` case diffing both the create echo and the created
character's overlay readback (proving the ST-derived scenarios / systemPrompts
/ firstMessage / exampleDialogues / sillyTavernData round-tripped).

P4.6i (characters server remainder, lane A): ported the `?action=chats`
enriched recent-chats DTO (`api::characters::character_chats`) — per-character
chats filtered to the caller, `lastMessageAt` round-tripped through JS
`Date`, stable desc sort, case-insensitive search over title + message
content, offset/limit pagination, and per-chat enrichment (project / tags /
`_count` / scriptorium status / 3 recent messages / story background /
`isDangerousChat`). Composed over already-ported repos; wired the
`CharacterChats` dispatch arm. Extended `characters_reads_equivalence` with
six chats cases (plain / search title+content+miss / limit / offset) against
v4's real route handler.

Four slash commands capture the porting-round workflow as repeatable process
docs under `.claude/commands/`: `/setupphase` (drift-check, scope the next
round, write parallel-lane work orders, report their paths), `/carryout
<order>` (execute one order as an isolated lane under the differential
discipline), `/unify <orders>` (cherry-pick finished lanes onto main,
unification wires, the full gate, cleanup, docs/memory), and `/dogfood
<orders>` (produce the hands-on test script for a landed round, then
diagnose/fix findings in place with the finding-class taxonomy and the
broad-gesture rule). Follow-up: the round-lifecycle handoff made explicit —
`/unify` also keeps the phase plan current and must make its "next ask"
reconstructible from docs alone; `/dogfood` gains a "leave the trail"
section (promote unfixable findings into the standing notes / order OPEN
lists, correct stale status headers immediately); `/setupphase` names the
five handoff sources and says to fix-and-flag any that are stale.

Dogfood finding #4 fixed: clicking a character card on `/characters` did
nothing unless the click landed on the name/avatar link. v4's `AuroraView`
card is clickable anywhere (`handleCardClick`, ignoring clicks that land on
inner buttons/links); the v5 card had narrowed the target to the avatar+name
`<a>`. The card now carries the whole-card click with v4's
`closest('button')`/`closest('a')` guard (the inner link stays, so
middle-click still works). A unit test proves navigate-from-body /
no-navigate-from-star (195 SPA tests), and the `characters-flow` e2e's
detail-open beat now clicks the card BODY instead of the name link. SPA
version 0.5.3.

P4.6f slice 4 is UNIFIED on main: the five lane commits (create/quick-create/
update, wardrobe mutations, tags CRUD + the six-table delete fan-out,
depiction-guidelines GET/PUT, stats) cherry-picked with only the CHANGELOG
conflicting, and the `characters-flow` e2e's two annotated beats RESTORED as
the unification wire: the add-tag beat mints a brand-new tag through the Tags
tab's Enter-to-create path (`tagCreate` + `characterAddTag`) and proves it
across a reload, and the edit-title→Save beat retitles Aria through the edit
screen (`characterUpdate`) and proves the write on the roster card after a
full reload. Two spec fixes en route: the "Edit Character" link renders on the
detail view's DETAILS tab (not the header), so the walk switches back off the
Tags tab first; and the now-three-reload walk gets a 60s budget. The P4.6f
order's remaining OPEN items: delete-cascade + cascade-preview, the
per-character `chats` read, the photo gallery, ST import/export (plus the
long-standing tier-3 refusal deferrals). Full gate: fmt + release build clean,
clippy (default and native-transport) clean, 1,207 workspace tests green with
all five characters/tags differentials re-verified against FRESH v4 oracles
(`a7b1398d`: mutations 18 / reads 15 / actions 11 / sub-resources 9 / tags
tier-2), 194 SPA unit tests, the SPA prod build, and the full 7-test
Playwright suite. Versions: core 0.0.167, harness 0.0.152, host 0.0.10,
web 0.0.7, SPA 0.5.2.

P4.6f (Characters server, lane A): the `stats` read action. `character_stats`
fans out the independent counts (memories / conversations / wardrobe items / the
vault file links / group memberships) and derives photos / knowledge / core /
characterFiles from the link relative paths (the `isPhotosRelativePath` predicate
+ the `SINGLE_FILE_OVERLAY_PATHS` health figure), hydrating the character's groups
through the overlay. `{ stats, groups }`. Composes ported reads only. The arm
replaces its `not_available` refusal. Differential: `characters_reads` extended
with `stats` (+ a `depiction_guidelines` GET case) — over the fixture Aria's
stats read memories 2 / conversations 1 / photos 1 / characterFiles 8-of-8.
Versions: core 0.0.167, harness 0.0.152.

P4.6f (Characters server, lane A): the depiction-guidelines GET/PUT actions.
`character_depiction_guidelines` (overlaid `findById` ownership → RAW single-tier
read of `depiction-guidelines.md` from the character's own vault root →
`{ content }`, `''` when no vault/file) and `character_depiction_guidelines_update`
(RAW `findByIdRaw` ownership so a broken-vault character can still edit →
`writeStoreFile`: empty/whitespace deletes the file, else create-or-update →
`{ success: true }`; no vault → BadRequest). Composes the ported
`database_store::{write,delete}_database_document` + the aesthetics module's
`DEPICTION_GUIDELINES_FILENAME`. The two arms replace their `not_available`
refusals. Differential: `characters_mutations` extended to 18 cases (depiction
get-empty / put-write / put-clear; each PUT reads the file back through the GET
path to prove the write landed). Versions: core 0.0.166, harness 0.0.151.

P4.6f (Characters server, lane A) slice 4d: the tags CRUD + the delete fan-out.
`tag_list` (`findAll` → search filter → `localeCompare` sort → the 6-entity
usage-count DTO), `tag_get` (full spread + `_count`/`totalUsage`), `tag_create`
(dedup by name → return the existing tag), `tag_update` (rename-conflict guard +
name/quickHide/visualStyle), and `tag_delete` (the multi-entity fan-out — remove
the id from every taggable table, then delete the tag). All six taggable tables
(characters / chats / connection_profiles / image_profiles / embedding_profiles /
files) live in MAIN, so these are main-only. New `tags::{find_all, find_by_name,
count_tag_usage, remove_tag_from_table, TAGGABLE_TABLES}` and a `visual_style`
field on `TagUpdate`. The five arms replace their `not_available` refusals.
Extended the committed characters fixture: tagged the connection profile, image
profile, and legacy file with "Adventure" (so the delete fan-out exercises five
of six entity shapes with real mutations) and materialized the empty
`embedding_profiles` table (v4 auto-creates it via `ensureCollection`; the Rust
raw SQL needs it present). Extended `characters_mutations` to 15 cases (+ tag
list/get/create-new/create-dedup/update/delete); tag_delete additionally diffs
all six taggable tables + the tags table against the oracle's post-delete dump.
Versions: core 0.0.165, harness 0.0.150.

P4.6f (Characters server, lane A) slice 4c: the wardrobe mutation handlers.
`character_wardrobe_create` (mints id/timestamps → the vault-backed
`create_vault_wardrobe_item`), `character_wardrobe_get`
(`find_by_id_for_character`), `character_wardrobe_update`
(`update_vault_wardrobe_item` then a re-read for the echo), and
`character_wardrobe_delete` (equipped-reference cleanup via
`remove_equipped_item_from_all_chats`, then `delete_vault_wardrobe_item`), each
gated by v4's overlaid `findById` ownership. The four arms replace their
`not_available` refusals. Echo-shape seam proven against the oracle: v4's CREATE
echo is the constructed object (carries `migratedFromClothingRecordId: null`,
omits `archivedAt`), while the UPDATE echo is the full read-shaped item (includes
`archivedAt: null`) — so create serializes the write-struct (with
`migratedFromClothingRecordId` set to null) and update re-reads through
`find_by_id_for_character`. Extended the `characters_mutations` differential
with four wardrobe cases (create / get / update / delete; item ids discovered by
title since they mint at fixture-build). Versions: core 0.0.164, harness 0.0.149.

P4.6f (Characters server, lane A) slice 4a: the create / quick-create / update
handlers. `characterCreate` runs v4's `createCharacterSchema` shaping (slim
defaults, `controlledBy`→`'llm'`, `npc`→`false`, the managed-field bag off the
body) into the ported `create_character` (vault provisioning) then reloads
through the overlay; `characterQuickCreate` is the minimal name-only variant
with the fixed `"Character created during chat import"` description;
`characterUpdate` does `findByIdRaw` first (broken-vault characters stay
editable), whitelists the patch to the `updateCharacterSchema` keys with v4's
empty-string transforms, routes managed fields to the vault and the remainder
to the slim `_update`, and re-reads the overlay for the echo. The three arms
replace their `not_available` refusals. New `characters_mutations` differential
(oracle drives v4's real POST/PUT handlers; five cases — create-full,
create-minimal, quick-create, update-managed, update-slim — echo-diffed with
minted ids/timestamps blanked); the echo is a full overlay re-read, so it
transitively proves the vault round-trip in composition (the raw storage rows
stay proven by the standing create-tier2 / vault-update-tier2 differentials).
Versions: core 0.0.163, harness 0.0.148.

Docs: the P4.6f work order (`docs/developer/porting/work-orders/
p4.6f-characters-server.md`) now carries a status header marking slices 1–3
LANDED (unification `b29f2bb`) and enumerating the open slice-4 remainder, so
the order is self-contained for a fresh handoff.

The P4.6f ∥ P4.6g ∥ P4.6h ∥ P4.4u3 round is UNIFIED on main: the four lane
branches cherry-picked onto the reconciliation branch (zero source-level
conflicts — only version files and append-only docs), the P4.6f/g Shared
contract verified name-for-name (all 48 characters/tags request variants match
between `api/types.rs` and the SPA's `core-contract.ts`), and the
`characters-flow` Playwright walk UN-SKIPPED on a spec-private server over
lane A's committed characters fixture (the `salon-scroll` recipe): unlock →
the roster renders the fixture cards favorites-first → optimistic favorite
toggle → Aria's detail view → remove the baked "Adventure" tag → the change
survives a full reload. **Scope note:** P4.6f landed slices 1–3 of its order
(the read surface, the action verbs, the sub-resource mutations — each
differential-proven); the banked remainder ("slice 4": create/quick-create/
update, delete-cascade, wardrobe mutations, tags CRUD + delete fan-out,
stats/chats, the photo gallery, ST import/export, depiction-guidelines) stays
OPEN under the same order, and the SPA's edit-save / create / Default-Settings
autosave / add-tag surfaces answer its loud typed refusal until it lands — the
e2e's edit-title→Save and add-tag beats are annotated to be restored then.
Two unification fixes to the new e2e walks: the salon-scroll spec now DRAINS
the multi-strategy initial scroll (its last correction at +300ms yanked a
too-early scroll-up back to the bottom — v4 has the same window) and scrolls
up with REAL wheel input (a bare `scrollTop` assignment fires no scroll event
in a frame-throttled renderer, since scroll events dispatch during rendering
steps); the characters walk locates the favorite star by `title` (its
accessible NAME is the `☆` glyph — text content outranks the title attribute
in accname computation). Full gate: fmt + clippy (default and
native-transport) clean, the 847-test workspace sweep green, all six
new/extended differentials re-verified against FRESH v4 oracles (characters
reads / actions / sub-resources, builtin-templates, builtin-mounts,
provisioning incl. both cross-compat directions), 194 SPA unit tests, the SPA
prod build, and the full 8-spec Playwright suite. Versions: core 0.0.162,
harness 0.0.147, host 0.0.10, web 0.0.7, SPA 0.5.1.

P4.6f (Characters server, lane A) slice 3: the sub-resource mutation handlers
— prompts (`create`/`update`/`delete`/`set-default`), scenarios
(`create`/`update`/`delete`), and plugin-data (`upsert`/`delete`) — composed
over the already-proven `vault_character_arrays` + `character_plugin_data` ops.
One seam closed: the plugin-data upsert echo returns `data` as the input OBJECT
(v4's `upsert` returns the base create/update entity, whose `data` is the input
value, not the stored-then-re-parsed string that the item GET returns). Added
`character_plugin_data::delete_by_character_and_plugin`. Proven by
`characters_subresources_equivalence` (9 cases; update/delete target baked
sub-items resolved by name, creates normalize the minted id/timestamps) vs v4's
real route handlers. core 0.0.161, harness 0.0.146.

P4.6f (Characters server, lane A) slice 2: the thin action verbs
(`characters/[id]/handlers/post.ts`) as dispatch handlers —
`character_favorite`, `character_toggle_controlled_by`,
`character_toggle_carina`, `character_set_default_partner` (with its
partner-exists / must-be-user-controlled / not-self guards),
`character_avatar` (image resolve + `image/*` validation, set + clear), and
`character_add_tag` / `character_remove_tag` (the generic Taggable pattern
composed from `find_by_id` + `update_character`). The flip/avatar echoes
reproduce v4's base `_update` MERGE semantics (`validate({...preUpdateRead,
...patch, updatedAt: now})` — the patch overlaid on the pre-update read, NOT a
re-read, so an explicit `defaultImageId: null` survives; the P4.6c D4 finding).
Fixed a shared-op seam: `db::vault_character_update::update_character` now
NULLs a nullable slim column when the patch carries an explicit JSON `null`
(the `Option<String>` slim patch previously collapsed absent and null to
"skip", so it could never clear a column — v4's `_update` does; the avatar /
default-partner "clear" verbs need it). Added `tags::find_full_by_id` (the
marshaled Tag entity for the add-tag echo). Proven by
`characters_actions_equivalence` (11 cases: the seven verbs, the two
set-partner guard failures, avatar set + clear) vs v4's real handlers;
`characters_update_tier2` re-verified against the null-clearing change (no
regression).

P4.6f (Characters server, lane A) lands its first slice: the characters
**read** surface as dispatch variants. New `Request`/`Response` contract for
the whole characters + tags family (binding, shared with the P4.6g SPA lane);
a `character_enrichment` service (the list whitelist DTO + the detail
projection + the `enrichWithDefaultImage` wrapper, reproducing v4's `||`/`??`
coercions); and the read handlers `character_list` (npc/controlledBy filters,
createdAt-desc sort, N+1 partner-name + chat-count), `character_get`,
`character_default_partner`, `character_get_tags`, and the prompts / scenarios
/ wardrobe / plugin-data (map + item) sub-resource GETs. Added marshaled reads
`character_plugin_data::{find_by_character_id, get_plugin_data_map,
find_by_character_and_plugin}` (plugin `data` round-trips as its raw stored
string, not a parsed object) and `tags::find_details_by_ids` (omits
`visualStyle` when null). Committed the characters web fixture
(`build-characters-fixture.ts` + `characters.json` + `characters-{main,mount}.db`:
five characters exercising favorite/npc/controlledBy/canBeCarina/default-partner
/tags/prompts/scenarios/vault-avatar/legacy-avatar/wardrobe/plugin-data/
broken-vault branches). Proven by `characters_reads_equivalence` (13 cases vs
v4's real route handlers). The mutations, tags CRUD, actions, the heavier read
actions, the gallery, and ST import/export land in the following slices.
P4.4u3 built-in seeds: a fresh v5 instance now carries the two built-in
roleplay templates ("Standard" / "Quilltap RP") and the three built-in mount
stores ("Lantern Backgrounds" / "Quilltap Uploads" / "Quilltap General"),
matching a fresh v4 instance. The `roleplay_templates` `delimiters`
discriminated-union marshaling the Phase-2 port deferred is completed: typed
serde structs in schema field order for the three kinds (wrap / linePrefix /
tagPrefix), the `addOns` and string-or-pair sub-unions, and the read-side
`kind:'wrap'` backfill v4's `_update` applies on rewrite. The seeder
reproduces v4's two-path quirk exactly — the INSERT path stores delimiters in
Zod schema order, the drift-UPDATE path stores them in the raw seed-literal
order — proven byte-for-byte. Mount provisioning is the three v4 migrations as
one idempotent unit: settings-pointer provision-or-adopt (a live pointer
adopts its store, a dangling one re-provisions), the verbatim `doc_mount_points`
row, and the subfolder scaffolds. Both families run in fresh-instance
provisioning and on every host assemble/unlock (drift-update + adopt/heal),
tolerating a not-yet-provisioned db. New differentials drive v4's REAL
`seedBuiltInTemplates()` and the migration `run()` functions
(`builtin_templates_equivalence`, `builtin_mounts_equivalence`), and the
provisioning differential now diffs the seeded tables against a
fresh-v4-with-migrations+seed instance. The `lorian-and-riya.qtap`
sample-content import stays deferred.
P4.6g (Characters SPA, lane B) foundation + list lands (`apps/web` 0.4.0). The
`/characters` route goes live in the shell nav; `app.routes.ts` gains the four
lazy routes (list / new / :id / :id/edit). The core contract TS mirror
transcribes the p4.6f Shared contract — every character/tag `Request` variant
plus the list / detail / stats / tags / cascade-preview / physical-description
DTOs. A small pure `processTemplate` port substitutes `{{char}}`/`{{user}}` in
card previews. The Characters roster screen ships: cards over the
`characterList` dispatch with the v4 sort (NPCs last → favorites first → chat
count desc → name A–Z), the three inline toggles (favorite / Carina /
controlledBy) with optimistic updates, the Chat / Export / Delete actions, the
delete dialog with the `cascadeChats`/`cascadeImages` flags over
`cascade-preview`, and the SillyTavern import dialog (JSON via dispatch, PNG via
the multipart web route). "Summon From Lore", "Reset Built-ins", and the Groups
grid render disabled / omitted per the deferral list.

The detail / edit / create screens land, completing the P4.6g vertical
(`apps/web` 0.5.0). **Detail** (`/characters/:id`): the tabbed hall over
`qt-entity-tabs` (`?tab=` deep links) — a header (avatar / name / title /
pronouns / aliases / the `characterStats` line / the three optimistic toggles /
Start-Chat / Convert-to-NPC), Details (read-only render with `{{char}}`/`{{user}}`
highlighting + the template replace/reverse fan-out over `characterUpdate` +
per-prompt `characterPromptUpdate`), System Prompts (read), Tags (add/remove/
create over `characterAddTag`/`characterRemoveTag`/`tagCreate`), the Default
Settings autosave tab (per-control save-on-change, one `characterUpdate` /
`characterSetDefaultPartner` per field with the v4 payload shapes pinned by
tests), Photo Gallery (grid + `characterPhotoRemove`), Appearance (physical
description read + the depiction-guidelines editor), and the deferred Wardrobe /
Conversations / Memories bodies. **Create** (`/characters/new`): the plain
full-page form (name + the four DISTINCT vantage points with v4's helper copy
verbatim + a singular scenario + first message / example dialogues / system
prompt / avatar URL / default profile) → `characterCreate`. **Edit**
(`/characters/:id/edit`): the explicit-save form (ONE `characterUpdate` of the
whole Details bag, a `window.confirm` dirty guard), the inline scenarios array
editor, the tag chip editor, the System Prompts CRUD modals, the Appearance tab
(separate `physicalDescription` + depiction-guidelines saves), and an avatar
picker over the gallery (`characterAvatar`). The image-generation-profile picker
renders disabled (no P4.6d contract variant yet); the optimizer, AI wizards,
Rename/Replace, and the wardrobe dialog are named deferrals. The Playwright
`characters-flow.spec.ts` skeleton is written and skipped, to un-skip against
lane A's committed characters fixture at unification. `ng test` green (182),
prod build green.
Dogfood finding #3b is fixed: the Salon message list is virtualized, a port of
v4's own `@tanstack/react-virtual` + `useAutoScroll` architecture. The Angular
adapter `@tanstack/angular-virtual` (5.0.7, pinned) windows the existing
render-item array (estimate 150, overscan 5, dynamic measurement via a
`measureElement` directive, total-size spacer + translated absolute rows), so a
large chat renders only the viewport plus overscan instead of pushing every
message through the markdown pipeline at once. Markdown output is now memoized
per `(content, renderingPatterns, dialogueDetection)` so a row re-entering the
window re-mounts as a cache hit. A new `AutoScrollController` ports the
`useAutoScroll` state machine — initial settle + one-time instant
scroll-to-bottom, 100px stick-to-bottom tracking, completion-gated auto-scroll
(reads the `autoScrollOnResponseComplete` chat setting), scroll-on-user-send,
and the jump-to-bottom button — with unit tests over a fake scroll element. A
separate committed long-chat fixture (`salon-long-*.db`, ~300 mixed messages
via a new `build-long-chat-fixture.ts`, built through v4's real
`repos.chats.addMessages`) backs a new Playwright walk (`salon-scroll.spec.ts`):
the long chat opens interactive in under 3s, lands at the bottom, keeps only a
window of rows in the DOM, and the jump button round-trips. The virtualizer's
window is additionally driven from a plain effect (`_willUpdate`) so the list
also renders under the jsdom unit harness, where afterRender hooks do not fire.
SPA 0.4.0.

CLAUDE.md is trimmed from 5,922 lines (~430 KB, loaded into every turn of
every session and lane agent) to 287: the unit-by-unit Status journal moved
VERBATIM (diff-verified) to `docs/developer/porting/status-log.md`, and
CLAUDE.md keeps the standing rules plus a phase-level summary. New
convention: append unit/round records to the status log; update CLAUDE.md's
summary only at phase/round boundaries. The commit checklist (step 8),
`overview.md`'s status pointers, and the P4.6f order's ownership block are
retargeted accordingly.

The next parallel round is planned and its four work orders are written
(docs-only; drift check clean — v4 HEAD still `a7b1398d`; four fresh v4
surveys): **P4.6f** the Characters server surface (dispatch backfill over the
fully-ported characters repo layer — list DTO, detail + read actions,
create/update/cascade-delete, action verbs, the prompts/scenarios/plugin-data/
wardrobe sub-resources, tags CRUD incl. the delete fan-out, the photo gallery
service, ST import/export; the four LLM services deferred), **P4.6g** the
Characters SPA (list / view / edit / create screens over a pinned Shared
contract; the ~5k-line wardrobe dialog and the AI wizards deferred as their
own verticals), **P4.6h** Salon virtualization (dogfood finding #3b — a port
of v4's own tanstack-virtual + `useAutoScroll` architecture, a long-chat
fixture, and the scroll e2e beat), and **P4.4u3** the built-in seeds (the
Standard/Quilltap-RP roleplay templates closing the deferred `delimiters`
discriminated-union marshaling, plus the three built-in mount stores with
settings-pointer idempotent provision-or-adopt; the sample-content import
stays deferred). The round layout + ownership matrix is in `phase-4.md`.

Dogfood finding #3a is fixed: no Salon chat could scroll (an 80-message chat
reproduced it — masked on fixtures because their content fits the viewport and
the e2e never scrolls). The v5 shell had dropped v4 `app-layout.tsx`'s inner
`flex-1 min-h-0 overflow-y-auto` scroller wrapper around the routed content,
and two unstyled Angular component hosts (`qt-salon-conversation`,
`qt-message-list`) broke the flex/height chain React never has, so
`.qt-chat-messages`' own `overflow-y-auto` never received a bounded height.
Restored the wrapper + added `host:` classes to both components. The 10+ s
synchronous render on LARGE chats remains open as #3b (virtualization, the
next Salon order's first deliverable). SPA 0.3.2.

The Friday dogfood findings log is started
(`docs/developer/porting/dogfood-findings.md`): findings #1/#2 recorded as
fixed; finding #3 — a large chat renders 10+ s and lands stuck at the top (no
virtualization; scroll-to-bottom fires pre-layout) — is logged OPEN and
promotes virtualization to the top of the next Salon order.

The second Friday dogfood finding is fixed: the chat GET errored with
`no such column: timezone` — the INVERSE affinity class. v4 added
`chat_settings.timezone` to the schema with NO migration (nothing calls its
`generateAlterStatements` at runtime), and its `SELECT *` reads never notice a
missing column — but the port's explicit column list does. New
`db::tolerant_select_list` (PRAGMA table_info → present columns named
verbatim, missing ones substituted `NULL AS "col"`, so the positional
extraction is unchanged and a missing column reads as v4's absent key),
applied to `chat_settings::find_by_user_id`; `sidebarWidth`'s extraction also
went NULL-tolerant (`.default(256).optional()` — the OUTER optional means an
absent key stays absent). Regression test over a migration-vintage table;
`settings_routes_equivalence` regenerated + green (the fresh-shape echo is
unchanged).

The first Friday dogfood finding is fixed: the Salon list errored with
`Invalid column type Integer … isSilentMessage` against a real instance. Root
cause: a fresh `generateDDL` table declares `isSilentMessage` TEXT (the
row-schema union → numeric-TEXT `"1.0"` cells, the shape every fixture bakes),
but a real v4 instance got the column from the `add-silent-message-field`
migration — `ADD COLUMN "isSilentMessage" INTEGER` — so migrated cells are
stored INTEGER `1`/`0`, and the port's strictly-`String` read refused them
(v4's better-sqlite3 read is dynamically typed and coerces either through the
same union). `put_is_silent` now reads the RAW sql value and coerces
Integer/Real/Text uniformly, with regression tests over BOTH table shapes. A
migrations audit found no other fresh-vs-migration affinity divergence that a
strictly-typed read consumes (the numeric INTEGER-vs-REAL divergences are
harmless under `f64` reads).

The P4.6c ∥ P4.6d ∥ P4.6e round is unified on main. The three lane branches
cherry-picked with zero source-level conflicts (CHANGELOG/version unions only);
the two named unification wires are closed live: (1) the swipe-generate
engine-arm swap — `EngineAssembly`/`ReadyEngine` gained the P4.6c
`SwipeGenerateDriver` slot, the `MessageSwipe` generate branch now delegates to
the assembly's driver (`ChatSpine` implements it; the production factory wires
it), and (2) the P4.6d provider wire actions went LIVE — a new
`api::provider_actions` module holds the dyn-erased `ProviderActionsDriver` the
engine gates on plus the live seam impls composed in core over the
`SyncWireTransport` seam (the W4.7f `Real*Provider` precedent): the
per-provider `validateApiKey` matrix surveyed from v4 at `a7b1398d` (the
OpenAI-SDK family's models-list GET, OPENAI's `/v1/moderations` probe,
ANTHROPIC/GOOGLE's minimal-completion probes via the ported request builders,
OLLAMA's `/api/tags`, every wire failure → `false` never `Err`) and the live
models fetcher (the ported `models_list_request`/`parse_models_list` + the
transcribed anthropic static fallback list; the per-plugin model-METADATA
enrichment is a documented divergence — `modelsWithInfo` carries `{id}` rows
only, matching v4's metadata-less providers' net effect). The unification's
live Settings e2e surfaced a REAL port bug, fixed per the discipline: the
chat-settings PUT deserialized nested `cheapLLMSettings`/`themePreference` bags
into the strict storage structs, but v4's base-repo merge-then-validate runs
the FULL nested Zod schema — a partial bag (the wizard's exact
`{strategy: 'PROVIDER_CHEAPEST'}` save) gets its defaults MATERIALIZED and its
nullable-optional ids OMITTED. The PUT now applies the Zod-parse semantics
(`zod_cheap_llm_settings` / `zod_theme_preference`, schema field order, unknown
keys stripped), proven by two new corpus cases (`s_put_cheap_partial` /
`s_put_theme_partial`) in the regenerated 21-case `settings_routes_equivalence`
— byte-exact vs v4's REAL handler. Verified on the integrated tree: the full
workspace gate, a **twelve-differential fresh-oracle sweep** at v4 `a7b1398d`
(the four salon differentials, settings routes + wire actions, providers
listing, the 28-case orchestrator regen, the three adjacent tier-2s, and
`regenerate_swipe_tier3`), the SPA Vitest suite (139), and ALL FIVE Playwright
specs — including the newly-LIVE Settings first-run walk (fresh instance →
setup → the provider wizard → a validated OPENAI_COMPATIBLE profile against
the mock LLM → the profile in the Providers tab), un-skipped and green with
three spec corrections (v4's real hyphenated `OpenAI-Compatible` display name,
the no-key-input optional-key step, a strict-mode locator).

P4.6c (Salon consolidation) is ported and green against v4 `a7b1398d`. Server:
the skipUserTurn differential (`salon_skip_equivalence` — the minted-values skip
success + the all-others-skipped refusal; caught and fixed a turn-action
`participant.name` bug — a user-controlled skip must resolve to "Unknown" via the
active-LLM character map); swipe-generate through a new `SwipeGenerateDriver`
host seam (`api::salon::message_swipe_generate` + the production
`ChatSpine::generate_swipe` + `salon_swipe_generate_equivalence` vs v4's real
`handleGenerateSwipe`) — the engine-arm swap stays a unification wire; the
`pendingToolResults` orchestrator corpus case (the TOOL row pre-inserted before
the model turn, byte-exact); the full `processChatUpdates` `chat` bag via a raw
`UPDATE` (every `updateChatSchema` column + the roleplayTemplateId/projectId 404
gates; extended the chat-PUT differential); and the single-chat GET
attachment-resolution branch (linked `files` + image sha256 + link summary; the
salon fixture now links an image to a message). SPA: the skip-signal TS port + the
user-turn Skip banner, Speaking-As (`SpeakerSelector` + set-active-speaker +
`speakingAsParticipantId`), and pause/resume + nudge; component tests over the
mocked CoreClient. Deferred (named): the chat-settings GET default-injection
(needs `updateForUser`, P4.6d's file); the mount-file (Scriptorium) attachment
branch; the participant/conciergeState PUT families; the impersonate menu and
per-participant turn-queue UI. Flagged for a build_context follow-up: the salon
fixture surfaces a v4 identity-stack quirk (a literal `undefined` leaks into a
character's base system-prompt slot) the Rust port does not reproduce — orthogonal
to the swipe route, whose output byte-matches.

P4.6d (the Settings server surface) lands the Settings-vertical route
backfill as Core dispatch variants, each a differential port of v4's real
route handlers (`api/settings.rs`). Chat settings: the GET now
default-injects the seed row when none exists (closing the P4.6a deferral)
and a new PUT (`chatSettingsUpdate`) folds the ~27-field validation layer
into a ported `updateForUser` upsert (`db::chat_settings::update_for_user`
over the captured default seed). Connection profiles: list (the
`enrichWithApiKey` + `enrichWithTags` join, the `imageCapable` filter, the
sortIndex→localeCompare sort), create (name uniqueness, apiKey
provider-match, default-unset sweep, auto sortIndex, courier forced flags),
update (per-field validation + courier gating + name collision), delete,
reorder, reset-sort. API keys: list with the `maskApiKey` projection,
create (autoAssociate deferred → `associations: []`), update, delete. The
providers listing off the W4.7a manifest `Registry`; the models cached read
+ live fetch/cache. The wire actions (test-connection / test-message /
api-key test / models fetch) are ported over injected seams
(`ConnectionValidator` / `CompletionProvider` / `ModelsFetcher` — the
per-provider validate WIRE is a host plugin seam); the engine gates them
behind a not-assembled refusal until a host provider-actions driver is
wired (the swipe-generate precedent). DB additions: `provider_models`
net reads (`find_all` / `find_by_provider`), `connection_profiles`
`CpUpdate` null-clearing + `create_return_shape`,
`chat_settings::update_for_user`. Theme preference is stored in
`chat_settings.themePreference` (P4.6e persists via `chatSettingsUpdate`).
Verified: `providers_listing_equivalence` (tier-1 vs v4's real plugins),
`settings_routes_equivalence` (19 cases driving v4's REAL route handlers
for chat-settings / connection-profiles / api-keys / provider-models over
a baked fixture), the `settings_wire_actions` composition tests, and
`api::settings` unit tests.

P4.6e (Settings SPA vertical, tier-4): the first Settings slice in
`apps/web`. The Settings screen shell ports v4's seven-tab hall over new
`EntityTabs` + `CollapsibleCard` primitives (`?tab=`/`&section=` deep
links, a per-tab `data-subsystem` background); AI Providers + Appearance
are populated, the other five tabs render a v4-voiced "not yet fitted out"
placeholder. AI Providers: an API Keys card (masked `keyPreview` rows,
create modal filtered to key-requiring providers, per-key Test, delete
with confirm — export/import deferred), a Connection Profiles card (the
profile modal's Connect → Fetch Models → Test Message flow with the model
combobox + free-text fallback, the full flag set [default/cheap/uncensored/
tool-use + pseudo-tool mode/image-upload/web-search/model-class/max-context/
sampling], the Courier transport option, up/down reorder + Reset Sort,
inline duplicate-name validation; Auto-Configure slot disabled, tag editing
deferred), and a Cheap LLM card (PUT-merge of `cheapLLMSettings`). The
provider setup wizard (providers → api-keys → models → confirm; the
embedding/image steps render skippable and skip immediately) maps 1:1 onto
the pinned dispatch variants; settings-mode re-entry pre-populates from the
list variants. Basic Appearance: theme select over the bundled packs, color
mode, the nav quick-theme toggle, and avatar mode/style — the theme
preference now persists server-side via `chatSettingsUpdate
{themePreference}` (v4's `chat_settings.themePreference` store, surveyed and
pinned) and re-applies on boot, with localStorage as the offline fallback.
A fresh instance hands off to the wizard after setup (v4
`navigateAfterSetup`). The contract mirror grows the pinned Settings request
+ response variants; the SPA is built against a mocked `CoreClient` (live
wire-up at unification). New `ModelSelector` + `Modal` primitives. 96 Vitest
tests (tab deep links, masked-key rendering, duplicate-name validation, the
wizard reducer walk, PUT-merge, theme round-trip) + a clean SPA prod build;
a skipped live-flow Playwright spec + the mock-LLM `/models` endpoint. SPA
0.2.1 → 0.3.0. Contract note for P4.6d: the provider-test / api-key-test
response `type` strings are not pinned by name in the Shared contract (only
their `data` bodies are) — the SPA reads them defensively via a new
`CoreClient.dispatchData`, so the exact type names reconcile at unification.

The P4.6c ∥ Settings round is planned: three work orders written from
fresh v4 surveys at `a7b1398d` —
`docs/developer/porting/work-orders/p4.6c-salon-consolidation.md` (the
carried Salon follow-ups: the skipUserTurn differential case,
swipe-generate through a host-driver seam, the pendingToolResults
orchestrator corpus case, the full processChatUpdates field set, the two
deferred GET branches; SPA tier-2 controls — the skip-signal TS port +
Skip banner, Speaking-As, pause/resume, nudge),
`p4.6d-settings-server.md` (the Settings dispatch backfill: chat-settings
PUT + default-injection, connection profiles CRUD/enrichment/provider
actions [test-connection / test-message / models fetch+cache], API keys
CRUD + masking + test, the providers listing off the manifest registry —
each family differentially verified against v4's real route handlers),
and `p4.6e-settings-spa.md` (the Settings shell + AI Providers tab + the
setup wizard [settings mode] + basic Appearance with server-persisted
theme preference). Three-lane ownership: P4.6c owns `api/salon.rs` /
`chat_send.rs` / `spine.rs` / the orchestrator corpus + the Salon SPA
regions; P4.6d owns `api/types.rs` / `engine.rs` / a new `api/settings.rs`
+ the settings oracles; P4.6e owns the contract mirror / routes / shell /
settings screens. P4.6c's one engine-arm swap (the swipe-generate refusal
→ driver call) is a named unification wire. Deferred whole: the themes
service (`.qtap-theme` registry/bundle-loader — the largest genuinely-new
surface), embedding/image-profile route families, key export/import,
auto-associate/auto-configure.

P4.6 unification: the first Salon vertical is integrated on main —
**milestone M4 stands, run live.** The two lane branches (P4.6a Salon
server surface, P4.6b Salon SPA) cherry-picked cleanly (one CHANGELOG
union; ownership held exactly — zero source-level conflicts). Verified on
the integrated tree: the full workspace gate (1,174 tests / 0 failed;
clippy `-D warnings` default + `native-transport`; fmt), the two new Salon
differentials re-run green against freshly regenerated v4 oracles at
`a7b1398d` (`salon_reads_equivalence` 6 cases, `salon_mutations_equivalence`
11 cases), `orchestrator_tier3_equivalence` regenerated + green (the lane's
nudge/`pendingToolResults` threading is inert on the corpus), 76 Vitest
tests + a clean SPA prod build, and all three Playwright specs green —
including the previously-skipped **live M4 walk** (unlock → salon list →
open the baked Group Expedition history [staff chip renders] → send in
Solo Voyage → the streamed mock-LLM reply appears live and survives a
reload). Unification wiring (this pass): the e2e instance switched to the
committed Salon fixture, the user-id rewrite extended to the user-scoped
tables the send path reads, the mock-LLM `baseUrl` rewrite moved BEFORE
server launch (the CLI write-lock refuses a live holder — the spec's
original live rewrite could never work) with the mock on a fixed port, and
the M4 spec un-skipped + made unlock-state-tolerant (the shared server is
already unlocked after the foundation spec). SPA 0.2.0 → 0.2.1.
Follow-ups carried from the lanes: the turn `skipUserTurn` differential
case, swipe **generate** through dispatch, the `pendingToolResults`
orchestrator corpus case, the full `processChatUpdates` field set, and the
SPA tier-2 controls (Skip banner + skip-signal TS port, Speaking-As,
pause/resume).

P4.6b (the Salon SPA vertical) landed in `apps/web` (lane branch; unifies
with P4.6a). Introduced real Angular routing (`/salon` list + `/salon/:id`
conversation; the startup gate still owns the pre-operational states and the
shell hosts the outlet). The Salon list renders the enriched `listChats` DTO
as v4-faithful `ChatCard`s (participant avatar stack, message/memory counts,
danger flag, project chip, tags, `updatedAt`) with v4-verbatim microcopy. The
conversation screen reads `chatGet` + `chatSettings`, collapses swipe groups
to the highest-`swipeIndex` variant (client-side swipe switching), and renders
the message list via a render-item pipeline (message rows + packed Staff
announcement chips, whisper/silent labels, reasoning blocks, timestamps,
avatars). The markdown/roleplay/qtap-linkify renderer is a byte-for-byte TS
port of v4's server `renderMarkdownToHtml` (same pinned unified/remark/rehype
versions), verified against 23 fixtures captured from v4's real renderer. Send
+ live streaming ride the P4.5 stream reducer over the global SSE (optimistic
user bubble, live bubble through the same pipeline, status line, tool frames,
`done` → canonical refetch); tier-1 message actions (copy, inline edit, delete
+ the memory-cascade dialog, regenerate, swipe arrows) are wired. The composer
is the sanctioned textarea MVP (Enter-sends, Stop, Continue) — Lexical is a
locked deferral. Shipped a Node OPENAI-compatible mock LLM and the M4
Playwright spec (skipped-with-reason until the sibling lane's fixture + server
dispatch variants land). Verification: 76 Vitest tests (render parity,
swipe-group split, list/conversation components, reducer→bubble), the existing
foundation + setup Playwright specs re-run green against the real binary, and
the SPA prod build is clean. Tracked deferrals: the tier-2 controls (Skip
banner + skip-signal TS port, Speaking-As, pause/resume), the full
`ToolMessage` renderer, token badges, virtualization, `qtap://` navigation
targets, the sidebar/modals, and the new-chat (Green Room) entry point.
SPA 0.1.1 → 0.2.0.

P4.6a (Salon server surface) in progress — the read surface is landed and
differentially verified against v4 `a7b1398d`. Ported the chat-enrichment
service (`services::chat_enrichment`): the LIST orchestration
(`enrich_chats_for_list` / `enrich_chat_for_list` / `enrich_tags` /
`filter_chats_by_excluded_tags`, `_allTagIds` stripped via `#[serde(skip)]`;
the batched-list vault-only avatar quirk reproduced — a legacy-file avatar
resolves to `null` in the list, unlike the GET/create no-preloaded path) and
the DETAIL participant path (`enrich_participant_detail` /
`get_character_detail` with the avatar-override branch / `get_connection_profile`
/ `get_image_profile`). Added the read gaps `tags::find_by_ids` +
`conversation_chunks::count_stats_by_chat_id`. New `api::salon` dispatch handlers
+ contract variants: `chatSettings` (settings GET), the enriched `listChats`
(`excludeTagIds`/`limit`/`includeAutonomous`), `chatGet` (the full single-chat
projection minus the deliberately-omitted `renderedHtml`), plus the turn action,
message edit/delete/swipe-switch, chat PUT (Salon-minimal), and the three
impersonation verbs. Extended `chatSend` with the `sendMessageSchema` superRefine
rejection + `nudge` + `pendingToolResults` (pre-inserted as TOOL messages, the
RNG-auto-detect pattern). Committed the shared Salon web fixture
(`crates/quilltap-web/tests/fixtures/salon-*.db`) for the M4 e2e + differentials.
Verified: `salon_reads_equivalence` (settings + enriched list [3 param variants]
+ single-chat GET [solo + group]) and `salon_mutations_equivalence` (the three
impersonation verbs, the turn action [query + nudge], message edit / delete
[confirmation + swipe-group + memory-cascade] / swipe-switch, and the chat PUT
[isPaused + title]) — both byte-exact vs v4's real route handlers over the
committed fixture, zero-mint zero-normalization — plus the send-gate rejection
unit test. Remaining P4.6a follow-ups: the turn `skipUserTurn` branch (posts a
Host announcement — excluded from the zero-mint differential), the swipe
**generate** branch (needs the model driver), the `pendingToolResults`
orchestrator corpus case, and the full `processChatUpdates` field set /
roster / conciergeState families.

P4.6 round planned: the two work orders for the first Salon vertical (M4)
are written from fresh v4 surveys at `a7b1398d` —
`docs/developer/porting/work-orders/p4.6a-salon-server.md` (the dispatch
backfill: enriched chat list, the single-chat GET, the send pre-gate, the
turn/skip action, message edit/delete/swipe, chat PUT, impersonation verbs,
the chat-settings read, and the committed Salon web fixture — each handler
differentially verified against v4's real route handler) and
`p4.6b-salon-spa.md` (the Angular Salon: routing, list + conversation
screens, the client-side markdown/roleplay/qtap-linkify pipeline port, the
composer MVP + live streaming, and the M4 Playwright e2e over a Node mock
LLM). Two survey findings baked into the orders: v4 has NO `canSkipTurn`
server field (the client computes eligibility via the pure skip-signal
logic, already ported in Rust), and v4's server-side `renderedHtml`
markdown pre-render is a LOCKED divergence — v5 renders client-side in the
SPA with the identical unified/remark/rehype pipeline. The shared-contract
and ownership sections are binding and identical in both orders.

Participants-null-seam subtask unification: integrated on main (pure
fast-forward). Verified on the integrated tree: the full workspace gate
(1,171 tests / 0 failed; clippy `-D warnings` default + `native-transport`;
fmt) and six differentials re-run green against freshly regenerated v4
oracles at `a7b1398d` — `chats_tier2` (the new explicit-null corpus rows),
`chats_read`, `chats_participants_tier2`, `chats_messages_tier2`,
`identity_compiler`, and the chat-create capstone with the
`strip_participant_null_seam` normalizer removed (the persisted participant
nulls now diff byte-exact). Remaining chat-creation follow-ups: the
create-echo DTO shape and the capstone corpus extension.

Fix the `chats.participants` explicit-null marshaling seam. v4's
`buildCharacterParticipant` writes `connectionProfileId` / `imageProfileId` /
`selectedSystemPromptId` as `... || null` (always present, `null` when falsy)
and `.nullable().optional()` keeps the stored `null`, but the ported
`ChatParticipant` marshaled them as plain `Option<String>` and dropped the key
on re-serialization. Changed all three to the present-keeps-null double-`Option`
(the `removedAt` pattern); `roleplayTemplateId` stays single-`Option` (v4 never
writes it). Banked with an explicit-null participant row in the `chats-tier2`
corpus. Closes the marshaling half of the P4.4u2b unification follow-up: the
capstone's `strip_participant_null_seam` normalizer is dropped and the
participant nulls now diff byte-exact.

P4.4u2b unification: the chat-creation spine integrated on main (pure
fast-forward; one fmt fix folded into the lane's capstone commit).
services::chat_create composes the seven leaf sub-units into v4's
handleCreate + the autoGenerateFirstMessage ladder behind the
ChatCreateDriver seam; Request::ChatCreate/Response::ChatCreate land the
contract; the host ChatCreateSpine assembles it and the /api/events SSE
replays the Green-Room backlog to late subscribers. Verified: 1,171
workspace tests / 0 failed, clippy -D warnings both feature sets, fmt;
the capstone tier-3 differential green against a freshly regenerated v4
oracle at a7b1398d (6 cases × 6 sections incl. the byte-exact seed rows
and Green-Room frames). Tracked follow-ups: the participants
explicit-null marshaling seam + the create-echo DTO shape, and the
capstone corpus extension (continuation, outfit modes,
scenario-precedence paths, the greeting retry/reroute branches).

P4.4u2b work order: the handleCreate spine + ChatCreate dispatch (the
chat-creation capstone). Composes the seven landed leaf sub-units into
v4's POST /api/v1/chats pipeline behind a ChatCreateDriver host seam,
with two small new ports (enrichParticipantSummary + the
resolveCharacterAvatar URL half), one capstone tier-3 differential
driving v4's real handler (delivering the deferred outfit/continuation
composed diffs + the Green Room frame-trace diff), and a quilltap-web
integration test. Solo lane; P4.6 consumes the contract next round.

P4.4u2 unification: the seven chat-creation leaf sub-units integrated on
main (pure fast-forward, zero conflicts). Verified: 1,161 workspace tests
/ 0 failed, clippy -D warnings on both feature sets, fmt; the four gated
differentials re-run green against freshly regenerated v4 oracles at
a7b1398d. Remaining: sub-unit 8 (the handleCreate spine + ChatCreate
dispatch + capstone), the next order.

P4.4 unit-2 sub-unit 6: chat continuation (services::chat_continuation),
ported from v4's lib/chat/apply-chat-continuation.ts. applyChatContinuation
posts a Host continuation-from bubble in the new chat, replays the carryover
window (the most recent Librarian summary onward) with participant ids remapped
by shared characterId + old-chat-lifecycle fields stripped, replicates turn
state (isPaused / turnQueue / lastTurnParticipantId / activeTypingParticipantId
/ impersonatingParticipantIds / allLLMPauseTurnCount / spokenThisCycle) with the
same remap, and posts a Host continuation-to tail bubble in the source chat
last. Composes the verified Host continuation writers + the single-writer
message/update path over Db; mints message ids + createdAt per replayed row.
Errors are logged, not fatal. The pure leaves (participant-id map, librarian
anchor, message projection with the drop-unmapped-author / drop-all-targets-gone
/ hostEvent-remap rules) are unit-tested here; the composed applyChatContinuation
tier-2 diff (both chats' tables) rides the capstone driving v4's real handleCreate
(the continuation-create case).

P4.4 unit-2 sub-unit 5: the initial-greeting core
(services::initial_greeting::generate_greeting_message), ported from v4's
lib/chat/initial-greeting.ts generateGreetingMessage. Streams a short
in-character greeting over the streaming model boundary (v4 consumes
streamMessage + concatenates), accumulates content + usage, and returns
{content, contentFilterDetected} (empty content + burned completion tokens =>
a likely content filter). buildContextSection folds project + participant
memories + the recent-conversations block into the augmented prompt; logLLMCall
(a CHAT_MESSAGE row) is an optional injected config (the spine attaches it).
Verified by a DB-free tier-3 differential (initial_greeting_equivalence)
driving v4's REAL generateGreetingMessage with the streaming provider mocked +
logLLMCall no-op, recording the request messages (proving the augmented prompt
bytes) and diffing {content, contentFilterDetected} across success /
content-filter / empty-no-usage / whitespace-only / with-context cases. The
route ladder autoGenerateFirstMessage (participant/profile/key resolution + the
four-attempt retry matrix + the Concierge reroute) is the handleCreate spine's
(capstone-verified).

P4.4 unit-2 sub-unit 4: outfit selections (services::outfit_selections),
ported from v4's lib/wardrobe/apply-outfit-selections.ts + the chooseLLMOutfit
cheap-LLM task. applyOutfitSelections dispatches each character's
OutfitSelection (default / manual / none / previous_chat / llm_choose) to
set_equipped_outfit; resolveDefaultOutfit (default-marked items, oldest-first,
per-slot) and the chooseLLMOutfit prompt (byte-exact OUTFIT_SELECTION_PROMPT +
wardrobe listing) + its id/slot-validating response parser compose the
verified cheap-LLM executor + wardrobe reads. The 6bf88959 progress narration
(wardrobe-start / wardrobe-result OutfitPreviewSlots, log fallback) rides the
Green Room emitter. Documented seam: the ported executor's infallible parser
means a malformed-JSON response yields empty slots (vs v4's throw ->
default-fallback); the corpus keeps responses valid JSON and drives the
fallback via a provider failure. The pure leaves (default resolution, prompt
layout, parser) are unit-tested here; the composed applyOutfitSelections tier-3
diff (equippedOutfit + progress frames) rides the capstone driving v4's real
handleCreate.

P4.4 unit-2 sub-unit 3: the identity-stack compiler write side
(services::system_prompt_compiler), ported from v4's
lib/services/system-prompt-compiler/compiler.ts (compileAllIdentityStacks).
Precompiles each LLM-controlled CHARACTER participant's identity stack (the
verified build_identity_stack, with {{user}}/{{scenario}}/{{persona}}
resolved) and persists the {participantId -> stack} map to
chats.compiledIdentityStacks via a new ChatUpdate.compiled_identity_stacks
setter (nullable JSON object, no updatedAt bump — the compression_cache
pattern). Errors never propagate past the create handler (writeStacks
swallows its update error; a character-read error surfaces for the spine's
try/catch). The single-participant compile is a P4.6 deferral. Verified by a
tier-2 differential (identity_compiler_equivalence) driving v4's real
compileAllIdentityStacks over a baked chat (Aria/llm rich, Bob/llm, Sam/user,
Ghost/llm-removed + a scenarioText), diffing the persisted map byte-for-byte
(only the two active LLM participants get a stack; user/removed skipped;
physicalDescription surfaces), zero normalization.

P4.4 unit-2 sub-unit 2: buildChatContext (services::chat_initialize), ported
from v4's lib/chat/initialize.ts. Resolves the {systemPrompt, firstMessage,
character, userCharacter} seed bundle: the vault-overlaid responding
character, the optional user-controlled character (explicit id or the
character's defaultPartnerId, gated on controlledBy === 'user'), the
system-prompt selection (selectedSystemPromptId -> isDefault -> first ->
nothing), the scenario override, and the template pass. Ports initialize.ts's
OWN flat buildSystemPrompt (distinct from the per-turn identity-stack
builder) over the verified template processor + characters_read. Verified by
a read-differential (chat_context_init_equivalence) driving v4's real
buildChatContext over a baked three-character fixture (llm / user / llm with
defaultPartner) — bare / user+scenario / selected-non-default-prompt /
default-partner cases, comparing systemPrompt + firstMessage + resolved
character/user-character ids and names, zero normalization.

P4.4 unit-2 sub-unit 7: the Green Room creation-progress bus (D6), ported
from v4's lib/chat/creation-progress.ts. The kind-tagged frames
(status/log/wardrobe-start/wardrobe-result/done/error) are a new
api::EventPayload::CreationProgress variant scope-tagged by progress_id on
the one global /api/events stream; services::creation_progress adds the
core-adjacent replay buffer (CreationProgressBus: 200-frame cap,
replay-on-subscribe via active_snapshot, 60s TTL after the terminal done —
pruned lazily, no core timer) and the inert-without-progressId emitter
(fans each frame out to the bus + the live broadcast). v4's un-emitted
terminal error frame is faithful (fail() is ported but handleCreate never
calls it). Unit tests cover cap/replay/TTL + the v4 frame serialization
shape; the frame TRACE is diffed in the capstone. The transport
replay-on-subscribe wiring lands with the handleCreate spine.

P4.4 unit-2 sub-unit 1: the preset-scenario resolvers
(db::scenarios::resolve_{general,project,group}_scenario_body), ported
from v4's lib/mount-index/{scenarios-common,project,group,general}-scenarios
(the resolveScenarioBody read slice chat creation needs). Composes the
verified read_database_document + parse_frontmatter; the general resolver
reads the "Quilltap General" store pointer from main-DB instance_settings.
Verified by a read-differential (scenario_resolvers_equivalence) driving
v4's real resolveGeneralScenarioBody / resolveProjectScenarioBody over a
baked two-store fixture across the path matrix (bare / full / missing-.md /
leading-slash / missing-file / empty-body). The list / set-default write
surface is a P4.6 deferral.

P4.4 unit-2 work order: the chat creation flow + the Green Room (D6),
decomposed leaf-first from a fresh survey at a7b1398d (scenario
resolvers, buildChatContext, the identity-stack compiler write side,
outfit selections + chooseLLMOutfit, the greeting generator + its
content-filter fallback ladder, chat continuation, the creation-progress
event bus, and the handleCreate spine + ChatCreate dispatch variant),
each with its differential plus a capstone tier-3 driving v4's real
handler. A solo lane; P4.6 is sequenced after it. Docs only.

P4.4/P4.5 unification: both lanes integrated on main (zero source
conflicts; ownership held exactly). The shared dispatch contract
cross-checks byte-for-byte between the TS mirror and the Rust enums. The
deferred LIVE setup-wizard e2e is closed (apps/web/e2e/setup-flow.spec.ts):
empty data dir -> wizard -> real setup dispatch -> one-time pepper reveal
-> shell on the freshly provisioned encrypted instance. Verified on the
integrated tree: 1,136 workspace tests / 0 failed, clippy -D warnings on
both feature sets, fmt; the provisioning differential + both v4-side
cross-compat scripts green against v4 HEAD a7b1398d; 39 SPA unit tests +
2 Playwright e2e. SPA 0.1.1.

P4.4 unit 1: the unlock/pepper-vault service + fresh-instance
provisioning. The CORE now creates a brand-new, encrypted-from-byte-zero
instance at `Setup` time — no plaintext window (v4 creates its DBs
plaintext during pre-setup migrations, then encrypts in place; v5 keys
every partition on creation). New `services::provisioning`: replays the
captured generateDDL schema across all three partitions (main /
mount-index / llm-logs — the tier-2-fixture-proven, v4-compatible
surface, dumped from v4's real repositories by
`harness/oracle/provision/dump-fresh-schema.ts`) and seeds v4's
deterministic first-boot rows — the single user (`getOrCreateSingleUser`),
its default chat settings (raw INSERT of v4's captured row — the ported
`ChatSettings` nested structs serialize optionals as explicit `null`,
but `updateForUser` omits them, so byte-exact seeding replays the
capture), and the default `Built-in TF-IDF` embedding profile. New
`Request::{Setup, StorePepper, ChangePassphrase}` + `Response::{Setup,
Ack}` DTOs + `ErrorKind::Unauthorized` (401); the engine wires them
(setup provisions+assembles from `needs-setup`; store writes the
`.dbkey` from `needs-vault-storage`; change-passphrase re-wraps from
`resolved`, writing both `.dbkey` files for v4 parity). `dbkey` gained
`change_passphrase` (decrypt-with-old, re-wrap, no DB re-encryption).
The provisioning differential proves it: v5's `sqlite_master` (per
partition) equals v4's LIVE generateDDL schema byte-for-byte; the seed
rows match (minted id/timestamps normalized); and both cross-compat
directions hold — a v4-built instance opens under v5's ported reads, a
v5-provisioned instance opens under v4's REAL repositories
(`verify-v5-provisioned.ts`), and a v5 change-passphrase `.dbkey`
unlocks under v4 (`verify-dbkey-crosscompat.ts`). The web `/setup` flow
is proven end-to-end over real HTTP (empty dir → 423/needs-setup →
`setup` dispatch → unlocked engine on a real schema'd instance →
`listChats` = `[]`). Named deferrals: the sample-content seed import
(lorian-and-riya.qtap → the import service), the built-in roleplay
templates (need the `delimiters` discriminated-union marshaling on the
ported repo), and the three built-in mount stores (General / Uploads /
Lantern). Unit 2 (chat creation + Green Room) is the next P4.4 order.
(core 0.0.143, harness 0.0.134, web 0.0.3)

P4.5: the Angular SPA foundation (`apps/web`). Scaffolded Angular 21
(standalone + zoneless + signals, Tailwind v4, Vitest). Built the one
`CoreClient` transport seam (`dispatch` over `POST /api/dispatch`, the
single global `EventSource` on `/api/events` with resync-on-reconnect,
the `/health` readiness vocabulary) with hand-written TS contract types
mirroring the Rust enums, and layered TanStack Query for server state.
Ported the SSE stream reducer from v4's Salon hooks (content append,
reasoning replace, tool-batch splice at anchor offsets, turn/chain,
skip/empty/pending-external done, mid-stream error) as a pure fold with
a committed frame-trace fixture. Ported the `qt-*` CSS system + globals
file-per-file, the six bundled theme packs (with a `ThemeService` that
applies by id + injects fonts + persists to localStorage), and the base
UI primitives (icon, brand-name, loading/empty/error, form-actions,
section-header, avatar, chevron). Built the startup-gate -> unlock ->
setup-wizard (one-time pepper reveal) -> app-shell (nav skeleton, theme
switcher, chats list) screens with v4-verbatim copy. Verified: 39
component/unit tests plus a Playwright e2e against the real
`quilltap-web` (locked -> unlock -> shell + theme switch over a
passphrase-locked copy of the committed fixture). SPA at 0.1.0; no crate
changes. Documented divergences: the theme asset-URL rewrites and the
localStorage theme persistence (both reconcile when the server themes
service lands).

P4.4/P4.5 round kickoff: the two lane work orders. P4.4 round 1 (the
route-logic backfill: the unlock/pepper-vault service with
fresh-instance provisioning, then the chat creation flow + the Green
Room creation-progress events) and P4.5 (the Angular SPA foundation:
scaffold, CoreClient, the SSE stream reducer, the qt-* CSS + bundled
theme port, the UI primitives, and the startup-gate/unlock/setup
screens), with the binding shared dispatch contract and cross-lane
ownership matrix pinned identically in both. v4 baseline a7b1398d
re-verified (no drift). Docs only.

P4.d unification: both drift re-port lanes integrated on main. Zero
source-level conflicts (doc unions only; version deltas verified
identical). The two P4.d2 ownership workarounds folded: skipped/
skippedParticipantId moved onto ProcessMessageResult (TurnResult wrapper
deleted) and onto DonePayload as optional fields in v4's key position
(the DoneSkipped variant deleted; a byte-level unit test pins the skip
frame's serialized order). One straggler fixture DDL (host_cadence)
gained turnSkippingEnabled. Verified: full workspace gate (1,127 tests,
clippy -D warnings on default and native-transport, fmt) and a
thirteen-differential sweep against fresh v4 oracles at a7b1398d.
Oracle baseline advances to a7b1398d. Regen gotcha recorded: the
enclave-step oracle requires TZ=UTC in the invocation env.

P4.d1: answer-confirmation drift catch-up to v4 a7b1398d. Ported
buildRecentConversationContext (the compact recent-dialogue transcript —
Staff/tool/silent filtering, the 20-message cap, the 8,000-UTF-16-unit
tail-slice truncation, name attribution over the ported
getParticipantName with User/Character fallbacks), the rewritten
re-affirmation system prompt (optional "You are <name>. " anchor), the
labeled-sections re-affirmation user message (leading scene block; the
reference relabeled background knowledge), the new characterName /
conversationContext options, and the finalizer threading. Corpus
extended 14 -> 17 cases (a >20-message scene, an over-budget non-ASCII
truncation scene, a Staff-whispers/silent-only null-context case), with
the responder now resolvable in name attribution;
answer_confirmation_tier3_equivalence regenerated green against v4 HEAD;
message_finalizer_tier3_equivalence re-verified inert against a
regenerated HEAD oracle. Unit tests for the new pure leaves.
P4.d2: ported v4 b90cd1f5 ("nothing to add" turn-skipping for group
chats). New pure module skip_signal (sentinel detection with the
strip-and-keep-prose cleaned path, isTurnPassMessage,
findSkippedSinceLastSubstantive, isFirstCharacterTurn,
isRecentlyAddressed, qualifiesForTurnSkipping, computeSkipEligibility
with the withhold precedence + stall guard); the turn-state walk now
advances lastSpeakerId past Host turn-pass records; shouldChainNext
excludes Staff rows from the all-LLM pause counter and threads
selectionReason (queue vs algorithm) into chained turns; executeTurnChain
continues past skipped turns and stamps skipped on every chained
turnComplete frame; buildContext gained the turnSkip option + the
byte-exact Turn note (trailing section on a user message, its own
trailing user message on chained/continue turns); the orchestrator spine
computes eligibility per turn (nudge/queue-pop summoned withhold), runs
the sentinel handling (tools-ran-clears precedence), and handleTurnSkip
posts the Host turn-pass note, advances the persisted cycle (minted
updatedAt), and emits the hostAnnouncement + skipped done frames; the
Host writers gained the three byte-exact turn-pass builders +
postHostTurnPassAnnouncement; chats gained the turnSkippingEnabled
nullable-boolean marshaling (create/update/read). New tier-1
skip_signal_equivalence (99 rows); regenerated + extended turn_state
(turn-pass rows), turn_orchestrator_tier2 (Staff-in-pause-window +
selectionReason), chats_tier2 (toggle create/update/null round-trip),
chats_read (materialized toggle), post_office_host (3 builders),
post_office_writers_tier3 (llm + user turn-pass rows),
orchestrator_tier3 (27 calls — skip fire, sentinel+prose, nudge
withhold, turnSkippingEnabled:false), and enclave_step_tier3 (20 calls
incl. an autonomous pass that consumes a run turn); build_context_tier3
/ message_context_leaves / primary_stream_tier3 re-verified inert
against fresh v4-HEAD oracles. ProcessMessageResult's skipped fields
ride a TurnResult wrapper (the finalizer file is lane-frozen this
round); the skip done frame is a dedicated DoneSkipped event variant —
both fold into their v4 homes at unification. Out of scope per the work
order: the Salon Skip-button route, migration script, qtap-export
schema line, and UI.

P4.d drift re-port round kickoff: work orders for the two lanes
(p4.d1 answer-confirmation catch-up; p4.d2 turn-skipping port) with the
binding ownership matrix. Docs only.

Drift check against v4 2494a84b..a7b1398d (two commits: "nothing to add"
turn-skipping b90cd1f5; answer-confirmation conversation anchoring
a7b1398d). Both stale ported units. Verified empirically against fresh
v4-HEAD oracles: answer_confirmation_tier3, orchestrator_tier3, and
enclave_step_tier3 FAIL (the rewritten re-affirmation prompt; the
[NOTHING TO ADD] Turn note now injected into qualifying group-chat
prompts — 21 recorded stream keys per spine oracle carry it);
turn_state, turn_orchestrator_tier2, and chats_tier2 still pass (the
turn-pass lastSpeakerId branch, the Staff pause-counter exclusion, and
the new turnSkippingEnabled column are all corpus-inert). Refreshed the
docs/v4 mirror (CHANGELOG, DDL.md, nothing-to-add.md,
salon-answer-confirmation.md). A drift re-port round is required; the
scope is recorded in CLAUDE.md. Docs only — no crate source changed.

P4.2/P4.3 unification: both transport lane branches integrated on main.
Conflicts were the four expected mechanical files only (doc unions; host
Cargo.toml version-only on both sides, resolved 0.0.4; Cargo.lock
regenerated); zero source-level conflicts. Verified on the integrated
tree: full workspace gate (1,110 tests, clippy -D warnings on default and
native-transport, fmt), the 124-case CLI differential re-run live against
the v4 launcher, and the quilltap-web suites (M2 chat-send smoke,
dispatch/SSE contract, terminal WS, binary routes). Milestones M1 and M2
both stand. Follow-ups recorded in CLAUDE.md (bare-quilltap serve wiring,
CLI Tier B, HTTP-dispatch mode, the P4.2 named deferrals, the remaining
job-handler registrations — all P4.4+).

P4.2 (part 2): the production chat-send spine + quilltap-web, milestone M2.
New quilltap-host::spine — the ChatSendDriver composition point: ChatSpine
(generic over the embedding/completion/streaming/pricing model boundaries
only; every other seam is the REAL one, mirroring the tier-3 orchestrator
differential's construction — RealBuildContextSeams, RealAnswerConfirmation
under the host 25s+60s timeout ceiling, RealAsyncCompression, a
pricing-backed CostTracker, RealCarinaQuery, RealBrahmaConsole, the erased
ask_carina engine, DangerContentRouter over DbApiKeys, the Prospero writer
bridged on a dedicated thread, OsRandomBytes). Each dispatch runs
process_message + executeTurnChain on its own thread + current-thread
runtime (the U4.4 non-Send bridge) with frames riding the engine Event
broadcast; a turn error emits v4's transport-shell {error, errorType,
details} frame. Per-request inputs are pre-resolved (the same deterministic
participant->profile resolution, then getModelContextLimit + the registry
web-search capability); chat.timestampConfig || defaultTimestampConfig and
the chat_settings -> OrchestratorChatSettings projection are documented
NEW host-tier mappings (flagged for the P4.4/P4.5 verified readers), and
the provider->key scan (first active key per provider) is a documented
host seam. ProductionSpineFactory wires the ProviderIo drivers and
registers the model-dependent job handlers per assembly:
AUTONOMOUS_ROOM_TURN (the step-runner closure), MEMORY_HOUSEKEEPING (the
v4 handler body over ported pieces), CHAT_DANGER_CLASSIFICATION,
CARINA_MEMORY_EXTRACTION, CHARACTER_AVATAR_GENERATION, and
STORY_BACKGROUND_GENERATION (per-job construction so now_ms is the wall
clock). The host assembler also constructs a per-assembly TerminalManager
(published on the Host for the transport; cleared on Lock). Core enablers:
execute_completion gained the per-call profile baseUrl override (the
streaming composer's manifest-base swap), build_pricing_context is pub,
SelfInventoryEnv is Clone, files::find_by_storage_key added, and
paths.rs resolves /app/quilltap inside a container.

New crate quilltap-web — the axum HTTP transport (D1-D5): POST
/api/dispatch (Response-to-status mapping; the Locked 503 carries v4's
{error: "Setup required", setupUrl: "/setup", pepperState} body merged
alongside the typed envelope), GET /api/events (one global SSE stream,
v4's data:-frame encoding with incrementing id: fields + the ": keep-alive"
comment every 15 s; broadcast lag = the resync signal), GET /health (v4's
vocabulary collapsed to v5's phases: 200 healthy / 423 locked / 409
lock-conflict via the host lock classifier / 503 unhealthy), the D4 binary
GETs (files proxy by storage key, files by id + the cached WebP thumbnail
action with the v4 size clamp and canonical _thumbnails cache key, the
mount-point raw file read, the blob read with the documents fallback —
cache/sha/disposition/frame headers per the v4 routes), the D5 terminal
surface (spawn posts the session-opened Ariel announcement — the P4.1c
call-site handoff closed — list/get/kill/write/delete, and the WebSocket
marshalling terminal::protocol verbatim incl. the unknown-session
exit-then-close-1000 semantics), static SPA serving with the index
fallback + embedded placeholder pages (/ and /setup readable pre-P4.5),
and the bind policy (--host default 127.0.0.1, --port default 3000,
--data-dir/--instance/--spa-dir). Tests: the M2 chat-send e2e smoke
(always-on: a committed v4-baked test-pepper fixture instance, real HTTP
dispatch -> live SSE content/done frames -> the assistant row + chat
bumps asserted in the DB), the transport contract tests (statuses, the
Locked body, unlock round-trip, exact SSE frame bytes), the terminal
REST+WS integration over a real PTY, and the binary-route matrix. The
Dockerfile (multi-stage, BuildKit cache mounts over the pinned
amalgamation) builds and the container serves /health 423 needs-setup on
an empty volume.

P4.2 (part 1): the ChatSend boundary contract. quilltap-core::api gains the
Request::ChatSend variant (camelCase projection of v4 SendMessageOptions:
chatId/content/continueMode/respondingParticipantId/targetParticipantIds/
speakingAsParticipantId/fileIds), Response::ChatSend(ChatSendResultDto), the
transport-shell error frame (EventPayload::ChatError — v4 encodeErrorEvent's
{error, errorType, details}), and the dyn-compatible ChatSendDriver seam
(api::chat_send — boxed-future, the JobHandler precedent).
EngineAssembler::assemble now takes the engine's event broadcast and returns
an EngineAssembly (shutdown handle + optional chat driver); NoopAssembler and
the host assembler updated (driver still None — the production spine lands
next). The engine's ChatSend arm is readiness-gated in dispatch; a ready
engine without a driver answers the typed "chat dispatch not assembled"
internal error (read-only embedders stay valid).
P4.3 (the `quilltap` CLI, Tier R): new `quilltap-cli` crate — the native
`quilltap` binary covering the v4 launcher's direct-mode verbs, each shipped
verb byte-diffed against `node <v4>/packages/quilltap/bin/quilltap.js` on
shared fixtures (118 differential cases green: stdout + stderr + exit code).
Shipped: the subcommand router (locateSubcommand semantics, all 11 v4
subcommands recognized, unshipped ones exit loud), `db` legacy flags
(--tables/--count/raw SQL reader+writer/--json/--write/--llm-logs/
--mount-points) with V8's console.table reproduced byte-exactly, the
instance-lock commands (--lock-status/--lock-clean/--lock-override, ANSI
classification, last-10 history), `docs` read verbs (list/show/ls/dir/tree/
read incl. --rendered and qtap:// addressing over the ported codec, the
post-link-table schema guard), and `instances` CRUD (list/show/path/add/
remove/set-passphrase/default/rename + verifyPassphrase), plus the
`completion` emitters (bash/zsh/fish — v4's templates transcribed
byte-exact). The resolution
chain (--data-dir → --instance → default instance → QUILLTAP_DATA_DIR →
platform default), the default-instance stderr hint, and the loadDbKey
passphrase chain (flag → env → hidden TTY prompt, Ctrl-C exit 130) are
ported over quilltap-core::dbkey. quilltap-host additions: the write-lock
(acquire_write_lock/release_write_lock — refuse on live holder, no
override), the Suspect PID-identity probe (verify_pid_is_quilltap +
classify_lock_status_probed), and the instance-registry write verbs
(upsert/remove/set-passphrase/default/rename/verify_passphrase, atomic
0600 tmp+rename writes). Help texts are byte transcriptions of the v4
launcher's output. Documented divergences: interactive-TTY table colors
not reproduced (non-TTY output is the diffed form); the Node
readline pipe-buffer discard on multi-prompt stdin scripting is not
reproduced (v5 reads line-per-prompt); elapsed-seconds heartbeat displays
normalized in the diff. Deferred per the work order: db high-level verbs
(schema/find/chats/...), docs files/status/find/grep, memories/logs
(Tier B); every server-required verb + HTTP-dispatch mode
(P4.4); themes/migrations/maintenance/file-verify; db --repl.

P4.2/P4.3 round kickoff: drift check clean (v4 HEAD still 2494a84b) and the
two lane work orders written (docs/developer/porting/work-orders/
p4.2-quilltap-web.md and p4.3-quilltap-cli.md), each with the binding
crate/file ownership matrix for the round. P4.2 owns quilltap-web, the
core api surface, and the quilltap-host spine/terminal/providers regions
(the production ChatSend spine composition + model-dependent job-handler
registrations; exit = the headless chat-send e2e smoke, milestone M2).
P4.3 owns quilltap-cli plus host lock.rs/instances.rs (the direct-mode
verb set diffed against v4's launcher, the write-lock + Suspect probe,
and the instance-registry write verbs; exit = db --tables / docs ls
byte-diffed vs the v4 launcher, milestone M1). Docs-only commit.

P4.1 unification: the four host-driver lane branches (P4.1a provider IO,
P4.1b files/images, P4.1c PTY/terminal, P4.1d environment/cadence) are
integrated on main. All conflicts were mechanical unions (host lib.rs mod
decls, host Cargo.toml dependency additions, the append-only
terminal_sessions.rs c+d functions, doc blocks); no cross-lane type drift
and no duplicate image-seam port (lane b's HostImageCodec implements the
core seams lane a's ProviderIo constructs against). Full workspace gate
green (tests, clippy -D warnings on default and native-transport, fmt);
twelve differentials re-verified against freshly regenerated v4 oracles at
2494a84b. Follow-ups recorded, not implemented: lane b's four handoffs
(keep_image connection-scoped ingest, ProjectImageUpload widened to Result,
maintenance-sweep byte-delete via delete_file_completely, the harness→host
dev-dep note), lane d's flat SelfInventoryEnv registry-default seam, and
the P4.2 handoffs (spine composition + ChatSend, terminal WS route
marshalling, thumbnail routes, startup-conflict 503).

P4.1a (host drivers, provider IO): the production streaming composer + the
reqwest wire + the live pricing fetch + the API-path embedding provider.
New `quilltap-core::model::streaming_provider` — the production
`StreamingCompletionProvider` composing the frozen sans-IO surfaces
(request builder with `stream: true` -> transport -> the manifest-selected
W4.7b decoder -> the normalized `StreamChunk` channel), the
`ChatCompletionsFlavor` split applied internally (DEEPSEEK/Z_AI/OPENROUTER),
google's decoder over the ported `isThinkingModel` predicate, the pump on a
plain OS thread (the core stays scheduler-free), an injected provider->key
source (the failover path re-calls with a different provider), and the
documented OpenRouter divergence (the raw chat-completions wire ALWAYS; the
SDK's no-tools OpenResponses protocol is not ported). Verified by a new
"free" differential (`streaming_composer_equivalence`) replaying all 21
committed W4.7b wire fixtures through the full compose path at whole-buffer
+ byte-at-a-time (ollama line-aligned per the ported no-buffer bug) against
the recorded v4 chunk sequences, plus 8 composer unit tests (auth per
manifest scheme, decoder selection for all nine providers, mid-stream and
pre-stream errors, EOF finish-once). `apply_auth` hoisted into the shared
`model::provider_auth` (completion + streaming paths cannot drift). New
`quilltap-host` modules: `wire` (reqwest `WireTransport` + the blocking
`SyncWireTransport` on a dedicated thread — a blocking client never runs on
a runtime thread) and `providers` (the `ProviderIo` constructor bundle +
`LivePricingFetch` — the three pricing HTTP calls with v4's 3 s fail-fast
timeout; loopback-smoke tested). The spine's `build_pricing_context` now
populates the connection-profile api keys (v4 `getApiKeyForProvider` via
`findApiKeyByIdAndUserId`), proven inert under the canned pricing seam by a
freshly regenerated `orchestrator_tier3_equivalence`.
The API-path embedding provider
(`quilltap-core::services::embedding_provider::ApiEmbeddingProvider`) ports
v4 `generateEmbeddingForUser` whole over the `WireTransport` seam: profile
resolution (explicit -> default via the new `embedding_profiles::
find_default`), the BUILTIN dispatch, the registry gate, the requiresApiKey
gate over `api_keys`, the openai/ollama/openrouter wire dialects over the
frozen embedding_wire builders (ollama num_ctx derivation + derived-only
cache + 404 legacy fallback; openrouter via the recorded SDK wire), and
`apply_embedding_profile`. New v4 fact banked: v4 `generateEmbedding`'s
error wrap is dead code (the async calls are returned without `await`), so
raw plugin errors escape unwrapped -- ported faithfully. Verified by a new
jest-real-DB differential (`embedding_provider_tier3_equivalence`, 12 cases
over a baked 9-profile fixture with a v4-fitted BUILTIN vocabulary; the
Rust side replays a CannedWireTransport registered from the oracle-recorded
wire, so a request-building divergence is a loud canned miss).

P4.1c: the PTY / terminal host driver. New `quilltap-host::terminal` — the
session manager over `portable-pty` (replacing node-pty): spawn with v4's
shell/cwd/size/env defaults (`QUILLTAP_DATA_DIR` set authoritatively last;
directories are constructor params), the 256 KB UTF-16 ring buffer, the raw
transcript stream under `logs/terminals/`, the `terminal_sessions` row at
spawn + the exit-stamp update, per-subscriber broadcast with the attach
replay (ring buffer as one `output` frame, then `meta`), kill (SIGTERM) /
write / resize / kick-for-chat, the exit sequence in v4's order, and the
Ariel flush drivers (30 s idle / 120 s max-age tokio timers, host-side).
The verbatim WS protocol types (`terminal::protocol`, round-tripped against
literal v4 JSON) land here so P4.2's WebSocket route only marshals. The
production `TerminalScrollbackSource` (`terminal::scrollback`) resolves the
live ring buffer vs the 1 MB transcript tail exactly as v4's terminal-read
handler. New core `services::ariel_notifications` — the three Ariel
announcement writers (session-opened / terminal-output with the fence-length
and 16 KB elide rules / session-closed) plus the session reconcile pass
(live-probe injected; explicit-NULL exitCode via the appended
`terminal_sessions::mark_session_exited`). Verified by a new tier-3
differential (`ariel_writers_tier3_equivalence` — 18 cases driving v4's REAL
writers + reconcile over a v4-baked fixture, diffing per-case results plus
`chat_messages`/`chats`/`terminal_sessions` byte-for-byte), 10 real-PTY host
integration tests, a fixture-driven end-to-end flush test (real PTY → idle
flush → posted row + `chat-update` broadcast), and re-verified
`terminal_sessions_tier2` / `terminal_tools` differentials. Deferred: the
shell-init alias/completions bootstrap (targets the Node launcher; needs the
P4.3 `quilltap` binary), the WS route (P4.2), xterm.js (P4.6).

P4.1d: the environment/cadence host-driver lane. The single-instance lock
(`quilltap-host::lock` — v4 `instance-lock.ts`: PID-in-file with hostname
disambiguation, atomic O_CREAT|O_EXCL create, re-entrant same-PID refresh,
dead-PID stale claim, the different-host heartbeat-freshness rule, the
capped history log, v4's exact file format so v4/v5 locks interoperate, and
the launcher-compatible absent/corrupt/active/stale status classification
for the P4.3 CLI verbs) is acquired at assembly, heartbeated every 60 s, and
released on shutdown; a live conflict is a typed boot error, and a LOST lock
stops the drivers then runs a configurable handler (default: exit 1, v4's
shutdown). The four scheduler sweeps now run as stop-aware host loops (v4
instrumentation order): LLM-log cleanup (immediate + 24 h), memory
housekeeping (5-min grace + the 20 h recent-COMPLETED-scheduled-job
short-circuit + 24 h), daily maintenance (grace + the `lastMaintenanceSweepAt`
20 h window + 24 h), and the danger-scan enqueuer (the all-users-OFF start
gate + immediate + 10 min). New core services: `scheduled_maintenance` (v4
`runScheduledMaintenance` — the four independently-isolated sweeps + the
end-of-pass stamp; the transcript-file unlink behind a `TranscriptStore`
host seam) and `danger_scan` (v4 `runScheduledDangerScan` — the per-chat
exempt/off-duty/sticky/grown gates, the controlledBy-filtered
participant-profile-first-then-fallback resolution, the summary / >50 / <=50
enqueue tree at priority -2). Ported the two missing repo ops the
maintenance pass needs: `doc_mount_file_links::sweep_orphaned_files` and the
terminal-session reaper read (`find_closed_before` + the
`cleanup_closed_sessions` orchestration). `queue_service` gained
`enqueue_context_summary` (plain enqueue, no dedupe) and
`enqueue_chat_danger_classification_with_priority` (the -2 passthrough).
`quilltap-host::env` adds the production `SelfInventoryEnv` (runtime-mode /
docker / lima probes, the release-notes semver scan + changelog read, the
mount-index degraded derivation, the flattened legacy fallback-pricing rows
— the flat-env DEEPSEEK/Z_AI registry-default gap is a documented seam).
Verified by two new differentials, both green against v4 at `2494a84b`: the
danger scan (a 10-chat / 3-user gate-matrix fixture, minted-values
`background_jobs` diff + the pre-check + result counts) and the whole
maintenance pass (driving v4's REAL `runScheduledMaintenance` over a two-DB
fixture — proving both new repo ops inside the real orchestration, the
per-status job windows, the never-reap-FAILED/live-session rules, both
transcript path forms + the ENOENT rule, and the stamp); the adjacent
`terminal_sessions` / `background_jobs` / `maintenance_sweep` tier-2
differentials re-verified green; plus lock unit tests, host cadence
integration tests (conflict boot error, loss handler, the 20 h window across
a re-boot, the danger gate + live enqueue), and core service self-tests.

P4.1b: the file/image host-driver lane — the byte layer is real. New core
`services::file_storage` ports v4's file-storage manager + bridges over two
injected seams: the pure key/path logic (`safeFilename`, storage keys,
thumbnail keys, the `mount-blob:` codec), the WebP POLICIES (`convertToWebP`
quality 90 / `transcodeToWebP` quality 85 with their mime/extension rewrites
and failure-passthrough shapes) over a low-level `PixelCodec` pixel seam, the
manager ops (`downloadFile`/`deleteFile`/`fileExists`/`uploadRaw`/`deleteRaw`/
`getFileUrl` — mount-blob keys resolve through the ported `doc_mount_blobs`,
disk keys through a `StorageBackend` seam), the `storeMountFile` database
blob branch, the user-uploads + project-store bridges, the images-v2 ingest
engine (`createFile`/`ingestImageBuffer` — auto-WebP, sha dedup with the
storage-existence recheck and orphaned-metadata cleanup, tag inheritance),
and `deleteFileCompletely`. The two `FileBytesStore` seams get a production
`ProductionFileBytes` (chat-files download + photos read/ingest; the ingest
carries a loud writer-thread guard — the keep_image in-closure fallback needs
a connection-scoped store, a tracked executor handoff), and
`ProjectImageUpload` gets `RealProjectImageUpload` (the frozen seam is
infallible while v4 throws — an upload failure returns an `fs-seam:error:`
sentinel key, flagged for a Result-widening pass). New core
`services::help_doc_sync` ports v4's `syncHelpDocs`/`ensureHelpDocsSynced`
(the local frontmatter/url/title extraction quirks, hash-skip, upsert +
embedding clear) over a host-walked file list. New `quilltap-host` modules:
`image_codec` (the `image` + `webp` crates — libwebp bindings for lossy WebP
encode per D19, with documented degradations: animated GIF→WebP goes
first-frame, AVIF/HEIC decode unavailable takes v4's own failure-passthrough
branch), implementing BOTH core `ImageTranscoder` seams + `PixelCodec` + the
thumbnail op; `files_store` (the local disk backend: tilde expansion, the
buildSafePath traversal guard, ENOENT-tolerant delete + legacy sidecar
unlink, the transient-error fs retry; plus the help-doc walker); `apply_fs`
(the four `ApplyHost` fs operations — inventory completion, no production
consumer until a batch-mode job returns). `instance_settings` gained
`get_user_uploads_mount_point_id` (append-only). Two new differentials, both
green against v4 at `2494a84b`: `help_doc_sync_equivalence` (drives v4's REAL
`syncHelpDocs` over a committed fixture help tree + a pre-seeded DB — banks
created/updated/unchanged/skipped-empty, the CRLF + unclosed/EOF-fence
frontmatter quirks, the embedding clear on change and the untouched-row
sentinel proof) and `image_ingest_tier2_equivalence` (drives v4's REAL
`ingestImageBuffer` under jest with sharp mocked to a passthrough mirrored by
`PassthroughPixelCodec` — banks fresh ingest, the dedup linkedTo merge and
no-op, the orphaned-metadata recheck re-ingest, webp/svg passthroughs, and
the gif convert; six-table cross-DB dump in the shared-UUID-remap form with
the mount aggregates pinned per the refreshStats precedent).

P4.1 kickoff: round drift check (v4 HEAD unchanged at the `2494a84b`
baseline — no ported unit stale) and the four host-driver lane work orders
written per the phase-4 decomposition (`docs/developer/porting/work-orders/
p4.1{a,b,c,d}-*.md`): (a) provider IO — the streaming composer, reqwest
wire transports, live pricing fetch, the API-path embedding provider; (b)
files/images — the FSM byte layer, the image codec over the sharp operation
inventory, help-doc sync, the ingest differential; (c) PTY/terminal — the
portable-pty session manager, the verbatim WS protocol types, the Ariel
announcement writer; (d) environment/cadence — the instance lock, the four
scheduler sweeps (porting the danger-scan enqueuer body with its
differential), the production SelfInventoryEnv. Includes a fresh v4 survey
of the FSM/terminal/lock/scheduler surfaces baked into the orders.

P4.0: the Core API boundary + the composition root (milestone M0). New
`quilltap-core::api` module — the `Request`/`Response`/`Event` contract
types (scope-tagged event envelope over the existing chat-frame vocabulary),
the `QuilltapCore` trait (dispatch + subscribe), the pepper-provisioning
state machine (the control-flow port of v4 `provisionDbKey`: env pepper /
`.dbkey` / hash-mismatch-fatal resolution to resolved / needs-setup /
needs-passphrase / needs-vault-storage), and the engine-backed `CoreEngine`
with the first variants: health, unlock-state/unlock/lock, list-instances,
list-chats. The readiness gate is enforced in dispatch (ready-gated variants
answer a locked error until the pepper resolves); `Lock` tears the assembled
drivers down through the new `EngineAssembler`/`EngineShutdown` seams and
returns to needs-passphrase. `dbkey` gained the write path (`save_dbkey` /
`generate_pepper` / `hash_pepper` / `read_pepper_hash` — PBKDF2-SHA256
600k, AES-256-GCM, v4's exact JSON field order and 0600 mode), round-trip
verified against the Friday-verified reader. New `quilltap-host` crate (the
composition root): instance-registry read path (the launcher's
`instances.json` incl. the POSIX permission refusal), base-dir/platform
path resolution, and the cadence drivers the core deliberately does not own
— the job-runner pump loop (enqueue wake via a fan-out over the process-
global wake hook, next-due wake delay, 2 s poll), the 5-minute stuck-job
reset, and the 60 s autonomous schedule tick (v4
`scheduled-autonomous-rooms.ts`), with the seam-free handler set registered
(schedule tick / wardrobe outfit announcement / embedding refit; everything
else stays on the loud fallback until its P4.1 lane). Integration tests
boot a fixture instance headless, pump enqueued jobs to completion, and
prove the lock → unlock → drivers-restart cycle against a
passphrase-protected `.dbkey` fixture. The `Setup` variant is deliberately
deferred to P4.4 (fresh instances also need schema creation); the full
unlock/pepper-vault service differential remains P4.4 per the work order
(docs/developer/porting/work-orders/p4.0-boundary-composition-root.md).
Drift check at round start: v4 HEAD still `2494a84b`.

Phase-4 kickoff planned (docs only). New docs/developer/porting/phase-4.md
locks 22 decisions for the transports + host-drivers + Angular-SPA phase,
built from three fresh surveys (the v5 host-seam/deferral inventory, the v4
API surface — 124 routes, ~162 action verbs, one terminal WebSocket, 9
binary asset routes, and a confirmed-vestigial auth layer — and the v4 UI
surface — ~24 screens, ~535 components, the 11k-line qt-* theme CSS).
Headline decisions: the axum HTTP transport is a first-class deployment
(Docker-Desktop-style local web use) with no authentication (localhost
trust; bind-address policy; the pepper-unlock readiness gate survives as a
non-auth concept); the browser and the Tauri webview are co-equal hosts of
one Angular SPA behind a single CoreClient seam; the dispatch surface is
POST /api/dispatch + one scope-tagged SSE event stream + enumerated binary
GET routes + the terminal WS (not a reproduction of v4's REST tree); crate
layout quilltap-core::api + quilltap-host + quilltap-web + quilltap-cli
(dual-mode) + quilltap-tauri + apps/web; tier-4 verification (transport
contract tests, headless HTTP e2e, CLI diffs vs npx quilltap, Playwright);
decomposition P4.0-P4.7 with milestones M0-M6. Includes the route-logic
backfill list (chat creation, wizards, help-chat orchestrator,
backup/restore, import/export, unlock/pepper-vault, the markdown renderer +
qtap-linkify, Document Mode ops, the Brahma streaming console) and the full
host-seam closure inventory. overview.md roadmap/status and CLAUDE.md
updated to match; Phase 3 marked complete in the roadmap. Kickoff-day drift
check: v4 6bf88959..2494a84b (1 commit, copy-conversation-UUID buttons +
Salon header link) audited — pure React UI + docs, the only lib/ touch a
test-mock type cast; no ported unit stale; docs/v4 CHANGELOG mirror
refreshed; new oracle baseline 2494a84b.

U4.4 (enclave engine, the capstone) — PHASE 3 IS COMPLETE. enclave::step
ports v4's handleAutonomousRoomTurn as the persisted one-transition step()
(guard chain incl. the concurrent-sibling (createdAt, id) tie-break,
idle-to-running fallback + banner, pre/post-turn budget gates with the
grace-turn flow, speaker selection, process_message with the autonomous
options and the run LogContext, monotonic token/turn accounting off the
local snapshot, pacing milestones, the awaited summary fold outside the
run scope, re-enqueue) plus schedule_tick (slot seed / stale-advance /
fresh start / wedge heal). Writes go DIRECTLY through the single-writer Db
(the enclave doc's write_apply routing superseded — the v4 oracle side
runs unforked, so the differential pins in-process direct-write
semantics; write_apply keeps its own re-verified proof). New llm_logs
usage reads (get_total_token_usage_for_run / _since) — the latter ports
v4's $ne:null translator bug byte-for-byte: on SQLite the daily-spend sum
is ALWAYS 0, so the autonomous daily-token-budget gates never bind
(empirically probed, banked in the corpus). Two more dead-code findings
pinned: turn_error: is unreachable (v4's stream shell swallows every
mid-turn error — a failed turn counts and re-enqueues, banked), and
suppressAutomaticImages has no consumer in v4. The LogContext threading
gap is closed (log_chat_message_call parameterized, default none —
primary_stream/orchestrator tier-3s regenerated inert), and the
autonomous_context_cap context-manager clamp — never plumbed in v5 — is
wired (shrink-only clamp on the context budget; build_context tier-3
re-verified). Job-runner dispatch rows for AUTONOMOUS_ROOM_TURN /
_SCHEDULE_TICK are live (the turn handler bridges step's non-Send future
on a dedicated thread) with the dispatcher-level failed-turn reconcile
hook and two runner end-to-end tests. Verified by
enclave_step_tier3_equivalence: 19 calls / 20 chats across all three DBs,
driving v4's real handlers with only the model boundaries mocked; diffs
chats + chat_messages (Host announcements byte-exact) + background_jobs +
llm_logs (run-tagged turn/distill rows vs untagged fold rows). Full
workspace gate green (705 core tests, clippy -D warnings on default and
native-transport, fmt). Versions: core 0.0.137, harness 0.0.131.

U4.1–U4.3 (enclave engine, the parallel phase): the first three sub-units of
the autonomous-room ("enclave") engine, each with its differential green
against v4 HEAD `6bf88959`. New module family `quilltap-core::enclave`.
U4.1 (`enclave::milestones`): the pacing-milestone bitmask/threshold logic
(halfway/near-end/grace bits; near-end sets both bits so a vaulted halfway
never fires late) + the Host-voiced milestone and grace message bodies,
extracted mechanically from the v4 source by a checked-in generator that
evaluates v4's own template literals under V8 (byte-exact composition proof
completes in U4.4's tier-3); the existing Phase-1 `enclave_budget`
differential regenerated — zero drift (42 rows).
U4.2 (`enclave::cron`): croner-10.0.1-semantics next-occurrence computation,
HAND-ROLLED (the Rust croner crate was rejected: v4 passes no timezone
option, so croner-JS runs on plain V8 local-Date semantics, not its own
fromTZ path); jiff's Compatible disambiguation proven identical to ES
LocalTZA; `next_occurrence` + the throw-vs-null `try_next_occurrence` split
(updateSettings rejects on the constructor throw). Tier-1 differential over
124 committed rows × 2 timezones (America/Chicago DST + Asia/Kolkata),
driving v4's real installed croner; a probe row pins croner's version. No
new dependency.
U4.3 (`enclave::announce` + `enclave::lifecycle`): the run-start row
contract + Host-authored announcement writers (banner caps/name-list
byte-exact), and the full lifecycle service — begin/start-scheduled/
start-manual (cron-slot consumption), pause/resume (pause-interval
accumulation)/stop (runId bump), update-settings (invalid cron rejects the
whole edit), startup + failed-turn reconciliation, with every
runStateMessage string verbatim. `ChatUpdate` gained 21 autonomous setters
(no `updatedAt` mint); `queue_service` gained the AUTONOMOUS_ROOM_TURN /
_SCHEDULE_TICK enqueues (maxAttempts 1; turn enqueue dedupe-free, tick
PENDING-deduped). Tier-2 real-DB differential over a 38-op lifecycle matrix
(18 chats, 7 jobs, 6 banners diffed byte-for-byte); the integration pass
closed the cron seam so the differential now proves the lifecycle∘cron
composition. The chats tier-2/read differentials re-verified green; the
en-US toLocaleString grouper deduped (primary_stream's is now pub(crate)).
Spec doc corrected: the startup-reconcile stamp is a nullish-coalesce chain
(lastMessageAt ?? runStartedAt ?? now), not a max; the runStateMessage
vocabulary gains turn_error:/no_eligible_speaker:.

Drift check against v4 `6b6e39ad..6bf88959` (1 commit): no ported unit is
stale. `6bf88959` ("The Green Room" new-conversation status dialog) touches
only unported surfaces — the new `lib/chat/creation-progress.ts` in-memory
progress bus + SSE route (a Phase-4 host/transport concern; in v5 these
events ride the boundary's `Event` channel) and the chat-creation-flow
`applyOutfitSelections`, which gained optional progress narration (the ported
functions it composes — `resolveEquippedOutfitForCharacter`,
`chooseLLMOutfit`, `chats.setEquippedOutfit` — are unchanged at this commit).
Refreshed the `docs/v4/` mirror (CHANGELOG, API.md). New oracle baseline:
`6bf88959`.

Cleanup-round unification: integrated the three parallel lanes (W4.11a spine
logging + owned-provider plumbing, W4.11b primary-stream logging regen,
W4.11c moderation logging seam) onto main — zero source-level conflicts for
the third consecutive round (docs unions only; every branch's Cargo.toml
delta verified version-only before take-theirs). Verified on the integrated
tree: the full workspace gate (903 tests, clippy -D warnings on default and
native-transport, fmt) and a thirteen-differential sweep against freshly
regenerated v4 oracles at 6b6e39ad (the three lane proofs plus ten
cross-checks), all green. Versions: core 0.0.135, harness 0.0.129. Every
pre-enclave follow-up is now closed or precisely narrowed; Round 5 (the
enclave) is ready to start.

W4.11c: closed the last `logLLMCall` seam — the gatekeeper moderation-path
`llm_logs` row. The moderation seam was widened so the wire's raw per-category
`flagged` survives the projection to the gatekeeper (added `flagged` to
`ModerationCategoryScore`, matching v4's `ModerationCategoryResult`;
`map_moderation_result` still never reads it — faithful), and the
`ModerationOutcome::Moderated` branch now writes v4's `modelName:'moderation'`
`DANGER_CLASSIFICATION` row: provider = the wire provider name, one `user`
request message, `response.content` = `JSON.stringify({flagged, categories})`
over the raw result (each category `{category, flagged, score}`, `score` via
`js_number_to_json`), `userId` + `chatId` only, awaited-and-ignored. The
`danger_gatekeeper_tier3` differential dropped its `strip_moderation` filter and
now diffs both moderation rows byte-for-byte (regenerated green). The
moderation-provider-failure case writes no row (v4 identical — the throw skips
the log), and a classification-cache hit never reaches the provider. Sibling
differentials `danger_routing` + `moderation_wire` re-verified green.
W4.11b: regenerated the `primary_stream_tier3` differential with `logLLMCall`
live and an `llm_logs` dump/diff (the W4.7e3 step-6 regen), and fixed the real
port gap it surfaced. The oracle's model mock moved down from the service-level
`streamMessage` wrapper to `createLLMProvider`, so v4's REAL wrapper (and its
terminal CHAT_MESSAGE `logLLMCall`) now runs; the recorded canned keys and every
pre-existing event trace / `chat_messages` / `chats` dump are unchanged. Port
fixes: the provider-failover retry legs now write CHAT_MESSAGE `llm_logs` rows
(v4's `restreamInto` logs per `streamMessage` call — sharing `primary_stream`'s
row construction, not forking it), with `characterId = NULL` (v4's `restreamInto`
passes no `characterId`); and the tool-unsupported retry's row likewise carries
`characterId = NULL` (v4's retry `streamMessage` call omits it, unlike the primary
attempt). Closed the documented `llm_logs` `temperature` seam: an integer-valued
temperature (e.g. `1.0`, common on the CHAT_MESSAGE path) now serializes bare
(`1`) via `js_number_to_json`, matching v4's `JSON.stringify`. `durationMs` is
pinned to 0 on both sides (the oracle freezes `Date.now`; the port hard-codes 0 —
a real stream clock is a spine-injected follow-up). `requestHashes` are asserted
as part of the row diff. The orchestrator spine's failover call keeps the
no-logging entry point (threading its db + pre-generated message id is a
spine-owner follow-up). Versions: core 0.0.135, harness 0.0.129.
W4.11a (spine logging + owned-provider plumbing): added `Arc<T>` blanket impls
for the three provider seams (`EmbeddingProvider` / `CompletionProvider` /
`StreamingCompletionProvider`) so one concrete provider can be shared by value
between a borrowed spine dep and an owned, effectively-`'static` erased seam —
the production-shaped ownership answer that lets a composition point hand the
same stateful stream provider to the primary stream and an inner ask_carina /
Brahma engine. Wired the `ask_carina` tool seam into the `process_message`
spine (`OrchestratorDeps.ask_carina` + the per-turn `BuiltInToolRunner`'s
`with_ask_carina`), closing the ask_carina-through-spine dispatch (previously
the spine's runner had no engine → loud fallback). The orchestrator differential
now attaches the `llm_logs` partition + a per-call `with_logging` executor and
diffs the `llm_logs` dump: the cheap-LLM rows (distill MEMORY_EXTRACTION, the
summary fold's SUMMARIZATION + TITLE_GENERATION) match v4 byte-for-byte, while
CHAT_MESSAGE (Rust primary-stream vs v4's swallowing service-level stream mock)
and DANGER_CLASSIFICATION (v4's inline pre-turn classify, a documented spine
seam) rows are filtered on both sides. The oracle mocks `runPreContextPreCompute`
to its inert empty result so v4's second (pre-compute) distill call — a spine
deferral — does not double the MEMORY_EXTRACTION rows. The harness's erased
ask_carina engine + a live `RealBrahmaConsole` are constructed over the shared
Arc providers (inert-verified against the 23-case corpus). The two live corpus
cases (ask_carina tool-call, live Brahma `@Name:`) are deferred: the ask_carina
case needs v4's tool-path `carinaAnswer` emit matched, which requires wiring the
per-turn sink through `ToolExecutionContext` (out of this lane's file ownership;
"fix the port not the diff" forbids filtering v4's frame); the live-Brahma case
needs a global default connection profile + api key that would ripple through
the 23 existing cases' profile/cheap-LLM resolution.

Cleanup-round prep: wrote the three work orders that close every standing
pre-enclave follow-up — W4.11a (spine `with_logging` + the orchestrator
`llm_logs` dump; Arc blanket impls on the provider traits so composition
points can share one provider between the borrowed spine deps and the owned
erased seams; the live `ask_carina`-through-spine and live-Brahma corpus
cases), W4.11b (the W4.7e3 step-6 `primary_stream_tier3` regen — the oracle's
model mock relocated below the `streamMessage` wrapper — plus the real
failover-logging gap fix the survey surfaced: v4's provider-failover retries
write CHAT_MESSAGE `llm_logs` rows and the ported drain loop doesn't), and
W4.11c (widen the moderation seam so the wire's per-category `flagged`
reaches the gatekeeper and write v4's `modelName:'moderation'` row
byte-exact, dropping the `strip_moderation` filter). v4 drift check: HEAD
still `6b6e39ad`, oracle baseline unchanged. Round table updated; this round
is the enclave's enabler (U4.4's token accounting sums real `llm_logs` rows).

Wiring-round unification: integrated the three parallel lanes (W4.10a spine
wiring, W4.5b Brahma console, W4.10b logging regens) onto main — zero
source-level conflicts for the second consecutive round. One integration fix:
the cherry-pick's take-theirs resolution on the harness Cargo.toml clobbered
W4.10b's tempfile dev-dependency (restored; caught by the gate). The W4.5b
spine swap-in landed here: the orchestrator differential's carina composition
now constructs the real RealBrahmaConsole (inert — no Brahma corpus case — so
it proves the generic composition typechecks). Verified on the integrated
tree: the full workspace gate (898 tests, clippy -D warnings on default and
native-transport, fmt) and an eighteen-differential sweep against freshly
regenerated v4 oracles at 6b6e39ad, all green. Versions: core 0.0.134, harness
0.0.128.

W4.10a (the spine wiring pass): closed three deferred composition-point seams.
(1) `model_supports_native_tools` is now sourced in-spine from the real
`check_model_supports_tools` over an injected `PricingFetcher` (the fetch stays a
seam); the `ProcessMessageInput` field was dropped. (2) The danger router is wired
with the real DB-backed `DbApiKeys` resolver, reading the fixture-seeded `api_keys`
table end to end (closing the W4.7d→W4.4b key-material handoff). (3) The real
`RunCarinaQuery` engine is wired: a `RealCarinaQuery` adapter over
`run_carina_query` at the finalizer markup path, plus an erased `ErasedAskCarina`
seam + `ask_carina` dispatch row on `BuiltInToolRunner` (additive; default = the
prior loud fallback). The orchestrator corpus gained a live `@Name:` markup case
(the recorded inner carina stream proves the engine's system-prompt bytes; the
carina message posts, the `carinaAnswer` event emits, the `CARINA_MEMORY_EXTRACTION`
job enqueues), and `tool_dispatch` gained an `ask_carina` row (a not-found answerer
drives the real engine's early-return against v4's real dispatch). Regenerated the
orchestrator oracle (un-mocked `checkModelSupportsTools` + empty `getPricingCache`;
un-monkey-patched `findApiKeyByIdAndUserId`; `textblock_mode` → OPENAI `o1-mini`).
`message_finalizer` / `carina_runner` / `mail_carina` / `tool_build` /
`regenerate_swipe` / `tool_dispatch` re-verified green. Deferred: a live
`ask_carina` tool-call THROUGH the `process_message` spine (the erased-seam
`'static` boundary needs owned engine providers, which the differential's shared
borrowed streaming provider cannot supply); the dispatch + engine are proven by the
seam unit tests, the live `@Name:` case, and the `tool_dispatch` row.

Wiring-round prep: wrote the three work orders for the post-Round-4 spine
closure — W4.10a (the spine wiring pass: source model_supports_native_tools
from the real check_model_supports_tools, wire the real DB-backed
ApiKeyResolver at the danger router, construct the real RunCarinaQuery at the
orchestrator/finalizer composition points with the ask_carina dispatch row and
the live @Name:/ask_carina orchestrator-corpus cases), W4.5b (the Brahma
one-shot console — v4's runBrahmaQuery composed from already-ported units,
implementing the frozen RunBrahmaConsole trait, with its own tier-3
differential), and W4.10b (the staged W4.7e3 llm_logs oracle regenerations,
steps 1-7). Round table updated with the three-lane parallel layout and
ownership rules; the spine with_logging wiring plus an orchestrator llm_logs
dump is deliberately post-round (it would couple W4.10a's corpus to W4.10b's
primary-stream regen). Written from two fresh surveys (the v5 composition
points; v4's brahma-console/one-shot.service.ts at 6b6e39ad — no drift). No
code changes.
W4.10b step 7 (logLLMCall regen — memory processor + context summary): un-mocked
`logLLMCall` in both oracles and gave the Rust side a per-call/per-op
`with_logging` executor over an attached llm-logs partition. memory_processor
diffs the 11 MEMORY_EXTRACTION rows the SELF/OTHER extraction passes write (chatId
+ the extracted characterId, no messageId); context_summary diffs the 11
SUMMARIZATION (fold) + TITLE_GENERATION (title) rows (chatId only). Both green with
no port change. Step 6 (primary_stream) is deferred — see the follow-up note.

W4.10b step 5 (logLLMCall regen — avatar + story-background jobs): un-mocked
`logLLMCall` in both job-handler oracles (per-case fresh llm-logs DB) and attached
the llm-logs partition on the Rust side. The avatar handler makes no cheap-LLM
call, so it writes only IMAGE_GENERATION rows via `generate_with_reroute` (the
`posthoc_reroute` case banks the reroute leg's second row); the story handler adds
a per-case `with_logging` executor, diffing the full type matrix
(SUMMARIZATION [derive-scene] + IMAGE_PROMPT_CRAFTING [craft, incl. the empty-craft
retry] + APPEARANCE_RESOLUTION [incl. the appearance retry] + IMAGE_GENERATION).
Both green with no port change.

W4.10b step 4 (logLLMCall regen — image generation): un-mocked `logLLMCall` in the
`image_generation_tier3` oracle (per-case fresh llm-logs DB) and attached the
llm-logs partition + per-case `with_logging` executor on the Rust side, diffing
the IMAGE_GENERATION rows (`durationMs: 0`, frozen clock) plus the cheap
IMAGE_PROMPT_CRAFTING task row on the craft-fallback case; avatar cases write
none. Fixed a second instance of the summarize divergence: v4's `summarizeRequest`
always emits `temperature`/`maxTokens` (present as `null`), but the port's
`LlmLogRequestSummary` skipped them when `None` — changed both to the same
present-null-vs-absent double-`Option` as `error`/`finishReason` (generalized the
double-option deserializer), surfaced by the IMAGE_GENERATION row (both null).
`llm_logs_tier2` re-verified (its fixture has temperature absent, maxTokens
present).

W4.10b step 3 (logLLMCall regen — answer confirmation): un-mocked `logLLMCall` in
the `answer_confirmation_tier3` oracle and gave the Rust finalizer a per-call
`with_logging` executor over an attached llm-logs partition, diffing the 13
ANSWER_CONFIRMATION rows the check + re-affirmation calls write (one per check,
plus one per re-affirmation on the three inconsistent cases). Each row carries the
call's chatId + assistant messageId + responder characterId. Green with no port
change.

W4.10b step 2 (logLLMCall regen — danger gatekeeper): un-mocked `logLLMCall` in
the `danger_gatekeeper_tier3` oracle and attached the llm-logs partition on the
Rust side, diffing the four `DANGER_CLASSIFICATION` rows the cheap-LLM classify
path writes. v4's moderation path also logs (`modelName:'moderation'`) but that
logging is a tracked unported seam (the projected `ModerationResult` drops the
raw per-category `flagged`), so those rows are filtered on both sides. Green with
no port change (the closure was already wired in W4.7e3).

W4.10b step 1 (logLLMCall regen — compression): converted the `compression_tier3`
oracle from a DB-free jest test to a real-DB one on both sides, un-mocking
`logLLMCall` and dumping the written `llm_logs` rows (`CONTEXT_COMPRESSION`), so
the writer is proven byte-for-byte through a real cheap-LLM call site. Six rows
land (happy-path + the two uncensored-fallback pairs + the unicode case; the
empty-window and llm-failure cases write none). Fixed a real port divergence the
row diff surfaced: v4's `summarizeResponse` always emits `error`/`finishReason`
(present as `null`), but the port's `LlmLogResponseSummary` skipped them when
`None` — changed both to the present-null-vs-absent double-`Option` (like `chats`'
`removedAt`), so the summarize path stores them present-null while a raw tier-2
write with the key absent still stores them absent (`llm_logs_tier2` re-verified).
Also fixed the corpus `userId` (`user-1` -> a real UUID) since the llm_logs schema
validates `userId` as a UUID and silently dropped the write otherwise. Added a
shared `tests/common` helper (real-Db-with-llm-logs setup + normalized dump) for
the remaining regen steps.
W4.5b: ported the Brahma one-shot console (v4 `runBrahmaQuery`,
`lib/services/brahma-console/one-shot.service.ts`), closing the `RunBrahmaConsole`
seam W4.5 left injected. New `services::brahma_console`: the default-profile
resolver + the tool-call stuck-loop signature (v4's two `orchestrator.service`
helpers), the byte-exact system prompt (base brief + `BRAHMA_SQL_PROMPT` in a
generated `prompt_text` submodule), and `run_brahma_query` — the isolated
`[system, question]`-only slate, the api-key gate, the console tool slate
(agent mode + doc read/write + read-only `run_sql` + search, no `ask_carina`,
no workspace tools), the simple-json→text-block coercion, and the 25-turn agent
tool loop (native/text-block detection, submit-via-args + raw-text fallback, the
`MAX_DUPLICATE_TOOL_CALLS = 2` dup/stale stuck-loop guard with the byte-exact
nudge, tool execution at operator surface with side effects standing but nothing
persisted). `RealBrahmaConsole` implements the frozen trait. Verified by
`brahma_console_tier3_equivalence` (drives v4's REAL `runBrahmaQuery` over nine
cases — no-profile, both api-key detail strings, plain answer, submit via args
and via raw text, empty response, a real `run_sql` iteration threading its
byte-exact result through the continuation, and the duplicate-call nudge — the
recorded canned stream keys proving the system-prompt bytes; REAL tools on both
sides through the real `BuiltInToolRunner`), plus nine module unit tests (the
loop bound, the dup + stale guards over a seeded Db, the never-throws / no-profile
sentinel, and the pure helpers). The spine/Carina swap-in (constructing a
`RealBrahmaConsole` at the `answer_as_brahma` composition point) is a unification
one-liner. Deferred: the differential doc-edit-write + search cases (both handlers
proven by `doc_text`/`doc_fm` + `search_tools`, and the console dispatches through
the identical real runner; a doc write threads a per-side-minted `mtime` that a
canned-key replay cannot reproduce, so `run_sql` proves the operator-surface loop
+ threading instead).

Round-4-remainder unification: integrated the four parallel lanes (W4.4b
file/attachment, W4.5 carina query, W4.7e2 TF-IDF vectorizer, W4.7e3 logLLMCall
call-site closures) onto main. No cross-branch code conflicts this time — the
disjoint-files discipline held completely (conflicts were docs/mod-decls only,
union-resolved; versions auto-merged to one round bump). Verified on the
integrated tree: the full workspace gate (886 tests, clippy -D warnings on
default and native-transport, fmt) and a fifteen-differential sweep against
freshly regenerated v4 oracles — the four units' own proofs (text_detection,
file_attachment, carina_query, carina_memory_extraction, tfidf_vectorizer,
embedding_refit) plus the regenerated orchestrator corpus, the shared-file
cross-checks (answer_confirmation, message_context_leaves, carina_runner,
mail_carina_tools over the now-async RunCarinaQuery seam), and the
e3-touched tier-3s (danger_gatekeeper, primary_stream, image_generation,
avatar_job) — all green.

W4.7e2: ported the BUILTIN TF-IDF/BM25 embedding provider (v4's zero-network
fallback embedder, `plugins/dist/qtap-plugin-builtin-embeddings/`). New
`quilltap-core::tfidf` module: the Porter stemmer + tokenizer (`porter` — a
byte-for-byte transcription of v4's hand-rolled stemmer, NOT a crate, since a
divergent stem shifts every stored vocabulary index; `STOP_WORDS`, `stem`,
`tokenize`, `generate_bigrams`), the BM25-enhanced vectorizer (`vectorizer` —
`fit_corpus`/`transform`/`get_state`/`load_state`/`is_fitted`, the BM25 IDF
`ln((N-df+0.5)/(df+0.5)+1)` and TF saturation, f64 throughout; the fit clock
injected), and the `BuiltinEmbeddingProvider` wrapper. Host glue
`services::builtin_embedding::generate_builtin_embedding` (v4
`generateBuiltinEmbedding`: load the persisted state via
`tfidf_vocabulary.findByProfileId`, transform, route through
`applyEmbeddingProfile`), plus new scoped reads
`embedding_profiles::find_by_id` and `tfidf_vocabulary::find_by_profile_id`.
The `EMBEDDING_REFIT` job handler (`services::embedding_refit_job` — gather
every character's memories + the help docs, `fit_corpus`, persist via
`tfidf_vocabulary.upsertByProfileId`, enqueue `EMBEDDING_REINDEX_ALL`; skip
branches for non-BUILTIN / no-characters / no-memories), registered with the
W4.8 runner via `EmbeddingRefitHandler`;
`queue_service::enqueue_embedding_reindex_all` added. The debounce scheduler is
host-timing (not ported — the only pure gate, BUILTIN-profile, is
`is_builtin_profile`). Two differentials: a tier-1
`tfidf_vectorizer_equivalence` (159 rows — stemmer suffix families, tokenizer,
bigrams, fit→getState + transform, loadState-from-JSON, the two throw messages;
`idf`/vectors compared at 1e-12) and a tier-3 `embedding_refit_tier3_equivalence`
(drives v4's REAL `handleEmbeddingRefit` over a two-DB fixture, diffs
`tfidf_vocabularies` + `background_jobs`, plus a runner-registration E2E).
Documented seam: the IDF's `Math.log` diverges from V8 by <=1 ULP on macOS libm
(and the `libm` crate), so the persisted `idf` JSON is compared numerically at
1e-12 in the tier-3 diff; everything else is byte-exact.

W4.7e3: wired the six `logLLMCall` call-site closures so ported call sites now
write `llm_logs` rows via the W4.7e `services::llm_logging` writer.
`CheapLlmTaskExecutor` gained an optional `CheapLlmLogConfig` (Db + per-service
userId/chatId/messageId + LogContext) and a per-call `task_type` on `execute`;
each successful cheap-LLM provider call writes one row (the log type mapped from
`task_type`), covering compression, answer-confirmation, image scene tasks,
memory extraction, context summary, scene-state, and recap. The gatekeeper's
LLM-classify path writes a `DANGER_CLASSIFICATION` row (`classify_content`
gained a `db` param); the moderation path is not ported (the projected
`ModerationResult` drops the raw per-category `flagged` v4 serializes — a
tracked seam). `generate_image` (4 sites), the avatar/story job handlers (via
the shared `generate_with_reroute`, 4 sites), and the primary stream (on
`chunk.done`, with the request-prefix hashes + finishReason) each write their
rows; `durationMs` emits 0 (the frozen-clock differential expectation; a real
value needs a spine-injected clock — a follow-up). All request-path sites pass
`LogContext::none()`. A new in-process self-test drives a cheap-LLM task through
a real single-writer `Db` (main + llm-logs partitions) and asserts one
correctly-shaped `llm_logs` row (the writer's through-a-real-call-site proof,
in process). The byte-exact per-oracle differential regenerations (compression,
danger_gatekeeper, answer_confirmation, image_generation, avatar/story,
primary_stream, memory_processor, context_summary — each un-mocking
`logLLMCall` + dumping `llm_logs`) are staged follow-ups. No spine files
touched.

W4.5: ported the Carina query engine (`services::carina_query`, v4
`carina.service.ts` `runCarinaQuery`) — the isolated reference-answer engine that
resolves an answerer character and produces a minimal, isolated answer. Composes
the ported subsystems: answerer resolution (all name matches oldest-first, prefer
`canBeCarina`, else the operator/user-controlled/`canBeCarina`-asker gate), the
not-participant-scoped connection-profile chain (answerer default →
`connections.findDefault` [new `connection_profiles::find_default`] → first
web-search-capable via the provider registry → no-profile), the system-prompt
build (identity stack + `## Scenario` + the surface-level asker identity card +
the Commonplace memory-recall block), prior-Carina-exchange replay, Carina's own
5-iteration detect→execute→re-stream tool loop + the forced-text final turn, the
`systemSender:'carina'` post + the live `carinaAnswer` emit, and the
`CARINA_MEMORY_EXTRACTION` enqueue. The Brahma one-shot console is an injected
seam (`RunBrahmaConsole`, default = the `llm-failed` shape; the gate + sentinel-id
post path ARE ported — the console engine is the W4.5b follow-up). Added
`services::carina_memory_extraction` (the SELF-only synthetic-transcript
extraction over the ported `process_turn_for_memory`) and
`queue_service::enqueue_carina_memory_extraction` (deduped by `carinaMessageId`).

W4.5: converted the `RunCarinaQuery` seam to async (RPITIT `-> impl Future +
Send`). The work orders' "frozen" constraint is the seam's behavior + argument
shape, not its sync-ness (an artifact of the canned test impl); every real caller
(the runner, the finalizer, the `ask_carina` dispatch) is already async and simply
awaits, matching how `BuildContextSeams` / `ContextSummarySeams` /
`LanternNotificationSink` went async. `run_carina_markup_query` / `execute_ask_carina`
became generic over the seam (RPITIT is not dyn-compatible); the sync `#[test]`
harnesses that drive the runner gained a current-thread runtime. `carina_runner_tier3`
and `mail_carina_tools` re-verified green against fresh v4 oracles (behavior
identical — oracles NOT regenerated). Verified: `carina_query_tier3_equivalence`
(13 cases driving v4's REAL `runCarinaQuery` — plain / name-collision /
profile-chain / memory-recall / prior-exchange / one tool iteration+threading /
forced-text / whisper vs public / Brahma reachable+unreachable / asker-gate→not-found
/ empty→llm-failed / extraction-enqueue — the system-prompt + recall bytes proven
via the canned stream key; no engine divergence) and
`carina_memory_extraction_tier3_equivalence` (the SELF-only outcome over v4's REAL
`handleCarinaMemoryExtraction`). Spine seam closure (the `ask_carina`
`BuiltInToolRunner` dispatch row + constructing the real `RunCarinaQuery` at the
orchestrator/finalizer composition point + the live `@Name:`/`ask_carina`
spine-corpus cases) is handed to the spine owner (W4.4b/unification) per the round
layout.

W4.4b: ported the chat file/attachment LLM-load subsystem and closed its two
standing seams (`OrchestratorSeams::process_files` and
`MessageContextSeams::load_lantern_images`). New pure leaves under `files::` —
`text_detection` (the full 96-entry ext→MIME table + content sniffing, with its
own tier-1 differential), `image_processing` (the base64-size + provider-limit
resize DECISION logic over an injected `ImageTranscoder` seam — no image codec in
the core; the geometric-scale loop and its quirks reproduced faithfully), and
`attachment_support` (v4's client-safe `PROVIDER_ATTACHMENT_CAPABILITIES` map).
New services — `file_fallback` (`file-attachment-fallback.ts`: the three-tier
image description [persisted-prompt reuse FIRST, then the vision call over the
`CompletionProvider` seam with the uncensored retry, then the `IMAGE_DESCRIPTION`
`logLLMCall` write], text→inline, the keep-vs-drop rule, the prefix markers) and
`chat_files` (the LLM-load half of `chat-files-v2`: `loadChatFilesForLLM` +
`loadMountFileAsAttachment` + `readFileAsBase64` over the injected `FileBytesStore`
byte seam, plus `loadAndProcessFiles` and the Lantern K-loader). The vision call
reuses the completion seam via new `CompletionParams.attachments` +
`CompletionResponse.finish_reason` + a backward-compatible
`canned_completion_key_with_attachments` (byte-identical to the base key when
attachments are empty, so every pre-W4.4b oracle keys unchanged). The K seam went
async (RPITIT + Send). Widened `db::files::FileEntry` with `size` + `description`;
added `find_link_meta_by_linked_to` and `doc_mount_file_links::find_with_content_by_file_id`.
Regenerated `orchestrator_tier3` and re-ran `message_context_leaves` green (the
new seams are inert on the existing corpus — file ids empty, no prior-image
attachments). Deferred (flagged, out of the deliverables checklist): the two
inherited spine handoffs — sourcing `model_supports_native_tools` from
`pricing_fetcher::check_model_supports_tools`, and wiring `ConnApiKeys` into the
danger/cheap/image composition points.

Docs: wrote the two remaining follow-up work orders — W4.7e2 (the BUILTIN
TF-IDF/BM25 vectorizer: Porter stemmer transcription, the BM25 fit/transform
math, loadState over the ported tfidf_vocabulary rows, and the EMBEDDING_REFIT
job handler) and W4.7e3 (the logLLMCall call-site closures: six in-scope sites
mapped with their log types, plus the staged per-oracle regeneration plan with
llm_logs dumped). Updated the W4.4b order (the IMAGE_DESCRIPTION logging seam
note retired — W4.7e landed — and the two inherited spine handoffs recorded)
and the chat-orchestration round table with the Round-4-remainder parallel
layout: W4.4b ∥ W4.5 ∥ W4.7e2 ∥ W4.7e3, contention rules included. No code
changes.

Round-4 unification: integrated the four parallel Round-4 branches (W4.7d,
W4.7e sub-units 1-4, W4.9c, W4.6c) onto main alongside the already-landed
W4.7f. One real cross-branch conflict fixed: the W4.9c handlers were written
against the pre-W4.7f `GeneratedImageData` (`data: String`); adapted both
handlers to the widened `Option<String>` + `url` shape with v4's exact falsy
semantics (`rawData = imageData.data || imageData.b64Json; if (!rawData)` —
missing AND empty-string payloads both no-op) and updated the two canned-image
test constructions. One clippy doc-comment fix (a doc_fm header line read as a
markdown list). Verified on the integrated tree: full workspace tests (619
core + harness self-tests), clippy `-D warnings` (default and
`native-transport`), fmt, and all eleven Round-4 differentials re-run green
against freshly regenerated v4 oracles (api_keys, llm_errors, google-wire,
pricing_fetcher, request_prefix_hashes, embedding_wire, avatar_job,
story_background_job, doc_fm/doc_blob/doc_ui with the Librarian announcements
live) plus build_context_tier3 confirming the harness `float_roundtrip`
enablement is inert on existing normalizations.

Phase 3 — W4.6c (the remaining Librarian doc-edit announcements, the Round-3
Group-6 leftover): the file-management, blob, and document-UI doc-edit handlers
now emit their Librarian announcements — move, copy, delete, folder-created,
folder-deleted, open, and blob-write (previously only the doc-save
`change:{diff}` write announcement fired, from G6). Generalized the shared
`DocEditToolResult.pending_librarian_announcement` field from
`Option<LibrarianWriteAnnouncement>` to an `Option<PendingLibrarianAnnouncement>`
enum (one variant per announcement kind, each carrying the frozen W4.6b writer's
argument struct); the field stays `#[serde(skip)]` so the ~23-handler serialized
result shape is byte-unchanged. Each database-store handler branch builds its
announcement inside the synchronous `Db::write` closure (it needs the RW
connections for `uriForResolvedPath` / `resolveActorOrigin` /
`documentHiddenFromCharacters`) and the executor spine dispatches by kind to the
matching async `postLibrarian*` poster after the closure returns (the G6 /
wardrobe-drain `pending*` precedent; best-effort, a failed post never fails the
tool). `doc_open_document` ports v4's bespoke open-origin resolution
(`characters.findById` name lookup → `opened-by-character` else `opened-by-user`,
NOT the shared `resolveActorOrigin`). Added sync `post_librarian_*_announcement_conn`
siblings for the seven writers that lacked one + a `post_pending_librarian_announcement_conn`
dispatcher so the direct-drive differentials post over the held RW `main`
connection, and a synchronous `document_hidden_from_characters` handler helper.
Regenerated `doc_fm` / `doc_blob` / `doc_ui` with the announcement writers LIVE
(un-mocked) and a MAIN-db `chat_messages` dump added to each (ordered by
`content`, a remap-invariant key), diffing the Librarian rows byte-for-byte
(7 file-management rows, 3 blob rows, 2 open rows). The open announcement is an
actual `type:'message'` event, so it bumps the chat's `updatedAt` on both sides
(the doc-ui "updatedAt never bumped by open/close" pin is retired accordingly).
`doc_text` + `tool_dispatch` re-verified green (the enum generalization is inert
for the write kind and for the non-announcing read handlers). The
filesystem-mount announcement sites stay behind the existing `FsSeam` (out of
scope); `syncChatDocuments*` stays the corpus-verified no-op seam.

W4.9c: ported the avatar + story-background background-job handlers
(`CHARACTER_AVATAR_GENERATION` / `STORY_BACKGROUND_GENERATION`), removing both
job types from the runner's loud fallback. New: the two scene cheap-LLM tasks
(`deriveSceneContext`, `craftStoryBackgroundPrompt` — the GROK 1000-char length
guidance, prompts byte-exact); the aesthetics module (`resolveAesthetic` tiered
project-official → Quilltap General, `resolveDepictionGuidelines` — the Ariel
Clause, `getProjectOfficialMountPointId`); the avatar prompt builder
(`buildCharacterAvatarPrompt` with the reworked bare-top collarbone-crop branch);
the two storage bridges (`writeCharacterAvatarToVault` → the character vault
`images/history/`, `writeLanternBackgroundToMountStore` → the Lantern Backgrounds
store `generated/`); the `enqueueStoryBackgroundGeneration` queue op +
`resolveImageProfileForChat` + the `queueStoryBackgroundIfEnabled` gate (the
TITLE_UPDATE handler wiring point is documented, not yet wired). Added a
`describeOutfit` omit-aware variant to the wardrobe leaf, and the
`characterAvatars` / `storyBackgroundImageId` / `lastBackgroundGeneratedAt`
`ChatUpdate` setters (no `updatedAt` bump). Aesthetics differ by handler: avatars
use aurora only (the Ariel Clause deliberately does not apply); story backgrounds
use lantern + aurora + the Ariel Clause. Both handlers reuse the W4.9a image
subsystem (image/completion/moderation/transcoder seams, the Concierge pre-scan +
post-hoc moderation reroute, `resolveOrientation`) and the W4.8 job runner. Both
verified by jest real-DB tier-3 differentials driving v4's REAL handlers.
`logLLMCall` stays a documented deferral (the generate_image precedent); the
project-store `fileStorageManager.uploadFile` branch is an injected host FsSeam.

Phase 3 — Wave 4 (W4.7e, pricing / capability / logging / embeddings): ported
four of the five W4.7e sub-units, each with a green differential against v4's
real code.

- The LLM logging service (`services::llm_logging`, v4 `llm-logging.service.ts`)
  closes the standing `logLLMCall` deferral: `summarize_request`/`_response`
  (full content, UTF-16 `contentLength`, `hasAttachments`, `toolCalls` mapped),
  `is_logging_enabled` (logs by default — missing settings and read errors both →
  enabled), the row writer over the ported `llm_logs.create` (usage/cacheUsage/
  requestHashes gated, `rawProviderUsage` null-collapsed), `map_task_type_to_log_type`
  (verbatim incl. the `SUMMARIZATION` default), the 19 `LLMLogType` constants
  (`TOOL_CONTINUATION` has no emitter), and an explicit `LogContext`
  autonomous-run-id (no thread-locals — v4's AsyncLocalStorage becomes a param).
- The cache-prefix hashes (`cache_prefix_hashes`, v4 `cache-prefix-hashes.ts`):
  per-tier SHA-256 (first 16 hex) of the cacheable request regions. Reproduces
  the sorted-key `stableStringify` (distinct from every insertion-order serializer
  in the port) and the history-tail `undefined`-renders-literally quirk. Tier-1
  differential (`request_prefix_hashes_equivalence`, 17 rows).
- The pricing fetcher + cost estimation + capability check
  (`services::pricing_fetcher`, v4 `pricing-fetcher.ts` + `cost-estimation.service.ts`
  + `checkModelSupportsTools`): sans-IO (the fetch is an injected `PricingFetch`
  seam, `now_ms` injected), the two OpenRouter response casings ported as separate
  parsers, JS `parseFloat` string-price semantics (garbage → NaN), the 24 h TTL +
  5 min negative cache, slug exact-then-fuzzy match, `findCheapestAvailableModel`
  filters, and the `estimateMessageCost` cascade with all source tags. Closes the
  finalizer cost-estimation seam; the `LEGACY_FALLBACK_PRICING` rows are a
  generated Rust static. Tier-1 differential (`pricing_fetcher_equivalence`,
  6 scenarios driving v4's real async exports with fetch/SDK/repo mocked).
- The embedding wire (`model::embedding_wire`, the plugin embedding providers):
  sans-IO per-provider request builders + response parsers — OpenAI
  (`{model, input, dimensions?}`), Ollama (empty-input guard, `/api/embed` with
  the `/api/show`-derived `num_ctx`, the 404 legacy fallback, the finite-vector
  guard), and OpenRouter (the SDK request body + the base64-Float32 decode). Tier-1
  differential (`embedding_wire_equivalence`, 12 rows). `applyEmbeddingProfile`
  was already ported (`embedding_vector`).

Enabled the `float_roundtrip` serde_json feature in the harness so an oracle's
exact-float text (e.g. a price `0.09999999999999999`) parses correctly-rounded,
matching the core's own f64 (the default fast parser is 1-ULP lossy).

Tracked follow-ups (explicit, per the W4.7e work order's degradation plan): the
`logLLMCall` writer's through-a-real-call-site row diff (regenerate the smallest
cheap-LLM oracle with logging un-mocked) + the call-site closures (`cheap_llm_exec`,
`primary_stream`, gatekeeper, answer confirmation, image generation) and their
oracle regenerations; and sub-unit 5, the BUILTIN TF-IDF/BM25 vectorizer, split
off as W4.7e2 (it has no dependency on sub-units 1–4). The `model_supports_native_tools`
field removal is handed to Round-4's spine owner (W4.4b) per the work order.

Phase 3 — Wave 4 (W4.7d): transport, the LLM error taxonomy, and the `api_keys`
table (the last unported repo). Ported:

- `db::api_keys` — the plaintext `api_keys` table (hosted inside v4's
  ConnectionProfilesRepository). `create`/`update`/`delete`/`recordUsage` +
  `findById`(unscoped)/`findByIdAndUserId`/`getApiKeysByUserId` (the per-row
  safeParse DROP). Tier-2 differential `api_keys_tier2_equivalence` (minted-values
  remap; proves the recordUsage lastUsed set + the malformed-row drop).
- `services::api_key_service` — `get_api_key_for_connection_profile` /
  `get_api_key_for_cheap_llm_selection` + the user-scoped wrappers +
  `find_active_api_key_for_provider` (the web-search/moderation provider scan).
  Closed the `ApiKeyResolver` seam with a real DB-backed resolver
  (`ConnApiKeys`); spine wiring at the composition points is handed to W4.4b.
- `services::llm_errors` — the 8-class error taxonomy + `handleProviderError`
  (precedence-ordered normalizer) + `getUserFriendlyError`. Tier-1
  `llm_errors_equivalence` (54 rows, incl. precedence collisions + predicate
  regression rows).
- `model::response_parse` — non-streaming response parsers for all 5 wire
  families (chat-completions flavors, responses-API, anthropic, google, ollama)
  → LLMResponse. `model::provider_models_api` — validate/models endpoints + list
  parsers. Unit-tested; the recorded-payload differential is a tracked follow-up.
- `model::transport` — the `ProviderTransport` IO boundary (trait + policy +
  per-provider header builder, all IO-free) with a feature-gated
  (`native-transport`) reqwest impl. `model::completion_provider` — the production
  CompletionProvider composition (build → transport → parse → CompletionResponse).
- Closed the W4.7c Google `config → wire` framing deferral: `build_request` now
  emits the genai-SDK wire body for GOOGLE (generationConfig split,
  `{name,args}`→`{args,name}`, systemInstruction wrapper). Byte-verified against
  the recorded wire (`request_builder_google_wire_equivalence`, 5 cases).

W4.7f: image wire dialects + OpenAI moderation + Serper web search. Ported the
five sans-IO image-generation dialects (`model::image_dialects` —
`build_image_request` + `parse_image_response` for OPENAI, GOOGLE Imagen +
Gemini, GROK, OPENROUTER, Z-AI), with every rejection path normalized to the
exact error strings v4 surfaces and the three refusal-keyword gaps (Gemini
"No images returned", OpenRouter "Model declined", z-ai's absent moderation
handling) carried faithfully. Added `RealImageProvider` composing build + a new
injected `model::wire::WireTransport` seam + parse. Transcribed the real
per-provider orientation/constraint declarations into `image_gen_data`
(OPENAI/GOOGLE/OPENROUTER per-model, GROK/Z-AI provider-level). Ported the
OpenAI moderation wire (`dangerous_content::moderation_wire` +
`RealModerationProvider`) and the Serper web-search wire (`tools::web_search` —
`build_serper_request` / `map_serper_results` / the plugin + fallback error sets
/ `RealWebSearchProvider`), closing the W4.2 and W4.1d5 provider seams (the
api-key lookups stay behind the existing seams pending W4.7d's `db::api_keys`).
`GeneratedImageData` now carries `url` + an optional `data` (v4's
`GeneratedImage`, for z-ai's dual b64+URL happy path). Three new tier-1
differentials against v4's REAL plugins (`image_dialects_equivalence`,
`moderation_wire_equivalence`, `web_search_wire_equivalence`); regenerated
`web_search_tool` (real provider + the env-var fallback path), `danger_gatekeeper`
(real moderation plugin over canned wire, the failure case a canned 500), and
`image_generation` (real dialect over canned wire) tier-3 differentials green.

Docs: Round-4 work orders complete. Wrote the five remaining agent-ready work
orders from fresh v4 surveys at `6b6e39ad`: W4.7d (transport, errors, the
`api_keys` table — the last unported repo, a hand-rolled plaintext collection
inside v4's ConnectionProfilesRepository), W4.7e (pricing fetcher, model
capability, logLLMCall, embedding wire + the BUILTIN TF-IDF vectorizer — the
decomposition's "builtin already ported" claim corrected: only the storage repo
is), W4.7f (the FIVE image wire dialects — z-ai was omitted from the plan —
plus moderation and web search, with the refusal-keyword gap matrix documented
as faithful), W4.9c (the avatar + story-background job handlers, carrying the
`6b6e39ad` bare-top drift), and W4.6c (the remaining Librarian doc-edit
announcements — the Round-3 Group-6 leftover). Round-4 lane layout + contention
notes added to chat-orchestration.md; provider-manifest.md decomposition
corrected. No code changes.

Phase 3 — Round-3 unification (Group 6, the Librarian doc-save `change:{diff}`
announcement coupling — **Round 3 now fully complete**): the five mutating doc-edit
write handlers (`doc_write_file` / `doc_str_replace` / `doc_insert_text` /
`doc_update_frontmatter` / `doc_update_heading`) now emit the Librarian doc-save
announcement (v4 commit `8617ce7a`) — a `change:{created,body}` payload for a fresh
file, a `change:{edited,diff}` unified diff for an edit (via the W4.d1
`generate_unified_diff`). Ported `resolveActorOrigin`; added a
`pending_librarian_announcement` field to the shared `DocEditToolResult` (never
serialized — v4 puts `change` only in the announcement call, not the tool result)
so a handler can build the announcement inside the synchronous `Db::write` closure
and the async caller (the executor spine) posts it via the already-ported
`post_librarian_write_announcement` after the closure returns (the wardrobe-drain
`pending*` precedent). A failed announcement never fails the tool (best-effort, as
v4). Added the synchronous `post_librarian_write_announcement_conn` (posts over an
already-held RW `main` connection) so the direct-drive differential can post it.
Regenerated `doc_text_equivalence` with the write announcement LIVE on the v4 side
(un-mocked `postLibrarianWriteAnnouncement` + `contentHiddenFromCharacters`), the
fixture's existing chat + participant now targeted, and a third dumped table — the
MAIN-db `chat_messages` (ordered by `content`, a remap-invariant key) — diffing the
10 Librarian rows (8 edited-by-character + 2 created-by-character) byte-for-byte
(persona content + opaque content + `systemSender:'librarian'` + per-kind
`systemKind` + null targeting). `doc_fm` / `doc_ui` / `doc_blob` / `doc_enum` /
`tool_dispatch` re-verified green (the additive field is `None` for every non-write
handler). The file-management / blob / open announcements (move / copy / delete /
folder-created / folder-deleted / open / blob-write) remain separate seams the port
still omits — out of Group 6 scope.

Phase 3 — Round-3 unification (Group 7, context-summary vault-mirror + relevant-
conversations-refresh LIVE): `RealContextSummarySeams::mirror_summary_to_vaults` and
`refresh_relevant_conversations` (previously no-ops) now run live — the fold mirrors
the fresh summary into every participant character's vault
(`writeConversationSummaryToVaults`) and then re-runs the relevant-past-conversations
search against it (`refreshRelevantConversationsOnFold`), in that order (the refresh
must read the fresh corpus). The seam trait's two methods now take the built inputs,
and `RealContextSummarySeams` is generic over an embedding provider (the refresh
embeds the query). Extended `context_summary_service_tier3` to a two-DB fixture
(main + mount-index with one provisioned vault + a pre-seeded prior summary whose
chunk carries a canned unit embedding) and regenerated the differential un-mocking
the mirror/refresh one-for-one: the mirror's write is proven by the
`doc_mount_file_links` path set (`Conversation Summaries/Old Title A.md` appears on
both sides), the refresh's `relevant-conversations` whisper by the `chat_messages`
dump. `vault_summary_mirror_tier2` (which separately proves the mirror byte-exact)
and `orchestrator_tier3` (whose summary check keeps `NoopSeams`) re-verified green.

Phase 3 — Round-3 unification (Group 8, cheap-LLM-selection spine threading): the
`processMessage` spine now resolves a real `CheapLlmSelection` at the composition
point (v4 `getCheapLLMProvider` over the user's connection profiles + the chat
settings' `cheapLLMSettings`, registry-cheapest seam injected `None`) and threads it
into `buildContext` (activating the proactive memory recap + the keyword-distillation
feeders, plus the cached-compression window) and the finalizer's async-compression
trigger — previously hardcoded `None`, which left those feeders inert in
`process_message`. Regenerated `orchestrator_tier3` dropping the `generateMemoryRecap`
+ `extractMemorySearchKeywords` mocks one-for-one: v4's real recap produces empty
content (no memories/vault summaries seeded), and the distill feeder now fires 61
live cheap-LLM calls across the 22 cases — each replayed byte-for-byte by the Rust
distill (proving the spine-resolved selection matches v4's). The empty `memories`
table yields no search results either way, so the stream canned keys do not cascade.
`regenerate_swipe_tier3` re-verified green (its BuildContextArgs takes `None`,
behavior-preserving).

Phase 3 — Round-3 unification (Group 5, commonplace-builder dedup): removed the
private `CommonplaceParts` + `build_commonplace_persona_whisper` /
`build_commonplace_llm_context` copies from `build_context.rs` and reused the
canonical `commonplace_notifications` versions (the per-turn consolidated whisper
leaves `relevant_conversations` empty, so the output is byte-identical). No behavior
change; `build_context_tier3` re-verified green.

Phase 3 — Round-3 unification (Group 4, Lantern sink rewire): deleted the truncated
`lantern_character_image_notification` placeholder in `generate_image` and wired the
W4.9a Lantern sink to the canonical W4.6b writer. `LanternNotificationSink` is now
async with a `RealLanternNotification` impl delegating to
`lantern_notifications::post_lantern_image_notification` (which composes the full
byte-exact `build_content`, incl. the "attached here" tail the placeholder dropped).
Regenerated `image_generation_tier3` with the Lantern writer un-mocked and the
persisted `character-image` notification content diffed byte-exact.

Phase 3 — Round-3 unification (Group 3, end-of-turn wardrobe drain): the
`processMessage` spine now threads ONE shared `pendingWardrobeAnnouncements` set
through every per-turn tool context (native loop + text passes) and drains it at
turn close (before finalize, v4 orchestrator.service.ts:1406) via
`aurora_notifications::flush_pending_wardrobe_announcements`, which enqueues one
`WARDROBE_OUTFIT_ANNOUNCEMENT` job per affected character. Added
`WardrobeOutfitAnnouncementHandler` (a `JobHandler` wrapping
`handle_wardrobe_outfit_announcement`) for the host/runner to register. The
pending-set recording remains proven by `wardrobe_tools_equivalence`; the flush /
enqueue / handler are individually ported (W4.1d2 / W4.8 / W4.1d2). Residual: a
Db-based end-to-end drain differential (the wardrobe_tools harness uses raw
writers, no Db).

Phase 3 — Round-3 unification (Group 2, W4.6b post-office writers): wired the
personified-system whisper POSTs live. `BuildContextSeams` is now async (RPITIT,
matching `ContextSummarySeams`) with a `RealBuildContextSeams` production impl that
delegates each POST to its W4.6b writer — core-whisper + commonplace (each with the
v4 stale-whisper sweep), host timestamp + off-scene (the off-scene scan now returns
the newcomer cards so the writer builds the announcement + stamps
`introducedCharacterIds`), and Suparṇā mail (built from the unalerted letters,
targeted at the responding participant). The commonplace `posted` still gates the
scene-cache / recall-history persists. The Prospero cadence block (public context
announcement + group-context whisper) is wired directly into the `processMessage`
spine (dropped the `post_prospero_context` seam). Regenerated `build_context_tier3`
(un-mocked writers, BuiltContext diff green) and `orchestrator_tier3` (whisper rows
— commonplace / host / prospero group-context — now appear in the diffed
chat_messages dump, matching v4's real writers). Residual: the Prospero public
project/general announcement needs a provisioned General store in the fixture (the
group-context cadence whisper is proven).

Phase 3 — Round-3 unification (Group 1, W4.7c spine wiring): wired the provider
tool reshape + native detector + provider text-markers strategy live into the
`processMessage` spine. `tool_build::build_tools` now applies
`format_tools_for_provider` as its final step, so the orchestrator sends
provider-shaped tools at the wire (Anthropic `input_schema`, etc.); OPENAI passes
through byte-identically so `tool_build_equivalence` stays green. The orchestrator
constructs `RegistryToolCallDetector::built_in()` and gates the provider-text pass
on `provider_has_text_markers` internally (dropped the `tool_detector` /
`provider_text_strategy` seam fields from `OrchestratorDeps` and the
`NoToolCallDetector` call site). Regenerated `orchestrator_tier3` with the real
provider registry initialized on the v4 oracle side so both reshape identically;
the tools-at-wire assertion now compares the reshaped slate.

Phase 3 — wave 4 (W4.4a4): the Courier transport + the compression-cache spine
plumbing. Ported v4's `courier-transport.service.ts` (the manual / clipboard
dispatch) as `services::courier_transport` + `courier::render_markdown`: the two
Markdown renderers (`renderCourierRequestAsMarkdown` / `renderCourierDeltaAsMarkdown`
— byte-exact, incl. the `\n{3,}`→`\n\n` collapse and `trimEnd()+'\n'`),
`buildCourierDeltaEvents` (the per-character checkpoint scan with the strict
`createdAt <= resolvedAt` skip, targeted-whisper filtering, the exact Staff speaker
labels, and file-attachment loading), `dispatchCourierTransport` (the placeholder
ASSISTANT message with the rendered bundle in `pendingExternalPrompt` + the delta
fallback + the union attachments, the chat pause, and the `pendingExternalTurn` +
`done{pendingExternalTurn:true}` SSE frames), and the paste/cancel resolvers
(`resolve_external_turn` / `cancel_external_turn` — public service functions; the
HTTP route is Phase-4). Closed the orchestrator courier gate (was erroring): after
`build_message_context` + the `preparing` status, a courier-transport turn now
dispatches (tool build skipped, no tool instructions — matching v4). Added the
`pendingExternalTurn` frame + `DonePayload.pendingExternalTurn` to `chat_events`
and the `ChatUpdate.courier_checkpoints` write setter. Compression-cache plumbing:
the finalizer's real `AsyncCompressionTrigger` (now async, over
`compression_cache::trigger_async_compression`) computing + persisting the cache
when the gate fires, and the `build_context` cached-compression window
(`cached_compression_result` / `cached_compression_message_count` — phase-1 uses a
warm cache verbatim, no sync compression call; the dynamic effective-window sizing).
The orchestrator reads `get_cached_compression` before buildContext (inert until the
spine threads a `cheap_llm_selection`, the tracked deferral). New differential
`courier_transport_tier3_equivalence` (drives v4's REAL `dispatchCourierTransport`
over a four-case corpus — first send / delta with whisper-filter + boundary + staff
label / forced-full / attachment union — diffing the result + SSE trace + the
persisted placeholder bytes + `isPaused`). Regenerated + green:
`orchestrator_tier3` (added a `courier_send` spine case), `message_finalizer_tier3`
(the trigger adaptation), `build_context_tier3` (a warm-cache case proving the
cached window), `compression_cache_tier3`. Marshaling: `courierCheckpoints` +
`pendingExternal*` were already ported (no drift); added the
`ChatUpdate.courier_checkpoints` setter. Tracked deferral: the paste/cancel route
handlers aren't exported (Phase-4 HTTP transport); their constituent repo ops are
tier-2/tier-3-proven and the ported service functions are unit-tested.

Phase 3 — wave 4 (W4.6b): the post-office / personified whisper writers. Ported
every v4 `lib/services/<persona>-notifications/writer.ts` into new
`services::<persona>_notifications` modules — Host, Prospero, Librarian,
Concierge, Suparṇā, Aurora (core-whisper post + the outfit whispers + the
`WARDROBE_OUTFIT_ANNOUNCEMENT` drain), Commonplace (persona/LLM whisper builders +
`refreshRelevantConversationsOnFold`), and the Lantern image notification — each
posting one `chat_messages` row through the ported `add_message` with the exact
`systemSender` / `systemKind` / targeting / `opaqueContent` / `hostEvent` /
`summaryAnchor` tuple, best-effort/error-swallowing. The steampunk/Wodehouse voice
strings are byte-exact. Also ported the conversation-summary vault bridge
(`writeConversationSummaryToVaults` + `removeConversationSummariesFromVaults`, over
the ported document store + frontmatter emitter) and composed the `chats.delete`
participant-vault summary sweep (`delete_conversation_with_vault_sweep`) — closing
the LAST Phase-2 deferral — plus the cost/system-event writer (`createSystemEvent`
+ the memory/title/context-summary wrappers, posting a SYSTEM row + the ported
token-aggregate bump). Non-spine seams closed live: the Concierge announcer seams
in `dangerous_content` (`RealDangerAnnouncer` / `RealConciergeAnnouncer` — the W4.2
`postConcierge{Danger,Manual}Announcement` deferrals), and the context-summary
Librarian re-post + cost events (`RealContextSummarySeams`); the announcer/seam
traits went async (RPITIT `-> impl Future + Send`, no boxing). Verified: six
tier-1 pure-builder differentials (host/librarian/prospero/commonplace/aurora +
concierge-lantern-suparna, byte-exact vs v4's real exports); a combined
`post_office_writers_tier3_equivalence` (drives v4's real post functions over a
two-DB fixture, diffs `chat_messages` + the cost `chats` aggregate, one case per
row-shape/systemKind); a `vault_summary_mirror_tier2_equivalence` (mirror +
rename-in-place + `syncVaults` skip + the delete sweep, five mount-index tables in
the shared-cross-db id-map remap form); and the regenerated
`context_summary_service_tier3` + `danger_gatekeeper_tier3` + the manual-flip case
(the writers now post live on both sides). Handoffs (spine-owned, deferred): wiring
the `BuildContextSeams` post methods (`post_core_whisper` /
`post_commonplace_whisper` / `post_host_*` / `post_suparna_mail`), the
`OrchestratorSeams::post_prospero_context`, and the end-of-turn wardrobe drain into
the orchestrator/build_context spine; the context-summary vault-mirror +
relevant-conversations-refresh seams (need vault fixtures + embedding); rewiring the
image subsystem's Lantern sink to the full byte-exact writer; and the Librarian
save-announcement `change:{kind:'edited',diff}` coupling in the doc-edit handlers.

Phase 3 — wave 4 (W4.7c, part 2): the request builders + the four RequestTransform
hooks. Ported the sans-IO per-provider request-envelope builders into
`quilltap-core::model::request_builder` (build a request VALUE — method/url/headers/
body — no HTTP; the transport is W4.7d). Dispatched by the W4.7a manifest
(baseUrl+endpoint → url, auth → headers). Every SDK/raw-fetch sends
`JSON.stringify(body)` verbatim, so bodies are built key-order-exact (preserve_order,
integer-valued numbers bare). The four hooks: anthropic (mid-history cache
breakpoint + tool-result batching + adaptive-thinking/sampling-param-rejection for
Sonnet 5 / Opus 4.7+ / Fable / Mythos — the rejected-model list ported as a compiled
constant, not lifted to the manifest [noted]), openai (previous_response_id chaining
— the fallback-to-full-input is a transport concern), google (the recursive
JSON-Schema sanitizer + the thoughtSignature round-trip), deepseek (reasoning_content
echo + thinking-incompatible-param strip). Chat-completions family (deepseek, z-ai
[+ web search + reasoning-effort default], openrouter [raw-fetch tools path], ollama,
openai-compatible base) and responses-API family (openai, grok) are byte-exact
against the wire. Google's genai-SDK config→generationConfig wire framing is deferred
to the transport; the google request LOGIC (sanitizer + contents/thoughtSignature)
is verified against v4's real plugin. Verified by two new differentials:
`request_builder_equivalence` (31 rows byte-exact vs v4's real plugin requests,
captured by intercepting fetch in `record-request-envelopes.mjs`) and
`request_builder_google_equivalence` (5 rows: contents/systemInstruction/
shouldDisableTools + the sanitizer via the wire functionDeclarations). With this,
W4.7c is fully DONE; the remaining provider-layer units are W4.7d/e/f.

Phase 3 — wave 4 (W4.7c, part 1): the provider tool-wire. Ported v4's
`packages/plugin-utils/src/tools/*` + the per-plugin tool glue into
`quilltap-core::model::tool_wire` — the tool-format reshape (`formatTools`:
Anthropic `input_schema` / Google `parameters` / OpenAI passthrough), the native
tool-call parse (`parseOpenAIToolCalls` / `parseAnthropicToolCalls` /
`parseGoogleToolCalls` + the Google `functionCalls` fast path), and the
spontaneous XML text-marker detect/parse/strip (the full `hasAnyXMLToolMarkers` /
`parseAllXMLAsToolCalls` / `stripAllXMLToolMarkers` suite + Google's tool_use-only
variant), all dispatched by the manifest `toolFormat` (the registry replaces
`getProvider`). The one backreference regex (`<key>value</key>`) is hand-rolled;
the other regexes reproduce JS ASCII `\w`/`\s` semantics. Closes three live seams:
the native-tool-loop `ToolCallDetector` (new `RegistryToolCallDetector`), the
text-tool-loop provider-text-markers strategy (new `ProviderTextMarkersStrategy`),
and the W4.1g `formatTools` provider reshape
(`tool_build::format_tools_for_provider`, available + tested; wiring into
`build_tools` is a documented spine handoff). Verified by `tool_wire_equivalence`
(231 rows byte-exact against v4's real plugin methods over the real b.3 catalog +
recorded rawResponses), and by regenerating `native_tool_loop_tier3_equivalence`
(real Anthropic detector over real anthropic rawResponses) and
`text_tool_loop_tier3_equivalence` (real DeepSeek provider strategy) — both green.
Deferred to W4.7c part 2: the per-provider request-envelope builders + the four
`RequestTransform` hooks.

Drift check: v4 `8617ce7a..6b6e39ad` audited — no ported unit is stale. The
commit (image-description reuse off the reply hot path + the bare-topped
avatar crop) touches only pending surfaces. Docs only: the W4.4b
file/attachment work order is retrofitted to the reworked
`file-attachment-fallback.ts` (the persisted-text reuse tiers before any
vision call, the hardened/logged/timeout-bounded vision fallback, new corpus
cases), a W4.9c drift note records the avatar-prompt bare-top branch (the
ported `describeOutfit` leaf is unchanged), and the `docs/v4/` CHANGELOG
mirror is refreshed. New oracle baseline for future orders: `6b6e39ad`.

Phase 3 — wave 4 (W4.3): the answer-confirmation service. Ported v4's
`answer-confirmation.service.ts` (the pre-landing Salon consistency check +
re-affirmation): the gate/leaf functions (`isAnswerConfirmationActive`,
`hasCheckableInputs`, `findLatestCommonplaceWhisper`, `isUserDrivenTurn`,
`gatherConfirmationInputs` with the 24 K oldest-first reference truncation) and
`runAnswerConfirmation` (the cheap-LLM consistency check, the fenced-JSON verdict
parser, the uncensored escalation of the check's cheap selection on a dangerous
chat, and the re-affirmation pass on the character's own model — consistent →
confirmed; stood by → not-confirmed + notes; rewrote → confirmed + revised +
original stashed; empty rewrite / parse failure / error → could-not-verify). The
byte-exact prompts live in a generated `prompt_text` submodule. The finalizer
seam (`NoAnswerConfirmation`) is closed with the real runner at the composition
point: the finalizer now reads the prior messages, finds the Commonplace whisper,
assembles the reference, emits the `confirming` / `affirming` status frames, and
applies the outcome (the rewrite's tool-anchor drop + reasoning collapse). The
finalizer's `isAnswerConfirmationActive` / `isUserDrivenTurn` gate leaves were
hoisted into the service (single source of truth). Verified by
`answer_confirmation_tier3_equivalence` — a jest real-DB oracle driving v4's real
`finalizeMessageResponse` with the feature ON over a 14-case corpus (the gate
matrix, user-driven skip, no-checkable-inputs skip, whisper-only /
whisper-plus-tool references, the 24 K truncation, every outcome band, and the
dangerous-chat escalation whose recorded canned key proves the cheap-profile
switch to the uncensored profile), completions pinned by oracle-recorded canned
keys; results + the ordered event trace + `chats` / `chat_messages` diffed. The
timeout wrappers are host-side (no tokio timers in the core; only the
failure→could-not-verify mapping is ported). Re-verified
`message_finalizer_tier3` + `orchestrator_tier3` green against regenerated
oracles. Full workspace `cargo test` / `clippy -D warnings` / `fmt --check`
green.

Phase 3 — wave 4 (W4.9a): the image-generation subsystem (`generate_image`).
Ported v4's `executeImageGenerationTool` end to end and dispatched it, closing
the long-deferred image handler. New `model::image` boundary (the tier-3 seam at
v4's `provider.generateImage(params, apiKey)`): the `ImageProvider` trait +
`CannedImageProvider` keyed by the exact merged request (the key proves
`mergeParameters` + `applyOrientation`), plus a separate `ImageTranscoder` seam
for the WebP transcode (no image-codec crate in the core — the `doc_blob`
precedent; `PassthroughTranscoder` is the default). Three cheap-LLM tasks
(`services::image_scene_tasks` — `craftImagePrompt` / `resolveAppearance` /
`sanitizeAppearance`, prompts byte-exact in a generated `prompt_text` submodule)
over the ported `CheapLlmTaskExecutor`. Appearance resolution
(`services::appearance_resolution` — the sceneState/trivial-skip/cheap-LLM
resolution + the five-step Concierge sanitize gate IN ORDER). The handler spine
(`tools::generate_image`): input validation, profile load/validate (API key via
the `ApiKeyResolver` seam), the Concierge integration composing W4.2 (prompt
classification when `scanImagePrompts`, expanded-prompt classification when
`scanImageGeneration`, the AUTO_ROUTE reroute, and the post-hoc reroute on a
provider moderation error), `resolveOrientation` mutating the merged params, and
`saveGeneratedImage` (base64 decode → WebP transcode seam → SHA-256 → the Lantern
Backgrounds store write under `tool/` via `link_blob_content` → the `files` row
with `source='GENERATED'` / `category='IMAGE'` / generation metadata → tag
inheritance → the Lantern notification, a recorded seam with the byte-exact
string handed to W4.6b). The avatar trigger (`services::avatar_generation` —
`triggerAvatarGenerationIfEnabled`, the `avatarGenerationEnabled` gate + the
autonomous-chat skip + profile resolution + the `CHARACTER_AVATAR_GENERATION`
enqueue in `queue_service`), closing the W4.1d2 wardrobe deferral. `generate_image`
is dispatched through the `BuiltInToolRunner` (removed from the loud-fallback set)
via an erased `ImageGenerationRunner` seam, threading the generated-image paths
into `process_tool_calls` + the finalizer link loop. Verified by the tier-3
differential `image_generation_tier3_equivalence` (jest real-DB oracle driving
v4's REAL `executeImageGenerationTool`, mocking only the image provider [canned
by exact request], the completion boundary [recorded keys prove all three task
prompts + classification], WebP transcode [deterministic pass-through both
sides], and the Lantern notification). Tracked deferrals (host / cross-subsystem
seams): the aesthetic subsystem (`resolveAesthetic` / `resolveDepictionGuidelines`
— v4 error-swallows it, so the port supplies `None` and keeps the swallow shape),
`logLLMCall`, the real WebP encoder, and the personified Lantern writer (W4.6b).
The avatar + story-background JOB HANDLERS are the follow-up W4.9c.

Phase 3 — wave 4 (W4.6a): the buildContext feeder closures. Closed the
READ/COMPUTE half of the `BuildContextSeams` trait in `services::build_context`
— the ten former seams now run real, leaving only the W4.6b whisper-POSTing
methods. New feeder modules: `services::frozen_archive`
(`getOrComputeFrozenArchive` — the effective-weight-ranked top-25, process-cached
per compaction generation, `localeCompare` id sort), `services::memory_recap`
(`generateMemoryRecap` composing the tiered-memory narrative + the vault
conversation-summary recall lists over `search_document_chunks` /
`read_database_document` / `parse_frontmatter`; prompt bodies byte-exact in a
generated `prompt_text` submodule) with the `distill` submodule
(`extractMemorySearchKeywords`), `services::off_scene` (the Host off-scene SCAN +
the content builders + `applyHostTemplates` + `findIntroducedOffSceneCharacterIds`
— the POST stays W4.6b), `services::core_whisper` (Aurora's
`resolveCoreWhisperConfig` + `assembleCorePacket` reading own + group `Core/**.md`
+ the three content builders — the POST stays W4.6b), `services::suparna_mail`
(the mail READ — `collectUnalertedMail` + `markAlerted` +
`buildSuparnaMailLLMContext`), and `services::scene_state_tracking` (the
`updateSceneState` cheap-LLM task + `capClothingSummary`, prompt bodies byte-exact;
the full `handleSceneStateTracking` job wrapper lands with the W4.8 runner
dispatch). Closed with existing code: the tiered mount pool,
`getMemoryRecallSettings`, and the live-wardrobe clothing override (adding the
small `hash_equipped_slots` / `has_equipped_items` /
`decorate_outfit_items_title_only` leaves + a `resolve_equipped_outfit_leaf_values`
variant of the outfit resolver). The scene-cache + recall-history persist writes
(`chats.update({ commonplaceSceneCache })` / `{ commonplaceRecallHistory }`) are
ported directly, gated on the commonplace POST (W4.6b). New reads:
`instance_settings::get_memory_recall_settings`,
`chats_read::find_core_whisper_overrides`,
`characters_read::find_core_whisper_enabled`,
`groups::find_name_and_official_mount_point_id_raw`, and a recursive variant of
`doc_mount_documents::find_many_by_mount_points_in_folder`. Three `ChatUpdate`
setters added (`sceneState` / `commonplaceSceneCache` / `commonplaceRecallHistory`).
Verified: `build_context_tier3_equivalence` runs green with the feeder mocks
dropped one-for-one against the real feeders (memories → frozen archive, vault
summaries → recap, mount pool, core-whisper config); a new
`context_feeders_leaves_equivalence` tier-1 differential proves the pure
builders/formatters/config resolvers byte-exact against v4's real exports;
`knowledge_injector` / `first_message_context` / `orchestrator_tier3` re-verified.
Tracked deferral: the orchestrator spine still passes `cheap_llm_selection: None`
into buildContext (it threads only a `cheap_llm_settings_present` bool), so the
recap/distill feeders are gated OFF there and stay mocked in the orchestrator
oracle — closing that is a spine-owner follow-up (thread a resolved
`CheapLlmSelection`); the scene-state job wrapper is W4.8. Full workspace
`cargo test` / `clippy -D warnings` / `fmt --check` green.

Integration of the five parallel wave-4 units (W4.7a / W4.7b / W4.2u / W4.8 /
W4.9b), each developed and verified in isolation. Two reconciliation touches:
the two independent ports of `doc_mount_file_links.findByIdWithContent` were
merged — the job-runner stale-chat sweep keeps the full-`LinkRow` shape as
`find_link_row_by_id`, and the photo tools keep the content-subset
`find_by_id_with_content` (both v4-faithful; a post-port cleanup may unify them);
and the process-global wake-hook unit test's exact-count assertion was relaxed to
monotonic, since the shared `OnceLock` hook is fired by concurrent enqueues from
sibling tests in the larger integrated suite. Full workspace `cargo test` /
`clippy -D warnings` / `fmt --check` green.

Phase 3 — wave 4 (W4.7a): the provider manifest + registry core. Replaced v4's
npm-plugin provider registry — which does not survive the port (no Node, no
dynamic import, no shipping third-party JS into the Rust core) — with a
declarative-manifest + compiled-discriminator design. New `provider_manifest`
module: serde structs for the manifest schema (deserialization is the schema
validation; a missing field, a bad enum, or a wrong `schemaVersion` each fails
loud with a typed `ManifestError` naming the field), the `StreamDecoder` /
`RequestTransform` closed enums (the values W4.7b/c implement against), the nine
built-in provider manifests generated from v4's registered plugin metadata by a
checked-in generator (`harness/oracle/providers/gen-provider-manifests.mjs`,
transcription not re-derivation — embedded via `include_str!`, parsed once behind
a `LazyLock`), the `Registry` accessors reproducing v4's provider-registry
convenience getters (`get_provider` exact-case lookup — v4 does not resolve
`legacyNames`, they are display metadata; the capability getters with their v4
defaults `charsPerToken` 3.5 / `defaultContextWindow` 8192 / `toolFormat`
"openai"), and `rewrite_localhost_url` (pure — the host gateway resolution
injected). Verified by `provider_registry_equivalence` (a tsx oracle driving v4's
real registry over every provider × getter — 253 rows, incl. absent-field
defaults, legacy-name lookups that must not resolve, and a determinism dump) plus
malformed-manifest fail-loud unit tests.

Also closed the four registry-seam replacements in their leaf consumers. The big
one: `message_formatter::get_provider_name_support` now consults the manifest
registry before the legacy fallback, matching v4's `getProviderNameSupport` — a
real behavior change from the pre-W4.7a empty-registry state (DEEPSEEK / Z_AI /
OPENAI_COMPATIBLE now report message name-field support via the registry, where
the legacy table alone said no); its differential regenerated with the real
registry initialized. `model_context`'s registry-default input and `cheap_model`'s
recommended-list / default input keep their injected parameters (the orchestrator
spine populates them), but their oracles were regenerated with the real registry
so the injected values reflect the real manifest data (e.g. ANTHROPIC default
200000, DEEPSEEK/Z_AI 131072); `tool_build`'s `provider_supports_web_search` stays
a corpus-controlled input in its differential. The pins for all four moved to
"the registry value equals the pinned value," asserted in
`provider_registry_equivalence` so a manifest drift is caught there. Spine-side
seam removals (sourcing these injected inputs from the registry at the
orchestrator composition point) are deferred to the orchestrator-spine owner.
Phase 3 — wave 4 (W4.7b): the five stream decoders. Ported the sans-IO
push-state-machine wire decoders that turn a provider's streamed bytes into the
normalized `StreamChunk` sequence, in a new `model::decoders` module: a shared
spec-faithful SSE frame splitter (`sse`) plus `chat_completions_sse`
(openai-compatible / deepseek / z-ai / openrouter — the tool-call accumulator
keyed by `tool_calls[].index`, reasoning routing, usage in the trailing chunk,
`[DONE]`), `responses_api_sse` (openai / grok — the Responses-API event
taxonomy, cumulative reasoning re-sends, terminal `response.completed`),
`anthropic_sse` (`content_block_start`/`delta`/`stop` state machine,
`input_json_delta` per-index buffering, thinking/signature, usage split across
`message_start`/`message_delta`, mid-stream `error` events),
`google_parts` (genai `generateContentStream` — `data:`-SSE parts iteration,
`thought===true` → reasoning, `thoughtSignature`, functionCall parts), and
`ollama_ndjson` (newline-delimited JSON, whole-object tool_calls normalized to
OpenAI shape, `done:true` terminal). Each also assembles the terminal
`rawResponse` value v4 hands back for tool-call detection. `StreamChunk` was NOT
extended. Each decoder is a `StreamDecoder` (`push` / idempotent `finish`)
correct when fed one byte at a time. Verified by `stream_decoders_equivalence`:
a checked-in fetch-mock recorder drives v4's REAL plugin `streamMessage` parsers
over committed wire transcripts and records the normalized chunk NDJSON; the
Rust decoders replay each transcript at whole-buffer / per-frame /
byte-at-a-time and diff the chunk sequence + rawResponse. Two documented
transport-artifact normalizations: google's SDK-injected `sdkHttpResponse` is
stripped, and ollama's no-cross-read-buffer split-line loss (a faithfully ported
v4 bug) is diffed at line-aligned chunkings only (byte-at-a-time bug-parity is a
Rust-side unit test). Three STOP-rule divergences from the design-doc table,
flagged: the four "chat-completions-sse" providers do not share one
normalization (deepseek/z-ai via the OpenAI SDK vs openrouter's raw-fetch
`streamViaChatCompletions`, distinct rawResponse/reasoning shapes; deepseek and
z-ai further differ on cache source + `rawProviderUsage`), reproduced via an
internal `Flavor` selector over one shared parser; google is `data:`-prefixed
SSE, not JSON-array/newline as the table's caption said; and openrouter's
no-tools OpenResponses SDK path is out of scope (a deferred distinct wire).
Phase 3 — wave 4 (W4.2u): danger spine unification. Wired the real
dangerous-content resolver + router into the `process_message` orchestrator
spine, replacing the injected `NoRouter` / hardcoded `DETECT_ONLY` test stub.
The spine now resolves the effective danger settings via
`resolve_dangerous_content_settings` (the global `dangerousContentSettings`
sub-object + the chat's `conciergeOverride` / `chatType` off-duty /
moderation-exempt collapse), computes `is_chat_active_dangerous`, and
reproduces v4 `resolveMessageDangerState`'s first branch: an actively-dangerous,
non-continue turn with content synthesizes danger flags and — under AUTO_ROUTE
with a non-`isDangerousCompatible` profile — reroutes the primary stream through
an uncensored provider via the real `DangerContentRouter` (constructed with its
`ApiKeyResolver` seam), attaching the flags to the saved user message. The
finalizer's danger-classification enqueue now honors the resolver's OFF
short-circuit (`FinalizerChatSettings.danger_mode_off`); the memory-extraction
and danger-classification enqueues use the original `connectionProfile.id`
(distinct from the rerouted `effectiveProfile.id`, added as
`FinalizeOptions.connection_profile_id`), while the persisted assistant message
and cost tracking stay on the effective profile — matching v4. The
classification branch (cheap-LLM / moderation of the current user message) stays
the gatekeeper seam (behavioral no-op on the diffed trace/tables when
not-dangerous). Added two orchestrator-corpus cases driving v4's real danger
resolution: `danger_off_short_circuit` (off-duty chat → resolved OFF → no
classification enqueue, router never consulted) and `danger_live_reroute`
(permanently-dangerous chat + AUTO_ROUTE + uncensored profile → primary stream
rerouted, proven by a distinct recorded canned stream key). The oracle now runs
v4's real `resolveMessageDangerState` (global mode AUTO_ROUTE, no
`uncensoredTextProfileId` so the empty-response failover stays inert) with a
canned `findApiKeyByIdAndUserId` seam. `orchestrator_tier3_equivalence`,
`message_finalizer_tier3_equivalence`, `primary_stream_tier3_equivalence`,
`danger_resolver_equivalence`, `danger_routing_equivalence`, and
`danger_gatekeeper_tier3_equivalence` all green against regenerated oracles; the
pre-existing orchestrator cases are a behavioral no-op under the real resolver.
Phase 3 — wave 4 (W4.8): the background job runner. Ported v4's forked-child
job processor as an in-process runner over the single-writer runtime. The
fork/IPC/buffered-write-proxy architecture does not port — v5's `Db` already
enforces the single-writer invariant in the type system, so job handlers run
in-process and write through `Db` directly. New `services::job_runner`: the
claim-loop core (`pump_claim` with the reentrancy lock, the `maxConcurrentJobs`
instance-settings read each pump [default 4, clamp 1–32], the claim-until-full
loop over the ported `claim_next_job`, and the next-`scheduledAt` wake-delay
decision returned to the host), dispatch by job type through a `HandlerRegistry`
with a loud fallback for unported/unknown types (v4's failure shape),
completion/failure marking (`markCompleted` now wiring the `merge_result_into_payload`
path — closes Phase-2 deferral #3, forward-only since v4-on-SQLite throws
there), and startup/stuck recovery (`reset_orphaned_jobs` / `tick_stuck_reset`).
All timers are host-driver seams (no timers in the runner core), per the enclave
`step()` philosophy. New `services::job_scheduler` with the pure decision leaves
(`clamp_wake_delay`, `should_run_startup_tick`) + the cadence constants. Closed
the `ensureProcessorRunning` seam: `queue_service` enqueues now fire a
process-global wake hook (`set_wake_hook` / `JobRunner::install_wake_hook`); the
runner's `wake()` signals an immediate pump. Extended `queue_service` with the
read/admin surface (`get_job_status` / `get_queue_stats` /
`get_active_counts_by_type` / `cancel_job` / `get_pending_jobs_for_chat` /
`cleanup_old_jobs` / `cleanup_finished_jobs`), the retention windows, and the
portable scheduler sweep bodies (`run_scheduled_housekeeping` /
`run_scheduled_cleanup`). Ported the stale-chat asset maintenance sweep
(`services::maintenance::collapse_stale_chat_assets`, v4
`collapse-stale-chat-assets.ts`) with the new `chats.getLastPlayedMessageAt`
scoped read, the keep-set avatar-sha resolution, and the four protection
branches (current / current-sha / album-or-vault-link / character-reference);
the storage-bytes delete is a host FsSeam. Verified by a tier-1 differential
(`photos_relative_path_equivalence`) and a tsx real-DB tier-2 differential
(`maintenance_sweep_tier2_equivalence`, driving v4's REAL
`collapseStaleChatAssets` over a two-DB fixture), plus eleven runner self-tests
(concurrency cap, wake-on-enqueue, claim-order, loud fallback, stuck/orphan
reset, drain-on-shutdown, and one end-to-end memory-housekeeping dispatch
enqueue→claim→dispatch→markCompleted-merge); the `memory_watermark_tier3` and
`context_summary_service_tier3` differentials regenerated green with the wake
hook (the DB effect is unchanged).
Phase 3 — wave 4 (W4.9b): the photo trio (`keep_image` / `list_images` /
`attach_image`), the last deferred tool handlers, is ported and dispatched.
New `photos` module: `keep_image_markdown` (the kept-image Markdown builder +
parser — YAML frontmatter, prompt/revised-prompt/scene/attribution sections,
the caption regex, slug/filename, `linkedByRole` back-compat), `photos_paths`
(the `photos/` folder helpers), and `save_image_to_album` (resolve the FileEntry
with the mount-blob fallback, dedup by sha within the mount's `photos/` folder,
build the markdown, hard-link the binary, roll up the link's chunk counts). The
three `tools::photo` handlers compose that over the ported vault reads/search,
wired into `BuiltInToolRunner` (removed from the loud fallback) each inside a
both-connections `Db::write` closure. Image bytes stay behind an injected
`FileBytesStore` seam; the mount invalidation + embedding enqueue are recorded
no-op seams; the chunker is not re-ported (chunkCount pinned / doc_mount_chunks
excluded, the groups/projects precedent). Added photo-facing reads
(`files::find_by_id`/`find_by_sha256`, `doc_mount_file_links::find_by_id_with_content`
+ the chunk-rollup setters). Verified by `photo_tools_tier3_equivalence` (a
jest-real-DB oracle driving v4's REAL handlers over a two-DB fixture with baked
photos — keep fresh/duplicate/malformed-scene with six-table dumps, plain +
semantic + peer-vault + silent-fallback listing, attach by link-id/file-id +
cross-vault + missing) and one new `list_images` row in `tool_dispatch`; the
five `doc_*` handler differentials re-verified green.

Phase 3 — wave 4 (W4.d1): drift re-port of the unified diff. v4 commit
`8617ce7a` replaced the greedy look-ahead line diff with a real, minimal,
git-style unified diff, so the ported `doc_edit::unified_diff` no longer
matched. Ported the new v4 `lib/doc-edit/line-diff.ts` as a new leaf
`doc_edit::line_diff` (`diff_lines` — a Myers O(ND) shortest-edit-script diff
over line arrays, a byte-faithful transcription including the exact tie-break
so the recovered op order matches under ties — plus `changed_block_indices`),
and rewrote `doc_edit::unified_diff` on top of it: git-style hunks with three
lines of context, maximal changed runs coalesced when their expanded ranges
touch, correct `@@ -start,count +start,count @@` ranges (count 0 →
`start-1,0`), empty content treated as zero lines, and a whole-file
replacement-hunk fallback past 10,000 combined lines. Deleted the old greedy
walker. Regenerated and extended `doc_edit_leaves_equivalence` (coalesce vs
split hunks, context truncation at file start/end, the formatRange shapes
incl. the delete-at-top/empty-side `0,0` range, create-from-empty and
empty-from-content, a shifted-block case, a Unicode line, the >10,000-line
fallback, plus `diff_lines`/`changed_block_indices` rows driven directly); the
`doc_text` and `doc_fm` handler differentials re-verified green against
regenerated oracles (their handlers do not build the diff payload). No handler
change: the ported doc-edit handlers still omit the `change` payload that
consumes this diff — that seam closes with the Librarian save-announcement
writer in W4.6b.

Phase 3 — the endgame plan. Docs only. Re-planned the remainder of the port
from fresh surveys of every unported v4 subsystem (courier, answer-confirmation,
carina query, file/attachment, the buildContext feeders, the post-office
writers, the job runner, image generation, the photo trio, the provider layer,
the autonomous-room engine). Every remaining unit now has a self-contained work
order under `docs/developer/porting/work-orders/` (W4.2u, W4.3, W4.4a4, W4.4b,
W4.5, W4.6a/b, W4.7a/b, W4.8, W4.9a/b, U4), with the batch table and per-round
parallelism/ownership rules in `chat-orchestration.md`. New docs: the W4.7
provider-layer decomposition (six units, appended to `provider-manifest.md`)
and the enclave (Unit 4) decomposition (`enclave-engine.md`). Key decisions
recorded: the job runner drops v4's fork/IPC/buffered-proxy architecture
(in-process handlers over the single-writer runtime; the autonomous turn keeps
the `write_apply` main-primary batch path), file bytes / image transcode are
injected host seams, the provider core stays sans-IO, and image generation gets
a canned `model::image` seam ahead of the real wire dialects. Also a drift
check of v4 `42242a3e..8617ce7a`: one ported unit is stale —
`doc_edit::unified_diff` (v4 replaced the greedy walker with a Myers line
diff + git-style hunks) — scoped as work order W4.d1, first in Round 1; the
`docs/v4/` CHANGELOG mirror refreshed.

Phase 3 — wave 4 (W4.4a, part 3): the compression cache service. Ported v4's
`compression-cache.service.ts` — `triggerAsyncCompression` /
`getCachedCompression` / `invalidateCompressionCache` (+ `hashString` /
`isCacheValid` / `cacheKey` / the `persistToDatabase` / `loadFromDatabase` /
`clearFromDatabase` DB layer) into `services::compression_cache`. The durable
cache lives in the `chats.compressionCache` column (a JSON object, per-participant
in multi-char chats); a process-global in-memory map is the fast path. Added the
`ChatUpdate.compression_cache` update setter (a JSON `null` clears the column to
SQL NULL, no `updatedAt` bump) and `Deserialize` to `ContextCompressionResult` /
`CompressionDetails`. v4's per-chat promise lock (`withPersistLock`) is not ported
— the single-writer task already serializes the load-modify-save; and there is no
in-flight-promise state (`trigger_async_compression` computes synchronously within
its async fn), so `isFallback` is always false. Verified by
`compression_cache_tier3_equivalence` — a five-op corpus (trigger→persist,
trigger-guard [too few messages], get-DB-hit, get-miss, invalidate) driving v4's
REAL functions, diffing the persisted column (minted `createdAt` normalized) + the
`getCachedCompression` return; the canned cheap-LLM key proves the compression
prompt. The two seam closures — the finalizer's `AsyncCompressionTrigger` real
production impl (needs the trigger inputs — messages / systemPrompt / options —
threaded through the finalizer) and the `buildContext` cached-compression window
(the `cachedCompressionResult` / `cachedCompressionMessageCount` inputs, computed
by the spine via `getCachedCompression`) — are additive spine plumbing tracked as
the remaining part of W4.4a; the differentials keep the recording / empty-cache
seams meanwhile.

Phase 3 — wave 4 (W4.4a, part 2): regenerate-swipe. Ported
`regenerateMessageAsSwipe` (`services::regenerate_swipe`), the sibling entry
point to `processMessage`: it generates an alternative ("swipe") for an existing
ASSISTANT message and persists it as a properly-attributed variant, grouped in
place. Composes the ported services — responder resolution, user identity,
`buildMessageContext` (continue-mode, everything strictly before the target), the
`CompletionProvider` seam for a single non-streaming generation, the swipe-group
bookkeeping on `chat_messages` (write back the original's `swipeGroupId` on the
first regeneration; the new swipe shares the original's `createdAt` +
participant), and the ported `deleteMemoriesBySourceMessageWithVectors` cascade
(gated by the per-user `memoryCascadePreferences.onSwipeRegenerate`). The
orchestrator's `build_context_input` / `BuildContextArgs` were made reusable
(scalar clock/model-limit fields instead of `&ProcessMessageInput`). Verified by
`regenerate_swipe_tier3_equivalence` — a four-case corpus (first regeneration,
existing group, KEEP_MEMORIES, and the not-assistant throw) driving v4's REAL
`regenerateMessageAsSwipe`, diffing `chats` / `chat_messages` / `memories` /
`vector_indices` / `vector_entries` (the canned completion key proves the
rebuilt continue-mode prompt bytes). Tracked deferral: the swipe's
`rawResponse` / `reasoningContent` / `thoughtSignature` are null (the cheap-LLM
`CompletionResponse` subset carries none; the corpus canned response has none, so
null is byte-faithful — the richer wire-decoded response lands with W4.7).

Phase 3 — wave 4 (W4.4a, part 1): the agent-mode resolver. Ported
`resolveAgentModeSetting` (the Global → Character → Project → Chat cascade),
`DEFAULT_AGENT_MODE_SETTINGS`, and `buildAgentModeInstructions` into
`services::agent_mode`, closing the orchestrator's agent-mode seam. The spine now
computes the real resolution: reads the project's `defaultAgentModeEnabled` (a
store-managed field, via the overlaid projects read), resolves the cascade, fires
the `agentTurnCount: 0` reset on a new user turn, feeds `agentMode.enabled` to
`buildTools` (adding `submit_final_response`), injects the agent-mode
system-prompt block into `formattedMessages`, and passes the resolved
`ResolvedAgentMode` to the native loop. The orchestrator tier-3 corpus gained an
`agent_mode_on` case (chat-level opt-in, custom `maxTurns: 15` via settings)
banking the byte-exact instruction injection, the `submit_final_response`
slate addition at the wire, and the turn-count reset (seeded 5 → 0); resolver
unit tests cover the cascade matrix.

Phase 1 — pure-function ports to `quilltap-core`, each with a tier-1 differential
test against the v4 oracle:

- Memory: weighting/decay, ranking blend, recall-tag multipliers, recall-history
  ring buffer.
- Write path: write-batch partitioning, main-primary policy, folder-conflict id
  remap, unique-constraint detection.
- Context: sliding-window compression sizing; per-purpose context-budget
  arithmetic (summarize trigger, recent-message count, max-available, allocation
  split); the summarisation cadence (fold/hard gate, interchange count,
  title-check crossing, turn partition); per-character context shaping
  (history-access gate, presence windows, whisper visibility, role/name
  attribution).
- Enclave: autonomous-run budget verdict and progress-toward-binding-cap, plus
  the per-turn context cap that paces a token-budgeted room across turns
  (`computeAutonomousContextCap` = remaining-budget / turns-left, floored).
- LLM: completion cost estimate, cost-aware model selection, model classes,
  character-based token estimation.
- Turn manager: the turn-state machine — queue ops, history-derived state, and
  the spoken-this-cycle wrap; the all-LLM auto-pause thresholds; the
  participant-list filters (user/LLM/active resolvers); the display-only
  predicted turn order; and the weighted-random next-speaker selection (with the
  RNG injected for determinism).
- Memory name-resolution leaves: reinforced-importance formula, name+pronoun
  formatting, the about/holder name-set builders, and the word-boundary name
  matchers (presence / occurrence-count / about-character resolution) — the
  Unicode-boundary + lookahead regex reproduced without a backtracking engine.
- Embedding: L2 vector normalisation, the profile storage policy (Matryoshka
  truncate + optional normalise), cosine similarity with the dimension-mismatch
  guard and message, the fallback keyword/phrase scorer, the literal-phrase
  boost helpers, Float32 ↔ little-endian-byte BLOB conversion, and the legacy
  JSON-text recovery (`parseLegacyEmbeddingText` — reproducing JS `Object.values`
  ascending integer-key ordering for the index-keyed-object shape).
- Canon: the memory-extraction canon blocks (self / other ALREADY ESTABLISHED
  rendering) and the New-Chat scenario-text combiner.
- Mentioned-character scan: detecting non-participant characters named in a chat
  corpus (ASCII word-boundary alternation, longest-token-first, lowercased
  token→ids map).
- Novel-detail extraction: the deterministic proper-noun / date / currency /
  number-with-unit / CamelCase / acronym scanner (ASCII `\d`/`\b`, the JS `\s`
  whitespace set reproduced exactly, case-insensitive dedup).
- Chat-task text shaping: tool-artifact stripping, visible-conversation
  extraction, and the chat-card preview, over shared JS string primitives (the
  JS `\s`/`trim` set and UTF-16 length/slice).
- Docs: added `docs/developer/porting/phase-2-onramp.md` scoping the tier-2
  DB-state oracle and its fixtures (the next build); cross-linked from the
  porting overview and CLAUDE.md, and marked Phase 1 complete in the roadmap.
- Model context limit: `getModelContextLimit` (+ `hasExtendedContext`,
  `getSafeInputLimit`) — the override / provider-default tables ported as
  constants, with the plugin model-info, `FALLBACK_PRICING` rows, and registry
  default injected; reproduces v4's lookup order and substring matching, and the
  JS-truthy fall-through on a zero/null context value.
- Cheap-model classifiers: `isCheapModel` / `estimateModelCost` /
  `getCheapestModel` and their deprecated fallback tables — the registry-sourced
  recommended-list and default-model are injected (empty / none takes the
  fallback path), the string heuristics (expensive/mid/cheap indicators, the
  dashed-vs-undashed `o1`/`o3` split) are pure.
- Version compare: documented `compareVersions`' `localeCompare` fallback (the
  malformed-input path) as a deferred ICU-collation seam — the parseable
  numeric path stays exact; faithful collation waits on the ICU-crate decision.
- Tool canonicalization: byte-stable `UniversalTool` serialization for
  cache-prefix stability — deep code-unit key-sort of `function.parameters` plus
  the tool-name array sort. The name sort is a documented `localeCompare`
  residual seam (the lowercase snake_case tool-name corpus collates identically
  under code-unit order; the ICU-collation decision is deferred).
- Number formatting: the JS `Number.prototype.toFixed` kernel (V8
  half-away-from-zero rounding on the f64's exact value, via IEEE-754
  mantissa/exponent + u128 — distinct from Rust's half-to-even formatter), and
  the display formatters built on it (`formatBytes`, `formatCostForDisplay`, and
  both the `K` and lowercase-`k` `formatTokenCount` variants).
- Small leaf utilities: chat-type/participant predicates, semver parse/compare,
  pronoun→gender hint, tag-style merge, char-count colour class.

Drift catch-up — v4's answer-confirmation feature (a Salon consistency check +
re-affirmation) added columns/keys to six already-ported marshaling surfaces; this
extends each to match, re-verified byte-exact against v4's current oracle output
(no existing test regressed — the new columns are additive/nullable-default, so
the pre-catch-up corpora still passed unchanged before these edits).

- `chat_settings.answerConfirmationSettings` (global default JSON object,
  `{"enabled":false}`) — a new typed struct in schema position between
  `thinkingDisplay` and `storyBackgroundsSettings`; corpus create/update now set
  it.
- `chats.answerConfirmationOverride` (nullable `'ON'|'OFF'` TEXT, parallel to the
  existing `conciergeOverride`) — wired in both the writer and the read path;
  corpus banks both enum values plus the NULL case.
- `chat_messages`' five new `MessageEvent` fields (`confirmed`,
  `confirmationChecked`, `confirmationRevised`, `confirmationNotes`,
  `confirmationOriginalContent`) — ordinary nullable boolean/string columns
  (INTEGER 0/1, NOT the `isSilentMessage` TEXT-affinity union seam); wired in the
  message insert and the read marshaling, so `updateMessage`'s read-modify-write
  carries them through unchanged. Corpus banks all three badge states (Vouched /
  Stood-by / Amended-with-original-content) across the write and read fixtures.
- `projects` properties.json's `answerConfirmationOverride` (now a 17-key bag) —
  added to `ProjectPropertiesSchema`'s field order and to
  `PROJECT_STORE_MANAGED_FIELDS`; corpus create sets it and the roster
  read-modify-write ops prove it survives untouched.
- `llm_logs`' `ANSWER_CONFIRMATION` enum member — the column is plain TEXT on the
  port side (no code change), so this is corpus-only: one surviving row now
  banks the new value.

Phase 3 — the writer-task runtime (Unit 0) and the model-boundary core (Unit
0.5). Native infrastructure that replaces v4's child-process write machinery, so
verified by self-tests rather than a v4 oracle diff.

- `db::runtime`: `Db`, the `Clone + Send + Sync` handle every service holds — a
  per-partition read pool plus a `tokio::mpsc` write channel that is the only
  mutator. A dedicated OS thread owns the `WriterSet` (main + optional
  mount-index/llm-logs RW writers) and drains the channel serially, so batch
  apply stays serial (the property the folder-conflict remap and main-primary
  ordering assume). A write is a type-erased `FnOnce(&mut WriterSet)` closure
  carrying its own `oneshot` reply; `write_apply` remains available for the
  multi-DB job path, invoked inside a closure. Reads go direct to a pooled
  read-only connection (`PRAGMA key` first-and-only, per the read-path rule).
  API: `write` (async) / `write_blocking` / `read_main` / `read_mount_index` /
  `read_llm_logs`, plus `DbError::{WriterGone, WriterSpawn, PartitionUnavailable}`.
  Four self-tests: 100 concurrent writers serialize with no lost updates,
  read-after-write sees committed state, `write_blocking` commits, and a
  missing-partition read is a clean typed error.
- `model::embedding`: `EmbeddingProvider` (the tier-3 seam mirroring v4's
  `generateEmbeddingForUser`) with `EmbeddingResult` / `EmbeddingError` /
  `EmbeddingPriority`, plus `CannedEmbeddingProvider` — a deterministic responder
  keyed by exact input text (fixed vector; explicit failures for
  `SKIP_EMBEDDING_FAILED`; an unregistered input errors rather than answering).
  Async and generic (no trait object), three self-tests. The v4-oracle-side
  canned injection lands with Unit 1's memory-gate differential.
- Added `tokio` (`sync` only in the library — the writer is a plain OS thread, so
  no scheduler is pulled into the core; `macros`/`rt-multi-thread` dev-only).
- Docs: CLAUDE.md's "Never accept unverified Rust" corrected — `cargo
  build`/`test`/`clippy` do run in this environment and should be run before
  presenting Rust as done; the real-instance open + oracle diff remain the proof
  for crypto/cipher. Status sections (CLAUDE.md, `overview.md`, `phase-3.md`)
  updated for Units 0 and 0.5.

Phase 3 — the **memory gate** (Unit 1), the first decision service. Ported v4's
`createMemoryWithGate` / `runMemoryGate`, verified the new tier-3 → tier-2 way (a
canned embedding injected identically on both sides, then a structural DB diff).

- `services::memory_gate`: the pre-write similarity gate — `INSERT` /
  `INSERT_RELATED` / `REINFORCE` / `SKIP_NEAR_DUPLICATE` / `SKIP_EMBEDDING_FAILED`
  by cosine band (`NEAR_DUPLICATE_THRESHOLD` 0.90 / `MERGE_THRESHOLD` 0.85 /
  `RELATED_THRESHOLD` 0.70; the stale v4 header comment ignored). Async, generic
  over an `EmbeddingProvider`, reading off the read pool and funnelling every
  mutation through the writer thread — the first service to drive the whole Unit-0
  write path end to end. Reinforcement re-extracts novel details, appends
  footnotes, bumps count/importance, and re-embeds on a content change; related
  inserts bidirectionally link. Deferred (tracked): `maybeEnqueueHousekeeping`,
  the `skipGate` direct path, `applyNamePresenceCheck`'s cross-character lookup,
  and the 500 ms inter-retry delay.
- `db::vector_store`: the in-memory `CharacterVectorStore` shim (v4
  `vector-store.ts`) — load off a read connection, linear cosine top-K (stable
  descending, dimension guard), and an incremental flush (add/update/saveMeta)
  through the writer.
- `db::memories::MemUpdate` gained `embedding` (the `Some`-gated BLOB setter the
  gate writes through) and `related_memory_ids` setters; `dump_table_json_conn`
  lets the harness snapshot a table off a read-only pooled connection after a
  service commits.
- Differential: a tier-3 oracle drives v4's REAL `createMemoryWithGate` under jest
  (mocking only `generateEmbeddingForUser`, with the real cipher binding wired in
  via `better-sqlite3-multiple-ciphers`) over a seven-scenario corpus — one per
  outcome, each on its own character — and the Rust gate is diffed across
  `memories` + `vector_indices` + `vector_entries` in the shared-cross-table
  id-map remap form. Four core self-tests exercise the outcomes over an in-memory
  `Db` + canned provider.

Phase 3 — the memory deletion chokepoint (the first memory-family follow-on).
Ported v4's `deleteMemoryWithUnlink` / `deleteMemoriesWithUnlinkBatch` (memory-gate.ts)
as `MemoriesRepository::delete_with_unlink` / `delete_many_with_unlink` — the single
point every cascade (housekeeping sweeps, chat-wipe, swipe-group cleanup) deletes
through, so a removed id never lingers in another memory's `relatedMemoryIds`.

- `delete_with_unlink`: `LIKE '%"<id>"%'` neighbour pre-filter, per-neighbour
  character-scoped `relatedMemoryIds` rewrite, then the row delete. Idempotent — a
  missing row returns false without touching neighbours.
- `delete_many_with_unlink`: one-pass scan of every row with a non-empty links
  array, scrubs every doomed id from each neighbour in one update, then deletes the
  doomed set grouped by character (`bulkDelete` is characterId-scoped). Empty → 0.
- Differential: a tsx real-DB oracle drives v4's REAL chokepoint over a pre-seeded
  nine-memory graph (cross-linked across two characters), and the `memories` dump
  is diffed in the sentinel-aware minted-`updatedAt` form (an untouched row stays at
  the seed sentinel — proving no stray bump). Four repo self-tests cover the
  single/batch scrub, the missing-row no-op, and the empty batch.

Phase 3 — the memory-service cascade-delete family (the second memory-family
follow-on). Ported v4's `deleteMemoryWithVector` and the three
`deleteMemoriesBy*WithVectors` cascades (memory-service.ts) as
`services::memory_service` — the vector-store-aware wrappers around the deletion
chokepoint that every bulk delete path (single UI delete, source-message cascade,
swipe-group cascade, chat wipe) goes through.

- `services::memory_service`: `delete_memory_with_vector` (ownership check before
  the characterId-agnostic chokepoint; non-fatal vector cleanup after a
  successful delete), `delete_memories_by_source_message_with_vectors`,
  `delete_memories_by_source_messages_with_vectors` (gathers the whole swipe
  group up front so the neighbour scan sweeps once), and
  `delete_memories_by_chat_id_with_vectors` (adds `characterCount`). Cascades
  group the doomed set by character in first-appearance order, count only vectors
  the store actually held (`hasVector` first), guard each character's cleanup
  non-fatally, then batch-delete through the chokepoint. Three self-tests.
- `db::vector_store::CharacterVectorStore::remove_vector` (v4 `removeVector`):
  un-adds a same-flush add, otherwise tracks the id for deletion and drops any
  pending update; a store whose sweep removed nothing flushes as a no-op, so its
  `vector_indices.updatedAt` is not bumped. Three unit tests.
- Differential (`memory_cascade_tier2_equivalence`): a tsx real-DB oracle drives
  v4's REAL memory-service over an 8-op sequence on an 11-memory / 6-character
  fixture (cross-character links, two vector-less memories, one entry-less
  store), asserting each op's return against the spec on both sides, then diffing
  `memories` + `vector_indices` + `vector_entries` in the sentinel-aware
  minted-`updatedAt` form — the untouched stores' metadata provably keeps the
  seed sentinel.

Phase 3 — memory housekeeping (the third memory-family follow-on). Ported v4's
`runHousekeeping` / `getHousekeepingPreview` / `needsHousekeeping`
(housekeeping.ts) as `services::housekeeping` — the retention sweep the
`MEMORY_HOUSEKEEPING` job runs. No model call: the merge pass searches the
already-stored vector index against itself.

- `services::housekeeping`: three passes then a gated apply. (1) Retention —
  MANUAL is a hard protection override, otherwise the blended
  `calculate_protection_score` >= 0.5 protects; an unprotected memory goes only
  when below the importance floor AND old AND inactive. (2) Opt-in similarity
  merge over stored vectors (>= threshold folds into the more-important/newer
  survivor; the merge pass does not consult protection — faithful to v4).
  (3) Cap enforcement deletes the lowest-effective-weight unprotected memories
  from the tail, with v4's all-protected pre-check. Apply deletes through the
  chokepoint then cleans the vector store non-fatally; `dry_run` reports without
  writing. Detail reasons formatted with the ported JS `toFixed` so they match
  v4 byte-for-byte at equal wall clock. Three self-tests.
- `clock` gained `now_unix_ms` and `iso_to_ms` (the strict inverse of
  `iso_from_unix_ms`, matching JS `Date.parse` on the repo-minted shape);
  `CharacterVectorStore` gained `all_entries` (v4 `getAllEntries`, load order).
- Differential (`memory_housekeeping_tier2_equivalence`): a tsx real-DB oracle
  drives v4's REAL housekeeping over a 6-op sequence (dry-run, retention sweep,
  merge sweep, cap sweep, both `needsHousekeeping` branches) on a 15-memory /
  3-character fixture, then BOTH the per-op result objects (counts, id lists,
  details — age/inactive month numbers placeholdered, being wall-clock-derived)
  and the three table dumps (sentinel-aware minted-`updatedAt`) are diffed.
  Corpus-freshness note recorded: the "recent" seed dates age past the 6-month
  windows ~2026-12; refresh them when regenerating after that.

Phase 3 — the completion half of the model boundary (`model::completion`),
mirroring `model::embedding`'s shape. Native tier-3 infrastructure (like Unit
0.5), so verified by self-tests; the v4-oracle-side canned injection lands with
the memory-processor differential.

- `model::completion`: `CompletionProvider` — the seam every completion call
  goes through, sitting at v4's `provider.sendMessage(params, apiKey)` (the
  `LLMParams`/`LLMResponse` subset the cheap-LLM path consumes: role+content
  messages, model, optional temperature, maxTokens, strictMaxTokens, cacheKey,
  profileParameters). Everything above the seam (the temperature fallback, the
  uncensored-provider retry, response parsing) is ported orchestration that must
  sit inside the differential; API-key acquisition stays host-side.
- `CannedCompletionProvider`: a deterministic responder keyed by the exact call
  input (`canned_completion_key` = provider | model | temperature-or-`-` | the
  `[{role, content}]` JSON) → fixed response text + token usage. Unregistered
  input errors rather than answering; failure entries carry their exact error
  message so message-inspecting fallbacks can be driven deterministically. Five
  self-tests (incl. temperature-presence and provider/model key separation, the
  two fallback paths' key shapes).

Phase 3 — the memory-extraction pure leaves (`memory_tasks`), the tier-1 half
of the memory-processor unit. Ported from v4
`lib/memory/cheap-llm-tasks/memory-tasks.ts`.

- `memory_tasks`: the SELF/OTHER extraction prompt builders
  (`get_self_memory_extraction_prompt` / `get_other_memory_extraction_prompt` —
  the byte-stable bodies, the first-person-user and autonomous-room preambles,
  the ORIENTING CONTEXT footer with its 1500-UTF-16-unit truncation, the
  numbered multi-subject CONTEXT footer), the shared turn-context renderer
  (`render_turn_context` — roster branches, the user-controlled-slice
  single-rendering rule, the standalone-opener fallback), the message builders
  (`build_self_extraction_messages` / `build_other_extraction_messages`, `None`
  = v4's no-slice/no-subjects early return), and the response parsers
  (`parse_memory_candidate_array` / `parse_other_candidates_by_subject` /
  `coerce_memory_candidate` / `apply_targeting_tags` — fence stripping, closed-
  vocabulary tag validation with present/wide/information defaults, JS-truthy
  content/summary coercion via `JSON.stringify`, `HARD_CANDIDATE_CAP` = 2, the
  per-subject and total caps, JS `Number.isInteger` subjectIndex semantics, and
  the null-item TypeError that empties the whole SELF array). `importance` is
  kept as the raw JSON number so integer emissions re-serialize bare.
- The big prompt bodies live in a **generated** submodule
  (`memory_tasks/prompt_text.rs`), extracted mechanically from the v4 source —
  no hand transcription. Also hosts `strip_code_fences` (v4 keeps it in
  `ai-import.service.ts`); `jsstr` gained `js_trim_end`; the `recall_tags`
  closed-vocabulary parsers (`from_kw`) went public for the extraction side.
- Differential (`memory_tasks_equivalence`): a jest oracle (the seam is a
  module export only `jest.mock` can replace — the same seam v4's own
  extraction tests use) drives v4's REAL `extractSelfMemoriesFromTurn` /
  `extractOtherMemoriesFromTurn` over a committed 14-case corpus with ONLY
  `executeCheapLLMTask` mocked, capturing the built messages byte-for-byte and
  feeding each case's canned response text into the real parser. Four
  self-tests on top.

Phase 3 — the **memory processor** (`services::memory_processor`, v4
`processTurnForMemory`), the model-dependent per-turn extraction service — the
first tier-3 differential to pin BOTH model boundaries (completion +
embedding). Also closes the memory gate's `applyNamePresenceCheck` deferral.

- `cheap_llm`: v4 `lib/llm/cheap-llm.ts`'s pure selection logic —
  `get_cheap_llm_provider` (the five-priority order: global default cheap
  profile, USER_DEFINED, any `isCheap` profile local-preferred, local-first
  Ollama, current-provider-cheapest fallback, with the registry seam injected
  as in `cheap_model`) and `resolve_uncensored_cheap_llm_selection` (dangerous
  chats swap to the configured uncensored profile, then any
  dangerous-compatible one, else fail open). Plus `build_character_cache_key`
  (v4 `lib/llm/cache-key.ts`) and the `CheapLlmProfile` / `CheapLlmSelection` /
  `DangerousContentSettings` / `UncensoredFallbackOptions` types. Three
  self-tests.
- `services::cheap_llm_exec`: v4 `core-execution.ts`'s pipeline —
  `CheapLlmTaskExecutor` holds the session-level no-custom-temperature cache
  (v4's module-global `profilesWithoutCustomTemp`, instance state here); the
  0.3-temperature first try with the message-inspecting retry-without-
  temperature; the strict 2048 max-tokens floor; the uncensored-provider retry
  on empty responses (`should_attempt_uncensored_fallback`, incl. the exact
  both-providers-empty error string); parse-and-wrap into
  `CheapLlmTaskResult`. **Deferred (tracked):** API-key acquisition (host-side;
  the boundary starts at the provider call) and the fire-and-forget
  `logLLMCall` llm-logs write. Two self-tests.
- `services::memory_processor`: the orchestration — per-character rate limits
  (`countCreatedSince` over the last wall-clock hour; skip at the cap,
  throttle past the soft-start fraction with the importance floor), the
  once-per-turn selection resolve, the SELF pass (own-fields canon) and the
  multi-subject OTHER pass (canon from the observer's vault
  `Others/<subject>.md` via the new `read_vault_text_file` +
  `load_canon_for_observer_about_subject`, falling back identity →
  description → none), dry-run collection, and every candidate written through
  the ported memory gate with the per-outcome debug lines reproduced
  byte-for-byte (JS number interpolation, `toFixed(3)` similarity,
  `${undefined}` semantics).
- Memory gate: the `applyNamePresenceCheck` **lookup branch is now ported**
  (deferral closed) — a cross-character AUTO proposal reads both characters
  through the vault-overlaid `characters_read::find_by_id` and resolves via the
  Phase-1 `resolve_about_character_id`, collapsing a mis-attributed
  about-target to a self-reference; any lookup failure passes through
  unchanged (v4's never-block-a-write catch). `MemoryGateOutcome` gained
  `reinforcement_count` (the extraction driver's debug line reads it).
- Differential (`memory_processor_tier3_equivalence`): a jest oracle drives
  v4's REAL `processTurnForMemory` over a two-database fixture (characters
  with real vaults + a seeded `Others/Charlie.md`, gate-band vector seeds, and
  future-dated rate-limit ballast — a 2099 `createdAt` is always "in the last
  hour", so counts are wall-clock-proof) with only the model/infra seams
  stubbed. The completion mock resolves calls by (pass, CONTEXT-footer label,
  model, autonomous-clause) rules and RECORDS each exact
  `provider|model|temperature|messages` canned key; the Rust side replays
  those entries through `CannedCompletionProvider`, so any prompt/selection
  divergence surfaces as a canned-miss. Three calls (a full mixed turn, an
  autonomous dangerous dry run, an empty turn) banking: throttle drops +
  skip/duplicate-user logs, all five gate outcomes (incl. the uncensored
  fallback feeding SKIP_EMBEDDING_FAILED), all four canon sources, the
  name-presence flip, sourceMessageTimestamp pinning, and usage aggregation
  (the discarded empty-response usage included). Result objects (debug logs
  byte-for-byte) AND the three tables (shared-id-map remap form) are diffed;
  the memory-gate differential re-verified green after the gate change.

Phase 3 — the memory gate's **watermark auto-housekeeping check** (v4
`maybeEnqueueHousekeeping`), closing the gate's last write-side deferral.
After an INSERT / INSERT_RELATED the gate now checks whether the character has
reached the watermark fraction (0.9) of its auto-housekeeping cap and, if so,
enqueues a `MEMORY_HOUSEKEEPING` background job — unless backed off.

- `services::queue_service`: the `enqueueJob` + `enqueueMemoryHousekeeping`
  slice of v4's queue service — mint a PENDING `background_jobs` row; the
  housekeeping variant de-dupes against in-flight (PENDING/PROCESSING) jobs
  for the same (userId, characterId) and caps attempts at 1 (retry-hostile).
  **Deferred:** `ensureProcessorRunning` (the job runner is a later unit; the
  oracle pins v4's auto-start to a no-op to match).
- `services::housekeeping_outcome_cache`: v4's in-memory ineffective-sweep
  back-off. **Rust home decision:** v4 holds it as a module-global Map; the
  port keeps the same process-global shape (`OnceLock<Mutex<HashMap>>`),
  keyed by characterId. One self-test.
- The gate's `maybe_enqueue_housekeeping`: enabled-settings gate (via a new
  scoped `chat_settings::find_auto_housekeeping_settings_by_user_id` read —
  the full `findByUserId` marshaling remains a later chat-settings read
  sub-unit), the `perCharacterCapOverrides ?? perCharacterCap ?? 2000` cap
  resolution, the post-write count vs `floor(cap × 0.9)`, the in-memory
  back-off, and the durable 15-minute throttle over
  `findRecentByType('MEMORY_HOUSEKEEPING', 50)`. Never propagates an error
  (v4's catch); the port awaits the call v4 `void`s — same DB effect once
  settled, no detached-task machinery in the core.
- Differential (`memory_watermark_tier3_equivalence`): seven
  `createMemoryWithGate` INSERTs over a seeded fixture (settings rows,
  watermark-exact memory ballast, a future-`updatedAt` COMPLETED sweep and a
  PENDING dedupe target — future timestamps make the wall-clock windows
  deterministic) banking: a real enqueue, below-watermark, the override
  raise, disabled settings, the durable throttle, the in-flight dedupe, and
  the in-memory back-off (both sides record the same outcome through their
  real cache first). Four tables diffed; the memory-gate and memory-processor
  differentials re-verified green with the watermark path live.

Phase 3 — chat orchestration (Unit 3) started: the decomposition doc plus
waves 1–2, ported in parallel (six pure-leaf agents, then three composed
units), each with its own fresh-oracle differential.

- Added `docs/developer/porting/chat-orchestration.md`: the survey of v4's
  send-message engine (`lib/services/chat-message/`, `buildContext`, the
  stateful turn chain), the SSE event vocabulary → `Event`-channel mapping, and
  the four-wave leaf-first decomposition with per-unit verification plans.
- Template processor (`templates`): `processTemplate` / `buildTemplateContext`
  / `processCharacterTemplates` — ASCII-`\w` token rule, single-pass
  non-recursive replacement, and the two-pass `{{trim}}` quirk (the paired
  macro can never fire) ported faithfully. Turn-predicate gap closed
  (`is_users_turn` / `is_participants_turn` / `get_selection_explanation`).
- Chat timestamps (`chat_timestamp`): timezone resolution, real/fictional
  timestamp calculation (clock injected), injection cadence, system-prompt
  formatting. Added `jiff` (pinned) for the IANA UTC-offset lookup — proven
  byte-exact against `Intl.DateTimeFormat` across both US DST boundaries,
  fractional-offset zones, and the invalid-zone throw; v4's CUSTOM-token
  sequential-replace bug reproduced. Plus the formatting prompt hint
  (`template_prompt_hint`).
- Memory-injector formatters (`memory_injector`): metadata tag, scene state
  (sceneHash + `_unchanged_` compaction), memory/inter-character/frozen-archive
  /dynamic-head/summary blocks — sort stability, insertion-order maps, and
  UTF-16 slicing all byte-exact.
- Message selector (`message_selector`, the greedy tail fit) and the
  core-whisper cadence gate (`core_whisper`).
- Carina markup parser (`carina_parser`): JS-dot / ASCII-`\w` / smart-quote
  pairing semantics.
- Message formatter (`message_formatter`): the anti-hijack cleanups
  (name-prefix strip, foreign-speaker truncation, content-block normalization)
  and provider name-field helpers; finish-reason extraction (`finish_reason`).
- System-prompt builder (`system_prompt`): identity stack, public identity
  card, other-participants info, identity reinforcement, `buildSystemPrompt` —
  composed over `templates` + `chat_timestamp`.
- Stateful turn-orchestration decision core (`services::turn_orchestrator`):
  `should_chain_next` (guard chain, all-LLM auto-pause write, turn-queue pop +
  write-back, weighted selection with injected RNG), `persist_turn_participant_id`,
  and the turn-action mutation core (nudge/queue/dequeue/skipUserTurn/query).
  `ChatUpdate` gained `turn_queue` + nullable `last_turn_participant_id`
  setters. Verified by a 13-op tsx real-DB tier-2 differential (two-DB seeded
  fixture, zero normalization).
- Streaming model boundary (`model::stream`): `StreamChunk` (v4's normalized
  chunk vocabulary — the target for the future manifest stream decoders),
  `StreamingCompletionProvider`, and `CannedStreamingProvider` with
  first-class mid-stream failures; oracle-side injection lands with the
  wave-3 primary-stream differential.
- Eleven new tier-1 oracle cases + the turn-orchestrator tier-2 case/fixture;
  the `chats` tier-2 differential re-verified green with the new setters.

Phase 3 — chat orchestration wave 3, batch 1: the seven mutually-independent
model-calling/DB-reading services, ported in parallel (six agents on disjoint
files; shared `ChatUpdate` setters + `services/mod.rs` pre-staged serially),
each with its own fresh-oracle differential.

- Compression service half (`services::compression`): `applyContextCompression`
  + `compressConversationHistory` over the ported sizing leaves and the
  cheap-LLM executor; system-prompt compression stays disabled (result shape
  matched, dead path not ported). Result-object tier-3 differential, 6 cases.
- Context-summary service half (`services::context_summary`):
  `generateContextSummary` / `invalidateContextSummaryIfMessageCovered` /
  `checkAndGenerateSummaryIfNeeded` + `foldChatSummary` and both title
  generators; the prior-generation Librarian-whisper sweep ported;
  `queue_service` gained `enqueue_title_update`. Librarian re-post, vault
  mirror, relevant-conversations refresh, and cost events deferred behind a
  no-op `ContextSummarySeams` trait (oracle mocks match). 11-op tier-3
  differential over `chats` + `chat_messages` + `background_jobs`.
- Knowledge injector (`services::knowledge_injector`) with
  `search_document_chunks` and the qtap-uri/tier-dedupe leaves; first-message
  context (`services::first_message_context`) with
  `memory_service::search_memories_semantic` (recallContext re-rank deferred).
  Two read-differentials, zero normalization, embeddings canned both sides.
- Participant resolver (`services::participant_resolver`, incl.
  `resolveConnectionProfile`) and user-identity resolver
  (`services::user_identity_resolver`); scoped reads added to
  `connection_profiles` / `roleplay_templates` / `users`; the inherited
  roleplay template persists via the new `ChatUpdate.roleplay_template_id`
  setter. API-key acquisition stays host-side. Two tsx real-DB differentials.
- Primary stream / recovery / provider failover (`services::primary_stream`,
  `services::recovery`, `services::provider_failover`) over `model::stream`,
  with the first typed event vocabulary (`services::chat_events`: `ChatEvent`
  + `EventSink`, byte-identical to v4's SSE frames) and
  `save_assistant_message` as the shared persistence primitive; the
  `lib/llm/errors.ts` classifiers ported. 12-call tier-3 differential diffing
  the ordered event trace, both table dumps, and result objects.
- Carina markup runner (`services::carina_runner` + the `postCarinaResponse`
  writer). `runCarinaQuery` established as an injected seam (it requires the
  wave-4 tool subsystem and other unported services); Prospero error-posting
  behind a recorded seam. 7-case tier-3 differential.
- `ChatUpdate` gained the summary-counter/anchor/title-watermark and
  `roleplay_template_id` setters; `chats_tier2` and `turn_orchestrator`
  differentials re-verified green against regenerated oracles.

Phase 3 — chat orchestration wave 3, batch 2: the message finalizer and the
`buildContext` capstone, ported in parallel, each with a tier-3 differential.

- Message finalizer (`services::message_finalizer`): `finalizeMessageResponse`
  + `calculateNextSpeaker` — the core clean → re-base → persist → carina →
  next-speaker → done-event → background-triggers path. The tool /
  answer-confirmation / async-compression / RNG / cost-estimation subsystems
  are injected seams with their gate conditions reproduced and banked;
  `save_assistant_message` extended (confirmation bag, isSilentMessage, image
  links via the new `db::files::add_link`); `chat_events` gained the full done
  payload plus `CarinaAnswer`/`ConfirmationResult` variants (recovery frames
  unchanged — primary-stream differential re-verified); `queue_service` gained
  `enqueue_memory_extraction` + `enqueue_chat_danger_classification`. Ten-call
  tier-3 differential diffing results, ordered event traces, seam records, and
  `chats`/`chat_messages`/`background_jobs`/`files`.
- `buildContext` capstone (`services::build_context`): the full context
  assembler composed from the ported subsystem (system prompt, budgets,
  phase-1 compression, two-pool memory retrieval, scene state, inter-character
  memories, knowledge retrieval, summary anchor + Librarian cache breakpoint,
  attribution/whisper shaping, timestamps, the Commonplace recall fold).
  Unported feeders and whisper-posting side effects behind a
  `BuildContextSeams` trait mirrored by the oracle mocks. Seven-op tier-3
  differential diffing the full `BuiltContext` byte-for-byte (frozen wall
  clock both sides).
- Remaining wave-3 unit: `processMessage` + `executeTurnChain` (also picks up
  the finalizer's deferred summary-check invocation and buildContext's
  autonomous-cap plumbing).

Phase 3 — chat orchestration wave 3 capstone: the `processMessage` spine +
`executeTurnChain` (`services::orchestrator`), completing the planned wave-3
roadmap.

- Composes every landed wave-1..3 service into the full user-message →
  assistant-response cycle; the finalizer's deferred summary-check invocation
  is closed here (wired where v4 wires it). `chat_events` gained the
  `turnStart`/`turnComplete`/`chainComplete` frames and the empty-response
  done fields. Unported subsystems (attachments, tools, agent mode, danger
  reroute, courier, RNG, prospero cadence) are `OrchestratorSeams` with their
  v4 gates reproduced and banked inactive.
- First end-to-end tier-3 differential: six cases (full single turn,
  continue-mode, empty-response retry, mid-stream preserve-partial, a real
  summary fold, a multi-character chain) driving v4's real send path with
  frozen clock/RNG; ordered event trace + chats/chat_messages/background_jobs
  diffed; message-finalizer and primary-stream differentials re-verified.
- Discovered and documented: v4's `buildMessageContext` wrapper
  (context-builder.service.ts) is not yet ported (reduced to a passthrough on
  both differential sides) — the remaining orchestrator-family unit; and a
  chain-depth divergence on non-continue single-LLM-character chats is
  flagged for a dedicated follow-up corpus.

Drift check — v4 `8efe1ba9..f69200bb` (17 commits) audited against the ported
surface; no ported unit is stale. Docs only, no crate source changed.

- Confirmed in the port already: the `profileParameters` forwarding fix
  (`8cf7272e`) and the answer-confirmation service halves (`29f3ae63` — the
  finalizer gates + the `confirmationResult` event) landed inside the wave-3
  ports; corrected the stale CLAUDE.md note that called the forwarding fix
  unported.
- v4's jest-config change (`69fa611e` — `.integration.test` files excluded from
  unit runs; `better-sqlite3-multiple-ciphers` now mapped to the DB mock)
  verified harmless to the oracle machinery by regenerating the memory-gate
  oracle under the new config and re-running its differential green.
- New unported v4 surfaces recorded in the plans: the anthropic
  adaptive-thinking / sampling-param-rejection rules (`provider-manifest.md`),
  the wardrobe transfers endpoint + public READ trio as archetype-tier
  consumers (`overview.md`), and server-side markdown rendering +
  `qtap-linkify` (with its lookbehind-regex porting note) plus the expanded
  answer-confirmation unit in `chat-orchestration.md`'s wave-4 list.
- Refreshed the `docs/v4/` mirror (CHANGELOG, DDL.md, the answer-confirmation
  feature doc).

Phase 3 — chat orchestration: the chain-depth divergence resolved and the
`buildMessageContext` wrapper ported, closing the two orchestrator-family open
items the wave-3 capstone flagged.

- Chain-depth divergence investigated and resolved as an oracle-harness
  artifact, not a v5 bug: the differential's oracle froze `Date.now()`, so
  identical `createdAt` values let `getMessages`' `ORDER BY createdAt ASC`
  tie-break the non-continue user row after the assistant replies, flipping
  `calculateTurnStateFromHistory`'s `lastSpeakerId` to the user and re-picking
  the sole LLM character to max depth. The Rust spine stamps `createdAt` from a
  real monotonic clock, so it correctly stops at `user_turn`; proven by ticking
  the oracle clock +1ms/read (v4 then also stops at `user_turn`). Fix: the
  orchestrator oracle clock advances 1ms per read, the differential now diffs
  `spokenThisCycleParticipantIds`/`turnQueue`/`lastTurnParticipantId` exactly
  (previously placeholdered) with the job-payload anchor ids remapped through
  the shared message idmap, and two chain-depth cases were added
  (`noncontinue_single_user_chain` → `user_turn`; `noncontinue_two_llm_maxdepth`
  → genuine `max_depth`).
- `buildMessageContext` wrapper ported (`services::message_context`, v4
  `context-builder.service.ts`), leaf-first. Three pure leaves ride a tier-1
  differential (`message_context_leaves_equivalence`): `buildConversationMessages`
  (type/role filter, `assistantAfter` reverse pass, TOOL-result render with the
  `>3`-turn elision), `normalizeWhisperRoles` (Staff→USER re-role, opaque-body
  swap, attachment-bearing exemption), and `collectLanternImageFileIdsForCharacter`
  (own-turn-stop walk, history cutoff, dedup, lookback cap). The composition runs
  the A–D whisper pre-filters (commonplace strip + relevant-conversations
  exception; TOOL-whisper target filtering; opaque-anywhere over LLM participants'
  `systemTransparency`; whisper re-role), `buildConversationMessages`, the ported
  `buildContext`, `formatMessagesForProvider`, the Lantern merge, trailing-prefix
  injection, and the multi-character scene block (Anthropic system-instruction
  route vs non-Anthropic `[Name]` prefill). Wired into the orchestrator spine
  where the direct `build_context` call sat, so `formatMessagesForProvider` + the
  scene block now reach the wire. The K file-loading half is the injected
  `MessageContextSeams` (wave-4 file subsystem); the id-collection leaf is
  exercised.
- Orchestrator oracle rebuilt to drive v4's REAL `buildMessageContext` (the
  passthrough mock dropped; only the K file-loader mocked, mirroring the Rust
  seam). Every corpus chat is multi-character, so the scene block + name
  prefixing apply throughout (changing the canned stream keys, re-recorded and
  reproduced byte-for-byte). Five cases added: `nonanthropic_scene`,
  `commonplace_strip`, `opaque_swap` vs `transparent_no_swap`, and
  `tool_whisper_filter`. `orchestrator_tier3_equivalence` re-verified green.

Phase 3 — wave 4 (W4.2): the dangerous-content ("Concierge") orchestration
subsystem (`services::dangerous_content`), replacing the injected
`DangerousContentRouter` stub with the real resolution. Ported v4's
`lib/services/dangerous-content/` + the `CHAT_DANGER_CLASSIFICATION` job runner:

- `chat_override` — the two-field danger-status derivation (`isConciergeOffDuty`
  / `getConciergeState` / `isChatActiveDangerous`; off-duty preserves the label,
  wins over the classification).
- `resolver` — `resolveDangerousContentSettings` (global + per-chat off-duty /
  moderation-exempt short-circuits; the DEFAULT / OFF_DUTY constant shapes).
- `gatekeeper` — content classification: the moderation-provider path (an
  injected `ModerationProvider` seam collapsing v4's plugin registry +
  `autoDetectModerationApiKey` + `provider.moderate`; the port still runs the
  ported `mapModerationResult` over the raw result), the cheap-LLM classify path
  (the byte-exact `CLASSIFICATION_SYSTEM_PROMPT` in a generated `prompt_text`
  submodule, temperature 0.1 / maxTokens 500, over the `CompletionProvider`
  seam), `parseClassificationResponse`, `CATEGORY_LABELS` /
  `MODERATION_CATEGORY_MAP`, and the module-global classification LRU cache.
- `provider_routing` — the REAL implementor of the frozen
  `DangerousContentRouter` seam: `resolveProviderForDangerousContent` +
  `resolveImageProviderForDangerousContent` +
  `resolveUncensoredImageProfileForReroute` + `isImageModerationError` (the five
  exact reason strings preserved). API-key material stays host-side (an injected
  `ApiKeyResolver` seam); `DangerContentRouter` maps the resolution into the
  failover's `RouteResult`. Added additive `connection_profiles` / `image_profiles`
  `find_by_id` / `find_all` / `find_by_user_id` net reads.
- `manual_flip` — `applyConciergeFlip` (the tri-state operator flip; raw
  multi-column chat `UPDATE` that mints no `updatedAt`, byte-identical to v4's
  `chats.update` — the frozen `ChatUpdate` is owned by the parallel W4.4a batch).
- `gatekeeper_job` — `handleChatDangerClassification` (the job runner): the
  sticky/exempt/off-duty/mode-OFF bails, the context-summary-else-concatenated-
  messages classification input, the cheap-LLM selection, the classify call, the
  `DANGER_CLASSIFICATION` system event + token aggregate (only on the LLM path,
  which mints `updatedAt`), and the chat-level danger-field persistence.

Three differentials, all green against v4 HEAD: `danger_resolver_equivalence`
(tier-1 resolver + override matrix, plus a tier-2 manual-flip chat-row dump),
`danger_routing_equivalence` (the reroute matrix — decision + profile identity +
resolved key + exact reason, canned api-key seam both sides), and
`danger_gatekeeper_tier3_equivalence` (drives v4's REAL job runner over a seeded
fixture — safe/dangerous/borderline/parse-failure LLM classifications, the
moderation-provider path incl. a provider failure, the system-event + chat writes
diffed sentinel-aware). Seams (tracked deferrals): the moderation plugin registry,
the cheap-LLM / routing API-key acquisition, `logLLMCall`, the job-runner
infrastructure, and the Concierge personified-announcement writers (W4.6).
Spine integration — constructing the real router/gatekeeper at the orchestrator
composition point + the OFF-short-circuit / live-reroute orchestrator-corpus
cases — is deferred to unification (it edits W4.4a-owned files).

Phase 3 — wave 4 (W4.1g): `buildTools` + the tool-slate spine wiring (closes
W4.1). Ported v4's `buildTools` + the built-in half of `buildToolsForProvider`
(`services::tool_build`): the flag→tool-set construction over the b.3 definition
catalog, the individual disabled-tool filter, the `allowToolUse === false` and
`disabledTools === undefined` short-circuits, and the canonical (universal/OpenAI)
provider shape. `checkModelSupportsTools` + `provider.supportsWebSearch` are
injected registry-seam inputs (the `getModelContextLimit` precedent); the plugin
tool registry, the provider `formatTools` reshape, and image-provider constraint
enrichment are documented W4.7 deferrals. Ported the orchestrator flag region
(`canDressThemselves` / `canCreateOutfits` / `helpToolsEnabled` /
`documentEditingEnabled`, the `characterIsTransparent` + `self_inventory` strip,
the `askCarinaEnabled` overlay-free probe, the autonomous-room destructive-tool
filter, `resolvedToolMode` / `useTextBlockTools` / `actualTools`, and the
mode-switched `toolInstructions`), and closed the spine seams: the real slate now
flows into the primary stream, the native loop (with the real `BuiltInToolRunner`
+ the injected W4.7 tool-call detector), and the text-tool passes'
`continuationTools`; the finalizer receives the real tool messages/images. Added
`plugin_config::find_by_user_id`. Verified by a new `tool_build_equivalence`
differential (27 flag-matrix cases driving v4's REAL `buildTools`, byte-exact
slate) and the rebuilt `orchestrator_tier3_equivalence` (18 cases running the REAL
`buildTools` + flag region; a per-call tools-at-wire assertion proves the slate
reaches the provider on every case; new cases bank the `self_inventory` strip
[transparent vs not], the `ask_carina` transparency probe, disabled-tools
filtering, and text-block-mode empty slate). `native_tool_loop`, `text_tool_loop`,
`message_finalizer`, and `primary_stream` differentials re-verified green.

Phase 3 — wave 4 (W4.1d batch 3b): the doc-edit tool handlers (part 2 — the
remaining handler groups + the dispatcher wiring). Ported the file-management
group (`doc_move_file` / `doc_copy_file` / `doc_delete_file` / `doc_create_folder`
/ `doc_delete_folder` / `doc_move_folder`, over the `db::database_store`
primitives; the `chat_documents` move-sync is a corpus-verified no-op seam), the
document-UI group (`doc_open_document` / `doc_close_document` / `doc_focus`, with
three new `chat_documents` scoped ops and the `documentMode` chat update that
does not bump `updatedAt`), the blob group (`doc_write_blob` / `doc_read_blob` /
`doc_list_blobs` / `doc_delete_blob`, over the newly-ported `linkBlobContent`
binary storage primitive + blob-repo methods; the WebP transcode is a native
passthrough seam), and the enumeration group (`doc_grep` / `doc_list_files`, over
a new `doc_mount_documents` finder + `list_database_files`). Wired all 23
non-photo `doc_*` tools into `BuiltInToolRunner` (one `run_doc_edit` dispatch
through `execute_doc_edit_tool` inside a both-connections write closure) and
extended `tool_dispatch_equivalence` with two doc-edit dispatch rows. Verified by
four new jest-real-DB differentials (`doc_fm` 20 ops, `doc_ui` 9, `doc_blob` 11,
`doc_enum` 14) driving v4's REAL handlers byte-exact. The photo group stays a
tracked scoped deferral (unported images-v2 + `keep-image-markdown` +
`chunkAndInsertExtractedText`) — it routes to the loud fallback. With this the
entire doc-edit tool subsystem except the photo trio is ported and dispatched.

Phase 3 — wave 4 (W4.1d batch 3b): the doc-edit tool handlers (part 1 — the
foundation + the text/markdown handlers). Ported the database-backed
document-store primitives (`db::database_store`: read/write/move/delete
documents, folder create/delete/move, existence checks — composing the ported
storage leaves) plus the repo finders they need (`doc_mount_folders` /
`doc_mount_file_links` find-by-path/by-mount + a `LinkRow` join, and a
REAL-affinity coercion fix on `chunkCount`/`fileSizeBytes` that was silently
failing the access-control gates); the `tools::doc_edit::shared` access-control
family (cross-character vault visibility, `systemTransparency` opacity, the
`character_read`/`character_write` gates, the folder-protected-descendants
guard, the read/write resolution-context builders, `getAccessibleMountPoints`,
`resolveOfficialProjectMount`); and the first eight `doc_*` handlers
(`doc_read_file` / `doc_write_file` / `doc_str_replace` / `doc_insert_text` +
`doc_read_frontmatter` / `doc_update_frontmatter` / `doc_read_heading` /
`doc_update_heading`) behind a v4-faithful `executeDocEditTool` dispatcher. The
Librarian-announcement and reindex layers are documented no-op seams (mocked in
the oracle, as with the wave-3 whisper-posting seams). Added a `documentMode`
`ChatUpdate` setter. Verified by `doc_text_equivalence`, a jest-real-DB
differential driving v4's REAL `executeDocEditTool` + `formatDocEditResults` over
a 26-op corpus (read line/offset/JSON, self + project + qtap:// addressing,
blocked read + read-only write, str_replace unique/not-found/multiple/diacritics,
insert start/end/before, frontmatter read/keys/none/merge/replace, heading
read/not-found/update) plus a two-table dump. The remaining handler groups
(grep/list, file-management, document-UI, blob) follow; the photo group
(`keep_image`/`list_images`/`attach_image`) is a tracked scoped deferral — it
drags in the unported images-v2 store + `keep-image-markdown` sidecar builder +
`chunkAndInsertExtractedText`, beyond the named byte-source seam.

Phase 3 — wave 4 (W4.1f): the text-tool loop. Ported `runTextToolPass`
(`services::text_tool_loop`): the strategy-driven detect-text-markers →
execute → re-stream-continuation pass the orchestrator runs after the native
loop. The engine is strategy-agnostic behind a `TextToolStrategy` trait
(`hasMarkers`/`parse`/`strip`/`formatToolResult`/`stopSequences`); ships
`SimpleJsonStrategy` and `TextBlockStrategy` composed from the b.1 leaves, and
takes a provider-text-markers strategy as an injected seam (the provider plugin
detector/parser/stripper is W4.7). Reproduces the duplicate-cap nudge (byte-exact
synthetic user message), the iteration cap, the un-stripped-assistant-turn ledger,
the per-continuation reasoning-display-only path, the `usage`/`cacheUsage`/
`rawResponse`/`thoughtSignature` overwrite-on-done, and `assembleStrippedWithOffsets`
(strip once per segment, drop whitespace-only segments with offset carry, UTF-16
`\n\n`-join anchor math). Wired into the orchestrator spine after the native loop
(provider pass seam-gated on an injected strategy, then simple-json vs the
text-block fall-through per an injected `ResolvedToolMode`; the real tool-config
plumbing + tool slate is W4.1g) — corpus-dormant, `orchestrator_tier3` re-verified
green. Differential `text_tool_loop_tier3_equivalence` (nine case families,
DB-free — the pass writes nothing): simple-json single-iteration + text-block
multi-call over the REAL strategy functions, and a synthetic `<<T:name:args>>`
strategy (identical in TS + Rust) for multi-iteration, the duplicate nudge, the
parse-empty no-op, a mid-continuation stream failure, the iteration cap, empty-
stripped-segment assembly (surrogate-pair UTF-16), and stopSequences forwarding.

Phase 3 — wave 4 (W4.1d batch 4): the four search/introspection tool handlers,
each byte-exact against v4's REAL handler and wired into `BuiltInToolRunner`.

- `search` (`tools::search`): the Scriptorium unified search over memories (the
  ported `search_memories_semantic`), conversations (new `db::conversation_search`
  = v4 `searchConversationChunks`, a sibling of `document_search` over
  `conversation_chunks` BLOB embeddings), documents (`document_search`), and
  knowledge (the same document search narrowed per tier to `Knowledge/`), merged
  and ranked. Reproduces the per-source error-swallowing branches, the
  tier-ordered knowledge dedup (character > group > project > global, knowledge
  wins over document for a shared chunk), the `qtap://` URI tagging via
  `DocStoreUriResolver`, the operator/Brahma surface (memory forced off,
  operator-wide stores + conversations by userId), the 500-char content
  truncation, and the exact result-strings/labels (`(score*100).toFixed(0)%` via
  the ported `to_fixed`). Serves both the standard and Brahma tool definitions.
- `project_info` (`tools::project_info`): `get_info` (overview, roster, item
  counts, and the linked store summary via the new pure leaf
  `db::project_store_naming::pick_primary_project_store` = v4
  `pickPrimaryProjectStore`) and `get_instructions`, byte-exact including the
  no-project error.
- `help_search` (`tools::help_search` + new `db::help_search`): semantic search
  over `help_docs` embeddings with the automatic keyword fallback when embedding
  fails (the `extract_search_terms` keyword extractor added to `embedding_vector`).
  The `ensureHelpDocsSynced` disk index-build is a documented host seam (no-op once
  `help_docs` is populated); the tool path is a pure read.
- `request_full_context` (`tools::request_full_context`): flips the chat's
  `requestFullContextOnNextMessage` flag. Ported as a self-contained single-column
  `UPDATE` (byte-identical to v4's `repos.chats.update`, which does not bump
  `updatedAt`) so it needs no `db/chats.rs` change.
- Dispatcher: the runner now carries an injectable `ErasedEmbeddingProvider`
  (default a never-succeeds `NoEmbeddingProvider`) so `search`/`help_search` reach
  the embedding seam without a second generic on the shared struct; a real provider
  wires with W4.1g.
- New read helpers (all additive): `conversation_chunks::find_all_with_embeddings`,
  `help_docs::find_all`/`find_all_with_embeddings`, `doc_mount_blobs::count_by_mount_point`,
  `files::count_by_project_id`, `doc_mount_points::find_store_naming_by_id`.
- Differential `search_tools_equivalence` (24 cases across two jest real-DB oracles
  driving v4's REAL handlers, only `generateEmbeddingForUser` mocked to canned
  vectors, `Date.now()` frozen): each case on a fresh two-DB fixture copy (search
  bumps `lastAccessedAt`; request_full_context writes), comparing serialized result
  JSON + `format*` strings (float-safe) and, for request_full_context, the full
  `chats` row. `knowledge_injector` / `first_message_context` /
  `tool_execution_process_tier3` re-verified green (the `document_search` module was
  made public + a read added, no behavior change).

Phase 3 — wave 4 (W4.1d batch 5, part 4): the `generate_image` pure leaves
(`crate::image_gen`), ported leaf-first ahead of the stateful handler. Ported
`resolveOrientation` (v4 `lib/image-gen/orientation.ts`) — the pure `(provider,
model, orientation)` → concrete-request-mutation mapping (`matchModel` exact +
longest-prefix, `realize` strategy-honouring + degrade-to-hint, the host fallback),
with the plugin-registry declarations (`getImageGenerationModels` /
`getImageProviderConstraints`) passed in as data — and `parsePlaceholders`
(`prompt-expansion.ts`, the `{{name}}` scanner, name `.trim()`-ed). Differential
`image_gen_leaves_equivalence` (tier-1, DB-free) drives v4's REAL functions (the
registry jest-mocked to canned declarations) and diffs `JSON.stringify`.
**Scoped deferral:** the full `executeImageGenerationTool` handler +
`saveGeneratedImage` persistence — they compose the image-provider call + WebP +
Lantern store/notification (host seams), the entire W4.2 dangerous-content
classify/route path (with a double profile reroute), and three cheap-LLM tasks
(`craftImagePrompt` / `resolveCharacterAppearances` / `sanitizeAppearance`),
several themselves large unported units; the handler lands once those exist.

Phase 3 — wave 4 (W4.1d batch 5, part 3): the `search_web` tool handler
(`tools::web_search`), byte-exact against v4's REAL handler. The whole search
boundary (the plugin `searchProviderRegistry` + API-key lookup + Serper fallback)
is the injected `WebSearchProvider` seam (canned outcome both sides); the portable
half is the input validation, the outcome → output mapping (byte-exact error
strings for the not-configured / missing-key / provider-failure branches), and the
built-in result formatter (a `publishedDate` renders via a UTC-pinned
`toLocaleDateString()` added to `format_time`). Wired into `BuiltInToolRunner` with
a default `NotConfiguredWebSearch` provider (faithful to a no-search-plugin
instance — v4's "not configured" error; a real provider is host-wired). Differential
`web_search_tool_equivalence` (DB-free, jest-mocked registry) diffs the serialized
output + `format_web_search_results` over success/failure/missing-key/not-configured/
validation cases. Deferrals: the provider's own `formatResults`, host-side API-key
acquisition, and a date-only `publishedDate` (the corpus uses full-ISO dates).

Phase 3 — wave 4 (W4.1d batch 5, part 2): the Post Office (`send_mail` /
`list_email`) + `ask_carina` tool handlers, byte-exact against v4's REAL handlers.

- New `post_office` module (v4 `lib/post-office/`): the mailbox storage layer
  (`mailbox` — slugify/compose/parse/reply-preface + `deliver_letter` /
  `read_letter` / `list_mailbox`), the shared delivery service (`deliver` —
  `compose_and_deliver_letter` / `resolve_reply_in_sender_mailbox`), and the
  agent-facing instruction snippets (`instructions`). All over the ported vault
  primitives (`write_database_document` / `ensure_character_vault` / the
  `Mail/` folder conventions); the delivery `sentAt` is injected so it can be
  pinned. Plus `db::character_resolver` (`resolve_character_by_name_or_id`) and
  `format_time` (the UTC-pinned `formatDateTime` — v4's system-timezone
  `toLocaleDateString`, reproduced in UTC for the differential).
- `send_mail` / `list_email` (`tools::send_mail` / `tools::list_email`) compose
  those over both writer connections; wired into `BuiltInToolRunner`.
- `ask_carina` (`tools::ask_carina`) over the existing `RunCarinaQuery` +
  `PostProsperoCarinaError` seams from `services::carina_runner`. The handler +
  differential are complete; its dispatch stays on the loud fallback until the
  W4.5 Carina query engine is orchestrator-injected as the seam (the `onPosted`
  / `emitCarinaAnswer` slot is the documented tool-context deferral).
- Differential `mail_carina_tools_equivalence`: the mail half (real-DB, delivery
  clock pinned) drives v4's REAL handlers over a fresh two-DB fixture copy per
  scenario — diffing the serialized output + `format*` and reading the delivered
  letter's content back byte-for-byte (send-then-list round-trip, reply preface,
  every validation/refusal path, empty + single + plural listings); the carina
  half (DB-free) injects canned seams and diffs output + `format*` + the recorded
  Prospero args. Deferrals: the Suparṇā mail-check helpers
  (`collect_unalerted_mail` / `mark_alerted`).

Phase 3 — wave 4 (W4.1d batch 5, part 1): the `state` + `run_sql` tool handlers,
each byte-exact against v4's REAL handler and wired into `BuiltInToolRunner`.

- `state` (`tools::state`): persistent per-chat / per-project key-value state.
  Ported `parsePath` (dot notation + array indexing), `getAtPath` (undefined vs
  stored-null distinguished), `setAtPath` (intermediate object/array creation),
  `deleteAtPath` (object delete + array splice), and the `mergeState` spread
  (chat overrides project). Chat writes go through `chats.update({state})` (no
  `updatedAt` mint); project writes route to the store-backed `state.json`
  overlay. The output serializes in a fixed field order that reproduces every
  per-branch `JSON.stringify` (undefined dropped, null kept), and
  `formatStateResults` matches byte-for-byte.
- `run_sql` (`tools::run_sql`, Brahma Console read-only SQL): the read-only guard
  ported faithfully (the literal/comment-stripping pre-scan + forbidden-keyword +
  single-statement + mutating-PRAGMA checks, then rusqlite `Statement::readonly`
  fail-closed, then the `max_rows` cap). BLOB cells sanitize to `<blob: N bytes>`;
  REAL cells render via `js_number_to_json`. SQLite prepare/exec error strings are
  byte-identical (same SQLite3MC engine). The `operatorSurface` gate is a
  dispatcher guard. Zod-validation-message fidelity is limited to the non-object
  case (documented; the pre-scan/prepare failures cover the real refusals).
- Differential `state_sql_tools_equivalence` (34 cases, one jest real-DB oracle
  driving v4's REAL handlers over a fresh three-DB fixture copy per case): state
  cases diff the serialized output + `formatStateResults` + the `chats` table dump
  (zero normalization — no `updatedAt` mint) + a project-`state` read-back (the
  overlay bytes are already proven by `projects_tier2`); run_sql cases diff the
  serialized envelope, covering each target DB, blob sanitize, truncation, and
  every refusal path.

Phase 3 — wave 4 (W4.1d batch 3a): the doc-edit foundation, part 3 — the path
resolver + URI producers (completing batch 3a). Ported `resolveDocEditPath`
(`doc_edit::path_resolver`) — the `document_store` scope (over the tiered mount
pool: the SELF token, name-vs-id mount matching, ambiguity/not-found/disabled
errors, traversal/absolute/missing-path guards) and the `project` scope's
official-mount alias — with byte-exact `PathResolutionError` codes + messages, plus
`resolveSelfVaultMountPointId` / `resolveMountPointRef`. The legacy on-disk
branches (`filesystem`/`obsidian` real paths, the project legacy fallback, the
whole `general` scope) are a **host-filesystem seam** deferred to the Phase-4 host.
Ported the async URI producers (`doc_edit::uri_producers`: `docStoreUriFor`,
`uriForResolvedPath`, `buildDocStoreUriResolver`) over the ported qtap producers +
`doc_mount_points::{count_by_name, find_enabled}`. Verified by a 23-case
read-differential (`doc_edit_path_resolver_equivalence`) driving v4's REAL resolver
+ producers over a two-DB fixture (a character + vault, a real project with a
provisioned official store, P-linked stores incl. a duplicate-named pair + a
disabled store, the General singleton); every store database-backed so the FS seam
is never hit. Added `projects::find_official_mount_point_id_raw`. With this the
whole doc-edit foundation (batch 3a) is complete; the ~26 `doc_*` tool handlers
(batch 3b) sit on it.

Phase 3 — wave 4 (W4.1d batch 3a): the doc-edit foundation, part 2 — the pure
leaves. Ported `lib/doc-edit/{diacritics, mime-registry, unified-diff,
markdown-parser}.ts` into `doc_edit::{diacritics, mime_registry, unified_diff,
markdown_parser}`, each verified by one grouped tier-1 differential
(`doc_edit_leaves_equivalence`, 81 rows) against v4's REAL exports. Diacritics:
NFD normalize + strip-combining (via `unicode-normalization`, proven byte-exact on
precomposed/decomposed Latin + Hangul) and the `findAllMatches`/`findUniqueMatch`
UTF-16 index/length remap. MIME registry: `detectMimeFromExtension`, the `isJson*`
predicates, and `parseContent`/`serializeContent`/`validateJson` (JSON +
JSONL) — the happy-path bytes byte-exact (`serde_json` pretty ==
`JSON.stringify(x, null, 2)`), with the V8 `JSON.parse` error TEXT a documented
normalized seam (structure/values/line-numbers compared exactly, failure messages
normalized). Unified diff: the hand-rolled greedy look-ahead algorithm reproduced
exactly (git-style `@@` hunks), not "a" diff. Markdown: `slugifyHeading` (ASCII
`\w` + JS `\s`), `parseHeadingTree` (ATX headings, code-fence exclusion, duplicate-
slug counter suffixes, UTF-16 offsets), `findHeadingSection` (byte-exact thrown
messages), `readHeadingContent`/`replaceHeadingContent`, and
`serializeFrontmatter`/`updateFrontmatterInContent` — the latter reusing the
already-ported eemeli scalar emitter so `YAML.stringify` is byte-exact over the
frontmatter value space (string/bool/number/null scalars + flat sequences; nested
maps/exotic numbers a documented seam). `document-policy.ts` needed no new port
(its leaves already live in `db::doc_mount_file_links`). The DB-backed path
resolver + URI producers follow.

Phase 3 — wave 4 (W4.1d batch 3a): the doc-edit foundation, part 1 — the tiered
mount pool + the `qtap://` URI codec. Ported `resolveTieredMountPool` /
`classifyMountTier` / `flattenTierPool` and hoisted the canonical
`dedupeTierTriple` into `db::tiered_mount_pool` (v4's
`lib/mount-index/tiered-mount-pool.ts` — its true home), refactoring the
knowledge injector to consume the dedup from there (its differential re-verified
green). The five-tier character/participant/group/project/global resolution
reproduces the ownership gate (fails closed without `userId`), the pre-resolved
character-mount fast path, the per-RESPONDING-character group tier, graceful
global-null, per-tier error swallowing, and the character>group>project>global
dedup — verified by a 9-case read-differential (`tiered_mount_pool_equivalence`)
against v4's REAL resolver over a two-DB fixture (2 characters + vaults, a group
with an official + linked store + membership, a project with colliding links, the
General singleton). Ported the full `qtap://` URI codec (`doc_edit::qtap_uri`,
v4's `qtap-uri.ts`) — `parseQtapUri` / `formatQtapUri` / `isQtapUri` /
`qtapUriToResolverInput` / `QtapUriError` + the producer helpers — unifying it
with the producers previously hoisted into the knowledge injector (now re-exported
from the canonical home). Reproduces JS `encodeURIComponent` /
`decodeURIComponent` exactly (a V8-faithful `Decode` with UTF-8 run validation),
the last-`:` fragment split, BAD_LEVEL bounds, the encoded-slash segment, and the
insertion-ordered query map; verified by a 54-row tier-1 differential
(`qtap_uri_equivalence`) incl. malformed-percent-encoding + non-ASCII round-trips.
Added the scoped mount-point reads the resolver needs
(`doc_mount_points::{find_by_id_for_docedit, find_enabled_for_docedit,
count_by_name}`, `groups::find_official_mount_point_id_raw`). Remaining batch-3a
foundation (diacritics, MIME registry, unified diff, markdown heading/frontmatter
ops, path resolver, URI producers) follows.

Phase 3 — wave 4 (W4.1e): the native tool loop + the finalizer response-RNG.
Ported `runNativeToolLoop` (`services::native_tool_loop`): the bounded
stream → detect → execute → thread → re-stream loop after the primary stream,
including the agent-mode `submit_final_response` accept (siblings-first,
replace-vs-preserve, ghost-wrap reject), the output-token truncation guard, and
the max-turns force-final pass. Two injected seams: a `ToolCallDetector` (the
provider wire parse is W4.7) and the frozen `ToolRunner` (W4.1d). Added the
partial `services::agent_mode` (the pure helpers the loop consumes; the resolver
cascade is W4.4), the `ChatUpdate.agentTurnCount` setter (the loop's only DB
write), a public `StreamingState::next_turn_seq`, and `jsstr::js_index_of`
(UTF-16). Wired into the orchestrator spine at v4's composition point
(corpus-dormant until `buildTools`, W4.1g). Closed the finalizer's assistant-
response RNG seam: the ported detector + executor now run inline (the
`auto-detect-response` TOOL-row shape with a UTF-16 `anchorOffset`), only the
CSPRNG byte source injected; the orchestrator shares one `rng_bytes` across the
user-message and assistant-response auto-detect (dropping the `finalizer_rng`
generic). Differentials: `native_tool_loop_tier3_equivalence` (seven case
families, a three-boundary mock split) and the extended
`message_finalizer_tier3_equivalence` (RNG fire + no-fire; its oracle un-stubs
detection and mocks `crypto.randomBytes`); `orchestrator_tier3` re-verified green.

Phase 3 — wave 4 (W4.1d batch 1): the first tool-handler batch. Ported the nine
immediately-portable tools (every underlying repo already ported, no model
calls) plus the real dispatching `ToolRunner` that batches 2–5 will extend. Each
handler ships a differential driving v4's real handler byte-exact.

- Handlers (`tools::{read_conversation, annotations, terminal, whisper, help,
  self_inventory}`): `read_conversation`, `upsert_annotation`/`delete_annotation`
  (over the ported `conversation_annotations` repo, extended with the find/delete
  readers + the ported `scriptorium::{merge,strip}_annotations` leaves),
  `terminal_read`/`terminal_list` (over `terminal_sessions` reads + the ported
  `terminal_clean::clean_terminal_output`; the live-PTY/transcript scrollback is
  an injected seam), `whisper` (resolves the target by name/alias among
  whisper-receivable participants, writes one `chat_messages` row — no post-office
  side effect), `help_settings`/`help_navigate`/`submit_final_response` (the
  first two + the pure agent-mode validator; `help_settings` needed the full
  `chat_settings::find_by_user_id` read marshaling, now ported), and the big
  `self_inventory` (the ten-section introspection report over ~a dozen repo
  readers + `build_system_prompt`; the runtime-mode/client-shell/release-notes/
  changelog/mount-index-degraded host bits are an injected `SelfInventoryEnv`
  seam — `quilltap.version` covered, releaseNotes/changelog deferred).
- `LoadedMemoriesContext` is now typed (`{ semantic, interCharacter, recap }`) —
  its consumer `self_inventory` landed.
- The dispatching runner (`tools::executor::BuiltInToolRunner`): routes a tool
  call by name to the ported handlers (reproducing v4
  `executeToolCallWithContext`'s built-in dispatch rows — the `{ formattedText,
  … }` result shape, the failure `null`/`error` mapping, the dispatcher-side
  guards + annotation character-name resolution), with an injected inner
  `ToolRunner` fallback for unported names (the loud default reproduces v4's
  `Unknown tool: <name>` for names v4 doesn't know, and a "recognized but not yet
  available" failure naming a not-yet-ported built-in). Plugin-vs-built-in
  routing precedence is a documented deferral (the plugin registry is unported).
- New leaf modules: `scriptorium`, `terminal_clean`, `folder_utils`;
  `format_scoped_uri` added to `knowledge_injector::qtap_uri`.
- Differentials: per-handler tsx/jest-real-DB oracles (success / invalid-input /
  edge per tool) + an end-to-end dispatcher differential driving v4's real
  `executeToolCallWithContext` over a mixed batch (read, two writes with
  character-name resolution, a pure tool, a handler failure, an invalid-input
  failure). The unknown-tool loud fallback is unit-tested (v4's genuine unknown
  path depends on the unported plugin registry). Existing `tool_execution_*` +
  `message_finalizer` + `orchestrator` differentials re-verified green.

Phase 3 — wave 4 (W4.1d batch 2): the seven wardrobe tool handlers. Ported
`wardrobe_list` / `wardrobe_read` / `wardrobe_create` / `wardrobe_update` /
`wardrobe_archive` / `wardrobe_wear` / `wardrobe_take_off` (`tools::wardrobe_*`)
over the already-ported vault-public CRUD, the public read trio + shared-archetype
tier, and the equipped-outfit ops, plus the pure `crate::wardrobe` leaves
(`unionTypes`, `describeOutfit`, `expandComposites`, the flag-driven equip
primitives, `describeWardrobeEffect`, sentinel normalization), the DB-touching
`tools::wardrobe_shared` helpers (across-tier item resolution, the persisted equip
primitives, `resolveEquippedOutfitForCharacter`, the coverage summary,
`resolveProjectMountPointIdsForChat`), and `find_by_ids_for_character`. Extended
`BuiltInToolRunner` with the seven dispatch rows (each runs inside a single writer
closure holding both the main + mount-index connections). The
`pendingWardrobeAnnouncements` field became `Arc<Mutex<HashSet<String>>>` so the
handlers can record an announcement through the immutable `ToolRunner::run`
boundary without changing the trait signature; the end-of-turn drain stays a
documented deferral. Avatar generation on equip is an image-subsystem seam (out of
scope; gated off in the corpus). Differentials: `wardrobe_tools_equivalence` (a
25-op sequence — success / invalid / edge per handler, gift, composite+equip,
shared read-only, slot mismatch, plus a read-back of both wardrobes / archetypes /
equipped outfit, minted ids/timestamps positionally normalized) drives v4's REAL
handlers; the dispatcher differential gained a `wardrobe_list` call; the existing
`tool_execution_*` + `tool_dispatch` differentials re-verified green.

Phase 3 — wave 4 (W4.1c): tool execution + persistence primitives
(`services::tool_execution`, v4 `tool-execution.service.ts`) — the harness and
the TOOL-row writer between the tool loops (W4.1e/f) and the tool handlers
(W4.1d).

- `save_tool_messages` + `compute_tool_message_targets` + `files::add_tag`:
  the TOOL-row persistence primitive (one `type:'message'`/`role:'TOOL'` row per
  tool message through the ported `chats_messages::add_message` path) with the
  whisper gate (ALWAYS_PRIVATE tools + VAULT_READ tools vs
  `allowCrossCharacterVaultReads`, whispered to the **user participant**) and the
  generated-image link+tag loop; the generic content JSON in v4 field order.
  Tier-2 differential (`tool_execution_tier2_equivalence`) driving v4's real
  `saveToolMessages` over the whisper matrix, content omission (anchorOffset/seq/
  callId + metadata), the multi-message batch + `firstToolMessageId`, and the
  image link+tag — byte-exact across `chat_messages`/`chats`/`files`.
- `process_tool_calls` + the injected `ToolRunner` boundary +
  `ToolExecutionContext`: the per-call dispatch harness (detection frame,
  per-tool `tool_executing` status, tool-result frame, generated-image
  extraction, the failure `ToolMessage` shape). `chat_events` gains the additive
  `toolsDetected` + `toolResult` frames. Tier-3 differential
  (`tool_execution_process_tier3_equivalence`) driving v4's real
  `processToolCalls` with only `executeToolCallWithContext` mocked — ordered
  frames + `toolMessages` + `generatedImagePaths` matched.
- Spine wiring: `save_tool_messages` wired into the finalizer's
  `toolMessages.length > 0` gate (inside `save_assistant_message`, before the
  assistant image-link loop, so a generated image's `linkedTo` order matches v4),
  and the orchestrator tool-only terminal branch (`saveToolMessages` + `updatedAt`
  bump + the `toolsExecuted: true` done frame). Fixed the finalizer done frame's
  `toolsExecuted` (was hardcoded `false`; now `toolMessages.length > 0`) — caught
  by the finalizer direct-drive. `message_finalizer_tier3_equivalence` gained a
  `tool-save` case driving v4's real finalizer with an injected tool slate;
  `orchestrator_tier3_equivalence` re-verified green (branches corpus-dormant
  until the tool loops).
- The canonical `ToolMessage` now lives once in `services::tool_execution`;
  `services::tool_call_threading` reuses it (its narrow subset removed), matching
  v4's single `chat-message/types.ts` definition. Threading differential
  re-verified.

Phase 3 — wave 4 (W4.1b): the tool-subsystem pure leaves. The pure foundations
the tool loops, executor, and handler catalog will consume — all tier-1 exact
against v4's real `lib/tools/` + service code.

- Tool-call threading (`services::tool_call_threading`, v4
  `tool-call-threading.ts`): `build_assistant_tool_call_message` /
  `build_tool_result_messages` — the callId-present-vs-absent pairing rule,
  empty/whitespace-prose collapse, reasoning/thoughtSignature forwarding, and the
  `[Tool Result: <name>]` text fallback. Tier-1 differential
  (`tool_call_threading_equivalence`, 22 cases).
- Pseudo-tool machinery (`tools::{simple_json_parser, text_block_parser,
  simple_json_prompt, text_block_prompt, native_tool_prompt, pseudo_tool_support}`
  + `services::pseudo_tool`): the three-tier simple-json parser, the text-block
  parser/converter, both prompt builders, the native-tool prompt, mode
  resolution, and the service wrappers. The two backreference regexes are
  hand-rolled (`regex` crate has no backreferences); the `jsonrepair` tier is a
  bounded hand-rolled subset (single/smart quotes, unquoted keys, trailing
  commas) that resolves conservatively (tier-fail → `[]`) outside its documented
  scope, corpus-pinned on both sides of the boundary. Tier-1 differentials
  (`pseudo_tool_parsers_equivalence`, 138 cases; `pseudo_tool_prompts_equivalence`,
  40 cases) driving v4's real exports.
- Tool-definition catalog (`tools::definitions`, all 57 definitions from the 56
  `*-tool.ts` files): byte-exact static JSON transcribed from v4's
  `JSON.stringify` output (not by re-implementing the Zod→JSON-Schema emitter),
  generated by a checked-in script. Byte-exact differential
  (`tool_definitions_equivalence`) proving the serde round-trip reproduces JS
  `JSON.stringify`, catalog completeness, and a `canonicalize_universal_tools`
  spot-check over the full real catalog.

Phase 3 — wave 4 (W4.1a): the RNG subsystem. v4's pre-message RNG auto-detect
path — scan the user message for dice/coin/bottle patterns, execute them, write
TOOL messages into the chat before the model turn — is ported and verified end
to end, closing the orchestrator's `user_message_rng` seam.

- `rng_patterns` (pure): v4's `rng-pattern-detector.service` —
  `detect_rng_patterns` / `convert_patterns_to_tool_calls` /
  `detect_and_convert_rng_patterns`. The three regexes reproduce JS fidelity:
  ASCII `\b`/`\d` via `(?-u:\b)`/`[0-9]`, the JS-`.` line-terminator exclusion,
  the "flip a coin" 1–3-char quirk (so "flip the coin" does NOT match), and the
  spin-bottle `{0,50}` bound. Tier-1 differential (`rng_patterns_equivalence`, 54
  cases) driving v4's real exports over both the detected patterns and the
  converted tool calls, incl. bounds rejections, non-ASCII adjacency, and a ReDoS
  adversarial string.
- `tools::rng` (executor): v4's `rng-handler` — `execute_rng_tool` /
  `secure_random_int` (rejection sampling) / `roll_dice` / `flip_coin` /
  `spin_the_bottle` / `format_rng_results` + the Zod input validation. The
  randomness source is an injected `RandomBytes` byte stream (production
  `OsRandomBytes`; the differential replays a committed sequence), so
  `secureRandomInt`'s variable-length byte consumption is itself part of what the
  diff proves. `RngType` serializes back to v4's number-or-string union.
  Differential (`rng_executor_equivalence`, 14 cases) drives v4's real
  `executeRngTool` against a real fixture DB (spin resolves participant names
  through the repos) with `crypto.randomBytes` pinned, diffing the output + the
  formatted string + asserting byte-exact stream consumption.
- Orchestrator seam closed: the ported detector + executor run inline in
  `process_message`, writing a TOOL message per detected pattern (byte-identical
  content JSON in v4's field order) and appending it to the context so the model
  turn sees the results. The byte source is injected via
  `OrchestratorDeps::rng_bytes`. The `user_message_rng` seam method was removed.
  The tier-3 corpus gained three cases (`rng_dice`, `rng_two_patterns`,
  `rng_no_fire`) and `autoDetectRng` was flipped on globally (a per-user setting;
  existing content carries no patterns, so they no-op); the whole
  `orchestrator_tier3_equivalence` corpus re-verified green.

Phase 3 — wave 4 (W4.0): the wardrobe drift batch. The public wardrobe READ
trio, the General/project shared-archetype tier, and the wardrobe transfers
service are all ported and verified — closing the 2026-07-03 drift-check's
wardrobe surfaces and the long-deferred archetype tier.

- `db::instance_settings`: the per-instance key/value store (main db);
  `get_general_mount_point_id` resolves the provisioned "Quilltap General" store
  id, tolerating a missing table like v4's `readSetting`. Unit tests.
- Archetype seeding generalized into the read overlay:
  `read_character_vault_wardrobe` gained `seed_archetypes` + an injected archetype
  fetch, and `resolve_and_check_component_items` moved from index-valued to
  `SeedArchetype`-seeded maps (v4's local-wins gap-fill) so a composite can
  reference a shared archetype it doesn't hold. Backward-compat: the existing
  `vault_wardrobe_read` / `vault_wardrobe_public` differentials stay green (empty
  seed = no-op), plus two new resolver unit tests bank real seeding + an
  archetype-routed cycle.
- `db::archetype_wardrobe`: `read_general_wardrobe` / `read_project_wardrobe`,
  the `find_archetypes` insertion-ordered General-under-project merge, and
  `find_archetype_by_id`.
- Public READ trio (`db::wardrobe_read::find_by_character_id` /
  `find_by_id_for_character`) — vault-aware reads over the seeded overlay.
  `findByCharacterIdRaw` is a tracked deferral (deprecated; reads the pre-cutover
  `wardrobe_items` table the vault era drops; no W4.0 consumer). Verified by a
  read-differential (`wardrobe_public_read_equivalence`) against v4's REAL repo:
  five cases where a character composite references a General archetype by slug
  AND UUID (both resolve only via seeding) plus the archetype fallback.
- Public WRITE generalized to a `WardrobeLocation` (character/General/project)
  with `create/update/delete_project_wardrobe_item` and General archetypes seeded
  into the cycle-peer check; a `null` characterId now resolves to Quilltap
  General instead of erroring. Re-verified green.
- `services::wardrobe_transfers`: v4's `/api/v1/wardrobe/transfers` POST
  (move/copy across the four tiers) + GET destination enumeration, composed over
  the ported repo ops + `ensure_official_store`. Verified by a tier-2 differential
  (`wardrobe_transfers_tier2_equivalence`) driving v4's REAL POST handler under a
  jest-real-DB oracle (session mocked, real encrypted DB) over five scenarios
  (copy→general, move→project, copy→character, same-location reject, id-collision
  reject), diffing the outcome + seven mount-index tables in the
  shared-cross-db-id-map remap form. The normalizer assigns `fileId` tokens by the
  `file_links` walk (store+path stable — a copy's minted-timestamp `.md` perturbs
  the content-addressed sha) and pins `chunkCount` before sorting.

Docs — Phase 2 marked complete; Phase 3 kickoff drafted. Docs only, no crate
source changed.

- `overview.md`: the Phase-2 roadmap row now reads repo-inventory-complete (every
  v4 repository round-trips green through the tier-2 harness), with the residual
  Phase-3-coupled deferrals named; the stale "nineteen repos" status prose was
  corrected the same way, and the document list + Phase-3 row now point at the new
  kickoff doc.
- `phase-2-onramp.md`: deferred seam #4 (`write_apply`'s `__finalizeFile` +
  post-commit effects) flipped from open to resolved, matching the
  ported-and-verified state.
- Added `docs/developer/porting/phase-3.md` — the Phase-3 kickoff: the tier-3
  mocked-LLM tier; the writer-task runtime (Unit 0); the tier-3 harness scaffold
  (Unit 0.5); the memory gate as first service (Unit 1), with a caution to port
  its similarity-band constants (0.90 / 0.85 / 0.70), not the file's stale
  0.80/0.70 doc comment; and the three Phase-2-carried deferrals.

Phase 2 on-ramp — the tier-2 DB-state oracle (structural DB diff for repo/service
ops), built as a thin vertical slice over the `folders` repo:

- Oracle harness (TypeScript, drives v4's real `lib/`): a committed plaintext
  fixture spec (`harness/oracle/fixtures/folders-tier2.json`) under a throwaway
  test pepper; a fixture builder that materializes a fresh ChaCha20 DB at test
  time via v4's own `ensureCollection` + `FoldersRepository.create`; and the
  `folders-tier2` case that copies the fixture, runs a fixed create + update
  through the real repo, and emits the canonical post-op `folders` dump as NDJSON.
- Canonical dump shaping (`harness/oracle/lib/tier2.ts`): columns in on-disk
  order, rows sorted by a stable key, BLOBs as hex, nulls explicit.
- Determinism: ids and timestamps pinned on both sides (CreateOptions on create,
  explicit `updatedAt` on update), so the dump needs zero normalization — the
  strongest tier-2 form. The id-remap / timestamp-placeholder fallbacks are
  reserved for later repos that cannot take injected ids/clocks.
- Rust DB layer (`quilltap-core::db`): the writable cipher-correct open (key
  pragma first, then `foreign_keys = ON` + `journal_mode = TRUNCATE`), the
  single-writer `Writer` that solely holds the RW connection, the `folders`
  repo's `create` + `update` ported from v4, and a canonical `dump_table_json`
  matching the oracle's shape.
- Build: the SQLite3MultipleCiphers amalgamation build (`build.rs` + `vendor/`)
  moved from the probe into `quilltap-core`, which now links the ChaCha20/sqleet
  library for the whole workspace; the workspace `rusqlite` dependency switched
  off `bundled-sqlcipher` to the amalgamation (`buildtime_bindgen`). The
  throwaway `sqlcipher-probe` / `sqlite3mc-probe` crates are retired.
- Harness: tier-2 differential test `folders_tier2_equivalence` — copies the
  same seed fixture, runs the Rust ops, structural-diffs the dump against the
  oracle NDJSON (`QT_ORACLE_FOLDERS` + `QT_FIXTURE_FOLDERS`, skip-if-unset).
  The `folders` repo round-trips green.

Phase 2 — the `chats` repo, sub-unit 1: slim-row marshaling
(`quilltap-core::db::chats`). The first cut of the last and largest repo (v4's
`ChatsRepository`, a `TaggableBaseRepository`). Ports `create` / `update` /
`delete` over the **~96-column** `chats` table (MAIN db) — the widest marshaling
surface in Phase 2. Banks: the typed `participants` **array-of-objects JSON
column** (`ChatParticipant`, 18 fields in schema order, nullable optionals
`skip_serializing_if`, `displayOrder` an `i64`, `talkativeness` rendered the JS
way so an integer-valued `1.0` → `1`; the schema `.refine()` requires ≥1
participant); the simple JSON-array columns; the **plain-string** `turnQueue` /
`spokenThisCycleParticipantIds` columns (which hold JSON text `'[]'` but are
`z.string()`, bound raw); the number-affinity columns (all bound `f64`);
booleans; enum TEXT; and the long tail of nullable strings/uuids/timestamps. Two
invariants banked: `update` **never mints `updatedAt`** (it preserves the
existing value unless the caller passes one — only a new message bumps it), so
the whole differential is the pinned zero-normalization form; and on SQLite
`create` writes nothing to `chat_messages`. Verified by a tier-2 differential
(`chats_tier2_equivalence`) driving v4's REAL `ChatsRepository` over a
create×3 / update×3 (both the preserved- and explicit-`updatedAt` branches) /
delete sequence, diffing the `chats` dump byte-for-byte. **Tracked deferrals:**
`delete`'s participant-vault summary sweep (external subsystem), the open-JSON
object columns' multi-key insertion order (constrained to `{}`/single-key/null),
and the rest of the repo (messages, participants, impersonation, tokens, search,
outfits, read queries) — the remaining sub-units.

The `chats` repo — sub-unit 2: the **slim-row read path** (`db::chats_read`,
`chats_read_equivalence`). Ports the read marshaling (the inverse of sub-unit 1's
~96-column write = v4 `_findById` = hydrateRow + Zod parse) + the `findBy*`
queries (`findById` / `findAll` / `findByUserId` / `findByCharacterId` /
`findByType` / `findRecentSummarizedByCharacter`). The marshaling reproduces v4's
net read shape: nullable-optional columns OMITTED when `NULL` (v4 `undefined`
dropped by `JSON.stringify`), `.default(...)` numbers/bools/enums/arrays + `state`
(`{}`) materialized, numbers rendered the JS way, and `participants` re-parsed
per-element so each participant's own defaults materialize (`controlledBy: 'llm'`,
`displayOrder: 0`, `isActive: true`, `status: 'active'`, `hasHistoryAccess:
false`) and its nullable-optionals drop. `findByCharacterId` /
`findRecentSummarizedByCharacter` use the nested `participants.characterId`
`json_each` + `json_extract` match v4's query translator emits; the latter
reproduces the `$exists`/`$nin`/`$ne` → `IS NOT NULL` / `NOT IN` / `!=` filter +
`ORDER BY "lastMessageAt" DESC` + `LIMIT`. Verified by a read-differential: both
sides READ a copy of one fixture baked by v4's REAL `repos.chats.create` (seven
chats — a rich chat exercising every marshaling branch, a minimal chat, salon /
help / brahma types, summarized chats with distinct `lastMessageAt`), running 16
queries compared exactly (no normalization — nothing mutated).

The `chats` repo — sub-unit 3: the **`chat_messages` read path**
(`db::chats_messages_read`, `chats_messages_read_equivalence`). Ports v4's
`ChatMessagesOps` read surface — `getMessages` / `getMessageCount` /
`findChatIdForMessage`. Messages live in their own MAIN-db `chat_messages` table
(one row per event); `getMessages` reads every row for a chat ordered by
`createdAt` and validates each through `ChatEventSchema`, a three-member union
(`MessageEvent` / `ContextSummaryEvent` / `SystemEvent`). The marshaling
dispatches on the `type` discriminator and reconstructs each member: required
columns read directly, nullable-optional columns OMITTED when `NULL`, and the
array/object JSON columns (`rawResponse` [`z.record`], `attachments`,
`reasoningSegments`, `dangerFlags`, `hostEvent`, `customAnnouncer`, `carinaMeta`,
`pendingExternalAttachments`, `summaryAnchor`, …) parsed straight to JSON. No
read-side default materialization is needed: v4 runs `ChatEventSchema.parse`
*before* every insert, so each `.default(...)` (e.g. `attachments` → `[]`, a
`DangerFlag`'s `userOverridden` / `wasRerouted` → `false`) and the exact
int-vs-float number representation are already baked into the stored bytes.
Verified by a read-differential: both sides READ a copy of one fixture baked by
v4's REAL `repos.chats.addMessages` (one chat + twelve messages covering every
event member and JSON column), running 7 queries compared exactly (no
normalization). **Tracked seam:** `isSilentMessage` — its
`z.union([boolean, number.transform])` maps to TEXT affinity, so a stored boolean
round-trips as the string `"1"` and v4 drops the whole message on read; the
corpus keeps it absent and the column is not read here (close before reading real
data that sets it).

The `chats` repo — sub-unit 4a: the **`chat_messages` write path**
(`db::chats_messages`, `chats_messages_tier2_equivalence`). Ports v4's
`ChatMessagesOps.addMessage` / `addMessages` — the row insert plus the chat
metadata side-effect. The write marshaling is the inverse of sub-unit 3 but
harder: the port must reproduce `ChatEventSchema.parse`'s output bytes itself —
materialize each Zod `.default(...)` (`attachments` → `[]`, a `DangerFlag`'s
`userOverridden`/`wasRerouted` → `false`) and emit every JSON-column object in
schema field order (matching v4's `JSON.stringify` of a Zod-parsed object) with
integer-valued nested numbers rendered bare (`1`, not `1.0`), since the stored
bytes are compared directly. Each fixed-shape nested object (`dangerFlags`,
`reasoningSegments`, `hostEvent`, `customAnnouncer`, `carinaMeta`,
`summaryAnchor`, `pendingExternalAttachments`) is a typed struct in schema order;
the open-JSON `rawResponse` is corpus-constrained to `{}`/single-key (seam #5). A
`message` insert names the `MessageEvent` columns (always writing `attachments`);
a `context-summary`/`system` insert omits `attachments` so SQLite fills its
`DEFAULT '[]'` — matching v4's insert-only-validated-keys behavior. The metadata
side-effect recounts visible messages (`countVisibleMessages`), bumps
`lastMessageAt`/`updatedAt` to a minted `now` only for an actual `type:'message'`
event, and folds `spokenThisCycleParticipantIds` over the batch via the
already-ported `computeSpokenThisCycleAfterMessage`; it routes through the
sub-unit-1 `chats.update` (extended with `lastMessageAt` +
`spokenThisCycleParticipantIds` setters). Verified by a tier-2 differential
driving v4's REAL `addMessage`/`addMessages` over a kitchen-sink message (every
JSON column), a context-summary (non-actual: no `lastMessageAt` bump, `updatedAt`
preserved, count 0), and a mixed batch (whisper + system event + public message),
diffing BOTH the `chat_messages` and `chats` tables. `chat_messages` is pinned;
the `chats` `lastMessageAt`/`updatedAt` collapse to `<ts>` only when they differ
from the seed sentinel (so a preserved-sentinel `updatedAt` stays pinned and a
stray mint would be caught). The differential caught a real bug: serde's
`camelCase` rename produced `estimatedCostUsd`, dropping the schema's
`estimatedCostUSD` value — fixed with an explicit rename.

The `chats` repo — sub-unit 4b: the **`chat_messages` mutation path**
(`db::chats_messages`, `chats_messages_ops_tier2_equivalence`). Ports v4's
`updateMessage` / `deleteMessagesByIds` / `clearMessages`. `updateMessage`
reproduces v4's `{...existing, ...updates}` → `ChatEventSchema.parse` →
`$set: validated`: it reads the existing event (reusing the sub-unit-3 read),
overlays the update keys, re-validates into the typed `ChatEventInput`, and
DELETE + re-INSERTs the merged event — which yields the byte-identical row
(a validly-created row's non-member columns already sit at their DDL defaults, so
resetting them is a no-op) while reusing the 4a insert marshaling. A
freshly-added `dangerFlags` bakes its defaults; an untouched `reasoningSegments`
round-trips byte-for-byte; a context-summary's `attachments` stays at its
`DEFAULT '[]'`; a not-found id no-ops. `deleteMessagesByIds` deletes each
`(id, chatId)` row and, when any were removed, recounts `messageCount` (so
`update` preserves `updatedAt`); a nonexistent id removes nothing and leaves
metadata untouched. `clearMessages` deletes all of a chat's rows and resets
`messageCount`→0 + `lastMessageAt`→null (`updatedAt` preserved). Verified by a
tier-2 differential driving v4's REAL methods over a seed of three chats
pre-populated via `addMessages`, diffing BOTH the `chat_messages` and `chats`
tables with ZERO normalization — no 4b op mints a chat timestamp, so the seed's
baked timestamps are read identically by both sides.

The `chats` repo — sub-unit 5: the **participant ops** (`db::chats_participants`,
`chats_participants_tier2_equivalence`). Ports v4's `ChatParticipantsOps`:
`addParticipant` / `updateParticipant` / `removeParticipant` /
`setParticipantStatus` plus the four pure in-memory filters
(`getCharacter`/`getActive`/`getLLMControlled`/`getUserControlled`Participants).
Each mutator is a read-modify-write of the `participants` JSON column —
`findById` → mutate the array in memory (minting the participant's own
id/createdAt/updatedAt) → `update` the chat — and the chat's OWN `updatedAt` is
never bumped (v4 `_update` preserves it; the minted clock values live inside the
participants JSON). `addParticipant` validates through the participant schema
(materializing the Zod defaults, stripping unknown keys) and carries the
user-control side-effect (a `controlledBy: 'user'` participant is appended to
`impersonatingParticipantIds` and, when nobody is typing, set as
`activeTypingParticipantId`); `removeParticipant` carries the last-participant
guard (throws, leaving the chat unmutated). Banks the `removedAt` three-shape
seam: absent (never removed), the minted string (removed), and an explicit JSON
`null` (a `setParticipantStatus` to a non-removed status clears it) — which
forced widening `ChatParticipant.removedAt` to a double-`Option` with a
present-keeps-null deserializer (plain serde maps a stored `null` to the outer
`None`, dropping it; v4's Zod `.nullable().optional()` keeps it through a re-read
+ re-write). Tier-2 differential drives v4's REAL ops (with `setParticipantStatus`
reached via the private ops field — not on the repository surface) over four
seeded chats, diffing the `chats` table; participant ids (pinned seed + minted)
are remapped to first-appearance tokens across the three referencing cells, and
nested participant timestamps are sentinel-placeholdered (a value equal to the
seed sentinel stays pinned — proving createdAt preservation and no stray mint),
while chat-level timestamps are diffed exactly.

Phase-2 on-ramp — the real-snapshot fixture sanitizer (Deliverable B), a new
`quilltap-fixture-sanitizer` crate (library + `--source/--dest/--verify` CLI). It
takes a COPY of a real instance, recovers the pepper from the copy's `.dbkey` (in
memory only — never printed, logged, or written), sanitizes each database, and
re-keys the output under the committed throwaway test pepper. It is schema-frozen
by construction: the destination schema is replayed verbatim from the source's own
`sqlite_master`, every row is copied (row counts + the FK-id graph preserved), and
numbers / 0-1 booleans / enum tokens (by name + the `*Type`/`*Status`/`*Kind`/
`*Mode`/`*Role` suffixes) / timestamps / ids + UUID-valued TEXT are kept, while all
other TEXT is scrubbed to deterministic same-length pseudo-text, JSON columns are
deep-scrubbed to stay valid (keys / numbers / bools / uuid-and-enum leaves kept),
BLOBs become deterministic same-length bytes, and the document store's content↔sha
invariant is recomputed so a scrubbed file's `sha256` still matches its bytes.
Document-store PATH strings keep their structural skeleton (folder names + the
managed vault filenames like `properties.json`) so a sanitized vault still resolves,
scrubbing only the title stems. The scrub is one-way (`SHA-256(column ‖ original)`,
the original never appears in the output) and equality-preserving (identical
originals map identically, keeping content-dedup relationships). The binary refuses
a source path that looks like a live instance and never writes the `.dbkey`. Per the
project decision (2026-07-01) NO Friday-derived data is committed — the committed
test is synthetic (a re-key A→B round-trip proving the policy: structure preserved,
free text / JSON / BLOB scrubbed, content↔sha recomputed); real snapshots are
regenerated locally on demand. Verified locally against a copy of Friday: 188,031
main-db rows + 20,772 mount-index rows sanitized and re-keyed, 3,400 document-store
files re-hashed, and the sanitized output read back through the ported repos —
20,868 memories, 609 chats, and 33 characters (through the full vault overlay,
which resolves because the structural path segments are preserved) — marshaling
cleanly against real-shaped rows.

Phase-2 deferred-seam closure — ported the `characters` startup-backfill family,
closing the last three characters deferrals: the `ensureCharacterVault` adopt
branch, provision-on-the-fly, and physicalDescription-via-update. On a
managed-field `update` to a vault-less character, `apply_document_store_write_overlay`
now provisions a vault on the fly (build the post-cutover write input →
`ensure_character_vault` → re-read + confirm FK → continue routing) instead of
erroring. `ensure_character_vault` now first searches for a populated same-name
`'character'` store (`doc_mount_points::find_by_name` — `enabled=1`, trimmed
case-insensitive match) that passes the new `vault_has_required_files` check (all
six required files present in `doc_mount_file_links`) and adopts it when exactly
one qualifies (ambiguous or zero → fresh provision); the FK-write-and-confirm is
factored into the shared `link_character_to_vault`. The two seams compose — a live
`update` is how a character reaches the adopt branch. physicalDescription-via-update
(writing `physical-description.md` + `physical-prompts.json` on a non-null patch and
stripping it from the DB patch) was already coded; it is now proven. Each seam
ships a green six-table cross-DB shared-id-map remap differential
(`characters_adopt` / `characters_provision` / `characters_physical`
`_tier2_equivalence`) driving v4's REAL `repos.characters.update`/`.create`; the
adopt case asserts a single surviving mount point (the orphan store reused and its
FK relinked, no duplicate). Added `doc_mount_points::find_by_name` and
`doc_mount_file_links::relative_paths_lower`.

Phase-2 deferred-seam closure — closed the WRITE side of the
`chat_messages.isSilentMessage` seam (#8), completing it. The read side was
already resolved; this closes the write. A `message`-type insert now emits the
same TEXT-affinity bytes v4 stores: `true` → `"1.0"`, `false` → `"0.0"`, absent →
`NULL`. That representation arises because v4's `prepareForStorage(bool)` returns
the JS number `1`/`0`, better-sqlite3 binds it as a REAL, and SQLite converts the
REAL to text on store (`"1.0"`) — confirmed by a raw better-sqlite3 probe. The
Rust binding reproduces it by binding `Some(1.0_f64)` / `Some(0.0_f64)` / `None`;
context-summary / system inserts still omit the column so SQLite fills its DDL
default. Verified by a new `chats_messages_tier2` `addMessages` op carrying both a
`true` and a `false` silent message, byte-compared in the pinned `chat_messages`
dump against v4's REAL `addMessages`.

Phase-2 deferred-seam closure — ported the PUBLIC wardrobe write path (seam #7):
v4's `WardrobeRepository.create`/`update`/`delete`, in the new
`quilltap-core::db::vault_wardrobe_public`. These are v4's vault-only overrides —
resolve the owning character's document-store mount, read the current
`Wardrobe/*.md` items, apply the change, cycle-check, and re-project the folder,
throwing when no mount resolves (there is no SQL mirror). The prior
`wardrobe_tier2` port verified only the legacy base-SQL marshaling; this ports the
composition itself, over the already-verified leaves (`read_character_vault_wardrobe`
+ `project_vault_wardrobe` + `detect_component_cycles` + characters
`find_by_id_raw`), including the read-modify-project round-trip, the minted-`updatedAt`
on update, and the `assertNoCycles` guard (v4's exact `… → …; …` message). Verified
by a **read-back differential** (`vault_wardrobe_public_equivalence`) driving v4's
REAL public repo over a baked character+vault fixture: create, a composite create
referencing the first by id, a rename update, a cycle-forming update that throws, a
real delete (with the surviving composite's now-dangling ref DROPPING on read), a
delete of the already-gone id returning false, and a create against a non-existent
character that throws no-mount — comparing each op's read-back item list (minted
`updatedAt` normalized). A read-back tier rather than a table dump because
`build_wardrobe_item_file` writes the item's minted `updatedAt` into the
content-addressed `.md`, which a byte-level dump can't normalize; the projection
primitive is separately byte-verified (`vault_wardrobe_write_equivalence`). Scope:
the character tier only — the General/project archetype tiers stay deferred (same
boundary as `read_character_vault_wardrobe`). Four unit tests cover the patch merge,
cycle rejection, and the read→item conversion.

Phase-2 deferred-seam closure — ported the write applier's `__finalizeFile` +
post-commit side effects (seam #4), the last deferred pieces of
`quilltap-core::write_apply`. `__finalizeFile` now runs inside the main-DB
transaction loop (ensure-dir + staging→final rename), tracked so a later failure
in that partition undoes the renames in reverse before rethrowing; `cleanupStagingDirs`
drops the per-job `.staging/<jobId>` shell post-commit; and `dispatchInvalidations`
fires the deduped, ordered vector-store / mount-cache targets post-commit (both
skipped when the batch throws). The engine keeps v4's orchestration-vs-effect
split — the pure path/target computation (`path_dirname` = Node posix `dirname`,
`find_staging_root`, `collect_invalidations`) lives in the engine; the fs/cache
ops route through four new `ApplyHost` methods (production wires real fs/IPC; the
harness records them). The `write_apply_equivalence` trace differential grew four
observable fields (renames incl. undo-on-rollback, mkdirs, staging cleanup,
invalidation notifications) and three scenarios, verified against v4's REAL
`applyWritesUnsafe` — the oracle now records the fs mutators via jest `fs` mock +
the `notifyChild` mock (12 scenarios green). Also added four `write_apply` unit
tests.

Phase-2 deferred-seam closure — closed the `chat_messages.isSilentMessage` seam
(#8), and corrected its premise. The deferral claimed the TEXT-affinity round-trip
(`z.union([boolean, number.transform])` → TEXT) made v4's `getMessages` DROP a
silent message. Probed empirically against v4: it does NOT — a written `true` is
stored as numeric TEXT (`"1.0"`), and the read applies the row-schema union
(coerce to number, `=== 1`) → a real boolean, so the message is KEPT with
`isSilentMessage: true`. The real gap was that `db::chats_messages_read` never read
the column and so omitted the field. Fixed by reading `isSilentMessage` and
reproducing the coercion (numeric-TEXT `=== 1.0` → bool; `NULL` → omitted); the
read corpus gained a silent-message row proving the output matches the oracle. (The
write side does not yet emit the `"1.0"` representation — a bounded follow-up, since
the write corpus never sets it.)

Phase-2 deferred-seam closure — ported `TagVisualStyleSchema`'s per-field defaults
(seam #3). v4's base `_create` runs the doc through `TagSchema.parse`, so a PARTIAL
`visualStyle` gets its missing fields materialized; the Rust `TagVisualStyle` now
carries serde defaults matching each Zod `.default(...)` (`foregroundColor` →
`#1f2937`, `backgroundColor` → `#e5e7eb`, the four bools → `false`). `emoji`
(`.optional().nullable()`, no default) gained a double-`Option` + present-keeps-null
deserializer for the absent-vs-null trichotomy (absent → dropped as v4 `undefined`;
explicit `null` → kept). Proven by two partial-style tags corpus creates —
`{ bold: true }` (emoji dropped, all six defaults expand) and `{ emoji: null,
italic: true }` (emoji null kept) — each byte-identical to the oracle.

Phase-2 deferred-seam closure — closed the `toLowerCase` case-mapping seam
(`tags.nameLower`, `text_replacement_rules` conflict detection) by proving
`str::to_lowercase` is byte-identical to JS `String.prototype.toLowerCase`. Both
implement locale-independent Unicode default case mapping; verified empirically on
every gnarly case — `İ` → `i` + combining dot (`0069 0307`), a FINAL `Σ` → `ς`
(the context-sensitive Final_Sigma rule), `ß` (unchanged), `É`→`é`, and titlecase
digraphs (`ǅ`→`ǆ`). The evaluated `icu_casemap` option is therefore unnecessary —
no code change, just differential proof: the `tags` tier-2 corpus gained a tag
named `İSTANBUL ÉCOLE ΣΟΦΟΣ Straße` (whose stored `nameLower` matches the oracle
byte-for-byte), and `text_replacement_rules` a non-ASCII case-insensitive conflict
pair (`Café` then `CAFÉ`, both lowercasing to `café`) that fires duplicate
rejection identically on both sides. With the collation seam (above) this closes
the whole Unicode-fidelity cluster.

Phase-2 deferred-seam closure — added ICU collation (`icu` 2.2, ICU4X) as
`quilltap-core::collation::locale_compare`, closing the `localeCompare` seam. v4
sorts several lists with `a.localeCompare(b)` (no locale) — true ICU collation,
not the code-unit order Rust's `str: Ord` gives. Node's no-arg `Intl.Collator`
resolves to en-US / tertiary (probed against ICU 78); `Collator::try_new` returns
a `CollatorBorrowed<'static>` over the baked compiled data (held in a `LazyLock`),
and ICU4X's tables match Node's for common Latin + accents (verified the order
`a,A,ä,b,B,e,é,z,Z` and the pairwise signs). The two ported `localeCompare` sites
now use it — `compareVersions`' malformed-input fallback and `canonicalize`'s
tool-name array sort — and each differential gained a divergent row (mixed
case/accents, e.g. `apple` < `Banana`) that exercises the ICU path against the
oracle, where code-unit order would disagree. The `canonicalize` `parameters`
key-sort stays code-unit (v4 uses `Object.keys().sort()` there, not collation).
Future Phase-3 name sorts reuse `locale_compare`. (The `toLowerCase` case-mapping
seam is separate and closed next.)

Phase-2 deferred-seam closure — proved the open-JSON multi-key key-order fix (#5)
end-to-end. With `preserve_order` enabled (below), a MULTI-KEY value in
deliberately NON-SORTED key order was added to each affected corpus and its
differential re-run green, confirming the port emits v4's `JSON.stringify`
insertion order rather than sorted keys: `plugin_config.config`,
`character_plugin_data.data`, `image_profiles.parameters`,
`connection_profiles.parameters`, `chat_settings.tagStyles`, `chats.state` +
`chats.sillyTavernMetadata`, and `chats_outfits.equippedOutfit` (a key-order chat
that appends a higher-sorting characterId before a lower one). Refreshed the
now-stale `chats_outfits` doc comment (it described the pre-`preserve_order`
sorted-key seam). Corpus-only; no Rust logic change.

Phase-2 deferred-seam closure (begins) — enabled `serde_json`'s `preserve_order`
feature workspace-wide (both crates), so every `Value::Object` is an `IndexMap`
emitting INSERTION order, matching v4's `JSON.stringify`. This is the locked
decision for the open-JSON multi-key key-order seam (`parameters` / `config` /
`equippedOutfit` / `sillyTavernData` / `state` / `tagStyles` / `data` / …), which
the typed-struct trick could not cover. Foundational + no-regression: the full
suite stays green (the existing single-key corpora are order-invariant), and it
makes the harness stricter — a re-serialized `Value` now preserves on-disk key
order instead of sorting, so a masked key-order difference would surface (none
did). Per-column multi-key corpus proofs follow as each affected repo is swept.

The `chats` repo — sub-unit 6: the **remaining four ops files**, ported in
parallel (four agents, each on its own new module + differential; the shared
`ChatUpdate` setters + `mod.rs` wiring pre-staged serially). This **completes the
`chats` capstone** — the entire `ChatsRepository` public surface is now ported.
- **impersonation** (`db::chats_impersonation`, `chats_impersonation_tier2_equivalence`):
  v4 `ChatImpersonationOps` — `addImpersonation`/`removeImpersonation`/
  `getImpersonatedParticipantIds`/`setActiveTypingParticipant`/
  `updateAllLLMPauseTurnCount`. RMW on `impersonatingParticipantIds` +
  `activeTypingParticipantId` (the activeTyping reassign-or-clear on remove) +
  `allLLMPauseTurnCount`; mints nothing, so the differential is zero-normalization.
- **tokens** (`db::chats_tokens`, `chats_tokens_tier2_equivalence`):
  v4 `ChatTokenTrackingOps`. `incrementTokenAggregates` lowers v4's `$inc`/`$set`
  to one self-referential `UPDATE … SET col = col + ?` with an unconditionally
  minted `updatedAt` and a conditional `estimatedCostUSD = current + cost` (+
  `priceSource`); `resetTokenAggregates` zeroes the counters + nulls the cost via
  `update` (preserving `updatedAt`). Sentinel-aware `updatedAt` normalization
  (increment mints → `<ts>`; reset preserves → pinned, diffed exactly).
- **search** (`db::chats_search`, `chats_search_equivalence`):
  v4 `ChatSearchReplaceOps` — `countMessagesWithText`/`findMessagesWithText`/
  `searchMessagesGlobal`/`replaceInMessages`. The `searchMessagesGlobal`
  `$regex`→SQL `LIKE` translation reuses `memories`' exact mangling
  (`escapeRegex` → `source.replace(/\.\*/g,'%').replace(/\./g,'_')`, bare `LIKE`,
  no `ESCAPE`), reproducing v4's broken-but-exact behavior on regex-special
  inputs; the role filter + `createdAt DESC` + `limit`; and the split/join
  replace-all (which mints nothing). Read-differential over the method results +
  the post-replace `chat_messages` dump.
- **outfits** (`db::chats_outfits`, `chats_outfits_tier2_equivalence`): v4's
  `getEquippedOutfit`/`getEquippedOutfitForCharacter`/`setEquippedOutfit`/
  `removeEquippedItemFromAllChats` (in `chats.repository.ts`). RMW on the
  `equippedOutfit` JSON column, stored as **raw `Value`** (v4 never re-validates
  it through Zod), so partial / extra-key slots objects are preserved verbatim —
  the remove path mutates each character's slots in place, dropping the item only
  from slots it was actually in (v4's `before.includes` guard), never
  materializing absent slots. Corpus banks a partial-slot character to prove the
  shape-preservation. **Tracked seam:** the open-JSON key-order divergence
  (`serde_json::Value` sorts; v4 emits insertion order) — corpus constrained to
  sorted key order, same as `parameters`/`sillyTavernData`.

Build — extracted the SQLite3MC (ChaCha20/sqleet) amalgamation into a dedicated
`quilltap-sqlite3mc-sys` crate (its `build.rs` + `vendor/`, moved out of
`quilltap-core`). Cargo's build-script fingerprint includes the package version,
so the per-commit version bump on `quilltap-core` used to throw away the cached
`libsqlite3.a` and recompile the 12 MB amalgamation from scratch (~4 min). The
sys crate's version is pinned, so that C compile now caches across our version
bumps: a `quilltap-core` version bump rebuilds in ~2 s instead of ~4 min. No
`links` key (libsqlite3-sys already claims `sqlite3`); `quilltap-core` depends on
the sys crate and references it as `use quilltap_sqlite3mc_sys as _;` so its
link-search flags reach the final binary. Cipher behavior unchanged, verified by
the tier-2 differentials still opening real ChaCha20 databases.

Phase 2 — the `memories` repo, ported whole
(`quilltap-core::db::memories` + `db::memories_read`). A plain main-DB
`AbstractBaseRepository<Memory>` (no overrides except the `embedding` BLOB
registration, no vault overlay), so every read is a single-connection SELECT +
marshal. Ports the full surface: the write/mutation side (`create` with embedding
BLOB + JSON-array columns + the three numeric columns — `importance` /
`reinforcedImportance` are INTEGER-affinity, `reinforcementCount` REAL, all bound
`f64`; `update` leaving the BLOB untouched; `delete`; `updateForCharacter` /
`deleteForCharacter` ownership gates; `bulkDelete`; `updateAccessTime{,Bulk}`;
`replaceInMemories`; `deleteByChatId` / `deleteBySourceMessageId{,s}`) and the
read side (all ~30 `findBy*` / `count*` queries, incl. the `$regex` → SQL `LIKE`
mangling reproduced byte-for-byte, the `findByCharacterAboutCharacters` window
function, `findByCharacterIdPaginated`'s in-memory search, and the importance
tiers). Banks a marshaling seam: the normal `findByFilter` path omits NULL
nullable-optional columns (v4's `undefined` dropped by `JSON.stringify`), but the
raw-SQL `findByCharacterAboutCharacters` path keeps them as `null` (its rawQuery
rows carry explicit NULLs that `MemorySchema.safeParse` retains) — the port
mirrors both. Verified two ways: a tier-2 differential (`memories_tier2_equivalence`,
the write/mutation sequence, minted-timestamp placeholder form) and a
read-differential (`memories_read_equivalence`, 39 queries over a v4-baked fixture,
zero normalization — nothing mutated, so no minted timestamp; a returned
embedding is the `Float32Array` `{"0":…}` object rebuilt from the BLOB).

Phase 2 — the `CharactersRepository` read path
(`quilltap-core::db::characters_read`), characters sub-unit 4c — the capstone's
last piece. Ports the slim-row read marshaling (row → `Character`, the inverse of
sub-unit 2's write marshaling = v4 `hydrateRow` + Zod parse) + the `findBy*`
queries, each overlaying the character vault. The marshaling reproduces v4's net
read shape over the slim columns: required strings present; `.nullable().optional()`
TEXT/UUID/JSON cells **omitted** when `NULL` (v4 emits `undefined`, dropped by
`JSON.stringify`) and parsed when present; `.default(false)` booleans coerced from
INTEGER; `.nullable().optional()` booleans omitted/coerced; `.default([])` arrays
parsed (`NULL`/empty → `[]`); `controlledBy` defaulting to `'llm'`. The managed
columns sit at their DDL defaults, so it reproduces their Zod defaults directly
(`scenarios`/`systemPrompts`/`aliases` → `[]`, `talkativeness` → `0.5`, the nullable
managed fields omitted); for a vault-linked character the read overlay then
overwrites every managed field. Queries: `find_by_id` / `find_by_id_raw` /
`find_all` / `find_by_user_id` / `find_user_controlled` / `find_llm_controlled` /
`find_by_ids` / `find_by_default_image_id` / `find_by_avatar_override_image_id` /
`find_by_tag` (the last two via SQLite `json_each`, matching v4's query translator).
Verified by a read-differential (`characters_read_equivalence`): both sides READ a
copy of one fixture baked by v4's REAL create (four characters + vaults), run the
same 11 queries, and compare the hydrated lists exactly (ids/timestamps identical —
no remap — only `physicalDescription`'s read-minted createdAt/updatedAt
placeholdered, lists sorted by id). `findByIdRaw` isolates the slim marshaling (no
overlay). Also refactored sub-unit 4b's array ops to ride this full `find_by_id`
(re-verified green), closing the scoped-reader deferral.

Phase 2 — the `CharactersRepository` array / sub-array ops
(`quilltap-core::db::vault_character_arrays`), characters sub-unit 4b. Ports the
`systemPrompts` / `scenarios` / `partnerLinks` mutators + the
`setFavorite` / `setControlledBy` / `setCanBeCarina` setters. Each sub-array op is
v4's three-beat shape: `find_by_id` (the read overlay) → mutate the array in memory
(applying the per-op `onBeforeAdd` / `onAfterBuild` / `onAfterRemove` default
normalization) → `update_character` (the 4a write overlay) reprojects the
`Prompts/` / `Scenarios/` folder (or writes the slim `partnerLinks` column). The
minted item `id` / `createdAt` / `updatedAt` never reach disk — the projection
writes `<sanitize(name|title)>.md` from `build_system_prompt_file` /
`build_scenario_file`, and the read side re-derives a prompt's id from its path —
so the DB effect is deterministic. Added a scoped `find_by_id` (the slim columns
the ops consume — `id` / `characterDocumentMountPointId` / `partnerLinks` — plus
the overlaid `systemPrompts` / `scenarios`; full slim-row read marshaling is
sub-unit 4c). The setters are thin `update_character(id, { … })` wrappers (no read,
no vault). Verified by a tier-2 differential (`characters_arrays_tier2_equivalence`)
over a fixture baked by v4's REAL create (one baked prompt / scenario / partner
link), driving v4's REAL repository methods across SIX tables in the
shared-cross-db-id-map remap form (`chunkCount`/`doc_mount_chunks` pinned/excluded);
the id-taking prompt/scenario ops carry a `targetName` / `targetTitle` resolved to
the current id via `findById` on each side. Banks addSystemPrompt (default-demote +
non-default), updateSystemPrompt (rename → sweep + content), setDefaultSystemPrompt,
deleteSystemPrompt (deleting the default → survivor promotion), the three scenario
ops, the two partner ops, and the three setters.

Phase 2 — `applyDocumentStoreWriteOverlay` + the `CharactersRepository.update`
integration (`quilltap-core::db::vault_character_update`), characters sub-unit 4a.
The managed-field write **router** — distinct from sub-unit 1's create-time writer
(which projects every field unconditionally): the update path routes only the
fields **present in the patch**, and `properties.json` is a **read-modify-write**
(a patch touching only `title` preserves pronouns/aliases/firstMessage/
talkativeness). Routes markdown (`None`→`""`), the properties RMW (seeded from the
current `properties.json`, falling back to the empty-managed default), physical
(non-null writes the two files; null leaves them), and `systemPrompts`/`scenarios`
(reproject the folder — sweep + write). Returns the unmanaged remainder;
`update_character` runs the slim `_update` for it (skipped when empty — a
managed-only update does NOT bump the slim row's `updatedAt`). The DB-bound
remainder is marshaled back through the slim repo's typed update. Verified by a
tier-2 differential (`characters_update_tier2_equivalence`) over a fixture baked by
v4's REAL create, driving v4's REAL `repos.characters.update` across SIX tables
(slim `characters` row + the five store tables) in the shared-cross-db-id-map remap
form (`chunkCount`/`doc_mount_chunks` pinned/excluded). Banks markdown routing, the
properties RMW preserving untouched keys (asserted), a DB-only field update
(`isFavorite` true→false → slim `_update`), and a `systemPrompts` reprojection
(sweep the old `Prompts/Default.md`, write the new one) on a managed-only update —
the orphan-on-rewrite + sweep-GC row counts matching v4 byte-for-byte via the
shared DDL. Added the public `render_properties_json` (the RMW serializer, reusing
the create-time `properties.json` shape + the `talkativeness` js-number rule) and
`DocMountFileLinksRepository::ensure_folder_path`'s sibling read
`link_exists_at_path` (used by 3a). **Tracked deferral:** provision-on-the-fly (a
patch with managed fields on a vault-less character) — the corpus always has a
vault; lands with the startup-backfill slice.

Phase 2 — `ensureCharacterVault` + the `CharactersRepository.create` integration
(`quilltap-core::db::character_vault`), characters sub-unit 3b — the store-backed
capstone's keystone. `create_character` runs v4's full create end-to-end: the
slim-row `_create` (FK nulled — a fresh character always provisions a fresh vault),
then `ensure_character_vault` mints a `<name> Character Vault` mount point
(mount-index DB), scaffolds its preset structure, projects the managed fields
(`write_character_vault_managed_fields`, sub-unit 1), and links it by setting
`characterDocumentMountPointId` on the slim row (main DB) — confirming the write
stuck (v4's `linkCharacterToVault` turns a silent "linked but not linked" into a
loud error). A character spans two databases, so the differential
(`characters_create_tier2_equivalence`) drives v4's REAL `repos.characters.create`
and diffs SIX tables — the main slim `characters` row + the mount-index store
tables (`doc_mount_points` / `_folders` / `_files` / `_documents` / `_file_links`)
— in the shared-cross-db-id-map remap form (nothing pinned; every id minted, FKs
verify by relationship; timestamps placeholdered; the link `chunkCount`
pinned and `doc_mount_chunks` excluded, as for groups/projects). Banks the 6-step
create, the **orphan-on-rewrite** default-`properties.json` file/document row (the
scaffold writes it, then the managed bag overwrites it; `writeDatabaseDocument`
does no GC, so the old row persists — 9 files, 8 live + 1 orphan), the five
identity markdown overwrites (the `physical-*` scaffold defaults survive — no
physicalDescription), and one systemPrompt + one scenario projected into `Prompts/`
+ `Scenarios/` (10 links). **Tracked deferral:** the `ensureCharacterVault` adopt
branch (startup-heal of a hand-linked same-name store) — the corpus always
provisions fresh; it needs a richer `doc_mount_points` read and lands with the
startup-backfill slice.

Phase 2 — `scaffoldCharacterMount` (`quilltap-core::db::character_vault`),
characters sub-unit 3a (the store-backed capstone's stateful provisioning glue,
mount-index DB). Populates a freshly-created database-backed character store with
the preset structure: seven empty top-level folders (Prompts/Scenarios/Wardrobe/
Outfits/lore/images/files), six blank Markdown files
(identity/description/manifesto/personality/physical-description/example-dialogues,
content `""`), and two seeded JSON files (`properties.json` +
`physical-prompts.json`, FIXED default content). The six blank files share the
empty-string content sha, so they dedup to ONE `doc_mount_files` /
`doc_mount_documents` row with six distinct links; result: 7 folders, 3 files, 3
documents, 8 links. All writes go through the verified storage primitive — folders
via the new `DocMountFileLinksRepository::ensure_folder_path` (v4 `ensureFolderPath`,
walks the path directly so a single segment makes one root folder; a sibling of
`ensure_link_folder_id` which walks a file's dirname), files via
`write_database_document` (idempotent, skip-if-link-exists). Verified standalone
(the create flow's `writeCharacterVaultManagedFields` overwrites the five identity
markdown files + `properties.json`, so the create differential would mask the
scaffold defaults — verifying here pins the default bytes). Tier-2 differential
(`characters_scaffold_tier2_equivalence`) drives v4's REAL `scaffoldCharacterMount`
and diffs five mount-index tables (points / folders / files / documents / links) in
the shared-cross-table-id-map remap form; the seeded `mountPointId` is pinned, the
link `chunkCount` (a `reindexSingleFile` artifact) pinned and `doc_mount_chunks`
excluded (as for groups/projects).

Phase 2 — the `characters` repo **slim-row marshaling**
(`quilltap-core::db::characters`), the first sub-unit of v4's
`CharactersRepository` (the store-backed capstone). Ports the base-repository SQL
CRUD (`_create`/`_update`/`_delete`) over the MAIN-db `characters` table. v4's
public `create`/`update` orchestrate the character vault (provision + project +
overlay) — a later sub-unit; both strip the `MANAGED_FIELDS` set (identity,
description, manifesto, personality, exampleDialogues, pronouns, aliases, title,
firstMessage, talkativeness, physicalDescription, systemPrompts, scenarios) before
the SQL write, leaving the non-managed "slim row" this differential checks. A
fresh fixture's table still has the managed columns (`ensureCollection` generates
them from `CharacterSchema`), but both sides omit them from every write, so they
sit at their DDL defaults identically. Banks the **widest nullable-boolean surface
in Phase 2** — seven `z.boolean().nullable().optional()` columns
(`defaultAgentModeEnabled`, `defaultHelpToolsEnabled`, `canDressThemselves`,
`canCreateOutfits`, `systemTransparency`, `coreWhisperEnabled`, `canBeCarina`),
INTEGER 0/1 when present, SQL NULL when absent — plus a typed JSON-object column
(`defaultTimestampConfig`, a nine-field struct in schema order so the compact JSON
matches `JSON.stringify` key order, NOT `serde_json::Value`), an open JSON column
(`sillyTavernData`, kept `null`/single-key per the multi-key seam), two
typed-struct array columns (`partnerLinks` `{partnerId,isDefault}`,
`avatarOverrides` `{chatId,imageId}`), a string-array column (`tags`), two
boolean-default columns (`isFavorite`/`npc`), an enum TEXT column (`controlledBy`),
and many nullable UUID columns. `update` is a partial `SET` that reproduces v4's
full `$set` on-disk result (the fixture cells are already in validated canonical
order). Verified by a tier-2 differential (`characters_slim_tier2_equivalence`)
driving v4's REAL protected internals via a thin subclass over a create / create /
update / delete sequence, diffing the `characters` table in the pinned
zero-normalization form (ids + timestamps pinned both sides).

Phase 2 — the `background_jobs` repo (`quilltap-core::db::background_jobs`), v4's
`BackgroundJobsRepository` — the durable work queue (memory extraction, context
summaries, embedding generation, autonomous room turns, …). A
`UserOwnedBaseRepository` (a `userId` column) with NO base-method override, so
`create`/`update`/`delete` honor pinned id/createdAt/updatedAt; on top of CRUD it
ports the full queue API. Banks three **REAL-affinity** number columns
(`priority`/`attempts`/`maxAttempts` — all bare `z.number().default(N)` → REAL,
NOT INTEGER; integer-collapsed in the dump) and the open-JSON `payload` column
(kept `{}`/single-key per the multi-key key-order seam). Ports and verifies the
queue ops: `claimNextJob` (atomic `SELECT … ORDER BY priority DESC, createdAt ASC
LIMIT 1` then UPDATE in a transaction, `attempts += 1`), `markFailed` (exponential
backoff `min(30·2^attempts, 300)`s, DEAD-vs-FAILED on `attempts >= maxAttempts`),
`markCompleted`, `pause`/`resume`, `cancel`, `cancelByType`, `resetAllProcessingJobs`,
`resetStuckJobs`, and `deleteByTypesAndStatuses` — with the exact `lastError`
strings byte-for-byte (`"Cancelled by user"`, `"Superseded by new reindex"`, the
em-dash `"Orphaned on startup — killed"`, `"Timed out after N minutes"`). The
nested-JSON path finders (`findPendingForChat`/`ForEntity`) reproduce v4's
`json_extract(payload, '$.chatId')` translation. Verified by a tier-2 differential
(`background_jobs_tier2_equivalence`) driving v4's REAL repo over a 13-op sequence
and diffing the table in the minted-timestamp placeholder form (ids + createdAt +
every deterministic column — status/attempts/lastError/payload/priority/maxAttempts
— diffed EXACTLY; only the four mintable timestamp columns placeholdered).
**Discovered v4-on-SQLite limitation:** `markCompleted`'s dotted `payload.result`
merge throws `no such column: payload.result` on v4's SQLite backend (no dotted
JSON sub-key translator), so that path is unreachable there; the port keeps the
merge as a forward v5 capability (via the pure `merge_result_into_payload`, three
unit tests) and the differential exercises only the no-result path (v4's working
behavior).

Phase 2 — the `vector_indices` repo (`quilltap-core::db::vector_indices`), v4's
`VectorIndicesRepository`. The first **standalone two-table** repo — it does NOT
extend the base repository; it manages `vector_indices` (per-character metadata)
+ `vector_entries` (per-embedding rows) in the MAIN db directly. Banks the third
Float32-BLOB embedding column (little-endian via `embedding_blob::float32_to_blob`,
`None`/empty → SQL NULL, never a zero-length blob; dumped as hex for a bit-exact
compare), two REAL-affinity number columns (`version`/`dimensions`, bare
`z.number()` → REAL, integer-collapsed in the dump), and a `saveMeta` upsert keyed
by `characterId` (`id == characterId`, so the meta `id` is pinned, not minted).
Reproduces v4's exact op semantics: `addEntries` mints one shared `createdAt`
across the batch; `removeEntries` is a per-id delete loop (not a single `IN (…)`);
`updateEntryEmbedding` touches only the embedding column (no timestamp);
`deleteByCharacterId` is two independent ops (entries then meta), not one SQL
transaction. Verified by a tier-2 differential (`vector_indices_tier2_equivalence`)
driving v4's REAL repo over a full op sequence (saveMeta create/update, addEntry,
addEntries, updateEntryEmbedding, removeEntries, and a `deleteByCharacterId` that
wipes a second character entirely) and diffing both tables in the minted-values
remap form (entry `id` remapped, timestamps placeholdered, `characterId`/embedding
pinned).

Phase 2 — repo-by-repo over the real DB (each ported repo arrives with its
tier-2 case):

- `tags` repo (`quilltap-core::db::tags`): `create`, `update`, and `delete`
  ported from v4's `TagsRepository` + base-repo internals. Widens the tier-2
  marshaling surface past `folders`' all-strings shape — a boolean column
  (`quickHide` stored as INTEGER 0/1), a nullable JSON-object column
  (`visualStyle` stored as compact JSON in schema field order, reproduced with a
  typed struct so key order matches v4's `JSON.stringify` rather than a sorted
  map), and the `nameLower` derivation (`(nameLower || name).toLowerCase()` on
  create; re-derived from `name` on update). Adds the `delete` op to the harness.
- Harness: tier-2 differential test `tags_tier2_equivalence` plus its fixture
  builder + `tags-tier2` oracle case, driven by the committed
  `harness/oracle/fixtures/tags-tier2.json` (the create op carries a
  fully-specified `visualStyle` so no Zod inner-default expansion is involved).
  Ids and timestamps pinned both sides → zero normalization. The `tags` repo
  round-trips green (`QT_ORACLE_TAGS` + `QT_FIXTURE_TAGS`, skip-if-unset).
- Generated-UUID remap + timestamp-placeholder normalization (the tier-2
  machinery for ops that mint their own ids/clocks, not just the pinned-id sync
  path). `folders.create` now ports v4 `_create`'s minted-values defaults
  (`id = options?.id || generateId()`, timestamps `|| now`) and returns the id
  used, so a caller can wire it into a dependent op. New `quilltap-core::clock`
  (`now_iso` / pure `iso_from_unix_ms`) reproduces v4's
  `new Date().toISOString()` shape; `uuid` (v4) generates ids. Verified by the
  `folders_remap_tier2_equivalence` test: a parent + child created with NOTHING
  pinned, so both v4 and Rust mint different random UUIDs and timestamps. One
  normalization (in the harness) runs over both dumps — rows walked in
  natural-key (`path`) order, id columns (`id`, `parentFolderId`) collapsed to
  first-seen tokens (`ID_0`, `ID_1`), so the child→parent FK relationship is
  verified without pinning the literal id; timestamps placeholdered after
  asserting the `createdAt == updatedAt` create invariant per row. Round-trips
  green (`QT_ORACLE_FOLDERS_REMAP` + `QT_FIXTURE_FOLDERS_REMAP`, skip-if-unset).
- The partitioned write APPLIER (`quilltap-core::write_apply`) — the writer-task
  apply path ported from v4's `applyWritesUnsafe` / `applyPartition` /
  `applySecondaryBestEffort` / `applyFolderCreateIdempotent`. Sequences the pure
  `write_partition` leaves into the real orchestration: each partition (main /
  mount-index / llm-logs) commits in its own `BEGIN IMMEDIATE` transaction;
  main-primary jobs (`AUTONOMOUS_ROOM_TURN`) commit main first then apply
  secondaries best-effort (a dropped doc-store effect can't lose the chat turn),
  while idempotent jobs apply secondaries first so a secondary failure prevents
  the main commit; and the concurrent `docMountFolders.create` unique-conflict
  reconcile resolves to the existing row and remaps the discarded buffered folder
  id for the rest of the batch. The engine is generic over an injected
  `ApplyHost` seam (the three connections + repo dispatch + the reconcile
  lookup), mirroring how v4 unit-tests this orchestration with fakes.
- Harness: `write_apply_equivalence` — a tier-1-style TRACE differential over a
  committed 9-scenario corpus (`harness/oracle/fixtures/write-apply.json`). Both
  sides emit the same observable trace (per-partition exec sequence, ordered repo
  dispatches with post-remap args, reconcile lookups, resolved/threw outcome).
  The oracle (`harness/oracle/cases/write-apply.test.ts`) drives v4's REAL
  `applyWritesUnsafe` — it runs under v4's jest (not tsx) because the applier's
  `getRawDatabase()` / `getRepositories()` singletons are `jest.mock`-injected;
  v4's jest resolves the v5-tree oracle file via an extra `--roots`. Deferred
  (documented): `__finalizeFile` (fs rename + undo-on-rollback) and the
  post-commit `cleanupStagingDirs` / `dispatchInvalidations` side effects.
- `text_replacement_rules` repo (`quilltap-core::db::text_replacement_rules`):
  `create`, `update`, and `delete` ported from v4's
  `TextReplacementRulesRepository`. The first repo with **conflict detection** —
  and so the first to need a repo-level *read*: `create`/`update` scan the
  existing rows and reject a duplicate `(fromText, caseSensitive)` pair
  (case-sensitive rules compare `fromText` exactly, case-insensitive ones
  compare lowercased; the `caseSensitive` flag is part of the key, and `update`
  only re-checks when that pair changes). A conflict surfaces as
  `TrrError::Conflict`, the analogue of v4's `TextReplacementRuleConflictError`.
  Single-user (no `userId`). Widens the tier-2 marshaling surface past `tags`
  with a real INTEGER number column (`sortOrder`) and two boolean columns
  (`caseSensitive`, `enabled`).
- Harness: tier-2 differential `text_replacement_rules_tier2_equivalence` plus
  its fixture builder + `text-replacement-rules-tier2` oracle case, driven by the
  committed `harness/oracle/fixtures/text-replacement-rules-tier2.json`. The op
  sequence includes two conflicting ops flagged `expectThrow`: both the oracle
  (asserting v4 threw `TextReplacementRuleConflictError`) and the Rust port
  (asserting `TrrError::Conflict`) prove the rejection independently, and the
  final-state dump confirms the rejected writes left no trace (a port lacking the
  check would have diverged). Ids + timestamps pinned → zero normalization.
  Round-trips green (`QT_ORACLE_TRR` + `QT_FIXTURE_TRR`, skip-if-unset). The
  toLowerCase case-mapping seam (shared with `tags.nameLower`) gains a second
  site here — tracked in the deferred-seams list.
- Canonical dump: `js_number_to_json` — the dump's REAL-cell rendering now
  mirrors JS `JSON.stringify(number)`, collapsing an integer-valued double
  (`9.0` → `9`) so a REAL-affinity numeric column (e.g. `z.number().int()`,
  which SQLite stores as an 8-byte float) matches the oracle, where
  better-sqlite3 hands JS a `Number` and `JSON.stringify` drops the `.0`. First
  exercised by `text_replacement_rules`' `sortOrder`.
- `prompt_templates` repo (`quilltap-core::db::prompt_templates`): `create`,
  `update`, and `delete` ported from v4's `PromptTemplatesRepository` (built-in
  *seeding* is a startup concern, out of scope). Widens the tier-2 marshaling
  surface with the **first JSON array column** (`tags: z.array(UUIDSchema)` →
  compact JSON text, `["id"]` / `[]`; reproduced via `serde_json::to_string` of a
  `Vec<String>` — arrays are order-preserving, so no key-order subtlety) and
  several **nullable string columns** (`userId` null-for-built-in, `description`,
  `category`, `modelHint`). Adds the **built-in read-only guard**: `update`/
  `delete` read the target's `isBuiltIn` and refuse to mutate a built-in row,
  returning a not-modified result (`Ok(false)`; v4's `null` / `false`) rather
  than throwing — a read-then-guard pattern that suppresses the op instead of
  raising. Plain `AbstractBaseRepository` (nullable `userId`).
- Harness: tier-2 differential `prompt_templates_tier2_equivalence` plus its
  fixture builder + `prompt-templates-tier2` oracle case, driven by the committed
  `harness/oracle/fixtures/prompt-templates-tier2.json`. The op sequence
  exercises the array column on create and on update (replacing the array), the
  nullable columns (null vs present), and the guard two ways via an `expectNoop`
  flag — an update and a delete that both target the built-in seed row; both
  sides assert the op reported not-modified (Rust `Ok(false)`; oracle `null` /
  `false`) and the final-state dump confirms the built-in row stayed
  byte-identical. Ids + timestamps pinned → zero normalization. Round-trips green
  (`QT_ORACLE_PROMPT_TEMPLATES` + `QT_FIXTURE_PROMPT_TEMPLATES`, skip-if-unset).
- Three more plain-base repos ported in parallel (each `create` / `update` /
  `delete`, pinned form, its own tier-2 case round-tripping green):
  - `conversation_annotations` (`quilltap-core::db::conversation_annotations`):
    banks a **REAL-affinity unbounded-int column** — `messageIndex` is
    `z.number().int().min(0)` with no `.max()`, and v4's schema translator
    (`mapToSQLiteType`) only assigns INTEGER affinity when a numeric field has
    both an integer min and max, so it maps to REAL; bound as `f64`, the dump's
    `js_number_to_json` collapses the integer-valued cell back to a bare integer.
    Also a **nullable UUID column** (`sourceMessageId`). Harness
    `conversation_annotations_tier2_equivalence` (`QT_ORACLE_CONV_ANNOTATIONS` +
    `QT_FIXTURE_CONV_ANNOTATIONS`).
  - `provider_models` (`quilltap-core::db::provider_models`): banks **two
    nullable REAL number columns** (`contextWindow`, `maxOutputTokens` — both
    bare `z.number()`, no min/max → REAL), **two boolean-default columns**
    (`deprecated`, `experimental` → INTEGER 0/1), and **enum TEXT columns**
    (`provider`, `modelType`). The corpus supplies every column explicitly so no
    Zod create-time default is relied on. Harness
    `provider_models_tier2_equivalence` (`QT_ORACLE_PROVIDER_MODELS` +
    `QT_FIXTURE_PROVIDER_MODELS`).
  - `help_docs` (`quilltap-core::db::help_docs`): the **first tier-2 BLOB
    column** — `embedding` is a Float32 buffer (little-endian `f32` bytes via
    `embedding_blob::float32_to_blob`), with empty/null → SQL NULL and the dump
    emitting BLOBs as lowercase hex on both sides for bit-exact comparison
    (fixture uses only exactly-float32-representable values so the f64→f32 cast
    is lossless). Banks that a **text-only update preserves the BLOB**: the
    partial `UPDATE SET` never names the embedding column, mirroring v4's
    whole-row rewrite that re-persists the existing embedding unchanged. Harness
    `help_docs_tier2_equivalence` (`QT_ORACLE_HELP_DOCS` + `QT_FIXTURE_HELP_DOCS`).
- A second parallel batch of three repos (each `create` / `update` / `delete`,
  pinned form, its own tier-2 case round-tripping green):
  - `roleplay_templates` (`quilltap-core::db::roleplay_templates`): the **first
    array-of-objects JSON column** — `renderingPatterns: z.array(...)` stored as a
    compact JSON array of objects, each element modeled by a typed serde struct in
    schema field order (`#[serde(rename_all = "camelCase")]` + `skip_serializing_if`
    on the optionals) so the key order and omitted-optional behavior match v4's
    `JSON.stringify(zodParsed)` byte-for-byte — plus a **nullable JSON-object
    column** (`dialogueDetection`). `delimiters` is held empty and
    `narrationDelimiters` kept to its plain-string form (the discriminated-union /
    tuple forms buy no new marshaling coverage). No built-in guard ported (the
    corpus never mutates a built-in row). Harness
    `roleplay_templates_tier2_equivalence` (`QT_ORACLE_ROLEPLAY_TEMPLATES` +
    `QT_FIXTURE_ROLEPLAY_TEMPLATES`).
  - `image_profiles` (`quilltap-core::db::image_profiles`): banks the **Taggable
    lineage** (`userId` + a JSON `tags` array) and the first **open / arbitrary-
    JSON object column** (`parameters`, `z.record`), modeled as `serde_json::Value`
    → compact JSON text, plus boolean and nullable-string columns. Harness
    `image_profiles_tier2_equivalence` (`QT_ORACLE_IMAGE_PROFILES` +
    `QT_FIXTURE_IMAGE_PROFILES`).
  - `connection_profiles` (`quilltap-core::db::connection_profiles`): the
    workhorse profile repo and the **widest marshaling surface** to date — ~29
    columns spanning three enum TEXT columns, eight booleans, two nullable REAL
    int-overrides (`maxContext`/`maxTokens`), five REAL token counters, three
    nullable strings, the `tags` array, and the open `parameters` object. The
    corpus supplies every column explicitly. Harness
    `connection_profiles_tier2_equivalence` (`QT_ORACLE_CONNECTION_PROFILES` +
    `QT_FIXTURE_CONNECTION_PROFILES`).
  - New tracked deferred seam (open-JSON multi-key key order): an open-JSON object
    column with **two or more keys** would diverge — `serde_json::Value` sorts keys
    while v4's `JSON.stringify` preserves insertion order. The `image_profiles` /
    `connection_profiles` corpora constrain `parameters` to `{}` or single-key
    objects; see "Deferred seams" in `docs/developer/porting/phase-2-onramp.md`.

- A third parallel batch — five plain-base single-table repos (each `create` /
  `update` / `delete`, its own tier-2 case round-tripping green):
  - `plugin_config` (`quilltap-core::db::plugin_config`): the **UserOwned lineage**
    (a `userId` scope column) plus an **open-JSON object column** (`config`,
    `z.record`) and an **optional (nullable) boolean** (`enabled`,
    `z.boolean().optional()` with no default → INTEGER 0/1 when present, SQL NULL
    when the key is absent — confirmed empirically). Harness
    `plugin_config_tier2_equivalence` (`QT_ORACLE_PLUGIN_CONFIG` +
    `QT_FIXTURE_PLUGIN_CONFIG`).
  - `embedding_profiles` (`quilltap-core::db::embedding_profiles`): the Taggable
    lineage again, widened with an **enum TEXT** column (`provider`), two **nullable
    REAL number** columns (`dimensions` bare `z.number()`, `truncateToDimensions`
    `z.number().int().positive()` — min-only, so REAL not INTEGER), and two
    **boolean-default** columns (`normalizeL2`, `isDefault`). Harness
    `embedding_profiles_tier2_equivalence` (`QT_ORACLE_EMBEDDING_PROFILES` +
    `QT_FIXTURE_EMBEDDING_PROFILES`).
  - `terminal_sessions` (`quilltap-core::db::terminal_sessions`): a clean
    string-heavy repo — nullable string columns (`label`, `transcriptPath`), a
    nullable timestamp (`exitedAt`), and a **nullable REAL** column (`exitCode`,
    `z.number().int()`, no max). v4's `create` injects no nondeterministic defaults,
    so the pinned zero-normalization form holds. Harness
    `terminal_sessions_tier2_equivalence` (`QT_ORACLE_TERMINAL_SESSIONS` +
    `QT_FIXTURE_TERMINAL_SESSIONS`).
  - `character_plugin_data` (`quilltap-core::db::character_plugin_data`): the first
    **open-JSON _value_ column** (`data`, `z.unknown()`) — any JSON value stored as
    compact JSON text via v4's `prepareForStorage`, modeled as `serde_json::Value`.
    Harness `character_plugin_data_tier2_equivalence`
    (`QT_ORACLE_CHARACTER_PLUGIN_DATA` + `QT_FIXTURE_CHARACTER_PLUGIN_DATA`).
  - `tfidf_vocabulary` (`quilltap-core::db::tfidf_vocabulary`): the first repo that
    **overrides the base `create`/`update`** — v4 mints `updatedAt =
    getCurrentTimestamp()` unconditionally (a passed `updatedAt` is ignored), so the
    port mints it via `clock::now_iso` and the harness placeholder-normalizes only
    that one column (ids / `createdAt` / every payload column stay pinned and diff
    exactly). Also the first **plain-string columns that hold JSON text**
    (`vocabulary`, `idf`, bound single-encoded, not re-stringified), plus a bare
    `z.number()` REAL (`avgDocLength`) and an int-positive REAL (`vocabularySize`).
    Harness `tfidf_vocabulary_tier2_equivalence` (`QT_ORACLE_TFIDF_VOCABULARY` +
    `QT_FIXTURE_TFIDF_VOCABULARY`).
  - The `plugin_config` / `character_plugin_data` open-JSON corpora are constrained
    to `{}` or single-key objects, same as the tracked multi-key key-order seam.

- A fourth parallel batch — five more main-DB repos (each `create` / `update` /
  `delete`, its own tier-2 case round-tripping green):
  - `users` (`quilltap-core::db::users`): the plainest surface yet — all strings
    plus five **nullable TEXT** columns (`email`, `name`, `image`, `emailVerified`,
    `passwordHash`), no booleans/numbers/JSON/BLOB. Harness
    `users_tier2_equivalence` (`QT_ORACLE_USERS` + `QT_FIXTURE_USERS`).
  - `conversation_chunks` (`quilltap-core::db::conversation_chunks`): the **second
    tier-2 BLOB column** (`embedding`, Float32 LE bytes via
    `embedding_blob::float32_to_blob`, null/empty → NULL, dumped as hex; a text-only
    update leaves it untouched) plus a REAL int (`interchangeIndex`,
    `z.number().int().min(0)` — min-only → REAL) and two **JSON string-array
    columns** (`participantNames`, `messageIds`). Harness
    `conversation_chunks_tier2_equivalence` (`QT_ORACLE_CONVERSATION_CHUNKS` +
    `QT_FIXTURE_CONVERSATION_CHUNKS`).
  - `files` (`quilltap-core::db::files`): the **widest repo to date** (~23 columns,
    Taggable) — a bare-`z.number()` REAL (`size`), two **nullable REAL** columns
    (`width`/`height`), an **optional boolean** (`isPlainText` — banks both the
    present 0/1 and the absent → NULL case), two JSON arrays (`linkedTo`, `tags`),
    three enum TEXT columns (`source`, `category`, `fileStatus`), and several
    nullable strings. Harness `files_tier2_equivalence` (`QT_ORACLE_FILES` +
    `QT_FIXTURE_FILES`).
  - `chat_documents` (`quilltap-core::db::chat_documents`): an enum TEXT column
    (`scope`), a boolean (`isActive`), and two nullable strings. Harness
    `chat_documents_tier2_equivalence` (`QT_ORACLE_CHAT_DOCUMENTS` +
    `QT_FIXTURE_CHAT_DOCUMENTS`).
  - `embedding_status` (`quilltap-core::db::embedding_status`): the second repo that
    **overrides the base `create`/`update`** with an unconditionally-minted
    `updatedAt` (like `tfidf_vocabulary`) — the port mints it via `clock::now_iso`
    and the harness placeholder-normalizes only `updatedAt` (id / `createdAt` /
    payload pinned). Two enum TEXT columns (`entityType`, `status`) + a nullable
    timestamp + a nullable string. Harness `embedding_status_tier2_equivalence`
    (`QT_ORACLE_EMBEDDING_STATUS` + `QT_FIXTURE_EMBEDDING_STATUS`).

Phase 2 — the mount-index sibling-DB slice (the first repos NOT in the main DB).
These tables live in v4's dedicated `quilltap-mount-index.db`. The tier-2
machinery was extended to target a sibling DB: the fixture builder + oracle point
`SQLITE_MOUNT_INDEX_PATH` at the fixture (with a throwaway main DB at
`SQLITE_PATH`), seed/run through v4's real repos (whose `getCollection` override
routes there), flush via `closeMountIndexSQLiteClient`, and read back through
`getRawMountIndexDatabase` directly (not `rawQuery`, which targets the main
backend). The Rust `Writer` needed no change — `open_writable` already opens any
ChaCha20 file by path, so the partition is simply which file the writer opened.
Five repos ported in one slice (a serial pilot, then four parallel), each with its
own tier-2 case round-tripping green (pinned ids + timestamps → zero
normalization):

  - `group_character_members` (`quilltap-core::db::group_character_members`): the
    pilot — the plainest join table (`id` + two UUID-as-TEXT refs + timestamps).
    Harness `group_character_members_tier2_equivalence`
    (`QT_ORACLE_GROUP_CHARACTER_MEMBERS` + `QT_FIXTURE_GROUP_CHARACTER_MEMBERS`).
  - `project_doc_mount_links` / `group_doc_mount_links`
    (`quilltap-core::db::{project_doc_mount_links,group_doc_mount_links}`):
    structurally identical join tables (cross-DB refs stored as plain TEXT — v4's
    `generateCreateTable` emits no FK constraints). Harnesses
    `project_doc_mount_links_tier2_equivalence` /
    `group_doc_mount_links_tier2_equivalence`.
  - `doc_mount_folders` (`quilltap-core::db::doc_mount_folders`): adds a **nullable
    UUID** column (`parentId`, null = mount-point root) — banks both the null and
    non-null paths. Harness `doc_mount_folders_tier2_equivalence`.
  - `doc_mount_points` (`quilltap-core::db::doc_mount_points`): the widest of the
    family (18 columns) — four enum TEXT columns, a boolean (`enabled`, banks 0 and
    1), two **JSON string-array** columns (`includePatterns`/`excludePatterns`,
    banks empty and non-empty), three nullable strings/timestamp, and three
    **REAL-affinity int counters** (`fileCount`/`chunkCount`/`totalSizeBytes`,
    `z.number().int()` with no min&max → REAL, integer-collapsed in the dump). Its
    runtime ALTER-TABLE "migrations" are no-ops on a fresh schema-generated table.
    Harness `doc_mount_points_tier2_equivalence`.

Phase 2 — the llm-logs sibling DB + the deferred `upsert*` methods (two
independent slices).

`llm_logs` (`quilltap-core::db::llm_logs`): the SECOND sibling-DB partition (v4's
`quilltap-llm-logs.db`) and the widest repo in Phase 2 — 18 columns including FIVE
nested typed-struct JSON columns (`request`, `response`, `usage`, `cacheUsage`,
`requestHashes`), an open-JSON `rawProviderUsage`, a nullable REAL (`durationMs`),
an 18-variant enum, and four nullable UUIDs. Same TS-only sibling-DB machinery as
the mount-index slice but pointed at `SQLITE_LLM_LOGS_PATH` / read back through
`getRawLLMLogsDatabase()` (the backend disconnect closes this client, so the
oracle reads before `closeDatabase()`). The nested JSON is reproduced byte-for-byte
with serde structs in schema field order: integer-valued nested numbers as `i64`
(so they render `3`, not `3.0`, matching `JSON.stringify`), `temperature` the lone
`f64` (kept fractional), optional nested fields `skip_serializing_if` (omitted, not
null). Pinned zero-normalization form; `rawProviderUsage` constrained to
null/`{}`/single-key (the open-JSON seam). Harness `llm_logs_tier2_equivalence`.

The deferred `upsert*` methods on six already-ported repos are now implemented,
each with its own tier-2 case in the REMAP (minted-values) form: the upsert mints
`id`/`createdAt`/`updatedAt` on the create branch and `updatedAt` (preserving
`id`/`createdAt`) on the update branch, so the test pins nothing for the upsert
ops — it remaps `id` to first-seen tokens in natural-key order and placeholders
both timestamps (the folders-remap `createdAt == updatedAt` invariant is dropped,
since an upsert-update legitimately differs). Each `upsert*` adds a private
find-by-key SELECT and mints via `clock::now_iso` + `uuid`.

  - `conversation_annotations.upsert` — find by (chatId, messageIndex,
    characterName); update sets only {content, sourceMessageId}. Added a nullable
    setter (`Option<Option<_>>`) for `sourceMessageId`. Harness
    `conversation_annotations_upsert_tier2_equivalence`.
  - `help_docs.upsertByPath` — find by `path`; update sets {title, url, content,
    contentHash}, leaving the `embedding` BLOB untouched; create stores a NULL
    embedding. The test proves an upsert-update preserves a non-null embedding.
    Harness `help_docs_upsert_tier2_equivalence`.
  - `provider_models.upsertModel` (+ a thin `upsertModelForProvider` loop) — find
    replicates v4's `findByProviderAndModelId`: `baseUrl` joins the predicate only
    when truthy (a falsy baseUrl leaves the column unconstrained — NOT "match
    NULL"). Update writes the full data. Harness
    `provider_models_upsert_tier2_equivalence`.
  - `plugin_config.upsertForUserPlugin` — find by (userId, pluginName); update
    MERGEs `{...existing, ...new}` config (corpus keeps the merge {}/single-key).
    Harness `plugin_config_upsert_tier2_equivalence`.
  - `character_plugin_data.upsert` — find by (characterId, pluginName); update sets
    {data} (open-JSON, {}/single-key). Harness
    `character_plugin_data_upsert_tier2_equivalence`.
  - `tfidf_vocabulary.upsertByProfileId` — find by `profileId`; update writes full
    data. Builds on the base-method-override minting (create/update mint
    `updatedAt` themselves). Harness `tfidf_vocabulary_upsert_tier2_equivalence`.

Phase 2 — a fifth parallel batch of five repos (`create` / `update` / `delete`
each, pinned ids + timestamps → zero normalization), spanning the main DB and the
mount-index sibling DB:

  - `chat_settings` (`quilltap-core::db::chat_settings`): a plain main-DB
    `AbstractBaseRepository`, and the **widest JSON-object surface in Phase 2** —
    ~33 columns including ~15 nested typed-struct JSON columns reproduced in schema
    field order (serde structs, not key-sorting `serde_json::Value`), nested integer
    fields typed `i64` so they render bare. Banks the **first INTEGER-affinity number
    column** (`sidebarWidth`, `.min(256).max(512)` — both bounds integer → INTEGER,
    unlike the prior min-only/bare REAL numbers). The `cheapLLMSettings` column keeps
    its uppercase acronym (camelCase would mangle it). The `*ForUser`
    default-injecting helpers and the multi-key open-JSON `tagStyles` key order are
    out of scope (the corpus keeps `tagStyles` `{}`). Harness
    `chat_settings_tier2_equivalence`.
  - `wardrobe` (`quilltap-core::db::wardrobe`, table `wardrobe_items`): the first
    repo whose **public CRUD is vault-only** — v4's `WardrobeRepository` writes to
    the document store and throws without a mount, with no SQL write mirror — so the
    differential drives v4's **real base-repository SQL CRUD** (`_create`/`_update`/
    `_delete`) against the table via a thin subclass exposing the protected
    internals (the marshaling the schema-translator builds from `WardrobeItemSchema`
    and the table's reads consume). Banks the first repo with **two JSON array
    columns** (`types` — the first enum-string array — and `componentItemIds`) and a
    **nullable soft-delete timestamp** (`archivedAt`, exercised null and
    set-to-non-null), alongside two booleans and several nullable string/UUID
    columns. The vault-overlay write path itself is NOT ported/verified (tracked
    deferral); the unarchive (`archivedAt` → NULL) nullable-setter is implemented but
    not in the corpus. Harness `wardrobe_tier2_equivalence`.
  - `doc_mount_files` (`quilltap-core::db::doc_mount_files`): a mount-index sibling-DB
    repo and the **narrowest tier-2 repo to date** (all-required columns, no JSON/
    boolean/nullable). Re-banks a REAL-affinity min-only int (`fileSizeBytes`,
    `.int().min(0)` → REAL, integer-collapsed) and two enum TEXT columns; v4's
    `getCollection` adds a non-UNIQUE sha256 lookup index that touches no row bytes.
    Harness `doc_mount_files_tier2_equivalence`.
  - `doc_mount_documents` (`quilltap-core::db::doc_mount_documents`): a mount-index
    sibling-DB repo — the database-backed file-content store keyed by a UNIQUE
    `fileId`. Banks a `plainTextLength` min-only REAL int, a UUID-as-TEXT UNIQUE
    natural key, and plain TEXT content/sha columns (the content-addressable +
    joined-view read helpers are out of scope). Harness
    `doc_mount_documents_tier2_equivalence`.
  - `doc_mount_chunks` (`quilltap-core::db::doc_mount_chunks`): a mount-index
    sibling-DB repo and the **first sibling-DB repo to carry a BLOB column** — the
    `embedding` Float32 little-endian BLOB (empty/null → NULL, dumped as hex for
    bit-exact compare, and a text-only update proven to leave it untouched, like
    `conversation_chunks`/`help_docs`) plus two REAL-affinity min-only int counters
    (`chunkIndex`/`tokenCount`) and a nullable `headingContext`. The `updateEmbedding`
    BLOB-mutating path is out of scope. Harness `doc_mount_chunks_tier2_equivalence`.

Phase 2 — the document-store STORAGE PRIMITIVE
(`quilltap-core::db::doc_mount_file_links`), build step 1 of the document-store
overlay slice. Ports v4's `writeDatabaseDocument` + `linkDocumentContent` +
`ensureLinkFolderId` — the byte-landing path every store-backed entity
(project/group store, character vault) ultimately calls. A
`(mountPointId, relativePath, content)` write is content-addressed by SHA-256 and
split across three tables in one transaction (find-or-create `doc_mount_files` by
sha → upsert `doc_mount_documents` by `fileId` → upsert `doc_mount_file_links` by
`(mountPointId, relativePath)`), with `doc_mount_folders` rows auto-created for any
parent path. Also ports the pure leaves it needs: `sha256OfString`,
`detectDatabaseFileType`, `normaliseRelativePath`, and the per-document policy
(`coercePolicyBool` / `policyFromFrontmatterData` / `policyFromContent`, scalar
frontmatter subset). The tier-2 differential (`doc_mount_file_links_tier2_equivalence`)
drives v4's REAL `linkDocumentContent` against a mount-index fixture and diffs all
FOUR resulting tables in the minted-values remap form, extended with a SHARED
cross-table id-map (so `document.fileId` / `link.fileId` / `link.folderId` /
`folder.parentId` FKs verify by relationship); `mountPointId` is the pinned seeded
store id. The corpus covers a fresh JSON + markdown write, subfolder creation,
dedup-by-sha (a second path with identical content reuses one file + one document
row), link upsert-in-place (rewriting a path), and the markdown frontmatter policy
cascade (`character_read: false` → all `allow*` = 0). The oracle drives
`linkDocumentContent` directly rather than `writeDatabaseDocument` to avoid the
post-write `reindexSingleFile` chunk/embed pass (which would mutate the link rows;
its only skip-switch, `QUILLTAP_JOB_CHILD=1`, reroutes repos through the
forked-child write proxy). Deferred: arbitrary-YAML frontmatter (scalar subset
only — lands with the character-vault YAML decision), the UTF-16 `plainTextLength`
vs UTF-8 `fileSizeBytes` split is reproduced but only exercised on ASCII content,
and `linkBlobContent` / the read/GC/conversion helpers.

Phase 2 — the document-store OVERLAY ENGINE + the `groups` store-backed pilot
(`quilltap-core::db::{document_store_overlay, ensure_official_store, groups}`),
build steps 2-3 of the overlay slice. Ports v4's generic
`createDocumentStoreOverlay` + `AbstractStoreBackedRepository` as a Rust generic
over a `StoreEntity` trait, plus `ensureOfficialStore` provisioning, bound to
`groups`. A group's substantive content lives not in `groups` columns but in its
official document store as four overlay files (`properties.json` — the typed
`color`/`icon` bag in schema order, 2-space pretty-print; `description.md` /
`instructions.md` — raw markdown, empty → `null` on read; `state.json`). The slim
row (id/name/officialMountPointId/timestamps) lives in the MAIN db, the store in
the MOUNT-INDEX db, so `GroupsRepository` spans both connections (new
`Writer::connection()` seam). Reads overlay the store (the `doc_mount_documents`
3-table path→content join, new `find_[many_by]_mount_point[s]_and_path`); writes
route store-resident fields to the store and strip them from the slim patch
(properties via read-modify-write so a partial patch preserves untouched keys);
create runs the 5-step sequence (slim row → provision a `Group Files: <name>`
mount point + link + raw FK → write the four files → overlay re-read). Failure is
asymmetric (v4): `find_by_id` THROWS `OverlayError::Unavailable`, `find_all` DROPS
the bad row. Also ports the pure `nextUniqueMountPointName` (tier-1 unit test).
The tier-2 differential (`groups_tier2_equivalence`) drives v4's REAL
`repos.groups.create`/`.update` end-to-end (no mocked storage boundary, no
`QUILLTAP_JOB_CHILD`) and diffs SEVEN tables across BOTH dbs — the slim `groups`
row + `doc_mount_points` / `_files` / `_documents` / `_file_links` / `_folders` +
`group_doc_mount_links` — in the minted-values remap form with ONE shared
cross-db id-map (so `groups.officialMountPointId` → the store, `link.fileId` →
`file.id`, etc. verify by relationship). v4's post-write `reindexSingleFile` runs
(database-backed stores chunk with no model — deterministic); its only divergence,
the link `chunkCount` + the derived `doc_mount_chunks` rows, is pinned/excluded.
The corpus banks the 5-step create, `properties.json` byte-exact (both keys + the
empty bag), a store-only update (slim `updatedAt` NOT bumped) with a properties
RMW that preserves the untouched `icon`, a DB-only `name` update (store
untouched), dedup-by-sha (`"{}"` shared by three links across two stores; `""` by
two), and orphan-on-rewrite. A second test banks the keystone throw-vs-drop
asymmetry. Deferred: step-2 store adoption (the startup-heal heuristic — the
corpus always provisions fresh), `state`/property null-vs-absent + multi-key
insertion order (open-JSON seam — corpus kept `{}`/single-key), and the
`projects` generalization (a larger bag + roster ops).

Phase 2 — the character vault **managed-fields write projection**
(`quilltap-core::db::vault_character_write::write_character_vault_managed_fields`),
v4's `writeCharacterVaultManagedFields` — the first piece of the `characters`
repo (a `TaggableBaseRepository` with a bespoke vault overlay, not a generic
store-backed entity). Projects every vault-managed content field of a character
out to its file, in v4's exact order: `properties.json` (the typed
`pronouns`/`aliases`/`title`/`firstMessage`/`talkativeness` bag, 2-space
pretty-print), the five markdown files (`identity` / `description` / `manifesto`
/ `personality` / `example-dialogues`, `None` → `""`), and — only when a primary
`physicalDescription` is present — `physical-description.md` +
`physical-prompts.json` (`renderPhysicalPromptsJson`), then the `Prompts/` and
`Scenarios/` folder projections. Composes the already-ported pure leaves
(`build_system_prompt_file` / `build_scenario_file` / `sanitize_file_name`) and
the folder projector (`project_array_into_vault_folder`) over the document-store
write primitive. `properties.json` feeds the content-dedup SHA, so an
integer-valued `talkativeness` (e.g. `1.0`) is serialized as the bare integer `1`
(a `serialize_with` mirroring `js_number_to_json`) to match `JSON.stringify`
byte-for-byte; the five `properties.json` keys are a typed struct (serde
preserves struct field order, unlike `serde_json::Value`). Verified by a tier-2
differential (`vault_character_write_equivalence`) driving v4's REAL
`writeCharacterVaultManagedFields` over a two-op sequence (a full create with a
`Prompts/` filename collision `Default Voice.md`/`Default Voice-1.md` and two
scenarios, then a reproject that sweeps the dropped prompt + both old scenarios,
clears `physicalDescription` — physical-* files PERSIST, v4 skips and does not
delete — and renders `talkativeness: 1`) and diffing five mount-index tables in
the shared-cross-table-id-map remap form; plus four exact unit tests. v4's
post-write reindex runs (database-backed chunking, no model); its only divergence
(link `chunkCount` + `doc_mount_chunks`) is pinned/excluded, exactly as the
groups/projects/wardrobe store-backed tests do.

Phase 2 — the character vault **wardrobe write projection**
(`quilltap-core::db::vault_wardrobe_write`), v4's `projectVaultWardrobe` +
`projectArrayIntoVaultFolder` — the final wardrobe write piece, and with it the
whole document-store slice is complete. Re-projects an authoritative
`WardrobeItem` list into a vault store's `Wardrobe/` folder: each item is written
as `Wardrobe/<title>.md` (filename collisions disambiguated with `-1`/`-2`/…
suffixes), any `.md` file in the folder not produced by the current list is swept,
and the legacy `wardrobe.json` is deleted so the folder layout is the single
on-disk source. Composes the already-ported pure leaves
(`build_slug_by_item_id_map`, the Decision-A `build_wardrobe_item_file` emitter,
`sanitize_file_name`) over the document-store write primitive
(`write_database_document`) and a new GC delete (`delete_database_document` +
`delete_with_gc`: unlink, then drop the file row when its last link is gone —
chunks/documents cascade via the FK). Verified by a tier-2 differential
(`vault_wardrobe_write_equivalence`) driving v4's REAL `projectVaultWardrobe` over
a two-op sequence (an initial 5-item projection with a `Hat.md`/`Hat-1.md`
filename collision and a composite emitting `componentItems` slugs, then a rename
that sweeps the old file + recomputes the composite's slug and removes two items)
and diffing five mount-index tables (`doc_mount_points` / `_files` / `_documents`
/ `_file_links` / `_folders`) in the shared-cross-table-id-map remap form. v4's
post-write reindex runs (database-backed chunking, no model); its only divergence
(link `chunkCount` + `doc_mount_chunks`) is pinned/excluded, exactly as the
groups/projects store-backed tests do.

Phase 2 — the character vault **wardrobe YAML emitter** (Decision A — the only
eemeli/yaml site), `quilltap-core::vault_overlay::build_wardrobe_item_file`, v4's
`buildWardrobeItemFile`. Projects a `WardrobeItem` to its `Wardrobe/*.md` content:
a YAML frontmatter block (keys in v4's exact insertion order; `componentItemIds`
translated to slugs with a UUID fallback) plus the description body. Per locked
Decision A the YAML is hand-rolled — the emitted bytes feed the content-dedup
SHA, so a quoting mismatch is a silent mis-dedup, not just a test gap. The emitter
is a faithful port of eemeli/yaml 2.9.0's `stringifyString` + `foldFlowLines`
(default options) for the bounded value space (string scalars, the boolean `true`,
block sequences of string scalars): plain/single/double quote selection, the
core-schema reparse-safety quoting (a scalar that would reparse as
number/bool/null is quoted), line folding past width 80, and block scalars
(`|`/`|-`/`>`) for multiline values. It operates on UTF-16 code units throughout
(as JS does) so fold offsets, the control-char force-quote check (matched on code
points, per eemeli's `/u` flag — a valid astral character is not a surrogate
match), and `JSON.stringify` escaping align byte-for-byte. Verified by a tier-1
differential (`vault_wardrobe_emit_equivalence`) against v4's real
`buildWardrobeItemFile` over a 100-item corpus spanning every quoting edge,
folding, block scalars, surrogate-pair fold offsets, the slug/UUID map, and all
flag branches; plus three exact unit tests. This was the last open vault decision;
the only wardrobe write piece still ahead is the stateful folder projection
(`projectVaultWardrobe` — filename dedup/rename/sweep + multi-table writes).

Phase 2 — the character vault **wardrobe read overlay**
(`quilltap-core::db::vault_read_overlay::read_character_vault_wardrobe` +
`quilltap-core::vault_overlay::resolve_and_check_component_items`), v4's
`readCharacterVaultWardrobe`. Enumerates `Wardrobe/*.md` (the Decision-B code-unit
sort, then `parseWardrobeItemFile`, dropping unparseable files), builds the
in-vault slug/id lookup maps (first-claimer wins a slug; every item is addressable
by id), and resolves each item's raw `componentItems:` refs to canonical ids —
slug-first then UUID, unknown refs dropped — before a cycle check that clears any
item whose resolved components form a cycle. The cycle pass reads the **live**
(already-mutated) component lists, so clearing one item mid-pass changes later
items' walks, exactly mirroring v4's mutable `itemById` (proven in the corpus: a
mutual `a → b`/`b → a` cycle clears `a`, then `b` survives because `a` was already
emptied when `b`'s walk ran). An empty/missing `Wardrobe/` folder falls through to
the legacy `wardrobe.json` (`parseLegacyWardrobeJson`); neither present → `null`.
Verified by a read-differential (`vault_wardrobe_read_equivalence`, three cases)
driving v4's REAL `readCharacterVaultWardrobe` over a shared seeded fixture —
slug/UUID/collided-slug/unknown resolution, the live-mutation cycle asymmetry, a
self-cycle clear, an archived item, the legacy fallback, and the empty-vault
`null` — comparing each `{ items } | null` exactly (no normalization; this read
path mints no clock value). Plus four tier-1 unit tests on the resolver.
**Tracked deferral:** the archetype-seeding branch (`findArchetypes` over the
General/project `Wardrobe` stores) is not ported — the corpus keeps no General
store provisioned, so v4's `findArchetypes` returns `[]` and the seed is a
verified no-op.

Phase 2 — the character vault **read overlay** (`quilltap-core::db::vault_read_overlay`),
the heart of the Family-B read path: v4's `hydrateOne` + `applyDocumentStoreOverlay`
+ `applyDocumentStoreOverlayOne`. Folds a character's vault files onto the
character so every read sees vault values transparently. Because the overlay is a
plain JSON merge, the port operates on the character as a `serde_json::Value`
object (not a fully-typed `Character`), patching the managed keys with values from
the already-ported pure parsers: `properties.json` →
pronouns/aliases/title/firstMessage/talkativeness; the five markdown fields
(identity/description/manifesto/personality/exampleDialogues) via
`markdownToNullable` (empty → null); `physical-description.md` +
`physical-prompts.json` → `physicalDescription` (base-reuse when the character
already has one, else a minted base with `stableUuidFromString('physical:<mp>')` +
clock-minted timestamps); `Prompts/*.md` → `systemPrompts` (the Decision-B
code-unit sort + parse + the exactly-one-`isDefault` normalization: keep the first
declared default and demote the rest, or promote the first when none is marked);
`Scenarios/*.md` → `scenarios`. The keystone is `properties.json`: a linked vault
that lacks it is broken — the batched apply DROPS the character (one corrupt vault
can't take down the roster) while the single apply returns an Unavailable error
(v4 throws → 503). Verified by a read-differential
(`vault_read_overlay_equivalence`) driving v4's REAL `applyDocumentStoreOverlay`
over seven input characters against a six-store seeded fixture — pass-through, full
overlay, drop, partial (arrays replaced with `[]`), physical mint, and all three
prompt-default cases — comparing the hydrated characters exactly (only the minted
physical timestamps placeholdered), plus the `…One` throw on the broken vault.

Phase 2 — the vault read overlay's directory-listing load
(`DocMountDocumentsRepository::find_many_by_mount_points_in_folder`), the first
stateful sub-unit of the character read overlay (Family B). Ports v4's
`findManyByMountPointsInFolder`: the 3-table join with a SQL
`LOWER(relativePath) LIKE '<folder>/%'` prefilter, then v4's JS post-filter
(case-folded prefix, non-empty remainder, single-level only — no `/` in the
remainder — and an extension match). The overlay-consumed subset of the row is
returned (`content`/`mountPointId`/`relativePath`/`fileName` + the document
`createdAt`/`updatedAt`); v4's unused `recursive` option is not ported. Verified
by the first **read-differential**: a fixture builder seeds two pinned stores and
writes a corpus via v4's real `linkDocumentContent` (driven directly — not
`writeDatabaseDocument`, whose `QUILLTAP_JOB_CHILD=1` skip-switch reroutes repos
through the forked-child write proxy and breaks `initializeDatabase`); both v4 and
the Rust port then READ the SAME fixture, so minted ids/timestamps are identical
and the returned rows compare exactly (sorted by `(mountPointId, relativePath)`,
the read having no defined order). The corpus covers the IN-clause across two
stores and excludes a top-level file, a nested file, and a wrong-extension file,
plus the empty-mount-point short-circuit (`vault_folder_read_equivalence`).

Phase 2 — the vault `Wardrobe/*.md` parser
(`quilltap-core::vault_overlay::parse_wardrobe_item_file`), the third and last
per-file frontmatter parser. Reuses the title fallback chain (frontmatter `title`
→ first `# heading` → filename-without-`.md`) and the already-ported
`parse_wardrobe_types_field` (a valid `types` list is required, else skip) /
`parse_component_items_field` (raw author refs kept for the overlay's later
resolution pass). Reproduces the id sanity check (`/^[0-9a-f-]{36}$/i` — 36 chars,
hex-or-`-`; otherwise `stableUuidFromString`, incl. a 36-char non-hex id that must
fall back), the non-empty-string fields (`appropriateness`/`imagePrompt`), the
boolean flags (`default || isDefault`, `replace`), the `archivedAt` precedence
(non-empty string wins, else `archived: true` → `doc.updatedAt`), the
`typeof === 'string'` keep of `migratedFromClothingRecordId` (incl. empty), and
the frontmatter-vs-doc timestamp precedence. Output is built directly (not via
Zod), so its nullable fields are ALWAYS present (`null` or value) and a heading
used as the title is dropped from the body (an empty body → `null` description,
NOT a skip). Tier-1 exact differential (`vault_wardrobe_item_file_equivalence`)
over 20 cases against v4's real `parseWardrobeItemFile`.

Phase 2 — the vault frontmatter READ parsers
(`quilltap-core::vault_overlay::parse_prompt_file` / `parse_scenario_file`),
built on the hand-rolled frontmatter reader. Each turns a vault markdown file
into a `CharacterSystemPrompt` / `CharacterScenario`, or `None` (skip — the
overlay falls back to the DB value for that one file). Faithful to v4: the
objects are built directly (not via Zod), so the JS `.trim()` / `.slice(0, n)`
caps are reproduced with the `jsstr` UTF-16 primitives (name ≤100, title ≤200,
description ≤500); `isDefault` is `=== true` (a `"true"` string → false); the
prompt body is the content after the frontmatter, `trimStart`ed; scenario title
resolution is frontmatter `name` → first `# heading` (`/^#\s+(.+)$/` with the JS
whitespace set) → filename-without-`.md`, and a heading used as the title is
dropped from the body while a frontmatter-supplied title leaves the body intact.
Added `jsstr::js_trim_start` and `markdown::body_after` (UTF-16-offset → byte
slice). Tier-1 exact differential (`vault_frontmatter_parsers_equivalence`) over
26 cases against v4's real `parsePromptFile`/`parseScenarioFile`, incl. multibyte
content to cover the UTF-16 body offset and every skip condition.

Phase 2 — the Markdown frontmatter parser + a hand-rolled YAML reader
(`quilltap-core::markdown::parse_frontmatter`), the shared read-path foundation
for the vault's per-file parsers. v4's `parseFrontmatter`
(`lib/doc-edit/markdown-parser.ts`) calls eemeli/yaml's `YAML.parse`; the read
side is the companion to locked Decision A, so this hand-rolls a parser for the
constrained subset our own emitters produce plus simple hand-edits — no YAML
crate in the vault — matching eemeli/yaml's **YAML 1.2 core-schema** output on
that subset. Reproduces the structural logic exactly (the `---\n`-only opener so
CRLF frontmatter isn't recognized; the exactly-`---` closing line; UTF-16
`bodyStartOffset` computed even when the YAML fails to yield an object;
empty/whitespace/comments-only → `{}`; array/scalar root → null; duplicate keys
→ null, since eemeli throws) and the scalar resolution (`~`/`null`/empty → null;
`true`/`false` case-variants → bool while `yes`/`no` stay strings; decimal
int/float → number; ISO timestamps and URLs stay strings; double-quoted
JSON-style escapes incl. `\uXXXX`; single-quoted `''`; the whitespace-gated `#`
comment rule; flow `[a, b]` and block `- item` sequences). Tier-1 exact
differential (`markdown_frontmatter_equivalence`) over 52 cases against v4's real
`parseFrontmatter`. Nested maps, flow maps, block scalars, anchors/tags, and
exotic numbers (hex/octal/exponent/`.inf`/`.nan`) are the documented
out-of-subset seam — kept out of the corpus; they resolve conservatively (a
null/string or a parse error), never to a silently-wrong typed value.

Phase 2 — the legacy `wardrobe.json` migration parser
(`quilltap-core::vault_overlay::parse_legacy_wardrobe_json`), the next
decision-free vault-overlay leaf (Family B). Unlike the two JSON projection
parsers, this validates an array of full `WardrobeItemSchema` items, so it
reproduces Zod 4's `z.uuid()` and `z.iso.datetime()` string formats verbatim
(the regex sources lifted from the live schema: version-nibble `[1-8]` /
variant `[89abAB]` UUIDs plus the all-zero/all-`f` sentinels; ISO dates with
leap-year arithmetic and a `Z`-only zone; JS `\d` rewritten to ASCII `[0-9]`).
Faithful to Zod's rules — any single bad item nulls the whole array; `.default()`
keys (`componentItemIds`/`isDefault`/`replace`) are materialized; output is in
schema order regardless of input key order; unknown keys are stripped (root
`presets`, per-item extras, in-`outfit` extras); and a present `outfit` is
validated (a malformed one fails the parse) then discarded — only `{ items }` is
returned. Tier-1 exact differential (`vault_legacy_wardrobe_equivalence`) over 39
cases against v4's real `parseLegacyWardrobeJson`, covering the valid shapes
(full/minimal-with-defaults/all-nulls/multi/empty/presets-stripped/outfit-valid)
and every interesting violation (bad/missing id, empty/missing title, bad-enum/
empty/non-string types, bad-uuid/non-array/null componentItemIds, non-bool/null
booleans, bad timestamps incl. non-leap `2023-02-29`, offset-zone, no-zone, and
trailing-newline rejection — confirming the `regex` `$` matches JS's absolute-end
anchor).

Phase 2 — the vault JSON projection parsers (`quilltap-core::vault_overlay`), the
next decision-free slice of the character/wardrobe vault overlay (Family B, build
step 6). `parseVaultProperties` + `parseVaultPhysicalPrompts` reproduce v4's Zod
`safeParse`-then-fall-back-to-`null` semantics (`vault-overlay/parsers.ts`): parse
the file JSON, validate against the vault schema, return the typed value or `None`
on a JSON-parse error OR any schema violation. Faithful to Zod's rules — unknown
keys stripped (default `z.object`, top-level and inside `pronouns`); a
`.nullable()` field is required-present (key must exist, value may be `null`) and
serializes `null` when unset; a `.nullable().optional()` field may be absent;
`talkativeness` is range-checked `0.1 ≤ t ≤ 1.0`; the nested `pronouns` fields are
required strings of 1–20 UTF-16 code units. Tier-1 exact differential
(`vault_json_parsers_equivalence`) over 24 cases against v4's real functions
(valid/all-nulls/extra-stripped/invalid-JSON/non-object/missing-key/range-bounds/
non-array-aliases/non-string-element/pronoun-missing-field/too-long/empty/
wrong-type), with integer-valued floats canonicalized on both sides so
`talkativeness: 1.0` (which v4 emits as `1`) compares equal. (`headAndShoulders`
present-`null` is the one tracked null-vs-absent divergence, kept out of the
corpus.)

Phase 2 — the vault write-projection string leaves (`quilltap-core::vault_overlay`),
the next decision-free slice of the character/wardrobe vault overlay (Family B,
build step 6). Five pure functions from v4's `character-vault.ts`:
`slugifyWardrobeTitle` (kebab slug — `toLowerCase` → JS-trim → collapse
non-`[a-z0-9]` runs to `-` → strip ends; the `[^a-z0-9]→-` filter makes it
collation/case-safe, so no ICU per the locked Decision B), `buildSlugByItemIdMap`
(first-wins `(itemId → slug)` list), `sanitizeFileName` (replace `\ / : * ? " < >
|` with `_`, collapse JS-whitespace runs, JS-trim, 100-UTF-16-unit slice,
`untitled` fallback — reusing the existing `jsstr` whitespace/trim/UTF-16
helpers), `buildSystemPromptFile` (the `Prompts/*.md` frontmatter, exercising the
private `escapeYaml` = `if /[:#"'\n]/ then JSON.stringify(v) else v`, reproduced
with `serde_json::to_string` which matches `JSON.stringify` for strings), and
`buildScenarioFile` (plain `# title\n\nbody`, no frontmatter). Tier-1 exact
differential (`vault_string_leaves_equivalence`) over 27 cases against v4's real
functions, incl. unicode→dash slugs, punctuation, the `escapeYaml` quote triggers
(`:`/`#`/`"`/`'`/`\n`), and the empty→`untitled` filename path. Per the locked
decisions, this confirms the prompt/scenario write projections need NO eemeli/yaml
(only `Wardrobe/*.md`, build step 7, does) and the slug path needs no ICU.

Phase 2 — the vault wardrobe-component pure leaves (`quilltap-core::vault_overlay`),
the first slice of the character/wardrobe vault overlay (Family B, build step 6),
ported leaf-first ahead of the stateful overlay so the YAML-emitter and
ICU-collation decisions the *write* path forces are not yet on the critical path.
Three decision-free pure functions: `parseComponentItemsField` (coerce a raw
`componentItems:` value → clean `Vec<String>`: non-arrays → `[]`, trim, drop
empty/non-string), `parseWardrobeTypesField` (validate a `types:` value against
`WardrobeItemTypeEnum` — all-or-nothing, de-dup first-seen, `None` on
empty/invalid), and `detectComponentCycles` (the save-time component-graph cycle
check: direct self-ref, indirect, sub-cycle, diamond-safe, deep-chain). Tier-1
exact differential (`vault_component_leaves_equivalence`) over 22 cases against
v4's real `parsers.ts` / `expand-composites.ts`. No YAML, no
case-mapping/collation — the JSON/array/graph leaves the vault needs, verified
before the projection that consumes them.

Phase 2 — `doc_mount_blobs` (`quilltap-core::db::doc_mount_blobs`), build step 8
of the document-store overlay slice: the document store's **binary** byte-store,
the sibling of the (ported) text store `doc_mount_documents`. Bytes (avatars,
PDF/DOCX content, any non-text) live in a `data BLOB NOT NULL` column keyed UNIQUE
by `fileId`. Unlike the Zod-schema repos, v4 hand-writes this repo and its DDL —
the `data` column is deliberately ABSENT from `DocMountBlobMetadataSchema`
(metadata reads never hydrate the bytes) — so the port reproduces the hand-written
`CREATE TABLE` verbatim (incl. the `FOREIGN KEY (fileId) REFERENCES
doc_mount_files(id)`). Ports `upsertByFileId` (insert-or-replace by `fileId`,
**recomputing `sha256` from the actual bytes** — the caller's sha is advisory —
with `sizeBytes = data.len()`; an existing row overwritten in place) plus the
metadata/`readData`/`delete` accessors. The tier-2 differential
(`doc_mount_blobs_tier2_equivalence`) drives v4's REAL `upsertByFileId` against a
mount-index fixture that seeds the parent `doc_mount_files` rows the FK requires
(enforced under the writable open's `foreign_keys = ON`), and diffs the table with
the `data` BLOB dumped as lowercase hex (bit-exact, mirrors `help_docs` /
`doc_mount_chunks`) in the minted-values remap form (`id` remapped, timestamps
placeholdered; `fileId` pinned, content compared directly). Banks a fresh insert,
an overwrite-in-place on a repeat `fileId`, the sha-recompute rule (every op
passes an all-zero advisory sha), and a non-UTF-8 binary payload (a PNG header +
`deadbeef`) round-tripping through the BLOB. `linkBlobContent` (the
`(mountPointId, relativePath)` content/link split, the binary analogue of
`linkDocumentContent`) remains deferred.

Phase 2 — `stableUuidFromString` (`quilltap-core::vault_overlay`), build step 5
of the document-store overlay slice: the first **character/wardrobe vault** leaf,
ported leaf-first ahead of the stateful vault overlay (Family B). It derives the
deterministic id every folder-enumerated vault entity (system prompts, scenarios,
wardrobe items) carries — `stableUuidFromString('<kind>:<mountPointId>:<relativePath>')`
— which chat references depend on. SHA-256 over the source's UTF-8 bytes → first
16 bytes → version nibble 8 (custom) + RFC-4122 variant → hyphenated lowercase
hex. Tier-1 exact differential (`stable_uuid_equivalence`) against v4's real
function over the `prompt:`/`scenario:`/`wardrobe-item:` prefixed forms, an empty
string, and a non-ASCII path (SHA-256 runs over UTF-8 both sides — the accented
source agrees byte-for-byte; there is no case mapping here, unlike the
`toLowerCase`/`localeCompare` seams).

Phase 2 — the `projects` store-backed entity + the store-backed GENERALIZATION
(`quilltap-core::db::{store_backed, projects}`), build step 4 of the overlay
slice. Generalizes the slim-row plumbing + provisioning that `groups` proved into
a reusable `StoreBackedRepository<E: StoreEntity>` (v4's
`AbstractStoreBackedRepository`): the `StoreEntity` trait gains `slim_table` /
`store_name_prefix` / `find_store_links` / `link_store`, and `ensure_official_store`
becomes generic over `E` (the group/project ensure wrappers collapse into one).
`GroupsRepository` is refactored to a thin wrapper over the generic base (still
green); `projects` is the second instance. `ProjectsRepository` adds the **16-key
`properties.json` bag** (`ProjectPropertiesSchema` — five Zod-`.default` keys
ALWAYS materialized in schema order: `allowAnyCharacter` / `characterRoster` /
`defaultDisabledTools` / `defaultDisabledToolGroups` / `backgroundDisplayMode`; the
other eleven `.nullable().optional()` → `skip_serializing_if`) and the
**character-roster operations** (`addToRoster` / `removeFromRoster` /
`setAllowAnyCharacter` / `canCharacterParticipate` / `findByCharacterId`), each a
`properties.json` read-modify-write through `update` (or an in-memory `findAll`
filter). The tier-2 differential (`projects_tier2_equivalence`) drives v4's REAL
`repos.projects.create`/`.update`/roster ops end-to-end and diffs the same seven
tables across both dbs (the slim `projects` row + the store tables +
`project_doc_mount_links`) in the shared-cross-db-id-map remap form, `chunkCount`
pinned + `doc_mount_chunks` excluded (database-backed reindex uses no model). The
corpus banks a rich create (roster + color + `defaultImageProfileId` +
`backgroundDisplayMode`, the optional keys interleaved with the materialized
defaults in schema order — byte-exact) and a minimal create (only the five
defaults), `addToRoster`/`removeFromRoster` (the `characterRoster` array RMW
preserving the other fifteen keys), `setAllowAnyCharacter` (a bool RMW), and a
DB-only `name` update. The `ensureOfficialStore` step-2 adopt branch stays
deferred (corpus always provisions fresh); the property null-vs-absent +
multi-key insertion-order seam is unchanged (corpus kept to present/absent +
`{}`/single-key `state`).

Docs — the document-store-overlay design slice
(`docs/developer/porting/document-store-overlay.md`): the port plan for the
store-backed entities (`projects`, `groups`, `characters`, the `wardrobe` vault).
Establishes that the "document store" is DB rows in the mount-index DB (text in
`doc_mount_documents`, binary in `doc_mount_blobs`), not filesystem files, so no
filesystem fixture is needed; maps the generic overlay engine
(`createDocumentStoreOverlay` + `AbstractStoreBackedRepository`) shared by projects
and groups vs the heavier character/wardrobe markdown-vault family; sets a
dependency-first build order (port `doc_mount_file_links` + `linkDocumentContent` +
`writeDatabaseDocument` first, then the engine, then `groups` as pilot, then
`projects`); and specifies the tier-2 oracle strategy (drive v4's real storage code
against the existing mount-index fixtures with `QUILLTAP_JOB_CHILD=1`, dump the four
storage tables + the slim row, minted-values remap form). Linked from `overview.md`
and `CLAUDE.md`.

