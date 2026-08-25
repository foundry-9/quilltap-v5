# Dogfood walk — the `f6a10055` wardrobe-containers round + the standing 💸 queue — 2026-08-25

**Instance:** a COPY of Friday at `~/qt-dogfood-friday` (never the live iCloud tree).
**Server:** `./target/release/quilltap-web --data-dir ~/qt-dogfood-friday --spa-dir apps/web/dist/quilltap/browser`, `RUST_BACKTRACE=1`, log in the scratchpad.
**Findings log:** `docs/developer/porting/dogfood-findings.md` — next finding number is **#103**.
**Unlock:** prior passes measured `hasUserPassphrase: false`; confirm at launch before Part A.

## What this pass is for

One round has unified since the 2026-08-24 walk, and it is the first round in a
long while whose deliverables are *almost entirely* things only a real instance
can exercise interestingly: wardrobe containers over a vault with 70+ garments,
downloads over blobs whose stored names disagree with their `originalFileName`
(because real images were transcoded to WebP on upload), and a create schema
that had been enforcing nothing.

- **P4.D112 (server)** — the slug-collision vault fix (an ambiguous slug is
  assigned to NOBODY and colliders are referenced by UUID); wardrobe transfers
  gain an explicit `source` container + component-carrying `move|copy|none`;
  five NEW `groupWardrobe*` dispatch verbs (v5 had none).
- **P4.D113 (SPA)** — the container browser (four optgroups), the `canManage`
  kebab split with the `· shared` badge, **the editor pinned to the browsed
  container** (v5 measurably had v4's latent mis-target bug: any shared edit
  PUT Quilltap General), `imagePrompt` preserved on Duplicate, the transfer
  dialog's known-home hiding + component radios, and the avatar-preview
  download rider.
- **P4.D114** — `Content-Disposition` on the mount blobs endpoint (stored
  basename wins over `originalFileName`), the projects CREATE schema (v5
  enforced *nothing* but a non-blank name, and refused a whitespace-only name
  v4 accepts), four download surfaces + clipboard copy, and the create-project
  toasts answering v4's fixed sentence.

Plus the standing 💸 queue carried from five prior passes.

**Primary verification channels:**
- `./target/release/quilltap db --data-dir ~/qt-dogfood-friday --json "…"`
  (main partition); `--llm-logs` for `llm_logs` (call-type column is `type`).
- `~/qt-dogfood-friday/logs/combined.log` — P4.49's file appender (where the
  P4.61 `[Title Update]` lines land).
- The server's own stderr log in the scratchpad.
- The Browser pane's network responses (`read_network_requests`) for
  `Content-Disposition` and dispatch bodies.
- The emitted vault files on disk under `~/qt-dogfood-friday/files/` for the
  slug-collision proof.

## What NOT to expect to work (do not file these)

- **The group-wardrobe REST URLs** (`/api/v1/groups/{id}/wardrobe[/itemId]`)
  have **no `quilltap-web` edge** — the project-tier dispatch-only precedent.
  The SPA calls the dispatch verbs; a curl at the REST URL 404s by design.
- **`componentsTransferred` / `unresolvedComponentIds` are not rendered
  anywhere.** v4's own client never reads them (grep-proven at the pin).
  Their absence from the UI is fidelity, not a gap.
- **The Prospero project-wardrobe manager is a deliberate second door** to the
  same room now that `project:<id>` is browsable in the dialog. v4 ships the
  duplication too and says so in its own help text. Not a bug.
- **No Tauri/Electron native save dialog** — downloads take the browser path.
  Six other hand-rolled anchor-click download sites stay unconverged (v4 did
  not converge its counterparts either).
- **Help-doc content** is unported everywhere (`p4.9i2`).
- **Web search**: finding #98 said the configured `SERPER` key was invisible;
  P4.59 fixed it, and step G1 is that fix's live proof. If it fails, that IS a
  finding.
- **`embeddingProfileFetchModels`** answers a named loud refusal (P4.9H2A).
- Subsystem backgrounds other than a project story background, `?msg=` anchors,
  `/photos?tag=` filters — named deferrals.

---

## Part A — the container browser (P4.D113 units 3–4, P4.D112 unit 3)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| A1 | CLAUDE | The four optgroups on real data | Open the wardrobe dialog (roster/Wardrobe tab or in-chat) and read the container `<select>` | **PASS — four optgroups in v4's order over the real catalogue.** `#wardrobe-container-select` carries **Characters (32) / General (1) / Projects (8) / Groups (2)**, values encoded exactly as the shared module spells them (`character:af38f265-…`, `general:`, `project:0bb40dad-…`, `group:d07a2ade-…`) | PASS |
| A2 | CLAUDE | Browsing Quilltap General | Switch the selector to **General** | **PASS — the shared-container view, cross-checked against the store.** Switching to **Quilltap General** renders v4's banner verbatim (*"Browsing a shared wardrobe — items here can be worn by every character who can reach this library…"*), drops the right-hand outfit column, drops every equip control (the rows keep only **More actions**), and lists **13** items — exactly the 13 `Wardrobe/*.md` links under mount `40e4a1cc-…` (Quilltap General), name for name. Abigail's own garments (Azure Blue Lace Bardot Bralette, Cognac Leather Travel Backpack) are **absent**: nothing merged in | PASS |
| A3 | CLAUDE | `canManage` + the `· shared` badge | In a **character** view, find an item with `characterId = null` (a shared garment surfacing in the merged view) and open its kebab | **PASS — the per-item predicate, in one mixed real list.** Abigail's merged view holds 44 rows, some `· shared`, some her own. The kebab on **Apple Watch** (`· shared`) is exactly **[Move, Copy]**; the kebab on **Azure Blue Lace Bardot Bralette** (hers) is the full six in v4's order — **[Edit, ★ Mark as default outfit item, Duplicate, Move, Copy, Delete]**. Same list, same render pass, different answers | PASS |
| A4 | CLAUDE | A project container browses | Pick a real project from the Projects optgroup | **PASS — a real project container, counted against the store.** **Wardrobe Design** (`project:23209429-…`, store `82c1552c-…`) browses to **23 Items + 5 Outfits = 28**, exactly the 28 `Wardrobe/*.md` links in that store. The five outfits are real composites (`· bundle`, top/bottom/footwear/accessories/hair) — the test material Part C needs. The shared-container banner renders and the outfit column stands aside | PASS |
| A5 | CLAUDE | A group container browses (**the five NEW verbs' first live run**) | Pick a real group from the Groups optgroup | **PASS — the five NEW verbs' first live run.** **Sebold Family** (`group:4aae770d-…`) browses to its one garment, *Ansible Forge Mark Four* — the same item that showed as `· shared` in Abigail's merged view and was **absent from General**, so the group tier is demonstrably where it came from. Proven by name as well as by screen: `POST /api/dispatch {"type":"groupWardrobeList","groupId":"4aae770d-…"}` answers `{"type":"group","data":{"mountPointId":"42e542e5-…","wardrobeItems":[…]}}`. Every dispatch 200; zero ERROR lines in the server log | PASS |
| A6 | CLAUDE | A mangled container value | Set the select to a junk value via the DOM and dispatch `change` | **PASS — a mangled value parks rather than guesses.** Injecting `banana:not-a-container` into the select and firing `change` leaves **0 rows** and the *"Select a wardrobe to browse"* placeholder — `decodeWardrobeContainer` refuses the scope and the dialog holds still | PASS |

## Part B — the editor pinned to the container (P4.D113 unit 4 — the bug fix)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| B1 | CLAUDE | **The mis-target bug fix, live** | Browse a **project** container, open an existing garment in the editor, change its title, Save | **PASS — the round's headline bug fix, proven on real data.** Browsing **Wardrobe Design**, edited *Bare Feet* → *Bare Feet dogfood B1* and hit Update. The tapped dispatch body is `{"type":"projectWardrobeUpdate","projectId":"23209429-…","itemId":"76e7250e-…",…}` — **not** `wardrobeUpdate`, which is what every `isEditing && isShared` case sent before this round. The DB agrees: project store `82c1552c-…` now holds `Wardrobe/Bare Feet dogfood B1.md` (updatedAt **17:01:33**, seconds old), while Quilltap General is still **13** items with a newest updatedAt of **2026-08-07** — untouched. Pre-fix this exact gesture would have written into General | PASS |
| B2 | CLAUDE | The same for a **group** container | Edit a group garment (created in C1 if none exists), Save | **PASS — the group arm of the same fix.** Editing *Ansible Forge Mark Four* while browsing **Sebold Family** sends `{"type":"groupWardrobeUpdate","groupId":"4aae770d-…","itemId":"912eabcc-…",…}`, and group store `42e542e5-…` now holds `Wardrobe/Ansible Forge Mark Four dogfood B2.md` (updatedAt 17:02:56). Quilltap General: still 13 items, newest 2026-08-07. The dialog banner reads the **group** arm — *"…can be worn by every character who can reach this **group**"* | PASS |
| B3 | CLAUDE | The destination note + no scope radios | Open the editor from a shared container | **PASS — and the order's two arms are v4's own split, checked at the pin.** On EDIT in a shared container the editor shows *"Changes to shared items affect all characters"* and **`[role=radiogroup]` count is 0** — no scope selector. The **"Add to" destination note is the CREATE arm**, not the edit arm: `git show f6a10055:components/wardrobe/wardrobe-item-editor.tsx` gates it on `{!isEditing && sharedContainer && (` at `:489`, with the shared notice at `:590`. v5's template carries the same two gates. The create arm is proven in B4 | PASS |
| B4 | CLAUDE | Create into a shared container | From the General container, create a new garment | **PASS — create into a shared container, with v4's destination note.** New Item while browsing **Quilltap General** opens *"New Wardrobe Item"* headed by **Add to / Quilltap General / "Every character, in every chat, can wear it."**, the isDefault helper reading the general CREATE arm *"Worn by default by every character"* (v4 `:100`), and **no scope radiogroup**. `{"type":"wardrobeCreate","item":{"title":"Dogfood B4 Scarf",…,"types":["accessories"]}}` → the list went 13 → **14** and General now holds `Wardrobe/Dogfood B4 Scarf.md` (17:03:50) | PASS |
| B5 | CLAUDE | `imagePrompt` preserved on Duplicate | Duplicate a garment that HAS an `imagePrompt` (find one by query first) | **PASS — both halves.** Duplicated *Burnished Bronze Covenant Longcoat* (a real item carrying a 300-char `imagePrompt`) while browsing **Wardrobe Design**. The copy — *Burnished Bronze Covenant Longcoat (copy)*, id `1557b983-…` — carries the **identical `imagePrompt`** (v5 used to drop it) and lands in the **browsed container**: `doc_mount_file_links` puts `Wardrobe/Burnished Bronze Covenant Longcoat (copy).md` in project store `82c1552c-…`, beside the original, not in General | PASS |
| B6 | CLAUDE | The group `isDefault` helper text | Open the editor for a group garment and read the default toggle's helper | **PASS — the group CREATE arm, both strings.** New Item while browsing **Sebold Family** heads the form with **Add to / "Sebold Family" / "Every character in this group can wear it."** (the note takes the container's real LABEL, the spec's "with a label" case) and the default toggle reads **"Worn by default by every character in this group"** — v4 `:103`. The generic `"…who can reach this item"` string I first saw is v4's EDIT arm (`:96`), not a missing group arm | PASS |

## Part C — group wardrobe CRUD + transfers with components (P4.D112 units 2–3)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| C1 | CLAUDE | Group create / update / delete | In a group container: create a garment, edit it, delete it | **PASS — create / update / delete, all three group verbs live.** `groupWardrobeCreate` (Dogfood C1 Family Sash, `types:["top"]`) → the group list went 1 → 2; `groupWardrobeUpdate` proven in B2; `groupWardrobeDelete` fired after v4's confirm sentence **`Delete "Dogfood C1 Family Sash"? This cannot be undone.`** and the store is back to its single `Wardrobe/Ansible Forge Mark Four dogfood B2.md`. Zero 500s. ⓘ **Harness note:** delete is guarded by `window.confirm`, which the Browser pane auto-dismisses — the step needs a confirm stub, and an un-stubbed run looks exactly like a dead button | PASS |
| C2 | CLAUDE | The group create refusals | Create with an empty title; create with an unknown slot type (force via the dispatch API) | **PASS — four refusals, nothing written.** Empty title / unknown slot `"hat"` / empty `types` array / `title: null` each answer **400 `{"kind":"bad-request","message":"Validation error"}`** — the flat envelope (the standing details-array deferral), and a re-list shows the group still holds exactly its one item, so no arm half-wrote | PASS |
| C3 | CLAUDE | **A real component-carrying move** (💸) | Take a composite outfit with components on a real character; Move it to General with **"Move the components along with it"** | **PASS — a real component-carrying MOVE, ids kept.** Moved *Singularity Armor 1 - Jackie* (3 components) Wardrobe Design → **Sebold Family**, components=move. The group went 1 → **5**; **every id is the source id** (`1edb14d3` outfit, `506dfa73`/`7cb3ac66`/`56e1085d` components) — a move mints nothing — and the outfit's refs resolve inside the group. The project went 28 → **24** with all four gone. The move dialog offers v4's three radios with the first checked: **Move the components along with it** / Copy the components (originals stay behind) / Leave the components behind | PASS |
| C4 | CLAUDE | A component-carrying **copy** | Copy a composite to a project with components=copy | **PASS — a real component-carrying COPY, ids minted and refs rewired.** Copied *Singularity Armor 1 - Friday* (3 components) from Wardrobe Design → **Quilltap General** with components=copy. General went **14 → 18** (outfit + 3 components). Every landed id is **newly minted** — none equals a source id — and the copied outfit's `componentItemIds` are the three NEW ids, all resolving inside General (`b5109738`→White Pointed Boots, `ff86faac`→White Form-Fitting Catsuit, `0d2219df`→Cascading Red Hair). The source project is untouched at **28** items with its original ids and refs intact | PASS |
| C5 | CLAUDE | The copy+move refusal is unreachable | In the transfer dialog, choose **Copy** on a composite | **PASS — the illegal combination is unreachable, and the legend counts right.** Copy on *Singularity Armor 1 - Friday* offers exactly **two** radios — **"Copy the components along with it"** (checked, the default) and **"Copy the outfit alone"**. **"Move the components along with it" is absent**, so copy+move cannot be composed in the UI at all — v4's chosen shape for the refusal. Above them: *"This outfit bundles 3 components"* (plural, correct) and v4's *"All or nothing — the choice covers every component, nested pieces included…"* paragraph | PASS |
| C6 | CLAUDE | Known-home hiding | Open Transfer on a garment that lives in General | **PASS — the known home is dropped by scope AND id.** Transferring an item that lives in **Wardrobe Design**, the destination select offers **52** options — General, all EIGHT other projects, both groups, and every character — with **`project:23209429-…` (Wardrobe Design) absent**. General is preselected because it is not this item's home (the skip-the-preselect rule is the General-home case) | PASS |
| C7 | CLAUDE | Components omitted → unresolved refs | Move a composite with **components: none** to a container where its refs cannot resolve | **PASS — `unresolvedComponentIds` and its log line, both on real data.** Moved *Singularity Armor 1 - Abigail* (4 components) → **Constellation** with components=none. `HTTP 200`, `"componentsTransferred":0`, and `"unresolvedComponentIds":["f4b2eb37…","b5fabd4f…","eb646111…","498ce023…"]` — all four. The server log carries v4's error line with every field: `[WardrobeTransfers v1] Transferred outfit did not read back with its planned component references` + `outfit_found_at_destination=true`, `planned_component_ids`, `read_back_component_ids=[]`, `unresolved_component_ids`, `destination_scope="group"`. Nothing is rendered for it — v4's client never reads the field | PASS |
| C8 | CLAUDE | The collision refusal sentence | Force a destination id collision (transfer an item into a container that already holds that id) | **PASS — v4's sentence, quirk and all, and nothing written.** A collision was engineered the way a real one arises (a hand-edited vault): `documentWrite` planted `Wardrobe/Decoy Belt.md` carrying `id: 477e8247-…` into the Sebold Family store, then the REAL *Black Leather Belt* (that same id, in Wardrobe Design) was moved there. **HTTP 400 `{"error":"An item with the ID of \"Black Leather Belt\" already exists at the destination"}`** — v4's title-in-"the ID of" quirk byte-for-byte. All-or-nothing held: the belt is still in the project and the group gained nothing. Decoy removed | PASS |

## Part D — the slug-collision vault fix (P4.D112 unit 1)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| D1 | CLAUDE | Do real colliders already exist? | Query the real vault for two items on one character whose titles slugify identically | **PASS (as a measurement): the real instance has ZERO natural colliders.** All `Wardrobe/*.md` names were slugified across **44 containers** — not one slug is claimed twice. So the collision had to be built, which is what D2 does. Worth recording: the fix protects against a shape this instance has never happened to hit, so no existing outfit here was mis-wired | PASS |
| D2 | CLAUDE | The emitted file references colliders by UUID | Create/keep two same-slug garments and one outfit referencing one of them, then force a vault re-emit (any write to that character's wardrobe) | **PASS — the two-pass slug map, live.** Built the differential's own shape in **Sebold Family**: *Test Boots* + *test   BOOTS!* (same slug `test-boots`), a unique *Test Coat*, and *Test Ensemble* referencing Test Boots + Test Coat. The emitted `Wardrobe/Test Ensemble.md` reads `componentItems: [b0af0cd7-3181-4bd3-9d67-4535f1f11f9a, test-coat]` — **the collider by its exact UUID, the unique sibling still by slug**. Pre-fix the ambiguous slug went to the first item in WRITE order while the reader resolved it in FILENAME order, which is the silent rewiring this closes | PASS |
| D3 | CLAUDE | The reader still resolves | Reopen the outfit in the editor / wear it | **PASS — the reader (deliberately unchanged) resolves it.** Reading the group back, *Test Ensemble* resolves to **Test Boots** (`b0af0cd7…`) and *Test Coat* — the same-slug twin `0f84690a…` never appears. UUID-out, UUID-back, correct item | PASS |

## Part E — downloads + `Content-Disposition` (P4.D114 units 1, 3, 5)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| E1 | CLAUDE | **The stored basename wins** (real-data proof) | Find a real blob whose stored name is `*.webp` but whose `originalFileName` is a `.HEIC`/`.png`/`.jpg` (query `doc_mount_blobs` first), then download it | **PASS — the stored basename wins on real transcoded data.** `GET /api/v1/mount-points/96eab235-…/blobs/Image.webp` answers `content-disposition: inline; filename="Image.webp"` while that link's `originalFileName` is **`Image.png`** — the exact WebP-transcode shape v4's why-comment is about, and one no fresh fixture has. (3,320 blob rows on this instance; the `.webp`-stored / `.png`-original population is large) | PASS |
| E2 | CLAUDE | The RFC 5987 arm on a real name | Find a blob whose stored basename has non-ASCII characters (query first) and download it | **PASS — both arms, on names the real instance already had.** (a) BLOB arm, a macOS screenshot carrying U+202F NARROW NO-BREAK SPACE (`hex` verified `…E280AF…`): `inline; filename="Screenshot 2026-06-11 at 1.54.54_PM.webp"; filename*=UTF-8''Screenshot%202026-06-11%20at%201.54.54%E2%80%AFPM.webp`. (b) DOCUMENTS-FALLBACK arm, a journal `.md` with an em dash (`E28094`): `inline; filename="2026-06-10 _ The First Entry.md"; filename*=UTF-8''2026-06-10%20%E2%80%94%20The%20First%20Entry.md`. The non-ASCII → `_` ASCII fallback matches v4's real helper byte-for-byte (the lane measured `Supar__'s cat.webp` the same way) | PASS |
| E3 | CLAUDE | 💸 Photos Download + Copy | My Photos → open a photo → the modal footer | **PASS — both buttons, on a real 435-photo library.** The detail modal's footer carries v4's four in order: **Remove from this album | Download | Copy | Close**, with `title="Copy image to clipboard"` on the Copy button. **Download** fetched the bytes and handed them to `triggerBlobDownload`: the anchor is `download="2026-08-25T15-04-17.089Z-kissing-abigail-while-ariel-watches-at-the-north-dock.webp"` (the stored `fileName`, matching the vault path the modal shows) with a `blob:` href — no bare link. **Copy** wrote a `ClipboardItem` of type **`image/png`** (the canvas transcode — the stored bytes are WebP) and toasted v4's **`Image copied to clipboard`**. The no-bytes catch arm is left to the e2e beat, which drives it against a live 404 | PASS |
| E4 | CLAUDE | Scriptorium file download | Scriptorium → a store → expand a file row | **PASS — and on a row where the two names genuinely disagree.** Expanded a BLOB row in **Project Files: The Estate** (1,438 files): the detail row reads `MIME: image/webp` / **`Original name: story_background_1781054047424.webp`** while the file is stored as `2026-06-10T01-15-37.592Z-jackie-and-sunny-and-ariel-driven-by-prospero.webp`. Download → the anchor is `download="2026-06-10T01-15-37.592Z-…-prospero.webp"` with a `blob:` href — **the file's name, not the blob's `originalFileName`**. That is v4's why-comment demonstrated on data that already had the shape | PASS |
| E5 | CLAUDE | The image-gallery hover download | A chat gallery / image gallery tile | **BLOCKED — `qt-image-gallery` has no v5 host, and that predates this round.** Its own header records it: v4's two mount sites (the avatar selector and the AI wizard) are unported, and v4's `GET /api/v1/images?tagType&tagId` list route has no v5 verb, so the component takes `images` as an INPUT and is exercised only by its spec. The `af1bc479` hover Download button landed inside it correctly (bottom-left, `stopPropagation`, 3 new specs) but there is no screen in v5 that renders the component. Not a finding; recorded so the next pass does not go looking | BLOCKED(no host) |
| E6 | CLAUDE | The avatar-preview download rider | Wardrobe → avatar generation pane → download a preview | **DEFERRED — needs one real image generation.** The avatar pane's Download button only exists after a preview has been generated, which is live image spend. The rider is unit-pinned (3 new specs + one re-pinned: the hidden anchor is gone, the blob is fetched, and a non-ok fetch toasts `Failed to download avatar preview`). Offered in the human remainder | DEFERRED-TO-HUMAN |
| E7 | HUMAN | The generate-image download | Generate an image, then Download it | Real image spend — deferred by cost. The `res.ok` guard is unit-pinned | PENDING |

## Part F — the projects CREATE schema + toasts (P4.D114 units 2, 4)

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| F1 | CLAUDE | 💸 **A create refusal reads v4's sentence** | Force a refused create through the dispatch API (e.g. `color: "blue"`, or a 101-char name the UI's `maxlength` blocks) | **PASS — v4's fixed sentence, never the server's.** The dialog caps both fields (`maxlength` 100 / 2000) so a refusal is unreachable by typing — the description was set past the cap through the real input event, and the real dialog → real dispatch → real 400 path ran. The accumulating toast observer (ticks confirmed live) caught **`qt-toast qt-toast-error` / "Failed to create project"** — not `Validation error`, which is what the server actually answered. The toast had already left the DOM by the next read, so a snapshot would have reported nothing | PASS |
| F2 | CLAUDE | The four newly-refused shapes | `description` 2001 chars; `instructions` 10001; `icon` 51; `characterRoster: ["not-a-uuid"]`; `allowAnyCharacter: "yes"` | **PASS — nine shapes refused, nothing written.** `description` 2001 / `instructions` 10001 / `icon` 51 / `color:"blue"` / `characterRoster:["not-a-uuid"]` / `characterRoster:null` / `allowAnyCharacter:"yes"` / `allowAnyCharacter:null` / `name` 101 chars each answer **400 `Validation error`**. **Every one of these answered 200 before this round** — and the null roster was actually written. `SELECT count(*) FROM projects WHERE name LIKE "DF F2%"` → **0**: no arm half-wrote | PASS |
| F3 | CLAUDE | The four null-tolerant keys (bug 98 proper) | `description: null`, `instructions: null`, `color: null`, `icon: null` | **PASS — bug 98 proper.** `description: null`, `instructions: null`, `color: null`, `icon: null` each answer **200** and create the row; all four are present in `projects`. This is the behaviour v4 changed TO at `c93ec7ff`, and the measurement at the pin showed v5 already had it | PASS |
| F4 | CLAUDE | The whitespace-only name is accepted | Create with `name: "   "` | **PASS — the v5-only refusal is gone.** `{"name":"   "}` answers **200** and the row exists in `projects` with the literal three-space name. `.min(1)` runs on the RAW string and this schema has no `.trim()`, so v4 accepts it; v5 used to 400 | PASS |
| F5 | CLAUDE | The success toast on both hosts | Create a project from the home Quick Actions row AND from Prospero | **PASS — both hosts, same sentence.** `Project created successfully!` (`qt-toast-success`) from the **home Quick Actions** row and from **Prospero**, each closing the dialog and writing the row (`DF F5 home host`, `DF F5 prospero host` both in `projects`). v5 reaches both through one shared dialog, so v4's two hosts agreeing is structural here. ⓘ **Trap:** a synthetic `.click()` on the footer's `form="qt-project-create-form"` submit button did nothing on the Prospero host; a real mouse click submitted. The jsdom sibling of this is already a known trap — it has a browser-side cousin | PASS |

## Part G — the standing 💸 queue

| # | Owner | Step | Gesture | Expected + how verified | Status |
|---|---|---|---|---|---|
| G1 | CLAUDE | 💸 **finding #98's own scenario** (P4.59) | A chat turn that calls `search_web`, with **no `SERPER_API_KEY` in the environment** | **PASS — finding #98's own scenario, closed.** The server process carries **no `SERPER_API_KEY`** (checked with `ps eww` on pid 30418) and the instance's `api_keys` holds the `SERPER` row **v4 itself wrote** (40-char key, `isActive=1`). `providerList` now returns it as a first-class row — `{"id":"SERPER","displayName":"Serper Web Search","type":"search","configRequirements":{"requiresApiKey":true,"apiKeyLabel":"Serper API Key"}}` — and `chatRunTool{"toolName":"search_web"}` on a real chat came back **`"Found 5 search results:"`** with live URLs. The exact thing the finding said could not work without an environment variable | PASS |
| G2 | CLAUDE | 💸 The five `[Title Update]` log lines (P4.61) | Drive a real title cycle on a chat | **PASS — a real title cycle, three lines in the real `combined.log`.** Forced cheaply rather than waited for: the early checkpoints are interchanges **2, 3, 5, 7, 10**, so a throwaway chat (Friday on the local `qwen3.5-9b` profile) reached the first checkpoint in two turns. `background_jobs` shows `TITLE_UPDATE … COMPLETED` at 17:39:43, and the log carries the success trio verbatim: **`[Title Update] Chat 751720ae-… - needsNewTitle: true, reason: The current title 'DF G2 title cycle' is generic and technical, not reflecting the poetic, introspective nature of the conversation…`**, **`[Title Update] Updated title for chat 751720ae-… to: "…"`**, and **`[Title Update] Queued story background generation`**. The other three ported lines are failure arms (`Failed for chat`, `checkpoint burned`, `Failed to queue story background generation`) and correctly stayed silent — the silence leg, for free | PASS |
| G3 | CLAUDE | 💸 Pascal's other three write paths (P4.D35) | A custom tool whose `effects` target a project / group / chat-state write | **DEFERRED a fourth time — but with the recipe now written down so the next pass need not re-derive it.** The character-vault path closed on 2026-08-21; the other three are the `state.<key>` tiers, and `pascal/side_effects.rs` documents how a tier is chosen: the tier is *where the first path segment already exists*, defaulting to the **chat** tier when the key is found nowhere. So the walk is (a) a tool with `effects: [{target: "state.dogfoodCounter", …}]` run in a project-less chat → chat tier; (b) the same key pre-seeded at the project tier, run in a chat inside that project → project tier; (c) the group tier, which additionally needs the exactly-one rule (`groupTier.status == "single"`). Each is verifiable from the effect's own `{"target","previous","next","tier"}` record plus the tier store. That is its own sitting | DEFERRED |
| G4 | HUMAN | 💸 The raised Brahma budget on a deep query | A Brahma run that genuinely exhausts 25 agent turns | Expensive and non-deterministic; the wire half is already proven | PENDING |
| G5 | HUMAN | 💸 Memory dedup + conversation-summaries first run | Settings → the maintenance cards | Real batch spend across the whole Friday corpus — the human's call | PENDING |
| G6 | HUMAN | 💸 The candid story-background arm (P4.D94) | Needs a dangerous-compatible image profile | Real image spend + a rerouted profile; the concealed arm is already byte-proven | PENDING |

---

## Traps banked by this walk

- **The kebab menu is clipped by the list's own scroll container**, and with a
  short list four of its six actions land below the clip line — invisible and
  unclickable until the inner list is scrolled. **v4 is identical** (same
  `flex-1 overflow-y-auto space-y-1 max-h-[55vh] pb-12` container at
  `wardrobe-control-dialog.tsx:1186`, same `absolute right-0 top-full mt-1 z-30`
  menu), so it is a faithfully ported wart, not a v5 defect. A candidate
  upstream nicety; recorded here so the next walk does not chase it.
- **Wardrobe delete is guarded by `window.confirm`**, which the Browser pane
  auto-dismisses. An un-stubbed run looks exactly like a dead button. Stub it
  (`window.confirm = () => true`) and assert the captured sentence.
- **A synthetic `.click()` on a modal-footer `form="…"` submit button can be
  inert in a real browser too** — the Prospero create needed a real mouse
  click where the home host accepted `.click()`. The jsdom version of this is
  already a known trap; it has a browser-side cousin.
- **The transfer POST does not go through `/api/dispatch`** — it is
  `POST /api/v1/wardrobe/transfers`, so a dispatch-only fetch tap sees the
  refetches and not the transfer. Verify transfers through the DB or by
  driving the REST route directly.
- **Toasts leave the DOM fast.** Every toast in this walk was caught by an
  accumulating `MutationObserver` whose own tick counter was checked; a
  snapshot taken a second later reported nothing at all. (The standing lesson
  from finding #99, confirmed again twice.)
- **`quilltap db` flags:** the mount partition is `--mount-points`, not
  `--mount-index`; `api_keys` spells its column `key_value`, and
  `connection_profiles` spells its model `modelName`.
- **Forcing a title cycle is cheap**: the early checkpoints are interchanges
  **2, 3, 5, 7, 10**, so a brand-new chat reaches the first one in two turns.
  The manual `chatRegenerateTitle` verb is a *different path* and emits none
  of the `[Title Update]` lines.

## Results log

_(each row's evidence is in its table cell above)_
