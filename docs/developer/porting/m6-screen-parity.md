# M6 — the v4↔v5 screen-parity checklist + the v4 retirement criteria

> **Produced by work order P4.8** (lane C of the P4.6aw ∥ P4.6ax ∥ P4.8
> round), 2026-07-16. This is the M6 decision instrument: after it,
> *"what is left before v4 retires"* is answerable by reading one file.
>
> **v4 baseline: `02865bdb`** (`~/source/quilltap-server`; drift-checked at
> lane start — `git log 02865bdb..HEAD` empty). **v5 baseline: `41e1a0a`**
> on `main`.
>
> **This document records; it does not fix.** Every finding below is
> evidence, not a patch. Fixes belong to the orders proposed in §4.

## How to read this

**The verdict set** (closed — every row carries exactly one):

| Verdict | Meaning |
| --- | --- |
| **PARITY** | The surface exists in v5. Cites the v5 route/component + the order that landed it. |
| **DIVERGENCE-DOCUMENTED** | v5 deliberately differs, and the difference is recorded somewhere durable. |
| **MISSING** | No v5 counterpart. Carries a proposed order slug + size + dependencies (§4). |
| **WON'T-PORT** | Dead in v4, a v4-internal workaround, or retired by a locked decision — with the evidence. |

**The evidence rule.** Every row cites v4 `file:line` at `02865bdb` **and**
v5 `file:line` on main (or an absence proof). Rows without both sides do
not ship. Absence proofs in this document were produced with `git grep`
against a control query that verifiably hits — a plain recursive `grep -r`
silently returned zero on a matching file during this lane, so any absence
proof re-derived later should re-validate its tool the same way.

**Sibling-lane note (settled at unification).** Six rows were reviewed
while the two code lanes were still in flight: `__bold__` on-type, the
form-field source-mode toggle, and the GFM table transformer (lane B,
`p4.6ax-editor-riders.md:3`); the cost-estimator consolidation, the stale
seam-note sweep, and the depiction-guidelines `disabledHint` arm (lane A,
`p4.6aw-rust-riders-depiction-hint.md:3`). All six LANDED in the same
unification that landed this checklist (the P4.6aw ∥ P4.6ax ∥ P4.8
round), and their rows below read accordingly.

---

## 0. Headline findings — read these first

These four changed or corrected the order's own seed inventory. Each is
recorded, not fixed.

### F1 — v4's tabbed workspace is its DEFAULT shell, not an experiment

The order's seed said to *"weigh WON'T-PORT vs MISSING against the flag
default."* The flag defaults **ON**:

```
lib/config/feature-flags.ts:21-22
export const WORKSPACE_TABS_ENABLED: boolean =
  process.env.NEXT_PUBLIC_WORKSPACE_TABS !== '0'
```

`!== '0'` means unset ⇒ enabled, and `grep -n "WORKSPACE" .env.local
.env.example` returns nothing, so a stock v4 checkout runs with tabs on.
`app/page.tsx:13` then calls `redirectToWorkspaceTab('home')` — **`/`
redirects to `/workspace?open=home`**. Fourteen more routes do the same
(§1.6). v4's real, default, out-of-the-box UX is a keep-alive tabbed
workspace with split panes; the per-surface routes v5 implements are v4's
**opt-out** mode (`NEXT_PUBLIC_WORKSPACE_TABS=0`).

Two v4 docstrings assert the opposite and are stale — do not let them
mislead a future reader: `lib/navigation/workspace-redirect.ts:6-7` says
*"When the flag is off (the default)"*, and `app/workspace/page.tsx:11-13`
still describes the route as dev-only.

**Why this matters for retirement, and why it is not a blocker.** No
*capability* is lost: v5's route model is byte-for-byte the shape v4 ships
under its own supported opt-out flag, and every workspace tab kind maps to
a v5 route or a known gap. What is lost is a *workflow* — keep-alive tabs,
split panes, cross-pane drag. So this is MISSING (§1.6), it is the single
largest parity gap in the product, and **it is a human decision, not a
mechanical one**: retiring v4 without it asks every user who lives in the
tabbed workspace to accept single-surface navigation. §5 states it as an
explicit retirement gate requiring a human ruling rather than assuming it.

### F2 — the seed conflated two distinct v4 LLM-log surfaces

The seed said *"LLMLogViewerModal → the v5 Inspector slide-over is a
DIVERGENCE-DOCUMENTED row (P4.6as)."* That is two different v4 components:

- **`components/chat/LLMInspectorPanel.tsx`** — the in-chat slide-over,
  rendered at `app/salon/[id]/SalonView.tsx:1696`. **v5 ports this 1:1**
  (`apps/web/src/app/chat/llm-inspector-panel.ts:138`, consumed at
  `apps/web/src/app/screens/salon/salon-conversation.ts:18,137,362`). This
  is a **PARITY** row, not a divergence.
- **`components/chat/LLMLogViewerModal.tsx`** — a separate modal with **no
  salon caller at all**. Its two entry points are Settings → Data & System
  (`components/tools/llm-logs-card.tsx:133`) and character **edit**
  (`components/characters/LLMLogsSection.tsx:102`, mounted
  `app/aurora/[id]/edit/CharacterEditView.tsx:390`). **v5 has neither**
  (`git grep -niE "LLMLogViewerModal|llm-logs-card" -- 'apps/web/src/app/**/*.ts'`
  → 0). This is a **MISSING** row.

The single seed row is therefore split into §1.2 (PARITY) and §2.6
(MISSING). The P4.6as divergence that *is* real is narrower: the v5 panel
declares `role="dialog"` only while open, where v4's is a permanent phantom
modal.

### F3 — the chat-level Core Whisper override is a gap the seed never named

v4 exposes Core Whisper at **three** inheritance levels (null = inherit
chat → character → global): the global Settings card
(`components/settings/core-whisper/CoreWhisperSection.tsx`, mounted
`components/settings/tabs/TemplatesPromptsTabContent.tsx:28`), the
character field (`app/aurora/[id]/edit/components/CharacterBasicInfo.tsx:174-192`),
and the **per-chat override** (`app/salon/[id]/SalonView.tsx:1760-1763` →
`ChatSidebar`; types at `app/salon/[id]/types.ts:230-233`).

v5 has exactly one of the three. `git grep -niE
"coreWhisperEnabled|coreWhisperInterval" -- 'apps/web/src/app/**/*.ts'`
returns only `screens/characters/edit/character-form.ts` and
`screens/characters/edit/details-tab.ts` — the character level. The seed
named only the global Settings card as deferred; **the chat-sidebar
override was unrecorded anywhere**. Both are rows in §1.5 / §1.2, and the
proposed order `p4.9h` covers the chain as one unit, because porting the
global card without the chat override would ship a visibly broken
inheritance story.

### F4 — three stale v5 docstrings (record only, per Tier 3)

Each overstates how little is ported; each is a comment-only fix belonging
to a future lane, **not** to this read-only lane.

| File | Claim | Reality |
| --- | --- | --- |
| `apps/web/src/app/screens/settings/settings.ts:16-20` | *"one populated slice — AI Providers + Appearance. The remaining five tabs render the … placeholder."* | **6 of 7 tabs are real**; only `system` (Data & System) is a placeholder (`settings.ts:66-68`). |
| `apps/web/src/app/screens/settings/placeholder-tab.ts:6-8` | Lists Chat, Commonplace Book, Images, Templates & Prompts, Data & System as placeholders. | Four of those five are now real. |
| `apps/web/src/app/shell/shell.ts:19-20` | *"Only the Salon (Chats) route is live this round; the rest stay disabled."* | **6 of 7 nav items are live**; only `photos` is disabled (`shell.ts:44-50`, `route: null`). |

The `settings.ts:20` row is the one the order named explicitly. The other
two were found during the walk.

---

## 1. The routed-screen checklist

### 1.1 Home

| Surface | v4 @ `02865bdb` | v5 @ main | Verdict |
| --- | --- | --- | --- |
| Home dashboard | `app/page.tsx` → `components/homepage/HomeView.tsx` | `apps/web/src/app/screens/home/home-page.ts`, route `app.routes.ts:95-99` | **PARITY** (P4.6au/av) |
| Quick actions row | `components/homepage/QuickActionsRow.tsx` | `screens/home/quick-actions-row.ts` | **DIVERGENCE-DOCUMENTED** |
| New-Chat from a character card | `components/homepage/CharacterCard.tsx:83` opens `NewChatModal` | `screens/home/home-character-card.ts:21` navigates to `/salon/new?characterId=` | **DIVERGENCE-DOCUMENTED** |
| Recent-chats quick-hide filtering | `components/homepage/RecentChatsSection.tsx` via `useQuickHide` | absent — `screens/home/recent-chats-section.ts:13-17` | **MISSING** → `p4.9d` |
| Characters-section tag filtering | `components/homepage/CharactersSection.tsx` via `useQuickHide` | absent — `screens/home/characters-section.ts:34-36` | **MISSING** → `p4.9d` |

The quick-actions divergence is that v5 ships five of v4's six actions and
omits **Generate Image**, because `/generate-image` is unported; the
omission is deliberate and pinned by a test rather than left as a dead
link (`screens/home/quick-actions-row.ts:14-16`, asserted at
`screens/home/home.spec.ts:221`). It stops being a divergence and becomes
PARITY the moment `p4.9b` lands. The card-Chat divergence is recorded at
`status-log.md:15520-15521`.

### 1.2 Salon (chats)

| Surface | v4 | v5 | Verdict |
| --- | --- | --- | --- |
| Chat list | `app/salon/page.tsx:11` → `app/salon/SalonListView.tsx` | `screens/salon/salon-list.ts`, route `app.routes.ts:19-22` | **PARITY** (P4.6b) |
| Conversation | `app/salon/[id]/page.tsx` → `SalonView.tsx` | `screens/salon/salon-conversation.ts`, route `app.routes.ts:31-35` | **PARITY** (P4.6b/c) |
| New chat | `app/salon/new/page.tsx:19-179` | `screens/new-chat/new-chat-page.ts`, route `app.routes.ts:23-26` | **PARITY** (P4.6q) |
| Terminal pop-out | `app/salon/[id]/terminal/[sessionId]/page.tsx:14-80` | `screens/salon/terminal-popout.ts`, route `app.routes.ts:27-30` | **PARITY** (P4.6u) |
| LLM Inspector slide-over | `components/chat/LLMInspectorPanel.tsx:42`, mounted `SalonView.tsx:1696` | `chat/llm-inspector-panel.ts:138`, mounted `salon-conversation.ts:362` | **PARITY** (P4.6as) — see F2 |
| Autonomous-include toggle | `components/dashboard/nav-user-menu-quick-hide.tsx:100-105` (user menu) | `screens/salon/autonomous-visibility.ts:13-14` (salon-list header) | **DIVERGENCE-DOCUMENTED** |
| Tag-hide / hide-dangerous filtering | `components/providers/quick-hide-provider.tsx:183-209` | absent — `screens/salon/salon-list.ts:24-28` | **MISSING** → `p4.9d` |
| Per-chat Core Whisper override | `SalonView.tsx:1760-1763` → `ChatSidebar` | absent (F3) | **MISSING** → `p4.9h` |
| Boxed `ChatCostSummary` variant | `components/chat/ChatCostSummary.tsx:138-175` | n/a — v5 ports compact only | **WON'T-PORT** |
| Client `detailed=true` cost breakdown | `app/api/v1/chats/[id]/handlers/get.ts:222-225` | n/a | **WON'T-PORT** |

The autonomous-toggle divergence is placement-only and was engineered to
be reversible: v5 has no user menu yet, so the toggle rides the salon-list
header **but persists to v4's exact localStorage key**
(`quilltap.quickHide.includeAutonomousRooms`), so a future user-menu port
inherits the setting for free (`autonomous-visibility.ts:6-14`).

**The two WON'T-PORT rows are verified dead in v4, not merely unused by
v5.** `ChatCostSummary` has exactly one caller repo-wide —
`SalonView.tsx:1014` — and it passes `variant="compact"` at `:1017`, so the
`'default'` boxed branch is unreachable code. `detailed=true` has zero
clients: the only fetcher of `?action=cost` is `ChatCostSummary.tsx:58`,
which never sends the param, so `getDetailedChatCostBreakdown`
(`lib/services/cost-estimation.service.ts:193`) is a dead server path —
and its own comment at `:232` (*"we'd need to re-estimate costs"*) suggests
it was never finished. Porting either would mean porting dead code. This
renders the verdict lane A's Tier 3 delegated here
(`p4.8-m6-screen-parity-review.md:145-149`), and closes the standing pool
item at `status-log.md:14678-14679`.

### 1.3 Characters (Aurora)

| Surface | v4 | v5 | Verdict |
| --- | --- | --- | --- |
| Roster | `app/aurora/page.tsx` → `AuroraView.tsx` | `screens/characters/list/characters-list.ts`, `app.routes.ts:36-40` | **PARITY** (P4.6g) |
| New character | `app/aurora/new/NewCharacterView.tsx` | `screens/characters/new/new-character.ts`, `app.routes.ts:41-45` | **PARITY** (P4.6g) |
| Detail (9 tabs) | `app/aurora/[id]/view/CharacterDetailView.tsx` | `screens/characters/view/character-detail.ts:46-58`, `app.routes.ts:55-59` | **PARITY** (P4.6g/i/j) |
| Edit | `app/aurora/[id]/edit/CharacterEditView.tsx` | `screens/characters/edit/character-edit.ts`, `app.routes.ts:50-54` | **PARITY** (P4.6g) |
| Character LLM-logs section | `CharacterEditView.tsx:390` → `LLMLogsSection.tsx:102` | absent (F2) | **MISSING** → `p4.9g` |
| Depiction-guidelines no-vault hint | proactive suppression | proactive suppression on both appearance tabs | **PARITY** (P4.6aw) |
| Group detail | `app/aurora/groups/[id]/GroupDetailView.tsx` | `screens/groups/group-editor.ts`, `app.routes.ts:46-49` | **PARITY** (P4.6l) |

The character detail screen's nine tabs match v4's `CHARACTER_TABS`
one-for-one (`character-detail.ts:45-58`). The `disabledHint` row lands
with lane A (`status-log.md:14691-14693`).

### 1.4 Prospero (projects), Files, Scriptorium, Scenarios

| Surface | v4 | v5 | Verdict |
| --- | --- | --- | --- |
| Project list | `app/prospero/ProsperoView.tsx` | `screens/prospero/prospero-list.ts`, `app.routes.ts:64-67` | **PARITY** (P4.6l) |
| Project detail | `app/prospero/[id]/ProjectDetailView.tsx` | `screens/prospero/project-detail.ts`, `app.routes.ts:68-72` | **PARITY** (P4.6l) |
| Files browser | `app/files/FilesView.tsx` | `screens/files/files-browser.ts`, `app.routes.ts:60-63` | **PARITY** (P4.6af) |
| Scriptorium grid | `app/scriptorium/ScriptoriumView.tsx` | `screens/scriptorium/scriptorium-list.ts`, `app.routes.ts:77-81` | **PARITY** (P4.6z) |
| Store detail | `app/scriptorium/[id]/DocumentStoreDetailView.tsx` | `screens/scriptorium/store-detail.ts`, `app.routes.ts:82-85` | **PARITY** (P4.6z) |
| Scenarios (general) | `app/scenarios/ScenariosView.tsx` | `screens/scenarios/scenarios-page.ts`, `app.routes.ts:73-76` | **PARITY** (P4.6o) |
| Project-page backgrounds / backdrop arbitration | workspace-backdrop | absent — Salon is v5's only background source | **DIVERGENCE-DOCUMENTED** |

The backdrop row is a consequence of F1, not an independent gap: v5 has no
tabbed workspace, so there is nothing to arbitrate between
(`status-log.md:12477-12481`). It resolves with `p4.9j` or never.

### 1.5 Settings

v4 and v5 both ship a seven-tab hall in the same order —
`app/settings/SettingsView.tsx:31-39` vs
`apps/web/src/app/screens/settings/settings.ts:82-90`.

| Tab | v4 content | v5 | Verdict |
| --- | --- | --- | --- |
| AI Providers | `ProvidersTabContent` (`SettingsView.tsx:44`) | `providers/providers-tab.ts` (`settings.ts:48-50`) | **PARITY** (P4.6e) |
| Chat | `ChatTabContent` (`:46`) | `chat/chat-tab.ts` (`settings.ts:54-56`) | **PARITY** (P4.6an) |
| Appearance | `AppearanceTabContent` (`:48`) | `appearance/appearance-tab.ts` (`settings.ts:51-53`) | **PARITY** (P4.6e) |
| Commonplace Book | `MemorySearchTabContent` (`:50`) | `memory/memory-tab.ts` (`settings.ts:57-59`) | **PARITY** (P4.6t) |
| Images | `ImagesTabContent` (`:52`) | `images/images-tab.ts` (`settings.ts:60-62`) | **PARITY** (P4.6r/aq/at) |
| Templates & Prompts | `TemplatesPromptsTabContent` (`:54`) | `templates/templates-tab.ts` (`settings.ts:63-65`) | **PARITY (partial)** — see below |
| **Data & System** | `DataSystemTabContent` (`:56`) | **placeholder** (`settings.ts:66-68`) | **MISSING** → `p4.9g` |
| Settings wizard | `app/settings/wizard/SettingsWizardView.tsx` | `screens/settings/wizard/wizard-screen.ts`, `app.routes.ts:86-90` | **PARITY** (P4.6e) |

Templates & Prompts renders its Roleplay Templates card for real and
carries **two named card-level deferrals** — v4's Prompts library and
Aurora's Core Whisper card (`templates/templates-tab.ts:9-14`). Those are
MISSING rows under §2.5 and §1.5-adjacent, routed to `p4.9h`.

Sub-surfaces deferred inside otherwise-PARITY tabs, each already recorded:

| Sub-surface | v4 | v5 | Verdict |
| --- | --- | --- | --- |
| Structured per-provider parameters editor | providers tab | JSON textarea stands in — `status-log.md:8031-8033` | **DIVERGENCE-DOCUMENTED** |
| Image-profile Validate-key / List-models buttons | images tab | refusal-armed — `p4.6ai:12-14` | **MISSING** → `p4.9g` |
| Embedding Profiles sub-tab | memory tab | rendered as nothing — `status-log.md:8392-8395` | **MISSING** → `p4.9h` |
| Memory Deduplication card | memory tab | rendered as nothing — `status-log.md:8392-8395` | **MISSING** → `p4.9h` |
| Regenerate Conversation Summaries card | memory tab | rendered as nothing — `status-log.md:8392-8395` | **MISSING** → `p4.9h` |
| Template "Draft formatting instructions" helper | templates tab | absent — `status-log.md:8033-8035` | **MISSING** → `p4.9h` |
| Template/profile tag pickers | templates tab | absent — `status-log.md:8034-8035` | **MISSING** → `p4.9h` |
| Tags tab `quickHide` authoring | `components/settings/tags-tab.tsx:372` | absent | **MISSING** → `p4.9d` |

The memory cards deserve a note for whoever picks up `p4.9h`: they render
as **nothing**, not as dead cards — a deliberate choice recorded at
`status-log.md:8392-8395`, and the right precedent for every gap in this
document.

### 1.6 Screens with no v5 counterpart at all

| Surface | v4 | v5 absence proof | Verdict |
| --- | --- | --- | --- |
| **PhotosView** `/photos` | `app/photos/page.tsx:13` → `app/photos/PhotosView.tsx` (19 KB) | no route in `app.routes.ts`; nav item is a **disabled dead button** — `shell.ts:44-50` `route: null` | **MISSING** → `p4.9a` |
| **GenerateImageView** `/generate-image` | `app/generate-image/page.tsx:11` → `GenerateImageView.tsx` (13.9 KB) | no route; omission pinned at `quick-actions-row.ts:14-16`, `home.spec.ts:221` | **MISSING** → `p4.9b` |
| **ProfileView** `/profile` | `app/profile/page.tsx:11` → `ProfileView.tsx` (2.5 KB); entry `profile-menu.tsx:40-43` | `git grep -niE "path: *'(profile\|about)'"` → 0 | **MISSING** → `p4.9c` |
| **AboutView** `/about` | `app/about/page.tsx:11` → `AboutView.tsx` (21 KB); version badge `:52-55`; entry `profile-menu.tsx:45-48` | `git grep -niE "appVersion\|app_version\|versionBadge\|shields\.io"` → 0 | **MISSING** → `p4.9c` |
| **BrahmaConsoleView** | `components/brahma-console/BrahmaConsoleView.tsx:14-16`; dialog `BrahmaConsoleDialog.tsx`; entry `sidebar-footer.tsx:213-226` | `git grep -ni "brahma"` → 3 hits, **all non-UI**: `core-contract.ts:1514,1668` (wire enum), `ui/icon.ts:22` (orphaned icon) | **MISSING** → `p4.9i` |
| **Tabbed workspace** | `app/workspace/page.tsx:21-32`, `WorkspaceHost.tsx:29`, 21 tab kinds `TabView.tsx:71-119` | `git grep -niE "tabstrip\|tab-strip\|workspaceTab\|openTabs\|redirectToWorkspace"` → 0 | **MISSING** → `p4.9j` — see F1 |

`/photos` is the most user-visible of these: v5 **renders the nav item and
disables it**, so the gap is on screen in every session. `AboutView`
carries a wrinkle worth deciding rather than porting blind — its version
badge is a remote `img.shields.io` fetch (`AboutView.tsx:53`), along with
License/Docker/Discord badges, so a faithful port breaks offline, which is
squarely the deployment v5 targets. `p4.9c` should render the version
locally and record that as a deliberate divergence.

`BrahmaConsoleView.tsx` is a 460-byte `asTab` re-skin returning
`<BrahmaConsoleDialog asTab />` — the console's real body is the dialog, so
`p4.9i` ports one surface, not two. Its services already exist (W4.5b);
only the UI never landed.

### 1.7 Setup / unlock / startup

| Surface | v4 | v5 | Verdict |
| --- | --- | --- | --- |
| Startup progress | `components/loading/StartupProgress.tsx:152`, gated `app-layout.tsx:89-91` | `screens/startup/startup-screen.ts`, gated `app.ts:32-34` | **PARITY** (P4.5) |
| Unlock | `app/unlock/page.tsx:8-151` | `screens/unlock/unlock.ts`, `app.ts:29-31` | **PARITY** (P4.5) |
| Setup wizard | `app/setup/page.tsx:9-299` | `screens/setup/setup-wizard.ts`, `app.ts:26-28` | **PARITY** (P4.4u1) |
| Setup → profile/archetype | `app/setup/profile/page.tsx:13-32` (3 archetypes) | folded into `setup-wizard.ts` + first-run handoff `shell.ts:149` | **DIVERGENCE-DOCUMENTED** |
| Setup → providers | `app/setup/providers/page.tsx:9-19` | `settings/wizard?mode=setup` (`shell.ts:149`) | **DIVERGENCE-DOCUMENTED** |

v4 splits first-run across three routes chained by `navigateAfterSetup`
(`app/setup/page.tsx:20-52`); v5 folds them into one pre-router wizard plus
a `?mode=setup` handoff. Same states, different route decomposition.

### 1.8 Redirect-only aliases

| Surface | v4 | v5 | Verdict |
| --- | --- | --- | --- |
| `/foundry` + 11 `/foundry/*` deep-links | `app/foundry/*/page.tsx:4` (each a one-line `redirect()` to `/settings?tab=…`) | no equivalent | **WON'T-PORT** |
| `/characters`, `/characters/*` → `/aurora/*` | `app/characters/page.tsx:4` etc. | v5 **is** at `/characters` (`app.routes.ts:36-40`) | **WON'T-PORT** |
| `/personas`, `/personas/*` | `app/personas/page.tsx:4` (→ `/aurora?filter=user-controlled`) | no filtered alias | **WON'T-PORT** |
| `/projects`, `/chats`, `/tools`, `/dashboard` | `app/projects/page.tsx:4`, `app/chats/page.tsx:4`, `app/tools/page.tsx:4`, `app/dashboard/page.tsx:12` | destinations exist under v5 names | **WON'T-PORT** |

These are v4-internal navigation sugar: every one of them is a server
`redirect()` to a surface v5 already has, so no capability is lost — only
bookmark compatibility for URLs that were themselves aliases. Note the
naming inverts for characters: v4's *canonical* route is `/aurora` with
`/characters` as the alias; v5 made `/characters` canonical and dropped
`/aurora`. If bookmark continuity is ever wanted, it is a redirect table,
not a port — cheap, and deliberately out of scope here.

Two v4 curiosities found while proving these rows, recorded and not fixed:
`app/dashboard/layout.tsx:15` redirects to `/auth/signin`, which **does not
exist** (`ls app/auth` → no such file; `find app -type d -name auth` →
empty); and `/salon` is the **only** list route that does not redirect into
the workspace (`app/salon/page.tsx` has no `redirectToWorkspaceTab` call,
and `TabView.tsx:71-119` has no `salon-list` tab kind). Neither affects a
v5 verdict.

---

## 2. The screen-grade dialog inventory

### 2.1 Landed

| Dialog | v4 | v5 | Verdict |
| --- | --- | --- | --- |
| ChatCreationProgressModal | `components/new-chat/ChatCreationProgressModal.tsx`, provider-driven `creation-progress-provider.tsx:248` | `screens/new-chat/green-room-dialog.ts:7` ("The Green Room") | **PARITY** (P4.6q) |
| EditEnclaveModal | `components/new-chat/EditEnclaveModal.tsx` | `autonomous/edit-enclave-modal.ts:25` | **PARITY** (P4.6ad) |
| ScenarioEditorModal | `components/scenarios/ScenarioEditorModal.tsx` | `screens/scenarios/shared/scenario-editor-modal.ts:28` | **PARITY** (P4.6o) |
| StateEditorModal | `components/state/StateEditorModal.tsx` | `screens/prospero/state-editor-modal.ts:17` | **PARITY (partial)** — see below |
| FilePreviewModal | `components/files/FilePreview/FilePreviewModal.tsx` | `screens/files/file-preview-modal.ts:28` | **PARITY** (P4.6af) |
| CreateFolderModal | `components/files/FolderManagement/CreateFolderModal.tsx` | `screens/files/create-folder-dialog.ts:7` | **PARITY** (P4.6af) |
| MoveToProjectModal | `components/files/MoveToProjectModal.tsx` | `screens/files/move-to-project-dialog.ts:19` | **PARITY** (P4.6af) |
| OrphanCleanupModal | `components/files/OrphanCleanupModal.tsx` | `screens/files/orphan-cleanup-dialog.ts:17` | **PARITY** (P4.6af) |
| PhotoGalleryModal | `components/images/PhotoGalleryModal.tsx` | `images/photo-gallery-modal.ts:26` | **PARITY (partial)** — chat mode only |
| ImageModal | `components/chat/ImageModal.tsx` | `images/image-modal.ts:16` | **PARITY** (P4.6ac) |
| GenerateImageDialog | `components/images/…` via `ChatModals.tsx:209` | `images/generate-image-dialog.ts:26` | **PARITY (narrowed)** |
| Scriptorium's five store dialogs | `app/scriptorium/components/{Create,Edit,Delete,ConvertToDatabase,DeconvertToFilesystem}*.tsx` | `screens/scriptorium/{create,edit,delete,convert,deconvert}-store-dialog.ts` | **PARITY** (P4.6z) |
| Project create/delete | `app/prospero/components/{Create,Delete}ProjectDialog.tsx` | `screens/prospero/project-{create,delete}-dialog.ts` | **PARITY** (P4.6l) |
| MemoryCascadeDialog | via `ChatModals.tsx:454` | `chat/memory-cascade-dialog.ts:6` | **PARITY** (P4.6t) |
| FileConflictDialog | via `ChatModals.tsx:434` | `chat/file-conflict-dialog.ts:14` | **PARITY** (P4.6ac) |
| NewChatModal | `components/new-chat/NewChatModal.tsx`, 3 callers | **replaced by route** `/salon/new` (`app.routes.ts:23-26`) | **DIVERGENCE-DOCUMENTED** |

`StateEditorModal` is dual-host in v4 — project (`SettingsCard.tsx:102`)
**and** chat (`ChatModals.tsx:414`); v5 ports the project host only, so the
chat entry point is a MISSING row under §2.3. `PhotoGalleryModal` ports
chat mode and names its own deferrals at `images/photo-gallery-modal.ts:31`
(ChatGalleryImageViewModal, tag editing, prev/next navigation). The
generate dialog carries a recorded narrowing to four params
(`p4.6ai:15-16`).

### 2.2 The in-chat dialog family — all MISSING

Every row here is absent from v5; each absence proof is
`git grep -niE "<pattern>" -- 'apps/web/src/app/**/*.ts'` → 0 hits
(spec files excluded). This is the P4.6al "no-host dialog consumers"
family (`status-log.md:14681-14683`), widened by the order
(`p4.8:139-143`).

| Dialog | v4 | Proposed order |
| --- | --- | --- |
| CreateNPCDialog | `components/chat/CreateNPCDialog.tsx`, nested in `AddCharacterDialog.tsx:616` | `p4.9e1` |
| AddCharacterDialog | `components/chat/AddCharacterDialog.tsx`, `ChatModals.tsx:305` | `p4.9e1` |
| SummonFromLoreModal | `components/chat/SummonFromLoreModal.tsx`, nested `AddCharacterDialog.tsx:624` | `p4.9e1` |
| ComposeMailDialog | `components/chat/ComposeMailDialog.tsx`, `ChatModals.tsx:332` | `p4.9e2` |
| InsertAnnouncementDialog | `ChatModals.tsx:317` | `p4.9e2` |
| WhisperDialog | `components/chat/WhisperDialog.tsx`, `SalonView.tsx:1806` | `p4.9e2` |
| MergeConversationModal | `components/chat/MergeConversationModal.tsx`, `SalonView.tsx:1599` | `p4.9e3` |
| ReattributeMessageDialog | `ChatModals.tsx:351` | `p4.9e3` |
| BulkCharacterReplaceModal | `ChatModals.tsx:375` | `p4.9e3` |
| RunToolModal | `ChatModals.tsx:403` | `p4.9e3` |
| ChatToolSettingsModal | `ChatModals.tsx:386` — v5 records it at `core-contract.ts:858` | `p4.9e3` |
| ChatProjectModal | `ChatModals.tsx:187` | `p4.9e3` |
| StateEditorModal (chat host) | `ChatModals.tsx:414` | `p4.9e3` |
| SearchReplaceModal | `ChatModals.tsx:361` | `p4.9e3` |
| AllLLMPauseModal | `ChatModals.tsx:423` | `p4.9e3` |
| SelectLLMProfileDialog | `ChatModals.tsx:443` | `p4.9e3` |
| LibraryFilePickerModal | `ChatModals.tsx:246` | `p4.9e3` |
| ChatRenameModal | `ChatModals.tsx:196` | `p4.9e3` |
| StandaloneGenerateImageDialog | `ChatModals.tsx:269` — `status-log.md:10508-10513` | `p4.9b` |

Three of these were **not** in the order's seed and surfaced during the
walk: `SearchReplaceModal`, `AllLLMPauseModal`, `SelectLLMProfileDialog`,
plus `LibraryFilePickerModal` and `ChatRenameModal`. All live in the same
`ChatModals.tsx` barrel, which is the right unit of work: v4 centralizes
open/close state in `app/salon/[id]/hooks/useModalState.ts`, so `p4.9e*`
should port the barrel + the state hook together rather than dialog-by-
dialog.

Also standing in this family: the announcement/mail/RNG **gutter tools**
and drag-and-drop upload (`status-log.md:10465`) — the entry points these
dialogs hang off. `p4.9e2` must carry them or the dialogs have no opener.

### 2.3 Wardrobe

| Surface | v4 | v5 | Verdict |
| --- | --- | --- | --- |
| **Global wardrobe dialog** | `components/wardrobe/wardrobe-control-dialog.tsx:88` + `components/providers/wardrobe-dialog-provider.tsx`, mounted `app-layout.tsx:137`; 4 entry points | absent — `project-wardrobe-manager.ts:50` explicitly notes it *"does NOT use the character wardrobe-control"* | **MISSING** → `p4.9f` |
| WardrobeTransferDialog | `components/wardrobe/WardrobeTransferDialog.tsx`, opened `:1249` | absent | **MISSING** → `p4.9f` |
| Import-from-image modal | `components/wardrobe/import-from-image-modal.tsx`, opened `:1203` | absent | **MISSING** → `p4.9f` |
| Wardrobe item editor | `components/wardrobe/wardrobe-item-editor.tsx`, opened `:1214` | absent | **MISSING** → `p4.9f` |
| Project wardrobe manager | `components/wardrobe/ProjectWardrobeManager.tsx`, hosted `WardrobeCard.tsx:49` | `screens/prospero/wardrobe/project-wardrobe-manager.ts:48` | **PARITY** (P4.6o) |

The distinction matters and is easy to get wrong, so it is worth stating
plainly: v5's project wardrobe manager is **not a reduced version** of the
global dialog — it is a different surface. v4's own header
(`wardrobe-control-dialog.tsx:3-21`) describes the global dialog as
character-centric and app-global, with three capabilities the project
manager has none of: a **character picker** across all characters in or out
of chat; **chat-aware equipping** (a "wearing now" column, per-slot
equip/layer/clear against `?action=equip`) when opened with a `chatId`; and
**avatar generation/regeneration**, optionally with a non-default image
model. `ProjectWardrobeManager.tsx:4` calls itself the "CRUD body for a
project's `Wardrobe/` folder" — plain item CRUD. So `p4.9f` is a real
vertical, not a re-skin, and CLAUDE.md's *"the wardrobe dialog"* deferral
(`CLAUDE.md:300-302`) is understating it.

### 2.4 Character AI dialogs

| Dialog | v4 | v5 | Verdict |
| --- | --- | --- | --- |
| AIWizardModal | `components/characters/ai-wizard/AIWizardModal.tsx`; `NewCharacterView.tsx:507`, `CharacterEditView.tsx:415` | absent (`git grep "AIWizard\|ai-wizard"` → 0) | **MISSING** → `p4.9k` |
| CharacterOptimizerModal | `components/characters/optimizer/CharacterOptimizerModal.tsx`; `CharacterDetailView.tsx:373` | absent | **MISSING** → `p4.9k` |
| system-prompts-editor modals | `components/characters/system-prompts-editor/{PromptModal,PreviewModal,ImportModal}.tsx` | `screens/characters/edit/prompt-modal.ts:26` (partial) | **MISSING (partial)** → `p4.9k` |
| ExternalPrompt / ReverseUser dialogs | `app/aurora/[id]/view/components/{ExternalPromptDialog,ExternalPromptResultDialog,ReverseUserDialog}.tsx` | absent | **MISSING** → `p4.9k` |

These are the character family's tier-3 **LLM-service** refusals
(`status-log.md:6163-6164`) — they need live model calls, which is why they
were armed as loud refusals rather than stubbed. One correction to that
source list: it names `reset-builtins` alongside them, but reset-builtins
**shipped** in P4.4u4 (`screens/characters/list/reset-builtins-dialog.ts:7`;
`CLAUDE.md:313-317`). Do not carry it forward as open.

### 2.5 Settings dialogs

| Dialog | v4 | v5 | Verdict |
| --- | --- | --- | --- |
| ApiKeyModal | `components/settings/api-keys/ApiKeyModal.tsx` | `screens/settings/providers/api-key-modal.ts:24` | **PARITY** (P4.6e) |
| Connection ProfileModal | `components/settings/connection-profiles/ProfileModal.tsx` | `screens/settings/providers/profile-modal.ts:51` | **PARITY** (P4.6e) |
| ImageProfileModal | `components/image-profiles/ImageProfileModal.tsx` | `screens/settings/images/image-profile-modal.ts:28` | **PARITY** (P4.6r) |
| Template form / preview | `components/settings/…` | `screens/settings/templates/template-{form,preview}-modal.ts` | **PARITY** (P4.6r) |
| **Prompt library** | `components/settings/prompts/index.tsx:16` + `PromptList/PromptCard/PromptModal/PreviewModal` | absent — deferral at `templates/templates-tab.ts:9-14` | **MISSING** → `p4.9h` |
| **Core Whisper card** | `components/settings/core-whisper/CoreWhisperSection.tsx`, mounted `TemplatesPromptsTabContent.tsx:28` | absent (F3) | **MISSING** → `p4.9h` |
| Export/Import keys | `components/settings/api-keys/{ExportKeysDialog,ImportKeysDialog}.tsx` | absent | **MISSING** → `p4.9g` |
| Embedding ProfileModal | `components/settings/embedding-profiles/ProfileModal.tsx` | absent | **MISSING** → `p4.9h` |
| Plugin config / upgrade modals | `components/settings/plugins/{PluginConfigModal,UpgradeConfirmModal}.tsx` | absent | **WON'T-PORT** (D21) |
| ThemePreviewModal | `components/settings/appearance/components/ThemePreviewModal.tsx` | absent | **MISSING** → `p4.9c` |

The plugin modals are WON'T-PORT by a **locked decision**, not by
oversight: `phase-4.md:273-276` defers the plugin system beyond provider
manifests, so their UI has nothing to configure.

Note `components/settings/prompts-tab.tsx` is a 5-line re-export shim of
`components/settings/prompts/index.tsx`, and `components/images/ImageDetailModal.tsx:8`
is likewise a shim for `components/images/image-detail/ImageDetailModal.tsx`.
Counting shims as screens double-counts; `p4.9h`/`p4.9a` should port the
real directories.

### 2.6 Tools / data dialogs (all under Data & System)

| Dialog | v4 | v5 | Verdict |
| --- | --- | --- | --- |
| **LLMLogViewerModal** | `components/chat/LLMLogViewerModal.tsx:16`; hosts `llm-logs-card.tsx:133` + `LLMLogsSection.tsx:102` | absent (F2) | **MISSING** → `p4.9g` |
| Backup / Restore | `components/tools/backup-dialog.tsx`, `components/tools/restore/RestoreDialog.tsx` | absent | **MISSING** → `p4.9g` |
| Export / Import | `components/tools/{export,import}-dialog.tsx` | absent | **MISSING** → `p4.9g` |
| Capabilities report | `components/tools/capabilities-report-dialog.tsx` | absent | **MISSING** → `p4.9g` |
| SearchReplaceModal (tools) | `components/tools/search-replace/SearchReplaceModal.tsx` | absent | **MISSING** → `p4.9g` |
| Housekeeping dialog | `components/memory/housekeeping-dialog.tsx` | `memory/housekeeping-dialog.ts:21` | **PARITY** (P4.6t) |
| Memory-creation dialog | `components/import/memory-creation-dialog.tsx` | absent | **MISSING** → `p4.9h` |
| Search dialog | `components/search/search-dialog.tsx` | absent | **MISSING** → `p4.9g` |
| HelpChatDialog | `components/help-chat/HelpChatDialog.tsx`, mounted `app-layout.tsx:135` | absent | **MISSING** → `p4.9i` |
| BrahmaConsoleDialog | `components/brahma-console/BrahmaConsoleDialog.tsx`, mounted `app-layout.tsx:136` | absent | **MISSING** → `p4.9i` |

This table is why `p4.9g` (Data & System) is larger than "one placeholder
tab": the tab is the host for **eight** dialogs plus the LLM-logs card.
`HelpChatDialog` and `BrahmaConsoleDialog` are grouped into `p4.9i` instead
because both are app-layout-mounted console surfaces with the same
`chatType` wire enum backing them (`core-contract.ts:1514`).

---

## 3. The deferral cross-reference

Every "deferred loud" item in the four sources named by the order (order
status headers, `status-log.md` round records, `dogfood-findings.md`,
CLAUDE.md's standing pool) maps to exactly one row above, or to the
non-screen bucket in §5.3. The authoritative live list is
`status-log.md:15508-15529` (the most recent round's *"Still OPEN … the
next-order pool"* block).

**Screen deferrals → their row:**

| Deferral (source) | Row |
| --- | --- |
| `/generate-image` screen (`status-log.md:15518-15520`) | §1.6 → `p4.9b` |
| NewChatModal-on-card (`status-log.md:15520-15521`) | §1.1 (DIVERGENCE) |
| Quick-hide filtering (`status-log.md:15285-15288`, `:15521-15522`) | §1.1, §1.2 → `p4.9d` |
| Tabbed workspace, "unowed" (`status-log.md:15523`) | §1.6 → `p4.9j` (F1) |
| Source-mode toggle (`status-log.md:15526`) | LANDED (P4.6ax, this round) |
| GFM table transformer (`status-log.md:15526-15527`, `:11380-11383`) | LANDED (P4.6ax, this round; table STYLING in the rich editor deferred → `p4.9` rider) |
| `__bold__` on-type (`status-log.md:14683-14684`) | LANDED (P4.6ax, this round) |
| `roleplayTemplateId` toolbar awareness (`status-log.md:12226-12227`; re-scoped `phase-4.md:1615-1618`) | §4 `p4.9l` (Salon composer slice — explicitly NOT a rider) |
| Project-page backgrounds / backdrop arbitration (`status-log.md:12477-12481`) | §1.4 (DIVERGENCE; resolves with `p4.9j`) |
| No-host chat dialogs (`status-log.md:14681-14683`; widened `p4.8:139-143`) | §2.2 → `p4.9e1/e2/e3` |
| Gutter tools + drag-and-drop upload (`status-log.md:10465`) | §2.2 → `p4.9e2` |
| StandaloneGenerateImageDialog + ImageProfilePicker (`status-log.md:10508-10513`) | §2.2 → `p4.9b` |
| Deep gallery detail modals (`status-log.md:10437-10441`) | §2.1 → `p4.9a` |
| Generate-dialog params beyond the locked four (`p4.6ai:15-16`) | §2.1 (DIVERGENCE) |
| Boxed `ChatCostSummary` + `detailed=true` (`status-log.md:14678-14679`) | §1.2 **WON'T-PORT** — verdict rendered |
| `doc_focus` scroll-to-anchor, maximize/focus beat, qtap:// link opening, standalone/workspace-tab surface (`status-log.md:9096-9099`) | §1.4 / §1.6 → `p4.9j` |
| xterm optional addons; exit/kill toasts (`status-log.md:8613-8621`) | §4 `p4.9m` (no toast bus) |
| `chat-update` WS side effects beyond refetch (`status-log.md:8614-8617`) | §4 `p4.9m` |
| Wardrobe dialog (`CLAUDE.md:300-302`) | §2.3 → `p4.9f` |
| Character tier-3 LLM services (`status-log.md:6163-6164`) | §2.4 → `p4.9k` |
| Depiction-guidelines `disabledHint` (`status-log.md:14691-14693`) | LANDED (P4.6aw, this round) |
| Files: rich text/pdf preview, rich FolderPicker, drag relocation, workspace-tab drill (`p4.6af:16-18`) | §4 `p4.9n` |
| Files: cross-mount move/copy UI, DnD relocation, FilePreview family (`p4.6aa:21-27`) | §4 `p4.9n` |
| Data & System tab (`p4.8:135-138`) | §1.5 → `p4.9g` |
| Prompt library + Core Whisper (`p4.8:133-134`, `status-log.md:13879`) | §1.5, §2.5 → `p4.9h` |
| Structured per-provider parameters editor (`status-log.md:8031-8033`) | §1.5 (DIVERGENCE) |
| Formatting-prompt helper; tag pickers (`status-log.md:8033-8035`) | §1.5 → `p4.9h` |
| Image-profile validate/list-models (`p4.6ai:12-14`) | §1.5 → `p4.9g` |
| Memory: embedding profiles, dedup card, summaries regen (`status-log.md:8392-8395`) | §1.5 → `p4.9h` |
| Memory editor / anchoring / tag editing (`status-log.md:8687-8690`) | §4 `p4.9h` |
| PhotosView, ProfileView, AboutView, Brahma console (`p4.8:116-124`) | §1.6 → `p4.9a`/`p4.9c`/`p4.9i` |
| Redirect aliases + `/foundry/*` (`p4.8:150-151`) | §1.8 **WON'T-PORT** — verdict rendered |
| Stale `settings.ts:20` comment (`p4.8:136-138`) | §0 F4 (record only) |

**Two corrections to the sources**, both stated above and repeated here so
a future reader does not re-open closed work: `reset-builtins` appears in
the characters tier-3 refusal list (`status-log.md:6163-6164`) but is
**CLOSED-BY-P4.4u4**; and the seed's single LLM-log row is two surfaces
(F2), one PARITY and one MISSING.

**Dogfood findings carry nothing into this checklist.**
`dogfood-findings.md` has zero open findings: #1/#2/#3a/#3b/#4/#5/#6/#7/#8/#9/#13
are FIXED, #10/#11 are NOT-A-BUG (recorded v4-faithful so they are not
re-reported), and #12's fix has landed and awaits only human visual
confirmation (§5.2). Its standing notes do name an **un-walked** surface
list — Text Replacements + a rule firing in the composer, composition mode,
draft persistence, delete-with-associations, composer file-attach +
duplicate-conflict, and `imageProfileGenerate` (real provider spend — ask
first). That is a dogfood gap, not a parity gap, and it is §5.2's business.

---

## 4. The prioritized backlog

Ordered by user-visible value. Sizes: **rider** (a few commits inside
another lane), **lane** (one work order), **round** (several parallel
lanes). These are liftable straight into `/setupphase`.

| # | Slug | What | Size | Depends on |
| --- | --- | --- | --- | --- |
| 1 | `p4.9a-photos-view` | `/photos` route + PhotosView; the deep gallery modals (`image-detail/`, ChatGalleryImageViewModal, tag edit, prev/next); flips `shell.ts:44-50` off `route: null` | lane | photo-album verbs (P4.6ab, landed) |
| 2 | `p4.9c-about-profile` | `/about` + `/profile` + ThemePreviewModal; render the version **locally** (divergence from v4's shields.io fetch) | rider | none |
| 3 | `p4.9b-generate-image-screen` | `/generate-image` route + StandaloneGenerateImageDialog + ImageProfilePicker; un-omits the homepage quick action | lane | `image_generation` seam (P4.6ai, LIVE) |
| 4 | `p4.9d-quick-hide-provider` | the provider + tag-hide + hide-dangerous across salon list, home, characters, prospero; the `tags-tab` `quickHide` authoring column | lane | tags surface (landed) |
| 5 | `p4.9g-data-system-tab` | the Data & System tab + its 8 dialogs (LLMLogViewer ×2 hosts, backup, restore, export, import, capabilities, search) + image-profile validate/list-models | round | llm-logs reads (P4.6ar, landed); live providers for validate |
| 6 | `p4.9h-prompt-library-core-whisper` | the prompt library; the Core Whisper card **and** the chat-sidebar override (F3 — port the chain as one); memory embedding-profiles / dedup / summaries; tag pickers; formatting-prompt helper | round | none |
| 7 | `p4.9f-wardrobe-dialog` | the global wardrobe dialog (character picker + chat-aware equip + avatar generation) + transfer + import-from-image + item editor | lane | `image_generation` (LIVE); equip verbs |
| 8 | `p4.9e1-chat-cast-dialogs` | AddCharacterDialog + nested CreateNPC + SummonFromLore | lane | tier-3 LLM services for Summon |
| 9 | `p4.9e2-chat-post-office-dialogs` | ComposeMail + InsertAnnouncement + Whisper **+ the gutter-tool entry points + DnD upload** | lane | post-office writers (landed) |
| 10 | `p4.9e3-chat-admin-dialogs` | the `ChatModals.tsx` barrel remainder + `useModalState` (Merge, Reattribute, BulkReplace, RunTool, ChatToolSettings, ChatProject, chat-host StateEditor, SearchReplace, AllLLMPause, SelectLLMProfile, LibraryFilePicker, ChatRename) | round | `?action=update-tool-settings` (`core-contract.ts:858`) |
| 11 | `p4.9i-brahma-help-consoles` | BrahmaConsoleDialog (+ its `asTab` re-skin) + HelpChatDialog | lane | W4.5b services (landed) |
| 12 | `p4.9k-character-ai-dialogs` | AIWizard, Optimizer, system-prompts import/preview, ExternalPrompt/ReverseUser | round | tier-3 LLM services |
| 13 | `p4.9n-files-fidelity` | rich text/pdf preview, rich FolderPicker, cross-mount move/copy UI, drag relocation | lane | pdf/docx extractor (refusing seam) |
| 14 | `p4.9l-salon-composer-toolbar` | `roleplayTemplateId`-aware toolbar delimiters — a composer vertical, **not** a rider (`phase-4.md:1615-1618`) | lane | a composer toolbar must exist first |
| 15 | `p4.9m-toast-bus` | a toast bus; terminal exit/kill toasts; `chat-update` side effects; xterm optional addons | rider | none |
| 16 | `p4.9j-workspace-tabs` | the tabbed workspace: host, tab strip, 21 tab kinds, split panes, keep-alive, drag reorder, `?open=` intents, backdrop arbitration | **round (largest)** | **a human ruling first** — see §5.1 |

Sequencing note: items 1–4 are small, high-visibility, and unblocked —
they are the natural next round, and 1/2/3 are close enough in shape
(routes + image surfaces) to run as parallel lanes. Item 16 should not be
scoped until §5.1 is answered; it is plausibly larger than every other row
combined.

---

## 5. The v4 retirement criteria

The milestone line, verbatim (`phase-4.md:459`):

> `| M6 | Screen-parity checklist complete; v4 retirement review (same DB files, so migration = open them) |`

### 5.1 What must be true — the screen side

**Migration is a non-issue, with one asterisk.** v5 opens the exact DB
files v4 writes — same tables, same UUIDs, same ChaCha20/sqleet cipher —
so retirement requires no data migration. The asterisk is that the
*reverse* is not symmetric: v4's `quantize-embeddings-v1` migration is
**one-way** (`CLAUDE.md:594-597`), so once v4 `4.8.0-dev.52`+ has run
against an instance, going back needs a backup. That is a caveat on
**un-retiring**, not on retiring, and it argues for keeping a pre-cutover
backup rather than for delaying.

**Retirement therefore gates on screens, and screens gate on one human
decision plus a floor.**

1. **The workspace ruling (F1) — a human call, not a checklist row.** v4's
   default shell is the tabbed workspace. v5 implements v4's opt-out route
   model faithfully, so nothing is *lost* functionally, but anyone who
   lives in tabs experiences retirement as a workflow regression. Three
   honest options: (a) accept the route model as v5's UX and retire without
   `p4.9j`; (b) run `p4.9j` first and retire at true parity; (c) retire for
   users who don't use tabs and keep v4 available for those who do. **This
   document does not choose.** It records that the choice is the single
   largest determinant of how far retirement is, and that (a) is defensible
   precisely because v4 ships the same mode under a flag.
2. **The MISSING floor.** Regardless of the ruling, these are hard to call
   "parity" while absent, in rough descending order of visibility:
   `/photos` (nav item is a **disabled dead button** on every screen —
   `shell.ts:44-50`), the Data & System tab (a rendered placeholder —
   `settings.ts:66-68`), `/about` + `/profile`, `/generate-image`,
   quick-hide, the global wardrobe dialog, and the in-chat dialog family.
   That is backlog items 1–10.
3. **Accepted divergences stay accepted.** The DIVERGENCE-DOCUMENTED rows
   above are deliberate and recorded; none needs closing before
   retirement. They need only to stay documented — which is what this file
   is now for.
4. **Zero silent gaps.** Every gap renders as nothing or as a loud refusal,
   never as a dead card (`status-log.md:8392-8395` is the precedent).
   `/photos`'s disabled nav button is the one live exception and should
   either route or disappear.

### 5.2 The acceptance walks

| Walk | Status | Source |
| --- | --- | --- |
| **The human M5 + finding-#12 walk** (staged instance; Tauri images on real data) | **OUTSTANDING** — the one open acceptance step | `status-log.md:15510-15515`; `CLAUDE.md:475-478` |
| A dogfood pass over the un-walked surfaces: Text Replacements + a rule firing in the composer, composition mode, draft persistence, delete-with-associations, composer file-attach + duplicate-conflict, `imageProfileGenerate` (**real provider spend — ask first**) | not yet run | `dogfood-findings.md`, Standing notes |
| A full-instance dogfood pass on a **copy** of real Friday data | ongoing practice | `dogfood-findings.md`; `CLAUDE.md` dogfood setup |
| The automated floor: 325 Rust suites / 1357 tests, `ng test` 1172, Playwright 65/65 zero skips | green at `41e1a0a` | `status-log.md` round record |

### 5.3 The non-screen pools that gate retirement independently

These do **not** appear as checklist rows — they are not screens — but
each can block a real retirement on its own. Screen parity is necessary,
not sufficient.

| Pool | Status | Source |
| --- | --- | --- |
| **D21 — release / signing / notarization / updater / bundles** | deferred; the repo also carries a *"don't initiate a release"* hard stop | `phase-4.md:272-277`; `CLAUDE.md` hard stops |
| **uniffi / mobile** | deferred until Tauri-mobile is proven or disproven | `phase-4.md:272-273`, `:37`, `:268` |
| **Native niceties** (menus beyond defaults, tray, dock badge, window-state persistence, deep links) | deferred (D14 progressive enhancement) | `status-log.md:14862-14866` |
| **Windows/Linux one-origin re-checks** | macOS-verified only; the `http://qtap.localhost` Windows window-URL shape is **not** wired in `tauri.conf.json` | `status-log.md:15458-15464`, `:15516-15517` |
| **Turnkey `tauri dev` loop** | documented, not wired | `status-log.md:14855-14857`, `:15140-15141` |
| **Plugin system beyond provider manifests** | **WON'T-PORT** by locked decision — v4's npm plugin tools/routes; the `ToolRunner` inner-fallback seam stays loud | `phase-4.md:273-276` |
| **`Last-Event-ID` replay** beyond the §2 resync signal | deferred | `status-log.md:14866-14868` |
| **Refusing service seams**: `filesSync`, attach-mount-file, thumbnail codec, cleanup-stale disk keys, auto-describe, pdf/docx extractor, WebP codec, `conversion.ts`, fs watcher + store-event chain, `quilltap docs` CLI, `memoryGenerateEmbeddings`/`RebuildIndex`/`chatQueueMemories`, extract-memories-dry-run, `EMBEDDING_GENERATE` | all loud, named, armed | `status-log.md:11542-11562`, `:9517-9524`, `:8683-8688`; `p4.6ah:11-16`; `p4.d3:19` |
| **D22 — no new features during the port** | standing; v5-only banked capabilities stay dormant | `phase-4.md:278-281` |

The refusing-seam row is the one most likely to surprise: those are
**behaviors a user can reach**, armed to refuse loudly rather than to lie.
A retirement that ships them still-refusing is a product decision (v4 could
do these things), even though none is a screen.

### 5.4 The short answer

If the workspace ruling lands on option (a): retirement needs backlog items
1–10 plus the outstanding M5 walk, the Windows/Linux re-checks, and a D21
release story. If it lands on (b): add item 16, which is plausibly larger
than 1–15 combined. Nothing in the data layer stands in the way in either
case.

---

## 6. Findings recorded, not fixed (Tier 3)

Per the order, this lane records and proposes; it does not fix.

**In v5 (comment-only, no behavior):**

1. `screens/settings/settings.ts:16-20` — "five tabs are placeholders" is
   stale; one is. (Named by the order.)
2. `screens/settings/placeholder-tab.ts:6-8` — lists five placeholder tabs;
   four are now real.
3. `shell/shell.ts:19-20` — "Only the Salon (Chats) route is live"; six of
   seven are.

A single comment-only rider closes all three; they are natural riders on
`p4.9a` (which edits `shell.ts` anyway) and `p4.9g` (which edits
`settings.ts` anyway).

**In v4 (no action — recorded so a future porter is not misled):**

4. `lib/navigation/workspace-redirect.ts:6-7` and
   `app/workspace/page.tsx:11-13` both claim the workspace flag is off by
   default. It is on (F1). Any future porter reading only the docstrings
   will reach the wrong conclusion about v4's default UX.
5. `app/dashboard/layout.tsx:15` redirects to `/auth/signin`, which does
   not exist. Unreachable in single-user mode; a dangling reference.
6. `/salon` is the only list route that does not redirect into the
   workspace, and has no `salon-list` tab kind (`TabView.tsx:71-119`).
7. `SettingsView.tsx:78,81` — the `&section=` deep-link is plumbed from
   `app/settings/page.tsx:19` through `WorkspaceIntent.tsx:73` and then
   discarded as `_section`. It is inert in v4. **v5 honours `?section=`**
   (`templates/templates-tab.ts:9-14`), so v5 is *ahead* here — worth
   knowing before someone "fixes" v5 toward v4.

**Tooling gotcha worth a memory note:** a recursive `grep -r --include`
silently returned zero matches on a file that verifiably contained the
pattern during this lane. Every absence proof here was re-derived with
`git grep` against a control query that hits. An absence proof from an
unvalidated tool is worthless — validate the tool, then prove the absence.
