# Quilltap Changelog

## Recent Changes

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

